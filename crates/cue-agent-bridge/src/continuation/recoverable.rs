//! Resume-error classification: decide whether an agent's resume failure is one
//! a fresh (no-resume) retry can recover from.
//!
//! Pure string classification, agent-agnostic — moved out of the daemon so the
//! continuation spine owns the policy. The caller (the answer engine) uses the
//! verdict to decide between surfacing the error and retrying without `--resume`.

/// Whether an agent error means native **resume specifically** failed in a way a
/// fresh (no-resume) retry can recover. Three agent-agnostic classes:
///
/// - **Too large:** the session exceeds the context window and can't be loaded
///   or compacted headlessly ("prompt is too long", "conversation too long", …).
/// - **Not resumable:** the pinned session id can't be found/loaded by this CLI
///   ("no conversation found", "session not found", "invalid session"). Common
///   when an app-only session was never written to the CLI's shared store, or
///   the cwd differs.
/// - **Tool-incompatible:** the persisted transcript references tools that aren't
///   available in Bluey's headless resume context, so the agent rejects
///   replaying it (seen live: Claude `400 invalid_request_error: "Tool reference
///   'X' not found in available tools"`).
///
/// In all cases a fresh session in the project dir still answers — it has the
/// code, project rules, and MCP connectors regardless of the prior transcript.
#[must_use]
pub fn is_resume_recoverable_error(message: &str) -> bool {
    let m = message.to_lowercase();
    // Too-large class.
    let too_large = m.contains("prompt is too long")
        || m.contains("conversation too long")
        || m.contains("context length")
        || m.contains("context window")
        || m.contains("too many tokens")
        || (m.contains("maximum") && m.contains("token"));
    // Session-not-resumable class.
    let not_resumable = m.contains("no conversation found")
        || m.contains("session not found")
        || m.contains("no session")
        || m.contains("invalid session")
        || (m.contains("session") && m.contains("not found"));
    // Resume-incompatibility class: the persisted transcript references tools (or
    // other state) that aren't available in Bluey's headless resume context, so
    // the agent rejects replaying it as-is. A fresh (non-resume) drive in the
    // same project still answers, so this is recoverable like an overflow.
    let tool_incompatible = m.contains("not found in available tools")
        || (m.contains("tool reference") && m.contains("not found"));
    too_large || not_resumable || tool_incompatible
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resume_recoverable_error_matches_too_large_and_not_found_classes() {
        // Too-large class.
        assert!(is_resume_recoverable_error("Prompt is too long"));
        assert!(is_resume_recoverable_error("Error: conversation too long"));
        assert!(is_resume_recoverable_error("exceeds the context window"));
        assert!(is_resume_recoverable_error(
            "This exceeds the maximum number of tokens"
        ));
        assert!(is_resume_recoverable_error(
            "input length and max tokens exceed context length"
        ));
        // Real overflow phrasing captured live.
        assert!(is_resume_recoverable_error(
            "206453 tokens > 200000 maximum"
        ));
        // Not-resumable class.
        assert!(is_resume_recoverable_error("session not found"));
        assert!(is_resume_recoverable_error("invalid session id"));
        assert!(is_resume_recoverable_error(
            "No conversation found with that id"
        ));
        assert!(is_resume_recoverable_error(
            "No conversation found with session ID: 12dea178-…"
        ));
        // Resume-incompatibility class (CAPTURED LIVE): the full Anthropic 400
        // body, plus the bare phrasing.
        assert!(is_resume_recoverable_error(
            r#"400 {"type":"error","error":{"type":"invalid_request_error","message":"Tool reference 'TaskCreate' not found in available tools"}}"#
        ));
        assert!(is_resume_recoverable_error(
            "Tool reference 'mcp__x' not found in available tools"
        ));

        // Non-recoverable: auth / missing binary / generic.
        assert!(!is_resume_recoverable_error("not signed in"));
        assert!(!is_resume_recoverable_error("command not found: claude"));
        assert!(!is_resume_recoverable_error(
            "some unrelated runtime failure"
        ));
        assert!(!is_resume_recoverable_error(""));
    }
}
