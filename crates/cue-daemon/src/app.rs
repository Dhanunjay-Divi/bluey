use std::env;
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Context, Result};
use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use clap::Parser;
use cue_core::ai::{
    AnswerFinishReason, AnswerRequest, AnswerResponse, AnswerResponseMetadata, AnswerStreamEvent,
    CostEstimate, ProviderClientConfig, ProviderRequestPayload, RouteAttemptMetadata,
    SafetyOutcome, TokenUsage,
};
use cue_core::app_paths::AppPaths;
use cue_core::audio::AudioRuntimeMode;
use cue_core::ipc::{DaemonRequest, DaemonResponse, DEFAULT_DAEMON_ADDR};
use cue_core::overlay_ipc::ListeningState;
use cue_core::{
    analyze_segment, clock, generate_recap, load_account, local_answer, new_trace_id,
    sanitize_observability_id, trace_id_from_env, AiCapabilities, AiProviderId, AiProviderKind,
    AiRuntimeStatus, AnswerContext, AnswerContextKind, AudioBackend, AudioCaptureConfig,
    AudioCaptureStatus, AudioChunkMetadata, AudioDeviceDescriptor, AudioDeviceRole,
    AudioPipelineStatus, AudioSourceKind, CardArtifactType, CardKind, CloudEndpointConfig,
    CloudEnvironment, CloudSyncState, CloudSyncStatus, ContextArtifact, ContextKind,
    ContextProcessingStatus, ConversationTurn, CueCard, CueCardArtifact, DaemonState,
    MeetingRecord, MeetingState, MemoryHit, OverlayCommand, OverlayContextItem, OverlayEvent,
    OverlaySessionItem, PrivacyFlags, ProviderRoute, ProviderSelector, ProviderStatus, RouteBudget,
    Speaker, TranscriptSegment,
};
use cue_llm::{
    bluey_managed::{BlueyManagedProvider, ManagedLane},
    LlmProvider as _, LlmRequest,
};
use futures_util::{stream::FuturesUnordered, SinkExt, StreamExt};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::process::Command as TokioCommand;
use tokio::sync::{broadcast, mpsc, oneshot, watch, Mutex};
use tokio::time::{sleep, Duration};
use tokio_tungstenite::tungstenite::{
    client::IntoClientRequest, http::HeaderValue, Message as WebSocketMessage,
};
use tracing::{debug, error, info, trace, warn};

use crate::doc_conversion::{
    build_markdown_preview, classify_context_path, convert_context_file_to_markdown,
    is_supported_context_file, write_markdown_artifact,
};
use crate::overlay_state::{
    enter_overlay_ui_state, new_shared_overlay_ui_state, reset_overlay_ui_state_on_scope_exit,
    SharedOverlayUiState,
};
use crate::rag_indexer::RagIndexCoordinator;
use crate::storage::MeetingStore;

struct LiveProviderAnswer {
    provider: ProviderSelector,
    answer: String,
    token_usage: Option<TokenUsage>,
    latency_ms: u64,
}

struct ProviderPromptParts {
    system: String,
    user: String,
    image_data_urls: Vec<String>,
}

struct OverlayAnswerStream {
    daemon: Arc<Daemon>,
    card_id: uuid::Uuid,
    generation_id: u64,
    body: String,
}

impl OverlayAnswerStream {
    fn new(daemon: Arc<Daemon>, card_id: uuid::Uuid, generation_id: u64) -> Self {
        Self {
            daemon,
            card_id,
            generation_id,
            body: String::new(),
        }
    }

    fn has_text(&self) -> bool {
        !self.body.trim().is_empty()
    }

    async fn set_body(&mut self, body: impl Into<String>, done: bool) -> Result<()> {
        self.body = body.into();
        self.flush(done).await
    }

    async fn push_delta(&mut self, delta: &str) -> Result<()> {
        if delta.is_empty() {
            return Ok(());
        }
        self.body.push_str(delta);
        self.flush(false).await
    }

    async fn replay_text(&mut self, text: &str) -> Result<()> {
        self.body.clear();
        for chunk in streaming_word_chunks(text) {
            self.body.push_str(&chunk);
            self.flush(false).await?;
            sleep(Duration::from_millis(12)).await;
        }
        Ok(())
    }

    async fn finish(&mut self, final_body: &str) -> Result<()> {
        self.finish_with_cost_label(final_body, None).await
    }

    async fn finish_with_cost_label(
        &mut self,
        final_body: &str,
        cost_label: Option<String>,
    ) -> Result<()> {
        if self.body != final_body {
            self.body = final_body.to_string();
        }
        self.flush_with_cost_label(true, cost_label).await
    }

    async fn flush(&self, done: bool) -> Result<()> {
        self.flush_with_cost_label(done, None).await
    }

    async fn flush_with_cost_label(&self, done: bool, cost_label: Option<String>) -> Result<()> {
        if !is_answer_generation_current(&self.daemon, self.generation_id) {
            return Ok(());
        }
        let _ = send_overlay(
            &self.daemon,
            OverlayCommand::UpdateCard {
                id: self.card_id,
                body: self.body.clone(),
                done,
                cost_label,
                artifact: answer_overlay_artifact(&self.body).filter(|_| done),
            },
        )
        .await;
        Ok(())
    }
}

fn streaming_word_chunks(text: &str) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut current = String::new();
    for ch in text.chars() {
        current.push(ch);
        if ch.is_whitespace() {
            chunks.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        chunks.push(current);
    }
    chunks
}

fn next_answer_generation(daemon: &Arc<Daemon>) -> u64 {
    daemon
        .answer_generation
        .fetch_add(1, Ordering::SeqCst)
        .saturating_add(1)
}

fn is_answer_generation_current(daemon: &Arc<Daemon>, generation_id: u64) -> bool {
    daemon.answer_generation.load(Ordering::SeqCst) == generation_id
}

async fn register_active_answer_card(
    daemon: &Arc<Daemon>,
    generation_id: u64,
    card_id: uuid::Uuid,
) {
    let previous = {
        let mut active = daemon.active_answer_card.lock().await;
        active.replace((generation_id, card_id))
    };

    if let Some((previous_generation_id, previous_card_id)) = previous {
        if previous_generation_id != generation_id {
            let _ = send_overlay(
                daemon,
                OverlayCommand::UpdateCard {
                    id: previous_card_id,
                    body: "Superseded by a newer Bluey answer.".to_string(),
                    done: true,
                    cost_label: None,
                    artifact: None,
                },
            )
            .await;
        }
    }
}

async fn clear_active_answer_card(daemon: &Arc<Daemon>, generation_id: u64, card_id: uuid::Uuid) {
    let mut active = daemon.active_answer_card.lock().await;
    if *active == Some((generation_id, card_id)) {
        *active = None;
    }
}

fn is_near_duplicate_transcript(
    meeting: &MeetingRecord,
    speaker: Speaker,
    text: &str,
    is_final: bool,
) -> bool {
    if !is_final {
        return false;
    }

    let normalized = normalize_transcript_text(text);
    if normalized.len() < 4 {
        return false;
    }

    let now_ms = clock::now_epoch_ms_string().parse::<u64>().unwrap_or(0);
    meeting.transcript.iter().rev().take(8).any(|segment| {
        segment.is_final
            && segment.speaker == speaker
            && normalize_transcript_text(&segment.text) == normalized
            && transcript_age_ms(&segment.created_at, now_ms) <= 8_000
    })
}

pub fn normalize_transcript_text(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn transcript_age_ms(created_at: &str, now_ms: u64) -> u64 {
    created_at
        .parse::<u64>()
        .ok()
        .map(|created_ms| now_ms.saturating_sub(created_ms))
        .unwrap_or(u64::MAX)
}

/// Partial→Final dedup: when a final transcript arrives, remove the most recent
/// partial from the same speaker if the final text starts with (or equals) the
/// partial text (case-insensitive, whitespace-normalized).
/// Returns true if a partial was removed.
pub fn dedup_partial_on_final(
    meeting: &mut MeetingRecord,
    speaker: Speaker,
    final_text: &str,
) -> bool {
    let norm_final = normalize_transcript_text(final_text);
    // Search backwards for the most recent non-final segment from same speaker
    if let Some(idx) = meeting
        .transcript
        .iter()
        .rposition(|seg| !seg.is_final && seg.speaker == speaker)
    {
        let norm_partial = normalize_transcript_text(&meeting.transcript[idx].text);
        // Final supersedes partial if final starts with partial text
        if norm_final.starts_with(&norm_partial) || norm_partial.starts_with(&norm_final) {
            meeting.transcript.remove(idx);
            return true;
        }
    }
    false
}

#[derive(Debug, Clone, serde::Deserialize)]
struct ActivePageCapture {
    #[serde(default)]
    title: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    text: String,
}

#[derive(Debug, serde::Serialize)]
struct ChatCompletionRequest {
    model: String,
    messages: Vec<ChatMessage>,
    stream: bool,
    max_tokens: u32,
}

#[derive(Debug, serde::Serialize)]
struct ChatMessage {
    role: String,
    content: ChatMessageContent,
}

#[derive(Debug, serde::Serialize)]
#[serde(untagged)]
enum ChatMessageContent {
    Text(String),
    Parts(Vec<ChatMessagePart>),
}

#[derive(Debug, serde::Serialize)]
#[serde(tag = "type")]
enum ChatMessagePart {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image_url")]
    ImageUrl { image_url: ChatImageUrl },
}

#[derive(Debug, serde::Serialize)]
struct ChatImageUrl {
    url: String,
}

#[derive(Debug, serde::Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<ChatChoice>,
    usage: Option<ChatUsage>,
}

#[derive(Debug, serde::Deserialize)]
struct ChatCompletionStreamResponse {
    #[serde(default)]
    choices: Vec<ChatStreamChoice>,
    usage: Option<ChatUsage>,
}

#[derive(Debug, serde::Deserialize)]
struct ChatStreamChoice {
    delta: ChatStreamDelta,
}

