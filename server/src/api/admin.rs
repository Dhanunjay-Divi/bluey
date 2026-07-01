//! Admin endpoints.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Extension, Json,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path as FsPath, PathBuf};
use std::time::UNIX_EPOCH;

use super::AppState;
use crate::auth::AuthedAccount;
use crate::db::accounts::Account;
use crate::db::{account_data, ops_audit};

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

#[derive(Serialize)]
pub struct BillingRiskSummary {
    pub id: String,
    pub email: String,
    pub balance_cents: i64,
    pub billing_restriction_reason: Option<String>,
    pub billing_restricted_at: Option<String>,
    pub latest_ledger_event_type: Option<String>,
    pub latest_ledger_amount_cents: Option<i64>,
    pub latest_ledger_created_at: Option<String>,
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

pub async fn billing_risk(State(state): State<AppState>) -> impl IntoResponse {
    match Account::list_billing_risk_summaries(&state.pool, 100) {
        Ok(rows) => (
            StatusCode::OK,
            Json(
                rows.into_iter()
                    .map(|row| BillingRiskSummary {
                        id: row.id,
                        email: row.email,
                        balance_cents: row.balance_cents,
                        billing_restriction_reason: row.billing_restriction_reason,
                        billing_restricted_at: row.billing_restricted_at,
                        latest_ledger_event_type: row.latest_ledger_event_type,
                        latest_ledger_amount_cents: row.latest_ledger_amount_cents,
                        latest_ledger_created_at: row.latest_ledger_created_at,
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

#[derive(Serialize)]
pub struct StorageHealth {
    pub db_backend: String,
    pub object_storage_configured: bool,
    pub object_bucket: Option<String>,
    pub object_key_prefix: Option<String>,
    pub backup_dir: String,
    pub latest_backup: Option<BackupSnapshot>,
    pub offsite_destination_configured: bool,
    pub offsite_destination_kind: Option<String>,
    pub export_zip_supported: bool,
    pub retention_delete_objects_supported: bool,
    pub support_bundle_supported: bool,
    pub restore_drill_script: String,
}

#[derive(Serialize)]
pub struct BackupSnapshot {
    pub path: String,
    pub size_bytes: u64,
    pub modified_at_ms: Option<i64>,
    pub age_seconds: Option<i64>,
    pub backend_hint: String,
}

pub async fn storage_health(State(state): State<AppState>) -> impl IntoResponse {
    let backup_dir =
        std::env::var("BLUEY_BACKUP_DIR").unwrap_or_else(|_| "/var/backups/bluey-api".to_string());
    let offsite = std::env::var("OFFSITE_DESTINATION").ok();
    let object_storage = state.config.object_storage.as_ref();
    (
        StatusCode::OK,
        Json(StorageHealth {
            db_backend: format!("{:?}", state.config.db_backend).to_lowercase(),
            object_storage_configured: object_storage.is_some(),
            object_bucket: object_storage.map(|config| config.bucket.clone()),
            object_key_prefix: object_storage.map(|config| config.key_prefix.clone()),
            latest_backup: latest_backup_snapshot(&backup_dir),
            backup_dir,
            offsite_destination_configured: offsite.is_some(),
            offsite_destination_kind: offsite.as_deref().map(destination_kind),
            export_zip_supported: true,
            retention_delete_objects_supported: true,
            support_bundle_supported: true,
            restore_drill_script: "ops/restore-drill-bluey-db.sh".to_string(),
        }),
    )
        .into_response()
}

#[derive(Serialize)]
pub struct SupportAccountBundle {
    pub generated_at: String,
    pub account_id_hash: String,
    pub email_hash: String,
    pub balance_cents: i64,
    pub trial_seconds_remaining: i64,
    pub created_at: Option<String>,
    pub last_login_at: Option<String>,
    pub counts: SupportCounts,
    pub recent_usage_events: Vec<SupportUsageEvent>,
    pub recent_sessions: Vec<SupportSessionSummary>,
    pub artifact_objects: Vec<SupportArtifactObject>,
    pub redaction_note: &'static str,
}

#[derive(Serialize)]
pub struct SupportCounts {
    pub credit_batches: usize,
    pub usage_events: usize,
    pub sessions: usize,
    pub transcript_segments: usize,
    pub cue_responses: usize,
    pub context_artifacts: usize,
    pub rag_chunks: i64,
    pub refresh_tokens: i64,
    pub stripe_webhook_events: i64,
    pub artifact_objects: usize,
}

#[derive(Serialize)]
pub struct SupportUsageEvent {
    pub request_id: Option<String>,
    pub ts: Option<String>,
    pub kind: Option<String>,
    pub task_type: Option<String>,
    pub lane: Option<String>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub latency_ms: Option<i64>,
    pub cost_cents_to_customer: Option<i64>,
}

#[derive(Serialize)]
pub struct SupportSessionSummary {
    pub session_id_hash: String,
    pub status: Option<String>,
    pub created_at_ms: Option<i64>,
    pub updated_at_ms: Option<i64>,
    pub last_active_at_ms: Option<i64>,
}

#[derive(Serialize)]
pub struct SupportArtifactObject {
    pub artifact_id_hash: String,
    pub object_key_hash: String,
    pub content_type: Option<String>,
    pub size_bytes: Option<i64>,
    pub sha256: Option<String>,
    pub expires_at_ms: Option<i64>,
}

pub async fn support_account(
    State(state): State<AppState>,
    Path(account_id): Path<String>,
    Extension(AuthedAccount(actor)): Extension<AuthedAccount>,
) -> impl IntoResponse {
    let bundle = match account_data::export_bundle(&state.pool, &account_id) {
        Ok(Some(bundle)) => bundle,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "account not found"})),
            )
                .into_response()
        }
        Err(error) => {
            tracing::warn!(
                account_id_hash = %cue_core::account_id_hash_prefix(&account_id),
                error = %error,
                "failed to build admin support account bundle"
            );
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "failed to load account support bundle"})),
            )
                .into_response();
        }
    };
    let artifact_objects = match account_data::artifact_object_refs(&state.pool, &account_id) {
        Ok(refs) => refs,
        Err(error) => {
            tracing::warn!(
                account_id_hash = %cue_core::account_id_hash_prefix(&account_id),
                error = %error,
                "failed to list support artifact object refs"
            );
            Vec::new()
        }
    };
    let response = SupportAccountBundle {
        generated_at: chrono::Utc::now().to_rfc3339(),
        account_id_hash: stable_hash(&bundle.account.id),
        email_hash: stable_hash(&bundle.account.email.to_lowercase()),
        balance_cents: bundle.account.balance_cents,
        trial_seconds_remaining: bundle.account.trial_seconds_remaining,
        created_at: bundle.account.created_at.clone(),
        last_login_at: bundle.account.last_login_at.clone(),
        counts: SupportCounts {
            credit_batches: bundle.credit_batches.len(),
            usage_events: bundle.usage_events.len(),
            sessions: bundle.cloud_sessions.len(),
            transcript_segments: bundle.cloud_transcript_segments.len(),
            cue_responses: bundle.cloud_cue_responses.len(),
            context_artifacts: bundle.cloud_context_artifacts.len(),
            rag_chunks: bundle.cloud_rag_chunks_count,
            refresh_tokens: bundle.refresh_tokens_count,
            stripe_webhook_events: bundle.stripe_webhook_events_count,
            artifact_objects: artifact_objects.len(),
        },
        recent_usage_events: bundle
            .usage_events
            .iter()
            .take(25)
            .map(support_usage_event)
            .collect(),
        recent_sessions: bundle
            .cloud_sessions
            .iter()
            .take(25)
            .map(support_session_summary)
            .collect(),
        artifact_objects: artifact_objects
            .into_iter()
            .map(|object_ref| SupportArtifactObject {
                artifact_id_hash: stable_hash(&object_ref.artifact_id),
                object_key_hash: stable_hash(&object_ref.object_key),
                content_type: object_ref.content_type,
                size_bytes: object_ref.size_bytes,
                sha256: object_ref.sha256,
                expires_at_ms: object_ref.expires_at_ms,
            })
            .collect(),
        redaction_note:
            "This admin support bundle excludes transcript text, answer text, document previews, source URIs, raw object keys, and raw email.",
    };
    if let Err(error) = ops_audit::record_event(
        &state.pool,
        ops_audit::OpsAuditEventInput {
            account_id_hash: Some(cue_core::account_id_hash_prefix(&account_id)),
            actor_account_id_hash: Some(cue_core::account_id_hash_prefix(&actor.id)),
            event_type: "admin.support_bundle".to_string(),
            status: "completed".to_string(),
            metadata_json: serde_json::json!({
                "artifact_object_count": response.counts.artifact_objects,
                "usage_event_count": response.counts.usage_events
            }),
        },
    ) {
        tracing::warn!(
            account_id_hash = %cue_core::account_id_hash_prefix(&account_id),
            actor_account_id_hash = %cue_core::account_id_hash_prefix(&actor.id),
            error = %error,
            "failed to record support bundle ops audit event"
        );
    }
    (StatusCode::OK, Json(response)).into_response()
}

