//! Account dashboard/export/delete read models.

use anyhow::{Context, Result};
use postgres::{Row as PgRow, Transaction as PgTransaction};
use rusqlite::{params, OptionalExtension, Transaction as SqliteTransaction, TransactionBehavior};
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    sync::{mpsc, Arc, LazyLock},
};
use tokio::sync::{
    oneshot, OwnedRwLockReadGuard, OwnedRwLockWriteGuard, OwnedSemaphorePermit, RwLock,
};

use crate::db::{jobs, DbPool, PostgresDbConn};

/// A pending object remains an active PUT/finalization risk for this long.
/// This matches the five-minute object-storage processing lease: deletion
/// fences new writers immediately, then waits for recent writers to drain or
/// become eligible for durable stale-upload cleanup.
pub const ACCOUNT_DELETION_FRESH_UPLOAD_WINDOW_MS: i64 = 5 * 60 * 1000;

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

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct AccountDeletionIntent {
    pub account_id: String,
    pub requested_at_ms: i64,
    pub last_checked_at_ms: i64,
    pub fresh_upload_cutoff_ms: i64,
    pub fresh_in_flight_puts: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BeginAccountDeletionResult {
    Ready(AccountDeletionIntent),
    WaitingForUploads(AccountDeletionIntent),
    WaitingForIrreversibleSubmissions { active_submissions: i64 },
    WaitingForIrreversibleCommunications { active_actions: i64 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BeginAccountDeletionWithWorkflowCleanupResult {
    pub deletion: BeginAccountDeletionResult,
    pub workflow_cleanup: Option<jobs::JobsWorkflowCleanupDeletionStatus>,
}

impl BeginAccountDeletionResult {
    pub fn intent(&self) -> Option<&AccountDeletionIntent> {
        match self {
            Self::Ready(intent) | Self::WaitingForUploads(intent) => Some(intent),
            Self::WaitingForIrreversibleSubmissions { .. }
            | Self::WaitingForIrreversibleCommunications { .. } => None,
        }
    }
}

// SQLite is a local/dev backend, so a process-wide reader/writer lock keeps its
// in-process async object I/O ordered with deletion. PostgreSQL uses a database
// advisory reader/writer lock so coordination spans every server replica and
// remains held across the physical object-store operation.
static SQLITE_ACCOUNT_OBJECT_LIFECYCLE_LOCK: LazyLock<Arc<RwLock<()>>> =
    LazyLock::new(|| Arc::new(RwLock::new(())));

#[must_use = "the account-object lifecycle guard must live across object-store I/O"]
pub enum AccountObjectLifecycleGuard {
    SqliteWriter(OwnedRwLockReadGuard<()>),
    SqliteDeletion(OwnedRwLockWriteGuard<()>),
    PostgresAdvisory(PostgresAccountObjectLifecycleGuard),
}

/// Releases a session-level PostgreSQL advisory lock on its dedicated blocking
/// coordination thread. The pooled connection never returns while still
/// carrying an advisory lock.
pub struct PostgresAccountObjectLifecycleGuard {
    release: Option<mpsc::Sender<()>>,
}

struct LockedPostgresAdvisorySession {
    connection: Option<PostgresDbConn>,
    lock_key: i64,
    shared: bool,
}

impl Drop for LockedPostgresAdvisorySession {
    fn drop(&mut self) {
        let Some(mut connection) = self.connection.take() else {
            return;
        };
        let unlock_sql = if self.shared {
            "SELECT pg_advisory_unlock_shared($1)"
        } else {
            "SELECT pg_advisory_unlock($1)"
        };
        let unlocked = connection
            .query_one(unlock_sql, &[&self.lock_key])
            .and_then(|row| row.try_get::<_, bool>(0))
            .unwrap_or(false);
        if !unlocked {
            tracing::error!(
                shared = self.shared,
                "failed to release PostgreSQL account object lifecycle lock; discarding session"
            );
            connection.mark_broken();
        }
    }
}

impl Drop for PostgresAccountObjectLifecycleGuard {
    fn drop(&mut self) {
        if let Some(release) = self.release.take() {
            let _ = release.send(());
        }
    }
}

/// Acquire object-writer coordination before reserving or beginning a physical
/// account-scoped PUT. Callers retain the returned guard through terminal ledger
/// state and every awaited object-store operation.
pub async fn acquire_account_object_writer(
    pool: &DbPool,
    account_id: &str,
) -> Result<AccountObjectLifecycleGuard> {
    acquire_account_object_lifecycle_guard(pool, account_id, true).await
}

/// Enter account-object deletion coordination. The exclusive guard remains live
/// through prefix purge and hard delete, preventing a previously paused PUT or
/// another server replica from recreating account bytes after the final sweep.
pub async fn acquire_account_object_deletion(
    pool: &DbPool,
    account_id: &str,
) -> Result<AccountObjectLifecycleGuard> {
    acquire_account_object_lifecycle_guard(pool, account_id, false).await
}

async fn acquire_account_object_lifecycle_guard(
    pool: &DbPool,
    account_id: &str,
    shared: bool,
) -> Result<AccountObjectLifecycleGuard> {
    anyhow::ensure!(!account_id.trim().is_empty(), "account_id is required");
    match pool {
        DbPool::Sqlite(_) if shared => Ok(AccountObjectLifecycleGuard::SqliteWriter(
            SQLITE_ACCOUNT_OBJECT_LIFECYCLE_LOCK
                .clone()
                .read_owned()
                .await,
        )),
        DbPool::Sqlite(_) => Ok(AccountObjectLifecycleGuard::SqliteDeletion(
            SQLITE_ACCOUNT_OBJECT_LIFECYCLE_LOCK
                .clone()
                .write_owned()
                .await,
        )),
        DbPool::Postgres(pools) => {
            let lifecycle_slot = pools
                .lifecycle_slots()
                .acquire_owned()
                .await
                .context("wait for PostgreSQL lifecycle lock capacity")?;
            acquire_postgres_account_object_guard(
                pools.lifecycle_pool(),
                lifecycle_slot,
                account_id,
                shared,
            )
            .await
            .map(AccountObjectLifecycleGuard::PostgresAdvisory)
        }
    }
}

async fn acquire_postgres_account_object_guard(
    pool: crate::db::PostgresDbPool,
    lifecycle_slot: OwnedSemaphorePermit,
    account_id: &str,
    shared: bool,
) -> Result<PostgresAccountObjectLifecycleGuard> {
    let lock_key = postgres_account_object_lock_key(account_id);
    let (acquired_tx, acquired_rx) = oneshot::channel::<std::result::Result<(), String>>();
    let (release_tx, release_rx) = mpsc::channel::<()>();
    std::thread::Builder::new()
        .name("bluey-account-object-lock".to_string())
        .spawn(move || {
            let lock_sql = if shared {
                "SELECT pg_advisory_lock_shared($1)"
            } else {
                "SELECT pg_advisory_lock($1)"
            };
            let _lifecycle_slot = lifecycle_slot;
            let mut connection = match pool.get().context("get PostgreSQL lifecycle lock") {
                Ok(connection) => connection,
                Err(error) => {
                    let _ = acquired_tx.send(Err(format!("{error:#}")));
                    return;
                }
            };
            if let Err(error) = connection
                .query_one(lock_sql, &[&lock_key])
                .context("acquire PostgreSQL lifecycle lock")
            {
                let _ = acquired_tx.send(Err(format!("{error:#}")));
                return;
            }
            let locked_session = LockedPostgresAdvisorySession {
                connection: Some(connection),
                lock_key,
                shared,
            };
            if acquired_tx.send(Ok(())).is_err() {
                return;
            }
            let _ = release_rx.recv();
            drop(locked_session);
        })
        .context("spawn PostgreSQL lifecycle lock thread")?;
    acquired_rx
        .await
        .context("PostgreSQL lifecycle lock thread stopped")?
        .map_err(anyhow::Error::msg)?;
    Ok(PostgresAccountObjectLifecycleGuard {
        release: Some(release_tx),
    })
}

fn postgres_account_object_lock_key(account_id: &str) -> i64 {
    let digest = Sha256::digest(
        [
            b"bluey-account-object-lifecycle\0".as_slice(),
            account_id.as_bytes(),
        ]
        .concat(),
    );
    i64::from_be_bytes(
        digest[..8]
            .try_into()
            .expect("SHA-256 prefix is eight bytes"),
    )
}

/// Transaction-scoped account write authority used by reservation and
/// finalization paths. PostgreSQL checks take the account row `FOR UPDATE`;
/// SQLite callers must already hold an `IMMEDIATE` transaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AccountWriteFence {
    Active,
    DeletionRequested,
    Missing,
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
        value.jobs = jobs::account_export(pool, account_id, &value.account.email)?;
    }
    Ok(bundle)
}

/// Remove a newly-created account whose setup failed before any deletion
/// workflow or runner-volume authority was established.
///
/// This deliberately refuses accounts with a deletion intent or purge request;
/// normal account deletion must use `hard_delete_account_after_runner_purge`.
pub(crate) fn hard_delete_account_after_setup_failure(
    pool: &DbPool,
    account_id: &str,
) -> Result<bool> {
    anyhow::ensure!(!account_id.trim().is_empty(), "account_id is required");
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => hard_delete_account_after_setup_failure_sqlite(pool, account_id),
        DbPool::Postgres(_) => hard_delete_account_after_setup_failure_postgres(pool, account_id),
    })
}

/// Hard-delete a fenced account only after the exact runner-volume purge,
/// workflow-cleanup tombstone, and authorized object sweep are durable.
///
/// The caller must hold the account's exclusive object-lifecycle guard across
/// its final object-store sweep and this transaction. This transaction
/// independently reasserts the durable fence, drained uploads, absence of an
/// unresolved irreversible submission, the exact completed runner purge, the
/// current workflow-cleanup proof, and the completed authorized object sweep
/// so no alternate call site can bypass an erasure gate.
pub(crate) fn hard_delete_account_after_runner_purge(
    pool: &DbPool,
    account_id: &str,
    requested_at_ms: i64,
    purge_request_id: &str,
    sweep_attempt_id: &str,
    workflow_cleanup_proof: &jobs::JobsWorkflowCleanupDeletionProof,
) -> Result<bool> {
    anyhow::ensure!(!account_id.trim().is_empty(), "account_id is required");
    anyhow::ensure!(requested_at_ms >= 0, "requested_at_ms must be non-negative");
    anyhow::ensure!(
        !purge_request_id.trim().is_empty(),
        "purge_request_id is required"
    );
    anyhow::ensure!(
        !sweep_attempt_id.trim().is_empty(),
        "sweep_attempt_id is required"
    );
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => hard_delete_account_after_runner_purge_sqlite(
            pool,
            account_id,
            requested_at_ms,
            purge_request_id,
            sweep_attempt_id,
            workflow_cleanup_proof,
        ),
        DbPool::Postgres(_) => hard_delete_account_after_runner_purge_postgres(
            pool,
            account_id,
            requested_at_ms,
            purge_request_id,
            sweep_attempt_id,
            workflow_cleanup_proof,
        ),
    })
}

/// Persist the account-deletion write fence and report whether recent pending
/// object PUTs still need to drain. A `WaitingForUploads` result is durable:
/// callers must stop deletion, while all subsequent fenced writers fail
/// closed until hard deletion removes the account and intent by cascade.
pub fn begin_account_deletion(
    pool: &DbPool,
    account_id: &str,
    now_ms: i64,
) -> Result<Option<BeginAccountDeletionResult>> {
    anyhow::ensure!(!account_id.trim().is_empty(), "account_id is required");
    anyhow::ensure!(now_ms >= 0, "now_ms must be non-negative");
    crate::db::run_blocking_db(|| {
        let result = match pool {
            DbPool::Sqlite(_) => begin_account_deletion_sqlite(pool, account_id, now_ms, None)?,
            DbPool::Postgres(_) => begin_account_deletion_postgres(pool, account_id, now_ms, None)?,
        };
        Ok(result.map(|result| result.deletion))
    })
}

