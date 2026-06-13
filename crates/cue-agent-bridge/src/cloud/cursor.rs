//! Cursor Cloud Agents adapter.
//!
//! See `docs/vendors/cursor.md` for the full vendor dossier (auth model,
//! billing, edge cases). This file holds the **irreducible per-vendor code**:
//! the registry row, the exact request bodies for create-agent and get-run,
//! the SSE event → [`AnswerChunk`] mapping, and the run-status → terminal
//! classification. Everything generic (HTTPS transport, keychain, audit)
//! lives in the shared `cloud/` modules.
//!
//! ### Why a static [`ENTRY`] constant
//!
//! Every cloud-vendor adapter exposes `pub static ENTRY: &CloudAgentEntry`
//! (or `pub const`). The cloud registry table is then literally a one-line
//! append per vendor — adding the *next* cloud vendor means adding ONE row,
//! not branching on the vendor name anywhere.
//!
//! ### Shape: task, not turn
//!
//! Cursor Cloud is *task-shaped*: `POST /v1/agents` kicks off a run that
//! lives minutes (not seconds) and produces a PR, not a chat reply. The
//! daemon's answer-ladder reads `task_shaped: true` off the entry and
//! decides between "stream + wait" and "dispatch + park for notification."

use crate::cloud::registry::{BillingModel, CloudAgentEntry, CloudTaskState};
use crate::drive::AnswerChunk;
use crate::registry::KindTag;

/// Cursor Cloud's API base URL. Pinned here so the URL never escapes into
/// adapter code as a free-string; the registry row reads it through
/// [`ENTRY.base_url`].
pub const BASE_URL: &str = "https://api.cursor.com";

/// Documented vendor short name; used as the audit log `vendor` field and
/// (with the `bluey_cloud_` prefix) as the OS keychain service name.
pub const VENDOR_SHORT: &str = "cursor_cloud";

/// Maximum task duration we tell the daemon to expect for a Cursor Cloud
/// run. Cursor doesn't document a hard limit, but forum reports cite
/// long-session crashes past ~500k tokens (≈ tens of minutes). We size the
/// poll-loop generously: 90 minutes, after which a still-running task is
/// surfaced as "long-running, link queued."
const MAX_TASK_DURATION_SECS: u32 = 90 * 60;

/// The registry row. **The only place "Cursor Cloud" is named as a row.**
/// All other code reads off this row via [`crate::cloud::registry::CLOUD_REGISTRY`].
pub const ENTRY: &CloudAgentEntry = &CloudAgentEntry {
    kind_tag: KindTag::CursorCloud,
    display_name: "Cursor Cloud Agent",
    vendor_short: VENDOR_SHORT,
    base_url: BASE_URL,
    // Cloud agents are "charged at API pricing for the selected model" —
    // ApiCredits, not Subscription. See dossier §Billing.
    billing_model: BillingModel::ApiCredits,
    consent_warning: "Cursor Cloud Agents run on Cursor's servers and require \
the agent to hold a copy of your repository while the run is in flight. Your \
prompt, repo contents, and any artifacts produced are stored by Cursor. Bluey \
only relays — disconnecting Bluey does NOT revoke the API key on Cursor's \
side (you must revoke it at https://cursor.com/dashboard/integrations).",
    task_shaped: true,
    max_task_duration_secs: MAX_TASK_DURATION_SECS,
};

// ---------------------------------------------------------------------------
// Request builders — pure data, unit-testable without HTTPS.
// ---------------------------------------------------------------------------

/// Build the JSON body for `POST /v1/agents` (create an agent + first run).
///
/// The body shape is the **exact** shape Cursor's docs publish
/// (https://cursor.com/docs/cloud-agent/api/endpoints): `prompt`, `model`,
/// `repos`, `workOnCurrentBranch`, `autoCreatePR`, `skipReviewerRequest`.
/// We deliberately OMIT optional fields (`name`, `envVars`, `mcpServers`,
/// `customSubagents`, `agentId`, `env`) in v1 to keep the surface tight.
///
/// `model_id` should typically be `"composer-2"` (the documented default).
pub fn build_create_agent_body(
    prompt: &str,
    repo_url: &str,
    starting_ref: &str,
    model_id: &str,
) -> serde_json::Value {
    serde_json::json!({
        "prompt": {
            "text": prompt,
            "images": [],
        },
        "model": {
            "id": model_id,
        },
        "repos": [
            {
                "url": repo_url,
                "startingRef": starting_ref,
            }
        ],
        "workOnCurrentBranch": false,
        "autoCreatePR": true,
        "skipReviewerRequest": false,
    })
}

