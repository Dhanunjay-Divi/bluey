//! Cloud drive entry — turn a [`Question`] for a cloud-shaped agent into an
//! [`AnswerStream`] without ever naming a vendor in this file.
//!
//! The shared transport (`cloud/transport.rs`) does the actual HTTP work; the
//! per-vendor adapter (`cloud/<vendor>.rs`) carries the irreducible request-
//! body construction and response-parsing. This file is the *dispatcher*: it
//! reads [`cloud::registry::CLOUD_REGISTRY`] for the kind, calls the matching
//! adapter to build the request, fires the transport, and turns the result
//! into a stream the existing daemon answer-ladder consumes.
//!
//! ### Task-shaped vs synchronous
//!
//! Cloud vendors are mostly task-shaped (minutes-long, returns a PR). For
//! the *synchronous* answer ladder Bluey's overlay assumes ("answer in
//! seconds"), task-shaped vendors emit a single acknowledgement chunk:
//!
//! > Kicked off `<vendor>` task `<id>` in `<owner>/<repo>`. PR will land at
//! > <url> in ~5–15 minutes. I'll notify you when it's ready.
//!
//! …then the stream completes. The daemon parks the task id in its pending-
//! task map (future work — see dossier OPEN section); polling + push
//! notification when the task completes is the next milestone.
//!
//! ### Honest errors, never silent
//!
//! If the keychain is empty, the credential is shaped wrong, or the
//! task-create call errors, we emit a terminal [`AnswerChunk::Error`] with
//! the truth from the vendor (truncated to one line). The daemon's existing
//! "agent not ready" guidance card surfaces it.

use anyhow::{anyhow, Result};
use async_stream::stream;

use crate::cloud::keychain::{KeychainCredentialStore, VendorCredentialStore};
use crate::cloud::registry::{cloud_entry_for, CloudAgentEntry};
use crate::cloud::transport::{CloudAuth, CloudHttpsTransport, HttpRequest, BEARER_AUTH};
use crate::drive::{AnswerChunk, AnswerStream, Question};
use crate::registry::KindTag;
use crate::AgentKind;

/// Per-vendor task-creation payload, normalized.
///
/// The adapter (e.g. `cloud::copilot`) consumes this + its own knowledge of
/// JSON shape to build the actual HTTP request. We keep the layer thin so a
/// future vendor can plug in without changing this file.
#[derive(Debug, Clone, Default)]
pub struct CloudTaskInputs {
    pub prompt: String,
    /// Repository owner — used by GitHub-style vendors (Copilot Cloud, which
    /// builds paths like `/repos/{owner}/{repo}/…`). `None` when not
    /// applicable.
    pub owner: Option<String>,
    pub repo: Option<String>,
    /// Repository URL — used by URL-style vendors (Cursor Cloud, which puts
    /// the full URL into the `repos[0].url` body field). `None` when not
    /// applicable.
    pub repo_url: Option<String>,
    pub base_ref: Option<String>,
}

impl CloudTaskInputs {
    /// Build from a [`Question`]. Owner/repo and `repo_url` aren't carried on
    /// the question today — they live on a separate per-call extension. For
    /// now both shapes return `None`; the daemon supplies them at the call
    /// site (or the adapter surfaces a clean "no repo selected" error).
    pub fn from_question(q: &Question) -> Self {
        Self {
            prompt: q.prompt.clone(),
            owner: None,
            repo: None,
            repo_url: None,
            base_ref: None,
        }
    }
}

/// Whether an agent kind has a cloud registry row — and therefore should be
/// dispatched through [`drive_cloud`] rather than the local-CLI
/// [`crate::drive::drive`]. Pure lookup; data-driven, no per-vendor branch.
pub fn is_cloud_kind(kind: &AgentKind) -> bool {
    KindTag::from_agent_kind(kind)
        .and_then(cloud_entry_for)
        .is_some()
}

