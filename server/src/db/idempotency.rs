//! Idempotency layer for `/router/complete`.
//!
//! Codex Stage 4 S4.1: clients can retry after a network timeout/lost
//! response. Without (account_id, request_id) deduplication a retry
//! would dispatch the upstream call AND charge the customer twice.
//!
//! Lifecycle:
//!   1. `reserve(...)` — atomic INSERT with `status='in_progress'`.
//!      Returns FreshReservation if we won the race; CachedComplete if
//!      a previous attempt finished; InProgress if the original is
//!      still in-flight; CachedFailed if the previous attempt is
//!      terminally failed.
//!   2. After upstream dispatch + charging, call `mark_complete` to
//!      write the final response JSON for future retries.
//!   3. On error before charging, call `release` to clear the
//!      reservation (transient errors should be retryable).
//!   4. On terminal failure (insufficient balance, no pricing entry),
//!      call `mark_failed` so the customer must mint a new request_id.

use anyhow::Result;
use rusqlite::params;

use crate::db::DbPool;

#[derive(Debug, PartialEq, Eq)]
pub enum ReserveOutcome {
    FreshReservation,
    InProgress,
    CachedComplete(String),
    CachedFailed,
}

pub fn reserve(pool: &DbPool, account_id: &str, request_id: &str) -> Result<ReserveOutcome> {
    let conn = pool.get()?;

    // Try to INSERT a new in_progress row. If a row already exists for
    // (account_id, request_id), the INSERT silently does nothing.
    let inserted = conn.execute(
        "INSERT OR IGNORE INTO request_idempotency
            (account_id, request_id, status, created_at)
         VALUES (?1, ?2, 'in_progress', datetime('now'))",
        params![account_id, request_id],
    )?;

    if inserted == 1 {
        return Ok(ReserveOutcome::FreshReservation);
    }

    // Existing row: read it.
    let row: Option<(String, Option<String>)> = conn
        .query_row(
            "SELECT status, response_json FROM request_idempotency
             WHERE account_id = ?1 AND request_id = ?2",
            params![account_id, request_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .ok();

    match row {
        Some((status, json)) if status == "complete" => {
            if let Some(j) = json {
                Ok(ReserveOutcome::CachedComplete(j))
            } else {
                // Shouldn't happen — complete rows should have json.
                Ok(ReserveOutcome::CachedFailed)
            }
        }
        Some((status, _)) if status == "failed" => Ok(ReserveOutcome::CachedFailed),
        _ => Ok(ReserveOutcome::InProgress),
    }
}

pub fn mark_complete(
    pool: &DbPool,
    account_id: &str,
    request_id: &str,
    response_json: &str,
) -> Result<()> {
    let conn = pool.get()?;
    conn.execute(
        "UPDATE request_idempotency
            SET status = 'complete',
                response_json = ?3,
                http_status = 200,
                completed_at = datetime('now')
          WHERE account_id = ?1 AND request_id = ?2",
        params![account_id, request_id, response_json],
    )?;
    Ok(())
}

pub fn mark_failed(pool: &DbPool, account_id: &str, request_id: &str) -> Result<()> {
    let conn = pool.get()?;
    conn.execute(
        "UPDATE request_idempotency
            SET status = 'failed', completed_at = datetime('now')
          WHERE account_id = ?1 AND request_id = ?2",
        params![account_id, request_id],
    )?;
    Ok(())
}

/// Release the reservation so the same request_id is retryable. Used
/// for transient upstream errors (502 etc) where the customer should
/// not be forced to mint a new id.
pub fn release(pool: &DbPool, account_id: &str, request_id: &str) -> Result<()> {
    let conn = pool.get()?;
    conn.execute(
        "DELETE FROM request_idempotency
          WHERE account_id = ?1 AND request_id = ?2 AND status = 'in_progress'",
        params![account_id, request_id],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_pool() -> DbPool {
        let path = std::env::temp_dir().join(format!("bluey-idem-{}.db", uuid::Uuid::new_v4()));
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
    fn fresh_reservation_then_in_progress_on_replay() {
        let pool = temp_pool();
        let id = make_account(&pool, "idem1@example.com");
        let r = reserve(&pool, &id, "req-1").unwrap();
        assert_eq!(r, ReserveOutcome::FreshReservation);
        let r = reserve(&pool, &id, "req-1").unwrap();
        assert_eq!(r, ReserveOutcome::InProgress);
    }

    #[test]
    fn cached_complete_returns_payload_on_replay() {
        let pool = temp_pool();
        let id = make_account(&pool, "idem2@example.com");
        reserve(&pool, &id, "req-2").unwrap();
        mark_complete(&pool, &id, "req-2", r#"{"text":"hi"}"#).unwrap();
        let r = reserve(&pool, &id, "req-2").unwrap();
        match r {
            ReserveOutcome::CachedComplete(json) => assert_eq!(json, r#"{"text":"hi"}"#),
            other => panic!("expected CachedComplete, got {other:?}"),
        }
    }

    #[test]
    fn release_allows_retry() {
        let pool = temp_pool();
        let id = make_account(&pool, "idem3@example.com");
        reserve(&pool, &id, "req-3").unwrap();
        release(&pool, &id, "req-3").unwrap();
        let r = reserve(&pool, &id, "req-3").unwrap();
        assert_eq!(r, ReserveOutcome::FreshReservation);
    }

    #[test]
    fn failed_is_terminal() {
        let pool = temp_pool();
        let id = make_account(&pool, "idem4@example.com");
        reserve(&pool, &id, "req-4").unwrap();
        mark_failed(&pool, &id, "req-4").unwrap();
        let r = reserve(&pool, &id, "req-4").unwrap();
        assert_eq!(r, ReserveOutcome::CachedFailed);
    }

    #[test]
    fn different_accounts_share_request_id_namespace_safely() {
        let pool = temp_pool();
        let a = make_account(&pool, "a@example.com");
        let b = make_account(&pool, "b@example.com");
        let r1 = reserve(&pool, &a, "shared").unwrap();
        let r2 = reserve(&pool, &b, "shared").unwrap();
        assert_eq!(r1, ReserveOutcome::FreshReservation);
        assert_eq!(r2, ReserveOutcome::FreshReservation);
    }
}
