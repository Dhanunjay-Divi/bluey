//! REAL on-device Parakeet inference smoke test.
//!
//! Unlike the mock-STT loop tests, this exercises the ACTUAL `parakeet-rs`
//! Nemotron model: it builds the real [`ParakeetProvider`], feeds it a real
//! 16 kHz mono PCM16 WAV, and asserts a non-empty transcript comes back. This
//! is the one thing the headless/mock harness cannot cover.
//!
//! Gated twice so it never breaks a normal build or model-less CI:
//!   1. compiled only under `--features parakeet-stt`;
//!   2. skipped (returns Ok) unless `BLUEY_PARAKEET_MODEL_DIR` points at a dir
//!      with the model files, and a test WAV fixture exists.
//!
//! Run it with:
//!   BLUEY_PARAKEET_MODEL_DIR=/abs/path/to/models/parakeet-en \
//!     cargo test -p cue-daemon --features parakeet-stt --test parakeet_real_inference -- --nocapture

#![cfg(feature = "parakeet-stt")]

use std::path::{Path, PathBuf};

use cue_core::pcm::{AudioChunk, AudioSource, SampleRate};
use cue_core::stt::{SttProvider, TranscriptEvent};
use cue_daemon::stt::parakeet::{ParakeetPaths, ParakeetProvider};

/// Decode a 16 kHz mono PCM16 WAV by walking RIFF chunks to find `data`.
/// (Mirrors the daemon's own decoder; duplicated here to keep the test
/// self-contained and to exercise real model input shaping.)
fn decode_wav_i16(wav: &[u8]) -> Vec<i16> {
    const RIFF_HEADER_LEN: usize = 12;
    if wav.len() < RIFF_HEADER_LEN || &wav[0..4] != b"RIFF" || &wav[8..12] != b"WAVE" {
        return Vec::new();
    }
    let mut offset = RIFF_HEADER_LEN;
    while offset + 8 <= wav.len() {
        let id = &wav[offset..offset + 4];
        let size = u32::from_le_bytes([
            wav[offset + 4],
            wav[offset + 5],
            wav[offset + 6],
            wav[offset + 7],
        ]) as usize;
        let body_start = offset + 8;
        let body_end = body_start.saturating_add(size).min(wav.len());
        if id == b"data" {
            return wav[body_start..body_end]
                .chunks_exact(2)
                .map(|b| i16::from_le_bytes([b[0], b[1]]))
                .collect();
        }
        offset += 8 + size + (size & 1);
    }
    Vec::new()
}

fn model_dir() -> Option<PathBuf> {
    let dir = std::env::var("BLUEY_PARAKEET_MODEL_DIR").ok()?;
    let dir = PathBuf::from(dir.trim());
    let complete = ["encoder.onnx", "decoder_joint.onnx", "tokenizer.model"]
        .iter()
        .all(|f| dir.join(f).is_file());
    complete.then_some(dir)
}

fn fixture_wav() -> Option<PathBuf> {
    for name in ["clip8s.wav", "clip.wav"] {
        let p = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join(name);
        if p.is_file() {
            return Some(p);
        }
    }
    None
}

#[tokio::test]
async fn parakeet_transcribes_a_real_wav() {
    let Some(dir) = model_dir() else {
        eprintln!(
            "SKIP: set BLUEY_PARAKEET_MODEL_DIR to a dir with encoder.onnx/decoder_joint.onnx/tokenizer.model"
        );
        return;
    };
    let Some(wav_path) = fixture_wav() else {
        eprintln!("SKIP: no tests/fixtures/clip*.wav present");
        return;
    };

    let sortformer = dir.join("diar_streaming_sortformer_4spk-v2.onnx");
    let paths = ParakeetPaths {
        nemotron_dir: dir.clone(),
        sortformer_model: sortformer.is_file().then_some(sortformer),
    };

    let wav = std::fs::read(&wav_path).expect("read fixture wav");
    let samples = decode_wav_i16(&wav);
    assert!(
        samples.len() > 16_000,
        "expected >1s of 16kHz audio, got {} samples",
        samples.len()
    );

    // Build the REAL provider (loads the ONNX model on its worker thread).
    let mut provider = ParakeetProvider::connect(paths, AudioSource::Microphone);

    // Feed the audio in ~1s chunks (the streaming model commits blocks as it
    // accumulates), then a trailing silence chunk to flush the tail.
    for window in samples.chunks(16_000) {
        let chunk = AudioChunk {
            source: AudioSource::Microphone,
            sample_rate: SampleRate::SR_16K,
            samples: window.to_vec(),
            captured_at_ms: 0,
        };
        provider.send_audio(&chunk).await.expect("send_audio");
    }
    // Flush: a second of silence pushes the model's buffered tail out.
    let silence = AudioChunk {
        source: AudioSource::Microphone,
        sample_rate: SampleRate::SR_16K,
        samples: vec![0i16; 16_000],
        captured_at_ms: 0,
    };
    provider.send_audio(&silence).await.expect("send silence");
    provider.finalize().await.expect("finalize");

    // Drain finals with a generous overall budget (model load + inference).
    let mut transcript = String::new();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    while std::time::Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        match tokio::time::timeout(
            remaining.min(std::time::Duration::from_secs(5)),
            provider.next_event(),
        )
        .await
        {
            Ok(Some(Ok(TranscriptEvent::Final { text, .. }))) => {
                if !text.trim().is_empty() {
                    if !transcript.is_empty() {
                        transcript.push(' ');
                    }
                    transcript.push_str(text.trim());
                }
            }
            Ok(Some(Ok(_))) => {} // partial / speaker label
            Ok(Some(Err(e))) => panic!("parakeet error: {e}"),
            Ok(None) => break, // provider closed
            Err(_) => {
                // No event within the slice. If we already have text, we're done.
                if !transcript.is_empty() {
                    break;
                }
            }
        }
    }

    let _ = provider.close().await;

    eprintln!("=== REAL PARAKEET TRANSCRIPT ===\n{transcript}\n================================");
    assert!(
        !transcript.trim().is_empty(),
        "real Parakeet inference produced no transcript from {}",
        wav_path.display()
    );
    // Real speech transcribes to multiple words, not a stray token.
    assert!(
        transcript.split_whitespace().count() >= 3,
        "transcript suspiciously short: {transcript:?}"
    );
}
