//! Account-scoped review and approval endpoints for outbound Jobs communication.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
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
    approved_at_ms: Option<i64>,
    dispatched_at_ms: Option<i64>,
    created_at_ms: i64,
    updated_at_ms: i64,
}

impl From<JobsCommunicationAction> for CommunicationActionSummary {
    fn from(action: JobsCommunicationAction) -> Self {
        Self {
            id: action.id,
            application_id: action.application_id,
            connection_id: action.connection_id,
            source_message_id: action.source_message_id,
            kind: action.kind,
            provider: action.provider,
            payload_sha256: action.payload_sha256,
            status: action.status,
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
    payload: Value,
}

impl From<JobsCommunicationAction> for CommunicationActionDetail {
    fn from(action: JobsCommunicationAction) -> Self {
        let payload = action.payload.clone();
        Self {
            summary: CommunicationActionSummary::from(action),
            payload,
        }
    }
}

pub(super) async fn communication_actions(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
    Query(query): Query<CommunicationActionsQuery>,
) -> Result<Json<Vec<CommunicationActionSummary>>, ApiError> {
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
    .map(|actions| {
        Json(
            actions
                .into_iter()
                .map(CommunicationActionSummary::from)
                .collect(),
        )
    })
    .map_err(internal)
}

pub(super) async fn communication_action(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
    Path(action_id): Path<String>,
) -> Result<Json<CommunicationActionDetail>, ApiError> {
    jobs::communication_action(&state.pool, &account.id, &action_id)
        .map_err(internal)?
        .map(CommunicationActionDetail::from)
        .map(Json)
        .ok_or_else(not_found)
}

pub(super) async fn create_communication_action(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
    Json(request): Json<CreateCommunicationActionRequest>,
) -> Result<(StatusCode, Json<CommunicationActionDetail>), ApiError> {
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
        status: String::new(),
        provider_object_id: String::new(),
        lease_owner: None,
        lease_expires_at_ms: None,
        next_attempt_at_ms: 0,
        attempt_count: 0,
        approved_at_ms: None,
        dispatched_at_ms: None,
        created_at_ms: 0,
        updated_at_ms: 0,
    };
    let (action, inserted) = jobs::create_communication_action(&state.pool, &account.id, &action)
        .map_err(domain_error)?;
    Ok((
        if inserted {
            StatusCode::CREATED
        } else {
            StatusCode::OK
        },
        Json(CommunicationActionDetail::from(action)),
    ))
}

pub(super) async fn approve_communication_action(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
    Path(action_id): Path<String>,
) -> Result<Json<CommunicationActionDetail>, ApiError> {
    jobs::approve_communication_action(&state.pool, &account.id, &action_id)
        .map_err(domain_error)?
        .map(CommunicationActionDetail::from)
        .map(Json)
        .ok_or_else(not_found)
}

pub(super) async fn cancel_communication_action(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
    Path(action_id): Path<String>,
) -> Result<Json<CommunicationActionDetail>, ApiError> {
    jobs::cancel_communication_action(&state.pool, &account.id, &action_id)
        .map_err(domain_error)?
        .map(CommunicationActionDetail::from)
        .map(Json)
        .ok_or_else(not_found)
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
            status: "awaiting_approval".to_string(),
            provider_object_id: String::new(),
            lease_owner: Some("worker-private".to_string()),
            lease_expires_at_ms: Some(123),
            next_attempt_at_ms: 123,
            attempt_count: 1,
            approved_at_ms: None,
            dispatched_at_ms: None,
            created_at_ms: 100,
            updated_at_ms: 101,
        };

        let encoded = serde_json::to_value(CommunicationActionSummary::from(action)).unwrap();
        assert!(encoded.get("payload").is_none());
        assert!(encoded.get("idempotency_key").is_none());
        assert!(encoded.get("lease_owner").is_none());
        assert!(encoded.get("attempt_count").is_none());
    }
}
