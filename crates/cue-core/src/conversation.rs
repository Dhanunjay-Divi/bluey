//! Conversation memory — pure assembly logic (Wave 1).
//!
//! App-owned Q&A memory for in-meeting copilot exchanges: the daemon stores
//! every user↔copilot turn itself, and this module assembles a token-bounded
//! conversation block (running summary + verbatim tail of the newest turns)
//! that the answer path injects into the envelope. Owning the memory in OUR
//! store makes the agent-side session disposable — "follow up on that" keeps
//! working across agent restarts, compaction, and (Wave 3) ephemeral drives.
//!
//! Design constraints:
//! - Pure functions only — no I/O, no async, fully unit-testable. The daemon
//!   owns persistence, cadence, and the fold (summarization) call.
//! - Zero new dependencies. Token counting is the chars/4 heuristic (see
//!   [`estimate_tokens`]) — budgeting needs consistency, not precision.
//! - Every tunable lives in [`ConvConfig`] with env overrides, following the
//!   existing `interval_words()` pattern (env var, parse, min-clamp, default).

use std::env;

use serde::{Deserialize, Serialize};

/// Assumed characters per token for the budgeting heuristic. Shared by
/// [`estimate_tokens`] and its inversion (token caps expressed as character
/// guides in [`build_fold_prompt`] / [`bound_summary`]) so the two directions
/// can never drift apart.
const CHARS_PER_TOKEN: usize = 4;

/// Default token budget for the verbatim recent-turns tail.
///
/// PRODUCTION-GROUNDED (2026): a ~20-turn conversation is ~5-10k tokens
/// (production memory guides), and 200k context windows are now standard — so a
/// 10k verbatim tail holds a whole meeting's worth of Q&A (~20-25 turns) while
/// still using only ~5% of a 200k window, leaving the rest for the agent to read
/// code. The prior 2k default folded to summary after ~5 exchanges, discarding
/// verbatim history the modern window can easily hold. The `window_frac` cap
/// below still scales this down for smaller-window models.
pub const DEFAULT_TAIL_TOKENS: usize = 10_000;
const MIN_TAIL_TOKENS: usize = 200;

/// Default cap on the running conversation summary (tokens). A compact rolling
/// summary of turns that have aged out of the verbatim tail — production hybrid
/// memory keeps this small (recent verbatim + compressed older).
pub const DEFAULT_SUMMARY_TOKENS: usize = 600;
const MIN_SUMMARY_TOKENS: usize = 100;

/// Default per-meeting stored-turn cap (FIFO prune, enforced by the daemon).
pub const DEFAULT_MAX_STORED_TURNS: usize = 200;
const MIN_MAX_STORED_TURNS: usize = 20;

/// Default fraction of the model context window the tail may occupy.
///
/// PRODUCTION-GROUNDED: production token-budget guidance summarizes only when the
/// buffer nears 70-80% of the window, so a single slice (the conversation tail)
/// taking ~5% is comfortably conservative and lets a large-window model keep more
/// verbatim history (0.05 × 200k = 10k, matching the tail default; 0.05 × 100k =
/// 5k for a smaller model). The prior 2% under-used modern windows.
pub const DEFAULT_WINDOW_FRAC: f64 = 0.05;
const MIN_WINDOW_FRAC: f64 = 0.005;

/// Conservative fallback context window (tokens) for unknown models. Modern
/// production models are routinely 200k+, but an unknown model gets the safe
/// lower bound so the tail budget never over-reaches on a smaller one.
pub const DEFAULT_CONTEXT_WINDOW: u32 = 128_000;

