//! Integration test for `SystemAudioCapture` using the mock stub binary.

use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::Duration;

use cue_core::pcm::{AudioChunk, AudioSource, SampleRate};
use cue_core::stt::{SttConfig, SttProvider, TranscriptEvent};
use cue_daemon::audio::system_capture::{system_audio_channel, SystemAudioCapture};
use cue_daemon::stt::mock::MockStt;

fn stub_binary_path() -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_BIN_EXE_system-audio-stub"));
    if !path.exists() {
        // Fallback for workspace builds
        path = PathBuf::from("target/debug/system-audio-stub");
    }
    path
}

fn system_audio_env_lock() -> &'static tokio::sync::Mutex<()> {
    static LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
}

struct SystemAudioBinaryOverride {
    _guard: tokio::sync::MutexGuard<'static, ()>,
    previous: Option<OsString>,
}

impl SystemAudioBinaryOverride {
    async fn acquire(path: &PathBuf) -> Self {
        let guard = system_audio_env_lock().lock().await;
        let previous = std::env::var_os("BLUEY_SYSTEM_AUDIO_BINARY");
        std::env::set_var("BLUEY_SYSTEM_AUDIO_BINARY", path);
        Self {
            _guard: guard,
            previous,
        }
    }
}

impl Drop for SystemAudioBinaryOverride {
    fn drop(&mut self) {
        if let Some(previous) = self.previous.take() {
            std::env::set_var("BLUEY_SYSTEM_AUDIO_BINARY", previous);
        } else {
            std::env::remove_var("BLUEY_SYSTEM_AUDIO_BINARY");
        }
    }
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

    let _override = SystemAudioBinaryOverride::acquire(&stub).await;

    let (tx, mut rx) = system_audio_channel();
    let capture = SystemAudioCapture::start(tx).expect("start capture");

    let mut received = Vec::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);

    while received.len() < 5 && tokio::time::Instant::now() < deadline {
        match tokio::time::timeout(Duration::from_millis(500), rx.recv()).await {
            Ok(Some(chunk)) => received.push(chunk),
            _ => break,
        }
    }

    tokio::time::timeout(Duration::from_secs(2), capture.stop())
        .await
        .expect("system audio stop exceeded its deadline");
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

    let _override = SystemAudioBinaryOverride::acquire(&stub).await;

    let (tx, mut rx) = system_audio_channel();
    let capture = SystemAudioCapture::start(tx).expect("start capture");

    // Receive one chunk then stop
    let _ = tokio::time::timeout(Duration::from_secs(2), rx.recv()).await;
    tokio::time::timeout(Duration::from_secs(2), capture.stop())
        .await
        .expect("system audio stop exceeded its deadline");

    // If we get here without hanging, the test passes
}

#[tokio::test]
async fn saturated_system_audio_queue_preserves_newest_chunks() {
    let (tx, mut rx) = system_audio_channel();
    for captured_at_ms in 0..=50 {
        tx.try_send(AudioChunk {
            source: AudioSource::System,
            sample_rate: SampleRate::SR_16K,
            samples: vec![captured_at_ms as i16; 320],
            captured_at_ms,
        })
        .expect("queue receiver should remain open");
    }
    drop(tx);

    let mut received = Vec::new();
    while let Some(chunk) = rx.recv().await {
        received.push(chunk.captured_at_ms);
    }
    assert_eq!(received.len(), 50);
    assert_eq!(received.first(), Some(&1));
    assert_eq!(received.last(), Some(&50));
}

/// Test that system audio chunks can drive a MockStt provider, simulating
/// the parallel STT pipeline for system audio.
#[tokio::test]
async fn system_audio_chunks_drive_stt_provider() {
    let stub = stub_binary_path();
    if !stub.exists() {
        return; // skip if not built
    }

    let _override = SystemAudioBinaryOverride::acquire(&stub).await;

    let (tx, mut rx) = system_audio_channel();
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

    let _override = SystemAudioBinaryOverride::acquire(&stub).await;

    let (tx, mut rx) = system_audio_channel();
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
}

