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
pub mod capability;
pub mod connectors;
pub mod discover;
pub mod drive;
pub mod fix;
pub mod prove;
pub mod provision;
pub mod registry;
pub mod sessions;

pub use capability::compute_capability;
pub use connectors::read_connectors;
pub use discover::{discover_agents, discover_in_home, probe_sqlite_store};
pub use drive::{drive, AnswerChunk, AnswerStream, Question};
pub use fix::{fix_apply_prompt, fix_proposal_prompt, parse_fix_proposal, FixProposal};
pub use sessions::{reader_for, SessionReader};

/// A known (or generically detected) coding agent.
///
/// `Other` carries a free-form label for VS Code-family forks or agents that
/// are detected on disk but not in the registry; `Unknown` is the inert
/// default used before classification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentKind {
    ClaudeCode,
    Cursor,
    Antigravity,
    Copilot,
    Gemini,
    Codex,
    Aider,
    Windsurf,
    /// A generic VS Code-family agent detected by footprint, not by name.
    VsCodeFork,
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
    /// Protobuf conversation files (Antigravity `*.pb`).
    Protobuf,
    /// One JSON file per session (VS Code / Copilot `chatSessions/*.json`).
    JsonFiles,
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
