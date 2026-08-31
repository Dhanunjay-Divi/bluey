//! Durable object metadata, quota reservations, and R2/S3 outbox state.

use anyhow::{Context, Result};
use postgres::{Row as PgRow, Transaction as PgTransaction};
use rusqlite::{params, OptionalExtension, Transaction as SqliteTransaction, TransactionBehavior};

use super::{DbPool, SafePostgresClient};
use crate::object_storage::{sha256_hex, UploadLimits};

const DAY_MS: i64 = 86_400_000;
pub(crate) const PROCESSING_LEASE_MS: i64 = 5 * 60 * 1000;

pub(crate) fn context_artifact_advisory_lock_key(artifact_id: &str) -> String {
    format!("context artifact:{artifact_id}")
}

pub(crate) fn session_advisory_lock_key(session_id: &str) -> String {
    format!("session:{session_id}")
}

fn lock_context_artifact_postgres_tx(tx: &mut PgTransaction<'_>, artifact_id: &str) -> Result<()> {
    let lock_key = context_artifact_advisory_lock_key(artifact_id);
    tx.query(
        "SELECT pg_advisory_xact_lock(hashtextextended($1, 0::bigint))",
        &[&lock_key],
    )?;
    Ok(())
}

fn lock_session_postgres_tx(tx: &mut PgTransaction<'_>, session_id: &str) -> Result<()> {
    let lock_key = session_advisory_lock_key(session_id);
    tx.query(
        "SELECT pg_advisory_xact_lock(hashtextextended($1, 0::bigint))",
        &[&lock_key],
    )?;
    Ok(())
}

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
    #[error("the stable object id is already bound to different content or parent session")]
    IdempotencyConflict,
    #[error("the object upload is already in progress")]
    UploadInProgress,
    #[error("session not found for this account")]
    SessionNotOwned,
    #[error("object upload metadata not found")]
    UploadNotFound,
    #[error("object upload has been deleted")]
    UploadGone,
    #[error("active explicit support-diagnostics consent is required")]
    SupportDiagnosticConsentRequired,
    #[error("invalid object upload metadata: {0}")]
    InvalidMetadata(&'static str),
}

pub fn reserve_upload(pool: &DbPool, input: &NewObjectUpload) -> Result<UploadReservation> {
    validate_input(input)?;
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => reserve_upload_sqlite(pool, input, true),
        DbPool::Postgres(_) => reserve_upload_postgres(pool, input, true),
    })
}

/// Reserve an account-owned object PUT that is not a child of a synced
/// session (for example, a Jobs resume or submission receipt). The durable PUT
/// lease is published before network I/O and shares the account deletion row
/// lock, so deletion either fences the write or waits for its resolution.
pub fn reserve_account_object_put(
    pool: &DbPool,
    input: &NewObjectUpload,
) -> Result<UploadReservation> {
    validate_account_object_input(input)?;
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => reserve_upload_sqlite(pool, input, false),
        DbPool::Postgres(_) => reserve_upload_postgres(pool, input, false),
    })
}

/// Commit a sessionless account object only while the account remains live.
/// A deletion fence published after reservation changes the upload to
/// `delete_pending`; callers must then delete the object and confirm cleanup.
pub fn finalize_account_object_put(
    pool: &DbPool,
    upload_id: &str,
    now_ms: i64,
) -> Result<ObjectUpload> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => finalize_account_object_put_sqlite(pool, upload_id, now_ms),
        DbPool::Postgres(_) => finalize_account_object_put_postgres(pool, upload_id, now_ms),
    })
}

/// Record a confirmed object deletion and close both PUT and DELETE outbox
/// records. This is only for the owner of a completed/failed account-scoped
/// PUT after object storage has acknowledged DELETE.
pub fn confirm_account_object_deleted(
    pool: &DbPool,
    account_id: &str,
    object_key: &str,
    now_ms: i64,
) -> Result<bool> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let upload = load_account_object_by_key_sqlite(&tx, account_id, object_key)?;
            let Some(upload) = upload else {
                tx.commit()?;
                return Ok(false);
            };
            abandon_put_outbox_sqlite(&tx, &upload.id, now_ms)?;
            ensure_delete_outbox_sqlite(&tx, &upload, now_ms)?;
            mark_account_object_deleted_sqlite_tx(&tx, &upload, now_ms)?;
            tx.commit()?;
            Ok(true)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            let identity =
                load_account_object_by_key_postgres_unlocked(&mut tx, account_id, object_key)?;
            let Some(identity) = identity else {
                tx.commit()?;
                return Ok(false);
            };
            if tx
                .query_opt(
                    "SELECT id FROM accounts WHERE id = $1 FOR UPDATE",
                    &[&identity.account_id],
                )?
                .is_none()
            {
                tx.commit()?;
                return Ok(false);
            }
            let upload = load_account_object_by_key_postgres(&mut tx, account_id, object_key)?;
            let Some(upload) = upload else {
                tx.commit()?;
                return Ok(false);
            };
            abandon_put_outbox_postgres(&mut tx, &upload.id, now_ms)?;
            ensure_delete_outbox_postgres(&mut tx, &upload, now_ms)?;
            mark_account_object_deleted_postgres_tx(&mut tx, &upload, now_ms)?;
            tx.commit()?;
            Ok(true)
        }
    })
}

/// Fence a failed account-object PUT for durable worker cleanup when an
/// immediate object-storage DELETE could not be confirmed.
pub fn schedule_account_object_cleanup(
    pool: &DbPool,
    account_id: &str,
    object_key: &str,
    now_ms: i64,
) -> Result<bool> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let upload = load_account_object_by_key_sqlite(&tx, account_id, object_key)?;
            let Some(upload) = upload else {
                tx.commit()?;
                return Ok(false);
            };
            tx.execute(
                "UPDATE object_uploads SET state = 'delete_pending', updated_at_ms = ?2
                  WHERE id = ?1 AND state IN ('pending', 'ready')",
                params![upload.id, now_ms],
            )?;
            abandon_put_outbox_sqlite(&tx, &upload.id, now_ms)?;
            ensure_delete_outbox_sqlite(&tx, &upload, now_ms)?;
            tx.commit()?;
            Ok(true)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            let identity =
                load_account_object_by_key_postgres_unlocked(&mut tx, account_id, object_key)?;
            let Some(identity) = identity else {
                tx.commit()?;
                return Ok(false);
            };
            if tx
                .query_opt(
                    "SELECT id FROM accounts WHERE id = $1 FOR UPDATE",
                    &[&identity.account_id],
                )?
                .is_none()
            {
                tx.commit()?;
                return Ok(false);
            }
            let upload = load_account_object_by_key_postgres(&mut tx, account_id, object_key)?;
            let Some(upload) = upload else {
                tx.commit()?;
                return Ok(false);
            };
            tx.execute(
                "UPDATE object_uploads SET state = 'delete_pending', updated_at_ms = $2
                  WHERE id = $1 AND state IN ('pending', 'ready')",
                &[&upload.id, &now_ms],
            )?;
            abandon_put_outbox_postgres(&mut tx, &upload.id, now_ms)?;
            ensure_delete_outbox_postgres(&mut tx, &upload, now_ms)?;
            tx.commit()?;
            Ok(true)
        }
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

/// Fence every support-diagnostic object in an account, optionally narrowed
/// to one session, and durably enqueue R2 deletion. This never deletes product
/// session data or ordinary synced artifacts.
pub fn schedule_support_diagnostic_cleanup(
    pool: &DbPool,
    account_id: &str,
    session_id: Option<&str>,
    now_ms: i64,
) -> Result<usize> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            if let Some(session_id) = session_id {
                tombstone_support_diagnostic_session_sqlite_tx(
                    &tx, account_id, session_id, now_ms,
                )?;
            } else {
                crate::db::support_diagnostics::ensure_revoked_sqlite_tx(&tx, account_id, now_ms)?;
            }
            let changed =
                schedule_support_diagnostic_cleanup_sqlite_tx(&tx, account_id, session_id, now_ms)?;
            tx.commit()?;
            Ok(changed)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            if let Some(session_id) = session_id {
                lock_session_postgres_tx(&mut tx, session_id)?;
            }
            tx.query_opt(
                "SELECT id FROM accounts WHERE id = $1 FOR UPDATE",
                &[&account_id],
            )?;
            if let Some(session_id) = session_id {
                tombstone_support_diagnostic_session_postgres_tx(
                    &mut tx, account_id, session_id, now_ms,
                )?;
            } else {
                crate::db::support_diagnostics::ensure_revoked_postgres_tx(
                    &mut tx, account_id, now_ms,
                )?;
            }
            let changed = schedule_support_diagnostic_cleanup_postgres_tx(
                &mut tx, account_id, session_id, now_ms,
            )?;
            tx.commit()?;
            Ok(changed)
        }
    })
}

fn tombstone_support_diagnostic_session_sqlite_tx(
    tx: &SqliteTransaction<'_>,
    account_id: &str,
    session_id: &str,
    now_ms: i64,
) -> Result<()> {
    tx.execute(
        "INSERT INTO support_diagnostic_session_tombstones (
            account_id, session_id, deleted_at_ms
         ) VALUES (?1, ?2, ?3)
         ON CONFLICT(account_id, session_id) DO UPDATE SET
            deleted_at_ms = MAX(
                support_diagnostic_session_tombstones.deleted_at_ms,
                excluded.deleted_at_ms
            )",
        params![account_id, session_id, now_ms],
    )?;
    Ok(())
}

fn tombstone_support_diagnostic_session_postgres_tx(
    tx: &mut PgTransaction<'_>,
    account_id: &str,
    session_id: &str,
    now_ms: i64,
) -> Result<()> {
    tx.execute(
        "INSERT INTO support_diagnostic_session_tombstones (
            account_id, session_id, deleted_at_ms
         ) VALUES ($1, $2, $3)
         ON CONFLICT(account_id, session_id) DO UPDATE SET
            deleted_at_ms = GREATEST(
                support_diagnostic_session_tombstones.deleted_at_ms,
                EXCLUDED.deleted_at_ms
            )",
        &[&account_id, &session_id, &now_ms],
    )?;
    Ok(())
}

