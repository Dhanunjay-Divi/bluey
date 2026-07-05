//! Atomic balance + per-batch credit accounting (FIFO).

use anyhow::Result;
use chrono::{Duration, Utc};
use postgres::Transaction as PgTransaction;
use rusqlite::{params, Transaction as SqliteTransaction};

use crate::db::DbPool;

/// Reloaded account credits are valid for up to 12 months from purchase.
///
/// The implementation uses 365 days so every credit batch has a deterministic
/// expiry timestamp and FIFO consumption can compare timestamps directly.
pub const CREDIT_VALIDITY_DAYS: i64 = 365;

#[derive(Debug, Clone, Copy)]
pub(crate) struct BalanceLedgerEntry<'a> {
    pub account_id: &'a str,
    pub event_type: &'a str,
    pub amount_cents: i64,
    pub balance_cents_before: i64,
    pub balance_cents_after: i64,
    pub reason: Option<&'a str>,
    pub provider: Option<&'a str>,
    pub processor_payment_id: Option<&'a str>,
    pub source_id: Option<&'a str>,
    pub idempotency_key: Option<&'a str>,
    pub request_id: Option<&'a str>,
    pub metadata_json: Option<&'a str>,
}

pub(crate) fn insert_balance_ledger_sqlite_tx(
    tx: &SqliteTransaction<'_>,
    entry: BalanceLedgerEntry<'_>,
) -> Result<()> {
    tx.execute(
        "INSERT INTO balance_ledger_entries
            (id, account_id, event_type, amount_cents, balance_cents_before,
             balance_cents_after, reason, provider, processor_payment_id,
             source_id, idempotency_key, request_id, metadata_json)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
        params![
            uuid::Uuid::new_v4().to_string(),
            entry.account_id,
            entry.event_type,
            entry.amount_cents,
            entry.balance_cents_before,
            entry.balance_cents_after,
            entry.reason,
            entry.provider,
            entry.processor_payment_id,
            entry.source_id,
            entry.idempotency_key,
            entry.request_id,
            entry.metadata_json.unwrap_or("{}"),
        ],
    )?;
    Ok(())
}

pub(crate) fn insert_balance_ledger_pg_tx(
    tx: &mut PgTransaction<'_>,
    entry: BalanceLedgerEntry<'_>,
) -> Result<()> {
    tx.execute(
        "INSERT INTO balance_ledger_entries
            (id, account_id, event_type, amount_cents, balance_cents_before,
             balance_cents_after, reason, provider, processor_payment_id,
             source_id, idempotency_key, request_id, metadata_json)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)",
        &[
            &uuid::Uuid::new_v4().to_string(),
            &entry.account_id,
            &entry.event_type,
            &entry.amount_cents,
            &entry.balance_cents_before,
            &entry.balance_cents_after,
            &entry.reason,
            &entry.provider,
            &entry.processor_payment_id,
            &entry.source_id,
            &entry.idempotency_key,
            &entry.request_id,
            &entry.metadata_json.unwrap_or("{}"),
        ],
    )?;
    Ok(())
}

/// Atomically deduct `cost_cents` from the account's balance AND from
/// the oldest non-expired credit batch (FIFO consumption).
///
/// Returns Ok(true) on success, Ok(false) on insufficient balance or
/// race-loss to a concurrent deduction.
pub fn deduct(pool: &DbPool, account_id: &str, cost_cents: i64) -> Result<bool> {
    deduct_with_evidence(
        pool,
        account_id,
        cost_cents,
        "usage_deduction",
        Some("paid_usage"),
        None,
    )
}

pub fn deduct_for_request(
    pool: &DbPool,
    account_id: &str,
    cost_cents: i64,
    reason: &str,
    request_id: &str,
) -> Result<bool> {
    deduct_with_evidence(
        pool,
        account_id,
        cost_cents,
        "usage_deduction",
        Some(reason),
        Some(request_id),
    )
}

