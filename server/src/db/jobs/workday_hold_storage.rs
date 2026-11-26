//! Transaction-local persistence for immutable Workday reconciliation holds.
//!
//! Callers own the account writer lock and transaction ordering. In
//! particular, observations are admitted only after the source run has been
//! completed in that same transaction.

use anyhow::Result;
use rusqlite::params;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum WorkdayReconciliationHoldWrite {
    Inserted,
    Existing,
}

pub(super) struct WorkdayReconciliationHoldInput<'a> {
    pub(super) account_id: &'a str,
    pub(super) source_id: &'a str,
    pub(super) prior_job_id: &'a str,
    pub(super) legacy_external_id: &'a str,
    pub(super) current_run_id: &'a str,
    pub(super) current_snapshot_sha256: &'a str,
    pub(super) proposed_external_id: &'a str,
    pub(super) observed_at_ms: i64,
}

pub(super) struct WorkdayReconciliationHoldObservationInput<'a> {
    pub(super) run_id: &'a str,
    pub(super) account_id: &'a str,
    pub(super) source_id: &'a str,
    pub(super) job_id: &'a str,
}

pub(super) struct WorkdayReconciliationHoldRunInput<'a> {
    pub(super) run_id: &'a str,
    pub(super) account_id: &'a str,
    pub(super) source_id: &'a str,
}

fn require_nonempty(field: &str, value: &str) -> Result<()> {
    if value.trim().is_empty() {
        anyhow::bail!("Workday reconciliation hold {field} is empty");
    }
    Ok(())
}

fn validate_hold_input(input: &WorkdayReconciliationHoldInput<'_>) -> Result<()> {
    for (field, value) in [
        ("account id", input.account_id),
        ("source id", input.source_id),
        ("prior job id", input.prior_job_id),
        ("legacy external id", input.legacy_external_id),
        ("current run id", input.current_run_id),
        ("proposed external id", input.proposed_external_id),
    ] {
        require_nonempty(field, value)?;
    }
    if input.legacy_external_id == input.proposed_external_id {
        anyhow::bail!("Workday reconciliation hold cannot propose its legacy external id");
    }
    if input.observed_at_ms <= 0 {
        anyhow::bail!("Workday reconciliation hold observation time is invalid");
    }
    if input.current_snapshot_sha256.len() != 64
        || !input
            .current_snapshot_sha256
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        anyhow::bail!(
            "Workday reconciliation hold snapshot hash is not canonical lowercase SHA-256"
        );
    }
    Ok(())
}

fn validate_current_membership_sqlite(
    tx: &rusqlite::Transaction<'_>,
    input: &WorkdayReconciliationHoldInput<'_>,
) -> Result<()> {
    let valid: i64 = tx.query_row(
        "SELECT COUNT(*)
           FROM jobs_discovery_memberships membership
           JOIN jobs_discovery_sources source ON source.id = membership.source_id
           JOIN jobs_discovery_runs run ON run.id = ?5
          WHERE membership.account_id = ?1 AND membership.source_id = ?2
            AND membership.job_id = ?3 AND membership.external_id = ?4
            AND source.account_id = ?1 AND source.provider = 'workday'
            AND run.account_id = ?1 AND run.source_id = ?2 AND run.status = 'running'
            AND run.snapshot_hash = ?6",
        params![
            input.account_id,
            input.source_id,
            input.prior_job_id,
            input.legacy_external_id,
            input.current_run_id,
            input.current_snapshot_sha256,
        ],
        |row| row.get(0),
    )?;
    if valid != 1 {
        anyhow::bail!("Workday reconciliation hold membership identity drifted");
    }
    Ok(())
}

fn validate_current_membership_postgres(
    tx: &mut postgres::Transaction<'_>,
    input: &WorkdayReconciliationHoldInput<'_>,
) -> Result<()> {
    let valid = tx
        .query_opt(
            "SELECT 1
               FROM jobs_discovery_memberships membership
               JOIN jobs_discovery_sources source ON source.id = membership.source_id
               JOIN jobs_discovery_runs run ON run.id = $5
              WHERE membership.account_id = $1 AND membership.source_id = $2
                AND membership.job_id = $3 AND membership.external_id = $4
                AND source.account_id = $1 AND source.provider = 'workday'
                AND run.account_id = $1 AND run.source_id = $2 AND run.status = 'running'
                AND run.snapshot_hash = $6
              FOR KEY SHARE OF membership, source, run",
            &[
                &input.account_id,
                &input.source_id,
                &input.prior_job_id,
                &input.legacy_external_id,
                &input.current_run_id,
                &input.current_snapshot_sha256,
            ],
        )?
        .is_some();
    if !valid {
        anyhow::bail!("Workday reconciliation hold membership identity drifted");
    }
    Ok(())
}

