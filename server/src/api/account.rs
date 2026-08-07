//! Account endpoints — real implementations.

use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{header, HeaderValue, Method, Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Extension, Json,
};
use serde::{Deserialize, Serialize};
use std::io::{Cursor, Write};

use super::{jobs_runner_volumes, AppState};
use crate::auth::AuthedAccount;
use crate::billing::policy::{
    is_internal_or_test_billing_account, INTERNAL_TEST_BILLING_BLOCK_MESSAGE,
};
use crate::config::{BillingProvider, ObjectStorageConfig};
use crate::db::devices::{DeviceRecord, DeviceRegistration};
use crate::db::{account_data, diagnostic_logs, jobs};
use crate::object_storage::ObjectStorage;

const MIN_AUTO_RELOAD_CENTS: i64 = 1500;
const MAX_AUTO_RELOAD_CENTS: i64 = 50_000;
const MIN_AUTO_RELOAD_THRESHOLD_CENTS: i64 = 100;
const MAX_AUTO_RELOAD_THRESHOLD_CENTS: i64 = 5_000;

pub async fn reject_mutation_after_deletion_fence(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
    request: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    let read_only = request.method() == Method::GET
        || request.method() == Method::HEAD
        || request.method() == Method::OPTIONS;
    let path = request.uri().path();
    if read_only || path == "/account/delete" || path == "/auth/logout" {
        return Ok(next.run(request).await);
    }
    let deletion_pending = account_data::account_deletion_intent(&state.pool, &account.id)
        .map_err(|error| {
            tracing::warn!(
                account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
                error = %error,
                "failed to check account-deletion fence before mutation"
            );
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .is_some();
    if deletion_pending {
        return Ok((
            StatusCode::CONFLICT,
            Json(ApiError {
                error: concat!(
                    "Account deletion is pending. Bluey has fenced new writes and launches; ",
                    "retry account deletion to check cleanup."
                )
                .to_string(),
            }),
        )
            .into_response());
    }
    Ok(next.run(request).await)
}

#[derive(Serialize)]
pub struct AccountMe {
    pub id: String,
    pub email: String,
    pub balance_cents: i64,
    pub trial_seconds_remaining: i64,
    pub is_temporary: bool,
    pub temporary_expires_at: Option<String>,
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
    pub device_id: String,
    pub label: String,
    pub kind: String,
    pub platform: String,
    pub arch: Option<String>,
    pub app_version: Option<String>,
    pub created_at: String,
    pub last_used_at: Option<String>,
    pub last_heartbeat_at: Option<String>,
    pub live: bool,
    pub expires_at: String,
}

#[derive(Serialize)]
pub struct AccountDevicesResponse {
    pub devices: Vec<AccountDevice>,
}

#[derive(Deserialize)]
pub struct RegisterDeviceRequest {
    pub device_id: String,
    pub device_name: Option<String>,
    pub platform: Option<String>,
    pub arch: Option<String>,
    pub app_version: Option<String>,
}

#[derive(Deserialize)]
pub struct DeviceStatusRequest {
    pub device_id: String,
}

#[derive(Serialize)]
pub struct DeviceStatusResponse {
    pub active: bool,
}

#[derive(Deserialize)]
pub struct BillingSettingsRequest {
    pub auto_topup_enabled: bool,
    pub auto_topup_threshold_cents: Option<i64>,
    pub auto_topup_amount_cents: Option<i64>,
}

pub async fn me(
    State(state): State<AppState>,
    Extension(AuthedAccount(mut account)): Extension<AuthedAccount>,
) -> Json<AccountMe> {
    match crate::db::stt_accounting::release_stale_sessions(
        &state.pool,
        &account.id,
        chrono::Utc::now().timestamp_millis(),
    ) {
        Ok(released) if released > 0 => {
            tracing::info!(
                account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
                released,
                "released stale STT reservations during account refresh"
            );
            if let Ok(Some(refreshed)) =
                crate::db::accounts::Account::fetch_by_id(&state.pool, &account.id)
            {
                account = refreshed;
            }
        }
        Ok(_) => {}
        Err(error) => {
            tracing::warn!(
                account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
                error = %error,
                "failed to reconcile stale STT reservations during account refresh"
            );
        }
    }
    Json(account_me_payload(&state, account))
}

pub async fn devices(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
) -> Result<Json<AccountDevicesResponse>, (StatusCode, Json<ApiError>)> {
    account_devices_payload(&state, &account.id).map(Json)
}

pub async fn register_device(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
    Json(req): Json<RegisterDeviceRequest>,
) -> Result<Json<AccountDevice>, (StatusCode, Json<ApiError>)> {
    let device_id = req.device_id.trim();
    if device_id.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiError {
                error: "device_id is required.".to_string(),
            }),
        ));
    }
    let registration = DeviceRegistration {
        device_id: device_id.to_string(),
        device_name: req
            .device_name
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("Bluey desktop")
            .to_string(),
        platform: req
            .platform
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("desktop")
            .to_string(),
        arch: non_empty_trimmed(req.arch),
        app_version: non_empty_trimmed(req.app_version),
    };
    let record =
        crate::db::devices::upsert(&state.pool, &account.id, &registration).map_err(|e| {
            tracing::warn!(
                account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
                error = %e,
                "failed to register linked device"
            );
            internal_error("Could not register this device.")
        })?;
    Ok(Json(account_device_from_record(record)))
}

pub async fn device_status(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
    Json(req): Json<DeviceStatusRequest>,
) -> Result<Json<DeviceStatusResponse>, (StatusCode, Json<ApiError>)> {
    let device_id = req.device_id.trim();
    if device_id.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiError {
                error: "device_id is required.".to_string(),
            }),
        ));
    }
    let active = crate::db::devices::is_active_for_account(&state.pool, &account.id, device_id)
        .map_err(|e| {
            tracing::warn!(
                account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
                error = %e,
                "failed to check linked device status"
            );
            internal_error("Could not check this device.")
        })?;
    if !active {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(ApiError {
                error: "This desktop is no longer linked to this account.".to_string(),
            }),
        ));
    }
    Ok(Json(DeviceStatusResponse { active }))
}

