//! Static registry of known agents — a **data table**, not code paths.
//!
//! Adding a named agent = adding one [`AgentEntry`] row below. Discovery and
//! capability logic read this table; they never special-case an agent by name.
//! The drive command template is stored as data even though Slice 1 never runs
//! it (driving lands in Slice 2).

use crate::{AgentKind, SessionFormat};

/// One row in the registry: everything needed to detect and (later) drive an
/// agent, expressed as pure data.
#[derive(Debug, Clone, Copy)]
pub struct AgentEntry {
    /// Stable kind this row maps to.
    pub kind_tag: KindTag,
    /// Human-facing display name (e.g. "Claude Code").
    pub display_name: &'static str,
    /// CLI binary names to look for on `PATH`.
    pub binary_candidates: &'static [&'static str],
    /// macOS `.app` bundle names to look for under `/Applications`.
    pub app_bundles: &'static [&'static str],
    /// Data-dir glob roots, relative to `$HOME` (e.g. `.cursor`,
    /// `Library/Application Support/Cursor`). Read-only scan targets.
    pub data_dir_globs: &'static [&'static str],
    /// Session store format, when this agent has a local store.
    pub session_format: Option<SessionFormat>,
    /// Drive command template: program + args, `{prompt}` substituted later.
    /// Stored as data only — never executed in Slice 1.
    pub drive_command: &'static [&'static str],
}

/// A `Copy`-friendly tag that maps to [`AgentKind`] without owning a `String`
/// (registry rows are `const`, so they cannot hold `AgentKind::Other(String)`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KindTag {
    ClaudeCode,
    Cursor,
    Antigravity,
    Copilot,
    Gemini,
    Codex,
    Aider,
    Windsurf,
    VsCode,
}

impl KindTag {
    /// Resolve to the owned [`AgentKind`]. Generic VS Code maps to `VsCodeFork`
    /// (the registry row is the "known" VS Code, but it shares the fork code
    /// path with unrecognized forks).
    pub fn to_agent_kind(self) -> AgentKind {
        match self {
            KindTag::ClaudeCode => AgentKind::ClaudeCode,
            KindTag::Cursor => AgentKind::Cursor,
            KindTag::Antigravity => AgentKind::Antigravity,
            KindTag::Copilot => AgentKind::Copilot,
            KindTag::Gemini => AgentKind::Gemini,
            KindTag::Codex => AgentKind::Codex,
            KindTag::Aider => AgentKind::Aider,
            KindTag::Windsurf => AgentKind::Windsurf,
            KindTag::VsCode => AgentKind::VsCodeFork,
        }
    }
}

/// The known-agent table. Order is detection priority (earlier rows win on a
/// tie when the same evidence could match multiple rows).
pub const REGISTRY: &[AgentEntry] = &[
    AgentEntry {
        kind_tag: KindTag::ClaudeCode,
        display_name: "Claude Code",
        binary_candidates: &["claude"],
        app_bundles: &["Claude.app"],
        data_dir_globs: &[".claude"],
        session_format: Some(SessionFormat::Jsonl),
        drive_command: &["claude", "-p", "{prompt}"],
    },
    AgentEntry {
        kind_tag: KindTag::Cursor,
        display_name: "Cursor",
        binary_candidates: &["cursor-agent", "cursor"],
        app_bundles: &["Cursor.app"],
        data_dir_globs: &[".cursor", "Library/Application Support/Cursor"],
        session_format: Some(SessionFormat::SqliteVscdb),
        drive_command: &["cursor-agent", "-p", "{prompt}"],
    },
    AgentEntry {
        kind_tag: KindTag::Antigravity,
        display_name: "Antigravity",
        binary_candidates: &["antigravity", "gemini"],
        app_bundles: &["Antigravity.app"],
        data_dir_globs: &[".gemini/antigravity"],
        session_format: Some(SessionFormat::Protobuf),
        drive_command: &["gemini", "-p", "{prompt}"],
    },
    AgentEntry {
        kind_tag: KindTag::Copilot,
        display_name: "GitHub Copilot",
        binary_candidates: &["copilot"],
        app_bundles: &[],
        data_dir_globs: &["Library/Application Support/Code"],
        session_format: Some(SessionFormat::JsonFiles),
        drive_command: &["copilot", "-p", "{prompt}"],
    },
    AgentEntry {
        kind_tag: KindTag::Gemini,
        display_name: "Gemini CLI",
        binary_candidates: &["gemini"],
        app_bundles: &[],
        data_dir_globs: &[".gemini"],
        session_format: None,
        drive_command: &["gemini", "-p", "{prompt}"],
    },
    AgentEntry {
        kind_tag: KindTag::Codex,
        display_name: "Codex",
        binary_candidates: &["codex"],
        app_bundles: &[],
        data_dir_globs: &[".codex"],
        session_format: None,
        drive_command: &["codex", "exec", "{prompt}"],
    },
    AgentEntry {
        kind_tag: KindTag::Aider,
        display_name: "Aider",
        binary_candidates: &["aider"],
        app_bundles: &[],
        data_dir_globs: &[".aider"],
        session_format: None,
        drive_command: &["aider", "--message", "{prompt}"],
    },
    AgentEntry {
        kind_tag: KindTag::Windsurf,
        display_name: "Windsurf",
        binary_candidates: &["windsurf"],
        app_bundles: &["Windsurf.app"],
        data_dir_globs: &[".codeium/windsurf", "Library/Application Support/Windsurf"],
        session_format: Some(SessionFormat::SqliteVscdb),
        drive_command: &[],
    },
    AgentEntry {
        kind_tag: KindTag::VsCode,
        display_name: "VS Code",
        binary_candidates: &["code"],
        app_bundles: &["Visual Studio Code.app"],
        data_dir_globs: &["Library/Application Support/Code"],
        session_format: Some(SessionFormat::JsonFiles),
        drive_command: &[],
    },
];

/// All distinct binary candidate names across the registry (for a single PATH
/// scan pass). Duplicates (e.g. `gemini`) are intentionally retained per-row;
/// callers dedup by resulting agent.
pub fn all_binary_candidates() -> impl Iterator<Item = (&'static AgentEntry, &'static str)> {
    REGISTRY
        .iter()
        .flat_map(|e| e.binary_candidates.iter().map(move |b| (e, *b)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_has_expected_agents() {
        let names: Vec<_> = REGISTRY.iter().map(|e| e.display_name).collect();
        for expected in [
            "Claude Code",
            "Cursor",
            "Antigravity",
            "GitHub Copilot",
            "Gemini CLI",
            "Codex",
            "Aider",
            "Windsurf",
            "VS Code",
        ] {
            assert!(
                names.contains(&expected),
                "missing registry row: {expected}"
            );
        }
    }

    #[test]
    fn test_kind_tag_maps_to_agent_kind() {
        assert_eq!(KindTag::ClaudeCode.to_agent_kind(), AgentKind::ClaudeCode);
        assert_eq!(KindTag::VsCode.to_agent_kind(), AgentKind::VsCodeFork);
    }

    #[test]
    fn test_all_binary_candidates_nonempty() {
        let count = all_binary_candidates().count();
        assert!(count >= 9, "expected at least one binary per agent");
    }
}
