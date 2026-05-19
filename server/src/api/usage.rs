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
    match usage::record(&state.pool, &account.id, &event) {
        Ok(true) => StatusCode::ACCEPTED,
        // Codex Stage 7 S7.1: replay -> 200 OK. The caller knows the
        // request was already accepted; this is the standard idempotent
        // semantic. Returning 202 again would be confusing because the
        // event is no longer "newly accepted".
        Ok(false) => StatusCode::OK,
        Err(e) => {
            tracing::warn!(error = %e, account_id = %account.id, "usage event record failed");
            StatusCode::INTERNAL_SERVER_ERROR
        }
    }
}
