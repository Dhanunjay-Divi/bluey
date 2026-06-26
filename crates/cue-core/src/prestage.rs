//! Pre-meeting context builder (master doc §4 — "pre-staging is the spine").
//!
//! Before / at the start of a meeting, Bluey can warm the context package so
//! the first answer is instant and grounded, not cold-started. This module
//! assembles a compact **pre-meeting brief** from whatever signals are
//! available locally — the meeting title, the people in the room, agenda
//! lines, and any pre-loaded tickets / PRs the user attached ahead of time.
//!
//! ## Lean by design, extensible by seam
//!
//! The MVP populates [`PrestageInput`] from local signals only (no external
//! calendar API — that is a separate, heavier integration). A future calendar
//! connector simply fills the same struct (invite title, attendees, agenda,
//! linked tickets) and the rest of the pipeline is unchanged. The function is
//! pure and side-effect free so it is cheap to unit test and safe to call on
//! the hot path.

use serde::{Deserialize, Serialize};

/// Everything the pre-stage brief is built from. Each field is optional /
/// possibly empty; the builder includes only the sections that have content.
/// Local callers fill `title` (+ whatever they have); a calendar connector
/// would later fill `attendees`, `agenda`, and `linked_refs`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrestageInput {
    /// Meeting / invite title.
    pub title: Option<String>,
    /// People expected in the meeting (display names).
    pub attendees: Vec<String>,
    /// Agenda lines, if known.
    pub agenda: Vec<String>,
    /// Pre-loaded references the agent should expect to be asked about —
    /// ticket ids, PR titles, doc names (e.g. `"AUTH-12"`, `"PR #4821"`).
    /// Per the master doc these make even a mangled "AUTH-12" resolve because
    /// the agent already knows it is in play.
    pub linked_refs: Vec<String>,
}

impl PrestageInput {
    /// True when there is nothing to brief — the builder returns `None`.
    pub fn is_empty(&self) -> bool {
        self.title
            .as_ref()
            .map(|t| t.trim().is_empty())
            .unwrap_or(true)
            && self.attendees.iter().all(|a| a.trim().is_empty())
            && self.agenda.iter().all(|a| a.trim().is_empty())
            && self.linked_refs.iter().all(|r| r.trim().is_empty())
    }
}

/// Assemble the pre-meeting brief as a compact, agent-readable block, or
/// `None` when there is nothing worth staging. Sections are bounded so the
/// brief stays small relative to the live transcript.
pub fn build_prestage_brief(input: &PrestageInput) -> Option<String> {
    const MAX_ATTENDEES: usize = 20;
    const MAX_AGENDA: usize = 20;
    const MAX_REFS: usize = 30;

    if input.is_empty() {
        return None;
    }

    let mut block = String::from("Pre-meeting brief (staged before the call):\n");

    if let Some(title) = input.title.as_ref().filter(|t| !t.trim().is_empty()) {
        block.push_str("Meeting: ");
        block.push_str(title.trim());
        block.push('\n');
    }

    let attendees = nonempty_bounded(&input.attendees, MAX_ATTENDEES);
    if !attendees.is_empty() {
        block.push_str("Participants: ");
        block.push_str(&attendees.join(", "));
        block.push('\n');
    }

    let agenda = nonempty_bounded(&input.agenda, MAX_AGENDA);
    if !agenda.is_empty() {
        block.push_str("Agenda:\n");
        for line in agenda {
            block.push_str("- ");
            block.push_str(&line);
            block.push('\n');
        }
    }

    let refs = nonempty_bounded(&input.linked_refs, MAX_REFS);
    if !refs.is_empty() {
        block.push_str("Pre-loaded references (expect questions about these):\n");
        for r in refs {
            block.push_str("- ");
            block.push_str(&r);
            block.push('\n');
        }
    }

    Some(block.trim_end().to_string())
}

/// Trim, drop empties, and cap to `max` while preserving order.
fn nonempty_bounded(values: &[String], max: usize) -> Vec<String> {
    values
        .iter()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .take(max)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_input_yields_no_brief() {
        assert!(build_prestage_brief(&PrestageInput::default()).is_none());
        let blank = PrestageInput {
            title: Some("   ".to_string()),
            attendees: vec!["".to_string()],
            ..Default::default()
        };
        assert!(blank.is_empty());
        assert!(build_prestage_brief(&blank).is_none());
    }

    #[test]
    fn title_only_yields_a_brief() {
        let input = PrestageInput {
            title: Some("Weekly standup".to_string()),
            ..Default::default()
        };
        let brief = build_prestage_brief(&input).expect("brief");
        assert!(brief.contains("Meeting: Weekly standup"));
    }

    #[test]
    fn full_brief_includes_all_sections_bounded_and_trimmed() {
        let input = PrestageInput {
            title: Some("Auth review".to_string()),
            attendees: vec!["Alex".to_string(), "  ".to_string(), "Sam".to_string()],
            agenda: vec!["Ship auth flow".to_string()],
            linked_refs: vec!["AUTH-12".to_string(), "PR #4821".to_string()],
        };
        let brief = build_prestage_brief(&input).expect("brief");
        assert!(brief.contains("Meeting: Auth review"));
        // Empty attendee dropped.
        assert!(brief.contains("Participants: Alex, Sam"));
        assert!(brief.contains("Agenda:"));
        assert!(brief.contains("- Ship auth flow"));
        assert!(brief.contains("AUTH-12"));
        assert!(brief.contains("PR #4821"));
        // No trailing whitespace.
        assert_eq!(brief, brief.trim_end());
    }
}
