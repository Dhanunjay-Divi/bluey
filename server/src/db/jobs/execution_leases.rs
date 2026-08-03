
type ExecutionLeaseResult<T> = std::result::Result<T, ExecutionLeaseError>;

#[derive(Debug)]
struct StoredExecutionLease {
    account_id: String,
    application_id: String,
    browser_profile_id: String,
    owner_id: String,
    lease_token_sha256: String,
    fence: i64,
    phase: String,
    lease_expires_at_ms: i64,
}

pub fn execution_browser_profile_id(account_id: &str, identity_id: &str) -> String {
    format!(
        "{}:{}",
        execution_scope_digest(account_id),
        execution_scope_digest(identity_id)
    )
}

fn execution_scope_digest(value: &str) -> String {
    hex::encode(Sha256::digest(value.as_bytes()))[..24].to_string()
}

fn validate_execution_binding(value: &str, max_len: usize) -> bool {
    !value.is_empty()
        && value.len() <= max_len
        && value.trim() == value
        && value.bytes().all(|byte| !byte.is_ascii_control())
}

fn validate_execution_access(
    account_id: &str,
    application_id: &str,
    run_id: &str,
    lease_token: &str,
    fence: i64,
) -> ExecutionLeaseResult<()> {
    if !validate_execution_binding(account_id, 240)
        || !validate_execution_binding(application_id, 240)
        || !validate_execution_binding(run_id, 240)
        || lease_token.is_empty()
        || lease_token.len() > 256
        || fence <= 0
    {
        return Err(ExecutionLeaseError::InvalidRequest);
    }
    Ok(())
}

