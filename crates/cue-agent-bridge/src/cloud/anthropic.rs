//! Anthropic **Claude Managed Agents** vendor adapter.
//!
//! See `docs/vendors/anthropic_managed.md` for the full dossier covering auth,
//! billing, endpoints, SSE wire format, error taxonomy, and the BYOT policy
//! constraint. This module holds the per-vendor irreducible bits — request
//! body builders, the SSE event parser, request-shape constructors, and the
//! key-format validator that REJECTS subscription OAuth tokens
//! (`sk-ant-oat01-*`).
//!
//! Everything generic — auth header construction, the HTTPS dispatcher, the
//! keychain wrapper, the audit log, the cloud registry row shape — lives in
//! [`super::transport`], [`super::keychain`], [`super::audit`], and
//! [`super::registry`]. This file never names "anthropic" or "claude" in
//! cross-vendor dispatch logic; the registry row's data carries the base URL,
//! beta header, auth shape, and endpoint paths.
//!
//! # The OpenClaw / subscription-OAuth ban (CRITICAL)
//!
//! Anthropic's Consumer Terms of Service explicitly forbid third-party tools
//! from using Claude Pro/Max subscription OAuth tokens:
//!
//! > *"Using OAuth tokens obtained through Claude Free, Pro, or Max accounts
//! > in any other product, tool, or service — including the Agent SDK — is
//! > not permitted and constitutes a violation of the Consumer Terms of
//! > Service."*
//!
//! Bluey enforces this in [`validate_api_key`]: a credential beginning with
//! `sk-ant-oat01-` is REJECTED at attach time with a typed
//! [`KeyValidationError::SubscriptionOAuth`] error so the user gets a clear
//! "wrong key type" message instead of a 401 storm hours later. The user
//! MUST paste a Console API key (`sk-ant-api03-*`) from
//! <https://platform.claude.com/settings/keys>.
//!
//! # BYOT billing disclosure (CRITICAL)
//!
//! Managed Agents bills per token to the user's Console API key — NOT to
//! their Claude Pro/Max subscription. The disclosure text
//! ([`BYOT_CONSENT_TEXT`]) MUST be shown to the user before Bluey stores
//! their key. The registry row's `billing_model` is
//! [`super::registry::BillingModel::ApiCredits`] so the disclosure UI flows
//! through the same code path as any other API-credits vendor.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::registry::{BillingModel, CloudAgentEntry};
use super::transport::{
    CloudAuth, CloudEndpoints, CloudHeaders, CloudTransport, HttpRequest, BEARER_AUTH,
};
use crate::registry::KindTag;

// ---- vendor identity constants -----------------------------------------

/// Vendor short name (keychain service suffix + audit log `vendor` field).
/// Lowercase ASCII to match the cross-vendor convention.
pub const VENDOR: &str = "anthropic";

/// Display name surfaced in the UI. **Branding-compliant** per Anthropic's
/// guideline ([Reference, "Branding guidelines"](https://platform.claude.com/docs/en/managed-agents/reference#branding-guidelines)):
/// the allowed string for a dropdown is *"Claude Agent"*; "Claude Code",
/// "Claude Code Agent", "Claude Cowork", and "Claude Cowork Agent" are
/// explicitly disallowed for partners. We append "(Cloud)" so the row is
/// distinguishable from the local `Claude Code (CLI)` row.
pub const DISPLAY_NAME: &str = "Claude Agent (Cloud)";

/// Base URL of the Anthropic API. No trailing slash.
pub const BASE_URL: &str = "https://api.anthropic.com";

/// Required beta header for every Managed-Agents request. Carried on the
/// transport row as DATA so the dispatcher never names the vendor —
/// duplicated here only as a stable constant for the registry-row construction.
pub const BETA_HEADER_VALUE: &str = "managed-agents-2026-04-01";

/// API version header value Anthropic requires on every call.
pub const VERSION_HEADER_VALUE: &str = "2023-06-01";

/// Static request headers carried on every Managed-Agents call. The beta
/// header is expressed AS DATA here, so it never lives in a code branch.
pub const REQUIRED_HEADERS: CloudHeaders = &[
    ("anthropic-version", VERSION_HEADER_VALUE),
    ("anthropic-beta", BETA_HEADER_VALUE),
];

/// Keychain credential key under which Bluey stores the user's Console API
/// key. Full keychain path: `bluey_cloud_anthropic / api_key`.
pub const CREDENTIAL_KEY: &str = "api_key";

// ---- endpoint paths ----------------------------------------------------

pub const CREATE_AGENT_PATH: &str = "/v1/agents";
pub const CREATE_ENVIRONMENT_PATH: &str = "/v1/environments";
pub const CREATE_SESSION_PATH: &str = "/v1/sessions";
pub const SEND_EVENT_PATH_TEMPLATE: &str = "/v1/sessions/{session_id}/events";
pub const STREAM_EVENTS_PATH_TEMPLATE: &str = "/v1/sessions/{session_id}/stream";
pub const DELETE_SESSION_PATH_TEMPLATE: &str = "/v1/sessions/{session_id}";

// ---- default agent / model / system prompt -----------------------------

/// Default model id used when Bluey provisions its per-install agent. Spelled
/// here, NOT in the registry row, because it's a "what we send in the
/// create-agent body" concern. Swap freely. `claude-opus-4-8` matches the
/// Anthropic quickstart's recommended model.
pub const DEFAULT_MODEL_ID: &str = "claude-opus-4-8";

/// Default system prompt for the Bluey-managed agent. Generic on purpose —
/// per-question payloads carry the actual meeting context.
pub const DEFAULT_SYSTEM_PROMPT: &str =
    "You are a helpful coding assistant driven by Bluey. The user is in a meeting; \
     answers should be concise and directly address the question asked.";

/// Tool-type literal Anthropic requires to enable the pre-built agent
/// toolset (bash, file ops, web search, …). Versioned with the beta header.
pub const AGENT_TOOLSET_TYPE: &str = "agent_toolset_20260401";

/// Deterministic name Bluey uses for the per-install agent resource. Sending
/// the same name twice creates two agents; the daemon's provision flow
/// resolves the existing id by name rather than creating a duplicate.
pub const BLUEY_AGENT_NAME: &str = "bluey-cowork";

