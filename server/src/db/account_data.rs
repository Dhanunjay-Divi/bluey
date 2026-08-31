//! Account dashboard/export/delete read models.

use anyhow::{Context, Result};
use postgres::Row as PgRow;
use rusqlite::{params, OptionalExtension};
use sha2::{Digest, Sha256};
use std::collections::HashSet;

use crate::db::{jobs, DbPool};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccountDeletionCompletion {
    Deleted,
    Pending,
    NotFound,
}

pub const ACCOUNT_DELETION_RECEIPT_TTL_MS: i64 = 30 * 24 * 60 * 60 * 1_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccountDeletionReceiptState {
    Prepared,
    Pending,
    Deleted,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountDeletionReceipt {
    pub state: AccountDeletionReceiptState,
    pub completed_at_ms: Option<i64>,
    pub expires_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrepareAccountDeletionReceipt {
    Ready(AccountDeletionReceipt),
    Conflict,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LookupAccountDeletionReceipt {
    Found(AccountDeletionReceipt),
    Expired,
    NotFound,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UsageSummary {
    pub total_cues: i64,
    pub total_cents_spent: i64,
    pub mix: Vec<UsageMixRow>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsageMixRow {
    pub task_type: String,
    pub count: i64,
    pub cost_cents: i64,
}

#[derive(Debug, serde::Serialize)]
pub struct ExportBundle {
    pub account: ExportAccount,
    pub credit_batches: Vec<serde_json::Value>,
    pub usage_events: Vec<serde_json::Value>,
    pub cloud_sessions: Vec<serde_json::Value>,
    pub cloud_transcript_segments: Vec<serde_json::Value>,
    pub cloud_cue_responses: Vec<serde_json::Value>,
    pub cloud_context_artifacts: Vec<serde_json::Value>,
    pub cloud_rag_chunks_count: i64,
    pub refresh_tokens_count: i64,
    pub stripe_webhook_events_count: i64,
    pub support_diagnostic_consent_receipts:
        Vec<crate::db::support_diagnostics::SupportDiagnosticConsentReceipt>,
    pub jobs: Option<jobs::JobsAccountExport>,
    pub exported_at: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ArtifactObjectRef {
    pub artifact_id: String,
    pub title: String,
    pub object_key: String,
    pub content_type: Option<String>,
    pub size_bytes: Option<i64>,
    pub sha256: Option<String>,
    pub expires_at_ms: Option<i64>,
}

#[derive(Debug, serde::Serialize)]
pub struct ExportAccount {
    pub id: String,
    pub email: String,
    pub balance_cents: i64,
    pub trial_seconds_remaining: i64,
    pub created_at: Option<String>,
    pub last_login_at: Option<String>,
    pub stripe_customer_id: Option<String>,
    pub stripe_payment_method_id: Option<String>,
    pub square_customer_id: Option<String>,
    pub square_card_id: Option<String>,
    pub square_card_brand: Option<String>,
    pub square_card_last4: Option<String>,
}

pub fn usage_summary(pool: &DbPool, account_id: &str) -> Result<UsageSummary> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => usage_summary_sqlite(pool, account_id),
        DbPool::Postgres(_) => usage_summary_postgres(pool, account_id),
    })
}

pub fn export_bundle(pool: &DbPool, account_id: &str) -> Result<Option<ExportBundle>> {
    let mut bundle = crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => export_bundle_sqlite(pool, account_id),
        DbPool::Postgres(_) => export_bundle_postgres(pool, account_id),
    })?;
    if let Some(value) = &mut bundle {
        value.support_diagnostic_consent_receipts =
            crate::db::support_diagnostics::consent_history(pool, account_id)?;
        value.jobs = jobs::account_export(pool, account_id, &value.account.email)?;
    }
    Ok(bundle)
}

pub fn hard_delete_account(pool: &DbPool, account_id: &str) -> Result<bool> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => hard_delete_account_sqlite(pool, account_id),
        DbPool::Postgres(_) => hard_delete_account_postgres(pool, account_id),
    })
}

pub fn account_deletion_is_pending(pool: &DbPool, account_id: &str) -> Result<bool> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get()?;
            let pending = conn
                .query_row(
                    "SELECT deletion_pending_at_ms FROM accounts WHERE id = ?1",
                    params![account_id],
                    |row| row.get::<_, Option<i64>>(0),
                )
                .optional()?
                .flatten()
                .is_some();
            Ok(pending)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let pending = conn
                .query_opt(
                    "SELECT deletion_pending_at_ms FROM accounts WHERE id = $1",
                    &[&account_id],
                )?
                .and_then(|row| row.get::<_, Option<i64>>(0))
                .is_some();
            Ok(pending)
        }
    })
}

/// Publish the account deletion fence before callers enumerate any external
/// objects. New bearer requests and object reservations fail closed from this
/// commit onward. Existing object PUT leases remain visible until completion
/// or lease expiry, preventing a late PUT from being orphaned by hard delete.
pub fn begin_account_deletion(pool: &DbPool, account_id: &str, now_ms: i64) -> Result<bool> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => begin_account_deletion_sqlite(pool, account_id, now_ms),
        DbPool::Postgres(_) => begin_account_deletion_postgres(pool, account_id, now_ms),
    })
}

pub fn complete_account_deletion(
    pool: &DbPool,
    account_id: &str,
) -> Result<AccountDeletionCompletion> {
    let completed_at_ms = chrono::Utc::now().timestamp_millis();
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => complete_account_deletion_sqlite(pool, account_id, completed_at_ms),
        DbPool::Postgres(_) => {
            complete_account_deletion_postgres(pool, account_id, completed_at_ms)
        }
    })
}

pub fn prepare_account_deletion_receipt(
    pool: &DbPool,
    account_id: &str,
    operation_id: &str,
    recovery_token: &str,
    now_ms: i64,
) -> Result<PrepareAccountDeletionReceipt> {
    let account_binding = account_deletion_hash("account", account_id);
    let capability_hash = account_deletion_hash("capability", recovery_token);
    let expires_at_ms = now_ms
        .checked_add(ACCOUNT_DELETION_RECEIPT_TTL_MS)
        .context("account deletion receipt expiry overflow")?;
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => prepare_account_deletion_receipt_sqlite(
            pool,
            operation_id,
            &account_binding,
            &capability_hash,
            now_ms,
            expires_at_ms,
        ),
        DbPool::Postgres(_) => prepare_account_deletion_receipt_postgres(
            pool,
            operation_id,
            &account_binding,
            &capability_hash,
            now_ms,
            expires_at_ms,
        ),
    })
}