fn require_exact_hold(
    input: &WorkdayReconciliationHoldInput<'_>,
    stored: (String, String, String, String, String, String, String),
) -> Result<(String, String)> {
    if stored.0 != input.account_id
        || stored.1 != input.source_id
        || stored.2 != input.prior_job_id
        || stored.3 != input.legacy_external_id
        || stored.6 != input.proposed_external_id
    {
        anyhow::bail!("Workday reconciliation hold conflicts with immutable first evidence");
    }
    Ok((stored.4, stored.5))
}

fn validate_stored_origin_sqlite(
    tx: &rusqlite::Transaction<'_>,
    input: &WorkdayReconciliationHoldInput<'_>,
    run_id: &str,
    snapshot_sha256: &str,
) -> Result<()> {
    let valid: i64 = tx.query_row(
        "SELECT COUNT(*)
           FROM jobs_discovery_memberships membership
           JOIN jobs_discovery_sources source ON source.id = membership.source_id
           JOIN jobs_discovery_runs run ON run.id = ?5
          WHERE membership.account_id = ?1 AND membership.source_id = ?2
            AND membership.job_id = ?3 AND membership.external_id = ?4
            AND source.account_id = ?1 AND source.provider = 'workday'
            AND run.account_id = ?1 AND run.source_id = ?2 AND run.snapshot_hash = ?6",
        params![
            input.account_id,
            input.source_id,
            input.prior_job_id,
            input.legacy_external_id,
            run_id,
            snapshot_sha256,
        ],
        |row| row.get(0),
    )?;
    if valid != 1 {
        anyhow::bail!("stored Workday reconciliation hold origin drifted");
    }
    Ok(())
}

fn validate_stored_origin_postgres(
    tx: &mut postgres::Transaction<'_>,
    input: &WorkdayReconciliationHoldInput<'_>,
    run_id: &str,
    snapshot_sha256: &str,
) -> Result<()> {
    let valid = tx
        .query_opt(
            "SELECT 1
               FROM jobs_discovery_memberships membership
               JOIN jobs_discovery_sources source ON source.id = membership.source_id
               JOIN jobs_discovery_runs run ON run.id = $5
              WHERE membership.account_id = $1 AND membership.source_id = $2
                AND membership.job_id = $3 AND membership.external_id = $4
                AND source.account_id = $1 AND source.provider = 'workday'
                AND run.account_id = $1 AND run.source_id = $2 AND run.snapshot_hash = $6
              FOR KEY SHARE OF membership, source, run",
            &[
                &input.account_id,
                &input.source_id,
                &input.prior_job_id,
                &input.legacy_external_id,
                &run_id,
                &snapshot_sha256,
            ],
        )?
        .is_some();
    if !valid {
        anyhow::bail!("stored Workday reconciliation hold origin drifted");
    }
    Ok(())
}

pub(super) fn write_workday_reconciliation_hold_sqlite(
    tx: &rusqlite::Transaction<'_>,
    input: &WorkdayReconciliationHoldInput<'_>,
) -> Result<WorkdayReconciliationHoldWrite> {
    validate_hold_input(input)?;
    validate_current_membership_sqlite(tx, input)?;
    let changed = tx.execute(
        "INSERT INTO jobs_workday_reconciliation_holds (
             account_id, source_id, job_id, legacy_external_id, first_run_id,
             first_snapshot_sha256, proposed_external_id, reason, created_at_ms
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'protected_history', ?8)
         ON CONFLICT(account_id, source_id, job_id) DO NOTHING",
        params![
            input.account_id,
            input.source_id,
            input.prior_job_id,
            input.legacy_external_id,
            input.current_run_id,
            input.current_snapshot_sha256,
            input.proposed_external_id,
            input.observed_at_ms,
        ],
    )?;
    let stored = tx.query_row(
        "SELECT account_id, source_id, job_id, legacy_external_id, first_run_id,
                first_snapshot_sha256, proposed_external_id
           FROM jobs_workday_reconciliation_holds
          WHERE account_id = ?1 AND source_id = ?2 AND job_id = ?3",
        params![input.account_id, input.source_id, input.prior_job_id],
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
    let (first_run_id, first_snapshot_sha256) = require_exact_hold(input, stored)?;
    if changed == 1 {
        if first_run_id != input.current_run_id
            || first_snapshot_sha256 != input.current_snapshot_sha256
        {
            anyhow::bail!("inserted Workday reconciliation hold lost current first evidence");
        }
        Ok(WorkdayReconciliationHoldWrite::Inserted)
    } else {
        validate_stored_origin_sqlite(tx, input, &first_run_id, &first_snapshot_sha256)?;
        Ok(WorkdayReconciliationHoldWrite::Existing)
    }
}

