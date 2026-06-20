//! **Real** drive proof — the test that actually answers "does this agent work?"
//!
//! [`prove`](crate::prove) is read-only: it proves an agent is *present and
//! could* be driven, but never spawns it (driving costs money / hits quota). It
//! answers "is the door there?" — not "does anyone answer when I knock."
//!
//! This module knocks. For each agent that is discoverable and drivable on this
//! machine, it sends a real canary question through the **production** drive
//! path ([`crate::drive`]) — the exact code the daemon uses — consumes the real
//! streamed answer, and reports the honest outcome:
//!
//! - **Answered** — a non-empty answer came back. If it contains the canary
//!   token we asked for, that's a clean pass; otherwise it answered but didn't
//!   follow the instruction (still "alive", flagged).
//! - **Failed** — the agent returned a terminal error (not signed in, rate
//!   limited, context too long, …). The *real* error text is preserved.
//! - **Skipped** — not drivable on this machine (CLI/credential missing), with
//!   the reason. Cloud vendors with no stored credential land here.
//!
//! This is **consent-gated and explicit** — it is never part of the read-only
//! `prove_all()`. It runs only when the user asks (`bluey agent prove --drive`),
//! because every "Answered" line cost a real model call against the user's
//! account.
//!
//! It is the opposite of a mock: there is no fixture, no simulated response, no
//! asserted request shape. It drives the real agent and reports what really
//! happened.

use std::time::{Duration, Instant};

use futures_util::StreamExt;

use crate::titler::{self, TitleSource};
use crate::{
    discover_agents, reader_for, registry, AgentKind, AnswerChunk, DiscoveredAgent, Question, Role,
    SessionRef, SessionStore,
};

/// The canary we ask every agent to echo. Distinctive enough that a coincidental
/// match is implausible; short enough that a well-behaved agent returns exactly
/// it. We check `contains`, not equality, because some agents wrap or prefix.
pub const CANARY: &str = "LIVEPROOF7";

/// The exact prompt sent to each agent. Phrased to elicit the canary alone.
pub const PROVE_PROMPT: &str = "Reply with exactly this single word and nothing else: LIVEPROOF7";

/// Per-agent wall-clock cap for one drive. A live agent answers a one-word
/// prompt well within this; past it we record a timeout rather than hang.
const DRIVE_TIMEOUT: Duration = Duration::from_secs(60);

/// The honest outcome of one real drive attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DriveProof {
    /// The agent answered. `echoed_canary` is true when the answer contained
    /// [`CANARY`] (clean pass); false means it answered but ignored the
    /// instruction (alive, but flagged). `answer` is the (truncated) real text.
    Answered {
        echoed_canary: bool,
        answer: String,
        elapsed_ms: u64,
    },
    /// The agent produced a terminal error. `reason` is the REAL error text from
    /// the agent/CLI — never a hardcoded guess.
    Failed { reason: String, elapsed_ms: u64 },
    /// Not driven on this machine (no CLI / no credential / not drivable), with
    /// the concrete reason. No model call was made.
    Skipped { reason: String },
}

impl DriveProof {
    pub fn marker(&self) -> &'static str {
        match self {
            DriveProof::Answered {
                echoed_canary: true,
                ..
            } => "ANSWERED ✅",
            DriveProof::Answered {
                echoed_canary: false,
                ..
            } => "ANSWERED (off-canary) 🟡",
            DriveProof::Failed { .. } => "FAILED 🔴",
            DriveProof::Skipped { .. } => "skip ⬜",
        }
    }
}

/// One agent's real-drive result.
#[derive(Debug, Clone)]
pub struct AgentDriveProof {
    pub display_name: &'static str,
    pub kind: AgentKind,
    pub result: DriveProof,
}

/// Drive every discoverable agent once with the canary prompt and report the
/// real outcome. **Spawns real agents and spends real quota** — call only on
/// explicit user request.
///
/// `include_cloud` controls whether cloud vendors are attempted (they need a
/// stored credential; without one they `Skip`). Local agents are always tried
/// when discoverable.
pub async fn prove_drive_all(include_cloud: bool) -> Vec<AgentDriveProof> {
    let discovered = discover_agents();
    let mut out = Vec::new();

    for entry in registry::REGISTRY {
        let kind = entry.kind_tag.to_agent_kind();
        let is_cloud = crate::cloud::cloud_entry_for(entry.kind_tag).is_some();

        if is_cloud && !include_cloud {
            out.push(AgentDriveProof {
                display_name: entry.display_name,
                kind: kind.clone(),
                result: DriveProof::Skipped {
                    reason: "cloud vendor (pass --cloud to attempt; needs stored credential)"
                        .to_string(),
                },
            });
            continue;
        }

        // A local agent is only drivable if discovery found a drivable footprint
        // (a CLI binary). Cloud agents are "discovered" by credential presence,
        // checked inside the drive path; here we gate locals on discovery.
        if !is_cloud {
            let found = discovered.iter().any(|d| d.kind == kind);
            if !found {
                out.push(AgentDriveProof {
                    display_name: entry.display_name,
                    kind: kind.clone(),
                    result: DriveProof::Skipped {
                        reason: "not installed on this machine".to_string(),
                    },
                });
                continue;
            }
        }

        let result = drive_once(kind.clone()).await;
        out.push(AgentDriveProof {
            display_name: entry.display_name,
            kind,
            result,
        });
    }

    out
}

/// Drive a single agent kind once with the canary, bounded by [`DRIVE_TIMEOUT`].
/// Consumes the real [`AnswerChunk`] stream from the production [`crate::drive`].
///
/// This is a **faithful single attempt**: it surfaces the agent's REAL terminal
/// error verbatim (no model-fallback retry). Callers that want the propose/apply
/// model-block resolution use it as the raw probe — e.g. `bluey agent
/// resolve-model` drives this once to OBSERVE the failure, then runs its own
/// consent-gated resolution. The matrix Ask step instead uses
/// [`drive_once_resolving_model`], which layers the fallback loop on top.
pub async fn drive_once(kind: AgentKind) -> DriveProof {
    drive_once_with_model(kind, &[]).await
}

/// Like [`drive_once`], but **self-resolves a model-policy block**: if the agent
/// is signed in yet the requested model is blocked for the account (the real
/// Codex/ChatGPT case), it re-drives under a fallback model the account supports
/// — exactly like the daemon's answer path — using the registry's
/// `fallback_models` + `model_flag` (data-driven, no agent named). It tries each
/// fallback once; if every fallback is also blocked, it returns a `Failed` whose
/// reason is the honest BYOT guidance (connect an API key) rather than the raw
/// vendor 400, so the matrix Ask cell tells the truth. Bounded: a fallback is
/// recorded once tried, so the loop can never cycle. Non-model failures and a
/// successful answer are returned exactly as [`drive_once`] would.
pub async fn drive_once_resolving_model(kind: AgentKind) -> DriveProof {
    use crate::model_resolve::{byot_guidance_line, decide_model_block, ModelLoopStep};

    let display = registry::KindTag::from_agent_kind(&kind)
        .and_then(registry::entry_for)
        .map(|e| e.display_name)
        .unwrap_or("the agent");

    let mut model_override: Vec<String> = Vec::new();
    let mut tried_models: Vec<String> = Vec::new();
    loop {
        let proof = drive_once_with_model(kind.clone(), &model_override).await;
        // Only a terminal agent error is a candidate for a model-block retry; an
        // Answered/Skipped/timeout proof is returned as-is.
        let DriveProof::Failed { reason, elapsed_ms } = &proof else {
            return proof;
        };
        match decide_model_block(&kind, reason, &tried_models) {
            ModelLoopStep::RetryWithModel {
                fallback_model,
                model_flag_args,
            } => {
                tried_models.push(fallback_model.to_string());
                model_override = model_flag_args;
                // Re-drive under the fallback model on the next loop turn.
                continue;
            }
            ModelLoopStep::ConnectApiKey(byot) => {
                // Exhausted every fallback — report the honest BYOT guidance as
                // the Ask outcome, not the raw 400.
                return DriveProof::Failed {
                    reason: byot_guidance_line(display, &byot),
                    elapsed_ms: *elapsed_ms,
                };
            }
            // Not a model block (auth, runtime, timeout, …) — keep the real error.
            ModelLoopStep::NotModelBlock => return proof,
        }
    }
}

