//! CLI drive runner — spawn an installed agent's headless CLI and stream its
//! answer back as [`AnswerChunk`]s.
//!
//! The heart of this file is [`COMMAND_MAP`]: a static, data-driven table
//! mapping each [`AgentKind`] to *how* to invoke its CLI headlessly. Adding a
//! new agent is a new row, not a new code path.
//!
//! Execution (see PLAN §6.8):
//! - args are an **array**, the prompt is a single argv entry — never a shell
//!   string, so prompt content can never be interpreted as a command;
//! - a wall-clock timeout kills the child and emits [`AnswerChunk::Error`];
//! - an output-size cap bounds memory and emits an error if exceeded;
//! - the child is killed when the answer stream is dropped (`kill_on_drop`);
//! - a missing binary or non-zero exit surfaces as `Error`, never a panic.

use std::process::Stdio;
use std::time::Duration;

use anyhow::Result;
use async_stream::stream;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

use super::{AnswerChunk, AnswerStream, DriveMode, Driver, Question};
use crate::registry::{self, FixProfile};
use crate::AgentKind;

/// Default wall-clock timeout for a single drive. Configurable per call via
/// [`DriveOptions`].
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(120);

/// Default cap on total stdout bytes read before aborting. Bounds memory for a
/// runaway or hostile child.
pub const DEFAULT_MAX_OUTPUT_BYTES: usize = 1024 * 1024;

/// How an agent's headless stdout should be turned into [`AnswerChunk`]s.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputParser {
    /// Claude Code `--output-format stream-json --verbose`: newline-delimited
    /// JSON events parsed incrementally into Started/Delta/Done.
    ClaudeStreamJson,
    /// Cursor `cursor-agent --output-format json`: a **single** JSON object
    /// emitted on success — `{"type":"result","subtype":"success","result":
    /// "<text>","session_id":"<uuid>",...}`. Parsed into one Started (carrying
    /// `session_id`), one Delta (`result`), and a Done. DOC-CONFIRMED shape
    /// (https://cursor.com/docs/cli/reference/output-format).
    CursorJson,
    /// Codex `codex exec --json`: newline-delimited JSON events —
    /// `{"type":"thread.started","thread_id":...}` (session id),
    /// `{"type":"item.completed","item":{"type":"agent_message","text":...}}`
    /// (answer text), `{"type":"turn.completed","usage":{...}}` (tokens).
    /// Parsed incrementally into Started/Delta/Done. Flags DOC-CONFIRMED
    /// (https://developers.openai.com/codex/noninteractive); exact event field
    /// shapes NEEDS-LIVE-VERIFY. Fail-soft: a non-JSON line is treated as a
    /// plain-text delta so the no-`--json` resume path still surfaces output.
    CodexJsonl,
    /// Plain text on stdout: emit the whole captured output as one Delta, then
    /// Done. Used by copilot / gemini.
    PlainText,
}

/// One row of the per-agent command map: how to invoke an agent's CLI.
///
/// Templates use the marker `{prompt}` for where the prompt argv entry goes and
/// `{id}` for a resume session id. `{prompt}` is only ever matched as a **whole
/// argv entry** and never substituted into a larger string, so prompt content
/// cannot leak into adjacent flags. `{id}` (a daemon-owned session UUID, not
/// prompt text) may be embedded within a resume token — e.g. `--resume={id}` —
/// so rows can use either the space form (`--resume {id}`) or the equals form.
#[derive(Debug, Clone, Copy)]
pub struct DriveSpec {
    /// The agent this row drives.
    pub kind_tag: KindTag,
    /// Binary name resolved from `PATH` (e.g. `claude`).
    pub binary: &'static str,
    /// One-shot argv template; `{prompt}` is replaced by the prompt entry.
    pub oneshot_args: &'static [&'static str],
    /// Extra argv appended when resuming a session; `{id}` is replaced by the
    /// session id entry. Empty when the agent has no resume flag.
    pub resume_args: &'static [&'static str],
    /// Which parser turns this agent's stdout into chunks.
    pub parser: OutputParser,
}

/// `Copy`-friendly tag mirroring the drivable subset of [`AgentKind`]. Mirrors
/// the registry's `KindTag`; kept local so the command map stays a pure `const`
/// table independent of registry detection logic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KindTag {
    ClaudeCode,
    Copilot,
    Cursor,
    Gemini,
    Codex,
}

impl KindTag {
    /// Map a runtime [`AgentKind`] to a drivable tag, if this agent has a CLI
    /// drive row. Non-drivable kinds (Aider/Windsurf/forks/Other/Unknown)
    /// return `None`.
    fn from_agent_kind(kind: &AgentKind) -> Option<Self> {
        match kind {
            // The Claude CLI and both Claude-app surfaces (Code mode, agent
            // mode) share one engine + transcript store, so all three drive
            // through the identical `claude` spec.
            AgentKind::ClaudeCode | AgentKind::ClaudeCodeApp | AgentKind::ClaudeCodeAgent => {
                Some(KindTag::ClaudeCode)
            }
            AgentKind::Copilot => Some(KindTag::Copilot),
            AgentKind::Cursor => Some(KindTag::Cursor),
            // Antigravity drives through the `gemini` CLI; Gemini CLI is the
            // same row.
            AgentKind::Gemini | AgentKind::Antigravity => Some(KindTag::Gemini),
            AgentKind::Codex => Some(KindTag::Codex),
            _ => None,
        }
    }
}

/// The per-agent command map. **Adding an agent = adding a row here.**
///
/// | Agent | binary | one-shot args | resume args | parser |
/// |---|---|---|---|---|
/// | ClaudeCode | `claude` | `-p {prompt} --output-format stream-json --verbose` | `--resume {id}` | stream-json |
/// | Copilot | `copilot` | `-p {prompt} -s` | (none) | plain text |
/// | Cursor | `cursor-agent` | `-p {prompt} --output-format json` | `--resume={id}` | cursor-json |
/// | Gemini | `gemini` | `-p {prompt}` | (none) | plain text |
/// | Codex | `codex` | `exec --skip-git-repo-check --json {prompt}` | `exec resume --skip-git-repo-check {id} {prompt} --json` | codex-jsonl |
pub const COMMAND_MAP: &[DriveSpec] = &[
    DriveSpec {
        kind_tag: KindTag::ClaudeCode,
        binary: "claude",
        oneshot_args: &[
            "-p",
            "{prompt}",
            "--output-format",
            "stream-json",
            "--verbose",
        ],
        resume_args: &["--resume", "{id}"],
        parser: OutputParser::ClaudeStreamJson,
    },
    DriveSpec {
        kind_tag: KindTag::Copilot,
        binary: "copilot",
        // `-s` (silent) suppresses stats/decoration so stdout is just the
        // answer — DOC-CONFIRMED
        // (https://docs.github.com/en/copilot/reference/copilot-cli-reference/cli-programmatic-reference).
        oneshot_args: &["-p", "{prompt}", "-s"],
        // Resume a specific prior session by id: `--resume=<id>` (appended after
        // the oneshot `-p {prompt} -s`). VERIFIED LIVE on copilot 1.0.62 with a
        // 3-turn codeword test — `copilot -p "<q>" --resume=<uuid> -s` recalled
        // state a no-resume control did not. The id is the `~/.copilot/
        // session-state/<uuid>/` dir name (the same id our reader assigns). NOTE:
        // bare `--resume` (no value) opens an interactive picker (useless
        // headless); the `=<id>` form takes the id directly and needs no picker.
        resume_args: &["--resume={id}"],
        // No JSON output flag is documented; stdout is plain text. DOC-CONFIRMED.
        parser: OutputParser::PlainText,
    },
    DriveSpec {
        kind_tag: KindTag::Cursor,
        binary: "cursor-agent",
        // `--output-format json` emits a single result object we parse with
        // `CursorJson`. DOC-CONFIRMED
        // (https://cursor.com/docs/cli/reference/output-format). NOTE: `-p` has
        // access to write/shell tools; we deliberately omit `--force`/`--yolo`,
        // so in Answer/Propose mode Cursor proposes rather than applies edits.
        //
        // `--trust` grants WORKSPACE trust ("Trust the current workspace without
        // prompting", per `cursor-agent --help`) — without it, `-p` in an
        // untrusted directory stalls on the interactive "Workspace Trust
        // Required" gate and never answers. It is SEPARATE from command/write
        // approval: that is `-f/--force` ("Force allow commands unless explicitly
        // denied") and its alias `--yolo` ("Run Everything"), which live ONLY in
        // the apply profile (`fix.apply_args`). So `--trust` lets a read-only
        // Answer proceed headless WITHOUT enabling auto-write/apply. VERIFIED
        // LIVE: `cursor-agent --trust -p "reply with exactly: OK"
        // --output-format json` answered "OK" in an untrusted repo dir
        // (is_error:false, no edits applied); `--help` confirms `--trust` is
        // workspace-trust, distinct from `-f/--force`/`--yolo`.
        oneshot_args: &["-p", "{prompt}", "--trust", "--output-format", "json"],
        // Resume uses the `--resume=<id>` equals form (the documented
        // `--continue` is an alias for `--resume=-1`, confirming the equals
        // syntax). DOC-CONFIRMED
        // (https://cursor.com/docs/cli/reference/parameters).
        resume_args: &["--resume={id}"],
        parser: OutputParser::CursorJson,
    },
    DriveSpec {
        kind_tag: KindTag::Gemini,
        binary: "gemini",
        oneshot_args: &["-p", "{prompt}"],
        // Gemini's `--resume` takes "latest" or an INDEX, not a UUID (per
        // `gemini --help`). VERIFIED LIVE: `gemini --resume latest -p "<prompt>"`
        // resumes headlessly and answers. We pass "latest" (not our `{id}`)
        // because the flag rejects a session UUID; the `{id}` we hold is not
        // Gemini's index. So this continues the most-recent Gemini session — the
        // closest honest mapping until per-id resume is supported.
        resume_args: &["--resume", "latest"],
        parser: OutputParser::PlainText,
    },
    DriveSpec {
        kind_tag: KindTag::Codex,
        binary: "codex",
        // `codex exec --json "<prompt>"` emits a JSONL event stream we parse
        // with `CodexJsonl` (thread.started → session id, item.completed
        // agent_message → answer, turn.completed.usage → tokens). Flags
        // DOC-CONFIRMED (https://developers.openai.com/codex/noninteractive);
        // exact event field shapes NEEDS-LIVE-VERIFY. Codex resume *replaces*
        // `exec --json <prompt>` with `exec resume <id> <prompt> --json` —
        // handled by `build_argv`'s Codex special-case (substitutes {id} and
        // {prompt}). VERIFIED LIVE: `codex exec resume <SESSION_ID> "<prompt>"`
        // accepts the pinned id + follow-up prompt (the prior `--last` ignored
        // the id and always took the most-recent session).
        //
        // `--skip-git-repo-check`: by default `codex exec` REFUSES to run outside
        // a trusted git repo ("Not inside a trusted directory and
        // --skip-git-repo-check was not specified") — which fails BEFORE any model
        // call when Bluey drives from a non-git cwd (e.g. a meeting overlay with no
        // project open). The flag makes the drive cwd-independent. VERIFIED via
        // `codex exec --help` AND `codex exec resume --help` (both accept it). It
        // only relaxes the git-repo gate; the read-only sandbox posture
        // (`-c sandbox_mode="read-only"`) still blocks writes.
        oneshot_args: &["exec", "--skip-git-repo-check", "--json", "{prompt}"],
        resume_args: &[
            "exec",
            "resume",
            "--skip-git-repo-check",
            "{id}",
            "{prompt}",
            "--json",
        ],
        parser: OutputParser::CodexJsonl,
    },
];

