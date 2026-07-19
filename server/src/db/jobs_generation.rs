//! Durable, tenant-scoped cache for Jobs resume model generations.

use anyhow::{Context, Result};
use chrono::Utc;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::DbPool;

const RESERVATION_STALE_MS: i64 = 2 * 60 * 1_000;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResumeGenerationRecord {
    pub id: String,
    pub job_id: String,
    pub generation_key: String,
    pub reservation_token: String,
    pub status: String,
    pub output: Option<Value>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cost_cents_to_bluey: i64,
    pub failure_code: Option<String>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ResumeGenerationReservation {
    Start(ResumeGenerationRecord),
    Ready(ResumeGenerationRecord),
    Pending,
}

fn now_ms() -> i64 {
    Utc::now().timestamp_millis()
}

fn decode_output(raw: Option<String>) -> Result<Option<Value>> {
    raw.map(|value| serde_json::from_str(&value).context("decode resume generation output"))
        .transpose()
}

fn sqlite_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<ResumeGenerationRecord> {
    let output_raw: Option<String> = row.get(5)?;
    let output = output_raw
        .map(|value| {
            serde_json::from_str(&value).map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(
                    5,
                    rusqlite::types::Type::Text,
                    Box::new(error),
                )
            })
        })
        .transpose()?;
    Ok(ResumeGenerationRecord {
        id: row.get(0)?,
        job_id: row.get(1)?,
        generation_key: row.get(2)?,
        reservation_token: row.get(3)?,
        status: row.get(4)?,
        output,
        provider: row.get(6)?,
        model: row.get(7)?,
        input_tokens: row.get(8)?,
        output_tokens: row.get(9)?,
        cost_cents_to_bluey: row.get(10)?,
        failure_code: row.get(11)?,
        created_at_ms: row.get(12)?,
        updated_at_ms: row.get(13)?,
    })
}

fn pg_record(row: postgres::Row) -> Result<ResumeGenerationRecord> {
    let output = decode_output(row.get(5))?;
    Ok(ResumeGenerationRecord {
        id: row.get(0),
        job_id: row.get(1),
        generation_key: row.get(2),
        reservation_token: row.get(3),
        status: row.get(4),
        output,
        provider: row.get(6),
        model: row.get(7),
        input_tokens: row.get(8),
        output_tokens: row.get(9),
        cost_cents_to_bluey: row.get(10),
        failure_code: row.get(11),
        created_at_ms: row.get(12),
        updated_at_ms: row.get(13),
    })
}

const SELECT_COLUMNS: &str =
    "id, job_id, generation_key, reservation_token, status, output_json, provider, model, input_tokens, \
     output_tokens, cost_cents_to_bluey, failure_code, created_at_ms, updated_at_ms";

