
#[allow(clippy::too_many_arguments)]
pub fn save_local_run_ticket(
    pool: &DbPool,
    account_id: &str,
    application_id: &str,
    run_id: &str,
    ticket_hash: &str,
    ticket_secret: &str,
    payload: Value,
    expires_at_ms: i64,
) -> Result<LocalRunTicket> {
    let now = now_ms();
    let value = LocalRunTicket {
        id: run_id.to_string(),
        account_id: account_id.to_string(),
        application_id: application_id.to_string(),
        ticket_hash: ticket_hash.to_string(),
        ticket_secret: ticket_secret.to_string(),
        payload,
        status: "queued".to_string(),
        expires_at_ms,
        created_at_ms: now,
        updated_at_ms: now,
    };
    let encrypted_ticket = encrypt_payload(&value.ticket_secret)?;
    let encrypted_payload = to_json(&value.payload, "Jobs local browser packet")?;
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            pool.get()?.execute(
                "INSERT INTO jobs_local_run_tickets (
                    id, account_id, application_id, ticket_hash, ticket_secret,
                    payload_json, status, expires_at_ms, created_at_ms, updated_at_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9)
                 ON CONFLICT(id) DO UPDATE SET
                    ticket_hash = excluded.ticket_hash,
                    ticket_secret = excluded.ticket_secret,
                    payload_json = excluded.payload_json,
                    status = excluded.status,
                    expires_at_ms = excluded.expires_at_ms,
                    updated_at_ms = excluded.updated_at_ms
                 WHERE jobs_local_run_tickets.account_id = excluded.account_id
                   AND jobs_local_run_tickets.application_id = excluded.application_id",
                params![
                    value.id,
                    value.account_id,
                    value.application_id,
                    value.ticket_hash,
                    encrypted_ticket,
                    encrypted_payload,
                    value.status,
                    value.expires_at_ms,
                    value.created_at_ms,
                ],
            )?;
            Ok(value)
        }
        DbPool::Postgres(_) => {
            pool.get_pg()?.execute(
                "INSERT INTO jobs_local_run_tickets (
                    id, account_id, application_id, ticket_hash, ticket_secret,
                    payload_json, status, expires_at_ms, created_at_ms, updated_at_ms
                 ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $9)
                 ON CONFLICT(id) DO UPDATE SET
                    ticket_hash = EXCLUDED.ticket_hash,
                    ticket_secret = EXCLUDED.ticket_secret,
                    payload_json = EXCLUDED.payload_json,
                    status = EXCLUDED.status,
                    expires_at_ms = EXCLUDED.expires_at_ms,
                    updated_at_ms = EXCLUDED.updated_at_ms
                 WHERE jobs_local_run_tickets.account_id = EXCLUDED.account_id
                   AND jobs_local_run_tickets.application_id = EXCLUDED.application_id",
                &[
                    &value.id,
                    &value.account_id,
                    &value.application_id,
                    &value.ticket_hash,
                    &encrypted_ticket,
                    &encrypted_payload,
                    &value.status,
                    &value.expires_at_ms,
                    &value.created_at_ms,
                ],
            )?;
            Ok(value)
        }
    })
}

pub fn get_local_run_ticket(
    pool: &DbPool,
    account_id: &str,
    run_id: &str,
) -> Result<Option<LocalRunTicket>> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => pool
            .get()?
            .query_row(
                "SELECT id, account_id, application_id, ticket_hash, ticket_secret,
                        payload_json, status, expires_at_ms, created_at_ms, updated_at_ms
                   FROM jobs_local_run_tickets
                  WHERE account_id = ?1 AND id = ?2",
                params![account_id, run_id],
                local_run_ticket_from_sqlite_row,
            )
            .optional()
            .context("get Jobs local run ticket"),
        DbPool::Postgres(_) => pool
            .get_pg()?
            .query_opt(
                "SELECT id, account_id, application_id, ticket_hash, ticket_secret,
                        payload_json, status, expires_at_ms, created_at_ms, updated_at_ms
                   FROM jobs_local_run_tickets
                  WHERE account_id = $1 AND id = $2",
                &[&account_id, &run_id],
            )?
            .map(local_run_ticket_from_pg_row)
            .transpose(),
    })
}

pub fn get_local_run_ticket_by_hash(
    pool: &DbPool,
    run_id: &str,
    ticket_hash: &str,
) -> Result<Option<LocalRunTicket>> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => pool
            .get()?
            .query_row(
                "SELECT id, account_id, application_id, ticket_hash, ticket_secret,
                        payload_json, status, expires_at_ms, created_at_ms, updated_at_ms
                   FROM jobs_local_run_tickets
                  WHERE id = ?1 AND ticket_hash = ?2",
                params![run_id, ticket_hash],
                local_run_ticket_from_sqlite_row,
            )
            .optional()
            .context("get Jobs local run capability"),
        DbPool::Postgres(_) => pool
            .get_pg()?
            .query_opt(
                "SELECT id, account_id, application_id, ticket_hash, ticket_secret,
                        payload_json, status, expires_at_ms, created_at_ms, updated_at_ms
                   FROM jobs_local_run_tickets
                  WHERE id = $1 AND ticket_hash = $2",
                &[&run_id, &ticket_hash],
            )?
            .map(local_run_ticket_from_pg_row)
            .transpose(),
    })
}