#[derive(Debug, Deserialize)]
pub struct OpsEventsQuery {
    #[serde(default)]
    pub limit: Option<i64>,
}

#[derive(Serialize)]
pub struct OpsEventsResponse {
    pub events: Vec<ops_audit::OpsAuditEvent>,
}

pub async fn ops_events(
    State(state): State<AppState>,
    Query(query): Query<OpsEventsQuery>,
) -> impl IntoResponse {
    match ops_audit::recent_events(&state.pool, query.limit.unwrap_or(100)) {
        Ok(events) => (StatusCode::OK, Json(OpsEventsResponse { events })).into_response(),
        Err(error) => {
            tracing::warn!(error = %error, "failed to load ops audit events");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "failed to load ops events"})),
            )
                .into_response()
        }
    }
}

fn latest_backup_snapshot(backup_dir: &str) -> Option<BackupSnapshot> {
    let hourly = FsPath::new(backup_dir).join("hourly");
    let candidates = read_backup_candidates(&hourly)
        .or_else(|| read_backup_candidates(FsPath::new(backup_dir)))?;
    candidates
        .into_iter()
        .filter_map(|path| backup_snapshot_for_path(path).ok())
        .max_by_key(|snapshot| snapshot.modified_at_ms.unwrap_or(0))
}

