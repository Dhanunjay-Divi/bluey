//! Durable object metadata, quota reservations, and R2/S3 outbox state.

use anyhow::{Context, Result};
use postgres::{Row as PgRow, Transaction as PgTransaction};
use rusqlite::{params, OptionalExtension, Transaction as SqliteTransaction, TransactionBehavior};

use super::{
    account_data::{self, AccountWriteFence},
    DbPool, SafePostgresClient,
};
use crate::object_storage::{sha256_hex, UploadLimits};

const DAY_MS: i64 = 86_400_000;
const PROCESSING_LEASE_MS: i64 = 5 * 60 * 1000;

// `metadata_json` predates the Jobs evidence ledger and remains TEXT in
// PostgreSQL. Guard its conversion so one malformed legacy row cannot abort a
// cleanup batch. PostgreSQL 16+ guarantees `IS JSON OBJECT` returns false
// rather than raising for invalid text, and CASE evaluates the cast only for a
// valid JSON object.
const POSTGRES_SAFE_UPLOAD_METADATA_JOIN: &str = r#"
LEFT JOIN LATERAL (
    SELECT CASE
        WHEN upload.metadata_json IS JSON OBJECT THEN upload.metadata_json::jsonb
    END AS value
) parsed_metadata ON TRUE"#;

const POSTGRES_CLEANUP_CANDIDATE_PREDICATE: &str = r#"
((upload.state = 'ready' AND upload.expires_at_ms <= $3)
 OR (upload.state = 'pending' AND upload.updated_at_ms <= $2
     AND NOT EXISTS (
         SELECT 1
           FROM jobs_submission_evidence_capacity capacity
          WHERE capacity.account_id = upload.account_id
            AND capacity.state = 'active'
            AND capacity.expires_at_ms > $3
            AND capacity.application_id =
                (parsed_metadata.value ->> 'jobs_application_id')
            AND capacity.run_id = (parsed_metadata.value ->> 'jobs_run_id')
            AND capacity.runner = (parsed_metadata.value ->> 'jobs_runner')
            AND (parsed_metadata.value ->> 'artifact_class') =
                'jobs_submission_evidence'
     ))
 OR (upload.state = 'delete_pending'
     AND NOT EXISTS (
         SELECT 1
           FROM object_storage_outbox deletion
          WHERE deletion.upload_id = upload.id
            AND deletion.operation = 'delete'
            AND deletion.state IN ('pending', 'processing', 'retry')
     )))"#;

const POSTGRES_CLAIM_EXISTING_CLEANUP_SQL: &str = r#"
SELECT outbox.upload_id, upload.account_id, upload.object_key
  FROM object_storage_outbox outbox
  JOIN object_uploads upload ON upload.id = outbox.upload_id
 WHERE ($1::text IS NULL OR outbox.account_id = $1)
   AND upload.storage_scope = $2 AND upload.state = 'delete_pending'
   AND outbox.operation = 'delete'
   AND ((outbox.state IN ('pending', 'retry')
         AND outbox.next_attempt_at_ms <= $3)
     OR (outbox.state = 'processing' AND outbox.updated_at_ms <= $4))
 ORDER BY outbox.created_at_ms, outbox.id
 FOR UPDATE OF outbox SKIP LOCKED
 LIMIT $5"#;

fn postgres_cleanup_candidate_accounts_query() -> String {
    format!(
        "SELECT account_row.id
           FROM accounts account_row
          WHERE EXISTS (
                SELECT 1
                  FROM object_uploads upload
                  {POSTGRES_SAFE_UPLOAD_METADATA_JOIN}
                 WHERE upload.account_id = account_row.id
                   AND upload.storage_scope = $1
                   AND {POSTGRES_CLEANUP_CANDIDATE_PREDICATE}
          )
          ORDER BY account_row.id
          FOR UPDATE OF account_row SKIP LOCKED
          LIMIT $4"
    )
}

fn postgres_cleanup_candidates_query() -> String {
    format!(
        "SELECT upload.id, upload.account_id, upload.object_key
           FROM object_uploads upload
           {POSTGRES_SAFE_UPLOAD_METADATA_JOIN}
          WHERE upload.account_id = ANY($4::text[])
            AND upload.storage_scope = $1
            AND {POSTGRES_CLEANUP_CANDIDATE_PREDICATE}
          ORDER BY upload.created_at_ms, upload.id
          FOR UPDATE OF upload SKIP LOCKED
          LIMIT $5"
    )
}

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

#[derive(Debug, Clone)]
pub struct NewSubmissionEvidenceCapacity {
    pub account_id: String,
    pub application_id: String,
    pub run_id: String,
    pub runner: String,
    pub reserved_bytes: i64,
    pub reserved_objects: i64,
    pub expires_at_ms: i64,
    pub now_ms: i64,
    pub limits: UploadLimits,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubmissionEvidenceCapacity {
    pub account_id: String,
    pub application_id: String,
    pub run_id: String,
    pub runner: String,
    pub reserved_bytes: i64,
    pub reserved_objects: i64,
    pub consumed_bytes: i64,
    pub consumed_objects: i64,
    pub state: String,
    pub expires_at_ms: i64,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
    pub completed_at_ms: Option<i64>,
}

/// Exact pending object metadata carried into the Jobs submission transaction.
///
/// Jobs submission evidence is stored under the existing account artifact
/// scope, but unlike session artifacts it is parented by a Jobs application.
/// These bindings are verified against the durable upload ledger and promoted
/// from `pending` to `ready` in the same transaction that commits Submitted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplicationObjectBinding {
    pub upload_id: String,
    pub object_key: String,
    pub size_bytes: i64,
    pub sha256: String,
    pub content_type: String,
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
    #[error("account deletion is in progress")]
    AccountDeleting,
    #[error("submission evidence capacity is not active")]
    SubmissionEvidenceCapacityUnavailable,
    #[error("submission evidence exceeds its protected capacity")]
    SubmissionEvidenceCapacityExceeded,
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

/// Reserve a Jobs application evidence object before issuing the object-store
/// PUT. The row intentionally remains pending until `finalize_submission`
/// commits the application, evidence rows, and upload metadata atomically.
pub fn reserve_application_object_upload(
    pool: &DbPool,
    application_id: &str,
    input: &NewObjectUpload,
) -> Result<UploadReservation> {
    validate_application_object_input(application_id, input)?;
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => reserve_application_object_upload_sqlite(pool, application_id, input),
        DbPool::Postgres(_) => {
            reserve_application_object_upload_postgres(pool, application_id, input)
        }
    })
}

/// Reserve an account-parented Jobs object that has no session or application
/// parent. This path is intentionally limited to immutable source resumes and
/// encrypted browser-profile snapshots; all other artifacts must use their
/// stronger session- or application-scoped lifecycle.
pub fn reserve_account_object_upload(
    pool: &DbPool,
    input: &NewObjectUpload,
) -> Result<UploadReservation> {
    validate_account_object_input(input)?;
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => reserve_account_object_upload_sqlite(pool, input),
        DbPool::Postgres(_) => reserve_account_object_upload_postgres(pool, input),
    })
}

/// Reserve bounded account-level headroom before an employer-facing click.
/// Production callers must use the transaction-scoped variants below so this
/// reservation commits atomically with their irreversible authority change.
pub fn reserve_submission_evidence_capacity(
    pool: &DbPool,
    input: &NewSubmissionEvidenceCapacity,
) -> Result<SubmissionEvidenceCapacity> {
    validate_submission_evidence_capacity_input(input)?;
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let capacity = reserve_submission_evidence_capacity_sqlite_tx(&tx, input)?;
            tx.commit()?;
            Ok(capacity)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            let capacity = reserve_submission_evidence_capacity_postgres_tx(&mut tx, input)?;
            tx.commit()?;
            Ok(capacity)
        }
    })
}

/// Replay-safe release for a run that is proven not to have crossed the
/// irreversible boundary. `side_effect_unknown` callers must retain capacity
/// for reconciliation instead of invoking this function.
pub fn release_submission_evidence_capacity(
    pool: &DbPool,
    account_id: &str,
    application_id: &str,
    run_id: &str,
    now_ms: i64,
) -> Result<bool> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            if account_data::account_write_fence_sqlite_tx(&tx, account_id)?
                == AccountWriteFence::Missing
            {
                tx.commit()?;
                return Ok(false);
            }
            let released = release_submission_evidence_capacity_sqlite_tx(
                &tx,
                account_id,
                application_id,
                run_id,
                now_ms,
            )?;
            tx.commit()?;
            Ok(released)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            if account_data::account_write_fence_postgres_tx(&mut tx, account_id)?
                == AccountWriteFence::Missing
            {
                tx.commit()?;
                return Ok(false);
            }
            let released = release_submission_evidence_capacity_postgres_tx(
                &mut tx,
                account_id,
                application_id,
                run_id,
                now_ms,
            )?;
            tx.commit()?;
            Ok(released)
        }
    })
}

/// Renew the durable PUT lease immediately before issuing an object-store PUT.
///
/// The account write fence serializes this transition with account deletion.
/// A deletion request that commits first rejects the PUT and moves its pending
/// metadata to durable cleanup; a PUT begin that commits first refreshes the
/// pending upload inside the account-deletion freshness window.
pub fn begin_upload_put(pool: &DbPool, upload_id: &str, now_ms: i64) -> Result<ObjectUpload> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => begin_upload_put_sqlite(pool, upload_id, now_ms),
        DbPool::Postgres(_) => begin_upload_put_postgres(pool, upload_id, now_ms),
    })
}

/// Releases a successfully verified PUT lease while keeping a Jobs submission
/// object unpublished until the application and its complete receipt commit.
/// This makes an exact receipt retry immediately reusable after a later object
/// or database step fails, without exposing a partial evidence set as ready.
pub fn release_verified_upload_put(pool: &DbPool, upload_id: &str, now_ms: i64) -> Result<()> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => release_verified_upload_put_sqlite(pool, upload_id, now_ms),
        DbPool::Postgres(_) => release_verified_upload_put_postgres(pool, upload_id, now_ms),
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

/// Move one tracked object to durable delete-pending state. Callers may try the
/// DELETE immediately; any failure remains in the shared cleanup outbox.
pub fn schedule_upload_cleanup(pool: &DbPool, upload_id: &str, now_ms: i64) -> Result<CleanupJob> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => schedule_upload_cleanup_sqlite(pool, upload_id, now_ms),
        DbPool::Postgres(_) => schedule_upload_cleanup_postgres(pool, upload_id, now_ms),
    })
}

/// Durably schedule deletion of one account-parented Jobs object by its exact
/// account and object-store key. Missing or already-deleted objects are a
/// replay-safe `None`; this function never performs the physical DELETE.
pub fn schedule_account_object_cleanup(
    pool: &DbPool,
    account_id: &str,
    object_key: &str,
    now_ms: i64,
) -> Result<Option<CleanupJob>> {
    validate_account_object_cleanup_identity(account_id, object_key)?;
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            schedule_account_object_cleanup_sqlite(pool, account_id, object_key, now_ms)
        }
        DbPool::Postgres(_) => {
            schedule_account_object_cleanup_postgres(pool, account_id, object_key, now_ms)
        }
    })
}

/// Publish a verified account-parented object inside the same SQLite
/// transaction that installs its authoritative Jobs pointer.
pub(crate) fn publish_account_object_sqlite_tx(
    tx: &SqliteTransaction<'_>,
    upload_id: &str,
    account_id: &str,
    object_key: &str,
    now_ms: i64,
) -> Result<ObjectUpload> {
    require_active_account_write_fence_sqlite_tx(tx, account_id)?;
    let mut upload =
        load_upload_sqlite(tx, upload_id)?.ok_or(UploadControlError::UploadNotFound)?;
    validate_stored_account_object(&upload)?;
    if upload.account_id != account_id || upload.object_key != object_key {
        return Err(UploadControlError::IdempotencyConflict.into());
    }
    match upload.state.as_str() {
        "ready" => {
            let put_state = tx
                .query_row(
                    "SELECT state FROM object_storage_outbox
                      WHERE upload_id = ?1 AND operation = 'put'",
                    params![upload_id],
                    |row| row.get::<_, String>(0),
                )
                .optional()?;
            if put_state.as_deref() != Some("completed") {
                return Err(UploadControlError::IdempotencyConflict.into());
            }
            return Ok(upload);
        }
        "pending" => {}
        "delete_pending" | "deleted" => return Err(UploadControlError::UploadGone.into()),
        _ => return Err(UploadControlError::IdempotencyConflict.into()),
    }
    if tx.execute(
        "UPDATE object_uploads
            SET state = 'ready', uploaded_at_ms = COALESCE(uploaded_at_ms, ?2),
                updated_at_ms = ?2
          WHERE id = ?1 AND state = 'pending'",
        params![upload_id, now_ms],
    )? != 1
        || tx.execute(
            "UPDATE object_storage_outbox
                SET state = 'completed', updated_at_ms = ?2, completed_at_ms = ?2,
                    next_attempt_at_ms = ?2, last_error = NULL
              WHERE upload_id = ?1 AND operation = 'put'
                AND state IN ('pending', 'processing', 'retry')",
            params![upload_id, now_ms],
        )? != 1
    {
        return Err(UploadControlError::UploadInProgress.into());
    }
    upload.state = "ready".to_string();
    upload.updated_at_ms = now_ms;
    upload.uploaded_at_ms.get_or_insert(now_ms);
    Ok(upload)
}

/// PostgreSQL counterpart to `publish_account_object_sqlite_tx`. Call this
/// before locking Jobs child rows so the order remains logical object,
/// account, upload, then authoritative pointer.
pub(crate) fn publish_account_object_postgres_tx(
    tx: &mut PgTransaction<'_>,
    upload_id: &str,
    account_id: &str,
    object_key: &str,
    now_ms: i64,
) -> Result<ObjectUpload> {
    let identity =
        load_upload_postgres_unlocked(tx, upload_id)?.ok_or(UploadControlError::UploadNotFound)?;
    validate_stored_account_object(&identity)?;
    if identity.account_id != account_id || identity.object_key != object_key {
        return Err(UploadControlError::IdempotencyConflict.into());
    }
    lock_context_artifact_postgres_tx(tx, &identity.logical_id)?;
    require_active_account_write_fence_postgres_tx(tx, account_id)?;
    let mut upload =
        load_upload_postgres(tx, upload_id)?.ok_or(UploadControlError::UploadNotFound)?;
    validate_stored_account_object(&upload)?;
    if upload.id != identity.id
        || upload.logical_id != identity.logical_id
        || upload.object_key != object_key
    {
        return Err(UploadControlError::IdempotencyConflict.into());
    }
    match upload.state.as_str() {
        "ready" => {
            let put_state = tx
                .query_opt(
                    "SELECT state FROM object_storage_outbox
                      WHERE upload_id = $1 AND operation = 'put' FOR UPDATE",
                    &[&upload_id],
                )?
                .map(|row| row.get::<_, String>(0));
            if put_state.as_deref() != Some("completed") {
                return Err(UploadControlError::IdempotencyConflict.into());
            }
            return Ok(upload);
        }
        "pending" => {}
        "delete_pending" | "deleted" => return Err(UploadControlError::UploadGone.into()),
        _ => return Err(UploadControlError::IdempotencyConflict.into()),
    }
    if tx.execute(
        "UPDATE object_uploads
            SET state = 'ready', uploaded_at_ms = COALESCE(uploaded_at_ms, $2),
                updated_at_ms = $2
          WHERE id = $1 AND state = 'pending'",
        &[&upload_id, &now_ms],
    )? != 1
        || tx.execute(
            "UPDATE object_storage_outbox
                SET state = 'completed', updated_at_ms = $2, completed_at_ms = $2,
                    next_attempt_at_ms = $2, last_error = NULL
              WHERE upload_id = $1 AND operation = 'put'
                AND state IN ('pending', 'processing', 'retry')",
            &[&upload_id, &now_ms],
        )? != 1
    {
        return Err(UploadControlError::UploadInProgress.into());
    }
    upload.state = "ready".to_string();
    upload.updated_at_ms = now_ms;
    upload.uploaded_at_ms.get_or_insert(now_ms);
    Ok(upload)
}