#[cfg(test)]
fn claim_local_run_ticket(
    pool: &DbPool,
    run_id: &str,
    ticket_hash: &str,
) -> Result<Option<LocalRunTicket>> {
    let now = now_ms();
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let transaction = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let value = transaction
                .query_row(
                    "SELECT id, account_id, application_id, ticket_hash, ticket_secret,
                            payload_json, status, expires_at_ms, created_at_ms, updated_at_ms
                       FROM jobs_local_run_tickets
                      WHERE id = ?1 AND ticket_hash = ?2 AND expires_at_ms > ?3
                        AND status = 'queued'",
                    params![run_id, ticket_hash, now],
                    local_run_ticket_from_sqlite_row,
                )
                .optional()?;
            if value.is_some()
                && transaction.execute(
                    "UPDATE jobs_local_run_tickets SET status = 'claimed', updated_at_ms = ?3
                      WHERE id = ?1 AND ticket_hash = ?2 AND status = 'queued'",
                    params![run_id, ticket_hash, now],
                )? != 1
            {
                transaction.commit()?;
                return Ok(None);
            }
            transaction.commit()?;
            Ok(value.map(|mut value| {
                value.status = "claimed".to_string();
                value.updated_at_ms = now;
                value
            }))
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut transaction = conn.transaction()?;
            let value = transaction
                .query_opt(
                    "SELECT id, account_id, application_id, ticket_hash, ticket_secret,
                            payload_json, status, expires_at_ms, created_at_ms, updated_at_ms
                       FROM jobs_local_run_tickets
                      WHERE id = $1 AND ticket_hash = $2 AND expires_at_ms > $3
                        AND status = 'queued'
                      FOR UPDATE",
                    &[&run_id, &ticket_hash, &now],
                )?
                .map(local_run_ticket_from_pg_row)
                .transpose()?;
            if value.is_some()
                && transaction.execute(
                    "UPDATE jobs_local_run_tickets SET status = 'claimed', updated_at_ms = $3
                      WHERE id = $1 AND ticket_hash = $2 AND status = 'queued'",
                    &[&run_id, &ticket_hash, &now],
                )? != 1
            {
                transaction.commit()?;
                return Ok(None);
            }
            transaction.commit()?;
            Ok(value.map(|mut value| {
                value.status = "claimed".to_string();
                value.updated_at_ms = now;
                value
            }))
        }
    })
}

/// Claims a local-browser ticket only while the account, application,
/// verified application identity, browser profile, and local-run entitlement
/// still match the exact queued packet. This is the server-side admission
/// authority; the launch URL alone is not enough.
pub fn claim_authorized_local_run_ticket(
    pool: &DbPool,
    run_id: &str,
    ticket_hash: &str,
) -> Result<Option<LocalRunTicket>> {
    let now = now_ms();
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let value = sqlite_local_run_authority(
                &tx,
                run_id,
                ticket_hash,
                now,
                LocalRunAuthorityPhase::Claim,
            )?;
            let Some(mut value) = value else {
                tx.commit()?;
                return Ok(None);
            };
            if tx.execute(
                "UPDATE jobs_local_run_tickets SET status = 'claimed', updated_at_ms = ?3
                  WHERE id = ?1 AND ticket_hash = ?2 AND status = 'queued'
                    AND expires_at_ms > ?3",
                params![run_id, ticket_hash, now],
            )? != 1
            {
                tx.commit()?;
                return Ok(None);
            }
            tx.commit()?;
            value.status = "claimed".to_string();
            value.updated_at_ms = now;
            Ok(Some(value))
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            let value = postgres_local_run_authority(
                &mut tx,
                run_id,
                ticket_hash,
                now,
                LocalRunAuthorityPhase::Claim,
            )?;
            let Some(mut value) = value else {
                tx.commit()?;
                return Ok(None);
            };
            if tx.execute(
                "UPDATE jobs_local_run_tickets SET status = 'claimed', updated_at_ms = $3
                  WHERE id = $1 AND ticket_hash = $2 AND status = 'queued'
                    AND expires_at_ms > $3",
                &[&run_id, &ticket_hash, &now],
            )? != 1
            {
                tx.commit()?;
                return Ok(None);
            }
            tx.commit()?;
            value.status = "claimed".to_string();
            value.updated_at_ms = now;
            Ok(Some(value))
        }
    })
}

/// Performs a non-mutating, current-authority check immediately before the
/// local browser crosses the irreversible employer Submit boundary.
pub fn local_run_submit_authorized(pool: &DbPool, run_id: &str, ticket_hash: &str) -> Result<bool> {
    let now = now_ms();
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Deferred)?;
            let authorized = sqlite_local_run_authority(
                &tx,
                run_id,
                ticket_hash,
                now,
                LocalRunAuthorityPhase::Submit,
            )?
            .is_some();
            tx.commit()?;
            Ok(authorized)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            let authorized = postgres_local_run_authority(
                &mut tx,
                run_id,
                ticket_hash,
                now,
                LocalRunAuthorityPhase::Submit,
            )?
            .is_some();
            tx.commit()?;
            Ok(authorized)
        }
    })
}

