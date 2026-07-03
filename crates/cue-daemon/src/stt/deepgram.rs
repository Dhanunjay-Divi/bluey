//! Deepgram Nova-3 streaming STT provider.
//!
//! Implements [`SttProvider`] against Deepgram's real-time WebSocket API.
//! One `DeepgramProvider` == one persistent `wss://api.deepgram.com/v1/listen`
//! connection supervised by a background task that handles reconnects.
//!
//! ### Connection lifecycle
//!
//! 1. [`DeepgramProvider::connect`] builds the URL (model / encoding /
//!    sample_rate / language params), constructs an HTTP upgrade request,
//!    attaches the `Authorization: Token <key>` header, and spawns the
//!    supervisor task.
//! 2. The supervisor repeatedly calls `run_connection` which opens the WS,
//!    selects over (a) outbound audio chunks and (b) inbound JSON frames.
//!    Audio bytes go out as `Message::Binary`; frames come in as
//!    `Message::Text` and are pushed to the events channel after parsing.
//! 3. On transient failure (`SttError::Network` / `Protocol` / `Provider`)
//!    the supervisor sleeps `reconnect_delay(attempt)` and reopens the WS.
//!    State transitions: `Connected` → `Reconnecting { attempt }` →
//!    `Connected`.
//! 4. On fatal failure (`Auth`, `Quota`, `AudioFormat`) the supervisor
//!    surfaces the error on the events channel and stops.
//! 5. `close()` flips the atomic `closed` flag; the supervisor observes it
//!    and sends a WS close frame before returning.
//!
//! ### Auth
//!
//! The API key is read from [`DeepgramConfig::api_key`]. It is stored as a
//! plain `String`, but NEVER printed: logging helpers use `mask_api_key`
//! so only the last 4 characters ever appear. The one place the key
//! leaves this module is the `Authorization` header value passed to
//! `tokio_tungstenite::connect_async`.
//!
//! ### Error classification
//!
//! Mirrors Deepgram's documented HTTP status codes on handshake + the
//! `"type":"Error"` control frames on the open stream. See
//! `map_handshake_status` + `map_ws_error`.
//!
//! ### Tests
//!
//! Parser / URL builder / error mapping are covered by pure unit tests.
//! Live-connection behavior (auth header, happy-path transcript delivery,
//! handshake rejection) is covered by in-process mock WebSocket servers
//! bound to `127.0.0.1:0`, so none of this touches the real Deepgram API.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use cue_core::pcm::{AudioChunk, AudioSource};
use cue_core::stt::{
    ConnectionState, SttConfig, SttError, SttProvider, TranscriptEvent, WordTiming,
};
use futures_util::{SinkExt, StreamExt};
use parking_lot::Mutex;
use serde::Deserialize;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
use tokio_tungstenite::tungstenite::Error as WsError;
use tokio_tungstenite::tungstenite::Message;

/// Upper bound on consecutive reconnect attempts before we give up and
/// return `ConnectionState::Failed`. Backoff starts at 250 ms and doubles,
/// capped at 10 s; total budget is ~30 s.
pub const MAX_RECONNECT_ATTEMPTS: u32 = 6;

/// Configuration for the provider.
#[derive(Debug, Clone)]
pub struct DeepgramConfig {
    pub api_key: String,
    /// Which Deepgram model to request. Defaults to `nova-3`.
    pub model: String,
    /// When true, Deepgram returns `is_final: false` partial transcripts.
    pub interim_results: bool,
    /// BCP-47 language hint passed as `language=…`. `None` = auto-detect.
    pub language: Option<String>,
    /// Enable punctuation.
    pub punctuate: bool,
    /// Enable Deepgram smart formatting for numbers, dates, and common
    /// spoken forms. This is useful for answer prompts and screen notes.
    pub smart_format: bool,
    /// Deepgram endpointing silence window in milliseconds. `None` lets the
    /// provider use its default; Bluey's default is tuned for live answers.
    pub endpointing_ms: Option<u32>,
    /// Emit utterance-end events after this much silence. Requires interim
    /// results and VAD events to be useful.
    pub utterance_end_ms: Option<u32>,
    /// Ask Deepgram to emit VAD lifecycle events.
    pub vad_events: bool,
    /// Enable speaker diarization (the `TranscriptEvent::SpeakerLabel` path).
    pub diarize: bool,
    /// Override the base URL (tests use `ws://localhost:…`).
    pub base_url: Option<String>,
}

impl Default for DeepgramConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            model: "nova-3".into(),
            interim_results: true,
            language: None,
            punctuate: true,
            smart_format: true,
            endpointing_ms: Some(200),
            utterance_end_ms: Some(1_000),
            vad_events: true,
            diarize: false,
            base_url: None,
        }
    }
}

