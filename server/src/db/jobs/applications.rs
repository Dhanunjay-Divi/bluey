
pub fn list_applications(pool: &DbPool, account_id: &str) -> Result<Vec<JobApplication>> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get()?;
            let mut stmt = conn.prepare(
                "SELECT id, job_id, application_json FROM jobs_applications
                  WHERE account_id = ?1 ORDER BY updated_at_ms DESC",
            )?;
            let raws = stmt
                .query_map(params![account_id], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                })?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            raws.into_iter()
                .map(|(id, job_id, raw)| {
                    parse_application_json(raw, &id, &job_id, "job application")
                })
                .collect()
        }
        DbPool::Postgres(_) => pool
            .get_pg()?
            .query(
                "SELECT id, job_id, application_json FROM jobs_applications
                  WHERE account_id = $1 ORDER BY updated_at_ms DESC",
                &[&account_id],
            )?
            .into_iter()
            .map(|row| {
                parse_application_json(row.get(2), row.get(0), row.get(1), "job application")
            })
            .collect(),
    })
}

pub fn get_application(
    pool: &DbPool,
    account_id: &str,
    application_id: &str,
) -> Result<Option<JobApplication>> {
    crate::db::run_blocking_db(|| {
        match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get()?;
            let raw: Option<(String, String, String)> = conn
                .query_row(
                    "SELECT id, job_id, application_json FROM jobs_applications WHERE account_id = ?1 AND id = ?2",
                    params![account_id, application_id],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .optional()?;
            raw.map(|(id, job_id, value)| {
                parse_application_json(value, &id, &job_id, "job application")
            })
                .transpose()
        }
        DbPool::Postgres(_) => pool
            .get_pg()?
            .query_opt(
                "SELECT id, job_id, application_json FROM jobs_applications WHERE account_id = $1 AND id = $2",
                &[&account_id, &application_id],
            )?
            .map(|row| {
                parse_application_json(row.get(2), row.get(0), row.get(1), "job application")
            })
            .transpose(),
    }
    })
}

pub fn get_resume_version(
    pool: &DbPool,
    account_id: &str,
    resume_version_id: &str,
) -> Result<Option<ResumeVersion>> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get()?;
            conn.query_row(
                "SELECT id, job_id, version_no, mode, content_json, diff_json,
                        claim_ids_json, checksum, created_at_ms
                   FROM jobs_resume_versions WHERE account_id = ?1 AND id = ?2",
                params![account_id, resume_version_id],
                resume_from_sqlite_row,
            )
            .optional()
            .context("get resume version")
        }
        DbPool::Postgres(_) => pool
            .get_pg()?
            .query_opt(
                "SELECT id, job_id, version_no, mode, content_json, diff_json,
                        claim_ids_json, checksum, created_at_ms
                   FROM jobs_resume_versions WHERE account_id = $1 AND id = $2",
                &[&account_id, &resume_version_id],
            )?
            .map(resume_from_pg_row)
            .transpose(),
    })
}

pub fn list_resume_versions(pool: &DbPool, account_id: &str) -> Result<Vec<ResumeVersion>> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get()?;
            let mut stmt = conn.prepare(
                "SELECT id, job_id, version_no, mode, content_json, diff_json,
                        claim_ids_json, checksum, created_at_ms
                   FROM jobs_resume_versions WHERE account_id = ?1
                  ORDER BY created_at_ms ASC, version_no ASC",
            )?;
            let rows = stmt.query_map(params![account_id], resume_from_sqlite_row)?;
            rows.collect::<std::result::Result<Vec<_>, _>>()
                .context("list resume versions")
        }
        DbPool::Postgres(_) => pool
            .get_pg()?
            .query(
                "SELECT id, job_id, version_no, mode, content_json, diff_json,
                        claim_ids_json, checksum, created_at_ms
                   FROM jobs_resume_versions WHERE account_id = $1
                  ORDER BY created_at_ms ASC, version_no ASC",
                &[&account_id],
            )?
            .into_iter()
            .map(resume_from_pg_row)
            .collect(),
    })
}

fn resume_from_sqlite_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ResumeVersion> {
    let content: String = row.get(4)?;
    let diff: String = row.get(5)?;
    let claims: String = row.get(6)?;
    Ok(ResumeVersion {
        id: row.get(0)?,
        job_id: row.get(1)?,
        version_no: row.get(2)?,
        mode: row.get(3)?,
        content: parse_json_lossy(&content).unwrap_or_else(|| json!({})),
        diff: parse_json_lossy(&diff).unwrap_or_else(|| json!({})),
        claim_ids: parse_json_lossy(&claims).unwrap_or_default(),
        checksum: row.get(7)?,
        created_at_ms: row.get(8)?,
    })
}

fn resume_from_pg_row(row: postgres::Row) -> Result<ResumeVersion> {
    let content: String = row.get(4);
    let diff: String = row.get(5);
    let claims: String = row.get(6);
    Ok(ResumeVersion {
        id: row.get(0),
        job_id: row.get(1),
        version_no: row.get(2),
        mode: row.get(3),
        content: parse_json(content, "resume content")?,
        diff: parse_json(diff, "resume diff")?,
        claim_ids: parse_json(claims, "resume claims")?,
        checksum: row.get(7),
        created_at_ms: row.get(8),
    })
}

pub fn prepare_application(
    pool: &DbPool,
    account_id: &str,
    job_id: &str,
    mode: &str,
    submission_mode: &str,
) -> Result<(JobApplication, ResumeVersion)> {
    let prepared = prepare_application_draft(pool, account_id, job_id, mode, submission_mode)?;
    finalize_prepared_application(
        pool,
        account_id,
        &prepared,
        prepared.baseline_resume.content.clone(),
        prepared.baseline_resume.diff.clone(),
        json!({
            "status": "deterministic",
            "provider": "bluey-evidence-planner",
            "claims_added": 0,
        }),
    )
}

pub fn prepare_application_draft(
    pool: &DbPool,
    account_id: &str,
    job_id: &str,
    mode: &str,
    submission_mode: &str,
) -> Result<PreparedApplicationDraft> {
    prepare_application_inner(pool, account_id, job_id, mode, submission_mode)
}

