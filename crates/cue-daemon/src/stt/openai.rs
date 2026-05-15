//! OpenAI Realtime STT provider — transcription session protocol.
//!
//! Connects via WebSocket to the OpenAI Realtime API with `?intent=transcription`.
//! After handshake, sends `session.update` to configure transcription mode.
//! Audio is sent as base64-encoded PCM16 24kHz mono via `input_audio_buffer.append`.
//! Transcripts arrive as `conversation.item.input_audio_transcription.delta` (partial)
//! and `conversation.item.input_audio_transcription.completed` (final).

use std::sync::Arc;

use async_trait::async_trait;
use base64::Engine;
use cue_core::pcm::{AudioChunk, AudioSource};
use cue_core::stt::{ConnectionState, SttConfig, SttError, SttProvider, TranscriptEvent};
use futures_util::{SinkExt, StreamExt};
use parking_lot::Mutex;
use serde::Deserialize;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
use tokio_tungstenite::tungstenite::Message;

use super::deepgram::{map_ws_error, mask_api_key, reconnect_delay, MAX_RECONNECT_ATTEMPTS};

/// Configuration for the OpenAI Realtime provider.
#[derive(Debug, Clone)]
pub struct OpenAiRealtimeConfig {
    pub api_key: String,
    /// Transcription model. Defaults to "gpt-4o-mini-transcribe".
    pub model: String,
    /// Override base URL for tests.
    pub base_url: Option<String>,
}

impl Default for OpenAiRealtimeConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            model: "gpt-4o-mini-transcribe".into(),
            base_url: None,
        }
    }
}

/// Linear interpolation resample from 16kHz to 24kHz (ratio 2:3).
pub fn resample_16k_to_24k(samples: &[i16]) -> Vec<i16> {
    if samples.is_empty() {
        return Vec::new();
    }
    let out_len = (samples.len() as u64 * 3 / 2) as usize;
    let mut out = Vec::with_capacity(out_len);
    for i in 0..out_len {
        let src = i as f64 * 2.0 / 3.0;
        let idx = src as usize;
        let frac = src - idx as f64;
        let s = if idx + 1 < samples.len() {
            let a = samples[idx] as f64;
            let b = samples[idx + 1] as f64;
            (a + frac * (b - a)) as i16
        } else {
            samples[samples.len() - 1]
        };
        out.push(s);
    }
    out
}

// ========== JSON frame parsing ==========

#[derive(Debug, Deserialize)]
struct OaiEvent {
    #[serde(rename = "type", default)]
    ty: Option<String>,
    #[serde(default)]
    delta: Option<String>,
    #[serde(default)]
    transcript: Option<String>,
    #[serde(default)]
    error: Option<OaiError>,
}

#[derive(Debug, Deserialize)]
struct OaiError {
    #[serde(default)]
    code: Option<String>,
    #[serde(default)]
    message: Option<String>,
}

/// Parse an OpenAI Realtime transcription event JSON into transcript events.
pub fn parse_event(payload: &str, source: AudioSource) -> Result<Vec<TranscriptEvent>, SttError> {
    let ev: OaiEvent = serde_json::from_str(payload)
        .map_err(|e| SttError::Protocol(format!("invalid OpenAI JSON: {e}")))?;

    let ty = ev.ty.as_deref().unwrap_or("");

    if ty == "error" {
        if let Some(err) = &ev.error {
            let code = err.code.as_deref().unwrap_or("");
            let msg = err.message.as_deref().unwrap_or("unknown");
            return Err(match code {
                "invalid_api_key" | "authentication_error" => SttError::Auth,
                "rate_limit_exceeded" => SttError::Quota(msg.to_string()),
                _ => SttError::Provider(msg.to_string()),
            });
        }
        return Err(SttError::Provider(payload.to_string()));
    }

    match ty {
        "conversation.item.input_audio_transcription.delta" => {
            if let Some(text) = ev.delta {
                if !text.is_empty() {
                    return Ok(vec![TranscriptEvent::Partial {
                        text,
                        confidence: None,
                        source,
                    }]);
                }
            }
            Ok(Vec::new())
        }
        "conversation.item.input_audio_transcription.completed" => {
            if let Some(text) = ev.transcript {
                if !text.is_empty() {
                    return Ok(vec![TranscriptEvent::Final {
                        text,
                        confidence: None,
                        source,
                        words: Vec::new(),
                    }]);
                }
            }
            Ok(Vec::new())
        }
        _ => Ok(Vec::new()),
    }
}

