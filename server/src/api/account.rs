//! Account endpoints — real implementations.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Extension, Json,
};
use serde::{Deserialize, Serialize};

use super::AppState;
use crate::auth::AuthedAccount;
use crate::config::BillingProvider;
use crate::db::account_data;

const MIN_AUTO_RELOAD_CENTS: i64 = 1500;
const MAX_AUTO_RELOAD_CENTS: i64 = 10_000;
const MIN_AUTO_RELOAD_THRESHOLD_CENTS: i64 = 100;
const MAX_AUTO_RELOAD_THRESHOLD_CENTS: i64 = 5_000;

#[derive(Serialize)]
pub struct AccountMe {
    pub id: String,
    pub email: String,
    pub balance_cents: i64,
    pub trial_seconds_remaining: i64,
    pub auto_topup_enabled: bool,
    pub auto_topup_threshold_cents: i64,
    pub auto_topup_amount_cents: i64,
    pub is_admin: bool,
    pub billing_provider: String,
    pub auto_topup_available: bool,
    pub auto_topup_unavailable_reason: Option<String>,
    pub saved_payment_method_label: Option<String>,
    pub square_application_id: Option<String>,
    pub square_location_id: Option<String>,
    pub square_environment: Option<String>,
    pub billing_restricted: bool,
    pub billing_restriction_reason: Option<String>,
}

#[derive(Serialize)]
pub struct ApiError {
    pub error: String,
}

#[derive(Serialize)]
pub struct AccountDevice {
    pub id: String,
    pub label: String,
    pub kind: String,
    pub created_at: String,
    pub last_used_at: Option<String>,
    pub expires_at: String,
}

#[derive(Serialize)]
pub struct AccountDevicesResponse {
    pub devices: Vec<AccountDevice>,
}

#[derive(Deserialize)]
pub struct BillingSettingsRequest {
    pub auto_topup_enabled: bool,
    pub auto_topup_threshold_cents: Option<i64>,
    pub auto_topup_amount_cents: Option<i64>,
}

pub async fn me(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
) -> Json<AccountMe> {
    Json(account_me_payload(&state, account))
}

pub async fn devices(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
) -> Result<Json<AccountDevicesResponse>, (StatusCode, Json<ApiError>)> {
    account_devices_payload(&state, &account.id).map(Json)
}

pub async fn revoke_device(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
    Path(device_id): Path<String>,
) -> Result<Json<AccountDevicesResponse>, (StatusCode, Json<ApiError>)> {
    let removed =
        crate::db::refresh_tokens::revoke_hash_for_account(&state.pool, &account.id, &device_id)
            .map_err(|e| {
                tracing::warn!(
                    account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
                    error = %e,
                    "failed to revoke linked device"
                );
                internal_error("Could not remove that device.")
            })?;
    if !removed {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ApiError {
                error: "That linked device was not found.".to_string(),
            }),
        ));
    }
    account_devices_payload(&state, &account.id).map(Json)
}

pub async fn revoke_all_devices(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
) -> Result<Json<AccountDevicesResponse>, (StatusCode, Json<ApiError>)> {
    crate::db::refresh_tokens::revoke_all_for_account(&state.pool, &account.id).map_err(|e| {
        tracing::warn!(
            account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
            error = %e,
            "failed to revoke linked devices"
        );
        internal_error("Could not remove linked devices.")
    })?;
    account_devices_payload(&state, &account.id).map(Json)
}

pub async fn update_billing_settings(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
    Json(req): Json<BillingSettingsRequest>,
) -> Result<Json<AccountMe>, (StatusCode, Json<ApiError>)> {
    let amount = req
        .auto_topup_amount_cents
        .unwrap_or(account.auto_topup_amount_cents)
        .clamp(0, MAX_AUTO_RELOAD_CENTS);
    let threshold = req
        .auto_topup_threshold_cents
        .unwrap_or(account.auto_topup_threshold_cents)
        .clamp(0, MAX_AUTO_RELOAD_THRESHOLD_CENTS);

    let settings_changed =
        req.auto_topup_amount_cents.is_some() || req.auto_topup_threshold_cents.is_some();
    if req.auto_topup_enabled || settings_changed {
        if amount < MIN_AUTO_RELOAD_CENTS {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ApiError {
                    error: "Auto Reload amount must be at least $15.".to_string(),
                }),
            ));
        }
        if threshold < MIN_AUTO_RELOAD_THRESHOLD_CENTS {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ApiError {
                    error: "Auto Reload threshold must be at least $1.".to_string(),
                }),
            ));
        }
        if threshold >= amount {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ApiError {
                    error: "Auto Reload amount must be greater than the threshold.".to_string(),
                }),
            ));
        }
    }

    if req.auto_topup_enabled {
        if account.billing_restricted {
            return Err((
                StatusCode::FORBIDDEN,
                Json(ApiError {
                    error: "Billing is paused while this account is under review.".to_string(),
                }),
            ));
        }
        let (_, available, reason, _) = auto_topup_capability(&state, &account);
        if !available {
            return Err((
                StatusCode::CONFLICT,
                Json(ApiError {
                    error: reason.unwrap_or_else(|| {
                        "Save a payment method before enabling Auto Reload.".to_string()
                    }),
                }),
            ));
        }
    }

    let updated = crate::db::accounts::Account::update_auto_topup_settings(
        &state.pool,
        &account.id,
        req.auto_topup_enabled,
        threshold,
        amount,
    )
    .map_err(|e| {
        tracing::warn!(
            account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
            error = %e,
            "failed to update billing settings"
        );
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError {
                error: "Could not update billing settings.".to_string(),
            }),
        )
    })?
    .ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(ApiError {
                error: "Account not found.".to_string(),
            }),
        )
    })?;

    Ok(Json(account_me_payload(&state, updated)))
}

