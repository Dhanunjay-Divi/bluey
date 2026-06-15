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
    /// How [`mcp_allow_flag`]'s value(s) are SHAPED for this agent's CLI (see
    /// [`McpAllowStyle`]). `None` only when `mcp_allow_flag` is `None`. Lets each
    /// CLI's allow-list syntax stay data, not a code branch.
    pub mcp_allow_style: Option<McpAllowStyle>,
    /// Review-gated "Fix" profile: the extra args that switch this agent
    /// between propose-only and apply (see [`FixProfile`]).
    pub fix: FixProfile,
    /// Ordered list of **fallback models** to try when a drive fails because the
    /// agent's configured model is not allowed for the user's account/plan (the
    /// "model-blocked" case — see [`crate::model_resolve`]). The model-fallback
    /// resolver, on a `ModelBlocked` error, proposes retrying with the FIRST
    /// entry here that isn't the already-blocked model, applied via
    /// [`model_flag`](AgentEntry::model_flag).
    ///
    /// This is the **mechanism, expressed as data** — never an `if agent == …`
    /// branch. The concrete model IDs are best-effort and shift over time
    /// (OpenAI's per-plan allowlist in particular is in flux as of 2026-06), so
    /// the values are documented as **NEEDS-LIVE-VERIFY**: the resolver tries
    /// them in order and falls through to the BYOT (API-key) proposal when none
    /// works. Empty for agents with no model flag or no known safe fallback
    /// (the resolver then proposes BYOT or falls back to today's honest error).
    pub fallback_models: &'static [&'static str],
    /// The CLI flag this agent uses to **override the model for one run**, as a
    /// token template ending in the model name. Codex accepts `-m <MODEL>`
    /// (VERIFIED via `codex exec --help`: `-m, --model <MODEL>`); the `-c
    /// model="X"` config-override form also works but `-m` is the simplest
    /// single-token-pair. The resolver emits `[model_flag, <fallback>]` as two
    /// argv entries. `None` for agents that don't take a per-run model flag, in
    /// which case [`fallback_models`](AgentEntry::fallback_models) is unused and
    /// the resolver goes straight to the BYOT proposal (or honest error).
    pub model_flag: Option<&'static str>,
    /// How to PROACTIVELY install this agent's CLI when it's missing (so a user
    /// with only the GUI app becomes drivable). `None` for agents with no known
    /// official CLI installer. Recipes are vetted, official sources only — never
    /// an arbitrary string — and are always run consent-gated + verified.
    pub install: Option<InstallRecipe>,
    /// The agent's OWN login invocation (program + args), as DATA — so the
    /// auth/login resolver can TRIGGER it (consent-gated) when a drive error
    /// shows the CLI is installed but not signed in, instead of telling the
    /// user to type a command. Verified via `<bin> --help` on a real machine:
    /// cursor-agent=`login`, codex=`login`, claude=`setup-token` (the long-lived
    /// token flow; there is no `claude login` subcommand). `None` for agents
    /// with no non-interactive CLI login (Gemini authenticates on first run /
    /// via `GEMINI_API_KEY`; VS Code / Windsurf / Aider have no login CLI).
    /// Read by [`crate::auth_resolve`]; never special-cased by name in logic.
    pub login_command: Option<&'static [&'static str]>,
    /// How to make this agent's login surface a device-code / printed URL
    /// instead of popping a browser, as DATA — so the trigger works headlessly
    /// (Bluey-as-overlay) without any per-agent `if` in the resolver. Verified
    /// per CLI: cursor-agent reads a `NO_OPEN_BROWSER` env var; codex takes a
    /// `--device-auth` flag; claude's `setup-token` is interactive with no
    /// no-browser switch. [`LoginAuth::None`] for agents with no CLI login.
    pub login_auth: LoginAuth,
    /// The agent's OWN "list my MCP servers" CLI invocation — the argv AFTER the
    /// binary (e.g. `&["mcp", "list"]`) — as DATA, so the validation matrix can
    /// run it to confirm the agent can actually SEE/launch its MCP tools (a
    /// LIVE Level-2 signal), not merely that they sit in a config file (Level
    /// 1). This is **read-only and NON-quota**: the command lists/health-checks
    /// connectors; it never drives the model. VERIFIED live per CLI (2026-06):
    /// claude/gemini/copilot=`mcp list`; cursor enumerates per-server TOOLS via
    /// `mcp list-tools <server>` (see [`mcp_list_tools_per_server`]); antigravity
    /// drives through `gemini`, so it shares `gemini mcp list`. `None` for agents
    /// with no such CLI (Aider/Windsurf/VS Code, and the Claude-app index rows
    /// that are not themselves CLI-driven). Consumed by [`crate::mcp_tools`];
    /// never special-cased by agent name in logic.
    ///
    /// SECURITY: only the `mcp list` / `mcp list-tools` forms are listed here —
    /// NEVER `mcp get <name>`, which dumps the connector's env (API keys) in
    /// plaintext. The list forms were verified to print server names + health
    /// only, no secret values.
    pub mcp_list_command: Option<&'static [&'static str]>,
    /// When `true`, [`mcp_list_command`] is the *prefix* of a PER-SERVER tool
    /// enumeration: the runner first reads the agent's configured server names
    /// (the Level-1 config path) and then runs `<mcp_list_command> <server>` for
    /// each, collecting the real TOOL names. Only Cursor needs this — its
    /// `cursor-agent mcp list-tools <server>` returns the actual tools of one
    /// server (the richest per-tool signal). `false` for the server-level
    /// `mcp list` agents (claude/gemini/copilot), whose single command lists all
    /// servers at once.
    pub mcp_list_tools_per_server: bool,
}