fn deduct_with_evidence(
    pool: &DbPool,
    account_id: &str,
    cost_cents: i64,
    event_type: &str,
    reason: Option<&str>,
    request_id: Option<&str>,
) -> Result<bool> {
    crate::db::run_blocking_db(|| {
        if cost_cents < 0 {
            anyhow::bail!("cost_cents must be non-negative");
        }
        if cost_cents == 0 {
            return Ok(true);
        }
        match pool {
            DbPool::Sqlite(_) => {
                let mut conn = pool.get()?;
                let tx = conn.transaction()?;
                let balance_before: i64 = tx.query_row(
                    "SELECT balance_cents FROM accounts WHERE id = ?1",
                    params![account_id],
                    |r| r.get(0),
                )?;

                // Atomic balance check + deduction.
                let updated = tx.execute(
                    "UPDATE accounts SET balance_cents = balance_cents - ?1
                 WHERE id = ?2 AND balance_cents >= ?1",
                    params![cost_cents, account_id],
                )?;
                if updated == 0 {
                    return Ok(false);
                }

                consume_credit_batches_tx(&tx, account_id, cost_cents)?;
                insert_balance_ledger_sqlite_tx(
                    &tx,
                    BalanceLedgerEntry {
                        account_id,
                        event_type,
                        amount_cents: -cost_cents,
                        balance_cents_before: balance_before,
                        balance_cents_after: balance_before - cost_cents,
                        reason,
                        provider: None,
                        processor_payment_id: None,
                        source_id: None,
                        idempotency_key: request_id,
                        request_id,
                        metadata_json: None,
                    },
                )?;

                tx.commit()?;
                Ok(true)
            }
            DbPool::Postgres(_) => {
                let mut conn = pool.get_pg()?;
                let mut tx = conn.transaction()?;
                let balance_before: i64 = tx
                    .query_one(
                        "SELECT balance_cents FROM accounts WHERE id = $1",
                        &[&account_id],
                    )?
                    .try_get(0)?;

                let updated = tx.execute(
                    "UPDATE accounts SET balance_cents = balance_cents - $1
                 WHERE id = $2 AND balance_cents >= $1",
                    &[&cost_cents, &account_id],
                )?;
                if updated == 0 {
                    tx.rollback()?;
                    return Ok(false);
                }

                consume_credit_batches_pg_tx(&mut tx, account_id, cost_cents)?;
                insert_balance_ledger_pg_tx(
                    &mut tx,
                    BalanceLedgerEntry {
                        account_id,
                        event_type,
                        amount_cents: -cost_cents,
                        balance_cents_before: balance_before,
                        balance_cents_after: balance_before - cost_cents,
                        reason,
                        provider: None,
                        processor_payment_id: None,
                        source_id: None,
                        idempotency_key: request_id,
                        request_id,
                        metadata_json: None,
                    },
                )?;

                tx.commit()?;
                Ok(true)
            }
        }
    })
}

