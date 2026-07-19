//! Jobs-owned packet allowance holds for managed resume generation.
//!
//! Provider dispatch is permitted only after one included packet slot is held
//! atomically. The hold is scoped to a job and generation fence, converts into
//! ordinary packet metering without a second increment, and never debits the
//! account's general chat balance.

use anyhow::Result;
use rusqlite::{params, OptionalExtension, TransactionBehavior};

use super::{jobs, jobs_generation, jobs_provider_cost_holds, DbPool};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AllowanceReservation {
    Reserved,
    AlreadyMetered,
    Busy,
    Exhausted,
}

pub fn reserve(
    pool: &DbPool,
    account_id: &str,
    job_id: &str,
    generation_key: &str,
    reservation_token: &str,
) -> Result<AllowanceReservation> {
    let _ = jobs::get_entitlement(pool, account_id)?;
    let now = jobs::now_ms();
    let stale_before = now.saturating_sub(
        jobs_generation::RESERVATION_TTL
            .as_millis()
            .try_into()
            .unwrap_or(i64::MAX),
    );
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            if tx
                .query_row(
                    "SELECT 1 FROM jobs_packet_metering WHERE account_id = ?1 AND job_id = ?2",
                    params![account_id, job_id],
                    |row| row.get::<_, i64>(0),
                )
                .optional()?
                .is_some()
            {
                tx.commit()?;
                return Ok(AllowanceReservation::AlreadyMetered);
            }
            let (used, limit, period_start): (i64, i64, i64) = tx.query_row(
                "SELECT used_packets, monthly_packet_limit, period_start_ms
                   FROM jobs_entitlements WHERE account_id = ?1",
                params![account_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )?;
            let existing: Option<(String, String, String, i64)> = tx
                .query_row(
                    "SELECT generation_key, reservation_token, status, period_start_ms
                       FROM jobs_generation_allowance_reservations
                      WHERE account_id = ?1 AND job_id = ?2",
                    params![account_id, job_id],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
                )
                .optional()?;
            if let Some((existing_key, existing_token, status, held_period)) = &existing {
                if status == "committed" {
                    anyhow::bail!("committed Jobs allowance has no packet metering row")
                }
                if status == "reserved" && *held_period == period_start {
                    if existing_key != generation_key {
                        let owner: Option<(String, i64)> = tx
                            .query_row(
                                "SELECT status, updated_at_ms FROM jobs_resume_generations
                                  WHERE account_id = ?1 AND generation_key = ?2",
                                params![account_id, existing_key],
                                |row| Ok((row.get(0)?, row.get(1)?)),
                            )
                            .optional()?;
                        if owner.as_ref().is_some_and(|(owner_status, updated)| {
                            owner_status == "completed"
                                || (owner_status == "reserved" && *updated >= stale_before)
                        }) {
                            tx.commit()?;
                            return Ok(AllowanceReservation::Busy);
                        }
                        tx.execute(
                            "UPDATE jobs_generation_allowance_reservations
                                SET generation_key = ?3, reservation_token = ?4,
                                    updated_at_ms = ?5
                              WHERE account_id = ?1 AND job_id = ?2",
                            params![account_id, job_id, generation_key, reservation_token, now],
                        )?;
                        tx.commit()?;
                        return Ok(AllowanceReservation::Reserved);
                    }
                    if existing_token != reservation_token {
                        tx.execute(
                            "UPDATE jobs_generation_allowance_reservations
                                SET reservation_token = ?3, updated_at_ms = ?4
                              WHERE account_id = ?1 AND job_id = ?2",
                            params![account_id, job_id, reservation_token, now],
                        )?;
                    }
                    tx.commit()?;
                    return Ok(AllowanceReservation::Reserved);
                }
            }
            if used >= limit {
                tx.commit()?;
                return Ok(AllowanceReservation::Exhausted);
            }
            if existing.is_some() {
                tx.execute(
                    "UPDATE jobs_generation_allowance_reservations
                        SET generation_key = ?3, reservation_token = ?4, status = 'reserved',
                            period_start_ms = ?5, application_id = NULL, updated_at_ms = ?6
                      WHERE account_id = ?1 AND job_id = ?2",
                    params![
                        account_id,
                        job_id,
                        generation_key,
                        reservation_token,
                        period_start,
                        now
                    ],
                )?;
            } else {
                tx.execute(
                    "INSERT INTO jobs_generation_allowance_reservations (
                        account_id, job_id, generation_key, reservation_token, status,
                        period_start_ms, created_at_ms, updated_at_ms
                     ) VALUES (?1, ?2, ?3, ?4, 'reserved', ?5, ?6, ?6)",
                    params![
                        account_id,
                        job_id,
                        generation_key,
                        reservation_token,
                        period_start,
                        now
                    ],
                )?;
            }
            tx.execute(
                "UPDATE jobs_entitlements SET used_packets = used_packets + 1,
                    updated_at_ms = ?2 WHERE account_id = ?1",
                params![account_id, now],
            )?;
            tx.commit()?;
            Ok(AllowanceReservation::Reserved)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            tx.query_one(
                "SELECT pg_advisory_xact_lock(hashtextextended($1, 0))",
                &[&format!("jobs-allowance:{account_id}:{job_id}")],
            )?;
            let entitlement = tx.query_one(
                "SELECT used_packets, monthly_packet_limit, period_start_ms
                   FROM jobs_entitlements WHERE account_id = $1 FOR UPDATE",
                &[&account_id],
            )?;
            if tx
                .query_opt(
                    "SELECT 1 FROM jobs_packet_metering WHERE account_id = $1 AND job_id = $2",
                    &[&account_id, &job_id],
                )?
                .is_some()
            {
                tx.commit()?;
                return Ok(AllowanceReservation::AlreadyMetered);
            }
            let used: i64 = entitlement.get(0);
            let limit: i64 = entitlement.get(1);
            let period_start: i64 = entitlement.get(2);
            let existing = tx.query_opt(
                "SELECT generation_key, reservation_token, status, period_start_ms
                   FROM jobs_generation_allowance_reservations
                  WHERE account_id = $1 AND job_id = $2 FOR UPDATE",
                &[&account_id, &job_id],
            )?;
            if let Some(row) = &existing {
                let existing_key: String = row.get(0);
                let existing_token: String = row.get(1);
                let status: String = row.get(2);
                let held_period: i64 = row.get(3);
                if status == "committed" {
                    anyhow::bail!("committed Jobs allowance has no packet metering row")
                }
                if status == "reserved" && held_period == period_start {
                    if existing_key != generation_key {
                        let owner = tx.query_opt(
                            "SELECT status, updated_at_ms FROM jobs_resume_generations
                              WHERE account_id = $1 AND generation_key = $2 FOR UPDATE",
                            &[&account_id, &existing_key],
                        )?;
                        if owner.as_ref().is_some_and(|row| {
                            let owner_status: String = row.get(0);
                            let updated: i64 = row.get(1);
                            owner_status == "completed"
                                || (owner_status == "reserved" && updated >= stale_before)
                        }) {
                            tx.commit()?;
                            return Ok(AllowanceReservation::Busy);
                        }
                        tx.execute(
                            "UPDATE jobs_generation_allowance_reservations
                                SET generation_key = $3, reservation_token = $4,
                                    updated_at_ms = $5
                              WHERE account_id = $1 AND job_id = $2",
                            &[
                                &account_id,
                                &job_id,
                                &generation_key,
                                &reservation_token,
                                &now,
                            ],
                        )?;
                        tx.commit()?;
                        return Ok(AllowanceReservation::Reserved);
                    }
                    if existing_token != reservation_token {
                        tx.execute(
                            "UPDATE jobs_generation_allowance_reservations
                                SET reservation_token = $3, updated_at_ms = $4
                              WHERE account_id = $1 AND job_id = $2",
                            &[&account_id, &job_id, &reservation_token, &now],
                        )?;
                    }
                    tx.commit()?;
                    return Ok(AllowanceReservation::Reserved);
                }
            }
            if used >= limit {
                tx.commit()?;
                return Ok(AllowanceReservation::Exhausted);
            }
            if existing.is_some() {
                tx.execute(
                    "UPDATE jobs_generation_allowance_reservations
                        SET generation_key = $3, reservation_token = $4, status = 'reserved',
                            period_start_ms = $5, application_id = NULL, updated_at_ms = $6
                      WHERE account_id = $1 AND job_id = $2",
                    &[
                        &account_id,
                        &job_id,
                        &generation_key,
                        &reservation_token,
                        &period_start,
                        &now,
                    ],
                )?;
            } else {
                tx.execute(
                    "INSERT INTO jobs_generation_allowance_reservations (
                        account_id, job_id, generation_key, reservation_token, status,
                        period_start_ms, created_at_ms, updated_at_ms
                     ) VALUES ($1, $2, $3, $4, 'reserved', $5, $6, $6)",
                    &[
                        &account_id,
                        &job_id,
                        &generation_key,
                        &reservation_token,
                        &period_start,
                        &now,
                    ],
                )?;
            }
            tx.execute(
                "UPDATE jobs_entitlements SET used_packets = used_packets + 1,
                    updated_at_ms = $2 WHERE account_id = $1",
                &[&account_id, &now],
            )?;
            tx.commit()?;
            Ok(AllowanceReservation::Reserved)
        }
    })
}

