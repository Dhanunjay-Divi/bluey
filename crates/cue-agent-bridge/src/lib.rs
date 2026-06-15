//! `cue-agent-bridge` — discover and read from the user's already-installed
//! coding agents (Claude Code, Cursor, Antigravity, Copilot, Gemini, Codex, …).
//!
//! Slice 1 scope: **read-only discovery + crate foundation only.** This crate
//! finds which agents are installed, reads their MCP connector config (shape
//! and auth tier only — never secrets), and computes a coarse [`Capability`].
//!
//! Explicitly out of scope for this slice (lands later):
//! - Driving an agent / `ask()` (Slice 2).
//! - Session-store decoders that return real [`Transcript`]s (Slice 3).
//! - Daemon wiring and any subprocess execution.
//!
//! Every filesystem operation here is read-only and fail-soft: a malformed or
//! inaccessible path for one agent degrades that agent and discovery continues.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

pub mod adaptive;
pub mod auth_resolve;
pub mod capability;
pub mod cloud;
pub mod connectors;
pub mod discover;
pub mod drive;
pub mod fix;
pub mod mcp_probe;
pub mod mcp_tools;
pub mod model_resolve;
pub mod prove;
pub mod prove_drive;
pub mod provision;
pub mod registry;
pub mod runtime_resolve;
pub mod sessions;
pub mod titler;

pub use capability::compute_capability;
pub use connectors::read_connectors;
pub use discover::{discover_agents, discover_in_home, probe_sqlite_store};
// Local-CLI drive (unchanged surface) — local agents call this directly.
pub use drive::{drive as drive_cli, AnswerChunk, AnswerStream, Question};
pub use fix::{fix_apply_prompt, fix_proposal_prompt, parse_fix_proposal, FixProposal};
pub use sessions::{reader_for, SessionReader};

/// Drive an agent for one question.
///
/// Generic dispatcher: reads the agent's registry tag and chooses between
/// the local-CLI drive ([`drive_cli`]) and the cloud-vendor task drive
/// ([`cloud::drive_cloud`]) **without naming a vendor**. Adding a new
/// vendor adds a row to `cloud::CLOUD_REGISTRY`; this function picks it up
/// automatically.
///
/// The daemon's existing answer ladder calls this entry point unchanged —
/// cloud vendors flow through the same overlay stream contract (Started +
/// Delta + Done, or terminal Error).
pub async fn drive(agent: AgentKind, question: Question) -> anyhow::Result<AnswerStream> {
    // Cloud row wins when present: the same `KindTag` can appear in both
    // tables (e.g. `Copilot` / `CopilotCloud` are distinct tags), so a
    // cloud-only tag flows to the cloud route and a local-only tag flows
    // to the CLI. Tags with rows in both are reserved for a future
    // "prefer local if installed, fall back to cloud" policy.
    if let Some(tag) = registry::KindTag::from_agent_kind(&agent) {
        if cloud::cloud_entry_for(tag).is_some() {
            return cloud::drive_cloud(agent, question).await;
        }
    }
    drive_cli(agent, question).await
}

