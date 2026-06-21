//! Metrics read model.

use anyhow::Result;

use crate::db::DbPool;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MetricsSnapshot {
    pub accounts: i64,
    pub balance_sum: i64,
    pub trial_active: i64,
    pub request_idempotency_total: i64,
    pub request_idempotency_complete: i64,
    pub request_idempotency_in_progress: i64,
    pub mark_complete_failed: i64,
    pub credit_batches: i64,
    pub usage_24h: i64,
    pub webhook_processed: i64,
}

pub fn snapshot(pool: &DbPool) -> Result<MetricsSnapshot> {
    let conn = pool.get()?;
    Ok(MetricsSnapshot {
        accounts: count_one(&conn, "SELECT COUNT(*) FROM accounts")?,
        balance_sum: count_one(&conn, "SELECT COALESCE(SUM(balance_cents), 0) FROM accounts")?,
        trial_active: count_one(
            &conn,
            "SELECT COUNT(*) FROM accounts WHERE trial_seconds_remaining > 0",
        )?,
        request_idempotency_total: count_one(&conn, "SELECT COUNT(*) FROM request_idempotency")?,
        request_idempotency_complete: count_one(
            &conn,
            "SELECT COUNT(*) FROM request_idempotency WHERE status = 'complete'",
        )?,
        request_idempotency_in_progress: count_one(
            &conn,
            "SELECT COUNT(*) FROM request_idempotency WHERE status = 'in_progress'",
        )?,
        mark_complete_failed: count_one(
            &conn,
            "SELECT COUNT(*) FROM request_idempotency
             WHERE status = 'in_progress'
               AND created_at < datetime('now', '-5 minutes')",
        )?,
        credit_batches: count_one(&conn, "SELECT COUNT(*) FROM credit_batches")?,
        usage_24h: count_one(
            &conn,
            "SELECT COUNT(*) FROM usage_events WHERE ts >= datetime('now', '-1 day')",
        )?,
        webhook_processed: count_one(
            &conn,
            "SELECT COUNT(*) FROM stripe_webhook_events WHERE processed_at IS NOT NULL",
        )?,
    })
}

fn count_one(conn: &rusqlite::Connection, sql: &str) -> Result<i64> {
    Ok(conn.query_row(sql, [], |row| row.get::<_, i64>(0))?)
}
