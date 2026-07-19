//! Atomic reservation and settlement for managed provider usage.
//!
//! Paid requests move their maximum customer charge from spendable balance to
//! `accounts.reserved_cents` before provider dispatch. Trial requests reserve
//! the account's remaining trial time, which deliberately admits only one
//! trial LLM request at a time. Every terminal path either settles actual
//! usage or releases the reservation; expired rows are safe to reconcile by
//! `(account_id, request_id)`.

use rusqlite::{params, OptionalExtension, TransactionBehavior};

use crate::config::UpstreamSpendGuard;
use crate::db::{
    balance,
    usage::{
        UsageEvent, CUTOVER_SPEND_BASELINE_GRACE_MS, MAX_AUTHORITATIVE_EVENT_COST_CENTS,
        MAX_AUTHORITATIVE_EVENT_LATENCY_MS, MAX_AUTHORITATIVE_EVENT_TOKENS,
    },
    DbPool,
};

const STATUS_RESERVED: &str = "reserved";
const STATUS_SETTLED: &str = "settled";
const STATUS_RELEASED: &str = "released";

#[derive(Debug, Clone, Copy)]
pub(crate) struct ReserveUsageInput<'a> {
    pub account_id: &'a str,
    pub request_id: &'a str,
    pub kind: &'a str,
    pub reason: &'a str,
    pub estimated_customer_cents: i64,
    pub estimated_upstream_cents: i64,
    pub upstream_spend_guard: Option<UpstreamSpendGuard>,
    pub created_at_ms: i64,
    pub expires_at_ms: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ReservedUsage {
    pub attempt: i64,
    pub reserved_cents: i64,
    pub reserved_trial_seconds: i64,
    pub estimated_customer_cents: i64,
    pub estimated_upstream_cents: i64,
    pub expires_at_ms: i64,
}