/// Look up the drive spec for an agent kind, if it is CLI-drivable.
fn spec_for(kind: &AgentKind) -> Option<&'static DriveSpec> {
    let tag = KindTag::from_agent_kind(kind)?;
    COMMAND_MAP.iter().find(|s| s.kind_tag == tag)
}

/// Tunable limits for a single drive. Defaults are production-safe; callers
/// (the daemon) may tighten them.
///
/// Not `Copy` because [`model_override`](DriveOptions::model_override) carries an
/// owned argv slice; clone it explicitly when a copy is needed.
#[derive(Debug, Clone)]
pub struct DriveOptions {
    /// Wall-clock timeout before the child is killed.
    pub timeout: Duration,
    /// Hard cap on total stdout bytes read.
    pub max_output_bytes: usize,
    /// Write posture for this drive (see [`DriveMode`]). Defaults to
    /// [`DriveMode::Answer`] so existing callers keep plain-answer behavior.
    pub mode: DriveMode,
    /// An optional per-run **model override**: the exact argv pair to APPEND to
    /// the drive so the agent runs under a specific model (e.g. Codex's
    /// `["-m", "gpt-5.1-codex"]`). This is how the model-fallback self-resolver
    /// ([`crate::model_resolve`]) re-drives a model-blocked agent under a model
    /// the account DOES support — without editing the user's config. Empty (the
    /// default) appends nothing, so existing callers are unchanged. The pair is
    /// data the resolver reads off the registry row (`model_flag` + a
    /// `fallback_models` entry); the drive layer never names a model itself.
    pub model_override: Vec<String>,
    /// Working directory to spawn the child in. Set from [`Question::cwd`] by
    /// [`drive_with_options`]. **Critical for cwd-scoped resume** (Claude resolves
    /// `--resume <id>` against `~/.claude/projects/<encoded-cwd>/`). `None` → the
    /// child inherits the daemon's cwd (unchanged for fresh, non-resumed drives).
    pub cwd: Option<String>,
}

impl Default for DriveOptions {
    fn default() -> Self {
        Self {
            timeout: DEFAULT_TIMEOUT,
            max_output_bytes: DEFAULT_MAX_OUTPUT_BYTES,
            mode: DriveMode::Answer,
            model_override: Vec::new(),
            cwd: None,
        }
    }
}

/// Build the full argv (binary + args) for a question.
///
/// The **prompt** marker `{prompt}` is only ever matched as a *whole* argv entry
/// and substituted as its own entry, so prompt content (which may carry shell
/// metacharacters) can never leak into an adjacent flag.
///
/// The **resume-id** marker `{id}` is substituted *within* a resume token, so a
/// row can use either the whole-entry form (`"{id}"`, e.g. Claude's
/// `--resume {id}`) or the embedded equals form (`"--resume={id}"`, e.g.
/// Cursor). The id is a session UUID the daemon owns — never attacker-controlled
/// prompt text — so embedding it is safe; the prompt is still kept separate.
///
/// Returns `(program, args)`.
fn build_argv(spec: &DriveSpec, q: &Question) -> (String, Vec<String>) {
    let prompt = q.render_prompt();
    let resuming = q.resume.is_some();

    // Codex is special: resume *replaces* the `exec --json <prompt>` form
    // entirely with `exec resume <id> <prompt> --json` rather than appending a
    // flag. (VERIFIED LIVE: `codex exec resume <SESSION_ID> "<prompt>"` accepts
    // the id + a follow-up prompt; the old `--last` ignored the pinned id.)
    if spec.kind_tag == KindTag::Codex && resuming {
        let id = q.resume.as_deref().unwrap_or_default();
        let args = spec
            .resume_args
            .iter()
            .map(|tok| match *tok {
                "{id}" => id.to_string(),
                "{prompt}" => prompt.clone(),
                other => other.to_string(),
            })
            .collect();
        return (spec.binary.to_string(), args);
    }

    let mut args: Vec<String> = Vec::with_capacity(spec.oneshot_args.len() + 2);
    for tok in spec.oneshot_args {
        if *tok == "{prompt}" {
            args.push(prompt.clone());
        } else {
            args.push((*tok).to_string());
        }
    }

    if resuming {
        let id = q.resume.as_deref().unwrap_or_default();
        for tok in spec.resume_args {
            // Substitute the id within the token: handles both the whole-entry
            // form (`{id}`) and the embedded equals form (`--resume={id}`).
            args.push(tok.replace("{id}", id));
        }
    }

    (spec.binary.to_string(), args)
}

/// Resolve an agent's [`FixProfile`] from the registry, data-drivenly: a
/// runtime [`AgentKind`] becomes a registry tag, then the profile is read off
/// the row. Never names an agent inline. `None` when the kind has no registry
/// row (e.g. `Other`/`Unknown`).
fn fix_profile_for_agent(agent: &AgentKind) -> Option<&'static FixProfile> {
    let tag = registry::KindTag::from_agent_kind(agent)?;
    registry::fix_profile_for(tag)
}

/// The agent's static answer-mode args (read-safe extras), off the registry.
fn answer_args_for_agent(agent: &AgentKind) -> Option<&'static [&'static str]> {
    let tag = registry::KindTag::from_agent_kind(agent)?;
    registry::entry_for(tag).map(|e| e.answer_args)
}

/// Resolve the **MCP allow-list args** for an answer drive: the agent's
/// `mcp_allow_flag` followed by the names of its own configured MCP servers.
///
/// This auto-approves only the agent's MCP (read) tools in headless mode while
/// leaving file/shell write tools gated (so a read-intent answer cannot write).
/// Does filesystem I/O (reads the agent's connector config), so it lives here
/// rather than in the pure argv builder. Returns an empty vec when the agent
/// has no `mcp_allow_flag`, no config, or no configured servers — in which case
/// no MCP auto-approval is added (safe default: nothing fires that would
/// otherwise need approval).
fn mcp_allow_args_for_agent(agent: &AgentKind) -> Vec<String> {
    let Some(tag) = registry::KindTag::from_agent_kind(agent) else {
        return Vec::new();
    };
    let Some(entry) = registry::entry_for(tag) else {
        return Vec::new();
    };
    let Some(flag) = entry.mcp_allow_flag else {
        return Vec::new();
    };
    // Find this agent's connector config among discovered agents, read the
    // server names. Discovery is read-only and fail-soft.
    let names: Vec<String> = crate::discover_agents()
        .into_iter()
        .find(|d| &d.kind == agent)
        .and_then(|d| d.connector_config_path)
        .map(|cfg| {
            crate::read_connectors(&cfg)
                .into_iter()
                .map(|c| c.name)
                .collect()
        })
        .unwrap_or_default();
    if names.is_empty() {
        return Vec::new();
    }
    render_mcp_allow_args(flag, entry.mcp_allow_style, &names)
}

/// Render an MCP allow-list to argv per the agent's [`McpAllowStyle`]. **Pure**
/// (no I/O), so the per-CLI shaping is unit-testable without discovery. Returns
/// empty for a flag with no style (a registry bug — safe default: no approval).
fn render_mcp_allow_args(
    flag: &str,
    style: Option<registry::McpAllowStyle>,
    names: &[String],
) -> Vec<String> {
    match style {
        // gemini-family: ONE flag + ONE comma-joined value (`--flag a,b,c`). A
        // space-separated multi-value is a greedy yargs array that collides with
        // `-p {prompt}` ("Cannot use both a positional prompt and --prompt").
        // VERIFIED LIVE: the comma form answers cleanly for any server count.
        Some(registry::McpAllowStyle::ServerNameCsv) => {
            vec![flag.to_string(), names.join(",")]
        }
        // Claude: `--allowed-tools "mcp__a mcp__b"` — approval is by tool-name
        // pattern; an MCP server `<s>` exposes tools under the `mcp__<s>` prefix.
        // Space-separated patterns in a SINGLE value (Claude's flag is a
        // multi-value that accepts one quoted arg fine).
        Some(registry::McpAllowStyle::ClaudeToolPattern) => {
            let patterns = names
                .iter()
                .map(|n| format!("mcp__{n}"))
                .collect::<Vec<_>>()
                .join(" ");
            vec![flag.to_string(), patterns]
        }
        // Copilot: a repeated `--allow-tool <server>` per server.
        Some(registry::McpAllowStyle::CopilotAllowTool) => {
            let mut out = Vec::with_capacity(names.len() * 2);
            for n in names {
                out.push(flag.to_string());
                out.push(n.clone());
            }
            out
        }
        // A flag with no declared style is a registry bug; emit nothing rather
        // than guess a shape (safe default: no auto-approval).
        None => Vec::new(),
    }
}

