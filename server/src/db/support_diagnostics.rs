//! Server-owned consent authority for metadata-only support diagnostics.
//!
//! Consent is an append-only account-scoped ledger. Revocation is recorded in
//! the same transaction that fences every pending/ready support object for
//! durable deletion, so a racing upload cannot outlive the revocation.

use anyhow::{Context, Result};
use postgres::Transaction as PgTransaction;
use rusqlite::{params, OptionalExtension, Transaction as SqliteTransaction, TransactionBehavior};
use serde::Serialize;

use super::{object_uploads, DbPool};

pub const SUPPORT_DIAGNOSTIC_POLICY_VERSION: &str = "2026-08-30";
pub const SUPPORT_DIAGNOSTIC_CONTENT_POLICY: &str = "metadata_only";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SupportDiagnosticConsentReceipt {
    pub receipt_id: String,
    pub enabled: bool,
    pub policy_version: String,
    pub content_policy: String,
    pub revision: i64,
    pub recorded_at_ms: i64,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SupportDiagnosticConsentError {
    #[error("support diagnostics policy version is not supported")]
    UnsupportedPolicyVersion,
    #[error("support diagnostics content policy is not supported")]
    UnsupportedContentPolicy,
    #[error("account not found")]
    AccountNotFound,
}

pub fn current_consent(
    pool: &DbPool,
    account_id: &str,
) -> Result<Option<SupportDiagnosticConsentReceipt>> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => current_consent_sqlite(pool, account_id),
        DbPool::Postgres(_) => current_consent_postgres(pool, account_id),
    })
}

pub fn consent_is_active(pool: &DbPool, account_id: &str) -> Result<bool> {
    Ok(current_consent(pool, account_id)?.is_some_and(|receipt| {
        receipt.enabled
            && receipt.policy_version == SUPPORT_DIAGNOSTIC_POLICY_VERSION
            && receipt.content_policy == SUPPORT_DIAGNOSTIC_CONTENT_POLICY
    }))
}

pub fn consent_history(
    pool: &DbPool,
    account_id: &str,
) -> Result<Vec<SupportDiagnosticConsentReceipt>> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => consent_history_sqlite(pool, account_id),
        DbPool::Postgres(_) => consent_history_postgres(pool, account_id),
    })
}

pub fn record_consent(
    pool: &DbPool,
    account_id: &str,
    enabled: bool,
    policy_version: &str,
    content_policy: &str,
    recorded_at_ms: i64,
) -> Result<SupportDiagnosticConsentReceipt> {
    validate_policy(policy_version, content_policy)?;
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => record_consent_sqlite(
            pool,
            account_id,
            enabled,
            policy_version,
            content_policy,
            recorded_at_ms,
        ),
        DbPool::Postgres(_) => record_consent_postgres(
            pool,
            account_id,
            enabled,
            policy_version,
            content_policy,
            recorded_at_ms,
        ),
    })
}

pub(crate) fn consent_is_active_sqlite_tx(
    tx: &SqliteTransaction<'_>,
    account_id: &str,
) -> Result<bool> {
    Ok(tx
        .query_row(
            "SELECT action, policy_version, content_policy
               FROM support_diagnostic_consent_events
              WHERE account_id = ?1
              ORDER BY revision DESC
              LIMIT 1",
            params![account_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            },
        )
        .optional()?
        .is_some_and(|(action, policy_version, content_policy)| {
            action == "granted"
                && policy_version == SUPPORT_DIAGNOSTIC_POLICY_VERSION
                && content_policy == SUPPORT_DIAGNOSTIC_CONTENT_POLICY
        }))
}

pub(crate) fn consent_is_active_postgres_tx(
    tx: &mut PgTransaction<'_>,
    account_id: &str,
) -> Result<bool> {
    Ok(tx
        .query_opt(
            "SELECT action, policy_version, content_policy
               FROM support_diagnostic_consent_events
              WHERE account_id = $1
              ORDER BY revision DESC
              LIMIT 1",
            &[&account_id],
        )?
        .is_some_and(|row| {
            row.get::<_, String>(0) == "granted"
                && row.get::<_, String>(1) == SUPPORT_DIAGNOSTIC_POLICY_VERSION
                && row.get::<_, String>(2) == SUPPORT_DIAGNOSTIC_CONTENT_POLICY
        }))
}

