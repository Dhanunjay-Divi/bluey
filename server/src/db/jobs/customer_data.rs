pub fn list_browser_sessions(pool: &DbPool, account_id: &str) -> Result<Vec<BrowserSession>> {
    list_payloads(
        pool,
        account_id,
        "jobs_browser_sessions",
        "session_json",
        "updated_at_ms DESC",
        "browser session",
    )
}

pub fn upsert_browser_session(
    pool: &DbPool,
    account_id: &str,
    session: &BrowserSession,
) -> Result<BrowserSession> {
    let mut value = session.clone();
    if value.id.is_empty() {
        value.id = uuid::Uuid::new_v4().to_string();
    }
    let now = now_ms();
    if value.created_at_ms == 0 {
        value.created_at_ms = now;
    }
    value.updated_at_ms = now;
    let payload = to_json(&value, "browser session")?;
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            crate::db::object_uploads::require_active_account_write_fence_sqlite_tx(
                &tx, account_id,
            )?;
            tx.execute(
                "INSERT INTO jobs_browser_sessions (
                    id, account_id, runner, status, session_json, created_at_ms, updated_at_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                 ON CONFLICT(id) DO UPDATE SET status = excluded.status,
                    session_json = excluded.session_json, updated_at_ms = excluded.updated_at_ms
                 WHERE jobs_browser_sessions.account_id = excluded.account_id",
                params![
                    value.id,
                    account_id,
                    value.runner,
                    value.status,
                    payload,
                    value.created_at_ms,
                    value.updated_at_ms,
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
                "INSERT INTO jobs_browser_sessions (
                    id, account_id, runner, status, session_json, created_at_ms, updated_at_ms
                 ) VALUES ($1, $2, $3, $4, $5, $6, $7)
                 ON CONFLICT(id) DO UPDATE SET status = EXCLUDED.status,
                    session_json = EXCLUDED.session_json, updated_at_ms = EXCLUDED.updated_at_ms
                 WHERE jobs_browser_sessions.account_id = EXCLUDED.account_id",
                &[
                    &value.id,
                    &account_id,
                    &value.runner,
                    &value.status,
                    &payload,
                    &value.created_at_ms,
                    &value.updated_at_ms,
                ],
            )?;
            tx.commit()?;
            Ok(value)
        }
    })
}

pub fn list_interventions(pool: &DbPool, account_id: &str) -> Result<Vec<Intervention>> {
    list_payloads(
        pool,
        account_id,
        "jobs_interventions",
        "intervention_json",
        "status = 'open' DESC, created_at_ms DESC",
        "intervention",
    )
}

pub fn save_intervention(
    pool: &DbPool,
    account_id: &str,
    intervention: &Intervention,
) -> Result<Intervention> {
    let mut value = intervention.clone();
    value.kind = value.kind.trim().to_ascii_lowercase();
    value.status = value.status.trim().to_ascii_lowercase();
    value.resolution_kind = value.resolution_kind.trim().to_ascii_lowercase();
    value.provider = value.provider.trim().to_ascii_lowercase();
    if !matches!(
        value.kind.as_str(),
        "captcha"
            | "two_factor"
            | "assessment"
            | "unknown_question"
            | "missing_fact"
            | "sensitive_question"
            | "browser_takeover"
    ) {
        anyhow::bail!("invalid intervention kind")
    }
    if !matches!(
        value.status.as_str(),
        "open" | "approved" | "resolved" | "expired" | "cancelled"
    ) {
        anyhow::bail!("invalid intervention status")
    }
    if !matches!(
        value.resolution_kind.as_str(),
        "" | "browser_takeover" | "email_otp_approval" | "answer"
    ) {
        anyhow::bail!("invalid intervention resolution")
    }
    if let Some(application_id) = value.application_id.as_deref() {
        if get_application(pool, account_id, application_id)?.is_none() {
            anyhow::bail!("application not found")
        }
    }
    if value.resolution_kind == "email_otp_approval"
        && (value.kind != "two_factor"
            || !matches!(value.provider.as_str(), "gmail" | "outlook_email")
            || value.provider_message_id.trim().is_empty()
            || value.expires_at_ms.is_none())
    {
        anyhow::bail!("email verification needs a provider message and expiry")
    }
    if value.metadata.is_null() {
        value.metadata = json!({});
    }
    if contains_authentication_secret(&value.metadata) {
        anyhow::bail!("authentication codes and credentials cannot be stored in interventions")
    }
    if value.id.is_empty() {
        value.id = uuid::Uuid::new_v4().to_string();
    }
    let now = now_ms();
    if value.created_at_ms == 0 {
        value.created_at_ms = now;
    }
    if value.status == "open"
        && value
            .expires_at_ms
            .is_some_and(|expires_at| expires_at <= now)
    {
        value.status = "expired".to_string();
    }
    if matches!(value.status.as_str(), "resolved" | "cancelled" | "expired")
        && value.resolved_at_ms.is_none()
    {
        value.resolved_at_ms = Some(now);
    }
    let payload = to_json(&value, "intervention")?;
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            crate::db::object_uploads::require_active_account_write_fence_sqlite_tx(
                &tx, account_id,
            )?;
            tx.execute(
                "INSERT INTO jobs_interventions (
                    id, account_id, application_id, kind, status, intervention_json,
                    created_at_ms, resolved_at_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                 ON CONFLICT(id) DO UPDATE SET status = excluded.status,
                    intervention_json = excluded.intervention_json,
                    resolved_at_ms = excluded.resolved_at_ms
                 WHERE jobs_interventions.account_id = excluded.account_id",
                params![
                    value.id,
                    account_id,
                    value.application_id,
                    value.kind,
                    value.status,
                    payload,
                    value.created_at_ms,
                    value.resolved_at_ms,
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
                "INSERT INTO jobs_interventions (
                    id, account_id, application_id, kind, status, intervention_json,
                    created_at_ms, resolved_at_ms
                 ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
                 ON CONFLICT(id) DO UPDATE SET status = EXCLUDED.status,
                    intervention_json = EXCLUDED.intervention_json,
                    resolved_at_ms = EXCLUDED.resolved_at_ms
                 WHERE jobs_interventions.account_id = EXCLUDED.account_id",
                &[
                    &value.id,
                    &account_id,
                    &value.application_id,
                    &value.kind,
                    &value.status,
                    &payload,
                    &value.created_at_ms,
                    &value.resolved_at_ms,
                ],
            )?;
            tx.commit()?;
            Ok(value)
        }
    })
}

#[derive(Debug, Clone)]
pub struct InterventionAnswerRevisionResult {
    pub intervention: Intervention,
    pub application: JobApplication,
    pub question: String,
}

fn prepare_intervention_answer_revision(
    mut intervention: Intervention,
    mut application: JobApplication,
    answer: &str,
    now: i64,
) -> Result<(InterventionAnswerRevisionResult, Option<String>)> {
    let answer = answer.trim();
    if answer.is_empty() {
        anyhow::bail!("enter the answer Bluey should use")
    }
    if answer.len() > 10_000 {
        anyhow::bail!("application answer is too long")
    }
    if intervention.status != "open" {
        anyhow::bail!("this intervention has already been resolved")
    }
    if intervention.resolution_kind != "answer"
        || !matches!(
            intervention.kind.as_str(),
            "unknown_question" | "missing_fact" | "sensitive_question"
        )
    {
        anyhow::bail!("this intervention is not waiting for an application answer")
    }
    if application.state != "needs_input" {
        anyhow::bail!("this application is no longer waiting for an answer")
    }
    validate_application_transition(&application.state, "awaiting_review")?;

    let field = intervention
        .metadata
        .pointer("/receipt/intervention/field")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::trim);
    let question = intervention
        .metadata
        .pointer("/receipt/intervention/question")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(&intervention.title)
        .trim()
        .to_string();
    let answer_key = normalize_answer_memory_key(field.unwrap_or(&question));
    if answer_key.is_empty() {
        anyhow::bail!("application question is missing")
    }
    let answer_value = json!({
        "key": answer_key,
        "question": question,
        "value": answer,
        "source": "intervention",
        "confirmed": true,
        "intervention_id": intervention.id,
        "updated_at_ms": now,
    });
    let matching_answer = application.answers.iter_mut().find(|candidate| {
        let existing = candidate
            .get("key")
            .or_else(|| candidate.get("question"))
            .or_else(|| candidate.get("field"))
            .or_else(|| candidate.get("name"))
            .and_then(Value::as_str)
            .unwrap_or_default();
        normalize_answer_memory_key(existing) == answer_key
    });
    if let Some(existing) = matching_answer {
        *existing = answer_value;
    } else {
        application.answers.push(answer_value);
    }

    let answers = Value::Array(application.answers.clone());
    let receipt = application.receipt.as_object_mut().ok_or_else(|| {
        anyhow::anyhow!("application receipt is unavailable; prepare the packet again")
    })?;
    receipt.insert("final_answers".to_string(), answers);
    let invalidated_checksum = receipt
        .remove("approved_execution")
        .and_then(|value| value.get("checksum").cloned())
        .unwrap_or(Value::Null);
    let revision = json!({
        "reason": "intervention_answer",
        "intervention_id": intervention.id,
        "revised_at_ms": now,
        "reapproval_required": true,
        "invalidated_packet_checksum": invalidated_checksum,
    });
    let revisions = receipt
        .entry("packet_revisions".to_string())
        .or_insert_with(|| json!([]));
    let revisions = revisions
        .as_array_mut()
        .ok_or_else(|| anyhow::anyhow!("application packet revision history is invalid"))?;
    revisions.push(revision.clone());
    receipt.insert("packet_revision".to_string(), revision);

    let metadata = intervention
        .metadata
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("this intervention cannot be answered"))?;
    metadata.insert("resolved_answer".to_string(), json!(answer));
    metadata.insert("answered_at_ms".to_string(), json!(now));
    metadata.insert("reapproval_required".to_string(), json!(true));
    intervention.status = "resolved".to_string();
    intervention.resolved_at_ms = Some(now);

    let previous_run_id = application.run_id.take();
    application.state = "awaiting_review".to_string();
    application.updated_at_ms = now;
    Ok((
        InterventionAnswerRevisionResult {
            intervention,
            application,
            question,
        },
        previous_run_id,
    ))
}

fn reject_irreversible_answer_revision(
    lease_phase: Option<&str>,
    local_ticket_status: Option<&str>,
) -> Result<()> {
    if matches!(
        lease_phase,
        Some("click_started" | "submitted" | "side_effect_unknown")
    ) || matches!(
        local_ticket_status,
        Some("click_started" | "complete" | "side_effect_unknown")
    ) {
        anyhow::bail!(
            "application submission is awaiting reconciliation; answers cannot change yet"
        )
    }
    Ok(())
}

pub fn resolve_intervention_answer_for_review(
    pool: &DbPool,
    account_id: &str,
    intervention_id: &str,
    answer: &str,
) -> Result<InterventionAnswerRevisionResult> {
    let account_id = account_id.trim();
    let intervention_id = intervention_id.trim();
    if account_id.is_empty() || intervention_id.is_empty() {
        anyhow::bail!("intervention not found")
    }
    let now = now_ms();

    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let row: Option<(Option<String>, String)> = tx
                .query_row(
                    "SELECT application_id, intervention_json FROM jobs_interventions
                      WHERE account_id = ?1 AND id = ?2",
                    params![account_id, intervention_id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()?;
            let Some((application_id, raw)) = row else {
                anyhow::bail!("intervention not found")
            };
            let application_id = application_id
                .ok_or_else(|| anyhow::anyhow!("intervention is not attached to an application"))?;
            let mut intervention: Intervention = parse_json(raw, "intervention")?;
            intervention.id = intervention_id.to_string();
            intervention.application_id = Some(application_id.clone());
            let row: Option<(String, String)> = tx
                .query_row(
                    "SELECT job_id, application_json FROM jobs_applications
                      WHERE account_id = ?1 AND id = ?2",
                    params![account_id, application_id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()?;
            let Some((job_id, raw)) = row else {
                anyhow::bail!("application not found")
            };
            let application =
                parse_application_json(raw, &application_id, &job_id, "job application")?;
            let (revision, previous_run_id) =
                prepare_intervention_answer_revision(intervention, application, answer, now)?;

            let mut browser_session: Option<BrowserSession> = None;
            let mut lease_phase: Option<String> = None;
            let mut local_ticket_status: Option<String> = None;
            if let Some(run_id) = previous_run_id.as_deref() {
                lease_phase = tx
                    .query_row(
                        "SELECT phase FROM jobs_execution_leases
                          WHERE account_id = ?1 AND application_id = ?2 AND run_id = ?3",
                        params![account_id, application_id, run_id],
                        |row| row.get(0),
                    )
                    .optional()?;
                local_ticket_status = tx
                    .query_row(
                        "SELECT status FROM jobs_local_run_tickets
                          WHERE account_id = ?1 AND application_id = ?2 AND id = ?3",
                        params![account_id, application_id, run_id],
                        |row| row.get(0),
                    )
                    .optional()?;
                browser_session = tx
                    .query_row(
                        "SELECT session_json FROM jobs_browser_sessions
                          WHERE account_id = ?1 AND id = ?2",
                        params![account_id, run_id],
                        |row| row.get::<_, String>(0),
                    )
                    .optional()?
                    .map(|raw| parse_json(raw, "browser session"))
                    .transpose()?;
            }
            reject_irreversible_answer_revision(
                lease_phase.as_deref(),
                local_ticket_status.as_deref(),
            )?;

            if let Some(run_id) = previous_run_id.as_deref() {
                let ats_binding = tx
                    .query_row(
                        "SELECT binding_id, attempt_id, nonce_sha256
                           FROM jobs_application_ats_certification_bindings
                          WHERE account_id = ?1 AND application_id = ?2 AND run_id = ?3",
                        params![account_id, application_id, run_id],
                        |row| {
                            Ok((
                                row.get::<_, String>(0)?,
                                row.get::<_, String>(1)?,
                                row.get::<_, String>(2)?,
                            ))
                        },
                    )
                    .optional()?;
                if let Some((binding_id, application_attempt_id, nonce_sha256)) = ats_binding {
                    invalidate_ats_application_certification_binding_sqlite_tx(
                        &tx,
                        &AtsCertificationBindingInvalidationRequest {
                            binding_id,
                            account_id: account_id.to_string(),
                            application_id: application_id.clone(),
                            run_id: run_id.to_string(),
                            application_attempt_id,
                            nonce_sha256,
                            expected_fence: 0,
                            invalidation_kind: "packet_changed".to_string(),
                        },
                        now,
                    )?;
                }
                if lease_phase.as_deref() == Some("prepared")
                    && tx.execute(
                        "UPDATE jobs_execution_leases
                            SET phase = 'released', updated_at_ms = ?4, finished_at_ms = ?4
                          WHERE account_id = ?1 AND application_id = ?2 AND run_id = ?3
                            AND phase = 'prepared'",
                        params![account_id, application_id, run_id, now],
                    )? != 1
                {
                    anyhow::bail!("cloud execution authority changed")
                }
                if matches!(
                    local_ticket_status.as_deref(),
                    Some("queued" | "claimed" | "needs_input")
                ) && tx.execute(
                    "UPDATE jobs_local_run_tickets SET status = 'failed', updated_at_ms = ?4
                      WHERE account_id = ?1 AND application_id = ?2 AND id = ?3
                        AND status IN ('queued', 'claimed', 'needs_input')",
                    params![account_id, application_id, run_id, now],
                )? != 1
                {
                    anyhow::bail!("local execution authority changed")
                }
                tx.execute(
                    "DELETE FROM jobs_local_run_resume_actions
                      WHERE account_id = ?1 AND application_id = ?2 AND run_id = ?3
                        AND status = 'approved'",
                    params![account_id, application_id, run_id],
                )?;
                tx.execute(
                    "UPDATE jobs_attempt_reservations
                        SET status = 'released', updated_at_ms = ?3
                      WHERE account_id = ?1 AND application_id = ?2
                        AND status IN ('reserved', 'running')",
                    params![account_id, application_id, now],
                )?;
                if let Some(mut session) = browser_session {
                    session.status = "paused".to_string();
                    session.current_step = "Application kit changed; review required".to_string();
                    session.updated_at_ms = now;
                    let payload = to_json(&session, "browser session")?;
                    if tx.execute(
                        "UPDATE jobs_browser_sessions SET status = 'paused',
                            session_json = ?3, updated_at_ms = ?4
                          WHERE account_id = ?1 AND id = ?2",
                        params![account_id, run_id, payload, now],
                    )? != 1
                    {
                        anyhow::bail!("browser session changed")
                    }
                }
            }

            let application_payload = to_json(&revision.application, "job application")?;
            if tx.execute(
                "UPDATE jobs_applications SET state = 'awaiting_review',
                    application_json = ?3, updated_at_ms = ?4
                  WHERE account_id = ?1 AND id = ?2 AND state = 'needs_input'",
                params![account_id, application_id, application_payload, now],
            )? != 1
            {
                anyhow::bail!("application no longer waiting for an answer")
            }
            let intervention_payload = to_json(&revision.intervention, "intervention")?;
            if tx.execute(
                "UPDATE jobs_interventions SET status = 'resolved',
                    intervention_json = ?3, resolved_at_ms = ?4
                  WHERE account_id = ?1 AND id = ?2 AND status = 'open'",
                params![account_id, intervention_id, intervention_payload, now],
            )? != 1
            {
                anyhow::bail!("intervention has already been resolved")
            }
            tx.commit()?;
            Ok(revision)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            lock_postgres_ats_certification(&mut tx)?;

            // Discover the scope without row locks, then acquire every row in
            // the same order as certified pre-click authorization: cloud
            // lease, local ticket, ATS binding, and application. The ATS
            // advisory lock keeps the binding set stable while the scope is
            // discovered. Every authoritative value is re-read under its row
            // lock before mutation.
            let row = tx.query_opt(
                "SELECT intervention.application_id, application.job_id,
                        application.application_json
                   FROM jobs_interventions intervention
                   LEFT JOIN jobs_applications application
                     ON application.account_id = intervention.account_id
                    AND application.id = intervention.application_id
                  WHERE intervention.account_id = $1 AND intervention.id = $2",
                &[&account_id, &intervention_id],
            )?;
            let Some(row) = row else {
                anyhow::bail!("intervention not found")
            };
            let application_id: Option<String> = row.get(0);
            let application_id = application_id
                .ok_or_else(|| anyhow::anyhow!("intervention is not attached to an application"))?;
            let job_id: Option<String> = row.get(1);
            let application_json: Option<String> = row.get(2);
            let (Some(job_id), Some(application_json)) = (job_id, application_json) else {
                anyhow::bail!("application not found")
            };
            let discovered_application = parse_application_json(
                application_json,
                &application_id,
                &job_id,
                "job application",
            )?;
            let discovered_run_id = discovered_application.run_id.clone();

            let mut lease_phase: Option<String> = None;
            let mut local_ticket_status: Option<String> = None;
            let mut ats_binding: Option<(String, String, String)> = None;
            if let Some(run_id) = discovered_run_id.as_deref() {
                lease_phase = tx
                    .query_opt(
                        "SELECT phase FROM jobs_execution_leases
                          WHERE account_id = $1 AND application_id = $2 AND run_id = $3
                          FOR UPDATE",
                        &[&account_id, &application_id, &run_id],
                    )?
                    .map(|row| row.get::<_, String>(0));
                local_ticket_status = tx
                    .query_opt(
                        "SELECT status FROM jobs_local_run_tickets
                          WHERE account_id = $1 AND application_id = $2 AND id = $3
                          FOR UPDATE",
                        &[&account_id, &application_id, &run_id],
                    )?
                    .map(|row| row.get::<_, String>(0));
                ats_binding = tx
                    .query_opt(
                        "SELECT binding_id, attempt_id, nonce_sha256
                           FROM jobs_application_ats_certification_bindings
                          WHERE account_id = $1 AND application_id = $2 AND run_id = $3
                          FOR UPDATE",
                        &[&account_id, &application_id, &run_id],
                    )?
                    .map(|row| (row.get(0), row.get(1), row.get(2)));
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
            let application =
                parse_application_json(row.get(1), &application_id, &job_id, "job application")?;
            if application.run_id.as_deref() != discovered_run_id.as_deref() {
                anyhow::bail!("application execution authority changed")
            }

            let row = tx.query_opt(
                "SELECT application_id, intervention_json FROM jobs_interventions
                  WHERE account_id = $1 AND id = $2 FOR UPDATE",
                &[&account_id, &intervention_id],
            )?;
            let Some(row) = row else {
                anyhow::bail!("intervention not found")
            };
            if row.get::<_, Option<String>>(0).as_deref() != Some(application_id.as_str()) {
                anyhow::bail!("intervention is not attached to this application")
            }
            let mut intervention: Intervention = parse_json(row.get(1), "intervention")?;
            intervention.id = intervention_id.to_string();
            intervention.application_id = Some(application_id.clone());
            let (revision, previous_run_id) =
                prepare_intervention_answer_revision(intervention, application, answer, now)?;
            if previous_run_id.as_deref() != discovered_run_id.as_deref() {
                anyhow::bail!("application execution authority changed")
            }

            let browser_session: Option<BrowserSession> = if let Some(run_id) =
                previous_run_id.as_deref()
            {
                tx.query_opt(
                    "SELECT session_json FROM jobs_browser_sessions
                      WHERE account_id = $1 AND id = $2 FOR UPDATE",
                    &[&account_id, &run_id],
                )?
                .map(|row| parse_json(row.get(0), "browser session"))
                .transpose()?
            } else {
                None
            };
            reject_irreversible_answer_revision(
                lease_phase.as_deref(),
                local_ticket_status.as_deref(),
            )?;

            if let Some(run_id) = previous_run_id.as_deref() {
                if let Some((binding_id, application_attempt_id, nonce_sha256)) = ats_binding {
                    invalidate_ats_application_certification_binding_postgres_tx(
                        &mut tx,
                        &AtsCertificationBindingInvalidationRequest {
                            binding_id,
                            account_id: account_id.to_string(),
                            application_id: application_id.clone(),
                            run_id: run_id.to_string(),
                            application_attempt_id,
                            nonce_sha256,
                            expected_fence: 0,
                            invalidation_kind: "packet_changed".to_string(),
                        },
                        now,
                    )?;
                }
                if lease_phase.as_deref() == Some("prepared")
                    && tx.execute(
                        "UPDATE jobs_execution_leases
                            SET phase = 'released', updated_at_ms = $4, finished_at_ms = $4
                          WHERE account_id = $1 AND application_id = $2 AND run_id = $3
                            AND phase = 'prepared'",
                        &[&account_id, &application_id, &run_id, &now],
                    )? != 1
                {
                    anyhow::bail!("cloud execution authority changed")
                }
                if matches!(
                    local_ticket_status.as_deref(),
                    Some("queued" | "claimed" | "needs_input")
                ) && tx.execute(
                    "UPDATE jobs_local_run_tickets SET status = 'failed', updated_at_ms = $4
                      WHERE account_id = $1 AND application_id = $2 AND id = $3
                        AND status IN ('queued', 'claimed', 'needs_input')",
                    &[&account_id, &application_id, &run_id, &now],
                )? != 1
                {
                    anyhow::bail!("local execution authority changed")
                }
                tx.execute(
                    "DELETE FROM jobs_local_run_resume_actions
                      WHERE account_id = $1 AND application_id = $2 AND run_id = $3
                        AND status = 'approved'",
                    &[&account_id, &application_id, &run_id],
                )?;
                tx.execute(
                    "UPDATE jobs_attempt_reservations
                        SET status = 'released', updated_at_ms = $3
                      WHERE account_id = $1 AND application_id = $2
                        AND status IN ('reserved', 'running')",
                    &[&account_id, &application_id, &now],
                )?;
                if let Some(mut session) = browser_session {
                    session.status = "paused".to_string();
                    session.current_step = "Application kit changed; review required".to_string();
                    session.updated_at_ms = now;
                    let payload = to_json(&session, "browser session")?;
                    if tx.execute(
                        "UPDATE jobs_browser_sessions SET status = 'paused',
                            session_json = $3, updated_at_ms = $4
                          WHERE account_id = $1 AND id = $2",
                        &[&account_id, &run_id, &payload, &now],
                    )? != 1
                    {
                        anyhow::bail!("browser session changed")
                    }
                }
            }

            let application_payload = to_json(&revision.application, "job application")?;
            if tx.execute(
                "UPDATE jobs_applications SET state = 'awaiting_review',
                    application_json = $3, updated_at_ms = $4
                  WHERE account_id = $1 AND id = $2 AND state = 'needs_input'",
                &[&account_id, &application_id, &application_payload, &now],
            )? != 1
            {
                anyhow::bail!("application no longer waiting for an answer")
            }
            let intervention_payload = to_json(&revision.intervention, "intervention")?;
            if tx.execute(
                "UPDATE jobs_interventions SET status = 'resolved',
                    intervention_json = $3, resolved_at_ms = $4
                  WHERE account_id = $1 AND id = $2 AND status = 'open'",
                &[&account_id, &intervention_id, &intervention_payload, &now],
            )? != 1
            {
                anyhow::bail!("intervention has already been resolved")
            }
            tx.commit()?;
            Ok(revision)
        }
    })
}

pub fn normalize_answer_memory_key(value: &str) -> String {
    let mut normalized = String::with_capacity(value.len());
    let mut pending_space = false;
    for character in value.chars().flat_map(char::to_lowercase) {
        if character.is_ascii_alphanumeric() {
            if pending_space && !normalized.is_empty() {
                normalized.push(' ');
            }
            normalized.push(character);
            pending_space = false;
        } else {
            pending_space = true;
        }
    }
    normalized
}

pub fn list_answer_memory(pool: &DbPool, account_id: &str) -> Result<Vec<AnswerMemory>> {
    list_payloads(
        pool,
        account_id,
        "jobs_answer_memory",
        "answer_json",
        "updated_at_ms DESC",
        "answer memory",
    )
}

pub fn save_answer_memory(
    pool: &DbPool,
    account_id: &str,
    answer: &AnswerMemory,
) -> Result<AnswerMemory> {
    let mut value = answer.clone();
    value.question = value.question.trim().to_string();
    value.value = value.value.trim().to_string();
    value.scope = value.scope.trim().to_ascii_lowercase();
    value.source = value.source.trim().to_ascii_lowercase();
    value.key = normalize_answer_memory_key(if value.key.trim().is_empty() {
        &value.question
    } else {
        &value.key
    });
    if value.question.is_empty() || value.key.is_empty() {
        anyhow::bail!("enter the application question")
    }
    if value.value.is_empty() {
        anyhow::bail!("enter the answer Bluey should remember")
    }
    if value.question.len() > 2_000 || value.value.len() > 10_000 {
        anyhow::bail!("answer memory is too long")
    }
    if !matches!(value.scope.as_str(), "account" | "track" | "company") {
        anyhow::bail!("invalid answer memory scope")
    }
    let scope_id = value
        .scope_id
        .as_deref()
        .unwrap_or_default()
        .trim()
        .to_string();
    if value.scope == "account" {
        value.scope_id = None;
    } else if scope_id.is_empty() {
        anyhow::bail!("choose where this answer should be reused")
    } else {
        value.scope_id = Some(scope_id);
    }
    if value.scope == "track"
        && !list_tracks(pool, account_id)?
            .iter()
            .any(|track| Some(track.id.as_str()) == value.scope_id.as_deref())
    {
        anyhow::bail!("career track not found")
    }
    if value.source.is_empty() {
        value.source = "settings".to_string();
    }
    value.confirmed = true;

    let existing = list_answer_memory(pool, account_id)?
        .into_iter()
        .find(|item| {
            item.scope == value.scope
                && item.scope_id.as_deref().unwrap_or_default()
                    == value.scope_id.as_deref().unwrap_or_default()
                && item.key == value.key
        });
    if let Some(existing) = existing {
        value.id = existing.id;
        if value.created_at_ms == 0 {
            value.created_at_ms = existing.created_at_ms;
        }
        if value.last_used_at_ms.is_none() {
            value.last_used_at_ms = existing.last_used_at_ms;
        }
        value.use_count = value.use_count.max(existing.use_count);
    }
    if value.id.is_empty() {
        value.id = uuid::Uuid::new_v4().to_string();
    }
    let now = now_ms();
    if value.created_at_ms == 0 {
        value.created_at_ms = now;
    }
    value.updated_at_ms = now;
    value.use_count = value.use_count.max(0);
    let scope_id = value.scope_id.as_deref().unwrap_or_default().to_string();
    let question_hash = private_lookup_hash(
        &format!("jobs-answer-memory:{account_id}:{}:{scope_id}", value.scope),
        &value.key,
    )?;
    let payload = to_json(&value, "answer memory")?;
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            pool.get()?.execute(
                "INSERT INTO jobs_answer_memory (
                    id, account_id, scope, scope_id, question_hash, answer_json,
                    created_at_ms, updated_at_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                 ON CONFLICT(id) DO UPDATE SET scope = excluded.scope,
                    scope_id = excluded.scope_id, question_hash = excluded.question_hash,
                    answer_json = excluded.answer_json, updated_at_ms = excluded.updated_at_ms
                 WHERE jobs_answer_memory.account_id = excluded.account_id",
                params![
                    value.id,
                    account_id,
                    value.scope,
                    scope_id,
                    question_hash,
                    payload,
                    value.created_at_ms,
                    value.updated_at_ms,
                ],
            )?;
            Ok(value)
        }
        DbPool::Postgres(_) => {
            pool.get_pg()?.execute(
                "INSERT INTO jobs_answer_memory (
                    id, account_id, scope, scope_id, question_hash, answer_json,
                    created_at_ms, updated_at_ms
                 ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
                 ON CONFLICT(id) DO UPDATE SET scope = EXCLUDED.scope,
                    scope_id = EXCLUDED.scope_id, question_hash = EXCLUDED.question_hash,
                    answer_json = EXCLUDED.answer_json, updated_at_ms = EXCLUDED.updated_at_ms
                 WHERE jobs_answer_memory.account_id = EXCLUDED.account_id",
                &[
                    &value.id,
                    &account_id,
                    &value.scope,
                    &scope_id,
                    &question_hash,
                    &payload,
                    &value.created_at_ms,
                    &value.updated_at_ms,
                ],
            )?;
            Ok(value)
        }
    })
}

