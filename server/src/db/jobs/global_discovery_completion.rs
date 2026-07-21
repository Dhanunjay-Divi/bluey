fn global_discovery_next_run_at(
    source: &GlobalDiscoverySource,
    completed_at_ms: i64,
    failures: i64,
) -> i64 {
    let multiplier = 1_i64 << failures.saturating_sub(1).clamp(0, 3) as u32;
    let interval = source
        .run_interval_ms
        .saturating_mul(multiplier)
        .min(DISCOVERY_MAX_INTERVAL_MS);
    let digest = Sha256::digest(source.id.as_bytes());
    let jitter_seed = u64::from_be_bytes(digest[..8].try_into().expect("SHA-256 prefix"));
    let jitter_window = (interval / 10).max(1);
    completed_at_ms
        .saturating_add(interval)
        .saturating_add((jitter_seed % jitter_window as u64) as i64)
}

pub fn complete_global_discovery_ingestion(
    pool: &DbPool,
    source_id: &str,
    input: &GlobalIngestionCompleteInput,
) -> Result<GlobalIngestionRunResult> {
    validate_global_completion_input(input)?;
    let artifact_sha256 = normalized_sha256(&input.artifact_sha256)?;
    let run_id = global_run_id(source_id, &input.replay_key);
    let now = now_ms();
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            if let Some(result) = completed_global_run_sqlite(
                &tx,
                source_id,
                &run_id,
                input,
                &artifact_sha256,
            )? {
                tx.commit()?;
                return Ok(result);
            }
            let source = validate_global_lease_sqlite(
                &tx,
                source_id,
                &input.lease_token,
                &input.replay_key,
                input.scheduled_for_ms,
                &artifact_sha256,
                now,
            )?;
            let (received_rows, received_batches) =
                validate_global_run_batches_sqlite(&tx, &run_id, input, &artifact_sha256)?;
            let expired_count = expire_missing_global_memberships_sqlite(
                &tx,
                source_id,
                &run_id,
                input.complete_snapshot,
                now,
            )?;
            tx.execute(
                "UPDATE jobs_global_ingestion_runs
                    SET status = 'completed', expired_count = ?2, completed_at_ms = ?3,
                        error_code = NULL
                  WHERE id = ?1 AND status = 'running'",
                params![run_id, expired_count, now],
            )?;
            let next_run_at = global_discovery_next_run_at(&source, now, 0);
            tx.execute(
                "UPDATE jobs_global_discovery_sources
                    SET health = 'healthy', consecutive_failures = 0,
                        next_run_at_ms = ?2, last_success_at_ms = ?3,
                        last_error_code = NULL, lease_owner = NULL, lease_token = NULL,
                        lease_expires_at_ms = NULL, updated_at_ms = ?3
                  WHERE id = ?1",
                params![source_id, next_run_at, now],
            )?;
            tx.commit()?;
            Ok(GlobalIngestionRunResult {
                run_id,
                replay_key: input.replay_key.clone(),
                status: "completed".to_string(),
                received_rows,
                received_batches,
                expired_count,
                replayed: false,
            })
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            if let Some(result) = completed_global_run_postgres(
                &mut tx,
                source_id,
                &run_id,
                input,
                &artifact_sha256,
            )? {
                tx.commit()?;
                return Ok(result);
            }
            let source = validate_global_lease_postgres(
                &mut tx,
                source_id,
                &input.lease_token,
                &input.replay_key,
                input.scheduled_for_ms,
                &artifact_sha256,
                now,
            )?;
            let (received_rows, received_batches) =
                validate_global_run_batches_postgres(&mut tx, &run_id, input, &artifact_sha256)?;
            let expired_count = expire_missing_global_memberships_postgres(
                &mut tx,
                source_id,
                &run_id,
                input.complete_snapshot,
                now,
            )?;
            tx.execute(
                "UPDATE jobs_global_ingestion_runs
                    SET status = 'completed', expired_count = $2, completed_at_ms = $3,
                        error_code = NULL
                  WHERE id = $1 AND status = 'running'",
                &[&run_id, &expired_count, &now],
            )?;
            let next_run_at = global_discovery_next_run_at(&source, now, 0);
            tx.execute(
                "UPDATE jobs_global_discovery_sources
                    SET health = 'healthy', consecutive_failures = 0,
                        next_run_at_ms = $2, last_success_at_ms = $3,
                        last_error_code = NULL, lease_owner = NULL, lease_token = NULL,
                        lease_expires_at_ms = NULL, updated_at_ms = $3
                  WHERE id = $1",
                &[&source_id, &next_run_at, &now],
            )?;
            tx.commit()?;
            Ok(GlobalIngestionRunResult {
                run_id,
                replay_key: input.replay_key.clone(),
                status: "completed".to_string(),
                received_rows,
                received_batches,
                expired_count,
                replayed: false,
            })
        }
    })
}

