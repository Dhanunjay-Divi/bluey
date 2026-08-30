pub fn local_result_replay_credential_sha256(capability: &str) -> Option<String> {
    if capability.is_empty() || capability.len() > 4_096 {
        return None;
    }
    let mut parts = capability.split('.');
    let payload = parts.next().unwrap_or_default();
    let signature = parts.next().unwrap_or_default();
    if payload.is_empty()
        || signature.len() != 64
        || !signature.bytes().all(|byte| byte.is_ascii_hexdigit())
        || parts.next().is_some()
    {
        return None;
    }
    Some(hex::encode(Sha256::digest(capability.as_bytes())))
}

/// Verifies possession of the exact nonce-bearing result capability that was
/// frozen into an immutable local submission receipt. This authorizes only an
/// already-Submitted no-op replay and deliberately does not extend the bounded
/// reconciliation window for an uncertain employer outcome.
pub fn submitted_local_receipt_replay_authorized(
    application: &JobApplication,
    run_id: &str,
    result_capability: &str,
) -> bool {
    if application.state != "submitted"
        || application.run_id.as_deref() != Some(run_id)
        || application.receipt.get("runner").and_then(Value::as_str) != Some("local")
        || application.receipt.get("runId").and_then(Value::as_str) != Some(run_id)
    {
        return false;
    }
    let Some(supplied_hash) = local_result_replay_credential_sha256(result_capability) else {
        return false;
    };
    let Some(execution) = application
        .receipt
        .pointer(&format!(
            "/{SERVER_SUBMISSION_AUTHORITY_KEY}/executionAuthority"
        ))
        .and_then(Value::as_object)
    else {
        return false;
    };
    let ticket_hash = execution
        .get("ticketHash")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let stored_hash = execution
        .get("resultCapabilitySha256")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let release_authority_valid = match execution.len() {
        4 => !execution.contains_key("browserRelease"),
        5 => execution
            .get("browserRelease")
            .is_some_and(browser_release_receipt_authority_valid),
        _ => false,
    };
    release_authority_valid
        && execution.get("kind").and_then(Value::as_str) == Some("local_run_ticket")
        && execution.get("runId").and_then(Value::as_str) == Some(run_id)
        && ticket_hash.len() == 64
        && ticket_hash == ticket_hash.to_ascii_lowercase()
        && ticket_hash.bytes().all(|byte| byte.is_ascii_hexdigit())
        && stored_hash.len() == 64
        && stored_hash == stored_hash.to_ascii_lowercase()
        && stored_hash.bytes().all(|byte| byte.is_ascii_hexdigit())
        && supplied_hash.len() == stored_hash.len()
        && supplied_hash
            .as_bytes()
            .ct_eq(stored_hash.as_bytes())
            .unwrap_u8()
            == 1
}

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
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            crate::db::object_uploads::require_active_account_write_fence_sqlite_tx(
                &tx, account_id,
            )?;
            tx.execute(
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
            tx.commit()?;
            Ok(value)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            crate::db::object_uploads::require_active_account_write_fence_postgres_tx(
                &mut tx, account_id,
            )?;
            tx.execute(
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
            tx.commit()?;
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
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let now = local_run_claim_db_now_sqlite(&tx)?;
            let account_id: Option<String> = tx
                .query_row(
                    "SELECT account_id FROM jobs_local_run_tickets
                      WHERE id = ?1 AND ticket_hash = ?2",
                    params![run_id, ticket_hash],
                    |row| row.get(0),
                )
                .optional()?;
            let Some(account_id) = account_id else {
                tx.commit()?;
                return Ok(None);
            };
            crate::db::object_uploads::require_active_account_write_fence_sqlite_tx(
                &tx,
                &account_id,
            )?;
            let authority = sqlite_local_run_authority(
                &tx,
                run_id,
                ticket_hash,
                now,
                LocalRunAuthorityPhase::Claim,
            )?;
            let Some(mut authority) = authority else {
                tx.commit()?;
                return Ok(None);
            };
            match require_local_run_operational_capability_sqlite_after_authority(
                &tx,
                OperationalCapability::RunnerClaim,
                &authority,
            ) {
                Ok(()) => {}
                Err(OperationalHoldError::Storage(error)) => return Err(error),
                Err(_) => {
                    tx.commit()?;
                    return Ok(None);
                }
            }
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
            authority.ticket.status = "claimed".to_string();
            authority.ticket.updated_at_ms = now;
            Ok(Some(authority.ticket))
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            lock_operational_hold_shared_postgres_tx(&mut tx).map_err(anyhow::Error::new)?;
            lock_managed_cloud_release_registry_shared_postgres_tx(&mut tx)
                .map_err(anyhow::Error::new)?;
            lock_postgres_ats_certification(&mut tx)?;
            let account_id = tx
                .query_opt(
                    "SELECT account_id FROM jobs_local_run_tickets
                      WHERE id = $1 AND ticket_hash = $2",
                    &[&run_id, &ticket_hash],
                )?
                .map(|row| row.get::<_, String>(0));
            let Some(account_id) = account_id else {
                tx.commit()?;
                return Ok(None);
            };
            lock_discovery_account_shared_postgres(&mut tx, &account_id)?;
            crate::db::object_uploads::require_active_account_write_fence_postgres_tx(
                &mut tx,
                &account_id,
            )?;
            let authority_prelock = postgres_local_run_authority_prelock(
                &mut tx,
                &account_id,
                run_id,
                ticket_hash,
                LocalRunAuthorityPhase::Claim,
            )?;
            let Some(authority_prelock) = authority_prelock else {
                tx.commit()?;
                return Ok(None);
            };
            let now = local_run_claim_db_now_postgres(&mut tx)?;
            let authority =
                postgres_local_run_authority_after_prelock_at_ms(&mut tx, authority_prelock, now)?;
            let Some(mut authority) = authority else {
                tx.commit()?;
                return Ok(None);
            };
            match require_local_run_operational_capability_postgres_after_authority_prelock(
                &mut tx,
                OperationalCapability::RunnerClaim,
                &authority,
            ) {
                Ok(()) => {}
                Err(OperationalHoldError::Storage(error)) => return Err(error),
                Err(_) => {
                    tx.commit()?;
                    return Ok(None);
                }
            }
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
            authority.ticket.status = "claimed".to_string();
            authority.ticket.updated_at_ms = now;
            Ok(Some(authority.ticket))
        }
    })
}

fn local_run_claim_db_now_sqlite(tx: &rusqlite::Transaction<'_>) -> Result<i64> {
    tx.query_row(
        "SELECT CAST(unixepoch('subsec') * 1000 AS INTEGER)",
        [],
        |row| row.get(0),
    )
    .context("sample SQLite local-run claim database time")
}

fn local_run_claim_db_now_postgres(tx: &mut postgres::Transaction<'_>) -> Result<i64> {
    tx.query_one(
        "SELECT floor(extract(epoch FROM clock_timestamp()) * 1000)::bigint",
        &[],
    )
    .map(|row| row.get(0))
    .context("sample PostgreSQL local-run claim database time")
}

#[cfg(test)]
#[test]
fn standalone_local_claim_uses_signed_domain_after_one_postgres_prelock() {
    let source = include_str!("local_runner.rs");
    let claim = source
        .split("pub fn claim_authorized_local_run_ticket(")
        .nth(1)
        .expect("standalone local claim")
        .split("fn local_run_claim_db_now_sqlite(")
        .next()
        .expect("bounded standalone local claim");
    let postgres = claim
        .split("DbPool::Postgres(_) =>")
        .nth(1)
        .expect("PostgreSQL standalone local claim");
    let mut previous = 0;
    for operation in [
        "lock_operational_hold_shared_postgres_tx",
        "lock_managed_cloud_release_registry_shared_postgres_tx",
        "lock_postgres_ats_certification",
        "SELECT account_id FROM jobs_local_run_tickets",
        "lock_discovery_account_shared_postgres",
        "require_active_account_write_fence_postgres_tx",
        "postgres_local_run_authority_prelock",
        "local_run_claim_db_now_postgres",
        "postgres_local_run_authority_after_prelock_at_ms",
        "require_local_run_operational_capability_postgres_after_authority_prelock",
        "UPDATE jobs_local_run_tickets SET status = 'claimed'",
    ] {
        let position = postgres
            .find(operation)
            .unwrap_or_else(|| panic!("missing standalone claim operation {operation}"));
        assert!(position >= previous, "standalone claim order inverted");
        previous = position;
    }
    assert!(!postgres.contains("operational_hold_context_for_application_postgres_tx("));
    assert_eq!(
        postgres.matches("local_run_claim_db_now_postgres").count(),
        1
    );

    let prelock = source
        .rsplit("fn postgres_local_run_authority_prelock(")
        .next()
        .expect("PostgreSQL local-run authority prelock")
        .split("fn postgres_local_run_authority_after_prelock_at_ms(")
        .next()
        .expect("bounded PostgreSQL local-run authority prelock");
    assert!(prelock.contains("lock_current_execution_authority_postgres_after_prelock"));
    assert!(prelock.contains("FOR UPDATE"));
    assert!(prelock.contains("jobs_entitlements"));
    assert!(prelock.contains("FOR SHARE"));
    let application = prelock
        .rfind("FROM jobs_applications")
        .expect("locked application authority");
    let entitlement = prelock
        .find("FROM jobs_entitlements")
        .expect("locked entitlement authority");
    let reservation = prelock
        .find("FROM jobs_attempt_reservations")
        .expect("locked attempt reservation authority");
    let ticket = prelock
        .rfind("FROM jobs_local_run_tickets")
        .expect("locked local-run ticket authority");
    let session = prelock
        .find("FROM jobs_browser_sessions")
        .expect("locked browser-session authority");
    assert!(application < entitlement && entitlement < reservation);
    assert!(reservation < ticket && ticket < session);
    assert!(prelock.contains("local_run_ticket_snapshot_matches"));
    assert!(!prelock.contains("local_run_claim_db_now_postgres"));

    let resolution = source
        .rsplit("fn postgres_local_run_authority_after_prelock_at_ms(")
        .next()
        .expect("PostgreSQL local-run authority at-ms resolution")
        .split("fn local_run_authority_matches(")
        .next()
        .expect("bounded PostgreSQL local-run authority at-ms resolution");
    assert!(resolution.contains("resolve_current_execution_authority_postgres_after_prelock_at_ms"));
    for forbidden in [
        "current_execution_authorized_postgres(",
        "resolve_current_execution_authority_postgres_after_prelock(",
        "lock_current_execution_authority_postgres_after_prelock",
        "lock_operational_hold_shared_postgres_tx",
        "lock_managed_cloud_release_registry_shared_postgres_tx",
        "lock_postgres_ats_certification",
        "lock_discovery_account_shared_postgres",
        "local_run_claim_db_now_postgres",
    ] {
        assert!(
            !resolution.contains(forbidden),
            "authority relocks {forbidden}"
        );
    }
}

#[cfg(test)]
#[test]
fn initial_local_submit_uses_one_post_lock_database_time() {
    let source = include_str!("local_runner.rs");
    let submit = source
        .rsplit("fn local_run_submit_authorization_inner(")
        .next()
        .expect("local FinalSubmit authorization")
        .split("fn local_click_started_ticket_matches(")
        .next()
        .expect("bounded local FinalSubmit authorization");
    let postgres = submit
        .split("DbPool::Postgres(_) =>")
        .nth(1)
        .expect("PostgreSQL local FinalSubmit authorization");
    let initial = postgres
        .split("if require_distribution_ready")
        .nth(1)
        .expect("initial PostgreSQL FinalSubmit path");
    let mut previous = 0;
    for operation in [
        "postgres_runner_volume_fleet_distribution_ready",
        "require_active_account_write_fence_postgres_tx",
        "postgres_local_run_authority_prelock",
        "postgres_lock_browser_release_registry_shared",
        "prelock_local_submission_evidence_capacity_postgres_tx",
        "local_run_claim_db_now_postgres",
        "local_submission_evidence_capacity_at_ms",
        "postgres_local_run_authority_after_prelock_at_ms",
        "postgres_bound_browser_release_submit_allowed_at_ms",
        "require_local_run_operational_capability_postgres_after_authority_prelock",
        "consume_local_ats_certification_postgres_tx",
        "reserve_submission_evidence_capacity_postgres_tx",
        "bind_final_submit_proof_postgres_tx",
        "UPDATE jobs_local_run_tickets",
    ] {
        let position = initial
            .find(operation)
            .unwrap_or_else(|| panic!("missing initial FinalSubmit operation {operation}"));
        assert!(position >= previous, "initial FinalSubmit order inverted");
        previous = position;
    }
    assert_eq!(
        initial.matches("local_run_claim_db_now_postgres").count(),
        1
    );
    assert!(!initial.contains("now_ms()"));
}

