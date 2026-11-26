fn is_owner_not_submitted_resolution(application: &JobApplication) -> bool {
    application
        .receipt
        .pointer("/submission_reconciliation/outcome")
        .and_then(Value::as_str)
        == Some("not_submitted")
        && application
            .receipt
            .pointer("/submission_reconciliation/resolved_by")
            .and_then(Value::as_str)
            == Some("account_owner")
}

fn resolved_not_submitted_application(
    mut application: JobApplication,
    run_id: &str,
    now: i64,
) -> Result<JobApplication> {
    if !application.receipt.is_object() {
        application.receipt = json!({});
    }
    application
        .receipt
        .as_object_mut()
        .expect("receipt normalized above")
        .insert(
            "submission_reconciliation".to_string(),
            json!({
                "schema_version": 1,
                "outcome": "not_submitted",
                "resolved_by": "account_owner",
                "resolved_at_ms": now,
                "run_id": run_id,
            }),
        );
    application.state = "failed".to_string();
    application.updated_at_ms = now;
    application.submitted_at_ms = None;
    Ok(application)
}

fn resolved_not_submitted_session(
    raw: String,
    runner: &str,
    application_id: &str,
    run_id: &str,
    now: i64,
) -> Result<BrowserSession> {
    let mut session: BrowserSession = parse_json(raw, "browser session")?;
    if session.id != run_id
        || session.runner != runner
        || session.application_id.as_deref() != Some(application_id)
        || !matches!(runner, "local" | "cloud")
    {
        anyhow::bail!("browser session does not match this application")
    }
    session.status = "failed".to_string();
    session.current_step = "Confirmed not submitted".to_string();
    session.takeover_url = None;
    session.updated_at_ms = now;
    Ok(session)
}