/// Build the Deepgram WebSocket URL from a config + the per-session
/// [`SttConfig`]. Exposed so unit tests can verify URL construction
/// deterministically without spinning up an actual WS server.
pub fn build_url(deepgram: &DeepgramConfig, stt: &SttConfig) -> Result<url::Url, SttError> {
    let default_url = obfstr::obfstr!("wss://api.deepgram.com").to_string();
    let base = deepgram.base_url.as_deref().unwrap_or(&default_url);
    let mut u = url::Url::parse(&format!("{base}/v1/listen"))
        .map_err(|e| SttError::Protocol(format!("invalid base url: {e}")))?;
    {
        let mut q = u.query_pairs_mut();
        q.append_pair("model", &deepgram.model);
        q.append_pair("encoding", "linear16");
        q.append_pair("sample_rate", &stt.sample_rate.hz().to_string());
        q.append_pair("channels", "1");
        if deepgram.punctuate {
            q.append_pair("punctuate", "true");
        }
        if deepgram.smart_format {
            q.append_pair("smart_format", "true");
        }
        if let Some(ms) = deepgram.endpointing_ms {
            q.append_pair("endpointing", &ms.to_string());
        }
        // Interim results: SttConfig.emit_partials wins (per-session) over
        // DeepgramConfig.interim_results (per-provider). If the session
        // explicitly disables partials, never request them; otherwise
        // defer to provider config.
        let want_interim = stt.emit_partials && deepgram.interim_results;
        if want_interim {
            q.append_pair("interim_results", "true");
            if let Some(ms) = deepgram.utterance_end_ms {
                q.append_pair("utterance_end_ms", &ms.to_string());
            }
            if deepgram.vad_events {
                q.append_pair("vad_events", "true");
            }
        }
        if deepgram.diarize {
            q.append_pair("diarize", "true");
        }
        let language = stt.language.as_deref().or(deepgram.language.as_deref());
        if let Some(lang) = language {
            q.append_pair("language", lang);
        }
    }
    Ok(u)
}

/// Return a safe-to-log form of the API key: never more than the last 4
/// characters, never the head of the secret.
pub fn mask_api_key(key: &str) -> String {
    // Character-based (not byte-based): safe for any UTF-8 key. Emits
    // `****` when the key is 4 or fewer chars, otherwise `****<last 4>`.
    let mut last4: Vec<char> = key.chars().rev().take(4).collect();
    if last4.len() < 4 || key.chars().count() <= 4 {
        return "****".to_string();
    }
    last4.reverse();
    let tail: String = last4.into_iter().collect();
    format!("****{tail}")
}

// ========== JSON frame shape (Deepgram response) ==========

/// The subset of Deepgram's response frame shape we actually use. Fields
/// outside this struct are ignored via serde's default-on-missing behavior.
#[derive(Debug, Deserialize)]
pub struct DgFrame {
    #[serde(rename = "type", default)]
    pub ty: Option<String>,
    #[serde(default)]
    pub is_final: bool,
    #[serde(default)]
    pub channel: Option<DgChannel>,
}

#[derive(Debug, Deserialize)]
pub struct DgChannel {
    #[serde(default)]
    pub alternatives: Vec<DgAlternative>,
}

#[derive(Debug, Deserialize)]
pub struct DgAlternative {
    #[serde(default)]
    pub transcript: String,
    #[serde(default)]
    pub confidence: Option<f32>,
    #[serde(default)]
    pub words: Vec<DgWord>,
}

#[derive(Debug, Deserialize)]
pub struct DgWord {
    pub word: String,
    pub start: f32,
    pub end: f32,
    #[serde(default)]
    pub confidence: Option<f32>,
    #[serde(default)]
    pub speaker: Option<u32>,
}

/// Parse a raw Deepgram JSON payload into an `SttProvider` event list.
/// Deepgram can emit 0, 1, or 2 events per frame (transcript + diarization).
pub fn parse_frame(payload: &str, source: AudioSource) -> Result<Vec<TranscriptEvent>, SttError> {
    let value: serde_json::Value = serde_json::from_str(payload)
        .map_err(|e| SttError::Protocol(format!("invalid Deepgram JSON: {e}")))?;
    let frame_type = value.get("type").and_then(|ty| ty.as_str());

    if matches!(frame_type, Some("Error")) {
        return Err(SttError::Provider(payload.to_string()));
    }

    // Deepgram also emits lifecycle/VAD frames such as SpeechStarted and
    // UtteranceEnd. Those frames may use `channel: 0` or `channel: [0, 1]`,
    // so parsing the full response as a transcript frame would incorrectly
    // kill live captions. Only Results frames can produce transcript events.
    if !matches!(frame_type, None | Some("Results")) {
        return Ok(Vec::new());
    }

    let Some(channel_value) = value.get("channel") else {
        return Ok(Vec::new());
    };
    if !channel_value.is_object() {
        return Ok(Vec::new());
    }
    let channel: DgChannel = match serde_json::from_value(channel_value.clone()) {
        Ok(channel) => channel,
        Err(error) => {
            return Err(SttError::Protocol(format!(
                "invalid Deepgram channel JSON: {error}"
            )));
        }
    };
    let is_final = value
        .get("is_final")
        .and_then(|is_final| is_final.as_bool())
        .unwrap_or(false);

    let alt = match channel.alternatives.into_iter().next() {
        Some(a) => a,
        None => return Ok(Vec::new()),
    };
    if alt.transcript.trim().is_empty() && alt.words.is_empty() {
        return Ok(Vec::new());
    }

    let words: Vec<WordTiming> = alt
        .words
        .iter()
        .map(|w| WordTiming {
            word: w.word.clone(),
            start_s: w.start,
            end_s: w.end,
            confidence: w.confidence,
        })
        .collect();

    let mut out = Vec::with_capacity(2);

    if is_final {
        out.push(TranscriptEvent::Final {
            text: alt.transcript,
            confidence: alt.confidence,
            source,
            words,
        });
    } else {
        out.push(TranscriptEvent::Partial {
            text: alt.transcript,
            confidence: alt.confidence,
            source,
        });
    }

    // Surface the first word-level speaker label if diarization is on.
    if let Some(speaker) = alt.words.iter().find_map(|w| w.speaker) {
        out.push(TranscriptEvent::SpeakerLabel { speaker, source });
    }

    Ok(out)
}

