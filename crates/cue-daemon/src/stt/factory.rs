//! STT provider factory — builds the production fallback chain from env config.
//!
//! Chain order: Deepgram (primary) -> OpenAI Realtime (if enabled) -> LocalWhisper (if enabled).
//! If `BLUEY_STT_ROUTER=1` or chain length >= 2, wraps in SttRouter.
//!
//! ## Scope (IMPORTANT)
//!
//! This factory produces **streaming** `SttProvider` instances (WebSocket /
//! NDJSON child-process based). It is currently called from:
//!
//! - `build_system_audio_stt_provider()` — continuous system-audio capture
//!   (`BLUEY_SYSTEM_AUDIO_CONTINUOUS=1` + `BLUEY_SYSTEM_AUDIO_STT=1`).
//! - `build_mic_stt_provider()` — a public helper for future use.
//!
//! ## Mic + chunk-based real-audio path is NOT routed through this factory
//!
//! The default `real_audio_loop` (driving both mic and system-audio in
//! chunk mode) calls `transcribe_audio_file()` which posts WAV chunks to
//! `runtime.stt_endpoint` via REST and does NOT consume an `SttProvider`.
//! That code path is intentionally untouched by this factory because the
//! REST + chunk format is incompatible with the streaming provider trait.
//!
//! Implication: enabling `BLUEY_STT_FALLBACK_OPENAI=1` /
//! `BLUEY_STT_LOCAL_WHISPER=1` only affects the continuous streaming
//! system-audio path. Mic + chunked-REST path uses whatever
//! `runtime.stt_endpoint` is configured to.
//!
//! Unifying the two paths (streaming providers everywhere) is a planned
//! follow-up round; tracked as a deferral in IMPL-PHASE-3-ROUND-7.md and
//! IMPL-PHASE-3-ROUND-9.md.

use cue_core::pcm::AudioSource;
use cue_core::stt::{SttConfig, SttError, SttProvider};

use super::router::{
    is_local_whisper_enabled, is_openai_fallback_enabled, is_router_enabled, SttRouter,
};

/// Build the STT provider chain from environment configuration.
///
/// Tries each enabled provider in order. If a provider's config is missing
/// (e.g. no API key) or its `connect` fails, that provider is logged and
/// SKIPPED — the chain continues with whatever other providers are enabled.
/// This means:
///
/// - Local-only mode works: set only `BLUEY_STT_LOCAL_WHISPER=1` and the
///   chain is built from LocalWhisper alone.
/// - Deepgram + LocalWhisper without OpenAI works: missing OPENAI_API_KEY
///   is not an error.
/// - Empty chain returns `SttError::NotActive`.
///
/// Returns a single provider when only one is configured, or an
/// `SttRouter` wrapping multiple providers when more are configured (or
/// when `BLUEY_STT_ROUTER=1` forces router wrapping).
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
            model: env_value("BLUEY_DEEPGRAM_MODEL").unwrap_or_else(|| "nova-3".into()),
            language: env_value("BLUEY_DEEPGRAM_LANGUAGE"),
            smart_format: env_bool("BLUEY_DEEPGRAM_SMART_FORMAT").unwrap_or(true),
            endpointing_ms: env_u32("BLUEY_DEEPGRAM_ENDPOINTING_MS").or(Some(300)),
            utterance_end_ms: env_u32("BLUEY_DEEPGRAM_UTTERANCE_END_MS").or(Some(1_000)),
            vad_events: env_bool("BLUEY_DEEPGRAM_VAD_EVENTS").unwrap_or(true),
            ..Default::default()
        };
        match super::deepgram::DeepgramProvider::connect(dg_cfg, stt_cfg.clone(), source).await {
            Ok(p) => providers.push(Box::new(p)),
            Err(e) => tracing::warn!(
                provider = "deepgram",
                error = %e,
                "primary STT unavailable; trying configured fallbacks"
            ),
        }
    } else {
        tracing::info!(
            provider = "deepgram",
            "no Deepgram API key configured; will try fallback providers"
        );
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
                Err(e) => tracing::warn!(provider = "openai", error = %e, "fallback unavailable"),
            }
        } else {
            tracing::info!(
                provider = "openai",
                "OpenAI fallback enabled but OPENAI_API_KEY not set; skipping"
            );
        }
    }

    // Fallback: Local Whisper
    if is_local_whisper_enabled() {
        match super::whisper::LocalWhisperProvider::connect(stt_cfg.clone()) {
            Ok(p) => providers.push(Box::new(p)),
            Err(e) => tracing::warn!(
                provider = "local_whisper",
                error = %e,
                "fallback unavailable"
            ),
        }
    }

    if providers.is_empty() {
        return Err(SttError::NotActive);
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

fn env_value(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn env_bool(name: &str) -> Option<bool> {
    env_value(name).and_then(|value| match value.to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    })
}

fn env_u32(name: &str) -> Option<u32> {
    env_value(name).and_then(|value| value.parse::<u32>().ok())
}
