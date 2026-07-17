//! Profile-bank live diarization — the measured replacement for anchor pinning.
//!
//! speakrs cannot compare voices ACROSS separate `diarize()` runs by cluster id
//! (per-run ids are arbitrary), but the underlying WeSpeaker EMBEDDINGS ARE
//! comparable across runs (measured cross-window centroid cosine: 0.94 at 2
//! speakers, ~0.66 at 11 — well above a 0.55 match threshold; the documented
//! "~0 self-similarity" was an id-numbering confound, not a vector property).
//!
//! So instead of carrying each speaker forward as 8s of raw audio (anchors,
//! re-embedded every tick — cost grows with the gallery), carry each speaker
//! as ONE 256-d centroid vector, EMA-updated. Every tick: diarize just the
//! recent window, take each window-cluster's mean embedding, and match it to
//! the bank by cosine. Constant memory, constant per-tick cost, no anchor
//! concatenation. This is the constant-memory recurrent-state design.
//!
//! Measured on VoxConverse vs anchor pinning (docs/work/STT-DIARIZATION-FINDINGS.md):
//!
//!   * 240s trims:  bank 9.1% DER / 10-of-15 spk exact  vs  anchors 12.3% / 8-of-15
//!   * 20-min files: bank 6.9% / better counting  vs  anchors 17.2% (anchors DEGRADE
//!     with length — the gallery accretes duplicate pins; the bank IMPROVES).
//!
//! The bank's live labels land within ~0.5% DER of the authoritative offline pass.
//!
//! `ProfileBank` is pure vector logic (std only, deterministic, no model deps);
//! `BankLiveDiarizer` wraps it with the window-diarize tick loop and exposes the
//! same `load` / `push_window` / `speaker_count` contract as `AnchorLiveDiarizer`.

use anyhow::{Context, Result};

use crate::{overlap, Backend, Diarizer, Segment};

const SAMPLE_RATE: usize = 16_000;

/// Window the live tier feeds per tick (seconds). Same 90s the anchor design
/// used — the clusterer needs context to SEPARATE voices; shorter windows merge
/// them (measured). Also the retention rolling-buffer length the daemon keeps.
pub const BANK_WINDOW_SECS: usize = 90;

/// A window cluster with more than this fraction of its speech overlapped by
/// another speaker is talk-over-contaminated: it still gets a label, but must
/// not update its centroid (poisoning the profile is how identity rots).
const OVERLAP_UNCLEAN_FRAC: f64 = 0.30;

/// Minimum window speech (secs) for an unmatched cluster to count toward
/// enrollment / be treated as a real observation at all.
const MIN_OBSERVE_SECS: f64 = 1.0;

// ---------------------------------------------------------------------------
// ProfileBank — pure identity state (lifted from the validated harness module).
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct BankConfig {
    /// Min cosine to assign an observation to an existing profile.
    pub match_threshold: f32,
    /// top1 - top2 below this ⇒ ambiguous (assigned, but no centroid update).
    pub ambig_margin: f32,
    /// Centroid EMA rate: `c = norm((1-a)*c + a*e)`.
    pub ema_alpha: f32,
    /// Cumulative candidate speech (secs) before it can mint a new profile.
    pub min_enroll_secs: f64,
    /// Observations of a candidate before it can mint (1 = immediate).
    pub mint_patience: u32,
}

impl Default for BankConfig {
    fn default() -> Self {
        Self {
            match_threshold: 0.55,
            ambig_margin: 0.10,
            ema_alpha: 0.10,
            min_enroll_secs: 3.0,
            mint_patience: 1,
        }
    }
}

#[derive(Clone, Debug)]
pub struct MatchDecision {
    /// Assigned stable id (None = unmatched, still pending — wait for next tick).
    pub gid: Option<i64>,
    // The remaining fields are the decision's diagnostics/confidence signal:
    // asserted in the unit tests and available to callers, but the daemon's
    // live tier reads only `gid` today. `margin` is the calibrated confidence
    // (measured monotone on real-length audio) reserved for a future
    // confidence-gated commit path. Kept, not pruned, so the record is honest.
    #[allow(dead_code)]
    pub top_score: f32,
    #[allow(dead_code)]
    pub margin: f32,
    #[allow(dead_code)]
    pub ambiguous: bool,
    #[allow(dead_code)]
    pub minted: bool,
    #[allow(dead_code)]
    pub updated: bool,
}

struct Profile {
    centroid: Vec<f32>,
}

struct Candidate {
    mean_sum: Vec<f32>,
    mean_count: u32,
    unclean_seed: bool,
    observations: u32,
    speech_secs: f64,
}