/// Deterministic name Bluey uses for the per-install environment resource.
pub const BLUEY_ENVIRONMENT_NAME: &str = "bluey-env";

// ---- BYOT consent text -------------------------------------------------

/// Mandatory consent text the UI MUST show to the user before Bluey stores
/// their Console API key. Two non-obvious promises are encoded here:
///
/// 1. **Billing is BYOT** — every token is billed to the user's Console
///    account, NOT their Claude Pro/Max subscription.
/// 2. **Managed Agents are not ZDR-eligible** — Anthropic holds session
///    state server-side, so this surface is ineligible for Zero Data
///    Retention or HIPAA BAA coverage. The user can delete sessions via
///    the API at any time.
pub const BYOT_CONSENT_TEXT: &str =
    "Claude Agent (Cloud) uses your Anthropic Console API key. Every message Bluey \
     sends is billed by Anthropic at your tier's per-token rate against the API key \
     you paste below. This is NOT your Claude Pro or Max subscription — Pro/Max \
     billing only applies inside Anthropic's own apps (claude.ai, Claude Code). \
     Spend appears on platform.claude.com/usage; revoke the key at \
     platform.claude.com/settings/keys at any time. Managed Agents sessions are \
     stateful, so they're NOT eligible for Zero Data Retention or HIPAA BAA \
     coverage — Anthropic holds the conversation and sandbox state until you \
     delete the session. Bluey stores the key in your OS keychain; it never \
     leaves this machine except to call api.anthropic.com directly.";

/// Vendor-documented soft ceiling on a managed-agent session's wall-clock
/// life. The doc doesn't pin a hard number; we use a 30-minute envelope that
/// matches the Codex Cloud row for parity.
pub const MAX_SESSION_DURATION_SECS: u32 = 30 * 60;

// ---- registry row ------------------------------------------------------

/// The cloud-registry row for Anthropic Managed Agents. Replaces the
/// placeholder the parallel Copilot agent shipped. Marked `task_shaped: false`
/// because Anthropic's surface is session/turn-shaped — distinct from
/// Copilot Cloud and Codex Cloud, which are task-shaped.
///
/// `kind_tag: KindTag::AnthropicCloud` keeps this row distinct from the
/// LOCAL `KindTag::ClaudeCode` (the `claude` CLI on the user's machine) so
/// the daemon's route logic can dispatch each independently.
pub const ENTRY: &CloudAgentEntry = &CloudAgentEntry {
    kind_tag: KindTag::AnthropicCloud,
    display_name: DISPLAY_NAME,
    vendor_short: VENDOR,
    base_url: BASE_URL,
    // BYOT-style billing: the user's Console API key is charged per token.
    // The disclosure UI MUST surface `BYOT_CONSENT_TEXT` (the `consent_warning`
    // field below) before storing the credential.
    billing_model: BillingModel::ApiCredits,
    consent_warning: BYOT_CONSENT_TEXT,
    task_shaped: false,
    max_task_duration_secs: MAX_SESSION_DURATION_SECS,
};

/// `CloudTransport` shape Bluey reads when dispatching to this vendor. Pure
/// data; carries the base URL, the required static headers (including the
/// beta header), the auth-header model, and the endpoint paths.
///
/// Adding this row's transport is what makes the dispatcher "know" about
/// Anthropic — no per-vendor `if` branch anywhere else.
pub const TRANSPORT: CloudTransport = CloudTransport::Https {
    base_url: BASE_URL,
    headers: REQUIRED_HEADERS,
    auth: CloudAuth::HeaderToken {
        header_name: "x-api-key",
        prefix: "",
    },
    endpoints: CloudEndpoints {
        create_agent: Some(CREATE_AGENT_PATH),
        create_environment: Some(CREATE_ENVIRONMENT_PATH),
        create_session: Some(CREATE_SESSION_PATH),
        send_event: Some(SEND_EVENT_PATH_TEMPLATE),
        stream_events: Some(STREAM_EVENTS_PATH_TEMPLATE),
        delete_session: Some(DELETE_SESSION_PATH_TEMPLATE),
        // Anthropic's surface is session-shaped, not task-shaped.
        create_task: None,
        get_task_status: None,
    },
};

// `BEARER_AUTH` is intentionally unused by the Anthropic vendor (Anthropic
// uses `x-api-key`, not Bearer); the re-export above is just to keep the
// dispatch surface consistent across cloud adapters.
#[allow(dead_code)]
const _BEARER_AUTH_IS_REEXPORTED: CloudAuth = BEARER_AUTH;

// ---- key validation ----------------------------------------------------

/// Reasons a presented credential string is rejected at attach-time.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum KeyValidationError {
    #[error("API key is empty")]
    Empty,
    /// The key prefix matches a Claude Pro/Max subscription OAuth token,
    /// which Anthropic's Consumer Terms of Service forbids in third-party
    /// products. The user must paste a Console API key (`sk-ant-api03-…`)
    /// instead.
    #[error(
        "That's a Claude subscription OAuth token (sk-ant-oat01-*). Bluey needs a Console API key \
         starting with sk-ant-api03-*. Generate one at https://platform.claude.com/settings/keys. \
         Using a subscription token in third-party tools violates Anthropic's Consumer Terms of \
         Service and the Messages/Managed-Agents APIs will reject it with 401."
    )]
    SubscriptionOAuth,
}

/// Validate a credential string at attach time. Accepts the documented
/// Console-key prefix (`sk-ant-api03-`), accepts anything else that's
/// non-empty (Anthropic may add new prefixes), and REJECTS the subscription
/// OAuth prefix per policy.
///
/// Returns `Ok(())` if the key is plausibly a Console API key; the actual
/// 401-vs-200 verdict comes from the live API call (intentional — the
/// adapter never tries to be the source of truth on Anthropic's key format).
pub fn validate_api_key(key: &str) -> Result<(), KeyValidationError> {
    let trimmed = key.trim();
    if trimmed.is_empty() {
        return Err(KeyValidationError::Empty);
    }
    // The OAuth prefix is the single hardcoded rejection. Be liberal in what
    // we accept (future Console-key prefixes are fine) and strict in what we
    // reject (only the documented subscription-OAuth shape).
    if trimmed.starts_with("sk-ant-oat01-") {
        return Err(KeyValidationError::SubscriptionOAuth);
    }
    Ok(())
}

