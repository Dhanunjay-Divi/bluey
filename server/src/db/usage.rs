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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProviderRoutingSummary {
    pub generated_at_ms: i64,
    pub window_hours: i64,
    pub total_events: i64,
    pub fallback_events: i64,
    pub fallback_rate: f64,
    pub avg_latency_ms: f64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cost_cents_to_bluey: i64,
    pub cost_cents_to_customer: i64,
    pub by_provider_model: Vec<ProviderModelUsageRow>,
    pub by_lane: Vec<UsageGroupRow>,
    pub by_task_type: Vec<UsageGroupRow>,
    pub privacy_note: String,
    pub limitation_note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProviderModelUsageRow {
    pub provider: String,
    pub model: String,
    pub lane: String,
    pub events: i64,
    pub fallback_events: i64,
    pub avg_latency_ms: f64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cost_cents_to_bluey: i64,
    pub cost_cents_to_customer: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UsageGroupRow {
    pub key: String,
    pub events: i64,
    pub fallback_events: i64,
    pub avg_latency_ms: f64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cost_cents_to_bluey: i64,
    pub cost_cents_to_customer: i64,
}

pub fn record(pool: &DbPool, account_id: &str, event: &UsageEvent) -> Result<bool> {
    crate::db::run_blocking_db(|| {
        // Codex Stage 7 S7.1: idempotent ingestion. INSERT OR IGNORE
        // returns 0 affected rows when (account_id, request_id, kind)
        // already exists; we surface that as Ok(false) so callers can log
        // the dedup without treating it as an error.
        let id = uuid::Uuid::new_v4().to_string();
        match pool {
            DbPool::Sqlite(_) => {
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
            DbPool::Postgres(_) => {
                let mut conn = pool.get_pg()?;
                let was_speculative = event.was_speculative as i32;
                let was_fallback = event.was_fallback as i32;
                let inserted = conn.execute(
                    "INSERT INTO usage_events
                    (id, account_id, request_id, kind, task_type, lane, provider, model,
                     input_tokens, output_tokens, latency_ms,
                     cost_cents_to_bluey, cost_cents_to_customer,
                     was_speculative, was_fallback)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)
                 ON CONFLICT (account_id, request_id, kind) DO NOTHING",
                    &[
                        &id,
                        &account_id,
                        &event.request_id,
                        &event.kind,
                        &event.task_type,
                        &event.lane,
                        &event.provider,
                        &event.model,
                        &event.input_tokens,
                        &event.output_tokens,
                        &event.latency_ms,
                        &event.cost_cents_to_bluey,
                        &event.cost_cents_to_customer,
                        &was_speculative,
                        &was_fallback,
                    ],
                )?;
                Ok(inserted == 1)
            }
        }
    })
}

pub fn provider_routing_summary(
    pool: &DbPool,
    window_hours: i64,
) -> Result<ProviderRoutingSummary> {
    let window_hours = window_hours.max(0);
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => provider_routing_summary_sqlite(pool, window_hours),
        DbPool::Postgres(_) => provider_routing_summary_postgres(pool, window_hours),
    })
}