#[derive(Debug, serde::Deserialize)]
struct ChatStreamDelta {
    content: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
struct ChatChoice {
    message: ChatChoiceMessage,
}

#[derive(Debug, serde::Deserialize)]
struct ChatChoiceMessage {
    content: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
struct ChatUsage {
    prompt_tokens: Option<u32>,
    completion_tokens: Option<u32>,
    total_tokens: Option<u32>,
}

#[derive(Debug, Clone)]
struct RealAudioRuntimeConfig {
    ffmpeg_path: Option<PathBuf>,
    stt_endpoint: String,
    stt_api_key: String,
    stt_model: String,
    stt_provider_label: String,
    stt_transport: RealSttTransport,
    chunk_duration_ms: u32,
    sources: Vec<RealAudioSource>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RealSttTransport {
    OpenAiMultipart,
    BlueyManagedRaw,
    BlueyManagedRelay,
}

#[derive(Debug, Clone)]
struct RealAudioSource {
    source: AudioSourceKind,
    device: AudioDeviceDescriptor,
    ffmpeg_input: FfmpegAudioInput,
    stream_id: String,
}

#[derive(Debug, Clone)]
enum FfmpegAudioInput {
    NativeHelper {
        helper_path: PathBuf,
        source_arg: String,
    },
    #[cfg(target_os = "macos")]
    MacAvFoundation { device_name: String },
    #[cfg(target_os = "windows")]
    WindowsDshow { device_name: String },
    #[cfg(target_os = "windows")]
    WindowsWasapiLoopback { device_name: String },
}

#[derive(Debug, serde::Deserialize)]
struct TranscriptionResponse {
    text: Option<String>,
    language: Option<String>,
}

#[derive(Debug, Parser)]
#[command(name = "bluey-daemon", version, about = "Bluey background daemon")]
struct Args {
    /// Listen address for local CLI IPC.
    #[arg(long, default_value = DEFAULT_DAEMON_ADDR)]
    addr: String,
    /// Do not spawn the native overlay sidecar.
    #[arg(long)]
    no_overlay: bool,
    /// Explicit path to native overlay sidecar.
    #[arg(long)]
    overlay_bin: Option<PathBuf>,
}

/// Payload emitted on the live transcript broadcast channel whenever a new
/// transcript segment is added to the active meeting.
#[derive(Debug, Clone, serde::Serialize)]
pub struct LiveTranscriptEvent {
    pub session_id: String,
    pub source: String,
    pub text: String,
    pub is_final: bool,
    pub speaker: Option<u8>,
    pub ts_ms: u64,
}

struct Daemon {
    paths: AppPaths,
    store: MeetingStore,
    state: Mutex<DaemonState>,
    meeting: Mutex<Option<MeetingRecord>>,
    overlay: Mutex<Option<OverlayProcess>>,
    overlay_enabled: bool,
    overlay_bin: Option<PathBuf>,
    overlay_events_tx: mpsc::UnboundedSender<OverlayEvent>,
    capture: Mutex<CaptureRuntime>,
    audio: Mutex<AudioPipelineStatus>,
    audio_runtime: Mutex<AudioRuntime>,
    cloud: Mutex<CloudSyncStatus>,
    balance_poll_shutdown: Mutex<Option<watch::Sender<bool>>>,
    balance_watch: crate::cloud::balance::BalanceWatch,
    answer_generation: AtomicU64,
    active_answer_card: Mutex<Option<(u64, uuid::Uuid)>>,
    system_audio: Mutex<Option<crate::audio::system_capture::SystemAudioCapture>>,
    live_transcript_tx: broadcast::Sender<LiveTranscriptEvent>,
    rag_indexer: RagIndexCoordinator,
    /// Per-session token issued at boot. Native overlay must echo this in
    /// every event; mismatched / missing token -> event dropped.
    overlay_session_token: String,
    /// State-machine of the overlay UI (Idle / AttachOpen / InstructionsOpen).
    ///
    /// SINGLE source of truth (R12.2): the Arc is cloned into the production
    /// overlay reader thread so the gate at validate_and_decode_overlay_line
    /// observes live transitions written by event handlers in this file.
    ///
    /// Event handlers transition as follows:
    ///   AttachRequested        -> AttachOpen
    ///   AttachFilesRequested   -> Idle    (panel closes after submit)
    ///   InstructionsRequested  -> InstructionsOpen
    ///   InstructionsUpdated    -> Idle    (form closes after save)
    ///
    /// The gate then rejects:
    ///   AttachFilesRequested when state is neither Idle nor AttachOpen
    ///   InstructionsUpdated  when state != InstructionsOpen
    /// while AttachRequested + InstructionsRequested are entry-point events
    /// allowed from any state.
    overlay_ui_state: SharedOverlayUiState,
}

struct OverlayProcess {
    child: Child,
    transport: OverlayTransport,
}

enum OverlayTransport {
    Stdio(ChildStdin),
    #[cfg(unix)]
    Socket(std::os::unix::net::UnixStream),
}

struct CaptureRuntime {
    stop: Option<oneshot::Sender<()>>,
    interval_secs: u64,
}

struct AudioRuntime {
    stop: Option<oneshot::Sender<()>>,
    session_id: Option<String>,
}

#[derive(Debug, Clone)]
enum AudioRuntimeConfigResolution {
    Real(RealAudioRuntimeConfig),
    Unavailable(String),
}

const DEFAULT_AUDIO_IDLE_STOP_SECS: u64 = 5 * 60;
const ANSWER_TRANSCRIPT_TURN_LIMIT: usize = 32;
const ANSWER_TRANSCRIPT_CHAR_BUDGET: usize = 8_000;

impl OverlayProcess {
    fn send(&mut self, command: &OverlayCommand) -> Result<()> {
        let line = serde_json::to_string(command)?;
        match &mut self.transport {
            OverlayTransport::Stdio(stdin) => {
                stdin.write_all(line.as_bytes())?;
                stdin.write_all(b"\n")?;
                stdin.flush()?;
            }
            #[cfg(unix)]
            OverlayTransport::Socket(stream) => {
                stream.write_all(line.as_bytes())?;
                stream.write_all(b"\n")?;
                stream.flush()?;
            }
        }
        Ok(())
    }
}

#[tokio::main]
pub async fn run() -> Result<()> {
    let _log_guard = cue_core::init_local_json_logging(
        "cue-daemon",
        "cue_daemon=info,cue_core=info,cue_cloud_client=info,cue_llm=info,cue_router=info",
    );

    let args = Args::parse();
    let paths = AppPaths::discover()?;
    paths.ensure()?;
    let store = MeetingStore::new(&paths)?;
    let active_meeting = store.load_active()?;
    let initial_state = state_from_active_meeting(active_meeting.as_ref());
    let cloud_status = cloud_status_from_env(&paths);
    let (overlay_events_tx, overlay_events_rx) = mpsc::unbounded_channel();
    let overlay_bin = args.overlay_bin.clone();
    let rag_indexer = RagIndexCoordinator::from_paths(&paths);
    let balance_watch = crate::cloud::balance::BalanceWatch::default();

    let daemon = Arc::new(Daemon {
        paths,
        store,
        state: Mutex::new(initial_state),
        meeting: Mutex::new(active_meeting),
        overlay: Mutex::new(None),
        overlay_enabled: !args.no_overlay,
        overlay_bin: overlay_bin.clone(),
        overlay_events_tx: overlay_events_tx.clone(),
        capture: Mutex::new(CaptureRuntime {
            stop: None,
            interval_secs: 12,
        }),
        audio: Mutex::new(AudioPipelineStatus::idle()),
        audio_runtime: Mutex::new(AudioRuntime {
            stop: None,
            session_id: None,
        }),
        cloud: Mutex::new(cloud_status),
        balance_poll_shutdown: Mutex::new(None),
        balance_watch,
        answer_generation: AtomicU64::new(0),
        active_answer_card: Mutex::new(None),
        system_audio: Mutex::new(None),
        live_transcript_tx: broadcast::channel(64).0,
        rag_indexer,
        overlay_session_token: crate::overlay::generate_session_token()
            .context("failed to generate overlay session token")?,
        overlay_ui_state: new_shared_overlay_ui_state(),
    });

    maybe_spawn_balance_polling(&daemon).await;

    if !args.no_overlay {
        match spawn_overlay(
            overlay_bin.as_deref(),
            overlay_events_tx.clone(),
            daemon.overlay_session_token.clone(),
            daemon.overlay_ui_state.clone(),
        ) {
            Ok(overlay) => {
                info!("native overlay started");
                *daemon.overlay.lock().await = Some(overlay);
                daemon.state.lock().await.overlay_capture_excluded = Some(true);
            }
            Err(error) => {
                warn!("native overlay not available yet: {error:#}");
            }
        }
    }
    spawn_overlay_event_handler(daemon.clone(), overlay_events_rx);
    spawn_overlay_balance_bridge(daemon.clone());

    // System audio continuous capture (opt-in via env var).
    if std::env::var("BLUEY_SYSTEM_AUDIO_CONTINUOUS")
        .map(|v| v == "1")
        .unwrap_or(false)
    {
        let stt_enabled = crate::audio::system_capture::is_system_audio_stt_enabled();
        let (sys_tx, mut sys_rx) = mpsc::unbounded_channel();
        match crate::audio::system_capture::SystemAudioCapture::start(sys_tx) {
            Ok(handle) => {
                info!(
                    system_stt = stt_enabled,
                    "system audio continuous capture started"
                );
                *daemon.system_audio.lock().await = Some(handle);
                let daemon_sys = daemon.clone();
                tokio::spawn(async move {
                    let mut stt: Option<Box<dyn cue_core::stt::SttProvider>> = if stt_enabled {
                        match build_system_audio_stt_provider().await {
                            Ok(provider) => Some(provider),
                            Err(e) => {
                                warn!("system audio STT provider failed to start: {e:#}");
                                None
                            }
                        }
                    } else {
                        None
                    };
                    let vad_config = crate::audio::vad::config_from_env();
                    let mut vad = if stt_enabled {
                        // WebRTC VAD's native handle is not Send, so the
                        // async system-audio task uses the Send-safe RMS gate
                        // and relies on provider endpointing for the second
                        // speech-boundary signal.
                        Some(crate::audio::vad::RmsGate::new(&vad_config))
                    } else {
                        None
                    };

                    // Single-task select! loop: send audio AND drain events
                    // from the SAME provider instance.
                    loop {
                        if let Some(ref mut provider) = stt {
                            tokio::select! {
                                chunk_opt = sys_rx.recv() => {
                                    match chunk_opt {
                                        Some(chunk) => {
                                            debug!("[system audio chunk: {}ms]", chunk.duration_ms());
                                            if let Some(vad) = vad.as_mut() {
                                                let action = vad.process(&chunk);
                                                if !action.should_forward() {
                                                    trace!(
                                                        vad_action = action.as_str(),
                                                        "system audio VAD dropped silence frame"
                                                    );
                                                    continue;
                                                }
                                                trace!(
                                                    vad_action = action.as_str(),
                                                    "system audio VAD forwarded frame"
                                                );
                                            }
                                            if let Err(e) = provider.send_audio(&chunk).await {
                                                warn!("system audio STT send failed: {e}");
                                            }
                                        }
                                        None => break,
                                    }
                                }
                                event_opt = provider.next_event() => {
                                    match event_opt {
                                        Some(Ok(event)) => {
                                            if let Some(segment) = transcript_event_to_stt_segment(&event) {
                                                if let Err(e) = add_audio_transcript_segment_allowing_session_start(&daemon_sys, &segment).await {
                                                    warn!("system audio STT drain: forward failed: {e:#}");
                                                }
                                            }
                                        }
                                        Some(Err(e)) => {
                                            warn!("system audio STT drain: provider error: {e}");
                                            if !e.is_retryable() {
                                                break;
                                            }
                                        }
                                        None => break,
                                    }
                                }
                            }
                        } else {
                            // No STT provider — just drain audio chunks.
                            match sys_rx.recv().await {
                                Some(chunk) => {
                                    debug!("[system audio chunk: {}ms]", chunk.duration_ms());
                                }
                                None => break,
                            }
                        }
                    }

                    if let Some(ref mut provider) = stt {
                        let _ = provider.close().await;
                    }
                });
            }
            Err(e) => {
                debug!("system audio continuous capture not available: {e}");
            }
        }
    }
    write_state(&daemon).await?;

    let listener = TcpListener::bind(&args.addr)
        .await
        .with_context(|| format!("failed to bind Bluey daemon IPC at {}", args.addr))?;
    info!("Bluey daemon listening on {}", args.addr);

    loop {
        let (stream, peer) = listener.accept().await?;
        debug!("accepted CLI connection from {peer}");
        let daemon = daemon.clone();
        tokio::spawn(async move {
            if let Err(error) = handle_client(daemon, stream).await {
                error!("client handler failed: {error:#}");
            }
        });
    }
}

async fn handle_client(daemon: Arc<Daemon>, stream: TcpStream) -> Result<()> {
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let mut line = String::new();
    let read = reader.read_line(&mut line).await?;
    if read == 0 {
        return Ok(());
    }

    let request: DaemonRequest = serde_json::from_str(line.trim_end())?;
    let shutdown = request.is_shutdown();
    let response = handle_request(&daemon, request).await;
    let line = serde_json::to_string(&response)?;
    writer.write_all(line.as_bytes()).await?;
    writer.write_all(b"\n").await?;
    writer.flush().await?;

    if shutdown {
        shutdown_daemon(&daemon).await;
        std::process::exit(0);
    }

    Ok(())
}

async fn handle_request(daemon: &Arc<Daemon>, request: DaemonRequest) -> DaemonResponse {
    let (request, trace_id) = request.into_trace_parts();
    let trace_id = trace_id
        .or_else(trace_id_from_env)
        .unwrap_or_else(new_trace_id);
    debug!(trace_id = %trace_id, "daemon ipc request received");
    match handle_request_inner(daemon, request, &trace_id).await {
        Ok(response) => response,
        Err(error) => DaemonResponse::Error {
            message: format!("{error:#}"),
        },
    }
}

async fn handle_request_inner(
    daemon: &Arc<Daemon>,
    request: DaemonRequest,
    trace_id: &str,
) -> Result<DaemonResponse> {
    match request {
        DaemonRequest::WithTrace { .. } => unreachable!("trace envelope should be stripped"),
        DaemonRequest::Ping => Ok(DaemonResponse::Pong),
        DaemonRequest::Status => Ok(DaemonResponse::Status {
            state: daemon.state.lock().await.clone(),
        }),
        DaemonRequest::Shutdown => Ok(DaemonResponse::Ok),
        DaemonRequest::OverlayShow => {
            send_overlay(daemon, OverlayCommand::Show).await?;
            daemon.state.lock().await.overlay_visible = true;
            write_state(daemon).await?;
            Ok(DaemonResponse::Ok)
        }
        DaemonRequest::OverlayHide => {
            send_overlay(daemon, OverlayCommand::Hide).await?;
            daemon.state.lock().await.overlay_visible = false;
            write_state(daemon).await?;
            Ok(DaemonResponse::Ok)
        }
        DaemonRequest::OverlayToggle => {
            let visible = {
                let mut state = daemon.state.lock().await;
                state.overlay_visible = !state.overlay_visible;
                state.overlay_visible
            };
            send_overlay(
                daemon,
                if visible {
                    OverlayCommand::Show
                } else {
                    OverlayCommand::Hide
                },
            )
            .await?;
            write_state(daemon).await?;
            Ok(DaemonResponse::Ok)
        }
        DaemonRequest::OverlayClear => {
            send_overlay(daemon, OverlayCommand::Clear).await?;
            Ok(DaemonResponse::Ok)
        }
        DaemonRequest::OverlayBoot { title, lines } => {
            send_overlay(daemon, OverlayCommand::Boot { title, lines }).await?;
            daemon.state.lock().await.overlay_visible = true;
            write_state(daemon).await?;
            Ok(DaemonResponse::Ok)
        }
        DaemonRequest::OverlaySetOpacity { opacity } => {
            let opacity = opacity.clamp(0.05, 1.0);
            send_overlay(daemon, OverlayCommand::SetOpacity { opacity }).await?;
            daemon.state.lock().await.overlay_opacity = opacity;
            write_state(daemon).await?;
            Ok(DaemonResponse::Ok)
        }
        DaemonRequest::OverlaySetPosition { position } => {
            send_overlay(daemon, OverlayCommand::SetPosition { position }).await?;
            daemon.state.lock().await.overlay_position = position;
            write_state(daemon).await?;
            Ok(DaemonResponse::Ok)
        }
        DaemonRequest::PushCard { card } => {
            send_overlay(daemon, OverlayCommand::PushCard { card }).await?;
            Ok(DaemonResponse::Ok)
        }
        DaemonRequest::MeetingStart { title } => {
            let meeting = {
                let mut meeting_guard = daemon.meeting.lock().await;
                if meeting_guard.is_some() {
                    return Ok(DaemonResponse::Text {
                        text: "A meeting is already active.".to_string(),
                    });
                }

                let meeting = MeetingRecord::new(title);
                daemon.store.save_active(&meeting)?;
                *meeting_guard = Some(meeting.clone());
                meeting
            };

            update_state_from_meeting(daemon, Some(&meeting)).await?;
            let card = CueCard::new(
                CardKind::System,
                "Meeting started",
                format!("Bluey is listening: {}", meeting.title),
            )
            .with_source("bluey daemon");
            let _ = send_overlay(daemon, OverlayCommand::PushCard { card }).await;
            write_state(daemon).await?;
            Ok(DaemonResponse::Text {
                text: "Meeting started.".to_string(),
            })
        }
        DaemonRequest::MeetingEnd => {
            let mut meeting = {
                let mut meeting_guard = daemon.meeting.lock().await;
                let Some(meeting) = meeting_guard.take() else {
                    return Ok(DaemonResponse::Text {
                        text: "No meeting is active.".to_string(),
                    });
                };
                meeting
            };

            meeting.ended_at = Some(clock::now_epoch_ms_string());
            let recap = generate_recap(&meeting);
            meeting.summary = Some(recap.summary.clone());
            let path = daemon.store.archive(&meeting)?;
            update_state_from_meeting(daemon, None).await?;
            let card = CueCard::new(
                CardKind::System,
                "Meeting ended",
                format!(
                    "{} segment(s), {} action item(s), {} decision(s).",
                    recap.transcript_segments,
                    recap.action_items.len(),
                    recap.decisions.len()
                ),
            )
            .with_source(path.display().to_string());
            let _ = send_overlay(daemon, OverlayCommand::PushCard { card }).await;
            write_state(daemon).await?;
            // R10: Auto-recap via LLM (best-effort, fire-and-forget).
            spawn_auto_recap(daemon, &meeting);
            Ok(DaemonResponse::Recap { recap })
        }
        DaemonRequest::TranscriptAdd {
            speaker,
            text,
            is_final,
        } => {
            let Some((meeting_snapshot, cards, indexed_segment)) = ({
                let mut meeting_guard = daemon.meeting.lock().await;
                if meeting_guard.is_none() {
                    *meeting_guard = Some(MeetingRecord::new(Some("Ad hoc meeting".to_string())));
                }

                let meeting = meeting_guard.as_mut().expect("meeting exists");
                if is_near_duplicate_transcript(meeting, speaker, &text, is_final) {
                    None
                } else {
                    let segment = TranscriptSegment::new(speaker, text, is_final);
                    meeting.transcript.push(segment.clone());

                    let analysis = analyze_segment(&segment, meeting);
                    meeting.action_items.extend(analysis.action_items);
                    meeting.decisions.extend(analysis.decisions);
                    daemon.store.save_active(meeting)?;
                    let indexed_segment = segment
                        .is_final
                        .then(|| (meeting.id.to_string(), segment.text.clone()));
                    Some((meeting.clone(), analysis.cards, indexed_segment))
                }
            }) else {
                return Ok(DaemonResponse::Text {
                    text: format!("Skipped duplicate transcript segment from {speaker}."),
                });
            };

            if let Some((session_id, text)) = indexed_segment {
                index_transcript_for_rag(daemon, session_id, text);
            }

            update_state_from_meeting(daemon, Some(&meeting_snapshot)).await?;
            let has_cards = !cards.is_empty();
            for card in cards {
                let _ = send_overlay(daemon, OverlayCommand::PushCard { card }).await;
            }
            if has_cards {
                write_state(daemon).await?;
            }

            Ok(DaemonResponse::Text {
                text: format!("Added transcript segment from {speaker}."),
            })
        }
        DaemonRequest::Ask { question } => {
            let response = answer_question(daemon, question, "manual ask").await?;
            Ok(DaemonResponse::Text {
                text: response.answer,
            })
        }
        DaemonRequest::Answer { request } => {
            let (response, events) =
                answer_with_provider_runtime(daemon, request, "answer ipc").await?;
            Ok(DaemonResponse::Answer { response, events })
        }
        DaemonRequest::ContextAdd { path, title, note } => {
            let artifact = build_context_artifact(&daemon.paths, path, title, note)?;
            let meeting_snapshot = attach_context_artifacts(daemon, vec![artifact.clone()]).await?;

            update_state_from_meeting(daemon, Some(&meeting_snapshot)).await?;
            let card = CueCard::new(
                CardKind::Context,
                "Context attached",
                format!(
                    "{} ({}:{}){}{}",
                    artifact.title,
                    artifact.kind,
                    artifact.processing_status,
                    artifact
                        .note
                        .as_ref()
                        .filter(|note| !note.trim().is_empty())
                        .map(|note| format!("\n{note}"))
                        .unwrap_or_default(),
                    artifact
                        .processing_error
                        .as_ref()
                        .filter(|error| !error.trim().is_empty())
                        .map(|error| format!("\n{error}"))
                        .unwrap_or_default()
                ),
            )
            .with_source(artifact.path.clone());
            let _ = send_overlay(daemon, OverlayCommand::PushCard { card }).await;
            write_state(daemon).await?;
            Ok(DaemonResponse::ContextItems {
                items: vec![artifact],
            })
        }
        DaemonRequest::ContextList => {
            let meeting = if let Some(active) = daemon.meeting.lock().await.as_ref() {
                Some(active.clone())
            } else {
                daemon.store.last_meeting()?
            };
            Ok(DaemonResponse::ContextItems {
                items: meeting.map(|m| m.context).unwrap_or_default(),
            })
        }
        DaemonRequest::ActivePageCapture => {
            let artifact = capture_active_page_context(daemon, "CLI").await?;
            Ok(DaemonResponse::ContextItems {
                items: vec![artifact],
            })
        }
        DaemonRequest::ScreenCaptureStart { interval_secs } => {
            let interval_secs = interval_secs.unwrap_or(12).clamp(3, 300);
            start_screen_capture(daemon, interval_secs, "CLI").await?;
            Ok(DaemonResponse::Text {
                text: format!("Screen context capture started every {interval_secs}s."),
            })
        }
        DaemonRequest::ScreenCaptureStop => {
            stop_screen_capture(daemon, "CLI").await?;
            Ok(DaemonResponse::Text {
                text: "Screen context capture stopped.".to_string(),
            })
        }
        DaemonRequest::InstructionsSet { text } => {
            let meeting_snapshot = set_answer_instructions(daemon, Some(text)).await?;
            update_state_from_meeting(daemon, Some(&meeting_snapshot)).await?;
            push_system_card(
                daemon,
                CardKind::System,
                "Answer style saved",
                meeting_snapshot
                    .answer_instructions
                    .clone()
                    .unwrap_or_else(|| "No instructions set.".to_string()),
            )
            .await;
            Ok(DaemonResponse::Text {
                text: "Answer instructions saved.".to_string(),
            })
        }
        DaemonRequest::InstructionsGet => {
            let meeting = if let Some(active) = daemon.meeting.lock().await.as_ref() {
                Some(active.clone())
            } else {
                daemon.store.last_meeting()?
            };
            Ok(DaemonResponse::Text {
                text: meeting
                    .and_then(|meeting| meeting.answer_instructions)
                    .unwrap_or_else(|| "No answer instructions set.".to_string()),
            })
        }
        DaemonRequest::InstructionsClear => {
            let meeting_snapshot = set_answer_instructions(daemon, None).await?;
            update_state_from_meeting(daemon, Some(&meeting_snapshot)).await?;
            push_system_card(
                daemon,
                CardKind::System,
                "Answer style cleared",
                "Bluey will use the default answer style.",
            )
            .await;
            Ok(DaemonResponse::Text {
                text: "Answer instructions cleared.".to_string(),
            })
        }
        DaemonRequest::MemorySearch { query, limit } => {
            let query = query.trim().to_string();
            if query.is_empty() {
                return Ok(DaemonResponse::MemoryHits { hits: Vec::new() });
            }
            let meetings = daemon.store.all_meetings()?;
            Ok(DaemonResponse::MemoryHits {
                hits: search_memory(&meetings, &query, limit.clamp(1, 20)),
            })
        }
        DaemonRequest::AudioStatus => Ok(DaemonResponse::AudioStatus {
            status: daemon.audio.lock().await.clone(),
        }),
        DaemonRequest::AudioStart {
            enable_system,
            enable_microphone,
            mic_device_id,
        } => {
            if !enable_system && !enable_microphone {
                return Ok(DaemonResponse::Text {
                    text: "Choose at least one audio source.".to_string(),
                });
            }

            let mut config =
                AudioCaptureConfig::from_enabled_sources(enable_system, enable_microphone);
            if let Some(device_id) = mic_device_id {
                config.microphone.device_id = Some(device_id);
            }
            set_overlay_listening_state(daemon, ListeningState::Connecting).await;
            let status = match start_audio_capture(daemon, config).await {
                Ok(status) => status,
                Err(error) => {
                    set_overlay_listening_state(daemon, ListeningState::Failed).await;
                    return Err(error);
                }
            };
            set_overlay_listening_state(daemon, ListeningState::Listening).await;
            Ok(DaemonResponse::AudioStatus { status })
        }
        DaemonRequest::AudioStop => {
            let status = stop_audio_capture(daemon).await;
            set_overlay_listening_state(daemon, ListeningState::Paused).await;
            let _ = refresh_overlay_balance(daemon, Some(trace_id)).await;
            Ok(DaemonResponse::AudioStatus { status })
        }
        DaemonRequest::AiStatus => Ok(DaemonResponse::AiStatus {
            status: ai_status_from_env(),
        }),
        DaemonRequest::CloudStatus => {
            let status = cloud_status_from_env(&daemon.paths);
            *daemon.cloud.lock().await = status.clone();
            if status.sync_state == CloudSyncState::Disabled {
                stop_balance_polling(daemon).await;
            } else {
                maybe_spawn_balance_polling(daemon).await;
                daemon.rag_indexer.refresh_from_paths(&daemon.paths);
            }
            let _ = refresh_overlay_balance(daemon, Some(trace_id)).await;
            Ok(DaemonResponse::CloudStatus { status })
        }
        DaemonRequest::CloudLogout => {
            stop_balance_polling(daemon).await;
            let status = cloud_status_from_env(&daemon.paths);
            *daemon.cloud.lock().await = status.clone();
            let _ = send_overlay(
                daemon,
                OverlayCommand::SetBalance {
                    label: "--".to_string(),
                },
            )
            .await;
            Ok(DaemonResponse::CloudStatus { status })
        }
        DaemonRequest::CloudSyncNow => {
            let mut status = cloud_status_from_env(&daemon.paths);
            if status.sync_state == CloudSyncState::Disabled {
                *daemon.cloud.lock().await = status.clone();
                return Ok(DaemonResponse::CloudStatus { status });
            }
            status.mark_syncing();
            *daemon.cloud.lock().await = status.clone();

            let status = match build_cloud_client(&daemon.paths, Some(trace_id)) {
                Ok(client) => {
                    match crate::cloud::sync::sync_local_meetings(
                        &daemon.store,
                        &daemon.paths.data_dir,
                        &client,
                    )
                    .await
                    {
                        Ok(summary) => {
                            let mut synced = cloud_status_from_env(&daemon.paths);
                            synced.mark_synced();
                            if summary.total_records() == 0 {
                                synced.last_error = Some(
                                    "No local sessions were available to sync yet.".to_string(),
                                );
                            } else {
                                info!(
                                    batches = summary.batches,
                                    records = summary.total_records(),
                                    "cloud sync complete"
                                );
                            }
                            synced
                        }
                        Err(error) => {
                            let mut failed = cloud_status_from_env(&daemon.paths);
                            failed.mark_failed(format!("{error:#}"));
                            failed
                        }
                    }
                }
                Err(error) => {
                    let mut failed = cloud_status_from_env(&daemon.paths);
                    failed.mark_failed(format!("{error:#}"));
                    failed
                }
            };
            *daemon.cloud.lock().await = status.clone();
            Ok(DaemonResponse::CloudStatus { status })
        }
        DaemonRequest::Recap => {
            let meeting = if let Some(active) = daemon.meeting.lock().await.as_ref() {
                Some(active.clone())
            } else {
                daemon.store.last_meeting()?
            };
            let Some(meeting) = meeting else {
                return Ok(DaemonResponse::Text {
                    text: "No meeting has been captured yet.".to_string(),
                });
            };
            Ok(DaemonResponse::Recap {
                recap: generate_recap(&meeting),
            })
        }
        DaemonRequest::ActionItems => {
            let meeting = if let Some(active) = daemon.meeting.lock().await.as_ref() {
                Some(active.clone())
            } else {
                daemon.store.last_meeting()?
            };
            Ok(DaemonResponse::ActionItems {
                items: meeting.map(|m| m.action_items).unwrap_or_default(),
            })
        }
    }
}

async fn send_overlay(daemon: &Arc<Daemon>, command: OverlayCommand) -> Result<()> {
    let mut overlay_guard = daemon.overlay.lock().await;
    ensure_overlay_ready(daemon, &mut overlay_guard).await?;

    if let Some(overlay) = overlay_guard.as_mut() {
        if let Err(first_error) = overlay.send(&command) {
            warn!("overlay command failed; restarting overlay: {first_error:#}");
            dispose_overlay_process(overlay_guard.take());
            ensure_overlay_ready(daemon, &mut overlay_guard)
                .await
                .with_context(|| {
                    format!("overlay pipe failed ({first_error:#}) and restart failed")
                })?;
            let Some(overlay) = overlay_guard.as_mut() else {
                return Err(anyhow!("overlay process is not running after restart"));
            };
            overlay.send(&command).with_context(|| {
                format!("overlay pipe failed ({first_error:#}) and retry failed")
            })?;
        }
        return Ok(());
    }

    return Err(anyhow!("overlay process is not running"));
}

async fn maybe_spawn_balance_polling(daemon: &Arc<Daemon>) {
    let mut shutdown_guard = daemon.balance_poll_shutdown.lock().await;
    if shutdown_guard.is_some() {
        return;
    }

    let Ok(client) = build_cloud_client(&daemon.paths, None) else {
        debug!("balance polling skipped; account store unavailable");
        return;
    };
    if client.current_tokens().is_none() {
        debug!("balance polling skipped; no Bluey account token");
        return;
    }

    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    crate::cloud::balance::spawn_loop_with_shutdown(
        client,
        daemon.balance_watch.clone(),
        Some(shutdown_rx),
    );
    *shutdown_guard = Some(shutdown_tx);
}

async fn stop_balance_polling(daemon: &Arc<Daemon>) {
    if let Some(shutdown_tx) = daemon.balance_poll_shutdown.lock().await.take() {
        let _ = shutdown_tx.send(true);
    }
}

fn spawn_overlay_balance_bridge(daemon: Arc<Daemon>) {
    let mut rx = daemon.balance_watch.subscribe();
    tokio::spawn(async move {
        let initial = rx.borrow().clone();
        if let Some(snapshot) = initial {
            push_overlay_balance_snapshot(&daemon, snapshot).await;
        }

        while rx.changed().await.is_ok() {
            let next = rx.borrow().clone();
            if let Some(snapshot) = next {
                push_overlay_balance_snapshot(&daemon, snapshot).await;
            }
        }
    });
}

async fn push_overlay_balance_snapshot(
    daemon: &Arc<Daemon>,
    snapshot: crate::cloud::balance::BalanceSnapshot,
) {
    let mut label = format_balance_cents(snapshot.balance_cents);
    if snapshot.low_balance_warning {
        label.push_str(" low");
    }
    let _ = send_overlay(daemon, OverlayCommand::SetBalance { label }).await;
}

async fn set_overlay_listening_state(daemon: &Arc<Daemon>, state: ListeningState) {
    let _ = send_overlay(daemon, OverlayCommand::ListeningStateChanged { state }).await;
}

async fn ensure_overlay_ready(
    daemon: &Arc<Daemon>,
    overlay: &mut Option<OverlayProcess>,
) -> Result<()> {
    if !daemon.overlay_enabled {
        return Err(anyhow!("overlay process is disabled for this daemon"));
    }

    if let Some(process) = overlay.as_mut() {
        if let Some(status) = process.child.try_wait()? {
            warn!("overlay process exited before command: {status}");
            *overlay = None;
        }
    }

    if overlay.is_none() {
        let process = spawn_overlay(
            daemon.overlay_bin.as_deref(),
            daemon.overlay_events_tx.clone(),
            daemon.overlay_session_token.clone(),
            daemon.overlay_ui_state.clone(),
        )
        .context("failed to start native overlay")?;
        *overlay = Some(process);
        let mut state = daemon.state.lock().await;
        state.overlay_capture_excluded = Some(true);
    }

    Ok(())
}

fn dispose_overlay_process(process: Option<OverlayProcess>) {
    if let Some(mut process) = process {
        let _ = process.child.kill();
        let _ = process.child.wait();
    }
}

fn spawn_overlay_event_handler(
    daemon: Arc<Daemon>,
    mut events: mpsc::UnboundedReceiver<OverlayEvent>,
) {
    tokio::spawn(async move {
        while let Some(event) = events.recv().await {
            if let Err(error) = handle_overlay_event(&daemon, event).await {
                warn!("failed to handle overlay event: {error:#}");
            }
        }
    });
}

async fn handle_overlay_event(daemon: &Arc<Daemon>, event: OverlayEvent) -> Result<()> {
    match event {
        OverlayEvent::Ready {
            capture_excluded, ..
        } => {
            daemon.state.lock().await.overlay_capture_excluded = Some(capture_excluded);
            write_state(daemon).await?;
            if let Some(meeting) = daemon.meeting.lock().await.clone() {
                refresh_overlay_context_items(daemon, &meeting).await;
            }
            refresh_overlay_sessions(daemon).await;
            let daemon_balance = Arc::clone(daemon);
            tokio::spawn(async move {
                let _ = refresh_overlay_balance(&daemon_balance, None).await;
            });
        }
        OverlayEvent::Shown => {
            daemon.state.lock().await.overlay_visible = true;
            write_state(daemon).await?;
        }
        OverlayEvent::Hidden => {
            daemon.state.lock().await.overlay_visible = false;
            write_state(daemon).await?;
        }
        OverlayEvent::OpacityUpdated { opacity } => {
            let opacity = opacity.clamp(0.05, 1.0);
            daemon.state.lock().await.overlay_opacity = opacity;
            write_state(daemon).await?;
        }
        OverlayEvent::AskRequested {
            question,
            provider,
            model,
            mode,
        } => {
            let request = answer_request_from_overlay(&question, provider, model, mode);
            let _ = answer_with_provider_runtime(daemon, request, "overlay ask").await?;
        }
        OverlayEvent::AttachRequested => {
            // Drive Idle -> AttachOpen while the daemon-owned picker is open.
            // The guard resets on success, cancel, or error so stale submit
            // events cannot pass through after the picker closes.
            let _ui_state = enter_overlay_ui_state(
                &daemon.overlay_ui_state,
                cue_core::overlay_ipc::OverlayUiState::AttachOpen,
            );
            handle_attach_requested(daemon).await?;
        }
        OverlayEvent::AttachFilesRequested { paths } => {
            let _ui_state = reset_overlay_ui_state_on_scope_exit(&daemon.overlay_ui_state);
            handle_attach_paths(daemon, paths.into_iter().map(PathBuf::from).collect()).await?;
        }
        OverlayEvent::RemoveContextRequested { id } => {
            handle_remove_context_requested(daemon, id).await?;
        }
        OverlayEvent::InstructionsRequested => {
            // Drive Idle -> InstructionsOpen while the daemon-owned prompt is
            // open. The guard resets on save, cancel, or error.
            let _ui_state = enter_overlay_ui_state(
                &daemon.overlay_ui_state,
                cue_core::overlay_ipc::OverlayUiState::InstructionsOpen,
            );
            handle_instructions_requested(daemon).await?;
        }
        OverlayEvent::InstructionsUpdated { text } => {
            let _ui_state = reset_overlay_ui_state_on_scope_exit(&daemon.overlay_ui_state);
            let instructions = if text.trim().is_empty() {
                None
            } else {
                Some(text.trim().to_string())
            };
            let meeting_snapshot = set_answer_instructions(daemon, instructions).await?;
            update_state_from_meeting(daemon, Some(&meeting_snapshot)).await?;
            push_system_card(
                daemon,
                CardKind::System,
                "Answer style saved",
                meeting_snapshot
                    .answer_instructions
                    .clone()
                    .unwrap_or_else(|| "Bluey will use the default answer style.".to_string()),
            )
            .await;
        }
        OverlayEvent::SessionOpenRequested { id } => {
            open_meeting_session(daemon, id).await?;
        }
        OverlayEvent::SessionRenameRequested { id, title } => {
            rename_meeting_session(daemon, id, &title).await?;
        }
        OverlayEvent::SessionDeleteRequested { id } => {
            delete_meeting_session(daemon, id).await?;
        }
        OverlayEvent::SessionContinueRequested => {
            continue_session(daemon, "overlay session").await?;
        }
        OverlayEvent::SessionNewRequested => {
            start_new_session(daemon, "overlay session").await?;
        }
        OverlayEvent::ActivePageCaptureRequested => {
            if let Err(error) = capture_active_page_context(daemon, "overlay page").await {
                push_system_card(
                    daemon,
                    CardKind::Warning,
                    "Page context failed",
                    format!("{error:#}"),
                )
                .await;
            }
        }
        OverlayEvent::AnalyzeScreenRequested { question } => {
            if let Err(error) = analyze_active_page_context(daemon, question.as_deref()).await {
                push_system_card(
                    daemon,
                    CardKind::Warning,
                    "Analyse failed",
                    format!("{error:#}"),
                )
                .await;
            }
        }
        OverlayEvent::RecapRequested => {
            push_recap_card(daemon).await?;
        }
        OverlayEvent::ContextListRequested => {
            push_context_list_card(daemon).await?;
        }
        OverlayEvent::CaptureStartRequested => {
            if let Err(error) = start_screen_capture(daemon, 12, "overlay eye").await {
                push_system_card(
                    daemon,
                    CardKind::Warning,
                    "Screen context failed",
                    format!("{error:#}"),
                )
                .await;
            }
        }
        OverlayEvent::CaptureStopRequested => {
            stop_screen_capture(daemon, "overlay eye").await?;
        }
        OverlayEvent::RecordingStartRequested => {
            set_overlay_listening_state(daemon, ListeningState::Connecting).await;
            match start_audio_capture(daemon, AudioCaptureConfig::dual_default()).await {
                Ok(status) => {
                    set_overlay_listening_state(daemon, ListeningState::Listening).await;
                    let balance = refresh_overlay_balance(daemon, None).await;
                    let balance_line = balance
                        .map(|label| format!("\nBalance: {label}."))
                        .unwrap_or_default();
                    push_system_card(
                        daemon,
                        CardKind::System,
                        "Recording on",
                        format!(
                            "{} Auto-stop after {} with no transcript.{}",
                            recording_sources_label(&status),
                            format_duration(audio_idle_stop_timeout()),
                            balance_line
                        ),
                    )
                    .await;
                }
                Err(error) => {
                    set_overlay_listening_state(daemon, ListeningState::Failed).await;
                    push_system_card(
                        daemon,
                        CardKind::Warning,
                        "Audio setup needed",
                        format!("{error:#}"),
                    )
                    .await;
                }
            }
        }
        OverlayEvent::RecordingStopRequested => {
            let status = stop_audio_capture(daemon).await;
            set_overlay_listening_state(daemon, ListeningState::Paused).await;
            let balance = refresh_overlay_balance(daemon, None).await;
            let balance_line = balance
                .map(|label| format!("\nFinal balance: {label}."))
                .unwrap_or_default();
            push_system_card(
                daemon,
                CardKind::System,
                "Recording off",
                format!(
                    "Audio runtime stopped: {:?}.{}",
                    status.capture.state, balance_line
                ),
            )
            .await;
        }
        OverlayEvent::CloseRequested => {
            shutdown_daemon(daemon).await;
            std::process::exit(0);
        }
        OverlayEvent::Exited => {
            let _ = stop_screen_capture(daemon, "overlay exited").await;
            let current_overlay_exited = {
                let mut overlay = daemon.overlay.lock().await;
                match overlay.as_mut() {
                    Some(process) => process.child.try_wait()?.is_some(),
                    None => true,
                }
            };
            if current_overlay_exited {
                dispose_overlay_process(daemon.overlay.lock().await.take());
                daemon.state.lock().await.overlay_visible = false;
                write_state(daemon).await?;
            } else {
                debug!("ignored stale overlay exit event while replacement overlay is running");
            }
        }
        OverlayEvent::Pong | OverlayEvent::CardRendered { .. } => {}
        OverlayEvent::Error { message } => {
            warn!("overlay error: {message}");
        }
        OverlayEvent::Lifecycle {
            stage,
            status,
            detail,
        } => {
            info!(
                overlay_stage = %stage,
                overlay_status = status.as_deref().unwrap_or(""),
                overlay_detail = detail.as_deref().unwrap_or(""),
                "overlay lifecycle"
            );
        }
    }

    Ok(())
}

async fn start_screen_capture(
    daemon: &Arc<Daemon>,
    interval_secs: u64,
    source: &str,
) -> Result<()> {
    ensure_screen_capture_supported()?;

    let (stop_tx, stop_rx) = oneshot::channel();
    {
        let mut capture = daemon.capture.lock().await;
        if capture.stop.is_some() {
            return Ok(());
        }
        capture.interval_secs = interval_secs;
        capture.stop = Some(stop_tx);
    }
    update_capture_state(daemon, true, Some(interval_secs)).await?;

    push_system_card(
        daemon,
        CardKind::System,
        "Screen context on",
        format!(
            "Bluey will attach a user-approved screenshot every {interval_secs}s. Source: {source}."
        ),
    )
    .await;

    let daemon_for_loop = daemon.clone();
    tokio::spawn(async move {
        capture_loop(daemon_for_loop, interval_secs, stop_rx).await;
    });

    Ok(())
}

async fn stop_screen_capture(daemon: &Arc<Daemon>, source: &str) -> Result<()> {
    let stop = {
        let mut capture = daemon.capture.lock().await;
        capture.stop.take()
    };

    if let Some(stop) = stop {
        let _ = stop.send(());
        update_capture_state(daemon, false, None).await?;
        push_system_card(
            daemon,
            CardKind::System,
            "Screen context off",
            format!("Screen context capture stopped. Source: {source}."),
        )
        .await;
    }

    Ok(())
}

async fn start_audio_capture(
    daemon: &Arc<Daemon>,
    config: AudioCaptureConfig,
) -> Result<AudioPipelineStatus> {
    let _ = stop_audio_capture(daemon).await;

    let session_id = format!("audio-{}", clock::now_epoch_ms_string());
    let (stop_tx, stop_rx) = oneshot::channel();

    let runtime = build_real_audio_runtime_config(&daemon.paths, &config).await?;
    let status = if let AudioRuntimeConfigResolution::Real(real_runtime) = runtime.clone() {
        let devices = real_runtime
            .sources
            .iter()
            .map(|source| source.device.clone())
            .collect::<Vec<_>>();
        let mut runtime_config = config.clone();
        runtime_config.chunk_duration_ms = real_runtime.chunk_duration_ms;
        runtime_config.system.enabled = real_runtime
            .sources
            .iter()
            .any(|source| source.source == AudioSourceKind::System);
        runtime_config.microphone.enabled = real_runtime
            .sources
            .iter()
            .any(|source| source.source == AudioSourceKind::Microphone);
        AudioPipelineStatus::native(
            session_id.clone(),
            runtime_config,
            devices,
            real_runtime.stt_provider_label.clone(),
            real_audio_platform_note(&real_runtime),
        )
    } else {
        let message = match runtime {
            AudioRuntimeConfigResolution::Unavailable(message) => message,
            AudioRuntimeConfigResolution::Real(_) => {
                "Audio capture is not available yet.".to_string()
            }
        };
        let status = failed_audio_status(config, &message);
        *daemon.audio.lock().await = status;
        return Err(anyhow!(message));
    };

    {
        let mut runtime = daemon.audio_runtime.lock().await;
        runtime.stop = Some(stop_tx);
        runtime.session_id = Some(session_id.clone());
    }
    *daemon.audio.lock().await = status.clone();

    let daemon_for_loop = daemon.clone();
    match runtime {
        AudioRuntimeConfigResolution::Real(real_runtime) => {
            tokio::spawn(async move {
                real_audio_loop(daemon_for_loop, session_id, real_runtime, stop_rx).await;
            });
        }
        AudioRuntimeConfigResolution::Unavailable(_) => {
            unreachable!("handled before runtime spawn")
        }
    }

    Ok(status)
}

/// Build an STT provider for the system audio continuous capture path.
async fn build_system_audio_stt_provider() -> anyhow::Result<Box<dyn cue_core::stt::SttProvider>> {
    if !dev_direct_stt_enabled() {
        anyhow::bail!(
            "direct streaming STT providers are debug/dev-only; release builds use Bluey managed STT"
        );
    }

    use cue_core::pcm::AudioSource;
    use cue_core::stt::SttConfig;

    let stt_cfg = SttConfig {
        source: AudioSource::System,
        ..Default::default()
    };

    crate::stt::factory::build_stt_chain(&stt_cfg, AudioSource::System)
        .await
        .map_err(|e| anyhow::anyhow!("STT factory: {e}"))
}

/// Build an STT provider for the microphone path via the factory chain.
/// Called when the streaming factory is preferred (e.g. LocalWhisper enabled).
pub async fn build_mic_stt_provider() -> anyhow::Result<Box<dyn cue_core::stt::SttProvider>> {
    if !dev_direct_stt_enabled() {
        anyhow::bail!(
            "direct streaming STT providers are debug/dev-only; release builds use Bluey managed STT"
        );
    }

    use cue_core::pcm::AudioSource;
    use cue_core::stt::SttConfig;

    let stt_cfg = SttConfig {
        source: AudioSource::Microphone,
        ..Default::default()
    };

    crate::stt::factory::build_stt_chain(&stt_cfg, AudioSource::Microphone)
        .await
        .map_err(|e| anyhow::anyhow!("STT factory (mic): {e}"))
}

async fn build_real_audio_runtime_config(
    paths: &AppPaths,
    config: &AudioCaptureConfig,
) -> Result<AudioRuntimeConfigResolution> {
    let ffmpeg_path = find_ffmpeg();
    let native_audio_helper = find_native_audio_helper();
    if ffmpeg_path.is_none() && native_audio_helper.is_none() {
        return Ok(AudioRuntimeConfigResolution::Unavailable(
            "Audio helper is missing. Reinstall Bluey from https://bluey.sh/download, then run bluey on again.".to_string(),
        ));
    }

    let sources = resolve_real_audio_sources(
        config,
        native_audio_helper.as_deref(),
        ffmpeg_path.as_deref(),
    )
    .await?;
    if sources.is_empty() {
        return Ok(AudioRuntimeConfigResolution::Unavailable(
            "No usable system or microphone audio source was found. Check macOS Microphone and Screen Recording permissions, then try Listen again.".to_string(),
        ));
    }

    let explicit_stt_api_key = if dev_direct_stt_enabled() {
        env_first(&["BLUEY_STT_API_KEY", "OPENAI_API_KEY"])
    } else {
        None
    };
    let account = load_account(paths).ok().flatten();
    let account_token = cloud_access_token_from_env().or_else(|| {
        let store = cue_cloud_client::SecureAccountStore::new(paths.clone());
        cue_cloud_client::TokenStore::load(&store)
            .ok()
            .flatten()
            .map(|tokens| tokens.access)
            .filter(|token| !token.trim().is_empty())
    });
    let account_api_url = env::var("BLUEY_CLOUD_API_URL")
        .or_else(|_| env::var("CUE_CLOUD_API_URL"))
        .ok()
        .or_else(|| account.as_ref().map(|account| account.api_url.clone()));
    let supports_live_relay = sources
        .iter()
        .all(|source| matches!(source.ffmpeg_input, FfmpegAudioInput::NativeHelper { .. }));

    let (stt_endpoint, stt_api_key, stt_model, stt_provider_label, stt_transport) = if let Some(
        explicit_stt_key,
    ) =
        explicit_stt_api_key
    {
        let stt_endpoint = env_first(&[
            "BLUEY_STT_API_URL",
            "OPENAI_AUDIO_TRANSCRIPTIONS_URL",
            "OPENAI_TRANSCRIPTIONS_URL",
        ])
        .unwrap_or_else(|| "https://api.openai.com/v1/audio/transcriptions".to_string());
        let stt_model = env_first(&["BLUEY_STT_MODEL", "OPENAI_STT_MODEL"])
            .unwrap_or_else(|| "whisper-1".into());
        let stt_provider_label =
            env_first(&["BLUEY_STT_PROVIDER"]).unwrap_or_else(|| format!("openai:{stt_model}"));
        (
            stt_endpoint,
            explicit_stt_key,
            stt_model,
            stt_provider_label,
            RealSttTransport::OpenAiMultipart,
        )
    } else {
        match (account_token, account_api_url) {
            (Some(token), Some(api_url)) => {
                let stt_model = env_first(&["BLUEY_STT_MODEL"]).unwrap_or_else(|| "nova-3".into());
                if env_truthy_any(&["BLUEY_STT_FORCE_CHUNKED", "BLUEY_MANAGED_STT_CHUNKED"])
                    || !supports_live_relay
                {
                    (
                        format!("{}/router/transcribe", api_url.trim_end_matches('/')),
                        token,
                        stt_model.clone(),
                        format!("bluey-managed:deepgram/{stt_model} chunked"),
                        RealSttTransport::BlueyManagedRaw,
                    )
                } else {
                    (
                        api_url.trim_end_matches('/').to_string(),
                        token,
                        stt_model.clone(),
                        format!("bluey-managed:deepgram/{stt_model} live"),
                        RealSttTransport::BlueyManagedRelay,
                    )
                }
            }
            _ => {
                return Ok(AudioRuntimeConfigResolution::Unavailable(
                    "Sign in to Bluey before using cloud speech-to-text. Local recording is ready, but Listen needs a linked account to transcribe real audio.".to_string(),
                ))
            }
        }
    };
    let chunk_duration_ms = real_stt_chunk_duration_ms(config.chunk_duration_ms);

    Ok(AudioRuntimeConfigResolution::Real(RealAudioRuntimeConfig {
        ffmpeg_path,
        stt_endpoint,
        stt_api_key,
        stt_model,
        stt_provider_label,
        stt_transport,
        chunk_duration_ms,
        sources,
    }))
}

fn failed_audio_status(config: AudioCaptureConfig, message: &str) -> AudioPipelineStatus {
    let mut status = AudioPipelineStatus::planned(config);
    status.capture = AudioCaptureStatus::failed(message);
    status.runtime_mode = AudioRuntimeMode::Unavailable;
    status.backend_ready = false;
    status.note = Some(message.to_string());
    status.updated_at = clock::now_epoch_ms_string();
    status
}

fn find_ffmpeg() -> Option<PathBuf> {
    let candidates = env_first(&["BLUEY_FFMPEG_PATH", "FFMPEG_PATH"])
        .map(PathBuf::from)
        .into_iter()
        .chain(std::iter::once(PathBuf::from("ffmpeg")));

    candidates
        .filter(|candidate| {
            Command::new(candidate)
                .arg("-version")
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .map(|status| status.success())
                .unwrap_or(false)
        })
        .next()
}

#[cfg(target_os = "macos")]
fn find_native_audio_helper() -> Option<PathBuf> {
    env_first(&["BLUEY_AUDIO_HELPER_BIN", "CUE_AUDIO_HELPER_BIN"])
        .map(PathBuf::from)
        .into_iter()
        .chain(
            env::current_exe()
                .ok()
                .and_then(|path| path.parent().map(Path::to_path_buf))
                .into_iter()
                .flat_map(|dir| {
                    [
                        dir.join("bluey-audio-macos"),
                        dir.join("cue-audio-macos"),
                        dir.join("../../native/macos/cue-audio/.build/bluey-audio-macos"),
                        dir.join("../native/macos/cue-audio/.build/bluey-audio-macos"),
                    ]
                }),
        )
        .chain([
            PathBuf::from("native/macos/cue-audio/.build/bluey-audio-macos"),
            PathBuf::from("./bluey-audio-macos"),
        ])
        .find(|path| path.exists())
}

#[cfg(target_os = "windows")]
fn find_native_audio_helper() -> Option<PathBuf> {
    env_first(&["BLUEY_AUDIO_HELPER_BIN", "CUE_AUDIO_HELPER_BIN"])
        .map(PathBuf::from)
        .into_iter()
        .chain(
            env::current_exe()
                .ok()
                .and_then(|path| path.parent().map(Path::to_path_buf))
                .into_iter()
                .flat_map(|dir| {
                    [
                        dir.join("bluey-audio.exe"),
                        dir.join("cue-audio.exe"),
                        dir.join("../../native/windows/cue-audio/build/bluey-audio.exe"),
                        dir.join("../native/windows/cue-audio/build/bluey-audio.exe"),
                    ]
                }),
        )
        .chain([
            PathBuf::from("native/windows/cue-audio/build/bluey-audio.exe"),
            PathBuf::from("./bluey-audio.exe"),
        ])
        .find(|path| path.exists())
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn find_native_audio_helper() -> Option<PathBuf> {
    None
}

fn env_first(names: &[&str]) -> Option<String> {
    names
        .iter()
        .find_map(|name| env::var(name).ok())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn url_component(value: &str) -> String {
    value
        .bytes()
        .flat_map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                vec![byte as char]
            }
            _ => format!("%{byte:02X}").chars().collect(),
        })
        .collect()
}

fn env_truthy_any(names: &[&str]) -> bool {
    names.iter().any(|name| {
        env::var(name)
            .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
            .unwrap_or(false)
    })
}

fn dev_env_truthy_any(names: &[&str]) -> bool {
    cfg!(debug_assertions) && env_truthy_any(names)
}

fn dev_direct_provider_keys_enabled() -> bool {
    dev_env_truthy_any(&["BLUEY_DEV_DIRECT_PROVIDERS"])
}

fn dev_direct_stt_enabled() -> bool {
    dev_env_truthy_any(&["BLUEY_DEV_DIRECT_STT", "BLUEY_DEV_DIRECT_PROVIDERS"])
}

fn dev_direct_vision_enabled() -> bool {
    dev_env_truthy_any(&["BLUEY_DEV_DIRECT_VISION", "BLUEY_DEV_DIRECT_PROVIDERS"])
}

fn real_stt_chunk_duration_ms(configured: u32) -> u32 {
    env_first(&["BLUEY_STT_CHUNK_MS", "CUE_STT_CHUNK_MS"])
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or_else(|| configured.max(1_000))
        .clamp(500, 15_000)
}

async fn resolve_real_audio_sources(
    config: &AudioCaptureConfig,
    native_audio_helper: Option<&Path>,
    ffmpeg_path: Option<&Path>,
) -> Result<Vec<RealAudioSource>> {
    resolve_platform_audio_sources(config, native_audio_helper, ffmpeg_path).await
}

#[cfg(target_os = "macos")]
async fn resolve_platform_audio_sources(
    config: &AudioCaptureConfig,
    native_audio_helper: Option<&Path>,
    ffmpeg_path: Option<&Path>,
) -> Result<Vec<RealAudioSource>> {
    if let Some(helper_path) = native_audio_helper {
        let mut sources = Vec::new();
        if config.system.enabled {
            let device = AudioDeviceDescriptor::new(
                AudioSourceKind::System,
                AudioBackend::ScreenCaptureKit,
                "native_screen_capture_kit_audio",
                "Native system audio",
            )
            .with_role(AudioDeviceRole::Loopback)
            .with_manufacturer("Bluey ScreenCaptureKit helper")
            .with_format(16_000, 1);
            sources.push(RealAudioSource {
                source: AudioSourceKind::System,
                stream_id: "system-native".to_string(),
                ffmpeg_input: FfmpegAudioInput::NativeHelper {
                    helper_path: helper_path.to_path_buf(),
                    source_arg: "system".to_string(),
                },
                device,
            });
        }
        if config.microphone.enabled {
            let device = AudioDeviceDescriptor::new(
                AudioSourceKind::Microphone,
                AudioBackend::CoreAudio,
                "native_core_audio_microphone",
                "Default microphone",
            )
            .with_role(AudioDeviceRole::Input)
            .with_manufacturer("Bluey AVFoundation helper")
            .with_format(16_000, 1);
            sources.push(RealAudioSource {
                source: AudioSourceKind::Microphone,
                stream_id: "microphone-native".to_string(),
                ffmpeg_input: FfmpegAudioInput::NativeHelper {
                    helper_path: helper_path.to_path_buf(),
                    source_arg: "microphone".to_string(),
                },
                device,
            });
        }
        if !sources.is_empty() {
            return Ok(sources);
        }
    }

    let Some(ffmpeg_path) = ffmpeg_path else {
        return Ok(Vec::new());
    };
    let devices = discover_macos_avfoundation_audio_devices(ffmpeg_path).unwrap_or_default();
    let mut sources = Vec::new();

    if config.system.enabled {
        if let Some(device_name) = select_macos_system_device(&devices, &config.system.device_id) {
            let device = AudioDeviceDescriptor::new(
                AudioSourceKind::System,
                AudioBackend::CoreAudio,
                device_name.clone(),
                device_name.clone(),
            )
            .with_role(AudioDeviceRole::Loopback)
            .with_manufacturer("FFmpeg AVFoundation")
            .with_format(16_000, 1);
            sources.push(RealAudioSource {
                source: AudioSourceKind::System,
                stream_id: "system-real".to_string(),
                ffmpeg_input: FfmpegAudioInput::MacAvFoundation { device_name },
                device,
            });
        }
    }

    if config.microphone.enabled {
        if let Some(device_name) =
            select_macos_microphone_device(&devices, &config.microphone.device_id)
        {
            let device = AudioDeviceDescriptor::new(
                AudioSourceKind::Microphone,
                AudioBackend::CoreAudio,
                device_name.clone(),
                device_name.clone(),
            )
            .with_role(AudioDeviceRole::Input)
            .with_manufacturer("FFmpeg AVFoundation")
            .with_format(16_000, 1);
            sources.push(RealAudioSource {
                source: AudioSourceKind::Microphone,
                stream_id: "microphone-real".to_string(),
                ffmpeg_input: FfmpegAudioInput::MacAvFoundation { device_name },
                device,
            });
        }
    }

    Ok(sources)
}

#[cfg(target_os = "windows")]
async fn resolve_platform_audio_sources(
    config: &AudioCaptureConfig,
    native_audio_helper: Option<&Path>,
    ffmpeg_path: Option<&Path>,
) -> Result<Vec<RealAudioSource>> {
    if let Some(helper_path) = native_audio_helper {
        let mut sources = Vec::new();
        if config.system.enabled {
            let device = AudioDeviceDescriptor::new(
                AudioSourceKind::System,
                AudioBackend::Wasapi,
                "native_wasapi_loopback_audio",
                "Native system audio",
            )
            .with_role(AudioDeviceRole::Loopback)
            .with_manufacturer("Bluey WASAPI helper")
            .with_format(16_000, 1);
            sources.push(RealAudioSource {
                source: AudioSourceKind::System,
                stream_id: "system-native".to_string(),
                ffmpeg_input: FfmpegAudioInput::NativeHelper {
                    helper_path: helper_path.to_path_buf(),
                    source_arg: "system".to_string(),
                },
                device,
            });
        }
        if config.microphone.enabled {
            let device = AudioDeviceDescriptor::new(
                AudioSourceKind::Microphone,
                AudioBackend::Wasapi,
                "native_wasapi_microphone",
                "Default microphone",
            )
            .with_role(AudioDeviceRole::Input)
            .with_manufacturer("Bluey WASAPI helper")
            .with_format(16_000, 1);
            sources.push(RealAudioSource {
                source: AudioSourceKind::Microphone,
                stream_id: "microphone-native".to_string(),
                ffmpeg_input: FfmpegAudioInput::NativeHelper {
                    helper_path: helper_path.to_path_buf(),
                    source_arg: "microphone".to_string(),
                },
                device,
            });
        }
        if !sources.is_empty() {
            return Ok(sources);
        }
    }

    if ffmpeg_path.is_none() {
        return Ok(Vec::new());
    }

    let mut sources = Vec::new();

    if config.system.enabled {
        let device_name = config
            .system
            .device_id
            .clone()
            .or_else(|| {
                env_first(&[
                    "BLUEY_SYSTEM_AUDIO_DEVICE",
                    "BLUEY_AUDIO_SYSTEM_DEVICE",
                    "BLUEY_LOOPBACK_AUDIO_DEVICE",
                    "CUE_SYSTEM_AUDIO_DEVICE",
                ])
            })
            .unwrap_or_else(|| "default".to_string());
        let device = AudioDeviceDescriptor::new(
            AudioSourceKind::System,
            AudioBackend::Wasapi,
            device_name.clone(),
            device_name.clone(),
        )
        .with_role(AudioDeviceRole::Loopback)
        .with_manufacturer("FFmpeg WASAPI")
        .with_format(16_000, 1);
        sources.push(RealAudioSource {
            source: AudioSourceKind::System,
            stream_id: "system-real".to_string(),
            ffmpeg_input: FfmpegAudioInput::WindowsWasapiLoopback { device_name },
            device,
        });
    }

    if config.microphone.enabled {
        let device_name = config
            .microphone
            .device_id
            .clone()
            .or_else(|| {
                env_first(&[
                    "BLUEY_MIC_AUDIO_DEVICE",
                    "BLUEY_MICROPHONE_AUDIO_DEVICE",
                    "BLUEY_AUDIO_MIC_DEVICE",
                    "CUE_MIC_AUDIO_DEVICE",
                ])
            })
            .unwrap_or_else(|| "default".to_string());
        let device = AudioDeviceDescriptor::new(
            AudioSourceKind::Microphone,
            AudioBackend::Wasapi,
            device_name.clone(),
            device_name.clone(),
        )
        .with_role(AudioDeviceRole::Input)
        .with_manufacturer("FFmpeg DirectShow")
        .with_format(16_000, 1);
        sources.push(RealAudioSource {
            source: AudioSourceKind::Microphone,
            stream_id: "microphone-real".to_string(),
            ffmpeg_input: FfmpegAudioInput::WindowsDshow { device_name },
            device,
        });
    }

    Ok(sources)
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
async fn resolve_platform_audio_sources(
    _config: &AudioCaptureConfig,
    _native_audio_helper: Option<&Path>,
    _ffmpeg_path: Option<&Path>,
) -> Result<Vec<RealAudioSource>> {
    Ok(Vec::new())
}

#[cfg(target_os = "macos")]
fn discover_macos_avfoundation_audio_devices(ffmpeg_path: &Path) -> Result<Vec<String>> {
    let output = Command::new(ffmpeg_path)
        .args([
            "-hide_banner",
            "-f",
            "avfoundation",
            "-list_devices",
            "true",
            "-i",
            "",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .context("failed to list AVFoundation audio devices with ffmpeg")?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    let mut in_audio_devices = false;
    let mut devices = Vec::new();

    for line in stderr.lines() {
        if line.contains("AVFoundation audio devices:") {
            in_audio_devices = true;
            continue;
        }
        if line.contains("AVFoundation video devices:") {
            in_audio_devices = false;
            continue;
        }
        if !in_audio_devices {
            continue;
        }
        if let Some((_, name)) = line.rsplit_once("] ") {
            let name = name.trim();
            if !name.is_empty() && !name.contains("Error opening input") {
                devices.push(name.to_string());
            }
        }
    }

    Ok(devices)
}

#[cfg(target_os = "macos")]
fn select_macos_system_device(devices: &[String], requested: &Option<String>) -> Option<String> {
    requested
        .clone()
        .or_else(|| {
            env_first(&[
                "BLUEY_SYSTEM_AUDIO_DEVICE",
                "BLUEY_AUDIO_SYSTEM_DEVICE",
                "BLUEY_LOOPBACK_AUDIO_DEVICE",
                "CUE_SYSTEM_AUDIO_DEVICE",
            ])
        })
        .map(|requested| match_device_name(devices, &requested))
        .or_else(|| {
            devices
                .iter()
                .find(|name| name.to_ascii_lowercase().contains("blackhole"))
                .cloned()
        })
        .or_else(|| {
            devices
                .iter()
                .find(|name| {
                    let lower = name.to_ascii_lowercase();
                    lower.contains("loopback") || lower.contains("output")
                })
                .cloned()
        })
}

#[cfg(target_os = "macos")]
fn select_macos_microphone_device(
    devices: &[String],
    requested: &Option<String>,
) -> Option<String> {
    requested
        .clone()
        .or_else(|| {
            env_first(&[
                "BLUEY_MIC_AUDIO_DEVICE",
                "BLUEY_MICROPHONE_AUDIO_DEVICE",
                "BLUEY_AUDIO_MIC_DEVICE",
                "CUE_MIC_AUDIO_DEVICE",
            ])
        })
        .map(|requested| match_device_name(devices, &requested))
        .or_else(|| best_macos_microphone_device(devices))
}

#[cfg(target_os = "macos")]
fn match_device_name(devices: &[String], requested: &str) -> String {
    let requested_lower = requested.to_ascii_lowercase();
    devices
        .iter()
        .find(|name| name.eq_ignore_ascii_case(requested))
        .or_else(|| {
            devices
                .iter()
                .find(|name| name.to_ascii_lowercase().contains(&requested_lower))
        })
        .cloned()
        .unwrap_or_else(|| requested.to_string())
}

#[cfg(target_os = "macos")]
fn best_macos_microphone_device(devices: &[String]) -> Option<String> {
    devices
        .iter()
        .find(|name| {
            let lower = name.to_ascii_lowercase();
            lower.contains("macbook") && lower.contains("microphone")
        })
        .or_else(|| {
            devices.iter().find(|name| {
                let lower = name.to_ascii_lowercase();
                lower.contains("microphone") && !lower.contains("nomachine")
            })
        })
        .or_else(|| {
            devices.iter().find(|name| {
                let lower = name.to_ascii_lowercase();
                lower.contains("microphone")
            })
        })
        .or_else(|| {
            devices.iter().find(|name| {
                let lower = name.to_ascii_lowercase();
                !lower.contains("blackhole")
                    && !lower.contains("output")
                    && !lower.contains("aggregate")
            })
        })
        .cloned()
}

fn real_audio_platform_note(runtime: &RealAudioRuntimeConfig) -> String {
    let source_list = runtime
        .sources
        .iter()
        .map(|source| format!("{}: {}", source.source, source.device.name))
        .collect::<Vec<_>>()
        .join(", ");
    let backend = if runtime
        .sources
        .iter()
        .any(|source| matches!(source.ffmpeg_input, FfmpegAudioInput::NativeHelper { .. }))
    {
        if cfg!(target_os = "macos") {
            "native macOS helper"
        } else if cfg!(target_os = "windows") {
            "native Windows helper"
        } else {
            "native helper"
        }
    } else {
        "FFmpeg"
    };
    format!(
        "Real {backend} chunk capture is active with {} STT. Sources: {source_list}.",
        runtime.stt_provider_label
    )
}

async fn real_audio_loop(
    daemon: Arc<Daemon>,
    session_id: String,
    runtime: RealAudioRuntimeConfig,
    mut stop_rx: oneshot::Receiver<()>,
) {
    if runtime.stt_transport == RealSttTransport::BlueyManagedRelay {
        real_audio_relay_loop(daemon, session_id, runtime, stop_rx).await;
        return;
    }

    let client = reqwest::Client::new();
    let mut system_sequence = 0_u64;
    let mut microphone_sequence = 0_u64;
    let mut warned_stt_error = false;
    let idle_timeout = audio_idle_stop_timeout();
    let mut last_transcript_at = Instant::now();

    loop {
        let mut source_jobs = FuturesUnordered::new();
        for source in &runtime.sources {
            let sequence = match source.source {
                AudioSourceKind::System => {
                    system_sequence = system_sequence.saturating_add(1);
                    system_sequence
                }
                AudioSourceKind::Microphone => {
                    microphone_sequence = microphone_sequence.saturating_add(1);
                    microphone_sequence
                }
            };

            let daemon_ref = &daemon;
            let session_id_ref = &session_id;
            let runtime_ref = &runtime;
            let client_ref = &client;
            source_jobs.push(async move {
                let source_kind = source.source;
                let result = capture_transcribe_audio_chunk(
                    daemon_ref,
                    session_id_ref,
                    runtime_ref,
                    source,
                    sequence,
                    client_ref,
                )
                .await;
                (source_kind, result)
            });
        }

        while let Some((source_kind, result)) = source_jobs.next().await {
            match result {
                Ok(Some(segment)) => {
                    last_transcript_at = Instant::now();
                    if let Err(error) = add_audio_transcript_segment(&daemon, &segment).await {
                        warn!("real audio transcript emission failed: {error:#}");
                    } else {
                        daemon.audio.lock().await.record_stt_segment();
                    }
                }
                Ok(None) => {}
                Err(error) => {
                    let message = compact_snippet(&format!("{error:#}"), 260);
                    let is_permission = crate::audio::capture::is_permission_denied_message(
                        &message,
                    )
                        || crate::audio::system_capture::is_system_audio_permission_denied_message(
                            &message,
                        );
                    {
                        let mut audio = daemon.audio.lock().await;
                        if audio.session_id.as_deref() != Some(session_id.as_str()) {
                            return;
                        }
                        audio.record_drop(source_kind, message.clone());
                        if is_permission {
                            audio.capture.permission_denied_source = Some(source_kind);
                        }
                    }
                    if !warned_stt_error {
                        warned_stt_error = true;
                        push_system_card(
                            &daemon,
                            CardKind::Warning,
                            if is_permission {
                                "Audio permission denied"
                            } else {
                                "Audio transcription needs attention"
                            },
                            message,
                        )
                        .await;
                    }
                }
            }

            if daemon.audio.lock().await.session_id.as_deref() != Some(session_id.as_str()) {
                return;
            }
            if maybe_auto_stop_idle_audio(&daemon, &session_id, last_transcript_at, idle_timeout)
                .await
            {
                return;
            }
        }

        tokio::select! {
            _ = &mut stop_rx => return,
            _ = sleep(Duration::from_millis(80)) => {
                if maybe_auto_stop_idle_audio(
                    &daemon,
                    &session_id,
                    last_transcript_at,
                    idle_timeout,
                )
                .await
                {
                    return;
                }
            }
        }
    }
}

async fn real_audio_relay_loop(
    daemon: Arc<Daemon>,
    session_id: String,
    runtime: RealAudioRuntimeConfig,
    mut stop_rx: oneshot::Receiver<()>,
) {
    let source_count = runtime.sources.len();
    if source_count == 0 {
        return;
    }

    let (relay_stop_tx, relay_stop_rx) = watch::channel(false);
    let (done_tx, mut done_rx) = mpsc::channel::<()>(source_count);
    let idle_timeout = audio_idle_stop_timeout();
    let last_transcript_at = Arc::new(Mutex::new(Instant::now()));
    let mut handles = Vec::with_capacity(source_count);

    for source in runtime.sources.clone() {
        let daemon_for_source = Arc::clone(&daemon);
        let session_id_for_source = session_id.clone();
        let runtime_for_source = runtime.clone();
        let mut source_stop_rx = relay_stop_rx.clone();
        let done_tx = done_tx.clone();
        let last_transcript_at = Arc::clone(&last_transcript_at);
        let source_kind = source.source;
        handles.push(tokio::spawn(async move {
            if let Err(error) = run_relay_audio_source(
                Arc::clone(&daemon_for_source),
                session_id_for_source.clone(),
                runtime_for_source,
                source,
                &mut source_stop_rx,
                last_transcript_at,
            )
            .await
            {
                let message = compact_snippet(&format!("{error:#}"), 260);
                let is_permission = crate::audio::capture::is_permission_denied_message(&message)
                    || crate::audio::system_capture::is_system_audio_permission_denied_message(
                        &message,
                    );
                {
                    let mut audio = daemon_for_source.audio.lock().await;
                    if audio.session_id.as_deref() == Some(session_id_for_source.as_str()) {
                        audio.record_drop(source_kind, message.clone());
                        if is_permission {
                            audio.capture.permission_denied_source = Some(source_kind);
                        }
                    }
                }
                push_system_card(
                    &daemon_for_source,
                    CardKind::Warning,
                    if is_permission {
                        "Audio permission denied"
                    } else {
                        "Live transcription needs attention"
                    },
                    message,
                )
                .await;
            }
            let _ = done_tx.send(()).await;
        }));
    }
    drop(done_tx);

    let mut completed_sources = 0_usize;
    loop {
        tokio::select! {
            _ = &mut stop_rx => {
                let _ = relay_stop_tx.send(true);
                break;
            }
            Some(()) = done_rx.recv() => {
                completed_sources = completed_sources.saturating_add(1);
                if completed_sources >= source_count {
                    break;
                }
            }
            _ = sleep(Duration::from_secs(1)) => {
                if daemon.audio.lock().await.session_id.as_deref() != Some(session_id.as_str()) {
                    let _ = relay_stop_tx.send(true);
                    break;
                }
                let last_transcript_at = *last_transcript_at.lock().await;
                if maybe_auto_stop_idle_audio(&daemon, &session_id, last_transcript_at, idle_timeout).await {
                    let _ = relay_stop_tx.send(true);
                    break;
                }
            }
        }
    }

    let _ = relay_stop_tx.send(true);
    for handle in handles {
        let _ = tokio::time::timeout(Duration::from_secs(2), handle).await;
    }
    if completed_sources >= source_count
        && daemon
            .audio
            .lock()
            .await
            .session_id
            .as_deref()
            .is_some_and(|active| active == session_id)
    {
        let _ = stop_audio_capture(&daemon).await;
        set_overlay_listening_state(&daemon, ListeningState::Paused).await;
        push_system_card(
            &daemon,
            CardKind::System,
            "Listening stopped",
            "Live transcription ended. Press Listen to start a fresh stream.",
        )
        .await;
    }
}

async fn run_relay_audio_source(
    daemon: Arc<Daemon>,
    session_id: String,
    runtime: RealAudioRuntimeConfig,
    source: RealAudioSource,
    stop_rx: &mut watch::Receiver<bool>,
    last_transcript_at: Arc<Mutex<Instant>>,
) -> Result<()> {
    let (helper_path, source_arg) = match &source.ffmpeg_input {
        FfmpegAudioInput::NativeHelper {
            helper_path,
            source_arg,
        } => (helper_path.clone(), source_arg.clone()),
        _ => {
            return Err(anyhow!(
                "live STT relay requires the native audio helper for {}",
                source.source
            ));
        }
    };

    let cloud = build_cloud_client(&daemon.paths, None)
        .with_context(|| format!("failed to create Bluey cloud client for {}", source.source))?;
    let stt_session = cloud
        .create_stt_session(&cue_cloud_client::SttSessionRequest {
            session_id: session_id.clone(),
            source: source.source.default_label().to_string(),
            provider: Some("deepgram".to_string()),
            model: Some(runtime.stt_model.clone()),
            requested_seconds: Some(10 * 60),
        })
        .await
        .with_context(|| format!("failed to create live STT session for {}", source.source))?;
    let access_token = cloud
        .current_tokens()
        .context("Bluey account token unavailable after live STT session creation")?
        .access;
    let websocket_url = stt_relay_websocket_url(
        stt_session
            .websocket_url
            .as_deref()
            .context("Bluey STT session did not include a websocket URL")?,
        &stt_session.session_token,
    )?;
    let mut request = websocket_url.into_client_request()?;
    request.headers_mut().insert(
        "Authorization",
        HeaderValue::from_str(&format!("Bearer {access_token}"))?,
    );

    let (socket, _) = tokio_tungstenite::connect_async(request)
        .await
        .with_context(|| format!("failed to open live STT websocket for {}", source.source))?;
    let (mut ws_tx, mut ws_rx) = socket.split();

    let mut command = TokioCommand::new(&helper_path);
    command
        .arg("--source")
        .arg(&source_arg)
        .arg("--continuous")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let mut child = command.spawn().with_context(|| {
        format!(
            "failed to start native live audio helper for {}",
            source.source
        )
    })?;
    let mut stdout = child
        .stdout
        .take()
        .context("native audio helper did not expose stdout")?;

    let mut sequence = 0_u64;
    let mut start_ms = 0_u64;
    let mut buffer = vec![0_u8; 4096];
    loop {
        tokio::select! {
            changed = stop_rx.changed() => {
                if changed.is_err() || *stop_rx.borrow() {
                    break;
                }
            }
            read = stdout.read(&mut buffer) => {
                let read = read.with_context(|| format!("failed to read live {} audio", source.source))?;
                if read == 0 {
                    break;
                }
                sequence = sequence.saturating_add(1);
                let duration_ms = pcm16_16k_duration_ms(read);
                let chunk = AudioChunkMetadata::new(
                    source.source,
                    source.stream_id.clone(),
                    sequence,
                    start_ms,
                    duration_ms,
                    cue_core::AudioStreamFormat::stt_mono(),
                    read as u64,
                );
                start_ms = start_ms.saturating_add(duration_ms as u64);
                {
                    let mut audio = daemon.audio.lock().await;
                    if audio.session_id.as_deref() != Some(session_id.as_str()) {
                        break;
                    }
                    audio.record_chunk(&chunk);
                }
                ws_tx
                    .send(WebSocketMessage::Binary(buffer[..read].to_vec()))
                    .await
                    .with_context(|| format!("failed to send live {} audio to Bluey STT relay", source.source))?;
            }
            message = ws_rx.next() => {
                match message {
                    Some(Ok(WebSocketMessage::Text(payload))) => {
                        emit_deepgram_relay_payload(
                            &daemon,
                            &session_id,
                            source.source,
                            sequence,
                            &payload,
                            Arc::clone(&last_transcript_at),
                        ).await?;
                    }
                    Some(Ok(WebSocketMessage::Binary(payload))) => {
                        if let Ok(payload) = std::str::from_utf8(&payload) {
                            emit_deepgram_relay_payload(
                                &daemon,
                                &session_id,
                                source.source,
                                sequence,
                                payload,
                                Arc::clone(&last_transcript_at),
                            ).await?;
                        }
                    }
                    Some(Ok(WebSocketMessage::Close(_))) | None => break,
                    Some(Ok(WebSocketMessage::Ping(_))) | Some(Ok(WebSocketMessage::Pong(_))) | Some(Ok(WebSocketMessage::Frame(_))) => {}
                    Some(Err(error)) => {
                        return Err(anyhow!("live STT websocket failed for {}: {error}", source.source));
                    }
                }
            }
        }
    }

    let _ = ws_tx.send(WebSocketMessage::Close(None)).await;
    let _ = child.kill().await;
    let _ = child.wait().await;
    Ok(())
}

async fn emit_deepgram_relay_payload(
    daemon: &Arc<Daemon>,
    session_id: &str,
    source: AudioSourceKind,
    sequence: u64,
    payload: &str,
    last_transcript_at: Arc<Mutex<Instant>>,
) -> Result<()> {
    let pcm_source = pcm_source_for_audio_source(source);
    let events = crate::stt::deepgram::parse_frame(payload, pcm_source)
        .map_err(|error| anyhow!("Deepgram relay frame parse failed: {error}"))?;
    for event in events {
        let Some(segment) = transcript_event_to_stt_segment(&event) else {
            continue;
        };
        let segment = segment
            .with_provider_segment_id(format!("relay-{}-{sequence}", source.default_label()))
            .with_source_sequence_range(sequence, sequence);
        *last_transcript_at.lock().await = Instant::now();
        if let Err(error) = add_audio_transcript_segment(daemon, &segment).await {
            warn!("live relay transcript emission failed: {error:#}");
        } else if segment.is_final {
            daemon.audio.lock().await.record_stt_segment();
        }
        if daemon.audio.lock().await.session_id.as_deref() != Some(session_id) {
            break;
        }
    }
    Ok(())
}

fn stt_relay_websocket_url(endpoint: &str, session_token: &str) -> Result<String> {
    let endpoint = endpoint.trim();
    if endpoint.is_empty() {
        return Err(anyhow!("empty Bluey STT relay URL"));
    }
    let mut url = if let Some(rest) = endpoint.strip_prefix("https://") {
        format!("wss://{rest}")
    } else if let Some(rest) = endpoint.strip_prefix("http://") {
        format!("ws://{rest}")
    } else {
        endpoint.to_string()
    };
    let sep = if url.contains('?') { '&' } else { '?' };
    url.push(sep);
    url.push_str("session_token=");
    url.push_str(&url_component(session_token));
    Ok(url)
}

fn pcm_source_for_audio_source(source: AudioSourceKind) -> cue_core::pcm::AudioSource {
    match source {
        AudioSourceKind::System => cue_core::pcm::AudioSource::System,
        AudioSourceKind::Microphone => cue_core::pcm::AudioSource::Microphone,
    }
}

fn pcm16_16k_duration_ms(byte_len: usize) -> u32 {
    let samples = (byte_len / 2) as u64;
    ((samples.saturating_mul(1_000) / 16_000)
        .max(1)
        .min(u32::MAX as u64)) as u32
}

async fn maybe_auto_stop_idle_audio(
    daemon: &Arc<Daemon>,
    session_id: &str,
    last_transcript_at: Instant,
    idle_timeout: Duration,
) -> bool {
    if last_transcript_at.elapsed() < idle_timeout {
        return false;
    }

    let is_current_session = daemon
        .audio
        .lock()
        .await
        .session_id
        .as_deref()
        .is_some_and(|active| active == session_id);
    if !is_current_session {
        return true;
    }

    let _status = stop_audio_capture(daemon).await;
    set_overlay_listening_state(daemon, ListeningState::Paused).await;
    {
        let mut audio = daemon.audio.lock().await;
        audio.note = Some(format!(
            "Recording auto-stopped after {} with no transcribed audio.",
            format_duration(idle_timeout)
        ));
        audio.updated_at = clock::now_epoch_ms_string();
    }

    let balance = refresh_overlay_balance(daemon, None).await;
    let balance_line = balance
        .map(|label| format!("\nFinal balance: {label}."))
        .unwrap_or_else(|| {
            "\nFinal balance unavailable; sign in to Bluey to show wallet balance.".to_string()
        });
    push_system_card(
        daemon,
        CardKind::System,
        "Recording auto-stopped",
        format!(
            "No audio was transcribed for {}. Bluey stopped recording to cut STT costs.{}",
            format_duration(idle_timeout),
            balance_line
        ),
    )
    .await;
    true
}

fn audio_idle_stop_timeout() -> Duration {
    let secs = env_first(&["BLUEY_AUDIO_IDLE_STOP_SECS", "CUE_AUDIO_IDLE_STOP_SECS"])
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(DEFAULT_AUDIO_IDLE_STOP_SECS)
        .max(1);
    Duration::from_secs(secs)
}

fn recording_sources_label(status: &AudioPipelineStatus) -> &'static str {
    match status.runtime_mode {
        AudioRuntimeMode::Native => "Bluey is listening to system audio and microphone.",
        AudioRuntimeMode::SimulatedDevelopment => {
            "Local preview audio is active. Real STT starts after permissions and provider setup."
        }
        AudioRuntimeMode::Idle => "Bluey is ready to listen.",
        AudioRuntimeMode::Unavailable => {
            "Audio is not available yet. Check permissions or provider setup."
        }
    }
}

fn format_duration(duration: Duration) -> String {
    let total = duration.as_secs();
    let minutes = total / 60;
    let seconds = total % 60;
    match (minutes, seconds) {
        (0, 1) => "1 second".to_string(),
        (0, s) => format!("{s} seconds"),
        (1, 0) => "1 minute".to_string(),
        (m, 0) => format!("{m} minutes"),
        (1, 1) => "1 minute 1 second".to_string(),
        (1, s) => format!("1 minute {s} seconds"),
        (m, 1) => format!("{m} minutes 1 second"),
        (m, s) => format!("{m} minutes {s} seconds"),
    }
}

async fn refresh_overlay_balance(daemon: &Arc<Daemon>, trace_id: Option<&str>) -> Option<String> {
    let snapshot = fetch_current_balance_snapshot(trace_id).await?;
    let label = format_balance_cents(snapshot.balance_cents);
    daemon.balance_watch.publish(snapshot);
    let _ = send_overlay(
        daemon,
        OverlayCommand::SetBalance {
            label: label.clone(),
        },
    )
    .await;
    Some(label)
}

async fn refresh_overlay_context_items(daemon: &Arc<Daemon>, meeting: &MeetingRecord) {
    let _ = send_overlay(
        daemon,
        OverlayCommand::SetContextItems {
            items: overlay_context_items(meeting),
        },
    )
    .await;
}

async fn refresh_overlay_sessions(daemon: &Arc<Daemon>) {
    let active_id = daemon.meeting.lock().await.as_ref().map(|m| m.id);
    match overlay_session_items(daemon, active_id) {
        Ok(sessions) => {
            let _ = send_overlay(daemon, OverlayCommand::SetSessions { sessions }).await;
        }
        Err(error) => {
            debug!("overlay session list refresh skipped: {error:#}");
        }
    }
}

fn overlay_session_items(
    daemon: &Arc<Daemon>,
    active_id: Option<uuid::Uuid>,
) -> Result<Vec<OverlaySessionItem>> {
    let mut seen = std::collections::HashSet::new();
    let mut items = Vec::new();
    for meeting in daemon.store.all_meetings()? {
        if !seen.insert(meeting.id) {
            continue;
        }
        let is_active = Some(meeting.id) == active_id;
        let has_content = !meeting.transcript.is_empty()
            || !meeting.context.is_empty()
            || !meeting.conversation.is_empty()
            || meeting
                .summary
                .as_ref()
                .is_some_and(|summary| !summary.trim().is_empty());
        if !is_active && !has_content {
            continue;
        }
        let mut bits = Vec::new();
        if is_active || meeting.ended_at.is_none() {
            bits.push("active".to_string());
        }
        if !meeting.transcript.is_empty() {
            bits.push(format!("{} transcript", meeting.transcript.len()));
        }
        if !meeting.context.is_empty() {
            bits.push(format!(
                "{} file{}",
                meeting.context.len(),
                plural_s(meeting.context.len())
            ));
        }
        if !meeting.conversation.is_empty() {
            bits.push(format!(
                "{} answer{}",
                meeting.conversation.len(),
                plural_s(meeting.conversation.len())
            ));
        }
        items.push(OverlaySessionItem {
            id: meeting.id,
            title: meeting.title,
            subtitle: if bits.is_empty() {
                "saved recording".to_string()
            } else {
                bits.join(" · ")
            },
            is_active,
        });
    }
    Ok(items.into_iter().take(8).collect())
}

fn plural_s(count: usize) -> &'static str {
    if count == 1 {
        ""
    } else {
        "s"
    }
}

fn overlay_context_items(meeting: &MeetingRecord) -> Vec<OverlayContextItem> {
    meeting
        .context
        .iter()
        .map(|item| OverlayContextItem {
            id: item.id,
            title: item.title.clone(),
            kind: item.kind.to_string(),
            path: Some(item.path.clone()),
        })
        .collect()
}

async fn fetch_current_balance_snapshot(
    trace_id: Option<&str>,
) -> Option<crate::cloud::balance::BalanceSnapshot> {
    let paths = match AppPaths::discover() {
        Ok(paths) => paths,
        Err(error) => {
            debug!("balance lookup skipped; app paths unavailable: {error}");
            return None;
        }
    };
    let client = match build_cloud_client(&paths, trace_id) {
        Ok(client) => client,
        Err(error) => {
            debug!("balance lookup skipped; account store unavailable: {error}");
            return None;
        }
    };

    match tokio::time::timeout(
        Duration::from_secs(3),
        client.auth_get::<cue_cloud_client::AccountMe>("/account/me"),
    )
    .await
    {
        Ok(Ok(me)) => Some(crate::cloud::balance::BalanceSnapshot {
            balance_cents: me.balance_cents,
            trial_seconds_remaining: me.trial_seconds_remaining,
            auto_topup_enabled: me.auto_topup_enabled,
            auto_topup_threshold_cents: me.auto_topup_threshold_cents,
            auto_topup_amount_cents: me.auto_topup_amount_cents,
            fetched_at_unix_ms: clock::now_epoch_ms_string().parse().unwrap_or_default(),
            low_balance_warning: me.balance_cents < me.auto_topup_threshold_cents
                && me.balance_cents > 0,
        }),
        Ok(Err(error)) => {
            debug!("balance lookup skipped: {error}");
            None
        }
        Err(_) => {
            debug!("balance lookup skipped: timed out");
            None
        }
    }
}

fn build_cloud_client(
    paths: &AppPaths,
    trace_id: Option<&str>,
) -> Result<cue_cloud_client::CloudClient> {
    let account = load_account(paths).ok().flatten();
    let base_url = env::var("BLUEY_CLOUD_API_URL")
        .or_else(|_| env::var("CUE_CLOUD_API_URL"))
        .ok()
        .or_else(|| account.as_ref().map(|account| account.api_url.clone()))
        .unwrap_or_else(|| "https://bluey.sh".to_string());

    let config = cue_cloud_client::client::ClientConfig {
        base_url,
        ..Default::default()
    };

    if let Some(access) = cloud_access_token_from_env() {
        let store = cue_cloud_client::tokens::MemoryStore::new();
        cue_cloud_client::TokenStore::save(
            &store,
            &cue_cloud_client::Tokens {
                access,
                refresh: env::var("BLUEY_CLOUD_REFRESH_TOKEN")
                    .or_else(|_| env::var("CUE_CLOUD_REFRESH_TOKEN"))
                    .unwrap_or_default(),
                email: env::var("BLUEY_USER_ID")
                    .or_else(|_| env::var("CUE_USER_ID"))
                    .unwrap_or_else(|_| "env-token".to_string()),
            },
        )?;
        let client = cue_cloud_client::CloudClient::new(config, Arc::new(store))?;
        return Ok(cloud_client_with_optional_trace(client, trace_id));
    }

    let store = cue_cloud_client::SecureAccountStore::new(paths.clone());
    let client = cue_cloud_client::CloudClient::new(config.clone(), Arc::new(store))?;
    if client.current_tokens().is_some() {
        return Ok(cloud_client_with_optional_trace(client, trace_id));
    }

    if env_truthy_any(&["BLUEY_LEGACY_KEYRING_FALLBACK"]) {
        let client = cue_cloud_client::CloudClient::new(
            config,
            Arc::new(cue_cloud_client::tokens::KeyringStore::new()),
        )?;
        return Ok(cloud_client_with_optional_trace(client, trace_id));
    }

    Err(anyhow::anyhow!(
        "Bluey cloud account is not linked; run `bluey login`"
    ))
}

fn cloud_client_with_optional_trace(
    client: cue_cloud_client::CloudClient,
    trace_id: Option<&str>,
) -> cue_cloud_client::CloudClient {
    trace_id
        .and_then(sanitize_observability_id)
        .map(|trace_id| client.clone().with_trace_id(trace_id))
        .unwrap_or(client)
}

fn format_balance_cents(cents: i64) -> String {
    let sign = if cents < 0 { "-" } else { "" };
    let abs = cents.saturating_abs();
    format!("{sign}${}.{:02}", abs / 100, abs % 100)
}

async fn capture_transcribe_audio_chunk(
    daemon: &Arc<Daemon>,
    session_id: &str,
    runtime: &RealAudioRuntimeConfig,
    source: &RealAudioSource,
    sequence: u64,
    client: &reqwest::Client,
) -> Result<Option<cue_core::audio::SttSegmentMetadata>> {
    let audio_dir = daemon.paths.runtime_dir.join("audio");
    tokio::fs::create_dir_all(&audio_dir)
        .await
        .with_context(|| format!("failed to create {}", audio_dir.display()))?;
    let chunk_path = audio_dir.join(format!(
        "{}-{}-{sequence}.wav",
        source.source.default_label(),
        clock::now_epoch_ms_string()
    ));

    capture_audio_chunk_to_file(runtime, source, &chunk_path).await?;
    let byte_len = tokio::fs::metadata(&chunk_path)
        .await
        .map(|metadata| metadata.len())
        .unwrap_or(0);
    let start_ms = sequence
        .saturating_sub(1)
        .saturating_mul(runtime.chunk_duration_ms as u64);
    let chunk = AudioChunkMetadata::new(
        source.source,
        source.stream_id.clone(),
        sequence,
        start_ms,
        runtime.chunk_duration_ms,
        cue_core::AudioStreamFormat::stt_mono(),
        byte_len,
    );
    {
        let mut audio = daemon.audio.lock().await;
        audio.record_chunk(&chunk);
    }

    if daemon.audio.lock().await.session_id.as_deref() != Some(session_id) {
        let _ = tokio::fs::remove_file(&chunk_path).await;
        return Ok(None);
    }

    let transcript_result =
        transcribe_audio_file(runtime, source.source, sequence, &chunk_path, client)
            .await
            .with_context(|| format!("failed to transcribe {}", source.source));
    let _ = tokio::fs::remove_file(&chunk_path).await;
    Ok(transcript_result?)
}

async fn capture_audio_chunk_to_file(
    runtime: &RealAudioRuntimeConfig,
    source: &RealAudioSource,
    chunk_path: &Path,
) -> Result<()> {
    if let FfmpegAudioInput::NativeHelper {
        helper_path,
        source_arg,
    } = &source.ffmpeg_input
    {
        return capture_native_audio_chunk_to_file(
            helper_path,
            source_arg,
            runtime.chunk_duration_ms,
            source.source,
            chunk_path,
        )
        .await;
    }

    let ffmpeg_path = runtime
        .ffmpeg_path
        .as_ref()
        .context("ffmpeg path is not configured for this audio source")?;
    let mut command = TokioCommand::new(ffmpeg_path);
    command
        .arg("-hide_banner")
        .arg("-nostdin")
        .arg("-loglevel")
        .arg("error")
        .arg("-y");
    append_ffmpeg_input_args(&mut command, &source.ffmpeg_input);
    command
        .arg("-t")
        .arg(format!("{:.3}", runtime.chunk_duration_ms as f32 / 1_000.0))
        .arg("-vn")
        .arg("-ac")
        .arg("1")
        .arg("-ar")
        .arg("16000")
        .arg("-acodec")
        .arg("pcm_s16le")
        .arg(chunk_path);
    command.kill_on_drop(true);

    let timeout_ms = runtime.chunk_duration_ms as u64 + 7_000;
    let output = tokio::time::timeout(Duration::from_millis(timeout_ms), command.output())
        .await
        .with_context(|| format!("ffmpeg timed out capturing {}", source.source))?
        .with_context(|| format!("failed to run ffmpeg for {}", source.source))?;
    if !output.status.success() {
        return Err(anyhow!(
            "ffmpeg capture failed for {}: {}",
            source.source,
            compact_snippet(&String::from_utf8_lossy(&output.stderr), 320)
        ));
    }

    Ok(())
}

async fn capture_native_audio_chunk_to_file(
    helper_path: &Path,
    source_arg: &str,
    duration_ms: u32,
    source: AudioSourceKind,
    chunk_path: &Path,
) -> Result<()> {
    let mut command = TokioCommand::new(helper_path);
    command
        .arg("--source")
        .arg(source_arg)
        .arg("--duration-ms")
        .arg(duration_ms.to_string())
        .kill_on_drop(true);

    let timeout_ms = duration_ms as u64 + 8_000;
    let output = tokio::time::timeout(Duration::from_millis(timeout_ms), command.output())
        .await
        .with_context(|| format!("native audio helper timed out capturing {source}"))?
        .with_context(|| format!("failed to run native audio helper for {source}"))?;
    if !output.status.success() {
        return Err(anyhow!(
            "native audio helper failed for {source}: {}",
            compact_snippet(&String::from_utf8_lossy(&output.stderr), 320)
        ));
    }
    if output.stdout.len() < 1_024 {
        return Err(anyhow!(
            "native audio helper captured no usable {source} audio"
        ));
    }

    let wav = wav_from_i16le_16k_mono(&output.stdout);
    tokio::fs::write(chunk_path, wav)
        .await
        .with_context(|| format!("failed to write {}", chunk_path.display()))?;
    Ok(())
}

fn wav_from_i16le_16k_mono(raw: &[u8]) -> Vec<u8> {
    let pcm_len = raw.len() - (raw.len() % 2);
    let pcm = &raw[..pcm_len];

    let data_len = pcm.len() as u32;
    let mut wav = Vec::with_capacity(44 + pcm.len());
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36_u32.saturating_add(data_len)).to_le_bytes());
    wav.extend_from_slice(b"WAVE");
    wav.extend_from_slice(b"fmt ");
    wav.extend_from_slice(&16_u32.to_le_bytes());
    wav.extend_from_slice(&1_u16.to_le_bytes());
    wav.extend_from_slice(&1_u16.to_le_bytes());
    wav.extend_from_slice(&16_000_u32.to_le_bytes());
    wav.extend_from_slice(&32_000_u32.to_le_bytes());
    wav.extend_from_slice(&2_u16.to_le_bytes());
    wav.extend_from_slice(&16_u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_len.to_le_bytes());
    wav.extend_from_slice(pcm);
    wav
}

fn append_ffmpeg_input_args(command: &mut TokioCommand, input: &FfmpegAudioInput) {
    match input {
        FfmpegAudioInput::NativeHelper { .. } => {}
        #[cfg(target_os = "macos")]
        FfmpegAudioInput::MacAvFoundation { device_name } => {
            command
                .arg("-f")
                .arg("avfoundation")
                .arg("-i")
                .arg(format!(":{device_name}"));
        }
        #[cfg(target_os = "windows")]
        FfmpegAudioInput::WindowsDshow { device_name } => {
            command
                .arg("-f")
                .arg("dshow")
                .arg("-i")
                .arg(format!("audio={device_name}"));
        }
        #[cfg(target_os = "windows")]
        FfmpegAudioInput::WindowsWasapiLoopback { device_name } => {
            command
                .arg("-f")
                .arg("wasapi")
                .arg("-loopback")
                .arg("1")
                .arg("-i")
                .arg(device_name);
        }
    }
}

async fn transcribe_audio_file(
    runtime: &RealAudioRuntimeConfig,
    source: AudioSourceKind,
    sequence: u64,
    chunk_path: &Path,
    client: &reqwest::Client,
) -> Result<Option<cue_core::audio::SttSegmentMetadata>> {
    let audio = tokio::fs::read(chunk_path)
        .await
        .with_context(|| format!("failed to read {}", chunk_path.display()))?;
    if audio.len() < 1_024 {
        return Ok(None);
    }

    let response = match runtime.stt_transport {
        RealSttTransport::OpenAiMultipart => {
            let file_name = chunk_path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("bluey-audio.wav")
                .to_string();
            let file_part = reqwest::multipart::Part::bytes(audio)
                .file_name(file_name)
                .mime_str("audio/wav")
                .context("failed to build audio multipart body")?;
            let form = reqwest::multipart::Form::new()
                .text("model", runtime.stt_model.clone())
                .text("response_format", "json")
                .part("file", file_part);
            client
                .post(&runtime.stt_endpoint)
                .bearer_auth(&runtime.stt_api_key)
                .multipart(form)
                .send()
                .await
        }
        RealSttTransport::BlueyManagedRaw => {
            let sep = if runtime.stt_endpoint.contains('?') {
                '&'
            } else {
                '?'
            };
            let request_id = format!(
                "audio-{}-{sequence}-{}",
                source.default_label(),
                clock::now_epoch_ms_string()
            );
            let url = format!(
                "{}{sep}request_id={}&model={}",
                runtime.stt_endpoint,
                url_component(&request_id),
                url_component(&runtime.stt_model)
            );
            client
                .post(url)
                .bearer_auth(&runtime.stt_api_key)
                .header(reqwest::header::CONTENT_TYPE, "audio/wav")
                .body(audio)
                .send()
                .await
        }
        RealSttTransport::BlueyManagedRelay => {
            return Err(anyhow!(
                "live STT relay cannot transcribe a saved audio chunk"
            ));
        }
    }
    .with_context(|| format!("failed to call STT endpoint {}", runtime.stt_endpoint))?;
    let status = response.status();
    let body = response
        .text()
        .await
        .context("failed to read STT response")?;
    if !status.is_success() {
        return Err(anyhow!(
            "STT endpoint returned HTTP {status}: {}",
            compact_snippet(&body, 320)
        ));
    }

    let parsed: TranscriptionResponse =
        serde_json::from_str(&body).context("STT response was not transcription JSON")?;
    let Some(text) = parsed.text.map(|text| text.trim().to_string()) else {
        return Ok(None);
    };
    if text.is_empty() {
        return Ok(None);
    }

    let label = source.default_label();
    let mut segment = cue_core::audio::SttSegmentMetadata::new(
        text,
        sequence
            .saturating_sub(1)
            .saturating_mul(runtime.chunk_duration_ms as u64),
        runtime.chunk_duration_ms,
        true,
    )
    .with_provider_segment_id(format!("{label}-{sequence}"))
    .with_source(source)
    .with_speaker_label(label)
    .with_source_sequence_range(sequence, sequence);
    if let Some(language) = parsed
        .language
        .filter(|language| !language.trim().is_empty())
    {
        segment = segment.with_language(language);
    }
    Ok(Some(segment))
}

async fn stop_audio_capture(daemon: &Arc<Daemon>) -> AudioPipelineStatus {
    if let Some(stop) = daemon.audio_runtime.lock().await.stop.take() {
        let _ = stop.send(());
    }
    daemon.audio_runtime.lock().await.session_id = None;

    let mut audio = daemon.audio.lock().await;
    let status = audio.clone().stopped();
    *audio = status.clone();
    status
}

async fn add_audio_transcript_segment(
    daemon: &Arc<Daemon>,
    segment: &cue_core::audio::SttSegmentMetadata,
) -> Result<()> {
    add_audio_transcript_segment_inner(daemon, segment, false).await
}

async fn add_audio_transcript_segment_allowing_session_start(
    daemon: &Arc<Daemon>,
    segment: &cue_core::audio::SttSegmentMetadata,
) -> Result<()> {
    add_audio_transcript_segment_inner(daemon, segment, true).await
}

async fn add_audio_transcript_segment_inner(
    daemon: &Arc<Daemon>,
    segment: &cue_core::audio::SttSegmentMetadata,
    allow_session_start: bool,
) -> Result<()> {
    let audio_session_id = daemon.audio.lock().await.session_id.clone();
    if audio_session_id.is_none() && !allow_session_start {
        debug!("dropping late audio transcript segment after capture stopped");
        return Ok(());
    }

    let speaker = match segment.source {
        Some(AudioSourceKind::System) => Speaker::System,
        Some(AudioSourceKind::Microphone) => Speaker::User,
        None => Speaker::Unknown,
    };
    let text = segment.text.trim();
    if text.is_empty() {
        return Ok(());
    }
    let source_label = match segment.source {
        Some(AudioSourceKind::System) => "system",
        Some(AudioSourceKind::Microphone) => "microphone",
        None => "unknown",
    };

    if !segment.is_final {
        let _ = send_overlay(
            daemon,
            OverlayCommand::TranscriptPartial {
                source: source_label.to_string(),
                text: text.to_string(),
            },
        )
        .await;
        let _ = daemon.live_transcript_tx.send(LiveTranscriptEvent {
            session_id: audio_session_id.unwrap_or_default(),
            source: source_label.to_string(),
            text: text.to_string(),
            is_final: false,
            speaker: None,
            ts_ms: clock::now_epoch_ms_string().parse::<u64>().unwrap_or(0),
        });
        return Ok(());
    }

    let meeting_snapshot = {
        let mut meeting_guard = daemon.meeting.lock().await;
        if meeting_guard.is_none() {
            *meeting_guard = Some(MeetingRecord::new(Some("Ad hoc audio meeting".to_string())));
        }

        let meeting = meeting_guard.as_mut().expect("meeting exists");
        if is_near_duplicate_transcript(meeting, speaker, text, segment.is_final) {
            return Ok(());
        }
        // Dedup: if this is a final, remove superseded partial from same speaker
        if segment.is_final {
            dedup_partial_on_final(meeting, speaker, text);
        }
        let transcript_segment = TranscriptSegment::new(speaker, text, segment.is_final);
        meeting.transcript.push(transcript_segment.clone());
        let analysis = analyze_segment(&transcript_segment, meeting);
        meeting.action_items.extend(analysis.action_items);
        meeting.decisions.extend(analysis.decisions);
        daemon.store.save_active(meeting)?;
        meeting.clone()
    };

    update_state_from_meeting(daemon, Some(&meeting_snapshot)).await?;
    let title = match speaker {
        Speaker::System => "System",
        Speaker::User => "Mic",
        Speaker::Other => "Other",
        Speaker::Unknown => "Transcript",
    };
    let source = segment
        .speaker_label
        .as_deref()
        .filter(|label| !label.trim().is_empty())
        .map(|label| format!("{label} STT"))
        .unwrap_or_else(|| "audio STT".to_string());
    let _ = send_overlay(
        daemon,
        OverlayCommand::TranscriptFinal {
            source: source_label.to_string(),
            text: text.to_string(),
        },
    )
    .await;
    let card = CueCard::new(CardKind::Transcript, title, text).with_source(source);
    let _ = send_overlay(daemon, OverlayCommand::PushCard { card }).await;

    // Broadcast live transcript event for dashboard consumption.
    let ts_ms = meeting_snapshot
        .transcript
        .last()
        .and_then(|s| s.created_at.parse::<u64>().ok())
        .unwrap_or(0);
    let _ = daemon.live_transcript_tx.send(LiveTranscriptEvent {
        session_id: meeting_snapshot.id.to_string(),
        source: source_label.to_string(),
        text: text.to_string(),
        is_final: segment.is_final,
        speaker: None,
        ts_ms,
    });

    if segment.is_final {
        index_transcript_for_rag(daemon, meeting_snapshot.id.to_string(), text.to_string());
    }
    Ok(())
}

fn index_transcript_for_rag(daemon: &Arc<Daemon>, session_id: String, text: String) {
    daemon
        .rag_indexer
        .index_transcript(daemon.store.clone(), session_id, text);
}

fn index_context_artifacts_for_rag(
    daemon: &Arc<Daemon>,
    session_id: String,
    artifacts: Vec<ContextArtifact>,
) {
    daemon
        .rag_indexer
        .index_context_artifacts(daemon.store.clone(), session_id, artifacts);
}

fn reindex_meeting_for_rag(daemon: &Arc<Daemon>, meeting: MeetingRecord) {
    daemon
        .rag_indexer
        .reindex_meeting(daemon.store.clone(), meeting);
}

async fn handle_attach_requested(daemon: &Arc<Daemon>) -> Result<()> {
    let paths = choose_context_files().await?;
    handle_attach_paths(daemon, paths).await
}

async fn handle_attach_paths(daemon: &Arc<Daemon>, paths: Vec<PathBuf>) -> Result<()> {
    if paths.is_empty() {
        return Ok(());
    }

    let mut attached = Vec::new();
    for path in paths {
        if !is_supported_context_file(&path) {
            push_system_card(
                daemon,
                CardKind::Warning,
                "File skipped",
                format!(
                    "{} is not a readable Bluey context file. Attach text, Markdown, code, PDF, DOC/DOCX, CSV/TSV, JSON/YAML/TOML, HTML/CSS, shell/SQL, or RTF.",
                    path.file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or("Selected file")
                ),
            )
            .await;
            continue;
        }

        match build_context_artifact(
            &daemon.paths,
            path.display().to_string(),
            path.file_name()
                .and_then(|name| name.to_str())
                .map(str::to_string),
            Some("Added from overlay paperclip".to_string()),
        ) {
            Ok(artifact) => attached.push(artifact),
            Err(error) => {
                push_system_card(
                    daemon,
                    CardKind::Warning,
                    "Could not attach file",
                    format!("{error:#}"),
                )
                .await;
            }
        }
    }

    if attached.is_empty() {
        return Ok(());
    }

    let meeting_snapshot = attach_context_artifacts(daemon, attached.clone()).await?;
    update_state_from_meeting(daemon, Some(&meeting_snapshot)).await?;
    refresh_overlay_context_items(daemon, &meeting_snapshot).await;
    refresh_overlay_sessions(daemon).await;
    push_system_card(
        daemon,
        CardKind::Context,
        "Context attached",
        format!("{} file(s) added to this session.", attached.len()),
    )
    .await;
    Ok(())
}

async fn handle_remove_context_requested(daemon: &Arc<Daemon>, id: uuid::Uuid) -> Result<()> {
    let Some((meeting_snapshot, removed_title)) = ({
        let mut meeting_guard = daemon.meeting.lock().await;
        let Some(meeting) = meeting_guard.as_mut() else {
            return Ok(());
        };

        let Some(position) = meeting
            .context
            .iter()
            .position(|artifact| artifact.id == id)
        else {
            return Ok(());
        };
        let removed = meeting.context.remove(position);
        daemon.store.save_active(meeting)?;
        Some((meeting.clone(), removed.title))
    }) else {
        return Ok(());
    };

    update_state_from_meeting(daemon, Some(&meeting_snapshot)).await?;
    refresh_overlay_context_items(daemon, &meeting_snapshot).await;
    refresh_overlay_sessions(daemon).await;
    reindex_meeting_for_rag(daemon, meeting_snapshot);
    push_system_card(
        daemon,
        CardKind::Context,
        "Context removed",
        format!("{removed_title} removed from this session."),
    )
    .await;
    Ok(())
}

async fn handle_instructions_requested(daemon: &Arc<Daemon>) -> Result<()> {
    let current = daemon
        .meeting
        .lock()
        .await
        .as_ref()
        .and_then(|meeting| meeting.answer_instructions.clone())
        .unwrap_or_default();
    let Some(text) = prompt_answer_instructions(current).await? else {
        return Ok(());
    };

    let instructions = if text.trim().is_empty() {
        None
    } else {
        Some(text.trim().to_string())
    };
    let meeting_snapshot = set_answer_instructions(daemon, instructions).await?;
    update_state_from_meeting(daemon, Some(&meeting_snapshot)).await?;
    push_system_card(
        daemon,
        CardKind::System,
        "Answer style saved",
        meeting_snapshot
            .answer_instructions
            .clone()
            .unwrap_or_else(|| "Bluey will use the default answer style.".to_string()),
    )
    .await;
    Ok(())
}

async fn answer_question(
    daemon: &Arc<Daemon>,
    question: String,
    source: impl Into<String>,
) -> Result<AnswerResponse> {
    let question = question.trim().to_string();
    if question.is_empty() {
        return Err(anyhow!("question cannot be empty"));
    }

    let request = default_answer_request(&question);
    let (response, _events) = answer_with_provider_runtime(daemon, request, source).await?;
    Ok(response)
}

async fn answer_with_provider_runtime(
    daemon: &Arc<Daemon>,
    mut request: AnswerRequest,
    source: impl Into<String>,
) -> Result<(AnswerResponse, Vec<AnswerStreamEvent>)> {
    let source = source.into();
    request.question = request.question.trim().to_string();
    if request.question.is_empty() {
        return Err(anyhow!("question cannot be empty"));
    }
    let generation_id = next_answer_generation(daemon);

    let (meeting_snapshot, answer_meeting) = {
        let mut meeting_guard = daemon.meeting.lock().await;
        if meeting_guard.is_none() {
            let meeting = MeetingRecord::new(Some("Ad hoc meeting".to_string()));
            daemon.store.save_active(&meeting)?;
            *meeting_guard = Some(meeting);
        }

        let meeting = meeting_guard.as_ref().expect("meeting exists");
        if request.metadata.meeting_id.is_none() {
            request.metadata.meeting_id = Some(meeting.id);
        }
        request.instructions = merge_answer_instructions(
            request.instructions.take(),
            meeting.answer_instructions.clone(),
        );
        let mut answer_meeting = meeting.clone();
        if let Some(instructions) = request
            .instructions
            .as_ref()
            .filter(|value| !value.trim().is_empty())
        {
            answer_meeting.answer_instructions = Some(instructions.clone());
        }

        (meeting.clone(), answer_meeting)
    };

    if request.context.is_empty() {
        request.context =
            answer_context_for_question(daemon, &meeting_snapshot, &request.question).await;
    }

    let (visible_question_title, visible_question) =
        visible_question_for_source(&request.question, &source);
    let question_card = CueCard::new(
        CardKind::Question,
        visible_question_title.clone(),
        visible_question.clone(),
    )
    .with_source(source.clone());
    let _ = send_overlay(
        daemon,
        OverlayCommand::PushCard {
            card: question_card,
        },
    )
    .await;
    write_state(daemon).await?;

    let answer_card = CueCard::new(CardKind::Answer, "Bluey", "Thinking...")
        .with_source(format!("{} ({})", source, request.metadata.request_id));
    let answer_card_id = answer_card.id;
    let _ = send_overlay(daemon, OverlayCommand::PushCard { card: answer_card }).await;
    register_active_answer_card(daemon, generation_id, answer_card_id).await;
    let mut overlay_stream =
        OverlayAnswerStream::new(Arc::clone(daemon), answer_card_id, generation_id);

    let outcome = match resolve_answer_route(
        &daemon.paths,
        &request,
        &answer_meeting,
        Some(&mut overlay_stream),
    )
    .await
    {
        Ok(outcome) => outcome,
        Err(error) => {
            if is_answer_generation_current(daemon, generation_id) {
                let _ = overlay_stream
                    .finish(&user_facing_answer_error(&error))
                    .await;
            }
            clear_active_answer_card(daemon, generation_id, answer_card_id).await;
            return Err(error);
        }
    };
    let safety = outcome.safety.clone();
    let metadata =
        AnswerResponseMetadata::new(request.metadata.request_id, outcome.provider.clone())
            .with_requested_route(request.route.clone())
            .with_latency(outcome.latency_ms)
            .with_usage(
                outcome
                    .token_usage
                    .unwrap_or_else(|| estimate_token_usage(&request, &outcome.answer)),
            )
            .with_cost(CostEstimate::usd(0.0))
            .with_safety(safety.clone())
            .finished(AnswerFinishReason::Stop);
    let metadata = outcome
        .attempts
        .iter()
        .cloned()
        .fold(metadata, |metadata, attempt| metadata.with_attempt(attempt));
    let response = AnswerResponse::new(
        request.metadata.request_id,
        outcome.provider.clone(),
        outcome.answer.clone(),
    )
    .with_metadata(metadata);

    let mut events = vec![
        AnswerStreamEvent::started(request.metadata.request_id, outcome.provider.clone()),
        AnswerStreamEvent::SafetyNotice {
            request_id: request.metadata.request_id,
            safety,
        },
    ];
    for attempt in outcome
        .attempts
        .iter()
        .filter(|attempt| attempt.fallback_depth > 0)
    {
        events.push(AnswerStreamEvent::ProviderSwitch {
            request_id: request.metadata.request_id,
            attempt: attempt.clone(),
        });
    }
    if request.metadata.stream {
        events.push(AnswerStreamEvent::delta(
            request.metadata.request_id,
            outcome.answer.clone(),
        ));
    }
    events.push(AnswerStreamEvent::Usage {
        request_id: request.metadata.request_id,
        usage: response
            .metadata
            .token_usage
            .unwrap_or_else(|| TokenUsage::new(0, 0)),
        cost_estimate: response.metadata.cost_estimate.clone(),
    });
    events.push(AnswerStreamEvent::completed(response.clone()));

    if !is_answer_generation_current(daemon, generation_id) {
        clear_active_answer_card(daemon, generation_id, answer_card_id).await;
        return Ok((response, events));
    }

    if !overlay_stream.has_text() {
        overlay_stream.replay_text(&response.answer).await?;
    }
    overlay_stream
        .finish_with_cost_label(
            &response.answer,
            answer_overlay_cost_label(&response.metadata),
        )
        .await?;
    let still_current = is_answer_generation_current(daemon, generation_id);
    clear_active_answer_card(daemon, generation_id, answer_card_id).await;
    if !still_current {
        return Ok((response, events));
    }

    let meeting_snapshot = {
        let mut meeting_guard = daemon.meeting.lock().await;
        if let Some(meeting) = meeting_guard.as_mut() {
            meeting.push_conversation_turn(ConversationTurn::new(
                visible_question.clone(),
                response.answer.clone(),
                Some(source.clone()),
                Some(outcome.provider.display_label()),
            ));
            daemon.store.save_active(meeting)?;
            meeting.clone()
        } else {
            meeting_snapshot
        }
    };

    update_state_from_meeting(daemon, Some(&meeting_snapshot)).await?;
    write_state(daemon).await?;
    Ok((response, events))
}

fn user_facing_answer_error(error: &anyhow::Error) -> String {
    let raw = format!("{error:#}");
    let lower = raw.to_ascii_lowercase();
    if lower.contains("insufficient_quota")
        || lower.contains("quota")
        || lower.contains("credit balance")
        || lower.contains("payment")
        || lower.contains("billing")
    {
        return "Bluey is connected, but the managed AI provider needs billing/quota attention before it can answer. Check provider credits or try again after the account is funded.".to_string();
    }
    if lower.contains("unauthorized")
        || lower.contains("forbidden")
        || lower.contains("auth")
        || lower.contains("api key")
    {
        return "Bluey needs provider authentication before it can answer. Check the server-side API key setup, then try again.".to_string();
    }
    if lower.contains("capacity busy")
        || lower.contains("provider_key_cooling_down")
        || lower.contains("provider_capacity")
        || lower.contains("upstream_spend_guard")
        || lower.contains("handling a burst")
    {
        let hint = retry_after_hint(&raw).unwrap_or_default();
        return format!(
            "Capacity busy. Bluey is waiting for provider capacity to recover before trying again.{hint}"
        );
    }
    if lower.contains("rate limit") || lower.contains("429") || lower.contains("too many requests")
    {
        return "Bluey hit provider capacity for this lane. Try again shortly; the router will use the next healthy lane when available.".to_string();
    }
    "Bluey could not complete that answer yet. Try again, or check the server logs for the detailed provider error.".to_string()
}

fn retry_after_hint(raw: &str) -> Option<String> {
    let lower = raw.to_ascii_lowercase();
    let after = lower.split("retry after ").nth(1)?;
    let digits: String = after
        .chars()
        .skip_while(|ch| !ch.is_ascii_digit())
        .take_while(|ch| ch.is_ascii_digit())
        .collect();
    if digits.is_empty() {
        None
    } else {
        Some(format!(" Retry in about {digits}s."))
    }
}

fn answer_overlay_cost_label(metadata: &AnswerResponseMetadata) -> Option<String> {
    let mut parts = Vec::new();
    if let Some(cost) = metadata
        .cost_estimate
        .as_ref()
        .filter(|cost| cost.amount > 0.0)
    {
        if cost.currency.eq_ignore_ascii_case("usd") {
            let prefix = if cost.estimated { "~" } else { "" };
            parts.push(format!("{prefix}${:.4}", cost.amount));
        } else {
            let prefix = if cost.estimated { "~" } else { "" };
            parts.push(format!("{prefix}{:.4} {}", cost.amount, cost.currency));
        }
    }
    if let Some(usage) = metadata.token_usage.as_ref() {
        parts.push(format!(
            "{} in / {} out",
            usage.input_tokens, usage.output_tokens
        ));
    }
    if let Some(latency_ms) = metadata.latency_ms {
        parts.push(format!("{latency_ms} ms"));
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" · "))
    }
}