/// Map an HTTP handshake status to SttError (OpenAI-specific).
pub fn map_handshake_status(status: u16) -> SttError {
    match status {
        401 | 403 => SttError::Auth,
        429 => SttError::Quota("rate limited".into()),
        s if (500..600).contains(&s) => SttError::Network(format!("server error {s}")),
        other => SttError::Protocol(format!("unexpected handshake status {other}")),
    }
}

/// Build the session.update JSON payload for transcription mode.
pub fn build_session_update(model: &str) -> String {
    format!(
        r#"{{"type":"session.update","session":{{"input_audio_transcription":{{"model":"{model}"}}}}}}"#
    )
}

// ========== Provider ==========

struct OpenAiState {
    connection: Mutex<ConnectionState>,
    closed: std::sync::atomic::AtomicBool,
}

impl OpenAiState {
    fn new() -> Self {
        Self {
            connection: Mutex::new(ConnectionState::Idle),
            closed: std::sync::atomic::AtomicBool::new(false),
        }
    }
}

/// OpenAI Realtime STT provider handle.
pub struct OpenAiRealtimeProvider {
    state: Arc<OpenAiState>,
    audio_tx: Option<UnboundedSender<Vec<u8>>>,
    events_rx: UnboundedReceiver<Result<TranscriptEvent, SttError>>,
    #[allow(dead_code)]
    source: AudioSource,
}

impl std::fmt::Debug for OpenAiRealtimeProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpenAiRealtimeProvider")
            .field("state", &*self.state.connection.lock())
            .finish()
    }
}

impl OpenAiRealtimeProvider {
    /// Build from pre-existing channels (for tests).
    pub fn from_channels(
        source: AudioSource,
        initial: ConnectionState,
        events_rx: UnboundedReceiver<Result<TranscriptEvent, SttError>>,
        audio_tx: UnboundedSender<Vec<u8>>,
    ) -> Self {
        let state = Arc::new(OpenAiState::new());
        *state.connection.lock() = initial;
        Self {
            state,
            audio_tx: Some(audio_tx),
            events_rx,
            source,
        }
    }

    /// Connect to OpenAI Realtime API and start supervisor task.
    pub async fn connect(
        cfg: OpenAiRealtimeConfig,
        _stt_cfg: SttConfig,
        source: AudioSource,
    ) -> Result<Self, SttError> {
        if cfg.api_key.is_empty() {
            return Err(SttError::Auth);
        }

        let state = Arc::new(OpenAiState::new());
        *state.connection.lock() = ConnectionState::Connecting;

        let (audio_tx, audio_rx) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();
        let (events_tx, events_rx) =
            tokio::sync::mpsc::unbounded_channel::<Result<TranscriptEvent, SttError>>();

        tokio::spawn(run_supervisor(
            cfg,
            source,
            state.clone(),
            audio_rx,
            events_tx,
        ));

        Ok(Self {
            state,
            audio_tx: Some(audio_tx),
            events_rx,
            source,
        })
    }
}

#[async_trait]
impl SttProvider for OpenAiRealtimeProvider {
    fn name(&self) -> &'static str {
        "openai_realtime"
    }

    fn connection_state(&self) -> ConnectionState {
        *self.state.connection.lock()
    }

    async fn send_audio(&self, chunk: &AudioChunk) -> Result<(), SttError> {
        if self.state.closed.load(std::sync::atomic::Ordering::Acquire) {
            return Err(SttError::NotActive);
        }
        let tx = self.audio_tx.as_ref().ok_or(SttError::NotActive)?;
        let bytes: Vec<u8> = bytemuck::cast_slice(&chunk.samples).to_vec();
        tx.send(bytes).map_err(|_| SttError::NotActive)?;
        Ok(())
    }

    async fn finalize(&self) -> Result<(), SttError> {
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
        *self.state.connection.lock() = ConnectionState::Closed;
        self.audio_tx = None;
        Ok(())
    }
}

// ========== Supervisor ==========

