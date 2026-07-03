//! Throwaway probe: run the real diarizer on a 16 kHz mono WAV and print
//! segments + distinct speaker ids. Isolates diarization from live capture.
//!
//!   cargo run -p cue-diarize --example probe --target aarch64-apple-darwin -- <wav>

use std::collections::BTreeSet;

fn main() -> anyhow::Result<()> {
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "crates/cue-daemon/tests/fixtures/clip.wav".to_string());
    eprintln!("loading {path}");

    // Decode WAV → f32 by walking RIFF chunks (16-bit PCM assumed).
    let bytes = std::fs::read(&path)?;
    let samples = decode_wav_i16(&bytes)?;
    let audio: Vec<f32> = samples.iter().map(|&s| s as f32 / 32768.0).collect();
    eprintln!(
        "{} samples ≈ {:.1}s @16kHz",
        audio.len(),
        audio.len() as f32 / 16000.0
    );

    let backend = cue_diarize::Backend::preferred();
    eprintln!("backend: {backend:?} — loading model…");
    let mut d = cue_diarize::Diarizer::load(backend)?;

    eprintln!("running diarization…");
    let segments = d.diarize(&audio)?;

    let speakers: BTreeSet<i64> = segments.iter().map(|s| s.speaker).collect();
    let file_id = std::path::Path::new(&path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("audio");

    // If PROBE_RTTM=1, emit RTTM lines only (for DER scoring); else human summary.
    if std::env::var("PROBE_RTTM").is_ok() {
        for s in &segments {
            // SPEAKER <file> 1 <start> <dur> <NA> <NA> spk<NN> <NA> <NA>
            println!(
                "SPEAKER {} 1 {:.3} {:.3} <NA> <NA> spk{:02} <NA> <NA>",
                file_id,
                s.start,
                s.end - s.start,
                s.speaker
            );
        }
        return Ok(());
    }

    println!("\n=== RESULT ===");
    println!("segments: {}", segments.len());
    println!("distinct speakers: {} → {:?}", speakers.len(), speakers);
    println!();
    for s in &segments {
        println!(
            "  Speaker {} : {:6.2}s → {:6.2}s  ({:.1}s)",
            s.speaker,
            s.start,
            s.end,
            s.end - s.start
        );
    }
    Ok(())
}

/// Minimal WAV → i16 PCM: find the `data` chunk and read 16-bit LE samples.
fn decode_wav_i16(bytes: &[u8]) -> anyhow::Result<Vec<i16>> {
    anyhow::ensure!(
        bytes.len() > 44 && &bytes[0..4] == b"RIFF",
        "not a RIFF/WAV"
    );
    let mut pos = 12; // skip RIFF header + WAVE
    while pos + 8 <= bytes.len() {
        let id = &bytes[pos..pos + 4];
        let sz = u32::from_le_bytes([
            bytes[pos + 4],
            bytes[pos + 5],
            bytes[pos + 6],
            bytes[pos + 7],
        ]) as usize;
        let body = pos + 8;
        if id == b"data" {
            let end = (body + sz).min(bytes.len());
            let mut out = Vec::with_capacity((end - body) / 2);
            let mut i = body;
            while i + 1 < end {
                out.push(i16::from_le_bytes([bytes[i], bytes[i + 1]]));
                i += 2;
            }
            return Ok(out);
        }
        pos = body + sz + (sz & 1); // chunks are word-aligned
    }
    anyhow::bail!("no data chunk found")
}
