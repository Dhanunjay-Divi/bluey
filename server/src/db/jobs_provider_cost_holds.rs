//! Atomic projected-cost holds for Jobs managed resume provider attempts.
//!
//! A hold is persisted immediately before network dispatch. Global and
//! per-generation ceilings include both settled usage and in-flight exposure,
//! so concurrent requests and uncertain timeout/cancellation outcomes cannot
//! disappear from spend accounting.

use anyhow::Result;
use rusqlite::{params, OptionalExtension, TransactionBehavior};
use sha2::{Digest, Sha256};

use crate::{
    config::{UpstreamSpendGuard, UPSTREAM_SPEND_TRUTH_RETENTION_MS},
    pricing::UsageProvenance,
};

use super::{
    usage::{
        UsageEvent, MAX_AUTHORITATIVE_EVENT_COST_CENTS, MAX_AUTHORITATIVE_EVENT_LATENCY_MS,
        MAX_AUTHORITATIVE_EVENT_TOKENS,
    },
    DbPool,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CostHoldReservation {
    Held { reservation_token: String },
    RecoveredAmbiguous { reservation_token: String },
    GenerationLimit,
    GlobalLimit,
}

#[cfg(test)]
thread_local! {
    static FAIL_NEXT_SETTLEMENT_FOR_TEST: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[cfg(test)]
pub(crate) fn fail_next_settlement_for_test() {
    FAIL_NEXT_SETTLEMENT_FOR_TEST.with(|flag| flag.set(true));
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpendTruthCleanup {
    pub provider_holds_deleted: u64,
    pub cutover_baseline_rows_deleted: u64,
}

pub(super) fn sqlite_transaction_now_ms(tx: &rusqlite::Transaction<'_>) -> Result<i64> {
    Ok(tx.query_row(
        "SELECT CAST((julianday('now') - 2440587.5) * 86400000 AS INTEGER)",
        [],
        |row| row.get(0),
    )?)
}

pub(super) fn postgres_transaction_now_ms(tx: &mut postgres::Transaction<'_>) -> Result<i64> {
    Ok(tx
        .query_one(
            "SELECT (EXTRACT(EPOCH FROM transaction_timestamp()) * 1000)::bigint",
            &[],
        )?
        .get(0))
}

/// Delete anonymous cutover evidence and pseudonymous provider holds only
/// after the fixed maximum configurable spend window plus grace. This is an
/// independent transaction so admission denial or a lack of paid traffic can
/// never postpone privacy cleanup indefinitely.
pub fn prune_expired_spend_truth(pool: &DbPool) -> Result<SpendTruthCleanup> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let now = sqlite_transaction_now_ms(&tx)?;
            let retention_cutoff = now.saturating_sub(UPSTREAM_SPEND_TRUTH_RETENTION_MS);
            let provider_holds_deleted = tx.execute(
                "DELETE FROM jobs_provider_cost_holds WHERE updated_at_ms < ?1",
                params![retention_cutoff],
            )?;
            let cutover_baseline_rows_deleted = tx.execute(
                "DELETE FROM usage_cutover_spend_baseline
                  WHERE occurred_at < datetime(?1 / 1000, 'unixepoch')",
                params![retention_cutoff],
            )?;
            tx.commit()?;
            Ok(SpendTruthCleanup {
                provider_holds_deleted: provider_holds_deleted as u64,
                cutover_baseline_rows_deleted: cutover_baseline_rows_deleted as u64,
            })
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            tx.query_one(
                "SELECT pg_advisory_xact_lock(hashtextextended('jobs-provider-cost-global', 0))",
                &[],
            )?;
            let now = postgres_transaction_now_ms(&mut tx)?;
            let retention_cutoff = now.saturating_sub(UPSTREAM_SPEND_TRUTH_RETENTION_MS);
            let provider_holds_deleted = tx.execute(
                "DELETE FROM jobs_provider_cost_holds WHERE updated_at_ms < $1",
                &[&retention_cutoff],
            )?;
            let cutover_baseline_rows_deleted = tx.execute(
                "DELETE FROM usage_cutover_spend_baseline
                  WHERE occurred_at < to_timestamp(($1::bigint)::double precision / 1000.0)",
                &[&retention_cutoff],
            )?;
            tx.commit()?;
            Ok(SpendTruthCleanup {
                provider_holds_deleted,
                cutover_baseline_rows_deleted,
            })
        }
    })
}

pub fn spawn_spend_truth_janitor(pool: DbPool) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let period = std::time::Duration::from_secs(60 * 60);
        let mut interval = tokio::time::interval_at(tokio::time::Instant::now() + period, period);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            interval.tick().await;
            match prune_expired_spend_truth(&pool) {
                Ok(cleanup)
                    if cleanup.provider_holds_deleted > 0
                        || cleanup.cutover_baseline_rows_deleted > 0 =>
                {
                    tracing::info!(
                        provider_holds_deleted = cleanup.provider_holds_deleted,
                        cutover_baseline_rows_deleted = cleanup.cutover_baseline_rows_deleted,
                        "expired upstream spend truth pruned"
                    );
                }
                Ok(_) => {}
                Err(error) => {
                    tracing::error!(error = %error, "upstream spend truth janitor failed")
                }
            }
        }
    })
}

fn opaque_scope_hash(domain: &str, parts: &[&str]) -> String {
    let mut digest = Sha256::new();
    digest.update(b"bluey-provider-cost-hold-v1\0");
    digest.update(domain.as_bytes());
    for part in parts {
        digest.update((part.len() as u64).to_be_bytes());
        digest.update(part.as_bytes());
    }
    hex::encode(digest.finalize())
}

pub(crate) fn account_scope_hash(account_id: &str) -> String {
    opaque_scope_hash("account", &[account_id])
}

pub(crate) fn generation_scope_hash(account_id: &str, generation_key: &str) -> String {
    opaque_scope_hash("generation", &[account_id, generation_key])
}

fn request_scope_hash(account_id: &str, request_id: &str) -> String {
    opaque_scope_hash("request", &[account_id, request_id])
}

fn root_scope_material(generation_key: &str) -> &str {
    if generation_key.starts_with("router:") {
        generation_key
            .rsplit_once(':')
            .map(|(root, _)| root)
            .unwrap_or(generation_key)
    } else {
        generation_key
    }
}

fn root_scope_hash(account_id: &str, generation_key: &str) -> String {
    opaque_scope_hash("root", &[account_id, root_scope_material(generation_key)])
}

fn prefix_root_scope_hash(account_id: &str, scope_prefix: &str) -> String {
    opaque_scope_hash(
        "root",
        &[
            account_id,
            scope_prefix.strip_suffix(':').unwrap_or(scope_prefix),
        ],
    )
}

pub fn has_generation_exposure(
    pool: &DbPool,
    account_id: &str,
    generation_key: &str,
) -> Result<bool> {
    let account_scope_hash = account_scope_hash(account_id);
    let generation_scope_hash = generation_scope_hash(account_id, generation_key);
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get()?;
            Ok(conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM jobs_provider_cost_holds
                  WHERE account_scope_hash = ?1 AND generation_scope_hash = ?2
                    AND status IN ('held', 'settled')
                    AND updated_at_ms >= CAST((julianday('now') - 2440587.5) * 86400000 AS INTEGER) - ?3)",
                params![
                    account_scope_hash,
                    generation_scope_hash,
                    UPSTREAM_SPEND_TRUTH_RETENTION_MS
                ],
                |row| row.get(0),
            )?)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            Ok(conn
                .query_one(
                    "SELECT EXISTS(SELECT 1 FROM jobs_provider_cost_holds
                      WHERE account_scope_hash = $1 AND generation_scope_hash = $2
                        AND status IN ('held', 'settled')
                        AND updated_at_ms >= (EXTRACT(EPOCH FROM statement_timestamp()) * 1000)::bigint - $3)",
                    &[
                        &account_scope_hash,
                        &generation_scope_hash,
                        &UPSTREAM_SPEND_TRUTH_RETENTION_MS,
                    ],
                )?
                .get(0))
        }
    })
}

pub fn has_scope_prefix_exposure(
    pool: &DbPool,
    account_id: &str,
    scope_prefix: &str,
) -> Result<bool> {
    let account_scope_hash = account_scope_hash(account_id);
    let root_scope_hash = prefix_root_scope_hash(account_id, scope_prefix);
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get()?;
            Ok(conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM jobs_provider_cost_holds
                  WHERE account_scope_hash = ?1 AND root_scope_hash = ?2
                    AND status IN ('held', 'settled')
                    AND updated_at_ms >= CAST((julianday('now') - 2440587.5) * 86400000 AS INTEGER) - ?3)",
                params![
                    account_scope_hash,
                    root_scope_hash,
                    UPSTREAM_SPEND_TRUTH_RETENTION_MS
                ],
                |row| row.get(0),
            )?)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            Ok(conn
                .query_one(
                    "SELECT EXISTS(SELECT 1 FROM jobs_provider_cost_holds
                      WHERE account_scope_hash = $1 AND root_scope_hash = $2
                        AND status IN ('held', 'settled')
                        AND updated_at_ms >= (EXTRACT(EPOCH FROM statement_timestamp()) * 1000)::bigint - $3)",
                    &[
                        &account_scope_hash,
                        &root_scope_hash,
                        &UPSTREAM_SPEND_TRUTH_RETENTION_MS,
                    ],
                )?
                .get(0))
        }
    })
}

