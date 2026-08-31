//! Closed wire schema for metadata-only desktop support diagnostics.
//!
//! This parser intentionally does not accept arbitrary JSON. A schema bump is
//! required before a new field or event name can leave a customer device.

use axum::http::{header, HeaderMap, StatusCode};
use serde::Deserialize;
use serde_json::Value;

use crate::db::support_diagnostics::SUPPORT_DIAGNOSTIC_CONTENT_POLICY;

const BUNDLE_SCHEMA_VERSION: u32 = 1;
const EVENT_SCHEMA_VERSION: u32 = 2;
const MAX_EVENTS: usize = 4_096;
const MAX_COUNTER: u64 = 1_000_000_000_000;

const PROVIDERS: &[&str] = &[
    "anthropic",
    "assemblyai",
    "bluey_managed",
    "deepgram",
    "google",
    "groq",
    "local",
    "openai",
    "other",
];
const MODEL_FAMILIES: &[&str] = &[
    "bluey",
    "claude",
    "gemini",
    "gpt",
    "kimi",
    "llama",
    "mistral",
    "nova",
    "openai_reasoning",
    "other",
    "qwen",
    "whisper",
];
const ACTIONS: &[&str] = &[
    "active_page_capture_requested",
    "analyze_screen_requested",
    "ask_answer_sent",
    "ask_answer_skipped",
    "ask_requested",
    "attach_files_requested",
    "attach_requested",
    "autosend_answer_sent",
    "autosend_answer_skipped",
    "capture_start_requested",
    "capture_stop_requested",
    "close_requested",
    "context_list_requested",
    "hidden",
    "instructions_requested",
    "instructions_updated",
    "meeting_banner_action",
    "opacity_updated",
    "paste_text_requested",
    "ready",
    "recap_requested",
    "recording_start_requested",
    "recording_stop_requested",
    "remove_context_requested",
    "session_continue_requested",
    "session_delete_requested",
    "session_drawer_opened",
    "session_drawer_sessions_rendered",
    "session_list_requested",
    "session_new_requested",
    "session_open_requested",
    "session_rename_requested",
    "shortcuts_coachmark_dismissed",
    "shortcuts_coachmark_shown",
    "shortcuts_overlay_opened",
    "shown",
    "sign_in_requested",
    "theme_changed",
    "transcript_buffer_consumed",
    "transcript_buffer_skip_consumed",
    "transcript_clear_requested",
    "transcript_context_cleared",
];
const ERROR_CATEGORIES: &[&str] = &[
    "authentication",
    "billing",
    "cancelled",
    "capacity",
    "dropped",
    "failed",
    "internal",
    "network",
    "none",
    "ok",
    "rate_limit",
    "response_db_write_failed",
    "runtime_setup",
    "runtime_unavailable",
    "safety",
    "start_canceled",
    "timed_out",
    "timeout",
    "unknown",
];
const QUESTION_INTENTS: &[&str] = &[
    "code_explanation",
    "code_or_debug",
    "explanation",
    "general",
    "quick_explanation",
    "short_query",
    "system_design",
];
const ARTIFACT_TYPES: &[&str] = &[
    "code",
    "document",
    "none",
    "screen",
    "structured",
    "system_design",
];

