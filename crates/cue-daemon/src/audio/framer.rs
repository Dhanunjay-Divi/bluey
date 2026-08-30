//! Audio framing primitives: ring-buffered accumulator that produces
//! fixed-duration `AudioChunk`s from variable-size capture callbacks.
//!
//! CPAL (and most OS audio APIs) deliver samples in callback-sized buffers
//! whose length depends on the driver / device / OS scheduler. We want to
//! feed the rest of the pipeline a steady 20 ms cadence so downstream
//! components (VAD, STT) can reason about framing. `Framer` absorbs the
//! jitter and emits complete chunks.

use cue_core::pcm::{AudioChunk, AudioSource, SampleRate};

/// Build fixed-duration [`AudioChunk`]s from streaming samples.
///
/// Not thread-safe by itself — wrap in a `Mutex`, or (recommended) pair the
/// producer side of a `ringbuf::HeapRb` with a dedicated consumer task that
/// owns the `Framer`.
pub struct Framer {
    source: AudioSource,
    sample_rate: SampleRate,
    /// How many samples per emitted chunk (e.g. 320 for 20 ms @ 16 kHz).
    chunk_samples: usize,
    /// Pending samples that haven't yet filled a full chunk.
    buffer: Vec<i16>,
    /// Monotonic ms clock at which the oldest buffered sample arrived.
    /// Used as `AudioChunk::captured_at_ms` when the chunk emits.
    head_captured_at_ms: u64,
    /// Most recent capture timestamp the caller supplied.
    latest_captured_at_ms: u64,
}

impl Framer {
    /// New framer targeting `chunk_ms` milliseconds per emitted chunk.
    ///
    /// Panics on `chunk_ms == 0` because that would emit an empty chunk per
    /// sample and is never intended.
    pub fn new(source: AudioSource, sample_rate: SampleRate, chunk_ms: u32) -> Self {
        assert!(chunk_ms > 0, "chunk_ms must be non-zero");
        let chunk_samples = (sample_rate.hz() as u64 * chunk_ms as u64 / 1000) as usize;
        Self {
            source,
            sample_rate,
            chunk_samples,
            buffer: Vec::with_capacity(chunk_samples * 2),
            head_captured_at_ms: 0,
            latest_captured_at_ms: 0,
        }
    }

    /// Samples per emitted chunk.
    pub fn chunk_samples(&self) -> usize {
        self.chunk_samples
    }

    /// Currently-buffered sample count (partial chunk waiting to emit).
    pub fn pending(&self) -> usize {
        self.buffer.len()
    }

    /// Push a block of samples with a monotonic timestamp for the FIRST
    /// sample in this block. Returns zero or more completed chunks.
    ///
    /// Under load we can emit multiple chunks in one call (if the caller
    /// pushed > 1 chunk's worth of samples at once).
    pub fn push(&mut self, samples: &[i16], captured_at_ms: u64) -> Vec<AudioChunk> {
        if samples.is_empty() {
            return Vec::new();
        }
        if self.buffer.is_empty() {
            self.head_captured_at_ms = captured_at_ms;
        }
        self.latest_captured_at_ms = captured_at_ms;
        self.buffer.extend_from_slice(samples);

        let mut emitted = Vec::new();
        while self.buffer.len() >= self.chunk_samples {
            let drain: Vec<i16> = self.buffer.drain(..self.chunk_samples).collect();
            let chunk = AudioChunk {
                source: self.source,
                sample_rate: self.sample_rate,
                samples: drain,
                captured_at_ms: self.head_captured_at_ms,
            };
            // Advance the head timestamp by the emitted chunk's duration.
            let emitted_duration_ms =
                self.chunk_samples as u64 * 1000 / self.sample_rate.hz() as u64;
            self.head_captured_at_ms = self.head_captured_at_ms.saturating_add(emitted_duration_ms);
            emitted.push(chunk);
        }
        emitted
    }

    /// Flush any buffered partial frame as a chunk, padding with zeros to
    /// reach the target chunk size. Use on shutdown so we don't drop the
    /// trailing tail.
    pub fn flush_padded(&mut self) -> Option<AudioChunk> {
        if self.buffer.is_empty() {
            return None;
        }
        let pad_needed = self.chunk_samples.saturating_sub(self.buffer.len());
        self.buffer.extend(std::iter::repeat_n(0i16, pad_needed));
        let drained = std::mem::replace(
            &mut self.buffer,
            Vec::with_capacity(self.chunk_samples.saturating_mul(2)),
        );
        Some(AudioChunk {
            source: self.source,
            sample_rate: self.sample_rate,
            samples: drained,
            captured_at_ms: self.head_captured_at_ms,
        })
    }
}

/// Convenience: convert a buffer of 32-bit floats (CPAL's common native
/// format) to signed 16-bit little-endian PCM. Clamps to avoid wraparound
/// on values outside [-1.0, 1.0].
pub fn f32_to_i16(src: &[f32], out: &mut Vec<i16>) {
    out.reserve(src.len());
    for &s in src {
        let clamped = s.clamp(-1.0, 1.0);
        // 32767 not 32768 so symmetric around zero.
        let v = (clamped * i16::MAX as f32).round() as i32;
        out.push(v.clamp(i16::MIN as i32, i16::MAX as i32) as i16);
    }
}