/// How an agent's CLI login is made headless-friendly (device-code / printed
/// URL instead of an opened browser) — pure data consumed by
/// [`crate::auth_resolve::run_login`], so the trigger never hardcodes a
/// per-agent env var or flag. Verified against the real CLIs (2026-06).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoginAuth {
    /// Set an environment variable to suppress the browser and print the
    /// device-code/URL — e.g. cursor-agent's `NO_OPEN_BROWSER=1`.
    NoBrowserEnv {
        name: &'static str,
        value: &'static str,
    },
    /// Append extra args to the login command for a device-code flow — e.g.
    /// codex's `login --device-auth`.
    DeviceCodeArgs(&'static [&'static str]),
    /// Plain interactive login, no documented no-browser/device-code mode
    /// (e.g. claude `setup-token`). The trigger runs the command as-is.
    InteractiveOnly,
    /// No CLI login flow at all (paired with `login_command: None`). Inert.
    None,
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

/// How an agent's CLI expresses "auto-approve these MCP servers' tools in
/// headless mode" — the SHAPE of the allow-list args, since each CLI differs.
/// The drive layer reads the agent's own configured MCP server names and renders
/// them per this style. Keeping the shape as data (not an `if agent == …`) is
/// what lets a new agent be a registry row, not a code branch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpAllowStyle {
    /// One flag + ONE comma-joined value: `--flag a,b,c`. The gemini-family CLIs
    /// (`--allowed-mcp-server-names`) need the comma form — a space-separated
    /// multi-value collides with `-p {prompt}` (verified live).
    ServerNameCsv,
    /// Claude Code: `--allowed-tools "mcp__a mcp__b"` — approval is by TOOL-NAME
    /// pattern, and an MCP server `a` exposes tools under the `mcp__a` prefix.
    /// Space-separated tool patterns in a single value.
    ClaudeToolPattern,
    /// GitHub Copilot CLI: a repeated `--allow-tool <server>` per server (the
    /// flag takes one tool/server and may be given multiple times); approves the
    /// named MCP servers' tools without the interactive permission prompt.
    CopilotAllowTool,
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
    ClaudeCodeApp,
    ClaudeCodeAgent,
    Cursor,
    Antigravity,
    /// Cursor's cloud-hosted Background Agents. Distinct from `Cursor` (the
    /// local IDE/CLI); routed through [`crate::cloud::cursor`].
    CursorCloud,
    /// GitHub Copilot Coding Agent (cloud-hosted, task-shaped). Distinct
    /// from `Copilot` (the local standalone CLI); routed through
    /// [`crate::cloud::copilot`].
    CopilotCloud,
    Copilot,
    Gemini,
    Codex,
    /// OpenAI's cloud-hosted Codex Cloud (task-shaped). Distinct from `Codex`
    /// (the local CLI); routed through [`crate::cloud::codex_cloud`].
    CodexCloud,
    /// Anthropic's Claude Managed Agents (session-shaped, BYOT API key).
    /// Distinct from `ClaudeCode` (the local CLI); routed through
    /// [`crate::cloud::anthropic`]. The registry row carries the
    /// `managed-agents-2026-04-01` beta header as DATA in the cloud-registry
    /// table (`crate::cloud::registry::CLOUD_REGISTRY`).
    AnthropicCloud,
    Aider,
    Windsurf,
    VsCode,
    /// Google Antigravity (Cloud) — the Managed Agents surface of the Gemini
    /// API (session/turn-shaped, BYOT Gemini API key). Distinct from the local
    /// `Antigravity` (the bundled `agy`/`gemini` CLI); routed through
    /// [`crate::cloud::antigravity_cloud`]. The registry row lives in the
    /// cloud-registry table (`crate::cloud::registry::CLOUD_REGISTRY`); this tag
    /// is the only thing in the local table. The cloud row carries the
    /// `Api-Revision: 2026-05-20` header and the `x-goog-api-key` auth as DATA.
    AntigravityCloud,
    /// Google's Gemini Managed Agents — the generic cloud surface of the Gemini
    /// API (turn-shaped, BYOT Gemini API key). Distinct from the local `Gemini`
    /// (the `gemini` CLI); routed through [`crate::cloud::gemini_cloud`]. Shares
    /// the Interactions API endpoint with the sibling `AntigravityCloud` row but
    /// is a distinct vendor identity. The cloud row (in
    /// `crate::cloud::registry::CLOUD_REGISTRY`) carries the
    /// `Api-Revision: 2026-05-20` header and the `x-goog-api-key` auth as DATA.
    GeminiCloud,
}

