//! Durable, metadata-only queue for local RAG index rebuilds.
//!
//! The queue intentionally stores no transcript, document, screenshot, prompt,
//! or answer content. Workers reload the current meeting from `MeetingStore`
//! after claiming a job and verify both owner scope and revision before indexing.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result};
use cue_rag::RagScope;
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};

const STATUS_PENDING: &str = "pending";
const STATUS_PROCESSING: &str = "processing";
const STATUS_RETRY: &str = "retry";
const STATUS_COMPLETE: &str = "complete";
const STATUS_DEAD: &str = "dead";

const MAX_ATTEMPTS: u32 = 6;
const BASE_RETRY_DELAY_MS: i64 = 1_000;
const MAX_RETRY_DELAY_MS: i64 = 5 * 60 * 1_000;

#[cfg(test)]
type RagIndexQueueState = (String, String, u32, i64, Option<String>);

#[derive(Debug, Clone)]
pub(crate) struct RagIndexJob {
    pub(crate) account_id: String,
    pub(crate) workspace_id: String,
    pub(crate) session_id: String,
    pub(crate) claimed_revision: String,
    pub(crate) lease_owner: String,
    pub(crate) attempts: u32,
}

#[derive(Debug, Clone)]
pub(crate) struct RagIndexQueue {
    path: PathBuf,
}