/// Atomically establish the deletion intent and freeze its exact workflow
/// cleanup generation against a preexisting global legacy authority.
///
/// Irreversible submission/communication blockers return without creating an
/// account deletion intent or workflow-cleanup binding. A missing or drifted
/// global authority rolls the whole transaction back.
pub fn begin_account_deletion_with_workflow_cleanup(
    pool: &DbPool,
    account_id: &str,
    now_ms: i64,
    legacy_authority: &jobs::JobsLegacyInventoryAuthorityRef,
) -> Result<Option<BeginAccountDeletionWithWorkflowCleanupResult>> {
    anyhow::ensure!(!account_id.trim().is_empty(), "account_id is required");
    anyhow::ensure!(now_ms >= 0, "now_ms must be non-negative");
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            begin_account_deletion_sqlite(pool, account_id, now_ms, Some(legacy_authority))
        }
        DbPool::Postgres(_) => {
            begin_account_deletion_postgres(pool, account_id, now_ms, Some(legacy_authority))
        }
    })
}

/// Read the durable deletion fence without acquiring write authority.
pub fn account_deletion_intent(
    pool: &DbPool,
    account_id: &str,
) -> Result<Option<AccountDeletionIntent>> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => account_deletion_intent_sqlite(pool, account_id),
        DbPool::Postgres(_) => account_deletion_intent_postgres(pool, account_id),
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
        if let Some(receipt_object) = receipt
            .get("receiptObject")
            .and_then(serde_json::Value::as_object)
        {
            let key = receipt_object
                .get("storageKey")
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .unwrap_or_default();
            if !key.is_empty() && seen.insert(key.to_string()) {
                refs.push(ArtifactObjectRef {
                    artifact_id: format!("{}:receipt", application.id),
                    title: "Application receipt bundle".to_string(),
                    object_key: key.to_string(),
                    content_type: receipt_object
                        .get("mediaType")
                        .and_then(serde_json::Value::as_str)
                        .map(ToString::to_string)
                        .or_else(|| Some("application/json".to_string())),
                    size_bytes: receipt_object
                        .get("sizeBytes")
                        .and_then(serde_json::Value::as_i64),
                    sha256: receipt_object
                        .get("sha256")
                        .and_then(serde_json::Value::as_str)
                        .map(ToString::to_string),
                    expires_at_ms: None,
                });
            }
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
    for reference in browser_profile_snapshot_object_refs(pool, account_id)? {
        if seen.insert(reference.object_key.clone()) {
            refs.push(reference);
        }
    }
    Ok(refs)
}

fn browser_profile_snapshot_object_refs(
    pool: &DbPool,
    account_id: &str,
) -> Result<Vec<ArtifactObjectRef>> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get()?;
            let mut stmt = conn.prepare(
                "SELECT browser_profile_id, generation, object_key, size_bytes, sha256
                   FROM jobs_browser_profile_snapshots WHERE account_id = ?1",
            )?;
            let rows = stmt.query_map(params![account_id], |row| {
                let browser_profile_id: String = row.get(0)?;
                let generation: i64 = row.get(1)?;
                Ok(ArtifactObjectRef {
                    artifact_id: format!(
                        "browser-profile-snapshot:{browser_profile_id}:{generation}"
                    ),
                    title: "Encrypted Bluey Browser profile snapshot".to_string(),
                    object_key: row.get(2)?,
                    content_type: Some(
                        "application/vnd.bluey.browser-profile+encrypted".to_string(),
                    ),
                    size_bytes: Some(row.get(3)?),
                    sha256: Some(row.get(4)?),
                    expires_at_ms: None,
                })
            })?;
            rows.collect::<rusqlite::Result<Vec<_>>>()
                .map_err(Into::into)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            conn.query(
                "SELECT browser_profile_id, generation, object_key, size_bytes, sha256
                   FROM jobs_browser_profile_snapshots WHERE account_id = $1",
                &[&account_id],
            )?
            .into_iter()
            .map(|row| {
                let browser_profile_id: String = row.try_get(0)?;
                let generation: i64 = row.try_get(1)?;
                Ok(ArtifactObjectRef {
                    artifact_id: format!(
                        "browser-profile-snapshot:{browser_profile_id}:{generation}"
                    ),
                    title: "Encrypted Bluey Browser profile snapshot".to_string(),
                    object_key: row.try_get(2)?,
                    content_type: Some(
                        "application/vnd.bluey.browser-profile+encrypted".to_string(),
                    ),
                    size_bytes: Some(row.try_get(3)?),
                    sha256: Some(row.try_get(4)?),
                    expires_at_ms: None,
                })
            })
            .collect::<Result<Vec<_>>>()
        }
    })
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
        jobs: None,
        exported_at: chrono::Utc::now().to_rfc3339(),
    }))
}

fn begin_account_deletion_sqlite(
    pool: &DbPool,
    account_id: &str,
    now_ms: i64,
    legacy_authority: Option<&jobs::JobsLegacyInventoryAuthorityRef>,
) -> Result<Option<BeginAccountDeletionWithWorkflowCleanupResult>> {
    let mut conn = pool
        .get()
        .context("get sqlite account-deletion connection")?;
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .context("begin sqlite account-deletion fence")?;
    let account_exists = tx
        .query_row(
            "SELECT 1 FROM accounts WHERE id = ?1",
            params![account_id],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    if !account_exists {
        return Ok(None);
    }

    tx.execute(
        "INSERT OR IGNORE INTO jobs_communication_write_fences (
            account_id, connection_id, reason, created_at_ms
         ) VALUES (?1, '', 'account_deletion', ?2)",
        params![account_id, now_ms],
    )?;

    let active_submissions: i64 = tx.query_row(
        "SELECT COUNT(*) FROM jobs_applications AS application
          WHERE application.account_id = ?1 AND application.state <> 'submitted'
            AND (
              EXISTS (
                SELECT 1 FROM jobs_execution_leases AS lease
                 WHERE lease.account_id = application.account_id
                   AND lease.application_id = application.id
                   AND lease.phase IN ('click_started', 'submitted', 'side_effect_unknown')
              )
              OR EXISTS (
                SELECT 1 FROM jobs_local_run_tickets AS ticket
                 WHERE ticket.account_id = application.account_id
                   AND ticket.application_id = application.id
                   AND ticket.status IN ('click_started', 'side_effect_unknown')
              )
            )",
        params![account_id],
        |row| row.get(0),
    )?;
    if active_submissions > 0 {
        tx.commit()?;
        return Ok(Some(BeginAccountDeletionWithWorkflowCleanupResult {
            deletion: BeginAccountDeletionResult::WaitingForIrreversibleSubmissions {
                active_submissions,
            },
            workflow_cleanup: None,
        }));
    }

    let active_actions: i64 = tx.query_row(
        "SELECT COUNT(*) FROM jobs_communication_actions
          WHERE account_id = ?1 AND status IN ('dispatching', 'side_effect_unknown')",
        params![account_id],
        |row| row.get(0),
    )?;
    if active_actions > 0 {
        tx.commit()?;
        return Ok(Some(BeginAccountDeletionWithWorkflowCleanupResult {
            deletion: BeginAccountDeletionResult::WaitingForIrreversibleCommunications {
                active_actions,
            },
            workflow_cleanup: None,
        }));
    }

    let cutoff_ms = now_ms.saturating_sub(ACCOUNT_DELETION_FRESH_UPLOAD_WINDOW_MS);
    tx.execute(
        "INSERT OR IGNORE INTO account_deletion_intents (
            account_id, requested_at_ms, last_checked_at_ms,
            fresh_upload_cutoff_ms, fresh_in_flight_puts
         ) VALUES (?1, ?2, ?2, ?3, 0)",
        params![account_id, now_ms, cutoff_ms],
    )?;
    let fresh_in_flight_puts: i64 = tx.query_row(
        "SELECT COUNT(*) FROM object_uploads
          WHERE account_id = ?1 AND state = 'pending' AND updated_at_ms > ?2",
        params![account_id, cutoff_ms],
        |row| row.get(0),
    )?;
    tx.execute(
        "UPDATE account_deletion_intents
            SET last_checked_at_ms = ?2, fresh_upload_cutoff_ms = ?3,
                fresh_in_flight_puts = ?4
          WHERE account_id = ?1",
        params![account_id, now_ms, cutoff_ms, fresh_in_flight_puts],
    )?;
    let intent = tx.query_row(
        "SELECT account_id, requested_at_ms, last_checked_at_ms,
                fresh_upload_cutoff_ms, fresh_in_flight_puts
           FROM account_deletion_intents WHERE account_id = ?1",
        params![account_id],
        account_deletion_intent_from_sqlite,
    )?;
    let workflow_cleanup = legacy_authority
        .map(|legacy_authority| {
            jobs::freeze_jobs_workflow_cleanup_for_deletion_sqlite_tx(
                &tx,
                account_id,
                intent.requested_at_ms,
                legacy_authority,
                now_ms,
            )
        })
        .transpose()?;
    tx.commit()?;
    Ok(Some(BeginAccountDeletionWithWorkflowCleanupResult {
        deletion: classify_account_deletion_intent(intent),
        workflow_cleanup,
    }))
}

fn begin_account_deletion_postgres(
    pool: &DbPool,
    account_id: &str,
    now_ms: i64,
    legacy_authority: Option<&jobs::JobsLegacyInventoryAuthorityRef>,
) -> Result<Option<BeginAccountDeletionWithWorkflowCleanupResult>> {
    let mut conn = pool
        .get_pg()
        .context("get postgres account-deletion connection")?;
    let mut tx = conn
        .transaction()
        .context("begin postgres account-deletion fence")?;
    let account_exists = tx
        .query_opt(
            "SELECT id FROM accounts WHERE id = $1 FOR UPDATE",
            &[&account_id],
        )?
        .is_some();
    if !account_exists {
        return Ok(None);
    }

    tx.execute(
        "INSERT INTO jobs_communication_write_fences (
            account_id, connection_id, reason, created_at_ms
         ) VALUES ($1, '', 'account_deletion', $2)
         ON CONFLICT(account_id, connection_id) DO NOTHING",
        &[&account_id, &now_ms],
    )?;

    let active_submissions: i64 = tx
        .query_one(
            "SELECT COUNT(*)::bigint FROM jobs_applications AS application
              WHERE application.account_id = $1 AND application.state <> 'submitted'
                AND (
                  EXISTS (
                    SELECT 1 FROM jobs_execution_leases AS lease
                     WHERE lease.account_id = application.account_id
                       AND lease.application_id = application.id
                       AND lease.phase IN ('click_started', 'submitted', 'side_effect_unknown')
                  )
                  OR EXISTS (
                    SELECT 1 FROM jobs_local_run_tickets AS ticket
                     WHERE ticket.account_id = application.account_id
                       AND ticket.application_id = application.id
                       AND ticket.status IN ('click_started', 'side_effect_unknown')
                  )
                )",
            &[&account_id],
        )?
        .try_get(0)?;
    if active_submissions > 0 {
        tx.commit()?;
        return Ok(Some(BeginAccountDeletionWithWorkflowCleanupResult {
            deletion: BeginAccountDeletionResult::WaitingForIrreversibleSubmissions {
                active_submissions,
            },
            workflow_cleanup: None,
        }));
    }

    let active_actions: i64 = tx
        .query_one(
            "SELECT COUNT(*)::bigint FROM jobs_communication_actions
              WHERE account_id = $1 AND status IN ('dispatching', 'side_effect_unknown')",
            &[&account_id],
        )?
        .get(0);
    if active_actions > 0 {
        tx.commit()?;
        return Ok(Some(BeginAccountDeletionWithWorkflowCleanupResult {
            deletion: BeginAccountDeletionResult::WaitingForIrreversibleCommunications {
                active_actions,
            },
            workflow_cleanup: None,
        }));
    }

    let cutoff_ms = now_ms.saturating_sub(ACCOUNT_DELETION_FRESH_UPLOAD_WINDOW_MS);
    tx.execute(
        "INSERT INTO account_deletion_intents (
            account_id, requested_at_ms, last_checked_at_ms,
            fresh_upload_cutoff_ms, fresh_in_flight_puts
         ) VALUES ($1, $2, $2, $3, 0)
         ON CONFLICT(account_id) DO NOTHING",
        &[&account_id, &now_ms, &cutoff_ms],
    )?;
    let fresh_in_flight_puts: i64 = tx
        .query_one(
            "SELECT COUNT(*)::bigint FROM object_uploads
              WHERE account_id = $1 AND state = 'pending' AND updated_at_ms > $2",
            &[&account_id, &cutoff_ms],
        )?
        .try_get(0)?;
    tx.execute(
        "UPDATE account_deletion_intents
            SET last_checked_at_ms = $2, fresh_upload_cutoff_ms = $3,
                fresh_in_flight_puts = $4
          WHERE account_id = $1",
        &[&account_id, &now_ms, &cutoff_ms, &fresh_in_flight_puts],
    )?;
    let row = tx.query_one(
        "SELECT account_id, requested_at_ms, last_checked_at_ms,
                fresh_upload_cutoff_ms, fresh_in_flight_puts
           FROM account_deletion_intents WHERE account_id = $1",
        &[&account_id],
    )?;
    let intent = account_deletion_intent_from_pg(&row)?;
    let workflow_cleanup = legacy_authority
        .map(|legacy_authority| {
            jobs::freeze_jobs_workflow_cleanup_for_deletion_postgres_tx(
                &mut tx,
                account_id,
                intent.requested_at_ms,
                legacy_authority,
                now_ms,
            )
        })
        .transpose()?;
    tx.commit()?;
    Ok(Some(BeginAccountDeletionWithWorkflowCleanupResult {
        deletion: classify_account_deletion_intent(intent),
        workflow_cleanup,
    }))
}

