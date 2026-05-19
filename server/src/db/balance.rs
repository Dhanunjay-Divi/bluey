//! Atomic balance + per-batch credit accounting (FIFO).

use anyhow::Result;
use chrono::{Duration, Utc};
use rusqlite::params;

use crate::db::DbPool;

/// Atomically deduct `cost_cents` from the account's balance AND from
/// the oldest non-expired credit batch (FIFO consumption).
///
/// Returns Ok(true) on success, Ok(false) on insufficient balance or
/// race-loss to a concurrent deduction.
pub fn deduct(pool: &DbPool, account_id: &str, cost_cents: i64) -> Result<bool> {
    if cost_cents < 0 {
        anyhow::bail!("cost_cents must be non-negative");
    }
    if cost_cents == 0 {
        return Ok(true);
    }
    let mut conn = pool.get()?;
    let tx = conn.transaction()?;

    // Atomic balance check + deduction.
    let updated = tx.execute(
        "UPDATE accounts SET balance_cents = balance_cents - ?1
         WHERE id = ?2 AND balance_cents >= ?1",
        params![cost_cents, account_id],
    )?;
    if updated == 0 {
        return Ok(false);
    }

    // FIFO consumption: drain the oldest unexpired batch first.
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
    // balance check passed) we let `remaining > 0` fall through; the
    // accounts.balance_cents is the canonical source of truth.

    tx.commit()?;
    Ok(true)
}

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
        params![
            batch_id,
            account_id,
            amount_cents,
            expires_at,
            stripe_charge_id
        ],
    )?;

    conn.execute(
        "UPDATE accounts SET balance_cents = balance_cents + ?1 WHERE id = ?2",
        params![amount_cents, account_id],
    )?;

    Ok(())
}

pub fn can_afford(pool: &DbPool, account_id: &str, estimated_cost_cents: i64) -> Result<bool> {
    let conn = pool.get()?;
    let balance: i64 = conn.query_row(
        "SELECT balance_cents FROM accounts WHERE id = ?1",
        params![account_id],
        |r| r.get(0),
    )?;
    Ok(balance >= estimated_cost_cents)
}

pub fn current_balance(pool: &DbPool, account_id: &str) -> Result<i64> {
    let conn = pool.get()?;
    Ok(conn.query_row(
        "SELECT balance_cents FROM accounts WHERE id = ?1",
        params![account_id],
        |r| r.get(0),
    )?)
}

/// Decrement trial seconds. Returns the remaining count.
pub fn consume_trial_seconds(pool: &DbPool, account_id: &str, ms: i64) -> Result<i64> {
    let secs = (ms / 1000).max(0);
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

pub fn sweep_expired(pool: &DbPool) -> Result<i64> {
    let conn = pool.get()?;
    let now = Utc::now().to_rfc3339();
    let mut stmt = conn.prepare(
        "SELECT id, account_id, remaining_cents
         FROM credit_batches
         WHERE expires_at < ?1 AND remaining_cents > 0 AND expired_at IS NULL",
    )?;
    let rows: Vec<(String, String, i64)> = stmt
        .query_map(params![now], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
            ))
        })?
        .filter_map(|r| r.ok())
        .collect();

    let mut total: i64 = 0;
    for (batch_id, account_id, remaining) in rows {
        conn.execute(
            "UPDATE accounts SET balance_cents = MAX(0, balance_cents - ?1) WHERE id = ?2",
            params![remaining, account_id],
        )?;
        conn.execute(
            "UPDATE credit_batches SET expired_at = ?1, remaining_cents = 0 WHERE id = ?2",
            params![now, batch_id],
        )?;
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
        credit(&pool, &id, 1000, Some("ch_1")).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        credit(&pool, &id, 2000, Some("ch_2")).unwrap();
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
        credit(&pool, &id, 100, None).unwrap();
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
    fn credit_extends_expiry_by_365_days() {
        let pool = temp_pool();
        let id = make_account(&pool, "expiry@example.com");
        credit(&pool, &id, 3000, None).unwrap();
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
        assert!((360..=366).contains(&days));
    }

    #[test]
    fn trial_seconds_decrement_to_zero() {
        let pool = temp_pool();
        let id = make_account(&pool, "trial@example.com");
        // start: 600s
        let r = consume_trial_seconds(&pool, &id, 250_000).unwrap();
        assert_eq!(r, 350);
        let r = consume_trial_seconds(&pool, &id, 1_000_000).unwrap();
        assert_eq!(r, 0);
        let r = consume_trial_seconds(&pool, &id, 500_000).unwrap();
        assert_eq!(r, 0); // never goes negative
    }

    #[test]
    fn sweep_no_op_when_nothing_expired() {
        let pool = temp_pool();
        let id = make_account(&pool, "fresh@example.com");
        credit(&pool, &id, 3000, None).unwrap();
        assert_eq!(sweep_expired(&pool).unwrap(), 0);
    }
}
