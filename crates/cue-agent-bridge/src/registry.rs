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
    /// Subdirectory under the data dir holding the JSONL session store. Claude
    /// uses `projects/<encoded-cwd>/*.jsonl`; Codex uses
    /// `sessions/YYYY/MM/DD/rollout-*.jsonl`. Ignored for non-JSONL formats.
    pub jsonl_subdir: &'static str,
    /// Drive command template: program + args, `{prompt}` substituted later.
    /// Stored as data only — never executed in Slice 1.
    pub drive_command: &'static [&'static str],
    /// Extra static args appended for a normal **answer**. Kept minimal and
    /// **read-safe** — never an "auto-approve everything" flag (that would let a
    /// read-intent answer perform writes). Most agents need nothing here.
    pub answer_args: &'static [&'static str],
    /// When set, the flag this agent uses to **auto-approve only named MCP
    /// servers** in headless answer mode (e.g. Gemini's
    /// `--allowed-mcp-server-names`). The drive layer reads the agent's own
    /// configured MCP server names and appends them after this flag, so the
    /// agent's MCP read-tools fire while file/shell writes still require an
    /// approval that never arrives headless (and are therefore blocked). `None`
    /// for agents that load MCP without a flag (Claude) or have no MCP.
    pub mcp_allow_flag: Option<&'static str>,
    /// Review-gated "Fix" profile: the extra args that switch this agent
    /// between propose-only and apply (see [`FixProfile`]).
    pub fix: FixProfile,
    /// How to PROACTIVELY install this agent's CLI when it's missing (so a user
    /// with only the GUI app becomes drivable). `None` for agents with no known
    /// official CLI installer. Recipes are vetted, official sources only — never
    /// an arbitrary string — and are always run consent-gated + verified.
    pub install: Option<InstallRecipe>,
}

/// A vetted recipe for installing an agent's CLI. Pure data; the runner in
/// `provision.rs` executes it (consent-gated) and then verifies `verify_binary`
/// appeared on `PATH`. Commands are official-source only (see
/// `PLAN-PRODUCTION-VISION.md` §5 Phase A) — Bluey never runs an arbitrary
/// install string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InstallRecipe {
    /// How the recipe is delivered.
    pub method: InstallMethod,
    /// The package/URL the method consumes (npm package name, or curl script
    /// URL). e.g. `@openai/codex`, or `https://cursor.com/install`.
    pub spec: &'static str,
    /// The binary expected on `PATH` after a successful install — used to
    /// VERIFY the install actually worked (never trust the installer's exit
    /// code alone).
    pub verify_binary: &'static str,
}

/// Delivery method for an [`InstallRecipe`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallMethod {
    /// `npm install -g <spec>`. Requires Node/npm present.
    NpmGlobal,
    /// `curl -fsSL <spec> | bash` — official vendor install script.
    CurlScript,
}