fn prepare_application_inner(
    pool: &DbPool,
    account_id: &str,
    job_id: &str,
    mode: &str,
    submission_mode: &str,
) -> Result<PreparedApplicationDraft> {
    let profile = get_profile(pool, account_id, "")?;
    let posting =
        get_posting(pool, account_id, job_id)?.ok_or_else(|| anyhow::anyhow!("job not found"))?;
    let posting_fingerprint = posting_snapshot_fingerprint(&posting)?;
    let existing = find_application_for_job_with_revision(pool, account_id, job_id)?;
    let existing_application = existing
        .as_ref()
        .map(|(application, _)| application.clone());
    let expected_application = existing.as_ref().map(|(_, revision)| revision.clone());
    let eligibility = evaluate_job_eligibility(
        pool,
        account_id,
        &posting,
        false,
        existing_application
            .as_ref()
            .map(|application| application.id.as_str()),
    )?;
    if !eligibility.can_prepare {
        anyhow::bail!(eligibility_error_message(&eligibility))
    }
    // Resume contact data is user-editable and may differ from the Bluey login.
    // Only the authenticated account email may bootstrap a verified identity.
    let login_email = account_login_email(pool, account_id)?;
    let _ = ensure_primary_application_identity(pool, account_id, &login_email)?;
    let identities = list_application_identities(pool, account_id)?;
    let track = list_tracks(pool, account_id)?
        .into_iter()
        .find(|track| track.id == posting.track_id)
        .ok_or_else(|| anyhow::anyhow!("choose an active Career Track before preparing"))?;
    let track_identity_id = track.application_identity_id.as_deref();
    let application_identity =
        selected_application_identity(track_identity_id, &identities).ok_or_else(
            || anyhow::anyhow!("verify an application email before preparing this packet"),
        )?
        .clone();
    let facts = list_facts(pool, account_id)?;
    let approved_fact_ids: Vec<String> = facts
        .iter()
        .filter(|fact| fact.verification_status == "confirmed")
        .map(|fact| fact.id.clone())
        .collect();
    let confirmed_facts_fingerprint = confirmed_facts_fingerprint(&facts);
    let evidence_revision = build_profile_evidence_revision(
        account_id,
        &profile,
        &facts,
        &track,
        &application_identity,
        &eligibility.experience_evidence,
    )?;
    let tailored_resume = tailor_resume(&profile, &posting, mode);
    let truth_fingerprint = candidate_truth_fingerprint(&profile);
    let content = json!({
        "target": {
            "job_id": posting.id,
            "company": posting.company,
            "title": posting.title,
            "location": posting.location,
        },
        "contact": {
            "name": profile.full_name,
            "email": application_identity.email,
            "phone": profile.phone,
            "location": profile.current_location,
            "linkedin_url": profile.linkedin_url,
            "portfolio_url": profile.portfolio_url,
        },
        "headline": tailored_resume.headline,
        "summary": tailored_resume.summary,
        "skills": tailored_resume.skills,
        "employment": tailored_resume.employment,
        "education": profile.education,
        "projects": tailored_resume.projects,
        "certifications": profile.certifications,
        "source_resume_name": profile.source_resume_name,
        "provenance": {
            "confirmed_fact_ids": approved_fact_ids,
            "mode": mode,
            "generated_for_job_id": posting.id,
            "application_identity_id": application_identity.id,
            "career_track_id": posting.track_id,
            "candidate_truth_fingerprint": truth_fingerprint,
            "candidate_truth_fingerprint_version": 1,
            "confirmed_facts_fingerprint": confirmed_facts_fingerprint,
            "confirmed_facts_fingerprint_version": 1,
            "job_snapshot_fingerprint": posting_fingerprint,
            "evidence_revision_id": evidence_revision.id,
            "evidence_content_hash": evidence_revision.content_hash,
        },
    });
    let diff = tailored_resume.diff;
    let checksum_source = format!("{}|{}|{}", account_id, job_id, content);
    let checksum = hex::encode(Sha256::digest(checksum_source.as_bytes()));
    let now = now_ms();
    let resume = ResumeVersion {
        id: String::new(),
        job_id: job_id.to_string(),
        version_no: 0,
        mode: mode.to_string(),
        content,
        diff,
        claim_ids: Vec::new(),
        checksum,
        created_at_ms: now,
    };
    let mut application = existing_application.unwrap_or(JobApplication {
        id: uuid::Uuid::new_v4().to_string(),
        job_id: job_id.to_string(),
        resume_version_id: None,
        state: "preparing".to_string(),
        submission_mode: submission_mode.to_string(),
        match_score: posting.match_score,
        answers: Vec::new(),
        cover_letter: String::new(),
        receipt: json!({}),
        run_id: None,
        created_at_ms: now,
        updated_at_ms: now,
        submitted_at_ms: None,
    });
    let remembered_answers = answers_for_posting(pool, account_id, &posting)?;
    for remembered in remembered_answers {
        let key = remembered
            .get("key")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let already_answered = application.answers.iter().any(|answer| {
            ["key", "question", "field", "name"].iter().any(|field| {
                answer
                    .get(*field)
                    .and_then(Value::as_str)
                    .is_some_and(|value| normalize_answer_memory_key(value) == key)
            })
        });
        if !already_answered {
            application.answers.push(remembered);
        }
    }
    application.resume_version_id = None;
    application.state = "preparing".to_string();
    application.submission_mode = submission_mode.to_string();
    application.match_score = posting.match_score;
    application.updated_at_ms = now;
    application.receipt = json!({
        "job_snapshot": posting,
        "resume_version_id": Value::Null,
        "career_track_id": posting.track_id,
        "candidate_truth_fingerprint": truth_fingerprint,
        "candidate_truth_fingerprint_version": 1,
        "confirmed_facts_fingerprint": confirmed_facts_fingerprint,
        "confirmed_facts_fingerprint_version": 1,
        "job_snapshot_fingerprint": posting_fingerprint,
        "evidence_revision_id": evidence_revision.id,
        "evidence_content_hash": evidence_revision.content_hash,
        "application_identity": {
            "id": application_identity.id,
            "email": application_identity.email,
            "label": application_identity.label,
            "verified": true,
        },
        "prepared_at_ms": now,
        "final_answers": application.answers,
        "eligibility": eligibility,
        "cover_letter_status": if application.cover_letter.trim().is_empty() { "not_included" } else { "included" },
        "metering": {
            "status": "pending_generation",
            "canonical_job_key": posting.canonical_key,
        },
        "resume_generation": {
            "status": "pending",
        },
        "confirmation": Value::Null,
    });
    Ok(PreparedApplicationDraft {
        application,
        baseline_resume: resume,
        profile,
        facts,
        track,
        identity: application_identity,
        evidence_revision,
        posting,
        expected_application,
    })
}

pub fn finalize_prepared_application(
    pool: &DbPool,
    account_id: &str,
    prepared: &PreparedApplicationDraft,
    content: Value,
    diff: Value,
    generation: Value,
) -> Result<(JobApplication, ResumeVersion)> {
    let mut application = prepared.application.clone();
    let baseline = &prepared.baseline_resume;
    if application.state != "preparing" {
        anyhow::bail!("application is not waiting for resume generation")
    }
    let posting = get_posting(pool, account_id, &application.job_id)?
        .ok_or_else(|| anyhow::anyhow!("job not found"))?;
    let expected_posting_fingerprint = posting_snapshot_fingerprint(&prepared.posting)?;
    if posting_snapshot_fingerprint(&posting)? != expected_posting_fingerprint {
        anyhow::bail!("job posting changed while the application packet was generated")
    }
    if application
        .receipt
        .get("job_snapshot_fingerprint")
        .and_then(Value::as_str)
        != Some(expected_posting_fingerprint.as_str())
        || content
            .pointer("/provenance/job_snapshot_fingerprint")
            .and_then(Value::as_str)
            != Some(expected_posting_fingerprint.as_str())
    {
        anyhow::bail!("generated resume does not match the job posting snapshot")
    }
    if baseline.job_id != posting.id {
        anyhow::bail!("application baseline targets a different job")
    }
    if content.pointer("/target/job_id").and_then(Value::as_str) != Some(posting.id.as_str()) {
        anyhow::bail!("generated resume targets a different job")
    }
    let expected_truth_fingerprint = application
        .receipt
        .get("candidate_truth_fingerprint")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("application draft has no truth fingerprint"))?
        .to_string();
    if content
        .pointer("/provenance/candidate_truth_fingerprint")
        .and_then(Value::as_str)
        != Some(expected_truth_fingerprint.as_str())
    {
        anyhow::bail!("generated resume does not match the candidate truth snapshot")
    }
    if candidate_truth_fingerprint(&prepared.profile) != expected_truth_fingerprint {
        anyhow::bail!("application draft does not match the candidate truth snapshot")
    }
    let current_profile = get_profile(pool, account_id, "")?;
    if candidate_truth_fingerprint(&current_profile) != expected_truth_fingerprint {
        anyhow::bail!("candidate profile changed while the application packet was generated")
    }
    let expected_facts_fingerprint = application
        .receipt
        .get("confirmed_facts_fingerprint")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("application draft has no confirmed-facts fingerprint"))?
        .to_string();
    if content
        .pointer("/provenance/confirmed_facts_fingerprint")
        .and_then(Value::as_str)
        != Some(expected_facts_fingerprint.as_str())
    {
        anyhow::bail!("generated resume does not match the confirmed candidate facts")
    }
    let expected_identity_id = application
        .receipt
        .pointer("/application_identity/id")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("application draft has no verified identity"))?
        .to_string();
    let expected_identity_email = application
        .receipt
        .pointer("/application_identity/email")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("application draft has no verified identity email"))?
        .to_string();
    if content
        .pointer("/provenance/application_identity_id")
        .and_then(Value::as_str)
        != Some(expected_identity_id.as_str())
        || content.pointer("/contact/email").and_then(Value::as_str)
            != Some(expected_identity_email.as_str())
    {
        anyhow::bail!("generated resume does not match the verified application identity")
    }
    if content
        .pointer("/provenance/career_track_id")
        .and_then(Value::as_str)
        != Some(prepared.track.id.as_str())
        || posting.track_id != prepared.track.id
    {
        anyhow::bail!("generated resume does not match the selected Career Track")
    }
    if content
        .pointer("/provenance/evidence_revision_id")
        .and_then(Value::as_str)
        != Some(prepared.evidence_revision.id.as_str())
        || content
            .pointer("/provenance/evidence_content_hash")
            .and_then(Value::as_str)
            != Some(prepared.evidence_revision.content_hash.as_str())
        || application
            .receipt
            .get("evidence_revision_id")
            .and_then(Value::as_str)
            != Some(prepared.evidence_revision.id.as_str())
    {
        anyhow::bail!("generated resume does not match its candidate evidence revision")
    }
    validate_prepared_resume_content(
        &content,
        &baseline.content,
        &prepared.profile,
        &prepared.identity,
        &posting,
    )?;

    let checksum_source = format!("{}|{}|{}", account_id, posting.id, content);
    let checksum = hex::encode(Sha256::digest(checksum_source.as_bytes()));
    let mut eligibility = evaluate_job_eligibility(
        pool,
        account_id,
        &posting,
        false,
        Some(application.id.as_str()),
    )?;
    eligibility.evidence_revision_id = Some(prepared.evidence_revision.id.clone());
    eligibility.tailored_packet_coverage = Some(tailored_packet_coverage(
        &posting,
        &prepared.profile,
        &content,
    ));
    let claim_evidence = build_resume_claim_evidence(ResumeClaimEvidenceContext {
        account_id,
        job_id: &posting.id,
        resume_version_id: "",
        content: &content,
        profile: &prepared.profile,
        facts: &prepared.facts,
        track: &prepared.track,
        identity: &prepared.identity,
        evidence_revision: &prepared.evidence_revision,
    })?;
    let auto_submit_eligible =
        application.submission_mode == "auto_submit" && eligibility.can_auto_submit;
    application.state = if auto_submit_eligible {
        "queued".to_string()
    } else {
        "awaiting_review".to_string()
    };
    application.updated_at_ms = now_ms();
    let receipt = application
        .receipt
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("application receipt must be an object"))?;
    receipt.insert(
        "eligibility".to_string(),
        serde_json::to_value(eligibility)?,
    );
    receipt.insert("resume_generation".to_string(), generation);
    receipt.insert(
        "prepared_at_ms".to_string(),
        json!(application.updated_at_ms),
    );
    receipt.insert(
        "metering".to_string(),
        json!({
            "status": if auto_submit_eligible { "counts_when_queued" } else { "counts_when_approved_or_downloaded" },
            "canonical_job_key": posting.canonical_key,
        }),
    );
    commit_prepared_application(
        pool,
        account_id,
        &mut application,
        &prepared.expected_application,
        baseline,
        content,
        diff,
        checksum,
        &expected_truth_fingerprint,
        &expected_posting_fingerprint,
        &expected_identity_id,
        &expected_identity_email,
        &expected_facts_fingerprint,
        &prepared.evidence_revision,
        &claim_evidence,
        &prepared.facts,
    )
}

