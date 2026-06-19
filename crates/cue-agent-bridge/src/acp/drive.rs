//! ACP drive path — route a [`Question`] to an agent over the Agent Client
//! Protocol, returning the same [`AnswerStream`] the legacy CLI/cloud paths do.
//!
//! This is PHASE-2 plug-in point #2 from the [`crate::acp`] module docs: it
//! turns an [`AgentKind`] + [`Question`] into a live [`AcpClient`] drive. It is
//! deliberately opt-in (gated by `BLUEY_USE_ACP=1` in [`crate::drive`]) so the
//! migration is reversible and the default path is untouched.

use crate::acp::client::{AcpAgentSpec, AcpClient};
use crate::drive::{AnswerChunk, AnswerStream, Question};
use crate::AgentKind;
use futures_util::StreamExt;

/// Resolve the ACP entrypoint for an agent (the `AgentKind → AcpAgentSpec` map
/// in [`crate::acp::spec_map`]). `None` when the agent has no ACP entrypoint.
fn acp_spec_for(agent: &AgentKind) -> Option<AcpAgentSpec> {
    AcpAgentSpec::try_from(agent).ok()
}

/// True when `agent` has an ACP entrypoint (i.e. can be driven over ACP).
///
/// Used by the top-level dispatcher to decide routing without spawning anything;
/// a pure, side-effect-free predicate so the routing decision is unit-testable.
pub(crate) fn has_acp_spec(agent: &AgentKind) -> bool {
    acp_spec_for(agent).is_some()
}

/// Drive `agent` over ACP for one [`Question`], returning its streamed answer.
///
/// 1. Resolve the agent's [`AcpAgentSpec`]; if none, return `Err` (caller falls back).
/// 2. Build an [`AcpClient`] with the question's `cwd` (or the process cwd when `None`).
/// 3. Continue or open the session per [`Question::resume`] (see below).
///
/// ## Resume with automatic fork fallback
///
/// When [`Question::resume`] is `Some(id)` we attempt **true in-place resume**:
/// `session/load` restores the prior conversation from
/// `~/.claude/projects/<encoded-cwd>/<id>.jsonl` (the cwd the daemon set is part
/// of that path), so we send the **bare question** — the agent already has the
/// history. If that load FAILS (the adapter rejects `session/load`, or the CLI
/// subprocess dies before producing any answer — claude-agent-acp#338), we
/// transparently **fall back to fork**: open a fresh session and replay
/// [`Question::context`] as prompt context via [`Question::render_prompt`]. This
/// gives exact resume when it works and a non-destructive fork when it doesn't,
/// satisfying both continuation modes from one call. When `resume` is `None` we
/// just open a fresh session (folding any context for a plain fork).
///
/// Returns the [`AnswerStream`] eagerly; the ACP subprocess runs on a spawned
/// task inside the client and is torn down when the stream is dropped.
pub async fn drive_acp(agent: AgentKind, question: Question) -> anyhow::Result<AnswerStream> {
    let spec = acp_spec_for(&agent)
        .ok_or_else(|| anyhow::anyhow!("agent has no ACP entrypoint: {agent:?}"))?;

    // Own the cwd so the client factory can be moved into the fork closure below.
    let cwd = question.cwd.clone();
    let make_client = move || {
        let mut c = AcpClient::new(spec.clone());
        if let Some(cwd) = cwd.as_ref() {
            c = c.with_cwd(cwd);
        }
        c
    };

    let Some(session_id) = question.resume.clone() else {
        // Fresh ask (or a plain fork with no resume id): fold any context into the
        // prompt — `render_prompt()` returns just the prompt when there's none.
        return Ok(make_client().prompt(question.render_prompt()));
    };

    // Resume path. Send the BARE question (resume restores the history itself);
    // keep a fork prompt ready in case the load fails.
    let bare = question.prompt.clone();
    let fork_prompt = question.render_prompt();
    let resume_stream = make_client().resume(&session_id, bare);

    // The fork branch is built lazily so we only spawn a second subprocess when
    // the resume actually fails (the common case — resume succeeding — spawns one
    // subprocess, not two).
    let make_fork = move || make_client().prompt(fork_prompt);

    Ok(Box::pin(resume_with_fork_fallback(
        resume_stream,
        make_fork,
    )))
}