fn random_execution_lease_token() -> String {
    let mut bytes = [0u8; 32];
    getrandom::getrandom(&mut bytes).expect("OS random source");
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

fn execution_lease_token_hash(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

fn execution_lease_token_matches(stored_hash: &str, token: &str) -> bool {
    let supplied_hash = execution_lease_token_hash(token);
    supplied_hash.len() == stored_hash.len()
        && supplied_hash
            .as_bytes()
            .ct_eq(stored_hash.as_bytes())
            .unwrap_u8()
            == 1
}

fn execution_lease_from_sqlite_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<StoredExecutionLease> {
    Ok(StoredExecutionLease {
        account_id: row.get(1)?,
        application_id: row.get(2)?,
        browser_profile_id: row.get(3)?,
        owner_id: row.get(4)?,
        lease_token_sha256: row.get(5)?,
        fence: row.get(6)?,
        phase: row.get(7)?,
        lease_expires_at_ms: row.get(8)?,
    })
}

fn execution_lease_from_pg_row(row: postgres::Row) -> StoredExecutionLease {
    StoredExecutionLease {
        account_id: row.get(1),
        application_id: row.get(2),
        browser_profile_id: row.get(3),
        owner_id: row.get(4),
        lease_token_sha256: row.get(5),
        fence: row.get(6),
        phase: row.get(7),
        lease_expires_at_ms: row.get(8),
    }
}

#[allow(clippy::too_many_arguments)]
fn validate_execution_target_payloads(
    application_raw: String,
    application_job_id: &str,
    application_state: &str,
    session_raw: String,
    session_runner: &str,
    identity_raw: String,
    identity_status: &str,
    application_id: &str,
    run_id: &str,
) -> ExecutionLeaseResult<()> {
    let application = parse_application_json(
        application_raw,
        application_id,
        application_job_id,
        "job application",
    )?;
    if application.run_id.as_deref() != Some(run_id) || application.state != application_state {
        return Err(ExecutionLeaseError::NotFound);
    }
    if !matches!(application_state, "queued" | "running" | "needs_input") {
        return Err(ExecutionLeaseError::Conflict);
    }

    let session: BrowserSession = parse_json(session_raw, "browser session")?;
    if session.id != run_id
        || session.application_id.as_deref() != Some(application_id)
        || session.runner != session_runner
        || session_runner != "cloud"
    {
        return Err(ExecutionLeaseError::NotFound);
    }

    let identity_id = application
        .receipt
        .pointer("/application_identity/id")
        .and_then(Value::as_str)
        .filter(|value| validate_execution_binding(value, 240))
        .ok_or(ExecutionLeaseError::Conflict)?;
    let frozen_email = application
        .receipt
        .pointer("/application_identity/email")
        .and_then(Value::as_str)
        .ok_or(ExecutionLeaseError::Conflict)?;
    if application
        .receipt
        .pointer("/application_identity/verified")
        .and_then(Value::as_bool)
        != Some(true)
        || identity_status != "verified"
    {
        return Err(ExecutionLeaseError::Conflict);
    }
    let identity: ApplicationIdentity = parse_json(identity_raw, "application identity")?;
    if identity.id != identity_id || identity.email != frozen_email {
        return Err(ExecutionLeaseError::Conflict);
    }
    Ok(())
}

fn sqlite_execution_target(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    application_id: &str,
    run_id: &str,
) -> ExecutionLeaseResult<String> {
    let (application_job_id, application_raw, application_state): (String, String, String) = tx
        .query_row(
            "SELECT job_id, application_json, state FROM jobs_applications
              WHERE account_id = ?1 AND id = ?2",
            params![account_id, application_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?
        .ok_or(ExecutionLeaseError::NotFound)?;
    let application = parse_application_json(
        application_raw.clone(),
        application_id,
        &application_job_id,
        "job application",
    )?;
    let identity_id = application
        .receipt
        .pointer("/application_identity/id")
        .and_then(Value::as_str)
        .ok_or(ExecutionLeaseError::Conflict)?;
    let (session_raw, session_runner): (String, String) = tx
        .query_row(
            "SELECT session_json, runner FROM jobs_browser_sessions
              WHERE account_id = ?1 AND id = ?2",
            params![account_id, run_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?
        .ok_or(ExecutionLeaseError::NotFound)?;
    let (identity_raw, identity_status): (String, String) = tx
        .query_row(
            "SELECT identity_json, verification_status
               FROM jobs_application_identities
              WHERE account_id = ?1 AND id = ?2",
            params![account_id, identity_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?
        .ok_or(ExecutionLeaseError::Conflict)?;
    validate_execution_target_payloads(
        application_raw,
        &application_job_id,
        &application_state,
        session_raw,
        &session_runner,
        identity_raw,
        &identity_status,
        application_id,
        run_id,
    )?;
    if !current_execution_authorized_sqlite(
        tx,
        account_id,
        &application,
        ExecutionAuthorityRunner::Cloud,
    )? {
        return Err(ExecutionLeaseError::Conflict);
    }
    Ok(execution_browser_profile_id(account_id, identity_id))
}

fn postgres_execution_target(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    application_id: &str,
    run_id: &str,
) -> ExecutionLeaseResult<String> {
    let row = tx
        .query_opt(
            "SELECT job_id, application_json, state FROM jobs_applications
              WHERE account_id = $1 AND id = $2 FOR UPDATE",
            &[&account_id, &application_id],
        )?
        .ok_or(ExecutionLeaseError::NotFound)?;
    let application_job_id: String = row.get(0);
    let application_raw: String = row.get(1);
    let application_state: String = row.get(2);
    let application = parse_application_json(
        application_raw.clone(),
        application_id,
        &application_job_id,
        "job application",
    )?;
    let identity_id = application
        .receipt
        .pointer("/application_identity/id")
        .and_then(Value::as_str)
        .ok_or(ExecutionLeaseError::Conflict)?
        .to_string();
    let row = tx
        .query_opt(
            "SELECT session_json, runner FROM jobs_browser_sessions
              WHERE account_id = $1 AND id = $2",
            &[&account_id, &run_id],
        )?
        .ok_or(ExecutionLeaseError::NotFound)?;
    let session_raw: String = row.get(0);
    let session_runner: String = row.get(1);
    let row = tx
        .query_opt(
            "SELECT identity_json, verification_status
               FROM jobs_application_identities
              WHERE account_id = $1 AND id = $2",
            &[&account_id, &identity_id],
        )?
        .ok_or(ExecutionLeaseError::Conflict)?;
    let identity_raw: String = row.get(0);
    let identity_status: String = row.get(1);
    validate_execution_target_payloads(
        application_raw,
        &application_job_id,
        &application_state,
        session_raw,
        &session_runner,
        identity_raw,
        &identity_status,
        application_id,
        run_id,
    )?;
    if !current_execution_authorized_postgres(
        tx,
        account_id,
        &application,
        ExecutionAuthorityRunner::Cloud,
    )? {
        return Err(ExecutionLeaseError::Conflict);
    }
    Ok(execution_browser_profile_id(account_id, &identity_id))
}

fn sqlite_next_execution_fence(
    tx: &rusqlite::Transaction<'_>,
    application_id: &str,
    browser_profile_id: &str,
) -> ExecutionLeaseResult<i64> {
    let current: i64 = tx.query_row(
        "SELECT COALESCE(MAX(fence), 0) FROM jobs_execution_leases
          WHERE application_id = ?1 OR browser_profile_id = ?2",
        params![application_id, browser_profile_id],
        |row| row.get(0),
    )?;
    current.checked_add(1).ok_or(ExecutionLeaseError::Conflict)
}

fn postgres_next_execution_fence(
    tx: &mut postgres::Transaction<'_>,
    application_id: &str,
    browser_profile_id: &str,
) -> ExecutionLeaseResult<i64> {
    let current: i64 = tx
        .query_one(
            "SELECT COALESCE(MAX(fence), 0) FROM jobs_execution_leases
              WHERE application_id = $1 OR browser_profile_id = $2",
            &[&application_id, &browser_profile_id],
        )?
        .get(0);
    current.checked_add(1).ok_or(ExecutionLeaseError::Conflict)
}

fn sqlite_bind_cloud_attempt(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    application_id: &str,
    now: i64,
) -> ExecutionLeaseResult<()> {
    let updated = tx.execute(
        "UPDATE jobs_attempt_reservations
            SET runner = 'cloud', updated_at_ms = ?3
          WHERE account_id = ?1 AND application_id = ?2
            AND status IN ('reserved', 'running')
            AND runner IN ('unassigned', 'cloud')",
        params![account_id, application_id, now],
    )?;
    if updated != 1 {
        return Err(ExecutionLeaseError::Conflict);
    }
    Ok(())
}

fn postgres_bind_cloud_attempt(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    application_id: &str,
    now: i64,
) -> ExecutionLeaseResult<()> {
    let updated = tx.execute(
        "UPDATE jobs_attempt_reservations
            SET runner = 'cloud', updated_at_ms = $3
          WHERE account_id = $1 AND application_id = $2
            AND status IN ('reserved', 'running')
            AND runner IN ('unassigned', 'cloud')",
        &[&account_id, &application_id, &now],
    )?;
    if updated != 1 {
        return Err(ExecutionLeaseError::Conflict);
    }
    Ok(())
}

pub fn claim_execution_lease(
    pool: &DbPool,
    account_id: &str,
    application_id: &str,
    run_id: &str,
    supplied_browser_profile_id: &str,
    owner_id: &str,
) -> ExecutionLeaseResult<ExecutionLeaseGrant> {
    if !validate_execution_binding(account_id, 240)
        || !validate_execution_binding(application_id, 240)
        || !validate_execution_binding(run_id, 240)
        || !validate_execution_binding(supplied_browser_profile_id, 160)
        || !validate_execution_binding(owner_id, 240)
    {
        return Err(ExecutionLeaseError::InvalidRequest);
    }
    let lease_token = random_execution_lease_token();
    let lease_token_sha256 = execution_lease_token_hash(&lease_token);
    let now = now_ms();
    let lease_expires_at_ms = now.saturating_add(EXECUTION_LEASE_TTL_MS);

    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let browser_profile_id =
                sqlite_execution_target(&tx, account_id, application_id, run_id)?;
            if browser_profile_id != supplied_browser_profile_id {
                return Err(ExecutionLeaseError::Conflict);
            }
            sqlite_bind_cloud_attempt(&tx, account_id, application_id, now)?;
            let existing = tx
                .query_row(
                    "SELECT run_id, account_id, application_id, browser_profile_id,
                            owner_id, lease_token_sha256, fence, phase, lease_expires_at_ms
                       FROM jobs_execution_leases WHERE run_id = ?1",
                    params![run_id],
                    execution_lease_from_sqlite_row,
                )
                .optional()?;
            let fence = if let Some(existing) = existing {
                if existing.account_id != account_id
                    || existing.application_id != application_id
                    || existing.browser_profile_id != browser_profile_id
                    || existing.phase != "prepared"
                    || (existing.lease_expires_at_ms > now && existing.owner_id != owner_id)
                {
                    return Err(ExecutionLeaseError::Conflict);
                }
                let fence = sqlite_next_execution_fence(&tx, application_id, &browser_profile_id)?;
                if tx.execute(
                    "UPDATE jobs_execution_leases
                        SET owner_id = ?2, lease_token_sha256 = ?3, fence = ?4,
                            lease_expires_at_ms = ?5, updated_at_ms = ?6
                      WHERE run_id = ?1 AND phase = 'prepared' AND fence = ?7",
                    params![
                        run_id,
                        owner_id,
                        lease_token_sha256,
                        fence,
                        lease_expires_at_ms,
                        now,
                        existing.fence,
                    ],
                )? != 1
                {
                    return Err(ExecutionLeaseError::Conflict);
                }
                fence
            } else {
                tx.execute(
                    "UPDATE jobs_execution_leases
                        SET phase = 'released', updated_at_ms = ?3, finished_at_ms = ?3
                      WHERE run_id <> ?1
                        AND (application_id = ?2 OR browser_profile_id = ?4)
                        AND phase = 'prepared' AND lease_expires_at_ms <= ?3",
                    params![run_id, application_id, now, browser_profile_id],
                )?;
                let active: bool = tx.query_row(
                    "SELECT EXISTS(
                        SELECT 1 FROM jobs_execution_leases
                         WHERE (application_id = ?1 OR browser_profile_id = ?2)
                           AND phase IN ('prepared', 'click_started')
                    )",
                    params![application_id, browser_profile_id],
                    |row| row.get(0),
                )?;
                if active {
                    return Err(ExecutionLeaseError::Conflict);
                }
                let fence = sqlite_next_execution_fence(&tx, application_id, &browser_profile_id)?;
                if let Err(error) = tx.execute(
                    "INSERT INTO jobs_execution_leases (
                        run_id, account_id, application_id, browser_profile_id, owner_id,
                        lease_token_sha256, fence, phase, lease_expires_at_ms,
                        created_at_ms, updated_at_ms
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'prepared', ?8, ?9, ?9)",
                    params![
                        run_id,
                        account_id,
                        application_id,
                        browser_profile_id,
                        owner_id,
                        lease_token_sha256,
                        fence,
                        lease_expires_at_ms,
                        now,
                    ],
                ) {
                    if error.sqlite_error_code() == Some(rusqlite::ErrorCode::ConstraintViolation) {
                        return Err(ExecutionLeaseError::Conflict);
                    }
                    return Err(error.into());
                }
                fence
            };
            tx.commit()?;
            Ok(ExecutionLeaseGrant {
                run_id: run_id.to_string(),
                lease_token,
                fence,
                lease_expires_at_ms,
                phase: "prepared".to_string(),
            })
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            let browser_profile_id =
                postgres_execution_target(&mut tx, account_id, application_id, run_id)?;
            if browser_profile_id != supplied_browser_profile_id {
                return Err(ExecutionLeaseError::Conflict);
            }
            postgres_bind_cloud_attempt(&mut tx, account_id, application_id, now)?;
            let existing = tx
                .query_opt(
                    "SELECT run_id, account_id, application_id, browser_profile_id,
                            owner_id, lease_token_sha256, fence, phase, lease_expires_at_ms
                       FROM jobs_execution_leases WHERE run_id = $1 FOR UPDATE",
                    &[&run_id],
                )?
                .map(execution_lease_from_pg_row);
            let fence = if let Some(existing) = existing {
                if existing.account_id != account_id
                    || existing.application_id != application_id
                    || existing.browser_profile_id != browser_profile_id
                    || existing.phase != "prepared"
                    || (existing.lease_expires_at_ms > now && existing.owner_id != owner_id)
                {
                    return Err(ExecutionLeaseError::Conflict);
                }
                let fence =
                    postgres_next_execution_fence(&mut tx, application_id, &browser_profile_id)?;
                if tx.execute(
                    "UPDATE jobs_execution_leases
                        SET owner_id = $2, lease_token_sha256 = $3, fence = $4,
                            lease_expires_at_ms = $5, updated_at_ms = $6
                      WHERE run_id = $1 AND phase = 'prepared' AND fence = $7",
                    &[
                        &run_id,
                        &owner_id,
                        &lease_token_sha256,
                        &fence,
                        &lease_expires_at_ms,
                        &now,
                        &existing.fence,
                    ],
                )? != 1
                {
                    return Err(ExecutionLeaseError::Conflict);
                }
                fence
            } else {
                tx.execute(
                    "UPDATE jobs_execution_leases
                        SET phase = 'released', updated_at_ms = $3, finished_at_ms = $3
                      WHERE run_id <> $1
                        AND (application_id = $2 OR browser_profile_id = $4)
                        AND phase = 'prepared' AND lease_expires_at_ms <= $3",
                    &[&run_id, &application_id, &now, &browser_profile_id],
                )?;
                let active: bool = tx
                    .query_one(
                        "SELECT EXISTS(
                            SELECT 1 FROM jobs_execution_leases
                             WHERE (application_id = $1 OR browser_profile_id = $2)
                               AND phase IN ('prepared', 'click_started')
                        )",
                        &[&application_id, &browser_profile_id],
                    )?
                    .get(0);
                if active {
                    return Err(ExecutionLeaseError::Conflict);
                }
                let fence =
                    postgres_next_execution_fence(&mut tx, application_id, &browser_profile_id)?;
                if let Err(error) = tx.execute(
                    "INSERT INTO jobs_execution_leases (
                        run_id, account_id, application_id, browser_profile_id, owner_id,
                        lease_token_sha256, fence, phase, lease_expires_at_ms,
                        created_at_ms, updated_at_ms
                     ) VALUES ($1, $2, $3, $4, $5, $6, $7, 'prepared', $8, $9, $9)",
                    &[
                        &run_id,
                        &account_id,
                        &application_id,
                        &browser_profile_id,
                        &owner_id,
                        &lease_token_sha256,
                        &fence,
                        &lease_expires_at_ms,
                        &now,
                    ],
                ) {
                    if error.code() == Some(&postgres::error::SqlState::UNIQUE_VIOLATION) {
                        return Err(ExecutionLeaseError::Conflict);
                    }
                    return Err(error.into());
                }
                fence
            };
            tx.commit()?;
            Ok(ExecutionLeaseGrant {
                run_id: run_id.to_string(),
                lease_token,
                fence,
                lease_expires_at_ms,
                phase: "prepared".to_string(),
            })
        }
    })
}

pub fn heartbeat_execution_lease(
    pool: &DbPool,
    account_id: &str,
    application_id: &str,
    run_id: &str,
    lease_token: &str,
    fence: i64,
) -> ExecutionLeaseResult<ExecutionLeaseRecord> {
    validate_execution_access(account_id, application_id, run_id, lease_token, fence)?;
    let now = now_ms();
    let lease_expires_at_ms = now.saturating_add(EXECUTION_LEASE_TTL_MS);
    crate::db::run_blocking_db(|| match pool {
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
            if lease.fence != fence
                || !execution_lease_token_matches(&lease.lease_token_sha256, lease_token)
                || !matches!(lease.phase.as_str(), "prepared" | "click_started")
                || lease.lease_expires_at_ms <= now
            {
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
            Ok(ExecutionLeaseRecord {
                run_id: run_id.to_string(),
                fence,
                lease_expires_at_ms,
                phase: lease.phase,
            })
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
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
            if lease.fence != fence
                || !execution_lease_token_matches(&lease.lease_token_sha256, lease_token)
                || !matches!(lease.phase.as_str(), "prepared" | "click_started")
                || lease.lease_expires_at_ms <= now
            {
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
            Ok(ExecutionLeaseRecord {
                run_id: run_id.to_string(),
                fence,
                lease_expires_at_ms,
                phase: lease.phase,
            })
        }
    })
}

pub fn start_irreversible_submission(
    pool: &DbPool,
    account_id: &str,
    application_id: &str,
    run_id: &str,
    lease_token: &str,
    fence: i64,
) -> ExecutionLeaseResult<ExecutionLeaseRecord> {
    validate_execution_access(account_id, application_id, run_id, lease_token, fence)?;
    let now = now_ms();
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let browser_profile_id =
                sqlite_execution_target(&tx, account_id, application_id, run_id)?;
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
            if lease.browser_profile_id != browser_profile_id
                || lease.fence != fence
                || !execution_lease_token_matches(&lease.lease_token_sha256, lease_token)
                || lease.phase != "prepared"
                || lease.lease_expires_at_ms <= now
            {
                return Err(ExecutionLeaseError::Conflict);
            }
            if tx.execute(
                "UPDATE jobs_execution_leases
                    SET phase = 'click_started', updated_at_ms = ?6
                  WHERE run_id = ?1 AND account_id = ?2 AND application_id = ?3
                    AND lease_token_sha256 = ?4 AND fence = ?5
                    AND phase = 'prepared' AND lease_expires_at_ms > ?6",
                params![
                    run_id,
                    account_id,
                    application_id,
                    lease.lease_token_sha256,
                    fence,
                    now,
                ],
            )? != 1
            {
                return Err(ExecutionLeaseError::Conflict);
            }
            let lease_expires_at_ms = lease.lease_expires_at_ms;
            tx.commit()?;
            Ok(ExecutionLeaseRecord {
                run_id: run_id.to_string(),
                fence,
                lease_expires_at_ms,
                phase: "click_started".to_string(),
            })
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            let browser_profile_id =
                postgres_execution_target(&mut tx, account_id, application_id, run_id)?;
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
            if lease.browser_profile_id != browser_profile_id
                || lease.fence != fence
                || !execution_lease_token_matches(&lease.lease_token_sha256, lease_token)
                || lease.phase != "prepared"
                || lease.lease_expires_at_ms <= now
            {
                return Err(ExecutionLeaseError::Conflict);
            }
            if tx.execute(
                "UPDATE jobs_execution_leases
                    SET phase = 'click_started', updated_at_ms = $6
                  WHERE run_id = $1 AND account_id = $2 AND application_id = $3
                    AND lease_token_sha256 = $4 AND fence = $5
                    AND phase = 'prepared' AND lease_expires_at_ms > $6",
                &[
                    &run_id,
                    &account_id,
                    &application_id,
                    &lease.lease_token_sha256,
                    &fence,
                    &now,
                ],
            )? != 1
            {
                return Err(ExecutionLeaseError::Conflict);
            }
            let lease_expires_at_ms = lease.lease_expires_at_ms;
            tx.commit()?;
            Ok(ExecutionLeaseRecord {
                run_id: run_id.to_string(),
                fence,
                lease_expires_at_ms,
                phase: "click_started".to_string(),
            })
        }
    })
}