pub fn mark_account_deletion_receipt_pending(
    pool: &DbPool,
    account_id: &str,
    operation_id: &str,
    recovery_token: &str,
    now_ms: i64,
) -> Result<Option<AccountDeletionReceipt>> {
    let account_binding = account_deletion_hash("account", account_id);
    let capability_hash = account_deletion_hash("capability", recovery_token);
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => mark_account_deletion_receipt_pending_sqlite(
            pool,
            operation_id,
            &account_binding,
            &capability_hash,
            now_ms,
        ),
        DbPool::Postgres(_) => mark_account_deletion_receipt_pending_postgres(
            pool,
            operation_id,
            &account_binding,
            &capability_hash,
            now_ms,
        ),
    })
}

pub fn lookup_account_deletion_receipt(
    pool: &DbPool,
    operation_id: &str,
    recovery_token: &str,
    now_ms: i64,
) -> Result<LookupAccountDeletionReceipt> {
    let capability_hash = account_deletion_hash("capability", recovery_token);
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            lookup_account_deletion_receipt_sqlite(pool, operation_id, &capability_hash, now_ms)
        }
        DbPool::Postgres(_) => {
            lookup_account_deletion_receipt_postgres(pool, operation_id, &capability_hash, now_ms)
        }
    })
}

pub fn mark_account_deletion_receipt_deleted(
    pool: &DbPool,
    account_id: &str,
    operation_id: &str,
    recovery_token: &str,
    completed_at_ms: i64,
) -> Result<bool> {
    let account_binding = account_deletion_hash("account", account_id);
    let capability_hash = account_deletion_hash("capability", recovery_token);
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get()?;
            Ok(conn.execute(
                "UPDATE account_deletion_receipts
                    SET state = 'deleted', updated_at_ms = ?4, completed_at_ms = ?4
                  WHERE operation_id = ?1 AND account_binding = ?2 AND capability_hash = ?3",
                params![
                    operation_id,
                    account_binding,
                    capability_hash,
                    completed_at_ms
                ],
            )? > 0)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            Ok(conn.execute(
                "UPDATE account_deletion_receipts
                    SET state = 'deleted', updated_at_ms = $4, completed_at_ms = $4
                  WHERE operation_id = $1 AND account_binding = $2 AND capability_hash = $3",
                &[
                    &operation_id,
                    &account_binding,
                    &capability_hash,
                    &completed_at_ms,
                ],
            )? > 0)
        }
    })
}

fn account_deletion_hash(domain: &str, value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"bluey-account-deletion-receipt-v1\0");
    hasher.update(domain.as_bytes());
    hasher.update(b"\0");
    hasher.update(value.as_bytes());
    hex::encode(hasher.finalize())
}

fn receipt_state(value: &str) -> Result<AccountDeletionReceiptState> {
    match value {
        "prepared" => Ok(AccountDeletionReceiptState::Prepared),
        "pending" => Ok(AccountDeletionReceiptState::Pending),
        "deleted" => Ok(AccountDeletionReceiptState::Deleted),
        _ => anyhow::bail!("invalid account deletion receipt state"),
    }
}

fn receipt_from_parts(
    state: String,
    completed_at_ms: Option<i64>,
    expires_at_ms: i64,
) -> Result<AccountDeletionReceipt> {
    Ok(AccountDeletionReceipt {
        state: receipt_state(&state)?,
        completed_at_ms,
        expires_at_ms,
    })
}

pub fn artifact_object_refs(pool: &DbPool, account_id: &str) -> Result<Vec<ArtifactObjectRef>> {
    let mut refs = crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => artifact_object_refs_sqlite(pool, account_id),
        DbPool::Postgres(_) => artifact_object_refs_postgres(pool, account_id),
    })?;
    let mut seen = refs
        .iter()
        .map(|reference| reference.object_key.clone())
        .collect::<HashSet<_>>();
    for evidence in jobs::list_application_evidence(pool, account_id, None)? {
        let key = evidence.storage_key.trim();
        if key.is_empty() || !seen.insert(key.to_string()) {
            continue;
        }
        refs.push(ArtifactObjectRef {
            artifact_id: evidence.id,
            title: if evidence.file_name.trim().is_empty() {
                evidence.label
            } else {
                evidence.file_name
            },
            object_key: key.to_string(),
            content_type: (!evidence.media_type.trim().is_empty()).then_some(evidence.media_type),
            size_bytes: evidence
                .metadata
                .get("size_bytes")
                .and_then(serde_json::Value::as_i64),
            sha256: (!evidence.sha256.trim().is_empty()).then_some(evidence.sha256),
            expires_at_ms: evidence
                .metadata
                .get("expires_at_ms")
                .and_then(serde_json::Value::as_i64),
        });
    }
    for application in jobs::list_applications(pool, account_id)? {
        let receipt = &application.receipt;
        for (index, document) in receipt
            .get("documents")
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
            .enumerate()
        {
            let key = document
                .get("storageKey")
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .unwrap_or_default();
            if key.is_empty() || !seen.insert(key.to_string()) {
                continue;
            }
            let kind = document
                .get("kind")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("document");
            refs.push(ArtifactObjectRef {
                artifact_id: format!("{}:document:{index}", application.id),
                title: document
                    .get("fileName")
                    .and_then(serde_json::Value::as_str)
                    .filter(|value| !value.trim().is_empty())
                    .unwrap_or(kind)
                    .to_string(),
                object_key: key.to_string(),
                content_type: document
                    .get("mediaType")
                    .and_then(serde_json::Value::as_str)
                    .map(ToString::to_string),
                size_bytes: document
                    .get("sizeBytes")
                    .and_then(serde_json::Value::as_i64),
                sha256: document
                    .get("sha256")
                    .and_then(serde_json::Value::as_str)
                    .map(ToString::to_string),
                expires_at_ms: None,
            });
        }
        for (index, screenshot) in receipt
            .get("screenshotKeys")
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
            .enumerate()
        {
            let key = screenshot.as_str().map(str::trim).unwrap_or_default();
            if key.is_empty() || !seen.insert(key.to_string()) {
                continue;
            }
            refs.push(ArtifactObjectRef {
                artifact_id: format!("{}:screenshot:{index}", application.id),
                title: "Application confirmation screenshot".to_string(),
                object_key: key.to_string(),
                content_type: Some("image/png".to_string()),
                size_bytes: None,
                sha256: None,
                expires_at_ms: None,
            });
        }
    }
    if let Some(asset) = jobs::get_resume_source_asset(pool, account_id)? {
        let key = asset.storage_key.trim();
        if !key.is_empty() && seen.insert(key.to_string()) {
            refs.push(ArtifactObjectRef {
                artifact_id: asset.id,
                title: asset.file_name,
                object_key: key.to_string(),
                content_type: Some(asset.media_type),
                size_bytes: Some(asset.size_bytes),
                sha256: Some(asset.sha256),
                expires_at_ms: None,
            });
        }
    }
    Ok(refs)
}