fn sqlite_local_run_authority(
    tx: &rusqlite::Transaction<'_>,
    run_id: &str,
    ticket_hash: &str,
    now: i64,
    phase: LocalRunAuthorityPhase,
) -> Result<Option<LocalRunTicket>> {
    let expected_ticket_status = match phase {
        LocalRunAuthorityPhase::Claim => "queued",
        LocalRunAuthorityPhase::Submit => "claimed",
    };
    let ticket = tx
        .query_row(
            "SELECT id, account_id, application_id, ticket_hash, ticket_secret,
                    payload_json, status, expires_at_ms, created_at_ms, updated_at_ms
               FROM jobs_local_run_tickets
              WHERE id = ?1 AND ticket_hash = ?2 AND status = ?3
                AND expires_at_ms > ?4",
            params![run_id, ticket_hash, expected_ticket_status, now],
            local_run_ticket_from_sqlite_row,
        )
        .optional()?;
    let Some(ticket) = ticket else {
        return Ok(None);
    };
    let application_row: Option<(String, String, String)> = tx
        .query_row(
            "SELECT job_id, application_json, state FROM jobs_applications
              WHERE account_id = ?1 AND id = ?2",
            params![ticket.account_id, ticket.application_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    let Some((job_id, application_raw, application_state)) = application_row else {
        return Ok(None);
    };
    let application = parse_application_json(
        application_raw,
        &ticket.application_id,
        &job_id,
        "job application",
    )?;
    let identity_id = application
        .receipt
        .pointer("/application_identity/id")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let session_row: Option<(String, String, String)> = tx
        .query_row(
            "SELECT session_json, runner, status FROM jobs_browser_sessions
              WHERE account_id = ?1 AND id = ?2",
            params![ticket.account_id, run_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    let identity_row: Option<(String, String, i64)> = tx
        .query_row(
            "SELECT identity_json, verification_status, is_default
               FROM jobs_application_identities
              WHERE account_id = ?1 AND id = ?2",
            params![ticket.account_id, identity_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    let local_browser: Option<i64> = tx
        .query_row(
            "SELECT local_browser FROM jobs_entitlements WHERE account_id = ?1",
            params![ticket.account_id],
            |row| row.get(0),
        )
        .optional()?;
    let canonical_url: Option<String> = tx
        .query_row(
            "SELECT canonical_url FROM jobs_postings WHERE account_id = ?1 AND id = ?2",
            params![ticket.account_id, job_id],
            |row| row.get(0),
        )
        .optional()?;
    let (
        Some((session_raw, session_runner, session_status)),
        Some((identity_raw, identity_status, is_default)),
        Some(canonical_url),
    ) = (session_row, identity_row, canonical_url)
    else {
        return Ok(None);
    };
    let session: BrowserSession = parse_json(session_raw, "browser session")?;
    let identity =
        parse_application_identity_row(identity_raw, identity_status.clone(), is_default != 0)?;
    Ok(local_run_authority_matches(
        &ticket,
        &application,
        &application_state,
        &session,
        &session_runner,
        &session_status,
        &identity,
        &identity_status,
        local_browser == Some(1),
        &canonical_url,
        phase,
        now,
    )
    .then_some(ticket))
}

fn postgres_local_run_authority(
    tx: &mut postgres::Transaction<'_>,
    run_id: &str,
    ticket_hash: &str,
    now: i64,
    phase: LocalRunAuthorityPhase,
) -> Result<Option<LocalRunTicket>> {
    let expected_ticket_status = match phase {
        LocalRunAuthorityPhase::Claim => "queued",
        LocalRunAuthorityPhase::Submit => "claimed",
    };
    let ticket = tx
        .query_opt(
            "SELECT id, account_id, application_id, ticket_hash, ticket_secret,
                    payload_json, status, expires_at_ms, created_at_ms, updated_at_ms
               FROM jobs_local_run_tickets
              WHERE id = $1 AND ticket_hash = $2 AND status = $3
                AND expires_at_ms > $4
              FOR UPDATE",
            &[&run_id, &ticket_hash, &expected_ticket_status, &now],
        )?
        .map(local_run_ticket_from_pg_row)
        .transpose()?;
    let Some(ticket) = ticket else {
        return Ok(None);
    };
    let Some(application_row) = tx.query_opt(
        "SELECT job_id, application_json, state FROM jobs_applications
          WHERE account_id = $1 AND id = $2",
        &[&ticket.account_id, &ticket.application_id],
    )?
    else {
        return Ok(None);
    };
    let job_id: String = application_row.get(0);
    let application_raw: String = application_row.get(1);
    let application_state: String = application_row.get(2);
    let application = parse_application_json(
        application_raw,
        &ticket.application_id,
        &job_id,
        "job application",
    )?;
    let identity_id = application
        .receipt
        .pointer("/application_identity/id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let session_row = tx.query_opt(
        "SELECT session_json, runner, status FROM jobs_browser_sessions
          WHERE account_id = $1 AND id = $2",
        &[&ticket.account_id, &run_id],
    )?;
    let identity_row = tx.query_opt(
        "SELECT identity_json, verification_status, is_default
           FROM jobs_application_identities
          WHERE account_id = $1 AND id = $2",
        &[&ticket.account_id, &identity_id],
    )?;
    let local_browser = tx
        .query_opt(
            "SELECT local_browser FROM jobs_entitlements WHERE account_id = $1",
            &[&ticket.account_id],
        )?
        .map(|row| row.get::<_, i32>(0) != 0)
        .unwrap_or(false);
    let canonical_url = tx
        .query_opt(
            "SELECT canonical_url FROM jobs_postings WHERE account_id = $1 AND id = $2",
            &[&ticket.account_id, &job_id],
        )?
        .map(|row| row.get::<_, String>(0));
    let (Some(session_row), Some(identity_row), Some(canonical_url)) =
        (session_row, identity_row, canonical_url)
    else {
        return Ok(None);
    };
    let session_raw: String = session_row.get(0);
    let session_runner: String = session_row.get(1);
    let session_status: String = session_row.get(2);
    let identity_raw: String = identity_row.get(0);
    let identity_status: String = identity_row.get(1);
    let is_default = identity_row.get::<_, i32>(2) != 0;
    let session: BrowserSession = parse_json(session_raw, "browser session")?;
    let identity =
        parse_application_identity_row(identity_raw, identity_status.clone(), is_default)?;
    Ok(local_run_authority_matches(
        &ticket,
        &application,
        &application_state,
        &session,
        &session_runner,
        &session_status,
        &identity,
        &identity_status,
        local_browser,
        &canonical_url,
        phase,
        now,
    )
    .then_some(ticket))
}

#[allow(clippy::too_many_arguments)]
fn local_run_authority_matches(
    ticket: &LocalRunTicket,
    application: &JobApplication,
    application_state: &str,
    session: &BrowserSession,
    session_runner: &str,
    session_status: &str,
    identity: &ApplicationIdentity,
    identity_status: &str,
    local_browser: bool,
    canonical_url: &str,
    phase: LocalRunAuthorityPhase,
    now: i64,
) -> bool {
    let (expected_ticket_status, expected_application_state, expected_session_status) = match phase
    {
        LocalRunAuthorityPhase::Claim => ("queued", "queued", "queued"),
        LocalRunAuthorityPhase::Submit => ("claimed", "running", "running"),
    };
    let frozen_identity_id = application
        .receipt
        .pointer("/application_identity/id")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let frozen_identity_email = application
        .receipt
        .pointer("/application_identity/email")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let expected_browser_profile_id =
        execution_browser_profile_id(&ticket.account_id, frozen_identity_id);
    ticket.status == expected_ticket_status
        && ticket.expires_at_ms > now
        && local_browser
        && application.state == application_state
        && application_state == expected_application_state
        && application.run_id.as_deref() == Some(ticket.id.as_str())
        && session.id == ticket.id
        && session.application_id.as_deref() == Some(ticket.application_id.as_str())
        && session.runner == session_runner
        && session_runner == "local"
        && session.status == session_status
        && session_status == expected_session_status
        && identity.verification_status == identity_status
        && identity_status == "verified"
        && !frozen_identity_id.is_empty()
        && identity.id == frozen_identity_id
        && identity.email == frozen_identity_email
        && application
            .receipt
            .pointer("/application_identity/verified")
            .and_then(Value::as_bool)
            == Some(true)
        && ticket.payload.get("accountId").and_then(Value::as_str)
            == Some(ticket.account_id.as_str())
        && ticket.payload.get("applicationId").and_then(Value::as_str)
            == Some(ticket.application_id.as_str())
        && ticket.payload.get("runId").and_then(Value::as_str) == Some(ticket.id.as_str())
        && ticket
            .payload
            .get("applicationIdentityId")
            .and_then(Value::as_str)
            == Some(frozen_identity_id)
        && ticket
            .payload
            .get("browserProfileId")
            .and_then(Value::as_str)
            == Some(expected_browser_profile_id.as_str())
        && ticket.payload.get("runner").and_then(Value::as_str) == Some("local")
        && ticket.payload.get("url").and_then(Value::as_str) == Some(canonical_url)
}

pub fn update_local_run_ticket_status(
    pool: &DbPool,
    run_id: &str,
    ticket_hash: &str,
    status: &str,
) -> Result<bool> {
    let now = now_ms();
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => Ok(pool.get()?.execute(
            "UPDATE jobs_local_run_tickets SET status = ?3, updated_at_ms = ?4
              WHERE id = ?1 AND ticket_hash = ?2 AND expires_at_ms > ?4",
            params![run_id, ticket_hash, status, now],
        )? > 0),
        DbPool::Postgres(_) => Ok(pool.get_pg()?.execute(
            "UPDATE jobs_local_run_tickets SET status = $3, updated_at_ms = $4
              WHERE id = $1 AND ticket_hash = $2 AND expires_at_ms > $4",
            &[&run_id, &ticket_hash, &status, &now],
        )? > 0),
    })
}

pub fn finalize_local_side_effect_unknown(
    pool: &DbPool,
    account_id: &str,
    application_id: &str,
    run_id: &str,
    ticket_hash: &str,
    reconciliation_receipt: Value,
    session: &BrowserSession,
) -> Result<JobApplication> {
    if reconciliation_receipt.get("status").and_then(Value::as_str) != Some("side_effect_unknown")
        || session.id != run_id
        || session.runner != "local"
        || session.application_id.as_deref() != Some(application_id)
    {
        anyhow::bail!("invalid local reconciliation result")
    }
    let now = now_ms();
    let mut terminal_session = session.clone();
    terminal_session.status = "needs_input".to_string();
    terminal_session.current_step = "Submission outcome needs reconciliation".to_string();
    terminal_session.updated_at_ms = now;
    let session_payload = to_json(&terminal_session, "browser session")?;

    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let raw: Option<(String, String)> = tx
                .query_row(
                    "SELECT job_id, application_json FROM jobs_applications
                      WHERE account_id = ?1 AND id = ?2",
                    params![account_id, application_id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()?;
            let Some((job_id, raw)) = raw else {
                anyhow::bail!("application not found")
            };
            let mut application =
                parse_application_json(raw, application_id, &job_id, "Jobs application")?;
            if application.run_id.as_deref() != Some(run_id) {
                anyhow::bail!("local run ticket does not match this application")
            }
            if application.state == "side_effect_unknown" {
                let status: Option<String> = tx
                    .query_row(
                        "SELECT status FROM jobs_local_run_tickets
                          WHERE id = ?1 AND account_id = ?2 AND application_id = ?3
                            AND ticket_hash = ?4",
                        params![run_id, account_id, application_id, ticket_hash],
                        |row| row.get(0),
                    )
                    .optional()?;
                if status.as_deref() == Some("side_effect_unknown") {
                    tx.commit()?;
                    return Ok(application);
                }
                anyhow::bail!("local run ticket is not active")
            }
            validate_application_transition(&application.state, "side_effect_unknown")?;
            if tx.execute(
                "UPDATE jobs_local_run_tickets
                    SET status = 'side_effect_unknown', updated_at_ms = ?5
                  WHERE id = ?1 AND account_id = ?2 AND application_id = ?3
                    AND ticket_hash = ?4 AND expires_at_ms > ?5
                    AND status IN ('claimed', 'needs_input')",
                params![run_id, account_id, application_id, ticket_hash, now],
            )? != 1
            {
                anyhow::bail!("local run ticket is not active")
            }
            if tx.execute(
                "UPDATE jobs_attempt_reservations
                    SET status = 'side_effect_unknown', updated_at_ms = ?3
                  WHERE account_id = ?1 AND application_id = ?2",
                params![account_id, application_id, now],
            )? != 1
            {
                anyhow::bail!("application attempt reservation is missing")
            }
            if tx.execute(
                "UPDATE jobs_browser_sessions
                    SET status = 'needs_input', session_json = ?3, updated_at_ms = ?4
                  WHERE account_id = ?1 AND id = ?2",
                params![account_id, run_id, session_payload, now],
            )? != 1
            {
                anyhow::bail!("browser session not found")
            }
            if !application.receipt.is_object() {
                application.receipt = json!({});
            }
            application
                .receipt
                .as_object_mut()
                .expect("receipt normalized above")
                .insert(
                    "local_reconciliation".to_string(),
                    json!({
                        "status": "side_effect_unknown",
                        "recorded_at_ms": now,
                        "receipt": reconciliation_receipt,
                    }),
                );
            application.state = "side_effect_unknown".to_string();
            application.updated_at_ms = now;
            let application_payload = to_json(&application, "Jobs application")?;
            if tx.execute(
                "UPDATE jobs_applications SET state = 'side_effect_unknown',
                        application_json = ?3, updated_at_ms = ?4
                  WHERE account_id = ?1 AND id = ?2",
                params![account_id, application_id, application_payload, now],
            )? != 1
            {
                anyhow::bail!("application not found")
            }
            tx.commit()?;
            Ok(application)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            let row = tx.query_opt(
                "SELECT job_id, application_json FROM jobs_applications
                  WHERE account_id = $1 AND id = $2 FOR UPDATE",
                &[&account_id, &application_id],
            )?;
            let Some(row) = row else {
                anyhow::bail!("application not found")
            };
            let job_id: String = row.get(0);
            let mut application =
                parse_application_json(row.get(1), application_id, &job_id, "Jobs application")?;
            if application.run_id.as_deref() != Some(run_id) {
                anyhow::bail!("local run ticket does not match this application")
            }
            if application.state == "side_effect_unknown" {
                let status = tx
                    .query_opt(
                        "SELECT status FROM jobs_local_run_tickets
                          WHERE id = $1 AND account_id = $2 AND application_id = $3
                            AND ticket_hash = $4 FOR UPDATE",
                        &[&run_id, &account_id, &application_id, &ticket_hash],
                    )?
                    .map(|row| row.get::<_, String>(0));
                if status.as_deref() == Some("side_effect_unknown") {
                    tx.commit()?;
                    return Ok(application);
                }
                anyhow::bail!("local run ticket is not active")
            }
            validate_application_transition(&application.state, "side_effect_unknown")?;
            if tx.execute(
                "UPDATE jobs_local_run_tickets
                    SET status = 'side_effect_unknown', updated_at_ms = $5
                  WHERE id = $1 AND account_id = $2 AND application_id = $3
                    AND ticket_hash = $4 AND expires_at_ms > $5
                    AND status IN ('claimed', 'needs_input')",
                &[&run_id, &account_id, &application_id, &ticket_hash, &now],
            )? != 1
            {
                anyhow::bail!("local run ticket is not active")
            }
            if tx.execute(
                "UPDATE jobs_attempt_reservations
                    SET status = 'side_effect_unknown', updated_at_ms = $3
                  WHERE account_id = $1 AND application_id = $2",
                &[&account_id, &application_id, &now],
            )? != 1
            {
                anyhow::bail!("application attempt reservation is missing")
            }
            if tx.execute(
                "UPDATE jobs_browser_sessions
                    SET status = 'needs_input', session_json = $3, updated_at_ms = $4
                  WHERE account_id = $1 AND id = $2",
                &[&account_id, &run_id, &session_payload, &now],
            )? != 1
            {
                anyhow::bail!("browser session not found")
            }
            if !application.receipt.is_object() {
                application.receipt = json!({});
            }
            application
                .receipt
                .as_object_mut()
                .expect("receipt normalized above")
                .insert(
                    "local_reconciliation".to_string(),
                    json!({
                        "status": "side_effect_unknown",
                        "recorded_at_ms": now,
                        "receipt": reconciliation_receipt,
                    }),
                );
            application.state = "side_effect_unknown".to_string();
            application.updated_at_ms = now;
            let application_payload = to_json(&application, "Jobs application")?;
            if tx.execute(
                "UPDATE jobs_applications SET state = 'side_effect_unknown',
                        application_json = $3, updated_at_ms = $4
                  WHERE account_id = $1 AND id = $2",
                &[&account_id, &application_id, &application_payload, &now],
            )? != 1
            {
                anyhow::bail!("application not found")
            }
            tx.commit()?;
            Ok(application)
        }
    })
}

pub fn approve_local_run_resume_action(
    pool: &DbPool,
    account_id: &str,
    application_id: &str,
    run_id: &str,
    intervention_id: &str,
) -> Result<Option<LocalRunResumeAction>> {
    if [account_id, application_id, run_id, intervention_id]
        .iter()
        .any(|value| value.trim().is_empty() || value.len() > 240)
    {
        anyhow::bail!("invalid local resume approval")
    }
    let now = now_ms();
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let ticket_expires_at: Option<i64> = tx
                .query_row(
                    "SELECT expires_at_ms FROM jobs_local_run_tickets
                      WHERE id = ?1 AND account_id = ?2 AND application_id = ?3
                        AND status = 'needs_input' AND expires_at_ms > ?4",
                    params![run_id, account_id, application_id, now],
                    |row| row.get(0),
                )
                .optional()?;
            let Some(ticket_expires_at) = ticket_expires_at else {
                tx.commit()?;
                return Ok(None);
            };
            let intervention_valid: bool = tx.query_row(
                "SELECT EXISTS(
                    SELECT 1 FROM jobs_interventions
                     WHERE id = ?1 AND account_id = ?2 AND application_id = ?3
                       AND status = 'approved'
                )",
                params![intervention_id, account_id, application_id],
                |row| row.get(0),
            )?;
            if !intervention_valid {
                tx.commit()?;
                return Ok(None);
            }
            let existing: Option<(String, String, i64)> = tx
                .query_row(
                    "SELECT run_id, status, expires_at_ms
                       FROM jobs_local_run_resume_actions WHERE intervention_id = ?1",
                    params![intervention_id],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .optional()?;
            if let Some((existing_run_id, status, expires_at_ms)) = existing {
                tx.commit()?;
                return Ok(
                    (status == "approved" && existing_run_id == run_id).then(|| {
                        LocalRunResumeAction {
                            run_id: run_id.to_string(),
                            intervention_id: intervention_id.to_string(),
                            action: "approve_submission".to_string(),
                            expires_at_ms,
                            account_id: account_id.to_string(),
                            application_id: application_id.to_string(),
                            first_consumption: false,
                        }
                    }),
                );
            }
            let expires_at_ms = ticket_expires_at.min(now + LOCAL_RESUME_ACTION_TTL_MS);
            let id = uuid::Uuid::new_v4().to_string();
            if let Err(error) = tx.execute(
                "INSERT INTO jobs_local_run_resume_actions (
                    id, run_id, account_id, application_id, intervention_id,
                    action, status, expires_at_ms, created_at_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, 'approve_submission',
                           'approved', ?6, ?7)",
                params![
                    id,
                    run_id,
                    account_id,
                    application_id,
                    intervention_id,
                    expires_at_ms,
                    now,
                ],
            ) {
                if error.sqlite_error_code() == Some(rusqlite::ErrorCode::ConstraintViolation) {
                    return Ok(None);
                }
                return Err(error.into());
            }
            tx.commit()?;
            Ok(Some(LocalRunResumeAction {
                run_id: run_id.to_string(),
                intervention_id: intervention_id.to_string(),
                action: "approve_submission".to_string(),
                expires_at_ms,
                account_id: account_id.to_string(),
                application_id: application_id.to_string(),
                first_consumption: false,
            }))
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            let ticket_expires_at = tx
                .query_opt(
                    "SELECT expires_at_ms FROM jobs_local_run_tickets
                      WHERE id = $1 AND account_id = $2 AND application_id = $3
                        AND status = 'needs_input' AND expires_at_ms > $4
                      FOR UPDATE",
                    &[&run_id, &account_id, &application_id, &now],
                )?
                .map(|row| row.get::<_, i64>(0));
            let Some(ticket_expires_at) = ticket_expires_at else {
                tx.commit()?;
                return Ok(None);
            };
            let intervention_valid: bool = tx
                .query_one(
                    "SELECT EXISTS(
                        SELECT 1 FROM jobs_interventions
                         WHERE id = $1 AND account_id = $2 AND application_id = $3
                           AND status = 'approved'
                    )",
                    &[&intervention_id, &account_id, &application_id],
                )?
                .get(0);
            if !intervention_valid {
                tx.commit()?;
                return Ok(None);
            }
            if let Some(row) = tx.query_opt(
                "SELECT run_id, status, expires_at_ms
                   FROM jobs_local_run_resume_actions WHERE intervention_id = $1 FOR UPDATE",
                &[&intervention_id],
            )? {
                let existing_run_id: String = row.get(0);
                let status: String = row.get(1);
                let expires_at_ms: i64 = row.get(2);
                tx.commit()?;
                return Ok(
                    (status == "approved" && existing_run_id == run_id).then(|| {
                        LocalRunResumeAction {
                            run_id: run_id.to_string(),
                            intervention_id: intervention_id.to_string(),
                            action: "approve_submission".to_string(),
                            expires_at_ms,
                            account_id: account_id.to_string(),
                            application_id: application_id.to_string(),
                            first_consumption: false,
                        }
                    }),
                );
            }
            let expires_at_ms = ticket_expires_at.min(now + LOCAL_RESUME_ACTION_TTL_MS);
            let id = uuid::Uuid::new_v4().to_string();
            if let Err(error) = tx.execute(
                "INSERT INTO jobs_local_run_resume_actions (
                    id, run_id, account_id, application_id, intervention_id,
                    action, status, expires_at_ms, created_at_ms
                 ) VALUES ($1, $2, $3, $4, $5, 'approve_submission',
                           'approved', $6, $7)",
                &[
                    &id,
                    &run_id,
                    &account_id,
                    &application_id,
                    &intervention_id,
                    &expires_at_ms,
                    &now,
                ],
            ) {
                if error.code() == Some(&postgres::error::SqlState::UNIQUE_VIOLATION) {
                    return Ok(None);
                }
                return Err(error.into());
            }
            tx.commit()?;
            Ok(Some(LocalRunResumeAction {
                run_id: run_id.to_string(),
                intervention_id: intervention_id.to_string(),
                action: "approve_submission".to_string(),
                expires_at_ms,
                account_id: account_id.to_string(),
                application_id: application_id.to_string(),
                first_consumption: false,
            }))
        }
    })
}

