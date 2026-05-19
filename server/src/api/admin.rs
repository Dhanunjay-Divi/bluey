//! Admin endpoints.

use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use serde::Serialize;

use super::AppState;

#[derive(Serialize)]
pub struct Health {
    pub status: &'static str,
    pub version: &'static str,
    pub commit: &'static str,
}

pub async fn health() -> impl IntoResponse {
    (
        StatusCode::OK,
        Json(Health {
            status: "ok",
            version: env!("CARGO_PKG_VERSION"),
            commit: option_env!("BLUEY_GIT_COMMIT").unwrap_or("unknown"),
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
    let conn = match state.pool.get() {
        Ok(c) => c,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": e.to_string()})),
            )
                .into_response();
        }
    };
    let mut stmt = match conn
        .prepare("SELECT id, email, balance_cents FROM accounts ORDER BY created_at DESC LIMIT 100")
    {
        Ok(s) => s,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": e.to_string()})),
            )
                .into_response();
        }
    };
    let rows: Vec<CustomerSummary> = stmt
        .query_map([], |r| {
            Ok(CustomerSummary {
                id: r.get(0)?,
                email: r.get(1)?,
                balance_cents: r.get(2)?,
            })
        })
        .map(|i| i.filter_map(|r| r.ok()).collect())
        .unwrap_or_default();
    (StatusCode::OK, Json(rows)).into_response()
}
