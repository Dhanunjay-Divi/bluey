//! Google **Antigravity (Cloud)** adapter — the Managed Agents surface of the
//! Gemini API.
//!
//! See `docs/vendors/antigravity_cloud.md` for the full dossier (auth, billing,
//! endpoints, error taxonomy, the overlap-with-Gemini answer, and the
//! OPEN-QUESTIONS list). This module holds only the irreducible per-vendor bits:
//! the exact `POST /v1beta/interactions` and `POST /v1beta/agents` JSON bodies, the
//! response parser, the Google error-envelope parser, and the HTTP→guidance
//! mapping. Everything generic (HTTPS dispatch, auth-header construction, audit
//! logging, keychain access) lives in [`super::transport`], [`super::keychain`],
//! and [`super::audit`].
//!
//! # Antigravity cloud IS the Gemini API Managed Agents (CRITICAL)
//!
//! This is the most important fact about this adapter. Antigravity 2.0's cloud
//! surface is **not** a separate API: it is the Gemini API "Managed Agents"
//! Interactions endpoint
//! (`POST https://generativelanguage.googleapis.com/v1beta/interactions`),
//! invoked with the same `x-goog-api-key` auth. The ONLY thing that makes a
//! call "Antigravity" rather than a generic Gemini-cloud call is the value of
//! the `agent` field in the request body: [`DEFAULT_AGENT`]
//! (`"antigravity-preview-05-2026"`), the Antigravity agent harness built on
//! Gemini 3.5 Flash.
//!
//! Because the transport/auth/endpoints are identical to a generic Gemini-cloud
//! row, a future refactor SHOULD collapse this adapter and the parallel
//! `GeminiCloud` adapter into one shared `gemini_interactions` module
//! parameterized by the `agent` field. Until both rows exist, the small
//! request-body builder here is intentionally self-contained so the tree stays
//! green regardless of merge order — see the dossier's overlap section.
//!
//! # BYOT billing disclosure (CRITICAL)
//!
//! The interaction is billed **per Gemini token** to the user's own Gemini API
//! key (Gemini 3.5 Flash rates), NOT a flat subscription. Environment/sandbox
//! compute is free only during the preview. The disclosure text
//! ([`ANTIGRAVITY_BYOT_CONSENT_TEXT`]) MUST be shown before Bluey stores the
//! key. The registry row's `billing_model` is
//! [`super::registry::BillingModel::ApiCredits`] so the disclosure UI flows
//! through the same code path as any other API-credits vendor — no per-vendor
//! branch.

use anyhow::{anyhow, Result};
use serde_json::json;

use super::registry::{BillingModel, CloudAgentEntry};
#[cfg(test)]
use super::transport::CloudAuth;
use super::transport::{
    CloudEndpoints, CloudHeaders, CloudHttpsTransport, CloudTransport, HttpRequest, HttpResponse,
};
use crate::registry::KindTag;

// ---- vendor identity constants -----------------------------------------

/// Vendor short name — keychain service suffix (`bluey_cloud_antigravity_cloud`)
/// and the audit log's `vendor` field. Lowercase ASCII to match the convention
/// the other vendor adapters use. Distinct from the LOCAL Antigravity row's
/// identity so audit/keychain namespaces never collide.
pub const VENDOR: &str = "antigravity_cloud";

/// Display name surfaced in the UI. "(Cloud)" disambiguates this row from the
/// local `Antigravity` IDE row in `crate::registry::REGISTRY`.
pub const DISPLAY_NAME: &str = "Google Antigravity (Cloud)";

/// Base URL of the Gemini API (the AI-Studio / generativelanguage host). No
/// trailing slash. This is the same host the Managed Agents / Interactions API
/// lives on. (The Vertex mirror at `aiplatform.googleapis.com` needs Google
/// Cloud OAuth/ADC instead — see the dossier OPEN QUESTIONS; not built here.)
pub const BASE_URL: &str = "https://generativelanguage.googleapis.com";

/// Keychain credential key under which Bluey stores the user's Gemini API key.
/// Full keychain path: `bluey_cloud_antigravity_cloud / api_key`.
pub const CREDENTIAL_KEY: &str = "api_key";

/// Env var the user can paste from in their shell, mirrored to the keychain on
/// first use. Bluey reads this only as a one-shot import — the daemon never
/// writes the secret back out into the environment.
pub const CREDENTIAL_ENV_FALLBACK: &str = "GEMINI_API_KEY";

/// The stock Antigravity agent identifier. This is the SOLE differentiator
/// between an "Antigravity cloud" call and a generic Gemini-cloud call on the
/// same Interactions endpoint. A user-registered custom agent (via
/// `POST /v1beta/agents`) is invoked by passing its chosen `id` here instead.
pub const DEFAULT_AGENT: &str = "antigravity-preview-05-2026";

