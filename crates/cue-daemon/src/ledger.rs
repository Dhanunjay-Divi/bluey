//! Decisions ledger — daemon orchestration.
//!
//! Every N transcript turns the daemon fires a **stateless, cheap-lane** LLM call
//! that extracts decisions / constraints / owners from the recent transcript
//! window. The raw output is run through [`cue_core::parse_and_verify`] (the
//! anti-hallucination gate) and merged into the meeting's [`LedgerState`], which
//! the answer path renders as a pinned context block.
//!
//! Design constraints (see docs/LEDGER-PLAN.md):
//! - Reuses the existing LLM backend — no bundled model, no new dependency.
//! - Stateless: never reads or appends to the user's answer session.
//! - Cheap lane: forces the Instant-tier model, following whatever mode is
//!   configured (managed → openai → local). Off by default (`BLUEY_LEDGER=1`).

use std::env;

use cue_core::{parse_and_verify, LedgerState, ProviderSelector, EXTRACTION_PROMPT};

/// Default number of transcript turns between ledger passes.
pub const DEFAULT_INTERVAL_TURNS: usize = 15;

/// Max characters of transcript window handed to the extractor. Keeps the input
/// (and therefore cost) bounded regardless of how chatty a stretch is.
pub const WINDOW_MAX_CHARS: usize = 4_000;

/// Max output tokens for an extraction pass — the JSON is small.
pub const MAX_OUTPUT_TOKENS: u32 = 512;

/// Whether the ledger feature is enabled. Off by default until validated live;
/// the existing heuristic Context card remains the zero-cost floor.
pub fn enabled() -> bool {
    matches!(
        env::var("BLUEY_LEDGER").ok().as_deref(),
        Some("1") | Some("true") | Some("TRUE")
    )
}

/// How many turns between passes (env `BLUEY_LEDGER_INTERVAL_TURNS`, min 5).
/// Retained for the transcript-window sizing (`last_transcript_text_bounded`),
/// which is turn-based; the FIRING cadence is word-based (see `should_fire`).
pub fn interval_turns() -> usize {
    env::var("BLUEY_LEDGER_INTERVAL_TURNS")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .map(|n| n.max(5))
        .unwrap_or(DEFAULT_INTERVAL_TURNS)
}

/// Words of new transcript between extraction passes. WORD-based (not
/// segment-based) so the cadence is insensitive to how the STT chunks speech:
/// the direct-emit path produces ~2-word fragments, so a segment-count trigger
/// fired every ~13s (≈130 calls in a 30-min meeting — wasteful). ~350 words is
/// roughly 2-3 minutes of speech at conversational pace, a predictable cadence
/// regardless of fragmentation. Override with `BLUEY_LEDGER_INTERVAL_WORDS`.
pub const DEFAULT_INTERVAL_WORDS: usize = 350;

/// Words between ledger passes (env `BLUEY_LEDGER_INTERVAL_WORDS`, min 60).
pub fn interval_words() -> usize {
    env::var("BLUEY_LEDGER_INTERVAL_WORDS")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .map(|n| n.max(60))
        .unwrap_or(DEFAULT_INTERVAL_WORDS)
}

/// Should a ledger pass fire, given the total transcript word count and the word
/// count at the LAST fire? Fires once per `interval_words()` boundary crossed, so
/// cost scales with how much was actually SAID, not with fragment count. The
/// caller tracks `last_fired_words` and updates it when a pass fires.
pub fn should_fire_words(total_words: usize, last_fired_words: usize) -> bool {
    let n = interval_words();
    total_words >= last_fired_words + n
}

/// The cheap-lane provider selectors to try, cheapest-first. The daemon picks the
/// first one that is actually configured/usable at call time; if none is usable
/// the pass is skipped (no error, no cost). Mirrors the answer path's mode order
/// (managed → openai → local) but always on the Instant/cheap model.
pub fn cheap_provider_candidates() -> Vec<ProviderSelector> {
    // Allow an explicit override, e.g. BLUEY_LEDGER_MODEL=gpt-4o-mini.
    if let Ok(model) = env::var("BLUEY_LEDGER_MODEL") {
        let model = model.trim();
        if !model.is_empty() {
            // Heuristic: managed router models route through CueManaged, else OpenAI-compatible.
            let sel = if model.contains("managed") || model.contains("router") {
                ProviderSelector::cue_managed(model.to_string())
            } else {
                ProviderSelector::openai(model.to_string())
            };
            return vec![sel];
        }
    }
    vec![
        ProviderSelector::cue_managed("bluey-managed-instant"),
        ProviderSelector::openai("gpt-4o-mini"),
        ProviderSelector::local("bluey-local-answer-v0"),
    ]
}

/// Build the user-side extraction input: the labelled transcript window.
pub fn build_window(transcript: &str) -> String {
    transcript.trim().to_string()
}

/// Run one verification pass over `raw` model output against `window` and merge
/// the surviving items into `state`. Returns the number of newly added items.
/// Pure/synchronous — the network call happens in the daemon; this is the seam
/// that's identical whether the model is local or hosted, and is unit-testable.
pub fn ingest(state: &mut LedgerState, raw: &str, window: &str) -> usize {
    let verified = parse_and_verify(raw, window);
    state.merge(verified)
}

/// The system prompt for the extraction call.
pub fn system_prompt() -> &'static str {
    EXTRACTION_PROMPT
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_fire_words_respects_interval() {
        // default 350 words between passes.
        assert!(!should_fire_words(0, 0));
        assert!(!should_fire_words(349, 0));
        assert!(should_fire_words(350, 0)); // first boundary crossed
        assert!(should_fire_words(700, 0)); // well past → still fires
                                            // After a fire at 350, the next fire is at 350 + 350 = 700.
        assert!(!should_fire_words(699, 350));
        assert!(should_fire_words(700, 350));
    }

    #[test]
    fn ingest_only_merges_verified_items() {
        let window = "Speaker 1: Let's do phased rollout for Q3.";
        let mut state = LedgerState::default();

        // One real, one fabricated — only the real survives.
        let raw = r#"{
          "decisions": [
            {"quote":"Let's do phased rollout for Q3","text":"Phased rollout Q3","speaker":"Speaker 1"},
            {"quote":"We will double the budget","text":"Double budget","speaker":"Speaker 1"}
          ],
          "constraints": [], "owners": []
        }"#;
        let added = ingest(&mut state, raw, window);
        assert_eq!(added, 1, "fabricated item must not be merged");
        assert_eq!(state.len(), 1);

        // Re-running the same pass adds nothing (dedup).
        assert_eq!(ingest(&mut state, raw, window), 0);
    }

    #[test]
    fn cheap_candidates_default_order() {
        // Without an override, cheapest managed/openai/local order.
        std::env::remove_var("BLUEY_LEDGER_MODEL");
        let c = cheap_provider_candidates();
        assert_eq!(c.len(), 3);
    }
}