impl KindTag {
    /// Resolve to the owned [`AgentKind`]. Generic VS Code maps to `VsCodeFork`
    /// (the registry row is the "known" VS Code, but it shares the fork code
    /// path with unrecognized forks).
    pub fn to_agent_kind(self) -> AgentKind {
        match self {
            KindTag::ClaudeCode => AgentKind::ClaudeCode,
            KindTag::ClaudeCodeApp => AgentKind::ClaudeCodeApp,
            KindTag::ClaudeCodeAgent => AgentKind::ClaudeCodeAgent,
            KindTag::Cursor => AgentKind::Cursor,
            KindTag::Antigravity => AgentKind::Antigravity,
            KindTag::CursorCloud => AgentKind::CursorCloud,
            KindTag::CopilotCloud => AgentKind::CopilotCloud,
            KindTag::Copilot => AgentKind::Copilot,
            KindTag::Gemini => AgentKind::Gemini,
            KindTag::Codex => AgentKind::Codex,
            KindTag::CodexCloud => AgentKind::CodexCloud,
            KindTag::AnthropicCloud => AgentKind::AnthropicCloud,
            KindTag::Aider => AgentKind::Aider,
            KindTag::Windsurf => AgentKind::Windsurf,
            KindTag::VsCode => AgentKind::VsCodeFork,
            KindTag::AntigravityCloud => AgentKind::AntigravityCloud,
            KindTag::GeminiCloud => AgentKind::GeminiCloud,
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
            AgentKind::ClaudeCodeApp => Some(KindTag::ClaudeCodeApp),
            AgentKind::ClaudeCodeAgent => Some(KindTag::ClaudeCodeAgent),
            AgentKind::Cursor => Some(KindTag::Cursor),
            AgentKind::Antigravity => Some(KindTag::Antigravity),
            AgentKind::CursorCloud => Some(KindTag::CursorCloud),
            AgentKind::CopilotCloud => Some(KindTag::CopilotCloud),
            AgentKind::Copilot => Some(KindTag::Copilot),
            AgentKind::Gemini => Some(KindTag::Gemini),
            AgentKind::Codex => Some(KindTag::Codex),
            AgentKind::CodexCloud => Some(KindTag::CodexCloud),
            AgentKind::AnthropicCloud => Some(KindTag::AnthropicCloud),
            AgentKind::Aider => Some(KindTag::Aider),
            AgentKind::Windsurf => Some(KindTag::Windsurf),
            AgentKind::VsCodeFork => Some(KindTag::VsCode),
            AgentKind::AntigravityCloud => Some(KindTag::AntigravityCloud),
            AgentKind::GeminiCloud => Some(KindTag::GeminiCloud),
            AgentKind::Other(_) | AgentKind::Unknown => None,
        }
    }
}