/// Render the full URL for `POST /v1/agents`.
pub fn create_agent_url() -> String {
    format!("{BASE_URL}/v1/agents")
}

/// Render the full URL for `GET /v1/agents/{id}/runs/{runId}`.
pub fn get_run_url(agent_id: &str, run_id: &str) -> String {
    format!("{BASE_URL}/v1/agents/{agent_id}/runs/{run_id}")
}

/// Render the full URL for `GET /v1/agents/{id}/runs/{runId}/stream` (SSE).
pub fn stream_run_url(agent_id: &str, run_id: &str) -> String {
    format!("{BASE_URL}/v1/agents/{agent_id}/runs/{run_id}/stream")
}

/// Render the full URL for the API-key probe `GET /v1/me`.
pub fn probe_url() -> String {
    format!("{BASE_URL}/v1/me")
}

// ---------------------------------------------------------------------------
// Response parsers — input is the doc-published JSON, output is Bluey types.
// ---------------------------------------------------------------------------

/// A successful `POST /v1/agents` returns BOTH an `agent` and a first `run`.
/// We retain the two ids the daemon needs to poll/stream the run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreatedAgentRun {
    /// The agent id (`bc-*`). Held for resume / archive / delete.
    pub agent_id: String,
    /// The first run id (`run-*`). The unit the user is waiting on.
    pub run_id: String,
    /// Status reported by the server at create-time (almost always
    /// `CREATING`, occasionally `RUNNING`).
    pub run_status: CloudTaskState,
    /// Human-facing URL for the agent in Cursor's dashboard. Surfaced to the
    /// UI so the user can open the run in the browser.
    pub url: Option<String>,
}

/// Parse the JSON body of a successful `POST /v1/agents`. Returns the two
/// ids + the initial run status. Fails with `anyhow::Error` if the shape
/// diverges from the docs.
pub fn parse_create_agent_response(body: &serde_json::Value) -> anyhow::Result<CreatedAgentRun> {
    let agent = body
        .get("agent")
        .ok_or_else(|| anyhow::anyhow!("create-agent response missing `agent`"))?;
    let run = body
        .get("run")
        .ok_or_else(|| anyhow::anyhow!("create-agent response missing `run`"))?;
    let agent_id = agent
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("create-agent response: agent.id missing or non-string"))?
        .to_string();
    let run_id = run
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("create-agent response: run.id missing or non-string"))?
        .to_string();
    let run_status = parse_run_status(run.get("status").and_then(|v| v.as_str()).unwrap_or(""));
    let url = agent
        .get("url")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    Ok(CreatedAgentRun {
        agent_id,
        run_id,
        run_status,
        url,
    })
}

/// Result of polling `GET /v1/agents/{id}/runs/{runId}`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunPollResult {
    pub status: CloudTaskState,
    /// Final text result when the run is `Succeeded`; `None` otherwise.
    pub result_text: Option<String>,
    /// PR URL if the run produced one (always when `autoCreatePR: true`).
    pub pr_url: Option<String>,
    /// Run duration in milliseconds, when reported.
    pub duration_ms: Option<u64>,
}

