//! App-owned in-meeting conversation memory — daemon orchestration.
//!
//! Bluey stores every in-meeting Q&A turn in its OWN store (`conversation_turns`,
//! migration 012) and re-supplies the running dialogue to the agent each turn.
//! This is what lets "follow up on that" work WITHOUT depending on the agent's
//! resumable session — the foundation for driving agents ephemerally (Wave 3).
//!
//! Three pieces:
//! - [`record_turns`]: after a successful in-meeting answer, append the user's
//!   visible question + the copilot's post-guard answer. Fire-and-forget.
//! - [`conversation_context_block`]: assemble the token-bounded conversation
//!   block (running summary + verbatim tail) for the answer envelope.
//! - [`maybe_fold_conversation`]: when the tail overflows the budget, fold the
//!   oldest turns into the in-memory rolling summary via the stateless one-shot.
//!
//! The rolling summary lives in daemon memory (`Daemon::conv_summary`) for Wave
//! 1; the raw turns are the durable source of truth and re-foldable on restart.
//! All tunables come from [`cue_core::conversation::ConvConfig`] (env-overridable).

use std::sync::Arc;

use tracing::{debug, warn};
use uuid::Uuid;

use cue_core::conversation::{
    assemble_block, bound_summary, build_fold_prompt, ConvConfig, ConvRole,
};

use crate::app::Daemon;

/// Env flag gating the app-owned conversation Q&A memory. Default **OFF**.
///
/// This subsystem existed to hand-feed prior Q&A + a rolling summary into a
/// *stateless* CLI each turn. Since Bluey now drives agents via true session
/// **resume** (the agent keeps its own conversation state), that reconstruction
/// is redundant. Off by default means: no `conversation_turns` table is created
/// (see `Database::run_migrations`), and both the record and inject paths become
/// no-ops — zero storage, zero work. Set `BLUEY_CONV_MEMORY=1` to re-enable for
/// the case of driving a genuinely stateless agent.
pub(crate) const ENV_CONV_MEMORY: &str = "BLUEY_CONV_MEMORY";

/// Whether app-owned conversation Q&A memory is enabled. Default off.
pub(crate) fn conv_memory_enabled() -> bool {
    matches!(
        std::env::var(ENV_CONV_MEMORY).ok().as_deref(),
        Some("1") | Some("true") | Some("yes")
    )
}

/// Open the sessions DB (best-effort). Mirrors `persist_diarization`'s pattern:
/// SQLite calls are short and synchronous, the handle is never held across an
/// await. Returns `None` (logged) on failure — conversation memory is a
/// best-effort enhancement, never a hard dependency of the answer path.
fn open_db(daemon: &Arc<Daemon>) -> Option<crate::db::Database> {
    let db_path = daemon.paths.data_dir.join("sessions.db");
    match crate::db::Database::open(db_path.to_str().unwrap_or("sessions.db")) {
        Ok(db) => Some(db),
        Err(e) => {
            debug!("conversation: could not open db: {e:#}");
            None
        }
    }
}

/// Record one completed in-meeting exchange: the user's VISIBLE question and the
/// copilot's final (post-leak-guard) answer. Fire-and-forget — a DB hiccup must
/// never surface on the answer path. Skips empty text. Prunes to the stored-turn
/// cap after appending.
///
/// `question` MUST be the human-readable question (via `visible_question_for_source`),
/// never the raw internal ASK_RECENT_QUESTION pointer. Callers skip the warm-up
/// drive (it primes the session; it is not a conversation turn).
pub(crate) fn record_turns(daemon: &Arc<Daemon>, meeting_id: Uuid, question: &str, answer: &str) {
    if !conv_memory_enabled() {
        return;
    }
    let question = question.trim().to_string();
    let answer = answer.trim().to_string();
    if question.is_empty() && answer.is_empty() {
        return;
    }
    let daemon = Arc::clone(daemon);
    tokio::spawn(async move {
        let Some(db) = open_db(&daemon) else { return };
        // FK: conversation_turns references sessions(id); a meeting is a JSON
        // file, so ensure the placeholder parent row first (commit 338f1e9).
        if let Err(e) = db.ensure_meeting_session(meeting_id, None) {
            debug!("conversation: ensure_meeting_session failed: {e:#}");
        }
        if !question.is_empty() {
            if let Err(e) = db.conv_append(meeting_id, ConvRole::User, &question) {
                warn!("conversation: append user turn failed: {e:#}");
            }
        }
        if !answer.is_empty() {
            if let Err(e) = db.conv_append(meeting_id, ConvRole::Assistant, &answer) {
                warn!("conversation: append assistant turn failed: {e:#}");
            }
        }
        let cfg = ConvConfig::from_env();
        if let Err(e) = db.conv_prune(meeting_id, cfg.max_stored_turns) {
            debug!("conversation: prune failed: {e:#}");
        }
    });
}

