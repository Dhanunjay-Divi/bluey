//! Account endpoints — real implementations.

use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Extension, Json,
};
use serde::{Deserialize, Serialize};
use std::io::{Cursor, Write};

use super::AppState;
use crate::auth::AuthedAccount;
use crate::billing::policy::{
    is_internal_or_test_billing_account, INTERNAL_TEST_BILLING_BLOCK_MESSAGE,
};
use crate::config::BillingProvider;
use crate::db::account_data;
use crate::object_storage::ObjectStorage;

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
    let internal_or_test_billing = is_internal_or_test_billing_account(&account);
    AccountMe {
        id: account.id,
        email: account.email,
        balance_cents: account.balance_cents,
        trial_seconds_remaining: account.trial_seconds_remaining,
        auto_topup_enabled: account.auto_topup_enabled && !internal_or_test_billing,
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

#[derive(serde::Serialize)]
pub struct DeleteAck {
    pub deleted: bool,
    pub deleted_at: String,
    pub object_count_deleted: usize,
    pub note: &'static str,
}

#[derive(serde::Deserialize)]
pub struct DeleteAccountRequest {
    pub confirm_text: String,
    pub accept_data_loss: bool,
    pub accept_credit_loss: bool,
}

pub async fn delete_account(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
    Json(req): Json<DeleteAccountRequest>,
) -> Result<Json<DeleteAck>, axum::http::StatusCode> {
    if req.confirm_text.trim() != "DELETE" || !req.accept_data_loss || !req.accept_credit_loss {
        return Err(axum::http::StatusCode::BAD_REQUEST);
    }

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
        let storage_config = state
            .config
            .object_storage
            .clone()
            .ok_or(axum::http::StatusCode::SERVICE_UNAVAILABLE)?;
        let storage = ObjectStorage::new(storage_config);
        for object_ref in &object_refs {
            if !storage.key_belongs_to_account(&object_ref.object_key, &account.id) {
                tracing::error!(
                    account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
                    artifact_id = %object_ref.artifact_id,
                    "refusing account delete because artifact object key is outside account scope"
                );
                return Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR);
            }
        }
        for object_ref in &object_refs {
            storage.delete(&object_ref.object_key).await.map_err(|e| {
                tracing::warn!(
                    account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
                    artifact_id = %object_ref.artifact_id,
                    error = %e,
                    "failed to delete account artifact object"
                );
                axum::http::StatusCode::INTERNAL_SERVER_ERROR
            })?;
            object_count_deleted += 1;
        }
    }

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
    Ok(Json(DeleteAck {
        deleted: true,
        deleted_at: chrono::Utc::now().to_rfc3339(),
        object_count_deleted,
        note: "All account data has been removed. Re-signup is allowed with the same email.",
    }))
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
        let expected_size = object_ref.size_bytes.unwrap_or(0).max(0) as u64;
        if expected_size > 0
            && exported_object_bytes.saturating_add(expected_size) > object_budget_bytes
        {
            return Err(axum::http::StatusCode::PAYLOAD_TOO_LARGE);
        }
        match storage.get(&object_ref.object_key).await {
            Ok(stored) => {
                let bytes_len = stored.bytes.len() as u64;
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
            Err(error) => {
                tracing::warn!(
                    account_id_hash = %cue_core::account_id_hash_prefix(account_id),
                    artifact_id = %object_ref.artifact_id,
                    error = %error,
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
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(value.as_bytes()))
}