fn usage_summary_sqlite(pool: &DbPool, account_id: &str) -> Result<UsageSummary> {
    let conn = pool.get()?;
    let (total_cues, total_cents_spent): (i64, i64) = conn.query_row(
        "SELECT COUNT(*),
                CAST(MIN(MAX(TOTAL(MIN(MAX(cost_cents_to_customer, 0), 100000000)), 0),
                         9223372036854775807) AS INTEGER)
         FROM usage_events
         WHERE origin = 'server' AND account_id = ?1
           AND ts >= datetime('now', '-7 days')",
        params![account_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;

    let mut stmt = conn.prepare(
        "SELECT COALESCE(task_type, lane, 'general') AS bucket,
                COUNT(*) AS cnt,
                CAST(MIN(MAX(TOTAL(MIN(MAX(cost_cents_to_customer, 0), 100000000)), 0),
                         9223372036854775807) AS INTEGER) AS cost
         FROM usage_events
         WHERE origin = 'server' AND account_id = ?1
           AND ts >= datetime('now', '-7 days')
         GROUP BY bucket
         ORDER BY cost DESC",
    )?;
    let rows = stmt.query_map(params![account_id], |row| {
        Ok(UsageMixRow {
            task_type: row.get(0)?,
            count: row.get(1)?,
            cost_cents: row.get(2)?,
        })
    })?;
    let mix = rows.collect::<rusqlite::Result<Vec<_>>>()?;

    Ok(UsageSummary {
        total_cues,
        total_cents_spent,
        mix,
    })
}

fn usage_summary_postgres(pool: &DbPool, account_id: &str) -> Result<UsageSummary> {
    let mut conn = pool.get_pg()?;
    let row = conn.query_one(
        "SELECT COUNT(*)::bigint,
                LEAST(COALESCE(SUM(LEAST(GREATEST(cost_cents_to_customer, 0), 100000000)::numeric), 0),
                      9223372036854775807)::bigint
         FROM usage_events
         WHERE origin = 'server' AND account_id = $1
           AND ts >= now() - interval '7 days'",
        &[&account_id],
    )?;
    let total_cues: i64 = row.try_get(0)?;
    let total_cents_spent: i64 = row.try_get(1)?;
    let rows = conn.query(
        "SELECT COALESCE(task_type, lane, 'general') AS bucket,
                COUNT(*)::bigint AS cnt,
                LEAST(COALESCE(SUM(LEAST(GREATEST(cost_cents_to_customer, 0), 100000000)::numeric), 0),
                      9223372036854775807)::bigint AS cost
         FROM usage_events
         WHERE origin = 'server' AND account_id = $1
           AND ts >= now() - interval '7 days'
         GROUP BY bucket
         ORDER BY cost DESC",
        &[&account_id],
    )?;
    let mut mix = Vec::with_capacity(rows.len());
    for row in rows {
        mix.push(UsageMixRow {
            task_type: row.try_get(0)?,
            count: row.try_get(1)?,
            cost_cents: row.try_get(2)?,
        });
    }
    Ok(UsageSummary {
        total_cues,
        total_cents_spent,
        mix,
    })
}

fn export_bundle_sqlite(pool: &DbPool, account_id: &str) -> Result<Option<ExportBundle>> {
    let conn = pool.get()?;

    let account = conn
        .query_row(
            "SELECT id, email, balance_cents, trial_seconds_remaining,
                    created_at, last_login_at, stripe_customer_id,
                    stripe_payment_method_id, square_customer_id, square_card_id,
                    square_card_brand, square_card_last4
             FROM accounts WHERE id = ?1",
            params![account_id],
            |row| {
                Ok(ExportAccount {
                    id: row.get(0)?,
                    email: row.get(1)?,
                    balance_cents: row.get(2)?,
                    trial_seconds_remaining: row.get(3)?,
                    created_at: row.get(4)?,
                    last_login_at: row.get(5)?,
                    stripe_customer_id: row.get(6)?,
                    stripe_payment_method_id: row.get(7)?,
                    square_customer_id: row.get(8)?,
                    square_card_id: row.get(9)?,
                    square_card_brand: row.get(10)?,
                    square_card_last4: row.get(11)?,
                })
            },
        )
        .optional()?;
    let Some(account) = account else {
        return Ok(None);
    };

    let credit_batches = export_rows(
        &conn,
        "SELECT id, amount_cents, remaining_cents, purchased_at,
                expires_at, stripe_charge_id, expired_at
         FROM credit_batches WHERE account_id = ?1 ORDER BY purchased_at",
        account_id,
        &[
            "id",
            "amount_cents",
            "remaining_cents",
            "purchased_at",
            "expires_at",
            "stripe_charge_id",
            "expired_at",
        ],
    )?;

    let usage_events = export_rows(
        &conn,
        "SELECT request_id, ts, kind, task_type, lane, provider, model,
                input_tokens, output_tokens, latency_ms,
                cost_cents_to_customer
         FROM usage_events WHERE account_id = ?1 ORDER BY ts DESC LIMIT 10000",
        account_id,
        &[
            "request_id",
            "ts",
            "kind",
            "task_type",
            "lane",
            "provider",
            "model",
            "input_tokens",
            "output_tokens",
            "latency_ms",
            "cost_cents_to_customer",
        ],
    )?;

    let cloud_sessions = export_rows(
        &conn,
        "SELECT session_id, title, status, created_at_ms, updated_at_ms,
                last_active_at_ms, answer_style, metadata_json
         FROM cloud_sessions WHERE account_id = ?1 ORDER BY updated_at_ms DESC LIMIT 10000",
        account_id,
        &[
            "session_id",
            "title",
            "status",
            "created_at_ms",
            "updated_at_ms",
            "last_active_at_ms",
            "answer_style",
            "metadata_json",
        ],
    )?;
    let cloud_transcript_segments = export_rows(
        &conn,
        "SELECT segment_id, session_id, speaker, source, text, start_ms,
                end_ms, ts_ms, is_final, metadata_json
         FROM cloud_transcript_segments WHERE account_id = ?1 ORDER BY ts_ms ASC LIMIT 50000",
        account_id,
        &[
            "segment_id",
            "session_id",
            "speaker",
            "source",
            "text",
            "start_ms",
            "end_ms",
            "ts_ms",
            "is_final",
            "metadata_json",
        ],
    )?;
    let cloud_cue_responses = export_rows(
        &conn,
        "SELECT response_id, session_id, kind, text, source_text, ts_ms,
                provider, model, lane, task_type, cost_cents, balance_cents_after,
                cost_label, artifact_type, artifact_body, artifact_confidence,
                metadata_json
         FROM cloud_cue_responses WHERE account_id = ?1 ORDER BY ts_ms ASC LIMIT 50000",
        account_id,
        &[
            "response_id",
            "session_id",
            "kind",
            "text",
            "source_text",
            "ts_ms",
            "provider",
            "model",
            "lane",
            "task_type",
            "cost_cents",
            "balance_cents_after",
            "cost_label",
            "artifact_type",
            "artifact_body",
            "artifact_confidence",
            "metadata_json",
        ],
    )?;
    let cloud_context_artifacts = export_rows(
        &conn,
        "SELECT artifact_id, session_id, kind, title, note, source_uri,
                content_hash, text_preview, created_at_ms, metadata_json
         FROM cloud_context_artifacts WHERE account_id = ?1 ORDER BY created_at_ms ASC LIMIT 50000",
        account_id,
        &[
            "artifact_id",
            "session_id",
            "kind",
            "title",
            "note",
            "source_uri",
            "content_hash",
            "text_preview",
            "created_at_ms",
            "metadata_json",
        ],
    )?;
    let cloud_rag_chunks_count = conn
        .query_row(
            "SELECT COUNT(*) FROM cloud_rag_chunks WHERE account_id = ?1",
            params![account_id],
            |row| row.get(0),
        )
        .unwrap_or(0);
    let refresh_tokens_count = conn
        .query_row(
            "SELECT COUNT(*) FROM refresh_tokens WHERE account_id = ?1",
            params![account_id],
            |row| row.get(0),
        )
        .unwrap_or(0);
    let stripe_webhook_events_count = conn
        .query_row(
            "SELECT COUNT(*) FROM stripe_webhook_events
             WHERE json_extract(body, '$.data.object.client_reference_id') = ?1",
            params![account_id],
            |row| row.get(0),
        )
        .unwrap_or(0);

    Ok(Some(ExportBundle {
        account,
        credit_batches,
        usage_events,
        cloud_sessions,
        cloud_transcript_segments,
        cloud_cue_responses,
        cloud_context_artifacts,
        cloud_rag_chunks_count,
        refresh_tokens_count,
        stripe_webhook_events_count,
        support_diagnostic_consent_receipts: Vec::new(),
        jobs: None,
        exported_at: chrono::Utc::now().to_rfc3339(),
    }))
}

fn export_bundle_postgres(pool: &DbPool, account_id: &str) -> Result<Option<ExportBundle>> {
    let mut conn = pool.get_pg()?;
    let account = conn
        .query_opt(
            "SELECT id, email, balance_cents, trial_seconds_remaining,
                    created_at::text, last_login_at::text, stripe_customer_id,
                    stripe_payment_method_id, square_customer_id, square_card_id,
                    square_card_brand, square_card_last4
             FROM accounts WHERE id = $1",
            &[&account_id],
        )?
        .map(export_account_from_pg)
        .transpose()?;
    let Some(account) = account else {
        return Ok(None);
    };

    let credit_batches = export_rows_pg(
        &mut conn,
        "SELECT id, amount_cents, remaining_cents, purchased_at::text AS purchased_at,
                expires_at::text AS expires_at, stripe_charge_id, expired_at::text AS expired_at
         FROM credit_batches WHERE account_id = $1 ORDER BY purchased_at",
        account_id,
    )?;
    let usage_events = export_rows_pg(
        &mut conn,
        "SELECT request_id, ts::text AS ts, kind, task_type, lane, provider, model,
                input_tokens, output_tokens, latency_ms,
                cost_cents_to_customer
         FROM usage_events WHERE account_id = $1 ORDER BY ts DESC LIMIT 10000",
        account_id,
    )?;
    let cloud_sessions = export_rows_pg(
        &mut conn,
        "SELECT session_id, title, status, created_at_ms, updated_at_ms,
                last_active_at_ms, answer_style, metadata_json
         FROM cloud_sessions WHERE account_id = $1 ORDER BY updated_at_ms DESC LIMIT 10000",
        account_id,
    )?;
    let cloud_transcript_segments = export_rows_pg(
        &mut conn,
        "SELECT segment_id, session_id, speaker, source, text, start_ms,
                end_ms, ts_ms, is_final, metadata_json
         FROM cloud_transcript_segments WHERE account_id = $1 ORDER BY ts_ms ASC LIMIT 50000",
        account_id,
    )?;
    let cloud_cue_responses = export_rows_pg(
        &mut conn,
        "SELECT response_id, session_id, kind, text, source_text, ts_ms,
                provider, model, lane, task_type, cost_cents, balance_cents_after,
                cost_label, artifact_type, artifact_body, artifact_confidence,
                metadata_json
         FROM cloud_cue_responses WHERE account_id = $1 ORDER BY ts_ms ASC LIMIT 50000",
        account_id,
    )?;
    let cloud_context_artifacts = export_rows_pg(
        &mut conn,
        "SELECT artifact_id, session_id, kind, title, note, source_uri,
                content_hash, text_preview, created_at_ms, metadata_json
         FROM cloud_context_artifacts WHERE account_id = $1 ORDER BY created_at_ms ASC LIMIT 50000",
        account_id,
    )?;
    let cloud_rag_chunks_count: i64 = conn
        .query_one(
            "SELECT COUNT(*)::bigint FROM cloud_rag_chunks WHERE account_id = $1",
            &[&account_id],
        )?
        .try_get(0)?;
    let refresh_tokens_count: i64 = conn
        .query_one(
            "SELECT COUNT(*)::bigint FROM refresh_tokens WHERE account_id = $1",
            &[&account_id],
        )?
        .try_get(0)?;
    let stripe_webhook_events_count: i64 = conn
        .query_one(
            "SELECT COUNT(*)::bigint FROM stripe_webhook_events
             WHERE body::jsonb #>> '{data,object,client_reference_id}' = $1
                OR body::jsonb #>> '{data,object,metadata,bluey_account_id}' = $1",
            &[&account_id],
        )
        .map(|row| row.try_get(0).unwrap_or(0))
        .unwrap_or(0);

    Ok(Some(ExportBundle {
        account,
        credit_batches,
        usage_events,
        cloud_sessions,
        cloud_transcript_segments,
        cloud_cue_responses,
        cloud_context_artifacts,
        cloud_rag_chunks_count,
        refresh_tokens_count,
        stripe_webhook_events_count,
        support_diagnostic_consent_receipts: Vec::new(),
        jobs: None,
        exported_at: chrono::Utc::now().to_rfc3339(),
    }))
}

fn hard_delete_account_sqlite(pool: &DbPool, account_id: &str) -> Result<bool> {
    let mut conn = pool.get()?;
    let tx = conn.transaction()?;
    tx.execute(
        "DELETE FROM stripe_webhook_events
            WHERE json_extract(body, '$.data.object.client_reference_id') = ?1
               OR json_extract(body, '$.data.object.metadata.bluey_account_id') = ?1",
        params![account_id],
    )?;
    let deleted = tx.execute("DELETE FROM accounts WHERE id = ?1", params![account_id])?;
    tx.commit()?;
    Ok(deleted > 0)
}

fn hard_delete_account_postgres(pool: &DbPool, account_id: &str) -> Result<bool> {
    let mut conn = pool.get_pg()?;
    let mut tx = conn.transaction()?;
    tx.execute(
        "DELETE FROM stripe_webhook_events
            WHERE body::jsonb #>> '{data,object,client_reference_id}' = $1
               OR body::jsonb #>> '{data,object,metadata,bluey_account_id}' = $1",
        &[&account_id],
    )
    .context("delete account Stripe webhook events")?;
    let deleted = tx.execute("DELETE FROM accounts WHERE id = $1", &[&account_id])?;
    tx.commit()?;
    Ok(deleted > 0)
}

fn begin_account_deletion_sqlite(pool: &DbPool, account_id: &str, now_ms: i64) -> Result<bool> {
    let mut conn = pool.get()?;
    let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let exists = tx
        .query_row(
            "SELECT 1 FROM accounts WHERE id = ?1",
            params![account_id],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    if !exists {
        return Ok(false);
    }
    tx.execute(
        "UPDATE accounts
            SET deletion_pending_at_ms = COALESCE(deletion_pending_at_ms, ?2)
          WHERE id = ?1",
        params![account_id, now_ms],
    )?;
    tx.execute(
        "UPDATE refresh_tokens
            SET revoked_at = COALESCE(revoked_at, datetime('now'))
          WHERE account_id = ?1",
        params![account_id],
    )?;
    crate::db::object_uploads::fence_account_deletion_sqlite_tx(&tx, account_id, now_ms)?;
    tx.commit()?;
    Ok(true)
}

fn begin_account_deletion_postgres(pool: &DbPool, account_id: &str, now_ms: i64) -> Result<bool> {
    let mut conn = pool.get_pg()?;
    let mut tx = conn.transaction()?;
    let exists = tx
        .query_opt(
            "SELECT id FROM accounts WHERE id = $1 FOR UPDATE",
            &[&account_id],
        )?
        .is_some();
    if !exists {
        return Ok(false);
    }
    tx.execute(
        "UPDATE accounts
            SET deletion_pending_at_ms = COALESCE(deletion_pending_at_ms, $2)
          WHERE id = $1",
        &[&account_id, &now_ms],
    )?;
    tx.execute(
        "UPDATE refresh_tokens
            SET revoked_at = COALESCE(revoked_at, now())
          WHERE account_id = $1",
        &[&account_id],
    )?;
    crate::db::object_uploads::fence_account_deletion_postgres_tx(&mut tx, account_id, now_ms)?;
    tx.commit()?;
    Ok(true)
}

fn prepare_account_deletion_receipt_sqlite(
    pool: &DbPool,
    operation_id: &str,
    account_binding: &str,
    capability_hash: &str,
    now_ms: i64,
    expires_at_ms: i64,
) -> Result<PrepareAccountDeletionReceipt> {
    let mut conn = pool.get()?;
    let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    tx.execute(
        "DELETE FROM account_deletion_receipts WHERE expires_at_ms <= ?1",
        params![now_ms],
    )?;
    let existing = tx
        .query_row(
            "SELECT operation_id, account_binding, capability_hash, state,
                    completed_at_ms, expires_at_ms
               FROM account_deletion_receipts
              WHERE operation_id = ?1",
            params![operation_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<i64>>(4)?,
                    row.get::<_, i64>(5)?,
                ))
            },
        )
        .optional()?;
    if let Some((stored_operation, stored_account, stored_capability, state, completed, expiry)) =
        existing
    {
        tx.commit()?;
        if stored_operation != operation_id
            || stored_account != account_binding
            || stored_capability != capability_hash
        {
            return Ok(PrepareAccountDeletionReceipt::Conflict);
        }
        return Ok(PrepareAccountDeletionReceipt::Ready(receipt_from_parts(
            state, completed, expiry,
        )?));
    }
    tx.execute(
        "INSERT INTO account_deletion_receipts(
             operation_id, account_binding, capability_hash, state,
             created_at_ms, updated_at_ms, completed_at_ms, expires_at_ms
         ) VALUES (?1, ?2, ?3, 'prepared', ?4, ?4, NULL, ?5)",
        params![
            operation_id,
            account_binding,
            capability_hash,
            now_ms,
            expires_at_ms
        ],
    )?;
    tx.commit()?;
    Ok(PrepareAccountDeletionReceipt::Ready(
        AccountDeletionReceipt {
            state: AccountDeletionReceiptState::Prepared,
            completed_at_ms: None,
            expires_at_ms,
        },
    ))
}

