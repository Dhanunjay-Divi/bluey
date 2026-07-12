//! Durable object metadata, quota reservations, and R2/S3 outbox state.

use anyhow::{Context, Result};
use postgres::{Row as PgRow, Transaction as PgTransaction};
use rusqlite::{params, OptionalExtension, Transaction as SqliteTransaction, TransactionBehavior};

use super::{DbPool, SafePostgresClient};
use crate::object_storage::{sha256_hex, UploadLimits};

const DAY_MS: i64 = 86_400_000;
const PROCESSING_LEASE_MS: i64 = 5 * 60 * 1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObjectKind {
    Artifact,
    SessionAudit,
}

impl ObjectKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Artifact => "artifact",
            Self::SessionAudit => "session_audit",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageScope {
    Artifact,
    Audit,
}

impl StorageScope {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Artifact => "artifact",
            Self::Audit => "audit",
        }
    }
}

#[derive(Debug, Clone)]
pub struct NewObjectUpload {
    pub account_id: String,
    pub object_kind: ObjectKind,
    pub logical_id: String,
    pub session_id: Option<String>,
    pub storage_scope: StorageScope,
    pub object_key: String,
    pub size_bytes: i64,
    pub sha256: String,
    pub content_type: String,
    pub expires_at_ms: i64,
    pub metadata_json: serde_json::Value,
    pub now_ms: i64,
    pub limits: UploadLimits,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectUpload {
    pub id: String,
    pub account_id: String,
    pub object_kind: String,
    pub logical_id: String,
    pub session_id: Option<String>,
    pub storage_scope: String,
    pub object_key: String,
    pub size_bytes: i64,
    pub sha256: String,
    pub content_type: String,
    pub expires_at_ms: i64,
    pub state: String,
    pub metadata_json: String,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
    pub uploaded_at_ms: Option<i64>,
    pub deleted_at_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UploadReservation {
    pub upload: ObjectUpload,
    pub needs_put: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CleanupJob {
    pub upload_id: String,
    pub account_id: String,
    pub object_key: String,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum UploadControlError {
    #[error("object exceeds the per-object upload limit")]
    ObjectTooLarge,
    #[error("account object-byte quota exceeded")]
    AccountBytesQuotaExceeded,
    #[error("account object-count quota exceeded")]
    AccountObjectQuotaExceeded,
    #[error("daily account upload quota exceeded")]
    DailyQuotaExceeded,
    #[error("the stable object id is already bound to different content")]
    IdempotencyConflict,
    #[error("the object upload is already in progress")]
    UploadInProgress,
    #[error("session not found for this account")]
    SessionNotOwned,
    #[error("object upload metadata not found")]
    UploadNotFound,
    #[error("object upload has been deleted")]
    UploadGone,
    #[error("invalid object upload metadata: {0}")]
    InvalidMetadata(&'static str),
}

pub fn reserve_upload(pool: &DbPool, input: &NewObjectUpload) -> Result<UploadReservation> {
    validate_input(input)?;
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => reserve_upload_sqlite(pool, input),
        DbPool::Postgres(_) => reserve_upload_postgres(pool, input),
    })
}

pub fn mark_upload_ready(pool: &DbPool, upload_id: &str, now_ms: i64) -> Result<ObjectUpload> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => mark_upload_ready_sqlite(pool, upload_id, now_ms),
        DbPool::Postgres(_) => mark_upload_ready_postgres(pool, upload_id, now_ms),
    })
}

pub fn record_put_failure(pool: &DbPool, upload_id: &str, error: &str, now_ms: i64) -> Result<()> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => record_put_failure_sqlite(pool, upload_id, error, now_ms),
        DbPool::Postgres(_) => record_put_failure_postgres(pool, upload_id, error, now_ms),
    })
}

pub fn ready_artifact(
    pool: &DbPool,
    account_id: &str,
    artifact_id: &str,
) -> Result<Option<ObjectUpload>> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => ready_artifact_sqlite(pool, account_id, artifact_id),
        DbPool::Postgres(_) => ready_artifact_postgres(pool, account_id, artifact_id),
    })
}

pub fn artifact_upload(
    pool: &DbPool,
    account_id: &str,
    artifact_id: &str,
) -> Result<Option<ObjectUpload>> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => artifact_upload_sqlite(pool, account_id, artifact_id),
        DbPool::Postgres(_) => artifact_upload_postgres(pool, account_id, artifact_id),
    })
}

pub fn claim_cleanup_jobs(
    pool: &DbPool,
    account_id: &str,
    storage_scope: StorageScope,
    now_ms: i64,
    stale_pending_before_ms: i64,
    limit: i64,
) -> Result<Vec<CleanupJob>> {
    let limit = limit.clamp(1, 25);
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => claim_cleanup_jobs_sqlite(
            pool,
            account_id,
            storage_scope,
            now_ms,
            stale_pending_before_ms,
            limit,
        ),
        DbPool::Postgres(_) => claim_cleanup_jobs_postgres(
            pool,
            account_id,
            storage_scope,
            now_ms,
            stale_pending_before_ms,
            limit,
        ),
    })
}

pub fn claim_global_cleanup_jobs(
    pool: &DbPool,
    storage_scope: StorageScope,
    now_ms: i64,
    stale_pending_before_ms: i64,
    limit: i64,
) -> Result<Vec<CleanupJob>> {
    let limit = limit.clamp(1, 100);
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => claim_global_cleanup_jobs_sqlite(
            pool,
            storage_scope,
            now_ms,
            stale_pending_before_ms,
            limit,
        ),
        DbPool::Postgres(_) => claim_global_cleanup_jobs_postgres(
            pool,
            storage_scope,
            now_ms,
            stale_pending_before_ms,
            limit,
        ),
    })
}

pub fn mark_cleanup_succeeded(pool: &DbPool, upload_id: &str, now_ms: i64) -> Result<()> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => mark_cleanup_succeeded_sqlite(pool, upload_id, now_ms),
        DbPool::Postgres(_) => mark_cleanup_succeeded_postgres(pool, upload_id, now_ms),
    })
}

pub fn mark_cleanup_failed(pool: &DbPool, upload_id: &str, error: &str, now_ms: i64) -> Result<()> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => mark_cleanup_failed_sqlite(pool, upload_id, error, now_ms),
        DbPool::Postgres(_) => mark_cleanup_failed_postgres(pool, upload_id, error, now_ms),
    })
}

pub(crate) fn link_artifact_session_sqlite_tx(
    tx: &SqliteTransaction<'_>,
    account_id: &str,
    artifact_id: &str,
    session_id: &str,
    now_ms: i64,
) -> Result<()> {
    tx.execute(
        "UPDATE object_uploads
            SET session_id = ?3, updated_at_ms = MAX(updated_at_ms, ?4)
          WHERE account_id = ?1 AND object_kind = 'artifact' AND logical_id = ?2
            AND state <> 'deleted'",
        params![account_id, artifact_id, session_id, now_ms],
    )?;
    let deleted = tx
        .query_row(
            "SELECT 1 FROM cloud_sessions
              WHERE account_id = ?1 AND session_id = ?2 AND deleted_at_ms IS NOT NULL",
            params![account_id, session_id],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    if deleted {
        schedule_session_cleanup_sqlite_tx(tx, account_id, session_id, now_ms)?;
    }
    Ok(())
}

pub(crate) fn link_artifact_session_postgres_tx(
    tx: &mut PgTransaction<'_>,
    account_id: &str,
    artifact_id: &str,
    session_id: &str,
    now_ms: i64,
) -> Result<()> {
    tx.execute(
        "UPDATE object_uploads
            SET session_id = $3, updated_at_ms = GREATEST(updated_at_ms, $4)
          WHERE account_id = $1 AND object_kind = 'artifact' AND logical_id = $2
            AND state <> 'deleted'",
        &[&account_id, &artifact_id, &session_id, &now_ms],
    )?;
    let deleted = tx
        .query_opt(
            "SELECT 1 FROM cloud_sessions
              WHERE account_id = $1 AND session_id = $2 AND deleted_at_ms IS NOT NULL",
            &[&account_id, &session_id],
        )?
        .is_some();
    if deleted {
        schedule_session_cleanup_postgres_tx(tx, account_id, session_id, now_ms)?;
    }
    Ok(())
}

pub(crate) fn schedule_session_cleanup_sqlite_tx(
    tx: &SqliteTransaction<'_>,
    account_id: &str,
    session_id: &str,
    now_ms: i64,
) -> Result<usize> {
    let changed = tx.execute(
        "UPDATE object_uploads
            SET state = 'delete_pending', updated_at_ms = ?3
          WHERE account_id = ?1 AND session_id = ?2 AND state IN ('pending', 'ready')",
        params![account_id, session_id, now_ms],
    )?;
    enqueue_session_deletes_sqlite(tx, account_id, session_id, now_ms)?;
    Ok(changed)
}

pub(crate) fn schedule_session_cleanup_postgres_tx(
    tx: &mut PgTransaction<'_>,
    account_id: &str,
    session_id: &str,
    now_ms: i64,
) -> Result<usize> {
    let changed = tx.execute(
        "UPDATE object_uploads
            SET state = 'delete_pending', updated_at_ms = $3
          WHERE account_id = $1 AND session_id = $2 AND state IN ('pending', 'ready')",
        &[&account_id, &session_id, &now_ms],
    )?;
    enqueue_session_deletes_postgres(tx, account_id, session_id, now_ms)?;
    Ok(changed as usize)
}

fn validate_input(input: &NewObjectUpload) -> Result<()> {
    if input.account_id.trim().is_empty() {
        return Err(UploadControlError::InvalidMetadata("account id").into());
    }
    if input.logical_id.trim().is_empty() || input.logical_id.len() > 384 {
        return Err(UploadControlError::InvalidMetadata("logical id").into());
    }
    if input.object_key.trim().is_empty() || input.object_key.len() > 2048 {
        return Err(UploadControlError::InvalidMetadata("object key").into());
    }
    if input.content_type.trim().is_empty() || input.content_type.len() > 255 {
        return Err(UploadControlError::InvalidMetadata("content type").into());
    }
    if input.sha256.len() != 64 || !input.sha256.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(UploadControlError::InvalidMetadata("sha256").into());
    }
    if input.size_bytes <= 0 || input.limits.max_object_bytes <= 0 {
        return Err(UploadControlError::InvalidMetadata("size").into());
    }
    if input.size_bytes > input.limits.max_object_bytes {
        return Err(UploadControlError::ObjectTooLarge.into());
    }
    if input.expires_at_ms <= input.now_ms {
        return Err(UploadControlError::InvalidMetadata("expiration").into());
    }
    if input.object_kind == ObjectKind::SessionAudit && input.session_id.is_none() {
        return Err(UploadControlError::InvalidMetadata("audit session").into());
    }
    Ok(())
}