pub fn delete_answer_memory(pool: &DbPool, account_id: &str, answer_id: &str) -> Result<bool> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => Ok(pool.get()?.execute(
            "DELETE FROM jobs_answer_memory WHERE account_id = ?1 AND id = ?2",
            params![account_id, answer_id],
        )? > 0),
        DbPool::Postgres(_) => Ok(pool.get_pg()?.execute(
            "DELETE FROM jobs_answer_memory WHERE account_id = $1 AND id = $2",
            &[&account_id, &answer_id],
        )? > 0),
    })
}

pub fn list_candidate_events(pool: &DbPool, account_id: &str) -> Result<Vec<CandidateEvent>> {
    list_payloads(
        pool,
        account_id,
        "jobs_candidate_events",
        "event_json",
        "created_at_ms DESC, id DESC",
        "candidate event",
    )
}

pub fn save_candidate_event(
    pool: &DbPool,
    account_id: &str,
    event: &CandidateEvent,
) -> Result<CandidateEvent> {
    let mut value = event.clone();
    value.event_type = value.event_type.trim().to_ascii_lowercase();
    value.action = value.action.trim().to_ascii_lowercase();
    value.note = value.note.trim().to_string();
    value.reasons = value
        .reasons
        .iter()
        .map(|reason| reason.trim().to_ascii_lowercase())
        .filter(|reason| !reason.is_empty())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    if value.note.len() > 1_000 || value.reasons.len() > 8 {
        anyhow::bail!("candidate feedback is too long")
    }

    let application = if let Some(application_id) = value.application_id.as_deref() {
        Some(
            get_application(pool, account_id, application_id)?
                .ok_or_else(|| anyhow::anyhow!("application not found"))?,
        )
    } else {
        None
    };
    if value.job_id.is_none() {
        value.job_id = application.as_ref().map(|item| item.job_id.clone());
    }
    if let Some(application) = application.as_ref() {
        if value.job_id.as_deref() != Some(application.job_id.as_str()) {
            anyhow::bail!("application does not belong to this job")
        }
    }
    if let Some(job_id) = value.job_id.as_deref() {
        if get_posting(pool, account_id, job_id)?.is_none() {
            anyhow::bail!("job not found")
        }
    }

    value.status = match value.event_type.as_str() {
        "match_feedback" => {
            if value.application_id.is_some() || value.job_id.is_none() {
                anyhow::bail!("match feedback must reference one job")
            }
            if !matches!(value.action.as_str(), "pass" | "restore") {
                anyhow::bail!("invalid match feedback action")
            }
            if value.action == "pass"
                && value.reasons.iter().any(|reason| {
                    !matches!(
                        reason.as_str(),
                        "role_mismatch"
                            | "location"
                            | "compensation"
                            | "seniority"
                            | "company"
                            | "sponsorship"
                            | "already_applied"
                            | "not_interested"
                            | "other"
                    )
                })
            {
                anyhow::bail!("invalid match feedback reason")
            }
            "recorded"
        }
        "application_issue" => {
            if value.application_id.is_none() {
                anyhow::bail!("application issue must reference an application")
            }
            if !matches!(
                value.action.as_str(),
                "site_problem"
                    | "wrong_information"
                    | "duplicate_application"
                    | "submission_status"
                    | "billing"
                    | "other"
            ) {
                anyhow::bail!("invalid application issue category")
            }
            "open"
        }
        "application_outcome" => {
            if value.application_id.is_none() {
                anyhow::bail!("application outcome must reference an application")
            }
            if !matches!(
                value.action.as_str(),
                "interview" | "rejected" | "offer" | "withdrawn"
            ) {
                anyhow::bail!("invalid application outcome")
            }
            "confirmed"
        }
        _ => anyhow::bail!("invalid candidate event type"),
    }
    .to_string();

    value.id = uuid::Uuid::new_v4().to_string();
    let now = now_ms();
    value.created_at_ms = now;
    value.updated_at_ms = now;
    let payload = to_json(&value, "candidate event")?;
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            pool.get()?.execute(
                "INSERT INTO jobs_candidate_events (
                    id, account_id, event_type, job_id, application_id, status,
                    event_json, created_at_ms, updated_at_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    value.id,
                    account_id,
                    value.event_type,
                    value.job_id,
                    value.application_id,
                    value.status,
                    payload,
                    value.created_at_ms,
                    value.updated_at_ms,
                ],
            )?;
            Ok(value)
        }
        DbPool::Postgres(_) => {
            pool.get_pg()?.execute(
                "INSERT INTO jobs_candidate_events (
                    id, account_id, event_type, job_id, application_id, status,
                    event_json, created_at_ms, updated_at_ms
                 ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
                &[
                    &value.id,
                    &account_id,
                    &value.event_type,
                    &value.job_id,
                    &value.application_id,
                    &value.status,
                    &payload,
                    &value.created_at_ms,
                    &value.updated_at_ms,
                ],
            )?;
            Ok(value)
        }
    })
}

fn contains_authentication_secret(value: &Value) -> bool {
    match value {
        Value::Object(items) => items.iter().any(|(key, nested)| {
            matches!(
                key.trim().to_ascii_lowercase().as_str(),
                "code"
                    | "otp"
                    | "one_time_code"
                    | "password"
                    | "access_token"
                    | "refresh_token"
                    | "secret"
                    | "credential"
            ) || contains_authentication_secret(nested)
        }),
        Value::Array(items) => items.iter().any(contains_authentication_secret),
        _ => false,
    }
}

pub fn list_application_evidence(
    pool: &DbPool,
    account_id: &str,
    application_id: Option<&str>,
) -> Result<Vec<ApplicationEvidence>> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get()?;
            let raws = if let Some(application_id) = application_id {
                let mut stmt = conn.prepare(
                    "SELECT evidence_json FROM jobs_application_evidence
                      WHERE account_id = ?1 AND application_id = ?2
                      ORDER BY occurred_at_ms DESC, created_at_ms DESC",
                )?;
                let values = stmt
                    .query_map(params![account_id, application_id], |row| {
                        row.get::<_, String>(0)
                    })?
                    .collect::<std::result::Result<Vec<_>, _>>()?;
                values
            } else {
                let mut stmt = conn.prepare(
                    "SELECT evidence_json FROM jobs_application_evidence
                      WHERE account_id = ?1
                      ORDER BY occurred_at_ms DESC, created_at_ms DESC",
                )?;
                let values = stmt
                    .query_map(params![account_id], |row| row.get::<_, String>(0))?
                    .collect::<std::result::Result<Vec<_>, _>>()?;
                values
            };
            raws.into_iter()
                .map(|raw| parse_json(raw, "application evidence"))
                .collect()
        }
        DbPool::Postgres(_) => {
            let rows = if let Some(application_id) = application_id {
                pool.get_pg()?.query(
                    "SELECT evidence_json FROM jobs_application_evidence
                      WHERE account_id = $1 AND application_id = $2
                      ORDER BY occurred_at_ms DESC, created_at_ms DESC",
                    &[&account_id, &application_id],
                )?
            } else {
                pool.get_pg()?.query(
                    "SELECT evidence_json FROM jobs_application_evidence
                      WHERE account_id = $1
                      ORDER BY occurred_at_ms DESC, created_at_ms DESC",
                    &[&account_id],
                )?
            };
            rows.into_iter()
                .map(|row| parse_json(row.get(0), "application evidence"))
                .collect()
        }
    })
}

pub fn save_application_evidence(
    pool: &DbPool,
    account_id: &str,
    evidence: &ApplicationEvidence,
) -> Result<ApplicationEvidence> {
    let application = get_application(pool, account_id, &evidence.application_id)?
        .ok_or_else(|| anyhow::anyhow!("application not found"))?;
    let mut value = evidence.clone();
    if !matches!(
        value.kind.as_str(),
        "resume"
            | "cover_letter"
            | "attachment"
            | "application_receipt"
            | "submission_confirmation"
            | "status_email"
            | "interview_event"
    ) {
        anyhow::bail!("invalid application evidence kind")
    }
    if value.id.is_empty() {
        value.id = uuid::Uuid::new_v4().to_string();
    }
    if value.occurred_at_ms == 0 {
        value.occurred_at_ms = now_ms();
    }
    if value.created_at_ms == 0 {
        value.created_at_ms = now_ms();
    }
    if value.metadata.is_null() {
        value.metadata = json!({});
    }

    let idempotency_source = match value.kind.as_str() {
        "resume" => {
            let resume_version_id = value
                .resume_version_id
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("resume evidence needs a resume version"))?;
            if application.resume_version_id.as_deref() != Some(resume_version_id) {
                anyhow::bail!("resume evidence does not match this application's resume")
            }
            let resume = get_resume_version(pool, account_id, resume_version_id)?
                .ok_or_else(|| anyhow::anyhow!("resume version not found"))?;
            if resume.job_id != application.job_id {
                anyhow::bail!("resume evidence belongs to another job")
            }
            validate_document_evidence(&value)?;
            format!("{}:{}", resume_version_id, value.sha256)
        }
        "cover_letter" | "attachment" => {
            validate_document_evidence(&value)?;
            format!("{}:{}", value.storage_key, value.sha256)
        }
        "application_receipt" => {
            let resume_version_id = value
                .resume_version_id
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("application receipt needs a resume version"))?;
            if application.resume_version_id.as_deref() != Some(resume_version_id) {
                anyhow::bail!("application receipt does not match this application's resume")
            }
            validate_application_receipt_evidence(&value)?;
            let receipt_id = value
                .metadata
                .get("receipt_id")
                .and_then(Value::as_str)
                .expect("application receipt validation checks receipt_id");
            format!("{}:{}", receipt_id, value.sha256)
        }
        "status_email" | "interview_event" => {
            if value.provider.trim().is_empty() {
                anyhow::bail!("provider evidence needs a provider")
            }
            let external_id = value
                .metadata
                .get("external_id")
                .and_then(Value::as_str)
                .filter(|item| !item.trim().is_empty())
                .ok_or_else(|| anyhow::anyhow!("provider evidence needs an external event id"))?;
            format!("{}:{}", value.provider, external_id)
        }
        "submission_confirmation" => {
            let confirmation = value
                .metadata
                .get("confirmation")
                .and_then(Value::as_str)
                .unwrap_or(value.label.as_str())
                .trim();
            if confirmation.is_empty() {
                anyhow::bail!("submission confirmation cannot be empty")
            }
            value
                .metadata
                .get("external_id")
                .and_then(Value::as_str)
                .unwrap_or(confirmation)
                .to_string()
        }
        _ => unreachable!(),
    };
    let provider_event_hash = private_lookup_hash(
        &format!("application-evidence:{}:{}", value.kind, value.provider),
        &format!("{}:{idempotency_source}", value.application_id),
    )?;
    let payload = to_json(&value, "application evidence")?;

    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get()?;
            conn.execute(
                "INSERT INTO jobs_application_evidence (
                    id, account_id, application_id, kind, provider_event_hash,
                    evidence_json, occurred_at_ms, created_at_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                 ON CONFLICT(account_id, provider_event_hash) DO NOTHING",
                params![
                    value.id,
                    account_id,
                    value.application_id,
                    value.kind,
                    provider_event_hash,
                    payload,
                    value.occurred_at_ms,
                    value.created_at_ms,
                ],
            )?;
            let raw: String = conn.query_row(
                "SELECT evidence_json FROM jobs_application_evidence
                  WHERE account_id = ?1 AND provider_event_hash = ?2",
                params![account_id, provider_event_hash],
                |row| row.get(0),
            )?;
            parse_json(raw, "application evidence")
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            conn.execute(
                "INSERT INTO jobs_application_evidence (
                    id, account_id, application_id, kind, provider_event_hash,
                    evidence_json, occurred_at_ms, created_at_ms
                 ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
                 ON CONFLICT(account_id, provider_event_hash) DO NOTHING",
                &[
                    &value.id,
                    &account_id,
                    &value.application_id,
                    &value.kind,
                    &provider_event_hash,
                    &payload,
                    &value.occurred_at_ms,
                    &value.created_at_ms,
                ],
            )?;
            let row = conn.query_one(
                "SELECT evidence_json FROM jobs_application_evidence
                  WHERE account_id = $1 AND provider_event_hash = $2",
                &[&account_id, &provider_event_hash],
            )?;
            parse_json(row.get(0), "application evidence")
        }
    })
}

fn validate_document_evidence(evidence: &ApplicationEvidence) -> Result<()> {
    if evidence.file_name.trim().is_empty() || evidence.storage_key.trim().is_empty() {
        anyhow::bail!("document evidence needs the attached file name and storage key")
    }
    if evidence.sha256.len() != 64 || !evidence.sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        anyhow::bail!("document evidence needs a SHA-256 checksum")
    }
    Ok(())
}

fn validate_application_receipt_evidence(evidence: &ApplicationEvidence) -> Result<()> {
    validate_document_evidence(evidence)?;
    if evidence.media_type != "application/json" || !evidence.file_name.ends_with(".json") {
        anyhow::bail!("application receipt needs a JSON evidence object")
    }
    if evidence
        .metadata
        .get("receipt_id")
        .and_then(Value::as_str)
        .is_none_or(|receipt_id| receipt_id.trim().is_empty())
        || evidence
            .metadata
            .get("schema_version")
            .and_then(Value::as_i64)
            != Some(1)
        || evidence.metadata.get("immutable").and_then(Value::as_bool) != Some(true)
        || evidence
            .metadata
            .get("size_bytes")
            .and_then(Value::as_i64)
            .is_none_or(|size_bytes| size_bytes <= 0)
    {
        anyhow::bail!("application receipt metadata is incomplete")
    }
    Ok(())
}

struct PreparedSubmissionEvidence {
    value: ApplicationEvidence,
    provider_event_hash: String,
    payload: String,
}

const MAX_SUBMISSION_CONFIRMATIONS: usize = 4;

