//! Speech-to-text (STT) provider trait + common types.
//!
//! Concrete providers live in `cue-daemon::stt::*` (Deepgram, Soniox, OpenAI,
//! AssemblyAI, etc.) and implement `SttProvider` so the routing layer can
//! swap them at runtime without caring about WebSocket framing, auth, or
//! error models.
//!
//! Design goals reflected here:
//!
//! - **Streaming by default** — every provider we integrate is streaming-
//!   first. One-shot REST calls are modeled as `finalize()` returning a
//!   terminal transcript event.
//! - **Partial + final semantics explicit** — `TranscriptEvent::Partial`
//!   mirrors Deepgram/Soniox `is_final=false`, `TranscriptEvent::Final`
//!   mirrors `is_final=true`. The stable-partial detector (Phase 3 `L3`
//!   task in the master plan) consumes the partial stream.
//! - **Connection state machine surfaced** — the UI can show a
//!   reconnecting banner by listening on the `ConnectionState` events.
//! - **Error classification** — providers tag errors so the router knows
//!   which to retry (network) vs which to surface to the user (auth).

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::pcm::{AudioChunk, AudioSource, SampleRate};

/// A transcription event emitted by a running STT session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum TranscriptEvent {
    /// Provisional transcript from a still-open utterance. Replaced as the
    /// provider receives more audio. Latency-sensitive consumers should
    /// render this live.
    Partial {
        text: String,
        /// Provider-reported confidence in `[0.0, 1.0]` — not all providers
        /// emit this for partials; `None` when unavailable.
        confidence: Option<f32>,
        /// Source this transcript came from (mic vs system).
        source: AudioSource,
    },
    /// Finalized transcript for a completed utterance. The provider has
    /// committed and will not replace this with a subsequent event.
    Final {
        text: String,
        confidence: Option<f32>,
        source: AudioSource,
        /// Word-level timing if the provider supplies it. Empty if not.
        words: Vec<WordTiming>,
    },
    /// Provider sent a speaker label. Only some providers emit this
    /// (Deepgram diarization, AssemblyAI, etc.). When diarization is
    /// unavailable we rely on `AudioSource` to infer who spoke.
    SpeakerLabel {
        /// Opaque provider-assigned speaker id (0, 1, 2, ...).
        speaker: u32,
        source: AudioSource,
    },
    /// A structural boundary indicating a pause in speech or speaker turn.
    /// This signals downstream components that the previous utterance is complete
    /// and ready for post-processing or formatting.
    Boundary {
        source: AudioSource,
    },
}

/// Word-level timing information (seconds relative to stream start).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WordTiming {
    pub word: String,
    pub start_s: f32,
    pub end_s: f32,
    pub confidence: Option<f32>,
}

/// High-level connection state of an STT session. Dashboard listens on
/// this via a Tauri event to show banners.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionState {
    /// Not yet started.
    Idle,
    /// Establishing socket / handshake.
    Connecting,
    /// Connected and accepting audio.
    Connected,
    /// Lost connection, will auto-retry. `attempt` counts from 1.
    Reconnecting { attempt: u32 },
    /// Fatal error — caller must either recreate the provider or escalate.
    Failed,
    /// Cleanly closed by caller.
    Closed,
}

/// Error model for STT operations. Classification drives routing.
#[derive(Debug, Error)]
pub enum SttError {
    #[error("authentication failed (invalid or revoked API key)")]
    Auth,
    #[error("quota / rate-limit exceeded: {0}")]
    Quota(String),
    #[error("transient network error: {0}")]
    Network(String),
    #[error("protocol violation: {0}")]
    Protocol(String),
    #[error("provider returned error: {0}")]
    Provider(String),
    #[error("invalid audio format: {0}")]
    AudioFormat(String),
    #[error("session not active")]
    NotActive,
}

impl SttError {
    /// Whether the router should transparently retry this error with the
    /// same provider. `false` for auth/quota/audio-format (caller action
    /// required), `true` for network/protocol (transient).
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            SttError::Network(_) | SttError::Protocol(_) | SttError::Provider(_)
        )
    }

    /// Whether this error should trigger an immediate failover to the
    /// next STT provider in the fallback chain.
    pub fn should_failover(&self) -> bool {
        matches!(self, SttError::Auth | SttError::Quota(_))
    }
}

