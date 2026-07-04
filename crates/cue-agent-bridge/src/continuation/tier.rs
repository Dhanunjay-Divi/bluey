//! Continuation-tier policy: turn a pinned prior session into a ready-to-drive
//! [`Question`], per the agent's [`ContinuationTier`].
//!
//! This is the decision the registry row drives:
//!
//! - **NativeResume** (Claude/Codex/Copilot, and Cursor ledger-gated) — the agent
//!   can reopen a session by id. Over ACP we attempt TRUE in-place resume
//!   (`session/load`, cwd = the session's project dir, which is part of the
//!   on-disk path) and keep the transcript as a fork fallback; off ACP we set
//!   `--resume` + cwd. If the project cwd is gone we degrade to replay. For a
//!   `resume_requires_ledger` agent (Cursor), the id must ALSO be one Bluey
//!   itself minted — proven by a spawn-time [`crate::sessions::ledger`] hit;
//!   without ledger provenance the tier degrades to Replay (a wrong-cwd/unknown
//!   Cursor id silently mints an empty session, which no error can catch).
//! - **Replay** (Cursor GUI ids/VS Code/Gemini/Antigravity) — no resume-by-id, so
//!   load the transcript and replay it as `context` (compacted if huge).
//!
//! The session ledger ([`resolve_session`]) is consulted FIRST — before store
//! discovery — so an undiscovered agent can still recover its cwd, and the cwd
//! never depends on the drift-prone store re-scrape. The transcript read is
//! bounded and read-only; the compaction is injected as a closure so this stays
//! decoupled from the daemon's drive machinery (Bluey runs no AI of its own — the
//! user's agent summarizes).

use std::future::Future;
use std::path::Path;

use crate::registry::{entry_for, ContinuationTier, KindTag};
use crate::sessions::{ledger, reader_for};
use crate::{discover_agents, AgentKind, Question, Transcript};

/// Cap on turns loaded from a session being continued — bounded so a
/// pathological transcript can't blow memory before compaction runs.
pub const CONTINUATION_READ_MAX_TURNS: usize = 4_000;

/// The kind to ACTUALLY drive for a (possibly cross-surface) continuation. When
/// `replaying` and the agent declares a `continuation_via` sibling — i.e. it has
/// no CLI of its own but its transcript can be continued through a sibling's CLI
/// (VS Code Copilot → Copilot CLI) — return the sibling kind. Otherwise return
/// the original kind unchanged. Pure registry lookup, data-driven.
#[must_use]
pub fn continuation_bridge_kind(kind: &AgentKind, replaying: bool) -> AgentKind {
    if !replaying {
        return kind.clone();
    }
    let via = KindTag::from_agent_kind(kind)
        .and_then(entry_for)
        .and_then(|e| e.continuation_via);
    match via {
        Some(tag) => tag.to_agent_kind(),
        None => kind.clone(),
    }
}

/// Result of resolving a pinned session: its project cwd, transcript, and
/// whether the cwd came from the spawn-time ledger (provenance for
/// `resume_requires_ledger` agents).
#[derive(Debug, Clone, Default)]
pub struct ResolvedSession {
    /// The session's project/working dir, if recovered (ledger cwd wins; else the
    /// store re-scrape).
    pub project: Option<String>,
    /// The bounded transcript, if readable (Replay context / NativeResume
    /// fork-fallback safety net).
    pub transcript: Option<Transcript>,
    /// True when `project` came from a spawn-time ledger hit — the provenance a
    /// `resume_requires_ledger` agent (Cursor) requires to keep NativeResume.
    pub ledger_hit: bool,
}