#[allow(clippy::too_many_arguments)]
fn commit_prepared_application(
    pool: &DbPool,
    account_id: &str,
    application: &mut JobApplication,
    expected: &Option<ExpectedApplicationRevision>,
    baseline: &ResumeVersion,
    content: Value,
    diff: Value,
    checksum: String,
    expected_truth_fingerprint: &str,
    expected_posting_fingerprint: &str,
    expected_identity_id: &str,
    expected_identity_email: &str,
    expected_facts_fingerprint: &str,
    expected_evidence_revision: &ProfileEvidenceRevision,
    claim_evidence: &[ResumeClaimEvidence],
    expected_facts: &[CareerFact],
) -> Result<(JobApplication, ResumeVersion)> {
    validate_application_state(&application.state)?;
    let content_json = to_json(&content, "resume content")?;
    let diff_json = to_json(&diff, "resume diff")?;
    let claim_ids = claim_evidence
        .iter()
        .map(|claim| claim.claim_id.clone())
        .collect::<Vec<_>>();
    let claim_ids_json = to_json(&claim_ids, "resume claims")?;
    let resume_created_at_ms = now_ms();

    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let current_profile: CareerProfile = tx
                .query_row(
                    "SELECT profile_json FROM jobs_profiles WHERE account_id = ?1",
                    params![account_id],
                    |row| row.get::<_, String>(0),
                )
                .optional()?
                .map(|raw| parse_json(raw, "Jobs profile during application finalization"))
                .transpose()?
                .unwrap_or_else(|| default_profile(""));
            if candidate_truth_fingerprint(&current_profile) != expected_truth_fingerprint {
                anyhow::bail!(
                    "candidate profile changed while the application packet was generated"
                )
            }
            let current_posting: JobPosting = tx
                .query_row(
                    "SELECT posting_json FROM jobs_postings
                      WHERE account_id = ?1 AND id = ?2",
                    params![account_id, application.job_id],
                    |row| row.get::<_, String>(0),
                )
                .optional()?
                .map(|raw| parse_json(raw, "Jobs posting during application finalization"))
                .transpose()?
                .ok_or_else(|| anyhow::anyhow!("job not found during application finalization"))?;
            if posting_snapshot_fingerprint(&current_posting)? != expected_posting_fingerprint {
                anyhow::bail!("job posting changed while the application packet was generated")
            }
            let current_track: Option<CareerTrack> = tx
                .query_row(
                    "SELECT track_json FROM jobs_tracks
                      WHERE account_id = ?1 AND id = ?2",
                    params![account_id, current_posting.track_id],
                    |row| row.get::<_, String>(0),
                )
                .optional()?
                .map(|raw| parse_json(raw, "career track during application finalization"))
                .transpose()?;
            let track_identity_id = current_track
                .as_ref()
                .and_then(|track| track.application_identity_id.clone());
            let mut identity_stmt = tx.prepare(
                "SELECT identity_json, verification_status, is_default
                   FROM jobs_application_identities WHERE account_id = ?1",
            )?;
            let identity_rows = identity_stmt
                .query_map(params![account_id], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, i64>(2)? != 0,
                    ))
                })?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            let identities = identity_rows
                .into_iter()
                .map(|(raw, status, is_default)| {
                    parse_application_identity_row(raw, status, is_default)
                })
                .collect::<Result<Vec<_>>>()?;
            let current_identity =
                selected_application_identity(track_identity_id.as_deref(), &identities);
            if current_identity.is_none_or(|identity| {
                identity.id != expected_identity_id || identity.email != expected_identity_email
            }) {
                anyhow::bail!("verified application identity changed during generation")
            }
            let mut fact_stmt = tx.prepare(
                "SELECT id, category, label, value_json, source, verification_status,
                        confirmed_at_ms, confirmed_by, schema_version,
                        created_at_ms, updated_at_ms
                   FROM jobs_facts
                  WHERE account_id = ?1 AND verification_status = 'confirmed'",
            )?;
            let current_facts = fact_stmt
                .query_map(params![account_id], fact_from_sqlite_row)?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            let mut current_fact_ids = current_facts
                .iter()
                .map(|fact| fact.id.as_str())
                .collect::<Vec<_>>();
            current_fact_ids.sort_unstable();
            let mut expected_fact_ids = expected_facts
                .iter()
                .filter(|fact| fact.verification_status == "confirmed")
                .map(|fact| fact.id.as_str())
                .collect::<Vec<_>>();
            expected_fact_ids.sort_unstable();
            if current_fact_ids != expected_fact_ids
                || confirmed_facts_fingerprint(&current_facts) != expected_facts_fingerprint
            {
                anyhow::bail!("confirmed candidate facts changed during resume generation")
            }
            let preferences = tx
                .query_row(
                    "SELECT preferences_json FROM jobs_preferences WHERE account_id = ?1",
                    params![account_id],
                    |row| row.get::<_, String>(0),
                )
                .optional()?
                .map(|raw| {
                    parse_json::<JobPreferences>(raw, "Jobs preferences during finalization")
                })
                .transpose()?
                .unwrap_or_default();
            let preferences = enforce_job_preference_safety(preferences);
            let mut reservation_stmt = tx.prepare(
                "SELECT id, application_id, company_key, period_key, runner, status,
                        reserved_at_ms, updated_at_ms
                   FROM jobs_attempt_reservations WHERE account_id = ?1",
            )?;
            let reservations = reservation_stmt
                .query_map(params![account_id], |row| {
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
                })?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            let mut authority_stmt = tx.prepare(
                "SELECT s.id, s.provider, s.status, s.health,
                        m.availability_status, m.last_seen_at_ms, m.last_seen_run_id
                   FROM jobs_discovery_memberships m
                   JOIN jobs_discovery_sources s ON s.id = m.source_id
                  WHERE m.account_id = ?1 AND m.job_id = ?2",
            )?;
            let authorities = authority_stmt
                .query_map(params![account_id, application.job_id], |row| {
                    Ok(JobDiscoveryAuthority {
                        source_id: row.get(0)?,
                        provider: row.get(1)?,
                        source_status: row.get(2)?,
                        source_health: row.get(3)?,
                        membership_status: row.get(4)?,
                        last_seen_at_ms: row.get(5)?,
                        last_seen_run_id: row.get(6)?,
                    })
                })?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            enforce_application_finalization_eligibility(
                application,
                &current_posting,
                &current_profile,
                &preferences,
                current_track.as_ref(),
                &reservations,
                &authorities,
            )?;
            let current_track = current_track
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("Career Track was removed during generation"))?;
            let current_identity = current_identity
                .ok_or_else(|| anyhow::anyhow!("application identity changed during generation"))?;
            let current_experience =
                role_experience_evidence(&current_profile, Some(current_track), &current_posting);
            let current_evidence = build_profile_evidence_revision(
                account_id,
                &current_profile,
                &current_facts,
                current_track,
                current_identity,
                &current_experience,
            )?;
            if current_evidence.id != expected_evidence_revision.id
                || current_evidence.content_hash != expected_evidence_revision.content_hash
            {
                anyhow::bail!("candidate evidence changed during resume generation")
            }
            drop(identity_stmt);
            drop(fact_stmt);
            drop(reservation_stmt);
            drop(authority_stmt);
            let stored_evidence =
                persist_evidence_revision_sqlite(&tx, account_id, &current_evidence)?;
            let resume = if let Some(existing) = tx
                .query_row(
                    "SELECT id, job_id, version_no, mode, content_json, diff_json,
                            claim_ids_json, checksum, created_at_ms
                       FROM jobs_resume_versions
                      WHERE account_id = ?1 AND job_id = ?2 AND checksum = ?3",
                    params![account_id, application.job_id, checksum],
                    resume_from_sqlite_row,
                )
                .optional()?
            {
                existing
            } else {
                let version_no: i64 = tx.query_row(
                    "SELECT COALESCE(MAX(version_no), 0) + 1 FROM jobs_resume_versions
                      WHERE account_id = ?1 AND job_id = ?2",
                    params![account_id, application.job_id],
                    |row| row.get(0),
                )?;
                let id = uuid::Uuid::new_v4().to_string();
                tx.execute(
                    "INSERT INTO jobs_resume_versions (
                        id, account_id, job_id, version_no, mode, content_json,
                        diff_json, claim_ids_json, checksum, created_at_ms
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                    params![
                        id,
                        account_id,
                        application.job_id,
                        version_no,
                        baseline.mode,
                        content_json,
                        diff_json,
                        claim_ids_json,
                        checksum,
                        resume_created_at_ms,
                    ],
                )?;
                ResumeVersion {
                    id,
                    job_id: application.job_id.clone(),
                    version_no,
                    mode: baseline.mode.clone(),
                    content: content.clone(),
                    diff: diff.clone(),
                    claim_ids: claim_ids.clone(),
                    checksum: checksum.clone(),
                    created_at_ms: resume_created_at_ms,
                }
            };

            application.resume_version_id = Some(resume.id.clone());
            application
                .receipt
                .as_object_mut()
                .ok_or_else(|| anyhow::anyhow!("application receipt must be an object"))?
                .insert("resume_version_id".to_string(), json!(resume.id));
            let receipt = application
                .receipt
                .as_object_mut()
                .ok_or_else(|| anyhow::anyhow!("application receipt must be an object"))?;
            receipt.insert(
                "evidence_revision".to_string(),
                json!({
                    "id": stored_evidence.id,
                    "revision_no": stored_evidence.revision_no,
                    "content_hash": stored_evidence.content_hash,
                }),
            );
            receipt.insert("claim_ids".to_string(), json!(claim_ids));
            persist_claim_evidence_sqlite(
                &tx,
                account_id,
                &resume.id,
                claim_evidence,
            )?;
            let payload = to_json(application, "job application")?;
            let changed = match expected {
                Some(expected) => tx.execute(
                    "UPDATE jobs_applications SET resume_version_id = ?5, state = ?6,
                            application_json = ?7, updated_at_ms = ?8, submitted_at_ms = ?9
                      WHERE account_id = ?1 AND job_id = ?2 AND id = ?3
                        AND state = ?4 AND updated_at_ms = ?10 AND application_json = ?11",
                    params![
                        account_id,
                        application.job_id,
                        expected.id,
                        expected.state,
                        application.resume_version_id,
                        application.state,
                        payload,
                        application.updated_at_ms,
                        application.submitted_at_ms,
                        expected.updated_at_ms,
                        expected.payload,
                    ],
                )?,
                None => tx.execute(
                    "INSERT INTO jobs_applications (
                        id, account_id, job_id, resume_version_id, state,
                        application_json, created_at_ms, updated_at_ms, submitted_at_ms
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                     ON CONFLICT(account_id, job_id) DO NOTHING",
                    params![
                        application.id,
                        account_id,
                        application.job_id,
                        application.resume_version_id,
                        application.state,
                        payload,
                        application.created_at_ms,
                        application.updated_at_ms,
                        application.submitted_at_ms,
                    ],
                )?,
            };
            if changed != 1 {
                anyhow::bail!("application changed while resume generation was in progress");
            }
            tx.commit()?;
            Ok((application.clone(), resume))
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn
                .build_transaction()
                .isolation_level(postgres::IsolationLevel::Serializable)
                .start()?;
            let current_profile: CareerProfile = tx
                .query_opt(
                    "SELECT profile_json FROM jobs_profiles WHERE account_id = $1 FOR SHARE",
                    &[&account_id],
                )?
                .map(|row| {
                    parse_json(
                        row.get::<_, String>(0),
                        "Jobs profile during application finalization",
                    )
                })
                .transpose()?
                .unwrap_or_else(|| default_profile(""));
            if candidate_truth_fingerprint(&current_profile) != expected_truth_fingerprint {
                anyhow::bail!(
                    "candidate profile changed while the application packet was generated"
                )
            }
            let current_posting: JobPosting = tx
                .query_opt(
                    "SELECT posting_json FROM jobs_postings
                      WHERE account_id = $1 AND id = $2 FOR SHARE",
                    &[&account_id, &application.job_id],
                )?
                .map(|row| {
                    parse_json(
                        row.get::<_, String>(0),
                        "Jobs posting during application finalization",
                    )
                })
                .transpose()?
                .ok_or_else(|| anyhow::anyhow!("job not found during application finalization"))?;
            if posting_snapshot_fingerprint(&current_posting)? != expected_posting_fingerprint {
                anyhow::bail!("job posting changed while the application packet was generated")
            }
            let current_track: Option<CareerTrack> = tx
                .query_opt(
                    "SELECT track_json FROM jobs_tracks
                      WHERE account_id = $1 AND id = $2 FOR SHARE",
                    &[&account_id, &current_posting.track_id],
                )?
                .map(|row| {
                    parse_json(
                        row.get::<_, String>(0),
                        "career track during application finalization",
                    )
                })
                .transpose()?;
            let track_identity_id = current_track
                .as_ref()
                .and_then(|track| track.application_identity_id.clone());
            let identities = tx
                .query(
                    "SELECT identity_json, verification_status, is_default
                       FROM jobs_application_identities
                      WHERE account_id = $1 FOR SHARE",
                    &[&account_id],
                )?
                .into_iter()
                .map(|row| {
                    parse_application_identity_row(
                        row.get(0),
                        row.get(1),
                        row.get::<_, i32>(2) != 0,
                    )
                })
                .collect::<Result<Vec<_>>>()?;
            let current_identity =
                selected_application_identity(track_identity_id.as_deref(), &identities);
            if current_identity.is_none_or(|identity| {
                identity.id != expected_identity_id || identity.email != expected_identity_email
            }) {
                anyhow::bail!("verified application identity changed during generation")
            }
            let current_facts = tx
                .query(
                    "SELECT id, category, label, value_json, source, verification_status,
                            confirmed_at_ms, confirmed_by, schema_version,
                            created_at_ms, updated_at_ms
                       FROM jobs_facts
                      WHERE account_id = $1 AND verification_status = 'confirmed'
                      FOR SHARE",
                    &[&account_id],
                )?
                .into_iter()
                .map(fact_from_pg_row)
                .collect::<Result<Vec<_>>>()?;
            let mut current_fact_ids = current_facts
                .iter()
                .map(|fact| fact.id.as_str())
                .collect::<Vec<_>>();
            current_fact_ids.sort_unstable();
            let mut expected_fact_ids = expected_facts
                .iter()
                .filter(|fact| fact.verification_status == "confirmed")
                .map(|fact| fact.id.as_str())
                .collect::<Vec<_>>();
            expected_fact_ids.sort_unstable();
            if current_fact_ids != expected_fact_ids
                || confirmed_facts_fingerprint(&current_facts) != expected_facts_fingerprint
            {
                anyhow::bail!("confirmed candidate facts changed during resume generation")
            }
            let preferences = tx
                .query_opt(
                    "SELECT preferences_json FROM jobs_preferences
                      WHERE account_id = $1 FOR SHARE",
                    &[&account_id],
                )?
                .map(|row| {
                    parse_json::<JobPreferences>(
                        row.get::<_, String>(0),
                        "Jobs preferences during finalization",
                    )
                })
                .transpose()?
                .unwrap_or_default();
            let preferences = enforce_job_preference_safety(preferences);
            let reservations = tx
                .query(
                    "SELECT id, application_id, company_key, period_key, runner, status,
                            reserved_at_ms, updated_at_ms
                       FROM jobs_attempt_reservations
                      WHERE account_id = $1 FOR SHARE",
                    &[&account_id],
                )?
                .into_iter()
                .map(|row| AttemptReservation {
                    id: row.get(0),
                    application_id: row.get(1),
                    company_key: row.get(2),
                    period_key: row.get(3),
                    runner: row.get(4),
                    status: row.get(5),
                    reserved_at_ms: row.get(6),
                    updated_at_ms: row.get(7),
                })
                .collect::<Vec<_>>();
            let authorities = tx
                .query(
                    "SELECT s.id, s.provider, s.status, s.health,
                            m.availability_status, m.last_seen_at_ms, m.last_seen_run_id
                       FROM jobs_discovery_memberships m
                       JOIN jobs_discovery_sources s ON s.id = m.source_id
                      WHERE m.account_id = $1 AND m.job_id = $2
                      FOR SHARE OF m, s",
                    &[&account_id, &application.job_id],
                )?
                .into_iter()
                .map(|row| JobDiscoveryAuthority {
                    source_id: row.get(0),
                    provider: row.get(1),
                    source_status: row.get(2),
                    source_health: row.get(3),
                    membership_status: row.get(4),
                    last_seen_at_ms: row.get(5),
                    last_seen_run_id: row.get(6),
                })
                .collect::<Vec<_>>();
            enforce_application_finalization_eligibility(
                application,
                &current_posting,
                &current_profile,
                &preferences,
                current_track.as_ref(),
                &reservations,
                &authorities,
            )?;
            let current_track = current_track
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("Career Track was removed during generation"))?;
            let current_identity = current_identity
                .ok_or_else(|| anyhow::anyhow!("application identity changed during generation"))?;
            let current_experience =
                role_experience_evidence(&current_profile, Some(current_track), &current_posting);
            let current_evidence = build_profile_evidence_revision(
                account_id,
                &current_profile,
                &current_facts,
                current_track,
                current_identity,
                &current_experience,
            )?;
            if current_evidence.id != expected_evidence_revision.id
                || current_evidence.content_hash != expected_evidence_revision.content_hash
            {
                anyhow::bail!("candidate evidence changed during resume generation")
            }
            let stored_evidence =
                persist_evidence_revision_postgres(&mut tx, account_id, &current_evidence)?;
            let resume = if let Some(row) = tx.query_opt(
                "SELECT id, job_id, version_no, mode, content_json, diff_json,
                        claim_ids_json, checksum, created_at_ms
                   FROM jobs_resume_versions
                  WHERE account_id = $1 AND job_id = $2 AND checksum = $3",
                &[&account_id, &application.job_id, &checksum],
            )? {
                resume_from_pg_row(row)?
            } else {
                let version_no: i64 = tx
                    .query_one(
                        "SELECT COALESCE(MAX(version_no), 0) + 1 FROM jobs_resume_versions
                          WHERE account_id = $1 AND job_id = $2",
                        &[&account_id, &application.job_id],
                    )?
                    .get(0);
                let id = uuid::Uuid::new_v4().to_string();
                let inserted = tx.query_opt(
                    "INSERT INTO jobs_resume_versions (
                        id, account_id, job_id, version_no, mode, content_json,
                        diff_json, claim_ids_json, checksum, created_at_ms
                     ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
                     ON CONFLICT (account_id, job_id, checksum) DO NOTHING
                     RETURNING id, job_id, version_no, mode, content_json, diff_json,
                               claim_ids_json, checksum, created_at_ms",
                    &[
                        &id,
                        &account_id,
                        &application.job_id,
                        &version_no,
                        &baseline.mode,
                        &content_json,
                        &diff_json,
                        &claim_ids_json,
                        &checksum,
                        &resume_created_at_ms,
                    ],
                )?;
                match inserted {
                    Some(row) => resume_from_pg_row(row)?,
                    None => resume_from_pg_row(tx.query_one(
                        "SELECT id, job_id, version_no, mode, content_json, diff_json,
                                claim_ids_json, checksum, created_at_ms
                           FROM jobs_resume_versions
                          WHERE account_id = $1 AND job_id = $2 AND checksum = $3",
                        &[&account_id, &application.job_id, &checksum],
                    )?)?,
                }
            };

            application.resume_version_id = Some(resume.id.clone());
            application
                .receipt
                .as_object_mut()
                .ok_or_else(|| anyhow::anyhow!("application receipt must be an object"))?
                .insert("resume_version_id".to_string(), json!(resume.id));
            let receipt = application
                .receipt
                .as_object_mut()
                .ok_or_else(|| anyhow::anyhow!("application receipt must be an object"))?;
            receipt.insert(
                "evidence_revision".to_string(),
                json!({
                    "id": stored_evidence.id,
                    "revision_no": stored_evidence.revision_no,
                    "content_hash": stored_evidence.content_hash,
                }),
            );
            receipt.insert("claim_ids".to_string(), json!(claim_ids));
            persist_claim_evidence_postgres(
                &mut tx,
                account_id,
                &resume.id,
                claim_evidence,
            )?;
            let payload = to_json(application, "job application")?;
            let changed = match expected {
                Some(expected) => tx.execute(
                    "UPDATE jobs_applications SET resume_version_id = $5, state = $6,
                            application_json = $7, updated_at_ms = $8, submitted_at_ms = $9
                      WHERE account_id = $1 AND job_id = $2 AND id = $3
                        AND state = $4 AND updated_at_ms = $10 AND application_json = $11",
                    &[
                        &account_id,
                        &application.job_id,
                        &expected.id,
                        &expected.state,
                        &application.resume_version_id,
                        &application.state,
                        &payload,
                        &application.updated_at_ms,
                        &application.submitted_at_ms,
                        &expected.updated_at_ms,
                        &expected.payload,
                    ],
                )?,
                None => tx.execute(
                    "INSERT INTO jobs_applications (
                        id, account_id, job_id, resume_version_id, state,
                        application_json, created_at_ms, updated_at_ms, submitted_at_ms
                     ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
                     ON CONFLICT(account_id, job_id) DO NOTHING",
                    &[
                        &application.id,
                        &account_id,
                        &application.job_id,
                        &application.resume_version_id,
                        &application.state,
                        &payload,
                        &application.created_at_ms,
                        &application.updated_at_ms,
                        &application.submitted_at_ms,
                    ],
                )?,
            };
            if changed != 1 {
                anyhow::bail!("application changed while resume generation was in progress");
            }
            tx.commit()?;
            Ok((application.clone(), resume))
        }
    })
}