/// All conversation-memory knobs in one place. Every field is env-tunable via
/// [`ConvConfig::from_env`]:
///
/// | field | env var | default | min |
/// |---|---|---|---|
/// | `tail_tokens` | `BLUEY_CONV_TAIL_TOKENS` | 2000 | 200 |
/// | `summary_tokens` | `BLUEY_CONV_SUMMARY_TOKENS` | 500 | 100 |
/// | `max_stored_turns` | `BLUEY_CONV_MAX_TURNS` | 200 | 20 |
/// | `window_frac` | `BLUEY_CONV_WINDOW_FRAC` | 0.02 | 0.005 |
#[derive(Debug, Clone, PartialEq)]
pub struct ConvConfig {
    /// Token budget for the verbatim recent-turns tail.
    pub tail_tokens: usize,
    /// Cap on the running conversation summary (tokens).
    pub summary_tokens: usize,
    /// Per-meeting stored-turn cap (FIFO prune, enforced by the daemon).
    pub max_stored_turns: usize,
    /// Fraction of the model context window the tail may use. The effective
    /// tail budget is `min(tail_tokens, window_frac × model_window)`.
    pub window_frac: f64,
    // Reserved (Wave 2, session-history retrieval — documented, NOT implemented):
    //   BLUEY_RETRIEVAL_TOP_K   (default 5)    — retrieval hits per query.
    //   BLUEY_RETRIEVAL_TOKENS  (default 1500) — retrieval context token budget.
}

impl Default for ConvConfig {
    fn default() -> Self {
        Self {
            tail_tokens: DEFAULT_TAIL_TOKENS,
            summary_tokens: DEFAULT_SUMMARY_TOKENS,
            max_stored_turns: DEFAULT_MAX_STORED_TURNS,
            window_frac: DEFAULT_WINDOW_FRAC,
        }
    }
}

impl ConvConfig {
    /// Read the config from the environment: each knob is parsed from its env
    /// var, clamped to its minimum, and falls back to the default when unset
    /// or unparseable (the `interval_words()` pattern).
    pub fn from_env() -> Self {
        Self {
            tail_tokens: env_usize(
                "BLUEY_CONV_TAIL_TOKENS",
                DEFAULT_TAIL_TOKENS,
                MIN_TAIL_TOKENS,
            ),
            summary_tokens: env_usize(
                "BLUEY_CONV_SUMMARY_TOKENS",
                DEFAULT_SUMMARY_TOKENS,
                MIN_SUMMARY_TOKENS,
            ),
            max_stored_turns: env_usize(
                "BLUEY_CONV_MAX_TURNS",
                DEFAULT_MAX_STORED_TURNS,
                MIN_MAX_STORED_TURNS,
            ),
            window_frac: env_f64(
                "BLUEY_CONV_WINDOW_FRAC",
                DEFAULT_WINDOW_FRAC,
                MIN_WINDOW_FRAC,
            ),
        }
    }
}

/// Env-var knob reader: parse, min-clamp, default (`interval_words()` pattern).
fn env_usize(var: &str, default: usize, min: usize) -> usize {
    env::var(var)
        .ok()
        .and_then(|v| v.trim().parse::<usize>().ok())
        .map(|n| n.max(min))
        .unwrap_or(default)
}

/// Env-var knob reader for fractional knobs: parse, min-clamp, default.
fn env_f64(var: &str, default: f64, min: f64) -> f64 {
    env::var(var)
        .ok()
        .and_then(|v| v.trim().parse::<f64>().ok())
        .map(|n| n.max(min))
        .unwrap_or(default)
}

/// Estimate the token count of `text` for BUDGETING purposes.
///
/// Heuristic: `chars / 4`, floored at one token per whitespace-separated word.
/// Deliberately NOT a real tokenizer: budget math needs a consistent,
/// dependency-free measure (the same text must always cost the same), not
/// precision — LangChain's own fallback length function uses the same chars/4
/// rule. Char-based (not byte-based) so multibyte text isn't over-charged.
/// Empty or whitespace-only text estimates to 0.
pub fn estimate_tokens(text: &str) -> usize {
    let chars = text.chars().count();
    let words = text.split_whitespace().count();
    (chars / CHARS_PER_TOKEN).max(words)
}