/// Fence every account-scoped upload before GDPR object discovery starts.
/// Fresh `processing` PUT leases remain visible so hard deletion must wait;
/// retries and leases older than the storage request bound are abandoned.
pub(crate) fn fence_account_deletion_sqlite_tx(
    tx: &SqliteTransaction<'_>,
    account_id: &str,
    now_ms: i64,
) -> Result<()> {
    crate::db::support_diagnostics::ensure_revoked_sqlite_tx(tx, account_id, now_ms)?;
    tx.execute(
        "UPDATE object_uploads
            SET state = 'delete_pending', updated_at_ms = ?2
          WHERE account_id = ?1 AND state IN ('pending', 'ready')",
        params![account_id, now_ms],
    )?;
    enqueue_account_deletes_sqlite(tx, account_id, now_ms)?;
    tx.execute(
        "UPDATE object_storage_outbox
            SET state = 'abandoned',
                last_error = 'account deletion fenced object upload',
                updated_at_ms = ?2,
                completed_at_ms = ?2
          WHERE account_id = ?1
            AND operation = 'put'
            AND (
                state IN ('pending', 'retry')
                OR (state = 'processing' AND updated_at_ms <= ?3)
            )",
        params![
            account_id,
            now_ms,
            now_ms.saturating_sub(PROCESSING_LEASE_MS)
        ],
    )?;
    Ok(())
}

pub(crate) fn fence_account_deletion_postgres_tx(
    tx: &mut PgTransaction<'_>,
    account_id: &str,
    now_ms: i64,
) -> Result<()> {
    crate::db::support_diagnostics::ensure_revoked_postgres_tx(tx, account_id, now_ms)?;
    tx.execute(
        "UPDATE object_uploads
            SET state = 'delete_pending', updated_at_ms = $2
          WHERE account_id = $1 AND state IN ('pending', 'ready')",
        &[&account_id, &now_ms],
    )?;
    enqueue_account_deletes_postgres(tx, account_id, now_ms)?;
    tx.execute(
        "UPDATE object_storage_outbox
            SET state = 'abandoned',
                last_error = 'account deletion fenced object upload',
                updated_at_ms = $2,
                completed_at_ms = $2
          WHERE account_id = $1
            AND operation = 'put'
            AND (
                state IN ('pending', 'retry')
                OR (state = 'processing' AND updated_at_ms <= $3)
            )",
        &[
            &account_id,
            &now_ms,
            &now_ms.saturating_sub(PROCESSING_LEASE_MS),
        ],
    )?;
    Ok(())
}

pub(crate) fn account_deletion_objects_ready_sqlite_tx(
    tx: &SqliteTransaction<'_>,
    account_id: &str,
) -> Result<bool> {
    let unresolved: i64 = tx.query_row(
        "SELECT COUNT(*)
           FROM object_uploads u
          WHERE u.account_id = ?1
            AND (
                u.state <> 'deleted'
                OR EXISTS (
                    SELECT 1 FROM object_storage_outbox o
                     WHERE o.upload_id = u.id
                       AND o.operation = 'put'
                       AND o.state = 'processing'
                )
                OR EXISTS (
                    SELECT 1 FROM object_storage_outbox o
                     WHERE o.upload_id = u.id
                       AND o.operation = 'delete'
                       AND o.state <> 'completed'
                )
            )",
        params![account_id],
        |row| row.get(0),
    )?;
    Ok(unresolved == 0)
}

pub(crate) fn account_deletion_objects_ready_postgres_tx(
    tx: &mut PgTransaction<'_>,
    account_id: &str,
) -> Result<bool> {
    let unresolved: i64 = tx
        .query_one(
            "SELECT COUNT(*)::bigint
               FROM object_uploads u
              WHERE u.account_id = $1
                AND (
                    u.state <> 'deleted'
                    OR EXISTS (
                        SELECT 1 FROM object_storage_outbox o
                         WHERE o.upload_id = u.id
                           AND o.operation = 'put'
                           AND o.state = 'processing'
                    )
                    OR EXISTS (
                        SELECT 1 FROM object_storage_outbox o
                         WHERE o.upload_id = u.id
                           AND o.operation = 'delete'
                           AND o.state <> 'completed'
                    )
                )",
            &[&account_id],
        )?
        .get(0);
    Ok(unresolved == 0)
}

pub fn mark_account_object_deleted(
    pool: &DbPool,
    account_id: &str,
    object_key: &str,
    now_ms: i64,
) -> Result<bool> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let upload = load_account_object_by_key_sqlite(&tx, account_id, object_key)?;
            let Some(upload) = upload else {
                tx.commit()?;
                return Ok(false);
            };
            if account_object_put_is_processing_sqlite(&tx, &upload.id)? {
                tx.commit()?;
                return Ok(false);
            }
            mark_account_object_deleted_sqlite_tx(&tx, &upload, now_ms)?;
            tx.commit()?;
            Ok(true)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            let upload = load_account_object_by_key_postgres(&mut tx, account_id, object_key)?;
            let Some(upload) = upload else {
                tx.commit()?;
                return Ok(false);
            };
            if account_object_put_is_processing_postgres(&mut tx, &upload.id)? {
                tx.commit()?;
                return Ok(false);
            }
            mark_account_object_deleted_postgres_tx(&mut tx, &upload, now_ms)?;
            tx.commit()?;
            Ok(true)
        }
    })
}

fn account_object_put_is_processing_sqlite(
    tx: &SqliteTransaction<'_>,
    upload_id: &str,
) -> Result<bool> {
    Ok(tx
        .query_row(
            "SELECT 1 FROM object_storage_outbox
              WHERE upload_id = ?1 AND operation = 'put' AND state = 'processing'",
            params![upload_id],
            |_| Ok(()),
        )
        .optional()?
        .is_some())
}

fn account_object_put_is_processing_postgres(
    tx: &mut PgTransaction<'_>,
    upload_id: &str,
) -> Result<bool> {
    Ok(tx
        .query_opt(
            "SELECT 1 FROM object_storage_outbox
              WHERE upload_id = $1 AND operation = 'put' AND state = 'processing'",
            &[&upload_id],
        )?
        .is_some())
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

pub(crate) fn schedule_support_diagnostic_cleanup_sqlite_tx(
    tx: &SqliteTransaction<'_>,
    account_id: &str,
    session_id: Option<&str>,
    now_ms: i64,
) -> Result<usize> {
    let changed = tx.execute(
        "UPDATE object_uploads
            SET state = 'delete_pending', updated_at_ms = ?3
          WHERE account_id = ?1
            AND object_kind = 'session_audit'
            AND (?2 IS NULL OR session_id = ?2)
            AND state IN ('pending', 'ready')",
        params![account_id, session_id, now_ms],
    )?;
    tx.execute(
        "INSERT INTO object_storage_outbox (
            id, upload_id, account_id, operation, state, next_attempt_at_ms,
            created_at_ms, updated_at_ms
         )
         SELECT u.id || ':delete', u.id, u.account_id, 'delete', 'pending', ?3, ?3, ?3
           FROM object_uploads u
          WHERE u.account_id = ?1
            AND u.object_kind = 'session_audit'
            AND (?2 IS NULL OR u.session_id = ?2)
            AND u.state = 'delete_pending'
         ON CONFLICT(upload_id, operation) DO NOTHING",
        params![account_id, session_id, now_ms],
    )?;
    tx.execute(
        "UPDATE object_storage_outbox
            SET state = 'abandoned',
                last_error = 'support diagnostics deleted or consent revoked',
                updated_at_ms = ?3,
                completed_at_ms = ?3
          WHERE account_id = ?1
            AND operation = 'put'
            AND (
                state IN ('pending', 'retry')
                OR (state = 'processing' AND updated_at_ms <= ?4)
            )
            AND upload_id IN (
                SELECT id FROM object_uploads
                 WHERE account_id = ?1
                   AND object_kind = 'session_audit'
                   AND (?2 IS NULL OR session_id = ?2)
                   AND state = 'delete_pending'
            )",
        params![
            account_id,
            session_id,
            now_ms,
            now_ms.saturating_sub(PROCESSING_LEASE_MS)
        ],
    )?;
    Ok(changed)
}

pub(crate) fn schedule_support_diagnostic_cleanup_postgres_tx(
    tx: &mut PgTransaction<'_>,
    account_id: &str,
    session_id: Option<&str>,
    now_ms: i64,
) -> Result<usize> {
    let changed = tx.execute(
        "UPDATE object_uploads
            SET state = 'delete_pending', updated_at_ms = $3
          WHERE account_id = $1
            AND object_kind = 'session_audit'
            AND ($2::text IS NULL OR session_id = $2)
            AND state IN ('pending', 'ready')",
        &[&account_id, &session_id, &now_ms],
    )?;
    tx.execute(
        "INSERT INTO object_storage_outbox (
            id, upload_id, account_id, operation, state, next_attempt_at_ms,
            created_at_ms, updated_at_ms
         )
         SELECT u.id || ':delete', u.id, u.account_id, 'delete', 'pending', $3, $3, $3
           FROM object_uploads u
          WHERE u.account_id = $1
            AND u.object_kind = 'session_audit'
            AND ($2::text IS NULL OR u.session_id = $2)
            AND u.state = 'delete_pending'
         ON CONFLICT(upload_id, operation) DO NOTHING",
        &[&account_id, &session_id, &now_ms],
    )?;
    tx.execute(
        "UPDATE object_storage_outbox
            SET state = 'abandoned',
                last_error = 'support diagnostics deleted or consent revoked',
                updated_at_ms = $3,
                completed_at_ms = $3
          WHERE account_id = $1
            AND operation = 'put'
            AND (
                state IN ('pending', 'retry')
                OR (state = 'processing' AND updated_at_ms <= $4)
            )
            AND upload_id IN (
                SELECT id FROM object_uploads
                 WHERE account_id = $1
                   AND object_kind = 'session_audit'
                   AND ($2::text IS NULL OR session_id = $2)
                   AND state = 'delete_pending'
            )",
        &[
            &account_id,
            &session_id,
            &now_ms,
            &now_ms.saturating_sub(PROCESSING_LEASE_MS),
        ],
    )?;
    Ok(changed as usize)
}