/// Assemble the conversation block for the answer envelope: the in-memory
/// rolling summary + the newest turns that fit the token budget (sized to the
/// attached `model`'s context window). Returns `None` when there is no
/// conversation yet. Also spawns a fold when the tail overflows (older turns
/// exist that aren't yet in the summary) — see [`maybe_fold_conversation`].
pub(crate) async fn conversation_context_block(
    daemon: &Arc<Daemon>,
    meeting_id: Uuid,
    model: Option<&str>,
) -> Option<String> {
    if !conv_memory_enabled() {
        return None;
    }
    let cfg = ConvConfig::from_env();
    let db = open_db(daemon)?;
    let turns = match db.conv_turns(meeting_id, cfg.max_stored_turns) {
        Ok(t) => t,
        Err(e) => {
            debug!("conversation: read turns failed: {e:#}");
            return None;
        }
    };
    let summary = { daemon.conv_summary.lock().await.clone() };
    let (block, overflow) = assemble_block(&cfg, model, summary.as_deref(), &turns)?;
    if !overflow.is_empty() {
        maybe_fold_conversation(daemon, meeting_id, overflow.len());
    }
    Some(block)
}

/// Fold the `overflow_count` oldest stored turns into the rolling conversation
/// summary via the stateless one-shot, then delete those turns (so they aren't
/// summarized twice). Single-flight (`conv_fold_inflight`); the flag clears on
/// EVERY exit including a panic (Drop guard), mirroring the rolling-summary fold.
/// Best-effort: on failure the turns stay put and re-overflow next ask.
pub(crate) fn maybe_fold_conversation(
    daemon: &Arc<Daemon>,
    meeting_id: Uuid,
    overflow_count: usize,
) {
    use std::sync::atomic::Ordering::SeqCst;
    if overflow_count == 0 {
        return;
    }
    if !crate::app::live_memory_enabled(daemon) {
        return;
    }
    if daemon.conv_fold_inflight.swap(true, SeqCst) {
        return; // a fold is already running
    }
    let daemon = Arc::clone(daemon);
    tokio::spawn(async move {
        struct InflightClear(Arc<Daemon>);
        impl Drop for InflightClear {
            fn drop(&mut self) {
                self.0
                    .conv_fold_inflight
                    .store(false, std::sync::atomic::Ordering::SeqCst);
            }
        }
        let _inflight = InflightClear(Arc::clone(&daemon));

        let cfg = ConvConfig::from_env();
        // Read the exact overflow turns (the oldest `overflow_count`).
        let Some(db) = open_db(&daemon) else { return };
        let all = match db.conv_turns(meeting_id, cfg.max_stored_turns) {
            Ok(t) => t,
            Err(e) => {
                debug!("conversation fold: read turns failed: {e:#}");
                return;
            }
        };
        let take = overflow_count.min(all.len());
        if take == 0 {
            return;
        }
        let overflow = &all[..take];
        let current_summary = { daemon.conv_summary.lock().await.clone() };
        let prompt = build_fold_prompt(&cfg, current_summary.as_deref(), overflow);

        match memory_oneshot(&daemon, prompt).await {
            Some(raw) => {
                let bounded = bound_summary(&cfg, &raw);
                if bounded.is_empty() {
                    debug!("conversation fold: empty summary; turns kept for retry");
                    return;
                }
                // Only store + delete if the SAME meeting is still active
                // (a fold that outlives its meeting must not corrupt the next).
                let still_active = {
                    let guard = daemon.meeting.lock().await;
                    guard.as_ref().map(|m| m.id) == Some(meeting_id)
                };
                if !still_active {
                    debug!("conversation fold: meeting changed mid-fold; discarded");
                    return;
                }
                *daemon.conv_summary.lock().await = Some(bounded);
                if let Err(e) = db.conv_delete_oldest(meeting_id, take) {
                    warn!("conversation fold: delete folded turns failed: {e:#}");
                }
            }
            None => debug!("conversation fold: one-shot failed; turns kept for retry"),
        }
    });
}

/// Thin indirection to the app's stateless one-shot drive, kept here so the
/// module reads standalone. Delegates to `crate::app::memory_oneshot_via_agent`.
async fn memory_oneshot(daemon: &Arc<Daemon>, prompt: String) -> Option<String> {
    crate::app::memory_oneshot_via_agent(daemon, prompt).await
}

/// Reset the in-memory conversation summary + fold flag for a new meeting, and
/// clear any stale turns of the incoming meeting id from a prior run. Called at
/// every ledger-reset site so conversation memory never bleeds across meetings.
pub(crate) async fn reset_for_meeting(daemon: &Arc<Daemon>, new_meeting_id: Option<Uuid>) {
    *daemon.conv_summary.lock().await = None;
    daemon
        .conv_fold_inflight
        .store(false, std::sync::atomic::Ordering::SeqCst);
    // Only touch the DB when the feature is on — with it off the
    // `conversation_turns` table is never created (see `run_migrations`), so a
    // clear would be a guaranteed error on a non-existent table.
    if !conv_memory_enabled() {
        return;
    }
    if let Some(id) = new_meeting_id {
        if let Some(db) = open_db(daemon) {
            if let Err(e) = db.conv_clear(id) {
                debug!("conversation: clear stale turns for new meeting failed: {e:#}");
            }
        }
    }
}