/// Context-window size (tokens) for a model id, used to clamp the tail budget.
///
/// This is the production lookup-table pattern: a plain substring match kept
/// deliberately simple — to support a new model family, add one guard arm with
/// its substring and window size. Unknown models fall back to the conservative
/// [`DEFAULT_CONTEXT_WINDOW`]. The `BLUEY_MODEL_WINDOW` env var (a positive
/// integer) WINS over the table entirely — the escape hatch when a model id
/// isn't listed yet.
pub fn context_window_for_model(model: Option<&str>) -> u32 {
    let override_window = env::var("BLUEY_MODEL_WINDOW")
        .ok()
        .and_then(|v| v.trim().parse::<u32>().ok())
        .filter(|n| *n > 0);
    if let Some(window) = override_window {
        return window;
    }
    let name = match model {
        Some(m) => m.to_ascii_lowercase(),
        None => return DEFAULT_CONTEXT_WINDOW,
    };
    match name.as_str() {
        m if m.contains("opus") || m.contains("sonnet") || m.contains("haiku") => 200_000,
        m if m.contains("gpt-5")
            || m.contains("o1")
            || m.contains("o3")
            || m.contains("o4-mini") =>
        {
            200_000
        }
        m if m.contains("gemini") => 1_000_000,
        _ => DEFAULT_CONTEXT_WINDOW,
    }
}

/// One stored conversation turn (a user question or a copilot answer).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConvTurn {
    pub role: ConvRole,
    pub text: String,
    pub epoch_secs: u64,
}

/// Who spoke a [`ConvTurn`].
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum ConvRole {
    User,
    Assistant,
}

impl ConvRole {
    /// Speaker prefix used when rendering a turn into a conversation block.
    pub fn prefix(self) -> &'static str {
        match self {
            ConvRole::User => "You:",
            ConvRole::Assistant => "Copilot:",
        }
    }

    /// Stable lowercase string form for the `role` column of a persisted turn.
    pub fn as_str(self) -> &'static str {
        match self {
            ConvRole::User => "user",
            ConvRole::Assistant => "assistant",
        }
    }

    /// Parse the persisted string form. Unknown values default to `User` (a
    /// corrupt row should never crash a read; the worst case is a mislabeled
    /// prefix, and only `user`/`assistant` are ever written).
    pub fn from_str_lossy(s: &str) -> Self {
        match s {
            "assistant" => ConvRole::Assistant,
            _ => ConvRole::User,
        }
    }
}

/// Render one turn as a `You: …` / `Copilot: …` line (shared by the block
/// tail and the fold prompt so both surfaces read identically).
fn render_turn_line(out: &mut String, turn: &ConvTurn) {
    out.push_str(turn.role.prefix());
    let text = turn.text.trim();
    if !text.is_empty() {
        out.push(' ');
        out.push_str(text);
    }
    out.push('\n');
}

/// Assemble the conversation block: `Some((block_text, overflow))` or `None`
/// when there is nothing to render (no turns AND no summary).
///
/// `block_text` = an optional `Conversation summary:` section (when `summary`
/// is non-empty; it does NOT consume tail budget — it has its own cap
/// upstream via [`bound_summary`]) followed by a `Recent exchange:` tail
/// rendered oldest→newest with `You:` / `Copilot:` prefixes.
///
/// The tail takes as many of the NEWEST turns as fit the effective budget
/// `min(tail_tokens, window_frac × model_window)` — whole turns only, never
/// split, always a contiguous suffix. The newest turn is always included even
/// if it alone exceeds the budget (overshooting a heuristic budget beats
/// dropping the exchange the user is following up on).
///
/// `overflow` = the oldest turns that did NOT fit and are not yet folded into
/// `summary` — the caller summarizes these via [`build_fold_prompt`] and
/// passes the new summary next time. Pure function, fully unit-testable.
pub fn assemble_block(
    cfg: &ConvConfig,
    model: Option<&str>,
    summary: Option<&str>,
    turns: &[ConvTurn],
) -> Option<(String, Vec<ConvTurn>)> {
    let summary = summary.map(str::trim).filter(|s| !s.is_empty());
    if summary.is_none() && turns.is_empty() {
        return None;
    }

    // Effective tail budget: the fixed token cap, further clamped by a
    // fraction of the model's context window so small-window models don't
    // drown in conversation history.
    let window = context_window_for_model(model);
    let frac_budget = (cfg.window_frac * f64::from(window)) as usize;
    let budget = cfg.tail_tokens.min(frac_budget);

    // Select the tail newest-first until the budget is exhausted. `cut` ends
    // as the index of the oldest selected turn; the first (newest) turn is
    // taken unconditionally so the loop always terminates and the tail is
    // never empty while turns exist.
    let mut cut = turns.len();
    let mut used = 0usize;
    for (idx, turn) in turns.iter().enumerate().rev() {
        let cost = estimate_tokens(&turn.text);
        if cut < turns.len() && used.saturating_add(cost) > budget {
            break;
        }
        used = used.saturating_add(cost);
        cut = idx;
    }

    let overflow = turns[..cut].to_vec();
    let tail = &turns[cut..];

    let mut block = String::new();
    if let Some(summary) = summary {
        block.push_str("Conversation summary:\n");
        block.push_str(summary);
    }
    if !tail.is_empty() {
        if !block.is_empty() {
            block.push_str("\n\n");
        }
        block.push_str("Recent exchange:\n");
        for turn in tail {
            render_turn_line(&mut block, turn);
        }
    }
    Some((block.trim_end().to_string(), overflow))
}

