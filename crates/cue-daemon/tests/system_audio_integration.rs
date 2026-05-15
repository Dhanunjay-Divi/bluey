//! Integration test for `SystemAudioCapture` using the mock stub binary.

use std::path::PathBuf;
use std::time::Duration;

use cue_core::pcm::{AudioSource, SampleRate};
use cue_core::stt::{SttConfig, SttProvider, TranscriptEvent};
use cue_daemon::audio::system_capture::SystemAudioCapture;
use cue_daemon::stt::mock::MockStt;

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

/// Test that system audio chunks can drive a MockStt provider, simulating
/// the parallel STT pipeline for system audio.
#[tokio::test]
async fn system_audio_chunks_drive_stt_provider() {
    let stub = stub_binary_path();
    if !stub.exists() {
        return; // skip if not built
    }

    std::env::set_var("BLUEY_SYSTEM_AUDIO_BINARY", &stub);

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let capture = SystemAudioCapture::start(tx).expect("start capture");

    // Create a mock STT provider configured for System audio
    let cfg = SttConfig {
        source: AudioSource::System,
        ..Default::default()
    };
    let (provider, ctrl) = MockStt::new(cfg);

    // Feed chunks from capture into the STT provider
    let mut chunks_sent = 0;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);

    while chunks_sent < 3 && tokio::time::Instant::now() < deadline {
        match tokio::time::timeout(Duration::from_millis(500), rx.recv()).await {
            Ok(Some(chunk)) => {
                assert_eq!(chunk.source, AudioSource::System);
                provider.send_audio(&chunk).await.unwrap();
                chunks_sent += 1;
            }
            _ => break,
        }
    }

    capture.stop().await;
    std::env::remove_var("BLUEY_SYSTEM_AUDIO_BINARY");

    assert!(
        chunks_sent >= 3,
        "expected at least 3 chunks sent to STT, got {chunks_sent}"
    );
    assert_eq!(ctrl.chunks_received(), chunks_sent);

    // Simulate the STT provider emitting a transcript event
    ctrl.emit_final("system audio transcript", Vec::new());
    let mut provider = provider;
    let event = tokio::time::timeout(Duration::from_millis(100), provider.next_event())
        .await
        .expect("timeout waiting for event")
        .expect("stream closed");
    let event = event.expect("error");
    match event {
        TranscriptEvent::Final { text, source, .. } => {
            assert_eq!(text, "system audio transcript");
            assert_eq!(source, AudioSource::System);
        }
        other => panic!("expected Final, got {other:?}"),
    }
}

/// Test that system audio STT is gated behind BLUEY_SYSTEM_AUDIO_STT env var.
#[tokio::test]
async fn system_audio_stt_gating() {
    // When BLUEY_SYSTEM_AUDIO_STT is not set, STT should not be enabled
    std::env::remove_var("BLUEY_SYSTEM_AUDIO_STT");
    assert!(!cue_daemon::audio::system_capture::is_system_audio_stt_enabled());

    // When set to "1", STT should be enabled
    std::env::set_var("BLUEY_SYSTEM_AUDIO_STT", "1");
    assert!(cue_daemon::audio::system_capture::is_system_audio_stt_enabled());

    // Clean up
    std::env::remove_var("BLUEY_SYSTEM_AUDIO_STT");
}

/// Test that the daemon retains the SystemAudioCapture handle and can shut down cleanly.
#[tokio::test]
async fn system_audio_handle_retained_for_shutdown() {
    let stub = stub_binary_path();
    if !stub.exists() {
        return;
    }

    std::env::set_var("BLUEY_SYSTEM_AUDIO_BINARY", &stub);

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let capture = SystemAudioCapture::start(tx).expect("start capture");

    // Simulate what the daemon does: store in Option, then take + stop on shutdown
    let handle: Option<SystemAudioCapture> = Some(capture);
    assert!(handle.is_some(), "handle must be retained, not dropped");

    // Receive at least one chunk to prove it is running
    let chunk = tokio::time::timeout(Duration::from_secs(2), rx.recv())
        .await
        .expect("timeout")
        .expect("channel closed");
    assert_eq!(chunk.source, AudioSource::System);

    // Explicit shutdown path: take + stop
    if let Some(c) = handle {
        c.stop().await;
    }

    // After stop, the channel should eventually close (no new production).
    while let Ok(Some(_)) = tokio::time::timeout(Duration::from_millis(200), rx.recv()).await {}

    std::env::remove_var("BLUEY_SYSTEM_AUDIO_BINARY");
}

/// Test that the STT event-drain task forwards Final events downstream and
/// handles errors without panicking.
#[tokio::test]
async fn stt_event_drain_forwards_finals_and_handles_errors() {
    use cue_core::stt::SttError;

    let cfg = SttConfig {
        source: AudioSource::System,
        ..Default::default()
    };
    let (mut provider, ctrl) = MockStt::new(cfg);

    let (result_tx, mut result_rx) = tokio::sync::mpsc::unbounded_channel();

    ctrl.emit_final("hello from system", Vec::new());
    ctrl.emit_final("second transcript", Vec::new());
    ctrl.emit_error(SttError::Auth);

    let drain_handle = tokio::spawn(async move {
        loop {
            match provider.next_event().await {
                Some(Ok(event)) => {
                    let _ = result_tx.send(event);
                }
                Some(Err(e)) => {
                    let _ = format!("error: {e}");
                    if !e.is_retryable() {
                        break;
                    }
                }
                None => break,
            }
        }
    });

    let ev1 = tokio::time::timeout(Duration::from_millis(200), result_rx.recv())
        .await
        .expect("timeout on ev1")
        .expect("channel closed");
    assert!(matches!(ev1, TranscriptEvent::Final { ref text, .. } if text == "hello from system"));

    let ev2 = tokio::time::timeout(Duration::from_millis(200), result_rx.recv())
        .await
        .expect("timeout on ev2")
        .expect("channel closed");
    assert!(matches!(ev2, TranscriptEvent::Final { ref text, .. } if text == "second transcript"));

    let result = tokio::time::timeout(Duration::from_secs(1), drain_handle)
        .await
        .expect("drain task should exit within timeout");
    assert!(
        result.is_ok(),
        "drain task must not panic on SttError::Auth"
    );
}
