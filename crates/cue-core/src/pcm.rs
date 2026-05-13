//! Audio primitives used across the bluey listening pipeline.
//!
//! The daemon captures audio natively (CPAL on all platforms for mic; system
//! audio uses ScreenCaptureKit on macOS and WASAPI loopback on Windows in a
//! later phase). Everything downstream — VAD, resampling, STT — operates on
//! the types in this module so the pipeline is platform-agnostic.
//!
//! Samples are always `i16` little-endian mono after normalization, which is
//! what every STT provider we integrate accepts. Conversion from device
//! formats (f32 stereo, etc.) happens at the capture boundary.

use serde::{Deserialize, Serialize};

/// Which audio source a chunk originates from.
///
/// Kept deliberately narrow for Phase 3 — only the user's mic is wired
/// initially. System audio arrives with a different variant when we add
/// ScreenCaptureKit / WASAPI loopback capture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AudioSource {
    /// User's microphone (what the bluey operator says).
    Microphone,
    /// System audio (what the other party says, captured via OS loopback).
    System,
}

impl AudioSource {
    pub fn as_str(self) -> &'static str {
        match self {
            AudioSource::Microphone => "microphone",
            AudioSource::System => "system",
        }
    }
}

/// Sample rate in Hz. Constructed via the `SampleRate::new(u32)` helper which
/// rejects zero. STT providers accept 8000 / 16000 / 44100 / 48000; other
/// values are passed through untouched so callers can carry through the device
/// native rate and resample later in the pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SampleRate(u32);

impl SampleRate {
    /// Standard 16 kHz — most STT APIs default here.
    pub const SR_16K: SampleRate = SampleRate(16_000);
    /// 48 kHz — macOS CoreAudio native rate.
    pub const SR_48K: SampleRate = SampleRate(48_000);

    /// Construct from a raw Hz value. Returns `None` for zero.
    pub fn new(hz: u32) -> Option<Self> {
        if hz == 0 {
            None
        } else {
            Some(SampleRate(hz))
        }
    }

    #[inline]
    pub fn hz(self) -> u32 {
        self.0
    }
}

/// A chunk of PCM16-LE mono audio captured from one source.
///
/// Chunk duration is determined by the capturer's framing (typically 20 ms).
/// The pipeline does not require chunks to be any specific length — both VAD
/// and STT buffer internally — but capturers should stay under ~100 ms to
/// keep end-to-end latency low.
#[derive(Debug, Clone)]
pub struct AudioChunk {
    pub source: AudioSource,
    pub sample_rate: SampleRate,
    pub samples: Vec<i16>,
    /// Monotonic timestamp (millis since capture start) of the FIRST sample
    /// in this chunk. Used by VAD for speech-duration tracking and by the
    /// latency pipeline to carry the original capture time forward through
    /// every hop.
    pub captured_at_ms: u64,
}

impl AudioChunk {
    /// Duration of this chunk in milliseconds, derived from sample count /
    /// sample rate. Used by VAD state machines and diagnostic logging.
    pub fn duration_ms(&self) -> u32 {
        let hz = self.sample_rate.hz() as u64;
        if hz == 0 {
            return 0;
        }
        ((self.samples.len() as u64 * 1000) / hz) as u32
    }

    /// Bytes this chunk would occupy as little-endian raw PCM16. Exposed so
    /// STT providers can pre-size their outbound buffers.
    pub fn byte_len(&self) -> usize {
        self.samples.len() * 2
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_rate_rejects_zero() {
        assert!(SampleRate::new(0).is_none());
        assert_eq!(SampleRate::new(16_000).unwrap().hz(), 16_000);
    }

    #[test]
    fn audio_chunk_duration_calculation() {
        let chunk = AudioChunk {
            source: AudioSource::Microphone,
            sample_rate: SampleRate::SR_16K,
            samples: vec![0i16; 320], // 320 samples @ 16kHz = 20ms
            captured_at_ms: 0,
        };
        assert_eq!(chunk.duration_ms(), 20);
        assert_eq!(chunk.byte_len(), 640);
    }

    #[test]
    fn audio_chunk_duration_handles_varied_rates() {
        let chunk = AudioChunk {
            source: AudioSource::System,
            sample_rate: SampleRate::new(48_000).unwrap(),
            samples: vec![0i16; 960], // 20 ms @ 48 kHz
            captured_at_ms: 0,
        };
        assert_eq!(chunk.duration_ms(), 20);
        assert_eq!(chunk.byte_len(), 1920);
    }

    #[test]
    fn audio_source_serde_roundtrip() {
        let s = serde_json::to_string(&AudioSource::Microphone).unwrap();
        assert_eq!(s, r#""microphone""#);
        let back: AudioSource = serde_json::from_str(r#""system""#).unwrap();
        assert_eq!(back, AudioSource::System);
    }
}