#[cfg(test)]
#[test]
fn click_started_submit_replay_samples_database_time_after_exact_capacity_lock() {
    let source = include_str!("local_runner.rs");
    let submit = source
        .rsplit("fn local_run_submit_authorization_inner(")
        .next()
        .expect("local FinalSubmit authorization")
        .split("fn local_click_started_ticket_matches(")
        .next()
        .expect("bounded local FinalSubmit authorization");
    let click_started = submit
        .split("if ticket_status == \"click_started\"")
        .nth(1)
        .expect("PostgreSQL click-started replay branch")
        .split("if require_distribution_ready")
        .next()
        .expect("bounded PostgreSQL click-started replay branch");
    assert!(click_started.contains("require_active_account_write_fence_postgres_tx"));
    assert!(click_started.contains("postgres_local_click_started_submit_replay"));
    assert!(!click_started.contains("now_ms()"));
    assert!(!click_started.contains("local_run_claim_db_now_postgres"));

    let replay = source
        .rsplit("fn postgres_local_click_started_submit_replay(")
        .next()
        .expect("PostgreSQL click-started replay")
        .split("fn local_ats_observed_surface(")
        .next()
        .expect("bounded PostgreSQL click-started replay");
    let mut previous = 0;
    for operation in [
        "FROM jobs_local_run_tickets",
        "postgres_lock_browser_release_registry_shared",
        "postgres_local_click_started_release_matches",
        "recover_terminal_ats_authority_postgres_tx",
        "FROM jobs_applications",
        "FROM jobs_browser_sessions",
        "postgres_local_click_started_capacity_expires_at_ms",
        "local_run_claim_db_now_postgres",
        "capacity_expires_at_ms <= now",
    ] {
        let position = replay
            .find(operation)
            .unwrap_or_else(|| panic!("missing click-started replay operation {operation}"));
        assert!(position >= previous, "click-started replay order inverted");
        previous = position;
    }
    assert_eq!(replay.matches("local_run_claim_db_now_postgres").count(), 1);
    assert!(!replay.contains("now_ms()"));

    let capacity_lock = source
        .rsplit("fn postgres_local_click_started_capacity_expires_at_ms(")
        .next()
        .expect("PostgreSQL click-started capacity lock")
        .split("fn sqlite_local_click_started_submit_replay(")
        .next()
        .expect("bounded PostgreSQL click-started capacity lock");
    assert!(capacity_lock.contains("FOR UPDATE"));
    assert!(capacity_lock.contains("state = 'active'"));
    assert!(!capacity_lock.contains("expires_at_ms >"));
    assert!(!capacity_lock.contains("local_run_claim_db_now_postgres"));
}

/// Atomically validates the exact local-run authority and reserves protected
/// evidence headroom immediately before the local browser crosses the
/// irreversible employer Submit boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LocalRunSubmitAuthorization {
    pub ats_certified_receipt_authority: Option<AtsCertifiedReceiptAuthority>,
}

fn local_submission_evidence_capacity_at_ms(
    capacity: &crate::db::object_uploads::NewSubmissionEvidenceCapacity,
    now_ms: i64,
) -> crate::db::object_uploads::NewSubmissionEvidenceCapacity {
    let mut capacity = capacity.clone();
    capacity.now_ms = now_ms;
    capacity.expires_at_ms = now_ms.saturating_add(SUBMISSION_RECONCILIATION_GRACE_MS);
    capacity
}

fn prelock_local_submission_evidence_capacity_postgres_tx(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
) -> Result<()> {
    // The account row is already held FOR UPDATE, so locking the complete existing set freezes
    // both the account-wide expiry update and the exact reserve/insert decision until commit.
    let _ = tx.query(
        "SELECT application_id, run_id FROM jobs_submission_evidence_capacity
          WHERE account_id = $1 ORDER BY application_id, run_id FOR UPDATE",
        &[&account_id],
    )?;
    Ok(())
}

#[cfg(test)]
pub(crate) fn local_run_submit_authorized_for_server(
    pool: &DbPool,
    run_id: &str,
    ticket_hash: &str,
    server_release_id: &str,
    final_submit_proof: &FinalSubmitProof,
    capacity: &crate::db::object_uploads::NewSubmissionEvidenceCapacity,
) -> Result<bool> {
    Ok(local_run_submit_authorization_inner(
        pool,
        run_id,
        ticket_hash,
        server_release_id,
        final_submit_proof,
        capacity,
        false,
    )?
    .is_some())
}

#[cfg(test)]
pub(crate) fn local_run_submit_authorized_for_distribution(
    pool: &DbPool,
    run_id: &str,
    ticket_hash: &str,
    server_release_id: &str,
    final_submit_proof: &FinalSubmitProof,
    capacity: &crate::db::object_uploads::NewSubmissionEvidenceCapacity,
) -> Result<bool> {
    Ok(local_run_submit_authorization_for_distribution(
        pool,
        run_id,
        ticket_hash,
        server_release_id,
        final_submit_proof,
        capacity,
    )?
    .is_some())
}

pub(crate) fn local_run_submit_authorization_for_distribution(
    pool: &DbPool,
    run_id: &str,
    ticket_hash: &str,
    server_release_id: &str,
    final_submit_proof: &FinalSubmitProof,
    capacity: &crate::db::object_uploads::NewSubmissionEvidenceCapacity,
) -> Result<Option<LocalRunSubmitAuthorization>> {
    local_run_submit_authorization_inner(
        pool,
        run_id,
        ticket_hash,
        server_release_id,
        final_submit_proof,
        capacity,
        true,
    )
}

fn local_run_submit_authorization_inner(
    pool: &DbPool,
    run_id: &str,
    ticket_hash: &str,
    server_release_id: &str,
    final_submit_proof: &FinalSubmitProof,
    capacity: &crate::db::object_uploads::NewSubmissionEvidenceCapacity,
    require_distribution_ready: bool,
) -> Result<Option<LocalRunSubmitAuthorization>> {
    if capacity.run_id != run_id
        || capacity.runner != "local"
        || !browser_release_safe_id(server_release_id)
    {
        return Ok(None);
    }
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let click_started: i64 = tx.query_row(
                "SELECT EXISTS(
                    SELECT 1 FROM jobs_local_run_tickets
                     WHERE id = ?1 AND ticket_hash = ?2 AND status = 'click_started'
                 )",
                params![run_id, ticket_hash],
                |row| row.get(0),
            )?;
            if click_started != 0 {
                crate::db::object_uploads::require_active_account_write_fence_sqlite_tx(
                    &tx,
                    &capacity.account_id,
                )?;
                let now = now_ms();
                let authorization = sqlite_local_click_started_submit_replay(
                    &tx,
                    run_id,
                    ticket_hash,
                    server_release_id,
                    final_submit_proof,
                    capacity,
                    now,
                )?;
                tx.commit()?;
                return Ok(authorization);
            }
            if require_distribution_ready && !sqlite_runner_volume_fleet_distribution_ready(&tx)? {
                return Ok(None);
            }
            let now = local_run_claim_db_now_sqlite(&tx)?;
            let capacity = local_submission_evidence_capacity_at_ms(capacity, now);
            let authority = sqlite_local_run_authority(
                &tx,
                run_id,
                ticket_hash,
                now,
                LocalRunAuthorityPhase::Submit,
            )?;
            let Some(authority) = authority else {
                return Ok(None);
            };
            if authority.ticket.account_id != capacity.account_id
                || authority.ticket.application_id != capacity.application_id
            {
                return Ok(None);
            }
            if !sqlite_bound_browser_release_submit_allowed_at_ms(
                &tx,
                run_id,
                server_release_id,
                now,
            )? {
                return Ok(None);
            }
            match require_local_run_operational_capability_sqlite_after_authority(
                &tx,
                OperationalCapability::FinalSubmit,
                &authority,
            ) {
                Ok(()) => {}
                Err(OperationalHoldError::Storage(error)) => return Err(error),
                Err(_) => return Ok(None),
            }
            let ats_certified_receipt_authority = match consume_local_ats_certification_sqlite_tx(
                &tx,
                final_submit_proof,
                &capacity,
                now,
            )? {
                Some(LocalAtsCertificationConsume::Authorized(authority)) => Some(*authority),
                Some(LocalAtsCertificationConsume::LayoutDriftQuarantined) => {
                    tx.commit()?;
                    return Ok(None);
                }
                None if final_submit_proof.schema_version == 4 => return Ok(None),
                None => None,
            };
            crate::db::object_uploads::reserve_submission_evidence_capacity_sqlite_tx(
                &tx, &capacity,
            )?;
            match bind_final_submit_proof_sqlite_tx(
                &tx,
                &capacity.account_id,
                &capacity.application_id,
                final_submit_proof,
                now,
            ) {
                Ok(()) => {}
                Err(
                    ExecutionLeaseError::InvalidRequest
                    | ExecutionLeaseError::NotFound
                    | ExecutionLeaseError::Conflict,
                ) => return Ok(None),
                Err(ExecutionLeaseError::Storage(error)) => return Err(error),
            }
            if tx.execute(
                "UPDATE jobs_local_run_tickets
                    SET status = 'click_started', updated_at_ms = ?3
                  WHERE id = ?1 AND ticket_hash = ?2 AND status = 'claimed'",
                params![run_id, ticket_hash, now],
            )? != 1
            {
                return Ok(None);
            }
            tx.commit()?;
            Ok(Some(LocalRunSubmitAuthorization {
                ats_certified_receipt_authority,
            }))
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            lock_operational_hold_shared_postgres_tx(&mut tx).map_err(anyhow::Error::new)?;
            lock_managed_cloud_release_registry_shared_postgres_tx(&mut tx)
                .map_err(anyhow::Error::new)?;
            lock_postgres_ats_certification(&mut tx)?;
            lock_discovery_account_shared_postgres(&mut tx, &capacity.account_id)?;
            let ticket_identity = tx
                .query_opt(
                    "SELECT account_id, status FROM jobs_local_run_tickets
                      WHERE id = $1 AND ticket_hash = $2",
                    &[&run_id, &ticket_hash],
                )?
                .map(|row| (row.get::<_, String>(0), row.get::<_, String>(1)));
            let Some((account_id, ticket_status)) = ticket_identity else {
                return Ok(None);
            };
            if account_id != capacity.account_id {
                return Ok(None);
            }
            if ticket_status == "click_started" {
                crate::db::object_uploads::require_active_account_write_fence_postgres_tx(
                    &mut tx,
                    &capacity.account_id,
                )?;
                let authorization = postgres_local_click_started_submit_replay(
                    &mut tx,
                    run_id,
                    ticket_hash,
                    server_release_id,
                    final_submit_proof,
                    capacity,
                )?;
                tx.commit()?;
                return Ok(authorization);
            }
            if require_distribution_ready
                && !postgres_runner_volume_fleet_distribution_ready(&mut tx)?
            {
                return Ok(None);
            }
            crate::db::object_uploads::require_active_account_write_fence_postgres_tx(
                &mut tx,
                &capacity.account_id,
            )?;
            let authority_prelock = postgres_local_run_authority_prelock(
                &mut tx,
                &capacity.account_id,
                run_id,
                ticket_hash,
                LocalRunAuthorityPhase::Submit,
            )?;
            let Some(authority_prelock) = authority_prelock else {
                return Ok(None);
            };
            if authority_prelock.ticket.account_id != capacity.account_id
                || authority_prelock.ticket.application_id != capacity.application_id
            {
                return Ok(None);
            }
            postgres_lock_browser_release_registry_shared(&mut tx)?;
            prelock_local_submission_evidence_capacity_postgres_tx(&mut tx, &capacity.account_id)?;
            let now = local_run_claim_db_now_postgres(&mut tx)?;
            let capacity = local_submission_evidence_capacity_at_ms(capacity, now);
            let authority =
                postgres_local_run_authority_after_prelock_at_ms(&mut tx, authority_prelock, now)?;
            let Some(authority) = authority else {
                return Ok(None);
            };
            if !postgres_bound_browser_release_submit_allowed_at_ms(
                &mut tx,
                run_id,
                server_release_id,
                now,
            )? {
                return Ok(None);
            }
            match require_local_run_operational_capability_postgres_after_authority_prelock(
                &mut tx,
                OperationalCapability::FinalSubmit,
                &authority,
            ) {
                Ok(()) => {}
                Err(OperationalHoldError::Storage(error)) => return Err(error),
                Err(_) => return Ok(None),
            }
            let ats_certified_receipt_authority = match consume_local_ats_certification_postgres_tx(
                &mut tx,
                final_submit_proof,
                &capacity,
                now,
            )? {
                Some(LocalAtsCertificationConsume::Authorized(authority)) => Some(*authority),
                Some(LocalAtsCertificationConsume::LayoutDriftQuarantined) => {
                    tx.commit()?;
                    return Ok(None);
                }
                None if final_submit_proof.schema_version == 4 => return Ok(None),
                None => None,
            };
            crate::db::object_uploads::reserve_submission_evidence_capacity_postgres_tx(
                &mut tx, &capacity,
            )?;
            match bind_final_submit_proof_postgres_tx(
                &mut tx,
                &capacity.account_id,
                &capacity.application_id,
                final_submit_proof,
                now,
            ) {
                Ok(()) => {}
                Err(
                    ExecutionLeaseError::InvalidRequest
                    | ExecutionLeaseError::NotFound
                    | ExecutionLeaseError::Conflict,
                ) => return Ok(None),
                Err(ExecutionLeaseError::Storage(error)) => return Err(error),
            }
            if tx.execute(
                "UPDATE jobs_local_run_tickets
                    SET status = 'click_started', updated_at_ms = $3
                  WHERE id = $1 AND ticket_hash = $2 AND status = 'claimed'",
                &[&run_id, &ticket_hash, &now],
            )? != 1
            {
                return Ok(None);
            }
            tx.commit()?;
            Ok(Some(LocalRunSubmitAuthorization {
                ats_certified_receipt_authority,
            }))
        }
    })
}

