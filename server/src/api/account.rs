//! Account endpoints. Stubs for now.

use axum::{extract::State, http::StatusCode, Json};
use serde::Serialize;

use super::AppState;

#[derive(Serialize)]
pub struct AccountMe {
    pub id: String,
    pub email: String,
    pub balance_cents: i64,
    pub trial_seconds_remaining: i64,
    pub auto_topup_enabled: bool,
    pub auto_topup_threshold_cents: i64,
    pub auto_topup_amount_cents: i64,
}

pub async fn me(State(_state): State<AppState>) -> Result<Json<AccountMe>, StatusCode> {
    Err(StatusCode::NOT_IMPLEMENTED)
}

#[derive(Serialize)]
pub struct UsageWindow {
    pub period_days: i64,
    pub total_cues: i64,
    pub total_cents_spent: i64,
    pub mix: Vec<MixEntry>,
    pub tier_label: String,
    pub projected_days_remaining: f64,
}

#[derive(Serialize)]
pub struct MixEntry {
    pub task_type: String,
    pub count: i64,
    pub cost_cents: i64,
    pub percent: f64,
}

pub async fn usage(State(_state): State<AppState>) -> Result<Json<UsageWindow>, StatusCode> {
    Err(StatusCode::NOT_IMPLEMENTED)
}
