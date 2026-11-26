const LOCAL_PUBLICATION_TTL_MS: i64 = 24 * 60 * 60 * 1_000;

fn local_publication_credentials_valid(hash: &str, secret: &str) -> bool {
    secret.len() == 64
        && secret.bytes().all(|byte| byte.is_ascii_hexdigit())
        && valid_approved_submission_sha256(hash)
        && constant_time_equal(hash, &hex::encode(Sha256::digest(secret.as_bytes())))
}

fn local_publication_request_valid(request: &PublishLocalApplicationRun) -> bool {
    valid_approved_submission_identifier(&request.account_id)
        && (3..=160).contains(&request.run_id.len())
        && request
            .run_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
        && valid_approved_submission_identifier(&request.expected_application.id)
        && local_publication_credentials_valid(&request.ticket_hash, &request.ticket_secret)
        && request.expires_at_ms > 0
}

fn local_publication_session_matches(
    expected: &BrowserSession,
    current: &BrowserSession,
) -> Result<bool> {
    let mut expected = expected.clone();
    // Only the first publication supplies these two server-owned timestamps.
    if expected.created_at_ms == 0 {
        expected.created_at_ms = current.created_at_ms;
    }
    if expected.updated_at_ms == 0 {
        expected.updated_at_ms = current.updated_at_ms;
    }
    Ok(serde_json::to_value(expected)? == serde_json::to_value(current)?)
}

enum LocalPublicationDecision {
    Insert(PublishedLocalApplicationRun),
    Replay(PublishedLocalApplicationRun),
}

#[allow(clippy::too_many_arguments)]
fn prepare_local_publication(
    request: &PublishLocalApplicationRun,
    application: JobApplication,
    posting: &JobPosting,
    session: Option<BrowserSession>,
    ticket: Option<LocalRunTicket>,
    local_entitled: bool,
    reservation: Option<(String, String)>,
    now: i64,
) -> Result<LocalPublicationDecision> {
    let conflict = || anyhow::Error::new(LocalRunPublicationError::Conflict);
    if !local_entitled || application.submitted_at_ms.is_some() {
        return Err(conflict());
    }
    let Some((runner, reservation_status)) = reservation else {
        return Err(conflict());
    };
    if runner != "local" || !matches!(reservation_status.as_str(), "reserved" | "running") {
        return Err(conflict());
    }
    if serde_json::to_value(&request.expected_application)? != serde_json::to_value(&application)? {
        return Err(conflict());
    }
    validate_local_publication_transport(
        &request.account_id,
        &application,
        posting,
        &request.run_id,
        &request.browser_session,
        &request.workflow_input,
    )?;
    if let Some(ticket) = ticket {
        let session = session.ok_or_else(conflict)?;
        if ticket.account_id != request.account_id
            || ticket.application_id != application.id
            || ticket.id != request.run_id
            || application.run_id.as_deref() != Some(request.run_id.as_str())
            || !matches!(
                application.state.as_str(),
                "queued" | "running" | "needs_input"
            )
            || !matches!(ticket.status.as_str(), "queued" | "claimed" | "needs_input")
            || !matches!(
                session.status.as_str(),
                "queued" | "running" | "needs_input"
            )
            || ticket.expires_at_ms <= now
            || ticket.payload != request.workflow_input
            || !local_publication_credentials_valid(&ticket.ticket_hash, &ticket.ticket_secret)
            || !local_publication_session_matches(&request.browser_session, &session)?
        {
            return Err(conflict());
        }
        // The public queue route reloads the current application on each request.
        // A stale internal CAS, including the pre-publication snapshot, is never
        // silently upgraded to this revision just to make a retry succeed.
        return Ok(LocalPublicationDecision::Replay(
            PublishedLocalApplicationRun {
                application,
                browser_session: session,
                ticket,
            },
        ));
    }
    if session.is_some()
        || application.state != "queued"
        || application.run_id.is_some()
        || reservation_status != "reserved"
        || serde_json::to_value(&request.expected_application)?
            != serde_json::to_value(&application)?
        || request.browser_session.status != "queued"
        || request.browser_session.takeover_url.is_some()
        || request.browser_session.created_at_ms != 0
        || request.browser_session.updated_at_ms != 0
        || request.expires_at_ms <= now
        || request.expires_at_ms > now.saturating_add(LOCAL_PUBLICATION_TTL_MS)
    {
        return Err(conflict());
    }
    let mut application = application;
    application.run_id = Some(request.run_id.clone());
    application.updated_at_ms = now;
    let mut session = request.browser_session.clone();
    session.created_at_ms = now;
    session.updated_at_ms = now;
    let ticket = LocalRunTicket {
        id: request.run_id.clone(),
        account_id: request.account_id.clone(),
        application_id: application.id.clone(),
        ticket_hash: request.ticket_hash.clone(),
        ticket_secret: request.ticket_secret.clone(),
        payload: request.workflow_input.clone(),
        status: "queued".into(),
        expires_at_ms: request.expires_at_ms,
        created_at_ms: now,
        updated_at_ms: now,
    };
    Ok(LocalPublicationDecision::Insert(
        PublishedLocalApplicationRun {
            application,
            browser_session: session,
            ticket,
        },
    ))
}