/// Map an HTTP handshake status code into the provider-level error. Used
/// when the initial `connect_async` fails.
pub fn map_handshake_status(status: u16) -> SttError {
    match status {
        401 | 403 => SttError::Auth,
        402 => SttError::Quota(format!("status {status}")),
        429 => SttError::Quota("rate limited".into()),
        400 => SttError::AudioFormat("invalid request params".into()),
        s if (500..600).contains(&s) => SttError::Network(format!("server error {s}")),
        other => SttError::Protocol(format!("unexpected handshake status {other}")),
    }
}

/// Map a `tokio_tungstenite` error to an `SttError`. Used by both the
/// connect-time handshake path and the on-stream read/write path.
pub fn map_ws_error(e: WsError) -> SttError {
    match e {
        WsError::Http(resp) => map_handshake_status(resp.status().as_u16()),
        WsError::HttpFormat(_) => SttError::Protocol("malformed HTTP".into()),
        WsError::Io(io) => SttError::Network(io.to_string()),
        WsError::Tls(_) => SttError::Network("TLS error".into()),
        WsError::ConnectionClosed | WsError::AlreadyClosed => {
            SttError::Network("connection closed".into())
        }
        WsError::Protocol(p) => SttError::Protocol(p.to_string()),
        WsError::Utf8 => SttError::Protocol("invalid UTF-8".into()),
        WsError::Url(_) => SttError::Protocol("invalid URL".into()),
        _ => SttError::Network(format!("{e}")),
    }
}

// ========== Provider ==========

/// Shared state accessed from both the foreground `SttProvider` methods
/// and the background supervisor task.
#[derive(Default)]
struct DeepgramState {
    connection: Mutex<DgConnection>,
    closed: std::sync::atomic::AtomicBool,
}

struct DgConnection {
    state: ConnectionState,
}

impl Default for DgConnection {
    fn default() -> Self {
        Self {
            state: ConnectionState::Idle,
        }
    }
}

/// The live provider handle. Produced by [`DeepgramProvider::connect`].
pub struct DeepgramProvider {
    state: Arc<DeepgramState>,
    audio_tx: Option<UnboundedSender<Vec<u8>>>,
    events_rx: UnboundedReceiver<Result<TranscriptEvent, SttError>>,
}

impl std::fmt::Debug for DeepgramProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DeepgramProvider")
            .field("state", &self.state.connection.lock().state)
            .finish()
    }
}

impl DeepgramProvider {
    /// Build a provider from an existing event/audio channel pair and a
    /// starting connection state. This is the seam unit tests use to drive
    /// the provider without opening a real WebSocket.
    pub fn from_channels(
        _source: AudioSource,
        initial: ConnectionState,
        events_rx: UnboundedReceiver<Result<TranscriptEvent, SttError>>,
        audio_tx: UnboundedSender<Vec<u8>>,
    ) -> Self {
        let state = Arc::new(DeepgramState::default());
        state.connection.lock().state = initial;
        Self {
            state,
            audio_tx: Some(audio_tx),
            events_rx,
        }
    }

    /// Open a live Deepgram connection and start the supervisor task.
    ///
    /// Returns as soon as the supervisor is launched — actual WebSocket
    /// handshake happens inside the task, so early failures (auth etc.)
    /// surface on the first `next_event()` call rather than here.
    /// `connection_state()` reports `Connecting` until the handshake
    /// completes, then flips to `Connected`.
    pub async fn connect(
        cfg: DeepgramConfig,
        stt_cfg: SttConfig,
        source: AudioSource,
    ) -> Result<Self, SttError> {
        if cfg.api_key.is_empty() {
            return Err(SttError::Auth);
        }

        let state = Arc::new(DeepgramState::default());
        state.connection.lock().state = ConnectionState::Connecting;

        let (audio_tx, audio_rx) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();
        let (events_tx, events_rx) =
            tokio::sync::mpsc::unbounded_channel::<Result<TranscriptEvent, SttError>>();

        tokio::spawn(run_supervisor(
            cfg,
            stt_cfg,
            source,
            state.clone(),
            audio_rx,
            events_tx,
        ));

        Ok(Self {
            state,
            audio_tx: Some(audio_tx),
            events_rx,
        })
    }

    /// Convenience for tests that only need the state-machine half.
    pub fn set_connection_state(&self, s: ConnectionState) {
        self.state.connection.lock().state = s;
    }
}

#[async_trait]
impl SttProvider for DeepgramProvider {
    fn name(&self) -> &'static str {
        "deepgram_nova3"
    }

    fn connection_state(&self) -> ConnectionState {
        self.state.connection.lock().state
    }

    async fn send_audio(&self, chunk: &AudioChunk) -> Result<(), SttError> {
        if self.state.closed.load(std::sync::atomic::Ordering::Acquire) {
            return Err(SttError::NotActive);
        }
        let tx = self.audio_tx.as_ref().ok_or(SttError::NotActive)?;
        // Cheap LE byte cast (all our targets are little-endian).
        let bytes: Vec<u8> = bytemuck::cast_slice(&chunk.samples).to_vec();
        tx.send(bytes).map_err(|_| SttError::NotActive)?;
        Ok(())
    }

    async fn finalize(&self) -> Result<(), SttError> {
        // Empty binary frame is our in-process signal to the supervisor
        // that the utterance ended; it turns that into Deepgram's
        // CloseStream JSON control message on the wire.
        if let Some(tx) = &self.audio_tx {
            tx.send(Vec::new()).map_err(|_| SttError::NotActive)?;
        }
        Ok(())
    }

    async fn next_event(&mut self) -> Option<Result<TranscriptEvent, SttError>> {
        self.events_rx.recv().await
    }

    async fn close(&mut self) -> Result<(), SttError> {
        self.state
            .closed
            .store(true, std::sync::atomic::Ordering::Release);
        self.state.connection.lock().state = ConnectionState::Closed;
        self.audio_tx = None;
        Ok(())
    }
}

