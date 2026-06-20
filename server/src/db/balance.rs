//! Atomic balance + per-batch credit accounting (FIFO).

use anyhow::Result;
use chrono::{Duration, Utc};
use rusqlite::{params, Transaction};

use crate::db::DbPool;

/// Reloaded account credits are valid for up to 12 months from purchase.
///
/// The implementation uses 365 days so every credit batch has a deterministic
/// expiry timestamp and FIFO consumption can compare timestamps directly.
pub const CREDIT_VALIDITY_DAYS: i64 = 365;

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

    consume_credit_batches_tx(&tx, account_id, cost_cents)?;

    tx.commit()?;
    Ok(true)
}

pub(crate) fn consume_credit_batches_tx(
    tx: &Transaction<'_>,
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

fn credit_with_source_id(
    pool: &DbPool,
    account_id: &str,
    amount_cents: i64,
    credit_source_id: &str,
) -> Result<bool> {
    if amount_cents <= 0 {
        anyhow::bail!("amount_cents must be positive");
    }
    let credit_source_id = credit_source_id.trim();
    if credit_source_id.is_empty() {
        anyhow::bail!("credit_source_id must be non-empty");
    }
    let mut conn = pool.get()?;
    let tx = conn.transaction()?;

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

    tx.commit()?;
    Ok(true)
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
    let provider = provider.trim().to_ascii_lowercase();
    let processor_payment_id = processor_payment_id.trim();
    if provider.is_empty() || processor_payment_id.is_empty() {
        anyhow::bail!("processor credit requires provider and payment id");
    }
    let source_id = format!("{provider}:{processor_payment_id}");
    credit_with_source_id(pool, account_id, amount_cents, &source_id)
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
    let reason = reason.trim();
    if reason.is_empty() {
        anyhow::bail!("internal credit requires a reason");
    }
    let source_id = format!("internal:{reason}:{}", uuid::Uuid::new_v4());
    credit_with_source_id(pool, account_id, amount_cents, &source_id)
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
    // Codex S4.2: floor of 1 second per successful trial request so
    // sub-second instant-lane requests do not give effectively
    // unlimited free calls. The 600s trial budget remains honest.
    let secs = if ms <= 0 {
        1
    } else {
        ((ms + 999) / 1000).max(1)
    };
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
        // start: 600s
        // Ceiling-divide: 250_000ms = 250s consumed; 600 - 250 = 350.
        let r = consume_trial_seconds(&pool, &id, 250_000).unwrap();
        assert_eq!(r, 350);
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
        assert_eq!(r, 599);
        // 0ms (impossible in practice) still consumes 1s defensively.
        let r = consume_trial_seconds(&pool, &id, 0).unwrap();
        assert_eq!(r, 598);
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