fn local_publication_application(
    application_id: &str,
    job_id: &str,
    raw: String,
    state: &str,
    updated_at_ms: i64,
    submitted_at_ms: Option<i64>,
) -> Result<JobApplication> {
    let application =
        parse_application_json(raw, application_id, job_id, "local publication application")?;
    if application.state != state
        || application.updated_at_ms != updated_at_ms
        || application.submitted_at_ms != submitted_at_ms
    {
        return Err(LocalRunPublicationError::Conflict.into());
    }
    Ok(application)
}

/// Publish the application/run, local browser session and encrypted ticket as
/// one exact-snapshot operation. Reservation/metering presteps are not part of
/// this transaction. This grants no exemption from claim or first-effect gates.
pub fn publish_local_application_run(
    pool: &DbPool,
    request: &PublishLocalApplicationRun,
) -> Result<PublishedLocalApplicationRun> {
    if !local_publication_request_valid(request) {
        return Err(LocalRunPublicationError::InvalidRequest.into());
    }
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            crate::db::object_uploads::require_active_account_write_fence_sqlite_tx(
                &tx,
                &request.account_id,
            )?;
            if !crate::db::jobs_beta_access::public_beta_effect_authorized_sqlite_tx(
                &tx,
                &request.account_id,
            )? {
                return Err(LocalRunPublicationError::Conflict.into());
            }
            require_no_workflow_cleanup_sqlite_tx(&tx, &request.account_id)?;
            let row: Option<(String, String, String, i64, Option<i64>)> = tx.query_row(
                "SELECT job_id,application_json,state,updated_at_ms,submitted_at_ms FROM jobs_applications
                 WHERE account_id=?1 AND id=?2", params![request.account_id, request.expected_application.id],
                |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?)),
            ).optional()?;
            let (job_id, raw, state, updated, submitted) =
                row.ok_or(LocalRunPublicationError::Missing)?;
            let application = local_publication_application(
                &request.expected_application.id,
                &job_id,
                raw.clone(),
                &state,
                updated,
                submitted,
            )?;
            let posting_raw: String = tx.query_row(
                "SELECT posting_json FROM jobs_postings WHERE account_id=?1 AND id=?2",
                params![request.account_id, job_id],
                |r| r.get(0),
            )?;
            let mut stored_posting: JobPosting =
                parse_json(posting_raw, "local publication posting")?;
            stored_posting.id = job_id;
            let local_entitled: bool = tx
                .query_row(
                    "SELECT local_browser<>0 FROM jobs_entitlements WHERE account_id=?1",
                    params![request.account_id],
                    |r| r.get(0),
                )
                .optional()?
                .unwrap_or(false);
            let reservation = tx.query_row("SELECT runner,status FROM jobs_attempt_reservations WHERE account_id=?1 AND application_id=?2",
                params![request.account_id,application.id],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?))).optional()?;
            let ticket = tx.query_row("SELECT id,account_id,application_id,ticket_hash,ticket_secret,payload_json,status,expires_at_ms,created_at_ms,updated_at_ms
                FROM jobs_local_run_tickets WHERE account_id=?1 AND id=?2",params![request.account_id,request.run_id],local_run_ticket_from_sqlite_row).optional()?;
            let other_local: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM jobs_local_run_tickets WHERE account_id=?1
                 AND application_id=?2 AND id<>?3 AND status<>'failed')",
                params![request.account_id, application.id, request.run_id],
                |r| r.get(0),
            )?;
            if other_local {
                return Err(LocalRunPublicationError::Conflict.into());
            }
            let session = tx.query_row("SELECT session_json,runner,status FROM jobs_browser_sessions WHERE account_id=?1 AND id=?2",
                params![request.account_id,request.run_id],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?))).optional()?
                .map(|(raw,runner,status)| local_publication_session(raw,&runner,&status)).transpose()?;
            let conflicting_cloud: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM jobs_execution_leases
                WHERE account_id=?1 AND application_id=?2 AND phase NOT IN ('released','failed'))",
                params![request.account_id, application.id],
                |r| r.get(0),
            )?;
            if conflicting_cloud {
                return Err(LocalRunPublicationError::Conflict.into());
            }
            let now = original_source_db_now_sqlite(&tx)?;
            let composed = resolve_composed_job_integrity_projection_sqlite_tx_at_ms(
                &tx,
                &request.account_id,
                &stored_posting,
                now,
            )?;
            let authority = current_application_approval_authority_from_composed(
                application.clone(),
                stored_posting,
                composed,
            )
            .map_err(|_| LocalRunPublicationError::Conflict)?;
            let decision = prepare_local_publication(
                request,
                application,
                &authority.posting,
                session,
                ticket,
                local_entitled,
                reservation,
                now,
            )?;
            let value = match &decision {
                LocalPublicationDecision::Insert(v) | LocalPublicationDecision::Replay(v) => v,
            };
            // Prepare all encrypted values before the last current-time authority check.
            let app_json = to_json(&value.application, "job application")?;
            let session_json = to_json(&value.browser_session, "browser session")?;
            let ticket_secret = encrypt_payload(&value.ticket.ticket_secret)?;
            let ticket_json = to_json(&value.ticket.payload, "Jobs local browser packet")?;
            let final_now = original_source_db_now_sqlite(&tx)?;
            require_local_publication_authority_sqlite(
                &tx,
                &request.account_id,
                &value.application,
                final_now,
            )?;
            if value.ticket.expires_at_ms <= final_now {
                return Err(LocalRunPublicationError::Conflict.into());
            }
            if let LocalPublicationDecision::Insert(value) = &decision {
                if tx.execute("UPDATE jobs_applications SET application_json=?3,updated_at_ms=?4
                    WHERE account_id=?1 AND id=?2 AND application_json=?5 AND state=?6 AND updated_at_ms=?7 AND submitted_at_ms IS NULL",
                    params![request.account_id,value.application.id,app_json,value.application.updated_at_ms,raw,state,updated])? != 1 {
                    return Err(LocalRunPublicationError::Conflict.into());
                }
                if tx.execute("INSERT INTO jobs_browser_sessions(id,account_id,runner,status,session_json,created_at_ms,updated_at_ms)
                    VALUES(?1,?2,'local','queued',?3,?4,?4) ON CONFLICT(id) DO NOTHING",
                    params![value.browser_session.id,request.account_id,session_json,value.browser_session.created_at_ms])? != 1 {
                    return Err(LocalRunPublicationError::Conflict.into());
                }
                if tx.execute("INSERT INTO jobs_local_run_tickets(id,account_id,application_id,ticket_hash,ticket_secret,payload_json,status,expires_at_ms,created_at_ms,updated_at_ms)
                    VALUES(?1,?2,?3,?4,?5,?6,'queued',?7,?8,?8) ON CONFLICT(id) DO NOTHING",
                    params![value.ticket.id,request.account_id,value.application.id,value.ticket.ticket_hash,ticket_secret,ticket_json,value.ticket.expires_at_ms,value.ticket.created_at_ms])? != 1 {
                    return Err(LocalRunPublicationError::Conflict.into());
                }
            }
            tx.commit()?;
            Ok(match decision {
                LocalPublicationDecision::Insert(v) | LocalPublicationDecision::Replay(v) => v,
            })
        }
        DbPool::Postgres(_) => publish_local_application_run_postgres(pool, request),
    })
}