fn prepare_submission_evidence(
    application_id: &str,
    request_fingerprint: &str,
    evidence: &[ApplicationEvidence],
    now: i64,
) -> Result<Vec<PreparedSubmissionEvidence>> {
    if evidence.is_empty() || evidence.len() > 14 {
        anyhow::bail!("invalid final submission evidence")
    }
    let mut resume_count = 0usize;
    let mut confirmation_count = 0usize;
    let mut receipt_count = 0usize;
    let mut prepared = Vec::with_capacity(evidence.len());
    for (index, item) in evidence.iter().enumerate() {
        let mut value = item.clone();
        if value.application_id != application_id
            || !matches!(
                value.kind.as_str(),
                "resume"
                    | "cover_letter"
                    | "attachment"
                    | "application_receipt"
                    | "submission_confirmation"
            )
        {
            anyhow::bail!("invalid final submission evidence")
        }
        resume_count += usize::from(value.kind == "resume");
        confirmation_count += usize::from(value.kind == "submission_confirmation");
        receipt_count += usize::from(value.kind == "application_receipt");
        if value.kind == "application_receipt" {
            validate_application_receipt_evidence(&value)?;
        } else {
            validate_document_evidence(&value)?;
        }
        if value.kind == "submission_confirmation"
            && value
                .metadata
                .get("confirmation")
                .and_then(Value::as_str)
                .is_none_or(|confirmation| confirmation.trim().is_empty())
        {
            anyhow::bail!("submission confirmation cannot be empty")
        }
        if value.id.is_empty() {
            value.id = uuid::Uuid::new_v4().to_string();
        }
        if value.occurred_at_ms == 0 {
            value.occurred_at_ms = now;
        }
        if value.created_at_ms == 0 {
            value.created_at_ms = now;
        }
        if value.metadata.is_null() {
            value.metadata = json!({});
        }
        let provider_event_hash = private_lookup_hash(
            "jobs-final-submission-evidence",
            &format!(
                "{application_id}:{request_fingerprint}:{index}:{}",
                value.kind
            ),
        )?;
        let payload = to_json(&value, "application evidence")?;
        prepared.push(PreparedSubmissionEvidence {
            value,
            provider_event_hash,
            payload,
        });
    }
    if resume_count != 1
        || !(1..=MAX_SUBMISSION_CONFIRMATIONS).contains(&confirmation_count)
        || receipt_count != 1
    {
        anyhow::bail!(
            "final submission needs one resume, one to four confirmations, and one receipt"
        )
    }
    Ok(prepared)
}

fn validate_submission_evidence_resume_bindings(
    evidence: &[PreparedSubmissionEvidence],
    resume_version_id: &str,
) -> Result<()> {
    for (kind, message) in [
        (
            "resume",
            "final receipt resume does not match the application",
        ),
        (
            "application_receipt",
            "final receipt bundle does not match the application resume",
        ),
        (
            "submission_confirmation",
            "final receipt confirmation does not match the application resume",
        ),
    ] {
        if evidence.iter().any(|item| {
            item.value.kind == kind
                && item.value.resume_version_id.as_deref() != Some(resume_version_id)
        }) {
            anyhow::bail!(message)
        }
        if !evidence.iter().any(|item| {
            item.value.kind == kind
                && item.value.resume_version_id.as_deref() == Some(resume_version_id)
        }) {
            anyhow::bail!(message)
        }
    }
    Ok(())
}

#[cfg(test)]
mod submission_evidence_resume_binding_tests {
    use super::*;

    fn prepared(kind: &str, resume_version_id: Option<&str>) -> PreparedSubmissionEvidence {
        PreparedSubmissionEvidence {
            value: ApplicationEvidence {
                id: format!("evidence-{kind}"),
                application_id: "application-1".to_string(),
                kind: kind.to_string(),
                label: kind.to_string(),
                provider: "test".to_string(),
                file_name: format!("{kind}.bin"),
                media_type: "application/octet-stream".to_string(),
                storage_key: format!("objects/{kind}"),
                sha256: "a".repeat(64),
                resume_version_id: resume_version_id.map(str::to_string),
                occurred_at_ms: 1,
                metadata: json!({}),
                created_at_ms: 1,
            },
            provider_event_hash: format!("hash-{kind}"),
            payload: "{}".to_string(),
        }
    }

    #[test]
    fn submission_confirmation_requires_the_exact_application_resume_revision() {
        let mut evidence = vec![
            prepared("resume", Some("resume-exact")),
            prepared("application_receipt", Some("resume-exact")),
            prepared("submission_confirmation", Some("resume-exact")),
        ];
        validate_submission_evidence_resume_bindings(&evidence, "resume-exact").unwrap();

        evidence[2].value.resume_version_id = Some("resume-stale".to_string());
        let error = validate_submission_evidence_resume_bindings(&evidence, "resume-exact")
            .expect_err("confirmation from a stale resume revision must fail closed");
        assert!(error
            .to_string()
            .contains("confirmation does not match the application resume"));
    }
}

#[derive(Debug, Clone)]
struct SubmissionManifestObject {
    kind: String,
    sha256: String,
    media_type: String,
    size_bytes: i64,
}

fn submission_evidence_manifest(
    receipt: &Value,
) -> Result<BTreeMap<String, SubmissionManifestObject>> {
    let values = receipt
        .get("evidenceObjects")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow::anyhow!("final receipt evidence manifest is missing"))?;
    if values.is_empty() || values.len() > 12 {
        anyhow::bail!("final receipt evidence manifest has an invalid object count")
    }
    let mut manifest = BTreeMap::new();
    let mut previous_key: Option<&str> = None;
    for value in values {
        let key = value
            .get("storageKey")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| anyhow::anyhow!("final receipt evidence manifest key is invalid"))?;
        let kind = value
            .get("kind")
            .and_then(Value::as_str)
            .filter(|value| {
                matches!(
                    *value,
                    "resume" | "cover_letter" | "attachment" | "screenshot"
                )
            })
            .ok_or_else(|| anyhow::anyhow!("final receipt evidence manifest kind is invalid"))?;
        let sha256 = value
            .get("sha256")
            .and_then(Value::as_str)
            .filter(|value| value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()))
            .ok_or_else(|| {
                anyhow::anyhow!("final receipt evidence manifest checksum is invalid")
            })?;
        let media_type = value
            .get("mediaType")
            .and_then(Value::as_str)
            .filter(|value| matches!(*value, "application/pdf" | "image/png"))
            .ok_or_else(|| anyhow::anyhow!("final receipt evidence manifest media is invalid"))?;
        let size_bytes = value
            .get("sizeBytes")
            .and_then(Value::as_i64)
            .filter(|value| *value > 0)
            .ok_or_else(|| anyhow::anyhow!("final receipt evidence manifest size is invalid"))?;
        if previous_key.is_some_and(|previous| previous >= key)
            || (kind == "screenshot") != (media_type == "image/png")
            || manifest
                .insert(
                    key.to_string(),
                    SubmissionManifestObject {
                        kind: kind.to_string(),
                        sha256: sha256.to_ascii_lowercase(),
                        media_type: media_type.to_string(),
                        size_bytes,
                    },
                )
                .is_some()
        {
            anyhow::bail!("final receipt evidence manifest is not canonical")
        }
        previous_key = Some(key);
    }
    Ok(manifest)
}

fn validate_submission_receipt_evidence(
    receipt: &Value,
    evidence: &[PreparedSubmissionEvidence],
) -> Result<()> {
    let receipt_id = receipt
        .get("receiptId")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| anyhow::anyhow!("final receipt id is missing"))?;
    let receipt_object = receipt
        .get("receiptObject")
        .and_then(Value::as_object)
        .ok_or_else(|| anyhow::anyhow!("final receipt object is missing"))?;
    let storage_key = receipt_object
        .get("storageKey")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| anyhow::anyhow!("final receipt object key is missing"))?;
    let sha256 = receipt_object
        .get("sha256")
        .and_then(Value::as_str)
        .filter(|value| value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .ok_or_else(|| anyhow::anyhow!("final receipt object checksum is invalid"))?;
    let size_bytes = receipt_object
        .get("sizeBytes")
        .and_then(Value::as_i64)
        .filter(|value| *value > 0)
        .ok_or_else(|| anyhow::anyhow!("final receipt object size is invalid"))?;
    if receipt_object.get("mediaType").and_then(Value::as_str) != Some("application/json")
        || receipt_object.get("schemaVersion").and_then(Value::as_i64) != Some(1)
    {
        anyhow::bail!("final receipt object metadata is invalid")
    }
    let stored_receipt = evidence
        .iter()
        .find(|item| item.value.kind == "application_receipt")
        .ok_or_else(|| anyhow::anyhow!("final receipt evidence is missing"))?;
    if stored_receipt.value.storage_key != storage_key
        || stored_receipt.value.sha256 != sha256
        || stored_receipt.value.media_type != "application/json"
        || stored_receipt
            .value
            .metadata
            .get("receipt_id")
            .and_then(Value::as_str)
            != Some(receipt_id)
        || stored_receipt
            .value
            .metadata
            .get("schema_version")
            .and_then(Value::as_i64)
            != Some(1)
        || stored_receipt
            .value
            .metadata
            .get("size_bytes")
            .and_then(Value::as_i64)
            != Some(size_bytes)
    {
        anyhow::bail!("final receipt evidence does not match its immutable object")
    }
    let screenshots = receipt
        .get("screenshotKeys")
        .and_then(Value::as_array)
        .filter(|items| !items.is_empty() && items.len() <= MAX_SUBMISSION_CONFIRMATIONS)
        .ok_or_else(|| anyhow::anyhow!("final receipt confirmation is missing"))?;
    let mut screenshot_keys = Vec::with_capacity(screenshots.len());
    let mut unique_screenshot_keys = BTreeSet::new();
    for screenshot in screenshots {
        let key = screenshot
            .as_str()
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| anyhow::anyhow!("final receipt confirmation key is invalid"))?;
        if !unique_screenshot_keys.insert(key) {
            anyhow::bail!("final receipt confirmation keys are duplicated")
        }
        screenshot_keys.push(key);
    }
    let confirmations = evidence
        .iter()
        .filter(|item| item.value.kind == "submission_confirmation")
        .collect::<Vec<_>>();
    if confirmations.len() != screenshot_keys.len() {
        anyhow::bail!("final receipt confirmation evidence count is incomplete")
    }
    let mut seen_indexes = BTreeSet::new();
    let mut seen_storage_keys = BTreeSet::new();
    let mut seen_file_names = BTreeSet::new();
    for confirmation in confirmations {
        let value = &confirmation.value;
        let index = value
            .metadata
            .get("screenshot_index")
            .and_then(Value::as_u64)
            .and_then(|index| usize::try_from(index).ok())
            .filter(|index| (1..=screenshot_keys.len()).contains(index))
            .ok_or_else(|| anyhow::anyhow!("final receipt confirmation index is invalid"))?;
        let count = value
            .metadata
            .get("screenshot_count")
            .and_then(Value::as_u64)
            .and_then(|count| usize::try_from(count).ok())
            .filter(|count| *count == screenshot_keys.len())
            .ok_or_else(|| anyhow::anyhow!("final receipt confirmation count is invalid"))?;
        let evidence_screenshot_keys = value
            .metadata
            .get("screenshot_keys")
            .and_then(Value::as_array)
            .filter(|keys| {
                keys.len() == screenshot_keys.len()
                    && keys
                        .iter()
                        .zip(&screenshot_keys)
                        .all(|(actual, expected)| actual.as_str() == Some(*expected))
            })
            .ok_or_else(|| {
                anyhow::anyhow!("final receipt confirmation screenshot set is invalid")
            })?;
        debug_assert_eq!(evidence_screenshot_keys.len(), count);
        if value.storage_key != screenshot_keys[index - 1]
            || value.media_type != "image/png"
            || !value.file_name.to_ascii_lowercase().ends_with(".png")
            || value.metadata.get("immutable").and_then(Value::as_bool) != Some(true)
            || value
                .metadata
                .get("evidence_strength")
                .and_then(Value::as_str)
                != Some("browser_confirmed")
            || value.metadata.get("receipt_id").and_then(Value::as_str) != Some(receipt_id)
            || value
                .metadata
                .get("size_bytes")
                .and_then(Value::as_i64)
                .is_none_or(|size| size <= 0)
            || !seen_indexes.insert(index)
            || !seen_storage_keys.insert(value.storage_key.as_str())
            || !seen_file_names.insert(value.file_name.as_str())
        {
            anyhow::bail!("final receipt confirmation does not match its immutable object")
        }
    }
    for document in receipt
        .get("documents")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow::anyhow!("final receipt documents are missing"))?
    {
        let kind = document
            .get("kind")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let key = document
            .get("storageKey")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let hash = document
            .get("sha256")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if !evidence.iter().any(|item| {
            item.value.kind == kind && item.value.storage_key == key && item.value.sha256 == hash
        }) {
            anyhow::bail!("final receipt document does not match its immutable evidence")
        }
    }
    Ok(())
}

fn validate_submission_object_bindings(
    receipt: &Value,
    evidence: &[PreparedSubmissionEvidence],
    bindings: &[crate::db::object_uploads::ApplicationObjectBinding],
) -> Result<()> {
    let mut expected = submission_evidence_manifest(receipt)?;
    let mut referenced = BTreeSet::new();
    let documents = receipt
        .get("documents")
        .and_then(Value::as_array)
        .filter(|documents| !documents.is_empty() && documents.len() <= 12)
        .ok_or_else(|| anyhow::anyhow!("final receipt documents are missing"))?;
    let mut resume_count = 0usize;
    for document in documents {
        let kind = document
            .get("kind")
            .and_then(Value::as_str)
            .filter(|kind| matches!(*kind, "resume" | "cover_letter" | "attachment"))
            .ok_or_else(|| anyhow::anyhow!("final receipt document kind is invalid"))?;
        let key = document
            .get("storageKey")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| anyhow::anyhow!("final receipt document key is invalid"))?;
        let sha256 = document
            .get("sha256")
            .and_then(Value::as_str)
            .filter(|value| value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()))
            .ok_or_else(|| anyhow::anyhow!("final receipt document checksum is invalid"))?;
        resume_count += usize::from(kind == "resume");
        let Some(object) = expected.get(key) else {
            anyhow::bail!("final receipt document is missing from its evidence manifest")
        };
        if !referenced.insert(key)
            || object.kind != kind
            || object.media_type != "application/pdf"
            || !object.sha256.eq_ignore_ascii_case(sha256)
        {
            anyhow::bail!("final receipt document does not match its evidence manifest")
        }
    }
    if resume_count != 1 {
        anyhow::bail!("final receipt needs exactly one submitted resume")
    }
    let screenshots = receipt
        .get("screenshotKeys")
        .and_then(Value::as_array)
        .filter(|screenshots| {
            !screenshots.is_empty() && screenshots.len() <= MAX_SUBMISSION_CONFIRMATIONS
        })
        .ok_or_else(|| anyhow::anyhow!("final receipt screenshots are missing"))?;
    for screenshot in screenshots {
        let key = screenshot
            .as_str()
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| anyhow::anyhow!("final receipt screenshot key is invalid"))?;
        let Some(object) = expected.get(key) else {
            anyhow::bail!("final receipt screenshot is missing from its evidence manifest")
        };
        if !referenced.insert(key)
            || object.kind != "screenshot"
            || object.media_type != "image/png"
        {
            anyhow::bail!("final receipt screenshot does not match its evidence manifest")
        }
    }
    if referenced.len() != expected.len() {
        anyhow::bail!("final receipt evidence manifest has unreferenced objects")
    }
    let receipt_object = receipt
        .get("receiptObject")
        .and_then(Value::as_object)
        .ok_or_else(|| anyhow::anyhow!("final receipt object is missing"))?;
    let receipt_key = receipt_object
        .get("storageKey")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let receipt_sha256 = receipt_object
        .get("sha256")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let receipt_size = receipt_object
        .get("sizeBytes")
        .and_then(Value::as_i64)
        .unwrap_or_default();
    if receipt_key.is_empty()
        || expected
            .insert(
                receipt_key.to_string(),
                SubmissionManifestObject {
                    kind: "application_receipt".to_string(),
                    sha256: receipt_sha256.to_ascii_lowercase(),
                    media_type: "application/json".to_string(),
                    size_bytes: receipt_size,
                },
            )
            .is_some()
        || bindings.len() != expected.len()
    {
        anyhow::bail!("final receipt object bindings are incomplete")
    }
    let mut seen_uploads = BTreeSet::new();
    let mut seen_binding_keys = BTreeSet::new();
    for binding in bindings {
        let Some(object) = expected.get(&binding.object_key) else {
            anyhow::bail!("final receipt has an unreferenced durable object")
        };
        if !seen_uploads.insert(binding.upload_id.as_str())
            || !seen_binding_keys.insert(binding.object_key.as_str())
            || binding.content_type != object.media_type
            || binding.size_bytes != object.size_bytes
            || !binding.sha256.eq_ignore_ascii_case(&object.sha256)
        {
            anyhow::bail!("final receipt durable object binding is invalid")
        }
    }
    if seen_binding_keys.len() != expected.len()
        || expected
            .keys()
            .any(|key| !seen_binding_keys.contains(key.as_str()))
    {
        anyhow::bail!("final receipt durable object bindings are incomplete")
    }
    let mut expected_evidence_keys = documents
        .iter()
        .filter_map(|document| document.get("storageKey").and_then(Value::as_str))
        .collect::<BTreeSet<_>>();
    expected_evidence_keys.insert(receipt_key);
    expected_evidence_keys.extend(screenshots.iter().filter_map(Value::as_str));
    let mut seen_evidence = BTreeSet::new();
    for item in evidence {
        let Some(object) = expected.get(&item.value.storage_key) else {
            anyhow::bail!("final submission evidence is not durably tracked")
        };
        let expected_kind = if item.value.kind == "submission_confirmation" {
            "screenshot"
        } else {
            item.value.kind.as_str()
        };
        if !seen_evidence.insert(item.value.storage_key.as_str())
            || object.kind != expected_kind
            || item.value.media_type != object.media_type
            || !item.value.sha256.eq_ignore_ascii_case(&object.sha256)
            || item
                .value
                .metadata
                .get("size_bytes")
                .and_then(Value::as_i64)
                != Some(object.size_bytes)
        {
            anyhow::bail!("final submission evidence does not match its durable object")
        }
    }
    if seen_evidence != expected_evidence_keys {
        anyhow::bail!("final submission evidence records are incomplete")
    }
    Ok(())
}

struct ApprovedSubmissionSnapshot<'a> {
    packet: &'a Value,
    job: &'a Value,
    checksum: &'a str,
}

fn canonical_submission_json(value: &Value) -> Value {
    match value {
        Value::Array(values) => {
            Value::Array(values.iter().map(canonical_submission_json).collect())
        }
        Value::Object(values) => Value::Object(
            values
                .iter()
                .map(|(key, value)| (key.clone(), canonical_submission_json(value)))
                .collect::<BTreeMap<_, _>>()
                .into_iter()
                .collect(),
        ),
        other => other.clone(),
    }
}

fn approved_submission_checksum(
    schema_version: i64,
    packet: &Value,
    job: &Value,
    admission: Option<&Value>,
) -> Result<String> {
    let value = match (schema_version, admission) {
        (1, None) => json!({
            "schema_version": 1,
            "packet": packet,
            "job": job,
        }),
        (schema_version @ (2 | 3), Some(admission)) => json!({
            "schema_version": schema_version,
            "admission": admission,
            "packet": packet,
            "job": job,
        }),
        _ => anyhow::bail!("approved execution checksum inputs are invalid"),
    };
    let bytes = serde_json::to_vec(&canonical_submission_json(&value))?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

fn validate_approved_submission_admission(
    application: &JobApplication,
    schema_version: i64,
    admission: Option<&Value>,
) -> Result<()> {
    if schema_version == 1 {
        if application.submission_mode == "auto_submit" {
            anyhow::bail!("legacy approval cannot grant Auto-submit authority")
        }
        return Ok(());
    }
    let admission = admission
        .and_then(Value::as_object)
        .ok_or_else(|| anyhow::anyhow!("approved execution admission is missing"))?;
    let kind = admission
        .get("kind")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if application.submission_mode == "auto_submit" {
        let complete = kind == "track_auto_submit"
            && approved_submission_object_has_keys(
                admission,
                if schema_version == 3 {
                    &[
                        "ats_certification",
                        "authority_fingerprint",
                        "authorization_id",
                        "career_track_id",
                        "kind",
                        "revision_no",
                    ]
                } else {
                    &[
                        "authority_fingerprint",
                        "authorization_id",
                        "career_track_id",
                        "kind",
                        "revision_no",
                    ]
                },
            )
            && admission
                .get("authorization_id")
                .and_then(Value::as_str)
                .is_some_and(valid_approved_submission_identifier)
            && admission
                .get("career_track_id")
                .and_then(Value::as_str)
                .is_some_and(valid_approved_submission_identifier)
            && admission
                .get("revision_no")
                .and_then(Value::as_i64)
                .is_some_and(|value| value > 0)
            && admission
                .get("authority_fingerprint")
                .and_then(Value::as_str)
                .is_some_and(valid_approved_submission_sha256)
            && (schema_version != 3
                || admission
                    .get("ats_certification")
                    .is_some_and(valid_approved_submission_ats_certification));
        if !complete {
            anyhow::bail!("approved Auto-submit admission is incomplete")
        }
    } else if kind != "review_approval"
        || !approved_submission_object_has_keys(admission, &["kind"])
    {
        anyhow::bail!("approved execution does not contain review authority")
    }
    Ok(())
}

fn valid_approved_submission_ats_certification(value: &Value) -> bool {
    let Some(certification) = value.as_object() else {
        return false;
    };
    if !approved_submission_object_has_keys(
        certification,
        &[
            "activation_generation",
            "activation_sha256",
            "adapter_bundle_sha256",
            "adapter_version",
            "expires_at_ms",
            "layout_contract_version",
            "layout_set_sha256",
            "manifest_sha256",
            "provider",
            "runner_target_sha256s",
            "schema_version",
            "surface_sha256",
            "target_key_sha256",
            "variant_key",
        ],
    ) || certification.get("schema_version").and_then(Value::as_i64) != Some(1)
    {
        return false;
    }
    let provider = certification
        .get("provider")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let adapter_version = certification
        .get("adapter_version")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if !matches!(
        (provider, adapter_version),
        ("greenhouse", "2026.07.1-beta.1") | ("lever", "2026.07.0-beta.1")
    ) || certification
        .get("variant_key")
        .and_then(Value::as_str)
        .is_none_or(|value| !valid_approved_submission_identifier(value))
        || certification
            .get("layout_contract_version")
            .and_then(Value::as_i64)
            .is_none_or(|value| value <= 0)
        || certification
            .get("activation_generation")
            .and_then(Value::as_i64)
            .is_none_or(|value| value <= 0)
        || certification
            .get("expires_at_ms")
            .and_then(Value::as_i64)
            .is_none_or(|value| value <= 0)
    {
        return false;
    }
    for key in [
        "activation_sha256",
        "adapter_bundle_sha256",
        "layout_set_sha256",
        "manifest_sha256",
        "surface_sha256",
        "target_key_sha256",
    ] {
        if !certification
            .get(key)
            .and_then(Value::as_str)
            .is_some_and(valid_approved_submission_sha256)
        {
            return false;
        }
    }
    let Some(targets) = certification
        .get("runner_target_sha256s")
        .and_then(Value::as_array)
    else {
        return false;
    };
    if targets.is_empty() || targets.len() > 2 {
        return false;
    }
    let mut previous: Option<&str> = None;
    for target in targets {
        let Some(target) = target.as_str().filter(|value| {
            valid_approved_submission_sha256(value)
                && previous.is_none_or(|previous| previous < *value)
        }) else {
            return false;
        };
        previous = Some(target);
    }
    true
}

fn approved_submission_object_has_keys(
    value: &serde_json::Map<String, Value>,
    expected: &[&str],
) -> bool {
    value.len() == expected.len() && expected.iter().all(|key| value.contains_key(*key))
}

fn valid_approved_submission_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 240
        && value.trim() == value
        && !value.bytes().any(|byte| byte.is_ascii_control())
}

