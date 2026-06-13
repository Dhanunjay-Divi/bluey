//! OpenAI Codex Cloud adapter — task-shaped cloud agent.
//!
//! Codex Cloud delegates work to a sandboxed container in OpenAI's
//! infrastructure: clone the user's repo, run the task, open a PR. Tasks
//! run async and finish *minutes* later. The unit of work is a task, not a
//! streaming turn.
//!
//! ### What this module owns (irreducible per-vendor code)
//!
//! - The exact JSON body of `POST /v1/codex/cloud/tasks`. Field names
//!   (`task_prompt`, `environment`, `repository_context`, `webhook`) are
//!   Codex-specific.
//! - The polling URL template `/v1/codex/cloud/tasks/{task_id}` and the
//!   `status` enum parsing (`queued | running | completed | failed`).
//! - Mapping HTTP status codes onto user-facing guidance strings.
//!
//! Everything else (HTTPS dispatch, auth header construction, audit
//! logging, keychain access) lives in the vendor-agnostic primitives
//! ([`super::transport`], [`super::keychain`], [`super::audit`]).
//!
//! ### Honest gaps — NEEDS-LIVE-VERIFY (see `docs/vendors/codex_cloud.md`)
//!
//! - The request/response shapes here come from the third-party apidog
//!   digest, NOT an official OpenAI reference. The body builder is
//!   deliberately split into one function per shape so swapping any field
//!   in is a one-line patch. The unit tests assert the *current* shape so
//!   any drift produces a loud failure.
//! - No documented event-stream URL for cloud tasks; this adapter exposes
//!   only the polling form (`GET /v1/codex/cloud/tasks/{id}`).
//! - No documented third-party-app OAuth flow exists today. The adapter
//!   uses BYOT (Bring Your Own Token) — the user pastes an
//!   `OPENAI_API_KEY`, Bluey stores it in the OS keychain, and bills
//!   against the user's OpenAI Platform account. Disclosure is mandatory
//!   (see [`OPENAI_BYOT_CONSENT_TEXT`]).

use anyhow::{anyhow, Result};
use serde_json::json;

use super::registry::{BillingModel, CloudAgentEntry};
#[cfg(test)]
use super::transport::CloudAuth;
use super::transport::{
    CloudEndpoints, CloudHttpsTransport, CloudTransport, HttpRequest, HttpResponse, BEARER_AUTH,
};
use crate::registry::KindTag;

/// Vendor short-name. Used as the keychain service suffix and as the audit
/// log's `vendor` field. Kept lowercase ASCII to match the convention the
/// other vendor adapters use.
pub const VENDOR: &str = "codex_cloud";

/// Keychain credential key under which Bluey stores the user's
/// `OPENAI_API_KEY`. The full keychain path is
/// `bluey_cloud_codex_cloud / api_key`.
pub const CREDENTIAL_KEY: &str = "api_key";

/// Env var the user can paste from in their shell, mirrored to the
/// keychain on first use. Bluey reads this only as a one-shot import — the
/// daemon never writes a secret back out into the environment.
pub const CREDENTIAL_ENV_FALLBACK: &str = "OPENAI_API_KEY";

/// Base URL of the OpenAI Platform API. No trailing slash.
pub const BASE_URL: &str = "https://api.openai.com";

/// `POST` endpoint that creates a cloud task. Reproduced from the apidog
/// digest of the beta Codex Cloud API — NEEDS-LIVE-VERIFY against a real
/// account. The endpoint is documented as beta and may change.
pub const CREATE_TASK_PATH: &str = "/v1/codex/cloud/tasks";

/// `GET` endpoint that returns the current status of a cloud task. Template
/// substitutes `{task_id}` with the opaque id returned by
/// [`CREATE_TASK_PATH`].
pub const GET_TASK_STATUS_PATH_TEMPLATE: &str = "/v1/codex/cloud/tasks/{task_id}";

/// How often the adapter polls the status endpoint while a task is running.
/// Conservative default — OpenAI has not published a recommended cadence.
pub const POLL_INTERVAL_MS: u64 = 5_000;

/// Hard ceiling on total time to wait for a task to finish before detaching
/// the polling loop. The task continues server-side; the user can follow
/// the PR URL when it appears.
pub const MAX_TOTAL_WAIT_MS: u64 = 30 * 60 * 1_000;

/// The default Codex model the cloud task runs under, when the daemon does
/// not override. Read off the registry row.
pub const DEFAULT_MODEL: &str = "gpt-5-codex";