fn local_click_started_ticket_matches(
    ticket: &LocalRunTicket,
    run_id: &str,
    capacity: &crate::db::object_uploads::NewSubmissionEvidenceCapacity,
) -> bool {
    ticket.id == run_id
        && ticket.status == "click_started"
        && ticket.account_id == capacity.account_id
        && ticket.application_id == capacity.application_id
        && ticket.payload.get("accountId").and_then(Value::as_str)
            == Some(capacity.account_id.as_str())
        && ticket.payload.get("applicationId").and_then(Value::as_str)
            == Some(capacity.application_id.as_str())
        && ticket.payload.get("runId").and_then(Value::as_str) == Some(run_id)
        && ticket.payload.get("runner").and_then(Value::as_str) == Some("local")
}

fn local_click_started_application_matches(
    application: &JobApplication,
    relational_state: &str,
    run_id: &str,
    final_submit_proof: &FinalSubmitProof,
) -> Result<bool> {
    let presented = serde_json::to_value(final_submit_proof)?;
    Ok(application.state == relational_state
        && relational_state == "running"
        && application.run_id.as_deref() == Some(run_id)
        && application.receipt.get(FINAL_SUBMIT_PROOF_KEY) == Some(&presented))
}

fn local_click_started_session_matches(
    session: &BrowserSession,
    relational_runner: &str,
    relational_status: &str,
    application_id: &str,
    run_id: &str,
) -> bool {
    session.id == run_id
        && session.application_id.as_deref() == Some(application_id)
        && session.runner == relational_runner
        && relational_runner == "local"
        && session.status == relational_status
        && relational_status == "running"
}

