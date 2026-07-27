//! A thin client wrapper over the official [`agent_client_protocol`] SDK.
//!
//! [`AcpClient`] spawns a coding agent as a long-lived ACP subprocess (JSON-RPC
//! 2.0 over stdio), runs the protocol handshake (`initialize` → `session/new`),
//! sends a prompt, and exposes the agent's streaming response as the crate's
//! existing [`AnswerStream`] of [`AnswerChunk`]s — the *same* stream type the
//! rest of Bluey already consumes from the legacy CLI driver. Nothing in here
//! hand-rolls JSON-RPC: framing, id allocation, request/response correlation,
//! and notification dispatch are all handled by the SDK's
//! [`agent_client_protocol::Client`] builder and its
//! [`agent_client_protocol::AcpAgent`] stdio transport (which also kills the
//! child on drop and surfaces stderr on a non-zero exit).
//!
//! # Lifetime model (why one drive == one connection)
//!
//! The SDK's [`connect_with`] runs the *entire* client session inside a single
//! async closure: the call does not return until that closure finishes, at
//! which point the connection (and the subprocess) shut down. That is a
//! deliberate "drive the whole turn in one scope" design, not a "get a handle,
//! call methods on it later" design.
//!
//! So [`AcpClient`] is a cheap, reusable *spec* (binary + args + cwd), and each
//! [`AcpClient::prompt`] / [`AcpClient::resume`] call performs one full
//! connection: spawn → initialize → new/load session → prompt → stream until
//! the turn ends, then tear down. This mirrors exactly how the legacy
//! [`crate::drive::cli`] runner works (one drive = one process = one stream),
//! so it slots into Bluey's answer ladder with no contract change. Multi-turn
//! reuse of a single living subprocess is a deliberate non-goal for Phase 1
//! (see the PHASE 2 note at the bottom).
//!
//! [`connect_with`]: agent_client_protocol::Builder::connect_with

use std::path::PathBuf;
use std::pin::Pin;

use agent_client_protocol::schema::{
    ContentBlock, InitializeRequest, LoadSessionRequest, NewSessionResponse, ProtocolVersion,
    RequestPermissionOutcome, RequestPermissionRequest, RequestPermissionResponse,
    SelectedPermissionOutcome, SessionNotification, SessionUpdate, StopReason,
};
use agent_client_protocol::{AcpAgent, Client, Responder};
use futures::channel::mpsc;
use futures::Stream;

use crate::drive::{AnswerChunk, AnswerStream, ToolStatus};

/// How to launch an agent as an ACP subprocess: the executable plus its args.
///
/// This is intentionally an argv array (program + args), never a shell string —
/// the legacy CLI driver enforces the same posture (no shell, no command
/// injection). The SDK's [`AcpAgent::from_args`] consumes exactly this shape.
///
/// # PHASE 2
/// The per-agent binary mapping plugs in *here*: a `From<AgentKind> for
/// AcpAgentSpec` (or a registry lookup keyed by [`crate::AgentKind`]) that knows
/// each installed agent's ACP entrypoint — e.g. Claude Code via
/// `npx @zed-industries/claude-code-acp`, Gemini via
/// `gemini --experimental-acp`, etc. The SDK even ships convenience
/// constructors for the common ones ([`AcpAgent::zed_claude_code`],
/// [`AcpAgent::google_gemini`], [`AcpAgent::zed_codex`]). Phase 1 deliberately
/// takes the spec explicitly so the protocol plumbing can be tested against any
/// agent without baking in a vendor map.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcpAgentSpec {
    /// The agent executable (looked up on `PATH` if not absolute).
    pub program: String,
    /// Arguments passed to the executable, in order.
    pub args: Vec<String>,
}

impl AcpAgentSpec {
    /// Construct a spec from a program and its arguments.
    #[must_use]
    pub fn new(program: impl Into<String>, args: Vec<String>) -> Self {
        Self {
            program: program.into(),
            args,
        }
    }

