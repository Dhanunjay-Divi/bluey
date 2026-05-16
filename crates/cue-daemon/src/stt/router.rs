//! SttRouter — failover chain wrapping multiple `SttProvider` implementations.
//!
//! Implements `SttProvider` itself. Forwards audio to the currently active
//! provider. On `should_failover()` errors (Auth/Quota), advances to the
//! next provider in the chain. Network/Protocol errors are surfaced but do
//! NOT trigger failover (the inner provider handles its own reconnects).
//!
//! Gated behind `BLUEY_STT_ROUTER=1` (off by default).

use async_trait::async_trait;
use cue_core::pcm::AudioChunk;
use cue_core::stt::{ConnectionState, SttError, SttProvider, TranscriptEvent};
use tracing::{info, warn};

pub struct SttRouter {
    providers: Vec<Box<dyn SttProvider>>,
    active: usize,
}

impl SttRouter {
    /// Create a router from an ordered list of providers.
    /// The first provider is active initially; subsequent ones are fallbacks.
    pub fn new(providers: Vec<Box<dyn SttProvider>>) -> Self {
        assert!(
            !providers.is_empty(),
            "SttRouter requires at least one provider"
        );
        Self {
            providers,
            active: 0,
        }
    }

    /// Index of the currently active provider.
    /// Returns the names of all providers in the chain, in order.
    pub fn provider_names(&self) -> Vec<&'static str> {
        self.providers.iter().map(|p| p.name()).collect()
    }

    pub fn active_index(&self) -> usize {
        self.active
    }

    /// Advance to the next provider. Returns `true` if a fallback was available.
    fn failover(&mut self) -> bool {
        let next = self.active + 1;
        if next < self.providers.len() {
            info!(
                from = self.providers[self.active].name(),
                to = self.providers[next].name(),
                "STT router failing over"
            );
            self.active = next;
            true
        } else {
            warn!("STT router exhausted all providers");
            false
        }
    }
}

#[async_trait]
impl SttProvider for SttRouter {
    fn name(&self) -> &'static str {
        "stt_router"
    }

    fn connection_state(&self) -> ConnectionState {
        self.providers[self.active].connection_state()
    }

    async fn send_audio(&self, chunk: &AudioChunk) -> Result<(), SttError> {
        self.providers[self.active].send_audio(chunk).await
    }

    async fn finalize(&self) -> Result<(), SttError> {
        self.providers[self.active].finalize().await
    }

    async fn next_event(&mut self) -> Option<Result<TranscriptEvent, SttError>> {
        let event = self.providers[self.active].next_event().await;
        if let Some(Err(e)) = &event {
            if e.should_failover() {
                self.failover();
            }
        }
        event
    }

    async fn close(&mut self) -> Result<(), SttError> {
        // Close all providers, not just the active one.
        for provider in &mut self.providers {
            let _ = provider.close().await;
        }
        Ok(())
    }
}

/// Returns `true` if the STT router is enabled via environment variable.
pub fn is_router_enabled() -> bool {
    std::env::var("BLUEY_STT_ROUTER")
        .map(|v| v == "1")
        .unwrap_or(false)
}

/// Returns true if the OpenAI Realtime fallback is enabled via environment variable.
pub fn is_openai_fallback_enabled() -> bool {
    std::env::var("BLUEY_STT_FALLBACK_OPENAI")
        .map(|v| v == "1")
        .unwrap_or(false)
}

/// Returns true if local whisper fallback is enabled via environment variable.
pub fn is_local_whisper_enabled() -> bool {
    std::env::var("BLUEY_STT_LOCAL_WHISPER")
        .map(|v| v == "1")
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stt::mock::{MockStt, MockSttControl};
    use cue_core::pcm::{AudioSource, SampleRate};
    use cue_core::stt::SttConfig;

    fn chunk() -> AudioChunk {
        AudioChunk {
            source: AudioSource::Microphone,
            sample_rate: SampleRate::SR_16K,
            samples: vec![0i16; 320],
            captured_at_ms: 0,
        }
    }

    fn mock_pair() -> (Box<dyn SttProvider>, MockSttControl) {
        let (provider, ctrl) = MockStt::new(SttConfig::default());
        (Box::new(provider), ctrl)
    }

    #[tokio::test]
    async fn happy_path_forwards_to_first_provider() {
        let (p1, ctrl1) = mock_pair();
        let (p2, _ctrl2) = mock_pair();
        let mut router = SttRouter::new(vec![p1, p2]);

        router.send_audio(&chunk()).await.unwrap();
        assert_eq!(ctrl1.chunks_received(), 1);
        assert_eq!(router.active_index(), 0);

        ctrl1.emit_final("hello", Vec::new());
        let event = router.next_event().await.unwrap().unwrap();
        assert!(matches!(event, TranscriptEvent::Final { ref text, .. } if text == "hello"));
    }

    #[tokio::test]
    async fn auth_error_triggers_failover() {
        let (p1, ctrl1) = mock_pair();
        let (p2, ctrl2) = mock_pair();
        let mut router = SttRouter::new(vec![p1, p2]);

        // First provider emits Auth error
        ctrl1.emit_error(SttError::Auth);
        let event = router.next_event().await.unwrap();
        assert!(event.is_err());
        // Router should have failed over
        assert_eq!(router.active_index(), 1);

        // Second provider works
        router.send_audio(&chunk()).await.unwrap();
        assert_eq!(ctrl2.chunks_received(), 1);

        ctrl2.emit_final("from fallback", Vec::new());
        let event = router.next_event().await.unwrap().unwrap();
        assert!(
            matches!(event, TranscriptEvent::Final { ref text, .. } if text == "from fallback")
        );
    }

    #[tokio::test]
    async fn network_error_does_not_failover() {
        let (p1, ctrl1) = mock_pair();
        let (p2, _ctrl2) = mock_pair();
        let mut router = SttRouter::new(vec![p1, p2]);

        ctrl1.emit_error(SttError::Network("timeout".into()));
        let event = router.next_event().await.unwrap();
        assert!(event.is_err());
        // Should NOT have failed over
        assert_eq!(router.active_index(), 0);
    }

    #[tokio::test]
    async fn all_providers_exhausted_surfaces_error() {
        let (p1, ctrl1) = mock_pair();
        let (p2, ctrl2) = mock_pair();
        let mut router = SttRouter::new(vec![p1, p2]);

        // Exhaust both
        ctrl1.emit_error(SttError::Auth);
        let _ = router.next_event().await; // triggers failover to p2
        assert_eq!(router.active_index(), 1);

        ctrl2.emit_error(SttError::Quota("limit".into()));
        let event = router.next_event().await.unwrap();
        assert!(event.is_err());
        // Still at index 1 (no more to fail over to)
        assert_eq!(router.active_index(), 1);
    }
}