fn valid_approved_submission_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn validate_canonical_claim_ids(value: Option<&Value>) -> Result<()> {
    let claims = value
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow::anyhow!("approved execution verified claims are missing"))?;
    let mut previous: Option<&str> = None;
    for claim in claims {
        let claim = claim
            .as_str()
            .filter(|claim| !claim.trim().is_empty())
            .ok_or_else(|| anyhow::anyhow!("approved execution verified claims are invalid"))?;
        if previous.is_some_and(|previous| previous >= claim) {
            anyhow::bail!("approved execution verified claims are not canonical")
        }
        previous = Some(claim);
    }
    Ok(())
}

fn approved_submission_snapshot<'a>(
    account_id: &str,
    application: &'a JobApplication,
) -> Result<ApprovedSubmissionSnapshot<'a>> {
    let approved = application
        .receipt
        .get("approved_execution")
        .and_then(Value::as_object)
        .ok_or_else(|| anyhow::anyhow!("approved execution snapshot is missing"))?;
    let schema_version = approved
        .get("schema_version")
        .and_then(Value::as_i64)
        .unwrap_or_default();
    if !matches!(schema_version, 1..=3) {
        anyhow::bail!("approved execution schema is unsupported")
    }
    let packet = approved
        .get("packet")
        .filter(|value| value.is_object())
        .ok_or_else(|| anyhow::anyhow!("approved execution packet is missing"))?;
    let job = approved
        .get("job")
        .filter(|value| value.is_object())
        .ok_or_else(|| anyhow::anyhow!("approved execution job is missing"))?;
    let checksum = approved
        .get("checksum")
        .and_then(Value::as_str)
        .filter(|value| value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .ok_or_else(|| anyhow::anyhow!("approved execution checksum is invalid"))?;
    let admission = approved.get("admission");
    validate_approved_submission_admission(application, schema_version, admission)?;
    let expected_checksum = approved_submission_checksum(
        schema_version,
        packet,
        job,
        matches!(schema_version, 2 | 3).then_some(
            admission.ok_or_else(|| anyhow::anyhow!("approved execution admission is missing"))?,
        ),
    )?;
    if !constant_time_equal(checksum, &expected_checksum) {
        anyhow::bail!("approved execution checksum does not match its exact packet")
    }

    let resume_version_id = application
        .resume_version_id
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("application resume version is missing"))?;
    let identity_id = application
        .receipt
        .pointer("/application_identity/id")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| anyhow::anyhow!("application identity binding is missing"))?;
    let identity_email = application
        .receipt
        .pointer("/application_identity/email")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| anyhow::anyhow!("application email binding is missing"))?;
    let expected_browser_profile_id = execution_browser_profile_id(account_id, identity_id);
    if packet.get("applicationId").and_then(Value::as_str) != Some(application.id.as_str())
        || packet.get("jobId").and_then(Value::as_str) != Some(application.job_id.as_str())
        || packet.get("resumeVersionId").and_then(Value::as_str) != Some(resume_version_id)
        || packet.get("applicationIdentityId").and_then(Value::as_str) != Some(identity_id)
        || packet.get("applicationEmail").and_then(Value::as_str) != Some(identity_email)
        || packet.get("browserProfileId").and_then(Value::as_str)
            != Some(expected_browser_profile_id.as_str())
        || packet.get("answers").is_none_or(|value| !value.is_object())
    {
        anyhow::bail!("approved execution is not bound to this application")
    }
    validate_canonical_claim_ids(packet.get("verifiedClaimIds"))?;

    Ok(ApprovedSubmissionSnapshot {
        packet,
        job,
        checksum,
    })
}

fn validate_submission_authority_snapshot(
    account_id: &str,
    application: &JobApplication,
    run_id: &str,
    receipt: &Value,
    runner: &str,
) -> Result<()> {
    let authority = receipt
        .get(SERVER_SUBMISSION_AUTHORITY_KEY)
        .and_then(Value::as_object)
        .ok_or_else(|| anyhow::anyhow!("final receipt submission authority is missing"))?;
    let execution = authority
        .get("executionAuthority")
        .and_then(Value::as_object)
        .ok_or_else(|| anyhow::anyhow!("final receipt execution authority is missing"))?;
    let approved = approved_submission_snapshot(account_id, application)?;
    let receipt_packet = receipt
        .get("packet")
        .and_then(Value::as_object)
        .ok_or_else(|| anyhow::anyhow!("final receipt approved packet is missing"))?;
    let identity_id = approved
        .packet
        .get("applicationIdentityId")
        .and_then(Value::as_str)
        .expect("approved packet identity checked above");
    let browser_profile_id = approved
        .packet
        .get("browserProfileId")
        .and_then(Value::as_str)
        .expect("approved packet browser profile checked above");
    let resume_version_id = approved
        .packet
        .get("resumeVersionId")
        .and_then(Value::as_str)
        .expect("approved packet resume checked above");
    let submitted_resume =
        receipt
            .get("documents")
            .and_then(Value::as_array)
            .and_then(|documents| {
                let mut resumes = documents.iter().filter(|document| {
                    document.get("kind").and_then(Value::as_str) == Some("resume")
                });
                let resume = resumes.next()?;
                resumes.next().is_none().then_some(resume)
            });
    let confirmation = receipt
        .get("result")
        .and_then(Value::as_object)
        .filter(|result| result.get("status").and_then(Value::as_str) == Some("submitted"))
        .filter(|result| {
            result
                .get("confirmationText")
                .and_then(Value::as_str)
                .is_some_and(|value| !value.trim().is_empty())
                || result
                    .get("confirmationUrl")
                    .and_then(Value::as_str)
                    .is_some_and(|value| !value.trim().is_empty())
        });
    if receipt.get("schemaVersion").and_then(Value::as_i64) != Some(1)
        || receipt.get("accountId").and_then(Value::as_str) != Some(account_id)
        || receipt.get("applicationId").and_then(Value::as_str) != Some(application.id.as_str())
        || receipt.get("runId").and_then(Value::as_str) != Some(run_id)
        || receipt.get("runner").and_then(Value::as_str) != Some(runner)
        || receipt.get("applicationIdentityId").and_then(Value::as_str) != Some(identity_id)
        || receipt.get("browserProfileId").and_then(Value::as_str) != Some(browser_profile_id)
        || receipt
            .get("adapter")
            .and_then(Value::as_str)
            .is_none_or(|value| value.trim().is_empty())
        || receipt
            .get("adapterVersion")
            .and_then(Value::as_str)
            .is_none_or(|value| value.trim().is_empty())
        || confirmation.is_none()
        || receipt_packet.get("jobId") != approved.packet.get("jobId")
        || receipt_packet.get("resumeVersionId") != approved.packet.get("resumeVersionId")
        || receipt_packet.get("applicationEmail") != approved.packet.get("applicationEmail")
        || receipt_packet
            .get("approvedPacketChecksum")
            .and_then(Value::as_str)
            != Some(approved.checksum)
        || receipt_packet.get("answers") != approved.packet.get("answers")
        || receipt_packet.get("verifiedClaimIds") != approved.packet.get("verifiedClaimIds")
        || receipt.get("job") != Some(approved.job)
        || submitted_resume
            .and_then(|resume| resume.get("versionId"))
            .and_then(Value::as_str)
            != Some(resume_version_id)
        || authority.get("schemaVersion").and_then(Value::as_i64) != Some(1)
        || authority.get("preSubmissionReceipt") != Some(&application.receipt)
        || match runner {
            "cloud" => {
                execution.get("kind").and_then(Value::as_str) != Some("cloud_execution_lease")
            }
            "local" => execution.get("kind").and_then(Value::as_str) != Some("local_run_ticket"),
            _ => true,
        }
    {
        anyhow::bail!("final receipt submission authority does not match the approved packet")
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn validate_submission_execution_authority_sqlite_tx(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    application_id: &str,
    run_id: &str,
    runner: &str,
    receipt: &Value,
    local_ticket_hash: Option<&str>,
    now: i64,
) -> Result<()> {
    let execution = receipt
        .pointer(&format!(
            "/{SERVER_SUBMISSION_AUTHORITY_KEY}/executionAuthority"
        ))
        .and_then(Value::as_object)
        .ok_or_else(|| anyhow::anyhow!("final receipt execution authority is missing"))?;
    if runner == "cloud" {
        let stored = tx
            .query_row(
                "SELECT owner_id, lease_token_sha256, fence, phase, finished_at_ms
                   FROM jobs_execution_leases
                  WHERE account_id = ?1 AND application_id = ?2 AND run_id = ?3",
                params![account_id, application_id, run_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, Option<i64>>(4)?,
                    ))
                },
            )
            .optional()?;
        let Some((owner_id, token_sha256, fence, phase, finished_at_ms)) = stored else {
            anyhow::bail!("final receipt cloud execution authority is missing")
        };
        validate_cloud_submission_execution_authority(
            execution,
            &owner_id,
            &token_sha256,
            fence,
            &phase,
            finished_at_ms,
            now,
        )?;
    } else {
        let supplied_ticket_hash = local_ticket_hash
            .ok_or_else(|| anyhow::anyhow!("final receipt local authority is missing"))?;
        let stored = tx
            .query_row(
                "SELECT ticket_hash, status, expires_at_ms
                   FROM jobs_local_run_tickets
                  WHERE id = ?1 AND account_id = ?2 AND application_id = ?3",
                params![run_id, account_id, application_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, i64>(2)?,
                    ))
                },
            )
            .optional()?;
        let Some((stored_ticket_hash, status, expires_at_ms)) = stored else {
            anyhow::bail!("final receipt local execution authority is missing")
        };
        let active = (matches!(status.as_str(), "claimed" | "needs_input") && expires_at_ms > now)
            || (matches!(status.as_str(), "click_started" | "side_effect_unknown")
                && expires_at_ms.saturating_add(SUBMISSION_RECONCILIATION_GRACE_MS) > now);
        if execution
            .get("ticketHash")
            .and_then(Value::as_str)
            .is_none_or(|value| !constant_time_equal(value, &stored_ticket_hash))
            || !constant_time_equal(supplied_ticket_hash, &stored_ticket_hash)
            || execution.get("runId").and_then(Value::as_str) != Some(run_id)
            || !active
        {
            anyhow::bail!("final receipt local execution authority changed")
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn validate_submission_execution_authority_postgres_tx(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    application_id: &str,
    run_id: &str,
    runner: &str,
    receipt: &Value,
    local_ticket_hash: Option<&str>,
    now: i64,
) -> Result<()> {
    let execution = receipt
        .pointer(&format!(
            "/{SERVER_SUBMISSION_AUTHORITY_KEY}/executionAuthority"
        ))
        .and_then(Value::as_object)
        .ok_or_else(|| anyhow::anyhow!("final receipt execution authority is missing"))?;
    if runner == "cloud" {
        let stored = tx.query_opt(
            "SELECT owner_id, lease_token_sha256, fence, phase, finished_at_ms
               FROM jobs_execution_leases
              WHERE account_id = $1 AND application_id = $2 AND run_id = $3
              FOR UPDATE",
            &[&account_id, &application_id, &run_id],
        )?;
        let Some(stored) = stored else {
            anyhow::bail!("final receipt cloud execution authority is missing")
        };
        let owner_id: String = stored.try_get(0)?;
        let token_sha256: String = stored.try_get(1)?;
        let fence: i64 = stored.try_get(2)?;
        let phase: String = stored.try_get(3)?;
        let finished_at_ms: Option<i64> = stored.try_get(4)?;
        validate_cloud_submission_execution_authority(
            execution,
            &owner_id,
            &token_sha256,
            fence,
            &phase,
            finished_at_ms,
            now,
        )?;
    } else {
        let supplied_ticket_hash = local_ticket_hash
            .ok_or_else(|| anyhow::anyhow!("final receipt local authority is missing"))?;
        let stored = tx.query_opt(
            "SELECT ticket_hash, status, expires_at_ms
               FROM jobs_local_run_tickets
              WHERE id = $1 AND account_id = $2 AND application_id = $3
              FOR UPDATE",
            &[&run_id, &account_id, &application_id],
        )?;
        let Some(stored) = stored else {
            anyhow::bail!("final receipt local execution authority is missing")
        };
        let stored_ticket_hash: String = stored.try_get(0)?;
        let status: String = stored.try_get(1)?;
        let expires_at_ms: i64 = stored.try_get(2)?;
        let active = (matches!(status.as_str(), "claimed" | "needs_input") && expires_at_ms > now)
            || (matches!(status.as_str(), "click_started" | "side_effect_unknown")
                && expires_at_ms.saturating_add(SUBMISSION_RECONCILIATION_GRACE_MS) > now);
        if execution
            .get("ticketHash")
            .and_then(Value::as_str)
            .is_none_or(|value| !constant_time_equal(value, &stored_ticket_hash))
            || !constant_time_equal(supplied_ticket_hash, &stored_ticket_hash)
            || execution.get("runId").and_then(Value::as_str) != Some(run_id)
            || !active
        {
            anyhow::bail!("final receipt local execution authority changed")
        }
    }
    Ok(())
}

fn validate_cloud_submission_execution_authority(
    execution: &serde_json::Map<String, Value>,
    owner_id: &str,
    token_sha256: &str,
    fence: i64,
    phase: &str,
    finished_at_ms: Option<i64>,
    now: i64,
) -> Result<()> {
    let token_matches = execution
        .get("leaseTokenSha256")
        .and_then(Value::as_str)
        .is_some_and(|value| constant_time_equal(value, token_sha256));
    if !matches!(phase, "submitted" | "side_effect_unknown")
        || finished_at_ms.is_none()
        || (phase == "side_effect_unknown"
            && finished_at_ms.is_none_or(|finished_at_ms| {
                finished_at_ms.saturating_add(SUBMISSION_RECONCILIATION_GRACE_MS) <= now
            }))
        || execution.get("ownerId").and_then(Value::as_str) != Some(owner_id)
        || !token_matches
        || execution.get("fence").and_then(Value::as_i64) != Some(fence)
        || execution.get("phase").and_then(Value::as_str) != Some(phase)
    {
        anyhow::bail!("final receipt cloud execution authority changed")
    }
    Ok(())
}

#[derive(Debug, Error)]
#[error("final submission commit outcome is uncertain: {detail}")]
pub struct SubmissionCommitUncertain {
    detail: String,
}

fn submission_commit_uncertain(error: impl std::fmt::Display) -> anyhow::Error {
    SubmissionCommitUncertain {
        detail: error.to_string(),
    }
    .into()
}

#[allow(clippy::too_many_arguments)]
pub fn finalize_submission(
    pool: &DbPool,
    account_id: &str,
    application_id: &str,
    run_id: &str,
    runner: &str,
    receipt: Value,
    request_fingerprint: &str,
    evidence: &[ApplicationEvidence],
    object_uploads: &[crate::db::object_uploads::ApplicationObjectBinding],
    terminal_session: &BrowserSession,
    local_ticket_hash: Option<&str>,
) -> Result<SubmissionFinalizeResult> {
    if !matches!(runner, "cloud" | "local")
        || request_fingerprint.len() != 64
        || !request_fingerprint
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        anyhow::bail!("invalid final submission request")
    }
    if receipt
        .get("_bluey_server_submission_fingerprint_v1")
        .and_then(Value::as_str)
        != Some(request_fingerprint)
    {
        anyhow::bail!("invalid final submission fingerprint")
    }
    if (runner == "local") != local_ticket_hash.is_some() {
        anyhow::bail!("invalid final submission runner binding")
    }
    let now = now_ms();
    let prepared_evidence =
        prepare_submission_evidence(application_id, request_fingerprint, evidence, now)?;
    validate_submission_receipt_evidence(&receipt, &prepared_evidence)?;
    validate_submission_object_bindings(&receipt, &prepared_evidence, object_uploads)?;
    let mut terminal_session = terminal_session.clone();
    if terminal_session.application_id.as_deref() != Some(application_id)
        || terminal_session.id != run_id
        || terminal_session.runner != runner
    {
        anyhow::bail!("browser session does not match final submission")
    }
    terminal_session.status = "complete".to_string();
    terminal_session.current_step = "Application submitted".to_string();
    terminal_session.takeover_url = None;
    terminal_session.updated_at_ms = now;
    let terminal_session_payload = to_json(&terminal_session, "browser session")?;

    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            if runner == "cloud" {
                require_current_runner_volume_identity_sqlite_for_operation(
                    &tx, account_id, run_id, now,
                )?;
            }
            match crate::db::account_data::account_write_fence_sqlite_tx(&tx, account_id)? {
                crate::db::account_data::AccountWriteFence::Active => {}
                crate::db::account_data::AccountWriteFence::DeletionRequested => {
                    anyhow::bail!("account deletion has fenced final submission")
                }
                crate::db::account_data::AccountWriteFence::Missing => {
                    anyhow::bail!("account not found")
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
            if application.state == "submitted" {
                if application
                    .receipt
                    .get("_bluey_server_submission_fingerprint_v1")
                    .and_then(Value::as_str)
                    == Some(request_fingerprint)
                {
                    tx.commit().map_err(submission_commit_uncertain)?;
                    return Ok(SubmissionFinalizeResult::Replayed(application));
                }
                anyhow::bail!("application already has a different final receipt")
            }
            if application.run_id.as_deref() != Some(run_id) {
                anyhow::bail!("application browser run does not match final receipt")
            }
            validate_submission_authority_snapshot(
                account_id,
                &application,
                run_id,
                &receipt,
                runner,
            )?;
            validate_submission_execution_authority_sqlite_tx(
                &tx,
                account_id,
                application_id,
                run_id,
                runner,
                &receipt,
                local_ticket_hash,
                now,
            )?;
            validate_application_transition(&application.state, "submitted")?;
            let resume_version_id = application
                .resume_version_id
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("application resume version is missing"))?;
            validate_submission_evidence_resume_bindings(&prepared_evidence, resume_version_id)?;
            if runner == "cloud" {
                let phase: Option<String> = tx
                    .query_row(
                        "SELECT phase FROM jobs_execution_leases
                          WHERE account_id = ?1 AND application_id = ?2 AND run_id = ?3",
                        params![account_id, application_id, run_id],
                        |row| row.get(0),
                    )
                    .optional()?;
                match phase.as_deref() {
                    Some("submitted") => {}
                    Some("side_effect_unknown") => {
                        if tx.execute(
                            "UPDATE jobs_execution_leases
                                SET phase = 'submitted', updated_at_ms = ?4,
                                    finished_at_ms = ?4
                              WHERE account_id = ?1 AND application_id = ?2 AND run_id = ?3
                                AND phase = 'side_effect_unknown'",
                            params![account_id, application_id, run_id, now],
                        )? != 1
                        {
                            anyhow::bail!("matching cloud execution lease changed")
                        }
                    }
                    _ => anyhow::bail!("matching cloud execution lease cannot accept receipt"),
                }
            } else if tx.execute(
                "UPDATE jobs_local_run_tickets
                    SET status = 'complete', updated_at_ms = ?5
                  WHERE id = ?1 AND account_id = ?2 AND application_id = ?3
                    AND ticket_hash = ?4
                    AND (
                        (expires_at_ms > ?5
                            AND status IN ('claimed', 'needs_input'))
                        OR (status IN ('click_started', 'side_effect_unknown')
                            AND expires_at_ms + ?6 > ?5)
                    )",
                params![
                    run_id,
                    account_id,
                    application_id,
                    local_ticket_hash.expect("local ticket checked above"),
                    now,
                    SUBMISSION_RECONCILIATION_GRACE_MS,
                ],
            )? != 1
            {
                anyhow::bail!("local run ticket is not active")
            }
            for item in &prepared_evidence {
                tx.execute(
                    "INSERT INTO jobs_application_evidence (
                        id, account_id, application_id, kind, provider_event_hash,
                        evidence_json, occurred_at_ms, created_at_ms
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    params![
                        item.value.id,
                        account_id,
                        application_id,
                        item.value.kind,
                        item.provider_event_hash,
                        item.payload,
                        item.value.occurred_at_ms,
                        item.value.created_at_ms,
                    ],
                )?;
            }
            if tx.execute(
                "UPDATE jobs_attempt_reservations SET status = 'submitted', updated_at_ms = ?3
                  WHERE account_id = ?1 AND application_id = ?2
                    AND status IN ('running', 'side_effect_unknown')",
                params![account_id, application_id, now],
            )? != 1
            {
                anyhow::bail!("application attempt reservation is missing")
            }
            if tx.execute(
                "UPDATE jobs_browser_sessions SET status = 'complete', session_json = ?3,
                            updated_at_ms = ?4
                      WHERE account_id = ?1 AND id = ?2",
                params![
                    account_id,
                    terminal_session.id,
                    terminal_session_payload,
                    now
                ],
            )? != 1
            {
                anyhow::bail!("browser session not found")
            }
            crate::db::object_uploads::commit_application_object_uploads_sqlite_tx(
                &tx,
                account_id,
                application_id,
                run_id,
                runner,
                object_uploads,
                now,
            )?;
            application.receipt = receipt;
            application.state = "submitted".to_string();
            application.updated_at_ms = now;
            application.submitted_at_ms = Some(now);
            let application_payload = to_json(&application, "Jobs application")?;
            if tx.execute(
                "UPDATE jobs_applications SET state = 'submitted', application_json = ?3,
                        updated_at_ms = ?4, submitted_at_ms = ?4
                  WHERE account_id = ?1 AND id = ?2",
                params![account_id, application_id, application_payload, now],
            )? != 1
            {
                anyhow::bail!("application not found")
            }
            tx.commit().map_err(submission_commit_uncertain)?;
            Ok(SubmissionFinalizeResult::Committed(application))
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            if runner == "cloud" {
                require_current_runner_volume_identity_postgres_for_operation(
                    &mut tx, account_id, run_id, now,
                )?;
            }
            match crate::db::account_data::account_write_fence_postgres_tx(&mut tx, account_id)? {
                crate::db::account_data::AccountWriteFence::Active => {}
                crate::db::account_data::AccountWriteFence::DeletionRequested => {
                    anyhow::bail!("account deletion has fenced final submission")
                }
                crate::db::account_data::AccountWriteFence::Missing => {
                    anyhow::bail!("account not found")
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
            if application.state == "submitted" {
                if application
                    .receipt
                    .get("_bluey_server_submission_fingerprint_v1")
                    .and_then(Value::as_str)
                    == Some(request_fingerprint)
                {
                    tx.commit().map_err(submission_commit_uncertain)?;
                    return Ok(SubmissionFinalizeResult::Replayed(application));
                }
                anyhow::bail!("application already has a different final receipt")
            }
            if application.run_id.as_deref() != Some(run_id) {
                anyhow::bail!("application browser run does not match final receipt")
            }
            validate_submission_authority_snapshot(
                account_id,
                &application,
                run_id,
                &receipt,
                runner,
            )?;
            validate_submission_execution_authority_postgres_tx(
                &mut tx,
                account_id,
                application_id,
                run_id,
                runner,
                &receipt,
                local_ticket_hash,
                now,
            )?;
            validate_application_transition(&application.state, "submitted")?;
            let resume_version_id = application
                .resume_version_id
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("application resume version is missing"))?;
            validate_submission_evidence_resume_bindings(&prepared_evidence, resume_version_id)?;
            if runner == "cloud" {
                let phase = tx
                    .query_opt(
                        "SELECT phase FROM jobs_execution_leases
                          WHERE account_id = $1 AND application_id = $2 AND run_id = $3
                          FOR UPDATE",
                        &[&account_id, &application_id, &run_id],
                    )?
                    .map(|row| row.get::<_, String>(0));
                match phase.as_deref() {
                    Some("submitted") => {}
                    Some("side_effect_unknown") => {
                        if tx.execute(
                            "UPDATE jobs_execution_leases
                                SET phase = 'submitted', updated_at_ms = $4,
                                    finished_at_ms = $4
                              WHERE account_id = $1 AND application_id = $2 AND run_id = $3
                                AND phase = 'side_effect_unknown'",
                            &[&account_id, &application_id, &run_id, &now],
                        )? != 1
                        {
                            anyhow::bail!("matching cloud execution lease changed")
                        }
                    }
                    _ => anyhow::bail!("matching cloud execution lease cannot accept receipt"),
                }
            } else if tx.execute(
                "UPDATE jobs_local_run_tickets
                    SET status = 'complete', updated_at_ms = $5
                  WHERE id = $1 AND account_id = $2 AND application_id = $3
                    AND ticket_hash = $4
                    AND (
                        (expires_at_ms > $5
                            AND status IN ('claimed', 'needs_input'))
                        OR (status IN ('click_started', 'side_effect_unknown')
                            AND expires_at_ms + $6 > $5)
                    )",
                &[
                    &run_id,
                    &account_id,
                    &application_id,
                    &local_ticket_hash.expect("local ticket checked above"),
                    &now,
                    &SUBMISSION_RECONCILIATION_GRACE_MS,
                ],
            )? != 1
            {
                anyhow::bail!("local run ticket is not active")
            }
            for item in &prepared_evidence {
                tx.execute(
                    "INSERT INTO jobs_application_evidence (
                        id, account_id, application_id, kind, provider_event_hash,
                        evidence_json, occurred_at_ms, created_at_ms
                     ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
                    &[
                        &item.value.id,
                        &account_id,
                        &application_id,
                        &item.value.kind,
                        &item.provider_event_hash,
                        &item.payload,
                        &item.value.occurred_at_ms,
                        &item.value.created_at_ms,
                    ],
                )?;
            }
            if tx.execute(
                "UPDATE jobs_attempt_reservations SET status = 'submitted', updated_at_ms = $3
                  WHERE account_id = $1 AND application_id = $2
                    AND status IN ('running', 'side_effect_unknown')",
                &[&account_id, &application_id, &now],
            )? != 1
            {
                anyhow::bail!("application attempt reservation is missing")
            }
            if tx.execute(
                "UPDATE jobs_browser_sessions SET status = 'complete', session_json = $3,
                            updated_at_ms = $4
                      WHERE account_id = $1 AND id = $2",
                &[
                    &account_id,
                    &terminal_session.id,
                    &terminal_session_payload,
                    &now,
                ],
            )? != 1
            {
                anyhow::bail!("browser session not found")
            }
            crate::db::object_uploads::commit_application_object_uploads_postgres_tx(
                &mut tx,
                account_id,
                application_id,
                run_id,
                runner,
                object_uploads,
                now,
            )?;
            application.receipt = receipt;
            application.state = "submitted".to_string();
            application.updated_at_ms = now;
            application.submitted_at_ms = Some(now);
            let application_payload = to_json(&application, "Jobs application")?;
            if tx.execute(
                "UPDATE jobs_applications SET state = 'submitted', application_json = $3,
                        updated_at_ms = $4, submitted_at_ms = $4
                  WHERE account_id = $1 AND id = $2",
                &[&account_id, &application_id, &application_payload, &now],
            )? != 1
            {
                anyhow::bail!("application not found")
            }
            tx.commit().map_err(submission_commit_uncertain)?;
            Ok(SubmissionFinalizeResult::Committed(application))
        }
    })
}