/// Per-agent profile for the review-gated **Fix** lane (see
/// `docs/work/PLAN-FIX-BUTTON.md` §3). Pure data, `Copy`-friendly (only
/// `&'static` slices and a `bool`), so registry rows stay `const`.
///
/// Two arg sets express the same agent in two safety postures:
/// - [`propose_args`](FixProfile::propose_args) force **propose-only**
///   (read-only / plan): the agent diagnoses and proposes a fix but applies
///   nothing. Empty means the agent has no native propose flag and relies on
///   prompt engineering plus simply *omitting* the apply args (e.g. Cursor,
///   whose `--plan` flag is a known bug that writes files — never use it).
/// - [`apply_args`](FixProfile::apply_args) **allow apply** (write). These are
///   appended **only** after the user explicitly approves a proposal; the
///   propose and answer paths never append them.
///
/// [`apply_supported`](FixProfile::apply_supported) is `false` for agents that
/// can be read/proposed against but have no drivable apply path at all (e.g.
/// Windsurf and generic VS Code have no headless CLI). For those, an apply
/// request must be refused, never spawned.
#[derive(Debug, Clone, Copy)]
pub struct FixProfile {
    /// Extra args that force PROPOSE-ONLY (read-only / plan). Empty = rely on
    /// the prompt plus omitting the apply args.
    pub propose_args: &'static [&'static str],
    /// Extra args that ALLOW APPLY (write). Appended ONLY after user approval.
    pub apply_args: &'static [&'static str],
    /// `false` = this agent can propose but cannot be driven to apply at all
    /// (no CLI). An apply request for such an agent must error, not spawn.
    pub apply_supported: bool,
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

    /// Inverse of [`to_agent_kind`](KindTag::to_agent_kind): map a runtime
    /// [`AgentKind`] to its registry tag, when the kind names a known row.
    /// `Other`/`Unknown` (and any future un-tagged kind) return `None`. This
    /// lets the drive layer resolve a row generically by kind without ever
    /// special-casing an agent by name.
    pub fn from_agent_kind(kind: &AgentKind) -> Option<Self> {
        match kind {
            AgentKind::ClaudeCode => Some(KindTag::ClaudeCode),
            AgentKind::Cursor => Some(KindTag::Cursor),
            AgentKind::Antigravity => Some(KindTag::Antigravity),
            AgentKind::Copilot => Some(KindTag::Copilot),
            AgentKind::Gemini => Some(KindTag::Gemini),
            AgentKind::Codex => Some(KindTag::Codex),
            AgentKind::Aider => Some(KindTag::Aider),
            AgentKind::Windsurf => Some(KindTag::Windsurf),
            AgentKind::VsCodeFork => Some(KindTag::VsCode),
            AgentKind::Other(_) | AgentKind::Unknown => None,
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
        jsonl_subdir: "projects",
        drive_command: &["claude", "-p", "{prompt}"],
        answer_args: &[],
        mcp_allow_flag: None,
        install: Some(InstallRecipe {
            method: InstallMethod::CurlScript,
            spec: "https://claude.ai/install.sh",
            verify_binary: "claude",
        }),
        fix: FixProfile {
            propose_args: &["--permission-mode", "plan"],
            apply_args: &["--permission-mode", "acceptEdits"],
            apply_supported: true,
        },
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
        jsonl_subdir: "projects",
        drive_command: &["cursor-agent", "-p", "{prompt}"],
        answer_args: &[],
        // Cursor auto-loads `~/.cursor/mcp.json`; in `-p` mode without `--force`
        // it proposes rather than applies, so MCP read-tools are not blanket
        // auto-approved. No documented scoped allow-flag for headless read-tool
        // trust exists, so we add none. NEEDS-LIVE-VERIFY (the trust-gate
        // behavior for read-only MCP tools in `-p` mode is unconfirmed).
        mcp_allow_flag: None,
        // NEVER `--plan`: Cursor's `--plan` flag is a known bug that writes
        // files. Propose = omit `--force` + rely on the prompt.
        install: Some(InstallRecipe {
            method: InstallMethod::CurlScript,
            spec: "https://cursor.com/install",
            verify_binary: "cursor-agent",
        }),
        fix: FixProfile {
            propose_args: &[],
            apply_args: &["--force"],
            apply_supported: true,
        },
    },
    AgentEntry {
        kind_tag: KindTag::Antigravity,
        display_name: "Antigravity",
        // `agy` is Antigravity 2.0's CLI (successor to gemini-cli); list it
        // first so discovery prefers it when installed. It stores sessions as
        // JSONL at
        // `~/.gemini/antigravity-cli/brain/<id>/.system_generated/logs/transcript.jsonl`
        // — wiring that session reader is a follow-up slice. NEEDS-LIVE-VERIFY
        // (`agy` may not be installed; drive still falls back to `gemini`).
        binary_candidates: &["agy", "antigravity", "gemini"],
        app_bundles: &["Antigravity.app"],
        app_dirs_windows: &["Antigravity"],
        data_dir_globs: &[".gemini/antigravity"],
        app_data_windows: &[],
        session_format: Some(SessionFormat::Protobuf),
        jsonl_subdir: "projects",
        drive_command: &["gemini", "-p", "{prompt}"],
        answer_args: &[],
        mcp_allow_flag: Some("--allowed-mcp-server-names"),
        // Antigravity drives through the `gemini` CLI, so it shares Gemini's
        // approval-mode flags.
        install: Some(InstallRecipe {
            method: InstallMethod::CurlScript,
            spec: "https://antigravity.google/cli/install.sh",
            verify_binary: "agy",
        }),
        fix: FixProfile {
            propose_args: &["--approval-mode", "plan"],
            apply_args: &["--approval-mode", "yolo"],
            apply_supported: true,
        },
    },
    AgentEntry {
        kind_tag: KindTag::Copilot,
        // The STANDALONE GitHub Copilot CLI (`copilot`, GA Feb 2026) — its own
        // binary + `~/.copilot/` store. This is NOT "Copilot inside VS Code":
        // that extension's data lives in VS Code's own directory and belongs to
        // the VS Code row. Pointing this row at VS Code's dir double-counted one
        // install as two agents reading the same files; it now reads only the
        // standalone CLI's store.
        display_name: "GitHub Copilot CLI",
        binary_candidates: &["copilot"],
        app_bundles: &[],
        app_dirs_windows: &[],
        data_dir_globs: &[".copilot"],
        app_data_windows: &[],
        // The Copilot CLI stores sessions as `~/.copilot/session-state/<id>/
        // events.jsonl` — JSONL, not VS Code's chatSessions JSON files.
        session_format: Some(SessionFormat::Jsonl),
        jsonl_subdir: "session-state",
        drive_command: &["copilot", "-p", "{prompt}"],
        answer_args: &[],
        // Copilot auto-denies tools headlessly unless allowed. Its scoped flag
        // is `--allow-tool='SERVER(tool)'` — it needs per-*tool* names, not bare
        // server names, so the registry's "flag + server-name list" mechanism
        // (which works for Gemini's `--allowed-mcp-server-names`) cannot drive
        // it. We therefore set None here and do NOT use the blanket, unsafe
        // `--allow-all-tools`. Per-tool scoping requires enumerating tool names.
        // NEEDS-LIVE-VERIFY
        // (https://docs.github.com/en/copilot/how-tos/copilot-cli/use-copilot-cli/allowing-tools).
        mcp_allow_flag: None,
        // Copilot has no native propose flag; propose is prompt-only. Apply
        // needs `--allow-all-tools` (without it `-p` stalls).
        install: Some(InstallRecipe {
            method: InstallMethod::NpmGlobal,
            spec: "@github/copilot",
            verify_binary: "copilot",
        }),
        fix: FixProfile {
            propose_args: &[],
            apply_args: &["--allow-all-tools"],
            apply_supported: true,
        },
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
        jsonl_subdir: "projects",
        drive_command: &["gemini", "-p", "{prompt}"],
        answer_args: &[],
        mcp_allow_flag: Some("--allowed-mcp-server-names"),
        install: Some(InstallRecipe {
            method: InstallMethod::NpmGlobal,
            spec: "@google/gemini-cli",
            verify_binary: "gemini",
        }),
        fix: FixProfile {
            propose_args: &["--approval-mode", "plan"],
            apply_args: &["--approval-mode", "yolo"],
            apply_supported: true,
        },
    },
    AgentEntry {
        kind_tag: KindTag::Codex,
        display_name: "Codex",
        binary_candidates: &["codex"],
        app_bundles: &[],
        app_dirs_windows: &[],
        data_dir_globs: &[".codex"],
        app_data_windows: &[],
        session_format: Some(SessionFormat::Jsonl),
        jsonl_subdir: "sessions",
        drive_command: &["codex", "exec", "{prompt}"],
        // Answer mode runs Codex with the read-safe sandbox: reads are allowed,
        // writes are blocked, and no approval prompt is raised (which would
        // stall a headless run). Flags DOC-CONFIRMED
        // (https://developers.openai.com/codex/cli/reference); the precise
        // read-only MCP/tool side-effect behavior is NEEDS-LIVE-VERIFY.
        answer_args: &["--sandbox", "read-only", "--ask-for-approval", "never"],
        mcp_allow_flag: None,
        install: Some(InstallRecipe {
            method: InstallMethod::NpmGlobal,
            spec: "@openai/codex",
            verify_binary: "codex",
        }),
        fix: FixProfile {
            propose_args: &["--sandbox", "read-only", "--ask-for-approval", "never"],
            apply_args: &[
                "--sandbox",
                "workspace-write",
                "--ask-for-approval",
                "never",
            ],
            apply_supported: true,
        },
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
        jsonl_subdir: "projects",
        drive_command: &["aider", "--message", "{prompt}"],
        answer_args: &[],
        mcp_allow_flag: None,
        install: None,
        fix: FixProfile {
            propose_args: &["--dry-run"],
            apply_args: &["--yes-always"],
            apply_supported: true,
        },
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
        jsonl_subdir: "projects",
        drive_command: &[],
        answer_args: &[],
        mcp_allow_flag: None,
        // No headless CLI to drive an apply — propose-capable only.
        install: None,
        fix: FixProfile {
            propose_args: &[],
            apply_args: &[],
            apply_supported: false,
        },
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
        jsonl_subdir: "projects",
        drive_command: &[],
        answer_args: &[],
        mcp_allow_flag: None,
        // No headless CLI to drive an apply — propose-capable only.
        install: None,
        fix: FixProfile {
            propose_args: &[],
            apply_args: &[],
            apply_supported: false,
        },
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

/// Look up the registry row for a [`KindTag`]. Always `Some` for the const
/// tags (every tag has exactly one row), but returned as `Option` so callers
/// stay total without an `unwrap`.
pub fn entry_for(kind: KindTag) -> Option<&'static AgentEntry> {
    REGISTRY.iter().find(|e| e.kind_tag == kind)
}

/// Resolve the [`FixProfile`] for an agent's [`KindTag`], reading it straight
/// off the registry row. Data-driven: the Fix lane reads this; it never names
/// an agent. Returns `None` only if a tag has no row (impossible for the const
/// table, but kept total).
pub fn fix_profile_for(kind: KindTag) -> Option<&'static FixProfile> {
    entry_for(kind).map(|e| &e.fix)
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
            "GitHub Copilot CLI",
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

    // ---- FixProfile (Fix button, slice F1) ------------------------------

    /// Parse a Fix arg list into ordered `(flag, value)` pairs. A token
    /// starting with `-` opens a flag; an immediately following non-dash token
    /// is its value. A dash token followed by another dash token (or EOL) is a
    /// value-less flag (e.g. `--force`, `--dry-run`). Mirrors how the CLIs
    /// these args target read their own flags.
    fn arg_pairs(args: &[&'static str]) -> Vec<(&'static str, Option<&'static str>)> {
        let mut out = Vec::new();
        let mut i = 0;
        while i < args.len() {
            let tok = args[i];
            if tok.starts_with('-') {
                let val = args.get(i + 1).copied().filter(|n| !n.starts_with('-'));
                i += if val.is_some() { 2 } else { 1 };
                out.push((tok, val));
            } else {
                // A bare value with no preceding flag (none in current data);
                // record it so a future stray token is still inspected.
                out.push((tok, None));
                i += 1;
            }
        }
        out
    }

    /// The set of tokens that *grant write* when moving from propose to apply:
    /// for each apply `(flag, value)`, the token is write-enabling when that
    /// flag is absent from propose, or present with a different value. Returns
    /// both the divergent flag and its value (when any). Posture-invariant
    /// tokens shared by both lists (e.g. the flag name `--permission-mode`, or
    /// Codex's `--ask-for-approval never`) are deliberately excluded.
    fn write_enabling_tokens(
        propose: &[(&'static str, Option<&'static str>)],
        apply: &[(&'static str, Option<&'static str>)],
    ) -> Vec<&'static str> {
        let mut out = Vec::new();
        for (flag, apply_val) in apply {
            let propose_entry = propose.iter().find(|(f, _)| f == flag);
            let diverges = match propose_entry {
                None => true,                                       // flag absent in propose
                Some((_, propose_val)) => propose_val != apply_val, // value changed
            };
            if diverges {
                // The value (if any) is the new write posture; the flag itself
                // is also new when it was absent from propose.
                if let Some(v) = apply_val {
                    out.push(*v);
                }
                if propose_entry.is_none() {
                    out.push(*flag);
                }
            }
        }
        out
    }

    #[test]
    fn test_no_write_enabling_token_appears_in_propose() {
        // Defense-in-depth, pair-aware: the tokens that switch an agent from
        // read to write (Cursor `--force`, Codex `workspace-write`, Claude
        // `acceptEdits`, …) must NEVER appear in the propose arg list. Tokens
        // that are identical across both postures (a shared flag name, or
        // Codex's invariant `--ask-for-approval never`) are not write-granting
        // and are allowed to repeat.
        for e in REGISTRY {
            let propose = arg_pairs(e.fix.propose_args);
            let apply = arg_pairs(e.fix.apply_args);
            let write_tokens = write_enabling_tokens(&propose, &apply);
            for tok in write_tokens {
                assert!(
                    !e.fix.propose_args.contains(&tok),
                    "{}: write-enabling token {tok:?} leaked into propose_args",
                    e.display_name
                );
            }
        }
    }

    #[test]
    fn test_writable_agents_actually_have_a_write_enabling_token() {
        // The pair-diff is meaningful: every apply-capable agent must expose at
        // least one write-enabling token, otherwise its apply posture is
        // indistinguishable from propose and the gate is a no-op.
        for e in REGISTRY {
            if e.fix.apply_supported {
                let propose = arg_pairs(e.fix.propose_args);
                let apply = arg_pairs(e.fix.apply_args);
                assert!(
                    !write_enabling_tokens(&propose, &apply).is_empty(),
                    "{}: apply posture has no write-enabling token over propose",
                    e.display_name
                );
            }
        }
    }

    #[test]
    fn test_propose_and_apply_args_are_never_identical_for_writable_agents() {
        // For an apply-capable agent the two postures must actually differ;
        // identical arg sets would make the apply gate a no-op (a propose run
        // would already carry write permission).
        for e in REGISTRY {
            if e.fix.apply_supported {
                assert_ne!(
                    e.fix.propose_args, e.fix.apply_args,
                    "{}: propose and apply args are identical",
                    e.display_name
                );
            }
        }
    }

    #[test]
    fn test_cursor_never_uses_plan_flag() {
        // Cursor's `--plan` is a known bug that writes files; it must never
        // appear in either arg set (propose = omit --force + prompt only).
        let cursor = row(KindTag::Cursor);
        assert!(!cursor.fix.propose_args.contains(&"--plan"));
        assert!(!cursor.fix.apply_args.contains(&"--plan"));
    }

    #[test]
    fn test_no_row_carries_plan_flag_anywhere() {
        // Stronger guard: the known-bad `--plan` flag belongs to no agent's
        // Fix profile, so a future copy/paste can't reintroduce it.
        for e in REGISTRY {
            assert!(
                !e.fix.propose_args.contains(&"--plan") && !e.fix.apply_args.contains(&"--plan"),
                "{}: --plan must never appear in a Fix profile",
                e.display_name
            );
        }
    }

    #[test]
    fn test_cli_less_agents_cannot_apply() {
        // Windsurf and VS Code have no headless CLI, so they can propose but
        // not be driven to apply.
        for kind in [KindTag::Windsurf, KindTag::VsCode] {
            assert!(
                !row(kind).fix.apply_supported,
                "{kind:?} has no CLI and must have apply_supported = false"
            );
        }
    }

    #[test]
    fn test_cli_agents_support_apply() {
        // Every agent with a non-empty drive command is apply-capable.
        for e in REGISTRY {
            if !e.drive_command.is_empty() {
                assert!(
                    e.fix.apply_supported,
                    "{}: drivable agents should support apply",
                    e.display_name
                );
            }
        }
    }

    #[test]
    fn test_apply_unsupported_rows_have_empty_apply_args() {
        // An agent that cannot apply must not carry stray apply args.
        for e in REGISTRY {
            if !e.fix.apply_supported {
                assert!(
                    e.fix.apply_args.is_empty(),
                    "{}: apply_supported=false but apply_args is non-empty",
                    e.display_name
                );
            }
        }
    }

    #[test]
    fn test_claude_fix_profile_values() {
        // Spot-check the exact data for one solid-propose agent.
        let claude = row(KindTag::ClaudeCode).fix;
        assert_eq!(claude.propose_args, &["--permission-mode", "plan"]);
        assert_eq!(claude.apply_args, &["--permission-mode", "acceptEdits"]);
        assert!(claude.apply_supported);
    }

    #[test]
    fn test_from_agent_kind_round_trips_known_tags() {
        // Every tag's owned kind maps back to the same tag (Antigravity and
        // generic VS Code included).
        for e in REGISTRY {
            let kind = e.kind_tag.to_agent_kind();
            assert_eq!(
                KindTag::from_agent_kind(&kind),
                Some(e.kind_tag),
                "round-trip failed for {}",
                e.display_name
            );
        }
    }

    #[test]
    fn test_from_agent_kind_none_for_other_and_unknown() {
        assert_eq!(KindTag::from_agent_kind(&AgentKind::Unknown), None);
        assert_eq!(
            KindTag::from_agent_kind(&AgentKind::Other("x".to_string())),
            None
        );
    }

    #[test]
    fn test_fix_profile_for_resolves_off_the_row() {
        // The lookup returns the same profile stored on the row.
        let via_helper = fix_profile_for(KindTag::Codex).expect("profile");
        let on_row = &row(KindTag::Codex).fix;
        assert_eq!(via_helper.propose_args, on_row.propose_args);
        assert_eq!(via_helper.apply_args, on_row.apply_args);
        assert_eq!(via_helper.apply_supported, on_row.apply_supported);
    }
}