pub async fn revoke_device(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
    Path(device_id): Path<String>,
) -> Result<Json<AccountDevicesResponse>, (StatusCode, Json<ApiError>)> {
    let existing = crate::db::devices::list_for_account(&state.pool, &account.id)
        .map_err(|e| {
            tracing::warn!(
                account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
                error = %e,
                "failed to load linked device before revoke"
            );
            internal_error("Could not remove that device.")
        })?
        .into_iter()
        .find(|device| device.id == device_id);
    let removed = crate::db::devices::revoke_for_account(&state.pool, &account.id, &device_id)
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
    if let Some(device) = existing {
        let revoked = crate::db::refresh_tokens::revoke_for_device(
            &state.pool,
            &account.id,
            &device.device_id,
        )
        .map_err(|e| {
            tracing::warn!(
                account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
                device_id_hash = %cue_core::account_id_hash_prefix(&device.device_id),
                error = %e,
                "failed to revoke linked device refresh tokens"
            );
            internal_error("Could not sign out that device.")
        })?;
        tracing::info!(
            account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
            device_id_hash = %cue_core::account_id_hash_prefix(&device.device_id),
            revoked_refresh_tokens = revoked,
            "linked device removed and refresh tokens revoked"
        );
    }
    account_devices_payload(&state, &account.id).map(Json)
}

pub async fn revoke_all_devices(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
) -> Result<Json<AccountDevicesResponse>, (StatusCode, Json<ApiError>)> {
    crate::db::devices::revoke_all_for_account(&state.pool, &account.id).map_err(|e| {
        tracing::warn!(
            account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
            error = %e,
            "failed to revoke linked devices"
        );
        internal_error("Could not remove linked devices.")
    })?;
    crate::db::refresh_tokens::revoke_all_devices_for_account(&state.pool, &account.id).map_err(
        |e| {
            tracing::warn!(
                account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
                error = %e,
                "failed to revoke linked-device refresh tokens"
            );
            internal_error("Could not sign out linked devices.")
        },
    )?;
    account_devices_payload(&state, &account.id).map(Json)
}