pub fn list_application_identities(
    pool: &DbPool,
    account_id: &str,
) -> Result<Vec<ApplicationIdentity>> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get()?;
            let mut stmt = conn.prepare(
                "SELECT identity_json, verification_status, is_default
                   FROM jobs_application_identities WHERE account_id = ?1
                  ORDER BY is_default DESC, updated_at_ms DESC",
            )?;
            let rows = stmt
                .query_map(params![account_id], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, i64>(2)? != 0,
                    ))
                })?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            rows.into_iter()
                .map(|(raw, status, is_default)| {
                    parse_application_identity_row(raw, status, is_default)
                })
                .collect()
        }
        DbPool::Postgres(_) => pool
            .get_pg()?
            .query(
                "SELECT identity_json, verification_status, is_default
                   FROM jobs_application_identities WHERE account_id = $1
                  ORDER BY is_default DESC, updated_at_ms DESC",
                &[&account_id],
            )?
            .into_iter()
            .map(|row| {
                parse_application_identity_row(row.get(0), row.get(1), row.get::<_, i32>(2) != 0)
            })
            .collect(),
    })
}

fn parse_application_identity_row(
    raw: String,
    verification_status: String,
    is_default: bool,
) -> Result<ApplicationIdentity> {
    let mut identity: ApplicationIdentity = parse_json(raw, "application identity")?;
    identity.verification_status = verification_status;
    identity.is_default = is_default;
    Ok(identity)
}

pub fn get_application_identity(
    pool: &DbPool,
    account_id: &str,
    identity_id: &str,
) -> Result<Option<ApplicationIdentity>> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let raw: Option<(String, String, i64)> = pool
                .get()?
                .query_row(
                    "SELECT identity_json, verification_status, is_default
                       FROM jobs_application_identities
                      WHERE account_id = ?1 AND id = ?2",
                    params![account_id, identity_id],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .optional()?;
            raw.map(|(value, status, is_default)| {
                parse_application_identity_row(value, status, is_default != 0)
            })
            .transpose()
        }
        DbPool::Postgres(_) => pool
            .get_pg()?
            .query_opt(
                "SELECT identity_json, verification_status, is_default
                   FROM jobs_application_identities
                  WHERE account_id = $1 AND id = $2",
                &[&account_id, &identity_id],
            )?
            .map(|row| {
                parse_application_identity_row(row.get(0), row.get(1), row.get::<_, i32>(2) != 0)
            })
            .transpose(),
    })
}

fn application_identity_by_hash(
    pool: &DbPool,
    email_hash: &str,
) -> Result<Option<(String, ApplicationIdentity)>> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let row: Option<(String, String, String, i64)> = pool
                .get()?
                .query_row(
                    "SELECT account_id, identity_json, verification_status, is_default
                       FROM jobs_application_identities
                      WHERE email_hash = ?1",
                    params![email_hash],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
                )
                .optional()?;
            row.map(|(owner, raw, status, is_default)| {
                Ok((
                    owner,
                    parse_application_identity_row(raw, status, is_default != 0)?,
                ))
            })
            .transpose()
        }
        DbPool::Postgres(_) => pool
            .get_pg()?
            .query_opt(
                "SELECT account_id, identity_json, verification_status, is_default
                   FROM jobs_application_identities
                  WHERE email_hash = $1",
                &[&email_hash],
            )?
            .map(|row| {
                Ok((
                    row.get(0),
                    parse_application_identity_row(
                        row.get(1),
                        row.get(2),
                        row.get::<_, i32>(3) != 0,
                    )?,
                ))
            })
            .transpose(),
    })
}

pub fn ensure_primary_application_identity(
    pool: &DbPool,
    account_id: &str,
    email: &str,
) -> Result<ApplicationIdentity> {
    let normalized = normalize_application_email(email)?;
    let email_hash = private_lookup_hash("application-email", &normalized)?;
    if let Some((owner, existing)) = application_identity_by_hash(pool, &email_hash)? {
        if owner != account_id {
            anyhow::bail!("this application email belongs to another Bluey Jobs account")
        }
        return Ok(existing);
    }
    let is_default = list_application_identities(pool, account_id)?.is_empty();
    save_application_identity(
        pool,
        account_id,
        &ApplicationIdentity {
            id: String::new(),
            email: normalized,
            label: "Bluey login".to_string(),
            verification_status: "verified".to_string(),
            is_default,
            created_at_ms: 0,
            updated_at_ms: 0,
        },
    )
}

pub fn save_application_identity(
    pool: &DbPool,
    account_id: &str,
    identity: &ApplicationIdentity,
) -> Result<ApplicationIdentity> {
    let mut value = identity.clone();
    value.email = normalize_application_email(&value.email)?;
    if !matches!(value.verification_status.as_str(), "pending" | "verified") {
        anyhow::bail!("invalid application email verification status")
    }
    let email_hash = private_lookup_hash("application-email", &value.email)?;
    if let Some((owner, existing)) = application_identity_by_hash(pool, &email_hash)? {
        if owner != account_id {
            anyhow::bail!("this application email belongs to another Bluey Jobs account")
        }
        if value.id.is_empty() {
            value.id = existing.id;
            value.created_at_ms = existing.created_at_ms;
            value.verification_status = existing.verification_status;
        }
    }

    let existing = if value.id.is_empty() {
        None
    } else {
        get_application_identity(pool, account_id, &value.id)?
    };
    if existing.is_none() {
        let entitlement = get_entitlement(pool, account_id)?;
        if list_application_identities(pool, account_id)?.len() as i64
            >= entitlement.application_identity_limit
        {
            anyhow::bail!("application email limit reached for this Jobs plan")
        }
    } else if existing
        .as_ref()
        .is_some_and(|item| item.verification_status == "verified")
    {
        value.verification_status = "verified".to_string();
    }
    if value.id.is_empty() {
        value.id = uuid::Uuid::new_v4().to_string();
    }
    let now = now_ms();
    if value.created_at_ms == 0 {
        value.created_at_ms = now;
    }
    value.updated_at_ms = now;
    if value.is_default && value.verification_status != "verified" {
        anyhow::bail!("verify the application email before making it the default")
    }
    let payload = to_json(&value, "application identity")?;
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
            if value.is_default {
                tx.execute(
                    "UPDATE jobs_application_identities SET is_default = 0
                      WHERE account_id = ?1 AND id <> ?2",
                    params![account_id, value.id],
                )?;
            }
            tx.execute(
                "INSERT INTO jobs_application_identities (
                    id, account_id, email_hash, identity_json, verification_status,
                    is_default, created_at_ms, updated_at_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                 ON CONFLICT(id) DO UPDATE SET
                    email_hash = excluded.email_hash,
                    identity_json = excluded.identity_json,
                    verification_status = excluded.verification_status,
                    is_default = excluded.is_default,
                    updated_at_ms = excluded.updated_at_ms
                 WHERE jobs_application_identities.account_id = excluded.account_id",
                params![
                    value.id,
                    account_id,
                    email_hash,
                    payload,
                    value.verification_status,
                    i64::from(value.is_default),
                    value.created_at_ms,
                    value.updated_at_ms,
                ],
            )?;
            advance_account_input_generation_sqlite(
                &tx,
                account_id,
                "identity",
                &value.id,
                value.updated_at_ms,
            )?;
            tx.commit()?;
            Ok(value)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            lock_discovery_account_postgres(&mut tx, account_id)?;
            lock_account_policy_inputs_postgres(&mut tx, account_id, true)?;
            let is_default = i32::from(value.is_default);
            if value.is_default {
                tx.execute(
                    "UPDATE jobs_application_identities SET is_default = 0
                      WHERE account_id = $1 AND id <> $2",
                    &[&account_id, &value.id],
                )?;
            }
            tx.execute(
                "INSERT INTO jobs_application_identities (
                    id, account_id, email_hash, identity_json, verification_status,
                    is_default, created_at_ms, updated_at_ms
                 ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
                 ON CONFLICT(id) DO UPDATE SET
                    email_hash = EXCLUDED.email_hash,
                    identity_json = EXCLUDED.identity_json,
                    verification_status = EXCLUDED.verification_status,
                    is_default = EXCLUDED.is_default,
                    updated_at_ms = EXCLUDED.updated_at_ms
                 WHERE jobs_application_identities.account_id = EXCLUDED.account_id",
                &[
                    &value.id,
                    &account_id,
                    &email_hash,
                    &payload,
                    &value.verification_status,
                    &is_default,
                    &value.created_at_ms,
                    &value.updated_at_ms,
                ],
            )?;
            advance_account_input_generation_postgres(
                &mut tx,
                account_id,
                "identity",
                &value.id,
                value.updated_at_ms,
            )?;
            tx.commit()?;
            Ok(value)
        }
    })
}

pub fn delete_application_identity(
    pool: &DbPool,
    account_id: &str,
    identity_id: &str,
) -> Result<bool> {
    let identity = get_application_identity(pool, account_id, identity_id)?
        .ok_or_else(|| anyhow::anyhow!("application email not found"))?;
    if identity.is_default {
        anyhow::bail!("choose another default application email before removing this one")
    }
    if list_tracks(pool, account_id)?
        .iter()
        .any(|track| track.application_identity_id.as_deref() == Some(identity_id))
    {
        anyhow::bail!("choose another email for the Career Track before removing this one")
    }
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let is_default: i64 = tx
                .query_row(
                    "SELECT is_default FROM jobs_application_identities
                      WHERE account_id = ?1 AND id = ?2",
                    params![account_id, identity_id],
                    |row| row.get(0),
                )
                .optional()?
                .context("application email not found")?;
            anyhow::ensure!(is_default == 0, "choose another default application email first");
            let bound = {
                let mut stmt = tx.prepare(
                    "SELECT track_json FROM jobs_tracks WHERE account_id = ?1 ORDER BY id",
                )?;
                let rows = stmt
                    .query_map(params![account_id], |row| row.get::<_, String>(0))?
                    .collect::<std::result::Result<Vec<_>, _>>()?;
                rows.into_iter()
                    .map(|raw| parse_json::<CareerTrack>(raw, "Career Track"))
                    .collect::<Result<Vec<_>>>()?
                    .into_iter()
                    .any(|track| {
                        track.application_identity_id.as_deref() == Some(identity_id)
                    })
            };
            anyhow::ensure!(!bound, "choose another email for the Career Track first");
            let changed = tx.execute(
                "DELETE FROM jobs_application_identities WHERE account_id = ?1 AND id = ?2",
                params![account_id, identity_id],
            )? > 0;
            if changed {
                advance_account_input_generation_sqlite(
                    &tx,
                    account_id,
                    "identity",
                    identity_id,
                    now_ms(),
                )?;
            }
            tx.commit()?;
            Ok(changed)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            lock_discovery_account_postgres(&mut tx, account_id)?;
            lock_account_policy_inputs_postgres(&mut tx, account_id, true)?;
            let is_default: i32 = tx
                .query_opt(
                    "SELECT is_default FROM jobs_application_identities
                      WHERE account_id = $1 AND id = $2",
                    &[&account_id, &identity_id],
                )?
                .context("application email not found")?
                .get(0);
            anyhow::ensure!(is_default == 0, "choose another default application email first");
            let bound = tx
                .query(
                    "SELECT track_json FROM jobs_tracks
                      WHERE account_id = $1 ORDER BY id",
                    &[&account_id],
                )?
                .into_iter()
                .map(|row| parse_json::<CareerTrack>(row.get(0), "Career Track"))
                .collect::<Result<Vec<_>>>()?
                .into_iter()
                .any(|track| track.application_identity_id.as_deref() == Some(identity_id));
            anyhow::ensure!(!bound, "choose another email for the Career Track first");
            let changed = tx.execute(
                "DELETE FROM jobs_application_identities WHERE account_id = $1 AND id = $2",
                &[&account_id, &identity_id],
            )? > 0;
            if changed {
                advance_account_input_generation_postgres(
                    &mut tx,
                    account_id,
                    "identity",
                    identity_id,
                    now_ms(),
                )?;
            }
            tx.commit()?;
            Ok(changed)
        }
    })
}

pub fn save_identity_verification(
    pool: &DbPool,
    account_id: &str,
    identity_id: &str,
    code: &str,
    ttl_ms: i64,
) -> Result<ApplicationIdentity> {
    let identity = get_application_identity(pool, account_id, identity_id)?
        .ok_or_else(|| anyhow::anyhow!("application email not found"))?;
    if identity.verification_status == "verified" {
        return Ok(identity);
    }
    let created_at = now_ms();
    let expires_at = created_at + ttl_ms;
    let otp_hash = private_lookup_hash(&format!("identity-otp:{identity_id}"), code)?;
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get()?;
            let previous: Option<i64> = conn
                .query_row(
                    "SELECT created_at_ms FROM jobs_identity_verifications
                      WHERE account_id = ?1 AND identity_id = ?2",
                    params![account_id, identity_id],
                    |row| row.get(0),
                )
                .optional()?;
            if previous.is_some_and(|timestamp| timestamp > created_at - 60_000) {
                anyhow::bail!("wait a minute before requesting another verification code")
            }
            conn.execute(
                "INSERT INTO jobs_identity_verifications (
                    identity_id, account_id, otp_hash, attempts, expires_at_ms, created_at_ms
                 ) VALUES (?1, ?2, ?3, 0, ?4, ?5)
                 ON CONFLICT(identity_id) DO UPDATE SET otp_hash = excluded.otp_hash,
                    attempts = 0, expires_at_ms = excluded.expires_at_ms,
                    created_at_ms = excluded.created_at_ms
                 WHERE jobs_identity_verifications.account_id = excluded.account_id",
                params![identity_id, account_id, otp_hash, expires_at, created_at],
            )?;
            Ok(identity)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let previous = conn.query_opt(
                "SELECT created_at_ms FROM jobs_identity_verifications
                  WHERE account_id = $1 AND identity_id = $2",
                &[&account_id, &identity_id],
            )?;
            if previous.is_some_and(|row| row.get::<_, i64>(0) > created_at - 60_000) {
                anyhow::bail!("wait a minute before requesting another verification code")
            }
            conn.execute(
                "INSERT INTO jobs_identity_verifications (
                    identity_id, account_id, otp_hash, attempts, expires_at_ms, created_at_ms
                 ) VALUES ($1, $2, $3, 0, $4, $5)
                 ON CONFLICT(identity_id) DO UPDATE SET otp_hash = EXCLUDED.otp_hash,
                    attempts = 0, expires_at_ms = EXCLUDED.expires_at_ms,
                    created_at_ms = EXCLUDED.created_at_ms
                 WHERE jobs_identity_verifications.account_id = EXCLUDED.account_id",
                &[
                    &identity_id,
                    &account_id,
                    &otp_hash,
                    &expires_at,
                    &created_at,
                ],
            )?;
            Ok(identity)
        }
    })
}

fn constant_time_equal(left: &str, right: &str) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.as_bytes()
        .iter()
        .zip(right.as_bytes())
        .fold(0u8, |diff, (a, b)| diff | (a ^ b))
        == 0
}