/// Mandatory consent text the UI MUST show before Bluey stores an OpenAI
/// API key. This text encodes the two non-obvious promises the user must
/// understand: (1) BYOT bills their OpenAI Platform account, NOT their
/// ChatGPT subscription; (2) every task clones their repo into an
/// OpenAI-hosted sandbox.
pub const OPENAI_BYOT_CONSENT_TEXT: &str = "Bluey will store your OpenAI API key in your \
    OS keychain and send it on each Codex Cloud call. Your repo is cloned \
    into an OpenAI-hosted sandbox container for each task. This bills your \
    OpenAI API credits, NOT your ChatGPT subscription.";

/// Maximum wall-clock seconds the daemon will keep polling a Codex Cloud
/// task before detaching. Codex Cloud tasks run minutes (small) to tens of
/// minutes (large). 30 min is a defensive upper bound — the PR appears on
/// GitHub regardless of whether Bluey is still polling, so detaching does
/// not lose work.
pub const MAX_TASK_DURATION_SECS: u32 = 30 * 60;

/// The registry row for OpenAI Codex Cloud. Pure data; appended to
/// [`super::registry::CLOUD_REGISTRY`] so the cloud-vendor enumeration sees
/// it without any per-vendor branch.
///
/// `kind_tag` is [`KindTag::CodexCloud`] — distinct from the local
/// `KindTag::Codex` so the daemon's parse path can route an "ask Codex
/// Cloud" intent to this row, not the local `codex` CLI row.
pub const ENTRY: &CloudAgentEntry = &CloudAgentEntry {
    kind_tag: KindTag::CodexCloud,
    display_name: "OpenAI Codex Cloud",
    vendor_short: VENDOR,
    base_url: BASE_URL,
    // BYOT — the user's OpenAI Platform API key bills against their
    // OpenAI API ledger, NOT their ChatGPT Plus quota. The disclosure
    // in `consent_warning` MUST make this clear.
    billing_model: BillingModel::ApiCredits,
    consent_warning: OPENAI_BYOT_CONSENT_TEXT,
    task_shaped: true,
    max_task_duration_secs: MAX_TASK_DURATION_SECS,
};

/// `CloudTransport` shape for the Codex Cloud registry row. Pure data;
/// reading this hands the dispatcher the URL/auth/header set without
/// naming the vendor anywhere else.
pub const TRANSPORT: CloudTransport = CloudTransport::Https {
    base_url: BASE_URL,
    // OpenAI requires no extra static headers beyond what the dispatcher
    // adds (`Authorization`, `Content-Type`, `User-Agent`).
    headers: &[],
    auth: BEARER_AUTH,
    endpoints: CloudEndpoints {
        create_agent: None,
        create_environment: None,
        // SESSION-shape unused; task-shape used.
        create_session: None,
        send_event: None,
        stream_events: None,
        delete_session: None,
        create_task: Some(CREATE_TASK_PATH),
        get_task_status: Some(GET_TASK_STATUS_PATH_TEMPLATE),
    },
};

/// Normalized task lifecycle state. Maps every Codex `status` string the
/// adapter has seen onto a stable enum the daemon can route on. Unknown
/// strings are treated as `Running` (defensive — never finish a task on
/// a status we don't recognize).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskState {
    /// The task is queued but has not started executing yet.
    Queued,
    /// The task is actively running in a cloud sandbox.
    Running,
    /// The task finished successfully. `result_url` carries the PR URL when
    /// the response included one.
    Completed,
    /// The task failed (sandbox error, repo clone failure, agent ran out of
    /// budget). `error_message` carries the vendor's explanation.
    Failed,
}

/// Parsed task-status envelope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskStatus {
    pub id: String,
    pub state: TaskState,
    /// PR URL (or other artifact link) when `state == Completed`. `None`
    /// otherwise.
    pub result_url: Option<String>,
    /// Vendor's error message when `state == Failed`. `None` otherwise.
    pub error_message: Option<String>,
}

