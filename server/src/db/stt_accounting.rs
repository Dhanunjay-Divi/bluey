//! STT reservation accounting.
//!
//! Live captions can open two relay sessions at once (mic + system). This
//! module keeps the money/trial reservation state out of the websocket relay
//! code so each source has to reserve its worst-case budget before any provider
//! audio stream starts.

use anyhow::Result;
use rusqlite::{params, OptionalExtension};

use crate::{
    db::{balance, DbPool},
    pricing,
};

#[derive(Debug, Clone)]
pub(crate) struct ReserveSessionInput<'a> {
    pub account_id: &'a str,
    pub bluey_session_id: &'a str,
    pub provider: &'a str,
    pub model: &'a str,
    pub source: &'a str,
    pub mode: &'a str,
    pub token: &'a str,
    pub max_seconds: i64,
    pub created_at_ms: i64,
    pub expires_at_ms: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ReservedSttSession {
    pub reserved_cents: i64,
    pub reserved_trial_seconds: i64,
    pub reserved_billable_seconds: i64,
    pub projected_bluey_cents: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SettledSttSession {
    pub elapsed_ms: i64,
    pub elapsed_seconds: i64,
    pub billable_seconds: i64,
    pub trial_seconds: i64,
    pub customer_cents: i64,
    pub bluey_cents: i64,
    pub refunded_cents: i64,
    pub refunded_trial_seconds: i64,
}

#[derive(Debug, Clone)]
pub(crate) struct ClaimedSttSession {
    pub token: String,
    pub account_id: String,
    pub bluey_session_id: String,
    pub provider: String,
    pub model: String,
    pub source: String,
    pub max_seconds: i64,
    pub expires_at_ms: i64,
    pub reserved_cents: i64,
    pub reserved_trial_seconds: i64,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum SttAccountingError {
    #[error("unsupported Deepgram STT model")]
    UnsupportedModel,
    #[error("balance is required before starting this STT session")]
    InsufficientBalance,
    #[error("STT session was already settled")]
    AlreadySettled,
    #[error(transparent)]
    Db(#[from] anyhow::Error),
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum ClaimSttSessionError {
    #[error("invalid STT session")]
    InvalidSession,
    #[error("STT session expired")]
    Expired,
    #[error("only deepgram STT relay is enabled")]
    UnsupportedProvider,
    #[error("STT session is already active or closed")]
    AlreadyActiveOrClosed,
    #[error(transparent)]
    Db(#[from] anyhow::Error),
}

pub(crate) fn reserve_session(
    pool: &DbPool,
    input: ReserveSessionInput<'_>,
) -> Result<ReservedSttSession, SttAccountingError> {
    crate::db::run_blocking_db(|| {
        let pricing = pricing::lookup(input.provider, input.model)
            .ok_or(SttAccountingError::UnsupportedModel)?;
        let (
            reserved_trial_seconds,
            reserved_billable_seconds,
            projected_bluey_cents,
            reserved_cents,
        ) = match pool {
            DbPool::Sqlite(_) => {
                let mut conn = pool.get().map_err(SttAccountingError::Db)?;
                let tx = conn
                    .transaction()
                    .map_err(|err| SttAccountingError::Db(err.into()))?;

                let (available_cents, trial_remaining): (i64, i64) = tx
                    .query_row(
                        "SELECT balance_cents, trial_seconds_remaining
                         FROM accounts
                         WHERE id = ?1",
                        params![input.account_id],
                        |row| Ok((row.get(0)?, row.get(1)?)),
                    )
                    .map_err(|err| SttAccountingError::Db(err.into()))?;

                let reserved_trial_seconds = trial_remaining.max(0).min(input.max_seconds);
                let reserved_billable_seconds = (input.max_seconds - reserved_trial_seconds).max(0);
                let (projected_bluey_cents, reserved_cents) =
                    pricing::compute_cost(pricing, reserved_billable_seconds, 0);
                if available_cents < reserved_cents {
                    return Err(SttAccountingError::InsufficientBalance);
                }

                let updated = tx
                    .execute(
                        "UPDATE accounts
                        SET balance_cents = balance_cents - ?1,
                            reserved_cents = reserved_cents + ?1,
                            trial_seconds_remaining = trial_seconds_remaining - ?2
                      WHERE id = ?3
                        AND balance_cents >= ?1
                        AND trial_seconds_remaining >= ?2",
                        params![reserved_cents, reserved_trial_seconds, input.account_id],
                    )
                    .map_err(|err| SttAccountingError::Db(err.into()))?;
                if updated != 1 {
                    return Err(SttAccountingError::InsufficientBalance);
                }

                if reserved_cents > 0 {
                    let metadata = serde_json::json!({
                        "bluey_session_id": input.bluey_session_id,
                        "source": input.source,
                        "mode": input.mode,
                        "max_seconds": input.max_seconds,
                        "reserved_billable_seconds": reserved_billable_seconds,
                        "reserved_trial_seconds": reserved_trial_seconds
                    })
                    .to_string();
                    balance::insert_balance_ledger_sqlite_tx(
                        &tx,
                        balance::BalanceLedgerEntry {
                            account_id: input.account_id,
                            event_type: "stt_reserve",
                            amount_cents: -reserved_cents,
                            balance_cents_before: available_cents,
                            balance_cents_after: available_cents - reserved_cents,
                            reason: Some(input.source),
                            provider: Some(input.provider),
                            processor_payment_id: None,
                            source_id: Some(input.token),
                            idempotency_key: Some(input.token),
                            request_id: Some(input.bluey_session_id),
                            metadata_json: Some(&metadata),
                        },
                    )
                    .map_err(SttAccountingError::Db)?;
                }

                tx.execute(
                    "INSERT INTO stt_sessions (
                        session_token, account_id, bluey_session_id, provider, model, source,
                        mode, max_seconds, created_at_ms, expires_at_ms,
                        reserved_cents, reserved_trial_seconds
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                    params![
                        input.token,
                        input.account_id,
                        input.bluey_session_id,
                        input.provider,
                        input.model,
                        input.source,
                        input.mode,
                        input.max_seconds,
                        input.created_at_ms,
                        input.expires_at_ms,
                        reserved_cents,
                        reserved_trial_seconds,
                    ],
                )
                .map_err(|err| SttAccountingError::Db(err.into()))?;

                tx.commit()
                    .map_err(|err| SttAccountingError::Db(err.into()))?;
                (
                    reserved_trial_seconds,
                    reserved_billable_seconds,
                    projected_bluey_cents,
                    reserved_cents,
                )
            }
            DbPool::Postgres(_) => {
                let mut conn = pool.get_pg().map_err(SttAccountingError::Db)?;
                let mut tx = conn
                    .transaction()
                    .map_err(|err| SttAccountingError::Db(err.into()))?;

                let row = tx
                    .query_one(
                        "SELECT balance_cents, trial_seconds_remaining
                         FROM accounts
                         WHERE id = $1",
                        &[&input.account_id],
                    )
                    .map_err(|err| SttAccountingError::Db(err.into()))?;
                let available_cents: i64 = row
                    .try_get(0)
                    .map_err(|err| SttAccountingError::Db(err.into()))?;
                let trial_remaining: i64 = row
                    .try_get(1)
                    .map_err(|err| SttAccountingError::Db(err.into()))?;

                let reserved_trial_seconds = trial_remaining.max(0).min(input.max_seconds);
                let reserved_billable_seconds = (input.max_seconds - reserved_trial_seconds).max(0);
                let (projected_bluey_cents, reserved_cents) =
                    pricing::compute_cost(pricing, reserved_billable_seconds, 0);
                if available_cents < reserved_cents {
                    return Err(SttAccountingError::InsufficientBalance);
                }

                let updated = tx
                    .execute(
                        "UPDATE accounts
                            SET balance_cents = balance_cents - $1,
                                reserved_cents = reserved_cents + $1,
                                trial_seconds_remaining = trial_seconds_remaining - $2
                          WHERE id = $3
                            AND balance_cents >= $1
                            AND trial_seconds_remaining >= $2",
                        &[&reserved_cents, &reserved_trial_seconds, &input.account_id],
                    )
                    .map_err(|err| SttAccountingError::Db(err.into()))?;
                if updated != 1 {
                    return Err(SttAccountingError::InsufficientBalance);
                }

                if reserved_cents > 0 {
                    let metadata = serde_json::json!({
                        "bluey_session_id": input.bluey_session_id,
                        "source": input.source,
                        "mode": input.mode,
                        "max_seconds": input.max_seconds,
                        "reserved_billable_seconds": reserved_billable_seconds,
                        "reserved_trial_seconds": reserved_trial_seconds
                    })
                    .to_string();
                    balance::insert_balance_ledger_pg_tx(
                        &mut tx,
                        balance::BalanceLedgerEntry {
                            account_id: input.account_id,
                            event_type: "stt_reserve",
                            amount_cents: -reserved_cents,
                            balance_cents_before: available_cents,
                            balance_cents_after: available_cents - reserved_cents,
                            reason: Some(input.source),
                            provider: Some(input.provider),
                            processor_payment_id: None,
                            source_id: Some(input.token),
                            idempotency_key: Some(input.token),
                            request_id: Some(input.bluey_session_id),
                            metadata_json: Some(&metadata),
                        },
                    )
                    .map_err(SttAccountingError::Db)?;
                }

                tx.execute(
                    "INSERT INTO stt_sessions (
                        session_token, account_id, bluey_session_id, provider, model, source,
                        mode, max_seconds, created_at_ms, expires_at_ms,
                        reserved_cents, reserved_trial_seconds
                     ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)",
                    &[
                        &input.token,
                        &input.account_id,
                        &input.bluey_session_id,
                        &input.provider,
                        &input.model,
                        &input.source,
                        &input.mode,
                        &input.max_seconds,
                        &input.created_at_ms,
                        &input.expires_at_ms,
                        &reserved_cents,
                        &reserved_trial_seconds,
                    ],
                )
                .map_err(|err| SttAccountingError::Db(err.into()))?;

                tx.commit()
                    .map_err(|err| SttAccountingError::Db(err.into()))?;
                (
                    reserved_trial_seconds,
                    reserved_billable_seconds,
                    projected_bluey_cents,
                    reserved_cents,
                )
            }
        };
        Ok(ReservedSttSession {
            reserved_cents,
            reserved_trial_seconds,
            reserved_billable_seconds,
            projected_bluey_cents,
        })
    })
}

pub(crate) fn claim_relay_session(
    pool: &DbPool,
    account_id: &str,
    token: &str,
    now_ms: i64,
) -> Result<ClaimedSttSession, ClaimSttSessionError> {
    crate::db::run_blocking_db(|| {
        let session = match pool {
            DbPool::Sqlite(_) => {
                let conn = pool.get().map_err(ClaimSttSessionError::Db)?;
                conn.query_row(
                "SELECT account_id, bluey_session_id, provider, model, source, max_seconds, expires_at_ms,
                        reserved_cents, reserved_trial_seconds
                 FROM stt_sessions
                 WHERE session_token = ?1 AND account_id = ?2",
                params![token, account_id],
                |row| {
                    Ok(ClaimedSttSession {
                        token: token.to_string(),
                        account_id: row.get(0)?,
                        bluey_session_id: row.get(1)?,
                        provider: row.get(2)?,
                        model: row.get(3)?,
                        source: row.get(4)?,
                        max_seconds: row.get(5)?,
                        expires_at_ms: row.get(6)?,
                        reserved_cents: row.get(7)?,
                        reserved_trial_seconds: row.get(8)?,
                    })
                },
            )
            .optional()
            .map_err(|err| ClaimSttSessionError::Db(err.into()))?
            .ok_or(ClaimSttSessionError::InvalidSession)?
            }
            DbPool::Postgres(_) => {
                let mut conn = pool.get_pg().map_err(ClaimSttSessionError::Db)?;
                let row = conn
                .query_opt(
                    "SELECT account_id, bluey_session_id, provider, model, source, max_seconds, expires_at_ms,
                            reserved_cents, reserved_trial_seconds
                     FROM stt_sessions
                     WHERE session_token = $1 AND account_id = $2",
                    &[&token, &account_id],
                )
                .map_err(|err| ClaimSttSessionError::Db(err.into()))?
                .ok_or(ClaimSttSessionError::InvalidSession)?;
                ClaimedSttSession {
                    token: token.to_string(),
                    account_id: row
                        .try_get(0)
                        .map_err(|err| ClaimSttSessionError::Db(err.into()))?,
                    bluey_session_id: row
                        .try_get(1)
                        .map_err(|err| ClaimSttSessionError::Db(err.into()))?,
                    provider: row
                        .try_get(2)
                        .map_err(|err| ClaimSttSessionError::Db(err.into()))?,
                    model: row
                        .try_get(3)
                        .map_err(|err| ClaimSttSessionError::Db(err.into()))?,
                    source: row
                        .try_get(4)
                        .map_err(|err| ClaimSttSessionError::Db(err.into()))?,
                    max_seconds: row
                        .try_get(5)
                        .map_err(|err| ClaimSttSessionError::Db(err.into()))?,
                    expires_at_ms: row
                        .try_get(6)
                        .map_err(|err| ClaimSttSessionError::Db(err.into()))?,
                    reserved_cents: row
                        .try_get(7)
                        .map_err(|err| ClaimSttSessionError::Db(err.into()))?,
                    reserved_trial_seconds: row
                        .try_get(8)
                        .map_err(|err| ClaimSttSessionError::Db(err.into()))?,
                }
            }
        };
        if session.expires_at_ms <= now_ms {
            return Err(ClaimSttSessionError::Expired);
        }
        if session.provider != "deepgram" {
            return Err(ClaimSttSessionError::UnsupportedProvider);
        }
        let updated = match pool {
            DbPool::Sqlite(_) => {
                let conn = pool.get().map_err(ClaimSttSessionError::Db)?;
                conn.execute(
                    "UPDATE stt_sessions
                    SET started_at_ms = ?1
                  WHERE session_token = ?2
                    AND account_id = ?3
                    AND started_at_ms IS NULL
                    AND ended_at_ms IS NULL
                    AND expires_at_ms > ?1",
                    params![now_ms, token, account_id],
                )
                .map_err(|err| ClaimSttSessionError::Db(err.into()))?
            }
            DbPool::Postgres(_) => {
                let mut conn = pool.get_pg().map_err(ClaimSttSessionError::Db)?;
                conn.execute(
                    "UPDATE stt_sessions
                    SET started_at_ms = $1
                  WHERE session_token = $2
                    AND account_id = $3
                    AND started_at_ms IS NULL
                    AND ended_at_ms IS NULL
                    AND expires_at_ms > $1",
                    &[&now_ms, &token, &account_id],
                )
                .map_err(|err| ClaimSttSessionError::Db(err.into()))? as usize
            }
        };
        if updated != 1 {
            return Err(ClaimSttSessionError::AlreadyActiveOrClosed);
        }
        Ok(session)
    })
}

pub(crate) fn settle_session(
    pool: &DbPool,
    session_token: &str,
    account_id: &str,
    model: &str,
    elapsed_ms: i64,
    reason: &str,
    now_ms: i64,
) -> Result<SettledSttSession, SttAccountingError> {
    crate::db::run_blocking_db(|| {
        let pricing =
            pricing::lookup("deepgram", model).ok_or(SttAccountingError::UnsupportedModel)?;
        let elapsed_seconds = ((elapsed_ms.max(1) + 999) / 1000).max(1);
        let (
            capped_seconds,
            billable_seconds,
            trial_seconds,
            customer_cents,
            bluey_cents,
            refunded_cents,
            refunded_trial_seconds,
        ) = match pool {
            DbPool::Sqlite(_) => {
                let mut conn = pool.get().map_err(SttAccountingError::Db)?;
                let tx = conn
                    .transaction()
                    .map_err(|err| SttAccountingError::Db(err.into()))?;

                let (
                        max_seconds,
                        reserved_cents,
                        reserved_trial_seconds,
                        ended_at_ms,
                        bluey_session_id,
                    ): (i64, i64, i64, Option<i64>, String) = tx
                    .query_row(
                        "SELECT max_seconds, reserved_cents, reserved_trial_seconds, ended_at_ms, bluey_session_id
                         FROM stt_sessions
                         WHERE session_token = ?1 AND account_id = ?2",
                        params![session_token, account_id],
                        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
                    )
                    .optional()
                    .map_err(|err| SttAccountingError::Db(err.into()))?
                    .ok_or_else(|| SttAccountingError::Db(anyhow::anyhow!("missing STT session")))?;

                if ended_at_ms.is_some() {
                    return Err(SttAccountingError::AlreadySettled);
                }

                let capped_seconds = elapsed_seconds.min(max_seconds.max(1));
                let trial_seconds = reserved_trial_seconds.min(capped_seconds);
                let billable_seconds = capped_seconds - trial_seconds;
                let (bluey_cents, customer_cents) =
                    pricing::compute_cost(pricing, billable_seconds, 0);
                let refunded_cents = (reserved_cents - customer_cents).max(0);
                let extra_cents = (customer_cents - reserved_cents).max(0);
                let refunded_trial_seconds = (reserved_trial_seconds - trial_seconds).max(0);
                let balance_before: i64 = tx
                    .query_row(
                        "SELECT balance_cents FROM accounts WHERE id = ?1",
                        params![account_id],
                        |row| row.get(0),
                    )
                    .map_err(|err| SttAccountingError::Db(err.into()))?;

                let updated = tx
                    .execute(
                        "UPDATE accounts
                            SET reserved_cents = reserved_cents - ?1,
                                balance_cents = balance_cents + ?2 - ?3,
                                trial_seconds_remaining = trial_seconds_remaining + ?4
                          WHERE id = ?5
                            AND reserved_cents >= ?1
                            AND balance_cents >= ?3",
                        params![
                            reserved_cents,
                            refunded_cents,
                            extra_cents,
                            refunded_trial_seconds,
                            account_id
                        ],
                    )
                    .map_err(|err| SttAccountingError::Db(err.into()))?;
                if updated != 1 {
                    return Err(SttAccountingError::InsufficientBalance);
                }

                balance::consume_credit_batches_tx(&tx, account_id, customer_cents)
                    .map_err(SttAccountingError::Db)?;
                let balance_after: i64 = tx
                    .query_row(
                        "SELECT balance_cents FROM accounts WHERE id = ?1",
                        params![account_id],
                        |row| row.get(0),
                    )
                    .map_err(|err| SttAccountingError::Db(err.into()))?;
                if balance_after != balance_before {
                    let metadata = serde_json::json!({
                        "session_token": session_token,
                        "bluey_session_id": bluey_session_id,
                        "reason": reason,
                        "elapsed_ms": elapsed_ms,
                        "capped_seconds": capped_seconds,
                        "customer_cents": customer_cents,
                        "refunded_cents": refunded_cents,
                        "extra_cents": extra_cents,
                        "trial_seconds": trial_seconds
                    })
                    .to_string();
                    balance::insert_balance_ledger_sqlite_tx(
                        &tx,
                        balance::BalanceLedgerEntry {
                            account_id,
                            event_type: "stt_settle",
                            amount_cents: balance_after - balance_before,
                            balance_cents_before: balance_before,
                            balance_cents_after: balance_after,
                            reason: Some(reason),
                            provider: Some("deepgram"),
                            processor_payment_id: None,
                            source_id: Some(session_token),
                            idempotency_key: Some(session_token),
                            request_id: Some(&bluey_session_id),
                            metadata_json: Some(&metadata),
                        },
                    )
                    .map_err(SttAccountingError::Db)?;
                }

                let updated = tx
                    .execute(
                        "UPDATE stt_sessions
                            SET consumed_seconds = ?1,
                                settled_cents = ?2,
                                refunded_cents = ?3,
                                settled_trial_seconds = ?4,
                                refunded_trial_seconds = ?5,
                                ended_at_ms = ?6,
                                relay_close_reason = ?7
                          WHERE session_token = ?8
                            AND account_id = ?9
                            AND ended_at_ms IS NULL",
                        params![
                            capped_seconds,
                            customer_cents,
                            refunded_cents,
                            trial_seconds,
                            refunded_trial_seconds,
                            now_ms,
                            reason,
                            session_token,
                            account_id
                        ],
                    )
                    .map_err(|err| SttAccountingError::Db(err.into()))?;
                if updated != 1 {
                    return Err(SttAccountingError::AlreadySettled);
                }

                tx.commit()
                    .map_err(|err| SttAccountingError::Db(err.into()))?;
                (
                    capped_seconds,
                    billable_seconds,
                    trial_seconds,
                    customer_cents,
                    bluey_cents,
                    refunded_cents,
                    refunded_trial_seconds,
                )
            }
            DbPool::Postgres(_) => {
                let mut conn = pool.get_pg().map_err(SttAccountingError::Db)?;
                let mut tx = conn
                    .transaction()
                    .map_err(|err| SttAccountingError::Db(err.into()))?;

                let row = tx
                    .query_opt(
                        "SELECT max_seconds, reserved_cents, reserved_trial_seconds, ended_at_ms, bluey_session_id
                         FROM stt_sessions
                         WHERE session_token = $1 AND account_id = $2",
                        &[&session_token, &account_id],
                    )
                    .map_err(|err| SttAccountingError::Db(err.into()))?
                    .ok_or_else(|| SttAccountingError::Db(anyhow::anyhow!("missing STT session")))?;
                let max_seconds: i64 = row
                    .try_get(0)
                    .map_err(|err| SttAccountingError::Db(err.into()))?;
                let reserved_cents: i64 = row
                    .try_get(1)
                    .map_err(|err| SttAccountingError::Db(err.into()))?;
                let reserved_trial_seconds: i64 = row
                    .try_get(2)
                    .map_err(|err| SttAccountingError::Db(err.into()))?;
                let ended_at_ms: Option<i64> = row
                    .try_get(3)
                    .map_err(|err| SttAccountingError::Db(err.into()))?;
                let bluey_session_id: String = row
                    .try_get(4)
                    .map_err(|err| SttAccountingError::Db(err.into()))?;

                if ended_at_ms.is_some() {
                    return Err(SttAccountingError::AlreadySettled);
                }

                let capped_seconds = elapsed_seconds.min(max_seconds.max(1));
                let trial_seconds = reserved_trial_seconds.min(capped_seconds);
                let billable_seconds = capped_seconds - trial_seconds;
                let (bluey_cents, customer_cents) =
                    pricing::compute_cost(pricing, billable_seconds, 0);
                let refunded_cents = (reserved_cents - customer_cents).max(0);
                let extra_cents = (customer_cents - reserved_cents).max(0);
                let refunded_trial_seconds = (reserved_trial_seconds - trial_seconds).max(0);
                let balance_before: i64 = tx
                    .query_one(
                        "SELECT balance_cents FROM accounts WHERE id = $1",
                        &[&account_id],
                    )
                    .map_err(|err| SttAccountingError::Db(err.into()))?
                    .try_get(0)
                    .map_err(|err| SttAccountingError::Db(err.into()))?;

                let updated = tx
                    .execute(
                        "UPDATE accounts
                            SET reserved_cents = reserved_cents - $1,
                                balance_cents = balance_cents + $2 - $3,
                                trial_seconds_remaining = trial_seconds_remaining + $4
                          WHERE id = $5
                            AND reserved_cents >= $1
                            AND balance_cents >= $3",
                        &[
                            &reserved_cents,
                            &refunded_cents,
                            &extra_cents,
                            &refunded_trial_seconds,
                            &account_id,
                        ],
                    )
                    .map_err(|err| SttAccountingError::Db(err.into()))?;
                if updated != 1 {
                    return Err(SttAccountingError::InsufficientBalance);
                }

                balance::consume_credit_batches_pg_tx(&mut tx, account_id, customer_cents)
                    .map_err(SttAccountingError::Db)?;
                let balance_after: i64 = tx
                    .query_one(
                        "SELECT balance_cents FROM accounts WHERE id = $1",
                        &[&account_id],
                    )
                    .map_err(|err| SttAccountingError::Db(err.into()))?
                    .try_get(0)
                    .map_err(|err| SttAccountingError::Db(err.into()))?;
                if balance_after != balance_before {
                    let metadata = serde_json::json!({
                        "session_token": session_token,
                        "bluey_session_id": bluey_session_id,
                        "reason": reason,
                        "elapsed_ms": elapsed_ms,
                        "capped_seconds": capped_seconds,
                        "customer_cents": customer_cents,
                        "refunded_cents": refunded_cents,
                        "extra_cents": extra_cents,
                        "trial_seconds": trial_seconds
                    })
                    .to_string();
                    balance::insert_balance_ledger_pg_tx(
                        &mut tx,
                        balance::BalanceLedgerEntry {
                            account_id,
                            event_type: "stt_settle",
                            amount_cents: balance_after - balance_before,
                            balance_cents_before: balance_before,
                            balance_cents_after: balance_after,
                            reason: Some(reason),
                            provider: Some("deepgram"),
                            processor_payment_id: None,
                            source_id: Some(session_token),
                            idempotency_key: Some(session_token),
                            request_id: Some(&bluey_session_id),
                            metadata_json: Some(&metadata),
                        },
                    )
                    .map_err(SttAccountingError::Db)?;
                }

                let updated = tx
                    .execute(
                        "UPDATE stt_sessions
                            SET consumed_seconds = $1,
                                settled_cents = $2,
                                refunded_cents = $3,
                                settled_trial_seconds = $4,
                                refunded_trial_seconds = $5,
                                ended_at_ms = $6,
                                relay_close_reason = $7
                          WHERE session_token = $8
                            AND account_id = $9
                            AND ended_at_ms IS NULL",
                        &[
                            &capped_seconds,
                            &customer_cents,
                            &refunded_cents,
                            &trial_seconds,
                            &refunded_trial_seconds,
                            &now_ms,
                            &reason,
                            &session_token,
                            &account_id,
                        ],
                    )
                    .map_err(|err| SttAccountingError::Db(err.into()))?;
                if updated != 1 {
                    return Err(SttAccountingError::AlreadySettled);
                }

                tx.commit()
                    .map_err(|err| SttAccountingError::Db(err.into()))?;
                (
                    capped_seconds,
                    billable_seconds,
                    trial_seconds,
                    customer_cents,
                    bluey_cents,
                    refunded_cents,
                    refunded_trial_seconds,
                )
            }
        };

        Ok(SettledSttSession {
            elapsed_ms,
            elapsed_seconds: capped_seconds,
            billable_seconds,
            trial_seconds,
            customer_cents,
            bluey_cents,
            refunded_cents,
            refunded_trial_seconds,
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{self, accounts::Account, balance, DbPool};

    fn temp_pool() -> DbPool {
        let path = std::env::temp_dir().join(format!("bluey-stt-res-{}.db", uuid::Uuid::new_v4()));
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
        balance::credit_internal(pool, &account.id, cents, "stt-accounting-test").unwrap();
        account.id
    }

    fn input<'a>(account_id: &'a str, token: &'a str, max_seconds: i64) -> ReserveSessionInput<'a> {
        ReserveSessionInput {
            account_id,
            bluey_session_id: "bluey-session",
            provider: "deepgram",
            model: "nova-3",
            source: "microphone",
            mode: "server_relay",
            token,
            max_seconds,
            created_at_ms: 1_000,
            expires_at_ms: 61_000,
        }
    }

    #[test]
    fn claimed_relay_session_is_single_use() {
        let pool = temp_pool();
        let account = Account::create(&pool, "relay-single-use@example.com", "hash").unwrap();
        reserve_session(&pool, input(&account.id, "single-use-token", 60)).unwrap();

        assert!(claim_relay_session(&pool, &account.id, "single-use-token", 2_000).is_ok());
        let second =
            claim_relay_session(&pool, &account.id, "single-use-token", 2_000).unwrap_err();
        assert!(matches!(
            second,
            ClaimSttSessionError::AlreadyActiveOrClosed
        ));
    }

    #[test]
    fn claimed_relay_session_requires_matching_account() {
        let pool = temp_pool();
        let account = Account::create(&pool, "relay-account-owner@example.com", "hash").unwrap();
        reserve_session(&pool, input(&account.id, "account-bound-token", 60)).unwrap();

        let err =
            claim_relay_session(&pool, "other-account", "account-bound-token", 2_000).unwrap_err();
        assert!(matches!(err, ClaimSttSessionError::InvalidSession));
    }

    #[test]
    fn reserving_one_stt_source_blocks_second_when_balance_only_covers_one() {
        let pool = temp_pool();
        let account_id = create_paid_account(&pool, "one-source@example.com", 28);
        let first = reserve_session(&pool, input(&account_id, "stt-1", 600)).unwrap();
        assert_eq!(first.reserved_cents, 28);

        let second = reserve_session(&pool, input(&account_id, "stt-2", 600)).unwrap_err();
        assert!(matches!(second, SttAccountingError::InsufficientBalance));

        let conn = pool.get().unwrap();
        let (balance_cents, reserved_cents): (i64, i64) = conn
            .query_row(
                "SELECT balance_cents, reserved_cents FROM accounts WHERE id = ?1",
                params![account_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(balance_cents, 0);
        assert_eq!(reserved_cents, 28);
    }

    #[test]
    fn settlement_refunds_unused_reserved_credit_and_consumes_fifo_actual() {
        let pool = temp_pool();
        let account_id = create_paid_account(&pool, "refund@example.com", 1000);
        let reserved = reserve_session(&pool, input(&account_id, "stt-refund", 600)).unwrap();
        assert_eq!(reserved.reserved_cents, 28);

        let settled = settle_session(
            &pool,
            "stt-refund",
            &account_id,
            "nova-3",
            60_000,
            "completed",
            70_000,
        )
        .unwrap();
        assert_eq!(settled.customer_cents, 3);
        assert_eq!(settled.refunded_cents, 25);

        let conn = pool.get().unwrap();
        let (balance_cents, reserved_cents): (i64, i64) = conn
            .query_row(
                "SELECT balance_cents, reserved_cents FROM accounts WHERE id = ?1",
                params![account_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        let batch_remaining: i64 = conn
            .query_row(
                "SELECT remaining_cents FROM credit_batches WHERE account_id = ?1",
                params![account_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(balance_cents, 997);
        assert_eq!(reserved_cents, 0);
        assert_eq!(batch_remaining, 997);

        let ledger: Vec<(String, i64, i64, i64)> = conn
            .prepare(
                "SELECT event_type, amount_cents, balance_cents_before, balance_cents_after
                   FROM balance_ledger_entries
                  WHERE account_id = ?1 AND event_type LIKE 'stt_%'
                  ORDER BY created_at ASC",
            )
            .unwrap()
            .query_map(params![account_id], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
            })
            .unwrap()
            .map(|row| row.unwrap())
            .collect();
        assert_eq!(
            ledger,
            vec![
                ("stt_reserve".to_string(), -28, 1000, 972),
                ("stt_settle".to_string(), 25, 972, 997),
            ]
        );
    }

    #[test]
    fn trial_seconds_are_reserved_and_unused_trial_is_restored() {
        let pool = temp_pool();
        let account = Account::create(&pool, "trial-stt@example.com", "hash").unwrap();
        let reserved = reserve_session(&pool, input(&account.id, "stt-trial", 60)).unwrap();
        assert_eq!(reserved.reserved_trial_seconds, 60);
        assert_eq!(reserved.reserved_cents, 0);

        let settled = settle_session(
            &pool,
            "stt-trial",
            &account.id,
            "nova-3",
            10_000,
            "completed",
            20_000,
        )
        .unwrap();
        assert_eq!(settled.trial_seconds, 10);
        assert_eq!(settled.refunded_trial_seconds, 50);

        let trial_left: i64 = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT trial_seconds_remaining FROM accounts WHERE id = ?1",
                params![account.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(trial_left, 590);
    }
}
