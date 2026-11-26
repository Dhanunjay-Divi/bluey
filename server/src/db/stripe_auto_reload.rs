//! Durable state machine for Stripe Auto Reload.
//!
//! The processor flow creates a PaymentIntent without confirmation, persists
//! its id here, and only then confirms it. Success credit and risk reversal are
//! serialized on the attempt row so a late success cannot resurrect credit
//! after a refund or dispute.

use anyhow::{anyhow, bail, Result};
use chrono::{Duration, Utc};
use postgres::Row as PgRow;
use rusqlite::{params, OptionalExtension, Row as SqliteRow, TransactionBehavior};

use super::balance::{
    insert_balance_ledger_pg_tx, insert_balance_ledger_sqlite_tx, BalanceLedgerEntry,
    CREDIT_VALIDITY_DAYS,
};
use super::DbPool;

pub const STATUS_RESERVED: &str = "reserved";
pub const STATUS_REQUIRES_CONFIRMATION: &str = "requires_confirmation";
pub const STATUS_PROCESSING: &str = "processing";
pub const STATUS_RECONCILIATION_REQUIRED: &str = "reconciliation_required";
pub const STATUS_SUCCEEDED: &str = "succeeded";
pub const STATUS_FAILED: &str = "failed";
pub const STATUS_CANCELED: &str = "canceled";
pub const STATUS_REVERSED: &str = "reversed";

const MIN_AUTO_RELOAD_CENTS: i64 = 1_500;
const MAX_AUTO_RELOAD_CENTS: i64 = 50_000;
const MIN_AUTO_RELOAD_THRESHOLD_CENTS: i64 = 100;
const MAX_AUTO_RELOAD_THRESHOLD_CENTS: i64 = 5_000;

const ATTEMPT_COLUMNS: &str = "
    id, account_id, amount_cents, currency, stripe_customer_id,
    stripe_payment_method_id, stripe_payment_intent_id, stripe_charge_id,
    create_idempotency_key, confirm_idempotency_key, status, failure_code,
    last_event_id";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StripeAutoReloadAttempt {
    pub id: String,
    pub account_id: String,
    pub amount_cents: i64,
    pub currency: String,
    pub stripe_customer_id: String,
    pub stripe_payment_method_id: String,
    pub stripe_payment_intent_id: Option<String>,
    pub stripe_charge_id: Option<String>,
    pub create_idempotency_key: String,
    pub confirm_idempotency_key: String,
    pub status: String,
    pub failure_code: Option<String>,
    pub last_event_id: Option<String>,
}