fn local_publication_session(raw: String, runner: &str, status: &str) -> Result<BrowserSession> {
    let session: BrowserSession = parse_json(raw, "local publication session")?;
    if session.runner != runner || session.status != status {
        return Err(LocalRunPublicationError::Conflict.into());
    }
    Ok(session)
}

fn require_local_publication_authority_sqlite(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    application: &JobApplication,
    now: i64,
) -> Result<()> {
    let current = resolve_current_execution_authority_sqlite_after_prelock_at_ms(
        tx,
        account_id,
        application,
        ExecutionAuthorityRunner::Local,
        now,
    )?;
    if !current.authorized {
        return Err(LocalRunPublicationError::Conflict.into());
    }
    let domain = current
        .employer_domain
        .ok_or(LocalRunPublicationError::Conflict)?;
    let mut context = operational_hold_context_for_application_sqlite_tx_after_authority(
        tx,
        account_id,
        &application.id,
        &domain,
        Some("local"),
        None,
        None,
    )?;
    add_candidate_queue_hold_scopes(&mut context, application)?;
    require_operational_capability_sqlite_tx(tx, OperationalCapability::ApplicationQueue, &context)
        .map_err(anyhow::Error::new)
}

fn publish_local_application_run_postgres(
    pool: &DbPool,
    request: &PublishLocalApplicationRun,
) -> Result<PublishedLocalApplicationRun> {
    let mut conn = pool.get_pg()?;
    let mut tx = conn.transaction()?;
    lock_operational_hold_shared_postgres_tx(&mut tx).map_err(anyhow::Error::new)?;
    lock_managed_cloud_release_registry_shared_postgres_tx(&mut tx).map_err(anyhow::Error::new)?;
    lock_postgres_ats_certification(&mut tx)?;
    if !crate::db::jobs_beta_access::public_beta_effect_authorized_postgres_tx(
        &mut tx,
        &request.account_id,
    )? {
        return Err(LocalRunPublicationError::Conflict.into());
    }
    lock_discovery_account_shared_postgres(&mut tx, &request.account_id)?;
    crate::db::object_uploads::require_active_account_write_fence_postgres_tx(
        &mut tx,
        &request.account_id,
    )?;
    require_no_workflow_cleanup_postgres_tx(&mut tx, &request.account_id)?;
    let hint = tx.query_opt("SELECT job_id,application_json,state,updated_at_ms,submitted_at_ms FROM jobs_applications
        WHERE account_id=$1 AND id=$2", &[&request.account_id,&request.expected_application.id])?.ok_or(LocalRunPublicationError::Missing)?;
    let hint_raw: String = hint.get(1);
    let hint_app = local_publication_application(
        &request.expected_application.id,
        hint.get::<_, String>(0).as_str(),
        hint_raw.clone(),
        hint.get::<_, String>(2).as_str(),
        hint.get(3),
        hint.get(4),
    )?;
    if !lock_current_execution_authority_postgres_after_prelock(
        &mut tx,
        &request.account_id,
        &hint_app,
    )? {
        return Err(LocalRunPublicationError::Conflict.into());
    }
    let row=tx.query_opt("SELECT job_id,application_json,state,updated_at_ms,submitted_at_ms FROM jobs_applications
        WHERE account_id=$1 AND id=$2 FOR UPDATE", &[&request.account_id,&request.expected_application.id])?.ok_or(LocalRunPublicationError::Missing)?;
    let job_id: String = row.get(0);
    let raw: String = row.get(1);
    let state: String = row.get(2);
    let updated: i64 = row.get(3);
    if raw != hint_raw
        || job_id != hint_app.job_id
        || state != hint_app.state
        || updated != hint_app.updated_at_ms
    {
        return Err(LocalRunPublicationError::Conflict.into());
    }
    let application = local_publication_application(
        &request.expected_application.id,
        &job_id,
        raw.clone(),
        &state,
        updated,
        row.get(4),
    )?;
    let local_entitled = tx
        .query_opt(
            "SELECT local_browser FROM jobs_entitlements WHERE account_id=$1 FOR SHARE",
            &[&request.account_id],
        )?
        .is_some_and(|r| r.get::<_, i32>(0) != 0);
    let reservation=tx.query_opt("SELECT runner,status FROM jobs_attempt_reservations WHERE account_id=$1 AND application_id=$2 FOR UPDATE",
        &[&request.account_id,&application.id])?.map(|r|(r.get::<_,String>(0),r.get::<_,String>(1)));
    // Account-row admission serializes insertions; lock the complete existing
    // application set, not just the current run, before deciding it is safe.
    let local_tickets = tx.query(
        "SELECT id,status FROM jobs_local_run_tickets WHERE account_id=$1
         AND application_id=$2 ORDER BY id FOR UPDATE",
        &[&request.account_id, &application.id],
    )?;
    if local_tickets
        .iter()
        .any(|row| row.get::<_, String>(0) != request.run_id && row.get::<_, String>(1) != "failed")
    {
        return Err(LocalRunPublicationError::Conflict.into());
    }
    let ticket=tx.query_opt("SELECT id,account_id,application_id,ticket_hash,ticket_secret,payload_json,status,expires_at_ms,created_at_ms,updated_at_ms
        FROM jobs_local_run_tickets WHERE account_id=$1 AND id=$2 FOR UPDATE",&[&request.account_id,&request.run_id])?.map(local_run_ticket_from_pg_row).transpose()?;
    let session=tx.query_opt("SELECT session_json,runner,status FROM jobs_browser_sessions WHERE account_id=$1 AND id=$2 FOR UPDATE",
        &[&request.account_id,&request.run_id])?.map(|r|local_publication_session(r.get(0),r.get::<_,String>(1).as_str(),r.get::<_,String>(2).as_str())).transpose()?;
    let conflicting_cloud:bool=tx.query_one("SELECT EXISTS(SELECT 1 FROM jobs_execution_leases WHERE account_id=$1 AND application_id=$2 AND phase NOT IN ('released','failed'))",
        &[&request.account_id,&application.id])?.get(0);
    if conflicting_cloud {
        return Err(LocalRunPublicationError::Conflict.into());
    }
    let posting_raw: String = tx
        .query_one(
            "SELECT posting_json FROM jobs_postings WHERE account_id=$1 AND id=$2 FOR SHARE",
            &[&request.account_id, &job_id],
        )?
        .get(0);
    let mut posting: JobPosting = parse_json(posting_raw, "local publication posting")?;
    posting.id = job_id;
    let now = original_source_db_now_postgres(&mut tx)?;
    let composed = resolve_composed_job_integrity_projection_postgres_tx_after_prelock_at_ms(
        &mut tx,
        &request.account_id,
        &posting,
        now,
    )?;
    let authority = current_application_approval_authority_from_composed(
        application.clone(),
        posting,
        composed,
    )
    .map_err(|_| LocalRunPublicationError::Conflict)?;
    let decision = prepare_local_publication(
        request,
        application,
        &authority.posting,
        session,
        ticket,
        local_entitled,
        reservation,
        now,
    )?;
    let value = match &decision {
        LocalPublicationDecision::Insert(v) | LocalPublicationDecision::Replay(v) => v,
    };
    let app_json = to_json(&value.application, "job application")?;
    let session_json = to_json(&value.browser_session, "browser session")?;
    let ticket_secret = encrypt_payload(&value.ticket.ticket_secret)?;
    let ticket_json = to_json(&value.ticket.payload, "Jobs local browser packet")?;
    let final_now = original_source_db_now_postgres(&mut tx)?;
    let current = resolve_current_execution_authority_postgres_after_prelock_at_ms(
        &mut tx,
        &request.account_id,
        &value.application,
        ExecutionAuthorityRunner::Local,
        final_now,
    )?;
    if !current.authorized || value.ticket.expires_at_ms <= final_now {
        return Err(LocalRunPublicationError::Conflict.into());
    }
    let domain = current
        .employer_domain
        .ok_or(LocalRunPublicationError::Conflict)?;
    let mut context = operational_hold_context_for_application_postgres_tx_after_authority_prelock(
        &mut tx,
        &request.account_id,
        &value.application.id,
        &domain,
        Some("local"),
        None,
        None,
    )?;
    add_candidate_queue_hold_scopes(&mut context, &value.application)?;
    require_operational_capability_postgres_tx_after_authority_prelock(
        &mut tx,
        OperationalCapability::ApplicationQueue,
        &context,
    )
    .map_err(anyhow::Error::new)?;
    if let LocalPublicationDecision::Insert(value) = &decision {
        if tx.execute("UPDATE jobs_applications SET application_json=$3,updated_at_ms=$4 WHERE account_id=$1 AND id=$2
            AND application_json=$5 AND state=$6 AND updated_at_ms=$7 AND submitted_at_ms IS NULL",
            &[&request.account_id,&value.application.id,&app_json,&value.application.updated_at_ms,&raw,&state,&updated])?!=1 {return Err(LocalRunPublicationError::Conflict.into());}
        if tx.execute("INSERT INTO jobs_browser_sessions(id,account_id,runner,status,session_json,created_at_ms,updated_at_ms)
            VALUES($1,$2,'local','queued',$3,$4,$4) ON CONFLICT(id) DO NOTHING",
            &[&value.browser_session.id,&request.account_id,&session_json,&value.browser_session.created_at_ms])?!=1 {return Err(LocalRunPublicationError::Conflict.into());}
        if tx.execute("INSERT INTO jobs_local_run_tickets(id,account_id,application_id,ticket_hash,ticket_secret,payload_json,status,expires_at_ms,created_at_ms,updated_at_ms)
            VALUES($1,$2,$3,$4,$5,$6,'queued',$7,$8,$8) ON CONFLICT(id) DO NOTHING",
            &[&value.ticket.id,&request.account_id,&value.application.id,&value.ticket.ticket_hash,&ticket_secret,&ticket_json,&value.ticket.expires_at_ms,&value.ticket.created_at_ms])?!=1 {return Err(LocalRunPublicationError::Conflict.into());}
    }
    tx.commit()?;
    Ok(match decision {
        LocalPublicationDecision::Insert(v) | LocalPublicationDecision::Replay(v) => v,
    })
}

#[cfg(test)]
mod local_run_publication_input_tests {
    use super::*;

    #[test]
    fn local_publication_input_matches_browser_identifier_and_secret_grammar() {
        let secret = "a".repeat(64);
        let mut request = PublishLocalApplicationRun {
            account_id: "account".into(),
            expected_application: serde_json::from_value(
                json!({"id":"application","job_id":"job"}),
            )
            .unwrap(),
            run_id: "valid-run_123".into(),
            workflow_input: Value::Null,
            browser_session: serde_json::from_value(json!({})).unwrap(),
            ticket_hash: hex::encode(Sha256::digest(secret.as_bytes())),
            ticket_secret: secret,
            expires_at_ms: 1,
        };
        // Syntactic admission only; these values carry no database authority.
        assert!(local_publication_request_valid(&request));
        for invalid in ["a", "ab", "run.id", "run:id", "rún-id", "run id"] {
            request.run_id = invalid.into();
            assert!(!local_publication_request_valid(&request));
        }
        request.run_id = "a".repeat(161);
        assert!(!local_publication_request_valid(&request));
        request.run_id = "valid-run".into();
        for invalid in ["a".repeat(63), "a".repeat(65), "z".repeat(64)] {
            request.ticket_hash = hex::encode(Sha256::digest(invalid.as_bytes()));
            request.ticket_secret = invalid;
            assert!(!local_publication_request_valid(&request));
        }
        request.ticket_secret = "A".repeat(64);
        request.ticket_hash = hex::encode(Sha256::digest(request.ticket_secret.as_bytes()));
        assert!(local_publication_request_valid(&request));
        request.ticket_hash = "f".repeat(64);
        assert!(!local_publication_request_valid(&request));
    }
}