fn reserve_upload_sqlite(pool: &DbPool, input: &NewObjectUpload) -> Result<UploadReservation> {
    let mut conn = pool.get().context("get sqlite object upload conn")?;
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .context("begin sqlite object upload reservation")?;

    validate_session_sqlite(&tx, input)?;
    if let Some(existing) = load_logical_upload_sqlite(
        &tx,
        &input.account_id,
        input.object_kind.as_str(),
        &input.logical_id,
    )? {
        let reservation = retry_reservation(&existing, input)?;
        if reservation.needs_put {
            reopen_put_outbox_sqlite(&tx, &existing.id, input.now_ms)?;
        }
        tx.commit()?;
        return Ok(reservation);
    }

    enforce_quota_sqlite(&tx, input)?;
    let upload_id = stable_upload_id(input);
    let metadata_json = serde_json::to_string(&input.metadata_json)?;
    tx.execute(
        "INSERT INTO object_uploads (
            id, account_id, object_kind, logical_id, session_id, storage_scope,
            object_key, size_bytes, sha256, content_type, expires_at_ms, state,
            metadata_json, created_at_ms, updated_at_ms
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 'pending', ?12, ?13, ?13)",
        params![
            upload_id,
            input.account_id,
            input.object_kind.as_str(),
            input.logical_id,
            input.session_id,
            input.storage_scope.as_str(),
            input.object_key,
            input.size_bytes,
            input.sha256,
            input.content_type,
            input.expires_at_ms,
            metadata_json,
            input.now_ms,
        ],
    )?;
    insert_put_outbox_sqlite(&tx, &upload_id, &input.account_id, input.now_ms)?;
    add_daily_usage_sqlite(&tx, input)?;
    let upload = load_upload_sqlite(&tx, &upload_id)?.ok_or(UploadControlError::UploadNotFound)?;
    tx.commit()?;
    Ok(UploadReservation {
        upload,
        needs_put: true,
    })
}

fn reserve_upload_postgres(pool: &DbPool, input: &NewObjectUpload) -> Result<UploadReservation> {
    let mut conn = pool.get_pg().context("get postgres object upload conn")?;
    let mut tx = conn
        .transaction()
        .context("begin postgres object upload reservation")?;
    let account_exists = tx
        .query_opt(
            "SELECT id FROM accounts WHERE id = $1 FOR UPDATE",
            &[&input.account_id],
        )?
        .is_some();
    if !account_exists {
        return Err(UploadControlError::SessionNotOwned.into());
    }

    validate_session_postgres(&mut tx, input)?;
    if let Some(existing) = load_logical_upload_postgres(
        &mut tx,
        &input.account_id,
        input.object_kind.as_str(),
        &input.logical_id,
    )? {
        let reservation = retry_reservation(&existing, input)?;
        if reservation.needs_put {
            reopen_put_outbox_postgres(&mut tx, &existing.id, input.now_ms)?;
        }
        tx.commit()?;
        return Ok(reservation);
    }

    enforce_quota_postgres(&mut tx, input)?;
    let upload_id = stable_upload_id(input);
    let metadata_json = serde_json::to_string(&input.metadata_json)?;
    tx.execute(
        "INSERT INTO object_uploads (
            id, account_id, object_kind, logical_id, session_id, storage_scope,
            object_key, size_bytes, sha256, content_type, expires_at_ms, state,
            metadata_json, created_at_ms, updated_at_ms
         ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, 'pending', $12, $13, $13)",
        &[
            &upload_id,
            &input.account_id,
            &input.object_kind.as_str(),
            &input.logical_id,
            &input.session_id,
            &input.storage_scope.as_str(),
            &input.object_key,
            &input.size_bytes,
            &input.sha256,
            &input.content_type,
            &input.expires_at_ms,
            &metadata_json,
            &input.now_ms,
        ],
    )?;
    insert_put_outbox_postgres(&mut tx, &upload_id, &input.account_id, input.now_ms)?;
    add_daily_usage_postgres(&mut tx, input)?;
    let upload =
        load_upload_postgres(&mut tx, &upload_id)?.ok_or(UploadControlError::UploadNotFound)?;
    tx.commit()?;
    Ok(UploadReservation {
        upload,
        needs_put: true,
    })
}

fn validate_session_sqlite(tx: &SqliteTransaction<'_>, input: &NewObjectUpload) -> Result<()> {
    let Some(session_id) = input.session_id.as_deref() else {
        return Ok(());
    };
    if input.object_kind != ObjectKind::SessionAudit {
        return Ok(());
    }
    let owned = tx
        .query_row(
            "SELECT 1 FROM cloud_sessions
              WHERE account_id = ?1 AND session_id = ?2 AND deleted_at_ms IS NULL",
            params![input.account_id, session_id],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    if !owned {
        return Err(UploadControlError::SessionNotOwned.into());
    }
    Ok(())
}

fn validate_session_postgres(tx: &mut PgTransaction<'_>, input: &NewObjectUpload) -> Result<()> {
    let Some(session_id) = input.session_id.as_deref() else {
        return Ok(());
    };
    if input.object_kind != ObjectKind::SessionAudit {
        return Ok(());
    }
    let owned = tx
        .query_opt(
            "SELECT 1 FROM cloud_sessions
              WHERE account_id = $1 AND session_id = $2 AND deleted_at_ms IS NULL",
            &[&input.account_id, &session_id],
        )?
        .is_some();
    if !owned {
        return Err(UploadControlError::SessionNotOwned.into());
    }
    Ok(())
}

fn retry_reservation(
    existing: &ObjectUpload,
    input: &NewObjectUpload,
) -> Result<UploadReservation> {
    let same_content = existing.sha256.eq_ignore_ascii_case(&input.sha256)
        && existing.size_bytes == input.size_bytes
        && existing.storage_scope == input.storage_scope.as_str();
    if matches!(existing.state.as_str(), "delete_pending" | "deleted") {
        return Err(UploadControlError::UploadGone.into());
    }
    if !same_content {
        return Err(UploadControlError::IdempotencyConflict.into());
    }
    Ok(UploadReservation {
        upload: existing.clone(),
        needs_put: existing.state != "ready",
    })
}

fn enforce_quota_sqlite(tx: &SqliteTransaction<'_>, input: &NewObjectUpload) -> Result<()> {
    let (bytes, objects): (i64, i64) = tx.query_row(
        "SELECT COALESCE(SUM(size_bytes), 0), COUNT(*)
           FROM object_uploads
          WHERE account_id = ?1 AND state IN ('pending', 'ready', 'delete_pending')",
        params![input.account_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    enforce_total_limits(bytes, objects, input)?;

    let day_start_ms = day_start_ms(input.now_ms);
    let (daily_bytes, daily_objects) = tx
        .query_row(
            "SELECT reserved_bytes, reserved_objects
               FROM object_upload_daily_usage
              WHERE account_id = ?1 AND day_start_ms = ?2",
            params![input.account_id, day_start_ms],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
        )
        .optional()?
        .unwrap_or((0, 0));
    enforce_daily_limits(daily_bytes, daily_objects, input)
}

fn enforce_quota_postgres(tx: &mut PgTransaction<'_>, input: &NewObjectUpload) -> Result<()> {
    let row = tx.query_one(
        "SELECT COALESCE(SUM(size_bytes), 0)::bigint, COUNT(*)::bigint
           FROM object_uploads
          WHERE account_id = $1 AND state IN ('pending', 'ready', 'delete_pending')",
        &[&input.account_id],
    )?;
    enforce_total_limits(row.try_get(0)?, row.try_get(1)?, input)?;

    let day_start_ms = day_start_ms(input.now_ms);
    let usage = tx.query_opt(
        "SELECT reserved_bytes, reserved_objects
           FROM object_upload_daily_usage
          WHERE account_id = $1 AND day_start_ms = $2",
        &[&input.account_id, &day_start_ms],
    )?;
    let (daily_bytes, daily_objects) = match usage {
        Some(row) => (row.try_get::<_, i64>(0)?, row.try_get::<_, i64>(1)?),
        None => (0, 0),
    };
    enforce_daily_limits(daily_bytes, daily_objects, input)
}

fn enforce_total_limits(bytes: i64, objects: i64, input: &NewObjectUpload) -> Result<()> {
    if exceeds(bytes, input.size_bytes, input.limits.max_account_bytes) {
        return Err(UploadControlError::AccountBytesQuotaExceeded.into());
    }
    if exceeds(objects, 1, input.limits.max_account_objects) {
        return Err(UploadControlError::AccountObjectQuotaExceeded.into());
    }
    Ok(())
}

fn enforce_daily_limits(bytes: i64, objects: i64, input: &NewObjectUpload) -> Result<()> {
    if exceeds(bytes, input.size_bytes, input.limits.max_daily_bytes)
        || exceeds(objects, 1, input.limits.max_account_objects)
    {
        return Err(UploadControlError::DailyQuotaExceeded.into());
    }
    Ok(())
}

fn exceeds(current: i64, additional: i64, limit: i64) -> bool {
    limit <= 0 || additional > limit || current > limit.saturating_sub(additional)
}

fn add_daily_usage_sqlite(tx: &SqliteTransaction<'_>, input: &NewObjectUpload) -> Result<()> {
    tx.execute(
        "INSERT INTO object_upload_daily_usage (
            account_id, day_start_ms, reserved_bytes, reserved_objects, updated_at_ms
         ) VALUES (?1, ?2, ?3, 1, ?4)
         ON CONFLICT(account_id, day_start_ms) DO UPDATE SET
            reserved_bytes = object_upload_daily_usage.reserved_bytes + excluded.reserved_bytes,
            reserved_objects = object_upload_daily_usage.reserved_objects + 1,
            updated_at_ms = excluded.updated_at_ms",
        params![
            input.account_id,
            day_start_ms(input.now_ms),
            input.size_bytes,
            input.now_ms
        ],
    )?;
    Ok(())
}

fn add_daily_usage_postgres(tx: &mut PgTransaction<'_>, input: &NewObjectUpload) -> Result<()> {
    tx.execute(
        "INSERT INTO object_upload_daily_usage (
            account_id, day_start_ms, reserved_bytes, reserved_objects, updated_at_ms
         ) VALUES ($1, $2, $3, 1, $4)
         ON CONFLICT(account_id, day_start_ms) DO UPDATE SET
            reserved_bytes = object_upload_daily_usage.reserved_bytes + EXCLUDED.reserved_bytes,
            reserved_objects = object_upload_daily_usage.reserved_objects + 1,
            updated_at_ms = EXCLUDED.updated_at_ms",
        &[
            &input.account_id,
            &day_start_ms(input.now_ms),
            &input.size_bytes,
            &input.now_ms,
        ],
    )?;
    Ok(())
}

fn insert_put_outbox_sqlite(
    tx: &SqliteTransaction<'_>,
    upload_id: &str,
    account_id: &str,
    now_ms: i64,
) -> Result<()> {
    tx.execute(
        "INSERT INTO object_storage_outbox (
            id, upload_id, account_id, operation, state, attempt_count, next_attempt_at_ms,
            created_at_ms, updated_at_ms
         ) VALUES (?1, ?2, ?3, 'put', 'processing', 1, ?4, ?4, ?4)",
        params![outbox_id(upload_id, "put"), upload_id, account_id, now_ms],
    )?;
    Ok(())
}