fn sqlite_local_click_started_release_matches(
    tx: &rusqlite::Transaction<'_>,
    run_id: &str,
    account_id: &str,
    application_id: &str,
    server_release_id: &str,
) -> Result<bool> {
    let Some(binding) = sqlite_browser_release_binding(tx, run_id, account_id)? else {
        return Ok(false);
    };
    let row: Option<(String, String)> = tx
        .query_row(
            "SELECT b.binding_sha256, activation.accepted_server_release_ids_json
               FROM jobs_local_run_release_bindings b
               JOIN jobs_browser_release_activations activation
                 ON activation.activation_sha256 = b.activation_sha256
                AND activation.manifest_sha256 = b.manifest_sha256
                AND activation.channel = b.channel
                AND activation.activation_generation = b.activation_generation
                AND activation.trust_generation = b.trust_generation
                AND activation.channel_sequence = b.channel_sequence
                AND activation.manifest_signature_set_sha256 =
                    b.manifest_signature_set_sha256
                AND activation.authorization_signature_set_sha256 =
                    b.activation_authorization_signature_set_sha256
              WHERE b.run_id = ?1 AND b.account_id = ?2 AND b.application_id = ?3",
            params![run_id, account_id, application_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let Some((stored_binding_sha256, accepted_server_release_ids_json)) = row else {
        return Ok(false);
    };
    Ok(browser_release_constant_time_eq(
        &stored_binding_sha256,
        &browser_release_binding_sha256(&binding),
    ) && browser_activation_accepts_server(
        &accepted_server_release_ids_json,
        server_release_id,
    )?)
}

fn postgres_local_click_started_release_matches(
    tx: &mut postgres::Transaction<'_>,
    run_id: &str,
    account_id: &str,
    application_id: &str,
    server_release_id: &str,
) -> Result<bool> {
    let Some(binding) = postgres_browser_release_binding(tx, run_id, account_id)? else {
        return Ok(false);
    };
    let row = tx.query_opt(
        "SELECT b.binding_sha256, activation.accepted_server_release_ids_json
           FROM jobs_local_run_release_bindings b
           JOIN jobs_browser_release_activations activation
             ON activation.activation_sha256 = b.activation_sha256
            AND activation.manifest_sha256 = b.manifest_sha256
            AND activation.channel = b.channel
            AND activation.activation_generation = b.activation_generation
            AND activation.trust_generation = b.trust_generation
            AND activation.channel_sequence = b.channel_sequence
            AND activation.manifest_signature_set_sha256 = b.manifest_signature_set_sha256
            AND activation.authorization_signature_set_sha256 =
                b.activation_authorization_signature_set_sha256
          WHERE b.run_id = $1 AND b.account_id = $2 AND b.application_id = $3
          FOR SHARE OF b, activation",
        &[&run_id, &account_id, &application_id],
    )?;
    let Some(row) = row else {
        return Ok(false);
    };
    let stored_binding_sha256: String = row.get(0);
    let accepted_server_release_ids_json: String = row.get(1);
    Ok(browser_release_constant_time_eq(
        &stored_binding_sha256,
        &browser_release_binding_sha256(&binding),
    ) && browser_activation_accepts_server(
        &accepted_server_release_ids_json,
        server_release_id,
    )?)
}

fn sqlite_local_click_started_capacity_matches(
    tx: &rusqlite::Transaction<'_>,
    capacity: &crate::db::object_uploads::NewSubmissionEvidenceCapacity,
    now: i64,
) -> Result<bool> {
    let matches: i64 = tx.query_row(
        "SELECT EXISTS(
            SELECT 1 FROM jobs_submission_evidence_capacity
             WHERE account_id = ?1 AND application_id = ?2 AND run_id = ?3
               AND runner = ?4 AND reserved_bytes = ?5 AND reserved_objects = ?6
               AND state = 'active' AND expires_at_ms > ?7
         )",
        params![
            capacity.account_id,
            capacity.application_id,
            capacity.run_id,
            capacity.runner,
            capacity.reserved_bytes,
            capacity.reserved_objects,
            now,
        ],
        |row| row.get(0),
    )?;
    Ok(matches != 0)
}

fn postgres_local_click_started_capacity_expires_at_ms(
    tx: &mut postgres::Transaction<'_>,
    capacity: &crate::db::object_uploads::NewSubmissionEvidenceCapacity,
) -> Result<Option<i64>> {
    Ok(tx
        .query_opt(
            "SELECT expires_at_ms FROM jobs_submission_evidence_capacity
              WHERE account_id = $1 AND application_id = $2 AND run_id = $3
                AND runner = $4 AND reserved_bytes = $5 AND reserved_objects = $6
                AND state = 'active'
              FOR UPDATE",
            &[
                &capacity.account_id,
                &capacity.application_id,
                &capacity.run_id,
                &capacity.runner,
                &capacity.reserved_bytes,
                &capacity.reserved_objects,
            ],
        )?
        .map(|row| row.get(0)))
}

fn sqlite_local_click_started_submit_replay(
    tx: &rusqlite::Transaction<'_>,
    run_id: &str,
    ticket_hash: &str,
    server_release_id: &str,
    final_submit_proof: &FinalSubmitProof,
    capacity: &crate::db::object_uploads::NewSubmissionEvidenceCapacity,
    now: i64,
) -> Result<Option<LocalRunSubmitAuthorization>> {
    if final_submit_proof.schema_version != 4 {
        return Ok(None);
    }
    let ticket = tx
        .query_row(
            "SELECT id, account_id, application_id, ticket_hash, ticket_secret,
                    payload_json, status, expires_at_ms, created_at_ms, updated_at_ms
               FROM jobs_local_run_tickets
              WHERE id = ?1 AND ticket_hash = ?2 AND status = 'click_started'",
            params![run_id, ticket_hash],
            local_run_ticket_from_sqlite_row,
        )
        .optional()?;
    let Some(ticket) = ticket else {
        return Ok(None);
    };
    if !local_click_started_ticket_matches(&ticket, run_id, capacity)
        || !sqlite_local_click_started_release_matches(
            tx,
            run_id,
            &ticket.account_id,
            &ticket.application_id,
            server_release_id,
        )?
    {
        return Ok(None);
    }
    let authority = match recover_terminal_ats_authority_sqlite_tx(
        tx,
        &ticket.account_id,
        &ticket.application_id,
        run_id,
    ) {
        Ok(authority) => authority,
        Err(ExecutionLeaseError::Storage(error)) => return Err(error),
        Err(_) => return Ok(None),
    };
    let application_row: Option<(String, String, String)> = tx
        .query_row(
            "SELECT job_id, application_json, state FROM jobs_applications
              WHERE account_id = ?1 AND id = ?2",
            params![ticket.account_id, ticket.application_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    let Some((job_id, application_json, application_state)) = application_row else {
        return Ok(None);
    };
    let application = parse_application_json(
        application_json,
        &ticket.application_id,
        &job_id,
        "local click-started replay application",
    )?;
    if !local_click_started_application_matches(
        &application,
        &application_state,
        run_id,
        final_submit_proof,
    )? {
        return Ok(None);
    }
    let session_row: Option<(String, String, String)> = tx
        .query_row(
            "SELECT session_json, runner, status FROM jobs_browser_sessions
              WHERE account_id = ?1 AND id = ?2",
            params![ticket.account_id, run_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    let Some((session_json, session_runner, session_status)) = session_row else {
        return Ok(None);
    };
    let session: BrowserSession = parse_json(session_json, "local click-started replay session")?;
    if !local_click_started_session_matches(
        &session,
        &session_runner,
        &session_status,
        &ticket.application_id,
        run_id,
    ) || !sqlite_local_click_started_capacity_matches(tx, capacity, now)?
    {
        return Ok(None);
    }
    Ok(Some(LocalRunSubmitAuthorization {
        ats_certified_receipt_authority: Some(authority),
    }))
}

fn postgres_local_click_started_submit_replay(
    tx: &mut postgres::Transaction<'_>,
    run_id: &str,
    ticket_hash: &str,
    server_release_id: &str,
    final_submit_proof: &FinalSubmitProof,
    capacity: &crate::db::object_uploads::NewSubmissionEvidenceCapacity,
) -> Result<Option<LocalRunSubmitAuthorization>> {
    if final_submit_proof.schema_version != 4 {
        return Ok(None);
    }
    let ticket = tx
        .query_opt(
            "SELECT id, account_id, application_id, ticket_hash, ticket_secret,
                    payload_json, status, expires_at_ms, created_at_ms, updated_at_ms
               FROM jobs_local_run_tickets
              WHERE id = $1 AND ticket_hash = $2 AND status = 'click_started'
              FOR UPDATE",
            &[&run_id, &ticket_hash],
        )?
        .map(local_run_ticket_from_pg_row)
        .transpose()?;
    let Some(ticket) = ticket else {
        return Ok(None);
    };
    if !local_click_started_ticket_matches(&ticket, run_id, capacity) {
        return Ok(None);
    }
    postgres_lock_browser_release_registry_shared(tx)?;
    if !postgres_local_click_started_release_matches(
        tx,
        run_id,
        &ticket.account_id,
        &ticket.application_id,
        server_release_id,
    )? {
        return Ok(None);
    }
    let authority = match recover_terminal_ats_authority_postgres_tx(
        tx,
        &ticket.account_id,
        &ticket.application_id,
        run_id,
    ) {
        Ok(authority) => authority,
        Err(ExecutionLeaseError::Storage(error)) => return Err(error),
        Err(_) => return Ok(None),
    };
    let application_row = tx.query_opt(
        "SELECT job_id, application_json, state FROM jobs_applications
          WHERE account_id = $1 AND id = $2 FOR UPDATE",
        &[&ticket.account_id, &ticket.application_id],
    )?;
    let Some(application_row) = application_row else {
        return Ok(None);
    };
    let job_id: String = application_row.get(0);
    let application_json: String = application_row.get(1);
    let application_state: String = application_row.get(2);
    let application = parse_application_json(
        application_json,
        &ticket.application_id,
        &job_id,
        "local click-started replay application",
    )?;
    if !local_click_started_application_matches(
        &application,
        &application_state,
        run_id,
        final_submit_proof,
    )? {
        return Ok(None);
    }
    let session_row = tx.query_opt(
        "SELECT session_json, runner, status FROM jobs_browser_sessions
          WHERE account_id = $1 AND id = $2 FOR UPDATE",
        &[&ticket.account_id, &run_id],
    )?;
    let Some(session_row) = session_row else {
        return Ok(None);
    };
    let session: BrowserSession = parse_json(
        session_row.get::<_, String>(0),
        "local click-started replay session",
    )?;
    let session_runner: String = session_row.get(1);
    let session_status: String = session_row.get(2);
    if !local_click_started_session_matches(
        &session,
        &session_runner,
        &session_status,
        &ticket.application_id,
        run_id,
    ) {
        return Ok(None);
    }
    let Some(capacity_expires_at_ms) =
        postgres_local_click_started_capacity_expires_at_ms(tx, capacity)?
    else {
        return Ok(None);
    };
    let now = local_run_claim_db_now_postgres(tx)?;
    if capacity_expires_at_ms <= now {
        return Ok(None);
    }
    Ok(Some(LocalRunSubmitAuthorization {
        ats_certified_receipt_authority: Some(authority),
    }))
}

fn local_ats_observed_surface(final_submit_proof: &FinalSubmitProof) -> Option<AtsObservedSurface> {
    let observed = final_submit_proof.observed_surface.as_ref()?;
    Some(AtsObservedSurface {
        variant_key: observed.variant_key.clone(),
        layout_contract_version: observed.layout_contract_version,
        surface_sha256: observed.surface_sha256.clone(),
    })
}

enum LocalAtsCertificationConsume {
    Authorized(Box<AtsCertifiedReceiptAuthority>),
    LayoutDriftQuarantined,
}

fn consume_local_ats_certification_sqlite_tx(
    tx: &rusqlite::Transaction<'_>,
    final_submit_proof: &FinalSubmitProof,
    capacity: &crate::db::object_uploads::NewSubmissionEvidenceCapacity,
    now_ms: i64,
) -> Result<Option<LocalAtsCertificationConsume>> {
    if final_submit_proof.schema_version != 4 {
        return Ok(None);
    }
    let Some(observed_surface) = local_ats_observed_surface(final_submit_proof) else {
        return Ok(None);
    };
    let request = AtsCertificationPhaseBContextRequest {
        account_id: capacity.account_id.clone(),
        application_id: capacity.application_id.clone(),
        run_id: capacity.run_id.clone(),
        runner_kind: "local".to_string(),
        observed_surface,
        terminal_phase: "consumed".to_string(),
    };
    match validate_consume_reserve_ats_application_certification_from_context_sqlite_tx(
        tx, &request, now_ms,
    ) {
        Ok(AtsCertificationPhaseBTransactionOutcome::Authorized(result)) => {
            Ok(Some(LocalAtsCertificationConsume::Authorized(Box::new(
                result.ats_certified_receipt_authority,
            ))))
        }
        Ok(AtsCertificationPhaseBTransactionOutcome::LayoutDriftQuarantined) => {
            Ok(Some(LocalAtsCertificationConsume::LayoutDriftQuarantined))
        }
        Err(AtsCertificationAuthorityError::Storage(error)) => Err(error),
        Err(_) => Ok(None),
    }
}

fn consume_local_ats_certification_postgres_tx(
    tx: &mut postgres::Transaction<'_>,
    final_submit_proof: &FinalSubmitProof,
    capacity: &crate::db::object_uploads::NewSubmissionEvidenceCapacity,
    now_ms: i64,
) -> Result<Option<LocalAtsCertificationConsume>> {
    if final_submit_proof.schema_version != 4 {
        return Ok(None);
    }
    let Some(observed_surface) = local_ats_observed_surface(final_submit_proof) else {
        return Ok(None);
    };
    let request = AtsCertificationPhaseBContextRequest {
        account_id: capacity.account_id.clone(),
        application_id: capacity.application_id.clone(),
        run_id: capacity.run_id.clone(),
        runner_kind: "local".to_string(),
        observed_surface,
        terminal_phase: "consumed".to_string(),
    };
    match validate_consume_reserve_ats_application_certification_from_context_postgres_tx_after_prelock(
        tx, &request, now_ms,
    ) {
        Ok(AtsCertificationPhaseBTransactionOutcome::Authorized(result)) => {
            Ok(Some(LocalAtsCertificationConsume::Authorized(Box::new(
                result.ats_certified_receipt_authority,
            ))))
        }
        Ok(AtsCertificationPhaseBTransactionOutcome::LayoutDriftQuarantined) => {
            Ok(Some(LocalAtsCertificationConsume::LayoutDriftQuarantined))
        }
        Err(AtsCertificationAuthorityError::Storage(error)) => Err(error),
        Err(_) => Ok(None),
    }
}

#[cfg(test)]
pub fn local_run_submit_authorized(
    pool: &DbPool,
    run_id: &str,
    ticket_hash: &str,
    final_submit_proof: &FinalSubmitProof,
    capacity: &crate::db::object_uploads::NewSubmissionEvidenceCapacity,
) -> Result<bool> {
    local_run_submit_authorized_for_server(
        pool,
        run_id,
        ticket_hash,
        "test-server",
        final_submit_proof,
        capacity,
    )
}

#[derive(Debug, Clone)]
struct LocalRunAuthorityResolution {
    ticket: LocalRunTicket,
    employer_domain: OperationalHoldEmployerDomain,
}

fn require_local_run_operational_capability_sqlite_after_authority(
    tx: &rusqlite::Transaction<'_>,
    capability: OperationalCapability,
    authority: &LocalRunAuthorityResolution,
) -> std::result::Result<(), OperationalHoldError> {
    let context = operational_hold_context_for_application_sqlite_tx_after_authority(
        tx,
        &authority.ticket.account_id,
        &authority.ticket.application_id,
        &authority.employer_domain,
        Some("local"),
        None,
        None,
    )?;
    require_operational_capability_sqlite_tx(tx, capability, &context)
}

fn require_local_run_operational_capability_postgres_after_authority_prelock(
    tx: &mut postgres::Transaction<'_>,
    capability: OperationalCapability,
    authority: &LocalRunAuthorityResolution,
) -> std::result::Result<(), OperationalHoldError> {
    let context = operational_hold_context_for_application_postgres_tx_after_authority_prelock(
        tx,
        &authority.ticket.account_id,
        &authority.ticket.application_id,
        &authority.employer_domain,
        Some("local"),
        None,
        None,
    )?;
    require_operational_capability_postgres_tx_after_authority_prelock(tx, capability, &context)
}

fn sqlite_local_run_authority(
    tx: &rusqlite::Transaction<'_>,
    run_id: &str,
    ticket_hash: &str,
    now: i64,
    phase: LocalRunAuthorityPhase,
) -> Result<Option<LocalRunAuthorityResolution>> {
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
    let packet_matches = local_run_authority_matches(
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
    );
    if !packet_matches {
        return Ok(None);
    }
    let current = resolve_current_execution_authority_sqlite_after_prelock_at_ms(
        tx,
        &ticket.account_id,
        &application,
        ExecutionAuthorityRunner::Local,
        now,
    )?;
    let Some(employer_domain) = current.employer_domain else {
        return Ok(None);
    };
    if !current.authorized {
        return Ok(None);
    }
    Ok(Some(LocalRunAuthorityResolution {
        ticket,
        employer_domain,
    }))
}

struct PostgresLocalRunAuthorityPrelock {
    ticket: LocalRunTicket,
    application: JobApplication,
    application_state: String,
    session: BrowserSession,
    session_runner: String,
    session_status: String,
    identity: ApplicationIdentity,
    identity_status: String,
    local_browser: bool,
    canonical_url: String,
    phase: LocalRunAuthorityPhase,
}

fn local_run_ticket_snapshot_matches(snapshot: &LocalRunTicket, locked: &LocalRunTicket) -> bool {
    snapshot.id == locked.id
        && snapshot.account_id == locked.account_id
        && snapshot.application_id == locked.application_id
        && snapshot.ticket_hash == locked.ticket_hash
        && snapshot.ticket_secret == locked.ticket_secret
        && snapshot.payload == locked.payload
        && snapshot.status == locked.status
        && snapshot.expires_at_ms == locked.expires_at_ms
        && snapshot.created_at_ms == locked.created_at_ms
        && snapshot.updated_at_ms == locked.updated_at_ms
}

fn postgres_local_run_authority_prelock(
    tx: &mut postgres::Transaction<'_>,
    prelocked_account_id: &str,
    run_id: &str,
    ticket_hash: &str,
    phase: LocalRunAuthorityPhase,
) -> Result<Option<PostgresLocalRunAuthorityPrelock>> {
    let expected_ticket_status = match phase {
        LocalRunAuthorityPhase::Claim => "queued",
        LocalRunAuthorityPhase::Submit => "claimed",
    };
    let ticket_snapshot = tx
        .query_opt(
            "SELECT id, account_id, application_id, ticket_hash, ticket_secret,
                    payload_json, status, expires_at_ms, created_at_ms, updated_at_ms
               FROM jobs_local_run_tickets
              WHERE id = $1 AND ticket_hash = $2 AND status = $3",
            &[&run_id, &ticket_hash, &expected_ticket_status],
        )?
        .map(local_run_ticket_from_pg_row)
        .transpose()?;
    let Some(ticket_snapshot) = ticket_snapshot else {
        return Ok(None);
    };
    if ticket_snapshot.account_id != prelocked_account_id {
        return Ok(None);
    }
    let Some(application_snapshot) = tx.query_opt(
        "SELECT job_id, application_json, state FROM jobs_applications
          WHERE account_id = $1 AND id = $2",
        &[&ticket_snapshot.account_id, &ticket_snapshot.application_id],
    )?
    else {
        return Ok(None);
    };
    let snapshot_job_id: String = application_snapshot.get(0);
    let snapshot_application_raw: String = application_snapshot.get(1);
    let snapshot_application_state: String = application_snapshot.get(2);
    let snapshot_application = parse_application_json(
        snapshot_application_raw.clone(),
        &ticket_snapshot.application_id,
        &snapshot_job_id,
        "job application prelock",
    )?;
    if !lock_current_execution_authority_postgres_after_prelock(
        tx,
        &ticket_snapshot.account_id,
        &snapshot_application,
    )? {
        return Ok(None);
    }
    let Some(application_row) = tx.query_opt(
        "SELECT job_id, application_json, state FROM jobs_applications
          WHERE account_id = $1 AND id = $2 FOR UPDATE",
        &[&ticket_snapshot.account_id, &ticket_snapshot.application_id],
    )?
    else {
        return Ok(None);
    };
    let job_id: String = application_row.get(0);
    let application_raw: String = application_row.get(1);
    let application_state: String = application_row.get(2);
    if job_id != snapshot_job_id
        || application_raw != snapshot_application_raw
        || application_state != snapshot_application_state
    {
        return Ok(None);
    }
    let application = parse_application_json(
        application_raw,
        &ticket_snapshot.application_id,
        &job_id,
        "job application",
    )?;
    let local_browser = tx
        .query_opt(
            "SELECT local_browser FROM jobs_entitlements
              WHERE account_id = $1 FOR SHARE",
            &[&ticket_snapshot.account_id],
        )?
        .map(|row| row.get::<_, i32>(0) != 0)
        .unwrap_or(false);
    let expected_reservation_status = match phase {
        LocalRunAuthorityPhase::Claim => "reserved",
        LocalRunAuthorityPhase::Submit => "running",
    };
    let reservation_status = tx
        .query_opt(
            "SELECT status FROM jobs_attempt_reservations
              WHERE account_id = $1 AND application_id = $2 FOR UPDATE",
            &[&ticket_snapshot.account_id, &ticket_snapshot.application_id],
        )?
        .map(|row| row.get::<_, String>(0));
    let Some(reservation_status) = reservation_status else {
        return Ok(None);
    };
    if reservation_status != expected_reservation_status {
        return Ok(None);
    }
    let ticket = tx
        .query_opt(
            "SELECT id, account_id, application_id, ticket_hash, ticket_secret,
                    payload_json, status, expires_at_ms, created_at_ms, updated_at_ms
               FROM jobs_local_run_tickets
              WHERE id = $1 AND ticket_hash = $2 AND status = $3
              FOR UPDATE",
            &[&run_id, &ticket_hash, &expected_ticket_status],
        )?
        .map(local_run_ticket_from_pg_row)
        .transpose()?;
    let Some(ticket) = ticket else {
        return Ok(None);
    };
    if !local_run_ticket_snapshot_matches(&ticket_snapshot, &ticket) {
        return Ok(None);
    }
    let identity_id = application
        .receipt
        .pointer("/application_identity/id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let session_row = tx.query_opt(
        "SELECT session_json, runner, status FROM jobs_browser_sessions
          WHERE account_id = $1 AND id = $2 FOR UPDATE",
        &[&ticket.account_id, &run_id],
    )?;
    let identity_row = tx.query_opt(
        "SELECT identity_json, verification_status, is_default
           FROM jobs_application_identities
          WHERE account_id = $1 AND id = $2",
        &[&ticket.account_id, &identity_id],
    )?;
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
    Ok(Some(PostgresLocalRunAuthorityPrelock {
        ticket,
        application,
        application_state,
        session,
        session_runner,
        session_status,
        identity,
        identity_status,
        local_browser,
        canonical_url,
        phase,
    }))
}

fn postgres_local_run_authority_after_prelock_at_ms(
    tx: &mut postgres::Transaction<'_>,
    prelock: PostgresLocalRunAuthorityPrelock,
    now: i64,
) -> Result<Option<LocalRunAuthorityResolution>> {
    let PostgresLocalRunAuthorityPrelock {
        ticket,
        application,
        application_state,
        session,
        session_runner,
        session_status,
        identity,
        identity_status,
        local_browser,
        canonical_url,
        phase,
    } = prelock;
    let packet_matches = local_run_authority_matches(
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
    );
    if !packet_matches {
        return Ok(None);
    }
    let current = resolve_current_execution_authority_postgres_after_prelock_at_ms(
        tx,
        &ticket.account_id,
        &application,
        ExecutionAuthorityRunner::Local,
        now,
    )?;
    let Some(employer_domain) = current.employer_domain else {
        return Ok(None);
    };
    if !current.authorized {
        return Ok(None);
    }
    Ok(Some(LocalRunAuthorityResolution {
        ticket,
        employer_domain,
    }))
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
    if !matches!(status, "needs_input" | "complete" | "failed") {
        return Ok(false);
    }
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let now = now_ms();
            let account_id: Option<String> = tx
                .query_row(
                    "SELECT account_id FROM jobs_local_run_tickets
                      WHERE id = ?1 AND ticket_hash = ?2",
                    params![run_id, ticket_hash],
                    |row| row.get(0),
                )
                .optional()?;
            let Some(account_id) = account_id else {
                tx.commit()?;
                return Ok(false);
            };
            crate::db::object_uploads::require_active_account_write_fence_sqlite_tx(
                &tx,
                &account_id,
            )?;
            let changed = tx.execute(
                "UPDATE jobs_local_run_tickets SET status = ?3, updated_at_ms = ?4
                  WHERE id = ?1 AND ticket_hash = ?2 AND expires_at_ms > ?4
                    AND (status = 'claimed' OR status = ?3)",
                params![run_id, ticket_hash, status, now],
            )? > 0;
            tx.commit()?;
            Ok(changed)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            let account_id = tx
                .query_opt(
                    "SELECT account_id FROM jobs_local_run_tickets
                      WHERE id = $1 AND ticket_hash = $2",
                    &[&run_id, &ticket_hash],
                )?
                .map(|row| row.get::<_, String>(0));
            let Some(account_id) = account_id else {
                tx.commit()?;
                return Ok(false);
            };
            crate::db::object_uploads::require_active_account_write_fence_postgres_tx(
                &mut tx,
                &account_id,
            )?;
            let locked_account_id = tx
                .query_opt(
                    "SELECT account_id FROM jobs_local_run_tickets
                      WHERE id = $1 AND ticket_hash = $2 FOR UPDATE",
                    &[&run_id, &ticket_hash],
                )?
                .map(|row| row.get::<_, String>(0));
            if locked_account_id.as_deref() != Some(account_id.as_str()) {
                tx.commit()?;
                return Ok(false);
            }
            let now = local_run_claim_db_now_postgres(&mut tx)?;
            let changed = tx.execute(
                "UPDATE jobs_local_run_tickets SET status = $3, updated_at_ms = $4
                  WHERE id = $1 AND ticket_hash = $2 AND expires_at_ms > $4
                    AND (status = 'claimed' OR status = $3)",
                &[&run_id, &ticket_hash, &status, &now],
            )? > 0;
            tx.commit()?;
            Ok(changed)
        }
    })
}

#[derive(Debug, Clone)]
pub struct LocalRunResultCommitInput {
    pub status: String,
    pub intervention: Option<Intervention>,
    pub takeover_url: Option<String>,
}

fn local_result_intervention_id(account_id: &str, application_id: &str, run_id: &str) -> String {
    let mut digest = Sha256::new();
    digest.update(b"bluey-jobs-local-result-intervention-v1\0");
    digest.update(account_id.as_bytes());
    digest.update(b"\0");
    digest.update(application_id.as_bytes());
    digest.update(b"\0");
    digest.update(run_id.as_bytes());
    format!("local-result-{}", hex::encode(digest.finalize()))
}

fn normalize_local_result_intervention(
    account_id: &str,
    application_id: &str,
    run_id: &str,
    mut intervention: Intervention,
) -> Result<Intervention> {
    intervention.id = local_result_intervention_id(account_id, application_id, run_id);
    intervention.application_id = Some(application_id.to_string());
    intervention.kind = intervention.kind.trim().to_ascii_lowercase();
    intervention.status = intervention.status.trim().to_ascii_lowercase();
    intervention.resolution_kind = intervention.resolution_kind.trim().to_ascii_lowercase();
    intervention.provider = intervention.provider.trim().to_ascii_lowercase();
    intervention.created_at_ms = 0;
    intervention.resolved_at_ms = None;
    if !matches!(
        intervention.kind.as_str(),
        "captcha"
            | "two_factor"
            | "assessment"
            | "unknown_question"
            | "missing_fact"
            | "sensitive_question"
            | "browser_takeover"
    ) || intervention.status != "open"
        || !matches!(
            intervention.resolution_kind.as_str(),
            "" | "browser_takeover" | "email_otp_approval" | "answer"
        )
        || (intervention.resolution_kind == "email_otp_approval"
            && (intervention.kind != "two_factor"
                || !matches!(intervention.provider.as_str(), "gmail" | "outlook_email")
                || intervention.provider_message_id.trim().is_empty()
                || intervention.expires_at_ms.is_none()))
    {
        anyhow::bail!("invalid local result intervention")
    }
    if intervention.metadata.is_null() {
        intervention.metadata = json!({});
    }
    if contains_authentication_secret(&intervention.metadata) {
        anyhow::bail!("authentication codes and credentials cannot be stored in interventions")
    }
    Ok(intervention)
}

fn local_result_intervention_matches(expected: &Intervention, existing: &Intervention) -> bool {
    expected.id == existing.id
        && expected.application_id == existing.application_id
        && expected.kind == existing.kind
        && existing.status == "open"
        && expected.title == existing.title
        && expected.detail == existing.detail
        && expected.choices == existing.choices
        && expected.resolution_kind == existing.resolution_kind
        && expected.resume_after_resolution == existing.resume_after_resolution
        && expected.provider == existing.provider
        && expected.provider_message_id == existing.provider_message_id
        && expected.expires_at_ms == existing.expires_at_ms
        && expected.metadata == existing.metadata
        && existing.created_at_ms > 0
        && existing.resolved_at_ms.is_none()
}

#[allow(clippy::too_many_arguments)]
fn prepare_local_result_commit(
    application_id: &str,
    run_id: &str,
    ticket_status: &str,
    ticket_expires_at_ms: i64,
    reservation_status: &str,
    capacity_state: Option<&str>,
    mut application: JobApplication,
    mut session: BrowserSession,
    mut intervention: Option<Intervention>,
    existing_intervention: Option<&Intervention>,
    takeover_url: Option<&str>,
    status: &str,
    now: i64,
) -> Result<(JobApplication, BrowserSession, Option<Intervention>, bool)> {
    if ticket_expires_at_ms <= now
        || !matches!(ticket_status, "claimed" | "needs_input" | "failed")
        || (ticket_status != "claimed" && ticket_status != status)
        || application.id != application_id
        || application.run_id.as_deref() != Some(run_id)
        || session.id != run_id
        || session.runner != "local"
        || session.application_id.as_deref() != Some(application_id)
        || !matches!(
            session.status.as_str(),
            "running" | "needs_input" | "failed"
        )
    {
        anyhow::bail!("local run ticket is not active")
    }
    let expected_reservation_status = if status == "failed" {
        "released"
    } else {
        "running"
    };
    if !matches!(reservation_status, "running" | "released")
        || (reservation_status == "released" && expected_reservation_status != "released")
    {
        anyhow::bail!("application attempt reservation changed")
    }
    let application_replay = application.state == status;
    let expected_session_step = if status == "failed" {
        "Run stopped"
    } else {
        "Waiting for your input"
    };
    let expected_takeover_url = if status == "failed" {
        None
    } else {
        takeover_url
            .map(str::to_string)
            .or_else(|| session.takeover_url.clone())
    };
    let session_replay = session.status == status
        && session.current_step == expected_session_step
        && session.takeover_url.as_deref() == expected_takeover_url.as_deref();
    validate_application_transition(&application.state, status)?;
    application.state = status.to_string();
    application.updated_at_ms = now;

    session.status = status.to_string();
    session.current_step = if status == "failed" {
        session.takeover_url = None;
        "Run stopped".to_string()
    } else {
        if let Some(takeover_url) = takeover_url {
            session.takeover_url = Some(takeover_url.to_string());
        }
        "Waiting for your input".to_string()
    };
    session.updated_at_ms = now;

    if let Some(value) = intervention.as_mut() {
        if value
            .expires_at_ms
            .is_some_and(|expires_at_ms| expires_at_ms <= now)
        {
            anyhow::bail!("local result intervention has expired")
        }
        value.created_at_ms = now;
    }
    match (intervention.as_ref(), existing_intervention) {
        (Some(expected), Some(existing))
            if !local_result_intervention_matches(expected, existing) =>
        {
            anyhow::bail!("local result intervention changed")
        }
        (None, Some(_)) if status == "failed" => {}
        (Some(_), _) if status == "needs_input" => {}
        (None, _) if status == "failed" => {}
        _ => anyhow::bail!("local result intervention is missing"),
    }

    let capacity_complete = status != "failed" || capacity_state != Some("active");
    let exact_intervention = status != "needs_input"
        || intervention
            .as_ref()
            .zip(existing_intervention)
            .is_some_and(|(expected, existing)| {
                local_result_intervention_matches(expected, existing)
            });
    let replay = ticket_status == status
        && application_replay
        && session_replay
        && reservation_status == expected_reservation_status
        && capacity_complete
        && exact_intervention;
    Ok((application, session, intervention, replay))
}

pub fn commit_local_run_result(
    pool: &DbPool,
    run_id: &str,
    ticket_hash: &str,
    input: LocalRunResultCommitInput,
) -> Result<JobApplication> {
    if !matches!(input.status.as_str(), "needs_input" | "failed")
        || (input.status == "needs_input") != input.intervention.is_some()
        || input
            .takeover_url
            .as_deref()
            .is_some_and(|url| !url.starts_with("https://") && !url.starts_with("bluey-jobs://"))
    {
        anyhow::bail!("invalid local run result")
    }
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let discovered = tx
                .query_row(
                    "SELECT account_id, application_id FROM jobs_local_run_tickets
                      WHERE id = ?1 AND ticket_hash = ?2",
                    params![run_id, ticket_hash],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
                )
                .optional()?;
            let Some((account_id, application_id)) = discovered else {
                anyhow::bail!("local run ticket is not active")
            };
            crate::db::object_uploads::require_active_account_write_fence_sqlite_tx(
                &tx,
                &account_id,
            )?;
            let application_row = tx
                .query_row(
                    "SELECT job_id, state, application_json FROM jobs_applications
                      WHERE account_id = ?1 AND id = ?2",
                    params![account_id, application_id],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                        ))
                    },
                )
                .optional()?
                .ok_or_else(|| anyhow::anyhow!("application not found"))?;
            let ticket = tx
                .query_row(
                    "SELECT status, expires_at_ms FROM jobs_local_run_tickets
                      WHERE id = ?1 AND account_id = ?2 AND application_id = ?3
                        AND ticket_hash = ?4",
                    params![run_id, account_id, application_id, ticket_hash],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
                )
                .optional()?
                .ok_or_else(|| anyhow::anyhow!("local run ticket is not active"))?;
            let session_row = tx
                .query_row(
                    "SELECT runner, status, session_json FROM jobs_browser_sessions
                      WHERE account_id = ?1 AND id = ?2",
                    params![account_id, run_id],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                        ))
                    },
                )
                .optional()?
                .ok_or_else(|| anyhow::anyhow!("browser session changed"))?;
            let reservation_status = tx
                .query_row(
                    "SELECT status FROM jobs_attempt_reservations
                      WHERE account_id = ?1 AND application_id = ?2",
                    params![account_id, application_id],
                    |row| row.get::<_, String>(0),
                )
                .optional()?
                .ok_or_else(|| anyhow::anyhow!("application attempt reservation changed"))?;
            let capacity_state = tx
                .query_row(
                    "SELECT state FROM jobs_submission_evidence_capacity
                      WHERE account_id = ?1 AND application_id = ?2 AND run_id = ?3",
                    params![account_id, application_id, run_id],
                    |row| row.get::<_, String>(0),
                )
                .optional()?;
            let normalized_intervention = input
                .intervention
                .clone()
                .map(|value| {
                    normalize_local_result_intervention(&account_id, &application_id, run_id, value)
                })
                .transpose()?;
            let existing_intervention = normalized_intervention
                .as_ref()
                .map(|value| {
                    tx.query_row(
                        "SELECT intervention_json FROM jobs_interventions
                          WHERE id = ?1 AND account_id = ?2 AND application_id = ?3",
                        params![value.id, account_id, application_id],
                        |row| row.get::<_, String>(0),
                    )
                    .optional()
                })
                .transpose()?
                .flatten()
                .map(|raw| parse_json(raw, "local result intervention"))
                .transpose()?;
            let _: i64 = tx.query_row(
                "SELECT COUNT(*) FROM jobs_interventions
                  WHERE account_id = ?1 AND application_id = ?2",
                params![account_id, application_id],
                |row| row.get(0),
            )?;
            let now = local_run_claim_db_now_sqlite(&tx)?;
            let application = parse_application_json(
                application_row.2.clone(),
                &application_id,
                &application_row.0,
                "Jobs application",
            )?;
            if application.state != application_row.1 {
                anyhow::bail!("application changed before local result")
            }
            let session: BrowserSession = parse_json(session_row.2.clone(), "browser session")?;
            if session.runner != session_row.0 || session.status != session_row.1 {
                anyhow::bail!("browser session changed")
            }
            let (planned_application, planned_session, intervention, replay) =
                prepare_local_result_commit(
                    &application_id,
                    run_id,
                    &ticket.0,
                    ticket.1,
                    &reservation_status,
                    capacity_state.as_deref(),
                    application,
                    session,
                    normalized_intervention,
                    existing_intervention.as_ref(),
                    input.takeover_url.as_deref(),
                    &input.status,
                    now,
                )?;
            if replay {
                tx.commit()?;
                return Ok(planned_application);
            }
            let application_payload = to_json(&planned_application, "Jobs application")?;
            let session_payload = to_json(&planned_session, "browser session")?;
            if tx.execute(
                "UPDATE jobs_applications SET state = ?3, application_json = ?4,
                        updated_at_ms = ?5
                  WHERE account_id = ?1 AND id = ?2 AND state = ?6 AND application_json = ?7",
                params![
                    account_id,
                    application_id,
                    input.status,
                    application_payload,
                    now,
                    application_row.1,
                    application_row.2,
                ],
            )? != 1
                || tx.execute(
                    "UPDATE jobs_browser_sessions SET status = ?3, session_json = ?4,
                            updated_at_ms = ?5
                      WHERE account_id = ?1 AND id = ?2 AND runner = 'local'
                        AND status = ?6 AND session_json = ?7",
                    params![
                        account_id,
                        run_id,
                        input.status,
                        session_payload,
                        now,
                        session_row.1,
                        session_row.2,
                    ],
                )? != 1
                || tx.execute(
                    "UPDATE jobs_local_run_tickets SET status = ?3, updated_at_ms = ?4
                      WHERE id = ?1 AND ticket_hash = ?2 AND status = ?5
                        AND expires_at_ms > ?4",
                    params![run_id, ticket_hash, input.status, now, ticket.0],
                )? != 1
            {
                anyhow::bail!("local run result changed before commit")
            }
            if input.status == "failed" {
                if reservation_status != "released"
                    && tx.execute(
                        "UPDATE jobs_attempt_reservations SET status = 'released', updated_at_ms = ?3
                          WHERE account_id = ?1 AND application_id = ?2 AND status = ?4",
                        params![account_id, application_id, now, reservation_status],
                    )? != 1
                {
                    anyhow::bail!("application attempt reservation changed")
                }
                tx.execute(
                    "UPDATE jobs_submission_evidence_capacity
                        SET state = 'released', updated_at_ms = ?4, completed_at_ms = ?4
                      WHERE account_id = ?1 AND application_id = ?2 AND run_id = ?3
                        AND state = 'active'",
                    params![account_id, application_id, run_id, now],
                )?;
            }
            if let Some(intervention) = intervention.filter(|_| existing_intervention.is_none()) {
                let payload = to_json(&intervention, "intervention")?;
                tx.execute(
                    "INSERT INTO jobs_interventions (
                        id, account_id, application_id, kind, status, intervention_json,
                        created_at_ms, resolved_at_ms
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL)",
                    params![
                        intervention.id,
                        account_id,
                        intervention.application_id,
                        intervention.kind,
                        intervention.status,
                        payload,
                        intervention.created_at_ms,
                    ],
                )?;
            }
            tx.commit()?;
            Ok(planned_application)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            let discovered = tx.query_opt(
                "SELECT account_id, application_id FROM jobs_local_run_tickets
                  WHERE id = $1 AND ticket_hash = $2",
                &[&run_id, &ticket_hash],
            )?;
            let Some(discovered) = discovered else {
                anyhow::bail!("local run ticket is not active")
            };
            let account_id: String = discovered.get(0);
            let application_id: String = discovered.get(1);
            crate::db::object_uploads::require_active_account_write_fence_postgres_tx(
                &mut tx,
                &account_id,
            )?;
            let application_row = tx
                .query_opt(
                    "SELECT job_id, state, application_json FROM jobs_applications
                      WHERE account_id = $1 AND id = $2 FOR UPDATE",
                    &[&account_id, &application_id],
                )?
                .ok_or_else(|| anyhow::anyhow!("application not found"))?;
            let application_job_id: String = application_row.get(0);
            let application_state: String = application_row.get(1);
            let application_raw: String = application_row.get(2);
            let ticket = tx
                .query_opt(
                    "SELECT status, expires_at_ms FROM jobs_local_run_tickets
                      WHERE id = $1 AND account_id = $2 AND application_id = $3
                        AND ticket_hash = $4 FOR UPDATE",
                    &[&run_id, &account_id, &application_id, &ticket_hash],
                )?
                .ok_or_else(|| anyhow::anyhow!("local run ticket is not active"))?;
            let ticket_status: String = ticket.get(0);
            let ticket_expires_at_ms: i64 = ticket.get(1);
            let session_row = tx
                .query_opt(
                    "SELECT runner, status, session_json FROM jobs_browser_sessions
                      WHERE account_id = $1 AND id = $2 FOR UPDATE",
                    &[&account_id, &run_id],
                )?
                .ok_or_else(|| anyhow::anyhow!("browser session changed"))?;
            let session_runner: String = session_row.get(0);
            let session_status: String = session_row.get(1);
            let session_raw: String = session_row.get(2);
            let reservation_status = tx
                .query_opt(
                    "SELECT status FROM jobs_attempt_reservations
                      WHERE account_id = $1 AND application_id = $2 FOR UPDATE",
                    &[&account_id, &application_id],
                )?
                .map(|row| row.get::<_, String>(0))
                .ok_or_else(|| anyhow::anyhow!("application attempt reservation changed"))?;
            let capacity_state = tx
                .query_opt(
                    "SELECT state FROM jobs_submission_evidence_capacity
                      WHERE account_id = $1 AND application_id = $2 AND run_id = $3 FOR UPDATE",
                    &[&account_id, &application_id, &run_id],
                )?
                .map(|row| row.get::<_, String>(0));
            let normalized_intervention = input
                .intervention
                .clone()
                .map(|value| {
                    normalize_local_result_intervention(&account_id, &application_id, run_id, value)
                })
                .transpose()?;
            let intervention_namespace_id =
                local_result_intervention_id(&account_id, &application_id, run_id);
            tx.query_one(
                "SELECT pg_advisory_xact_lock(hashtextextended($1, 0))",
                &[&format!("jobs-intervention:{intervention_namespace_id}")],
            )?;
            let existing_intervention = if let Some(value) = normalized_intervention.as_ref() {
                tx.query_opt(
                    "SELECT intervention_json FROM jobs_interventions
                      WHERE id = $1 AND account_id = $2 AND application_id = $3 FOR UPDATE",
                    &[&value.id, &account_id, &application_id],
                )?
                .map(|row| parse_json(row.get(0), "local result intervention"))
                .transpose()?
            } else {
                tx.query(
                    "SELECT id FROM jobs_interventions
                      WHERE account_id = $1 AND application_id = $2 ORDER BY id FOR UPDATE",
                    &[&account_id, &application_id],
                )?;
                None
            };
            let now = local_run_claim_db_now_postgres(&mut tx)?;
            let application = parse_application_json(
                application_raw.clone(),
                &application_id,
                &application_job_id,
                "Jobs application",
            )?;
            if application.state != application_state {
                anyhow::bail!("application changed before local result")
            }
            let session: BrowserSession = parse_json(session_raw.clone(), "browser session")?;
            if session.runner != session_runner || session.status != session_status {
                anyhow::bail!("browser session changed")
            }
            let (planned_application, planned_session, intervention, replay) =
                prepare_local_result_commit(
                    &application_id,
                    run_id,
                    &ticket_status,
                    ticket_expires_at_ms,
                    &reservation_status,
                    capacity_state.as_deref(),
                    application,
                    session,
                    normalized_intervention,
                    existing_intervention.as_ref(),
                    input.takeover_url.as_deref(),
                    &input.status,
                    now,
                )?;
            if replay {
                tx.commit()?;
                return Ok(planned_application);
            }
            let application_payload = to_json(&planned_application, "Jobs application")?;
            let session_payload = to_json(&planned_session, "browser session")?;
            if tx.execute(
                "UPDATE jobs_applications SET state = $3, application_json = $4,
                        updated_at_ms = $5
                  WHERE account_id = $1 AND id = $2 AND state = $6 AND application_json = $7",
                &[
                    &account_id,
                    &application_id,
                    &input.status,
                    &application_payload,
                    &now,
                    &application_state,
                    &application_raw,
                ],
            )? != 1
                || tx.execute(
                    "UPDATE jobs_browser_sessions SET status = $3, session_json = $4,
                            updated_at_ms = $5
                      WHERE account_id = $1 AND id = $2 AND runner = 'local'
                        AND status = $6 AND session_json = $7",
                    &[
                        &account_id,
                        &run_id,
                        &input.status,
                        &session_payload,
                        &now,
                        &session_status,
                        &session_raw,
                    ],
                )? != 1
                || tx.execute(
                    "UPDATE jobs_local_run_tickets SET status = $3, updated_at_ms = $4
                      WHERE id = $1 AND ticket_hash = $2 AND status = $5
                        AND expires_at_ms > $4",
                    &[&run_id, &ticket_hash, &input.status, &now, &ticket_status],
                )? != 1
            {
                anyhow::bail!("local run result changed before commit")
            }
            if input.status == "failed" {
                if reservation_status != "released"
                    && tx.execute(
                        "UPDATE jobs_attempt_reservations SET status = 'released', updated_at_ms = $3
                          WHERE account_id = $1 AND application_id = $2 AND status = $4",
                        &[&account_id, &application_id, &now, &reservation_status],
                    )? != 1
                {
                    anyhow::bail!("application attempt reservation changed")
                }
                tx.execute(
                    "UPDATE jobs_submission_evidence_capacity
                        SET state = 'released', updated_at_ms = $4, completed_at_ms = $4
                      WHERE account_id = $1 AND application_id = $2 AND run_id = $3
                        AND state = 'active'",
                    &[&account_id, &application_id, &run_id, &now],
                )?;
            }
            if let Some(intervention) = intervention.filter(|_| existing_intervention.is_none()) {
                let payload = to_json(&intervention, "intervention")?;
                tx.execute(
                    "INSERT INTO jobs_interventions (
                        id, account_id, application_id, kind, status, intervention_json,
                        created_at_ms, resolved_at_ms
                     ) VALUES ($1, $2, $3, $4, $5, $6, $7, NULL)",
                    &[
                        &intervention.id,
                        &account_id,
                        &intervention.application_id,
                        &intervention.kind,
                        &intervention.status,
                        &payload,
                        &intervention.created_at_ms,
                    ],
                )?;
            }
            tx.commit()?;
            Ok(planned_application)
        }
    })
}