pub fn consume_local_run_resume_action(
    pool: &DbPool,
    run_id: &str,
    ticket_hash: &str,
) -> Result<Option<LocalRunResumeAction>> {
    let now = now_ms();
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let value: Option<(String, String, String, String, String, i64, String)> = tx
                .query_row(
                    "SELECT a.id, a.account_id, a.application_id, a.intervention_id,
                            a.action, a.expires_at_ms, a.status
                      FROM jobs_local_run_resume_actions a
                       JOIN jobs_local_run_tickets t ON t.id = a.run_id
                       JOIN jobs_interventions i ON i.id = a.intervention_id
                      WHERE a.run_id = ?1 AND t.ticket_hash = ?2
                        AND t.status IN ('needs_input', 'claimed') AND t.expires_at_ms > ?3
                        AND a.status IN ('approved', 'consumed') AND a.expires_at_ms > ?3
                        AND i.account_id = a.account_id
                        AND i.application_id = a.application_id
                        AND i.status = 'approved'
                      ORDER BY a.created_at_ms DESC LIMIT 1",
                    params![run_id, ticket_hash, now],
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
            let Some((
                id,
                account_id,
                application_id,
                intervention_id,
                action,
                expires_at_ms,
                action_status,
            )) = value
            else {
                tx.commit()?;
                return Ok(None);
            };
            let action_changed = tx.execute(
                "UPDATE jobs_local_run_resume_actions
                    SET status = 'consumed', consumed_at_ms = COALESCE(consumed_at_ms, ?2)
                  WHERE id = ?1 AND status IN ('approved', 'consumed')",
                params![id, now],
            )?;
            let ticket_changed = tx.execute(
                "UPDATE jobs_local_run_tickets
                    SET updated_at_ms = CASE WHEN status = 'needs_input' THEN ?3 ELSE updated_at_ms END,
                        status = 'claimed'
                  WHERE id = ?1 AND ticket_hash = ?2
                    AND status IN ('needs_input', 'claimed')",
                params![run_id, ticket_hash, now],
            )?;
            if action_changed != 1 || ticket_changed != 1 {
                return Ok(None);
            }
            tx.commit()?;
            Ok(Some(LocalRunResumeAction {
                run_id: run_id.to_string(),
                intervention_id,
                action,
                expires_at_ms,
                account_id,
                application_id,
                first_consumption: action_status == "approved",
            }))
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            let value = tx.query_opt(
                "SELECT a.id, a.account_id, a.application_id, a.intervention_id,
                        a.action, a.expires_at_ms, a.status
                  FROM jobs_local_run_resume_actions a
                   JOIN jobs_local_run_tickets t ON t.id = a.run_id
                   JOIN jobs_interventions i ON i.id = a.intervention_id
                  WHERE a.run_id = $1 AND t.ticket_hash = $2
                    AND t.status IN ('needs_input', 'claimed') AND t.expires_at_ms > $3
                    AND a.status IN ('approved', 'consumed') AND a.expires_at_ms > $3
                    AND i.account_id = a.account_id
                    AND i.application_id = a.application_id
                    AND i.status = 'approved'
                  ORDER BY a.created_at_ms DESC LIMIT 1
                  FOR UPDATE OF a, t",
                &[&run_id, &ticket_hash, &now],
            )?;
            let Some(row) = value else {
                tx.commit()?;
                return Ok(None);
            };
            let id: String = row.get(0);
            let account_id: String = row.get(1);
            let application_id: String = row.get(2);
            let intervention_id: String = row.get(3);
            let action: String = row.get(4);
            let expires_at_ms: i64 = row.get(5);
            let action_status: String = row.get(6);
            let action_changed = tx.execute(
                "UPDATE jobs_local_run_resume_actions
                    SET status = 'consumed', consumed_at_ms = COALESCE(consumed_at_ms, $2)
                  WHERE id = $1 AND status IN ('approved', 'consumed')",
                &[&id, &now],
            )?;
            let ticket_changed = tx.execute(
                "UPDATE jobs_local_run_tickets
                    SET updated_at_ms = CASE WHEN status = 'needs_input' THEN $3 ELSE updated_at_ms END,
                        status = 'claimed'
                  WHERE id = $1 AND ticket_hash = $2
                    AND status IN ('needs_input', 'claimed')",
                &[&run_id, &ticket_hash, &now],
            )?;
            if action_changed != 1 || ticket_changed != 1 {
                return Ok(None);
            }
            tx.commit()?;
            Ok(Some(LocalRunResumeAction {
                run_id: run_id.to_string(),
                intervention_id,
                action,
                expires_at_ms,
                account_id,
                application_id,
                first_consumption: action_status == "approved",
            }))
        }
    })
}