/// One real canary drive, with an optional per-run model override appended (the
/// model-fallback retry). Returns the honest [`DriveProof`]; the model-block
/// retry decision lives in the [`drive_once_resolving_model`] loop that calls it.
async fn drive_once_with_model(kind: AgentKind, model_override: &[String]) -> DriveProof {
    let started = Instant::now();
    let question = Question::new(PROVE_PROMPT);

    // Spawn the real drive, threading the model override (empty = none, so the
    // base case is byte-identical to a plain `crate::drive`). A spawn failure
    // (missing binary / no credential) is surfaced as Failed with the real reason.
    let opts = crate::drive::DriveOptions {
        model_override: model_override.to_vec(),
        ..Default::default()
    };
    let stream = match crate::drive::drive_with_options(kind, question, opts).await {
        Ok(s) => s,
        Err(e) => {
            return DriveProof::Failed {
                reason: format!("could not start: {e:#}"),
                elapsed_ms: started.elapsed().as_millis() as u64,
            };
        }
    };

    futures_util::pin_mut!(stream);
    let mut body = String::new();

    loop {
        let remaining = DRIVE_TIMEOUT.checked_sub(started.elapsed());
        let Some(remaining) = remaining else {
            return DriveProof::Failed {
                reason: format!(
                    "timed out after {}s with no terminal event",
                    DRIVE_TIMEOUT.as_secs()
                ),
                elapsed_ms: started.elapsed().as_millis() as u64,
            };
        };

        match tokio::time::timeout(remaining, stream.next()).await {
            Err(_) => {
                return DriveProof::Failed {
                    reason: format!("timed out after {}s", DRIVE_TIMEOUT.as_secs()),
                    elapsed_ms: started.elapsed().as_millis() as u64,
                };
            }
            Ok(None) => break, // stream ended
            Ok(Some(chunk)) => match chunk {
                AnswerChunk::Started { .. } => {}
                AnswerChunk::Delta(d) => body.push_str(&d),
                AnswerChunk::Done { .. } => break,
                AnswerChunk::Error(message) => {
                    return DriveProof::Failed {
                        reason: message,
                        elapsed_ms: started.elapsed().as_millis() as u64,
                    };
                }
            },
        }
    }

    let elapsed_ms = started.elapsed().as_millis() as u64;
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return DriveProof::Failed {
            reason: "returned no answer (empty stream)".to_string(),
            elapsed_ms,
        };
    }

    DriveProof::Answered {
        echoed_canary: trimmed.contains(CANARY),
        answer: truncate(trimmed, 120),
        elapsed_ms,
    }
}

/// Render the real-drive matrix as a human-readable report.
pub fn render_report(proofs: &[AgentDriveProof]) -> String {
    let mut s = String::new();
    s.push_str("REAL DRIVE PROOF — actually asked each agent the canary question.\n");
    s.push_str("(Every ANSWERED line was a real model call against your account.)\n\n");
    for p in proofs {
        s.push_str(&format!("{:<24} {}\n", p.display_name, p.result.marker()));
        match &p.result {
            DriveProof::Answered {
                answer, elapsed_ms, ..
            } => {
                s.push_str(&format!("    answer: {answer:?}  ({elapsed_ms} ms)\n"));
            }
            DriveProof::Failed { reason, elapsed_ms } => {
                s.push_str(&format!("    error:  {reason}  ({elapsed_ms} ms)\n"));
            }
            DriveProof::Skipped { reason } => {
                s.push_str(&format!("    {reason}\n"));
            }
        }
    }
    s
}

// ===========================================================================
// 5-STEP VALIDATION MATRIX HARNESS
// ===========================================================================
//
// `prove_drive_all` above answers ONE question per agent ("did it answer the
// canary?"). The matrix below answers the FULL product loop, per agent, in
// order: Detect → Sessions → Title quality → Ask → MCP. Each step is recorded
// fail-soft (a red step never aborts the others) and the result is a readable
// agent-rows × 5-columns scorecard — the single "is it all working?" output.
//
// LIVE runs are SEQUENTIAL: `validate_category` awaits each agent fully before
// the next. Parallel real-drives rate-limit and collide on auth (we saw Gemini
// 429 under light load), so there is deliberately no `join!`/`FuturesUnordered`
// over the live drives.

/// Per-step wall-clock cap for the read-only steps (detect / sessions / title).
/// These touch only the local filesystem, so they finish near-instantly; the
/// cap exists only so a pathological store can never wedge the matrix.
const READ_STEP_TIMEOUT: Duration = Duration::from_secs(20);

/// How many sessions to list + title-check per agent in the matrix. Bounded so
/// the read steps stay cheap; mirrors the daemon's `AGENT_SESSION_LIST_CAP`.
const MATRIX_SESSION_CAP: usize = 40;

/// How many of the listed sessions get their **real** first turns read so the
/// titler's mechanical/AI path can actually run. Only the first few sessions are
/// the ones a user would realistically see and pick, so we feed real material
/// for those and leave the long tail on the cheap store-title-only path. Bounds
/// both the per-session reads and (worst case) the number of AI titling calls.
const MATRIX_TITLE_CAP: usize = 8;

/// Max turns to decode when reading a session's opener for titling. The titler
/// only needs the first user message (mechanical) plus a little context for the
/// AI branch; transcripts can be enormous (a real one here is 14MB), so the read
/// is capped hard rather than decoding the whole body.
const TITLE_READ_MAX_TURNS: usize = 3;

/// The status of one matrix cell, rendered as a single glyph.
///
/// Four states map onto the brief's `🟢/🟡/🔴/⬜`: a clean pass, a pass with a
/// caveat (alive but flagged — e.g. answered off-canary, or a title that needed
/// the titler), an outright failure, and "not attempted on this machine".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CellStatus {
    /// Clean pass.
    Pass,
    /// Worked, but with a caveat worth surfacing (alive-but-flagged).
    Warn,
    /// Failed — the real reason is carried alongside.
    Fail,
    /// Not attempted (no CLI / no credential / nothing to read).
    Skip,
}

impl CellStatus {
    /// The single-glyph rendering for the scorecard grid.
    pub fn glyph(self) -> &'static str {
        match self {
            CellStatus::Pass => "🟢",
            CellStatus::Warn => "🟡",
            CellStatus::Fail => "🔴",
            CellStatus::Skip => "⬜",
        }
    }
}

/// A small pass/fail(+caveat) result for one matrix step, with a one-line human
/// detail. Used for the Detect step directly and embedded in the richer
/// per-step results below; every step ultimately renders down to one of these.
#[derive(Debug, Clone)]
pub struct StepResult {
    pub status: CellStatus,
    pub detail: String,
}

impl StepResult {
    fn pass(detail: impl Into<String>) -> Self {
        Self {
            status: CellStatus::Pass,
            detail: detail.into(),
        }
    }
    fn warn(detail: impl Into<String>) -> Self {
        Self {
            status: CellStatus::Warn,
            detail: detail.into(),
        }
    }
    fn fail(detail: impl Into<String>) -> Self {
        Self {
            status: CellStatus::Fail,
            detail: detail.into(),
        }
    }
    fn skip(detail: impl Into<String>) -> Self {
        Self {
            status: CellStatus::Skip,
            detail: detail.into(),
        }
    }
}

/// One session's title after the title-quality step: the resolved title and
/// where it came from. The [`TitleSource`] is Agent A's titler provenance
/// ([`crate::titler::TitleSource`]) — reused directly, not re-modeled.
#[derive(Debug, Clone)]
pub struct TitledSession {
    pub id: String,
    pub title: String,
    pub source: TitleSource,
}