pub(crate) fn schedule_artifact_cleanup_sqlite_tx(
    tx: &SqliteTransaction<'_>,
    account_id: &str,
    artifact_id: &str,
    now_ms: i64,
) -> Result<usize> {
    let changed = tx.execute(
        "UPDATE object_uploads
            SET state = 'delete_pending', updated_at_ms = ?3
          WHERE account_id = ?1
            AND object_kind = 'artifact'
            AND logical_id = ?2
            AND state IN ('pending', 'ready')",
        params![account_id, artifact_id, now_ms],
    )?;
    tx.execute(
        "INSERT INTO object_storage_outbox (
            id, upload_id, account_id, operation, state, next_attempt_at_ms,
            created_at_ms, updated_at_ms
         )
         SELECT u.id || ':delete', u.id, u.account_id, 'delete', 'pending', ?3, ?3, ?3
           FROM object_uploads u
          WHERE u.account_id = ?1
            AND u.object_kind = 'artifact'
            AND u.logical_id = ?2
            AND u.state = 'delete_pending'
         ON CONFLICT(upload_id, operation) DO NOTHING",
        params![account_id, artifact_id, now_ms],
    )?;
    tx.execute(
        "UPDATE object_storage_outbox
            SET state = 'abandoned',
                last_error = 'artifact deleted before upload completed',
                updated_at_ms = ?3,
                completed_at_ms = ?3
          WHERE operation = 'put'
            AND (
                state IN ('pending', 'retry')
                OR (state = 'processing' AND updated_at_ms <= ?4)
            )
            AND upload_id IN (
                SELECT id
                  FROM object_uploads
                 WHERE account_id = ?1
                   AND object_kind = 'artifact'
                   AND logical_id = ?2
            )",
        params![
            account_id,
            artifact_id,
            now_ms,
            now_ms.saturating_sub(PROCESSING_LEASE_MS)
        ],
    )?;
    Ok(changed)
}

pub(crate) fn schedule_artifact_cleanup_postgres_tx(
    tx: &mut PgTransaction<'_>,
    account_id: &str,
    artifact_id: &str,
    now_ms: i64,
) -> Result<usize> {
    let changed = tx.execute(
        "UPDATE object_uploads
            SET state = 'delete_pending', updated_at_ms = $3
          WHERE account_id = $1
            AND object_kind = 'artifact'
            AND logical_id = $2
            AND state IN ('pending', 'ready')",
        &[&account_id, &artifact_id, &now_ms],
    )?;
    tx.execute(
        "INSERT INTO object_storage_outbox (
            id, upload_id, account_id, operation, state, next_attempt_at_ms,
            created_at_ms, updated_at_ms
         )
         SELECT u.id || ':delete', u.id, u.account_id, 'delete', 'pending', $3, $3, $3
           FROM object_uploads u
          WHERE u.account_id = $1
            AND u.object_kind = 'artifact'
            AND u.logical_id = $2
            AND u.state = 'delete_pending'
         ON CONFLICT(upload_id, operation) DO NOTHING",
        &[&account_id, &artifact_id, &now_ms],
    )?;
    tx.execute(
        "UPDATE object_storage_outbox
            SET state = 'abandoned',
                last_error = 'artifact deleted before upload completed',
                updated_at_ms = $3,
                completed_at_ms = $3
          WHERE operation = 'put'
            AND (
                state IN ('pending', 'retry')
                OR (state = 'processing' AND updated_at_ms <= $4)
            )
            AND upload_id IN (
                SELECT id
                  FROM object_uploads
                 WHERE account_id = $1
                   AND object_kind = 'artifact'
                   AND logical_id = $2
            )",
        &[
            &account_id,
            &artifact_id,
            &now_ms,
            &now_ms.saturating_sub(PROCESSING_LEASE_MS),
        ],
    )?;
    Ok(changed as usize)
}

fn validate_input(input: &NewObjectUpload) -> Result<()> {
    validate_common_input(input)?;
    let session_id = input
        .session_id
        .as_deref()
        .map(str::trim)
        .filter(|session_id| !session_id.is_empty() && session_id.len() <= 128)
        .ok_or(UploadControlError::InvalidMetadata("parent session"))?;
    if session_id != input.session_id.as_deref().unwrap_or_default() {
        return Err(UploadControlError::InvalidMetadata("parent session").into());
    }
    Ok(())
}

fn validate_account_object_input(input: &NewObjectUpload) -> Result<()> {
    validate_common_input(input)?;
    if input.object_kind != ObjectKind::Artifact
        || input.storage_scope != StorageScope::Artifact
        || input.session_id.is_some()
    {
        return Err(UploadControlError::InvalidMetadata("account object scope").into());
    }
    Ok(())
}

fn validate_common_input(input: &NewObjectUpload) -> Result<()> {
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
    Ok(())
}