/// Build the `POST /v1/codex/cloud/tasks` body. Pure: takes a prompt and a
/// repo context string, returns a `serde_json::Value` the transport will
/// serialize. Side-effect-free so the wire shape is unit-testable without
/// firing a real HTTPS call.
///
/// Field meanings (from the apidog digest of the beta API — NEEDS-LIVE-VERIFY):
/// - `task_prompt`: the instruction the agent runs (free-form English).
/// - `environment.runtime`: container runtime (e.g. `"node:18"`,
///   `"python:3.11"`). Carried as an opaque string so callers can pass the
///   user's preference verbatim.
/// - `repository_context`: a `<repo_url>/<branch>` form per the apidog
///   sample. The function takes the components separately and joins so
///   callers don't have to.
/// - `webhook`: optional callback URL for completion notification. The
///   adapter does not wire webhooks in this batch; the polling form is
///   used. Field included as `None` so the request body shape matches the
///   docs even when the field is empty.
pub fn build_create_task_body(
    task_prompt: &str,
    repo_url: &str,
    branch: &str,
    runtime: Option<&str>,
) -> serde_json::Value {
    let mut env = serde_json::Map::new();
    if let Some(rt) = runtime {
        env.insert("runtime".to_string(), json!(rt));
    }
    json!({
        "task_prompt": task_prompt,
        "environment": serde_json::Value::Object(env),
        "repository_context": format!("{repo_url}/{branch}"),
        "webhook": serde_json::Value::Null,
    })
}

/// Build the [`HttpRequest`] for `POST /v1/codex/cloud/tasks`. Pure: returns
/// a request struct without firing it. The transport applies auth +
/// content-type at send time.
pub fn create_task_request(
    task_prompt: &str,
    repo_url: &str,
    branch: &str,
    runtime: Option<&str>,
) -> HttpRequest {
    HttpRequest::post_json(
        format!("{BASE_URL}{CREATE_TASK_PATH}"),
        build_create_task_body(task_prompt, repo_url, branch, runtime),
    )
}

/// Build the [`HttpRequest`] for `GET /v1/codex/cloud/tasks/{task_id}`.
pub fn task_status_request(task_id: &str) -> HttpRequest {
    let path = GET_TASK_STATUS_PATH_TEMPLATE.replace("{task_id}", task_id);
    HttpRequest::get(format!("{BASE_URL}{path}"))
}

