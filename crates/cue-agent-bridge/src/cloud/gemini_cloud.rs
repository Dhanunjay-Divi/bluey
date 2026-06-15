//! Google Gemini Cloud (Managed Agents / Interactions API) adapter — the 5th
//! cloud vendor, and the only **turn-shaped** one.
//!
//! See `docs/vendors/gemini_cloud.md` for the full dossier (auth, billing,
//! SSE taxonomy, OPEN questions). This file holds the **irreducible
//! per-vendor code**: the registry row, the `Api-Revision` schema header
//! (carried as DATA, never a code branch), the exact `POST /v1beta/interactions`
//! request body, the synchronous-response parser, the SSE event →
//! [`AnswerChunk`] mapping, and the HTTP-status → guidance mapping. Everything
//! generic (HTTPS transport, keychain, audit) lives in the shared `cloud/`
//! modules.
//!
//! ### Shape: TURN, not task
//!
//! Unlike Cursor/Copilot/Codex Cloud (task-shaped: kick off a minutes-long job,
//! return a PR link), the Gemini Interactions API is **synchronous** — it
//! "provisions a sandbox, runs the agent loop, and returns the result" in the
//! HTTP response, with optional SSE streaming. So `ENTRY.task_shaped == false`
//! and the dispatcher drives it like a normal answer (`Started → Delta → Done`),
//! the same overlay path a local CLI agent uses. The agent loop can still take
//! tens of seconds to a couple minutes (code execution + web browsing), so the
//! call uses a longer-than-default timeout (see [`ANSWER_TIMEOUT`]).
//!
//! ### Auth
//!
//! Bluey targets the **consumer Gemini Developer API** (an AI Studio key,
//! `x-goog-api-key` header). The token IS the whole header value (no `Bearer `
//! prefix) — identical in shape to Anthropic's `x-api-key`, so it reuses
//! [`CloudAuth::HeaderToken`] with `header_name: "x-goog-api-key", prefix: ""`.
//! Vertex AI (OAuth2/ADC, GCP project) is explicitly OUT of scope for v1.

use std::time::Duration;

use anyhow::{anyhow, Result};
use serde_json::json;

use super::registry::{BillingModel, CloudAgentEntry};
use super::transport::{
    CloudAuth, CloudEndpoints, CloudHttpsTransport, CloudTransport, HttpRequest, HttpResponse,
};
use crate::drive::AnswerChunk;
use crate::registry::KindTag;

/// Vendor short-name. Used as the keychain service suffix
/// (`bluey_cloud_gemini_cloud`) and the audit log's `vendor` field. Lowercase
/// ASCII to match the convention the other vendor adapters use. Distinct from
/// the LOCAL Gemini CLI (which has no cloud row) so the two never collide.
pub const VENDOR: &str = "gemini_cloud";

/// Keychain credential key under which Bluey stores the user's AI Studio
/// Gemini API key. Full keychain path: `bluey_cloud_gemini_cloud / api_key`.
pub const CREDENTIAL_KEY: &str = "api_key";

/// Env var the user can paste from in their shell, mirrored to the keychain on
/// first use. The unified `google-genai` SDK reads BOTH `GEMINI_API_KEY` and
/// `GOOGLE_API_KEY` (preferring the latter when both are set); Bluey imports
/// from the Gemini-specific var. The daemon never writes a secret back out
/// into the environment — this is a one-shot import only.
pub const CREDENTIAL_ENV_FALLBACK: &str = "GEMINI_API_KEY";

/// Base URL of the Gemini Developer API. No trailing slash.
pub const BASE_URL: &str = "https://generativelanguage.googleapis.com";

/// The single endpoint that creates + runs an interaction. Synchronous by
/// default; SSE when the body carries `"stream": true`. (The stream form uses
/// the SAME path — there is no `?alt=sse` or `:stream` suffix.)
pub const INTERACTIONS_PATH: &str = "/v1beta/interactions";

/// The HTTP header Gemini uses to deliver the API key. The token is the WHOLE
/// value (no `Bearer ` prefix) — same shape as Anthropic's `x-api-key`.
pub const AUTH_HEADER_NAME: &str = "x-goog-api-key";

/// Schema opt-in header NAME. Gemini's analog of Anthropic's
/// `anthropic-version`. Carried as DATA on [`TRANSPORT`], never hardcoded in a
/// dispatch branch.
pub const API_REVISION_HEADER: &str = "Api-Revision";

/// Schema opt-in header VALUE (the Interactions-API revision Bluey speaks).
/// Bumping this is a one-line data change. (The older `2026-05-07` is the
/// documented fallback revision.)
pub const API_REVISION_VALUE: &str = "2026-05-20";

/// The Managed-Agent id that gives the sandbox experience (reason + execute
/// code + browse the web in an isolated Linux env). Preview-dated; WILL roll to
/// the next dated id at GA — kept as one `const` for a one-line bump.
pub const AGENT_ID: &str = "antigravity-preview-05-2026";

/// The `environment` value that requests a FRESH ephemeral sandbox. Passing a
/// returned `environment_id` here instead would reuse an existing sandbox
/// (preserving files/state); v1 always sends a fresh one (no resume yet).
pub const FRESH_ENVIRONMENT: &str = "remote";

/// The Gemini auth shape: token in `x-goog-api-key`, no prefix. Reuses the
/// generic [`CloudAuth::HeaderToken`] variant — NO new auth variant needed.
pub const GEMINI_AUTH: CloudAuth = CloudAuth::HeaderToken {
    header_name: AUTH_HEADER_NAME,
    prefix: "",
};

