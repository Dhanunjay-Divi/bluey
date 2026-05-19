//! Usage event ingestion. Daemon emits a usage event after every cue.

use axum::{extract::State, http::StatusCode, Json};

use super::AppState;
use crate::db::usage::UsageEvent;

pub async fn ingest(
    State(_state): State<AppState>,
    Json(_event): Json<UsageEvent>,
) -> StatusCode {
    // TODO: extract account_id from auth middleware, persist event.
    StatusCode::NOT_IMPLEMENTED
}