/// Records the account owner's verified conclusion that an uncertain employer
/// action did not submit. The application row is the serialization point for
/// this decision and a late trusted runner receipt: whichever transaction
/// locks and resolves the row first wins.
pub fn reconcile_submission_not_submitted(
    pool: &DbPool,
    account_id: &str,
    application_id: &str,
) -> Result<Option<JobApplication>> {
    if account_id.trim().is_empty()
        || application_id.trim().is_empty()
        || account_id.len() > 240
        || application_id.len() > 240
    {
        anyhow::bail!("invalid submission reconciliation request")
    }
    let now = now_ms();
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            match crate::db::account_data::account_write_fence_sqlite_tx(&tx, account_id)? {
                crate::db::account_data::AccountWriteFence::Active => {}
                crate::db::account_data::AccountWriteFence::DeletionRequested => {
                    anyhow::bail!("account deletion has fenced submission reconciliation")
                }
                crate::db::account_data::AccountWriteFence::Missing => return Ok(None),
            }
            let row: Option<(String, String)> = tx
                .query_row(
                    "SELECT job_id, application_json FROM jobs_applications
                      WHERE account_id = ?1 AND id = ?2",
                    params![account_id, application_id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()?;
            let Some((job_id, raw)) = row else {
                return Ok(None);
            };
            let application =
                parse_application_json(raw, application_id, &job_id, "Jobs application")?;
            if application.state == "failed" && is_owner_not_submitted_resolution(&application) {
                tx.commit()?;
                return Ok(Some(application));
            }
            if application.state != "side_effect_unknown" {
                anyhow::bail!("submission outcome is no longer awaiting reconciliation")
            }
            let run_id = application
                .run_id
                .clone()
                .ok_or_else(|| anyhow::anyhow!("application browser run is missing"))?;
            let session_row: Option<(String, String)> = tx
                .query_row(
                    "SELECT session_json, runner FROM jobs_browser_sessions
                      WHERE account_id = ?1 AND id = ?2",
                    params![account_id, &run_id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()?;
            let Some((session_raw, runner)) = session_row else {
                anyhow::bail!("browser session not found")
            };
            let terminal_session = resolved_not_submitted_session(
                session_raw,
                &runner,
                application_id,
                &run_id,
                now,
            )?;
            let session_payload = to_json(&terminal_session, "browser session")?;

            let authority_updated = if runner == "local" {
                tx.execute(
                    "UPDATE jobs_local_run_tickets
                        SET status = 'failed', updated_at_ms = ?4
                      WHERE id = ?1 AND account_id = ?2 AND application_id = ?3
                        AND status = 'side_effect_unknown'",
                    params![&run_id, account_id, application_id, now],
                )?
            } else {
                tx.execute(
                    "UPDATE jobs_execution_leases
                        SET phase = 'released', updated_at_ms = ?4, finished_at_ms = ?4
                      WHERE run_id = ?1 AND account_id = ?2 AND application_id = ?3
                        AND phase = 'side_effect_unknown'",
                    params![&run_id, account_id, application_id, now],
                )?
            };
            if authority_updated != 1 {
                anyhow::bail!("matching execution authority changed")
            }
            if tx.execute(
                "UPDATE jobs_attempt_reservations
                    SET status = 'released', updated_at_ms = ?3
                  WHERE account_id = ?1 AND application_id = ?2
                    AND status = 'side_effect_unknown'",
                params![account_id, application_id, now],
            )? != 1
            {
                anyhow::bail!("application attempt reservation changed")
            }
            if tx.execute(
                "UPDATE jobs_browser_sessions
                    SET status = 'failed', session_json = ?3, updated_at_ms = ?4
                  WHERE account_id = ?1 AND id = ?2 AND status = 'needs_input'",
                params![account_id, &run_id, session_payload, now],
            )? != 1
            {
                anyhow::bail!("browser session changed")
            }
            let _ = crate::db::object_uploads::release_submission_evidence_capacity_sqlite_tx(
                &tx,
                account_id,
                application_id,
                &run_id,
                now,
            )?;
            validate_application_transition(&application.state, "failed")?;
            let application = resolved_not_submitted_application(application, &run_id, now)?;
            let application_payload = to_json(&application, "Jobs application")?;
            if tx.execute(
                "UPDATE jobs_applications
                    SET state = 'failed', application_json = ?3, updated_at_ms = ?4,
                        submitted_at_ms = NULL
                  WHERE account_id = ?1 AND id = ?2 AND state = 'side_effect_unknown'",
                params![account_id, application_id, application_payload, now],
            )? != 1
            {
                anyhow::bail!("submission outcome changed")
            }
            tx.commit()?;
            Ok(Some(application))
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            match crate::db::account_data::account_write_fence_postgres_tx(
                &mut tx, account_id,
            )? {
                crate::db::account_data::AccountWriteFence::Active => {}
                crate::db::account_data::AccountWriteFence::DeletionRequested => {
                    anyhow::bail!("account deletion has fenced submission reconciliation")
                }
                crate::db::account_data::AccountWriteFence::Missing => return Ok(None),
            }
            let row = tx.query_opt(
                "SELECT job_id, application_json FROM jobs_applications
                  WHERE account_id = $1 AND id = $2 FOR UPDATE",
                &[&account_id, &application_id],
            )?;
            let Some(row) = row else {
                return Ok(None);
            };
            let job_id: String = row.get(0);
            let application = parse_application_json(
                row.get(1),
                application_id,
                &job_id,
                "Jobs application",
            )?;
            if application.state == "failed" && is_owner_not_submitted_resolution(&application) {
                tx.commit()?;
                return Ok(Some(application));
            }
            if application.state != "side_effect_unknown" {
                anyhow::bail!("submission outcome is no longer awaiting reconciliation")
            }
            let run_id = application
                .run_id
                .clone()
                .ok_or_else(|| anyhow::anyhow!("application browser run is missing"))?;
            let session_row = tx.query_opt(
                "SELECT session_json, runner FROM jobs_browser_sessions
                  WHERE account_id = $1 AND id = $2 FOR UPDATE",
                &[&account_id, &run_id],
            )?;
            let Some(session_row) = session_row else {
                anyhow::bail!("browser session not found")
            };
            let session_raw: String = session_row.get(0);
            let runner: String = session_row.get(1);
            let terminal_session = resolved_not_submitted_session(
                session_raw,
                &runner,
                application_id,
                &run_id,
                now,
            )?;
            let session_payload = to_json(&terminal_session, "browser session")?;

            let authority_updated = if runner == "local" {
                tx.execute(
                    "UPDATE jobs_local_run_tickets
                        SET status = 'failed', updated_at_ms = $4
                      WHERE id = $1 AND account_id = $2 AND application_id = $3
                        AND status = 'side_effect_unknown'",
                    &[&run_id, &account_id, &application_id, &now],
                )?
            } else {
                tx.execute(
                    "UPDATE jobs_execution_leases
                        SET phase = 'released', updated_at_ms = $4, finished_at_ms = $4
                      WHERE run_id = $1 AND account_id = $2 AND application_id = $3
                        AND phase = 'side_effect_unknown'",
                    &[&run_id, &account_id, &application_id, &now],
                )?
            };
            if authority_updated != 1 {
                anyhow::bail!("matching execution authority changed")
            }
            if tx.execute(
                "UPDATE jobs_attempt_reservations
                    SET status = 'released', updated_at_ms = $3
                  WHERE account_id = $1 AND application_id = $2
                    AND status = 'side_effect_unknown'",
                &[&account_id, &application_id, &now],
            )? != 1
            {
                anyhow::bail!("application attempt reservation changed")
            }
            if tx.execute(
                "UPDATE jobs_browser_sessions
                    SET status = 'failed', session_json = $3, updated_at_ms = $4
                  WHERE account_id = $1 AND id = $2 AND status = 'needs_input'",
                &[&account_id, &run_id, &session_payload, &now],
            )? != 1
            {
                anyhow::bail!("browser session changed")
            }
            let _ = crate::db::object_uploads::release_submission_evidence_capacity_postgres_tx(
                &mut tx,
                account_id,
                application_id,
                &run_id,
                now,
            )?;
            validate_application_transition(&application.state, "failed")?;
            let application = resolved_not_submitted_application(application, &run_id, now)?;
            let application_payload = to_json(&application, "Jobs application")?;
            if tx.execute(
                "UPDATE jobs_applications
                    SET state = 'failed', application_json = $3, updated_at_ms = $4,
                        submitted_at_ms = NULL
                  WHERE account_id = $1 AND id = $2 AND state = 'side_effect_unknown'",
                &[&account_id, &application_id, &application_payload, &now],
            )? != 1
            {
                anyhow::bail!("submission outcome changed")
            }
            tx.commit()?;
            Ok(Some(application))
        }
    })
}
