//! Continuation-tier policy: turn a pinned prior session into a ready-to-drive
//! [`Question`], per the agent's [`ContinuationTier`].
//!
//! This is the decision the registry row drives:
//!
//! - **NativeResume** (Claude/Codex/Copilot) — the agent can reopen a session by
//!   id. Over ACP we attempt TRUE in-place resume (`session/load`, cwd = the
//!   session's project dir, which is part of the on-disk path) and keep the
//!   transcript as a fork fallback; off ACP we set `--resume` + cwd. If the
//!   project cwd is gone we degrade to replay.
//! - **Replay** (Cursor/VS Code/Gemini/Antigravity) — no resume-by-id, so load
//!   the transcript and replay it as `context` (compacted if huge).
//!
//! The transcript read is bounded and read-only ([`resolve_session`]); the
//! compaction is injected as a closure so this stays decoupled from the daemon's
//! drive machinery (Bluey runs no AI of its own — the user's agent summarizes).

use std::future::Future;

use crate::registry::{entry_for, ContinuationTier, KindTag};
use crate::sessions::reader_for;
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

/// Find a session's project path, plus its full transcript (bounded). Read-only,
/// fail-soft → `(None, None)` when the agent / store / session isn't found.
///
/// `list_cap` bounds how many session refs are scanned to recover the project
/// path (the daemon supplies its own list cap so policy stays in one place).
/// `tier` is accepted for call-site clarity but both tiers load the transcript:
/// for Replay it's the context; for NativeResume it's the fork-fallback safety
/// net used only if the project cwd turns out unusable.
pub async fn resolve_session(
    agent: &AgentKind,
    session_id: &str,
    tier: ContinuationTier,
    list_cap: usize,
) -> (Option<String>, Option<Transcript>) {
    let Some(discovered) = discover_agents().into_iter().find(|d| &d.kind == agent) else {
        return (None, None);
    };
    let Some(store) = discovered.session_store.as_ref() else {
        return (None, None);
    };
    let reader = reader_for(store.format);

    // Project: from the session's SessionRef (the readers populate it).
    let project = reader.list(store, list_cap).ok().and_then(|refs| {
        refs.into_iter()
            .find(|r| r.id == session_id)
            .and_then(|r| r.project)
    });

    // Transcript: always needed for Replay; for NativeResume it's the
    // fork-fallback safety net (used only if the project cwd is unusable). Load
    // it for both — a bounded read, far cheaper than the drive itself, and
    // `apply_tier` only attaches it where actually needed.
    let _ = tier; // both tiers load it now; kept for call-site clarity
    let transcript = reader
        .read(store, session_id, CONTINUATION_READ_MAX_TURNS)
        .ok();

    (project, transcript)
}

/// Upgrade `question` to **continue a specific prior session**, per the agent's
/// [`ContinuationTier`]. A no-op when `session_id` is empty/blank.
///
/// `via_acp` selects the true-resume-with-fork-fallback path for NativeResume
/// agents (see the module docs). `list_cap` bounds the project-path lookup.
/// `summarize` is the injected compaction summarizer (driven through the user's
/// own agent); it is only invoked when a transcript is large enough to need it
/// (see [`super::maybe_compact`]).
pub async fn apply_tier<F, Fut>(
    question: &mut Question,
    agent: &AgentKind,
    session_id: Option<&str>,
    via_acp: bool,
    list_cap: usize,
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

    // Resolve the session's project (cwd) and — for Replay — its transcript.
    let (project, transcript) =
        resolve_session(agent, session_id, entry.continuation, list_cap).await;

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

    match entry.continuation {
        // ACP path. The on-disk session id IS the SDK's resume key — `session/load`
        // resolves `~/.claude/projects/<encoded-cwd>/<id>.jsonl` and the CWD is part
        // of that path (platform.claude.com/docs/en/agent-sdk/sessions). So:
        //   - cwd usable  → attempt TRUE resume (set `resume` + the session's project
        //     cwd) AND keep the transcript in `context` as a fork fallback. The ACP
        //     drive tries `session/load` first; if it yields no prior context it
        //     retries as fresh + replayed context (fork). Best of both: exact
        //     in-place resume when it works, non-destructive fork when it doesn't.
        //   - cwd missing → resume can't resolve the path (would silently start
        //     fresh), so go straight to fork: drop `resume`, replay the transcript.
        ContinuationTier::NativeResume if via_acp => {
            if cwd_usable {
                tracing::info!(
                    session = %session_id,
                    cwd = project.as_deref().unwrap_or(""),
                    "ACP continuation: TRUE resume (session/load) with fork fallback"
                );
                question.cwd = project;
                question.resume = Some(session_id.to_string());
            } else {
                question.resume = None;
                tracing::info!(
                    session = %session_id,
                    "ACP continuation: cwd unusable → FORK (replay context, no resume)"
                );
            }
            if let Some(t) = transcript {
                question.context = Some(super::maybe_compact(t, &summarize).await);
            }
        }
        ContinuationTier::NativeResume if cwd_usable => {
            // Non-ACP native resume: drive in the project dir, resume by id, let
            // the vendor handle context compaction.
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
}
