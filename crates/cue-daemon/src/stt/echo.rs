//! EchoProvider — stub STT provider that echoes audio chunks as transcripts.
//!
//! Used as a secondary fallback in the `SttRouter` chain. Echoes each audio
//! chunk as a `TranscriptEvent::Final` with text `"echo:<chunk_index>"`.
//! Real secondary providers (OpenAI Whisper, AssemblyAI) replace this later.

use async_trait::async_trait;
use cue_core::pcm::AudioChunk;
use cue_core::stt::{ConnectionState, SttError, SttProvider, TranscriptEvent};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};

pub struct EchoProvider {
    events_rx: UnboundedReceiver<Result<TranscriptEvent, SttError>>,
    events_tx: UnboundedSender<Result<TranscriptEvent, SttError>>,
    counter: AtomicU64,
    closed: AtomicBool,
    source: cue_core::pcm::AudioSource,
}

impl EchoProvider {
    pub fn new(source: cue_core::pcm::AudioSource) -> Self {
        let (tx, rx) = unbounded_channel();
        Self {
            events_rx: rx,
            events_tx: tx,
            counter: AtomicU64::new(0),
            closed: AtomicBool::new(false),
            source,
        }
    }
}

#[async_trait]
impl SttProvider for EchoProvider {
    fn name(&self) -> &'static str {
        "echo"
    }

    fn connection_state(&self) -> ConnectionState {
        if self.closed.load(Ordering::Acquire) {
            ConnectionState::Closed
        } else {
            ConnectionState::Connected
        }
    }

    async fn send_audio(&self, _chunk: &AudioChunk) -> Result<(), SttError> {
        if self.closed.load(Ordering::Acquire) {
            return Err(SttError::NotActive);
        }
        let idx = self.counter.fetch_add(1, Ordering::Relaxed);
        let event = TranscriptEvent::Final {
            text: format!("echo:{idx}"),
            confidence: Some(1.0),
            source: self.source,
            words: Vec::new(),
        };
        let _ = self.events_tx.send(Ok(event));
        Ok(())
    }

    async fn finalize(&self) -> Result<(), SttError> {
        Ok(())
    }

    async fn next_event(&mut self) -> Option<Result<TranscriptEvent, SttError>> {
        self.events_rx.recv().await
    }

    async fn close(&mut self) -> Result<(), SttError> {
        self.closed.store(true, Ordering::Release);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cue_core::pcm::{AudioSource, SampleRate};

    fn chunk() -> AudioChunk {
        AudioChunk {
            source: AudioSource::Microphone,
            sample_rate: SampleRate::SR_16K,
            samples: vec![0i16; 320],
            captured_at_ms: 0,
        }
    }

    #[tokio::test]
    async fn echo_provider_emits_indexed_transcripts() {
        let mut provider = EchoProvider::new(AudioSource::Microphone);
        provider.send_audio(&chunk()).await.unwrap();
        provider.send_audio(&chunk()).await.unwrap();

        let e1 = provider.next_event().await.unwrap().unwrap();
        assert!(matches!(e1, TranscriptEvent::Final { ref text, .. } if text == "echo:0"));
        let e2 = provider.next_event().await.unwrap().unwrap();
        assert!(matches!(e2, TranscriptEvent::Final { ref text, .. } if text == "echo:1"));
    }

    #[tokio::test]
    async fn echo_provider_rejects_after_close() {
        let mut provider = EchoProvider::new(AudioSource::System);
        provider.close().await.unwrap();
        assert_eq!(provider.connection_state(), ConnectionState::Closed);
        let err = provider.send_audio(&chunk()).await.unwrap_err();
        assert!(matches!(err, SttError::NotActive));
    }
}
