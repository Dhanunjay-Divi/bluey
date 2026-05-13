//! End-to-end integration test for the capture → VAD → STT pipeline.
//!
//! Uses synthetic audio (no real microphone), `TwoStageVad` (real), and
//! `MockStt` in place of Deepgram. Proves the Phase 3 Round 2 wiring
//! conforms to the `SttProvider` trait and VAD decision contract.

use cue_core::pcm::{AudioChunk, AudioSource, SampleRate};
use cue_core::stt::{ConnectionState, SttConfig, SttProvider, TranscriptEvent};
use cue_core::vad::VadConfig;
use cue_daemon::audio::vad::TwoStageVad;
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
async fn pipeline_silence_is_dropped_before_stt() {
    let (provider, ctrl) = MockStt::new(SttConfig::default());
    let mut vad = TwoStageVad::new(
        &VadConfig {
            silence_hangover_frames: 1,
            ..Default::default()
        },
        SampleRate::new(16_000).unwrap(),
    )
    .unwrap();

    // Feed 10 silent chunks → hangover=1, so 1 SendSilence then 9 Drops.
    for i in 0..10 {
        let chunk = synth_chunk(0, i * 20);
        let action = vad.process(&chunk);
        if action.should_forward() {
            provider.send_audio(&chunk).await.unwrap();
        }
    }

    // At most 1 chunk forwarded (the hangover sendSilence), others dropped.
    assert!(
        ctrl.chunks_received() <= 1,
        "silent chunks must be dropped by VAD (got {})",
        ctrl.chunks_received()
    );
}

#[tokio::test]
async fn pipeline_loud_audio_passes_to_stt_and_delivers_scripted_transcript() {
    let (mut provider, ctrl) = MockStt::new(SttConfig::default());
    let mut vad =
        TwoStageVad::new(&VadConfig::default(), SampleRate::new(16_000).unwrap()).unwrap();

    // Feed 5 loud chunks — should all pass RMS stage.
    // WebRTC VAD may classify synthetic constant-amplitude frames as non-speech
    // (degrading Send → SendSilence), but both still forward.
    let mut forwarded = 0;
    for i in 0..5 {
        // Alternating +/- to build an actual waveform (non-DC).
        let amp = if i % 2 == 0 { 25_000 } else { -25_000 };
        let chunk = synth_chunk(amp, i as u64 * 20);
        if vad.process(&chunk).should_forward() {
            provider.send_audio(&chunk).await.unwrap();
            forwarded += 1;
        }
    }
    assert!(forwarded > 0, "RMS gate should admit loud chunks");
    assert_eq!(ctrl.chunks_received(), forwarded);

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
