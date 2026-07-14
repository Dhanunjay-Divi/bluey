//! Anchor-pinned live diarization — the measured replacement for the
//! time-overlap window stitcher.
//!
//! The idea: the diarizer cannot compare voices ACROSS runs (speakrs per-run
//! centroids are incomparable), so carry each known speaker's voice forward AS
//! AUDIO. Keep a gallery of one ~8s "anchor" clip per enrolled speaker; every
//! tick, diarize `[anchor₀ + gap + anchor₁ + … + recent window]` as ONE clip.
//! The cluster containing anchor_i's time-span IS speaker i — identity pinned by
//! construction. A window cluster overlapping no anchor is a genuinely new voice
//! → mint the next id and cut its anchor from the window.
//!
//! Measured on VoxConverse (15 files, 1–11 speakers, vs RTTM ground truth — see
//! docs/work/STT-DIARIZATION-FINDINGS.md):
//!   * this design (live ticks):   12.3% DER, best-in-test speaker counting
//!   * old window stitcher (live): 26.6% DER (47.9% on full files; errors compound)
//!   * plain offline pass:          8.7% DER
//!
//! Cost per tick is CONSTANT (gallery + window, ~8s compute at CoreML 18×) no
//! matter how long the meeting runs — unlike re-diarizing the growing full
//! buffer, which saturates the worker ~15 min in.
//!
//! Tuning that matters (measured — do not shrink): 8s pins + a ~90s window.
//! Shorter pins/windows make the clusterer MERGE voices (anchors pin identity
//! but cannot force splits — the clusterer needs context).

use std::collections::{BTreeSet, HashMap};

use anyhow::{Context, Result};

use crate::{Backend, Diarizer, Segment};

const SAMPLE_RATE: usize = 16_000;

/// Window the live tier should feed per tick (seconds). Also the retention
/// rolling-buffer length the daemon keeps for it.
pub const ANCHOR_WINDOW_SECS: usize = 90;

/// Anchor clip length (seconds). 8s is load-bearing: 4-5s pins measurably
/// under-split (merged voices, 20-36% DER on files 8s pins handle at ~5%).
const PIN_SECS: f64 = 8.0;
/// Silence spliced between gallery clips / before the window, so the clusterer
/// segments them apart.
const GAP_SECS: f64 = 0.75;
/// Min window speech before an unmatched cluster is minted as a NEW speaker.
const ENROLL_MIN_SECS: f64 = 3.0;
/// Min overlap with an anchor span for a cluster to claim that identity.
const CLAIM_MIN_SECS: f64 = 1.0;

/// One enrolled speaker: the stable id and the anchor audio that pins it.
struct Anchor {
    gid: i64,
    clip: Vec<f32>,
}

/// Live diarizer with anchor-pinned stable ids. Feed the most recent
/// [`ANCHOR_WINDOW_SECS`] of 16 kHz mono audio each tick via [`push_window`];
/// segments come back at ABSOLUTE meeting times with per-meeting stable speaker
/// ids (0-based, arrival-ordered), same contract as the old `LiveDiarizer`.
///
/// [`push_window`]: AnchorLiveDiarizer::push_window
pub struct AnchorLiveDiarizer {
    diarizer: Diarizer,
    gallery: Vec<Anchor>,
    next_id: i64,
}

impl AnchorLiveDiarizer {
    pub fn load(backend: Backend) -> Result<Self> {
        Ok(Self {
            diarizer: Diarizer::load(backend)?,
            gallery: Vec::new(),
            next_id: 0,
        })
    }

    /// Speakers enrolled so far.
    pub fn speaker_count(&self) -> usize {
        self.gallery.len()
    }

    /// Diarize one window and return its segments labeled with STABLE gallery
    /// ids at absolute times. `window_start_secs` is the absolute stream time of
    /// `window[0]` (the daemon's retention supplies both).
    pub fn push_window(&mut self, window: &[f32], window_start_secs: f64) -> Result<Vec<Segment>> {
        // Too little audio to say anything useful (also guards the mint path).
        if window.len() < SAMPLE_RATE * 2 {
            return Ok(Vec::new());
        }

        // Build [anchor₀ + gap + anchor₁ + gap + … + window]; record anchor spans.
        let gap = vec![0.0f32; (GAP_SECS * SAMPLE_RATE as f64) as usize];
        let mut concat: Vec<f32> = Vec::with_capacity(
            window.len()
                + self
                    .gallery
                    .iter()
                    .map(|a| a.clip.len() + gap.len())
                    .sum::<usize>(),
        );
        let mut spans: Vec<(i64, f64, f64)> = Vec::new();
        for a in &self.gallery {
            let s = concat.len() as f64 / SAMPLE_RATE as f64;
            concat.extend_from_slice(&a.clip);
            spans.push((a.gid, s, concat.len() as f64 / SAMPLE_RATE as f64));
            concat.extend_from_slice(&gap);
        }
        let win_off = concat.len() as f64 / SAMPLE_RATE as f64;
        concat.extend_from_slice(window);

        let raw = self
            .diarizer
            .diarize(&concat)
            .context("anchor-pinned window diarize")?;

        // Pin identities: raw cluster → gallery id by anchor-span overlap.
        let mut map = map_raw_to_gallery(&raw, &spans, CLAIM_MIN_SECS);

        // Window speech + longest window segment per raw cluster (for minting).
        let mut win_dur: HashMap<i64, f64> = HashMap::new();
        let mut longest: HashMap<i64, (f64, f64)> = HashMap::new();
        for s in &raw {
            let w = (s.end - s.start.max(win_off)).max(0.0);
            if w <= 0.0 {
                continue;
            }
            *win_dur.entry(s.speaker).or_insert(0.0) += w;
            let cur = longest.get(&s.speaker).map(|(a, b)| b - a).unwrap_or(0.0);
            if s.end - s.start.max(win_off) > cur {
                longest.insert(s.speaker, (s.start.max(win_off), s.end));
            }
        }

        // Mint genuinely-new voices: enough window speech, no anchor claimed.
        // Deterministic order (raw id) so enrollment order is stable.
        let mut fresh: Vec<i64> = win_dur
            .iter()
            .filter(|(raw_id, d)| !map.contains_key(raw_id) && **d >= ENROLL_MIN_SECS)
            .map(|(raw_id, _)| *raw_id)
            .collect();
        fresh.sort_unstable();
        for raw_id in fresh {
            let gid = self.next_id;
            self.next_id += 1;
            if let Some((cs, ce)) = longest.get(&raw_id) {
                // Cut the pin from the window audio (window-relative samples).
                let rel_s = cs - win_off;
                let rel_e =
                    (rel_s + (ce - cs).min(PIN_SECS)).min(window.len() as f64 / SAMPLE_RATE as f64);
                let ai = (rel_s * SAMPLE_RATE as f64) as usize;
                let bi = ((rel_e * SAMPLE_RATE as f64) as usize).min(window.len());
                if bi > ai {
                    self.gallery.push(Anchor {
                        gid,
                        clip: window[ai..bi].to_vec(),
                    });
                }
            }
            map.insert(raw_id, gid);
        }

        // Emit the window's segments at absolute times with stable ids. Clusters
        // that neither claimed an anchor nor enrolled stay unlabeled (dropped —
        // the daemon's assign step treats absence as "wait for the next tick").
        let mut out = Vec::new();
        for s in &raw {
            if s.end <= win_off {
                continue; // anchor-region segment, not meeting audio
            }
            let Some(&gid) = map.get(&s.speaker) else {
                continue;
            };
            let start = window_start_secs + (s.start.max(win_off) - win_off);
            let end = window_start_secs + (s.end - win_off);
            if end - start > 0.05 {
                out.push(Segment {
                    start,
                    end,
                    speaker: gid,
                });
            }
        }
        Ok(out)
    }
}

