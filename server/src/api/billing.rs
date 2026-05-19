//! Stripe checkout + webhook handling. Stubs for now.

use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};

use super::AppState;

#[derive(Deserialize)]
pub struct CheckoutRequest {
    pub amount_cents: i64,
}

#[derive(Serialize)]
pub struct CheckoutResponse {
    pub checkout_url: String,
}

pub async fn checkout(
    State(_state): State<AppState>,
    Json(_req): Json<CheckoutRequest>,
) -> Result<Json<CheckoutResponse>, StatusCode> {
    Err(StatusCode::NOT_IMPLEMENTED)
}

pub async fn webhook(
    State(_state): State<AppState>,
    body: String,
) -> Result<StatusCode, StatusCode> {
    let _ = body;
    Err(StatusCode::NOT_IMPLEMENTED)
}