pub(crate) fn consume_credit_batches_tx(
    tx: &SqliteTransaction<'_>,
    account_id: &str,
    cost_cents: i64,
) -> Result<()> {
    if cost_cents < 0 {
        anyhow::bail!("cost_cents must be non-negative");
    }
    let now = Utc::now().to_rfc3339();
    let mut remaining = cost_cents;
    while remaining > 0 {
        let row: Option<(String, i64)> = tx
            .query_row(
                "SELECT id, remaining_cents FROM credit_batches
                 WHERE account_id = ?1 AND remaining_cents > 0
                   AND expires_at > ?2 AND expired_at IS NULL
                 ORDER BY purchased_at ASC LIMIT 1",
                params![account_id, &now],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .ok();
        let Some((batch_id, batch_remaining)) = row else {
            break;
        };
        let take = remaining.min(batch_remaining);
        tx.execute(
            "UPDATE credit_batches SET remaining_cents = remaining_cents - ?1
             WHERE id = ?2",
            params![take, batch_id],
        )?;
        remaining -= take;
    }
    // If somehow no credit batch existed (shouldn't happen given the entry
    // balance check passed) the account balance remains the canonical source
    // of truth; there is no best-effort way to reconstruct an expired batch.
    Ok(())
}

pub(crate) fn consume_credit_batches_pg_tx(
    tx: &mut PgTransaction<'_>,
    account_id: &str,
    cost_cents: i64,
) -> Result<()> {
    if cost_cents < 0 {
        anyhow::bail!("cost_cents must be non-negative");
    }
    let mut remaining = cost_cents;
    while remaining > 0 {
        let row = tx.query_opt(
            "SELECT id, remaining_cents FROM credit_batches
             WHERE account_id = $1 AND remaining_cents > 0
               AND expires_at > now() AND expired_at IS NULL
             ORDER BY purchased_at ASC LIMIT 1",
            &[&account_id],
        )?;
        let Some(row) = row else {
            break;
        };
        let batch_id: String = row.try_get(0)?;
        let batch_remaining: i64 = row.try_get(1)?;
        let take = remaining.min(batch_remaining);
        tx.execute(
            "UPDATE credit_batches SET remaining_cents = remaining_cents - $1
             WHERE id = $2",
            &[&take, &batch_id],
        )?;
        remaining -= take;
    }
    Ok(())
}

fn credit_with_source_id(
    pool: &DbPool,
    account_id: &str,
    amount_cents: i64,
    credit_source_id: &str,
    event_type: &str,
    reason: Option<&str>,
    provider: Option<&str>,
    processor_payment_id: Option<&str>,
) -> Result<bool> {
    if amount_cents <= 0 {
        anyhow::bail!("amount_cents must be positive");
    }
    let credit_source_id = credit_source_id.trim();
    if credit_source_id.is_empty() {
        anyhow::bail!("credit_source_id must be non-empty");
    }
    match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction()?;
            let balance_before: i64 = tx.query_row(
                "SELECT balance_cents FROM accounts WHERE id = ?1",
                params![account_id],
                |r| r.get(0),
            )?;

            // Codex Stage 6 S6.1 + S6.2: dedupe-then-credit atomically.
            // An existing credit_batches row with the same source id means we
            // already credited this payment/internal grant.
            // Return Ok(false) to signal "no-op already processed" so the
            // webhook handler can mark the event processed without re-running
            // anything else. The whole INSERT+UPDATE pair is wrapped in a
            // transaction so a process crash between the two cannot leave the
            // account ledger inconsistent.
            let existing: Option<i64> = tx
                .query_row(
                    "SELECT 1 FROM credit_batches WHERE stripe_charge_id = ?1",
                    params![credit_source_id],
                    |r| r.get(0),
                )
                .ok();
            if existing.is_some() {
                tx.commit()?;
                return Ok(false);
            }

            let batch_id = uuid::Uuid::new_v4().to_string();
            let expires_at = (Utc::now() + Duration::days(CREDIT_VALIDITY_DAYS)).to_rfc3339();

            tx.execute(
                "INSERT INTO credit_batches
                    (id, account_id, amount_cents, remaining_cents, expires_at, stripe_charge_id)
                 VALUES (?1, ?2, ?3, ?3, ?4, ?5)",
                params![
                    batch_id,
                    account_id,
                    amount_cents,
                    expires_at,
                    credit_source_id
                ],
            )?;

            tx.execute(
                "UPDATE accounts SET balance_cents = balance_cents + ?1 WHERE id = ?2",
                params![amount_cents, account_id],
            )?;
            insert_balance_ledger_sqlite_tx(
                &tx,
                BalanceLedgerEntry {
                    account_id,
                    event_type,
                    amount_cents,
                    balance_cents_before: balance_before,
                    balance_cents_after: balance_before + amount_cents,
                    reason,
                    provider,
                    processor_payment_id,
                    source_id: Some(credit_source_id),
                    idempotency_key: Some(credit_source_id),
                    request_id: None,
                    metadata_json: None,
                },
            )?;

            tx.commit()?;
            Ok(true)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            let balance_before: i64 = tx
                .query_one(
                    "SELECT balance_cents FROM accounts WHERE id = $1",
                    &[&account_id],
                )?
                .try_get(0)?;

            let existing = tx
                .query_opt(
                    "SELECT 1 FROM credit_batches WHERE stripe_charge_id = $1",
                    &[&credit_source_id],
                )?
                .is_some();
            if existing {
                tx.commit()?;
                return Ok(false);
            }

            let batch_id = uuid::Uuid::new_v4().to_string();
            let expires_at = Utc::now() + Duration::days(CREDIT_VALIDITY_DAYS);

            tx.execute(
                "INSERT INTO credit_batches
                    (id, account_id, amount_cents, remaining_cents, expires_at, stripe_charge_id)
                 VALUES ($1, $2, $3, $3, $4, $5)",
                &[
                    &batch_id,
                    &account_id,
                    &amount_cents,
                    &expires_at,
                    &credit_source_id,
                ],
            )?;

            tx.execute(
                "UPDATE accounts SET balance_cents = balance_cents + $1 WHERE id = $2",
                &[&amount_cents, &account_id],
            )?;
            insert_balance_ledger_pg_tx(
                &mut tx,
                BalanceLedgerEntry {
                    account_id,
                    event_type,
                    amount_cents,
                    balance_cents_before: balance_before,
                    balance_cents_after: balance_before + amount_cents,
                    reason,
                    provider,
                    processor_payment_id,
                    source_id: Some(credit_source_id),
                    idempotency_key: Some(credit_source_id),
                    request_id: None,
                    metadata_json: None,
                },
            )?;

            tx.commit()?;
            Ok(true)
        }
    }
}