/// Pin identities: map each raw cluster to the gallery id whose anchor span it
/// overlaps most (greedy, 1:1, requiring ≥ `min_claim` seconds of overlap).
/// Pure — unit-tested without models.
fn map_raw_to_gallery(
    segs: &[Segment],
    anchor_spans: &[(i64, f64, f64)],
    min_claim: f64,
) -> HashMap<i64, i64> {
    let mut overlap: HashMap<(i64, i64), f64> = HashMap::new();
    for s in segs {
        for (gid, a, b) in anchor_spans {
            let o = (s.end.min(*b) - s.start.max(*a)).max(0.0);
            if o > 0.0 {
                *overlap.entry((s.speaker, *gid)).or_insert(0.0) += o;
            }
        }
    }
    let mut pairs: Vec<((i64, i64), f64)> = overlap.into_iter().collect();
    pairs.sort_by(|x, y| y.1.partial_cmp(&x.1).unwrap_or(std::cmp::Ordering::Equal));
    let mut map: HashMap<i64, i64> = HashMap::new();
    let mut claimed: BTreeSet<i64> = BTreeSet::new();
    for ((raw, gid), o) in pairs {
        if o < min_claim || map.contains_key(&raw) || claimed.contains(&gid) {
            continue;
        }
        map.insert(raw, gid);
        claimed.insert(gid);
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(start: f64, end: f64, speaker: i64) -> Segment {
        Segment {
            start,
            end,
            speaker,
        }
    }

    #[test]
    fn maps_each_cluster_to_its_dominant_anchor() {
        // Anchors: gid 7 at [0,8], gid 9 at [8.75,16.75]. Cluster 0 covers
        // anchor 7; cluster 1 covers anchor 9; both also talk in the window.
        let spans = vec![(7, 0.0, 8.0), (9, 8.75, 16.75)];
        let segs = vec![
            seg(0.2, 7.8, 0),
            seg(9.0, 16.0, 1),
            seg(20.0, 25.0, 0),
            seg(26.0, 30.0, 1),
        ];
        let map = map_raw_to_gallery(&segs, &spans, 1.0);
        assert_eq!(map.get(&0), Some(&7));
        assert_eq!(map.get(&1), Some(&9));
    }

    #[test]
    fn one_to_one_greedy_prefers_max_overlap() {
        // One cluster overlaps BOTH anchors (a merged pass): it claims the anchor
        // it overlaps MORE; the other anchor stays unclaimed (never double-claimed).
        let spans = vec![(0, 0.0, 8.0), (1, 8.75, 16.75)];
        let segs = vec![seg(0.0, 12.0, 5)]; // 8s on anchor0, 3.25s on anchor1
        let map = map_raw_to_gallery(&segs, &spans, 1.0);
        assert_eq!(map.get(&5), Some(&0));
        assert_eq!(map.len(), 1);
    }

    #[test]
    fn sub_second_grazes_do_not_claim() {
        let spans = vec![(0, 0.0, 8.0)];
        let segs = vec![seg(7.5, 12.0, 3)]; // only 0.5s on the anchor
        let map = map_raw_to_gallery(&segs, &spans, 1.0);
        assert!(map.is_empty());
    }

    #[test]
    fn two_clusters_cannot_claim_one_anchor() {
        // Both clusters overlap the same anchor: strongest wins, other unmapped.
        let spans = vec![(4, 0.0, 8.0)];
        let segs = vec![seg(0.0, 6.0, 0), seg(6.0, 8.0, 1)];
        let map = map_raw_to_gallery(&segs, &spans, 1.0);
        assert_eq!(map.get(&0), Some(&4));
        assert_eq!(map.get(&1), None);
    }
}