#[cfg(test)]
#[test]
fn local_result_commit_prelocks_every_effect_row_before_one_database_clock() {
    let source = include_str!("local_runner.rs");
    let commit = source
        .split("pub fn commit_local_run_result(")
        .nth(1)
        .expect("atomic local result commit")
        .split("\n#[cfg(test)]")
        .next()
        .expect("bounded atomic local result commit");
    let postgres = commit
        .split("DbPool::Postgres(_) =>")
        .nth(1)
        .expect("PostgreSQL local result commit");
    let mut prior = 0;
    for operation in [
        "require_active_account_write_fence_postgres_tx",
        "FROM jobs_applications",
        "SELECT status, expires_at_ms FROM jobs_local_run_tickets",
        "FROM jobs_browser_sessions",
        "FROM jobs_attempt_reservations",
        "FROM jobs_submission_evidence_capacity",
        "pg_advisory_xact_lock",
        "local_run_claim_db_now_postgres",
        "prepare_local_result_commit",
        "UPDATE jobs_applications",
        "UPDATE jobs_browser_sessions",
        "UPDATE jobs_local_run_tickets",
    ] {
        let position = postgres
            .find(operation)
            .unwrap_or_else(|| panic!("missing local-result operation {operation}"));
        assert!(
            position >= prior,
            "local-result order inverted at {operation}"
        );
        prior = position;
    }
    assert_eq!(
        postgres.matches("local_run_claim_db_now_postgres").count(),
        1
    );
}