fn prepare_account_deletion_receipt_postgres(
    pool: &DbPool,
    operation_id: &str,
    account_binding: &str,
    capability_hash: &str,
    now_ms: i64,
    expires_at_ms: i64,
) -> Result<PrepareAccountDeletionReceipt> {
    let mut conn = pool.get_pg()?;
    let mut tx = conn.transaction()?;
    tx.execute(
        "DELETE FROM account_deletion_receipts WHERE expires_at_ms <= $1",
        &[&now_ms],
    )?;
    tx.execute(
        "INSERT INTO account_deletion_receipts(
             operation_id, account_binding, capability_hash, state,
             created_at_ms, updated_at_ms, completed_at_ms, expires_at_ms
         ) VALUES ($1, $2, $3, 'prepared', $4, $4, NULL, $5)
         ON CONFLICT DO NOTHING",
        &[
            &operation_id,
            &account_binding,
            &capability_hash,
            &now_ms,
            &expires_at_ms,
        ],
    )?;
    let row = tx.query_one(
        "SELECT operation_id, account_binding, capability_hash, state,
                completed_at_ms, expires_at_ms
           FROM account_deletion_receipts
          WHERE operation_id = $1
          FOR UPDATE",
        &[&operation_id],
    )?;
    let stored_operation = row.get::<_, String>(0);
    let stored_account = row.get::<_, String>(1);
    let stored_capability = row.get::<_, String>(2);
    let state = row.get::<_, String>(3);
    let completed = row.get::<_, Option<i64>>(4);
    let expiry = row.get::<_, i64>(5);
    tx.commit()?;
    if stored_operation != operation_id
        || stored_account != account_binding
        || stored_capability != capability_hash
    {
        return Ok(PrepareAccountDeletionReceipt::Conflict);
    }
    Ok(PrepareAccountDeletionReceipt::Ready(receipt_from_parts(
        state, completed, expiry,
    )?))
}

