//! Account-scoped mailbox connection and read-only sync endpoints.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Extension, Json,
};
use serde::{Deserialize, Serialize};

use crate::{
    api::AppState,
    auth::AuthedAccount,
    db::jobs::{self, MailboxConnection},
};

use super::jobs::{bad_request, domain_error, internal, ApiError};

pub(super) async fn mailbox_connections(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
) -> Result<Json<Vec<MailboxConnection>>, ApiError> {
    jobs::list_mailbox_connections(&state.pool, &account.id)
        .map(Json)
        .map_err(internal)
}

#[derive(Debug, Deserialize)]
pub(super) struct MailboxMessagesQuery {
    connection_id: Option<String>,
    status: Option<String>,
    limit: Option<usize>,
}

#[derive(Debug, Serialize)]
pub(super) struct MailboxMessageSummary {
    id: String,
    connection_id: String,
    provider: String,
    sender: String,
    subject: String,
    received_at_ms: i64,
    application_id: Option<String>,
    processing_status: String,
    classification: String,
}

impl From<jobs::JobsProviderMessage> for MailboxMessageSummary {
    fn from(message: jobs::JobsProviderMessage) -> Self {
        Self {
            id: message.id,
            connection_id: message.connection_id,
            provider: message.provider,
            sender: message.sender,
            subject: message.subject,
            received_at_ms: message.received_at_ms,
            application_id: message.application_id,
            processing_status: message.processing_status,
            classification: message.classification,
        }
    }
}

pub(super) async fn mailbox_messages(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
    Query(query): Query<MailboxMessagesQuery>,
) -> Result<Json<Vec<MailboxMessageSummary>>, ApiError> {
    let connection_id = query
        .connection_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let status = query
        .status
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if let Some(status) = status {
        if !matches!(
            status,
            "received" | "processed" | "needs_input" | "ignored" | "failed"
        ) {
            return bad_request("Choose a valid mailbox message status.");
        }
    }

    let mut messages = jobs::list_provider_messages(
        &state.pool,
        &account.id,
        connection_id,
        query.limit.unwrap_or(50).clamp(1, 100),
    )
    .map_err(internal)?;
    if let Some(status) = status {
        messages.retain(|message| message.processing_status == status);
    }
    Ok(Json(
        messages
            .into_iter()
            .map(MailboxMessageSummary::from)
            .collect(),
    ))
}

pub(super) async fn mailbox_sync_state(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
    Path(connection_id): Path<String>,
) -> Result<Json<jobs::JobsProviderSyncState>, ApiError> {
    jobs::mailbox_sync_state(&state.pool, &account.id, &connection_id)
        .map_err(internal)?
        .map(Json)
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                "Connected inbox not found.".to_string(),
            )
        })
}

pub(super) async fn sync_mailbox_now(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
    Path(connection_id): Path<String>,
) -> Result<Json<jobs::JobsProviderSyncState>, ApiError> {
    if jobs::mailbox_sync_state(&state.pool, &account.id, &connection_id)
        .map_err(internal)?
        .is_none()
    {
        return Err((
            StatusCode::NOT_FOUND,
            "Connected inbox not found.".to_string(),
        ));
    }

    jobs::schedule_mailbox_sync_now(&state.pool, &account.id, &connection_id)
        .map_err(domain_error)?
        .map(Json)
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                "Connected inbox sync state not found.".to_string(),
            )
        })
}

pub(super) async fn remove_mailbox_connection(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
    Path(connection_id): Path<String>,
) -> Result<StatusCode, ApiError> {
    if !jobs::delete_mailbox_connection(&state.pool, &account.id, &connection_id)
        .map_err(internal)?
    {
        return Err((
            StatusCode::NOT_FOUND,
            "Connected inbox not found.".to_string(),
        ));
    }
    Ok(StatusCode::NO_CONTENT)
}
