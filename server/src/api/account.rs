//! Account endpoints — real implementations.

use axum::{extract::State, http::StatusCode, Extension, Json};
use serde::Serialize;

use super::AppState;
use crate::auth::AuthedAccount;

#[derive(Serialize)]
pub struct AccountMe {
    pub id: String,
    pub email: String,
    pub balance_cents: i64,
    pub trial_seconds_remaining: i64,
    pub auto_topup_enabled: bool,
    pub auto_topup_threshold_cents: i64,
    pub auto_topup_amount_cents: i64,
}

pub async fn me(Extension(AuthedAccount(account)): Extension<AuthedAccount>) -> Json<AccountMe> {
    Json(AccountMe {
        id: account.id,
        email: account.email,
        balance_cents: account.balance_cents,
        trial_seconds_remaining: account.trial_seconds_remaining,
        auto_topup_enabled: account.auto_topup_enabled,
        auto_topup_threshold_cents: account.auto_topup_threshold_cents,
        auto_topup_amount_cents: account.auto_topup_amount_cents,
    })
}

#[derive(Serialize)]
pub struct UsageWindow {
    pub period_days: i64,
    pub total_cues: i64,
    pub total_cents_spent: i64,
    pub mix: Vec<MixEntry>,
    pub tier_label: String,
    pub projected_days_remaining: f64,
}

#[derive(Serialize)]
pub struct MixEntry {
    pub task_type: String,
    pub count: i64,
    pub cost_cents: i64,
    pub percent: f64,
}

const PERIOD_DAYS: i64 = 7;