fn answers_for_posting(
    pool: &DbPool,
    account_id: &str,
    posting: &JobPosting,
) -> Result<Vec<Value>> {
    let company_scope = normalize_company_scope(&posting.company);
    let mut selected: BTreeMap<String, (u8, AnswerMemory)> = BTreeMap::new();
    for answer in list_answer_memory(pool, account_id)? {
        if !answer.confirmed {
            continue;
        }
        let rank = match answer.scope.as_str() {
            "company" if answer.scope_id.as_deref() == Some(company_scope.as_str()) => 3,
            "track"
                if !posting.track_id.is_empty()
                    && answer.scope_id.as_deref() == Some(posting.track_id.as_str()) =>
            {
                2
            }
            "account" => 1,
            _ => continue,
        };
        match selected.get(&answer.key) {
            Some((existing_rank, _)) if *existing_rank >= rank => {}
            _ => {
                selected.insert(answer.key.clone(), (rank, answer));
            }
        }
    }
    Ok(selected
        .into_values()
        .map(|(_, answer)| {
            json!({
                "key": answer.key,
                "question": answer.question,
                "value": answer.value,
                "source": "answer_memory",
                "memory_id": answer.id,
                "scope": answer.scope,
                "scope_id": answer.scope_id,
            })
        })
        .collect())
}