/// Soft ceiling (seconds) used to size the synchronous answer's HTTP timeout
/// and to tell the daemon how long a Gemini Cloud answer might take. NOT a
/// task-completion gate (this vendor is turn-shaped). The sandbox agent loop
/// can run tens of seconds to a couple minutes; 300 s is a defensive bound well
/// under any meeting-overlay patience limit while leaving room for a real run.
pub const MAX_ANSWER_DURATION_SECS: u32 = 300;

/// Per-call HTTP timeout for the synchronous interactions call. The shared
/// transport defaults to 30 s, which is too tight for a sandbox agent run, so
/// the request builder overrides it with this longer value.
pub const ANSWER_TIMEOUT: Duration = Duration::from_secs(MAX_ANSWER_DURATION_SECS as u64);

/// Mandatory consent text the UI MUST show before Bluey stores a Gemini API
/// key. Encodes the non-obvious promises: (1) the prompt (and anything the
/// agent fetches/executes) runs in a GOOGLE-hosted Linux sandbox; (2) BYOT —
/// billed to the user's OWN Gemini API account, tokens-only during preview;
/// (3) free-tier vs paid-tier data-use differs and Bluey can't tell which the
/// key is on, so it does NOT promise "never used for training"; (4) revoke the
/// key in Google AI Studio — disconnecting Bluey does NOT revoke it.
pub const GEMINI_BYOT_CONSENT_TEXT: &str = "Bluey will store your Google Gemini API key \
    (from Google AI Studio) in your OS keychain and send it on each Gemini Cloud call. \
    Your prompt runs inside a Google-hosted, isolated Linux sandbox where the agent can \
    execute code and browse the web. This is billed to your own Google Gemini API account \
    (Gemini model tokens only during the current preview). On Google's free tier, prompts \
    may be used to improve Google's products; the paid tier does not. Disconnecting Bluey \
    does NOT revoke the key — revoke it at https://aistudio.google.com/app/apikey.";

/// The registry row for Google Gemini Cloud. Pure data; appended to
/// [`super::registry::CLOUD_REGISTRY`] so the cloud-vendor enumeration sees it
/// without any per-vendor branch.
///
/// `kind_tag` is [`KindTag::GeminiCloud`] — distinct from the local
/// `KindTag::Gemini` (the `gemini` CLI) so the daemon's parse path routes an
/// "ask Gemini Cloud" intent here, not at the local CLI row.
pub const ENTRY: &CloudAgentEntry = &CloudAgentEntry {
    kind_tag: KindTag::GeminiCloud,
    display_name: "Google Gemini Agent (Cloud)",
    vendor_short: VENDOR,
    base_url: BASE_URL,
    // BYOT — Gemini-model tokens billed to the user's own Gemini API account
    // (environment compute not billed during preview). The disclosure in
    // `consent_warning` MUST make the BYOT + sandbox + data-use story clear.
    billing_model: BillingModel::ApiCredits,
    consent_warning: GEMINI_BYOT_CONSENT_TEXT,
    // TURN-shaped: the call is synchronous (answer in the HTTP response, or via
    // SSE). NOT a fire-and-poll task. This is the only `false` in the table.
    task_shaped: false,
    // Soft answer-duration bound (turn-shaped rows may set 0, but a non-zero
    // value lets the daemon size the stream timeout). See MAX_ANSWER_DURATION_SECS.
    max_task_duration_secs: MAX_ANSWER_DURATION_SECS,
};

/// `CloudTransport` shape for the Gemini Cloud registry row. Pure data; reading
/// this hands the dispatcher the URL/auth/header set without naming the vendor
/// anywhere else. The `Api-Revision` schema header lives HERE as data.
pub const TRANSPORT: CloudTransport = CloudTransport::Https {
    base_url: BASE_URL,
    // The schema opt-in header is carried as DATA — flipping the revision is a
    // one-line change, never a code branch (mirrors how the Anthropic row
    // carries `anthropic-version` / `anthropic-beta`).
    headers: &[(API_REVISION_HEADER, API_REVISION_VALUE)],
    auth: GEMINI_AUTH,
    endpoints: CloudEndpoints {
        create_agent: None,
        create_environment: None,
        // SESSION/turn-shape: the single interactions call is the "session
        // create"; the stream form uses the SAME path with `stream: true`.
        create_session: Some(INTERACTIONS_PATH),
        // Multi-turn continuation is `previous_interaction_id` in the body, not
        // a separate send-event path.
        send_event: None,
        stream_events: Some(INTERACTIONS_PATH),
        delete_session: None,
        // TASK-shape fields unused — this vendor is turn-shaped.
        create_task: None,
        get_task_status: None,
    },
};

/// The static headers the dispatcher must carry on every Gemini Cloud request
/// (the schema opt-in). Exposed so the drive path layers them generically off
/// the adapter — never inline — exactly as the Copilot adapter exposes its
/// `STATIC_HEADERS`. The shared transport applies auth/content-type/user-agent
/// itself but does NOT walk the registry row's `headers` slice, so the caller
/// adds these explicitly.
pub const STATIC_HEADERS: &[(&str, &str)] = &[(API_REVISION_HEADER, API_REVISION_VALUE)];

/// The `Api-Revision` schema header(s) as a slice the dispatcher can iterate.
/// A thin named accessor over [`STATIC_HEADERS`] so the drive path reads
/// intent-fully (`for (n, v) in gemini_cloud::api_revision_headers()`).
pub fn api_revision_headers() -> &'static [(&'static str, &'static str)] {
    STATIC_HEADERS
}