async fn run_supervisor(
    cfg: OpenAiRealtimeConfig,
    source: AudioSource,
    state: Arc<OpenAiState>,
    mut audio_rx: UnboundedReceiver<Vec<u8>>,
    events_tx: UnboundedSender<Result<TranscriptEvent, SttError>>,
) {
    let mut attempt: u32 = 0;
    loop {
        if state.closed.load(std::sync::atomic::Ordering::Acquire) {
            break;
        }

        *state.connection.lock() = if attempt == 0 {
            ConnectionState::Connecting
        } else {
            ConnectionState::Reconnecting { attempt }
        };

        match run_connection(&cfg, source, &state, &mut audio_rx, &events_tx).await {
            Ok(()) => {
                *state.connection.lock() = ConnectionState::Closed;
                return;
            }
            Err(e) => {
                if !e.is_retryable() {
                    tracing::error!(
                        provider = "openai_realtime",
                        api_key = %mask_api_key(&cfg.api_key),
                        error = ?e,
                        "fatal stream error"
                    );
                    let _ = events_tx.send(Err(e));
                    *state.connection.lock() = ConnectionState::Failed;
                    return;
                }
                attempt += 1;
                if attempt > MAX_RECONNECT_ATTEMPTS {
                    let _ = events_tx.send(Err(e));
                    *state.connection.lock() = ConnectionState::Failed;
                    return;
                }
                let delay = reconnect_delay(attempt - 1);
                tracing::warn!(
                    provider = "openai_realtime",
                    attempt,
                    delay_ms = delay.as_millis() as u64,
                    "reconnecting"
                );
                tokio::time::sleep(delay).await;
            }
        }
    }
}