// ---- request-body builders --------------------------------------------

/// Body for `POST /v1/agents`. Field names match the documented JSON shape
/// EXACTLY (see `docs/vendors/anthropic_managed.md` §3).
#[derive(Debug, Clone, Serialize)]
pub struct CreateAgentBody<'a> {
    pub name: &'a str,
    pub model: &'a str,
    pub system: &'a str,
    pub tools: Vec<AgentToolEntry<'a>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AgentToolEntry<'a> {
    #[serde(rename = "type")]
    pub kind: &'a str,
}

impl<'a> CreateAgentBody<'a> {
    /// Build the default Bluey agent: the recommended model, the generic
    /// system prompt, the pre-built tool catalog.
    pub fn bluey_default(name: &'a str) -> Self {
        Self {
            name,
            model: DEFAULT_MODEL_ID,
            system: DEFAULT_SYSTEM_PROMPT,
            tools: vec![AgentToolEntry {
                kind: AGENT_TOOLSET_TYPE,
            }],
        }
    }
}

/// Body for `POST /v1/environments`. Bluey uses the default cloud sandbox
/// with unrestricted networking (matches the quickstart's `quickstart-env`).
#[derive(Debug, Clone, Serialize)]
pub struct CreateEnvironmentBody<'a> {
    pub name: &'a str,
    pub config: EnvironmentConfig,
}

#[derive(Debug, Clone, Serialize)]
pub struct EnvironmentConfig {
    #[serde(rename = "type")]
    pub kind: &'static str,
    pub networking: NetworkingConfig,
}

#[derive(Debug, Clone, Serialize)]
pub struct NetworkingConfig {
    #[serde(rename = "type")]
    pub kind: &'static str,
}

impl<'a> CreateEnvironmentBody<'a> {
    /// Build the default Bluey environment: cloud, unrestricted networking.
    pub fn bluey_default(name: &'a str) -> Self {
        Self {
            name,
            config: EnvironmentConfig {
                kind: "cloud",
                networking: NetworkingConfig {
                    kind: "unrestricted",
                },
            },
        }
    }
}

/// Body for `POST /v1/sessions`. Title is optional; Bluey populates it so
/// the session is identifiable in the Console.
#[derive(Debug, Clone, Serialize)]
pub struct CreateSessionBody<'a> {
    pub agent: &'a str,
    pub environment_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<&'a str>,
}

/// Body for `POST /v1/sessions/{id}/events`. The events array can carry
/// multiple events per call; Bluey currently sends one `user.message` at a time.
#[derive(Debug, Clone, Serialize)]
pub struct SendEventBody {
    pub events: Vec<UserEvent>,
}

/// Discriminated union over the user event types Bluey emits. Tagged on
/// `type` per the documented JSON shape.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum UserEvent {
    #[serde(rename = "user.message")]
    Message { content: Vec<ContentBlock> },
    #[serde(rename = "user.interrupt")]
    Interrupt,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum ContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
}

impl SendEventBody {
    /// Build a single-`user.message` event with one text block.
    pub fn single_text(text: impl Into<String>) -> Self {
        Self {
            events: vec![UserEvent::Message {
                content: vec![ContentBlock::Text { text: text.into() }],
            }],
        }
    }

    /// Build a single `user.interrupt` event.
    pub fn interrupt() -> Self {
        Self {
            events: vec![UserEvent::Interrupt],
        }
    }
}

// ---- HttpRequest constructors (the shape the transport fires) ---------

/// Build the `POST /v1/agents` request the transport will fire.
pub fn create_agent_request(body: CreateAgentBody<'_>) -> HttpRequest {
    HttpRequest::post_json(
        format!("{BASE_URL}{CREATE_AGENT_PATH}"),
        serde_json::to_value(&body).expect("CreateAgentBody serializes"),
    )
    .with_header("anthropic-version", VERSION_HEADER_VALUE)
    .with_header("anthropic-beta", BETA_HEADER_VALUE)
}

/// Build the `POST /v1/environments` request.
pub fn create_environment_request(body: CreateEnvironmentBody<'_>) -> HttpRequest {
    HttpRequest::post_json(
        format!("{BASE_URL}{CREATE_ENVIRONMENT_PATH}"),
        serde_json::to_value(&body).expect("CreateEnvironmentBody serializes"),
    )
    .with_header("anthropic-version", VERSION_HEADER_VALUE)
    .with_header("anthropic-beta", BETA_HEADER_VALUE)
}

/// Build the `POST /v1/sessions` request.
pub fn create_session_request(body: CreateSessionBody<'_>) -> HttpRequest {
    HttpRequest::post_json(
        format!("{BASE_URL}{CREATE_SESSION_PATH}"),
        serde_json::to_value(&body).expect("CreateSessionBody serializes"),
    )
    .with_header("anthropic-version", VERSION_HEADER_VALUE)
    .with_header("anthropic-beta", BETA_HEADER_VALUE)
}

/// Build the `POST /v1/sessions/{session_id}/events` request carrying one
/// `user.message`.
pub fn send_event_request(session_id: &str, body: SendEventBody) -> HttpRequest {
    let path = CloudEndpoints::render(SEND_EVENT_PATH_TEMPLATE, session_id);
    HttpRequest::post_json(
        format!("{BASE_URL}{path}"),
        serde_json::to_value(&body).expect("SendEventBody serializes"),
    )
    .with_header("anthropic-version", VERSION_HEADER_VALUE)
    .with_header("anthropic-beta", BETA_HEADER_VALUE)
}

/// Build the `GET /v1/sessions/{session_id}/stream` request that opens the
/// SSE stream. The default `GET` constructor is augmented with
/// `Accept: text/event-stream` (the documented requirement).
pub fn stream_events_request(session_id: &str) -> HttpRequest {
    let path = CloudEndpoints::render(STREAM_EVENTS_PATH_TEMPLATE, session_id);
    HttpRequest::get(format!("{BASE_URL}{path}"))
        .with_header("anthropic-version", VERSION_HEADER_VALUE)
        .with_header("anthropic-beta", BETA_HEADER_VALUE)
        .with_header("Accept", "text/event-stream")
}