fn mark_account_deletion_receipt_pending_sqlite(
    pool: &DbPool,
    operation_id: &str,
    account_binding: &str,
    capability_hash: &str,
    now_ms: i64,
) -> Result<Option<AccountDeletionReceipt>> {
    let conn = pool.get()?;
    conn.execute(
        "UPDATE account_deletion_receipts
            SET state = CASE WHEN state = 'prepared' THEN 'pending' ELSE state END,
                updated_at_ms = ?4
          WHERE operation_id = ?1 AND account_binding = ?2 AND capability_hash = ?3
            AND expires_at_ms > ?4",
        params![operation_id, account_binding, capability_hash, now_ms],
    )?;
    conn.query_row(
        "SELECT state, completed_at_ms, expires_at_ms
           FROM account_deletion_receipts
          WHERE operation_id = ?1 AND account_binding = ?2 AND capability_hash = ?3
            AND expires_at_ms > ?4",
        params![operation_id, account_binding, capability_hash, now_ms],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<i64>>(1)?,
                row.get::<_, i64>(2)?,
            ))
        },
    )
    .optional()?
    .map(|(state, completed, expiry)| receipt_from_parts(state, completed, expiry))
    .transpose()
}

fn mark_account_deletion_receipt_pending_postgres(
    pool: &DbPool,
    operation_id: &str,
    account_binding: &str,
    capability_hash: &str,
    now_ms: i64,
) -> Result<Option<AccountDeletionReceipt>> {
    let mut conn = pool.get_pg()?;
    let row = conn.query_opt(
        "UPDATE account_deletion_receipts
            SET state = CASE WHEN state = 'prepared' THEN 'pending' ELSE state END,
                updated_at_ms = $4
          WHERE operation_id = $1 AND account_binding = $2 AND capability_hash = $3
            AND expires_at_ms > $4
      RETURNING state, completed_at_ms, expires_at_ms",
        &[&operation_id, &account_binding, &capability_hash, &now_ms],
    )?;
    row.map(|row| {
        receipt_from_parts(
            row.get::<_, String>(0),
            row.get::<_, Option<i64>>(1),
            row.get::<_, i64>(2),
        )
    })
    .transpose()
}