fn reserve_upload_sqlite(
    pool: &DbPool,
    input: &NewObjectUpload,
    requires_session: bool,
) -> Result<UploadReservation> {
    let mut conn = pool.get().context("get sqlite object upload conn")?;
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .context("begin sqlite object upload reservation")?;

    let deletion_pending = tx
        .query_row(
            "SELECT deletion_pending_at_ms FROM accounts WHERE id = ?1",
            params![input.account_id],
            |row| row.get::<_, Option<i64>>(0),
        )
        .optional()?;
    match deletion_pending {
        None => return Err(UploadControlError::SessionNotOwned.into()),
        Some(Some(_)) => return Err(UploadControlError::UploadGone.into()),
        Some(None) => {}
    }

    if requires_session {
        validate_session_sqlite(&tx, input)?;
        if input.object_kind == ObjectKind::SessionAudit
            && !crate::db::support_diagnostics::consent_is_active_sqlite_tx(&tx, &input.account_id)?
        {
            return Err(UploadControlError::SupportDiagnosticConsentRequired.into());
        }
        reject_tombstoned_support_diagnostic_session_sqlite(&tx, input)?;
        reject_tombstoned_artifact_sqlite(&tx, input)?;
    }
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

fn reserve_upload_postgres(
    pool: &DbPool,
    input: &NewObjectUpload,
    requires_session: bool,
) -> Result<UploadReservation> {
    let mut conn = pool.get_pg().context("get postgres object upload conn")?;
    let mut tx = conn
        .transaction()
        .context("begin postgres object upload reservation")?;
    if requires_session && input.object_kind == ObjectKind::Artifact {
        // Sync tombstones take this same identity lock before publishing the
        // tombstone. Take it before the account row lock so the reservation
        // cannot observe "not deleted" and insert after a concurrent delete.
        lock_context_artifact_postgres_tx(&mut tx, &input.logical_id)?;
    }
    if requires_session {
        lock_session_postgres_tx(
            &mut tx,
            input
                .session_id
                .as_deref()
                .ok_or(UploadControlError::InvalidMetadata("parent session"))?,
        )?;
    }
    let account_state = tx.query_opt(
        "SELECT deletion_pending_at_ms FROM accounts WHERE id = $1 FOR UPDATE",
        &[&input.account_id],
    )?;
    match account_state {
        None => return Err(UploadControlError::SessionNotOwned.into()),
        Some(row) if row.get::<_, Option<i64>>(0).is_some() => {
            return Err(UploadControlError::UploadGone.into());
        }
        Some(_) => {}
    }

    if requires_session {
        validate_session_postgres(&mut tx, input)?;
        if input.object_kind == ObjectKind::SessionAudit
            && !crate::db::support_diagnostics::consent_is_active_postgres_tx(
                &mut tx,
                &input.account_id,
            )?
        {
            return Err(UploadControlError::SupportDiagnosticConsentRequired.into());
        }
        reject_tombstoned_support_diagnostic_session_postgres(&mut tx, input)?;
        reject_tombstoned_artifact_postgres(&mut tx, input)?;
    }
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

fn reject_tombstoned_support_diagnostic_session_sqlite(
    tx: &SqliteTransaction<'_>,
    input: &NewObjectUpload,
) -> Result<()> {
    if input.object_kind != ObjectKind::SessionAudit {
        return Ok(());
    }
    let session_id = input
        .session_id
        .as_deref()
        .ok_or(UploadControlError::InvalidMetadata("parent session"))?;
    let tombstoned = tx
        .query_row(
            "SELECT 1 FROM support_diagnostic_session_tombstones
              WHERE account_id = ?1 AND session_id = ?2",
            params![input.account_id, session_id],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    if tombstoned {
        return Err(UploadControlError::UploadGone.into());
    }
    Ok(())
}

fn reject_tombstoned_support_diagnostic_session_postgres(
    tx: &mut PgTransaction<'_>,
    input: &NewObjectUpload,
) -> Result<()> {
    if input.object_kind != ObjectKind::SessionAudit {
        return Ok(());
    }
    let session_id = input
        .session_id
        .as_deref()
        .ok_or(UploadControlError::InvalidMetadata("parent session"))?;
    let tombstoned = tx
        .query_opt(
            "SELECT 1 FROM support_diagnostic_session_tombstones
              WHERE account_id = $1 AND session_id = $2",
            &[&input.account_id, &session_id],
        )?
        .is_some();
    if tombstoned {
        return Err(UploadControlError::UploadGone.into());
    }
    Ok(())
}

fn reject_tombstoned_artifact_sqlite(
    tx: &SqliteTransaction<'_>,
    input: &NewObjectUpload,
) -> Result<()> {
    if input.object_kind != ObjectKind::Artifact {
        return Ok(());
    }
    let tombstoned = tx
        .query_row(
            "SELECT 1 FROM cloud_child_tombstones
              WHERE account_id = ?1 AND child_kind = 'context' AND child_id = ?2",
            params![input.account_id, input.logical_id],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    if tombstoned {
        return Err(UploadControlError::UploadGone.into());
    }
    Ok(())
}

fn reject_tombstoned_artifact_postgres(
    tx: &mut PgTransaction<'_>,
    input: &NewObjectUpload,
) -> Result<()> {
    if input.object_kind != ObjectKind::Artifact {
        return Ok(());
    }
    let tombstoned = tx
        .query_opt(
            "SELECT 1 FROM cloud_child_tombstones
              WHERE account_id = $1 AND child_kind = 'context' AND child_id = $2",
            &[&input.account_id, &input.logical_id],
        )?
        .is_some();
    if tombstoned {
        return Err(UploadControlError::UploadGone.into());
    }
    Ok(())
}

fn retry_reservation(
    existing: &ObjectUpload,
    input: &NewObjectUpload,
) -> Result<UploadReservation> {
    let same_content = existing.sha256.eq_ignore_ascii_case(&input.sha256)
        && existing.size_bytes == input.size_bytes
        && existing.storage_scope == input.storage_scope.as_str()
        && existing.session_id == input.session_id;
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

fn abandon_put_outbox_sqlite(
    tx: &SqliteTransaction<'_>,
    upload_id: &str,
    now_ms: i64,
) -> Result<()> {
    tx.execute(
        "UPDATE object_storage_outbox
            SET state = 'abandoned', updated_at_ms = ?2, completed_at_ms = ?2,
                next_attempt_at_ms = ?2, last_error = 'account object PUT closed by cleanup'
          WHERE upload_id = ?1 AND operation = 'put'
            AND state IN ('pending', 'processing', 'retry')",
        params![upload_id, now_ms],
    )?;
    Ok(())
}

fn abandon_put_outbox_postgres(
    tx: &mut PgTransaction<'_>,
    upload_id: &str,
    now_ms: i64,
) -> Result<()> {
    tx.execute(
        "UPDATE object_storage_outbox
            SET state = 'abandoned', updated_at_ms = $2, completed_at_ms = $2,
                next_attempt_at_ms = $2, last_error = 'account object PUT closed by cleanup'
          WHERE upload_id = $1 AND operation = 'put'
            AND state IN ('pending', 'processing', 'retry')",
        &[&upload_id, &now_ms],
    )?;
    Ok(())
}

fn ensure_delete_outbox_sqlite(
    tx: &SqliteTransaction<'_>,
    upload: &ObjectUpload,
    now_ms: i64,
) -> Result<()> {
    tx.execute(
        "INSERT INTO object_storage_outbox (
            id, upload_id, account_id, operation, state, next_attempt_at_ms,
            created_at_ms, updated_at_ms
         ) VALUES (?1, ?2, ?3, 'delete', 'pending', ?4, ?4, ?4)
         ON CONFLICT(upload_id, operation) DO NOTHING",
        params![
            outbox_id(&upload.id, "delete"),
            upload.id,
            upload.account_id,
            now_ms
        ],
    )?;
    Ok(())
}

fn ensure_delete_outbox_postgres(
    tx: &mut PgTransaction<'_>,
    upload: &ObjectUpload,
    now_ms: i64,
) -> Result<()> {
    tx.execute(
        "INSERT INTO object_storage_outbox (
            id, upload_id, account_id, operation, state, next_attempt_at_ms,
            created_at_ms, updated_at_ms
         ) VALUES ($1, $2, $3, 'delete', 'pending', $4, $4, $4)
         ON CONFLICT(upload_id, operation) DO NOTHING",
        &[
            &outbox_id(&upload.id, "delete"),
            &upload.id,
            &upload.account_id,
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
          WHERE upload_id = ?1 AND operation = 'put'
            AND state IN ('pending', 'processing', 'retry')",
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
          WHERE upload_id = $1 AND operation = 'put'
            AND state IN ('pending', 'processing', 'retry')",
        &[&upload_id, &now_ms, &attempts.saturating_add(1)],
    )?;
    Ok(())
}

fn finalize_account_object_put_sqlite(
    pool: &DbPool,
    upload_id: &str,
    now_ms: i64,
) -> Result<ObjectUpload> {
    let mut conn = pool.get()?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let mut upload =
        load_upload_sqlite(&tx, upload_id)?.ok_or(UploadControlError::UploadNotFound)?;
    if upload.session_id.is_some() || upload.object_kind != ObjectKind::Artifact.as_str() {
        return Err(UploadControlError::InvalidMetadata("account object scope").into());
    }
    let account_state = tx
        .query_row(
            "SELECT deletion_pending_at_ms FROM accounts WHERE id = ?1",
            params![upload.account_id],
            |row| row.get::<_, Option<i64>>(0),
        )
        .optional()?;
    if !matches!(account_state, Some(None))
        || matches!(upload.state.as_str(), "delete_pending" | "deleted")
    {
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
    tx.commit()?;
    Ok(upload)
}

fn finalize_account_object_put_postgres(
    pool: &DbPool,
    upload_id: &str,
    now_ms: i64,
) -> Result<ObjectUpload> {
    let mut conn = pool.get_pg()?;
    let mut tx = conn.transaction()?;
    let identity = load_upload_postgres_unlocked(&mut tx, upload_id)?
        .ok_or(UploadControlError::UploadNotFound)?;
    let account_state = tx.query_opt(
        "SELECT deletion_pending_at_ms FROM accounts WHERE id = $1 FOR UPDATE",
        &[&identity.account_id],
    )?;
    let mut upload =
        load_upload_postgres(&mut tx, upload_id)?.ok_or(UploadControlError::UploadNotFound)?;
    if upload.session_id.is_some() || upload.object_kind != ObjectKind::Artifact.as_str() {
        return Err(UploadControlError::InvalidMetadata("account object scope").into());
    }
    if !account_state.is_some_and(|row| row.get::<_, Option<i64>>(0).is_none())
        || matches!(upload.state.as_str(), "delete_pending" | "deleted")
    {
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
    tx.commit()?;
    Ok(upload)
}

fn mark_upload_ready_sqlite(pool: &DbPool, upload_id: &str, now_ms: i64) -> Result<ObjectUpload> {
    let mut conn = pool.get()?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let mut upload =
        load_upload_sqlite(&tx, upload_id)?.ok_or(UploadControlError::UploadNotFound)?;
    if matches!(upload.state.as_str(), "delete_pending" | "deleted") {
        return Err(UploadControlError::UploadGone.into());
    }
    if !upload_parent_is_live_sqlite(&tx, &upload)? {
        schedule_session_cleanup_sqlite_tx(
            &tx,
            &upload.account_id,
            upload
                .session_id
                .as_deref()
                .ok_or(UploadControlError::InvalidMetadata("parent session"))?,
            now_ms,
        )?;
        tx.commit()?;
        return Err(UploadControlError::UploadGone.into());
    }
    if upload.object_kind == ObjectKind::Artifact.as_str()
        && artifact_tombstoned_sqlite(&tx, &upload.account_id, &upload.logical_id)?
    {
        schedule_artifact_cleanup_sqlite_tx(&tx, &upload.account_id, &upload.logical_id, now_ms)?;
        tx.commit()?;
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
    let identity = load_upload_postgres_unlocked(&mut tx, upload_id)?
        .ok_or(UploadControlError::UploadNotFound)?;
    if identity.object_kind == ObjectKind::Artifact.as_str() {
        lock_context_artifact_postgres_tx(&mut tx, &identity.logical_id)?;
    }
    lock_session_postgres_tx(
        &mut tx,
        identity
            .session_id
            .as_deref()
            .ok_or(UploadControlError::InvalidMetadata("parent session"))?,
    )?;
    let mut upload =
        load_upload_postgres(&mut tx, upload_id)?.ok_or(UploadControlError::UploadNotFound)?;
    if matches!(upload.state.as_str(), "delete_pending" | "deleted") {
        return Err(UploadControlError::UploadGone.into());
    }
    if !upload_parent_is_live_postgres(&mut tx, &upload)? {
        schedule_session_cleanup_postgres_tx(
            &mut tx,
            &upload.account_id,
            upload
                .session_id
                .as_deref()
                .ok_or(UploadControlError::InvalidMetadata("parent session"))?,
            now_ms,
        )?;
        tx.commit()?;
        return Err(UploadControlError::UploadGone.into());
    }
    if upload.object_kind == ObjectKind::Artifact.as_str()
        && artifact_tombstoned_postgres(&mut tx, &upload.account_id, &upload.logical_id)?
    {
        schedule_artifact_cleanup_postgres_tx(
            &mut tx,
            &upload.account_id,
            &upload.logical_id,
            now_ms,
        )?;
        tx.commit()?;
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

fn upload_parent_is_live_sqlite(tx: &SqliteTransaction<'_>, upload: &ObjectUpload) -> Result<bool> {
    let session_id = upload
        .session_id
        .as_deref()
        .ok_or(UploadControlError::InvalidMetadata("parent session"))?;
    Ok(tx
        .query_row(
            "SELECT 1 FROM cloud_sessions
              WHERE account_id = ?1 AND session_id = ?2 AND deleted_at_ms IS NULL",
            params![upload.account_id, session_id],
            |_| Ok(()),
        )
        .optional()?
        .is_some())
}

fn upload_parent_is_live_postgres(
    tx: &mut PgTransaction<'_>,
    upload: &ObjectUpload,
) -> Result<bool> {
    let session_id = upload
        .session_id
        .as_deref()
        .ok_or(UploadControlError::InvalidMetadata("parent session"))?;
    Ok(tx
        .query_opt(
            "SELECT 1 FROM cloud_sessions
              WHERE account_id = $1 AND session_id = $2 AND deleted_at_ms IS NULL",
            &[&upload.account_id, &session_id],
        )?
        .is_some())
}

fn artifact_tombstoned_sqlite(
    tx: &SqliteTransaction<'_>,
    account_id: &str,
    artifact_id: &str,
) -> Result<bool> {
    Ok(tx
        .query_row(
            "SELECT 1 FROM cloud_child_tombstones
              WHERE account_id = ?1 AND child_kind = 'context' AND child_id = ?2",
            params![account_id, artifact_id],
            |_| Ok(()),
        )
        .optional()?
        .is_some())
}

fn artifact_tombstoned_postgres(
    tx: &mut PgTransaction<'_>,
    account_id: &str,
    artifact_id: &str,
) -> Result<bool> {
    Ok(tx
        .query_opt(
            "SELECT 1 FROM cloud_child_tombstones
              WHERE account_id = $1 AND child_kind = 'context' AND child_id = $2",
            &[&account_id, &artifact_id],
        )?
        .is_some())
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
    let upload = load_upload_sqlite(&tx, upload_id)?.ok_or(UploadControlError::UploadNotFound)?;
    if matches!(upload.state.as_str(), "delete_pending" | "deleted") {
        abandon_put_outbox_sqlite(&tx, upload_id, now_ms)?;
        if upload.state == "delete_pending" {
            ensure_delete_outbox_sqlite(&tx, &upload, now_ms)?;
        }
        tx.commit()?;
        return Err(UploadControlError::UploadGone.into());
    }
    if upload.state != "pending" {
        return Err(UploadControlError::UploadNotFound.into());
    }
    let attempts = outbox_attempts_sqlite(&tx, upload_id, "put")?;
    let changed = tx.execute(
        "UPDATE object_storage_outbox
            SET state = 'retry', attempt_count = ?2, next_attempt_at_ms = ?3,
                last_error = ?4, updated_at_ms = ?5
          WHERE upload_id = ?1 AND operation = 'put' AND state = 'processing'",
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
    let identity = load_upload_postgres_unlocked(&mut tx, upload_id)?
        .ok_or(UploadControlError::UploadNotFound)?;
    if identity.object_kind == ObjectKind::Artifact.as_str() && identity.session_id.is_some() {
        lock_context_artifact_postgres_tx(&mut tx, &identity.logical_id)?;
    }
    if let Some(session_id) = identity.session_id.as_deref() {
        lock_session_postgres_tx(&mut tx, session_id)?;
    }
    if tx
        .query_opt(
            "SELECT id FROM accounts WHERE id = $1 FOR UPDATE",
            &[&identity.account_id],
        )?
        .is_none()
    {
        return Err(UploadControlError::UploadNotFound.into());
    }
    let upload =
        load_upload_postgres(&mut tx, upload_id)?.ok_or(UploadControlError::UploadNotFound)?;
    if matches!(upload.state.as_str(), "delete_pending" | "deleted") {
        abandon_put_outbox_postgres(&mut tx, upload_id, now_ms)?;
        if upload.state == "delete_pending" {
            ensure_delete_outbox_postgres(&mut tx, &upload, now_ms)?;
        }
        tx.commit()?;
        return Err(UploadControlError::UploadGone.into());
    }
    if upload.state != "pending" {
        return Err(UploadControlError::UploadNotFound.into());
    }
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
          WHERE upload_id = $1 AND operation = 'put' AND state = 'processing'",
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
            AND NOT EXISTS (
                SELECT 1 FROM object_storage_outbox p
                 WHERE p.upload_id = o.upload_id AND p.operation = 'put'
                   AND p.state = 'processing' AND p.updated_at_ms > ?5
            )
          ORDER BY o.created_at_ms, o.id
          LIMIT ?4",
    )?;
    let rows = stmt.query_map(
        params![
            account_id,
            storage_scope.as_str(),
            now_ms,
            limit,
            now_ms.saturating_sub(PROCESSING_LEASE_MS)
        ],
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
            AND NOT EXISTS (
                SELECT 1 FROM object_storage_outbox p
                 WHERE p.upload_id = o.upload_id AND p.operation = 'put'
                   AND p.state = 'processing' AND p.updated_at_ms > $5
            )
          ORDER BY o.created_at_ms, o.id
          FOR UPDATE OF o SKIP LOCKED
          LIMIT $4",
        &[
            &account_id,
            &storage_scope.as_str(),
            &now_ms,
            &limit,
            &now_ms.saturating_sub(PROCESSING_LEASE_MS),
        ],
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
          WHERE operation = 'put'
            AND (state IN ('pending', 'retry')
                 OR (state = 'processing' AND updated_at_ms <= ?3))
            AND upload_id IN (
                SELECT id FROM object_uploads
                 WHERE storage_scope = ?1 AND state = 'delete_pending'
            )",
        params![
            storage_scope.as_str(),
            now_ms,
            now_ms.saturating_sub(PROCESSING_LEASE_MS)
        ],
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
            AND NOT EXISTS (
                SELECT 1 FROM object_storage_outbox p
                 WHERE p.upload_id = o.upload_id AND p.operation = 'put'
                   AND p.state = 'processing' AND p.updated_at_ms > ?4
            )
          ORDER BY o.created_at_ms, o.id
          LIMIT ?3",
    )?;
    let rows = stmt.query_map(
        params![
            storage_scope.as_str(),
            now_ms,
            limit,
            now_ms.saturating_sub(PROCESSING_LEASE_MS)
        ],
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
          WHERE operation = 'put'
            AND (state IN ('pending', 'retry')
                 OR (state = 'processing' AND updated_at_ms <= $3))
            AND upload_id IN (
                SELECT id FROM object_uploads
                 WHERE storage_scope = $1 AND state = 'delete_pending'
            )",
        &[
            &storage_scope.as_str(),
            &now_ms,
            &now_ms.saturating_sub(PROCESSING_LEASE_MS),
        ],
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
            AND NOT EXISTS (
                SELECT 1 FROM object_storage_outbox p
                 WHERE p.upload_id = o.upload_id AND p.operation = 'put'
                   AND p.state = 'processing' AND p.updated_at_ms > $4
            )
          ORDER BY o.created_at_ms, o.id
          FOR UPDATE OF o SKIP LOCKED
          LIMIT $3",
        &[
            &storage_scope.as_str(),
            &now_ms,
            &limit,
            &now_ms.saturating_sub(PROCESSING_LEASE_MS),
        ],
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

fn enqueue_account_deletes_sqlite(
    tx: &SqliteTransaction<'_>,
    account_id: &str,
    now_ms: i64,
) -> Result<()> {
    tx.execute(
        "INSERT INTO object_storage_outbox (
            id, upload_id, account_id, operation, state, next_attempt_at_ms,
            created_at_ms, updated_at_ms
         )
         SELECT u.id || ':delete', u.id, u.account_id, 'delete', 'pending', ?2, ?2, ?2
           FROM object_uploads u
          WHERE u.account_id = ?1 AND u.state = 'delete_pending'
         ON CONFLICT(upload_id, operation) DO NOTHING",
        params![account_id, now_ms],
    )?;
    Ok(())
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

fn enqueue_account_deletes_postgres(
    tx: &mut PgTransaction<'_>,
    account_id: &str,
    now_ms: i64,
) -> Result<()> {
    tx.execute(
        "INSERT INTO object_storage_outbox (
            id, upload_id, account_id, operation, state, next_attempt_at_ms,
            created_at_ms, updated_at_ms
         )
         SELECT u.id || ':delete', u.id, u.account_id, 'delete', 'pending', $2, $2, $2
           FROM object_uploads u
          WHERE u.account_id = $1 AND u.state = 'delete_pending'
         ON CONFLICT(upload_id, operation) DO NOTHING",
        &[&account_id, &now_ms],
    )?;
    Ok(())
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
          WHERE account_id = ?1 AND operation = 'put'
            AND (state IN ('pending', 'retry')
                 OR (state = 'processing' AND updated_at_ms <= ?4))
            AND upload_id IN (
                SELECT id FROM object_uploads
                 WHERE account_id = ?1 AND state = 'delete_pending'
                   AND (?2 IS NULL OR session_id = ?2)
            )",
        params![
            account_id,
            session_id,
            now_ms,
            now_ms.saturating_sub(PROCESSING_LEASE_MS)
        ],
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
          WHERE account_id = $1 AND operation = 'put'
            AND (state IN ('pending', 'retry')
                 OR (state = 'processing' AND updated_at_ms <= $4))
            AND upload_id IN (
                SELECT id FROM object_uploads
                 WHERE account_id = $1 AND state = 'delete_pending'
                   AND ($2::text IS NULL OR session_id = $2)
            )",
        &[
            &account_id,
            &session_id,
            &now_ms,
            &now_ms.saturating_sub(PROCESSING_LEASE_MS),
        ],
    )?;
    Ok(())
}

fn mark_cleanup_succeeded_sqlite(pool: &DbPool, upload_id: &str, now_ms: i64) -> Result<()> {
    let mut conn = pool.get()?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let upload = load_upload_sqlite(&tx, upload_id)?.ok_or(UploadControlError::UploadNotFound)?;
    abandon_put_outbox_sqlite(&tx, upload_id, now_ms)?;
    mark_account_object_deleted_sqlite_tx(&tx, &upload, now_ms)?;
    tx.commit()?;
    Ok(())
}

fn mark_cleanup_succeeded_postgres(pool: &DbPool, upload_id: &str, now_ms: i64) -> Result<()> {
    let mut conn = pool.get_pg()?;
    let mut tx = conn.transaction()?;
    let upload =
        load_upload_postgres(&mut tx, upload_id)?.ok_or(UploadControlError::UploadNotFound)?;
    abandon_put_outbox_postgres(&mut tx, upload_id, now_ms)?;
    mark_account_object_deleted_postgres_tx(&mut tx, &upload, now_ms)?;
    tx.commit()?;
    Ok(())
}

fn mark_account_object_deleted_sqlite_tx(
    tx: &SqliteTransaction<'_>,
    upload: &ObjectUpload,
    now_ms: i64,
) -> Result<()> {
    tx.execute(
        "UPDATE object_uploads
            SET state = 'deleted', deleted_at_ms = ?2, updated_at_ms = ?2
          WHERE id = ?1",
        params![upload.id, now_ms],
    )?;
    tx.execute(
        "UPDATE object_storage_outbox
            SET state = 'completed', completed_at_ms = ?2, updated_at_ms = ?2,
                next_attempt_at_ms = ?2, last_error = NULL
          WHERE upload_id = ?1 AND operation = 'delete'",
        params![upload.id, now_ms],
    )?;
    remove_published_index_sqlite(tx, upload)
}

fn mark_account_object_deleted_postgres_tx(
    tx: &mut PgTransaction<'_>,
    upload: &ObjectUpload,
    now_ms: i64,
) -> Result<()> {
    tx.execute(
        "UPDATE object_uploads
            SET state = 'deleted', deleted_at_ms = $2, updated_at_ms = $2
          WHERE id = $1",
        &[&upload.id, &now_ms],
    )?;
    tx.execute(
        "UPDATE object_storage_outbox
            SET state = 'completed', completed_at_ms = $2, updated_at_ms = $2,
                next_attempt_at_ms = $2, last_error = NULL
          WHERE upload_id = $1 AND operation = 'delete'",
        &[&upload.id, &now_ms],
    )?;
    remove_published_index_postgres(tx, upload)
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

fn load_upload_postgres_unlocked(
    tx: &mut PgTransaction<'_>,
    upload_id: &str,
) -> Result<Option<ObjectUpload>> {
    tx.query_opt(
        &format!("SELECT {UPLOAD_COLUMNS} FROM object_uploads WHERE id = $1"),
        &[&upload_id],
    )?
    .map(row_to_upload_postgres)
    .transpose()
}

fn load_account_object_by_key_sqlite(
    tx: &SqliteTransaction<'_>,
    account_id: &str,
    object_key: &str,
) -> Result<Option<ObjectUpload>> {
    tx.query_row(
        &format!(
            "SELECT {UPLOAD_COLUMNS} FROM object_uploads
              WHERE account_id = ?1 AND object_key = ?2"
        ),
        params![account_id, object_key],
        row_to_upload_sqlite,
    )
    .optional()
    .map_err(Into::into)
}

fn load_account_object_by_key_postgres(
    tx: &mut PgTransaction<'_>,
    account_id: &str,
    object_key: &str,
) -> Result<Option<ObjectUpload>> {
    tx.query_opt(
        &format!(
            "SELECT {UPLOAD_COLUMNS} FROM object_uploads
              WHERE account_id = $1 AND object_key = $2
              FOR UPDATE"
        ),
        &[&account_id, &object_key],
    )?
    .map(row_to_upload_postgres)
    .transpose()
}

fn load_account_object_by_key_postgres_unlocked(
    tx: &mut PgTransaction<'_>,
    account_id: &str,
    object_key: &str,
) -> Result<Option<ObjectUpload>> {
    tx.query_opt(
        &format!(
            "SELECT {UPLOAD_COLUMNS} FROM object_uploads
              WHERE account_id = $1 AND object_key = $2"
        ),
        &[&account_id, &object_key],
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
    use crate::db::{open_pool, open_postgres_pool, run_migrations};
    use std::sync::mpsc::{self, RecvTimeoutError};
    use std::time::Duration;

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
        conn.execute(
            "INSERT INTO cloud_sessions (
                account_id, session_id, title, status, created_at_ms, updated_at_ms, metadata_json
             ) VALUES ('acct_1', 'session_2', 'Other session', 'active', 1, 1, '{}')",
            [],
        )
        .unwrap();
        drop(conn);
        crate::db::support_diagnostics::record_consent(
            &pool,
            "acct_1",
            true,
            crate::db::support_diagnostics::SUPPORT_DIAGNOSTIC_POLICY_VERSION,
            crate::db::support_diagnostics::SUPPORT_DIAGNOSTIC_CONTENT_POLICY,
            1,
        )
        .unwrap();
        pool
    }

    fn artifact_input(logical_id: &str, size_bytes: i64, now_ms: i64) -> NewObjectUpload {
        let hash = sha256_hex(format!("payload-{logical_id}"));
        NewObjectUpload {
            account_id: "acct_1".into(),
            object_kind: ObjectKind::Artifact,
            logical_id: logical_id.into(),
            session_id: Some("session_1".into()),
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

    fn account_object_input(logical_id: &str, now_ms: i64) -> NewObjectUpload {
        let mut input = artifact_input(logical_id, 40, now_ms);
        input.logical_id = format!("jobs/{logical_id}");
        input.session_id = None;
        input.object_key = format!("objects/accounts/acct_1/jobs/{logical_id}/{}", input.sha256);
        input
    }

    fn audit_input(bundle_id: &str, now_ms: i64) -> NewObjectUpload {
        let hash = sha256_hex(format!("audit-{bundle_id}"));
        NewObjectUpload {
            account_id: "acct_1".into(),
            object_kind: ObjectKind::SessionAudit,
            logical_id: format!("session_1/{bundle_id}"),
            session_id: Some("session_1".into()),
            storage_scope: StorageScope::Audit,
            object_key: format!("logs/accounts/acct_1/session_1/{bundle_id}/{hash}"),
            size_bytes: 20,
            sha256: hash,
            content_type: "application/json".into(),
            expires_at_ms: now_ms + DAY_MS,
            metadata_json: serde_json::json!({"bundle_id": bundle_id}),
            now_ms,
            limits: UploadLimits {
                max_object_bytes: 100,
                max_account_bytes: 1_000,
                max_daily_bytes: 1_000,
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
        assert_eq!(first.upload.session_id.as_deref(), Some("session_1"));
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

        let mut parent_conflict = artifact_input("artifact_1", 40, 1_200);
        parent_conflict.session_id = Some("session_2".into());
        let error = reserve_upload(&pool, &parent_conflict).unwrap_err();
        assert_eq!(
            error.downcast_ref::<UploadControlError>(),
            Some(&UploadControlError::IdempotencyConflict),
            "a stable artifact id cannot be rebound to another parent session"
        );
    }

    #[test]
    fn context_tombstone_rejects_stale_artifact_reservation() {
        let pool = test_pool();
        let input = artifact_input("deleted-artifact", 40, 1_000);
        pool.get()
            .unwrap()
            .execute(
                "INSERT INTO cloud_child_tombstones (
                    account_id, child_kind, child_id, session_id, deleted_at_ms
                 ) VALUES ('acct_1', 'context', 'deleted-artifact', 'session_1', 900)",
                [],
            )
            .unwrap();

        let error = reserve_upload(&pool, &input).expect_err("stale object must be rejected");
        assert_eq!(
            error.downcast_ref::<UploadControlError>(),
            Some(&UploadControlError::UploadGone)
        );
        assert!(artifact_upload(&pool, "acct_1", "deleted-artifact")
            .unwrap()
            .is_none());
    }

    #[test]
    fn tombstone_racing_reserved_artifact_prevents_ready_publication() {
        let pool = test_pool();
        let input = artifact_input("racing-artifact", 40, 1_000);
        let reservation = reserve_upload(&pool, &input).unwrap();
        pool.get()
            .unwrap()
            .execute(
                "INSERT INTO cloud_child_tombstones (
                    account_id, child_kind, child_id, session_id, deleted_at_ms
                 ) VALUES ('acct_1', 'context', 'racing-artifact', 'session_1', 1_050)",
                [],
            )
            .unwrap();

        let error = mark_upload_ready(&pool, &reservation.upload.id, 1_100)
            .expect_err("tombstoned object must not become ready");
        assert_eq!(
            error.downcast_ref::<UploadControlError>(),
            Some(&UploadControlError::UploadGone)
        );
        let upload = artifact_upload(&pool, "acct_1", "racing-artifact")
            .unwrap()
            .expect("delete-pending upload retained for cleanup");
        assert_eq!(upload.state, "delete_pending");
        let outbox_state: String = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT state FROM object_storage_outbox
                  WHERE upload_id = ?1 AND operation = 'delete'",
                params![reservation.upload.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(outbox_state, "pending");
    }

    #[test]
    #[serial_test::serial]
    fn postgres_tombstone_lock_blocks_a_racing_artifact_reservation() {
        let Ok(database_url) = std::env::var("BLUEY_TEST_POSTGRES_URL") else {
            return;
        };
        let pool = open_postgres_pool(&database_url).expect("open Postgres test pool");

        let suffix = uuid::Uuid::new_v4().simple().to_string();
        let account_id = format!("acct_lock_{suffix}");
        let session_id = format!("session_lock_{suffix}");
        let artifact_id = format!("artifact_lock_{suffix}");
        let email = format!("{account_id}@example.test");
        {
            let mut conn = pool.get_pg().expect("get Postgres setup connection");
            conn.execute(
                "INSERT INTO accounts (id, email, password_hash) VALUES ($1, $2, 'hash')",
                &[&account_id, &email],
            )
            .expect("insert Postgres test account");
            conn.execute(
                "INSERT INTO cloud_sessions (
                    account_id, session_id, title, status, created_at_ms, updated_at_ms,
                    last_active_at_ms, metadata_json
                 ) VALUES ($1, $2, 'Session', 'active', 1, 1, 1, '{}')",
                &[&account_id, &session_id],
            )
            .expect("insert Postgres test session");
        }

        let mut input = artifact_input(&artifact_id, 40, 1_000);
        input.account_id = account_id.clone();
        input.session_id = Some(session_id.clone());
        input.object_key = format!(
            "objects/accounts/{account_id}/{artifact_id}/{}",
            input.sha256
        );

        let mut blocker = pool.get_pg().expect("get Postgres blocker connection");
        let mut blocker_tx = blocker.transaction().expect("begin blocker transaction");
        lock_context_artifact_postgres_tx(&mut blocker_tx, &artifact_id)
            .expect("acquire artifact advisory lock");

        let reservation_pool = pool.clone();
        let (result_tx, result_rx) = mpsc::channel();
        let reservation_thread = std::thread::spawn(move || {
            let _ = result_tx.send(reserve_upload(&reservation_pool, &input));
        });

        assert!(
            matches!(
                result_rx.recv_timeout(Duration::from_millis(300)),
                Err(RecvTimeoutError::Timeout)
            ),
            "reservation completed while the tombstone identity lock was held"
        );

        blocker_tx
            .execute(
                "INSERT INTO cloud_child_tombstones (
                    account_id, child_kind, child_id, session_id, deleted_at_ms
                 ) VALUES ($1, 'context', $2, $3, 1050)",
                &[&account_id, &artifact_id, &session_id],
            )
            .expect("insert racing context tombstone");
        schedule_artifact_cleanup_postgres_tx(&mut blocker_tx, &account_id, &artifact_id, 1_050)
            .expect("schedule racing artifact cleanup");
        blocker_tx.commit().expect("commit racing tombstone");

        let reservation = result_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("reservation should finish after tombstone commit")
            .expect_err("tombstoned artifact reservation must be rejected");
        reservation_thread
            .join()
            .expect("reservation thread should not panic");
        assert_eq!(
            reservation.downcast_ref::<UploadControlError>(),
            Some(&UploadControlError::UploadGone)
        );
        assert!(
            artifact_upload(&pool, &account_id, &artifact_id)
                .expect("query raced artifact upload")
                .is_none(),
            "reservation must not insert an object after the tombstone commits"
        );

        let mut conn = pool.get_pg().expect("get Postgres cleanup connection");
        conn.execute(
            "DELETE FROM cloud_child_tombstones
              WHERE account_id = $1 AND child_kind = 'context' AND child_id = $2",
            &[&account_id, &artifact_id],
        )
        .expect("delete Postgres test tombstone");
        conn.execute(
            "DELETE FROM cloud_sessions WHERE account_id = $1 AND session_id = $2",
            &[&account_id, &session_id],
        )
        .expect("delete Postgres test session");
        conn.execute("DELETE FROM accounts WHERE id = $1", &[&account_id])
            .expect("delete Postgres test account");
    }

    #[test]
    #[serial_test::serial]
    fn postgres_session_lock_blocks_a_racing_audit_reservation() {
        let Ok(database_url) = std::env::var("BLUEY_TEST_POSTGRES_URL") else {
            return;
        };
        let pool = open_postgres_pool(&database_url).expect("open Postgres test pool");

        let suffix = uuid::Uuid::new_v4().simple().to_string();
        let account_id = format!("acct_audit_lock_{suffix}");
        let session_id = format!("session_audit_lock_{suffix}");
        let email = format!("{account_id}@example.test");
        {
            let mut conn = pool.get_pg().expect("get Postgres setup connection");
            conn.execute(
                "INSERT INTO accounts (id, email, password_hash) VALUES ($1, $2, 'hash')",
                &[&account_id, &email],
            )
            .expect("insert Postgres test account");
            conn.execute(
                "INSERT INTO cloud_sessions (
                    account_id, session_id, title, status, created_at_ms, updated_at_ms,
                    last_active_at_ms, metadata_json
                 ) VALUES ($1, $2, 'Session', 'active', 1, 1, 1, '{}')",
                &[&account_id, &session_id],
            )
            .expect("insert Postgres test session");
        }
        crate::db::support_diagnostics::record_consent(
            &pool,
            &account_id,
            true,
            crate::db::support_diagnostics::SUPPORT_DIAGNOSTIC_POLICY_VERSION,
            crate::db::support_diagnostics::SUPPORT_DIAGNOSTIC_CONTENT_POLICY,
            1,
        )
        .expect("grant Postgres support diagnostic consent");

        let hash = sha256_hex("audit payload");
        let input = NewObjectUpload {
            account_id: account_id.clone(),
            object_kind: ObjectKind::SessionAudit,
            logical_id: format!("{session_id}/bundle"),
            session_id: Some(session_id.clone()),
            storage_scope: StorageScope::Audit,
            object_key: format!("logs/accounts/{account_id}/{session_id}/bundle/{hash}"),
            size_bytes: 13,
            sha256: hash,
            content_type: "application/json".into(),
            expires_at_ms: DAY_MS + 1_000,
            metadata_json: serde_json::json!({"bundle_id":"bundle"}),
            now_ms: 1_000,
            limits: UploadLimits {
                max_object_bytes: 100,
                max_account_bytes: 1_000,
                max_daily_bytes: 1_000,
                max_account_objects: 10,
            },
        };

        let mut blocker = pool.get_pg().expect("get Postgres blocker connection");
        let mut blocker_tx = blocker.transaction().expect("begin blocker transaction");
        lock_session_postgres_tx(&mut blocker_tx, &session_id)
            .expect("acquire session advisory lock");

        let reservation_pool = pool.clone();
        let (result_tx, result_rx) = mpsc::channel();
        let reservation_thread = std::thread::spawn(move || {
            let _ = result_tx.send(reserve_upload(&reservation_pool, &input));
        });

        assert!(
            matches!(
                result_rx.recv_timeout(Duration::from_millis(300)),
                Err(RecvTimeoutError::Timeout)
            ),
            "audit reservation completed while its parent session lock was held"
        );

        blocker_tx
            .execute(
                "UPDATE cloud_sessions
                    SET status = 'deleted', deleted_at_ms = 1050, updated_at_ms = 1050
                  WHERE account_id = $1 AND session_id = $2",
                &[&account_id, &session_id],
            )
            .expect("tombstone racing parent session");
        schedule_session_cleanup_postgres_tx(&mut blocker_tx, &account_id, &session_id, 1_050)
            .expect("schedule racing audit cleanup");
        blocker_tx
            .commit()
            .expect("commit racing session tombstone");

        let reservation = result_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("reservation should finish after session tombstone commit")
            .expect_err("deleted parent must reject the audit reservation");
        reservation_thread
            .join()
            .expect("reservation thread should not panic");
        assert_eq!(
            reservation.downcast_ref::<UploadControlError>(),
            Some(&UploadControlError::SessionNotOwned)
        );

        let mut conn = pool.get_pg().expect("get Postgres cleanup connection");
        let count: i64 = conn
            .query_one(
                "SELECT COUNT(*)::bigint FROM object_uploads
                  WHERE account_id = $1 AND session_id = $2",
                &[&account_id, &session_id],
            )
            .expect("count raced audit uploads")
            .get(0);
        assert_eq!(count, 0, "deleted parent must not gain an audit upload");
        conn.execute(
            "DELETE FROM cloud_sessions WHERE account_id = $1 AND session_id = $2",
            &[&account_id, &session_id],
        )
        .expect("delete Postgres test session");
        conn.execute("DELETE FROM accounts WHERE id = $1", &[&account_id])
            .expect("delete Postgres test account");
    }

    #[test]
    #[serial_test::serial]
    fn postgres_support_tombstone_blocks_a_racing_audit_reservation() {
        let Ok(database_url) = std::env::var("BLUEY_TEST_POSTGRES_URL") else {
            return;
        };
        let pool = open_postgres_pool(&database_url).expect("open Postgres test pool");
        let suffix = uuid::Uuid::new_v4().simple().to_string();
        let account_id = format!("acct_support_tombstone_{suffix}");
        let session_id = format!("session_support_tombstone_{suffix}");
        let email = format!("{account_id}@example.test");
        {
            let mut conn = pool.get_pg().expect("get Postgres setup connection");
            conn.execute(
                "INSERT INTO accounts (id, email, password_hash) VALUES ($1, $2, 'hash')",
                &[&account_id, &email],
            )
            .expect("insert Postgres test account");
            conn.execute(
                "INSERT INTO cloud_sessions (
                    account_id, session_id, title, status, created_at_ms, updated_at_ms,
                    last_active_at_ms, metadata_json
                 ) VALUES ($1, $2, 'Session', 'active', 1, 1, 1, '{}')",
                &[&account_id, &session_id],
            )
            .expect("insert Postgres test session");
        }
        crate::db::support_diagnostics::record_consent(
            &pool,
            &account_id,
            true,
            crate::db::support_diagnostics::SUPPORT_DIAGNOSTIC_POLICY_VERSION,
            crate::db::support_diagnostics::SUPPORT_DIAGNOSTIC_CONTENT_POLICY,
            1,
        )
        .expect("grant Postgres support diagnostic consent");

        let hash = sha256_hex("support tombstone race");
        let input = NewObjectUpload {
            account_id: account_id.clone(),
            object_kind: ObjectKind::SessionAudit,
            logical_id: format!("{session_id}/bundle"),
            session_id: Some(session_id.clone()),
            storage_scope: StorageScope::Audit,
            object_key: format!("logs/accounts/{account_id}/{session_id}/bundle/{hash}"),
            size_bytes: 22,
            sha256: hash,
            content_type: "application/json".into(),
            expires_at_ms: DAY_MS + 1_000,
            metadata_json: serde_json::json!({"bundle_id":"bundle"}),
            now_ms: 1_000,
            limits: UploadLimits {
                max_object_bytes: 100,
                max_account_bytes: 1_000,
                max_daily_bytes: 1_000,
                max_account_objects: 10,
            },
        };

        let mut blocker = pool.get_pg().expect("get Postgres blocker connection");
        let mut blocker_tx = blocker.transaction().expect("begin blocker transaction");
        lock_session_postgres_tx(&mut blocker_tx, &session_id)
            .expect("acquire support session advisory lock");

        let reservation_pool = pool.clone();
        let (result_tx, result_rx) = mpsc::channel();
        let reservation_thread = std::thread::spawn(move || {
            let _ = result_tx.send(reserve_upload(&reservation_pool, &input));
        });
        assert!(matches!(
            result_rx.recv_timeout(Duration::from_millis(300)),
            Err(RecvTimeoutError::Timeout)
        ));

        tombstone_support_diagnostic_session_postgres_tx(
            &mut blocker_tx,
            &account_id,
            &session_id,
            1_050,
        )
        .expect("publish support-session tombstone");
        blocker_tx
            .commit()
            .expect("commit support-session tombstone");

        let error = result_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("reservation should finish after tombstone commit")
            .expect_err("tombstoned support session must reject a racing reservation");
        reservation_thread
            .join()
            .expect("reservation thread should not panic");
        assert_eq!(
            error.downcast_ref::<UploadControlError>(),
            Some(&UploadControlError::UploadGone)
        );

        let mut conn = pool.get_pg().expect("get Postgres cleanup connection");
        conn.execute("DELETE FROM accounts WHERE id = $1", &[&account_id])
            .expect("delete Postgres test account");
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

        input.logical_id = "session_1/bundle_2".into();
        input.object_key = input.object_key.replace("bundle_1", "bundle_2");
        input.now_ms = 1_200;
        input.expires_at_ms = input.now_ms + DAY_MS;
        let pending = reserve_upload(&pool, &input).unwrap();
        crate::db::sync::tombstone_session(&pool, "acct_1", "session_1").unwrap();
        let error = mark_upload_ready(&pool, &pending.upload.id, 1_300)
            .expect_err("an audit upload cannot become ready after its parent is deleted");
        assert_eq!(
            error.downcast_ref::<UploadControlError>(),
            Some(&UploadControlError::UploadGone)
        );
        let state: String = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT state FROM object_uploads WHERE id = ?1",
                params![pending.upload.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(state, "delete_pending");
    }

    #[test]
    fn support_diagnostic_revocation_fences_uploads_and_enqueues_durable_cleanup() {
        let pool = test_pool();
        let input = audit_input("revocation-bundle", 1_000);
        let reservation = reserve_upload(&pool, &input).unwrap();
        mark_upload_ready(&pool, &reservation.upload.id, 1_050).unwrap();
        let racing_put = reserve_upload(&pool, &audit_input("racing-put", 1_060)).unwrap();

        crate::db::support_diagnostics::record_consent(
            &pool,
            "acct_1",
            false,
            crate::db::support_diagnostics::SUPPORT_DIAGNOSTIC_POLICY_VERSION,
            crate::db::support_diagnostics::SUPPORT_DIAGNOSTIC_CONTENT_POLICY,
            1_100,
        )
        .unwrap();

        let lifecycle: (String, String) = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT u.state, o.state
                   FROM object_uploads u
                   JOIN object_storage_outbox o
                     ON o.upload_id = u.id AND o.operation = 'delete'
                  WHERE u.id = ?1",
                params![reservation.upload.id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(lifecycle, ("delete_pending".into(), "pending".into()));

        let failure = record_put_failure(&pool, &racing_put.upload.id, "network", 1_150)
            .expect_err("a racing PUT failure must close the fenced PUT lease");
        assert_eq!(
            failure.downcast_ref::<UploadControlError>(),
            Some(&UploadControlError::UploadGone)
        );
        let racing_lifecycle: (String, String, String) = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT u.state, put.state, cleanup.state
                   FROM object_uploads u
                   JOIN object_storage_outbox put
                     ON put.upload_id = u.id AND put.operation = 'put'
                   JOIN object_storage_outbox cleanup
                     ON cleanup.upload_id = u.id AND cleanup.operation = 'delete'
                  WHERE u.id = ?1",
                params![racing_put.upload.id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(
            racing_lifecycle,
            (
                "delete_pending".into(),
                "abandoned".into(),
                "pending".into()
            )
        );

        let repeated_failure = record_put_failure(&pool, &racing_put.upload.id, "late", 1_175)
            .expect_err("a repeated late failure must remain terminal");
        assert_eq!(
            repeated_failure.downcast_ref::<UploadControlError>(),
            Some(&UploadControlError::UploadGone)
        );
        let outbox_states: (String, String) = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT put.state, cleanup.state
                   FROM object_storage_outbox put
                   JOIN object_storage_outbox cleanup ON cleanup.upload_id = put.upload_id
                  WHERE put.upload_id = ?1 AND put.operation = 'put'
                    AND cleanup.operation = 'delete'",
                params![racing_put.upload.id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(outbox_states, ("abandoned".into(), "pending".into()));

        let error = reserve_upload(&pool, &audit_input("after-revocation", 1_200)).unwrap_err();
        assert_eq!(
            error.downcast_ref::<UploadControlError>(),
            Some(&UploadControlError::SupportDiagnosticConsentRequired)
        );

        crate::db::support_diagnostics::record_consent(
            &pool,
            "acct_1",
            true,
            crate::db::support_diagnostics::SUPPORT_DIAGNOSTIC_POLICY_VERSION,
            crate::db::support_diagnostics::SUPPORT_DIAGNOSTIC_CONTENT_POLICY,
            1_250,
        )
        .unwrap();
        let old_epoch = reserve_upload(&pool, &audit_input("racing-put", 1_300)).unwrap_err();
        assert_eq!(
            old_epoch.downcast_ref::<UploadControlError>(),
            Some(&UploadControlError::UploadGone),
            "re-granting consent must not resurrect an upload fenced by the prior epoch"
        );
        assert!(reserve_upload(&pool, &audit_input("after-regrant", 1_300)).is_ok());
    }

    #[test]
    fn support_diagnostic_revocation_is_account_scoped() {
        let pool = test_pool();
        pool.get()
            .unwrap()
            .execute_batch(
                "INSERT INTO cloud_sessions (
                    account_id, session_id, title, status, created_at_ms, updated_at_ms,
                    metadata_json
                 ) VALUES ('acct_2', 'session_2', 'Two', 'active', 1, 1, '{}');",
            )
            .unwrap();
        crate::db::support_diagnostics::record_consent(
            &pool,
            "acct_2",
            true,
            crate::db::support_diagnostics::SUPPORT_DIAGNOSTIC_POLICY_VERSION,
            crate::db::support_diagnostics::SUPPORT_DIAGNOSTIC_CONTENT_POLICY,
            1,
        )
        .unwrap();

        let mut other = audit_input("other-account", 1_000);
        other.account_id = "acct_2".into();
        other.session_id = Some("session_2".into());
        other.logical_id = "session_2/other-account".into();
        other.object_key = other
            .object_key
            .replace("acct_1/session_1", "acct_2/session_2");
        let other_reservation = reserve_upload(&pool, &other).unwrap();

        crate::db::support_diagnostics::record_consent(
            &pool,
            "acct_1",
            false,
            crate::db::support_diagnostics::SUPPORT_DIAGNOSTIC_POLICY_VERSION,
            crate::db::support_diagnostics::SUPPORT_DIAGNOSTIC_CONTENT_POLICY,
            1_100,
        )
        .unwrap();

        assert!(crate::db::support_diagnostics::consent_is_active(&pool, "acct_2").unwrap());
        record_put_failure(&pool, &other_reservation.upload.id, "network", 1_150).unwrap();
        let states: (String, String) = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT u.state, o.state
                   FROM object_uploads u
                   JOIN object_storage_outbox o
                     ON o.upload_id = u.id AND o.operation = 'put'
                  WHERE u.id = ?1",
                params![other_reservation.upload.id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(states, ("pending".into(), "retry".into()));
    }

    #[test]
    fn support_delete_all_atomically_revokes_upload_authority() {
        let pool = test_pool();
        schedule_support_diagnostic_cleanup(&pool, "acct_1", None, 1_000).unwrap();

        let receipt = crate::db::support_diagnostics::current_consent(&pool, "acct_1")
            .unwrap()
            .expect("DELETE-all must leave a revocation receipt");
        assert!(!receipt.enabled);
        let error = reserve_upload(&pool, &audit_input("after-delete-all", 1_100)).unwrap_err();
        assert_eq!(
            error.downcast_ref::<UploadControlError>(),
            Some(&UploadControlError::SupportDiagnosticConsentRequired)
        );
    }

    #[test]
    fn support_session_delete_tombstone_prevents_resurrection() {
        let pool = test_pool();
        schedule_support_diagnostic_cleanup(&pool, "acct_1", Some("session_1"), 1_000).unwrap();

        assert!(crate::db::support_diagnostics::consent_is_active(&pool, "acct_1").unwrap());
        let error = reserve_upload(&pool, &audit_input("resurrection", 1_100)).unwrap_err();
        assert_eq!(
            error.downcast_ref::<UploadControlError>(),
            Some(&UploadControlError::UploadGone)
        );

        let mut other = audit_input("other-session", 1_200);
        other.session_id = Some("session_2".to_string());
        other.logical_id = "session_2/other-session".to_string();
        other.object_key = other.object_key.replace("session_1", "session_2");
        assert!(reserve_upload(&pool, &other).is_ok());
    }

    #[test]
    fn account_delete_waits_for_live_put_and_rejects_new_reservations() {
        let pool = test_pool();
        let input = artifact_input("delete-race", 40, 1_000);
        let reservation = reserve_upload(&pool, &input).unwrap();

        assert!(crate::db::account_data::begin_account_deletion(&pool, "acct_1", 1_050).unwrap());
        let error = reserve_upload(&pool, &artifact_input("after-fence", 40, 1_100)).unwrap_err();
        assert_eq!(
            error.downcast_ref::<UploadControlError>(),
            Some(&UploadControlError::UploadGone)
        );

        mark_account_object_deleted(&pool, "acct_1", &input.object_key, 1_150).unwrap();
        assert_eq!(
            crate::db::account_data::complete_account_deletion(&pool, "acct_1").unwrap(),
            crate::db::account_data::AccountDeletionCompletion::Pending,
            "a fresh processing PUT lease must block hard deletion"
        );

        assert!(crate::db::account_data::begin_account_deletion(
            &pool,
            "acct_1",
            reservation
                .upload
                .updated_at_ms
                .saturating_add(PROCESSING_LEASE_MS)
                .saturating_add(1),
        )
        .unwrap());
        mark_account_object_deleted(
            &pool,
            "acct_1",
            &input.object_key,
            reservation
                .upload
                .updated_at_ms
                .saturating_add(PROCESSING_LEASE_MS)
                .saturating_add(2),
        )
        .unwrap();
        assert_eq!(
            crate::db::account_data::complete_account_deletion(&pool, "acct_1").unwrap(),
            crate::db::account_data::AccountDeletionCompletion::Deleted
        );
    }

    #[test]
    fn sessionless_account_put_is_discoverable_and_cannot_finalize_after_delete_fence() {
        let pool = test_pool();
        let input = account_object_input("resume-source", 1_000);
        let reservation = reserve_account_object_put(&pool, &input).unwrap();

        let refs = crate::db::account_data::artifact_object_refs(&pool, "acct_1").unwrap();
        assert!(refs
            .iter()
            .any(|reference| reference.object_key == input.object_key));

        assert!(crate::db::account_data::begin_account_deletion(&pool, "acct_1", 1_050).unwrap());
        let finalize_error =
            finalize_account_object_put(&pool, &reservation.upload.id, 1_100).unwrap_err();
        assert_eq!(
            finalize_error.downcast_ref::<UploadControlError>(),
            Some(&UploadControlError::UploadGone)
        );
        let reserve_error =
            reserve_account_object_put(&pool, &account_object_input("receipt-after-fence", 1_100))
                .unwrap_err();
        assert_eq!(
            reserve_error.downcast_ref::<UploadControlError>(),
            Some(&UploadControlError::UploadGone)
        );

        confirm_account_object_deleted(&pool, "acct_1", &input.object_key, 1_150).unwrap();
        assert_eq!(
            crate::db::account_data::complete_account_deletion(&pool, "acct_1").unwrap(),
            crate::db::account_data::AccountDeletionCompletion::Deleted
        );
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
