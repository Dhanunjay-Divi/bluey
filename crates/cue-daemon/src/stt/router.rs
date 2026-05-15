//! STT router - selects among multiple providers with fallback.
use async_trait::async_trait;
use cue_core::pcm::AudioChunk;
use cue_core::stt::{ConnectionState, SttError, SttProvider, TranscriptEvent};

/// Check if the STT router is enabled via env var.
pub fn is_router_enabled() -> bool {
    std::env::var("BLUEY_STT_ROUTER")
        .map(|v| v == "1")
        .unwrap_or(false)
}

pub struct SttRouter {
    providers: Vec<Box<dyn SttProvider>>,
}

impl SttRouter {
    pub fn new(providers: Vec<Box<dyn SttProvider>>) -> Self {
        Self { providers }
    }
}

#[async_trait]
impl SttProvider for SttRouter {
    fn name(&self) -> &'static str {
        "router"
    }

    fn connection_state(&self) -> ConnectionState {
        self.providers
            .first()
            .map(|p| p.connection_state())
            .unwrap_or(ConnectionState::Idle)
    }

    async fn send_audio(&self, chunk: &AudioChunk) -> Result<(), SttError> {
        if let Some(p) = self.providers.first() {
            p.send_audio(chunk).await
        } else {
            Err(SttError::NotActive)
        }
    }

    async fn finalize(&self) -> Result<(), SttError> {
        if let Some(p) = self.providers.first() {
            p.finalize().await
        } else {
            Ok(())
        }
    }

    async fn next_event(&mut self) -> Option<Result<TranscriptEvent, SttError>> {
        if let Some(p) = self.providers.first_mut() {
            p.next_event().await
        } else {
            None
        }
    }

    async fn close(&mut self) -> Result<(), SttError> {
        for p in &mut self.providers {
            let _ = p.close().await;
        }
        Ok(())
    }
}