fn lookup_account_deletion_receipt_sqlite(
    pool: &DbPool,
    operation_id: &str,
    capability_hash: &str,
    now_ms: i64,
) -> Result<LookupAccountDeletionReceipt> {
    let conn = pool.get()?;
    let row = conn
        .query_row(
            "SELECT state, completed_at_ms, expires_at_ms
               FROM account_deletion_receipts
              WHERE operation_id = ?1 AND capability_hash = ?2",
            params![operation_id, capability_hash],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<i64>>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            },
        )
        .optional()?;
    lookup_receipt_from_row(row, now_ms)
}

fn lookup_account_deletion_receipt_postgres(
    pool: &DbPool,
    operation_id: &str,
    capability_hash: &str,
    now_ms: i64,
) -> Result<LookupAccountDeletionReceipt> {
    let mut conn = pool.get_pg()?;
    let row = conn.query_opt(
        "SELECT state, completed_at_ms, expires_at_ms
           FROM account_deletion_receipts
          WHERE operation_id = $1 AND capability_hash = $2",
        &[&operation_id, &capability_hash],
    )?;
    lookup_receipt_from_row(
        row.map(|row| {
            (
                row.get::<_, String>(0),
                row.get::<_, Option<i64>>(1),
                row.get::<_, i64>(2),
            )
        }),
        now_ms,
    )
}

fn lookup_receipt_from_row(
    row: Option<(String, Option<i64>, i64)>,
    now_ms: i64,
) -> Result<LookupAccountDeletionReceipt> {
    let Some((state, completed, expiry)) = row else {
        return Ok(LookupAccountDeletionReceipt::NotFound);
    };
    if expiry <= now_ms {
        return Ok(LookupAccountDeletionReceipt::Expired);
    }
    Ok(LookupAccountDeletionReceipt::Found(receipt_from_parts(
        state, completed, expiry,
    )?))
}

fn mark_account_deletion_receipt_deleted_sqlite_tx(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    completed_at_ms: i64,
) -> Result<()> {
    let account_binding = account_deletion_hash("account", account_id);
    tx.execute(
        "UPDATE account_deletion_receipts
            SET state = 'deleted', updated_at_ms = ?2, completed_at_ms = ?2
          WHERE account_binding = ?1",
        params![account_binding, completed_at_ms],
    )?;
    Ok(())
}

fn mark_account_deletion_receipt_deleted_postgres_tx(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    completed_at_ms: i64,
) -> Result<()> {
    let account_binding = account_deletion_hash("account", account_id);
    tx.execute(
        "UPDATE account_deletion_receipts
            SET state = 'deleted', updated_at_ms = $2, completed_at_ms = $2
          WHERE account_binding = $1",
        &[&account_binding, &completed_at_ms],
    )?;
    Ok(())
}

fn complete_account_deletion_sqlite(
    pool: &DbPool,
    account_id: &str,
    completed_at_ms: i64,
) -> Result<AccountDeletionCompletion> {
    let mut conn = pool.get()?;
    let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let state = tx
        .query_row(
            "SELECT deletion_pending_at_ms FROM accounts WHERE id = ?1",
            params![account_id],
            |row| row.get::<_, Option<i64>>(0),
        )
        .optional()?;
    let Some(state) = state else {
        return Ok(AccountDeletionCompletion::NotFound);
    };
    if state.is_none()
        || !crate::db::object_uploads::account_deletion_objects_ready_sqlite_tx(&tx, account_id)?
    {
        tx.commit()?;
        return Ok(AccountDeletionCompletion::Pending);
    }
    tx.execute(
        "DELETE FROM stripe_webhook_events
          WHERE json_extract(body, '$.data.object.client_reference_id') = ?1
             OR json_extract(body, '$.data.object.metadata.bluey_account_id') = ?1",
        params![account_id],
    )?;
    mark_account_deletion_receipt_deleted_sqlite_tx(&tx, account_id, completed_at_ms)?;
    tx.execute("DELETE FROM accounts WHERE id = ?1", params![account_id])?;
    tx.commit()?;
    Ok(AccountDeletionCompletion::Deleted)
}

fn complete_account_deletion_postgres(
    pool: &DbPool,
    account_id: &str,
    completed_at_ms: i64,
) -> Result<AccountDeletionCompletion> {
    let mut conn = pool.get_pg()?;
    let mut tx = conn.transaction()?;
    let state = tx.query_opt(
        "SELECT deletion_pending_at_ms FROM accounts WHERE id = $1 FOR UPDATE",
        &[&account_id],
    )?;
    let Some(state) = state else {
        return Ok(AccountDeletionCompletion::NotFound);
    };
    if state.get::<_, Option<i64>>(0).is_none()
        || !crate::db::object_uploads::account_deletion_objects_ready_postgres_tx(
            &mut tx, account_id,
        )?
    {
        tx.commit()?;
        return Ok(AccountDeletionCompletion::Pending);
    }
    tx.execute(
        "DELETE FROM stripe_webhook_events
          WHERE body::jsonb #>> '{data,object,client_reference_id}' = $1
             OR body::jsonb #>> '{data,object,metadata,bluey_account_id}' = $1",
        &[&account_id],
    )
    .context("delete pending account Stripe webhook events")?;
    mark_account_deletion_receipt_deleted_postgres_tx(&mut tx, account_id, completed_at_ms)?;
    tx.execute("DELETE FROM accounts WHERE id = $1", &[&account_id])?;
    tx.commit()?;
    Ok(AccountDeletionCompletion::Deleted)
}