fn insert_put_outbox_postgres(
    tx: &mut PgTransaction<'_>,
    upload_id: &str,
    account_id: &str,
    now_ms: i64,
) -> Result<()> {
    tx.execute(
        "INSERT INTO object_storage_outbox (
            id, upload_id, account_id, operation, state, attempt_count, next_attempt_at_ms,
            created_at_ms, updated_at_ms
         ) VALUES ($1, $2, $3, 'put', 'processing', 1, $4, $4, $4)",
        &[
            &outbox_id(upload_id, "put"),
            &upload_id,
            &account_id,
            &now_ms,
        ],
    )?;
    Ok(())
}

fn reopen_put_outbox_sqlite(
    tx: &SqliteTransaction<'_>,
    upload_id: &str,
    now_ms: i64,
) -> Result<()> {
    let (state, attempts, updated_at_ms) = tx
        .query_row(
            "SELECT state, attempt_count, updated_at_ms
               FROM object_storage_outbox
              WHERE upload_id = ?1 AND operation = 'put'",
            params![upload_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i32>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            },
        )
        .optional()?
        .ok_or(UploadControlError::UploadNotFound)?;
    if state == "processing" && updated_at_ms > now_ms.saturating_sub(PROCESSING_LEASE_MS) {
        return Err(UploadControlError::UploadInProgress.into());
    }
    tx.execute(
        "UPDATE object_uploads SET updated_at_ms = ?2
          WHERE id = ?1 AND state = 'pending'",
        params![upload_id, now_ms],
    )?;
    tx.execute(
        "UPDATE object_storage_outbox
            SET state = 'processing', attempt_count = ?3,
                next_attempt_at_ms = ?2, updated_at_ms = ?2
          WHERE upload_id = ?1 AND operation = 'put' AND state <> 'completed'",
        params![upload_id, now_ms, attempts.saturating_add(1)],
    )?;
    Ok(())
}

fn reopen_put_outbox_postgres(
    tx: &mut PgTransaction<'_>,
    upload_id: &str,
    now_ms: i64,
) -> Result<()> {
    let row = tx
        .query_opt(
            "SELECT state, attempt_count, updated_at_ms
               FROM object_storage_outbox
              WHERE upload_id = $1 AND operation = 'put'
              FOR UPDATE",
            &[&upload_id],
        )?
        .ok_or(UploadControlError::UploadNotFound)?;
    let state: String = row.try_get(0)?;
    let attempts: i32 = row.try_get(1)?;
    let updated_at_ms: i64 = row.try_get(2)?;
    if state == "processing" && updated_at_ms > now_ms.saturating_sub(PROCESSING_LEASE_MS) {
        return Err(UploadControlError::UploadInProgress.into());
    }
    tx.execute(
        "UPDATE object_uploads SET updated_at_ms = $2
          WHERE id = $1 AND state = 'pending'",
        &[&upload_id, &now_ms],
    )?;
    tx.execute(
        "UPDATE object_storage_outbox
            SET state = 'processing', attempt_count = $3,
                next_attempt_at_ms = $2, updated_at_ms = $2
          WHERE upload_id = $1 AND operation = 'put' AND state <> 'completed'",
        &[&upload_id, &now_ms, &attempts.saturating_add(1)],
    )?;
    Ok(())
}

fn mark_upload_ready_sqlite(pool: &DbPool, upload_id: &str, now_ms: i64) -> Result<ObjectUpload> {
    let mut conn = pool.get()?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let mut upload =
        load_upload_sqlite(&tx, upload_id)?.ok_or(UploadControlError::UploadNotFound)?;
    if matches!(upload.state.as_str(), "delete_pending" | "deleted") {
        return Err(UploadControlError::UploadGone.into());
    }
    tx.execute(
        "UPDATE object_uploads
            SET state = 'ready', uploaded_at_ms = COALESCE(uploaded_at_ms, ?2),
                updated_at_ms = ?2
          WHERE id = ?1",
        params![upload_id, now_ms],
    )?;
    tx.execute(
        "UPDATE object_storage_outbox
            SET state = 'completed', updated_at_ms = ?2, completed_at_ms = ?2,
                next_attempt_at_ms = ?2, last_error = NULL
          WHERE upload_id = ?1 AND operation = 'put'",
        params![upload_id, now_ms],
    )?;
    upload.state = "ready".to_string();
    upload.updated_at_ms = now_ms;
    upload.uploaded_at_ms.get_or_insert(now_ms);
    publish_index_sqlite(&tx, &upload)?;
    tx.commit()?;
    Ok(upload)
}

fn mark_upload_ready_postgres(pool: &DbPool, upload_id: &str, now_ms: i64) -> Result<ObjectUpload> {
    let mut conn = pool.get_pg()?;
    let mut tx = conn.transaction()?;
    let mut upload =
        load_upload_postgres(&mut tx, upload_id)?.ok_or(UploadControlError::UploadNotFound)?;
    if matches!(upload.state.as_str(), "delete_pending" | "deleted") {
        return Err(UploadControlError::UploadGone.into());
    }
    tx.execute(
        "UPDATE object_uploads
            SET state = 'ready', uploaded_at_ms = COALESCE(uploaded_at_ms, $2),
                updated_at_ms = $2
          WHERE id = $1",
        &[&upload_id, &now_ms],
    )?;
    tx.execute(
        "UPDATE object_storage_outbox
            SET state = 'completed', updated_at_ms = $2, completed_at_ms = $2,
                next_attempt_at_ms = $2, last_error = NULL
          WHERE upload_id = $1 AND operation = 'put'",
        &[&upload_id, &now_ms],
    )?;
    upload.state = "ready".to_string();
    upload.updated_at_ms = now_ms;
    upload.uploaded_at_ms.get_or_insert(now_ms);
    publish_index_postgres(&mut tx, &upload)?;
    tx.commit()?;
    Ok(upload)
}