impl RagIndexQueue {
    pub(crate) fn open(path: PathBuf) -> Result<Self> {
        let queue = Self { path };
        let conn = queue.connect()?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS rag_index_jobs (
                account_id TEXT NOT NULL,
                workspace_id TEXT NOT NULL,
                session_id TEXT NOT NULL,
                requested_revision TEXT NOT NULL,
                claimed_revision TEXT,
                status TEXT NOT NULL,
                attempts INTEGER NOT NULL DEFAULT 0,
                next_attempt_at_ms INTEGER NOT NULL DEFAULT 0,
                lease_owner TEXT,
                lease_expires_at_ms INTEGER,
                last_error_code TEXT,
                created_at_ms INTEGER NOT NULL,
                updated_at_ms INTEGER NOT NULL,
                PRIMARY KEY (account_id, workspace_id, session_id),
                CHECK (status IN ('pending', 'processing', 'retry', 'complete', 'dead'))
            );
            CREATE INDEX IF NOT EXISTS idx_rag_index_jobs_due
            ON rag_index_jobs (
                account_id,
                workspace_id,
                status,
                next_attempt_at_ms,
                updated_at_ms
            );",
        )
        .context("create RAG index queue schema")?;
        Ok(queue)
    }

    /// Insert or coalesce a session rebuild.
    ///
    /// A revision already pending, processing, retrying, or complete is a
    /// no-op. A changed revision replaces the requested target. If an older
    /// revision is currently processing, its lease remains intact and its
    /// completion will atomically transition the row back to `pending`.
    pub(crate) fn enqueue(
        &self,
        scope: &RagScope,
        session_id: &str,
        revision: &str,
        now_ms: i64,
    ) -> Result<bool> {
        validate_identifier("session id", session_id)?;
        validate_revision(revision)?;
        let account_id = scope.account_id();
        let workspace_id = normalized_workspace(scope);
        let mut conn = self.connect()?;
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .context("begin RAG queue enqueue")?;
        let existing = tx
            .query_row(
                "SELECT requested_revision, status, attempts,
                        COALESCE(lease_expires_at_ms, 0)
                 FROM rag_index_jobs
                 WHERE account_id = ?1 AND workspace_id = ?2 AND session_id = ?3",
                params![account_id, workspace_id, session_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, i64>(3)?,
                    ))
                },
            )
            .optional()?;

        let changed = match existing {
            None => {
                tx.execute(
                    "INSERT INTO rag_index_jobs (
                        account_id, workspace_id, session_id, requested_revision,
                        status, attempts, next_attempt_at_ms, created_at_ms, updated_at_ms
                     ) VALUES (?1, ?2, ?3, ?4, ?5, 0, 0, ?6, ?6)",
                    params![
                        account_id,
                        workspace_id,
                        session_id,
                        revision,
                        STATUS_PENDING,
                        now_ms
                    ],
                )?;
                true
            }
            Some((current_revision, status, attempts, lease_expires_at_ms)) => {
                let same_revision = current_revision == revision;
                let already_live = matches!(
                    status.as_str(),
                    STATUS_PENDING | STATUS_PROCESSING | STATUS_RETRY | STATUS_COMPLETE
                );
                if same_revision && already_live {
                    false
                } else {
                    let keep_active_lease =
                        status == STATUS_PROCESSING && lease_expires_at_ms > now_ms;
                    let next_status = if keep_active_lease {
                        STATUS_PROCESSING
                    } else {
                        STATUS_PENDING
                    };
                    let next_attempts = if same_revision { attempts } else { 0 };
                    tx.execute(
                        "UPDATE rag_index_jobs
                         SET requested_revision = ?4,
                             status = ?5,
                             attempts = ?6,
                             next_attempt_at_ms = 0,
                             lease_owner = CASE WHEN ?7 THEN lease_owner ELSE NULL END,
                             lease_expires_at_ms =
                                 CASE WHEN ?7 THEN lease_expires_at_ms ELSE NULL END,
                             last_error_code = NULL,
                             updated_at_ms = ?8
                         WHERE account_id = ?1 AND workspace_id = ?2 AND session_id = ?3",
                        params![
                            account_id,
                            workspace_id,
                            session_id,
                            revision,
                            next_status,
                            next_attempts,
                            keep_active_lease,
                            now_ms
                        ],
                    )?;
                    true
                }
            }
        };
        tx.commit().context("commit RAG queue enqueue")?;
        Ok(changed)
    }

    /// Atomically claim the oldest due job for one account/workspace scope.
    pub(crate) fn claim_next(
        &self,
        scope: &RagScope,
        worker_id: &str,
        now_ms: i64,
        lease_ttl: Duration,
    ) -> Result<Option<RagIndexJob>> {
        validate_identifier("worker id", worker_id)?;
        let account_id = scope.account_id();
        let workspace_id = normalized_workspace(scope);
        let lease_ms = lease_ttl.as_millis().min(i64::MAX as u128) as i64;
        let lease_expires_at_ms = now_ms.saturating_add(lease_ms.max(1));
        let mut conn = self.connect()?;
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .context("begin RAG queue claim")?;

        // A crashed worker never strands a row. A newer requested revision
        // returns directly to pending; otherwise the same revision is retried.
        tx.execute(
            "UPDATE rag_index_jobs
             SET status = CASE
                     WHEN requested_revision != COALESCE(claimed_revision, '')
                         THEN 'pending'
                     ELSE 'retry'
                 END,
                 next_attempt_at_ms = ?1,
                 lease_owner = NULL,
                 lease_expires_at_ms = NULL,
                 last_error_code = 'lease_expired',
                 updated_at_ms = ?1
             WHERE status = 'processing'
               AND COALESCE(lease_expires_at_ms, 0) <= ?1",
            params![now_ms],
        )?;

        let candidate = tx
            .query_row(
                "SELECT session_id, requested_revision, attempts
                 FROM rag_index_jobs
                 WHERE account_id = ?1
                   AND workspace_id = ?2
                   AND status IN ('pending', 'retry')
                   AND next_attempt_at_ms <= ?3
                 ORDER BY updated_at_ms ASC, session_id ASC
                 LIMIT 1",
                params![account_id, workspace_id, now_ms],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, i64>(2)?,
                    ))
                },
            )
            .optional()?;

        let Some((session_id, requested_revision, previous_attempts)) = candidate else {
            tx.commit().context("commit empty RAG queue claim")?;
            return Ok(None);
        };
        let attempts = previous_attempts.saturating_add(1).max(1);
        let changed = tx.execute(
            "UPDATE rag_index_jobs
             SET claimed_revision = requested_revision,
                 status = ?4,
                 attempts = ?5,
                 lease_owner = ?6,
                 lease_expires_at_ms = ?7,
                 last_error_code = NULL,
                 updated_at_ms = ?8
             WHERE account_id = ?1
               AND workspace_id = ?2
               AND session_id = ?3
               AND status IN ('pending', 'retry')",
            params![
                account_id,
                workspace_id,
                session_id,
                STATUS_PROCESSING,
                attempts,
                worker_id,
                lease_expires_at_ms,
                now_ms
            ],
        )?;
        anyhow::ensure!(changed == 1, "RAG queue claim lost its candidate");
        tx.commit().context("commit RAG queue claim")?;

        Ok(Some(RagIndexJob {
            account_id: account_id.to_string(),
            workspace_id,
            session_id,
            claimed_revision: requested_revision,
            lease_owner: worker_id.to_string(),
            attempts: u32::try_from(attempts).unwrap_or(u32::MAX),
        }))
    }

    /// Mark a claimed revision complete. If a newer revision arrived while the
    /// worker was running, atomically return the row to pending instead.
    pub(crate) fn complete(&self, job: &RagIndexJob, now_ms: i64) -> Result<bool> {
        let conn = self.connect()?;
        let changed = conn.execute(
            "UPDATE rag_index_jobs
             SET status = CASE
                     WHEN requested_revision = claimed_revision
                         THEN 'complete'
                     ELSE 'pending'
                 END,
                 attempts = CASE
                     WHEN requested_revision = claimed_revision
                         THEN attempts
                     ELSE 0
                 END,
                 next_attempt_at_ms = 0,
                 lease_owner = NULL,
                 lease_expires_at_ms = NULL,
                 last_error_code = NULL,
                 updated_at_ms = ?6
             WHERE account_id = ?1
               AND workspace_id = ?2
               AND session_id = ?3
               AND status = 'processing'
               AND claimed_revision = ?4
               AND lease_owner = ?5",
            params![
                job.account_id,
                job.workspace_id,
                job.session_id,
                job.claimed_revision,
                job.lease_owner,
                now_ms
            ],
        )?;
        Ok(changed == 1)
    }

    /// Retry a claimed revision with bounded exponential backoff, or move it
    /// to dead-letter after the attempt budget. A newer requested revision
    /// always wins and becomes pending immediately.
    pub(crate) fn fail(
        &self,
        job: &RagIndexJob,
        error_code: &str,
        retryable: bool,
        now_ms: i64,
    ) -> Result<bool> {
        validate_error_code(error_code)?;
        let exhausted = job.attempts >= MAX_ATTEMPTS;
        let status = if retryable && !exhausted {
            STATUS_RETRY
        } else {
            STATUS_DEAD
        };
        let next_attempt_at_ms = if status == STATUS_RETRY {
            now_ms.saturating_add(retry_delay_ms(job.attempts))
        } else {
            0
        };
        let conn = self.connect()?;
        let changed = conn.execute(
            "UPDATE rag_index_jobs
             SET status = CASE
                     WHEN requested_revision != claimed_revision
                         THEN 'pending'
                     ELSE ?6
                 END,
                 attempts = CASE
                     WHEN requested_revision != claimed_revision
                         THEN 0
                     ELSE attempts
                 END,
                 next_attempt_at_ms = CASE
                     WHEN requested_revision != claimed_revision
                         THEN 0
                     ELSE ?7
                 END,
                 lease_owner = NULL,
                 lease_expires_at_ms = NULL,
                 last_error_code = CASE
                     WHEN requested_revision != claimed_revision
                         THEN NULL
                     ELSE ?8
                 END,
                 updated_at_ms = ?9
             WHERE account_id = ?1
               AND workspace_id = ?2
               AND session_id = ?3
               AND status = 'processing'
               AND claimed_revision = ?4
               AND lease_owner = ?5",
            params![
                job.account_id,
                job.workspace_id,
                job.session_id,
                job.claimed_revision,
                job.lease_owner,
                status,
                next_attempt_at_ms,
                error_code,
                now_ms
            ],
        )?;
        Ok(changed == 1)
    }

    /// Remove queued metadata for a deleted session in exactly one tenant
    /// scope. A session identifier alone must never authorize deleting another
    /// account or workspace's queue row.
    pub(crate) fn cancel_session(&self, scope: &RagScope, session_id: &str) -> Result<usize> {
        validate_identifier("session id", session_id)?;
        let conn = self.connect()?;
        conn.execute(
            "DELETE FROM rag_index_jobs
             WHERE account_id = ?1 AND workspace_id = ?2 AND session_id = ?3",
            params![scope.account_id(), normalized_workspace(scope), session_id],
        )
        .context("cancel RAG session jobs")
    }

    #[cfg(test)]
    fn state(&self, scope: &RagScope, session_id: &str) -> Result<Option<RagIndexQueueState>> {
        let conn = self.connect()?;
        conn.query_row(
            "SELECT requested_revision, status, attempts, next_attempt_at_ms,
                    last_error_code
             FROM rag_index_jobs
             WHERE account_id = ?1 AND workspace_id = ?2 AND session_id = ?3",
            params![scope.account_id(), normalized_workspace(scope), session_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    u32::try_from(row.get::<_, i64>(2)?).unwrap_or(u32::MAX),
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )
        .optional()
        .map_err(Into::into)
    }

    fn connect(&self) -> Result<Connection> {
        if let Some(parent) = self.path.parent() {
            cue_core::app_paths::create_private_dir(parent)?;
        }
        reject_symlink(&self.path)?;
        let conn = Connection::open(&self.path)
            .with_context(|| format!("open RAG index queue {}", self.path.display()))?;
        conn.busy_timeout(Duration::from_secs(2))?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
        harden_file_permissions(&self.path)?;
        Ok(conn)
    }
}