fn artifact_object_refs_sqlite(pool: &DbPool, account_id: &str) -> Result<Vec<ArtifactObjectRef>> {
    let conn = pool.get()?;
    let mut stmt = conn.prepare(
        "SELECT artifact_id, title, metadata_json
         FROM cloud_context_artifacts
         WHERE account_id = ?1
         ORDER BY created_at_ms ASC",
    )?;
    let rows = stmt.query_map(params![account_id], |row| {
        let artifact_id: String = row.get(0)?;
        let title: String = row.get(1)?;
        let metadata: String = row.get(2)?;
        Ok((artifact_id, title, metadata))
    })?;
    let mut refs = Vec::new();
    for row in rows {
        let (artifact_id, title, metadata) = row?;
        if let Some(reference) = object_ref_from_metadata(artifact_id, title, &metadata) {
            refs.push(reference);
        }
    }
    append_upload_ledger_artifact_refs_sqlite(&conn, account_id, &mut refs)?;
    Ok(refs)
}

fn artifact_object_refs_postgres(
    pool: &DbPool,
    account_id: &str,
) -> Result<Vec<ArtifactObjectRef>> {
    let mut conn = pool.get_pg()?;
    let rows = conn.query(
        "SELECT artifact_id, title, metadata_json
         FROM cloud_context_artifacts
         WHERE account_id = $1
         ORDER BY created_at_ms ASC",
        &[&account_id],
    )?;
    let mut refs = Vec::with_capacity(rows.len());
    for row in rows {
        let artifact_id: String = row.try_get(0)?;
        let title: String = row.try_get(1)?;
        let metadata: String = row.try_get(2)?;
        if let Some(reference) = object_ref_from_metadata(artifact_id, title, &metadata) {
            refs.push(reference);
        }
    }
    append_upload_ledger_artifact_refs_postgres(&mut conn, account_id, &mut refs)?;
    Ok(refs)
}

fn append_upload_ledger_artifact_refs_sqlite(
    conn: &rusqlite::Connection,
    account_id: &str,
    refs: &mut Vec<ArtifactObjectRef>,
) -> Result<()> {
    let mut seen = refs
        .iter()
        .map(|reference| reference.object_key.clone())
        .collect::<HashSet<_>>();
    let mut stmt = conn.prepare(
        "SELECT logical_id, object_key, content_type, size_bytes, sha256, expires_at_ms
           FROM object_uploads
          WHERE account_id = ?1 AND object_kind = 'artifact' AND state <> 'deleted'
          ORDER BY created_at_ms",
    )?;
    let rows = stmt.query_map(params![account_id], |row| {
        Ok(ArtifactObjectRef {
            artifact_id: row.get(0)?,
            title: "Uploaded artifact".to_string(),
            object_key: row.get(1)?,
            content_type: row.get(2)?,
            size_bytes: row.get(3)?,
            sha256: row.get(4)?,
            expires_at_ms: row.get(5)?,
        })
    })?;
    for row in rows {
        let reference = row?;
        if seen.insert(reference.object_key.clone()) {
            refs.push(reference);
        }
    }
    Ok(())
}

fn append_upload_ledger_artifact_refs_postgres(
    conn: &mut crate::db::SafePostgresClient,
    account_id: &str,
    refs: &mut Vec<ArtifactObjectRef>,
) -> Result<()> {
    let mut seen = refs
        .iter()
        .map(|reference| reference.object_key.clone())
        .collect::<HashSet<_>>();
    let rows = conn.query(
        "SELECT logical_id, object_key, content_type, size_bytes, sha256, expires_at_ms
           FROM object_uploads
          WHERE account_id = $1 AND object_kind = 'artifact' AND state <> 'deleted'
          ORDER BY created_at_ms",
        &[&account_id],
    )?;
    for row in rows {
        let reference = ArtifactObjectRef {
            artifact_id: row.try_get(0)?,
            title: "Uploaded artifact".to_string(),
            object_key: row.try_get(1)?,
            content_type: row.try_get(2)?,
            size_bytes: row.try_get(3)?,
            sha256: row.try_get(4)?,
            expires_at_ms: row.try_get(5)?,
        };
        if seen.insert(reference.object_key.clone()) {
            refs.push(reference);
        }
    }
    Ok(())
}

fn object_ref_from_metadata(
    artifact_id: String,
    title: String,
    metadata_json: &str,
) -> Option<ArtifactObjectRef> {
    let metadata = serde_json::from_str::<serde_json::Value>(metadata_json).ok()?;
    let object_key = metadata
        .get("object_key")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())?
        .to_string();
    Some(ArtifactObjectRef {
        artifact_id,
        title,
        object_key,
        content_type: metadata
            .get("object_content_type")
            .and_then(|value| value.as_str())
            .map(ToString::to_string),
        size_bytes: metadata
            .get("object_size_bytes")
            .or_else(|| metadata.get("size_bytes"))
            .and_then(|value| value.as_i64()),
        sha256: metadata
            .get("object_sha256")
            .and_then(|value| value.as_str())
            .map(ToString::to_string),
        expires_at_ms: metadata
            .get("object_expires_at_ms")
            .and_then(|value| value.as_i64()),
    })
}