/// Resolve a pinned session: recover its project cwd (ledger-first, then the
/// store re-scrape) and its bounded transcript. Read-only, fail-soft.
///
/// Semantics (BINDING):
/// 1. The ledger lookup runs FIRST, before `discover_agents`, so an undiscovered
///    agent can still resolve a ledger cwd. A ledger record with `Some(cwd)` →
///    `project` = that cwd, `ledger_hit = true`, and the store `reader.list`
///    project re-scrape is SKIPPED.
/// 2. A ledger miss / a record without a cwd / `ledger_path` = `None` → today's
///    `reader.list` project path, `ledger_hit = false`.
/// 3. The transcript read (the fork-fallback safety net) is ALWAYS still
///    attempted via store resolution, fail-soft — independent of the ledger.
///
/// `list_cap` bounds how many session refs are scanned to recover the project
/// path. `tier` is accepted for call-site clarity but both tiers load the
/// transcript. `ledger_path = None` is byte-identical to the pre-ledger behavior.
pub async fn resolve_session(
    agent: &AgentKind,
    session_id: &str,
    tier: ContinuationTier,
    list_cap: usize,
    ledger_path: Option<&Path>,
) -> ResolvedSession {
    let _ = tier; // both tiers load the transcript now; kept for call-site clarity

    // (1) Ledger FIRST — before discovery, so it survives an undiscovered agent.
    let ledger_cwd = ledger_path
        .and_then(|p| ledger::lookup(p, agent, session_id))
        .and_then(|r| r.cwd);
    let ledger_hit = ledger_cwd.is_some();

    // Store resolution (for the store-derived project re-scrape AND the transcript
    // fork-fallback). An undiscovered agent / missing store still returns the
    // ledger cwd if we have one.
    let store = discover_agents()
        .into_iter()
        .find(|d| &d.kind == agent)
        .and_then(|d| d.session_store);
    let Some(store) = store else {
        // No store: a ledger cwd (if any) still supplies the project; no
        // transcript is readable.
        return ResolvedSession {
            project: ledger_cwd,
            transcript: None,
            ledger_hit,
        };
    };
    let reader = reader_for(store.format);

    // (2) Project: prefer the ledger cwd; only re-scrape the store when we have no
    // ledger hit (the drift-exposed read the ledger closes).
    let project = if ledger_hit {
        ledger_cwd
    } else {
        reader.list(&store, list_cap).ok().and_then(|refs| {
            refs.into_iter()
                .find(|r| r.id == session_id)
                .and_then(|r| r.project)
        })
    };

    // (3) Transcript: always attempted (Replay context / NativeResume fork
    // fallback). Bounded, far cheaper than the drive; `apply_tier` attaches it
    // only where needed.
    let transcript = reader
        .read(&store, session_id, CONTINUATION_READ_MAX_TURNS)
        .ok();

    ResolvedSession {
        project,
        transcript,
        ledger_hit,
    }
}

/// The tier actually applied after the `resume_requires_ledger` degrade: a
/// NativeResume agent that gates resume on ledger provenance (Cursor) falls back
/// to Replay when the session id has NO spawn-time ledger hit — a wrong-cwd /
/// unknown id would otherwise silently mint an empty session (exit 0), which no
/// error-based degrade can catch. Every other case keeps the row's tier. Pure.
#[must_use]
pub(crate) fn effective_tier(
    row_tier: ContinuationTier,
    resume_requires_ledger: bool,
    ledger_hit: bool,
) -> ContinuationTier {
    if row_tier == ContinuationTier::NativeResume && resume_requires_ledger && !ledger_hit {
        ContinuationTier::Replay
    } else {
        row_tier
    }
}

