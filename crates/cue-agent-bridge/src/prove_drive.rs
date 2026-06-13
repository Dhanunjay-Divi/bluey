//! **Real** drive proof — the test that actually answers "does this agent work?"
//!
//! [`prove`](crate::prove) is read-only: it proves an agent is *present and
//! could* be driven, but never spawns it (driving costs money / hits quota). It
//! answers "is the door there?" — not "does anyone answer when I knock."
//!
//! This module knocks. For each agent that is discoverable and drivable on this
//! machine, it sends a real canary question through the **production** drive
//! path ([`crate::drive`]) — the exact code the daemon uses — consumes the real
//! streamed answer, and reports the honest outcome:
//!
//! - **Answered** — a non-empty answer came back. If it contains the canary
//!   token we asked for, that's a clean pass; otherwise it answered but didn't
//!   follow the instruction (still "alive", flagged).
//! - **Failed** — the agent returned a terminal error (not signed in, rate
//!   limited, context too long, …). The *real* error text is preserved.
//! - **Skipped** — not drivable on this machine (CLI/credential missing), with
//!   the reason. Cloud vendors with no stored credential land here.
//!
//! This is **consent-gated and explicit** — it is never part of the read-only
//! `prove_all()`. It runs only when the user asks (`bluey agent prove --drive`),
//! because every "Answered" line cost a real model call against the user's
//! account.
//!
//! It is the opposite of a mock: there is no fixture, no simulated response, no
//! asserted request shape. It drives the real agent and reports what really
//! happened.

use std::time::{Duration, Instant};

use futures_util::StreamExt;

use crate::{discover_agents, registry, AgentKind, AnswerChunk, Question};

/// The canary we ask every agent to echo. Distinctive enough that a coincidental
/// match is implausible; short enough that a well-behaved agent returns exactly
/// it. We check `contains`, not equality, because some agents wrap or prefix.
pub const CANARY: &str = "LIVEPROOF7";

/// The exact prompt sent to each agent. Phrased to elicit the canary alone.
pub const PROVE_PROMPT: &str = "Reply with exactly this single word and nothing else: LIVEPROOF7";

/// Per-agent wall-clock cap for one drive. A live agent answers a one-word
/// prompt well within this; past it we record a timeout rather than hang.
const DRIVE_TIMEOUT: Duration = Duration::from_secs(60);

/// The honest outcome of one real drive attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DriveProof {
    /// The agent answered. `echoed_canary` is true when the answer contained
    /// [`CANARY`] (clean pass); false means it answered but ignored the
    /// instruction (alive, but flagged). `answer` is the (truncated) real text.
    Answered {
        echoed_canary: bool,
        answer: String,
        elapsed_ms: u64,
    },
    /// The agent produced a terminal error. `reason` is the REAL error text from
    /// the agent/CLI — never a hardcoded guess.
    Failed { reason: String, elapsed_ms: u64 },
    /// Not driven on this machine (no CLI / no credential / not drivable), with
    /// the concrete reason. No model call was made.
    Skipped { reason: String },
}

impl DriveProof {
    pub fn marker(&self) -> &'static str {
        match self {
            DriveProof::Answered {
                echoed_canary: true,
                ..
            } => "ANSWERED ✅",
            DriveProof::Answered {
                echoed_canary: false,
                ..
            } => "ANSWERED (off-canary) 🟡",
            DriveProof::Failed { .. } => "FAILED 🔴",
            DriveProof::Skipped { .. } => "skip ⬜",
        }
    }
}

/// One agent's real-drive result.
#[derive(Debug, Clone)]
pub struct AgentDriveProof {
    pub display_name: &'static str,
    pub kind: AgentKind,
    pub result: DriveProof,
}

/// Drive every discoverable agent once with the canary prompt and report the
/// real outcome. **Spawns real agents and spends real quota** — call only on
/// explicit user request.
///
/// `include_cloud` controls whether cloud vendors are attempted (they need a
/// stored credential; without one they `Skip`). Local agents are always tried
/// when discoverable.
pub async fn prove_drive_all(include_cloud: bool) -> Vec<AgentDriveProof> {
    let discovered = discover_agents();
    let mut out = Vec::new();

    for entry in registry::REGISTRY {
        let kind = entry.kind_tag.to_agent_kind();
        let is_cloud = crate::cloud::cloud_entry_for(entry.kind_tag).is_some();

        if is_cloud && !include_cloud {
            out.push(AgentDriveProof {
                display_name: entry.display_name,
                kind: kind.clone(),
                result: DriveProof::Skipped {
                    reason: "cloud vendor (pass --cloud to attempt; needs stored credential)"
                        .to_string(),
                },
            });
            continue;
        }

        // A local agent is only drivable if discovery found a drivable footprint
        // (a CLI binary). Cloud agents are "discovered" by credential presence,
        // checked inside the drive path; here we gate locals on discovery.
        if !is_cloud {
            let found = discovered.iter().any(|d| d.kind == kind);
            if !found {
                out.push(AgentDriveProof {
                    display_name: entry.display_name,
                    kind: kind.clone(),
                    result: DriveProof::Skipped {
                        reason: "not installed on this machine".to_string(),
                    },
                });
                continue;
            }
        }

        let result = drive_once(kind.clone()).await;
        out.push(AgentDriveProof {
            display_name: entry.display_name,
            kind,
            result,
        });
    }

    out
}