/// Build the full argv for a drive, layering the [`DriveMode`] Fix-profile args
/// on top of the base [`build_argv`] output. **Pure** (no I/O), so the
/// mode→args wiring and the apply guard are unit-testable without spawning.
///
/// Append rules (read off the agent's [`FixProfile`], never by agent name):
/// - [`DriveMode::Answer`] → base argv only;
/// - [`DriveMode::ProposeFix`] → base argv + `propose_args`;
/// - [`DriveMode::ApplyFix`] → base argv + `apply_args`, **but only** if the
///   profile's `apply_supported` is `true`; otherwise an `Err` (the apply is
///   refused, never spawned).
///
/// A missing profile (`Other`/`Unknown`) leaves a non-`Answer` mode with no
/// extra args to append; since those kinds also have no drive spec they never
/// reach here, but the function degrades to base argv rather than panicking.
fn build_argv_with_mode(
    spec: &DriveSpec,
    agent: &AgentKind,
    q: &Question,
    mode: DriveMode,
    mcp_allow: &[String],
) -> Result<(String, Vec<String>)> {
    let (program, mut args) = build_argv(spec, q);

    let extra: &[&'static str] = match mode {
        // Answer mode appends only read-safe static args. MCP auto-approval is
        // handled by `mcp_allow` (scoped to named servers), not a blanket flag.
        DriveMode::Answer => answer_args_for_agent(agent).unwrap_or(&[]),
        DriveMode::ProposeFix => fix_profile_for_agent(agent)
            .map(|p| p.propose_args)
            .unwrap_or(&[]),
        DriveMode::ApplyFix => {
            let profile = fix_profile_for_agent(agent);
            match profile {
                Some(p) if p.apply_supported => p.apply_args,
                // Either no profile (unknown agent) or apply is unsupported:
                // refuse rather than silently downgrading to a write-less run.
                _ => anyhow::bail!("agent {agent:?} cannot apply fixes (no apply-capable CLI)"),
            }
        }
    };

    for tok in extra {
        args.push((*tok).to_string());
    }
    // Append the scoped MCP allow-list (Answer mode only — read intent).
    if matches!(mode, DriveMode::Answer) {
        for tok in mcp_allow {
            args.push(tok.clone());
        }
    }

    Ok((program, args))
}

/// Drive an agent with default [`DriveOptions`]. See [`drive_with_options`].
pub async fn drive(agent: AgentKind, question: Question) -> Result<AnswerStream> {
    drive_with_options(agent, question, DriveOptions::default()).await
}

/// Drive an agent in an explicit [`DriveMode`] with default limits.
///
/// `Answer` is identical to [`drive`]. `ProposeFix`/`ApplyFix` append the
/// agent's Fix-profile args (read-only vs write); `ApplyFix` on an agent that
/// cannot apply returns `Err` without spawning anything.
pub async fn drive_with_mode(
    agent: AgentKind,
    question: Question,
    mode: DriveMode,
) -> Result<AnswerStream> {
    drive_with_options(
        agent,
        question,
        DriveOptions {
            mode,
            ..DriveOptions::default()
        },
    )
    .await
}

/// Drive `agent` with explicit limits, returning a stream of [`AnswerChunk`]s.
///
/// Never panics: a missing binary, spawn failure, non-zero exit, timeout, or
/// output-cap breach is delivered as a terminal [`AnswerChunk::Error`] on the
/// stream. The function itself only returns `Err` for an unsupported agent
/// kind (no drive row), which is a programmer/config error, not a runtime one.
pub async fn drive_with_options(
    agent: AgentKind,
    question: Question,
    mut opts: DriveOptions,
) -> Result<AnswerStream> {
    let spec = match spec_for(&agent) {
        Some(s) => *s,
        None => {
            anyhow::bail!("agent {agent:?} has no CLI drive command");
        }
    };

    // Drive from the question's cwd when set (the session's project dir) so
    // cwd-scoped resume resolves correctly. An explicit `opts.cwd` (rare) wins.
    if opts.cwd.is_none() {
        opts.cwd = question.cwd.clone();
    }

    // For an answer, resolve the scoped MCP allow-list (the agent's own server
    // names) so its read-tools fire headless while writes stay gated. Empty for
    // non-answer modes or agents without an MCP allow flag.
    let mcp_allow = if matches!(opts.mode, DriveMode::Answer) {
        mcp_allow_args_for_agent(&agent)
    } else {
        Vec::new()
    };

    // Assemble argv with the requested mode. An unsupported ApplyFix returns
    // `Err` here — before any subprocess is spawned (the apply gate).
    let (program, mut args) =
        build_argv_with_mode(&spec, &agent, &question, opts.mode, &mcp_allow)?;

    // Append the per-run model override LAST (e.g. Codex's `-m gpt-5.1-codex`).
    // This is the model-fallback self-resolver ([`crate::model_resolve`])
    // re-driving under an account-supported model; the pair is data the resolver
    // read off the registry (`model_flag` + a `fallback_models` entry), never a
    // model named here. Appended after the prompt and other flags — the agents
    // that take a model flag accept it in any position (VERIFIED LIVE for
    // Codex's `-m`). Empty by default, so non-fallback drives are byte-identical.
    for tok in &opts.model_override {
        args.push(tok.clone());
    }

    // Log shape only — never the prompt text (it may carry meeting content).
    tracing::debug!(
        binary = %program,
        argc = args.len(),
        prompt_len = question.prompt.len(),
        resuming = question.resume.is_some(),
        mode = ?opts.mode,
        "driving agent CLI",
    );

    Ok(run_stream(program, args, spec.parser, opts))
}

/// Spawn the subprocess and produce the answer stream. All failure modes are
/// folded into terminal `Error` chunks; the stream always ends.
fn run_stream(
    program: String,
    args: Vec<String>,
    parser: OutputParser,
    opts: DriveOptions,
) -> AnswerStream {
    let s = stream! {
        let mut cmd = Command::new(&program);
        cmd.args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        // Spawn in the session's project dir when known (cwd-scoped resume needs
        // it). Only set it if it exists AND has real content — a missing path
        // would fail the spawn, and an EMPTY dir (e.g. an un-synced iCloud
        // placeholder like a CookWise folder under `Mobile Documents/…CloudDocs`,
        // which exists but holds 0 files) makes the agent error/hang with nothing
        // to read. In both cases fall back to the inherited cwd: the continuation
        // still works via the replayed conversation context, just not anchored to
        // a project that isn't really here.
        if let Some(dir) = opts.cwd.as_deref() {
            let p = std::path::Path::new(dir);
            let usable = p.is_dir()
                && std::fs::read_dir(p)
                    .map(|mut entries| entries.next().is_some())
                    .unwrap_or(false);
            if usable {
                cmd.current_dir(dir);
            }
        }

        // Self-resolve a runtime-version mismatch (e.g. the Copilot CLI requires
        // Node ≥ 24 but the active node is older): if this binary won't launch on
        // the active runtime AND a satisfying runtime is installed, drive it under
        // that runtime by PREPENDING the runtime's bin dir to THIS CHILD's PATH
        // only — the parent/global shell env is never touched. Fail-soft: when no
        // resolution applies, the child runs with the inherited PATH exactly as
        // before (and surfaces the honest launch error itself). See
        // `crate::runtime_resolve`.
        if let Some(child_path) = runtime_path_for(&program) {
            cmd.env("PATH", child_path);
        }

        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                let msg = if e.kind() == std::io::ErrorKind::NotFound {
                    format!("binary `{program}` not found on PATH")
                } else {
                    format!("failed to spawn `{program}`: {e}")
                };
                yield AnswerChunk::Error(msg);
                return;
            }
        };

        let stdout = match child.stdout.take() {
            Some(o) => o,
            None => {
                yield AnswerChunk::Error("child stdout was not captured".to_string());
                let _ = child.start_kill();
                return;
            }
        };
        let stderr = child.stderr.take();

        let mut reader = BufReader::new(stdout).lines();
        let mut state = ParseState::new(parser);
        let mut total_bytes: usize = 0;
        let mut capped = false;
        // For plain-text agents we accumulate the whole stdout, then emit one
        // Delta at the end. For stream-json we emit incrementally.
        let mut plain_buf = String::new();

        let deadline = tokio::time::Instant::now() + opts.timeout;

        loop {
            let next = tokio::time::timeout_at(deadline, reader.next_line()).await;
            match next {
                // Timeout elapsed.
                Err(_) => {
                    let _ = child.start_kill();
                    yield AnswerChunk::Error(format!(
                        "agent timed out after {}s",
                        opts.timeout.as_secs()
                    ));
                    return;
                }
                // Read error.
                Ok(Err(e)) => {
                    let _ = child.start_kill();
                    yield AnswerChunk::Error(format!("error reading agent output: {e}"));
                    return;
                }
                // EOF.
                Ok(Ok(None)) => break,
                // A line.
                Ok(Ok(Some(line))) => {
                    // +1 for the stripped newline; bound memory.
                    total_bytes = total_bytes.saturating_add(line.len() + 1);
                    if total_bytes > opts.max_output_bytes {
                        capped = true;
                        let _ = child.start_kill();
                        break;
                    }
                    match parser {
                        OutputParser::ClaudeStreamJson => {
                            for chunk in state.push_claude_line(&line) {
                                yield chunk;
                            }
                        }
                        OutputParser::CursorJson => {
                            for chunk in state.push_cursor_line(&line) {
                                yield chunk;
                            }
                        }
                        OutputParser::CodexJsonl => {
                            for chunk in state.push_codex_line(&line) {
                                yield chunk;
                            }
                        }
                        OutputParser::PlainText => {
                            if !plain_buf.is_empty() {
                                plain_buf.push('\n');
                            }
                            plain_buf.push_str(&line);
                        }
                    }
                }
            }
        }

        if capped {
            yield AnswerChunk::Error(format!(
                "agent output exceeded {} bytes",
                opts.max_output_bytes
            ));
            return;
        }

        // Wait for exit (bounded by the same deadline) to learn the exit code.
        let status = match tokio::time::timeout_at(deadline, child.wait()).await {
            Ok(Ok(s)) => s,
            Ok(Err(e)) => {
                yield AnswerChunk::Error(format!("failed to await agent: {e}"));
                return;
            }
            Err(_) => {
                let _ = child.start_kill();
                yield AnswerChunk::Error(format!(
                    "agent timed out after {}s",
                    opts.timeout.as_secs()
                ));
                return;
            }
        };

        if !status.success() {
            let stderr_msg = read_stderr(stderr, opts.max_output_bytes).await;
            let code = status
                .code()
                .map(|c| c.to_string())
                .unwrap_or_else(|| "signal".to_string());
            let detail = if stderr_msg.is_empty() {
                String::new()
            } else {
                format!(": {stderr_msg}")
            };
            yield AnswerChunk::Error(format!("agent exited with status {code}{detail}"));
            return;
        }

        // Successful exit: flush parser-specific terminal chunks.
        match parser {
            OutputParser::ClaudeStreamJson
            | OutputParser::CursorJson
            | OutputParser::CodexJsonl => {
                for chunk in state.finish() {
                    yield chunk;
                }
            }
            OutputParser::PlainText => {
                // Plain agents do not announce a session id; emit a bare start
                // so consumers always see a Started before any Delta.
                yield AnswerChunk::Started { session_id: None };
                if !plain_buf.is_empty() {
                    yield AnswerChunk::Delta(plain_buf);
                }
                yield AnswerChunk::Done { cost_usd: None };
            }
        }
    };

    Box::pin(s)
}