/// Adopt an account object written by a pre-ledger server before replacing its
/// Jobs pointer. The caller must already hold the account write fence and the
/// authoritative child pointer in the same transaction. Adoption intentionally
/// does not charge daily upload usage because the bytes already existed before
/// the durable ledger was introduced.
pub(crate) fn adopt_ready_account_object_sqlite_tx(
    tx: &SqliteTransaction<'_>,
    input: &NewObjectUpload,
) -> Result<ObjectUpload> {
    validate_account_object_input(input)?;
    require_active_account_write_fence_sqlite_tx(tx, &input.account_id)?;
    let upload_id = stable_upload_id(input);
    let metadata_json = serde_json::to_string(&input.metadata_json)?;
    tx.execute(
        "INSERT INTO object_uploads (
            id, account_id, object_kind, logical_id, session_id, storage_scope,
            object_key, size_bytes, sha256, content_type, expires_at_ms, state,
            metadata_json, created_at_ms, updated_at_ms, uploaded_at_ms, deleted_at_ms
         ) VALUES (?1, ?2, ?3, ?4, NULL, ?5, ?6, ?7, ?8, ?9, ?10, 'ready',
                   ?11, ?12, ?12, ?12, NULL)",
        params![
            upload_id,
            input.account_id,
            input.object_kind.as_str(),
            input.logical_id,
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
    insert_completed_put_outbox_sqlite(tx, &upload_id, &input.account_id, input.now_ms)?;
    load_upload_sqlite(tx, &upload_id)?.ok_or_else(|| UploadControlError::UploadNotFound.into())
}

/// PostgreSQL counterpart to `adopt_ready_account_object_sqlite_tx`. The
/// account row held by the caller serializes this insertion with contemporary
/// reservations, including an old replica that took the logical advisory lock
/// before waiting on that account row.
pub(crate) fn adopt_ready_account_object_postgres_tx(
    tx: &mut PgTransaction<'_>,
    input: &NewObjectUpload,
) -> Result<ObjectUpload> {
    validate_account_object_input(input)?;
    require_active_account_write_fence_postgres_tx(tx, &input.account_id)?;
    let upload_id = stable_upload_id(input);
    let metadata_json = serde_json::to_string(&input.metadata_json)?;
    tx.execute(
        "INSERT INTO object_uploads (
            id, account_id, object_kind, logical_id, session_id, storage_scope,
            object_key, size_bytes, sha256, content_type, expires_at_ms, state,
            metadata_json, created_at_ms, updated_at_ms, uploaded_at_ms, deleted_at_ms
         ) VALUES ($1, $2, $3, $4, NULL, $5, $6, $7, $8, $9, $10, 'ready',
                   $11, $12, $12, $12, NULL)",
        &[
            &upload_id,
            &input.account_id,
            &input.object_kind.as_str(),
            &input.logical_id,
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
    insert_completed_put_outbox_postgres(tx, &upload_id, &input.account_id, input.now_ms)?;
    load_upload_postgres(tx, &upload_id)?.ok_or_else(|| UploadControlError::UploadNotFound.into())
}

pub(crate) fn schedule_account_object_cleanup_sqlite_tx(
    tx: &SqliteTransaction<'_>,
    account_id: &str,
    object_key: &str,
    now_ms: i64,
) -> Result<Option<CleanupJob>> {
    let Some(upload) = load_account_object_by_key_sqlite(tx, account_id, object_key)? else {
        return Ok(None);
    };
    validate_stored_account_object(&upload)?;
    if upload.state == "deleted" {
        return Ok(None);
    }
    schedule_upload_cleanup_sqlite_tx(tx, &upload.id, now_ms).map(Some)
}

pub(crate) fn schedule_account_object_cleanup_postgres_tx(
    tx: &mut PgTransaction<'_>,
    account_id: &str,
    object_key: &str,
    now_ms: i64,
) -> Result<Option<CleanupJob>> {
    let Some(upload) = load_account_object_by_key_postgres(tx, account_id, object_key)? else {
        return Ok(None);
    };
    validate_stored_account_object(&upload)?;
    if upload.state == "deleted" {
        return Ok(None);
    }
    schedule_upload_cleanup_postgres_tx(tx, &upload.id, now_ms).map(Some)
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
            AND state IN ('pending', 'processing', 'retry')
            AND upload_id IN (
                SELECT id
                  FROM object_uploads
                 WHERE account_id = ?1
                   AND object_kind = 'artifact'
                   AND logical_id = ?2
            )",
        params![account_id, artifact_id, now_ms],
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
            AND state IN ('pending', 'processing', 'retry')
            AND upload_id IN (
                SELECT id
                  FROM object_uploads
                 WHERE account_id = $1
                   AND object_kind = 'artifact'
                   AND logical_id = $2
            )",
        &[&account_id, &artifact_id, &now_ms],
    )?;
    Ok(changed as usize)
}

fn schedule_account_object_cleanup_sqlite(
    pool: &DbPool,
    account_id: &str,
    object_key: &str,
    now_ms: i64,
) -> Result<Option<CleanupJob>> {
    let mut conn = pool.get()?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    if account_data::account_write_fence_sqlite_tx(&tx, account_id)? == AccountWriteFence::Missing {
        tx.commit()?;
        return Ok(None);
    }
    let Some(upload) = load_account_object_by_key_sqlite(&tx, account_id, object_key)? else {
        tx.commit()?;
        return Ok(None);
    };
    validate_stored_account_object(&upload)?;
    if upload.state == "deleted" {
        tx.commit()?;
        return Ok(None);
    }
    let job = schedule_upload_cleanup_sqlite_tx(&tx, &upload.id, now_ms)?;
    tx.commit()?;
    Ok(Some(job))
}

fn schedule_account_object_cleanup_postgres(
    pool: &DbPool,
    account_id: &str,
    object_key: &str,
    now_ms: i64,
) -> Result<Option<CleanupJob>> {
    let mut conn = pool.get_pg()?;
    let mut tx = conn.transaction()?;
    // Resolve the immutable logical identity without taking a row lock. The
    // actual lock order remains logical advisory -> account -> upload, matching
    // every competing account-object reservation and account deletion fence.
    let Some(identity) =
        load_account_object_by_key_postgres_unlocked(&mut tx, account_id, object_key)?
    else {
        tx.commit()?;
        return Ok(None);
    };
    validate_stored_account_object(&identity)?;
    lock_context_artifact_postgres_tx(&mut tx, &identity.logical_id)?;
    if account_data::account_write_fence_postgres_tx(&mut tx, account_id)?
        == AccountWriteFence::Missing
    {
        tx.commit()?;
        return Ok(None);
    }
    let Some(upload) = load_account_object_by_key_postgres(&mut tx, account_id, object_key)? else {
        tx.commit()?;
        return Ok(None);
    };
    validate_stored_account_object(&upload)?;
    if upload.id != identity.id || upload.logical_id != identity.logical_id {
        return Err(UploadControlError::IdempotencyConflict.into());
    }
    if upload.state == "deleted" {
        tx.commit()?;
        return Ok(None);
    }
    let job = schedule_upload_cleanup_postgres_tx(&mut tx, &upload.id, now_ms)?;
    tx.commit()?;
    Ok(Some(job))
}

fn schedule_upload_cleanup_sqlite(
    pool: &DbPool,
    upload_id: &str,
    now_ms: i64,
) -> Result<CleanupJob> {
    let mut conn = pool.get()?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let job = schedule_upload_cleanup_sqlite_tx(&tx, upload_id, now_ms)?;
    tx.commit()?;
    Ok(job)
}

fn schedule_upload_cleanup_postgres(
    pool: &DbPool,
    upload_id: &str,
    now_ms: i64,
) -> Result<CleanupJob> {
    let mut conn = pool.get_pg()?;
    let mut tx = conn.transaction()?;
    let job = schedule_upload_cleanup_postgres_tx(&mut tx, upload_id, now_ms)?;
    tx.commit()?;
    Ok(job)
}

fn schedule_upload_cleanup_sqlite_tx(
    tx: &SqliteTransaction<'_>,
    upload_id: &str,
    now_ms: i64,
) -> Result<CleanupJob> {
    let upload = load_upload_sqlite(tx, upload_id)?.ok_or(UploadControlError::UploadNotFound)?;
    if upload.state == "deleted" {
        return Err(UploadControlError::UploadGone.into());
    }
    tx.execute(
        "UPDATE object_uploads
            SET state = 'delete_pending', updated_at_ms = ?2
          WHERE id = ?1 AND state IN ('pending', 'ready')",
        params![upload_id, now_ms],
    )?;
    tx.execute(
        "INSERT INTO object_storage_outbox (
            id, upload_id, account_id, operation, state, next_attempt_at_ms,
            created_at_ms, updated_at_ms
         ) VALUES (?1 || ':delete', ?1, ?2, 'delete', 'pending', ?3, ?3, ?3)
         ON CONFLICT(upload_id, operation) DO NOTHING",
        params![upload_id, upload.account_id, now_ms],
    )?;
    tx.execute(
        "UPDATE object_storage_outbox
            SET state = 'abandoned', last_error = 'upload abandoned before publication',
                updated_at_ms = ?2, completed_at_ms = ?2
          WHERE upload_id = ?1 AND operation = 'put' AND state <> 'completed'",
        params![upload_id, now_ms],
    )?;
    Ok(CleanupJob {
        upload_id: upload.id,
        account_id: upload.account_id,
        object_key: upload.object_key,
    })
}

fn schedule_upload_cleanup_postgres_tx(
    tx: &mut PgTransaction<'_>,
    upload_id: &str,
    now_ms: i64,
) -> Result<CleanupJob> {
    let upload = load_upload_postgres(tx, upload_id)?.ok_or(UploadControlError::UploadNotFound)?;
    if upload.state == "deleted" {
        return Err(UploadControlError::UploadGone.into());
    }
    tx.execute(
        "UPDATE object_uploads
            SET state = 'delete_pending', updated_at_ms = $2
          WHERE id = $1 AND state IN ('pending', 'ready')",
        &[&upload_id, &now_ms],
    )?;
    tx.execute(
        "INSERT INTO object_storage_outbox (
            id, upload_id, account_id, operation, state, next_attempt_at_ms,
            created_at_ms, updated_at_ms
         ) VALUES ($1 || ':delete', $1, $2, 'delete', 'pending', $3, $3, $3)
         ON CONFLICT(upload_id, operation) DO NOTHING",
        &[&upload_id, &upload.account_id, &now_ms],
    )?;
    tx.execute(
        "UPDATE object_storage_outbox
            SET state = 'abandoned', last_error = 'upload abandoned before publication',
                updated_at_ms = $2, completed_at_ms = $2
          WHERE upload_id = $1 AND operation = 'put' AND state <> 'completed'",
        &[&upload_id, &now_ms],
    )?;
    Ok(CleanupJob {
        upload_id: upload.id,
        account_id: upload.account_id,
        object_key: upload.object_key,
    })
}

pub(crate) fn require_active_account_write_fence_sqlite_tx(
    tx: &SqliteTransaction<'_>,
    account_id: &str,
) -> Result<()> {
    match account_data::account_write_fence_sqlite_tx(tx, account_id)? {
        AccountWriteFence::Active => Ok(()),
        AccountWriteFence::DeletionRequested => Err(UploadControlError::AccountDeleting.into()),
        AccountWriteFence::Missing => Err(UploadControlError::SessionNotOwned.into()),
    }
}

/// Acquire the account row before any child row that account deletion may
/// cascade through. Callers that own a larger transaction (notably Jobs final
/// submission) must invoke this before locking application or lease rows.
pub(crate) fn require_active_account_write_fence_postgres_tx(
    tx: &mut PgTransaction<'_>,
    account_id: &str,
) -> Result<()> {
    match account_data::account_write_fence_postgres_tx(tx, account_id)? {
        AccountWriteFence::Active => Ok(()),
        AccountWriteFence::DeletionRequested => Err(UploadControlError::AccountDeleting.into()),
        AccountWriteFence::Missing => Err(UploadControlError::SessionNotOwned.into()),
    }
}

fn validate_submission_evidence_capacity_input(
    input: &NewSubmissionEvidenceCapacity,
) -> Result<()> {
    if input.account_id.trim().is_empty()
        || input.application_id.trim().is_empty()
        || input.application_id.len() > 128
        || input.run_id.trim().is_empty()
        || input.run_id.len() > 128
        || !matches!(input.runner.as_str(), "cloud" | "local")
        || input.reserved_bytes <= 0
        || input.reserved_objects <= 0
        || input.limits.max_account_bytes <= 0
        || input.limits.max_account_objects <= 0
        || input.reserved_bytes > input.limits.max_account_bytes
        || input.reserved_objects > input.limits.max_account_objects
        || input.expires_at_ms <= input.now_ms
        || input.expires_at_ms > input.now_ms.saturating_add(DAY_MS)
    {
        return Err(UploadControlError::InvalidMetadata("submission evidence capacity").into());
    }
    Ok(())
}

pub(crate) fn reserve_submission_evidence_capacity_sqlite_tx(
    tx: &SqliteTransaction<'_>,
    input: &NewSubmissionEvidenceCapacity,
) -> Result<SubmissionEvidenceCapacity> {
    validate_submission_evidence_capacity_input(input)?;
    require_active_account_write_fence_sqlite_tx(tx, &input.account_id)?;
    expire_submission_evidence_capacity_sqlite_tx(tx, &input.account_id, input.now_ms)?;
    let application_owned = tx
        .query_row(
            "SELECT 1 FROM jobs_applications WHERE account_id = ?1 AND id = ?2",
            params![input.account_id, input.application_id],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    if !application_owned {
        return Err(UploadControlError::SessionNotOwned.into());
    }
    if let Some(existing) = load_submission_evidence_capacity_sqlite(
        tx,
        &input.account_id,
        &input.application_id,
        &input.run_id,
    )? {
        if existing.state == "active"
            && existing.expires_at_ms > input.now_ms
            && existing.runner == input.runner
            && existing.reserved_bytes == input.reserved_bytes
            && existing.reserved_objects == input.reserved_objects
        {
            return Ok(existing);
        }
        return Err(UploadControlError::IdempotencyConflict.into());
    }
    enforce_protected_capacity_total_sqlite(tx, input)?;
    tx.execute(
        "INSERT INTO jobs_submission_evidence_capacity (
            account_id, application_id, run_id, runner, reserved_bytes, reserved_objects,
            consumed_bytes, consumed_objects, state, expires_at_ms,
            created_at_ms, updated_at_ms
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0, 0, 'active', ?7, ?8, ?8)",
        params![
            input.account_id,
            input.application_id,
            input.run_id,
            input.runner,
            input.reserved_bytes,
            input.reserved_objects,
            input.expires_at_ms,
            input.now_ms,
        ],
    )?;
    load_submission_evidence_capacity_sqlite(
        tx,
        &input.account_id,
        &input.application_id,
        &input.run_id,
    )?
    .ok_or_else(|| UploadControlError::SubmissionEvidenceCapacityUnavailable.into())
}

pub(crate) fn reserve_submission_evidence_capacity_postgres_tx(
    tx: &mut PgTransaction<'_>,
    input: &NewSubmissionEvidenceCapacity,
) -> Result<SubmissionEvidenceCapacity> {
    validate_submission_evidence_capacity_input(input)?;
    require_active_account_write_fence_postgres_tx(tx, &input.account_id)?;
    expire_submission_evidence_capacity_postgres_tx(tx, &input.account_id, input.now_ms)?;
    let application_owned = tx
        .query_opt(
            "SELECT 1 FROM jobs_applications
              WHERE account_id = $1 AND id = $2 FOR SHARE",
            &[&input.account_id, &input.application_id],
        )?
        .is_some();
    if !application_owned {
        return Err(UploadControlError::SessionNotOwned.into());
    }
    if let Some(existing) = load_submission_evidence_capacity_postgres(
        tx,
        &input.account_id,
        &input.application_id,
        &input.run_id,
    )? {
        if existing.state == "active"
            && existing.expires_at_ms > input.now_ms
            && existing.runner == input.runner
            && existing.reserved_bytes == input.reserved_bytes
            && existing.reserved_objects == input.reserved_objects
        {
            return Ok(existing);
        }
        return Err(UploadControlError::IdempotencyConflict.into());
    }
    enforce_protected_capacity_total_postgres(tx, input)?;
    tx.execute(
        "INSERT INTO jobs_submission_evidence_capacity (
            account_id, application_id, run_id, runner, reserved_bytes, reserved_objects,
            consumed_bytes, consumed_objects, state, expires_at_ms,
            created_at_ms, updated_at_ms
         ) VALUES ($1, $2, $3, $4, $5, $6, 0, 0, 'active', $7, $8, $8)",
        &[
            &input.account_id,
            &input.application_id,
            &input.run_id,
            &input.runner,
            &input.reserved_bytes,
            &input.reserved_objects,
            &input.expires_at_ms,
            &input.now_ms,
        ],
    )?;
    load_submission_evidence_capacity_postgres(
        tx,
        &input.account_id,
        &input.application_id,
        &input.run_id,
    )?
    .ok_or_else(|| UploadControlError::SubmissionEvidenceCapacityUnavailable.into())
}

pub(crate) fn release_submission_evidence_capacity_sqlite_tx(
    tx: &SqliteTransaction<'_>,
    account_id: &str,
    application_id: &str,
    run_id: &str,
    now_ms: i64,
) -> Result<bool> {
    Ok(tx.execute(
        "UPDATE jobs_submission_evidence_capacity
            SET state = 'released', updated_at_ms = ?4, completed_at_ms = ?4
          WHERE account_id = ?1 AND application_id = ?2 AND run_id = ?3
            AND state = 'active'",
        params![account_id, application_id, run_id, now_ms],
    )? == 1)
}

/// The owning transaction must acquire the account row before any Jobs child
/// rows, then call this while releasing its execution authority.
pub(crate) fn release_submission_evidence_capacity_postgres_tx(
    tx: &mut PgTransaction<'_>,
    account_id: &str,
    application_id: &str,
    run_id: &str,
    now_ms: i64,
) -> Result<bool> {
    Ok(tx.execute(
        "UPDATE jobs_submission_evidence_capacity
            SET state = 'released', updated_at_ms = $4, completed_at_ms = $4
          WHERE account_id = $1 AND application_id = $2 AND run_id = $3
            AND state = 'active'",
        &[&account_id, &application_id, &run_id, &now_ms],
    )? == 1)
}

/// Monotonically extend the exact active capacity while the owning execution
/// authority is still live. The caller owns the account/application lock order
/// and decides whether a missing exact row is a domain conflict.
pub(crate) fn extend_submission_evidence_capacity_expiry_sqlite_tx(
    tx: &SqliteTransaction<'_>,
    account_id: &str,
    application_id: &str,
    run_id: &str,
    runner: &str,
    expires_at_ms: i64,
    now_ms: i64,
) -> Result<bool> {
    validate_submission_evidence_capacity_expiry(runner, expires_at_ms, now_ms)?;
    Ok(tx.execute(
        "UPDATE jobs_submission_evidence_capacity
            SET expires_at_ms = MAX(expires_at_ms, ?5),
                updated_at_ms = MAX(updated_at_ms, ?6)
          WHERE account_id = ?1 AND application_id = ?2 AND run_id = ?3
            AND runner = ?4 AND state = 'active' AND expires_at_ms > ?6",
        params![
            account_id,
            application_id,
            run_id,
            runner,
            expires_at_ms,
            now_ms,
        ],
    )? == 1)
}

/// PostgreSQL counterpart to
/// [`extend_submission_evidence_capacity_expiry_sqlite_tx`].
pub(crate) fn extend_submission_evidence_capacity_expiry_postgres_tx(
    tx: &mut PgTransaction<'_>,
    account_id: &str,
    application_id: &str,
    run_id: &str,
    runner: &str,
    expires_at_ms: i64,
    now_ms: i64,
) -> Result<bool> {
    validate_submission_evidence_capacity_expiry(runner, expires_at_ms, now_ms)?;
    Ok(tx.execute(
        "UPDATE jobs_submission_evidence_capacity
            SET expires_at_ms = GREATEST(expires_at_ms, $5),
                updated_at_ms = GREATEST(updated_at_ms, $6)
          WHERE account_id = $1 AND application_id = $2 AND run_id = $3
            AND runner = $4 AND state = 'active' AND expires_at_ms > $6",
        &[
            &account_id,
            &application_id,
            &run_id,
            &runner,
            &expires_at_ms,
            &now_ms,
        ],
    )? == 1)
}

/// Extends only the exact active capacity described by `expected`. This is
/// used after an irreversible boundary, where creating replacement capacity
/// would be too late and a differently sized reservation must fail closed.
pub(crate) fn extend_exact_submission_evidence_capacity_expiry_sqlite_tx(
    tx: &SqliteTransaction<'_>,
    expected: &NewSubmissionEvidenceCapacity,
    expires_at_ms: i64,
    now_ms: i64,
) -> Result<bool> {
    validate_submission_evidence_capacity_input(expected)?;
    validate_submission_evidence_capacity_expiry(&expected.runner, expires_at_ms, now_ms)?;
    Ok(tx.execute(
        "UPDATE jobs_submission_evidence_capacity
            SET expires_at_ms = MAX(expires_at_ms, ?7),
                updated_at_ms = MAX(updated_at_ms, ?8)
          WHERE account_id = ?1 AND application_id = ?2 AND run_id = ?3
            AND runner = ?4 AND reserved_bytes = ?5 AND reserved_objects = ?6
            AND state = 'active' AND expires_at_ms > ?8",
        params![
            expected.account_id,
            expected.application_id,
            expected.run_id,
            expected.runner,
            expected.reserved_bytes,
            expected.reserved_objects,
            expires_at_ms,
            now_ms,
        ],
    )? == 1)
}

/// PostgreSQL counterpart to
/// [`extend_exact_submission_evidence_capacity_expiry_sqlite_tx`].
pub(crate) fn extend_exact_submission_evidence_capacity_expiry_postgres_tx(
    tx: &mut PgTransaction<'_>,
    expected: &NewSubmissionEvidenceCapacity,
    expires_at_ms: i64,
    now_ms: i64,
) -> Result<bool> {
    validate_submission_evidence_capacity_input(expected)?;
    validate_submission_evidence_capacity_expiry(&expected.runner, expires_at_ms, now_ms)?;
    Ok(tx.execute(
        "UPDATE jobs_submission_evidence_capacity
            SET expires_at_ms = GREATEST(expires_at_ms, $7),
                updated_at_ms = GREATEST(updated_at_ms, $8)
          WHERE account_id = $1 AND application_id = $2 AND run_id = $3
            AND runner = $4 AND reserved_bytes = $5 AND reserved_objects = $6
            AND state = 'active' AND expires_at_ms > $8",
        &[
            &expected.account_id,
            &expected.application_id,
            &expected.run_id,
            &expected.runner,
            &expected.reserved_bytes,
            &expected.reserved_objects,
            &expires_at_ms,
            &now_ms,
        ],
    )? == 1)
}

/// Rebind an exact active capacity to a shorter or longer expiry. This is only
/// for an owning execution-authority transaction that atomically narrows its
/// own receipt window (for example Submitted -> side-effect-unknown recovery).
pub(crate) fn rebind_submission_evidence_capacity_expiry_sqlite_tx(
    tx: &SqliteTransaction<'_>,
    account_id: &str,
    application_id: &str,
    run_id: &str,
    runner: &str,
    expires_at_ms: i64,
    now_ms: i64,
) -> Result<bool> {
    validate_submission_evidence_capacity_expiry(runner, expires_at_ms, now_ms)?;
    Ok(tx.execute(
        "UPDATE jobs_submission_evidence_capacity
            SET expires_at_ms = ?5, updated_at_ms = MAX(updated_at_ms, ?6)
          WHERE account_id = ?1 AND application_id = ?2 AND run_id = ?3
            AND runner = ?4 AND state = 'active' AND expires_at_ms > ?6",
        params![
            account_id,
            application_id,
            run_id,
            runner,
            expires_at_ms,
            now_ms,
        ],
    )? == 1)
}

/// PostgreSQL counterpart to
/// [`rebind_submission_evidence_capacity_expiry_sqlite_tx`].
pub(crate) fn rebind_submission_evidence_capacity_expiry_postgres_tx(
    tx: &mut PgTransaction<'_>,
    account_id: &str,
    application_id: &str,
    run_id: &str,
    runner: &str,
    expires_at_ms: i64,
    now_ms: i64,
) -> Result<bool> {
    validate_submission_evidence_capacity_expiry(runner, expires_at_ms, now_ms)?;
    Ok(tx.execute(
        "UPDATE jobs_submission_evidence_capacity
            SET expires_at_ms = $5, updated_at_ms = GREATEST(updated_at_ms, $6)
          WHERE account_id = $1 AND application_id = $2 AND run_id = $3
            AND runner = $4 AND state = 'active' AND expires_at_ms > $6",
        &[
            &account_id,
            &application_id,
            &run_id,
            &runner,
            &expires_at_ms,
            &now_ms,
        ],
    )? == 1)
}

