//! Proof test: the engine transcribes a real WAV correctly.
//!
//! Requires the model + a test WAV (skips cleanly if absent so CI without the
//! ~650MB model doesn't fail):
//!   BLUEY_PARAKEET_MODEL_DIR=/abs/path/to/models/parakeet-en \
//!   BLUEY_TEST_WAV=/abs/path/to/clip.wav \
//!   cargo test -p cue-transcribe --test wav_transcribe -- --nocapture

use cue_transcribe::SttEngine;

fn decode_wav_i16(path: &str) -> Vec<i16> {
    let mut r = hound::WavReader::open(path).expect("open wav");
    r.samples::<i16>().map(|s| s.expect("sample")).collect()
}

#[test]
fn transcribes_a_wav() {
    let Ok(model_dir) = std::env::var("BLUEY_PARAKEET_MODEL_DIR") else {
        eprintln!("skip: BLUEY_PARAKEET_MODEL_DIR not set");
        return;
    };
    let Ok(wav) = std::env::var("BLUEY_TEST_WAV") else {
        eprintln!("skip: BLUEY_TEST_WAV not set");
        return;
    };

    let samples = decode_wav_i16(&wav);
    assert!(!samples.is_empty(), "wav decoded to no samples");

    let mut engine = SttEngine::load(&model_dir).expect("load engine");

    // Feed in 100ms chunks (1600 samples @ 16kHz), like the live capture path.
    let mut full = String::new();
    for block in samples.chunks(1600) {
        let f32s: Vec<f32> = block.iter().map(|&s| s as f32 / 32768.0).collect();
        if let Some(chunk) = engine.push(&f32s).expect("push") {
            full.push_str(&chunk.text);
        }
    }

    let transcript = full.trim();
    eprintln!("transcript: {transcript}");
    assert!(
        transcript.len() > 5,
        "expected a non-trivial transcript, got: {transcript:?}"
    );
}
