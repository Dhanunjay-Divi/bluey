//! Meeting audio retention for speaker diarization.
//!
//! The STT path streams 20ms chunks → text and discards the audio. Diarization
//! (both the live sliding-window tier and the post-meeting pass) needs the raw
//! audio, so we tee 16 kHz mono f32 samples into this buffer as they arrive.
//!
//! Two views over the same appended stream:
//!   * FULL — every sample since capture start, for the authoritative post pass.
//!   * ROLLING — the most recent `rolling_secs` seconds, for the live tier
//!     (bounded so memory stays flat over a long meeting; the full buffer is the
//!     memory cost we accept for an accurate end-of-meeting pass).
//!
//! Gated behind the `diarize` feature — this module is only compiled when
//! diarization is enabled, so the default build retains no audio.

use std::collections::VecDeque;

const SAMPLE_RATE: usize = 16_000;

/// A growing full buffer + a bounded rolling window of 16 kHz mono f32 audio.
pub struct AudioRetention {
    /// Every sample since start (for the post-meeting diarization pass).
    full: Vec<f32>,
    /// The most recent `rolling_cap` samples (for the live tier).
    rolling: VecDeque<f32>,
    rolling_cap: usize,
    /// Total samples appended (== full.len(); also the absolute stream position).
    total: usize,
}

impl AudioRetention {
    /// `rolling_secs` = live-window length to keep (e.g. 120s).
    pub fn new(rolling_secs: usize) -> Self {
        let rolling_cap = rolling_secs * SAMPLE_RATE;
        Self {
            full: Vec::new(),
            rolling: VecDeque::with_capacity(rolling_cap),
            rolling_cap,
            total: 0,
        }
    }

    /// Append one chunk of 16 kHz mono f32 samples.
    pub fn push(&mut self, samples: &[f32]) {
        self.full.extend_from_slice(samples);
        for &s in samples {
            if self.rolling.len() == self.rolling_cap {
                self.rolling.pop_front();
            }
            self.rolling.push_back(s);
        }
        self.total += samples.len();
    }

    /// The full recording so far (post-pass input).
    pub fn full(&self) -> &[f32] {
        &self.full
    }

    /// A copy of the recent rolling window (live-tier input), plus the absolute
    /// start time (seconds) of that window in the full stream — so the live
    /// diarizer can convert window-relative times to meeting-absolute times.
    pub fn rolling_window(&self) -> (Vec<f32>, f64) {
        let win: Vec<f32> = self.rolling.iter().copied().collect();
        let start_sample = self.total.saturating_sub(win.len());
        (win, start_sample as f64 / SAMPLE_RATE as f64)
    }

    /// Total seconds of audio retained so far.
    pub fn duration_secs(&self) -> f64 {
        self.total as f64 / SAMPLE_RATE as f64
    }

    /// Drop the full buffer (call after the post pass consumes it) to free RAM.
    pub fn clear(&mut self) {
        self.full = Vec::new();
        self.rolling.clear();
        self.total = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_and_rolling_track_appends() {
        let mut r = AudioRetention::new(1); // 1s = 16000 samples rolling cap
        r.push(&[0.1; 8000]);
        r.push(&[0.2; 8000]);
        assert_eq!(r.full().len(), 16000);
        let (win, start) = r.rolling_window();
        assert_eq!(win.len(), 16000);
        assert_eq!(start, 0.0);
        // exceed rolling cap → oldest dropped, full keeps growing
        r.push(&[0.3; 8000]);
        assert_eq!(r.full().len(), 24000);
        let (win, start) = r.rolling_window();
        assert_eq!(win.len(), 16000); // still capped at 1s
        assert!((start - 0.5).abs() < 1e-6); // window now starts at 0.5s
    }
}