/// Upgrade `question` to **continue a specific prior session**, per the agent's
/// [`ContinuationTier`]. A no-op when `session_id` is empty/blank.
///
/// `via_acp` selects the true-resume-with-fork-fallback path for NativeResume
/// agents (see the module docs). `list_cap` bounds the project-path lookup.
/// `ledger_path` (the spawn-time session ledger, or `None` on headless paths)
/// supplies cwd provenance: for a `resume_requires_ledger` agent a NativeResume
/// without a ledger hit degrades to Replay. `summarize` is the injected
/// compaction summarizer (driven through the user's own agent); it is only
/// invoked when a transcript is large enough to need it (see
/// [`super::maybe_compact`]).
#[allow(clippy::too_many_arguments)]
pub async fn apply_tier<F, Fut>(
    question: &mut Question,
    agent: &AgentKind,
    session_id: Option<&str>,
    via_acp: bool,
    list_cap: usize,
    ledger_path: Option<&Path>,
    summarize: F,
) where
    F: Fn(String) -> Fut,
    Fut: Future<Output = Option<String>>,
{
    let Some(session_id) = session_id.map(str::trim).filter(|s| !s.is_empty()) else {
        return; // fresh question — nothing to continue
    };
    let Some(tag) = KindTag::from_agent_kind(agent) else {
        return;
    };
    let Some(entry) = entry_for(tag) else {
        return;
    };

    // Resolve the session's project (cwd), transcript, and ledger provenance.
    let resolved =
        resolve_session(agent, session_id, entry.continuation, list_cap, ledger_path).await;
    let project = resolved.project;
    let transcript = resolved.transcript;

    // Degrade NativeResume → Replay for a ledger-gated agent (Cursor) whose id has
    // no spawn-time provenance; every other case keeps the row tier.
    let tier = effective_tier(
        entry.continuation,
        entry.resume_requires_ledger,
        resolved.ledger_hit,
    );

    // Will the project dir actually be usable as a cwd? (exists + non-empty —
    // mirrors the drive layer's guard). A missing/empty dir means a cwd-scoped
    // native resume (Claude) would resolve against the WRONG directory and
    // silently start fresh, losing all context.
    let cwd_usable = project.as_deref().is_some_and(|p| {
        let path = std::path::Path::new(p);
        path.is_dir()
            && std::fs::read_dir(path)
                .map(|mut e| e.next().is_some())
                .unwrap_or(false)
    });

    match tier {
        // ACP path. The on-disk session id IS the SDK's resume key — `session/load`
        // resolves `~/.claude/projects/<encoded-cwd>/<id>.jsonl` and the CWD is part
        // of that path (platform.claude.com/docs/en/agent-sdk/sessions). So:
        //   - cwd usable  → TRUE resume (set `resume` + the session's project cwd)
        //     and DO NOT also replay the transcript. `session/load` already gives
        //     the agent its full history; replaying the transcript on top makes the
        //     agent see every prior turn twice and echo old answers back into the
        //     new one (the "doubling" bug). The ACP drive has its own fork fallback
        //     (fresh session + replayed `render_prompt` context) when `session/load`
        //     fails, so we don't need to pre-load `context` here.
        //   - cwd missing → resume can't resolve the path (would silently start
        //     fresh), so go straight to fork: drop `resume`, replay the transcript.
        ContinuationTier::NativeResume if via_acp => {
            if cwd_usable {
                tracing::info!(
                    session = %session_id,
                    cwd = project.as_deref().unwrap_or(""),
                    "ACP continuation: TRUE resume (session/load), no transcript replay"
                );
                question.cwd = project;
                question.resume = Some(session_id.to_string());
                // NOTE: intentionally leave `question.context` unset — resume
                // already carries the history; replaying it duplicates the turns.
                // (The daemon may still attach MEETING context — brief/ledger/
                // transcript that the vendor session does NOT hold — and decides
                // per turn how much via the first-turn "primed" marker; that is a
                // separate channel from the prior-agent history and is correct to
                // send.)
            } else {
                question.resume = None;
                tracing::info!(
                    session = %session_id,
                    "ACP continuation: cwd unusable → FORK (replay context, no resume)"
                );
                if let Some(t) = transcript {
                    question.context = Some(super::maybe_compact(t, &summarize).await);
                }
            }
        }
        ContinuationTier::NativeResume if cwd_usable => {
            // Non-ACP native resume: drive in the project dir, resume by id, let
            // the vendor handle context compaction. Any meeting context the daemon
            // attached (brief/ledger/transcript the vendor session does NOT hold)
            // is a separate channel and stays — the daemon decides per turn how
            // much via the first-turn "primed" marker.
            question.cwd = project;
            question.resume = Some(session_id.to_string());
        }
        ContinuationTier::NativeResume => {
            // Native resume but the project cwd is MISSING/EMPTY (e.g. a moved or
            // un-synced folder). A cwd-scoped `--resume` (Claude) would resolve
            // against the wrong directory and silently start fresh with NO
            // context. So degrade to REPLAY: drop the resume id and the unusable
            // cwd, and replay the transcript as context instead — the
            // continuation still works, just without the vendor's native resume.
            question.resume = None;
            question.cwd = None;
            if let Some(t) = transcript {
                question.context = Some(super::maybe_compact(t, &summarize).await);
            }
        }
        ContinuationTier::Replay => {
            // No native resume: replay the transcript as context. Never pass a
            // resume id (the agent's --resume can't target it / would mis-fire).
            // Set the project cwd ONLY if usable, so the agent operates in the
            // right repo without crashing on a missing/empty folder.
            question.resume = None;
            if cwd_usable {
                question.cwd = project;
            }
            if let Some(t) = transcript {
                question.context = Some(super::maybe_compact(t, &summarize).await);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn continuation_bridge_routes_vscode_to_copilot_only_when_replaying() {
        // VS Code Copilot has no CLI of its own; its `continuation_via` sibling is
        // the Copilot CLI. When replaying, the drive kind bridges to the sibling.
        let bridged = continuation_bridge_kind(&AgentKind::VsCodeFork, true);
        assert_eq!(
            bridged,
            AgentKind::Copilot,
            "replay bridges to the sibling CLI"
        );

        // Not replaying → unchanged (native drive of the same kind).
        assert_eq!(
            continuation_bridge_kind(&AgentKind::VsCodeFork, false),
            AgentKind::VsCodeFork
        );

        // An agent with no `continuation_via` is unchanged even when replaying.
        assert_eq!(
            continuation_bridge_kind(&AgentKind::Cursor, true),
            AgentKind::Cursor
        );
        assert_eq!(
            continuation_bridge_kind(&AgentKind::ClaudeCode, true),
            AgentKind::ClaudeCode
        );
    }

    #[test]
    fn test_native_resume_degrades_to_replay_without_ledger_provenance() {
        // A ledger-gated NativeResume agent (Cursor) with NO ledger hit degrades to
        // Replay — a wrong-cwd/unknown id would otherwise silently mint an empty
        // session that no error can catch.
        assert_eq!(
            effective_tier(ContinuationTier::NativeResume, true, false),
            ContinuationTier::Replay,
            "ledger-gated NativeResume without provenance → Replay"
        );
        // With a ledger hit, the gate is satisfied → stays NativeResume.
        assert_eq!(
            effective_tier(ContinuationTier::NativeResume, true, true),
            ContinuationTier::NativeResume
        );
        // A non-gated NativeResume agent (Claude) is unaffected by ledger state.
        assert_eq!(
            effective_tier(ContinuationTier::NativeResume, false, false),
            ContinuationTier::NativeResume
        );
        // Replay stays Replay regardless.
        assert_eq!(
            effective_tier(ContinuationTier::Replay, true, false),
            ContinuationTier::Replay
        );
    }

    fn tmp_ledger_path(name: &str) -> std::path::PathBuf {
        let mut p = std::env::temp_dir();
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        p.push(format!("bluey-tier-test-{name}-{unique}.jsonl"));
        p
    }

    #[tokio::test]
    async fn test_resolve_session_ledger_hit_supplies_project_without_discovery() {
        // `Other("ledger-test")` is NEVER a discovered agent on any machine, so a
        // ledger hit is the ONLY way project can be non-None — deterministic.
        let agent = AgentKind::Other("ledger-test".to_string());
        let path = tmp_ledger_path("hit");
        let rec = ledger::SessionLedgerRecord::new(
            agent.clone(),
            "sess-led",
            Some("/tmp/ledger-cwd".to_string()),
        );
        ledger::append(&path, &rec).expect("append");
        let resolved = resolve_session(
            &agent,
            "sess-led",
            ContinuationTier::NativeResume,
            50,
            Some(&path),
        )
        .await;
        let _ = std::fs::remove_file(&path);
        assert_eq!(
            resolved.project,
            Some("/tmp/ledger-cwd".to_string()),
            "ledger cwd supplies the project even for an undiscovered agent"
        );
        assert!(resolved.ledger_hit, "a ledger cwd sets ledger_hit");
        assert!(
            resolved.transcript.is_none(),
            "undiscovered agent has no store → no transcript"
        );
    }

    #[tokio::test]
    async fn test_resolve_session_without_ledger_path_matches_existing_behavior() {
        // `Other(...)` is never discovered and ledger_path is None → the all-empty
        // ResolvedSession, byte-identical to the pre-ledger behavior.
        let agent = AgentKind::Other("no-ledger".to_string());
        let resolved =
            resolve_session(&agent, "whatever", ContinuationTier::Replay, 50, None).await;
        assert!(resolved.project.is_none());
        assert!(resolved.transcript.is_none());
        assert!(!resolved.ledger_hit);
    }
}