    /// Build the SDK's transport component (which spawns the subprocess).
    ///
    /// Uses [`AcpAgent::from_args`]: leading `NAME=value` argv entries become the
    /// child's environment variables, then the program and its args follow (no
    /// shell).
    ///
    /// The child normally inherits this process's environment. The daemon recovers
    /// the user's real shell `PATH`/env at startup (via `fix_path_env`) so that —
    /// in an app launched from Finder/Dock — agent binaries resolve the same way
    /// they do in the user's terminal.
    ///
    /// That is NOT sufficient for runtime-version-sensitive agents, though: the
    /// **GitHub Copilot CLI hard-requires Node ≥ 24**, but if an older `node` sits
    /// ahead of a satisfying one on `PATH`, Copilot's npm loader picks the wrong
    /// one and exits with *"requires Node.js v24 … Currently using v23"* before the
    /// ACP handshake. The legacy CLI driver already guards this by probing the
    /// binary and prepending a satisfying runtime's bin dir to the child's `PATH`
    /// ([`crate::runtime_resolve::runtime_path_for_program`]); the ACP spawn must
    /// do the same, or Node-pinned agents fail only over ACP. So we apply the same
    /// resolution and pass the corrected `PATH` as a leading `PATH=…` argv entry
    /// (consumed by `from_args` as a child env var). When the program launches
    /// cleanly under the inherited `PATH` (the common case) the probe returns
    /// `None` and argv is unchanged — the recovered shell env stays the source of
    /// truth.
    /// # MCP / the USP — do NOT add settingSources flags here
    /// The driven agent must use the USER's own MCP connectors (the product USP).
    /// We deliberately pass NO `settingSources`/`--mcp-config` flags to the
    /// adapters: the `claude-agent-acp` adapter ALREADY hardcodes
    /// `settingSources: ["user", "project", "local"]` internally (verified in its
    /// `acp-agent.js`), so it loads `~/.claude.json`'s user MCP servers on its
    /// own. Live-verified: Claude over ACP fires `mcp__perplexity__perplexity_ask`
    /// from the user's config. Passing a wrong `settingSources` value here could
    /// make the adapter refuse to start — so leave MCP scoping to the adapter.
    fn to_acp_agent(&self) -> anyhow::Result<AcpAgent> {
        let mut argv: Vec<String> = Vec::with_capacity(self.args.len() + 2);
        if let Some(child_path) = crate::runtime_resolve::runtime_path_for_program(&self.program) {
            argv.push(format!("PATH={child_path}"));
        }
        argv.push(self.program.clone());
        argv.extend(self.args.iter().cloned());
        AcpAgent::from_args(argv)
            .map_err(|e| anyhow::anyhow!("failed to build ACP agent transport: {e}"))
    }
}

/// A reusable handle describing *how* to drive an agent over ACP.
///
/// Cheap to clone and hold; the subprocess is only spawned when [`prompt`] or
/// [`resume`] is called.
///
/// [`prompt`]: AcpClient::prompt
/// [`resume`]: AcpClient::resume
#[derive(Debug, Clone)]
pub struct AcpClient {
    spec: AcpAgentSpec,
    /// Working directory for the session. Defaults to the daemon's cwd when
    /// `None`; should be set to the project path for correct, scoped behavior
    /// (and is *required* by ACP's `session/load` and `session/resume`, whose
    /// `cwd` must match the original session's).
    cwd: Option<PathBuf>,
    /// Client name advertised to the agent at `initialize`.
    client_name: String,
}

impl AcpClient {
    /// Create a client for the given agent spec.
    #[must_use]
    pub fn new(spec: AcpAgentSpec) -> Self {
        Self {
            spec,
            cwd: None,
            client_name: "bluey".to_string(),
        }
    }

    /// Set the working directory the session runs in. Required for correct
    /// resume/load on cwd-scoped agents.
    #[must_use]
    pub fn with_cwd(mut self, cwd: impl Into<PathBuf>) -> Self {
        self.cwd = Some(cwd.into());
        self
    }

    /// Override the client name advertised at `initialize` (default `"bluey"`).
    #[must_use]
    pub fn with_client_name(mut self, name: impl Into<String>) -> Self {
        self.client_name = name.into();
        self
    }

    /// The cwd to hand to the agent, resolving `None` to the current directory
    /// (the SDK requires an explicit path for `session/new`).
    fn resolved_cwd(&self) -> PathBuf {
        self.cwd
            .clone()
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_else(|| PathBuf::from("/"))
    }

    /// Drive a fresh prompt: spawn the agent, `initialize`, open a **new**
    /// session, send `prompt`, and stream the response.
    ///
    /// Returns immediately with an [`AnswerStream`]; the connection runs on a
    /// spawned task and feeds the stream. Dropping the stream tears the
    /// connection (and subprocess) down.
    ///
    /// The first chunk is always [`AnswerChunk::Started`] carrying the new
    /// session id (so a caller can persist it for a later [`resume`]). The
    /// stream ends with [`AnswerChunk::Done`] on a clean turn, or a terminal
    /// [`AnswerChunk::Error`].
    ///
    /// [`resume`]: AcpClient::resume
    pub fn prompt(&self, prompt: impl Into<String>) -> AnswerStream {
        self.run(prompt.into(), None, Vec::new())
    }

    /// Like [`prompt`], but attaches `images` (local file paths) as real ACP
    /// `ContentBlock::Image` blocks alongside the text — the multimodal "+"-menu
    /// path. Vision-capable agents receive the images natively.
    ///
    /// [`prompt`]: AcpClient::prompt
    pub fn prompt_with_images(
        &self,
        prompt: impl Into<String>,
        images: Vec<std::path::PathBuf>,
    ) -> AnswerStream {
        self.run(prompt.into(), None, images)
    }

