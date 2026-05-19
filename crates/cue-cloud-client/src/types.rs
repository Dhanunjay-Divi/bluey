//! Wire types matching `bluey-server`'s API.
//!
//! These shapes mirror `server/src/api/*::*Request/*Response` in the
//! server crate. Kept in this client crate (instead of a shared types
//! crate) for now to avoid a cross-workspace dependency; if drift
//! becomes a problem we'll factor out `bluey-api-types`.

use serde::{Deserialize, Serialize};

// ─── Auth ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AuthResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: i64,
    pub account: AuthAccountSummary,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AuthAccountSummary {
    pub id: String,
    pub email: String,
    pub balance_cents: i64,
    pub trial_seconds_remaining: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DeviceStartResponse {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: i64,
    pub interval: i64,
}

// ─── Account ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct AccountMe {
    pub id: String,
    pub email: String,
    pub balance_cents: i64,
    pub trial_seconds_remaining: i64,
    pub auto_topup_enabled: bool,
    pub auto_topup_threshold_cents: i64,
    pub auto_topup_amount_cents: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UsageWindow {
    pub period_days: i64,
    pub total_cues: i64,
    pub total_cents_spent: i64,
    pub mix: Vec<MixEntry>,
    pub tier_label: String,
    pub projected_days_remaining: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MixEntry {
    pub task_type: String,
    pub count: i64,
    pub cost_cents: i64,
    pub percent: f64,
}

// ─── Router (managed Auto Router endpoint) ──────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct CompleteRequest {
    /// Idempotency key. REQUIRED. Codex Stage 4 S4.1: a retry after a
    /// network timeout would otherwise be charged twice. Caller mints
    /// a fresh UUID per logical request and reuses it on retry.
    pub request_id: String,
    pub system: String,
    pub user: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    pub lane: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub estimated_input_tokens: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CompleteResponse {
    pub text: String,
    pub provider: String,
    pub model: String,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cost_cents: i64,
    pub balance_cents_after: i64,
    pub trial_seconds_remaining: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct EmbedRequest {
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct EmbedResponse {
    pub embedding: Vec<f32>,
    pub provider: String,
    pub model: String,
    pub cost_cents: i64,
    pub balance_cents_after: i64,
}

// ─── Usage ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct UsageEvent {
    pub request_id: String,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lane: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub latency_ms: i64,
    pub cost_cents_to_bluey: i64,
    pub cost_cents_to_customer: i64,
    pub was_speculative: bool,
    pub was_fallback: bool,
}

// ─── 402 reason payload ─────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct InsufficientBalanceBody {
    pub balance_cents: i64,
    #[serde(default)]
    pub estimated_cost_cents: Option<i64>,
    #[serde(default)]
    pub needed_cents: Option<i64>,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub reload_url: Option<String>,
}