pub async fn update_billing_settings(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
    Json(req): Json<BillingSettingsRequest>,
) -> Result<Json<AccountMe>, (StatusCode, Json<ApiError>)> {
    let amount = req
        .auto_topup_amount_cents
        .unwrap_or(account.auto_topup_amount_cents);
    let threshold = req
        .auto_topup_threshold_cents
        .unwrap_or(account.auto_topup_threshold_cents);

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
        if amount > MAX_AUTO_RELOAD_CENTS {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ApiError {
                    error: "Auto Reload amount can be at most $500.".to_string(),
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
        if threshold > MAX_AUTO_RELOAD_THRESHOLD_CENTS {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ApiError {
                    error: "Auto Reload threshold can be at most $50.".to_string(),
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
        if is_internal_or_test_billing_account(&account) {
            return Err((
                StatusCode::FORBIDDEN,
                Json(ApiError {
                    error: INTERNAL_TEST_BILLING_BLOCK_MESSAGE.to_string(),
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
    let records = crate::db::devices::list_for_account(&state.pool, account_id).map_err(|e| {
        tracing::warn!(
            account_id_hash = %cue_core::account_id_hash_prefix(account_id),
            error = %e,
            "failed to list linked devices"
        );
        internal_error("Could not load linked devices.")
    })?;
    Ok(AccountDevicesResponse {
        devices: records
            .into_iter()
            .map(account_device_from_record)
            .collect(),
    })
}

fn account_device_from_record(record: DeviceRecord) -> AccountDevice {
    let last_used_at = record
        .last_heartbeat_at
        .clone()
        .or_else(|| record.last_seen_at.clone());
    AccountDevice {
        id: record.id,
        device_id: record.device_id,
        label: record.device_name,
        kind: device_kind(&record.platform),
        platform: record.platform,
        arch: record.arch,
        app_version: record.app_version,
        created_at: record.registered_at,
        live: is_live_device(record.last_heartbeat_at.as_deref()),
        last_used_at,
        last_heartbeat_at: record.last_heartbeat_at,
        expires_at: String::new(),
    }
}

fn device_kind(platform: &str) -> String {
    match platform.trim().to_lowercase().as_str() {
        "macos" | "darwin" => "macOS".to_string(),
        "windows" | "win32" => "Windows".to_string(),
        "linux" => "Linux".to_string(),
        _ => "Desktop".to_string(),
    }
}

fn is_live_device(last_heartbeat_at: Option<&str>) -> bool {
    let Some(last_heartbeat_at) = last_heartbeat_at else {
        return false;
    };
    let Ok(last) = last_heartbeat_at.parse::<chrono::DateTime<chrono::Utc>>() else {
        return false;
    };
    chrono::Utc::now().signed_duration_since(last).num_seconds() < 90
}

fn non_empty_trimmed(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().chars().take(128).collect::<String>())
        .filter(|value| !value.is_empty())
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
    let internal_or_test_billing = is_internal_or_test_billing_account(&account);
    let auto_topup_enabled = account.auto_topup_enabled
        && !internal_or_test_billing
        && (provider != "stripe" || available);
    AccountMe {
        id: account.id,
        email: account.email,
        balance_cents: account.balance_cents,
        trial_seconds_remaining: account.trial_seconds_remaining,
        is_temporary: account.is_temporary,
        temporary_expires_at: account.temporary_expires_at,
        auto_topup_enabled,
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
                .map(|_| "Saved card".to_string());
            if is_internal_or_test_billing_account(account) {
                return (
                    "stripe".to_string(),
                    false,
                    Some(INTERNAL_TEST_BILLING_BLOCK_MESSAGE.to_string()),
                    label,
                );
            }
            if account.billing_restricted {
                return (
                    "stripe".to_string(),
                    false,
                    Some("Billing is paused while this account is under review.".to_string()),
                    label,
                );
            }
            let has_saved_method =
                account.stripe_customer_id.is_some() && account.stripe_payment_method_id.is_some();
            let configuration_error =
                crate::billing::topup::stripe_auto_reload_configuration_error(&state.config);
            let available = has_saved_method && configuration_error.is_none();
            let reason = if !has_saved_method {
                Some("Add balance once to save a card before enabling Auto Reload.".to_string())
            } else if configuration_error.is_some() {
                Some("Auto Reload is temporarily unavailable.".to_string())
            } else {
                None
            };
            ("stripe".to_string(), available, reason, label)
        }
        BillingProvider::Square => {
            let label = match (
                account.square_card_brand.as_deref(),
                account.square_card_last4.as_deref(),
            ) {
                (Some(brand), Some(last4)) => Some(format!("{brand} ending {last4}")),
                _ if account.square_card_id.is_some() => Some("Saved card".to_string()),
                _ => None,
            };
            if is_internal_or_test_billing_account(account) {
                return (
                    "square".to_string(),
                    false,
                    Some(INTERNAL_TEST_BILLING_BLOCK_MESSAGE.to_string()),
                    label,
                );
            }
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
    pub projection_label: String,
    pub projection_quality: String,
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

    // Projected days remaining at current burn rate. Very small samples make
    // the math look falsely precise, so expose a human label and cap the
    // legacy numeric field for older clients.
    let cues_per_day = (total_cues as f64) / (PERIOD_DAYS as f64);
    let avg_cents_per_day = cues_per_day * avg_cost_per_cue_cents;
    let raw_projected_days_remaining = if avg_cents_per_day > 0.01 {
        (account.balance_cents as f64) / avg_cents_per_day
    } else {
        0.0
    };
    let low_sample = total_cues < 20 || total_cents_spent < 100;
    let (projected_days_remaining, projection_label, projection_quality) =
        if total_cues == 0 || total_cents_spent == 0 {
            (
                0.0,
                "Projection appears after usage.".to_string(),
                "none".to_string(),
            )
        } else if low_sample {
            (
                raw_projected_days_remaining.min(60.0),
                "Light recent usage; estimate needs more activity.".to_string(),
                "low_sample".to_string(),
            )
        } else if raw_projected_days_remaining >= 90.0 {
            (
                90.0,
                "90+ days at recent pace.".to_string(),
                "capped".to_string(),
            )
        } else {
            let rounded = raw_projected_days_remaining.round().max(1.0);
            (
                raw_projected_days_remaining,
                format!("~{rounded:.0} days at recent pace."),
                "estimated".to_string(),
            )
        };

    Ok(Json(UsageWindow {
        period_days: PERIOD_DAYS,
        total_cues,
        total_cents_spent,
        mix,
        tier_label: tier_label.to_string(),
        projected_days_remaining: (projected_days_remaining * 10.0).round() / 10.0,
        projection_label,
        projection_quality,
    }))
}

// ─── Codex Stage 14: GDPR delete + export ───────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ExportQuery {
    #[serde(default)]
    pub format: Option<String>,
    #[serde(default)]
    pub include_objects: Option<bool>,
}

pub async fn export_data(
    State(state): State<AppState>,
    Query(query): Query<ExportQuery>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
) -> Result<Response, axum::http::StatusCode> {
    let bundle = account_data::export_bundle(&state.pool, &account.id)
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(axum::http::StatusCode::NOT_FOUND)?;
    if query
        .format
        .as_deref()
        .is_some_and(|format| format.eq_ignore_ascii_case("zip"))
    {
        return export_zip(
            &state,
            &account.id,
            bundle,
            query.include_objects.unwrap_or(true),
        )
        .await;
    }
    record_account_ops_event(
        &state,
        &account.id,
        "account.export",
        "completed",
        serde_json::json!({"format": "json"}),
    );
    Ok(Json(bundle).into_response())
}

const ACCOUNT_DELETE_RETRY_AFTER_MS: u64 = 5_000;

#[derive(serde::Serialize)]
pub struct DeleteAccountResponse {
    pub deleted: bool,
    pub state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required_target_count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved_target_count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub legacy_unresolved_count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_after_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<String>,
    pub object_count_deleted: usize,
    pub note: String,
}

#[derive(serde::Deserialize)]
pub struct DeleteAccountRequest {
    pub confirm_text: String,
    pub accept_data_loss: bool,
    pub accept_credit_loss: bool,
}

fn same_object_storage_namespace(left: &ObjectStorageConfig, right: &ObjectStorageConfig) -> bool {
    left.endpoint_url.trim_end_matches('/') == right.endpoint_url.trim_end_matches('/')
        && left.bucket == right.bucket
        && left.key_prefix.trim_matches('/') == right.key_prefix.trim_matches('/')
}

pub async fn delete_account(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
    Json(req): Json<DeleteAccountRequest>,
) -> Result<Response, axum::http::StatusCode> {
    if req.confirm_text.trim() != "DELETE" || !req.accept_data_loss || !req.accept_credit_loss {
        return Err(axum::http::StatusCode::BAD_REQUEST);
    }

    let initial_deletion = account_data::begin_account_deletion(
        &state.pool,
        &account.id,
        chrono::Utc::now().timestamp_millis(),
    )
    .map_err(|error| {
        tracing::warn!(
            account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
            error = %error,
            "failed to establish account-deletion write fence"
        );
        axum::http::StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let (intent, uploads_pending) = match initial_deletion {
        Some(account_data::BeginAccountDeletionResult::Ready(intent)) => (intent, false),
        Some(account_data::BeginAccountDeletionResult::WaitingForUploads(intent)) => (intent, true),
        Some(account_data::BeginAccountDeletionResult::WaitingForIrreversibleSubmissions {
            active_submissions,
        }) => {
            tracing::info!(
                account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
                active_submissions,
                "account deletion is waiting for an irreversible submission outcome"
            );
            return Err(axum::http::StatusCode::CONFLICT);
        }
        Some(account_data::BeginAccountDeletionResult::WaitingForIrreversibleCommunications {
            active_actions,
        }) => {
            tracing::info!(
                account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
                active_actions,
                "account deletion is waiting for an irreversible communication outcome"
            );
            return Err(axum::http::StatusCode::CONFLICT);
        }
        None => return Err(axum::http::StatusCode::NOT_FOUND),
    };

    let purge_request_id = account_deletion_purge_request_id(&account.id, intent.requested_at_ms);
    let fleet = jobs::runner_volume_fleet_status(&state.pool).map_err(|error| {
        tracing::warn!(
            account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
            error = %error,
            "failed to load legacy runner inventory authority after deletion fence"
        );
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    if fleet.legacy_inventory_state != "ready" {
        return Ok(pending_account_delete_without_purge_response(
            "pending_runner_legacy_inventory",
            &purge_request_id,
            concat!(
                "Account deletion is securely fenced and pending authorized legacy runner ",
                "inventory reconciliation. Your credentials are retained so you can check ",
                "deletion status."
            ),
        ));
    }
    let (
        Some(legacy_inventory_reconciliation_id),
        Some(legacy_inventory_authority_id),
        Some(legacy_inventory_authority_sha256),
    ) = (
        fleet.legacy_inventory_reconciliation_id.clone(),
        fleet.legacy_inventory_authority_id.clone(),
        fleet.legacy_inventory_authority_sha256.clone(),
    )
    else {
        tracing::error!("ready runner legacy inventory is missing its exact authority binding");
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    };
    let policy = jobs_runner_volumes::runner_volume_purge_policy()?;
    let prepare_input = jobs::PrepareRunnerVolumePurgeRequest {
        request_id: purge_request_id.clone(),
        account_id: account.id.clone(),
        minimum_runner_build_id: policy.minimum_runner_build_id,
        expected_legacy_inventory_generation: fleet.legacy_inventory_generation,
        expected_legacy_inventory_reconciliation_id: legacy_inventory_reconciliation_id,
        expected_legacy_inventory_authority_id: legacy_inventory_authority_id,
        expected_legacy_inventory_authority_sha256: legacy_inventory_authority_sha256,
        now_ms: chrono::Utc::now().timestamp_millis(),
    };
    let prepared = match jobs::prepare_runner_volume_purge(
        &state.pool,
        &policy.signer,
        &policy.key_ring,
        &prepare_input,
    ) {
        Ok(prepared) => prepared,
        Err(
            error @ (jobs::RunnerVolumePurgeError::NotReady
            | jobs::RunnerVolumePurgeError::Conflict),
        ) => {
            let refreshed =
                jobs::runner_volume_fleet_status(&state.pool).map_err(|refresh_error| {
                    tracing::warn!(
                        account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
                        error = %refresh_error,
                        "failed to refresh runner legacy inventory after prepare race"
                    );
                    StatusCode::INTERNAL_SERVER_ERROR
                })?;
            let authority_drifted = refreshed.legacy_inventory_state != "ready"
                || refreshed.legacy_inventory_generation
                    != prepare_input.expected_legacy_inventory_generation
                || refreshed.legacy_inventory_reconciliation_id.as_deref()
                    != Some(
                        prepare_input
                            .expected_legacy_inventory_reconciliation_id
                            .as_str(),
                    )
                || refreshed.legacy_inventory_authority_id.as_deref()
                    != Some(
                        prepare_input
                            .expected_legacy_inventory_authority_id
                            .as_str(),
                    )
                || refreshed.legacy_inventory_authority_sha256.as_deref()
                    != Some(
                        prepare_input
                            .expected_legacy_inventory_authority_sha256
                            .as_str(),
                    );
            if authority_drifted {
                return Ok(pending_account_delete_without_purge_response(
                    "pending_runner_legacy_inventory",
                    &purge_request_id,
                    "Account deletion is securely fenced and pending a stable authorized legacy runner inventory.",
                ));
            }
            tracing::warn!(
                account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
                purge_request_id = %purge_request_id,
                error = %error,
                "runner-volume purge preparation failed without legacy inventory drift"
            );
            return Err(runner_purge_account_delete_error_status(&error));
        }
        Err(error) => {
            tracing::warn!(
                account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
                purge_request_id = %purge_request_id,
                error = %error,
                "failed to prepare durable runner-volume purge"
            );
            return Err(runner_purge_account_delete_error_status(&error));
        }
    };
    if prepared.status.account_id.as_deref() != Some(account.id.as_str()) {
        tracing::error!(
            account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
            purge_request_id = %purge_request_id,
            "runner-volume purge replay did not match the fenced account"
        );
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }
    let active_purge_request_id = prepared.status.request_id.clone();
    let purge_status = complete_runner_purge_if_ready(
        &state,
        prepared.status,
        chrono::Utc::now().timestamp_millis(),
    )?;

    if purge_status.state != "complete" {
        tracing::info!(
            account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
            purge_request_id = %purge_status.request_id,
            required_target_count = purge_status.required_target_count,
            resolved_target_count = purge_status.resolved_target_count,
            legacy_unresolved_count = purge_status.legacy_unresolved_count,
            "account deletion is fenced and waiting for runner-volume purge"
        );
        return Ok(pending_account_delete_response(
            "pending_runner_volume_purge",
            &purge_request_id,
            &purge_status,
            concat!(
                "Account deletion is securely pending managed runner-volume purge ",
                "attestation. Your credentials are retained so you can check deletion status."
            ),
        ));
    }
    if uploads_pending {
        tracing::info!(
            account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
            fresh_in_flight_puts = intent.fresh_in_flight_puts,
            "account deletion is fenced and waiting for active object uploads"
        );
        return Ok(pending_account_delete_response(
            "pending_upload_drain",
            &purge_request_id,
            &purge_status,
            "Account deletion is securely pending active object-upload drain.",
        ));
    }

    let _object_deletion_guard =
        account_data::acquire_account_object_deletion(&state.pool, &account.id)
            .await
            .map_err(|error| {
                tracing::warn!(
                    account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
                    error = %error,
                    "failed to serialize final account deletion with object writers"
                );
                axum::http::StatusCode::INTERNAL_SERVER_ERROR
            })?;

    let final_deletion = account_data::begin_account_deletion(
        &state.pool,
        &account.id,
        chrono::Utc::now().timestamp_millis(),
    )
    .map_err(|error| {
        tracing::warn!(
            account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
            error = %error,
            "failed to revalidate account-deletion fence before object sweep"
        );
        axum::http::StatusCode::INTERNAL_SERVER_ERROR
    })?;
    match final_deletion {
        Some(account_data::BeginAccountDeletionResult::Ready(_)) => {}
        Some(account_data::BeginAccountDeletionResult::WaitingForUploads(intent)) => {
            tracing::info!(
                account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
                fresh_in_flight_puts = intent.fresh_in_flight_puts,
                "account deletion found an active upload during final revalidation"
            );
            return Ok(pending_account_delete_response(
                "pending_upload_drain",
                &purge_request_id,
                &purge_status,
                "Account deletion is securely pending active object-upload drain.",
            ));
        }
        Some(account_data::BeginAccountDeletionResult::WaitingForIrreversibleSubmissions {
            active_submissions,
        }) => {
            tracing::error!(
                account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
                active_submissions,
                "irreversible submission appeared after account deletion was fenced"
            );
            return Err(axum::http::StatusCode::CONFLICT);
        }
        Some(account_data::BeginAccountDeletionResult::WaitingForIrreversibleCommunications {
            active_actions,
        }) => {
            tracing::error!(
                account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
                active_actions,
                "irreversible communication appeared after account deletion was fenced"
            );
            return Err(axum::http::StatusCode::CONFLICT);
        }
        None => return Err(axum::http::StatusCode::NOT_FOUND),
    }
    let final_purge_status =
        jobs::runner_volume_purge_status(&state.pool, &active_purge_request_id).map_err(
            |error| {
                tracing::warn!(
                    account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
                    purge_request_id = %active_purge_request_id,
                    error = %error,
                    "failed to revalidate runner-volume purge before object sweep"
                );
                runner_purge_account_delete_error_status(&error)
            },
        )?;
    if final_purge_status.state != "complete"
        || final_purge_status.account_id.as_deref() != Some(account.id.as_str())
    {
        return Ok(pending_account_delete_response(
            "pending_runner_volume_purge",
            &purge_request_id,
            &final_purge_status,
            "Account deletion is securely pending managed runner-volume purge attestation.",
        ));
    }

    let artifact_storage_config = state.config.object_storage.clone().ok_or_else(|| {
        tracing::warn!(
            account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
            "account deletion is fenced but artifact storage is unavailable"
        );
        axum::http::StatusCode::SERVICE_UNAVAILABLE
    })?;
    let audit_storage_config = state
        .config
        .log_storage
        .clone()
        .unwrap_or_else(|| artifact_storage_config.clone());
    let artifact_storage = ObjectStorage::new(artifact_storage_config.clone());
    let audit_storage = ObjectStorage::new(audit_storage_config.clone());

    let object_refs =
        account_data::artifact_object_refs(&state.pool, &account.id).map_err(|e| {
            tracing::warn!(
                account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
                error = %e,
                "failed to list account artifact objects before delete"
            );
            axum::http::StatusCode::INTERNAL_SERVER_ERROR
        })?;
    let mut object_count_deleted = 0usize;
    if !object_refs.is_empty() {
        for object_ref in &object_refs {
            if !artifact_storage.key_belongs_to_account(&object_ref.object_key, &account.id) {
                tracing::error!(
                    account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
                    artifact_id = %object_ref.artifact_id,
                    "refusing account delete because artifact object key is outside account scope"
                );
                return Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR);
            }
        }
        for object_ref in &object_refs {
            artifact_storage
                .delete(&object_ref.object_key)
                .await
                .map_err(|_| {
                    tracing::warn!(
                        account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
                        artifact_id = %object_ref.artifact_id,
                        "failed to delete account artifact object"
                    );
                    axum::http::StatusCode::SERVICE_UNAVAILABLE
                })?;
            object_count_deleted += 1;
        }
    }

    let diagnostic_object_refs = diagnostic_logs::object_refs_for_account(&state.pool, &account.id)
        .map_err(|e| {
            tracing::warn!(
                account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
                error = %e,
                "failed to list account diagnostic log objects before delete"
            );
            axum::http::StatusCode::INTERNAL_SERVER_ERROR
        })?;
    if !diagnostic_object_refs.is_empty() {
        for object_ref in &diagnostic_object_refs {
            if !audit_storage.key_belongs_to_account(&object_ref.object_key, &account.id) {
                tracing::error!(
                    account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
                    diagnostic_log_id_hash = %cue_core::account_id_hash_prefix(&object_ref.id),
                    "refusing account delete because diagnostic log object key is outside account scope"
                );
                return Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR);
            }
        }
        for object_ref in &diagnostic_object_refs {
            audit_storage
                .delete(&object_ref.object_key)
                .await
                .map_err(|_| {
                    tracing::warn!(
                        account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
                        diagnostic_log_id_hash = %cue_core::account_id_hash_prefix(&object_ref.id),
                        bytes = object_ref.bytes,
                        sha256 = object_ref.sha256.as_deref().unwrap_or(""),
                        "failed to delete account diagnostic log object"
                    );
                    axum::http::StatusCode::SERVICE_UNAVAILABLE
                })?;
            object_count_deleted += 1;
        }
    }

    let mut storage_namespaces = vec![("artifact", artifact_storage_config)];
    if !same_object_storage_namespace(&storage_namespaces[0].1, &audit_storage_config) {
        storage_namespaces.push(("audit", audit_storage_config));
    }
    for (storage_scope, storage_config) in storage_namespaces {
        let storage = ObjectStorage::new(storage_config);
        let orphan_count = storage
            .delete_all_account_objects(&account.id)
            .await
            .map_err(|_| {
                tracing::warn!(
                    account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
                    storage_scope,
                    "failed to purge account object namespace"
                );
                axum::http::StatusCode::SERVICE_UNAVAILABLE
            })?;
        object_count_deleted = object_count_deleted
            .checked_add(orphan_count)
            .ok_or(axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;
    }

    // Hard delete. ON DELETE CASCADE on the foreign keys (accounts ->
    // credit_batches, refresh_tokens, usage_events,
    // email_verification_tokens, password_reset_tokens, request_idempotency)
    // takes care of dependent rows.
    let deleted = match account_data::hard_delete_account_after_runner_purge(
        &state.pool,
        &account.id,
        &active_purge_request_id,
    ) {
        Ok(deleted) => deleted,
        Err(error) => {
            let refreshed_fleet = jobs::runner_volume_fleet_status(&state.pool)
                .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;
            let legacy_inventory_drifted = refreshed_fleet.legacy_inventory_state != "ready"
                || refreshed_fleet.legacy_inventory_generation
                    != final_purge_status.legacy_inventory_generation
                || refreshed_fleet
                    .legacy_inventory_reconciliation_id
                    .as_deref()
                    != Some(
                        final_purge_status
                            .legacy_inventory_reconciliation_id
                            .as_str(),
                    )
                || refreshed_fleet.legacy_inventory_authority_id.as_deref()
                    != Some(final_purge_status.legacy_inventory_authority_id.as_str())
                || refreshed_fleet.legacy_inventory_authority_sha256.as_deref()
                    != Some(
                        final_purge_status
                            .legacy_inventory_authority_sha256
                            .as_str(),
                    );
            if legacy_inventory_drifted {
                tracing::info!(
                    account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
                    purge_request_id = %active_purge_request_id,
                    "account deletion remains fenced after legacy inventory changed before hard delete"
                );
                return Ok(pending_account_delete_response(
                    "pending_runner_legacy_inventory",
                    &purge_request_id,
                    &final_purge_status,
                    "Account deletion is securely pending renewed legacy runner inventory authority.",
                ));
            }
            tracing::warn!(
                account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
                purge_request_id = %active_purge_request_id,
                error = %error,
                "final account hard delete failed its durable runner-purge gate"
            );
            return Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR);
        }
    };
    if !deleted {
        return Err(axum::http::StatusCode::NOT_FOUND);
    }
    tracing::info!(
        account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
        "account deleted (GDPR hard-delete)"
    );
    record_account_ops_event(
        &state,
        &account.id,
        "account.delete",
        "completed",
        serde_json::json!({
            "object_count_deleted": object_count_deleted,
            "credits_lost": true,
            "data_loss_accepted": true
        }),
    );
    Ok(Json(DeleteAccountResponse {
        deleted: true,
        state: "deleted".to_string(),
        request_id: Some(purge_request_id),
        required_target_count: Some(final_purge_status.required_target_count),
        resolved_target_count: Some(final_purge_status.resolved_target_count),
        legacy_unresolved_count: Some(final_purge_status.legacy_unresolved_count),
        retry_after_ms: None,
        deleted_at: Some(chrono::Utc::now().to_rfc3339()),
        object_count_deleted,
        note: "All account data has been removed. Re-signup is allowed with the same email."
            .to_string(),
    })
    .into_response())
}

fn account_deletion_purge_request_id(account_id: &str, requested_at_ms: i64) -> String {
    let material =
        format!("bluey-jobs-runner\0account-deletion-request-v1\0{account_id}\0{requested_at_ms}");
    format!("delete-{}", sha256_text(&material))
}

fn runner_purge_account_delete_error_status(error: &jobs::RunnerVolumePurgeError) -> StatusCode {
    match error {
        jobs::RunnerVolumePurgeError::NotFound => StatusCode::NOT_FOUND,
        jobs::RunnerVolumePurgeError::Storage(_) => StatusCode::INTERNAL_SERVER_ERROR,
        jobs::RunnerVolumePurgeError::InvalidRequest
        | jobs::RunnerVolumePurgeError::Conflict
        | jobs::RunnerVolumePurgeError::Unauthorized
        | jobs::RunnerVolumePurgeError::NotReady => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

fn complete_runner_purge_if_ready(
    state: &AppState,
    status: jobs::RunnerPurgeRequestStatus,
    now_ms: i64,
) -> Result<jobs::RunnerPurgeRequestStatus, StatusCode> {
    if !matches!(status.state.as_str(), "pending" | "complete") {
        tracing::error!(
            purge_request_id = %status.request_id,
            state = %status.state,
            "runner-volume purge has an unsupported deletion state"
        );
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }
    if status.state == "pending"
        && (status.legacy_unresolved_count != 0
            || status.resolved_target_count != status.required_target_count)
    {
        return Ok(status);
    }
    match jobs::complete_runner_volume_purge(&state.pool, &status.request_id, now_ms) {
        Ok(completion) => Ok(completion.status),
        Err(jobs::RunnerVolumePurgeError::NotReady) => {
            jobs::runner_volume_purge_status(&state.pool, &status.request_id).map_err(|error| {
                tracing::warn!(
                    purge_request_id = %status.request_id,
                    error = %error,
                    "failed to refresh runner-volume purge after completion race"
                );
                runner_purge_account_delete_error_status(&error)
            })
        }
        Err(error) => {
            tracing::warn!(
                purge_request_id = %status.request_id,
                error = %error,
                "failed to finalize ready runner-volume purge tombstone"
            );
            Err(runner_purge_account_delete_error_status(&error))
        }
    }
}

fn pending_account_delete_response(
    state: &str,
    deletion_request_id: &str,
    purge: &jobs::RunnerPurgeRequestStatus,
    note: &str,
) -> Response {
    let mut response = (
        StatusCode::ACCEPTED,
        Json(DeleteAccountResponse {
            deleted: false,
            state: state.to_string(),
            request_id: Some(deletion_request_id.to_string()),
            required_target_count: Some(purge.required_target_count),
            resolved_target_count: Some(purge.resolved_target_count),
            legacy_unresolved_count: Some(purge.legacy_unresolved_count),
            retry_after_ms: Some(ACCOUNT_DELETE_RETRY_AFTER_MS),
            deleted_at: None,
            object_count_deleted: 0,
            note: note.to_string(),
        }),
    )
        .into_response();
    response
        .headers_mut()
        .insert(header::RETRY_AFTER, HeaderValue::from_static("5"));
    response
}

fn pending_account_delete_without_purge_response(
    state: &str,
    request_id: &str,
    note: &str,
) -> Response {
    let mut response = (
        StatusCode::ACCEPTED,
        Json(DeleteAccountResponse {
            deleted: false,
            state: state.to_string(),
            request_id: Some(request_id.to_string()),
            required_target_count: None,
            resolved_target_count: None,
            legacy_unresolved_count: None,
            retry_after_ms: Some(ACCOUNT_DELETE_RETRY_AFTER_MS),
            deleted_at: None,
            object_count_deleted: 0,
            note: note.to_string(),
        }),
    )
        .into_response();
    response
        .headers_mut()
        .insert(header::RETRY_AFTER, HeaderValue::from_static("5"));
    response
}

async fn export_zip(
    state: &AppState,
    account_id: &str,
    bundle: account_data::ExportBundle,
    include_objects: bool,
) -> Result<Response, axum::http::StatusCode> {
    let object_refs = account_data::artifact_object_refs(&state.pool, account_id).map_err(|e| {
        tracing::warn!(
            account_id_hash = %cue_core::account_id_hash_prefix(account_id),
            error = %e,
            "failed to list account artifact objects for export"
        );
        axum::http::StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let object_budget_bytes = std::env::var("BLUEY_EXPORT_MAX_OBJECT_BYTES")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(100 * 1024 * 1024);
    let storage = if include_objects {
        match state.config.object_storage.clone() {
            Some(config) => Some(ObjectStorage::new(config)),
            None if object_refs.is_empty() => None,
            None => return Err(axum::http::StatusCode::SERVICE_UNAVAILABLE),
        }
    } else {
        None
    };
    let mut manifest_objects = Vec::new();
    let mut exported_object_bytes = 0u64;

    let cursor = Cursor::new(Vec::new());
    let mut zip = zip::ZipWriter::new(cursor);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    write_zip_json(&mut zip, options, "account-export.json", &bundle)?;
    write_zip_file(
        &mut zip,
        options,
        "README.txt",
        "Bluey account export. account-export.json contains the complete structured export. sessions/ contains readable transcript and answer views. artifacts/manifest.json lists attached artifact objects and whether original bytes were included.\n".as_bytes(),
    )?;
    write_zip_file(
        &mut zip,
        options,
        "sessions/transcript.md",
        render_transcripts_markdown(&bundle).as_bytes(),
    )?;
    write_zip_file(
        &mut zip,
        options,
        "sessions/answers.md",
        render_answers_markdown(&bundle).as_bytes(),
    )?;

    for object_ref in &object_refs {
        let mut object_manifest = serde_json::json!({
            "artifact_id": object_ref.artifact_id,
            "title": object_ref.title,
            "object_key_sha256": sha256_text(&object_ref.object_key),
            "content_type": object_ref.content_type,
            "size_bytes": object_ref.size_bytes,
            "sha256": object_ref.sha256,
            "expires_at_ms": object_ref.expires_at_ms,
            "included": false,
            "skipped_reason": null,
        });
        let Some(storage) = storage.as_ref() else {
            object_manifest["skipped_reason"] = serde_json::json!("object storage not configured");
            manifest_objects.push(object_manifest);
            continue;
        };
        if !storage.key_belongs_to_account(&object_ref.object_key, account_id) {
            tracing::error!(
                account_id_hash = %cue_core::account_id_hash_prefix(account_id),
                artifact_id = %object_ref.artifact_id,
                "refusing account export because artifact object key is outside account scope"
            );
            return Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR);
        }
        let expected_size = object_ref
            .size_bytes
            .map(|value| {
                u64::try_from(value).map_err(|_| {
                    tracing::error!(
                        account_id_hash = %cue_core::account_id_hash_prefix(account_id),
                        artifact_id = %object_ref.artifact_id,
                        "refusing account export because artifact size authority is invalid"
                    );
                    axum::http::StatusCode::INTERNAL_SERVER_ERROR
                })
            })
            .transpose()?;
        if expected_size.is_some_and(|expected_size| {
            exported_object_bytes.saturating_add(expected_size) > object_budget_bytes
        }) {
            return Err(axum::http::StatusCode::PAYLOAD_TOO_LARGE);
        }
        let expected_content_type = object_ref
            .content_type
            .as_deref()
            .map(|value| {
                let media_type = base_media_type(value);
                if media_type.is_empty() {
                    return Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR);
                }
                Ok(media_type)
            })
            .transpose()
            .inspect_err(|_| {
                tracing::error!(
                    account_id_hash = %cue_core::account_id_hash_prefix(account_id),
                    artifact_id = %object_ref.artifact_id,
                    "refusing account export because artifact media-type authority is invalid"
                );
            })?;
        match storage.get(&object_ref.object_key).await {
            Ok(stored) => {
                let bytes_len = stored.bytes.len() as u64;
                let expected_sha256 = object_ref.sha256.as_deref();
                let integrity_matches = expected_size.is_none_or(|expected| bytes_len == expected)
                    && expected_content_type.is_none_or(|expected| {
                        base_media_type(&stored.content_type).eq_ignore_ascii_case(expected)
                    })
                    && expected_sha256.is_none_or(|expected| {
                        valid_sha256(expected)
                            && sha256_bytes(&stored.bytes).eq_ignore_ascii_case(expected)
                    });
                if !integrity_matches {
                    tracing::error!(
                        account_id_hash = %cue_core::account_id_hash_prefix(account_id),
                        artifact_id = %object_ref.artifact_id,
                        "refusing account export because artifact read-back failed integrity verification"
                    );
                    return Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR);
                }
                if exported_object_bytes.saturating_add(bytes_len) > object_budget_bytes {
                    return Err(axum::http::StatusCode::PAYLOAD_TOO_LARGE);
                }
                exported_object_bytes = exported_object_bytes.saturating_add(bytes_len);
                let name = format!(
                    "artifacts/files/{}-{}",
                    safe_zip_name(&object_ref.artifact_id),
                    safe_zip_name(&object_ref.title)
                );
                write_zip_file(&mut zip, options, &name, &stored.bytes)?;
                object_manifest["included"] = serde_json::json!(true);
                object_manifest["zip_path"] = serde_json::json!(name);
                object_manifest["downloaded_content_type"] = serde_json::json!(stored.content_type);
            }
            Err(_) => {
                tracing::warn!(
                    account_id_hash = %cue_core::account_id_hash_prefix(account_id),
                    artifact_id = %object_ref.artifact_id,
                    "failed to include account artifact object in export"
                );
                return Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR);
            }
        }
        manifest_objects.push(object_manifest);
    }

    let manifest = serde_json::json!({
        "exported_at": chrono::Utc::now().to_rfc3339(),
        "format": "bluey-account-export-v1",
        "object_budget_bytes": object_budget_bytes,
        "exported_object_bytes": exported_object_bytes,
        "include_objects_requested": include_objects,
        "objects": manifest_objects,
        "counts": {
            "credit_batches": bundle.credit_batches.len(),
            "usage_events": bundle.usage_events.len(),
            "sessions": bundle.cloud_sessions.len(),
            "transcript_segments": bundle.cloud_transcript_segments.len(),
            "cue_responses": bundle.cloud_cue_responses.len(),
            "context_artifacts": bundle.cloud_context_artifacts.len(),
            "rag_chunks": bundle.cloud_rag_chunks_count,
            "refresh_tokens": bundle.refresh_tokens_count,
            "stripe_webhook_events": bundle.stripe_webhook_events_count
        }
    });
    write_zip_json(&mut zip, options, "manifest.json", &manifest)?;

    let bytes = zip
        .finish()
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?
        .into_inner();
    let mut response = Body::from(bytes).into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/zip"),
    );
    response.headers_mut().insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_static("attachment; filename=\"bluey-account-export.zip\""),
    );
    record_account_ops_event(
        state,
        account_id,
        "account.export",
        "completed",
        serde_json::json!({
            "format": "zip",
            "include_objects": include_objects,
            "object_count": object_refs.len(),
            "exported_object_bytes": exported_object_bytes
        }),
    );
    Ok(response)
}

fn record_account_ops_event(
    state: &AppState,
    account_id: &str,
    event_type: &str,
    status: &str,
    metadata_json: serde_json::Value,
) {
    if let Err(error) = crate::db::ops_audit::record_event(
        &state.pool,
        crate::db::ops_audit::OpsAuditEventInput {
            account_id_hash: Some(cue_core::account_id_hash_prefix(account_id)),
            actor_account_id_hash: Some(cue_core::account_id_hash_prefix(account_id)),
            event_type: event_type.to_string(),
            status: status.to_string(),
            metadata_json,
        },
    ) {
        tracing::warn!(
            account_id_hash = %cue_core::account_id_hash_prefix(account_id),
            event_type,
            error = %error,
            "failed to record account ops audit event"
        );
    }
}

fn write_zip_json<T: Serialize>(
    zip: &mut zip::ZipWriter<Cursor<Vec<u8>>>,
    options: zip::write::SimpleFileOptions,
    path: &str,
    value: &T,
) -> Result<(), axum::http::StatusCode> {
    let bytes = serde_json::to_vec_pretty(value)
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;
    write_zip_file(zip, options, path, &bytes)
}

fn write_zip_file(
    zip: &mut zip::ZipWriter<Cursor<Vec<u8>>>,
    options: zip::write::SimpleFileOptions,
    path: &str,
    bytes: &[u8],
) -> Result<(), axum::http::StatusCode> {
    zip.start_file(path, options)
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;
    zip.write_all(bytes)
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)
}

fn render_transcripts_markdown(bundle: &account_data::ExportBundle) -> String {
    let mut out = String::from("# Bluey Transcripts\n\n");
    for segment in &bundle.cloud_transcript_segments {
        let session_id = json_str(segment, "session_id").unwrap_or("unknown");
        let speaker = json_str(segment, "speaker").unwrap_or("speaker");
        let text = json_str(segment, "text").unwrap_or("");
        out.push_str(&format!(
            "## Session {session_id}\n\n**{speaker}:** {text}\n\n"
        ));
    }
    out
}

fn render_answers_markdown(bundle: &account_data::ExportBundle) -> String {
    let mut out = String::from("# Bluey Answers\n\n");
    for answer in &bundle.cloud_cue_responses {
        let session_id = json_str(answer, "session_id").unwrap_or("unknown");
        let kind = json_str(answer, "kind").unwrap_or("answer");
        let model = json_str(answer, "model").unwrap_or("unknown model");
        let text = json_str(answer, "text").unwrap_or("");
        out.push_str(&format!(
            "## Session {session_id} - {kind}\n\n_Model: {model}_\n\n{text}\n\n"
        ));
    }
    out
}

fn json_str<'a>(value: &'a serde_json::Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(serde_json::Value::as_str)
}

fn safe_zip_name(value: &str) -> String {
    let mut out = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    out.truncate(96);
    if out.trim_matches('_').is_empty() {
        "artifact".to_string()
    } else {
        out
    }
}

fn sha256_text(value: &str) -> String {
    sha256_bytes(value.as_bytes())
}

fn sha256_bytes(value: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(value))
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn base_media_type(value: &str) -> &str {
    value.split(';').next().unwrap_or_default().trim()
}
