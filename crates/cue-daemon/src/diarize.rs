//! Daemon-side speaker-diarization orchestration (feature `diarize`).
//!
//! Two tiers over the retained meeting audio (see `audio::retention`):
//!   * LIVE — every `live_interval_secs()`, re-diarize the rolling window with a
//!     persistent [`cue_diarize::LiveDiarizer`] (stable ids via centroid
//!     inheritance) and label recent transcript segments by time overlap.
//!   * POST — on meeting end, run [`cue_diarize::Diarizer`] over the full buffer
//!     for authoritative labels, persist utterances + resolved speakers, and
//!     rewrite the transcript speaker ids.
//!
//! All heavy work runs off the async runtime via `spawn_blocking` (speakrs
//! inference is CPU/CoreML-bound). Gated so the default daemon never links it.

use std::sync::Arc;

use tracing::{debug, info, warn};

use crate::app::Daemon;

/// Rolling window the live tier re-diarizes (seconds). 30s is the diart-style
/// "local segmentation buffer": long enough for speakrs's VBx to separate ≤4
/// speakers reliably, short enough that after 30s the window is genuinely ROLLING
/// (fixed length) rather than growing-from-0 — which, combined with the persistent
/// arrival-ordered speaker set in `LiveDiarizer`, is what keeps ids stable.
pub const LIVE_WINDOW_SECS: usize = 30;