impl Candidate {
    fn mean(&self) -> Vec<f32> {
        let n = self.mean_count.max(1) as f32;
        let mut m: Vec<f32> = self.mean_sum.iter().map(|v| v / n).collect();
        l2_normalize(&mut m);
        m
    }
}

/// Persistent speaker identity: EMA-maintained profile centroids + pending
/// candidates. gids are minted strictly in arrival order, 0-based.
pub struct ProfileBank {
    cfg: BankConfig,
    profiles: Vec<Profile>,
    candidates: Vec<Candidate>,
}

impl ProfileBank {
    pub fn new(cfg: BankConfig) -> Self {
        Self {
            cfg,
            profiles: Vec::new(),
            candidates: Vec::new(),
        }
    }

    /// One window-cluster observation: its mean embedding, seconds of speech it
    /// had in the window, and whether it is `clean` (not overlap-contaminated).
    pub fn observe(&mut self, embedding: &[f32], speech_secs: f64, clean: bool) -> MatchDecision {
        let mut e = embedding.to_vec();
        l2_normalize(&mut e);

        // Score against every profile centroid (top1 + top2).
        let mut best: Option<(usize, f32)> = None;
        let mut second: Option<f32> = None;
        for (i, p) in self.profiles.iter().enumerate() {
            let s = dot(&e, &p.centroid);
            match best {
                None => best = Some((i, s)),
                Some((_, bs)) if s > bs => {
                    second = Some(second.map_or(bs, |x| x.max(bs)));
                    best = Some((i, s));
                }
                Some(_) => second = Some(second.map_or(s, |x| x.max(s))),
            }
        }
        let top_score = best.map(|(_, s)| s).unwrap_or(0.0);
        let margin = match second {
            Some(s2) => top_score - s2,
            None => f32::INFINITY,
        };
        let ambiguous = self.profiles.len() >= 2 && margin < self.cfg.ambig_margin;

        // Assigned to an existing profile?
        if let Some((idx, score)) = best {
            if score >= self.cfg.match_threshold {
                let updated = clean && !ambiguous && speech_secs >= 1.0;
                if updated {
                    let a = self.cfg.ema_alpha;
                    let c = &mut self.profiles[idx].centroid;
                    for (cv, ev) in c.iter_mut().zip(e.iter()) {
                        *cv = (1.0 - a) * *cv + a * *ev;
                    }
                    l2_normalize(c);
                }
                return MatchDecision {
                    gid: Some(idx as i64),
                    top_score,
                    margin,
                    ambiguous,
                    minted: false,
                    updated,
                };
            }
        }

        // Unmatched: route to pending candidates.
        let mut cand_best: Option<(usize, f32)> = None;
        for (i, c) in self.candidates.iter().enumerate() {
            let s = dot(&e, &c.mean());
            if s >= self.cfg.match_threshold && cand_best.is_none_or(|(_, bs)| s > bs) {
                cand_best = Some((i, s));
            }
        }
        let ci = match cand_best {
            Some((i, _)) => {
                let c = &mut self.candidates[i];
                c.observations += 1;
                c.speech_secs += speech_secs;
                if clean {
                    if c.unclean_seed {
                        c.mean_sum = e.clone();
                        c.mean_count = 1;
                        c.unclean_seed = false;
                    } else {
                        for (mv, ev) in c.mean_sum.iter_mut().zip(e.iter()) {
                            *mv += *ev;
                        }
                        c.mean_count += 1;
                    }
                }
                i
            }
            None => {
                self.candidates.push(Candidate {
                    mean_sum: e.clone(),
                    mean_count: 1,
                    unclean_seed: !clean,
                    observations: 1,
                    speech_secs,
                });
                self.candidates.len() - 1
            }
        };

        // Mint check for the candidate this observation landed on.
        let ready = {
            let c = &self.candidates[ci];
            c.observations >= self.cfg.mint_patience && c.speech_secs >= self.cfg.min_enroll_secs
        };
        if ready {
            let cand = self.candidates.remove(ci);
            let gid = self.profiles.len() as i64;
            self.profiles.push(Profile {
                centroid: cand.mean(),
            });
            return MatchDecision {
                gid: Some(gid),
                top_score,
                margin,
                ambiguous,
                minted: true,
                updated: false,
            };
        }

        MatchDecision {
            gid: None,
            top_score,
            margin,
            ambiguous,
            minted: false,
            updated: false,
        }
    }

    pub fn profile_count(&self) -> usize {
        self.profiles.len()
    }
}