/// Build the one-shot prompt that folds overflow turns into the running
/// conversation summary. Mirrors `summary::build_prompt`'s shape: plain text
/// out, no fences, no preamble; the size cap is expressed as a character
/// guide (`summary_tokens × 4` — the [`estimate_tokens`] inversion).
pub fn build_fold_prompt(
    cfg: &ConvConfig,
    current_summary: Option<&str>,
    overflow: &[ConvTurn],
) -> String {
    let max_chars = cfg.summary_tokens.saturating_mul(CHARS_PER_TOKEN);
    let mut prompt = String::with_capacity(overflow.len() * 64 + 1024);
    prompt.push_str(
        "You maintain the running summary of the conversation between the user \
         and their meeting copilot. Fold the older exchange below into the \
         current summary. Keep every decision, question asked, and answer given \
         that still matters (names, numbers, ids, file paths); drop greetings \
         and filler. Output ONLY the updated summary as short plain-text lines \
         starting with \"- \". ",
    );
    prompt.push_str(&format!(
        "Keep the whole summary under about {max_chars} characters. \
         No markdown fences, no preamble, no commentary.\n\n"
    ));
    match current_summary.map(str::trim).filter(|s| !s.is_empty()) {
        Some(summary) => {
            prompt.push_str("CURRENT SUMMARY:\n");
            prompt.push_str(summary);
            prompt.push('\n');
        }
        None => prompt.push_str("CURRENT SUMMARY:\n(none yet — this is the first fold)\n"),
    }
    prompt.push_str("\nOLDER EXCHANGE:\n");
    for turn in overflow {
        render_turn_line(&mut prompt, turn);
    }
    prompt
}