    /// Like [`prompt`], but continue an existing session via ACP `session/load`
    /// (which restores prior context) instead of opening a new one.
    ///
    /// `session_id` is the agent-native id previously reported by
    /// [`AnswerChunk::Started`]. The agent must support the `loadSession`
    /// capability; if it does not, the agent rejects the request and the error
    /// surfaces as a terminal [`AnswerChunk::Error`].
    ///
    /// [`prompt`]: AcpClient::prompt
    pub fn resume(&self, session_id: impl Into<String>, prompt: impl Into<String>) -> AnswerStream {
        self.run(prompt.into(), Some(session_id.into()), Vec::new())
    }

    /// Like [`resume`], but attaches `images` as ACP image blocks.
    ///
    /// [`resume`]: AcpClient::resume
    pub fn resume_with_images(
        &self,
        session_id: impl Into<String>,
        prompt: impl Into<String>,
        images: Vec<std::path::PathBuf>,
    ) -> AnswerStream {
        self.run(prompt.into(), Some(session_id.into()), images)
    }

    /// Shared drive path for both [`prompt`] and [`resume`].
    ///
    /// `resume_id == None` → open a new session; `Some(id)` → `session/load`
    /// that id. Builds the channel-backed [`AnswerStream`], spawns the
    /// connection task, and returns the consumer end immediately.
    fn run(
        &self,
        prompt: String,
        resume_id: Option<String>,
        images: Vec<std::path::PathBuf>,
    ) -> AnswerStream {
        let (tx, rx) = mpsc::unbounded::<AnswerChunk>();

        let spec = self.spec.clone();
        let cwd = self.resolved_cwd();
        let client_name = self.client_name.clone();

        // The connection drives the whole turn; it runs on its own task so this
        // call can return the stream eagerly. `connect_with` only returns once
        // the inner closure finishes, so the task naturally lives exactly as
        // long as the turn.
        tokio::spawn(async move {
            let result = drive_connection(
                spec,
                cwd,
                client_name,
                prompt,
                resume_id,
                images,
                tx.clone(),
            )
            .await;

            // The closure inside `drive_connection` already emits a terminal
            // Done/Error in the happy and most error paths. This is the
            // backstop for failures *before* the closure runs (spawn failure,
            // transport setup) or a connection-level error after it: never let
            // the stream end silently.
            if let Err(err) = result {
                // Best-effort: if the receiver is gone the send fails, which is
                // fine — nobody is listening.
                let _ = tx.unbounded_send(AnswerChunk::Error(format!("acp connection: {err}")));
            }
        });

        Box::pin(rx) as Pin<Box<dyn Stream<Item = AnswerChunk> + Send>>
    }
}

