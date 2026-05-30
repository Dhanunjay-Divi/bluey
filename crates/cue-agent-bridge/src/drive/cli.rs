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

use super::{AnswerChunk, AnswerStream, Driver, Question};
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
    /// Plain text on stdout: emit the whole captured output as one Delta, then
    /// Done. Used by copilot / cursor-agent / gemini / codex.
    PlainText,
}

/// One row of the per-agent command map: how to invoke an agent's CLI.
///
/// Templates use the marker `{prompt}` for where the prompt argv entry goes and
/// `{id}` for a resume session id. Markers are replaced as **whole argv
/// entries** — never substituted into a larger string — so the prompt cannot
/// leak into adjacent flags.
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
            AgentKind::ClaudeCode => Some(KindTag::ClaudeCode),
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
/// | Copilot | `copilot` | `-p {prompt}` | `--continue` | plain text |
/// | Cursor | `cursor-agent` | `-p {prompt}` | (none) | plain text |
/// | Gemini | `gemini` | `-p {prompt}` | (none) | plain text |
/// | Codex | `codex` | `exec {prompt}` | `exec resume --last` | plain text |
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
        oneshot_args: &["-p", "{prompt}"],
        resume_args: &["--continue"],
        parser: OutputParser::PlainText,
    },
    DriveSpec {
        kind_tag: KindTag::Cursor,
        binary: "cursor-agent",
        oneshot_args: &["-p", "{prompt}"],
        resume_args: &[],
        parser: OutputParser::PlainText,
    },
    DriveSpec {
        kind_tag: KindTag::Gemini,
        binary: "gemini",
        oneshot_args: &["-p", "{prompt}"],
        resume_args: &[],
        parser: OutputParser::PlainText,
    },
    DriveSpec {
        kind_tag: KindTag::Codex,
        binary: "codex",
        // Codex resume replaces `exec <prompt>` with `exec resume --last`,
        // so the one-shot form is `exec {prompt}` and resume is handled by
        // `build_argv` swapping the prompt entry for the resume args.
        oneshot_args: &["exec", "{prompt}"],
        resume_args: &["exec", "resume", "--last"],
        parser: OutputParser::PlainText,
    },
];

/// Look up the drive spec for an agent kind, if it is CLI-drivable.
fn spec_for(kind: &AgentKind) -> Option<&'static DriveSpec> {
    let tag = KindTag::from_agent_kind(kind)?;
    COMMAND_MAP.iter().find(|s| s.kind_tag == tag)
}

/// Tunable limits for a single drive. Defaults are production-safe; callers
/// (the daemon) may tighten them.
#[derive(Debug, Clone, Copy)]
pub struct DriveOptions {
    /// Wall-clock timeout before the child is killed.
    pub timeout: Duration,
    /// Hard cap on total stdout bytes read.
    pub max_output_bytes: usize,
}

impl Default for DriveOptions {
    fn default() -> Self {
        Self {
            timeout: DEFAULT_TIMEOUT,
            max_output_bytes: DEFAULT_MAX_OUTPUT_BYTES,
        }
    }
}

/// Build the full argv (binary + args) for a question, substituting the prompt
/// and any resume id as **whole entries**.
///
/// Returns `(program, args)`. The prompt is never concatenated into a flag, so
/// shell metacharacters in the prompt are inert.
fn build_argv(spec: &DriveSpec, q: &Question) -> (String, Vec<String>) {
    let prompt = q.render_prompt();
    let resuming = q.resume.is_some();

    // Codex is special: resume *replaces* the `exec <prompt>` form entirely
    // with `exec resume --last` rather than appending a flag.
    if spec.kind_tag == KindTag::Codex && resuming {
        let args = spec.resume_args.iter().map(|s| s.to_string()).collect();
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
            if *tok == "{id}" {
                args.push(id.to_string());
            } else {
                args.push((*tok).to_string());
            }
        }
    }

    (spec.binary.to_string(), args)
}

/// Drive an agent with default [`DriveOptions`]. See [`drive_with_options`].
pub async fn drive(agent: AgentKind, question: Question) -> Result<AnswerStream> {
    drive_with_options(agent, question, DriveOptions::default()).await
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
    opts: DriveOptions,
) -> Result<AnswerStream> {
    let spec = match spec_for(&agent) {
        Some(s) => *s,
        None => {
            anyhow::bail!("agent {agent:?} has no CLI drive command");
        }
    };

    let (program, args) = build_argv(&spec, &question);

    // Log shape only — never the prompt text (it may carry meeting content).
    tracing::debug!(
        binary = %program,
        argc = args.len(),
        prompt_len = question.prompt.len(),
        resuming = question.resume.is_some(),
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
            OutputParser::ClaudeStreamJson => {
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

    /// Terminal chunks after stdout EOF. If the CLI never emitted a `result`
    /// event we still close the stream so consumers are not left hanging:
    /// emit a `Started` if none was seen, then a `Done` with no cost.
    fn finish(&mut self) -> Vec<AnswerChunk> {
        debug_assert_eq!(self.parser, OutputParser::ClaudeStreamJson);
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
#[derive(Debug, Clone, Copy, Default)]
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
        drive_with_options(agent, question, self.opts).await
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
    fn test_build_argv_codex_resume_replaces_prompt() {
        let spec = COMMAND_MAP
            .iter()
            .find(|s| s.kind_tag == KindTag::Codex)
            .unwrap();
        let mut q = Question::new("ignored when resuming");
        q.resume = Some("whatever".to_string());
        let (prog, args) = build_argv(spec, &q);
        assert_eq!(prog, "codex");
        assert_eq!(args, vec!["exec", "resume", "--last"]);
        assert!(!args.iter().any(|a| a.contains("ignored")));
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
}