fn execution_finish_allowed(phase: &str, outcome: &str) -> bool {
    match phase {
        "prepared" => matches!(outcome, "failed" | "released"),
        "click_started" => matches!(outcome, "submitted" | "side_effect_unknown"),
        _ => false,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CheckpointRecoveryOutcome {
    Released,
    SideEffectUnknown,
}

impl CheckpointRecoveryOutcome {
    fn lease_phase(self) -> &'static str {
        match self {
            Self::Released => "released",
            Self::SideEffectUnknown => "side_effect_unknown",
        }
    }

    fn application_state(self) -> &'static str {
        match self {
            Self::Released => "failed",
            Self::SideEffectUnknown => "side_effect_unknown",
        }
    }

    fn browser_status(self) -> &'static str {
        match self {
            Self::Released => "failed",
            Self::SideEffectUnknown => "needs_input",
        }
    }

    fn browser_step(self) -> &'static str {
        match self {
            Self::Released => "Browser run stopped before submission",
            Self::SideEffectUnknown => "Submission outcome needs reconciliation",
        }
    }

    fn attempt_status(self) -> &'static str {
        match self {
            Self::Released => "released",
            Self::SideEffectUnknown => "side_effect_unknown",
        }
    }
}

struct CheckpointReconciliationPlan {
    outcome: CheckpointRecoveryOutcome,
    application: JobApplication,
    browser_session: BrowserSession,
}

