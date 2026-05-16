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