/// A known (or generically detected) coding agent.
///
/// `Other` carries a free-form label for VS Code-family forks or agents that
/// are detected on disk but not in the registry; `Unknown` is the inert
/// default used before classification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentKind {
    ClaudeCode,
    /// The Claude desktop app's **Code mode**. Shares the Claude Code engine and
    /// transcript store with `ClaudeCode` (the CLI), but indexes sessions in its
    /// own richer store (`Library/Application Support/Claude/claude-code-sessions`)
    /// with pre-computed titles. Driven via the same `claude` CLI.
    ClaudeCodeApp,
    /// The Claude desktop app's **agent (cowork) mode**
    /// (`local-agent-mode-sessions`). Same engine/transcript store as above.
    ClaudeCodeAgent,
    Cursor,
    Antigravity,
    /// Cursor's cloud-hosted **Background Agents** (a.k.a. Cloud Agents).
    /// Distinct from the local `Cursor` (the IDE) — this row points at
    /// `https://api.cursor.com` and dispatches asynchronous, PR-producing
    /// runs. Drives via the [`crate::cloud::cursor`] adapter, not the local
    /// CLI command map.
    CursorCloud,
    /// GitHub **Copilot Coding Agent** — the cloud-hosted, task-shaped agent
    /// that runs in GitHub Actions and opens a draft pull request authored by
    /// `copilot-swe-agent[bot]`. Distinct from the local `Copilot` (the
    /// standalone `copilot` CLI on the user's machine) — this row points at
    /// `https://api.github.com/agents/...` and delegates work to a GitHub-
    /// Actions runner that clones the user's repo and produces a PR ≤59
    /// minutes later. Drives via the [`crate::cloud::copilot`] adapter, not
    /// the local CLI command map. See `docs/vendors/copilot_cloud.md` for
    /// the dossier, billing disclosure, and OPEN-QUESTIONS list.
    CopilotCloud,
    Copilot,
    Gemini,
    Codex,
    /// OpenAI's cloud-hosted **Codex Cloud** (task-shaped). Distinct from the
    /// local `Codex` (the CLI on the user's machine) — this row points at
    /// `https://api.openai.com/v1/codex/cloud/tasks` and delegates work to a
    /// sandboxed container in OpenAI's infrastructure that clones the user's
    /// repo and opens a PR minutes later. Drives via the
    /// [`crate::cloud::codex_cloud`] adapter, not the local CLI command map.
    CodexCloud,
    /// Anthropic's **Claude Managed Agents** (session-shaped, BYOT). Distinct
    /// from the local `ClaudeCode` (the CLI on the user's machine) — this row
    /// points at `https://api.anthropic.com/v1/sessions` and delegates work to
    /// an Anthropic-hosted sandbox with the `managed-agents-2026-04-01` beta
    /// header. The user MUST provide their own Console API key
    /// (`sk-ant-api03-*`); subscription OAuth (`sk-ant-oat01-*`) is REJECTED
    /// per Anthropic's third-party policy. Drives via the
    /// [`crate::cloud::anthropic`] adapter, not the local CLI command map. See
    /// `docs/vendors/anthropic_managed.md` for the BYOT disclosure obligations.
    AnthropicCloud,
    Aider,
    Windsurf,
    /// A generic VS Code-family agent detected by footprint, not by name.
    VsCodeFork,
    /// Google **Antigravity (Cloud)** — the Managed Agents surface of the Gemini
    /// API. Distinct from the local `Antigravity` (the bundled `agy`/`gemini`
    /// CLI on the user's machine) — this row points at
    /// `https://generativelanguage.googleapis.com/v1beta/interactions` and runs the
    /// Antigravity agent (`antigravity-preview-05-2026`, built on Gemini 3.5
    /// Flash) in a Google-hosted ephemeral Linux sandbox. It is **the same
    /// Gemini API endpoint** a generic Gemini-cloud agent would use — only the
    /// `agent` field of the request body differs. Session/turn-shaped (BYOT
    /// Gemini API key). Drives via the [`crate::cloud::antigravity_cloud`]
    /// adapter, not the local CLI command map. See
    /// `docs/vendors/antigravity_cloud.md` for the dossier, billing disclosure,
    /// the overlap-with-Gemini answer, and the OPEN-QUESTIONS list.
    AntigravityCloud,
    /// Google's **Gemini Managed Agents** — the generic cloud surface of the
    /// Gemini API (the Interactions API, announced at Google I/O 2026).
    /// Distinct from the local `Gemini` (the `gemini` CLI on the user's
    /// machine) — this row points at
    /// `https://generativelanguage.googleapis.com/v1beta/interactions` and runs
    /// a managed agent (Gemini 3.5 Flash) in a Google-hosted, ephemeral Linux
    /// sandbox that reasons, executes code, and browses the web. It shares the
    /// Interactions API endpoint with `AntigravityCloud` (a sibling row added
    /// in parallel); the rows are distinct vendor identities (separate keychain
    /// service, audit label, consent text). It is **turn-shaped** (a
    /// synchronous answer, optionally streamed over SSE), NOT task-shaped like
    /// the other cloud vendors — so it drives like a normal answer
    /// (`Started → Delta → Done`). The user MUST provide their own AI Studio
    /// API key, the `AIza…` form, delivered in the `x-goog-api-key` header;
    /// Vertex AI with OAuth2/ADC is out of scope for v1.
    /// Drives via the [`crate::cloud::gemini_cloud`] adapter, not the local CLI
    /// command map. See `docs/vendors/gemini_cloud.md` for the dossier, BYOT
    /// disclosure, and OPEN-QUESTIONS list.
    GeminiCloud,
    /// A detected-but-unrecognized agent; the string is a best-effort label.
    Other(String),
    /// Inert default: nothing classified yet.
    Unknown,
}

/// What a discovered agent can do for Bluey, at a coarse grain.
///
/// `NeedsTrust`, `NeedsReauth`, and `CloudBlocked` are honest stubs in Slice 1:
/// the data needed to assert them (an interactive trust probe, a per-connector
/// auth check, a cloud-only marker) is not gathered yet. `NeedsReauth` is
/// fundamentally per-connector and is derived from [`connectors::AuthTier`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    /// A drivable CLI binary is on `PATH`.
    Drive,
    /// A session store / connector config is readable, but no CLI to drive.
    ReadOnly,
    /// Drivable only after a one-time interactive trust step. (TODO: probe.)
    NeedsTrust,
    /// A connector needs re-login before use. (TODO: per-connector, Slice 2+.)
    NeedsReauth,
    /// Cloud-only agent: nothing local to read or drive. (TODO: detect.)
    CloudBlocked,
}

pub use connectors::{AuthTier, Connector, Transport};