/// Drive a cloud-shaped agent for a [`Question`].
///
/// Returns `Err` only for "this kind isn't a cloud vendor" (the generic
/// dispatcher's caller should fall back to the CLI drive). Every runtime
/// failure (no token, HTTP error, missing repo) flows out as a terminal
/// [`AnswerChunk::Error`] on the returned stream, matching the local-CLI
/// `drive` contract.
pub async fn drive_cloud(agent: AgentKind, question: Question) -> Result<AnswerStream> {
    let tag = KindTag::from_agent_kind(&agent)
        .ok_or_else(|| anyhow!("agent {agent:?} has no registry tag"))?;
    let entry =
        cloud_entry_for(tag).ok_or_else(|| anyhow!("agent {agent:?} is not a cloud vendor"))?;

    // Pick the drive shape off the registry row — pure data, never a vendor
    // name. Turn-shaped vendors (the Gemini Interactions API) answer
    // synchronously and stream `Started → Delta → Done` like a local CLI
    // agent; task-shaped vendors (Copilot/Cursor/Codex Cloud) kick off a
    // minutes-long job and emit a single acknowledgement card.
    if entry.task_shaped {
        Ok(spawn_task_stream(entry, agent, question))
    } else {
        Ok(spawn_turn_stream(entry, agent, question))
    }
}