/// API revision header value the Antigravity Agent docs require on every
/// Interactions call. Carried as registry DATA (the transport `headers` slice)
/// so it never lives in a code branch — a Google revision bump is a one-line
/// data change. NEEDS-LIVE-VERIFY against the live API.
pub const API_REVISION_VALUE: &str = "2026-05-20";

/// Static request headers carried on every Interactions/Agents call. The
/// `Api-Revision` header is expressed AS DATA here, so it never lives in a code
/// branch.
pub const REQUIRED_HEADERS: CloudHeaders = &[("Api-Revision", API_REVISION_VALUE)];

// ---- endpoint paths ----------------------------------------------------

/// `POST` endpoint that runs one interaction (the answer call). The interaction
/// IS the unit of work — synchronous request→response, with multi-turn chaining
/// via `previous_interaction_id`. (Mapped onto the session-shape `create_session`
/// slot on the transport row; this vendor is turn-shaped, not task-shaped.)
///
/// The `v1beta` prefix is the documented version for the Interactions API (per
/// the official Antigravity Agent page) — the SAME path the sibling
/// `gemini_cloud` adapter uses, which is exactly the point: Antigravity cloud
/// and generic Gemini cloud share this endpoint and differ only in the `agent`
/// field of the body.
pub const INTERACTIONS_PATH: &str = "/v1beta/interactions";

/// `POST` endpoint that registers a named managed agent (custom-agent
/// lifecycle). Bluey doesn't need this for the meeting-question use case (it can
/// call [`DEFAULT_AGENT`] directly), but the builder is provided for the future
/// "ask about my repo" path (the `sources[]` field injects repo context).
pub const AGENTS_PATH: &str = "/v1beta/agents";

// ---- auth model --------------------------------------------------------

/// How the Gemini API expects the credential: a header token in `x-goog-api-key`
/// with no prefix. Identical in shape to Anthropic's `x-api-key` — we REUSE the
/// generic [`super::transport::CloudAuth::HeaderToken`] primitive; there is no
/// new auth variant. The `?key=` query-param form is deliberately NOT used (it
/// would risk the key landing in a logged URL).
pub const ANTIGRAVITY_AUTH: super::transport::CloudAuth =
    super::transport::CloudAuth::HeaderToken {
        header_name: "x-goog-api-key",
        prefix: "",
    };

// ---- consent text ------------------------------------------------------

/// Mandatory consent text the UI MUST show before Bluey stores the Gemini API
/// key. Encodes the non-obvious promises: (1) billing is BYOT, metered per
/// Gemini token (Gemini 3.5 Flash rates), NOT a flat plan; (2) prompts run in a
/// Google-hosted remote sandbox; (3) sandbox compute is free only during the
/// preview; (4) the key lives in the OS keychain and only ever goes to the
/// Gemini API host.
pub const ANTIGRAVITY_BYOT_CONSENT_TEXT: &str = "Google Antigravity (Cloud) uses your \
    Gemini API key. Every question Bluey sends runs the Antigravity agent on Gemini 3.5 \
    Flash and is billed per token to YOUR Gemini API key (about $1.50 per million input \
    and $9 per million output tokens) — this is NOT a flat subscription, and a single \
    agentic question can use millions of tokens. Your prompt runs in a Google-hosted, \
    ephemeral remote Linux sandbox; sandbox compute is free only during the preview. \
    Bluey stores the key in your OS keychain; it never leaves this machine except to \
    call generativelanguage.googleapis.com directly. Revoke the key in Google AI Studio \
    at any time.";

/// Soft envelope for a single interaction's wall-clock life, used to size poll
/// timeouts. Mirrors the Anthropic/Codex 30-minute parity value. The
/// Interactions API is synchronous (not poll-shaped), so this is a defensive
/// ceiling, not a documented hard limit.
pub const MAX_SESSION_DURATION_SECS: u32 = 30 * 60;

// ---- registry row ------------------------------------------------------

/// The cloud-registry row for Google Antigravity (Cloud). Pure data; appended at
/// the END of [`super::registry::CLOUD_REGISTRY`].
///
/// `kind_tag: KindTag::AntigravityCloud` keeps this row DISTINCT from the LOCAL
/// `KindTag::Antigravity` (the bundled `agy`/`gemini` CLI on the user's machine)
/// so the daemon's route logic dispatches each independently.
///
/// `task_shaped: false` because the Interactions API is session/turn-shaped
/// (synchronous request→response with conversation chaining), NOT task-shaped
/// like Codex Cloud / Copilot Cloud. This mirrors the Anthropic Managed Agents
/// row.
pub const ENTRY: &CloudAgentEntry = &CloudAgentEntry {
    kind_tag: KindTag::AntigravityCloud,
    display_name: DISPLAY_NAME,
    vendor_short: VENDOR,
    base_url: BASE_URL,
    // BYOT — the user's Gemini API key is metered per token. The disclosure UI
    // MUST surface `ANTIGRAVITY_BYOT_CONSENT_TEXT` before storing the key.
    billing_model: BillingModel::ApiCredits,
    consent_warning: ANTIGRAVITY_BYOT_CONSENT_TEXT,
    task_shaped: false,
    max_task_duration_secs: MAX_SESSION_DURATION_SECS,
};