impl ReservedUsage {
    pub(crate) fn is_trial(self) -> bool {
        self.reserved_trial_seconds > 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SettledUsage {
    pub attempt: i64,
    pub actual_customer_cents: i64,
    pub charged_customer_cents: i64,
    pub refunded_cents: i64,
    pub settled_trial_seconds: i64,
    pub refunded_trial_seconds: i64,
    pub balance_cents_after: i64,
    pub trial_seconds_remaining: i64,
}

#[derive(Debug, Clone)]
pub(crate) struct SettlementUsageEvent {
    pub event: UsageEvent,
    /// Desired customer charge for this component before reservation/trial
    /// settlement. Components are funded in slice order.
    pub customer_cost_cents: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ReleasedUsage {
    pub attempt: i64,
    pub refunded_cents: i64,
    pub refunded_trial_seconds: i64,
    pub balance_cents_after: i64,
    pub trial_seconds_remaining: i64,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum UsageReservationError {
    #[error("invalid managed usage reservation")]
    InvalidReservation,
    #[error("balance is required before dispatching this managed request")]
    InsufficientBalance,
    #[error("account is unavailable for managed usage")]
    AccountUnavailable,
    #[error("managed usage request is already reserved")]
    InProgress,
    #[error("managed usage request is already settled")]
    AlreadySettled,
    #[error("managed usage request reservation was already released")]
    AlreadyReleased,
    #[error("managed usage reservation was not found")]
    NotFound,
    #[error("global upstream spend limit would be exceeded")]
    UpstreamSpendLimit,
    #[error(transparent)]
    Db(#[from] anyhow::Error),
}

#[derive(Debug, Clone)]
struct ReservationRow {
    kind: String,
    status: String,
    attempt: i64,
    estimated_customer_cents: i64,
    estimated_upstream_cents: i64,
    reserved_cents: i64,
    actual_customer_cents: i64,
    settled_cents: i64,
    refunded_cents: i64,
    reserved_trial_seconds: i64,
    settled_trial_seconds: i64,
    refunded_trial_seconds: i64,
    expires_at_ms: i64,
    balance_cents_after: Option<i64>,
    trial_seconds_remaining_after: Option<i64>,
}

#[derive(Debug, Clone, Copy)]
struct AccountSnapshot {
    balance_cents: i64,
    trial_seconds_remaining: i64,
    billing_restricted: bool,
}

/// The Postgres compatibility schema deliberately mirrors SQLite flags with
/// `INTEGER` columns. Decode them as `i32` instead of asking `postgres` to
/// deserialize an `INT4` value as a SQL `BOOLEAN`.
fn postgres_integer_flag(row: &postgres::Row, index: usize) -> Result<bool, UsageReservationError> {
    row.try_get::<_, i32>(index)
        .map(|value| value != 0)
        .map_err(|error| UsageReservationError::Db(error.into()))
}

pub(crate) fn reserve(
    pool: &DbPool,
    input: ReserveUsageInput<'_>,
) -> Result<ReservedUsage, UsageReservationError> {
    validate_reserve_input(input)?;
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => reserve_sqlite(pool, input),
        DbPool::Postgres(_) => reserve_postgres(pool, input),
    })
}

fn validate_reserve_input(input: ReserveUsageInput<'_>) -> Result<(), UsageReservationError> {
    if input.account_id.trim().is_empty()
        || input.request_id.trim().is_empty()
        || input.kind.trim().is_empty()
        || input.reason.trim().is_empty()
        || input.estimated_customer_cents < 0
        || input.estimated_upstream_cents < 0
        || input.estimated_customer_cents > MAX_AUTHORITATIVE_EVENT_COST_CENTS
        || input.estimated_upstream_cents > MAX_AUTHORITATIVE_EVENT_COST_CENTS
        || input.expires_at_ms <= input.created_at_ms
    {
        return Err(UsageReservationError::InvalidReservation);
    }
    Ok(())
}

fn reserve_sqlite(
    pool: &DbPool,
    input: ReserveUsageInput<'_>,
) -> Result<ReservedUsage, UsageReservationError> {
    let mut conn = pool.get().map_err(UsageReservationError::Db)?;
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| UsageReservationError::Db(error.into()))?;

    let account = tx
        .query_row(
            "SELECT balance_cents, trial_seconds_remaining, billing_restricted
               FROM accounts
              WHERE id = ?1",
            params![input.account_id],
            |row| {
                Ok(AccountSnapshot {
                    balance_cents: row.get(0)?,
                    trial_seconds_remaining: row.get(1)?,
                    billing_restricted: row.get::<_, i64>(2)? != 0,
                })
            },
        )
        .optional()
        .map_err(|error| UsageReservationError::Db(error.into()))?
        .ok_or(UsageReservationError::AccountUnavailable)?;
    if account.billing_restricted {
        return Err(UsageReservationError::AccountUnavailable);
    }

    let existing: Option<(String, i64)> = tx
        .query_row(
            "SELECT status, attempt
               FROM usage_reservations
              WHERE account_id = ?1 AND request_id = ?2",
            params![input.account_id, input.request_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|error| UsageReservationError::Db(error.into()))?;
    let attempt = next_attempt(existing.as_ref())?;
    if let Some(guard) = input.upstream_spend_guard {
        let baseline_retention_ms = guard
            .window_hours
            .max(0)
            .saturating_mul(3_600_000)
            .saturating_add(CUTOVER_SPEND_BASELINE_GRACE_MS);
        tx.execute(
            "DELETE FROM usage_cutover_spend_baseline
              WHERE occurred_at < datetime('now', ?1)",
            params![format!("-{} seconds", baseline_retention_ms / 1_000)],
        )
        .map_err(|error| UsageReservationError::Db(error.into()))?;
        let usage_without_jobs_holds = super::usage::sqlite_saturated_cost_sum(
            &tx,
            "SELECT u.cost_cents_to_bluey
                   FROM usage_events u
                  WHERE u.origin = 'server' AND u.ts >= datetime('now', ?1)
                    AND substr(u.kind, -8) != '_attempt'",
            params![format!("-{} hours", guard.window_hours)],
        )
        .map_err(UsageReservationError::Db)?;
        let window_start = input
            .created_at_ms
            .saturating_sub(guard.window_hours.saturating_mul(3_600_000));
        let jobs_exposure = super::usage::sqlite_saturated_cost_sum(
            &tx,
            "SELECT CASE WHEN status = 'held'
                             THEN projected_cost_cents ELSE settled_cost_cents END
                   FROM jobs_provider_cost_holds
                  WHERE status IN ('held', 'settled') AND updated_at_ms >= ?1",
            params![window_start],
        )
        .map_err(UsageReservationError::Db)?;
        let cutover_baseline_exposure = super::usage::sqlite_saturated_cost_sum(
            &tx,
            "SELECT cost_cents FROM usage_cutover_spend_baseline
              WHERE occurred_at >= datetime('now', ?1)",
            params![format!("-{} hours", guard.window_hours)],
        )
        .map_err(UsageReservationError::Db)?;
        let ordinary_exposure = super::usage::sqlite_saturated_cost_sum(
            &tx,
            "SELECT estimated_upstream_cents
                   FROM usage_reservations r
                  WHERE NOT (r.account_id = ?1 AND r.request_id = ?2)
                    AND ((r.status = 'reserved' AND r.expires_at_ms > ?3)
                      OR (r.status = 'settled'
                        AND r.settled_at_ms >= ?4
                        AND NOT EXISTS (
                            SELECT 1 FROM usage_events u
                             WHERE u.account_id = r.account_id
                               AND u.request_id = r.request_id
                               AND u.origin = 'server'
                               AND u.kind = r.kind
                        )))",
            params![
                input.account_id,
                input.request_id,
                input.created_at_ms,
                window_start
            ],
        )
        .map_err(UsageReservationError::Db)?;
        if usage_without_jobs_holds
            .saturating_add(jobs_exposure)
            .saturating_add(cutover_baseline_exposure)
            .saturating_add(ordinary_exposure)
            .saturating_add(input.estimated_upstream_cents)
            > guard.limit_cents
        {
            return Err(UsageReservationError::UpstreamSpendLimit);
        }
    }
    let active_trial_reservation: bool = tx
        .query_row(
            "SELECT EXISTS(
                SELECT 1
                  FROM usage_reservations
                 WHERE account_id = ?1
                   AND status = 'reserved'
                   AND reserved_trial_seconds > 0
                   AND request_id <> ?2
             )",
            params![input.account_id, input.request_id],
            |row| row.get(0),
        )
        .map_err(|error| UsageReservationError::Db(error.into()))?;
    if account.trial_seconds_remaining <= 0 && active_trial_reservation {
        return Err(UsageReservationError::InProgress);
    }
    let reserved_trial_seconds = account.trial_seconds_remaining.max(0);
    let reserved_cents = if reserved_trial_seconds > 0 {
        0
    } else {
        input.estimated_customer_cents
    };
    if account.balance_cents < reserved_cents {
        return Err(UsageReservationError::InsufficientBalance);
    }

    let updated = tx
        .execute(
            "UPDATE accounts
                SET balance_cents = balance_cents - ?1,
                    reserved_cents = reserved_cents + ?1,
                    trial_seconds_remaining = trial_seconds_remaining - ?2
              WHERE id = ?3
                AND billing_restricted = 0
                AND balance_cents >= ?1
                AND trial_seconds_remaining >= ?2",
            params![reserved_cents, reserved_trial_seconds, input.account_id],
        )
        .map_err(|error| UsageReservationError::Db(error.into()))?;
    if updated != 1 {
        return Err(UsageReservationError::InsufficientBalance);
    }

    if existing.is_some() {
        let reservation_updated = tx
            .execute(
                "UPDATE usage_reservations
                SET kind = ?3,
                    status = 'reserved',
                    attempt = ?4,
                    estimated_customer_cents = ?5,
                    estimated_upstream_cents = ?6,
                    reserved_cents = ?7,
                    actual_customer_cents = 0,
                    settled_cents = 0,
                    refunded_cents = 0,
                    reserved_trial_seconds = ?8,
                    settled_trial_seconds = 0,
                    refunded_trial_seconds = 0,
                    created_at_ms = ?9,
                    expires_at_ms = ?10,
                    settled_at_ms = NULL,
                    balance_cents_after = NULL,
                    trial_seconds_remaining_after = NULL,
                    reservation_reason = ?11,
                    terminal_reason = NULL
              WHERE account_id = ?1 AND request_id = ?2 AND status = 'released'",
                params![
                    input.account_id,
                    input.request_id,
                    input.kind,
                    attempt,
                    input.estimated_customer_cents,
                    input.estimated_upstream_cents,
                    reserved_cents,
                    reserved_trial_seconds,
                    input.created_at_ms,
                    input.expires_at_ms,
                    input.reason,
                ],
            )
            .map_err(|error| UsageReservationError::Db(error.into()))?;
        if reservation_updated != 1 {
            return Err(UsageReservationError::InvalidReservation);
        }
    } else {
        tx.execute(
            "INSERT INTO usage_reservations (
                account_id, request_id, kind, status, attempt,
                estimated_customer_cents, estimated_upstream_cents,
                reserved_cents, reserved_trial_seconds,
                created_at_ms, expires_at_ms, reservation_reason
             ) VALUES (?1, ?2, ?3, 'reserved', ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                input.account_id,
                input.request_id,
                input.kind,
                attempt,
                input.estimated_customer_cents,
                input.estimated_upstream_cents,
                reserved_cents,
                reserved_trial_seconds,
                input.created_at_ms,
                input.expires_at_ms,
                input.reason,
            ],
        )
        .map_err(|error| UsageReservationError::Db(error.into()))?;
    }

    record_reserve_ledger_sqlite(&tx, input, attempt, account.balance_cents, reserved_cents)?;
    tx.commit()
        .map_err(|error| UsageReservationError::Db(error.into()))?;

    Ok(ReservedUsage {
        attempt,
        reserved_cents,
        reserved_trial_seconds,
        estimated_customer_cents: input.estimated_customer_cents,
        estimated_upstream_cents: input.estimated_upstream_cents,
        expires_at_ms: input.expires_at_ms,
    })
}

fn reserve_postgres(
    pool: &DbPool,
    input: ReserveUsageInput<'_>,
) -> Result<ReservedUsage, UsageReservationError> {
    let mut conn = pool.get_pg().map_err(UsageReservationError::Db)?;
    let mut tx = conn
        .transaction()
        .map_err(|error| UsageReservationError::Db(error.into()))?;
    tx.query_one(
        "SELECT pg_advisory_xact_lock(hashtextextended('jobs-provider-cost-global', 0))",
        &[],
    )
    .map_err(|error| UsageReservationError::Db(error.into()))?;

    let row = tx
        .query_opt(
            "SELECT balance_cents, trial_seconds_remaining, billing_restricted
               FROM accounts
              WHERE id = $1
              FOR UPDATE",
            &[&input.account_id],
        )
        .map_err(|error| UsageReservationError::Db(error.into()))?
        .ok_or(UsageReservationError::AccountUnavailable)?;
    let account = AccountSnapshot {
        balance_cents: row
            .try_get(0)
            .map_err(|error| UsageReservationError::Db(error.into()))?,
        trial_seconds_remaining: row
            .try_get(1)
            .map_err(|error| UsageReservationError::Db(error.into()))?,
        billing_restricted: postgres_integer_flag(&row, 2)?,
    };
    if account.billing_restricted {
        return Err(UsageReservationError::AccountUnavailable);
    }

    let existing = tx
        .query_opt(
            "SELECT status, attempt
               FROM usage_reservations
              WHERE account_id = $1 AND request_id = $2
              FOR UPDATE",
            &[&input.account_id, &input.request_id],
        )
        .map_err(|error| UsageReservationError::Db(error.into()))?
        .map(|row| {
            Ok::<_, UsageReservationError>((
                row.try_get(0)
                    .map_err(|error| UsageReservationError::Db(error.into()))?,
                row.try_get(1)
                    .map_err(|error| UsageReservationError::Db(error.into()))?,
            ))
        })
        .transpose()?;
    let attempt = next_attempt(existing.as_ref())?;
    if let Some(guard) = input.upstream_spend_guard {
        let baseline_retention_ms = guard
            .window_hours
            .max(0)
            .saturating_mul(3_600_000)
            .saturating_add(CUTOVER_SPEND_BASELINE_GRACE_MS);
        tx.execute(
            "DELETE FROM usage_cutover_spend_baseline
              WHERE occurred_at < now() - ($1::bigint * interval '1 millisecond')",
            &[&baseline_retention_ms],
        )
        .map_err(|error| UsageReservationError::Db(error.into()))?;
        let usage_without_jobs_holds: i64 = tx
            .query_one(
                "SELECT LEAST(COALESCE(SUM(LEAST(GREATEST(u.cost_cents_to_bluey, 0), 100000000)::numeric), 0),
                                  9223372036854775807)::bigint
                   FROM usage_events u
                  WHERE u.origin = 'server'
                    AND u.ts >= now() - ($1::bigint * interval '1 hour')
                    AND right(u.kind, 8) != '_attempt'",
                &[&guard.window_hours],
            )
            .map_err(|error| UsageReservationError::Db(error.into()))?
            .get(0);
        let window_start = input
            .created_at_ms
            .saturating_sub(guard.window_hours.saturating_mul(3_600_000));
        let jobs_exposure: i64 = tx
            .query_one(
                "SELECT LEAST(COALESCE(SUM(LEAST(GREATEST(CASE WHEN status = 'held'
                                         THEN projected_cost_cents
                                         ELSE settled_cost_cents END, 0), 100000000)::numeric), 0),
                                  9223372036854775807)::bigint
                   FROM jobs_provider_cost_holds
                  WHERE status IN ('held', 'settled') AND updated_at_ms >= $1",
                &[&window_start],
            )
            .map_err(|error| UsageReservationError::Db(error.into()))?
            .get(0);
        let cutover_baseline_exposure: i64 = tx
            .query_one(
                "SELECT LEAST(COALESCE(SUM(LEAST(GREATEST(cost_cents, 0), 100000000)::numeric), 0),
                                  9223372036854775807)::bigint
                   FROM usage_cutover_spend_baseline
                  WHERE occurred_at >= now() - ($1::bigint * interval '1 hour')",
                &[&guard.window_hours],
            )
            .map_err(|error| UsageReservationError::Db(error.into()))?
            .get(0);
        let ordinary_exposure: i64 = tx
            .query_one(
                "SELECT LEAST(COALESCE(SUM(LEAST(GREATEST(estimated_upstream_cents, 0), 100000000)::numeric), 0),
                                  9223372036854775807)::bigint
                   FROM usage_reservations r
                  WHERE NOT (r.account_id = $1 AND r.request_id = $2)
                    AND ((r.status = 'reserved' AND r.expires_at_ms > $3)
                      OR (r.status = 'settled'
                        AND r.settled_at_ms >= $4
                        AND NOT EXISTS (
                            SELECT 1 FROM usage_events u
                             WHERE u.account_id = r.account_id
                               AND u.request_id = r.request_id
                               AND u.origin = 'server'
                               AND u.kind = r.kind
                        )))",
                &[
                    &input.account_id,
                    &input.request_id,
                    &input.created_at_ms,
                    &window_start,
                ],
            )
            .map_err(|error| UsageReservationError::Db(error.into()))?
            .get(0);
        if usage_without_jobs_holds
            .saturating_add(jobs_exposure)
            .saturating_add(cutover_baseline_exposure)
            .saturating_add(ordinary_exposure)
            .saturating_add(input.estimated_upstream_cents)
            > guard.limit_cents
        {
            return Err(UsageReservationError::UpstreamSpendLimit);
        }
    }
    let active_trial_reservation: bool = tx
        .query_one(
            "SELECT EXISTS(
                SELECT 1
                  FROM usage_reservations
                 WHERE account_id = $1
                   AND status = 'reserved'
                   AND reserved_trial_seconds > 0
                   AND request_id <> $2
             )",
            &[&input.account_id, &input.request_id],
        )
        .map_err(|error| UsageReservationError::Db(error.into()))?
        .try_get(0)
        .map_err(|error| UsageReservationError::Db(error.into()))?;
    if account.trial_seconds_remaining <= 0 && active_trial_reservation {
        return Err(UsageReservationError::InProgress);
    }
    let reserved_trial_seconds = account.trial_seconds_remaining.max(0);
    let reserved_cents = if reserved_trial_seconds > 0 {
        0
    } else {
        input.estimated_customer_cents
    };
    if account.balance_cents < reserved_cents {
        return Err(UsageReservationError::InsufficientBalance);
    }

    let updated = tx
        .execute(
            "UPDATE accounts
                SET balance_cents = balance_cents - $1,
                    reserved_cents = reserved_cents + $1,
                    trial_seconds_remaining = trial_seconds_remaining - $2
              WHERE id = $3
                AND billing_restricted = 0
                AND balance_cents >= $1
                AND trial_seconds_remaining >= $2",
            &[&reserved_cents, &reserved_trial_seconds, &input.account_id],
        )
        .map_err(|error| UsageReservationError::Db(error.into()))?;
    if updated != 1 {
        return Err(UsageReservationError::InsufficientBalance);
    }

    if existing.is_some() {
        let reservation_updated = tx
            .execute(
                "UPDATE usage_reservations
                SET kind = $3,
                    status = 'reserved',
                    attempt = $4,
                    estimated_customer_cents = $5,
                    estimated_upstream_cents = $6,
                    reserved_cents = $7,
                    actual_customer_cents = 0,
                    settled_cents = 0,
                    refunded_cents = 0,
                    reserved_trial_seconds = $8,
                    settled_trial_seconds = 0,
                    refunded_trial_seconds = 0,
                    created_at_ms = $9,
                    expires_at_ms = $10,
                    settled_at_ms = NULL,
                    balance_cents_after = NULL,
                    trial_seconds_remaining_after = NULL,
                    reservation_reason = $11,
                    terminal_reason = NULL
              WHERE account_id = $1 AND request_id = $2 AND status = 'released'",
                &[
                    &input.account_id,
                    &input.request_id,
                    &input.kind,
                    &attempt,
                    &input.estimated_customer_cents,
                    &input.estimated_upstream_cents,
                    &reserved_cents,
                    &reserved_trial_seconds,
                    &input.created_at_ms,
                    &input.expires_at_ms,
                    &input.reason,
                ],
            )
            .map_err(|error| UsageReservationError::Db(error.into()))?;
        if reservation_updated != 1 {
            return Err(UsageReservationError::InvalidReservation);
        }
    } else {
        tx.execute(
            "INSERT INTO usage_reservations (
                account_id, request_id, kind, status, attempt,
                estimated_customer_cents, estimated_upstream_cents,
                reserved_cents, reserved_trial_seconds,
                created_at_ms, expires_at_ms, reservation_reason
             ) VALUES ($1, $2, $3, 'reserved', $4, $5, $6, $7, $8, $9, $10, $11)",
            &[
                &input.account_id,
                &input.request_id,
                &input.kind,
                &attempt,
                &input.estimated_customer_cents,
                &input.estimated_upstream_cents,
                &reserved_cents,
                &reserved_trial_seconds,
                &input.created_at_ms,
                &input.expires_at_ms,
                &input.reason,
            ],
        )
        .map_err(|error| UsageReservationError::Db(error.into()))?;
    }

    record_reserve_ledger_postgres(
        &mut tx,
        input,
        attempt,
        account.balance_cents,
        reserved_cents,
    )?;
    tx.commit()
        .map_err(|error| UsageReservationError::Db(error.into()))?;

    Ok(ReservedUsage {
        attempt,
        reserved_cents,
        reserved_trial_seconds,
        estimated_customer_cents: input.estimated_customer_cents,
        estimated_upstream_cents: input.estimated_upstream_cents,
        expires_at_ms: input.expires_at_ms,
    })
}