/// Produce the stream for a task-shaped cloud agent. Always returns a
/// non-empty stream: either a Started + Delta + Done sequence, or a single
/// terminal Error chunk explaining what went wrong.
fn spawn_task_stream(
    entry: &'static CloudAgentEntry,
    agent: AgentKind,
    question: Question,
) -> AnswerStream {
    let vendor = entry.vendor_short;
    let inputs = CloudTaskInputs::from_question(&question);

    let s = stream! {
        if !entry.task_shaped {
            yield AnswerChunk::Error(format!(
                "{vendor}: session-shaped cloud drive is not implemented yet"
            ));
            return;
        }

        // Resolve credential. Production code wires
        // [`KeychainCredentialStore`]; the daemon may inject a memory store
        // for tests. We hold the credential just long enough to hand to the
        // transport.
        let creds = KeychainCredentialStore::new(vendor);
        let token = match creds.load("api_key") {
            Ok(Some(t)) => t,
            Ok(None) => {
                yield AnswerChunk::Error(format!(
                    "no {vendor} credential found in keychain. \
                     Run `bluey agent connect {vendor}` to enroll."
                ));
                return;
            }
            Err(e) => {
                yield AnswerChunk::Error(format!(
                    "{vendor}: reading keychain failed: {e}"
                ));
                return;
            }
        };

        // Per-vendor pre-flight (token shape check). Each adapter exposes
        // its own pre-flight; we look it up generically by kind below
        // (currently inline for Copilot — the other adapters can hook in
        // by adding a `pre_flight_check` function alongside their `ENTRY`).
        if matches!(agent, AgentKind::CopilotCloud) {
            if let Err(e) = crate::cloud::copilot::reject_server_to_server_token(&token) {
                yield AnswerChunk::Error(format!("{vendor}: {e}"));
                return;
            }
        }

        // Build the request. Per-vendor builders live in the adapter; the
        // dispatch picks via the AgentKind only as the index into the table.
        // Repo-shape validation is per-vendor too: GitHub-style vendors need
        // `owner + repo`, URL-style vendors (Cursor) need `repo_url`. Each
        // arm validates its own shape and surfaces an honest error if the
        // daemon hasn't supplied what it needs.
        let (path, body, owner_repo_for_ack) = match agent {
            AgentKind::CopilotCloud => {
                let (owner, repo) = match (inputs.owner.as_deref(), inputs.repo.as_deref()) {
                    (Some(o), Some(r)) if !o.is_empty() && !r.is_empty() => {
                        (o.to_string(), r.to_string())
                    }
                    _ => {
                        yield AnswerChunk::Error(format!(
                            "{vendor}: this vendor is task-shaped and needs an explicit \
                             (owner, repo). Bluey's repo-picker UI is the gating piece \
                             here — until it lands, kick-off is refused."
                        ));
                        return;
                    }
                };
                let req = crate::cloud::copilot::CreateTaskRequest {
                    prompt: inputs.prompt.clone(),
                    base_ref: inputs.base_ref.clone(),
                    model: None,
                };
                let path = crate::cloud::copilot::create_task_path(&owner, &repo);
                (path, req.to_json(), Some((owner, repo)))
            }
            AgentKind::CursorCloud => {
                // Cursor Cloud takes the repo URL directly (the docs' example
                // body is `repos: [{ url, startingRef }]`). The starting ref
                // defaults to `main`; future work surfaces this in the UI.
                let repo_url = match inputs.repo_url.as_deref() {
                    Some(u) if !u.is_empty() => u.to_string(),
                    _ => {
                        yield AnswerChunk::Error(format!(
                            "{vendor}: this vendor is task-shaped and needs an explicit \
                             repo URL. Bluey's repo-picker UI is the gating piece here \
                             — until it lands, kick-off is refused."
                        ));
                        return;
                    }
                };
                let starting_ref = inputs.base_ref.as_deref().unwrap_or("main");
                // composer-2 is the documented default model id; future work
                // surfaces model selection in the UI / settings.
                let body = crate::cloud::cursor::build_create_agent_body(
                    &inputs.prompt,
                    &repo_url,
                    starting_ref,
                    "composer-2",
                );
                let path = "/v1/agents".to_string();
                (path, body, None)
            }
            _ => {
                yield AnswerChunk::Error(format!(
                    "{vendor}: cloud drive dispatch not yet implemented for this vendor"
                ));
                return;
            }
        };

        let url = format!("{}{}", entry.base_url, path);
        let mut http_req = HttpRequest::post_json(url, body);
        // Layer the per-vendor static headers (e.g. GitHub's
        // `X-GitHub-Api-Version`). These come off the adapter, never inline.
        if matches!(agent, AgentKind::CopilotCloud) {
            for (name, value) in crate::cloud::copilot::STATIC_HEADERS {
                http_req = http_req.with_header(*name, *value);
            }
        }

        let transport = CloudHttpsTransport::new();

        // The kick-off is a Started + Delta + Done sequence. We do not emit
        // a partial Started until we know the task id (the vendor decides).
        let response = match transport
            .send(vendor, "create_task", &http_req, BEARER_AUTH, &creds, "api_key")
            .await
        {
            Ok(r) => r,
            Err(e) => {
                yield AnswerChunk::Error(format!("{vendor}: kick-off failed: {e}"));
                return;
            }
        };

        // Drop the local `_` shadow of CloudAuth so clippy is satisfied.
        let _ = CloudAuth::BasicApiKey;

        if !(200..300).contains(&response.status) {
            // Surface the real error body (truncated) — never a guessed
            // generic message.
            let body_snip = String::from_utf8_lossy(&response.body)
                .chars()
                .take(200)
                .collect::<String>();
            yield AnswerChunk::Error(format!(
                "{vendor}: HTTP {} from create-task: {body_snip}",
                response.status
            ));
            return;
        }

        // Per-vendor parse + acknowledgement. Each arm yields the same
        // (task_id, ack_text) shape so the emit code below stays generic.
        // Per-vendor handle structs (Copilot's CloudTaskHandle, Cursor's
        // CreatedAgentRun) stay private to the adapter — the dispatcher only
        // sees the dispatcher-shaped tuple.
        let (task_id, ack) = match agent {
            AgentKind::CopilotCloud => {
                let handle = match crate::cloud::copilot::parse_task_response(&response.body) {
                    Ok(h) => h,
                    Err(e) => {
                        yield AnswerChunk::Error(format!(
                            "{vendor}: response parse failed: {e}"
                        ));
                        return;
                    }
                };
                let (owner, repo) = owner_repo_for_ack
                    .as_ref()
                    .expect("CopilotCloud arm always sets owner_repo_for_ack");
                let ack = crate::cloud::copilot::render_acknowledgement(owner, repo, &handle);
                (handle.id, ack)
            }
            AgentKind::CursorCloud => {
                match crate::cloud::cursor::parse_task_response_for_dispatch(&response.body) {
                    Ok((id, _url, ack)) => (id, ack),
                    Err(e) => {
                        yield AnswerChunk::Error(format!(
                            "{vendor}: response parse failed: {e}"
                        ));
                        return;
                    }
                }
            }
            _ => {
                yield AnswerChunk::Error(format!(
                    "{vendor}: no response parser registered for this vendor"
                ));
                return;
            }
        };

        // Emit Started (with the task id as the "session id"), one Delta
        // (the acknowledgement text), and Done. The daemon's existing
        // streaming consumer treats this as a fast synchronous answer.
        yield AnswerChunk::Started {
            session_id: Some(task_id),
        };
        yield AnswerChunk::Delta(ack);
        yield AnswerChunk::Done { cost_usd: None };
    };

    Box::pin(s)
}