/// `CloudTransport` shape Bluey reads when dispatching to this vendor. Pure
/// data; carries the base URL, the required `Api-Revision` header, the
/// `x-goog-api-key` auth model, and the endpoint paths.
///
/// The Interactions endpoint is mapped onto the SESSION-shape slots
/// (`create_session` / `send_event` / `stream_events`) because the interaction
/// is turn-shaped — the same path serves "start a conversation" and "continue
/// it" (the request body's `previous_interaction_id` distinguishes them). The
/// TASK-shape slots are `None` (this vendor is not fire-and-poll).
pub const TRANSPORT: CloudTransport = CloudTransport::Https {
    base_url: BASE_URL,
    headers: REQUIRED_HEADERS,
    auth: ANTIGRAVITY_AUTH,
    endpoints: CloudEndpoints {
        create_agent: Some(AGENTS_PATH),
        create_environment: None,
        // The interaction == the "session" turn; same path for first + later
        // turns (chained by `previous_interaction_id` in the body).
        create_session: Some(INTERACTIONS_PATH),
        send_event: Some(INTERACTIONS_PATH),
        stream_events: Some(INTERACTIONS_PATH),
        delete_session: None,
        // Turn-shaped, not task-shaped.
        create_task: None,
        get_task_status: None,
    },
};

// ---- request-body builders ---------------------------------------------

/// Build the `POST /v1beta/interactions` body for a single-turn answer call.
///
/// Field meanings (from the official Antigravity Agent page + quickstart — see
/// dossier §4):
/// - `agent`: the agent id ([`DEFAULT_AGENT`] for the stock Antigravity agent,
///   or a custom registered agent's `id`).
/// - `input`: the prompt text (free-form). The API also accepts a content-block
///   array; Bluey's meeting questions are plain text, so we send the string
///   form.
/// - `environment`: `"remote"` requests a fresh ephemeral Linux sandbox.
/// - `stream`: when `true`, the response is an SSE stream of step deltas.
/// - `previous_interaction_id`: when `Some`, chains conversation history from a
///   prior interaction (multi-turn). Omitted entirely when `None` (a fresh
///   conversation) — NOT sent as `null`, matching the doc examples.
///
/// `max_output_tokens` / `temperature` are intentionally NEVER sent — the
/// Antigravity Agent docs say the Interactions API rejects them with `400`.
pub fn build_interaction_body(
    agent: &str,
    input: &str,
    environment: &str,
    stream: bool,
    previous_interaction_id: Option<&str>,
) -> serde_json::Value {
    let mut obj = serde_json::Map::new();
    obj.insert("agent".to_string(), json!(agent));
    obj.insert("input".to_string(), json!(input));
    obj.insert("environment".to_string(), json!(environment));
    obj.insert("stream".to_string(), json!(stream));
    if let Some(prev) = previous_interaction_id {
        obj.insert("previous_interaction_id".to_string(), json!(prev));
    }
    serde_json::Value::Object(obj)
}

/// Build the [`HttpRequest`] for `POST /v1beta/interactions`. Pure: returns the
/// request struct without firing it. The transport applies auth + content-type
/// at send time; the `Api-Revision` header is attached here (it's also on the
/// registry row's `headers` slice for the data-driven dispatch path).
pub fn interaction_request(
    agent: &str,
    input: &str,
    environment: &str,
    stream: bool,
    previous_interaction_id: Option<&str>,
) -> HttpRequest {
    let mut req = HttpRequest::post_json(
        format!("{BASE_URL}{INTERACTIONS_PATH}"),
        build_interaction_body(agent, input, environment, stream, previous_interaction_id),
    )
    .with_header("Api-Revision", API_REVISION_VALUE);
    if stream {
        // The streaming form is an SSE response; advertise that we accept it.
        req = req.with_header("Accept", "text/event-stream");
    }
    req
}

/// Convenience: the default single-turn answer request against the stock
/// Antigravity agent in a fresh remote sandbox. This is the shape Bluey uses for
/// a meeting question.
pub fn default_answer_request(input: &str) -> HttpRequest {
    interaction_request(DEFAULT_AGENT, input, "remote", false, None)
}

