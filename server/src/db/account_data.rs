//! Account dashboard/export/delete read models.

use anyhow::{Context, Result};
use postgres::Row as PgRow;
use rusqlite::{params, OptionalExtension};

use crate::db::DbPool;

#[derive(Debug, Clone, PartialEq)]
pub struct UsageSummary {
    pub total_cues: i64,
    pub total_cents_spent: i64,
    pub mix: Vec<UsageMixRow>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsageMixRow {
    pub task_type: String,
    pub count: i64,
    pub cost_cents: i64,
}

#[derive(Debug, serde::Serialize)]
pub struct ExportBundle {
    pub account: ExportAccount,
    pub credit_batches: Vec<serde_json::Value>,
    pub usage_events: Vec<serde_json::Value>,
    pub cloud_sessions: Vec<serde_json::Value>,
    pub cloud_transcript_segments: Vec<serde_json::Value>,
    pub cloud_cue_responses: Vec<serde_json::Value>,
    pub cloud_context_artifacts: Vec<serde_json::Value>,
    pub cloud_rag_chunks_count: i64,
    pub refresh_tokens_count: i64,
    pub stripe_webhook_events_count: i64,
    pub exported_at: String,
}

#[derive(Debug, serde::Serialize)]
pub struct ExportAccount {
    pub id: String,
    pub email: String,
    pub balance_cents: i64,
    pub trial_seconds_remaining: i64,
    pub created_at: Option<String>,
    pub last_login_at: Option<String>,
    pub stripe_customer_id: Option<String>,
    pub stripe_payment_method_id: Option<String>,
    pub square_customer_id: Option<String>,
    pub square_card_id: Option<String>,
    pub square_card_brand: Option<String>,
    pub square_card_last4: Option<String>,
}

pub fn usage_summary(pool: &DbPool, account_id: &str) -> Result<UsageSummary> {
    match pool {
        DbPool::Sqlite(_) => usage_summary_sqlite(pool, account_id),
        DbPool::Postgres(_) => usage_summary_postgres(pool, account_id),
    }
}

pub fn export_bundle(pool: &DbPool, account_id: &str) -> Result<Option<ExportBundle>> {
    match pool {
        DbPool::Sqlite(_) => export_bundle_sqlite(pool, account_id),
        DbPool::Postgres(_) => export_bundle_postgres(pool, account_id),
    }
}

pub fn hard_delete_account(pool: &DbPool, account_id: &str) -> Result<bool> {
    match pool {
        DbPool::Sqlite(_) => hard_delete_account_sqlite(pool, account_id),
        DbPool::Postgres(_) => hard_delete_account_postgres(pool, account_id),
    }
}

fn usage_summary_sqlite(pool: &DbPool, account_id: &str) -> Result<UsageSummary> {
    let conn = pool.get()?;
    let (total_cues, total_cents_spent): (i64, i64) = conn.query_row(
        "SELECT COUNT(*), COALESCE(SUM(cost_cents_to_customer), 0)
         FROM usage_events
         WHERE account_id = ?1 AND ts >= datetime('now', '-7 days')",
        params![account_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;

    let mut stmt = conn.prepare(
        "SELECT COALESCE(task_type, lane, 'general') AS bucket,
                COUNT(*) AS cnt,
                COALESCE(SUM(cost_cents_to_customer), 0) AS cost
         FROM usage_events
         WHERE account_id = ?1 AND ts >= datetime('now', '-7 days')
         GROUP BY bucket
         ORDER BY cost DESC",
    )?;
    let rows = stmt.query_map(params![account_id], |row| {
        Ok(UsageMixRow {
            task_type: row.get(0)?,
            count: row.get(1)?,
            cost_cents: row.get(2)?,
        })
    })?;
    let mix = rows.collect::<rusqlite::Result<Vec<_>>>()?;

    Ok(UsageSummary {
        total_cues,
        total_cents_spent,
        mix,
    })
}

fn usage_summary_postgres(pool: &DbPool, account_id: &str) -> Result<UsageSummary> {
    let mut conn = pool.get_pg()?;
    let row = conn.query_one(
        "SELECT COUNT(*)::bigint, COALESCE(SUM(cost_cents_to_customer), 0)::bigint
         FROM usage_events
         WHERE account_id = $1 AND ts >= now() - interval '7 days'",
        &[&account_id],
    )?;
    let total_cues: i64 = row.try_get(0)?;
    let total_cents_spent: i64 = row.try_get(1)?;
    let rows = conn.query(
        "SELECT COALESCE(task_type, lane, 'general') AS bucket,
                COUNT(*)::bigint AS cnt,
                COALESCE(SUM(cost_cents_to_customer), 0)::bigint AS cost
         FROM usage_events
         WHERE account_id = $1 AND ts >= now() - interval '7 days'
         GROUP BY bucket
         ORDER BY cost DESC",
        &[&account_id],
    )?;
    let mut mix = Vec::with_capacity(rows.len());
    for row in rows {
        mix.push(UsageMixRow {
            task_type: row.try_get(0)?,
            count: row.try_get(1)?,
            cost_cents: row.try_get(2)?,
        });
    }
    Ok(UsageSummary {
        total_cues,
        total_cents_spent,
        mix,
    })
}

fn export_bundle_sqlite(pool: &DbPool, account_id: &str) -> Result<Option<ExportBundle>> {
    let conn = pool.get()?;

    let account = conn
        .query_row(
            "SELECT id, email, balance_cents, trial_seconds_remaining,
                    created_at, last_login_at, stripe_customer_id,
                    stripe_payment_method_id, square_customer_id, square_card_id,
                    square_card_brand, square_card_last4
             FROM accounts WHERE id = ?1",
            params![account_id],
            |row| {
                Ok(ExportAccount {
                    id: row.get(0)?,
                    email: row.get(1)?,
                    balance_cents: row.get(2)?,
                    trial_seconds_remaining: row.get(3)?,
                    created_at: row.get(4)?,
                    last_login_at: row.get(5)?,
                    stripe_customer_id: row.get(6)?,
                    stripe_payment_method_id: row.get(7)?,
                    square_customer_id: row.get(8)?,
                    square_card_id: row.get(9)?,
                    square_card_brand: row.get(10)?,
                    square_card_last4: row.get(11)?,
                })
            },
        )
        .optional()?;
    let Some(account) = account else {
        return Ok(None);
    };

    let credit_batches = export_rows(
        &conn,
        "SELECT id, amount_cents, remaining_cents, purchased_at,
                expires_at, stripe_charge_id, expired_at
         FROM credit_batches WHERE account_id = ?1 ORDER BY purchased_at",
        account_id,
        &[
            "id",
            "amount_cents",
            "remaining_cents",
            "purchased_at",
            "expires_at",
            "stripe_charge_id",
            "expired_at",
        ],
    )?;

    let usage_events = export_rows(
        &conn,
        "SELECT request_id, ts, kind, task_type, lane, provider, model,
                input_tokens, output_tokens, latency_ms,
                cost_cents_to_customer
         FROM usage_events WHERE account_id = ?1 ORDER BY ts DESC LIMIT 10000",
        account_id,
        &[
            "request_id",
            "ts",
            "kind",
            "task_type",
            "lane",
            "provider",
            "model",
            "input_tokens",
            "output_tokens",
            "latency_ms",
            "cost_cents_to_customer",
        ],
    )?;

    let cloud_sessions = export_rows(
        &conn,
        "SELECT session_id, title, status, created_at_ms, updated_at_ms,
                last_active_at_ms, answer_style, metadata_json
         FROM cloud_sessions WHERE account_id = ?1 ORDER BY updated_at_ms DESC LIMIT 10000",
        account_id,
        &[
            "session_id",
            "title",
            "status",
            "created_at_ms",
            "updated_at_ms",
            "last_active_at_ms",
            "answer_style",
            "metadata_json",
        ],
    )?;
    let cloud_transcript_segments = export_rows(
        &conn,
        "SELECT segment_id, session_id, speaker, source, text, start_ms,
                end_ms, ts_ms, is_final, metadata_json
         FROM cloud_transcript_segments WHERE account_id = ?1 ORDER BY ts_ms ASC LIMIT 50000",
        account_id,
        &[
            "segment_id",
            "session_id",
            "speaker",
            "source",
            "text",
            "start_ms",
            "end_ms",
            "ts_ms",
            "is_final",
            "metadata_json",
        ],
    )?;
    let cloud_cue_responses = export_rows(
        &conn,
        "SELECT response_id, session_id, kind, text, source_text, ts_ms,
                provider, model, lane, task_type, cost_cents, balance_cents_after,
                cost_label, artifact_type, artifact_body, artifact_confidence,
                metadata_json
         FROM cloud_cue_responses WHERE account_id = ?1 ORDER BY ts_ms ASC LIMIT 50000",
        account_id,
        &[
            "response_id",
            "session_id",
            "kind",
            "text",
            "source_text",
            "ts_ms",
            "provider",
            "model",
            "lane",
            "task_type",
            "cost_cents",
            "balance_cents_after",
            "cost_label",
            "artifact_type",
            "artifact_body",
            "artifact_confidence",
            "metadata_json",
        ],
    )?;
    let cloud_context_artifacts = export_rows(
        &conn,
        "SELECT artifact_id, session_id, kind, title, note, source_uri,
                content_hash, text_preview, created_at_ms, metadata_json
         FROM cloud_context_artifacts WHERE account_id = ?1 ORDER BY created_at_ms ASC LIMIT 50000",
        account_id,
        &[
            "artifact_id",
            "session_id",
            "kind",
            "title",
            "note",
            "source_uri",
            "content_hash",
            "text_preview",
            "created_at_ms",
            "metadata_json",
        ],
    )?;
    let cloud_rag_chunks_count = conn
        .query_row(
            "SELECT COUNT(*) FROM cloud_rag_chunks WHERE account_id = ?1",
            params![account_id],
            |row| row.get(0),
        )
        .unwrap_or(0);
    let refresh_tokens_count = conn
        .query_row(
            "SELECT COUNT(*) FROM refresh_tokens WHERE account_id = ?1",
            params![account_id],
            |row| row.get(0),
        )
        .unwrap_or(0);
    let stripe_webhook_events_count = conn
        .query_row(
            "SELECT COUNT(*) FROM stripe_webhook_events
             WHERE json_extract(body, '$.data.object.client_reference_id') = ?1",
            params![account_id],
            |row| row.get(0),
        )
        .unwrap_or(0);

    Ok(Some(ExportBundle {
        account,
        credit_batches,
        usage_events,
        cloud_sessions,
        cloud_transcript_segments,
        cloud_cue_responses,
        cloud_context_artifacts,
        cloud_rag_chunks_count,
        refresh_tokens_count,
        stripe_webhook_events_count,
        exported_at: chrono::Utc::now().to_rfc3339(),
    }))
}

fn export_bundle_postgres(pool: &DbPool, account_id: &str) -> Result<Option<ExportBundle>> {
    let mut conn = pool.get_pg()?;
    let account = conn
        .query_opt(
            "SELECT id, email, balance_cents, trial_seconds_remaining,
                    created_at::text, last_login_at::text, stripe_customer_id,
                    stripe_payment_method_id, square_customer_id, square_card_id,
                    square_card_brand, square_card_last4
             FROM accounts WHERE id = $1",
            &[&account_id],
        )?
        .map(export_account_from_pg)
        .transpose()?;
    let Some(account) = account else {
        return Ok(None);
    };

    let credit_batches = export_rows_pg(
        &mut conn,
        "SELECT id, amount_cents, remaining_cents, purchased_at::text AS purchased_at,
                expires_at::text AS expires_at, stripe_charge_id, expired_at::text AS expired_at
         FROM credit_batches WHERE account_id = $1 ORDER BY purchased_at",
        account_id,
    )?;
    let usage_events = export_rows_pg(
        &mut conn,
        "SELECT request_id, ts::text AS ts, kind, task_type, lane, provider, model,
                input_tokens, output_tokens, latency_ms,
                cost_cents_to_customer
         FROM usage_events WHERE account_id = $1 ORDER BY ts DESC LIMIT 10000",
        account_id,
    )?;
    let cloud_sessions = export_rows_pg(
        &mut conn,
        "SELECT session_id, title, status, created_at_ms, updated_at_ms,
                last_active_at_ms, answer_style, metadata_json
         FROM cloud_sessions WHERE account_id = $1 ORDER BY updated_at_ms DESC LIMIT 10000",
        account_id,
    )?;
    let cloud_transcript_segments = export_rows_pg(
        &mut conn,
        "SELECT segment_id, session_id, speaker, source, text, start_ms,
                end_ms, ts_ms, is_final, metadata_json
         FROM cloud_transcript_segments WHERE account_id = $1 ORDER BY ts_ms ASC LIMIT 50000",
        account_id,
    )?;
    let cloud_cue_responses = export_rows_pg(
        &mut conn,
        "SELECT response_id, session_id, kind, text, source_text, ts_ms,
                provider, model, lane, task_type, cost_cents, balance_cents_after,
                cost_label, artifact_type, artifact_body, artifact_confidence,
                metadata_json
         FROM cloud_cue_responses WHERE account_id = $1 ORDER BY ts_ms ASC LIMIT 50000",
        account_id,
    )?;
    let cloud_context_artifacts = export_rows_pg(
        &mut conn,
        "SELECT artifact_id, session_id, kind, title, note, source_uri,
                content_hash, text_preview, created_at_ms, metadata_json
         FROM cloud_context_artifacts WHERE account_id = $1 ORDER BY created_at_ms ASC LIMIT 50000",
        account_id,
    )?;
    let cloud_rag_chunks_count: i64 = conn
        .query_one(
            "SELECT COUNT(*)::bigint FROM cloud_rag_chunks WHERE account_id = $1",
            &[&account_id],
        )?
        .try_get(0)?;
    let refresh_tokens_count: i64 = conn
        .query_one(
            "SELECT COUNT(*)::bigint FROM refresh_tokens WHERE account_id = $1",
            &[&account_id],
        )?
        .try_get(0)?;
    let stripe_webhook_events_count: i64 = conn
        .query_one(
            "SELECT COUNT(*)::bigint FROM stripe_webhook_events
             WHERE body::jsonb #>> '{data,object,client_reference_id}' = $1
                OR body::jsonb #>> '{data,object,metadata,bluey_account_id}' = $1",
            &[&account_id],
        )
        .map(|row| row.try_get(0).unwrap_or(0))
        .unwrap_or(0);

    Ok(Some(ExportBundle {
        account,
        credit_batches,
        usage_events,
        cloud_sessions,
        cloud_transcript_segments,
        cloud_cue_responses,
        cloud_context_artifacts,
        cloud_rag_chunks_count,
        refresh_tokens_count,
        stripe_webhook_events_count,
        exported_at: chrono::Utc::now().to_rfc3339(),
    }))
}

fn hard_delete_account_sqlite(pool: &DbPool, account_id: &str) -> Result<bool> {
    let mut conn = pool.get()?;
    let tx = conn.transaction()?;
    tx.execute(
        "DELETE FROM stripe_webhook_events
            WHERE json_extract(body, '$.data.object.client_reference_id') = ?1
               OR json_extract(body, '$.data.object.metadata.bluey_account_id') = ?1",
        params![account_id],
    )?;
    let deleted = tx.execute("DELETE FROM accounts WHERE id = ?1", params![account_id])?;
    tx.commit()?;
    Ok(deleted > 0)
}

fn hard_delete_account_postgres(pool: &DbPool, account_id: &str) -> Result<bool> {
    let mut conn = pool.get_pg()?;
    let mut tx = conn.transaction()?;
    tx.execute(
        "DELETE FROM stripe_webhook_events
            WHERE body::jsonb #>> '{data,object,client_reference_id}' = $1
               OR body::jsonb #>> '{data,object,metadata,bluey_account_id}' = $1",
        &[&account_id],
    )
    .ok();
    let deleted = tx.execute("DELETE FROM accounts WHERE id = $1", &[&account_id])?;
    tx.commit()?;
    Ok(deleted > 0)
}

fn export_rows(
    conn: &rusqlite::Connection,
    sql: &str,
    account_id: &str,
    columns: &[&str],
) -> Result<Vec<serde_json::Value>> {
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map(params![account_id], |row| {
        let mut obj = serde_json::Map::new();
        for (idx, column) in columns.iter().enumerate() {
            let value: rusqlite::types::Value = row.get(idx)?;
            obj.insert((*column).to_string(), sqlite_value_to_json(value));
        }
        Ok(serde_json::Value::Object(obj))
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

fn export_account_from_pg(row: PgRow) -> Result<ExportAccount> {
    Ok(ExportAccount {
        id: row.try_get(0)?,
        email: row.try_get(1)?,
        balance_cents: row.try_get(2)?,
        trial_seconds_remaining: row.try_get(3)?,
        created_at: row.try_get(4)?,
        last_login_at: row.try_get(5)?,
        stripe_customer_id: row.try_get(6)?,
        stripe_payment_method_id: row.try_get(7)?,
        square_customer_id: row.try_get(8)?,
        square_card_id: row.try_get(9)?,
        square_card_brand: row.try_get(10)?,
        square_card_last4: row.try_get(11)?,
    })
}

fn export_rows_pg(
    conn: &mut postgres::Client,
    sql: &str,
    account_id: &str,
) -> Result<Vec<serde_json::Value>> {
    let wrapped = format!(
        "SELECT COALESCE(jsonb_agg(to_jsonb(rows)), '[]'::jsonb)::text FROM ({sql}) rows"
    );
    let raw: String = conn
        .query_one(&wrapped, &[&account_id])
        .with_context(|| format!("export postgres rows for query: {sql}"))?
        .try_get(0)?;
    Ok(serde_json::from_str(&raw)?)
}

fn sqlite_value_to_json(value: rusqlite::types::Value) -> serde_json::Value {
    match value {
        rusqlite::types::Value::Null => serde_json::Value::Null,
        rusqlite::types::Value::Integer(v) => serde_json::json!(v),
        rusqlite::types::Value::Real(v) => serde_json::json!(v),
        rusqlite::types::Value::Text(v) => serde_json::Value::String(v),
        rusqlite::types::Value::Blob(_) => serde_json::Value::String("<blob>".into()),
    }
}