/// Bound + clean a model-produced conversation summary for storage: strip
/// stray code fences and hard-cap the length, truncating on a line boundary
/// where possible (same technique as `summary::bound_summary`, but the cap is
/// `cfg.summary_tokens` converted to chars via the shared 4×-heuristic).
pub fn bound_summary(cfg: &ConvConfig, raw: &str) -> String {
    let max_chars = cfg.summary_tokens.saturating_mul(CHARS_PER_TOKEN);
    let cleaned = raw
        .lines()
        .filter(|line| !line.trim_start().starts_with("```"))
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string();
    if cleaned.chars().count() <= max_chars {
        return cleaned;
    }
    // Truncate to the cap, then back off to the last complete line.
    let capped: String = cleaned.chars().take(max_chars).collect();
    match capped.rfind('\n') {
        Some(pos) if pos > 0 => capped[..pos].trim_end().to_string(),
        _ => capped,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Env-var tests mutate process-global state; serialize them with a lock
    /// (same pattern as `cue-daemon/tests/facts_memory_real.rs::ENV_LOCK`).
    /// Every test that sets, removes, or READS these env vars (including
    /// indirectly via `assemble_block` → `context_window_for_model`) must
    /// hold the guard so a concurrent test's overrides can't leak in.
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn lock_env() -> std::sync::MutexGuard<'static, ()> {
        ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn clear_conv_env() {
        for var in [
            "BLUEY_CONV_TAIL_TOKENS",
            "BLUEY_CONV_SUMMARY_TOKENS",
            "BLUEY_CONV_MAX_TURNS",
            "BLUEY_CONV_WINDOW_FRAC",
            "BLUEY_MODEL_WINDOW",
        ] {
            std::env::remove_var(var);
        }
    }

    fn turn(role: ConvRole, text: &str) -> ConvTurn {
        ConvTurn {
            role,
            text: text.to_string(),
            epoch_secs: 0,
        }
    }

    fn cfg_with(tail_tokens: usize, window_frac: f64) -> ConvConfig {
        ConvConfig {
            tail_tokens,
            window_frac,
            ..ConvConfig::default()
        }
    }

    #[test]
    fn from_env_uses_defaults_when_env_unset() {
        let _guard = lock_env();
        clear_conv_env();

        let cfg = ConvConfig::from_env();
        assert_eq!(cfg.tail_tokens, DEFAULT_TAIL_TOKENS);
        assert_eq!(cfg.summary_tokens, DEFAULT_SUMMARY_TOKENS);
        assert_eq!(cfg.max_stored_turns, DEFAULT_MAX_STORED_TURNS);
        assert!((cfg.window_frac - DEFAULT_WINDOW_FRAC).abs() < f64::EPSILON);
    }

    #[test]
    fn from_env_reads_overrides_and_clamps_minimums() {
        let _guard = lock_env();
        clear_conv_env();

        // Valid overrides are read through.
        std::env::set_var("BLUEY_CONV_TAIL_TOKENS", "5000");
        std::env::set_var("BLUEY_CONV_SUMMARY_TOKENS", "800");
        std::env::set_var("BLUEY_CONV_MAX_TURNS", "50");
        std::env::set_var("BLUEY_CONV_WINDOW_FRAC", "0.1");
        let cfg = ConvConfig::from_env();
        assert_eq!(cfg.tail_tokens, 5000);
        assert_eq!(cfg.summary_tokens, 800);
        assert_eq!(cfg.max_stored_turns, 50);
        assert!((cfg.window_frac - 0.1).abs() < 1e-9);

        // Below-minimum values clamp UP to the minimums.
        std::env::set_var("BLUEY_CONV_TAIL_TOKENS", "10");
        std::env::set_var("BLUEY_CONV_SUMMARY_TOKENS", "1");
        std::env::set_var("BLUEY_CONV_MAX_TURNS", "2");
        std::env::set_var("BLUEY_CONV_WINDOW_FRAC", "0.0001");
        let cfg = ConvConfig::from_env();
        assert_eq!(cfg.tail_tokens, MIN_TAIL_TOKENS);
        assert_eq!(cfg.summary_tokens, MIN_SUMMARY_TOKENS);
        assert_eq!(cfg.max_stored_turns, MIN_MAX_STORED_TURNS);
        assert!((cfg.window_frac - MIN_WINDOW_FRAC).abs() < 1e-9);

        // Unparseable values fall back to the defaults.
        std::env::set_var("BLUEY_CONV_TAIL_TOKENS", "not-a-number");
        let cfg = ConvConfig::from_env();
        assert_eq!(cfg.tail_tokens, DEFAULT_TAIL_TOKENS);

        clear_conv_env();
    }

    #[test]
    fn estimate_tokens_uses_chars_over_four_with_word_floor() {
        // ASCII: 8 chars / 4 = 2 tokens.
        assert_eq!(estimate_tokens("abcdefgh"), 2);
        // Word floor: 5 chars / 4 = 1, but 3 words → 3 tokens.
        assert_eq!(estimate_tokens("a b c"), 3);
        // Unicode counts CHARS, not bytes: 11 CJK chars (33 bytes) → 2.
        assert_eq!(estimate_tokens("日本語テスト実行中です"), 2);
        // Empty and whitespace-only estimate to zero.
        assert_eq!(estimate_tokens(""), 0);
        assert_eq!(estimate_tokens("   "), 0);
    }

    #[test]
    fn context_window_known_models_hit_table_unknown_hits_default() {
        let _guard = lock_env();
        clear_conv_env();

        assert_eq!(context_window_for_model(Some("claude-opus-4-5")), 200_000);
        assert_eq!(
            context_window_for_model(Some("claude-3-7-sonnet-latest")),
            200_000
        );
        assert_eq!(context_window_for_model(Some("Claude-Haiku")), 200_000);
        assert_eq!(context_window_for_model(Some("gpt-5.2")), 200_000);
        assert_eq!(context_window_for_model(Some("o3-mini")), 200_000);
        assert_eq!(context_window_for_model(Some("gemini-2.5-pro")), 1_000_000);
        assert_eq!(
            context_window_for_model(Some("llama-3-70b")),
            DEFAULT_CONTEXT_WINDOW
        );
        assert_eq!(context_window_for_model(None), DEFAULT_CONTEXT_WINDOW);
    }

    #[test]
    fn context_window_env_override_wins_over_table() {
        let _guard = lock_env();
        clear_conv_env();

        std::env::set_var("BLUEY_MODEL_WINDOW", "32000");
        assert_eq!(context_window_for_model(Some("claude-opus-4-5")), 32_000);
        assert_eq!(context_window_for_model(None), 32_000);

        // Invalid or zero overrides are ignored — the table still applies.
        std::env::set_var("BLUEY_MODEL_WINDOW", "banana");
        assert_eq!(context_window_for_model(Some("claude-opus-4-5")), 200_000);
        std::env::set_var("BLUEY_MODEL_WINDOW", "0");
        assert_eq!(context_window_for_model(None), DEFAULT_CONTEXT_WINDOW);

        clear_conv_env();
    }

    #[test]
    fn assemble_block_empty_input_returns_none() {
        let _guard = lock_env();
        clear_conv_env();

        let cfg = ConvConfig::default();
        assert!(assemble_block(&cfg, None, None, &[]).is_none());
        // A whitespace-only summary counts as no summary.
        assert!(assemble_block(&cfg, None, Some("   "), &[]).is_none());
    }

    #[test]
    fn assemble_block_summary_only_renders_without_tail() {
        let _guard = lock_env();
        clear_conv_env();

        let cfg = ConvConfig::default();
        let (block, overflow) = assemble_block(&cfg, None, Some("- talked about sharding"), &[])
            .expect("summary alone must produce a block");
        assert_eq!(block, "Conversation summary:\n- talked about sharding");
        assert!(!block.contains("Recent exchange:"));
        assert!(overflow.is_empty());
    }

    #[test]
    fn assemble_block_budget_selects_newest_turns_and_overflows_oldest() {
        let _guard = lock_env();
        clear_conv_env();

        // Four turns of 2 tokens each (8 chars, 1 word); budget = 5 tokens →
        // newest two fit (4 used), the third would hit 6 > 5 → cut.
        let cfg = cfg_with(5, 1.0);
        let turns = vec![
            turn(ConvRole::User, "aaaaaaaa"),
            turn(ConvRole::Assistant, "bbbbbbbb"),
            turn(ConvRole::User, "cccccccc"),
            turn(ConvRole::Assistant, "dddddddd"),
        ];
        let (block, overflow) =
            assemble_block(&cfg, None, None, &turns).expect("turns must produce a block");
        assert!(block.contains("cccccccc") && block.contains("dddddddd"));
        assert!(!block.contains("aaaaaaaa") && !block.contains("bbbbbbbb"));
        assert_eq!(
            overflow,
            turns[..2].to_vec(),
            "oldest two overflow, in order"
        );
    }

    #[test]
    fn assemble_block_includes_single_oversized_turn_whole() {
        let _guard = lock_env();
        clear_conv_env();

        // Budget 2 tokens; the newest turn alone is ~100 tokens. It must be
        // included whole (never split, never an empty tail, no infinite loop)
        // and the older turn overflows.
        let cfg = cfg_with(2, 1.0);
        let big = "y".repeat(400);
        let turns = vec![
            turn(ConvRole::User, "earlier question"),
            turn(ConvRole::Assistant, &big),
        ];
        let (block, overflow) =
            assemble_block(&cfg, None, None, &turns).expect("oversized turn must still render");
        assert!(block.contains(&big), "oversized turn included in full");
        assert_eq!(overflow, turns[..1].to_vec());

        // Alone, it renders with no overflow at all.
        let only = vec![turn(ConvRole::Assistant, &big)];
        let (block, overflow) = assemble_block(&cfg, None, None, &only).expect("must render");
        assert!(block.contains(&big));
        assert!(overflow.is_empty());
    }

    #[test]
    fn assemble_block_renders_tail_oldest_to_newest_with_prefixes() {
        let _guard = lock_env();
        clear_conv_env();

        let cfg = ConvConfig::default();
        let turns = vec![
            turn(ConvRole::User, "how do we deploy?"),
            turn(ConvRole::Assistant, "use the release script."),
        ];
        let (block, overflow) =
            assemble_block(&cfg, None, Some("- deploy discussion"), &turns).expect("must render");
        assert!(overflow.is_empty());

        let summary_at = block.find("Conversation summary:").expect("summary header");
        let tail_at = block.find("Recent exchange:").expect("tail header");
        let user_at = block.find("You: how do we deploy?").expect("user line");
        let copilot_at = block
            .find("Copilot: use the release script.")
            .expect("copilot line");
        assert!(summary_at < tail_at, "summary section precedes the tail");
        assert!(tail_at < user_at, "header precedes turns");
        assert!(user_at < copilot_at, "turns render oldest→newest");
    }

    #[test]
    fn assemble_block_window_frac_clamps_budget_for_small_window() {
        let _guard = lock_env();
        clear_conv_env();

        // Five 10-token turns (40 chars each). With a 1000-token window and
        // frac 0.02 the effective budget is min(2000, 20) = 20 → newest two
        // turns fit, three overflow.
        std::env::set_var("BLUEY_MODEL_WINDOW", "1000");
        let cfg = cfg_with(2000, 0.02);
        let turns: Vec<ConvTurn> = (0..5)
            .map(|i| {
                let role = if i % 2 == 0 {
                    ConvRole::User
                } else {
                    ConvRole::Assistant
                };
                turn(role, &"x".repeat(40))
            })
            .collect();
        let (_, overflow) =
            assemble_block(&cfg, Some("tiny-model"), None, &turns).expect("must render");
        assert_eq!(overflow.len(), 3, "small window clamps the tail budget");

        // Without the override the default 100k window allows the full tail.
        clear_conv_env();
        let (_, overflow) =
            assemble_block(&cfg, Some("tiny-model"), None, &turns).expect("must render");
        assert!(overflow.is_empty(), "default window fits all five turns");
    }

    #[test]
    fn fold_prompt_carries_current_summary_and_overflow() {
        let cfg = ConvConfig::default();
        let overflow = vec![
            turn(ConvRole::User, "which db did we pick?"),
            turn(ConvRole::Assistant, "sqlite, single-writer."),
        ];
        let prompt = build_fold_prompt(&cfg, Some("- earlier: schema debate"), &overflow);
        assert!(prompt.contains("CURRENT SUMMARY:"));
        assert!(prompt.contains("- earlier: schema debate"));
        assert!(prompt.contains("OLDER EXCHANGE:"));
        assert!(prompt.contains("You: which db did we pick?"));
        assert!(prompt.contains("Copilot: sqlite, single-writer."));
        // Cap guidance is the token cap expressed as chars — value-agnostic so
        // tuning `summary_tokens` never breaks this test.
        let expected_chars = cfg.summary_tokens * CHARS_PER_TOKEN;
        assert!(prompt.contains(&format!("{expected_chars} characters")));
        assert!(!prompt.contains("```"));

        let first = build_fold_prompt(&cfg, None, &overflow);
        assert!(first.contains("first fold"));
    }

    #[test]
    fn bound_summary_strips_fences_and_caps_on_line_boundary() {
        // summary_tokens 25 → 100-char cap.
        let cfg = ConvConfig {
            summary_tokens: 25,
            ..ConvConfig::default()
        };

        let fenced = "```\n- point one\n```";
        assert_eq!(bound_summary(&cfg, fenced), "- point one");

        let long_line = "- ".to_string() + &"x".repeat(200);
        let many = format!("- keep\n{long_line}");
        let bounded = bound_summary(&cfg, &many);
        assert!(bounded.chars().count() <= 100);
        assert!(bounded.starts_with("- keep"));
    }
}