fn export_rows(
    conn: &rusqlite::Connection,
    sql: &str,
    account_id: &str,
    columns: &[&str],
) -> Result<Vec<serde_json::Value>> {
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map(params![account_id], |row| {
        let mut obj = serde_json::Map::new();
        for (idx, column) in columns.iter().enumerate() {
            let value: rusqlite::types::Value = row.get(idx)?;
            obj.insert((*column).to_string(), sqlite_value_to_json(value));
        }
        Ok(serde_json::Value::Object(obj))
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

fn export_account_from_pg(row: PgRow) -> Result<ExportAccount> {
    Ok(ExportAccount {
        id: row.try_get(0)?,
        email: row.try_get(1)?,
        balance_cents: row.try_get(2)?,
        trial_seconds_remaining: row.try_get(3)?,
        created_at: row.try_get(4)?,
        last_login_at: row.try_get(5)?,
        stripe_customer_id: row.try_get(6)?,
        stripe_payment_method_id: row.try_get(7)?,
        square_customer_id: row.try_get(8)?,
        square_card_id: row.try_get(9)?,
        square_card_brand: row.try_get(10)?,
        square_card_last4: row.try_get(11)?,
    })
}

fn export_rows_pg(
    conn: &mut postgres::Client,
    sql: &str,
    account_id: &str,
) -> Result<Vec<serde_json::Value>> {
    let wrapped =
        format!("SELECT COALESCE(jsonb_agg(to_jsonb(rows)), '[]'::jsonb)::text FROM ({sql}) rows");
    let raw: String = conn
        .query_one(&wrapped, &[&account_id])
        .with_context(|| format!("export postgres rows for query: {sql}"))?
        .try_get(0)?;
    Ok(serde_json::from_str(&raw)?)
}

fn sqlite_value_to_json(value: rusqlite::types::Value) -> serde_json::Value {
    match value {
        rusqlite::types::Value::Null => serde_json::Value::Null,
        rusqlite::types::Value::Integer(v) => serde_json::json!(v),
        rusqlite::types::Value::Real(v) => serde_json::json!(v),
        rusqlite::types::Value::Text(v) => serde_json::Value::String(v),
        rusqlite::types::Value::Blob(_) => serde_json::Value::String("<blob>".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deletion_receipt_is_idempotent_capability_bound_and_expires() {
        let path = std::env::temp_dir().join(format!(
            "bluey-account-deletion-receipt-{}.db",
            uuid::Uuid::new_v4()
        ));
        let pool = crate::db::open_pool(&path).expect("open receipt test database");
        crate::db::run_migrations(&pool).expect("migrate receipt test database");
        {
            let conn = pool.get().expect("receipt test connection");
            conn.execute(
                "INSERT INTO accounts(id, email, password_hash) VALUES (?1, ?2, 'hash')",
                rusqlite::params!["acct-a", "a@example.test"],
            )
            .expect("insert account A");
            conn.execute(
                "INSERT INTO accounts(id, email, password_hash) VALUES (?1, ?2, 'hash')",
                rusqlite::params!["acct-b", "b@example.test"],
            )
            .expect("insert account B");
        }

        let operation = "550e8400-e29b-41d4-a716-446655440301";
        let capability = "550e8400-e29b-41d4-a716-446655440302";
        let wrong_capability = "550e8400-e29b-41d4-a716-446655440303";
        let now_ms = chrono::Utc::now().timestamp_millis();
        let first =
            prepare_account_deletion_receipt(&pool, "acct-a", operation, capability, now_ms)
                .unwrap();
        assert!(matches!(
            first.clone(),
            PrepareAccountDeletionReceipt::Ready(AccountDeletionReceipt {
                state: AccountDeletionReceiptState::Prepared,
                ..
            })
        ));
        assert_eq!(
            prepare_account_deletion_receipt(&pool, "acct-a", operation, capability, now_ms + 1,)
                .unwrap(),
            first,
            "the exact operation must replay without changing its capability"
        );
        assert_eq!(
            prepare_account_deletion_receipt(&pool, "acct-b", operation, capability, now_ms + 2,)
                .unwrap(),
            PrepareAccountDeletionReceipt::Conflict,
            "another account cannot claim an existing operation"
        );
        assert_eq!(
            lookup_account_deletion_receipt(&pool, operation, wrong_capability, now_ms + 3,)
                .unwrap(),
            LookupAccountDeletionReceipt::NotFound
        );
        let stored: (String, String) = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT account_binding, capability_hash
                   FROM account_deletion_receipts
                  WHERE operation_id = ?1",
                rusqlite::params![operation],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_ne!(stored.0, "acct-a");
        assert_ne!(stored.1, capability);
        assert!(matches!(
            mark_account_deletion_receipt_pending(
                &pool,
                "acct-a",
                operation,
                capability,
                now_ms + 4,
            )
            .unwrap(),
            Some(AccountDeletionReceipt {
                state: AccountDeletionReceiptState::Pending,
                ..
            })
        ));
        assert!(begin_account_deletion(&pool, "acct-a", now_ms + 5).unwrap());
        assert_eq!(
            complete_account_deletion(&pool, "acct-a").unwrap(),
            AccountDeletionCompletion::Deleted
        );
        assert!(matches!(
            lookup_account_deletion_receipt(&pool, operation, capability, now_ms + 6).unwrap(),
            LookupAccountDeletionReceipt::Found(AccountDeletionReceipt {
                state: AccountDeletionReceiptState::Deleted,
                completed_at_ms: Some(_),
                ..
            })
        ));
        assert_eq!(
            lookup_account_deletion_receipt(
                &pool,
                operation,
                capability,
                now_ms + ACCOUNT_DELETION_RECEIPT_TTL_MS,
            )
            .unwrap(),
            LookupAccountDeletionReceipt::Expired
        );

        drop(pool);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    #[serial_test::serial]
    fn postgres_webhook_cleanup_failure_preserves_retryable_account_deletion() {
        let Ok(database_url) = std::env::var("BLUEY_TEST_POSTGRES_URL") else {
            return;
        };
        let pool = crate::db::open_postgres_pool(&database_url).expect("open Postgres test pool");
        crate::db::run_migrations(&pool).expect("apply Postgres runtime migrations");
        let suffix = uuid::Uuid::new_v4().simple().to_string();
        let account_id = format!("acct_delete_retry_{suffix}");
        let email = format!("delete-retry-{suffix}@example.test");
        let event_id = format!("evt_delete_retry_{suffix}");
        {
            let mut conn = pool.get_pg().expect("Postgres setup connection");
            conn.execute(
                "INSERT INTO accounts (id, email, password_hash) VALUES ($1, $2, 'hash')",
                &[&account_id, &email],
            )
            .expect("insert deletion test account");
            conn.execute(
                "INSERT INTO stripe_webhook_events (event_id, type, body)
                 VALUES ($1, 'test.invalid_json', '{invalid-json')",
                &[&event_id],
            )
            .expect("insert malformed webhook fixture");
        }

        assert!(begin_account_deletion(&pool, &account_id, 123).unwrap());
        let error = complete_account_deletion(&pool, &account_id).unwrap_err();
        assert!(format!("{error:#}").contains("delete pending account Stripe webhook events"));
        assert!(account_deletion_is_pending(&pool, &account_id).unwrap());

        {
            let mut conn = pool.get_pg().expect("Postgres cleanup connection");
            conn.execute(
                "DELETE FROM stripe_webhook_events WHERE event_id = $1",
                &[&event_id],
            )
            .expect("remove malformed webhook fixture");
        }
        assert_eq!(
            complete_account_deletion(&pool, &account_id).unwrap(),
            AccountDeletionCompletion::Deleted
        );
    }
}