pub fn reserve(
    pool: &DbPool,
    account_id: &str,
    job_id: &str,
    generation_key: &str,
) -> Result<ResumeGenerationReservation> {
    let now = now_ms();
    let stale_before = now - RESERVATION_STALE_MS;
    let reservation_token = uuid::Uuid::new_v4().to_string();
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
            let existing = tx
                .query_row(
                    &format!(
                        "SELECT {SELECT_COLUMNS} FROM jobs_resume_generations \
                         WHERE account_id = ?1 AND generation_key = ?2"
                    ),
                    params![account_id, generation_key],
                    sqlite_record,
                )
                .optional()?;
            if let Some(mut record) = existing {
                if record.status == "completed" && record.output.is_some() {
                    tx.commit()?;
                    return Ok(ResumeGenerationReservation::Ready(record));
                }
                if record.status == "reserved" && record.updated_at_ms >= stale_before {
                    tx.commit()?;
                    return Ok(ResumeGenerationReservation::Pending);
                }
                tx.execute(
                    "UPDATE jobs_resume_generations SET reservation_token = ?3, status = 'reserved', output_json = NULL, \
                     provider = NULL, model = NULL, input_tokens = 0, output_tokens = 0, \
                     cost_cents_to_bluey = 0, failure_code = NULL, updated_at_ms = ?4 \
                     WHERE account_id = ?1 AND generation_key = ?2",
                    params![account_id, generation_key, reservation_token, now],
                )?;
                record.reservation_token = reservation_token;
                record.status = "reserved".into();
                record.output = None;
                record.provider = None;
                record.model = None;
                record.input_tokens = 0;
                record.output_tokens = 0;
                record.cost_cents_to_bluey = 0;
                record.failure_code = None;
                record.updated_at_ms = now;
                tx.commit()?;
                return Ok(ResumeGenerationReservation::Start(record));
            }
            let id = uuid::Uuid::new_v4().to_string();
            tx.execute(
                "INSERT INTO jobs_resume_generations (id, account_id, job_id, generation_key, \
                 reservation_token, status, created_at_ms, updated_at_ms) \
                 VALUES (?1, ?2, ?3, ?4, ?5, 'reserved', ?6, ?6)",
                params![
                    id,
                    account_id,
                    job_id,
                    generation_key,
                    reservation_token,
                    now
                ],
            )?;
            tx.commit()?;
            Ok(ResumeGenerationReservation::Start(ResumeGenerationRecord {
                id,
                job_id: job_id.to_string(),
                generation_key: generation_key.to_string(),
                reservation_token,
                status: "reserved".into(),
                output: None,
                provider: None,
                model: None,
                input_tokens: 0,
                output_tokens: 0,
                cost_cents_to_bluey: 0,
                failure_code: None,
                created_at_ms: now,
                updated_at_ms: now,
            }))
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            let id = uuid::Uuid::new_v4().to_string();
            let inserted = tx.execute(
                "INSERT INTO jobs_resume_generations (id, account_id, job_id, generation_key, \
                 reservation_token, status, created_at_ms, updated_at_ms) \
                 VALUES ($1, $2, $3, $4, $5, 'reserved', $6, $6) \
                 ON CONFLICT (account_id, generation_key) DO NOTHING",
                &[
                    &id,
                    &account_id,
                    &job_id,
                    &generation_key,
                    &reservation_token,
                    &now,
                ],
            )?;
            let existing = tx.query_opt(
                &format!(
                    "SELECT {SELECT_COLUMNS} FROM jobs_resume_generations \
                     WHERE account_id = $1 AND generation_key = $2 FOR UPDATE"
                ),
                &[&account_id, &generation_key],
            )?;
            if let Some(row) = existing {
                let mut record = pg_record(row)?;
                if inserted == 1 {
                    tx.commit()?;
                    return Ok(ResumeGenerationReservation::Start(record));
                }
                if record.status == "completed" && record.output.is_some() {
                    tx.commit()?;
                    return Ok(ResumeGenerationReservation::Ready(record));
                }
                if record.status == "reserved" && record.updated_at_ms >= stale_before {
                    tx.commit()?;
                    return Ok(ResumeGenerationReservation::Pending);
                }
                tx.execute(
                    "UPDATE jobs_resume_generations SET reservation_token = $3, status = 'reserved', output_json = NULL, \
                     provider = NULL, model = NULL, input_tokens = 0, output_tokens = 0, \
                     cost_cents_to_bluey = 0, failure_code = NULL, updated_at_ms = $4 \
                     WHERE account_id = $1 AND generation_key = $2",
                    &[&account_id, &generation_key, &reservation_token, &now],
                )?;
                record.reservation_token = reservation_token;
                record.status = "reserved".into();
                record.output = None;
                record.provider = None;
                record.model = None;
                record.input_tokens = 0;
                record.output_tokens = 0;
                record.cost_cents_to_bluey = 0;
                record.failure_code = None;
                record.updated_at_ms = now;
                tx.commit()?;
                return Ok(ResumeGenerationReservation::Start(record));
            }
            Err(anyhow::anyhow!("resume generation reservation disappeared"))
        }
    })
}

#[allow(clippy::too_many_arguments)]
pub fn complete(
    pool: &DbPool,
    account_id: &str,
    generation_key: &str,
    reservation_token: &str,
    output: &Value,
    provider: &str,
    model: &str,
    input_tokens: i64,
    output_tokens: i64,
    cost_cents_to_bluey: i64,
) -> Result<ResumeGenerationRecord> {
    let now = now_ms();
    let output_json = serde_json::to_string(output).context("encode resume generation output")?;
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get()?;
            let updated = conn.execute(
                "UPDATE jobs_resume_generations SET status = 'completed', output_json = ?4, \
                 provider = ?5, model = ?6, input_tokens = ?7, output_tokens = ?8, \
                 cost_cents_to_bluey = ?9, failure_code = NULL, updated_at_ms = ?10 \
                 WHERE account_id = ?1 AND generation_key = ?2 AND reservation_token = ?3 \
                 AND status = 'reserved'",
                params![
                    account_id,
                    generation_key,
                    reservation_token,
                    output_json,
                    provider,
                    model,
                    input_tokens,
                    output_tokens,
                    cost_cents_to_bluey,
                    now
                ],
            )?;
            if updated != 1 {
                anyhow::bail!("resume generation reservation is no longer owned by this worker");
            }
            conn.query_row(
                &format!("SELECT {SELECT_COLUMNS} FROM jobs_resume_generations WHERE account_id = ?1 AND generation_key = ?2"),
                params![account_id, generation_key],
                sqlite_record,
            ).context("load completed resume generation")
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let row = conn
                .query_opt(
                    &format!(
                    "UPDATE jobs_resume_generations SET status = 'completed', output_json = $4, \
                 provider = $5, model = $6, input_tokens = $7, output_tokens = $8, \
                 cost_cents_to_bluey = $9, failure_code = NULL, updated_at_ms = $10 \
                 WHERE account_id = $1 AND generation_key = $2 AND reservation_token = $3 \
                 AND status = 'reserved' \
                 RETURNING {SELECT_COLUMNS}"
                ),
                    &[
                        &account_id,
                        &generation_key,
                        &reservation_token,
                        &output_json,
                        &provider,
                        &model,
                        &input_tokens,
                        &output_tokens,
                        &cost_cents_to_bluey,
                        &now,
                    ],
                )?
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "resume generation reservation is no longer owned by this worker"
                    )
                })?;
            pg_record(row)
        }
    })
}

