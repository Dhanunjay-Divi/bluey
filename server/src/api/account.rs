//! Account endpoints — real implementations using auth middleware.

use axum::{extract::State, http::StatusCode, Extension, Json};
use serde::Serialize;

use super::AppState;
use crate::auth::AuthedAccount;

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

pub async fn me(
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
) -> Json<AccountMe> {
    Json(AccountMe {
        id: account.id,
        email: account.email,
        balance_cents: account.balance_cents,
        trial_seconds_remaining: account.trial_seconds_remaining,
        auto_topup_enabled: account.auto_topup_enabled,
        auto_topup_threshold_cents: account.auto_topup_threshold_cents,
        auto_topup_amount_cents: account.auto_topup_amount_cents,
    })
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

pub async fn usage(
    State(_state): State<AppState>,
    Extension(_account): Extension<AuthedAccount>,
) -> Result<Json<UsageWindow>, StatusCode> {
    // Real aggregation lands in Stage 7. Stub returns 501 for now so
    // clients consuming the route get a clear signal.
    Err(StatusCode::NOT_IMPLEMENTED)
}