/// The Sessions + Title-quality steps combined: how many sessions were listed
/// and, for each (bounded), its resolved title and title-source. The matrix
/// renders Sessions from `count`/`listed_ok` and Title from the source mix.
#[derive(Debug, Clone)]
pub struct SessionsResult {
    /// Outcome of the *listing* itself (step 2, "Sessions").
    pub list: StepResult,
    /// Number of sessions the store reported (0 when none / unlisted).
    pub count: usize,
    /// Per-session titles with their source (step 3, "Title quality"),
    /// bounded to [`MATRIX_SESSION_CAP`].
    pub sessions: Vec<TitledSession>,
}

impl SessionsResult {
    /// Roll the per-session title sources (Agent A's [`TitleSource`]) up into the
    /// single Title cell.
    ///
    /// A title is "usable without the titler's LLM" when it came from the store
    /// ([`TitleSource::Provided`]) or was derived mechanically
    /// ([`TitleSource::Mechanical`]). [`TitleSource::AiGenerated`] is still a real
    /// title but means the titler had to spend the user's own agent; a bare
    /// [`TitleSource::Fallback`] is only a placeholder (no real topic).
    ///
    /// 🟢 every listed session had a usable title with no AI needed; 🟡 some
    /// needed the AI titler or only got a placeholder; ⬜ nothing to title.
    fn title_cell(&self) -> StepResult {
        if self.sessions.is_empty() {
            return StepResult::skip("no sessions to title".to_string());
        }
        let total = self.sessions.len();
        let provided = self.count_source(TitleSource::Provided);
        let mechanical = self.count_source(TitleSource::Mechanical);
        let ai = self.count_source(TitleSource::AiGenerated);
        let fallback = self.count_source(TitleSource::Fallback);
        let clean = provided + mechanical;
        let detail = format!(
            "{clean}/{total} usable (store {provided}, mech {mechanical}, ai {ai}, fb {fallback})"
        );
        if clean == total {
            StepResult::pass(detail)
        } else {
            StepResult::warn(detail)
        }
    }

    fn count_source(&self, want: TitleSource) -> usize {
        self.sessions.iter().filter(|s| s.source == want).count()
    }

    /// The dominant title source across listed sessions, for the per-agent note.
    /// Returns `None` when there were no sessions to summarize.
    fn dominant_source(&self) -> Option<TitleSource> {
        [
            TitleSource::Provided,
            TitleSource::Mechanical,
            TitleSource::AiGenerated,
            TitleSource::Fallback,
        ]
        .into_iter()
        .map(|src| (src, self.count_source(src)))
        .filter(|(_, n)| *n > 0)
        .max_by_key(|(_, n)| *n)
        .map(|(src, _)| src)
    }
}

/// The MCP step outcome for a scorecard.
///
/// The MCP step measures **availability**, not model execution: can the user's
/// agent SEE and launch its MCP connectors that Bluey can detect and inherit?
/// This is still a read-only, **NON-quota** check — it never drives the model.
///
/// It now prefers a stronger, LIVE signal over a config read, with an honest
/// fallback so it can never regress below today:
/// - [`LiveEnumerated`](McpStep::LiveEnumerated) — the agent's own
///   `mcp list` / `mcp list-tools` command ran and reported its servers (and,
///   for Cursor, the per-server TOOLS). This proves the agent can actually see /
///   launch the connectors, not merely that they sit in a file. The command
///   launches the connector process / reads health — no model call, no quota.
/// - [`Available`](McpStep::Available) — the **config-only fallback**: the live
///   command had no equivalent for this agent, or it failed/timed out, so we fell
///   back to reading the connector config (the prior Level-1 behavior). Reported
///   distinctly so the cell is honest about which signal it is.
/// - [`NoneConfigured`](McpStep::NoneConfigured) / [`NoConfig`](McpStep::NoConfig)
///   — a config exists but declares none, or none was located.
#[derive(Debug, Clone)]
pub enum McpStep {
    /// STRONGEST (Level-3): during a real connector-forcing drive the agent
    /// actually INVOKED an MCP tool — an ACP `ToolCall` (a `[tool: …]` chunk) was
    /// observed mid-answer. This is the only level that proves Cap-5 for real: the
    /// tool FIRED, not merely that it's listed/configured. Holds the tool
    /// title(s) seen. Independent of whether the tool then returned data (a
    /// quota'd/401 connector still fires) — so it's the anti-fake gate the USP
    /// needs. (Costs a real model call; only set on an explicit firing probe.)
    Fired { tools: Vec<String> },
    /// LIVE Level-2: the agent's own list command enumerated its MCP surface.
    /// `servers` are the live-reported server names; `tools` are per-server tool
    /// names when the agent enumerates them (Cursor), else empty.
    LiveEnumerated {
        servers: Vec<String>,
        tools: Vec<String>,
    },
    /// Config-only fallback (Level-1): connectors were read from the config —
    /// how many MCP servers this agent exposes, and a sample of their names.
    /// Used only when the live command is unavailable or failed for this agent.
    Available { count: usize, names: Vec<String> },
    /// The agent has a config location but it declares no MCP servers.
    NoneConfigured,
    /// No connector config file was located for this agent.
    NoConfig,
}

/// How many live-enumerated server/tool names to sample into the cell detail.
const MCP_LIVE_SAMPLE: usize = 6;

impl McpStep {
    /// Map the MCP step onto a matrix cell.
    ///
    /// 🟢 the agent's connectors were LIVE-enumerated (strongest) OR ≥1 connector
    /// was found in config (fallback) — both mean tools are available to inherit;
    /// the detail text says which signal it is. ⬜ a config exists but declares
    /// none, or no config was located.
    fn cell(&self) -> StepResult {
        match self {
            McpStep::Fired { tools } => StepResult::pass(format!(
                "FIRED: agent invoked {} MCP tool call(s) mid-answer ({})",
                tools.len(),
                sample_join(tools, MCP_LIVE_SAMPLE),
            )),
            McpStep::LiveEnumerated { servers, tools } => {
                let detail = if !tools.is_empty() {
                    // Per-tool signal (Cursor): name the servers + tool count, and
                    // sample a few tool names.
                    let tool_sample = sample_join(tools, MCP_LIVE_SAMPLE);
                    let server_sample = sample_join(servers, MCP_LIVE_SAMPLE);
                    format!(
                        "live: {} server(s) [{}], {} tool(s) ({})",
                        servers.len(),
                        server_sample,
                        tools.len(),
                        tool_sample,
                    )
                } else {
                    // Server-only signal (Claude/Gemini/Copilot): name the servers.
                    format!(
                        "live: {} MCP server(s) ({})",
                        servers.len(),
                        sample_join(servers, MCP_LIVE_SAMPLE),
                    )
                };
                StepResult::pass(detail)
            }
            McpStep::Available { count, names } => {
                let sample = if names.is_empty() {
                    String::new()
                } else {
                    format!(" ({})", names.join(", "))
                };
                // "(config-only)" is the honest distinction from the live signal.
                StepResult::pass(format!(
                    "config-only: {count} MCP connector(s) available{sample}"
                ))
            }
            McpStep::NoneConfigured => {
                StepResult::skip("config present, no MCP servers declared".to_string())
            }
            McpStep::NoConfig => StepResult::skip("no connector config located".to_string()),
        }
    }
}

/// Join up to `max` items with `, `, appending `…` when truncated. Empty → "".
fn sample_join(items: &[String], max: usize) -> String {
    if items.is_empty() {
        return String::new();
    }
    let shown: Vec<&str> = items.iter().take(max).map(|s| s.as_str()).collect();
    let mut s = shown.join(", ");
    if items.len() > max {
        s.push('…');
    }
    s
}

