//! On-device speaker diarization for Bluey.
//!
//! Two tiers, both backed by [`speakrs`] (a pure-Rust VBx+PLDA reimplementation
//! of the pyannote pipeline — measured ~7.8% DER on the VoxConverse test set,
//! at CoreML 18× real-time on Apple Silicon):
//!
//!   * [`Diarizer`] — offline/post-process. Diarize a whole recording in one
//!     pass (authoritative labels, ~6-8% DER). Run at meeting end.
//!   * [`BankLiveDiarizer`] — near-real-time, profile-bank. The live tier the
//!     daemon uses: carries each speaker forward as ONE EMA-updated embedding
//!     centroid and re-identifies by cosine match each tick. Measured 9.1% DER
//!     on 240s trims / 6.9% on 20-min files — beating the earlier anchor-pinned
//!     and window-stitcher tiers on accuracy, speaker counting, AND cost, and
//!     constant in meeting length (see docs/work/STT-DIARIZATION-FINDINGS.md).
//!
//! Both take 16 kHz mono f32 samples (the daemon's capture format). Speaker
//! identity is a per-meeting integer id, orthogonal to the coarse mic-vs-system
//! `Speaker` channel tag.

use anyhow::Result;
use speakrs::{ExecutionMode, OwnedDiarizationPipeline};

mod bank;
mod overlap;
pub use bank::{BankLiveDiarizer, BANK_WINDOW_SECS};

/// One diarized speech span with a per-meeting speaker id.
#[derive(Debug, Clone)]
pub struct Segment {
    /// Seconds from the start of the audio fed to the diarizer.
    pub start: f64,
    /// End time in seconds.
    pub end: f64,
    /// Stable per-meeting speaker id (0-based). For [`LiveDiarizer`] this id is
    /// consistent across windows; for [`Diarizer`] it is arrival-time ordered.
    pub speaker: i64,
}

/// Which compute backend speakrs uses. CoreML (Apple Silicon) is ~18× real-time;
/// Cpu works everywhere but is ~real-time (backend-pass territory on weak boxes).
#[derive(Debug, Clone, Copy)]
pub enum Backend {
    CoreMl,
    Cpu,
}

impl Backend {
    fn to_speakrs(self) -> ExecutionMode {
        match self {
            Backend::CoreMl => ExecutionMode::CoreMl,
            Backend::Cpu => ExecutionMode::Cpu,
        }
    }
    /// Default: CoreML on macOS, CPU elsewhere.
    pub fn preferred() -> Self {
        if cfg!(target_os = "macos") {
            Backend::CoreMl
        } else {
            Backend::Cpu
        }
    }
}

/// Offline diarizer — one pass over a whole recording. Authoritative labels.
pub struct Diarizer {
    pipeline: OwnedDiarizationPipeline,
}

impl Diarizer {
    /// Load the speakrs pipeline. With speakrs's `online` feature its models are
    /// fetched on first use (or resolved from `SPEAKRS_MODELS_DIR`).
    pub fn load(backend: Backend) -> Result<Self> {
        let pipeline = OwnedDiarizationPipeline::from_pretrained(backend.to_speakrs())
            .map_err(|e| anyhow::anyhow!("speakrs pipeline load failed: {e:?}"))?;
        Ok(Self { pipeline })
    }

    /// Diarize a full 16 kHz mono f32 recording → arrival-time-ordered speakers.
    pub fn diarize(&mut self, audio: &[f32]) -> Result<Vec<Segment>> {
        let out = self
            .pipeline
            .run(audio)
            .map_err(|e| anyhow::anyhow!("speakrs run failed: {e:?}"))?;
        Ok(segments_from_result(&out))
    }

    /// Diarize + return per-speaker centroids (for the DB / cross-meeting gallery).
    pub fn diarize_with_centroids(&mut self, audio: &[f32]) -> Result<DiarizeOutput> {
        let out = self
            .pipeline
            .run(audio)
            .map_err(|e| anyhow::anyhow!("speakrs run failed: {e:?}"))?;
        let segments = segments_from_result(&out);
        let centroids = centroids_from_result(&out);
        Ok(DiarizeOutput {
            segments,
            centroids,
        })
    }
}