#[derive(Debug)]
pub(super) struct ValidatedSupportDiagnosticBundle {
    pub schema_version: u32,
    pub event_count: usize,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Bundle {
    schema_version: u32,
    bundle_id: String,
    session_id: String,
    session_code: String,
    generated_at_ms: i64,
    content_policy: String,
    manifest: Manifest,
    events: Vec<Event>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    schema_version: u32,
    bundle_id: String,
    session_id: String,
    session_code: String,
    generated_at_ms: i64,
    updated_at_ms: i64,
    content_policy: String,
    record_counts: RecordCounts,
    excluded_content: Vec<String>,
    local_retention: LocalRetention,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RecordCounts {
    events: usize,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LocalRetention {
    uploaded_session_dirs_removed: bool,
    failed_upload_dirs_retention_days: u64,
    failed_upload_root_max_bytes: u64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Event {
    schema_version: u32,
    sequence: u64,
    kind: EventName,
    created_at_ms: u64,
    source: EventSource,
    content_policy: String,
    payload: Payload,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum EventSource {
    DesktopDiagnostic,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum EventName {
    OverlayUserAction,
    OverlayLifecycle,
    AnswerRequestAccepted,
    AnswerContextPrepared,
    AnswerCardCreated,
    AnswerStatusPresented,
    AnswerReplayStarted,
    AnswerRouteCompleted,
    AnswerFirstText,
    AnswerCompleted,
    AnswerFailed,
    AnswerSlowStart,
    NativeFirstTextRendered,
    NativeFinalRendered,
    TranscriptSettled,
    TranscriptBufferConsumed,
    AudioStartRequested,
    AudioCaptureReady,
    AudioStopRequested,
    AudioCaptureStopped,
    AudioFirstChunk,
    SttConnected,
    SttFirstPartial,
    SttFirstFinal,
    RagQueryCompleted,
    ModelAttemptStarted,
    ModelConnected,
    ModelFirstEvent,
    ModelFirstText,
    ModelAttemptCompleted,
    PersistenceCompleted,
    ContextWatchObserved,
    DiagnosticEventsDropped,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum Component {
    NativeOverlay,
    Daemon,
    Audio,
    Stt,
    Rag,
    Model,
    Persistence,
    Support,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum Outcome {
    Started,
    Succeeded,
    Failed,
    TimedOut,
    Dropped,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Payload {
    schema_version: u32,
    event_name: EventName,
    component: Component,
    outcome: Outcome,
    created_at_ms: u64,
    monotonic_offset_ms: u64,
    #[serde(default)]
    interaction_id: Option<String>,
    #[serde(default)]
    trace_id: Option<String>,
    #[serde(default)]
    request_id: Option<String>,
    #[serde(default)]
    audio_run_id: Option<String>,
    #[serde(default)]
    card_id: Option<String>,
    #[serde(default)]
    provider: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    route: Option<String>,
    #[serde(default)]
    action: Option<String>,
    #[serde(default)]
    error_category: Option<String>,
    #[serde(default)]
    duration_ms: Option<u64>,
    #[serde(default)]
    queue_wait_ms: Option<u64>,
    #[serde(default)]
    queue_depth: Option<u64>,
    #[serde(default)]
    queue_high_water: Option<u64>,
    #[serde(default)]
    attempt: Option<u64>,
    #[serde(default)]
    generation: Option<u64>,
    #[serde(default)]
    sequence: Option<u64>,
    #[serde(default)]
    streaming: Option<bool>,
    #[serde(default)]
    count: Option<u64>,
    #[serde(default)]
    bytes: Option<u64>,
    #[serde(default)]
    input_chars: Option<u64>,
    #[serde(default)]
    output_chars: Option<u64>,
    #[serde(default)]
    context_count: Option<u64>,
    #[serde(default)]
    document_count: Option<u64>,
    #[serde(default)]
    screenshot_count: Option<u64>,
    #[serde(default)]
    transcript_count: Option<u64>,
    #[serde(default)]
    memory_count: Option<u64>,
    #[serde(default)]
    source_count: Option<u64>,
    #[serde(default)]
    question_intent: Option<String>,
    #[serde(default)]
    artifact_type: Option<String>,
    #[serde(default)]
    dropped_count: Option<u64>,
    #[serde(default)]
    coalesced_count: Option<u64>,
    content_policy: String,
}

pub(super) fn validate_headers(headers: &HeaderMap) -> Result<(), (StatusCode, String)> {
    if !has_exact_single_header(headers, header::CONTENT_TYPE.as_str(), "application/json") {
        return Err((
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "Support diagnostics require application/json.".to_string(),
        ));
    }
    if !has_exact_single_header(headers, "x-bluey-audit-schema-version", "1")
        || !has_exact_single_header(
            headers,
            "x-bluey-content-policy",
            SUPPORT_DIAGNOSTIC_CONTENT_POLICY,
        )
    {
        return Err((
            StatusCode::BAD_REQUEST,
            "Support diagnostics schema headers are invalid.".to_string(),
        ));
    }
    Ok(())
}

fn has_exact_single_header(headers: &HeaderMap, name: &'static str, expected: &str) -> bool {
    let mut values = headers.get_all(name).iter();
    values
        .next()
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.trim() == expected)
        && values.next().is_none()
}

pub(super) fn validate_bundle(
    body: &[u8],
    path_session_id: &str,
    path_bundle_id: &str,
) -> Result<ValidatedSupportDiagnosticBundle, (StatusCode, String)> {
    let value: Value = serde_json::from_slice(body).map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            "Invalid support diagnostics JSON.".to_string(),
        )
    })?;
    reject_forbidden_content_keys(&value)?;
    let bundle: Bundle = serde_json::from_value(value).map_err(|_| invalid_schema_error())?;
    let future_limit = now_ms().saturating_add(10 * 60 * 1_000);

    if bundle.schema_version != BUNDLE_SCHEMA_VERSION
        || bundle.content_policy != SUPPORT_DIAGNOSTIC_CONTENT_POLICY
        || bundle.session_id != path_session_id
        || bundle.bundle_id != path_bundle_id
        || bundle.generated_at_ms <= 0
        || bundle.generated_at_ms > future_limit
    {
        return invalid_schema();
    }
    validate_session_code(path_session_id, &bundle.session_code)?;
    if bundle.events.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "Support diagnostics must contain at least one event.".to_string(),
        ));
    }
    if bundle.events.len() > MAX_EVENTS {
        return Err((
            StatusCode::PAYLOAD_TOO_LARGE,
            "Support diagnostics event count is outside the allowed range.".to_string(),
        ));
    }

    let manifest = &bundle.manifest;
    if manifest.schema_version != BUNDLE_SCHEMA_VERSION
        || manifest.bundle_id != bundle.bundle_id
        || manifest.session_id != bundle.session_id
        || manifest.session_code != bundle.session_code
        || manifest.generated_at_ms != bundle.generated_at_ms
        || manifest.updated_at_ms <= 0
        || manifest.updated_at_ms > future_limit
        || manifest.content_policy != SUPPORT_DIAGNOSTIC_CONTENT_POLICY
        || manifest.record_counts.events != bundle.events.len()
        || !manifest.local_retention.uploaded_session_dirs_removed
        || !(1..=30).contains(&manifest.local_retention.failed_upload_dirs_retention_days)
        || manifest.local_retention.failed_upload_root_max_bytes == 0
        || manifest.local_retention.failed_upload_root_max_bytes > 10 * 1024 * 1024 * 1024
    {
        return invalid_schema();
    }
    let expected_excluded = [
        "questions",
        "answers",
        "transcripts",
        "prompts",
        "audio",
        "screenshots",
        "files",
        "paths",
        "urls",
        "clipboard",
        "tokens",
        "raw_errors",
    ];
    if manifest.excluded_content.len() != expected_excluded.len()
        || !manifest
            .excluded_content
            .iter()
            .map(String::as_str)
            .eq(expected_excluded)
    {
        return invalid_schema();
    }

    let future_limit = future_limit as u64;
    let mut previous_sequence = None;
    for event in &bundle.events {
        if event.schema_version != BUNDLE_SCHEMA_VERSION
            || event.content_policy != SUPPORT_DIAGNOSTIC_CONTENT_POLICY
            || event.sequence == 0
            || event.created_at_ms == 0
            || event.created_at_ms > future_limit
            || event.source != EventSource::DesktopDiagnostic
            || event.kind != event.payload.event_name
            || event.payload.schema_version != EVENT_SCHEMA_VERSION
            || event.payload.content_policy != SUPPORT_DIAGNOSTIC_CONTENT_POLICY
            || event.payload.created_at_ms == 0
            || event.payload.created_at_ms > future_limit
        {
            return invalid_schema();
        }
        if previous_sequence.is_some_and(|previous| event.sequence <= previous) {
            return invalid_schema();
        }
        previous_sequence = Some(event.sequence);
        validate_payload(&event.payload)?;
    }

    Ok(ValidatedSupportDiagnosticBundle {
        schema_version: bundle.schema_version,
        event_count: bundle.events.len(),
    })
}

fn validate_session_code(session_id: &str, session_code: &str) -> Result<(), (StatusCode, String)> {
    if session_code.is_empty()
        || session_code.len() > 32
        || !session_code
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return invalid_schema();
    }
    if let Ok(session_uuid) = uuid::Uuid::parse_str(session_id) {
        if cue_core::short_session_code(session_uuid) != session_code {
            return invalid_schema();
        }
    }
    Ok(())
}

fn validate_payload(payload: &Payload) -> Result<(), (StatusCode, String)> {
    let _closed_values = (payload.component, payload.outcome, payload.streaming);
    for value in [
        payload.interaction_id.as_deref(),
        payload.trace_id.as_deref(),
        payload.request_id.as_deref(),
        payload.audio_run_id.as_deref(),
        payload.card_id.as_deref(),
    ]
    .into_iter()
    .flatten()
    {
        if uuid::Uuid::parse_str(value).is_err() {
            return invalid_schema();
        }
    }
    validate_closed_label(payload.provider.as_deref(), PROVIDERS)?;
    validate_closed_label(payload.model.as_deref(), MODEL_FAMILIES)?;
    validate_closed_label(payload.route.as_deref(), PROVIDERS)?;
    validate_closed_label(payload.action.as_deref(), ACTIONS)?;
    validate_closed_label(payload.error_category.as_deref(), ERROR_CATEGORIES)?;
    validate_closed_label(payload.question_intent.as_deref(), QUESTION_INTENTS)?;
    validate_closed_label(payload.artifact_type.as_deref(), ARTIFACT_TYPES)?;
    for value in [
        Some(payload.monotonic_offset_ms),
        payload.duration_ms,
        payload.queue_wait_ms,
        payload.queue_depth,
        payload.queue_high_water,
        payload.attempt,
        payload.generation,
        payload.sequence,
        payload.count,
        payload.bytes,
        payload.input_chars,
        payload.output_chars,
        payload.context_count,
        payload.document_count,
        payload.screenshot_count,
        payload.transcript_count,
        payload.memory_count,
        payload.source_count,
        payload.dropped_count,
        payload.coalesced_count,
    ]
    .into_iter()
    .flatten()
    {
        if value > MAX_COUNTER {
            return invalid_schema();
        }
    }
    Ok(())
}

fn validate_closed_label(
    value: Option<&str>,
    allowed: &[&str],
) -> Result<(), (StatusCode, String)> {
    if value.is_some_and(|value| !allowed.contains(&value)) {
        return invalid_schema();
    }
    Ok(())
}

fn reject_forbidden_content_keys(value: &Value) -> Result<(), (StatusCode, String)> {
    match value {
        Value::Object(object) => {
            for (key, value) in object {
                if key_is_forbidden(key) {
                    return Err((
                        StatusCode::BAD_REQUEST,
                        "Support diagnostics contain a forbidden content field.".to_string(),
                    ));
                }
                reject_forbidden_content_keys(value)?;
            }
        }
        Value::Array(values) => {
            for value in values {
                reject_forbidden_content_keys(value)?;
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }
    Ok(())
}

fn key_is_forbidden(key: &str) -> bool {
    if matches!(
        key,
        "excluded_content"
            | "content_policy"
            | "audio_run_id"
            | "screenshot_count"
            | "transcript_count"
            | "input_chars"
            | "output_chars"
            | "question_intent"
    ) {
        return false;
    }
    key.split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|part| !part.is_empty())
        .any(|part| {
            matches!(
                part.to_ascii_lowercase().as_str(),
                "question"
                    | "questions"
                    | "answer"
                    | "answers"
                    | "transcript"
                    | "transcripts"
                    | "prompt"
                    | "prompts"
                    | "audio"
                    | "screenshot"
                    | "screenshots"
                    | "file"
                    | "files"
                    | "path"
                    | "paths"
                    | "url"
                    | "urls"
                    | "clipboard"
                    | "token"
                    | "tokens"
                    | "text"
                    | "body"
                    | "email"
                    | "cookie"
                    | "cookies"
                    | "secret"
                    | "credentials"
                    | "authorization"
                    | "bearer"
                    | "raw"
                    | "stack"
                    | "stacktrace"
                    | "message"
                    | "hash"
                    | "sha256"
                    | "digest"
                    | "key"
            )
        })
}

fn invalid_schema<T>() -> Result<T, (StatusCode, String)> {
    Err(invalid_schema_error())
}

fn invalid_schema_error() -> (StatusCode, String) {
    (
        StatusCode::BAD_REQUEST,
        "Support diagnostics do not match the metadata-only schema.".to_string(),
    )
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    fn valid_bundle() -> (String, String, Value) {
        let session_id = "61b8c310-27de-4cc1-b598-c62bdcc07ba8".to_string();
        let session_code =
            cue_core::short_session_code(uuid::Uuid::parse_str(&session_id).unwrap());
        let bundle_id = format!("diagnostic-{session_code}-1780000000000-1024-1-7-536870912");
        let generated_at_ms = now_ms().saturating_sub(1_000);
        let event = serde_json::json!({
            "schema_version": 1,
            "sequence": 1,
            "kind": "answer_completed",
            "created_at_ms": generated_at_ms,
            "source": "desktop_diagnostic",
            "content_policy": "metadata_only",
            "payload": {
                "schema_version": 2,
                "event_name": "answer_completed",
                "component": "daemon",
                "outcome": "succeeded",
                "action": "ask_requested",
                "created_at_ms": generated_at_ms,
                "monotonic_offset_ms": 42,
                "interaction_id": "6e404b3e-392a-467b-858a-c975b31a03e5",
                "duration_ms": 42,
                "output_chars": 240,
                "content_policy": "metadata_only"
            }
        });
        let value = serde_json::json!({
            "schema_version": 1,
            "bundle_id": bundle_id,
            "session_id": session_id,
            "session_code": session_code,
            "generated_at_ms": generated_at_ms,
            "content_policy": "metadata_only",
            "manifest": {
                "schema_version": 1,
                "bundle_id": bundle_id,
                "session_id": session_id,
                "session_code": session_code,
                "generated_at_ms": generated_at_ms,
                "updated_at_ms": generated_at_ms,
                "content_policy": "metadata_only",
                "record_counts": { "events": 1 },
                "excluded_content": [
                    "questions", "answers", "transcripts", "prompts", "audio",
                    "screenshots", "files", "paths", "urls", "clipboard", "tokens",
                    "raw_errors"
                ],
                "local_retention": {
                    "uploaded_session_dirs_removed": true,
                    "failed_upload_dirs_retention_days": 7,
                    "failed_upload_root_max_bytes": 536870912
                }
            },
            "events": [event]
        });
        (session_id, bundle_id, value)
    }

    #[test]
    fn accepts_current_closed_schema_and_event_names() {
        let (session_id, bundle_id, value) = valid_bundle();
        let validated = validate_bundle(
            &serde_json::to_vec(&value).unwrap(),
            &session_id,
            &bundle_id,
        )
        .unwrap();
        assert_eq!(validated.event_count, 1);

        for event_name in [
            "model_first_event",
            "answer_route_completed",
            "overlay_lifecycle",
        ] {
            let mut current = value.clone();
            current["events"][0]["kind"] = serde_json::json!(event_name);
            current["events"][0]["payload"]["event_name"] = serde_json::json!(event_name);
            validate_bundle(
                &serde_json::to_vec(&current).unwrap(),
                &session_id,
                &bundle_id,
            )
            .unwrap();
        }
    }

    #[test]
    fn rejects_unknown_content_fields_and_path_body_mismatch() {
        let (session_id, bundle_id, mut value) = valid_bundle();
        value["events"][0]["payload"]["question_text"] = serde_json::json!("private question");
        let error = validate_bundle(
            &serde_json::to_vec(&value).unwrap(),
            &session_id,
            &bundle_id,
        )
        .unwrap_err();
        assert_eq!(error.0, StatusCode::BAD_REQUEST);
        assert!(error.1.contains("forbidden content"));

        let (_, _, mut clean) = valid_bundle();
        clean["events"][0]["payload"]["unexpected_metric"] = serde_json::json!(1);
        assert!(validate_bundle(
            &serde_json::to_vec(&clean).unwrap(),
            &session_id,
            &bundle_id,
        )
        .is_err());

        let (_, _, clean) = valid_bundle();
        assert!(validate_bundle(
            &serde_json::to_vec(&clean).unwrap(),
            "different-session",
            &bundle_id,
        )
        .is_err());
        assert!(validate_bundle(
            &serde_json::to_vec(&clean).unwrap(),
            &session_id,
            "different-bundle",
        )
        .is_err());
    }

    #[test]
    fn rejects_stale_event_names_and_unsafe_labels() {
        let (session_id, bundle_id, value) = valid_bundle();
        for stale_name in ["model_first_byte", "audio_capture_started"] {
            let mut stale = value.clone();
            stale["events"][0]["kind"] = serde_json::json!(stale_name);
            stale["events"][0]["payload"]["event_name"] = serde_json::json!(stale_name);
            assert!(validate_bundle(
                &serde_json::to_vec(&stale).unwrap(),
                &session_id,
                &bundle_id,
            )
            .is_err());
        }

        let mut unsafe_label = value;
        unsafe_label["events"][0]["payload"]["action"] = serde_json::json!("open/private");
        assert!(validate_bundle(
            &serde_json::to_vec(&unsafe_label).unwrap(),
            &session_id,
            &bundle_id,
        )
        .is_err());

        let (session_id, bundle_id, mut secret_shaped_label) = valid_bundle();
        secret_shaped_label["events"][0]["payload"]["action"] =
            serde_json::json!("private_secret_123");
        assert!(validate_bundle(
            &serde_json::to_vec(&secret_shaped_label).unwrap(),
            &session_id,
            &bundle_id,
        )
        .is_err());
    }

    #[test]
    fn requires_exact_schema_headers() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        headers.insert(
            "x-bluey-audit-schema-version",
            HeaderValue::from_static("1"),
        );
        headers.insert(
            "x-bluey-content-policy",
            HeaderValue::from_static("metadata_only"),
        );
        validate_headers(&headers).unwrap();

        headers.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/octet-stream"),
        );
        assert_eq!(
            validate_headers(&headers).unwrap_err().0,
            StatusCode::UNSUPPORTED_MEDIA_TYPE
        );

        headers.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        headers.append(
            "x-bluey-content-policy",
            HeaderValue::from_static("metadata_only"),
        );
        assert!(validate_headers(&headers).is_err());
    }
}