/// Run one full ACP connection for a single prompt turn.
///
/// This is the bridge between the SDK's closure-scoped connection model and
/// Bluey's pull-based [`AnswerStream`]: inside [`Client::connect_with`] it
/// handshakes, opens (or loads) a session, sends the prompt, and forwards every
/// `session/update` to `tx` as an [`AnswerChunk`] until the turn's stop reason
/// arrives.
///
/// All session traffic is pulled from the [`ActiveSession`]'s own update
/// channel via [`read_update`], rather than a connection-level
/// `on_receive_notification` handler. That is deliberate: the SDK runs the
/// connection-level (static) handler *before* the per-session (dynamic)
/// handler and short-circuits on the first that claims a message
/// (`incoming_actor`), so a static `SessionNotification` handler would *steal*
/// every `session/update` from the session channel. Reading from one channel
/// keeps text deltas and the terminal stop reason strictly ordered and avoids
/// that hazard. The only static handler we install is for
/// `session/request_permission`, which is a *request* (the agent waits on our
/// response) and is correctly the connection's job, not the session channel's.
///
/// [`ActiveSession`]: agent_client_protocol::ActiveSession
/// [`read_update`]: agent_client_protocol::ActiveSession::read_update
async fn drive_connection(
    spec: AcpAgentSpec,
    cwd: PathBuf,
    client_name: String,
    prompt: String,
    resume_id: Option<String>,
    images: Vec<PathBuf>,
    tx: mpsc::UnboundedSender<AnswerChunk>,
) -> anyhow::Result<()> {
    use agent_client_protocol::util::MatchDispatch;
    use agent_client_protocol::SessionMessage;

    let agent = spec.to_acp_agent()?;

    // Build the attached-image content blocks ONCE (before the connection
    // closure). Each becomes a `ContentBlock::Image` carrying BOTH the base64
    // `data` and the `uri` (file path), so an agent can use whichever it prefers
    // (bytes for inline-vision agents, path for file-reading ones). A file we
    // can't read or whose type isn't a known image is skipped with a warning —
    // never fail the whole turn over one bad attachment.
    let image_blocks: Vec<ContentBlock> = images
        .iter()
        .filter_map(|path| match image_block_from_path(path) {
            Some(block) => Some(block),
            None => {
                tracing::warn!(path = %path.display(), "acp: skipping unreadable/unsupported image attachment");
                None
            }
        })
        .collect();

    // The SDK's `Error` is not `std::error::Error`; render it to a string so it
    // flows through `anyhow` and our `AnswerChunk::Error`.
    let outcome = Client
        .builder()
        .name(client_name)
        // session/request_permission: the agent asks before it does something
        // that needs consent — crucially, **calling an MCP tool** (its Jira/
        // GitHub/perplexity connectors), which is exactly the meeting-oracle's
        // value. Auto-denying here blocks all tool use ("NO MCP").
        //
        // So we AUTO-ALLOW: pick the agent's least-privilege "allow" option
        // (prefer AllowOnce, then AllowAlways), falling back to the first offered
        // option, and only Cancelled when the agent offers nothing. This lets the
        // driven agent use its own connectors in its own environment.
        //
        // PHASE 2 (later): route write/exec-class permissions to Bluey's consent
        // UI; for the read-only meeting-oracle, allowing the agent to use its own
        // tools is the intended behavior.
        .on_receive_request(
            move |request: RequestPermissionRequest,
                  responder: Responder<RequestPermissionResponse>,
                  _cx| async move {
                use agent_client_protocol::schema::PermissionOptionKind;
                let pick = request
                    .options
                    .iter()
                    .find(|o| matches!(o.kind, PermissionOptionKind::AllowOnce))
                    .or_else(|| {
                        request
                            .options
                            .iter()
                            .find(|o| matches!(o.kind, PermissionOptionKind::AllowAlways))
                    })
                    .or_else(|| request.options.first());
                match pick {
                    Some(opt) => {
                        tracing::debug!(option = ?opt.kind, "ACP permission auto-allowed");
                        responder.respond(RequestPermissionResponse::new(
                            RequestPermissionOutcome::Selected(SelectedPermissionOutcome::new(
                                opt.option_id.clone(),
                            )),
                        ))
                    }
                    None => responder.respond(RequestPermissionResponse::new(
                        RequestPermissionOutcome::Cancelled,
                    )),
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        .connect_with(agent, {
            let tx = tx.clone();
            move |cx: agent_client_protocol::ConnectionTo<agent_client_protocol::Agent>| {
                let tx = tx.clone();
                async move {
                    // 1. initialize (protocol handshake).
                    cx.send_request(InitializeRequest::new(ProtocolVersion::V1))
                        .block_task()
                        .await?;

                    // 2. Open the session — new or resumed.
                    //
                    // For a fresh prompt we use the high-level session builder.
                    // For resume we send `session/load` directly (the builder
                    // only knows `session/new`) and re-attach a session handle
                    // to the loaded id.
                    let resumed = resume_id.is_some();
                    let mut session = match resume_id {
                        None => cx.build_session(&cwd).block_task().start_session().await?,
                        Some(id) => {
                            cx.send_request(LoadSessionRequest::new(id.clone(), cwd.clone()))
                                .block_task()
                                .await?;
                            // `attach_session` wants a NewSessionResponse; the
                            // only field it reads for routing is the session id,
                            // which is the id we just loaded.
                            cx.attach_session(NewSessionResponse::new(id), Default::default())?
                        }
                    };

                    // REPLAY SUPPRESSION: on `session/load` the agent replays
                    // the ENTIRE prior conversation as session/update
                    // notifications — observed live: the prior answer arrived
                    // as Deltas even AFTER the load response (the replay
                    // streams in as a burst, it is not pre-buffered). Anything
                    // arriving BEFORE `send_prompt` is by construction
                    // history, never the new turn — drain until the channel
                    // is quiescent (replay is a fast local burst; 400ms of
                    // silence marks its end), bounded at 5s so a hung agent
                    // can't stall the turn.
                    if resumed {
                        let drain_deadline =
                            tokio::time::Instant::now() + std::time::Duration::from_secs(5);
                        loop {
                            match tokio::time::timeout(
                                std::time::Duration::from_millis(400),
                                session.read_update(),
                            )
                            .await
                            {
                                Ok(Ok(_replayed)) => {
                                    if tokio::time::Instant::now() > drain_deadline {
                                        break;
                                    }
                                }
                                // Quiet for 400ms → replay is done.
                                Err(_) => break,
                                // Channel error → surface via the normal pump.
                                Ok(Err(_)) => break,
                            }
                        }
                    }

                    // Emit Started with the (native) session id so callers can
                    // persist it for a future resume.
                    let _ = tx.unbounded_send(AnswerChunk::Started {
                        session_id: Some(session.session_id().to_string()),
                    });

                    // 3. Send the prompt.
                    //
                    // TEXT-ONLY (the common path): `send_prompt` fires the request
                    // and arranges for the turn's StopReason to arrive on the
                    // session's update channel (`read_update`), which the pump
                    // below breaks on.
                    //
                    // WITH IMAGES (the "+"-menu multimodal path): the SDK's
                    // `send_prompt` hardcodes a single TEXT block, so we build the
                    // multi-block request ourselves — a text block plus one
                    // `ContentBlock::Image` per attached file (uri = the file
                    // path, NOT base64-in-text, which overflows the prompt) — and
                    // send it via the session's connection. The connection routes
                    // the agent's answer notifications to `read_update` exactly as
                    // before; only the terminal StopReason arrives on the awaited
                    // result instead, so we forward it to the pump via a oneshot
                    // the loop also selects on.
                    let mut img_done_rx: Option<futures::channel::oneshot::Receiver<StopReason>> =
                        None;
                    if image_blocks.is_empty() {
                        session.send_prompt(prompt)?;
                    } else {
                        use agent_client_protocol::schema::PromptRequest;
                        let mut blocks = vec![ContentBlock::from(prompt.clone())];
                        blocks.extend(image_blocks.clone());
                        let (done_tx, done_rx) = futures::channel::oneshot::channel::<StopReason>();
                        img_done_rx = Some(done_rx);
                        let done_tx = std::sync::Mutex::new(Some(done_tx));
                        session
                            .connection()
                            .send_request_to(
                                agent_client_protocol::Agent,
                                PromptRequest::new(session.session_id().clone(), blocks),
                            )
                            .on_receiving_result(move |result| {
                                let done_tx = done_tx.lock().ok().and_then(|mut g| g.take());
                                async move {
                                    let resp = result?;
                                    if let Some(tx) = done_tx {
                                        let _ = tx.send(resp.stop_reason);
                                    }
                                    Ok(())
                                }
                            })?;
                    }

                    // 4. Pump the session's update channel until the turn ends.
                    //    Each `SessionMessage` is either a `session/update`
                    //    notification (parsed into an `AnswerChunk` and pushed
                    //    to the consumer) or the terminal `StopReason`.
                    //
                    // TERMINAL-ECHO DEDUP: claude-code-acp streams the answer as
                    // incremental AgentMessageChunks and THEN re-emits the
                    // COMPLETE message as one final chunk — verified live
                    // (acp_delta_probe): for a 3-sentence answer the deltas were
                    // [s1][s2][s3] then a 4th chunk equal to s1+s2+s3, so the
                    // naive concat doubles the whole message. The terminal echo
                    // is ALWAYS the full text accumulated since the last message
                    // boundary, so a Delta whose (trimmed) text equals the
                    // (trimmed) accumulator is that echo — suppressed. A tool
                    // call ends the assistant message, so it resets the
                    // accumulator (a fresh message follows). Trimmed compare
                    // absorbs trailing-whitespace drift between the incremental
                    // tail and the echo.
                    let answer_acc = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
                    loop {
                        // For the image path the terminal StopReason arrives on the
                        // oneshot (the request result) rather than as a
                        // `read_update` message, so race the two: whichever fires
                        // first ends the turn. For the text path `img_done_rx` is
                        // None and this collapses to a plain `read_update`.
                        let msg = if let Some(rx) = img_done_rx.as_mut() {
                            use futures::future::{select, Either};
                            match select(std::pin::pin!(session.read_update()), rx).await {
                                Either::Left((update, _)) => update?,
                                Either::Right((stop, _)) => {
                                    let reason = stop.unwrap_or(StopReason::EndTurn);
                                    let _ = tx.unbounded_send(stop_reason_to_chunk(reason));
                                    break;
                                }
                            }
                        } else {
                            session.read_update().await?
                        };
                        match msg {
                            SessionMessage::SessionMessage(dispatch) => {
                                let tx = tx.clone();
                                let acc = std::sync::Arc::clone(&answer_acc);
                                // Parse the untyped dispatch as a
                                // `SessionNotification`; non-notification or
                                // unrecognized messages are ignored. This is the
                                // same pattern the SDK's own `read_to_string`
                                // uses internally.
                                MatchDispatch::new(dispatch)
                                    .if_notification(move |notif: SessionNotification| async move {
                                        if let Some(chunk) = session_update_to_chunk(notif.update) {
                                            let suppress = {
                                                let mut acc = acc.lock().expect("acc");
                                                match &chunk {
                                                    AnswerChunk::Delta(text) => {
                                                        // The echo is the full
                                                        // accumulated message. A
                                                        // single-chunk answer
                                                        // (acc empty until now)
                                                        // is NEVER an echo.
                                                        let echo = !acc.trim().is_empty()
                                                            && text.trim() == acc.trim();
                                                        if !echo {
                                                            acc.push_str(text);
                                                        }
                                                        echo
                                                    }
                                                    AnswerChunk::ToolCall { .. } => {
                                                        acc.clear();
                                                        false
                                                    }
                                                    _ => false,
                                                }
                                            };
                                            if !suppress {
                                                let _ = tx.unbounded_send(chunk);
                                            }
                                        }
                                        Ok(())
                                    })
                                    .await
                                    .otherwise_ignore()?;
                            }
                            SessionMessage::StopReason(reason) => {
                                let _ = tx.unbounded_send(stop_reason_to_chunk(reason));
                                break;
                            }
                            // `SessionMessage` is #[non_exhaustive]; ignore any
                            // future variant rather than failing the build.
                            _ => {}
                        }
                    }

                    Ok(())
                }
            }
        })
        .await;

    outcome.map_err(|e| anyhow::anyhow!("{e}"))
}

/// Map one ACP `session/update` payload to an [`AnswerChunk`], or `None` if the
/// update carries nothing Bluey's answer stream represents.
///
/// `SessionUpdate` is `#[non_exhaustive]`, so the wildcard arm is load-bearing:
/// new update kinds added upstream are silently ignored rather than breaking
/// the build.
fn session_update_to_chunk(update: SessionUpdate) -> Option<AnswerChunk> {
    match update {
        // The agent's visible answer text — the primary Delta stream.
        SessionUpdate::AgentMessageChunk(chunk) => text_of(&chunk.content).map(AnswerChunk::Delta),
        // The agent's reasoning/thinking — surfaced as a dedicated chunk so the
        // UI can show it in the live status area, NOT inline in the answer.
        SessionUpdate::AgentThoughtChunk(chunk) => {
            text_of(&chunk.content).map(AnswerChunk::Reasoning)
        }
        // A tool call STARTED — a real MCP/connector round-trip or built-in
        // (read/edit/search/…). Surfaced as a first-class ToolCall chunk (no
        // longer flattened into the answer body) so it appears in the live
        // status feed with its own running/done state.
        SessionUpdate::ToolCall(tool_call) => Some(AnswerChunk::ToolCall {
            id: tool_call.tool_call_id.0.to_string(),
            title: tool_call.title.clone(),
            status: tool_status_of(tool_call.status),
            // The KIND and the concrete TARGET — the minable signal the plain
            // title drops. Location wins over raw_input (a clean file path beats
            // a JSON arg blob); fall back to raw_input when no location is given.
            kind: tool_call_kind_str(&tool_call.kind),
            detail: tool_call_detail(&tool_call.locations, tool_call.raw_input.as_ref()),
        }),
        // A tool call PROGRESS update — same tool-call id, new status (and
        // sometimes a refined title). Collapses onto the existing status row.
        SessionUpdate::ToolCallUpdate(update) => Some(AnswerChunk::ToolCall {
            id: update.tool_call_id.0.to_string(),
            // Title may be absent on a progress update; empty string lets the
            // UI keep the title it already has for this id.
            title: update.fields.title.clone().unwrap_or_default(),
            status: update
                .fields
                .status
                .map(tool_status_of)
                .unwrap_or(ToolStatus::InProgress),
            // Updates carry refined fields; capture them when present. The
            // downstream capture only records the FIRST sighting of a tool id,
            // so a later update won't duplicate — but a start event that lacked
            // a location and an update that has one still improves the live row.
            kind: update.fields.kind.as_ref().and_then(tool_call_kind_str),
            detail: tool_call_detail(
                update.fields.locations.as_deref().unwrap_or(&[]),
                update.fields.raw_input.as_ref(),
            ),
        }),
        // Everything else (user echo, plans, mode/usage/info updates, and any
        // future variants) is not part of the answer or status, so it is
        // dropped. Cost is taken from the StopReason path instead.
        _ => None,
    }
}

/// Map ACP `ToolCallStatus` onto our transport-neutral [`ToolStatus`].
fn tool_status_of(status: agent_client_protocol::schema::ToolCallStatus) -> ToolStatus {
    use agent_client_protocol::schema::ToolCallStatus as S;
    match status {
        S::Pending => ToolStatus::Pending,
        S::InProgress => ToolStatus::InProgress,
        S::Completed => ToolStatus::Completed,
        S::Failed => ToolStatus::Failed,
        // `ToolCallStatus` is #[non_exhaustive]; treat unknowns as in-progress.
        _ => ToolStatus::InProgress,
    }
}

/// The ACP tool KIND as a lowercase string ("read"/"edit"/"search"/…), or
/// `None`. Serialized via serde (the schema is macro-generated, so we go through
/// the stable JSON form rather than matching Rust variant names) — the ACP wire
/// value is exactly the kebab/lower string we want to store.
fn tool_call_kind_str(kind: &agent_client_protocol::schema::ToolKind) -> Option<String> {
    match serde_json::to_value(kind) {
        Ok(serde_json::Value::String(s)) if !s.is_empty() => Some(s),
        _ => None,
    }
}

/// The concrete TARGET a tool acted on: the first `locations` file path if any,
/// else a compact rendering of `raw_input`. This is the "which file / which
/// query" the human title drops — the actual input to a who-touched-what graph.
/// Returns `None` when the agent surfaced neither (some agents send empty tool
/// events — see the ACP MCP-identity gap).
fn tool_call_detail(
    locations: &[agent_client_protocol::schema::ToolCallLocation],
    raw_input: Option<&serde_json::Value>,
) -> Option<String> {
    // Prefer a real file path from `locations`. Go through JSON so we don't
    // depend on the macro-generated field names; the ACP location object is
    // `{ "path": "...", "line": N? }`.
    if let Some(first) = locations.first() {
        if let Ok(serde_json::Value::Object(map)) = serde_json::to_value(first) {
            if let Some(serde_json::Value::String(path)) = map.get("path") {
                if !path.is_empty() {
                    return Some(path.clone());
                }
            }
        }
    }
    // Fall back to the raw tool input, compacted and length-bounded so a large
    // arg blob can't bloat the row. Metadata-grade, not full content.
    if let Some(v) = raw_input {
        let s = match v {
            serde_json::Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        let s = s.trim();
        if !s.is_empty() {
            let bounded: String = s.chars().take(200).collect();
            return Some(bounded);
        }
    }
    None
}

/// Extract plain text from a [`ContentBlock`], if it is textual.
///
/// `ContentBlock` is `#[non_exhaustive]`; non-text blocks (image, audio,
/// resource) have no place in the linear answer stream and yield `None`.
fn text_of(block: &ContentBlock) -> Option<String> {
    match block {
        ContentBlock::Text(text) => Some(text.text.clone()),
        _ => None,
    }
}

/// Map an ACP turn [`StopReason`] to the terminal [`AnswerChunk`].
///
/// A clean end-of-turn becomes [`AnswerChunk::Done`]; a refusal or limit becomes
/// a terminal [`AnswerChunk::Error`] so the caller does not mistake a truncated
/// or refused turn for a complete answer. ACP does not report per-turn cost on
/// the stop reason (cost arrives via the unstable `UsageUpdate`/token-usage
/// surface), so `cost_usd` is `None` here.
fn stop_reason_to_chunk(reason: StopReason) -> AnswerChunk {
    match reason {
        StopReason::EndTurn => AnswerChunk::Done { cost_usd: None },
        StopReason::MaxTokens => {
            AnswerChunk::Error("agent stopped: maximum tokens reached".to_string())
        }
        StopReason::MaxTurnRequests => {
            AnswerChunk::Error("agent stopped: maximum tool-call rounds reached".to_string())
        }
        StopReason::Refusal => AnswerChunk::Error("agent refused to continue".to_string()),
        StopReason::Cancelled => AnswerChunk::Error("turn cancelled".to_string()),
        // `StopReason` is #[non_exhaustive]: treat any future reason as a clean
        // stop rather than failing the build.
        _ => AnswerChunk::Done { cost_usd: None },
    }
}

/// Build the prompt's content blocks. ACP prompts are a vector of
/// [`ContentBlock`]s; Bluey sends a single text block. Exposed as a tiny helper
/// so the PHASE-2 multimodal path (images, embedded resources) has an obvious
/// seam.
#[allow(dead_code)]
fn text_prompt(prompt: impl Into<String>) -> Vec<ContentBlock> {
    vec![ContentBlock::from(prompt.into())]
}

/// The image MIME type for a file path by extension, or `None` if it is not a
/// supported image. Bounds attachments to real image types.
fn image_mime_for_path(path: &std::path::Path) -> Option<&'static str> {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .as_deref()
    {
        Some("png") => Some("image/png"),
        Some("jpg") | Some("jpeg") => Some("image/jpeg"),
        Some("gif") => Some("image/gif"),
        Some("webp") => Some("image/webp"),
        Some("heic") => Some("image/heic"),
        Some("bmp") => Some("image/bmp"),
        _ => None,
    }
}

/// Build an ACP `ContentBlock::Image` from a local image file: base64 the bytes
/// into `data` AND set `uri` to the file path (agents pick whichever they
/// support). `None` if the file can't be read or isn't a supported image type.
fn image_block_from_path(path: &std::path::Path) -> Option<ContentBlock> {
    use base64::Engine;
    let mime = image_mime_for_path(path)?;
    let bytes = std::fs::read(path).ok()?;
    let data = base64::engine::general_purpose::STANDARD.encode(&bytes);
    let mut img = agent_client_protocol::schema::ImageContent::new(data.clone(), mime);
    // The `uri` must be a UNIVERSALLY VALID image URL. A `file://` path is NOT:
    // agents that forward the uri to a cloud vision API (Codex → OpenAI) send it
    // as `image_url`, and OpenAI/Anthropic reject `file://` with
    // "Invalid image_url ... invalid format" (400). A `data:` URI embeds the
    // bytes and is accepted everywhere, so use that instead of the local path.
    img.uri = Some(format!("data:{mime};base64,{data}"));
    Some(ContentBlock::Image(img))
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_client_protocol::schema::{ContentChunk, TextContent};

    #[test]
    fn image_mime_maps_known_extensions_only() {
        assert_eq!(
            image_mime_for_path(std::path::Path::new("a.png")),
            Some("image/png")
        );
        assert_eq!(
            image_mime_for_path(std::path::Path::new("a.JPG")),
            Some("image/jpeg")
        );
        assert_eq!(
            image_mime_for_path(std::path::Path::new("a.jpeg")),
            Some("image/jpeg")
        );
        assert_eq!(
            image_mime_for_path(std::path::Path::new("a.webp")),
            Some("image/webp")
        );
        // Non-image / no extension → None (never attached as an image).
        assert_eq!(image_mime_for_path(std::path::Path::new("a.txt")), None);
        assert_eq!(image_mime_for_path(std::path::Path::new("noext")), None);
    }

    #[test]
    fn image_block_carries_base64_data_uri_and_mime() {
        // Write a tiny PNG-ish file and confirm the block base64s it, sets the
        // mime from the extension, and records a `data:` uri (NOT `file://`,
        // which a cloud vision agent rejects — the Codex `image_url` 400).
        let dir = std::env::temp_dir().join(format!("bluey-imgblk-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("shot.png");
        std::fs::write(&path, b"\x89PNG\r\n\x1a\nhello-bytes").unwrap();

        let block = image_block_from_path(&path).expect("image block");
        match block {
            ContentBlock::Image(img) => {
                assert_eq!(img.mime_type, "image/png");
                // data is base64 of the bytes (non-empty, decodes back).
                use base64::Engine;
                let decoded = base64::engine::general_purpose::STANDARD
                    .decode(img.data.as_bytes())
                    .expect("valid base64");
                assert_eq!(decoded, b"\x89PNG\r\n\x1a\nhello-bytes");
                // The uri is a data: URI that any cloud/local agent accepts —
                // never a file:// path (which OpenAI/Anthropic reject).
                let uri = img.uri.as_deref().expect("uri");
                assert!(uri.starts_with("data:image/png;base64,"), "uri: {uri}");
                assert!(!uri.starts_with("file://"));
                assert!(uri.ends_with(&img.data));
            }
            other => panic!("expected Image block, got {other:?}"),
        }
        // Unsupported type → None (skipped, never a bad block).
        let txt = dir.join("notes.txt");
        std::fs::write(&txt, b"hi").unwrap();
        assert!(image_block_from_path(&txt).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn spec_builds_argv_with_program_first() {
        let spec = AcpAgentSpec::new("gemini", vec!["--experimental-acp".to_string()]);
        // from_args should succeed and place the program before its args.
        let agent = spec.to_acp_agent().expect("valid spec");
        // The SDK exposes the parsed server config; assert the command + args.
        match agent.server() {
            agent_client_protocol::schema::McpServer::Stdio(stdio) => {
                assert_eq!(stdio.command, std::path::PathBuf::from("gemini"));
                assert_eq!(stdio.args, vec!["--experimental-acp".to_string()]);
            }
            _ => panic!("expected stdio transport"),
        }
    }

    #[test]
    fn agent_message_chunk_maps_to_delta() {
        let update = SessionUpdate::AgentMessageChunk(ContentChunk::new(ContentBlock::Text(
            TextContent::new("hello world"),
        )));
        assert_eq!(
            session_update_to_chunk(update),
            Some(AnswerChunk::Delta("hello world".to_string()))
        );
    }

    #[test]
    fn non_text_agent_chunk_yields_no_delta() {
        // An image content block carries no answer text.
        let img = agent_client_protocol::schema::ImageContent::new("base64", "image/png");
        let update = SessionUpdate::AgentMessageChunk(ContentChunk::new(ContentBlock::Image(img)));
        assert_eq!(session_update_to_chunk(update), None);
    }

    #[test]
    fn tool_call_maps_to_toolcall_chunk() {
        use agent_client_protocol::schema::ToolCall;
        let tc = ToolCall::new("tool-1", "Read file");
        let update = SessionUpdate::ToolCall(tc);
        // A tool call is now a first-class status chunk (no longer flattened
        // into the answer body as a `[tool: …]` Delta), carrying its id, title,
        // and run state for the live status feed.
        assert_eq!(
            session_update_to_chunk(update),
            Some(AnswerChunk::ToolCall {
                id: "tool-1".to_string(),
                title: "Read file".to_string(),
                status: ToolStatus::Pending,
                kind: Some("other".to_string()),
                detail: None,
            })
        );
    }

    #[test]
    fn agent_thought_maps_to_reasoning_chunk() {
        let update = SessionUpdate::AgentThoughtChunk(ContentChunk::new(ContentBlock::Text(
            TextContent::new("let me check the schema"),
        )));
        assert_eq!(
            session_update_to_chunk(update),
            Some(AnswerChunk::Reasoning(
                "let me check the schema".to_string()
            ))
        );
    }

    #[test]
    fn stop_reasons_map_to_terminal_chunks() {
        assert_eq!(
            stop_reason_to_chunk(StopReason::EndTurn),
            AnswerChunk::Done { cost_usd: None }
        );
        assert!(matches!(
            stop_reason_to_chunk(StopReason::Refusal),
            AnswerChunk::Error(_)
        ));
        assert!(matches!(
            stop_reason_to_chunk(StopReason::MaxTokens),
            AnswerChunk::Error(_)
        ));
        assert!(matches!(
            stop_reason_to_chunk(StopReason::Cancelled),
            AnswerChunk::Error(_)
        ));
    }

    #[test]
    fn text_prompt_wraps_a_single_text_block() {
        let blocks = text_prompt("do the thing");
        assert_eq!(blocks.len(), 1);
        assert!(matches!(&blocks[0], ContentBlock::Text(t) if t.text == "do the thing"));
    }
}