fn validate_submission_evidence_capacity_expiry(
    runner: &str,
    expires_at_ms: i64,
    now_ms: i64,
) -> Result<()> {
    if !matches!(runner, "cloud" | "local") || now_ms < 0 || expires_at_ms <= now_ms {
        return Err(
            UploadControlError::InvalidMetadata("submission evidence capacity expiry").into(),
        );
    }
    Ok(())
}

fn expire_submission_evidence_capacity_sqlite_tx(
    tx: &SqliteTransaction<'_>,
    account_id: &str,
    now_ms: i64,
) -> Result<()> {
    tx.execute(
        "UPDATE jobs_submission_evidence_capacity
            SET state = 'expired', updated_at_ms = ?2, completed_at_ms = ?2
          WHERE account_id = ?1 AND state = 'active' AND expires_at_ms <= ?2",
        params![account_id, now_ms],
    )?;
    Ok(())
}

fn expire_submission_evidence_capacity_postgres_tx(
    tx: &mut PgTransaction<'_>,
    account_id: &str,
    now_ms: i64,
) -> Result<()> {
    tx.execute(
        "UPDATE jobs_submission_evidence_capacity
            SET state = 'expired', updated_at_ms = $2, completed_at_ms = $2
          WHERE account_id = $1 AND state = 'active' AND expires_at_ms <= $2",
        &[&account_id, &now_ms],
    )?;
    Ok(())
}

fn enforce_protected_capacity_total_sqlite(
    tx: &SqliteTransaction<'_>,
    input: &NewSubmissionEvidenceCapacity,
) -> Result<()> {
    let (bytes, objects) = effective_account_usage_sqlite(tx, &input.account_id, input.now_ms)?;
    enforce_capacity_total_limits(bytes, objects, input)
}

fn enforce_protected_capacity_total_postgres(
    tx: &mut PgTransaction<'_>,
    input: &NewSubmissionEvidenceCapacity,
) -> Result<()> {
    let (bytes, objects) = effective_account_usage_postgres(tx, &input.account_id, input.now_ms)?;
    enforce_capacity_total_limits(bytes, objects, input)
}