async fn run_connection(
    cfg: &OpenAiRealtimeConfig,
    source: AudioSource,
    state: &Arc<OpenAiState>,
    audio_rx: &mut UnboundedReceiver<Vec<u8>>,
    events_tx: &UnboundedSender<Result<TranscriptEvent, SttError>>,
) -> Result<(), SttError> {
    let base = cfg.base_url.as_deref().unwrap_or("wss://api.openai.com");
    let url_str = format!(
        "{base}/v1/realtime?model={}&intent=transcription",
        cfg.model
    );
    let url: url::Url =
        url::Url::parse(&url_str).map_err(|e| SttError::Protocol(format!("bad url: {e}")))?;

    let mut request = url.as_str().into_client_request().map_err(map_ws_error)?;
    let headers = request.headers_mut();
    headers.insert(
        "Authorization",
        format!("Bearer {}", cfg.api_key)
            .parse()
            .map_err(|_| SttError::Auth)?,
    );
    headers.insert(
        "OpenAI-Beta",
        "realtime=v1"
            .parse()
            .map_err(|_| SttError::Protocol("bad header".into()))?,
    );

    let (ws_stream, _response) = tokio_tungstenite::connect_async(request)
        .await
        .map_err(map_ws_error)?;

    let (mut write, mut read) = ws_stream.split();

    // Send session.update to configure transcription mode (proceed optimistically).
    let session_update = build_session_update(&cfg.model);
    write
        .send(Message::text(session_update))
        .await
        .map_err(map_ws_error)?;

    *state.connection.lock() = ConnectionState::Connected;

    loop {
        if state.closed.load(std::sync::atomic::Ordering::Acquire) {
            let _ = write.send(Message::Close(None)).await;
            return Ok(());
        }

        tokio::select! {
            maybe_audio = audio_rx.recv() => {
                match maybe_audio {
                    Some(bytes) => {
                        if bytes.is_empty() {
                            let msg = r#"{"type":"input_audio_buffer.commit"}"#;
                            if let Err(e) = write.send(Message::text(msg)).await {
                                return Err(map_ws_error(e));
                            }
                        } else {
                            let samples: &[i16] = bytemuck::cast_slice(&bytes);
                            let resampled = resample_16k_to_24k(samples);
                            let resampled_bytes: &[u8] = bytemuck::cast_slice(&resampled);
                            let b64 = base64::engine::general_purpose::STANDARD
                                .encode(resampled_bytes);
                            let frame = format!(
                                r#"{{"type":"input_audio_buffer.append","audio":"{b64}"}}"#
                            );
                            if let Err(e) = write.send(Message::text(frame)).await {
                                return Err(map_ws_error(e));
                            }
                        }
                    }
                    None => {
                        let _ = write.send(Message::Close(None)).await;
                        return Ok(());
                    }
                }
            }
            maybe_frame = read.next() => {
                match maybe_frame {
                    Some(Ok(Message::Text(text))) => {
                        match parse_event(&text, source) {
                            Ok(events) => {
                                for ev in events {
                                    if events_tx.send(Ok(ev)).is_err() {
                                        return Ok(());
                                    }
                                }
                            }
                            Err(e) if e.is_retryable() => return Err(e),
                            Err(e) => {
                                let _ = events_tx.send(Err(e));
                            }
                        }
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
                    Some(Ok(_)) => {}
                    Some(Err(e)) => return Err(map_ws_error(e)),
                    None => return Err(SttError::Network("WS stream ended".into())),
                }
            }
        }
    }
}

// ========== Tests ==========

#[cfg(test)]
#[allow(clippy::result_large_err)]
mod tests {
    use std::time::Duration;

    use super::*;
    use cue_core::pcm::{AudioSource, SampleRate};
    use std::net::SocketAddr;
    use std::sync::mpsc as stdmpsc;
    use tokio::net::TcpListener;
    use tokio_tungstenite::tungstenite::handshake::server::{ErrorResponse, Request, Response};

    #[test]
    fn parse_event_delta_yields_partial() {
        let payload =
            r#"{"type":"conversation.item.input_audio_transcription.delta","delta":"hello "}"#;
        let events = parse_event(payload, AudioSource::Microphone).unwrap();
        assert_eq!(events.len(), 1);
        assert!(matches!(&events[0], TranscriptEvent::Partial { text, .. } if text == "hello "));
    }

    #[test]
    fn parse_event_completed_yields_final() {
        let payload = r#"{"type":"conversation.item.input_audio_transcription.completed","transcript":"hello world"}"#;
        let events = parse_event(payload, AudioSource::Microphone).unwrap();
        assert_eq!(events.len(), 1);
        assert!(matches!(&events[0], TranscriptEvent::Final { text, .. } if text == "hello world"));
    }

    #[test]
    fn parse_event_error_auth() {
        let payload = r#"{"type":"error","error":{"code":"invalid_api_key","message":"bad key"}}"#;
        let err = parse_event(payload, AudioSource::Microphone).unwrap_err();
        assert!(matches!(err, SttError::Auth));
    }

    #[test]
    fn parse_event_error_rate_limit() {
        let payload =
            r#"{"type":"error","error":{"code":"rate_limit_exceeded","message":"slow down"}}"#;
        let err = parse_event(payload, AudioSource::Microphone).unwrap_err();
        assert!(matches!(err, SttError::Quota(_)));
    }

    #[test]
    fn parse_event_unknown_type_ignored() {
        let payload = r#"{"type":"session.created","session":{}}"#;
        let events = parse_event(payload, AudioSource::Microphone).unwrap();
        assert!(events.is_empty());
    }

    #[test]
    fn parse_event_malformed_json() {
        let err = parse_event("not json", AudioSource::Microphone).unwrap_err();
        assert!(matches!(err, SttError::Protocol(_)));
    }

    #[test]
    fn parse_event_old_event_names_ignored() {
        // Old event names must NOT produce transcript events
        let payload = r#"{"type":"response.audio_transcript.delta","delta":"old"}"#;
        let events = parse_event(payload, AudioSource::Microphone).unwrap();
        assert!(events.is_empty());

        let payload = r#"{"type":"response.audio_transcript.done","transcript":"old"}"#;
        let events = parse_event(payload, AudioSource::Microphone).unwrap();
        assert!(events.is_empty());
    }

    #[test]
    fn resample_16k_to_24k_ratio() {
        let input = vec![0i16; 320]; // 20ms @ 16kHz
        let output = resample_16k_to_24k(&input);
        assert_eq!(output.len(), 480); // 20ms @ 24kHz
    }

    #[test]
    fn resample_16k_to_24k_preserves_dc() {
        let input = vec![1000i16; 100];
        let output = resample_16k_to_24k(&input);
        assert!(output.iter().all(|&s| (s - 1000).unsigned_abs() <= 1));
    }

    #[test]
    fn default_model_is_transcription_model() {
        let cfg = OpenAiRealtimeConfig::default();
        assert_eq!(cfg.model, "gpt-4o-mini-transcribe");
    }

    #[test]
    fn build_session_update_contains_model() {
        let json = build_session_update("gpt-4o-mini-transcribe");
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["type"], "session.update");
        assert_eq!(
            v["session"]["input_audio_transcription"]["model"],
            "gpt-4o-mini-transcribe"
        );
    }

    // ========== Mock WebSocket tests ==========

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

    #[tokio::test]
    async fn connect_sends_session_update_first() {
        let (tx, _rx) = stdmpsc::channel();
        let (frame_tx, frame_rx) = stdmpsc::channel();
        let addr = start_mock_server(tx, move |mut ws| async move {
            // Read the first frame from the client — should be session.update
            if let Some(Ok(msg)) = ws.next().await {
                let text = msg.into_text().unwrap_or_default();
                let _ = frame_tx.send(text);
            }
            let _ = ws.close(None).await;
        })
        .await;

        let cfg = OpenAiRealtimeConfig {
            api_key: "sk-test-key".into(),
            base_url: Some(format!("ws://{addr}")),
            model: "gpt-4o-mini-transcribe".into(),
        };
        let mut provider =
            OpenAiRealtimeProvider::connect(cfg, SttConfig::default(), AudioSource::Microphone)
                .await
                .unwrap();

        // Wait for connection to close
        let _ = tokio::time::timeout(Duration::from_secs(3), provider.next_event()).await;

        let first_frame =
            tokio::task::spawn_blocking(move || frame_rx.recv_timeout(Duration::from_secs(2)))
                .await
                .unwrap()
                .unwrap();

        let v: serde_json::Value = serde_json::from_str(&first_frame).unwrap();
        assert_eq!(v["type"], "session.update");
        assert_eq!(
            v["session"]["input_audio_transcription"]["model"],
            "gpt-4o-mini-transcribe"
        );
    }

    #[tokio::test]
    async fn connect_sends_bearer_auth_header() {
        let (tx, rx) = stdmpsc::channel();
        let addr = start_mock_server(tx, |mut ws| async move {
            // Consume session.update then send a transcript
            let _ = ws.next().await;
            let _ = ws
                .send(Message::text(
                    r#"{"type":"conversation.item.input_audio_transcription.completed","transcript":"hi"}"#,
                ))
                .await;
            let _ = ws.close(None).await;
        })
        .await;

        let cfg = OpenAiRealtimeConfig {
            api_key: "sk-test-key-1234".into(),
            base_url: Some(format!("ws://{addr}")),
            ..Default::default()
        };
        let mut provider =
            OpenAiRealtimeProvider::connect(cfg, SttConfig::default(), AudioSource::Microphone)
                .await
                .unwrap();

        let _ = tokio::time::timeout(Duration::from_secs(5), provider.next_event())
            .await
            .expect("timeout")
            .expect("closed")
            .expect("error");

        let captured = tokio::task::spawn_blocking(move || rx.recv_timeout(Duration::from_secs(2)))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(captured.as_deref(), Some("Bearer sk-test-key-1234"));
    }

    #[tokio::test]
    async fn connect_delivers_delta_then_completed() {
        let (tx, _rx) = stdmpsc::channel();
        let addr = start_mock_server(tx, |mut ws| async move {
            // Consume session.update
            let _ = ws.next().await;
            let _ = ws
                .send(Message::text(
                    r#"{"type":"conversation.item.input_audio_transcription.delta","delta":"hel"}"#,
                ))
                .await;
            let _ = ws
                .send(Message::text(
                    r#"{"type":"conversation.item.input_audio_transcription.completed","transcript":"hello"}"#,
                ))
                .await;
            let _ = ws.close(None).await;
        })
        .await;

        let cfg = OpenAiRealtimeConfig {
            api_key: "sk-test".into(),
            base_url: Some(format!("ws://{addr}")),
            ..Default::default()
        };
        let mut provider =
            OpenAiRealtimeProvider::connect(cfg, SttConfig::default(), AudioSource::Microphone)
                .await
                .unwrap();

        let e1 = tokio::time::timeout(Duration::from_secs(5), provider.next_event())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert!(matches!(e1, TranscriptEvent::Partial { ref text, .. } if text == "hel"));

        let e2 = tokio::time::timeout(Duration::from_secs(5), provider.next_event())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert!(matches!(e2, TranscriptEvent::Final { ref text, .. } if text == "hello"));
    }

    #[tokio::test]
    async fn audio_sent_as_json_text_frame_with_base64() {
        let (tx, _rx) = stdmpsc::channel();
        let (sig_tx, sig_rx) = stdmpsc::channel();
        let addr = start_mock_server(tx, move |mut ws| async move {
            // First frame is session.update — skip it
            let _ = ws.next().await;
            // Second frame should be audio
            if let Some(Ok(msg)) = ws.next().await {
                let is_text = msg.is_text();
                let data = msg.into_text().unwrap_or_default();
                let has_audio_field =
                    data.contains("input_audio_buffer.append") && data.contains("\"audio\"");
                let _ = sig_tx.send((is_text, has_audio_field));
            }
            let _ = ws.close(None).await;
        })
        .await;

        let cfg = OpenAiRealtimeConfig {
            api_key: "sk-test".into(),
            base_url: Some(format!("ws://{addr}")),
            ..Default::default()
        };
        let provider =
            OpenAiRealtimeProvider::connect(cfg, SttConfig::default(), AudioSource::Microphone)
                .await
                .unwrap();

        tokio::time::sleep(Duration::from_millis(100)).await;

        let chunk = AudioChunk {
            source: AudioSource::Microphone,
            sample_rate: SampleRate::SR_16K,
            samples: vec![0i16; 320],
            captured_at_ms: 0,
        };
        provider.send_audio(&chunk).await.unwrap();

        let (is_text, has_audio) =
            tokio::task::spawn_blocking(move || sig_rx.recv_timeout(Duration::from_secs(3)))
                .await
                .unwrap()
                .unwrap();
        assert!(is_text, "audio must be sent as text frame (JSON)");
        assert!(
            has_audio,
            "frame must contain input_audio_buffer.append with audio field"
        );
    }

    #[tokio::test]
    async fn connect_rejects_empty_api_key() {
        let cfg = OpenAiRealtimeConfig::default();
        let err =
            OpenAiRealtimeProvider::connect(cfg, SttConfig::default(), AudioSource::Microphone)
                .await
                .unwrap_err();
        assert!(matches!(err, SttError::Auth));
    }

    #[tokio::test]
    async fn error_event_auth_mapping() {
        let (tx, _rx) = stdmpsc::channel();
        let addr = start_mock_server(tx, |mut ws| async move {
            let _ = ws.next().await; // session.update
            let _ = ws
                .send(Message::text(
                    r#"{"type":"error","error":{"code":"invalid_api_key","message":"bad"}}"#,
                ))
                .await;
            let _ = ws.close(None).await;
        })
        .await;

        let cfg = OpenAiRealtimeConfig {
            api_key: "sk-test".into(),
            base_url: Some(format!("ws://{addr}")),
            ..Default::default()
        };
        let mut provider =
            OpenAiRealtimeProvider::connect(cfg, SttConfig::default(), AudioSource::Microphone)
                .await
                .unwrap();

        let ev = tokio::time::timeout(Duration::from_secs(5), provider.next_event())
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(ev, Err(SttError::Auth)));
    }

    #[tokio::test]
    async fn error_event_quota_mapping() {
        let (tx, _rx) = stdmpsc::channel();
        let addr = start_mock_server(tx, |mut ws| async move {
            let _ = ws.next().await; // session.update
            let _ = ws
                .send(Message::text(
                    r#"{"type":"error","error":{"code":"rate_limit_exceeded","message":"slow"}}"#,
                ))
                .await;
            let _ = ws.close(None).await;
        })
        .await;

        let cfg = OpenAiRealtimeConfig {
            api_key: "sk-test".into(),
            base_url: Some(format!("ws://{addr}")),
            ..Default::default()
        };
        let mut provider =
            OpenAiRealtimeProvider::connect(cfg, SttConfig::default(), AudioSource::Microphone)
                .await
                .unwrap();

        let ev = tokio::time::timeout(Duration::from_secs(5), provider.next_event())
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(ev, Err(SttError::Quota(_))));
    }
}