fn next_attempt(existing: Option<&(String, i64)>) -> Result<i64, UsageReservationError> {
    match existing {
        None => Ok(1),
        Some((status, _)) if status == STATUS_RESERVED => Err(UsageReservationError::InProgress),
        Some((status, _)) if status == STATUS_SETTLED => Err(UsageReservationError::AlreadySettled),
        Some((status, attempt)) if status == STATUS_RELEASED => Ok(attempt.saturating_add(1)),
        _ => Err(UsageReservationError::InvalidReservation),
    }
}

fn record_reserve_ledger_sqlite(
    tx: &rusqlite::Transaction<'_>,
    input: ReserveUsageInput<'_>,
    attempt: i64,
    balance_before: i64,
    reserved_cents: i64,
) -> Result<(), UsageReservationError> {
    if reserved_cents == 0 {
        return Ok(());
    }
    let evidence_id = format!("{}:{attempt}:reserve", input.request_id);
    let metadata = serde_json::json!({
        "kind": input.kind,
        "attempt": attempt,
        "estimated_customer_cents": input.estimated_customer_cents,
        "estimated_upstream_cents": input.estimated_upstream_cents,
        "expires_at_ms": input.expires_at_ms,
    })
    .to_string();
    balance::insert_balance_ledger_sqlite_tx(
        tx,
        balance::BalanceLedgerEntry {
            account_id: input.account_id,
            event_type: "usage_reserve",
            amount_cents: -reserved_cents,
            balance_cents_before: balance_before,
            balance_cents_after: balance_before - reserved_cents,
            reason: Some(input.reason),
            provider: None,
            processor_payment_id: None,
            source_id: Some(&evidence_id),
            idempotency_key: Some(&evidence_id),
            request_id: Some(input.request_id),
            metadata_json: Some(&metadata),
        },
    )
    .map_err(UsageReservationError::Db)
}