pub fn fail_global_discovery_ingestion(
    pool: &DbPool,
    source_id: &str,
    input: &GlobalIngestionFailureInput,
) -> Result<GlobalIngestionRunResult> {
    let artifact_sha256 = normalized_sha256(&input.artifact_sha256)?;
    let error_code = normalized_global_error_code(&input.error_code)?;
    let run_id = global_run_id(source_id, &input.replay_key);
    let now = now_ms();
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            if let Some(result) = failed_global_run_sqlite(
                &tx,
                source_id,
                &run_id,
                input,
                &artifact_sha256,
                &error_code,
            )? {
                tx.commit()?;
                return Ok(result);
            }
            let source = validate_global_lease_sqlite(
                &tx,
                source_id,
                &input.lease_token,
                &input.replay_key,
                input.scheduled_for_ms,
                &artifact_sha256,
                now,
            )?;
            let (received_rows, received_batches): (i64, i64) = tx.query_row(
                "SELECT received_rows, received_batches FROM jobs_global_ingestion_runs
                  WHERE id = ?1 AND status = 'running'",
                params![run_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?;
            tx.execute(
                "UPDATE jobs_global_ingestion_runs
                    SET status = 'failed', error_code = ?2, completed_at_ms = ?3
                  WHERE id = ?1 AND status = 'running'",
                params![run_id, error_code, now],
            )?;
            let failures = source.consecutive_failures.saturating_add(1);
            let next_run_at = global_discovery_next_run_at(&source, now, failures);
            tx.execute(
                "UPDATE jobs_global_discovery_sources
                    SET health = 'degraded', consecutive_failures = ?2,
                        next_run_at_ms = ?3, last_failure_at_ms = ?4,
                        last_error_code = ?5, lease_owner = NULL, lease_token = NULL,
                        lease_expires_at_ms = NULL, updated_at_ms = ?4
                  WHERE id = ?1",
                params![source_id, failures, next_run_at, now, error_code],
            )?;
            tx.commit()?;
            Ok(GlobalIngestionRunResult {
                run_id,
                replay_key: input.replay_key.clone(),
                status: "failed".to_string(),
                received_rows,
                received_batches,
                expired_count: 0,
                replayed: false,
            })
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            if let Some(result) = failed_global_run_postgres(
                &mut tx,
                source_id,
                &run_id,
                input,
                &artifact_sha256,
                &error_code,
            )? {
                tx.commit()?;
                return Ok(result);
            }
            let source = validate_global_lease_postgres(
                &mut tx,
                source_id,
                &input.lease_token,
                &input.replay_key,
                input.scheduled_for_ms,
                &artifact_sha256,
                now,
            )?;
            let row = tx.query_one(
                "SELECT received_rows, received_batches FROM jobs_global_ingestion_runs
                  WHERE id = $1 AND status = 'running' FOR UPDATE",
                &[&run_id],
            )?;
            let received_rows = row.get(0);
            let received_batches = row.get(1);
            tx.execute(
                "UPDATE jobs_global_ingestion_runs
                    SET status = 'failed', error_code = $2, completed_at_ms = $3
                  WHERE id = $1 AND status = 'running'",
                &[&run_id, &error_code, &now],
            )?;
            let failures = source.consecutive_failures.saturating_add(1);
            let next_run_at = global_discovery_next_run_at(&source, now, failures);
            tx.execute(
                "UPDATE jobs_global_discovery_sources
                    SET health = 'degraded', consecutive_failures = $2,
                        next_run_at_ms = $3, last_failure_at_ms = $4,
                        last_error_code = $5, lease_owner = NULL, lease_token = NULL,
                        lease_expires_at_ms = NULL, updated_at_ms = $4
                  WHERE id = $1",
                &[&source_id, &failures, &next_run_at, &now, &error_code],
            )?;
            tx.commit()?;
            Ok(GlobalIngestionRunResult {
                run_id,
                replay_key: input.replay_key.clone(),
                status: "failed".to_string(),
                received_rows,
                received_batches,
                expired_count: 0,
                replayed: false,
            })
        }
    })
}