/// The known-agent table. Order is detection priority (earlier rows win on a
/// tie when the same evidence could match multiple rows).
pub const REGISTRY: &[AgentEntry] = &[
    AgentEntry {
        kind_tag: KindTag::ClaudeCode,
        // No account/plan model-block has been observed for Claude Code, so no
        // fallback ordering is asserted. Claude's CLI does accept `--model`, so
        // the mechanism is wired (a future plan-gated model could populate the
        // list) but the safe-fallback values stay empty until proven.
        fallback_models: &[],
        model_flag: Some("--model"),
        // Claude's CLI login is `claude setup-token` (long-lived token; there is
        // no `claude login` subcommand). Interactive, no no-browser switch.
        login_command: Some(&["claude", "setup-token"]),
        login_auth: LoginAuth::InteractiveOnly,
        // "(CLI)" disambiguates the terminal Claude Code from the Claude app's
        // "(App)" / "(Agent)" rows below, which share the same engine + store.
        display_name: "Claude Code (CLI)",
        // `claude mcp list` launches each configured server and prints its health
        // (`<name>: <cmd> - ✓ Connected`). Server-level, with a live connection
        // check. VERIFIED live. NEVER `mcp get` (leaks the server env / API key).
        mcp_list_command: Some(&["mcp", "list"]),
        mcp_list_tools_per_server: false,
        binary_candidates: &["claude"],
        app_bundles: &["Claude.app"],
        app_dirs_windows: &["Claude"],
        data_dir_globs: &[".claude"],
        app_data_windows: &[],
        session_format: Some(SessionFormat::Jsonl),
        jsonl_subdir: "projects",
        drive_command: &["claude", "-p", "{prompt}"],
        answer_args: &[],
        // Claude approves MCP tools by TOOL-NAME pattern via `--allowed-tools`
        // (verified `claude --help`). An MCP server `<s>` exposes tools under the
        // `mcp__<s>` prefix, so the drive layer renders `--allowed-tools
        // "mcp__<s> …"`. Without it, headless `claude -p` asks for permission and
        // answers from training data instead of firing the tool (caught live by
        // the MCP matrix's no-live-data check). This approves only the agent's
        // own configured MCP servers — file/shell write tools stay gated.
        mcp_allow_flag: Some("--allowed-tools"),
        mcp_allow_style: Some(McpAllowStyle::ClaudeToolPattern),
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
    // The Claude desktop app's Code mode. Same engine + transcript store as the
    // CLI above (driven by `claude`), but its sessions are indexed in the app's
    // own store with rich pre-computed titles. `jsonl_subdir` names the app's
    // session-index subdir under `Library/Application Support/Claude`; the
    // ClaudeAppIndex reader follows each entry's `cliSessionId` into the shared
    // `~/.claude/projects/*.jsonl` for the body.
    AgentEntry {
        kind_tag: KindTag::ClaudeCodeApp,
        // Same `claude` engine as the CLI row — shares its (empty) fallback set.
        fallback_models: &[],
        model_flag: Some("--model"),
        // Driven through the same `claude` CLI, so it shares its login command.
        login_command: Some(&["claude", "setup-token"]),
        login_auth: LoginAuth::InteractiveOnly,
        display_name: "Claude Code (App)",
        // GUI-surface index row (not itself CLI-driven in the matrix; classified
        // GUI). The CLI row above already provides the live `claude mcp list`
        // signal, so this row stays config-only (Level-1 fallback).
        mcp_list_command: None,
        mcp_list_tools_per_server: false,
        // No separate binary — driven through the same `claude` CLI.
        binary_candidates: &["claude"],
        app_bundles: &["Claude.app"],
        app_dirs_windows: &["Claude"],
        data_dir_globs: &["Library/Application Support/Claude"],
        app_data_windows: &["Claude"],
        session_format: Some(SessionFormat::ClaudeAppIndex),
        jsonl_subdir: "claude-code-sessions",
        drive_command: &["claude", "-p", "{prompt}"],
        answer_args: &[],
        mcp_allow_flag: None,
        mcp_allow_style: None,
        // Install handled by the CLI row; the app is GUI-installed out of band.
        install: None,
        fix: FixProfile {
            propose_args: &["--permission-mode", "plan"],
            apply_args: &["--permission-mode", "acceptEdits"],
            apply_supported: true,
        },
    },
    // The Claude desktop app's agent (cowork) mode — same engine/store, a
    // different session-index subdir.
    AgentEntry {
        kind_tag: KindTag::ClaudeCodeAgent,
        // Same `claude` engine as the CLI row — shares its (empty) fallback set.
        fallback_models: &[],
        model_flag: Some("--model"),
        // Same `claude` CLI engine → same login command.
        login_command: Some(&["claude", "setup-token"]),
        login_auth: LoginAuth::InteractiveOnly,
        display_name: "Claude Code (Agent)",
        // GUI-surface index row (see the App row); config-only fallback.
        mcp_list_command: None,
        mcp_list_tools_per_server: false,
        binary_candidates: &["claude"],
        app_bundles: &["Claude.app"],
        app_dirs_windows: &["Claude"],
        data_dir_globs: &["Library/Application Support/Claude"],
        app_data_windows: &["Claude"],
        session_format: Some(SessionFormat::ClaudeAppIndex),
        jsonl_subdir: "local-agent-mode-sessions",
        drive_command: &["claude", "-p", "{prompt}"],
        answer_args: &[],
        mcp_allow_flag: None,
        mcp_allow_style: None,
        install: None,
        fix: FixProfile {
            propose_args: &["--permission-mode", "plan"],
            apply_args: &["--permission-mode", "acceptEdits"],
            apply_supported: true,
        },
    },
    AgentEntry {
        kind_tag: KindTag::Cursor,
        // No account/plan model-block observed for Cursor; mechanism left inert.
        fallback_models: &[],
        model_flag: None,
        // `cursor-agent login` opens a browser; `NO_OPEN_BROWSER` prints the
        // device-code/URL instead (verified via `cursor-agent login --help`).
        login_command: Some(&["cursor-agent", "login"]),
        login_auth: LoginAuth::NoBrowserEnv {
            name: "NO_OPEN_BROWSER",
            value: "1",
        },
        display_name: "Cursor",
        // Cursor enumerates the actual TOOLS of one server via
        // `cursor-agent mcp list-tools <server>` (e.g. `perplexity` →
        // perplexity_ask/_reason/_research/_search) — the richest per-tool live
        // signal. The runner reads the configured server names first, then runs
        // this prefix per server (see `mcp_list_tools_per_server`). VERIFIED live.
        mcp_list_command: Some(&["mcp", "list-tools"]),
        mcp_list_tools_per_server: true,
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
        mcp_allow_style: None,
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
        // Drives through the `gemini` CLI; no model-block case observed.
        fallback_models: &[],
        model_flag: None,
        // Drives through the `gemini` CLI, which has no non-interactive login
        // subcommand (it authenticates on first run / via GEMINI_API_KEY).
        login_command: None,
        login_auth: LoginAuth::None,
        display_name: "Antigravity",
        // Antigravity drives through the `gemini` CLI, so it shares Gemini's
        // server-level `gemini mcp list` (server name + connected health).
        mcp_list_command: Some(&["mcp", "list"]),
        mcp_list_tools_per_server: false,
        // Sessions are read from Antigravity's plaintext conversation INDEX
        // (`agyhub_summaries_proto.pb`) — it lists ALL conversations with their
        // real titles + projects (the desktop UI lists from this same index).
        // Bodies are resolved per-session by the reader: `brain/<id>/…/
        // transcript.jsonl` (richest) or `conversations/<id>.db` (SQLite);
        // encrypted `conversations/<id>.pb` bodies are list-only. The earlier
        // brain-only JSONL approach saw ~6 of ~105 conversations.
        // `agy` is Antigravity 2.0's CLI (successor to gemini-cli); listed first
        // so discovery prefers it for driving when installed, else `gemini`.
        binary_candidates: &["agy", "antigravity", "gemini"],
        app_bundles: &["Antigravity.app"],
        app_dirs_windows: &["Antigravity"],
        data_dir_globs: &[".gemini/antigravity"],
        app_data_windows: &[],
        session_format: Some(SessionFormat::AntigravityIndex),
        // The AntigravityIndex reader works from the store root, not a subdir;
        // `jsonl_subdir` is unused for this format (kept non-empty for the
        // struct; the reader never reads it).
        jsonl_subdir: "",
        drive_command: &["gemini", "-p", "{prompt}"],
        answer_args: &[],
        mcp_allow_flag: Some("--allowed-mcp-server-names"),
        mcp_allow_style: Some(McpAllowStyle::ServerNameCsv),
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
        // No account/plan model-block observed for the Copilot CLI.
        fallback_models: &[],
        model_flag: None,
        // The standalone `copilot` CLI has no `login` subcommand (verified via
        // `copilot --help`): it authenticates via `GH_TOKEN`/`GITHUB_TOKEN` or an
        // interactive in-REPL `/login`. No non-interactive CLI login to trigger.
        login_command: None,
        login_auth: LoginAuth::None,
        // The STANDALONE GitHub Copilot CLI (`copilot`, GA Feb 2026) — its own
        // binary + `~/.copilot/` store. This is NOT "Copilot inside VS Code":
        // that extension's data lives in VS Code's own directory and belongs to
        // the VS Code row. Pointing this row at VS Code's dir double-counted one
        // install as two agents reading the same files; it now reads only the
        // standalone CLI's store.
        display_name: "GitHub Copilot CLI",
        // `copilot mcp list` prints `User servers:` + `  <name> (local)`.
        // Server-level. Copilot requires Node ≥ 24; the runner drives this under
        // a satisfying runtime via the shared per-spawn PATH (same
        // `runtime_resolve` path the drive layer uses), so the list runs even
        // when the active node is older. VERIFIED live under node 24.
        mcp_list_command: Some(&["mcp", "list"]),
        mcp_list_tools_per_server: false,
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
        // Copilot auto-denies tools headlessly unless allowed. Its scoped flag is
        // a repeated `--allow-tool <name>` (verified `copilot --help`), which
        // approves the named MCP server's tools without the permission prompt.
        // The drive layer emits one `--allow-tool <server>` per configured MCP
        // server (CopilotAllowTool style) — scoped to the agent's own servers, so
        // we never use the blanket, unsafe `--allow-all-tools`. Without it,
        // headless `copilot -p` answers "I don't have access to web search tools"
        // instead of firing the connector (caught live by the MCP matrix).
        mcp_allow_flag: Some("--allow-tool"),
        mcp_allow_style: Some(McpAllowStyle::CopilotAllowTool),
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
        // No account/plan model-block observed for Gemini CLI (its capacity
        // errors are 429 "no capacity", a transient class, not a plan block).
        fallback_models: &[],
        model_flag: Some("--model"),
        // The `gemini` CLI has no `login`/`auth` subcommand (verified via
        // `gemini --help`): it authenticates interactively on first launch or
        // via `GEMINI_API_KEY`. Nothing non-interactive to trigger.
        login_command: None,
        login_auth: LoginAuth::None,
        display_name: "Gemini CLI",
        // `gemini mcp list` prints `✓ <name>: <cmd> (stdio) - Connected`.
        // Server-level + connected health. VERIFIED live.
        mcp_list_command: Some(&["mcp", "list"]),
        mcp_list_tools_per_server: false,
        binary_candidates: &["gemini"],
        app_bundles: &[],
        app_dirs_windows: &[],
        data_dir_globs: &[".gemini"],
        app_data_windows: &[],
        // Gemini CLI auto-persists every conversation (no flag) as JSONL under
        // `~/.gemini/tmp/<project-token>/chats/session-*.jsonl` (verified: 76
        // sessions on a real machine). The store was previously mis-pointed at
        // `projects` with no format, so Gemini listed 0 sessions. The shared
        // JSONL reader already understands the per-line `user`/`gemini`/`$set`/
        // header shapes; it recurses to the `chats/` dir within MAX_DEPTH.
        session_format: Some(SessionFormat::Jsonl),
        jsonl_subdir: "tmp",
        drive_command: &["gemini", "-p", "{prompt}"],
        answer_args: &[],
        mcp_allow_flag: Some("--allowed-mcp-server-names"),
        mcp_allow_style: Some(McpAllowStyle::ServerNameCsv),
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
        // Model-fallback ordering for the ChatGPT-account model-block (see
        // `crate::model_resolve`). As of 2026-06-02 OpenAI restricts newer Codex
        // models for ChatGPT-subscription accounts (they require an OpenAI API
        // key); the real error is "The '<model>' model is not supported when
        // using Codex with a ChatGPT account." The mechanism: on that error,
        // retry with the first entry below that isn't the already-blocked model,
        // via `model_flag` (`codex exec -m <MODEL>`).
        //
        // NEEDS-LIVE-VERIFY: the per-plan allowlist is in flux. On the test
        // machine (ChatGPT-account auth, no API key) EVERY candidate below was
        // ALSO blocked with the same error — i.e. the honest verdict for that
        // account is BYOT-only (connect an OpenAI API key). These values are the
        // best-effort ordering for accounts where at least one still works; the
        // resolver tries them in order and, when none does, falls through to the
        // BYOT (API-key) proposal. Ordered most-capable → most-available.
        fallback_models: &["gpt-5.1-codex", "gpt-5-codex", "gpt-5.1", "o4-mini"],
        // `codex exec` accepts `-m <MODEL>` (VERIFIED via `codex exec --help`:
        // `-m, --model <MODEL>`). The `-c model="X"` config-override form also
        // works on both `exec` and `exec resume`; `-m` is the simplest pair.
        model_flag: Some("-m"),
        // `codex login` opens a browser; `--device-auth` runs the headless
        // device-code flow instead (verified via `codex login --help`).
        login_command: Some(&["codex", "login"]),
        login_auth: LoginAuth::DeviceCodeArgs(&["--device-auth"]),
        display_name: "Codex",
        // No verified non-quota `codex mcp list` equivalent, so Codex stays on
        // the Level-1 config read (the matrix falls back automatically). Left
        // `None` rather than guessing a command that might drive the model.
        mcp_list_command: None,
        mcp_list_tools_per_server: false,
        binary_candidates: &["codex"],
        app_bundles: &[],
        app_dirs_windows: &[],
        data_dir_globs: &[".codex"],
        app_data_windows: &[],
        // The CLI rollouts (`~/.codex/sessions/YYYY/MM/DD/rollout-*.jsonl`,
        // plaintext JSONL) are the canonical, legitimately-readable Codex surface
        // (204 here). The ChatGPT DESKTOP app keeps a SEPARATE store at
        // `~/Library/Application Support/com.openai.chat/conversations-v3-*/*.data`
        // (~81 GUI conversations, DISJOINT from the CLI set — desktop ids are
        // UUIDv4, rollouts are UUIDv7, 0 overlap), but Bluey **intentionally does
        // NOT read it**, and this is the correct call, not a limitation:
        //   • The `.data` blobs are encrypted via Electron `safeStorage`; the key
        //     is a macOS Keychain item whose ACL is bound to OpenAI's Team ID
        //     (`2DC432GLL2`). A process not signed with that Team ID cannot read
        //     the key WITHOUT a per-access login-password prompt — unusable for a
        //     passive copilot — and the only prompt-free route is injecting into
        //     the signed ChatGPT app, a security bypass Bluey must never do.
        //   • OpenAI ENCRYPTED this deliberately (a July-2024 fix after the app
        //     was found storing chats in plaintext). Reading it would defeat a
        //     control they added on purpose — wrong on ethics, not just feasibility.
        //   • The only legitimate path is the app's own user-initiated Export
        //     (Settings → Data Controls), which yields a separate `conversations.json`
        //     — a manual action, not a live store Bluey can read autonomously.
        // Verified 2026-06-15 (web + on-disk + keychain-ACL analysis). Same posture
        // as Antigravity's encrypted per-conversation `.pb` bodies.
        session_format: Some(SessionFormat::Jsonl),
        jsonl_subdir: "sessions",
        drive_command: &["codex", "exec", "{prompt}"],
        // Answer mode runs Codex with the read-safe sandbox: reads are allowed,
        // writes are blocked, and no approval prompt is raised (which would
        // stall a headless run). Expressed as `-c` CONFIG OVERRIDES, not the
        // `--sandbox`/`--ask-for-approval` flags, because the `exec resume`
        // subcommand does NOT accept those flags (VERIFIED LIVE: `exec resume`
        // errors `unexpected argument '--sandbox'`), whereas `-c key=value` is
        // accepted by BOTH `exec` and `exec resume`. Same posture, one form.
        answer_args: &[
            "-c",
            "sandbox_mode=\"read-only\"",
            "-c",
            "approval_policy=\"never\"",
        ],
        mcp_allow_flag: None,
        mcp_allow_style: None,
        install: Some(InstallRecipe {
            method: InstallMethod::NpmGlobal,
            spec: "@openai/codex",
            verify_binary: "codex",
        }),
        fix: FixProfile {
            propose_args: &[
                "-c",
                "sandbox_mode=\"read-only\"",
                "-c",
                "approval_policy=\"never\"",
            ],
            apply_args: &[
                "-c",
                "sandbox_mode=\"workspace-write\"",
                "-c",
                "approval_policy=\"never\"",
            ],
            apply_supported: true,
        },
    },
    AgentEntry {
        kind_tag: KindTag::Aider,
        // Aider is model-agnostic (BYO key already); no plan model-block class.
        fallback_models: &[],
        model_flag: Some("--model"),
        // Aider authenticates via provider env vars (OPENAI_API_KEY, etc.), not
        // a login subcommand. Nothing to trigger.
        login_command: None,
        login_auth: LoginAuth::None,
        display_name: "Aider",
        // No MCP-list CLI (Aider has no MCP surface here); config-only.
        mcp_list_command: None,
        mcp_list_tools_per_server: false,
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
        mcp_allow_style: None,
        install: None,
        fix: FixProfile {
            propose_args: &["--dry-run"],
            apply_args: &["--yes-always"],
            apply_supported: true,
        },
    },
    AgentEntry {
        kind_tag: KindTag::Windsurf,
        // No headless CLI to drive, so no model flag / fallback applies.
        fallback_models: &[],
        model_flag: None,
        // GUI-only auth (the IDE handles sign-in); no headless login CLI.
        login_command: None,
        login_auth: LoginAuth::None,
        display_name: "Windsurf",
        // No headless CLI to enumerate MCP; config-only fallback.
        mcp_list_command: None,
        mcp_list_tools_per_server: false,
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
        mcp_allow_style: None,
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
        // No headless CLI to drive, so no model flag / fallback applies.
        fallback_models: &[],
        model_flag: None,
        // GUI-only auth (the editor handles sign-in); no headless login CLI.
        login_command: None,
        login_auth: LoginAuth::None,
        display_name: "VS Code",
        // No headless CLI to enumerate MCP; config-only fallback.
        mcp_list_command: None,
        mcp_list_tools_per_server: false,
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
        mcp_allow_style: None,
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

/// The human display name for an [`AgentKind`], from the registry — the single
/// source of truth. Generic: used to build session fallback labels etc. without
/// hardcoding an agent name anywhere else. `Other(label)` returns its own label;
/// `Unknown` and un-tagged kinds return `None`.
pub fn display_name_for(kind: &AgentKind) -> Option<String> {
    if let AgentKind::Other(label) = kind {
        return Some(label.clone());
    }
    let tag = KindTag::from_agent_kind(kind)?;
    // Try the local registry first (covers the LOCAL kinds: ClaudeCode,
    // Cursor IDE, Copilot CLI, …). If absent, fall back to the CLOUD
    // registry — cloud-only kinds (CursorCloud, …) live there.
    entry_for(tag)
        .map(|e| e.display_name.to_string())
        .or_else(|| {
            crate::cloud::registry::cloud_entry_for(tag).map(|e| e.display_name.to_string())
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_has_expected_agents() {
        let names: Vec<_> = REGISTRY.iter().map(|e| e.display_name).collect();
        for expected in [
            "Claude Code (CLI)",
            "Claude Code (App)",
            "Claude Code (Agent)",
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
                // A `-c key=value` config override is keyed by its CONFIG KEY,
                // not the repeated `-c` flag — otherwise two `-c` pairs collide
                // and the divergence check misfires (e.g. comparing apply's
                // `approval_policy` against propose's `sandbox_mode`). Split the
                // value into key + value so each config key is its own pair.
                if tok == "-c" || tok == "--config" {
                    if let Some(kv) = val {
                        match kv.split_once('=') {
                            Some((key, value)) => out.push((key, Some(value))),
                            None => out.push((kv, None)),
                        }
                        continue;
                    }
                }
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