struct CheckpointReconciliationRequest<'a> {
    account_id: &'a str,
    application_id: &'a str,
    run_id: &'a str,
    owner_id: &'a str,
    lease_token: Option<&'a str>,
    fence: i64,
    checkpoint_version: i64,
    checkpoint_phase: &'a str,
}

fn validate_checkpoint_reconciliation_request(
    request: &CheckpointReconciliationRequest<'_>,
) -> ExecutionLeaseResult<()> {
    if !validate_execution_binding(request.account_id, 240)
        || !validate_execution_binding(request.application_id, 240)
        || !validate_execution_binding(request.run_id, 240)
        || !validate_execution_binding(request.owner_id, 240)
        || request.fence <= 0
        || !matches!(request.checkpoint_version, 1 | 2)
        || !matches!(
            request.checkpoint_phase,
            "prepared"
                | "needs_input"
                | "provider_review"
                | "final_submit_started"
                | "final_submit_activated"
                | "side_effect_unknown"
        )
        || request
            .lease_token
            .is_some_and(|token| token.is_empty() || token.len() > 256)
        || (request.checkpoint_version == 2 && request.lease_token.is_none())
    {
        return Err(ExecutionLeaseError::InvalidRequest);
    }
    Ok(())
}

fn checkpoint_recovery_outcome(
    checkpoint_phase: &str,
    lease_phase: &str,
) -> ExecutionLeaseResult<CheckpointRecoveryOutcome> {
    if matches!(
        checkpoint_phase,
        "final_submit_started" | "final_submit_activated" | "side_effect_unknown"
    ) || matches!(
        lease_phase,
        "click_started" | "submitted" | "side_effect_unknown"
    ) {
        return Ok(CheckpointRecoveryOutcome::SideEffectUnknown);
    }
    if matches!(checkpoint_phase, "prepared" | "needs_input" | "provider_review")
        && matches!(lease_phase, "prepared" | "failed" | "released")
    {
        return Ok(CheckpointRecoveryOutcome::Released);
    }
    Err(ExecutionLeaseError::Conflict)
}