#[allow(clippy::too_many_arguments)]
pub fn reserve(
    pool: &DbPool,
    account_id: &str,
    generation_key: &str,
    reservation_token: &str,
    request_id: &str,
    provider: &str,
    model: &str,
    projected_cost_cents: i64,
    generation_limit_cents: i64,
    guard: UpstreamSpendGuard,
) -> Result<CostHoldReservation> {
    if projected_cost_cents <= 0
        || projected_cost_cents > MAX_AUTHORITATIVE_EVENT_COST_CENTS
        || generation_limit_cents <= 0
    {
        anyhow::bail!("Jobs provider cost hold must be positive")
    }
    let account_scope_hash = account_scope_hash(account_id);
    let generation_scope_hash = generation_scope_hash(account_id, generation_key);
    let request_scope_hash = request_scope_hash(account_id, request_id);
    let root_scope_hash = root_scope_hash(account_id, generation_key);
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let now = sqlite_transaction_now_ms(&tx)?;
            let window_start = now.saturating_sub(guard.window_hours.saturating_mul(3_600_000));
            let retention_cutoff = now.saturating_sub(UPSTREAM_SPEND_TRUTH_RETENTION_MS);
            tx.execute(
                "DELETE FROM jobs_provider_cost_holds WHERE updated_at_ms < ?1",
                params![retention_cutoff],
            )?;
            tx.execute(
                "DELETE FROM usage_cutover_spend_baseline
                  WHERE occurred_at < datetime(?1 / 1000, 'unixepoch')",
                params![retention_cutoff],
            )?;
            let existing: Option<(String, String, String, String, i64, String, String)> = tx
                .query_row(
                    "SELECT account_scope_hash, generation_scope_hash, provider, model,
                            projected_cost_cents, status, reservation_token
                       FROM jobs_provider_cost_holds WHERE request_scope_hash = ?1",
                    params![request_scope_hash],
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
                )
                .optional()?;
            if let Some((
                held_account,
                held_generation,
                held_provider,
                held_model,
                cost,
                status,
                held_token,
            )) = existing
            {
                if held_account != account_scope_hash
                    || held_generation != generation_scope_hash
                    || held_provider != provider
                    || held_model != model
                    || cost != projected_cost_cents
                {
                    anyhow::bail!("Jobs provider cost hold identity collision")
                }
                tx.commit()?;
                if status == "held" {
                    return Ok(CostHoldReservation::RecoveredAmbiguous {
                        reservation_token: held_token,
                    });
                }
                anyhow::bail!("Jobs provider cost hold was already terminal")
            }
            let jobs_fence: Option<(String, String, String)> = tx
                .query_row(
                    "SELECT reservation_token, status, job_id FROM jobs_resume_generations
                      WHERE account_id = ?1 AND generation_key = ?2",
                    params![account_id, generation_key],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .optional()?;
            if let Some((active_token, status, job_id)) = jobs_fence {
                if active_token != reservation_token || status != "reserved" {
                    anyhow::bail!("Jobs provider cost hold lost its generation lease fence")
                }
                let hold_context = super::jobs::operational_hold_context_for_job_sqlite_tx(
                    &tx,
                    account_id,
                    &job_id,
                    Some(provider),
                    Some(model),
                )
                .map_err(anyhow::Error::new)?;
                super::jobs::require_operational_capability_sqlite_tx(
                    &tx,
                    super::jobs::OperationalCapability::Generation,
                    &hold_context,
                )
                .map_err(anyhow::Error::new)?;
                tx.execute(
                    "UPDATE jobs_resume_generations SET updated_at_ms = ?3
                      WHERE account_id = ?1 AND generation_key = ?2
                        AND reservation_token = ?4 AND status = 'reserved'",
                    params![account_id, generation_key, now, reservation_token],
                )?;
            }
            let generation_exposure = super::usage::sqlite_saturated_cost_sum(
                &tx,
                "SELECT CASE WHEN status = 'held'
                             THEN projected_cost_cents ELSE settled_cost_cents END
                   FROM jobs_provider_cost_holds
                  WHERE account_scope_hash = ?1 AND generation_scope_hash = ?2
                    AND status IN ('held', 'settled')",
                params![account_scope_hash, generation_scope_hash],
            )?;
            if generation_exposure.saturating_add(projected_cost_cents) > generation_limit_cents {
                tx.commit()?;
                return Ok(CostHoldReservation::GenerationLimit);
            }
            let usage_without_holds = super::usage::sqlite_saturated_cost_sum(
                &tx,
                "SELECT u.cost_cents_to_bluey
                   FROM usage_events u
                  WHERE u.origin = 'server' AND u.ts >= datetime(?1 / 1000, 'unixepoch')
                    AND substr(u.kind, -8) != '_attempt'",
                params![window_start],
            )?;
            let held_exposure = super::usage::sqlite_saturated_cost_sum(
                &tx,
                "SELECT CASE WHEN status = 'held'
                             THEN projected_cost_cents ELSE settled_cost_cents END
                   FROM jobs_provider_cost_holds
                  WHERE status IN ('held', 'settled') AND updated_at_ms >= ?1",
                params![window_start],
            )?;
            let cutover_baseline_exposure = super::usage::sqlite_saturated_cost_sum(
                &tx,
                "SELECT cost_cents FROM usage_cutover_spend_baseline
                  WHERE occurred_at >= datetime(?1 / 1000, 'unixepoch')",
                params![window_start],
            )?;
            let ordinary_managed_exposure = super::usage::sqlite_saturated_cost_sum(
                &tx,
                "SELECT estimated_upstream_cents
                   FROM usage_reservations r
                  WHERE (r.status = 'reserved' AND r.expires_at_ms > ?1)
                     OR (r.status = 'settled'
                       AND r.settled_at_ms >= ?2
                       AND NOT EXISTS (
                           SELECT 1 FROM usage_events u
                            WHERE u.account_id = r.account_id
                              AND u.request_id = r.request_id
                              AND u.origin = 'server'
                              AND u.kind = r.kind
                       ))",
                params![now, window_start],
            )?;
            if usage_without_holds
                .saturating_add(held_exposure)
                .saturating_add(cutover_baseline_exposure)
                .saturating_add(ordinary_managed_exposure)
                .saturating_add(projected_cost_cents)
                > guard.limit_cents
            {
                tx.commit()?;
                return Ok(CostHoldReservation::GlobalLimit);
            }
            tx.execute(
                "INSERT INTO jobs_provider_cost_holds (
                    request_scope_hash, account_scope_hash, generation_scope_hash,
                    root_scope_hash, reservation_token,
                    provider, model, projected_cost_cents, status,
                    created_at_ms, updated_at_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'held', ?9, ?9)",
                params![
                    request_scope_hash,
                    account_scope_hash,
                    generation_scope_hash,
                    root_scope_hash,
                    reservation_token,
                    provider,
                    model,
                    projected_cost_cents,
                    now
                ],
            )?;
            tx.commit()?;
            Ok(CostHoldReservation::Held {
                reservation_token: reservation_token.to_string(),
            })
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            super::jobs::lock_operational_hold_shared_postgres_tx(&mut tx)
                .map_err(anyhow::Error::new)?;
            super::jobs::lock_discovery_account_shared_postgres(&mut tx, account_id)?;
            let now = postgres_transaction_now_ms(&mut tx)?;
            let window_start = now.saturating_sub(guard.window_hours.saturating_mul(3_600_000));
            let retention_cutoff = now.saturating_sub(UPSTREAM_SPEND_TRUTH_RETENTION_MS);
            tx.query_one(
                "SELECT pg_advisory_xact_lock(hashtextextended('jobs-provider-cost-global', 0))",
                &[],
            )?;
            tx.execute(
                "DELETE FROM jobs_provider_cost_holds WHERE updated_at_ms < $1",
                &[&retention_cutoff],
            )?;
            tx.execute(
                "DELETE FROM usage_cutover_spend_baseline
                  WHERE occurred_at < to_timestamp(($1::bigint)::double precision / 1000.0)",
                &[&retention_cutoff],
            )?;
            let existing = tx.query_opt(
                "SELECT account_scope_hash, generation_scope_hash, provider, model,
                        projected_cost_cents, status, reservation_token
                   FROM jobs_provider_cost_holds WHERE request_scope_hash = $1 FOR UPDATE",
                &[&request_scope_hash],
            )?;
            if let Some(row) = existing {
                let held_account: String = row.get(0);
                let held_generation: String = row.get(1);
                let held_provider: String = row.get(2);
                let held_model: String = row.get(3);
                let cost: i64 = row.get(4);
                let status: String = row.get(5);
                let held_token: String = row.get(6);
                if held_account != account_scope_hash
                    || held_generation != generation_scope_hash
                    || held_provider != provider
                    || held_model != model
                    || cost != projected_cost_cents
                {
                    anyhow::bail!("Jobs provider cost hold identity collision")
                }
                tx.commit()?;
                if status == "held" {
                    return Ok(CostHoldReservation::RecoveredAmbiguous {
                        reservation_token: held_token,
                    });
                }
                anyhow::bail!("Jobs provider cost hold was already terminal")
            }
            let jobs_fence = tx.query_opt(
                "SELECT reservation_token, status, job_id FROM jobs_resume_generations
                  WHERE account_id = $1 AND generation_key = $2 FOR UPDATE",
                &[&account_id, &generation_key],
            )?;
            if let Some(row) = jobs_fence {
                let active_token: String = row.get(0);
                let status: String = row.get(1);
                let job_id: String = row.get(2);
                if active_token != reservation_token || status != "reserved" {
                    anyhow::bail!("Jobs provider cost hold lost its generation lease fence")
                }
                let hold_context = super::jobs::operational_hold_context_for_job_postgres_tx(
                    &mut tx,
                    account_id,
                    &job_id,
                    Some(provider),
                    Some(model),
                )
                .map_err(anyhow::Error::new)?;
                super::jobs::require_operational_capability_postgres_tx(
                    &mut tx,
                    super::jobs::OperationalCapability::Generation,
                    &hold_context,
                )
                .map_err(anyhow::Error::new)?;
                tx.execute(
                    "UPDATE jobs_resume_generations SET updated_at_ms = $3
                      WHERE account_id = $1 AND generation_key = $2
                        AND reservation_token = $4 AND status = 'reserved'",
                    &[&account_id, &generation_key, &now, &reservation_token],
                )?;
            }
            let generation_exposure: i64 = tx
                .query_one(
                    "SELECT LEAST(COALESCE(SUM(LEAST(GREATEST(CASE WHEN status = 'held'
                                             THEN projected_cost_cents
                                             ELSE settled_cost_cents END, 0), 100000000)::numeric), 0),
                                      9223372036854775807)::bigint
                       FROM jobs_provider_cost_holds
                      WHERE account_scope_hash = $1 AND generation_scope_hash = $2
                        AND status IN ('held', 'settled')",
                    &[&account_scope_hash, &generation_scope_hash],
                )?
                .get(0);
            if generation_exposure.saturating_add(projected_cost_cents) > generation_limit_cents {
                tx.commit()?;
                return Ok(CostHoldReservation::GenerationLimit);
            }
            let usage_without_holds: i64 = tx
                .query_one(
                    "SELECT LEAST(COALESCE(SUM(LEAST(GREATEST(u.cost_cents_to_bluey, 0), 100000000)::numeric), 0),
                                      9223372036854775807)::bigint
                      FROM usage_events u
                     WHERE u.origin = 'server'
                        AND u.ts >= to_timestamp(($1::bigint)::double precision / 1000.0)
                        AND right(u.kind, 8) != '_attempt'",
                    &[&window_start],
                )?
                .get(0);
            let held_exposure: i64 = tx
                .query_one(
                    "SELECT LEAST(COALESCE(SUM(LEAST(GREATEST(CASE WHEN status = 'held'
                                             THEN projected_cost_cents
                                             ELSE settled_cost_cents END, 0), 100000000)::numeric), 0),
                                      9223372036854775807)::bigint
                       FROM jobs_provider_cost_holds
                      WHERE status IN ('held', 'settled') AND updated_at_ms >= $1",
                    &[&window_start],
                )?
                .get(0);
            let cutover_baseline_exposure: i64 = tx
                .query_one(
                    "SELECT LEAST(COALESCE(SUM(LEAST(GREATEST(cost_cents, 0), 100000000)::numeric), 0),
                                      9223372036854775807)::bigint
                       FROM usage_cutover_spend_baseline
                      WHERE occurred_at >= to_timestamp(($1::bigint)::double precision / 1000.0)",
                    &[&window_start],
                )?
                .get(0);
            let ordinary_managed_exposure: i64 = tx
                .query_one(
                    "SELECT LEAST(COALESCE(SUM(LEAST(GREATEST(estimated_upstream_cents, 0), 100000000)::numeric), 0),
                                      9223372036854775807)::bigint
                       FROM usage_reservations r
                      WHERE (r.status = 'reserved' AND r.expires_at_ms > $1)
                         OR (r.status = 'settled'
                           AND r.settled_at_ms >= $2
                           AND NOT EXISTS (
                               SELECT 1 FROM usage_events u
                                WHERE u.account_id = r.account_id
                                  AND u.request_id = r.request_id
                                  AND u.origin = 'server'
                                  AND u.kind = r.kind
                           ))",
                    &[&now, &window_start],
                )?
                .get(0);
            if usage_without_holds
                .saturating_add(held_exposure)
                .saturating_add(cutover_baseline_exposure)
                .saturating_add(ordinary_managed_exposure)
                .saturating_add(projected_cost_cents)
                > guard.limit_cents
            {
                tx.commit()?;
                return Ok(CostHoldReservation::GlobalLimit);
            }
            tx.execute(
                "INSERT INTO jobs_provider_cost_holds (
                    request_scope_hash, account_scope_hash, generation_scope_hash,
                    root_scope_hash, reservation_token,
                    provider, model, projected_cost_cents, status,
                    created_at_ms, updated_at_ms
                 ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 'held', $9, $9)",
                &[
                    &request_scope_hash,
                    &account_scope_hash,
                    &generation_scope_hash,
                    &root_scope_hash,
                    &reservation_token,
                    &provider,
                    &model,
                    &projected_cost_cents,
                    &now,
                ],
            )?;
            tx.commit()?;
            Ok(CostHoldReservation::Held {
                reservation_token: reservation_token.to_string(),
            })
        }
    })
}

