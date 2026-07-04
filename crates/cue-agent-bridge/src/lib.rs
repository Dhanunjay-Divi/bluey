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

pub mod acp;
pub mod adaptive;
pub mod auth_resolve;
pub mod capability;
pub mod cloud;
pub mod connectors;
pub mod continuation;
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
pub use drive::{
    drive as drive_cli, is_transient_network_error, AnswerChunk, AnswerStream, DriveOverrides,
    Question, ToolStatus,
};
pub use fix::{fix_apply_prompt, fix_proposal_prompt, parse_fix_proposal, FixProposal};
pub use sessions::{list_with_health_check, reader_for, ReaderHealth, SessionReader};

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
    // ACP path (default-ON for ACP-capable agents; `BLUEY_USE_ACP=0` opts out —
    // see [`should_use_acp`]). Safe to default on because the attempt is wrapped
    // in a transparent CLI fallback: a pre-first-output ACP failure (spawn /
    // transport / handshake / adapter death before any chunk) silently re-drives
    // the SAME agent+question over the legacy CLI, so an ACP-only fault never
    // becomes a hard answer failure. Non-ACP agents skip this entirely and the
    // cloud/CLI routing below is unchanged.
    if should_use_acp(&agent) {
        let cli_question = question.clone();
        let cli_agent = agent.clone();
        let acp = acp::drive_acp(agent, question).await;
        return Ok(acp_with_cli_fallback(acp, move || {
            drive_cli(cli_agent, cli_question)
        }));
    }

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

/// Like [`drive`], but threads per-run **model + effort overrides** into the
/// local-CLI branch.
///
/// This is the single dispatch entry point the daemon's answer ladder uses: it
/// owns the cloud-vs-ACP-vs-CLI decision in ONE place (so adding an agent is a
/// registry row, never a new dispatch branch in the daemon). The overrides let
/// the daemon re-drive a CLI agent under a specific model (the model-block
/// self-resolver) or reasoning effort (the overlay speed tier), applied via
/// [`drive::DriveOptions::model_override`] / `effort_override`.
///
/// Routing rule (BINDING): the ACP branch is taken ONLY when
/// `should_use_acp(&agent) && overrides.is_empty()`. ACP has no model/effort
/// parameter, so a non-empty override FORCES the cloud-check-then-CLI path —
/// an explicit user/resolver pick must never be a silent no-op on the ACP route,
/// and this also makes the ModelBlocked retry deterministic (CLI directly)
/// instead of ACP-then-fallback. When overrides are empty, behavior is
/// byte-identical to [`drive`].
pub async fn drive_with_overrides(
    agent: AgentKind,
    question: Question,
    overrides: DriveOverrides,
) -> anyhow::Result<AnswerStream> {
    if should_use_acp(&agent) && overrides.is_empty() {
        // Empty overrides: the ACP route is unchanged. Its CLI fallback also needs
        // no overrides (empty by construction on this branch), so it can build a
        // default DriveOptions.
        let cli_question = question.clone();
        let cli_agent = agent.clone();
        let acp = acp::drive_acp(agent, question).await;
        return Ok(acp_with_cli_fallback(acp, move || {
            drive::drive_with_options(cli_agent, cli_question, drive::DriveOptions::default())
        }));
    }
    if let Some(tag) = registry::KindTag::from_agent_kind(&agent) {
        if cloud::cloud_entry_for(tag).is_some() {
            return cloud::drive_cloud(agent, question).await;
        }
    }
    let opts = drive::DriveOptions {
        model_override: overrides.model_args,
        effort_override: overrides.effort_args,
        ..Default::default()
    };
    drive::drive_with_options(agent, question, opts).await
}