pub fn verify_application_identity(
    pool: &DbPool,
    account_id: &str,
    identity_id: &str,
    code: &str,
) -> Result<ApplicationIdentity> {
    let submitted_hash = private_lookup_hash(&format!("identity-otp:{identity_id}"), code)?;
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
            let (stored_hash, attempts, expires_at): (String, i64, i64) = tx
                .query_row(
                    "SELECT otp_hash, attempts, expires_at_ms FROM jobs_identity_verifications
                      WHERE account_id = ?1 AND identity_id = ?2",
                    params![account_id, identity_id],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .optional()?
                .ok_or_else(|| anyhow::anyhow!("request a new verification code"))?;
            if expires_at <= now_ms() {
                anyhow::bail!("verification code expired")
            }
            if attempts >= 5 {
                anyhow::bail!("too many verification attempts; request a new code")
            }
            if !constant_time_equal(&stored_hash, &submitted_hash) {
                tx.execute(
                    "UPDATE jobs_identity_verifications SET attempts = attempts + 1
                      WHERE account_id = ?1 AND identity_id = ?2",
                    params![account_id, identity_id],
                )?;
                tx.commit()?;
                anyhow::bail!("verification code is incorrect")
            }
            let raw: String = tx.query_row(
                "SELECT identity_json FROM jobs_application_identities
                  WHERE account_id = ?1 AND id = ?2",
                params![account_id, identity_id],
                |row| row.get(0),
            )?;
            let mut identity: ApplicationIdentity = parse_json(raw, "application identity")?;
            identity.verification_status = "verified".to_string();
            identity.updated_at_ms = now_ms();
            let payload = to_json(&identity, "application identity")?;
            tx.execute(
                "UPDATE jobs_application_identities SET verification_status = 'verified',
                    identity_json = ?3, updated_at_ms = ?4
                  WHERE account_id = ?1 AND id = ?2",
                params![account_id, identity_id, payload, identity.updated_at_ms],
            )?;
            advance_account_input_generation_sqlite(
                &tx,
                account_id,
                "identity",
                identity_id,
                identity.updated_at_ms,
            )?;
            tx.execute(
                "DELETE FROM jobs_identity_verifications WHERE account_id = ?1 AND identity_id = ?2",
                params![account_id, identity_id],
            )?;
            tx.commit()?;
            Ok(identity)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            lock_discovery_account_postgres(&mut tx, account_id)?;
            lock_account_policy_inputs_postgres(&mut tx, account_id, true)?;
            let row = tx
                .query_opt(
                    "SELECT otp_hash, attempts, expires_at_ms FROM jobs_identity_verifications
                      WHERE account_id = $1 AND identity_id = $2 FOR UPDATE",
                    &[&account_id, &identity_id],
                )?
                .ok_or_else(|| anyhow::anyhow!("request a new verification code"))?;
            let stored_hash: String = row.get(0);
            let attempts: i64 = row.get(1);
            let expires_at: i64 = row.get(2);
            if expires_at <= now_ms() {
                anyhow::bail!("verification code expired")
            }
            if attempts >= 5 {
                anyhow::bail!("too many verification attempts; request a new code")
            }
            if !constant_time_equal(&stored_hash, &submitted_hash) {
                tx.execute(
                    "UPDATE jobs_identity_verifications SET attempts = attempts + 1
                      WHERE account_id = $1 AND identity_id = $2",
                    &[&account_id, &identity_id],
                )?;
                tx.commit()?;
                anyhow::bail!("verification code is incorrect")
            }
            let raw: String = tx
                .query_one(
                    "SELECT identity_json FROM jobs_application_identities
                      WHERE account_id = $1 AND id = $2",
                    &[&account_id, &identity_id],
                )?
                .get(0);
            let mut identity: ApplicationIdentity = parse_json(raw, "application identity")?;
            identity.verification_status = "verified".to_string();
            identity.updated_at_ms = now_ms();
            let payload = to_json(&identity, "application identity")?;
            tx.execute(
                "UPDATE jobs_application_identities SET verification_status = 'verified',
                    identity_json = $3, updated_at_ms = $4
                  WHERE account_id = $1 AND id = $2",
                &[&account_id, &identity_id, &payload, &identity.updated_at_ms],
            )?;
            advance_account_input_generation_postgres(
                &mut tx,
                account_id,
                "identity",
                identity_id,
                identity.updated_at_ms,
            )?;
            tx.execute(
                "DELETE FROM jobs_identity_verifications WHERE account_id = $1 AND identity_id = $2",
                &[&account_id, &identity_id],
            )?;
            tx.commit()?;
            Ok(identity)
        }
    })
}

pub fn delete_identity_verification(
    pool: &DbPool,
    account_id: &str,
    identity_id: &str,
) -> Result<()> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            pool.get()?.execute(
                "DELETE FROM jobs_identity_verifications
                  WHERE account_id = ?1 AND identity_id = ?2",
                params![account_id, identity_id],
            )?;
            Ok(())
        }
        DbPool::Postgres(_) => {
            pool.get_pg()?.execute(
                "DELETE FROM jobs_identity_verifications
                  WHERE account_id = $1 AND identity_id = $2",
                &[&account_id, &identity_id],
            )?;
            Ok(())
        }
    })
}

pub fn list_mailbox_connections(pool: &DbPool, account_id: &str) -> Result<Vec<MailboxConnection>> {
    list_payloads(
        pool,
        account_id,
        "jobs_mailbox_connections",
        "connection_json",
        "updated_at_ms DESC",
        "mailbox connection",
    )
}

pub fn mailbox_connection_by_subject(
    pool: &DbPool,
    account_id: &str,
    provider: &str,
    subject_hash: &str,
) -> Result<Option<MailboxConnection>> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let raw: Option<String> = pool
                .get()?
                .query_row(
                    "SELECT connection_json FROM jobs_mailbox_connections
                      WHERE account_id = ?1 AND provider = ?2 AND provider_subject_hash = ?3",
                    params![account_id, provider, subject_hash],
                    |row| row.get(0),
                )
                .optional()?;
            raw.map(|value| parse_json(value, "mailbox connection"))
                .transpose()
        }
        DbPool::Postgres(_) => pool
            .get_pg()?
            .query_opt(
                "SELECT connection_json FROM jobs_mailbox_connections
                  WHERE account_id = $1 AND provider = $2 AND provider_subject_hash = $3",
                &[&account_id, &provider, &subject_hash],
            )?
            .map(|row| parse_json(row.get(0), "mailbox connection"))
            .transpose(),
    })
}

pub fn mailbox_connection_for_provider_subject(
    pool: &DbPool,
    account_id: &str,
    provider: &str,
    provider_subject: &str,
) -> Result<Option<MailboxConnection>> {
    if !matches!(provider, "gmail" | "outlook") {
        anyhow::bail!("unsupported Jobs credential provider")
    }
    let subject = provider_subject.trim();
    if subject.is_empty() {
        anyhow::bail!("provider subject is required")
    }
    let subject_hash = private_lookup_hash(&format!("mailbox:{provider}"), subject)?;
    mailbox_connection_by_subject(pool, account_id, provider, &subject_hash)
}

pub fn save_mailbox_connection_with_credential(
    pool: &DbPool,
    account_id: &str,
    connection: &MailboxConnection,
    credential: &JobsProviderCredential,
) -> Result<(MailboxConnection, JobsProviderCredential)> {
    let mut mailbox = connection.clone();
    if !matches!(mailbox.provider.as_str(), "gmail" | "outlook")
        || mailbox.provider != credential.provider
    {
        anyhow::bail!("mailbox and credential provider must match")
    }
    if mailbox.status != "connected" {
        anyhow::bail!("OAuth mailbox must be connected")
    }
    if credential.provider_subject.trim().is_empty()
        || credential.access_token.trim().is_empty()
        || credential.refresh_token.trim().is_empty()
    {
        anyhow::bail!("provider credential is incomplete")
    }

    mailbox.account_label = normalize_application_email(&mailbox.account_label)?;
    mailbox.aliases = mailbox
        .aliases
        .iter()
        .filter_map(|alias| normalize_application_email(alias).ok())
        .collect();
    mailbox.aliases.sort();
    mailbox.aliases.dedup();
    if mailbox.capabilities.is_empty() {
        mailbox.capabilities = vec![
            "status_sync".to_string(),
            "application_correlation".to_string(),
            "review_interventions".to_string(),
        ];
    }

    let provider_subject = credential.provider_subject.trim();
    let subject_hash =
        private_lookup_hash(&format!("mailbox:{}", mailbox.provider), provider_subject)?;
    let existing =
        mailbox_connection_by_subject(pool, account_id, &mailbox.provider, &subject_hash)?;
    let is_new = existing.is_none();
    if let Some(existing) = existing {
        mailbox.id = existing.id;
        mailbox.created_at_ms = existing.created_at_ms;
    } else if mailbox.id.is_empty() {
        mailbox.id = uuid::Uuid::new_v4().to_string();
    }

    let now = now_ms();
    if mailbox.created_at_ms == 0 {
        mailbox.created_at_ms = now;
    }
    mailbox.updated_at_ms = now;

    let mut stored_credential = credential.clone();
    stored_credential.connection_id = mailbox.id.clone();
    stored_credential.scopes = stored_credential
        .scopes
        .iter()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect();
    stored_credential.scopes.sort();
    stored_credential.scopes.dedup();
    stored_credential.capabilities = stored_credential
        .capabilities
        .iter()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect();
    stored_credential.capabilities.sort();
    stored_credential.capabilities.dedup();
    if stored_credential.grant_revision < 0 {
        anyhow::bail!("provider grant revision is invalid")
    }
    if stored_credential.grant_revision > 0 {
        let computed = communication_grant_sha256(&stored_credential)?;
        if !stored_credential.grant_sha256.is_empty()
            && stored_credential.grant_sha256 != computed
        {
            anyhow::bail!("provider grant digest is invalid")
        }
        stored_credential.grant_sha256 = computed;
    } else if !stored_credential.grant_sha256.is_empty() {
        anyhow::bail!("legacy provider credential cannot carry a grant digest")
    }
    if stored_credential.created_at_ms == 0 {
        stored_credential.created_at_ms = now;
    }
    stored_credential.updated_at_ms = now;

    let mailbox_payload = to_json(&mailbox, "mailbox connection")?;
    let credential_payload = to_json(&stored_credential, "Jobs provider credential")?;
    let sync_state = JobsProviderSyncState {
        connection_id: mailbox.id.clone(),
        provider: mailbox.provider.clone(),
        cursor: json!({}),
        next_sync_at_ms: now,
        last_synced_at_ms: None,
        last_error: String::new(),
        lease_owner: None,
        lease_expires_at_ms: None,
        created_at_ms: now,
        updated_at_ms: now,
    };
    let sync_payload = to_json(&sync_state, "Jobs provider sync state")?;
    let entitlement = get_entitlement(pool, account_id)?;

    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
            crate::db::object_uploads::require_active_account_write_fence_sqlite_tx(
                &tx,
                account_id,
            )?;
            let was_disconnected = tx
                .query_row(
                    "SELECT status FROM jobs_mailbox_connections
                      WHERE account_id = ?1 AND id = ?2",
                    params![account_id, mailbox.id],
                    |row| row.get::<_, String>(0),
                )
                .optional()?
                .is_some_and(|status| status == "disconnected");
            let unresolved_actions: i64 = tx.query_row(
                "SELECT COUNT(*) FROM jobs_communication_actions
                  WHERE account_id = ?1 AND connection_id = ?2
                    AND status IN ('dispatching', 'side_effect_unknown')",
                params![account_id, mailbox.id],
                |row| row.get(0),
            )?;
            if unresolved_actions > 0 {
                anyhow::bail!("provider grant cannot change while communication is unresolved")
            }
            if is_new {
                let active_count: i64 = tx.query_row(
                    "SELECT COUNT(*) FROM jobs_mailbox_connections
                      WHERE account_id = ?1 AND status != 'disconnected'",
                    params![account_id],
                    |row| row.get(0),
                )?;
                if active_count >= entitlement.connected_inbox_limit {
                    anyhow::bail!("connected inbox limit reached for this Jobs plan")
                }
            }
            tx.execute(
                "INSERT INTO jobs_mailbox_connections (
                    id, account_id, provider, provider_subject_hash, status,
                    connection_json, created_at_ms, updated_at_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                 ON CONFLICT(id) DO UPDATE SET status = excluded.status,
                    connection_json = excluded.connection_json,
                    updated_at_ms = excluded.updated_at_ms
                 WHERE jobs_mailbox_connections.account_id = excluded.account_id",
                params![
                    mailbox.id,
                    account_id,
                    mailbox.provider,
                    subject_hash,
                    mailbox.status,
                    mailbox_payload,
                    mailbox.created_at_ms,
                    mailbox.updated_at_ms,
                ],
            )?;
            tx.execute(
                "INSERT INTO jobs_provider_credentials (
                    connection_id, account_id, provider, provider_subject_hash,
                    credential_json, created_at_ms, updated_at_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                 ON CONFLICT(connection_id) DO UPDATE SET
                    provider = excluded.provider,
                    provider_subject_hash = excluded.provider_subject_hash,
                    credential_json = excluded.credential_json,
                    updated_at_ms = excluded.updated_at_ms
                 WHERE jobs_provider_credentials.account_id = excluded.account_id",
                params![
                    stored_credential.connection_id,
                    account_id,
                    stored_credential.provider,
                    subject_hash,
                    credential_payload,
                    stored_credential.created_at_ms,
                    stored_credential.updated_at_ms,
                ],
            )?;
            tx.execute(
                "INSERT INTO jobs_provider_sync_state (
                    connection_id, account_id, provider, sync_json, next_sync_at_ms,
                    last_synced_at_ms, lease_owner, lease_expires_at_ms,
                    created_at_ms, updated_at_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, NULL, NULL, NULL, ?6, ?7)
                 ON CONFLICT(connection_id) DO NOTHING",
                params![
                    mailbox.id,
                    account_id,
                    mailbox.provider,
                    sync_payload,
                    sync_state.next_sync_at_ms,
                    sync_state.created_at_ms,
                    sync_state.updated_at_ms,
                ],
            )?;
            if was_disconnected {
                tx.execute(
                    "DELETE FROM jobs_communication_write_fences
                      WHERE account_id = ?1 AND connection_id = ?2
                        AND reason = 'mailbox_disconnect'",
                    params![account_id, mailbox.id],
                )?;
            }
            tx.commit()?;
            Ok((mailbox, stored_credential))
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            crate::db::object_uploads::require_active_account_write_fence_postgres_tx(
                &mut tx,
                account_id,
            )?;
            let was_disconnected = tx
                .query_opt(
                    "SELECT status FROM jobs_mailbox_connections
                      WHERE account_id = $1 AND id = $2 FOR UPDATE",
                    &[&account_id, &mailbox.id],
                )?
                .is_some_and(|row| row.get::<_, String>(0) == "disconnected");
            let unresolved_actions: i64 = tx
                .query_one(
                    "SELECT COUNT(*)::bigint FROM jobs_communication_actions
                      WHERE account_id = $1 AND connection_id = $2
                        AND status IN ('dispatching', 'side_effect_unknown')",
                    &[&account_id, &mailbox.id],
                )?
                .get(0);
            if unresolved_actions > 0 {
                anyhow::bail!("provider grant cannot change while communication is unresolved")
            }
            if is_new {
                let active_count: i64 = tx
                    .query_one(
                        "SELECT COUNT(*) FROM jobs_mailbox_connections
                          WHERE account_id = $1 AND status != 'disconnected'",
                        &[&account_id],
                    )?
                    .get(0);
                if active_count >= entitlement.connected_inbox_limit {
                    anyhow::bail!("connected inbox limit reached for this Jobs plan")
                }
            }
            tx.execute(
                "INSERT INTO jobs_mailbox_connections (
                    id, account_id, provider, provider_subject_hash, status,
                    connection_json, created_at_ms, updated_at_ms
                 ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
                 ON CONFLICT(id) DO UPDATE SET status = EXCLUDED.status,
                    connection_json = EXCLUDED.connection_json,
                    updated_at_ms = EXCLUDED.updated_at_ms
                 WHERE jobs_mailbox_connections.account_id = EXCLUDED.account_id",
                &[
                    &mailbox.id,
                    &account_id,
                    &mailbox.provider,
                    &subject_hash,
                    &mailbox.status,
                    &mailbox_payload,
                    &mailbox.created_at_ms,
                    &mailbox.updated_at_ms,
                ],
            )?;
            tx.execute(
                "INSERT INTO jobs_provider_credentials (
                    connection_id, account_id, provider, provider_subject_hash,
                    credential_json, created_at_ms, updated_at_ms
                 ) VALUES ($1, $2, $3, $4, $5, $6, $7)
                 ON CONFLICT(connection_id) DO UPDATE SET
                    provider = EXCLUDED.provider,
                    provider_subject_hash = EXCLUDED.provider_subject_hash,
                    credential_json = EXCLUDED.credential_json,
                    updated_at_ms = EXCLUDED.updated_at_ms
                 WHERE jobs_provider_credentials.account_id = EXCLUDED.account_id",
                &[
                    &stored_credential.connection_id,
                    &account_id,
                    &stored_credential.provider,
                    &subject_hash,
                    &credential_payload,
                    &stored_credential.created_at_ms,
                    &stored_credential.updated_at_ms,
                ],
            )?;
            tx.execute(
                "INSERT INTO jobs_provider_sync_state (
                    connection_id, account_id, provider, sync_json, next_sync_at_ms,
                    last_synced_at_ms, lease_owner, lease_expires_at_ms,
                    created_at_ms, updated_at_ms
                 ) VALUES ($1, $2, $3, $4, $5, NULL, NULL, NULL, $6, $7)
                 ON CONFLICT(connection_id) DO NOTHING",
                &[
                    &mailbox.id,
                    &account_id,
                    &mailbox.provider,
                    &sync_payload,
                    &sync_state.next_sync_at_ms,
                    &sync_state.created_at_ms,
                    &sync_state.updated_at_ms,
                ],
            )?;
            if was_disconnected {
                tx.execute(
                    "DELETE FROM jobs_communication_write_fences
                      WHERE account_id = $1 AND connection_id = $2
                        AND reason = 'mailbox_disconnect'",
                    &[&account_id, &mailbox.id],
                )?;
            }
            tx.commit()?;
            Ok((mailbox, stored_credential))
        }
    })
}

pub fn save_mailbox_connection_with_credential_cas(
    pool: &DbPool,
    account_id: &str,
    connection: &MailboxConnection,
    credential: &JobsProviderCredential,
    expected_grant_revision: i64,
) -> Result<(MailboxConnection, JobsProviderCredential)> {
    if connection.id.trim().is_empty()
        || credential.connection_id != connection.id
        || connection.provider != credential.provider
        || !matches!(connection.provider.as_str(), "gmail" | "outlook")
        || expected_grant_revision < 0
        || credential.grant_revision != expected_grant_revision.saturating_add(1)
    {
        anyhow::bail!("provider write-grant CAS request is invalid")
    }
    let mut mailbox = connection.clone();
    mailbox.status = "connected".to_string();
    mailbox.capabilities = credential.capabilities.clone();
    mailbox.capabilities.sort();
    mailbox.capabilities.dedup();
    mailbox.updated_at_ms = now_ms();

    let mut stored_credential = credential.clone();
    stored_credential.scopes.sort();
    stored_credential.scopes.dedup();
    stored_credential.capabilities = mailbox.capabilities.clone();
    stored_credential.grant_sha256 = communication_grant_sha256(&stored_credential)?;
    let subject_hash = private_lookup_hash(
        &format!("mailbox:{}", mailbox.provider),
        stored_credential.provider_subject.trim(),
    )?;

    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
            crate::db::object_uploads::require_active_account_write_fence_sqlite_tx(
                &tx,
                account_id,
            )?;
            let unresolved_actions: i64 = tx.query_row(
                "SELECT COUNT(*) FROM jobs_communication_actions
                  WHERE account_id = ?1 AND connection_id = ?2
                    AND status IN ('dispatching', 'side_effect_unknown')",
                params![account_id, mailbox.id],
                |row| row.get(0),
            )?;
            if unresolved_actions > 0 {
                anyhow::bail!("provider grant cannot change while communication is unresolved")
            }
            let raw: Option<(String, String)> = tx
                .query_row(
                    "SELECT mailbox.connection_json, credential.credential_json
                       FROM jobs_mailbox_connections mailbox
                       JOIN jobs_provider_credentials credential
                         ON credential.account_id = mailbox.account_id
                        AND credential.connection_id = mailbox.id
                      WHERE mailbox.account_id = ?1 AND mailbox.id = ?2
                        AND mailbox.provider = ?3 AND mailbox.provider_subject_hash = ?4
                        AND mailbox.status = 'connected'
                        AND credential.provider = ?3
                        AND credential.provider_subject_hash = ?4",
                    params![account_id, mailbox.id, mailbox.provider, subject_hash],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()?;
            let (current_mailbox_json, current_credential_json) = raw
                .ok_or_else(|| anyhow::anyhow!("connected mailbox provider grant not found"))?;
            let current_mailbox: MailboxConnection =
                parse_json(current_mailbox_json, "mailbox connection")?;
            let current: JobsProviderCredential =
                parse_json(current_credential_json, "Jobs provider credential")?;
            if current.grant_revision != expected_grant_revision
                || current.provider_subject != stored_credential.provider_subject
                || current_mailbox.status != "connected"
                || current_mailbox.provider != mailbox.provider
            {
                anyhow::bail!("provider grant revision changed during authorization")
            }
            if expected_grant_revision > 0
                && (current.grant_sha256.len() != 64
                    || communication_grant_sha256(&current)? != current.grant_sha256)
            {
                anyhow::bail!("current provider grant digest is invalid")
            }
            let authority_updated_at_ms = now_ms()
                .max(current_mailbox.updated_at_ms.saturating_add(1))
                .max(current.updated_at_ms.saturating_add(1));
            mailbox.updated_at_ms = authority_updated_at_ms;
            stored_credential.updated_at_ms = authority_updated_at_ms;
            let mailbox_json = to_json(&mailbox, "mailbox connection")?;
            let credential_json =
                to_json(&stored_credential, "Jobs provider credential")?;
            let mailbox_updated = tx.execute(
                "UPDATE jobs_mailbox_connections
                    SET status = 'connected', connection_json = ?4, updated_at_ms = ?5
                  WHERE account_id = ?1 AND id = ?2 AND provider = ?3
                    AND provider_subject_hash = ?6 AND status = 'connected'",
                params![
                    account_id,
                    mailbox.id,
                    mailbox.provider,
                    mailbox_json,
                    mailbox.updated_at_ms,
                    subject_hash,
                ],
            )?;
            let credential_updated = tx.execute(
                "UPDATE jobs_provider_credentials
                    SET credential_json = ?5, updated_at_ms = ?6
                  WHERE account_id = ?1 AND connection_id = ?2 AND provider = ?3
                    AND provider_subject_hash = ?4",
                params![
                    account_id,
                    stored_credential.connection_id,
                    stored_credential.provider,
                    subject_hash,
                    credential_json,
                    stored_credential.updated_at_ms,
                ],
            )?;
            if mailbox_updated != 1 || credential_updated != 1 {
                anyhow::bail!("provider grant CAS target changed")
            }
            tx.commit()?;
            Ok((mailbox, stored_credential))
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            crate::db::object_uploads::require_active_account_write_fence_postgres_tx(
                &mut tx,
                account_id,
            )?;
            let unresolved_actions: i64 = tx
                .query_one(
                    "SELECT COUNT(*)::bigint FROM jobs_communication_actions
                      WHERE account_id = $1 AND connection_id = $2
                        AND status IN ('dispatching', 'side_effect_unknown')",
                    &[&account_id, &mailbox.id],
                )?
                .get(0);
            if unresolved_actions > 0 {
                anyhow::bail!("provider grant cannot change while communication is unresolved")
            }
            let row = tx
                .query_opt(
                    "SELECT mailbox.connection_json, credential.credential_json
                       FROM jobs_mailbox_connections mailbox
                       JOIN jobs_provider_credentials credential
                         ON credential.account_id = mailbox.account_id
                        AND credential.connection_id = mailbox.id
                      WHERE mailbox.account_id = $1 AND mailbox.id = $2
                        AND mailbox.provider = $3 AND mailbox.provider_subject_hash = $4
                        AND mailbox.status = 'connected'
                        AND credential.provider = $3
                        AND credential.provider_subject_hash = $4
                      FOR UPDATE OF mailbox, credential",
                    &[&account_id, &mailbox.id, &mailbox.provider, &subject_hash],
                )?
                .ok_or_else(|| anyhow::anyhow!("connected mailbox provider grant not found"))?;
            let current_mailbox: MailboxConnection =
                parse_json(row.get(0), "mailbox connection")?;
            let current: JobsProviderCredential =
                parse_json(row.get(1), "Jobs provider credential")?;
            if current.grant_revision != expected_grant_revision
                || current.provider_subject != stored_credential.provider_subject
                || current_mailbox.status != "connected"
                || current_mailbox.provider != mailbox.provider
            {
                anyhow::bail!("provider grant revision changed during authorization")
            }
            if expected_grant_revision > 0
                && (current.grant_sha256.len() != 64
                    || communication_grant_sha256(&current)? != current.grant_sha256)
            {
                anyhow::bail!("current provider grant digest is invalid")
            }
            let authority_updated_at_ms = now_ms()
                .max(current_mailbox.updated_at_ms.saturating_add(1))
                .max(current.updated_at_ms.saturating_add(1));
            mailbox.updated_at_ms = authority_updated_at_ms;
            stored_credential.updated_at_ms = authority_updated_at_ms;
            let mailbox_json = to_json(&mailbox, "mailbox connection")?;
            let credential_json =
                to_json(&stored_credential, "Jobs provider credential")?;
            let mailbox_updated = tx.execute(
                "UPDATE jobs_mailbox_connections
                    SET status = 'connected', connection_json = $4, updated_at_ms = $5
                  WHERE account_id = $1 AND id = $2 AND provider = $3
                    AND provider_subject_hash = $6 AND status = 'connected'",
                &[
                    &account_id,
                    &mailbox.id,
                    &mailbox.provider,
                    &mailbox_json,
                    &mailbox.updated_at_ms,
                    &subject_hash,
                ],
            )?;
            let credential_updated = tx.execute(
                "UPDATE jobs_provider_credentials
                    SET credential_json = $5, updated_at_ms = $6
                  WHERE account_id = $1 AND connection_id = $2 AND provider = $3
                    AND provider_subject_hash = $4",
                &[
                    &account_id,
                    &stored_credential.connection_id,
                    &stored_credential.provider,
                    &subject_hash,
                    &credential_json,
                    &stored_credential.updated_at_ms,
                ],
            )?;
            if mailbox_updated != 1 || credential_updated != 1 {
                anyhow::bail!("provider grant CAS target changed")
            }
            tx.commit()?;
            Ok((mailbox, stored_credential))
        }
    })
}