/// Exponential backoff helper: returns how long to sleep before the Nth
/// reconnect attempt (0-indexed). Caps at 10 s.
pub fn reconnect_delay(attempt: u32) -> Duration {
    let base_ms: u64 = 250;
    let capped = attempt.min(8);
    let ms = base_ms.saturating_mul(1u64 << capped);
    Duration::from_millis(ms.min(10_000))
}

// ========== Supervisor + connection loop ==========

/// Top-level task: repeatedly establish a connection, deliver events, and
/// handle reconnects on retryable failures.
async fn run_supervisor(
    cfg: DeepgramConfig,
    stt_cfg: SttConfig,
    source: AudioSource,
    state: Arc<DeepgramState>,
    mut audio_rx: UnboundedReceiver<Vec<u8>>,
    events_tx: UnboundedSender<Result<TranscriptEvent, SttError>>,
) {
    let mut attempt: u32 = 0;
    loop {
        if state.closed.load(std::sync::atomic::Ordering::Acquire) {
            break;
        }

        state.connection.lock().state = if attempt == 0 {
            ConnectionState::Connecting
        } else {
            ConnectionState::Reconnecting { attempt }
        };

        match run_connection(&cfg, &stt_cfg, source, &state, &mut audio_rx, &events_tx).await {
            Ok(()) => {
                // Clean close initiated by our side.
                state.connection.lock().state = ConnectionState::Closed;
                return;
            }
            Err(e) => {
                if !e.is_retryable() {
                    tracing::error!(
                        provider = "deepgram_nova3",
                        api_key = %mask_api_key(&cfg.api_key),
                        error = ?e,
                        "fatal stream error"
                    );
                    let _ = events_tx.send(Err(e));
                    state.connection.lock().state = ConnectionState::Failed;
                    return;
                }
                attempt += 1;
                if attempt > MAX_RECONNECT_ATTEMPTS {
                    tracing::error!(
                        provider = "deepgram_nova3",
                        api_key = %mask_api_key(&cfg.api_key),
                        attempts = attempt,
                        "giving up after max reconnect attempts"
                    );
                    let _ = events_tx.send(Err(e));
                    state.connection.lock().state = ConnectionState::Failed;
                    return;
                }
                let delay = reconnect_delay(attempt - 1);
                tracing::warn!(
                    provider = "deepgram_nova3",
                    api_key = %mask_api_key(&cfg.api_key),
                    attempt,
                    delay_ms = delay.as_millis() as u64,
                    error = ?e,
                    "reconnecting"
                );
                tokio::time::sleep(delay).await;
            }
        }
    }
}

