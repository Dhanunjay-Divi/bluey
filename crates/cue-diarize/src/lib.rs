//! On-device speaker diarization for Bluey.
//!
//! Two tiers, both backed by [`speakrs`] (a pure-Rust VBx+PLDA reimplementation
//! of the pyannote pipeline — measured ~7.8% DER on the VoxConverse test set,
//! at CoreML 18× real-time on Apple Silicon):
//!
//!   * [`Diarizer`] — offline/post-process. Diarize a whole recording in one
//!     pass (authoritative labels, ~6-8% DER). Run at meeting end.
//!   * [`LiveDiarizer`] — near-real-time. Re-diarize a rolling window every few
//!     seconds and stitch each window's speakers into STABLE global ids via
//!     nearest-centroid inheritance (measured ~11% DER at constant ~3s latency).
//!     speakrs itself renumbers speakers per run, so inheritance is what keeps
//!     "Speaker 2" the same person across windows.
//!
//! Both take 16 kHz mono f32 samples (the daemon's capture format). Speaker
//! identity is a per-meeting integer id, orthogonal to the coarse mic-vs-system
//! `Speaker` channel tag.

use anyhow::{Context, Result};
use speakrs::{ExecutionMode, OwnedDiarizationPipeline};

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

/// Live diarizer — re-diarize a rolling window each tick and keep stable ids.
///
/// The daemon calls [`LiveDiarizer::push_window`] every ~N seconds with the most
/// recent ~120s of audio and the window's absolute start time. Each call runs
/// speakrs on the window (fast on CoreML) and maps the window's raw speakers to
/// STABLE global ids by matching each window-speaker's centroid against the
/// running set of known speaker centroids (cosine ≥ `match_threshold` → inherit
/// that id; else mint a new one). Matched centroids are updated toward the new
/// observation (running mean) so a voice's prototype tracks it over the meeting.
pub struct LiveDiarizer {
    diarizer: Diarizer,
    /// Stable speaker id → running centroid (L2-normalized).
    known: Vec<(i64, Vec<f32>)>,
    next_id: i64,
    match_threshold: f32,
}

impl LiveDiarizer {
    pub fn load(backend: Backend) -> Result<Self> {
        Ok(Self {
            diarizer: Diarizer::load(backend)?,
            known: Vec::new(),
            next_id: 0,
            match_threshold: 0.70, // tuned on VoxConverse (see crate docs / probe)
        })
    }

    /// Diarize one rolling window; returns segments with STABLE global speaker
    /// ids and absolute times (`window_start_secs` is added to each segment).
    pub fn push_window(&mut self, audio: &[f32], window_start_secs: f64) -> Result<Vec<Segment>> {
        let out = self
            .diarizer
            .diarize_with_centroids(audio)
            .context("live window diarize")?;

        // Build raw-speaker → centroid map for THIS window.
        let mut raw_centroid: std::collections::HashMap<i64, Vec<f32>> =
            out.centroids.into_iter().collect();

        // Greedy best-match assignment: for every (raw, known) pair sorted by
        // similarity, inherit the known id if above threshold and neither side is
        // already used this window.
        let mut mapping: std::collections::HashMap<i64, i64> = std::collections::HashMap::new();
        let mut used_known: std::collections::HashSet<i64> = std::collections::HashSet::new();
        let mut pairs: Vec<(f32, i64, i64)> = Vec::new();
        for (raw, rc) in &raw_centroid {
            for (kid, kc) in &self.known {
                pairs.push((cosine(rc, kc), *raw, *kid));
            }
        }
        pairs.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        for (sim, raw, kid) in pairs {
            if mapping.contains_key(&raw) || used_known.contains(&kid) {
                continue;
            }
            if sim >= self.match_threshold {
                mapping.insert(raw, kid);
                used_known.insert(kid);
            }
        }
        // Unmatched raw speakers → new stable ids; matched → update centroid.
        for (raw, rc) in raw_centroid.drain() {
            match mapping.get(&raw) {
                Some(&kid) => {
                    if let Some(slot) = self.known.iter_mut().find(|(id, _)| *id == kid) {
                        slot.1 = running_mean(&slot.1, &rc);
                    }
                }
                None => {
                    let id = self.next_id;
                    self.next_id += 1;
                    self.known.push((id, rc));
                    mapping.insert(raw, id);
                }
            }
        }

        Ok(out
            .segments
            .into_iter()
            .map(|s| Segment {
                start: s.start + window_start_secs,
                end: s.end + window_start_secs,
                speaker: *mapping.get(&s.speaker).unwrap_or(&s.speaker),
            })
            .collect())
    }

    /// Number of distinct stable speakers seen so far.
    pub fn speaker_count(&self) -> usize {
        self.known.len()
    }
}

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

fn cosine(a: &[f32], b: &[f32]) -> f32 {
    // Both L2-normalized → dot product is cosine similarity.
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

fn running_mean(old: &[f32], new: &[f32]) -> Vec<f32> {
    let avg: Vec<f32> = old.iter().zip(new).map(|(o, n)| (o + n) / 2.0).collect();
    l2_normalize(&avg)
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