pub fn save_mailbox_connection(
    pool: &DbPool,
    account_id: &str,
    connection: &MailboxConnection,
    provider_subject: &str,
) -> Result<MailboxConnection> {
    let mut value = connection.clone();
    if !matches!(value.provider.as_str(), "gmail" | "outlook") {
        anyhow::bail!("choose Gmail or Outlook")
    }
    if !matches!(
        value.status.as_str(),
        "pending" | "connected" | "disconnected"
    ) {
        anyhow::bail!("invalid mailbox connection status")
    }
    value.account_label = normalize_application_email(&value.account_label)?;
    value.aliases = value
        .aliases
        .iter()
        .filter_map(|alias| normalize_application_email(alias).ok())
        .collect();
    value.aliases.sort();
    value.aliases.dedup();
    let subject = if provider_subject.trim().is_empty() {
        value.account_label.as_str()
    } else {
        provider_subject.trim()
    };
    let subject_hash = private_lookup_hash(&format!("mailbox:{}", value.provider), subject)?;
    let existing = list_mailbox_connections(pool, account_id)?;
    if value.id.is_empty() {
        if let Some(item) =
            mailbox_connection_by_subject(pool, account_id, &value.provider, &subject_hash)?
        {
            value.id = item.id.clone();
            value.created_at_ms = item.created_at_ms;
        } else {
            let entitlement = get_entitlement(pool, account_id)?;
            let active_count = existing
                .iter()
                .filter(|item| item.status != "disconnected")
                .count() as i64;
            if value.status != "disconnected" && active_count >= entitlement.connected_inbox_limit {
                anyhow::bail!("connected inbox limit reached for this Jobs plan")
            }
            value.id = uuid::Uuid::new_v4().to_string();
        }
    }
    let now = now_ms();
    if value.created_at_ms == 0 {
        value.created_at_ms = now;
    }
    value.updated_at_ms = now;
    if value.capabilities.is_empty() {
        value.capabilities = vec![
            "status_sync".to_string(),
            "application_correlation".to_string(),
            "review_interventions".to_string(),
        ];
    }
    let payload = to_json(&value, "mailbox connection")?;
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            pool.get()?.execute(
                "INSERT INTO jobs_mailbox_connections (
                    id, account_id, provider, provider_subject_hash, status,
                    connection_json, created_at_ms, updated_at_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                 ON CONFLICT(id) DO UPDATE SET status = excluded.status,
                    connection_json = excluded.connection_json,
                    updated_at_ms = excluded.updated_at_ms
                 WHERE jobs_mailbox_connections.account_id = excluded.account_id",
                params![
                    value.id,
                    account_id,
                    value.provider,
                    subject_hash,
                    value.status,
                    payload,
                    value.created_at_ms,
                    value.updated_at_ms,
                ],
            )?;
            Ok(value)
        }
        DbPool::Postgres(_) => {
            pool.get_pg()?.execute(
                "INSERT INTO jobs_mailbox_connections (
                    id, account_id, provider, provider_subject_hash, status,
                    connection_json, created_at_ms, updated_at_ms
                 ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
                 ON CONFLICT(id) DO UPDATE SET status = EXCLUDED.status,
                    connection_json = EXCLUDED.connection_json,
                    updated_at_ms = EXCLUDED.updated_at_ms
                 WHERE jobs_mailbox_connections.account_id = EXCLUDED.account_id",
                &[
                    &value.id,
                    &account_id,
                    &value.provider,
                    &subject_hash,
                    &value.status,
                    &payload,
                    &value.created_at_ms,
                    &value.updated_at_ms,
                ],
            )?;
            Ok(value)
        }
    })
}

pub fn delete_mailbox_connection(
    pool: &DbPool,
    account_id: &str,
    connection_id: &str,
) -> Result<bool> {
    let now = now_ms();
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
            crate::db::object_uploads::require_active_account_write_fence_sqlite_tx(
                &tx,
                account_id,
            )?;
            let raw: Option<String> = tx
                .query_row(
                    "SELECT connection_json FROM jobs_mailbox_connections
                      WHERE account_id = ?1 AND id = ?2",
                    params![account_id, connection_id],
                    |row| row.get(0),
                )
                .optional()?;
            let Some(raw) = raw else {
                tx.commit()?;
                return Ok(false);
            };
            tx.execute(
                "INSERT OR IGNORE INTO jobs_communication_write_fences (
                    account_id, connection_id, reason, created_at_ms
                 ) VALUES (?1, ?2, 'mailbox_disconnect', ?3)",
                params![account_id, connection_id, now],
            )?;
            let ambiguous: i64 = tx.query_row(
                "SELECT COUNT(*) FROM jobs_communication_actions
                  WHERE account_id = ?1 AND connection_id = ?2
                    AND status IN ('dispatching', 'side_effect_unknown')",
                params![account_id, connection_id],
                |row| row.get(0),
            )?;
            if ambiguous > 0 {
                tx.commit()?;
                anyhow::bail!(
                    "mailbox cannot be disconnected while communication outcome is unresolved"
                )
            }
            let exhausted: i64 = tx.query_row(
                "SELECT COUNT(*) FROM jobs_communication_actions
                  WHERE account_id = ?1 AND connection_id = ?2
                    AND status IN ('awaiting_approval', 'needs_input', 'approved')
                    AND (action_revision >= 9007199254740991
                         OR updated_at_ms >= 9223372036854775807)",
                params![account_id, connection_id],
                |row| row.get(0),
            )?;
            if exhausted > 0 {
                anyhow::bail!("mailbox communication revision authority is exhausted")
            }
            let mut mailbox: MailboxConnection = parse_json(raw, "mailbox connection")?;
            mailbox.status = "disconnected".to_string();
            mailbox.capabilities.clear();
            mailbox.updated_at_ms = now;
            let payload = to_json(&mailbox, "mailbox connection")?;
            tx.execute(
                "UPDATE jobs_communication_actions
                    SET status = 'cancelled', lease_owner = NULL, lease_kind = NULL,
                        lease_token_sha256 = NULL, lease_expires_at_ms = NULL,
                        active_attempt_id = NULL, approved_authority_sha256 = '',
                        approved_grant_revision = 0, approved_grant_sha256 = '',
                        action_revision = action_revision + 1,
                        updated_at_ms = MAX(?3, CASE
                          WHEN updated_at_ms < 9223372036854775807 THEN updated_at_ms + 1
                          ELSE updated_at_ms END)
                  WHERE account_id = ?1 AND connection_id = ?2
                    AND status IN ('awaiting_approval', 'needs_input', 'approved')
                    AND action_revision < 9007199254740991
                    AND updated_at_ms < 9223372036854775807",
                params![account_id, connection_id, now],
            )?;
            tx.execute(
                "DELETE FROM jobs_provider_credentials
                  WHERE account_id = ?1 AND connection_id = ?2",
                params![account_id, connection_id],
            )?;
            tx.execute(
                "DELETE FROM jobs_provider_sync_state
                  WHERE account_id = ?1 AND connection_id = ?2",
                params![account_id, connection_id],
            )?;
            let changed = tx.execute(
                "UPDATE jobs_mailbox_connections
                    SET status = 'disconnected', connection_json = ?3, updated_at_ms = ?4
                  WHERE account_id = ?1 AND id = ?2",
                params![account_id, connection_id, payload, now],
            )?;
            tx.commit()?;
            Ok(changed == 1)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            crate::db::object_uploads::require_active_account_write_fence_postgres_tx(
                &mut tx,
                account_id,
            )?;
            let Some(row) = tx.query_opt(
                "SELECT connection_json FROM jobs_mailbox_connections
                  WHERE account_id = $1 AND id = $2 FOR UPDATE",
                &[&account_id, &connection_id],
            )? else {
                tx.commit()?;
                return Ok(false);
            };
            tx.execute(
                "INSERT INTO jobs_communication_write_fences (
                    account_id, connection_id, reason, created_at_ms
                 ) VALUES ($1, $2, 'mailbox_disconnect', $3)
                 ON CONFLICT(account_id, connection_id) DO NOTHING",
                &[&account_id, &connection_id, &now],
            )?;
            let ambiguous: i64 = tx
                .query_one(
                    "SELECT COUNT(*) FROM jobs_communication_actions
                      WHERE account_id = $1 AND connection_id = $2
                        AND status IN ('dispatching', 'side_effect_unknown')",
                    &[&account_id, &connection_id],
                )?
                .get(0);
            if ambiguous > 0 {
                tx.commit()?;
                anyhow::bail!(
                    "mailbox cannot be disconnected while communication outcome is unresolved"
                )
            }
            let exhausted: i64 = tx
                .query_one(
                    "SELECT COUNT(*)::bigint FROM jobs_communication_actions
                      WHERE account_id = $1 AND connection_id = $2
                        AND status IN ('awaiting_approval', 'needs_input', 'approved')
                        AND (action_revision >= 9007199254740991
                             OR updated_at_ms >= 9223372036854775807)",
                    &[&account_id, &connection_id],
                )?
                .get(0);
            if exhausted > 0 {
                anyhow::bail!("mailbox communication revision authority is exhausted")
            }
            let mut mailbox: MailboxConnection =
                parse_json(row.get(0), "mailbox connection")?;
            mailbox.status = "disconnected".to_string();
            mailbox.capabilities.clear();
            mailbox.updated_at_ms = now;
            let payload = to_json(&mailbox, "mailbox connection")?;
            tx.execute(
                "UPDATE jobs_communication_actions
                    SET status = 'cancelled', lease_owner = NULL, lease_kind = NULL,
                        lease_token_sha256 = NULL, lease_expires_at_ms = NULL,
                        active_attempt_id = NULL, approved_authority_sha256 = '',
                        approved_grant_revision = 0, approved_grant_sha256 = '',
                        action_revision = action_revision + 1,
                        updated_at_ms = GREATEST($3, CASE
                          WHEN updated_at_ms < 9223372036854775807 THEN updated_at_ms + 1
                          ELSE updated_at_ms END)
                  WHERE account_id = $1 AND connection_id = $2
                    AND status IN ('awaiting_approval', 'needs_input', 'approved')
                    AND action_revision < 9007199254740991
                    AND updated_at_ms < 9223372036854775807",
                &[&account_id, &connection_id, &now],
            )?;
            tx.execute(
                "DELETE FROM jobs_provider_credentials
                  WHERE account_id = $1 AND connection_id = $2",
                &[&account_id, &connection_id],
            )?;
            tx.execute(
                "DELETE FROM jobs_provider_sync_state
                  WHERE account_id = $1 AND connection_id = $2",
                &[&account_id, &connection_id],
            )?;
            let changed = tx.execute(
                "UPDATE jobs_mailbox_connections
                    SET status = 'disconnected', connection_json = $3, updated_at_ms = $4
                  WHERE account_id = $1 AND id = $2",
                &[&account_id, &connection_id, &payload, &now],
            )?;
            tx.commit()?;
            Ok(changed == 1)
        }
    })
}

pub fn mark_mailbox_reauthorization_required(
    pool: &DbPool,
    account_id: &str,
    connection_id: &str,
) -> Result<bool> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            crate::db::object_uploads::require_active_account_write_fence_sqlite_tx(
                &tx, account_id,
            )?;
            let current: Option<(String, String, i64)> = tx
                .query_row(
                    "SELECT status, connection_json, updated_at_ms
                       FROM jobs_mailbox_connections
                      WHERE account_id = ?1 AND id = ?2",
                    params![account_id, connection_id],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .optional()?;
            let Some((status, payload, updated_at_ms)) = current else {
                tx.commit()?;
                return Ok(false);
            };
            let unresolved_actions: i64 = tx.query_row(
                "SELECT COUNT(*) FROM jobs_communication_actions
                  WHERE account_id = ?1 AND connection_id = ?2
                    AND status IN ('dispatching', 'side_effect_unknown')",
                params![account_id, connection_id],
                |row| row.get(0),
            )?;
            if unresolved_actions > 0 {
                anyhow::bail!(
                    "mailbox authorization cannot change while communication is unresolved"
                )
            }
            if status == "reauthorization_required" {
                tx.commit()?;
                return Ok(true);
            }
            if status != "connected" {
                tx.commit()?;
                return Ok(false);
            }
            let mut mailbox: MailboxConnection = parse_json(payload, "mailbox connection")?;
            if mailbox.id != connection_id || mailbox.status != status {
                anyhow::bail!("mailbox connection authority is inconsistent")
            }
            mailbox.status = "reauthorization_required".to_string();
            mailbox.updated_at_ms = now_ms().max(updated_at_ms.saturating_add(1));
            let payload = to_json(&mailbox, "mailbox connection")?;
            let changed = tx.execute(
                "UPDATE jobs_mailbox_connections
                    SET status = 'reauthorization_required',
                        connection_json = ?3,
                        updated_at_ms = ?4
                  WHERE account_id = ?1 AND id = ?2 AND status = 'connected'",
                params![account_id, connection_id, payload, mailbox.updated_at_ms],
            )?;
            if changed != 1 {
                anyhow::bail!("mailbox authorization authority changed")
            }
            tx.commit()?;
            Ok(true)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            crate::db::object_uploads::require_active_account_write_fence_postgres_tx(
                &mut tx, account_id,
            )?;
            let current = tx.query_opt(
                "SELECT status, connection_json, updated_at_ms
                   FROM jobs_mailbox_connections
                  WHERE account_id = $1 AND id = $2 FOR UPDATE",
                &[&account_id, &connection_id],
            )?;
            let Some(current) = current else {
                tx.commit()?;
                return Ok(false);
            };
            let status: String = current.get(0);
            let payload: String = current.get(1);
            let updated_at_ms: i64 = current.get(2);
            let unresolved_actions: i64 = tx
                .query_one(
                    "SELECT COUNT(*)::bigint FROM jobs_communication_actions
                      WHERE account_id = $1 AND connection_id = $2
                        AND status IN ('dispatching', 'side_effect_unknown')",
                    &[&account_id, &connection_id],
                )?
                .get(0);
            if unresolved_actions > 0 {
                anyhow::bail!(
                    "mailbox authorization cannot change while communication is unresolved"
                )
            }
            if status == "reauthorization_required" {
                tx.commit()?;
                return Ok(true);
            }
            if status != "connected" {
                tx.commit()?;
                return Ok(false);
            }
            let mut mailbox: MailboxConnection = parse_json(payload, "mailbox connection")?;
            if mailbox.id != connection_id || mailbox.status != status {
                anyhow::bail!("mailbox connection authority is inconsistent")
            }
            mailbox.status = "reauthorization_required".to_string();
            mailbox.updated_at_ms = now_ms().max(updated_at_ms.saturating_add(1));
            let payload = to_json(&mailbox, "mailbox connection")?;
            let changed = tx.execute(
                "UPDATE jobs_mailbox_connections
                    SET status = 'reauthorization_required',
                        connection_json = $3,
                        updated_at_ms = $4
                  WHERE account_id = $1 AND id = $2 AND status = 'connected'",
                &[&account_id, &connection_id, &payload, &mailbox.updated_at_ms],
            )?;
            if changed != 1 {
                anyhow::bail!("mailbox authorization authority changed")
            }
            tx.commit()?;
            Ok(true)
        }
    })
}

fn canonical_jobs_oauth_state_token(state_token: &str) -> Result<&str> {
    if state_token.len() < 32
        || state_token.len() > 512
        || state_token.trim() != state_token
        || !state_token
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        anyhow::bail!("OAuth state token is invalid")
    }
    Ok(state_token)
}

pub fn save_jobs_oauth_state(
    pool: &DbPool,
    account_id: &str,
    state_token: &str,
    state: &JobsOAuthState,
) -> Result<()> {
    let state_token = canonical_jobs_oauth_state_token(state_token)?;
    if !matches!(state.provider.as_str(), "gmail" | "outlook") {
        anyhow::bail!("unsupported Jobs OAuth provider")
    }
    let state_hash = private_lookup_hash("jobs-oauth-state", state_token)?;
    let payload = to_json(state, "Jobs OAuth state")?;
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            crate::db::object_uploads::require_active_account_write_fence_sqlite_tx(
                &tx, account_id,
            )?;
            tx.execute(
                "DELETE FROM jobs_oauth_states
                  WHERE account_id = ?1 AND expires_at_ms <= ?2",
                params![account_id, now_ms()],
            )?;
            let inserted = tx.execute(
                "INSERT INTO jobs_oauth_states (
                    state_hash, account_id, provider, state_json, expires_at_ms, created_at_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT(state_hash) DO NOTHING",
                params![
                    state_hash,
                    account_id,
                    state.provider,
                    payload,
                    state.expires_at_ms,
                    state.created_at_ms,
                ],
            )?;
            if inserted != 1 {
                anyhow::bail!("OAuth state token already exists")
            }
            tx.commit()?;
            Ok(())
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            crate::db::object_uploads::require_active_account_write_fence_postgres_tx(
                &mut tx, account_id,
            )?;
            tx.execute(
                "DELETE FROM jobs_oauth_states
                  WHERE account_id = $1 AND expires_at_ms <= $2",
                &[&account_id, &now_ms()],
            )?;
            let inserted = tx.execute(
                "INSERT INTO jobs_oauth_states (
                    state_hash, account_id, provider, state_json, expires_at_ms, created_at_ms
                 ) VALUES ($1, $2, $3, $4, $5, $6)
                 ON CONFLICT(state_hash) DO NOTHING",
                &[
                    &state_hash,
                    &account_id,
                    &state.provider,
                    &payload,
                    &state.expires_at_ms,
                    &state.created_at_ms,
                ],
            )?;
            if inserted != 1 {
                anyhow::bail!("OAuth state token already exists")
            }
            tx.commit()?;
            Ok(())
        }
    })
}