/// Which surface an agent belongs to, for `validate_category`.
///
/// Data-driven classification (see [`category_of`]): an agent is `Cloud` when it
/// has a cloud-registry row; `Gui` when it is one of the Claude-app modes or a
/// GUI-bundle agent with no headless CLI of its own; `Cli` otherwise (a local
/// CLI binary is the drive surface).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentCategory {
    /// Local CLI binary is the drive surface (Claude Code CLI, Codex, Gemini …).
    Cli,
    /// The Claude desktop-app modes + GUI-bundle agents (no own headless CLI).
    Gui,
    /// Cloud-hosted vendor rows (Cursor Cloud, Codex Cloud, …).
    Cloud,
}

impl AgentCategory {
    /// Parse the `--category` CLI value. Accepts the three lowercase names.
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "cli" => Some(AgentCategory::Cli),
            "gui" => Some(AgentCategory::Gui),
            "cloud" => Some(AgentCategory::Cloud),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            AgentCategory::Cli => "CLI",
            AgentCategory::Gui => "GUI",
            AgentCategory::Cloud => "Cloud",
        }
    }
}

/// Classify a registry tag into its [`AgentCategory`], data-driven.
///
/// Order matters: a cloud row wins first (it is the only thing that makes a
/// tag a cloud agent). Then the GUI bucket: the Claude-app modes
/// (`ClaudeCodeApp`/`ClaudeCodeAgent`) and any agent that ships a GUI bundle
/// but has **no CLI binary of its own to drive** (Windsurf, VS Code — their
/// `drive_command` is empty). Everything else is a local CLI agent.
pub fn category_of(tag: registry::KindTag) -> AgentCategory {
    if crate::cloud::cloud_entry_for(tag).is_some() {
        return AgentCategory::Cloud;
    }
    // The Claude desktop-app surfaces are GUI by identity (they are driven via
    // the shared `claude` CLI, but the *surface* the user sees is the app).
    if matches!(
        tag,
        registry::KindTag::ClaudeCodeApp | registry::KindTag::ClaudeCodeAgent
    ) {
        return AgentCategory::Gui;
    }
    // A GUI-bundle agent with no headless CLI of its own (empty drive command)
    // is GUI-only — there is no CLI surface to validate.
    if let Some(entry) = registry::entry_for(tag) {
        let has_gui_bundle = !entry.app_bundles.is_empty() || !entry.app_dirs_windows.is_empty();
        if has_gui_bundle && entry.drive_command.is_empty() {
            return AgentCategory::Gui;
        }
    }
    AgentCategory::Cli
}

/// All 5 steps captured for one agent — the matrix row.
#[derive(Debug, Clone)]
pub struct AgentScorecard {
    pub display_name: &'static str,
    pub kind: AgentKind,
    pub category: AgentCategory,
    /// Step 1 — discovered + drivable on this machine.
    pub detect: StepResult,
    /// Steps 2 + 3 — sessions listed, and their title quality/source.
    pub sessions: SessionsResult,
    /// Step 4 — attached + driven a canary; the agent answered.
    pub ask: DriveProof,
    /// Step 5 — the agent fired its own connector and returned live data.
    pub mcp: McpStep,
}

impl AgentScorecard {
    /// The Ask cell, derived from the reused [`DriveProof`].
    fn ask_cell(&self) -> StepResult {
        match &self.ask {
            DriveProof::Answered {
                echoed_canary: true,
                elapsed_ms,
                ..
            } => StepResult::pass(format!("answered canary ({elapsed_ms} ms)")),
            DriveProof::Answered {
                echoed_canary: false,
                answer,
                elapsed_ms,
            } => StepResult::warn(format!("answered off-canary: {answer:?} ({elapsed_ms} ms)")),
            DriveProof::Failed { reason, elapsed_ms } => {
                StepResult::fail(format!("{reason} ({elapsed_ms} ms)"))
            }
            DriveProof::Skipped { reason } => StepResult::skip(reason.clone()),
        }
    }
}

/// Run all 5 steps for ONE agent, sequentially and fail-soft.
///
/// A red step still records its real reason and never aborts the others, so the
/// row is always complete. The MCP step prefers a LIVE enumeration of the agent's
/// own MCP servers/tools (the agent's `mcp list` command — read-only, NO model
/// call, NO quota) and falls back to a config read; it never drives the model.
/// Detect/sessions/title are local and fast. Only the Ask step spends real model
/// quota (bounded by [`DRIVE_TIMEOUT`]).
///
/// **Spends real quota** on the Ask step for any drivable agent — call only on
/// explicit user request.
pub async fn validate_agent(kind: &AgentKind) -> AgentScorecard {
    let tag = registry::KindTag::from_agent_kind(kind);
    let category = tag.map(category_of).unwrap_or(AgentCategory::Cli);
    let display_name = tag
        .and_then(registry::entry_for)
        .map(|e| e.display_name)
        .or_else(|| {
            tag.and_then(crate::cloud::cloud_entry_for)
                .map(|e| e.display_name)
        })
        .unwrap_or("(unknown)");

    let discovered = discover_agents();
    let found = discovered.iter().find(|d| &d.kind == kind);
    let is_cloud = category == AgentCategory::Cloud;

    // Step 1 — Detect.
    let detect = detect_step(kind, found, is_cloud);

    // Steps 2 + 3 — Sessions + Title quality (read-only, from disk).
    let sessions = sessions_step(kind, found).await;

    // Step 4 — Ask (reuses the production drive path). Only attempted when the
    // agent is detected as drivable; otherwise skip-shaped so we don't spend a
    // spawn on a known-absent agent. Uses the model-block-RESOLVING variant so
    // the scorecard reflects what the daemon's answer path actually does: a
    // blocked model auto-falls-back, and an account where every model is blocked
    // shows the honest BYOT guidance rather than a raw vendor 400.
    let ask = if detect.status == CellStatus::Fail || detect.status == CellStatus::Skip {
        DriveProof::Skipped {
            reason: format!("not driven: detect = {}", detect.detail),
        }
    } else {
        drive_once_resolving_model(kind.clone()).await
    };

    // Step 5 — MCP availability. Prefers a LIVE enumeration of the agent's own
    // MCP servers/tools (the agent's `mcp list` command — read-only, NO model
    // call, NO quota), falling back to the connector-config read. Never drives
    // the model.
    let mcp = mcp_step(kind).await;

    AgentScorecard {
        display_name,
        kind: kind.clone(),
        category,
        detect,
        sessions,
        ask,
        mcp,
    }
}

/// Run every agent in a category, **SEQUENTIALLY** — each agent is awaited fully
/// before the next begins. This is deliberate: parallel real-drives rate-limit
/// and collide on auth, so there is no `join!`/`FuturesUnordered` here.
///
/// **Spends real quota** for every drivable agent in the category.
pub async fn validate_category(category: AgentCategory) -> Vec<AgentScorecard> {
    let mut out = Vec::new();
    // One pass over the union of the local + cloud registries, in registry
    // order, filtered to the requested category. Sequential by construction:
    // the `for` loop awaits each `validate_agent` before the next iteration.
    for kind in category_members(category) {
        out.push(validate_agent(&kind).await);
    }
    out
}

/// The agent kinds that belong to a category, in registry order.
fn category_members(category: AgentCategory) -> Vec<AgentKind> {
    let mut kinds = Vec::new();
    match category {
        AgentCategory::Cloud => {
            for entry in crate::cloud::CLOUD_REGISTRY {
                kinds.push(entry.kind_tag.to_agent_kind());
            }
        }
        AgentCategory::Cli | AgentCategory::Gui => {
            for entry in registry::REGISTRY {
                if category_of(entry.kind_tag) == category {
                    kinds.push(entry.kind_tag.to_agent_kind());
                }
            }
        }
    }
    kinds
}

/// Step 1 — Detect: is the agent discovered and drivable on this machine?
fn detect_step(_kind: &AgentKind, found: Option<&DiscoveredAgent>, is_cloud: bool) -> StepResult {
    if is_cloud {
        // Cloud agents are "discovered" by stored credential, which the drive
        // path checks at spawn time. We don't read the keychain here (read-only,
        // no secrets), so detect for cloud is a warn: the row exists and is
        // routable, but liveness is proven by the Ask step.
        return StepResult::warn("cloud vendor (credential checked at drive)".to_string());
    }
    match found {
        Some(d) => StepResult::pass(format!("{:?} via {}", d.capability, evidence_summary(d))),
        None => StepResult::fail("not installed on this machine".to_string()),
    }
}