/// Parse the JSON body returned from a status read. Defensive: any unknown
/// `status` string maps to [`TaskState::Running`] so the polling loop never
/// finishes a task on an unrecognized state.
pub fn parse_task_status(body: &serde_json::Value) -> Result<TaskStatus> {
    let id = body
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("task status envelope missing `id`"))?
        .to_string();
    let state = match body.get("status").and_then(|v| v.as_str()) {
        Some("queued") => TaskState::Queued,
        Some("completed") | Some("succeeded") => TaskState::Completed,
        Some("failed") | Some("error") => TaskState::Failed,
        // Includes `Some("running")`, `None`, and any unknown future value.
        _ => TaskState::Running,
    };
    let result_url = body
        .get("result")
        .and_then(|r| r.get("pr_url"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let error_message = body
        .get("error")
        .and_then(|e| e.get("message"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    Ok(TaskStatus {
        id,
        state,
        result_url,
        error_message,
    })
}

/// Parse a `create_task` response into the opaque task id the caller will
/// poll on.
pub fn parse_create_task_response(body: &serde_json::Value) -> Result<String> {
    body.get("id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| anyhow!("create_task response missing `id`"))
}

/// Map an HTTP status code from the Codex Cloud REST surface onto a stable,
/// user-facing guidance string. Used by the daemon's "agent not ready"
/// path so we never escalate to Bluey's own AI on a vendor error.
///
/// The exact Codex error envelope is undocumented — these messages cover
/// the OpenAI-platform convention (401 = bad/revoked token, 403 = lacks
/// entitlement, 429 = rate limited, 5xx = transient). NEEDS-LIVE-VERIFY.
pub fn guidance_for_status(status: u16) -> &'static str {
    match status {
        401 => {
            "Your OpenAI API key was rejected by Codex Cloud. Reconnect Codex Cloud from \
                Bluey settings."
        }
        403 => {
            "Your OpenAI account doesn't have Codex Cloud access. Confirm your ChatGPT \
                plan or API entitlements at platform.openai.com."
        }
        404 => "Codex Cloud could not find that task. It may have expired.",
        409 => {
            "Codex Cloud could not start the task — your GitHub integration may not be \
                connected. Open chatgpt.com/codex/settings to set it up."
        }
        422 => "Bluey sent Codex Cloud a malformed task body (internal error).",
        429 => "Codex Cloud is rate-limiting your account. Try again in a few minutes.",
        500..=599 => "Codex Cloud is temporarily unavailable.",
        _ => "Codex Cloud rejected the request.",
    }
}

/// Convert a [`HttpResponse`] into the rendered guidance string the daemon
/// shows the user when Codex Cloud rejects a request. Pure: takes the
/// status from the response and looks up the message; the body is
/// inspected only for an optional verbatim error message the vendor sent.
pub fn render_error_message(response: &HttpResponse) -> String {
    let base = guidance_for_status(response.status);
    // Best-effort: if the body is JSON with a `.error.message` field, append
    // it. Never panic on a malformed envelope.
    if let Ok(value) = serde_json::from_slice::<serde_json::Value>(&response.body) {
        if let Some(detail) = value
            .get("error")
            .and_then(|e| e.get("message"))
            .and_then(|m| m.as_str())
        {
            return format!("{base} ({detail})");
        }
    }
    base.to_string()
}

/// Convenience: fire the create-task call using the supplied transport +
/// credential store. Async because the transport is async, but the function
/// itself is a thin wrapper — useful from integration tests and from the
/// daemon's cloud-task route.
pub async fn create_task(
    transport: &CloudHttpsTransport,
    creds: &dyn super::keychain::VendorCredentialStore,
    task_prompt: &str,
    repo_url: &str,
    branch: &str,
    runtime: Option<&str>,
) -> Result<String> {
    let request = create_task_request(task_prompt, repo_url, branch, runtime);
    let response = transport
        .send(
            VENDOR,
            CREATE_TASK_PATH,
            &request,
            BEARER_AUTH,
            creds,
            CREDENTIAL_KEY,
        )
        .await?;
    if !(200..300).contains(&response.status) {
        return Err(anyhow!("{}", render_error_message(&response)));
    }
    let body: serde_json::Value = serde_json::from_slice(&response.body)
        .map_err(|e| anyhow!("Codex Cloud returned non-JSON response: {e}"))?;
    parse_create_task_response(&body)
}

/// Convenience: fire the status-read call using the supplied transport +
/// credential store.
pub async fn read_task_status(
    transport: &CloudHttpsTransport,
    creds: &dyn super::keychain::VendorCredentialStore,
    task_id: &str,
) -> Result<TaskStatus> {
    let request = task_status_request(task_id);
    // The audit log carries the path TEMPLATE (with `{task_id}`) rather than
    // the rendered URL, so a downstream log aggregator can pivot on
    // endpoint without a per-task-id cardinality explosion.
    let response = transport
        .send(
            VENDOR,
            GET_TASK_STATUS_PATH_TEMPLATE,
            &request,
            BEARER_AUTH,
            creds,
            CREDENTIAL_KEY,
        )
        .await?;
    if !(200..300).contains(&response.status) {
        return Err(anyhow!("{}", render_error_message(&response)));
    }
    let body: serde_json::Value = serde_json::from_slice(&response.body)
        .map_err(|e| anyhow!("Codex Cloud returned non-JSON status: {e}"))?;
    parse_task_status(&body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cloud::keychain::{MemoryCredentialStore, VendorCredentialStore};

    // ---- request body shape: this is the contract NEEDS-LIVE-VERIFY guards ----

    #[test]
    fn create_task_body_matches_documented_shape() {
        let body = build_create_task_body(
            "Refactor this module for TypeScript",
            "https://github.com/user/repo",
            "main",
            Some("node:18"),
        );
        // Each field is asserted individually so a future drift surfaces the
        // exact field name that changed. Per the apidog digest:
        //   {
        //     "task_prompt": "...",
        //     "environment": { "runtime": "node:18" },
        //     "repository_context": "<url>/<branch>",
        //     "webhook": null
        //   }
        assert_eq!(
            body["task_prompt"].as_str(),
            Some("Refactor this module for TypeScript"),
        );
        assert_eq!(body["environment"]["runtime"].as_str(), Some("node:18"));
        assert_eq!(
            body["repository_context"].as_str(),
            Some("https://github.com/user/repo/main"),
        );
        assert!(body["webhook"].is_null());
    }

    #[test]
    fn create_task_body_omits_runtime_when_absent() {
        let body = build_create_task_body("do work", "https://github.com/x/y", "main", None);
        // Empty environment object — NOT a missing field, since the docs
        // show `environment` as always present.
        assert!(body["environment"].is_object());
        assert!(body["environment"].get("runtime").is_none());
    }

    #[test]
    fn create_task_request_url_is_documented_path() {
        let req = create_task_request("p", "https://github.com/u/r", "main", None);
        assert_eq!(req.url, "https://api.openai.com/v1/codex/cloud/tasks");
        assert_eq!(req.method, reqwest::Method::POST);
        assert!(req.json_body.is_some());
    }

    // ---- status URL template + path rendering ----

    #[test]
    fn task_status_request_substitutes_id() {
        let req = task_status_request("task_xyz");
        assert_eq!(
            req.url,
            "https://api.openai.com/v1/codex/cloud/tasks/task_xyz",
        );
        assert_eq!(req.method, reqwest::Method::GET);
        assert!(req.json_body.is_none());
    }

    #[test]
    fn task_status_template_substitutes_via_cloud_endpoints_helper() {
        // The shared CloudEndpoints helper must produce the same rendered
        // path the adapter constructs by hand — that's the contract that
        // keeps us data-driven.
        let rendered = CloudEndpoints::render_task(GET_TASK_STATUS_PATH_TEMPLATE, "task_abc");
        assert_eq!(rendered, "/v1/codex/cloud/tasks/task_abc");
    }

    // ---- response parsing ----

    #[test]
    fn parse_create_task_response_extracts_id() {
        let body = json!({ "id": "task_AbCdEf" });
        assert_eq!(parse_create_task_response(&body).unwrap(), "task_AbCdEf");
    }

    #[test]
    fn parse_create_task_response_rejects_missing_id() {
        let body = json!({});
        assert!(parse_create_task_response(&body).is_err());
    }

    #[test]
    fn parse_status_recognizes_completed_with_pr_url() {
        let body = json!({
            "id": "task_X",
            "status": "completed",
            "result": { "pr_url": "https://github.com/u/r/pull/42" },
        });
        let s = parse_task_status(&body).unwrap();
        assert_eq!(s.id, "task_X");
        assert_eq!(s.state, TaskState::Completed);
        assert_eq!(
            s.result_url.as_deref(),
            Some("https://github.com/u/r/pull/42"),
        );
        assert!(s.error_message.is_none());
    }

    #[test]
    fn parse_status_recognizes_failed_with_message() {
        let body = json!({
            "id": "task_Y",
            "status": "failed",
            "error": { "message": "repo clone failed" },
        });
        let s = parse_task_status(&body).unwrap();
        assert_eq!(s.state, TaskState::Failed);
        assert_eq!(s.error_message.as_deref(), Some("repo clone failed"));
    }

    #[test]
    fn parse_status_treats_unknown_string_as_running() {
        // Defensive: a future vendor status (`"compacting"`, `"reviewing"`)
        // must never be parsed as Completed and bail out of the polling
        // loop. The contract is: only EXPLICIT terminal strings end the loop.
        let body = json!({ "id": "task_Z", "status": "some_future_state" });
        let s = parse_task_status(&body).unwrap();
        assert_eq!(s.state, TaskState::Running);
    }

    #[test]
    fn parse_status_missing_status_field_is_running() {
        let body = json!({ "id": "task_Q" });
        let s = parse_task_status(&body).unwrap();
        assert_eq!(s.state, TaskState::Running);
    }

    #[test]
    fn parse_status_rejects_missing_id() {
        let body = json!({ "status": "completed" });
        assert!(parse_task_status(&body).is_err());
    }

    // ---- guidance strings (no panic on any status) ----

    #[test]
    fn guidance_distinguishes_401_403_429_5xx() {
        let g401 = guidance_for_status(401);
        let g403 = guidance_for_status(403);
        let g429 = guidance_for_status(429);
        let g500 = guidance_for_status(500);
        let g502 = guidance_for_status(502);
        // All distinct so the UI can tell the user the actual problem.
        assert_ne!(g401, g403);
        assert_ne!(g403, g429);
        assert_ne!(g429, g500);
        assert_eq!(g500, g502, "all 5xx codes share the same guidance");
        for g in [g401, g403, g429, g500] {
            assert!(!g.is_empty());
        }
    }

    #[test]
    fn guidance_for_unknown_status_falls_through() {
        // An undocumented status code must still produce SOME message —
        // never panic and never empty.
        let g = guidance_for_status(418); // a tea-pot in the wild
        assert!(!g.is_empty());
    }

    #[test]
    fn render_error_message_appends_vendor_detail_when_present() {
        let body = json!({
            "error": { "message": "insufficient quota" },
        });
        let bytes = serde_json::to_vec(&body).unwrap();
        let response = HttpResponse {
            status: 429,
            body: bytes,
            request_id_present: true,
        };
        let rendered = render_error_message(&response);
        assert!(rendered.contains("rate-limit"));
        assert!(rendered.contains("insufficient quota"));
    }

    #[test]
    fn render_error_message_tolerates_non_json_body() {
        let response = HttpResponse {
            status: 500,
            body: b"<html>nginx</html>".to_vec(),
            request_id_present: false,
        };
        let rendered = render_error_message(&response);
        assert!(rendered.contains("unavailable"));
    }

    // ---- TRANSPORT registry shape ----

    #[test]
    fn transport_declares_bearer_auth() {
        // The registry row reads TRANSPORT as data — every field below is
        // part of the contract.
        match TRANSPORT {
            CloudTransport::Https {
                base_url,
                auth,
                endpoints,
                ..
            } => {
                assert_eq!(base_url, "https://api.openai.com");
                assert_eq!(
                    auth,
                    CloudAuth::HeaderToken {
                        header_name: "Authorization",
                        prefix: "Bearer ",
                    }
                );
                assert_eq!(endpoints.create_task, Some(CREATE_TASK_PATH));
                assert_eq!(
                    endpoints.get_task_status,
                    Some(GET_TASK_STATUS_PATH_TEMPLATE),
                );
                // Session-shape fields MUST be absent for a task-shaped
                // vendor — never confuse the two.
                assert!(endpoints.create_session.is_none());
                assert!(endpoints.stream_events.is_none());
            }
            CloudTransport::None => panic!("Codex Cloud row must be Https-shaped"),
        }
    }

    // ---- consent text is the BYOT disclosure ----

    #[test]
    fn consent_text_states_billing_and_data_promise() {
        let text = OPENAI_BYOT_CONSENT_TEXT;
        // The two non-obvious promises the user MUST see.
        assert!(text.to_lowercase().contains("api credits"));
        assert!(text.to_lowercase().contains("not your chatgpt"));
        assert!(text.to_lowercase().contains("sandbox"));
    }

    // ---- transport round-trip with the memory credential store ----

    #[tokio::test]
    async fn create_task_returns_error_on_4xx() {
        // Fire the adapter against a mock HTTP server returning 401. The
        // adapter must NOT panic, must NOT loop, must surface a guidance
        // string — and NEVER reflect the token.
        use wiremock::matchers::{header, method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/codex/cloud/tasks"))
            .and(header("Authorization", "Bearer sk-test-abc"))
            .respond_with(
                ResponseTemplate::new(401)
                    .set_body_string(r#"{"error":{"message":"invalid key"}}"#),
            )
            .mount(&server)
            .await;

        // Build a transport pointed at the mock by hand (the real
        // adapter's `create_task` uses BASE_URL; we bypass for the test).
        let creds = MemoryCredentialStore::new("codex_cloud");
        creds.save(CREDENTIAL_KEY, "sk-test-abc").unwrap();
        let transport = CloudHttpsTransport::new();
        let request = HttpRequest::post_json(
            format!("{}/v1/codex/cloud/tasks", server.uri()),
            build_create_task_body("do work", "https://github.com/u/r", "main", None),
        );
        let response = transport
            .send(
                VENDOR,
                CREATE_TASK_PATH,
                &request,
                BEARER_AUTH,
                &creds,
                CREDENTIAL_KEY,
            )
            .await
            .expect("transport returned no result");
        assert_eq!(response.status, 401);
        let rendered = render_error_message(&response);
        assert!(rendered.contains("rejected") || rendered.contains("Reconnect"));
        assert!(
            !rendered.contains("sk-test-abc"),
            "MUST never reflect the token"
        );
    }

    #[tokio::test]
    async fn create_task_round_trips_id_on_2xx() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/codex/cloud/tasks"))
            .respond_with(ResponseTemplate::new(201).set_body_string(r#"{"id":"task_001"}"#))
            .mount(&server)
            .await;

        let creds = MemoryCredentialStore::new("codex_cloud");
        creds.save(CREDENTIAL_KEY, "sk-test-xyz").unwrap();
        let transport = CloudHttpsTransport::new();
        let request = HttpRequest::post_json(
            format!("{}/v1/codex/cloud/tasks", server.uri()),
            build_create_task_body("hello", "https://github.com/u/r", "main", None),
        );
        let response = transport
            .send(
                VENDOR,
                CREATE_TASK_PATH,
                &request,
                BEARER_AUTH,
                &creds,
                CREDENTIAL_KEY,
            )
            .await
            .expect("transport call failed");
        assert_eq!(response.status, 201);
        let value: serde_json::Value = serde_json::from_slice(&response.body).unwrap();
        assert_eq!(parse_create_task_response(&value).unwrap(), "task_001");
    }
}