fn publish_index_sqlite(tx: &SqliteTransaction<'_>, upload: &ObjectUpload) -> Result<()> {
    if upload.object_kind == ObjectKind::SessionAudit.as_str() {
        let session_id = upload
            .session_id
            .as_deref()
            .ok_or(UploadControlError::InvalidMetadata("audit session"))?;
        let session_code = uuid::Uuid::parse_str(session_id)
            .ok()
            .map(cue_core::short_session_code);
        tx.execute(
            "INSERT INTO diagnostic_log_chunks (
                id, account_id, session_id, session_code, kind, storage, object_key,
                bytes, sha256, created_at_ms, expires_at_ms, metadata_json
             ) VALUES (?1, ?2, ?3, ?4, 'session_audit_bundle', 'r2', ?5, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT(id) DO UPDATE SET
                account_id = excluded.account_id,
                session_id = excluded.session_id,
                session_code = excluded.session_code,
                object_key = excluded.object_key,
                bytes = excluded.bytes,
                sha256 = excluded.sha256,
                expires_at_ms = excluded.expires_at_ms,
                metadata_json = excluded.metadata_json",
            params![
                diagnostic_id(upload),
                upload.account_id,
                session_id,
                session_code,
                upload.object_key,
                upload.size_bytes,
                upload.sha256,
                upload.created_at_ms,
                upload.expires_at_ms,
                upload.metadata_json,
            ],
        )?;
    } else {
        merge_artifact_metadata_sqlite(tx, upload)?;
    }
    Ok(())
}

fn publish_index_postgres(tx: &mut PgTransaction<'_>, upload: &ObjectUpload) -> Result<()> {
    if upload.object_kind == ObjectKind::SessionAudit.as_str() {
        let session_id = upload
            .session_id
            .as_deref()
            .ok_or(UploadControlError::InvalidMetadata("audit session"))?;
        let session_code = uuid::Uuid::parse_str(session_id)
            .ok()
            .map(cue_core::short_session_code);
        tx.execute(
            "INSERT INTO diagnostic_log_chunks (
                id, account_id, session_id, session_code, kind, storage, object_key,
                bytes, sha256, created_at_ms, expires_at_ms, metadata_json
             ) VALUES ($1, $2, $3, $4, 'session_audit_bundle', 'r2', $5, $6, $7, $8, $9, $10)
             ON CONFLICT(id) DO UPDATE SET
                account_id = EXCLUDED.account_id,
                session_id = EXCLUDED.session_id,
                session_code = EXCLUDED.session_code,
                object_key = EXCLUDED.object_key,
                bytes = EXCLUDED.bytes,
                sha256 = EXCLUDED.sha256,
                expires_at_ms = EXCLUDED.expires_at_ms,
                metadata_json = EXCLUDED.metadata_json",
            &[
                &diagnostic_id(upload),
                &upload.account_id,
                &session_id,
                &session_code,
                &upload.object_key,
                &upload.size_bytes,
                &upload.sha256,
                &upload.created_at_ms,
                &upload.expires_at_ms,
                &upload.metadata_json,
            ],
        )?;
    } else {
        merge_artifact_metadata_postgres(tx, upload)?;
    }
    Ok(())
}