fn read_backup_candidates(dir: &FsPath) -> Option<Vec<PathBuf>> {
    let entries = std::fs::read_dir(dir).ok()?;
    let paths = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            matches!(
                path.extension().and_then(|ext| ext.to_str()),
                Some("pgdump" | "db")
            )
        })
        .collect::<Vec<_>>();
    if paths.is_empty() {
        None
    } else {
        Some(paths)
    }
}

fn backup_snapshot_for_path(path: PathBuf) -> std::io::Result<BackupSnapshot> {
    let metadata = std::fs::metadata(&path)?;
    let modified_at_ms = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_millis() as i64);
    let age_seconds = modified_at_ms.map(|ms| {
        chrono::Utc::now()
            .timestamp_millis()
            .saturating_sub(ms)
            .saturating_div(1000)
    });
    let backend_hint = match path.extension().and_then(|ext| ext.to_str()) {
        Some("pgdump") => "postgres",
        Some("db") => "sqlite",
        _ => "unknown",
    }
    .to_string();
    Ok(BackupSnapshot {
        path: path.display().to_string(),
        size_bytes: metadata.len(),
        modified_at_ms,
        age_seconds,
        backend_hint,
    })
}

fn destination_kind(value: &str) -> String {
    if let Some((kind, _)) = value.split_once("://") {
        kind.to_string()
    } else if value.contains(':') {
        "ssh".to_string()
    } else {
        "path".to_string()
    }
}

fn support_usage_event(value: &serde_json::Value) -> SupportUsageEvent {
    SupportUsageEvent {
        request_id: string_field(value, "request_id"),
        ts: string_field(value, "ts"),
        kind: string_field(value, "kind"),
        task_type: string_field(value, "task_type"),
        lane: string_field(value, "lane"),
        provider: string_field(value, "provider"),
        model: string_field(value, "model"),
        input_tokens: i64_field(value, "input_tokens"),
        output_tokens: i64_field(value, "output_tokens"),
        latency_ms: i64_field(value, "latency_ms"),
        cost_cents_to_customer: i64_field(value, "cost_cents_to_customer"),
    }
}

fn support_session_summary(value: &serde_json::Value) -> SupportSessionSummary {
    SupportSessionSummary {
        session_id_hash: string_field(value, "session_id")
            .map(|session_id| stable_hash(&session_id))
            .unwrap_or_default(),
        status: string_field(value, "status"),
        created_at_ms: i64_field(value, "created_at_ms"),
        updated_at_ms: i64_field(value, "updated_at_ms"),
        last_active_at_ms: i64_field(value, "last_active_at_ms"),
    }
}

fn string_field(value: &serde_json::Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(serde_json::Value::as_str)
        .map(ToString::to_string)
}

fn i64_field(value: &serde_json::Value, key: &str) -> Option<i64> {
    value.get(key).and_then(serde_json::Value::as_i64)
}

fn stable_hash(value: &str) -> String {
    hex::encode(Sha256::digest(value.as_bytes()))
}