/// A short human summary of the evidence that proved an agent (first binary /
/// path), for the detect detail line.
fn evidence_summary(d: &DiscoveredAgent) -> String {
    d.install_evidence
        .first()
        .map(|p| {
            p.file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| p.to_string_lossy().into_owned())
        })
        .unwrap_or_else(|| "discovered".to_string())
}

/// Steps 2 + 3 — list the agent's sessions (read-only, replicating the daemon's
/// `reader_for(format).list(...)`) and resolve each title via the titler.
async fn sessions_step(kind: &AgentKind, found: Option<&DiscoveredAgent>) -> SessionsResult {
    let empty = || SessionsResult {
        list: StepResult::skip("no session store on disk".to_string()),
        count: 0,
        sessions: Vec::new(),
    };

    let Some(store) = found.and_then(|d| d.session_store.clone()) else {
        return empty();
    };

    // The read is synchronous + filesystem-bound; run it under a timeout on a
    // blocking thread so a pathological store can never wedge the matrix.
    let store_for_task = store.clone();
    let listed = tokio::time::timeout(
        READ_STEP_TIMEOUT,
        tokio::task::spawn_blocking(move || {
            reader_for(store_for_task.format).list(&store_for_task, MATRIX_SESSION_CAP)
        }),
    )
    .await;

    let refs: Vec<SessionRef> = match listed {
        Ok(Ok(Ok(refs))) => refs,
        Ok(Ok(Err(e))) => {
            return SessionsResult {
                list: StepResult::fail(format!("store present but list failed: {e}")),
                count: 0,
                sessions: Vec::new(),
            };
        }
        Ok(Err(join_err)) => {
            return SessionsResult {
                list: StepResult::fail(format!("session list task panicked: {join_err}")),
                count: 0,
                sessions: Vec::new(),
            };
        }
        Err(_) => {
            return SessionsResult {
                list: StepResult::fail(format!(
                    "session list timed out after {}s",
                    READ_STEP_TIMEOUT.as_secs()
                )),
                count: 0,
                sessions: Vec::new(),
            };
        }
    };

    let count = refs.len();
    let list = if count == 0 {
        StepResult::warn("store readable but empty (no sessions)".to_string())
    } else {
        StepResult::pass(format!("{count} session(s) listed [{:?}]", store.format))
    };

    // Title-quality (step 3) per session, via Agent A's titler. Sequential by
    // construction — `title_one` is awaited per session — to stay within the
    // matrix's "no parallel drives" rule (the titler's AI branch can drive the
    // agent, see `title_one`).
    //
    // Only the first `MATRIX_TITLE_CAP` sessions get their real first turns read
    // (and thus the titler's mechanical/AI path); the long tail stays on the
    // cheap store-title-only path so a 200-session store can't trigger hundreds
    // of reads or AI calls.
    let mut sessions = Vec::with_capacity(count);
    for (idx, r) in refs.into_iter().enumerate() {
        let store_for_title = (idx < MATRIX_TITLE_CAP).then(|| store.clone());
        sessions.push(title_one(kind, r, store_for_title).await);
    }

    SessionsResult {
        list,
        count,
        sessions,
    }
}

/// Step 3 for one session — resolve its title and provenance via Agent A's
/// titler ([`crate::titler::title_for_session`]).
///
/// When `store` is `Some`, the session's first few **user** turns are decoded
/// (bounded to [`TITLE_READ_MAX_TURNS`], on a blocking thread under
/// [`READ_STEP_TIMEOUT`]) and handed to the titler, so its mechanical path —
/// and, only for a genuinely noisy opener, its AI path (which drives the
/// **user's own** agent, never Bluey's) — actually has material to work with.
/// This is what makes a verbose store title clean up mechanically and a junk
/// store title (codex canaries, Copilot's identical banner) get a real topic.
///
/// When `store` is `None` (the long tail past [`MATRIX_TITLE_CAP`], or a read
/// that timed out / failed), only the raw store title is judged with an empty
/// `first_turns` slice: `Provided` if it stands on its own, else `Fallback`.
/// That path costs no read and no model call.
async fn title_one(kind: &AgentKind, r: SessionRef, store: Option<SessionStore>) -> TitledSession {
    let first_turns = match store {
        Some(store) => read_first_user_turns(&store, &r.id).await,
        None => Vec::new(),
    };
    let tr = titler::title_for_session(kind, r.title.clone(), &first_turns).await;
    TitledSession {
        id: r.id,
        title: tr.title,
        source: tr.source,
    }
}

/// Decode a session's opening **user** turns for titling — bounded, fail-soft,
/// and never blocking the async runtime.
///
/// Reads at most [`TITLE_READ_MAX_TURNS`] turns via the format's
/// [`crate::reader_for`] decoder (synchronous + filesystem-bound, so it runs on
/// a blocking thread under [`READ_STEP_TIMEOUT`]) and keeps only the
/// [`Role::User`] turns' text. Returns an empty `Vec` on any read failure,
/// timeout, or task panic — the titler then degrades to store-title-or-fallback
/// exactly as if no store were available, so titling can never wedge or fail the
/// matrix.
async fn read_first_user_turns(store: &SessionStore, id: &str) -> Vec<String> {
    let store = store.clone();
    let id = id.to_string();
    let read = tokio::time::timeout(
        READ_STEP_TIMEOUT,
        tokio::task::spawn_blocking(move || {
            reader_for(store.format).read(&store, &id, TITLE_READ_MAX_TURNS)
        }),
    )
    .await;

    match read {
        Ok(Ok(Ok(transcript))) => transcript
            .turns
            .into_iter()
            .filter(|t| t.role == Role::User)
            .map(|t| t.text)
            .collect(),
        // Read error, task panic, or timeout: degrade to no first-turns.
        _ => Vec::new(),
    }
}

/// Step 5 — MCP: can the user's agent SEE and launch its MCP connectors for
/// Bluey to inherit? This is READ-ONLY and **NON-quota** — it never drives the
/// model. The product question is "does the agent have MCP tools wired up and
/// visible," not "do those tools return live data right now" (a rate-limited /
/// misconfigured connector is the user's concern; its tools are still
/// *available* to the agent, which is what Bluey detects and inherits).
///
/// Two-level signal, strongest-first, with an honest fallback so it can never
/// regress below the prior config-read.
///
/// **Level 2 (live):** run the agent's own `mcp list` / `mcp list-tools` command
/// ([`crate::mcp_tools`]) and report the servers (and, for Cursor, the per-server
/// TOOLS) the agent itself sees. These commands launch the connector process /
/// read health — no model call, no quota — and are bounded + secret-redacted. A
/// 🟢 here is the truthful "the agent can launch its MCP tools" signal.
///
/// **Level 1 (config-only fallback):** if the agent has no live command, or it
/// failed/timed out, fall back to reading the connector config (the prior
/// behavior). Reported distinctly (`config-only: …`) so the cell stays honest.
///
/// `async` because Level 2 spawns the agent's list process (bounded by the
/// enumerator's own timeout); the fallback config read is synchronous.
async fn mcp_step(kind: &AgentKind) -> McpStep {
    use crate::mcp_tools::{enumerate_mcp_tools, McpToolsResult};

    // Level 2 first: ask the agent to enumerate its own MCP surface. Non-quota,
    // bounded, fail-soft.
    match enumerate_mcp_tools(kind).await {
        McpToolsResult::Enumerated { servers, tools } => {
            return McpStep::LiveEnumerated { servers, tools };
        }
        // No live command for this agent, or it failed/timed out → fall through
        // to the Level-1 config read so we never regress below today.
        McpToolsResult::NoCommand | McpToolsResult::Failed(_) => {}
    }

    mcp_step_config_only(kind)
}