fn answer_overlay_artifact(answer: &str) -> Option<CueCardArtifact> {
    let body = answer.trim();
    if body.is_empty() {
        return None;
    }

    let lower = body.to_lowercase();
    let code_blocks = extract_fenced_code_blocks(body);
    if !code_blocks.is_empty() || looks_like_code_answer(&lower) {
        let artifact_body = format_code_artifact(body, &code_blocks);
        return Some(CueCardArtifact {
            artifact_type: CardArtifactType::Code,
            title: "Code canvas".to_string(),
            body: artifact_body,
            confidence: if code_blocks.is_empty() { 0.74 } else { 0.95 },
        });
    }

    if looks_like_system_design_answer(&lower) {
        return Some(CueCardArtifact {
            artifact_type: CardArtifactType::SystemDesign,
            title: "System design canvas".to_string(),
            body: format_structured_artifact(body, "System Design"),
            confidence: 0.88,
        });
    }

    if looks_like_screen_answer(&lower) {
        return Some(CueCardArtifact {
            artifact_type: CardArtifactType::Screen,
            title: "Screen analysis".to_string(),
            body: format_structured_artifact(body, "Screen Context"),
            confidence: 0.86,
        });
    }

    if looks_like_document_answer(&lower) {
        return Some(CueCardArtifact {
            artifact_type: CardArtifactType::Document,
            title: "Document notes".to_string(),
            body: format_structured_artifact(body, "Document Context"),
            confidence: 0.78,
        });
    }

    if body.chars().count() > 950 && has_structured_answer_shape(body) {
        return Some(CueCardArtifact {
            artifact_type: CardArtifactType::Structured,
            title: "Workspace".to_string(),
            body: format_structured_artifact(body, "Details"),
            confidence: 0.70,
        });
    }

    None
}