// ---------------------------------------------------------------------------
// Request builders — pure data, unit-testable without HTTPS.
// ---------------------------------------------------------------------------

/// Build the JSON body for `POST /v1beta/interactions` in the **Managed-Agent**
/// (sandbox) form. Pure: returns a `serde_json::Value` the transport
/// serializes; side-effect-free so the wire shape is unit-testable.
///
/// Field meanings (from `docs/vendors/gemini_cloud.md` §3):
/// - `agent`: the Managed-Agent id (sandbox experience). Defaults to
///   [`AGENT_ID`].
/// - `input`: the user's prompt (verbatim).
/// - `environment`: [`FRESH_ENVIRONMENT`] (`"remote"`) for a fresh ephemeral
///   sandbox. (A returned `environment_id` could be passed instead to reuse a
///   sandbox; v1 always sends fresh.)
/// - `stream`: whether to request the SSE form.
pub fn build_interaction_body(prompt: &str, stream: bool) -> serde_json::Value {
    json!({
        "agent": AGENT_ID,
        "input": prompt,
        "environment": FRESH_ENVIRONMENT,
        "stream": stream,
    })
}

/// Build the [`HttpRequest`] for the synchronous (non-streaming)
/// `POST /v1beta/interactions`. Pure: returns a request struct without firing
/// it. The transport applies auth + content-type + the `Api-Revision` header
/// (the latter via the registry row's `headers` slice) at send time. The
/// per-call timeout is widened to [`ANSWER_TIMEOUT`] for the sandbox agent loop.
pub fn interaction_request(prompt: &str) -> HttpRequest {
    let mut req = HttpRequest::post_json(
        format!("{BASE_URL}{INTERACTIONS_PATH}"),
        build_interaction_body(prompt, false),
    );
    req.timeout = Some(ANSWER_TIMEOUT);
    req
}

/// Build the [`HttpRequest`] for the **streaming** form. Same path + body but
/// `stream: true` and an `Accept: text/event-stream` header so the server
/// emits SSE. The streaming wire-up in the dispatcher is gated on live latency
/// (see dossier §11.5), but the builder + SSE parser are unit-tested now.
pub fn interaction_stream_request(prompt: &str) -> HttpRequest {
    let mut req = HttpRequest::post_json(
        format!("{BASE_URL}{INTERACTIONS_PATH}"),
        build_interaction_body(prompt, true),
    )
    .with_header("Accept", "text/event-stream");
    req.timeout = Some(ANSWER_TIMEOUT);
    req
}

// ---------------------------------------------------------------------------
// Synchronous-response parser.
// ---------------------------------------------------------------------------

/// A parsed synchronous interaction response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InteractionResult {
    /// Interaction id (e.g. `int_…`). Surfaced as the answer's `session_id`.
    pub id: Option<String>,
    /// The sandbox environment id, returned for reuse. `None` when absent.
    pub environment_id: Option<String>,
    /// Lifecycle status (`completed | requires_action | in_progress`).
    pub status: Option<String>,
    /// The flattened answer text (the user-visible result).
    pub text: String,
}