fn provider_routing_summary_sqlite(
    pool: &DbPool,
    window_hours: i64,
) -> Result<ProviderRoutingSummary> {
    let conn = pool.get()?;
    let window = format!("-{window_hours} hours");
    let total: (i64, i64, f64, i64, i64, i64, i64) = conn.query_row(
        "SELECT COUNT(*),
                COALESCE(SUM(CASE WHEN was_fallback != 0 THEN 1 ELSE 0 END), 0),
                COALESCE(AVG(latency_ms), 0.0),
                COALESCE(SUM(input_tokens), 0),
                COALESCE(SUM(output_tokens), 0),
                COALESCE(SUM(cost_cents_to_bluey), 0),
                COALESCE(SUM(cost_cents_to_customer), 0)
           FROM usage_events
          WHERE kind = 'llm'
            AND (?1 <= 0 OR ts >= datetime('now', ?2))",
        params![window_hours, window],
        |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
                row.get(6)?,
            ))
        },
    )?;

    let mut provider_stmt = conn.prepare(
        "SELECT COALESCE(provider, 'unknown'),
                COALESCE(model, 'unknown'),
                COALESCE(lane, 'unknown'),
                COUNT(*),
                COALESCE(SUM(CASE WHEN was_fallback != 0 THEN 1 ELSE 0 END), 0),
                COALESCE(AVG(latency_ms), 0.0),
                COALESCE(SUM(input_tokens), 0),
                COALESCE(SUM(output_tokens), 0),
                COALESCE(SUM(cost_cents_to_bluey), 0),
                COALESCE(SUM(cost_cents_to_customer), 0)
           FROM usage_events
          WHERE kind = 'llm'
            AND (?1 <= 0 OR ts >= datetime('now', ?2))
          GROUP BY 1, 2, 3
          ORDER BY COUNT(*) DESC, COALESCE(SUM(cost_cents_to_customer), 0) DESC
          LIMIT 100",
    )?;
    let by_provider_model = provider_stmt
        .query_map(params![window_hours, window], |row| {
            Ok(ProviderModelUsageRow {
                provider: row.get(0)?,
                model: row.get(1)?,
                lane: row.get(2)?,
                events: row.get(3)?,
                fallback_events: row.get(4)?,
                avg_latency_ms: row.get(5)?,
                input_tokens: row.get(6)?,
                output_tokens: row.get(7)?,
                cost_cents_to_bluey: row.get(8)?,
                cost_cents_to_customer: row.get(9)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    let by_lane = usage_group_rows_sqlite(&conn, "lane", window_hours)?;
    let by_task_type = usage_group_rows_sqlite(&conn, "task_type", window_hours)?;

    Ok(build_provider_routing_summary(
        window_hours,
        total,
        by_provider_model,
        by_lane,
        by_task_type,
    ))
}

fn usage_group_rows_sqlite(
    conn: &rusqlite::Connection,
    column: &str,
    window_hours: i64,
) -> Result<Vec<UsageGroupRow>> {
    let window = format!("-{window_hours} hours");
    let mut stmt = conn.prepare(&format!(
        "SELECT COALESCE({column}, 'unknown'),
                COUNT(*),
                COALESCE(SUM(CASE WHEN was_fallback != 0 THEN 1 ELSE 0 END), 0),
                COALESCE(AVG(latency_ms), 0.0),
                COALESCE(SUM(input_tokens), 0),
                COALESCE(SUM(output_tokens), 0),
                COALESCE(SUM(cost_cents_to_bluey), 0),
                COALESCE(SUM(cost_cents_to_customer), 0)
           FROM usage_events
          WHERE kind = 'llm'
            AND (?1 <= 0 OR ts >= datetime('now', ?2))
          GROUP BY 1
          ORDER BY COUNT(*) DESC, COALESCE(SUM(cost_cents_to_customer), 0) DESC
          LIMIT 100"
    ))?;
    let rows = stmt
        .query_map(params![window_hours, window], |row| {
            Ok(UsageGroupRow {
                key: row.get(0)?,
                events: row.get(1)?,
                fallback_events: row.get(2)?,
                avg_latency_ms: row.get(3)?,
                input_tokens: row.get(4)?,
                output_tokens: row.get(5)?,
                cost_cents_to_bluey: row.get(6)?,
                cost_cents_to_customer: row.get(7)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

fn provider_routing_summary_postgres(
    pool: &DbPool,
    window_hours: i64,
) -> Result<ProviderRoutingSummary> {
    let mut conn = pool.get_pg()?;
    let total_row = conn.query_one(
        "SELECT COUNT(*)::bigint,
                COALESCE(SUM(CASE WHEN was_fallback != 0 THEN 1 ELSE 0 END), 0)::bigint,
                COALESCE(AVG(latency_ms), 0.0)::double precision,
                COALESCE(SUM(input_tokens), 0)::bigint,
                COALESCE(SUM(output_tokens), 0)::bigint,
                COALESCE(SUM(cost_cents_to_bluey), 0)::bigint,
                COALESCE(SUM(cost_cents_to_customer), 0)::bigint
           FROM usage_events
          WHERE kind = 'llm'
            AND ($1::bigint <= 0 OR ts >= now() - ($1::bigint * interval '1 hour'))",
        &[&window_hours],
    )?;
    let total = (
        total_row.try_get(0)?,
        total_row.try_get(1)?,
        total_row.try_get(2)?,
        total_row.try_get(3)?,
        total_row.try_get(4)?,
        total_row.try_get(5)?,
        total_row.try_get(6)?,
    );

    let by_provider_model = conn
        .query(
            "SELECT COALESCE(provider, 'unknown'),
                    COALESCE(model, 'unknown'),
                    COALESCE(lane, 'unknown'),
                    COUNT(*)::bigint,
                    COALESCE(SUM(CASE WHEN was_fallback != 0 THEN 1 ELSE 0 END), 0)::bigint,
                    COALESCE(AVG(latency_ms), 0.0)::double precision,
                    COALESCE(SUM(input_tokens), 0)::bigint,
                    COALESCE(SUM(output_tokens), 0)::bigint,
                    COALESCE(SUM(cost_cents_to_bluey), 0)::bigint,
                    COALESCE(SUM(cost_cents_to_customer), 0)::bigint
               FROM usage_events
              WHERE kind = 'llm'
                AND ($1::bigint <= 0 OR ts >= now() - ($1::bigint * interval '1 hour'))
              GROUP BY 1, 2, 3
              ORDER BY COUNT(*) DESC, COALESCE(SUM(cost_cents_to_customer), 0) DESC
              LIMIT 100",
            &[&window_hours],
        )?
        .into_iter()
        .map(|row| {
            Ok(ProviderModelUsageRow {
                provider: row.try_get(0)?,
                model: row.try_get(1)?,
                lane: row.try_get(2)?,
                events: row.try_get(3)?,
                fallback_events: row.try_get(4)?,
                avg_latency_ms: row.try_get(5)?,
                input_tokens: row.try_get(6)?,
                output_tokens: row.try_get(7)?,
                cost_cents_to_bluey: row.try_get(8)?,
                cost_cents_to_customer: row.try_get(9)?,
            })
        })
        .collect::<Result<Vec<_>>>()?;

    let by_lane = usage_group_rows_postgres(&mut conn, "lane", window_hours)?;
    let by_task_type = usage_group_rows_postgres(&mut conn, "task_type", window_hours)?;

    Ok(build_provider_routing_summary(
        window_hours,
        total,
        by_provider_model,
        by_lane,
        by_task_type,
    ))
}

fn usage_group_rows_postgres(
    conn: &mut crate::db::PostgresDbConn,
    column: &str,
    window_hours: i64,
) -> Result<Vec<UsageGroupRow>> {
    let sql = format!(
        "SELECT COALESCE({column}, 'unknown'),
                COUNT(*)::bigint,
                COALESCE(SUM(CASE WHEN was_fallback != 0 THEN 1 ELSE 0 END), 0)::bigint,
                COALESCE(AVG(latency_ms), 0.0)::double precision,
                COALESCE(SUM(input_tokens), 0)::bigint,
                COALESCE(SUM(output_tokens), 0)::bigint,
                COALESCE(SUM(cost_cents_to_bluey), 0)::bigint,
                COALESCE(SUM(cost_cents_to_customer), 0)::bigint
           FROM usage_events
          WHERE kind = 'llm'
            AND ($1::bigint <= 0 OR ts >= now() - ($1::bigint * interval '1 hour'))
          GROUP BY 1
          ORDER BY COUNT(*) DESC, COALESCE(SUM(cost_cents_to_customer), 0) DESC
          LIMIT 100"
    );
    let rows = conn
        .query(&sql, &[&window_hours])?
        .into_iter()
        .map(|row| {
            Ok(UsageGroupRow {
                key: row.try_get(0)?,
                events: row.try_get(1)?,
                fallback_events: row.try_get(2)?,
                avg_latency_ms: row.try_get(3)?,
                input_tokens: row.try_get(4)?,
                output_tokens: row.try_get(5)?,
                cost_cents_to_bluey: row.try_get(6)?,
                cost_cents_to_customer: row.try_get(7)?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(rows)
}

fn build_provider_routing_summary(
    window_hours: i64,
    total: (i64, i64, f64, i64, i64, i64, i64),
    by_provider_model: Vec<ProviderModelUsageRow>,
    by_lane: Vec<UsageGroupRow>,
    by_task_type: Vec<UsageGroupRow>,
) -> ProviderRoutingSummary {
    let (
        total_events,
        fallback_events,
        avg_latency_ms,
        input_tokens,
        output_tokens,
        bluey_cost,
        customer_cost,
    ) = total;
    let fallback_rate = if total_events > 0 {
        fallback_events as f64 / total_events as f64
    } else {
        0.0
    };
    ProviderRoutingSummary {
        generated_at_ms: chrono::Utc::now().timestamp_millis(),
        window_hours,
        total_events,
        fallback_events,
        fallback_rate,
        avg_latency_ms,
        input_tokens,
        output_tokens,
        cost_cents_to_bluey: bluey_cost,
        cost_cents_to_customer: customer_cost,
        by_provider_model,
        by_lane,
        by_task_type,
        privacy_note: "Aggregates only; prompts, transcripts, documents, and answer text are intentionally omitted.".to_string(),
        limitation_note: "This report shows final routed provider/model and fallback count from usage events. Persisting first-candidate versus final-provider attempts requires a route-attempt schema addition.".to_string(),
    }
}

pub fn bluey_spend_cents_in_window(pool: &DbPool, window_hours: i64) -> Result<i64> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
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
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let total: i64 = if window_hours > 0 {
                conn.query_one(
                    "SELECT COALESCE(SUM(cost_cents_to_bluey), 0)::bigint
                       FROM usage_events
                      WHERE ts >= now() - ($1::bigint * interval '1 hour')",
                    &[&window_hours],
                )?
                .try_get(0)?
            } else {
                conn.query_one(
                    "SELECT COALESCE(SUM(cost_cents_to_bluey), 0)::bigint FROM usage_events",
                    &[],
                )?
                .try_get(0)?
            };
            Ok(total)
        }
    })
}

pub fn count_task_events_in_window(
    pool: &DbPool,
    account_id: &str,
    task_type: &str,
    window_hours: i64,
) -> Result<i64> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get()?;
            let total = if window_hours > 0 {
                let window = format!("-{window_hours} hours");
                conn.query_row(
                    "SELECT COUNT(*)
                       FROM usage_events
                      WHERE account_id = ?1
                        AND task_type = ?2
                        AND ts >= datetime('now', ?3)",
                    params![account_id, task_type, window],
                    |row| row.get(0),
                )?
            } else {
                conn.query_row(
                    "SELECT COUNT(*)
                       FROM usage_events
                      WHERE account_id = ?1
                        AND task_type = ?2",
                    params![account_id, task_type],
                    |row| row.get(0),
                )?
            };
            Ok(total)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let total: i64 = if window_hours > 0 {
                conn.query_one(
                    "SELECT COUNT(*)::bigint
                       FROM usage_events
                      WHERE account_id = $1
                        AND task_type = $2
                        AND ts >= now() - ($3::bigint * interval '1 hour')",
                    &[&account_id, &task_type, &window_hours],
                )?
                .try_get(0)?
            } else {
                conn.query_one(
                    "SELECT COUNT(*)::bigint
                       FROM usage_events
                      WHERE account_id = $1
                        AND task_type = $2",
                    &[&account_id, &task_type],
                )?
                .try_get(0)?
            };
            Ok(total)
        }
    })
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

    #[test]
    fn count_task_events_in_window_counts_recent_matching_task_type() {
        let pool = temp_pool();
        let id = make_account(&pool);
        let mut recent_search = sample_event("search-recent");
        recent_search.kind = "web_search".into();
        recent_search.task_type = Some("web_search".into());
        let mut old_search = sample_event("search-old");
        old_search.kind = "web_search".into();
        old_search.task_type = Some("web_search".into());
        let mut llm = sample_event("llm-recent");
        llm.task_type = None;

        assert!(record(&pool, &id, &recent_search).unwrap());
        assert!(record(&pool, &id, &old_search).unwrap());
        assert!(record(&pool, &id, &llm).unwrap());
        pool.get()
            .unwrap()
            .execute(
                "UPDATE usage_events SET ts = datetime('now', '-2 days') WHERE request_id = ?1",
                params!["search-old"],
            )
            .unwrap();

        assert_eq!(
            count_task_events_in_window(&pool, &id, "web_search", 24).unwrap(),
            1
        );
        assert_eq!(
            count_task_events_in_window(&pool, &id, "web_search", 0).unwrap(),
            2
        );
    }

    #[test]
    fn provider_routing_summary_groups_final_provider_usage_without_content() {
        let pool = temp_pool();
        let id = make_account(&pool);
        let mut openai = sample_event("route-openai");
        openai.lane = Some("instant".into());
        openai.provider = Some("openai".into());
        openai.model = Some("gpt-5.5-mini".into());
        openai.input_tokens = 100;
        openai.output_tokens = 40;
        openai.latency_ms = 900;
        openai.cost_cents_to_bluey = 2;
        openai.cost_cents_to_customer = 4;

        let mut glm = sample_event("route-glm");
        glm.task_type = Some("code".into());
        glm.lane = Some("deep".into());
        glm.provider = Some("zai".into());
        glm.model = Some("glm-5.2".into());
        glm.input_tokens = 300;
        glm.output_tokens = 200;
        glm.latency_ms = 2500;
        glm.cost_cents_to_bluey = 3;
        glm.cost_cents_to_customer = 7;
        glm.was_fallback = true;

        let mut embed = sample_event("route-embed");
        embed.kind = "embed".into();
        embed.provider = Some("openai".into());

        assert!(record(&pool, &id, &openai).unwrap());
        assert!(record(&pool, &id, &glm).unwrap());
        assert!(record(&pool, &id, &embed).unwrap());

        let summary = provider_routing_summary(&pool, 24).unwrap();
        assert_eq!(summary.total_events, 2);
        assert_eq!(summary.fallback_events, 1);
        assert_eq!(summary.input_tokens, 400);
        assert_eq!(summary.output_tokens, 240);
        assert_eq!(summary.cost_cents_to_bluey, 5);
        assert_eq!(summary.cost_cents_to_customer, 11);
        assert!(summary
            .privacy_note
            .contains("prompts, transcripts, documents"));
        assert_eq!(summary.by_provider_model.len(), 2);
        assert!(summary
            .by_provider_model
            .iter()
            .any(|row| row.provider == "zai"
                && row.model == "glm-5.2"
                && row.lane == "deep"
                && row.fallback_events == 1));
        assert!(summary
            .by_lane
            .iter()
            .any(|row| row.key == "instant" && row.events == 1));
        assert!(summary
            .by_task_type
            .iter()
            .any(|row| row.key == "code" && row.events == 1));
    }
}