pub fn settle_with_usage(
    pool: &DbPool,
    account_id: &str,
    request_id: &str,
    reservation_token: &str,
    actual_or_conservative_cost_cents: i64,
    usage_provenance: UsageProvenance,
    event: &UsageEvent,
) -> Result<()> {
    if event.request_id != request_id || event.kind.trim().is_empty() {
        anyhow::bail!("Jobs provider usage event does not match its hold")
    }
    #[cfg(test)]
    if FAIL_NEXT_SETTLEMENT_FOR_TEST.with(|flag| flag.replace(false)) {
        anyhow::bail!("injected provider hold settlement failure")
    }
    let reported_cost =
        actual_or_conservative_cost_cents.clamp(0, MAX_AUTHORITATIVE_EVENT_COST_CENTS);
    let mut event = event.clone();
    event.input_tokens = event.input_tokens.clamp(0, MAX_AUTHORITATIVE_EVENT_TOKENS);
    event.output_tokens = event.output_tokens.clamp(0, MAX_AUTHORITATIVE_EVENT_TOKENS);
    event.latency_ms = event
        .latency_ms
        .clamp(0, MAX_AUTHORITATIVE_EVENT_LATENCY_MS);
    event.cost_cents_to_customer = event
        .cost_cents_to_customer
        .clamp(0, MAX_AUTHORITATIVE_EVENT_COST_CENTS);
    let account_scope_hash = account_scope_hash(account_id);
    let request_scope_hash = request_scope_hash(account_id, request_id);
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let now = sqlite_transaction_now_ms(&tx)?;
            let account_exists: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM accounts WHERE id = ?1)",
                params![account_id],
                |row| row.get(0),
            )?;
            let existing: Option<(String, i64, i64, String, String, String)> = tx
                .query_row(
                    "SELECT status, settled_cost_cents, projected_cost_cents,
                            provider, model, usage_provenance
                       FROM jobs_provider_cost_holds
                      WHERE request_scope_hash = ?1 AND account_scope_hash = ?2
                        AND reservation_token = ?3",
                    params![request_scope_hash, account_scope_hash, reservation_token],
                    |row| {
                        Ok((
                            row.get(0)?,
                            row.get(1)?,
                            row.get(2)?,
                            row.get(3)?,
                            row.get(4)?,
                            row.get(5)?,
                        ))
                    },
                )
                .optional()?;
            if existing
                .as_ref()
                .is_some_and(|(_, _, _, provider, model, _)| {
                    event.provider.as_deref() != Some(provider.as_str())
                        || event.model.as_deref() != Some(model.as_str())
                })
            {
                anyhow::bail!("Jobs provider usage event route does not match its hold")
            }
            let projected_cost = existing
                .as_ref()
                .map(|(_, _, projected, _, _, _)| *projected)
                .ok_or_else(|| anyhow::anyhow!("Jobs provider cost hold is missing"))?;
            let settled_cost = if usage_provenance.is_exact() {
                reported_cost
            } else {
                reported_cost.max(projected_cost)
            };
            event.cost_cents_to_bluey = settled_cost;
            if existing
                .as_ref()
                .is_some_and(|(status, cost, _, _, _, provenance)| {
                    status == "settled"
                        && *cost == settled_cost
                        && provenance == usage_provenance.as_str()
                })
            {
                if !account_exists {
                    tx.commit()?;
                    return Ok(());
                }
                let recorded: bool = tx.query_row(
                    "SELECT EXISTS(SELECT 1 FROM usage_events
                      WHERE account_id = ?1 AND request_id = ?2 AND kind = ?3
                        AND origin = 'server'
                        AND task_type IS ?4 AND lane IS ?5
                        AND provider IS ?6 AND model IS ?7
                        AND input_tokens = ?8 AND output_tokens = ?9
                        AND latency_ms = ?10 AND cost_cents_to_bluey = ?11
                        AND cost_cents_to_customer = ?12
                        AND was_speculative = ?13 AND was_fallback = ?14)",
                    params![
                        account_id,
                        request_id,
                        event.kind,
                        event.task_type,
                        event.lane,
                        event.provider,
                        event.model,
                        event.input_tokens,
                        event.output_tokens,
                        event.latency_ms,
                        settled_cost,
                        event.cost_cents_to_customer,
                        i64::from(event.was_speculative),
                        i64::from(event.was_fallback),
                    ],
                    |row| row.get(0),
                )?;
                if !recorded {
                    anyhow::bail!("settled Jobs provider hold is missing its usage event")
                }
                tx.commit()?;
                return Ok(());
            }
            let updated = tx.execute(
                "UPDATE jobs_provider_cost_holds
                    SET status = 'settled', settled_cost_cents = ?3,
                        usage_provenance = ?4, updated_at_ms = ?5
                  WHERE request_scope_hash = ?1 AND reservation_token = ?2 AND status = 'held'
                    AND account_scope_hash = ?6",
                params![
                    request_scope_hash,
                    reservation_token,
                    settled_cost,
                    usage_provenance.as_str(),
                    now,
                    account_scope_hash
                ],
            )?;
            if updated != 1 {
                anyhow::bail!("Jobs provider cost hold is no longer owned by this attempt")
            }
            if !account_exists {
                // Privacy deletion removes account-owned usage rows, but the
                // opaque, bounded-retention hold remains as global spend truth.
                tx.commit()?;
                return Ok(());
            }
            let inserted = tx.execute(
                "INSERT OR IGNORE INTO usage_events
                    (id, account_id, request_id, origin, kind, task_type, lane, provider, model,
                     input_tokens, output_tokens, latency_ms,
                     cost_cents_to_bluey, cost_cents_to_customer,
                     was_speculative, was_fallback)
                 VALUES (?1, ?2, ?3, 'server', ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
                params![
                    uuid::Uuid::new_v4().to_string(),
                    account_id,
                    request_id,
                    event.kind,
                    event.task_type,
                    event.lane,
                    event.provider,
                    event.model,
                    event.input_tokens,
                    event.output_tokens,
                    event.latency_ms,
                    settled_cost,
                    event.cost_cents_to_customer,
                    i64::from(event.was_speculative),
                    i64::from(event.was_fallback),
                ],
            )?;
            if inserted != 1 {
                anyhow::bail!("Jobs provider usage event identity collision")
            }
            tx.commit()?;
            Ok(())
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            let now = postgres_transaction_now_ms(&mut tx)?;
            tx.query_one(
                "SELECT pg_advisory_xact_lock(hashtextextended('jobs-provider-cost-global', 0))",
                &[],
            )?;
            let account_exists = tx
                .query_opt(
                    "SELECT 1 FROM accounts WHERE id = $1 FOR KEY SHARE",
                    &[&account_id],
                )?
                .is_some();
            let existing = tx.query_opt(
                "SELECT status, settled_cost_cents, projected_cost_cents,
                        provider, model, usage_provenance
                   FROM jobs_provider_cost_holds
                  WHERE request_scope_hash = $1 AND account_scope_hash = $2
                    AND reservation_token = $3
                  FOR UPDATE",
                &[&request_scope_hash, &account_scope_hash, &reservation_token],
            )?;
            if existing.as_ref().is_some_and(|row| {
                event.provider.as_deref() != Some(row.get::<_, String>(3).as_str())
                    || event.model.as_deref() != Some(row.get::<_, String>(4).as_str())
            }) {
                anyhow::bail!("Jobs provider usage event route does not match its hold")
            }
            let projected_cost = existing
                .as_ref()
                .map(|row| row.get::<_, i64>(2))
                .ok_or_else(|| anyhow::anyhow!("Jobs provider cost hold is missing"))?;
            let settled_cost = if usage_provenance.is_exact() {
                reported_cost
            } else {
                reported_cost.max(projected_cost)
            };
            event.cost_cents_to_bluey = settled_cost;
            if existing.as_ref().is_some_and(|row| {
                row.get::<_, String>(0) == "settled"
                    && row.get::<_, i64>(1) == settled_cost
                    && row.get::<_, String>(5) == usage_provenance.as_str()
            }) {
                if !account_exists {
                    tx.commit()?;
                    return Ok(());
                }
                let recorded: bool = tx
                    .query_one(
                        "SELECT EXISTS(SELECT 1 FROM usage_events
                          WHERE account_id = $1 AND request_id = $2 AND kind = $3
                            AND origin = 'server'
                            AND task_type IS NOT DISTINCT FROM $4
                            AND lane IS NOT DISTINCT FROM $5
                            AND provider IS NOT DISTINCT FROM $6
                            AND model IS NOT DISTINCT FROM $7
                            AND input_tokens = $8 AND output_tokens = $9
                            AND latency_ms = $10 AND cost_cents_to_bluey = $11
                            AND cost_cents_to_customer = $12
                            AND was_speculative = $13 AND was_fallback = $14)",
                        &[
                            &account_id,
                            &request_id,
                            &event.kind,
                            &event.task_type,
                            &event.lane,
                            &event.provider,
                            &event.model,
                            &event.input_tokens,
                            &event.output_tokens,
                            &event.latency_ms,
                            &settled_cost,
                            &event.cost_cents_to_customer,
                            &i32::from(event.was_speculative),
                            &i32::from(event.was_fallback),
                        ],
                    )?
                    .get(0);
                if !recorded {
                    anyhow::bail!("settled Jobs provider hold is missing its usage event")
                }
                tx.commit()?;
                return Ok(());
            }
            let updated = tx.execute(
                "UPDATE jobs_provider_cost_holds
                    SET status = 'settled', settled_cost_cents = $3,
                        usage_provenance = $4, updated_at_ms = $5
                  WHERE request_scope_hash = $1 AND reservation_token = $2 AND status = 'held'
                    AND account_scope_hash = $6",
                &[
                    &request_scope_hash,
                    &reservation_token,
                    &settled_cost,
                    &usage_provenance.as_str(),
                    &now,
                    &account_scope_hash,
                ],
            )?;
            if updated != 1 {
                anyhow::bail!("Jobs provider cost hold is no longer owned by this attempt")
            }
            if !account_exists {
                tx.commit()?;
                return Ok(());
            }
            let was_speculative = i32::from(event.was_speculative);
            let was_fallback = i32::from(event.was_fallback);
            let inserted = tx.execute(
                "INSERT INTO usage_events
                    (id, account_id, request_id, origin, kind, task_type, lane, provider, model,
                     input_tokens, output_tokens, latency_ms,
                     cost_cents_to_bluey, cost_cents_to_customer,
                     was_speculative, was_fallback)
                 VALUES ($1, $2, $3, 'server', $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)
                 ON CONFLICT (account_id, request_id, kind) DO NOTHING",
                &[
                    &uuid::Uuid::new_v4().to_string(),
                    &account_id,
                    &request_id,
                    &event.kind,
                    &event.task_type,
                    &event.lane,
                    &event.provider,
                    &event.model,
                    &event.input_tokens,
                    &event.output_tokens,
                    &event.latency_ms,
                    &settled_cost,
                    &event.cost_cents_to_customer,
                    &was_speculative,
                    &was_fallback,
                ],
            )?;
            if inserted != 1 {
                anyhow::bail!("Jobs provider usage event identity collision")
            }
            tx.commit()?;
            Ok(())
        }
    })
}

/// Bounded private input. as_of_ms must be zero: only the database's current
/// consistent snapshot is supported, not a reconstruction of historical rows.
#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub(crate) struct ProviderExposureWindow {
    pub attempt_start_ms: i64,
    pub attempt_end_ms: i64,
    pub receipt_start_ms: i64,
    pub receipt_end_ms: i64,
    pub as_of_ms: i64,
}

#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub(crate) struct ProviderExposureReport {
    pub window: ProviderExposureWindow,
    pub physical_retention_ms: i64,
    pub receipt_closeout_lag_ms: i64,
    pub retention_safety_margin_ms: i64,
    pub retention_window_satisfied: bool,
    pub usage_query_start_ms: i64,
    pub usage_timestamp_precision_ms: i64,
    pub read_byte_limit: usize,
    pub read_budget_basis: &'static str,
    pub usage_boundary_ambiguous_events: usize,
    pub held_unresolved_projection_cents: i64,
    pub settled_exact_usage_priced_cents: i64,
    pub settled_estimated_conservative_cents: i64,
    pub settled_missing_conservative_cents: i64,
    pub provider_exposure_cents: i64,
    pub released_nonexposed_holds: usize,
    pub missing_usage_linkage_holds: usize,
    pub mismatched_usage_linkage_holds: usize,
    pub unmatched_usage_events: usize,
    pub late_usage_events: usize,
    pub accepted_attempt_events: usize,
    pub rejected_attempt_events: usize,
    pub failed_or_unsubmitted_applications: usize,
    pub unvalidated_late_submitted_applications: usize,
    pub unvalidated_submitted_applications_outside_receipt_window: usize,
    pub unverifiable_receipts: std::collections::BTreeMap<&'static str, usize>,
    /// Stored metadata only, not employer delivery or object-byte readback.
    pub stored_receipt_metadata_consistent_application_ids: Vec<String>,
    pub stored_receipt_bindings_revalidated: bool,
    pub usage_provenance_source: &'static str,
    pub original_request_fingerprint_recomputed: bool,
    pub remote_object_bytes: &'static str,
    pub employer_delivery: &'static str,
    pub completeness: &'static str,
    pub per_success_ratio: &'static str,
}