pub fn fail(
    pool: &DbPool,
    account_id: &str,
    generation_key: &str,
    reservation_token: &str,
    code: &str,
) -> Result<()> {
    let now = now_ms();
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get()?;
            let updated = conn.execute(
                "UPDATE jobs_resume_generations SET status = 'failed', failure_code = ?4, \
                 updated_at_ms = ?5 WHERE account_id = ?1 AND generation_key = ?2 \
                 AND reservation_token = ?3 AND status = 'reserved'",
                params![account_id, generation_key, reservation_token, code, now],
            )?;
            if updated != 1 {
                anyhow::bail!("resume generation reservation is no longer owned by this worker");
            }
            Ok(())
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let updated = conn.execute(
                "UPDATE jobs_resume_generations SET status = 'failed', failure_code = $4, \
                 updated_at_ms = $5 WHERE account_id = $1 AND generation_key = $2 \
                 AND reservation_token = $3 AND status = 'reserved'",
                &[
                    &account_id,
                    &generation_key,
                    &reservation_token,
                    &code,
                    &now,
                ],
            )?;
            if updated != 1 {
                anyhow::bail!("resume generation reservation is no longer owned by this worker");
            }
            Ok(())
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{self, DbPool};

    fn pool_with_job() -> (DbPool, String, String) {
        let path = std::env::temp_dir().join(format!(
            "bluey-jobs-generation-test-{}-{}.sqlite3",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let pool = db::open_pool(&path).unwrap();
        db::run_migrations(&pool).unwrap();
        let account_id = uuid::Uuid::new_v4().to_string();
        let job_id = uuid::Uuid::new_v4().to_string();
        let now = now_ms();
        pool.get()
            .unwrap()
            .execute(
                "INSERT INTO accounts (id, email, password_hash, trial_seconds_remaining) \
                 VALUES (?1, 'generation@bluey.test', 'hash', 0)",
                params![account_id],
            )
            .unwrap();
        pool.get()
            .unwrap()
            .execute(
                "INSERT INTO jobs_postings (id, account_id, canonical_key, posting_json, source, \
                 canonical_url, company, title, location, match_score, status, created_at_ms, updated_at_ms) \
                 VALUES (?1, ?2, ?3, '{}', 'test', NULL, 'Example', 'Software Engineer', NULL, 0, \
                 'matched', ?4, ?4)",
                params![job_id, account_id, format!("test:{job_id}"), now],
            )
            .unwrap();
        (pool, account_id, job_id)
    }

    #[test]
    fn reservation_is_idempotent_and_completed_output_is_reused() {
        let (pool, account_id, job_id) = pool_with_job();
        let ResumeGenerationReservation::Start(reservation) =
            reserve(&pool, &account_id, &job_id, "key-1").unwrap()
        else {
            panic!("first worker should own the generation")
        };
        assert_eq!(
            reserve(&pool, &account_id, &job_id, "key-1").unwrap(),
            ResumeGenerationReservation::Pending
        );
        complete(
            &pool,
            &account_id,
            "key-1",
            &reservation.reservation_token,
            &serde_json::json!({"headline":"Engineer"}),
            "openai",
            "model",
            20,
            10,
            1,
        )
        .unwrap();
        let ResumeGenerationReservation::Ready(record) =
            reserve(&pool, &account_id, &job_id, "key-1").unwrap()
        else {
            panic!("completed generation should be reused")
        };
        assert_eq!(record.output.unwrap()["headline"], "Engineer");
        assert_eq!(record.cost_cents_to_bluey, 1);
    }

    #[test]
    fn reclaimed_reservation_rejects_the_stale_worker() {
        let (pool, account_id, job_id) = pool_with_job();
        let ResumeGenerationReservation::Start(first) =
            reserve(&pool, &account_id, &job_id, "key-reclaimed").unwrap()
        else {
            panic!("first worker should own the generation")
        };
        pool.get()
            .unwrap()
            .execute(
                "UPDATE jobs_resume_generations SET updated_at_ms = 0 \
                 WHERE account_id = ?1 AND generation_key = ?2",
                params![account_id, "key-reclaimed"],
            )
            .unwrap();

        let ResumeGenerationReservation::Start(second) =
            reserve(&pool, &account_id, &job_id, "key-reclaimed").unwrap()
        else {
            panic!("second worker should reclaim the stale generation")
        };
        assert_ne!(first.reservation_token, second.reservation_token);
        assert!(complete(
            &pool,
            &account_id,
            "key-reclaimed",
            &first.reservation_token,
            &serde_json::json!({"headline":"stale"}),
            "openai",
            "model",
            20,
            10,
            1,
        )
        .is_err());
        complete(
            &pool,
            &account_id,
            "key-reclaimed",
            &second.reservation_token,
            &serde_json::json!({"headline":"current"}),
            "openai",
            "model",
            20,
            10,
            1,
        )
        .unwrap();
    }
}