/// Parse the JSON body of a synchronous `POST /v1beta/interactions` response.
///
/// ⚠️ NEEDS-LIVE-VERIFY (dossier §STATUS / OPEN-Q 1): the docs publish `id`,
/// `status`, a `steps`/`outputs` array, `usage`, and the SDK sugar
/// `output_text` (last text block(s), auto-joined), but the REST docs page did
/// not include a complete verbatim non-streaming body. This parser is
/// deliberately defensive — it tries, in order:
///   1. a top-level `output_text` string (SDK-sugar mirror),
///   2. the last `{type:"text", text}` block walked out of `outputs[]`,
///   3. the same out of `steps[]` (the May-2026 rename of `outputs`).
///
/// If the live shape differs, the fix is a one-line path change here; the unit
/// tests pin the CURRENT assumed shape so any drift fails loudly.
pub fn parse_interaction_response(body: &serde_json::Value) -> Result<InteractionResult> {
    let id = body
        .get("id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    // environment_id may appear top-level or nested under `environment`.
    let environment_id = body
        .get("environment_id")
        .and_then(|v| v.as_str())
        .or_else(|| {
            body.get("environment")
                .and_then(|e| e.get("id"))
                .and_then(|v| v.as_str())
        })
        .map(|s| s.to_string());
    let status = body
        .get("status")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let text = extract_answer_text(body);
    if text.is_empty() {
        return Err(anyhow!(
            "Gemini Cloud response carried no answer text (id={id:?}, status={status:?})"
        ));
    }
    Ok(InteractionResult {
        id,
        environment_id,
        status,
        text,
    })
}

/// Pull the user-visible answer text out of a response body, trying the three
/// documented shapes in order (see [`parse_interaction_response`]). Returns an
/// empty string when none match (the caller decides whether that is an error).
fn extract_answer_text(body: &serde_json::Value) -> String {
    // 1. SDK-sugar mirror: a plain top-level `output_text` string.
    if let Some(s) = body.get("output_text").and_then(|v| v.as_str()) {
        if !s.is_empty() {
            return s.to_string();
        }
    }
    // 2 & 3. Walk `outputs` then `steps` for text blocks, joining consecutive
    // text content the way the SDK's `output_text` helper does.
    for key in ["outputs", "steps"] {
        if let Some(arr) = body.get(key).and_then(|v| v.as_array()) {
            let joined = collect_text_blocks(arr);
            if !joined.is_empty() {
                return joined;
            }
        }
    }
    String::new()
}

/// Collect and join every `{type:"text", text}` block found in an array of
/// output/step objects. Blocks may carry text directly (`block.text`) or nested
/// under `block.content` (an object or an array of typed parts) — both the
/// streaming `step.delta` shape and the SDK helper description show this
/// nesting, so we walk both.
fn collect_text_blocks(arr: &[serde_json::Value]) -> String {
    let mut out = String::new();
    for block in arr {
        // Direct `{type:"text", text:"…"}`.
        if block.get("type").and_then(|v| v.as_str()) == Some("text") {
            if let Some(t) = block.get("text").and_then(|v| v.as_str()) {
                out.push_str(t);
                continue;
            }
        }
        // Nested `content`: either a single typed object or an array of parts.
        match block.get("content") {
            Some(serde_json::Value::Array(parts)) => out.push_str(&collect_text_blocks(parts)),
            Some(obj @ serde_json::Value::Object(_))
                if obj.get("type").and_then(|v| v.as_str()) == Some("text") =>
            {
                if let Some(t) = obj.get("text").and_then(|v| v.as_str()) {
                    out.push_str(t);
                }
            }
            _ => {}
        }
    }
    out
}

// ---------------------------------------------------------------------------
// SSE event → AnswerChunk mapping.
// ---------------------------------------------------------------------------

/// Parse one SSE event into zero or more [`AnswerChunk`]s.
///
/// Gemini Interactions API SSE event types (doc-confirmed):
/// `interaction.created`, `step.start`, `step.delta`, `step.stop`,
/// `interaction.completed`, `error`, `done` (with `data: [DONE]`).
///
/// Mapping:
/// - `interaction.created` → `Started { session_id: Some(id) }` (first only)
/// - `step.delta` with a `delta.type == "text"` → `Delta(text)`
/// - `step.delta` of `thought_summary` / `arguments_delta` / `image` → dropped
///   (telemetry / not rendered in the meeting overlay)
/// - `step.start` / `step.stop` → dropped (telemetry)
/// - `interaction.completed` → `Done { cost_usd: None }`
/// - `error` → `Error(message)`
/// - `done` → nothing (the stream just ends)
///
/// `started` is a caller-tracked flag so a `Started` is emitted at most once.
/// The `data: [DONE]` sentinel on the `done` event is handled gracefully (it is
/// not JSON). Unknown event types and malformed JSON are dropped fail-soft —
/// the same defensive posture as the Cursor SSE parser.
pub fn parse_sse_event(event_name: &str, data: &str, started: &mut bool) -> Vec<AnswerChunk> {
    // The terminal `done` event carries the literal `[DONE]` sentinel, not
    // JSON — handle it before attempting a parse.
    if event_name == "done" || data.trim() == "[DONE]" {
        return Vec::new();
    }
    if data.is_empty() {
        return Vec::new();
    }
    let Ok(v) = serde_json::from_str::<serde_json::Value>(data) else {
        // Non-JSON event data — be defensive: drop instead of crashing.
        return Vec::new();
    };

    match event_name {
        "interaction.created" => {
            if *started {
                return Vec::new();
            }
            *started = true;
            let id = v
                .get("id")
                .or_else(|| v.get("interaction").and_then(|i| i.get("id")))
                .and_then(|s| s.as_str())
                .map(|s| s.to_string());
            vec![AnswerChunk::Started { session_id: id }]
        }
        "step.delta" => {
            // Only `text` deltas are surfaced to the user. The delta object is
            // `{"type":"text","text":"…"}` (verbatim from the docs).
            let delta = v.get("delta").unwrap_or(&serde_json::Value::Null);
            if delta.get("type").and_then(|t| t.as_str()) == Some("text") {
                if let Some(text) = delta.get("text").and_then(|t| t.as_str()) {
                    if !text.is_empty() {
                        let mut out = Vec::new();
                        if !*started {
                            *started = true;
                            out.push(AnswerChunk::Started { session_id: None });
                        }
                        out.push(AnswerChunk::Delta(text.to_string()));
                        return out;
                    }
                }
            }
            // thought_summary / arguments_delta / image / anything else: drop.
            Vec::new()
        }
        "interaction.completed" => {
            let mut out = Vec::new();
            if !*started {
                *started = true;
                out.push(AnswerChunk::Started { session_id: None });
            }
            // Gemini bills by tokens; no per-call USD cost on the wire.
            out.push(AnswerChunk::Done { cost_usd: None });
            out
        }
        "error" => {
            let mut out = Vec::new();
            if !*started {
                *started = true;
                out.push(AnswerChunk::Started { session_id: None });
            }
            // Error events may carry `{message}` or the Google `{error:{message}}`
            // envelope — try both.
            let msg = v
                .get("message")
                .and_then(|s| s.as_str())
                .or_else(|| {
                    v.get("error")
                        .and_then(|e| e.get("message"))
                        .and_then(|s| s.as_str())
                })
                .unwrap_or("Gemini Cloud reported an error")
                .to_string();
            out.push(AnswerChunk::Error(msg));
            out
        }
        // Telemetry / lifecycle events — drop without surfacing.
        "step.start" | "step.stop" => Vec::new(),
        // Unknown event type: drop fail-soft (a future Gemini event must not
        // break the stream parser).
        _ => Vec::new(),
    }
}

// ---------------------------------------------------------------------------
// Error mapping (HTTP status → user guidance) + error-envelope parsing.
// ---------------------------------------------------------------------------

/// Map an HTTP status code from the Gemini Developer API onto a stable,
/// user-facing guidance string. Used by the daemon's "agent not ready" path so
/// we never escalate to Bluey's own AI on a vendor error.
///
/// All `generativelanguage.googleapis.com` errors use the `google.rpc.Status`
/// envelope `{"error":{"code","message","status"}}` (see dossier §7).
pub fn guidance_for_status(status: u16) -> &'static str {
    match status {
        400 => {
            "Bluey sent Gemini Cloud a malformed request, or the Gemini API isn't \
                enabled for this key (internal error)."
        }
        401 | 403 => {
            "Your Google Gemini API key was rejected by Gemini Cloud. Reconnect \
                Gemini Cloud from Bluey settings, and make sure it's an AI Studio key \
                (starts with AIza), not a Vertex AI credential."
        }
        404 => "Gemini Cloud could not find that interaction or sandbox — it may have expired.",
        429 => {
            "Gemini Cloud is rate-limiting your account (quota exhausted). Try again \
                shortly, or check your plan and billing."
        }
        500 | 503 => "Gemini Cloud is temporarily unavailable.",
        504 => "The Gemini Cloud agent run timed out.",
        _ => "Gemini Cloud rejected the request.",
    }
}