fn validate_global_completion_input(input: &GlobalIngestionCompleteInput) -> Result<()> {
    if !input.complete_snapshot
        || input.expected_rows <= 0
        || input.expected_rows > 5_000_000
        || input.expected_batches <= 0
        || input.expected_batches > input.expected_rows
    {
        anyhow::bail!("global discovery completion is invalid")
    }
    normalized_sha256(&input.artifact_sha256)?;
    Ok(())
}

fn normalized_global_error_code(value: &str) -> Result<String> {
    let value = value.trim().to_ascii_lowercase();
    if value.is_empty()
        || value.len() > 80
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
    {
        anyhow::bail!("global discovery error code is invalid")
    }
    Ok(value)
}

fn completed_global_run_sqlite(
    tx: &rusqlite::Transaction<'_>,
    source_id: &str,
    run_id: &str,
    input: &GlobalIngestionCompleteInput,
    artifact_sha256: &str,
) -> Result<Option<GlobalIngestionRunResult>> {
    let row = tx
        .query_row(
            "SELECT source_id, replay_key, status, expected_rows, received_rows,
                    received_batches, expired_count, artifact_sha256
               FROM jobs_global_ingestion_runs WHERE id = ?1",
            params![run_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, i64>(5)?,
                    row.get::<_, i64>(6)?,
                    row.get::<_, String>(7)?,
                ))
            },
        )
        .optional()?;
    let Some(row) = row else { return Ok(None) };
    if row.2 != "completed" {
        return Ok(None);
    }
    if row.0 != source_id
        || row.1 != input.replay_key
        || row.3 != input.expected_rows
        || row.4 != input.expected_rows
        || row.5 != input.expected_batches
        || row.7 != artifact_sha256
    {
        anyhow::bail!("global discovery completion replay conflicts with stored evidence")
    }
    Ok(Some(GlobalIngestionRunResult {
        run_id: run_id.to_string(),
        replay_key: input.replay_key.clone(),
        status: row.2,
        received_rows: row.4,
        received_batches: row.5,
        expired_count: row.6,
        replayed: true,
    }))
}

fn completed_global_run_postgres(
    tx: &mut postgres::Transaction<'_>,
    source_id: &str,
    run_id: &str,
    input: &GlobalIngestionCompleteInput,
    artifact_sha256: &str,
) -> Result<Option<GlobalIngestionRunResult>> {
    let Some(row) = tx.query_opt(
        "SELECT source_id, replay_key, status, expected_rows, received_rows,
                received_batches, expired_count, artifact_sha256
           FROM jobs_global_ingestion_runs WHERE id = $1 FOR UPDATE",
        &[&run_id],
    )? else {
        return Ok(None);
    };
    let status = row.get::<_, String>(2);
    if status != "completed" {
        return Ok(None);
    }
    let expected_rows = row.get::<_, i64>(3);
    let received_rows = row.get::<_, i64>(4);
    let received_batches = row.get::<_, i64>(5);
    if row.get::<_, String>(0) != source_id
        || row.get::<_, String>(1) != input.replay_key
        || expected_rows != input.expected_rows
        || received_rows != input.expected_rows
        || received_batches != input.expected_batches
        || row.get::<_, String>(7) != artifact_sha256
    {
        anyhow::bail!("global discovery completion replay conflicts with stored evidence")
    }
    Ok(Some(GlobalIngestionRunResult {
        run_id: run_id.to_string(),
        replay_key: input.replay_key.clone(),
        status,
        received_rows,
        received_batches,
        expired_count: row.get(6),
        replayed: true,
    }))
}

fn failed_global_run_sqlite(
    tx: &rusqlite::Transaction<'_>,
    source_id: &str,
    run_id: &str,
    input: &GlobalIngestionFailureInput,
    artifact_sha256: &str,
    error_code: &str,
) -> Result<Option<GlobalIngestionRunResult>> {
    let row = tx
        .query_row(
            "SELECT source_id, replay_key, status, received_rows, received_batches,
                    artifact_sha256, error_code
               FROM jobs_global_ingestion_runs WHERE id = ?1",
            params![run_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, Option<String>>(6)?,
                ))
            },
        )
        .optional()?;
    let Some(row) = row else { return Ok(None) };
    if row.2 == "completed" {
        anyhow::bail!("completed global discovery run cannot be failed")
    }
    if row.2 != "failed" {
        return Ok(None);
    }
    if row.0 != source_id
        || row.1 != input.replay_key
        || row.5 != artifact_sha256
        || row.6.as_deref() != Some(error_code)
    {
        anyhow::bail!("global discovery failure replay conflicts with stored evidence")
    }
    Ok(Some(GlobalIngestionRunResult {
        run_id: run_id.to_string(),
        replay_key: input.replay_key.clone(),
        status: row.2,
        received_rows: row.3,
        received_batches: row.4,
        expired_count: 0,
        replayed: true,
    }))
}