fn record_reserve_ledger_postgres(
    tx: &mut postgres::Transaction<'_>,
    input: ReserveUsageInput<'_>,
    attempt: i64,
    balance_before: i64,
    reserved_cents: i64,
) -> Result<(), UsageReservationError> {
    if reserved_cents == 0 {
        return Ok(());
    }
    let evidence_id = format!("{}:{attempt}:reserve", input.request_id);
    let metadata = serde_json::json!({
        "kind": input.kind,
        "attempt": attempt,
        "estimated_customer_cents": input.estimated_customer_cents,
        "estimated_upstream_cents": input.estimated_upstream_cents,
        "expires_at_ms": input.expires_at_ms,
    })
    .to_string();
    balance::insert_balance_ledger_pg_tx(
        tx,
        balance::BalanceLedgerEntry {
            account_id: input.account_id,
            event_type: "usage_reserve",
            amount_cents: -reserved_cents,
            balance_cents_before: balance_before,
            balance_cents_after: balance_before - reserved_cents,
            reason: Some(input.reason),
            provider: None,
            processor_payment_id: None,
            source_id: Some(&evidence_id),
            idempotency_key: Some(&evidence_id),
            request_id: Some(input.request_id),
            metadata_json: Some(&metadata),
        },
    )
    .map_err(UsageReservationError::Db)
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn settle(
    pool: &DbPool,
    account_id: &str,
    request_id: &str,
    actual_customer_cents: i64,
    elapsed_ms: i64,
    reason: &str,
    settled_at_ms: i64,
) -> Result<SettledUsage, UsageReservationError> {
    settle_with_events(
        pool,
        account_id,
        request_id,
        actual_customer_cents,
        elapsed_ms,
        reason,
        settled_at_ms,
        &[],
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn settle_with_events(
    pool: &DbPool,
    account_id: &str,
    request_id: &str,
    actual_customer_cents: i64,
    elapsed_ms: i64,
    reason: &str,
    settled_at_ms: i64,
    components: &[SettlementUsageEvent],
) -> Result<SettledUsage, UsageReservationError> {
    if account_id.trim().is_empty()
        || request_id.trim().is_empty()
        || reason.trim().is_empty()
        || actual_customer_cents < 0
        || elapsed_ms < 0
        || components.iter().any(|component| {
            component.customer_cost_cents < 0
                || component.customer_cost_cents > MAX_AUTHORITATIVE_EVENT_COST_CENTS
                || component.event.request_id.trim().is_empty()
                || component.event.kind.trim().is_empty()
        })
        || (!components.is_empty()
            && components.iter().fold(0i64, |sum, component| {
                sum.saturating_add(component.customer_cost_cents)
            }) != actual_customer_cents)
    {
        return Err(UsageReservationError::InvalidReservation);
    }
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => settle_sqlite(
            pool,
            account_id,
            request_id,
            actual_customer_cents,
            elapsed_ms,
            reason,
            settled_at_ms,
            components,
        ),
        DbPool::Postgres(_) => settle_postgres(
            pool,
            account_id,
            request_id,
            actual_customer_cents,
            elapsed_ms,
            reason,
            settled_at_ms,
            components,
        ),
    })
}

#[allow(clippy::too_many_arguments)]
fn settle_sqlite(
    pool: &DbPool,
    account_id: &str,
    request_id: &str,
    actual_customer_cents: i64,
    elapsed_ms: i64,
    reason: &str,
    settled_at_ms: i64,
    components: &[SettlementUsageEvent],
) -> Result<SettledUsage, UsageReservationError> {
    let mut conn = pool.get().map_err(UsageReservationError::Db)?;
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| UsageReservationError::Db(error.into()))?;
    let account = account_snapshot_sqlite(&tx, account_id)?;
    let reservation = load_reservation_sqlite(&tx, account_id, request_id)?;
    if reservation.status == STATUS_SETTLED {
        if reservation.actual_customer_cents != actual_customer_cents {
            return Err(UsageReservationError::InvalidReservation);
        }
        let settled = settled_from_row(&reservation)?;
        persist_settlement_events_sqlite(
            &tx,
            account_id,
            request_id,
            &reservation.kind,
            components,
            settled.charged_customer_cents,
            true,
        )?;
        tx.commit()
            .map_err(|error| UsageReservationError::Db(error.into()))?;
        return Ok(settled);
    }
    if reservation.status == STATUS_RELEASED {
        return Err(UsageReservationError::AlreadyReleased);
    }
    if reservation.status != STATUS_RESERVED {
        return Err(UsageReservationError::InvalidReservation);
    }

    let settled_trial_seconds = reservation
        .reserved_trial_seconds
        .min(elapsed_seconds(elapsed_ms));
    let refunded_trial_seconds = reservation
        .reserved_trial_seconds
        .saturating_sub(settled_trial_seconds);
    let charged_customer_cents = if reservation.reserved_trial_seconds > 0 {
        0
    } else {
        actual_customer_cents.min(reservation.reserved_cents)
    };
    let refunded_cents = reservation
        .reserved_cents
        .saturating_sub(charged_customer_cents);
    let balance_after = account.balance_cents.saturating_add(refunded_cents);
    let trial_after = account
        .trial_seconds_remaining
        .saturating_add(refunded_trial_seconds);

    let updated = tx
        .execute(
            "UPDATE accounts
                SET reserved_cents = reserved_cents - ?1,
                    balance_cents = balance_cents + ?2,
                    trial_seconds_remaining = trial_seconds_remaining + ?3
              WHERE id = ?4 AND reserved_cents >= ?1",
            params![
                reservation.reserved_cents,
                refunded_cents,
                refunded_trial_seconds,
                account_id,
            ],
        )
        .map_err(|error| UsageReservationError::Db(error.into()))?;
    if updated != 1 {
        return Err(UsageReservationError::InvalidReservation);
    }
    balance::consume_credit_batches_tx(&tx, account_id, charged_customer_cents)
        .map_err(UsageReservationError::Db)?;

    let updated = tx
        .execute(
            "UPDATE usage_reservations
                SET status = 'settled',
                    actual_customer_cents = ?3,
                    settled_cents = ?4,
                    refunded_cents = ?5,
                    settled_trial_seconds = ?6,
                    refunded_trial_seconds = ?7,
                    settled_at_ms = ?8,
                    balance_cents_after = ?9,
                    trial_seconds_remaining_after = ?10,
                    terminal_reason = ?11
              WHERE account_id = ?1 AND request_id = ?2 AND status = 'reserved'",
            params![
                account_id,
                request_id,
                actual_customer_cents,
                charged_customer_cents,
                refunded_cents,
                settled_trial_seconds,
                refunded_trial_seconds,
                settled_at_ms,
                balance_after,
                trial_after,
                reason,
            ],
        )
        .map_err(|error| UsageReservationError::Db(error.into()))?;
    if updated != 1 {
        return Err(UsageReservationError::AlreadySettled);
    }
    record_terminal_ledger_sqlite(
        &tx,
        account_id,
        request_id,
        &reservation,
        "usage_settle",
        reason,
        account.balance_cents,
        balance_after,
        refunded_cents,
        actual_customer_cents,
        charged_customer_cents,
    )?;
    persist_settlement_events_sqlite(
        &tx,
        account_id,
        request_id,
        &reservation.kind,
        components,
        charged_customer_cents,
        false,
    )?;
    tx.commit()
        .map_err(|error| UsageReservationError::Db(error.into()))?;

    Ok(SettledUsage {
        attempt: reservation.attempt,
        actual_customer_cents,
        charged_customer_cents,
        refunded_cents,
        settled_trial_seconds,
        refunded_trial_seconds,
        balance_cents_after: balance_after,
        trial_seconds_remaining: trial_after,
    })
}