fn merge_artifact_metadata_sqlite(tx: &SqliteTransaction<'_>, upload: &ObjectUpload) -> Result<()> {
    let metadata = tx
        .query_row(
            "SELECT metadata_json FROM cloud_context_artifacts
              WHERE account_id = ?1 AND artifact_id = ?2",
            params![upload.account_id, upload.logical_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    let Some(metadata) = metadata else {
        return Ok(());
    };
    tx.execute(
        "UPDATE cloud_context_artifacts SET metadata_json = ?3
          WHERE account_id = ?1 AND artifact_id = ?2",
        params![
            upload.account_id,
            upload.logical_id,
            with_object_metadata(&metadata, upload)
        ],
    )?;
    Ok(())
}

fn merge_artifact_metadata_postgres(
    tx: &mut PgTransaction<'_>,
    upload: &ObjectUpload,
) -> Result<()> {
    let metadata = tx.query_opt(
        "SELECT metadata_json FROM cloud_context_artifacts
          WHERE account_id = $1 AND artifact_id = $2",
        &[&upload.account_id, &upload.logical_id],
    )?;
    let Some(metadata) = metadata else {
        return Ok(());
    };
    let metadata: String = metadata.try_get(0)?;
    tx.execute(
        "UPDATE cloud_context_artifacts SET metadata_json = $3
          WHERE account_id = $1 AND artifact_id = $2",
        &[
            &upload.account_id,
            &upload.logical_id,
            &with_object_metadata(&metadata, upload),
        ],
    )?;
    Ok(())
}

fn with_object_metadata(metadata: &str, upload: &ObjectUpload) -> String {
    let mut value = serde_json::from_str::<serde_json::Value>(metadata)
        .unwrap_or_else(|_| serde_json::json!({}));
    let Some(map) = value.as_object_mut() else {
        return serde_json::json!({
            "object_key": upload.object_key,
            "object_size_bytes": upload.size_bytes,
            "object_sha256": upload.sha256,
            "object_content_type": upload.content_type,
            "object_expires_at_ms": upload.expires_at_ms,
        })
        .to_string();
    };
    map.insert("object_key".into(), upload.object_key.clone().into());
    map.insert("object_size_bytes".into(), upload.size_bytes.into());
    map.insert("object_sha256".into(), upload.sha256.clone().into());
    map.insert(
        "object_content_type".into(),
        upload.content_type.clone().into(),
    );
    map.insert("object_expires_at_ms".into(), upload.expires_at_ms.into());
    value.to_string()
}

fn record_put_failure_sqlite(
    pool: &DbPool,
    upload_id: &str,
    error: &str,
    now_ms: i64,
) -> Result<()> {
    let mut conn = pool.get()?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let attempts = outbox_attempts_sqlite(&tx, upload_id, "put")?;
    let changed = tx.execute(
        "UPDATE object_storage_outbox
            SET state = 'retry', attempt_count = ?2, next_attempt_at_ms = ?3,
                last_error = ?4, updated_at_ms = ?5
          WHERE upload_id = ?1 AND operation = 'put' AND state <> 'completed'",
        params![
            upload_id,
            attempts,
            retry_at_ms(now_ms, attempts),
            bounded_error(error),
            now_ms
        ],
    )?;
    if changed == 0 {
        return Err(UploadControlError::UploadNotFound.into());
    }
    tx.execute(
        "UPDATE object_uploads SET updated_at_ms = ?2 WHERE id = ?1 AND state = 'pending'",
        params![upload_id, now_ms],
    )?;
    tx.commit()?;
    Ok(())
}

fn record_put_failure_postgres(
    pool: &DbPool,
    upload_id: &str,
    error: &str,
    now_ms: i64,
) -> Result<()> {
    let mut conn = pool.get_pg()?;
    let mut tx = conn.transaction()?;
    let attempts: i32 = tx
        .query_opt(
            "SELECT attempt_count FROM object_storage_outbox
              WHERE upload_id = $1 AND operation = 'put' FOR UPDATE",
            &[&upload_id],
        )?
        .map(|row| row.get(0))
        .ok_or(UploadControlError::UploadNotFound)?;
    let changed = tx.execute(
        "UPDATE object_storage_outbox
            SET state = 'retry', attempt_count = $2, next_attempt_at_ms = $3,
                last_error = $4, updated_at_ms = $5
          WHERE upload_id = $1 AND operation = 'put' AND state <> 'completed'",
        &[
            &upload_id,
            &attempts,
            &retry_at_ms(now_ms, attempts),
            &bounded_error(error),
            &now_ms,
        ],
    )?;
    if changed == 0 {
        return Err(UploadControlError::UploadNotFound.into());
    }
    tx.execute(
        "UPDATE object_uploads SET updated_at_ms = $2 WHERE id = $1 AND state = 'pending'",
        &[&upload_id, &now_ms],
    )?;
    tx.commit()?;
    Ok(())
}

fn ready_artifact_sqlite(
    pool: &DbPool,
    account_id: &str,
    artifact_id: &str,
) -> Result<Option<ObjectUpload>> {
    let conn = pool.get()?;
    conn.query_row(
        &format!(
            "SELECT {UPLOAD_COLUMNS} FROM object_uploads
              WHERE account_id = ?1 AND object_kind = 'artifact' AND logical_id = ?2
                AND state = 'ready'"
        ),
        params![account_id, artifact_id],
        row_to_upload_sqlite,
    )
    .optional()
    .map_err(Into::into)
}

fn ready_artifact_postgres(
    pool: &DbPool,
    account_id: &str,
    artifact_id: &str,
) -> Result<Option<ObjectUpload>> {
    let mut conn = pool.get_pg()?;
    conn.query_opt(
        &format!(
            "SELECT {UPLOAD_COLUMNS} FROM object_uploads
              WHERE account_id = $1 AND object_kind = 'artifact' AND logical_id = $2
                AND state = 'ready'"
        ),
        &[&account_id, &artifact_id],
    )?
    .map(row_to_upload_postgres)
    .transpose()
}

fn artifact_upload_sqlite(
    pool: &DbPool,
    account_id: &str,
    artifact_id: &str,
) -> Result<Option<ObjectUpload>> {
    let conn = pool.get()?;
    conn.query_row(
        &format!(
            "SELECT {UPLOAD_COLUMNS} FROM object_uploads
              WHERE account_id = ?1 AND object_kind = 'artifact' AND logical_id = ?2"
        ),
        params![account_id, artifact_id],
        row_to_upload_sqlite,
    )
    .optional()
    .map_err(Into::into)
}

fn artifact_upload_postgres(
    pool: &DbPool,
    account_id: &str,
    artifact_id: &str,
) -> Result<Option<ObjectUpload>> {
    let mut conn = pool.get_pg()?;
    conn.query_opt(
        &format!(
            "SELECT {UPLOAD_COLUMNS} FROM object_uploads
              WHERE account_id = $1 AND object_kind = 'artifact' AND logical_id = $2"
        ),
        &[&account_id, &artifact_id],
    )?
    .map(row_to_upload_postgres)
    .transpose()
}

fn claim_cleanup_jobs_sqlite(
    pool: &DbPool,
    account_id: &str,
    storage_scope: StorageScope,
    now_ms: i64,
    stale_pending_before_ms: i64,
    limit: i64,
) -> Result<Vec<CleanupJob>> {
    let mut conn = pool.get()?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    schedule_due_cleanup_sqlite(
        &tx,
        account_id,
        storage_scope,
        now_ms,
        stale_pending_before_ms,
    )?;
    tx.execute(
        "UPDATE object_storage_outbox
            SET state = 'retry', next_attempt_at_ms = ?2, updated_at_ms = ?2
          WHERE operation = 'delete' AND state = 'processing' AND updated_at_ms <= ?1",
        params![now_ms.saturating_sub(PROCESSING_LEASE_MS), now_ms],
    )?;
    let mut stmt = tx.prepare(
        "SELECT o.upload_id, u.account_id, u.object_key
           FROM object_storage_outbox o
           JOIN object_uploads u ON u.id = o.upload_id
          WHERE o.account_id = ?1 AND u.storage_scope = ?2
            AND o.operation = 'delete' AND o.state IN ('pending', 'retry')
            AND o.next_attempt_at_ms <= ?3
          ORDER BY o.created_at_ms, o.id
          LIMIT ?4",
    )?;
    let rows = stmt.query_map(
        params![account_id, storage_scope.as_str(), now_ms, limit],
        |row| {
            Ok(CleanupJob {
                upload_id: row.get(0)?,
                account_id: row.get(1)?,
                object_key: row.get(2)?,
            })
        },
    )?;
    let jobs = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    drop(stmt);
    for job in &jobs {
        tx.execute(
            "UPDATE object_storage_outbox
                SET state = 'processing', attempt_count = attempt_count + 1,
                    updated_at_ms = ?2
              WHERE upload_id = ?1 AND operation = 'delete'",
            params![job.upload_id, now_ms],
        )?;
    }
    tx.commit()?;
    Ok(jobs)
}

fn claim_cleanup_jobs_postgres(
    pool: &DbPool,
    account_id: &str,
    storage_scope: StorageScope,
    now_ms: i64,
    stale_pending_before_ms: i64,
    limit: i64,
) -> Result<Vec<CleanupJob>> {
    let mut conn = pool.get_pg()?;
    let mut tx = conn.transaction()?;
    schedule_due_cleanup_postgres(
        &mut tx,
        account_id,
        storage_scope,
        now_ms,
        stale_pending_before_ms,
    )?;
    tx.execute(
        "UPDATE object_storage_outbox
            SET state = 'retry', next_attempt_at_ms = $2, updated_at_ms = $2
          WHERE operation = 'delete' AND state = 'processing' AND updated_at_ms <= $1",
        &[&now_ms.saturating_sub(PROCESSING_LEASE_MS), &now_ms],
    )?;
    let rows = tx.query(
        "SELECT o.upload_id, u.account_id, u.object_key
           FROM object_storage_outbox o
           JOIN object_uploads u ON u.id = o.upload_id
          WHERE o.account_id = $1 AND u.storage_scope = $2
            AND o.operation = 'delete' AND o.state IN ('pending', 'retry')
            AND o.next_attempt_at_ms <= $3
          ORDER BY o.created_at_ms, o.id
          FOR UPDATE OF o SKIP LOCKED
          LIMIT $4",
        &[&account_id, &storage_scope.as_str(), &now_ms, &limit],
    )?;
    let jobs = rows
        .iter()
        .map(|row| CleanupJob {
            upload_id: row.get(0),
            account_id: row.get(1),
            object_key: row.get(2),
        })
        .collect::<Vec<_>>();
    for job in &jobs {
        tx.execute(
            "UPDATE object_storage_outbox
                SET state = 'processing', attempt_count = attempt_count + 1,
                    updated_at_ms = $2
              WHERE upload_id = $1 AND operation = 'delete'",
            &[&job.upload_id, &now_ms],
        )?;
    }
    tx.commit()?;
    Ok(jobs)
}

fn claim_global_cleanup_jobs_sqlite(
    pool: &DbPool,
    storage_scope: StorageScope,
    now_ms: i64,
    stale_pending_before_ms: i64,
    limit: i64,
) -> Result<Vec<CleanupJob>> {
    let mut conn = pool.get()?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    tx.execute(
        "UPDATE object_uploads
            SET state = 'delete_pending', updated_at_ms = ?3
          WHERE storage_scope = ?1
            AND ((state = 'ready' AND expires_at_ms <= ?3)
              OR (state = 'pending' AND updated_at_ms <= ?2))",
        params![storage_scope.as_str(), stale_pending_before_ms, now_ms],
    )?;
    tx.execute(
        "INSERT INTO object_storage_outbox (
            id, upload_id, account_id, operation, state, next_attempt_at_ms,
            created_at_ms, updated_at_ms
         )
         SELECT u.id || ':delete', u.id, u.account_id, 'delete', 'pending', ?2, ?2, ?2
           FROM object_uploads u
          WHERE u.storage_scope = ?1 AND u.state = 'delete_pending'
         ON CONFLICT(upload_id, operation) DO NOTHING",
        params![storage_scope.as_str(), now_ms],
    )?;
    tx.execute(
        "UPDATE object_storage_outbox
            SET state = 'abandoned', updated_at_ms = ?2, completed_at_ms = ?2
          WHERE operation = 'put' AND state <> 'completed'
            AND upload_id IN (
                SELECT id FROM object_uploads
                 WHERE storage_scope = ?1 AND state = 'delete_pending'
            )",
        params![storage_scope.as_str(), now_ms],
    )?;
    tx.execute(
        "UPDATE object_storage_outbox
            SET state = 'retry', next_attempt_at_ms = ?2, updated_at_ms = ?2
          WHERE operation = 'delete' AND state = 'processing' AND updated_at_ms <= ?1",
        params![now_ms.saturating_sub(PROCESSING_LEASE_MS), now_ms],
    )?;
    let mut stmt = tx.prepare(
        "SELECT o.upload_id, u.account_id, u.object_key
           FROM object_storage_outbox o
           JOIN object_uploads u ON u.id = o.upload_id
          WHERE u.storage_scope = ?1
            AND o.operation = 'delete' AND o.state IN ('pending', 'retry')
            AND o.next_attempt_at_ms <= ?2
          ORDER BY o.created_at_ms, o.id
          LIMIT ?3",
    )?;
    let rows = stmt.query_map(params![storage_scope.as_str(), now_ms, limit], |row| {
        Ok(CleanupJob {
            upload_id: row.get(0)?,
            account_id: row.get(1)?,
            object_key: row.get(2)?,
        })
    })?;
    let jobs = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    drop(stmt);
    for job in &jobs {
        tx.execute(
            "UPDATE object_storage_outbox
                SET state = 'processing', attempt_count = attempt_count + 1,
                    updated_at_ms = ?2
              WHERE upload_id = ?1 AND operation = 'delete'",
            params![job.upload_id, now_ms],
        )?;
    }
    tx.commit()?;
    Ok(jobs)
}

fn claim_global_cleanup_jobs_postgres(
    pool: &DbPool,
    storage_scope: StorageScope,
    now_ms: i64,
    stale_pending_before_ms: i64,
    limit: i64,
) -> Result<Vec<CleanupJob>> {
    let mut conn = pool.get_pg()?;
    let mut tx = conn.transaction()?;
    tx.execute(
        "UPDATE object_uploads
            SET state = 'delete_pending', updated_at_ms = $3
          WHERE storage_scope = $1
            AND ((state = 'ready' AND expires_at_ms <= $3)
              OR (state = 'pending' AND updated_at_ms <= $2))",
        &[&storage_scope.as_str(), &stale_pending_before_ms, &now_ms],
    )?;
    tx.execute(
        "INSERT INTO object_storage_outbox (
            id, upload_id, account_id, operation, state, next_attempt_at_ms,
            created_at_ms, updated_at_ms
         )
         SELECT u.id || ':delete', u.id, u.account_id, 'delete', 'pending', $2, $2, $2
           FROM object_uploads u
          WHERE u.storage_scope = $1 AND u.state = 'delete_pending'
         ON CONFLICT(upload_id, operation) DO NOTHING",
        &[&storage_scope.as_str(), &now_ms],
    )?;
    tx.execute(
        "UPDATE object_storage_outbox
            SET state = 'abandoned', updated_at_ms = $2, completed_at_ms = $2
          WHERE operation = 'put' AND state <> 'completed'
            AND upload_id IN (
                SELECT id FROM object_uploads
                 WHERE storage_scope = $1 AND state = 'delete_pending'
            )",
        &[&storage_scope.as_str(), &now_ms],
    )?;
    tx.execute(
        "UPDATE object_storage_outbox
            SET state = 'retry', next_attempt_at_ms = $2, updated_at_ms = $2
          WHERE operation = 'delete' AND state = 'processing' AND updated_at_ms <= $1",
        &[&now_ms.saturating_sub(PROCESSING_LEASE_MS), &now_ms],
    )?;
    let rows = tx.query(
        "SELECT o.upload_id, u.account_id, u.object_key
           FROM object_storage_outbox o
           JOIN object_uploads u ON u.id = o.upload_id
          WHERE u.storage_scope = $1
            AND o.operation = 'delete' AND o.state IN ('pending', 'retry')
            AND o.next_attempt_at_ms <= $2
          ORDER BY o.created_at_ms, o.id
          FOR UPDATE OF o SKIP LOCKED
          LIMIT $3",
        &[&storage_scope.as_str(), &now_ms, &limit],
    )?;
    let jobs = rows
        .iter()
        .map(|row| CleanupJob {
            upload_id: row.get(0),
            account_id: row.get(1),
            object_key: row.get(2),
        })
        .collect::<Vec<_>>();
    for job in &jobs {
        tx.execute(
            "UPDATE object_storage_outbox
                SET state = 'processing', attempt_count = attempt_count + 1,
                    updated_at_ms = $2
              WHERE upload_id = $1 AND operation = 'delete'",
            &[&job.upload_id, &now_ms],
        )?;
    }
    tx.commit()?;
    Ok(jobs)
}