/// A pointer to a prior agent session, cheap to list without decoding the body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionRef {
    pub id: String,
    pub title: Option<String>,
    /// Best-effort last-updated marker (RFC3339 or epoch string, per source).
    pub updated_at: String,
    /// The project/workspace this session belongs to (a filesystem path), when
    /// the store encodes it. Claude Code stores sessions under
    /// `~/.claude/projects/<encoded-cwd>/`, so the cwd is recoverable; other
    /// stores may leave this `None`.
    #[serde(default)]
    pub project: Option<String>,
}

/// A normalized conversation, identical in shape across every agent format.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Transcript {
    pub turns: Vec<Turn>,
}

/// One normalized turn in a [`Transcript`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Turn {
    pub role: Role,
    pub text: String,
}

/// Speaker role for a [`Turn`], normalized across source formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    User,
    Assistant,
    System,
    Other,
}

/// How a discovered agent stores its session history on disk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionFormat {
    /// Newline-delimited JSON (Claude Code `~/.claude/projects/*/*.jsonl`).
    Jsonl,
    /// VS Code-family `state.vscdb` SQLite store (Cursor, VS Code).
    SqliteVscdb,
    /// Antigravity's plaintext conversation **index**
    /// (`~/.gemini/antigravity/agyhub_summaries_proto.pb`): a protobuf listing
    /// every conversation's `(uuid, title, project)`. Bodies live separately in
    /// `conversations/<uuid>.db` (SQLite) or `brain/<uuid>/…/transcript.jsonl`;
    /// the reader resolves the richest readable body per session. Encrypted
    /// `conversations/<uuid>.pb` bodies are list-only (no readable transcript).
    AntigravityIndex,
    /// One JSON file per session (VS Code / Copilot `chatSessions/*.json`).
    JsonFiles,
    /// The Claude desktop app's session **index**: one `local_*.json` per
    /// session holding rich metadata (title, model, `cliSessionId`, turn count)
    /// but NOT the message bodies. The transcript itself lives in the shared
    /// Claude Code JSONL store, resolved by following `cliSessionId` into
    /// `~/.claude/projects/<encoded-cwd>/<cliSessionId>.jsonl`. A two-hop read.
    ClaudeAppIndex,
}

/// A located session store: where it is and how to decode it (decoder lands in
/// Slice 3; Slice 1 only records the location and format).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionStore {
    pub path: PathBuf,
    pub format: SessionFormat,
}

/// One agent found on the system, with the evidence that proved it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiscoveredAgent {
    pub kind: AgentKind,
    /// Paths/binaries that proved this agent is installed (combined on dedup).
    pub install_evidence: Vec<PathBuf>,
    pub capability: Capability,
    /// MCP connector config path, if one was located.
    pub connector_config_path: Option<PathBuf>,
    /// Session store location + format, if one was located.
    pub session_store: Option<SessionStore>,
}

impl DiscoveredAgent {
    /// True when a drivable CLI binary proved this agent (a binary on `PATH`,
    /// not merely a data-dir footprint). Used by [`compute_capability`].
    pub(crate) fn has_cli_evidence(&self) -> bool {
        self.install_evidence
            .iter()
            .any(|p| p.components().count() == 1 || is_executable_path(p))
    }
}

/// Heuristic: a path that lives under a `bin`-like dir is treated as an
/// executable. Read-only; never executes anything.
fn is_executable_path(path: &std::path::Path) -> bool {
    path.components().any(|c| {
        let s = c.as_os_str().to_string_lossy();
        s == "bin" || s == "sbin" || s.ends_with(".local")
    })
}

/// A uniform handle over one installed agent: discover, read context, and
/// drive a new question.
#[async_trait::async_trait]
pub trait AgentSource: Send + Sync {
    /// Which agent this is.
    fn kind(&self) -> AgentKind;

    /// Coarse capability classification (see [`Capability`]).
    fn capability(&self) -> Capability;

    /// MCP connectors inherited from this agent's config (shape + auth tier
    /// only — never secrets).
    fn connectors(&self) -> Vec<Connector>;

    /// Top-`limit` most-recent sessions, cheaply (no body decode).
    fn recent_sessions(&self, limit: usize) -> anyhow::Result<Vec<SessionRef>>;

    /// Decode a single session into a normalized [`Transcript`], bounded to
    /// `max_turns`.
    fn read_session(&self, id: &str, max_turns: usize) -> anyhow::Result<Transcript>;

    /// Drive a new question through this agent's headless CLI and stream the
    /// answer. The default delegates to the data-driven [`drive`] runner for
    /// this agent's [`AgentKind`]; agents whose [`Capability`] is not
    /// [`Capability::Drive`] should be filtered by the caller first.
    async fn ask(&self, question: Question) -> anyhow::Result<AnswerStream> {
        drive(self.kind(), question).await
    }
}

/// Errors surfaced by the bridge. Discovery itself never returns these (it is
/// fail-soft and degrades per-agent), but connector/session reads can.
#[derive(Debug, thiserror::Error)]
pub enum BridgeError {
    #[error("path not found or unreadable: {0}")]
    Unreadable(PathBuf),
    #[error("config parse failed: {0}")]
    Parse(String),
    #[error("session store error: {0}")]
    Session(String),
}
