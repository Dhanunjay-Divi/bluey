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

pub fn record(pool: &DbPool, account_id: &str, event: &UsageEvent) -> Result<()> {
    let id = uuid::Uuid::new_v4().to_string();
    let conn = pool.get()?;
    conn.execute(
        "INSERT INTO usage_events
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
    Ok(())
}