/// Segments + the per-speaker mean embedding (L2-normalized) that produced them.
pub struct DiarizeOutput {
    pub segments: Vec<Segment>,
    /// speaker id → centroid embedding.
    pub centroids: Vec<(i64, Vec<f32>)>,
}

/// Sentinel speaker id for a segment the live tier declined to label this window
/// (a new voice that hasn't spoken long enough to enroll). Downstream
/// (`assign_speaker` in the daemon) treats it as no-overlap → the segment waits
/// for the next tick rather than being forced into the wrong id.
pub const UNLABELED: i64 = -1;

// ---- helpers ----

fn segments_from_result(out: &speakrs::DiarizationResult) -> Vec<Segment> {
    // speakrs `Segment { start, end, speaker: "SPEAKER_NN" }`. Parse the integer.
    out.segments
        .iter()
        .map(|s| Segment {
            start: s.start,
            end: s.end,
            speaker: parse_speaker(&s.speaker),
        })
        .collect()
}

fn centroids_from_result(out: &speakrs::DiarizationResult) -> Vec<(i64, Vec<f32>)> {
    // embeddings: (chunks, speakers, dim); hard_clusters: (chunks, speakers).
    let emb = &out.embeddings.0;
    let clusters = &out.hard_clusters.0;
    let (n_chunks, n_spk, dim) = emb.dim();
    use std::collections::HashMap;
    let mut sums: HashMap<i64, (Vec<f64>, usize)> = HashMap::new();
    for c in 0..n_chunks {
        for s in 0..n_spk {
            let cid = clusters[[c, s]] as i64;
            if cid < 0 {
                continue;
            }
            let e = sums.entry(cid).or_insert_with(|| (vec![0.0; dim], 0));
            for d in 0..dim {
                let v = emb[[c, s, d]];
                if v.is_finite() {
                    e.0[d] += v as f64;
                }
            }
            e.1 += 1;
        }
    }
    let mut out_v = Vec::new();
    for (id, (sum, n)) in sums {
        if n == 0 {
            continue;
        }
        let mean: Vec<f32> = sum.iter().map(|x| (*x / n as f64) as f32).collect();
        out_v.push((id, l2_normalize(&mean)));
    }
    out_v.sort_by_key(|(id, _)| *id);
    out_v
}

fn parse_speaker(label: &str) -> i64 {
    label
        .rsplit(|c: char| !c.is_ascii_digit())
        .find(|s| !s.is_empty())
        .and_then(|s| s.parse().ok())
        .unwrap_or(-1)
}

fn l2_normalize(v: &[f32]) -> Vec<f32> {
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt().max(1e-9);
    v.iter().map(|x| x / norm).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cosine(a: &[f32], b: &[f32]) -> f32 {
        a.iter().zip(b).map(|(x, y)| x * y).sum()
    }

    #[test]
    fn parse_speaker_labels() {
        assert_eq!(parse_speaker("SPEAKER_00"), 0);
        assert_eq!(parse_speaker("SPEAKER_13"), 13);
        assert_eq!(parse_speaker("S7"), 7);
    }

    #[test]
    fn cosine_of_identical_is_one() {
        let v = l2_normalize(&[1.0, 2.0, 3.0]);
        assert!((cosine(&v, &v) - 1.0).abs() < 1e-5);
    }

    // Time-overlap mapping is the core of the stable live tier. Validate the pure
    // overlap math directly (the behavior on real audio is checked in dev against
    // VoxConverse). Two prior global segments; a new window's raw speakers map to
    // the global id they overlap most.
    #[test]
    fn overlap_maps_raw_to_max_overlapping_global() {
        // prev: global 0 over [0,10], global 1 over [10,20].
        // raw A over [1,9] → overlaps 0 by 8, 1 by 0 → global 0.
        // raw B over [11,19] → overlaps 1 by 8 → global 1.
        let ov = |s1: f64, e1: f64, s2: f64, e2: f64| (e1.min(e2) - s1.max(s2)).max(0.0);
        assert_eq!(ov(1.0, 9.0, 0.0, 10.0), 8.0);
        assert_eq!(ov(1.0, 9.0, 10.0, 20.0), 0.0);
        assert_eq!(ov(11.0, 19.0, 10.0, 20.0), 8.0);
    }
}