/// Single connection lifecycle: open → pump events → return on close/error.
async fn run_connection(
    cfg: &DeepgramConfig,
    stt_cfg: &SttConfig,
    source: AudioSource,
    state: &Arc<DeepgramState>,
    audio_rx: &mut UnboundedReceiver<Vec<u8>>,
    events_tx: &UnboundedSender<Result<TranscriptEvent, SttError>>,
) -> Result<(), SttError> {
    let url = build_url(cfg, stt_cfg)?;
    let auth_value = format!("Token {}", cfg.api_key);

    let mut request = url.as_str().into_client_request().map_err(map_ws_error)?;
    {
        let hdr = obfstr::obfstr!("authorization").to_string();
        request.headers_mut().insert(
            tokio_tungstenite::tungstenite::http::HeaderName::from_bytes(hdr.as_bytes()).unwrap(),
            auth_value.parse().map_err(|_| SttError::Auth)?,
        );
    }

    let (ws_stream, _response) = tokio_tungstenite::connect_async(request)
        .await
        .map_err(map_ws_error)?;

    state.connection.lock().state = ConnectionState::Connected;

    let (mut write, mut read) = ws_stream.split();

    loop {
        if state.closed.load(std::sync::atomic::Ordering::Acquire) {
            let _ = write.send(Message::Close(None)).await;
            return Ok(());
        }

        tokio::select! {
            maybe_audio = audio_rx.recv() => {
                match maybe_audio {
                    Some(bytes) => {
                        let msg = if bytes.is_empty() {
                            // finalize(): send Deepgram's CloseStream JSON
                            // control without actually closing the socket.
                            Message::text(r#"{"type":"CloseStream"}"#)
                        } else {
                            Message::binary(bytes)
                        };
                        if let Err(e) = write.send(msg).await {
                            return Err(map_ws_error(e));
                        }
                    }
                    None => {
                        // Sender dropped — provider being torn down.
                        let _ = write.send(Message::Close(None)).await;
                        return Ok(());
                    }
                }
            }
            maybe_frame = read.next() => {
                match maybe_frame {
                    Some(Ok(Message::Text(text))) => {
                        match parse_frame(&text, source) {
                            Ok(events) => {
                                for ev in events {
                                    if events_tx.send(Ok(ev)).is_err() {
                                        return Ok(());
                                    }
                                }
                            }
                            Err(SttError::Provider(_)) => {
                                // Provider-level error in the JSON body.
                                // Surface but keep the stream alive — the
                                // next frame may recover.
                                let _ = events_tx.send(
                                    Err(SttError::Provider(text.to_string())),
                                );
                            }
                            Err(e) => {
                                // Protocol parse failure is retryable.
                                return Err(e);
                            }
                        }
                    }
                    Some(Ok(Message::Binary(_)))
                    | Some(Ok(Message::Ping(_)))
                    | Some(Ok(Message::Pong(_)))
                    | Some(Ok(Message::Frame(_))) => {
                        // Not expected from Deepgram; ignore.
                    }
                    Some(Ok(Message::Close(frame))) => {
                        if let Some(cf) = frame {
                            if cf.code != CloseCode::Normal && cf.code != CloseCode::Away {
                                return Err(SttError::Network(format!(
                                    "WS closed with code {}",
                                    cf.code
                                )));
                            }
                        }
                        return Ok(());
                    }
                    Some(Err(e)) => {
                        return Err(map_ws_error(e));
                    }
                    None => {
                        return Err(SttError::Network("WS stream ended".into()));
                    }
                }
            }
        }
    }
}

// ========== Tests ==========

#[cfg(test)]
#[allow(clippy::result_large_err)]
mod tests {
    use super::*;
    use cue_core::pcm::{AudioSource, SampleRate};
    use tokio::sync::mpsc::unbounded_channel;

    fn cfg() -> (DeepgramConfig, SttConfig) {
        let dc = DeepgramConfig {
            api_key: "dg_secret_xxx1234".into(),
            ..Default::default()
        };
        let sc = SttConfig::default();
        (dc, sc)
    }

    #[test]
    fn url_builder_includes_model_and_encoding_params() {
        let (dc, sc) = cfg();
        let url = build_url(&dc, &sc).unwrap();
        let q: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
        assert_eq!(q.get("model").map(String::as_str), Some("nova-3"));
        assert_eq!(q.get("encoding").map(String::as_str), Some("linear16"));
        assert_eq!(q.get("sample_rate").map(String::as_str), Some("16000"));
        assert_eq!(q.get("channels").map(String::as_str), Some("1"));
        assert_eq!(q.get("punctuate").map(String::as_str), Some("true"));
        assert_eq!(q.get("smart_format").map(String::as_str), Some("true"));
        assert_eq!(q.get("endpointing").map(String::as_str), Some("200"));
        assert_eq!(q.get("interim_results").map(String::as_str), Some("true"));
        assert_eq!(q.get("utterance_end_ms").map(String::as_str), Some("1000"));
        assert_eq!(q.get("vad_events").map(String::as_str), Some("true"));
    }

    #[test]
    fn url_builder_suppresses_utterance_events_when_partials_disabled() {
        let (dc, mut sc) = cfg();
        sc.emit_partials = false;
        let url = build_url(&dc, &sc).unwrap();
        let q: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
        assert_eq!(q.get("interim_results"), None);
        assert_eq!(q.get("utterance_end_ms"), None);
        assert_eq!(q.get("vad_events"), None);
        assert_eq!(q.get("endpointing").map(String::as_str), Some("200"));
    }

    #[test]
    fn url_builder_honors_custom_base_url_for_tests() {
        let dc = DeepgramConfig {
            base_url: Some("ws://localhost:8234".into()),
            ..Default::default()
        };
        let sc = SttConfig::default();
        let url = build_url(&dc, &sc).unwrap();
        assert_eq!(url.scheme(), "ws");
        assert_eq!(url.host_str(), Some("localhost"));
        assert_eq!(url.port(), Some(8234));
        assert_eq!(url.path(), "/v1/listen");
    }

    #[test]
    fn url_builder_threads_language_and_diarize() {
        let dc = DeepgramConfig {
            diarize: true,
            ..Default::default()
        };
        let sc = SttConfig {
            language: Some("en-US".into()),
            ..Default::default()
        };
        let url = build_url(&dc, &sc).unwrap();
        let q: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
        assert_eq!(q.get("diarize").map(String::as_str), Some("true"));
        assert_eq!(q.get("language").map(String::as_str), Some("en-US"));
    }

    #[test]
    fn mask_api_key_never_leaks_secret() {
        assert_eq!(mask_api_key("dg_abcdefghij1234"), "****1234");
        assert_eq!(mask_api_key("abc"), "****");
        assert_eq!(mask_api_key(""), "****");
    }

    #[test]
    fn parse_frame_partial_transcript() {
        let payload = r#"{
          "type": "Results",
          "is_final": false,
          "channel": {
            "alternatives": [
              {"transcript": "hello wor", "confidence": 0.82, "words": []}
            ]
          }
        }"#;
        let events = parse_frame(payload, AudioSource::Microphone).unwrap();
        assert_eq!(events.len(), 1);
        assert!(matches!(
            events[0],
            TranscriptEvent::Partial { ref text, .. } if text == "hello wor"
        ));
    }

    #[test]
    fn parse_frame_final_with_word_timing() {
        let payload = r#"{
          "type": "Results",
          "is_final": true,
          "channel": {
            "alternatives": [{
              "transcript": "hello world",
              "confidence": 0.97,
              "words": [
                {"word": "hello", "start": 0.0, "end": 0.4, "confidence": 0.98},
                {"word": "world", "start": 0.4, "end": 0.9, "confidence": 0.96}
              ]
            }]
          }
        }"#;
        let events = parse_frame(payload, AudioSource::Microphone).unwrap();
        assert_eq!(events.len(), 1);
        let TranscriptEvent::Final { text, words, .. } = &events[0] else {
            panic!("expected Final, got {:?}", events[0]);
        };
        assert_eq!(text, "hello world");
        assert_eq!(words.len(), 2);
        assert_eq!(words[0].word, "hello");
        assert!((words[0].start_s - 0.0).abs() < 1e-6);
        assert!((words[0].end_s - 0.4).abs() < 1e-6);
    }

    #[test]
    fn parse_frame_emits_speaker_label_on_diarization() {
        let payload = r#"{
          "type": "Results",
          "is_final": true,
          "channel": {
            "alternatives": [{
              "transcript": "hi",
              "confidence": 0.9,
              "words": [
                {"word": "hi", "start": 0.0, "end": 0.2, "speaker": 0}
              ]
            }]
          }
        }"#;
        let events = parse_frame(payload, AudioSource::Microphone).unwrap();
        assert_eq!(events.len(), 2);
        assert!(matches!(events[0], TranscriptEvent::Final { .. }));
        assert!(matches!(
            events[1],
            TranscriptEvent::SpeakerLabel {
                speaker: 0,
                source: AudioSource::Microphone
            }
        ));
    }

    #[test]
    fn parse_frame_empty_transcript_returns_no_events() {
        let payload = r#"{"type":"Results","is_final":false,"channel":{"alternatives":[{"transcript":"","words":[]}]}}"#;
        let events = parse_frame(payload, AudioSource::Microphone).unwrap();
        assert!(events.is_empty());
    }

    #[test]
    fn parse_frame_ignores_speech_started_control_frame() {
        let payload = r#"{"type":"SpeechStarted","channel":0,"timestamp":1.24}"#;
        let events = parse_frame(payload, AudioSource::Microphone).unwrap();
        assert!(events.is_empty());
    }

    #[test]
    fn parse_frame_ignores_utterance_end_control_frame() {
        let payload = r#"{"type":"UtteranceEnd","channel":[0,1],"last_word_end":2.5}"#;
        let events = parse_frame(payload, AudioSource::System).unwrap();
        assert!(events.is_empty());
    }

    #[test]
    fn parse_frame_ignores_non_object_results_channel() {
        let payload = r#"{"type":"Results","is_final":false,"channel":0}"#;
        let events = parse_frame(payload, AudioSource::Microphone).unwrap();
        assert!(events.is_empty());
    }

    #[test]
    fn parse_frame_error_type_surfaces_provider_error() {
        let payload = r#"{"type":"Error","description":"bad"}"#;
        let err = parse_frame(payload, AudioSource::Microphone).unwrap_err();
        assert!(matches!(err, SttError::Provider(_)));
    }

    #[test]
    fn parse_frame_malformed_json_is_protocol_error() {
        let err = parse_frame("not json", AudioSource::Microphone).unwrap_err();
        assert!(matches!(err, SttError::Protocol(_)));
    }

    #[test]
    fn handshake_status_map_auth() {
        assert!(matches!(map_handshake_status(401), SttError::Auth));
        assert!(matches!(map_handshake_status(403), SttError::Auth));
    }

    #[test]
    fn handshake_status_map_quota() {
        assert!(matches!(map_handshake_status(402), SttError::Quota(_)));
        assert!(matches!(map_handshake_status(429), SttError::Quota(_)));
    }

    #[test]
    fn handshake_status_map_audio_format() {
        assert!(matches!(
            map_handshake_status(400),
            SttError::AudioFormat(_)
        ));
    }

    #[test]
    fn handshake_status_map_server_error_is_retryable() {
        let err = map_handshake_status(503);
        assert!(err.is_retryable());
        assert!(matches!(err, SttError::Network(_)));
    }

    #[test]
    fn handshake_status_map_auth_triggers_failover_not_retry() {
        let err = map_handshake_status(401);
        assert!(err.should_failover());
        assert!(!err.is_retryable());
    }

    #[test]
    fn reconnect_delay_grows_exponentially_and_caps_at_10s() {
        assert_eq!(reconnect_delay(0).as_millis(), 250);
        assert_eq!(reconnect_delay(1).as_millis(), 500);
        assert_eq!(reconnect_delay(2).as_millis(), 1_000);
        assert_eq!(reconnect_delay(3).as_millis(), 2_000);
        assert_eq!(reconnect_delay(20).as_millis(), 10_000);
    }

    #[tokio::test]
    async fn provider_reports_initial_connection_state() {
        let (audio_tx, _audio_rx) = unbounded_channel::<Vec<u8>>();
        let (_ev_tx, ev_rx) = unbounded_channel();
        let provider = DeepgramProvider::from_channels(
            AudioSource::Microphone,
            ConnectionState::Connected,
            ev_rx,
            audio_tx,
        );
        assert_eq!(provider.connection_state(), ConnectionState::Connected);
    }

    #[tokio::test]
    async fn provider_close_flips_state_and_rejects_send() {
        let (audio_tx, _audio_rx) = unbounded_channel::<Vec<u8>>();
        let (_ev_tx, ev_rx) = unbounded_channel();
        let mut provider = DeepgramProvider::from_channels(
            AudioSource::Microphone,
            ConnectionState::Connected,
            ev_rx,
            audio_tx,
        );
        provider.close().await.unwrap();
        assert_eq!(provider.connection_state(), ConnectionState::Closed);

        let chunk = AudioChunk {
            source: AudioSource::Microphone,
            sample_rate: SampleRate::new(16_000).unwrap(),
            samples: vec![0i16; 320],
            captured_at_ms: 0,
        };
        let err = provider.send_audio(&chunk).await.unwrap_err();
        assert!(matches!(err, SttError::NotActive));
    }

    #[tokio::test]
    async fn provider_forwards_scripted_events() {
        let (audio_tx, _audio_rx) = unbounded_channel::<Vec<u8>>();
        let (ev_tx, ev_rx) = unbounded_channel();
        let mut provider = DeepgramProvider::from_channels(
            AudioSource::System,
            ConnectionState::Connected,
            ev_rx,
            audio_tx,
        );
        ev_tx
            .send(Ok(TranscriptEvent::Partial {
                text: "hi".into(),
                confidence: Some(0.9),
                source: AudioSource::System,
            }))
            .unwrap();
        let e = provider.next_event().await.unwrap().unwrap();
        assert!(matches!(e, TranscriptEvent::Partial { ref text, .. } if text == "hi"));
    }

    #[tokio::test]
    async fn connect_rejects_empty_api_key_before_io() {
        let cfg = DeepgramConfig::default();
        let err = DeepgramProvider::connect(cfg, SttConfig::default(), AudioSource::Microphone)
            .await
            .unwrap_err();
        assert!(matches!(err, SttError::Auth));
    }

    // ========== Mock WebSocket server tests ==========
    //
    // Spin up a real TCP listener on 127.0.0.1:0, accept ONE websocket
    // connection, and run a scripted handler against it. These tests
    // exercise the full `connect_async` path (real handshake, real TCP)
    // without touching Deepgram's API.

    use std::net::SocketAddr;
    use std::sync::mpsc as stdmpsc;
    use tokio::net::TcpListener;
    use tokio_tungstenite::tungstenite::handshake::server::{ErrorResponse, Request, Response};

    /// Accept exactly one WS connection on a fresh local port. The
    /// `auth_capture` Sender receives the value of the inbound
    /// `Authorization` header (or `None` if absent) so tests can assert
    /// on it.
    async fn start_mock_server<F, Fut>(
        auth_capture: stdmpsc::Sender<Option<String>>,
        handler: F,
    ) -> SocketAddr
    where
        F: FnOnce(tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>) -> Fut
            + Send
            + 'static,
        Fut: std::future::Future<Output = ()> + Send + 'static,
    {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let capture = auth_capture;
            let cb = move |req: &Request,
                           resp: Response|
                  -> std::result::Result<Response, ErrorResponse> {
                let auth = req
                    .headers()
                    .get("Authorization")
                    .and_then(|v| v.to_str().ok().map(str::to_string));
                let _ = capture.send(auth);
                Ok(resp)
            };
            let ws = match tokio_tungstenite::accept_hdr_async(stream, cb).await {
                Ok(ws) => ws,
                Err(_) => return,
            };
            handler(ws).await;
        });
        addr
    }

    /// Reject-handshake variant: returns a non-success HTTP status so the
    /// client's `connect_async` maps it to `SttError::Auth` / etc.
    async fn start_rejecting_server(status: u16) -> SocketAddr {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            if let Ok((stream, _)) = listener.accept().await {
                let cb = move |_req: &Request,
                               _resp: Response|
                      -> std::result::Result<Response, ErrorResponse> {
                    let body = tokio_tungstenite::tungstenite::http::Response::builder()
                        .status(status)
                        .body(Some("rejected".to_string()))
                        .unwrap();
                    Err(body)
                };
                let _ = tokio_tungstenite::accept_hdr_async(stream, cb).await;
            }
        });
        addr
    }

    #[tokio::test]
    async fn connect_sends_authorization_header() {
        let (tx, rx) = stdmpsc::channel();
        let addr = start_mock_server(tx, |mut ws| async move {
            // Send one final transcript then close so the test completes.
            let _ = ws
                .send(Message::text(
                    r#"{"type":"Results","is_final":true,"channel":{"alternatives":[{"transcript":"hi","confidence":0.9,"words":[]}]}}"#,
                ))
                .await;
            let _ = ws.close(None).await;
        })
        .await;

        let cfg = DeepgramConfig {
            api_key: "dg_real_key_abcd1234".into(),
            base_url: Some(format!("ws://{addr}")),
            ..Default::default()
        };
        let mut provider =
            DeepgramProvider::connect(cfg, SttConfig::default(), AudioSource::Microphone)
                .await
                .unwrap();

        // Drive the connection by waiting for the first event.
        let _ = tokio::time::timeout(Duration::from_secs(5), provider.next_event())
            .await
            .expect("no transcript within timeout")
            .expect("stream closed")
            .expect("stream error");

        // Now the handshake has definitely happened; read the captured header.
        let captured = tokio::task::spawn_blocking(move || rx.recv_timeout(Duration::from_secs(2)))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            captured.as_deref(),
            Some("Token dg_real_key_abcd1234"),
            "Authorization header was not sent correctly"
        );
    }

    #[tokio::test]
    async fn connect_delivers_final_transcript_from_server() {
        let (tx, _rx) = stdmpsc::channel();
        let addr = start_mock_server(tx, |mut ws| async move {
            let _ = ws
                .send(Message::text(
                    r#"{"type":"Results","is_final":true,"channel":{"alternatives":[{"transcript":"the quick brown fox","confidence":0.95,"words":[]}]}}"#,
                ))
                .await;
            let _ = ws.close(None).await;
        })
        .await;

        let cfg = DeepgramConfig {
            api_key: "dg_test".into(),
            base_url: Some(format!("ws://{addr}")),
            ..Default::default()
        };
        let mut provider =
            DeepgramProvider::connect(cfg, SttConfig::default(), AudioSource::Microphone)
                .await
                .unwrap();

        let event = tokio::time::timeout(Duration::from_secs(5), provider.next_event())
            .await
            .expect("timeout")
            .expect("closed")
            .expect("error");
        let TranscriptEvent::Final { text, .. } = event else {
            panic!("expected Final, got {event:?}");
        };
        assert_eq!(text, "the quick brown fox");
    }

    #[tokio::test]
    async fn connect_forwards_audio_as_binary_frame() {
        let (tx, _rx) = stdmpsc::channel();
        let (sig_tx, sig_rx) = std::sync::mpsc::channel();
        let addr = start_mock_server(tx, move |mut ws| async move {
            // Read the first message from the client. It must be binary.
            if let Some(Ok(msg)) = ws.next().await {
                let is_binary = msg.is_binary();
                let len = msg.into_data().len();
                let _ = sig_tx.send((is_binary, len));
            }
            let _ = ws
                .send(Message::text(
                    r#"{"type":"Results","is_final":true,"channel":{"alternatives":[{"transcript":"ack","confidence":1.0,"words":[]}]}}"#,
                ))
                .await;
            let _ = ws.close(None).await;
        })
        .await;

        let cfg = DeepgramConfig {
            api_key: "dg_test".into(),
            base_url: Some(format!("ws://{addr}")),
            ..Default::default()
        };
        let provider =
            DeepgramProvider::connect(cfg, SttConfig::default(), AudioSource::Microphone)
                .await
                .unwrap();

        // Wait briefly so the supervisor has a chance to complete the
        // handshake before we push audio.
        tokio::time::sleep(Duration::from_millis(100)).await;

        let chunk = AudioChunk {
            source: AudioSource::Microphone,
            sample_rate: SampleRate::new(16_000).unwrap(),
            samples: vec![0i16; 320], // 20 ms @ 16 kHz
            captured_at_ms: 0,
        };
        provider.send_audio(&chunk).await.unwrap();

        // Give the server a chance to observe the binary frame.
        let (is_binary, len) =
            tokio::task::spawn_blocking(move || sig_rx.recv_timeout(Duration::from_secs(3)))
                .await
                .unwrap()
                .unwrap();
        assert!(is_binary, "client must send audio as a binary WS frame");
        assert_eq!(len, 640, "320 i16 samples == 640 LE bytes");
    }

    #[tokio::test]
    async fn connect_maps_401_handshake_rejection_to_auth() {
        let addr = start_rejecting_server(401).await;
        let cfg = DeepgramConfig {
            api_key: "dg_bad_key".into(),
            base_url: Some(format!("ws://{addr}")),
            ..Default::default()
        };
        let mut provider =
            DeepgramProvider::connect(cfg, SttConfig::default(), AudioSource::Microphone)
                .await
                .unwrap();
        // Auth errors are fatal — supervisor surfaces and stops.
        let err = tokio::time::timeout(Duration::from_secs(5), provider.next_event())
            .await
            .expect("timeout waiting for error")
            .expect("stream closed before error")
            .expect_err("expected auth error");
        assert!(matches!(err, SttError::Auth), "got {err:?}");
        // After receiving the fatal error, state should be Failed.
        // Give the supervisor a tick to update the flag.
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(provider.connection_state(), ConnectionState::Failed);
    }

    #[tokio::test]
    async fn connect_maps_429_handshake_rejection_to_quota() {
        let addr = start_rejecting_server(429).await;
        let cfg = DeepgramConfig {
            api_key: "dg_ok_key".into(),
            base_url: Some(format!("ws://{addr}")),
            ..Default::default()
        };
        let mut provider =
            DeepgramProvider::connect(cfg, SttConfig::default(), AudioSource::Microphone)
                .await
                .unwrap();
        let err = tokio::time::timeout(Duration::from_secs(5), provider.next_event())
            .await
            .expect("timeout")
            .expect("closed")
            .expect_err("expected quota error");
        assert!(matches!(err, SttError::Quota(_)), "got {err:?}");
    }

    /// Mock server that fails the FIRST connection with an abnormal WS
    /// close code (classified as retryable `SttError::Network`) and then
    /// serves a happy-path transcript on the SECOND connection. Exercises
    /// the supervisor's reconnect loop end-to-end.
    async fn start_retry_server() -> SocketAddr {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            use tokio_tungstenite::tungstenite::protocol::{frame::coding::CloseCode, CloseFrame};
            // First connection: accept handshake, then close with an
            // abnormal close code to force a retry.
            if let Ok((stream, _)) = listener.accept().await {
                if let Ok(mut ws) = tokio_tungstenite::accept_async(stream).await {
                    let cf = CloseFrame {
                        code: CloseCode::Error,
                        reason: "simulated server error".into(),
                    };
                    let _ = ws.send(Message::Close(Some(cf))).await;
                }
            }
            // Second connection: deliver a final transcript, then clean close.
            if let Ok((stream, _)) = listener.accept().await {
                if let Ok(mut ws) = tokio_tungstenite::accept_async(stream).await {
                    let _ = ws
                        .send(Message::text(
                            r#"{"type":"Results","is_final":true,"channel":{"alternatives":[{"transcript":"after reconnect","confidence":0.9,"words":[]}]}}"#,
                        ))
                        .await;
                    let _ = ws.close(None).await;
                }
            }
        });
        addr
    }

    #[tokio::test]
    async fn connect_reconnects_on_retryable_server_close() {
        let addr = start_retry_server().await;
        let cfg = DeepgramConfig {
            api_key: "dg_test".into(),
            base_url: Some(format!("ws://{addr}")),
            ..Default::default()
        };
        let mut provider =
            DeepgramProvider::connect(cfg, SttConfig::default(), AudioSource::Microphone)
                .await
                .unwrap();

        // First connection fails with a retryable close; supervisor
        // backs off `reconnect_delay(0) == 250 ms` and retries. The
        // transcript we assert on is only sent on the SECOND connection,
        // so receiving it proves the retry path ran.
        let event = tokio::time::timeout(Duration::from_secs(5), provider.next_event())
            .await
            .expect("no transcript within timeout — reconnect path did not recover")
            .expect("stream closed before any event")
            .expect("stream error — reconnect did not succeed");

        let TranscriptEvent::Final { text, .. } = event else {
            panic!("expected Final after reconnect, got {event:?}");
        };
        assert_eq!(text, "after reconnect");
    }
}