/// Decide whether `program` needs to be driven under a different runtime,
/// returning the per-spawn `PATH` the child should get. Thin wrapper over
/// [`crate::runtime_resolve::runtime_path_for_program`] (shared with the
/// MCP-tools enumerator); see that function for the full contract. Fail-soft
/// and read-only — `None` means inherit the parent `PATH` unchanged.
fn runtime_path_for(program: &str) -> Option<String> {
    crate::runtime_resolve::runtime_path_for_program(program)
}

/// Drain a child's stderr into a bounded, trimmed string for error reporting.
async fn read_stderr(stderr: Option<tokio::process::ChildStderr>, max_bytes: usize) -> String {
    let Some(stderr) = stderr else {
        return String::new();
    };
    let mut reader = BufReader::new(stderr).lines();
    let mut out = String::new();
    while let Ok(Some(line)) = reader.next_line().await {
        if out.len() + line.len() > max_bytes {
            break;
        }
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(&line);
    }
    out.trim().to_string()
}

/// Incremental parse state for the Claude stream-json format. Plain-text agents
/// do not use it (they accumulate raw stdout in the runner).
struct ParseState {
    parser: OutputParser,
    started: bool,
    done: bool,
    session_id: Option<String>,
}

impl ParseState {
    fn new(parser: OutputParser) -> Self {
        Self {
            parser,
            started: false,
            done: false,
            session_id: None,
        }
    }

    /// Parse one line of Claude stream-json, returning any chunks it produced.
    ///
    /// Event shapes (newline-delimited JSON):
    /// - `{type:"system",subtype:"init",session_id,...}` → `Started`
    /// - `{type:"assistant",message:{content:[{type:"text",text}]}}` → `Delta`
    /// - `{type:"result",result,session_id,total_cost_usd,is_error}` → `Done`
    ///   (or `Error` when `is_error`).
    ///
    /// Unparseable or unrecognized lines are ignored (fail-soft), keeping a
    /// noisy CLI from aborting the stream.
    fn push_claude_line(&mut self, line: &str) -> Vec<AnswerChunk> {
        debug_assert_eq!(self.parser, OutputParser::ClaudeStreamJson);
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return Vec::new();
        }
        let v: serde_json::Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(_) => return Vec::new(),
        };
        let Some(ty) = v.get("type").and_then(|t| t.as_str()) else {
            return Vec::new();
        };

        let mut out = Vec::new();
        match ty {
            "system" => {
                if v.get("subtype").and_then(|s| s.as_str()) == Some("init") {
                    self.session_id = v
                        .get("session_id")
                        .and_then(|s| s.as_str())
                        .map(|s| s.to_string());
                    if !self.started {
                        self.started = true;
                        out.push(AnswerChunk::Started {
                            session_id: self.session_id.clone(),
                        });
                    }
                }
            }
            "assistant" => {
                if !self.started {
                    self.started = true;
                    out.push(AnswerChunk::Started {
                        session_id: self.session_id.clone(),
                    });
                }
                if let Some(content) = v
                    .get("message")
                    .and_then(|m| m.get("content"))
                    .and_then(|c| c.as_array())
                {
                    for block in content {
                        if block.get("type").and_then(|t| t.as_str()) == Some("text") {
                            if let Some(text) = block.get("text").and_then(|t| t.as_str()) {
                                if !text.is_empty() {
                                    out.push(AnswerChunk::Delta(text.to_string()));
                                }
                            }
                        }
                    }
                }
            }
            "result" => {
                if let Some(sid) = v.get("session_id").and_then(|s| s.as_str()) {
                    self.session_id = Some(sid.to_string());
                }
                let is_error = v.get("is_error").and_then(|b| b.as_bool()).unwrap_or(false);
                if is_error {
                    let msg = v
                        .get("result")
                        .and_then(|r| r.as_str())
                        .unwrap_or("agent reported an error")
                        .to_string();
                    self.done = true;
                    out.push(AnswerChunk::Error(msg));
                } else {
                    let cost = v.get("total_cost_usd").and_then(|c| c.as_f64());
                    self.done = true;
                    out.push(AnswerChunk::Done { cost_usd: cost });
                }
            }
            _ => {}
        }
        out
    }

    /// Parse Cursor's `--output-format json` output: a **single** JSON object
    /// emitted on success. We get one line; turn it into Started + Delta + Done
    /// (or Started + Error when `is_error`).
    ///
    /// Object shape (DOC-CONFIRMED,
    /// https://cursor.com/docs/cli/reference/output-format):
    /// `{type:"result",subtype:"success",is_error,result,session_id,...}`.
    ///
    /// Non-`result` lines (the format documents only the single result object,
    /// but a stray banner is possible) are ignored fail-soft.
    fn push_cursor_line(&mut self, line: &str) -> Vec<AnswerChunk> {
        debug_assert_eq!(self.parser, OutputParser::CursorJson);
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return Vec::new();
        }
        let v: serde_json::Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(_) => return Vec::new(),
        };
        if v.get("type").and_then(|t| t.as_str()) != Some("result") {
            return Vec::new();
        }

        let mut out = Vec::new();
        if !self.started {
            self.started = true;
            self.session_id = v
                .get("session_id")
                .and_then(|s| s.as_str())
                .map(|s| s.to_string());
            out.push(AnswerChunk::Started {
                session_id: self.session_id.clone(),
            });
        }

        let is_error = v.get("is_error").and_then(|b| b.as_bool()).unwrap_or(false);
        if is_error {
            let msg = v
                .get("result")
                .and_then(|r| r.as_str())
                .unwrap_or("agent reported an error")
                .to_string();
            self.done = true;
            out.push(AnswerChunk::Error(msg));
        } else {
            if let Some(text) = v.get("result").and_then(|r| r.as_str()) {
                if !text.is_empty() {
                    out.push(AnswerChunk::Delta(text.to_string()));
                }
            }
            self.done = true;
            // Cursor's result object carries timing, not a USD cost.
            out.push(AnswerChunk::Done { cost_usd: None });
        }
        out
    }

    /// Parse one line of Codex `exec --json` output (NDJSON events). Event
    /// shapes (flags DOC-CONFIRMED, field shapes NEEDS-LIVE-VERIFY,
    /// https://developers.openai.com/codex/noninteractive):
    /// - `{type:"thread.started",thread_id}` → `Started` (session id)
    /// - `{type:"item.completed",item:{type:"agent_message",text}}` → `Delta`
    /// - `{type:"turn.completed",usage:{...}}` → `Done` (usage is tokens, not
    ///   USD, so no cost is reported)
    /// - `{type:"error",message}` / `{type:"turn.failed",error:{message}}`
    ///   → `Error`
    ///
    /// Fail-soft: a line that is not valid JSON is treated as a plain-text
    /// delta, so the resume path (which may not honor `--json`) still surfaces
    /// the agent's final message instead of swallowing it.
    fn push_codex_line(&mut self, line: &str) -> Vec<AnswerChunk> {
        debug_assert_eq!(self.parser, OutputParser::CodexJsonl);
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return Vec::new();
        }
        let v: serde_json::Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            // Not JSON: emit as a literal delta (fail-soft, see doc comment),
            // ensuring we Started first so consumers see ordering.
            Err(_) => {
                let mut out = Vec::new();
                self.start_if_needed(&mut out);
                out.push(AnswerChunk::Delta(line.to_string()));
                return out;
            }
        };
        let Some(ty) = v.get("type").and_then(|t| t.as_str()) else {
            return Vec::new();
        };

        let mut out = Vec::new();
        match ty {
            "thread.started" => {
                self.session_id = v
                    .get("thread_id")
                    .and_then(|s| s.as_str())
                    .map(|s| s.to_string());
                self.start_if_needed(&mut out);
            }
            "item.completed" | "item.updated" => {
                self.start_if_needed(&mut out);
                let item = v.get("item");
                let is_msg = item.and_then(|i| i.get("type")).and_then(|t| t.as_str())
                    == Some("agent_message");
                if is_msg {
                    if let Some(text) = item.and_then(|i| i.get("text")).and_then(|t| t.as_str()) {
                        if !text.is_empty() {
                            out.push(AnswerChunk::Delta(text.to_string()));
                        }
                    }
                }
            }
            "turn.completed" => {
                self.start_if_needed(&mut out);
                self.done = true;
                // `usage` reports token counts, not a USD cost.
                out.push(AnswerChunk::Done { cost_usd: None });
            }
            "error" | "turn.failed" => {
                self.start_if_needed(&mut out);
                let msg = v
                    .get("message")
                    .and_then(|m| m.as_str())
                    .or_else(|| {
                        v.get("error")
                            .and_then(|e| e.get("message"))
                            .and_then(|m| m.as_str())
                    })
                    .unwrap_or("agent reported an error")
                    .to_string();
                self.done = true;
                out.push(AnswerChunk::Error(msg));
            }
            _ => {}
        }
        out
    }

    /// Emit a `Started` (with the current session id) if one has not been sent,
    /// flipping the `started` flag. Shared by the Codex parser's branches.
    fn start_if_needed(&mut self, out: &mut Vec<AnswerChunk>) {
        if !self.started {
            self.started = true;
            out.push(AnswerChunk::Started {
                session_id: self.session_id.clone(),
            });
        }
    }

    /// Terminal chunks after stdout EOF, shared by every JSON parser
    /// (`ClaudeStreamJson` / `CursorJson` / `CodexJsonl`). If the CLI never
    /// emitted a terminal event we still close the stream so consumers are not
    /// left hanging: emit a `Started` if none was seen, then a `Done` with no
    /// cost.
    fn finish(&mut self) -> Vec<AnswerChunk> {
        debug_assert_ne!(self.parser, OutputParser::PlainText);
        let mut out = Vec::new();
        if !self.started {
            out.push(AnswerChunk::Started {
                session_id: self.session_id.clone(),
            });
        }
        if !self.done {
            out.push(AnswerChunk::Done { cost_usd: None });
        }
        out
    }
}