fn l2_normalize(v: &mut [f32]) {
    let norm: f64 = v
        .iter()
        .map(|&x| (x as f64) * (x as f64))
        .sum::<f64>()
        .sqrt();
    if norm > 1e-12 {
        let inv = (1.0 / norm) as f32;
        for x in v.iter_mut() {
            *x *= inv;
        }
    }
}

fn dot(a: &[f32], b: &[f32]) -> f32 {
    a.iter()
        .zip(b.iter())
        .map(|(&x, &y)| (x as f64) * (y as f64))
        .sum::<f64>() as f32
}

// ---------------------------------------------------------------------------
// BankLiveDiarizer — the live tier: window diarize + bank match per tick.
// ---------------------------------------------------------------------------

/// One window cluster's aggregated stats for the tick loop.
struct WindowCluster {
    cid: i64,
    speech: f64,
    ovfrac: f64,
    spans: Vec<(f64, f64)>,
}

/// Live diarizer with profile-bank stable ids. Feed the most recent
/// [`BANK_WINDOW_SECS`] of 16 kHz mono audio each tick via [`push_window`];
/// segments come back at ABSOLUTE meeting times with per-meeting stable speaker
/// ids (0-based, arrival-ordered) — the SAME contract as `AnchorLiveDiarizer`,
/// so the daemon worker swap is a drop-in.
///
/// [`push_window`]: BankLiveDiarizer::push_window
pub struct BankLiveDiarizer {
    diarizer: Diarizer,
    bank: ProfileBank,
    /// Ticks processed so far — used to scale mint patience with meeting length
    /// (measured: patience 1 is right early when ticks are scarce; patience 2+
    /// suppresses transient over-count once a meeting is long — 3/3 exact spk on
    /// 20-min files vs 2/3 at patience 1).
    ticks: u32,
}

impl BankLiveDiarizer {
    pub fn load(backend: Backend) -> Result<Self> {
        Ok(Self {
            diarizer: Diarizer::load(backend)?,
            bank: ProfileBank::new(BankConfig::default()),
            ticks: 0,
        })
    }

    /// Speakers enrolled so far.
    pub fn speaker_count(&self) -> usize {
        self.bank.profile_count()
    }

