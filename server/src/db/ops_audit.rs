//! Redacted operations audit events for exports, deletes, and owner support.

use anyhow::Result;
use rusqlite::params;

use crate::db::DbPool;

#[derive(Debug, Clone)]
pub struct OpsAuditEventInput {
    pub account_id_hash: Option<String>,
    pub actor_account_id_hash: Option<String>,
    pub event_type: String,
    pub status: String,
    pub metadata_json: serde_json::Value,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct OpsAuditEvent {
    pub id: String,
    pub account_id_hash: Option<String>,
    pub actor_account_id_hash: Option<String>,
    pub event_type: String,
    pub status: String,
    pub metadata_json: serde_json::Value,
    pub created_at: String,
}

pub fn record_event(pool: &DbPool, input: OpsAuditEventInput) -> Result<()> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => record_event_sqlite(pool, input),
        DbPool::Postgres(_) => record_event_postgres(pool, input),
    })
}

pub fn recent_events(pool: &DbPool, limit: i64) -> Result<Vec<OpsAuditEvent>> {
    let limit = limit.clamp(1, 500);
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => recent_events_sqlite(pool, limit),
        DbPool::Postgres(_) => recent_events_postgres(pool, limit),
    })
}

fn record_event_sqlite(pool: &DbPool, input: OpsAuditEventInput) -> Result<()> {
    let conn = pool.get()?;
    conn.execute(
        "INSERT INTO ops_audit_events
            (id, account_id_hash, actor_account_id_hash, event_type, status, metadata_json)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            uuid::Uuid::new_v4().to_string(),
            input.account_id_hash,
            input.actor_account_id_hash,
            input.event_type,
            input.status,
            input.metadata_json.to_string()
        ],
    )?;
    Ok(())
}

fn record_event_postgres(pool: &DbPool, input: OpsAuditEventInput) -> Result<()> {
    let mut conn = pool.get_pg()?;
    let id = uuid::Uuid::new_v4().to_string();
    let metadata_json = input.metadata_json.to_string();
    conn.execute(
        "INSERT INTO ops_audit_events
            (id, account_id_hash, actor_account_id_hash, event_type, status, metadata_json)
         VALUES ($1, $2, $3, $4, $5, $6)",
        &[
            &id,
            &input.account_id_hash,
            &input.actor_account_id_hash,
            &input.event_type,
            &input.status,
            &metadata_json,
        ],
    )?;
    Ok(())
}

fn recent_events_sqlite(pool: &DbPool, limit: i64) -> Result<Vec<OpsAuditEvent>> {
    let conn = pool.get()?;
    let mut stmt = conn.prepare(
        "SELECT id, account_id_hash, actor_account_id_hash, event_type,
                status, metadata_json, created_at
         FROM ops_audit_events
         ORDER BY created_at DESC
         LIMIT ?1",
    )?;
    let rows = stmt.query_map(params![limit], |row| {
        let metadata: String = row.get(5)?;
        Ok(OpsAuditEvent {
            id: row.get(0)?,
            account_id_hash: row.get(1)?,
            actor_account_id_hash: row.get(2)?,
            event_type: row.get(3)?,
            status: row.get(4)?,
            metadata_json: parse_json(&metadata),
            created_at: row.get(6)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

fn recent_events_postgres(pool: &DbPool, limit: i64) -> Result<Vec<OpsAuditEvent>> {
    let mut conn = pool.get_pg()?;
    let rows = conn.query(
        "SELECT id, account_id_hash, actor_account_id_hash, event_type,
                status, metadata_json, created_at::text
         FROM ops_audit_events
         ORDER BY created_at DESC
         LIMIT $1",
        &[&limit],
    )?;
    let mut events = Vec::with_capacity(rows.len());
    for row in rows {
        let metadata: String = row.try_get(5)?;
        events.push(OpsAuditEvent {
            id: row.try_get(0)?,
            account_id_hash: row.try_get(1)?,
            actor_account_id_hash: row.try_get(2)?,
            event_type: row.try_get(3)?,
            status: row.try_get(4)?,
            metadata_json: parse_json(&metadata),
            created_at: row.try_get(6)?,
        });
    }
    Ok(events)
}

fn parse_json(value: &str) -> serde_json::Value {
    serde_json::from_str(value).unwrap_or_else(|_| serde_json::json!({}))
}
