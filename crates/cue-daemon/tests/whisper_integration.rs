//! Integration tests for LocalWhisperProvider using the whisper-stub binary.

use cue_core::pcm::{AudioChunk, AudioSource, SampleRate};
use cue_core::stt::{SttConfig, SttError, SttProvider, TranscriptEvent};
use cue_daemon::stt::mock::{MockStt, MockSttControl};
use cue_daemon::stt::router::SttRouter;
use cue_daemon::stt::whisper::LocalWhisperProvider;

/// Build the whisper-stub binary path from cargo's target directory.
fn stub_binary_path() -> String {
    let mut path = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    path.push("whisper-stub");
    path.to_string_lossy().to_string()
}

fn audio_chunk() -> AudioChunk {
    AudioChunk {
        source: AudioSource::Microphone,
        sample_rate: SampleRate::SR_16K,
        samples: vec![1000i16; 320],
        captured_at_ms: 0,
    }
}

fn mock_pair() -> (Box<dyn SttProvider>, MockSttControl) {
    let (provider, ctrl) = MockStt::new(SttConfig::default());
    (Box::new(provider), ctrl)
}

#[tokio::test]
async fn whisper_provider_receives_ndjson_events() {
    std::env::set_var("BLUEY_LOCAL_WHISPER_BINARY", stub_binary_path());

    let config = SttConfig::default();
    let mut provider = LocalWhisperProvider::connect(config).unwrap();

    assert_eq!(provider.name(), "local_whisper");

    // Send audio chunk to trigger stub output
    provider.send_audio(&audio_chunk()).await.unwrap();

    // Read events with timeout
    let evt1 = tokio::time::timeout(std::time::Duration::from_secs(2), provider.next_event())
        .await
        .expect("timeout waiting for event 1")
        .expect("channel closed")
        .expect("error event");

    assert!(matches!(evt1, TranscriptEvent::Partial { ref text, .. } if text == "hello"));

    let evt2 = tokio::time::timeout(std::time::Duration::from_secs(2), provider.next_event())
        .await
        .expect("timeout waiting for event 2")
        .expect("channel closed")
        .expect("error event");

    assert!(matches!(evt2, TranscriptEvent::Partial { ref text, .. } if text == "hello world"));

    let evt3 = tokio::time::timeout(std::time::Duration::from_secs(2), provider.next_event())
        .await
        .expect("timeout waiting for event 3")
        .expect("channel closed")
        .expect("error event");

    match evt3 {
        TranscriptEvent::Final {
            text, confidence, ..
        } => {
            assert_eq!(text, "hello world");
            assert_eq!(confidence, Some(0.95f32));
        }
        _ => panic!("expected Final event, got {:?}", evt3),
    }

    provider.close().await.unwrap();
}

#[tokio::test]
async fn whisper_provider_name_is_local_whisper() {
    std::env::set_var("BLUEY_LOCAL_WHISPER_BINARY", stub_binary_path());
    let config = SttConfig::default();
    let provider = LocalWhisperProvider::connect(config).unwrap();
    assert_eq!(provider.name(), "local_whisper");
}

#[tokio::test]
async fn whisper_stub_spawn_and_close() {
    std::env::set_var("BLUEY_LOCAL_WHISPER_BINARY", stub_binary_path());
    let config = SttConfig::default();
    let mut provider = LocalWhisperProvider::connect(config).unwrap();

    provider.send_audio(&audio_chunk()).await.unwrap();
    provider.close().await.unwrap();

    assert_eq!(
        provider.connection_state(),
        cue_core::stt::ConnectionState::Closed
    );
}

#[tokio::test]
async fn router_three_tier_failover_deepgram_openai_whisper() {
    // Simulate: Deepgram (Auth fail) -> OpenAI (Quota fail) -> LocalWhisper (works)
    std::env::set_var("BLUEY_LOCAL_WHISPER_BINARY", stub_binary_path());

    let (p1, ctrl1) = mock_pair(); // "Deepgram"
    let (p2, ctrl2) = mock_pair(); // "OpenAI"

    let config = SttConfig::default();
    let p3 = LocalWhisperProvider::connect(config).unwrap(); // LocalWhisper

    let mut router = SttRouter::new(vec![p1, p2, Box::new(p3)]);

    // Deepgram fails with Auth error -> failover
    ctrl1.emit_error(SttError::Auth);
    let _ = router.next_event().await;
    assert_eq!(router.active_index(), 1);

    // OpenAI fails with Quota error -> failover
    ctrl2.emit_error(SttError::Quota("rate limited".into()));
    let _ = router.next_event().await;
    assert_eq!(router.active_index(), 2);

    // LocalWhisper is now active — send audio
    router.send_audio(&audio_chunk()).await.unwrap();

    // Should receive events from the whisper stub
    let evt = tokio::time::timeout(std::time::Duration::from_secs(2), router.next_event())
        .await
        .expect("timeout")
        .expect("channel closed")
        .expect("error");

    assert!(matches!(evt, TranscriptEvent::Partial { ref text, .. } if text == "hello"));
}