/// Wrap a resume stream so a `session/load` failure transparently retries as a
/// fork. We forward chunks as they arrive; the moment we see real answer content
/// (a [`AnswerChunk::Delta`]) the resume is committed and everything passes
/// through unchanged. But if the stream ends in [`AnswerChunk::Error`] **before**
/// any `Delta` (load rejected / subprocess died early), we discard the resume
/// output entirely and start the fork stream (`make_fork`) — the consumer sees a
/// single clean stream and never the failed attempt.
///
/// `make_fork` is a `FnOnce` so the fork subprocess is only spawned on actual
/// fallback; it also keeps this adapter pure enough to unit-test with a canned
/// fork stream (no real agent needed).
fn resume_with_fork_fallback<F>(
    resume_stream: AnswerStream,
    make_fork: F,
) -> impl futures_util::Stream<Item = AnswerChunk> + Send
where
    F: FnOnce() -> AnswerStream + Send,
{
    async_stream::stream! {
        let mut resume_stream = resume_stream;
        let mut produced_content = false;
        // Hold back `Started` until we know the resume is viable: if we fall back,
        // the fork emits its own `Started` with the fresh session id.
        let mut pending_started: Option<AnswerChunk> = None;

        while let Some(chunk) = resume_stream.next().await {
            match chunk {
                AnswerChunk::Started { .. } => {
                    pending_started = Some(chunk);
                }
                AnswerChunk::Delta(_) => {
                    if !produced_content {
                        tracing::info!(
                            "ACP resume (session/load) committed — true in-place resume"
                        );
                    }
                    produced_content = true;
                    if let Some(started) = pending_started.take() {
                        yield started;
                    }
                    yield chunk;
                }
                AnswerChunk::Done { .. } => {
                    // Clean finish. If the turn produced no content at all we still
                    // treat it as a (possibly empty) successful resume rather than
                    // forking — a `Done` is the agent's own signal it handled the
                    // turn. Emit the held `Started` so the session id reaches the
                    // caller, then the `Done`.
                    if let Some(started) = pending_started.take() {
                        yield started;
                    }
                    yield chunk;
                    return;
                }
                AnswerChunk::Error(ref e) if !produced_content => {
                    // Resume failed before any answer — fall back to fork. Drop the
                    // error + any held `Started`; replay against a fresh session.
                    tracing::warn!(
                        error = %e,
                        "ACP resume (session/load) failed before any answer — falling back to FORK"
                    );
                    let mut fork_stream = make_fork();
                    while let Some(c) = fork_stream.next().await {
                        yield c;
                    }
                    return;
                }
                AnswerChunk::Error(_) => {
                    // Error AFTER content: the resume was real and then failed
                    // mid-turn. Surface it — forking now would re-answer and double
                    // the output.
                    yield chunk;
                    return;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    #[test]
    fn has_acp_spec_true_for_native_agents_false_for_cloud() {
        assert!(has_acp_spec(&AgentKind::Gemini));
        assert!(has_acp_spec(&AgentKind::Cursor));
        assert!(has_acp_spec(&AgentKind::ClaudeCode));
        assert!(!has_acp_spec(&AgentKind::GeminiCloud));
        assert!(!has_acp_spec(&AgentKind::Aider));
        assert!(!has_acp_spec(&AgentKind::Unknown));
    }

    /// Build an [`AnswerStream`] from a fixed list of chunks (for testing the
    /// fork-fallback adapter without a real agent subprocess).
    fn canned(chunks: Vec<AnswerChunk>) -> AnswerStream {
        Box::pin(futures_util::stream::iter(chunks))
    }

    async fn collect(stream: impl futures_util::Stream<Item = AnswerChunk>) -> Vec<AnswerChunk> {
        futures_util::pin_mut!(stream);
        let mut out = Vec::new();
        while let Some(c) = stream.next().await {
            out.push(c);
        }
        out
    }

    #[tokio::test]
    async fn resume_with_content_passes_through_and_never_forks() {
        let forked = Arc::new(AtomicBool::new(false));
        let f = forked.clone();
        let resume = canned(vec![
            AnswerChunk::Started {
                session_id: Some("s1".into()),
            },
            AnswerChunk::Delta("hello".into()),
            AnswerChunk::Done { cost_usd: None },
        ]);
        let out = collect(resume_with_fork_fallback(resume, move || {
            f.store(true, Ordering::SeqCst);
            canned(vec![])
        }))
        .await;

        assert!(
            !forked.load(Ordering::SeqCst),
            "fork must NOT run when resume produced content"
        );
        assert_eq!(
            out,
            vec![
                AnswerChunk::Started {
                    session_id: Some("s1".into())
                },
                AnswerChunk::Delta("hello".into()),
                AnswerChunk::Done { cost_usd: None },
            ]
        );
    }

    #[tokio::test]
    async fn resume_error_before_content_falls_back_to_fork() {
        let resume = canned(vec![
            AnswerChunk::Started {
                session_id: Some("dead".into()),
            },
            AnswerChunk::Error("session/load rejected".into()),
        ]);
        // The fork stream carries the real answer; the failed resume's Started +
        // Error must be dropped so the consumer never sees them.
        let out = collect(resume_with_fork_fallback(resume, || {
            canned(vec![
                AnswerChunk::Started {
                    session_id: Some("fresh".into()),
                },
                AnswerChunk::Delta("forked answer".into()),
                AnswerChunk::Done { cost_usd: None },
            ])
        }))
        .await;

        assert_eq!(
            out,
            vec![
                AnswerChunk::Started {
                    session_id: Some("fresh".into())
                },
                AnswerChunk::Delta("forked answer".into()),
                AnswerChunk::Done { cost_usd: None },
            ],
            "consumer sees only the fork's clean stream, not the failed resume"
        );
    }

    #[tokio::test]
    async fn error_after_content_is_surfaced_not_forked() {
        let forked = Arc::new(AtomicBool::new(false));
        let f = forked.clone();
        let resume = canned(vec![
            AnswerChunk::Started { session_id: None },
            AnswerChunk::Delta("partial".into()),
            AnswerChunk::Error("died mid-turn".into()),
        ]);
        let out = collect(resume_with_fork_fallback(resume, move || {
            f.store(true, Ordering::SeqCst);
            canned(vec![])
        }))
        .await;

        assert!(
            !forked.load(Ordering::SeqCst),
            "forking after partial output would double the answer"
        );
        assert_eq!(
            out,
            vec![
                AnswerChunk::Started { session_id: None },
                AnswerChunk::Delta("partial".into()),
                AnswerChunk::Error("died mid-turn".into()),
            ]
        );
    }

    #[tokio::test]
    async fn clean_done_without_delta_does_not_fork() {
        // A turn that finishes cleanly with no answer text is still a successful
        // resume (the agent's own Done is the signal) — do NOT fork.
        let forked = Arc::new(AtomicBool::new(false));
        let f = forked.clone();
        let resume = canned(vec![
            AnswerChunk::Started {
                session_id: Some("s".into()),
            },
            AnswerChunk::Done { cost_usd: None },
        ]);
        let out = collect(resume_with_fork_fallback(resume, move || {
            f.store(true, Ordering::SeqCst);
            canned(vec![])
        }))
        .await;

        assert!(!forked.load(Ordering::SeqCst));
        assert_eq!(
            out,
            vec![
                AnswerChunk::Started {
                    session_id: Some("s".into())
                },
                AnswerChunk::Done { cost_usd: None },
            ]
        );
    }
}