/// Best-effort parse of the Google error envelope
/// `{"error":{"code":N,"message":"…","status":"…"}}`. Returns the `message`
/// when present; falls back to the raw body otherwise.
pub fn parse_error_body(body: &[u8]) -> String {
    if let Ok(v) = serde_json::from_slice::<serde_json::Value>(body) {
        if let Some(msg) = v
            .get("error")
            .and_then(|e| e.get("message"))
            .and_then(|m| m.as_str())
        {
            if !msg.is_empty() {
                return msg.to_string();
            }
        }
    }
    String::from_utf8_lossy(body).to_string()
}

/// Render the user-facing guidance for a failed response: the status-based
/// guidance, plus the vendor's verbatim error message in parentheses when the
/// body carried one. Pure; never panics on a malformed envelope, and NEVER
/// reflects the token (the envelope doesn't carry it, and we only read
/// `error.message`).
pub fn render_error_message(response: &HttpResponse) -> String {
    let base = guidance_for_status(response.status);
    let detail = parse_error_body(&response.body);
    // Only append the detail when it's a real, distinct vendor message (not the
    // raw body falling through as the same text, and not empty).
    if !detail.is_empty() && detail.len() < 300 && detail != base {
        format!("{base} ({detail})")
    } else {
        base.to_string()
    }
}

// ---------------------------------------------------------------------------
// Dispatcher-side helper.
// ---------------------------------------------------------------------------

/// Dispatcher-side helper: parse a synchronous `POST /v1beta/interactions`
/// response body into the `(session_id, answer_text)` the turn-shaped cloud
/// dispatcher emits as `Started → Delta → Done`. Stays free of cross-adapter
/// type coupling — the dispatcher reads exactly what it needs out of the tuple.
pub fn parse_answer_for_dispatch(body: &[u8]) -> Result<(Option<String>, String)> {
    let v: serde_json::Value = serde_json::from_slice(body)
        .map_err(|e| anyhow!("Gemini Cloud response was not valid JSON: {e}"))?;
    let parsed = parse_interaction_response(&v)?;
    Ok((parsed.id, parsed.text))
}