/// Build the `POST /v1beta/agents` body to register a named managed agent. Provided
/// for the future "ask about my repo" path (the `sources[]` array injects repo
/// context as inline files); the meeting-question path does not need it.
///
/// `sources` is a list of `(target, content)` inline file pairs (e.g.
/// `(".agents/AGENTS.md", "<instructions>")`).
pub fn build_register_agent_body(
    id: &str,
    base_agent: &str,
    system_instruction: &str,
    sources: &[(&str, &str)],
) -> serde_json::Value {
    let source_objs: Vec<serde_json::Value> = sources
        .iter()
        .map(|(target, content)| json!({ "type": "inline", "target": target, "content": content }))
        .collect();
    json!({
        "id": id,
        "base_agent": base_agent,
        "system_instruction": system_instruction,
        "base_environment": {
            "type": "remote",
            "sources": source_objs,
        },
    })
}

/// Build the [`HttpRequest`] for `POST /v1beta/agents`.
pub fn register_agent_request(
    id: &str,
    base_agent: &str,
    system_instruction: &str,
    sources: &[(&str, &str)],
) -> HttpRequest {
    HttpRequest::post_json(
        format!("{BASE_URL}{AGENTS_PATH}"),
        build_register_agent_body(id, base_agent, system_instruction, sources),
    )
    .with_header("Api-Revision", API_REVISION_VALUE)
}

// ---- response parsing --------------------------------------------------

/// Parsed result of a synchronous interaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InteractionResult {
    /// Opaque interaction id — pass as `previous_interaction_id` to continue the
    /// conversation on the next turn.
    pub id: String,
    /// Sandbox id — pass as `environment` to reuse the same workspace state on
    /// the next turn. `None` if the response omitted it.
    pub environment_id: Option<String>,
    /// The agent's final answer text.
    pub output_text: String,
}

/// Parse a synchronous `POST /v1beta/interactions` response into the answer +
/// continuation handles.
///
/// Defensive about the answer field's location (NEEDS-LIVE-VERIFY): tries the
/// documented top-level `output_text`, then a nested `result.output_text`, so a
/// minor doc/shape drift doesn't silently drop the answer. `id` is required (we
/// can't continue a conversation without it); `environment_id` and the answer
/// are optional-but-expected.
pub fn parse_interaction_response(body: &serde_json::Value) -> Result<InteractionResult> {
    let id = body
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("interaction response missing `id`"))?
        .to_string();

    let environment_id = body
        .get("environment_id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    // Primary: top-level `output_text`. Fallback: `result.output_text`.
    let output_text = body
        .get("output_text")
        .and_then(|v| v.as_str())
        .or_else(|| {
            body.get("result")
                .and_then(|r| r.get("output_text"))
                .and_then(|v| v.as_str())
        })
        .unwrap_or("")
        .to_string();

    Ok(InteractionResult {
        id,
        environment_id,
        output_text,
    })
}

/// Parse a `POST /v1beta/agents` response into the registered agent's id (echoed
/// back, or the chosen id). Used by the custom-agent registration path.
pub fn parse_register_agent_response(body: &serde_json::Value) -> Result<String> {
    // The id may come back as `id` or, if Google returns a resource name, as
    // `name`. Prefer `id`; fall back to `name`.
    body.get("id")
        .and_then(|v| v.as_str())
        .or_else(|| body.get("name").and_then(|v| v.as_str()))
        .map(|s| s.to_string())
        .ok_or_else(|| anyhow!("register-agent response missing `id`/`name`"))
}

// ---- error taxonomy ----------------------------------------------------

/// Map an HTTP status code from the Gemini API onto a stable, user-facing
/// guidance string. Used by the daemon's "agent not ready" path so we never
/// escalate to Bluey's own AI on a vendor error.
///
/// The codes follow Google's standard API error taxonomy (see dossier §8):
/// 400 = malformed / free-tier-region, 401/403 = bad key, 404 = unknown agent,
/// 429 = rate-limited, 5xx = transient.
pub fn guidance_for_status(status: u16) -> &'static str {
    match status {
        400 => {
            "Antigravity Cloud rejected the request (400). This usually means a malformed \
                request, or the Gemini free tier isn't available in your country — enable \
                billing in Google AI Studio."
        }
        401 | 403 => {
            "Your Gemini API key was rejected by Antigravity Cloud. Reconnect Antigravity \
                Cloud from Bluey settings (generate a key at aistudio.google.com)."
        }
        404 => {
            "Antigravity Cloud could not find that agent. If you used a custom agent, \
                re-register it; otherwise this is an internal error."
        }
        429 => {
            "Antigravity Cloud is rate-limiting your Gemini project. Try again in a few minutes."
        }
        500 => "Antigravity Cloud hit a Google-side error. Try a shorter question or retry.",
        503 => "Antigravity Cloud is temporarily overloaded. Try again shortly.",
        504 => "Antigravity Cloud timed out — your question or context may be too large.",
        // Remaining 5xx (501, 502, …) — transient/unavailable. Listed AFTER the
        // specific codes above so the ranges don't overlap (clippy-clean).
        501 | 502 | 505..=599 => "Antigravity Cloud is temporarily unavailable.",
        _ => "Antigravity Cloud rejected the request.",
    }
}

