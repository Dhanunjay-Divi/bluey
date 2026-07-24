//! Rolling meeting summary — daemon orchestration helpers.
//!
//! Every N transcript segments the daemon fires a **stateless, throwaway
//! one-shot drive of the user's attached agent** that UPDATES the meeting's
//! rolling summary: input = (current summary + the newest transcript window),
//! output = the refreshed summary. The accumulation lives in OUR store
//! (`MeetingRecord.summary`), never in an agent session — so it survives agent
//! restarts/compaction and each pass only pays for the delta
//! (PLAN-CONTEXT-WARMUP Appendix C, decision: one-shot, accumulate in our store).
//!
//! Design constraints:
//! - Stateless one-shot (`resume: None`): verified (claude, 2026-07) to persist
//!   NO session — a pure side-computation that never pollutes the user's
//!   session list or answer session.
//! - The summary is REPLACED each pass (regenerated, bounded); the decisions
//!   ledger is the append/supersede record. Two structures, two update rules.
//! - Fire-and-forget in the background; never blocks the transcript path.

use std::env;

/// Default number of transcript segments between summary passes. Deliberately
/// coarser than the ledger's 15 — the summary is heavier work for the agent
/// and the narrative changes slower than decisions land.
pub const DEFAULT_INTERVAL_SEGMENTS: usize = 24;

/// Max characters of NEW transcript window handed to the summarizer per pass.
pub const WINDOW_MAX_CHARS: usize = 3_500;

/// Bound on the stored rolling summary. Keeps the per-turn delta the answer
/// path re-sends (and the summarizer's own input) capped regardless of meeting
/// length.
pub const SUMMARY_MAX_CHARS: usize = 1_800;

/// How many segments between passes (env `BLUEY_SUMMARY_INTERVAL_SEGMENTS`,
/// min 8). Retained for window sizing; the FIRE cadence is word-based below.
pub fn interval_segments() -> usize {
    env::var("BLUEY_SUMMARY_INTERVAL_SEGMENTS")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .map(|n| n.max(8))
        .unwrap_or(DEFAULT_INTERVAL_SEGMENTS)
}

/// Words of new transcript between summary passes. WORD-based (not segment-
/// based) so cadence is insensitive to STT fragmentation.
///
/// LIVE-FIRST (2026): the rolling summary is a LIVE overlay artifact re-shown on
/// every ask DURING the meeting — not a post-meeting batch summarizer. Batch
/// summarizers fire at ~20-32k tokens because latency is irrelevant when the
/// summary is only read once, at the end; a live copilot is the opposite. If the
/// summary only refreshes every ~20 min, the first 20 min of a meeting has NO
/// summary at all, and the card is stale for anyone who asks mid-meeting. The
/// narrative changes slower than decisions, so the summary can be coarser than
/// the ledger (~350 words) — but it must exist EARLY and stay reasonably fresh.
/// At ~130 wpm, ~800 words ≈ ~6 min of speech: a summary appears within the
/// first few minutes and never lags more than ~6. Cost is bounded by SELECTIVITY,
/// not cadence: `build_prompt` lets the model return the summary UNCHANGED when a
/// window added nothing meaningful (see `build_prompt`), so a chitchat stretch is
/// a cheap near-no-op. Override with `BLUEY_SUMMARY_INTERVAL_WORDS`.
// TESTING (temporary): lowered 800 → 200 so a rolling summary appears within a
// short test meeting. Revert to 800 for production. Override with
// BLUEY_SUMMARY_INTERVAL_WORDS.
pub const DEFAULT_INTERVAL_WORDS: usize = 200;

/// Words between summary passes (env `BLUEY_SUMMARY_INTERVAL_WORDS`, min 150).
pub fn interval_words() -> usize {
    env::var("BLUEY_SUMMARY_INTERVAL_WORDS")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .map(|n| n.max(150))
        .unwrap_or(DEFAULT_INTERVAL_WORDS)
}