/// Parse the JSON body of `GET /v1/agents/{id}/runs/{runId}`.
pub fn parse_run_response(body: &serde_json::Value) -> RunPollResult {
    let status = parse_run_status(body.get("status").and_then(|v| v.as_str()).unwrap_or(""));
    let result_text = body
        .get("result")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let duration_ms = body.get("durationMs").and_then(|v| v.as_u64());
    let pr_url = body
        .get("git")
        .and_then(|g| g.get("branches"))
        .and_then(|bs| bs.as_array())
        .and_then(|bs| bs.first())
        .and_then(|b| b.get("prUrl"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    RunPollResult {
        status,
        result_text,
        pr_url,
        duration_ms,
    }
}

/// Map Cursor's native run-status string to the normalized [`CloudTaskState`].
/// Unknown values are bucketed into `Other(raw)` so the audit log keeps the
/// raw label visible — a new Cursor status (e.g. `RETRYING`) is not silently
/// classified as terminal.
pub fn parse_run_status(raw: &str) -> CloudTaskState {
    // Doc-published values: CREATING, RUNNING, FINISHED, ERROR, CANCELLED,
    // EXPIRED. Cursor uses SCREAMING_SNAKE; the normalization layer accepts
    // any case so a vendor casing tweak doesn't break classification.
    match raw.to_ascii_uppercase().as_str() {
        "CREATING" => CloudTaskState::Queued,
        "RUNNING" => CloudTaskState::Running,
        "FINISHED" => CloudTaskState::Succeeded,
        "ERROR" => CloudTaskState::Failed,
        "CANCELLED" | "CANCELED" => CloudTaskState::Cancelled,
        // Per CloudTaskState's design, EXPIRED collapses to Failed (terminal,
        // no progress) so the daemon stops polling. The raw label is still
        // available in the audit log via the response body.
        "EXPIRED" => CloudTaskState::Failed,
        "" => CloudTaskState::Other("UNSPECIFIED".to_string()),
        other => CloudTaskState::Other(other.to_string()),
    }
}

// ---------------------------------------------------------------------------
// SSE event → AnswerChunk mapping.
// ---------------------------------------------------------------------------

/// Parse one SSE event into zero or more [`AnswerChunk`]s.
///
/// Cursor's SSE event types (doc-confirmed):
/// `status`, `assistant`, `thinking`, `tool_call`, `interaction_update`,
/// `heartbeat`, `result`, `error`, `done`.
///
/// Mapping:
/// - first `status` carrying a `runId` → `Started { session_id: Some(runId) }`
/// - `assistant` with `text` → `Delta(text)`
/// - `thinking` / `tool_call` / `interaction_update` → dropped (telemetry)
/// - `heartbeat` → dropped (keep-alive)
/// - `result` → `Delta(result.text)` if present, then `Done { cost_usd: None }`
/// - `error` → `Error(message)`
/// - `done` → nothing (the stream just ends)
///
/// `started` is a caller-tracked flag so a `Started` is emitted at most
/// once per stream — Cursor sends multiple `status` events, only the first
/// becomes our `Started`.
pub fn parse_sse_event(event_name: &str, data: &str, started: &mut bool) -> Vec<AnswerChunk> {
    if data.is_empty() {
        return Vec::new();
    }
    let parsed: Result<serde_json::Value, _> = serde_json::from_str(data);
    let Ok(v) = parsed else {
        // Non-JSON event data — Cursor's docs only describe JSON, but be
        // defensive: drop instead of crashing the stream.
        return Vec::new();
    };
    match event_name {
        "status" => {
            // First status carrying a runId emits our Started.
            if *started {
                return Vec::new();
            }
            let run_id = v
                .get("runId")
                .and_then(|s| s.as_str())
                .map(|s| s.to_string());
            *started = true;
            vec![AnswerChunk::Started { session_id: run_id }]
        }
        "assistant" => {
            let mut out = Vec::new();
            if !*started {
                *started = true;
                out.push(AnswerChunk::Started { session_id: None });
            }
            if let Some(text) = v.get("text").and_then(|s| s.as_str()) {
                if !text.is_empty() {
                    out.push(AnswerChunk::Delta(text.to_string()));
                }
            }
            out
        }
        "result" => {
            let mut out = Vec::new();
            if !*started {
                *started = true;
                out.push(AnswerChunk::Started { session_id: None });
            }
            // Cursor's result event carries the final text + status; mirror
            // it into a final Delta + Done so consumers always see the body.
            if let Some(text) = v.get("text").and_then(|s| s.as_str()) {
                if !text.is_empty() {
                    out.push(AnswerChunk::Delta(text.to_string()));
                }
            }
            // No per-call USD cost: Cursor bills by tokens/credits.
            out.push(AnswerChunk::Done { cost_usd: None });
            out
        }
        "error" => {
            let mut out = Vec::new();
            if !*started {
                *started = true;
                out.push(AnswerChunk::Started { session_id: None });
            }
            let msg = v
                .get("message")
                .and_then(|s| s.as_str())
                .unwrap_or("Cursor Cloud reported an error")
                .to_string();
            out.push(AnswerChunk::Error(msg));
            out
        }
        // Telemetry events and keep-alives — drop without surfacing.
        "thinking" | "tool_call" | "interaction_update" | "heartbeat" | "done" => Vec::new(),
        _ => {
            // Unknown event type: drop fail-soft (a future Cursor event
            // shouldn't break the stream parser).
            Vec::new()
        }
    }
}

/// Dispatcher-side helper: parse a `POST /v1/agents` response body and
/// return a 3-tuple `(run_id, agent_dashboard_url, acknowledgement_text)`
/// for the cloud-drive dispatcher to surface back to the daemon.
///
/// Stays free of any cross-adapter type coupling — the dispatcher reads what
/// it needs out of the tuple. The acknowledgement text follows the same
/// shape as the Copilot adapter's: "Kicked off `<vendor>` task `<id>`. The
/// PR will appear at `<url>` in ~5-15 minutes."
pub fn parse_task_response_for_dispatch(
    body: &[u8],
) -> anyhow::Result<(String, Option<String>, String)> {
    let v: serde_json::Value = serde_json::from_slice(body)
        .map_err(|e| anyhow::anyhow!("Cursor Cloud response was not valid JSON: {e}"))?;
    let parsed = parse_create_agent_response(&v)?;
    let ack = render_acknowledgement(&parsed);
    Ok((parsed.run_id, parsed.url, ack))
}

/// Build the acknowledgement text shown to the user after a successful
/// kick-off. Task-shaped agents emit this as a single Delta so the overlay's
/// streaming UI completes; the user is then notified later when the actual
/// run finishes.
pub fn render_acknowledgement(run: &CreatedAgentRun) -> String {
    let url_phrase = match &run.url {
        Some(u) => format!(" Track progress at {u}."),
        None => String::new(),
    };
    format!(
        "Kicked off Cursor Cloud agent (run {}).{url_phrase} The PR will land in \
         a few minutes — Bluey will notify you when it does.",
        run.run_id
    )
}

/// Best-effort parse of Cursor's error envelope:
/// `{"error":"<HTTP phrase>","message":"<text>"}`. Falls back to the raw body
/// when the shape isn't what's documented.
pub fn parse_error_body(body: &[u8]) -> String {
    if let Ok(v) = serde_json::from_slice::<serde_json::Value>(body) {
        if let Some(msg) = v.get("message").and_then(|m| m.as_str()) {
            if !msg.is_empty() {
                return msg.to_string();
            }
        }
        if let Some(err) = v.get("error").and_then(|m| m.as_str()) {
            return err.to_string();
        }
    }
    String::from_utf8_lossy(body).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- entry shape -----------------------------------------------------

    #[test]
    fn entry_has_required_shape() {
        assert_eq!(ENTRY.kind_tag, KindTag::CursorCloud);
        assert_eq!(ENTRY.vendor_short, "cursor_cloud");
        assert_eq!(ENTRY.base_url, "https://api.cursor.com");
        assert!(ENTRY.task_shaped, "Cursor Cloud is task-shaped");
        assert!(
            ENTRY.max_task_duration_secs > 0,
            "task-shaped vendors must declare a max duration"
        );
        // Billing model must be ApiCredits (not Subscription) — the user is
        // billed per call at API pricing for the selected model.
        assert_eq!(ENTRY.billing_model, BillingModel::ApiCredits);
    }

    #[test]
    fn entry_consent_warning_mentions_repo_storage_and_revocation() {
        // The dossier's headline corporate risk: prompt + repo go to Cursor,
        // and disconnecting from Bluey does NOT revoke the key. Both must
        // appear in the consent text.
        let w = ENTRY.consent_warning;
        assert!(w.to_lowercase().contains("repo"), "must mention repo");
        assert!(w.contains("revoke"), "must mention revocation flow");
        assert!(
            w.contains("cursor.com/dashboard/integrations"),
            "must give the exact revocation URL"
        );
    }

    // ---- URL builders ----------------------------------------------------

    #[test]
    fn create_agent_url_is_v1_agents() {
        assert_eq!(create_agent_url(), "https://api.cursor.com/v1/agents");
    }

    #[test]
    fn get_run_url_interpolates_both_ids() {
        let u = get_run_url("bc-abc", "run-xyz");
        assert_eq!(u, "https://api.cursor.com/v1/agents/bc-abc/runs/run-xyz");
    }

    #[test]
    fn stream_run_url_carries_stream_suffix() {
        let u = stream_run_url("bc-abc", "run-xyz");
        assert_eq!(
            u,
            "https://api.cursor.com/v1/agents/bc-abc/runs/run-xyz/stream"
        );
    }

    #[test]
    fn probe_url_is_v1_me() {
        assert_eq!(probe_url(), "https://api.cursor.com/v1/me");
    }

    // ---- create-agent body shape (matches the docs verbatim) -------------

    #[test]
    fn create_agent_body_has_documented_shape() {
        let body = build_create_agent_body(
            "Add a README with setup instructions",
            "https://github.com/your-org/your-repo",
            "main",
            "composer-2",
        );
        // prompt.text / prompt.images
        assert_eq!(
            body["prompt"]["text"],
            "Add a README with setup instructions"
        );
        assert!(body["prompt"]["images"].is_array());
        assert_eq!(body["prompt"]["images"].as_array().unwrap().len(), 0);
        // model.id
        assert_eq!(body["model"]["id"], "composer-2");
        // repos[0]
        assert_eq!(
            body["repos"][0]["url"],
            "https://github.com/your-org/your-repo"
        );
        assert_eq!(body["repos"][0]["startingRef"], "main");
        // flags
        assert_eq!(body["workOnCurrentBranch"], false);
        assert_eq!(body["autoCreatePR"], true);
        assert_eq!(body["skipReviewerRequest"], false);
    }

    #[test]
    fn create_agent_body_omits_optional_fields_in_v1() {
        let body = build_create_agent_body("hi", "https://example/repo", "main", "composer-2");
        // The v1 contract is "send only what's required" — these optional
        // fields stay absent so a future server-side default can take effect.
        for forbidden in [
            "name",
            "envVars",
            "mcpServers",
            "customSubagents",
            "agentId",
            "env",
        ] {
            assert!(
                body.get(forbidden).is_none(),
                "v1 body should omit `{forbidden}` (got {body})"
            );
        }
    }

    // ---- create-agent response parser -----------------------------------

    #[test]
    fn parse_create_agent_response_extracts_both_ids() {
        // The exact response shape from cursor.com/docs/cloud-agent/api/endpoints.
        let body = serde_json::json!({
            "agent": {
                "id": "bc-00000000-0000-0000-0000-000000000001",
                "name": "Add README",
                "status": "ACTIVE",
                "url": "https://cursor.com/agents/bc-00000000-0000-0000-0000-000000000001",
                "createdAt": "2026-04-13T18:30:00.000Z",
                "updatedAt": "2026-04-13T18:30:00.000Z",
                "latestRunId": "run-00000000-0000-0000-0000-000000000001"
            },
            "run": {
                "id": "run-00000000-0000-0000-0000-000000000001",
                "agentId": "bc-00000000-0000-0000-0000-000000000001",
                "status": "CREATING",
                "createdAt": "2026-04-13T18:30:00.000Z",
                "updatedAt": "2026-04-13T18:30:00.000Z"
            }
        });
        let parsed = parse_create_agent_response(&body).unwrap();
        assert_eq!(parsed.agent_id, "bc-00000000-0000-0000-0000-000000000001");
        assert_eq!(parsed.run_id, "run-00000000-0000-0000-0000-000000000001");
        assert_eq!(parsed.run_status, CloudTaskState::Queued);
        assert_eq!(
            parsed.url.as_deref(),
            Some("https://cursor.com/agents/bc-00000000-0000-0000-0000-000000000001")
        );
    }

    #[test]
    fn parse_create_agent_response_errors_clearly_on_missing_agent() {
        let body = serde_json::json!({ "run": { "id": "run-1" } });
        let err = parse_create_agent_response(&body).unwrap_err();
        assert!(err.to_string().contains("agent"));
    }

    #[test]
    fn parse_create_agent_response_errors_clearly_on_missing_run() {
        let body = serde_json::json!({ "agent": { "id": "bc-1" } });
        let err = parse_create_agent_response(&body).unwrap_err();
        assert!(err.to_string().contains("run"));
    }

    // ---- get-run parser --------------------------------------------------

    #[test]
    fn parse_run_response_handles_finished_with_pr() {
        // Body from the docs verbatim (FINISHED + PR URL + duration).
        let body = serde_json::json!({
            "id": "run-00000000-0000-0000-0000-000000000001",
            "agentId": "bc-00000000-0000-0000-0000-000000000001",
            "status": "FINISHED",
            "createdAt": "2026-04-13T18:30:00.000Z",
            "updatedAt": "2026-04-13T18:45:00.000Z",
            "durationMs": 12357,
            "result": "Added README.md with installation instructions and usage examples.",
            "git": {
                "branches": [
                    {
                        "repoUrl": "github.com/your-org/your-repo",
                        "branch": "cursor/add-readme-a1b2",
                        "prUrl": "https://github.com/your-org/your-repo/pull/123"
                    }
                ]
            }
        });
        let parsed = parse_run_response(&body);
        assert_eq!(parsed.status, CloudTaskState::Succeeded);
        assert_eq!(
            parsed.result_text.as_deref(),
            Some("Added README.md with installation instructions and usage examples.")
        );
        assert_eq!(
            parsed.pr_url.as_deref(),
            Some("https://github.com/your-org/your-repo/pull/123")
        );
        assert_eq!(parsed.duration_ms, Some(12357));
    }

    #[test]
    fn parse_run_response_handles_in_flight_run() {
        let body = serde_json::json!({
            "id": "run-1",
            "agentId": "bc-1",
            "status": "RUNNING",
            "createdAt": "2026-04-13T18:30:00.000Z",
            "updatedAt": "2026-04-13T18:31:00.000Z"
        });
        let parsed = parse_run_response(&body);
        assert_eq!(parsed.status, CloudTaskState::Running);
        assert_eq!(parsed.result_text, None);
        assert_eq!(parsed.pr_url, None);
        assert_eq!(parsed.duration_ms, None);
    }

    // ---- status normalizer -----------------------------------------------

    #[test]
    fn run_status_normalization_round_trips_documented_values() {
        // Doc-published values map to their normalized counterparts.
        assert_eq!(parse_run_status("CREATING"), CloudTaskState::Queued);
        assert_eq!(parse_run_status("RUNNING"), CloudTaskState::Running);
        assert_eq!(parse_run_status("FINISHED"), CloudTaskState::Succeeded);
        assert_eq!(parse_run_status("ERROR"), CloudTaskState::Failed);
        assert_eq!(parse_run_status("CANCELLED"), CloudTaskState::Cancelled);
        // British/American spelling defense.
        assert_eq!(parse_run_status("CANCELED"), CloudTaskState::Cancelled);
        // EXPIRED → Failed so the daemon stops polling.
        assert_eq!(parse_run_status("EXPIRED"), CloudTaskState::Failed);
    }

    #[test]
    fn run_status_normalization_accepts_lowercase() {
        // A casing tweak by Cursor shouldn't break classification.
        assert_eq!(parse_run_status("running"), CloudTaskState::Running);
        assert_eq!(parse_run_status("Finished"), CloudTaskState::Succeeded);
    }

    #[test]
    fn run_status_normalization_bucket_unknown_into_other() {
        // A new vendor status (RETRYING, …) lands in Other(raw) — visible in
        // the audit log, not silently terminal.
        let s = parse_run_status("RETRYING");
        match s {
            CloudTaskState::Other(raw) => assert_eq!(raw, "RETRYING"),
            other => panic!("expected Other(\"RETRYING\"), got {other:?}"),
        }
    }

    #[test]
    fn empty_run_status_does_not_panic() {
        let s = parse_run_status("");
        match s {
            CloudTaskState::Other(raw) => assert_eq!(raw, "UNSPECIFIED"),
            other => panic!("expected Other for empty status, got {other:?}"),
        }
    }

    // ---- SSE event mapping ----------------------------------------------

    #[test]
    fn sse_first_status_emits_started_with_run_id() {
        let mut started = false;
        let out = parse_sse_event(
            "status",
            r#"{"runId":"run-7","status":"RUNNING"}"#,
            &mut started,
        );
        assert_eq!(
            out,
            vec![AnswerChunk::Started {
                session_id: Some("run-7".to_string()),
            }]
        );
        assert!(started);
    }

    #[test]
    fn sse_second_status_is_dropped() {
        // Cursor sends multiple status events; only the first becomes our
        // Started, the rest are silent.
        let mut started = true;
        let out = parse_sse_event(
            "status",
            r#"{"runId":"run-7","status":"RUNNING"}"#,
            &mut started,
        );
        assert!(out.is_empty());
    }

    #[test]
    fn sse_assistant_emits_delta_text() {
        let mut started = true;
        let out = parse_sse_event(
            "assistant",
            r#"{"text":"I'll update the README now."}"#,
            &mut started,
        );
        assert_eq!(
            out,
            vec![AnswerChunk::Delta(
                "I'll update the README now.".to_string()
            )]
        );
    }

    #[test]
    fn sse_assistant_before_status_emits_started_first() {
        // If the assistant chunk arrives before any status event, the parser
        // still has to send a Started so downstream sees the correct ordering.
        let mut started = false;
        let out = parse_sse_event("assistant", r#"{"text":"hi"}"#, &mut started);
        assert_eq!(
            out,
            vec![
                AnswerChunk::Started { session_id: None },
                AnswerChunk::Delta("hi".to_string()),
            ]
        );
        assert!(started);
    }

    #[test]
    fn sse_result_emits_delta_then_done() {
        let mut started = true;
        let out = parse_sse_event(
            "result",
            r#"{"runId":"run-7","status":"FINISHED","text":"Added README.md","durationMs":12357}"#,
            &mut started,
        );
        assert_eq!(
            out,
            vec![
                AnswerChunk::Delta("Added README.md".to_string()),
                AnswerChunk::Done { cost_usd: None },
            ]
        );
    }

    #[test]
    fn sse_error_emits_error_with_message() {
        let mut started = true;
        let out = parse_sse_event(
            "error",
            r#"{"message":"composer-2 is currently unavailable"}"#,
            &mut started,
        );
        assert_eq!(
            out,
            vec![AnswerChunk::Error(
                "composer-2 is currently unavailable".to_string()
            )]
        );
    }

    #[test]
    fn sse_telemetry_events_are_dropped() {
        for ev in [
            "thinking",
            "tool_call",
            "interaction_update",
            "heartbeat",
            "done",
        ] {
            let mut started = true;
            let out = parse_sse_event(ev, r#"{"x":1}"#, &mut started);
            assert!(out.is_empty(), "{ev} should drop, got {out:?}");
        }
    }

    #[test]
    fn sse_unknown_event_is_dropped_fail_soft() {
        let mut started = true;
        let out = parse_sse_event("future_event_type", r#"{"x":1}"#, &mut started);
        assert!(out.is_empty());
    }

    #[test]
    fn sse_invalid_json_is_dropped_not_propagated() {
        // Defense: a single malformed event must not crash the stream.
        let mut started = true;
        let out = parse_sse_event("assistant", "not json", &mut started);
        assert!(out.is_empty());
    }

    #[test]
    fn sse_empty_data_is_dropped() {
        let mut started = false;
        let out = parse_sse_event("status", "", &mut started);
        assert!(out.is_empty());
        assert!(!started);
    }

    // ---- error envelope parser -------------------------------------------

    #[test]
    fn parse_error_body_extracts_message() {
        let body =
            br#"{"error":"Forbidden","message":"Cloud agents not available on the Free plan."}"#;
        let msg = parse_error_body(body);
        assert_eq!(msg, "Cloud agents not available on the Free plan.");
    }

    #[test]
    fn parse_error_body_falls_back_to_error_field() {
        let body = br#"{"error":"Bad Request"}"#;
        let msg = parse_error_body(body);
        assert_eq!(msg, "Bad Request");
    }

    #[test]
    fn parse_error_body_falls_back_to_raw_for_unknown_shape() {
        let body = b"not json at all";
        let msg = parse_error_body(body);
        assert_eq!(msg, "not json at all");
    }

    // ---- dispatcher helpers ----------------------------------------------

    #[test]
    fn parse_task_response_for_dispatch_returns_id_url_ack() {
        // The dispatcher-side wrapper returns (run_id, agent_url, ack_text).
        // All three derive from the documented response shape.
        let body = br#"{
            "agent": {
                "id": "bc-1",
                "url": "https://cursor.com/agents/bc-1"
            },
            "run": {
                "id": "run-1",
                "status": "CREATING"
            }
        }"#;
        let (run_id, url, ack) = parse_task_response_for_dispatch(body).unwrap();
        assert_eq!(run_id, "run-1");
        assert_eq!(url.as_deref(), Some("https://cursor.com/agents/bc-1"));
        assert!(ack.contains("run-1"));
        // The acknowledgement must mention the dashboard URL so the user can
        // open the run in the browser.
        assert!(ack.contains("https://cursor.com/agents/bc-1"));
        // And it must set expectations (minutes-long, PR-shaped).
        assert!(ack.to_lowercase().contains("minute") || ack.to_lowercase().contains("pr"));
    }

    #[test]
    fn parse_task_response_for_dispatch_errors_on_invalid_json() {
        let err = parse_task_response_for_dispatch(b"not json").unwrap_err();
        let msg = err.to_string();
        assert!(msg.to_lowercase().contains("json"));
    }

    #[test]
    fn render_acknowledgement_includes_run_id_and_url() {
        let run = CreatedAgentRun {
            agent_id: "bc-1".to_string(),
            run_id: "run-7".to_string(),
            run_status: CloudTaskState::Queued,
            url: Some("https://cursor.com/agents/bc-1".to_string()),
        };
        let text = render_acknowledgement(&run);
        assert!(text.contains("run-7"));
        assert!(text.contains("https://cursor.com/agents/bc-1"));
    }

    #[test]
    fn render_acknowledgement_handles_missing_url() {
        let run = CreatedAgentRun {
            agent_id: "bc-1".to_string(),
            run_id: "run-7".to_string(),
            run_status: CloudTaskState::Queued,
            url: None,
        };
        let text = render_acknowledgement(&run);
        assert!(text.contains("run-7"));
        // No URL means no "Track progress at" sentence.
        assert!(!text.contains("Track progress"));
    }

    // ---- end-to-end through the shared transport -----------------------
    //
    // Exercises the WHOLE adapter path against a wiremock server: build the
    // request, fire it with Bearer auth and the keychain abstraction, then
    // parse the response with our dispatcher helper. The only piece this
    // doesn't cover is `drive_cloud`'s composition (the dispatcher-level
    // routing + stream emission), which is tested in cloud::drive::tests.

    #[tokio::test]
    async fn end_to_end_create_agent_against_wiremock() {
        use crate::cloud::keychain::{MemoryCredentialStore, VendorCredentialStore};
        use crate::cloud::transport::{BearerAuth, CloudHttpsTransport, HttpRequest};
        use wiremock::matchers::{body_json, header, header_exists, method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;

        // Mock asserts that we send EXACTLY the documented body shape with
        // the correct auth header. The order of fields in the json! macro
        // matches the docs example verbatim.
        let expected_body = build_create_agent_body(
            "Add a README",
            "https://github.com/your-org/your-repo",
            "main",
            "composer-2",
        );
        Mock::given(method("POST"))
            .and(path("/v1/agents"))
            .and(header_exists("authorization"))
            .and(header("authorization", "Bearer crsr_test_key"))
            .and(body_json(expected_body.clone()))
            .respond_with(
                ResponseTemplate::new(201)
                    .insert_header("x-cursor-request-id", "req-1")
                    .set_body_json(serde_json::json!({
                        "agent": {
                            "id": "bc-abc",
                            "name": "Add README",
                            "status": "ACTIVE",
                            "url": "https://cursor.com/agents/bc-abc",
                            "latestRunId": "run-xyz"
                        },
                        "run": {
                            "id": "run-xyz",
                            "agentId": "bc-abc",
                            "status": "CREATING"
                        }
                    })),
            )
            .mount(&server)
            .await;

        // Wire the transport with a memory store holding a fake key.
        let creds = MemoryCredentialStore::new(VENDOR_SHORT);
        creds.save("api_key", "crsr_test_key").unwrap();

        // The dispatcher calls the URL on the entry's base_url; we use the
        // mock server's URL as the substitute base.
        let req = HttpRequest::post_json(format!("{}/v1/agents", server.uri()), expected_body);
        let transport = CloudHttpsTransport::new();
        let resp = transport
            .send(
                VENDOR_SHORT,
                "/v1/agents",
                &req,
                BearerAuth::AUTH,
                &creds,
                "api_key",
            )
            .await
            .expect("send");

        assert_eq!(resp.status, 201);
        assert!(resp.request_id_present);
        // Now run the dispatcher-side parse helper end-to-end.
        let (run_id, url, ack) = parse_task_response_for_dispatch(&resp.body).expect("parse");
        assert_eq!(run_id, "run-xyz");
        assert_eq!(url.as_deref(), Some("https://cursor.com/agents/bc-abc"));
        assert!(ack.contains("run-xyz"));
        assert!(ack.contains("https://cursor.com/agents/bc-abc"));
    }

    #[tokio::test]
    async fn end_to_end_403_free_plan_surfaces_documented_message() {
        use crate::cloud::keychain::{MemoryCredentialStore, VendorCredentialStore};
        use crate::cloud::transport::{BearerAuth, CloudHttpsTransport, HttpRequest};
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/agents"))
            .respond_with(ResponseTemplate::new(403).set_body_json(serde_json::json!({
                "error": "Forbidden",
                "message": "Cloud agents not available on the Free plan."
            })))
            .mount(&server)
            .await;

        let creds = MemoryCredentialStore::new(VENDOR_SHORT);
        creds.save("api_key", "crsr_free_plan").unwrap();
        let body = build_create_agent_body("hi", "https://github.com/x/y", "main", "composer-2");
        let req = HttpRequest::post_json(format!("{}/v1/agents", server.uri()), body);
        let transport = CloudHttpsTransport::new();
        let resp = transport
            .send(
                VENDOR_SHORT,
                "/v1/agents",
                &req,
                BearerAuth::AUTH,
                &creds,
                "api_key",
            )
            .await
            .expect("send returns the 4xx response, not Err");
        assert_eq!(resp.status, 403);
        // The adapter's error-envelope parser must extract the message
        // verbatim from the response body.
        let msg = parse_error_body(&resp.body);
        assert_eq!(msg, "Cloud agents not available on the Free plan.");
    }
}