/// Build the `DELETE /v1/sessions/{session_id}` request. Anthropic returns a
/// JSON envelope on delete; the body field is empty (content-type is not
/// applied for body-less requests).
pub fn delete_session_request(session_id: &str) -> HttpRequest {
    let path = CloudEndpoints::render(DELETE_SESSION_PATH_TEMPLATE, session_id);
    HttpRequest {
        method: reqwest::Method::DELETE,
        url: format!("{BASE_URL}{path}"),
        json_body: None,
        extra_headers: vec![
            (
                "anthropic-version".to_string(),
                VERSION_HEADER_VALUE.to_string(),
            ),
            ("anthropic-beta".to_string(), BETA_HEADER_VALUE.to_string()),
        ],
        timeout: None,
    }
}

// ---- SSE event parsing -------------------------------------------------

/// Typed Managed-Agents SSE event, subset of the documented vocabulary that
/// Bluey actually consumes. Unknown event types are surfaced as
/// [`AnthropicEvent::Unknown`] (fail-soft, no panic) so a doc revision
/// adding new types doesn't crash the daemon.
#[derive(Debug, Clone, PartialEq)]
pub enum AnthropicEvent {
    /// `agent.message` — agent text reply. `text` is the concatenation of
    /// every `text`-typed block in the `content` array.
    AgentMessage { text: String },
    /// `agent.thinking` — extended-thinking content, kept separately from
    /// the answer text.
    AgentThinking { text: String },
    /// `agent.tool_use` — agent invoked a pre-built tool (bash, file ops,
    /// …).
    AgentToolUse { name: String },
    /// `session.status_idle` — agent finished its current task and is
    /// waiting for input. Carries the documented `stop_reason`.
    SessionStatusIdle { stop_reason: Option<String> },
    /// `session.status_terminated` — session ended on unrecoverable error.
    SessionStatusTerminated,
    /// `session.error` — typed error during processing. Carries the
    /// documented `error.type` / `error.message` / `retry_status` fields.
    SessionError {
        error_type: Option<String>,
        message: Option<String>,
        retry_status: Option<String>,
    },
    /// Any event Bluey doesn't model explicitly. The `type` is preserved so
    /// the audit log knows what was skipped.
    Unknown { event_type: String },
}

/// Errors raised while parsing the SSE stream or HTTP error bodies.
#[derive(Debug, Error, PartialEq)]
pub enum AnthropicError {
    #[error("SSE line was not valid JSON: {0}")]
    BadJson(String),
    #[error("API returned {status} ({error_type}): {message}")]
    Api {
        status: u16,
        error_type: String,
        message: String,
        request_id: Option<String>,
    },
}