/// Convenience: downmix interleaved N-channel PCM into mono by averaging
/// all channels. The input length must be a multiple of `channels`;
/// otherwise the trailing partial frame is dropped.
pub fn downmix_to_mono(interleaved: &[i16], channels: usize, out: &mut Vec<i16>) {
    if channels == 0 {
        return;
    }
    if channels == 1 {
        out.extend_from_slice(interleaved);
        return;
    }
    let frames = interleaved.len() / channels;
    out.reserve(frames);
    for frame in 0..frames {
        let start = frame * channels;
        let sum: i32 = interleaved[start..start + channels]
            .iter()
            .map(|&s| s as i32)
            .sum();
        out.push((sum / channels as i32) as i16);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rate_16k() -> SampleRate {
        SampleRate::new(16_000).unwrap()
    }

    #[test]
    fn framer_emits_chunks_at_target_size() {
        let mut f = Framer::new(AudioSource::Microphone, rate_16k(), 20);
        assert_eq!(f.chunk_samples(), 320); // 20 ms @ 16 kHz
                                            // push 200 samples — no chunk yet
        let out = f.push(&vec![0i16; 200], 1_000);
        assert!(out.is_empty());
        assert_eq!(f.pending(), 200);
        // push 200 more — exactly 400, should emit 1 chunk of 320, leave 80
        let out = f.push(&vec![0i16; 200], 1_012);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].samples.len(), 320);
        assert_eq!(out[0].captured_at_ms, 1_000);
        assert_eq!(f.pending(), 80);
    }

    #[test]
    fn framer_emits_multiple_chunks_when_push_is_large() {
        let mut f = Framer::new(AudioSource::Microphone, rate_16k(), 20);
        // push 1000 samples → should emit 3 chunks (960 samples), leave 40
        let out = f.push(&vec![0i16; 1000], 2_000);
        assert_eq!(out.len(), 3);
        assert_eq!(f.pending(), 40);
        // Timestamps advance by chunk duration each
        assert_eq!(out[0].captured_at_ms, 2_000);
        assert_eq!(out[1].captured_at_ms, 2_020);
        assert_eq!(out[2].captured_at_ms, 2_040);
    }

    #[test]
    fn framer_flush_pads_final_partial_chunk() {
        let mut f = Framer::new(AudioSource::System, rate_16k(), 20);
        f.push(&[1i16; 100], 5_000);
        let flushed = f.flush_padded().unwrap();
        assert_eq!(flushed.samples.len(), 320);
        assert_eq!(flushed.samples[99], 1); // original content preserved
        assert_eq!(flushed.samples[100], 0); // zero-padded
        assert_eq!(flushed.captured_at_ms, 5_000);
        assert!(f.flush_padded().is_none());
    }

    #[test]
    fn framer_flush_empty_returns_none() {
        let mut f = Framer::new(AudioSource::Microphone, rate_16k(), 20);
        assert!(f.flush_padded().is_none());
    }

    #[test]
    fn framer_refills_without_losing_preallocated_capacity_after_flush() {
        let mut f = Framer::new(AudioSource::Microphone, rate_16k(), 20);
        f.push(&[1i16; 100], 5_000);
        assert!(f.flush_padded().is_some());
        assert!(f.buffer.capacity() >= f.chunk_samples().saturating_mul(2));

        let emitted = f.push(&[2i16; 320], 5_020);
        assert_eq!(emitted.len(), 1);
        assert_eq!(emitted[0].samples, vec![2i16; 320]);
    }

    #[test]
    fn framer_handles_48khz_correctly() {
        let f = Framer::new(
            AudioSource::Microphone,
            SampleRate::new(48_000).unwrap(),
            20,
        );
        assert_eq!(f.chunk_samples(), 960); // 20 ms @ 48 kHz
    }

    #[test]
    fn f32_to_i16_clamps_and_scales() {
        let mut out = Vec::new();
        f32_to_i16(&[0.0, 1.0, -1.0, 1.5, -2.0, 0.5], &mut out);
        assert_eq!(out[0], 0);
        assert_eq!(out[1], i16::MAX);
        assert_eq!(out[2], -i16::MAX); // symmetric
        assert_eq!(out[3], i16::MAX); // clamped above 1.0
        assert_eq!(out[4], -i16::MAX); // clamped below -1.0
                                       // 0.5 scales to ~16383
        assert!((out[5] - 16383).abs() <= 1);
    }

    #[test]
    fn downmix_stereo_to_mono_averages() {
        let stereo = vec![100i16, 200, 1000, 2000, -50, 50];
        let mut mono = Vec::new();
        downmix_to_mono(&stereo, 2, &mut mono);
        assert_eq!(mono, vec![150, 1500, 0]);
    }

    #[test]
    fn downmix_mono_is_passthrough() {
        let mono_in = vec![1i16, 2, 3];
        let mut out = Vec::new();
        downmix_to_mono(&mono_in, 1, &mut out);
        assert_eq!(out, mono_in);
    }

    #[test]
    fn downmix_with_trailing_partial_frame_drops_it() {
        // 5 samples, 2 channels → 2 full frames + 1 leftover sample
        let stereo = vec![10i16, 20, 30, 40, 99];
        let mut mono = Vec::new();
        downmix_to_mono(&stereo, 2, &mut mono);
        assert_eq!(mono, vec![15, 35]); // 99 dropped
    }
}