fn trusted_submitted_application(application: &JobApplication, evidence_count: i64) -> bool {
    application.state == "submitted"
        && application.submitted_at_ms.is_some_and(|value| value > 0)
        && application
            .receipt
            .get("_bluey_server_submission_fingerprint_v1")
            .and_then(Value::as_str)
            .is_some_and(|fingerprint| {
                fingerprint.len() == 64
                    && fingerprint.bytes().all(|byte| byte.is_ascii_hexdigit())
            })
        && evidence_count > 0
}

fn validate_checkpoint_lease_access(
    lease: &StoredExecutionLease,
    owner_id: &str,
    lease_token: Option<&str>,
    fence: i64,
    checkpoint_version: i64,
) -> ExecutionLeaseResult<()> {
    let token_matches = match (checkpoint_version, lease_token) {
        (1, None) => true,
        (_, Some(token)) => execution_lease_token_matches(&lease.lease_token_sha256, token),
        _ => false,
    };
    if lease.owner_id != owner_id || lease.fence != fence || !token_matches {
        return Err(ExecutionLeaseError::Conflict);
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn exact_checkpoint_recovery_replay(
    account_id: &str,
    application_id: &str,
    run_id: &str,
    lease: &StoredExecutionLease,
    checkpoint_version: i64,
    checkpoint_phase: &str,
    application_job_id: &str,
    application_raw: &str,
    application_state: &str,
    session_raw: &str,
    session_runner: &str,
    session_status: &str,
    attempt: Option<&AttemptReservation>,
) -> ExecutionLeaseResult<Option<CheckpointRecoveryOutcome>> {
    let application = parse_application_json(
        application_raw.to_string(),
        application_id,
        application_job_id,
        "job application",
    )?;
    let Some(recovery) = application.receipt.get("cloud_recovery") else {
        return Ok(None);
    };
    let outcome = match recovery.get("status").and_then(Value::as_str) {
        Some("released") => CheckpointRecoveryOutcome::Released,
        Some("side_effect_unknown") => CheckpointRecoveryOutcome::SideEffectUnknown,
        _ => return Err(ExecutionLeaseError::Conflict),
    };
    let identity_id = application
        .receipt
        .pointer("/application_identity/id")
        .and_then(Value::as_str)
        .filter(|value| validate_execution_binding(value, 240))
        .ok_or(ExecutionLeaseError::Conflict)?;
    let browser_session: BrowserSession =
        parse_json(session_raw.to_string(), "browser session")?;
    if recovery.get("schema_version").and_then(Value::as_i64) != Some(1)
        || recovery.get("run_id").and_then(Value::as_str) != Some(run_id)
        || recovery.get("checkpoint_version").and_then(Value::as_i64)
            != Some(checkpoint_version)
        || recovery.get("checkpoint_phase").and_then(Value::as_str) != Some(checkpoint_phase)
        || application.id != application_id
        || application.state != application_state
        || application.state != outcome.application_state()
        || application.run_id.as_deref() != Some(run_id)
        || application.submitted_at_ms.is_some()
        || lease.account_id != account_id
        || lease.application_id != application_id
        || lease.phase != outcome.lease_phase()
        || lease.browser_profile_id != execution_browser_profile_id(account_id, identity_id)
        || browser_session.id != run_id
        || browser_session.application_id.as_deref() != Some(application_id)
        || browser_session.runner != session_runner
        || session_runner != "cloud"
        || browser_session.status != session_status
        || session_status != outcome.browser_status()
        || browser_session.current_step != outcome.browser_step()
        || browser_session.takeover_url.is_some()
        || attempt.is_some_and(|attempt| {
            attempt.application_id != application_id
                || attempt.runner != "cloud"
                || attempt.status != outcome.attempt_status()
        })
    {
        return Err(ExecutionLeaseError::Conflict);
    }
    Ok(Some(outcome))
}

fn validate_trusted_submitted_checkpoint(
    request: &CheckpointReconciliationRequest<'_>,
    lease: &StoredExecutionLease,
    application_job_id: &str,
    application_raw: String,
    application_state: &str,
    evidence_count: i64,
) -> ExecutionLeaseResult<bool> {
    if application_state != "submitted" {
        return Ok(false);
    }
    let application = parse_application_json(
        application_raw,
        request.application_id,
        application_job_id,
        "job application",
    )?;
    let identity_id = application
        .receipt
        .pointer("/application_identity/id")
        .and_then(Value::as_str)
        .filter(|value| validate_execution_binding(value, 240))
        .ok_or(ExecutionLeaseError::Conflict)?;
    if application.run_id.as_deref() != Some(request.run_id)
        || lease.browser_profile_id
            != execution_browser_profile_id(request.account_id, identity_id)
        || !trusted_submitted_application(&application, evidence_count)
    {
        return Err(ExecutionLeaseError::Conflict);
    }
    Ok(true)
}

#[allow(clippy::too_many_arguments)]
fn checkpoint_reconciliation_plan(
    account_id: &str,
    application_id: &str,
    run_id: &str,
    lease: &StoredExecutionLease,
    checkpoint_version: i64,
    checkpoint_phase: &str,
    application_job_id: &str,
    application_raw: String,
    application_state: &str,
    session_raw: String,
    session_runner: &str,
    session_status: &str,
    attempt: Option<&AttemptReservation>,
    now: i64,
) -> ExecutionLeaseResult<CheckpointReconciliationPlan> {
    if lease.account_id != account_id || lease.application_id != application_id {
        return Err(ExecutionLeaseError::NotFound);
    }
    let mut application = parse_application_json(
        application_raw,
        application_id,
        application_job_id,
        "job application",
    )?;
    if application.id != application_id
        || application.state != application_state
        || application.run_id.as_deref() != Some(run_id)
    {
        return Err(ExecutionLeaseError::NotFound);
    }
    if !matches!(
        application_state,
        "queued" | "running" | "needs_input" | "failed" | "side_effect_unknown"
    ) {
        return Err(ExecutionLeaseError::Conflict);
    }

    let identity_id = application
        .receipt
        .pointer("/application_identity/id")
        .and_then(Value::as_str)
        .filter(|value| validate_execution_binding(value, 240))
        .ok_or(ExecutionLeaseError::Conflict)?;
    if lease.browser_profile_id != execution_browser_profile_id(account_id, identity_id) {
        return Err(ExecutionLeaseError::Conflict);
    }

    let mut browser_session: BrowserSession = parse_json(session_raw, "browser session")?;
    if browser_session.id != run_id
        || browser_session.application_id.as_deref() != Some(application_id)
        || browser_session.runner != session_runner
        || session_runner != "cloud"
        || browser_session.status != session_status
        || !matches!(
            session_status,
            "queued" | "running" | "needs_input" | "failed" | "complete"
        )
    {
        return Err(ExecutionLeaseError::Conflict);
    }
    if let Some(attempt) = attempt {
        if attempt.application_id != application_id
            || attempt.runner != "cloud"
            || !matches!(
                attempt.status.as_str(),
                "reserved" | "running" | "released" | "side_effect_unknown" | "submitted"
            )
        {
            return Err(ExecutionLeaseError::Conflict);
        }
    }

    let outcome = checkpoint_recovery_outcome(checkpoint_phase, &lease.phase)?;
    if !application.receipt.is_object() {
        application.receipt = json!({});
    }
    let recovery = application
        .receipt
        .get("cloud_recovery")
        .filter(|value| {
            value.get("run_id").and_then(Value::as_str) == Some(run_id)
                && value.get("status").and_then(Value::as_str) == Some(outcome.lease_phase())
        })
        .cloned()
        .unwrap_or_else(|| {
            json!({
                "schema_version": 1,
                "status": outcome.lease_phase(),
                "checkpoint_version": checkpoint_version,
                "checkpoint_phase": checkpoint_phase,
                "lease_phase": lease.phase,
                "recorded_at_ms": now,
                "run_id": run_id,
            })
        });
    application
        .receipt
        .as_object_mut()
        .expect("receipt normalized above")
        .insert("cloud_recovery".to_string(), recovery);
    application.state = outcome.application_state().to_string();
    application.updated_at_ms = now;
    application.submitted_at_ms = None;

    browser_session.status = outcome.browser_status().to_string();
    browser_session.current_step = outcome.browser_step().to_string();
    browser_session.takeover_url = None;
    browser_session.updated_at_ms = now;

    Ok(CheckpointReconciliationPlan {
        outcome,
        application,
        browser_session,
    })
}

pub fn execution_lease_phase_for_application(
    pool: &DbPool,
    account_id: &str,
    application_id: &str,
    run_id: &str,
) -> Result<Option<String>> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => pool
            .get()?
            .query_row(
                "SELECT phase FROM jobs_execution_leases
                  WHERE run_id = ?1 AND account_id = ?2 AND application_id = ?3",
                params![run_id, account_id, application_id],
                |row| row.get(0),
            )
            .optional()
            .context("get execution lease phase"),
        DbPool::Postgres(_) => Ok(pool
            .get_pg()?
            .query_opt(
                "SELECT phase FROM jobs_execution_leases
                  WHERE run_id = $1 AND account_id = $2 AND application_id = $3",
                &[&run_id, &account_id, &application_id],
            )?
            .map(|row| row.get(0))),
    })
}

