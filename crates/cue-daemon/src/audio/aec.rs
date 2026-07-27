//! Reference-based acoustic echo cancellation (software AEC).
//!
//! When the user is on open SPEAKERS (no headphones), the far side's voice —
//! captured cleanly on the SYSTEM-audio stream — also plays out the speakers and
//! leaks back into the MICROPHONE, where it is transcribed a second time and
//! mislabeled as the user ("You"). Apple's VoiceProcessingIO AEC in the helper
//! handles the common case, but fails on aggregate/multi-channel input devices
//! (it can't model the echo path). This module is the software fallback that
//! covers exactly that case, using the WebRTC AEC3 algorithm (via `sonora`, a
//! pure-Rust port — no C toolchain).
//!
//! The design is standard reference-based AEC: the SYSTEM stream is the
//! reference / far-end (what is playing out the speakers), the MICROPHONE stream
//! is the near-end (what the mic hears). AEC3 adaptively subtracts the reference
//! from the near-end, so the far side's voice is removed from the mic samples
//! BEFORE they reach STT and never becomes mislabeled text.
//!
//! Both daemon audio streams are 16 kHz mono i16 and carry a shared wall-clock
//! `captured_at_ms`, so the reference can be coarse-aligned to the mic by
//! timestamp; AEC3's internal delay estimator tracks the fine per-sample delay.

use std::collections::VecDeque;

use sonora::config::EchoCanceller;
use sonora::{AudioProcessing, Config, StreamConfig};

/// 16 kHz mono: AEC3 processes 10 ms frames = 160 samples.
const AEC_SAMPLE_RATE: u32 = 16_000;
const AEC_FRAME: usize = 160;

/// Coarse system→mic acoustic delay seed (ms). The mic hears the speakers
/// ~100–250 ms after they play; 150 ms lands the reference inside AEC3's
/// delay-search window, and its adaptive filter tracks the exact delay from
/// there. Overridable via `BLUEY_AEC_DELAY_MS` for field tuning.
const DEFAULT_DELAY_MS: u64 = 150;

/// How much recent system (reference) audio to retain: ~3 s at 16 kHz. Enough to
/// cover the coarse delay plus jitter, bounded so memory can't grow.
const REFERENCE_RETAIN_SAMPLES: usize = AEC_SAMPLE_RATE as usize * 3;