impl StripeAutoReloadAttempt {
    pub fn is_active(&self) -> bool {
        is_active_status(&self.status)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreditDisposition {
    Credited,
    AlreadyCredited,
    SuppressedAfterReversal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReversalDisposition {
    pub account_id: String,
    pub revoked_cents: i64,
    pub already_reversed: bool,
}

fn is_active_status(status: &str) -> bool {
    matches!(
        status,
        STATUS_RESERVED
            | STATUS_REQUIRES_CONFIRMATION
            | STATUS_PROCESSING
            | STATUS_RECONCILIATION_REQUIRED
    )
}

fn sqlite_attempt(row: &SqliteRow<'_>) -> rusqlite::Result<StripeAutoReloadAttempt> {
    Ok(StripeAutoReloadAttempt {
        id: row.get(0)?,
        account_id: row.get(1)?,
        amount_cents: row.get(2)?,
        currency: row.get(3)?,
        stripe_customer_id: row.get(4)?,
        stripe_payment_method_id: row.get(5)?,
        stripe_payment_intent_id: row.get(6)?,
        stripe_charge_id: row.get(7)?,
        create_idempotency_key: row.get(8)?,
        confirm_idempotency_key: row.get(9)?,
        status: row.get(10)?,
        failure_code: row.get(11)?,
        last_event_id: row.get(12)?,
    })
}

fn pg_attempt(row: &PgRow) -> Result<StripeAutoReloadAttempt> {
    Ok(StripeAutoReloadAttempt {
        id: row.try_get(0)?,
        account_id: row.try_get(1)?,
        amount_cents: row.try_get(2)?,
        currency: row.try_get(3)?,
        stripe_customer_id: row.try_get(4)?,
        stripe_payment_method_id: row.try_get(5)?,
        stripe_payment_intent_id: row.try_get(6)?,
        stripe_charge_id: row.try_get(7)?,
        create_idempotency_key: row.try_get(8)?,
        confirm_idempotency_key: row.try_get(9)?,
        status: row.try_get(10)?,
        failure_code: row.try_get(11)?,
        last_event_id: row.try_get(12)?,
    })
}

fn clean_required_id(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn valid_settings(amount_cents: i64, threshold_cents: i64) -> bool {
    (MIN_AUTO_RELOAD_CENTS..=MAX_AUTO_RELOAD_CENTS).contains(&amount_cents)
        && (MIN_AUTO_RELOAD_THRESHOLD_CENTS..=MAX_AUTO_RELOAD_THRESHOLD_CENTS)
            .contains(&threshold_cents)
        && amount_cents > threshold_cents
}

fn clean_code(value: &str) -> String {
    let cleaned = value
        .trim()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
        .take(128)
        .collect::<String>();
    if cleaned.is_empty() {
        "stripe_auto_reload_error".to_string()
    } else {
        cleaned
    }
}

fn active_sqlite_attempt(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
) -> Result<Option<StripeAutoReloadAttempt>> {
    let sql = format!(
        "SELECT {ATTEMPT_COLUMNS}
           FROM stripe_auto_reload_attempts
          WHERE account_id = ?1
            AND status IN ('reserved', 'requires_confirmation', 'processing',
                           'reconciliation_required')
          LIMIT 1"
    );
    Ok(tx
        .query_row(&sql, params![account_id], sqlite_attempt)
        .optional()?)
}

fn active_pg_attempt(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
) -> Result<Option<StripeAutoReloadAttempt>> {
    let sql = format!(
        "SELECT {ATTEMPT_COLUMNS}
           FROM stripe_auto_reload_attempts
          WHERE account_id = $1
            AND status IN ('reserved', 'requires_confirmation', 'processing',
                           'reconciliation_required')
          LIMIT 1
          FOR UPDATE"
    );
    tx.query_opt(&sql, &[&account_id])?
        .as_ref()
        .map(pg_attempt)
        .transpose()
}

/// Return the existing active attempt or reserve a new one from canonical
/// account settings. A stale low-balance signal from a request cannot bypass
/// the checks in this transaction.
pub fn reserve_if_eligible(
    pool: &DbPool,
    account_id: &str,
) -> Result<Option<StripeAutoReloadAttempt>> {
    super::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            if let Some(attempt) = active_sqlite_attempt(&tx, account_id)? {
                tx.commit()?;
                return Ok(Some(attempt));
            }

            let account = tx
                .query_row(
                    "SELECT balance_cents, auto_topup_enabled,
                            auto_topup_threshold_cents, auto_topup_amount_cents,
                            stripe_customer_id, stripe_payment_method_id,
                            billing_restricted, email_verified_at IS NOT NULL
                       FROM accounts WHERE id = ?1",
                    params![account_id],
                    |row| {
                        Ok((
                            row.get::<_, i64>(0)?,
                            row.get::<_, i64>(1)? != 0,
                            row.get::<_, i64>(2)?,
                            row.get::<_, i64>(3)?,
                            row.get::<_, Option<String>>(4)?,
                            row.get::<_, Option<String>>(5)?,
                            row.get::<_, i64>(6)? != 0,
                            row.get::<_, i64>(7)? != 0,
                        ))
                    },
                )
                .optional()?;
            let Some((
                balance_cents,
                enabled,
                threshold_cents,
                amount_cents,
                customer_id,
                payment_method_id,
                restricted,
                email_verified,
            )) = account
            else {
                tx.commit()?;
                return Ok(None);
            };
            let customer_id = clean_required_id(customer_id);
            let payment_method_id = clean_required_id(payment_method_id);
            if !enabled
                || restricted
                || !email_verified
                || balance_cents >= threshold_cents
                || !valid_settings(amount_cents, threshold_cents)
                || customer_id.is_none()
                || payment_method_id.is_none()
            {
                tx.commit()?;
                return Ok(None);
            }

            let id = uuid::Uuid::new_v4().to_string();
            let create_key = format!("bluey-ar-create-{id}");
            let confirm_key = format!("bluey-ar-confirm-{id}");
            tx.execute(
                "INSERT OR IGNORE INTO stripe_auto_reload_attempts
                    (id, account_id, amount_cents, currency, stripe_customer_id,
                     stripe_payment_method_id, create_idempotency_key,
                     confirm_idempotency_key, status)
                 VALUES (?1, ?2, ?3, 'usd', ?4, ?5, ?6, ?7, 'reserved')",
                params![
                    id,
                    account_id,
                    amount_cents,
                    customer_id,
                    payment_method_id,
                    create_key,
                    confirm_key
                ],
            )?;
            let attempt = active_sqlite_attempt(&tx, account_id)?
                .ok_or_else(|| anyhow!("failed to reserve Stripe Auto Reload attempt"))?;
            tx.commit()?;
            Ok(Some(attempt))
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            if let Some(attempt) = active_pg_attempt(&mut tx, account_id)? {
                tx.commit()?;
                return Ok(Some(attempt));
            }

            let account = tx.query_opt(
                "SELECT balance_cents, auto_topup_enabled,
                        auto_topup_threshold_cents, auto_topup_amount_cents,
                        stripe_customer_id, stripe_payment_method_id,
                        billing_restricted, email_verified_at IS NOT NULL
                   FROM accounts WHERE id = $1 FOR UPDATE",
                &[&account_id],
            )?;
            let Some(account) = account else {
                tx.commit()?;
                return Ok(None);
            };
            let balance_cents: i64 = account.try_get(0)?;
            let enabled = account.try_get::<_, i32>(1)? != 0;
            let threshold_cents: i64 = account.try_get(2)?;
            let amount_cents: i64 = account.try_get(3)?;
            let customer_id = clean_required_id(account.try_get(4)?);
            let payment_method_id = clean_required_id(account.try_get(5)?);
            let restricted = account.try_get::<_, i32>(6)? != 0;
            let email_verified: bool = account.try_get(7)?;
            if !enabled
                || restricted
                || !email_verified
                || balance_cents >= threshold_cents
                || !valid_settings(amount_cents, threshold_cents)
                || customer_id.is_none()
                || payment_method_id.is_none()
            {
                tx.commit()?;
                return Ok(None);
            }

            let id = uuid::Uuid::new_v4().to_string();
            let create_key = format!("bluey-ar-create-{id}");
            let confirm_key = format!("bluey-ar-confirm-{id}");
            tx.execute(
                "INSERT INTO stripe_auto_reload_attempts
                    (id, account_id, amount_cents, currency, stripe_customer_id,
                     stripe_payment_method_id, create_idempotency_key,
                     confirm_idempotency_key, status)
                 VALUES ($1, $2, $3, 'usd', $4, $5, $6, $7, 'reserved')
                 ON CONFLICT DO NOTHING",
                &[
                    &id,
                    &account_id,
                    &amount_cents,
                    &customer_id,
                    &payment_method_id,
                    &create_key,
                    &confirm_key,
                ],
            )?;
            let attempt = active_pg_attempt(&mut tx, account_id)?
                .ok_or_else(|| anyhow!("failed to reserve Stripe Auto Reload attempt"))?;
            tx.commit()?;
            Ok(Some(attempt))
        }
    })
}

pub fn find_by_id(pool: &DbPool, id: &str) -> Result<Option<StripeAutoReloadAttempt>> {
    find_one(pool, "id", id)
}

pub fn find_by_payment_intent(
    pool: &DbPool,
    payment_intent_id: &str,
) -> Result<Option<StripeAutoReloadAttempt>> {
    find_one(pool, "stripe_payment_intent_id", payment_intent_id)
}

pub fn find_by_charge(pool: &DbPool, charge_id: &str) -> Result<Option<StripeAutoReloadAttempt>> {
    find_one(pool, "stripe_charge_id", charge_id)
}

fn find_one(pool: &DbPool, column: &str, value: &str) -> Result<Option<StripeAutoReloadAttempt>> {
    let value = value.trim();
    if value.is_empty() {
        return Ok(None);
    }
    let column = match column {
        "id" => "id",
        "stripe_payment_intent_id" => "stripe_payment_intent_id",
        "stripe_charge_id" => "stripe_charge_id",
        _ => bail!("unsupported Stripe Auto Reload lookup column"),
    };
    super::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get()?;
            let sql = format!(
                "SELECT {ATTEMPT_COLUMNS} FROM stripe_auto_reload_attempts WHERE {column} = ?1"
            );
            Ok(conn
                .query_row(&sql, params![value], sqlite_attempt)
                .optional()?)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let sql = format!(
                "SELECT {ATTEMPT_COLUMNS} FROM stripe_auto_reload_attempts WHERE {column} = $1"
            );
            conn.query_opt(&sql, &[&value])?
                .as_ref()
                .map(pg_attempt)
                .transpose()
        }
    })
}

