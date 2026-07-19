//! Usage event ingestion + aggregation. Stub for now; the daemon emits
//! events via POST /usage/event after every cue request.

use anyhow::Result;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::db::DbPool;

/// No single Bluey provider request can legitimately cost one million USD.
/// This bound keeps malformed provider telemetry from overflowing durable
/// integer aggregates while remaining far above every supported route cap.
pub const MAX_AUTHORITATIVE_EVENT_COST_CENTS: i64 = 100_000_000;
/// A single provider response cannot legitimately report a billion tokens.
/// Clamp before persistence and aggregation so corrupted telemetry cannot
/// overflow either SQLite integer SUMs or PostgreSQL bigint casts.
pub const MAX_AUTHORITATIVE_EVENT_TOKENS: i64 = 1_000_000_000;
/// Provider latency is bounded to one day per authoritative event. Long-lived
/// streams use dedicated duration accounting rather than an unbounded LLM
/// latency sample.
pub const MAX_AUTHORITATIVE_EVENT_LATENCY_MS: i64 = 86_400_000;
/// Retain anonymous pre-cutover spend for one extra day beyond the configured
/// rolling cap so clock skew or a delayed cleanup cannot reopen the boundary.
pub const CUTOVER_SPEND_BASELINE_GRACE_MS: i64 = 86_400_000;

pub(crate) fn sqlite_saturated_cost_sum<P: rusqlite::Params>(
    conn: &rusqlite::Connection,
    sql: &str,
    params: P,
) -> Result<i64> {
    let mut stmt = conn.prepare(sql)?;
    let mut rows = stmt.query(params)?;
    let mut total = 0_i128;
    while let Some(row) = rows.next()? {
        let value = row
            .get::<_, i64>(0)?
            .clamp(0, MAX_AUTHORITATIVE_EVENT_COST_CENTS);
        total = total.saturating_add(i128::from(value));
    }
    Ok(total.min(i128::from(i64::MAX)) as i64)
}

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
    record_with_origin(pool, account_id, event, "server")
}

pub fn record_client_analytics(
    pool: &DbPool,
    account_id: &str,
    event: &UsageEvent,
) -> Result<bool> {
    if !event.kind.starts_with("client_analytics:")
        || event.cost_cents_to_bluey != 0
        || event.cost_cents_to_customer != 0
        || event.provider.is_some()
        || event.model.is_some()
    {
        anyhow::bail!("invalid client analytics usage event")
    }
    record_with_origin(pool, account_id, event, "client")
}

/// Persist all server-authoritative components of one managed request in a
/// single transaction. This prevents a settled multi-component reservation
/// from disappearing after only its primary LLM row becomes visible.
pub fn record_server_batch(
    pool: &DbPool,
    account_id: &str,
    events: &[UsageEvent],
) -> Result<Vec<bool>> {
    if events.is_empty() {
        return Ok(Vec::new());
    }
    let events = events
        .iter()
        .cloned()
        .map(normalize_authoritative_event)
        .collect::<Vec<_>>();
    let mut identities = std::collections::BTreeSet::new();
    for event in &events {
        if event.request_id.trim().is_empty()
            || event.kind.trim().is_empty()
            || !identities.insert((event.request_id.as_str(), event.kind.as_str()))
        {
            anyhow::bail!("invalid or duplicate authoritative usage event identity")
        }
    }
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
            let mut inserted = Vec::with_capacity(events.len());
            for event in &events {
                inserted.push(
                    tx.execute(
                        "INSERT OR IGNORE INTO usage_events
                            (id, account_id, request_id, origin, kind, task_type, lane,
                             provider, model, input_tokens, output_tokens, latency_ms,
                             cost_cents_to_bluey, cost_cents_to_customer,
                             was_speculative, was_fallback)
                         VALUES (?1, ?2, ?3, 'server', ?4, ?5, ?6, ?7, ?8, ?9,
                                 ?10, ?11, ?12, ?13, ?14, ?15)",
                        params![
                            uuid::Uuid::new_v4().to_string(),
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
                            i64::from(event.was_speculative),
                            i64::from(event.was_fallback),
                        ],
                    )? == 1,
                );
            }
            tx.commit()?;
            Ok(inserted)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            tx.query_one(
                "SELECT pg_advisory_xact_lock(hashtextextended('jobs-provider-cost-global', 0))",
                &[],
            )?;
            let mut inserted = Vec::with_capacity(events.len());
            for event in &events {
                let was_speculative = i32::from(event.was_speculative);
                let was_fallback = i32::from(event.was_fallback);
                inserted.push(
                    tx.execute(
                        "INSERT INTO usage_events
                            (id, account_id, request_id, origin, kind, task_type, lane,
                             provider, model, input_tokens, output_tokens, latency_ms,
                             cost_cents_to_bluey, cost_cents_to_customer,
                             was_speculative, was_fallback)
                         VALUES ($1, $2, $3, 'server', $4, $5, $6, $7, $8, $9,
                                 $10, $11, $12, $13, $14, $15)
                         ON CONFLICT (account_id, request_id, kind) DO NOTHING",
                        &[
                            &uuid::Uuid::new_v4().to_string(),
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
                    )? == 1,
                );
            }
            tx.commit()?;
            Ok(inserted)
        }
    })
}