pub fn finish_execution_lease(
    pool: &DbPool,
    account_id: &str,
    application_id: &str,
    run_id: &str,
    lease_token: &str,
    fence: i64,
    outcome: &str,
) -> ExecutionLeaseResult<()> {
    validate_execution_access(account_id, application_id, run_id, lease_token, fence)?;
    if !matches!(
        outcome,
        "submitted" | "failed" | "side_effect_unknown" | "released"
    ) {
        return Err(ExecutionLeaseError::InvalidRequest);
    }
    let now = now_ms();
    crate::db::run_blocking_db(|| match pool {
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
            if lease.fence != fence
                || !execution_lease_token_matches(&lease.lease_token_sha256, lease_token)
            {
                return Err(ExecutionLeaseError::Conflict);
            }
            if lease.phase == outcome {
                tx.commit()?;
                return Ok(());
            }
            if !execution_finish_allowed(&lease.phase, outcome) {
                return Err(ExecutionLeaseError::Conflict);
            }
            if tx.execute(
                "UPDATE jobs_execution_leases SET phase = ?6, updated_at_ms = ?7,
                        finished_at_ms = ?7
                  WHERE run_id = ?1 AND account_id = ?2 AND application_id = ?3
                    AND lease_token_sha256 = ?4 AND fence = ?5 AND phase = ?8",
                params![
                    run_id,
                    account_id,
                    application_id,
                    lease.lease_token_sha256,
                    fence,
                    outcome,
                    now,
                    lease.phase,
                ],
            )? != 1
            {
                return Err(ExecutionLeaseError::Conflict);
            }
            tx.commit()?;
            Ok(())
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
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
            if lease.fence != fence
                || !execution_lease_token_matches(&lease.lease_token_sha256, lease_token)
            {
                return Err(ExecutionLeaseError::Conflict);
            }
            if lease.phase == outcome {
                tx.commit()?;
                return Ok(());
            }
            if !execution_finish_allowed(&lease.phase, outcome) {
                return Err(ExecutionLeaseError::Conflict);
            }
            if tx.execute(
                "UPDATE jobs_execution_leases SET phase = $6, updated_at_ms = $7,
                        finished_at_ms = $7
                  WHERE run_id = $1 AND account_id = $2 AND application_id = $3
                    AND lease_token_sha256 = $4 AND fence = $5 AND phase = $8",
                &[
                    &run_id,
                    &account_id,
                    &application_id,
                    &lease.lease_token_sha256,
                    &fence,
                    &outcome,
                    &now,
                    &lease.phase,
                ],
            )? != 1
            {
                return Err(ExecutionLeaseError::Conflict);
            }
            tx.commit()?;
            Ok(())
        }
    })
}