pub fn attach_payment_intent(
    pool: &DbPool,
    attempt_id: &str,
    payment_intent_id: &str,
) -> Result<StripeAutoReloadAttempt> {
    let payment_intent_id = payment_intent_id.trim();
    if payment_intent_id.is_empty() {
        bail!("Stripe PaymentIntent id is empty");
    }
    super::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let sql =
                format!("SELECT {ATTEMPT_COLUMNS} FROM stripe_auto_reload_attempts WHERE id = ?1");
            let current = tx
                .query_row(&sql, params![attempt_id], sqlite_attempt)
                .optional()?
                .ok_or_else(|| anyhow!("Stripe Auto Reload attempt not found"))?;
            if let Some(existing) = current.stripe_payment_intent_id.as_deref() {
                if existing != payment_intent_id {
                    bail!("Stripe Auto Reload attempt already has a different PaymentIntent");
                }
            }
            tx.execute(
                "UPDATE stripe_auto_reload_attempts
                    SET stripe_payment_intent_id = COALESCE(stripe_payment_intent_id, ?2),
                        status = CASE
                            WHEN status IN ('reserved', 'reconciliation_required')
                            THEN 'requires_confirmation'
                            ELSE status
                        END,
                        payment_intent_created_at = COALESCE(
                            payment_intent_created_at, datetime('now')
                        ),
                        updated_at = datetime('now')
                  WHERE id = ?1",
                params![attempt_id, payment_intent_id],
            )?;
            let updated = tx.query_row(&sql, params![attempt_id], sqlite_attempt)?;
            tx.commit()?;
            Ok(updated)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            let sql = format!(
                "SELECT {ATTEMPT_COLUMNS} FROM stripe_auto_reload_attempts WHERE id = $1 FOR UPDATE"
            );
            let current = tx
                .query_opt(&sql, &[&attempt_id])?
                .as_ref()
                .map(pg_attempt)
                .transpose()?
                .ok_or_else(|| anyhow!("Stripe Auto Reload attempt not found"))?;
            if let Some(existing) = current.stripe_payment_intent_id.as_deref() {
                if existing != payment_intent_id {
                    bail!("Stripe Auto Reload attempt already has a different PaymentIntent");
                }
            }
            tx.execute(
                "UPDATE stripe_auto_reload_attempts
                    SET stripe_payment_intent_id = COALESCE(stripe_payment_intent_id, $2),
                        status = CASE
                            WHEN status IN ('reserved', 'reconciliation_required')
                            THEN 'requires_confirmation'
                            ELSE status
                        END,
                        payment_intent_created_at = COALESCE(payment_intent_created_at, now()),
                        updated_at = now()
                  WHERE id = $1",
                &[&attempt_id, &payment_intent_id],
            )?;
            let updated = pg_attempt(&tx.query_one(&sql, &[&attempt_id])?)?;
            tx.commit()?;
            Ok(updated)
        }
    })
}

/// Re-check mutable account state immediately before confirmation.
pub fn confirmation_allowed(pool: &DbPool, attempt_id: &str) -> Result<bool> {
    super::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get()?;
            let allowed = conn
                .query_row(
                    "SELECT 1
                       FROM stripe_auto_reload_attempts r
                       JOIN accounts a ON a.id = r.account_id
                      WHERE r.id = ?1
                        AND r.status IN ('requires_confirmation',
                                         'reconciliation_required')
                        AND r.stripe_payment_intent_id IS NOT NULL
                        AND a.auto_topup_enabled = 1
                        AND a.billing_restricted = 0
                        AND a.email_verified_at IS NOT NULL
                        AND a.balance_cents < a.auto_topup_threshold_cents
                        AND a.auto_topup_amount_cents = r.amount_cents
                        AND a.stripe_customer_id = r.stripe_customer_id
                        AND a.stripe_payment_method_id = r.stripe_payment_method_id",
                    params![attempt_id],
                    |_| Ok(()),
                )
                .optional()?
                .is_some();
            Ok(allowed)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            Ok(conn
                .query_opt(
                    "SELECT 1
                       FROM stripe_auto_reload_attempts r
                       JOIN accounts a ON a.id = r.account_id
                      WHERE r.id = $1
                        AND r.status IN ('requires_confirmation',
                                         'reconciliation_required')
                        AND r.stripe_payment_intent_id IS NOT NULL
                        AND a.auto_topup_enabled != 0
                        AND a.billing_restricted = 0
                        AND a.email_verified_at IS NOT NULL
                        AND a.balance_cents < a.auto_topup_threshold_cents
                        AND a.auto_topup_amount_cents = r.amount_cents
                        AND a.stripe_customer_id = r.stripe_customer_id
                        AND a.stripe_payment_method_id = r.stripe_payment_method_id",
                    &[&attempt_id],
                )?
                .is_some())
        }
    })
}

pub fn mark_processing(
    pool: &DbPool,
    attempt_id: &str,
    charge_id: Option<&str>,
    event_id: Option<&str>,
) -> Result<()> {
    update_pending_status(
        pool,
        attempt_id,
        STATUS_PROCESSING,
        charge_id,
        event_id,
        None,
    )
}

pub fn mark_reconciliation_required(pool: &DbPool, attempt_id: &str, code: &str) -> Result<()> {
    update_pending_status(
        pool,
        attempt_id,
        STATUS_RECONCILIATION_REQUIRED,
        None,
        None,
        Some(&clean_code(code)),
    )
}

fn update_pending_status(
    pool: &DbPool,
    attempt_id: &str,
    status: &str,
    charge_id: Option<&str>,
    event_id: Option<&str>,
    failure_code: Option<&str>,
) -> Result<()> {
    let charge_id = charge_id.map(str::trim).filter(|value| !value.is_empty());
    super::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let sql =
                format!("SELECT {ATTEMPT_COLUMNS} FROM stripe_auto_reload_attempts WHERE id = ?1");
            let current = tx
                .query_row(&sql, params![attempt_id], sqlite_attempt)
                .optional()?
                .ok_or_else(|| anyhow!("Stripe Auto Reload attempt not found"))?;
            if !current.is_active() {
                tx.commit()?;
                return Ok(());
            }
            if let (Some(existing), Some(observed)) =
                (current.stripe_charge_id.as_deref(), charge_id)
            {
                if existing != observed {
                    bail!("Stripe Auto Reload attempt charge id mismatch");
                }
            }
            tx.execute(
                "UPDATE stripe_auto_reload_attempts
                    SET status = ?2,
                        stripe_charge_id = COALESCE(stripe_charge_id, ?3),
                        last_event_id = COALESCE(?4, last_event_id),
                        failure_code = COALESCE(?5, failure_code),
                        last_reconciled_at = datetime('now'),
                        updated_at = datetime('now')
                  WHERE id = ?1",
                params![attempt_id, status, charge_id, event_id, failure_code],
            )?;
            tx.commit()?;
            Ok(())
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            let sql = format!(
                "SELECT {ATTEMPT_COLUMNS} FROM stripe_auto_reload_attempts WHERE id = $1 FOR UPDATE"
            );
            let current = tx
                .query_opt(&sql, &[&attempt_id])?
                .as_ref()
                .map(pg_attempt)
                .transpose()?
                .ok_or_else(|| anyhow!("Stripe Auto Reload attempt not found"))?;
            if !current.is_active() {
                tx.commit()?;
                return Ok(());
            }
            if let (Some(existing), Some(observed)) =
                (current.stripe_charge_id.as_deref(), charge_id)
            {
                if existing != observed {
                    bail!("Stripe Auto Reload attempt charge id mismatch");
                }
            }
            tx.execute(
                "UPDATE stripe_auto_reload_attempts
                    SET status = $2,
                        stripe_charge_id = COALESCE(stripe_charge_id, $3),
                        last_event_id = COALESCE($4, last_event_id),
                        failure_code = COALESCE($5, failure_code),
                        last_reconciled_at = now(),
                        updated_at = now()
                  WHERE id = $1",
                &[&attempt_id, &status, &charge_id, &event_id, &failure_code],
            )?;
            tx.commit()?;
            Ok(())
        }
    })
}

pub fn mark_failed_and_disable(
    pool: &DbPool,
    attempt_id: &str,
    failure_code: &str,
    event_id: Option<&str>,
) -> Result<bool> {
    finish_without_credit(
        pool,
        attempt_id,
        STATUS_FAILED,
        failure_code,
        event_id,
        true,
    )
}