#[allow(clippy::too_many_arguments)]
pub fn finalize_local_side_effect_unknown(
    pool: &DbPool,
    account_id: &str,
    application_id: &str,
    run_id: &str,
    ticket_hash: &str,
    capacity: &crate::db::object_uploads::NewSubmissionEvidenceCapacity,
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
    if capacity.account_id != account_id
        || capacity.application_id != application_id
        || capacity.run_id != run_id
        || capacity.runner != "local"
    {
        anyhow::bail!("local submission evidence capacity does not match this application")
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
            match crate::db::account_data::account_write_fence_sqlite_tx(&tx, account_id)? {
                crate::db::account_data::AccountWriteFence::Active => {}
                crate::db::account_data::AccountWriteFence::DeletionRequested => {
                    anyhow::bail!("account deletion has fenced local reconciliation")
                }
                crate::db::account_data::AccountWriteFence::Missing => {
                    anyhow::bail!("application not found")
                }
            }
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
            let ticket: Option<(String, i64)> = tx
                .query_row(
                    "SELECT status, expires_at_ms FROM jobs_local_run_tickets
                      WHERE id = ?1 AND account_id = ?2 AND application_id = ?3
                        AND ticket_hash = ?4",
                    params![run_id, account_id, application_id, ticket_hash],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()?;
            let Some((ticket_status, ticket_expires_at_ms)) = ticket else {
                anyhow::bail!("local run ticket is not active")
            };
            if application.state == "side_effect_unknown" {
                let stored_session: Option<(String, String, String)> = tx
                    .query_row(
                        "SELECT session_json, runner, status FROM jobs_browser_sessions
                          WHERE account_id = ?1 AND id = ?2",
                        params![account_id, run_id],
                        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                    )
                    .optional()?;
                if ticket_status != "side_effect_unknown"
                    || application.receipt.pointer("/local_reconciliation/receipt")
                        != Some(&reconciliation_receipt)
                    || stored_session.is_none_or(|(raw, runner, status)| {
                        !local_unknown_terminal_session_matches(
                            raw,
                            &runner,
                            &status,
                            run_id,
                            application_id,
                            &terminal_session,
                        )
                        .unwrap_or(false)
                    })
                {
                    anyhow::bail!("submission outcome changed")
                }
                retain_local_unknown_capacity_sqlite_tx(
                    &tx,
                    &ticket_status,
                    ticket_expires_at_ms,
                    now,
                    capacity,
                )?;
                tx.commit()?;
                return Ok(application);
            }
            let prior_application_state = application.state.clone();
            validate_application_transition(&application.state, "side_effect_unknown")?;
            if !local_side_effect_unknown_transition_allowed(
                &ticket_status,
                ticket_expires_at_ms,
                now,
            ) {
                anyhow::bail!("local run ticket is not active")
            }
            retain_local_unknown_capacity_sqlite_tx(
                &tx,
                &ticket_status,
                ticket_expires_at_ms,
                now,
                capacity,
            )?;
            if tx.execute(
                "UPDATE jobs_local_run_tickets
                    SET status = 'side_effect_unknown', updated_at_ms = ?5
                  WHERE id = ?1 AND account_id = ?2 AND application_id = ?3
                    AND ticket_hash = ?4 AND status = ?6",
                params![
                    run_id,
                    account_id,
                    application_id,
                    ticket_hash,
                    now,
                    ticket_status,
                ],
            )? != 1
            {
                anyhow::bail!("local run ticket is not active")
            }
            if tx.execute(
                "UPDATE jobs_attempt_reservations
                    SET status = 'side_effect_unknown', updated_at_ms = ?3
                  WHERE account_id = ?1 AND application_id = ?2
                    AND status IN ('reserved', 'running')",
                params![account_id, application_id, now],
            )? != 1
            {
                anyhow::bail!("application attempt reservation is missing")
            }
            let prior_session_status = if ticket_status == "needs_input" {
                "needs_input"
            } else {
                "running"
            };
            if tx.execute(
                "UPDATE jobs_browser_sessions
                    SET status = 'needs_input', session_json = ?3, updated_at_ms = ?4
                  WHERE account_id = ?1 AND id = ?2 AND runner = 'local'
                    AND status = ?5",
                params![
                    account_id,
                    run_id,
                    session_payload,
                    now,
                    prior_session_status,
                ],
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
                  WHERE account_id = ?1 AND id = ?2 AND state = ?5",
                params![
                    account_id,
                    application_id,
                    application_payload,
                    now,
                    prior_application_state,
                ],
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
            match crate::db::account_data::account_write_fence_postgres_tx(&mut tx, account_id)? {
                crate::db::account_data::AccountWriteFence::Active => {}
                crate::db::account_data::AccountWriteFence::DeletionRequested => {
                    anyhow::bail!("account deletion has fenced local reconciliation")
                }
                crate::db::account_data::AccountWriteFence::Missing => {
                    anyhow::bail!("application not found")
                }
            }
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
            let ticket = tx
                .query_opt(
                    "SELECT status, expires_at_ms FROM jobs_local_run_tickets
                      WHERE id = $1 AND account_id = $2 AND application_id = $3
                        AND ticket_hash = $4 FOR UPDATE",
                    &[&run_id, &account_id, &application_id, &ticket_hash],
                )?
                .map(|row| (row.get::<_, String>(0), row.get::<_, i64>(1)));
            let Some((ticket_status, ticket_expires_at_ms)) = ticket else {
                anyhow::bail!("local run ticket is not active")
            };
            if tx
                .query_opt(
                    "SELECT status FROM jobs_attempt_reservations
                      WHERE account_id = $1 AND application_id = $2 FOR UPDATE",
                    &[&account_id, &application_id],
                )?
                .is_none()
            {
                anyhow::bail!("application attempt reservation is missing")
            }
            let stored_session = tx
                .query_opt(
                    "SELECT session_json, runner, status FROM jobs_browser_sessions
                      WHERE account_id = $1 AND id = $2 FOR UPDATE",
                    &[&account_id, &run_id],
                )?
                .map(|row| {
                    (
                        row.get::<_, String>(0),
                        row.get::<_, String>(1),
                        row.get::<_, String>(2),
                    )
                });
            if tx
                .query_opt(
                    "SELECT run_id FROM jobs_submission_evidence_capacity
                      WHERE account_id = $1 AND application_id = $2 AND run_id = $3
                        AND runner = 'local' FOR UPDATE",
                    &[&account_id, &application_id, &run_id],
                )?
                .is_none()
            {
                anyhow::bail!("local submission evidence capacity is missing")
            }
            let now = local_run_claim_db_now_postgres(&mut tx)?;
            let mut terminal_session = session.clone();
            terminal_session.status = "needs_input".to_string();
            terminal_session.current_step = "Submission outcome needs reconciliation".to_string();
            terminal_session.updated_at_ms = now;
            let session_payload = to_json(&terminal_session, "browser session")?;
            if application.state == "side_effect_unknown" {
                if ticket_status != "side_effect_unknown"
                    || application.receipt.pointer("/local_reconciliation/receipt")
                        != Some(&reconciliation_receipt)
                    || stored_session.is_none_or(|(raw, runner, status)| {
                        !local_unknown_terminal_session_matches(
                            raw,
                            &runner,
                            &status,
                            run_id,
                            application_id,
                            &terminal_session,
                        )
                        .unwrap_or(false)
                    })
                {
                    anyhow::bail!("submission outcome changed")
                }
                retain_local_unknown_capacity_postgres_tx(
                    &mut tx,
                    &ticket_status,
                    ticket_expires_at_ms,
                    now,
                    capacity,
                )?;
                tx.commit()?;
                return Ok(application);
            }
            let prior_application_state = application.state.clone();
            validate_application_transition(&application.state, "side_effect_unknown")?;
            if !local_side_effect_unknown_transition_allowed(
                &ticket_status,
                ticket_expires_at_ms,
                now,
            ) {
                anyhow::bail!("local run ticket is not active")
            }
            retain_local_unknown_capacity_postgres_tx(
                &mut tx,
                &ticket_status,
                ticket_expires_at_ms,
                now,
                capacity,
            )?;
            if tx.execute(
                "UPDATE jobs_local_run_tickets
                    SET status = 'side_effect_unknown', updated_at_ms = $5
                  WHERE id = $1 AND account_id = $2 AND application_id = $3
                    AND ticket_hash = $4 AND status = $6",
                &[
                    &run_id,
                    &account_id,
                    &application_id,
                    &ticket_hash,
                    &now,
                    &ticket_status,
                ],
            )? != 1
            {
                anyhow::bail!("local run ticket is not active")
            }
            if tx.execute(
                "UPDATE jobs_attempt_reservations
                    SET status = 'side_effect_unknown', updated_at_ms = $3
                  WHERE account_id = $1 AND application_id = $2
                    AND status IN ('reserved', 'running')",
                &[&account_id, &application_id, &now],
            )? != 1
            {
                anyhow::bail!("application attempt reservation is missing")
            }
            let prior_session_status = if ticket_status == "needs_input" {
                "needs_input"
            } else {
                "running"
            };
            if tx.execute(
                "UPDATE jobs_browser_sessions
                    SET status = 'needs_input', session_json = $3, updated_at_ms = $4
                  WHERE account_id = $1 AND id = $2 AND runner = 'local'
                    AND status = $5",
                &[
                    &account_id,
                    &run_id,
                    &session_payload,
                    &now,
                    &prior_session_status,
                ],
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
                  WHERE account_id = $1 AND id = $2 AND state = $5",
                &[
                    &account_id,
                    &application_id,
                    &application_payload,
                    &now,
                    &prior_application_state,
                ],
            )? != 1
            {
                anyhow::bail!("application not found")
            }
            tx.commit()?;
            Ok(application)
        }
    })
}