/// Credit spendable balance from a payment processor event.
///
/// This is intentionally explicit: customer spendable credits must come
/// from a processor-confirmed payment id, not from checkout/link setup.
/// The backing DB column is still named `stripe_charge_id` for migration
/// compatibility, but stores namespaced ids such as `square:payment_123`.
pub fn credit_processor_payment(
    pool: &DbPool,
    account_id: &str,
    amount_cents: i64,
    provider: &str,
    processor_payment_id: &str,
) -> Result<bool> {
    crate::db::run_blocking_db(|| {
        let provider = provider.trim().to_ascii_lowercase();
        let processor_payment_id = processor_payment_id.trim();
        if provider.is_empty() || processor_payment_id.is_empty() {
            anyhow::bail!("processor credit requires provider and payment id");
        }
        let source_id = format!("{provider}:{processor_payment_id}");
        credit_with_source_id(
            pool,
            account_id,
            amount_cents,
            &source_id,
            "processor_payment_credit",
            Some("processor_confirmed_payment"),
            Some(&provider),
            Some(processor_payment_id),
        )
    })
}

/// Credit spendable balance from an explicit internal operator action.
///
/// Use this for tests/admin grants only. External payment flows should
/// call `credit_processor_payment` after the processor confirms payment.
pub fn credit_internal(
    pool: &DbPool,
    account_id: &str,
    amount_cents: i64,
    reason: &str,
) -> Result<bool> {
    crate::db::run_blocking_db(|| {
        let reason = reason.trim();
        if reason.is_empty() {
            anyhow::bail!("internal credit requires a reason");
        }
        let source_id = format!("internal:{reason}:{}", uuid::Uuid::new_v4());
        credit_with_source_id(
            pool,
            account_id,
            amount_cents,
            &source_id,
            "internal_credit",
            Some(reason),
            None,
            None,
        )
    })
}

/// Remove any unused spendable credit from a processor payment.
///
/// Refunds/disputes must not leave remaining processor-backed credits usable.
/// Already-consumed credits are intentionally not reconstructed here; the
/// billing risk lock stops further paid spend and preserves the evidence trail.
pub fn revoke_processor_credit(
    pool: &DbPool,
    provider: &str,
    processor_payment_id: &str,
    reason: &str,
) -> Result<Option<(String, i64)>> {
    crate::db::run_blocking_db(|| {
        let provider = provider.trim().to_ascii_lowercase();
        let processor_payment_id = processor_payment_id.trim();
        if provider.is_empty() || processor_payment_id.is_empty() {
            return Ok(None);
        }
        let source_id = format!("{provider}:{processor_payment_id}");
        let revoked = match pool {
            DbPool::Sqlite(_) => {
                let mut conn = pool.get()?;
                let tx = conn.transaction()?;

                let row: Option<(String, String, i64)> = tx
                    .query_row(
                        "SELECT id, account_id, remaining_cents
                       FROM credit_batches
                      WHERE stripe_charge_id = ?1
                      LIMIT 1",
                        params![source_id],
                        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                    )
                    .ok();
                let Some((batch_id, account_id, remaining_cents)) = row else {
                    tx.commit()?;
                    return Ok(None);
                };

                if remaining_cents > 0 {
                    let balance_before: i64 = tx.query_row(
                        "SELECT balance_cents FROM accounts WHERE id = ?1",
                        params![&account_id],
                        |r| r.get(0),
                    )?;
                    tx.execute(
                        "UPDATE accounts
                        SET balance_cents = MAX(0, balance_cents - ?1)
                      WHERE id = ?2",
                        params![remaining_cents, &account_id],
                    )?;
                    tx.execute(
                        "UPDATE credit_batches
                        SET remaining_cents = 0,
                            expired_at = COALESCE(expired_at, datetime('now'))
                      WHERE id = ?1",
                        params![batch_id],
                    )?;
                    let balance_after: i64 = tx.query_row(
                        "SELECT balance_cents FROM accounts WHERE id = ?1",
                        params![&account_id],
                        |r| r.get(0),
                    )?;
                    insert_balance_ledger_sqlite_tx(
                        &tx,
                        BalanceLedgerEntry {
                            account_id: &account_id,
                            event_type: "processor_credit_revoked",
                            amount_cents: balance_after - balance_before,
                            balance_cents_before: balance_before,
                            balance_cents_after: balance_after,
                            reason: Some(reason),
                            provider: Some(&provider),
                            processor_payment_id: Some(processor_payment_id),
                            source_id: Some(&source_id),
                            idempotency_key: Some(&source_id),
                            request_id: None,
                            metadata_json: None,
                        },
                    )?;
                }
                tx.commit()?;
                (account_id, remaining_cents)
            }
            DbPool::Postgres(_) => {
                let mut conn = pool.get_pg()?;
                let mut tx = conn.transaction()?;

                let row = tx.query_opt(
                    "SELECT id, account_id, remaining_cents
                   FROM credit_batches
                  WHERE stripe_charge_id = $1
                  LIMIT 1",
                    &[&source_id],
                )?;
                let Some(row) = row else {
                    tx.commit()?;
                    return Ok(None);
                };
                let batch_id: String = row.try_get(0)?;
                let account_id: String = row.try_get(1)?;
                let remaining_cents: i64 = row.try_get(2)?;

                if remaining_cents > 0 {
                    let balance_before: i64 = tx
                        .query_one(
                            "SELECT balance_cents FROM accounts WHERE id = $1",
                            &[&account_id],
                        )?
                        .try_get(0)?;
                    tx.execute(
                        "UPDATE accounts
                        SET balance_cents = GREATEST(0, balance_cents - $1)
                      WHERE id = $2",
                        &[&remaining_cents, &account_id],
                    )?;
                    tx.execute(
                        "UPDATE credit_batches
                        SET remaining_cents = 0,
                            expired_at = COALESCE(expired_at, now())
                      WHERE id = $1",
                        &[&batch_id],
                    )?;
                    let balance_after: i64 = tx
                        .query_one(
                            "SELECT balance_cents FROM accounts WHERE id = $1",
                            &[&account_id],
                        )?
                        .try_get(0)?;
                    insert_balance_ledger_pg_tx(
                        &mut tx,
                        BalanceLedgerEntry {
                            account_id: &account_id,
                            event_type: "processor_credit_revoked",
                            amount_cents: balance_after - balance_before,
                            balance_cents_before: balance_before,
                            balance_cents_after: balance_after,
                            reason: Some(reason),
                            provider: Some(&provider),
                            processor_payment_id: Some(processor_payment_id),
                            source_id: Some(&source_id),
                            idempotency_key: Some(&source_id),
                            request_id: None,
                            metadata_json: None,
                        },
                    )?;
                }
                tx.commit()?;
                (account_id, remaining_cents)
            }
        };
        let (account_id, remaining_cents) = revoked;
        tracing::warn!(
            account_id_hash = %cue_core::account_id_hash_prefix(&account_id),
            provider = %provider,
            processor_payment_id = %processor_payment_id,
            revoked_cents = remaining_cents.max(0),
            reason = %reason,
            "processor-backed credits revoked"
        );
        Ok(Some((account_id, remaining_cents.max(0))))
    })
}