fn schedule_due_cleanup_sqlite(
    tx: &SqliteTransaction<'_>,
    account_id: &str,
    storage_scope: StorageScope,
    now_ms: i64,
    stale_pending_before_ms: i64,
) -> Result<()> {
    tx.execute(
        "UPDATE object_uploads
            SET state = 'delete_pending', updated_at_ms = ?4
          WHERE account_id = ?1 AND storage_scope = ?2
            AND ((state = 'ready' AND expires_at_ms <= ?4)
              OR (state = 'pending' AND updated_at_ms <= ?3))",
        params![
            account_id,
            storage_scope.as_str(),
            stale_pending_before_ms,
            now_ms
        ],
    )?;
    enqueue_scope_deletes_sqlite(tx, account_id, storage_scope, now_ms)
}

fn schedule_due_cleanup_postgres(
    tx: &mut PgTransaction<'_>,
    account_id: &str,
    storage_scope: StorageScope,
    now_ms: i64,
    stale_pending_before_ms: i64,
) -> Result<()> {
    tx.execute(
        "UPDATE object_uploads
            SET state = 'delete_pending', updated_at_ms = $4
          WHERE account_id = $1 AND storage_scope = $2
            AND ((state = 'ready' AND expires_at_ms <= $4)
              OR (state = 'pending' AND updated_at_ms <= $3))",
        &[
            &account_id,
            &storage_scope.as_str(),
            &stale_pending_before_ms,
            &now_ms,
        ],
    )?;
    enqueue_scope_deletes_postgres(tx, account_id, storage_scope, now_ms)
}

fn enqueue_scope_deletes_sqlite(
    tx: &SqliteTransaction<'_>,
    account_id: &str,
    storage_scope: StorageScope,
    now_ms: i64,
) -> Result<()> {
    tx.execute(
        "INSERT INTO object_storage_outbox (
            id, upload_id, account_id, operation, state, next_attempt_at_ms,
            created_at_ms, updated_at_ms
         )
         SELECT u.id || ':delete', u.id, u.account_id, 'delete', 'pending', ?3, ?3, ?3
           FROM object_uploads u
          WHERE u.account_id = ?1 AND u.storage_scope = ?2 AND u.state = 'delete_pending'
         ON CONFLICT(upload_id, operation) DO NOTHING",
        params![account_id, storage_scope.as_str(), now_ms],
    )?;
    abandon_pending_puts_sqlite(tx, account_id, None, now_ms)
}

fn enqueue_scope_deletes_postgres(
    tx: &mut PgTransaction<'_>,
    account_id: &str,
    storage_scope: StorageScope,
    now_ms: i64,
) -> Result<()> {
    tx.execute(
        "INSERT INTO object_storage_outbox (
            id, upload_id, account_id, operation, state, next_attempt_at_ms,
            created_at_ms, updated_at_ms
         )
         SELECT u.id || ':delete', u.id, u.account_id, 'delete', 'pending', $3, $3, $3
           FROM object_uploads u
          WHERE u.account_id = $1 AND u.storage_scope = $2 AND u.state = 'delete_pending'
         ON CONFLICT(upload_id, operation) DO NOTHING",
        &[&account_id, &storage_scope.as_str(), &now_ms],
    )?;
    abandon_pending_puts_postgres(tx, account_id, None, now_ms)
}

fn enqueue_session_deletes_sqlite(
    tx: &SqliteTransaction<'_>,
    account_id: &str,
    session_id: &str,
    now_ms: i64,
) -> Result<()> {
    tx.execute(
        "INSERT INTO object_storage_outbox (
            id, upload_id, account_id, operation, state, next_attempt_at_ms,
            created_at_ms, updated_at_ms
         )
         SELECT u.id || ':delete', u.id, u.account_id, 'delete', 'pending', ?3, ?3, ?3
           FROM object_uploads u
          WHERE u.account_id = ?1 AND u.session_id = ?2 AND u.state = 'delete_pending'
         ON CONFLICT(upload_id, operation) DO NOTHING",
        params![account_id, session_id, now_ms],
    )?;
    abandon_pending_puts_sqlite(tx, account_id, Some(session_id), now_ms)
}

fn enqueue_session_deletes_postgres(
    tx: &mut PgTransaction<'_>,
    account_id: &str,
    session_id: &str,
    now_ms: i64,
) -> Result<()> {
    tx.execute(
        "INSERT INTO object_storage_outbox (
            id, upload_id, account_id, operation, state, next_attempt_at_ms,
            created_at_ms, updated_at_ms
         )
         SELECT u.id || ':delete', u.id, u.account_id, 'delete', 'pending', $3, $3, $3
           FROM object_uploads u
          WHERE u.account_id = $1 AND u.session_id = $2 AND u.state = 'delete_pending'
         ON CONFLICT(upload_id, operation) DO NOTHING",
        &[&account_id, &session_id, &now_ms],
    )?;
    abandon_pending_puts_postgres(tx, account_id, Some(session_id), now_ms)
}

fn abandon_pending_puts_sqlite(
    tx: &SqliteTransaction<'_>,
    account_id: &str,
    session_id: Option<&str>,
    now_ms: i64,
) -> Result<()> {
    tx.execute(
        "UPDATE object_storage_outbox
            SET state = 'abandoned', updated_at_ms = ?3, completed_at_ms = ?3
          WHERE account_id = ?1 AND operation = 'put' AND state <> 'completed'
            AND upload_id IN (
                SELECT id FROM object_uploads
                 WHERE account_id = ?1 AND state = 'delete_pending'
                   AND (?2 IS NULL OR session_id = ?2)
            )",
        params![account_id, session_id, now_ms],
    )?;
    Ok(())
}

fn abandon_pending_puts_postgres(
    tx: &mut PgTransaction<'_>,
    account_id: &str,
    session_id: Option<&str>,
    now_ms: i64,
) -> Result<()> {
    tx.execute(
        "UPDATE object_storage_outbox
            SET state = 'abandoned', updated_at_ms = $3, completed_at_ms = $3
          WHERE account_id = $1 AND operation = 'put' AND state <> 'completed'
            AND upload_id IN (
                SELECT id FROM object_uploads
                 WHERE account_id = $1 AND state = 'delete_pending'
                   AND ($2::text IS NULL OR session_id = $2)
            )",
        &[&account_id, &session_id, &now_ms],
    )?;
    Ok(())
}

fn mark_cleanup_succeeded_sqlite(pool: &DbPool, upload_id: &str, now_ms: i64) -> Result<()> {
    let mut conn = pool.get()?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let upload = load_upload_sqlite(&tx, upload_id)?.ok_or(UploadControlError::UploadNotFound)?;
    tx.execute(
        "UPDATE object_uploads
            SET state = 'deleted', deleted_at_ms = ?2, updated_at_ms = ?2
          WHERE id = ?1",
        params![upload_id, now_ms],
    )?;
    tx.execute(
        "UPDATE object_storage_outbox
            SET state = 'completed', completed_at_ms = ?2, updated_at_ms = ?2,
                next_attempt_at_ms = ?2, last_error = NULL
          WHERE upload_id = ?1 AND operation = 'delete'",
        params![upload_id, now_ms],
    )?;
    remove_published_index_sqlite(&tx, &upload)?;
    tx.commit()?;
    Ok(())
}