pub async fn usage(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
) -> Result<Json<UsageWindow>, StatusCode> {
    let conn = state
        .pool
        .get()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    // Codex Stage 7 S7.2: use SQLite datetime() consistently. Stored
    // ts is "YYYY-MM-DD HH:MM:SS" via datetime('now') default; we
    // compute the cutoff the same way to avoid mixed-format text
    // comparison breaking around boundaries.
    let _ = PERIOD_DAYS; // (referenced in datetime literal below)

    let (total_cues, total_cents_spent): (i64, i64) = conn
        .query_row(
            "SELECT COUNT(*), COALESCE(SUM(cost_cents_to_customer), 0)
             FROM usage_events
             WHERE account_id = ?1 AND ts >= datetime('now', '-7 days')",
            rusqlite::params![&account.id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Per-task-type breakdown. Uses lane as the bucket when task_type
    // is NULL because v0.1 client-side classifier metadata isn't always
    // forwarded into usage_events. Coalesce to "general" for legibility.
    let mut stmt = conn
        .prepare(
            "SELECT COALESCE(task_type, lane, 'general') AS bucket,
                    COUNT(*) AS cnt,
                    COALESCE(SUM(cost_cents_to_customer), 0) AS cost
             FROM usage_events
             WHERE account_id = ?1 AND ts >= datetime('now', '-7 days')
             GROUP BY bucket
             ORDER BY cost DESC",
        )
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let mix_raw: Vec<(String, i64, i64)> = stmt
        .query_map(rusqlite::params![&account.id], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .filter_map(|r| r.ok())
        .collect();

    let mix: Vec<MixEntry> = mix_raw
        .into_iter()
        .map(|(t, count, cost)| {
            let percent = if total_cues > 0 {
                100.0 * (count as f64) / (total_cues as f64)
            } else {
                0.0
            };
            MixEntry {
                task_type: t,
                count,
                cost_cents: cost,
                percent,
            }
        })
        .collect();

    // Tier classification + projection.
    let avg_cost_per_cue_cents = if total_cues > 0 {
        (total_cents_spent as f64) / (total_cues as f64)
    } else {
        2.2 // typical-tier default ~2.2 cents/cue
    };
    let cues_per_30 = if avg_cost_per_cue_cents > 0.0 {
        3000.0 / avg_cost_per_cue_cents
    } else {
        f64::INFINITY
    };
    let tier_label = if cues_per_30 >= 2200.0 {
        "Light"
    } else if cues_per_30 >= 1100.0 {
        "Typical tech"
    } else {
        "Heavy"
    };

    // Projected days remaining at current burn rate.
    let cues_per_day = (total_cues as f64) / (PERIOD_DAYS as f64);
    let avg_cents_per_day = cues_per_day * avg_cost_per_cue_cents;
    let projected_days_remaining = if avg_cents_per_day > 0.01 {
        (account.balance_cents as f64) / avg_cents_per_day
    } else {
        365.0 // no usage yet → cap at 1 year (credit-validity boundary)
    };

    Ok(Json(UsageWindow {
        period_days: PERIOD_DAYS,
        total_cues,
        total_cents_spent,
        mix,
        tier_label: tier_label.to_string(),
        projected_days_remaining: (projected_days_remaining * 10.0).round() / 10.0,
    }))
}

// ─── Codex Stage 14: GDPR delete + export ───────────────────────────────

#[derive(serde::Serialize)]
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

#[derive(serde::Serialize)]
pub struct ExportAccount {
    pub id: String,
    pub email: String,
    pub balance_cents: i64,
    pub trial_seconds_remaining: i64,
    pub created_at: Option<String>,
    pub last_login_at: Option<String>,
    pub stripe_customer_id: Option<String>,
}

pub async fn export_data(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
) -> Result<Json<ExportBundle>, axum::http::StatusCode> {
    let conn = state
        .pool
        .get()
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;

    let exp_account: ExportAccount = conn
        .query_row(
            "SELECT id, email, balance_cents, trial_seconds_remaining,
                    created_at, last_login_at, stripe_customer_id
             FROM accounts WHERE id = ?1",
            rusqlite::params![&account.id],
            |r| {
                Ok(ExportAccount {
                    id: r.get(0)?,
                    email: r.get(1)?,
                    balance_cents: r.get(2)?,
                    trial_seconds_remaining: r.get(3)?,
                    created_at: r.get(4)?,
                    last_login_at: r.get(5)?,
                    stripe_customer_id: r.get(6)?,
                })
            },
        )
        .map_err(|_| axum::http::StatusCode::NOT_FOUND)?;

    // Credit batches.
    let mut stmt = conn
        .prepare(
            "SELECT id, amount_cents, remaining_cents, purchased_at,
                    expires_at, stripe_charge_id, expired_at
             FROM credit_batches WHERE account_id = ?1 ORDER BY purchased_at",
        )
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;
    let batches: Vec<serde_json::Value> = stmt
        .query_map(rusqlite::params![&account.id], |r| {
            Ok(serde_json::json!({
                "id": r.get::<_, String>(0)?,
                "amount_cents": r.get::<_, i64>(1)?,
                "remaining_cents": r.get::<_, i64>(2)?,
                "purchased_at": r.get::<_, Option<String>>(3)?,
                "expires_at": r.get::<_, Option<String>>(4)?,
                "stripe_charge_id": r.get::<_, Option<String>>(5)?,
                "expired_at": r.get::<_, Option<String>>(6)?,
            }))
        })
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?
        .filter_map(|r| r.ok())
        .collect();

    // Usage events.
    let mut stmt = conn
        .prepare(
            "SELECT request_id, ts, kind, task_type, lane, provider, model,
                    input_tokens, output_tokens, latency_ms,
                    cost_cents_to_customer
             FROM usage_events WHERE account_id = ?1 ORDER BY ts DESC LIMIT 10000",
        )
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;
    let events: Vec<serde_json::Value> = stmt
        .query_map(rusqlite::params![&account.id], |r| {
            Ok(serde_json::json!({
                "request_id": r.get::<_, String>(0)?,
                "ts": r.get::<_, String>(1)?,
                "kind": r.get::<_, String>(2)?,
                "task_type": r.get::<_, Option<String>>(3)?,
                "lane": r.get::<_, Option<String>>(4)?,
                "provider": r.get::<_, Option<String>>(5)?,
                "model": r.get::<_, Option<String>>(6)?,
                "input_tokens": r.get::<_, i64>(7)?,
                "output_tokens": r.get::<_, i64>(8)?,
                "latency_ms": r.get::<_, i64>(9)?,
                "cost_cents_to_customer": r.get::<_, i64>(10)?,
            }))
        })
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?
        .filter_map(|r| r.ok())
        .collect();

    let cloud_sessions = export_rows(
        &conn,
        "SELECT session_id, title, status, created_at_ms, updated_at_ms,
                last_active_at_ms, answer_style, metadata_json
         FROM cloud_sessions WHERE account_id = ?1 ORDER BY updated_at_ms DESC LIMIT 10000",
        &account.id,
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
        &account.id,
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
        &account.id,
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
        &account.id,
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
    let cloud_rag_chunks_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM cloud_rag_chunks WHERE account_id = ?1",
            rusqlite::params![&account.id],
            |r| r.get(0),
        )
        .unwrap_or(0);

    let refresh_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM refresh_tokens WHERE account_id = ?1",
            rusqlite::params![&account.id],
            |r| r.get(0),
        )
        .unwrap_or(0);
    // Stripe webhook events are not joined to accounts directly in the
    // schema (account_id lives in metadata); we count via JSON extract.
    let stripe_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM stripe_webhook_events
             WHERE json_extract(body, '$.data.object.client_reference_id') = ?1",
            rusqlite::params![&account.id],
            |r| r.get(0),
        )
        .unwrap_or(0);

    Ok(Json(ExportBundle {
        account: exp_account,
        credit_batches: batches,
        usage_events: events,
        cloud_sessions,
        cloud_transcript_segments,
        cloud_cue_responses,
        cloud_context_artifacts,
        cloud_rag_chunks_count,
        refresh_tokens_count: refresh_count,
        stripe_webhook_events_count: stripe_count,
        exported_at: chrono::Utc::now().to_rfc3339(),
    }))
}

fn export_rows(
    conn: &rusqlite::Connection,
    sql: &str,
    account_id: &str,
    columns: &[&str],
) -> Result<Vec<serde_json::Value>, axum::http::StatusCode> {
    let mut stmt = conn
        .prepare(sql)
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;
    let rows = stmt
        .query_map(rusqlite::params![account_id], |row| {
            let mut obj = serde_json::Map::new();
            for (idx, column) in columns.iter().enumerate() {
                let value: rusqlite::types::Value = row.get(idx)?;
                obj.insert((*column).to_string(), sqlite_value_to_json(value));
            }
            Ok(serde_json::Value::Object(obj))
        })
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?
        .filter_map(|r| r.ok())
        .collect();
    Ok(rows)
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

#[derive(serde::Serialize)]
pub struct DeleteAck {
    pub deleted: bool,
    pub deleted_at: String,
    pub note: &'static str,
}

pub async fn delete_account(
    State(state): State<AppState>,
    Extension(AuthedAccount(account)): Extension<AuthedAccount>,
) -> Result<Json<DeleteAck>, axum::http::StatusCode> {
    // Hard delete. ON DELETE CASCADE on the foreign keys (accounts ->
    // credit_batches, refresh_tokens, usage_events,
    // email_verification_tokens, password_reset_tokens, request_idempotency)
    // takes care of dependent rows.
    let conn = state
        .pool
        .get()
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;
    // Codex S12-17 blocker 4: stripe_webhook_events has no FK to
    // accounts; account_id is buried in the JSON body via
    // client_reference_id / metadata.bluey_account_id. Hard-delete must
    // remove (or scrub) those rows too. We do BOTH:
    //   1. DELETE rows where the JSON-extracted client_reference_id
    //      matches this account.
    //   2. DELETE rows where metadata.bluey_account_id matches.
    // Done in the same transaction so a partial crash leaves no
    // PII-bearing webhook rows tied to a deleted account.
    let mut conn = conn;
    let tx = conn
        .transaction()
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;
    tx.execute(
        "DELETE FROM stripe_webhook_events
            WHERE json_extract(body, '$.data.object.client_reference_id') = ?1
               OR json_extract(body, '$.data.object.metadata.bluey_account_id') = ?1",
        rusqlite::params![&account.id],
    )
    .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;
    let n = tx
        .execute(
            "DELETE FROM accounts WHERE id = ?1",
            rusqlite::params![&account.id],
        )
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;
    tx.commit()
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;
    if n == 0 {
        return Err(axum::http::StatusCode::NOT_FOUND);
    }
    tracing::info!(
        account_id_hash = %cue_core::account_id_hash_prefix(&account.id),
        "account deleted (GDPR hard-delete)"
    );
    Ok(Json(DeleteAck {
        deleted: true,
        deleted_at: chrono::Utc::now().to_rfc3339(),
        note: "All account data has been removed. Re-signup is allowed with the same email.",
    }))
}
