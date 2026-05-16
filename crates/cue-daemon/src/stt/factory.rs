//! STT provider factory — builds the production fallback chain from env config.
//!
//! Chain order: Deepgram (primary) -> OpenAI Realtime (if enabled) -> LocalWhisper (if enabled).
//! If `BLUEY_STT_ROUTER=1` or chain length >= 2, wraps in SttRouter.

use cue_core::pcm::AudioSource;
use cue_core::stt::{SttConfig, SttError, SttProvider};

use super::router::{
    is_local_whisper_enabled, is_openai_fallback_enabled, is_router_enabled, SttRouter,
};

/// Build the STT provider chain from environment configuration.
///
/// Returns a single provider or an SttRouter wrapping multiple providers.
pub async fn build_stt_chain(
    stt_cfg: &SttConfig,
    source: AudioSource,
) -> Result<Box<dyn SttProvider>, SttError> {
    let mut providers: Vec<Box<dyn SttProvider>> = Vec::new();

    // Primary: Deepgram (or Mock if BLUEY_USE_MOCK_STT=1)
    if use_mock_stt() {
        providers.push(Box::new(super::echo::EchoProvider::new(source)));
    } else if let Some(api_key) = env_stt_key() {
        let dg_cfg = super::deepgram::DeepgramConfig {
            api_key,
            ..Default::default()
        };
        let provider = super::deepgram::DeepgramProvider::connect(dg_cfg, stt_cfg.clone(), source)
            .await
            .map_err(|e| SttError::Provider(e.to_string()))?;
        providers.push(Box::new(provider));
    } else {
        return Err(SttError::Provider("no STT API key configured".into()));
    }

    // Fallback: OpenAI Realtime
    if is_openai_fallback_enabled() {
        if let Some(api_key) = openai_key() {
            let oai_cfg = super::openai::OpenAiRealtimeConfig {
                api_key,
                ..Default::default()
            };
            match super::openai::OpenAiRealtimeProvider::connect(oai_cfg, stt_cfg.clone(), source)
                .await
            {
                Ok(p) => providers.push(Box::new(p)),
                Err(e) => tracing::warn!("OpenAI fallback unavailable: {e}"),
            }
        }
    }

    // Fallback: Local Whisper
    if is_local_whisper_enabled() {
        match super::whisper::LocalWhisperProvider::connect(stt_cfg.clone()) {
            Ok(p) => providers.push(Box::new(p)),
            Err(e) => tracing::warn!("LocalWhisper fallback unavailable: {e}"),
        }
    }

    // Wrap in router if multiple providers or router forced
    if providers.len() >= 2 || is_router_enabled() {
        Ok(Box::new(SttRouter::new(providers)))
    } else {
        Ok(providers.into_iter().next().expect("at least one provider"))
    }
}

fn use_mock_stt() -> bool {
    std::env::var("BLUEY_USE_MOCK_STT")
        .map(|v| v == "1")
        .unwrap_or(false)
}

fn env_stt_key() -> Option<String> {
    std::env::var("BLUEY_STT_API_KEY")
        .or_else(|_| std::env::var("DEEPGRAM_API_KEY"))
        .ok()
        .filter(|v| !v.is_empty())
}

fn openai_key() -> Option<String> {
    std::env::var("OPENAI_API_KEY")
        .ok()
        .filter(|v| !v.is_empty())
}