fn failed_global_run_postgres(
    tx: &mut postgres::Transaction<'_>,
    source_id: &str,
    run_id: &str,
    input: &GlobalIngestionFailureInput,
    artifact_sha256: &str,
    error_code: &str,
) -> Result<Option<GlobalIngestionRunResult>> {
    let Some(row) = tx.query_opt(
        "SELECT source_id, replay_key, status, received_rows, received_batches,
                artifact_sha256, error_code
           FROM jobs_global_ingestion_runs WHERE id = $1 FOR UPDATE",
        &[&run_id],
    )? else {
        return Ok(None);
    };
    let status = row.get::<_, String>(2);
    if status == "completed" {
        anyhow::bail!("completed global discovery run cannot be failed")
    }
    if status != "failed" {
        return Ok(None);
    }
    if row.get::<_, String>(0) != source_id
        || row.get::<_, String>(1) != input.replay_key
        || row.get::<_, String>(5) != artifact_sha256
        || row.get::<_, Option<String>>(6).as_deref() != Some(error_code)
    {
        anyhow::bail!("global discovery failure replay conflicts with stored evidence")
    }
    Ok(Some(GlobalIngestionRunResult {
        run_id: run_id.to_string(),
        replay_key: input.replay_key.clone(),
        status,
        received_rows: row.get(3),
        received_batches: row.get(4),
        expired_count: 0,
        replayed: true,
    }))
}

