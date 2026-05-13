//! MockStt — an in-memory test double for `SttProvider`.
//!
//! Drives integration tests for the capture → VAD → STT pipeline without
//! network, auth, or real providers. Tests create a `MockStt` + its paired
//! `MockSttControl` handle. The provider is handed to pipeline code;
//! the control handle is used by the test to script provider behavior
//! (queue transcript events, flip connection state, count received chunks).

use std::sync::Arc;

use async_trait::async_trait;
use cue_core::pcm::{AudioChunk, AudioSource};
use cue_core::stt::{
    ConnectionState, SttConfig, SttError, SttProvider, TranscriptEvent, WordTiming,
};
use parking_lot::Mutex;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};

#[derive(Default)]
struct MockState {
    connection: MockConnectionState,
    chunks_received: usize,
    bytes_received: usize,
    finalized: bool,
    closed: bool,
}

#[derive(Clone, Copy)]
struct MockConnectionState(ConnectionState);

impl Default for MockConnectionState {
    fn default() -> Self {
        MockConnectionState(ConnectionState::Connected)
    }
}

/// Inner shared state. Split out so both the provider and the control
/// handle can read/write via `Arc<Mutex<..>>`.
pub struct MockStt {
    inner: Arc<Mutex<MockState>>,
    events_rx: UnboundedReceiver<Result<TranscriptEvent, SttError>>,
    /// Retained so we can stay in a stable "Closed" state after `close()`
    /// even if the control tx was dropped prematurely.
    _cfg: SttConfig,
}

/// Test-side handle to drive a [`MockStt`] from outside the pipeline.
#[derive(Clone)]
pub struct MockSttControl {
    inner: Arc<Mutex<MockState>>,
    events_tx: UnboundedSender<Result<TranscriptEvent, SttError>>,
    source: AudioSource,
}

impl MockStt {
    /// Build a `(provider, control)` pair for a test.
    pub fn new(cfg: SttConfig) -> (Self, MockSttControl) {
        let (tx, rx) = unbounded_channel();
        let inner = Arc::new(Mutex::new(MockState::default()));
        let ctrl = MockSttControl {
            inner: inner.clone(),
            events_tx: tx,
            source: cfg.source,
        };
        let provider = Self {
            inner,
            events_rx: rx,
            _cfg: cfg,
        };
        (provider, ctrl)
    }
}

impl MockSttControl {
    /// Emit a partial transcript, tagged with the provider's configured source.
    pub fn emit_partial(&self, text: &str) {
        let _ = self.events_tx.send(Ok(TranscriptEvent::Partial {
            text: text.to_string(),
            confidence: Some(0.9),
            source: self.source,
        }));
    }

    /// Emit a final transcript with optional word-level timing.
    pub fn emit_final(&self, text: &str, words: Vec<WordTiming>) {
        let _ = self.events_tx.send(Ok(TranscriptEvent::Final {
            text: text.to_string(),
            confidence: Some(0.95),
            source: self.source,
            words,
        }));
    }

    /// Emit an error (forces the pipeline to observe failure semantics).
    pub fn emit_error(&self, err: SttError) {
        let _ = self.events_tx.send(Err(err));
    }

    /// Flip the connection state. Pipeline queries this via `connection_state()`.
    pub fn set_connection_state(&self, state: ConnectionState) {
        self.inner.lock().connection = MockConnectionState(state);
    }

    /// How many audio chunks has the provider seen?
    pub fn chunks_received(&self) -> usize {
        self.inner.lock().chunks_received
    }

    /// How many bytes total?
    pub fn bytes_received(&self) -> usize {
        self.inner.lock().bytes_received
    }

    /// Has finalize() been called?
    pub fn was_finalized(&self) -> bool {
        self.inner.lock().finalized
    }

    /// Has close() been called?
    pub fn was_closed(&self) -> bool {
        self.inner.lock().closed
    }
}