fn normalized_workspace(scope: &RagScope) -> String {
    scope.workspace_id().unwrap_or_default().to_string()
}

fn retry_delay_ms(attempts: u32) -> i64 {
    let exponent = attempts.saturating_sub(1).min(20);
    BASE_RETRY_DELAY_MS
        .saturating_mul(1_i64 << exponent)
        .min(MAX_RETRY_DELAY_MS)
}

fn validate_identifier(label: &str, value: &str) -> Result<()> {
    let value = value.trim();
    anyhow::ensure!(!value.is_empty(), "{label} cannot be empty");
    anyhow::ensure!(value.len() <= 512, "{label} is too long");
    Ok(())
}

fn validate_revision(revision: &str) -> Result<()> {
    anyhow::ensure!(
        revision.len() == 64 && revision.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "invalid RAG revision"
    );
    Ok(())
}

fn validate_error_code(error_code: &str) -> Result<()> {
    anyhow::ensure!(
        !error_code.is_empty()
            && error_code.len() <= 64
            && error_code
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_'),
        "invalid RAG queue error code"
    );
    Ok(())
}

fn reject_symlink(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            anyhow::ensure!(
                !metadata.file_type().is_symlink(),
                "RAG index queue path cannot be a symlink"
            );
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error).context("inspect RAG index queue path"),
    }
    Ok(())
}

