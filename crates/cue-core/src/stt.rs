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

pub mod agreement;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::pcm::{AudioChunk, AudioSource, SampleRate};
use agreement::TranscriptAgreementUpdate;

/// A transcription event emitted by a running STT session.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
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
}

impl std::fmt::Debug for TranscriptEvent {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TranscriptEvent::Partial {
                text,
                confidence,
                source,
            } => formatter
                .debug_struct("TranscriptEvent::Partial")
                .field("text_chars", &text.chars().count())
                .field("confidence", confidence)
                .field("source", source)
                .finish(),
            TranscriptEvent::Final {
                text,
                confidence,
                source,
                words,
            } => formatter
                .debug_struct("TranscriptEvent::Final")
                .field("text_chars", &text.chars().count())
                .field("confidence", confidence)
                .field("source", source)
                .field("word_count", &words.len())
                .finish(),
            TranscriptEvent::SpeakerLabel { speaker, source } => formatter
                .debug_struct("TranscriptEvent::SpeakerLabel")
                .field("speaker", speaker)
                .field("source", source)
                .finish(),
        }
    }
}

/// Word-level timing information (seconds relative to stream start).
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct WordTiming {
    pub word: String,
    pub start_s: f32,
    pub end_s: f32,
    pub confidence: Option<f32>,
}

impl std::fmt::Debug for WordTiming {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("WordTiming")
            .field("word_chars", &self.word.chars().count())
            .field("start_s", &self.start_s)
            .field("end_s", &self.end_s)
            .field("confidence", &self.confidence)
            .finish()
    }
}

/// Internal sidecar for stability-aware consumers. `event` remains the exact
/// legacy wire event; agreement metadata is intentionally carried beside it
/// so existing serialization and downstream matches remain compatible.
#[derive(Clone, PartialEq)]
pub struct StableTranscriptEvent {
    pub event: TranscriptEvent,
    pub agreement: Option<TranscriptAgreementUpdate>,
}

impl std::fmt::Debug for StableTranscriptEvent {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("StableTranscriptEvent")
            .field("event", &self.event)
            .field("agreement_present", &self.agreement.is_some())
            .finish()
    }
}

impl StableTranscriptEvent {
    pub fn legacy(event: TranscriptEvent) -> Self {
        Self {
            event,
            agreement: None,
        }
    }

    pub fn with_agreement(event: TranscriptEvent, agreement: TranscriptAgreementUpdate) -> Self {
        Self {
            event,
            agreement: Some(agreement),
        }
    }

    pub fn into_legacy(self) -> TranscriptEvent {
        self.event
    }

    pub fn is_partial(&self) -> bool {
        matches!(self.event, TranscriptEvent::Partial { .. })
    }
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
///
/// String payloads are retained for typed internal handling, but neither
/// `Display` nor `Debug` renders them. Provider responses can contain echoed
/// transcripts, credentials, request URLs, or arbitrary remote text and must
/// never become local diagnostics by formatting this error.
#[derive(Error)]
pub enum SttError {
    #[error("authentication failed (invalid or revoked API key)")]
    Auth,
    #[error("quota / rate-limit exceeded")]
    Quota(String),
    #[error("transient network error")]
    Network(String),
    #[error("protocol violation")]
    Protocol(String),
    #[error("provider returned error")]
    Provider(String),
    #[error("invalid audio format")]
    AudioFormat(String),
    #[error("session not active")]
    NotActive,
}

impl std::fmt::Debug for SttError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SttError")
            .field("category", &self.diagnostic_category())
            .finish()
    }
}

impl SttError {
    /// Closed, metadata-only category safe for local logs and support bundles.
    pub const fn diagnostic_category(&self) -> &'static str {
        match self {
            SttError::Auth => "authentication",
            SttError::Quota(_) => "quota",
            SttError::Network(_) => "network",
            SttError::Protocol(_) => "protocol",
            SttError::Provider(_) => "provider",
            SttError::AudioFormat(_) => "audio_format",
            SttError::NotActive => "not_active",
        }
    }

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
#[derive(Clone)]
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