fn normalize_company_scope(company: &str) -> String {
    let mut value = String::new();
    let mut separator = false;
    for character in company.chars().flat_map(char::to_lowercase) {
        if character.is_ascii_alphanumeric() {
            if separator && !value.is_empty() {
                value.push('-');
            }
            value.push(character);
            separator = false;
        } else {
            separator = true;
        }
    }
    value
}

fn account_login_email(pool: &DbPool, account_id: &str) -> Result<String> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => pool
            .get()?
            .query_row(
                "SELECT email FROM accounts WHERE id = ?1",
                params![account_id],
                |row| row.get(0),
            )
            .context("get Bluey account email"),
        DbPool::Postgres(_) => Ok(pool
            .get_pg()?
            .query_one("SELECT email FROM accounts WHERE id = $1", &[&account_id])?
            .get(0)),
    })
}

fn find_application_for_job_with_revision(
    pool: &DbPool,
    account_id: &str,
    job_id: &str,
) -> Result<Option<(JobApplication, ExpectedApplicationRevision)>> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get()?;
            let raw: Option<(String, String, i64, String)> = conn
                .query_row(
                    "SELECT id, state, updated_at_ms, application_json
                       FROM jobs_applications WHERE account_id = ?1 AND job_id = ?2",
                    params![account_id, job_id],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
                )
                .optional()?;
            raw.map(|(id, state, updated_at_ms, value)| {
                let application =
                    parse_application_json(value.clone(), &id, job_id, "job application")?;
                Ok((
                    application,
                    ExpectedApplicationRevision {
                        id,
                        state,
                        updated_at_ms,
                        payload: value,
                    },
                ))
            })
            .transpose()
        }
        DbPool::Postgres(_) => pool
            .get_pg()?
            .query_opt(
                "SELECT id, state, updated_at_ms, application_json
                   FROM jobs_applications WHERE account_id = $1 AND job_id = $2",
                &[&account_id, &job_id],
            )?
            .map(|row| {
                let value: String = row.get(3);
                let id: String = row.get(0);
                let application =
                    parse_application_json(value.clone(), &id, job_id, "job application")?;
                Ok((
                    application,
                    ExpectedApplicationRevision {
                        id,
                        state: row.get(1),
                        updated_at_ms: row.get(2),
                        payload: value,
                    },
                ))
            })
            .transpose(),
    })
}

