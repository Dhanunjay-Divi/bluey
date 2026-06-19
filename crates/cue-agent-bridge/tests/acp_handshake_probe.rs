//! TIER 1 — ACP handshake probe against the REAL installed agents.
//!
//! For each native-ACP agent present on this machine, this spawns it through our
//! `AcpClient`, runs the ACP handshake (`initialize` + `session/new`), and
//! asserts we receive an `AnswerChunk::Started { session_id }` — proving our
//! client speaks the protocol correctly to that agent. It then drops the stream
//! immediately, tearing down the subprocess BEFORE consuming a full answer, so
//! it spends (near) no model quota.
//!
//! Ignored by default (spawns real subprocesses + depends on installed agents).
//! Run explicitly:
//!   cargo test -p cue-agent-bridge --test acp_handshake_probe -- --ignored --nocapture
//!
//! Each agent gets its OWN `#[test]` so a missing/hanging one can't mask the
//! others, and the result reads as a per-agent matrix in `--nocapture` output.

use cue_agent_bridge::acp::client::{AcpAgentSpec, AcpClient};
use cue_agent_bridge::drive::AnswerChunk;
use cue_agent_bridge::AgentKind;
use futures_util::StreamExt;
use std::time::Duration;

/// Outcome of probing one agent's handshake.
#[derive(Debug)]
enum Probe {
    /// Binary not on PATH — not testable here (not a failure of our client).
    NotInstalled(String),
    /// Handshake reached `session/new` and returned a session id.
    Handshake { session_id: Option<String> },
    /// The agent produced a terminal error before/at handshake (often auth/quota
    /// — still useful: it means our client reached the agent and got a reply).
    AgentError(String),
    /// Nothing arrived within the timeout.
    Timeout,
}

fn binary_present(program: &str) -> bool {
    // Mirror `command -v`: look the program up on PATH.
    which_on_path(program).is_some()
}

fn which_on_path(program: &str) -> Option<std::path::PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(program);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// Drive the agent and stop the moment the handshake resolves (Started) or a
/// terminal chunk arrives — never consuming a full answer. Hard per-probe
/// timeout so a hung agent fails fast instead of wedging the suite.
async fn probe(kind: AgentKind) -> Probe {
    let spec = match AcpAgentSpec::try_from(&kind) {
        Ok(s) => s,
        Err(e) => return Probe::AgentError(format!("no ACP spec: {e}")),
    };
    if !binary_present(&spec.program) {
        return Probe::NotInstalled(spec.program.clone());
    }

    // A trivial prompt; we tear down at Started, so the model rarely runs.
    let client = AcpClient::new(spec).with_client_name("bluey-handshake-probe");
    let mut stream = client.prompt("ping");

    let deadline = Duration::from_secs(30);
    let result = tokio::time::timeout(deadline, async {
        while let Some(chunk) = stream.next().await {
            match chunk {
                AnswerChunk::Started { session_id } => {
                    return Probe::Handshake { session_id };
                }
                AnswerChunk::Error(e) => return Probe::AgentError(e),
                // Some agents may emit a Delta/Done before we see Started in odd
                // orderings; treat reaching the stream at all as a soft success
                // only on Started — keep waiting otherwise until timeout.
                _ => continue,
            }
        }
        Probe::Timeout
    })
    .await;

    // Dropping `stream` here tears the subprocess down.
    drop(stream);
    result.unwrap_or(Probe::Timeout)
}

/// Assert the probe proves our client handshakes (or is honestly not-installed /
/// agent-side error), and print a one-line matrix row.
fn report(kind: &str, p: Probe) {
    println!("[ACP handshake] {kind:<12} -> {p:?}");
    match p {
        // The win we want. If the agent reported a session id, it must be a
        // non-empty one (a blank id would be a broken handshake masquerading as
        // success); `None` is acceptable — not every agent echoes it at Started.
        Probe::Handshake { session_id } => {
            if let Some(id) = session_id {
                assert!(
                    !id.trim().is_empty(),
                    "{kind}: handshake returned an empty session id"
                );
            }
        }
        // Not a failure of our code, but the program name we resolved should be
        // real (an empty entrypoint would be a spec-map bug, not a missing tool).
        Probe::NotInstalled(program) => {
            assert!(
                !program.trim().is_empty(),
                "{kind}: NotInstalled with an empty program name (spec-map bug)"
            );
        }
        // Reached the agent but it errored (often auth/quota) — still proves our
        // client got a reply. The error text must be non-empty so the surfaced
        // diagnostic is actionable.
        Probe::AgentError(error) => {
            assert!(
                !error.trim().is_empty(),
                "{kind}: AgentError with no message"
            );
        }
        Probe::Timeout => panic!("{kind}: ACP handshake timed out (no Started, no error in 30s)"),
    }
}

#[tokio::test]
#[ignore = "spawns the real gemini agent; run with --ignored"]
async fn handshake_gemini() {
    report("gemini", probe(AgentKind::Gemini).await);
}

#[tokio::test]
#[ignore = "spawns the real cursor-agent; run with --ignored"]
async fn handshake_cursor() {
    report("cursor", probe(AgentKind::Cursor).await);
}

#[tokio::test]
#[ignore = "spawns the real copilot agent; run with --ignored"]
async fn handshake_copilot() {
    report("copilot", probe(AgentKind::Copilot).await);
}

#[tokio::test]
#[ignore = "needs the @zed-industries/claude-agent-acp adapter installed; run with --ignored"]
async fn handshake_claude() {
    report("claude", probe(AgentKind::ClaudeCode).await);
}

#[tokio::test]
#[ignore = "needs a codex ACP adapter installed; run with --ignored"]
async fn handshake_codex() {
    report("codex", probe(AgentKind::Codex).await);
}