fn normalize_authoritative_event(mut event: UsageEvent) -> UsageEvent {
    event.input_tokens = event.input_tokens.clamp(0, MAX_AUTHORITATIVE_EVENT_TOKENS);
    event.output_tokens = event.output_tokens.clamp(0, MAX_AUTHORITATIVE_EVENT_TOKENS);
    event.latency_ms = event
        .latency_ms
        .clamp(0, MAX_AUTHORITATIVE_EVENT_LATENCY_MS);
    event.cost_cents_to_bluey = event
        .cost_cents_to_bluey
        .clamp(0, MAX_AUTHORITATIVE_EVENT_COST_CENTS);
    event.cost_cents_to_customer = event
        .cost_cents_to_customer
        .clamp(0, MAX_AUTHORITATIVE_EVENT_COST_CENTS);
    event
}

fn record_with_origin(
    pool: &DbPool,
    account_id: &str,
    event: &UsageEvent,
    origin: &str,
) -> Result<bool> {
    let event = normalize_authoritative_event(event.clone());
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
                    (id, account_id, request_id, origin, kind, task_type, lane, provider, model,
                     input_tokens, output_tokens, latency_ms,
                     cost_cents_to_bluey, cost_cents_to_customer,
                     was_speculative, was_fallback)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
                    params![
                        id,
                        account_id,
                        event.request_id,
                        origin,
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
                let mut tx = conn.transaction()?;
                tx.query_one(
                    "SELECT pg_advisory_xact_lock(hashtextextended('jobs-provider-cost-global', 0))",
                    &[],
                )?;
                let was_speculative = event.was_speculative as i32;
                let was_fallback = event.was_fallback as i32;
                let inserted = tx.execute(
                    "INSERT INTO usage_events
                    (id, account_id, request_id, origin, kind, task_type, lane, provider, model,
                     input_tokens, output_tokens, latency_ms,
                     cost_cents_to_bluey, cost_cents_to_customer,
                     was_speculative, was_fallback)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16)
                 ON CONFLICT (account_id, request_id, kind) DO NOTHING",
                    &[
                        &id,
                        &account_id,
                        &event.request_id,
                        &origin,
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
                tx.commit()?;
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
    let mut stmt = conn.prepare(
        "SELECT kind,
                COALESCE(provider, 'unknown'),
                COALESCE(model, 'unknown'),
                COALESCE(lane, 'unknown'),
                COALESCE(task_type, 'unknown'),
                was_fallback,
                latency_ms,
                input_tokens,
                output_tokens,
                cost_cents_to_bluey,
                cost_cents_to_customer
           FROM usage_events
          WHERE origin = 'server'
            AND (?1 <= 0 OR ts >= datetime('now', ?2))",
    )?;
    let mut rows = stmt.query(params![window_hours, window])?;
    let mut total = UsageAggregate::default();
    let mut provider_groups: BTreeMap<(String, String, String), UsageAggregate> = BTreeMap::new();
    let mut lane_groups: BTreeMap<String, UsageAggregate> = BTreeMap::new();
    let mut task_groups: BTreeMap<String, UsageAggregate> = BTreeMap::new();
    while let Some(row) = rows.next()? {
        let kind: String = row.get(0)?;
        let provider: String = row.get(1)?;
        let model: String = row.get(2)?;
        let lane: String = row.get(3)?;
        let task_type: String = row.get(4)?;
        let sample = UsageSample {
            was_fallback: row.get::<_, i64>(5)? != 0,
            latency_ms: row.get(6)?,
            input_tokens: row.get(7)?,
            output_tokens: row.get(8)?,
            cost_cents_to_bluey: row.get(9)?,
            cost_cents_to_customer: row.get(10)?,
        };
        let provider_group = provider_groups
            .entry((provider, model, lane.clone()))
            .or_default();
        let lane_group = lane_groups.entry(lane).or_default();
        let task_group = task_groups.entry(task_type).or_default();
        if kind.ends_with("_attempt") {
            total.add_provider_sample(sample);
            provider_group.add_provider_sample(sample);
            lane_group.add_provider_sample(sample);
            task_group.add_provider_sample(sample);
        } else {
            total.add_customer_cost(sample.cost_cents_to_customer);
            provider_group.add_customer_cost(sample.cost_cents_to_customer);
            lane_group.add_customer_cost(sample.cost_cents_to_customer);
            task_group.add_customer_cost(sample.cost_cents_to_customer);
        }
    }

    let mut by_provider_model = provider_groups
        .into_iter()
        .map(|((provider, model, lane), aggregate)| {
            let values = aggregate.values();
            ProviderModelUsageRow {
                provider,
                model,
                lane,
                events: values.0,
                fallback_events: values.1,
                avg_latency_ms: values.2,
                input_tokens: values.3,
                output_tokens: values.4,
                cost_cents_to_bluey: values.5,
                cost_cents_to_customer: values.6,
            }
        })
        .collect::<Vec<_>>();
    by_provider_model.sort_by(|left, right| {
        right
            .events
            .cmp(&left.events)
            .then_with(|| {
                right
                    .cost_cents_to_customer
                    .cmp(&left.cost_cents_to_customer)
            })
            .then_with(|| left.provider.cmp(&right.provider))
            .then_with(|| left.model.cmp(&right.model))
            .then_with(|| left.lane.cmp(&right.lane))
    });
    by_provider_model.truncate(100);

    let by_lane = sqlite_usage_group_rows(lane_groups);
    let by_task_type = sqlite_usage_group_rows(task_groups);

    Ok(build_provider_routing_summary(
        window_hours,
        total.values(),
        by_provider_model,
        by_lane,
        by_task_type,
    ))
}

#[derive(Debug, Clone, Copy)]
struct UsageSample {
    was_fallback: bool,
    latency_ms: i64,
    input_tokens: i64,
    output_tokens: i64,
    cost_cents_to_bluey: i64,
    cost_cents_to_customer: i64,
}

#[derive(Debug, Default, Clone, Copy)]
struct UsageAggregate {
    events: i128,
    fallback_events: i128,
    latency_ms: i128,
    input_tokens: i128,
    output_tokens: i128,
    cost_cents_to_bluey: i128,
    cost_cents_to_customer: i128,
}

impl UsageAggregate {
    fn add_provider_sample(&mut self, sample: UsageSample) {
        self.events = self.events.saturating_add(1);
        self.fallback_events = self
            .fallback_events
            .saturating_add(i128::from(sample.was_fallback));
        self.latency_ms = self.latency_ms.saturating_add(i128::from(
            sample
                .latency_ms
                .clamp(0, MAX_AUTHORITATIVE_EVENT_LATENCY_MS),
        ));
        self.input_tokens = self.input_tokens.saturating_add(i128::from(
            sample.input_tokens.clamp(0, MAX_AUTHORITATIVE_EVENT_TOKENS),
        ));
        self.output_tokens = self.output_tokens.saturating_add(i128::from(
            sample
                .output_tokens
                .clamp(0, MAX_AUTHORITATIVE_EVENT_TOKENS),
        ));
        self.cost_cents_to_bluey = self.cost_cents_to_bluey.saturating_add(i128::from(
            sample
                .cost_cents_to_bluey
                .clamp(0, MAX_AUTHORITATIVE_EVENT_COST_CENTS),
        ));
    }

    fn add_customer_cost(&mut self, cost_cents: i64) {
        self.cost_cents_to_customer = self.cost_cents_to_customer.saturating_add(i128::from(
            cost_cents.clamp(0, MAX_AUTHORITATIVE_EVENT_COST_CENTS),
        ));
    }

    fn values(self) -> (i64, i64, f64, i64, i64, i64, i64) {
        let events = saturated_i64(self.events);
        let average_latency = if self.events > 0 {
            self.latency_ms as f64 / self.events as f64
        } else {
            0.0
        };
        (
            events,
            saturated_i64(self.fallback_events),
            average_latency,
            saturated_i64(self.input_tokens),
            saturated_i64(self.output_tokens),
            saturated_i64(self.cost_cents_to_bluey),
            saturated_i64(self.cost_cents_to_customer),
        )
    }
}

fn saturated_i64(value: i128) -> i64 {
    value.clamp(0, i128::from(i64::MAX)) as i64
}

fn sqlite_usage_group_rows(groups: BTreeMap<String, UsageAggregate>) -> Vec<UsageGroupRow> {
    let mut rows = groups
        .into_iter()
        .map(|(key, aggregate)| {
            let values = aggregate.values();
            UsageGroupRow {
                key,
                events: values.0,
                fallback_events: values.1,
                avg_latency_ms: values.2,
                input_tokens: values.3,
                output_tokens: values.4,
                cost_cents_to_bluey: values.5,
                cost_cents_to_customer: values.6,
            }
        })
        .collect::<Vec<_>>();
    rows.sort_by(|left, right| {
        right
            .events
            .cmp(&left.events)
            .then_with(|| {
                right
                    .cost_cents_to_customer
                    .cmp(&left.cost_cents_to_customer)
            })
            .then_with(|| left.key.cmp(&right.key))
    });
    rows.truncate(100);
    rows
}

fn provider_routing_summary_postgres(
    pool: &DbPool,
    window_hours: i64,
) -> Result<ProviderRoutingSummary> {
    let mut conn = pool.get_pg()?;
    let total_row = conn.query_one(
        "SELECT COUNT(*) FILTER (WHERE right(kind, 8) = '_attempt')::bigint,
                COUNT(*) FILTER (WHERE right(kind, 8) = '_attempt' AND was_fallback != 0)::bigint,
                COALESCE(AVG(LEAST(GREATEST(latency_ms, 0), $3)::numeric)
                    FILTER (WHERE right(kind, 8) = '_attempt'), 0)::double precision,
                LEAST(COALESCE(SUM(LEAST(GREATEST(input_tokens, 0), $2)::numeric)
                    FILTER (WHERE right(kind, 8) = '_attempt'), 0), $5::numeric)::bigint,
                LEAST(COALESCE(SUM(LEAST(GREATEST(output_tokens, 0), $2)::numeric)
                    FILTER (WHERE right(kind, 8) = '_attempt'), 0), $5::numeric)::bigint,
                LEAST(COALESCE(SUM(LEAST(GREATEST(cost_cents_to_bluey, 0), $4)::numeric)
                    FILTER (WHERE right(kind, 8) = '_attempt'), 0), $5::numeric)::bigint,
                LEAST(COALESCE(SUM(LEAST(GREATEST(cost_cents_to_customer, 0), $4)::numeric)
                    FILTER (WHERE right(kind, 8) != '_attempt'), 0), $5::numeric)::bigint
           FROM usage_events
          WHERE origin = 'server'
            AND ($1::bigint <= 0 OR ts >= now() - ($1::bigint * interval '1 hour'))",
        &[
            &window_hours,
            &MAX_AUTHORITATIVE_EVENT_TOKENS,
            &MAX_AUTHORITATIVE_EVENT_LATENCY_MS,
            &MAX_AUTHORITATIVE_EVENT_COST_CENTS,
            &i64::MAX,
        ],
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
            "WITH aggregated AS (
                SELECT COALESCE(provider, 'unknown') AS provider,
                       COALESCE(model, 'unknown') AS model,
                       COALESCE(lane, 'unknown') AS lane,
                       COUNT(*) FILTER (WHERE right(kind, 8) = '_attempt')::bigint AS events,
                       COUNT(*) FILTER (WHERE right(kind, 8) = '_attempt' AND was_fallback != 0)::bigint AS fallback_events,
                       COALESCE(AVG(LEAST(GREATEST(latency_ms, 0), $3)::numeric)
                           FILTER (WHERE right(kind, 8) = '_attempt'), 0)::double precision AS avg_latency_ms,
                       LEAST(COALESCE(SUM(LEAST(GREATEST(input_tokens, 0), $2)::numeric)
                           FILTER (WHERE right(kind, 8) = '_attempt'), 0), $5::numeric)::bigint AS input_tokens,
                       LEAST(COALESCE(SUM(LEAST(GREATEST(output_tokens, 0), $2)::numeric)
                           FILTER (WHERE right(kind, 8) = '_attempt'), 0), $5::numeric)::bigint AS output_tokens,
                       LEAST(COALESCE(SUM(LEAST(GREATEST(cost_cents_to_bluey, 0), $4)::numeric)
                           FILTER (WHERE right(kind, 8) = '_attempt'), 0), $5::numeric)::bigint AS bluey_cost,
                       LEAST(COALESCE(SUM(LEAST(GREATEST(cost_cents_to_customer, 0), $4)::numeric)
                           FILTER (WHERE right(kind, 8) != '_attempt'), 0), $5::numeric)::bigint AS customer_cost
                  FROM usage_events
                 WHERE origin = 'server'
                   AND ($1::bigint <= 0 OR ts >= now() - ($1::bigint * interval '1 hour'))
                 GROUP BY 1, 2, 3
            )
            SELECT provider, model, lane, events, fallback_events, avg_latency_ms,
                   input_tokens, output_tokens, bluey_cost, customer_cost
              FROM aggregated
             ORDER BY events DESC, customer_cost DESC, provider, model, lane
             LIMIT 100",
            &[
                &window_hours,
                &MAX_AUTHORITATIVE_EVENT_TOKENS,
                &MAX_AUTHORITATIVE_EVENT_LATENCY_MS,
                &MAX_AUTHORITATIVE_EVENT_COST_CENTS,
                &i64::MAX,
            ],
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
        "WITH aggregated AS (
            SELECT COALESCE({column}, 'unknown') AS group_key,
                   COUNT(*) FILTER (WHERE right(kind, 8) = '_attempt')::bigint AS events,
                   COUNT(*) FILTER (WHERE right(kind, 8) = '_attempt' AND was_fallback != 0)::bigint AS fallback_events,
                   COALESCE(AVG(LEAST(GREATEST(latency_ms, 0), $3)::numeric)
                       FILTER (WHERE right(kind, 8) = '_attempt'), 0)::double precision AS avg_latency_ms,
                   LEAST(COALESCE(SUM(LEAST(GREATEST(input_tokens, 0), $2)::numeric)
                       FILTER (WHERE right(kind, 8) = '_attempt'), 0), $5::numeric)::bigint AS input_tokens,
                   LEAST(COALESCE(SUM(LEAST(GREATEST(output_tokens, 0), $2)::numeric)
                       FILTER (WHERE right(kind, 8) = '_attempt'), 0), $5::numeric)::bigint AS output_tokens,
                   LEAST(COALESCE(SUM(LEAST(GREATEST(cost_cents_to_bluey, 0), $4)::numeric)
                       FILTER (WHERE right(kind, 8) = '_attempt'), 0), $5::numeric)::bigint AS bluey_cost,
                   LEAST(COALESCE(SUM(LEAST(GREATEST(cost_cents_to_customer, 0), $4)::numeric)
                       FILTER (WHERE right(kind, 8) != '_attempt'), 0), $5::numeric)::bigint AS customer_cost
              FROM usage_events
             WHERE origin = 'server'
               AND ($1::bigint <= 0 OR ts >= now() - ($1::bigint * interval '1 hour'))
             GROUP BY 1
        )
        SELECT group_key, events, fallback_events, avg_latency_ms, input_tokens,
               output_tokens, bluey_cost, customer_cost
          FROM aggregated
         ORDER BY events DESC, customer_cost DESC, group_key
         LIMIT 100"
    );
    let rows = conn
        .query(
            &sql,
            &[
                &window_hours,
                &MAX_AUTHORITATIVE_EVENT_TOKENS,
                &MAX_AUTHORITATIVE_EVENT_LATENCY_MS,
                &MAX_AUTHORITATIVE_EVENT_COST_CENTS,
                &i64::MAX,
            ],
        )?
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
        limitation_note: "Provider events, tokens, latency, fallback routing, and upstream cost come from server-authoritative attempt rows; customer cost comes once from customer-facing root rows. Aggregate reports intentionally omit request content.".to_string(),
    }
}