/// Parse a single `data:` payload (with the `data: ` prefix already
/// stripped) into a typed [`AnthropicEvent`].
///
/// The SSE wire is line-oriented: `event: <type>\n data: <json>\n\n`. Bluey
/// reads the `data:` JSON only (the `event:` line is informational; the
/// canonical type is inside the JSON's `type` field per the documented
/// shape).
pub fn parse_sse_event(data: &str) -> Result<AnthropicEvent, AnthropicError> {
    let value: serde_json::Value =
        serde_json::from_str(data.trim()).map_err(|e| AnthropicError::BadJson(e.to_string()))?;
    let event_type = value
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    match event_type.as_str() {
        "agent.message" => Ok(AnthropicEvent::AgentMessage {
            text: collect_text_from_content_array(value.get("content")),
        }),
        "agent.thinking" => {
            // `agent.thinking` carries either a `content` array (text blocks)
            // or a top-level `text` field per the docs. Try both.
            let from_content = collect_text_from_content_array(value.get("content"));
            let text = if from_content.is_empty() {
                value
                    .get("text")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string()
            } else {
                from_content
            };
            Ok(AnthropicEvent::AgentThinking { text })
        }
        "agent.tool_use" => Ok(AnthropicEvent::AgentToolUse {
            name: value
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
        }),
        "session.status_idle" => Ok(AnthropicEvent::SessionStatusIdle {
            stop_reason: value
                .get("stop_reason")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
        }),
        "session.status_terminated" => Ok(AnthropicEvent::SessionStatusTerminated),
        "session.error" => {
            let error = value.get("error");
            Ok(AnthropicEvent::SessionError {
                error_type: error
                    .and_then(|e| e.get("type"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                message: error
                    .and_then(|e| e.get("message"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                retry_status: value
                    .get("retry_status")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
            })
        }
        other => Ok(AnthropicEvent::Unknown {
            event_type: other.to_string(),
        }),
    }
}

/// Extract concatenated text from `[{"type":"text","text":"..."}, ...]`.
/// Non-text blocks are ignored. Returns the empty string for a
/// missing / non-array value.
fn collect_text_from_content_array(value: Option<&serde_json::Value>) -> String {
    let Some(arr) = value.and_then(|v| v.as_array()) else {
        return String::new();
    };
    let mut out = String::new();
    for block in arr {
        if block.get("type").and_then(|v| v.as_str()) == Some("text") {
            if let Some(text) = block.get("text").and_then(|v| v.as_str()) {
                out.push_str(text);
            }
        }
    }
    out
}

/// Iterate `event:`/`data:` line pairs in an SSE body chunk into typed
/// events. `data:` lines that fail to parse are skipped with a
/// `tracing::warn!` (the stream MUST stay open if one line is malformed).
/// Returns the typed events in order.
pub fn parse_sse_chunk(chunk: &str) -> Vec<AnthropicEvent> {
    let mut out = Vec::new();
    for line in chunk.lines() {
        let line = line.trim_end_matches('\r');
        let Some(data) = line.strip_prefix("data:") else {
            // Skip `event:`, `id:`, `:`, blank lines, and any other field
            // the SSE spec allows. We only need the JSON payload.
            continue;
        };
        let data = data.trim_start();
        if data.is_empty() {
            continue;
        }
        match parse_sse_event(data) {
            Ok(event) => out.push(event),
            Err(e) => {
                tracing::warn!(
                    target: "cue_agent_bridge::cloud::anthropic",
                    error = %e,
                    "Anthropic SSE parse failed; skipping line",
                );
            }
        }
    }
    out
}

// ---- error body parsing ------------------------------------------------

/// Documented Anthropic error body shape:
/// `{ "type": "error", "error": { "type": "...", "message": "..." },
///    "request_id": "req_..." }`
/// (see `docs/vendors/anthropic_managed.md` §7).
#[derive(Debug, Clone, Deserialize)]
struct ErrorBody {
    error: ErrorBodyInner,
    #[serde(default)]
    request_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct ErrorBodyInner {
    #[serde(rename = "type")]
    kind: String,
    message: String,
}

/// Parse an HTTP error body + status into [`AnthropicError::Api`]. Falls
/// back to a generic message when the body isn't the documented shape.
pub fn parse_error_response(status: u16, body: &str) -> AnthropicError {
    match serde_json::from_str::<ErrorBody>(body) {
        Ok(parsed) => AnthropicError::Api {
            status,
            error_type: parsed.error.kind,
            message: parsed.error.message,
            request_id: parsed.request_id,
        },
        Err(_) => AnthropicError::Api {
            status,
            error_type: "unknown".to_string(),
            message: if body.is_empty() {
                "<empty body>".to_string()
            } else {
                body.chars().take(200).collect()
            },
            request_id: None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- key validation --------------------------------------------------

    #[test]
    fn validate_api_key_accepts_console_key() {
        // Real Console API keys begin sk-ant-api03-*. Adapter must accept.
        assert!(validate_api_key("sk-ant-api03-real-looking-key").is_ok());
    }

    #[test]
    fn validate_api_key_rejects_subscription_oauth() {
        // Subscription OAuth tokens (sk-ant-oat01-*) are forbidden by
        // Anthropic's Consumer Terms — Bluey rejects at attach time with a
        // clear error rather than letting it fail with a 401 hours later.
        // See docs/vendors/anthropic_managed.md §2.
        let err = validate_api_key("sk-ant-oat01-from-claude-cli")
            .expect_err("OAuth token must be rejected");
        assert_eq!(err, KeyValidationError::SubscriptionOAuth);
        // Error message must reference the policy + the corrective action.
        let msg = err.to_string();
        assert!(msg.contains("Console API key"));
        assert!(msg.contains("sk-ant-api03"));
        assert!(msg.contains("Consumer Terms"));
    }

    #[test]
    fn validate_api_key_rejects_empty_and_whitespace() {
        assert_eq!(validate_api_key("").unwrap_err(), KeyValidationError::Empty);
        assert_eq!(
            validate_api_key("   \n  ").unwrap_err(),
            KeyValidationError::Empty,
        );
    }

    #[test]
    fn validate_api_key_accepts_unknown_prefix_liberally() {
        // Anthropic may add new prefixes; the validator is conservative —
        // only rejects the documented forbidden shape.
        assert!(validate_api_key("sk-ant-future-prefix-xyz").is_ok());
        assert!(validate_api_key("sk-ant-api04-newer").is_ok());
    }

    // ---- request-body shape (matches docs verbatim) ----------------------

    #[test]
    fn create_agent_request_matches_docs_shape() {
        // Verify the body matches the quickstart's verbatim JSON
        // (docs/vendors/anthropic_managed.md §3). Field names, casing, the
        // tool-type literal — all spelled exactly.
        let body = CreateAgentBody {
            name: "Coding Assistant",
            model: "claude-opus-4-8",
            system: "You are a helpful coding assistant. Write clean, well-documented code.",
            tools: vec![AgentToolEntry {
                kind: "agent_toolset_20260401",
            }],
        };
        let json = serde_json::to_value(&body).expect("serialize");
        assert_eq!(json["name"], "Coding Assistant");
        assert_eq!(json["model"], "claude-opus-4-8");
        assert_eq!(
            json["system"],
            "You are a helpful coding assistant. Write clean, well-documented code."
        );
        let tools = json["tools"].as_array().expect("tools array");
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0]["type"], "agent_toolset_20260401");
    }

    #[test]
    fn create_environment_request_matches_docs_shape() {
        let body = CreateEnvironmentBody::bluey_default("bluey-env");
        let json = serde_json::to_value(&body).expect("serialize");
        assert_eq!(json["name"], "bluey-env");
        assert_eq!(json["config"]["type"], "cloud");
        assert_eq!(json["config"]["networking"]["type"], "unrestricted");
    }

    #[test]
    fn create_session_request_matches_docs_shape() {
        // Session body shape: { agent, environment_id, title? }.
        let body = CreateSessionBody {
            agent: "agent_abc",
            environment_id: "env_xyz",
            title: Some("Quickstart session"),
        };
        let json = serde_json::to_value(&body).expect("serialize");
        assert_eq!(json["agent"], "agent_abc");
        assert_eq!(json["environment_id"], "env_xyz");
        assert_eq!(json["title"], "Quickstart session");

        let no_title = CreateSessionBody {
            agent: "a",
            environment_id: "e",
            title: None,
        };
        let json = serde_json::to_value(&no_title).expect("serialize");
        assert!(
            json.get("title").is_none(),
            "title must be omitted when None"
        );
    }

    #[test]
    fn send_event_request_matches_docs_shape() {
        // Verbatim quickstart JSON:
        // { "events": [ { "type": "user.message",
        //                  "content": [ { "type": "text", "text": "..." } ] } ] }
        let body = SendEventBody::single_text("Hello agent");
        let json = serde_json::to_value(&body).expect("serialize");
        let events = json["events"].as_array().expect("events array");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0]["type"], "user.message");
        let content = events[0]["content"].as_array().expect("content array");
        assert_eq!(content.len(), 1);
        assert_eq!(content[0]["type"], "text");
        assert_eq!(content[0]["text"], "Hello agent");
    }

    #[test]
    fn interrupt_event_matches_docs_shape() {
        // Interrupt is a bare `{ "type": "user.interrupt" }` — no content.
        let body = SendEventBody::interrupt();
        let json = serde_json::to_value(&body).expect("serialize");
        let events = json["events"].as_array().expect("events array");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0]["type"], "user.interrupt");
        assert!(events[0].get("content").is_none());
    }

    // ---- HttpRequest constructors carry the beta header -----------------

    #[test]
    fn create_session_http_request_carries_beta_header() {
        // The HTTPS dispatcher applies the beta header from `extra_headers`.
        // Every Anthropic request MUST carry it — that's the documented
        // gate. This test pins it as DATA, not as a code branch.
        let req = create_session_request(CreateSessionBody {
            agent: "a",
            environment_id: "e",
            title: None,
        });
        let beta = req
            .extra_headers
            .iter()
            .find(|(k, _)| k == "anthropic-beta")
            .expect("anthropic-beta header on every Managed-Agents request");
        assert_eq!(beta.1, "managed-agents-2026-04-01");
        let version = req
            .extra_headers
            .iter()
            .find(|(k, _)| k == "anthropic-version")
            .expect("anthropic-version header on every Anthropic request");
        assert_eq!(version.1, "2023-06-01");
        // The URL must hit the documented path on the documented base URL.
        assert_eq!(req.url, "https://api.anthropic.com/v1/sessions");
        assert_eq!(req.method, reqwest::Method::POST);
        assert!(req.json_body.is_some());
    }

    #[test]
    fn stream_events_http_request_is_get_with_text_event_stream() {
        // The streaming endpoint is GET; the Accept header must announce
        // text/event-stream so the server doesn't fall back to JSON. The
        // session-id template must be substituted into the URL.
        let req = stream_events_request("sess_01ABC");
        assert_eq!(
            req.url,
            "https://api.anthropic.com/v1/sessions/sess_01ABC/stream"
        );
        assert_eq!(req.method, reqwest::Method::GET);
        let accept = req
            .extra_headers
            .iter()
            .find(|(k, _)| k == "Accept")
            .expect("Accept header set");
        assert_eq!(accept.1, "text/event-stream");
        assert!(req.json_body.is_none());
        // Beta header still carried on the streaming call.
        assert!(req
            .extra_headers
            .iter()
            .any(|(k, v)| k == "anthropic-beta" && v == "managed-agents-2026-04-01"));
    }

    #[test]
    fn send_event_http_request_substitutes_session_id() {
        let req = send_event_request("sess_XYZ", SendEventBody::single_text("hi"));
        assert_eq!(
            req.url,
            "https://api.anthropic.com/v1/sessions/sess_XYZ/events"
        );
        assert_eq!(req.method, reqwest::Method::POST);
    }

    #[test]
    fn delete_session_http_request_uses_delete_method() {
        let req = delete_session_request("sess_DEL");
        assert_eq!(req.url, "https://api.anthropic.com/v1/sessions/sess_DEL");
        assert_eq!(req.method, reqwest::Method::DELETE);
        assert!(req.json_body.is_none());
    }

    // ---- SSE parser ------------------------------------------------------

    #[test]
    fn sse_parser_emits_typed_events_agent_message() {
        let json = r#"{
            "type": "agent.message",
            "content": [
                {"type": "text", "text": "Hello, "},
                {"type": "text", "text": "world."}
            ]
        }"#;
        let evt = parse_sse_event(json).unwrap();
        assert_eq!(
            evt,
            AnthropicEvent::AgentMessage {
                text: "Hello, world.".to_string()
            }
        );
    }

    #[test]
    fn sse_parser_emits_typed_events_agent_tool_use() {
        let json = r#"{ "type": "agent.tool_use", "name": "bash", "input": { "command": "ls" } }"#;
        let evt = parse_sse_event(json).unwrap();
        assert_eq!(
            evt,
            AnthropicEvent::AgentToolUse {
                name: "bash".to_string()
            }
        );
    }

    #[test]
    fn sse_parser_emits_typed_events_session_idle() {
        let json = r#"{ "type": "session.status_idle", "stop_reason": "end_turn" }"#;
        let evt = parse_sse_event(json).unwrap();
        assert_eq!(
            evt,
            AnthropicEvent::SessionStatusIdle {
                stop_reason: Some("end_turn".to_string())
            }
        );
    }

    #[test]
    fn sse_parser_emits_typed_events_session_error() {
        // `session.error` carries a typed error object + a retry_status.
        let json = r#"{
            "type": "session.error",
            "error": { "type": "rate_limit_error", "message": "slow down" },
            "retry_status": "scheduled"
        }"#;
        let evt = parse_sse_event(json).unwrap();
        match evt {
            AnthropicEvent::SessionError {
                error_type,
                message,
                retry_status,
            } => {
                assert_eq!(error_type.as_deref(), Some("rate_limit_error"));
                assert_eq!(message.as_deref(), Some("slow down"));
                assert_eq!(retry_status.as_deref(), Some("scheduled"));
            }
            other => panic!("expected SessionError, got {other:?}"),
        }
    }

    #[test]
    fn sse_parser_emits_unknown_for_unmodelled_type() {
        // A future event type Bluey doesn't model surfaces as Unknown
        // (carrying the type for the audit log) rather than crashing.
        let json = r#"{ "type": "span.outcome_evaluation_ongoing" }"#;
        let evt = parse_sse_event(json).unwrap();
        assert_eq!(
            evt,
            AnthropicEvent::Unknown {
                event_type: "span.outcome_evaluation_ongoing".to_string()
            }
        );
    }

    #[test]
    fn sse_parser_rejects_non_json_line() {
        // A `data:` line that isn't JSON is a stream error, not silent
        // garbage. The dispatcher logs + skips it.
        let err = parse_sse_event("not json at all").unwrap_err();
        match err {
            AnthropicError::BadJson(_) => {}
            other => panic!("expected BadJson, got {other:?}"),
        }
    }

    #[test]
    fn sse_chunk_parser_walks_event_data_pairs() {
        // The streaming response is `event: ...\ndata: {json}\n\n` blocks.
        // The chunk parser strips `event:` / blank lines and parses `data:`.
        let chunk = "\
event: agent.message\n\
data: {\"type\":\"agent.message\",\"content\":[{\"type\":\"text\",\"text\":\"hi\"}]}\n\
\n\
event: session.status_idle\n\
data: {\"type\":\"session.status_idle\",\"stop_reason\":\"end_turn\"}\n\
\n";
        let events = parse_sse_chunk(chunk);
        assert_eq!(events.len(), 2);
        assert_eq!(
            events[0],
            AnthropicEvent::AgentMessage {
                text: "hi".to_string()
            }
        );
        assert_eq!(
            events[1],
            AnthropicEvent::SessionStatusIdle {
                stop_reason: Some("end_turn".to_string())
            }
        );
    }

    #[test]
    fn sse_chunk_parser_skips_garbage_lines_without_dying() {
        // A malformed `data:` line must NOT take down the stream — the next
        // valid event still parses.
        let chunk = "\
data: not json\n\
\n\
data: {\"type\":\"session.status_idle\"}\n\
\n";
        let events = parse_sse_chunk(chunk);
        // The garbage line was skipped (audited via tracing::warn!); the
        // valid event still made it through.
        assert_eq!(events.len(), 1);
        assert!(matches!(
            events[0],
            AnthropicEvent::SessionStatusIdle { .. }
        ));
    }

    // ---- error body parsing ----------------------------------------------

    #[test]
    fn error_body_maps_to_typed_error() {
        // The documented Anthropic error body (verbatim quote in the dossier
        // §7). The parser must turn it into a typed Api error carrying the
        // status, the inner type, the message, and request_id.
        let body = r#"{
            "type": "error",
            "error": {
                "type": "not_found_error",
                "message": "The requested resource could not be found."
            },
            "request_id": "req_011CSHoEeqs5C35K2UUqR7Fy"
        }"#;
        let err = parse_error_response(404, body);
        match err {
            AnthropicError::Api {
                status,
                error_type,
                message,
                request_id,
            } => {
                assert_eq!(status, 404);
                assert_eq!(error_type, "not_found_error");
                assert_eq!(message, "The requested resource could not be found.");
                assert_eq!(request_id.as_deref(), Some("req_011CSHoEeqs5C35K2UUqR7Fy"));
            }
            other => panic!("expected Api, got {other:?}"),
        }
    }

    #[test]
    fn error_body_oauth_rejection_surfaces_as_typed_error() {
        // The 401 the Messages/Managed-Agents API returns when a Pro/Max
        // OAuth token is sent as x-api-key. Bluey rejects these BEFORE the
        // call (via validate_api_key), but the live API may still return
        // one if Anthropic adds new prefixes — the typed error surfaces
        // the message faithfully so the daemon can degrade.
        let body = r#"{
            "type": "error",
            "error": {
                "type": "authentication_error",
                "message": "OAuth authentication is currently not supported."
            }
        }"#;
        let err = parse_error_response(401, body);
        match err {
            AnthropicError::Api {
                status,
                error_type,
                message,
                ..
            } => {
                assert_eq!(status, 401);
                assert_eq!(error_type, "authentication_error");
                assert!(message.contains("OAuth authentication"));
            }
            other => panic!("expected Api, got {other:?}"),
        }
    }

    #[test]
    fn error_body_falls_back_for_malformed_body() {
        // Non-JSON or unexpected body: still produce an Api error, capture
        // the status, mark error_type "unknown", carry a truncated body.
        let err = parse_error_response(500, "internal server error (plain text)");
        match err {
            AnthropicError::Api {
                status,
                error_type,
                message,
                request_id,
            } => {
                assert_eq!(status, 500);
                assert_eq!(error_type, "unknown");
                assert!(message.contains("internal server error"));
                assert!(request_id.is_none());
            }
            other => panic!("expected Api, got {other:?}"),
        }
    }

    // ---- vendor identity sanity -----------------------------------------

    #[test]
    fn vendor_name_is_lowercase_ascii() {
        // VENDOR flows into keychain service names and audit log fields;
        // it MUST be lowercase ASCII or the namespace prefix breaks.
        assert!(VENDOR.chars().all(|c| c.is_ascii_lowercase()));
    }

    #[test]
    fn beta_header_value_matches_dossier() {
        // The beta header value is also stored in REQUIRED_HEADERS on the
        // transport row — they MUST stay in sync.
        assert_eq!(BETA_HEADER_VALUE, "managed-agents-2026-04-01");
        assert!(REQUIRED_HEADERS
            .iter()
            .any(|(k, v)| *k == "anthropic-beta" && *v == BETA_HEADER_VALUE));
    }

    #[test]
    fn display_name_is_branding_compliant() {
        // Anthropic's branding guidelines explicitly disallow the strings
        // "Claude Code", "Claude Code Agent", "Claude Cowork", "Claude
        // Cowork Agent" for partner products. The allowed dropdown label is
        // "Claude Agent". Pin the row's display_name against accidental
        // drift in either direction.
        assert_eq!(DISPLAY_NAME, "Claude Agent (Cloud)");
        for forbidden in ["Claude Code", "Claude Cowork"] {
            assert!(
                !DISPLAY_NAME.contains(forbidden),
                "DISPLAY_NAME {DISPLAY_NAME:?} must not contain branding-forbidden {forbidden:?}"
            );
        }
    }

    #[test]
    fn registry_row_uses_anthropic_cloud_kind_tag() {
        // The cloud row must be tagged AnthropicCloud (NOT ClaudeCode) so
        // the daemon's route logic can dispatch it independently from the
        // local `claude` CLI row.
        assert_eq!(ENTRY.kind_tag, KindTag::AnthropicCloud);
    }

    #[test]
    fn registry_row_carries_byot_consent_text() {
        // The registry's billing model must be ApiCredits (BYOT), and the
        // consent_warning MUST carry the BYOT disclosure (not the
        // placeholder). The UI flow reads consent_warning off the row.
        assert_eq!(ENTRY.billing_model, BillingModel::ApiCredits);
        assert!(!ENTRY.consent_warning.starts_with("PLACEHOLDER"));
        // The disclosure MUST mention the non-obvious BYOT facts.
        let text = ENTRY.consent_warning.to_lowercase();
        assert!(
            text.contains("anthropic console api key"),
            "consent must reference Console API key"
        );
        assert!(
            text.contains("not your claude pro"),
            "consent must distinguish from subscription"
        );
        assert!(
            text.contains("zero data retention"),
            "consent must disclose ZDR-ineligibility"
        );
        assert!(
            text.contains("keychain"),
            "consent must promise OS keychain storage"
        );
    }

    #[test]
    fn transport_carries_beta_header_and_x_api_key_auth() {
        match TRANSPORT {
            CloudTransport::Https {
                base_url,
                headers,
                auth,
                endpoints,
            } => {
                assert_eq!(base_url, "https://api.anthropic.com");
                // Beta header is DATA on the row.
                assert!(headers
                    .iter()
                    .any(|(k, v)| { *k == "anthropic-beta" && *v == "managed-agents-2026-04-01" }));
                assert!(headers
                    .iter()
                    .any(|(k, v)| *k == "anthropic-version" && *v == "2023-06-01"));
                // x-api-key, no prefix — distinguishing Anthropic from
                // Bearer-style vendors.
                assert_eq!(
                    auth,
                    CloudAuth::HeaderToken {
                        header_name: "x-api-key",
                        prefix: "",
                    }
                );
                // Session-shape endpoints are populated.
                assert_eq!(endpoints.create_agent, Some("/v1/agents"));
                assert_eq!(endpoints.create_environment, Some("/v1/environments"));
                assert_eq!(endpoints.create_session, Some("/v1/sessions"));
                assert_eq!(
                    endpoints.send_event,
                    Some("/v1/sessions/{session_id}/events")
                );
                assert_eq!(
                    endpoints.stream_events,
                    Some("/v1/sessions/{session_id}/stream")
                );
                // Task-shape endpoints are NOT populated.
                assert!(endpoints.create_task.is_none());
                assert!(endpoints.get_task_status.is_none());
            }
            CloudTransport::None => panic!("Anthropic row must be Https-shaped"),
        }
    }

    // ---- audit / live-call shape (uses MemoryCredentialStore) -----------

    #[tokio::test]
    async fn live_call_shape_against_wiremock() {
        // End-to-end shape test against a mock HTTP server: build the
        // create_session request, fire it through the real transport with a
        // MemoryCredentialStore, assert the request received matches the
        // documented Anthropic shape (URL, method, beta + version + x-api-key
        // headers). NEEDS-LIVE-VERIFY is reserved for the real Anthropic
        // server's response shape; the request shape is fully tested here.
        use crate::cloud::keychain::{MemoryCredentialStore, VendorCredentialStore};
        use crate::cloud::transport::CloudHttpsTransport;
        use wiremock::matchers::{header, method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/sessions"))
            .and(header("x-api-key", "sk-ant-api03-test"))
            .and(header("anthropic-version", "2023-06-01"))
            .and(header("anthropic-beta", "managed-agents-2026-04-01"))
            .respond_with(
                ResponseTemplate::new(201)
                    .insert_header("request-id", "req_test_001")
                    .set_body_string(r#"{"id":"sess_test_001","status":"idle"}"#),
            )
            .mount(&server)
            .await;

        // Build a request pointed at the mock by hand (the real adapter's
        // constructors use BASE_URL; we redirect for the test).
        let creds = MemoryCredentialStore::new(VENDOR);
        creds.save(CREDENTIAL_KEY, "sk-ant-api03-test").unwrap();
        let request = HttpRequest::post_json(
            format!("{}/v1/sessions", server.uri()),
            serde_json::to_value(CreateSessionBody {
                agent: "agent_x",
                environment_id: "env_y",
                title: Some("test"),
            })
            .unwrap(),
        )
        .with_header("anthropic-version", "2023-06-01")
        .with_header("anthropic-beta", "managed-agents-2026-04-01");

        let transport = CloudHttpsTransport::new();
        let response = transport
            .send(
                VENDOR,
                CREATE_SESSION_PATH,
                &request,
                CloudAuth::HeaderToken {
                    header_name: "x-api-key",
                    prefix: "",
                },
                &creds,
                CREDENTIAL_KEY,
            )
            .await
            .expect("transport call succeeds");

        assert_eq!(response.status, 201);
        assert!(
            response.request_id_present,
            "the response carried request-id; the audit log should have noted presence"
        );
        // The audit log emitted by the transport is captured by tracing; we
        // assert the body did NOT echo the credential back as a safety net.
        let body_text = response.body_text();
        assert!(
            !body_text.contains("sk-ant-api03-test"),
            "response body must not echo the credential; got {body_text}"
        );
    }

    #[tokio::test]
    async fn live_call_401_returns_typed_api_error() {
        // The 401 path: the live API returned an authentication_error. The
        // transport surfaces the response; the adapter's parse_error_response
        // turns it into a typed AnthropicError. The credential MUST NOT
        // appear anywhere in the error rendering.
        use crate::cloud::keychain::{MemoryCredentialStore, VendorCredentialStore};
        use crate::cloud::transport::CloudHttpsTransport;
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/sessions"))
            .respond_with(ResponseTemplate::new(401).set_body_string(
                r#"{"type":"error","error":{"type":"authentication_error","message":"invalid x-api-key"}}"#,
            ))
            .mount(&server)
            .await;

        let creds = MemoryCredentialStore::new(VENDOR);
        creds.save(CREDENTIAL_KEY, "sk-ant-api03-revoked").unwrap();
        let request = HttpRequest::post_json(
            format!("{}/v1/sessions", server.uri()),
            serde_json::json!({}),
        );

        let transport = CloudHttpsTransport::new();
        let response = transport
            .send(
                VENDOR,
                CREATE_SESSION_PATH,
                &request,
                CloudAuth::HeaderToken {
                    header_name: "x-api-key",
                    prefix: "",
                },
                &creds,
                CREDENTIAL_KEY,
            )
            .await
            .expect("transport call returns a response");

        assert_eq!(response.status, 401);
        let err = parse_error_response(response.status, &response.body_text());
        match err {
            AnthropicError::Api {
                status,
                error_type,
                message,
                ..
            } => {
                assert_eq!(status, 401);
                assert_eq!(error_type, "authentication_error");
                assert!(message.contains("invalid"));
                // The credential MUST NEVER appear in a rendered error.
                let rendered = message.to_string();
                assert!(
                    !rendered.contains("sk-ant-api03-revoked"),
                    "rendered error must not contain the credential"
                );
            }
            other => panic!("expected Api, got {other:?}"),
        }
    }
}