pub(super) fn write_workday_reconciliation_hold_postgres(
    tx: &mut postgres::Transaction<'_>,
    input: &WorkdayReconciliationHoldInput<'_>,
) -> Result<WorkdayReconciliationHoldWrite> {
    validate_hold_input(input)?;
    validate_current_membership_postgres(tx, input)?;
    let changed = tx.execute(
        "INSERT INTO jobs_workday_reconciliation_holds (
             account_id, source_id, job_id, legacy_external_id, first_run_id,
             first_snapshot_sha256, proposed_external_id, reason, created_at_ms
         ) VALUES ($1, $2, $3, $4, $5, $6, $7, 'protected_history', $8)
         ON CONFLICT(account_id, source_id, job_id) DO NOTHING",
        &[
            &input.account_id,
            &input.source_id,
            &input.prior_job_id,
            &input.legacy_external_id,
            &input.current_run_id,
            &input.current_snapshot_sha256,
            &input.proposed_external_id,
            &input.observed_at_ms,
        ],
    )?;
    let stored = tx.query_one(
        "SELECT account_id, source_id, job_id, legacy_external_id, first_run_id,
                first_snapshot_sha256, proposed_external_id
           FROM jobs_workday_reconciliation_holds
          WHERE account_id = $1 AND source_id = $2 AND job_id = $3",
        &[&input.account_id, &input.source_id, &input.prior_job_id],
    )?;
    let (first_run_id, first_snapshot_sha256) = require_exact_hold(
        input,
        (
            stored.get(0),
            stored.get(1),
            stored.get(2),
            stored.get(3),
            stored.get(4),
            stored.get(5),
            stored.get(6),
        ),
    )?;
    if changed == 1 {
        if first_run_id != input.current_run_id
            || first_snapshot_sha256 != input.current_snapshot_sha256
        {
            anyhow::bail!("inserted Workday reconciliation hold lost current first evidence");
        }
        Ok(WorkdayReconciliationHoldWrite::Inserted)
    } else {
        validate_stored_origin_postgres(tx, input, &first_run_id, &first_snapshot_sha256)?;
        Ok(WorkdayReconciliationHoldWrite::Existing)
    }
}

fn validate_observation(input: &WorkdayReconciliationHoldObservationInput<'_>) -> Result<()> {
    for (field, value) in [
        ("run id", input.run_id),
        ("account id", input.account_id),
        ("source id", input.source_id),
        ("job id", input.job_id),
    ] {
        require_nonempty(field, value)?;
    }
    Ok(())
}

pub(super) fn record_workday_reconciliation_hold_observation_sqlite(
    tx: &rusqlite::Transaction<'_>,
    input: &WorkdayReconciliationHoldObservationInput<'_>,
) -> Result<()> {
    validate_observation(input)?;
    tx.execute(
        "INSERT INTO jobs_workday_reconciliation_hold_observations
             (run_id, account_id, source_id, job_id) VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(run_id, account_id, source_id, job_id) DO NOTHING",
        params![
            input.run_id,
            input.account_id,
            input.source_id,
            input.job_id
        ],
    )?;
    Ok(())
}

pub(super) fn record_workday_reconciliation_hold_observation_postgres(
    tx: &mut postgres::Transaction<'_>,
    input: &WorkdayReconciliationHoldObservationInput<'_>,
) -> Result<()> {
    validate_observation(input)?;
    tx.execute(
        "INSERT INTO jobs_workday_reconciliation_hold_observations
             (run_id, account_id, source_id, job_id) VALUES ($1, $2, $3, $4)
         ON CONFLICT(run_id, account_id, source_id, job_id) DO NOTHING",
        &[
            &input.run_id,
            &input.account_id,
            &input.source_id,
            &input.job_id,
        ],
    )?;
    Ok(())
}