fn extract_fenced_code_blocks(text: &str) -> Vec<String> {
    let mut blocks = Vec::new();
    let mut current = Vec::new();
    let mut in_fence = false;

    for line in text.lines() {
        if line.trim_start().starts_with("```") {
            if in_fence {
                let block = current.join("\n").trim().to_string();
                if !block.is_empty() {
                    blocks.push(block);
                }
                current.clear();
            }
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            current.push(line);
        }
    }

    blocks
}

fn strip_fenced_code(text: &str) -> String {
    let mut lines = Vec::new();
    let mut in_fence = false;
    for line in text.lines() {
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        if !in_fence {
            lines.push(line);
        }
    }
    lines.join("\n")
}

fn format_code_artifact(body: &str, code_blocks: &[String]) -> String {
    let notes = strip_fenced_code(body).trim().to_string();
    let mut sections = Vec::new();
    if !code_blocks.is_empty() {
        sections.push(format!(
            "CODE\n----\n{}",
            code_blocks.join("\n\n// ---\n\n")
        ));
    }
    if !notes.is_empty() {
        sections.push(format!("NOTES\n-----\n{notes}"));
    }
    if sections.is_empty() {
        body.to_string()
    } else {
        sections.join("\n\n")
    }
}

fn format_structured_artifact(body: &str, fallback_heading: &str) -> String {
    let clean = body.trim();
    if clean.starts_with('#')
        || clean
            .to_lowercase()
            .starts_with(&fallback_heading.to_lowercase())
    {
        clean.to_string()
    } else {
        format!(
            "{fallback_heading}\n{}\n{clean}",
            "-".repeat(fallback_heading.len())
        )
    }
}

fn looks_like_code_answer(lower: &str) -> bool {
    const SIGNALS: &[&str] = &[
        "class solution",
        "def ",
        "function ",
        "const ",
        "let ",
        "public ",
        "private ",
        "time complexity",
        "space complexity",
        "test case",
        "edge case",
        "sql",
    ];
    SIGNALS
        .iter()
        .filter(|signal| lower.contains(**signal))
        .count()
        >= 2
}

fn looks_like_system_design_answer(lower: &str) -> bool {
    const SIGNALS: &[&str] = &[
        "system design",
        "architecture",
        "api",
        "database",
        "cache",
        "queue",
        "scale",
        "latency",
        "throughput",
        "tradeoff",
        "shard",
        "load balancer",
        "microservice",
        "event-driven",
    ];
    SIGNALS
        .iter()
        .filter(|signal| lower.contains(**signal))
        .count()
        >= 3
}

fn looks_like_screen_answer(lower: &str) -> bool {
    lower.contains("screenshot")
        || lower.contains("screen context")
        || lower.contains("analyse screen")
        || lower.contains("analyze screen")
        || lower.contains("image shows")
}

fn looks_like_document_answer(lower: &str) -> bool {
    lower.contains("attached document")
        || lower.contains("pdf")
        || lower.contains("resume")
        || lower.contains("document context")
        || lower.contains("source:")
}

