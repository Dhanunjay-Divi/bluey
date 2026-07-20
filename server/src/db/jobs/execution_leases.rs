
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