fn local_side_effect_unknown_transition_allowed(
    ticket_status: &str,
    ticket_expires_at_ms: i64,
    now: i64,
) -> bool {
    (matches!(ticket_status, "claimed" | "needs_input") && ticket_expires_at_ms > now)
        || (ticket_status == "click_started"
            && ticket_expires_at_ms.saturating_add(SUBMISSION_RECONCILIATION_GRACE_MS) > now)
}

fn retain_local_unknown_capacity_sqlite_tx(
    tx: &rusqlite::Transaction<'_>,
    ticket_status: &str,
    ticket_expires_at_ms: i64,
    now: i64,
    capacity: &crate::db::object_uploads::NewSubmissionEvidenceCapacity,
) -> Result<()> {
    if matches!(ticket_status, "claimed" | "needs_input") {
        crate::db::object_uploads::reserve_submission_evidence_capacity_sqlite_tx(tx, capacity)?;
    }
    let retain_until = ticket_expires_at_ms.saturating_add(SUBMISSION_RECONCILIATION_GRACE_MS);
    if !crate::db::object_uploads::extend_exact_submission_evidence_capacity_expiry_sqlite_tx(
        tx,
        capacity,
        retain_until,
        now,
    )? {
        anyhow::bail!("local submission evidence capacity is missing")
    }
    Ok(())
}

