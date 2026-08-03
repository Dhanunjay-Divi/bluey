fn browser_profile_snapshot_from_sqlite_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<BrowserProfileSnapshotRecord> {
    Ok(BrowserProfileSnapshotRecord {
        browser_profile_id: row.get(0)?,
        generation: row.get(1)?,
        object_key: row.get(2)?,
        sha256: row.get(3)?,
        size_bytes: row.get(4)?,
        envelope_version: row.get(5)?,
        writer_run_id: row.get(6)?,
        writer_fence: row.get(7)?,
        updated_at_ms: row.get(8)?,
    })
}

fn browser_profile_snapshot_from_pg_row(row: postgres::Row) -> BrowserProfileSnapshotRecord {
    BrowserProfileSnapshotRecord {
        browser_profile_id: row.get(0),
        generation: row.get(1),
        object_key: row.get(2),
        sha256: row.get(3),
        size_bytes: row.get(4),
        envelope_version: row.get(5),
        writer_run_id: row.get(6),
        writer_fence: row.get(7),
        updated_at_ms: row.get(8),
    }
}

fn validate_browser_profile_snapshot_request(
    browser_profile_id: &str,
    expected_generation: Option<i64>,
    object_key: Option<&str>,
    sha256: Option<&str>,
    size_bytes: Option<i64>,
    envelope_version: Option<i64>,
) -> ExecutionLeaseResult<()> {
    if !validate_execution_binding(browser_profile_id, 240)
        || expected_generation.is_some_and(|generation| generation < 0)
        || object_key.is_some_and(|key| !validate_execution_binding(key, 1_024))
        || sha256.is_some_and(|value| {
            value.len() != 64
                || !value
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        })
        || size_bytes.is_some_and(|value| value <= 0)
        || envelope_version.is_some_and(|value| value <= 0)
    {
        return Err(ExecutionLeaseError::InvalidRequest);
    }
    Ok(())
}

fn validate_browser_profile_snapshot_lease(
    lease: &StoredExecutionLease,
    browser_profile_id: &str,
    lease_token: &str,
    fence: i64,
    now: i64,
) -> ExecutionLeaseResult<()> {
    if lease.browser_profile_id != browser_profile_id
        || lease.fence != fence
        || !execution_lease_token_matches(&lease.lease_token_sha256, lease_token)
        || !matches!(lease.phase.as_str(), "prepared" | "click_started")
        || lease.lease_expires_at_ms <= now
    {
        return Err(ExecutionLeaseError::Conflict);
    }
    Ok(())
}