fn has_structured_answer_shape(text: &str) -> bool {
    text.lines()
        .filter(|line| {
            let trimmed = line.trim_start();
            trimmed.starts_with("- ")
                || trimmed.starts_with("* ")
                || trimmed.starts_with('#')
                || numbered_list_prefix(trimmed)
        })
        .count()
        >= 3
}

fn numbered_list_prefix(line: &str) -> bool {
    let mut chars = line.chars().peekable();
    let mut saw_digit = false;
    while matches!(chars.peek(), Some(ch) if ch.is_ascii_digit()) {
        saw_digit = true;
        chars.next();
    }
    saw_digit
        && matches!(chars.next(), Some('.' | ')'))
        && matches!(chars.next(), Some(ch) if ch.is_whitespace())
}

fn visible_question_for_source(question: &str, source: &str) -> (String, String) {
    match source {
        "overlay analyse" => (
            "Analyse Screen".to_string(),
            "Analyse the current browser page or screen context.".to_string(),
        ),
        "overlay screenshot analyse" => (
            "Analyse Screen".to_string(),
            "Analyse the captured screenshot context.".to_string(),
        ),
        _ => ("Question".to_string(), question.to_string()),
    }
}

struct AnswerRouteOutcome {
    provider: ProviderSelector,
    answer: String,
    attempts: Vec<RouteAttemptMetadata>,
    latency_ms: u64,
    token_usage: Option<TokenUsage>,
    safety: SafetyOutcome,
}

async fn resolve_answer_route(
    paths: &AppPaths,
    request: &AnswerRequest,
    meeting: &MeetingRecord,
    mut stream: Option<&mut OverlayAnswerStream>,
) -> Result<AnswerRouteOutcome> {
    let mut attempts = Vec::new();
    let mut failures = Vec::new();

    for (fallback_depth, step) in request.route.steps().enumerate() {
        if let Some(stream) = stream.as_mut() {
            stream
                .set_body(
                    format!("Thinking with {}...", step.provider.display_label()),
                    false,
                )
                .await?;
        }

        let config = provider_client_config(&step.provider);
        let required_capabilities = if step.required_capabilities.is_empty() {
            vec![cue_core::AiCapability::Chat]
        } else {
            step.required_capabilities.clone()
        };

        if !config
            .capabilities
            .supports_all(required_capabilities.iter())
        {
            let message = format!(
                "{} does not support required capabilities: {}",
                step.provider.display_label(),
                required_capabilities
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            attempts.push(
                RouteAttemptMetadata::started(step.provider.clone(), fallback_depth)
                    .failed(message.clone()),
            );
            failures.push(message);
            continue;
        }

        let budget = step.budget_override.unwrap_or(request.route.budgets);
        let payload = ProviderRequestPayload::from_request(
            request,
            step.provider.clone(),
            config.endpoint.clone(),
            default_model_for_provider(step.provider.provider_kind),
            budget,
        );

        if matches!(step.provider.provider_kind, AiProviderKind::Local) {
            let started_at = Instant::now();
            let answer = local_answer(&request.question, meeting);
            if let Some(stream) = stream.as_mut() {
                stream.replay_text(&answer).await?;
            }
            let latency_ms = started_at.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
            attempts.push(
                RouteAttemptMetadata::started(step.provider.clone(), fallback_depth)
                    .succeeded(latency_ms),
            );
            let safety = SafetyOutcome::pass()
                .with_notice("local deterministic answer; no external provider call was made");
            return Ok(AnswerRouteOutcome {
                provider: step.provider.clone(),
                answer,
                attempts,
                latency_ms,
                token_usage: None,
                safety,
            });
        }

        if matches!(step.provider.provider_kind, AiProviderKind::CueManaged) {
            let stream_ref = stream.as_mut().map(|stream| &mut **stream);
            match call_bluey_managed_provider(paths, request, &step.provider, &payload, stream_ref)
                .await
            {
                Ok(answer) => {
                    attempts.push(
                        RouteAttemptMetadata::started(answer.provider.clone(), fallback_depth)
                            .succeeded(answer.latency_ms),
                    );
                    let safety = SafetyOutcome::pass().with_notice(format!(
                        "managed Bluey route used: {}",
                        answer.provider.display_label()
                    ));
                    return Ok(AnswerRouteOutcome {
                        provider: answer.provider,
                        answer: answer.answer,
                        attempts,
                        latency_ms: answer.latency_ms,
                        token_usage: answer.token_usage,
                        safety,
                    });
                }
                Err(error) => {
                    let message = format!(
                        "{} request failed: {error:#}",
                        step.provider.display_label()
                    );
                    attempts.push(
                        RouteAttemptMetadata::started(step.provider.clone(), fallback_depth)
                            .failed(message.clone()),
                    );
                    failures.push(message);
                    continue;
                }
            }
        }

        if let Some(message) = config.unavailable_message() {
            attempts.push(
                RouteAttemptMetadata::started(step.provider.clone(), fallback_depth)
                    .failed(message.clone()),
            );
            failures.push(format!("{}: {message}", step.provider.display_label()));
            continue;
        }

        let stream_ref = stream.as_mut().map(|stream| &mut **stream);
        match call_chat_provider(&config, &payload, stream_ref).await {
            Ok(answer) => {
                attempts.push(
                    RouteAttemptMetadata::started(answer.provider.clone(), fallback_depth)
                        .succeeded(answer.latency_ms),
                );
                let safety = SafetyOutcome::pass().with_notice(format!(
                    "live provider route used: {}",
                    answer.provider.display_label()
                ));
                return Ok(AnswerRouteOutcome {
                    provider: answer.provider,
                    answer: answer.answer,
                    attempts,
                    latency_ms: answer.latency_ms,
                    token_usage: answer.token_usage,
                    safety,
                });
            }
            Err(error) => {
                let message = format!(
                    "{} request failed: {error:#}",
                    step.provider.display_label()
                );
                attempts.push(
                    RouteAttemptMetadata::started(step.provider.clone(), fallback_depth)
                        .failed(message.clone()),
                );
                failures.push(message);
            }
        }
    }

    Err(anyhow!(
        "no available answer provider for request {}. {}",
        request.metadata.request_id,
        failures.join("; ")
    ))
}

async fn call_bluey_managed_provider(
    paths: &AppPaths,
    request: &AnswerRequest,
    provider: &ProviderSelector,
    payload: &ProviderRequestPayload,
    mut stream: Option<&mut OverlayAnswerStream>,
) -> Result<LiveProviderAnswer> {
    let client = build_cloud_client(paths, request.metadata.correlation_id.as_deref())?;
    let lane = managed_lane_for_provider(provider, payload);
    let managed = BlueyManagedProvider::new(client, lane);
    let prompt = provider_prompt_parts(payload)?;
    let llm_request = LlmRequest {
        system: prompt.system,
        user: prompt.user,
        session_id: request
            .metadata
            .meeting_id
            .map(|meeting_id| meeting_id.to_string()),
        max_tokens: payload.max_output_tokens,
        temperature: None,
        reasoning_effort: managed_reasoning_effort(lane),
        thinking_budget_tokens: None,
        request_id: Some(payload.request_id.to_string()),
        image_data_urls: prompt.image_data_urls,
    };
    let started_at = Instant::now();

    if payload.stream {
        let mut chunks = managed
            .complete_stream(&llm_request)
            .await
            .map_err(managed_llm_error)?;
        let mut answer = String::new();
        let mut token_usage = None;
        let mut cost_label = None;
        let mut saw_finished = false;
        while let Some(chunk) = chunks.next().await {
            let chunk = chunk.map_err(managed_llm_error)?;
            if !chunk.text.is_empty() {
                answer.push_str(&chunk.text);
                if let Some(stream) = stream.as_mut() {
                    stream.push_delta(&chunk.text).await?;
                }
            }
            if let Some(cost) = chunk.cost.as_ref() {
                token_usage = Some(token_usage_from_llm_cost(cost));
            }
            if let Some(label) = chunk.cost_label {
                cost_label = Some(label);
            }
            if chunk.finished {
                saw_finished = true;
            }
        }

        let answer = answer.trim().to_string();
        if answer.is_empty() {
            return Err(anyhow!("managed provider stream returned no answer text"));
        }
        if !saw_finished {
            return Err(anyhow!(
                "managed provider stream ended before final billing metadata"
            ));
        }
        if let Some(stream) = stream.as_mut() {
            stream
                .finish_with_cost_label(&answer, cost_label.clone())
                .await?;
        }

        return Ok(LiveProviderAnswer {
            provider: provider.clone(),
            answer,
            token_usage,
            latency_ms: started_at.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
        });
    }

    let response = managed
        .complete(&llm_request)
        .await
        .map_err(managed_llm_error)?;
    let answer = response.text.trim().to_string();
    if answer.is_empty() {
        return Err(anyhow!("managed provider returned no answer text"));
    }
    if let Some(stream) = stream.as_mut() {
        stream.replay_text(&answer).await?;
        stream
            .finish_with_cost_label(&answer, response.cost_label.clone())
            .await?;
    }
    let token_usage = response.cost.as_ref().map(token_usage_from_llm_cost);
    Ok(LiveProviderAnswer {
        provider: provider.clone(),
        answer,
        token_usage,
        latency_ms: started_at.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
    })
}

fn managed_lane_for_provider(
    provider: &ProviderSelector,
    payload: &ProviderRequestPayload,
) -> ManagedLane {
    provider
        .model
        .as_ref()
        .map(|model| managed_lane_from_value(model.as_str()))
        .unwrap_or_else(|| managed_lane_from_value(&payload.model))
}

fn managed_reasoning_effort(lane: ManagedLane) -> Option<String> {
    match lane {
        ManagedLane::Deep => Some("high".to_string()),
        ManagedLane::Instant | ManagedLane::Balanced | ManagedLane::Vision => None,
    }
}

fn token_usage_from_llm_cost(cost: &cue_llm::LlmCostMetadata) -> TokenUsage {
    TokenUsage::new(
        cost.input_tokens.max(0).min(i64::from(u32::MAX)) as u32,
        cost.output_tokens.max(0).min(i64::from(u32::MAX)) as u32,
    )
}

fn managed_llm_error(error: cue_llm::LlmError) -> anyhow::Error {
    anyhow!("{error}")
}

async fn call_chat_provider(
    config: &ProviderClientConfig,
    payload: &ProviderRequestPayload,
    stream: Option<&mut OverlayAnswerStream>,
) -> Result<LiveProviderAnswer> {
    if !config.can_attempt_live_request() {
        return Err(anyhow!(
            "{}",
            config
                .unavailable_message()
                .unwrap_or_else(|| "provider is unavailable".to_string())
        ));
    }

    if !matches!(
        config.provider.provider_kind,
        AiProviderKind::OpenAi
            | AiProviderKind::Groq
            | AiProviderKind::Cerebras
            | AiProviderKind::CueManaged
    ) {
        return Err(anyhow!(
            "{} does not have an OpenAI-compatible HTTP adapter yet",
            config.provider.display_label()
        ));
    }

    let endpoint = config
        .endpoint
        .as_ref()
        .filter(|endpoint| !endpoint.trim().is_empty())
        .context("provider endpoint is not configured")?;
    let api_key = provider_api_key(config).context("provider API key is not configured")?;

    let messages = provider_messages(payload)?;
    let request_body = ChatCompletionRequest {
        model: payload.model.clone(),
        messages,
        stream: payload.stream,
        max_tokens: payload.max_output_tokens.unwrap_or(1_024),
    };
    let timeout_ms = payload.latency_timeout_ms.clamp(1_000, 120_000);
    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(timeout_ms))
        .build()
        .context("failed to build provider HTTP client")?;

    let started_at = Instant::now();
    let response = client
        .post(endpoint)
        .bearer_auth(api_key)
        .json(&request_body)
        .send()
        .await
        .with_context(|| format!("failed to call {endpoint}"))?;
    let status = response.status();
    if !status.is_success() {
        let body = response
            .text()
            .await
            .context("failed to read provider error response")?;
        return Err(anyhow!(
            "provider returned HTTP {status}: {}",
            compact_snippet(&body, 320)
        ));
    }

    if payload.stream {
        return read_streaming_chat_response(response, config, started_at, stream).await;
    }

    let body = response
        .text()
        .await
        .context("failed to read provider response")?;

    let parsed: ChatCompletionResponse =
        serde_json::from_str(&body).context("provider response was not chat-completions JSON")?;
    let answer = parsed
        .choices
        .into_iter()
        .find_map(|choice| choice.message.content)
        .map(|content| content.trim().to_string())
        .filter(|content| !content.is_empty())
        .context("provider returned no answer text")?;
    let token_usage = parsed.usage.map(|usage| {
        let input = usage.prompt_tokens.unwrap_or_default();
        let output = usage.completion_tokens.unwrap_or_default();
        TokenUsage {
            input_tokens: input,
            output_tokens: output,
            total_tokens: usage.total_tokens.unwrap_or(input.saturating_add(output)),
        }
    });

    Ok(LiveProviderAnswer {
        provider: config.provider.clone(),
        answer,
        token_usage,
        latency_ms: started_at.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
    })
}

async fn read_streaming_chat_response(
    mut response: reqwest::Response,
    config: &ProviderClientConfig,
    started_at: Instant,
    mut stream: Option<&mut OverlayAnswerStream>,
) -> Result<LiveProviderAnswer> {
    let mut pending = String::new();
    let mut answer = String::new();
    let mut token_usage = None;

    while let Some(chunk) = response
        .chunk()
        .await
        .context("failed to read provider stream chunk")?
    {
        pending.push_str(&String::from_utf8_lossy(&chunk));
        while let Some((frame_end, delimiter_len)) = next_sse_frame(&pending) {
            let frame = pending[..frame_end].to_string();
            pending = pending[frame_end + delimiter_len..].to_string();
            for line in frame.lines() {
                let line = line.trim();
                let Some(data) = line.strip_prefix("data:") else {
                    continue;
                };
                let data = data.trim();
                if data.is_empty() || data == "[DONE]" {
                    continue;
                }
                let parsed: ChatCompletionStreamResponse = serde_json::from_str(data)
                    .with_context(|| {
                        format!("provider stream event was not chat-completions JSON: {data}")
                    })?;
                if let Some(usage) = parsed.usage {
                    let input = usage.prompt_tokens.unwrap_or_default();
                    let output = usage.completion_tokens.unwrap_or_default();
                    token_usage = Some(TokenUsage {
                        input_tokens: input,
                        output_tokens: output,
                        total_tokens: usage.total_tokens.unwrap_or(input.saturating_add(output)),
                    });
                }
                for choice in parsed.choices {
                    if let Some(delta) = choice.delta.content.filter(|delta| !delta.is_empty()) {
                        answer.push_str(&delta);
                        if let Some(stream) = stream.as_mut() {
                            stream.push_delta(&delta).await?;
                        }
                    }
                }
            }
        }
    }

    if !pending.trim().is_empty() {
        for line in pending.lines() {
            let line = line.trim();
            let Some(data) = line.strip_prefix("data:") else {
                continue;
            };
            let data = data.trim();
            if data.is_empty() || data == "[DONE]" {
                continue;
            }
            let parsed: ChatCompletionStreamResponse =
                serde_json::from_str(data).context("trailing provider stream event was invalid")?;
            for choice in parsed.choices {
                if let Some(delta) = choice.delta.content.filter(|delta| !delta.is_empty()) {
                    answer.push_str(&delta);
                    if let Some(stream) = stream.as_mut() {
                        stream.push_delta(&delta).await?;
                    }
                }
            }
        }
    }

    let answer = answer.trim().to_string();
    if answer.is_empty() {
        return Err(anyhow!("provider stream returned no answer text"));
    }

    Ok(LiveProviderAnswer {
        provider: config.provider.clone(),
        answer,
        token_usage,
        latency_ms: started_at.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
    })
}

fn next_sse_frame(pending: &str) -> Option<(usize, usize)> {
    match (pending.find("\n\n"), pending.find("\r\n\r\n")) {
        (Some(lf), Some(crlf)) if crlf < lf => Some((crlf, 4)),
        (Some(lf), _) => Some((lf, 2)),
        (None, Some(crlf)) => Some((crlf, 4)),
        (None, None) => None,
    }
}

fn provider_api_key(config: &ProviderClientConfig) -> Option<String> {
    config
        .api_key_env
        .as_ref()
        .and_then(|name| env::var(name).ok())
        .or_else(|| {
            if matches!(config.provider.provider_kind, AiProviderKind::CueManaged) {
                env::var("BLUEY_CLOUD_TOKEN")
                    .ok()
                    .or_else(|| env::var("BLUEY_API_TOKEN").ok())
                    .or_else(|| env::var("CUE_CLOUD_TOKEN").ok())
                    .or_else(|| env::var("CUE_API_TOKEN").ok())
            } else {
                None
            }
        })
        .filter(|value| !value.trim().is_empty())
}

const HUMAN_SPEAK_CONTRACT: &str = "\
Human-speak contract:
- Start with a short talk track the user could say naturally, not a meta answer about what to say.
- Use first person for plans, tradeoffs, and explanations: \"I would...\", \"My approach is...\", \"The reason I prefer...\".
- Prefer a natural spoken flow: acknowledge the question, give the core answer, then add the reason or example.
- Do not invent personal experience, shipped work, metrics, or ownership that is not in the question or session context.
- No assistant preamble such as \"Sure\", \"Here is\", \"As an AI\", or \"You can say\".
- Do not sound like a polished memo: avoid source labels, repeated headings, and long markdown checklists in the chat answer.
- Include a concise rationale when it helps the user defend the answer, but do not expose hidden chain-of-thought.
- If the topic needs depth, keep the chat answer speakable and put deeper code/design/detail in the structured sections or artifact.";

fn provider_prompt_parts(payload: &ProviderRequestPayload) -> Result<ProviderPromptParts> {
    let mut system = String::from(
        "You are Bluey, a concise meeting and work copilot. Answer only from the supplied session context when possible. If context is thin, say what is missing and give the most useful next step.",
    );
    system.push_str("\n\n");
    system.push_str(HUMAN_SPEAK_CONTRACT);
    system.push_str(
        "\n\nOutput format:\n- Stream a clear, readable answer with short line breaks.\n- Put the direct, speakable answer first as one natural paragraph whenever possible.\n- Do not turn normal chat answers into a markdown outline. Use headings only when the task truly needs structure or when an artifact/canvas will render the deeper detail.\n- Auto-detect the task type. For coding, debugging, algorithms, API, or configuration questions, use this shape after the talk track when useful: Approach, Code, Explanation, Complexity, Edge cases. Put code in fenced Markdown code blocks with a language tag when possible.\n- For code follow-ups or requested changes, return the full updated implementation or full replacement snippet in the artifact/canvas body, not only a tiny line diff, unless the user explicitly asks for a patch.\n- For system design questions, use Architecture, Data flow, Components, Scaling, Tradeoffs, and Risks / next steps after the talk track when useful.\n- For system design follow-ups, return the updated whole architecture section in the artifact/canvas body so the canvas remains the current source of truth.\n- For design/debug/product questions, use compact bullets with concrete next steps.\n- Avoid long paragraphs; make the overlay easy to scan while it streams.",
    );
    if let Some(instructions) = payload
        .instructions
        .as_ref()
        .filter(|value| !value.trim().is_empty())
    {
        system.push_str("\n\nAnswer rules:\n");
        system.push_str(instructions);
    }

    let mut image_data_urls = Vec::new();
    let mut text_context = Vec::new();
    for item in &payload.context {
        let title = item.title.as_deref().unwrap_or("Context");
        let source = item.source.as_deref().unwrap_or("session");
        if item.kind == AnswerContextKind::Screenshot {
            if let Some(path) = item
                .source
                .as_deref()
                .map(Path::new)
                .filter(|path| path.is_file())
                .filter(|path| image_mime_for_path(path).is_some())
            {
                if payload.privacy.allow_image_upload {
                    image_data_urls.push(image_data_url_from_path(path)?);
                    push_provider_context_item(
                        &mut text_context,
                        item.kind,
                        title,
                        source,
                        &item.content,
                    );
                    continue;
                }
                push_provider_context_item(
                    &mut text_context,
                    item.kind,
                    title,
                    source,
                    &format!(
                        "{}\nomitted from provider upload because image upload is disabled for this route.",
                        item.content
                    ),
                );
                continue;
            }
        }

        push_provider_context_item(&mut text_context, item.kind, title, source, &item.content);
    }

    let context = compact_provider_context(&text_context);

    let user = if context.trim().is_empty() {
        payload.question.clone()
    } else {
        format!(
            "Question:\n{}\n\nSession context:\n{}",
            payload.question, context
        )
    };

    Ok(ProviderPromptParts {
        system,
        user,
        image_data_urls,
    })
}

fn provider_messages(payload: &ProviderRequestPayload) -> Result<Vec<ChatMessage>> {
    let prompt = provider_prompt_parts(payload)?;

    let user_content = if prompt.image_data_urls.is_empty() {
        ChatMessageContent::Text(prompt.user)
    } else {
        let mut parts = vec![ChatMessagePart::Text { text: prompt.user }];
        parts.extend(
            prompt
                .image_data_urls
                .into_iter()
                .map(|url| ChatMessagePart::ImageUrl {
                    image_url: ChatImageUrl { url },
                }),
        );
        ChatMessageContent::Parts(parts)
    };

    Ok(vec![
        ChatMessage {
            role: "system".to_string(),
            content: ChatMessageContent::Text(prompt.system),
        },
        ChatMessage {
            role: "user".to_string(),
            content: user_content,
        },
    ])
}

fn push_provider_context_item(
    items: &mut Vec<String>,
    kind: AnswerContextKind,
    title: &str,
    source: &str,
    content: &str,
) {
    let limit = provider_context_item_limit(kind);
    let content = compact_preserve_lines(content, limit);
    items.push(format!("[{} from {}]\n{}", title, source, content));
}

fn provider_context_item_limit(kind: AnswerContextKind) -> usize {
    match kind {
        AnswerContextKind::Transcript => 10_000,
        AnswerContextKind::MeetingMemory => 9_000,
        AnswerContextKind::Document => 6_000,
        AnswerContextKind::Screenshot => 3_000,
        AnswerContextKind::UserNote => 4_000,
        AnswerContextKind::System => 3_000,
        AnswerContextKind::Other => 2_500,
    }
}

fn compact_provider_context(items: &[String]) -> String {
    const TOTAL_LIMIT: usize = 32_000;
    let mut output = String::new();

    for item in items {
        let separator = if output.is_empty() { "" } else { "\n\n" };
        let projected = output
            .chars()
            .count()
            .saturating_add(separator.chars().count())
            .saturating_add(item.chars().count());
        if projected <= TOTAL_LIMIT {
            output.push_str(separator);
            output.push_str(item);
            continue;
        }

        let used = output
            .chars()
            .count()
            .saturating_add(separator.chars().count());
        let remaining = TOTAL_LIMIT.saturating_sub(used);
        if remaining > 120 {
            output.push_str(separator);
            output.push_str(&compact_preserve_lines(item, remaining));
        }
        output.push_str("\n\n[older context compacted to stay within the active model window]");
        break;
    }

    output
}

fn compact_preserve_lines(text: &str, max_chars: usize) -> String {
    let clean = text.trim();
    if clean.chars().count() <= max_chars {
        return clean.to_string();
    }

    let mut compacted = clean.chars().take(max_chars).collect::<String>();
    compacted.push_str("\n...[compacted]");
    compacted
}

fn image_data_url_from_path(path: &Path) -> Result<String> {
    let mime = image_mime_for_path(path).context("unsupported image type for vision request")?;
    let bytes =
        std::fs::read(path).with_context(|| format!("failed to read image {}", path.display()))?;
    let encoded = BASE64_STANDARD.encode(bytes);
    Ok(format!("data:{mime};base64,{encoded}"))
}

fn image_mime_for_path(path: &Path) -> Option<&'static str> {
    match path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "webp" => Some("image/webp"),
        "gif" => Some("image/gif"),
        "bmp" => Some("image/bmp"),
        _ => None,
    }
}

fn provider_client_config(provider: &ProviderSelector) -> ProviderClientConfig {
    match provider.provider_kind {
        AiProviderKind::CueManaged => {
            ProviderClientConfig::new(provider.clone(), AiCapabilities::all())
                .with_endpoint(
                    env::var("BLUEY_CLOUD_API_URL")
                        .or_else(|_| env::var("CUE_CLOUD_API_URL"))
                        .unwrap_or_else(|_| "http://127.0.0.1:8787".to_string()),
                )
                .with_api_key_env("BLUEY_CLOUD_API_TOKEN", cloud_token_configured())
                .with_live_requests_enabled(
                    env::var("BLUEY_CLOUD_ANSWER_COMPAT")
                        .or_else(|_| env::var("CUE_CLOUD_ANSWER_COMPAT"))
                        .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
                        .unwrap_or(false),
                )
        }
        AiProviderKind::OpenAi => {
            ProviderClientConfig::new(provider.clone(), AiCapabilities::all())
                .with_endpoint(
                    env::var("OPENAI_API_URL").unwrap_or_else(|_| {
                        "https://api.openai.com/v1/chat/completions".to_string()
                    }),
                )
                .with_api_key_env("OPENAI_API_KEY", env_configured("OPENAI_API_KEY"))
                .with_live_requests_enabled(true)
        }
        AiProviderKind::Anthropic => {
            ProviderClientConfig::new(provider.clone(), AiCapabilities::chat().with_vision())
                .with_endpoint(
                    env::var("ANTHROPIC_API_URL")
                        .unwrap_or_else(|_| "https://api.anthropic.com/v1/messages".to_string()),
                )
                .with_api_key_env("ANTHROPIC_API_KEY", env_configured("ANTHROPIC_API_KEY"))
        }
        AiProviderKind::Groq => {
            ProviderClientConfig::new(provider.clone(), groq_capabilities(provider))
                .with_endpoint(env::var("GROQ_API_URL").unwrap_or_else(|_| {
                    "https://api.groq.com/openai/v1/chat/completions".to_string()
                }))
                .with_api_key_env("GROQ_API_KEY", env_configured("GROQ_API_KEY"))
                .with_live_requests_enabled(true)
        }
        AiProviderKind::Cerebras => {
            ProviderClientConfig::new(provider.clone(), AiCapabilities::chat())
                .with_endpoint(
                    env::var("CEREBRAS_API_URL").unwrap_or_else(|_| {
                        "https://api.cerebras.ai/v1/chat/completions".to_string()
                    }),
                )
                .with_api_key_env("CEREBRAS_API_KEY", env_configured("CEREBRAS_API_KEY"))
                .with_live_requests_enabled(true)
        }
        AiProviderKind::Google => {
            ProviderClientConfig::new(provider.clone(), AiCapabilities::chat().with_vision())
                .with_endpoint(env::var("GOOGLE_AI_API_URL").unwrap_or_else(|_| {
                    "https://generativelanguage.googleapis.com/v1beta".to_string()
                }))
                .with_api_key_env("GOOGLE_API_KEY", env_configured("GOOGLE_API_KEY"))
        }
        AiProviderKind::Local => {
            ProviderClientConfig::new(provider.clone(), AiCapabilities::chat())
                .with_live_requests_enabled(true)
        }
        _ => ProviderClientConfig::new(provider.clone(), AiCapabilities::chat())
            .with_endpoint(env::var("CUE_PROVIDER_API_URL").unwrap_or_default())
            .with_api_key_env(
                "CUE_PROVIDER_API_KEY",
                env_configured("CUE_PROVIDER_API_KEY"),
            ),
    }
}

fn groq_capabilities(provider: &ProviderSelector) -> AiCapabilities {
    let caps = AiCapabilities::chat().with_stt();
    let model = provider
        .model
        .as_ref()
        .map(|model| model.as_str().to_ascii_lowercase())
        .unwrap_or_default();
    if model.contains("vision")
        || model.contains("llama-4")
        || model.contains("scout")
        || model.contains("maverick")
    {
        caps.with_vision()
    } else {
        caps
    }
}

fn default_model_for_provider(provider_kind: AiProviderKind) -> &'static str {
    match provider_kind {
        AiProviderKind::CueManaged => "bluey-router-v1",
        AiProviderKind::OpenAi => "gpt-4.1-mini",
        AiProviderKind::Anthropic => "claude-3-7-sonnet-latest",
        AiProviderKind::Groq => "llama-3.1-8b-instant",
        AiProviderKind::Cerebras => "llama3.1-8b",
        AiProviderKind::Google => "gemini-1.5-flash",
        AiProviderKind::Local => "bluey-local-answer-v0",
        _ => "default",
    }
}

fn default_answer_request(question: &str) -> AnswerRequest {
    let route = if dev_direct_provider_keys_enabled() {
        ai_status_from_env().route
    } else {
        managed_provider_route("balanced")
    };
    AnswerRequest::new(question, route).streaming()
}

fn vision_answer_request(question: &str, provider: ProviderSelector) -> AnswerRequest {
    let route = if matches!(provider.provider_kind, AiProviderKind::CueManaged) {
        managed_provider_route(provider.model_or("vision"))
    } else {
        ProviderRoute::direct(provider)
            .require(cue_core::AiCapability::Vision)
            .with_budgets(RouteBudget::realtime())
            .with_privacy(PrivacyFlags::managed_commercial().with_image_upload())
    };
    AnswerRequest::new(question, route).streaming()
}

fn managed_provider_route(lane: &str) -> ProviderRoute {
    let lane = managed_lane_name_from_value(lane).unwrap_or("balanced");
    let mut route = ProviderRoute::direct(ProviderSelector::cue_managed(lane))
        .with_budgets(RouteBudget::realtime())
        .with_policy(cue_core::ai::RouteSelectionPolicy::Balanced)
        .with_privacy(PrivacyFlags::managed_commercial());
    if lane == "vision" {
        route = route
            .require(cue_core::AiCapability::Vision)
            .with_privacy(PrivacyFlags::managed_commercial().with_image_upload());
    }
    route
}

fn select_vision_provider(paths: &AppPaths) -> Option<ProviderSelector> {
    let configured_model = env::var("BLUEY_VISION_MODEL")
        .or_else(|_| env::var("CUE_VISION_MODEL"))
        .ok()
        .filter(|value| !value.trim().is_empty());

    if dev_direct_vision_enabled() {
        if let Some(provider) = env::var("BLUEY_VISION_PROVIDER")
            .or_else(|_| env::var("CUE_VISION_PROVIDER"))
            .ok()
            .filter(|value| !value.trim().is_empty())
        {
            return Some(provider_selector(&provider, configured_model.as_deref()));
        }

        if env_configured("OPENAI_API_KEY") {
            return Some(provider_selector("openai", configured_model.as_deref()));
        }

        // Groq vision model names vary by availability. Require an explicit model so
        // the screenshot fallback does not accidentally send images to a text-only
        // low-latency default.
        if env_configured("GROQ_API_KEY") && configured_model.is_some() {
            return Some(provider_selector("groq", configured_model.as_deref()));
        }
    }

    if cloud_account_linked(paths) {
        return Some(ProviderSelector::cue_managed(
            managed_lane_name_from_value(configured_model.as_deref().unwrap_or("vision"))
                .unwrap_or("vision"),
        ));
    }

    None
}

fn answer_request_from_overlay(
    question: &str,
    provider: Option<String>,
    model: Option<String>,
    mode: Option<String>,
) -> AnswerRequest {
    let provider = provider
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("auto");
    let model = normalized_overlay_model(provider, model.as_deref());
    let route = if let Some(lane) = overlay_managed_lane(provider, model, mode.as_deref()) {
        managed_provider_route(lane)
    } else if dev_direct_provider_keys_enabled() {
        ProviderRoute::direct(provider_selector(provider, model))
    } else {
        managed_provider_route("balanced")
    };
    let mut request = AnswerRequest::new(question, route).streaming();

    if let Some(mode) = mode.filter(|value| !value.trim().is_empty()) {
        request = request.with_instructions(mode_instructions(&mode));
    }

    request
}

fn mode_instructions(mode: &str) -> String {
    match mode.trim().to_ascii_lowercase().as_str() {
        "code" => {
            "Answer in Code mode. Use a scan-friendly layout with `### Approach`, `### Code`, `### Explanation`, `### Complexity`, and `### Edge cases`. Put the main implementation in one fenced code block with a language tag so Bluey can render it as the code pane. Keep commentary practical and avoid unrelated theory.".to_string()
        }
        "system design" | "system-design" | "design" => {
            "Answer in System Design mode. Use `### Architecture`, `### Data flow`, `### Components`, `### Scaling`, `### Tradeoffs`, and `### Risks / next steps`. Prefer concrete services, storage choices, queues, cache boundaries, APIs, and failure modes. Use compact bullets and simple text diagrams when useful.".to_string()
        }
        "meeting" => {
            "Answer in Meeting mode. Be concise and source-grounded. Use `### Direct answer`, then only the relevant `### Evidence`, `### Decisions`, `### Action items`, and `### Follow-up` sections. Do not over-explain.".to_string()
        }
        "writing" => {
            "Answer in Writing mode. Produce polished copy first, then a short `### Notes` section explaining tone, edits, and optional variants. Keep the draft easy to reuse.".to_string()
        }
        _ => {
            "Answer in General mode. Auto-detect the task type. Put the direct answer first, then concise bullets for context, reasoning, and next steps. If the question is about code, debugging, algorithms, APIs, config, or terminal commands, still use `### Approach`, `### Code`, `### Explanation`, `### Complexity`, and `### Edge cases`, with fenced code blocks where useful. Keep it practical and easy to scan in a small overlay.".to_string()
        }
    }
}

fn is_auto_provider(provider: &str) -> bool {
    matches!(
        provider.trim().to_ascii_lowercase().as_str(),
        "auto" | "bluey_auto" | "bluey" | "bluey_managed" | "cue" | "cue_managed" | "managed"
    )
}