pub fn local_submission_approval_consumed(
    pool: &DbPool,
    account_id: &str,
    application_id: &str,
    run_id: &str,
) -> Result<bool> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => pool
            .get()?
            .query_row(
                "SELECT EXISTS(
                    SELECT 1 FROM jobs_local_run_resume_actions
                     WHERE run_id = ?1 AND account_id = ?2 AND application_id = ?3
                       AND action = 'approve_submission' AND status = 'consumed'
                )",
                params![run_id, account_id, application_id],
                |row| row.get(0),
            )
            .context("check local submission approval"),
        DbPool::Postgres(_) => pool
            .get_pg()?
            .query_one(
                "SELECT EXISTS(
                    SELECT 1 FROM jobs_local_run_resume_actions
                     WHERE run_id = $1 AND account_id = $2 AND application_id = $3
                       AND action = 'approve_submission' AND status = 'consumed'
                )",
                &[&run_id, &account_id, &application_id],
            )
            .map(|row| row.get(0))
            .context("check local submission approval"),
    })
}

fn local_run_ticket_from_sqlite_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<LocalRunTicket> {
    let encrypted_ticket: String = row.get(4)?;
    let encrypted_payload: String = row.get(5)?;
    let ticket_secret = decrypt_payload(&encrypted_ticket).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(4, rusqlite::types::Type::Text, error.into())
    })?;
    let payload = parse_json(encrypted_payload, "Jobs local browser packet").map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(5, rusqlite::types::Type::Text, error.into())
    })?;
    Ok(LocalRunTicket {
        id: row.get(0)?,
        account_id: row.get(1)?,
        application_id: row.get(2)?,
        ticket_hash: row.get(3)?,
        ticket_secret,
        payload,
        status: row.get(6)?,
        expires_at_ms: row.get(7)?,
        created_at_ms: row.get(8)?,
        updated_at_ms: row.get(9)?,
    })
}

fn local_run_ticket_from_pg_row(row: postgres::Row) -> Result<LocalRunTicket> {
    let encrypted_ticket: String = row.get(4);
    let encrypted_payload: String = row.get(5);
    Ok(LocalRunTicket {
        id: row.get(0),
        account_id: row.get(1),
        application_id: row.get(2),
        ticket_hash: row.get(3),
        ticket_secret: decrypt_payload(&encrypted_ticket)?,
        payload: parse_json(encrypted_payload, "Jobs local browser packet")?,
        status: row.get(6),
        expires_at_ms: row.get(7),
        created_at_ms: row.get(8),
        updated_at_ms: row.get(9),
    })
}