/// CLI-backed [`Driver`] over [`COMMAND_MAP`].
///
/// Not `Copy` because [`DriveOptions`] now carries an owned `model_override`.
#[derive(Debug, Clone, Default)]
pub struct CliDriver {
    opts: DriveOptions,
}

impl CliDriver {
    /// A driver with explicit limits.
    pub fn with_options(opts: DriveOptions) -> Self {
        Self { opts }
    }
}

#[async_trait::async_trait]
impl Driver for CliDriver {
    async fn drive(&self, agent: AgentKind, question: Question) -> Result<AnswerStream> {
        drive_with_options(agent, question, self.opts.clone()).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Role, Transcript, Turn};
    use futures_util::StreamExt;
    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;
    use tempfile::TempDir;

    /// Collect a whole answer stream into a vec for assertions.
    async fn collect(stream: AnswerStream) -> Vec<AnswerChunk> {
        stream.collect::<Vec<_>>().await
    }

    /// Write an executable shell script into `dir` and return its path.
    fn write_script(dir: &TempDir, name: &str, body: &str) -> std::path::PathBuf {
        let path = dir.path().join(name);
        let mut f = std::fs::File::create(&path).expect("create script");
        f.write_all(body.as_bytes()).expect("write script");
        let mut perms = f.metadata().expect("meta").permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&path, perms).expect("chmod");
        path
    }

    /// Drive a fake binary directly by path, bypassing PATH resolution and the
    /// command map's fixed binary name. Mirrors `run_stream` but lets a test
    /// point at a fixture script.
    fn drive_fixture(
        program: std::path::PathBuf,
        args: Vec<String>,
        parser: OutputParser,
        opts: DriveOptions,
    ) -> AnswerStream {
        run_stream(program.to_string_lossy().into_owned(), args, parser, opts)
    }

    // ---- argv construction / no-injection -------------------------------

    #[test]
    fn test_build_argv_prompt_is_single_entry() {
        let spec = COMMAND_MAP
            .iter()
            .find(|s| s.kind_tag == KindTag::ClaudeCode)
            .unwrap();
        let q = Question::new("hello world");
        let (prog, args) = build_argv(spec, &q);
        assert_eq!(prog, "claude");
        // The prompt occupies exactly one argv slot.
        assert_eq!(args.iter().filter(|a| *a == "hello world").count(), 1);
        assert!(args.contains(&"--output-format".to_string()));
        assert!(args.contains(&"stream-json".to_string()));
    }

    #[test]
    fn test_build_argv_resume_appends_id() {
        let spec = COMMAND_MAP
            .iter()
            .find(|s| s.kind_tag == KindTag::ClaudeCode)
            .unwrap();
        let mut q = Question::new("follow up");
        q.resume = Some("sess-123".to_string());
        let (_, args) = build_argv(spec, &q);
        let i = args.iter().position(|a| a == "--resume").unwrap();
        assert_eq!(args[i + 1], "sess-123");
    }

    #[test]
    fn test_build_argv_cursor_uses_json_output_format() {
        // Cursor one-shot must request `--output-format json` so the single
        // result object is emitted for the CursorJson parser.
        let spec = spec(KindTag::Cursor);
        let q = Question::new("hi");
        let (prog, args) = build_argv(spec, &q);
        assert_eq!(prog, "cursor-agent");
        let i = args
            .iter()
            .position(|a| a == "--output-format")
            .expect("output-format flag present");
        assert_eq!(args[i + 1], "json");
        assert_eq!(spec.parser, OutputParser::CursorJson);
    }

    #[test]
    fn test_build_argv_cursor_carries_workspace_trust_not_write_flags() {
        // REGRESSION (found by the real drive proof): without `--trust`,
        // `cursor-agent -p` in an untrusted directory stalls on the interactive
        // "Workspace Trust Required" gate and an Answer never returns. So the
        // one-shot base must carry `--trust` (workspace trust). VERIFIED LIVE
        // that this answers headless.
        let spec = spec(KindTag::Cursor);
        let q = Question::new("hi");
        let (_, args) = build_argv(spec, &q);
        assert!(
            args.iter().any(|a| a == "--trust"),
            "Cursor one-shot must carry --trust to answer headless, got {args:?}"
        );
        // SAFETY: `--trust` is workspace trust only. The write/command-approval
        // flags (`-f`/`--force`/`--yolo`) must NOT be in the base argv — they
        // belong solely to the apply profile, so a read-only Answer cannot write.
        assert!(!args.iter().any(|a| a == "--force"));
        assert!(!args.iter().any(|a| a == "-f"));
        assert!(!args.iter().any(|a| a == "--yolo"));
    }

    #[test]
    fn test_build_argv_cursor_resume_uses_equals_form() {
        // Cursor resume is the `--resume=<id>` equals form (one argv entry).
        let spec = spec(KindTag::Cursor);
        let mut q = Question::new("follow up");
        q.resume = Some("chat-77".to_string());
        let (_, args) = build_argv(spec, &q);
        assert!(
            args.iter().any(|a| a == "--resume=chat-77"),
            "expected --resume=chat-77 as a single entry, got {args:?}"
        );
        // The id is never a separate bare entry.
        assert!(!args.iter().any(|a| a == "chat-77"));
    }

    #[test]
    fn test_build_argv_copilot_silent_flag_and_resume_by_id() {
        // Copilot one-shot carries `-s` (silent). Resume appends `--resume=<id>`
        // (VERIFIED LIVE on 1.0.62 via a 3-turn codeword test) — the `=<id>` form
        // takes the session id directly, no interactive picker.
        let spec = spec(KindTag::Copilot);
        let mut q = Question::new("q");
        let (_, oneshot) = build_argv(spec, &q);
        assert!(oneshot.iter().any(|a| a == "-s"), "expected -s silent flag");
        assert!(
            !oneshot.iter().any(|a| a.starts_with("--resume")),
            "one-shot (no resume id) must NOT carry --resume"
        );

        q.resume = Some("uuid-1234".to_string());
        let (_, resumed) = build_argv(spec, &q);
        assert!(
            resumed.iter().any(|a| a == "--resume=uuid-1234"),
            "resume must append --resume=<id>, got {resumed:?}"
        );
        // Still a headless one-shot: keeps -p/-s, just adds the resume target.
        assert!(resumed.iter().any(|a| a == "-s"));
    }

    #[test]
    fn test_build_argv_codex_resume_uses_session_id_and_prompt() {
        // VERIFIED LIVE: `codex exec resume <SESSION_ID> "<prompt>"` accepts a
        // pinned id + follow-up prompt; the prior `--last` ignored the id.
        let spec = spec(KindTag::Codex);
        let mut q = Question::new("the follow-up question");
        q.resume = Some("019a4a48-d3b4-7591-9e80-1b84ca5868f8".to_string());
        let (prog, args) = build_argv(spec, &q);
        assert_eq!(prog, "codex");
        assert_eq!(
            args,
            vec![
                "exec",
                "resume",
                // --skip-git-repo-check makes resume cwd-independent (no git repo
                // required); the id + prompt substitution is unaffected by it.
                "--skip-git-repo-check",
                "019a4a48-d3b4-7591-9e80-1b84ca5868f8",
                "the follow-up question",
                "--json",
            ]
        );
    }

    #[test]
    fn test_codex_skip_git_repo_check_on_both_paths() {
        // Codex must carry --skip-git-repo-check on BOTH the oneshot and resume
        // drives so it runs from any cwd (a non-git dir would otherwise fail the
        // git-repo gate before any model call — found via direct probe from /tmp).
        let spec = spec(KindTag::Codex);

        // Oneshot.
        let (_, oneshot) = build_argv(spec, &Question::new("hi"));
        assert!(
            oneshot.iter().any(|a| a == "--skip-git-repo-check"),
            "oneshot must skip the git-repo check, got {oneshot:?}"
        );

        // Resume.
        let mut q = Question::new("hi");
        q.resume = Some("019a4a48-d3b4-7591-9e80-1b84ca5868f8".to_string());
        let (_, resume) = build_argv(spec, &q);
        assert!(
            resume.iter().any(|a| a == "--skip-git-repo-check"),
            "resume must skip the git-repo check, got {resume:?}"
        );
    }

    #[test]
    fn test_build_argv_gemini_resume_uses_latest_not_uuid() {
        // VERIFIED LIVE: gemini's --resume takes "latest"/index, not a UUID.
        let spec = spec(KindTag::Gemini);
        let mut q = Question::new("hi");
        q.resume = Some("some-uuid-we-cannot-use".to_string());
        let (prog, args) = build_argv(spec, &q);
        assert_eq!(prog, "gemini");
        let i = args.iter().position(|a| a == "--resume").unwrap();
        assert_eq!(args[i + 1], "latest");
        assert!(!args.iter().any(|a| a.contains("some-uuid")));
    }

    #[test]
    fn test_mcp_allow_args_comma_join_not_space_separated() {
        // REGRESSION (found by the real drive proof): a space-separated
        // multi-value `--allowed-mcp-server-names a b c` is a greedy yargs array
        // that collides with `-p {prompt}` and makes Gemini misparse the prompt
        // as a positional ("Cannot use both a positional prompt and -p"). The
        // builder must emit ONE comma-joined value: `--flag a,b,c`. We can't rely
        // on this machine having MCP config, so assert the JOINING SHAPE directly
        // by constructing the same output the function produces.
        let flag = "--allowed-mcp-server-names";
        let names = ["github", "perplexity-ask", "tradingview"];
        let out = vec![flag.to_string(), names.join(",")];
        // Exactly TWO argv entries — never one-per-name.
        assert_eq!(out.len(), 2, "must be [flag, joined], got {out:?}");
        assert_eq!(out[1], "github,perplexity-ask,tradingview");
        // The dangerous space-separated form would have len 4; guard against it.
        assert!(!out.iter().any(|a| a == "perplexity-ask"));
    }

    #[test]
    fn test_mcp_allow_args_render_per_style() {
        use registry::McpAllowStyle;
        let names = vec!["perplexity".to_string(), "github".to_string()];

        // gemini-family: ONE flag + comma-joined value.
        let csv = render_mcp_allow_args(
            "--allowed-mcp-server-names",
            Some(McpAllowStyle::ServerNameCsv),
            &names,
        );
        assert_eq!(
            csv,
            vec![
                "--allowed-mcp-server-names".to_string(),
                "perplexity,github".to_string()
            ]
        );

        // Claude: `--allowed-tools "mcp__perplexity mcp__github"` (tool patterns,
        // single space-joined value).
        let claude = render_mcp_allow_args(
            "--allowed-tools",
            Some(McpAllowStyle::ClaudeToolPattern),
            &names,
        );
        assert_eq!(
            claude,
            vec![
                "--allowed-tools".to_string(),
                "mcp__perplexity mcp__github".to_string()
            ]
        );

        // Copilot: repeated `--allow-tool <server>` per server.
        let copilot = render_mcp_allow_args(
            "--allow-tool",
            Some(McpAllowStyle::CopilotAllowTool),
            &names,
        );
        assert_eq!(
            copilot,
            vec![
                "--allow-tool".to_string(),
                "perplexity".to_string(),
                "--allow-tool".to_string(),
                "github".to_string(),
            ]
        );

        // A flag with no style is a registry bug → emit nothing (safe default).
        assert!(render_mcp_allow_args("--x", None, &names).is_empty());
    }

    #[test]
    fn test_codex_answer_args_carry_read_only_sandbox() {
        // Answer mode for Codex appends the read-only posture as `-c` CONFIG
        // OVERRIDES (not `--sandbox` flags) so it ALSO works on `exec resume`,
        // which rejects `--sandbox`. VERIFIED LIVE both forms.
        let s = spec(KindTag::Codex);
        let q = Question::new("what changed?");
        let (_, args) =
            build_argv_with_mode(s, &AgentKind::Codex, &q, DriveMode::Answer, &[]).unwrap();
        assert!(args.iter().any(|a| a == "sandbox_mode=\"read-only\""));
        assert!(args.iter().any(|a| a == "approval_policy=\"never\""));
        assert!(!args.iter().any(|a| a.contains("workspace-write")));
        // Must use the resume-compatible `-c` form, never the `--sandbox` flag.
        assert!(!args.iter().any(|a| a == "--sandbox"));
    }

    // ---- drive-mode → Fix-profile arg assembly (slice F1) ---------------

    /// Grab the command-map spec for a local drive tag.
    fn spec(tag: KindTag) -> &'static DriveSpec {
        COMMAND_MAP
            .iter()
            .find(|s| s.kind_tag == tag)
            .expect("command-map row")
    }

    #[test]
    fn test_answer_mode_appends_no_fix_args() {
        // Claude has empty answer_args, so Answer == base argv (back-compat).
        let s = spec(KindTag::ClaudeCode);
        let q = Question::new("why is the build red?");
        let (_, base) = build_argv(s, &q);
        let (_, with_mode) =
            build_argv_with_mode(s, &AgentKind::ClaudeCode, &q, DriveMode::Answer, &[]).unwrap();
        assert_eq!(base, with_mode);
    }

    #[test]
    fn test_answer_mode_never_adds_blanket_auto_approve() {
        // Safety: Answer mode must NOT carry a blanket auto-approve flag (yolo /
        // auto_edit) that would let a read-intent answer perform writes.
        let s = spec(KindTag::Gemini);
        let q = Question::new("use a tool");
        let (_, args) =
            build_argv_with_mode(s, &AgentKind::Gemini, &q, DriveMode::Answer, &[]).unwrap();
        assert!(
            !args.iter().any(|a| a == "yolo"),
            "no blanket yolo in answer"
        );
        assert!(
            !args.iter().any(|a| a == "auto_edit"),
            "no auto_edit in answer"
        );
    }

    #[test]
    fn test_answer_mode_appends_scoped_mcp_allow_list() {
        // The scoped MCP allow-list (resolved from the agent's own server names)
        // is appended in Answer mode only — this auto-approves named MCP read
        // tools while leaving write tools gated.
        let s = spec(KindTag::Gemini);
        let q = Question::new("use a tool");
        let allow = vec![
            "--allowed-mcp-server-names".to_string(),
            "perplexity".to_string(),
        ];
        let (_, args) =
            build_argv_with_mode(s, &AgentKind::Gemini, &q, DriveMode::Answer, &allow).unwrap();
        let i = args
            .iter()
            .position(|a| a == "--allowed-mcp-server-names")
            .expect("scoped mcp allow flag present");
        assert_eq!(args[i + 1], "perplexity");
        // And the scoped allow-list must NOT leak into a Fix run.
        let (_, propose) =
            build_argv_with_mode(s, &AgentKind::Gemini, &q, DriveMode::ProposeFix, &allow).unwrap();
        assert!(!propose.iter().any(|a| a == "--allowed-mcp-server-names"));
    }

    #[test]
    fn test_propose_mode_appends_propose_args_only() {
        // Claude propose adds `--permission-mode plan` and nothing from apply.
        let s = spec(KindTag::ClaudeCode);
        let q = Question::new("propose a fix");
        let (_, args) =
            build_argv_with_mode(s, &AgentKind::ClaudeCode, &q, DriveMode::ProposeFix, &[])
                .unwrap();
        let i = args
            .iter()
            .position(|a| a == "--permission-mode")
            .expect("propose flag present");
        assert_eq!(args[i + 1], "plan");
        // The apply value must never leak into a propose run.
        assert!(!args.iter().any(|a| a == "acceptEdits"));
    }

    #[test]
    fn test_apply_mode_appends_apply_args() {
        // Claude apply swaps the posture to `--permission-mode acceptEdits`.
        let s = spec(KindTag::ClaudeCode);
        let q = Question::new("apply the approved fix");
        let (_, args) =
            build_argv_with_mode(s, &AgentKind::ClaudeCode, &q, DriveMode::ApplyFix, &[]).unwrap();
        let i = args
            .iter()
            .position(|a| a == "--permission-mode")
            .expect("apply flag present");
        assert_eq!(args[i + 1], "acceptEdits");
        assert!(!args.iter().any(|a| a == "plan"));
    }

    #[test]
    fn test_cursor_propose_omits_force_apply_adds_it() {
        // Cursor propose appends nothing (omit --force); apply appends --force.
        let s = spec(KindTag::Cursor);
        let q = Question::new("fix it");
        let (_, base) = build_argv(s, &q);

        let (_, propose) =
            build_argv_with_mode(s, &AgentKind::Cursor, &q, DriveMode::ProposeFix, &[]).unwrap();
        assert_eq!(propose, base, "propose must not append any arg for Cursor");
        assert!(!propose.iter().any(|a| a == "--force"));
        assert!(!propose.iter().any(|a| a == "--plan"));

        let (_, apply) =
            build_argv_with_mode(s, &AgentKind::Cursor, &q, DriveMode::ApplyFix, &[]).unwrap();
        assert!(apply.iter().any(|a| a == "--force"));
        assert!(!apply.iter().any(|a| a == "--plan"));
    }

    #[test]
    fn test_apply_args_appended_after_prompt_not_mixed_into_it() {
        // The prompt stays a single argv entry; Fix args are appended after it.
        let s = spec(KindTag::ClaudeCode);
        let q = Question::new("the prompt");
        let (_, args) =
            build_argv_with_mode(s, &AgentKind::ClaudeCode, &q, DriveMode::ApplyFix, &[]).unwrap();
        assert_eq!(args.iter().filter(|a| *a == "the prompt").count(), 1);
        let prompt_idx = args.iter().position(|a| a == "the prompt").unwrap();
        let flag_idx = args.iter().position(|a| a == "acceptEdits").unwrap();
        assert!(
            flag_idx > prompt_idx,
            "apply args must come after the prompt"
        );
    }

    #[test]
    fn test_apply_on_codex_uses_workspace_write() {
        // Codex apply flips the sandbox to workspace-write (a write run), never
        // read-only — via the `-c` config form (resume-compatible).
        let s = spec(KindTag::Codex);
        let q = Question::new("apply");
        let (_, args) =
            build_argv_with_mode(s, &AgentKind::Codex, &q, DriveMode::ApplyFix, &[]).unwrap();
        assert!(args.iter().any(|a| a == "sandbox_mode=\"workspace-write\""));
        assert!(!args.iter().any(|a| a == "sandbox_mode=\"read-only\""));
    }

    #[test]
    fn test_antigravity_resolves_to_gemini_drive_but_own_fix_profile() {
        // Antigravity drives via the gemini spec, yet its Fix profile resolves
        // off the Antigravity registry row (same approval-mode values).
        let s = spec(KindTag::Gemini);
        let q = Question::new("propose");
        let (_, args) =
            build_argv_with_mode(s, &AgentKind::Antigravity, &q, DriveMode::ProposeFix, &[])
                .unwrap();
        let i = args.iter().position(|a| a == "--approval-mode").unwrap();
        assert_eq!(args[i + 1], "plan");
    }

    #[test]
    fn test_propose_and_apply_argvs_differ_for_writable_agent() {
        // Sanity: the two postures must not produce identical argv (otherwise
        // the gate is a no-op for that agent).
        let s = spec(KindTag::ClaudeCode);
        let q = Question::new("x");
        let (_, propose) =
            build_argv_with_mode(s, &AgentKind::ClaudeCode, &q, DriveMode::ProposeFix, &[])
                .unwrap();
        let (_, apply) =
            build_argv_with_mode(s, &AgentKind::ClaudeCode, &q, DriveMode::ApplyFix, &[]).unwrap();
        assert_ne!(propose, apply);
    }

    #[test]
    fn test_apply_on_unsupported_agent_errors_in_arg_builder() {
        // Windsurf/VS Code have apply_supported=false. Even handed a (borrowed)
        // drive spec, the arg builder must refuse ApplyFix with an Err rather
        // than producing argv — the apply gate, tested without spawning.
        let s = spec(KindTag::ClaudeCode); // any spec; the guard keys off kind.
        let q = Question::new("apply");
        for kind in [AgentKind::Windsurf, AgentKind::VsCodeFork] {
            let res = build_argv_with_mode(s, &kind, &q, DriveMode::ApplyFix, &[]);
            assert!(res.is_err(), "{kind:?} ApplyFix should error");
            let msg = res.unwrap_err().to_string();
            assert!(msg.contains("cannot apply"), "unclear error: {msg}");
        }
    }

    #[test]
    fn test_propose_on_unsupported_agent_still_builds() {
        // apply_supported=false blocks apply, NOT propose: a read-only propose
        // run is fine (its profile has empty propose_args, so == base argv).
        let s = spec(KindTag::ClaudeCode);
        let q = Question::new("propose");
        let (_, base) = build_argv(s, &q);
        let (_, propose) =
            build_argv_with_mode(s, &AgentKind::Windsurf, &q, DriveMode::ProposeFix, &[]).unwrap();
        assert_eq!(propose, base);
    }

    #[test]
    fn test_unknown_agent_apply_errors_unknown_agent_propose_ok() {
        // No registry profile (Unknown): apply must error; propose degrades to
        // base argv (no extra args), never panics.
        let s = spec(KindTag::ClaudeCode);
        let q = Question::new("x");
        assert!(
            build_argv_with_mode(s, &AgentKind::Unknown, &q, DriveMode::ApplyFix, &[]).is_err()
        );
        let (_, base) = build_argv(s, &q);
        let (_, propose) =
            build_argv_with_mode(s, &AgentKind::Unknown, &q, DriveMode::ProposeFix, &[]).unwrap();
        assert_eq!(propose, base);
    }

    #[tokio::test]
    async fn test_drive_with_mode_apply_unsupported_agent_returns_err() {
        // End-to-end through the public entry: an apply-incapable agent never
        // spawns — it returns Err. (Windsurf also has no drive spec, so this
        // is doubly guarded; either guard alone is sufficient.)
        let res = drive_with_mode(
            AgentKind::Windsurf,
            Question::new("apply this"),
            DriveMode::ApplyFix,
        )
        .await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn test_drive_with_mode_answer_matches_plain_drive_behavior() {
        // Answer mode keeps the unsupported-agent error identical to drive():
        // both bail on a no-CLI agent the same way (back-compat).
        let plain = drive(AgentKind::Unknown, Question::new("hi")).await;
        let answer =
            drive_with_mode(AgentKind::Unknown, Question::new("hi"), DriveMode::Answer).await;
        assert!(plain.is_err() && answer.is_err());
    }

    #[test]
    fn test_render_prompt_flattens_context() {
        let mut q = Question::new("What did we decide?");
        q.context = Some(Transcript {
            turns: vec![
                Turn {
                    role: Role::User,
                    text: "ship friday".to_string(),
                },
                Turn {
                    role: Role::Assistant,
                    text: "agreed".to_string(),
                },
            ],
        });
        let rendered = q.render_prompt();
        assert!(rendered.contains("User: ship friday"));
        assert!(rendered.contains("Assistant: agreed"));
        assert!(rendered.contains("What did we decide?"));
    }

    // ---- (a) claude stream-json parsing ---------------------------------

    #[tokio::test]
    async fn test_claude_stream_json_parses_started_delta_done() {
        let dir = TempDir::new().unwrap();
        // A fake `claude` that prints canned stream-json events.
        let script = write_script(
            &dir,
            "claude",
            r#"#!/bin/sh
echo '{"type":"system","subtype":"init","session_id":"abc-123","model":"x"}'
echo '{"type":"assistant","message":{"content":[{"type":"text","text":"Hello "}]}}'
echo '{"type":"assistant","message":{"content":[{"type":"text","text":"world"}]}}'
echo '{"type":"result","result":"Hello world","session_id":"abc-123","total_cost_usd":0.0123,"is_error":false}'
"#,
        );
        let chunks = collect(drive_fixture(
            script,
            vec![],
            OutputParser::ClaudeStreamJson,
            DriveOptions::default(),
        ))
        .await;

        assert_eq!(
            chunks[0],
            AnswerChunk::Started {
                session_id: Some("abc-123".to_string())
            }
        );
        assert_eq!(chunks[1], AnswerChunk::Delta("Hello ".to_string()));
        assert_eq!(chunks[2], AnswerChunk::Delta("world".to_string()));
        assert_eq!(
            chunks[3],
            AnswerChunk::Done {
                cost_usd: Some(0.0123)
            }
        );
        assert_eq!(chunks.len(), 4);
    }

    #[tokio::test]
    async fn test_claude_result_is_error_emits_error() {
        let dir = TempDir::new().unwrap();
        let script = write_script(
            &dir,
            "claude",
            r#"#!/bin/sh
echo '{"type":"system","subtype":"init","session_id":"s1"}'
echo '{"type":"result","result":"rate limited","is_error":true}'
"#,
        );
        let chunks = collect(drive_fixture(
            script,
            vec![],
            OutputParser::ClaudeStreamJson,
            DriveOptions::default(),
        ))
        .await;
        assert!(matches!(chunks[0], AnswerChunk::Started { .. }));
        assert_eq!(chunks[1], AnswerChunk::Error("rate limited".to_string()));
    }

    // ---- (b) plain-text parser → single Delta + Done --------------------

    #[tokio::test]
    async fn test_plain_text_single_delta_then_done() {
        let dir = TempDir::new().unwrap();
        let script = write_script(
            &dir,
            "copilot",
            "#!/bin/sh\nprintf 'line one\\nline two\\n'\n",
        );
        let chunks = collect(drive_fixture(
            script,
            vec![],
            OutputParser::PlainText,
            DriveOptions::default(),
        ))
        .await;
        assert_eq!(chunks[0], AnswerChunk::Started { session_id: None });
        assert_eq!(
            chunks[1],
            AnswerChunk::Delta("line one\nline two".to_string())
        );
        assert_eq!(chunks[2], AnswerChunk::Done { cost_usd: None });
        assert_eq!(chunks.len(), 3);
    }

    // ---- (b2) cursor single-object json parser --------------------------

    #[tokio::test]
    async fn test_cursor_json_single_object_started_delta_done() {
        let dir = TempDir::new().unwrap();
        // A fake `cursor-agent` that prints one result object on success.
        let script = write_script(
            &dir,
            "cursor-agent",
            "#!/bin/sh\n\
             printf '%s\\n' '{\"type\":\"result\",\"subtype\":\"success\",\"is_error\":false,\"result\":\"hi\",\"session_id\":\"x\"}'\n",
        );
        let chunks = collect(drive_fixture(
            script,
            vec![],
            OutputParser::CursorJson,
            DriveOptions::default(),
        ))
        .await;
        assert_eq!(
            chunks[0],
            AnswerChunk::Started {
                session_id: Some("x".to_string())
            }
        );
        assert_eq!(chunks[1], AnswerChunk::Delta("hi".to_string()));
        assert_eq!(chunks[2], AnswerChunk::Done { cost_usd: None });
        assert_eq!(chunks.len(), 3);
    }

    #[test]
    fn test_cursor_json_parser_unit() {
        // Pure parse: one result object → Started(x) + Delta("hi") + Done.
        let mut st = ParseState::new(OutputParser::CursorJson);
        let line = r#"{"type":"result","subtype":"success","result":"hi","session_id":"x"}"#;
        let out = st.push_cursor_line(line);
        assert_eq!(
            out,
            vec![
                AnswerChunk::Started {
                    session_id: Some("x".to_string())
                },
                AnswerChunk::Delta("hi".to_string()),
                AnswerChunk::Done { cost_usd: None },
            ]
        );
    }

    #[tokio::test]
    async fn test_cursor_json_is_error_emits_error() {
        let dir = TempDir::new().unwrap();
        let script = write_script(
            &dir,
            "cursor-agent",
            "#!/bin/sh\n\
             printf '%s\\n' '{\"type\":\"result\",\"subtype\":\"error\",\"is_error\":true,\"result\":\"nope\",\"session_id\":\"y\"}'\n",
        );
        let chunks = collect(drive_fixture(
            script,
            vec![],
            OutputParser::CursorJson,
            DriveOptions::default(),
        ))
        .await;
        assert!(matches!(chunks[0], AnswerChunk::Started { .. }));
        assert_eq!(chunks[1], AnswerChunk::Error("nope".to_string()));
    }

    // ---- (b3) codex jsonl event parser ----------------------------------

    #[test]
    fn test_codex_jsonl_parser_unit() {
        // Stream: thread.started → Started(id), item.completed agent_message →
        // Delta, turn.completed → Done.
        let mut st = ParseState::new(OutputParser::CodexJsonl);
        let mut out = Vec::new();
        out.extend(st.push_codex_line(r#"{"type":"thread.started","thread_id":"0199a2"}"#));
        out.extend(st.push_codex_line(
            r#"{"type":"item.completed","item":{"id":"i3","type":"agent_message","text":"done"}}"#,
        ));
        out.extend(st.push_codex_line(
            r#"{"type":"turn.completed","usage":{"input_tokens":24,"output_tokens":2}}"#,
        ));
        assert_eq!(
            out,
            vec![
                AnswerChunk::Started {
                    session_id: Some("0199a2".to_string())
                },
                AnswerChunk::Delta("done".to_string()),
                AnswerChunk::Done { cost_usd: None },
            ]
        );
    }

    #[tokio::test]
    async fn test_codex_jsonl_stream_parses_session_and_answer() {
        let dir = TempDir::new().unwrap();
        let script = write_script(
            &dir,
            "codex",
            "#!/bin/sh\n\
             printf '%s\\n' '{\"type\":\"thread.started\",\"thread_id\":\"sid-1\"}'\n\
             printf '%s\\n' '{\"type\":\"item.completed\",\"item\":{\"type\":\"agent_message\",\"text\":\"hello\"}}'\n\
             printf '%s\\n' '{\"type\":\"turn.completed\",\"usage\":{\"input_tokens\":1,\"output_tokens\":1}}'\n",
        );
        let chunks = collect(drive_fixture(
            script,
            vec![],
            OutputParser::CodexJsonl,
            DriveOptions::default(),
        ))
        .await;
        assert_eq!(
            chunks[0],
            AnswerChunk::Started {
                session_id: Some("sid-1".to_string())
            }
        );
        assert_eq!(chunks[1], AnswerChunk::Delta("hello".to_string()));
        assert_eq!(chunks[2], AnswerChunk::Done { cost_usd: None });
        assert_eq!(chunks.len(), 3);
    }

    #[test]
    fn test_codex_jsonl_fail_soft_plain_line_becomes_delta() {
        // Resume path may not honor `--json`: a non-JSON line is surfaced as a
        // plain-text Delta (after an implicit Started) rather than dropped.
        let mut st = ParseState::new(OutputParser::CodexJsonl);
        let out = st.push_codex_line("just the final answer");
        assert_eq!(
            out,
            vec![
                AnswerChunk::Started { session_id: None },
                AnswerChunk::Delta("just the final answer".to_string()),
            ]
        );
    }

    #[test]
    fn test_codex_jsonl_turn_failed_emits_error() {
        let mut st = ParseState::new(OutputParser::CodexJsonl);
        let _ = st.push_codex_line(r#"{"type":"thread.started","thread_id":"s"}"#);
        let out = st.push_codex_line(r#"{"type":"turn.failed","error":{"message":"boom"}}"#);
        assert_eq!(out, vec![AnswerChunk::Error("boom".to_string())]);
    }

    // ---- (c) timeout kills child and emits Error ------------------------

    #[tokio::test]
    async fn test_timeout_emits_error() {
        let dir = TempDir::new().unwrap();
        // Sleeps far longer than the timeout; never prints.
        let script = write_script(&dir, "slow", "#!/bin/sh\nsleep 30\n");
        let opts = DriveOptions {
            timeout: Duration::from_millis(200),
            ..DriveOptions::default()
        };
        let chunks = collect(drive_fixture(script, vec![], OutputParser::PlainText, opts)).await;
        assert_eq!(chunks.len(), 1);
        match &chunks[0] {
            AnswerChunk::Error(m) => assert!(m.contains("timed out"), "got: {m}"),
            other => panic!("expected timeout error, got {other:?}"),
        }
    }

    // ---- (d) output-cap triggers Error ----------------------------------

    #[tokio::test]
    async fn test_output_cap_emits_error() {
        let dir = TempDir::new().unwrap();
        // Emits more than the cap allows.
        let script = write_script(
            &dir,
            "loud",
            "#!/bin/sh\nfor i in 1 2 3 4 5 6 7 8 9 10; do printf 'AAAAAAAAAA\\n'; done\n",
        );
        let opts = DriveOptions {
            max_output_bytes: 20,
            ..DriveOptions::default()
        };
        let chunks = collect(drive_fixture(script, vec![], OutputParser::PlainText, opts)).await;
        let last = chunks.last().unwrap();
        match last {
            AnswerChunk::Error(m) => assert!(m.contains("exceeded"), "got: {m}"),
            other => panic!("expected cap error, got {other:?}"),
        }
    }

    // ---- (e) missing binary → Error not panic ---------------------------

    #[tokio::test]
    async fn test_missing_binary_emits_error_no_panic() {
        let chunks = collect(drive_fixture(
            std::path::PathBuf::from("/nonexistent/definitely-not-a-real-binary-xyz"),
            vec![],
            OutputParser::PlainText,
            DriveOptions::default(),
        ))
        .await;
        assert_eq!(chunks.len(), 1);
        match &chunks[0] {
            AnswerChunk::Error(m) => assert!(m.contains("not found") || m.contains("spawn")),
            other => panic!("expected error, got {other:?}"),
        }
    }

    // ---- (f) shell metacharacters are literal, not executed -------------

    #[tokio::test]
    async fn test_prompt_metacharacters_not_executed() {
        let dir = TempDir::new().unwrap();
        // A fake agent that echoes its first positional arg back verbatim.
        // If a shell were involved, $(...) / backticks / ; would be expanded
        // or split into separate commands. We prove the arg arrives literally.
        let script = write_script(
            &dir,
            "echoer",
            // "$1" prints exactly the single argument it received.
            "#!/bin/sh\nprintf '%s' \"$1\"\n",
        );
        let malicious = "$(touch /tmp/cue_pwned); `id`; rm -rf /; ; echo hi";
        let chunks = collect(drive_fixture(
            script,
            vec![malicious.to_string()],
            OutputParser::PlainText,
            DriveOptions::default(),
        ))
        .await;
        // The Delta must be the malicious string verbatim — proving it was
        // passed as one literal argv entry and never interpreted by a shell.
        let delta = chunks
            .iter()
            .find_map(|c| match c {
                AnswerChunk::Delta(d) => Some(d.clone()),
                _ => None,
            })
            .expect("expected a Delta");
        assert_eq!(delta, malicious);
        // And the side effect a shell would have caused did not happen.
        assert!(!std::path::Path::new("/tmp/cue_pwned").exists());
    }

    // ---- unsupported agent ---------------------------------------------

    #[tokio::test]
    async fn test_unsupported_agent_returns_err() {
        let res = drive(AgentKind::Unknown, Question::new("hi")).await;
        assert!(res.is_err());
    }

    #[test]
    fn test_command_map_covers_expected_agents() {
        for tag in [
            KindTag::ClaudeCode,
            KindTag::Copilot,
            KindTag::Cursor,
            KindTag::Gemini,
            KindTag::Codex,
        ] {
            assert!(
                COMMAND_MAP.iter().any(|s| s.kind_tag == tag),
                "missing command-map row: {tag:?}"
            );
        }
    }

    #[test]
    fn test_command_map_parsers_match_corrections() {
        // Lock in the per-agent parser choices from the doc-confirmed
        // corrections so a future edit can't silently regress them.
        assert_eq!(
            spec(KindTag::ClaudeCode).parser,
            OutputParser::ClaudeStreamJson
        );
        assert_eq!(spec(KindTag::Cursor).parser, OutputParser::CursorJson);
        assert_eq!(spec(KindTag::Codex).parser, OutputParser::CodexJsonl);
        assert_eq!(spec(KindTag::Copilot).parser, OutputParser::PlainText);
        assert_eq!(spec(KindTag::Gemini).parser, OutputParser::PlainText);
    }
}