fn account_devices_payload(
    state: &AppState,
    account_id: &str,
) -> Result<AccountDevicesResponse, (StatusCode, Json<ApiError>)> {
    let sessions = crate::db::refresh_tokens::list_active_for_account(&state.pool, account_id)
        .map_err(|e| {
            tracing::warn!(
                account_id_hash = %cue_core::account_id_hash_prefix(account_id),
                error = %e,
                "failed to list linked devices"
            );
            internal_error("Could not load linked devices.")
        })?;
    Ok(AccountDevicesResponse {
        devices: sessions
            .into_iter()
            .map(|session| {
                let (label, kind) = device_label_and_kind(session.device_label.as_deref());
                AccountDevice {
                    id: session.token_hash,
                    label,
                    kind,
                    created_at: session.created_at,
                    last_used_at: session.last_used_at,
                    expires_at: session.expires_at,
                }
            })
            .collect(),
    })
}

fn device_label_and_kind(device_label: Option<&str>) -> (String, String) {
    match device_label {
        Some("device-link") => ("Bluey desktop".to_string(), "Desktop".to_string()),
        Some(label) if !label.trim().is_empty() => (label.trim().to_string(), "Device".to_string()),
        _ => ("Browser session".to_string(), "Browser".to_string()),
    }
}

fn internal_error(message: &str) -> (StatusCode, Json<ApiError>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ApiError {
            error: message.to_string(),
        }),
    )
}

pub(crate) fn account_me_payload(
    state: &AppState,
    account: crate::db::accounts::Account,
) -> AccountMe {
    let (provider, available, reason, payment_label) = auto_topup_capability(state, &account);
    let square_public = square_public_config(state);
    AccountMe {
        id: account.id,
        email: account.email,
        balance_cents: account.balance_cents,
        trial_seconds_remaining: account.trial_seconds_remaining,
        auto_topup_enabled: account.auto_topup_enabled,
        auto_topup_threshold_cents: account.auto_topup_threshold_cents,
        auto_topup_amount_cents: account.auto_topup_amount_cents,
        is_admin: account.is_admin,
        billing_provider: provider,
        auto_topup_available: available,
        auto_topup_unavailable_reason: reason,
        saved_payment_method_label: payment_label,
        square_application_id: square_public.as_ref().map(|cfg| cfg.0.clone()),
        square_location_id: square_public.as_ref().map(|cfg| cfg.1.clone()),
        square_environment: square_public.map(|cfg| cfg.2),
        billing_restricted: account.billing_restricted,
        billing_restriction_reason: account.billing_restriction_reason,
    }
}

fn square_public_config(state: &AppState) -> Option<(String, String, String)> {
    if !matches!(state.config.billing_provider(), BillingProvider::Square) {
        return None;
    }
    let square = state.config.square_config();
    Some((
        square.application_id?,
        square.location_id?,
        match square.environment {
            crate::config::SquareEnvironment::Sandbox => "sandbox".to_string(),
            crate::config::SquareEnvironment::Production => "production".to_string(),
        },
    ))
}