fn mark_cleanup_succeeded_postgres(pool: &DbPool, upload_id: &str, now_ms: i64) -> Result<()> {
    let mut conn = pool.get_pg()?;
    let mut tx = conn.transaction()?;
    let upload =
        load_upload_postgres(&mut tx, upload_id)?.ok_or(UploadControlError::UploadNotFound)?;
    tx.execute(
        "UPDATE object_uploads
            SET state = 'deleted', deleted_at_ms = $2, updated_at_ms = $2
          WHERE id = $1",
        &[&upload_id, &now_ms],
    )?;
    tx.execute(
        "UPDATE object_storage_outbox
            SET state = 'completed', completed_at_ms = $2, updated_at_ms = $2,
                next_attempt_at_ms = $2, last_error = NULL
          WHERE upload_id = $1 AND operation = 'delete'",
        &[&upload_id, &now_ms],
    )?;
    remove_published_index_postgres(&mut tx, &upload)?;
    tx.commit()?;
    Ok(())
}

fn remove_published_index_sqlite(tx: &SqliteTransaction<'_>, upload: &ObjectUpload) -> Result<()> {
    if upload.object_kind == ObjectKind::SessionAudit.as_str() {
        tx.execute(
            "DELETE FROM diagnostic_log_chunks WHERE id = ?1",
            params![diagnostic_id(upload)],
        )?;
    } else {
        clear_artifact_metadata_sqlite(tx, upload)?;
    }
    Ok(())
}

fn remove_published_index_postgres(
    tx: &mut PgTransaction<'_>,
    upload: &ObjectUpload,
) -> Result<()> {
    if upload.object_kind == ObjectKind::SessionAudit.as_str() {
        tx.execute(
            "DELETE FROM diagnostic_log_chunks WHERE id = $1",
            &[&diagnostic_id(upload)],
        )?;
    } else {
        clear_artifact_metadata_postgres(tx, upload)?;
    }
    Ok(())
}

