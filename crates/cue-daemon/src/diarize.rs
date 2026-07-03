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

use tracing::{info, warn};

use crate::app::Daemon;

/// Rolling window the live tier re-diarizes (seconds). 120s balances accuracy
/// (enough speech per speaker to embed well) against per-window cost.
pub const LIVE_WINDOW_SECS: usize = 120;

/// How often the live tier re-diarizes, in seconds. Overridable via
/// `BLUEY_DIARIZE_INTERVAL_SECS`. Default 30s → ~30s effective label latency,
/// which is fine for AI context (labels firm up as the meeting proceeds).
pub fn live_interval_secs() -> u64 {
    std::env::var("BLUEY_DIARIZE_INTERVAL_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .filter(|&n| n >= 5)
        .unwrap_or(30)
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

impl LiveDiarizerHandle {
    /// Non-blocking submit of a rolling window. Drops silently if the worker is
    /// still processing the previous one (channel full) — best-effort.
    fn try_submit(&self, window: Vec<f32>, start_secs: f64) {
        let _ = self.window_tx.try_send((window, start_secs));
    }
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

/// One LIVE tick (called on the audio task): submit the current rolling window
/// to the worker (non-blocking) and drain any ready results, stamping stable
/// speaker ids onto overlapping transcript segments. Never blocks on inference.
pub(crate) async fn live_tick(daemon: &Arc<Daemon>, handle: &mut LiveDiarizerHandle) {
    // Submit the current window (cheap copy; non-blocking send).
    let submit = {
        let guard = daemon.audio_retention.lock().await;
        match guard.as_ref() {
            Some(r) if r.duration_secs() >= 3.0 => Some(r.rolling_window()),
            _ => None,
        }
    };
    if let Some((window, start)) = submit {
        handle.try_submit(window, start);
    }

    // Drain any completed results (from THIS or a prior tick) and apply labels.
    while let Ok(segments) = handle.result_rx.try_recv() {
        label_segments_by_overlap(daemon, &segments).await;
    }
}

/// Assign each diarized segment's speaker id to transcript segments whose time
/// window overlaps it (system-side only). Updates the in-memory meeting; the
/// live-transcript event consumers pick up the label on the next emit.
async fn label_segments_by_overlap(daemon: &Arc<Daemon>, segments: &[cue_diarize::Segment]) {
    let mut guard = daemon.meeting.lock().await;
    let Some(meeting) = guard.as_mut() else {
        return;
    };
    // Meeting start epoch ms → convert segment abs seconds to a comparable clock.
    // Transcript segments carry `created_at` epoch-ms strings; we approximate
    // overlap by matching each transcript segment to the diarized segment whose
    // [start,end] contains its arrival time relative to meeting start.
    let meeting_start_ms: i64 = meeting
        .transcript
        .first()
        .and_then(|s| s.created_at.parse::<i64>().ok())
        .unwrap_or(0);
    for seg in meeting.transcript.iter_mut() {
        // Only the far (system) side gets an individual id.
        if seg.speaker.is_me() {
            continue;
        }
        let Ok(ts_ms) = seg.created_at.parse::<i64>() else {
            continue;
        };
        let rel_secs = (ts_ms - meeting_start_ms) as f64 / 1000.0;
        if let Some(d) = segments
            .iter()
            .find(|d| rel_secs >= d.start - 1.0 && rel_secs <= d.end + 1.0)
        {
            seg.speaker_id = Some(d.speaker);
        }
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

    // Rewrite far-side transcript speaker ids by time overlap, then re-archive.
    let meeting_start_ms: i64 = meeting
        .transcript
        .first()
        .and_then(|s| s.created_at.parse::<i64>().ok())
        .unwrap_or(0);
    let mut labeled = 0usize;
    for seg in meeting.transcript.iter_mut() {
        if seg.speaker.is_me() {
            continue;
        }
        let Ok(ts_ms) = seg.created_at.parse::<i64>() else {
            continue;
        };
        let rel = (ts_ms - meeting_start_ms) as f64 / 1000.0;
        if let Some(d) = out
            .segments
            .iter()
            .find(|d| rel >= d.start - 1.0 && rel <= d.end + 1.0)
        {
            seg.speaker_id = Some(d.speaker);
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
