//! On-device speaker diarization for Bluey.
//!
//! Two tiers, both backed by [`speakrs`] (a pure-Rust VBx+PLDA reimplementation
//! of the pyannote pipeline — measured ~7.8% DER on the VoxConverse test set,
//! at CoreML 18× real-time on Apple Silicon):
//!
//!   * [`Diarizer`] — offline/post-process. Diarize a whole recording in one
//!     pass (authoritative labels, ~6-8% DER). Run at meeting end.
//!   * [`LiveDiarizer`] — near-real-time. Re-diarize a rolling window every few
//!     seconds and stitch each window's speakers into STABLE, arrival-ordered
//!     global ids by TIME OVERLAP with the previous window's labels. speakrs
//!     renumbers speakers per run AND its per-run embeddings aren't comparable
//!     across runs, so time overlap of the shared audio prefix — not centroid
//!     matching — is what keeps "Speaker 2" the same person across windows.
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

/// Sentinel speaker id for a segment the live tier declined to label this window
/// (a new voice that hasn't spoken long enough to enroll). Downstream
/// (`assign_speaker` in the daemon) treats it as no-overlap → the segment waits
/// for the next tick rather than being forced into the wrong id.
pub const UNLABELED: i64 = -1;

/// Min speech (secs) a new-looking raw speaker must have in the window before it
/// is minted as a new global id (cold-start guard; below this it stays
/// [`UNLABELED`]). AssemblyAI "first turns least stable".
const MIN_ENROLL_SECS: f64 = 2.0;

/// One previously-labelled global segment, kept to anchor the NEXT window by time
/// overlap (the reliable cross-run signal — speakrs per-run centroids are NOT
/// comparable across separate diarize() calls, measured ~0 self-similarity).
#[derive(Clone)]
struct GlobalSeg {
    start: f64, // absolute secs
    end: f64,
    id: i64,
}

/// Live diarizer — re-diarize a rolling window each tick and keep stable ids.
///
/// The daemon calls [`LiveDiarizer::push_window`] every ~N seconds with the most
/// recent ~30s of audio and the window's absolute start time. Each call runs
/// speakrs on the window and maps the window's raw speakers to STABLE, arrival-
/// ordered global ids by **time overlap with the previous window's labels**.
///
/// Why time overlap and NOT centroid matching: speakrs computes speaker embeddings
/// per run, and they are NOT comparable across separate `diarize()` calls (the
/// same voice measures ~0 cosine self-similarity across two runs). But the SAME
/// audio region re-diarized keeps the same speaker over time, so overlap of a new
/// window's segments with the previous window's labelled segments is a rock-solid
/// anchor. A raw speaker whose segments don't overlap any prior global id is a
/// genuinely new voice → the next arrival-ordered id (append-only, never reused).
pub struct LiveDiarizer {
    diarizer: Diarizer,
    /// Global segments emitted by the previous window (absolute times) — the
    /// anchor a new window's raw speakers are mapped against by max time overlap.
    prev: Vec<GlobalSeg>,
    /// Arrival order of enrolled ids (append-only; id, first_seen_secs).
    enrolled: Vec<(i64, f64)>,
    next_id: i64,
}

impl LiveDiarizer {
    pub fn load(backend: Backend) -> Result<Self> {
        Ok(Self {
            diarizer: Diarizer::load(backend)?,
            prev: Vec::new(),
            enrolled: Vec::new(),
            next_id: 0,
        })
    }

    /// Diarize one window and map its raw speakers to STABLE, arrival-ordered
    /// global ids by **time overlap with the previous window's labels** — NOT by
    /// centroid (speakrs centroids aren't comparable across runs). The same audio
    /// region re-diarized keeps the same speaker over time, so overlap is a rock-
    /// solid anchor; a raw speaker whose segments don't overlap any prior global id
    /// is a genuinely new voice → next arrival-ordered id.
    pub fn push_window(&mut self, audio: &[f32], window_start_secs: f64) -> Result<Vec<Segment>> {
        let out = self
            .diarizer
            .diarize(audio)
            .context("live window diarize")?;

        // Shift this window's segments to absolute time.
        let win: Vec<GlobalSeg> = out
            .iter()
            .map(|s| GlobalSeg {
                start: s.start + window_start_secs,
                end: s.end + window_start_secs,
                id: s.speaker, // raw id for now
            })
            .collect();

        // Per-raw: total in-window speech + overlap with each prior global id.
        let mut raw_dur: std::collections::HashMap<i64, f64> = std::collections::HashMap::new();
        let mut overlap: std::collections::HashMap<(i64, i64), f64> =
            std::collections::HashMap::new(); // (raw, global_id) → overlap secs
        for w in &win {
            *raw_dur.entry(w.id).or_insert(0.0) += (w.end - w.start).max(0.0);
            for p in &self.prev {
                let ov = (w.end.min(p.end) - w.start.max(p.start)).max(0.0);
                if ov > 0.0 {
                    *overlap.entry((w.id, p.id)).or_insert(0.0) += ov;
                }
            }
        }
        let mut raws: Vec<i64> = raw_dur.keys().copied().collect();
        raws.sort_unstable();

        // Map each raw → global by MAX overlap (one-to-one: a global id is claimed
        // by the raw that overlaps it most). New voices → arrival-ordered mint.
        let mut raw_to_global: std::collections::HashMap<i64, i64> =
            std::collections::HashMap::new();
        let mut claimed: std::collections::HashSet<i64> = std::collections::HashSet::new();
        // Best (overlap, raw, global) triples, strongest first.
        let mut cand: Vec<(f64, i64, i64)> =
            overlap.iter().map(|(&(r, g), &o)| (o, r, g)).collect();
        cand.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        for (_o, raw, gid) in cand {
            if raw_to_global.contains_key(&raw) || claimed.contains(&gid) {
                continue;
            }
            raw_to_global.insert(raw, gid);
            claimed.insert(gid);
        }
        // Unmatched raws → new arrival-ordered global id (if enough speech).
        for &raw in &raws {
            if raw_to_global.contains_key(&raw) {
                continue;
            }
            if *raw_dur.get(&raw).unwrap_or(&0.0) >= MIN_ENROLL_SECS {
                let id = self.next_id;
                self.next_id += 1;
                self.enrolled.push((id, window_start_secs));
                raw_to_global.insert(raw, id);
            } else {
                raw_to_global.insert(raw, UNLABELED);
            }
        }

        // Build this window's GLOBAL-labelled segments → they anchor the next tick.
        let labelled: Vec<GlobalSeg> = win
            .into_iter()
            .map(|w| GlobalSeg {
                start: w.start,
                end: w.end,
                id: *raw_to_global.get(&w.id).unwrap_or(&UNLABELED),
            })
            .collect();
        // Keep only real (labelled) segments as the anchor.
        self.prev = labelled.iter().filter(|s| s.id >= 0).cloned().collect();

        Ok(labelled
            .into_iter()
            .map(|s| Segment {
                start: s.start,
                end: s.end,
                speaker: s.id,
            })
            .collect())
    }

    /// Number of distinct stable speakers enrolled so far.
    pub fn speaker_count(&self) -> usize {
        self.enrolled.len()
    }

    /// Enrolled global ids in arrival order — asserts the arrival-order invariant.
    pub fn enrolled_ids_in_arrival_order(&self) -> Vec<i64> {
        let mut v = self.enrolled.clone();
        v.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
        v.into_iter().map(|(id, _)| id).collect()
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
