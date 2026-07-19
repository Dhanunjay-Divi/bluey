
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
            pool.get()?.execute(
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
            Ok(value)
        }
        DbPool::Postgres(_) => {
            pool.get_pg()?.execute(
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
            pool.get()?.execute(
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
            Ok(value)
        }
        DbPool::Postgres(_) => {
            pool.get_pg()?.execute(
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
            Ok(value)
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

fn application_submission_evidence_complete(
    pool: &DbPool,
    account_id: &str,
    application: &JobApplication,
) -> Result<bool> {
    let Some(resume_version_id) = application.resume_version_id.as_deref() else {
        return Ok(false);
    };
    let evidence = list_application_evidence(pool, account_id, Some(&application.id))?;
    let matching_resume = evidence.iter().any(|item| {
        item.kind == "resume" && item.resume_version_id.as_deref() == Some(resume_version_id)
    });
    let confirmation = evidence
        .iter()
        .any(|item| item.kind == "submission_confirmation");
    Ok(matching_resume && confirmation)
}

struct PreparedSubmissionEvidence {
    value: ApplicationEvidence,
    provider_event_hash: String,
    payload: String,
}

fn prepare_submission_evidence(
    application_id: &str,
    request_fingerprint: &str,
    evidence: &[ApplicationEvidence],
    now: i64,
) -> Result<Vec<PreparedSubmissionEvidence>> {
    if evidence.is_empty() || evidence.len() > 9 {
        anyhow::bail!("invalid final submission evidence")
    }
    let mut resume_count = 0usize;
    let mut confirmation_count = 0usize;
    let mut prepared = Vec::with_capacity(evidence.len());
    for (index, item) in evidence.iter().enumerate() {
        let mut value = item.clone();
        if value.application_id != application_id
            || !matches!(
                value.kind.as_str(),
                "resume" | "cover_letter" | "attachment" | "submission_confirmation"
            )
        {
            anyhow::bail!("invalid final submission evidence")
        }
        resume_count += usize::from(value.kind == "resume");
        confirmation_count += usize::from(value.kind == "submission_confirmation");
        validate_document_evidence(&value)?;
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
    if resume_count != 1 || confirmation_count != 1 {
        anyhow::bail!("final submission needs one resume and one confirmation")
    }
    Ok(prepared)
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
                    tx.commit()?;
                    return Ok(SubmissionFinalizeResult::Replayed(application));
                }
                anyhow::bail!("application already has a different final receipt")
            }
            if application.run_id.as_deref() != Some(run_id) {
                anyhow::bail!("application browser run does not match final receipt")
            }
            validate_application_transition(&application.state, "submitted")?;
            let resume_version_id = application
                .resume_version_id
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("application resume version is missing"))?;
            if !prepared_evidence.iter().any(|item| {
                item.value.kind == "resume"
                    && item.value.resume_version_id.as_deref() == Some(resume_version_id)
            }) {
                anyhow::bail!("final receipt resume does not match the application")
            }
            if runner == "cloud" {
                let phase: Option<String> = tx
                    .query_row(
                        "SELECT phase FROM jobs_execution_leases
                          WHERE account_id = ?1 AND application_id = ?2 AND run_id = ?3",
                        params![account_id, application_id, run_id],
                        |row| row.get(0),
                    )
                    .optional()?;
                if phase.as_deref() != Some("submitted") {
                    anyhow::bail!("matching cloud execution lease is not terminal submitted")
                }
            } else if tx.execute(
                "UPDATE jobs_local_run_tickets
                    SET status = 'complete', updated_at_ms = ?5
                  WHERE id = ?1 AND account_id = ?2 AND application_id = ?3
                    AND ticket_hash = ?4 AND expires_at_ms > ?5
                    AND status IN ('claimed', 'needs_input')",
                params![
                    run_id,
                    account_id,
                    application_id,
                    local_ticket_hash.expect("local ticket checked above"),
                    now
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
                  WHERE account_id = ?1 AND application_id = ?2",
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
            tx.commit()?;
            Ok(SubmissionFinalizeResult::Committed(application))
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
            if application.state == "submitted" {
                if application
                    .receipt
                    .get("_bluey_server_submission_fingerprint_v1")
                    .and_then(Value::as_str)
                    == Some(request_fingerprint)
                {
                    tx.commit()?;
                    return Ok(SubmissionFinalizeResult::Replayed(application));
                }
                anyhow::bail!("application already has a different final receipt")
            }
            if application.run_id.as_deref() != Some(run_id) {
                anyhow::bail!("application browser run does not match final receipt")
            }
            validate_application_transition(&application.state, "submitted")?;
            let resume_version_id = application
                .resume_version_id
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("application resume version is missing"))?;
            if !prepared_evidence.iter().any(|item| {
                item.value.kind == "resume"
                    && item.value.resume_version_id.as_deref() == Some(resume_version_id)
            }) {
                anyhow::bail!("final receipt resume does not match the application")
            }
            if runner == "cloud" {
                let phase = tx
                    .query_opt(
                        "SELECT phase FROM jobs_execution_leases
                          WHERE account_id = $1 AND application_id = $2 AND run_id = $3
                          FOR UPDATE",
                        &[&account_id, &application_id, &run_id],
                    )?
                    .map(|row| row.get::<_, String>(0));
                if phase.as_deref() != Some("submitted") {
                    anyhow::bail!("matching cloud execution lease is not terminal submitted")
                }
            } else if tx.execute(
                "UPDATE jobs_local_run_tickets
                    SET status = 'complete', updated_at_ms = $5
                  WHERE id = $1 AND account_id = $2 AND application_id = $3
                    AND ticket_hash = $4 AND expires_at_ms > $5
                    AND status IN ('claimed', 'needs_input')",
                &[
                    &run_id,
                    &account_id,
                    &application_id,
                    &local_ticket_hash.expect("local ticket checked above"),
                    &now,
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
                  WHERE account_id = $1 AND application_id = $2",
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
            tx.commit()?;
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
            tx.commit()?;
            Ok(value)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
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
        DbPool::Sqlite(_) => Ok(pool.get()?.execute(
            "DELETE FROM jobs_application_identities WHERE account_id = ?1 AND id = ?2",
            params![account_id, identity_id],
        )? > 0),
        DbPool::Postgres(_) => Ok(pool.get_pg()?.execute(
            "DELETE FROM jobs_application_identities WHERE account_id = $1 AND id = $2",
            &[&account_id, &identity_id],
        )? > 0),
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

fn mailbox_connection_by_subject(
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
        value.capabilities = vec!["status_sync".to_string(), "follow_ups".to_string()];
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
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => Ok(pool.get()?.execute(
            "DELETE FROM jobs_mailbox_connections WHERE account_id = ?1 AND id = ?2",
            params![account_id, connection_id],
        )? > 0),
        DbPool::Postgres(_) => Ok(pool.get_pg()?.execute(
            "DELETE FROM jobs_mailbox_connections WHERE account_id = $1 AND id = $2",
            &[&account_id, &connection_id],
        )? > 0),
    })
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
            pool.get()?.execute(
                "INSERT INTO jobs_run_events(id, account_id, run_id, event_type, event_json, created_at_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![value.id, account_id, value.run_id, value.event_type, payload, value.created_at_ms],
            )?;
            Ok(value)
        }
        DbPool::Postgres(_) => {
            pool.get_pg()?.execute(
                "INSERT INTO jobs_run_events(id, account_id, run_id, event_type, event_json, created_at_ms)
                 VALUES ($1, $2, $3, $4, $5, $6)",
                &[&value.id, &account_id, &value.run_id, &value.event_type, &payload, &value.created_at_ms],
            )?;
            Ok(value)
        }
    })
}