#[allow(clippy::too_many_arguments)]
fn settle_postgres(
    pool: &DbPool,
    account_id: &str,
    request_id: &str,
    actual_customer_cents: i64,
    elapsed_ms: i64,
    reason: &str,
    settled_at_ms: i64,
    components: &[SettlementUsageEvent],
) -> Result<SettledUsage, UsageReservationError> {
    let mut conn = pool.get_pg().map_err(UsageReservationError::Db)?;
    let mut tx = conn
        .transaction()
        .map_err(|error| UsageReservationError::Db(error.into()))?;
    tx.query_one(
        "SELECT pg_advisory_xact_lock(hashtextextended('jobs-provider-cost-global', 0))",
        &[],
    )
    .map_err(|error| UsageReservationError::Db(error.into()))?;
    let account = account_snapshot_postgres(&mut tx, account_id)?;
    let reservation = load_reservation_postgres(&mut tx, account_id, request_id)?;
    if reservation.status == STATUS_SETTLED {
        if reservation.actual_customer_cents != actual_customer_cents {
            return Err(UsageReservationError::InvalidReservation);
        }
        let settled = settled_from_row(&reservation)?;
        persist_settlement_events_postgres(
            &mut tx,
            account_id,
            request_id,
            &reservation.kind,
            components,
            settled.charged_customer_cents,
            true,
        )?;
        tx.commit()
            .map_err(|error| UsageReservationError::Db(error.into()))?;
        return Ok(settled);
    }
    if reservation.status == STATUS_RELEASED {
        return Err(UsageReservationError::AlreadyReleased);
    }
    if reservation.status != STATUS_RESERVED {
        return Err(UsageReservationError::InvalidReservation);
    }

    let settled_trial_seconds = reservation
        .reserved_trial_seconds
        .min(elapsed_seconds(elapsed_ms));
    let refunded_trial_seconds = reservation
        .reserved_trial_seconds
        .saturating_sub(settled_trial_seconds);
    let charged_customer_cents = if reservation.reserved_trial_seconds > 0 {
        0
    } else {
        actual_customer_cents.min(reservation.reserved_cents)
    };
    let refunded_cents = reservation
        .reserved_cents
        .saturating_sub(charged_customer_cents);
    let balance_after = account.balance_cents.saturating_add(refunded_cents);
    let trial_after = account
        .trial_seconds_remaining
        .saturating_add(refunded_trial_seconds);

    let updated = tx
        .execute(
            "UPDATE accounts
                SET reserved_cents = reserved_cents - $1,
                    balance_cents = balance_cents + $2,
                    trial_seconds_remaining = trial_seconds_remaining + $3
              WHERE id = $4 AND reserved_cents >= $1",
            &[
                &reservation.reserved_cents,
                &refunded_cents,
                &refunded_trial_seconds,
                &account_id,
            ],
        )
        .map_err(|error| UsageReservationError::Db(error.into()))?;
    if updated != 1 {
        return Err(UsageReservationError::InvalidReservation);
    }
    balance::consume_credit_batches_pg_tx(&mut tx, account_id, charged_customer_cents)
        .map_err(UsageReservationError::Db)?;

    let updated = tx
        .execute(
            "UPDATE usage_reservations
                SET status = 'settled',
                    actual_customer_cents = $3,
                    settled_cents = $4,
                    refunded_cents = $5,
                    settled_trial_seconds = $6,
                    refunded_trial_seconds = $7,
                    settled_at_ms = $8,
                    balance_cents_after = $9,
                    trial_seconds_remaining_after = $10,
                    terminal_reason = $11
              WHERE account_id = $1 AND request_id = $2 AND status = 'reserved'",
            &[
                &account_id,
                &request_id,
                &actual_customer_cents,
                &charged_customer_cents,
                &refunded_cents,
                &settled_trial_seconds,
                &refunded_trial_seconds,
                &settled_at_ms,
                &balance_after,
                &trial_after,
                &reason,
            ],
        )
        .map_err(|error| UsageReservationError::Db(error.into()))?;
    if updated != 1 {
        return Err(UsageReservationError::AlreadySettled);
    }
    record_terminal_ledger_postgres(
        &mut tx,
        account_id,
        request_id,
        &reservation,
        "usage_settle",
        reason,
        account.balance_cents,
        balance_after,
        refunded_cents,
        actual_customer_cents,
        charged_customer_cents,
    )?;
    persist_settlement_events_postgres(
        &mut tx,
        account_id,
        request_id,
        &reservation.kind,
        components,
        charged_customer_cents,
        false,
    )?;
    tx.commit()
        .map_err(|error| UsageReservationError::Db(error.into()))?;

    Ok(SettledUsage {
        attempt: reservation.attempt,
        actual_customer_cents,
        charged_customer_cents,
        refunded_cents,
        settled_trial_seconds,
        refunded_trial_seconds,
        balance_cents_after: balance_after,
        trial_seconds_remaining: trial_after,
    })
}

fn allocated_settlement_events(
    request_id: &str,
    reservation_kind: &str,
    components: &[SettlementUsageEvent],
    charged_customer_cents: i64,
) -> Result<Vec<UsageEvent>, UsageReservationError> {
    if components.is_empty() {
        return Ok(Vec::new());
    }
    if !components.iter().any(|component| {
        component.event.request_id == request_id && component.event.kind == reservation_kind
    }) {
        return Err(UsageReservationError::InvalidReservation);
    }
    let mut identities = std::collections::BTreeSet::new();
    let mut remaining = charged_customer_cents.max(0);
    let mut events = Vec::with_capacity(components.len());
    for component in components {
        if !identities.insert((
            component.event.request_id.as_str(),
            component.event.kind.as_str(),
        )) {
            return Err(UsageReservationError::InvalidReservation);
        }
        let mut event = component.event.clone();
        event.input_tokens = event.input_tokens.clamp(0, MAX_AUTHORITATIVE_EVENT_TOKENS);
        event.output_tokens = event.output_tokens.clamp(0, MAX_AUTHORITATIVE_EVENT_TOKENS);
        event.latency_ms = event
            .latency_ms
            .clamp(0, MAX_AUTHORITATIVE_EVENT_LATENCY_MS);
        event.cost_cents_to_bluey = event
            .cost_cents_to_bluey
            .clamp(0, MAX_AUTHORITATIVE_EVENT_COST_CENTS);
        let charged = component.customer_cost_cents.min(remaining);
        event.cost_cents_to_customer = charged;
        remaining = remaining.saturating_sub(charged);
        events.push(event);
    }
    if remaining != 0 {
        return Err(UsageReservationError::InvalidReservation);
    }
    Ok(events)
}

#[allow(clippy::too_many_arguments)]
fn exact_usage_event_exists_sqlite(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    event: &UsageEvent,
) -> Result<bool, UsageReservationError> {
    tx.query_row(
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
        |row| row.get(0),
    )
    .map_err(|error| UsageReservationError::Db(error.into()))
}

fn persist_settlement_events_sqlite(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    request_id: &str,
    reservation_kind: &str,
    components: &[SettlementUsageEvent],
    charged_customer_cents: i64,
    replay: bool,
) -> Result<(), UsageReservationError> {
    for event in allocated_settlement_events(
        request_id,
        reservation_kind,
        components,
        charged_customer_cents,
    )? {
        let inserted = if replay {
            0
        } else {
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
            )
            .map_err(|error| UsageReservationError::Db(error.into()))?
        };
        if inserted != 1 && !exact_usage_event_exists_sqlite(tx, account_id, &event)? {
            return Err(UsageReservationError::Db(anyhow::anyhow!(
                "authoritative usage event replay mismatch"
            )));
        }
    }
    Ok(())
}

fn exact_usage_event_exists_postgres(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    event: &UsageEvent,
) -> Result<bool, UsageReservationError> {
    let was_speculative = i32::from(event.was_speculative);
    let was_fallback = i32::from(event.was_fallback);
    tx.query_one(
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
    )
    .map(|row| row.get(0))
    .map_err(|error| UsageReservationError::Db(error.into()))
}

fn persist_settlement_events_postgres(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    request_id: &str,
    reservation_kind: &str,
    components: &[SettlementUsageEvent],
    charged_customer_cents: i64,
    replay: bool,
) -> Result<(), UsageReservationError> {
    for event in allocated_settlement_events(
        request_id,
        reservation_kind,
        components,
        charged_customer_cents,
    )? {
        let was_speculative = i32::from(event.was_speculative);
        let was_fallback = i32::from(event.was_fallback);
        let inserted = if replay {
            0
        } else {
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
            )
            .map_err(|error| UsageReservationError::Db(error.into()))?
        };
        if inserted != 1 && !exact_usage_event_exists_postgres(tx, account_id, &event)? {
            return Err(UsageReservationError::Db(anyhow::anyhow!(
                "authoritative usage event replay mismatch"
            )));
        }
    }
    Ok(())
}

pub(crate) fn release(
    pool: &DbPool,
    account_id: &str,
    request_id: &str,
    reason: &str,
    released_at_ms: i64,
) -> Result<ReleasedUsage, UsageReservationError> {
    if account_id.trim().is_empty() || request_id.trim().is_empty() || reason.trim().is_empty() {
        return Err(UsageReservationError::InvalidReservation);
    }
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => release_sqlite(pool, account_id, request_id, reason, released_at_ms),
        DbPool::Postgres(_) => {
            release_postgres(pool, account_id, request_id, reason, released_at_ms)
        }
    })
}

