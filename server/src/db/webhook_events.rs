//! Processor webhook event receipt tracking.

use anyhow::Result;
use rusqlite::params;

use crate::db::DbPool;

pub fn processed(pool: &DbPool, event_id: &str) -> Result<bool> {
    match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get()?;
            let processed_at: Option<String> = conn
                .query_row(
                    "SELECT processed_at FROM stripe_webhook_events WHERE event_id = ?1",
                    params![event_id],
                    |row| row.get::<_, Option<String>>(0),
                )
                .ok()
                .flatten();
            Ok(processed_at.is_some())
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let processed = conn
                .query_opt(
                    "SELECT processed_at IS NOT NULL FROM stripe_webhook_events WHERE event_id = $1",
                    &[&event_id],
                )?
                .map(|row| row.try_get::<_, bool>(0))
                .transpose()?
                .unwrap_or(false);
            Ok(processed)
        }
    }
}

pub fn record_received(pool: &DbPool, event_id: &str, event_type: &str, body: &str) -> Result<()> {
    match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get()?;
            conn.execute(
                "INSERT OR IGNORE INTO stripe_webhook_events (event_id, type, body)
                 VALUES (?1, ?2, ?3)",
                params![event_id, event_type, body],
            )?;
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            conn.execute(
                "INSERT INTO stripe_webhook_events (event_id, type, body)
                 VALUES ($1, $2, $3)
                 ON CONFLICT (event_id) DO NOTHING",
                &[&event_id, &event_type, &body],
            )?;
        }
    }
    Ok(())
}

pub fn mark_processed(pool: &DbPool, event_id: &str) -> Result<usize> {
    match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get()?;
            Ok(conn.execute(
                "UPDATE stripe_webhook_events SET processed_at = datetime('now') WHERE event_id = ?1",
                params![event_id],
            )?)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let affected = conn.execute(
                "UPDATE stripe_webhook_events SET processed_at = now() WHERE event_id = $1",
                &[&event_id],
            )?;
            Ok(usize::try_from(affected).unwrap_or(usize::MAX))
        }
    }
}