fn validate_run(input: &WorkdayReconciliationHoldRunInput<'_>) -> Result<()> {
    for (field, value) in [
        ("run id", input.run_id),
        ("account id", input.account_id),
        ("source id", input.source_id),
    ] {
        require_nonempty(field, value)?;
    }
    Ok(())
}

pub(super) fn count_workday_reconciliation_holds_sqlite(
    tx: &rusqlite::Transaction<'_>,
    input: &WorkdayReconciliationHoldRunInput<'_>,
) -> Result<i64> {
    validate_run(input)?;
    let (account, source, status, completed, provider, source_account, delta): (String, String, String, Option<i64>, String, String, i64) =
        tx.query_row(
            "SELECT run.account_id, run.source_id, run.status, run.completed_at_ms, source.provider, source.account_id,
                    run.discovered_count - run.upserted_count
               FROM jobs_discovery_runs run JOIN jobs_discovery_sources source ON source.id = run.source_id
              WHERE run.id = ?1", params![input.run_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?, row.get(6)?)),
        )?;
    let (count, valid, foreign): (i64, i64, i64) = tx.query_row(
        "SELECT COUNT(*), COUNT(hold.job_id), COALESCE(SUM(CASE WHEN observation.account_id <> ?2
                 OR observation.source_id <> ?3 THEN 1 ELSE 0 END), 0)
           FROM jobs_workday_reconciliation_hold_observations observation
           LEFT JOIN jobs_workday_reconciliation_holds hold ON hold.account_id = observation.account_id
             AND hold.source_id = observation.source_id AND hold.job_id = observation.job_id
          WHERE observation.run_id = ?1", params![input.run_id, input.account_id, input.source_id],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?;
    if account != input.account_id
        || source != input.source_id
        || status != "completed"
        || completed.is_none()
        || provider != "workday"
        || source_account != input.account_id
        || !(1..=10_000).contains(&delta)
        || count == 0
        || count != valid
        || foreign != 0
        || count > delta
    {
        anyhow::bail!("Workday reconciliation hold observation count is unsafe");
    }
    Ok(count)
}

pub(super) fn count_workday_reconciliation_holds_postgres(
    tx: &mut postgres::Transaction<'_>,
    input: &WorkdayReconciliationHoldRunInput<'_>,
) -> Result<i64> {
    validate_run(input)?;
    let run = tx.query_one(
        "SELECT run.account_id, run.source_id, run.status, run.completed_at_ms, source.provider, source.account_id,
                run.discovered_count - run.upserted_count
           FROM jobs_discovery_runs run JOIN jobs_discovery_sources source ON source.id = run.source_id
          WHERE run.id = $1 FOR KEY SHARE OF run, source", &[&input.run_id],
    )?;
    let count = tx.query_one(
        "SELECT COUNT(*), COUNT(hold.job_id), COUNT(*) FILTER (
                    WHERE observation.account_id <> $2 OR observation.source_id <> $3)
           FROM jobs_workday_reconciliation_hold_observations observation
           LEFT JOIN jobs_workday_reconciliation_holds hold ON hold.account_id = observation.account_id
             AND hold.source_id = observation.source_id AND hold.job_id = observation.job_id
          WHERE observation.run_id = $1", &[&input.run_id, &input.account_id, &input.source_id],
    )?;
    let total: i64 = count.get(0);
    let valid: i64 = count.get(1);
    let foreign: i64 = count.get(2);
    let completed: Option<i64> = run.get(3);
    let delta: i64 = run.get(6);
    if run.get::<_, String>(0) != input.account_id
        || run.get::<_, String>(1) != input.source_id
        || run.get::<_, String>(2) != "completed"
        || completed.is_none()
        || run.get::<_, String>(4) != "workday"
        || run.get::<_, String>(5) != input.account_id
        || !(1..=10_000).contains(&delta)
        || total == 0
        || total != valid
        || foreign != 0
        || total > delta
    {
        anyhow::bail!("Workday reconciliation hold observation count is unsafe");
    }
    Ok(total)
}