fn account_deletion_intent_sqlite(
    pool: &DbPool,
    account_id: &str,
) -> Result<Option<AccountDeletionIntent>> {
    let conn = pool
        .get()
        .context("get sqlite account-deletion read connection")?;
    Ok(conn
        .query_row(
            "SELECT account_id, requested_at_ms, last_checked_at_ms,
                    fresh_upload_cutoff_ms, fresh_in_flight_puts
               FROM account_deletion_intents WHERE account_id = ?1",
            params![account_id],
            account_deletion_intent_from_sqlite,
        )
        .optional()?)
}

fn account_deletion_intent_postgres(
    pool: &DbPool,
    account_id: &str,
) -> Result<Option<AccountDeletionIntent>> {
    let mut conn = pool
        .get_pg()
        .context("get postgres account-deletion read connection")?;
    conn.query_opt(
        "SELECT account_id, requested_at_ms, last_checked_at_ms,
                fresh_upload_cutoff_ms, fresh_in_flight_puts
           FROM account_deletion_intents WHERE account_id = $1",
        &[&account_id],
    )?
    .map(|row| account_deletion_intent_from_pg(&row))
    .transpose()
}

fn classify_account_deletion_intent(intent: AccountDeletionIntent) -> BeginAccountDeletionResult {
    if intent.fresh_in_flight_puts == 0 {
        BeginAccountDeletionResult::Ready(intent)
    } else {
        BeginAccountDeletionResult::WaitingForUploads(intent)
    }
}

