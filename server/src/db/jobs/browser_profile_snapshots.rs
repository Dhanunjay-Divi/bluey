pub const BROWSER_PROFILE_SNAPSHOT_CONTENT_TYPE: &str =
    "application/vnd.bluey.browser-profile+encrypted";

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
    validate_browser_profile_snapshot_lease_identity(
        lease,
        browser_profile_id,
        lease_token,
        fence,
    )?;
    if !matches!(lease.phase.as_str(), "prepared" | "click_started")
        || lease.lease_expires_at_ms <= now
    {
        return Err(ExecutionLeaseError::Conflict);
    }
    Ok(())
}

fn validate_browser_profile_snapshot_lease_identity(
    lease: &StoredExecutionLease,
    browser_profile_id: &str,
    lease_token: &str,
    fence: i64,
) -> ExecutionLeaseResult<()> {
    if lease.browser_profile_id != browser_profile_id
        || lease.fence != fence
        || !execution_lease_token_matches(&lease.lease_token_sha256, lease_token)
    {
        return Err(ExecutionLeaseError::Conflict);
    }
    Ok(())
}

struct ExactBrowserProfileSnapshot<'a> {
    generation: i64,
    object_key: &'a str,
    sha256: &'a str,
    size_bytes: i64,
    envelope_version: i64,
    run_id: &'a str,
    fence: i64,
}

fn browser_profile_snapshot_is_exact(
    current: &BrowserProfileSnapshotRecord,
    expected: &ExactBrowserProfileSnapshot<'_>,
) -> bool {
    current.generation == expected.generation
        && current.object_key == expected.object_key
        && current.sha256 == expected.sha256
        && current.size_bytes == expected.size_bytes
        && current.envelope_version == expected.envelope_version
        && current.writer_run_id == expected.run_id
        && current.writer_fence == expected.fence
}