pub fn get_browser_profile_snapshot_for_lease(
    pool: &DbPool,
    account_id: &str,
    application_id: &str,
    run_id: &str,
    browser_profile_id: &str,
    lease_token: &str,
    fence: i64,
) -> ExecutionLeaseResult<Option<BrowserProfileSnapshotRecord>> {
    validate_execution_access(account_id, application_id, run_id, lease_token, fence)?;
    validate_browser_profile_snapshot_request(
        browser_profile_id,
        None,
        None,
        None,
        None,
        None,
    )?;
    let now = now_ms();
    match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get()?;
            let lease = conn
                .query_row(
                    "SELECT run_id, account_id, application_id, browser_profile_id,
                            owner_id, lease_token_sha256, fence, phase, lease_expires_at_ms
                       FROM jobs_execution_leases
                      WHERE run_id = ?1 AND account_id = ?2 AND application_id = ?3",
                    params![run_id, account_id, application_id],
                    execution_lease_from_sqlite_row,
                )
                .optional()?
                .ok_or(ExecutionLeaseError::NotFound)?;
            validate_browser_profile_snapshot_lease(
                &lease,
                browser_profile_id,
                lease_token,
                fence,
                now,
            )?;
            conn.query_row(
                "SELECT browser_profile_id, generation, object_key, sha256, size_bytes,
                        envelope_version, writer_run_id, writer_fence, updated_at_ms
                   FROM jobs_browser_profile_snapshots
                  WHERE account_id = ?1 AND browser_profile_id = ?2",
                params![account_id, browser_profile_id],
                browser_profile_snapshot_from_sqlite_row,
            )
            .optional()
            .map_err(ExecutionLeaseError::from)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let lease = conn
                .query_opt(
                    "SELECT run_id, account_id, application_id, browser_profile_id,
                            owner_id, lease_token_sha256, fence, phase, lease_expires_at_ms
                       FROM jobs_execution_leases
                      WHERE run_id = $1 AND account_id = $2 AND application_id = $3",
                    &[&run_id, &account_id, &application_id],
                )?
                .map(execution_lease_from_pg_row)
                .ok_or(ExecutionLeaseError::NotFound)?;
            validate_browser_profile_snapshot_lease(
                &lease,
                browser_profile_id,
                lease_token,
                fence,
                now,
            )?;
            Ok(conn
                .query_opt(
                    "SELECT browser_profile_id, generation, object_key, sha256, size_bytes,
                            envelope_version, writer_run_id, writer_fence, updated_at_ms
                       FROM jobs_browser_profile_snapshots
                      WHERE account_id = $1 AND browser_profile_id = $2",
                    &[&account_id, &browser_profile_id],
                )?
                .map(browser_profile_snapshot_from_pg_row))
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn commit_browser_profile_snapshot_for_lease(
    pool: &DbPool,
    account_id: &str,
    application_id: &str,
    run_id: &str,
    browser_profile_id: &str,
    lease_token: &str,
    fence: i64,
    expected_generation: i64,
    object_key: &str,
    sha256: &str,
    size_bytes: i64,
    envelope_version: i64,
) -> ExecutionLeaseResult<BrowserProfileSnapshotRecord> {
    validate_execution_access(account_id, application_id, run_id, lease_token, fence)?;
    validate_browser_profile_snapshot_request(
        browser_profile_id,
        Some(expected_generation),
        Some(object_key),
        Some(sha256),
        Some(size_bytes),
        Some(envelope_version),
    )?;
    let now = now_ms();
    let next_generation = expected_generation
        .checked_add(1)
        .ok_or(ExecutionLeaseError::InvalidRequest)?;
    match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let lease = tx
                .query_row(
                    "SELECT run_id, account_id, application_id, browser_profile_id,
                            owner_id, lease_token_sha256, fence, phase, lease_expires_at_ms
                       FROM jobs_execution_leases
                      WHERE run_id = ?1 AND account_id = ?2 AND application_id = ?3",
                    params![run_id, account_id, application_id],
                    execution_lease_from_sqlite_row,
                )
                .optional()?
                .ok_or(ExecutionLeaseError::NotFound)?;
            validate_browser_profile_snapshot_lease(
                &lease,
                browser_profile_id,
                lease_token,
                fence,
                now,
            )?;
            let current = tx
                .query_row(
                    "SELECT browser_profile_id, generation, object_key, sha256, size_bytes,
                            envelope_version, writer_run_id, writer_fence, updated_at_ms
                       FROM jobs_browser_profile_snapshots
                      WHERE account_id = ?1 AND browser_profile_id = ?2",
                    params![account_id, browser_profile_id],
                    browser_profile_snapshot_from_sqlite_row,
                )
                .optional()?;
            if let Some(current) = current.as_ref() {
                if current.generation == next_generation
                    && current.object_key == object_key
                    && current.sha256 == sha256
                    && current.size_bytes == size_bytes
                    && current.envelope_version == envelope_version
                    && current.writer_run_id == run_id
                    && current.writer_fence == fence
                {
                    return Ok(current.clone());
                }
            }
            if current.as_ref().map_or(0, |snapshot| snapshot.generation)
                != expected_generation
            {
                return Err(ExecutionLeaseError::Conflict);
            }
            tx.execute(
                "INSERT INTO jobs_browser_profile_snapshots
                    (account_id, browser_profile_id, generation, object_key, sha256, size_bytes,
                     envelope_version, writer_run_id, writer_fence, updated_at_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                 ON CONFLICT(account_id, browser_profile_id) DO UPDATE SET
                    generation = excluded.generation,
                    object_key = excluded.object_key,
                    sha256 = excluded.sha256,
                    size_bytes = excluded.size_bytes,
                    envelope_version = excluded.envelope_version,
                    writer_run_id = excluded.writer_run_id,
                    writer_fence = excluded.writer_fence,
                    updated_at_ms = excluded.updated_at_ms",
                params![
                    account_id,
                    browser_profile_id,
                    next_generation,
                    object_key,
                    sha256,
                    size_bytes,
                    envelope_version,
                    run_id,
                    fence,
                    now,
                ],
            )?;
            tx.commit()?;
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            let lock_scope = format!("{account_id}\0{browser_profile_id}");
            tx.query_one(
                "SELECT pg_advisory_xact_lock(hashtextextended($1, 0))",
                &[&lock_scope],
            )?;
            let lease = tx
                .query_opt(
                    "SELECT run_id, account_id, application_id, browser_profile_id,
                            owner_id, lease_token_sha256, fence, phase, lease_expires_at_ms
                       FROM jobs_execution_leases
                      WHERE run_id = $1 AND account_id = $2 AND application_id = $3
                      FOR UPDATE",
                    &[&run_id, &account_id, &application_id],
                )?
                .map(execution_lease_from_pg_row)
                .ok_or(ExecutionLeaseError::NotFound)?;
            validate_browser_profile_snapshot_lease(
                &lease,
                browser_profile_id,
                lease_token,
                fence,
                now,
            )?;
            let current = tx
                .query_opt(
                    "SELECT browser_profile_id, generation, object_key, sha256, size_bytes,
                            envelope_version, writer_run_id, writer_fence, updated_at_ms
                       FROM jobs_browser_profile_snapshots
                      WHERE account_id = $1 AND browser_profile_id = $2
                      FOR UPDATE",
                    &[&account_id, &browser_profile_id],
                )?
                .map(browser_profile_snapshot_from_pg_row);
            if let Some(current) = current.as_ref() {
                if current.generation == next_generation
                    && current.object_key == object_key
                    && current.sha256 == sha256
                    && current.size_bytes == size_bytes
                    && current.envelope_version == envelope_version
                    && current.writer_run_id == run_id
                    && current.writer_fence == fence
                {
                    return Ok(current.clone());
                }
            }
            if current.as_ref().map_or(0, |snapshot| snapshot.generation)
                != expected_generation
            {
                return Err(ExecutionLeaseError::Conflict);
            }
            tx.execute(
                "INSERT INTO jobs_browser_profile_snapshots
                    (account_id, browser_profile_id, generation, object_key, sha256, size_bytes,
                     envelope_version, writer_run_id, writer_fence, updated_at_ms)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
                 ON CONFLICT(account_id, browser_profile_id) DO UPDATE SET
                    generation = EXCLUDED.generation,
                    object_key = EXCLUDED.object_key,
                    sha256 = EXCLUDED.sha256,
                    size_bytes = EXCLUDED.size_bytes,
                    envelope_version = EXCLUDED.envelope_version,
                    writer_run_id = EXCLUDED.writer_run_id,
                    writer_fence = EXCLUDED.writer_fence,
                    updated_at_ms = EXCLUDED.updated_at_ms",
                &[
                    &account_id,
                    &browser_profile_id,
                    &next_generation,
                    &object_key,
                    &sha256,
                    &size_bytes,
                    &envelope_version,
                    &run_id,
                    &fence,
                    &now,
                ],
            )?;
            tx.commit()?;
        }
    }
    get_browser_profile_snapshot_for_lease(
        pool,
        account_id,
        application_id,
        run_id,
        browser_profile_id,
        lease_token,
        fence,
    )?
    .ok_or(ExecutionLeaseError::Conflict)
}