fn delay_ms() -> u64 {
    std::env::var("BLUEY_AEC_DELAY_MS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(DEFAULT_DELAY_MS)
}

/// A rolling store of the most recent SYSTEM (far-end / reference) samples,
/// indexed by wall-clock so the mic path can fetch "what was playing when this
/// mic frame was captured." Shared across the system (producer) and mic
/// (consumer) capture tasks.
pub struct ReferenceRingBuffer {
    samples: VecDeque<i16>,
    /// `captured_at_ms` of the OLDEST sample still retained (front of the deque).
    base_ms: u64,
    /// True once at least one push has established `base_ms`.
    seeded: bool,
}

impl Default for ReferenceRingBuffer {
    fn default() -> Self {
        Self {
            samples: VecDeque::with_capacity(REFERENCE_RETAIN_SAMPLES),
            base_ms: 0,
            seeded: false,
        }
    }
}

impl ReferenceRingBuffer {
    /// Append a system-audio chunk (the reference) stamped with the wall-clock ms
    /// at which its FIRST sample was captured. Trims the front to the retention
    /// window, advancing `base_ms` accordingly.
    pub fn push(&mut self, samples: &[i16], captured_at_ms: u64) {
        if !self.seeded {
            self.base_ms = captured_at_ms;
            self.seeded = true;
        }
        self.samples.extend(samples.iter().copied());

        // Trim overflow from the front, advancing the base timestamp by the
        // duration of the dropped samples (1 ms = 16 samples at 16 kHz).
        if self.samples.len() > REFERENCE_RETAIN_SAMPLES {
            let drop = self.samples.len() - REFERENCE_RETAIN_SAMPLES;
            self.samples.drain(..drop);
            self.base_ms += (drop as u64 * 1000) / AEC_SAMPLE_RATE as u64;
        }
    }

    /// Read `len` reference samples aligned to `target_ms` (the wall-clock time
    /// the mic frame corresponds to, already delay-compensated by the caller).
    /// Returns zero-filled samples for any region not (yet) covered — mic-only
    /// startup, gaps, or a target before the retained window — so AEC sees a
    /// silent far-end and safely passes the mic through.
    pub fn read_aligned(&self, target_ms: u64, len: usize) -> Vec<i16> {
        let mut out = vec![0i16; len];
        if !self.seeded || self.samples.is_empty() || target_ms < self.base_ms {
            return out;
        }
        // ms since base → sample index (16 samples per ms at 16 kHz).
        let start = ((target_ms - self.base_ms) * AEC_SAMPLE_RATE as u64 / 1000) as usize;
        for (i, slot) in out.iter_mut().enumerate() {
            if let Some(&s) = self.samples.get(start + i) {
                *slot = s;
            }
        }
        out
    }

    /// True when there is meaningful reference coverage around `target_ms`, i.e.
    /// running the AEC is worthwhile. When false the caller passes the mic frame
    /// through untouched (no echo to cancel).
    pub fn has_coverage(&self, target_ms: u64, len: usize) -> bool {
        if !self.seeded || self.samples.is_empty() || target_ms < self.base_ms {
            return false;
        }
        let start = ((target_ms - self.base_ms) * AEC_SAMPLE_RATE as u64 / 1000) as usize;
        // At least the first sample of the requested window must be present.
        start < self.samples.len() && (self.samples.len() - start) >= len / 4
    }
}

/// The per-mic-task echo canceller. Wraps one AEC3 pipeline; `process` runs a
/// 100 ms (1600-sample) mic chunk through it against the time-aligned reference,
/// returning the cleaned mic samples (same length, same cadence — the STT feed's
/// uniform-100 ms invariant is preserved).
pub struct MicAec {
    apm: AudioProcessing,
    delay_ms: u64,
    /// Once the underlying AEC library errors OR panics, we stop calling it and
    /// pass the mic through untouched for the rest of the session. `sonora` is a
    /// young (v0.1.0) port and has edge-case panics in its adaptive filter; a
    /// buggy echo canceller must NEVER take down the mic capture task, and once
    /// it has faulted its internal state is untrustworthy, so we don't retry.
    poisoned: bool,
}

impl MicAec {
    pub fn new() -> Self {
        let stream = StreamConfig::new(AEC_SAMPLE_RATE, 1);
        let config = Config {
            echo_canceller: Some(EchoCanceller::default()),
            // Leave NS/AGC off: we only want echo removal, not timbre changes
            // (the transcriber, and any later diarization, want the raw voice).
            ..Default::default()
        };
        let apm = AudioProcessing::builder()
            .config(config)
            .capture_config(stream)
            .render_config(stream)
            .build();
        Self {
            apm,
            delay_ms: delay_ms(),
            poisoned: false,
        }
    }

    /// Coarse system→mic delay this canceller applies when fetching the reference.
    pub fn delay_ms(&self) -> u64 {
        self.delay_ms
    }

    /// Cancel the far-end echo from one 100 ms mic chunk.
    ///
    /// `mic` and `reference` must both be 1600 samples (ten 10 ms AEC frames).
    /// FAIL-SOFT / PANIC-SAFE: on a length mismatch, an AEC error, or a panic
    /// inside the (young) sonora library, the raw mic samples are returned
    /// unchanged and the canceller is poisoned so it is never called again this
    /// session — a mic with echo beats a crashed capture task.
    pub fn process(&mut self, mic: &[i16], reference: &[i16]) -> Vec<i16> {
        if self.poisoned || mic.len() != reference.len() || mic.len() % AEC_FRAME != 0 {
            return mic.to_vec();
        }

        let apm = &mut self.apm;
        // catch_unwind: a panic in sonora's adaptive filter must degrade to
        // pass-through, not unwind into (and abort) the audio capture task.
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut out = Vec::with_capacity(mic.len());
            let mut ref_f = [0f32; AEC_FRAME];
            let mut mic_f = [0f32; AEC_FRAME];
            let mut ref_out = [0f32; AEC_FRAME];
            let mut mic_out = [0f32; AEC_FRAME];

            for frame in 0..(mic.len() / AEC_FRAME) {
                let off = frame * AEC_FRAME;
                for i in 0..AEC_FRAME {
                    ref_f[i] = i16_to_f32(reference[off + i]);
                    mic_f[i] = i16_to_f32(mic[off + i]);
                }
                // Render first (tell AEC what is playing), then capture (cancel it).
                apm.process_render_f32(&[&ref_f], &mut [&mut ref_out]).ok()?;
                apm.process_capture_f32(&[&mic_f], &mut [&mut mic_out]).ok()?;
                for &v in mic_out.iter() {
                    out.push(f32_to_i16(v));
                }
            }
            Some(out)
        }));

        match result {
            Ok(Some(out)) => out,
            _ => {
                // Error or panic: poison and pass through from here on.
                self.poisoned = true;
                mic.to_vec()
            }
        }
    }
}