fn overlay_managed_lane<'a>(
    provider: &str,
    model: Option<&'a str>,
    mode: Option<&'a str>,
) -> Option<&'static str> {
    for value in [model, mode, Some(provider)].into_iter().flatten() {
        if let Some(lane) = managed_lane_name_from_value(value) {
            return Some(lane);
        }
    }
    if is_auto_provider(provider) {
        Some("balanced")
    } else {
        None
    }
}

fn managed_lane_name_from_value(value: &str) -> Option<&'static str> {
    let normalized = value.trim().to_ascii_lowercase().replace(['-', '_'], " ");
    match normalized.as_str() {
        "instant" | "quick" | "fast" | "easy" | "bluey managed instant" => Some("instant"),
        "auto" | "balanced" | "normal" | "default" | "general" | "bluey managed balanced" => {
            Some("balanced")
        }
        "deep" | "hard" | "reasoning" | "extra high" | "system design" | "code" => Some("deep"),
        "vision" | "screen" | "screenshot" | "analyse screen" | "analyze screen" => Some("vision"),
        _ => None,
    }
}

fn managed_lane_from_value(value: &str) -> ManagedLane {
    match managed_lane_name_from_value(value).unwrap_or("balanced") {
        "instant" => ManagedLane::Instant,
        "deep" => ManagedLane::Deep,
        "vision" => ManagedLane::Vision,
        _ => ManagedLane::Balanced,
    }
}

fn cloud_account_linked(paths: &AppPaths) -> bool {
    cue_cloud_client::tokens::tokens_available(paths)
}

fn normalized_overlay_model<'a>(provider: &str, model: Option<&'a str>) -> Option<&'a str> {
    let model = model.map(str::trim).filter(|value| !value.is_empty())?;
    let provider = provider.trim().to_ascii_lowercase();
    let lower_model = model.to_ascii_lowercase();
    let invalid_alias = matches!(
        (provider.as_str(), lower_model.as_str()),
        ("openai", "managed-reasoning")
            | ("groq", "managed-realtime")
            | ("cerebras", "managed-fast")
    );
    if invalid_alias {
        None
    } else {
        Some(model)
    }
}

fn merge_answer_instructions(
    request_instructions: Option<String>,
    session_instructions: Option<String>,
) -> Option<String> {
    let request = request_instructions
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let session = session_instructions
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());

    match (request, session) {
        (Some(request), Some(session)) => Some(format!(
            "Mode / request instructions:\n{request}\n\nSession answer rules:\n{session}"
        )),
        (Some(request), None) => Some(request),
        (None, Some(session)) => Some(session),
        (None, None) => None,
    }
}

fn provider_selector(provider: &str, model: Option<&str>) -> ProviderSelector {
    let provider_id = AiProviderId::new(provider);
    let provider_kind = match provider.trim().to_ascii_lowercase().as_str() {
        "bluey" | "bluey_managed" | "cue" | "cue_managed" | "managed" => AiProviderKind::CueManaged,
        "openai" => AiProviderKind::OpenAi,
        "anthropic" => AiProviderKind::Anthropic,
        "google" | "gemini" => AiProviderKind::Google,
        "azure" | "azure_openai" => AiProviderKind::AzureOpenAi,
        "mistral" => AiProviderKind::Mistral,
        "groq" => AiProviderKind::Groq,
        "cerebras" => AiProviderKind::Cerebras,
        "cohere" => AiProviderKind::Cohere,
        "deepgram" => AiProviderKind::Deepgram,
        "local" => AiProviderKind::Local,
        _ => AiProviderKind::Custom,
    };

    let selector = ProviderSelector::new(provider_id, provider_kind);
    match model.filter(|value| !value.trim().is_empty()) {
        Some(model) => selector.with_model(model),
        None => selector,
    }
}

async fn answer_context_for_question(
    daemon: &Arc<Daemon>,
    meeting: &MeetingRecord,
    question: &str,
) -> Vec<AnswerContext> {
    let mut context = answer_context_from_meeting(meeting);
    context.extend(retrieved_memory_contexts(daemon, meeting, question).await);
    context
}

async fn retrieved_memory_contexts(
    daemon: &Arc<Daemon>,
    meeting: &MeetingRecord,
    question: &str,
) -> Vec<AnswerContext> {
    if question.trim().is_empty() {
        return Vec::new();
    }

    let current_session_id = meeting.id.to_string();
    let mut contexts = Vec::new();
    let mut seen = std::collections::HashSet::new();

    match daemon
        .rag_indexer
        .query(question, 4, Some(&current_session_id))
        .await
    {
        Ok(hits) => {
            for hit in hits {
                if let Some(context) =
                    rag_hit_to_answer_context(hit, &current_session_id, &mut seen)
                {
                    contexts.push(context);
                }
            }
        }
        Err(error) => {
            debug!(
                session_id = %current_session_id,
                error = %error,
                "local RAG current-session query failed"
            );
        }
    }

    match daemon.rag_indexer.query(question, 6, None).await {
        Ok(hits) => {
            for hit in hits {
                if contexts.len() >= 8 {
                    break;
                }
                if let Some(context) =
                    rag_hit_to_answer_context(hit, &current_session_id, &mut seen)
                {
                    contexts.push(context);
                }
            }
        }
        Err(error) => {
            debug!(
                session_id = %current_session_id,
                error = %error,
                "local RAG global query failed"
            );
        }
    }

    contexts
}

fn rag_hit_to_answer_context(
    hit: cue_rag::RagHit,
    current_session_id: &str,
    seen: &mut std::collections::HashSet<(String, String)>,
) -> Option<AnswerContext> {
    if hit.score < 0.18 || hit.chunk_text.trim().is_empty() {
        return None;
    }
    let key = (hit.session_id.clone(), hit.chunk_text.clone());
    if !seen.insert(key) {
        return None;
    }

    let same_session = hit.session_id == current_session_id;
    let title = if same_session {
        "Relevant current-session memory"
    } else {
        "Relevant older Bluey memory"
    };
    let source = if same_session {
        "local RAG · current session".to_string()
    } else {
        format!("local RAG · session {}", hit.session_id)
    };
    let content = format!("Relevance: {:.2}\n{}", hit.score, hit.chunk_text.trim());
    Some(
        AnswerContext::new(AnswerContextKind::MeetingMemory, content)
            .with_title(title)
            .with_source(source),
    )
}

fn answer_context_from_meeting(meeting: &MeetingRecord) -> Vec<AnswerContext> {
    let mut context = Vec::new();
    if let Some(summary) = meeting
        .summary
        .as_ref()
        .filter(|summary| !summary.trim().is_empty())
    {
        context.push(
            AnswerContext::new(
                AnswerContextKind::MeetingMemory,
                format!("Compacted summary:\n{}", summary.trim()),
            )
            .with_title(format!("{} summary", meeting.title))
            .with_source("saved session summary"),
        );
    }

    let transcript = meeting
        .last_transcript_text_bounded(ANSWER_TRANSCRIPT_TURN_LIMIT, ANSWER_TRANSCRIPT_CHAR_BUDGET);
    if !transcript.trim().is_empty() {
        context.push(
            AnswerContext::transcript(transcript)
                .with_title(meeting.title.clone())
                .with_source("active meeting transcript"),
        );
    }

    let conversation = meeting.last_conversation_text(10);
    if !conversation.trim().is_empty() {
        context.push(
            AnswerContext::new(AnswerContextKind::MeetingMemory, conversation)
                .with_title("Recent Bluey Q&A")
                .with_source("active session answer history"),
        );
    }

    for artifact in meeting
        .context
        .iter()
        .rev()
        .filter(|artifact| {
            artifact.processing_status == ContextProcessingStatus::Ready
                || artifact
                    .note
                    .as_ref()
                    .is_some_and(|note| !note.trim().is_empty())
        })
        .take(12)
    {
        let mut content = format!("{} ({})", artifact.title, artifact.kind);
        if artifact.processing_status != ContextProcessingStatus::Ready {
            content.push_str(&format!("\nStatus: {}", artifact.processing_status));
            if let Some(error) = artifact
                .processing_error
                .as_ref()
                .filter(|error| !error.trim().is_empty())
            {
                content.push_str("\n");
                content.push_str(error);
            }
        }
        if let Some(note) = artifact
            .note
            .as_ref()
            .filter(|note| !note.trim().is_empty())
        {
            content.push_str("\n");
            content.push_str(note);
        }
        if let Some(preview) = artifact
            .text_preview
            .as_ref()
            .filter(|preview| !preview.trim().is_empty())
        {
            content.push_str("\n");
            content.push_str(preview);
        }
        context.push(
            AnswerContext::new(answer_context_kind(artifact.kind), content)
                .with_title(artifact.title.clone())
                .with_source(artifact.path.clone()),
        );
    }

    context
}

fn answer_context_kind(kind: ContextKind) -> AnswerContextKind {
    match kind {
        ContextKind::Image | ContextKind::Diagram => AnswerContextKind::Screenshot,
        ContextKind::Code | ContextKind::Document | ContextKind::Text => {
            AnswerContextKind::Document
        }
        ContextKind::Other => AnswerContextKind::Other,
    }
}

fn estimate_token_usage(request: &AnswerRequest, answer: &str) -> TokenUsage {
    let input_words = request
        .question
        .split_whitespace()
        .count()
        .saturating_add(
            request
                .instructions
                .as_deref()
                .unwrap_or_default()
                .split_whitespace()
                .count(),
        )
        .saturating_add(
            request
                .context
                .iter()
                .map(|context| context.content.split_whitespace().count())
                .sum::<usize>(),
        );
    let output_words = answer.split_whitespace().count();
    TokenUsage::new(
        estimate_tokens_from_words(input_words),
        estimate_tokens_from_words(output_words),
    )
}

fn estimate_tokens_from_words(words: usize) -> u32 {
    words
        .saturating_mul(4)
        .saturating_add(2)
        .checked_div(3)
        .unwrap_or(0)
        .min(u32::MAX as usize) as u32
}

async fn capture_loop(daemon: Arc<Daemon>, interval_secs: u64, mut stop_rx: oneshot::Receiver<()>) {
    loop {
        tokio::select! {
            _ = &mut stop_rx => break,
            result = capture_once_and_attach(&daemon) => {
                if let Err(error) = result {
                    warn!("screen context capture failed: {error:#}");
                    {
                        let mut capture = daemon.capture.lock().await;
                        capture.stop.take();
                    }
                    let _ = update_capture_state(&daemon, false, None).await;
                    push_system_card(
                        &daemon,
                        CardKind::Warning,
                        "Screen context stopped",
                        format!("{error:#}"),
                    )
                    .await;
                    break;
                }
            }
        }

        tokio::select! {
            _ = &mut stop_rx => break,
            _ = sleep(Duration::from_secs(interval_secs)) => {}
        }
    }
}

async fn capture_once_and_attach(daemon: &Arc<Daemon>) -> Result<()> {
    let capture_path = capture_screen_to_file(&daemon.paths).await?;
    let artifact = build_context_artifact(
        &daemon.paths,
        capture_path.display().to_string(),
        capture_path
            .file_name()
            .and_then(|name| name.to_str())
            .map(str::to_string),
        Some("Eye capture mode".to_string()),
    )?;

    let meeting_snapshot = attach_context_artifacts(daemon, vec![artifact.clone()]).await?;
    update_state_from_meeting(daemon, Some(&meeting_snapshot)).await?;
    refresh_overlay_context_items(daemon, &meeting_snapshot).await;

    let card = CueCard::new(
        CardKind::Context,
        "Screenshot attached",
        format!(
            "{} ({}:{}){}",
            artifact.title,
            artifact.kind,
            artifact.processing_status,
            artifact
                .processing_error
                .as_ref()
                .filter(|error| !error.trim().is_empty())
                .map(|error| format!("\n{error}"))
                .unwrap_or_default()
        ),
    )
    .with_source(artifact.path);
    let _ = send_overlay(daemon, OverlayCommand::PushCard { card }).await;
    write_state(daemon).await?;

    Ok(())
}

async fn capture_active_page_context(
    daemon: &Arc<Daemon>,
    source: impl Into<String>,
) -> Result<ContextArtifact> {
    let source = source.into();
    let (path, page) = capture_active_page_to_file(&daemon.paths).await?;
    let artifact = build_context_artifact(
        &daemon.paths,
        path.display().to_string(),
        Some(if page.title.trim().is_empty() {
            "Active page context".to_string()
        } else {
            page.title.clone()
        }),
        Some(format!(
            "Captured from active browser page{}{}",
            if page.url.trim().is_empty() { "" } else { ": " },
            page.url
        )),
    )?;

    let meeting_snapshot = attach_context_artifacts(daemon, vec![artifact.clone()]).await?;
    update_state_from_meeting(daemon, Some(&meeting_snapshot)).await?;
    refresh_overlay_context_items(daemon, &meeting_snapshot).await;
    push_system_card(
        daemon,
        CardKind::Context,
        "Page context attached",
        format!(
            "{}\n{} characters captured. Source: {source}.",
            artifact.title,
            page.text.chars().count()
        ),
    )
    .await;
    Ok(artifact)
}

async fn analyze_active_page_context(
    daemon: &Arc<Daemon>,
    question_context: Option<&str>,
) -> Result<()> {
    match capture_active_page_context(daemon, "overlay analyse").await {
        Ok(artifact) => {
            let mut question = format!(
                "Analyse the active browser page that was just attached as context: {}.",
                artifact.title
            );
            if let Some(context) = question_context
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                question.push_str("\n\nUser and live-caption context:\n");
                question.push_str(context);
            }
            question.push_str("\n\nAnswer the visible question or prompt if there is one, use the provided live-caption context when relevant, then give concise next steps.");
            let _ = answer_question(daemon, question, "overlay analyse").await?;
        }
        Err(page_error) => {
            analyze_screen_with_screenshot_fallback(daemon, page_error, question_context).await?;
        }
    }
    Ok(())
}

async fn analyze_screen_with_screenshot_fallback(
    daemon: &Arc<Daemon>,
    page_error: anyhow::Error,
    question_context: Option<&str>,
) -> Result<()> {
    let page_error_text = format!("{page_error:#}");
    let Some(provider) = select_vision_provider(&daemon.paths) else {
        push_system_card(
            daemon,
            CardKind::Warning,
            "Analyse needs vision",
            format!(
                "Bluey could not read browser page text. Sign in to Bluey for cloud screen analysis, then try again.\n\nBrowser text error: {}\n\nDeveloper direct vision providers require a debug/dev build with BLUEY_DEV_DIRECT_VISION=1.",
                compact_snippet(&page_error_text, 520)
            ),
        )
        .await;
        return Ok(());
    };

    push_system_card(
        daemon,
        CardKind::Warning,
        "Page text unavailable",
        format!(
            "Browser text was not available, so Bluey is capturing one screenshot and routing it to vision. Reason: {}",
            compact_snippet(&page_error_text, 360)
        ),
    )
    .await;

    let capture_path = capture_screen_to_file(&daemon.paths).await?;
    let artifact = build_context_artifact(
        &daemon.paths,
        capture_path.display().to_string(),
        Some("Screen capture fallback".to_string()),
        Some(format!(
            "Captured after active browser page text failed: {}",
            compact_snippet(&page_error_text, 220)
        )),
    )?;
    let meeting_snapshot = attach_context_artifacts(daemon, vec![artifact.clone()]).await?;
    update_state_from_meeting(daemon, Some(&meeting_snapshot)).await?;
    refresh_overlay_context_items(daemon, &meeting_snapshot).await;

    let card = CueCard::new(
        CardKind::Context,
        "Screenshot fallback attached",
        format!(
            "{}\n{} bytes. Vision route: {}.",
            artifact.title,
            artifact.size_bytes.unwrap_or_default(),
            provider.display_label()
        ),
    )
    .with_source(artifact.path.clone());
    let _ = send_overlay(daemon, OverlayCommand::PushCard { card }).await;
    write_state(daemon).await?;

    let mut question = format!(
        "Browser page text was unavailable, so analyze the attached screenshot instead. If the screenshot contains a question, task, code, diagram, or UI, answer it directly and give concise next steps. Browser text error for context: {}",
        compact_snippet(&page_error_text, 260)
    );
    if let Some(context) = question_context
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        question.push_str("\n\nUser and live-caption context:\n");
        question.push_str(context);
    }
    let mut request = vision_answer_request(&question, provider);
    request.context = answer_context_for_question(daemon, &meeting_snapshot, &question).await;
    request.context.push(
        AnswerContext::new(
            AnswerContextKind::Screenshot,
            format!(
                "{} ({})\nCaptured as screenshot fallback after browser text extraction failed.",
                artifact.title, artifact.kind
            ),
        )
        .with_title(artifact.title)
        .with_source(artifact.path),
    );

    let _ = answer_with_provider_runtime(daemon, request, "overlay screenshot analyse").await?;
    Ok(())
}

async fn capture_active_page_to_file(paths: &AppPaths) -> Result<(PathBuf, ActivePageCapture)> {
    let mut page = tokio::task::spawn_blocking(capture_active_page_platform)
        .await
        .context("active page capture task failed")??;
    page.text = normalize_page_text(&page.text, 240_000);
    if page.text.trim().len() < 20 {
        return Err(anyhow!("active page did not expose enough readable text"));
    }

    let page_dir = paths.data_dir.join("page-context");
    tokio::fs::create_dir_all(&page_dir)
        .await
        .with_context(|| format!("failed to create {}", page_dir.display()))?;
    let file_name = format!(
        "page-{}-{}.txt",
        epoch_ms()?,
        sanitize_file_stem(if page.title.trim().is_empty() {
            "active-page"
        } else {
            &page.title
        })
    );
    let path = page_dir.join(file_name);
    let contents = format!(
        "Title: {}\nURL: {}\nCaptured from active browser page.\n\n{}",
        page.title.trim(),
        page.url.trim(),
        page.text
    );
    tokio::fs::write(&path, contents)
        .await
        .with_context(|| format!("failed to write {}", path.display()))?;
    Ok((path, page))
}

async fn capture_screen_to_file(paths: &AppPaths) -> Result<PathBuf> {
    ensure_screen_capture_supported()?;
    let capture_dir = paths.data_dir.join("captures");
    tokio::fs::create_dir_all(&capture_dir)
        .await
        .with_context(|| format!("failed to create {}", capture_dir.display()))?;
    let path = capture_dir.join(format!("eye-capture-{}.png", epoch_ms()?));

    let path_for_task = path.clone();
    tokio::task::spawn_blocking(move || capture_screen_platform(&path_for_task))
        .await
        .context("screen capture task failed")??;

    let metadata = tokio::fs::metadata(&path).await?;
    if metadata.len() == 0 {
        return Err(anyhow!("screen capture was empty"));
    }
    Ok(path)
}

#[cfg(target_os = "macos")]
fn ensure_screen_capture_supported() -> Result<()> {
    Ok(())
}

#[cfg(not(target_os = "macos"))]
#[cfg(not(target_os = "windows"))]
fn ensure_screen_capture_supported() -> Result<()> {
    Err(anyhow!(
        "continuous screen context capture is implemented on macOS first"
    ))
}

#[cfg(target_os = "windows")]
fn ensure_screen_capture_supported() -> Result<()> {
    Ok(())
}

#[cfg(target_os = "macos")]
fn capture_screen_platform(path: &Path) -> Result<()> {
    let status = Command::new("screencapture")
        .arg("-x")
        .arg(path)
        .status()
        .context("failed to launch macOS screencapture")?;
    if !status.success() {
        return Err(anyhow!("screen capture failed or was denied"));
    }
    Ok(())
}

#[cfg(not(target_os = "macos"))]
#[cfg(not(target_os = "windows"))]
fn capture_screen_platform(_path: &Path) -> Result<()> {
    ensure_screen_capture_supported()
}

