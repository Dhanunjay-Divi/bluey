//! Integration test for `SystemAudioCapture` using the mock stub binary.

use std::path::PathBuf;
use std::time::Duration;

use cue_core::pcm::{AudioSource, SampleRate};
use cue_daemon::audio::system_capture::SystemAudioCapture;

fn stub_binary_path() -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_BIN_EXE_system-audio-stub"));
    if !path.exists() {
        // Fallback for workspace builds
        path = PathBuf::from("target/debug/system-audio-stub");
    }
    path
}

#[tokio::test]
async fn system_audio_capture_receives_chunks_from_stub() {
    let stub = stub_binary_path();
    if !stub.exists() {
        panic!(
            "system-audio-stub not found at {}; build with `cargo build --bin system-audio-stub`",
            stub.display()
        );
    }

    std::env::set_var("BLUEY_SYSTEM_AUDIO_BINARY", &stub);

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let capture = SystemAudioCapture::start(tx).expect("start capture");

    let mut received = Vec::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);

    while received.len() < 5 && tokio::time::Instant::now() < deadline {
        match tokio::time::timeout(Duration::from_millis(500), rx.recv()).await {
            Ok(Some(chunk)) => received.push(chunk),
            _ => break,
        }
    }

    capture.stop().await;
    std::env::remove_var("BLUEY_SYSTEM_AUDIO_BINARY");

    assert!(
        received.len() >= 5,
        "expected at least 5 AudioChunks, got {}",
        received.len()
    );

    for chunk in &received {
        assert_eq!(chunk.source, AudioSource::System);
        assert_eq!(chunk.sample_rate, SampleRate::SR_16K);
        assert_eq!(
            chunk.samples.len(),
            320,
            "each chunk should be 20ms = 320 samples at 16kHz"
        );
        assert!(chunk.captured_at_ms > 0);
    }
}

#[tokio::test]
async fn system_audio_capture_stops_cleanly() {
    let stub = stub_binary_path();
    if !stub.exists() {
        return; // skip if not built
    }

    std::env::set_var("BLUEY_SYSTEM_AUDIO_BINARY", &stub);

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let capture = SystemAudioCapture::start(tx).expect("start capture");

    // Receive one chunk then stop
    let _ = tokio::time::timeout(Duration::from_secs(2), rx.recv()).await;
    capture.stop().await;

    std::env::remove_var("BLUEY_SYSTEM_AUDIO_BINARY");
    // If we get here without hanging, the test passes
}