fn enforce_capacity_total_limits(
    bytes: i64,
    objects: i64,
    input: &NewSubmissionEvidenceCapacity,
) -> Result<()> {
    if exceeds(bytes, input.reserved_bytes, input.limits.max_account_bytes) {
        return Err(UploadControlError::AccountBytesQuotaExceeded.into());
    }
    if exceeds(
        objects,
        input.reserved_objects,
        input.limits.max_account_objects,
    ) {
        return Err(UploadControlError::AccountObjectQuotaExceeded.into());
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn consume_submission_evidence_capacity_sqlite_tx(
    tx: &SqliteTransaction<'_>,
    account_id: &str,
    application_id: &str,
    run_id: &str,
    runner: &str,
    bytes: i64,
    objects: i64,
    now_ms: i64,
) -> Result<()> {
    let capacity =
        load_submission_evidence_capacity_sqlite(tx, account_id, application_id, run_id)?
            .ok_or(UploadControlError::SubmissionEvidenceCapacityUnavailable)?;
    validate_capacity_consumption(&capacity, runner, bytes, objects, now_ms)?;
    if bytes == 0 && objects == 0 {
        return Ok(());
    }
    if tx.execute(
        "UPDATE jobs_submission_evidence_capacity
            SET consumed_bytes = consumed_bytes + ?5,
                consumed_objects = consumed_objects + ?6,
                updated_at_ms = ?7
          WHERE account_id = ?1 AND application_id = ?2 AND run_id = ?3
            AND runner = ?4 AND state = 'active' AND expires_at_ms > ?7
            AND consumed_bytes <= reserved_bytes - ?5
            AND consumed_objects <= reserved_objects - ?6",
        params![
            account_id,
            application_id,
            run_id,
            runner,
            bytes,
            objects,
            now_ms,
        ],
    )? != 1
    {
        return Err(UploadControlError::SubmissionEvidenceCapacityExceeded.into());
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn consume_submission_evidence_capacity_postgres_tx(
    tx: &mut PgTransaction<'_>,
    account_id: &str,
    application_id: &str,
    run_id: &str,
    runner: &str,
    bytes: i64,
    objects: i64,
    now_ms: i64,
) -> Result<()> {
    let capacity =
        load_submission_evidence_capacity_postgres(tx, account_id, application_id, run_id)?
            .ok_or(UploadControlError::SubmissionEvidenceCapacityUnavailable)?;
    validate_capacity_consumption(&capacity, runner, bytes, objects, now_ms)?;
    if bytes == 0 && objects == 0 {
        return Ok(());
    }
    if tx.execute(
        "UPDATE jobs_submission_evidence_capacity
            SET consumed_bytes = consumed_bytes + $5,
                consumed_objects = consumed_objects + $6,
                updated_at_ms = $7
          WHERE account_id = $1 AND application_id = $2 AND run_id = $3
            AND runner = $4 AND state = 'active' AND expires_at_ms > $7
            AND consumed_bytes <= reserved_bytes - $5
            AND consumed_objects <= reserved_objects - $6",
        &[
            &account_id,
            &application_id,
            &run_id,
            &runner,
            &bytes,
            &objects,
            &now_ms,
        ],
    )? != 1
    {
        return Err(UploadControlError::SubmissionEvidenceCapacityExceeded.into());
    }
    Ok(())
}

fn validate_capacity_consumption(
    capacity: &SubmissionEvidenceCapacity,
    runner: &str,
    bytes: i64,
    objects: i64,
    now_ms: i64,
) -> Result<()> {
    if capacity.state != "active" || capacity.expires_at_ms <= now_ms || capacity.runner != runner {
        return Err(UploadControlError::SubmissionEvidenceCapacityUnavailable.into());
    }
    if bytes < 0
        || objects < 0
        || exceeds(capacity.consumed_bytes, bytes, capacity.reserved_bytes)
        || exceeds(
            capacity.consumed_objects,
            objects,
            capacity.reserved_objects,
        )
    {
        return Err(UploadControlError::SubmissionEvidenceCapacityExceeded.into());
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn commit_submission_evidence_capacity_sqlite_tx(
    tx: &SqliteTransaction<'_>,
    account_id: &str,
    application_id: &str,
    run_id: &str,
    runner: &str,
    committed_bytes: i64,
    committed_objects: i64,
    now_ms: i64,
) -> Result<()> {
    let capacity =
        load_submission_evidence_capacity_sqlite(tx, account_id, application_id, run_id)?
            .ok_or(UploadControlError::SubmissionEvidenceCapacityUnavailable)?;
    validate_capacity_commit(
        &capacity,
        runner,
        committed_bytes,
        committed_objects,
        now_ms,
    )?;
    if tx.execute(
        "UPDATE jobs_submission_evidence_capacity
            SET consumed_bytes = ?5, consumed_objects = ?6,
                state = 'committed', updated_at_ms = ?7, completed_at_ms = ?7
          WHERE account_id = ?1 AND application_id = ?2 AND run_id = ?3
            AND runner = ?4 AND state = 'active' AND expires_at_ms > ?7
            AND consumed_bytes >= ?5 AND consumed_objects >= ?6",
        params![
            account_id,
            application_id,
            run_id,
            runner,
            committed_bytes,
            committed_objects,
            now_ms,
        ],
    )? != 1
    {
        return Err(UploadControlError::SubmissionEvidenceCapacityUnavailable.into());
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn commit_submission_evidence_capacity_postgres_tx(
    tx: &mut PgTransaction<'_>,
    account_id: &str,
    application_id: &str,
    run_id: &str,
    runner: &str,
    committed_bytes: i64,
    committed_objects: i64,
    now_ms: i64,
) -> Result<()> {
    let capacity =
        load_submission_evidence_capacity_postgres(tx, account_id, application_id, run_id)?
            .ok_or(UploadControlError::SubmissionEvidenceCapacityUnavailable)?;
    validate_capacity_commit(
        &capacity,
        runner,
        committed_bytes,
        committed_objects,
        now_ms,
    )?;
    if tx.execute(
        "UPDATE jobs_submission_evidence_capacity
            SET consumed_bytes = $5, consumed_objects = $6,
                state = 'committed', updated_at_ms = $7, completed_at_ms = $7
          WHERE account_id = $1 AND application_id = $2 AND run_id = $3
            AND runner = $4 AND state = 'active' AND expires_at_ms > $7
            AND consumed_bytes >= $5 AND consumed_objects >= $6",
        &[
            &account_id,
            &application_id,
            &run_id,
            &runner,
            &committed_bytes,
            &committed_objects,
            &now_ms,
        ],
    )? != 1
    {
        return Err(UploadControlError::SubmissionEvidenceCapacityUnavailable.into());
    }
    Ok(())
}

fn validate_capacity_commit(
    capacity: &SubmissionEvidenceCapacity,
    runner: &str,
    committed_bytes: i64,
    committed_objects: i64,
    now_ms: i64,
) -> Result<()> {
    if capacity.state != "active" || capacity.expires_at_ms <= now_ms || capacity.runner != runner {
        return Err(UploadControlError::SubmissionEvidenceCapacityUnavailable.into());
    }
    if committed_bytes <= 0
        || committed_objects <= 0
        || committed_bytes > capacity.consumed_bytes
        || committed_objects > capacity.consumed_objects
    {
        return Err(UploadControlError::SubmissionEvidenceCapacityExceeded.into());
    }
    Ok(())
}

fn upload_matches_submission_run(
    upload: &ObjectUpload,
    account_id: &str,
    application_id: &str,
    run_id: &str,
    runner: &str,
) -> bool {
    if upload.account_id != account_id
        || upload.object_kind != ObjectKind::Artifact.as_str()
        || upload.session_id.is_some()
        || upload.state != "pending"
    {
        return false;
    }
    serde_json::from_str::<serde_json::Value>(&upload.metadata_json)
        .ok()
        .is_some_and(|metadata| {
            metadata
                .get("artifact_class")
                .and_then(serde_json::Value::as_str)
                == Some("jobs_submission_evidence")
                && metadata
                    .get("jobs_application_id")
                    .and_then(serde_json::Value::as_str)
                    == Some(application_id)
                && metadata
                    .get("jobs_run_id")
                    .and_then(serde_json::Value::as_str)
                    == Some(run_id)
                && metadata
                    .get("jobs_runner")
                    .and_then(serde_json::Value::as_str)
                    == Some(runner)
        })
}

fn cleanup_speculative_submission_uploads_sqlite_tx(
    tx: &SqliteTransaction<'_>,
    account_id: &str,
    application_id: &str,
    run_id: &str,
    runner: &str,
    winning_upload_ids: &std::collections::HashSet<&str>,
    now_ms: i64,
) -> Result<()> {
    let mut stmt = tx.prepare(&format!(
        "SELECT {UPLOAD_COLUMNS} FROM object_uploads
          WHERE account_id = ?1 AND object_kind = 'artifact'
            AND session_id IS NULL AND state = 'pending'"
    ))?;
    let uploads = stmt
        .query_map(params![account_id], row_to_upload_sqlite)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(stmt);
    let delete_not_before_ms = now_ms.saturating_add(PROCESSING_LEASE_MS);
    for upload in uploads.into_iter().filter(|upload| {
        !winning_upload_ids.contains(upload.id.as_str())
            && upload_matches_submission_run(upload, account_id, application_id, run_id, runner)
    }) {
        schedule_upload_cleanup_sqlite_tx(tx, &upload.id, now_ms)?;
        tx.execute(
            "UPDATE object_storage_outbox
                SET next_attempt_at_ms = MAX(next_attempt_at_ms, ?2)
              WHERE upload_id = ?1 AND operation = 'delete'",
            params![upload.id, delete_not_before_ms],
        )?;
    }
    Ok(())
}

fn cleanup_speculative_submission_uploads_postgres_tx(
    tx: &mut PgTransaction<'_>,
    account_id: &str,
    application_id: &str,
    run_id: &str,
    runner: &str,
    winning_upload_ids: &std::collections::HashSet<&str>,
    now_ms: i64,
) -> Result<()> {
    let uploads = tx
        .query(
            &format!(
                "SELECT {UPLOAD_COLUMNS} FROM object_uploads
                  WHERE account_id = $1 AND object_kind = 'artifact'
                    AND session_id IS NULL AND state = 'pending'
                  FOR UPDATE"
            ),
            &[&account_id],
        )?
        .into_iter()
        .map(row_to_upload_postgres)
        .collect::<Result<Vec<_>>>()?;
    let delete_not_before_ms = now_ms.saturating_add(PROCESSING_LEASE_MS);
    for upload in uploads.into_iter().filter(|upload| {
        !winning_upload_ids.contains(upload.id.as_str())
            && upload_matches_submission_run(upload, account_id, application_id, run_id, runner)
    }) {
        schedule_upload_cleanup_postgres_tx(tx, &upload.id, now_ms)?;
        tx.execute(
            "UPDATE object_storage_outbox
                SET next_attempt_at_ms = GREATEST(next_attempt_at_ms, $2)
              WHERE upload_id = $1 AND operation = 'delete'",
            &[&upload.id, &delete_not_before_ms],
        )?;
    }
    Ok(())
}

pub(crate) fn commit_application_object_uploads_sqlite_tx(
    tx: &SqliteTransaction<'_>,
    account_id: &str,
    application_id: &str,
    run_id: &str,
    runner: &str,
    bindings: &[ApplicationObjectBinding],
    now_ms: i64,
) -> Result<()> {
    require_active_account_write_fence_sqlite_tx(tx, account_id)?;
    let application_state = tx
        .query_row(
            "SELECT state FROM jobs_applications WHERE account_id = ?1 AND id = ?2",
            params![account_id, application_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    if application_state
        .as_deref()
        .is_none_or(|state| state == "submitted")
    {
        return Err(UploadControlError::SubmissionEvidenceCapacityUnavailable.into());
    }
    let mut seen = std::collections::HashSet::new();
    let mut committed_bytes = 0_i64;
    for binding in bindings {
        if !seen.insert(binding.upload_id.as_str()) {
            return Err(UploadControlError::IdempotencyConflict.into());
        }
        let upload = load_upload_sqlite(tx, &binding.upload_id)?
            .ok_or(UploadControlError::UploadNotFound)?;
        validate_application_object_binding(
            &upload,
            account_id,
            application_id,
            run_id,
            runner,
            binding,
        )?;
        committed_bytes = committed_bytes
            .checked_add(binding.size_bytes)
            .ok_or(UploadControlError::SubmissionEvidenceCapacityExceeded)?;
        if tx.execute(
            "UPDATE object_uploads
                SET state = 'ready', uploaded_at_ms = ?2, updated_at_ms = ?2
              WHERE id = ?1 AND state = 'pending'",
            params![binding.upload_id, now_ms],
        )? != 1
        {
            return Err(UploadControlError::UploadInProgress.into());
        }
        if tx.execute(
            "UPDATE object_storage_outbox
                SET state = 'completed', updated_at_ms = ?2, completed_at_ms = ?2,
                    next_attempt_at_ms = ?2, last_error = NULL
              WHERE upload_id = ?1 AND operation = 'put'
                AND state IN ('pending', 'processing', 'retry')",
            params![binding.upload_id, now_ms],
        )? != 1
        {
            return Err(UploadControlError::UploadNotFound.into());
        }
    }
    cleanup_speculative_submission_uploads_sqlite_tx(
        tx,
        account_id,
        application_id,
        run_id,
        runner,
        &seen,
        now_ms,
    )?;
    commit_submission_evidence_capacity_sqlite_tx(
        tx,
        account_id,
        application_id,
        run_id,
        runner,
        committed_bytes,
        i64::try_from(bindings.len())
            .map_err(|_| UploadControlError::SubmissionEvidenceCapacityExceeded)?,
        now_ms,
    )?;
    Ok(())
}

pub(crate) fn commit_application_object_uploads_postgres_tx(
    tx: &mut PgTransaction<'_>,
    account_id: &str,
    application_id: &str,
    run_id: &str,
    runner: &str,
    bindings: &[ApplicationObjectBinding],
    now_ms: i64,
) -> Result<()> {
    // `finalize_submission` must have acquired this fence before locking its
    // Jobs application row. Rechecking here proves that the fence remains
    // active at the exact upload publication boundary.
    require_active_account_write_fence_postgres_tx(tx, account_id)?;
    // Every application-object reservation acquires this same row FOR SHARE
    // before consuming capacity. Owning it FOR UPDATE closes the reservation
    // set before we choose the exact receipt bindings that become durable.
    let application_state = tx
        .query_opt(
            "SELECT state FROM jobs_applications
              WHERE account_id = $1 AND id = $2 FOR UPDATE",
            &[&account_id, &application_id],
        )?
        .map(|row| row.get::<_, String>(0));
    if application_state
        .as_deref()
        .is_none_or(|state| state == "submitted")
    {
        return Err(UploadControlError::SubmissionEvidenceCapacityUnavailable.into());
    }
    let mut seen = std::collections::HashSet::new();
    let mut committed_bytes = 0_i64;
    for binding in bindings {
        if !seen.insert(binding.upload_id.as_str()) {
            return Err(UploadControlError::IdempotencyConflict.into());
        }
        let upload = load_upload_postgres(tx, &binding.upload_id)?
            .ok_or(UploadControlError::UploadNotFound)?;
        validate_application_object_binding(
            &upload,
            account_id,
            application_id,
            run_id,
            runner,
            binding,
        )?;
        committed_bytes = committed_bytes
            .checked_add(binding.size_bytes)
            .ok_or(UploadControlError::SubmissionEvidenceCapacityExceeded)?;
        if tx.execute(
            "UPDATE object_uploads
                SET state = 'ready', uploaded_at_ms = $2, updated_at_ms = $2
              WHERE id = $1 AND state = 'pending'",
            &[&binding.upload_id, &now_ms],
        )? != 1
        {
            return Err(UploadControlError::UploadInProgress.into());
        }
        if tx.execute(
            "UPDATE object_storage_outbox
                SET state = 'completed', updated_at_ms = $2, completed_at_ms = $2,
                    next_attempt_at_ms = $2, last_error = NULL
              WHERE upload_id = $1 AND operation = 'put'
                AND state IN ('pending', 'processing', 'retry')",
            &[&binding.upload_id, &now_ms],
        )? != 1
        {
            return Err(UploadControlError::UploadNotFound.into());
        }
    }
    cleanup_speculative_submission_uploads_postgres_tx(
        tx,
        account_id,
        application_id,
        run_id,
        runner,
        &seen,
        now_ms,
    )?;
    commit_submission_evidence_capacity_postgres_tx(
        tx,
        account_id,
        application_id,
        run_id,
        runner,
        committed_bytes,
        i64::try_from(bindings.len())
            .map_err(|_| UploadControlError::SubmissionEvidenceCapacityExceeded)?,
        now_ms,
    )?;
    Ok(())
}

fn validate_application_object_binding(
    upload: &ObjectUpload,
    account_id: &str,
    application_id: &str,
    run_id: &str,
    runner: &str,
    binding: &ApplicationObjectBinding,
) -> Result<()> {
    let metadata = serde_json::from_str::<serde_json::Value>(&upload.metadata_json)
        .context("parse application object upload metadata")?;
    if upload.account_id != account_id
        || upload.object_kind != ObjectKind::Artifact.as_str()
        || upload.session_id.is_some()
        || upload.state != "pending"
        || metadata
            .get("artifact_class")
            .and_then(serde_json::Value::as_str)
            != Some("jobs_submission_evidence")
        || metadata
            .get("jobs_application_id")
            .and_then(serde_json::Value::as_str)
            != Some(application_id)
        || metadata
            .get("jobs_run_id")
            .and_then(serde_json::Value::as_str)
            != Some(run_id)
        || metadata
            .get("jobs_runner")
            .and_then(serde_json::Value::as_str)
            != Some(runner)
        || upload.id != binding.upload_id
        || upload.object_key != binding.object_key
        || upload.size_bytes != binding.size_bytes
        || upload.sha256 != binding.sha256
        || upload.content_type != binding.content_type
    {
        return Err(UploadControlError::IdempotencyConflict.into());
    }
    Ok(())
}

fn validate_input(input: &NewObjectUpload) -> Result<()> {
    validate_object_input(input)?;
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

fn validate_object_input(input: &NewObjectUpload) -> Result<()> {
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

fn validate_account_object_input(input: &NewObjectUpload) -> Result<()> {
    validate_object_input(input)?;
    if input.object_kind != ObjectKind::Artifact
        || input.storage_scope != StorageScope::Artifact
        || input.session_id.is_some()
    {
        return Err(UploadControlError::InvalidMetadata("account object kind").into());
    }
    if !is_supported_account_object_class(
        input
            .metadata_json
            .get("artifact_class")
            .and_then(serde_json::Value::as_str),
    ) {
        return Err(UploadControlError::InvalidMetadata("account object class").into());
    }
    Ok(())
}

fn validate_account_object_cleanup_identity(account_id: &str, object_key: &str) -> Result<()> {
    if account_id.trim().is_empty() {
        return Err(UploadControlError::InvalidMetadata("account id").into());
    }
    if object_key.trim().is_empty() || object_key.len() > 2048 {
        return Err(UploadControlError::InvalidMetadata("object key").into());
    }
    Ok(())
}

fn validate_stored_account_object(upload: &ObjectUpload) -> Result<()> {
    if !is_stored_account_object(upload) {
        return Err(UploadControlError::InvalidMetadata("account object class").into());
    }
    Ok(())
}

fn is_stored_account_object(upload: &ObjectUpload) -> bool {
    upload.object_kind == ObjectKind::Artifact.as_str()
        && upload.storage_scope == StorageScope::Artifact.as_str()
        && upload.session_id.is_none()
        && serde_json::from_str::<serde_json::Value>(&upload.metadata_json)
            .ok()
            .is_some_and(|metadata| {
                is_supported_account_object_class(
                    metadata
                        .get("artifact_class")
                        .and_then(serde_json::Value::as_str),
                )
            })
}

fn is_supported_account_object_class(artifact_class: Option<&str>) -> bool {
    matches!(
        artifact_class,
        Some("jobs_resume_source" | "jobs_browser_profile_snapshot")
    )
}

fn validate_application_object_input(application_id: &str, input: &NewObjectUpload) -> Result<()> {
    validate_object_input(input)?;
    if input.object_kind != ObjectKind::Artifact
        || input.storage_scope != StorageScope::Artifact
        || input.session_id.is_some()
    {
        return Err(UploadControlError::InvalidMetadata("application object kind").into());
    }
    let application_id = application_id.trim();
    if application_id.is_empty() || application_id.len() > 128 {
        return Err(UploadControlError::InvalidMetadata("parent application").into());
    }
    if input
        .metadata_json
        .get("jobs_application_id")
        .and_then(serde_json::Value::as_str)
        != Some(application_id)
        || input
            .metadata_json
            .get("artifact_class")
            .and_then(serde_json::Value::as_str)
            != Some("jobs_submission_evidence")
        || submission_capacity_binding(input).is_err()
    {
        return Err(UploadControlError::InvalidMetadata("parent application").into());
    }
    Ok(())
}

fn submission_capacity_binding(input: &NewObjectUpload) -> Result<(&str, &str)> {
    let run_id = input
        .metadata_json
        .get("jobs_run_id")
        .and_then(serde_json::Value::as_str)
        .filter(|value| {
            let value = value.trim();
            !value.is_empty() && value.len() <= 128
        })
        .ok_or(UploadControlError::InvalidMetadata("submission run"))?;
    let runner = input
        .metadata_json
        .get("jobs_runner")
        .and_then(serde_json::Value::as_str)
        .filter(|value| matches!(*value, "cloud" | "local"))
        .ok_or(UploadControlError::InvalidMetadata("submission runner"))?;
    Ok((run_id, runner))
}

fn reserve_upload_sqlite(pool: &DbPool, input: &NewObjectUpload) -> Result<UploadReservation> {
    let mut conn = pool.get().context("get sqlite object upload conn")?;
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .context("begin sqlite object upload reservation")?;

    require_active_account_write_fence_sqlite_tx(&tx, &input.account_id)?;
    validate_session_sqlite(&tx, input)?;
    reject_tombstoned_artifact_sqlite(&tx, input)?;
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
    if input.object_kind == ObjectKind::Artifact {
        // Sync tombstones take this same identity lock before publishing the
        // tombstone. Take it before the account row lock so the reservation
        // cannot observe "not deleted" and insert after a concurrent delete.
        lock_context_artifact_postgres_tx(&mut tx, &input.logical_id)?;
    }
    lock_session_postgres_tx(
        &mut tx,
        input
            .session_id
            .as_deref()
            .ok_or(UploadControlError::InvalidMetadata("parent session"))?,
    )?;
    require_active_account_write_fence_postgres_tx(&mut tx, &input.account_id)?;

    validate_session_postgres(&mut tx, input)?;
    reject_tombstoned_artifact_postgres(&mut tx, input)?;
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

fn reserve_account_object_upload_sqlite(
    pool: &DbPool,
    input: &NewObjectUpload,
) -> Result<UploadReservation> {
    let mut conn = pool
        .get()
        .context("get sqlite account object upload conn")?;
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .context("begin sqlite account object upload reservation")?;
    require_active_account_write_fence_sqlite_tx(&tx, &input.account_id)?;
    if let Some(existing) = load_logical_upload_sqlite(
        &tx,
        &input.account_id,
        input.object_kind.as_str(),
        &input.logical_id,
    )? {
        let reservation = retry_account_object_reservation(&existing, input)?;
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
         ) VALUES (?1, ?2, ?3, ?4, NULL, ?5, ?6, ?7, ?8, ?9, ?10, 'pending', ?11, ?12, ?12)",
        params![
            upload_id,
            input.account_id,
            input.object_kind.as_str(),
            input.logical_id,
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

fn reserve_account_object_upload_postgres(
    pool: &DbPool,
    input: &NewObjectUpload,
) -> Result<UploadReservation> {
    let mut conn = pool
        .get_pg()
        .context("get postgres account object upload conn")?;
    let mut tx = conn
        .transaction()
        .context("begin postgres account object upload reservation")?;
    lock_context_artifact_postgres_tx(&mut tx, &input.logical_id)?;
    require_active_account_write_fence_postgres_tx(&mut tx, &input.account_id)?;
    if let Some(existing) = load_logical_upload_postgres(
        &mut tx,
        &input.account_id,
        input.object_kind.as_str(),
        &input.logical_id,
    )? {
        let reservation = retry_account_object_reservation(&existing, input)?;
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
         ) VALUES ($1, $2, $3, $4, NULL, $5, $6, $7, $8, $9, $10, 'pending', $11, $12, $12)",
        &[
            &upload_id,
            &input.account_id,
            &input.object_kind.as_str(),
            &input.logical_id,
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

fn reserve_application_object_upload_sqlite(
    pool: &DbPool,
    application_id: &str,
    input: &NewObjectUpload,
) -> Result<UploadReservation> {
    let (run_id, runner) = submission_capacity_binding(input)?;
    let mut conn = pool
        .get()
        .context("get sqlite application object upload conn")?;
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .context("begin sqlite application object upload reservation")?;
    require_active_account_write_fence_sqlite_tx(&tx, &input.account_id)?;
    let application_state = tx
        .query_row(
            "SELECT state FROM jobs_applications WHERE account_id = ?1 AND id = ?2",
            params![input.account_id, application_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    match application_state.as_deref() {
        None => return Err(UploadControlError::SessionNotOwned.into()),
        Some("submitted") => {
            return Err(UploadControlError::SubmissionEvidenceCapacityUnavailable.into())
        }
        Some(_) => {}
    }
    if let Some(existing) = load_logical_upload_sqlite(
        &tx,
        &input.account_id,
        input.object_kind.as_str(),
        &input.logical_id,
    )? {
        consume_submission_evidence_capacity_sqlite_tx(
            &tx,
            &input.account_id,
            application_id,
            run_id,
            runner,
            0,
            0,
            input.now_ms,
        )?;
        let reservation = retry_reservation(&existing, input)?;
        if reservation.needs_put {
            reopen_put_outbox_sqlite(&tx, &existing.id, input.now_ms)?;
        }
        tx.commit()?;
        return Ok(reservation);
    }

    consume_submission_evidence_capacity_sqlite_tx(
        &tx,
        &input.account_id,
        application_id,
        run_id,
        runner,
        input.size_bytes,
        1,
        input.now_ms,
    )?;
    let upload_id = stable_upload_id(input);
    let metadata_json = serde_json::to_string(&input.metadata_json)?;
    tx.execute(
        "INSERT INTO object_uploads (
            id, account_id, object_kind, logical_id, session_id, storage_scope,
            object_key, size_bytes, sha256, content_type, expires_at_ms, state,
            metadata_json, created_at_ms, updated_at_ms
         ) VALUES (?1, ?2, ?3, ?4, NULL, ?5, ?6, ?7, ?8, ?9, ?10, 'pending', ?11, ?12, ?12)",
        params![
            upload_id,
            input.account_id,
            input.object_kind.as_str(),
            input.logical_id,
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

fn reserve_application_object_upload_postgres(
    pool: &DbPool,
    application_id: &str,
    input: &NewObjectUpload,
) -> Result<UploadReservation> {
    let (run_id, runner) = submission_capacity_binding(input)?;
    let mut conn = pool
        .get_pg()
        .context("get postgres application object upload conn")?;
    let mut tx = conn
        .transaction()
        .context("begin postgres application object upload reservation")?;
    lock_context_artifact_postgres_tx(&mut tx, &input.logical_id)?;
    require_active_account_write_fence_postgres_tx(&mut tx, &input.account_id)?;
    let application_state = tx
        .query_opt(
            "SELECT state FROM jobs_applications
              WHERE account_id = $1 AND id = $2 FOR SHARE",
            &[&input.account_id, &application_id],
        )?
        .map(|row| row.get::<_, String>(0));
    match application_state.as_deref() {
        None => return Err(UploadControlError::SessionNotOwned.into()),
        Some("submitted") => {
            return Err(UploadControlError::SubmissionEvidenceCapacityUnavailable.into())
        }
        Some(_) => {}
    }
    if let Some(existing) = load_logical_upload_postgres(
        &mut tx,
        &input.account_id,
        input.object_kind.as_str(),
        &input.logical_id,
    )? {
        consume_submission_evidence_capacity_postgres_tx(
            &mut tx,
            &input.account_id,
            application_id,
            run_id,
            runner,
            0,
            0,
            input.now_ms,
        )?;
        let reservation = retry_reservation(&existing, input)?;
        if reservation.needs_put {
            reopen_put_outbox_postgres(&mut tx, &existing.id, input.now_ms)?;
        }
        tx.commit()?;
        return Ok(reservation);
    }

    consume_submission_evidence_capacity_postgres_tx(
        &mut tx,
        &input.account_id,
        application_id,
        run_id,
        runner,
        input.size_bytes,
        1,
        input.now_ms,
    )?;
    let upload_id = stable_upload_id(input);
    let metadata_json = serde_json::to_string(&input.metadata_json)?;
    tx.execute(
        "INSERT INTO object_uploads (
            id, account_id, object_kind, logical_id, session_id, storage_scope,
            object_key, size_bytes, sha256, content_type, expires_at_ms, state,
            metadata_json, created_at_ms, updated_at_ms
         ) VALUES ($1, $2, $3, $4, NULL, $5, $6, $7, $8, $9, $10, 'pending', $11, $12, $12)",
        &[
            &upload_id,
            &input.account_id,
            &input.object_kind.as_str(),
            &input.logical_id,
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

fn retry_account_object_reservation(
    existing: &ObjectUpload,
    input: &NewObjectUpload,
) -> Result<UploadReservation> {
    if matches!(existing.state.as_str(), "delete_pending" | "deleted") {
        return Err(UploadControlError::UploadGone.into());
    }
    if !account_object_reservation_exact_match(existing, input)? {
        return Err(UploadControlError::IdempotencyConflict.into());
    }
    Ok(UploadReservation {
        upload: existing.clone(),
        needs_put: existing.state != "ready",
    })
}

fn account_object_reservation_exact_match(
    existing: &ObjectUpload,
    input: &NewObjectUpload,
) -> Result<bool> {
    let existing_metadata = serde_json::from_str::<serde_json::Value>(&existing.metadata_json)
        .map_err(|_| UploadControlError::IdempotencyConflict)?;
    Ok(existing.account_id == input.account_id
        && existing.object_kind == input.object_kind.as_str()
        && existing.logical_id == input.logical_id
        && existing.session_id.is_none()
        && existing.storage_scope == input.storage_scope.as_str()
        && existing.object_key == input.object_key
        && existing.size_bytes == input.size_bytes
        && existing.sha256.eq_ignore_ascii_case(&input.sha256)
        && existing.content_type == input.content_type
        && existing.expires_at_ms == input.expires_at_ms
        && existing_metadata == input.metadata_json)
}

fn enforce_quota_sqlite(tx: &SqliteTransaction<'_>, input: &NewObjectUpload) -> Result<()> {
    let (bytes, objects) = effective_account_usage_sqlite(tx, &input.account_id, input.now_ms)?;
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
    let (bytes, objects) = effective_account_usage_postgres(tx, &input.account_id, input.now_ms)?;
    enforce_total_limits(bytes, objects, input)?;

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

fn effective_account_usage_sqlite(
    tx: &SqliteTransaction<'_>,
    account_id: &str,
    now_ms: i64,
) -> Result<(i64, i64)> {
    Ok(tx.query_row(
        "SELECT
            COALESCE((
                SELECT SUM(size_bytes) FROM object_uploads
                 WHERE account_id = ?1
                   AND state IN ('pending', 'ready', 'delete_pending')
            ), 0) + COALESCE((
                SELECT SUM(reserved_bytes - consumed_bytes)
                  FROM jobs_submission_evidence_capacity
                 WHERE account_id = ?1 AND state = 'active' AND expires_at_ms > ?2
            ), 0),
            COALESCE((
                SELECT COUNT(*) FROM object_uploads
                 WHERE account_id = ?1
                   AND state IN ('pending', 'ready', 'delete_pending')
            ), 0) + COALESCE((
                SELECT SUM(reserved_objects - consumed_objects)
                  FROM jobs_submission_evidence_capacity
                 WHERE account_id = ?1 AND state = 'active' AND expires_at_ms > ?2
            ), 0)",
        params![account_id, now_ms],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?)
}

fn effective_account_usage_postgres(
    tx: &mut PgTransaction<'_>,
    account_id: &str,
    now_ms: i64,
) -> Result<(i64, i64)> {
    let row = tx.query_one(
        "SELECT
            COALESCE((
                SELECT SUM(size_bytes) FROM object_uploads
                 WHERE account_id = $1
                   AND state IN ('pending', 'ready', 'delete_pending')
            ), 0)::bigint + COALESCE((
                SELECT SUM(reserved_bytes - consumed_bytes)
                  FROM jobs_submission_evidence_capacity
                 WHERE account_id = $1 AND state = 'active' AND expires_at_ms > $2
            ), 0)::bigint,
            COALESCE((
                SELECT COUNT(*) FROM object_uploads
                 WHERE account_id = $1
                   AND state IN ('pending', 'ready', 'delete_pending')
            ), 0)::bigint + COALESCE((
                SELECT SUM(reserved_objects - consumed_objects)
                  FROM jobs_submission_evidence_capacity
                 WHERE account_id = $1 AND state = 'active' AND expires_at_ms > $2
            ), 0)::bigint",
        &[&account_id, &now_ms],
    )?;
    Ok((row.try_get(0)?, row.try_get(1)?))
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

fn insert_completed_put_outbox_sqlite(
    tx: &SqliteTransaction<'_>,
    upload_id: &str,
    account_id: &str,
    now_ms: i64,
) -> Result<()> {
    tx.execute(
        "INSERT INTO object_storage_outbox (
            id, upload_id, account_id, operation, state, attempt_count,
            next_attempt_at_ms, last_error, created_at_ms, updated_at_ms, completed_at_ms
         ) VALUES (?1, ?2, ?3, 'put', 'completed', 1, ?4, NULL, ?4, ?4, ?4)",
        params![outbox_id(upload_id, "put"), upload_id, account_id, now_ms],
    )?;
    Ok(())
}

fn insert_completed_put_outbox_postgres(
    tx: &mut PgTransaction<'_>,
    upload_id: &str,
    account_id: &str,
    now_ms: i64,
) -> Result<()> {
    tx.execute(
        "INSERT INTO object_storage_outbox (
            id, upload_id, account_id, operation, state, attempt_count,
            next_attempt_at_ms, last_error, created_at_ms, updated_at_ms, completed_at_ms
         ) VALUES ($1, $2, $3, 'put', 'completed', 1, $4, NULL, $4, $4, $4)",
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

fn begin_upload_put_sqlite(pool: &DbPool, upload_id: &str, now_ms: i64) -> Result<ObjectUpload> {
    let mut conn = pool.get().context("get sqlite PUT-begin connection")?;
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .context("begin sqlite object PUT lease")?;
    let mut upload =
        load_upload_sqlite(&tx, upload_id)?.ok_or(UploadControlError::UploadNotFound)?;
    if matches!(upload.state.as_str(), "delete_pending" | "deleted") {
        return Err(UploadControlError::UploadGone.into());
    }
    match account_data::account_write_fence_sqlite_tx(&tx, &upload.account_id)? {
        AccountWriteFence::Active => {}
        AccountWriteFence::DeletionRequested => {
            schedule_upload_cleanup_sqlite_tx(&tx, upload_id, now_ms)?;
            tx.commit()?;
            return Err(UploadControlError::AccountDeleting.into());
        }
        AccountWriteFence::Missing => return Err(UploadControlError::SessionNotOwned.into()),
    }
    if upload.state != "pending" {
        return Err(UploadControlError::UploadInProgress.into());
    }
    let outbox_state = tx
        .query_row(
            "SELECT state FROM object_storage_outbox
              WHERE upload_id = ?1 AND operation = 'put'",
            params![upload_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        .ok_or(UploadControlError::UploadNotFound)?;
    if outbox_state == "abandoned" {
        return Err(UploadControlError::UploadGone.into());
    }
    if outbox_state == "completed" {
        return Err(UploadControlError::UploadInProgress.into());
    }
    if tx.execute(
        "UPDATE object_uploads
            SET updated_at_ms = MAX(updated_at_ms, ?2)
          WHERE id = ?1 AND state = 'pending'",
        params![upload_id, now_ms],
    )? != 1
    {
        return Err(UploadControlError::UploadInProgress.into());
    }
    if tx.execute(
        "UPDATE object_storage_outbox
            SET state = 'processing',
                attempt_count = CASE
                    WHEN state IN ('pending', 'retry') THEN attempt_count + 1
                    ELSE attempt_count
                END,
                next_attempt_at_ms = ?2, updated_at_ms = ?2,
                last_error = NULL, completed_at_ms = NULL
          WHERE upload_id = ?1 AND operation = 'put'
            AND state IN ('pending', 'processing', 'retry')",
        params![upload_id, now_ms],
    )? != 1
    {
        return Err(UploadControlError::UploadNotFound.into());
    }
    upload.updated_at_ms = upload.updated_at_ms.max(now_ms);
    tx.commit()?;
    Ok(upload)
}

fn begin_upload_put_postgres(pool: &DbPool, upload_id: &str, now_ms: i64) -> Result<ObjectUpload> {
    let mut conn = pool.get_pg().context("get postgres PUT-begin connection")?;
    let mut tx = conn
        .transaction()
        .context("begin postgres object PUT lease")?;
    let identity = load_upload_postgres_unlocked(&mut tx, upload_id)?
        .ok_or(UploadControlError::UploadNotFound)?;
    if identity.object_kind == ObjectKind::Artifact.as_str() {
        lock_context_artifact_postgres_tx(&mut tx, &identity.logical_id)?;
    }
    if let Some(session_id) = identity.session_id.as_deref() {
        lock_session_postgres_tx(&mut tx, session_id)?;
    }
    let account_fence =
        account_data::account_write_fence_postgres_tx(&mut tx, &identity.account_id)?;
    let mut upload =
        load_upload_postgres(&mut tx, upload_id)?.ok_or(UploadControlError::UploadNotFound)?;
    if matches!(upload.state.as_str(), "delete_pending" | "deleted") {
        return Err(UploadControlError::UploadGone.into());
    }
    match account_fence {
        AccountWriteFence::Active => {}
        AccountWriteFence::DeletionRequested => {
            let _ = schedule_upload_cleanup_postgres_tx(&mut tx, upload_id, now_ms)?;
            tx.commit()?;
            return Err(UploadControlError::AccountDeleting.into());
        }
        AccountWriteFence::Missing => return Err(UploadControlError::SessionNotOwned.into()),
    }
    if upload.state != "pending" {
        return Err(UploadControlError::UploadInProgress.into());
    }
    let outbox_state = tx
        .query_opt(
            "SELECT state FROM object_storage_outbox
              WHERE upload_id = $1 AND operation = 'put' FOR UPDATE",
            &[&upload_id],
        )?
        .map(|row| row.get::<_, String>(0))
        .ok_or(UploadControlError::UploadNotFound)?;
    if outbox_state == "abandoned" {
        return Err(UploadControlError::UploadGone.into());
    }
    if outbox_state == "completed" {
        return Err(UploadControlError::UploadInProgress.into());
    }
    if tx.execute(
        "UPDATE object_uploads
            SET updated_at_ms = GREATEST(updated_at_ms, $2)
          WHERE id = $1 AND state = 'pending'",
        &[&upload_id, &now_ms],
    )? != 1
    {
        return Err(UploadControlError::UploadInProgress.into());
    }
    if tx.execute(
        "UPDATE object_storage_outbox
            SET state = 'processing',
                attempt_count = CASE
                    WHEN state IN ('pending', 'retry') THEN attempt_count + 1
                    ELSE attempt_count
                END,
                next_attempt_at_ms = $2, updated_at_ms = $2,
                last_error = NULL, completed_at_ms = NULL
          WHERE upload_id = $1 AND operation = 'put'
            AND state IN ('pending', 'processing', 'retry')",
        &[&upload_id, &now_ms],
    )? != 1
    {
        return Err(UploadControlError::UploadNotFound.into());
    }
    upload.updated_at_ms = upload.updated_at_ms.max(now_ms);
    tx.commit()?;
    Ok(upload)
}

fn release_verified_upload_put_sqlite(pool: &DbPool, upload_id: &str, now_ms: i64) -> Result<()> {
    let mut conn = pool.get().context("get sqlite verified PUT connection")?;
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .context("begin sqlite verified PUT release")?;
    let upload = load_upload_sqlite(&tx, upload_id)?.ok_or(UploadControlError::UploadNotFound)?;
    if upload.state == "ready" {
        tx.commit()?;
        return Ok(());
    }
    if matches!(upload.state.as_str(), "delete_pending" | "deleted") {
        return Err(UploadControlError::UploadGone.into());
    }
    match account_data::account_write_fence_sqlite_tx(&tx, &upload.account_id)? {
        AccountWriteFence::Active => {}
        AccountWriteFence::DeletionRequested => {
            schedule_upload_cleanup_sqlite_tx(&tx, upload_id, now_ms)?;
            tx.commit()?;
            return Err(UploadControlError::AccountDeleting.into());
        }
        AccountWriteFence::Missing => return Err(UploadControlError::SessionNotOwned.into()),
    }
    if upload.state != "pending" {
        return Err(UploadControlError::UploadInProgress.into());
    }
    if tx.execute(
        "UPDATE object_storage_outbox
            SET state = 'retry', next_attempt_at_ms = ?2, updated_at_ms = ?2,
                last_error = NULL, completed_at_ms = NULL
          WHERE upload_id = ?1 AND operation = 'put'
            AND state IN ('pending', 'processing', 'retry')",
        params![upload_id, now_ms],
    )? != 1
    {
        return Err(UploadControlError::UploadNotFound.into());
    }
    tx.execute(
        "UPDATE object_uploads SET updated_at_ms = MAX(updated_at_ms, ?2)
          WHERE id = ?1 AND state = 'pending'",
        params![upload_id, now_ms],
    )?;
    tx.commit()?;
    Ok(())
}

fn release_verified_upload_put_postgres(pool: &DbPool, upload_id: &str, now_ms: i64) -> Result<()> {
    let mut conn = pool
        .get_pg()
        .context("get Postgres verified PUT connection")?;
    let mut tx = conn
        .transaction()
        .context("begin Postgres verified PUT release")?;
    let identity = load_upload_postgres_unlocked(&mut tx, upload_id)?
        .ok_or(UploadControlError::UploadNotFound)?;
    if identity.object_kind == ObjectKind::Artifact.as_str() {
        lock_context_artifact_postgres_tx(&mut tx, &identity.logical_id)?;
    }
    let account_fence =
        account_data::account_write_fence_postgres_tx(&mut tx, &identity.account_id)?;
    let upload =
        load_upload_postgres(&mut tx, upload_id)?.ok_or(UploadControlError::UploadNotFound)?;
    if upload.state == "ready" {
        tx.commit()?;
        return Ok(());
    }
    if matches!(upload.state.as_str(), "delete_pending" | "deleted") {
        return Err(UploadControlError::UploadGone.into());
    }
    match account_fence {
        AccountWriteFence::Active => {}
        AccountWriteFence::DeletionRequested => {
            let _ = schedule_upload_cleanup_postgres_tx(&mut tx, upload_id, now_ms)?;
            tx.commit()?;
            return Err(UploadControlError::AccountDeleting.into());
        }
        AccountWriteFence::Missing => return Err(UploadControlError::SessionNotOwned.into()),
    }
    if upload.state != "pending" {
        return Err(UploadControlError::UploadInProgress.into());
    }
    if tx.execute(
        "UPDATE object_storage_outbox
            SET state = 'retry', next_attempt_at_ms = $2, updated_at_ms = $2,
                last_error = NULL, completed_at_ms = NULL
          WHERE upload_id = $1 AND operation = 'put'
            AND state IN ('pending', 'processing', 'retry')",
        &[&upload_id, &now_ms],
    )? != 1
    {
        return Err(UploadControlError::UploadNotFound.into());
    }
    tx.execute(
        "UPDATE object_uploads SET updated_at_ms = GREATEST(updated_at_ms, $2)
          WHERE id = $1 AND state = 'pending'",
        &[&upload_id, &now_ms],
    )?;
    tx.commit()?;
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
    match account_data::account_write_fence_sqlite_tx(&tx, &upload.account_id)? {
        AccountWriteFence::Active => {}
        AccountWriteFence::DeletionRequested => {
            schedule_upload_cleanup_sqlite_tx(&tx, upload_id, now_ms)?;
            tx.commit()?;
            return Err(UploadControlError::AccountDeleting.into());
        }
        AccountWriteFence::Missing => return Err(UploadControlError::SessionNotOwned.into()),
    }
    let account_object = is_stored_account_object(&upload);
    if !account_object && !upload_parent_is_live_sqlite(&tx, &upload)? {
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
    if !account_object
        && upload.object_kind == ObjectKind::Artifact.as_str()
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
    if !account_object {
        publish_index_sqlite(&tx, &upload)?;
    }
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
    let account_object = is_stored_account_object(&identity);
    if !account_object {
        lock_session_postgres_tx(
            &mut tx,
            identity
                .session_id
                .as_deref()
                .ok_or(UploadControlError::InvalidMetadata("parent session"))?,
        )?;
    }
    let account_fence =
        account_data::account_write_fence_postgres_tx(&mut tx, &identity.account_id)?;
    let mut upload =
        load_upload_postgres(&mut tx, upload_id)?.ok_or(UploadControlError::UploadNotFound)?;
    if account_object != is_stored_account_object(&upload) {
        return Err(UploadControlError::IdempotencyConflict.into());
    }
    if matches!(upload.state.as_str(), "delete_pending" | "deleted") {
        return Err(UploadControlError::UploadGone.into());
    }
    match account_fence {
        AccountWriteFence::Active => {}
        AccountWriteFence::DeletionRequested => {
            schedule_upload_cleanup_postgres_tx(&mut tx, upload_id, now_ms)?;
            tx.commit()?;
            return Err(UploadControlError::AccountDeleting.into());
        }
        AccountWriteFence::Missing => return Err(UploadControlError::SessionNotOwned.into()),
    }
    if !account_object && !upload_parent_is_live_postgres(&mut tx, &upload)? {
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
    if !account_object
        && upload.object_kind == ObjectKind::Artifact.as_str()
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
    if !account_object {
        publish_index_postgres(&mut tx, &upload)?;
    }
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
    let mut jobs =
        claim_existing_cleanup_jobs_sqlite_tx(&tx, Some(account_id), storage_scope, now_ms, limit)?;
    let claimed = i64::try_from(jobs.len()).context("convert SQLite cleanup batch size")?;
    let remaining = limit.saturating_sub(claimed);
    if remaining > 0 {
        jobs.extend(schedule_cleanup_candidates_sqlite_tx(
            &tx,
            Some(account_id),
            storage_scope,
            now_ms,
            stale_pending_before_ms,
            remaining,
        )?);
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
    let mut jobs = claim_existing_cleanup_jobs_postgres_tx(
        &mut tx,
        Some(account_id),
        storage_scope,
        now_ms,
        limit,
    )?;
    let claimed = i64::try_from(jobs.len()).context("convert Postgres cleanup batch size")?;
    let remaining = limit.saturating_sub(claimed);
    if remaining > 0 {
        jobs.extend(schedule_cleanup_candidates_postgres_tx(
            &mut tx,
            Some(account_id),
            storage_scope,
            now_ms,
            stale_pending_before_ms,
            remaining,
        )?);
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
    let mut jobs = claim_existing_cleanup_jobs_sqlite_tx(&tx, None, storage_scope, now_ms, limit)?;
    let claimed = i64::try_from(jobs.len()).context("convert global SQLite cleanup batch size")?;
    let remaining = limit.saturating_sub(claimed);
    if remaining > 0 {
        jobs.extend(schedule_cleanup_candidates_sqlite_tx(
            &tx,
            None,
            storage_scope,
            now_ms,
            stale_pending_before_ms,
            remaining,
        )?);
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
    let mut jobs =
        claim_existing_cleanup_jobs_postgres_tx(&mut tx, None, storage_scope, now_ms, limit)?;
    let claimed =
        i64::try_from(jobs.len()).context("convert global Postgres cleanup batch size")?;
    let remaining = limit.saturating_sub(claimed);
    if remaining > 0 {
        jobs.extend(schedule_cleanup_candidates_postgres_tx(
            &mut tx,
            None,
            storage_scope,
            now_ms,
            stale_pending_before_ms,
            remaining,
        )?);
    }
    tx.commit()?;
    Ok(jobs)
}

fn claim_existing_cleanup_jobs_sqlite_tx(
    tx: &SqliteTransaction<'_>,
    account_id: Option<&str>,
    storage_scope: StorageScope,
    now_ms: i64,
    limit: i64,
) -> Result<Vec<CleanupJob>> {
    let processing_expired_before_ms = now_ms.saturating_sub(PROCESSING_LEASE_MS);
    let mut stmt = tx.prepare(
        "SELECT outbox.upload_id, upload.account_id, upload.object_key
           FROM object_storage_outbox outbox
           JOIN object_uploads upload ON upload.id = outbox.upload_id
          WHERE (?1 IS NULL OR outbox.account_id = ?1)
            AND upload.storage_scope = ?2 AND upload.state = 'delete_pending'
            AND outbox.operation = 'delete'
            AND ((outbox.state IN ('pending', 'retry')
                  AND outbox.next_attempt_at_ms <= ?3)
              OR (outbox.state = 'processing' AND outbox.updated_at_ms <= ?4))
          ORDER BY outbox.created_at_ms, outbox.id
          LIMIT ?5",
    )?;
    let rows = stmt.query_map(
        params![
            account_id,
            storage_scope.as_str(),
            now_ms,
            processing_expired_before_ms,
            limit,
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
        if tx.execute(
            "UPDATE object_storage_outbox
                SET state = 'processing', attempt_count = attempt_count + 1,
                    next_attempt_at_ms = ?2, updated_at_ms = ?2
              WHERE upload_id = ?1 AND operation = 'delete'",
            params![job.upload_id, now_ms],
        )? != 1
        {
            return Err(UploadControlError::UploadInProgress.into());
        }
    }
    Ok(jobs)
}

fn claim_existing_cleanup_jobs_postgres_tx(
    tx: &mut PgTransaction<'_>,
    account_id: Option<&str>,
    storage_scope: StorageScope,
    now_ms: i64,
    limit: i64,
) -> Result<Vec<CleanupJob>> {
    let processing_expired_before_ms = now_ms.saturating_sub(PROCESSING_LEASE_MS);
    let rows = tx.query(
        POSTGRES_CLAIM_EXISTING_CLEANUP_SQL,
        &[
            &account_id,
            &storage_scope.as_str(),
            &now_ms,
            &processing_expired_before_ms,
            &limit,
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
        if tx.execute(
            "UPDATE object_storage_outbox
                SET state = 'processing', attempt_count = attempt_count + 1,
                    next_attempt_at_ms = $2, updated_at_ms = $2
              WHERE upload_id = $1 AND operation = 'delete'",
            &[&job.upload_id, &now_ms],
        )? != 1
        {
            return Err(UploadControlError::UploadInProgress.into());
        }
    }
    Ok(jobs)
}

fn schedule_cleanup_candidates_sqlite_tx(
    tx: &SqliteTransaction<'_>,
    account_id: Option<&str>,
    storage_scope: StorageScope,
    now_ms: i64,
    stale_pending_before_ms: i64,
    limit: i64,
) -> Result<Vec<CleanupJob>> {
    let mut stmt = tx.prepare(
        "SELECT upload.id, upload.account_id, upload.object_key
           FROM object_uploads upload
          WHERE (?1 IS NULL OR upload.account_id = ?1)
            AND upload.storage_scope = ?2
            AND ((upload.state = 'ready' AND upload.expires_at_ms <= ?4)
              OR (upload.state = 'pending' AND upload.updated_at_ms <= ?3
                  AND NOT EXISTS (
                      SELECT 1
                        FROM jobs_submission_evidence_capacity capacity
                       WHERE capacity.account_id = upload.account_id
                         AND capacity.state = 'active'
                         AND capacity.expires_at_ms > ?4
                         AND capacity.application_id = CASE
                              WHEN json_valid(upload.metadata_json)
                              THEN json_extract(
                                  upload.metadata_json,
                                  '$.jobs_application_id'
                              )
                             END
                         AND capacity.run_id = CASE
                              WHEN json_valid(upload.metadata_json)
                              THEN json_extract(upload.metadata_json, '$.jobs_run_id')
                             END
                         AND capacity.runner = CASE
                              WHEN json_valid(upload.metadata_json)
                              THEN json_extract(upload.metadata_json, '$.jobs_runner')
                             END
                         AND CASE
                              WHEN json_valid(upload.metadata_json)
                              THEN json_extract(upload.metadata_json, '$.artifact_class')
                             END = 'jobs_submission_evidence'
                  ))
              OR (upload.state = 'delete_pending'
                  AND NOT EXISTS (
                      SELECT 1
                        FROM object_storage_outbox deletion
                       WHERE deletion.upload_id = upload.id
                         AND deletion.operation = 'delete'
                         AND deletion.state IN ('pending', 'processing', 'retry')
                  )))
          ORDER BY upload.created_at_ms, upload.id
          LIMIT ?5",
    )?;
    let rows = stmt.query_map(
        params![
            account_id,
            storage_scope.as_str(),
            stale_pending_before_ms,
            now_ms,
            limit,
        ],
        |row| {
            Ok(CleanupJob {
                upload_id: row.get(0)?,
                account_id: row.get(1)?,
                object_key: row.get(2)?,
            })
        },
    )?;
    let candidates = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    drop(stmt);

    let mut jobs = Vec::with_capacity(candidates.len());
    let processing_expired_before_ms = now_ms.saturating_sub(PROCESSING_LEASE_MS);
    for candidate in candidates {
        if tx.execute(
            "UPDATE object_uploads
                SET state = 'delete_pending', updated_at_ms = ?2
              WHERE id = ?1 AND state IN ('ready', 'pending', 'delete_pending')",
            params![candidate.upload_id, now_ms],
        )? != 1
        {
            continue;
        }
        tx.execute(
            "INSERT INTO object_storage_outbox (
                id, upload_id, account_id, operation, state, next_attempt_at_ms,
                created_at_ms, updated_at_ms
             ) VALUES (?1 || ':delete', ?1, ?2, 'delete', 'pending', ?3, ?3, ?3)
             ON CONFLICT(upload_id, operation) DO UPDATE SET
                state = 'pending', next_attempt_at_ms = excluded.next_attempt_at_ms,
                last_error = NULL, updated_at_ms = excluded.updated_at_ms,
                completed_at_ms = NULL
              WHERE object_storage_outbox.state IN ('completed', 'abandoned')",
            params![candidate.upload_id, candidate.account_id, now_ms],
        )?;
        tx.execute(
            "UPDATE object_storage_outbox
                SET state = 'abandoned', updated_at_ms = ?2, completed_at_ms = ?2
              WHERE upload_id = ?1 AND operation = 'put' AND state <> 'completed'",
            params![candidate.upload_id, now_ms],
        )?;
        if tx.execute(
            "UPDATE object_storage_outbox
                SET state = 'processing', attempt_count = attempt_count + 1,
                    next_attempt_at_ms = ?2, updated_at_ms = ?2
              WHERE upload_id = ?1 AND operation = 'delete'
                AND ((state IN ('pending', 'retry') AND next_attempt_at_ms <= ?2)
                  OR (state = 'processing' AND updated_at_ms <= ?3))",
            params![candidate.upload_id, now_ms, processing_expired_before_ms],
        )? == 1
        {
            jobs.push(candidate);
        }
    }
    Ok(jobs)
}

fn schedule_cleanup_candidates_postgres_tx(
    tx: &mut PgTransaction<'_>,
    account_id: Option<&str>,
    storage_scope: StorageScope,
    now_ms: i64,
    stale_pending_before_ms: i64,
    limit: i64,
) -> Result<Vec<CleanupJob>> {
    let account_ids = lock_cleanup_candidate_accounts_postgres_tx(
        tx,
        account_id,
        storage_scope,
        now_ms,
        stale_pending_before_ms,
        limit,
    )?;
    if account_ids.is_empty() {
        return Ok(Vec::new());
    }
    let rows = tx.query(
        &postgres_cleanup_candidates_query(),
        &[
            &storage_scope.as_str(),
            &stale_pending_before_ms,
            &now_ms,
            &account_ids,
            &limit,
        ],
    )?;
    let candidates = rows
        .iter()
        .map(|row| CleanupJob {
            upload_id: row.get(0),
            account_id: row.get(1),
            object_key: row.get(2),
        })
        .collect::<Vec<_>>();

    let mut jobs = Vec::with_capacity(candidates.len());
    let processing_expired_before_ms = now_ms.saturating_sub(PROCESSING_LEASE_MS);
    for candidate in candidates {
        if tx.execute(
            "UPDATE object_uploads
                SET state = 'delete_pending', updated_at_ms = $2
              WHERE id = $1 AND state IN ('ready', 'pending', 'delete_pending')",
            &[&candidate.upload_id, &now_ms],
        )? != 1
        {
            continue;
        }
        tx.execute(
            "INSERT INTO object_storage_outbox (
                id, upload_id, account_id, operation, state, next_attempt_at_ms,
                created_at_ms, updated_at_ms
             ) VALUES ($1 || ':delete', $1, $2, 'delete', 'pending', $3, $3, $3)
             ON CONFLICT(upload_id, operation) DO UPDATE SET
                state = 'pending', next_attempt_at_ms = EXCLUDED.next_attempt_at_ms,
                last_error = NULL, updated_at_ms = EXCLUDED.updated_at_ms,
                completed_at_ms = NULL
              WHERE object_storage_outbox.state IN ('completed', 'abandoned')",
            &[&candidate.upload_id, &candidate.account_id, &now_ms],
        )?;
        tx.execute(
            "UPDATE object_storage_outbox
                SET state = 'abandoned', updated_at_ms = $2, completed_at_ms = $2
              WHERE upload_id = $1 AND operation = 'put' AND state <> 'completed'",
            &[&candidate.upload_id, &now_ms],
        )?;
        if tx.execute(
            "UPDATE object_storage_outbox
                SET state = 'processing', attempt_count = attempt_count + 1,
                    next_attempt_at_ms = $2, updated_at_ms = $2
              WHERE upload_id = $1 AND operation = 'delete'
                AND ((state IN ('pending', 'retry') AND next_attempt_at_ms <= $2)
                  OR (state = 'processing' AND updated_at_ms <= $3))",
            &[&candidate.upload_id, &now_ms, &processing_expired_before_ms],
        )? == 1
        {
            jobs.push(candidate);
        }
    }
    Ok(jobs)
}

fn lock_cleanup_candidate_accounts_postgres_tx(
    tx: &mut PgTransaction<'_>,
    account_id: Option<&str>,
    storage_scope: StorageScope,
    now_ms: i64,
    stale_pending_before_ms: i64,
    limit: i64,
) -> Result<Vec<String>> {
    if let Some(account_id) = account_id {
        return match account_data::account_write_fence_postgres_tx(tx, account_id)? {
            AccountWriteFence::Missing => Ok(Vec::new()),
            AccountWriteFence::Active | AccountWriteFence::DeletionRequested => {
                Ok(vec![account_id.to_string()])
            }
        };
    }

    // Receipt authority locks account -> application/capacity. Cleanup takes
    // the same first lock, but only for a bounded, stable candidate set. Busy
    // accounts are skipped so one tenant cannot stall the global worker.
    Ok(tx
        .query(
            &postgres_cleanup_candidate_accounts_query(),
            &[
                &storage_scope.as_str(),
                &stale_pending_before_ms,
                &now_ms,
                &limit,
            ],
        )?
        .iter()
        .map(|row| row.get(0))
        .collect())
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
    if upload.state == "deleted" {
        tx.commit()?;
        return Ok(());
    }
    if upload.state != "delete_pending" {
        return Err(UploadControlError::UploadInProgress.into());
    }
    restore_submission_capacity_after_cleanup_sqlite_tx(&tx, &upload, now_ms)?;
    if tx.execute(
        "UPDATE object_uploads
            SET state = 'deleted', deleted_at_ms = ?2, updated_at_ms = ?2
          WHERE id = ?1 AND state = 'delete_pending'",
        params![upload_id, now_ms],
    )? != 1
    {
        return Err(UploadControlError::UploadInProgress.into());
    }
    if tx.execute(
        "UPDATE object_storage_outbox
            SET state = 'completed', completed_at_ms = ?2, updated_at_ms = ?2,
                next_attempt_at_ms = ?2, last_error = NULL
          WHERE upload_id = ?1 AND operation = 'delete'
            AND state IN ('pending', 'processing', 'retry')",
        params![upload_id, now_ms],
    )? != 1
    {
        return Err(UploadControlError::UploadNotFound.into());
    }
    remove_published_index_sqlite(&tx, &upload)?;
    tx.commit()?;
    Ok(())
}

fn mark_cleanup_succeeded_postgres(pool: &DbPool, upload_id: &str, now_ms: i64) -> Result<()> {
    let mut conn = pool.get_pg()?;
    let mut tx = conn.transaction()?;
    let identity = load_upload_postgres_unlocked(&mut tx, upload_id)?
        .ok_or(UploadControlError::UploadNotFound)?;
    if identity.object_kind == ObjectKind::Artifact.as_str() {
        lock_context_artifact_postgres_tx(&mut tx, &identity.logical_id)?;
    }
    if let Some(session_id) = identity.session_id.as_deref() {
        lock_session_postgres_tx(&mut tx, session_id)?;
    }
    if account_data::account_write_fence_postgres_tx(&mut tx, &identity.account_id)?
        == AccountWriteFence::Missing
    {
        return Err(UploadControlError::UploadNotFound.into());
    }
    let upload =
        load_upload_postgres(&mut tx, upload_id)?.ok_or(UploadControlError::UploadNotFound)?;
    if upload.state == "deleted" {
        tx.commit()?;
        return Ok(());
    }
    if upload.state != "delete_pending" {
        return Err(UploadControlError::UploadInProgress.into());
    }
    restore_submission_capacity_after_cleanup_postgres_tx(&mut tx, &upload, now_ms)?;
    if tx.execute(
        "UPDATE object_uploads
            SET state = 'deleted', deleted_at_ms = $2, updated_at_ms = $2
          WHERE id = $1 AND state = 'delete_pending'",
        &[&upload_id, &now_ms],
    )? != 1
    {
        return Err(UploadControlError::UploadInProgress.into());
    }
    if tx.execute(
        "UPDATE object_storage_outbox
            SET state = 'completed', completed_at_ms = $2, updated_at_ms = $2,
                next_attempt_at_ms = $2, last_error = NULL
          WHERE upload_id = $1 AND operation = 'delete'
            AND state IN ('pending', 'processing', 'retry')",
        &[&upload_id, &now_ms],
    )? != 1
    {
        return Err(UploadControlError::UploadNotFound.into());
    }
    remove_published_index_postgres(&mut tx, &upload)?;
    tx.commit()?;
    Ok(())
}

fn restore_submission_capacity_after_cleanup_sqlite_tx(
    tx: &SqliteTransaction<'_>,
    upload: &ObjectUpload,
    now_ms: i64,
) -> Result<()> {
    let Some((application_id, run_id)) = upload_submission_capacity_parent(upload)? else {
        return Ok(());
    };
    tx.execute(
        "UPDATE jobs_submission_evidence_capacity
            SET consumed_bytes = consumed_bytes - ?4,
                consumed_objects = consumed_objects - 1,
                updated_at_ms = ?5
          WHERE account_id = ?1 AND application_id = ?2 AND run_id = ?3
            AND state = 'active' AND consumed_bytes >= ?4 AND consumed_objects >= 1",
        params![
            upload.account_id,
            application_id,
            run_id,
            upload.size_bytes,
            now_ms,
        ],
    )?;
    Ok(())
}

fn restore_submission_capacity_after_cleanup_postgres_tx(
    tx: &mut PgTransaction<'_>,
    upload: &ObjectUpload,
    now_ms: i64,
) -> Result<()> {
    let Some((application_id, run_id)) = upload_submission_capacity_parent(upload)? else {
        return Ok(());
    };
    tx.execute(
        "UPDATE jobs_submission_evidence_capacity
            SET consumed_bytes = consumed_bytes - $4,
                consumed_objects = consumed_objects - 1,
                updated_at_ms = $5
          WHERE account_id = $1 AND application_id = $2 AND run_id = $3
            AND state = 'active' AND consumed_bytes >= $4 AND consumed_objects >= 1",
        &[
            &upload.account_id,
            &application_id,
            &run_id,
            &upload.size_bytes,
            &now_ms,
        ],
    )?;
    Ok(())
}

fn upload_submission_capacity_parent(upload: &ObjectUpload) -> Result<Option<(String, String)>> {
    let metadata = serde_json::from_str::<serde_json::Value>(&upload.metadata_json)
        .context("parse object upload cleanup metadata")?;
    if metadata
        .get("artifact_class")
        .and_then(serde_json::Value::as_str)
        != Some("jobs_submission_evidence")
    {
        return Ok(None);
    }
    let application_id = metadata
        .get("jobs_application_id")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or(UploadControlError::InvalidMetadata("parent application"))?;
    let run_id = metadata
        .get("jobs_run_id")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or(UploadControlError::InvalidMetadata("submission run"))?;
    Ok(Some((application_id.to_string(), run_id.to_string())))
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

const SUBMISSION_CAPACITY_COLUMNS: &str = "account_id, application_id, run_id, runner,
    reserved_bytes, reserved_objects, consumed_bytes, consumed_objects, state,
    expires_at_ms, created_at_ms, updated_at_ms, completed_at_ms";

fn load_submission_evidence_capacity_sqlite(
    tx: &SqliteTransaction<'_>,
    account_id: &str,
    application_id: &str,
    run_id: &str,
) -> Result<Option<SubmissionEvidenceCapacity>> {
    tx.query_row(
        &format!(
            "SELECT {SUBMISSION_CAPACITY_COLUMNS}
               FROM jobs_submission_evidence_capacity
              WHERE account_id = ?1 AND application_id = ?2 AND run_id = ?3"
        ),
        params![account_id, application_id, run_id],
        row_to_submission_evidence_capacity_sqlite,
    )
    .optional()
    .map_err(Into::into)
}

fn load_submission_evidence_capacity_postgres(
    tx: &mut PgTransaction<'_>,
    account_id: &str,
    application_id: &str,
    run_id: &str,
) -> Result<Option<SubmissionEvidenceCapacity>> {
    tx.query_opt(
        &format!(
            "SELECT {SUBMISSION_CAPACITY_COLUMNS}
               FROM jobs_submission_evidence_capacity
              WHERE account_id = $1 AND application_id = $2 AND run_id = $3
              FOR UPDATE"
        ),
        &[&account_id, &application_id, &run_id],
    )?
    .map(row_to_submission_evidence_capacity_postgres)
    .transpose()
}

fn row_to_submission_evidence_capacity_sqlite(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<SubmissionEvidenceCapacity> {
    Ok(SubmissionEvidenceCapacity {
        account_id: row.get(0)?,
        application_id: row.get(1)?,
        run_id: row.get(2)?,
        runner: row.get(3)?,
        reserved_bytes: row.get(4)?,
        reserved_objects: row.get(5)?,
        consumed_bytes: row.get(6)?,
        consumed_objects: row.get(7)?,
        state: row.get(8)?,
        expires_at_ms: row.get(9)?,
        created_at_ms: row.get(10)?,
        updated_at_ms: row.get(11)?,
        completed_at_ms: row.get(12)?,
    })
}

fn row_to_submission_evidence_capacity_postgres(row: PgRow) -> Result<SubmissionEvidenceCapacity> {
    Ok(SubmissionEvidenceCapacity {
        account_id: row.try_get(0)?,
        application_id: row.try_get(1)?,
        run_id: row.try_get(2)?,
        runner: row.try_get(3)?,
        reserved_bytes: row.try_get(4)?,
        reserved_objects: row.try_get(5)?,
        consumed_bytes: row.try_get(6)?,
        consumed_objects: row.try_get(7)?,
        state: row.try_get(8)?,
        expires_at_ms: row.try_get(9)?,
        created_at_ms: row.try_get(10)?,
        updated_at_ms: row.try_get(11)?,
        completed_at_ms: row.try_get(12)?,
    })
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
              WHERE account_id = ?1 AND storage_scope = 'artifact' AND object_key = ?2"
        ),
        params![account_id, object_key],
        row_to_upload_sqlite,
    )
    .optional()
    .map_err(Into::into)
}

fn load_account_object_by_key_postgres_unlocked(
    tx: &mut PgTransaction<'_>,
    account_id: &str,
    object_key: &str,
) -> Result<Option<ObjectUpload>> {
    tx.query_opt(
        &format!(
            "SELECT {UPLOAD_COLUMNS} FROM object_uploads
              WHERE account_id = $1 AND storage_scope = 'artifact' AND object_key = $2"
        ),
        &[&account_id, &object_key],
    )?
    .map(row_to_upload_postgres)
    .transpose()
}

fn load_account_object_by_key_postgres(
    tx: &mut PgTransaction<'_>,
    account_id: &str,
    object_key: &str,
) -> Result<Option<ObjectUpload>> {
    tx.query_opt(
        &format!(
            "SELECT {UPLOAD_COLUMNS} FROM object_uploads
              WHERE account_id = $1 AND storage_scope = 'artifact' AND object_key = $2
              FOR UPDATE"
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

    fn account_object_input(
        logical_id: &str,
        artifact_class: &str,
        size_bytes: i64,
        now_ms: i64,
    ) -> NewObjectUpload {
        let hash = sha256_hex(format!("account-object-{logical_id}"));
        NewObjectUpload {
            account_id: "acct_1".into(),
            object_kind: ObjectKind::Artifact,
            logical_id: logical_id.into(),
            session_id: None,
            storage_scope: StorageScope::Artifact,
            object_key: format!(
                "objects/accounts/acct_1/jobs/{artifact_class}/{logical_id}/{hash}"
            ),
            size_bytes,
            sha256: hash,
            content_type: "application/octet-stream".into(),
            expires_at_ms: now_ms + DAY_MS,
            metadata_json: serde_json::json!({
                "artifact_class": artifact_class,
                "logical_id": logical_id,
            }),
            now_ms,
            limits: UploadLimits {
                max_object_bytes: 100,
                max_account_bytes: 150,
                max_daily_bytes: 120,
                max_account_objects: 10,
            },
        }
    }

    fn insert_test_application(pool: &DbPool, application_id: &str) {
        let job_id = format!("job-{application_id}");
        let conn = pool.get().unwrap();
        conn.execute(
            "INSERT INTO jobs_postings (
                id, account_id, canonical_key, posting_json, source, company, title,
                created_at_ms, updated_at_ms
             ) VALUES (?1, 'acct_1', ?2, '{}', 'test', 'Acme', 'Engineer', 1, 1)",
            params![job_id, format!("canonical-{application_id}")],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO jobs_applications (
                id, account_id, job_id, state, application_json, created_at_ms, updated_at_ms
             ) VALUES (?1, 'acct_1', ?2, 'running', '{}', 1, 1)",
            params![application_id, job_id],
        )
        .unwrap();
    }

    fn capacity_input(
        application_id: &str,
        run_id: &str,
        now_ms: i64,
    ) -> NewSubmissionEvidenceCapacity {
        NewSubmissionEvidenceCapacity {
            account_id: "acct_1".into(),
            application_id: application_id.into(),
            run_id: run_id.into(),
            runner: "cloud".into(),
            reserved_bytes: 100,
            reserved_objects: 3,
            expires_at_ms: now_ms + DAY_MS,
            now_ms,
            limits: UploadLimits {
                max_object_bytes: 100,
                max_account_bytes: 150,
                max_daily_bytes: 10,
                max_account_objects: 10,
            },
        }
    }

    fn submission_object_input(
        application_id: &str,
        run_id: &str,
        logical_id: &str,
        size_bytes: i64,
        now_ms: i64,
    ) -> NewObjectUpload {
        let hash = sha256_hex(format!("submission-{logical_id}"));
        NewObjectUpload {
            account_id: "acct_1".into(),
            object_kind: ObjectKind::Artifact,
            logical_id: logical_id.into(),
            session_id: None,
            storage_scope: StorageScope::Artifact,
            object_key: format!("objects/accounts/acct_1/jobs/{logical_id}/{hash}"),
            size_bytes,
            sha256: hash,
            content_type: "application/pdf".into(),
            expires_at_ms: i64::MAX,
            metadata_json: serde_json::json!({
                "artifact_class": "jobs_submission_evidence",
                "jobs_application_id": application_id,
                "jobs_run_id": run_id,
                "jobs_runner": "cloud",
                "evidence_kind": "resume",
            }),
            now_ms,
            limits: UploadLimits {
                max_object_bytes: 100,
                max_account_bytes: 150,
                max_daily_bytes: 10,
                max_account_objects: 10,
            },
        }
    }

    fn postgres_audit_input(
        account_id: &str,
        session_id: &str,
        logical_id: &str,
        now_ms: i64,
    ) -> NewObjectUpload {
        let hash = sha256_hex(format!("audit-{logical_id}"));
        NewObjectUpload {
            account_id: account_id.into(),
            object_kind: ObjectKind::SessionAudit,
            logical_id: logical_id.into(),
            session_id: Some(session_id.into()),
            storage_scope: StorageScope::Audit,
            object_key: format!("logs/accounts/{account_id}/{logical_id}/{hash}"),
            size_bytes: 30,
            sha256: hash,
            content_type: "application/json".into(),
            expires_at_ms: now_ms + DAY_MS,
            metadata_json: serde_json::json!({"bundle_id": logical_id}),
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
    fn postgres_cleanup_sql_guards_legacy_json_and_bounds_every_lock_set() {
        let validity_check = POSTGRES_SAFE_UPLOAD_METADATA_JOIN
            .find("metadata_json IS JSON OBJECT")
            .expect("cleanup SQL must validate legacy metadata text");
        let jsonb_cast = POSTGRES_SAFE_UPLOAD_METADATA_JOIN
            .find("metadata_json::jsonb")
            .expect("valid cleanup metadata may be converted to jsonb");
        assert!(
            validity_check < jsonb_cast,
            "the non-throwing validity check must guard the jsonb cast"
        );
        assert!(
            !POSTGRES_CLEANUP_CANDIDATE_PREDICATE.contains("::jsonb"),
            "capacity matching must consume only the safely parsed value"
        );

        let account_query = postgres_cleanup_candidate_accounts_query();
        assert!(account_query.contains("FOR UPDATE OF account_row SKIP LOCKED"));
        assert!(account_query.contains("LIMIT $4"));
        assert!(account_query.contains("metadata_json IS JSON OBJECT"));

        let upload_query = postgres_cleanup_candidates_query();
        assert!(upload_query.contains("FOR UPDATE OF upload SKIP LOCKED"));
        assert!(upload_query.contains("LIMIT $5"));
        assert!(upload_query.contains("metadata_json IS JSON OBJECT"));

        assert!(POSTGRES_CLAIM_EXISTING_CLEANUP_SQL.contains("FOR UPDATE OF outbox SKIP LOCKED"));
        assert!(POSTGRES_CLAIM_EXISTING_CLEANUP_SQL.contains("LIMIT $5"));
        assert!(POSTGRES_CLAIM_EXISTING_CLEANUP_SQL.contains("outbox.state = 'processing'"));
    }

    #[test]
    fn account_object_reservation_is_strictly_classed_and_idempotent() {
        let pool = test_pool();
        let source = account_object_input("resume-source-1", "jobs_resume_source", 40, 1_000);
        let reservation = reserve_account_object_upload(&pool, &source).unwrap();
        assert!(reservation.needs_put);
        assert!(reservation.upload.session_id.is_none());
        assert_eq!(reservation.upload.state, "pending");
        mark_upload_ready(&pool, &reservation.upload.id, 1_100).unwrap();

        let replay = reserve_account_object_upload(&pool, &source).unwrap();
        assert_eq!(replay.upload.id, reservation.upload.id);
        assert!(!replay.needs_put);
        let usage: (i64, i64) = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT reserved_bytes, reserved_objects
                   FROM object_upload_daily_usage WHERE account_id = 'acct_1'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(
            usage,
            (40, 1),
            "an exact replay must not reserve quota twice"
        );

        let mut changed = source.clone();
        changed.metadata_json["logical_id"] = "different".into();
        let error = reserve_account_object_upload(&pool, &changed)
            .expect_err("changed immutable metadata must conflict");
        assert_eq!(
            error.downcast_ref::<UploadControlError>(),
            Some(&UploadControlError::IdempotencyConflict)
        );

        let snapshot = account_object_input(
            "profile-generation-1",
            "jobs_browser_profile_snapshot",
            40,
            1_200,
        );
        reserve_account_object_upload(&pool, &snapshot)
            .expect("browser profile snapshots use the same durable lifecycle");
        let mut unsupported = account_object_input("unsupported", "jobs_other", 1, 1_300);
        let error = reserve_account_object_upload(&pool, &unsupported)
            .expect_err("unrecognized account object classes must fail closed");
        assert_eq!(
            error.downcast_ref::<UploadControlError>(),
            Some(&UploadControlError::InvalidMetadata("account object class"))
        );
        unsupported.metadata_json["artifact_class"] = "jobs_resume_source".into();
        unsupported.session_id = Some("session_1".into());
        let error = reserve_account_object_upload(&pool, &unsupported)
            .expect_err("account objects cannot acquire a session parent");
        assert_eq!(
            error.downcast_ref::<UploadControlError>(),
            Some(&UploadControlError::InvalidMetadata("account object kind"))
        );
    }

    #[test]
    fn account_object_reservation_enforces_quota_and_deletion_fence() {
        let pool = test_pool();
        let mut first = account_object_input("quota-first", "jobs_resume_source", 40, 1_000);
        first.limits.max_account_bytes = 50;
        reserve_account_object_upload(&pool, &first).unwrap();

        let mut total_limited =
            account_object_input("quota-total", "jobs_resume_source", 20, 1_100);
        total_limited.limits.max_account_bytes = 50;
        let error = reserve_account_object_upload(&pool, &total_limited)
            .expect_err("account-parented objects count against total quota");
        assert_eq!(
            error.downcast_ref::<UploadControlError>(),
            Some(&UploadControlError::AccountBytesQuotaExceeded)
        );

        let mut daily_limited =
            account_object_input("quota-daily", "jobs_resume_source", 20, 1_200);
        daily_limited.limits.max_daily_bytes = 50;
        let error = reserve_account_object_upload(&pool, &daily_limited)
            .expect_err("account-parented objects count against daily quota");
        assert_eq!(
            error.downcast_ref::<UploadControlError>(),
            Some(&UploadControlError::DailyQuotaExceeded)
        );

        let fenced_pool = test_pool();
        assert!(matches!(
            crate::db::account_data::begin_account_deletion(&fenced_pool, "acct_1", 2_000).unwrap(),
            Some(crate::db::account_data::BeginAccountDeletionResult::Ready(
                _
            ))
        ));
        let error = reserve_account_object_upload(
            &fenced_pool,
            &account_object_input("after-delete", "jobs_resume_source", 20, 2_100),
        )
        .expect_err("account deletion must fence account-object reservation");
        assert_eq!(
            error.downcast_ref::<UploadControlError>(),
            Some(&UploadControlError::AccountDeleting)
        );
    }

    #[test]
    fn account_object_cleanup_preserves_tombstones_and_rejects_recreation() {
        let pool = test_pool();
        let source = account_object_input("cleanup-source", "jobs_resume_source", 40, 1_000);
        let snapshot = account_object_input(
            "cleanup-profile",
            "jobs_browser_profile_snapshot",
            40,
            1_001,
        );
        let source_reservation = reserve_account_object_upload(&pool, &source).unwrap();
        let snapshot_reservation = reserve_account_object_upload(&pool, &snapshot).unwrap();
        mark_upload_ready(&pool, &source_reservation.upload.id, 1_100).unwrap();

        assert!(
            schedule_account_object_cleanup(&pool, "acct_2", &source.object_key, 1_200,)
                .unwrap()
                .is_none()
        );
        let cleanup = schedule_account_object_cleanup(&pool, "acct_1", &source.object_key, 1_200)
            .unwrap()
            .expect("exact source object must be durably scheduled");
        assert_eq!(cleanup.upload_id, source_reservation.upload.id);
        assert_eq!(
            schedule_account_object_cleanup(&pool, "acct_1", &source.object_key, 1_201)
                .unwrap()
                .expect("cleanup scheduling is replay-safe")
                .upload_id,
            cleanup.upload_id
        );

        let states: (String, String, String, String) = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT source.state, source_put.state, source_delete.state, snapshot.state
                   FROM object_uploads source
                   JOIN object_storage_outbox source_put
                     ON source_put.upload_id = source.id AND source_put.operation = 'put'
                   JOIN object_storage_outbox source_delete
                     ON source_delete.upload_id = source.id AND source_delete.operation = 'delete'
                   JOIN object_uploads snapshot ON snapshot.id = ?2
                  WHERE source.id = ?1",
                params![source_reservation.upload.id, snapshot_reservation.upload.id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap();
        assert_eq!(
            states,
            (
                "delete_pending".into(),
                "completed".into(),
                "pending".into(),
                "pending".into(),
            )
        );
        mark_cleanup_succeeded(&pool, &cleanup.upload_id, 1_300).unwrap();
        assert!(
            schedule_account_object_cleanup(&pool, "acct_1", &source.object_key, 1_400,)
                .unwrap()
                .is_none()
        );

        let mut exact_retry = source.clone();
        exact_retry.now_ms = 1_500;
        let durable_tombstone_before: (String, i64, i64, Option<i64>, Option<i64>) = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT state, created_at_ms, updated_at_ms, uploaded_at_ms, deleted_at_ms
                   FROM object_uploads WHERE id = ?1",
                params![source_reservation.upload.id],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(
            durable_tombstone_before,
            (
                "deleted".to_string(),
                1_000,
                1_300,
                Some(1_100),
                Some(1_300)
            )
        );
        let outboxes_before = pool
            .get()
            .unwrap()
            .prepare(
                "SELECT operation, state, attempt_count, next_attempt_at_ms, last_error,
                        created_at_ms, updated_at_ms, completed_at_ms
                   FROM object_storage_outbox
                  WHERE upload_id = ?1
                  ORDER BY operation",
            )
            .unwrap()
            .query_map(params![source_reservation.upload.id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, i64>(5)?,
                    row.get::<_, i64>(6)?,
                    row.get::<_, Option<i64>>(7)?,
                ))
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(
            outboxes_before,
            vec![
                (
                    "delete".to_string(),
                    "completed".to_string(),
                    0,
                    1_300,
                    None,
                    1_200,
                    1_300,
                    Some(1_300),
                ),
                (
                    "put".to_string(),
                    "completed".to_string(),
                    1,
                    1_100,
                    None,
                    1_000,
                    1_100,
                    Some(1_100),
                ),
            ]
        );

        let error = reserve_account_object_upload(&pool, &exact_retry)
            .expect_err("a deleted content-addressed object must keep its tombstone");
        assert_eq!(
            error.downcast_ref::<UploadControlError>(),
            Some(&UploadControlError::UploadGone)
        );
        let durable_tombstone_after: (String, i64, i64, Option<i64>, Option<i64>) = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT state, created_at_ms, updated_at_ms, uploaded_at_ms, deleted_at_ms
                   FROM object_uploads WHERE id = ?1",
                params![source_reservation.upload.id],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                    ))
                },
            )
            .unwrap();
        let outboxes_after = pool
            .get()
            .unwrap()
            .prepare(
                "SELECT operation, state, attempt_count, next_attempt_at_ms, last_error,
                        created_at_ms, updated_at_ms, completed_at_ms
                   FROM object_storage_outbox
                  WHERE upload_id = ?1
                  ORDER BY operation",
            )
            .unwrap()
            .query_map(params![source_reservation.upload.id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, i64>(5)?,
                    row.get::<_, i64>(6)?,
                    row.get::<_, Option<i64>>(7)?,
                ))
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(durable_tombstone_after, durable_tombstone_before);
        assert_eq!(outboxes_after, outboxes_before);
    }

    #[test]
    fn cleanup_batch_bounds_sqlite_scheduling_and_tolerates_malformed_metadata() {
        let pool = test_pool();
        let reservations = (0..3)
            .map(|index| {
                reserve_upload(
                    &pool,
                    &artifact_input(&format!("bounded-cleanup-{index}"), 30, 1_000 + index),
                )
                .unwrap()
            })
            .collect::<Vec<_>>();
        pool.get()
            .unwrap()
            .execute(
                "UPDATE object_uploads SET metadata_json = '{malformed'
                  WHERE id = ?1",
                params![reservations[0].upload.id],
            )
            .unwrap();

        let first = claim_global_cleanup_jobs(&pool, StorageScope::Artifact, 10_000, 2_000, 1)
            .expect("malformed legacy metadata must not poison SQLite cleanup");
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].upload_id, reservations[0].upload.id);
        let first_batch_counts: (i64, i64, i64) = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT
                    (SELECT COUNT(*) FROM object_uploads
                      WHERE state = 'delete_pending'),
                    (SELECT COUNT(*) FROM object_storage_outbox
                      WHERE operation = 'delete'),
                    (SELECT COUNT(*) FROM object_storage_outbox
                      WHERE operation = 'put' AND state = 'abandoned')",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(first_batch_counts, (1, 1, 1));

        let second =
            claim_global_cleanup_jobs(&pool, StorageScope::Artifact, 10_001, 2_000, 1).unwrap();
        assert_eq!(second.len(), 1);
        assert_eq!(second[0].upload_id, reservations[1].upload.id);
        let second_batch_counts: (i64, i64) = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT
                    (SELECT COUNT(*) FROM object_uploads
                      WHERE state = 'delete_pending'),
                    (SELECT COUNT(*) FROM object_storage_outbox
                      WHERE operation = 'delete')",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(second_batch_counts, (2, 2));
    }

    #[test]
    fn cleanup_batch_bounds_stale_processing_reclaims() {
        let pool = test_pool();
        let reservations = (0..3)
            .map(|index| {
                let reservation = reserve_upload(
                    &pool,
                    &artifact_input(&format!("bounded-reclaim-{index}"), 30, 1_000 + index),
                )
                .unwrap();
                schedule_upload_cleanup(&pool, &reservation.upload.id, 1_500 + index).unwrap();
                reservation
            })
            .collect::<Vec<_>>();
        pool.get()
            .unwrap()
            .execute(
                "UPDATE object_storage_outbox
                    SET state = 'processing', attempt_count = 1, updated_at_ms = 1_000
                  WHERE operation = 'delete'",
                [],
            )
            .unwrap();

        let now_ms = PROCESSING_LEASE_MS + 2_000;
        let jobs = claim_global_cleanup_jobs(&pool, StorageScope::Artifact, now_ms, 0, 1).unwrap();
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].upload_id, reservations[0].upload.id);
        let reclaim_counts: (i64, i64) = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT
                    SUM(CASE WHEN attempt_count = 2 AND updated_at_ms = ?1 THEN 1 ELSE 0 END),
                    SUM(CASE WHEN attempt_count = 1 AND updated_at_ms = 1_000 THEN 1 ELSE 0 END)
                   FROM object_storage_outbox
                  WHERE operation = 'delete'",
                params![now_ms],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(reclaim_counts, (1, 2));
    }

    #[test]
    fn protected_capacity_counts_against_total_but_not_daily_quota() {
        let pool = test_pool();
        insert_test_application(&pool, "app-capacity");
        let capacity = reserve_submission_evidence_capacity(
            &pool,
            &capacity_input("app-capacity", "run-capacity", 1_000),
        )
        .unwrap();
        assert_eq!(capacity.reserved_bytes, 100);
        assert_eq!(capacity.consumed_bytes, 0);
        let daily_rows: i64 = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM object_upload_daily_usage WHERE account_id = 'acct_1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(daily_rows, 0, "worst-case headroom is not daily usage");

        let error = reserve_upload(&pool, &artifact_input("generic-after-capacity", 60, 1_100))
            .expect_err("generic objects must count unused protected capacity");
        assert_eq!(
            error.downcast_ref::<UploadControlError>(),
            Some(&UploadControlError::AccountBytesQuotaExceeded)
        );
    }

    #[test]
    fn application_objects_convert_capacity_and_restore_it_after_cleanup() {
        let pool = test_pool();
        insert_test_application(&pool, "app-convert");
        reserve_submission_evidence_capacity(
            &pool,
            &capacity_input("app-convert", "run-convert", 1_000),
        )
        .unwrap();
        let first = reserve_application_object_upload(
            &pool,
            "app-convert",
            &submission_object_input("app-convert", "run-convert", "evidence-one", 40, 1_100),
        )
        .expect("actual evidence bypasses the lower generic daily limit");
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
        assert_eq!(
            usage,
            (40, 1),
            "actual bytes remain observable as daily usage"
        );

        let too_large = reserve_application_object_upload(
            &pool,
            "app-convert",
            &submission_object_input("app-convert", "run-convert", "evidence-two", 61, 1_200),
        )
        .expect_err("evidence cannot exceed the exact protected capacity");
        assert_eq!(
            too_large.downcast_ref::<UploadControlError>(),
            Some(&UploadControlError::SubmissionEvidenceCapacityExceeded)
        );

        schedule_upload_cleanup(&pool, &first.upload.id, 1_300).unwrap();
        mark_cleanup_succeeded(&pool, &first.upload.id, 1_400).unwrap();
        let restored: (i64, i64) = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT consumed_bytes, consumed_objects
                   FROM jobs_submission_evidence_capacity
                  WHERE account_id = 'acct_1' AND application_id = 'app-convert'
                    AND run_id = 'run-convert'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(restored, (0, 0));
        reserve_application_object_upload(
            &pool,
            "app-convert",
            &submission_object_input("app-convert", "run-convert", "evidence-three", 100, 1_500),
        )
        .expect("verified cleanup restores the exact active capacity");
    }

    #[test]
    fn stale_cleanup_preserves_pending_evidence_while_exact_capacity_is_active() {
        let pool = test_pool();
        insert_test_application(&pool, "app-stale-evidence");
        reserve_submission_evidence_capacity(
            &pool,
            &capacity_input("app-stale-evidence", "run-stale-evidence", 1_000),
        )
        .unwrap();
        let reservation = reserve_application_object_upload(
            &pool,
            "app-stale-evidence",
            &submission_object_input(
                "app-stale-evidence",
                "run-stale-evidence",
                "stale-evidence-object",
                40,
                1_100,
            ),
        )
        .unwrap();

        assert!(
            claim_cleanup_jobs(&pool, "acct_1", StorageScope::Artifact, 2_000, 1_500, 10,)
                .unwrap()
                .is_empty()
        );
        assert!(
            claim_global_cleanup_jobs(&pool, StorageScope::Artifact, 2_100, 1_500, 10,)
                .unwrap()
                .is_empty()
        );
        let protected = artifact_upload(&pool, "acct_1", "stale-evidence-object")
            .unwrap()
            .expect("active receipt authority must retain deterministic pending evidence");
        assert_eq!(protected.state, "pending");

        assert!(release_submission_evidence_capacity(
            &pool,
            "acct_1",
            "app-stale-evidence",
            "run-stale-evidence",
            2_200,
        )
        .unwrap());
        let cleanup =
            claim_cleanup_jobs(&pool, "acct_1", StorageScope::Artifact, 2_300, 2_200, 10).unwrap();
        assert_eq!(cleanup.len(), 1);
        assert_eq!(cleanup[0].upload_id, reservation.upload.id);
        let lifecycle: (String, String, String) = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT upload.state, put.state, deletion.state
                   FROM object_uploads upload
                   JOIN object_storage_outbox put
                     ON put.upload_id = upload.id AND put.operation = 'put'
                   JOIN object_storage_outbox deletion
                     ON deletion.upload_id = upload.id AND deletion.operation = 'delete'
                  WHERE upload.id = ?1",
                params![reservation.upload.id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(
            lifecycle,
            (
                "delete_pending".into(),
                "abandoned".into(),
                "processing".into(),
            )
        );
    }

    #[test]
    fn stale_cleanup_resumes_after_exact_capacity_expiry() {
        let pool = test_pool();
        insert_test_application(&pool, "app-expired-evidence");
        reserve_submission_evidence_capacity(
            &pool,
            &capacity_input("app-expired-evidence", "run-expired-evidence", 1_000),
        )
        .unwrap();
        let reservation = reserve_application_object_upload(
            &pool,
            "app-expired-evidence",
            &submission_object_input(
                "app-expired-evidence",
                "run-expired-evidence",
                "expired-evidence-object",
                40,
                1_100,
            ),
        )
        .unwrap();

        let cleanup =
            claim_global_cleanup_jobs(&pool, StorageScope::Artifact, DAY_MS + 2_000, 1_500, 10)
                .unwrap();
        assert_eq!(cleanup.len(), 1);
        assert_eq!(cleanup[0].upload_id, reservation.upload.id);
        let upload = artifact_upload(&pool, "acct_1", "expired-evidence-object")
            .unwrap()
            .expect("expired evidence remains ledgered until physical cleanup");
        assert_eq!(upload.state, "delete_pending");
    }

    #[test]
    fn capacity_expires_and_can_be_replaced_by_a_new_run() {
        let pool = test_pool();
        insert_test_application(&pool, "app-expiry");
        reserve_submission_evidence_capacity(
            &pool,
            &capacity_input("app-expiry", "run-old", 1_000),
        )
        .unwrap();
        let mut replacement = capacity_input("app-expiry", "run-new", DAY_MS + 1_000);
        replacement.limits.max_account_bytes = 150;
        reserve_submission_evidence_capacity(&pool, &replacement)
            .expect("expired headroom must not pin account quota or application uniqueness");
        let old_state: String = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT state FROM jobs_submission_evidence_capacity
                  WHERE account_id = 'acct_1' AND application_id = 'app-expiry'
                    AND run_id = 'run-old'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(old_state, "expired");
    }

    #[test]
    fn application_publication_commits_exact_consumed_capacity() {
        let pool = test_pool();
        insert_test_application(&pool, "app-commit");
        reserve_submission_evidence_capacity(
            &pool,
            &capacity_input("app-commit", "run-commit", 1_000),
        )
        .unwrap();
        let reservation = reserve_application_object_upload(
            &pool,
            "app-commit",
            &submission_object_input("app-commit", "run-commit", "receipt-object", 40, 1_100),
        )
        .unwrap();
        let binding = ApplicationObjectBinding {
            upload_id: reservation.upload.id.clone(),
            object_key: reservation.upload.object_key.clone(),
            size_bytes: reservation.upload.size_bytes,
            sha256: reservation.upload.sha256.clone(),
            content_type: reservation.upload.content_type.clone(),
        };
        let mut conn = pool.get().unwrap();
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        commit_application_object_uploads_sqlite_tx(
            &tx,
            "acct_1",
            "app-commit",
            "run-commit",
            "cloud",
            &[binding],
            1_200,
        )
        .unwrap();
        tx.commit().unwrap();
        drop(conn);

        let lifecycle: (String, String, i64, i64) = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT u.state, c.state, c.consumed_bytes, c.consumed_objects
                   FROM object_uploads u
                   JOIN jobs_submission_evidence_capacity c
                     ON c.account_id = u.account_id
                    AND c.application_id = 'app-commit' AND c.run_id = 'run-commit'
                  WHERE u.id = ?1",
                params![reservation.upload.id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap();
        assert_eq!(lifecycle, ("ready".into(), "committed".into(), 40, 1));
    }

    #[test]
    fn exact_publication_fences_and_cleanup_ledgers_a_parallel_receipt_set() {
        let pool = test_pool();
        insert_test_application(&pool, "app-parallel-receipt");
        reserve_submission_evidence_capacity(
            &pool,
            &capacity_input("app-parallel-receipt", "run-parallel-receipt", 1_000),
        )
        .unwrap();
        let winner = reserve_application_object_upload(
            &pool,
            "app-parallel-receipt",
            &submission_object_input(
                "app-parallel-receipt",
                "run-parallel-receipt",
                "receipt-winner",
                40,
                1_100,
            ),
        )
        .unwrap();
        let speculative = reserve_application_object_upload(
            &pool,
            "app-parallel-receipt",
            &submission_object_input(
                "app-parallel-receipt",
                "run-parallel-receipt",
                "receipt-speculative",
                40,
                1_101,
            ),
        )
        .unwrap();
        begin_upload_put(&pool, &winner.upload.id, 1_110).unwrap();
        begin_upload_put(&pool, &speculative.upload.id, 1_111).unwrap();

        let binding = ApplicationObjectBinding {
            upload_id: winner.upload.id.clone(),
            object_key: winner.upload.object_key.clone(),
            size_bytes: winner.upload.size_bytes,
            sha256: winner.upload.sha256.clone(),
            content_type: winner.upload.content_type.clone(),
        };
        let mut conn = pool.get().unwrap();
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        commit_application_object_uploads_sqlite_tx(
            &tx,
            "acct_1",
            "app-parallel-receipt",
            "run-parallel-receipt",
            "cloud",
            &[binding],
            1_200,
        )
        .expect("speculative capacity use must not invalidate the exact winning receipt set");
        tx.execute(
            "UPDATE jobs_applications SET state = 'submitted'
              WHERE account_id = 'acct_1' AND id = 'app-parallel-receipt'",
            [],
        )
        .unwrap();
        tx.commit().unwrap();
        drop(conn);

        let lifecycle: (String, String, String, String, String, i64, i64, i64) = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT winner.state, speculative.state, winner_put.state,
                        speculative_put.state, speculative_delete.state,
                        speculative_delete.next_attempt_at_ms,
                        capacity.consumed_bytes, capacity.consumed_objects
                   FROM object_uploads winner
                   JOIN object_uploads speculative ON speculative.id = ?2
                   JOIN object_storage_outbox winner_put
                     ON winner_put.upload_id = winner.id AND winner_put.operation = 'put'
                   JOIN object_storage_outbox speculative_put
                     ON speculative_put.upload_id = speculative.id
                    AND speculative_put.operation = 'put'
                   JOIN object_storage_outbox speculative_delete
                     ON speculative_delete.upload_id = speculative.id
                    AND speculative_delete.operation = 'delete'
                   JOIN jobs_submission_evidence_capacity capacity
                     ON capacity.account_id = winner.account_id
                    AND capacity.application_id = 'app-parallel-receipt'
                    AND capacity.run_id = 'run-parallel-receipt'
                  WHERE winner.id = ?1",
                params![winner.upload.id, speculative.upload.id],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                        row.get(6)?,
                        row.get(7)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(
            lifecycle,
            (
                "ready".into(),
                "delete_pending".into(),
                "completed".into(),
                "abandoned".into(),
                "pending".into(),
                1_200 + PROCESSING_LEASE_MS,
                40,
                1,
            )
        );

        let error = reserve_application_object_upload(
            &pool,
            "app-parallel-receipt",
            &submission_object_input(
                "app-parallel-receipt",
                "run-parallel-receipt",
                "receipt-after-publication",
                1,
                1_300,
            ),
        )
        .expect_err("Submitted must fence every later receipt-object reservation");
        assert_eq!(
            error.downcast_ref::<UploadControlError>(),
            Some(&UploadControlError::SubmissionEvidenceCapacityUnavailable)
        );
        assert!(
            artifact_upload(&pool, "acct_1", "receipt-after-publication")
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn safe_release_returns_unused_capacity_to_total_quota() {
        let pool = test_pool();
        insert_test_application(&pool, "app-release");
        reserve_submission_evidence_capacity(
            &pool,
            &capacity_input("app-release", "run-release", 1_000),
        )
        .unwrap();
        assert!(release_submission_evidence_capacity(
            &pool,
            "acct_1",
            "app-release",
            "run-release",
            1_100,
        )
        .unwrap());
        assert!(!release_submission_evidence_capacity(
            &pool,
            "acct_1",
            "app-release",
            "run-release",
            1_101,
        )
        .unwrap());
        reserve_upload(&pool, &artifact_input("after-capacity-release", 60, 1_200))
            .expect("released unused capacity no longer pins total quota");
    }

    #[test]
    fn capacity_expiry_updates_are_monotonic_or_explicitly_rebound_on_exact_authority() {
        let pool = test_pool();
        insert_test_application(&pool, "app-capacity-expiry");
        reserve_submission_evidence_capacity(
            &pool,
            &capacity_input("app-capacity-expiry", "run-capacity-expiry", 1_000),
        )
        .unwrap();

        let extended_expiry = DAY_MS.saturating_mul(2);
        let rebound_expiry = DAY_MS.saturating_add(5_000);
        let mut conn = pool.get().unwrap();
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        assert!(extend_submission_evidence_capacity_expiry_sqlite_tx(
            &tx,
            "acct_1",
            "app-capacity-expiry",
            "run-capacity-expiry",
            "cloud",
            extended_expiry,
            2_000,
        )
        .unwrap());
        assert!(extend_submission_evidence_capacity_expiry_sqlite_tx(
            &tx,
            "acct_1",
            "app-capacity-expiry",
            "run-capacity-expiry",
            "cloud",
            rebound_expiry,
            2_100,
        )
        .unwrap());
        let monotonic_expiry: i64 = tx
            .query_row(
                "SELECT expires_at_ms FROM jobs_submission_evidence_capacity
                  WHERE account_id = 'acct_1' AND application_id = 'app-capacity-expiry'
                    AND run_id = 'run-capacity-expiry'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(monotonic_expiry, extended_expiry);

        assert!(rebind_submission_evidence_capacity_expiry_sqlite_tx(
            &tx,
            "acct_1",
            "app-capacity-expiry",
            "run-capacity-expiry",
            "cloud",
            rebound_expiry,
            2_200,
        )
        .unwrap());
        assert!(!extend_submission_evidence_capacity_expiry_sqlite_tx(
            &tx,
            "acct_1",
            "app-capacity-expiry",
            "run-capacity-expiry",
            "local",
            extended_expiry,
            2_300,
        )
        .unwrap());
        let exact_expiry: i64 = tx
            .query_row(
                "SELECT expires_at_ms FROM jobs_submission_evidence_capacity
                  WHERE account_id = 'acct_1' AND application_id = 'app-capacity-expiry'
                    AND run_id = 'run-capacity-expiry'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(exact_expiry, rebound_expiry);
        tx.commit().unwrap();
        drop(conn);

        assert!(release_submission_evidence_capacity(
            &pool,
            "acct_1",
            "app-capacity-expiry",
            "run-capacity-expiry",
            2_400,
        )
        .unwrap());
        let mut conn = pool.get().unwrap();
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        assert!(!extend_submission_evidence_capacity_expiry_sqlite_tx(
            &tx,
            "acct_1",
            "app-capacity-expiry",
            "run-capacity-expiry",
            "cloud",
            extended_expiry,
            2_500,
        )
        .unwrap());
        tx.commit().unwrap();
    }

    #[test]
    #[serial_test::serial]
    fn postgres_protected_capacity_converts_and_restores_on_cleanup() {
        let Ok(database_url) = std::env::var("BLUEY_TEST_POSTGRES_URL") else {
            return;
        };
        let pool = open_postgres_pool(&database_url).expect("open Postgres test pool");
        run_migrations(&pool).expect("apply Postgres runtime migrations");
        let suffix = uuid::Uuid::new_v4().simple().to_string();
        let account_id = format!("acct_capacity_{suffix}");
        let application_id = format!("app_capacity_{suffix}");
        let job_id = format!("job_capacity_{suffix}");
        let run_id = format!("run_capacity_{suffix}");
        {
            let mut conn = pool.get_pg().expect("get Postgres setup connection");
            conn.execute(
                "INSERT INTO accounts (id, email, password_hash) VALUES ($1, $2, 'hash')",
                &[&account_id, &format!("{account_id}@example.test")],
            )
            .expect("insert Postgres capacity account");
            conn.execute(
                "INSERT INTO jobs_postings (
                    id, account_id, canonical_key, posting_json, source, company, title,
                    created_at_ms, updated_at_ms
                 ) VALUES ($1, $2, $3, '{}', 'test', 'Acme', 'Engineer', 1, 1)",
                &[&job_id, &account_id, &format!("canonical-{suffix}")],
            )
            .expect("insert Postgres capacity job");
            conn.execute(
                "INSERT INTO jobs_applications (
                    id, account_id, job_id, state, application_json,
                    created_at_ms, updated_at_ms
                 ) VALUES ($1, $2, $3, 'running', '{}', 1, 1)",
                &[&application_id, &account_id, &job_id],
            )
            .expect("insert Postgres capacity application");
        }

        let capacity = NewSubmissionEvidenceCapacity {
            account_id: account_id.clone(),
            application_id: application_id.clone(),
            run_id: run_id.clone(),
            runner: "cloud".into(),
            reserved_bytes: 100,
            reserved_objects: 3,
            expires_at_ms: DAY_MS + 1_000,
            now_ms: 1_000,
            limits: UploadLimits {
                max_object_bytes: 100,
                max_account_bytes: 150,
                max_daily_bytes: 10,
                max_account_objects: 10,
            },
        };
        reserve_submission_evidence_capacity(&pool, &capacity)
            .expect("reserve Postgres protected capacity");
        let mut object = submission_object_input(
            &application_id,
            &run_id,
            &format!("evidence-{suffix}"),
            40,
            1_100,
        );
        object.account_id = account_id.clone();
        object.object_key = object.object_key.replace("acct_1", &account_id);
        let reservation = reserve_application_object_upload(&pool, &application_id, &object)
            .expect("convert Postgres protected capacity");
        assert!(
            claim_cleanup_jobs(&pool, &account_id, StorageScope::Artifact, 2_000, 1_500, 10,)
                .expect("run account-scoped Postgres stale cleanup")
                .is_empty()
        );
        assert!(
            !claim_global_cleanup_jobs(&pool, StorageScope::Artifact, 2_100, 1_500, 10,)
                .expect("run global Postgres stale cleanup")
                .iter()
                .any(|job| job.upload_id == reservation.upload.id)
        );
        assert_eq!(
            artifact_upload(&pool, &account_id, &object.logical_id)
                .expect("load protected Postgres evidence")
                .expect("protected Postgres evidence remains ledgered")
                .state,
            "pending"
        );
        schedule_upload_cleanup(&pool, &reservation.upload.id, 1_200)
            .expect("schedule Postgres evidence cleanup");
        mark_cleanup_succeeded(&pool, &reservation.upload.id, 1_300)
            .expect("finish Postgres evidence cleanup");

        let mut conn = pool.get_pg().expect("get Postgres assertion connection");
        let capacity_row = conn
            .query_one(
                "SELECT consumed_bytes, consumed_objects, state
                   FROM jobs_submission_evidence_capacity
                  WHERE account_id = $1 AND application_id = $2 AND run_id = $3",
                &[&account_id, &application_id, &run_id],
            )
            .expect("load Postgres protected capacity");
        assert_eq!(capacity_row.get::<_, i64>(0), 0);
        assert_eq!(capacity_row.get::<_, i64>(1), 0);
        assert_eq!(capacity_row.get::<_, String>(2), "active");
        conn.execute("DELETE FROM accounts WHERE id = $1", &[&account_id])
            .expect("delete Postgres capacity test account");
    }

    #[test]
    #[serial_test::serial]
    fn postgres_cleanup_batch_is_bounded_and_malformed_metadata_is_safe() {
        let Ok(database_url) = std::env::var("BLUEY_TEST_POSTGRES_URL") else {
            return;
        };
        let pool = open_postgres_pool(&database_url).expect("open Postgres test pool");
        run_migrations(&pool).expect("apply Postgres runtime migrations");
        let suffix = uuid::Uuid::new_v4().simple().to_string();
        let account_id = format!("acct_cleanup_batch_{suffix}");
        let session_id = format!("session_cleanup_batch_{suffix}");
        {
            let mut conn = pool.get_pg().expect("get Postgres setup connection");
            conn.execute(
                "INSERT INTO accounts (id, email, password_hash) VALUES ($1, $2, 'hash')",
                &[&account_id, &format!("{account_id}@example.test")],
            )
            .expect("insert Postgres cleanup account");
            conn.execute(
                "INSERT INTO cloud_sessions (
                    account_id, session_id, title, status, created_at_ms, updated_at_ms,
                    last_active_at_ms, metadata_json
                 ) VALUES ($1, $2, 'Session', 'active', 1, 1, 1, '{}')",
                &[&account_id, &session_id],
            )
            .expect("insert Postgres cleanup session");
        }
        let reservations = (0..3)
            .map(|index| {
                reserve_upload(
                    &pool,
                    &postgres_audit_input(
                        &account_id,
                        &session_id,
                        &format!("{session_id}/bundle-{index}"),
                        1_000 + index,
                    ),
                )
                .expect("reserve Postgres cleanup candidate")
            })
            .collect::<Vec<_>>();
        pool.get_pg()
            .expect("get malformed metadata connection")
            .execute(
                "UPDATE object_uploads SET metadata_json = '{malformed' WHERE id = $1",
                &[&reservations[0].upload.id],
            )
            .expect("write malformed legacy metadata fixture");

        let jobs = claim_cleanup_jobs(&pool, &account_id, StorageScope::Audit, 10_000, 2_000, 1)
            .expect("malformed legacy metadata must not poison Postgres cleanup");
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].upload_id, reservations[0].upload.id);

        let mut conn = pool.get_pg().expect("get Postgres assertion connection");
        let counts = conn
            .query_one(
                "SELECT
                    (SELECT COUNT(*) FROM object_uploads
                      WHERE account_id = $1 AND state = 'delete_pending'),
                    (SELECT COUNT(*) FROM object_storage_outbox
                      WHERE account_id = $1 AND operation = 'delete'),
                    (SELECT COUNT(*) FROM object_storage_outbox
                      WHERE account_id = $1 AND operation = 'put' AND state = 'abandoned')",
                &[&account_id],
            )
            .expect("query bounded Postgres cleanup state");
        assert_eq!(counts.get::<_, i64>(0), 1);
        assert_eq!(counts.get::<_, i64>(1), 1);
        assert_eq!(counts.get::<_, i64>(2), 1);
        conn.execute("DELETE FROM accounts WHERE id = $1", &[&account_id])
            .expect("delete Postgres cleanup account");
    }

    #[test]
    #[serial_test::serial]
    fn postgres_account_object_lifecycle_is_idempotent_and_durable() {
        let Ok(database_url) = std::env::var("BLUEY_TEST_POSTGRES_URL") else {
            return;
        };
        let pool = open_postgres_pool(&database_url).expect("open Postgres test pool");
        run_migrations(&pool).expect("apply Postgres runtime migrations");
        let suffix = uuid::Uuid::new_v4().simple().to_string();
        let account_id = format!("acct_account_object_{suffix}");
        pool.get_pg()
            .expect("get Postgres setup connection")
            .execute(
                "INSERT INTO accounts (id, email, password_hash) VALUES ($1, $2, 'hash')",
                &[&account_id, &format!("{account_id}@example.test")],
            )
            .expect("insert Postgres account-object account");

        let mut input = account_object_input(
            &format!("resume-source-{suffix}"),
            "jobs_resume_source",
            40,
            1_000,
        );
        input.account_id = account_id.clone();
        input.object_key = input.object_key.replace("acct_1", &account_id);
        let reservation =
            reserve_account_object_upload(&pool, &input).expect("reserve Postgres account object");
        mark_upload_ready(&pool, &reservation.upload.id, 1_100)
            .expect("publish Postgres account object");
        let replay = reserve_account_object_upload(&pool, &input)
            .expect("replay exact Postgres account object");
        assert_eq!(replay.upload.id, reservation.upload.id);
        assert!(!replay.needs_put);

        let cleanup = schedule_account_object_cleanup(&pool, &account_id, &input.object_key, 1_200)
            .expect("schedule exact Postgres account object cleanup")
            .expect("Postgres account object exists");
        assert_eq!(cleanup.upload_id, reservation.upload.id);
        let mut conn = pool.get_pg().expect("get Postgres assertion connection");
        let lifecycle = conn
            .query_one(
                "SELECT upload.state, deletion.state
                   FROM object_uploads upload
                   JOIN object_storage_outbox deletion
                     ON deletion.upload_id = upload.id AND deletion.operation = 'delete'
                  WHERE upload.id = $1",
                &[&reservation.upload.id],
            )
            .expect("query Postgres account-object cleanup lifecycle");
        assert_eq!(lifecycle.get::<_, String>(0), "delete_pending");
        assert_eq!(lifecycle.get::<_, String>(1), "pending");
        conn.execute("DELETE FROM accounts WHERE id = $1", &[&account_id])
            .expect("delete Postgres account-object account");
    }

    #[test]
    #[serial_test::serial]
    fn postgres_global_cleanup_skips_locked_accounts_within_its_batch() {
        let Ok(database_url) = std::env::var("BLUEY_TEST_POSTGRES_URL") else {
            return;
        };
        let pool = open_postgres_pool(&database_url).expect("open Postgres test pool");
        run_migrations(&pool).expect("apply Postgres runtime migrations");
        let suffix = uuid::Uuid::new_v4().simple().to_string();
        let account_a = format!("acct_cleanup_a_{suffix}");
        let account_b = format!("acct_cleanup_b_{suffix}");
        let session_a = format!("session_cleanup_a_{suffix}");
        let session_b = format!("session_cleanup_b_{suffix}");
        {
            let mut conn = pool.get_pg().expect("get Postgres setup connection");
            for (account_id, session_id) in [(&account_a, &session_a), (&account_b, &session_b)] {
                conn.execute(
                    "INSERT INTO accounts (id, email, password_hash)
                     VALUES ($1, $2, 'hash')",
                    &[account_id, &format!("{account_id}@example.test")],
                )
                .expect("insert global cleanup account");
                conn.execute(
                    "INSERT INTO cloud_sessions (
                        account_id, session_id, title, status, created_at_ms, updated_at_ms,
                        last_active_at_ms, metadata_json
                     ) VALUES ($1, $2, 'Session', 'active', 1, 1, 1, '{}')",
                    &[account_id, session_id],
                )
                .expect("insert global cleanup session");
            }
        }
        let reservation_a = reserve_upload(
            &pool,
            &postgres_audit_input(
                &account_a,
                &session_a,
                &format!("{session_a}/bundle"),
                1_000,
            ),
        )
        .expect("reserve locked-account cleanup candidate");
        let reservation_b = reserve_upload(
            &pool,
            &postgres_audit_input(
                &account_b,
                &session_b,
                &format!("{session_b}/bundle"),
                1_001,
            ),
        )
        .expect("reserve unlocked-account cleanup candidate");

        let mut blocker = pool.get_pg().expect("get Postgres blocker connection");
        let mut blocker_tx = blocker.transaction().expect("begin account blocker");
        blocker_tx
            .query_one(
                "SELECT id FROM accounts WHERE id = $1 FOR UPDATE",
                &[&account_a],
            )
            .expect("lock first cleanup account");

        let cleanup_pool = pool.clone();
        let (result_tx, result_rx) = mpsc::channel();
        let cleanup_thread = std::thread::spawn(move || {
            let _ = result_tx.send(claim_global_cleanup_jobs(
                &cleanup_pool,
                StorageScope::Audit,
                10_000,
                2_000,
                1,
            ));
        });
        let cleanup_result = result_rx.recv_timeout(Duration::from_secs(5));
        blocker_tx.rollback().expect("release account blocker");
        cleanup_thread
            .join()
            .expect("global cleanup thread should not panic");
        let jobs = cleanup_result
            .expect("global cleanup must skip the locked account instead of blocking")
            .expect("global Postgres cleanup succeeds");
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].upload_id, reservation_b.upload.id);
        assert_ne!(jobs[0].upload_id, reservation_a.upload.id);

        let mut conn = pool.get_pg().expect("get Postgres cleanup connection");
        conn.execute(
            "DELETE FROM accounts WHERE id IN ($1, $2)",
            &[&account_a, &account_b],
        )
        .expect("delete global cleanup test accounts");
    }

    #[test]
    fn put_begin_refreshes_the_durable_processing_lease() {
        let pool = test_pool();
        let reservation = reserve_upload(&pool, &artifact_input("put-begin", 40, 1_000)).unwrap();

        let begun = begin_upload_put(&pool, &reservation.upload.id, 1_500).unwrap();
        assert_eq!(begun.state, "pending");
        assert_eq!(begun.updated_at_ms, 1_500);
        let durable: (String, i32, i64, Option<String>) = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT state, attempt_count, updated_at_ms, last_error
                   FROM object_storage_outbox
                  WHERE upload_id = ?1 AND operation = 'put'",
                params![reservation.upload.id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap();
        assert_eq!(durable, ("processing".into(), 1, 1_500, None));
    }

    #[test]
    fn deletion_intent_rejects_new_object_reservations() {
        let pool = test_pool();
        assert!(matches!(
            crate::db::account_data::begin_account_deletion(&pool, "acct_1", 1_000).unwrap(),
            Some(crate::db::account_data::BeginAccountDeletionResult::Ready(
                _
            ))
        ));

        let error = reserve_upload(&pool, &artifact_input("after-delete", 40, 1_100))
            .expect_err("a deleting account must not reserve another object");
        assert_eq!(
            error.downcast_ref::<UploadControlError>(),
            Some(&UploadControlError::AccountDeleting)
        );
    }

    #[test]
    fn deletion_intent_fences_put_begin_and_schedules_cleanup() {
        let pool = test_pool();
        let reservation = reserve_upload(&pool, &artifact_input("fenced-put", 40, 1_000)).unwrap();
        assert!(matches!(
            crate::db::account_data::begin_account_deletion(&pool, "acct_1", 1_100).unwrap(),
            Some(crate::db::account_data::BeginAccountDeletionResult::WaitingForUploads(_))
        ));

        let error = begin_upload_put(&pool, &reservation.upload.id, 1_200)
            .expect_err("a deletion intent must win before an object PUT begins");
        assert_eq!(
            error.downcast_ref::<UploadControlError>(),
            Some(&UploadControlError::AccountDeleting)
        );
        let lifecycle: (String, String, String) = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT u.state, put.state, deletion.state
                   FROM object_uploads u
                   JOIN object_storage_outbox put
                     ON put.upload_id = u.id AND put.operation = 'put'
                   JOIN object_storage_outbox deletion
                     ON deletion.upload_id = u.id AND deletion.operation = 'delete'
                  WHERE u.id = ?1",
                params![reservation.upload.id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(
            lifecycle,
            (
                "delete_pending".into(),
                "abandoned".into(),
                "pending".into()
            )
        );
    }

    #[test]
    fn deletion_intent_fences_ready_publication_after_put_begin() {
        let pool = test_pool();
        let reservation =
            reserve_upload(&pool, &artifact_input("fenced-ready", 40, 1_000)).unwrap();
        begin_upload_put(&pool, &reservation.upload.id, 1_100).unwrap();
        assert!(matches!(
            crate::db::account_data::begin_account_deletion(&pool, "acct_1", 1_200).unwrap(),
            Some(crate::db::account_data::BeginAccountDeletionResult::WaitingForUploads(_))
        ));

        let error = mark_upload_ready(&pool, &reservation.upload.id, 1_300)
            .expect_err("a PUT must not publish after account deletion starts");
        assert_eq!(
            error.downcast_ref::<UploadControlError>(),
            Some(&UploadControlError::AccountDeleting)
        );
        let upload = artifact_upload(&pool, "acct_1", "fenced-ready")
            .unwrap()
            .expect("delete-pending metadata remains until physical cleanup");
        assert_eq!(upload.state, "delete_pending");
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