/// Configuration passed when creating a provider instance.
#[derive(Debug, Clone)]
pub struct SttConfig {
    pub sample_rate: SampleRate,
    /// Optional language hint (BCP-47, e.g. "en-US"). `None` = auto.
    pub language: Option<String>,
    /// Optional custom vocabulary / boost hints — Deepgram `keywords`,
    /// Soniox `dictionary`, etc. Provider translates as appropriate.
    pub vocabulary: Vec<String>,
    /// Whether to request interim (partial) results. Providers that can't
    /// disable this ignore the flag.
    pub emit_partials: bool,
    /// Which audio source this session carries. Used for TranscriptEvent
    /// tagging when the provider doesn't attach it itself.
    pub source: AudioSource,
}

impl Default for SttConfig {
    fn default() -> Self {
        Self {
            sample_rate: SampleRate::SR_16K,
            language: None,
            vocabulary: Vec::new(),
            emit_partials: true,
            source: AudioSource::Microphone,
        }
    }
}

/// The core trait every STT integration implements.
///
/// Providers hold persistent connections internally; `send_audio` and
/// `finalize` are cheap writes into those connections. `next_event` is the
/// consumer side, yielding transcription events as the provider emits them.
///
/// Impls MUST be `Send + Sync` so they can live in a Tauri managed state.
#[async_trait]
pub trait SttProvider: Send + Sync {
    /// Stable identifier for routing / telemetry (e.g. `"deepgram_nova3"`).
    fn name(&self) -> &'static str;

    /// Current connection state — polled by the UI for live banners.
    fn connection_state(&self) -> ConnectionState;

    /// Push a PCM16 audio chunk into the provider. Must be fast (<1 ms),
    /// non-blocking. Internal queue handles backpressure.
    async fn send_audio(&self, chunk: &AudioChunk) -> Result<(), SttError>;

    /// Signal end of utterance. Some providers use this to force a final.
    /// Others ignore. Always safe to call.
    async fn finalize(&self) -> Result<(), SttError>;

    /// Await the next transcription event. Cancel-safe. Returns `None` when
    /// the session is closed.
    async fn next_event(&mut self) -> Option<Result<TranscriptEvent, SttError>>;

    /// Cleanly close the session. Connection goes to `Closed`.
    async fn close(&mut self) -> Result<(), SttError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stt_error_retry_classification() {
        assert!(SttError::Network("oops".into()).is_retryable());
        assert!(SttError::Protocol("bad frame".into()).is_retryable());
        assert!(!SttError::Auth.is_retryable());
        assert!(!SttError::Quota("limit".into()).is_retryable());
    }

    #[test]
    fn stt_error_failover_classification() {
        assert!(SttError::Auth.should_failover());
        assert!(SttError::Quota("limit".into()).should_failover());
        assert!(!SttError::Network("oops".into()).should_failover());
    }

    #[test]
    fn transcript_event_serde_roundtrip_partial() {
        let e = TranscriptEvent::Partial {
            text: "hello world".into(),
            confidence: Some(0.82),
            source: AudioSource::Microphone,
        };
        let s = serde_json::to_string(&e).unwrap();
        let back: TranscriptEvent = serde_json::from_str(&s).unwrap();
        assert_eq!(e, back);
    }

    #[test]
    fn connection_state_serde_roundtrip_with_reconnect_attempt() {
        let s = ConnectionState::Reconnecting { attempt: 3 };
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains("reconnecting"));
        let back: ConnectionState = serde_json::from_str(&json).unwrap();
        assert_eq!(s, back);
    }

    #[test]
    fn stt_config_default_uses_16k_partials() {
        let c = SttConfig::default();
        assert_eq!(c.sample_rate, SampleRate::SR_16K);
        assert!(c.emit_partials);
        assert_eq!(c.source, AudioSource::Microphone);
    }
}