    /// Diarize one window and return its segments labeled with STABLE bank ids at
    /// absolute times. `window_start_secs` is the absolute stream time of
    /// `window[0]` (the daemon's retention supplies both).
    pub fn push_window(&mut self, window: &[f32], window_start_secs: f64) -> Result<Vec<Segment>> {
        if window.len() < SAMPLE_RATE * 2 {
            return Ok(Vec::new());
        }
        self.ticks = self.ticks.saturating_add(1);
        // Patience grows with meeting length: immediate for the first stretch,
        // then require 2 observations so a one-off cluster can't over-count.
        self.bank.cfg.mint_patience = if self.ticks <= 4 { 1 } else { 2 };

        let out = self
            .diarizer
            .diarize_with_centroids(window)
            .context("bank window diarize")?;

        // Group window segments by raw cluster; compute per-cluster speech,
        // overlap fraction, and absolute-time spans.
        let all_spans: Vec<(f64, f64)> = out
            .segments
            .iter()
            .map(|s| (window_start_secs + s.start, window_start_secs + s.end))
            .collect();
        let regions = overlap::overlap_regions(&all_spans);

        // Preserve cluster iteration by descending speech (dominant first) so
        // enrollment order is deterministic and matches the measured harness.
        use std::collections::HashMap;
        let mut spans_by_cluster: HashMap<i64, Vec<(f64, f64)>> = HashMap::new();
        for s in &out.segments {
            spans_by_cluster
                .entry(s.speaker)
                .or_default()
                .push((window_start_secs + s.start, window_start_secs + s.end));
        }
        let centroids: HashMap<i64, Vec<f32>> = out.centroids.into_iter().collect();

        let mut clusters: Vec<WindowCluster> = Vec::new();
        for (cid, spans) in spans_by_cluster {
            let speech: f64 = spans.iter().map(|(s, e)| e - s).sum();
            let ov: f64 = spans
                .iter()
                .map(|(s, e)| overlap::overlapped_secs(&regions, *s, *e))
                .sum();
            let ovfrac = if speech > 0.0 { ov / speech } else { 0.0 };
            clusters.push(WindowCluster {
                cid,
                speech,
                ovfrac,
                spans,
            });
        }
        clusters.sort_by(|a, b| {
            b.speech
                .partial_cmp(&a.speech)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Observe each cluster's centroid; emit its spans under the assigned gid.
        // An unmatched cluster (gid None) waits for the next tick (the daemon's
        // assign step treats absence as "unlabeled").
        let mut segs = Vec::new();
        for WindowCluster {
            cid,
            speech,
            ovfrac,
            spans,
        } in clusters
        {
            if speech < MIN_OBSERVE_SECS {
                continue;
            }
            let Some(centroid) = centroids.get(&cid) else {
                continue;
            };
            let clean = ovfrac < OVERLAP_UNCLEAN_FRAC;
            let d = self.bank.observe(centroid, speech, clean);
            let Some(gid) = d.gid else { continue };
            for (s, e) in spans {
                if e - s > 0.05 {
                    segs.push(Segment {
                        start: s,
                        end: e,
                        speaker: gid,
                    });
                }
            }
        }
        Ok(segs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DIM: usize = 8;

    fn unit(axis: usize) -> Vec<f32> {
        let mut v = vec![0.0f32; DIM];
        v[axis] = 1.0;
        v
    }

    fn blend(a: &[f32], b: &[f32], t: f32) -> Vec<f32> {
        let mut v: Vec<f32> = a.iter().zip(b.iter()).map(|(x, y)| x + t * y).collect();
        l2_normalize(&mut v);
        v
    }

    fn probe_score(bank: &mut ProfileBank, probe: &[f32], expect_gid: i64) -> f32 {
        let d = bank.observe(probe, 0.5, true);
        assert_eq!(d.gid, Some(expect_gid));
        assert!(!d.updated);
        d.top_score
    }

    #[test]
    fn empty_bank_mints_then_matches() {
        let mut bank = ProfileBank::new(BankConfig::default());
        let e0 = unit(0);
        let d = bank.observe(&e0, 5.0, true);
        assert_eq!(d.gid, Some(0));
        assert!(d.minted);
        let d2 = bank.observe(&blend(&e0, &unit(1), 0.05), 5.0, true);
        assert_eq!(d2.gid, Some(0));
        assert!(!d2.minted);
        assert_eq!(bank.profile_count(), 1);
    }

    #[test]
    fn two_orthogonal_voices_get_distinct_gids() {
        let mut bank = ProfileBank::new(BankConfig::default());
        assert_eq!(bank.observe(&unit(0), 5.0, true).gid, Some(0));
        assert_eq!(bank.observe(&unit(1), 5.0, true).gid, Some(1));
        assert_eq!(bank.profile_count(), 2);
        for _ in 0..3 {
            assert_eq!(
                bank.observe(&blend(&unit(0), &unit(1), 0.05), 2.0, true)
                    .gid,
                Some(0)
            );
            assert_eq!(
                bank.observe(&blend(&unit(1), &unit(0), 0.05), 2.0, true)
                    .gid,
                Some(1)
            );
        }
    }

    #[test]
    fn patience_two_defers_then_mints() {
        let mut bank = ProfileBank::new(BankConfig {
            mint_patience: 2,
            ..BankConfig::default()
        });
        assert_eq!(bank.observe(&unit(0), 5.0, true).gid, None);
        assert_eq!(
            bank.observe(&blend(&unit(0), &unit(1), 0.05), 5.0, true)
                .gid,
            Some(0)
        );
    }

    #[test]
    fn ambiguous_assigns_but_never_updates() {
        let mut bank = ProfileBank::new(BankConfig::default());
        bank.observe(&unit(0), 5.0, true);
        bank.observe(&unit(1), 5.0, true);
        let d = bank.observe(&blend(&unit(0), &unit(1), 0.9), 5.0, true);
        assert_eq!(d.gid, Some(0));
        assert!(d.ambiguous);
        assert!(!d.updated);
        assert!(probe_score(&mut bank, &unit(0), 0) > 0.99999);
    }

    #[test]
    fn unclean_assigns_but_does_not_update() {
        let mut bank = ProfileBank::new(BankConfig::default());
        bank.observe(&unit(0), 5.0, true);
        let off = blend(&unit(0), &unit(1), 0.3);
        let before = probe_score(&mut bank, &unit(0), 0);
        let d = bank.observe(&off, 5.0, false);
        assert_eq!(d.gid, Some(0));
        assert!(!d.updated);
        let after = probe_score(&mut bank, &unit(0), 0);
        assert!((before - after).abs() < 1e-6);
        assert!(bank.observe(&off, 5.0, true).updated);
    }

    #[test]
    fn min_enroll_gate_blocks_until_enough_speech() {
        let mut bank = ProfileBank::new(BankConfig::default());
        assert_eq!(bank.observe(&unit(0), 1.0, true).gid, None);
        assert_eq!(bank.observe(&unit(0), 1.0, true).gid, None);
        assert_eq!(bank.observe(&unit(0), 1.0, true).gid, Some(0));
    }
}