#[cfg(test)]
const MAX_PROVIDER_EXPOSURE_HOLDS: usize = 1_000;
#[cfg(test)]
const MAX_PROVIDER_EXPOSURE_USAGE_EVENTS: usize = 4_000;
#[cfg(test)]
const MAX_PROVIDER_EXPOSURE_RECEIPTS: usize = 1_000;
#[cfg(test)]
const MAX_PROVIDER_EXPOSURE_BYTES: usize = super::jobs::STORED_REPORT_BYTE_LIMIT;
#[cfg(test)]
const PROVIDER_EXPOSURE_SAFETY_MARGIN_MS: i64 = 60_000;

#[cfg(test)]
#[derive(Debug, serde::Serialize)]
struct ExposureHold {
    request_hash: String,
    provider: String,
    model: String,
    projected: i64,
    settled: i64,
    provenance: String,
    status: String,
    created_at_ms: i64,
    updated_at_ms: i64,
}

#[cfg(test)]
#[derive(Debug, serde::Serialize)]
struct ExposureUsage {
    request_id: String,
    provider: Option<String>,
    model: Option<String>,
    cost: i64,
    task_type: Option<String>,
    occurred_at_ms: i64,
}

#[cfg(test)]
#[derive(Debug, serde::Serialize)]
struct ExposureApplication {
    id: String,
    state: String,
    submitted_at_ms: Option<i64>,
}

#[cfg(test)]
struct ExposureSnapshot {
    holds: Vec<ExposureHold>,
    usage: Vec<ExposureUsage>,
    applications: Vec<ExposureApplication>,
    remaining_bytes: usize,
}

#[cfg(test)]
fn current_exposure_window(
    mut window: ProviderExposureWindow,
    cutoff: i64,
) -> Result<ProviderExposureWindow> {
    if window.as_of_ms != 0
        || cutoff <= 0
        || window.attempt_start_ms <= 0
        || window.attempt_start_ms >= window.attempt_end_ms
        || window.attempt_end_ms > cutoff
        || window.receipt_start_ms <= 0
        || window.receipt_start_ms > window.attempt_start_ms
        || window.receipt_end_ms < window.attempt_end_ms
        || window.receipt_start_ms >= window.receipt_end_ms
        || window.receipt_end_ms > cutoff
    {
        anyhow::bail!("provider exposure report requires valid windows and current database cutoff")
    }
    window.as_of_ms = cutoff;
    Ok(window)
}

/// Only read-only SQL and existing frozen receipt validators are used. No
/// observer handle can turn retained historical data into complete history.
#[cfg(test)]
pub(crate) fn read_provider_exposure_report(
    pool: &DbPool,
    account_id: &str,
    window: ProviderExposureWindow,
) -> Result<ProviderExposureReport> {
    use super::jobs::{bounded_report_projection, charge_report_sql_row};
    if account_id.trim().is_empty() {
        anyhow::bail!("provider exposure report requires an explicit account")
    }
    let account_hash = account_scope_hash(account_id);
    let hold_columns = [
        "request_scope_hash",
        "provider",
        "model",
        "projected_cost_cents",
        "settled_cost_cents",
        "usage_provenance",
        "status",
        "created_at_ms",
        "updated_at_ms",
    ];
    let usage_columns = [
        "request_id",
        "provider",
        "model",
        "cost_cents_to_bluey",
        "task_type",
    ];
    let application_columns = ["id", "state", "submitted_at_ms"];
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Deferred)?;
            // Establish the read snapshot before sampling SQLite's wall clock.
            let account_count = tx.query_row(
                "SELECT COUNT(*) FROM accounts WHERE id = ?1",
                params![account_id],
                |row| row.get::<_, i64>(0),
            )?;
            if account_count != 1 {
                anyhow::bail!("provider exposure account is missing")
            }
            let window = current_exposure_window(window, sqlite_transaction_now_ms(&tx)?)?;
            let mut remaining_bytes = MAX_PROVIDER_EXPOSURE_BYTES;
            let projection = bounded_report_projection(false, &hold_columns, &[0, 1, 2, 5, 6]);
            let holds = {
                let mut stmt = tx.prepare(&format!(
                    "SELECT {projection}
                    FROM jobs_provider_cost_holds
                    WHERE account_scope_hash = ?1 AND created_at_ms >= ?2 AND created_at_ms < ?3
                    ORDER BY created_at_ms, request_scope_hash LIMIT ?4"
                ))?;
                let mut rows = stmt.query(params![
                    account_hash,
                    window.attempt_start_ms,
                    window.attempt_end_ms,
                    (MAX_PROVIDER_EXPOSURE_HOLDS + 1) as i64
                ])?;
                let mut values = Vec::new();
                while let Some(row) = rows.next()? {
                    if values.len() == MAX_PROVIDER_EXPOSURE_HOLDS {
                        anyhow::bail!("provider exposure report row cap exceeded")
                    }
                    charge_report_sql_row(&mut remaining_bytes, row.get(9)?)?;
                    values.push(ExposureHold {
                        request_hash: row.get(0)?,
                        provider: row.get(1)?,
                        model: row.get(2)?,
                        projected: row.get(3)?,
                        settled: row.get(4)?,
                        provenance: row.get(5)?,
                        status: row.get(6)?,
                        created_at_ms: row.get(7)?,
                        updated_at_ms: row.get(8)?,
                    });
                }
                values
            };
            let projection = bounded_report_projection(false, &usage_columns, &[0, 1, 2, 4]);
            let usage = {
                let mut stmt = tx.prepare(&format!(
                    "SELECT {projection},
                        CAST(strftime('%s', ts) AS INTEGER) * 1000 FROM usage_events
                    WHERE account_id = ?1 AND origin = 'server'
                      AND kind = 'jobs_resume_generation_attempt'
                      AND ts >= datetime(?2 / 1000, 'unixepoch')
                      AND ts <= datetime(?3 / 1000, 'unixepoch')
                    ORDER BY ts, id LIMIT ?4"
                ))?;
                let mut rows = stmt.query(params![
                    account_id,
                    window.attempt_start_ms,
                    window.as_of_ms,
                    (MAX_PROVIDER_EXPOSURE_USAGE_EVENTS + 1) as i64
                ])?;
                let mut values = Vec::new();
                while let Some(row) = rows.next()? {
                    if values.len() == MAX_PROVIDER_EXPOSURE_USAGE_EVENTS {
                        anyhow::bail!("provider exposure report row cap exceeded")
                    }
                    charge_report_sql_row(&mut remaining_bytes, row.get(5)?)?;
                    values.push(ExposureUsage {
                        request_id: row.get(0)?,
                        provider: row.get(1)?,
                        model: row.get(2)?,
                        cost: row.get(3)?,
                        task_type: row.get(4)?,
                        occurred_at_ms: row.get(6)?,
                    });
                }
                values
            };
            let projection = bounded_report_projection(false, &application_columns, &[0, 1]);
            let applications = {
                let mut stmt = tx.prepare(&format!(
                    "SELECT {projection} FROM jobs_applications
                    WHERE account_id = ?1 AND
                      ((submitted_at_ms >= ?2 AND submitted_at_ms <= ?3)
                        OR (created_at_ms >= ?4 AND created_at_ms < ?5))
                    ORDER BY id LIMIT ?6"
                ))?;
                let mut rows = stmt.query(params![
                    account_id,
                    window.receipt_start_ms,
                    window.as_of_ms,
                    window.attempt_start_ms,
                    window.attempt_end_ms,
                    (MAX_PROVIDER_EXPOSURE_RECEIPTS + 1) as i64
                ])?;
                let mut values = Vec::new();
                while let Some(row) = rows.next()? {
                    if values.len() == MAX_PROVIDER_EXPOSURE_RECEIPTS {
                        anyhow::bail!("provider exposure report row cap exceeded")
                    }
                    charge_report_sql_row(&mut remaining_bytes, row.get(3)?)?;
                    values.push(ExposureApplication {
                        id: row.get(0)?,
                        state: row.get(1)?,
                        submitted_at_ms: row.get(2)?,
                    });
                }
                values
            };
            let report = summarize_exposure_snapshot(
                account_id,
                window,
                1_000,
                ExposureSnapshot {
                    holds,
                    usage,
                    applications,
                    remaining_bytes,
                },
                |id, remaining| {
                    super::jobs::validate_stored_submitted_receipt_sqlite_tx(
                        &tx,
                        account_id,
                        id,
                        window.receipt_start_ms,
                        window.receipt_end_ms,
                        window.as_of_ms,
                        remaining,
                    )
                },
            )?;
            tx.rollback()?;
            Ok(report)
        }
        DbPool::Postgres(_) => {
            use postgres::fallible_iterator::FallibleIterator;
            let mut conn = pool.get_pg()?;
            let mut tx = conn
                .build_transaction()
                .isolation_level(postgres::IsolationLevel::RepeatableRead)
                .read_only(true)
                .start()?;
            if tx
                .query_opt("SELECT 1 FROM accounts WHERE id = $1", &[&account_id])?
                .is_none()
            {
                anyhow::bail!("provider exposure account is missing")
            }
            // Sample current DB time after acquiring the repeatable snapshot.
            let cutoff: i64 = tx
                .query_one(
                    "SELECT FLOOR(EXTRACT(EPOCH FROM clock_timestamp()) * 1000)::bigint",
                    &[],
                )?
                .try_get(0)?;
            let window = current_exposure_window(window, cutoff)?;
            let mut remaining_bytes = MAX_PROVIDER_EXPOSURE_BYTES;
            let projection = bounded_report_projection(true, &hold_columns, &[0, 1, 2, 5, 6]);
            let holds = {
                let sql = format!(
                    "SELECT {projection} FROM jobs_provider_cost_holds
                    WHERE account_scope_hash = $1 AND created_at_ms >= $2 AND created_at_ms < $3
                    ORDER BY created_at_ms, request_scope_hash LIMIT $4"
                );
                let limit = (MAX_PROVIDER_EXPOSURE_HOLDS + 1) as i64;
                let params: &[&(dyn postgres::types::ToSql + Sync)] = &[
                    &account_hash,
                    &window.attempt_start_ms,
                    &window.attempt_end_ms,
                    &limit,
                ];
                let mut rows = tx.query_raw(&sql, params.iter().copied())?;
                let mut values = Vec::new();
                while let Some(row) = rows.next()? {
                    if values.len() == MAX_PROVIDER_EXPOSURE_HOLDS {
                        anyhow::bail!("provider exposure report row cap exceeded")
                    }
                    charge_report_sql_row(&mut remaining_bytes, row.try_get(9)?)?;
                    values.push(ExposureHold {
                        request_hash: row.try_get(0)?,
                        provider: row.try_get(1)?,
                        model: row.try_get(2)?,
                        projected: row.try_get(3)?,
                        settled: row.try_get(4)?,
                        provenance: row.try_get(5)?,
                        status: row.try_get(6)?,
                        created_at_ms: row.try_get(7)?,
                        updated_at_ms: row.try_get(8)?,
                    });
                }
                values
            };
            let projection = bounded_report_projection(true, &usage_columns, &[0, 1, 2, 4]);
            let usage = {
                let sql = format!("SELECT {projection},
                        FLOOR(EXTRACT(EPOCH FROM ts) * 1000)::bigint FROM usage_events
                    WHERE account_id = $1 AND origin = 'server'
                      AND kind = 'jobs_resume_generation_attempt'
                      AND ts >= to_timestamp((($2::bigint / 1000) * 1000)::double precision / 1000.0)
                      AND ts <= to_timestamp(($3::bigint)::double precision / 1000.0)
                    ORDER BY ts, id LIMIT $4");
                let limit = (MAX_PROVIDER_EXPOSURE_USAGE_EVENTS + 1) as i64;
                let params: &[&(dyn postgres::types::ToSql + Sync)] = &[
                    &account_id,
                    &window.attempt_start_ms,
                    &window.as_of_ms,
                    &limit,
                ];
                let mut rows = tx.query_raw(&sql, params.iter().copied())?;
                let mut values = Vec::new();
                while let Some(row) = rows.next()? {
                    if values.len() == MAX_PROVIDER_EXPOSURE_USAGE_EVENTS {
                        anyhow::bail!("provider exposure report row cap exceeded")
                    }
                    charge_report_sql_row(&mut remaining_bytes, row.try_get(5)?)?;
                    values.push(ExposureUsage {
                        request_id: row.try_get(0)?,
                        provider: row.try_get(1)?,
                        model: row.try_get(2)?,
                        cost: row.try_get(3)?,
                        task_type: row.try_get(4)?,
                        occurred_at_ms: row.try_get(6)?,
                    });
                }
                values
            };
            let projection = bounded_report_projection(true, &application_columns, &[0, 1]);
            let applications = {
                let sql = format!(
                    "SELECT {projection} FROM jobs_applications
                    WHERE account_id = $1 AND
                      ((submitted_at_ms >= $2 AND submitted_at_ms <= $3)
                        OR (created_at_ms >= $4 AND created_at_ms < $5))
                    ORDER BY id LIMIT $6"
                );
                let limit = (MAX_PROVIDER_EXPOSURE_RECEIPTS + 1) as i64;
                let params: &[&(dyn postgres::types::ToSql + Sync)] = &[
                    &account_id,
                    &window.receipt_start_ms,
                    &window.as_of_ms,
                    &window.attempt_start_ms,
                    &window.attempt_end_ms,
                    &limit,
                ];
                let mut rows = tx.query_raw(&sql, params.iter().copied())?;
                let mut values = Vec::new();
                while let Some(row) = rows.next()? {
                    if values.len() == MAX_PROVIDER_EXPOSURE_RECEIPTS {
                        anyhow::bail!("provider exposure report row cap exceeded")
                    }
                    charge_report_sql_row(&mut remaining_bytes, row.try_get(3)?)?;
                    values.push(ExposureApplication {
                        id: row.try_get(0)?,
                        state: row.try_get(1)?,
                        submitted_at_ms: row.try_get(2)?,
                    });
                }
                values
            };
            let report = summarize_exposure_snapshot(
                account_id,
                window,
                1,
                ExposureSnapshot {
                    holds,
                    usage,
                    applications,
                    remaining_bytes,
                },
                |id, remaining| {
                    super::jobs::validate_stored_submitted_receipt_postgres_tx(
                        &mut tx,
                        account_id,
                        id,
                        window.receipt_start_ms,
                        window.receipt_end_ms,
                        window.as_of_ms,
                        remaining,
                    )
                },
            )?;
            tx.rollback()?;
            Ok(report)
        }
    })
}