fn release_sqlite(
    pool: &DbPool,
    account_id: &str,
    request_id: &str,
    reason: &str,
    released_at_ms: i64,
) -> Result<ReleasedUsage, UsageReservationError> {
    let mut conn = pool.get().map_err(UsageReservationError::Db)?;
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| UsageReservationError::Db(error.into()))?;
    let account = account_snapshot_sqlite(&tx, account_id)?;
    let reservation = load_reservation_sqlite(&tx, account_id, request_id)?;
    if reservation.status == STATUS_SETTLED {
        return Err(UsageReservationError::AlreadySettled);
    }
    if reservation.status == STATUS_RELEASED {
        return released_from_row(&reservation);
    }
    if reservation.status != STATUS_RESERVED {
        return Err(UsageReservationError::InvalidReservation);
    }

    let balance_after = account
        .balance_cents
        .saturating_add(reservation.reserved_cents);
    let trial_after = account
        .trial_seconds_remaining
        .saturating_add(reservation.reserved_trial_seconds);
    let updated = tx
        .execute(
            "UPDATE accounts
                SET reserved_cents = reserved_cents - ?1,
                    balance_cents = balance_cents + ?1,
                    trial_seconds_remaining = trial_seconds_remaining + ?2
              WHERE id = ?3 AND reserved_cents >= ?1",
            params![
                reservation.reserved_cents,
                reservation.reserved_trial_seconds,
                account_id,
            ],
        )
        .map_err(|error| UsageReservationError::Db(error.into()))?;
    if updated != 1 {
        return Err(UsageReservationError::InvalidReservation);
    }
    tx.execute(
        "UPDATE usage_reservations
            SET status = 'released',
                actual_customer_cents = 0,
                settled_cents = 0,
                refunded_cents = reserved_cents,
                settled_trial_seconds = 0,
                refunded_trial_seconds = reserved_trial_seconds,
                settled_at_ms = ?3,
                balance_cents_after = ?4,
                trial_seconds_remaining_after = ?5,
                terminal_reason = ?6
          WHERE account_id = ?1 AND request_id = ?2 AND status = 'reserved'",
        params![
            account_id,
            request_id,
            released_at_ms,
            balance_after,
            trial_after,
            reason,
        ],
    )
    .map_err(|error| UsageReservationError::Db(error.into()))?;
    tx.execute(
        "DELETE FROM request_idempotency
          WHERE account_id = ?1 AND request_id = ?2 AND status = 'in_progress'",
        params![account_id, request_id],
    )
    .map_err(|error| UsageReservationError::Db(error.into()))?;
    record_terminal_ledger_sqlite(
        &tx,
        account_id,
        request_id,
        &reservation,
        "usage_release",
        reason,
        account.balance_cents,
        balance_after,
        reservation.reserved_cents,
        0,
        0,
    )?;
    tx.commit()
        .map_err(|error| UsageReservationError::Db(error.into()))?;

    Ok(ReleasedUsage {
        attempt: reservation.attempt,
        refunded_cents: reservation.reserved_cents,
        refunded_trial_seconds: reservation.reserved_trial_seconds,
        balance_cents_after: balance_after,
        trial_seconds_remaining: trial_after,
    })
}

fn release_postgres(
    pool: &DbPool,
    account_id: &str,
    request_id: &str,
    reason: &str,
    released_at_ms: i64,
) -> Result<ReleasedUsage, UsageReservationError> {
    let mut conn = pool.get_pg().map_err(UsageReservationError::Db)?;
    let mut tx = conn
        .transaction()
        .map_err(|error| UsageReservationError::Db(error.into()))?;
    tx.query_one(
        "SELECT pg_advisory_xact_lock(hashtextextended('jobs-provider-cost-global', 0))",
        &[],
    )
    .map_err(|error| UsageReservationError::Db(error.into()))?;
    let account = account_snapshot_postgres(&mut tx, account_id)?;
    let reservation = load_reservation_postgres(&mut tx, account_id, request_id)?;
    if reservation.status == STATUS_SETTLED {
        return Err(UsageReservationError::AlreadySettled);
    }
    if reservation.status == STATUS_RELEASED {
        return released_from_row(&reservation);
    }
    if reservation.status != STATUS_RESERVED {
        return Err(UsageReservationError::InvalidReservation);
    }

    let balance_after = account
        .balance_cents
        .saturating_add(reservation.reserved_cents);
    let trial_after = account
        .trial_seconds_remaining
        .saturating_add(reservation.reserved_trial_seconds);
    let updated = tx
        .execute(
            "UPDATE accounts
                SET reserved_cents = reserved_cents - $1,
                    balance_cents = balance_cents + $1,
                    trial_seconds_remaining = trial_seconds_remaining + $2
              WHERE id = $3 AND reserved_cents >= $1",
            &[
                &reservation.reserved_cents,
                &reservation.reserved_trial_seconds,
                &account_id,
            ],
        )
        .map_err(|error| UsageReservationError::Db(error.into()))?;
    if updated != 1 {
        return Err(UsageReservationError::InvalidReservation);
    }
    tx.execute(
        "UPDATE usage_reservations
            SET status = 'released',
                actual_customer_cents = 0,
                settled_cents = 0,
                refunded_cents = reserved_cents,
                settled_trial_seconds = 0,
                refunded_trial_seconds = reserved_trial_seconds,
                settled_at_ms = $3,
                balance_cents_after = $4,
                trial_seconds_remaining_after = $5,
                terminal_reason = $6
          WHERE account_id = $1 AND request_id = $2 AND status = 'reserved'",
        &[
            &account_id,
            &request_id,
            &released_at_ms,
            &balance_after,
            &trial_after,
            &reason,
        ],
    )
    .map_err(|error| UsageReservationError::Db(error.into()))?;
    tx.execute(
        "DELETE FROM request_idempotency
          WHERE account_id = $1 AND request_id = $2 AND status = 'in_progress'",
        &[&account_id, &request_id],
    )
    .map_err(|error| UsageReservationError::Db(error.into()))?;
    record_terminal_ledger_postgres(
        &mut tx,
        account_id,
        request_id,
        &reservation,
        "usage_release",
        reason,
        account.balance_cents,
        balance_after,
        reservation.reserved_cents,
        0,
        0,
    )?;
    tx.commit()
        .map_err(|error| UsageReservationError::Db(error.into()))?;

    Ok(ReleasedUsage {
        attempt: reservation.attempt,
        refunded_cents: reservation.reserved_cents,
        refunded_trial_seconds: reservation.reserved_trial_seconds,
        balance_cents_after: balance_after,
        trial_seconds_remaining: trial_after,
    })
}

/// Refund expired reservations for an account.
///
/// This is safe to call from request admission, a periodic worker, and a
/// per-request delayed task. Settlement and release lock the same account and
/// reservation rows, so exactly one terminal transition wins. A TTL release
/// also removes only an `in_progress` idempotency row, allowing the same
/// request ID to be retried without permitting a settled request to dispatch.
pub(crate) fn reconcile_expired_for_account(
    pool: &DbPool,
    account_id: &str,
    now_ms: i64,
) -> Result<usize, UsageReservationError> {
    let request_ids = crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get().map_err(UsageReservationError::Db)?;
            let mut statement = conn
                .prepare(
                    "SELECT request_id
                       FROM usage_reservations
                      WHERE account_id = ?1
                        AND status = 'reserved'
                        AND expires_at_ms <= ?2",
                )
                .map_err(|error| UsageReservationError::Db(error.into()))?;
            let rows = statement
                .query_map(params![account_id, now_ms], |row| row.get::<_, String>(0))
                .map_err(|error| UsageReservationError::Db(error.into()))?;
            rows.collect::<Result<Vec<_>, _>>()
                .map_err(|error| UsageReservationError::Db(error.into()))
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg().map_err(UsageReservationError::Db)?;
            conn.query(
                "SELECT request_id
                   FROM usage_reservations
                  WHERE account_id = $1
                    AND status = 'reserved'
                    AND expires_at_ms <= $2",
                &[&account_id, &now_ms],
            )
            .map_err(|error| UsageReservationError::Db(error.into()))?
            .into_iter()
            .map(|row| {
                row.try_get(0)
                    .map_err(|error| UsageReservationError::Db(error.into()))
            })
            .collect::<Result<Vec<String>, UsageReservationError>>()
        }
    })?;

    let mut released = 0;
    for request_id in request_ids {
        match release(pool, account_id, &request_id, "ttl_expired", now_ms) {
            Ok(_) => released += 1,
            Err(UsageReservationError::AlreadySettled)
            | Err(UsageReservationError::AlreadyReleased)
            | Err(UsageReservationError::NotFound) => {}
            Err(error) => return Err(error),
        }
    }
    Ok(released)
}

fn elapsed_seconds(elapsed_ms: i64) -> i64 {
    if elapsed_ms <= 0 {
        1
    } else {
        ((elapsed_ms + 999) / 1_000).max(1)
    }
}

fn account_snapshot_sqlite(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
) -> Result<AccountSnapshot, UsageReservationError> {
    tx.query_row(
        "SELECT balance_cents, trial_seconds_remaining, billing_restricted
           FROM accounts
          WHERE id = ?1",
        params![account_id],
        |row| {
            Ok(AccountSnapshot {
                balance_cents: row.get(0)?,
                trial_seconds_remaining: row.get(1)?,
                billing_restricted: row.get::<_, i64>(2)? != 0,
            })
        },
    )
    .optional()
    .map_err(|error| UsageReservationError::Db(error.into()))?
    .ok_or(UsageReservationError::AccountUnavailable)
}

fn account_snapshot_postgres(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
) -> Result<AccountSnapshot, UsageReservationError> {
    let row = tx
        .query_opt(
            "SELECT balance_cents, trial_seconds_remaining, billing_restricted
               FROM accounts
              WHERE id = $1
              FOR UPDATE",
            &[&account_id],
        )
        .map_err(|error| UsageReservationError::Db(error.into()))?
        .ok_or(UsageReservationError::AccountUnavailable)?;
    Ok(AccountSnapshot {
        balance_cents: row
            .try_get(0)
            .map_err(|error| UsageReservationError::Db(error.into()))?,
        trial_seconds_remaining: row
            .try_get(1)
            .map_err(|error| UsageReservationError::Db(error.into()))?,
        billing_restricted: postgres_integer_flag(&row, 2)?,
    })
}