/// Append a revocation receipt when the current account state is not already
/// revoked. Callers hold the same write transaction used to fence objects, so
/// DELETE-all cannot acknowledge cleanup while upload authority remains live.
pub(crate) fn ensure_revoked_sqlite_tx(
    tx: &SqliteTransaction<'_>,
    account_id: &str,
    recorded_at_ms: i64,
) -> Result<()> {
    let account_exists = tx
        .query_row(
            "SELECT 1 FROM accounts WHERE id = ?1",
            params![account_id],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    if !account_exists {
        return Err(SupportDiagnosticConsentError::AccountNotFound.into());
    }
    let current_action = tx
        .query_row(
            "SELECT action
               FROM support_diagnostic_consent_events
              WHERE account_id = ?1
              ORDER BY revision DESC
              LIMIT 1",
            params![account_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    if current_action.as_deref() == Some("revoked") {
        return Ok(());
    }
    let revision = tx.query_row(
        "SELECT COALESCE(MAX(revision), 0) + 1
           FROM support_diagnostic_consent_events
          WHERE account_id = ?1",
        params![account_id],
        |row| row.get::<_, i64>(0),
    )?;
    tx.execute(
        "INSERT INTO support_diagnostic_consent_events (
            receipt_id, account_id, revision, action, policy_version,
            content_policy, recorded_at_ms
         ) VALUES (?1, ?2, ?3, 'revoked', ?4, ?5, ?6)",
        params![
            uuid::Uuid::new_v4().hyphenated().to_string(),
            account_id,
            revision,
            SUPPORT_DIAGNOSTIC_POLICY_VERSION,
            SUPPORT_DIAGNOSTIC_CONTENT_POLICY,
            recorded_at_ms,
        ],
    )?;
    Ok(())
}

pub(crate) fn ensure_revoked_postgres_tx(
    tx: &mut PgTransaction<'_>,
    account_id: &str,
    recorded_at_ms: i64,
) -> Result<()> {
    let account_exists = tx
        .query_opt(
            "SELECT 1 FROM accounts WHERE id = $1 FOR UPDATE",
            &[&account_id],
        )?
        .is_some();
    if !account_exists {
        return Err(SupportDiagnosticConsentError::AccountNotFound.into());
    }
    let current_action = tx
        .query_opt(
            "SELECT action
               FROM support_diagnostic_consent_events
              WHERE account_id = $1
              ORDER BY revision DESC
              LIMIT 1",
            &[&account_id],
        )?
        .map(|row| row.get::<_, String>(0));
    if current_action.as_deref() == Some("revoked") {
        return Ok(());
    }
    let revision: i64 = tx
        .query_one(
            "SELECT COALESCE(MAX(revision), 0) + 1
               FROM support_diagnostic_consent_events
              WHERE account_id = $1",
            &[&account_id],
        )?
        .get(0);
    let receipt_id = uuid::Uuid::new_v4().hyphenated().to_string();
    tx.execute(
        "INSERT INTO support_diagnostic_consent_events (
            receipt_id, account_id, revision, action, policy_version,
            content_policy, recorded_at_ms
        ) VALUES ($1, $2, $3, 'revoked', $4, $5, $6)",
        &[
            &receipt_id,
            &account_id,
            &revision,
            &SUPPORT_DIAGNOSTIC_POLICY_VERSION,
            &SUPPORT_DIAGNOSTIC_CONTENT_POLICY,
            &recorded_at_ms,
        ],
    )?;
    Ok(())
}

fn validate_policy(policy_version: &str, content_policy: &str) -> Result<()> {
    if policy_version != SUPPORT_DIAGNOSTIC_POLICY_VERSION {
        return Err(SupportDiagnosticConsentError::UnsupportedPolicyVersion.into());
    }
    if content_policy != SUPPORT_DIAGNOSTIC_CONTENT_POLICY {
        return Err(SupportDiagnosticConsentError::UnsupportedContentPolicy.into());
    }
    Ok(())
}

fn current_consent_sqlite(
    pool: &DbPool,
    account_id: &str,
) -> Result<Option<SupportDiagnosticConsentReceipt>> {
    let conn = pool.get()?;
    conn.query_row(
        "SELECT receipt_id, action, policy_version, content_policy, revision, recorded_at_ms
           FROM support_diagnostic_consent_events
          WHERE account_id = ?1
          ORDER BY revision DESC
          LIMIT 1",
        params![account_id],
        receipt_from_sqlite_row,
    )
    .optional()
    .map_err(Into::into)
}

fn current_consent_postgres(
    pool: &DbPool,
    account_id: &str,
) -> Result<Option<SupportDiagnosticConsentReceipt>> {
    let mut conn = pool.get_pg()?;
    conn.query_opt(
        "SELECT receipt_id, action, policy_version, content_policy, revision, recorded_at_ms
           FROM support_diagnostic_consent_events
          WHERE account_id = $1
          ORDER BY revision DESC
          LIMIT 1",
        &[&account_id],
    )?
    .map(receipt_from_postgres_row)
    .transpose()
}

fn consent_history_sqlite(
    pool: &DbPool,
    account_id: &str,
) -> Result<Vec<SupportDiagnosticConsentReceipt>> {
    let conn = pool.get()?;
    let mut stmt = conn.prepare(
        "SELECT receipt_id, action, policy_version, content_policy, revision, recorded_at_ms
           FROM support_diagnostic_consent_events
          WHERE account_id = ?1
          ORDER BY revision",
    )?;
    let rows = stmt.query_map(params![account_id], receipt_from_sqlite_row)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

fn consent_history_postgres(
    pool: &DbPool,
    account_id: &str,
) -> Result<Vec<SupportDiagnosticConsentReceipt>> {
    let mut conn = pool.get_pg()?;
    conn.query(
        "SELECT receipt_id, action, policy_version, content_policy, revision, recorded_at_ms
           FROM support_diagnostic_consent_events
          WHERE account_id = $1
          ORDER BY revision",
        &[&account_id],
    )?
    .into_iter()
    .map(receipt_from_postgres_row)
    .collect()
}

fn record_consent_sqlite(
    pool: &DbPool,
    account_id: &str,
    enabled: bool,
    policy_version: &str,
    content_policy: &str,
    recorded_at_ms: i64,
) -> Result<SupportDiagnosticConsentReceipt> {
    let mut conn = pool
        .get()
        .context("get support consent sqlite connection")?;
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .context("begin support consent sqlite transaction")?;
    let account_state = tx
        .query_row(
            "SELECT deletion_pending_at_ms FROM accounts WHERE id = ?1",
            params![account_id],
            |row| row.get::<_, Option<i64>>(0),
        )
        .optional()?;
    if !matches!(account_state, Some(None)) {
        return Err(SupportDiagnosticConsentError::AccountNotFound.into());
    }
    let revision = tx.query_row(
        "SELECT COALESCE(MAX(revision), 0) + 1
           FROM support_diagnostic_consent_events
          WHERE account_id = ?1",
        params![account_id],
        |row| row.get::<_, i64>(0),
    )?;
    let receipt = new_receipt(
        enabled,
        policy_version,
        content_policy,
        revision,
        recorded_at_ms,
    );
    tx.execute(
        "INSERT INTO support_diagnostic_consent_events (
            receipt_id, account_id, revision, action, policy_version,
            content_policy, recorded_at_ms
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            &receipt.receipt_id,
            account_id,
            receipt.revision,
            consent_action(enabled),
            &receipt.policy_version,
            &receipt.content_policy,
            receipt.recorded_at_ms,
        ],
    )?;
    if !enabled {
        object_uploads::schedule_support_diagnostic_cleanup_sqlite_tx(
            &tx,
            account_id,
            None,
            recorded_at_ms,
        )?;
    }
    tx.commit()?;
    Ok(receipt)
}

fn record_consent_postgres(
    pool: &DbPool,
    account_id: &str,
    enabled: bool,
    policy_version: &str,
    content_policy: &str,
    recorded_at_ms: i64,
) -> Result<SupportDiagnosticConsentReceipt> {
    let mut conn = pool
        .get_pg()
        .context("get support consent postgres connection")?;
    let mut tx = conn
        .transaction()
        .context("begin support consent postgres transaction")?;
    let account_state = tx.query_opt(
        "SELECT deletion_pending_at_ms FROM accounts WHERE id = $1 FOR UPDATE",
        &[&account_id],
    )?;
    if !account_state.is_some_and(|row| row.get::<_, Option<i64>>(0).is_none()) {
        return Err(SupportDiagnosticConsentError::AccountNotFound.into());
    }
    let revision: i64 = tx
        .query_one(
            "SELECT COALESCE(MAX(revision), 0) + 1
               FROM support_diagnostic_consent_events
              WHERE account_id = $1",
            &[&account_id],
        )?
        .get(0);
    let receipt = new_receipt(
        enabled,
        policy_version,
        content_policy,
        revision,
        recorded_at_ms,
    );
    tx.execute(
        "INSERT INTO support_diagnostic_consent_events (
            receipt_id, account_id, revision, action, policy_version,
            content_policy, recorded_at_ms
         ) VALUES ($1, $2, $3, $4, $5, $6, $7)",
        &[
            &receipt.receipt_id,
            &account_id,
            &receipt.revision,
            &consent_action(enabled),
            &receipt.policy_version,
            &receipt.content_policy,
            &receipt.recorded_at_ms,
        ],
    )?;
    if !enabled {
        object_uploads::schedule_support_diagnostic_cleanup_postgres_tx(
            &mut tx,
            account_id,
            None,
            recorded_at_ms,
        )?;
    }
    tx.commit()?;
    Ok(receipt)
}