/// Fence one snapshot-store operation before object I/O. Exact committed
/// replay is accepted even after lease expiry; a new operation must hold a
/// live lease, and receives a fresh lease window before reserving its immutable
/// upload record.
#[allow(clippy::too_many_arguments)]
pub fn authorize_browser_profile_snapshot_store(
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
) -> ExecutionLeaseResult<Option<BrowserProfileSnapshotRecord>> {
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
    let exact_snapshot = ExactBrowserProfileSnapshot {
        generation: next_generation,
        object_key,
        sha256,
        size_bytes,
        envelope_version,
        run_id,
        fence,
    };
    let lease_expires_at_ms = now.saturating_add(EXECUTION_LEASE_TTL_MS);
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            require_current_runner_volume_binding_sqlite_for_operation(
                &tx, account_id, run_id, now,
            )?;
            crate::db::object_uploads::require_active_account_write_fence_sqlite_tx(
                &tx, account_id,
            )?;
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
            validate_browser_profile_snapshot_lease_identity(
                &lease,
                browser_profile_id,
                lease_token,
                fence,
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
            if current
                .as_ref()
                .is_some_and(|current| browser_profile_snapshot_is_exact(current, &exact_snapshot))
            {
                tx.commit()?;
                return Ok(current);
            }
            validate_browser_profile_snapshot_lease(
                &lease,
                browser_profile_id,
                lease_token,
                fence,
                now,
            )?;
            if current.as_ref().map_or(0, |snapshot| snapshot.generation) != expected_generation {
                return Err(ExecutionLeaseError::Conflict);
            }
            if tx.execute(
                "UPDATE jobs_execution_leases
                    SET lease_expires_at_ms = ?6, updated_at_ms = ?7
                  WHERE run_id = ?1 AND account_id = ?2 AND application_id = ?3
                    AND lease_token_sha256 = ?4 AND fence = ?5 AND phase = ?8
                    AND lease_expires_at_ms > ?7",
                params![
                    run_id,
                    account_id,
                    application_id,
                    lease.lease_token_sha256,
                    fence,
                    lease_expires_at_ms,
                    now,
                    lease.phase,
                ],
            )? != 1
            {
                return Err(ExecutionLeaseError::Conflict);
            }
            tx.commit()?;
            Ok(current)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            require_current_runner_volume_binding_postgres_for_operation(
                &mut tx, account_id, run_id, now,
            )?;
            crate::db::object_uploads::require_active_account_write_fence_postgres_tx(
                &mut tx, account_id,
            )?;
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
            validate_browser_profile_snapshot_lease_identity(
                &lease,
                browser_profile_id,
                lease_token,
                fence,
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
            if current
                .as_ref()
                .is_some_and(|current| browser_profile_snapshot_is_exact(current, &exact_snapshot))
            {
                tx.commit()?;
                return Ok(current);
            }
            validate_browser_profile_snapshot_lease(
                &lease,
                browser_profile_id,
                lease_token,
                fence,
                now,
            )?;
            if current.as_ref().map_or(0, |snapshot| snapshot.generation) != expected_generation {
                return Err(ExecutionLeaseError::Conflict);
            }
            if tx.execute(
                "UPDATE jobs_execution_leases
                    SET lease_expires_at_ms = $6, updated_at_ms = $7
                  WHERE run_id = $1 AND account_id = $2 AND application_id = $3
                    AND lease_token_sha256 = $4 AND fence = $5 AND phase = $8
                    AND lease_expires_at_ms > $7",
                &[
                    &run_id,
                    &account_id,
                    &application_id,
                    &lease.lease_token_sha256,
                    &fence,
                    &lease_expires_at_ms,
                    &now,
                    &lease.phase,
                ],
            )? != 1
            {
                return Err(ExecutionLeaseError::Conflict);
            }
            tx.commit()?;
            Ok(current)
        }
    })
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
    validate_browser_profile_snapshot_request(browser_profile_id, None, None, None, None, None)?;
    let now = now_ms();
    match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            require_current_runner_volume_binding_sqlite_for_operation(
                &tx, account_id, run_id, now,
            )?;
            crate::db::object_uploads::require_active_account_write_fence_sqlite_tx(
                &tx, account_id,
            )?;
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
            let snapshot = tx
                .query_row(
                    "SELECT browser_profile_id, generation, object_key, sha256, size_bytes,
                        envelope_version, writer_run_id, writer_fence, updated_at_ms
                   FROM jobs_browser_profile_snapshots
                  WHERE account_id = ?1 AND browser_profile_id = ?2",
                    params![account_id, browser_profile_id],
                    browser_profile_snapshot_from_sqlite_row,
                )
                .optional()?;
            tx.commit()?;
            Ok(snapshot)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            require_current_runner_volume_binding_postgres_for_operation(
                &mut tx, account_id, run_id, now,
            )?;
            crate::db::object_uploads::require_active_account_write_fence_postgres_tx(
                &mut tx, account_id,
            )?;
            let lease = tx
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
            let snapshot = tx
                .query_opt(
                    "SELECT browser_profile_id, generation, object_key, sha256, size_bytes,
                            envelope_version, writer_run_id, writer_fence, updated_at_ms
                       FROM jobs_browser_profile_snapshots
                      WHERE account_id = $1 AND browser_profile_id = $2",
                    &[&account_id, &browser_profile_id],
                )?
                .map(browser_profile_snapshot_from_pg_row);
            tx.commit()?;
            Ok(snapshot)
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
    commit_browser_profile_snapshot_internal(
        pool,
        account_id,
        application_id,
        run_id,
        browser_profile_id,
        lease_token,
        fence,
        expected_generation,
        object_key,
        sha256,
        size_bytes,
        envelope_version,
        None,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn publish_browser_profile_snapshot_for_lease(
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
    upload_id: &str,
) -> ExecutionLeaseResult<BrowserProfileSnapshotRecord> {
    commit_browser_profile_snapshot_internal(
        pool,
        account_id,
        application_id,
        run_id,
        browser_profile_id,
        lease_token,
        fence,
        expected_generation,
        object_key,
        sha256,
        size_bytes,
        envelope_version,
        Some(upload_id),
    )
}

#[allow(clippy::too_many_arguments)]
fn commit_browser_profile_snapshot_internal(
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
    upload_id: Option<&str>,
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
    let exact_snapshot = ExactBrowserProfileSnapshot {
        generation: next_generation,
        object_key,
        sha256,
        size_bytes,
        envelope_version,
        run_id,
        fence,
    };
    match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            require_current_runner_volume_binding_sqlite_for_operation(
                &tx, account_id, run_id, now,
            )?;
            let published_upload = if let Some(upload_id) = upload_id {
                Some(crate::db::object_uploads::publish_account_object_sqlite_tx(
                    &tx, upload_id, account_id, object_key, now,
                )?)
            } else {
                crate::db::object_uploads::require_active_account_write_fence_sqlite_tx(
                    &tx, account_id,
                )?;
                None
            };
            if let Some(upload) = published_upload.as_ref() {
                validate_browser_profile_snapshot_upload(
                    upload,
                    application_id,
                    run_id,
                    browser_profile_id,
                    fence,
                    expected_generation,
                    next_generation,
                    object_key,
                    sha256,
                    size_bytes,
                    envelope_version,
                )?;
            }
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
            validate_browser_profile_snapshot_lease_identity(
                &lease,
                browser_profile_id,
                lease_token,
                fence,
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
            if let Some(current) = current
                .as_ref()
                .filter(|current| browser_profile_snapshot_is_exact(current, &exact_snapshot))
            {
                tx.commit()?;
                return Ok(current.clone());
            }
            if let Some(upload) = published_upload.as_ref() {
                if !matches!(lease.phase.as_str(), "prepared" | "click_started")
                    || upload.created_at_ms > lease.lease_expires_at_ms
                {
                    return Err(ExecutionLeaseError::Conflict);
                }
            } else {
                validate_browser_profile_snapshot_lease(
                    &lease,
                    browser_profile_id,
                    lease_token,
                    fence,
                    now,
                )?;
            }
            if current.as_ref().map_or(0, |snapshot| snapshot.generation) != expected_generation {
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
            if let Some(previous) = current
                .as_ref()
                .filter(|previous| previous.object_key != object_key)
            {
                let cleanup = crate::db::object_uploads::schedule_account_object_cleanup_sqlite_tx(
                    &tx,
                    account_id,
                    &previous.object_key,
                    now,
                )?;
                if cleanup.is_none() {
                    let adopted = legacy_browser_profile_snapshot_upload(account_id, previous, now);
                    crate::db::object_uploads::adopt_ready_account_object_sqlite_tx(&tx, &adopted)?;
                    if crate::db::object_uploads::schedule_account_object_cleanup_sqlite_tx(
                        &tx,
                        account_id,
                        &previous.object_key,
                        now,
                    )?
                    .is_none()
                    {
                        return Err(ExecutionLeaseError::Conflict);
                    }
                }
            }
            let committed = BrowserProfileSnapshotRecord {
                browser_profile_id: browser_profile_id.to_string(),
                generation: next_generation,
                object_key: object_key.to_string(),
                sha256: sha256.to_string(),
                size_bytes,
                envelope_version,
                writer_run_id: run_id.to_string(),
                writer_fence: fence,
                updated_at_ms: now,
            };
            tx.commit()?;
            Ok(committed)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            require_current_runner_volume_binding_postgres_for_operation(
                &mut tx, account_id, run_id, now,
            )?;
            let published_upload = if let Some(upload_id) = upload_id {
                Some(
                    crate::db::object_uploads::publish_account_object_postgres_tx(
                        &mut tx, upload_id, account_id, object_key, now,
                    )?,
                )
            } else {
                crate::db::object_uploads::require_active_account_write_fence_postgres_tx(
                    &mut tx, account_id,
                )?;
                None
            };
            if let Some(upload) = published_upload.as_ref() {
                validate_browser_profile_snapshot_upload(
                    upload,
                    application_id,
                    run_id,
                    browser_profile_id,
                    fence,
                    expected_generation,
                    next_generation,
                    object_key,
                    sha256,
                    size_bytes,
                    envelope_version,
                )?;
            }
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
            validate_browser_profile_snapshot_lease_identity(
                &lease,
                browser_profile_id,
                lease_token,
                fence,
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
            if let Some(current) = current
                .as_ref()
                .filter(|current| browser_profile_snapshot_is_exact(current, &exact_snapshot))
            {
                tx.commit()?;
                return Ok(current.clone());
            }
            if let Some(upload) = published_upload.as_ref() {
                if !matches!(lease.phase.as_str(), "prepared" | "click_started")
                    || upload.created_at_ms > lease.lease_expires_at_ms
                {
                    return Err(ExecutionLeaseError::Conflict);
                }
            } else {
                validate_browser_profile_snapshot_lease(
                    &lease,
                    browser_profile_id,
                    lease_token,
                    fence,
                    now,
                )?;
            }
            if current.as_ref().map_or(0, |snapshot| snapshot.generation) != expected_generation {
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
            if let Some(previous) = current
                .as_ref()
                .filter(|previous| previous.object_key != object_key)
            {
                let cleanup =
                    crate::db::object_uploads::schedule_account_object_cleanup_postgres_tx(
                        &mut tx,
                        account_id,
                        &previous.object_key,
                        now,
                    )?;
                if cleanup.is_none() {
                    let adopted = legacy_browser_profile_snapshot_upload(account_id, previous, now);
                    crate::db::object_uploads::adopt_ready_account_object_postgres_tx(
                        &mut tx, &adopted,
                    )?;
                    if crate::db::object_uploads::schedule_account_object_cleanup_postgres_tx(
                        &mut tx,
                        account_id,
                        &previous.object_key,
                        now,
                    )?
                    .is_none()
                    {
                        return Err(ExecutionLeaseError::Conflict);
                    }
                }
            }
            let committed = BrowserProfileSnapshotRecord {
                browser_profile_id: browser_profile_id.to_string(),
                generation: next_generation,
                object_key: object_key.to_string(),
                sha256: sha256.to_string(),
                size_bytes,
                envelope_version,
                writer_run_id: run_id.to_string(),
                writer_fence: fence,
                updated_at_ms: now,
            };
            tx.commit()?;
            Ok(committed)
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn validate_browser_profile_snapshot_upload(
    upload: &crate::db::object_uploads::ObjectUpload,
    application_id: &str,
    run_id: &str,
    browser_profile_id: &str,
    fence: i64,
    expected_generation: i64,
    next_generation: i64,
    object_key: &str,
    sha256: &str,
    size_bytes: i64,
    envelope_version: i64,
) -> ExecutionLeaseResult<()> {
    let metadata = serde_json::from_str::<Value>(&upload.metadata_json)
        .map_err(|_| ExecutionLeaseError::Conflict)?;
    let Some(metadata) = metadata.as_object() else {
        return Err(ExecutionLeaseError::Conflict);
    };
    if metadata.len() != 9
        || metadata.get("artifact_class").and_then(Value::as_str)
            != Some("jobs_browser_profile_snapshot")
        || metadata
            .get("jobs_browser_profile_id")
            .and_then(Value::as_str)
            != Some(browser_profile_id)
        || metadata.get("jobs_application_id").and_then(Value::as_str)
            != Some(application_id)
        || metadata.get("jobs_run_id").and_then(Value::as_str) != Some(run_id)
        || metadata.get("generation").and_then(Value::as_i64) != Some(next_generation)
        || metadata.get("expected_generation").and_then(Value::as_i64)
            != Some(expected_generation)
        || metadata.get("writer_fence").and_then(Value::as_i64) != Some(fence)
        || metadata.get("envelope_version").and_then(Value::as_i64)
            != Some(envelope_version)
        || metadata.get("retention_policy").and_then(Value::as_str)
            != Some("account_lifetime_until_deletion")
        || upload.logical_id
            != format!(
                "jobs-browser-profile:{browser_profile_id}:{next_generation}:{envelope_version}:{sha256}"
            )
        || upload.object_key != object_key
        || upload.sha256 != sha256
        || upload.size_bytes != size_bytes
        || upload.content_type != BROWSER_PROFILE_SNAPSHOT_CONTENT_TYPE
        || upload.expires_at_ms != i64::MAX
        || upload.state != "ready"
    {
        return Err(ExecutionLeaseError::Conflict);
    }
    Ok(())
}

fn legacy_browser_profile_snapshot_upload(
    account_id: &str,
    snapshot: &BrowserProfileSnapshotRecord,
    now_ms: i64,
) -> crate::db::object_uploads::NewObjectUpload {
    crate::db::object_uploads::NewObjectUpload {
        account_id: account_id.to_string(),
        object_kind: crate::db::object_uploads::ObjectKind::Artifact,
        logical_id: format!(
            "jobs-browser-profile:{}:{}:{}:{}",
            snapshot.browser_profile_id,
            snapshot.generation,
            snapshot.envelope_version,
            snapshot.sha256.to_ascii_lowercase()
        ),
        session_id: None,
        storage_scope: crate::db::object_uploads::StorageScope::Artifact,
        object_key: snapshot.object_key.clone(),
        size_bytes: snapshot.size_bytes,
        sha256: snapshot.sha256.to_ascii_lowercase(),
        content_type: BROWSER_PROFILE_SNAPSHOT_CONTENT_TYPE.to_string(),
        expires_at_ms: i64::MAX,
        metadata_json: json!({
            "artifact_class": "jobs_browser_profile_snapshot",
            "jobs_browser_profile_id": snapshot.browser_profile_id,
            "generation": snapshot.generation,
            "envelope_version": snapshot.envelope_version,
            "retention_policy": "account_lifetime_until_deletion",
        }),
        now_ms,
        limits: crate::object_storage::UploadLimits {
            max_object_bytes: i64::MAX,
            max_account_bytes: i64::MAX,
            max_daily_bytes: i64::MAX,
            max_account_objects: i64::MAX,
        },
    }
}