fn load_reservation_sqlite(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    request_id: &str,
) -> Result<ReservationRow, UsageReservationError> {
    tx.query_row(
        "SELECT kind, status, attempt,
                estimated_customer_cents, estimated_upstream_cents,
                reserved_cents, actual_customer_cents, settled_cents, refunded_cents,
                reserved_trial_seconds, settled_trial_seconds, refunded_trial_seconds,
                expires_at_ms, balance_cents_after, trial_seconds_remaining_after
           FROM usage_reservations
          WHERE account_id = ?1 AND request_id = ?2",
        params![account_id, request_id],
        reservation_row_sqlite,
    )
    .optional()
    .map_err(|error| UsageReservationError::Db(error.into()))?
    .ok_or(UsageReservationError::NotFound)
}

fn reservation_row_sqlite(row: &rusqlite::Row<'_>) -> rusqlite::Result<ReservationRow> {
    Ok(ReservationRow {
        kind: row.get(0)?,
        status: row.get(1)?,
        attempt: row.get(2)?,
        estimated_customer_cents: row.get(3)?,
        estimated_upstream_cents: row.get(4)?,
        reserved_cents: row.get(5)?,
        actual_customer_cents: row.get(6)?,
        settled_cents: row.get(7)?,
        refunded_cents: row.get(8)?,
        reserved_trial_seconds: row.get(9)?,
        settled_trial_seconds: row.get(10)?,
        refunded_trial_seconds: row.get(11)?,
        expires_at_ms: row.get(12)?,
        balance_cents_after: row.get(13)?,
        trial_seconds_remaining_after: row.get(14)?,
    })
}

fn load_reservation_postgres(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    request_id: &str,
) -> Result<ReservationRow, UsageReservationError> {
    let row = tx
        .query_opt(
            "SELECT kind, status, attempt,
                    estimated_customer_cents, estimated_upstream_cents,
                    reserved_cents, actual_customer_cents, settled_cents, refunded_cents,
                    reserved_trial_seconds, settled_trial_seconds, refunded_trial_seconds,
                    expires_at_ms, balance_cents_after, trial_seconds_remaining_after
               FROM usage_reservations
              WHERE account_id = $1 AND request_id = $2
              FOR UPDATE",
            &[&account_id, &request_id],
        )
        .map_err(|error| UsageReservationError::Db(error.into()))?
        .ok_or(UsageReservationError::NotFound)?;
    reservation_row_postgres(&row)
}

fn reservation_row_postgres(row: &postgres::Row) -> Result<ReservationRow, UsageReservationError> {
    Ok(ReservationRow {
        kind: row
            .try_get(0)
            .map_err(|error| UsageReservationError::Db(error.into()))?,
        status: row
            .try_get(1)
            .map_err(|error| UsageReservationError::Db(error.into()))?,
        attempt: row
            .try_get(2)
            .map_err(|error| UsageReservationError::Db(error.into()))?,
        estimated_customer_cents: row
            .try_get(3)
            .map_err(|error| UsageReservationError::Db(error.into()))?,
        estimated_upstream_cents: row
            .try_get(4)
            .map_err(|error| UsageReservationError::Db(error.into()))?,
        reserved_cents: row
            .try_get(5)
            .map_err(|error| UsageReservationError::Db(error.into()))?,
        actual_customer_cents: row
            .try_get(6)
            .map_err(|error| UsageReservationError::Db(error.into()))?,
        settled_cents: row
            .try_get(7)
            .map_err(|error| UsageReservationError::Db(error.into()))?,
        refunded_cents: row
            .try_get(8)
            .map_err(|error| UsageReservationError::Db(error.into()))?,
        reserved_trial_seconds: row
            .try_get(9)
            .map_err(|error| UsageReservationError::Db(error.into()))?,
        settled_trial_seconds: row
            .try_get(10)
            .map_err(|error| UsageReservationError::Db(error.into()))?,
        refunded_trial_seconds: row
            .try_get(11)
            .map_err(|error| UsageReservationError::Db(error.into()))?,
        expires_at_ms: row
            .try_get(12)
            .map_err(|error| UsageReservationError::Db(error.into()))?,
        balance_cents_after: row
            .try_get(13)
            .map_err(|error| UsageReservationError::Db(error.into()))?,
        trial_seconds_remaining_after: row
            .try_get(14)
            .map_err(|error| UsageReservationError::Db(error.into()))?,
    })
}

fn settled_from_row(row: &ReservationRow) -> Result<SettledUsage, UsageReservationError> {
    Ok(SettledUsage {
        attempt: row.attempt,
        actual_customer_cents: row.actual_customer_cents,
        charged_customer_cents: row.settled_cents,
        refunded_cents: row.refunded_cents,
        settled_trial_seconds: row.settled_trial_seconds,
        refunded_trial_seconds: row.refunded_trial_seconds,
        balance_cents_after: row
            .balance_cents_after
            .ok_or(UsageReservationError::InvalidReservation)?,
        trial_seconds_remaining: row
            .trial_seconds_remaining_after
            .ok_or(UsageReservationError::InvalidReservation)?,
    })
}

fn released_from_row(row: &ReservationRow) -> Result<ReleasedUsage, UsageReservationError> {
    Ok(ReleasedUsage {
        attempt: row.attempt,
        refunded_cents: row.refunded_cents,
        refunded_trial_seconds: row.refunded_trial_seconds,
        balance_cents_after: row
            .balance_cents_after
            .ok_or(UsageReservationError::InvalidReservation)?,
        trial_seconds_remaining: row
            .trial_seconds_remaining_after
            .ok_or(UsageReservationError::InvalidReservation)?,
    })
}

#[allow(clippy::too_many_arguments)]
fn record_terminal_ledger_sqlite(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    request_id: &str,
    reservation: &ReservationRow,
    event_type: &str,
    reason: &str,
    balance_before: i64,
    balance_after: i64,
    amount_cents: i64,
    actual_customer_cents: i64,
    charged_customer_cents: i64,
) -> Result<(), UsageReservationError> {
    if reservation.reserved_cents == 0 {
        return Ok(());
    }
    let evidence_id = format!("{}:{}:{event_type}", request_id, reservation.attempt);
    let metadata = terminal_ledger_metadata(
        reservation,
        actual_customer_cents,
        charged_customer_cents,
        amount_cents,
    );
    balance::insert_balance_ledger_sqlite_tx(
        tx,
        balance::BalanceLedgerEntry {
            account_id,
            event_type,
            amount_cents,
            balance_cents_before: balance_before,
            balance_cents_after: balance_after,
            reason: Some(reason),
            provider: None,
            processor_payment_id: None,
            source_id: Some(&evidence_id),
            idempotency_key: Some(&evidence_id),
            request_id: Some(request_id),
            metadata_json: Some(&metadata),
        },
    )
    .map_err(UsageReservationError::Db)
}

#[allow(clippy::too_many_arguments)]
fn record_terminal_ledger_postgres(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    request_id: &str,
    reservation: &ReservationRow,
    event_type: &str,
    reason: &str,
    balance_before: i64,
    balance_after: i64,
    amount_cents: i64,
    actual_customer_cents: i64,
    charged_customer_cents: i64,
) -> Result<(), UsageReservationError> {
    if reservation.reserved_cents == 0 {
        return Ok(());
    }
    let evidence_id = format!("{}:{}:{event_type}", request_id, reservation.attempt);
    let metadata = terminal_ledger_metadata(
        reservation,
        actual_customer_cents,
        charged_customer_cents,
        amount_cents,
    );
    balance::insert_balance_ledger_pg_tx(
        tx,
        balance::BalanceLedgerEntry {
            account_id,
            event_type,
            amount_cents,
            balance_cents_before: balance_before,
            balance_cents_after: balance_after,
            reason: Some(reason),
            provider: None,
            processor_payment_id: None,
            source_id: Some(&evidence_id),
            idempotency_key: Some(&evidence_id),
            request_id: Some(request_id),
            metadata_json: Some(&metadata),
        },
    )
    .map_err(UsageReservationError::Db)
}