/// How often the live tier re-diarizes, in seconds. Overridable via
/// `BLUEY_DIARIZE_INTERVAL_SECS`. Default 15s → 30s window / 15s step: labels firm
/// up reasonably fast, but the per-tick window CLONE (held under the retention
/// mutex, contended with the 20 ms chunk push) happens half as often as at 10s,
/// so STT stays smooth. Don't drop below ~15 without profiling the STT hot path.
pub fn live_interval_secs() -> u64 {
    std::env::var("BLUEY_DIARIZE_INTERVAL_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .filter(|&n| n >= 5)
        .unwrap_or(15)
}

/// Whether diarization is enabled at runtime. Off unless `BLUEY_DIARIZE=1` — the
/// feature being compiled in does not by itself turn it on (it's still heavy).
pub fn enabled() -> bool {
    std::env::var("BLUEY_DIARIZE")
        .map(|v| v == "1")
        .unwrap_or(false)
}

/// Handle to the live diarizer running on its OWN OS thread.
///
/// Critical: speakrs inference blocks for ~1s. Running it inline on the audio
/// `tokio::select!` task would stall `sys_rx.recv()` for that second every tick
/// and back up the audio channel (the same dropout class we fixed for the STT
/// sink). So the diarizer lives on a dedicated thread — the audio task only does
/// a NON-BLOCKING `try_send(window)` and receives labeled segments back
/// asynchronously. If the worker is busy, the window is simply dropped (best-
/// effort; the next tick re-diarizes the latest audio anyway).
pub(crate) struct LiveDiarizerHandle {
    /// audio task → worker: (window samples, window_start_secs). Capacity 1 so a
    /// slow worker never queues stale windows; a full channel drops the tick.
    /// Tokio channels (Send + Sync) so the handle can be held across `.await`.
    window_tx: tokio::sync::mpsc::Sender<(Vec<f32>, f64)>,
    /// worker → daemon: labeled segments for the last processed window.
    result_rx: tokio::sync::mpsc::UnboundedReceiver<Vec<cue_diarize::Segment>>,
}

/// Spawn the live diarizer worker thread if diarization is enabled. The heavy
/// speakrs model loads ON the worker thread. Returns `None` when disabled or on
/// load failure (best-effort; STT must never be blocked).
pub(crate) fn spawn_live_diarizer() -> Option<LiveDiarizerHandle> {
    if !enabled() {
        return None;
    }
    let (window_tx, mut window_rx) = tokio::sync::mpsc::channel::<(Vec<f32>, f64)>(1);
    let (result_tx, result_rx) =
        tokio::sync::mpsc::unbounded_channel::<Vec<cue_diarize::Segment>>();
    // Dedicated OS thread (NOT a tokio task): speakrs inference is blocking and
    // must never run on the async runtime / audio task.
    std::thread::Builder::new()
        .name("diarize-live".into())
        .spawn(move || {
            let mut diarizer =
                match cue_diarize::LiveDiarizer::load(cue_diarize::Backend::preferred()) {
                    Ok(d) => {
                        info!("diarize: live diarizer loaded (worker thread)");
                        d
                    }
                    Err(e) => {
                        warn!("diarize: live diarizer unavailable: {e:#}");
                        return;
                    }
                };
            // Block on each submitted window; run inference; ship results back.
            while let Some((window, start)) = window_rx.blocking_recv() {
                match diarizer.push_window(&window, start) {
                    Ok(segs) if !segs.is_empty() => {
                        if result_tx.send(segs).is_err() {
                            break; // daemon gone
                        }
                    }
                    Ok(_) => {}
                    Err(e) => warn!("diarize: live window failed: {e:#}"),
                }
            }
        })
        .ok()?;
    Some(LiveDiarizerHandle {
        window_tx,
        result_rx,
    })
}

/// One LIVE tick (called on the audio task). CRITICAL: this must do essentially
/// ZERO work on the audio task — the select! loop that calls it is the same one
/// that reads `sys_rx`, so any lock wait, big clone, or disk I/O here stalls audio
/// intake and EATS WORDS. So it only (1) drains ready diarizer results NON-blocking
/// (`try_recv`) and hands the label+persist work to a DETACHED task, and (2) spawns
/// a DETACHED task to grab the rolling window (which locks retention and clones
/// ~1.9 MB) and submit it to the worker thread. The audio loop returns to
/// `sys_rx.recv()` immediately.
pub(crate) fn live_tick(daemon: &Arc<Daemon>, handle: &mut LiveDiarizerHandle) {
    // 1) Drain completed results without blocking; label+persist off the audio task.
    while let Ok(segments) = handle.result_rx.try_recv() {
        let d = daemon.clone();
        tokio::spawn(async move {
            label_segments_by_overlap(&d, &segments).await;
        });
    }

    // 2) Grab + submit the FULL meeting audio (from t=0) on a detached task.
    //    Re-diarizing the whole meeting each tick — not a 30s rolling window — is
    //    what lets the live tier re-label EARLY segments correctly: with a rolling
    //    window, a segment labelled when only one speaker had spoken freezes at
    //    that id once the window scrolls past it (it can never see the later
    //    speakers). The full buffer always contains every speaker, and
    //    `label_segments_by_overlap` overwrites every segment each tick, so labels
    //    converge to the whole-meeting picture. Runs on the diarizer's dedicated
    //    thread, so the growing per-tick cost never touches STT. (retention lock +
    //    clone must NOT run on the audio select! loop.)
    let d = daemon.clone();
    let tx = handle.window_tx.clone();
    tokio::spawn(async move {
        let submit = {
            let guard = d.audio_retention.lock().await;
            match guard.as_ref() {
                Some(r) if r.duration_secs() >= 3.0 => Some((r.full().to_vec(), 0.0_f64)),
                _ => None,
            }
        };
        if let Some((window, start)) = submit {
            debug!(
                samples = window.len(),
                start_secs = start,
                "diarize: live_tick submitting FULL meeting audio"
            );
            let _ = tx.try_send((window, start));
        }
    });
}

/// Assign each diarized segment's speaker id to transcript segments whose time
/// window overlaps it (system-side only). Updates the in-memory meeting AND
/// re-broadcasts each newly-labelled segment over the live-transcript WebSocket,
/// so a view that already received the line (with `speaker=None`) can upgrade the
/// label in place to the real "Speaker N".
async fn label_segments_by_overlap(daemon: &Arc<Daemon>, segments: &[cue_diarize::Segment]) {
    // Collect what changed while holding the meeting lock; broadcast after
    // releasing it (broadcast::send is sync and non-blocking, but keep the
    // lock scope tight).
    let mut updates: Vec<(String, String, i64, u64)> = Vec::new();
    let session_id;
    // Snapshot to persist AFTER releasing the meeting lock. Saving under the lock
    // is a ~50-200ms synchronous disk write that would block the STT sink from
    // committing segments — i.e. "STT goes off while diarization runs". We stamp
    // ids under the lock (cheap), clone once, release, then save off-lock.
    let mut to_persist: Option<cue_core::meeting::MeetingRecord> = None;
    {
        let mut guard = daemon.meeting.lock().await;
        let Some(meeting) = guard.as_mut() else {
            return;
        };
        session_id = meeting.id.to_string();
        for seg in meeting.transcript.iter_mut() {
            // Only the far (system) side gets an individual id (mic = the user).
            if seg.speaker.is_me() {
                continue;
            }
            if let Some(speaker) = assign_speaker(seg, segments) {
                if seg.speaker_id != Some(speaker) {
                    seg.speaker_id = Some(speaker);
                    let ts_ms = seg.created_at.parse::<u64>().unwrap_or(0);
                    updates.push((seg.text.clone(), seg.speaker.to_string(), speaker, ts_ms));
                }
            }
        }
        if !updates.is_empty() {
            to_persist = Some(meeting.clone());
        }
    } // meeting lock released HERE — before any disk I/O.

    // Persist off-lock so the STT sink never waits on the diarizer's disk write.
    if let Some(meeting) = to_persist {
        if let Err(e) = daemon.store.save_active(&meeting) {
            warn!("diarize: failed to persist live speaker labels: {e:#}");
        }
    }

    for (text, source, speaker_id, ts_ms) in updates {
        crate::app::broadcast_speaker_update(
            daemon,
            session_id.clone(),
            source,
            text,
            speaker_id,
            ts_ms,
        );
    }
}

/// Distance (seconds) from a point to a [start, end] range: 0 if inside, else
/// the gap to the nearer edge. Used to pick the nearest diarized segment.
fn dist_to_range(point: f64, start: f64, end: f64) -> f64 {
    if point < start {
        start - point
    } else if point > end {
        point - end
    } else {
        0.0
    }
}

/// Default speech span (seconds) assumed for a transcript segment lacking a
/// measured `audio_dur_secs` — roughly one Nemotron final's worth of audio.
const DEFAULT_SEGMENT_DUR_SECS: f64 = 0.6;

/// Cap (seconds) on the nearest-segment fallback: if the closest diarized turn is
/// farther than this from the transcript segment, leave it unlabeled rather than
/// inherit a distant speaker (WhisperX drops words with no overlap unless
/// `fill_nearest`; this is the capped compromise).
const NEAREST_FALLBACK_CAP_SECS: f64 = 2.0;

/// Assign a diarized speaker to a transcript segment using WhisperX-style
/// **max-total-overlap**: the segment forms an interval `[start, start+dur]` on
/// the shared audio clock; for each diarized turn accumulate the overlap
/// `max(0, min(ends) − max(starts))`, and pick the speaker with the most total
/// overlap. If nothing overlaps, fall back to the nearest turn within
/// [`NEAREST_FALLBACK_CAP_SECS`]; beyond that, return `None` (don't mislabel).
fn assign_speaker(
    seg: &cue_core::meeting::TranscriptSegment,
    turns: &[cue_diarize::Segment],
) -> Option<i64> {
    let start = seg.audio_start_secs?;
    let end = start + seg.audio_dur_secs.unwrap_or(DEFAULT_SEGMENT_DUR_SECS);

    // Only consider turns the live tier actually labelled (skip the UNLABELED
    // sentinel — an unmatched turn must never stamp -1 onto a transcript segment).
    let labelled = || turns.iter().filter(|t| t.speaker >= 0);

    // 1) Max-total-overlap.
    let mut best: Option<(i64, f64)> = None;
    let mut overlap_by_speaker: std::collections::HashMap<i64, f64> =
        std::collections::HashMap::new();
    for t in labelled() {
        let ov = (t.end.min(end) - t.start.max(start)).max(0.0);
        if ov > 0.0 {
            *overlap_by_speaker.entry(t.speaker).or_insert(0.0) += ov;
        }
    }
    for (&spk, &ov) in &overlap_by_speaker {
        if best.map(|(_, b)| ov > b).unwrap_or(true) {
            best = Some((spk, ov));
        }
    }
    if let Some((spk, _)) = best {
        return Some(spk);
    }

    // 2) Nearest-turn fallback, capped.
    let point = start;
    let nearest = labelled().min_by(|a, b| {
        dist_to_range(point, a.start, a.end)
            .partial_cmp(&dist_to_range(point, b.start, b.end))
            .unwrap_or(std::cmp::Ordering::Equal)
    })?;
    if dist_to_range(point, nearest.start, nearest.end) <= NEAREST_FALLBACK_CAP_SECS {
        Some(nearest.speaker)
    } else {
        None
    }
}

/// Convert an i16 PCM chunk (the capture format) to f32 in [-1, 1] for retention.
pub fn i16_to_f32(samples: &[i16]) -> Vec<f32> {
    samples.iter().map(|&s| s as f32 / 32768.0).collect()
}

/// POST-process pass: run the authoritative diarizer over the full retained
/// buffer, rewrite the (already-ended, taken-by-value) meeting's transcript
/// speaker ids, and RE-ARCHIVE it so the saved record + AI context carry the
/// resolved speakers. Fire-and-forget on meeting end. `_session_id` reserved for
/// future DB persistence of utterances/centroids.
pub(crate) async fn post_process_meeting(
    daemon: Arc<Daemon>,
    mut meeting: cue_core::meeting::MeetingRecord,
    session_id: String,
) {
    if !enabled() {
        return;
    }
    // Take + clear the full buffer (frees RAM after the meeting).
    let audio = {
        let mut guard = daemon.audio_retention.lock().await;
        match guard.as_mut() {
            Some(r) if r.duration_secs() >= 3.0 => {
                let a = r.full().to_vec();
                r.clear();
                a
            }
            _ => return,
        }
    };
    info!(
        secs = audio.len() / 16_000,
        "diarize: post-process starting"
    );

    // Heavy: run speakrs off the async runtime.
    let out = match tokio::task::spawn_blocking(move || {
        let mut d = cue_diarize::Diarizer::load(cue_diarize::Backend::preferred())?;
        d.diarize_with_centroids(&audio)
    })
    .await
    {
        Ok(Ok(out)) => out,
        Ok(Err(e)) => {
            warn!("diarize: post-process diarize failed: {e:#}");
            return;
        }
        Err(e) => {
            warn!("diarize: post-process task join failed: {e}");
            return;
        }
    };

    // Persist voice embeddings + resolved speakers to SQLite (enables a future
    // cross-meeting speaker gallery). Best-effort: a DB failure must not lose the
    // transcript rewrite below.
    persist_diarization(&daemon, &session_id, &out).await;

    // Rewrite far-side transcript speaker ids using the shared audio clock and the
    // same max-total-overlap rule as the live tier (`assign_speaker`). The post
    // pass is authoritative — it re-runs the diarizer over the full buffer.
    let mut labeled = 0usize;
    for seg in meeting.transcript.iter_mut() {
        if seg.speaker.is_me() {
            continue;
        }
        if let Some(speaker) = assign_speaker(seg, &out.segments) {
            seg.speaker_id = Some(speaker);
            labeled += 1;
        }
    }
    if let Err(e) = daemon.store.archive(&meeting) {
        warn!("diarize: re-archive after diarization failed: {e:#}");
    }
    info!(
        speakers = out.centroids.len(),
        labeled, "diarize: post-process complete"
    );
}

/// Write the resolved-speaker centroids (`meeting_speaker`) and per-segment
/// voice embeddings (`utterance`) to SQLite. Each utterance embedding is the
/// centroid of its assigned speaker (speakrs gives us per-speaker centroids, not
/// per-segment vectors — sufficient for the cross-meeting gallery use case).
/// Best-effort: opens the same `sessions.db` the rest of the daemon uses.
async fn persist_diarization(
    daemon: &Arc<Daemon>,
    session_id: &str,
    out: &cue_diarize::DiarizeOutput,
) {
    let db_path = daemon.paths.data_dir.join("sessions.db");
    let db = match crate::db::Database::open(db_path.to_str().unwrap_or("sessions.db")) {
        Ok(db) => db,
        Err(e) => {
            warn!("diarize: could not open db to persist embeddings: {e:#}");
            return;
        }
    };
    let now_ms = cue_core::clock::now_epoch_ms_string()
        .parse::<i64>()
        .unwrap_or(0);
    let centroid_of: std::collections::HashMap<i64, &Vec<f32>> =
        out.centroids.iter().map(|(id, c)| (*id, c)).collect();

    // Resolved speakers.
    for (id, centroid) in &out.centroids {
        if let Err(e) = db.upsert_meeting_speaker(session_id, *id, centroid, 0, 0, now_ms) {
            warn!("diarize: upsert_meeting_speaker failed: {e:#}");
        }
    }
    // One utterance row per diarized segment (embedding = its speaker's centroid).
    let mut n = 0;
    for seg in &out.segments {
        let Some(emb) = centroid_of.get(&seg.speaker) else {
            continue;
        };
        let start_ms = (seg.start * 1000.0) as i64;
        let end_ms = (seg.end * 1000.0) as i64;
        match db.insert_utterance(
            session_id,
            "system",
            start_ms,
            end_ms,
            Some(seg.speaker),
            emb,
            now_ms,
        ) {
            Ok(row_id) => {
                // This IS the authoritative pass, so final == provisional.
                let _ = db.set_utterance_final_speaker(row_id, seg.speaker);
                n += 1;
            }
            Err(e) => warn!("diarize: insert_utterance failed: {e:#}"),
        }
    }
    // Mark each utterance's authoritative speaker (== provisional here, since this
    // IS the authoritative pass).
    info!(
        speakers = out.centroids.len(),
        utterances = n,
        "diarize: persisted embeddings to sessions.db"
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use cue_core::meeting::{Speaker, TranscriptSegment};
    use cue_diarize::Segment;

    fn seg(start: f64, dur: f64) -> TranscriptSegment {
        TranscriptSegment::new(Speaker::System, "x", true)
            .with_audio_start_secs(Some(start))
            .with_audio_dur_secs(Some(dur))
    }
    fn turn(start: f64, end: f64, speaker: i64) -> Segment {
        Segment {
            start,
            end,
            speaker,
        }
    }

    #[test]
    fn max_overlap_picks_the_dominant_speaker() {
        // Segment [10.0, 11.0]. Speaker 0 covers 10.0–10.2 (0.2s overlap),
        // Speaker 1 covers 10.2–11.5 (0.8s overlap) → Speaker 1 wins.
        let s = seg(10.0, 1.0);
        let turns = [turn(9.0, 10.2, 0), turn(10.2, 11.5, 1)];
        assert_eq!(assign_speaker(&s, &turns), Some(1));
    }

    #[test]
    fn full_containment_assigns_that_speaker() {
        let s = seg(5.0, 0.6);
        let turns = [turn(4.0, 6.0, 2), turn(6.0, 8.0, 3)];
        assert_eq!(assign_speaker(&s, &turns), Some(2));
    }

    #[test]
    fn no_overlap_uses_nearest_within_cap() {
        // Segment at 20.0; nearest turn ends at 19.5 (0.5s away < 2s cap).
        let s = seg(20.0, 0.6);
        let turns = [turn(10.0, 19.5, 7)];
        assert_eq!(assign_speaker(&s, &turns), Some(7));
    }

    #[test]
    fn far_beyond_cap_returns_none() {
        // Nearest turn ends 5s before the segment → beyond the 2s cap → unlabeled.
        let s = seg(30.0, 0.6);
        let turns = [turn(10.0, 25.0, 4)];
        assert_eq!(assign_speaker(&s, &turns), None);
    }

    #[test]
    fn no_audio_clock_returns_none() {
        let s = TranscriptSegment::new(Speaker::System, "x", true); // audio_start_secs = None
        let turns = [turn(0.0, 10.0, 0)];
        assert_eq!(assign_speaker(&s, &turns), None);
    }
}