pub fn consume_jobs_oauth_state(
    pool: &DbPool,
    state_token: &str,
) -> Result<Option<(String, JobsOAuthState)>> {
    let state_token = canonical_jobs_oauth_state_token(state_token)?;
    let state_hash = private_lookup_hash("jobs-oauth-state", state_token)?;
    let now = now_ms();
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction()?;
            let row: Option<(String, String, i64)> = tx
                .query_row(
                    "SELECT account_id, state_json, expires_at_ms
                       FROM jobs_oauth_states
                      WHERE state_hash = ?1",
                    params![state_hash],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .optional()?;
            tx.execute(
                "DELETE FROM jobs_oauth_states WHERE state_hash = ?1",
                params![state_hash],
            )?;
            tx.commit()?;
            let Some((account_id, payload, expires_at_ms)) = row else {
                return Ok(None);
            };
            if expires_at_ms <= now {
                return Ok(None);
            }
            Ok(Some((account_id, parse_json(payload, "Jobs OAuth state")?)))
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            let row = tx.query_opt(
                "DELETE FROM jobs_oauth_states
                       WHERE state_hash = $1
                   RETURNING account_id, state_json, expires_at_ms",
                &[&state_hash],
            )?;
            tx.commit()?;
            let Some(row) = row else {
                return Ok(None);
            };
            let expires_at_ms: i64 = row.get(2);
            if expires_at_ms <= now {
                return Ok(None);
            }
            Ok(Some((
                row.get(0),
                parse_json(row.get(1), "Jobs OAuth state")?,
            )))
        }
    })
}

pub fn save_jobs_provider_credential(
    pool: &DbPool,
    account_id: &str,
    credential: &JobsProviderCredential,
) -> Result<JobsProviderCredential> {
    let mut credential = credential.clone();
    credential.scopes = credential
        .scopes
        .iter()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect();
    credential.scopes.sort();
    credential.scopes.dedup();
    credential.capabilities = credential
        .capabilities
        .iter()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect();
    credential.capabilities.sort();
    credential.capabilities.dedup();
    if credential.grant_revision < 0 {
        anyhow::bail!("provider grant revision is invalid")
    }
    if credential.grant_revision > 0 {
        let computed = communication_grant_sha256(&credential)?;
        if !credential.grant_sha256.is_empty() && credential.grant_sha256 != computed {
            anyhow::bail!("provider grant digest is invalid")
        }
        credential.grant_sha256 = computed;
    } else if !credential.grant_sha256.is_empty() {
        anyhow::bail!("legacy provider credential cannot carry a grant digest")
    }
    if !matches!(credential.provider.as_str(), "gmail" | "outlook") {
        anyhow::bail!("unsupported Jobs credential provider")
    }
    if credential.connection_id.trim().is_empty()
        || credential.provider_subject.trim().is_empty()
        || credential.access_token.trim().is_empty()
    {
        anyhow::bail!("provider credential is incomplete")
    }
    let provider_subject_hash = private_lookup_hash(
        &format!("mailbox:{}", credential.provider),
        credential.provider_subject.trim(),
    )?;
    let payload = to_json(&credential, "Jobs provider credential")?;
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
            crate::db::object_uploads::require_active_account_write_fence_sqlite_tx(
                &tx,
                account_id,
            )?;
            tx.query_row(
                "SELECT 1 FROM jobs_mailbox_connections
                  WHERE account_id = ?1 AND id = ?2 AND provider = ?3
                    AND status = 'connected'",
                params![account_id, credential.connection_id, credential.provider],
                |_| Ok(()),
            )?;
            let unresolved_actions: i64 = tx.query_row(
                "SELECT COUNT(*) FROM jobs_communication_actions
                  WHERE account_id = ?1 AND connection_id = ?2
                    AND status IN ('dispatching', 'side_effect_unknown')",
                params![account_id, credential.connection_id],
                |row| row.get(0),
            )?;
            if unresolved_actions > 0 {
                anyhow::bail!("provider grant cannot change while communication is unresolved")
            }
            tx.execute(
                "INSERT INTO jobs_provider_credentials (
                    connection_id, account_id, provider, provider_subject_hash,
                    credential_json, created_at_ms, updated_at_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                 ON CONFLICT(connection_id) DO UPDATE SET
                    provider = excluded.provider,
                    provider_subject_hash = excluded.provider_subject_hash,
                    credential_json = excluded.credential_json,
                    updated_at_ms = excluded.updated_at_ms
                 WHERE jobs_provider_credentials.account_id = excluded.account_id",
                params![
                    credential.connection_id,
                    account_id,
                    credential.provider,
                    provider_subject_hash,
                    payload,
                    credential.created_at_ms,
                    credential.updated_at_ms,
                ],
            )?;
            tx.commit()?;
            Ok(credential)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            crate::db::object_uploads::require_active_account_write_fence_postgres_tx(
                &mut tx,
                account_id,
            )?;
            tx.query_one(
                "SELECT 1 FROM jobs_mailbox_connections
                  WHERE account_id = $1 AND id = $2 AND provider = $3
                    AND status = 'connected' FOR UPDATE",
                &[&account_id, &credential.connection_id, &credential.provider],
            )?;
            let unresolved_actions: i64 = tx
                .query_one(
                    "SELECT COUNT(*)::bigint FROM jobs_communication_actions
                      WHERE account_id = $1 AND connection_id = $2
                        AND status IN ('dispatching', 'side_effect_unknown')",
                    &[&account_id, &credential.connection_id],
                )?
                .get(0);
            if unresolved_actions > 0 {
                anyhow::bail!("provider grant cannot change while communication is unresolved")
            }
            tx.execute(
                "INSERT INTO jobs_provider_credentials (
                    connection_id, account_id, provider, provider_subject_hash,
                    credential_json, created_at_ms, updated_at_ms
                 ) VALUES ($1, $2, $3, $4, $5, $6, $7)
                 ON CONFLICT(connection_id) DO UPDATE SET
                    provider = EXCLUDED.provider,
                    provider_subject_hash = EXCLUDED.provider_subject_hash,
                    credential_json = EXCLUDED.credential_json,
                    updated_at_ms = EXCLUDED.updated_at_ms
                 WHERE jobs_provider_credentials.account_id = EXCLUDED.account_id",
                &[
                    &credential.connection_id,
                    &account_id,
                    &credential.provider,
                    &provider_subject_hash,
                    &payload,
                    &credential.created_at_ms,
                    &credential.updated_at_ms,
                ],
            )?;
            tx.commit()?;
            Ok(credential)
        }
    })
}

pub fn jobs_provider_credential(
    pool: &DbPool,
    account_id: &str,
    connection_id: &str,
) -> Result<Option<JobsProviderCredential>> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let raw: Option<String> = pool
                .get()?
                .query_row(
                    "SELECT credential_json FROM jobs_provider_credentials
                      WHERE account_id = ?1 AND connection_id = ?2",
                    params![account_id, connection_id],
                    |row| row.get(0),
                )
                .optional()?;
            raw.map(|value| parse_json(value, "Jobs provider credential"))
                .transpose()
        }
        DbPool::Postgres(_) => pool
            .get_pg()?
            .query_opt(
                "SELECT credential_json FROM jobs_provider_credentials
                  WHERE account_id = $1 AND connection_id = $2",
                &[&account_id, &connection_id],
            )?
            .map(|row| parse_json(row.get(0), "Jobs provider credential"))
            .transpose(),
    })
}

/// Atomically rotates provider tokens without changing the authority grant.
/// The caller supplies the exact credential snapshot it refreshed from; a
/// disconnect, consent upgrade, or concurrent refresh makes the CAS fail.
pub fn refresh_jobs_provider_credential_cas(
    pool: &DbPool,
    account_id: &str,
    expected: &JobsProviderCredential,
    access_token: &str,
    refresh_token: Option<&str>,
    expires_at_ms: i64,
) -> Result<JobsProviderCredential> {
    if !crate::jobs_provider_auth::valid_provider_token(access_token)
        || refresh_token.is_some_and(|value| {
            !crate::jobs_provider_auth::valid_provider_token(value)
        })
        || !crate::jobs_provider_auth::valid_provider_token(&expected.access_token)
        || (!expected.refresh_token.is_empty()
            && !crate::jobs_provider_auth::valid_provider_token(&expected.refresh_token))
    {
        anyhow::bail!("provider credential refresh token is invalid")
    }
    if expected.connection_id.trim().is_empty()
        || !matches!(expected.provider.as_str(), "gmail" | "outlook")
        || expected.provider_subject.trim().is_empty()
        || expected.access_token.is_empty()
        || expires_at_ms <= now_ms()
        || expected.grant_revision < 0
    {
        anyhow::bail!("provider credential refresh CAS request is invalid")
    }
    if expected.grant_revision > 0
        && (expected.grant_sha256.len() != 64
            || communication_grant_sha256(expected)? != expected.grant_sha256)
    {
        anyhow::bail!("provider credential refresh grant digest is invalid")
    }
    let subject_hash = private_lookup_hash(
        &format!("mailbox:{}", expected.provider),
        expected.provider_subject.trim(),
    )?;

    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
            crate::db::object_uploads::require_active_account_write_fence_sqlite_tx(
                &tx,
                account_id,
            )?;
            let raw: Option<(String, String)> = tx
                .query_row(
                    "SELECT mailbox.connection_json, credential.credential_json
                       FROM jobs_provider_credentials credential
                       JOIN jobs_mailbox_connections mailbox
                         ON mailbox.account_id = credential.account_id
                        AND mailbox.id = credential.connection_id
                      WHERE credential.account_id = ?1 AND credential.connection_id = ?2
                        AND credential.provider = ?3
                        AND credential.provider_subject_hash = ?4
                        AND credential.updated_at_ms = ?5
                        AND mailbox.status = 'connected'",
                    params![
                        account_id,
                        expected.connection_id,
                        expected.provider,
                        subject_hash,
                        expected.updated_at_ms,
                    ],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()?;
            let (mailbox_json, raw) =
                raw.ok_or_else(|| anyhow::anyhow!("provider credential refresh lost CAS"))?;
            let mailbox: MailboxConnection = parse_json(mailbox_json, "mailbox connection")?;
            let current: JobsProviderCredential =
                parse_json(raw, "Jobs provider credential")?;
            validate_provider_refresh_mailbox(expected, &mailbox)?;
            validate_provider_refresh_snapshot(expected, &current)?;
            let mut updated = current;
            updated.access_token = access_token.to_string();
            if let Some(refresh_token) = refresh_token {
                updated.refresh_token = refresh_token.to_string();
            }
            updated.expires_at_ms = expires_at_ms;
            updated.updated_at_ms = now_ms().max(expected.updated_at_ms.saturating_add(1));
            let payload = to_json(&updated, "Jobs provider credential")?;
            let changed = tx.execute(
                "UPDATE jobs_provider_credentials
                    SET credential_json = ?6, updated_at_ms = ?7
                  WHERE account_id = ?1 AND connection_id = ?2 AND provider = ?3
                    AND provider_subject_hash = ?4 AND updated_at_ms = ?5",
                params![
                    account_id,
                    expected.connection_id,
                    expected.provider,
                    subject_hash,
                    expected.updated_at_ms,
                    payload,
                    updated.updated_at_ms,
                ],
            )?;
            if changed != 1 {
                anyhow::bail!("provider credential refresh lost CAS")
            }
            tx.commit()?;
            Ok(updated)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            crate::db::object_uploads::require_active_account_write_fence_postgres_tx(
                &mut tx,
                account_id,
            )?;
            let row = tx
                .query_opt(
                    "SELECT mailbox.connection_json, credential.credential_json
                       FROM jobs_provider_credentials credential
                       JOIN jobs_mailbox_connections mailbox
                         ON mailbox.account_id = credential.account_id
                        AND mailbox.id = credential.connection_id
                      WHERE credential.account_id = $1 AND credential.connection_id = $2
                        AND credential.provider = $3
                        AND credential.provider_subject_hash = $4
                        AND credential.updated_at_ms = $5
                        AND mailbox.status = 'connected'
                      FOR UPDATE OF mailbox, credential",
                    &[
                        &account_id,
                        &expected.connection_id,
                        &expected.provider,
                        &subject_hash,
                        &expected.updated_at_ms,
                    ],
                )?
                .ok_or_else(|| anyhow::anyhow!("provider credential refresh lost CAS"))?;
            let current: JobsProviderCredential =
                parse_json(row.get(1), "Jobs provider credential")?;
            let mailbox: MailboxConnection = parse_json(row.get(0), "mailbox connection")?;
            validate_provider_refresh_mailbox(expected, &mailbox)?;
            validate_provider_refresh_snapshot(expected, &current)?;
            let mut updated = current;
            updated.access_token = access_token.to_string();
            if let Some(refresh_token) = refresh_token {
                updated.refresh_token = refresh_token.to_string();
            }
            updated.expires_at_ms = expires_at_ms;
            updated.updated_at_ms = now_ms().max(expected.updated_at_ms.saturating_add(1));
            let payload = to_json(&updated, "Jobs provider credential")?;
            let changed = tx.execute(
                "UPDATE jobs_provider_credentials
                    SET credential_json = $6, updated_at_ms = $7
                  WHERE account_id = $1 AND connection_id = $2 AND provider = $3
                    AND provider_subject_hash = $4 AND updated_at_ms = $5",
                &[
                    &account_id,
                    &expected.connection_id,
                    &expected.provider,
                    &subject_hash,
                    &expected.updated_at_ms,
                    &payload,
                    &updated.updated_at_ms,
                ],
            )?;
            if changed != 1 {
                anyhow::bail!("provider credential refresh lost CAS")
            }
            tx.commit()?;
            Ok(updated)
        }
    })
}

fn validate_provider_refresh_snapshot(
    expected: &JobsProviderCredential,
    current: &JobsProviderCredential,
) -> Result<()> {
    if current.connection_id != expected.connection_id
        || current.provider != expected.provider
        || current.provider_subject != expected.provider_subject
        || current.updated_at_ms != expected.updated_at_ms
        || current.access_token != expected.access_token
        || current.refresh_token != expected.refresh_token
        || current.grant_revision != expected.grant_revision
        || current.grant_sha256 != expected.grant_sha256
        || current.scopes != expected.scopes
        || current.capabilities != expected.capabilities
    {
        anyhow::bail!("provider credential refresh lost CAS")
    }
    Ok(())
}

fn validate_provider_refresh_mailbox(
    expected: &JobsProviderCredential,
    mailbox: &MailboxConnection,
) -> Result<()> {
    let mut capabilities = mailbox.capabilities.clone();
    capabilities.sort();
    capabilities.dedup();
    let mut expected_capabilities = expected.capabilities.clone();
    expected_capabilities.sort();
    expected_capabilities.dedup();
    if mailbox.id != expected.connection_id
        || mailbox.provider != expected.provider
        || mailbox.status != "connected"
        || (expected.grant_revision > 0 && capabilities != expected_capabilities)
        || (expected.grant_revision == 0
            && !expected_capabilities
                .iter()
                .all(|capability| capabilities.contains(capability)))
    {
        anyhow::bail!("provider credential refresh mailbox authority changed")
    }
    Ok(())
}

pub fn list_integrations(pool: &DbPool, account_id: &str) -> Result<Vec<JobsIntegration>> {
    let mut integrations: Vec<JobsIntegration> = list_payloads(
        pool,
        account_id,
        "jobs_integrations",
        "integration_json",
        "provider ASC",
        "Jobs integration",
    )?;
    integrations.retain(|item| {
        matches!(
            item.provider.as_str(),
            "google_calendar" | "outlook_calendar"
        )
    });
    for (provider, capabilities) in [
        ("google_calendar", vec!["interview_calendar"]),
        ("outlook_calendar", vec!["interview_calendar"]),
    ] {
        if !integrations.iter().any(|item| item.provider == provider) {
            integrations.push(JobsIntegration {
                id: format!("{account_id}:{provider}"),
                provider: provider.to_string(),
                status: "disconnected".to_string(),
                account_label: String::new(),
                capabilities: capabilities.into_iter().map(str::to_string).collect(),
                updated_at_ms: 0,
            });
        }
    }
    Ok(integrations)
}

pub fn save_integration(
    pool: &DbPool,
    account_id: &str,
    integration: &JobsIntegration,
) -> Result<JobsIntegration> {
    let mut value = integration.clone();
    if value.id.is_empty() {
        value.id = uuid::Uuid::new_v4().to_string();
    }
    value.updated_at_ms = now_ms();
    let payload = to_json(&value, "Jobs integration")?;
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            pool.get()?.execute(
                "INSERT INTO jobs_integrations(id, account_id, provider, status, integration_json, updated_at_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT(account_id, provider) DO UPDATE SET
                    status = excluded.status, integration_json = excluded.integration_json,
                    updated_at_ms = excluded.updated_at_ms",
                params![
                    value.id,
                    account_id,
                    value.provider,
                    value.status,
                    payload,
                    value.updated_at_ms,
                ],
            )?;
            Ok(value)
        }
        DbPool::Postgres(_) => {
            pool.get_pg()?.execute(
                "INSERT INTO jobs_integrations(id, account_id, provider, status, integration_json, updated_at_ms)
                 VALUES ($1, $2, $3, $4, $5, $6)
                 ON CONFLICT(account_id, provider) DO UPDATE SET
                    status = EXCLUDED.status, integration_json = EXCLUDED.integration_json,
                    updated_at_ms = EXCLUDED.updated_at_ms",
                &[
                    &value.id,
                    &account_id,
                    &value.provider,
                    &value.status,
                    &payload,
                    &value.updated_at_ms,
                ],
            )?;
            Ok(value)
        }
    })
}

fn list_payloads<T: DeserializeOwned>(
    pool: &DbPool,
    account_id: &str,
    table: &str,
    payload_column: &str,
    order_by: &str,
    label: &str,
) -> Result<Vec<T>> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get()?;
            let sql = format!(
                "SELECT {payload_column} FROM {table} WHERE account_id = ?1 ORDER BY {order_by}"
            );
            let mut stmt = conn.prepare(&sql)?;
            let raws = stmt
                .query_map(params![account_id], |row| row.get::<_, String>(0))?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            raws.into_iter().map(|raw| parse_json(raw, label)).collect()
        }
        DbPool::Postgres(_) => {
            let sql = format!(
                "SELECT {payload_column} FROM {table} WHERE account_id = $1 ORDER BY {order_by}"
            );
            pool.get_pg()?
                .query(&sql, &[&account_id])?
                .into_iter()
                .map(|row| parse_json(row.get(0), label))
                .collect()
        }
    })
}

pub fn list_run_events(pool: &DbPool, account_id: &str, run_id: &str) -> Result<Vec<RunEvent>> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get()?;
            let mut stmt = conn.prepare(
                "SELECT id, run_id, event_type, event_json, created_at_ms
                   FROM jobs_run_events WHERE account_id = ?1 AND run_id = ?2
                  ORDER BY created_at_ms ASC",
            )?;
            let rows = stmt.query_map(params![account_id, run_id], |row| {
                let raw: String = row.get(3)?;
                Ok(RunEvent {
                    id: row.get(0)?,
                    run_id: row.get(1)?,
                    event_type: row.get(2)?,
                    event: parse_json_lossy(&raw).unwrap_or_else(|| json!({})),
                    created_at_ms: row.get(4)?,
                })
            })?;
            rows.collect::<std::result::Result<Vec<_>, _>>()
                .context("list Jobs run events")
        }
        DbPool::Postgres(_) => pool
            .get_pg()?
            .query(
                "SELECT id, run_id, event_type, event_json, created_at_ms
                   FROM jobs_run_events WHERE account_id = $1 AND run_id = $2
                  ORDER BY created_at_ms ASC",
                &[&account_id, &run_id],
            )?
            .into_iter()
            .map(|row| {
                let raw: String = row.get(3);
                Ok(RunEvent {
                    id: row.get(0),
                    run_id: row.get(1),
                    event_type: row.get(2),
                    event: parse_json(raw, "Jobs run event")?,
                    created_at_ms: row.get(4),
                })
            })
            .collect(),
    })
}

pub fn list_account_run_events(pool: &DbPool, account_id: &str) -> Result<Vec<RunEvent>> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get()?;
            let mut stmt = conn.prepare(
                "SELECT id, run_id, event_type, event_json, created_at_ms
                   FROM jobs_run_events WHERE account_id = ?1
                  ORDER BY created_at_ms ASC",
            )?;
            let rows = stmt.query_map(params![account_id], |row| {
                let raw: String = row.get(3)?;
                Ok(RunEvent {
                    id: row.get(0)?,
                    run_id: row.get(1)?,
                    event_type: row.get(2)?,
                    event: parse_json_lossy(&raw).unwrap_or_else(|| json!({})),
                    created_at_ms: row.get(4)?,
                })
            })?;
            rows.collect::<std::result::Result<Vec<_>, _>>()
                .context("list account Jobs run events")
        }
        DbPool::Postgres(_) => pool
            .get_pg()?
            .query(
                "SELECT id, run_id, event_type, event_json, created_at_ms
                   FROM jobs_run_events WHERE account_id = $1
                  ORDER BY created_at_ms ASC",
                &[&account_id],
            )?
            .into_iter()
            .map(|row| {
                let raw: String = row.get(3);
                Ok(RunEvent {
                    id: row.get(0),
                    run_id: row.get(1),
                    event_type: row.get(2),
                    event: parse_json(raw, "Jobs run event")?,
                    created_at_ms: row.get(4),
                })
            })
            .collect(),
    })
}

pub fn save_run_event(
    pool: &DbPool,
    account_id: &str,
    run_id: &str,
    event_type: &str,
    event: Value,
) -> Result<RunEvent> {
    let value = RunEvent {
        id: uuid::Uuid::new_v4().to_string(),
        run_id: run_id.to_string(),
        event_type: event_type.to_string(),
        event,
        created_at_ms: now_ms(),
    };
    let payload = to_json(&value.event, "Jobs run event")?;
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            crate::db::object_uploads::require_active_account_write_fence_sqlite_tx(
                &tx, account_id,
            )?;
            tx.execute(
                "INSERT INTO jobs_run_events(id, account_id, run_id, event_type, event_json, created_at_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![value.id, account_id, value.run_id, value.event_type, payload, value.created_at_ms],
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
                "INSERT INTO jobs_run_events(id, account_id, run_id, event_type, event_json, created_at_ms)
                 VALUES ($1, $2, $3, $4, $5, $6)",
                &[&value.id, &account_id, &value.run_id, &value.event_type, &payload, &value.created_at_ms],
            )?;
            tx.commit()?;
            Ok(value)
        }
    })
}