pub fn can_afford(pool: &DbPool, account_id: &str, estimated_cost_cents: i64) -> Result<bool> {
    crate::db::run_blocking_db(|| {
        let balance: i64 = match pool {
            DbPool::Sqlite(_) => {
                let conn = pool.get()?;
                conn.query_row(
                    "SELECT balance_cents FROM accounts WHERE id = ?1",
                    params![account_id],
                    |r| r.get(0),
                )?
            }
            DbPool::Postgres(_) => {
                let mut conn = pool.get_pg()?;
                conn.query_one(
                    "SELECT balance_cents FROM accounts WHERE id = $1",
                    &[&account_id],
                )?
                .try_get(0)?
            }
        };
        Ok(balance >= estimated_cost_cents)
    })
}

pub fn current_balance(pool: &DbPool, account_id: &str) -> Result<i64> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get()?;
            Ok(conn.query_row(
                "SELECT balance_cents FROM accounts WHERE id = ?1",
                params![account_id],
                |r| r.get(0),
            )?)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            Ok(conn
                .query_one(
                    "SELECT balance_cents FROM accounts WHERE id = $1",
                    &[&account_id],
                )?
                .try_get(0)?)
        }
    })
}

/// Decrement trial seconds. Returns the remaining count.
pub fn consume_trial_seconds(pool: &DbPool, account_id: &str, ms: i64) -> Result<i64> {
    crate::db::run_blocking_db(|| {
        // Codex S4.2: floor of 1 second per successful trial request so
        // sub-second instant-lane requests do not give effectively
        // unlimited free calls. The trial budget remains honest even for
        // sub-second completions.
        let secs = if ms <= 0 {
            1
        } else {
            ((ms + 999) / 1000).max(1)
        };
        match pool {
            DbPool::Sqlite(_) => {
                let conn = pool.get()?;
                conn.execute(
                    "UPDATE accounts
                    SET trial_seconds_remaining = MAX(0, trial_seconds_remaining - ?1)
                  WHERE id = ?2",
                    params![secs, account_id],
                )?;
                Ok(conn.query_row(
                    "SELECT trial_seconds_remaining FROM accounts WHERE id = ?1",
                    params![account_id],
                    |r| r.get(0),
                )?)
            }
            DbPool::Postgres(_) => {
                let mut conn = pool.get_pg()?;
                conn.execute(
                    "UPDATE accounts
                    SET trial_seconds_remaining = GREATEST(0, trial_seconds_remaining - $1)
                  WHERE id = $2",
                    &[&secs, &account_id],
                )?;
                Ok(conn
                    .query_one(
                        "SELECT trial_seconds_remaining FROM accounts WHERE id = $1",
                        &[&account_id],
                    )?
                    .try_get(0)?)
            }
        }
    })
}