/// Should a summary pass fire, given total transcript words and the words at the
/// last fire? Fires once per `interval_words()` boundary crossed.
pub fn should_fire_words(total_words: usize, last_fired_words: usize) -> bool {
    let n = interval_words();
    total_words >= last_fired_words + n
}

/// Build the one-shot summarization prompt: update the running summary with
/// the newest window. Plain text out (no fences, no headers) so the result can
/// be stored and re-fed verbatim.
pub fn build_prompt(current_summary: Option<&str>, window: &str) -> String {
    let mut prompt = String::with_capacity(window.len() + 1024);
    prompt.push_str(
        "You maintain the running summary of a live technical meeting. \
         Update the summary below with the new transcript lines. Keep every \
         still-relevant fact from the current summary (topics, decisions, \
         owners, blockers, numbers, ticket/PR ids); fold in what the new lines \
         add; drop filler. If the new lines add NOTHING that belongs in a \
         meeting summary (small talk, tangents, repetition), return the CURRENT \
         SUMMARY exactly as-is, unchanged. Output ONLY the summary as at most 12 \
         short plain-text bullet lines starting with \"- \". No markdown fences, \
         no preamble, no commentary.\n\n",
    );
    match current_summary.map(str::trim).filter(|s| !s.is_empty()) {
        Some(summary) => {
            prompt.push_str("CURRENT SUMMARY:\n");
            prompt.push_str(summary);
            prompt.push('\n');
        }
        None => prompt.push_str("CURRENT SUMMARY:\n(none yet — this is the first pass)\n"),
    }
    prompt.push_str("\nNEW TRANSCRIPT LINES:\n");
    prompt.push_str(window.trim());
    prompt
}

/// Bound + clean a model-produced summary for storage: strip stray code fences
/// and hard-cap the length (truncating on a line boundary where possible).
pub fn bound_summary(raw: &str) -> String {
    let cleaned = raw
        .lines()
        .filter(|line| !line.trim_start().starts_with("```"))
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string();
    if cleaned.chars().count() <= SUMMARY_MAX_CHARS {
        return cleaned;
    }
    // Truncate to the cap, then back off to the last complete line.
    let capped: String = cleaned.chars().take(SUMMARY_MAX_CHARS).collect();
    match capped.rfind('\n') {
        Some(pos) if pos > 0 => capped[..pos].trim_end().to_string(),
        _ => capped,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_fire_words_respects_interval() {
        // Value-agnostic: assert the behavior against the live interval so
        // tuning the default never breaks this test.
        let n = interval_words();
        assert!(!should_fire_words(0, 0));
        assert!(!should_fire_words(n - 1, 0), "one word short must not fire");
        assert!(should_fire_words(n, 0), "the first boundary fires");
        // Next boundary after a fire at `n` is `2n`.
        assert!(!should_fire_words(2 * n - 1, n));
        assert!(should_fire_words(2 * n, n));
    }

    #[test]
    fn prompt_carries_current_summary_and_window() {
        let p = build_prompt(Some("- decided to shard by tenant"), "S1: what about auth?");
        assert!(p.contains("CURRENT SUMMARY:"));
        assert!(p.contains("- decided to shard by tenant"));
        assert!(p.contains("NEW TRANSCRIPT LINES:"));
        assert!(p.contains("what about auth?"));

        let first = build_prompt(None, "S1: hello");
        assert!(first.contains("first pass"));
    }

    #[test]
    fn bound_summary_strips_fences_and_caps_on_line_boundary() {
        let fenced = "```\n- point one\n```";
        assert_eq!(bound_summary(fenced), "- point one");

        let long_line = "- ".to_string() + &"x".repeat(SUMMARY_MAX_CHARS);
        let many = format!("- keep\n{long_line}");
        let bounded = bound_summary(&many);
        assert!(bounded.chars().count() <= SUMMARY_MAX_CHARS);
        assert!(bounded.starts_with("- keep"));
    }
}