/// Wrap an ACP [`AnswerStream`] so a failure **before any output** transparently
/// falls back to the legacy CLI drive for the same agent+question.
///
/// This is what makes the ACP route safe to default on (see [`should_use_acp`]).
/// Because [`acp::drive_acp`] returns its stream eagerly and runs the whole turn
/// on a spawned task, EVERY pre-token fault — subprocess spawn failure, transport
/// setup, the `initialize` handshake, or an adapter that dies before emitting a
/// chunk — surfaces as a terminal [`AnswerChunk::Error`] as the stream's FIRST
/// item, never as an `Err` from `drive_acp`. (The only `Err` `drive_acp` returns
/// is "no ACP entrypoint", which [`should_use_acp`] already gates out; we still
/// fall back on it defensively.)
///
/// Policy, mirroring the proven `resume_with_fork_fallback` peek pattern:
/// - The stream's FIRST chunk is an [`AnswerChunk::Error`] → the ACP attempt
///   never produced output, so discard it and stream `make_cli()` instead. The
///   consumer sees a single clean CLI stream and never the failed ACP attempt.
/// - The first chunk is ANYTHING else ([`Started`](AnswerChunk::Started),
///   [`Delta`](AnswerChunk::Delta), [`Reasoning`](AnswerChunk::Reasoning),
///   [`ToolCall`](AnswerChunk::ToolCall), [`Done`](AnswerChunk::Done)) → ACP has
///   committed; we yield it and pass the rest of the stream through unchanged. A
///   later mid-stream error is surfaced as-is (we cannot un-emit already-streamed
///   output, so re-driving over CLI would double the answer).
///
/// `make_cli` is a `FnOnce` returning the CLI drive future, so the CLI subprocess
/// is spawned ONLY on actual fallback (the common case — ACP succeeding — never
/// touches the CLI). If `acp` is itself an `Err`, we skip the peek and go straight
/// to the CLI.
fn acp_with_cli_fallback<F, Fut>(acp: anyhow::Result<AnswerStream>, make_cli: F) -> AnswerStream
where
    F: FnOnce() -> Fut + Send + 'static,
    Fut: std::future::Future<Output = anyhow::Result<AnswerStream>> + Send,
{
    use futures_util::StreamExt;
    Box::pin(async_stream::stream! {
        // `drive_acp` only `Err`s when the agent has no ACP entrypoint — already
        // gated by `should_use_acp`, but if it ever happens, fall straight to CLI.
        let mut acp_stream = match acp {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!(error = %e, "ACP drive could not start — falling back to CLI");
                match make_cli().await {
                    Ok(mut cli) => { while let Some(c) = cli.next().await { yield c; } }
                    Err(e) => yield AnswerChunk::Error(format!("CLI fallback failed: {e}")),
                }
                return;
            }
        };

        // Peek the first item. A leading `Error` means ACP failed before any
        // output (handshake/spawn/adapter), so fall back; anything else commits.
        match acp_stream.next().await {
            None => {
                // Empty ACP stream (no chunk at all): treat like a pre-output
                // failure and fall back rather than hand the user silence.
                tracing::warn!("ACP drive produced no chunks — falling back to CLI");
                match make_cli().await {
                    Ok(mut cli) => { while let Some(c) = cli.next().await { yield c; } }
                    Err(e) => yield AnswerChunk::Error(format!("CLI fallback failed: {e}")),
                }
            }
            Some(AnswerChunk::Error(e)) => {
                tracing::warn!(
                    error = %e,
                    "ACP drive failed before any output — falling back to CLI"
                );
                match make_cli().await {
                    Ok(mut cli) => { while let Some(c) = cli.next().await { yield c; } }
                    Err(e) => yield AnswerChunk::Error(format!("CLI fallback failed: {e}")),
                }
            }
            Some(first) => {
                // ACP committed (real output started). Yield the first chunk and
                // pass everything else through unchanged — no fallback possible.
                yield first;
                while let Some(c) = acp_stream.next().await {
                    yield c;
                }
            }
        }
    })
}