pub fn sweep_expired(pool: &DbPool) -> Result<i64> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let now = Utc::now().to_rfc3339();
            let rows: Vec<(String, String, i64)> = {
                let mut stmt = conn.prepare(
                    "SELECT id, account_id, remaining_cents
                     FROM credit_batches
                     WHERE expires_at < ?1 AND remaining_cents > 0 AND expired_at IS NULL",
                )?;
                let rows = stmt
                    .query_map(params![now], |r| {
                        Ok((
                            r.get::<_, String>(0)?,
                            r.get::<_, String>(1)?,
                            r.get::<_, i64>(2)?,
                        ))
                    })?
                    .filter_map(|r| r.ok())
                    .collect();
                rows
            };

            let mut total: i64 = 0;
            for (batch_id, account_id, remaining) in rows {
                let tx = conn.transaction()?;
                let balance_before: i64 = tx.query_row(
                    "SELECT balance_cents FROM accounts WHERE id = ?1",
                    params![&account_id],
                    |r| r.get(0),
                )?;
                tx.execute(
                    "UPDATE accounts SET balance_cents = MAX(0, balance_cents - ?1) WHERE id = ?2",
                    params![remaining, account_id],
                )?;
                tx.execute(
                    "UPDATE credit_batches SET expired_at = ?1, remaining_cents = 0 WHERE id = ?2",
                    params![now, batch_id],
                )?;
                let balance_after: i64 = tx.query_row(
                    "SELECT balance_cents FROM accounts WHERE id = ?1",
                    params![&account_id],
                    |r| r.get(0),
                )?;
                insert_balance_ledger_sqlite_tx(
                    &tx,
                    BalanceLedgerEntry {
                        account_id: &account_id,
                        event_type: "credit_expired",
                        amount_cents: balance_after - balance_before,
                        balance_cents_before: balance_before,
                        balance_cents_after: balance_after,
                        reason: Some("credit_batch_expired"),
                        provider: None,
                        processor_payment_id: None,
                        source_id: Some(&batch_id),
                        idempotency_key: Some(&batch_id),
                        request_id: None,
                        metadata_json: None,
                    },
                )?;
                tx.commit()?;
                total += remaining;
                tracing::info!(
                    account_id,
                    batch_id,
                    remaining_cents = remaining,
                    "credit batch expired"
                );
            }
            Ok(total)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let rows: Vec<(String, String, i64)> = conn
                .query(
                    "SELECT id, account_id, remaining_cents
                     FROM credit_batches
                     WHERE expires_at < now() AND remaining_cents > 0 AND expired_at IS NULL",
                    &[],
                )?
                .into_iter()
                .map(|row| Ok((row.try_get(0)?, row.try_get(1)?, row.try_get(2)?)))
                .collect::<Result<Vec<_>>>()?;

            let mut total: i64 = 0;
            for (batch_id, account_id, remaining) in rows {
                let mut tx = conn.transaction()?;
                let balance_before: i64 = tx
                    .query_one(
                        "SELECT balance_cents FROM accounts WHERE id = $1",
                        &[&account_id],
                    )?
                    .try_get(0)?;
                tx.execute(
                    "UPDATE accounts SET balance_cents = GREATEST(0, balance_cents - $1) WHERE id = $2",
                    &[&remaining, &account_id],
                )?;
                tx.execute(
                    "UPDATE credit_batches SET expired_at = now(), remaining_cents = 0 WHERE id = $1",
                    &[&batch_id],
                )?;
                let balance_after: i64 = tx
                    .query_one(
                        "SELECT balance_cents FROM accounts WHERE id = $1",
                        &[&account_id],
                    )?
                    .try_get(0)?;
                insert_balance_ledger_pg_tx(
                    &mut tx,
                    BalanceLedgerEntry {
                        account_id: &account_id,
                        event_type: "credit_expired",
                        amount_cents: balance_after - balance_before,
                        balance_cents_before: balance_before,
                        balance_cents_after: balance_after,
                        reason: Some("credit_batch_expired"),
                        provider: None,
                        processor_payment_id: None,
                        source_id: Some(&batch_id),
                        idempotency_key: Some(&batch_id),
                        request_id: None,
                        metadata_json: None,
                    },
                )?;
                tx.commit()?;
                total += remaining;
                tracing::info!(
                    account_id,
                    batch_id,
                    remaining_cents = remaining,
                    "credit batch expired"
                );
            }
            Ok(total)
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_pool() -> DbPool {
        let path = std::env::temp_dir().join(format!("bluey-bal-{}.db", uuid::Uuid::new_v4()));
        let pool = crate::db::open_pool(&path).unwrap();
        crate::db::run_migrations(&pool).unwrap();
        pool
    }

    fn make_account(pool: &DbPool, email: &str) -> String {
        crate::db::accounts::Account::create(pool, email, "stub")
            .unwrap()
            .id
    }

    #[test]
    fn deduct_consumes_oldest_batch_first_fifo() {
        let pool = temp_pool();
        let id = make_account(&pool, "fifo@example.com");
        // Two reloads, oldest first.
        credit_processor_payment(&pool, &id, 1000, "stripe", "ch_1").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        credit_processor_payment(&pool, &id, 2000, "stripe", "ch_2").unwrap();
        // Spend $5: should drain the first batch (1000) entirely + 4 from second.
        assert!(deduct(&pool, &id, 1400).unwrap());
        let conn = pool.get().unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT remaining_cents FROM credit_batches WHERE account_id = ?1 ORDER BY purchased_at ASC",
            )
            .unwrap();
        let remainings: Vec<i64> = stmt
            .query_map(params![&id], |r| r.get(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        assert_eq!(remainings, vec![0, 1600]);
    }

    #[test]
    fn deduct_failure_doesnt_touch_batches() {
        let pool = temp_pool();
        let id = make_account(&pool, "broke@example.com");
        credit_internal(&pool, &id, 100, "test-seed").unwrap();
        assert!(!deduct(&pool, &id, 200).unwrap());
        let conn = pool.get().unwrap();
        let r: i64 = conn
            .query_row(
                "SELECT remaining_cents FROM credit_batches WHERE account_id = ?1",
                params![&id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(r, 100);
    }

    #[test]
    fn credit_extends_expiry_by_credit_validity_days() {
        let pool = temp_pool();
        let id = make_account(&pool, "expiry@example.com");
        credit_internal(&pool, &id, 3000, "test-seed").unwrap();
        let conn = pool.get().unwrap();
        let expires_at: String = conn
            .query_row(
                "SELECT expires_at FROM credit_batches WHERE account_id = ?1",
                params![&id],
                |r| r.get(0),
            )
            .unwrap();
        let parsed: chrono::DateTime<chrono::Utc> = expires_at.parse().unwrap();
        let days = (parsed - chrono::Utc::now()).num_days();
        assert!(((CREDIT_VALIDITY_DAYS - 5)..=(CREDIT_VALIDITY_DAYS + 1)).contains(&days));
    }

    #[test]
    fn trial_seconds_decrement_to_zero() {
        let pool = temp_pool();
        let id = make_account(&pool, "trial@example.com");
        // start: 15-minute trial budget
        // Ceiling-divide: 250_000ms = 250s consumed.
        let r = consume_trial_seconds(&pool, &id, 250_000).unwrap();
        assert_eq!(r, crate::db::accounts::DEFAULT_TRIAL_SECONDS - 250);
        // Way over: 1_000_000ms = 1000s consumed; clamped to 0.
        let r = consume_trial_seconds(&pool, &id, 1_000_000).unwrap();
        assert_eq!(r, 0);
        // Already 0; stays 0.
        let r = consume_trial_seconds(&pool, &id, 500_000).unwrap();
        assert_eq!(r, 0);
    }

    #[test]
    fn trial_seconds_min_1s_per_request() {
        let pool = temp_pool();
        let id = make_account(&pool, "trialmin@example.com");
        // Sub-second request must consume >=1s of trial budget.
        let r = consume_trial_seconds(&pool, &id, 100).unwrap();
        assert_eq!(r, crate::db::accounts::DEFAULT_TRIAL_SECONDS - 1);
        // 0ms (impossible in practice) still consumes 1s defensively.
        let r = consume_trial_seconds(&pool, &id, 0).unwrap();
        assert_eq!(r, crate::db::accounts::DEFAULT_TRIAL_SECONDS - 2);
    }

    #[test]
    fn sweep_no_op_when_nothing_expired() {
        let pool = temp_pool();
        let id = make_account(&pool, "fresh@example.com");
        credit_internal(&pool, &id, 3000, "test-seed").unwrap();
        assert_eq!(sweep_expired(&pool).unwrap(), 0);
    }

    #[test]
    fn credit_idempotent_on_same_processor_payment_id() {
        // Codex Stage 6 S6.1: webhook replay should not double-credit.
        let pool = temp_pool();
        let id = make_account(&pool, "idem-credit@example.com");
        let first = credit_processor_payment(&pool, &id, 3000, "stripe", "ch_abc").unwrap();
        assert!(first); // first credit ran
        let second = credit_processor_payment(&pool, &id, 3000, "stripe", "ch_abc").unwrap();
        assert!(!second); // duplicate detected, no-op
                          // Balance reflects single credit.
        let bal = current_balance(&pool, &id).unwrap();
        assert_eq!(bal, 3000);
    }

    #[test]
    fn balance_ledger_records_credit_debit_and_request_evidence() {
        let pool = temp_pool();
        let id = make_account(&pool, "ledger@example.com");
        assert!(credit_processor_payment(&pool, &id, 3000, "square", "pay_ledger").unwrap());
        assert!(deduct_for_request(&pool, &id, 175, "llm", "req-ledger").unwrap());

        let conn = pool.get().unwrap();
        let rows: Vec<(
            String,
            i64,
            i64,
            i64,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
        )> = conn
            .prepare(
                "SELECT event_type, amount_cents, balance_cents_before,
                        balance_cents_after, provider, processor_payment_id,
                        idempotency_key, request_id
                   FROM balance_ledger_entries
                  WHERE account_id = ?1
                  ORDER BY created_at ASC",
            )
            .unwrap()
            .query_map(params![&id], |row| {
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
            })
            .unwrap()
            .map(|row| row.unwrap())
            .collect();

        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].0, "processor_payment_credit");
        assert_eq!(rows[0].1, 3000);
        assert_eq!(rows[0].2, 0);
        assert_eq!(rows[0].3, 3000);
        assert_eq!(rows[0].4.as_deref(), Some("square"));
        assert_eq!(rows[0].5.as_deref(), Some("pay_ledger"));
        assert_eq!(rows[0].6.as_deref(), Some("square:pay_ledger"));

        assert_eq!(rows[1].0, "usage_deduction");
        assert_eq!(rows[1].1, -175);
        assert_eq!(rows[1].2, 3000);
        assert_eq!(rows[1].3, 2825);
        assert_eq!(rows[1].7.as_deref(), Some("req-ledger"));
    }

    #[test]
    fn revoke_processor_credit_records_actual_balance_delta() {
        let pool = temp_pool();
        let id = make_account(&pool, "revoke-ledger@example.com");
        credit_processor_payment(&pool, &id, 3000, "square", "pay_ledger_revoke").unwrap();
        assert!(deduct(&pool, &id, 1000).unwrap());
        revoke_processor_credit(&pool, "square", "pay_ledger_revoke", "refund.created").unwrap();

        let conn = pool.get().unwrap();
        let row: (i64, i64, i64, String, String) = conn
            .query_row(
                "SELECT amount_cents, balance_cents_before, balance_cents_after,
                        provider, processor_payment_id
                   FROM balance_ledger_entries
                  WHERE account_id = ?1 AND event_type = 'processor_credit_revoked'",
                params![&id],
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
        assert_eq!(row.0, -2000);
        assert_eq!(row.1, 2000);
        assert_eq!(row.2, 0);
        assert_eq!(row.3, "square");
        assert_eq!(row.4, "pay_ledger_revoke");
    }

    #[test]
    fn processor_credit_requires_non_empty_source() {
        let pool = temp_pool();
        let id = make_account(&pool, "source-required@example.com");
        assert!(credit_processor_payment(&pool, &id, 3000, "", "pay_1").is_err());
        assert!(credit_processor_payment(&pool, &id, 3000, "square", "").is_err());
        assert_eq!(current_balance(&pool, &id).unwrap(), 0);
    }

    #[test]
    fn credit_atomic_failure_leaves_account_clean() {
        // Codex Stage 6 S6.2: if the UPDATE somehow fails after the
        // INSERT, the transaction rolls back and balance + batches stay
        // in sync. We can\'t easily force an UPDATE failure here, but
        // we can at least sanity-check the happy path: balance after
        // single credit equals the credited amount, no orphan batch
        // count drift.
        let pool = temp_pool();
        let id = make_account(&pool, "atomic-credit@example.com");
        credit_internal(&pool, &id, 5000, "test-seed").unwrap();
        let bal = current_balance(&pool, &id).unwrap();
        assert_eq!(bal, 5000);
        let conn = pool.get().unwrap();
        let batch_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM credit_batches WHERE account_id = ?1",
                params![&id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(batch_count, 1);
    }
}