#[allow(clippy::too_many_arguments)]
pub fn reconcile_execution_lease_checkpoint(
    pool: &DbPool,
    account_id: &str,
    application_id: &str,
    run_id: &str,
    owner_id: &str,
    lease_token: Option<&str>,
    fence: i64,
    checkpoint_version: i64,
    checkpoint_phase: &str,
) -> ExecutionLeaseResult<ExecutionLeaseRecord> {
    let request = CheckpointReconciliationRequest {
        account_id,
        application_id,
        run_id,
        owner_id,
        lease_token,
        fence,
        checkpoint_version,
        checkpoint_phase,
    };
    validate_checkpoint_reconciliation_request(&request)?;
    let now = now_ms();
    crate::db::run_blocking_db(|| match pool {
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
            validate_checkpoint_lease_access(
                &lease,
                owner_id,
                lease_token,
                fence,
                checkpoint_version,
            )?;
            let (application_job_id, application_raw, application_state): (
                String,
                String,
                String,
            ) = tx
                .query_row(
                    "SELECT job_id, application_json, state FROM jobs_applications
                      WHERE account_id = ?1 AND id = ?2",
                    params![account_id, application_id],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .optional()?
                .ok_or(ExecutionLeaseError::NotFound)?;
            let evidence_count: i64 = tx.query_row(
                "SELECT COUNT(*) FROM jobs_application_evidence
                  WHERE account_id = ?1 AND application_id = ?2",
                params![account_id, application_id],
                |row| row.get(0),
            )?;
            if validate_trusted_submitted_checkpoint(
                &request,
                &lease,
                &application_job_id,
                application_raw.clone(),
                &application_state,
                evidence_count,
            )? {
                tx.commit()?;
                return Ok(ExecutionLeaseRecord {
                    run_id: run_id.to_string(),
                    fence,
                    lease_expires_at_ms: lease.lease_expires_at_ms,
                    phase: "submitted".to_string(),
                });
            }
            let (session_raw, session_runner, session_status): (String, String, String) = tx
                .query_row(
                    "SELECT session_json, runner, status FROM jobs_browser_sessions
                      WHERE account_id = ?1 AND id = ?2",
                    params![account_id, run_id],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .optional()?
                .ok_or(ExecutionLeaseError::NotFound)?;
            let attempt = tx
                .query_row(
                    "SELECT id, application_id, company_key, period_key, runner, status,
                            reserved_at_ms, updated_at_ms
                       FROM jobs_attempt_reservations
                      WHERE account_id = ?1 AND application_id = ?2",
                    params![account_id, application_id],
                    |row| {
                        Ok(AttemptReservation {
                            id: row.get(0)?,
                            application_id: row.get(1)?,
                            company_key: row.get(2)?,
                            period_key: row.get(3)?,
                            runner: row.get(4)?,
                            status: row.get(5)?,
                            reserved_at_ms: row.get(6)?,
                            updated_at_ms: row.get(7)?,
                        })
                    },
                )
                .optional()?;
            if let Some(outcome) = exact_checkpoint_recovery_replay(
                account_id,
                application_id,
                run_id,
                &lease,
                checkpoint_version,
                checkpoint_phase,
                &application_job_id,
                &application_raw,
                &application_state,
                &session_raw,
                &session_runner,
                &session_status,
                attempt.as_ref(),
            )? {
                tx.commit()?;
                return Ok(ExecutionLeaseRecord {
                    run_id: run_id.to_string(),
                    fence,
                    lease_expires_at_ms: lease.lease_expires_at_ms,
                    phase: outcome.lease_phase().to_string(),
                });
            }
            let plan = checkpoint_reconciliation_plan(
                account_id,
                application_id,
                run_id,
                &lease,
                checkpoint_version,
                checkpoint_phase,
                &application_job_id,
                application_raw,
                &application_state,
                session_raw,
                &session_runner,
                &session_status,
                attempt.as_ref(),
                now,
            )?;
            let application_payload = to_json(&plan.application, "Jobs application")?;
            let session_payload = to_json(&plan.browser_session, "browser session")?;
            if tx.execute(
                "UPDATE jobs_applications SET state = ?3, application_json = ?4,
                        updated_at_ms = ?5, submitted_at_ms = NULL
                  WHERE account_id = ?1 AND id = ?2 AND state = ?6",
                params![
                    account_id,
                    application_id,
                    plan.outcome.application_state(),
                    application_payload,
                    now,
                    application_state,
                ],
            )? != 1
            {
                return Err(ExecutionLeaseError::Conflict);
            }
            if tx.execute(
                "UPDATE jobs_browser_sessions SET status = ?3, session_json = ?4,
                        updated_at_ms = ?5
                  WHERE account_id = ?1 AND id = ?2 AND status = ?6",
                params![
                    account_id,
                    run_id,
                    plan.outcome.browser_status(),
                    session_payload,
                    now,
                    session_status,
                ],
            )? != 1
            {
                return Err(ExecutionLeaseError::Conflict);
            }
            if let Some(attempt) = &attempt {
                if tx.execute(
                    "UPDATE jobs_attempt_reservations SET status = ?4, updated_at_ms = ?5
                      WHERE id = ?1 AND account_id = ?2 AND application_id = ?3
                        AND status = ?6",
                    params![
                        attempt.id,
                        account_id,
                        application_id,
                        plan.outcome.attempt_status(),
                        now,
                        attempt.status,
                    ],
                )? != 1
                {
                    return Err(ExecutionLeaseError::Conflict);
                }
            }
            if tx.execute(
                "UPDATE jobs_execution_leases SET phase = ?6, updated_at_ms = ?7,
                        finished_at_ms = ?7
                  WHERE run_id = ?1 AND account_id = ?2 AND application_id = ?3
                    AND owner_id = ?4 AND fence = ?5 AND phase = ?8",
                params![
                    run_id,
                    account_id,
                    application_id,
                    owner_id,
                    fence,
                    plan.outcome.lease_phase(),
                    now,
                    lease.phase,
                ],
            )? != 1
            {
                return Err(ExecutionLeaseError::Conflict);
            }
            tx.commit()?;
            Ok(ExecutionLeaseRecord {
                run_id: run_id.to_string(),
                fence,
                lease_expires_at_ms: lease.lease_expires_at_ms,
                phase: plan.outcome.lease_phase().to_string(),
            })
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
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
            validate_checkpoint_lease_access(
                &lease,
                owner_id,
                lease_token,
                fence,
                checkpoint_version,
            )?;
            let application_row = tx
                .query_opt(
                    "SELECT job_id, application_json, state FROM jobs_applications
                      WHERE account_id = $1 AND id = $2 FOR UPDATE",
                    &[&account_id, &application_id],
                )?
                .ok_or(ExecutionLeaseError::NotFound)?;
            let application_job_id: String = application_row.get(0);
            let application_raw: String = application_row.get(1);
            let application_state: String = application_row.get(2);
            let evidence_count: i64 = tx
                .query_one(
                    "SELECT COUNT(*) FROM jobs_application_evidence
                      WHERE account_id = $1 AND application_id = $2",
                    &[&account_id, &application_id],
                )?
                .get(0);
            if validate_trusted_submitted_checkpoint(
                &request,
                &lease,
                &application_job_id,
                application_raw.clone(),
                &application_state,
                evidence_count,
            )? {
                tx.commit()?;
                return Ok(ExecutionLeaseRecord {
                    run_id: run_id.to_string(),
                    fence,
                    lease_expires_at_ms: lease.lease_expires_at_ms,
                    phase: "submitted".to_string(),
                });
            }
            let session_row = tx
                .query_opt(
                    "SELECT session_json, runner, status FROM jobs_browser_sessions
                      WHERE account_id = $1 AND id = $2 FOR UPDATE",
                    &[&account_id, &run_id],
                )?
                .ok_or(ExecutionLeaseError::NotFound)?;
            let session_raw: String = session_row.get(0);
            let session_runner: String = session_row.get(1);
            let session_status: String = session_row.get(2);
            let attempt = tx
                .query_opt(
                    "SELECT id, application_id, company_key, period_key, runner, status,
                            reserved_at_ms, updated_at_ms
                       FROM jobs_attempt_reservations
                      WHERE account_id = $1 AND application_id = $2 FOR UPDATE",
                    &[&account_id, &application_id],
                )?
                .map(|row| AttemptReservation {
                    id: row.get(0),
                    application_id: row.get(1),
                    company_key: row.get(2),
                    period_key: row.get(3),
                    runner: row.get(4),
                    status: row.get(5),
                    reserved_at_ms: row.get(6),
                    updated_at_ms: row.get(7),
                });
            if let Some(outcome) = exact_checkpoint_recovery_replay(
                account_id,
                application_id,
                run_id,
                &lease,
                checkpoint_version,
                checkpoint_phase,
                &application_job_id,
                &application_raw,
                &application_state,
                &session_raw,
                &session_runner,
                &session_status,
                attempt.as_ref(),
            )? {
                tx.commit()?;
                return Ok(ExecutionLeaseRecord {
                    run_id: run_id.to_string(),
                    fence,
                    lease_expires_at_ms: lease.lease_expires_at_ms,
                    phase: outcome.lease_phase().to_string(),
                });
            }
            let plan = checkpoint_reconciliation_plan(
                account_id,
                application_id,
                run_id,
                &lease,
                checkpoint_version,
                checkpoint_phase,
                &application_job_id,
                application_raw,
                &application_state,
                session_raw,
                &session_runner,
                &session_status,
                attempt.as_ref(),
                now,
            )?;
            let application_payload = to_json(&plan.application, "Jobs application")?;
            let session_payload = to_json(&plan.browser_session, "browser session")?;
            if tx.execute(
                "UPDATE jobs_applications SET state = $3, application_json = $4,
                        updated_at_ms = $5, submitted_at_ms = NULL
                  WHERE account_id = $1 AND id = $2 AND state = $6",
                &[
                    &account_id,
                    &application_id,
                    &plan.outcome.application_state(),
                    &application_payload,
                    &now,
                    &application_state,
                ],
            )? != 1
            {
                return Err(ExecutionLeaseError::Conflict);
            }
            if tx.execute(
                "UPDATE jobs_browser_sessions SET status = $3, session_json = $4,
                        updated_at_ms = $5
                  WHERE account_id = $1 AND id = $2 AND status = $6",
                &[
                    &account_id,
                    &run_id,
                    &plan.outcome.browser_status(),
                    &session_payload,
                    &now,
                    &session_status,
                ],
            )? != 1
            {
                return Err(ExecutionLeaseError::Conflict);
            }
            if let Some(attempt) = &attempt {
                if tx.execute(
                    "UPDATE jobs_attempt_reservations SET status = $4, updated_at_ms = $5
                      WHERE id = $1 AND account_id = $2 AND application_id = $3
                        AND status = $6",
                    &[
                        &attempt.id,
                        &account_id,
                        &application_id,
                        &plan.outcome.attempt_status(),
                        &now,
                        &attempt.status,
                    ],
                )? != 1
                {
                    return Err(ExecutionLeaseError::Conflict);
                }
            }
            if tx.execute(
                "UPDATE jobs_execution_leases SET phase = $6, updated_at_ms = $7,
                        finished_at_ms = $7
                  WHERE run_id = $1 AND account_id = $2 AND application_id = $3
                    AND owner_id = $4 AND fence = $5 AND phase = $8",
                &[
                    &run_id,
                    &account_id,
                    &application_id,
                    &owner_id,
                    &fence,
                    &plan.outcome.lease_phase(),
                    &now,
                    &lease.phase,
                ],
            )? != 1
            {
                return Err(ExecutionLeaseError::Conflict);
            }
            tx.commit()?;
            Ok(ExecutionLeaseRecord {
                run_id: run_id.to_string(),
                fence,
                lease_expires_at_ms: lease.lease_expires_at_ms,
                phase: plan.outcome.lease_phase().to_string(),
            })
        }
    })
}
