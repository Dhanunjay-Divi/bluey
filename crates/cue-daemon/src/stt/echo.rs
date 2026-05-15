//! Echo STT provider stub - no-op provider for fallback.
use async_trait::async_trait;
use cue_core::pcm::{AudioChunk, AudioSource};
use cue_core::stt::{ConnectionState, SttError, SttProvider, TranscriptEvent};

pub struct EchoProvider {
    _source: AudioSource,
}

impl EchoProvider {
    pub fn new(source: AudioSource) -> Self {
        Self { _source: source }
    }
}

#[async_trait]
impl SttProvider for EchoProvider {
    fn name(&self) -> &'static str {
        "echo"
    }

    fn connection_state(&self) -> ConnectionState {
        ConnectionState::Idle
    }

    async fn send_audio(&self, _chunk: &AudioChunk) -> Result<(), SttError> {
        Ok(())
    }

    async fn finalize(&self) -> Result<(), SttError> {
        Ok(())
    }

    async fn next_event(&mut self) -> Option<Result<TranscriptEvent, SttError>> {
        None
    }

    async fn close(&mut self) -> Result<(), SttError> {
        Ok(())
    }
}