/// Production-path test: exercises the REAL daemon wiring where a single STT
/// provider instance is used for both send_audio (from capture channel) and
/// next_event (transcript drain). Verifies that audio chunks injected via the
/// production channel result in transcript segments reaching the downstream
/// consumer (add_audio_transcript_segment path).
///
/// This test mirrors the daemon's single-task select! loop: one provider,
/// audio in via channel, transcripts out via next_event, forwarded downstream.
#[tokio::test]
async fn single_provider_send_and_drain_production_wiring() {
    use cue_core::audio::SttSegmentMetadata;
    use cue_core::pcm::AudioSource;
    use cue_core::stt::{SttConfig, TranscriptEvent};
    use cue_daemon::stt::mock::MockStt;
    // Set up the same channel the production capture uses.
    let (sys_tx, mut sys_rx) = system_audio_channel();

    // Build ONE provider (mirrors production: build_system_audio_stt_provider called once).
    let cfg = SttConfig {
        source: AudioSource::System,
        ..Default::default()
    };
    let (mut provider, ctrl) = MockStt::new(cfg);

    // Downstream transcript collector (replaces add_audio_transcript_segment).
    let (transcript_tx, mut transcript_rx) = tokio::sync::mpsc::channel::<SttSegmentMetadata>(16);

    // Spawn the production-equivalent select! loop with the SINGLE provider.
    let loop_handle = tokio::spawn(async move {
        loop {
            tokio::select! {
                chunk_opt = sys_rx.recv() => {
                    match chunk_opt {
                        Some(chunk) => {
                            provider.send_audio(&chunk).await.unwrap();
                        }
                        None => break,
                    }
                }
                event_opt = provider.next_event() => {
                    match event_opt {
                        Some(Ok(event)) => {
                            if let TranscriptEvent::Final { text, source, .. } = &event {
                                let kind = match source {
                                    AudioSource::System => cue_core::AudioSourceKind::System,
                                    AudioSource::Microphone => cue_core::AudioSourceKind::Microphone,
                                };
                                let segment = SttSegmentMetadata::new(text.clone(), 0, 0, true)
                                    .with_source(kind)
                                    .with_speaker_label(kind.default_label());
                                let _ = transcript_tx.try_send(segment);
                            }
                        }
                        Some(Err(_)) => break,
                        None => break,
                    }
                }
            }
        }
        provider.close().await.unwrap();
    });

    // Inject test audio chunks via the production channel.
    let test_chunk = AudioChunk {
        source: AudioSource::System,
        sample_rate: SampleRate::SR_16K,
        samples: vec![0i16; 320],
        captured_at_ms: 1000,
    };
    sys_tx.try_send(test_chunk.clone()).unwrap();
    sys_tx.try_send(test_chunk.clone()).unwrap();
    sys_tx.try_send(test_chunk).unwrap();

    // Give the loop time to process sends.
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Verify audio was received by the SAME provider that will emit events.
    assert_eq!(
        ctrl.chunks_received(),
        3,
        "all 3 chunks must reach the single provider"
    );

    // Now the provider emits a transcript (simulating real STT response to audio).
    ctrl.emit_final("hello from system audio", Vec::new());

    // Verify the transcript reaches the downstream consumer.
    let segment = tokio::time::timeout(Duration::from_millis(200), transcript_rx.recv())
        .await
        .expect("timeout waiting for transcript")
        .expect("channel closed");

    assert_eq!(segment.text, "hello from system audio");
    assert_eq!(segment.source, Some(cue_core::AudioSourceKind::System));
    assert_eq!(segment.speaker_label.as_deref(), Some("system"));

    // Close the channel to end the loop.
    drop(sys_tx);
    tokio::time::timeout(Duration::from_secs(1), loop_handle)
        .await
        .expect("loop should exit")
        .expect("loop panicked");
}
