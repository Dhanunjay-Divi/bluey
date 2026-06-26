//! Admin endpoints.

use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use serde::Serialize;

use super::AppState;
use crate::db::accounts::Account;

#[derive(Serialize)]
pub struct Health {
    pub status: &'static str,
    pub version: &'static str,
    pub commit: &'static str,
    pub platform: String,
    pub server_time_ms: i64,
}

pub async fn health() -> impl IntoResponse {
    (
        StatusCode::OK,
        Json(Health {
            status: "ok",
            version: env!("CARGO_PKG_VERSION"),
            commit: option_env!("BLUEY_GIT_COMMIT").unwrap_or("unknown"),
            platform: format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH),
            server_time_ms: chrono::Utc::now().timestamp_millis(),
        }),
    )
}

#[derive(Serialize)]
pub struct CustomerSummary {
    pub id: String,
    pub email: String,
    pub balance_cents: i64,
}

pub async fn customers(State(state): State<AppState>) -> impl IntoResponse {
    match Account::list_customer_summaries(&state.pool, 100) {
        Ok(rows) => (
            StatusCode::OK,
            Json(
                rows.into_iter()
                    .map(|row| CustomerSummary {
                        id: row.id,
                        email: row.email,
                        balance_cents: row.balance_cents,
                    })
                    .collect::<Vec<_>>(),
            ),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

/// Codex S12-17 blocker 2 production-path proof: returns the
/// rate-limiter-computed peer key for the current request. Admin-only.
pub async fn echo_peer_key(req: axum::extract::Request) -> impl IntoResponse {
    let key = crate::rate_limit::client_key_for_test(&req);
    (StatusCode::OK, Json(serde_json::json!({"peer_key": key})))
}

/// Admin-only trial abuse summary. Returns hashed signals only.
pub async fn trial_abuse(State(state): State<AppState>) -> impl IntoResponse {
    match crate::db::trial_abuse::admin_summary(&state.pool, 100) {
        Ok(summary) => (StatusCode::OK, Json(summary)).into_response(),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": error.to_string()})),
        )
            .into_response(),
    }
}