fn account_deletion_intent_from_sqlite(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<AccountDeletionIntent> {
    Ok(AccountDeletionIntent {
        account_id: row.get(0)?,
        requested_at_ms: row.get(1)?,
        last_checked_at_ms: row.get(2)?,
        fresh_upload_cutoff_ms: row.get(3)?,
        fresh_in_flight_puts: row.get(4)?,
    })
}

fn account_deletion_intent_from_pg(row: &PgRow) -> Result<AccountDeletionIntent> {
    Ok(AccountDeletionIntent {
        account_id: row.try_get(0)?,
        requested_at_ms: row.try_get(1)?,
        last_checked_at_ms: row.try_get(2)?,
        fresh_upload_cutoff_ms: row.try_get(3)?,
        fresh_in_flight_puts: row.try_get(4)?,
    })
}

pub(crate) fn account_write_fence_sqlite_tx(
    tx: &SqliteTransaction<'_>,
    account_id: &str,
) -> Result<AccountWriteFence> {
    let account_exists = tx
        .query_row(
            "SELECT 1 FROM accounts WHERE id = ?1",
            params![account_id],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    if !account_exists {
        return Ok(AccountWriteFence::Missing);
    }
    let deletion_requested = tx
        .query_row(
            "SELECT 1 FROM account_deletion_intents WHERE account_id = ?1",
            params![account_id],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    Ok(if deletion_requested {
        AccountWriteFence::DeletionRequested
    } else {
        AccountWriteFence::Active
    })
}

pub(crate) fn account_write_fence_postgres_tx(
    tx: &mut PgTransaction<'_>,
    account_id: &str,
) -> Result<AccountWriteFence> {
    let account_exists = tx
        .query_opt(
            "SELECT id FROM accounts WHERE id = $1 FOR UPDATE",
            &[&account_id],
        )?
        .is_some();
    if !account_exists {
        return Ok(AccountWriteFence::Missing);
    }
    let deletion_requested = tx
        .query_opt(
            "SELECT 1 FROM account_deletion_intents WHERE account_id = $1",
            &[&account_id],
        )?
        .is_some();
    Ok(if deletion_requested {
        AccountWriteFence::DeletionRequested
    } else {
        AccountWriteFence::Active
    })
}

fn hard_delete_account_after_setup_failure_sqlite(pool: &DbPool, account_id: &str) -> Result<bool> {
    let mut conn = pool.get()?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let deletion_authority_exists: bool = tx.query_row(
        "SELECT EXISTS(
            SELECT 1 FROM account_deletion_intents WHERE account_id = ?1
            UNION ALL
            SELECT 1 FROM jobs_runner_purge_requests WHERE account_id = ?1
         )",
        params![account_id],
        |row| row.get(0),
    )?;
    anyhow::ensure!(
        !deletion_authority_exists,
        "setup-failure cleanup refuses an account with durable deletion authority"
    );
    tx.execute(
        "DELETE FROM stripe_webhook_events
            WHERE json_extract(body, '$.data.object.client_reference_id') = ?1
               OR json_extract(body, '$.data.object.metadata.bluey_account_id') = ?1",
        params![account_id],
    )?;
    let deleted = tx.execute("DELETE FROM accounts WHERE id = ?1", params![account_id])?;
    tx.execute(
        DELETE_WORKFLOW_CLEANUP_CASCADE_TOKEN_SQLITE,
        params![account_id],
    )?;
    tx.commit()?;
    Ok(deleted > 0)
}

fn hard_delete_account_after_setup_failure_postgres(
    pool: &DbPool,
    account_id: &str,
) -> Result<bool> {
    let mut conn = pool.get_pg()?;
    let mut tx = conn.transaction()?;
    let deletion_authority_exists: bool = tx
        .query_one(
            "SELECT EXISTS(
                SELECT 1 FROM account_deletion_intents WHERE account_id = $1
                UNION ALL
                SELECT 1 FROM jobs_runner_purge_requests WHERE account_id = $1
             )",
            &[&account_id],
        )?
        .get(0);
    anyhow::ensure!(
        !deletion_authority_exists,
        "setup-failure cleanup refuses an account with durable deletion authority"
    );
    tx.execute(
        "DELETE FROM stripe_webhook_events
            WHERE body::jsonb #>> '{data,object,client_reference_id}' = $1
               OR body::jsonb #>> '{data,object,metadata,bluey_account_id}' = $1",
        &[&account_id],
    )
    .ok();
    let deleted = tx.execute("DELETE FROM accounts WHERE id = $1", &[&account_id])?;
    tx.execute(
        DELETE_WORKFLOW_CLEANUP_CASCADE_TOKEN_POSTGRES,
        &[&account_id],
    )?;
    tx.commit()?;
    Ok(deleted > 0)
}

// The account BEFORE DELETE guard creates this token for child delete guards.
// Remove it only after the parent DELETE statement returns, when every FK
// cascade has completed, and before the deferred token FK is checked at commit.
const DELETE_WORKFLOW_CLEANUP_CASCADE_TOKEN_SQLITE: &str =
    "DELETE FROM jobs_workflow_cleanup_hard_delete_cascade_tokens WHERE account_id = ?1";
const DELETE_WORKFLOW_CLEANUP_CASCADE_TOKEN_POSTGRES: &str =
    "DELETE FROM jobs_workflow_cleanup_hard_delete_cascade_tokens WHERE account_id = $1";

fn hard_delete_account_after_runner_purge_sqlite(
    pool: &DbPool,
    account_id: &str,
    requested_at_ms: i64,
    purge_request_id: &str,
    sweep_attempt_id: &str,
    workflow_cleanup_proof: &jobs::JobsWorkflowCleanupDeletionProof,
) -> Result<bool> {
    let mut conn = pool.get()?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let account_exists = tx
        .query_row(
            "SELECT 1 FROM accounts WHERE id = ?1",
            params![account_id],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    if !account_exists {
        return Ok(false);
    }

    let intent = tx
        .query_row(
            "SELECT fresh_upload_cutoff_ms, fresh_in_flight_puts
               FROM account_deletion_intents WHERE account_id = ?1",
            params![account_id],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
        )
        .optional()?;
    let Some((fresh_upload_cutoff_ms, recorded_fresh_puts)) = intent else {
        anyhow::bail!("runner-purge hard delete requires a durable deletion fence");
    };
    let live_fresh_puts: i64 = tx.query_row(
        "SELECT COUNT(*) FROM object_uploads
          WHERE account_id = ?1 AND state = 'pending' AND updated_at_ms > ?2",
        params![account_id, fresh_upload_cutoff_ms],
        |row| row.get(0),
    )?;
    anyhow::ensure!(
        recorded_fresh_puts == 0 && live_fresh_puts == 0,
        "runner-purge hard delete refuses active object uploads"
    );
    let irreversible_count: i64 = tx.query_row(
        "SELECT COUNT(*) FROM jobs_applications AS application
          WHERE application.account_id = ?1 AND application.state <> 'submitted'
            AND (
              EXISTS (
                SELECT 1 FROM jobs_execution_leases AS lease
                 WHERE lease.account_id = application.account_id
                   AND lease.application_id = application.id
                   AND lease.phase IN ('click_started', 'submitted', 'side_effect_unknown')
              )
              OR EXISTS (
                SELECT 1 FROM jobs_local_run_tickets AS ticket
                 WHERE ticket.account_id = application.account_id
                   AND ticket.application_id = application.id
                   AND ticket.status IN ('click_started', 'side_effect_unknown')
              )
            )",
        params![account_id],
        |row| row.get(0),
    )?;
    anyhow::ensure!(
        irreversible_count == 0,
        "runner-purge hard delete refuses unresolved irreversible submissions"
    );
    let irreversible_communication_count: i64 = tx.query_row(
        "SELECT COUNT(*) FROM jobs_communication_actions
          WHERE account_id = ?1 AND status IN ('dispatching', 'side_effect_unknown')",
        params![account_id],
        |row| row.get(0),
    )?;
    anyhow::ensure!(
        irreversible_communication_count == 0,
        "runner-purge hard delete refuses unresolved irreversible communications"
    );
    let purge_is_complete: bool = tx.query_row(
        "SELECT EXISTS(
            SELECT 1
              FROM jobs_runner_purge_requests AS request
              JOIN jobs_runner_purge_tombstones AS tombstone
                ON tombstone.request_id = request.request_id
               AND tombstone.purge_generation = request.purge_generation
               AND tombstone.purge_subject = request.purge_subject
               AND tombstone.target_set_sha256 = request.target_set_sha256
               AND tombstone.required_target_count = request.required_target_count
              JOIN jobs_runner_volume_fleet_state AS fleet
                ON fleet.singleton_id = 1
               AND fleet.legacy_inventory_state = 'ready'
               AND fleet.legacy_inventory_generation = request.legacy_inventory_generation
               AND fleet.legacy_inventory_reconciliation_id =
                   request.legacy_inventory_reconciliation_id
               AND fleet.legacy_inventory_authority_id = request.legacy_inventory_authority_id
               AND fleet.legacy_inventory_authority_sha256 =
                   request.legacy_inventory_authority_sha256
             WHERE request.request_id = ?1 AND request.account_id = ?2
               AND request.state = 'complete'
               AND request.legacy_unresolved_count = 0
               AND request.resolved_target_count = request.required_target_count
               AND request.completed_at_ms IS NOT NULL
               AND tombstone.completed_at_ms = request.completed_at_ms
         )",
        params![purge_request_id, account_id],
        |row| row.get(0),
    )?;
    anyhow::ensure!(
        purge_is_complete,
        "runner-purge hard delete requires the exact completed purge tombstone"
    );
    jobs::require_account_deletion_workflow_cleanup_complete_sqlite_tx(
        &tx,
        account_id,
        requested_at_ms,
        sweep_attempt_id,
        workflow_cleanup_proof,
    )?;

    tx.execute(
        "DELETE FROM stripe_webhook_events
            WHERE json_extract(body, '$.data.object.client_reference_id') = ?1
               OR json_extract(body, '$.data.object.metadata.bluey_account_id') = ?1",
        params![account_id],
    )?;
    let deleted = tx.execute("DELETE FROM accounts WHERE id = ?1", params![account_id])?;
    let cascade_token_deleted = tx.execute(
        DELETE_WORKFLOW_CLEANUP_CASCADE_TOKEN_SQLITE,
        params![account_id],
    )?;
    anyhow::ensure!(
        deleted == 1 && cascade_token_deleted == 1,
        "runner-purge hard delete requires exactly one cascade token"
    );
    tx.commit()?;
    Ok(deleted > 0)
}

const POSTGRES_LOCK_RUNNER_PURGE_FLEET_FOR_HARD_DELETE_SQL: &str =
    "SELECT legacy_inventory_state, legacy_inventory_generation,
            legacy_inventory_reconciliation_id, legacy_inventory_authority_id,
            legacy_inventory_authority_sha256
       FROM jobs_runner_volume_fleet_state
      WHERE singleton_id = 1
      FOR UPDATE";

const POSTGRES_LOCK_ACCOUNT_FOR_RUNNER_PURGE_HARD_DELETE_SQL: &str =
    "SELECT id FROM accounts WHERE id = $1 FOR UPDATE";

fn hard_delete_account_after_runner_purge_postgres(
    pool: &DbPool,
    account_id: &str,
    requested_at_ms: i64,
    purge_request_id: &str,
    sweep_attempt_id: &str,
    workflow_cleanup_proof: &jobs::JobsWorkflowCleanupDeletionProof,
) -> Result<bool> {
    let mut conn = pool.get_pg()?;
    let mut tx = conn.transaction()?;

    // Match every runner-volume mutation's global-to-account lock order. The
    // singleton lock also prevents legacy authority from drifting after the
    // exact tombstone check but before the account row is deleted.
    let locked_fleet = tx.query_one(POSTGRES_LOCK_RUNNER_PURGE_FLEET_FOR_HARD_DELETE_SQL, &[])?;
    let legacy_inventory_state: String = locked_fleet.try_get(0)?;
    let legacy_inventory_generation: i64 = locked_fleet.try_get(1)?;
    let legacy_inventory_reconciliation_id: Option<String> = locked_fleet.try_get(2)?;
    let legacy_inventory_authority_id: Option<String> = locked_fleet.try_get(3)?;
    let legacy_inventory_authority_sha256: Option<String> = locked_fleet.try_get(4)?;
    let (
        Some(legacy_inventory_reconciliation_id),
        Some(legacy_inventory_authority_id),
        Some(legacy_inventory_authority_sha256),
    ) = (
        legacy_inventory_reconciliation_id,
        legacy_inventory_authority_id,
        legacy_inventory_authority_sha256,
    )
    else {
        anyhow::bail!("runner-purge hard delete requires the exact completed purge tombstone");
    };
    anyhow::ensure!(
        legacy_inventory_state == "ready" && legacy_inventory_generation >= 1,
        "runner-purge hard delete requires the exact completed purge tombstone"
    );

    let account_exists = tx
        .query_opt(
            POSTGRES_LOCK_ACCOUNT_FOR_RUNNER_PURGE_HARD_DELETE_SQL,
            &[&account_id],
        )?
        .is_some();
    if !account_exists {
        return Ok(false);
    }

    let intent = tx.query_opt(
        "SELECT fresh_upload_cutoff_ms, fresh_in_flight_puts
           FROM account_deletion_intents WHERE account_id = $1 FOR UPDATE",
        &[&account_id],
    )?;
    let Some(intent) = intent else {
        anyhow::bail!("runner-purge hard delete requires a durable deletion fence");
    };
    let fresh_upload_cutoff_ms: i64 = intent.try_get(0)?;
    let recorded_fresh_puts: i64 = intent.try_get(1)?;
    let live_fresh_puts: i64 = tx
        .query_one(
            "SELECT COUNT(*)::bigint FROM object_uploads
              WHERE account_id = $1 AND state = 'pending' AND updated_at_ms > $2",
            &[&account_id, &fresh_upload_cutoff_ms],
        )?
        .try_get(0)?;
    anyhow::ensure!(
        recorded_fresh_puts == 0 && live_fresh_puts == 0,
        "runner-purge hard delete refuses active object uploads"
    );
    let irreversible_count: i64 = tx
        .query_one(
            "SELECT COUNT(*)::bigint FROM jobs_applications AS application
              WHERE application.account_id = $1 AND application.state <> 'submitted'
                AND (
                  EXISTS (
                    SELECT 1 FROM jobs_execution_leases AS lease
                     WHERE lease.account_id = application.account_id
                       AND lease.application_id = application.id
                       AND lease.phase IN ('click_started', 'submitted', 'side_effect_unknown')
                  )
                  OR EXISTS (
                    SELECT 1 FROM jobs_local_run_tickets AS ticket
                     WHERE ticket.account_id = application.account_id
                       AND ticket.application_id = application.id
                       AND ticket.status IN ('click_started', 'side_effect_unknown')
                  )
                )",
            &[&account_id],
        )?
        .try_get(0)?;
    anyhow::ensure!(
        irreversible_count == 0,
        "runner-purge hard delete refuses unresolved irreversible submissions"
    );
    let irreversible_communication_count: i64 = tx
        .query_one(
            "SELECT COUNT(*)::bigint FROM jobs_communication_actions
              WHERE account_id = $1 AND status IN ('dispatching', 'side_effect_unknown')",
            &[&account_id],
        )?
        .try_get(0)?;
    anyhow::ensure!(
        irreversible_communication_count == 0,
        "runner-purge hard delete refuses unresolved irreversible communications"
    );
    let purge_is_complete: bool = tx
        .query_one(
            "SELECT EXISTS(
                SELECT 1
                  FROM jobs_runner_purge_requests AS request
                  JOIN jobs_runner_purge_tombstones AS tombstone
                    ON tombstone.request_id = request.request_id
                   AND tombstone.purge_generation = request.purge_generation
                   AND tombstone.purge_subject = request.purge_subject
                   AND tombstone.target_set_sha256 = request.target_set_sha256
                   AND tombstone.required_target_count = request.required_target_count
                 WHERE request.request_id = $1 AND request.account_id = $2
                   AND request.state = 'complete'
                   AND request.legacy_unresolved_count = 0
                   AND request.resolved_target_count = request.required_target_count
                   AND request.completed_at_ms IS NOT NULL
                   AND tombstone.completed_at_ms = request.completed_at_ms
                   AND request.legacy_inventory_generation = $3
                   AND request.legacy_inventory_reconciliation_id = $4
                   AND request.legacy_inventory_authority_id = $5
                   AND request.legacy_inventory_authority_sha256 = $6
             )",
            &[
                &purge_request_id,
                &account_id,
                &legacy_inventory_generation,
                &legacy_inventory_reconciliation_id,
                &legacy_inventory_authority_id,
                &legacy_inventory_authority_sha256,
            ],
        )?
        .get(0);
    anyhow::ensure!(
        purge_is_complete,
        "runner-purge hard delete requires the exact completed purge tombstone"
    );
    jobs::require_account_deletion_workflow_cleanup_complete_postgres_tx(
        &mut tx,
        account_id,
        requested_at_ms,
        sweep_attempt_id,
        workflow_cleanup_proof,
    )?;

    tx.execute(
        "DELETE FROM stripe_webhook_events
            WHERE body::jsonb #>> '{data,object,client_reference_id}' = $1
               OR body::jsonb #>> '{data,object,metadata,bluey_account_id}' = $1",
        &[&account_id],
    )?;
    let deleted = tx.execute("DELETE FROM accounts WHERE id = $1", &[&account_id])?;
    let cascade_token_deleted = tx.execute(
        DELETE_WORKFLOW_CLEANUP_CASCADE_TOKEN_POSTGRES,
        &[&account_id],
    )?;
    anyhow::ensure!(
        deleted == 1 && cascade_token_deleted == 1,
        "runner-purge hard delete requires exactly one cascade token"
    );
    tx.commit()?;
    Ok(deleted > 0)
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
        "SELECT logical_id, object_key, content_type, size_bytes, sha256, expires_at_ms,
                metadata_json
           FROM object_uploads
          WHERE account_id = ?1 AND object_kind = 'artifact' AND state <> 'deleted'
          ORDER BY created_at_ms",
    )?;
    let rows = stmt.query_map(params![account_id], |row| {
        Ok(ArtifactObjectRef {
            artifact_id: row.get(0)?,
            title: upload_ledger_title(&row.get::<_, String>(6)?),
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
        "SELECT logical_id, object_key, content_type, size_bytes, sha256, expires_at_ms,
                metadata_json
           FROM object_uploads
          WHERE account_id = $1 AND object_kind = 'artifact' AND state <> 'deleted'
          ORDER BY created_at_ms",
        &[&account_id],
    )?;
    for row in rows {
        let reference = ArtifactObjectRef {
            artifact_id: row.try_get(0)?,
            title: upload_ledger_title(&row.try_get::<_, String>(6)?),
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

fn upload_ledger_title(metadata_json: &str) -> String {
    serde_json::from_str::<serde_json::Value>(metadata_json)
        .ok()
        .and_then(|metadata| {
            metadata
                .get("title")
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|title| !title.is_empty())
                .map(ToString::to_string)
        })
        .unwrap_or_else(|| "Uploaded artifact".to_string())
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
    use crate::db::{open_pool, open_postgres_pool, run_migrations};
    use std::path::{Path, PathBuf};
    use tokio::sync::oneshot;

    fn test_pool() -> DbPool {
        let pool = open_pool(":memory:".as_ref()).unwrap();
        run_migrations(&pool).unwrap();
        let conn = pool.get().unwrap();
        for (id, email) in [
            ("acct-delete", "delete@example.test"),
            ("acct-active", "active@example.test"),
        ] {
            conn.execute(
                "INSERT INTO accounts(id, email, password_hash) VALUES (?1, ?2, 'hash')",
                params![id, email],
            )
            .unwrap();
        }
        drop(conn);
        pool
    }

    fn file_test_pool(label: &str) -> (DbPool, PathBuf) {
        let path = std::env::temp_dir().join(format!(
            "bluey-account-data-{label}-{}.db",
            uuid::Uuid::new_v4().simple()
        ));
        let pool = open_pool(&path).expect("open file-backed SQLite test pool");
        run_migrations(&pool).expect("apply SQLite test migrations");
        (pool, path)
    }

    fn remove_sqlite_test_files(path: &Path) {
        for candidate in [
            path.to_path_buf(),
            PathBuf::from(format!("{}-wal", path.display())),
            PathBuf::from(format!("{}-shm", path.display())),
        ] {
            if let Err(error) = std::fs::remove_file(&candidate) {
                assert_eq!(
                    error.kind(),
                    std::io::ErrorKind::NotFound,
                    "remove SQLite test file {}",
                    candidate.display()
                );
            }
        }
    }

    fn postgres_test_pool() -> Option<DbPool> {
        let database_url = std::env::var("BLUEY_TEST_POSTGRES_URL").ok()?;
        let pool = open_postgres_pool(&database_url).expect("open PostgreSQL test pool");
        run_migrations(&pool).expect("apply PostgreSQL test migrations");
        Some(pool)
    }

    fn insert_test_account(pool: &DbPool, account_id: &str) {
        let email = format!("{account_id}@example.test");
        crate::db::run_blocking_db(|| -> Result<()> {
            match pool {
                DbPool::Sqlite(_) => {
                    pool.get()?.execute(
                        "INSERT INTO accounts(id, email, password_hash) VALUES (?1, ?2, 'hash')",
                        params![account_id, email],
                    )?;
                    Ok(())
                }
                DbPool::Postgres(_) => {
                    pool.get_pg()?.execute(
                        "INSERT INTO accounts(id, email, password_hash) VALUES ($1, $2, 'hash')",
                        &[&account_id, &email],
                    )?;
                    Ok(())
                }
            }
        })
        .expect("insert account-deletion test account");
    }

    fn seed_irreversible_submissions(pool: &DbPool, account_id: &str, prefix: &str) {
        let cases = [
            ("cloud-click", "cloud", "click_started"),
            ("cloud-unknown", "cloud", "side_effect_unknown"),
            ("local-click", "local", "click_started"),
            ("local-unknown", "local", "side_effect_unknown"),
        ];
        crate::db::run_blocking_db(|| -> Result<()> {
            match pool {
                DbPool::Sqlite(_) => {
                    let mut conn = pool.get()?;
                    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
                    for (label, authority, phase) in cases {
                        let job_id = format!("{prefix}-job-{label}");
                        let application_id = format!("{prefix}-app-{label}");
                        tx.execute(
                            "INSERT INTO jobs_postings (
                            id, account_id, canonical_key, posting_json, source, company, title,
                            created_at_ms, updated_at_ms
                         ) VALUES (?1, ?2, ?3, '{}', 'test', 'Acme', 'Engineer', 1, 1)",
                            params![job_id, account_id, format!("{prefix}-canonical-{label}")],
                        )?;
                        tx.execute(
                            "INSERT INTO jobs_applications (
                            id, account_id, job_id, state, application_json,
                            created_at_ms, updated_at_ms
                         ) VALUES (?1, ?2, ?3, 'running', '{}', 1, 1)",
                            params![application_id, account_id, job_id],
                        )?;
                        if authority == "cloud" {
                            tx.execute(
                                "INSERT INTO jobs_execution_leases (
                                run_id, account_id, application_id, browser_profile_id, owner_id,
                                lease_token_sha256, fence, phase, lease_expires_at_ms,
                                created_at_ms, updated_at_ms
                             ) VALUES (?1, ?2, ?3, ?4, 'worker', ?5, 1, ?6, 10000, 1, 1)",
                                params![
                                    format!("{prefix}-run-{label}"),
                                    account_id,
                                    application_id,
                                    format!("{prefix}-profile-{label}"),
                                    "a".repeat(64),
                                    phase,
                                ],
                            )?;
                        } else {
                            tx.execute(
                                "INSERT INTO jobs_local_run_tickets (
                                id, account_id, application_id, ticket_hash, ticket_secret,
                                payload_json, status, expires_at_ms, created_at_ms, updated_at_ms
                             ) VALUES (?1, ?2, ?3, ?4, 'encrypted', '{}', ?5, 10000, 1, 1)",
                                params![
                                    format!("{prefix}-ticket-{label}"),
                                    account_id,
                                    application_id,
                                    format!("{prefix}-hash-{label}"),
                                    phase,
                                ],
                            )?;
                        }
                    }
                    tx.commit()?;
                    Ok(())
                }
                DbPool::Postgres(_) => {
                    let mut conn = pool.get_pg()?;
                    let mut tx = conn.transaction()?;
                    for (label, authority, phase) in cases {
                        let job_id = format!("{prefix}-job-{label}");
                        let application_id = format!("{prefix}-app-{label}");
                        tx.execute(
                            "INSERT INTO jobs_postings (
                            id, account_id, canonical_key, posting_json, source, company, title,
                            created_at_ms, updated_at_ms
                         ) VALUES ($1, $2, $3, '{}', 'test', 'Acme', 'Engineer', 1, 1)",
                            &[&job_id, &account_id, &format!("{prefix}-canonical-{label}")],
                        )?;
                        tx.execute(
                            "INSERT INTO jobs_applications (
                            id, account_id, job_id, state, application_json,
                            created_at_ms, updated_at_ms
                         ) VALUES ($1, $2, $3, 'running', '{}', 1, 1)",
                            &[&application_id, &account_id, &job_id],
                        )?;
                        if authority == "cloud" {
                            tx.execute(
                                "INSERT INTO jobs_execution_leases (
                                run_id, account_id, application_id, browser_profile_id, owner_id,
                                lease_token_sha256, fence, phase, lease_expires_at_ms,
                                created_at_ms, updated_at_ms
                             ) VALUES ($1, $2, $3, $4, 'worker', $5, 1, $6, 10000, 1, 1)",
                                &[
                                    &format!("{prefix}-run-{label}"),
                                    &account_id,
                                    &application_id,
                                    &format!("{prefix}-profile-{label}"),
                                    &"a".repeat(64),
                                    &phase,
                                ],
                            )?;
                        } else {
                            tx.execute(
                                "INSERT INTO jobs_local_run_tickets (
                                id, account_id, application_id, ticket_hash, ticket_secret,
                                payload_json, status, expires_at_ms, created_at_ms, updated_at_ms
                             ) VALUES ($1, $2, $3, $4, 'encrypted', '{}', $5, 10000, 1, 1)",
                                &[
                                    &format!("{prefix}-ticket-{label}"),
                                    &account_id,
                                    &application_id,
                                    &format!("{prefix}-hash-{label}"),
                                    &phase,
                                ],
                            )?;
                        }
                    }
                    tx.commit()?;
                    Ok(())
                }
            }
        })
        .expect("seed active irreversible submissions");
    }

    fn finish_irreversible_submissions(pool: &DbPool, account_id: &str) {
        crate::db::run_blocking_db(|| -> Result<()> {
            match pool {
                DbPool::Sqlite(_) => {
                    let mut conn = pool.get()?;
                    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
                    tx.execute(
                        "UPDATE jobs_execution_leases SET phase = 'failed' WHERE account_id = ?1",
                        params![account_id],
                    )?;
                    tx.execute(
                        "UPDATE jobs_local_run_tickets SET status = 'failed' WHERE account_id = ?1",
                        params![account_id],
                    )?;
                    tx.commit()?;
                    Ok(())
                }
                DbPool::Postgres(_) => {
                    let mut conn = pool.get_pg()?;
                    let mut tx = conn.transaction()?;
                    tx.execute(
                        "UPDATE jobs_execution_leases SET phase = 'failed' WHERE account_id = $1",
                        &[&account_id],
                    )?;
                    tx.execute(
                        "UPDATE jobs_local_run_tickets SET status = 'failed' WHERE account_id = $1",
                        &[&account_id],
                    )?;
                    tx.commit()?;
                    Ok(())
                }
            }
        })
        .expect("finish irreversible submissions");
    }

    fn seed_stale_pending_upload(pool: &DbPool, account_id: &str, prefix: &str) -> String {
        let upload_id = format!("{prefix}-upload");
        let outbox_id = format!("{prefix}-put");
        crate::db::run_blocking_db(|| -> Result<()> {
            match pool {
                DbPool::Sqlite(_) => {
                    let mut conn = pool.get()?;
                    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
                    tx.execute(
                        "INSERT INTO object_uploads (
                            id, account_id, object_kind, logical_id, storage_scope,
                            object_key, size_bytes, sha256, content_type, expires_at_ms,
                            state, metadata_json, created_at_ms, updated_at_ms
                         ) VALUES (
                            ?1, ?2, 'session_audit', ?3, 'audit', ?4, 10, ?5,
                            'application/json', 2000000, 'pending', '{}', 1, 1
                         )",
                        params![
                            upload_id,
                            account_id,
                            format!("{prefix}-logical"),
                            format!("objects/accounts/{account_id}/{prefix}"),
                            "c".repeat(64),
                        ],
                    )?;
                    tx.execute(
                        "INSERT INTO object_storage_outbox (
                            id, upload_id, account_id, operation, state, attempt_count,
                            next_attempt_at_ms, created_at_ms, updated_at_ms
                         ) VALUES (?1, ?2, ?3, 'put', 'pending', 0, 1, 1, 1)",
                        params![outbox_id, upload_id, account_id],
                    )?;
                    tx.commit()?;
                    Ok(())
                }
                DbPool::Postgres(_) => {
                    let mut conn = pool.get_pg()?;
                    let mut tx = conn.transaction()?;
                    tx.execute(
                        "INSERT INTO object_uploads (
                            id, account_id, object_kind, logical_id, storage_scope,
                            object_key, size_bytes, sha256, content_type, expires_at_ms,
                            state, metadata_json, created_at_ms, updated_at_ms
                         ) VALUES (
                            $1, $2, 'session_audit', $3, 'audit', $4, 10, $5,
                            'application/json', 2000000, 'pending', '{}', 1, 1
                         )",
                        &[
                            &upload_id,
                            &account_id,
                            &format!("{prefix}-logical"),
                            &format!("objects/accounts/{account_id}/{prefix}"),
                            &"c".repeat(64),
                        ],
                    )?;
                    tx.execute(
                        "INSERT INTO object_storage_outbox (
                            id, upload_id, account_id, operation, state, attempt_count,
                            next_attempt_at_ms, created_at_ms, updated_at_ms
                         ) VALUES ($1, $2, $3, 'put', 'pending', 0, 1, 1, 1)",
                        &[&outbox_id, &upload_id, &account_id],
                    )?;
                    tx.commit()?;
                    Ok(())
                }
            }
        })
        .expect("seed stale pending object upload");
        upload_id
    }

    fn test_account_write_fence(pool: &DbPool, account_id: &str) -> AccountWriteFence {
        crate::db::run_blocking_db(|| -> Result<AccountWriteFence> {
            match pool {
                DbPool::Sqlite(_) => {
                    let mut conn = pool.get()?;
                    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
                    let fence = account_write_fence_sqlite_tx(&tx, account_id)?;
                    tx.commit()?;
                    Ok(fence)
                }
                DbPool::Postgres(_) => {
                    let mut conn = pool.get_pg()?;
                    let mut tx = conn.transaction()?;
                    let fence = account_write_fence_postgres_tx(&mut tx, account_id)?;
                    tx.commit()?;
                    Ok(fence)
                }
            }
        })
        .expect("read account write fence")
    }

    fn delete_test_account(pool: &DbPool, account_id: &str) {
        crate::db::run_blocking_db(|| -> Result<()> {
            match pool {
                DbPool::Sqlite(_) => {
                    let mut conn = pool.get()?;
                    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
                    // These lifecycle tests establish a fence only to exercise
                    // writer serialization. Remove that test fixture before
                    // teardown so the production hard-delete guard remains
                    // strict and is never weakened for test cleanup.
                    tx.execute(
                        "DELETE FROM account_deletion_intents WHERE account_id = ?1",
                        params![account_id],
                    )?;
                    tx.execute("DELETE FROM accounts WHERE id = ?1", params![account_id])?;
                    tx.execute(
                        DELETE_WORKFLOW_CLEANUP_CASCADE_TOKEN_SQLITE,
                        params![account_id],
                    )?;
                    tx.commit()?;
                    Ok(())
                }
                DbPool::Postgres(_) => {
                    let mut conn = pool.get_pg()?;
                    let mut tx = conn.transaction()?;
                    tx.execute(
                        "DELETE FROM account_deletion_intents WHERE account_id = $1",
                        &[&account_id],
                    )?;
                    tx.execute("DELETE FROM accounts WHERE id = $1", &[&account_id])?;
                    tx.execute(
                        DELETE_WORKFLOW_CLEANUP_CASCADE_TOKEN_POSTGRES,
                        &[&account_id],
                    )?;
                    tx.commit()?;
                    Ok(())
                }
            }
        })
        .expect("delete account-deletion test account");
    }

    async fn assert_sqlite_account_object_lifecycle_serialization(
        pool: &DbPool,
        account_id: &str,
        upload_id: &str,
    ) {
        let active_writer = acquire_account_object_writer(pool, account_id)
            .await
            .expect("acquire active object writer");

        let (deletion_waiting_tx, deletion_waiting_rx) = oneshot::channel();
        let (deletion_fenced_tx, deletion_fenced_rx) = oneshot::channel();
        let (release_deletion_tx, release_deletion_rx) = oneshot::channel();
        let deletion_pool = pool.clone();
        let deletion_account_id = account_id.to_string();
        let deletion = tokio::spawn(async move {
            deletion_waiting_tx
                .send(())
                .expect("signal deletion lock attempt");
            let deletion_guard =
                acquire_account_object_deletion(&deletion_pool, &deletion_account_id)
                    .await
                    .expect("acquire exclusive deletion guard");
            let result = begin_account_deletion(&deletion_pool, &deletion_account_id, 1_000_000)
                .expect("establish account-deletion fence")
                .expect("deletion account exists");
            deletion_fenced_tx
                .send(result)
                .expect("signal durable deletion fence");
            release_deletion_rx
                .await
                .expect("release exclusive deletion guard");
            drop(deletion_guard);
        });
        deletion_waiting_rx
            .await
            .expect("deletion task reached exclusive lock");

        let (late_writer_waiting_tx, late_writer_waiting_rx) = oneshot::channel();
        let (late_writer_acquired_tx, mut late_writer_acquired_rx) = oneshot::channel();
        let (continue_late_writer_tx, continue_late_writer_rx) = oneshot::channel();
        let late_pool = pool.clone();
        let late_account_id = account_id.to_string();
        let late_upload_id = upload_id.to_string();
        let late_writer = tokio::spawn(async move {
            late_writer_waiting_tx
                .send(())
                .expect("signal late writer lock attempt");
            let writer_guard = acquire_account_object_writer(&late_pool, &late_account_id)
                .await
                .expect("acquire late object writer");
            late_writer_acquired_tx
                .send(())
                .expect("signal late writer acquisition");
            continue_late_writer_rx
                .await
                .expect("continue late object writer");
            let fence = test_account_write_fence(&late_pool, &late_account_id);
            let put_may_start = match crate::db::object_uploads::begin_upload_put(
                &late_pool,
                &late_upload_id,
                1_000_001,
            ) {
                Ok(_) => true,
                Err(error) => {
                    assert_eq!(
                        error.downcast_ref::<crate::db::object_uploads::UploadControlError>(),
                        Some(&crate::db::object_uploads::UploadControlError::AccountDeleting)
                    );
                    false
                }
            };
            drop(writer_guard);
            (fence, put_may_start)
        });
        late_writer_waiting_rx
            .await
            .expect("late writer reached shared lock");

        drop(active_writer);
        let deletion_result = tokio::select! {
            biased;
            acquired = &mut late_writer_acquired_rx => {
                acquired.expect("late writer acquisition signal");
                panic!("a writer queued after deletion bypassed the exclusive deletion guard");
            }
            fenced = deletion_fenced_rx => fenced.expect("deletion fence signal"),
        };
        assert!(matches!(
            deletion_result,
            BeginAccountDeletionResult::Ready(_)
        ));
        assert!(account_deletion_intent(pool, account_id)
            .expect("read durable deletion intent")
            .is_some());

        release_deletion_tx.send(()).expect("release deletion task");
        deletion.await.expect("join deletion task");
        late_writer_acquired_rx
            .await
            .expect("late writer acquires after deletion releases");
        continue_late_writer_tx
            .send(())
            .expect("continue fenced late writer");
        let (fence, put_may_start) = late_writer.await.expect("join late writer task");
        assert_eq!(fence, AccountWriteFence::DeletionRequested);
        assert!(
            !put_may_start,
            "a writer must revalidate the durable fence before beginning its PUT"
        );
    }

    #[test]
    fn deletion_intent_fences_writes_and_reports_fresh_pending_uploads() {
        let pool = test_pool();
        let now_ms = 1_000_000;
        let fresh_updated_at_ms = now_ms - 1;
        pool.get()
            .unwrap()
            .execute(
                "INSERT INTO object_uploads (
                    id, account_id, object_kind, logical_id, storage_scope,
                    object_key, size_bytes, sha256, content_type, expires_at_ms,
                    state, metadata_json, created_at_ms, updated_at_ms
                 ) VALUES (
                    'upload-fresh', 'acct-delete', 'artifact', 'receipt', 'artifact',
                    'objects/accounts/acct-delete/receipt', 10, ?1,
                    'application/json', ?2, 'pending', '{}', ?3, ?3
                 )",
                params!["a".repeat(64), now_ms + 10_000, fresh_updated_at_ms],
            )
            .unwrap();

        let started = begin_account_deletion(&pool, "acct-delete", now_ms)
            .unwrap()
            .expect("account exists");
        let BeginAccountDeletionResult::WaitingForUploads(intent) = started else {
            panic!("fresh pending upload must hold deletion");
        };
        assert_eq!(intent.fresh_in_flight_puts, 1);
        assert_eq!(intent.requested_at_ms, now_ms);
        assert_eq!(
            intent.fresh_upload_cutoff_ms,
            now_ms - ACCOUNT_DELETION_FRESH_UPLOAD_WINDOW_MS
        );
        assert_eq!(
            account_deletion_intent(&pool, "acct-delete").unwrap(),
            Some(intent.clone()),
            "the fence must commit even while deletion waits"
        );

        let mut conn = pool.get().unwrap();
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        assert_eq!(
            account_write_fence_sqlite_tx(&tx, "acct-delete").unwrap(),
            AccountWriteFence::DeletionRequested
        );
        assert_eq!(
            account_write_fence_sqlite_tx(&tx, "acct-active").unwrap(),
            AccountWriteFence::Active
        );
        assert_eq!(
            account_write_fence_sqlite_tx(&tx, "acct-missing").unwrap(),
            AccountWriteFence::Missing
        );
        tx.commit().unwrap();
    }

    #[test]
    fn deletion_retry_preserves_request_time_and_accepts_stale_pending_uploads() {
        let pool = test_pool();
        let first_check_ms = 1_000_000;
        pool.get()
            .unwrap()
            .execute(
                "INSERT INTO object_uploads (
                    id, account_id, object_kind, logical_id, storage_scope,
                    object_key, size_bytes, sha256, content_type, expires_at_ms,
                    state, metadata_json, created_at_ms, updated_at_ms
                 ) VALUES (
                    'upload-aging', 'acct-delete', 'artifact', 'receipt', 'artifact',
                    'objects/accounts/acct-delete/receipt', 10, ?1,
                    'application/json', ?2, 'pending', '{}', ?3, ?3
                 )",
                params!["b".repeat(64), first_check_ms + 1_000_000, first_check_ms],
            )
            .unwrap();
        assert!(matches!(
            begin_account_deletion(&pool, "acct-delete", first_check_ms)
                .unwrap()
                .unwrap(),
            BeginAccountDeletionResult::WaitingForUploads(_)
        ));

        let retry_ms = first_check_ms + ACCOUNT_DELETION_FRESH_UPLOAD_WINDOW_MS;
        let retried = begin_account_deletion(&pool, "acct-delete", retry_ms)
            .unwrap()
            .expect("account still exists");
        let BeginAccountDeletionResult::Ready(intent) = retried else {
            panic!("an upload at the stale cutoff must no longer block deletion");
        };
        assert_eq!(intent.requested_at_ms, first_check_ms);
        assert_eq!(intent.last_checked_at_ms, retry_ms);
        assert_eq!(intent.fresh_upload_cutoff_ms, first_check_ms);
        assert_eq!(intent.fresh_in_flight_puts, 0);
    }

    #[test]
    fn irreversible_submission_outcomes_block_deletion_without_creating_a_fence() {
        let pool = test_pool();
        let legacy = jobs::prepare_jobs_legacy_inventory_generation(
            &pool,
            &jobs::PrepareJobsLegacyInventoryGeneration {
                namespace: "bluey-jobs-account-delete-test".to_string(),
                visibility_cutoff_ms: 900_000,
                confirmation_age_ms: 1_000,
                now_ms: 900_000,
            },
        )
        .unwrap();
        seed_irreversible_submissions(&pool, "acct-delete", "sqlite-irreversible");

        let blocked = begin_account_deletion_with_workflow_cleanup(
            &pool,
            "acct-delete",
            1_000_000,
            &legacy.authority,
        )
        .unwrap()
        .expect("account exists");
        assert_eq!(
            blocked.deletion,
            BeginAccountDeletionResult::WaitingForIrreversibleSubmissions {
                active_submissions: 4,
            }
        );
        assert!(blocked.workflow_cleanup.is_none());
        assert_eq!(
            account_deletion_intent(&pool, "acct-delete").unwrap(),
            None,
            "click_started and side_effect_unknown outcomes must not create deletion intent"
        );
        let cleanup_binding_count: i64 = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM jobs_workflow_cleanup_account_bindings
                  WHERE account_id = 'acct-delete'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(cleanup_binding_count, 0);

        finish_irreversible_submissions(&pool, "acct-delete");
        let ready = begin_account_deletion_with_workflow_cleanup(
            &pool,
            "acct-delete",
            1_000_001,
            &legacy.authority,
        )
        .unwrap()
        .expect("account exists");
        assert!(matches!(
            ready.deletion,
            BeginAccountDeletionResult::Ready(_)
        ));
        assert!(ready.workflow_cleanup.is_some());
        assert!(account_deletion_intent(&pool, "acct-delete")
            .unwrap()
            .is_some());
    }

    #[test]
    fn workflow_cleanup_authority_drift_rolls_back_the_deletion_intent() {
        let pool = test_pool();
        let prepared = jobs::prepare_jobs_legacy_inventory_generation(
            &pool,
            &jobs::PrepareJobsLegacyInventoryGeneration {
                namespace: "bluey-jobs-account-drift-test".to_string(),
                visibility_cutoff_ms: 900_000,
                confirmation_age_ms: 1_000,
                now_ms: 900_000,
            },
        )
        .unwrap();
        let mut stale = prepared.authority;
        stale.query_digest = "9".repeat(64);

        assert!(begin_account_deletion_with_workflow_cleanup(
            &pool,
            "acct-delete",
            1_000_000,
            &stale,
        )
        .is_err());
        assert_eq!(
            account_deletion_intent(&pool, "acct-delete").unwrap(),
            None,
            "authority mismatch must roll the deletion intent back"
        );
        let cleanup_binding_count: i64 = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM jobs_workflow_cleanup_account_bindings
                  WHERE account_id = 'acct-delete'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(cleanup_binding_count, 0);
    }

    #[test]
    fn cloud_browser_state_is_fenced_before_async_volume_cleanup() {
        let pool = test_pool();
        pool.get()
            .unwrap()
            .execute(
                "INSERT INTO jobs_browser_sessions (
                    id, account_id, runner, status, session_json, created_at_ms, updated_at_ms
                 ) VALUES
                    ('cloud-session', 'acct-delete', 'cloud', 'needs_input', '{}', 1, 1),
                    ('local-session', 'acct-delete', 'local', 'completed', '{}', 1, 1)",
                [],
            )
            .unwrap();

        assert!(matches!(
            begin_account_deletion(&pool, "acct-delete", 1_000_000)
                .unwrap()
                .unwrap(),
            BeginAccountDeletionResult::Ready(_)
        ));
        assert!(account_deletion_intent(&pool, "acct-delete")
            .unwrap()
            .is_some());

        let error = jobs::upsert_browser_session(
            &pool,
            "acct-delete",
            &jobs::BrowserSession {
                id: "late-cloud-session".to_string(),
                runner: "cloud".to_string(),
                status: "queued".to_string(),
                current_company: "Acme".to_string(),
                current_step: "Waiting for runner".to_string(),
                application_id: None,
                takeover_url: None,
                created_at_ms: 0,
                updated_at_ms: 0,
            },
        )
        .expect_err("the deletion fence must reject new cloud browser state");
        assert_eq!(
            error.downcast_ref::<crate::db::object_uploads::UploadControlError>(),
            Some(&crate::db::object_uploads::UploadControlError::AccountDeleting)
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn sqlite_deletion_guard_precedes_late_writer_and_exposes_the_durable_fence() {
        let (pool, path) = file_test_pool("lifecycle-serialization");
        let account_id = "acct-lifecycle-sqlite";
        insert_test_account(&pool, account_id);
        let upload_id = seed_stale_pending_upload(&pool, account_id, "sqlite-lifecycle");

        assert_sqlite_account_object_lifecycle_serialization(&pool, account_id, &upload_id).await;

        delete_test_account(&pool, account_id);
        drop(pool);
        remove_sqlite_test_files(&path);
    }

    #[test]
    #[serial_test::serial]
    fn postgres_irreversible_submission_outcomes_block_deletion_without_creating_a_fence() {
        let Some(pool) = postgres_test_pool() else {
            return;
        };
        let suffix = uuid::Uuid::new_v4().simple().to_string();
        let account_id = format!("acct-delete-pg-{suffix}");
        let prefix = format!("pg-irreversible-{suffix}");
        insert_test_account(&pool, &account_id);
        seed_irreversible_submissions(&pool, &account_id, &prefix);

        assert_eq!(
            begin_account_deletion(&pool, &account_id, 1_000_000).unwrap(),
            Some(
                BeginAccountDeletionResult::WaitingForIrreversibleSubmissions {
                    active_submissions: 4,
                }
            )
        );
        assert_eq!(
            account_deletion_intent(&pool, &account_id).unwrap(),
            None,
            "PostgreSQL must not create a deletion intent around an irreversible outcome"
        );

        finish_irreversible_submissions(&pool, &account_id);
        assert!(matches!(
            begin_account_deletion(&pool, &account_id, 1_000_001)
                .unwrap()
                .unwrap(),
            BeginAccountDeletionResult::Ready(_)
        ));
        delete_test_account(&pool, &account_id);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    #[serial_test::serial]
    async fn postgres_advisory_coordination_blocks_deletion_behind_an_active_writer() {
        let Some(pool) = postgres_test_pool() else {
            return;
        };
        let deletion_replica = open_postgres_pool(
            &std::env::var("BLUEY_TEST_POSTGRES_URL")
                .expect("PostgreSQL test URL was present for the first replica"),
        )
        .expect("open independent PostgreSQL deletion replica");
        let suffix = uuid::Uuid::new_v4().simple().to_string();
        let account_id = format!("acct-lifecycle-pg-{suffix}");
        insert_test_account(&pool, &account_id);
        let upload_id =
            seed_stale_pending_upload(&pool, &account_id, &format!("pg-lifecycle-{suffix}"));

        let writer_coordination = acquire_account_object_writer(&pool, &account_id)
            .await
            .expect("enter PostgreSQL object-writer coordination");

        let (attempting_tx, attempting_rx) = oneshot::channel();
        let (acquired_tx, mut acquired_rx) = oneshot::channel();
        let (release_tx, release_rx) = oneshot::channel();
        let deletion_pool = deletion_replica.clone();
        let deletion_account_id = account_id.clone();
        let deletion_task = tokio::spawn(async move {
            attempting_tx
                .send(())
                .expect("signal PostgreSQL deletion lock attempt");
            let deletion_guard =
                acquire_account_object_deletion(&deletion_pool, &deletion_account_id)
                    .await
                    .expect("acquire PostgreSQL deletion lock");
            acquired_tx
                .send(())
                .expect("signal PostgreSQL deletion lock acquisition");
            release_rx.await.expect("release PostgreSQL deletion lock");
            drop(deletion_guard);
        });
        attempting_rx
            .await
            .expect("deletion task reached PostgreSQL advisory lock");
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(100), &mut acquired_rx)
                .await
                .is_err(),
            "the exclusive deletion lock must wait for an active shared writer"
        );

        let put_started_at_ms = 1_000_000;
        crate::db::object_uploads::begin_upload_put(&pool, &upload_id, put_started_at_ms)
            .expect("renew the durable PUT lease before object I/O");
        drop(writer_coordination);
        acquired_rx
            .await
            .expect("deletion acquires after the active writer releases");

        let deletion =
            begin_account_deletion(&deletion_replica, &account_id, put_started_at_ms + 1)
                .expect("establish the durable PostgreSQL deletion fence")
                .expect("deletion account exists");
        let BeginAccountDeletionResult::WaitingForUploads(intent) = deletion else {
            panic!("the fresh durable PUT lease must stop account deletion");
        };
        assert_eq!(intent.fresh_in_flight_puts, 1);
        assert_eq!(
            account_deletion_intent(&pool, &account_id).unwrap(),
            Some(intent),
            "the deletion fence must commit while the caller waits for the lease"
        );
        release_tx
            .send(())
            .expect("release PostgreSQL deletion coordination");
        deletion_task.await.expect("join PostgreSQL deletion task");

        let error =
            crate::db::object_uploads::begin_upload_put(&pool, &upload_id, put_started_at_ms + 2)
                .expect_err("the durable deletion intent must reject every later PUT begin");
        assert_eq!(
            error.downcast_ref::<crate::db::object_uploads::UploadControlError>(),
            Some(&crate::db::object_uploads::UploadControlError::AccountDeleting)
        );

        delete_test_account(&pool, &account_id);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    #[serial_test::serial]
    async fn postgres_lifecycle_pool_saturation_does_not_starve_primary_finalization() {
        let Some(pool) = postgres_test_pool() else {
            return;
        };
        let suffix = uuid::Uuid::new_v4().simple().to_string();
        let account_id = format!("acct-lifecycle-capacity-pg-{suffix}");
        insert_test_account(&pool, &account_id);
        let upload_id =
            seed_stale_pending_upload(&pool, &account_id, &format!("pg-capacity-{suffix}"));
        let lifecycle_capacity = match &pool {
            DbPool::Postgres(pools) => pools.lifecycle_pool().max_size(),
            DbPool::Sqlite(_) => unreachable!("PostgreSQL test helper returned SQLite"),
        };

        let mut writer_guards = Vec::new();
        for _ in 0..lifecycle_capacity {
            writer_guards.push(
                acquire_account_object_writer(&pool, &account_id)
                    .await
                    .expect("fill one lifecycle writer slot"),
            );
        }

        let finalization_pool = pool.clone();
        let finalization_upload_id = upload_id.clone();
        tokio::time::timeout(
            std::time::Duration::from_secs(5),
            tokio::task::spawn_blocking(move || {
                crate::db::object_uploads::begin_upload_put(
                    &finalization_pool,
                    &finalization_upload_id,
                    1_000_000,
                )?;
                crate::db::object_uploads::mark_upload_ready(
                    &finalization_pool,
                    &finalization_upload_id,
                    1_000_001,
                )?;
                Result::<()>::Ok(())
            }),
        )
        .await
        .expect("primary-pool finalization must not wait for lifecycle capacity")
        .expect("join primary-pool finalization")
        .expect("finalize upload through the primary pool");

        let (attempting_tx, attempting_rx) = oneshot::channel();
        let (acquired_tx, mut acquired_rx) = oneshot::channel();
        let (release_tx, release_rx) = oneshot::channel();
        let deletion_pool = pool.clone();
        let deletion_account_id = account_id.clone();
        let deletion_task = tokio::spawn(async move {
            attempting_tx
                .send(())
                .expect("signal saturated deletion attempt");
            let deletion_guard =
                acquire_account_object_deletion(&deletion_pool, &deletion_account_id)
                    .await
                    .expect("acquire deletion after saturated writers drain");
            acquired_tx
                .send(())
                .expect("signal deletion acquisition after saturation");
            release_rx.await.expect("release saturated deletion guard");
            drop(deletion_guard);
        });
        attempting_rx.await.expect("deletion task started");
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(100), &mut acquired_rx)
                .await
                .is_err(),
            "deletion must wait while every lifecycle slot belongs to an active writer"
        );

        drop(writer_guards);
        tokio::time::timeout(std::time::Duration::from_secs(5), &mut acquired_rx)
            .await
            .expect("deletion should acquire after saturated writers release")
            .expect("receive saturated deletion acquisition");
        release_tx
            .send(())
            .expect("release deletion after saturation test");
        deletion_task.await.expect("join saturated deletion task");
        delete_test_account(&pool, &account_id);
    }

    #[test]
    fn postgres_account_object_lock_keys_are_stable_and_account_scoped() {
        assert_eq!(
            postgres_account_object_lock_key("acct-one"),
            postgres_account_object_lock_key("acct-one")
        );
        assert_ne!(
            postgres_account_object_lock_key("acct-one"),
            postgres_account_object_lock_key("acct-two")
        );
    }

    #[test]
    fn postgres_runner_purge_hard_delete_locks_fleet_before_account() {
        assert!(POSTGRES_LOCK_RUNNER_PURGE_FLEET_FOR_HARD_DELETE_SQL
            .contains("WHERE singleton_id = 1\n      FOR UPDATE"));
        assert!(POSTGRES_LOCK_ACCOUNT_FOR_RUNNER_PURGE_HARD_DELETE_SQL.ends_with("FOR UPDATE"));

        // There is no live PostgreSQL dependency in the unit-test harness, so
        // keep a structural regression for the security-critical lock order
        // and exact authority binding used by the production transaction.
        let source = include_str!("account_data.rs");
        let function = source
            .split_once("fn hard_delete_account_after_runner_purge_postgres(")
            .expect("PostgreSQL runner-purge hard-delete function")
            .1
            .split_once("\nfn artifact_object_refs_sqlite")
            .expect("end of PostgreSQL runner-purge hard-delete function")
            .0;
        let fleet_lock = function
            .find("POSTGRES_LOCK_RUNNER_PURGE_FLEET_FOR_HARD_DELETE_SQL")
            .expect("fleet singleton lock");
        let account_lock = function
            .find("POSTGRES_LOCK_ACCOUNT_FOR_RUNNER_PURGE_HARD_DELETE_SQL")
            .expect("account row lock");
        let authority_check = function
            .find("request.legacy_inventory_generation = $3")
            .expect("exact locked authority check");
        let workflow_cleanup_check = function
            .find("require_account_deletion_workflow_cleanup_complete_postgres_tx")
            .expect("exact workflow-cleanup and object-sweep check");
        let account_delete = function
            .find("DELETE FROM accounts WHERE id = $1")
            .expect("account deletion");
        let cascade_token_cleanup = function
            .find("DELETE_WORKFLOW_CLEANUP_CASCADE_TOKEN_POSTGRES")
            .expect("post-cascade token cleanup");
        let commit = function.find("tx.commit()?").expect("transaction commit");

        assert!(fleet_lock < account_lock);
        assert!(account_lock < authority_check);
        assert!(authority_check < workflow_cleanup_check);
        assert!(workflow_cleanup_check < account_delete);
        assert!(account_delete < cascade_token_cleanup);
        assert!(cascade_token_cleanup < commit);
        assert!(function.contains("deleted == 1 && cascade_token_deleted == 1"));
        assert!(function.contains("sweep_attempt_id"));
        for exact_authority_clause in [
            "request.legacy_inventory_reconciliation_id = $4",
            "request.legacy_inventory_authority_id = $5",
            "request.legacy_inventory_authority_sha256 = $6",
        ] {
            assert!(function.contains(exact_authority_clause));
        }
    }

    #[test]
    fn postgres_cascade_token_lifetime_is_deferred_and_caller_owned() {
        let migration = include_str!(
            "../../../infra/postgres/server-runtime/032_jobs_workflow_cleanup_authority.sql"
        );
        assert!(migration.contains(
            "FOREIGN KEY(account_id) REFERENCES accounts(id)\n    \
             ON DELETE NO ACTION DEFERRABLE INITIALLY DEFERRED"
        ));
        assert!(migration.contains(concat!(
            "DROP TRIGGER IF EXISTS ",
            "trg_jobs_workflow_account_delete_cascade_token_cleanup ON accounts;"
        )));
        assert!(!migration
            .contains("CREATE TRIGGER trg_jobs_workflow_account_delete_cascade_token_cleanup"));
        assert!(!migration
            .contains("CREATE OR REPLACE FUNCTION cleanup_jobs_workflow_account_delete_token"));

        let source = include_str!("account_data.rs");
        let setup_failure = source
            .split_once("fn hard_delete_account_after_setup_failure_postgres(")
            .expect("PostgreSQL setup-failure hard-delete function")
            .1
            .split_once("\n// The account BEFORE DELETE guard")
            .expect("end of PostgreSQL setup-failure hard-delete function")
            .0;
        let account_delete = setup_failure
            .find("DELETE FROM accounts WHERE id = $1")
            .expect("setup-failure account deletion");
        let cascade_token_cleanup = setup_failure
            .find("DELETE_WORKFLOW_CLEANUP_CASCADE_TOKEN_POSTGRES")
            .expect("setup-failure cascade-token cleanup");
        let commit = setup_failure
            .find("tx.commit()?")
            .expect("setup-failure commit");
        assert!(account_delete < cascade_token_cleanup);
        assert!(cascade_token_cleanup < commit);
    }

    #[test]
    fn hard_delete_requires_exact_runner_and_workflow_cleanup_tombstones() {
        let pool = test_pool();
        let reconciling = jobs::record_runner_legacy_inventory_authority(
            &pool,
            &jobs::RecordRunnerLegacyInventoryAuthorityRequest {
                reconciliation_id: "account-delete-inventory".to_string(),
                authority_state: "reconciling".to_string(),
                expected_predecessor_generation: 0,
                expected_predecessor_authority_id: None,
                expected_predecessor_authority_sha256: None,
                root_count: 0,
                root_set_sha256: jobs::EMPTY_RUNNER_LEGACY_ROOT_SET_SHA256.to_string(),
                scope_ref: "all-managed-runner-storage-roots".to_string(),
                evidence_ref: "inventory-scan-start".to_string(),
                evidence_sha256: "1".repeat(64),
                authorized_by: "account-delete-admin".to_string(),
                recorded_at_ms: 999_998,
            },
        )
        .unwrap()
        .authority;
        let ready = jobs::record_runner_legacy_inventory_authority(
            &pool,
            &jobs::RecordRunnerLegacyInventoryAuthorityRequest {
                reconciliation_id: reconciling.reconciliation_id.clone(),
                authority_state: "ready".to_string(),
                expected_predecessor_generation: reconciling.authority_generation,
                expected_predecessor_authority_id: Some(reconciling.authority_id.clone()),
                expected_predecessor_authority_sha256: Some(reconciling.authority_sha256.clone()),
                root_count: reconciling.root_count,
                root_set_sha256: reconciling.root_set_sha256.clone(),
                scope_ref: reconciling.scope_ref.clone(),
                evidence_ref: "inventory-scan-complete".to_string(),
                evidence_sha256: "2".repeat(64),
                authorized_by: "account-delete-admin".to_string(),
                recorded_at_ms: 999_999,
            },
        )
        .unwrap()
        .authority;
        assert!(matches!(
            begin_account_deletion(&pool, "acct-delete", 1_000_000)
                .unwrap()
                .unwrap(),
            BeginAccountDeletionResult::Ready(_)
        ));
        assert!(account_deletion_intent(&pool, "acct-delete")
            .unwrap()
            .is_some());
        let missing_workflow_proof = jobs::JobsWorkflowCleanupDeletionProof {
            account_generation: 1_000_000,
            cleanup_generation_id: "wfcleanupgen-v3-missing-proof".to_string(),
            target_set_digest: "7".repeat(64),
            legacy_authority: jobs::JobsLegacyInventoryAuthorityRef {
                inventory_generation_id: "wfinventory-v3-missing-proof".to_string(),
                query_digest: "8".repeat(64),
            },
            tombstone_id: "wfcleantomb-v3-missing-proof".to_string(),
            completion_digest: "9".repeat(64),
        };

        assert!(hard_delete_account_after_setup_failure(&pool, "acct-delete").is_err());
        assert!(hard_delete_account_after_runner_purge(
            &pool,
            "acct-delete",
            1_000_000,
            "delete-request",
            "wfsweep-account-v3-missing-test",
            &missing_workflow_proof,
        )
        .is_err());

        let conn = pool.get().unwrap();
        conn.execute(
            "INSERT INTO jobs_runner_purge_requests (
                request_id, deletion_request_id, account_id, purge_subject, purge_generation,
                legacy_inventory_generation, legacy_inventory_reconciliation_id,
                legacy_inventory_authority_id, legacy_inventory_authority_sha256, state,
                legacy_unresolved_count, required_target_count, resolved_target_count,
                target_set_sha256, created_at_ms, updated_at_ms, completed_at_ms
             ) VALUES (?1, ?1, ?2, ?3, 1, ?4, ?5, ?6, ?7, 'pending', 0, 0, 0, ?8, ?9, ?9, NULL)",
            params![
                "delete-request",
                "acct-delete",
                "A".repeat(43),
                ready.authority_generation,
                ready.reconciliation_id,
                ready.authority_id,
                ready.authority_sha256,
                "0".repeat(64),
                1_000_001_i64,
            ],
        )
        .unwrap();
        drop(conn);
        assert!(hard_delete_account_after_runner_purge(
            &pool,
            "acct-delete",
            1_000_000,
            "delete-request",
            "wfsweep-account-v3-missing-test",
            &missing_workflow_proof,
        )
        .is_err());

        let conn = pool.get().unwrap();
        conn.execute(
            "UPDATE jobs_runner_purge_requests
                SET state = 'complete', updated_at_ms = ?2, completed_at_ms = ?2
              WHERE request_id = ?1",
            params!["delete-request", 1_000_001_i64],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO jobs_runner_purge_tombstones (
                purge_subject, request_id, purge_generation, tombstone_generation,
                target_set_sha256, required_target_count, completed_at_ms
             ) VALUES (?1, ?2, 1, 1, ?3, 0, ?4)",
            params![
                "A".repeat(43),
                "delete-request",
                "0".repeat(64),
                1_000_001_i64,
            ],
        )
        .unwrap();
        assert!(conn
            .execute(
                "UPDATE jobs_runner_purge_requests SET target_set_sha256 = ?2 \
                  WHERE request_id = ?1",
                params!["delete-request", "9".repeat(64)],
            )
            .is_err());
        assert!(conn
            .execute(
                "UPDATE jobs_runner_purge_tombstones SET required_target_count = 1 \
                  WHERE request_id = ?1",
                params!["delete-request"],
            )
            .is_err());
        drop(conn);

        pool.get()
            .unwrap()
            .execute(
                "UPDATE jobs_runner_volume_fleet_state \
                    SET legacy_inventory_state = 'reconciling' WHERE singleton_id = 1",
                [],
            )
            .unwrap();
        assert!(hard_delete_account_after_runner_purge(
            &pool,
            "acct-delete",
            1_000_000,
            "delete-request",
            "wfsweep-account-v3-missing-test",
            &missing_workflow_proof,
        )
        .is_err());
        pool.get()
            .unwrap()
            .execute(
                "UPDATE jobs_runner_volume_fleet_state \
                    SET legacy_inventory_state = 'ready' WHERE singleton_id = 1",
                [],
            )
            .unwrap();

        assert!(hard_delete_account_after_runner_purge(
            &pool,
            "acct-delete",
            1_000_000,
            "delete-request",
            "wfsweep-account-v3-missing-test",
            &missing_workflow_proof,
        )
        .is_err());
        assert!(account_deletion_intent(&pool, "acct-delete")
            .unwrap()
            .is_some());
    }

    #[test]
    fn setup_failure_cleanup_refuses_deletion_authority_but_removes_fresh_account() {
        let pool = test_pool();
        assert!(hard_delete_account_after_setup_failure(&pool, "acct-active").unwrap());
        assert!(!hard_delete_account_after_setup_failure(&pool, "acct-active").unwrap());
    }

    #[test]
    fn sqlite_cascade_token_survives_child_deletes_and_must_be_removed_before_commit() {
        let (pool, path) = file_test_pool("cascade-token-lifetime");
        let account_id = "acct-cascade-token";
        insert_test_account(&pool, account_id);

        {
            let conn = pool.get().unwrap();
            let obsolete_cleanup_trigger_count: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master
                      WHERE type = 'trigger'
                        AND name = 'trg_jobs_workflow_account_delete_cascade_token_cleanup'",
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(obsolete_cleanup_trigger_count, 0);
            conn.execute_batch(
                "CREATE TABLE account_data_cascade_token_children (
                    account_id TEXT PRIMARY KEY
                      REFERENCES accounts(id) ON DELETE CASCADE
                 );
                 CREATE TRIGGER account_data_test_account_delete_token
                 BEFORE DELETE ON accounts
                 WHEN OLD.id = 'acct-cascade-token'
                 BEGIN
                   INSERT INTO jobs_workflow_cleanup_hard_delete_cascade_tokens (
                     account_id, account_generation, sweep_attempt_id,
                     authorization_digest_sha256, created_at_ms
                   ) VALUES (
                     OLD.id, 1, 'wfsweep-cascade-token-lifetime',
                     'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa', 1
                   );
                 END;
                 CREATE TRIGGER account_data_test_child_delete_guard
                 BEFORE DELETE ON account_data_cascade_token_children
                 WHEN NOT EXISTS (
                   SELECT 1 FROM jobs_workflow_cleanup_hard_delete_cascade_tokens token
                    WHERE token.account_id = OLD.account_id
                      AND NOT EXISTS (
                        SELECT 1 FROM accounts account
                         WHERE account.id = OLD.account_id
                      )
                 )
                 BEGIN
                   SELECT RAISE(ABORT, 'cascade token disappeared before child delete');
                 END;",
            )
            .unwrap();
            conn.execute(
                "INSERT INTO account_data_cascade_token_children(account_id) VALUES (?1)",
                params![account_id],
            )
            .unwrap();
        }

        {
            let mut conn = pool.get().unwrap();
            let tx = conn
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .unwrap();
            assert_eq!(
                tx.execute("DELETE FROM accounts WHERE id = ?1", params![account_id])
                    .unwrap(),
                1,
                "the parent DELETE and guarded child cascade must finish before commit"
            );
            let error = tx
                .commit()
                .expect_err("omitting explicit token cleanup must fail the deferred FK");
            assert!(error.to_string().contains("FOREIGN KEY constraint failed"));
        }

        let rolled_back: (i64, i64, i64) = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT
                    (SELECT COUNT(*) FROM accounts WHERE id = ?1),
                    (SELECT COUNT(*) FROM account_data_cascade_token_children
                      WHERE account_id = ?1),
                    (SELECT COUNT(*)
                       FROM jobs_workflow_cleanup_hard_delete_cascade_tokens
                      WHERE account_id = ?1)",
                params![account_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(rolled_back, (1, 1, 0));

        assert!(hard_delete_account_after_setup_failure(&pool, account_id).unwrap());
        let committed: (i64, i64, i64) = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT
                    (SELECT COUNT(*) FROM accounts WHERE id = ?1),
                    (SELECT COUNT(*) FROM account_data_cascade_token_children
                      WHERE account_id = ?1),
                    (SELECT COUNT(*)
                       FROM jobs_workflow_cleanup_hard_delete_cascade_tokens
                      WHERE account_id = ?1)",
                params![account_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(committed, (0, 0, 0));

        drop(pool);
        remove_sqlite_test_files(&path);
    }

    #[test]
    fn upload_ledger_title_uses_server_owned_metadata_with_safe_fallback() {
        assert_eq!(
            upload_ledger_title(r#"{"title":"Application receipt bundle"}"#),
            "Application receipt bundle"
        );
        assert_eq!(
            upload_ledger_title(r#"{"title":"   "}"#),
            "Uploaded artifact"
        );
        assert_eq!(upload_ledger_title("not-json"), "Uploaded artifact");
    }
}