fn clear_artifact_metadata_sqlite(tx: &SqliteTransaction<'_>, upload: &ObjectUpload) -> Result<()> {
    let metadata = tx
        .query_row(
            "SELECT metadata_json FROM cloud_context_artifacts
              WHERE account_id = ?1 AND artifact_id = ?2",
            params![upload.account_id, upload.logical_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    if let Some(metadata) = metadata {
        tx.execute(
            "UPDATE cloud_context_artifacts SET metadata_json = ?3
              WHERE account_id = ?1 AND artifact_id = ?2",
            params![
                upload.account_id,
                upload.logical_id,
                without_object_metadata(&metadata)
            ],
        )?;
    }
    Ok(())
}

fn clear_artifact_metadata_postgres(
    tx: &mut PgTransaction<'_>,
    upload: &ObjectUpload,
) -> Result<()> {
    let metadata = tx.query_opt(
        "SELECT metadata_json FROM cloud_context_artifacts
          WHERE account_id = $1 AND artifact_id = $2",
        &[&upload.account_id, &upload.logical_id],
    )?;
    if let Some(metadata) = metadata {
        let metadata: String = metadata.try_get(0)?;
        tx.execute(
            "UPDATE cloud_context_artifacts SET metadata_json = $3
              WHERE account_id = $1 AND artifact_id = $2",
            &[
                &upload.account_id,
                &upload.logical_id,
                &without_object_metadata(&metadata),
            ],
        )?;
    }
    Ok(())
}

fn without_object_metadata(metadata: &str) -> String {
    let mut value = serde_json::from_str::<serde_json::Value>(metadata)
        .unwrap_or_else(|_| serde_json::json!({}));
    if let Some(map) = value.as_object_mut() {
        for key in [
            "object_key",
            "object_size_bytes",
            "object_sha256",
            "object_content_type",
            "object_expires_at_ms",
        ] {
            map.remove(key);
        }
    }
    value.to_string()
}

fn mark_cleanup_failed_sqlite(
    pool: &DbPool,
    upload_id: &str,
    error: &str,
    now_ms: i64,
) -> Result<()> {
    let conn = pool.get()?;
    let attempts = outbox_attempts_sqlite(&conn, upload_id, "delete")?;
    let changed = conn.execute(
        "UPDATE object_storage_outbox
            SET state = 'retry', next_attempt_at_ms = ?2, last_error = ?3,
                updated_at_ms = ?4
          WHERE upload_id = ?1 AND operation = 'delete'",
        params![
            upload_id,
            retry_at_ms(now_ms, attempts.max(1)),
            bounded_error(error),
            now_ms
        ],
    )?;
    if changed == 0 {
        return Err(UploadControlError::UploadNotFound.into());
    }
    Ok(())
}

fn mark_cleanup_failed_postgres(
    pool: &DbPool,
    upload_id: &str,
    error: &str,
    now_ms: i64,
) -> Result<()> {
    let mut conn = pool.get_pg()?;
    let attempts = outbox_attempts_postgres(&mut conn, upload_id, "delete")?;
    let changed = conn.execute(
        "UPDATE object_storage_outbox
            SET state = 'retry', next_attempt_at_ms = $2, last_error = $3,
                updated_at_ms = $4
          WHERE upload_id = $1 AND operation = 'delete'",
        &[
            &upload_id,
            &retry_at_ms(now_ms, attempts.max(1)),
            &bounded_error(error),
            &now_ms,
        ],
    )?;
    if changed == 0 {
        return Err(UploadControlError::UploadNotFound.into());
    }
    Ok(())
}

const UPLOAD_COLUMNS: &str = "id, account_id, object_kind, logical_id, session_id,
    storage_scope, object_key, size_bytes, sha256, content_type, expires_at_ms,
    state, metadata_json, created_at_ms, updated_at_ms, uploaded_at_ms, deleted_at_ms";

fn load_upload_sqlite(tx: &SqliteTransaction<'_>, upload_id: &str) -> Result<Option<ObjectUpload>> {
    tx.query_row(
        &format!("SELECT {UPLOAD_COLUMNS} FROM object_uploads WHERE id = ?1"),
        params![upload_id],
        row_to_upload_sqlite,
    )
    .optional()
    .map_err(Into::into)
}

fn load_upload_postgres(
    tx: &mut PgTransaction<'_>,
    upload_id: &str,
) -> Result<Option<ObjectUpload>> {
    tx.query_opt(
        &format!("SELECT {UPLOAD_COLUMNS} FROM object_uploads WHERE id = $1 FOR UPDATE"),
        &[&upload_id],
    )?
    .map(row_to_upload_postgres)
    .transpose()
}

fn load_logical_upload_sqlite(
    tx: &SqliteTransaction<'_>,
    account_id: &str,
    object_kind: &str,
    logical_id: &str,
) -> Result<Option<ObjectUpload>> {
    tx.query_row(
        &format!(
            "SELECT {UPLOAD_COLUMNS} FROM object_uploads
              WHERE account_id = ?1 AND object_kind = ?2 AND logical_id = ?3"
        ),
        params![account_id, object_kind, logical_id],
        row_to_upload_sqlite,
    )
    .optional()
    .map_err(Into::into)
}

fn load_logical_upload_postgres(
    tx: &mut PgTransaction<'_>,
    account_id: &str,
    object_kind: &str,
    logical_id: &str,
) -> Result<Option<ObjectUpload>> {
    tx.query_opt(
        &format!(
            "SELECT {UPLOAD_COLUMNS} FROM object_uploads
              WHERE account_id = $1 AND object_kind = $2 AND logical_id = $3
              FOR UPDATE"
        ),
        &[&account_id, &object_kind, &logical_id],
    )?
    .map(row_to_upload_postgres)
    .transpose()
}

fn row_to_upload_sqlite(row: &rusqlite::Row<'_>) -> rusqlite::Result<ObjectUpload> {
    Ok(ObjectUpload {
        id: row.get(0)?,
        account_id: row.get(1)?,
        object_kind: row.get(2)?,
        logical_id: row.get(3)?,
        session_id: row.get(4)?,
        storage_scope: row.get(5)?,
        object_key: row.get(6)?,
        size_bytes: row.get(7)?,
        sha256: row.get(8)?,
        content_type: row.get(9)?,
        expires_at_ms: row.get(10)?,
        state: row.get(11)?,
        metadata_json: row.get(12)?,
        created_at_ms: row.get(13)?,
        updated_at_ms: row.get(14)?,
        uploaded_at_ms: row.get(15)?,
        deleted_at_ms: row.get(16)?,
    })
}

fn row_to_upload_postgres(row: PgRow) -> Result<ObjectUpload> {
    Ok(ObjectUpload {
        id: row.try_get(0)?,
        account_id: row.try_get(1)?,
        object_kind: row.try_get(2)?,
        logical_id: row.try_get(3)?,
        session_id: row.try_get(4)?,
        storage_scope: row.try_get(5)?,
        object_key: row.try_get(6)?,
        size_bytes: row.try_get(7)?,
        sha256: row.try_get(8)?,
        content_type: row.try_get(9)?,
        expires_at_ms: row.try_get(10)?,
        state: row.try_get(11)?,
        metadata_json: row.try_get(12)?,
        created_at_ms: row.try_get(13)?,
        updated_at_ms: row.try_get(14)?,
        uploaded_at_ms: row.try_get(15)?,
        deleted_at_ms: row.try_get(16)?,
    })
}

fn outbox_attempts_sqlite(
    conn: &rusqlite::Connection,
    upload_id: &str,
    operation: &str,
) -> Result<i32> {
    conn.query_row(
        "SELECT attempt_count FROM object_storage_outbox
          WHERE upload_id = ?1 AND operation = ?2",
        params![upload_id, operation],
        |row| row.get(0),
    )
    .optional()?
    .ok_or_else(|| UploadControlError::UploadNotFound.into())
}

fn outbox_attempts_postgres(
    conn: &mut SafePostgresClient,
    upload_id: &str,
    operation: &str,
) -> Result<i32> {
    conn.query_opt(
        "SELECT attempt_count FROM object_storage_outbox
          WHERE upload_id = $1 AND operation = $2",
        &[&upload_id, &operation],
    )?
    .map(|row| row.get(0))
    .ok_or_else(|| UploadControlError::UploadNotFound.into())
}

fn stable_upload_id(input: &NewObjectUpload) -> String {
    sha256_hex(format!(
        "{}\0{}\0{}",
        input.account_id,
        input.object_kind.as_str(),
        input.logical_id
    ))
}

fn outbox_id(upload_id: &str, operation: &str) -> String {
    format!("{upload_id}:{operation}")
}

fn diagnostic_id(upload: &ObjectUpload) -> String {
    format!("session-audit:{}", upload.id)
}

fn day_start_ms(now_ms: i64) -> i64 {
    now_ms.saturating_sub(now_ms.rem_euclid(DAY_MS))
}

fn retry_at_ms(now_ms: i64, attempt_count: i32) -> i64 {
    let shift = attempt_count.saturating_sub(1).clamp(0, 9) as u32;
    let delay_ms = 5_000_i64.saturating_mul(1_i64 << shift);
    now_ms.saturating_add(delay_ms.min(60 * 60 * 1000))
}

fn bounded_error(error: &str) -> String {
    error
        .replace('\0', "")
        .chars()
        .take(512)
        .collect::<String>()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{open_pool, run_migrations};

    fn test_pool() -> DbPool {
        let pool = open_pool(":memory:".as_ref()).unwrap();
        run_migrations(&pool).unwrap();
        let conn = pool.get().unwrap();
        for (id, email) in [
            ("acct_1", "one@example.test"),
            ("acct_2", "two@example.test"),
        ] {
            conn.execute(
                "INSERT INTO accounts(id, email, password_hash) VALUES (?1, ?2, 'hash')",
                params![id, email],
            )
            .unwrap();
        }
        conn.execute(
            "INSERT INTO cloud_sessions (
                account_id, session_id, title, status, created_at_ms, updated_at_ms, metadata_json
             ) VALUES ('acct_1', 'session_1', 'Session', 'active', 1, 1, '{}')",
            [],
        )
        .unwrap();
        drop(conn);
        pool
    }

    fn artifact_input(logical_id: &str, size_bytes: i64, now_ms: i64) -> NewObjectUpload {
        let hash = sha256_hex(format!("payload-{logical_id}"));
        NewObjectUpload {
            account_id: "acct_1".into(),
            object_kind: ObjectKind::Artifact,
            logical_id: logical_id.into(),
            session_id: None,
            storage_scope: StorageScope::Artifact,
            object_key: format!("objects/accounts/acct_1/{logical_id}/{hash}"),
            size_bytes,
            sha256: hash,
            content_type: "application/octet-stream".into(),
            expires_at_ms: now_ms + DAY_MS,
            metadata_json: serde_json::json!({}),
            now_ms,
            limits: UploadLimits {
                max_object_bytes: 100,
                max_account_bytes: 150,
                max_daily_bytes: 120,
                max_account_objects: 10,
            },
        }
    }

    #[test]
    fn reservation_is_idempotent_and_conflicting_content_is_rejected() {
        let pool = test_pool();
        let input = artifact_input("artifact_1", 40, 1_000);
        let first = reserve_upload(&pool, &input).unwrap();
        assert!(first.needs_put);
        assert_eq!(first.upload.state, "pending");
        let account_refs = crate::db::account_data::artifact_object_refs(&pool, "acct_1").unwrap();
        assert_eq!(account_refs.len(), 1);
        assert_eq!(account_refs[0].object_key, input.object_key);
        let concurrent = reserve_upload(&pool, &input).unwrap_err();
        assert_eq!(
            concurrent.downcast_ref::<UploadControlError>(),
            Some(&UploadControlError::UploadInProgress)
        );
        let ready = mark_upload_ready(&pool, &first.upload.id, 1_100).unwrap();
        assert_eq!(ready.state, "ready");

        let retry = reserve_upload(&pool, &input).unwrap();
        assert!(!retry.needs_put);
        assert_eq!(retry.upload.id, first.upload.id);
        let usage: (i64, i64) = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT reserved_bytes, reserved_objects FROM object_upload_daily_usage
                  WHERE account_id = 'acct_1'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(usage, (40, 1), "idempotent retries do not consume quota");

        let mut conflict = input;
        conflict.sha256 = "f".repeat(64);
        let error = reserve_upload(&pool, &conflict).unwrap_err();
        assert_eq!(
            error.downcast_ref::<UploadControlError>(),
            Some(&UploadControlError::IdempotencyConflict)
        );
    }

    #[test]
    fn quota_reservations_bound_object_daily_and_total_bytes() {
        let pool = test_pool();
        let oversized = artifact_input("too_big", 101, 1_000);
        assert_eq!(
            reserve_upload(&pool, &oversized)
                .unwrap_err()
                .downcast_ref::<UploadControlError>(),
            Some(&UploadControlError::ObjectTooLarge)
        );

        reserve_upload(&pool, &artifact_input("first", 70, 1_000)).unwrap();
        let daily = reserve_upload(&pool, &artifact_input("second", 60, 1_100)).unwrap_err();
        assert_eq!(
            daily.downcast_ref::<UploadControlError>(),
            Some(&UploadControlError::DailyQuotaExceeded)
        );

        let mut total_limited = artifact_input("third", 90, DAY_MS + 1_000);
        total_limited.limits.max_daily_bytes = 200;
        let total = reserve_upload(&pool, &total_limited).unwrap_err();
        assert_eq!(
            total.downcast_ref::<UploadControlError>(),
            Some(&UploadControlError::AccountBytesQuotaExceeded)
        );
    }

    #[test]
    fn audit_reservation_requires_an_owned_live_session_and_publishes_index() {
        let pool = test_pool();
        let now_ms = 1_000;
        let hash = sha256_hex("audit");
        let mut input = NewObjectUpload {
            account_id: "acct_2".into(),
            object_kind: ObjectKind::SessionAudit,
            logical_id: "session_1/bundle_1".into(),
            session_id: Some("session_1".into()),
            storage_scope: StorageScope::Audit,
            object_key: format!("logs/accounts/acct_2/session_1/bundle_1/{hash}"),
            size_bytes: 10,
            sha256: hash,
            content_type: "application/json".into(),
            expires_at_ms: now_ms + DAY_MS,
            metadata_json: serde_json::json!({"bundle_id":"bundle_1"}),
            now_ms,
            limits: UploadLimits {
                max_object_bytes: 100,
                max_account_bytes: 1_000,
                max_daily_bytes: 1_000,
                max_account_objects: 10,
            },
        };
        let error = reserve_upload(&pool, &input).unwrap_err();
        assert_eq!(
            error.downcast_ref::<UploadControlError>(),
            Some(&UploadControlError::SessionNotOwned)
        );

        input.account_id = "acct_1".into();
        input.object_key = input.object_key.replace("acct_2", "acct_1");
        let reservation = reserve_upload(&pool, &input).unwrap();
        let pending_refs =
            crate::db::diagnostic_logs::object_refs_for_account(&pool, "acct_1").unwrap();
        assert_eq!(pending_refs.len(), 1);
        assert_eq!(pending_refs[0].object_key, input.object_key);
        mark_upload_ready(&pool, &reservation.upload.id, 1_100).unwrap();
        assert_eq!(
            crate::db::diagnostic_logs::object_refs_for_account(&pool, "acct_1")
                .unwrap()
                .len(),
            1,
            "published diagnostic and upload ledger references are deduplicated"
        );
        let indexed: (String, String) = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT account_id, session_id FROM diagnostic_log_chunks WHERE object_key = ?1",
                params![input.object_key],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(indexed, ("acct_1".into(), "session_1".into()));
    }

    #[test]
    fn stale_pending_uploads_become_retryable_delete_jobs() {
        let pool = test_pool();
        let input = artifact_input("orphan", 40, 1_000);
        let reservation = reserve_upload(&pool, &input).unwrap();
        record_put_failure(&pool, &reservation.upload.id, "r2 unavailable", 1_100).unwrap();

        let jobs =
            claim_global_cleanup_jobs(&pool, StorageScope::Artifact, 10_000, 2_000, 5).unwrap();
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].upload_id, reservation.upload.id);
        assert_eq!(jobs[0].account_id, "acct_1");
        mark_cleanup_failed(&pool, &jobs[0].upload_id, "delete failed", 10_100).unwrap();

        let state: (String, String, i64) = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT u.state, o.state, o.next_attempt_at_ms
                   FROM object_uploads u
                   JOIN object_storage_outbox o ON o.upload_id = u.id AND o.operation = 'delete'
                  WHERE u.id = ?1",
                params![reservation.upload.id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(state.0, "delete_pending");
        assert_eq!(state.1, "retry");
        assert!(state.2 > 10_100);

        let jobs =
            claim_global_cleanup_jobs(&pool, StorageScope::Artifact, state.2, 2_000, 5).unwrap();
        assert_eq!(jobs.len(), 1);
        mark_cleanup_succeeded(&pool, &jobs[0].upload_id, state.2 + 1).unwrap();
        assert!(ready_artifact(&pool, "acct_1", "orphan").unwrap().is_none());
    }
}