#[cfg(test)]
fn summarize_exposure_snapshot(
    account_id: &str,
    window: ProviderExposureWindow,
    usage_precision_ms: i64,
    snapshot: ExposureSnapshot,
    mut validate_receipt: impl FnMut(&str, &mut usize) -> Result<super::jobs::StoredReceiptValidation>,
) -> Result<ProviderExposureReport> {
    let ExposureSnapshot {
        holds,
        usage,
        applications,
        remaining_bytes: mut remaining,
    } = snapshot;
    if holds.len() > MAX_PROVIDER_EXPOSURE_HOLDS
        || usage.len() > MAX_PROVIDER_EXPOSURE_USAGE_EVENTS
        || applications.len() > MAX_PROVIDER_EXPOSURE_RECEIPTS
    {
        anyhow::bail!("provider exposure report row cap exceeded")
    }
    // SQL bounded each row before transport; its conservative byte charge was
    // consumed before any owned string/vector/map entry was allocated.
    let span = window
        .receipt_end_ms
        .checked_sub(window.receipt_start_ms)
        .and_then(|value| value.checked_add(PROVIDER_EXPOSURE_SAFETY_MARGIN_MS))
        .ok_or_else(|| anyhow::anyhow!("provider exposure window overflow"))?;
    let age = window
        .as_of_ms
        .checked_sub(window.receipt_start_ms)
        .and_then(|value| value.checked_add(PROVIDER_EXPOSURE_SAFETY_MARGIN_MS))
        .ok_or_else(|| anyhow::anyhow!("provider exposure window overflow"))?;
    let mut report = ProviderExposureReport {
        window,
        physical_retention_ms: UPSTREAM_SPEND_TRUTH_RETENTION_MS,
        receipt_closeout_lag_ms: window.receipt_end_ms - window.attempt_end_ms,
        retention_safety_margin_ms: PROVIDER_EXPOSURE_SAFETY_MARGIN_MS,
        retention_window_satisfied: span < UPSTREAM_SPEND_TRUTH_RETENTION_MS
            && age < UPSTREAM_SPEND_TRUTH_RETENTION_MS,
        usage_query_start_ms: (window.attempt_start_ms / 1000) * 1000,
        usage_timestamp_precision_ms: usage_precision_ms,
        read_byte_limit: MAX_PROVIDER_EXPOSURE_BYTES,
        read_budget_basis: "sql_utf8_json_upper_bound_6x_plus_1024_per_row_and_streamed_output",
        usage_boundary_ambiguous_events: 0,
        held_unresolved_projection_cents: 0,
        settled_exact_usage_priced_cents: 0,
        settled_estimated_conservative_cents: 0,
        settled_missing_conservative_cents: 0,
        provider_exposure_cents: 0,
        released_nonexposed_holds: 0,
        missing_usage_linkage_holds: 0,
        mismatched_usage_linkage_holds: 0,
        unmatched_usage_events: 0,
        late_usage_events: 0,
        accepted_attempt_events: 0,
        rejected_attempt_events: 0,
        failed_or_unsubmitted_applications: 0,
        unvalidated_late_submitted_applications: 0,
        unvalidated_submitted_applications_outside_receipt_window: 0,
        unverifiable_receipts: std::collections::BTreeMap::new(),
        stored_receipt_metadata_consistent_application_ids: Vec::new(),
        stored_receipt_bindings_revalidated: true,
        usage_provenance_source: "hold_row",
        original_request_fingerprint_recomputed: false,
        remote_object_bytes: "not_checked",
        employer_delivery: "not_checked",
        completeness: "unverifiable",
        per_success_ratio: "unavailable",
    };
    let mut usage_by_request = std::collections::BTreeMap::<String, ExposureUsage>::new();
    for event in usage {
        if event.request_id.trim().is_empty()
            || event.cost < 0
            || event.cost > MAX_AUTHORITATIVE_EVENT_COST_CENTS
        {
            anyhow::bail!("provider exposure report found invalid usage identity/amount")
        }
        if event.occurred_at_ms >= window.attempt_end_ms {
            report.late_usage_events += 1;
        }
        if event.occurred_at_ms < window.attempt_end_ms
            && event.occurred_at_ms.saturating_add(usage_precision_ms) > window.attempt_end_ms
        {
            report.usage_boundary_ambiguous_events += 1;
        }
        let hash = request_scope_hash(account_id, &event.request_id);
        if usage_by_request.insert(hash, event).is_some() {
            anyhow::bail!("provider exposure report found duplicate usage linkage")
        }
    }
    let mut seen_holds = std::collections::BTreeSet::new();
    let mut totals = [0_i128; 4];
    for hold in holds {
        if !seen_holds.insert(hold.request_hash.clone())
            || hold.request_hash.len() != 64
            || !hold
                .request_hash
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            || hold.provider.trim().is_empty()
            || hold.model.trim().is_empty()
            || hold.projected <= 0
            || hold.projected > MAX_AUTHORITATIVE_EVENT_COST_CENTS
            || hold.settled < 0
            || hold.settled > MAX_AUTHORITATIVE_EVENT_COST_CENTS
            || !matches!(hold.provenance.as_str(), "exact" | "estimated" | "missing")
            || !matches!(hold.status.as_str(), "held" | "settled" | "released")
            || hold.updated_at_ms < hold.created_at_ms
            || hold.updated_at_ms > window.as_of_ms
        {
            anyhow::bail!("provider exposure report found invalid hold content")
        }
        let event = usage_by_request.remove(&hold.request_hash);
        if hold.status == "released" {
            // A release is provably pre-dispatch only when no attempt event
            // exists. Never discard a released row with contradictory usage.
            if hold.settled != 0 || hold.provenance != "missing" || event.is_some() {
                anyhow::bail!("provider exposure report found exposed or invalid released hold")
            }
            report.released_nonexposed_holds += 1;
            continue;
        }
        if hold.status == "held" && (hold.settled != 0 || hold.provenance != "missing") {
            anyhow::bail!("provider exposure report found invalid unresolved hold")
        }
        let coherent = match event {
            None => {
                report.missing_usage_linkage_holds += 1;
                false
            }
            Some(event) => {
                let outcome = event.task_type.as_deref().unwrap_or("");
                let accepted = outcome == "jobs_resume_tailoring_accepted";
                let rejected = matches!(
                    outcome,
                    "jobs_resume_tailoring_rejected_truth"
                        | "jobs_resume_tailoring_rejected_cost"
                        | "jobs_resume_tailoring_rejected_deadline"
                        | "jobs_resume_tailoring_rejected_provider_boundary"
                        | "jobs_resume_tailoring_rejected_provider_error"
                        | "jobs_resume_tailoring_rejected_timeout"
                        | "jobs_resume_tailoring_rejected_cancelled"
                );
                report.accepted_attempt_events += usize::from(accepted);
                report.rejected_attempt_events += usize::from(rejected);
                // Usage stores the already-settled amount; raw nonexact
                // reported cost and independent event provenance are absent.
                let valid = hold.status == "settled"
                    && (accepted || rejected)
                    && event.provider.as_deref() == Some(hold.provider.as_str())
                    && event.model.as_deref() == Some(hold.model.as_str())
                    && event.occurred_at_ms.saturating_add(usage_precision_ms - 1)
                        >= hold.created_at_ms
                    && event.cost == hold.settled
                    && (hold.provenance == "exact" || hold.settled >= hold.projected);
                if !valid {
                    report.mismatched_usage_linkage_holds += 1;
                }
                valid
            }
        };
        let (bucket, amount) = if hold.status == "held" {
            (0, hold.projected)
        } else if !coherent {
            (3, hold.settled)
        } else {
            (
                match hold.provenance.as_str() {
                    "exact" => 1,
                    "estimated" => 2,
                    _ => 3,
                },
                hold.settled,
            )
        };
        totals[bucket] = totals[bucket]
            .checked_add(i128::from(amount))
            .ok_or_else(|| anyhow::anyhow!("provider exposure amount overflow"))?;
    }
    report.unmatched_usage_events = usage_by_request.len();
    report.held_unresolved_projection_cents = i64::try_from(totals[0])?;
    report.settled_exact_usage_priced_cents = i64::try_from(totals[1])?;
    report.settled_estimated_conservative_cents = i64::try_from(totals[2])?;
    report.settled_missing_conservative_cents = i64::try_from(totals[3])?;
    report.provider_exposure_cents =
        i64::try_from(totals.into_iter().try_fold(0_i128, |total, value| {
            total
                .checked_add(value)
                .ok_or_else(|| anyhow::anyhow!("provider exposure amount overflow"))
        })?)?;
    let mut receipt_ids = std::collections::BTreeSet::new();
    for application in applications {
        if application.state != "submitted" {
            report.failed_or_unsubmitted_applications += 1;
            continue;
        }
        let Some(submitted) = application.submitted_at_ms.filter(|value| *value > 0) else {
            *report
                .unverifiable_receipts
                .entry("missing_submission_time")
                .or_default() += 1;
            report.stored_receipt_bindings_revalidated = false;
            continue;
        };
        if submitted < window.receipt_start_ms || submitted >= window.receipt_end_ms {
            report.unvalidated_submitted_applications_outside_receipt_window += 1;
            report.unvalidated_late_submitted_applications +=
                usize::from(submitted >= window.receipt_end_ms);
            continue;
        }
        match validate_receipt(&application.id, &mut remaining)? {
            super::jobs::StoredReceiptValidation::StoredMetadataValidated => {
                receipt_ids.insert(application.id);
            }
            super::jobs::StoredReceiptValidation::Unverifiable(reason) => {
                *report.unverifiable_receipts.entry(reason).or_default() += 1;
                report.stored_receipt_bindings_revalidated = false;
            }
        }
    }
    report.stored_receipt_metadata_consistent_application_ids = receipt_ids.into_iter().collect();
    super::jobs::charge_report_serialization(&mut remaining, &report)?;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn append_generation_hold(
        pool: &DbPool,
        account_id: &str,
        event_id: &str,
        transition: super::super::jobs::OperationalHoldTransition,
        revision: i64,
        predecessor: Option<&str>,
    ) {
        super::super::jobs::append_operational_hold_event(
            pool,
            &super::super::jobs::AppendOperationalHoldEventRequest {
                event_id: event_id.to_string(),
                capability: super::super::jobs::OperationalCapability::Generation,
                scope_kind: super::super::jobs::OperationalHoldScopeKind::Account,
                scope_id: account_id.to_string(),
                transition,
                reason_code: if transition == super::super::jobs::OperationalHoldTransition::Held {
                    super::super::jobs::OperationalHoldReasonCode::Incident
                } else {
                    super::super::jobs::OperationalHoldReasonCode::ManualRelease
                },
                reason_ref: Some("INC-606".to_string()),
                expected_head_revision: revision,
                expected_current_event_id: predecessor.map(str::to_string),
            },
            "admin-606",
        )
        .unwrap();
    }

    fn temp_pool() -> DbPool {
        let path = std::env::temp_dir().join(format!(
            "bluey-provider-holds-{}.sqlite3",
            uuid::Uuid::new_v4()
        ));
        let pool = crate::db::open_pool(&path).unwrap();
        crate::db::run_migrations(&pool).unwrap();
        pool
    }

    fn account(pool: &DbPool) -> String {
        crate::db::accounts::Account::create(pool, "holds@bluey.test", "hash")
            .unwrap()
            .id
    }

    fn exposure_reserve(pool: &DbPool, account_id: &str, request_id: &str, projected: i64) {
        assert!(matches!(
            reserve(
                pool,
                account_id,
                &format!("router:{request_id}:llm"),
                &format!("token:{request_id}"),
                request_id,
                "openai",
                "gpt-5.4-mini",
                projected,
                100_000,
                UpstreamSpendGuard {
                    limit_cents: 100_000,
                    window_hours: 24
                }
            )
            .unwrap(),
            CostHoldReservation::Held { .. }
        ));
    }

    fn exposure_event(request_id: &str, outcome: &str, cost: i64) -> UsageEvent {
        UsageEvent {
            request_id: request_id.into(),
            kind: "jobs_resume_generation_attempt".into(),
            task_type: Some(outcome.into()),
            lane: Some("deep".into()),
            provider: Some("openai".into()),
            model: Some("gpt-5.4-mini".into()),
            input_tokens: 20,
            output_tokens: 10,
            latency_ms: 5,
            cost_cents_to_bluey: cost,
            cost_cents_to_customer: 0,
            was_speculative: false,
            was_fallback: false,
        }
    }

    fn exposure_settle(
        pool: &DbPool,
        account_id: &str,
        id: &str,
        projected: i64,
        reported: i64,
        provenance: UsageProvenance,
        outcome: &str,
    ) {
        exposure_reserve(pool, account_id, id, projected);
        settle_with_usage(
            pool,
            account_id,
            id,
            &format!("token:{id}"),
            reported,
            provenance,
            &exposure_event(id, outcome, reported),
        )
        .unwrap();
    }

    fn exposure_window(pool: &DbPool) -> ProviderExposureWindow {
        std::thread::sleep(std::time::Duration::from_millis(5));
        let mut conn = pool.get().unwrap();
        let tx = conn.transaction().unwrap();
        let end = sqlite_transaction_now_ms(&tx).unwrap();
        tx.rollback().unwrap();
        ProviderExposureWindow {
            attempt_start_ms: end - 60_000,
            attempt_end_ms: end,
            receipt_start_ms: end - 60_000,
            receipt_end_ms: end,
            as_of_ms: 0,
        }
    }

    fn exposure_financial_state(pool: &DbPool) -> Vec<Vec<String>> {
        let conn = pool.get().unwrap();
        ["jobs_provider_cost_holds", "usage_events"]
            .into_iter()
            .map(|table| {
                let mut stmt = conn
                    .prepare(&format!("SELECT * FROM {table} ORDER BY 1"))
                    .unwrap();
                let columns = stmt.column_count();
                let rows = stmt
                    .query_map([], |row| {
                        (0..columns)
                            .map(|index| row.get::<_, rusqlite::types::Value>(index))
                            .collect::<rusqlite::Result<Vec<_>>>()
                            .map(|values| format!("{values:?}"))
                    })
                    .unwrap()
                    .collect::<rusqlite::Result<Vec<_>>>()
                    .unwrap();
                rows
            })
            .collect()
    }

    #[test]
    fn sqlite_exposure_report_counts_all_attempt_outcomes_once_and_is_read_only() {
        let pool = temp_pool();
        let account_id = account(&pool);
        exposure_settle(
            &pool,
            &account_id,
            "accepted",
            12,
            3,
            UsageProvenance::Exact,
            "jobs_resume_tailoring_accepted",
        );
        exposure_settle(
            &pool,
            &account_id,
            "rejected",
            8,
            2,
            UsageProvenance::Estimated,
            "jobs_resume_tailoring_rejected_truth",
        );
        exposure_settle(
            &pool,
            &account_id,
            "unknown-delivery",
            10,
            0,
            UsageProvenance::Missing,
            "jobs_resume_tailoring_rejected_timeout",
        );
        exposure_reserve(&pool, &account_id, "failed-settlement", 7);
        fail_next_settlement_for_test();
        assert!(settle_with_usage(
            &pool,
            &account_id,
            "failed-settlement",
            "token:failed-settlement",
            2,
            UsageProvenance::Exact,
            &exposure_event("failed-settlement", "jobs_resume_tailoring_accepted", 2)
        )
        .is_err());
        let other =
            crate::db::accounts::Account::create(&pool, "exposure-tenant@example.test", "hash")
                .unwrap();
        exposure_settle(
            &pool,
            &other.id,
            "foreign",
            50,
            40,
            UsageProvenance::Exact,
            "jobs_resume_tailoring_accepted",
        );
        let window = exposure_window(&pool);
        let before = exposure_financial_state(&pool);
        for _ in 0..2 {
            let report = read_provider_exposure_report(&pool, &account_id, window).unwrap();
            assert_eq!(report.provider_exposure_cents, 28);
            assert_eq!(report.held_unresolved_projection_cents, 7);
            assert_eq!(report.settled_exact_usage_priced_cents, 3);
            assert_eq!(report.settled_estimated_conservative_cents, 8);
            assert_eq!(report.settled_missing_conservative_cents, 10);
            assert_eq!(report.accepted_attempt_events, 1);
            assert_eq!(report.rejected_attempt_events, 2);
            assert_eq!(report.missing_usage_linkage_holds, 1);
            assert_eq!(report.usage_provenance_source, "hold_row");
            assert!(report
                .stored_receipt_metadata_consistent_application_ids
                .is_empty());
            assert_eq!(report.per_success_ratio, "unavailable");
            assert!(report.window.as_of_ms >= window.attempt_end_ms);
            assert_eq!(
                report.physical_retention_ms,
                UPSTREAM_SPEND_TRUTH_RETENTION_MS
            );
        }
        assert_eq!(exposure_financial_state(&pool), before);
    }

    #[test]
    fn sqlite_exposure_report_missing_mismatched_and_released_holds_keep_explicit_gaps() {
        let pool = temp_pool();
        let account_id = account(&pool);
        exposure_settle(
            &pool,
            &account_id,
            "missing",
            5,
            2,
            UsageProvenance::Exact,
            "jobs_resume_tailoring_accepted",
        );
        exposure_settle(
            &pool,
            &account_id,
            "mismatch",
            5,
            3,
            UsageProvenance::Exact,
            "jobs_resume_tailoring_rejected_cost",
        );
        exposure_reserve(&pool, &account_id, "predispatch-release", 5);
        let conn = pool.get().unwrap();
        conn.execute("DELETE FROM usage_events WHERE request_id = 'missing'", [])
            .unwrap();
        conn.execute(
            "UPDATE usage_events SET model = 'different-model'
            WHERE request_id = 'mismatch'",
            [],
        )
        .unwrap();
        // There is no production release API for provider holds. Exercise the
        // schema's legacy released state without inventing a dispatch event.
        conn.execute(
            "UPDATE jobs_provider_cost_holds SET status = 'released'
            WHERE request_scope_hash = ?1",
            params![request_scope_hash(&account_id, "predispatch-release")],
        )
        .unwrap();
        drop(conn);
        let window = exposure_window(&pool);
        let report = read_provider_exposure_report(&pool, &account_id, window).unwrap();
        assert_eq!(report.provider_exposure_cents, 5);
        assert_eq!(report.settled_exact_usage_priced_cents, 0);
        assert_eq!(report.settled_missing_conservative_cents, 5);
        assert_eq!(report.missing_usage_linkage_holds, 1);
        assert_eq!(report.mismatched_usage_linkage_holds, 1);
        assert_eq!(report.released_nonexposed_holds, 1);
        super::super::usage::record(
            &pool,
            &account_id,
            &exposure_event("predispatch-release", "jobs_resume_tailoring_accepted", 0),
        )
        .unwrap();
        assert!(read_provider_exposure_report(&pool, &account_id, window).is_err());
    }

    #[test]
    fn sqlite_exposure_report_duplicate_usage_is_a_hard_failure_without_fanout() {
        let pool = temp_pool();
        let account_id = account(&pool);
        exposure_settle(
            &pool,
            &account_id,
            "duplicated",
            8,
            3,
            UsageProvenance::Exact,
            "jobs_resume_tailoring_accepted",
        );
        let conn = pool.get().unwrap();
        // Simulate a legacy/imported violation, after normal atomic settlement.
        conn.execute("DROP INDEX idx_usage_events_dedupe", [])
            .unwrap();
        conn.execute("INSERT INTO usage_events
            (id, account_id, request_id, origin, kind, task_type, provider, model, cost_cents_to_bluey)
            SELECT 'duplicate-row', account_id, request_id, origin, kind, task_type, provider, model,
                cost_cents_to_bluey FROM usage_events WHERE request_id = 'duplicated'", []).unwrap();
        drop(conn);
        let window = exposure_window(&pool);
        let before = exposure_financial_state(&pool);
        let error = read_provider_exposure_report(&pool, &account_id, window).unwrap_err();
        assert!(error.to_string().contains("duplicate usage linkage"));
        assert_eq!(exposure_financial_state(&pool), before);
    }

    #[test]
    fn sqlite_exposure_report_created_membership_keeps_late_settlement_and_unmatched_usage() {
        let pool = temp_pool();
        let account_id = account(&pool);
        exposure_settle(
            &pool,
            &account_id,
            "late-settlement",
            9,
            2,
            UsageProvenance::Exact,
            "jobs_resume_tailoring_accepted",
        );
        super::super::usage::record(
            &pool,
            &account_id,
            &exposure_event(
                "unmatched-in-range",
                "jobs_resume_tailoring_rejected_timeout",
                6,
            ),
        )
        .unwrap();
        let mut window = exposure_window(&pool);
        window.attempt_end_ms -= 10_000;
        pool.get()
            .unwrap()
            .execute(
                "UPDATE jobs_provider_cost_holds SET created_at_ms = ?1",
                params![window.attempt_end_ms - 1],
            )
            .unwrap();
        let report = read_provider_exposure_report(&pool, &account_id, window).unwrap();
        assert_eq!(report.provider_exposure_cents, 2);
        assert_eq!(report.settled_exact_usage_priced_cents, 2);
        assert_eq!(report.late_usage_events, 2);
        assert_eq!(report.unmatched_usage_events, 1);
        assert_eq!(report.receipt_closeout_lag_ms, 10_000);
        let mut historical = window;
        historical.as_of_ms = window.attempt_end_ms;
        assert!(read_provider_exposure_report(&pool, &account_id, historical).is_err());
    }

    #[test]
    fn sqlite_exposure_report_old_usage_does_not_exhaust_the_current_window_cap() {
        let pool = temp_pool();
        let account_id = account(&pool);
        exposure_reserve(&pool, &account_id, "current-hold", 4);
        pool.get()
            .unwrap()
            .execute(
                "WITH RECURSIVE n(x) AS
            (VALUES(0) UNION ALL SELECT x + 1 FROM n WHERE x < 4000)
            INSERT INTO usage_events(id, account_id, request_id, origin, kind, ts)
            SELECT 'old-' || x, ?1, 'old-' || x, 'server', 'jobs_resume_generation_attempt',
                datetime('now', '-32 days') FROM n",
                params![account_id],
            )
            .unwrap();
        let window = exposure_window(&pool);
        let report = read_provider_exposure_report(&pool, &account_id, window).unwrap();
        assert_eq!(report.provider_exposure_cents, 4);
        assert_eq!(report.unmatched_usage_events, 0);
        let mut expired = window;
        expired.attempt_start_ms -= UPSTREAM_SPEND_TRUTH_RETENTION_MS + 2 * 86_400_000;
        expired.receipt_start_ms = expired.attempt_start_ms;
        let error = read_provider_exposure_report(&pool, &account_id, expired).unwrap_err();
        assert!(error.to_string().contains("row cap"));
    }

    #[test]
    fn sqlite_exposure_report_retention_unknown_and_byte_row_bounds_fail_closed() {
        let pool = temp_pool();
        let account_id = account(&pool);
        let mut window = exposure_window(&pool);
        window.attempt_start_ms -= UPSTREAM_SPEND_TRUTH_RETENTION_MS;
        window.receipt_start_ms = window.attempt_start_ms;
        let report = read_provider_exposure_report(&pool, &account_id, window).unwrap();
        assert!(!report.retention_window_satisfied);
        assert_eq!(report.completeness, "unverifiable");
        exposure_reserve(&pool, &account_id, "huge-model", 4);
        let window = exposure_window(&pool);
        pool.get()
            .unwrap()
            .execute(
                "UPDATE jobs_provider_cost_holds SET model = ?1",
                params!["x".repeat(MAX_PROVIDER_EXPOSURE_BYTES + 1)],
            )
            .unwrap();
        assert!(read_provider_exposure_report(&pool, &account_id, window)
            .unwrap_err()
            .to_string()
            .contains("byte cap"));
        pool.get()
            .unwrap()
            .execute(
                "UPDATE jobs_provider_cost_holds SET model = 'gpt-5.4-mini'",
                [],
            )
            .unwrap();
        pool.get()
            .unwrap()
            .execute(
                "WITH RECURSIVE n(x) AS
            (VALUES(0) UNION ALL SELECT x + 1 FROM n WHERE x < 1000)
            INSERT INTO jobs_provider_cost_holds(request_scope_hash, account_scope_hash,
                generation_scope_hash, root_scope_hash, reservation_token, provider, model,
                projected_cost_cents, created_at_ms, updated_at_ms)
            SELECT printf('%064x', x), account_scope_hash, generation_scope_hash, root_scope_hash,
                reservation_token, provider, model, 1, created_at_ms, updated_at_ms
            FROM n, jobs_provider_cost_holds WHERE reservation_token = 'token:huge-model'",
                [],
            )
            .unwrap();
        assert!(read_provider_exposure_report(&pool, &account_id, window)
            .unwrap_err()
            .to_string()
            .contains("row cap"));
    }

    #[test]
    fn sqlite_exposure_report_validates_hold_content_before_linkage_coercion_and_keeps_exact_zero()
    {
        let pool = temp_pool();
        let account_id = account(&pool);
        exposure_settle(
            &pool,
            &account_id,
            "exact-zero",
            5,
            0,
            UsageProvenance::Exact,
            "jobs_resume_tailoring_accepted",
        );
        let window = exposure_window(&pool);
        let report = read_provider_exposure_report(&pool, &account_id, window).unwrap();
        assert_eq!(report.provider_exposure_cents, 0);
        assert_eq!(report.missing_usage_linkage_holds, 0);
        assert_eq!(report.mismatched_usage_linkage_holds, 0);
        assert_eq!(report.per_success_ratio, "unavailable");
        let conn = pool.get().unwrap();
        conn.execute("PRAGMA ignore_check_constraints = ON", [])
            .unwrap();
        for (column, value) in [("status", "unknown"), ("usage_provenance", "untrusted")] {
            conn.execute(
                &format!("UPDATE jobs_provider_cost_holds SET {column} = ?1"),
                params![value],
            )
            .unwrap();
            assert!(read_provider_exposure_report(&pool, &account_id, window)
                .unwrap_err()
                .to_string()
                .contains("invalid hold content"));
            conn.execute(
                "UPDATE jobs_provider_cost_holds SET status = 'settled',
                usage_provenance = 'exact'",
                [],
            )
            .unwrap();
        }
        conn.execute(
            "UPDATE jobs_provider_cost_holds SET settled_cost_cents = -1",
            [],
        )
        .unwrap();
        assert!(read_provider_exposure_report(&pool, &account_id, window).is_err());
        conn.execute("PRAGMA ignore_check_constraints = OFF", [])
            .unwrap();
    }

    #[test]
    fn sqlite_exposure_report_charges_cumulative_sql_rows_before_owned_copies() {
        let pool = temp_pool();
        let account_id = account(&pool);
        exposure_reserve(&pool, &account_id, "bounded-model", 1);
        let conn = pool.get().unwrap();
        conn.execute(
            "UPDATE jobs_provider_cost_holds SET model = ?1",
            params!["x".repeat(64_000)],
        )
        .unwrap();
        conn.execute(
            "WITH RECURSIVE n(x) AS
            (VALUES(0) UNION ALL SELECT x + 1 FROM n WHERE x < 29)
            INSERT INTO jobs_provider_cost_holds(request_scope_hash, account_scope_hash,
                generation_scope_hash, root_scope_hash, reservation_token, provider, model,
                projected_cost_cents, created_at_ms, updated_at_ms)
            SELECT printf('%064x', x), account_scope_hash, generation_scope_hash, root_scope_hash,
                reservation_token, provider, model, 1, created_at_ms, updated_at_ms
            FROM n, jobs_provider_cost_holds WHERE reservation_token = 'token:bounded-model'",
            [],
        )
        .unwrap();
        let projection = super::super::jobs::bounded_report_projection(false, &["model"], &[0]);
        let (value, charge): (Option<String>, i64) = conn
            .query_row(
                &format!("SELECT {projection} FROM jobs_provider_cost_holds LIMIT 1"),
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(value.as_ref().map(String::len), Some(64_000));
        assert!(charge < MAX_PROVIDER_EXPOSURE_BYTES as i64);
        assert!(charge * 31 > MAX_PROVIDER_EXPOSURE_BYTES as i64);
        drop(conn);
        let window = exposure_window(&pool);
        assert!(read_provider_exposure_report(&pool, &account_id, window)
            .unwrap_err()
            .to_string()
            .contains("byte cap"));
        let mut small_output_budget = 64;
        assert!(super::super::jobs::charge_report_serialization(
            &mut small_output_budget,
            &"x".repeat(128)
        )
        .unwrap_err()
        .to_string()
        .contains("byte cap"));
    }

    #[test]
    fn jobs_generation_hold_blocks_provider_reservation_until_explicit_release() {
        let pool = temp_pool();
        let account_id = account(&pool);
        let generation_key = "router:held-generation:llm";
        let reservation_token = "held-generation-token";
        let now = chrono::Utc::now().timestamp_millis();
        let posting_json = serde_json::json!({
            "company": "Acme",
            "title": "Engineer",
            "source": "greenhouse",
            "canonical_url": "https://boards.greenhouse.io/acme/jobs/held-generation",
            "location": "New York, NY",
            "track_id": "software-engineering",
        })
        .to_string();
        let conn = pool.get().unwrap();
        conn.execute(
            "INSERT INTO jobs_postings (
                id, account_id, canonical_key, posting_json, source, canonical_url,
                company, title, location, match_score, status, created_at_ms, updated_at_ms
             ) VALUES (
                'job-held-generation', ?1, 'job-held-generation', ?3,
                'greenhouse', 'https://boards.greenhouse.io/acme/jobs/held-generation',
                'Acme', 'Engineer', 'New York, NY', 90, 'matched', ?2, ?2
             )",
            params![account_id, now, posting_json],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO jobs_resume_generations (
                id, account_id, job_id, generation_key, reservation_token,
                status, created_at_ms, updated_at_ms
             ) VALUES (
                'generation-held', ?1, 'job-held-generation', ?2, ?3,
                'reserved', ?4, ?4
             )",
            params![account_id, generation_key, reservation_token, now],
        )
        .unwrap();
        drop(conn);
        append_generation_hold(
            &pool,
            &account_id,
            "generation-hold-1",
            super::super::jobs::OperationalHoldTransition::Held,
            0,
            None,
        );
        let guard = UpstreamSpendGuard {
            limit_cents: 1_000,
            window_hours: 24,
        };
        assert!(reserve(
            &pool,
            &account_id,
            generation_key,
            reservation_token,
            "held-generation-attempt",
            "openai",
            "gpt-5.4-mini",
            3,
            100,
            guard,
        )
        .is_err());
        assert_eq!(
            pool.get()
                .unwrap()
                .query_row("SELECT COUNT(*) FROM jobs_provider_cost_holds", [], |row| {
                    row.get::<_, i64>(0)
                })
                .unwrap(),
            0
        );
        append_generation_hold(
            &pool,
            &account_id,
            "generation-hold-2",
            super::super::jobs::OperationalHoldTransition::Released,
            1,
            Some("generation-hold-1"),
        );
        assert!(matches!(
            reserve(
                &pool,
                &account_id,
                generation_key,
                reservation_token,
                "held-generation-attempt",
                "openai",
                "gpt-5.4-mini",
                3,
                100,
                guard,
            )
            .unwrap(),
            CostHoldReservation::Held { .. }
        ));
    }

    #[test]
    #[serial_test::serial]
    fn postgres_runtime_reserve_and_exact_settlement_preserve_every_usage_field() {
        let Ok(database_url) = std::env::var("BLUEY_TEST_POSTGRES_URL") else {
            return;
        };
        let pool = crate::db::open_postgres_pool(&database_url).expect("open Postgres test pool");
        crate::db::run_migrations(&pool).expect("apply Postgres runtime migrations");

        let suffix = uuid::Uuid::new_v4().simple().to_string();
        let account_id = format!("acct_provider_truth_{suffix}");
        let email = format!("provider-truth-{suffix}@example.test");
        let generation_key = format!("router:pg18-{suffix}:llm");
        let request_id = format!("pg18-{suffix}:attempt:0");
        let reservation_token = format!("pg18-token-{suffix}");
        {
            let mut conn = pool.get_pg().expect("Postgres setup connection");
            conn.execute(
                "INSERT INTO accounts (id, email, password_hash) VALUES ($1, $2, 'hash')",
                &[&account_id, &email],
            )
            .expect("insert Postgres test account");
        }

        let admission = reserve(
            &pool,
            &account_id,
            &generation_key,
            &reservation_token,
            &request_id,
            "openai",
            "gpt-5.4-mini",
            17,
            100,
            UpstreamSpendGuard {
                limit_cents: 100_000_000,
                window_hours: 24,
            },
        )
        .expect("reserve Postgres provider hold");
        assert_eq!(
            admission,
            CostHoldReservation::Held {
                reservation_token: reservation_token.clone()
            }
        );

        let event = UsageEvent {
            request_id: request_id.clone(),
            kind: "llm_attempt".into(),
            task_type: Some("pg18_settlement".into()),
            lane: Some("balanced".into()),
            provider: Some("openai".into()),
            model: Some("gpt-5.4-mini".into()),
            input_tokens: 123,
            output_tokens: 45,
            latency_ms: 67,
            cost_cents_to_bluey: 5,
            cost_cents_to_customer: 3,
            was_speculative: true,
            was_fallback: true,
        };
        settle_with_usage(
            &pool,
            &account_id,
            &request_id,
            &reservation_token,
            5,
            UsageProvenance::Exact,
            &event,
        )
        .expect("settle exact Postgres provider usage");
        settle_with_usage(
            &pool,
            &account_id,
            &request_id,
            &reservation_token,
            5,
            UsageProvenance::Exact,
            &event,
        )
        .expect("repeat exact Postgres settlement idempotently");

        let request_hash = request_scope_hash(&account_id, &request_id);
        let mut conn = pool.get_pg().expect("Postgres assertion connection");
        let row = conn
            .query_one(
                "SELECT h.projected_cost_cents, h.settled_cost_cents,
                        h.usage_provenance, h.status,
                        e.kind, e.task_type, e.lane, e.provider, e.model,
                        e.input_tokens, e.output_tokens, e.latency_ms,
                        e.cost_cents_to_bluey, e.cost_cents_to_customer,
                        e.was_speculative, e.was_fallback
                   FROM jobs_provider_cost_holds h
                   JOIN usage_events e
                     ON e.account_id = $2 AND e.request_id = $3
                    AND e.kind = 'llm_attempt'
                  WHERE h.request_scope_hash = $1",
                &[&request_hash, &account_id, &request_id],
            )
            .expect("read exact Postgres settlement");
        assert_eq!(row.get::<_, i64>(0), 17);
        assert_eq!(row.get::<_, i64>(1), 5);
        assert_eq!(row.get::<_, String>(2), "exact");
        assert_eq!(row.get::<_, String>(3), "settled");
        assert_eq!(row.get::<_, String>(4), "llm_attempt");
        assert_eq!(
            row.get::<_, Option<String>>(5).as_deref(),
            Some("pg18_settlement")
        );
        assert_eq!(row.get::<_, Option<String>>(6).as_deref(), Some("balanced"));
        assert_eq!(row.get::<_, Option<String>>(7).as_deref(), Some("openai"));
        assert_eq!(
            row.get::<_, Option<String>>(8).as_deref(),
            Some("gpt-5.4-mini")
        );
        assert_eq!(row.get::<_, i64>(9), 123);
        assert_eq!(row.get::<_, i64>(10), 45);
        assert_eq!(row.get::<_, i64>(11), 67);
        assert_eq!(row.get::<_, i64>(12), 5);
        assert_eq!(row.get::<_, i64>(13), 3);
        assert_eq!(row.get::<_, i32>(14), 1);
        assert_eq!(row.get::<_, i32>(15), 1);
        let event_count: i64 = conn
            .query_one(
                "SELECT COUNT(*) FROM usage_events
                  WHERE account_id = $1 AND request_id = $2 AND kind = 'llm_attempt'",
                &[&account_id, &request_id],
            )
            .expect("count idempotent Postgres event")
            .get(0);
        assert_eq!(event_count, 1);

        conn.execute(
            "DELETE FROM jobs_provider_cost_holds WHERE request_scope_hash = $1",
            &[&request_hash],
        )
        .expect("delete Postgres test hold");
        conn.execute("DELETE FROM accounts WHERE id = $1", &[&account_id])
            .expect("delete Postgres test account");
    }

    #[test]
    fn recovered_hold_precedes_later_generation_hold_without_weakening_identity() {
        let pool = temp_pool();
        let account_id = account(&pool);
        let guard = UpstreamSpendGuard {
            limit_cents: 1_000,
            window_hours: 24,
        };
        let first = reserve(
            &pool,
            &account_id,
            "router:req:llm",
            "first-token",
            "req:attempt:0",
            "openai",
            "gpt-5.4-mini",
            3,
            100,
            guard,
        )
        .unwrap();
        assert_eq!(
            first,
            CostHoldReservation::Held {
                reservation_token: "first-token".into()
            }
        );
        append_generation_hold(
            &pool,
            &account_id,
            "recovered-generation-hold",
            super::super::jobs::OperationalHoldTransition::Held,
            0,
            None,
        );
        let recovered = reserve(
            &pool,
            &account_id,
            "router:req:llm",
            "different-token",
            "req:attempt:0",
            "openai",
            "gpt-5.4-mini",
            3,
            100,
            guard,
        )
        .unwrap();
        assert_eq!(
            recovered,
            CostHoldReservation::RecoveredAmbiguous {
                reservation_token: "first-token".into()
            }
        );
        let changed_identity = reserve(
            &pool,
            &account_id,
            "router:req:llm",
            "different-token",
            "req:attempt:0",
            "openai",
            "gpt-5.6-mini",
            3,
            100,
            guard,
        );
        assert!(changed_identity
            .unwrap_err()
            .to_string()
            .contains("identity collision"));
        let count: i64 = pool
            .get()
            .unwrap()
            .query_row("SELECT COUNT(*) FROM jobs_provider_cost_holds", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn scope_prefix_fence_is_delimiter_safe_for_wildcards_and_unicode() {
        let pool = temp_pool();
        let account_id = account(&pool);
        reserve(
            &pool,
            &account_id,
            "router:req%_☃:llm",
            "token",
            "special:attempt:0",
            "openai",
            "gpt-5.4-mini",
            2,
            100,
            UpstreamSpendGuard {
                limit_cents: 1_000,
                window_hours: 24,
            },
        )
        .unwrap();
        assert!(has_scope_prefix_exposure(&pool, &account_id, "router:req%_☃:").unwrap());
        assert!(!has_scope_prefix_exposure(&pool, &account_id, "router:req%_:").unwrap());
        assert!(!has_scope_prefix_exposure(&pool, &account_id, "router:req%_☃x:").unwrap());
    }

    #[test]
    fn anonymous_cutover_baseline_uses_window_but_fixed_retention() {
        let pool = temp_pool();
        let account_id = account(&pool);
        pool.get()
            .unwrap()
            .execute(
                "INSERT INTO usage_cutover_spend_baseline(occurred_at, cost_cents)
                 VALUES (datetime('now'), 7)",
                [],
            )
            .unwrap();
        let guard = UpstreamSpendGuard {
            limit_cents: 10,
            window_hours: 24,
        };
        assert_eq!(
            reserve(
                &pool,
                &account_id,
                "router:baseline-blocked:llm",
                "baseline-blocked-token",
                "baseline-blocked:attempt:0",
                "openai",
                "gpt-5.4-mini",
                4,
                100,
                guard,
            )
            .unwrap(),
            CostHoldReservation::GlobalLimit
        );

        pool.get()
            .unwrap()
            .execute(
                "UPDATE usage_cutover_spend_baseline
                    SET occurred_at = datetime('now', '-3 days')",
                [],
            )
            .unwrap();
        assert!(matches!(
            reserve(
                &pool,
                &account_id,
                "router:baseline-expired:llm",
                "baseline-expired-token",
                "baseline-expired:attempt:0",
                "openai",
                "gpt-5.4-mini",
                4,
                100,
                UpstreamSpendGuard {
                    limit_cents: 4,
                    window_hours: 24,
                },
            )
            .unwrap(),
            CostHoldReservation::Held { .. }
        ));
        // Shrinking the active cap window must not shrink physical retention.
        // The maximum configurable window plus grace is a fixed privacy and
        // accounting contract, independent of the current guard setting.
        assert_eq!(
            pool.get()
                .unwrap()
                .query_row(
                    "SELECT COUNT(*) FROM usage_cutover_spend_baseline",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
            1
        );
        pool.get()
            .unwrap()
            .execute(
                "UPDATE usage_cutover_spend_baseline
                    SET occurred_at = datetime('now', '-32 days')",
                [],
            )
            .unwrap();
        let cleanup = prune_expired_spend_truth(&pool).unwrap();
        assert_eq!(cleanup.cutover_baseline_rows_deleted, 1);
    }

    #[test]
    fn old_binary_insert_is_legacy_and_proves_paid_rollout_must_be_atomic() {
        let pool = temp_pool();
        let account_id = account(&pool);
        // This is the pre-cutover server shape: it omits `origin` entirely.
        // The compatibility default must not elevate that unverified row to
        // server authority, so a mixed-version paid deployment cannot rely on
        // the new spend guard to see it.
        pool.get()
            .unwrap()
            .execute(
                "INSERT INTO usage_events (
                    id, account_id, request_id, kind, task_type, lane, provider, model,
                    input_tokens, output_tokens, latency_ms,
                    cost_cents_to_bluey, cost_cents_to_customer,
                    was_speculative, was_fallback
                 ) VALUES (?1, ?2, 'old-paid-request', 'llm', 'general', 'instant',
                           'openai', 'gpt-5.4-mini', 10, 5, 1, 50, 1, 0, 0)",
                params![uuid::Uuid::new_v4().to_string(), account_id],
            )
            .unwrap();
        let origin: String = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT origin FROM usage_events WHERE request_id = 'old-paid-request'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(origin, "legacy_unverified");
        assert_eq!(
            crate::db::usage::bluey_spend_cents_in_window(&pool, 24).unwrap(),
            0
        );

        // The new hold path correctly ignores unverified history and therefore
        // admits this cent. That is safe only when every paid dispatcher has
        // moved to holds; it is executable proof against a mixed old/new
        // rollout or rollback with paid routes enabled.
        assert!(matches!(
            reserve(
                &pool,
                &account_id,
                "router:new-paid-request:llm",
                "new-token",
                "new-paid-request:attempt:0",
                "openai",
                "gpt-5.4-mini",
                1,
                100,
                UpstreamSpendGuard {
                    limit_cents: 1,
                    window_hours: 24,
                },
            )
            .unwrap(),
            CostHoldReservation::Held { .. }
        ));
    }

    #[test]
    fn account_deletion_retains_bounded_opaque_global_spend_truth() {
        let pool = temp_pool();
        let deleted_account = account(&pool);
        let guard = UpstreamSpendGuard {
            limit_cents: 10,
            window_hours: 24,
        };
        assert_eq!(
            reserve(
                &pool,
                &deleted_account,
                "router:private-request:llm",
                "deleted-token",
                "private-request:llm-attempt:0",
                "openai",
                "gpt-5.4-mini",
                7,
                100,
                guard,
            )
            .unwrap(),
            CostHoldReservation::Held {
                reservation_token: "deleted-token".into()
            }
        );

        pool.get()
            .unwrap()
            .execute(
                "DELETE FROM accounts WHERE id = ?1",
                params![deleted_account],
            )
            .unwrap();

        let event = UsageEvent {
            request_id: "private-request:llm-attempt:0".into(),
            kind: "llm_attempt".into(),
            task_type: Some("balanced".into()),
            lane: Some("balanced".into()),
            provider: Some("openai".into()),
            model: Some("gpt-5.4-mini".into()),
            input_tokens: 10,
            output_tokens: 5,
            latency_ms: 100,
            cost_cents_to_bluey: 7,
            cost_cents_to_customer: 0,
            was_speculative: false,
            was_fallback: false,
        };
        settle_with_usage(
            &pool,
            &deleted_account,
            "private-request:llm-attempt:0",
            "deleted-token",
            7,
            UsageProvenance::Exact,
            &event,
        )
        .unwrap();

        let conn = pool.get().unwrap();
        let (status, cost, account_hash, request_hash, generation_hash): (
            String,
            i64,
            String,
            String,
            String,
        ) = conn
            .query_row(
                "SELECT status, settled_cost_cents, account_scope_hash,
                        request_scope_hash, generation_scope_hash
                   FROM jobs_provider_cost_holds",
                [],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!((status.as_str(), cost), ("settled", 7));
        assert_ne!(account_hash, deleted_account);
        assert!(!request_hash.contains("private-request"));
        assert!(!generation_hash.contains("private-request"));
        let deleted_usage: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM usage_events WHERE request_id = ?1",
                params![event.request_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(deleted_usage, 0);
        drop(conn);

        let surviving_account = account(&pool);
        assert_eq!(
            reserve(
                &pool,
                &surviving_account,
                "router:new-request:llm",
                "new-token",
                "new-request:llm-attempt:0",
                "openai",
                "gpt-5.4-mini",
                4,
                100,
                guard,
            )
            .unwrap(),
            CostHoldReservation::GlobalLimit
        );

        pool.get()
            .unwrap()
            .execute(
                "UPDATE jobs_provider_cost_holds SET updated_at_ms = ?1",
                params![crate::db::jobs::now_ms()
                    .saturating_sub(UPSTREAM_SPEND_TRUTH_RETENTION_MS)
                    // SQLite's julianday conversion is millisecond-granular
                    // and may round a boundary timestamp slightly backward.
                    // Keep this fixture decisively outside retention so the
                    // test measures deletion, not clock-conversion jitter.
                    .saturating_sub(1_000)],
            )
            .unwrap();
        let cleanup = prune_expired_spend_truth(&pool).unwrap();
        assert_eq!(cleanup.provider_holds_deleted, 1);
        assert!(matches!(
            reserve(
                &pool,
                &surviving_account,
                "router:new-request:llm",
                "new-token",
                "new-request:llm-attempt:0",
                "openai",
                "gpt-5.4-mini",
                4,
                100,
                guard,
            )
            .unwrap(),
            CostHoldReservation::Held { .. }
        ));
    }

    #[test]
    fn sqlite_exposure_sum_clamps_hostile_legacy_rows_without_overflow() {
        let pool = temp_pool();
        let conn = pool.get().unwrap();
        conn.execute_batch(
            "CREATE TEMP TABLE hostile_costs(value INTEGER NOT NULL);
             INSERT INTO hostile_costs VALUES (9223372036854775807);
             INSERT INTO hostile_costs VALUES (9223372036854775807);
             INSERT INTO hostile_costs VALUES (-9223372036854775808);",
        )
        .unwrap();
        let total = crate::db::usage::sqlite_saturated_cost_sum(
            &conn,
            "SELECT value FROM hostile_costs",
            [],
        )
        .unwrap();
        assert_eq!(total, 200_000_000);
    }
}
