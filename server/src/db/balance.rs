//! Atomic balance + per-batch credit accounting.
//!
//! Bluey's "no debt" guarantee comes from:
//!   1. entry check: balance >= estimated_cost before starting a request
//!   2. atomic deduction: SQL UPDATE with a WHERE balance_cents >= cost
//!      so the UPDATE returns 0 rows if a concurrent request already
//!      drained the balance (race-free)
//!   3. mid-stream cut: see routing/dispatcher.rs (server eats overrun)
//!
//! Per-batch credit tracking exists so a $30 reload that happens 11
//! months ago expires while a fresh reload from yesterday continues
//! living. FIFO consumption — oldest batch drains first, so unused
//! batches survive their full year.

use anyhow::Result;
use chrono::{Duration, Utc};
use rusqlite::params;

use crate::db::DbPool;

/// Atomically deduct `cost_cents` from `account_id`'s balance. Returns
/// `Ok(true)` on successful deduction; `Ok(false)` when the deduction
/// failed because of insufficient balance OR a concurrent deduction
/// already drained it.
pub fn deduct(pool: &DbPool, account_id: &str, cost_cents: i64) -> Result<bool> {
    if cost_cents < 0 {
        anyhow::bail!("cost_cents must be non-negative");
    }
    if cost_cents == 0 {
        return Ok(true);
    }
    let conn = pool.get()?;
    let updated = conn.execute(
        "UPDATE accounts SET balance_cents = balance_cents - ?1
         WHERE id = ?2 AND balance_cents >= ?1",
        params![cost_cents, account_id],
    )?;
    Ok(updated > 0)
}

/// Add `amount_cents` to the account's balance AND record a credit
/// batch with `expires_at = now + 365 days`. Stripe charge id is
/// optional (free top-ups for support cases, etc.).
pub fn credit(
    pool: &DbPool,
    account_id: &str,
    amount_cents: i64,
    stripe_charge_id: Option<&str>,
) -> Result<()> {
    if amount_cents <= 0 {
        anyhow::bail!("amount_cents must be positive");
    }
    let conn = pool.get()?;
    let batch_id = uuid::Uuid::new_v4().to_string();
    let expires_at = (Utc::now() + Duration::days(365)).to_rfc3339();

    conn.execute(
        "INSERT INTO credit_batches
            (id, account_id, amount_cents, remaining_cents, expires_at, stripe_charge_id)
         VALUES (?1, ?2, ?3, ?3, ?4, ?5)",
        params![batch_id, account_id, amount_cents, expires_at, stripe_charge_id],
    )?;

    conn.execute(
        "UPDATE accounts SET balance_cents = balance_cents + ?1 WHERE id = ?2",
        params![amount_cents, account_id],
    )?;

    Ok(())
}

/// Check whether a request with `estimated_cost_cents` would clear the
/// balance check. Used by the entry check before starting a streaming
/// upstream request.
pub fn can_afford(pool: &DbPool, account_id: &str, estimated_cost_cents: i64) -> Result<bool> {
    let conn = pool.get()?;
    let balance: i64 =
        conn.query_row("SELECT balance_cents FROM accounts WHERE id = ?1", params![account_id], |r| {
            r.get(0)
        })?;
    Ok(balance >= estimated_cost_cents)
}

/// Sweep expired credit batches: any batch with `expires_at < now` and
/// `remaining_cents > 0` AND `expired_at IS NULL` is debited from the
/// account balance and marked expired. Idempotent.
///
/// Run once a day from a cron task (or on-demand from admin).
pub fn sweep_expired(pool: &DbPool) -> Result<i64> {
    let conn = pool.get()?;
    let now = Utc::now().to_rfc3339();
    // Find expired batches.
    let mut stmt = conn.prepare(
        "SELECT id, account_id, remaining_cents
         FROM credit_batches
         WHERE expires_at < ?1 AND remaining_cents > 0 AND expired_at IS NULL",
    )?;
    let rows: Vec<(String, String, i64)> = stmt
        .query_map(params![now], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?))
        })?
        .filter_map(|r| r.ok())
        .collect();

    let mut total_swept_cents: i64 = 0;
    for (batch_id, account_id, remaining) in rows {
        // Debit account.
        conn.execute(
            "UPDATE accounts SET balance_cents = MAX(0, balance_cents - ?1) WHERE id = ?2",
            params![remaining, account_id],
        )?;
        // Mark batch expired.
        conn.execute(
            "UPDATE credit_batches
                 SET expired_at = ?1, remaining_cents = 0
                 WHERE id = ?2",
            params![now, batch_id],
        )?;
        total_swept_cents += remaining;
        tracing::info!(
            account_id,
            batch_id,
            remaining_cents = remaining,
            "credit batch expired"
        );
    }
    Ok(total_swept_cents)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_pool() -> DbPool {
        let path = std::env::temp_dir().join(format!(
            "bluey-test-{}.db",
            uuid::Uuid::new_v4()
        ));
        let pool = crate::db::open_pool(&path).unwrap();
        crate::db::run_migrations(&pool).unwrap();
        pool
    }

    fn make_account(pool: &DbPool, email: &str) -> String {
        crate::db::accounts::Account::create(pool, email, "stub-hash")
            .unwrap()
            .id
    }

    #[test]
    fn deduct_succeeds_with_sufficient_balance() {
        let pool = temp_pool();
        let id = make_account(&pool, "a@example.com");
        credit(&pool, &id, 1000, None).unwrap();
        assert!(deduct(&pool, &id, 200).unwrap());
        assert!(can_afford(&pool, &id, 700).unwrap());
        assert!(!can_afford(&pool, &id, 900).unwrap());
    }

    #[test]
    fn deduct_fails_with_insufficient_balance() {
        let pool = temp_pool();
        let id = make_account(&pool, "b@example.com");
        credit(&pool, &id, 100, None).unwrap();
        assert!(!deduct(&pool, &id, 200).unwrap());
        // balance unchanged
        assert!(can_afford(&pool, &id, 100).unwrap());
    }

    #[test]
    fn credit_extends_expiry_by_365_days() {
        let pool = temp_pool();
        let id = make_account(&pool, "c@example.com");
        credit(&pool, &id, 3000, Some("ch_test_1")).unwrap();
        // Verify a credit_batches row exists with the right amount + expiry > now.
        let conn = pool.get().unwrap();
        let (remaining, expires_at): (i64, String) = conn
            .query_row(
                "SELECT remaining_cents, expires_at FROM credit_batches WHERE account_id = ?1",
                params![&id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(remaining, 3000);
        let parsed: chrono::DateTime<chrono::Utc> = expires_at.parse().unwrap();
        let days_until_expiry = (parsed - chrono::Utc::now()).num_days();
        assert!((360..=366).contains(&days_until_expiry));
    }

    #[test]
    fn sweep_does_nothing_when_no_batches_expired() {
        let pool = temp_pool();
        let id = make_account(&pool, "d@example.com");
        credit(&pool, &id, 3000, None).unwrap();
        let swept = sweep_expired(&pool).unwrap();
        assert_eq!(swept, 0);
    }
}