/// Produce the stream for a **turn-shaped** cloud agent — one that answers
/// synchronously (the Gemini Interactions API). Always returns a non-empty
/// stream: either a `Started → Delta → Done` sequence (the answer text), or a
/// single terminal `Error` chunk explaining what went wrong. This is the same
/// overlay contract a local CLI agent uses; there is NO task-acknowledgement
/// card (that's the task-shaped path above).
///
/// Per-vendor request construction + response parsing live in the adapter; the
/// `match agent` here is purely an index into the table (the dispatcher is the
/// single allowed place for that wiring, exactly as in `spawn_task_stream`).
fn spawn_turn_stream(
    entry: &'static CloudAgentEntry,
    agent: AgentKind,
    question: Question,
) -> AnswerStream {
    let vendor = entry.vendor_short;
    let inputs = CloudTaskInputs::from_question(&question);

    let s = stream! {
        // Resolve credential from the OS keychain (production) — the same
        // pattern the task path uses. The credential is held only long enough
        // to hand to the transport's `send`.
        let creds = KeychainCredentialStore::new(vendor);
        match creds.load("api_key") {
            Ok(Some(_)) => {}
            Ok(None) => {
                yield AnswerChunk::Error(format!(
                    "no {vendor} credential found in keychain. \
                     Run `bluey agent connect {vendor}` to enroll."
                ));
                return;
            }
            Err(e) => {
                yield AnswerChunk::Error(format!("{vendor}: reading keychain failed: {e}"));
                return;
            }
        }

        // Build the synchronous request, the per-call auth, and the static
        // headers (e.g. Gemini's `Api-Revision`) off the adapter — never inline.
        let (http_req, auth) = match agent {
            AgentKind::GeminiCloud => {
                let mut req = crate::cloud::gemini_cloud::interaction_request(&inputs.prompt);
                for (name, value) in crate::cloud::gemini_cloud::api_revision_headers() {
                    req = req.with_header(*name, *value);
                }
                (req, crate::cloud::gemini_cloud::GEMINI_AUTH)
            }
            AgentKind::AntigravityCloud => {
                // Same Gemini Interactions endpoint as GeminiCloud above — the
                // request differs ONLY in the `agent` field (the Antigravity
                // agent id), which the adapter's builder bakes in. The
                // `Api-Revision` header is layered the same way.
                let mut req = crate::cloud::antigravity_cloud::default_answer_request(&inputs.prompt);
                for (name, value) in crate::cloud::antigravity_cloud::api_revision_headers() {
                    req = req.with_header(*name, *value);
                }
                (req, crate::cloud::antigravity_cloud::ANTIGRAVITY_AUTH)
            }
            _ => {
                yield AnswerChunk::Error(format!(
                    "{vendor}: turn-shaped cloud drive not yet implemented for this vendor"
                ));
                return;
            }
        };

        let transport = CloudHttpsTransport::new();
        let response = match transport
            .send(vendor, "interaction", &http_req, auth, &creds, "api_key")
            .await
        {
            Ok(r) => r,
            Err(e) => {
                yield AnswerChunk::Error(format!("{vendor}: request failed: {e}"));
                return;
            }
        };

        if !(200..300).contains(&response.status) {
            // Per-vendor error rendering — honest guidance + the vendor's
            // verbatim message, never a guessed generic string, never the token.
            let msg = match agent {
                AgentKind::GeminiCloud => {
                    crate::cloud::gemini_cloud::render_error_message(&response)
                }
                AgentKind::AntigravityCloud => {
                    crate::cloud::antigravity_cloud::render_error_message(&response)
                }
                _ => format!(
                    "{vendor}: HTTP {} from interaction",
                    response.status
                ),
            };
            yield AnswerChunk::Error(msg);
            return;
        }

        // Parse the synchronous answer (session id + text) per-vendor.
        let (session_id, text) = match agent {
            AgentKind::GeminiCloud => {
                match crate::cloud::gemini_cloud::parse_answer_for_dispatch(&response.body) {
                    Ok(pair) => pair,
                    Err(e) => {
                        yield AnswerChunk::Error(format!("{vendor}: response parse failed: {e}"));
                        return;
                    }
                }
            }
            AgentKind::AntigravityCloud => {
                match crate::cloud::antigravity_cloud::parse_answer_for_dispatch(&response.body) {
                    Ok(pair) => pair,
                    Err(e) => {
                        yield AnswerChunk::Error(format!("{vendor}: response parse failed: {e}"));
                        return;
                    }
                }
            }
            _ => {
                yield AnswerChunk::Error(format!(
                    "{vendor}: no response parser registered for this vendor"
                ));
                return;
            }
        };

        // Emit the answer as a normal synchronous turn: Started (carrying the
        // interaction id), the answer text, then Done.
        yield AnswerChunk::Started { session_id };
        yield AnswerChunk::Delta(text);
        yield AnswerChunk::Done { cost_usd: None };
    };

    Box::pin(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn drive_cloud_rejects_non_cloud_kind() {
        // A local-CLI kind (no cloud row) returns Err synchronously — no
        // stream produced. This lets the caller fall back to the CLI drive.
        // `AnswerStream` is `!Debug`, so unwrap_err panics; use a match.
        let result = drive_cloud(AgentKind::ClaudeCode, Question::new("hi")).await;
        let err = match result {
            Ok(_) => panic!("expected Err for non-cloud kind"),
            Err(e) => e.to_string(),
        };
        assert!(err.contains("not a cloud vendor") || err.contains("registry tag"));
    }

    #[tokio::test]
    async fn drive_cloud_copilot_without_owner_returns_terminal_error() {
        use futures_util::StreamExt;

        // No owner/repo provided in the question (we don't have a repo
        // picker yet). Should produce ONE terminal Error chunk explaining
        // the gap — not panic, not stall, not silently degrade.
        let stream = drive_cloud(AgentKind::CopilotCloud, Question::new("Fix bug"))
            .await
            .expect("dispatch ok");
        futures_util::pin_mut!(stream);
        let first = stream.next().await.expect("at least one chunk");
        match first {
            AnswerChunk::Error(msg) => {
                assert!(msg.contains("copilot_cloud"));
                // The message must name the gap honestly.
                assert!(
                    msg.contains("owner") || msg.contains("repo") || msg.contains("keychain"),
                    "unexpected error message: {msg}"
                );
            }
            other => panic!("expected Error chunk, got {other:?}"),
        }
        // No further chunks after the terminal Error.
        assert!(stream.next().await.is_none());
    }

    #[tokio::test]
    async fn drive_cloud_cursor_without_repo_url_returns_terminal_error() {
        use futures_util::StreamExt;

        // Cursor Cloud requires `repos[0].url` — the dispatch must surface
        // a clear, honest error when the daemon hasn't supplied it (e.g.
        // the user hasn't connected a repo yet).
        let stream = drive_cloud(AgentKind::CursorCloud, Question::new("Add README"))
            .await
            .expect("dispatch ok");
        futures_util::pin_mut!(stream);
        let first = stream.next().await.expect("at least one chunk");
        match first {
            AnswerChunk::Error(msg) => {
                assert!(msg.contains("cursor_cloud"));
                // The message must mention either the repo URL gap or the
                // keychain (depending on which check fails first).
                assert!(
                    msg.contains("repo URL")
                        || msg.contains("repo")
                        || msg.contains("keychain")
                        || msg.contains("credential"),
                    "unexpected error message: {msg}"
                );
            }
            other => panic!("expected Error chunk, got {other:?}"),
        }
        // No further chunks after the terminal Error.
        assert!(stream.next().await.is_none());
    }

    #[tokio::test]
    async fn is_cloud_kind_recognizes_cursor_cloud() {
        assert!(is_cloud_kind(&AgentKind::CursorCloud));
        assert!(is_cloud_kind(&AgentKind::CopilotCloud));
        // The turn-shaped Gemini-family cloud rows are cloud kinds too.
        assert!(is_cloud_kind(&AgentKind::AntigravityCloud));
        // Local kinds are not cloud — including the LOCAL Antigravity IDE,
        // which must stay distinct from the cloud row.
        assert!(!is_cloud_kind(&AgentKind::ClaudeCode));
        assert!(!is_cloud_kind(&AgentKind::Cursor));
        assert!(!is_cloud_kind(&AgentKind::Antigravity));
        assert!(!is_cloud_kind(&AgentKind::Unknown));
        assert!(!is_cloud_kind(&AgentKind::Other("x".to_string())));
    }

    #[tokio::test]
    async fn drive_cloud_antigravity_is_turn_shaped_and_surfaces_honest_error() {
        use futures_util::StreamExt;

        // AntigravityCloud is turn-shaped (task_shaped == false), so it routes
        // through `spawn_turn_stream`, NOT the task path. Without a live key in
        // the keychain it must produce ONE honest terminal Error chunk — never
        // the "turn-shaped cloud drive not yet implemented" fallback (that would
        // mean the dispatch arm is missing), never a task-acknowledgement card,
        // never a panic/stall.
        let stream = drive_cloud(AgentKind::AntigravityCloud, Question::new("What is 2+2?"))
            .await
            .expect("dispatch ok");
        futures_util::pin_mut!(stream);
        let first = stream.next().await.expect("at least one chunk");
        match first {
            AnswerChunk::Error(msg) => {
                assert!(msg.contains("antigravity_cloud"), "vendor-tagged: {msg}");
                // The dispatch arm IS wired — so we must NOT see the
                // not-implemented fallback string.
                assert!(
                    !msg.contains("not yet implemented"),
                    "AntigravityCloud must be wired into the turn-shaped dispatch: {msg}"
                );
                // The honest failure is either "no credential" (the usual CI
                // case) or a live request/parse error if a key happens to exist.
                assert!(
                    msg.contains("credential")
                        || msg.contains("keychain")
                        || msg.contains("request failed")
                        || msg.contains("rejected")
                        || msg.contains("parse failed"),
                    "unexpected error message: {msg}"
                );
            }
            other => panic!("expected terminal Error chunk, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn is_cloud_kind_recognizes_gemini_cloud_but_not_local_gemini() {
        // The generic Gemini cloud surface is a distinct kind from the local
        // `gemini` CLI — the two must never be conflated.
        assert!(is_cloud_kind(&AgentKind::GeminiCloud));
        assert!(!is_cloud_kind(&AgentKind::Gemini));
    }

    #[tokio::test]
    async fn drive_cloud_gemini_takes_turn_path_and_errors_without_credential() {
        use futures_util::StreamExt;

        // Gemini Cloud is turn-shaped (task_shaped == false), so it routes
        // through `spawn_turn_stream`. With no keychain credential it must emit
        // ONE terminal Error about the missing credential — NOT the task path's
        // "owner/repo"/"repo URL" gap (which would prove the wrong branch ran),
        // and NOT the "not yet implemented" fallback (which would mean the
        // GeminiCloud dispatch arm is missing). Honest, never silent.
        let stream = drive_cloud(AgentKind::GeminiCloud, Question::new("Did the build pass?"))
            .await
            .expect("dispatch ok");
        futures_util::pin_mut!(stream);
        let first = stream.next().await.expect("at least one chunk");
        match first {
            AnswerChunk::Error(msg) => {
                assert!(msg.contains("gemini_cloud"), "must name the vendor: {msg}");
                assert!(
                    !msg.contains("not yet implemented"),
                    "GeminiCloud must be wired into the turn-shaped dispatch: {msg}"
                );
                // Turn path's first gate is the keychain — NOT a repo gate, and
                // (in the rare case a real key exists) a live request/parse error.
                assert!(
                    msg.contains("keychain")
                        || msg.contains("credential")
                        || msg.contains("request failed")
                        || msg.contains("rejected")
                        || msg.contains("parse failed"),
                    "expected a credential/turn-path error, got: {msg}"
                );
                assert!(
                    !msg.contains("owner") && !msg.contains("repo URL"),
                    "must NOT hit the task-shaped repo gate: {msg}"
                );
            }
            other => panic!("expected Error chunk, got {other:?}"),
        }
        // No further chunks after the terminal Error.
        assert!(stream.next().await.is_none());
    }
}
