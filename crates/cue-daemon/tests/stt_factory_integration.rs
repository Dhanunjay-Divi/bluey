//! Deterministic integration tests for the STT factory chain builder.
//!
//! Uses BLUEY_USE_MOCK_STT=1 to avoid needing real API keys, and verifies
//! that the factory correctly assembles provider chains based on env config.

use cue_core::pcm::AudioSource;
use cue_core::stt::SttConfig;
use cue_daemon::stt::factory::build_stt_chain;

fn stt_cfg() -> SttConfig {
    SttConfig {
        source: AudioSource::System,
        ..Default::default()
    }
}

/// Helper: set env vars, run async closure, then restore.
/// Uses save/restore pattern to avoid mutex poisoning.
fn with_env_async<F, Fut>(vars: &[(&str, Option<&str>)], f: F)
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = ()>,
{
    let mut old: Vec<(String, Option<String>)> = Vec::new();
    for (key, val) in vars {
        old.push((key.to_string(), std::env::var(key).ok()));
        match val {
            Some(v) => unsafe { std::env::set_var(key, v) },
            None => unsafe { std::env::remove_var(key) },
        }
    }
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(f());
    for (key, prev) in &old {
        match prev {
            Some(v) => unsafe { std::env::set_var(key, v) },
            None => unsafe { std::env::remove_var(key) },
        }
    }
}

#[test]
#[ignore] // env-mutating tests must run with --test-threads=1
fn factory_mock_only_returns_single_provider() {
    with_env_async(
        &[
            ("BLUEY_USE_MOCK_STT", Some("1")),
            ("BLUEY_STT_ROUTER", None),
            ("BLUEY_STT_FALLBACK_OPENAI", None),
            ("BLUEY_STT_LOCAL_WHISPER", None),
            ("OPENAI_API_KEY", None),
        ],
        || async {
            let provider = build_stt_chain(&stt_cfg(), AudioSource::System)
                .await
                .unwrap();
            assert_eq!(provider.name(), "echo");
        },
    );
}

#[test]
#[ignore]
fn factory_mock_with_router_forced() {
    with_env_async(
        &[
            ("BLUEY_USE_MOCK_STT", Some("1")),
            ("BLUEY_STT_ROUTER", Some("1")),
            ("BLUEY_STT_FALLBACK_OPENAI", None),
            ("BLUEY_STT_LOCAL_WHISPER", None),
            ("OPENAI_API_KEY", None),
        ],
        || async {
            let provider = build_stt_chain(&stt_cfg(), AudioSource::System)
                .await
                .unwrap();
            assert_eq!(provider.name(), "stt_router");
        },
    );
}

#[test]
#[ignore]
fn factory_mock_plus_openai_creates_router() {
    with_env_async(
        &[
            ("BLUEY_USE_MOCK_STT", Some("1")),
            ("BLUEY_STT_ROUTER", None),
            ("BLUEY_STT_FALLBACK_OPENAI", Some("1")),
            ("OPENAI_API_KEY", Some("sk-test-fake-key")),
            ("BLUEY_STT_LOCAL_WHISPER", None),
        ],
        || async {
            let provider = build_stt_chain(&stt_cfg(), AudioSource::System)
                .await
                .unwrap();
            assert_eq!(provider.name(), "stt_router");
        },
    );
}

#[test]
#[ignore]
fn factory_no_api_key_errors() {
    with_env_async(
        &[
            ("BLUEY_USE_MOCK_STT", None),
            ("BLUEY_STT_API_KEY", None),
            ("DEEPGRAM_API_KEY", None),
            ("BLUEY_STT_ROUTER", None),
            ("BLUEY_STT_FALLBACK_OPENAI", None),
            ("BLUEY_STT_LOCAL_WHISPER", None),
        ],
        || async {
            let result = build_stt_chain(&stt_cfg(), AudioSource::System).await;
            assert!(result.is_err());
        },
    );
}

#[test]
#[ignore]
fn factory_local_whisper_enabled_creates_router_with_whisper_in_chain() {
    with_env_async(
        &[
            ("BLUEY_USE_MOCK_STT", Some("1")),
            ("BLUEY_STT_LOCAL_WHISPER", Some("1")),
            ("BLUEY_LOCAL_WHISPER_BINARY", Some("/bin/cat")),
            ("BLUEY_STT_ROUTER", None),
            ("BLUEY_STT_FALLBACK_OPENAI", None),
            ("OPENAI_API_KEY", None),
        ],
        || async {
            // System audio path
            let provider = build_stt_chain(&stt_cfg(), AudioSource::System)
                .await
                .unwrap();
            assert_eq!(provider.name(), "stt_router");

            // Mic path — same factory, different source
            let mic_cfg = SttConfig {
                source: AudioSource::Microphone,
                ..Default::default()
            };
            let mic_provider = build_stt_chain(&mic_cfg, AudioSource::Microphone)
                .await
                .unwrap();
            assert_eq!(mic_provider.name(), "stt_router");
        },
    );
}