impl std::fmt::Debug for SttConfig {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SttConfig")
            .field("sample_rate", &self.sample_rate)
            .field("language", &self.language.as_ref().map(|_| "[configured]"))
            .field("vocabulary_entries", &self.vocabulary.len())
            .field("emit_partials", &self.emit_partials)
            .field("source", &self.source)
            .finish()
    }
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

    /// Stability-aware counterpart to [`SttProvider::next_event`]. Providers
    /// without local agreement metadata retain the legacy behavior by default.
    async fn next_stable_event(&mut self) -> Option<Result<StableTranscriptEvent, SttError>> {
        self.next_event()
            .await
            .map(|result| result.map(StableTranscriptEvent::legacy))
    }

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
    fn stt_error_formatting_never_renders_private_details() {
        const SENTINEL: &str = "PRIVATE_TRANSCRIPT token=stt-secret https://secret.invalid";
        let errors = [
            SttError::Quota(SENTINEL.into()),
            SttError::Network(SENTINEL.into()),
            SttError::Protocol(SENTINEL.into()),
            SttError::Provider(SENTINEL.into()),
            SttError::AudioFormat(SENTINEL.into()),
        ];

        for error in errors {
            let display = error.to_string();
            let debug = format!("{error:?}");
            assert!(!display.contains(SENTINEL));
            assert!(!debug.contains(SENTINEL));
            assert!(!display.contains("stt-secret"));
            assert!(!debug.contains("PRIVATE_TRANSCRIPT"));
            assert!(matches!(
                error.diagnostic_category(),
                "quota" | "network" | "protocol" | "provider" | "audio_format"
            ));
        }
    }

    #[test]
    fn transcript_and_config_debug_render_only_closed_metadata() {
        const SENTINEL: &str =
            "PRIVATE_TRANSCRIPT token=debug-secret https://secret.invalid/private/path";
        let word = WordTiming {
            word: SENTINEL.into(),
            start_s: 0.0,
            end_s: 1.0,
            confidence: Some(0.9),
        };
        let event = TranscriptEvent::Final {
            text: SENTINEL.into(),
            confidence: Some(0.9),
            source: AudioSource::Microphone,
            words: vec![word.clone()],
        };
        let partial = TranscriptEvent::Partial {
            text: SENTINEL.into(),
            confidence: Some(0.5),
            source: AudioSource::System,
        };
        let agreement = TranscriptAgreementUpdate {
            generation_id: 1,
            segment_id: 1,
            revision: 1,
            phase: agreement::TranscriptAgreementPhase::Final,
            committed_text: SENTINEL.into(),
            tentative_text: SENTINEL.into(),
            newly_committed_text: SENTINEL.into(),
            stable_ticks: 1,
            final_corrected_committed_prefix: false,
            truncated: false,
        };
        let stable = StableTranscriptEvent::with_agreement(event.clone(), agreement);
        let config = SttConfig {
            language: Some(SENTINEL.into()),
            vocabulary: vec![SENTINEL.into()],
            ..Default::default()
        };

        for debug in [
            format!("{word:?}"),
            format!("{partial:?}"),
            format!("{event:?}"),
            format!("{stable:?}"),
            format!("{config:?}"),
        ] {
            assert!(!debug.contains(SENTINEL));
            assert!(!debug.contains("debug-secret"));
            assert!(!debug.contains("secret.invalid"));
            assert!(!debug.contains("/private/path"));
        }
    }

    #[test]
    fn transcript_event_serde_roundtrip_partial() {
        let e = TranscriptEvent::Partial {
            text: "hello world".into(),
            confidence: Some(0.82),
            source: AudioSource::Microphone,
        };
        let s = serde_json::to_string(&e).unwrap();
        assert_eq!(
            s,
            r#"{"kind":"partial","text":"hello world","confidence":0.82,"source":"microphone"}"#
        );
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