pub fn mark_abandoned(pool: &DbPool, attempt_id: &str, reason: &str) -> Result<bool> {
    finish_without_credit(pool, attempt_id, STATUS_CANCELED, reason, None, false)
}

fn finish_without_credit(
    pool: &DbPool,
    attempt_id: &str,
    status: &str,
    failure_code: &str,
    event_id: Option<&str>,
    disable_account: bool,
) -> Result<bool> {
    let failure_code = clean_code(failure_code);
    super::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let current_status: Option<(String, String)> = tx
                .query_row(
                    "SELECT status, account_id FROM stripe_auto_reload_attempts WHERE id = ?1",
                    params![attempt_id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()?;
            let Some((current_status, account_id)) = current_status else {
                bail!("Stripe Auto Reload attempt not found");
            };
            if matches!(current_status.as_str(), STATUS_SUCCEEDED | STATUS_REVERSED) {
                tx.commit()?;
                return Ok(false);
            }
            tx.execute(
                "UPDATE stripe_auto_reload_attempts
                    SET status = ?2,
                        failure_code = ?3,
                        last_event_id = COALESCE(?4, last_event_id),
                        failed_at = COALESCE(failed_at, datetime('now')),
                        updated_at = datetime('now')
                  WHERE id = ?1",
                params![attempt_id, status, failure_code, event_id],
            )?;
            if disable_account {
                tx.execute(
                    "UPDATE accounts
                        SET auto_topup_enabled = 0,
                            stripe_payment_method_id = NULL
                      WHERE id = ?1",
                    params![account_id],
                )?;
            }
            tx.commit()?;
            Ok(true)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            let row = tx.query_opt(
                "SELECT status, account_id
                   FROM stripe_auto_reload_attempts
                  WHERE id = $1 FOR UPDATE",
                &[&attempt_id],
            )?;
            let Some(row) = row else {
                bail!("Stripe Auto Reload attempt not found");
            };
            let current_status: String = row.try_get(0)?;
            let account_id: String = row.try_get(1)?;
            if matches!(current_status.as_str(), STATUS_SUCCEEDED | STATUS_REVERSED) {
                tx.commit()?;
                return Ok(false);
            }
            tx.execute(
                "UPDATE stripe_auto_reload_attempts
                    SET status = $2,
                        failure_code = $3,
                        last_event_id = COALESCE($4, last_event_id),
                        failed_at = COALESCE(failed_at, now()),
                        updated_at = now()
                  WHERE id = $1",
                &[&attempt_id, &status, &failure_code, &event_id],
            )?;
            if disable_account {
                tx.execute(
                    "UPDATE accounts
                        SET auto_topup_enabled = 0,
                            stripe_payment_method_id = NULL
                      WHERE id = $1",
                    &[&account_id],
                )?;
            }
            tx.commit()?;
            Ok(true)
        }
    })
}

/// Atomically credit a successful attempt and mark it terminal.
pub fn credit_succeeded(
    pool: &DbPool,
    attempt_id: &str,
    payment_intent_id: &str,
    charge_id: Option<&str>,
    event_id: Option<&str>,
) -> Result<CreditDisposition> {
    let payment_intent_id = payment_intent_id.trim();
    if payment_intent_id.is_empty() {
        bail!("Stripe PaymentIntent id is empty");
    }
    let charge_id = charge_id.map(str::trim).filter(|value| !value.is_empty());
    super::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let sql =
                format!("SELECT {ATTEMPT_COLUMNS} FROM stripe_auto_reload_attempts WHERE id = ?1");
            let attempt = tx
                .query_row(&sql, params![attempt_id], sqlite_attempt)
                .optional()?
                .ok_or_else(|| anyhow!("Stripe Auto Reload attempt not found"))?;
            validate_success_ids(&attempt, payment_intent_id, charge_id)?;
            if attempt.status == STATUS_REVERSED {
                tx.commit()?;
                return Ok(CreditDisposition::SuppressedAfterReversal);
            }

            let source_id = format!("stripe:{payment_intent_id}");
            let existing: Option<(String, i64)> = tx
                .query_row(
                    "SELECT account_id, amount_cents
                       FROM credit_batches WHERE stripe_charge_id = ?1",
                    params![source_id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()?;
            let disposition = if let Some((account_id, amount_cents)) = existing {
                if account_id != attempt.account_id || amount_cents != attempt.amount_cents {
                    bail!("Stripe PaymentIntent credit is attached to different account data");
                }
                CreditDisposition::AlreadyCredited
            } else {
                let balance_before: i64 = tx.query_row(
                    "SELECT balance_cents FROM accounts WHERE id = ?1",
                    params![attempt.account_id],
                    |row| row.get(0),
                )?;
                let batch_id = uuid::Uuid::new_v4().to_string();
                let expires_at = (Utc::now() + Duration::days(CREDIT_VALIDITY_DAYS)).to_rfc3339();
                tx.execute(
                    "INSERT INTO credit_batches
                        (id, account_id, amount_cents, remaining_cents,
                         expires_at, stripe_charge_id)
                     VALUES (?1, ?2, ?3, ?3, ?4, ?5)",
                    params![
                        batch_id,
                        attempt.account_id,
                        attempt.amount_cents,
                        expires_at,
                        source_id
                    ],
                )?;
                let updated = tx.execute(
                    "UPDATE accounts SET balance_cents = balance_cents + ?1 WHERE id = ?2",
                    params![attempt.amount_cents, attempt.account_id],
                )?;
                if updated != 1 {
                    bail!("Stripe Auto Reload account disappeared before credit");
                }
                let metadata = serde_json::json!({
                    "stripe_auto_reload_attempt_id": attempt.id
                })
                .to_string();
                insert_balance_ledger_sqlite_tx(
                    &tx,
                    BalanceLedgerEntry {
                        account_id: &attempt.account_id,
                        event_type: "processor_payment_credit",
                        amount_cents: attempt.amount_cents,
                        balance_cents_before: balance_before,
                        balance_cents_after: balance_before + attempt.amount_cents,
                        reason: Some("stripe_auto_reload_succeeded"),
                        provider: Some("stripe"),
                        processor_payment_id: Some(payment_intent_id),
                        source_id: Some(&source_id),
                        idempotency_key: Some(&source_id),
                        request_id: None,
                        metadata_json: Some(&metadata),
                    },
                )?;
                CreditDisposition::Credited
            };

            tx.execute(
                "UPDATE stripe_auto_reload_attempts
                    SET status = 'succeeded',
                        stripe_charge_id = COALESCE(stripe_charge_id, ?2),
                        last_event_id = COALESCE(?3, last_event_id),
                        charged_at = COALESCE(charged_at, datetime('now')),
                        credited_at = COALESCE(credited_at, datetime('now')),
                        failure_code = NULL,
                        updated_at = datetime('now')
                  WHERE id = ?1",
                params![attempt_id, charge_id, event_id],
            )?;
            tx.commit()?;
            Ok(disposition)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            let sql = format!(
                "SELECT {ATTEMPT_COLUMNS} FROM stripe_auto_reload_attempts WHERE id = $1 FOR UPDATE"
            );
            let attempt = tx
                .query_opt(&sql, &[&attempt_id])?
                .as_ref()
                .map(pg_attempt)
                .transpose()?
                .ok_or_else(|| anyhow!("Stripe Auto Reload attempt not found"))?;
            validate_success_ids(&attempt, payment_intent_id, charge_id)?;
            if attempt.status == STATUS_REVERSED {
                tx.commit()?;
                return Ok(CreditDisposition::SuppressedAfterReversal);
            }

            let source_id = format!("stripe:{payment_intent_id}");
            let existing = tx.query_opt(
                "SELECT account_id, amount_cents
                   FROM credit_batches WHERE stripe_charge_id = $1",
                &[&source_id],
            )?;
            let disposition = if let Some(existing) = existing {
                let account_id: String = existing.try_get(0)?;
                let amount_cents: i64 = existing.try_get(1)?;
                if account_id != attempt.account_id || amount_cents != attempt.amount_cents {
                    bail!("Stripe PaymentIntent credit is attached to different account data");
                }
                CreditDisposition::AlreadyCredited
            } else {
                let balance_before: i64 = tx
                    .query_one(
                        "SELECT balance_cents FROM accounts WHERE id = $1 FOR UPDATE",
                        &[&attempt.account_id],
                    )?
                    .try_get(0)?;
                let batch_id = uuid::Uuid::new_v4().to_string();
                let expires_at = Utc::now() + Duration::days(CREDIT_VALIDITY_DAYS);
                tx.execute(
                    "INSERT INTO credit_batches
                        (id, account_id, amount_cents, remaining_cents,
                         expires_at, stripe_charge_id)
                     VALUES ($1, $2, $3, $3, $4, $5)",
                    &[
                        &batch_id,
                        &attempt.account_id,
                        &attempt.amount_cents,
                        &expires_at,
                        &source_id,
                    ],
                )?;
                let updated = tx.execute(
                    "UPDATE accounts SET balance_cents = balance_cents + $1 WHERE id = $2",
                    &[&attempt.amount_cents, &attempt.account_id],
                )?;
                if updated != 1 {
                    bail!("Stripe Auto Reload account disappeared before credit");
                }
                let metadata = serde_json::json!({
                    "stripe_auto_reload_attempt_id": attempt.id
                })
                .to_string();
                insert_balance_ledger_pg_tx(
                    &mut tx,
                    BalanceLedgerEntry {
                        account_id: &attempt.account_id,
                        event_type: "processor_payment_credit",
                        amount_cents: attempt.amount_cents,
                        balance_cents_before: balance_before,
                        balance_cents_after: balance_before + attempt.amount_cents,
                        reason: Some("stripe_auto_reload_succeeded"),
                        provider: Some("stripe"),
                        processor_payment_id: Some(payment_intent_id),
                        source_id: Some(&source_id),
                        idempotency_key: Some(&source_id),
                        request_id: None,
                        metadata_json: Some(&metadata),
                    },
                )?;
                CreditDisposition::Credited
            };

            tx.execute(
                "UPDATE stripe_auto_reload_attempts
                    SET status = 'succeeded',
                        stripe_charge_id = COALESCE(stripe_charge_id, $2),
                        last_event_id = COALESCE($3, last_event_id),
                        charged_at = COALESCE(charged_at, now()),
                        credited_at = COALESCE(credited_at, now()),
                        failure_code = NULL,
                        updated_at = now()
                  WHERE id = $1",
                &[&attempt_id, &charge_id, &event_id],
            )?;
            tx.commit()?;
            Ok(disposition)
        }
    })
}

