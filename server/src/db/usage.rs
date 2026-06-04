//! Usage event ingestion + aggregation. Stub for now; the daemon emits
//! events via POST /usage/event after every cue request.

use anyhow::Result;
use rusqlite::params;
use serde::{Deserialize, Serialize};

use crate::db::DbPool;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageEvent {
    pub request_id: String,
    pub kind: String,
    pub task_type: Option<String>,
    pub lane: Option<String>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub latency_ms: i64,
    pub cost_cents_to_bluey: i64,
    pub cost_cents_to_customer: i64,
    pub was_speculative: bool,
    pub was_fallback: bool,
}

pub fn record(pool: &DbPool, account_id: &str, event: &UsageEvent) -> Result<bool> {
    // Codex Stage 7 S7.1: idempotent ingestion. INSERT OR IGNORE
    // returns 0 affected rows when (account_id, request_id, kind)
    // already exists; we surface that as Ok(false) so callers can log
    // the dedup without treating it as an error.
    let id = uuid::Uuid::new_v4().to_string();
    let conn = pool.get()?;
    let inserted = conn.execute(
        "INSERT OR IGNORE INTO usage_events
            (id, account_id, request_id, kind, task_type, lane, provider, model,
             input_tokens, output_tokens, latency_ms,
             cost_cents_to_bluey, cost_cents_to_customer,
             was_speculative, was_fallback)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
        params![
            id,
            account_id,
            event.request_id,
            event.kind,
            event.task_type,
            event.lane,
            event.provider,
            event.model,
            event.input_tokens,
            event.output_tokens,
            event.latency_ms,
            event.cost_cents_to_bluey,
            event.cost_cents_to_customer,
            event.was_speculative as i64,
            event.was_fallback as i64,
        ],
    )?;
    Ok(inserted == 1)
}

pub fn bluey_spend_cents_in_window(pool: &DbPool, window_hours: i64) -> Result<i64> {
    let conn = pool.get()?;
    let total = if window_hours > 0 {
        let window = format!("-{window_hours} hours");
        conn.query_row(
            "SELECT COALESCE(SUM(cost_cents_to_bluey), 0)
               FROM usage_events
              WHERE ts >= datetime('now', ?1)",
            params![window],
            |row| row.get(0),
        )?
    } else {
        conn.query_row(
            "SELECT COALESCE(SUM(cost_cents_to_bluey), 0) FROM usage_events",
            [],
            |row| row.get(0),
        )?
    };
    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{open_pool, run_migrations, DbPool};

    fn temp_pool() -> DbPool {
        let path = std::env::temp_dir().join(format!("bluey-usage-{}.db", uuid::Uuid::new_v4()));
        let pool = open_pool(&path).unwrap();
        run_migrations(&pool).unwrap();
        pool
    }

    fn make_account(pool: &DbPool) -> String {
        crate::db::accounts::Account::create(pool, "usage@example.com", "stub")
            .unwrap()
            .id
    }

    fn sample_event(req_id: &str) -> UsageEvent {
        UsageEvent {
            request_id: req_id.into(),
            kind: "llm".into(),
            task_type: Some("general".into()),
            lane: Some("instant".into()),
            provider: Some("openai".into()),
            model: Some("gpt-4o-mini".into()),
            input_tokens: 100,
            output_tokens: 50,
            latency_ms: 250,
            cost_cents_to_bluey: 1,
            cost_cents_to_customer: 1,
            was_speculative: false,
            was_fallback: false,
        }
    }

    #[test]
    fn record_idempotent_on_same_request_id() {
        let pool = temp_pool();
        let id = make_account(&pool);
        let e = sample_event("req-1");
        assert!(record(&pool, &id, &e).unwrap());
        assert!(!record(&pool, &id, &e).unwrap()); // dedup
                                                   // Verify only one row.
        let conn = pool.get().unwrap();
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM usage_events WHERE account_id = ?1 AND request_id = ?2",
                params![&id, "req-1"],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 1);
    }

    #[test]
    fn different_kinds_per_request_id_allowed() {
        // Codex Stage 7: same request_id + different kind is OK
        // (e.g. llm + embed on the same logical request).
        let pool = temp_pool();
        let id = make_account(&pool);
        let mut a = sample_event("req-2");
        a.kind = "llm".into();
        let mut b = sample_event("req-2");
        b.kind = "embed".into();
        assert!(record(&pool, &id, &a).unwrap());
        assert!(record(&pool, &id, &b).unwrap());
    }

    #[test]
    fn bluey_spend_cents_in_window_sums_recent_provider_cost() {
        let pool = temp_pool();
        let id = make_account(&pool);
        let mut recent = sample_event("req-recent");
        recent.cost_cents_to_bluey = 7;
        let mut old = sample_event("req-old");
        old.cost_cents_to_bluey = 11;

        assert!(record(&pool, &id, &recent).unwrap());
        assert!(record(&pool, &id, &old).unwrap());
        pool.get()
            .unwrap()
            .execute(
                "UPDATE usage_events SET ts = datetime('now', '-2 days') WHERE request_id = ?1",
                params!["req-old"],
            )
            .unwrap();

        assert_eq!(bluey_spend_cents_in_window(&pool, 24).unwrap(), 7);
        assert_eq!(bluey_spend_cents_in_window(&pool, 0).unwrap(), 18);
    }
}
