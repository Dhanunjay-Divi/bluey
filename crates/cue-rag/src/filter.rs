//! Index-time filters for agent-session history.
//!
//! Bluey drives headless agent sessions with its own prompts (ledger
//! extraction, rolling summary) and those prompts — plus agent error turns —
//! leak into the on-disk session stores as fake "user" turns. Indexing them
//! poisons retrieval, so the session indexer (Wave 2) filters them out with
//! [`is_self_prompt`]. Dependency-free by design.

/// Opening phrases of Bluey's own headless prompts and error turns, matched
/// case-insensitively as substrings. Measured leaks on real session data:
/// the ledger prompt (x67), the running-summary prompt (x10), and the Claude
/// Code error turn (x13). Keep each marker specific to the prompt's opening
/// phrasing so ordinary conversation about these topics does not match;
/// extend the list as new leaks are measured.
const SELF_PROMPT_MARKERS: &[&str] = &[
    "you extract a meeting ledger",
    "you maintain the running summary of a live technical meeting",
    "api error: claude code is unable to respond",
    // LIVE-MEASURED 2026-07-18: the first real index over this machine's Claude
    // Code history ranked Bluey's OWN driven prompt as the top hit for a real
    // query. Every in-meeting ask carries the answer-style reminder verbatim, so
    // without this marker each ask self-pollutes the index it later searches.
    "cover every point that matters in as few words as it takes",
    // The copilot persona (set once per meeting in the warm-up prime) and the
    // conversation-fold prompt (Wave 1) ride the same driven-session path.
    "for the rest of this meeting you are my meeting copilot",
    "you maintain the running summary of the conversation",
    // The internal "answer the most recent question" pointer, sent as the prompt
    // by the for-me / ask-recent paths.
    "answer the most recent question or request raised in the meeting transcript",
];

/// Detects Bluey's own headless prompts / error turns leaked into agent
/// session stores as fake "user" turns. Consumed by the session indexer
/// (Wave 2) to keep them out of the retrieval index. Case-insensitive
/// substring match over [`SELF_PROMPT_MARKERS`]; extend that list as new
/// leaks are measured.
pub fn is_self_prompt(text: &str) -> bool {
    let lowered = text.to_lowercase();
    SELF_PROMPT_MARKERS
        .iter()
        .any(|marker| lowered.contains(marker))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_self_prompt_matches_ledger_prompt() {
        assert!(is_self_prompt(
            "You extract a meeting ledger from the transcript below."
        ));
    }

    #[test]
    fn is_self_prompt_matches_running_summary_prompt() {
        assert!(is_self_prompt(
            "You maintain the running summary of a live technical meeting. Update it now."
        ));
    }

    #[test]
    fn is_self_prompt_matches_claude_code_error_turn() {
        assert!(is_self_prompt(
            "API Error: Claude Code is unable to respond to this request."
        ));
    }

    #[test]
    fn is_self_prompt_is_case_insensitive() {
        assert!(is_self_prompt("YOU EXTRACT A MEETING LEDGER from this."));
        assert!(is_self_prompt(
            "api error: claude code is unable to respond"
        ));
    }

    #[test]
    fn is_self_prompt_matches_marker_embedded_mid_text() {
        assert!(is_self_prompt(
            "system context follows. You extract a meeting ledger for the team."
        ));
    }

    #[test]
    fn is_self_prompt_ignores_ordinary_mentions_of_the_ledger() {
        assert!(!is_self_prompt("we discussed the meeting ledger yesterday"));
    }

    #[test]
    fn is_self_prompt_matches_the_live_measured_driven_prompt_leaks() {
        // The exact shapes observed polluting the first real index (2026-07-18):
        // an ask carrying the answer-style reminder, the warm-up persona, the
        // conversation-fold prompt, and the internal ask-recent pointer.
        assert!(is_self_prompt(
            "What was the root cause?\n\n(Cover every point that matters in as few \
             words as it takes — no padding, no preamble, no report headers.)"
        ));
        assert!(is_self_prompt(
            "For the rest of this meeting you are my meeting copilot. When I ask you a question…"
        ));
        assert!(is_self_prompt(
            "You maintain the running summary of the conversation. Fold these turns in."
        ));
        assert!(is_self_prompt(
            "Answer the most recent question or request raised in the meeting transcript."
        ));
        // Ordinary prose about brevity must NOT trip the reminder marker.
        assert!(!is_self_prompt(
            "we agreed to cover every point in the doc before shipping"
        ));
    }

    #[test]
    fn is_self_prompt_ignores_ordinary_text_and_empty_input() {
        assert!(!is_self_prompt(
            "What did the customer say about the pricing summary?"
        ));
        assert!(!is_self_prompt(""));
    }
}