#[cfg(target_os = "windows")]
fn capture_screen_platform(path: &Path) -> Result<()> {
    let escaped_path = powershell_single_quoted(path);
    let script = format!(
        r#"
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
$bounds = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
$bitmap = New-Object System.Drawing.Bitmap $bounds.Width, $bounds.Height
$graphics = [System.Drawing.Graphics]::FromImage($bitmap)
$graphics.CopyFromScreen($bounds.Location, [System.Drawing.Point]::Empty, $bounds.Size)
$bitmap.Save({escaped_path}, [System.Drawing.Imaging.ImageFormat]::Png)
$graphics.Dispose()
$bitmap.Dispose()
"#
    );
    let status = Command::new("powershell")
        .arg("-NoProfile")
        .arg("-ExecutionPolicy")
        .arg("Bypass")
        .arg("-Command")
        .arg(script)
        .status()
        .context("failed to launch Windows screen capture")?;
    if !status.success() {
        return Err(anyhow!("Windows screen capture failed or was denied"));
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn capture_active_page_platform() -> Result<ActivePageCapture> {
    let browser_order = macos_browser_order();
    let mut errors = Vec::new();

    for browser in browser_order {
        let result = if browser == "Safari" {
            macos_capture_safari_page()
        } else {
            macos_capture_chromium_page(&browser)
        };

        match result {
            Ok(page) if !page.text.trim().is_empty() => return Ok(page),
            Ok(_) => errors.push(format!("{browser}: empty page text")),
            Err(error) => errors.push(format!("{browser}: {error:#}")),
        }
    }

    Err(anyhow!(
        "could not capture active browser page text. Supported browsers: Chrome, Edge, Brave, Arc, Chromium, Safari. Details: {}",
        errors.join(" | ")
    ))
}

#[cfg(target_os = "macos")]
fn macos_browser_order() -> Vec<String> {
    let supported = [
        "Google Chrome",
        "Microsoft Edge",
        "Brave Browser",
        "Arc",
        "Chromium",
        "Safari",
    ];

    let mut ordered = Vec::new();
    if let Some(frontmost) = macos_frontmost_app_name() {
        if supported.contains(&frontmost.as_str()) && macos_app_is_running(&frontmost) {
            ordered.push(frontmost);
            return ordered;
        }
    }

    for browser in supported {
        if !ordered.iter().any(|known| known == browser) && macos_app_is_running(browser) {
            ordered.push(browser.to_string());
        }
    }
    ordered
}

#[cfg(target_os = "macos")]
fn macos_frontmost_app_name() -> Option<String> {
    let front = Command::new("lsappinfo").arg("front").output().ok()?;
    if !front.status.success() {
        return None;
    }
    let asn = String::from_utf8_lossy(&front.stdout).trim().to_string();
    if asn.is_empty() {
        return None;
    }

    let info = Command::new("lsappinfo")
        .arg("info")
        .arg("-only")
        .arg("name")
        .arg(asn)
        .output()
        .ok()?;
    if !info.status.success() {
        return None;
    }

    parse_lsdisplay_name(&String::from_utf8_lossy(&info.stdout))
}

#[cfg(target_os = "macos")]
fn parse_lsdisplay_name(output: &str) -> Option<String> {
    let marker = "\"LSDisplayName\"=\"";
    let start = output.find(marker)? + marker.len();
    let rest = &output[start..];
    let end = rest.find('"')?;
    let value = rest[..end].trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

#[cfg(target_os = "macos")]
fn macos_app_is_running(name: &str) -> bool {
    Command::new("pgrep")
        .arg("-x")
        .arg(name)
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

#[cfg(target_os = "macos")]
fn macos_capture_chromium_page(app_name: &str) -> Result<ActivePageCapture> {
    let script = format!(
        r#"
tell application "{}"
  if not (exists front window) then error "no front window"
  set payload to execute active tab of front window javascript "{}"
  return payload
end tell
"#,
        apple_script_string(app_name),
        apple_script_string(active_page_javascript())
    );
    parse_active_page_osascript_output(app_name, script)
}

#[cfg(target_os = "macos")]
fn macos_capture_safari_page() -> Result<ActivePageCapture> {
    let script = format!(
        r#"
tell application "Safari"
  if not (exists front window) then error "no front window"
  set payload to do JavaScript "{}" in current tab of front window
  return payload
end tell
"#,
        apple_script_string(active_page_javascript())
    );
    parse_active_page_osascript_output("Safari", script)
}

#[cfg(target_os = "macos")]
fn parse_active_page_osascript_output(app_name: &str, script: String) -> Result<ActivePageCapture> {
    let output = Command::new("osascript")
        .arg("-e")
        .arg(script)
        .output()
        .with_context(|| format!("failed to ask {app_name} for active page text"))?;
    if !output.status.success() {
        return Err(anyhow!(
            "{}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    serde_json::from_str(&stdout).with_context(|| format!("{app_name} returned invalid page JSON"))
}

#[cfg(target_os = "windows")]
fn capture_active_page_platform() -> Result<ActivePageCapture> {
    let script = r#"
$ErrorActionPreference = "Stop"
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes

$browserNames = @("chrome", "msedge", "brave", "arc", "chromium", "firefox")
$processes = Get-Process | Where-Object {
  $browserNames -contains $_.ProcessName -and $_.MainWindowHandle -ne 0
}

foreach ($process in $processes) {
  try {
    $window = [System.Windows.Automation.AutomationElement]::FromHandle($process.MainWindowHandle)
    if ($null -eq $window) { continue }

    $condition = New-Object System.Windows.Automation.PropertyCondition(
      [System.Windows.Automation.AutomationElement]::ControlTypeProperty,
      [System.Windows.Automation.ControlType]::Document
    )
    $documents = $window.FindAll([System.Windows.Automation.TreeScope]::Descendants, $condition)

    for ($i = 0; $i -lt $documents.Count; $i++) {
      $document = $documents.Item($i)
      $pattern = $null

      if ($document.TryGetCurrentPattern([System.Windows.Automation.TextPattern]::Pattern, [ref]$pattern)) {
        $text = $pattern.DocumentRange.GetText(-1)
        if (-not [string]::IsNullOrWhiteSpace($text) -and $text.Trim().Length -gt 20) {
          [pscustomobject]@{
            title = $window.Current.Name
            url = ""
            text = $text
          } | ConvertTo-Json -Compress
          exit 0
        }
      }

      if (-not [string]::IsNullOrWhiteSpace($document.Current.Name) -and $document.Current.Name.Trim().Length -gt 120) {
        [pscustomobject]@{
          title = $window.Current.Name
          url = ""
          text = $document.Current.Name
        } | ConvertTo-Json -Compress
        exit 0
      }
    }
  } catch {
    continue
  }
}

throw "No supported browser window exposed readable page text through Windows UI Automation."
"#;
    let output = Command::new("powershell")
        .arg("-NoProfile")
        .arg("-ExecutionPolicy")
        .arg("Bypass")
        .arg("-Command")
        .arg(script)
        .output()
        .context("failed to ask Windows UI Automation for active page text")?;
    if !output.status.success() {
        return Err(anyhow!(
            "{}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    serde_json::from_str(&stdout).context("Windows UI Automation returned invalid page JSON")
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn capture_active_page_platform() -> Result<ActivePageCapture> {
    Err(anyhow!(
        "active page text capture is implemented for macOS and Windows browser tabs first; use screenshot capture or attach a file on this platform"
    ))
}

async fn attach_context_artifacts(
    daemon: &Arc<Daemon>,
    artifacts: Vec<ContextArtifact>,
) -> Result<MeetingRecord> {
    let indexed_artifacts = artifacts.clone();
    let meeting_snapshot = {
        let mut meeting_guard = daemon.meeting.lock().await;
        if meeting_guard.is_none() {
            *meeting_guard = Some(MeetingRecord::new(Some("Ad hoc meeting".to_string())));
        }

        let meeting = meeting_guard.as_mut().expect("meeting exists");
        meeting.context.extend(artifacts);
        daemon.store.save_active(meeting)?;
        meeting.clone()
    };

    index_context_artifacts_for_rag(daemon, meeting_snapshot.id.to_string(), indexed_artifacts);
    Ok(meeting_snapshot)
}

async fn continue_session(
    daemon: &Arc<Daemon>,
    source: impl Into<String>,
) -> Result<MeetingRecord> {
    let source = source.into();
    enum ContinueOutcome {
        Active(MeetingRecord),
        Restored(MeetingRecord),
        Created(MeetingRecord),
    }

    let outcome = {
        let mut meeting_guard = daemon.meeting.lock().await;
        if let Some(meeting) = meeting_guard.as_ref() {
            ContinueOutcome::Active(meeting.clone())
        } else if let Some(mut meeting) = daemon.store.last_meeting()? {
            meeting.ended_at = None;
            daemon.store.save_active(&meeting)?;
            *meeting_guard = Some(meeting.clone());
            ContinueOutcome::Restored(meeting)
        } else {
            let meeting = MeetingRecord::new(Some("Bluey session".to_string()));
            daemon.store.save_active(&meeting)?;
            *meeting_guard = Some(meeting.clone());
            ContinueOutcome::Created(meeting)
        }
    };

    let (meeting, title, body, should_warm_memory) = match outcome {
        ContinueOutcome::Active(meeting) => {
            let body = format!(
                "Continuing {} with {} transcript segment(s) and {} context item(s).",
                meeting.title,
                meeting.transcript.len(),
                meeting.context.len()
            );
            (meeting, "Session continued", body, true)
        }
        ContinueOutcome::Restored(meeting) => {
            let body = format!(
                "Loaded latest saved session: {}.\n{} transcript segment(s), {} context item(s).",
                meeting.title,
                meeting.transcript.len(),
                meeting.context.len()
            );
            (meeting, "Session loaded", body, true)
        }
        ContinueOutcome::Created(meeting) => {
            let body = format!(
                "Started a new session from {source}. Attach docs/page context when needed."
            );
            (meeting, "Session started", body, false)
        }
    };

    if should_warm_memory {
        reindex_meeting_for_rag(daemon, meeting.clone());
    }
    update_state_from_meeting(daemon, Some(&meeting)).await?;
    refresh_overlay_context_items(daemon, &meeting).await;
    refresh_overlay_sessions(daemon).await;
    push_system_card(daemon, CardKind::System, title, body).await;
    write_state(daemon).await?;
    Ok(meeting)
}

async fn open_meeting_session(daemon: &Arc<Daemon>, id: uuid::Uuid) -> Result<MeetingRecord> {
    let selected = daemon
        .store
        .load_by_id(id)?
        .with_context(|| format!("session {id} not found"))?;

    let archived_summary = {
        let mut meeting_guard = daemon.meeting.lock().await;
        if let Some(mut current) = meeting_guard.take().filter(|current| current.id != id) {
            current.ended_at = Some(clock::now_epoch_ms_string());
            let recap = generate_recap(&current);
            current.summary = Some(recap.summary);
            let title = current.title.clone();
            let path = daemon.store.archive(&current)?;
            Some(format!("{title} archived to {}.", path.display()))
        } else {
            None
        }
    };

    let mut selected = selected;
    selected.ended_at = None;
    daemon.store.save_active(&selected)?;
    {
        let mut meeting_guard = daemon.meeting.lock().await;
        *meeting_guard = Some(selected.clone());
    }

    reindex_meeting_for_rag(daemon, selected.clone());
    update_state_from_meeting(daemon, Some(&selected)).await?;
    refresh_overlay_context_items(daemon, &selected).await;
    refresh_overlay_sessions(daemon).await;
    push_system_card(
        daemon,
        CardKind::System,
        "Session loaded",
        format!(
            "Continuing {}.\n{} transcript segment(s), {} context item(s).{}",
            selected.title,
            selected.transcript.len(),
            selected.context.len(),
            archived_summary
                .map(|summary| format!("\n{summary}"))
                .unwrap_or_default()
        ),
    )
    .await;
    write_state(daemon).await?;
    Ok(selected)
}

async fn rename_meeting_session(
    daemon: &Arc<Daemon>,
    id: uuid::Uuid,
    title: &str,
) -> Result<MeetingRecord> {
    let renamed = daemon.store.rename(id, title)?;
    {
        let mut meeting_guard = daemon.meeting.lock().await;
        if let Some(active) = meeting_guard.as_mut().filter(|active| active.id == id) {
            active.title = renamed.title.clone();
            daemon.store.save_active(active)?;
        }
    }
    refresh_overlay_sessions(daemon).await;
    push_system_card(
        daemon,
        CardKind::System,
        "Session renamed",
        format!("Now called {}.", renamed.title),
    )
    .await;
    write_state(daemon).await?;
    Ok(renamed)
}

async fn delete_meeting_session(daemon: &Arc<Daemon>, id: uuid::Uuid) -> Result<()> {
    let is_active = {
        let meeting_guard = daemon.meeting.lock().await;
        meeting_guard
            .as_ref()
            .is_some_and(|meeting| meeting.id == id)
    };
    if is_active {
        let _ = stop_audio_capture(daemon).await;
        let _ = stop_screen_capture(daemon, "session deleted").await;
        set_overlay_listening_state(daemon, ListeningState::Paused).await;
    }

    let was_active = {
        let mut meeting_guard = daemon.meeting.lock().await;
        if meeting_guard
            .as_ref()
            .is_some_and(|meeting| meeting.id == id)
        {
            *meeting_guard = None;
            true
        } else {
            false
        }
    };

    let deleted = daemon.store.delete(id)?;
    if !deleted {
        anyhow::bail!("session {id} not found");
    }

    daemon.rag_indexer.delete_session(id.to_string());

    if was_active {
        update_state_from_meeting(daemon, None).await?;
        let _ = send_overlay(daemon, OverlayCommand::SetContextItems { items: vec![] }).await;
    }
    refresh_overlay_sessions(daemon).await;
    push_system_card(
        daemon,
        CardKind::System,
        "Session deleted",
        "The saved recording was removed from this device.",
    )
    .await;
    write_state(daemon).await
}

async fn start_new_session(
    daemon: &Arc<Daemon>,
    source: impl Into<String>,
) -> Result<MeetingRecord> {
    let source = source.into();
    let archived_summary = {
        let mut meeting_guard = daemon.meeting.lock().await;
        if let Some(mut meeting) = meeting_guard.take() {
            meeting.ended_at = Some(clock::now_epoch_ms_string());
            let recap = generate_recap(&meeting);
            meeting.summary = Some(recap.summary);
            let title = meeting.title.clone();
            let path = daemon.store.archive(&meeting)?;
            Some(format!("{title} archived to {}.", path.display()))
        } else {
            None
        }
    };

    let meeting = {
        let mut meeting_guard = daemon.meeting.lock().await;
        let meeting = MeetingRecord::new(Some("Bluey session".to_string()));
        daemon.store.save_active(&meeting)?;
        *meeting_guard = Some(meeting.clone());
        meeting
    };

    update_state_from_meeting(daemon, Some(&meeting)).await?;
    refresh_overlay_context_items(daemon, &meeting).await;
    refresh_overlay_sessions(daemon).await;
    push_system_card(
        daemon,
        CardKind::System,
        "New session started",
        format!(
            "{}\nSource: {source}. Attach docs/page context for this session when needed.",
            archived_summary.unwrap_or_else(|| "No active session needed archiving.".to_string())
        ),
    )
    .await;
    write_state(daemon).await?;
    Ok(meeting)
}

async fn set_answer_instructions(
    daemon: &Arc<Daemon>,
    instructions: Option<String>,
) -> Result<MeetingRecord> {
    let mut meeting_guard = daemon.meeting.lock().await;
    if meeting_guard.is_none() {
        *meeting_guard = Some(MeetingRecord::new(Some("Ad hoc meeting".to_string())));
    }

    let meeting = meeting_guard.as_mut().expect("meeting exists");
    meeting.answer_instructions = instructions;
    daemon.store.save_active(meeting)?;
    Ok(meeting.clone())
}

async fn update_capture_state(
    daemon: &Arc<Daemon>,
    active: bool,
    interval_secs: Option<u64>,
) -> Result<()> {
    {
        let mut state = daemon.state.lock().await;
        state.screen_capture_active = active;
        state.screen_capture_interval_secs = interval_secs;
    }
    write_state(daemon).await
}

async fn push_system_card(
    daemon: &Arc<Daemon>,
    kind: CardKind,
    title: impl Into<String>,
    body: impl Into<String>,
) {
    let card = CueCard::new(kind, title, body).with_source("screen context");
    let _ = send_overlay(daemon, OverlayCommand::PushCard { card }).await;
}

async fn current_or_last_meeting(daemon: &Arc<Daemon>) -> Result<Option<MeetingRecord>> {
    if let Some(active) = daemon.meeting.lock().await.as_ref() {
        Ok(Some(active.clone()))
    } else {
        daemon.store.last_meeting()
    }
}

async fn push_recap_card(daemon: &Arc<Daemon>) -> Result<()> {
    let Some(meeting) = current_or_last_meeting(daemon).await? else {
        push_system_card(
            daemon,
            CardKind::System,
            "Session recap",
            "No session has been captured yet.",
        )
        .await;
        return Ok(());
    };

    let recap = generate_recap(&meeting);
    let mut body = recap.summary.clone();

    if !recap.decisions.is_empty() {
        body.push_str("\n\nDecisions:");
        for decision in recap.decisions.iter().take(6) {
            body.push_str("\n- ");
            body.push_str(&decision.text);
        }
    }

    if !recap.action_items.is_empty() {
        body.push_str("\n\nAction items:");
        for item in recap.action_items.iter().take(6) {
            body.push_str("\n- ");
            body.push_str(&item.text);
        }
    }

    push_system_card(daemon, CardKind::System, "Session recap", body).await;
    write_state(daemon).await
}

async fn push_context_list_card(daemon: &Arc<Daemon>) -> Result<()> {
    let Some(meeting) = current_or_last_meeting(daemon).await? else {
        push_system_card(
            daemon,
            CardKind::Context,
            "Attached context",
            "No session is active yet. Use Attach Files after starting Bluey.",
        )
        .await;
        return Ok(());
    };

    if meeting.context.is_empty() {
        push_system_card(
            daemon,
            CardKind::Context,
            "Attached context",
            "No documents, screenshots, page captures, or notes are attached yet.",
        )
        .await;
        return Ok(());
    }

    let mut body = format!("{} attached item(s):", meeting.context.len());
    for (index, item) in meeting.context.iter().rev().take(12).enumerate() {
        body.push_str(&format!(
            "\n{}. [{}:{}] {}",
            index + 1,
            item.kind,
            item.processing_status,
            item.title
        ));
        if let Some(note) = item.note.as_ref().filter(|note| !note.trim().is_empty()) {
            body.push_str(" - ");
            body.push_str(note.trim());
        }
        if let Some(error) = item
            .processing_error
            .as_ref()
            .filter(|error| !error.trim().is_empty())
        {
            body.push_str(" - ");
            body.push_str(error.trim());
        }
        if let Some(size) = item.size_bytes {
            body.push_str(&format!(" ({size} bytes)"));
        }
    }

    push_system_card(daemon, CardKind::Context, "Attached context", body).await;
    write_state(daemon).await
}

fn ai_status_from_env() -> AiRuntimeStatus {
    let mut status = AiRuntimeStatus::scaffolded(vec![
        provider_status(provider_client_config(&ProviderSelector::cue_managed(
            "bluey-router-v1",
        ))),
        provider_status(provider_client_config(&ProviderSelector::cerebras(
            "llama3.1-8b",
        ))),
        provider_status(provider_client_config(&ProviderSelector::groq(
            "llama-3.1-8b-instant",
        ))),
        provider_status(provider_client_config(&ProviderSelector::openai(
            "gpt-4.1-mini",
        ))),
        provider_status(provider_client_config(&ProviderSelector::anthropic(
            "claude-3-7-sonnet-latest",
        ))),
        ProviderStatus::healthy(
            ProviderSelector::local("bluey-local-answer-v0"),
            AiCapabilities::chat(),
        ),
    ]);
    status.streaming_answers_enabled = true;
    status.vision_enabled = status
        .providers
        .iter()
        .any(|provider| provider.is_usable() && provider.capabilities.vision);
    status.stt_enabled = status
        .providers
        .iter()
        .any(|provider| provider.is_usable() && provider.capabilities.stt);
    status
}

fn provider_status(config: ProviderClientConfig) -> ProviderStatus {
    if let Some(message) = config.missing_configuration_message() {
        ProviderStatus::unknown(config.provider).disabled(message)
    } else if let Some(message) = config.unavailable_message() {
        ProviderStatus::unknown(config.provider).unavailable(message)
    } else {
        ProviderStatus::healthy(config.provider, config.capabilities)
    }
}

fn cloud_status_from_env(paths: &AppPaths) -> CloudSyncStatus {
    let endpoint = CloudEndpointConfig::new(
        env::var("BLUEY_CLOUD_API_URL")
            .or_else(|_| env::var("CUE_CLOUD_API_URL"))
            .ok()
            .or_else(|| {
                load_account(paths)
                    .ok()
                    .flatten()
                    .map(|account| account.api_url)
            })
            .unwrap_or_else(|| "http://127.0.0.1:8787".to_string()),
        cloud_environment_from_env(),
    );

    let account = load_account(paths).ok().flatten();
    if cloud_token_configured() || cue_cloud_client::tokens::tokens_available(paths) {
        CloudSyncStatus::ready(
            endpoint,
            env::var("BLUEY_WORKSPACE_ID")
                .or_else(|_| env::var("CUE_WORKSPACE_ID"))
                .ok()
                .or_else(|| account.as_ref().map(|account| account.workspace_id.clone()))
                .unwrap_or_else(|| "default".to_string()),
            env::var("BLUEY_USER_ID")
                .or_else(|_| env::var("CUE_USER_ID"))
                .ok()
                .or_else(|| account.as_ref().map(|account| account.user_id.clone()))
                .unwrap_or_else(|| "local-user".to_string()),
        )
        .with_device_id(
            env::var("BLUEY_DEVICE_ID")
                .or_else(|_| env::var("CUE_DEVICE_ID"))
                .ok()
                .or_else(|| account.as_ref().map(|account| account.device_id.clone()))
                .unwrap_or_else(|| "local-device".to_string()),
        )
    } else {
        let message = if account.is_some() {
            "account is linked but no Bluey cloud token is stored yet"
        } else {
            "sign in or set BLUEY_CLOUD_TOKEN to enable secure cloud sync"
        };
        CloudSyncStatus::disabled(message).with_device_id(
            env::var("BLUEY_DEVICE_ID")
                .or_else(|_| env::var("CUE_DEVICE_ID"))
                .ok()
                .or_else(|| account.as_ref().map(|account| account.device_id.clone()))
                .unwrap_or_else(|| "local-device".to_string()),
        )
    }
}

fn cloud_environment_from_env() -> CloudEnvironment {
    match env::var("BLUEY_CLOUD_ENV")
        .or_else(|_| env::var("CUE_CLOUD_ENV"))
        .unwrap_or_else(|_| "development".to_string())
        .to_ascii_lowercase()
        .as_str()
    {
        "local" => CloudEnvironment::Local,
        "staging" => CloudEnvironment::Staging,
        "production" | "prod" => CloudEnvironment::Production,
        _ => CloudEnvironment::Development,
    }
}

fn cloud_token_configured() -> bool {
    cloud_access_token_from_env().is_some()
}

fn cloud_access_token_from_env() -> Option<String> {
    env::var("BLUEY_CLOUD_TOKEN")
        .ok()
        .or_else(|| env::var("BLUEY_API_TOKEN").ok())
        .or_else(|| env::var("CUE_CLOUD_TOKEN").ok())
        .or_else(|| env::var("CUE_API_TOKEN").ok())
        .filter(|value| !value.trim().is_empty())
}

fn env_configured(name: &str) -> bool {
    env::var_os(name)
        .filter(|value| !value.is_empty())
        .is_some()
}

fn search_memory(meetings: &[MeetingRecord], query: &str, limit: usize) -> Vec<MemoryHit> {
    let terms = query_terms(query);
    if terms.is_empty() {
        return Vec::new();
    }

    let mut hits = Vec::new();
    for meeting in meetings {
        push_memory_hit(
            &mut hits,
            meeting,
            "meeting",
            format!(
                "{} {}",
                meeting.title,
                meeting.summary.clone().unwrap_or_default()
            ),
            &terms,
        );

        if let Some(instructions) = meeting.answer_instructions.as_ref() {
            push_memory_hit(
                &mut hits,
                meeting,
                "answer_instructions",
                instructions.clone(),
                &terms,
            );
        }

        for segment in &meeting.transcript {
            push_memory_hit(
                &mut hits,
                meeting,
                "transcript",
                format!("{}: {}", segment.speaker, segment.text),
                &terms,
            );
        }

        for item in &meeting.context {
            push_memory_hit(
                &mut hits,
                meeting,
                "context",
                format!(
                    "{} {} {}",
                    item.title,
                    item.path,
                    item.note.clone().unwrap_or_default()
                ),
                &terms,
            );
        }

        for item in &meeting.action_items {
            push_memory_hit(&mut hits, meeting, "action_item", item.text.clone(), &terms);
        }

        for decision in &meeting.decisions {
            push_memory_hit(
                &mut hits,
                meeting,
                "decision",
                decision.text.clone(),
                &terms,
            );
        }
    }

    hits.sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then_with(|| right.meeting_id.cmp(&left.meeting_id))
    });
    hits.truncate(limit);
    hits
}

fn push_memory_hit(
    hits: &mut Vec<MemoryHit>,
    meeting: &MeetingRecord,
    source: &str,
    text: String,
    terms: &[String],
) {
    let score = score_text(&text, terms);
    if score == 0 {
        return;
    }

    hits.push(MemoryHit {
        meeting_id: meeting.id,
        meeting_title: meeting.title.clone(),
        source: source.to_string(),
        snippet: compact_snippet(&text, 220),
        score,
    });
}

fn query_terms(query: &str) -> Vec<String> {
    query
        .split(|ch: char| !ch.is_alphanumeric())
        .map(str::trim)
        .filter(|term| term.len() >= 2)
        .map(|term| term.to_ascii_lowercase())
        .collect()
}

fn score_text(text: &str, terms: &[String]) -> usize {
    let lower = text.to_ascii_lowercase();
    terms
        .iter()
        .map(|term| lower.matches(term).count() * term.len())
        .sum()
}

fn compact_snippet(text: &str, max_chars: usize) -> String {
    let clean = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if clean.chars().count() <= max_chars {
        return clean;
    }
    let mut snippet = clean.chars().take(max_chars).collect::<String>();
    snippet.push_str("...");
    snippet
}

async fn write_state(daemon: &Arc<Daemon>) -> Result<()> {
    let state = daemon.state.lock().await.clone();
    let json = serde_json::to_vec_pretty(&state)?;
    tokio::fs::write(&daemon.paths.state_file, json)
        .await
        .with_context(|| format!("failed to write {}", daemon.paths.state_file.display()))?;
    Ok(())
}

/// Convert a `TranscriptEvent` into an `SttSegmentMetadata` for the downstream consumer.
fn transcript_event_to_stt_segment(
    event: &cue_core::stt::TranscriptEvent,
) -> Option<cue_core::audio::SttSegmentMetadata> {
    use cue_core::pcm::AudioSource;
    use cue_core::stt::TranscriptEvent;
    match event {
        TranscriptEvent::Final { text, source, .. }
        | TranscriptEvent::Partial { text, source, .. } => {
            let kind = match source {
                AudioSource::System => AudioSourceKind::System,
                AudioSource::Microphone => AudioSourceKind::Microphone,
            };
            let is_final = matches!(event, TranscriptEvent::Final { .. });
            let segment = cue_core::audio::SttSegmentMetadata::new(text.clone(), 0, 0, is_final)
                .with_source(kind)
                .with_speaker_label(kind.default_label());
            Some(segment)
        }
        TranscriptEvent::SpeakerLabel { .. } => None,
    }
}

async fn shutdown_daemon(daemon: &Arc<Daemon>) {
    if let Some(capture) = daemon.system_audio.lock().await.take() {
        capture.stop().await;
    }
    let _ = stop_audio_capture(daemon).await;
    if let Some(meeting) = daemon.meeting.lock().await.as_ref() {
        let _ = daemon.store.save_active(meeting);
    }
    if let Some(mut overlay) = daemon.overlay.lock().await.take() {
        let _ = overlay.send(&OverlayCommand::Shutdown);
        let _ = overlay.child.kill();
        let _ = overlay.child.wait();
    }
    let _ = tokio::fs::remove_file(&daemon.paths.state_file).await;
}

/// Production overlay path with R11 hardening:
/// - Env-override gating (BLUEY_DEV_OVERLAY required for overrides in release builds)
/// - Binary path canonicalization + install-dir containment check
/// - Per-session token passed via env var; events without matching token dropped
/// - Per-event field length limits; oversized events dropped + logged
/// - UI state-machine: AttachFilesRequested allowed from drag/drop idle or AttachOpen, etc.
fn spawn_overlay(
    explicit: Option<&Path>,
    events: mpsc::UnboundedSender<OverlayEvent>,
    expected_token: String,
    ui_state: Arc<parking_lot::Mutex<cue_core::overlay_ipc::OverlayUiState>>,
) -> Result<OverlayProcess> {
    // Step 1: resolve path. In production builds, env overrides require
    // BLUEY_DEV_OVERLAY=1 (handled by overlay::resolve_overlay_path).
    let resolved = if let Some(path) = explicit {
        path.to_path_buf()
    } else {
        let default = discover_overlay_bin()?;
        crate::overlay::resolve_overlay_path(&default)
    };

    // Step 2: verify the binary path is canonical + inside the install dir.
    // The install dir is the parent of the daemon's own current_exe (Tauri+helpers
    // ship side-by-side). For dev builds we allow any path under the cwd.
    let has_overlay_override = explicit.is_some()
        || env::var_os("BLUEY_OVERLAY_BIN").is_some()
        || env::var_os("CUE_OVERLAY_BIN").is_some();
    let install_dir = if cfg!(debug_assertions)
        || (crate::overlay::is_dev_overlay_enabled() && has_overlay_override)
    {
        env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
    } else {
        env::current_exe()
            .ok()
            .map(|p| p.canonicalize().unwrap_or(p))
            .and_then(|p| p.parent().map(PathBuf::from))
            .unwrap_or_else(|| PathBuf::from("/"))
    };
    if let Err(e) = crate::overlay::verify_overlay_binary(&resolved, &install_dir) {
        warn!(
            error = %e,
            binary = %resolved.display(),
            install_dir = %install_dir.display(),
            "overlay binary verification failed; refusing to spawn"
        );
        return Err(anyhow!("overlay binary verification failed: {e}"));
    }

    #[cfg(target_os = "macos")]
    if should_use_macos_socket_overlay(&resolved) {
        return spawn_macos_socket_overlay(resolved, events, expected_token, ui_state);
    }

    spawn_stdio_overlay(resolved, events, expected_token, ui_state)
}

fn spawn_stdio_overlay(
    resolved: PathBuf,
    events: mpsc::UnboundedSender<OverlayEvent>,
    expected_token: String,
    ui_state: Arc<parking_lot::Mutex<cue_core::overlay_ipc::OverlayUiState>>,
) -> Result<OverlayProcess> {
    let mut child = Command::new(&resolved)
        .env("BLUEY_OVERLAY_SESSION_TOKEN", &expected_token)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .with_context(|| format!("failed to spawn overlay {}", resolved.display()))?;

    let stdin = child.stdin.take().context("overlay stdin is not piped")?;

    if let Some(stdout) = child.stdout.take() {
        spawn_overlay_reader(stdout, events, expected_token, ui_state, None);
    }

    Ok(OverlayProcess {
        child,
        transport: OverlayTransport::Stdio(stdin),
    })
}

#[cfg(target_os = "macos")]
fn should_use_macos_socket_overlay(path: &Path) -> bool {
    if std::env::var("BLUEY_OVERLAY_FORCE_STDIO")
        .map(|v| matches!(v.as_str(), "1" | "true" | "yes" | "on"))
        .unwrap_or(false)
    {
        return false;
    }
    path.file_name()
        .and_then(|name| name.to_str())
        .map(|name| name == "bluey-overlay-macos" || name == "cue-overlay-macos")
        .unwrap_or(false)
}

#[cfg(target_os = "macos")]
fn spawn_macos_socket_overlay(
    resolved: PathBuf,
    events: mpsc::UnboundedSender<OverlayEvent>,
    expected_token: String,
    ui_state: Arc<parking_lot::Mutex<cue_core::overlay_ipc::OverlayUiState>>,
) -> Result<OverlayProcess> {
    use std::os::unix::net::UnixListener;

    let token_prefix = expected_token.get(..8).unwrap_or("notoken");
    let socket_path = std::env::temp_dir().join(format!(
        "bluey-overlay-{}-{token_prefix}.sock",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&socket_path);
    let listener = UnixListener::bind(&socket_path)
        .with_context(|| format!("failed to bind overlay socket {}", socket_path.display()))?;
    listener
        .set_nonblocking(true)
        .context("failed to set overlay socket nonblocking")?;

    let mut launch = macos_overlay_launch_command(&resolved, &socket_path, &expected_token);
    let mut child = launch
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn()
        .with_context(|| format!("failed to spawn overlay {}", resolved.display()))?;

    let deadline = Instant::now() + std::time::Duration::from_secs(3);
    let stream = loop {
        match listener.accept() {
            Ok((stream, _addr)) => break stream,
            Err(error) if error.kind() == ErrorKind::WouldBlock => {
                if let Some(status) = child
                    .try_wait()
                    .context("failed to poll overlay child during socket handshake")?
                {
                    let _ = std::fs::remove_file(&socket_path);
                    return Err(anyhow!("overlay exited before socket handshake: {status}"));
                }
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    let _ = std::fs::remove_file(&socket_path);
                    return Err(anyhow!("overlay did not connect to socket before timeout"));
                }
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = std::fs::remove_file(&socket_path);
                return Err(error).context("failed to accept overlay socket connection");
            }
        }
    };
    stream
        .set_nonblocking(false)
        .context("failed to set overlay socket blocking")?;
    let reader = stream
        .try_clone()
        .context("failed to clone overlay socket reader")?;
    spawn_overlay_reader(reader, events, expected_token, ui_state, Some(socket_path));

    Ok(OverlayProcess {
        child,
        transport: OverlayTransport::Socket(stream),
    })
}

#[cfg(target_os = "macos")]
fn macos_overlay_launch_command(
    resolved: &Path,
    socket_path: &Path,
    expected_token: &str,
) -> Command {
    if !macos_overlay_force_raw_helper() {
        if let Some(app_bundle) = macos_overlay_app_bundle_for_binary(resolved) {
            return macos_overlay_open_app_command(&app_bundle, socket_path, expected_token);
        }
    }

    let mut command = Command::new(resolved);
    command
        .env("BLUEY_OVERLAY_SESSION_TOKEN", expected_token)
        .env("BLUEY_OVERLAY_SOCKET", socket_path);
    command
}

#[cfg(target_os = "macos")]
fn macos_overlay_open_app_command(
    app_bundle: &Path,
    socket_path: &Path,
    expected_token: &str,
) -> Command {
    let mut command = Command::new("/usr/bin/open");
    command
        .arg("-n")
        .arg("-W")
        .arg(app_bundle)
        .arg("--args")
        .arg("--bluey-overlay-socket")
        .arg(socket_path)
        .arg("--bluey-overlay-session-token")
        .arg(expected_token);
    if macos_overlay_capture_visible_for_debug() {
        command.arg("--bluey-dev-overlay");
        command.arg("--bluey-overlay-capture-visible");
    }
    command
}

#[cfg(target_os = "macos")]
fn macos_overlay_force_raw_helper() -> bool {
    std::env::var("BLUEY_OVERLAY_FORCE_RAW_HELPER")
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

#[cfg(target_os = "macos")]
fn macos_overlay_app_bundle_for_binary(binary: &Path) -> Option<PathBuf> {
    let dir = binary.parent()?;
    let candidates = [
        dir.join("BlueyOverlay.app"),
        dir.join("bluey-overlay-macos.app"),
        dir.join("cue-overlay-macos.app"),
    ];
    candidates.into_iter().find(|candidate| candidate.exists())
}

#[cfg(target_os = "macos")]
fn macos_overlay_capture_visible_for_debug() -> bool {
    macos_overlay_capture_visible_allowed(
        env_truthy_any(&["BLUEY_DEV_OVERLAY"]),
        env_truthy_any(&[
            "BLUEY_OVERLAY_CAPTURE_VISIBLE",
            "BLUEY_HOST_OVERLAY_CAPTURE_VISIBLE",
        ]),
    )
}

#[cfg(target_os = "macos")]
fn macos_overlay_capture_visible_allowed(dev_gate: bool, capture_requested: bool) -> bool {
    dev_gate && capture_requested
}

fn spawn_overlay_reader<R>(
    reader: R,
    events: mpsc::UnboundedSender<OverlayEvent>,
    expected_token: String,
    ui_state: Arc<parking_lot::Mutex<cue_core::overlay_ipc::OverlayUiState>>,
    cleanup_path: Option<PathBuf>,
) where
    R: std::io::Read + Send + 'static,
{
    std::thread::spawn(move || {
        let token_for_reader = expected_token;
        let ui_state_for_reader = ui_state;
        let reader = std::io::BufReader::new(reader);
        for line in std::io::BufRead::lines(reader).map_while(Result::ok) {
            match validate_and_decode_overlay_line(&line, &token_for_reader, &ui_state_for_reader) {
                Ok(event) => {
                    info!("overlay event: {:?}", event);
                    let _ = events.send(event);
                }
                Err(OverlayLineReject::NotJson) => {
                    // Plain log line from overlay (non-event output).
                    info!("overlay: {line}");
                }
                Err(OverlayLineReject::TokenMismatch) => {
                    warn!("overlay event rejected: token mismatch");
                }
                Err(OverlayLineReject::FieldTooLong { field, len, max }) => {
                    warn!(
                        field = %field,
                        len, max,
                        "overlay event rejected: field exceeds max length"
                    );
                }
                Err(OverlayLineReject::StateNotAllowed { kind, state }) => {
                    warn!(
                        kind = %kind,
                        state = ?state,
                        "overlay event rejected: not allowed in current UI state"
                    );
                }
                Err(OverlayLineReject::ParseError(e)) => {
                    warn!(error = %e, "overlay event parse error; line dropped");
                }
            }
        }
        if let Some(path) = cleanup_path {
            let _ = std::fs::remove_file(path);
        }
        let _ = events.send(OverlayEvent::Exited);
    });
}

/// Reasons a line from the overlay child can be rejected before being forwarded.
#[derive(Debug)]
pub enum OverlayLineReject {
    NotJson,
    TokenMismatch,
    FieldTooLong {
        field: &'static str,
        len: usize,
        max: usize,
    },
    StateNotAllowed {
        kind: String,
        state: cue_core::overlay_ipc::OverlayUiState,
    },
    ParseError(String),
}

impl std::fmt::Display for OverlayLineReject {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

/// Per-field length caps for the production overlay path.
/// Mirrors `cue_core::overlay_ipc` limits.
const OVERLAY_MAX_QUESTION: usize = 4 * 1024;
const OVERLAY_MAX_INSTRUCTIONS: usize = 16 * 1024;
const OVERLAY_MAX_PATH: usize = 1024;
const OVERLAY_MAX_PATHS: usize = 16;
const OVERLAY_MAX_TEXT: usize = 64 * 1024;
const OVERLAY_MAX_LINE: usize = 128 * 1024;

pub fn validate_and_decode_overlay_line(
    line: &str,
    expected_token: &str,
    ui_state: &parking_lot::Mutex<cue_core::overlay_ipc::OverlayUiState>,
) -> Result<OverlayEvent, OverlayLineReject> {
    if line.len() > OVERLAY_MAX_LINE {
        return Err(OverlayLineReject::FieldTooLong {
            field: "<line>",
            len: line.len(),
            max: OVERLAY_MAX_LINE,
        });
    }
    let value: serde_json::Value =
        serde_json::from_str(line).map_err(|_| OverlayLineReject::NotJson)?;
    let obj = match value.as_object() {
        Some(o) => o,
        None => return Err(OverlayLineReject::NotJson),
    };

    // Token validation. If the daemon has a non-empty token, the event MUST
    // include a matching `token` field. Empty expected token means legacy mode.
    if !expected_token.is_empty() {
        let supplied = obj.get("token").and_then(|v| v.as_str()).unwrap_or("");
        if supplied != expected_token {
            return Err(OverlayLineReject::TokenMismatch);
        }
    }

    // Field length limits BEFORE state-machine check (cheaper to reject).
    if let Some(s) = obj.get("question").and_then(|v| v.as_str()) {
        if s.len() > OVERLAY_MAX_QUESTION {
            return Err(OverlayLineReject::FieldTooLong {
                field: "question",
                len: s.len(),
                max: OVERLAY_MAX_QUESTION,
            });
        }
    }
    if let Some(s) = obj.get("text").and_then(|v| v.as_str()) {
        if s.len() > OVERLAY_MAX_TEXT {
            return Err(OverlayLineReject::FieldTooLong {
                field: "text",
                len: s.len(),
                max: OVERLAY_MAX_TEXT,
            });
        }
    }
    if let Some(s) = obj.get("title").and_then(|v| v.as_str()) {
        if s.len() > OVERLAY_MAX_QUESTION {
            return Err(OverlayLineReject::FieldTooLong {
                field: "title",
                len: s.len(),
                max: OVERLAY_MAX_QUESTION,
            });
        }
    }
    if let Some(s) = obj.get("instructions").and_then(|v| v.as_str()) {
        if s.len() > OVERLAY_MAX_INSTRUCTIONS {
            return Err(OverlayLineReject::FieldTooLong {
                field: "instructions",
                len: s.len(),
                max: OVERLAY_MAX_INSTRUCTIONS,
            });
        }
    }
    if let Some(arr) = obj.get("paths").and_then(|v| v.as_array()) {
        if arr.len() > OVERLAY_MAX_PATHS {
            return Err(OverlayLineReject::FieldTooLong {
                field: "paths",
                len: arr.len(),
                max: OVERLAY_MAX_PATHS,
            });
        }
        for p in arr {
            if let Some(s) = p.as_str() {
                if s.len() > OVERLAY_MAX_PATH {
                    return Err(OverlayLineReject::FieldTooLong {
                        field: "paths[entry]",
                        len: s.len(),
                        max: OVERLAY_MAX_PATH,
                    });
                }
            }
        }
    }
    if let Some(s) = obj.get("error").and_then(|v| v.as_str()) {
        if s.len() > OVERLAY_MAX_QUESTION {
            return Err(OverlayLineReject::FieldTooLong {
                field: "error",
                len: s.len(),
                max: OVERLAY_MAX_QUESTION,
            });
        }
    }
    if let Some(s) = obj.get("stage").and_then(|v| v.as_str()) {
        if s.len() > OVERLAY_MAX_QUESTION {
            return Err(OverlayLineReject::FieldTooLong {
                field: "stage",
                len: s.len(),
                max: OVERLAY_MAX_QUESTION,
            });
        }
    }
    if let Some(s) = obj.get("status").and_then(|v| v.as_str()) {
        if s.len() > OVERLAY_MAX_QUESTION {
            return Err(OverlayLineReject::FieldTooLong {
                field: "status",
                len: s.len(),
                max: OVERLAY_MAX_QUESTION,
            });
        }
    }
    if let Some(s) = obj.get("detail").and_then(|v| v.as_str()) {
        if s.len() > OVERLAY_MAX_TEXT {
            return Err(OverlayLineReject::FieldTooLong {
                field: "detail",
                len: s.len(),
                max: OVERLAY_MAX_TEXT,
            });
        }
    }

    // Deserialize into typed event after stripping token (serde will ignore
    // unknown fields by default for #[serde(tag = "type", ...)] enums).
    let event: OverlayEvent =
        serde_json::from_value(value).map_err(|e| OverlayLineReject::ParseError(e.to_string()))?;

    // State-machine validation: certain events are only allowed in certain UI states.
    let kind_str: String = format!("{event:?}")
        .split_whitespace()
        .next()
        .unwrap_or("Unknown")
        .to_string();
    let current_state = *ui_state.lock();
    use cue_core::overlay_ipc::OverlayUiState as S;
    let allowed = match &event {
        // AttachRequested / InstructionsRequested are user-initiated *entry*
        // events: clicking "open attach" or "open instructions" from any state.
        // They drive the transition Idle -> AttachOpen / InstructionsOpen.
        // They MUST be accepted from Idle (otherwise the panels can never open).
        OverlayEvent::AttachRequested | OverlayEvent::InstructionsRequested => true,
        // AttachFilesRequested can come from the attach picker or a direct
        // drag-and-drop path from the native overlay.
        OverlayEvent::AttachFilesRequested { .. } => {
            current_state == S::Idle || current_state == S::AttachOpen
        }
        // InstructionsUpdated may come from the inline native overlay textbox.
        // Token validation and length caps still apply; no separate modal state
        // is required for this product flow.
        OverlayEvent::InstructionsUpdated { .. } => true,
        // All other events allowed in any state.
        _ => true,
    };
    if !allowed {
        return Err(OverlayLineReject::StateNotAllowed {
            kind: kind_str,
            state: current_state,
        });
    }

    Ok(event)
}

fn discover_overlay_bin() -> Result<PathBuf> {
    let cwd = env::current_dir()?;
    #[cfg(target_os = "macos")]
    {
        let mut candidates = Vec::new();
        if cfg!(debug_assertions) {
            candidates.extend([
                cwd.join("native/macos/cue-overlay/.build/bluey-overlay-macos"),
                cwd.join("native/macos/cue-overlay/.build/cue-overlay-macos"),
            ]);
        }
        if let Ok(exe) = env::current_exe() {
            let mut dirs = Vec::new();
            if let Some(dir) = exe.parent() {
                dirs.push(dir.to_path_buf());
            }
            if let Ok(canonical) = exe.canonicalize() {
                if let Some(dir) = canonical.parent() {
                    dirs.push(dir.to_path_buf());
                }
            }
            for dir in dirs {
                candidates.extend([
                    dir.join("bluey-overlay-macos"),
                    dir.join("cue-overlay-macos"),
                    dir.join("bin/bluey-overlay-macos"),
                    dir.join("bin/cue-overlay-macos"),
                ]);
            }
        }
        if !cfg!(debug_assertions) {
            candidates.extend([
                cwd.join("native/macos/cue-overlay/.build/bluey-overlay-macos"),
                cwd.join("native/macos/cue-overlay/.build/cue-overlay-macos"),
            ]);
        }
        candidates.extend([
            cwd.join("bluey-overlay-macos"),
            cwd.join("cue-overlay-macos"),
        ]);
        for candidate in candidates {
            if candidate.exists() {
                return Ok(candidate);
            }
        }
    }

    #[cfg(target_os = "windows")]
    {
        let mut candidates = Vec::new();
        if let Ok(exe) = env::current_exe() {
            let mut dirs = Vec::new();
            if let Some(dir) = exe.parent() {
                dirs.push(dir.to_path_buf());
            }
            if let Ok(canonical) = exe.canonicalize() {
                if let Some(dir) = canonical.parent() {
                    dirs.push(dir.to_path_buf());
                }
            }
            for dir in dirs {
                candidates.extend([
                    dir.join("bluey-overlay.exe"),
                    dir.join("cue-overlay.exe"),
                    dir.join("bin/bluey-overlay.exe"),
                    dir.join("bin/cue-overlay.exe"),
                ]);
            }
        }
        candidates.extend([
            cwd.join("native/windows/cue-overlay/build/bluey-overlay.exe"),
            cwd.join("native/windows/cue-overlay/build/cue-overlay.exe"),
            cwd.join("bluey-overlay.exe"),
            cwd.join("cue-overlay.exe"),
        ]);
        for candidate in candidates {
            if candidate.exists() {
                return Ok(candidate);
            }
        }
    }

    Err(anyhow!(
        "native overlay binary not found; build it or set BLUEY_OVERLAY_BIN"
    ))
}

fn build_context_artifact(
    paths: &AppPaths,
    path: String,
    title: Option<String>,
    note: Option<String>,
) -> Result<ContextArtifact> {
    let raw_path = PathBuf::from(path);
    let canonical_path = raw_path
        .canonicalize()
        .with_context(|| format!("failed to resolve context path {}", raw_path.display()))?;
    let metadata = std::fs::metadata(&canonical_path).with_context(|| {
        format!(
            "failed to read context metadata {}",
            canonical_path.display()
        )
    })?;
    if !metadata.is_file() {
        return Err(anyhow!(
            "context path must be a file: {}",
            canonical_path.display()
        ));
    }

    let kind = classify_context_path(&canonical_path);
    let title = title
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| {
            canonical_path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("attached context")
                .to_string()
        });

    let artifact = ContextArtifact::new(
        kind,
        canonical_path.display().to_string(),
        title,
        note.filter(|value| !value.trim().is_empty()),
        Some(metadata.len()),
    )
    .with_processing_status(ContextProcessingStatus::Pending);

    let artifact = enrich_context_artifact(paths, artifact, &canonical_path, kind, metadata.len());
    validate_context_artifact(&artifact)?;
    Ok(artifact)
}

fn validate_context_artifact(artifact: &ContextArtifact) -> Result<()> {
    match artifact.processing_status {
        ContextProcessingStatus::Ready | ContextProcessingStatus::Pending => Ok(()),
        ContextProcessingStatus::Unsupported | ContextProcessingStatus::Failed => {
            let detail = artifact
                .processing_error
                .as_deref()
                .filter(|error| !error.trim().is_empty())
                .unwrap_or("Bluey could not read this file as useful session context");
            Err(anyhow!(
                "{} cannot be attached yet ({}): {}",
                artifact.title,
                artifact.processing_status,
                detail
            ))
        }
    }
}

async fn choose_context_files() -> Result<Vec<PathBuf>> {
    tokio::task::spawn_blocking(choose_context_files_platform)
        .await
        .context("file picker task failed")?
}

#[cfg(target_os = "macos")]
fn choose_context_files_platform() -> Result<Vec<PathBuf>> {
    if let Some(picker_app) = discover_macos_context_picker_app() {
        match run_context_picker_app(&picker_app) {
            Ok(paths) => return Ok(paths),
            Err(error) => warn!(
                picker = %picker_app.display(),
                "native macOS context picker app failed, falling back to AppleScript: {error:#}"
            ),
        }
    }

    let script = r#"
try
  set allowedTypes to {"public.text", "public.source-code", "public.shell-script", "public.json", "public.yaml", "public.xml", "public.html", "public.css", "com.adobe.pdf", "com.microsoft.word.doc", "org.openxmlformats.wordprocessingml.document", "public.rtf", "net.daringfireball.markdown", "md", "markdown", "txt", "log", "csv", "tsv", "rst", "adoc", "rs", "swift", "c", "h", "cpp", "hpp", "js", "jsx", "ts", "tsx", "py", "go", "java", "kt", "kts", "cs", "rb", "php", "sql", "sh", "ps1", "toml", "yaml", "yml", "json", "html", "css", "scss", "pdf", "doc", "docx", "rtf"}
  set pickedFiles to choose file with prompt "Choose readable text, code, PDF, DOC/DOCX, CSV/TSV, JSON/YAML/TOML, HTML/CSS, shell/SQL, or RTF files for this Bluey session. Video, audio, apps, and certificates are skipped." of type allowedTypes with multiple selections allowed
  set output to ""
  repeat with pickedFile in pickedFiles
    set output to output & POSIX path of pickedFile & linefeed
  end repeat
  return output
on error number -128
  return ""
end try
"#;
    let output = Command::new("osascript")
        .arg("-e")
        .arg(script)
        .output()
        .context("failed to launch macOS file picker")?;
    if !output.status.success() {
        return Ok(Vec::new());
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(stdout
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(PathBuf::from)
        .collect())
}

#[cfg(target_os = "macos")]
fn discover_macos_context_picker_app() -> Option<PathBuf> {
    if let Ok(value) = env::var("BLUEY_CONTEXT_PICKER_APP") {
        let path = PathBuf::from(value);
        if path.is_dir() {
            return Some(path);
        }
    }

    let mut candidates = Vec::new();
    if let Ok(exe) = env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.extend([
                dir.join("BlueyFilePicker.app"),
                dir.join("bin/BlueyFilePicker.app"),
            ]);
        }
    }
    if let Ok(cwd) = env::current_dir() {
        candidates.extend([
            cwd.join("native/macos/cue-picker/.build/BlueyFilePicker.app"),
            cwd.join("target/debug/BlueyFilePicker.app"),
            cwd.join("target/release/BlueyFilePicker.app"),
        ]);
    }

    candidates.into_iter().find(|path| path.is_dir())
}

#[cfg(target_os = "macos")]
fn run_context_picker_app(picker_app: &Path) -> Result<Vec<PathBuf>> {
    let output_path = env::temp_dir().join(format!(
        "bluey-context-picker-{}-{}.txt",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
    ));
    let output = Command::new("open")
        .arg("-W")
        .arg("-n")
        .arg(picker_app)
        .arg("--args")
        .arg("--output")
        .arg(&output_path)
        .output()
        .with_context(|| format!("failed to launch {}", picker_app.display()))?;
    if !output.status.success() {
        return Err(anyhow!(
            "{} exited with status {}",
            picker_app.display(),
            output.status
        ));
    }
    let selected = std::fs::read_to_string(&output_path).unwrap_or_default();
    let _ = std::fs::remove_file(&output_path);
    Ok(selected
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(PathBuf::from)
        .collect())
}

#[cfg(not(target_os = "macos"))]
#[cfg(not(target_os = "windows"))]
fn choose_context_files_platform() -> Result<Vec<PathBuf>> {
    Ok(Vec::new())
}

#[cfg(target_os = "windows")]
fn choose_context_files_platform() -> Result<Vec<PathBuf>> {
    let script = r#"
	Add-Type -AssemblyName System.Windows.Forms
	$dialog = New-Object System.Windows.Forms.OpenFileDialog
	$dialog.Title = "Choose readable files for this Bluey session"
	$dialog.Filter = "Bluey context files|*.md;*.markdown;*.txt;*.log;*.csv;*.tsv;*.rst;*.adoc;*.rs;*.swift;*.c;*.h;*.cpp;*.hpp;*.js;*.jsx;*.ts;*.tsx;*.py;*.go;*.java;*.kt;*.kts;*.cs;*.rb;*.php;*.sql;*.sh;*.ps1;*.toml;*.yaml;*.yml;*.json;*.html;*.css;*.scss;*.pdf;*.doc;*.docx;*.rtf"
	$dialog.Multiselect = $true
if ($dialog.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) {
  $dialog.FileNames -join "`n"
}
"#;
    let output = Command::new("powershell")
        .arg("-NoProfile")
        .arg("-STA")
        .arg("-ExecutionPolicy")
        .arg("Bypass")
        .arg("-Command")
        .arg(script)
        .output()
        .context("failed to launch Windows file picker")?;
    if !output.status.success() {
        return Ok(Vec::new());
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(PathBuf::from)
        .collect())
}

async fn prompt_answer_instructions(current: String) -> Result<Option<String>> {
    tokio::task::spawn_blocking(move || prompt_answer_instructions_platform(&current))
        .await
        .context("answer instructions prompt task failed")?
}

#[cfg(target_os = "macos")]
fn prompt_answer_instructions_platform(current: &str) -> Result<Option<String>> {
    let escaped = current.replace('\\', "\\\\").replace('"', "\\\"");
    let script = format!(
        r#"
try
  set dialogResult to display dialog "How should Bluey answer questions in this session?" default answer "{}" buttons {{"Cancel", "Save"}} default button "Save" with title "Bluey Answer Style"
  return text returned of dialogResult
on error number -128
  return ""
end try
"#,
        escaped
    );
    let output = Command::new("osascript")
        .arg("-e")
        .arg(script)
        .output()
        .context("failed to launch answer instructions prompt")?;
    if !output.status.success() {
        return Ok(None);
    }
    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Ok(Some(text))
}

#[cfg(not(target_os = "macos"))]
#[cfg(not(target_os = "windows"))]
fn prompt_answer_instructions_platform(_current: &str) -> Result<Option<String>> {
    Ok(None)
}

#[cfg(target_os = "windows")]
fn prompt_answer_instructions_platform(current: &str) -> Result<Option<String>> {
    let escaped_current = current.replace('\'', "''");
    let script = format!(
        r#"
Add-Type -AssemblyName Microsoft.VisualBasic
[Microsoft.VisualBasic.Interaction]::InputBox(
  'Tell Bluey how to answer during this session.',
  'Bluey Answer Style',
  '{escaped_current}'
)
"#
    );
    let output = Command::new("powershell")
        .arg("-NoProfile")
        .arg("-STA")
        .arg("-ExecutionPolicy")
        .arg("Bypass")
        .arg("-Command")
        .arg(script)
        .output()
        .context("failed to launch Windows answer instructions prompt")?;
    if !output.status.success() {
        return Ok(None);
    }
    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Ok(Some(text))
}

#[cfg(target_os = "windows")]
fn powershell_single_quoted(path: &Path) -> String {
    format!("'{}'", path.display().to_string().replace('\'', "''"))
}

fn enrich_context_artifact(
    paths: &AppPaths,
    artifact: ContextArtifact,
    path: &Path,
    kind: ContextKind,
    size_bytes: u64,
) -> ContextArtifact {
    match kind {
        ContextKind::Code | ContextKind::Text | ContextKind::Document => {
            match convert_context_file_to_markdown(path, kind, size_bytes) {
                Ok(markdown) => {
                    let preview = build_markdown_preview(&markdown);
                    if preview.is_empty() {
                        artifact.with_processing_error("file did not contain readable text")
                    } else {
                        match write_markdown_artifact(&paths.data_dir, artifact.id, &markdown) {
                            Ok(markdown_path) => artifact
                                .with_text_preview(preview)
                                .with_markdown_path(markdown_path.display().to_string()),
                            Err(error) => artifact.with_processing_error(format!(
                                "converted Markdown but could not save local copy: {error:#}"
                            )),
                        }
                    }
                }
                Err(error) => artifact.with_processing_error(format!("{error:#}")),
            }
        }
        ContextKind::Image | ContextKind::Diagram => {
            if vision_context_available_from_env() {
                artifact.with_processing_status(ContextProcessingStatus::Pending)
            } else {
                artifact.with_unsupported_error(
                    "image context needs a configured OCR/vision provider before answers can use it",
                )
            }
        }
        ContextKind::Other => artifact.with_unsupported_error(
            "unsupported context file type; attach readable text, Markdown, code, PDF, DOC, or DOCX",
        ),
    }
}

fn vision_context_available_from_env() -> bool {
    if dev_direct_vision_enabled() && ai_status_from_env().vision_enabled {
        return true;
    }
    AppPaths::discover()
        .map(|paths| cloud_account_linked(&paths))
        .unwrap_or_else(|_| cloud_access_token_from_env().is_some())
}

#[cfg(target_os = "macos")]
fn active_page_javascript() -> &'static str {
    r#"(() => {
const selectors = [
  '[data-cy="question-title"]',
  '[data-cy="description-content"]',
  '[data-track-load="description_content"]',
  '[data-track-load*="description"]',
  '[class*="question-title"]',
  '.question-content',
  '.question-detail',
  '.description__24sA',
  '[class*="question-content"]',
  '[class*="description"]',
  '[role="main"]',
  'main',
  'article'
];
const normalize = (value) => (value || '')
  .replace(/\u00a0/g, ' ')
  .replace(/[ \t]+\n/g, '\n')
  .replace(/\n{3,}/g, '\n\n')
  .trim();
const seen = new Set();
const chunks = [];
for (const selector of selectors) {
  for (const node of document.querySelectorAll(selector)) {
    const text = normalize(node.innerText || node.textContent || '');
    if (text.length < 8 || seen.has(text)) continue;
    seen.add(text);
    chunks.push(text);
  }
}
let text = normalize(chunks.join('\n\n'));
if (text.length < 200) {
  const bodyText = normalize((document.body && document.body.innerText) || '');
  if (bodyText.length > text.length) text = bodyText;
}
return JSON.stringify({
  title: document.title || '',
  url: location.href || '',
  text
});
})()"#
}

fn normalize_page_text(text: &str, max_chars: usize) -> String {
    let mut normalized = String::new();
    let mut previous_blank = false;

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            if !previous_blank && !normalized.is_empty() {
                normalized.push('\n');
            }
            previous_blank = true;
        } else {
            if !normalized.is_empty() && !normalized.ends_with('\n') {
                normalized.push('\n');
            }
            normalized.push_str(line);
            previous_blank = false;
        }

        if normalized.chars().count() >= max_chars {
            break;
        }
    }

    normalized.chars().take(max_chars).collect()
}

fn sanitize_file_stem(raw: &str) -> String {
    let mut stem = String::new();
    let mut previous_dash = false;

    for ch in raw.chars() {
        let next = if ch.is_ascii_alphanumeric() {
            previous_dash = false;
            ch.to_ascii_lowercase()
        } else if !previous_dash {
            previous_dash = true;
            '-'
        } else {
            continue;
        };

        stem.push(next);
        if stem.len() >= 72 {
            break;
        }
    }

    let stem = stem.trim_matches('-');
    if stem.is_empty() {
        "active-page".to_string()
    } else {
        stem.to_string()
    }
}

#[cfg(target_os = "macos")]
fn apple_script_string(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

fn epoch_ms() -> Result<u128> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock is before Unix epoch")?
        .as_millis())
}

fn state_from_active_meeting(active: Option<&MeetingRecord>) -> DaemonState {
    let mut state = DaemonState::new(std::process::id());
    if let Some(meeting) = active {
        state.meeting = MeetingState::InMeeting {
            id: meeting.id.to_string(),
            title: Some(meeting.title.clone()),
            started_at: meeting.started_at.clone(),
        };
        state.transcript_segments = meeting.transcript.len();
        state.context_items = meeting.context.len();
        state.answer_instructions_set = meeting
            .answer_instructions
            .as_ref()
            .is_some_and(|value| !value.trim().is_empty());
        state.action_items = meeting.action_items.len();
        state.decisions = meeting.decisions.len();
    }
    state
}

async fn update_state_from_meeting(
    daemon: &Arc<Daemon>,
    meeting: Option<&MeetingRecord>,
) -> Result<()> {
    {
        let mut state = daemon.state.lock().await;
        if let Some(meeting) = meeting {
            state.meeting = MeetingState::InMeeting {
                id: meeting.id.to_string(),
                title: Some(meeting.title.clone()),
                started_at: meeting.started_at.clone(),
            };
            state.transcript_segments = meeting.transcript.len();
            state.context_items = meeting.context.len();
            state.answer_instructions_set = meeting
                .answer_instructions
                .as_ref()
                .is_some_and(|value| !value.trim().is_empty());
            state.action_items = meeting.action_items.len();
            state.decisions = meeting.decisions.len();
        } else {
            state.meeting = MeetingState::Idle;
            state.transcript_segments = 0;
            state.context_items = 0;
            state.answer_instructions_set = false;
            state.action_items = 0;
            state.decisions = 0;
        }
    }
    write_state(daemon).await
}

/// Spawn a best-effort auto-recap via LLM after a session ends.
/// If no LLM provider is configured, logs a warning and returns.
fn spawn_auto_recap(daemon: &Arc<Daemon>, meeting: &MeetingRecord) {
    let transcript: String = meeting
        .transcript
        .iter()
        .map(|s| s.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    if transcript.trim().is_empty() {
        return;
    }
    let session_id = meeting.id.to_string();
    let db_dir = daemon.paths.data_dir.clone();
    tokio::spawn(async move {
        let llm = match build_recap_llm_from_env() {
            Some(p) => p,
            None => {
                warn!("auto-recap skipped: no LLM provider configured");
                return;
            }
        };
        let result = crate::llm::RecapLlm
            .run(&transcript, &session_id, llm.as_ref())
            .await;
        match result {
            Ok(resp) => {
                let db_path = db_dir.join("sessions.db");
                if let Ok(db) = crate::db::Database::open(db_path.to_str().unwrap_or("sessions.db"))
                {
                    if let Err(e) = db.insert_cue_response(crate::db::NewCueResponse {
                        id: &resp.id,
                        session_id: &resp.source_session_id,
                        kind: &resp.kind,
                        text: &resp.text,
                        source_text: resp.source_text.as_deref(),
                        ts_ms: resp.ts_ms as i64,
                        cost_cents: resp.cost_cents,
                        balance_cents_after: resp.balance_cents_after,
                        provider: resp.provider.as_deref(),
                        model: resp.model.as_deref(),
                        input_tokens: resp.input_tokens,
                        output_tokens: resp.output_tokens,
                        cost_label: resp.cost_label.as_deref(),
                        artifact_type: resp.artifact_type.as_deref(),
                        artifact_body: resp.artifact_body.as_deref(),
                        artifact_confidence: resp.artifact_confidence,
                    }) {
                        warn!(error = %e, "auto-recap: failed to persist");
                    } else {
                        info!(session_id = %session_id, "auto-recap persisted");
                    }
                }
            }
            Err(e) => {
                warn!(error = %e, "auto-recap LLM call failed");
            }
        }
    });
}

/// Build an LLM provider from env for auto-recap (best-effort).
fn build_recap_llm_from_env() -> Option<Box<dyn cue_llm::LlmProvider>> {
    if !dev_env_truthy_any(&["BLUEY_DEV_BYOK", "BLUEY_DEV_DIRECT_PROVIDERS"]) {
        return None;
    }
    let key = std::env::var("OPENAI_API_KEY")
        .ok()
        .filter(|k| !k.is_empty())
        .or_else(|| crate::secrets::load_api_key("llm_openai").ok().flatten())?;
    Some(Box::new(cue_llm::openai::OpenAiProvider::new(key)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_messages_include_image_parts_when_route_allows_upload() {
        let path = env::temp_dir().join(format!(
            "bluey-vision-payload-test-{}.png",
            std::process::id()
        ));
        std::fs::write(&path, [0x89, b'P', b'N', b'G']).expect("write test image");

        let route = ProviderRoute::direct(ProviderSelector::openai("gpt-4.1-mini"))
            .with_privacy(PrivacyFlags::managed_commercial().with_image_upload());
        let mut request = AnswerRequest::new("What is on screen?", route);
        request.context.push(
            AnswerContext::new(AnswerContextKind::Screenshot, "screen capture fallback")
                .with_title("Screen capture fallback")
                .with_source(path.display().to_string()),
        );

        let payload = ProviderRequestPayload::from_request(
            &request,
            ProviderSelector::openai("gpt-4.1-mini"),
            Some("https://api.openai.com/v1/chat/completions".to_string()),
            "fallback",
            RouteBudget::realtime(),
        );

        let messages = provider_messages(&payload).expect("build provider messages");
        let json = serde_json::to_string(&messages).expect("serialize messages");
        assert!(json.contains(r#""type":"image_url""#));
        assert!(json.contains("data:image/png;base64,"));

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn cloud_client_with_optional_trace_attaches_sanitized_trace_id() {
        let client = cue_cloud_client::CloudClient::new(
            cue_cloud_client::client::ClientConfig::default(),
            Arc::new(cue_cloud_client::tokens::MemoryStore::new()),
        )
        .expect("client");

        let traced = cloud_client_with_optional_trace(client, Some("trace-123"));

        assert_eq!(traced.config.trace_id.as_deref(), Some("trace-123"));
    }

    #[test]
    fn cloud_client_with_optional_trace_rejects_invalid_trace_id() {
        let client = cue_cloud_client::CloudClient::new(
            cue_cloud_client::client::ClientConfig::default(),
            Arc::new(cue_cloud_client::tokens::MemoryStore::new()),
        )
        .expect("client");

        let traced = cloud_client_with_optional_trace(client, Some("bad\ntrace"));

        assert_eq!(traced.config.trace_id, None);
    }

    #[test]
    fn answer_context_includes_compacted_session_summary() {
        let mut meeting = MeetingRecord::new(Some("System design prep".to_string()));
        meeting.summary = Some(
            "We established the cache invalidation strategy and the user prefers concise tradeoffs."
                .to_string(),
        );

        let context = answer_context_from_meeting(&meeting);

        let summary = context
            .iter()
            .find(|item| item.source.as_deref() == Some("saved session summary"))
            .expect("summary context");
        assert_eq!(summary.kind, AnswerContextKind::MeetingMemory);
        assert_eq!(summary.title.as_deref(), Some("System design prep summary"));
        assert!(summary.content.contains("cache invalidation strategy"));
        assert!(summary.content.contains("concise tradeoffs"));
    }

    #[test]
    fn provider_messages_include_overlay_friendly_answer_shape() {
        let route = ProviderRoute::direct(ProviderSelector::openai("gpt-4.1-mini"));
        let request = AnswerRequest::new("Solve this coding question", route);
        let payload = ProviderRequestPayload::from_request(
            &request,
            ProviderSelector::openai("gpt-4.1-mini"),
            Some("https://api.openai.com/v1/chat/completions".to_string()),
            "fallback",
            RouteBudget::realtime(),
        );

        let messages = provider_messages(&payload).expect("build provider messages");
        let system = match &messages[0].content {
            ChatMessageContent::Text(text) => text,
            ChatMessageContent::Parts(_) => panic!("system message should be text"),
        };

        assert!(system.contains("Human-speak contract"));
        assert!(system.contains("first person"));
        assert!(system.contains("Do not invent personal experience"));
        assert!(system.contains("No assistant preamble"));
        assert!(system.contains("Do not sound like a polished memo"));
        assert!(system.contains("concise rationale"));
        assert!(system.contains("Output format"));
        assert!(system.contains("direct, speakable answer first"));
        assert!(system.contains("Do not turn normal chat answers into a markdown outline"));
        assert!(system.contains("full updated implementation"));
        assert!(system.contains("updated whole architecture"));
        assert!(system.contains("Approach, Code, Explanation, Complexity, Edge cases"));
        assert!(system.contains("fenced Markdown code blocks"));
    }

    #[test]
    fn overlay_question_cards_are_titled_question() {
        let (title, body) = visible_question_for_source("What changed?", "overlay ask");

        assert_eq!(title, "Question");
        assert_eq!(body, "What changed?");
    }

    #[test]
    fn overlay_screen_analysis_cards_keep_specific_title() {
        let (title, body) = visible_question_for_source("Analyse everything", "overlay analyse");

        assert_eq!(title, "Analyse Screen");
        assert_eq!(body, "Analyse the current browser page or screen context.");
    }

    #[test]
    fn mode_instructions_specialize_default_answer_shapes() {
        let code = mode_instructions("Code");
        let design = mode_instructions("System Design");
        let meeting = mode_instructions("Meeting");

        assert!(code.contains("### Code"));
        assert!(code.contains("fenced code block"));
        assert!(design.contains("### Architecture"));
        assert!(design.contains("failure modes"));
        assert!(meeting.contains("### Action items"));
    }

    #[test]
    fn general_mode_keeps_code_shape_for_coding_questions() {
        let general = mode_instructions("General");

        assert!(general.contains("Auto-detect the task type"));
        assert!(general.contains("### Code"));
        assert!(general.contains("fenced code blocks"));
    }

    #[test]
    fn url_component_escapes_audio_request_ids() {
        assert_eq!(
            url_component("audio system/1 + model"),
            "audio%20system%2F1%20%2B%20model"
        );
    }

    #[test]
    fn stt_relay_websocket_url_converts_http_and_appends_token() {
        let url =
            stt_relay_websocket_url("https://bluey.sh/stt/relay", "tok /1").expect("websocket URL");

        assert_eq!(url, "wss://bluey.sh/stt/relay?session_token=tok%20%2F1");
    }

    #[test]
    fn stt_relay_websocket_url_preserves_existing_query() {
        let url = stt_relay_websocket_url("http://127.0.0.1:8787/stt/relay?debug=1", "tok")
            .expect("websocket URL");

        assert_eq!(
            url,
            "ws://127.0.0.1:8787/stt/relay?debug=1&session_token=tok"
        );
    }

    #[test]
    fn pcm16_16k_duration_ms_tracks_byte_length() {
        assert_eq!(pcm16_16k_duration_ms(3_200), 100);
        assert_eq!(pcm16_16k_duration_ms(0), 1);
    }

    #[test]
    fn native_helper_pcm_is_wrapped_as_16k_i16_wav_without_resampling() {
        let raw = [0x34, 0x12, 0x78, 0x56, 0xff];
        let wav = wav_from_i16le_16k_mono(&raw);

        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        assert_eq!(&wav[12..16], b"fmt ");
        assert_eq!(u16::from_le_bytes([wav[20], wav[21]]), 1);
        assert_eq!(u16::from_le_bytes([wav[22], wav[23]]), 1);
        assert_eq!(
            u32::from_le_bytes([wav[24], wav[25], wav[26], wav[27]]),
            16_000
        );
        assert_eq!(
            u32::from_le_bytes([wav[28], wav[29], wav[30], wav[31]]),
            32_000
        );
        assert_eq!(u16::from_le_bytes([wav[34], wav[35]]), 16);
        assert_eq!(&wav[36..40], b"data");
        assert_eq!(u32::from_le_bytes([wav[40], wav[41], wav[42], wav[43]]), 4);
        assert_eq!(&wav[44..], &[0x34, 0x12, 0x78, 0x56]);
    }

    #[test]
    fn failed_audio_status_does_not_mark_backend_ready() {
        let status = failed_audio_status(
            AudioCaptureConfig::dual_default(),
            "Sign in before using speech-to-text.",
        );

        assert_eq!(status.runtime_mode, AudioRuntimeMode::Unavailable);
        assert!(!status.backend_ready);
        assert_eq!(status.stt_provider, None);
        assert!(status.session_id.is_none());
        assert_eq!(status.capture.state, cue_core::AudioCaptureState::Failed);
        assert_eq!(
            status.capture.last_error.as_deref(),
            Some("Sign in before using speech-to-text.")
        );
    }

    #[test]
    fn recording_label_never_describes_unavailable_audio_as_preview() {
        let status = failed_audio_status(AudioCaptureConfig::dual_default(), "missing setup");

        assert_eq!(
            recording_sources_label(&status),
            "Audio is not available yet. Check permissions or provider setup."
        );
    }

    #[test]
    fn overlay_auto_uses_managed_route_fallbacks() {
        let request = answer_request_from_overlay(
            "Solve this in Rust",
            Some("auto".to_string()),
            None,
            Some("General".to_string()),
        );

        assert_eq!(
            request.route.primary.provider.provider_kind,
            AiProviderKind::CueManaged
        );
        assert_eq!(request.route.primary.provider.model_or(""), "balanced");
        assert!(request.route.fallbacks.is_empty());
        assert!(request
            .instructions
            .as_deref()
            .is_some_and(|instructions| instructions.contains("### Code")));
    }

    #[test]
    fn overlay_raw_provider_without_dev_flag_uses_managed_lane() {
        let request = answer_request_from_overlay(
            "What changed?",
            Some("openai".to_string()),
            Some("managed-reasoning".to_string()),
            Some("General".to_string()),
        );

        assert_eq!(
            request.route.primary.provider.provider_kind,
            AiProviderKind::CueManaged
        );
        assert_eq!(request.route.primary.provider.model_or(""), "balanced");
    }

    #[test]
    fn overlay_model_menu_maps_to_managed_lanes() {
        let instant = answer_request_from_overlay(
            "Quick answer",
            Some("managed".to_string()),
            Some("instant".to_string()),
            Some("instant".to_string()),
        );
        assert_eq!(
            instant.route.primary.provider.provider_kind,
            AiProviderKind::CueManaged
        );
        assert_eq!(instant.route.primary.provider.model_or(""), "instant");

        let deep = answer_request_from_overlay(
            "Design this deeply",
            Some("managed".to_string()),
            Some("deep".to_string()),
            Some("deep".to_string()),
        );
        assert_eq!(
            deep.route.primary.provider.provider_kind,
            AiProviderKind::CueManaged
        );
        assert_eq!(deep.route.primary.provider.model_or(""), "deep");
    }

    #[test]
    fn answer_instructions_merge_mode_and_session_rules() {
        let merged = merge_answer_instructions(
            Some(mode_instructions("Code")),
            Some("Be concise and mention risks.".to_string()),
        )
        .expect("merged instructions");

        assert!(merged.contains("Mode / request instructions"));
        assert!(merged.contains("### Code"));
        assert!(merged.contains("Session answer rules"));
        assert!(merged.contains("Be concise"));
    }

    #[test]
    fn streaming_word_chunks_preserve_spacing() {
        let chunks = streaming_word_chunks("one two\nthree");
        assert_eq!(chunks, vec!["one ", "two\n", "three"]);
    }

    #[test]
    fn answer_overlay_cost_label_includes_usage_and_latency() {
        let metadata = AnswerResponseMetadata::new(
            uuid::Uuid::new_v4(),
            ProviderSelector::openai("gpt-4o-mini"),
        )
        .with_usage(TokenUsage {
            input_tokens: 123,
            output_tokens: 45,
            total_tokens: 168,
        })
        .with_latency(812);

        assert_eq!(
            answer_overlay_cost_label(&metadata),
            Some("123 in / 45 out · 812 ms".to_string())
        );
    }

    #[test]
    fn answer_overlay_artifact_detects_fenced_code() {
        let artifact = answer_overlay_artifact(
            "Use a hash map.\n```rust\nfn solve() -> i32 { 42 }\n```\nTime Complexity: O(n)",
        )
        .expect("code artifact");

        assert_eq!(artifact.artifact_type, CardArtifactType::Code);
        assert!(artifact.body.contains("CODE\n----"));
        assert!(artifact.body.contains("fn solve()"));
        assert!(artifact.body.contains("NOTES\n-----"));
    }

    #[test]
    fn answer_overlay_artifact_detects_system_design() {
        let artifact = answer_overlay_artifact(
            "For this system design, use an API gateway, cache, queue, database, and load balancer to improve latency and scale.",
        )
        .expect("system design artifact");

        assert_eq!(artifact.artifact_type, CardArtifactType::SystemDesign);
        assert!(artifact.confidence > 0.8);
    }

    #[test]
    fn answer_overlay_artifact_ignores_short_chat() {
        assert!(answer_overlay_artifact("Yes, that is the right next step.").is_none());
    }

    #[test]
    fn overlay_lifecycle_event_is_accepted_by_production_validator() {
        let state = parking_lot::Mutex::new(cue_core::overlay_ipc::OverlayUiState::Idle);
        let event = validate_and_decode_overlay_line(
            r#"{"type":"lifecycle","token":"tok","stage":"started","status":"ok","detail":"capture_excluded=true"}"#,
            "tok",
            &state,
        )
        .expect("lifecycle event should decode");

        match event {
            OverlayEvent::Lifecycle {
                stage,
                status,
                detail,
            } => {
                assert_eq!(stage, "started");
                assert_eq!(status.as_deref(), Some("ok"));
                assert_eq!(detail.as_deref(), Some("capture_excluded=true"));
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn macos_overlay_capture_visible_requires_dev_gate() {
        assert!(!macos_overlay_capture_visible_allowed(false, false));
        assert!(!macos_overlay_capture_visible_allowed(false, true));
        assert!(!macos_overlay_capture_visible_allowed(true, false));
        assert!(macos_overlay_capture_visible_allowed(true, true));
    }

    #[test]
    fn duplicate_transcript_detection_ignores_recent_retries() {
        let mut meeting = MeetingRecord::new(Some("Audio".to_string()));
        meeting.transcript.push(TranscriptSegment::new(
            Speaker::System,
            "We should cache the answer.",
            true,
        ));

        assert!(is_near_duplicate_transcript(
            &meeting,
            Speaker::System,
            "we should   cache the answer.",
            true,
        ));
        assert!(!is_near_duplicate_transcript(
            &meeting,
            Speaker::User,
            "we should cache the answer.",
            true,
        ));
        assert!(!is_near_duplicate_transcript(
            &meeting,
            Speaker::System,
            "we should cache a different answer.",
            true,
        ));
    }

    #[test]
    fn duration_labels_are_human_readable_for_idle_guard() {
        assert_eq!(format_duration(Duration::from_secs(1)), "1 second");
        assert_eq!(format_duration(Duration::from_secs(59)), "59 seconds");
        assert_eq!(format_duration(Duration::from_secs(60)), "1 minute");
        assert_eq!(
            format_duration(Duration::from_secs(301)),
            "5 minutes 1 second"
        );
    }

    #[test]
    fn balance_labels_are_dollar_amounts() {
        assert_eq!(format_balance_cents(0), "$0.00");
        assert_eq!(format_balance_cents(1234), "$12.34");
        assert_eq!(format_balance_cents(-75), "-$0.75");
    }

    #[test]
    fn provider_context_compacts_large_items() {
        let mut items = Vec::new();
        let long_doc = "alpha\n".repeat(2_000);
        push_provider_context_item(
            &mut items,
            AnswerContextKind::Document,
            "Large doc",
            "test",
            &long_doc,
        );
        let context = compact_provider_context(&items);

        assert!(context.contains("[Large doc from test]"));
        assert!(context.contains("[compacted]"));
        assert!(context.chars().count() < long_doc.chars().count());
    }
}