fn terminal_ledger_metadata(
    reservation: &ReservationRow,
    actual_customer_cents: i64,
    charged_customer_cents: i64,
    refunded_cents: i64,
) -> String {
    serde_json::json!({
        "kind": reservation.kind,
        "attempt": reservation.attempt,
        "estimated_customer_cents": reservation.estimated_customer_cents,
        "estimated_upstream_cents": reservation.estimated_upstream_cents,
        "reserved_cents": reservation.reserved_cents,
        "actual_customer_cents": actual_customer_cents,
        "charged_customer_cents": charged_customer_cents,
        "refunded_cents": refunded_cents,
        "absorbed_overrun_cents": actual_customer_cents.saturating_sub(charged_customer_cents),
        "expires_at_ms": reservation.expires_at_ms,
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Barrier};

    use super::*;
    use crate::db::{self, accounts::Account, balance, idempotency};

    fn temp_pool() -> DbPool {
        let path = std::env::temp_dir().join(format!(
            "bluey-usage-reservations-{}.db",
            uuid::Uuid::new_v4()
        ));
        let pool = db::open_pool(&path).unwrap();
        db::run_migrations(&pool).unwrap();
        pool
    }

    fn create_paid_account(pool: &DbPool, email: &str, cents: i64) -> String {
        let account = Account::create(pool, email, "hash").unwrap();
        pool.get()
            .unwrap()
            .execute(
                "UPDATE accounts SET trial_seconds_remaining = 0 WHERE id = ?1",
                params![&account.id],
            )
            .unwrap();
        balance::credit_internal(pool, &account.id, cents, "usage-reservation-test").unwrap();
        account.id
    }

    fn input<'a>(
        account_id: &'a str,
        request_id: &'a str,
        estimated_customer_cents: i64,
        created_at_ms: i64,
        expires_at_ms: i64,
    ) -> ReserveUsageInput<'a> {
        ReserveUsageInput {
            account_id,
            request_id,
            kind: "llm",
            reason: "llm_test",
            estimated_customer_cents,
            estimated_upstream_cents: estimated_customer_cents / 2,
            upstream_spend_guard: None,
            created_at_ms,
            expires_at_ms,
        }
    }

    fn account_money(pool: &DbPool, account_id: &str) -> (i64, i64) {
        pool.get()
            .unwrap()
            .query_row(
                "SELECT balance_cents, reserved_cents FROM accounts WHERE id = ?1",
                params![account_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap()
    }

    #[test]
    fn concurrent_paid_reservations_cannot_overspend() {
        let pool = Arc::new(temp_pool());
        let account_id = create_paid_account(&pool, "usage-concurrency@example.com", 100);
        let barrier = Arc::new(Barrier::new(3));
        let mut workers = Vec::new();

        for request_id in ["concurrent-1", "concurrent-2"] {
            let pool = Arc::clone(&pool);
            let account_id = account_id.clone();
            let barrier = Arc::clone(&barrier);
            workers.push(std::thread::spawn(move || {
                barrier.wait();
                reserve(&pool, input(&account_id, request_id, 75, 1_000, 61_000))
            }));
        }
        barrier.wait();

        let results: Vec<_> = workers
            .into_iter()
            .map(|worker| worker.join().unwrap())
            .collect();
        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
        assert_eq!(
            results
                .iter()
                .filter(|result| matches!(result, Err(UsageReservationError::InsufficientBalance)))
                .count(),
            1
        );
        assert_eq!(account_money(&pool, &account_id), (25, 75));
    }

    #[test]
    fn anonymous_cutover_baseline_blocks_generic_reservation_until_window_and_grace_expire() {
        let pool = temp_pool();
        let account_id = create_paid_account(&pool, "usage-cutover@example.com", 100);
        pool.get()
            .unwrap()
            .execute(
                "INSERT INTO usage_cutover_spend_baseline(occurred_at, cost_cents)
                 VALUES (datetime('now'), 7)",
                [],
            )
            .unwrap();

        let now = crate::db::jobs::now_ms();
        let guard = UpstreamSpendGuard {
            limit_cents: 10,
            window_hours: 24,
        };
        let mut blocked = input(
            &account_id,
            "cutover-baseline-blocked",
            8,
            now,
            now.saturating_add(60_000),
        );
        blocked.estimated_upstream_cents = 4;
        blocked.upstream_spend_guard = Some(guard);
        assert!(matches!(
            reserve(&pool, blocked),
            Err(UsageReservationError::UpstreamSpendLimit)
        ));
        assert_eq!(account_money(&pool, &account_id), (100, 0));

        // Twenty-four-hour window plus the one-day cleanup grace has elapsed.
        // The baseline no longer contributes to admission and is physically
        // removed by the next guarded transaction.
        pool.get()
            .unwrap()
            .execute(
                "UPDATE usage_cutover_spend_baseline
                    SET occurred_at = datetime('now', '-3 days')",
                [],
            )
            .unwrap();
        let mut admitted = input(
            &account_id,
            "cutover-baseline-expired",
            8,
            now.saturating_add(1),
            now.saturating_add(60_001),
        );
        admitted.estimated_upstream_cents = 4;
        admitted.upstream_spend_guard = Some(guard);
        reserve(&pool, admitted).unwrap();
        assert_eq!(account_money(&pool, &account_id), (92, 8));
        assert_eq!(
            pool.get()
                .unwrap()
                .query_row(
                    "SELECT COUNT(*) FROM usage_cutover_spend_baseline",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
            0
        );
    }

    #[test]
    fn trial_reservation_admits_only_one_concurrent_request() {
        let pool = temp_pool();
        let account = Account::create(&pool, "usage-trial@example.com", "hash").unwrap();
        let initial_trial = account.trial_seconds_remaining;
        balance::credit_internal(&pool, &account.id, 100, "trial-serialization-test").unwrap();

        let first = reserve(&pool, input(&account.id, "trial-1", 50, 1_000, 61_000)).unwrap();
        assert_eq!(first.reserved_trial_seconds, initial_trial);
        assert!(first.is_trial());

        let second = reserve(&pool, input(&account.id, "trial-2", 50, 1_000, 61_000)).unwrap_err();
        assert!(matches!(second, UsageReservationError::InProgress));
        release(&pool, &account.id, "trial-1", "test_release", 2_000).unwrap();

        let retried = reserve(&pool, input(&account.id, "trial-2", 50, 3_000, 63_000)).unwrap();
        assert_eq!(retried.reserved_trial_seconds, initial_trial);
    }

    #[test]
    fn released_request_id_can_retry_and_settlement_is_idempotent() {
        let pool = temp_pool();
        let account_id = create_paid_account(&pool, "usage-retry@example.com", 100);

        let first = reserve(&pool, input(&account_id, "retry-1", 60, 1_000, 61_000)).unwrap();
        assert_eq!(first.attempt, 1);
        assert_eq!(account_money(&pool, &account_id), (40, 60));

        let released = release(&pool, &account_id, "retry-1", "upstream_error", 2_000).unwrap();
        assert_eq!(released.refunded_cents, 60);
        assert_eq!(account_money(&pool, &account_id), (100, 0));

        let second = reserve(&pool, input(&account_id, "retry-1", 60, 3_000, 63_000)).unwrap();
        assert_eq!(second.attempt, 2);
        assert_eq!(account_money(&pool, &account_id), (40, 60));

        let settled = settle(&pool, &account_id, "retry-1", 20, 1_500, "completed", 5_000).unwrap();
        assert_eq!(settled.charged_customer_cents, 20);
        assert_eq!(settled.refunded_cents, 40);
        assert_eq!(account_money(&pool, &account_id), (80, 0));

        let replay = settle(&pool, &account_id, "retry-1", 20, 1_500, "completed", 6_000).unwrap();
        assert_eq!(replay, settled);
        assert_eq!(account_money(&pool, &account_id), (80, 0));

        let batch_remaining: i64 = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT remaining_cents FROM credit_batches WHERE account_id = ?1",
                params![account_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(batch_remaining, 80);
    }

    #[test]
    fn ttl_reconciliation_refunds_and_reopens_request_id() {
        let pool = temp_pool();
        let account_id = create_paid_account(&pool, "usage-ttl@example.com", 100);
        assert_eq!(
            idempotency::reserve(&pool, &account_id, "ttl-1").unwrap(),
            idempotency::ReserveOutcome::FreshReservation
        );
        reserve(&pool, input(&account_id, "ttl-1", 70, 1_000, 2_000)).unwrap();
        assert_eq!(account_money(&pool, &account_id), (30, 70));

        assert_eq!(
            reconcile_expired_for_account(&pool, &account_id, 1_999).unwrap(),
            0
        );
        assert_eq!(
            reconcile_expired_for_account(&pool, &account_id, 2_000).unwrap(),
            1
        );
        assert_eq!(
            reconcile_expired_for_account(&pool, &account_id, 2_000).unwrap(),
            0
        );
        assert_eq!(account_money(&pool, &account_id), (100, 0));
        assert_eq!(
            idempotency::reserve(&pool, &account_id, "ttl-1").unwrap(),
            idempotency::ReserveOutcome::FreshReservation
        );

        let (status, reason): (String, String) = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT status, terminal_reason
                   FROM usage_reservations
                  WHERE account_id = ?1 AND request_id = ?2",
                params![account_id, "ttl-1"],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(status, STATUS_RELEASED);
        assert_eq!(reason, "ttl_expired");
    }
}
