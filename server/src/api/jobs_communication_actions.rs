//! Account-scoped review and approval endpoints for outbound Jobs communication.

use axum::{
    extract::{Path, Query, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    Extension, Json,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    api::AppState,
    auth::AuthedAccount,
    db::jobs::{self, JobsCommunicationAction},
};

use super::jobs::{domain_error, internal, ApiError};

#[derive(Debug, Deserialize)]
pub(super) struct CommunicationActionsQuery {
    application_id: Option<String>,
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub(super) struct CreateCommunicationActionRequest {
    application_id: String,
    connection_id: String,
    source_message_id: Option<String>,
    kind: String,
    provider: String,
    idempotency_key: String,
    payload: Value,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CommunicationActionMutationRequest {
    action_revision: i64,
    payload_sha256: String,
}

#[derive(Debug, Serialize)]
pub(super) struct CommunicationActionSummary {
    id: String,
    application_id: String,
    connection_id: String,
    source_message_id: Option<String>,
    kind: String,
    provider: String,
    payload_sha256: String,
    status: String,
    action_revision: i64,
    execution_available: bool,
    execution_unavailable_reason: String,
    approved_at_ms: Option<i64>,
    dispatched_at_ms: Option<i64>,
    created_at_ms: i64,
    updated_at_ms: i64,
}

impl CommunicationActionSummary {
    fn from_action(
        action: JobsCommunicationAction,
        execution_available: bool,
        execution_unavailable_reason: String,
    ) -> Self {
        Self {
            id: action.id,
            application_id: action.application_id,
            connection_id: action.connection_id,
            source_message_id: action.source_message_id,
            kind: action.kind,
            provider: action.provider,
            payload_sha256: action.payload_sha256,
            status: action.status,
            action_revision: action.action_revision,
            execution_available,
            execution_unavailable_reason,
            approved_at_ms: action.approved_at_ms,
            dispatched_at_ms: action.dispatched_at_ms,
            created_at_ms: action.created_at_ms,
            updated_at_ms: action.updated_at_ms,
        }
    }
}

#[derive(Debug, Serialize)]
pub(super) struct CommunicationActionDetail {
    #[serde(flatten)]
    summary: CommunicationActionSummary,
    connection_account_label: String,
    source_context: Option<CommunicationSourceContext>,
    payload: Value,
}

#[derive(Debug, Serialize)]
pub(super) struct CommunicationSourceContext {
    sender: String,
    reply_target: String,
    subject: String,
    received_at_ms: i64,
}

impl CommunicationActionDetail {
    fn from_action(
        action: JobsCommunicationAction,
        execution_available: bool,
        execution_unavailable_reason: String,
        connection_account_label: String,
        source_context: Option<CommunicationSourceContext>,
    ) -> Self {
        let payload = action.payload.clone();
        Self {
            summary: CommunicationActionSummary::from_action(
                action,
                execution_available,
                execution_unavailable_reason,
            ),
            connection_account_label,
            source_context,
            payload,
        }
    }
}

fn action_summary(
    state: &AppState,
    account_id: &str,
    action: JobsCommunicationAction,
) -> anyhow::Result<CommunicationActionSummary> {
    let (available, reason) =
        jobs::communication_action_execution_readiness(&state.pool, account_id, &action)?;
    Ok(CommunicationActionSummary::from_action(
        action, available, reason,
    ))
}

fn action_detail(
    state: &AppState,
    account_id: &str,
    action: JobsCommunicationAction,
) -> anyhow::Result<CommunicationActionDetail> {
    let (available, reason) =
        jobs::communication_action_execution_readiness(&state.pool, account_id, &action)?;
    let mailbox = jobs::mailbox_connection(&state.pool, account_id, &action.connection_id)?
        .ok_or_else(|| anyhow::anyhow!("communication mailbox context is unavailable"))?;
    let account_label = bounded_header_context(&mailbox.account_label, 320, false)?;
    let source_context = if action.kind == "reply" {
        let source_id = action
            .source_message_id
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("communication reply source context is missing"))?;
        let message =
            jobs::provider_message(&state.pool, account_id, &action.connection_id, source_id)?
                .ok_or_else(|| {
                    anyhow::anyhow!("communication reply source context is unavailable")
                })?;
        let expected_connection_provider = match action.provider.as_str() {
            "gmail" | "google_calendar" => "gmail",
            "outlook_email" | "outlook_calendar" => "outlook",
            _ => anyhow::bail!("communication provider context is invalid"),
        };
        if message.application_id.as_deref() != Some(action.application_id.as_str())
            || message.connection_id != action.connection_id
            || message.provider != expected_connection_provider
            || mailbox.provider != expected_connection_provider
        {
            anyhow::bail!("communication reply source context does not match its authority")
        }
        jobs::validate_communication_reply_source(&action, &message)?;
        let sender = bounded_header_context(&message.sender, 320, false)?.to_ascii_lowercase();
        let reply_target = jobs::communication_source_reply_target(&action.provider, &message)?;
        let subject = bounded_header_context(&message.subject, 998, true)?;
        if message.received_at_ms <= 0 {
            anyhow::bail!("communication source received time is invalid")
        }
        Some(CommunicationSourceContext {
            sender,
            reply_target,
            subject,
            received_at_ms: message.received_at_ms,
        })
    } else {
        None
    };
    Ok(CommunicationActionDetail::from_action(
        action,
        available,
        reason,
        account_label,
        source_context,
    ))
}

fn bounded_header_context(
    value: &str,
    max_bytes: usize,
    allow_empty: bool,
) -> anyhow::Result<String> {
    let value = value.trim();
    if value.len() > max_bytes
        || (!allow_empty && value.is_empty())
        || !jobs::communication_review_text_is_safe(value)
    {
        anyhow::bail!("communication review context is invalid")
    }
    Ok(if value.is_empty() {
        "(no subject)".to_string()
    } else {
        value.to_string()
    })
}

fn private_json<T: Serialize>(status: StatusCode, value: T) -> Response {
    (
        status,
        [
            (header::CACHE_CONTROL, "private, no-store"),
            (header::PRAGMA, "no-cache"),
        ],
        Json(value),
    )
        .into_response()
}

pub(super) async fn communication_actions(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
    Query(query): Query<CommunicationActionsQuery>,
) -> Result<Response, ApiError> {
    let application_id = query
        .application_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    jobs::list_communication_actions(
        &state.pool,
        &account.id,
        application_id,
        query.limit.unwrap_or(50).clamp(1, 100),
    )
    .map_err(internal)?
    .into_iter()
    .map(|action| action_summary(&state, &account.id, action))
    .collect::<anyhow::Result<Vec<_>>>()
    .map(|value| private_json(StatusCode::OK, value))
    .map_err(internal)
}

pub(super) async fn communication_action(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
    Path(action_id): Path<String>,
) -> Result<Response, ApiError> {
    jobs::communication_action(&state.pool, &account.id, &action_id)
        .map_err(internal)?
        .ok_or_else(not_found)
        .and_then(|action| {
            action_detail(&state, &account.id, action)
                .map(|value| private_json(StatusCode::OK, value))
                .map_err(internal)
        })
}

pub(super) async fn create_communication_action(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
    Json(request): Json<CreateCommunicationActionRequest>,
) -> Result<Response, ApiError> {
    let action = JobsCommunicationAction {
        id: String::new(),
        application_id: request.application_id,
        connection_id: request.connection_id,
        source_message_id: request.source_message_id,
        kind: request.kind,
        provider: request.provider,
        idempotency_key: request.idempotency_key,
        payload: request.payload,
        payload_sha256: String::new(),
        authority_sha256: String::new(),
        status: String::new(),
        provider_object_id: String::new(),
        lease_owner: None,
        lease_kind: None,
        lease_expires_at_ms: None,
        active_attempt_id: None,
        next_attempt_at_ms: 0,
        attempt_count: 0,
        reconciliation_count: 0,
        action_revision: 1,
        approval_revision: 0,
        approved_authority_sha256: String::new(),
        approved_grant_revision: 0,
        approved_grant_sha256: String::new(),
        approved_at_ms: None,
        dispatched_at_ms: None,
        created_at_ms: 0,
        updated_at_ms: 0,
    };
    let (action, inserted) = jobs::create_communication_action(&state.pool, &account.id, &action)
        .map_err(domain_error)?;
    Ok(private_json(
        if inserted {
            StatusCode::CREATED
        } else {
            StatusCode::OK
        },
        action_detail(&state, &account.id, action).map_err(internal)?,
    ))
}

pub(super) async fn approve_communication_action(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
    Path(action_id): Path<String>,
    Json(request): Json<CommunicationActionMutationRequest>,
) -> Result<Response, ApiError> {
    jobs::approve_communication_action(
        &state.pool,
        &account.id,
        &action_id,
        request.action_revision,
        &request.payload_sha256,
    )
    .map_err(domain_error)?
    .ok_or_else(not_found)
    .and_then(|action| {
        action_detail(&state, &account.id, action)
            .map(|value| private_json(StatusCode::OK, value))
            .map_err(internal)
    })
}

pub(super) async fn cancel_communication_action(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
    Path(action_id): Path<String>,
    Json(request): Json<CommunicationActionMutationRequest>,
) -> Result<Response, ApiError> {
    jobs::cancel_communication_action(
        &state.pool,
        &account.id,
        &action_id,
        request.action_revision,
        &request.payload_sha256,
    )
    .map_err(domain_error)?
    .ok_or_else(not_found)
    .and_then(|action| {
        action_detail(&state, &account.id, action)
            .map(|value| private_json(StatusCode::OK, value))
            .map_err(internal)
    })
}

fn not_found() -> ApiError {
    (
        StatusCode::NOT_FOUND,
        "Communication draft not found.".to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn summary_does_not_expose_payload_or_worker_state() {
        let action = JobsCommunicationAction {
            id: "action-1".to_string(),
            application_id: "application-1".to_string(),
            connection_id: "connection-1".to_string(),
            source_message_id: Some("message-1".to_string()),
            kind: "reply".to_string(),
            provider: "gmail".to_string(),
            idempotency_key: "reply:message-1".to_string(),
            payload: json!({"body_text": "Private draft"}),
            payload_sha256: "payload-hash".to_string(),
            authority_sha256: "authority-hash".to_string(),
            status: "awaiting_approval".to_string(),
            provider_object_id: String::new(),
            lease_owner: Some("worker-private".to_string()),
            lease_kind: Some("dispatch".to_string()),
            lease_expires_at_ms: Some(123),
            active_attempt_id: Some("attempt-private".to_string()),
            next_attempt_at_ms: 123,
            attempt_count: 1,
            reconciliation_count: 0,
            action_revision: 7,
            approval_revision: 1,
            approved_authority_sha256: "authority-hash".to_string(),
            approved_grant_revision: 1,
            approved_grant_sha256: "grant-hash".to_string(),
            approved_at_ms: None,
            dispatched_at_ms: None,
            created_at_ms: 100,
            updated_at_ms: 101,
        };

        let encoded = serde_json::to_value(CommunicationActionSummary::from_action(
            action,
            false,
            "disabled".to_string(),
        ))
        .unwrap();
        assert!(encoded.get("payload").is_none());
        assert!(encoded.get("idempotency_key").is_none());
        assert!(encoded.get("lease_owner").is_none());
        assert!(encoded.get("attempt_count").is_none());
        assert_eq!(encoded["execution_available"], false);
        assert_eq!(encoded["execution_unavailable_reason"], "disabled");
    }

    #[test]
    fn source_context_exposes_reply_target_without_provider_metadata() {
        let encoded = serde_json::to_value(CommunicationSourceContext {
            sender: "sender@example.org".to_string(),
            reply_target: "talent@example.org".to_string(),
            subject: "Interview availability".to_string(),
            received_at_ms: 1_785_999_000_000,
        })
        .unwrap();
        assert_eq!(encoded["sender"], "sender@example.org");
        assert_eq!(encoded["reply_target"], "talent@example.org");
        assert_eq!(encoded["subject"], "Interview availability");
        assert_eq!(encoded["received_at_ms"], 1_785_999_000_000_i64);
        assert!(encoded.get("provider_id").is_none());
        assert!(encoded.get("conversation_id").is_none());
        assert!(encoded.get("rfc_message_id").is_none());
        assert!(encoded.get("body_text").is_none());
    }

    #[test]
    fn mutation_request_requires_exact_review_snapshot_shape() {
        let valid = json!({
            "action_revision": 7,
            "payload_sha256": "a".repeat(64),
        });
        assert!(serde_json::from_value::<CommunicationActionMutationRequest>(valid).is_ok());
        for invalid in [
            json!({"payload_sha256": "a".repeat(64)}),
            json!({"action_revision": 7}),
            json!({
                "action_revision": 7,
                "payload_sha256": "a".repeat(64),
                "unexpected": true,
            }),
            json!({"action_revision": "7", "payload_sha256": "a".repeat(64)}),
        ] {
            assert!(serde_json::from_value::<CommunicationActionMutationRequest>(invalid).is_err());
        }
    }

    #[test]
    fn communication_json_is_private_and_non_storable() {
        let response = private_json(StatusCode::OK, json!({"payload": "private draft"}));
        assert_eq!(
            response.headers().get(header::CACHE_CONTROL).unwrap(),
            "private, no-store"
        );
        assert_eq!(response.headers().get(header::PRAGMA).unwrap(), "no-cache");
    }
}
