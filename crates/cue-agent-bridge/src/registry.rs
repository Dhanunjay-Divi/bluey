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
    /// Windows install-dir / `.exe` names to look for under the program dirs
    /// (`ProgramFiles`, `ProgramFiles(x86)`, `%LOCALAPPDATA%\Programs`). Each
    /// entry is matched both as a directory name (Electron apps install under
    /// `<App>/`) and as a bare `<name>.exe`. Empty when there is no GUI app.
    pub app_dirs_windows: &'static [&'static str],
    /// Data-dir glob roots, relative to `$HOME` (e.g. `.cursor`,
    /// `Library/Application Support/Cursor`). Read-only scan targets. On
    /// Windows the `Library/Application Support/<App>` rows are remapped to the
    /// Windows app-data base (`%APPDATA%`); see [`AgentEntry::home_relative_globs`].
    pub data_dir_globs: &'static [&'static str],
    /// Windows data-dir names under `%APPDATA%` (e.g. `Cursor`, `Code`). These
    /// hold the VS Code-family `User/globalStorage/state.vscdb` tree. Empty for
    /// agents whose footprint lives under `%USERPROFILE%` (dotfile CLIs), which
    /// are covered by the [`AgentEntry::data_dir_globs`] dotfile rows directly.
    pub app_data_windows: &'static [&'static str],
    /// Session store format, when this agent has a local store.
    pub session_format: Option<SessionFormat>,
    /// Drive command template: program + args, `{prompt}` substituted later.
    /// Stored as data only — never executed in Slice 1.
    pub drive_command: &'static [&'static str],
}

/// Prefix that marks a [`AgentEntry::data_dir_globs`] entry as a macOS-only
/// Application-Support path. Globs without this prefix are HOME-relative
/// dotfiles (`.claude`, `.cursor`, …) and resolve identically on every OS.
pub const MACOS_APP_SUPPORT_PREFIX: &str = "Library/Application Support/";