/// Drive a single agent kind once with the canary, bounded by [`DRIVE_TIMEOUT`].
/// Consumes the real [`AnswerChunk`] stream from the production [`crate::drive`].
pub async fn drive_once(kind: AgentKind) -> DriveProof {
    let started = Instant::now();
    let question = Question::new(PROVE_PROMPT);

    // Spawn the real drive. A spawn failure (missing binary / no credential) is a
    // skip-shaped outcome surfaced as Failed with the real reason.
    let stream = match crate::drive(kind, question).await {
        Ok(s) => s,
        Err(e) => {
            return DriveProof::Failed {
                reason: format!("could not start: {e:#}"),
                elapsed_ms: started.elapsed().as_millis() as u64,
            };
        }
    };

    futures_util::pin_mut!(stream);
    let mut body = String::new();

    loop {
        let remaining = DRIVE_TIMEOUT.checked_sub(started.elapsed());
        let Some(remaining) = remaining else {
            return DriveProof::Failed {
                reason: format!(
                    "timed out after {}s with no terminal event",
                    DRIVE_TIMEOUT.as_secs()
                ),
                elapsed_ms: started.elapsed().as_millis() as u64,
            };
        };

        match tokio::time::timeout(remaining, stream.next()).await {
            Err(_) => {
                return DriveProof::Failed {
                    reason: format!("timed out after {}s", DRIVE_TIMEOUT.as_secs()),
                    elapsed_ms: started.elapsed().as_millis() as u64,
                };
            }
            Ok(None) => break, // stream ended
            Ok(Some(chunk)) => match chunk {
                AnswerChunk::Started { .. } => {}
                AnswerChunk::Delta(d) => body.push_str(&d),
                AnswerChunk::Done { .. } => break,
                AnswerChunk::Error(message) => {
                    return DriveProof::Failed {
                        reason: message,
                        elapsed_ms: started.elapsed().as_millis() as u64,
                    };
                }
            },
        }
    }

    let elapsed_ms = started.elapsed().as_millis() as u64;
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return DriveProof::Failed {
            reason: "returned no answer (empty stream)".to_string(),
            elapsed_ms,
        };
    }

    DriveProof::Answered {
        echoed_canary: trimmed.contains(CANARY),
        answer: truncate(trimmed, 120),
        elapsed_ms,
    }
}

/// Render the real-drive matrix as a human-readable report.
pub fn render_report(proofs: &[AgentDriveProof]) -> String {
    let mut s = String::new();
    s.push_str("REAL DRIVE PROOF — actually asked each agent the canary question.\n");
    s.push_str("(Every ANSWERED line was a real model call against your account.)\n\n");
    for p in proofs {
        s.push_str(&format!("{:<24} {}\n", p.display_name, p.result.marker()));
        match &p.result {
            DriveProof::Answered {
                answer, elapsed_ms, ..
            } => {
                s.push_str(&format!("    answer: {answer:?}  ({elapsed_ms} ms)\n"));
            }
            DriveProof::Failed { reason, elapsed_ms } => {
                s.push_str(&format!("    error:  {reason}  ({elapsed_ms} ms)\n"));
            }
            DriveProof::Skipped { reason } => {
                s.push_str(&format!("    {reason}\n"));
            }
        }
    }
    s
}

/// Truncate to a single line of at most `max` chars (char-boundary safe).
fn truncate(text: &str, max: usize) -> String {
    let one_line: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if one_line.chars().count() <= max {
        one_line
    } else {
        let mut t: String = one_line.chars().take(max).collect();
        t.push('…');
        t
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canary_is_distinctive_and_in_prompt() {
        // The prompt must actually ask for the canary, or a pass is meaningless.
        assert!(PROVE_PROMPT.contains(CANARY));
        // Distinctive: not a real English word an agent would emit by chance.
        assert!(CANARY.chars().any(|c| c.is_ascii_digit()));
    }

    #[test]
    fn markers_distinguish_clean_pass_from_off_canary() {
        let clean = DriveProof::Answered {
            echoed_canary: true,
            answer: "LIVEPROOF7".into(),
            elapsed_ms: 10,
        };
        let off = DriveProof::Answered {
            echoed_canary: false,
            answer: "sure, here you go".into(),
            elapsed_ms: 10,
        };
        assert_eq!(clean.marker(), "ANSWERED ✅");
        assert_eq!(off.marker(), "ANSWERED (off-canary) 🟡");
        assert_ne!(clean.marker(), off.marker());
    }

    #[test]
    fn render_includes_real_error_text_not_a_guess() {
        let proofs = vec![AgentDriveProof {
            display_name: "Codex",
            kind: AgentKind::Codex,
            result: DriveProof::Failed {
                reason: "401 Unauthorized: refresh token already used".into(),
                elapsed_ms: 800,
            },
        }];
        let report = render_report(&proofs);
        // The REAL error must appear verbatim — never a hardcoded "not signed in".
        assert!(report.contains("401 Unauthorized: refresh token already used"));
        assert!(report.contains("FAILED 🔴"));
    }

    #[test]
    fn truncate_collapses_and_caps() {
        assert_eq!(truncate("  a   b ", 80), "a b");
        let long = "x".repeat(200);
        assert!(truncate(&long, 120).ends_with('…'));
    }
}