fn save_application(
    pool: &DbPool,
    account_id: &str,
    application: &JobApplication,
) -> Result<JobApplication> {
    validate_application_state(&application.state)?;
    let payload = to_json(application, "job application")?;
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            pool.get()?.execute(
                "INSERT INTO jobs_applications (
                    id, account_id, job_id, resume_version_id, state,
                    application_json, created_at_ms, updated_at_ms, submitted_at_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                 ON CONFLICT(account_id, job_id) DO UPDATE SET
                    resume_version_id = excluded.resume_version_id,
                    state = excluded.state,
                    application_json = excluded.application_json,
                    updated_at_ms = excluded.updated_at_ms,
                    submitted_at_ms = excluded.submitted_at_ms",
                params![
                    application.id,
                    account_id,
                    application.job_id,
                    application.resume_version_id,
                    application.state,
                    payload,
                    application.created_at_ms,
                    application.updated_at_ms,
                    application.submitted_at_ms,
                ],
            )?;
            Ok(application.clone())
        }
        DbPool::Postgres(_) => {
            pool.get_pg()?.execute(
                "INSERT INTO jobs_applications (
                    id, account_id, job_id, resume_version_id, state,
                    application_json, created_at_ms, updated_at_ms, submitted_at_ms
                 ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
                 ON CONFLICT(account_id, job_id) DO UPDATE SET
                    resume_version_id = EXCLUDED.resume_version_id,
                    state = EXCLUDED.state,
                    application_json = EXCLUDED.application_json,
                    updated_at_ms = EXCLUDED.updated_at_ms,
                    submitted_at_ms = EXCLUDED.submitted_at_ms",
                &[
                    &application.id,
                    &account_id,
                    &application.job_id,
                    &application.resume_version_id,
                    &application.state,
                    &payload,
                    &application.created_at_ms,
                    &application.updated_at_ms,
                    &application.submitted_at_ms,
                ],
            )?;
            Ok(application.clone())
        }
    })
}

pub fn update_application(
    pool: &DbPool,
    account_id: &str,
    application_id: &str,
    state: &str,
    submission_mode: Option<&str>,
) -> Result<Option<JobApplication>> {
    validate_application_state(state)?;
    let Some(mut application) = get_application(pool, account_id, application_id)? else {
        return Ok(None);
    };
    if state == "submitted" {
        anyhow::bail!(
            "only a verified runner receipt can finalize a submitted application"
        )
    }
    validate_application_transition(&application.state, state)?;
    if matches!(state, "queued" | "running") {
        let posting = get_posting(pool, account_id, &application.job_id)?
            .ok_or_else(|| anyhow::anyhow!("job not found"))?;
        let eligibility =
            evaluate_job_eligibility(pool, account_id, &posting, true, Some(&application.id))?;
        if !eligibility.can_queue_local {
            anyhow::bail!(eligibility_error_message(&eligibility))
        }
        if let Some(receipt) = application.receipt.as_object_mut() {
            receipt.insert(
                "eligibility".to_string(),
                serde_json::to_value(eligibility)?,
            );
        }
    }
    application.state = state.to_string();
    if let Some(mode) = submission_mode {
        if !matches!(mode, "review_first" | "auto_submit") {
            anyhow::bail!("invalid application submission mode");
        }
        application.submission_mode = mode.to_string();
    }
    application.updated_at_ms = now_ms();
    save_application(pool, account_id, &application).map(Some)
}

pub fn assign_application_run(
    pool: &DbPool,
    account_id: &str,
    application_id: &str,
    run_id: &str,
) -> Result<Option<JobApplication>> {
    let Some(mut application) = get_application(pool, account_id, application_id)? else {
        return Ok(None);
    };
    application.run_id = Some(run_id.to_string());
    application.updated_at_ms = now_ms();
    save_application(pool, account_id, &application).map(Some)
}