/// LEVEL 3 (the anti-fake Cap-5 gate): drive the agent with a prompt that forces
/// an MCP tool call and detect whether a tool actually FIRED — proving the USP
/// (the driven agent uses its own connectors), not merely that connectors are
/// listed/configured. Returns [`McpStep::Fired`] with the observed tool title(s)
/// if a `[tool: …]` chunk arrives; otherwise falls through to [`mcp_step`]
/// (live-list / config) so it never regresses below the cheaper signals.
///
/// **Spends a real model call** — call only on an explicit firing probe, never in
/// the default read-only matrix. A quota'd/401 connector STILL fires the tool, so
/// this stays green even when the live data call fails (which is the whole point:
/// we prove invocation, independent of the connector's own availability).
pub async fn mcp_step_with_firing(kind: &AgentKind, prompt: &str) -> McpStep {
    let question = Question::new(prompt);
    let stream = match crate::drive(kind.clone(), question).await {
        Ok(s) => s,
        Err(_) => return mcp_step(kind).await,
    };
    futures_util::pin_mut!(stream);
    let mut tools: Vec<String> = Vec::new();
    let deadline = tokio::time::Instant::now() + DRIVE_TIMEOUT;
    while let Ok(Some(chunk)) = tokio::time::timeout_at(deadline, stream.next())
        .await
        .map_err(|_| ())
    {
        if let AnswerChunk::Delta(d) = &chunk {
            if let Some(title) = tool_title_from_delta(d) {
                if !tools.contains(&title) {
                    tools.push(title);
                }
            }
        }
        if matches!(chunk, AnswerChunk::Done { .. } | AnswerChunk::Error(_)) {
            break;
        }
    }
    if tools.is_empty() {
        // No tool fired — fall back to the cheaper signals (don't claim Fired).
        mcp_step(kind).await
    } else {
        McpStep::Fired { tools }
    }
}

/// Extract the tool title from a `[tool: <title>]` Delta chunk, if it is one.
/// This marker is emitted ONLY for an ACP `ToolCall` update (`format_tool_call`),
/// never as user/agent prose — so its presence is a reliable tool-firing signal.
fn tool_title_from_delta(delta: &str) -> Option<String> {
    let rest = delta.strip_prefix("[tool: ")?;
    let title = rest.trim_end_matches(']').trim();
    (!title.is_empty()).then(|| title.to_string())
}

/// Level-1 config-read fallback for [`mcp_step`]: read the agent's connector
/// config (no spawn, no quota) and report availability. Up to [`MCP_NAME_SAMPLE`]
/// connector names are sampled for at-a-glance context. Extracted so the
/// fallback is a single pure-ish call and is unit-testable in isolation.
fn mcp_step_config_only(kind: &AgentKind) -> McpStep {
    const MCP_NAME_SAMPLE: usize = 6;
    let config = crate::discover_agents()
        .into_iter()
        .find(|d| &d.kind == kind)
        .and_then(|d| d.connector_config_path);
    let Some(path) = config else {
        return McpStep::NoConfig;
    };
    let conns = crate::read_connectors(&path);
    if conns.is_empty() {
        return McpStep::NoneConfigured;
    }
    let names = conns
        .iter()
        .take(MCP_NAME_SAMPLE)
        .map(|c| c.name.clone())
        .collect();
    McpStep::Available {
        count: conns.len(),
        names,
    }
}

/// Render the full 5-step matrix: agent rows × 5 columns, each cell a glyph +
/// one-line detail, plus a title-source note per agent. THE "is it all
/// working?" output.
pub fn render_scorecard(cards: &[AgentScorecard]) -> String {
    let mut s = String::new();
    s.push_str("5-STEP VALIDATION MATRIX — Detect · Sessions · Title · Ask · MCP\n");
    s.push_str("(Each 🟢 Ask cell was a real model call against your account.)\n");
    s.push_str(&"=".repeat(72));
    s.push('\n');

    // Compact header row naming the five steps.
    s.push_str(&format!(
        "\n{:<22} {:^3} {:^3} {:^3} {:^3} {:^3}   {}\n",
        "agent", "Det", "Ses", "Ttl", "Ask", "Mcp", "title-source"
    ));
    s.push_str(&"-".repeat(72));
    s.push('\n');

    for c in cards {
        let detect = c.detect.status.glyph();
        let ses = c.sessions.list.status.glyph();
        let ttl = c.sessions.title_cell().status.glyph();
        let ask = c.ask_cell().status.glyph();
        let mcp = c.mcp.cell().status.glyph();
        let title_source = c.sessions.dominant_source().map(|s| s.tag()).unwrap_or("—");
        s.push_str(&format!(
            "{:<22} {:^3} {:^3} {:^3} {:^3} {:^3}   {}\n",
            truncate(c.display_name, 22),
            detect,
            ses,
            ttl,
            ask,
            mcp,
            title_source,
        ));
        // Per-step one-line details, indented under the row.
        s.push_str(&format!("    Detect:   {} {}\n", detect, c.detect.detail));
        s.push_str(&format!(
            "    Sessions: {} {}\n",
            ses, c.sessions.list.detail
        ));
        let title_cell = c.sessions.title_cell();
        s.push_str(&format!("    Title:    {} {}\n", ttl, title_cell.detail));
        s.push_str(&format!("    Ask:      {} {}\n", ask, c.ask_cell().detail));
        s.push_str(&format!("    MCP:      {} {}\n", mcp, c.mcp.cell().detail));
    }
    s
}