fn new_receipt(
    enabled: bool,
    policy_version: &str,
    content_policy: &str,
    revision: i64,
    recorded_at_ms: i64,
) -> SupportDiagnosticConsentReceipt {
    SupportDiagnosticConsentReceipt {
        receipt_id: uuid::Uuid::new_v4().hyphenated().to_string(),
        enabled,
        policy_version: policy_version.to_string(),
        content_policy: content_policy.to_string(),
        revision,
        recorded_at_ms,
    }
}

fn consent_action(enabled: bool) -> &'static str {
    if enabled {
        "granted"
    } else {
        "revoked"
    }
}

fn receipt_from_sqlite_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<SupportDiagnosticConsentReceipt> {
    Ok(SupportDiagnosticConsentReceipt {
        receipt_id: row.get(0)?,
        enabled: row.get::<_, String>(1)? == "granted",
        policy_version: row.get(2)?,
        content_policy: row.get(3)?,
        revision: row.get(4)?,
        recorded_at_ms: row.get(5)?,
    })
}

fn receipt_from_postgres_row(row: postgres::Row) -> Result<SupportDiagnosticConsentReceipt> {
    Ok(SupportDiagnosticConsentReceipt {
        receipt_id: row.try_get(0)?,
        enabled: row.try_get::<_, String>(1)? == "granted",
        policy_version: row.try_get(2)?,
        content_policy: row.try_get(3)?,
        revision: row.try_get(4)?,
        recorded_at_ms: row.try_get(5)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_pool() -> DbPool {
        let pool = crate::db::open_pool(":memory:".as_ref()).unwrap();
        crate::db::run_migrations(&pool).unwrap();
        pool.get()
            .unwrap()
            .execute(
                "INSERT INTO accounts(id, email, password_hash)
                 VALUES ('acct_support', 'support@example.test', 'hash')",
                [],
            )
            .unwrap();
        pool
    }

    #[test]
    fn latest_append_only_receipt_controls_active_consent() {
        let pool = test_pool();
        assert!(!consent_is_active(&pool, "acct_support").unwrap());
        assert!(consent_history(&pool, "acct_support").unwrap().is_empty());

        let granted = record_consent(
            &pool,
            "acct_support",
            true,
            SUPPORT_DIAGNOSTIC_POLICY_VERSION,
            SUPPORT_DIAGNOSTIC_CONTENT_POLICY,
            100,
        )
        .unwrap();
        assert_eq!(granted.revision, 1);
        assert!(consent_is_active(&pool, "acct_support").unwrap());

        let revoked = record_consent(
            &pool,
            "acct_support",
            false,
            SUPPORT_DIAGNOSTIC_POLICY_VERSION,
            SUPPORT_DIAGNOSTIC_CONTENT_POLICY,
            101,
        )
        .unwrap();
        assert_eq!(revoked.revision, 2);
        assert!(!consent_is_active(&pool, "acct_support").unwrap());
        assert_eq!(consent_history(&pool, "acct_support").unwrap().len(), 2);

        let count: i64 = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM support_diagnostic_consent_events
                  WHERE account_id = 'acct_support'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 2, "revocation must preserve the grant receipt");
    }

    #[test]
    fn unsupported_policy_cannot_be_recorded() {
        let pool = test_pool();
        let error = record_consent(
            &pool,
            "acct_support",
            true,
            "legacy",
            SUPPORT_DIAGNOSTIC_CONTENT_POLICY,
            100,
        )
        .unwrap_err();
        assert_eq!(
            error.downcast_ref::<SupportDiagnosticConsentError>(),
            Some(&SupportDiagnosticConsentError::UnsupportedPolicyVersion)
        );
    }

    #[test]
    fn deletion_pending_account_cannot_regrant_support_consent() {
        let pool = test_pool();
        assert!(
            crate::db::account_data::begin_account_deletion(&pool, "acct_support", 100).unwrap()
        );

        let error = record_consent(
            &pool,
            "acct_support",
            true,
            SUPPORT_DIAGNOSTIC_POLICY_VERSION,
            SUPPORT_DIAGNOSTIC_CONTENT_POLICY,
            101,
        )
        .unwrap_err();
        assert_eq!(
            error.downcast_ref::<SupportDiagnosticConsentError>(),
            Some(&SupportDiagnosticConsentError::AccountNotFound)
        );
    }

    #[test]
    fn legacy_receipt_is_not_active_for_the_current_policy() {
        let pool = test_pool();
        pool.get()
            .unwrap()
            .execute(
                "INSERT INTO support_diagnostic_consent_events (
                    receipt_id, account_id, revision, action, policy_version,
                    content_policy, recorded_at_ms
                 ) VALUES ('legacy-receipt', 'acct_support', 1, 'granted',
                           'legacy', 'metadata_only', 100)",
                [],
            )
            .unwrap();

        assert!(!consent_is_active(&pool, "acct_support").unwrap());
        let mut conn = pool.get().unwrap();
        let tx = conn.transaction().unwrap();
        assert!(!consent_is_active_sqlite_tx(&tx, "acct_support").unwrap());
    }
}