pub fn replace_application_receipt(
    pool: &DbPool,
    account_id: &str,
    application_id: &str,
    receipt: Value,
) -> Result<Option<JobApplication>> {
    let Some(mut application) = get_application(pool, account_id, application_id)? else {
        return Ok(None);
    };
    application.receipt = receipt;
    application.updated_at_ms = now_ms();
    save_application(pool, account_id, &application).map(Some)
}

pub fn validate_application_state(state: &str) -> Result<()> {
    const STATES: &[&str] = &[
        "matched",
        "preparing",
        "needs_confirmation",
        "awaiting_review",
        "queued",
        "running",
        "needs_input",
        "side_effect_unknown",
        "submitted",
        "failed",
    ];
    if STATES.contains(&state) {
        Ok(())
    } else {
        anyhow::bail!("invalid application state")
    }
}

fn validate_application_transition(current: &str, next: &str) -> Result<()> {
    if current == next {
        return Ok(());
    }
    let allowed = match current {
        "matched" => matches!(next, "preparing" | "awaiting_review" | "failed"),
        "preparing" => matches!(
            next,
            "needs_confirmation" | "awaiting_review" | "queued" | "failed"
        ),
        "needs_confirmation" => matches!(next, "awaiting_review" | "failed"),
        "awaiting_review" => matches!(next, "queued" | "failed"),
        "queued" => matches!(
            next,
            "awaiting_review" | "running" | "needs_input" | "failed"
        ),
        "running" => matches!(
            next,
            "needs_input" | "side_effect_unknown" | "submitted" | "failed"
        ),
        "needs_input" => matches!(
            next,
            "queued" | "running" | "side_effect_unknown" | "failed"
        ),
        "side_effect_unknown" => matches!(next, "needs_input" | "submitted" | "failed"),
        "failed" => matches!(next, "queued" | "awaiting_review"),
        "submitted" => false,
        _ => false,
    };
    if allowed {
        Ok(())
    } else {
        anyhow::bail!("invalid application state transition")
    }
}

fn current_period() -> (i64, i64) {
    let now = Utc::now();
    let start = Utc
        .with_ymd_and_hms(now.year(), now.month(), 1, 0, 0, 0)
        .single()
        .unwrap_or(now);
    let (year, month) = if now.month() == 12 {
        (now.year() + 1, 1)
    } else {
        (now.year(), now.month() + 1)
    };
    let end = Utc
        .with_ymd_and_hms(year, month, 1, 0, 0, 0)
        .single()
        .unwrap_or(now);
    (start.timestamp_millis(), end.timestamp_millis())
}

pub fn get_entitlement(pool: &DbPool, account_id: &str) -> Result<JobsEntitlement> {
    let (period_start, period_end) = current_period();
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get()?;
            conn.execute(
                "INSERT INTO jobs_entitlements (
                    account_id, plan, track_limit, monthly_packet_limit, used_packets,
                    period_start_ms, period_end_ms, local_browser, cloud_browser, updated_at_ms
                 ) VALUES (?1, 'free', 1, 5, 0, ?2, ?3, 0, 0, ?4)
                 ON CONFLICT(account_id) DO NOTHING",
                params![account_id, period_start, period_end, now_ms()],
            )?;
            conn.execute(
                "UPDATE jobs_entitlements SET used_packets = 0, period_start_ms = ?2,
                    period_end_ms = ?3, updated_at_ms = ?4
                  WHERE account_id = ?1 AND period_end_ms <= ?4",
                params![account_id, period_start, period_end, now_ms()],
            )?;
            conn.query_row(
                "SELECT plan, track_limit, monthly_packet_limit, used_packets,
                        period_start_ms, period_end_ms, local_browser, cloud_browser
                   FROM jobs_entitlements WHERE account_id = ?1",
                params![account_id],
                |row| {
                    Ok(entitlement_with_policy(
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                        row.get::<_, i64>(6)? != 0,
                        row.get::<_, i64>(7)? != 0,
                    ))
                },
            )
            .context("get Jobs entitlement")
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            conn.execute(
                "INSERT INTO jobs_entitlements (
                    account_id, plan, track_limit, monthly_packet_limit, used_packets,
                    period_start_ms, period_end_ms, local_browser, cloud_browser, updated_at_ms
                 ) VALUES ($1, 'free', 1, 5, 0, $2, $3, 0, 0, $4)
                 ON CONFLICT(account_id) DO NOTHING",
                &[&account_id, &period_start, &period_end, &now_ms()],
            )?;
            conn.execute(
                "UPDATE jobs_entitlements SET used_packets = 0, period_start_ms = $2,
                    period_end_ms = $3, updated_at_ms = $4
                  WHERE account_id = $1 AND period_end_ms <= $4",
                &[&account_id, &period_start, &period_end, &now_ms()],
            )?;
            let row = conn.query_one(
                "SELECT plan, track_limit, monthly_packet_limit, used_packets,
                        period_start_ms, period_end_ms, local_browser, cloud_browser
                   FROM jobs_entitlements WHERE account_id = $1",
                &[&account_id],
            )?;
            Ok(entitlement_with_policy(
                row.get(0),
                row.get(1),
                row.get(2),
                row.get(3),
                row.get(4),
                row.get(5),
                row.get::<_, i32>(6) != 0,
                row.get::<_, i32>(7) != 0,
            ))
        }
    })
}

pub fn set_entitlement_plan(
    pool: &DbPool,
    account_id: &str,
    plan: &str,
) -> Result<JobsEntitlement> {
    let policy = plan_policy(plan);
    let track_limit = policy.track_limit;
    let packet_limit = policy.packet_limit;
    let local_browser = i32::from(policy.local_browser);
    let cloud_browser = i32::from(policy.cloud_browser);
    let _ = get_entitlement(pool, account_id)?;
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            pool.get()?.execute(
                "UPDATE jobs_entitlements SET plan = ?2, track_limit = ?3,
                    monthly_packet_limit = ?4, local_browser = ?5, cloud_browser = ?6,
                    updated_at_ms = ?7 WHERE account_id = ?1",
                params![
                    account_id,
                    plan,
                    track_limit,
                    packet_limit,
                    local_browser,
                    cloud_browser,
                    now_ms(),
                ],
            )?;
            get_entitlement(pool, account_id)
        }
        DbPool::Postgres(_) => {
            pool.get_pg()?.execute(
                "UPDATE jobs_entitlements SET plan = $2, track_limit = $3,
                    monthly_packet_limit = $4, local_browser = $5, cloud_browser = $6,
                    updated_at_ms = $7 WHERE account_id = $1",
                &[
                    &account_id,
                    &plan,
                    &track_limit,
                    &packet_limit,
                    &local_browser,
                    &cloud_browser,
                    &now_ms(),
                ],
            )?;
            get_entitlement(pool, account_id)
        }
    })
}