#[async_trait]
impl SttProvider for MockStt {
    fn name(&self) -> &'static str {
        "mock"
    }

    fn connection_state(&self) -> ConnectionState {
        self.inner.lock().connection.0
    }

    async fn send_audio(&self, chunk: &AudioChunk) -> Result<(), SttError> {
        let mut s = self.inner.lock();
        if s.closed {
            return Err(SttError::NotActive);
        }
        s.chunks_received += 1;
        s.bytes_received += chunk.byte_len();
        Ok(())
    }

    async fn finalize(&self) -> Result<(), SttError> {
        self.inner.lock().finalized = true;
        Ok(())
    }

    async fn next_event(&mut self) -> Option<Result<TranscriptEvent, SttError>> {
        self.events_rx.recv().await
    }

    async fn close(&mut self) -> Result<(), SttError> {
        self.inner.lock().closed = true;
        self.inner.lock().connection = MockConnectionState(ConnectionState::Closed);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cue_core::pcm::SampleRate;

    fn chunk() -> AudioChunk {
        AudioChunk {
            source: AudioSource::Microphone,
            sample_rate: SampleRate::new(16_000).unwrap(),
            samples: vec![0i16; 320],
            captured_at_ms: 0,
        }
    }

    #[tokio::test]
    async fn mock_stt_counts_chunks_and_bytes() {
        let (provider, ctrl) = MockStt::new(SttConfig::default());
        provider.send_audio(&chunk()).await.unwrap();
        provider.send_audio(&chunk()).await.unwrap();
        assert_eq!(ctrl.chunks_received(), 2);
        assert_eq!(ctrl.bytes_received(), 320 * 2 * 2); // 2 chunks × 320 samples × 2 bytes
    }

    #[tokio::test]
    async fn mock_stt_delivers_scripted_partial_and_final_in_order() {
        let (mut provider, ctrl) = MockStt::new(SttConfig::default());
        ctrl.emit_partial("hello");
        ctrl.emit_partial("hello wor");
        ctrl.emit_final("hello world", Vec::new());

        let e1 = provider.next_event().await.unwrap().unwrap();
        assert!(matches!(e1, TranscriptEvent::Partial { ref text, .. } if text == "hello"));
        let e2 = provider.next_event().await.unwrap().unwrap();
        assert!(matches!(e2, TranscriptEvent::Partial { ref text, .. } if text == "hello wor"));
        let e3 = provider.next_event().await.unwrap().unwrap();
        assert!(matches!(e3, TranscriptEvent::Final { ref text, .. } if text == "hello world"));
    }

    #[tokio::test]
    async fn mock_stt_surfaces_scripted_error() {
        let (mut provider, ctrl) = MockStt::new(SttConfig::default());
        ctrl.emit_error(SttError::Auth);
        let evt = provider.next_event().await.unwrap();
        assert!(evt.is_err());
        assert!(evt.unwrap_err().should_failover());
    }

    #[tokio::test]
    async fn mock_stt_finalize_and_close_tracked() {
        let (mut provider, ctrl) = MockStt::new(SttConfig::default());
        provider.finalize().await.unwrap();
        assert!(ctrl.was_finalized());
        provider.close().await.unwrap();
        assert!(ctrl.was_closed());
        assert_eq!(provider.connection_state(), ConnectionState::Closed);
    }

    #[tokio::test]
    async fn mock_stt_rejects_send_after_close() {
        let (mut provider, _ctrl) = MockStt::new(SttConfig::default());
        provider.close().await.unwrap();
        let err = provider.send_audio(&chunk()).await.unwrap_err();
        assert!(matches!(err, SttError::NotActive));
    }

    #[tokio::test]
    async fn mock_stt_connection_state_follows_control() {
        let (provider, ctrl) = MockStt::new(SttConfig::default());
        assert_eq!(provider.connection_state(), ConnectionState::Connected);
        ctrl.set_connection_state(ConnectionState::Reconnecting { attempt: 3 });
        assert_eq!(
            provider.connection_state(),
            ConnectionState::Reconnecting { attempt: 3 }
        );
    }
}