pub fn release(
    pool: &DbPool,
    account_id: &str,
    job_id: &str,
    generation_key: &str,
    reservation_token: &str,
) -> Result<bool> {
    let now = jobs::now_ms();
    let account_scope_hash = jobs_provider_cost_holds::account_scope_hash(account_id);
    let generation_scope_hash =
        jobs_provider_cost_holds::generation_scope_hash(account_id, generation_key);
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let held_period: Option<i64> = tx
                .query_row(
                    "SELECT period_start_ms FROM jobs_generation_allowance_reservations
                      WHERE account_id = ?1 AND job_id = ?2 AND generation_key = ?3
                        AND reservation_token = ?4 AND status = 'reserved'",
                    params![account_id, job_id, generation_key, reservation_token],
                    |row| row.get(0),
                )
                .optional()?;
            let Some(held_period) = held_period else {
                tx.commit()?;
                return Ok(false);
            };
            let provider_exposure: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM jobs_provider_cost_holds
                  WHERE account_scope_hash = ?1 AND generation_scope_hash = ?2
                    AND status IN ('held', 'settled'))",
                params![account_scope_hash, generation_scope_hash],
                |row| row.get(0),
            )?;
            if provider_exposure {
                // Once any provider request may have been billed, this packet
                // is consumed even if generation later times out, is rejected,
                // or falls back to the deterministic renderer.
                tx.commit()?;
                return Ok(false);
            }
            let current_period: i64 = tx.query_row(
                "SELECT period_start_ms FROM jobs_entitlements WHERE account_id = ?1",
                params![account_id],
                |row| row.get(0),
            )?;
            tx.execute(
                "UPDATE jobs_generation_allowance_reservations
                    SET status = 'released', updated_at_ms = ?5
                  WHERE account_id = ?1 AND job_id = ?2 AND generation_key = ?3
                    AND reservation_token = ?4 AND status = 'reserved'",
                params![account_id, job_id, generation_key, reservation_token, now],
            )?;
            if held_period == current_period {
                tx.execute(
                    "UPDATE jobs_entitlements SET used_packets = MAX(used_packets - 1, 0),
                        updated_at_ms = ?2 WHERE account_id = ?1",
                    params![account_id, now],
                )?;
            }
            tx.commit()?;
            Ok(true)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            tx.query_one(
                "SELECT pg_advisory_xact_lock(hashtextextended($1, 0))",
                &[&format!("jobs-allowance:{account_id}:{job_id}")],
            )?;
            let current_period: i64 = tx
                .query_one(
                    "SELECT period_start_ms FROM jobs_entitlements WHERE account_id = $1 FOR UPDATE",
                    &[&account_id],
                )?
                .get(0);
            tx.query_one(
                "SELECT pg_advisory_xact_lock(hashtextextended('jobs-provider-cost-global', 0))",
                &[],
            )?;
            let row = tx.query_opt(
                "SELECT period_start_ms FROM jobs_generation_allowance_reservations
                  WHERE account_id = $1 AND job_id = $2 AND generation_key = $3
                    AND reservation_token = $4 AND status = 'reserved' FOR UPDATE",
                &[&account_id, &job_id, &generation_key, &reservation_token],
            )?;
            let Some(row) = row else {
                tx.commit()?;
                return Ok(false);
            };
            let provider_exposure: bool = tx
                .query_one(
                    "SELECT EXISTS(SELECT 1 FROM jobs_provider_cost_holds
                      WHERE account_scope_hash = $1 AND generation_scope_hash = $2
                        AND status IN ('held', 'settled'))",
                    &[&account_scope_hash, &generation_scope_hash],
                )?
                .get(0);
            if provider_exposure {
                tx.commit()?;
                return Ok(false);
            }
            let held_period: i64 = row.get(0);
            tx.execute(
                "UPDATE jobs_generation_allowance_reservations
                    SET status = 'released', updated_at_ms = $5
                  WHERE account_id = $1 AND job_id = $2 AND generation_key = $3
                    AND reservation_token = $4 AND status = 'reserved'",
                &[
                    &account_id,
                    &job_id,
                    &generation_key,
                    &reservation_token,
                    &now,
                ],
            )?;
            if held_period == current_period {
                tx.execute(
                    "UPDATE jobs_entitlements SET used_packets = GREATEST(used_packets - 1, 0),
                        updated_at_ms = $2 WHERE account_id = $1",
                    &[&account_id, &now],
                )?;
            }
            tx.commit()?;
            Ok(true)
        }
    })
}