/// Convenience: fire the synchronous interaction call using the supplied
/// transport + credential store and return the parsed answer. Async because the
/// transport is async; a thin wrapper used by the dispatcher and integration
/// tests.
pub async fn run_interaction(
    transport: &CloudHttpsTransport,
    creds: &dyn super::keychain::VendorCredentialStore,
    prompt: &str,
) -> Result<InteractionResult> {
    let request = interaction_request(prompt);
    let response = transport
        .send(
            VENDOR,
            INTERACTIONS_PATH,
            &request,
            GEMINI_AUTH,
            creds,
            CREDENTIAL_KEY,
        )
        .await?;
    if !(200..300).contains(&response.status) {
        return Err(anyhow!("{}", render_error_message(&response)));
    }
    let body: serde_json::Value = serde_json::from_slice(&response.body)
        .map_err(|e| anyhow!("Gemini Cloud returned non-JSON response: {e}"))?;
    parse_interaction_response(&body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cloud::keychain::{MemoryCredentialStore, VendorCredentialStore};

    // ---- entry shape -----------------------------------------------------

    #[test]
    fn entry_has_required_shape() {
        assert_eq!(ENTRY.kind_tag, KindTag::GeminiCloud);
        assert_eq!(ENTRY.vendor_short, "gemini_cloud");
        assert_eq!(ENTRY.base_url, "https://generativelanguage.googleapis.com");
        // The ONLY turn-shaped vendor in the table.
        assert!(
            !ENTRY.task_shaped,
            "Gemini Cloud is turn-shaped (synchronous)"
        );
        // BYOT, billed per token to the user's own Gemini API account.
        assert_eq!(ENTRY.billing_model, BillingModel::ApiCredits);
    }

    #[test]
    fn entry_consent_warning_states_byot_sandbox_and_revocation() {
        let w = ENTRY.consent_warning;
        let lower = w.to_lowercase();
        // The non-obvious promises the user MUST see.
        assert!(lower.contains("sandbox"), "must mention the Google sandbox");
        assert!(
            lower.contains("your own google gemini api account"),
            "must state BYOT billing"
        );
        assert!(
            lower.contains("free tier"),
            "must disclose free-tier data-use caveat"
        );
        assert!(
            w.contains("aistudio.google.com"),
            "must give the revocation URL"
        );
        assert!(
            lower.contains("keychain"),
            "must say the key goes in the keychain"
        );
    }

    // ---- transport / auth shape (DATA contract) --------------------------

    #[test]
    fn transport_carries_api_revision_header_and_goog_auth() {
        match TRANSPORT {
            CloudTransport::Https {
                base_url,
                headers,
                auth,
                endpoints,
            } => {
                assert_eq!(base_url, "https://generativelanguage.googleapis.com");
                // The schema header MUST be carried as DATA, never a code branch.
                assert!(
                    headers.contains(&("Api-Revision", "2026-05-20")),
                    "Api-Revision must be a data header on the row, got {headers:?}"
                );
                // Auth is x-goog-api-key with NO prefix (token is the whole value).
                assert_eq!(
                    auth,
                    CloudAuth::HeaderToken {
                        header_name: "x-goog-api-key",
                        prefix: "",
                    }
                );
                // Turn-shaped: the interactions path is the session-create + stream
                // path; task-shape fields MUST be absent.
                assert_eq!(endpoints.create_session, Some(INTERACTIONS_PATH));
                assert_eq!(endpoints.stream_events, Some(INTERACTIONS_PATH));
                assert!(endpoints.create_task.is_none());
                assert!(endpoints.get_task_status.is_none());
            }
            CloudTransport::None => panic!("Gemini Cloud row must be Https-shaped"),
        }
    }

    #[test]
    fn auth_header_value_is_the_bare_key() {
        // x-goog-api-key carries the WHOLE key, no "Bearer " prefix — same
        // shape as Anthropic's x-api-key.
        let client = reqwest::Client::new();
        let req = GEMINI_AUTH
            .apply(client.get("http://localhost/"), "AIzaSyTESTKEY")
            .build()
            .expect("build request");
        let value = req
            .headers()
            .get("x-goog-api-key")
            .expect("x-goog-api-key header set")
            .to_str()
            .unwrap();
        assert_eq!(value, "AIzaSyTESTKEY");
        // And it must NOT have set an Authorization header.
        assert!(req.headers().get("authorization").is_none());
    }

    // ---- request body shape (the contract NEEDS-LIVE-VERIFY guards) -------

    #[test]
    fn interaction_body_matches_managed_agent_shape() {
        let body = build_interaction_body("Summarize the latest CI failures", false);
        // Each field asserted individually so a future drift surfaces the exact
        // field name that changed.
        assert_eq!(body["agent"].as_str(), Some("antigravity-preview-05-2026"));
        assert_eq!(
            body["input"].as_str(),
            Some("Summarize the latest CI failures")
        );
        assert_eq!(body["environment"].as_str(), Some("remote"));
        assert_eq!(body["stream"].as_bool(), Some(false));
    }

    #[test]
    fn stream_body_sets_stream_true() {
        let body = build_interaction_body("hi", true);
        assert_eq!(body["stream"].as_bool(), Some(true));
    }

    #[test]
    fn interaction_request_targets_documented_path_with_widened_timeout() {
        let req = interaction_request("p");
        assert_eq!(
            req.url,
            "https://generativelanguage.googleapis.com/v1beta/interactions"
        );
        assert_eq!(req.method, reqwest::Method::POST);
        assert!(req.json_body.is_some());
        // The sandbox agent loop needs more than the 30s default.
        assert_eq!(req.timeout, Some(ANSWER_TIMEOUT));
        assert!(ANSWER_TIMEOUT > super::super::transport::DEFAULT_TIMEOUT);
    }

    #[test]
    fn stream_request_sets_accept_event_stream() {
        let req = interaction_stream_request("p");
        assert!(req
            .extra_headers
            .iter()
            .any(|(n, v)| n.eq_ignore_ascii_case("accept") && v == "text/event-stream"));
        assert_eq!(
            req.json_body.as_ref().unwrap()["stream"].as_bool(),
            Some(true)
        );
    }

    // ---- synchronous response parsing (all three documented shapes) -------

    #[test]
    fn parse_response_reads_output_text_sugar() {
        let body = json!({
            "id": "int_123",
            "status": "completed",
            "environment_id": "env_abc",
            "output_text": "Here is the summary.",
        });
        let r = parse_interaction_response(&body).unwrap();
        assert_eq!(r.id.as_deref(), Some("int_123"));
        assert_eq!(r.environment_id.as_deref(), Some("env_abc"));
        assert_eq!(r.status.as_deref(), Some("completed"));
        assert_eq!(r.text, "Here is the summary.");
    }

    #[test]
    fn parse_response_walks_outputs_array() {
        // No output_text sugar — must walk the `outputs` array and join text.
        let body = json!({
            "id": "int_1",
            "status": "completed",
            "outputs": [
                { "type": "thought", "text": "thinking..." },
                { "type": "text", "text": "Part one. " },
                { "type": "text", "text": "Part two." }
            ],
        });
        let r = parse_interaction_response(&body).unwrap();
        assert_eq!(r.text, "Part one. Part two.");
    }

    #[test]
    fn parse_response_walks_steps_array_with_nested_content() {
        // The May-2026 rename: `steps` instead of `outputs`, text nested under
        // `content` as an array of typed parts.
        let body = json!({
            "id": "int_2",
            "status": "completed",
            "steps": [
                {
                    "type": "model_output",
                    "content": [
                        { "type": "text", "text": "Nested answer." }
                    ]
                }
            ],
        });
        let r = parse_interaction_response(&body).unwrap();
        assert_eq!(r.text, "Nested answer.");
    }

    #[test]
    fn parse_response_reads_environment_id_nested() {
        let body = json!({
            "id": "int_3",
            "output_text": "ok",
            "environment": { "id": "env_nested" },
        });
        let r = parse_interaction_response(&body).unwrap();
        assert_eq!(r.environment_id.as_deref(), Some("env_nested"));
    }

    #[test]
    fn parse_response_errors_when_no_text_anywhere() {
        // Defensive: a body with no recoverable text is an error (the dispatcher
        // surfaces it honestly rather than emitting an empty answer).
        let body = json!({ "id": "int_x", "status": "in_progress", "steps": [] });
        assert!(parse_interaction_response(&body).is_err());
    }

    // ---- SSE event mapping -----------------------------------------------

    #[test]
    fn sse_interaction_created_emits_started_with_id() {
        let mut started = false;
        let out = parse_sse_event(
            "interaction.created",
            r#"{"id":"int_7","status":"in_progress"}"#,
            &mut started,
        );
        assert_eq!(
            out,
            vec![AnswerChunk::Started {
                session_id: Some("int_7".to_string()),
            }]
        );
        assert!(started);
    }

    #[test]
    fn sse_second_created_is_dropped() {
        let mut started = true;
        let out = parse_sse_event("interaction.created", r#"{"id":"int_7"}"#, &mut started);
        assert!(out.is_empty());
    }

    #[test]
    fn sse_text_delta_emits_delta() {
        // The verbatim doc shape:
        // {"index":0,"delta":{"type":"text","text":"1, 2, 3"},"event_type":"step.delta"}
        let mut started = true;
        let out = parse_sse_event(
            "step.delta",
            r#"{"index":0,"delta":{"type":"text","text":"1, 2, 3"},"event_type":"step.delta"}"#,
            &mut started,
        );
        assert_eq!(out, vec![AnswerChunk::Delta("1, 2, 3".to_string())]);
    }

    #[test]
    fn sse_text_delta_before_created_emits_started_first() {
        let mut started = false;
        let out = parse_sse_event(
            "step.delta",
            r#"{"delta":{"type":"text","text":"hi"}}"#,
            &mut started,
        );
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
    fn sse_non_text_deltas_are_dropped() {
        // thought_summary / arguments_delta / image are telemetry, not answer.
        for data in [
            r#"{"delta":{"type":"thought_summary","content":{"type":"text","text":"hmm"}}}"#,
            r#"{"delta":{"type":"arguments_delta","arguments":"{\"q\":1}"}}"#,
            r#"{"delta":{"type":"image","mime_type":"image/jpeg","data":"abc"}}"#,
        ] {
            let mut started = true;
            let out = parse_sse_event("step.delta", data, &mut started);
            assert!(out.is_empty(), "non-text delta should drop, got {out:?}");
        }
    }

    #[test]
    fn sse_completed_emits_done() {
        let mut started = true;
        let out = parse_sse_event(
            "interaction.completed",
            r#"{"id":"int_7","usage":{"total_tokens":42}}"#,
            &mut started,
        );
        assert_eq!(out, vec![AnswerChunk::Done { cost_usd: None }]);
    }

    #[test]
    fn sse_error_emits_error_message() {
        let mut started = true;
        // Plain {message}.
        let out = parse_sse_event("error", r#"{"message":"model overloaded"}"#, &mut started);
        assert_eq!(
            out,
            vec![AnswerChunk::Error("model overloaded".to_string())]
        );
        // Google envelope {error:{message}}.
        let mut started2 = true;
        let out2 = parse_sse_event(
            "error",
            r#"{"error":{"code":429,"message":"quota","status":"RESOURCE_EXHAUSTED"}}"#,
            &mut started2,
        );
        assert_eq!(out2, vec![AnswerChunk::Error("quota".to_string())]);
    }

    #[test]
    fn sse_done_sentinel_and_lifecycle_events_drop() {
        // `done` carries the literal [DONE] (not JSON) and must not crash.
        let mut started = true;
        assert!(parse_sse_event("done", "[DONE]", &mut started).is_empty());
        // step.start / step.stop are telemetry.
        for ev in ["step.start", "step.stop"] {
            let mut s = true;
            assert!(parse_sse_event(ev, r#"{"index":0}"#, &mut s).is_empty());
        }
    }

    #[test]
    fn sse_unknown_event_and_malformed_json_drop_fail_soft() {
        let mut started = true;
        assert!(parse_sse_event("future.event", r#"{"x":1}"#, &mut started).is_empty());
        // Malformed JSON on a known event must not propagate.
        let mut s2 = true;
        assert!(parse_sse_event("step.delta", "not json", &mut s2).is_empty());
    }

    // ---- error mapping ----------------------------------------------------

    #[test]
    fn guidance_distinguishes_auth_404_429_5xx() {
        let g401 = guidance_for_status(401);
        let g403 = guidance_for_status(403);
        let g404 = guidance_for_status(404);
        let g429 = guidance_for_status(429);
        let g500 = guidance_for_status(500);
        let g503 = guidance_for_status(503);
        // 401 and 403 share the auth guidance; the rest are distinct.
        assert_eq!(g401, g403);
        assert_eq!(g500, g503, "5xx share the 'unavailable' guidance");
        for (a, b) in [(g401, g404), (g404, g429), (g429, g500)] {
            assert_ne!(a, b);
        }
        for g in [g401, g404, g429, g500] {
            assert!(!g.is_empty());
        }
        // The auth message must steer the user away from a Vertex credential.
        assert!(g401.to_lowercase().contains("aistudio") || g401.contains("AIza"));
    }

    #[test]
    fn guidance_for_unknown_status_falls_through() {
        assert!(!guidance_for_status(418).is_empty());
    }

    #[test]
    fn parse_error_body_extracts_google_envelope_message() {
        let body = br#"{"error":{"code":403,"message":"PERMISSION_DENIED on key","status":"PERMISSION_DENIED"}}"#;
        assert_eq!(parse_error_body(body), "PERMISSION_DENIED on key");
    }

    #[test]
    fn parse_error_body_falls_back_to_raw_for_unknown_shape() {
        assert_eq!(parse_error_body(b"<html>502</html>"), "<html>502</html>");
    }

    #[test]
    fn render_error_message_appends_vendor_detail() {
        let body = br#"{"error":{"code":429,"message":"Resource has been exhausted (e.g. check quota).","status":"RESOURCE_EXHAUSTED"}}"#;
        let response = HttpResponse {
            status: 429,
            body: body.to_vec(),
            request_id_present: true,
        };
        let rendered = render_error_message(&response);
        assert!(rendered.contains("rate-limit"));
        assert!(rendered.contains("exhausted"));
    }

    #[test]
    fn render_error_message_tolerates_non_json_body() {
        let response = HttpResponse {
            status: 503,
            body: b"upstream connect error".to_vec(),
            request_id_present: false,
        };
        let rendered = render_error_message(&response);
        assert!(rendered.contains("unavailable"));
    }

    // ---- dispatcher helper ------------------------------------------------

    #[test]
    fn parse_answer_for_dispatch_returns_id_and_text() {
        let body = br#"{"id":"int_9","status":"completed","output_text":"Done."}"#;
        let (id, text) = parse_answer_for_dispatch(body).unwrap();
        assert_eq!(id.as_deref(), Some("int_9"));
        assert_eq!(text, "Done.");
    }

    #[test]
    fn parse_answer_for_dispatch_errors_on_invalid_json() {
        let err = parse_answer_for_dispatch(b"not json").unwrap_err();
        assert!(err.to_string().to_lowercase().contains("json"));
    }

    // ---- wiremock round-trips: full adapter path, token never leaks -------

    #[tokio::test]
    async fn run_interaction_round_trips_answer_on_200() {
        use wiremock::matchers::{body_partial_json, header, header_exists, method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        // Assert we send EXACTLY the documented auth header, the Api-Revision
        // header, and the managed-agent body shape.
        Mock::given(method("POST"))
            .and(path("/v1beta/interactions"))
            .and(header_exists("x-goog-api-key"))
            .and(header("x-goog-api-key", "AIzaTESTKEY"))
            .and(header("api-revision", "2026-05-20"))
            .and(body_partial_json(json!({
                "agent": "antigravity-preview-05-2026",
                "environment": "remote",
                "stream": false,
            })))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("x-request-id", "req-1")
                    .set_body_json(json!({
                        "id": "int_round",
                        "status": "completed",
                        "environment_id": "env_round",
                        "output_text": "The build passed.",
                    })),
            )
            .mount(&server)
            .await;

        let creds = MemoryCredentialStore::new(VENDOR);
        creds.save(CREDENTIAL_KEY, "AIzaTESTKEY").unwrap();

        // Build a request against the mock server's URL, then carry the
        // Api-Revision header the way the dispatcher does (off the row's
        // `headers` slice) so the wiremock header matcher is satisfied.
        let transport = CloudHttpsTransport::new();
        let req = HttpRequest::post_json(
            format!("{}/v1beta/interactions", server.uri()),
            build_interaction_body("Did the build pass?", false),
        )
        .with_header(API_REVISION_HEADER, API_REVISION_VALUE);
        let resp = transport
            .send(
                VENDOR,
                INTERACTIONS_PATH,
                &req,
                GEMINI_AUTH,
                &creds,
                CREDENTIAL_KEY,
            )
            .await
            .expect("send");

        assert_eq!(resp.status, 200);
        assert!(resp.request_id_present);
        let (id, text) = parse_answer_for_dispatch(&resp.body).expect("parse");
        assert_eq!(id.as_deref(), Some("int_round"));
        assert_eq!(text, "The build passed.");
        // The response body must NEVER carry the token back.
        assert!(!resp.body_text().contains("AIzaTESTKEY"));
    }

    #[tokio::test]
    async fn run_interaction_surfaces_403_without_leaking_token() {
        use wiremock::matchers::{header, method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1beta/interactions"))
            .and(header("x-goog-api-key", "AIzaBADKEY"))
            .respond_with(ResponseTemplate::new(403).set_body_json(json!({
                "error": {
                    "code": 403,
                    "message": "API key not valid. Please pass a valid API key.",
                    "status": "PERMISSION_DENIED"
                }
            })))
            .mount(&server)
            .await;

        let creds = MemoryCredentialStore::new(VENDOR);
        creds.save(CREDENTIAL_KEY, "AIzaBADKEY").unwrap();
        let transport = CloudHttpsTransport::new();
        let req = HttpRequest::post_json(
            format!("{}/v1beta/interactions", server.uri()),
            build_interaction_body("hello", false),
        );
        let resp = transport
            .send(
                VENDOR,
                INTERACTIONS_PATH,
                &req,
                GEMINI_AUTH,
                &creds,
                CREDENTIAL_KEY,
            )
            .await
            .expect("send returns the 4xx, not Err");
        assert_eq!(resp.status, 403);
        let rendered = render_error_message(&resp);
        // Honest, actionable guidance — and never the token.
        assert!(rendered.contains("rejected") || rendered.contains("Reconnect"));
        assert!(rendered.contains("API key not valid"));
        assert!(
            !rendered.contains("AIzaBADKEY"),
            "MUST never reflect the token"
        );
    }
}
