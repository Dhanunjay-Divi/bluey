//! End-to-end test using the whisper-stub binary via the STT factory.
//!
//! This test sets BLUEY_STT_LOCAL_WHISPER=1 and BLUEY_LOCAL_WHISPER_BINARY
//! to the cargo-built whisper-stub, then drives audio through the factory-
//! built provider and verifies TranscriptEvent::Final is received.

use cue_core::pcm::{AudioChunk, AudioSource, SampleRate};
use cue_core::stt::{SttConfig, TranscriptEvent};

/// Resolve the whisper-stub binary path from CARGO_BIN_EXE_whisper-stub
/// (set automatically by cargo test for [[bin]] targets in the same crate).
fn whisper_stub_path() -> String {
    // cargo sets CARGO_BIN_EXE_whisper-stub for integration tests
    std::env::var("CARGO_BIN_EXE_whisper-stub").unwrap_or_else(|_| {
        // Fallback: derive from current exe location
        let mut path = std::env::current_exe()
            .unwrap()
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf();
        path.push(format!("whisper-stub{}", std::env::consts::EXE_SUFFIX));
        path.to_string_lossy().to_string()
    })
}

fn audio_chunk() -> AudioChunk {
    AudioChunk {
        source: AudioSource::Microphone,
        sample_rate: SampleRate::SR_16K,
        samples: vec![500i16; 640],
        captured_at_ms: 0,
    }
}

#[tokio::test]
#[ignore] // Requires whisper-stub binary to be built
async fn whisper_stub_e2e_via_factory() {
    let stub_path = whisper_stub_path();
    if !std::path::Path::new(&stub_path).exists() {
        eprintln!("whisper-stub not found at {stub_path}, skipping");
        return;
    }

    // Configure env for LocalWhisper via factory
    std::env::set_var("BLUEY_STT_LOCAL_WHISPER", "1");
    std::env::set_var("BLUEY_LOCAL_WHISPER_BINARY", &stub_path);
    // Disable other providers so factory only builds LocalWhisper
    std::env::remove_var("BLUEY_STT_API_KEY");
    std::env::remove_var("DEEPGRAM_API_KEY");
    std::env::remove_var("OPENAI_API_KEY");
    std::env::remove_var("BLUEY_USE_MOCK_STT");
    std::env::remove_var("BLUEY_STT_FALLBACK_OPENAI");

    let stt_cfg = SttConfig::default();
    let mut provider = cue_daemon::stt::factory::build_stt_chain(&stt_cfg, AudioSource::Microphone)
        .await
        .expect("factory should build LocalWhisper provider");

    // Send audio to trigger the stub
    provider.send_audio(&audio_chunk()).await.unwrap();

    // Collect events until we get a Final
    let mut got_final = false;
    for _ in 0..10 {
        let evt = tokio::time::timeout(std::time::Duration::from_secs(3), provider.next_event())
            .await
            .expect("timeout waiting for event")
            .expect("channel closed")
            .expect("provider error");

        if let TranscriptEvent::Final { ref text, .. } = evt {
            assert_eq!(text, "hello world");
            got_final = true;
            break;
        }
    }

    assert!(
        got_final,
        "expected TranscriptEvent::Final with 'hello world'"
    );
    provider.close().await.unwrap();
}

#[tokio::test]
#[ignore]
async fn whisper_stub_e2e_emits_partial_before_final() {
    let stub_path = whisper_stub_path();
    if !std::path::Path::new(&stub_path).exists() {
        return;
    }

    std::env::set_var("BLUEY_STT_LOCAL_WHISPER", "1");
    std::env::set_var("BLUEY_LOCAL_WHISPER_BINARY", &stub_path);
    std::env::remove_var("BLUEY_STT_API_KEY");
    std::env::remove_var("DEEPGRAM_API_KEY");
    std::env::remove_var("OPENAI_API_KEY");
    std::env::remove_var("BLUEY_USE_MOCK_STT");
    std::env::remove_var("BLUEY_STT_FALLBACK_OPENAI");

    let stt_cfg = SttConfig::default();
    let mut provider = cue_daemon::stt::factory::build_stt_chain(&stt_cfg, AudioSource::Microphone)
        .await
        .expect("factory should build LocalWhisper provider");

    provider.send_audio(&audio_chunk()).await.unwrap();

    let mut saw_partial = false;
    for _ in 0..10 {
        let evt = tokio::time::timeout(std::time::Duration::from_secs(3), provider.next_event())
            .await
            .expect("timeout")
            .expect("closed")
            .expect("error");

        match evt {
            TranscriptEvent::Partial { ref text, .. } => {
                assert!(text.starts_with("hello"));
                saw_partial = true;
            }
            TranscriptEvent::Final { .. } => break,
            _ => {}
        }
    }

    assert!(
        saw_partial,
        "expected at least one Partial event before Final"
    );
    provider.close().await.unwrap();
}