fn harden_file_permissions(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))
            .context("set private RAG queue permissions")?;
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn test_queue() -> (PathBuf, RagIndexQueue) {
        let root = std::env::temp_dir().join(format!("bluey-rag-queue-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let queue = RagIndexQueue::open(root.join("jobs.db")).unwrap();
        (root, queue)
    }

    fn scope() -> RagScope {
        RagScope::new("account-a", Some("workspace-a")).unwrap()
    }

    fn revision(ch: char) -> String {
        std::iter::repeat_n(ch, 64).collect()
    }

    #[test]
    fn enqueue_is_idempotent_for_the_same_revision() {
        let (root, queue) = test_queue();
        let scope = scope();
        assert!(queue
            .enqueue(&scope, "session-a", &revision('a'), 100)
            .unwrap());
        assert!(!queue
            .enqueue(&scope, "session-a", &revision('a'), 101)
            .unwrap());
        assert_eq!(
            queue.state(&scope, "session-a").unwrap().unwrap(),
            (revision('a'), STATUS_PENDING.into(), 0, 0, None)
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn a_new_revision_arriving_during_work_is_not_lost() {
        let (root, queue) = test_queue();
        let scope = scope();
        queue
            .enqueue(&scope, "session-a", &revision('a'), 100)
            .unwrap();
        let job = queue
            .claim_next(&scope, "worker-a", 101, Duration::from_secs(30))
            .unwrap()
            .unwrap();
        assert!(queue
            .enqueue(&scope, "session-a", &revision('b'), 102)
            .unwrap());
        assert!(queue.complete(&job, 103).unwrap());
        assert_eq!(
            queue.state(&scope, "session-a").unwrap().unwrap(),
            (revision('b'), STATUS_PENDING.into(), 0, 0, None)
        );
        let next = queue
            .claim_next(&scope, "worker-a", 104, Duration::from_secs(30))
            .unwrap()
            .unwrap();
        assert_eq!(next.claimed_revision, revision('b'));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn expired_lease_is_recovered_and_retried() {
        let (root, queue) = test_queue();
        let scope = scope();
        queue
            .enqueue(&scope, "session-a", &revision('a'), 100)
            .unwrap();
        let first = queue
            .claim_next(&scope, "worker-a", 101, Duration::from_millis(5))
            .unwrap()
            .unwrap();
        assert_eq!(first.attempts, 1);
        let second = queue
            .claim_next(&scope, "worker-b", 107, Duration::from_secs(30))
            .unwrap()
            .unwrap();
        assert_eq!(second.attempts, 2);
        assert_eq!(second.claimed_revision, revision('a'));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn failures_back_off_then_dead_letter() {
        let (root, queue) = test_queue();
        let scope = scope();
        queue
            .enqueue(&scope, "session-a", &revision('a'), 100)
            .unwrap();
        for attempt in 1..=MAX_ATTEMPTS {
            let now = 100 + i64::from(attempt) * MAX_RETRY_DELAY_MS;
            let job = queue
                .claim_next(&scope, "worker-a", now, Duration::from_secs(30))
                .unwrap()
                .unwrap();
            assert_eq!(job.attempts, attempt);
            queue.fail(&job, "embedding_failed", true, now + 1).unwrap();
        }
        let state = queue.state(&scope, "session-a").unwrap().unwrap();
        assert_eq!(state.1, STATUS_DEAD);
        assert_eq!(state.2, MAX_ATTEMPTS);
        assert_eq!(state.4.as_deref(), Some("embedding_failed"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn cancellation_removes_metadata_without_content_payloads() {
        let (root, queue) = test_queue();
        let scope = scope();
        queue
            .enqueue(&scope, "session-a", &revision('a'), 100)
            .unwrap();
        assert_eq!(queue.cancel_session(&scope, "session-a").unwrap(), 1);
        assert!(queue.state(&scope, "session-a").unwrap().is_none());

        let db_bytes = fs::read(root.join("jobs.db")).unwrap();
        let db_text = String::from_utf8_lossy(&db_bytes);
        assert!(!db_text.contains("private transcript canary"));
        assert!(!db_text.contains("document body canary"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn cancellation_is_strictly_tenant_scoped() {
        let (root, queue) = test_queue();
        let scope_a = scope();
        let scope_b = RagScope::new("account-b", Some("workspace-b")).unwrap();
        queue
            .enqueue(&scope_a, "shared-session", &revision('a'), 100)
            .unwrap();
        queue
            .enqueue(&scope_b, "shared-session", &revision('b'), 100)
            .unwrap();

        assert_eq!(queue.cancel_session(&scope_a, "shared-session").unwrap(), 1);
        assert!(queue.state(&scope_a, "shared-session").unwrap().is_none());
        assert_eq!(
            queue.state(&scope_b, "shared-session").unwrap().unwrap().0,
            revision('b')
        );
        let _ = fs::remove_dir_all(root);
    }
}