/// Truncate to a single line of at most `max` chars (char-boundary safe).
fn truncate(text: &str, max: usize) -> String {
    let one_line: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if one_line.chars().count() <= max {
        one_line
    } else {
        let mut t: String = one_line.chars().take(max).collect();
        t.push('…');
        t
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canary_is_distinctive_and_in_prompt() {
        // The prompt must actually ask for the canary, or a pass is meaningless.
        assert!(PROVE_PROMPT.contains(CANARY));
        // Distinctive: not a real English word an agent would emit by chance.
        assert!(CANARY.chars().any(|c| c.is_ascii_digit()));
    }

    #[test]
    fn markers_distinguish_clean_pass_from_off_canary() {
        let clean = DriveProof::Answered {
            echoed_canary: true,
            answer: "LIVEPROOF7".into(),
            elapsed_ms: 10,
        };
        let off = DriveProof::Answered {
            echoed_canary: false,
            answer: "sure, here you go".into(),
            elapsed_ms: 10,
        };
        assert_eq!(clean.marker(), "ANSWERED ✅");
        assert_eq!(off.marker(), "ANSWERED (off-canary) 🟡");
        assert_ne!(clean.marker(), off.marker());
    }

    #[test]
    fn render_includes_real_error_text_not_a_guess() {
        let proofs = vec![AgentDriveProof {
            display_name: "Codex",
            kind: AgentKind::Codex,
            result: DriveProof::Failed {
                reason: "401 Unauthorized: refresh token already used".into(),
                elapsed_ms: 800,
            },
        }];
        let report = render_report(&proofs);
        // The REAL error must appear verbatim — never a hardcoded "not signed in".
        assert!(report.contains("401 Unauthorized: refresh token already used"));
        assert!(report.contains("FAILED 🔴"));
    }

    #[test]
    fn truncate_collapses_and_caps() {
        assert_eq!(truncate("  a   b ", 80), "a b");
        let long = "x".repeat(200);
        assert!(truncate(&long, 120).ends_with('…'));
    }

    // ---- 5-step matrix: category classification --------------------------

    #[test]
    fn category_of_classifies_cli_gui_and_cloud() {
        use registry::KindTag;
        // Local CLI binaries → Cli.
        assert_eq!(category_of(KindTag::ClaudeCode), AgentCategory::Cli);
        assert_eq!(category_of(KindTag::Codex), AgentCategory::Cli);
        assert_eq!(category_of(KindTag::Gemini), AgentCategory::Cli);
        assert_eq!(category_of(KindTag::Cursor), AgentCategory::Cli);
        // Claude desktop-app modes → Gui (by identity).
        assert_eq!(category_of(KindTag::ClaudeCodeApp), AgentCategory::Gui);
        assert_eq!(category_of(KindTag::ClaudeCodeAgent), AgentCategory::Gui);
        // GUI bundle, no headless CLI of its own (empty drive command) → Gui.
        assert_eq!(category_of(KindTag::Windsurf), AgentCategory::Gui);
        assert_eq!(category_of(KindTag::VsCode), AgentCategory::Gui);
        // Cloud rows → Cloud (the cloud-registry membership wins first).
        assert_eq!(category_of(KindTag::CodexCloud), AgentCategory::Cloud);
        assert_eq!(category_of(KindTag::CursorCloud), AgentCategory::Cloud);
        assert_eq!(category_of(KindTag::GeminiCloud), AgentCategory::Cloud);
        assert_eq!(category_of(KindTag::AnthropicCloud), AgentCategory::Cloud);
    }

    #[test]
    fn category_of_cloud_wins_over_local_for_dual_tags() {
        // A tag that is registered in the cloud table is Cloud even though a
        // sibling local tag of the same vendor exists (Cursor vs CursorCloud).
        // This guards the ordering in `category_of`.
        for entry in crate::cloud::CLOUD_REGISTRY {
            assert_eq!(
                category_of(entry.kind_tag),
                AgentCategory::Cloud,
                "{} should classify as Cloud",
                entry.display_name
            );
        }
    }

    #[test]
    fn every_local_registry_row_classifies_as_cli_or_gui() {
        // No local row should ever fall into Cloud (the cloud rows live in the
        // cloud registry only), and every row gets a definite bucket.
        for entry in registry::REGISTRY {
            let cat = category_of(entry.kind_tag);
            assert_ne!(
                cat,
                AgentCategory::Cloud,
                "local row {} must not be Cloud",
                entry.display_name
            );
        }
    }

    #[test]
    fn category_members_are_disjoint_and_cover_local_registry() {
        // CLI ∪ GUI exactly partitions the local registry; neither overlaps.
        let cli = category_members(AgentCategory::Cli);
        let gui = category_members(AgentCategory::Gui);
        for k in &cli {
            assert!(!gui.contains(k), "{k:?} appears in both CLI and GUI");
        }
        let total = cli.len() + gui.len();
        assert_eq!(
            total,
            registry::REGISTRY.len(),
            "CLI+GUI must cover every local registry row exactly once"
        );
        // Cloud members come from the cloud registry, sized to match it.
        let cloud = category_members(AgentCategory::Cloud);
        assert_eq!(cloud.len(), crate::cloud::CLOUD_REGISTRY.len());
    }

    #[test]
    fn category_members_preserve_registry_order() {
        // The runner relies on registry order for deterministic sequential runs.
        let cli = category_members(AgentCategory::Cli);
        let expected: Vec<_> = registry::REGISTRY
            .iter()
            .filter(|e| category_of(e.kind_tag) == AgentCategory::Cli)
            .map(|e| e.kind_tag.to_agent_kind())
            .collect();
        assert_eq!(cli, expected);
    }

    #[test]
    fn agent_category_parse_accepts_three_names_case_insensitive() {
        assert_eq!(AgentCategory::parse("cli"), Some(AgentCategory::Cli));
        assert_eq!(AgentCategory::parse("GUI"), Some(AgentCategory::Gui));
        assert_eq!(AgentCategory::parse(" Cloud "), Some(AgentCategory::Cloud));
        assert_eq!(AgentCategory::parse("nope"), None);
    }

    // ---- 5-step matrix: cell + step semantics ----------------------------

    #[test]
    fn cell_status_glyphs_are_distinct() {
        let g = [
            CellStatus::Pass.glyph(),
            CellStatus::Warn.glyph(),
            CellStatus::Fail.glyph(),
            CellStatus::Skip.glyph(),
        ];
        // All four glyphs differ — the grid must visually distinguish states.
        for i in 0..g.len() {
            for j in (i + 1)..g.len() {
                assert_ne!(g[i], g[j], "glyphs {i} and {j} collide");
            }
        }
    }

    #[test]
    fn title_cell_passes_only_when_all_sessions_usable() {
        let all_usable = SessionsResult {
            list: StepResult::pass("2 listed"),
            count: 2,
            sessions: vec![
                titled("a", "Real topic", TitleSource::Provided),
                titled("b", "Cleaned up topic", TitleSource::Mechanical),
            ],
        };
        // Provided + Mechanical are both usable with no AI needed → 🟢.
        assert_eq!(all_usable.title_cell().status, CellStatus::Pass);

        let some_fallback = SessionsResult {
            list: StepResult::pass("2 listed"),
            count: 2,
            sessions: vec![
                titled("a", "Real topic", TitleSource::Provided),
                // A bare project-derived placeholder → the titler is needed.
                titled("b", "Bluey session", TitleSource::Fallback),
            ],
        };
        assert_eq!(some_fallback.title_cell().status, CellStatus::Warn);

        // AiGenerated counts as "needed the titler" too → 🟡.
        let needed_ai = SessionsResult {
            list: StepResult::pass("1 listed"),
            count: 1,
            sessions: vec![titled("a", "Topic from AI", TitleSource::AiGenerated)],
        };
        assert_eq!(needed_ai.title_cell().status, CellStatus::Warn);

        let none = SessionsResult {
            list: StepResult::skip("no store"),
            count: 0,
            sessions: vec![],
        };
        assert_eq!(none.title_cell().status, CellStatus::Skip);
    }

    #[test]
    fn dominant_source_reports_the_majority_title_origin() {
        let s = SessionsResult {
            list: StepResult::pass("3 listed"),
            count: 3,
            sessions: vec![
                titled("a", "t", TitleSource::Mechanical),
                titled("b", "t", TitleSource::Mechanical),
                titled("c", "t", TitleSource::Provided),
            ],
        };
        assert_eq!(s.dominant_source(), Some(TitleSource::Mechanical));
        // No sessions → no dominant source.
        let empty = SessionsResult {
            list: StepResult::skip("none"),
            count: 0,
            sessions: vec![],
        };
        assert_eq!(empty.dominant_source(), None);
    }

    #[test]
    fn ask_cell_maps_driveproof_to_status() {
        let card = |ask: DriveProof| AgentScorecard {
            display_name: "X",
            kind: AgentKind::Codex,
            category: AgentCategory::Cli,
            detect: StepResult::pass("ok"),
            sessions: SessionsResult {
                list: StepResult::skip("none"),
                count: 0,
                sessions: vec![],
            },
            ask,
            mcp: McpStep::NoConfig,
        };
        let pass = card(DriveProof::Answered {
            echoed_canary: true,
            answer: "LIVEPROOF7".into(),
            elapsed_ms: 10,
        });
        assert_eq!(pass.ask_cell().status, CellStatus::Pass);
        let warn = card(DriveProof::Answered {
            echoed_canary: false,
            answer: "hi".into(),
            elapsed_ms: 10,
        });
        assert_eq!(warn.ask_cell().status, CellStatus::Warn);
        let fail = card(DriveProof::Failed {
            reason: "429 rate limited".into(),
            elapsed_ms: 10,
        });
        assert_eq!(fail.ask_cell().status, CellStatus::Fail);
        let skip = card(DriveProof::Skipped {
            reason: "not installed".into(),
        });
        assert_eq!(skip.ask_cell().status, CellStatus::Skip);
    }

    #[test]
    fn mcp_step_availability_maps_to_cell_status() {
        // ≥1 connector available (config-only fallback) → 🟢, with the count +
        // names in the detail, clearly marked "config-only" so it is NOT confused
        // with the live signal.
        let avail = McpStep::Available {
            count: 2,
            names: vec!["perplexity".into(), "github".into()],
        };
        assert_eq!(avail.cell().status, CellStatus::Pass);
        assert!(avail.cell().detail.contains("2 MCP connector(s) available"));
        assert!(avail.cell().detail.contains("config-only"));
        assert!(avail.cell().detail.contains("perplexity"));

        // Config present but no servers declared → ⬜.
        assert_eq!(McpStep::NoneConfigured.cell().status, CellStatus::Skip);
        // No connector config located → ⬜.
        assert_eq!(McpStep::NoConfig.cell().status, CellStatus::Skip);
    }

    #[test]
    fn mcp_step_live_enumerated_maps_to_pass_and_says_live() {
        // Server-only live signal (Claude/Gemini/Copilot): 🟢, names listed, and
        // marked "live" to distinguish it from the config-only fallback.
        let server_only = McpStep::LiveEnumerated {
            servers: vec!["perplexity".into(), "github".into()],
            tools: vec![],
        };
        let cell = server_only.cell();
        assert_eq!(cell.status, CellStatus::Pass);
        assert!(cell.detail.contains("live:"), "detail: {}", cell.detail);
        assert!(cell.detail.contains("2 MCP server(s)"));
        assert!(cell.detail.contains("perplexity"));
        // The live signal must NOT be labeled config-only.
        assert!(!cell.detail.contains("config-only"));

        // Per-tool live signal (Cursor): server count + tool count + sampled
        // tool names, also 🟢.
        let with_tools = McpStep::LiveEnumerated {
            servers: vec!["perplexity".into()],
            tools: vec![
                "perplexity_ask".into(),
                "perplexity_reason".into(),
                "perplexity_research".into(),
                "perplexity_search".into(),
            ],
        };
        let cell = with_tools.cell();
        assert_eq!(cell.status, CellStatus::Pass);
        assert!(cell.detail.contains("4 tool(s)"), "detail: {}", cell.detail);
        assert!(cell.detail.contains("perplexity_ask"));
        assert!(cell.detail.contains("live:"));
    }

    #[test]
    fn tool_title_from_delta_detects_only_the_tool_marker() {
        // The firing signal: a `[tool: <title>]` chunk yields the title.
        assert_eq!(
            tool_title_from_delta("[tool: mcp__perplexity__perplexity_ask]"),
            Some("mcp__perplexity__perplexity_ask".to_string())
        );
        assert_eq!(
            tool_title_from_delta("[tool: github-mcp-server-search_code]"),
            Some("github-mcp-server-search_code".to_string())
        );
        // Plain answer text (even if it mentions tools) must NOT be a firing
        // signal — only our exact marker counts, so a 401'd connector's apology
        // text can't be mistaken for a real firing.
        assert_eq!(tool_title_from_delta("I will use the perplexity tool now."), None);
        assert_eq!(tool_title_from_delta("see [tool: x] mid-sentence"), None);
        assert_eq!(tool_title_from_delta("[tool: ]"), None);
    }

    #[test]
    fn mcp_fired_is_the_strongest_cap5_signal() {
        // FIRED (tool actually invoked mid-answer) is the anti-fake gate — a Pass
        // whose detail says FIRED, distinct from the live-enumerate / config-only
        // levels (which only prove availability, not invocation).
        let fired = McpStep::Fired {
            tools: vec!["mcp__perplexity__perplexity_ask".into()],
        };
        let cell = fired.cell();
        assert_eq!(cell.status, CellStatus::Pass);
        assert!(cell.detail.contains("FIRED"), "detail: {}", cell.detail);
        assert!(cell.detail.contains("1 MCP tool call"), "detail: {}", cell.detail);
        assert!(cell.detail.contains("perplexity_ask"));
        // Must NOT read as config-only / merely-available.
        assert!(!cell.detail.contains("config-only"));
        assert!(!cell.detail.contains("available"));
    }

    #[test]
    fn sample_join_caps_and_marks_truncation() {
        let items: Vec<String> = (0..10).map(|i| format!("s{i}")).collect();
        let joined = sample_join(&items, 3);
        assert_eq!(joined, "s0, s1, s2…");
        // No truncation marker when within the cap.
        assert_eq!(sample_join(&items[..2], 6), "s0, s1");
        assert_eq!(sample_join(&[], 6), "");
    }

    // ---- 5-step matrix: render -------------------------------------------

    /// Build a [`TitledSession`] for tests.
    fn titled(id: &str, title: &str, source: TitleSource) -> TitledSession {
        TitledSession {
            id: id.into(),
            title: title.into(),
            source,
        }
    }

    fn sample_cards() -> Vec<AgentScorecard> {
        vec![
            // A fully-green CLI agent.
            AgentScorecard {
                display_name: "Codex",
                kind: AgentKind::Codex,
                category: AgentCategory::Cli,
                detect: StepResult::pass("Drive via codex"),
                sessions: SessionsResult {
                    list: StepResult::pass("3 session(s) listed [Jsonl]"),
                    count: 3,
                    sessions: vec![titled("s1", "Refactor the chunker", TitleSource::Provided)],
                },
                ask: DriveProof::Answered {
                    echoed_canary: true,
                    answer: "LIVEPROOF7".into(),
                    elapsed_ms: 1200,
                },
                mcp: McpStep::Available {
                    count: 1,
                    names: vec!["perplexity-ask".into()],
                },
            },
            // A not-installed agent: detect fails, the rest skip.
            AgentScorecard {
                display_name: "Aider",
                kind: AgentKind::Aider,
                category: AgentCategory::Cli,
                detect: StepResult::fail("not installed on this machine"),
                sessions: SessionsResult {
                    list: StepResult::skip("no session store on disk"),
                    count: 0,
                    sessions: vec![],
                },
                ask: DriveProof::Skipped {
                    reason: "not driven: detect = not installed on this machine".into(),
                },
                mcp: McpStep::NoConfig,
            },
        ]
    }

    #[test]
    fn render_scorecard_shows_all_five_columns_and_real_details() {
        let out = render_scorecard(&sample_cards());
        // Header names every step.
        for col in ["Det", "Ses", "Ttl", "Ask", "Mcp"] {
            assert!(out.contains(col), "header missing column {col}");
        }
        // Both agents appear as rows.
        assert!(out.contains("Codex"));
        assert!(out.contains("Aider"));
        // The green agent shows pass glyphs and the MCP availability detail
        // (connector count + sampled name).
        assert!(out.contains("🟢"));
        assert!(out.contains("perplexity-ask"));
        assert!(out.contains("MCP connector(s) available"));
        // The absent agent surfaces the REAL reason, not a guess, and a 🔴/⬜ mix.
        assert!(out.contains("not installed on this machine"));
        assert!(out.contains("🔴"));
        assert!(out.contains("⬜"));
        // Title-source note (Agent A's tag) is present for the store-titled agent.
        assert!(out.contains("provided"));
    }

    #[test]
    fn render_scorecard_marks_off_canary_ask_as_warn() {
        let mut cards = sample_cards();
        cards[0].ask = DriveProof::Answered {
            echoed_canary: false,
            answer: "sure, here you go".into(),
            elapsed_ms: 900,
        };
        let out = render_scorecard(&cards);
        assert!(
            out.contains("🟡"),
            "off-canary ask should render a warn glyph"
        );
        assert!(out.contains("answered off-canary"));
    }

    #[test]
    fn render_scorecard_shows_live_enumerated_mcp_with_tools() {
        // When the MCP step is LIVE-enumerated (Cursor's per-tool path), the row
        // surfaces the live tool names + count, not the config-only wording.
        let mut cards = sample_cards();
        cards[0].mcp = McpStep::LiveEnumerated {
            servers: vec!["perplexity".into()],
            tools: vec![
                "perplexity_ask".into(),
                "perplexity_reason".into(),
                "perplexity_research".into(),
                "perplexity_search".into(),
            ],
        };
        let out = render_scorecard(&cards);
        assert!(out.contains("live:"), "should show a live MCP detail");
        assert!(out.contains("perplexity_ask"), "should name a live tool");
        assert!(out.contains("4 tool(s)"));
    }
}