fn retain_local_unknown_capacity_postgres_tx(
    tx: &mut postgres::Transaction<'_>,
    ticket_status: &str,
    ticket_expires_at_ms: i64,
    now: i64,
    capacity: &crate::db::object_uploads::NewSubmissionEvidenceCapacity,
) -> Result<()> {
    if matches!(ticket_status, "claimed" | "needs_input") {
        crate::db::object_uploads::reserve_submission_evidence_capacity_postgres_tx(tx, capacity)?;
    }
    let retain_until = ticket_expires_at_ms.saturating_add(SUBMISSION_RECONCILIATION_GRACE_MS);
    if !crate::db::object_uploads::extend_exact_submission_evidence_capacity_expiry_postgres_tx(
        tx,
        capacity,
        retain_until,
        now,
    )? {
        anyhow::bail!("local submission evidence capacity is missing")
    }
    Ok(())
}

fn local_unknown_terminal_session_matches(
    raw: String,
    runner: &str,
    status: &str,
    run_id: &str,
    application_id: &str,
    expected: &BrowserSession,
) -> Result<bool> {
    let stored: BrowserSession = parse_json(raw, "browser session")?;
    Ok(runner == "local"
        && status == "needs_input"
        && stored.id == run_id
        && stored.runner == runner
        && stored.status == status
        && stored.application_id.as_deref() == Some(application_id)
        && stored.current_company == expected.current_company
        && stored.current_step == expected.current_step
        && stored.takeover_url == expected.takeover_url
        && stored.created_at_ms == expected.created_at_ms)
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
            crate::db::object_uploads::require_active_account_write_fence_sqlite_tx(
                &tx, account_id,
            )?;
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
            crate::db::object_uploads::require_active_account_write_fence_postgres_tx(
                &mut tx, account_id,
            )?;
            let ticket_expires_at = tx
                .query_opt(
                    "SELECT status, expires_at_ms FROM jobs_local_run_tickets
                      WHERE id = $1 AND account_id = $2 AND application_id = $3
                      FOR UPDATE",
                    &[&run_id, &account_id, &application_id],
                )?
                .map(|row| (row.get::<_, String>(0), row.get::<_, i64>(1)));
            let Some((ticket_status, ticket_expires_at)) = ticket_expires_at else {
                tx.commit()?;
                return Ok(None);
            };
            let intervention_status = tx
                .query_opt(
                    "SELECT status FROM jobs_interventions
                      WHERE id = $1 AND account_id = $2 AND application_id = $3 FOR UPDATE",
                    &[&intervention_id, &account_id, &application_id],
                )?
                .map(|row| row.get::<_, String>(0));
            let existing = tx.query_opt(
                "SELECT run_id, status, expires_at_ms
                   FROM jobs_local_run_resume_actions WHERE intervention_id = $1 FOR UPDATE",
                &[&intervention_id],
            )?;
            let now = local_run_claim_db_now_postgres(&mut tx)?;
            if ticket_status != "needs_input"
                || ticket_expires_at <= now
                || intervention_status.as_deref() != Some("approved")
            {
                tx.commit()?;
                return Ok(None);
            }
            if let Some(row) = existing {
                let existing_run_id: String = row.get(0);
                let status: String = row.get(1);
                let expires_at_ms: i64 = row.get(2);
                tx.commit()?;
                return Ok((status == "approved"
                    && existing_run_id == run_id
                    && expires_at_ms > now)
                    .then(|| LocalRunResumeAction {
                        run_id: run_id.to_string(),
                        intervention_id: intervention_id.to_string(),
                        action: "approve_submission".to_string(),
                        expires_at_ms,
                        account_id: account_id.to_string(),
                        application_id: application_id.to_string(),
                        first_consumption: false,
                    }));
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
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let now = now_ms();
            let account_id: Option<String> = tx
                .query_row(
                    "SELECT account_id FROM jobs_local_run_tickets
                      WHERE id = ?1 AND ticket_hash = ?2",
                    params![run_id, ticket_hash],
                    |row| row.get(0),
                )
                .optional()?;
            let Some(account_id) = account_id else {
                tx.commit()?;
                return Ok(None);
            };
            crate::db::object_uploads::require_active_account_write_fence_sqlite_tx(
                &tx,
                &account_id,
            )?;
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
            let account_id = tx
                .query_opt(
                    "SELECT account_id FROM jobs_local_run_tickets
                      WHERE id = $1 AND ticket_hash = $2",
                    &[&run_id, &ticket_hash],
                )?
                .map(|row| row.get::<_, String>(0));
            let Some(account_id) = account_id else {
                tx.commit()?;
                return Ok(None);
            };
            crate::db::object_uploads::require_active_account_write_fence_postgres_tx(
                &mut tx,
                &account_id,
            )?;
            let discovered_action_id = tx
                .query_opt(
                    "SELECT id FROM jobs_local_run_resume_actions
                      WHERE run_id = $1 ORDER BY created_at_ms DESC LIMIT 1",
                    &[&run_id],
                )?
                .map(|row| row.get::<_, String>(0));
            let Some(discovered_action_id) = discovered_action_id else {
                tx.commit()?;
                return Ok(None);
            };
            let action_row = tx.query_opt(
                "SELECT a.id, a.account_id, a.application_id, a.intervention_id,
                        a.action, a.expires_at_ms, a.status
                  FROM jobs_local_run_resume_actions a WHERE a.id = $1 FOR UPDATE",
                &[&discovered_action_id],
            )?;
            let Some(row) = action_row else {
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
            let ticket = tx.query_opt(
                "SELECT account_id, application_id, status, expires_at_ms
                   FROM jobs_local_run_tickets
                  WHERE id = $1 AND ticket_hash = $2 FOR UPDATE",
                &[&run_id, &ticket_hash],
            )?;
            let intervention_status = tx
                .query_opt(
                    "SELECT status FROM jobs_interventions
                      WHERE id = $1 AND account_id = $2 AND application_id = $3 FOR UPDATE",
                    &[&intervention_id, &account_id, &application_id],
                )?
                .map(|row| row.get::<_, String>(0));
            let now = local_run_claim_db_now_postgres(&mut tx)?;
            let ticket_valid = ticket.is_some_and(|row| {
                row.get::<_, String>(0) == account_id
                    && row.get::<_, String>(1) == application_id
                    && matches!(row.get::<_, String>(2).as_str(), "needs_input" | "claimed")
                    && row.get::<_, i64>(3) > now
            });
            if !ticket_valid
                || !matches!(action_status.as_str(), "approved" | "consumed")
                || expires_at_ms <= now
                || intervention_status.as_deref() != Some("approved")
            {
                tx.commit()?;
                return Ok(None);
            }
            let action_changed = tx.execute(
                "UPDATE jobs_local_run_resume_actions
                    SET status = 'consumed', consumed_at_ms = COALESCE(consumed_at_ms, $2)
                  WHERE id = $1 AND status IN ('approved', 'consumed') AND expires_at_ms > $2",
                &[&id, &now],
            )?;
            let ticket_changed = tx.execute(
                "UPDATE jobs_local_run_tickets
                    SET updated_at_ms = CASE WHEN status = 'needs_input' THEN $3 ELSE updated_at_ms END,
                        status = 'claimed'
                  WHERE id = $1 AND ticket_hash = $2
                    AND status IN ('needs_input', 'claimed') AND expires_at_ms > $3",
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
