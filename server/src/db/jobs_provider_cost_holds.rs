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
            let jobs_fence: Option<(String, String)> = tx
                .query_row(
                    "SELECT reservation_token, status FROM jobs_resume_generations
                      WHERE account_id = ?1 AND generation_key = ?2",
                    params![account_id, generation_key],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()?;
            if let Some((active_token, status)) = jobs_fence {
                if active_token != reservation_token || status != "reserved" {
                    anyhow::bail!("Jobs provider cost hold lost its generation lease fence")
                }
                tx.execute(
                    "UPDATE jobs_resume_generations SET updated_at_ms = ?3
                      WHERE account_id = ?1 AND generation_key = ?2
                        AND reservation_token = ?4 AND status = 'reserved'",
                    params![account_id, generation_key, now, reservation_token],
                )?;
            }
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
            let now = postgres_transaction_now_ms(&mut tx)?;
            let window_start = now.saturating_sub(guard.window_hours.saturating_mul(3_600_000));
            let retention_cutoff = now.saturating_sub(UPSTREAM_SPEND_TRUTH_RETENTION_MS);
            let jobs_fence = tx.query_opt(
                "SELECT reservation_token, status FROM jobs_resume_generations
                  WHERE account_id = $1 AND generation_key = $2 FOR UPDATE",
                &[&account_id, &generation_key],
            )?;
            if let Some(row) = jobs_fence {
                let active_token: String = row.get(0);
                let status: String = row.get(1);
                if active_token != reservation_token || status != "reserved" {
                    anyhow::bail!("Jobs provider cost hold lost its generation lease fence")
                }
                tx.execute(
                    "UPDATE jobs_resume_generations SET updated_at_ms = $3
                      WHERE account_id = $1 AND generation_key = $2
                        AND reservation_token = $4 AND status = 'reserved'",
                    &[&account_id, &generation_key, &now, &reservation_token],
                )?;
            }
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

#[cfg(test)]
mod tests {
    use super::*;

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
    fn recovered_hold_returns_persisted_token_without_creating_a_second_hold() {
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