fn validate_success_ids(
    attempt: &StripeAutoReloadAttempt,
    payment_intent_id: &str,
    charge_id: Option<&str>,
) -> Result<()> {
    if attempt.stripe_payment_intent_id.as_deref() != Some(payment_intent_id) {
        bail!("Stripe Auto Reload PaymentIntent does not match durable attempt");
    }
    if let (Some(expected), Some(observed)) = (attempt.stripe_charge_id.as_deref(), charge_id) {
        if expected != observed {
            bail!("Stripe Auto Reload charge id does not match durable attempt");
        }
    }
    Ok(())
}

/// Atomically make a refund/dispute terminal, revoke any unspent credit, and
/// restrict the account. Calling this before a delayed succeeded event makes
/// that later event a successful no-op.
pub fn reverse_and_restrict(
    pool: &DbPool,
    attempt_id: &str,
    reason: &str,
    event_id: &str,
) -> Result<ReversalDisposition> {
    let reason = clean_code(reason);
    let event_id = event_id.trim();
    let restriction_reason = format!(
        "{reason}:{}",
        event_id.chars().take(128).collect::<String>()
    );
    super::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let sql =
                format!("SELECT {ATTEMPT_COLUMNS} FROM stripe_auto_reload_attempts WHERE id = ?1");
            let attempt = tx
                .query_row(&sql, params![attempt_id], sqlite_attempt)
                .optional()?
                .ok_or_else(|| anyhow!("Stripe Auto Reload attempt not found"))?;
            let already_reversed = attempt.status == STATUS_REVERSED;
            let revoked_cents =
                if let Some(payment_intent_id) = attempt.stripe_payment_intent_id.as_deref() {
                    let source_id = format!("stripe:{payment_intent_id}");
                    let batch: Option<(String, i64)> = tx
                        .query_row(
                            "SELECT id, remaining_cents
                           FROM credit_batches WHERE stripe_charge_id = ?1",
                            params![source_id],
                            |row| Ok((row.get(0)?, row.get(1)?)),
                        )
                        .optional()?;
                    if let Some((batch_id, remaining_cents)) = batch {
                        if remaining_cents > 0 {
                            let balance_before: i64 = tx.query_row(
                                "SELECT balance_cents FROM accounts WHERE id = ?1",
                                params![attempt.account_id],
                                |row| row.get(0),
                            )?;
                            tx.execute(
                                "UPDATE accounts
                                SET balance_cents = MAX(0, balance_cents - ?1)
                              WHERE id = ?2",
                                params![remaining_cents, attempt.account_id],
                            )?;
                            tx.execute(
                                "UPDATE credit_batches SET remaining_cents = 0 WHERE id = ?1",
                                params![batch_id],
                            )?;
                            let balance_after: i64 = tx.query_row(
                                "SELECT balance_cents FROM accounts WHERE id = ?1",
                                params![attempt.account_id],
                                |row| row.get(0),
                            )?;
                            let metadata = serde_json::json!({
                                "stripe_auto_reload_attempt_id": attempt.id,
                                "stripe_event_id": event_id
                            })
                            .to_string();
                            insert_balance_ledger_sqlite_tx(
                                &tx,
                                BalanceLedgerEntry {
                                    account_id: &attempt.account_id,
                                    event_type: "processor_credit_revoked",
                                    amount_cents: balance_after - balance_before,
                                    balance_cents_before: balance_before,
                                    balance_cents_after: balance_after,
                                    reason: Some(&reason),
                                    provider: Some("stripe"),
                                    processor_payment_id: Some(payment_intent_id),
                                    source_id: Some(&source_id),
                                    idempotency_key: Some(event_id),
                                    request_id: None,
                                    metadata_json: Some(&metadata),
                                },
                            )?;
                        }
                        remaining_cents.max(0)
                    } else {
                        0
                    }
                } else {
                    0
                };

            tx.execute(
                "UPDATE stripe_auto_reload_attempts
                    SET status = 'reversed',
                        failure_code = ?2,
                        last_event_id = ?3,
                        reversed_at = COALESCE(reversed_at, datetime('now')),
                        updated_at = datetime('now')
                  WHERE id = ?1",
                params![attempt_id, reason, event_id],
            )?;
            tx.execute(
                "UPDATE accounts
                    SET billing_restricted = 1,
                        billing_restriction_reason = ?2,
                        billing_restricted_at = COALESCE(
                            billing_restricted_at, datetime('now')
                        ),
                        auto_topup_enabled = 0,
                        stripe_payment_method_id = NULL
                  WHERE id = ?1",
                params![attempt.account_id, restriction_reason],
            )?;
            tx.commit()?;
            Ok(ReversalDisposition {
                account_id: attempt.account_id,
                revoked_cents,
                already_reversed,
            })
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            let sql = format!(
                "SELECT {ATTEMPT_COLUMNS} FROM stripe_auto_reload_attempts WHERE id = $1 FOR UPDATE"
            );
            let attempt = tx
                .query_opt(&sql, &[&attempt_id])?
                .as_ref()
                .map(pg_attempt)
                .transpose()?
                .ok_or_else(|| anyhow!("Stripe Auto Reload attempt not found"))?;
            let account_balance_before: i64 = tx
                .query_one(
                    "SELECT balance_cents FROM accounts WHERE id = $1 FOR UPDATE",
                    &[&attempt.account_id],
                )?
                .try_get(0)?;
            let already_reversed = attempt.status == STATUS_REVERSED;
            let revoked_cents =
                if let Some(payment_intent_id) = attempt.stripe_payment_intent_id.as_deref() {
                    let source_id = format!("stripe:{payment_intent_id}");
                    let batch = tx.query_opt(
                        "SELECT id, remaining_cents
                       FROM credit_batches
                      WHERE stripe_charge_id = $1
                      FOR UPDATE",
                        &[&source_id],
                    )?;
                    if let Some(batch) = batch {
                        let batch_id: String = batch.try_get(0)?;
                        let remaining_cents: i64 = batch.try_get(1)?;
                        if remaining_cents > 0 {
                            tx.execute(
                                "UPDATE accounts
                                SET balance_cents = GREATEST(0, balance_cents - $1)
                              WHERE id = $2",
                                &[&remaining_cents, &attempt.account_id],
                            )?;
                            tx.execute(
                                "UPDATE credit_batches SET remaining_cents = 0 WHERE id = $1",
                                &[&batch_id],
                            )?;
                            let balance_after: i64 = tx
                                .query_one(
                                    "SELECT balance_cents FROM accounts WHERE id = $1",
                                    &[&attempt.account_id],
                                )?
                                .try_get(0)?;
                            let metadata = serde_json::json!({
                                "stripe_auto_reload_attempt_id": attempt.id,
                                "stripe_event_id": event_id
                            })
                            .to_string();
                            insert_balance_ledger_pg_tx(
                                &mut tx,
                                BalanceLedgerEntry {
                                    account_id: &attempt.account_id,
                                    event_type: "processor_credit_revoked",
                                    amount_cents: balance_after - account_balance_before,
                                    balance_cents_before: account_balance_before,
                                    balance_cents_after: balance_after,
                                    reason: Some(&reason),
                                    provider: Some("stripe"),
                                    processor_payment_id: Some(payment_intent_id),
                                    source_id: Some(&source_id),
                                    idempotency_key: Some(event_id),
                                    request_id: None,
                                    metadata_json: Some(&metadata),
                                },
                            )?;
                        }
                        remaining_cents.max(0)
                    } else {
                        0
                    }
                } else {
                    0
                };

            tx.execute(
                "UPDATE stripe_auto_reload_attempts
                    SET status = 'reversed',
                        failure_code = $2,
                        last_event_id = $3,
                        reversed_at = COALESCE(reversed_at, now()),
                        updated_at = now()
                  WHERE id = $1",
                &[&attempt_id, &reason, &event_id],
            )?;
            tx.execute(
                "UPDATE accounts
                    SET billing_restricted = 1,
                        billing_restriction_reason = $2,
                        billing_restricted_at = COALESCE(billing_restricted_at, now()),
                        auto_topup_enabled = 0,
                        stripe_payment_method_id = NULL
                  WHERE id = $1",
                &[&attempt.account_id, &restriction_reason],
            )?;
            tx.commit()?;
            Ok(ReversalDisposition {
                account_id: attempt.account_id,
                revoked_cents,
                already_reversed,
            })
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::accounts::Account;

    fn temp_pool() -> DbPool {
        let path = std::env::temp_dir().join(format!(
            "bluey-stripe-auto-reload-{}.db",
            uuid::Uuid::new_v4()
        ));
        let pool = crate::db::open_pool(&path).unwrap();
        crate::db::run_migrations(&pool).unwrap();
        pool
    }

    fn eligible_account(pool: &DbPool, email: &str) -> Account {
        let account = Account::create(pool, email, "stub").unwrap();
        Account::mark_email_verified(pool, &account.id).unwrap();
        Account::save_stripe_checkout_refs(
            pool,
            &account.id,
            Some("cus_auto_reload"),
            Some("pm_auto_reload"),
        )
        .unwrap();
        Account::update_auto_topup_settings(pool, &account.id, true, 500, 1500)
            .unwrap()
            .unwrap()
    }

    fn attached_attempt(pool: &DbPool, email: &str, pi: &str) -> StripeAutoReloadAttempt {
        let account = eligible_account(pool, email);
        let attempt = reserve_if_eligible(pool, &account.id).unwrap().unwrap();
        attach_payment_intent(pool, &attempt.id, pi).unwrap()
    }

    fn bounded_postgres_pool(test_name: &str) -> Option<(DbPool, String)> {
        let database_url = std::env::var("BLUEY_TEST_POSTGRES_URL").ok()?;
        let pool = crate::db::open_postgres_pool(&database_url)
            .expect("open PostgreSQL Stripe Auto Reload test pool");
        crate::db::run_migrations(&pool)
            .expect("apply PostgreSQL Stripe Auto Reload test migrations");
        let application_name = format!("bluey-{test_name}-{}", uuid::Uuid::new_v4().simple());

        // Configure every primary-pool session so the production boundary, which checks out its
        // own connection, inherits the same bounded lock and statement timeouts.
        let mut connections = Vec::new();
        for _ in 0..crate::db::POSTGRES_PRIMARY_POOL_SIZE {
            let mut connection = pool
                .get_pg()
                .expect("get bounded PostgreSQL Stripe Auto Reload connection");
            connection
                .batch_execute(
                    "SET lock_timeout = '10s';
                     SET statement_timeout = '15s';",
                )
                .expect("bound PostgreSQL Stripe Auto Reload test session");
            connection
                .query_one(
                    "SELECT set_config('application_name', $1, false)",
                    &[&application_name],
                )
                .expect("name PostgreSQL Stripe Auto Reload test session");
            connections.push(connection);
        }
        drop(connections);
        Some((pool, application_name))
    }

    #[test]
    fn postgres_auto_reload_reversal_locks_account_before_credit_batch() {
        let source = include_str!("stripe_auto_reload.rs");
        let reverse_start = source
            .find("pub fn reverse_and_restrict")
            .expect("reverse_and_restrict source");
        let tests_start = source[reverse_start..]
            .find("\n#[cfg(test)]")
            .map(|offset| reverse_start + offset)
            .expect("reverse_and_restrict source boundary");
        let reverse_source = &source[reverse_start..tests_start];
        let postgres_start = reverse_source
            .find("DbPool::Postgres(_) =>")
            .expect("reverse_and_restrict PostgreSQL branch");
        let postgres_source = &reverse_source[postgres_start..];

        let account_lock = postgres_source
            .find("SELECT balance_cents FROM accounts WHERE id = $1 FOR UPDATE")
            .expect("PostgreSQL reversal account lock");
        let credit_batch_query = postgres_source
            .find("FROM credit_batches")
            .expect("PostgreSQL reversal credit-batch query");
        let credit_batch_update = postgres_source
            .find("UPDATE credit_batches SET remaining_cents = 0 WHERE id = $1")
            .expect("PostgreSQL reversal credit-batch update");

        assert_eq!(
            postgres_source
                .matches("SELECT balance_cents FROM accounts WHERE id = $1 FOR UPDATE")
                .count(),
            1,
            "PostgreSQL reversal must own one explicit account row fence"
        );
        assert!(
            account_lock < credit_batch_query && account_lock < credit_batch_update,
            "PostgreSQL reversal must lock Account before reading or updating credit_batches"
        );
    }

    #[test]
    #[serial_test::serial]
    fn postgres_auto_reload_reversal_and_account_metering_share_lock_order_when_configured() {
        let Some((pool, _application_name)) = bounded_postgres_pool("auto-reload-order") else {
            return;
        };
        let suffix = uuid::Uuid::new_v4().simple().to_string();
        let account_id = format!("acct-auto-reload-order-{suffix}");
        let email = format!("auto-reload-order-{suffix}@example.test");
        let attempt_id = format!("attempt-auto-reload-order-{suffix}");
        let payment_intent_id = format!("pi_auto_reload_order_{suffix}");
        let source_id = format!("stripe:{payment_intent_id}");
        let batch_id = format!("batch-auto-reload-order-{suffix}");
        let event_id = format!("evt_auto_reload_order_{suffix}");
        let customer_id = format!("cus_{suffix}");
        let payment_method_id = format!("pm_{suffix}");
        let create_idempotency_key = format!("create_{suffix}");
        let confirm_idempotency_key = format!("confirm_{suffix}");

        let mut setup = pool
            .get_pg()
            .expect("get PostgreSQL Stripe Auto Reload setup connection");
        setup
            .execute(
                "INSERT INTO accounts (
                    id, email, password_hash, email_verified_at, balance_cents,
                    trial_seconds_remaining, auto_topup_enabled,
                    auto_topup_threshold_cents, auto_topup_amount_cents,
                    stripe_customer_id, stripe_payment_method_id
                 ) VALUES ($1, $2, 'hash', now(), 1500, 0, 1, 500, 1500, $3, $4)",
                &[&account_id, &email, &customer_id, &payment_method_id],
            )
            .expect("insert PostgreSQL Stripe Auto Reload account fixture");
        setup
            .execute(
                "INSERT INTO credit_batches (
                    id, account_id, amount_cents, remaining_cents, expires_at,
                    stripe_charge_id
                 ) VALUES ($1, $2, 1500, 1500, now() + interval '30 days', $3)",
                &[&batch_id, &account_id, &source_id],
            )
            .expect("insert PostgreSQL Stripe Auto Reload credit-batch fixture");
        setup
            .execute(
                "INSERT INTO stripe_auto_reload_attempts (
                    id, account_id, amount_cents, stripe_customer_id,
                    stripe_payment_method_id, stripe_payment_intent_id,
                    create_idempotency_key, confirm_idempotency_key, status
                 ) VALUES ($1, $2, 1500, $3, $4, $5, $6, $7, 'succeeded')",
                &[
                    &attempt_id,
                    &account_id,
                    &customer_id,
                    &payment_method_id,
                    &payment_intent_id,
                    &create_idempotency_key,
                    &confirm_idempotency_key,
                ],
            )
            .expect("insert PostgreSQL Stripe Auto Reload attempt fixture");
        drop(setup);

        let mut metering_connection = pool
            .get_pg()
            .expect("get PostgreSQL account-metering connection");
        let mut metering = metering_connection
            .transaction()
            .expect("begin PostgreSQL account-metering transaction");
        let metering_pid = metering
            .query_one("SELECT pg_backend_pid()", &[])
            .expect("query PostgreSQL account-metering pid")
            .get::<_, i32>(0);
        metering
            .query_one(
                "SELECT balance_cents FROM accounts WHERE id = $1 FOR UPDATE",
                &[&account_id],
            )
            .expect("lock PostgreSQL account before credit batch");

        let reversal_pool = pool.clone();
        let reversal_attempt_id = attempt_id.clone();
        let reversal_event_id = event_id.clone();
        let (started_tx, started_rx) = std::sync::mpsc::channel();
        let (finished_tx, finished_rx) = std::sync::mpsc::channel();
        let reversal_worker = std::thread::spawn(move || {
            started_tx
                .send(())
                .expect("signal PostgreSQL Stripe reversal start");
            let result = reverse_and_restrict(
                &reversal_pool,
                &reversal_attempt_id,
                "refund.created",
                &reversal_event_id,
            );
            finished_tx
                .send(result)
                .expect("send PostgreSQL Stripe reversal result");
        });
        started_rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("PostgreSQL Stripe reversal worker started");

        let wait_deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let reversal_waited_on_account = loop {
            let waiting = metering
                .query_one(
                    "SELECT EXISTS (
                        SELECT 1
                          FROM pg_stat_activity AS activity
                         WHERE activity.pid <> $1
                           AND activity.wait_event_type = 'Lock'
                           AND $1 = ANY(pg_blocking_pids(activity.pid))
                     )",
                    &[&metering_pid],
                )
                .expect("observe PostgreSQL Stripe reversal account wait")
                .get::<_, bool>(0);
            if waiting {
                break true;
            }
            if reversal_worker.is_finished() || std::time::Instant::now() >= wait_deadline {
                break false;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        };
        assert!(
            reversal_waited_on_account,
            "Stripe reversal did not wait at the canonical account fence"
        );

        let observed_batch_id = metering
            .query_one(
                "SELECT id FROM credit_batches
                  WHERE account_id = $1 AND stripe_charge_id = $2
                  FOR UPDATE",
                &[&account_id, &source_id],
            )
            .expect("lock credit batch after account while Stripe reversal waits")
            .get::<_, String>(0);
        assert_eq!(observed_batch_id, batch_id);
        metering
            .execute(
                "UPDATE credit_batches
                    SET remaining_cents = remaining_cents
                  WHERE id = $1",
                &[&batch_id],
            )
            .expect("exercise account-to-credit-batch metering write");
        metering
            .commit()
            .expect("release account-to-credit-batch metering transaction");

        let reversal_result = finished_rx
            .recv_timeout(std::time::Duration::from_secs(10))
            .expect("Stripe reversal completes after account-metering commit");
        reversal_worker
            .join()
            .expect("join PostgreSQL Stripe reversal worker");
        let disposition = match reversal_result {
            Ok(disposition) => disposition,
            Err(error) => {
                let sqlstate = error
                    .downcast_ref::<postgres::Error>()
                    .and_then(postgres::Error::as_db_error)
                    .map(|error| error.code().code());
                assert_ne!(
                    sqlstate,
                    Some("40P01"),
                    "canonical Account -> credit_batches order must not deadlock"
                );
                panic!("PostgreSQL Stripe reversal failed: {error:#}");
            }
        };
        assert_eq!(disposition.account_id, account_id);
        assert_eq!(disposition.revoked_cents, 1500);
        assert!(!disposition.already_reversed);

        let mut assertion_connection = pool
            .get_pg()
            .expect("get PostgreSQL Stripe Auto Reload assertion connection");
        let state = assertion_connection
            .query_one(
                "SELECT account_row.balance_cents,
                        account_row.billing_restricted,
                        account_row.auto_topup_enabled,
                        account_row.stripe_payment_method_id,
                        attempt_row.status,
                        batch_row.remaining_cents
                   FROM accounts AS account_row
                   JOIN stripe_auto_reload_attempts AS attempt_row
                     ON attempt_row.account_id = account_row.id AND attempt_row.id = $2
                   JOIN credit_batches AS batch_row
                     ON batch_row.account_id = account_row.id AND batch_row.id = $3
                  WHERE account_row.id = $1",
                &[&account_id, &attempt_id, &batch_id],
            )
            .expect("query final PostgreSQL Stripe reversal state");
        assert_eq!(state.get::<_, i64>(0), 0);
        assert_eq!(state.get::<_, i32>(1), 1);
        assert_eq!(state.get::<_, i32>(2), 0);
        assert_eq!(state.get::<_, Option<String>>(3), None);
        assert_eq!(state.get::<_, String>(4), STATUS_REVERSED);
        assert_eq!(state.get::<_, i64>(5), 0);

        let ledger = assertion_connection
            .query(
                "SELECT amount_cents, balance_cents_before, balance_cents_after
                   FROM balance_ledger_entries
                  WHERE account_id = $1 AND event_type = 'processor_credit_revoked'",
                &[&account_id],
            )
            .expect("query PostgreSQL Stripe reversal ledger");
        assert_eq!(ledger.len(), 1);
        assert_eq!(ledger[0].get::<_, i64>(0), -1500);
        assert_eq!(ledger[0].get::<_, i64>(1), 1500);
        assert_eq!(ledger[0].get::<_, i64>(2), 0);

        assertion_connection
            .execute("DELETE FROM accounts WHERE id = $1", &[&account_id])
            .expect("delete PostgreSQL Stripe Auto Reload fixture");
    }

    #[test]
    fn reserve_is_durable_and_uses_current_balance() {
        let pool = temp_pool();
        let account = eligible_account(&pool, "reserve@example.com");
        let first = reserve_if_eligible(&pool, &account.id).unwrap().unwrap();
        let second = reserve_if_eligible(&pool, &account.id).unwrap().unwrap();
        assert_eq!(first.id, second.id);
        assert_eq!(first.create_idempotency_key, second.create_idempotency_key);

        mark_abandoned(&pool, &first.id, "test_abandon").unwrap();
        pool.get()
            .unwrap()
            .execute(
                "UPDATE accounts SET balance_cents = 500 WHERE id = ?1",
                params![account.id],
            )
            .unwrap();
        assert!(reserve_if_eligible(&pool, &account.id).unwrap().is_none());
    }

    #[test]
    fn success_credits_exactly_once() {
        let pool = temp_pool();
        let attempt = attached_attempt(&pool, "success@example.com", "pi_success");
        let first = credit_succeeded(
            &pool,
            &attempt.id,
            "pi_success",
            Some("ch_success"),
            Some("evt_success_1"),
        )
        .unwrap();
        let second = credit_succeeded(
            &pool,
            &attempt.id,
            "pi_success",
            Some("ch_success"),
            Some("evt_success_2"),
        )
        .unwrap();
        assert_eq!(first, CreditDisposition::Credited);
        assert_eq!(second, CreditDisposition::AlreadyCredited);

        let conn = pool.get().unwrap();
        let balance: i64 = conn
            .query_row(
                "SELECT balance_cents FROM accounts WHERE id = ?1",
                params![attempt.account_id],
                |row| row.get(0),
            )
            .unwrap();
        let batches: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM credit_batches WHERE stripe_charge_id = 'stripe:pi_success'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(balance, 1500);
        assert_eq!(batches, 1);
    }

    #[test]
    fn reversal_before_success_suppresses_late_credit() {
        let pool = temp_pool();
        let attempt = attached_attempt(&pool, "reverse-first@example.com", "pi_reverse_first");
        let reversed = reverse_and_restrict(
            &pool,
            &attempt.id,
            "charge.dispute.created",
            "evt_dispute_first",
        )
        .unwrap();
        assert_eq!(reversed.revoked_cents, 0);
        assert_eq!(
            credit_succeeded(
                &pool,
                &attempt.id,
                "pi_reverse_first",
                Some("ch_reverse_first"),
                Some("evt_success_late"),
            )
            .unwrap(),
            CreditDisposition::SuppressedAfterReversal
        );

        let account = Account::fetch_by_id(&pool, &attempt.account_id)
            .unwrap()
            .unwrap();
        assert_eq!(account.balance_cents, 0);
        assert!(account.billing_restricted);
        assert!(!account.auto_topup_enabled);
    }

    #[test]
    fn reversal_after_success_revokes_once() {
        let pool = temp_pool();
        let attempt = attached_attempt(&pool, "reverse-after@example.com", "pi_reverse_after");
        credit_succeeded(
            &pool,
            &attempt.id,
            "pi_reverse_after",
            Some("ch_reverse_after"),
            Some("evt_success"),
        )
        .unwrap();
        let first =
            reverse_and_restrict(&pool, &attempt.id, "refund.created", "evt_refund_1").unwrap();
        let second =
            reverse_and_restrict(&pool, &attempt.id, "refund.updated", "evt_refund_2").unwrap();
        assert_eq!(first.revoked_cents, 1500);
        assert_eq!(second.revoked_cents, 0);
        assert!(second.already_reversed);

        let account = Account::fetch_by_id(&pool, &attempt.account_id)
            .unwrap()
            .unwrap();
        assert_eq!(account.balance_cents, 0);
    }

    #[test]
    fn terminal_failure_disables_future_auto_reload() {
        let pool = temp_pool();
        let attempt = attached_attempt(&pool, "failure@example.com", "pi_failure");
        assert!(
            mark_failed_and_disable(&pool, &attempt.id, "card_declined", Some("evt_failed"))
                .unwrap()
        );
        let account = Account::fetch_by_id(&pool, &attempt.account_id)
            .unwrap()
            .unwrap();
        assert!(!account.auto_topup_enabled);
        assert!(account.stripe_payment_method_id.is_none());
        assert!(reserve_if_eligible(&pool, &attempt.account_id)
            .unwrap()
            .is_none());
    }
}