pub fn bluey_spend_cents_in_window(pool: &DbPool, window_hours: i64) -> Result<i64> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get()?;
            let mut total = 0_i128;
            if window_hours > 0 {
                let window = format!("-{window_hours} hours");
                let mut stmt = conn.prepare(
                    "SELECT cost_cents_to_bluey
                       FROM usage_events
                      WHERE origin = 'server' AND ts >= datetime('now', ?1)",
                )?;
                let rows = stmt.query_map(params![window], |row| row.get::<_, i64>(0))?;
                for row in rows {
                    total = total.saturating_add(i128::from(
                        row?.clamp(0, MAX_AUTHORITATIVE_EVENT_COST_CENTS),
                    ));
                }
            } else {
                let mut stmt = conn.prepare(
                    "SELECT cost_cents_to_bluey FROM usage_events WHERE origin = 'server'",
                )?;
                let rows = stmt.query_map([], |row| row.get::<_, i64>(0))?;
                for row in rows {
                    total = total.saturating_add(i128::from(
                        row?.clamp(0, MAX_AUTHORITATIVE_EVENT_COST_CENTS),
                    ));
                }
            }
            Ok(total.min(i128::from(i64::MAX)) as i64)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let total: i64 = if window_hours > 0 {
                conn.query_one(
                    "SELECT LEAST(
                                COALESCE(SUM(LEAST(GREATEST(cost_cents_to_bluey, 0), 100000000)::numeric), 0),
                                9223372036854775807
                            )::bigint
                       FROM usage_events
                      WHERE origin = 'server'
                        AND ts >= now() - ($1::bigint * interval '1 hour')",
                    &[&window_hours],
                )?
                .try_get(0)?
            } else {
                conn.query_one(
                    "SELECT LEAST(
                                COALESCE(SUM(LEAST(GREATEST(cost_cents_to_bluey, 0), 100000000)::numeric), 0),
                                9223372036854775807
                            )::bigint
                       FROM usage_events WHERE origin = 'server'",
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
                      WHERE origin = 'server'
                        AND account_id = ?1
                        AND task_type = ?2
                        AND ts >= datetime('now', ?3)",
                    params![account_id, task_type, window],
                    |row| row.get(0),
                )?
            } else {
                conn.query_row(
                    "SELECT COUNT(*)
                       FROM usage_events
                      WHERE origin = 'server'
                        AND account_id = ?1
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
                      WHERE origin = 'server'
                        AND account_id = $1
                        AND task_type = $2
                        AND ts >= now() - ($3::bigint * interval '1 hour')",
                    &[&account_id, &task_type, &window_hours],
                )?
                .try_get(0)?
            } else {
                conn.query_one(
                    "SELECT COUNT(*)::bigint
                       FROM usage_events
                      WHERE origin = 'server'
                        AND account_id = $1
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
        openai.kind = "llm_attempt".into();
        openai.lane = Some("instant".into());
        openai.provider = Some("openai".into());
        openai.model = Some("gpt-5.5-mini".into());
        openai.input_tokens = 100;
        openai.output_tokens = 40;
        openai.latency_ms = 900;
        openai.cost_cents_to_bluey = 2;
        openai.cost_cents_to_customer = 0;
        let mut openai_root = openai.clone();
        openai_root.kind = "llm".into();
        openai_root.cost_cents_to_bluey = 0;
        openai_root.cost_cents_to_customer = 4;

        let mut glm = sample_event("route-glm");
        glm.kind = "llm_attempt".into();
        glm.task_type = Some("code".into());
        glm.lane = Some("deep".into());
        glm.provider = Some("zai".into());
        glm.model = Some("glm-5.2".into());
        glm.input_tokens = 300;
        glm.output_tokens = 200;
        glm.latency_ms = 2500;
        glm.cost_cents_to_bluey = 3;
        glm.cost_cents_to_customer = 0;
        glm.was_fallback = true;
        let mut glm_root = glm.clone();
        glm_root.kind = "llm".into();
        glm_root.cost_cents_to_bluey = 0;
        glm_root.cost_cents_to_customer = 7;

        let mut embed = sample_event("route-embed");
        embed.kind = "embed_attempt".into();
        embed.task_type = Some("embed".into());
        embed.lane = None;
        embed.provider = Some("openai".into());
        embed.model = Some("text-embedding-3-small".into());
        embed.input_tokens = 5;
        embed.output_tokens = 0;
        embed.cost_cents_to_bluey = 1;
        embed.cost_cents_to_customer = 0;
        let mut embed_root = embed.clone();
        embed_root.kind = "embed".into();
        embed_root.cost_cents_to_bluey = 0;
        embed_root.cost_cents_to_customer = 2;

        assert!(record(&pool, &id, &openai).unwrap());
        assert!(record(&pool, &id, &openai_root).unwrap());
        assert!(record(&pool, &id, &glm).unwrap());
        assert!(record(&pool, &id, &glm_root).unwrap());
        assert!(record(&pool, &id, &embed).unwrap());
        assert!(record(&pool, &id, &embed_root).unwrap());

        let summary = provider_routing_summary(&pool, 24).unwrap();
        assert_eq!(summary.total_events, 3);
        assert_eq!(summary.fallback_events, 1);
        assert_eq!(summary.input_tokens, 405);
        assert_eq!(summary.output_tokens, 240);
        assert_eq!(summary.cost_cents_to_bluey, 6);
        assert_eq!(summary.cost_cents_to_customer, 13);
        assert!(summary
            .privacy_note
            .contains("prompts, transcripts, documents"));
        assert_eq!(summary.by_provider_model.len(), 3);
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

    #[test]
    fn provider_summary_counts_each_route_attempt_and_customer_root_once() {
        let pool = temp_pool();
        let id = make_account(&pool);
        let routes = [
            ("llm_attempt", "llm"),
            ("jobs_resume_generation_attempt", "jobs_resume_generation"),
            ("stt_live_attempt", "stt"),
            ("web_search_attempt", "web_search"),
            ("embed_attempt", "embed"),
            ("stt_attempt", "transcribe"),
        ];
        for (index, (attempt_kind, root_kind)) in routes.into_iter().enumerate() {
            let mut attempt = sample_event(&format!("attempt-{index}"));
            attempt.kind = attempt_kind.into();
            attempt.provider = Some(format!("provider-{index}"));
            attempt.model = Some(format!("model-{index}"));
            attempt.input_tokens = (index + 1) as i64;
            attempt.output_tokens = 1;
            attempt.cost_cents_to_bluey = (index + 1) as i64;
            // Attempt-side customer cost must never leak into customer totals.
            attempt.cost_cents_to_customer = 99;
            attempt.was_fallback = index == 1;
            assert!(record(&pool, &id, &attempt).unwrap());

            let mut root = sample_event(&format!("root-{index}"));
            root.kind = root_kind.into();
            root.provider = attempt.provider.clone();
            root.model = attempt.model.clone();
            // Root-side upstream cost must never double count the hold-backed
            // attempt authority, even if a malformed legacy row reports one.
            root.cost_cents_to_bluey = 99;
            root.cost_cents_to_customer = 1;
            assert!(record(&pool, &id, &root).unwrap());
        }

        let summary = provider_routing_summary(&pool, 0).unwrap();
        assert_eq!(summary.total_events, 6);
        assert_eq!(summary.fallback_events, 1);
        assert_eq!(summary.input_tokens, 21);
        assert_eq!(summary.output_tokens, 6);
        assert_eq!(summary.cost_cents_to_bluey, 21);
        assert_eq!(summary.cost_cents_to_customer, 6);
        assert_eq!(summary.by_provider_model.len(), 6);
        assert!(summary.by_provider_model.iter().any(|row| {
            row.provider == "provider-1"
                && row.events == 1
                && row.fallback_events == 1
                && row.cost_cents_to_bluey == 2
                && row.cost_cents_to_customer == 1
        }));
    }

    #[test]
    fn provider_routing_summary_clamps_hostile_rows_without_sqlite_sum_overflow() {
        let pool = temp_pool();
        let id = make_account(&pool);
        let conn = pool.get().unwrap();
        for request_id in ["hostile-1", "hostile-2"] {
            conn.execute(
                "INSERT INTO usage_events
                    (id, account_id, request_id, origin, kind, task_type, lane,
                     provider, model, input_tokens, output_tokens, latency_ms,
                     cost_cents_to_bluey, cost_cents_to_customer,
                     was_speculative, was_fallback)
                 VALUES (?1, ?2, ?3, 'server', 'llm_attempt', 'hostile', 'deep',
                         'hostile-provider', 'hostile-model', ?4, ?4, ?4,
                         ?4, ?4, 0, 1)",
                params![uuid::Uuid::new_v4().to_string(), id, request_id, i64::MAX,],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO usage_events
                    (id, account_id, request_id, origin, kind, task_type, lane,
                     provider, model, input_tokens, output_tokens, latency_ms,
                     cost_cents_to_bluey, cost_cents_to_customer,
                     was_speculative, was_fallback)
                 VALUES (?1, ?2, ?3, 'server', 'llm', 'hostile', 'deep',
                         'hostile-provider', 'hostile-model', ?4, ?4, ?4,
                         ?4, ?4, 0, 1)",
                params![uuid::Uuid::new_v4().to_string(), id, request_id, i64::MAX,],
            )
            .unwrap();
        }
        drop(conn);

        let summary = provider_routing_summary(&pool, 0).unwrap();
        assert_eq!(summary.total_events, 2);
        assert_eq!(summary.fallback_events, 2);
        assert_eq!(summary.input_tokens, MAX_AUTHORITATIVE_EVENT_TOKENS * 2);
        assert_eq!(summary.output_tokens, MAX_AUTHORITATIVE_EVENT_TOKENS * 2);
        assert_eq!(
            summary.avg_latency_ms,
            MAX_AUTHORITATIVE_EVENT_LATENCY_MS as f64
        );
        assert_eq!(
            summary.cost_cents_to_bluey,
            MAX_AUTHORITATIVE_EVENT_COST_CENTS * 2
        );
        assert_eq!(
            summary.cost_cents_to_customer,
            MAX_AUTHORITATIVE_EVENT_COST_CENTS * 2
        );
        assert_eq!(summary.by_provider_model[0].input_tokens, 2_000_000_000);
        assert_eq!(summary.by_lane[0].cost_cents_to_customer, 200_000_000);
    }

    #[test]
    fn only_server_origin_is_authoritative_and_cutover_is_one_time() {
        let pool = temp_pool();
        let id = make_account(&pool);
        let mut server = sample_event("server-event");
        server.kind = "llm_attempt".into();
        server.cost_cents_to_bluey = 7;
        assert!(record(&pool, &id, &server).unwrap());

        let mut client = sample_event("client-event");
        client.kind = "client_analytics:timing".into();
        client.task_type = Some("general".into());
        client.provider = None;
        client.model = None;
        client.cost_cents_to_bluey = 0;
        client.cost_cents_to_customer = 0;
        assert!(record_client_analytics(&pool, &id, &client).unwrap());

        pool.get()
            .unwrap()
            .execute(
                "INSERT INTO usage_events (id, account_id, request_id, kind,
                    task_type, cost_cents_to_bluey)
                 VALUES (?1, ?2, 'legacy-event', 'llm', 'general', 999999999)",
                params![uuid::Uuid::new_v4().to_string(), id],
            )
            .unwrap();
        run_migrations(&pool).unwrap();

        assert_eq!(bluey_spend_cents_in_window(&pool, 0).unwrap(), 7);
        assert_eq!(
            count_task_events_in_window(&pool, &id, "general", 0).unwrap(),
            1
        );
        assert_eq!(provider_routing_summary(&pool, 0).unwrap().total_events, 1);
        let origins: Vec<String> = {
            let conn = pool.get().unwrap();
            let mut stmt = conn
                .prepare("SELECT origin FROM usage_events ORDER BY request_id")
                .unwrap();
            stmt.query_map([], |row| row.get(0))
                .unwrap()
                .collect::<Result<_, _>>()
                .unwrap()
        };
        assert!(origins.contains(&"server".to_string()));
        assert!(origins.contains(&"client".to_string()));
        assert!(origins.contains(&"legacy_unverified".to_string()));
    }

    #[test]
    fn database_rejects_unknown_or_null_usage_origins() {
        let pool = temp_pool();
        let id = make_account(&pool);
        let conn = pool.get().unwrap();
        for origin in [Some("forged"), None] {
            assert!(conn
                .execute(
                    "INSERT INTO usage_events (id, account_id, request_id, origin, kind)
                     VALUES (?1, ?2, ?3, ?4, 'llm')",
                    params![
                        uuid::Uuid::new_v4().to_string(),
                        id,
                        uuid::Uuid::new_v4().to_string(),
                        origin
                    ],
                )
                .is_err());
        }
    }
}