/// Convert an [`HttpResponse`] into the rendered guidance string the daemon
/// shows the user when Antigravity Cloud rejects a request. Pure: looks up the
/// status guidance and, best-effort, appends the verbatim `error.message` from
/// the standard Google error envelope. Never panics on a malformed body; NEVER
/// reflects the request body or the key.
pub fn render_error_message(response: &HttpResponse) -> String {
    let base = guidance_for_status(response.status);
    if let Ok(value) = serde_json::from_slice::<serde_json::Value>(&response.body) {
        // Standard Google envelope: { "error": { "code", "message", "status" } }.
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

// ---- convenience send wrappers (used by the session drive + tests) -----

/// Fire one interaction using the supplied transport + credential store and
/// parse the synchronous result. Async because the transport is async; a thin
/// wrapper over the builder + parser so the wire path is exercised end-to-end
/// from integration tests and the daemon's session-drive route.
pub async fn run_interaction(
    transport: &CloudHttpsTransport,
    creds: &dyn super::keychain::VendorCredentialStore,
    input: &str,
    previous_interaction_id: Option<&str>,
    environment: &str,
) -> Result<InteractionResult> {
    let request = interaction_request(
        DEFAULT_AGENT,
        input,
        environment,
        false,
        previous_interaction_id,
    );
    let response = transport
        .send(
            VENDOR,
            INTERACTIONS_PATH,
            &request,
            ANTIGRAVITY_AUTH,
            creds,
            CREDENTIAL_KEY,
        )
        .await?;
    if !(200..300).contains(&response.status) {
        return Err(anyhow!("{}", render_error_message(&response)));
    }
    let body: serde_json::Value = serde_json::from_slice(&response.body)
        .map_err(|e| anyhow!("Antigravity Cloud returned non-JSON response: {e}"))?;
    parse_interaction_response(&body)
}

// ---- dispatcher-side helpers (mirror the sibling gemini_cloud contract) -

/// The `Api-Revision` schema header(s) as a slice the turn-shaped dispatcher
/// (`cloud/drive.rs::spawn_turn_stream`) can iterate and layer onto the request.
/// Mirrors the sibling `gemini_cloud::api_revision_headers()` so both rows wire
/// in identically — the eventual merge collapses the two into one.
pub fn api_revision_headers() -> &'static [(&'static str, &'static str)] {
    REQUIRED_HEADERS
}

/// Dispatcher-side parse: take the raw synchronous-interaction response bytes
/// and return `(session_id, answer_text)` in the exact shape the turn-shaped
/// dispatcher emits (`Started { session_id } → Delta(text) → Done`). The
/// interaction id becomes the answer's `session_id` so a future multi-turn
/// follow-up can chain it as `previous_interaction_id`. Mirrors
/// `gemini_cloud::parse_answer_for_dispatch` so the dispatcher arms are uniform.
pub fn parse_answer_for_dispatch(body: &[u8]) -> Result<(Option<String>, String)> {
    let value: serde_json::Value = serde_json::from_slice(body)
        .map_err(|e| anyhow!("Antigravity Cloud response was not valid JSON: {e}"))?;
    let parsed = parse_interaction_response(&value)?;
    Ok((Some(parsed.id), parsed.output_text))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cloud::keychain::{MemoryCredentialStore, VendorCredentialStore};

    // ---- request body shape: the contract NEEDS-LIVE-VERIFY guards --------

    #[test]
    fn interaction_body_matches_documented_shape() {
        let body = build_interaction_body(
            DEFAULT_AGENT,
            "What is the capital of France?",
            "remote",
            false,
            None,
        );
        // Per the official Antigravity Agent page:
        //   { "agent": "antigravity-preview-05-2026", "input": "...",
        //     "environment": "remote", "stream": false }
        assert_eq!(body["agent"].as_str(), Some("antigravity-preview-05-2026"));
        assert_eq!(
            body["input"].as_str(),
            Some("What is the capital of France?")
        );
        assert_eq!(body["environment"].as_str(), Some("remote"));
        assert_eq!(body["stream"].as_bool(), Some(false));
        // A fresh conversation omits previous_interaction_id entirely (not null).
        assert!(body.get("previous_interaction_id").is_none());
    }

    #[test]
    fn interaction_body_chains_previous_id_for_multi_turn() {
        let body = build_interaction_body(
            DEFAULT_AGENT,
            "and Germany?",
            "env_abc",
            false,
            Some("int_1"),
        );
        assert_eq!(body["previous_interaction_id"].as_str(), Some("int_1"));
        // Multi-turn reuses the sandbox by passing its id as `environment`.
        assert_eq!(body["environment"].as_str(), Some("env_abc"));
    }

    #[test]
    fn interaction_body_never_sends_temperature_or_max_tokens() {
        // The Antigravity Agent docs say these are rejected with 400 — they must
        // NEVER appear in the body we build.
        let body = build_interaction_body(DEFAULT_AGENT, "hi", "remote", true, None);
        assert!(body.get("temperature").is_none());
        assert!(body.get("max_output_tokens").is_none());
        assert_eq!(body["stream"].as_bool(), Some(true));
    }

    #[test]
    fn interaction_request_url_and_headers() {
        let req = default_answer_request("hello");
        assert_eq!(
            req.url,
            "https://generativelanguage.googleapis.com/v1beta/interactions"
        );
        assert_eq!(req.method, reqwest::Method::POST);
        assert!(req.json_body.is_some());
        // The Api-Revision header is attached as data.
        assert!(req
            .extra_headers
            .iter()
            .any(|(n, v)| n == "Api-Revision" && v == API_REVISION_VALUE));
    }

    #[test]
    fn streaming_request_advertises_event_stream_accept() {
        let req = interaction_request(DEFAULT_AGENT, "hi", "remote", true, None);
        assert!(req
            .extra_headers
            .iter()
            .any(|(n, v)| n == "Accept" && v == "text/event-stream"));
    }

    #[test]
    fn register_agent_body_matches_documented_shape() {
        let body = build_register_agent_body(
            "data-analyst",
            DEFAULT_AGENT,
            "You are a data analyst.",
            &[(".agents/AGENTS.md", "Always use matplotlib.")],
        );
        assert_eq!(body["id"].as_str(), Some("data-analyst"));
        assert_eq!(body["base_agent"].as_str(), Some(DEFAULT_AGENT));
        assert_eq!(
            body["system_instruction"].as_str(),
            Some("You are a data analyst.")
        );
        assert_eq!(body["base_environment"]["type"].as_str(), Some("remote"));
        let src = &body["base_environment"]["sources"][0];
        assert_eq!(src["type"].as_str(), Some("inline"));
        assert_eq!(src["target"].as_str(), Some(".agents/AGENTS.md"));
        assert_eq!(src["content"].as_str(), Some("Always use matplotlib."));
    }

    // ---- response parsing -------------------------------------------------

    #[test]
    fn parse_interaction_extracts_answer_and_handles() {
        let body = json!({
            "id": "int_42",
            "environment_id": "env_99",
            "output_text": "Paris.",
            "steps": [],
        });
        let r = parse_interaction_response(&body).unwrap();
        assert_eq!(r.id, "int_42");
        assert_eq!(r.environment_id.as_deref(), Some("env_99"));
        assert_eq!(r.output_text, "Paris.");
    }

    #[test]
    fn parse_interaction_falls_back_to_nested_output_text() {
        // Defensive against a shape drift where the answer is nested.
        let body = json!({
            "id": "int_7",
            "result": { "output_text": "nested answer" },
        });
        let r = parse_interaction_response(&body).unwrap();
        assert_eq!(r.output_text, "nested answer");
        assert!(r.environment_id.is_none());
    }

    #[test]
    fn parse_interaction_requires_id() {
        let body = json!({ "output_text": "orphan answer" });
        assert!(parse_interaction_response(&body).is_err());
    }

    #[test]
    fn parse_interaction_tolerates_missing_answer() {
        // A response with an id but no answer field yields an empty answer, not
        // an error — the caller decides whether empty is acceptable.
        let body = json!({ "id": "int_x" });
        let r = parse_interaction_response(&body).unwrap();
        assert_eq!(r.output_text, "");
    }

    #[test]
    fn parse_register_agent_prefers_id_then_name() {
        assert_eq!(
            parse_register_agent_response(&json!({ "id": "my-agent" })).unwrap(),
            "my-agent"
        );
        assert_eq!(
            parse_register_agent_response(&json!({ "name": "agents/abc" })).unwrap(),
            "agents/abc"
        );
        assert!(parse_register_agent_response(&json!({})).is_err());
    }

    // ---- guidance strings (no panic on any status) ------------------------

    #[test]
    fn guidance_distinguishes_400_403_404_429_5xx() {
        let g400 = guidance_for_status(400);
        let g403 = guidance_for_status(403);
        let g404 = guidance_for_status(404);
        let g429 = guidance_for_status(429);
        let g500 = guidance_for_status(500);
        let g503 = guidance_for_status(503);
        // 401 and 403 share the bad-key guidance.
        assert_eq!(guidance_for_status(401), g403);
        // The distinct buckets are actually distinct.
        for (a, b) in [
            (g400, g403),
            (g403, g404),
            (g404, g429),
            (g429, g500),
            (g500, g503),
        ] {
            assert_ne!(a, b);
        }
        for g in [g400, g403, g404, g429, g500, g503] {
            assert!(!g.is_empty());
        }
    }

    #[test]
    fn guidance_for_unknown_status_falls_through() {
        let g = guidance_for_status(418);
        assert!(!g.is_empty());
        // An unmapped 5xx still gets the generic-unavailable bucket.
        assert!(guidance_for_status(599).contains("unavailable"));
    }

    #[test]
    fn render_error_message_appends_google_envelope_detail() {
        let body = json!({
            "error": {
                "code": 429,
                "message": "Quota exceeded for quota metric 'Requests'.",
                "status": "RESOURCE_EXHAUSTED",
            }
        });
        let bytes = serde_json::to_vec(&body).unwrap();
        let response = HttpResponse {
            status: 429,
            body: bytes,
            request_id_present: true,
        };
        let rendered = render_error_message(&response);
        assert!(rendered.contains("rate-limiting"));
        assert!(rendered.contains("Quota exceeded"));
    }

    #[test]
    fn render_error_message_tolerates_non_json_body() {
        let response = HttpResponse {
            status: 503,
            body: b"<html>503 backend</html>".to_vec(),
            request_id_present: false,
        };
        let rendered = render_error_message(&response);
        assert!(rendered.contains("overloaded") || rendered.contains("unavailable"));
    }

    // ---- consent text is the BYOT disclosure ------------------------------

    #[test]
    fn consent_text_states_billing_and_data_promise() {
        let text = ANTIGRAVITY_BYOT_CONSENT_TEXT.to_lowercase();
        // The non-obvious promises the user MUST see.
        assert!(text.contains("per token"));
        assert!(text.contains("gemini 3.5 flash"));
        assert!(text.contains("not a flat subscription"));
        assert!(text.contains("sandbox"));
        assert!(text.contains("keychain"));
        assert!(text.contains("preview"));
    }

    // ---- TRANSPORT registry shape -----------------------------------------

    #[test]
    fn transport_declares_goog_api_key_header_auth() {
        match TRANSPORT {
            CloudTransport::Https {
                base_url,
                auth,
                endpoints,
                headers,
            } => {
                assert_eq!(base_url, "https://generativelanguage.googleapis.com");
                assert_eq!(
                    auth,
                    CloudAuth::HeaderToken {
                        header_name: "x-goog-api-key",
                        prefix: "",
                    }
                );
                // The Api-Revision header is carried as DATA, never in a branch.
                assert!(headers
                    .iter()
                    .any(|(n, v)| *n == "Api-Revision" && *v == API_REVISION_VALUE));
                // Session-shape slots are populated (the interaction is the turn);
                // task-shape slots MUST be absent.
                assert_eq!(endpoints.create_session, Some(INTERACTIONS_PATH));
                assert_eq!(endpoints.create_agent, Some(AGENTS_PATH));
                assert!(endpoints.create_task.is_none());
                assert!(endpoints.get_task_status.is_none());
            }
            CloudTransport::None => panic!("Antigravity Cloud row must be Https-shaped"),
        }
    }

    #[test]
    fn entry_is_session_shaped_byot() {
        assert_eq!(ENTRY.kind_tag, KindTag::AntigravityCloud);
        assert_eq!(ENTRY.vendor_short, "antigravity_cloud");
        assert!(!ENTRY.task_shaped, "Interactions API is turn-shaped");
        assert_eq!(ENTRY.billing_model, BillingModel::ApiCredits);
        assert!(!ENTRY.consent_warning.trim().is_empty());
    }

    #[test]
    fn default_agent_is_the_sole_gemini_differentiator() {
        // Documenting the overlap invariant in a test: the stock agent id is the
        // only thing that makes this "Antigravity" vs generic Gemini-cloud.
        assert_eq!(DEFAULT_AGENT, "antigravity-preview-05-2026");
        let body = default_answer_request("x").json_body.unwrap();
        assert_eq!(body["agent"].as_str(), Some(DEFAULT_AGENT));
    }

    // ---- Wiremock round-trips: real send() against a mock server ----------

    #[tokio::test]
    async fn interaction_round_trips_answer_on_200() {
        use wiremock::matchers::{body_partial_json, header, method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1beta/interactions"))
            // Auth header carries the key with NO prefix (x-goog-api-key shape).
            .and(header("x-goog-api-key", "gemini_test_key"))
            // Api-Revision is sent as data.
            .and(header("Api-Revision", API_REVISION_VALUE))
            // The agent field is the Antigravity agent.
            .and(body_partial_json(
                json!({ "agent": "antigravity-preview-05-2026" }),
            ))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("x-request-id", "req-abc")
                    .set_body_json(json!({
                        "id": "int_001",
                        "environment_id": "env_001",
                        "output_text": "The answer is 42.",
                    })),
            )
            .mount(&server)
            .await;

        let creds = MemoryCredentialStore::new("antigravity_cloud");
        creds.save(CREDENTIAL_KEY, "gemini_test_key").unwrap();
        let transport = CloudHttpsTransport::new();
        // Point the request at the mock server (the helper uses BASE_URL; we
        // build the request directly so we can override the host).
        let request = HttpRequest::post_json(
            format!("{}/v1beta/interactions", server.uri()),
            build_interaction_body(DEFAULT_AGENT, "What is 6 times 7?", "remote", false, None),
        )
        .with_header("Api-Revision", API_REVISION_VALUE);
        let response = transport
            .send(
                VENDOR,
                INTERACTIONS_PATH,
                &request,
                ANTIGRAVITY_AUTH,
                &creds,
                CREDENTIAL_KEY,
            )
            .await
            .expect("transport call failed");
        assert_eq!(response.status, 200);
        assert!(response.request_id_present);
        let value: serde_json::Value = serde_json::from_slice(&response.body).unwrap();
        let parsed = parse_interaction_response(&value).unwrap();
        assert_eq!(parsed.id, "int_001");
        assert_eq!(parsed.environment_id.as_deref(), Some("env_001"));
        assert_eq!(parsed.output_text, "The answer is 42.");
        // The token must NEVER appear in the response body handed back.
        assert!(!response.body_text().contains("gemini_test_key"));
    }

    #[tokio::test]
    async fn interaction_multi_turn_sends_previous_id_and_reuses_env() {
        use wiremock::matchers::{body_partial_json, method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1beta/interactions"))
            // The second turn MUST carry the prior interaction id + reuse the env.
            .and(body_partial_json(json!({
                "previous_interaction_id": "int_001",
                "environment": "env_001",
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": "int_002",
                "environment_id": "env_001",
                "output_text": "Following up: still 42.",
            })))
            .mount(&server)
            .await;

        let creds = MemoryCredentialStore::new("antigravity_cloud");
        creds.save(CREDENTIAL_KEY, "gemini_test_key").unwrap();
        let transport = CloudHttpsTransport::new();
        let request = HttpRequest::post_json(
            format!("{}/v1beta/interactions", server.uri()),
            build_interaction_body(
                DEFAULT_AGENT,
                "and double it?",
                "env_001",
                false,
                Some("int_001"),
            ),
        )
        .with_header("Api-Revision", API_REVISION_VALUE);
        let response = transport
            .send(
                VENDOR,
                INTERACTIONS_PATH,
                &request,
                ANTIGRAVITY_AUTH,
                &creds,
                CREDENTIAL_KEY,
            )
            .await
            .expect("transport call failed");
        assert_eq!(response.status, 200);
        let value: serde_json::Value = serde_json::from_slice(&response.body).unwrap();
        let parsed = parse_interaction_response(&value).unwrap();
        assert_eq!(parsed.id, "int_002");
        assert_eq!(parsed.environment_id.as_deref(), Some("env_001"));
    }

    #[tokio::test]
    async fn interaction_returns_error_and_never_leaks_token_on_403() {
        use wiremock::matchers::{header, method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1beta/interactions"))
            .and(header("x-goog-api-key", "bad_key_secret"))
            .respond_with(ResponseTemplate::new(403).set_body_json(json!({
                "error": {
                    "code": 403,
                    "message": "API key not valid. Please pass a valid API key.",
                    "status": "PERMISSION_DENIED",
                }
            })))
            .mount(&server)
            .await;

        let creds = MemoryCredentialStore::new("antigravity_cloud");
        creds.save(CREDENTIAL_KEY, "bad_key_secret").unwrap();
        let transport = CloudHttpsTransport::new();
        let request = HttpRequest::post_json(
            format!("{}/v1beta/interactions", server.uri()),
            build_interaction_body(DEFAULT_AGENT, "hi", "remote", false, None),
        )
        .with_header("Api-Revision", API_REVISION_VALUE);
        let response = transport
            .send(
                VENDOR,
                INTERACTIONS_PATH,
                &request,
                ANTIGRAVITY_AUTH,
                &creds,
                CREDENTIAL_KEY,
            )
            .await
            .expect("transport returns the response, not Err, on 4xx");
        assert_eq!(response.status, 403);
        let rendered = render_error_message(&response);
        assert!(rendered.contains("rejected") || rendered.contains("Reconnect"));
        assert!(rendered.contains("API key not valid"));
        // The token MUST never be reflected in the rendered error.
        assert!(
            !rendered.contains("bad_key_secret"),
            "MUST never reflect the token"
        );
    }
}