fn auto_topup_capability(
    state: &AppState,
    account: &crate::db::accounts::Account,
) -> (String, bool, Option<String>, Option<String>) {
    match state.config.billing_provider() {
        BillingProvider::Stripe => {
            let label = account
                .stripe_payment_method_id
                .as_ref()
                .map(|_| "Saved Stripe card".to_string());
            if account.billing_restricted {
                return (
                    "stripe".to_string(),
                    false,
                    Some("Billing is paused while this account is under review.".to_string()),
                    label,
                );
            }
            let available = account.stripe_customer_id.is_some()
                && account.stripe_payment_method_id.is_some()
                && state.config.stripe_secret_key.is_some();
            let reason = if available {
                None
            } else {
                Some("Add credits once to save a card before enabling Auto Reload.".to_string())
            };
            ("stripe".to_string(), available, reason, label)
        }
        BillingProvider::Square => {
            let label = match (
                account.square_card_brand.as_deref(),
                account.square_card_last4.as_deref(),
            ) {
                (Some(brand), Some(last4)) => Some(format!("{brand} ending {last4}")),
                _ if account.square_card_id.is_some() => Some("Saved Square card".to_string()),
                _ => None,
            };
            if account.billing_restricted {
                return (
                    "square".to_string(),
                    false,
                    Some("Billing is paused while this account is under review.".to_string()),
                    label,
                );
            }
            let square = state.config.square_config();
            let available = account.square_customer_id.is_some()
                && account.square_card_id.is_some()
                && square.access_token.is_some()
                && square.location_id.is_some();
            let reason = if available {
                None
            } else {
                Some("Save a card for Auto Reload before turning this on.".to_string())
            };
            ("square".to_string(), available, reason, label)
        }
    }
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

const PERIOD_DAYS: i64 = 7;

pub async fn usage(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
) -> Result<Json<UsageWindow>, StatusCode> {
    let usage = account_data::usage_summary(&state.pool, &account.id)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let total_cues = usage.total_cues;
    let total_cents_spent = usage.total_cents_spent;

    let mix: Vec<MixEntry> = usage
        .mix
        .into_iter()
        .map(|row| {
            let percent = if total_cues > 0 {
                100.0 * (row.count as f64) / (total_cues as f64)
            } else {
                0.0
            };
            MixEntry {
                task_type: row.task_type,
                count: row.count,
                cost_cents: row.cost_cents,
                percent,
            }
        })
        .collect();

    // Tier classification + projection.
    let avg_cost_per_cue_cents = if total_cues > 0 {
        (total_cents_spent as f64) / (total_cues as f64)
    } else {
        2.2 // typical-tier default ~2.2 cents/cue
    };
    let cues_per_reload = if avg_cost_per_cue_cents > 0.0 {
        1500.0 / avg_cost_per_cue_cents
    } else {
        f64::INFINITY
    };
    let tier_label = if cues_per_reload >= 1100.0 {
        "Light"
    } else if cues_per_reload >= 550.0 {
        "Typical tech"
    } else {
        "Heavy"
    };

    // Projected days remaining at current burn rate.
    let cues_per_day = (total_cues as f64) / (PERIOD_DAYS as f64);
    let avg_cents_per_day = cues_per_day * avg_cost_per_cue_cents;
    let projected_days_remaining = if avg_cents_per_day > 0.01 {
        (account.balance_cents as f64) / avg_cents_per_day
    } else {
        365.0 // no usage yet → cap at 1 year (credit-validity boundary)
    };

    Ok(Json(UsageWindow {
        period_days: PERIOD_DAYS,
        total_cues,
        total_cents_spent,
        mix,
        tier_label: tier_label.to_string(),
        projected_days_remaining: (projected_days_remaining * 10.0).round() / 10.0,
    }))
}

// ─── Codex Stage 14: GDPR delete + export ───────────────────────────────

pub async fn export_data(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
) -> Result<Json<account_data::ExportBundle>, axum::http::StatusCode> {
    let bundle = account_data::export_bundle(&state.pool, &account.id)
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(axum::http::StatusCode::NOT_FOUND)?;
    Ok(Json(bundle))
}

#[derive(serde::Serialize)]
pub struct DeleteAck {
    pub deleted: bool,
    pub deleted_at: String,
    pub note: &'static str,
}

pub async fn delete_account(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
) -> Result<Json<DeleteAck>, axum::http::StatusCode> {
    // Hard delete. ON DELETE CASCADE on the foreign keys (accounts ->
    // credit_batches, refresh_tokens, usage_events,
    // email_verification_tokens, password_reset_tokens, request_idempotency)
    // takes care of dependent rows.
    let deleted = account_data::hard_delete_account(&state.pool, &account.id)
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;
    if !deleted {
        return Err(axum::http::StatusCode::NOT_FOUND);
    }
    tracing::info!(
        account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
        "account deleted (GDPR hard-delete)"
    );
    Ok(Json(DeleteAck {
        deleted: true,
        deleted_at: chrono::Utc::now().to_rfc3339(),
        note: "All account data has been removed. Re-signup is allowed with the same email.",
    }))
}
