//! Usage event ingestion. Daemon emits a usage event after every cue.

use axum::{extract::State, http::StatusCode, Extension, Json};

use super::AppState;
use crate::auth::AuthedAccount;
use crate::db::usage::{self, UsageEvent};

pub async fn ingest(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
    Json(event): Json<UsageEvent>,
) -> StatusCode {
    if let Err(e) = usage::record(&state.pool, &account.id, &event) {
        tracing::warn!(error = %e, account_id = %account.id, "usage event record failed");
        return StatusCode::INTERNAL_SERVER_ERROR;
    }
    StatusCode::ACCEPTED
}