fn validate_global_run_batches_sqlite(
    tx: &rusqlite::Transaction<'_>,
    run_id: &str,
    input: &GlobalIngestionCompleteInput,
    artifact_sha256: &str,
) -> Result<(i64, i64)> {
    let (status, expected_rows, received_rows, received_batches, artifact):
        (String, i64, i64, i64, String) = tx.query_row(
        "SELECT status, expected_rows, received_rows, received_batches, artifact_sha256
           FROM jobs_global_ingestion_runs WHERE id = ?1",
        params![run_id],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
    )?;
    let (batch_count, minimum_index, maximum_index, summed_rows): (i64, i64, i64, i64) =
        tx.query_row(
            "SELECT COUNT(*), COALESCE(MIN(batch_index), 0),
                    COALESCE(MAX(batch_index), -1), COALESCE(SUM(row_count), 0)
               FROM jobs_global_ingestion_batches WHERE run_id = ?1",
            params![run_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )?;
    validate_global_run_counters(
        &status,
        expected_rows,
        received_rows,
        received_batches,
        &artifact,
        batch_count,
        minimum_index,
        maximum_index,
        summed_rows,
        input,
        artifact_sha256,
    )?;
    Ok((received_rows, received_batches))
}

fn validate_global_run_batches_postgres(
    tx: &mut postgres::Transaction<'_>,
    run_id: &str,
    input: &GlobalIngestionCompleteInput,
    artifact_sha256: &str,
) -> Result<(i64, i64)> {
    let run = tx.query_one(
        "SELECT status, expected_rows, received_rows, received_batches, artifact_sha256
           FROM jobs_global_ingestion_runs WHERE id = $1 FOR UPDATE",
        &[&run_id],
    )?;
    let batches = tx.query_one(
        "SELECT COUNT(*), COALESCE(MIN(batch_index), 0),
                COALESCE(MAX(batch_index), -1), COALESCE(SUM(row_count), 0)::BIGINT
           FROM jobs_global_ingestion_batches WHERE run_id = $1",
        &[&run_id],
    )?;
    let received_rows = run.get::<_, i64>(2);
    let received_batches = run.get::<_, i64>(3);
    validate_global_run_counters(
        &run.get::<_, String>(0),
        run.get(1),
        received_rows,
        received_batches,
        &run.get::<_, String>(4),
        batches.get(0),
        batches.get(1),
        batches.get(2),
        batches.get(3),
        input,
        artifact_sha256,
    )?;
    Ok((received_rows, received_batches))
}

#[allow(clippy::too_many_arguments)]
fn validate_global_run_counters(
    status: &str,
    expected_rows: i64,
    received_rows: i64,
    received_batches: i64,
    artifact: &str,
    batch_count: i64,
    minimum_index: i64,
    maximum_index: i64,
    summed_rows: i64,
    input: &GlobalIngestionCompleteInput,
    artifact_sha256: &str,
) -> Result<()> {
    if status != "running"
        || expected_rows != input.expected_rows
        || received_rows != input.expected_rows
        || received_batches != input.expected_batches
        || artifact != artifact_sha256
        || batch_count != input.expected_batches
        || minimum_index != 0
        || maximum_index != input.expected_batches - 1
        || summed_rows != input.expected_rows
    {
        anyhow::bail!("global discovery completion counters do not match stored batches")
    }
    Ok(())
}

fn expire_missing_global_memberships_sqlite(
    tx: &rusqlite::Transaction<'_>,
    source_id: &str,
    run_id: &str,
    complete_snapshot: bool,
    now: i64,
) -> Result<i64> {
    if !complete_snapshot {
        return Ok(0);
    }
    let stale = {
        let mut stmt = tx.prepare(
            "SELECT external_id, candidate_id, missing_count
               FROM jobs_global_candidate_memberships
              WHERE source_id = ?1 AND availability_status = 'active'
                AND last_seen_run_id <> ?2",
        )?;
        let values = stmt.query_map(params![source_id, run_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
        values
    };
    let mut expired = 0;
    let mut touched = BTreeSet::new();
    for (external_id, candidate_id, missing_count) in stale {
        let next_missing = missing_count.saturating_add(1);
        let next_status = if next_missing >= 2 { "expired" } else { "active" };
        tx.execute(
            "UPDATE jobs_global_candidate_memberships
                SET missing_count = ?3, missing_since_at_ms = COALESCE(missing_since_at_ms, ?4),
                    availability_status = ?5
              WHERE source_id = ?1 AND external_id = ?2",
            params![source_id, external_id, next_missing, now, next_status],
        )?;
        if next_status == "expired" {
            expired += 1;
        }
        touched.insert(candidate_id);
    }
    for candidate_id in touched {
        tx.execute(
            "UPDATE jobs_global_candidates
                SET availability_status = CASE WHEN EXISTS(
                        SELECT 1 FROM jobs_global_candidate_memberships membership
                         WHERE membership.candidate_id = ?1
                           AND membership.availability_status = 'active'
                    ) THEN 'unknown' ELSE 'expired' END,
                    updated_at_ms = ?2
              WHERE id = ?1",
            params![candidate_id, now],
        )?;
    }
    Ok(expired)
}

fn expire_missing_global_memberships_postgres(
    tx: &mut postgres::Transaction<'_>,
    source_id: &str,
    run_id: &str,
    complete_snapshot: bool,
    now: i64,
) -> Result<i64> {
    if !complete_snapshot {
        return Ok(0);
    }
    let stale = tx.query(
        "SELECT external_id, candidate_id, missing_count
           FROM jobs_global_candidate_memberships
          WHERE source_id = $1 AND availability_status = 'active'
            AND last_seen_run_id <> $2 FOR UPDATE",
        &[&source_id, &run_id],
    )?;
    let mut expired = 0;
    let mut touched = BTreeSet::new();
    for row in stale {
        let external_id = row.get::<_, String>(0);
        let candidate_id = row.get::<_, String>(1);
        let next_missing = row.get::<_, i64>(2).saturating_add(1);
        let next_status = if next_missing >= 2 { "expired" } else { "active" };
        tx.execute(
            "UPDATE jobs_global_candidate_memberships
                SET missing_count = $3, missing_since_at_ms = COALESCE(missing_since_at_ms, $4),
                    availability_status = $5
              WHERE source_id = $1 AND external_id = $2",
            &[&source_id, &external_id, &next_missing, &now, &next_status],
        )?;
        if next_status == "expired" {
            expired += 1;
        }
        touched.insert(candidate_id);
    }
    for candidate_id in touched {
        tx.execute(
            "UPDATE jobs_global_candidates
                SET availability_status = CASE WHEN EXISTS(
                        SELECT 1 FROM jobs_global_candidate_memberships membership
                         WHERE membership.candidate_id = $1
                           AND membership.availability_status = 'active'
                    ) THEN 'unknown' ELSE 'expired' END,
                    updated_at_ms = $2
              WHERE id = $1",
            &[&candidate_id, &now],
        )?;
    }
    Ok(expired)
}