pub fn commit_packet(
    pool: &DbPool,
    account_id: &str,
    application_id: &str,
) -> Result<PacketCommitResult> {
    let application = get_application(pool, account_id, application_id)?
        .ok_or_else(|| anyhow::anyhow!("application not found"))?;
    if application.resume_version_id.is_none() {
        anyhow::bail!("application packet has no job-specific resume");
    }
    let _ = get_entitlement(pool, account_id)?;
    let now = now_ms();
    let metering_key = format!("jobs-packet:{}", application.job_id);
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
            if let Some((included, amount_cents)) = tx
                .query_row(
                    "SELECT included, amount_cents FROM jobs_packet_metering
                      WHERE account_id = ?1 AND job_id = ?2",
                    params![account_id, application.job_id],
                    |row| Ok((row.get::<_, i64>(0)? != 0, row.get::<_, i64>(1)?)),
                )
                .optional()?
            {
                let (used, limit): (i64, i64) = tx.query_row(
                    "SELECT used_packets, monthly_packet_limit FROM jobs_entitlements WHERE account_id = ?1",
                    params![account_id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )?;
                return Ok(PacketCommitResult {
                    newly_metered: false,
                    included,
                    amount_cents,
                    used_packets: used,
                    monthly_packet_limit: limit,
                });
            }
            let (used, limit, period_start): (i64, i64, i64) = tx.query_row(
                "SELECT used_packets, monthly_packet_limit, period_start_ms
                   FROM jobs_entitlements WHERE account_id = ?1",
                params![account_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )?;
            let allowance: Option<(String, i64)> = tx
                .query_row(
                    "SELECT status, period_start_ms
                       FROM jobs_generation_allowance_reservations
                      WHERE account_id = ?1 AND job_id = ?2",
                    params![account_id, application.job_id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()?;
            if allowance
                .as_ref()
                .is_some_and(|(status, _)| status == "committed")
            {
                anyhow::bail!("committed Jobs allowance has no packet metering row")
            }
            let pre_reserved = allowance
                .as_ref()
                .is_some_and(|(status, held_period)| {
                    status == "reserved" && *held_period == period_start
                });
            let included = pre_reserved || used < limit;
            let amount_cents = if included { 0 } else { PACKET_OVERAGE_CENTS };
            if amount_cents > 0 {
                let balance_before: i64 = tx.query_row(
                    "SELECT balance_cents FROM accounts WHERE id = ?1",
                    params![account_id],
                    |row| row.get(0),
                )?;
                if tx.execute(
                    "UPDATE accounts SET balance_cents = balance_cents - ?1
                      WHERE id = ?2 AND balance_cents >= ?1",
                    params![amount_cents, account_id],
                )? == 0
                {
                    anyhow::bail!("insufficient Bluey balance for Jobs overage");
                }
                crate::db::balance::consume_credit_batches_tx(&tx, account_id, amount_cents)?;
                crate::db::balance::insert_balance_ledger_sqlite_tx(
                    &tx,
                    crate::db::balance::BalanceLedgerEntry {
                        account_id,
                        event_type: "jobs_packet_overage",
                        amount_cents: -amount_cents,
                        balance_cents_before: balance_before,
                        balance_cents_after: balance_before - amount_cents,
                        reason: Some("jobs_completed_packet"),
                        provider: None,
                        processor_payment_id: None,
                        source_id: Some(&application.job_id),
                        idempotency_key: Some(&metering_key),
                        request_id: Some(&metering_key),
                        metadata_json: Some("{\"product\":\"bluey_jobs\"}"),
                    },
                )?;
            }
            tx.execute(
                "INSERT INTO jobs_packet_metering (
                    account_id, job_id, application_id, metering_key,
                    included, amount_cents, created_at_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    account_id,
                    application.job_id,
                    application.id,
                    metering_key,
                    i64::from(included),
                    amount_cents,
                    now,
                ],
            )?;
            tx.execute(
                "UPDATE jobs_entitlements SET used_packets = used_packets + 1,
                    updated_at_ms = ?2 WHERE account_id = ?1 AND ?3 = 0",
                params![account_id, now, i64::from(pre_reserved)],
            )?;
            if allowance
                .as_ref()
                .is_some_and(|(status, _)| status == "reserved")
            {
                tx.execute(
                    "UPDATE jobs_generation_allowance_reservations
                        SET status = 'committed', application_id = ?3, updated_at_ms = ?4
                      WHERE account_id = ?1 AND job_id = ?2 AND status = 'reserved'",
                    params![account_id, application.job_id, application.id, now],
                )?;
            }
            tx.commit()?;
            Ok(PacketCommitResult {
                newly_metered: true,
                included,
                amount_cents,
                used_packets: used + i64::from(!pre_reserved),
                monthly_packet_limit: limit,
            })
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            tx.query_one(
                "SELECT pg_advisory_xact_lock(hashtextextended($1, 0))",
                &[&format!(
                    "jobs-allowance:{account_id}:{}",
                    application.job_id
                )],
            )?;
            let entitlement = tx.query_one(
                "SELECT used_packets, monthly_packet_limit, period_start_ms
                   FROM jobs_entitlements
                  WHERE account_id = $1 FOR UPDATE",
                &[&account_id],
            )?;
            if let Some(row) = tx.query_opt(
                "SELECT included, amount_cents FROM jobs_packet_metering
                  WHERE account_id = $1 AND job_id = $2",
                &[&account_id, &application.job_id],
            )? {
                return Ok(PacketCommitResult {
                    newly_metered: false,
                    included: row.get::<_, i32>(0) != 0,
                    amount_cents: row.get(1),
                    used_packets: entitlement.get(0),
                    monthly_packet_limit: entitlement.get(1),
                });
            }
            let used: i64 = entitlement.get(0);
            let limit: i64 = entitlement.get(1);
            let period_start: i64 = entitlement.get(2);
            let allowance = tx.query_opt(
                "SELECT status, period_start_ms
                   FROM jobs_generation_allowance_reservations
                  WHERE account_id = $1 AND job_id = $2 FOR UPDATE",
                &[&account_id, &application.job_id],
            )?;
            if allowance
                .as_ref()
                .is_some_and(|row| row.get::<_, String>(0) == "committed")
            {
                anyhow::bail!("committed Jobs allowance has no packet metering row")
            }
            let pre_reserved = allowance
                .as_ref()
                .is_some_and(|row| {
                    row.get::<_, String>(0) == "reserved"
                        && row.get::<_, i64>(1) == period_start
                });
            let included = pre_reserved || used < limit;
            let included_db = i32::from(included);
            let amount_cents = if included { 0 } else { PACKET_OVERAGE_CENTS };
            if amount_cents > 0 {
                let balance_before: i64 = tx
                    .query_one(
                        "SELECT balance_cents FROM accounts WHERE id = $1 FOR UPDATE",
                        &[&account_id],
                    )?
                    .get(0);
                if tx.execute(
                    "UPDATE accounts SET balance_cents = balance_cents - $1
                      WHERE id = $2 AND balance_cents >= $1",
                    &[&amount_cents, &account_id],
                )? == 0
                {
                    anyhow::bail!("insufficient Bluey balance for Jobs overage");
                }
                crate::db::balance::consume_credit_batches_pg_tx(
                    &mut tx,
                    account_id,
                    amount_cents,
                )?;
                crate::db::balance::insert_balance_ledger_pg_tx(
                    &mut tx,
                    crate::db::balance::BalanceLedgerEntry {
                        account_id,
                        event_type: "jobs_packet_overage",
                        amount_cents: -amount_cents,
                        balance_cents_before: balance_before,
                        balance_cents_after: balance_before - amount_cents,
                        reason: Some("jobs_completed_packet"),
                        provider: None,
                        processor_payment_id: None,
                        source_id: Some(&application.job_id),
                        idempotency_key: Some(&metering_key),
                        request_id: Some(&metering_key),
                        metadata_json: Some("{\"product\":\"bluey_jobs\"}"),
                    },
                )?;
            }
            tx.execute(
                "INSERT INTO jobs_packet_metering (
                    account_id, job_id, application_id, metering_key,
                    included, amount_cents, created_at_ms
                 ) VALUES ($1, $2, $3, $4, $5, $6, $7)",
                &[
                    &account_id,
                    &application.job_id,
                    &application.id,
                    &metering_key,
                    &included_db,
                    &amount_cents,
                    &now,
                ],
            )?;
            tx.execute(
                "UPDATE jobs_entitlements SET used_packets = used_packets + 1,
                    updated_at_ms = $2 WHERE account_id = $1 AND $3 = 0",
                &[&account_id, &now, &i32::from(pre_reserved)],
            )?;
            if allowance
                .as_ref()
                .is_some_and(|row| row.get::<_, String>(0) == "reserved")
            {
                tx.execute(
                    "UPDATE jobs_generation_allowance_reservations
                        SET status = 'committed', application_id = $3, updated_at_ms = $4
                      WHERE account_id = $1 AND job_id = $2 AND status = 'reserved'",
                    &[&account_id, &application.job_id, &application.id, &now],
                )?;
            }
            tx.commit()?;
            Ok(PacketCommitResult {
                newly_metered: true,
                included,
                amount_cents,
                used_packets: used + i64::from(!pre_reserved),
                monthly_packet_limit: limit,
            })
        }
    })
}