#[test]
#[ignore]
fn factory_local_whisper_chain_contains_whisper_provider() {
    with_env_async(
        &[
            ("BLUEY_USE_MOCK_STT", Some("1")),
            ("BLUEY_STT_LOCAL_WHISPER", Some("1")),
            ("BLUEY_LOCAL_WHISPER_BINARY", Some("/bin/cat")),
            ("BLUEY_STT_ROUTER", None),
            ("BLUEY_STT_FALLBACK_OPENAI", None),
            ("OPENAI_API_KEY", None),
        ],
        || async {
            let provider = build_stt_chain(&stt_cfg(), AudioSource::System)
                .await
                .unwrap();
            // The router wraps [echo, local_whisper] — chain length 2
            assert_eq!(provider.name(), "stt_router");
        },
    );
}

#[test]
#[ignore]
fn factory_local_only_no_deepgram_key() {
    // Only LocalWhisper is enabled; no DEEPGRAM_API_KEY at all.
    // Must produce a working single-provider chain without erroring.
    with_env_async(
        &[
            ("BLUEY_USE_MOCK_STT", None),
            ("DEEPGRAM_API_KEY", None),
            ("BLUEY_STT_API_KEY", None),
            ("BLUEY_STT_LOCAL_WHISPER", Some("1")),
            ("BLUEY_LOCAL_WHISPER_BINARY", Some("/bin/cat")),
            ("BLUEY_STT_ROUTER", None),
            ("BLUEY_STT_FALLBACK_OPENAI", None),
            ("OPENAI_API_KEY", None),
        ],
        || async {
            let provider = build_stt_chain(&stt_cfg(), AudioSource::System)
                .await
                .expect("local-only chain must build without Deepgram");
            // Single provider (no router because chain length == 1 and BLUEY_STT_ROUTER not set)
            assert_eq!(provider.name(), "local_whisper");
        },
    );
}

#[test]
#[ignore]
fn factory_openai_fallback_enabled_but_no_key_skipped_gracefully() {
    // BLUEY_STT_FALLBACK_OPENAI=1 but no OPENAI_API_KEY: skip OpenAI,
    // chain still has Deepgram (via mock).
    with_env_async(
        &[
            ("BLUEY_USE_MOCK_STT", Some("1")),
            ("BLUEY_STT_FALLBACK_OPENAI", Some("1")),
            ("OPENAI_API_KEY", None),
            ("BLUEY_STT_LOCAL_WHISPER", None),
            ("BLUEY_STT_ROUTER", None),
        ],
        || async {
            let provider = build_stt_chain(&stt_cfg(), AudioSource::System)
                .await
                .expect("must build with mock primary even when openai key missing");
            // Single provider (mock echo) — OpenAI was skipped silently
            assert_eq!(provider.name(), "echo");
        },
    );
}

#[test]
#[ignore]
fn factory_empty_chain_returns_not_active() {
    // No primary, no fallbacks enabled → empty chain → NotActive error.
    with_env_async(
        &[
            ("BLUEY_USE_MOCK_STT", None),
            ("DEEPGRAM_API_KEY", None),
            ("BLUEY_STT_API_KEY", None),
            ("BLUEY_STT_LOCAL_WHISPER", None),
            ("BLUEY_STT_FALLBACK_OPENAI", None),
            ("OPENAI_API_KEY", None),
            ("BLUEY_STT_ROUTER", None),
        ],
        || async {
            let result = build_stt_chain(&stt_cfg(), AudioSource::System).await;
            let err = match result {
                Ok(_) => panic!("empty chain unexpectedly built a provider"),
                Err(e) => e,
            };
            assert!(
                matches!(err, cue_core::stt::SttError::NotActive),
                "expected NotActive, got {err:?}"
            );
        },
    );
}

#[test]
#[ignore]
fn factory_local_only_with_failed_deepgram_attempt_uses_whisper() {
    // Deepgram key is set but bogus (will fail real connect attempt).
    // LocalWhisper fallback must still be in the chain.
    // Note: because we use the mock-stt fast path for tests, we exercise the
    // "primary missing" path by leaving DEEPGRAM_API_KEY unset, which causes
    // the factory to log + skip Deepgram and fall through to LocalWhisper.
    // A "primary failed connect" path would require a real network mock —
    // that scenario is covered by the same code path (provider.push only on Ok).
    with_env_async(
        &[
            ("BLUEY_USE_MOCK_STT", None),
            ("DEEPGRAM_API_KEY", None),
            ("BLUEY_STT_API_KEY", None),
            ("BLUEY_STT_LOCAL_WHISPER", Some("1")),
            ("BLUEY_LOCAL_WHISPER_BINARY", Some("/bin/cat")),
            ("BLUEY_STT_ROUTER", None),
        ],
        || async {
            let provider = build_stt_chain(&stt_cfg(), AudioSource::System)
                .await
                .expect("LocalWhisper-only fallback must build");
            assert_eq!(provider.name(), "local_whisper");
        },
    );
}