impl AgentEntry {
    /// HOME-relative data-dir globs that are cross-platform (the dotfile rows
    /// such as `.claude`/`.cursor`). The macOS-only `Library/Application
    /// Support/<App>` rows are filtered out — on Windows their equivalent is in
    /// [`AgentEntry::app_data_windows`], joined to `%APPDATA%` instead of HOME.
    pub fn home_relative_globs(&self) -> impl Iterator<Item = &'static str> {
        self.data_dir_globs
            .iter()
            .copied()
            .filter(|g| !g.starts_with(MACOS_APP_SUPPORT_PREFIX))
    }
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
        app_dirs_windows: &["Claude"],
        data_dir_globs: &[".claude"],
        app_data_windows: &[],
        session_format: Some(SessionFormat::Jsonl),
        drive_command: &["claude", "-p", "{prompt}"],
    },
    AgentEntry {
        kind_tag: KindTag::Cursor,
        display_name: "Cursor",
        binary_candidates: &["cursor-agent", "cursor"],
        app_bundles: &["Cursor.app"],
        app_dirs_windows: &["cursor", "Cursor"],
        data_dir_globs: &[".cursor", "Library/Application Support/Cursor"],
        app_data_windows: &["Cursor"],
        session_format: Some(SessionFormat::SqliteVscdb),
        drive_command: &["cursor-agent", "-p", "{prompt}"],
    },
    AgentEntry {
        kind_tag: KindTag::Antigravity,
        display_name: "Antigravity",
        binary_candidates: &["antigravity", "gemini"],
        app_bundles: &["Antigravity.app"],
        app_dirs_windows: &["Antigravity"],
        data_dir_globs: &[".gemini/antigravity"],
        app_data_windows: &[],
        session_format: Some(SessionFormat::Protobuf),
        drive_command: &["gemini", "-p", "{prompt}"],
    },
    AgentEntry {
        kind_tag: KindTag::Copilot,
        display_name: "GitHub Copilot",
        binary_candidates: &["copilot"],
        app_bundles: &[],
        app_dirs_windows: &[],
        data_dir_globs: &["Library/Application Support/Code"],
        app_data_windows: &["Code"],
        session_format: Some(SessionFormat::JsonFiles),
        drive_command: &["copilot", "-p", "{prompt}"],
    },
    AgentEntry {
        kind_tag: KindTag::Gemini,
        display_name: "Gemini CLI",
        binary_candidates: &["gemini"],
        app_bundles: &[],
        app_dirs_windows: &[],
        data_dir_globs: &[".gemini"],
        app_data_windows: &[],
        session_format: None,
        drive_command: &["gemini", "-p", "{prompt}"],
    },
    AgentEntry {
        kind_tag: KindTag::Codex,
        display_name: "Codex",
        binary_candidates: &["codex"],
        app_bundles: &[],
        app_dirs_windows: &[],
        data_dir_globs: &[".codex"],
        app_data_windows: &[],
        session_format: None,
        drive_command: &["codex", "exec", "{prompt}"],
    },
    AgentEntry {
        kind_tag: KindTag::Aider,
        display_name: "Aider",
        binary_candidates: &["aider"],
        app_bundles: &[],
        app_dirs_windows: &[],
        data_dir_globs: &[".aider"],
        app_data_windows: &[],
        session_format: None,
        drive_command: &["aider", "--message", "{prompt}"],
    },
    AgentEntry {
        kind_tag: KindTag::Windsurf,
        display_name: "Windsurf",
        binary_candidates: &["windsurf"],
        app_bundles: &["Windsurf.app"],
        app_dirs_windows: &["Windsurf"],
        data_dir_globs: &[".codeium/windsurf", "Library/Application Support/Windsurf"],
        app_data_windows: &["Windsurf"],
        session_format: Some(SessionFormat::SqliteVscdb),
        drive_command: &[],
    },
    AgentEntry {
        kind_tag: KindTag::VsCode,
        display_name: "VS Code",
        binary_candidates: &["code"],
        app_bundles: &["Visual Studio Code.app"],
        app_dirs_windows: &["Microsoft VS Code"],
        data_dir_globs: &["Library/Application Support/Code"],
        app_data_windows: &["Code"],
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

    fn row(kind: KindTag) -> &'static AgentEntry {
        REGISTRY
            .iter()
            .find(|e| e.kind_tag == kind)
            .expect("registry row")
    }

    #[test]
    fn test_home_relative_globs_drops_mac_only_app_support() {
        // Cursor has a dotfile glob (.cursor) AND a mac-only Application Support
        // glob. Only the dotfile is HOME-relative / cross-platform.
        let globs: Vec<_> = row(KindTag::Cursor).home_relative_globs().collect();
        assert_eq!(globs, vec![".cursor"]);
    }

    #[test]
    fn test_home_relative_globs_keeps_pure_dotfiles() {
        // Claude has only a dotfile glob → it survives unchanged on every OS.
        let globs: Vec<_> = row(KindTag::ClaudeCode).home_relative_globs().collect();
        assert_eq!(globs, vec![".claude"]);
    }

    #[test]
    fn test_vscode_family_rows_have_windows_app_data() {
        // VS Code-family agents must declare a %APPDATA% dir so their
        // User/globalStorage tree is discoverable on Windows.
        for kind in [KindTag::Cursor, KindTag::VsCode, KindTag::Windsurf] {
            assert!(
                !row(kind).app_data_windows.is_empty(),
                "{kind:?} should declare a Windows %APPDATA% dir"
            );
        }
    }

    #[test]
    fn test_gui_agents_have_windows_install_dirs() {
        // Agents that ship a GUI app should also declare a Windows install dir.
        for kind in [KindTag::Cursor, KindTag::VsCode, KindTag::Windsurf] {
            assert!(
                !row(kind).app_dirs_windows.is_empty(),
                "{kind:?} should declare a Windows install dir"
            );
        }
    }

    #[test]
    fn test_no_app_data_windows_glob_carries_path_separator() {
        // %APPDATA% dirs are single directory names, joined per-OS by PathBuf —
        // they must never embed a hardcoded separator.
        for e in REGISTRY {
            for dir in e.app_data_windows {
                assert!(
                    !dir.contains('/') && !dir.contains('\\'),
                    "{}: app_data_windows entry {dir:?} must be a bare name",
                    e.display_name
                );
            }
        }
    }
}