/// Whether [`drive`] (and the daemon's continuation-tier decision) should route
/// `agent` through the ACP path. **The single source of truth** — the daemon
/// MUST call this rather than re-implement it, so the route choice and the
/// `via_acp` continuation choice can never desync.
///
/// **ACP is now ON by default** for every agent that has an ACP entrypoint. This
/// is safe because [`drive`] wraps the ACP attempt in a transparent CLI fallback
/// ([`acp_with_cli_fallback`]): a failure before any output silently re-drives
/// the same agent+question over the legacy CLI, so flipping the default cannot
/// turn an adapter/handshake fault into a hard answer failure.
///
/// Two gates: (1) the **escape hatch** — `BLUEY_USE_ACP` set to a falsey value
/// (`0`, `false`, `off`, `no`, case-insensitive) DISABLES ACP (read defensively
/// via `var_os`, never panics); any other value (or unset) leaves it on; and
/// (2) the agent actually has an ACP entrypoint (the per-agent capability gate —
/// agents with no ACP spec are never routed through ACP). Factored out so the
/// routing decision is unit-testable without spawning a subprocess.
pub fn should_use_acp(agent: &AgentKind) -> bool {
    let disabled = std::env::var_os("BLUEY_USE_ACP")
        .map(|v| {
            let s = v.to_string_lossy();
            let s = s.trim().to_ascii_lowercase();
            matches!(s.as_str(), "0" | "false" | "off" | "no")
        })
        .unwrap_or(false);
    !disabled && acp::drive::has_acp_spec(agent)
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
    /// The **Antigravity IDE** desktop app — a SEPARATE install from the
    /// `Antigravity` 2.0 app above. Its conversations live in their own store
    /// (`~/.gemini/antigravity-ide/`, with `conversations/<uuid>.db` SQLite and
    /// `brain/<uuid>/…/transcript.jsonl` bodies — the SAME body format as the
    /// 2.0 app), but its session INDEX is NOT a `agyhub_summaries_proto.pb`
    /// proto; instead it lives in the IDE's VS Code-style state store
    /// (`Library/Application Support/Antigravity IDE/User/globalStorage/
    /// state.vscdb`, `ItemTable` key `antigravityUnifiedStateSync.trajectory
    /// Summaries`). Read-only / replay-only (fork). Driven by no CLI of its own.
    AntigravityIde,
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
    /// The **Antigravity IDE** session index, stored in the IDE's VS Code-style
    /// `state.vscdb` (`ItemTable` key `antigravityUnifiedStateSync.trajectory
    /// Summaries`) as a base64-wrapped protobuf of `(uuid, title, project)` rows.
    /// Distinct from [`AntigravityIndex`](SessionFormat::AntigravityIndex) (the
    /// 2.0 app's `agyhub_summaries_proto.pb`), which the IDE store does NOT have.
    /// Bodies are resolved from `~/.gemini/antigravity-ide/conversations/<uuid>.db`
    /// (SQLite) and `brain/<uuid>/…/transcript.jsonl` — the same readable formats
    /// the 2.0 reader uses, shared via the antigravity reader's body functions.
    /// The store `path` points at the `state.vscdb` index file (never the
    /// credential-bearing data-dir root); the reader derives the bodies dir from
    /// `$HOME/.gemini/antigravity-ide`.
    AntigravityIdeIndex,
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

#[cfg(test)]
mod acp_gate_tests {
    use super::*;
    use std::sync::Mutex;

    /// Serializes the env-mutating test below against any other test in this
    /// module that reads/writes `BLUEY_USE_ACP` (the var is process-global).
    static ENV_GUARD: Mutex<()> = Mutex::new(());

    #[test]
    fn should_use_acp_is_on_by_default_for_acp_capable_agents() {
        // C9: the SINGLE ACP-route gate. ACP is now ON by default (the CLI
        // fallback in `drive` makes it safe), so with BLUEY_USE_ACP UNSET it must
        // be TRUE for every ACP-capable agent and FALSE for agents with no ACP
        // entrypoint (the per-agent capability gate still holds). We do NOT mutate
        // the global env var here (that would race other tests); the disable
        // branch is exercised by the serialized test below. If this test ever runs
        // with BLUEY_USE_ACP already set to a falsey value, skip rather than fail
        // spuriously.
        let _g = ENV_GUARD.lock().unwrap_or_else(|p| p.into_inner());
        let disabled = std::env::var_os("BLUEY_USE_ACP")
            .map(|v| {
                let s = v.to_string_lossy().trim().to_ascii_lowercase();
                matches!(s.as_str(), "0" | "false" | "off" | "no")
            })
            .unwrap_or(false);
        if disabled {
            return;
        }
        // ACP-capable agents: ON by default.
        for agent in [
            AgentKind::ClaudeCode,
            AgentKind::Codex,
            AgentKind::Cursor,
            AgentKind::Gemini,
        ] {
            assert!(
                should_use_acp(&agent),
                "{agent:?}: ACP must be ON by default (BLUEY_USE_ACP unset)"
            );
        }
        // No ACP entrypoint: capability gate keeps these OFF regardless.
        for agent in [AgentKind::Aider, AgentKind::Unknown] {
            assert!(
                !should_use_acp(&agent),
                "{agent:?}: no ACP spec — must stay OFF even with the default on"
            );
        }
    }

    #[test]
    fn bluey_use_acp_falsey_disables_acp() {
        // The escape hatch: setting BLUEY_USE_ACP to a falsey value turns ACP OFF
        // even for ACP-capable agents. Serialized + restored because the var is
        // process-global.
        let _g = ENV_GUARD.lock().unwrap_or_else(|p| p.into_inner());
        let prev = std::env::var_os("BLUEY_USE_ACP");
        for falsey in ["0", "false", "off", "no", "OFF", "False"] {
            std::env::set_var("BLUEY_USE_ACP", falsey);
            for agent in [
                AgentKind::ClaudeCode,
                AgentKind::Codex,
                AgentKind::Cursor,
                AgentKind::Gemini,
            ] {
                assert!(
                    !should_use_acp(&agent),
                    "{agent:?}: BLUEY_USE_ACP={falsey} must DISABLE ACP"
                );
            }
        }
        // Restore the prior value so we don't leak into other tests.
        match prev {
            Some(v) => std::env::set_var("BLUEY_USE_ACP", v),
            None => std::env::remove_var("BLUEY_USE_ACP"),
        }
    }
}

#[cfg(test)]
mod acp_fallback_tests {
    use super::*;
    use futures_util::StreamExt;

    fn canned(chunks: Vec<AnswerChunk>) -> AnswerStream {
        Box::pin(futures_util::stream::iter(chunks))
    }

    async fn collect(stream: AnswerStream) -> Vec<AnswerChunk> {
        let mut s = stream;
        let mut out = Vec::new();
        while let Some(c) = s.next().await {
            out.push(c);
        }
        out
    }

    #[tokio::test]
    async fn leading_error_falls_back_to_cli() {
        // ACP failed before any output (handshake/spawn/adapter): the consumer
        // must see ONLY the CLI stream, never the ACP error.
        let acp = Ok(canned(vec![AnswerChunk::Error(
            "acp connection: failed to spawn".into(),
        )]));
        let out = collect(acp_with_cli_fallback(acp, || async {
            Ok(canned(vec![
                AnswerChunk::Started {
                    session_id: Some("cli".into()),
                },
                AnswerChunk::Delta("cli answer".into()),
                AnswerChunk::Done { cost_usd: None },
            ]))
        }))
        .await;
        assert_eq!(
            out,
            vec![
                AnswerChunk::Started {
                    session_id: Some("cli".into())
                },
                AnswerChunk::Delta("cli answer".into()),
                AnswerChunk::Done { cost_usd: None },
            ],
            "leading ACP error must transparently become the CLI stream"
        );
    }

    #[tokio::test]
    async fn first_content_commits_and_never_falls_back() {
        // Once ACP emits a non-error chunk (here Started), it is committed: pass
        // through unchanged and NEVER touch the CLI — even if ACP later errors.
        let ran_cli = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let flag = ran_cli.clone();
        let acp = Ok(canned(vec![
            AnswerChunk::Started {
                session_id: Some("acp".into()),
            },
            AnswerChunk::Delta("acp answer".into()),
            AnswerChunk::Error("died mid-turn".into()),
        ]));
        let out = collect(acp_with_cli_fallback(acp, move || {
            flag.store(true, std::sync::atomic::Ordering::SeqCst);
            async { Ok(canned(vec![])) }
        }))
        .await;
        assert!(
            !ran_cli.load(std::sync::atomic::Ordering::SeqCst),
            "committed ACP stream must NOT fall back (would double the answer)"
        );
        assert_eq!(
            out,
            vec![
                AnswerChunk::Started {
                    session_id: Some("acp".into())
                },
                AnswerChunk::Delta("acp answer".into()),
                AnswerChunk::Error("died mid-turn".into()),
            ],
            "a mid-stream error after content is surfaced as-is"
        );
    }

    #[tokio::test]
    async fn empty_acp_stream_falls_back_to_cli() {
        // No chunk at all from ACP → fall back rather than hand the user silence.
        let acp = Ok(canned(vec![]));
        let out = collect(acp_with_cli_fallback(acp, || async {
            Ok(canned(vec![AnswerChunk::Delta("cli".into())]))
        }))
        .await;
        assert_eq!(out, vec![AnswerChunk::Delta("cli".into())]);
    }

    #[tokio::test]
    async fn acp_err_result_falls_back_to_cli() {
        // `drive_acp` returning Err (defensive — should_use_acp gates this out):
        // skip the peek and go straight to CLI.
        let acp: anyhow::Result<AnswerStream> = Err(anyhow::anyhow!("no ACP entrypoint"));
        let out = collect(acp_with_cli_fallback(acp, || async {
            Ok(canned(vec![AnswerChunk::Delta("cli".into())]))
        }))
        .await;
        assert_eq!(out, vec![AnswerChunk::Delta("cli".into())]);
    }
}
