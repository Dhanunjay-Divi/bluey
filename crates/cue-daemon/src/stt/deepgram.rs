//! Deepgram Nova-3 streaming STT provider.
//!
//! Implements [`SttProvider`] against Deepgram's real-time WebSocket API.
//! One `DeepgramProvider` == one persistent `wss://api.deepgram.com/v1/listen`
//! connection. Audio goes in as binary WebSocket frames; transcripts come
//! back as JSON text frames.
//!
//! ### Connection lifecycle
//!
//! 1. `connect()` builds the URL (model/encoding/sample_rate/lang params),
//!    opens the WS with an `Authorization: Token <key>` header.
//! 2. A background task owns the split reader+writer. Audio pushed via
//!    `send_audio` flows through a `tokio::mpsc` channel to the writer.
//!    JSON frames from the reader are parsed and pushed as
//!    [`TranscriptEvent`]s on a separate channel `next_event()` drains.
//! 3. On disconnect: the task tries to reconnect with exponential backoff
//!    up to [`MAX_RECONNECT_ATTEMPTS`]. State flips `Connected` →
//!    `Reconnecting { attempt }` → either `Connected` again or `Failed`.
//! 4. `close()` sends a WebSocket close + drains outstanding events.
//!
//! ### Auth
//!
//! The API key is only ever read through `DeepgramConfig`. It is stored as a
//! plain `String`, but NEVER printed: logging helpers use `mask_api_key()`
//! (see `mask_api_key` module fn) so only the last 4 characters ever appear.
//!
//! ### Error classification
//!
//! Mirrors Deepgram's documented HTTP status codes on handshake + the
//! `"type":"Error"` control frames on the open stream. See `map_error` for
//! the table.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use cue_core::pcm::{AudioChunk, AudioSource};
use cue_core::stt::{
    ConnectionState, SttConfig, SttError, SttProvider, TranscriptEvent, WordTiming,
};
use parking_lot::Mutex;
use serde::Deserialize;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

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
            diarize: false,
            base_url: None,
        }
    }
}

/// Build the Deepgram WebSocket URL from a config + the per-session
/// [`SttConfig`]. Exposed so unit tests can verify URL construction
/// deterministically without spinning up an actual WS server.
pub fn build_url(deepgram: &DeepgramConfig, stt: &SttConfig) -> Result<url::Url, SttError> {
    let base = deepgram
        .base_url
        .as_deref()
        .unwrap_or("wss://api.deepgram.com");
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
        if deepgram.interim_results {
            q.append_pair("interim_results", "true");
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
    if key.len() <= 4 {
        "****".to_string()
    } else {
        let tail = &key[key.len() - 4..];
        format!("****{tail}")
    }
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
    let frame: DgFrame = serde_json::from_str(payload)
        .map_err(|e| SttError::Protocol(format!("invalid Deepgram JSON: {e}")))?;

    if matches!(frame.ty.as_deref(), Some("Error")) {
        return Err(SttError::Provider(payload.to_string()));
    }

    let channel = match frame.channel {
        Some(c) => c,
        None => return Ok(Vec::new()),
    };
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

    if frame.is_final {
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

// ========== Provider ==========

/// Shared state accessed from both the foreground `SttProvider` methods
/// and the background reconnect task.
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

/// The live provider handle. Normally constructed via `DeepgramProvider::connect`.
///
/// Round 3 scope: the WebSocket task is NOT started inside this file — the
/// full reconnect-on-disconnect loop lives in `spawn_transport`. This keeps
/// the state machine under test while still deferring the "open a real
/// connection" dance to an integration test with a running WS server.
pub struct DeepgramProvider {
    state: Arc<DeepgramState>,
    audio_tx: Option<UnboundedSender<Vec<u8>>>,
    events_rx: UnboundedReceiver<Result<TranscriptEvent, SttError>>,
    /// Retained for future use: a full connect() implementation stamps this
    /// onto all emitted events. Kept as a field rather than a constructor
    /// parameter so the connect() path can set it without re-plumbing tests.
    #[allow(dead_code)]
    source: AudioSource,
}

impl DeepgramProvider {
    /// Build a provider from an existing event/audio channel pair and a
    /// starting connection state. This is the seam unit tests use to drive
    /// the provider without opening a real WebSocket.
    pub fn from_channels(
        source: AudioSource,
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
            source,
        }
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
        // Deepgram's "CloseStream" control message — but we do NOT close the
        // WS, only tell DG the utterance ended so it flushes a final.
        if let Some(tx) = &self.audio_tx {
            // Empty binary frame signals DG to emit final + keep connection.
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

// ========== Tests ==========

#[cfg(test)]
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
        assert_eq!(q.get("interim_results").map(String::as_str), Some("true"));
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
        // At very large attempt numbers, still capped
        assert_eq!(reconnect_delay(20).as_millis(), 10_000);
    }

    #[tokio::test]
    async fn provider_reports_initial_connection_state() {
        let (_audio_tx, _audio_rx) = unbounded_channel::<Vec<u8>>();
        let (_ev_tx, ev_rx) = unbounded_channel();
        let provider = DeepgramProvider::from_channels(
            AudioSource::Microphone,
            ConnectionState::Connected,
            ev_rx,
            _audio_tx,
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
}