#[inline]
fn i16_to_f32(v: i16) -> f32 {
    v as f32 / i16::MAX as f32
}

#[inline]
fn f32_to_i16(v: f32) -> i16 {
    (v.clamp(-1.0, 1.0) * i16::MAX as f32).round() as i16
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ring_buffer_maps_timestamp_to_samples() {
        let mut rb = ReferenceRingBuffer::default();
        // 1600 samples (100 ms) starting at t=1000ms, ascending values.
        let chunk: Vec<i16> = (0..1600).map(|i| (i % 100) as i16).collect();
        rb.push(&chunk, 1000);
        // Reading exactly at the base returns the front samples.
        let got = rb.read_aligned(1000, 160);
        assert_eq!(got[0], 0);
        assert_eq!(got[1], 1);
        // 10 ms in = 160 samples in.
        let got = rb.read_aligned(1010, 160);
        assert_eq!(got[0], 160 % 100);
    }

    #[test]
    fn ring_buffer_zero_fills_uncovered_regions() {
        let rb = ReferenceRingBuffer::default();
        // Never pushed: everything is zero-filled and coverage is false.
        assert_eq!(rb.read_aligned(5000, 160), vec![0i16; 160]);
        assert!(!rb.has_coverage(5000, 160));

        let mut rb = ReferenceRingBuffer::default();
        rb.push(&vec![7i16; 160], 2000);
        // A target BEFORE the retained window zero-fills.
        assert_eq!(rb.read_aligned(1000, 160), vec![0i16; 160]);
    }

    #[test]
    fn ring_buffer_trims_and_advances_base() {
        let mut rb = ReferenceRingBuffer::default();
        // Push more than the retention window; base_ms must advance.
        let big: Vec<i16> = vec![1i16; REFERENCE_RETAIN_SAMPLES + 16_000];
        rb.push(&big, 0);
        assert!(rb.samples.len() <= REFERENCE_RETAIN_SAMPLES);
        // 16000 samples dropped = 1000 ms advanced.
        assert_eq!(rb.base_ms, 1000);
    }

    #[test]
    fn mic_aec_passthrough_on_length_mismatch() {
        let mut aec = MicAec::new();
        let mic = vec![5i16; 1600];
        // Reference wrong length → unchanged mic.
        assert_eq!(aec.process(&mic, &[0i16; 800]), mic);
    }

    #[test]
    fn mic_aec_is_length_preserving_and_never_panics() {
        // Contract that actually matters for the capture task: whatever we feed
        // it, process() returns exactly the input length and NEVER panics — even
        // on signals that trip an edge case inside the (young) sonora library,
        // which must degrade to pass-through, not crash the mic pipeline.
        let mut aec = MicAec::new();
        let reference: Vec<i16> = (0..1600).map(|i| ((i * 37 % 6000) as i16) - 3000).collect();
        let mic = reference.clone();
        for _ in 0..50 {
            let out = aec.process(&mic, &reference);
            assert_eq!(out.len(), 1600, "AEC must preserve the 100ms chunk length");
        }
    }

    #[test]
    fn mic_aec_passes_through_once_poisoned() {
        let mut aec = MicAec::new();
        aec.poisoned = true;
        let mic = vec![123i16; 1600];
        assert_eq!(aec.process(&mic, &vec![0i16; 1600]), mic);
    }
}
