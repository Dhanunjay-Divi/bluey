//! Integration test for the STT provider contract (capture → STT).
//!
//! Uses synthetic audio (no real microphone) and `MockStt` in place of Deepgram
//! to prove the `SttProvider` trait wiring — connection-state transitions and the
//! finalize/close round-trip. (The continuous engine feed is deliberately never
//! VAD-gated — see the invariant in app.rs — so there is no VAD stage here.)

use cue_core::pcm::{AudioChunk, AudioSource, SampleRate};
use cue_core::stt::{ConnectionState, SttConfig, SttProvider, TranscriptEvent};
use cue_daemon::stt::mock::MockStt;

/// Build a 20 ms chunk at 16 kHz filled with the provided sample value.
fn synth_chunk(value: i16, captured_at_ms: u64) -> AudioChunk {
    AudioChunk {
        source: AudioSource::Microphone,
        sample_rate: SampleRate::new(16_000).unwrap(),
        samples: vec![value; 320],
        captured_at_ms,
    }
}

#[tokio::test]
async fn pipeline_delivers_scripted_transcript() {
    let (mut provider, ctrl) = MockStt::new(SttConfig::default());

    // Feed a few chunks straight through (no VAD gate — the real pipeline feeds
    // the engine continuously).
    for i in 0..5 {
        let amp = if i % 2 == 0 { 25_000 } else { -25_000 };
        provider
            .send_audio(&synth_chunk(amp, i as u64 * 20))
            .await
            .unwrap();
    }
    assert_eq!(ctrl.chunks_received(), 5);

    // Pre-queue transcript events and drain.
    ctrl.emit_partial("hello");
    ctrl.emit_partial("hello world");
    ctrl.emit_final("hello world", Vec::new());

    let e1 = provider.next_event().await.unwrap().unwrap();
    let e2 = provider.next_event().await.unwrap().unwrap();
    let e3 = provider.next_event().await.unwrap().unwrap();
    assert!(matches!(e1, TranscriptEvent::Partial { .. }));
    assert!(matches!(e2, TranscriptEvent::Partial { .. }));
    assert!(matches!(e3, TranscriptEvent::Final { .. }));
}

#[tokio::test]
async fn pipeline_handles_connection_state_transitions() {
    let (provider, ctrl) = MockStt::new(SttConfig::default());
    assert_eq!(provider.connection_state(), ConnectionState::Connected);

    ctrl.set_connection_state(ConnectionState::Reconnecting { attempt: 2 });
    assert_eq!(
        provider.connection_state(),
        ConnectionState::Reconnecting { attempt: 2 }
    );

    ctrl.set_connection_state(ConnectionState::Failed);
    assert_eq!(provider.connection_state(), ConnectionState::Failed);
}

#[tokio::test]
async fn pipeline_finalize_and_close_round_trip() {
    let (mut provider, ctrl) = MockStt::new(SttConfig::default());
    let chunk = synth_chunk(10_000, 0);
    provider.send_audio(&chunk).await.unwrap();
    provider.finalize().await.unwrap();
    assert!(ctrl.was_finalized());

    provider.close().await.unwrap();
    assert!(ctrl.was_closed());
    // After close, send_audio must error — cannot continue the pipeline.
    let err = provider.send_audio(&chunk).await.unwrap_err();
    assert!(matches!(err, cue_core::stt::SttError::NotActive));
}
