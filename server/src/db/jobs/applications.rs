const MAX_APPLICATION_COVER_LETTER_CHARS: usize = 4_000;
const PENDING_AUTO_QUEUE_APPROVAL_KEY: &str = "_bluey_pending_auto_queue_approval_v1";

fn prepared_application_persistence_projection(
    application: &JobApplication,
) -> Result<JobApplication> {
    let mut persisted = application.clone();
    if application.state == "queued"
        && application.submission_mode == "auto_submit"
        && application.receipt.get("approved_execution").is_none()
    {
        persisted.state = "awaiting_review".to_string();
        persisted
            .receipt
            .as_object_mut()
            .ok_or_else(|| anyhow::anyhow!("application receipt must be an object"))?
            .insert(PENDING_AUTO_QUEUE_APPROVAL_KEY.to_string(), json!(true));
    }
    Ok(persisted)
}

fn add_prepared_execution_answer(answers: &mut BTreeMap<String, String>, key: &str, value: &str) {
    if !key.trim().is_empty() && !value.trim().is_empty() {
        answers.insert(key.trim().to_string(), value.trim().to_string());
    }
}

fn prepared_execution_answers(
    profile: &CareerProfile,
    application: &JobApplication,
    application_email: &str,
) -> BTreeMap<String, String> {
    let mut answers = BTreeMap::new();
    let mut names = profile.full_name.split_whitespace();
    let first_name = names.next().unwrap_or_default();
    let last_name = names.collect::<Vec<_>>().join(" ");
    add_prepared_execution_answer(&mut answers, "first_name", first_name);
    add_prepared_execution_answer(&mut answers, "last_name", &last_name);
    add_prepared_execution_answer(&mut answers, "full_name", &profile.full_name);
    add_prepared_execution_answer(&mut answers, "email", application_email);
    add_prepared_execution_answer(&mut answers, "phone", &profile.phone);
    add_prepared_execution_answer(&mut answers, "location", &profile.current_location);
    add_prepared_execution_answer(&mut answers, "address", &profile.street_address);
    add_prepared_execution_answer(&mut answers, "linkedin_url", &profile.linkedin_url);
    add_prepared_execution_answer(&mut answers, "portfolio_url", &profile.portfolio_url);
    add_prepared_execution_answer(
        &mut answers,
        "work_authorization",
        &profile.work_authorization,
    );
    add_prepared_execution_answer(
        &mut answers,
        "sponsorship_required",
        match profile.sponsorship_required {
            Some(true) => "Yes",
            Some(false) => "No",
            None => "",
        },
    );
    add_prepared_execution_answer(
        &mut answers,
        "salary_expectation",
        &profile.salary_expectation,
    );
    add_prepared_execution_answer(&mut answers, "notice_period", &profile.notice_period);
    if let Some(reusable) = profile.reusable_answers.as_object() {
        for (key, value) in reusable {
            if let Some(value) = value.as_str() {
                add_prepared_execution_answer(&mut answers, key, value);
            }
        }
    }
    for answer in &application.answers {
        let Some(answer) = answer.as_object() else {
            continue;
        };
        let key = ["key", "question", "field", "name"]
            .iter()
            .find_map(|key| answer.get(*key).and_then(Value::as_str))
            .unwrap_or_default();
        let value = ["value", "answer"]
            .iter()
            .find_map(|key| answer.get(*key).and_then(Value::as_str))
            .unwrap_or_default();
        add_prepared_execution_answer(&mut answers, key, value);
    }
    answers
}

fn prepared_execution_workplace(value: &str) -> &'static str {
    match value.to_ascii_lowercase().as_str() {
        "onsite" => "onsite",
        "hybrid" => "hybrid",
        "remote" => "remote",
        _ => "unknown",
    }
}

#[allow(clippy::too_many_arguments)]
fn prepared_auto_submit_approved_execution(
    account_id: &str,
    application: &JobApplication,
    posting: &JobPosting,
    resume: &ResumeVersion,
    profile: &CareerProfile,
    identity: &ApplicationIdentity,
    facts: &[CareerFact],
    authorization: &StoredAutoSubmitAuthorization,
    ats: &AtsCertificationPostingResolution,
    approved_at_ms: i64,
) -> Result<Value> {
    let binding = ats
        .active_binding
        .as_ref()
        .filter(|_| ats.status.status == "active")
        .ok_or_else(|| anyhow::anyhow!("current ATS certification is unavailable"))?;
    let ats_certification = ats_frozen_certification_admission_projection(binding)
        .context("freeze exact ATS certification into approved execution")?;
    let confirmed = facts
        .iter()
        .filter(|fact| fact.verification_status == "confirmed")
        .map(|fact| fact.id.as_str())
        .collect::<BTreeSet<_>>();
    let mut verified_claim_ids = resume
        .claim_ids
        .iter()
        .filter(|claim_id| confirmed.contains(claim_id.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    verified_claim_ids.sort();
    verified_claim_ids.dedup();
    let packet = json!({
        "applicationId": application.id,
        "jobId": posting.id,
        "resumeVersionId": resume.id,
        "resumeContent": resume.content,
        "coverLetterContent": application.cover_letter,
        "answers": prepared_execution_answers(profile, application, &identity.email),
        "verifiedClaimIds": verified_claim_ids,
        "applicationIdentityId": identity.id,
        "applicationEmail": identity.email,
        "browserProfileId": execution_browser_profile_id(account_id, &identity.id),
    });
    let job = json!({
        "externalId": posting.external_id,
        "canonicalUrl": posting.canonical_url,
        "company": posting.company,
        "title": posting.title,
        "location": posting.location,
        "workplace": prepared_execution_workplace(&posting.workplace),
        "description": posting.description,
        "source": binding.provider,
        "compensation": posting.compensation,
    });
    let admission = json!({
        "kind": "track_auto_submit",
        "authorization_id": authorization.id,
        "career_track_id": authorization.career_track_id,
        "revision_no": authorization.revision_no,
        "authority_fingerprint": authorization.authority_fingerprint,
        "ats_certification": ats_certification,
    });
    let checksum = approved_submission_checksum(3, &packet, &job, Some(&admission))?;
    Ok(json!({
        "schema_version": 3,
        "approved_at_ms": approved_at_ms,
        "checksum": checksum,
        "admission": admission,
        "packet": packet,
        "job": job,
    }))
}

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
    get_application_with_revision(pool, account_id, application_id)
        .map(|current| current.map(|(application, _)| application))
}

fn get_application_with_revision(
    pool: &DbPool,
    account_id: &str,
    application_id: &str,
) -> Result<Option<(JobApplication, ExpectedApplicationRevision)>> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get()?;
            let raw: Option<(String, String, String, i64, String)> = conn
                .query_row(
                    "SELECT id, job_id, state, updated_at_ms, application_json
                       FROM jobs_applications WHERE account_id = ?1 AND id = ?2",
                    params![account_id, application_id],
                    |row| {
                        Ok((
                            row.get(0)?,
                            row.get(1)?,
                            row.get(2)?,
                            row.get(3)?,
                            row.get(4)?,
                        ))
                    },
                )
                .optional()?;
            raw.map(|(id, job_id, state, updated_at_ms, payload)| {
                let application =
                    parse_application_json(payload.clone(), &id, &job_id, "job application")?;
                Ok((
                    application,
                    ExpectedApplicationRevision {
                        id,
                        state,
                        updated_at_ms,
                        payload,
                    },
                ))
            })
            .transpose()
        }
        DbPool::Postgres(_) => pool
            .get_pg()?
            .query_opt(
                "SELECT id, job_id, state, updated_at_ms, application_json
                   FROM jobs_applications WHERE account_id = $1 AND id = $2",
                &[&account_id, &application_id],
            )?
            .map(|row| {
                let id: String = row.get(0);
                let job_id: String = row.get(1);
                let state: String = row.get(2);
                let updated_at_ms: i64 = row.get(3);
                let payload: String = row.get(4);
                let application =
                    parse_application_json(payload.clone(), &id, &job_id, "job application")?;
                Ok((
                    application,
                    ExpectedApplicationRevision {
                        id,
                        state,
                        updated_at_ms,
                        payload,
                    },
                ))
            })
            .transpose(),
    })
}

#[derive(Debug, Clone)]
pub struct CurrentApplicationApprovalAuthority {
    pub application: JobApplication,
    pub posting: JobPosting,
    pub ats_certification: AtsCertificationPostingResolution,
    pub job_integrity: JobIntegrityReceiptV1,
    pub evaluated_at_ms: i64,
}

fn current_application_approval_authority_from_composed(
    application: JobApplication,
    mut posting: JobPosting,
    composed: ComposedJobIntegrityProjection,
) -> Result<CurrentApplicationApprovalAuthority> {
    if application.job_id != posting.id {
        anyhow::bail!("application job authority does not match the stored posting")
    }
    if !original_source_projection_matches_application(&application, &composed.original_source)? {
        anyhow::bail!("original-source authority changed after application preparation")
    }
    if composed.job_integrity.status != JobIntegrityResolutionStatus::Verified {
        anyhow::bail!(
            "current signed job-integrity authority does not permit application approval ({})",
            composed.job_integrity.reason_code
        )
    }
    let source = composed
        .original_source
        .integrity_binding
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("current original-source destination is unavailable"))?;
    let ats_certification = composed
        .ats_certification
        .filter(|resolution| resolution.status.status == "active")
        .ok_or_else(|| anyhow::anyhow!("current ATS certification is unavailable"))?;
    let integrity =
        composed.job_integrity.authority.as_ref().ok_or_else(|| {
            anyhow::anyhow!("current signed job-integrity receipt is unavailable")
        })?;
    posting.canonical_url = source.canonical_application_url.clone();
    posting.source = source.provider_family.clone();
    posting.discovery_evidence = composed.discovery_evidence;
    Ok(CurrentApplicationApprovalAuthority {
        application,
        posting,
        ats_certification,
        job_integrity: job_integrity_receipt_projection(integrity),
        evaluated_at_ms: composed.original_source.db_time_ms,
    })
}

pub fn current_application_approval_authority(
    pool: &DbPool,
    account_id: &str,
    account_email: &str,
    application_id: &str,
) -> Result<Option<CurrentApplicationApprovalAuthority>> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Deferred)?;
            let stored = tx
                .query_row(
                    "SELECT application.id, application.job_id, application.application_json,
                            posting.id, posting.posting_json
                       FROM jobs_applications application
                       JOIN jobs_postings posting
                         ON posting.account_id = application.account_id
                        AND posting.id = application.job_id
                      WHERE application.account_id = ?1 AND application.id = ?2",
                    params![account_id, application_id],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, String>(3)?,
                            row.get::<_, String>(4)?,
                        ))
                    },
                )
                .optional()?;
            let authority = stored
                .map(
                    |(application_id, job_id, application_raw, posting_id, posting_raw)| {
                        let application = parse_application_json(
                            application_raw,
                            &application_id,
                            &job_id,
                            "application approval authority",
                        )?;
                        let mut posting: JobPosting =
                            parse_json(posting_raw, "application approval posting")?;
                        posting.id = posting_id;
                        let inputs = load_posting_representation_inputs_sqlite(
                            &tx,
                            account_id,
                            account_email,
                        )?;
                        let db_time_ms = representation_db_now_sqlite(&tx)?;
                        let represented = represent_posting_page_from_inputs_sqlite(
                            &tx,
                            account_id,
                            vec![posting.clone()],
                            &inputs,
                            db_time_ms,
                        )?
                        .pop()
                        .ok_or_else(|| {
                            anyhow::anyhow!("application approval posting is unavailable")
                        })?;
                        let composed = resolve_composed_job_integrity_projection_sqlite_tx_at_ms(
                            &tx, account_id, &posting, db_time_ms,
                        )?;
                        current_application_approval_authority_from_composed(
                            application,
                            represented,
                            composed,
                        )
                    },
                )
                .transpose()?;
            tx.commit()?;
            Ok(authority)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            lock_operational_hold_shared_postgres_tx(&mut tx).map_err(anyhow::Error::new)?;
            lock_managed_cloud_release_registry_shared_postgres_tx(&mut tx)
                .map_err(anyhow::Error::new)?;
            lock_postgres_ats_certification(&mut tx).map_err(anyhow::Error::new)?;
            lock_discovery_account_shared_postgres(&mut tx, account_id)?;
            lock_job_integrity_publication_fence_shared_postgres_tx(&mut tx)
                .map_err(anyhow::Error::new)?;
            lock_account_policy_inputs_postgres(&mut tx, account_id, false)?;
            let mutable_inputs_sha256 =
                posting_representation_mutable_inputs_sha256_postgres(&mut tx, account_id)?;
            let stored = tx.query_opt(
                "SELECT application.id, application.job_id, application.application_json,
                        posting.id, posting.posting_json
                   FROM jobs_applications application
                   JOIN jobs_postings posting
                     ON posting.account_id = application.account_id
                    AND posting.id = application.job_id
                  WHERE application.account_id = $1 AND application.id = $2
                  FOR SHARE OF application, posting",
                &[&account_id, &application_id],
            )?;
            let authority = stored
                .map(|row| -> Result<CurrentApplicationApprovalAuthority> {
                    let application_id: String = row.get(0);
                    let job_id: String = row.get(1);
                    let application = parse_application_json(
                        row.get(2),
                        &application_id,
                        &job_id,
                        "application approval authority",
                    )?;
                    let posting_id: String = row.get(3);
                    let mut posting: JobPosting =
                        parse_json(row.get(4), "application approval posting")?;
                    posting.id = posting_id;
                    let inputs = load_posting_representation_inputs_postgres(
                        &mut tx,
                        account_id,
                        account_email,
                    )?;
                    let db_time_ms = representation_db_now_postgres(&mut tx)?;
                    let represented = represent_posting_page_from_inputs_postgres(
                        &mut tx,
                        account_id,
                        vec![posting.clone()],
                        &inputs,
                        db_time_ms,
                    )?
                    .pop()
                    .ok_or_else(|| {
                        anyhow::anyhow!("application approval posting is unavailable")
                    })?;
                    let composed =
                        resolve_composed_job_integrity_projection_postgres_tx_after_prelock_at_ms(
                            &mut tx, account_id, &posting, db_time_ms,
                        )?;
                    current_application_approval_authority_from_composed(
                        application,
                        represented,
                        composed,
                    )
                })
                .transpose()?;
            if posting_representation_mutable_inputs_sha256_postgres(&mut tx, account_id)?
                != mutable_inputs_sha256
            {
                return Err(anyhow::Error::new(PostingRepresentationSnapshotChanged));
            }
            tx.commit()?;
            Ok(authority)
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
    let stored_posting =
        get_posting(pool, account_id, job_id)?.ok_or_else(|| anyhow::anyhow!("job not found"))?;
    let composed =
        resolve_composed_job_integrity_projection(pool, account_id, &stored_posting)?;
    let posting =
        posting_with_original_source_projection(&stored_posting, &composed.original_source);
    let original_source_expected_head = composed.original_source.expected_head.clone();
    let posting_fingerprint = posting_snapshot_fingerprint(&posting)?;
    let existing = find_application_for_job_with_revision(pool, account_id, job_id)?;
    let existing_application = existing
        .as_ref()
        .map(|(application, _)| application.clone());
    let expected_application = existing.as_ref().map(|(_, revision)| revision.clone());
    let eligibility = evaluate_job_eligibility_with_composed_projection(
        pool,
        account_id,
        &posting,
        &composed,
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
    let application_identity = selected_application_identity(track_identity_id, &identities)
        .ok_or_else(|| anyhow::anyhow!("verify an application email before preparing this packet"))?
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
    let career_track_policy_authority = serde_json::to_value(&track.policy.authority)
        .context("encode Career Track policy authority")?;
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
            "career_track_policy_authority": career_track_policy_authority.clone(),
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
        "career_track_policy_authority": career_track_policy_authority,
        "candidate_truth_fingerprint": truth_fingerprint,
        "candidate_truth_fingerprint_version": 1,
        "confirmed_facts_fingerprint": confirmed_facts_fingerprint,
        "confirmed_facts_fingerprint_version": 1,
        "job_snapshot_fingerprint": posting_fingerprint,
        "original_source_verification": original_source_expected_head,
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
    finalize_prepared_application_kit(
        pool,
        account_id,
        prepared,
        content,
        diff,
        String::new(),
        generation,
    )
}

pub fn finalize_prepared_application_kit(
    pool: &DbPool,
    account_id: &str,
    prepared: &PreparedApplicationDraft,
    content: Value,
    diff: Value,
    cover_letter: String,
    generation: Value,
) -> Result<(JobApplication, ResumeVersion)> {
    let cover_letter = cover_letter.trim().to_string();
    if cover_letter.chars().count() > MAX_APPLICATION_COVER_LETTER_CHARS {
        anyhow::bail!("generated cover letter is too long")
    }
    if cover_letter.contains('\0') {
        anyhow::bail!("generated cover letter contains invalid content")
    }
    let mut application = prepared.application.clone();
    application.cover_letter = cover_letter;
    let baseline = &prepared.baseline_resume;
    if application.state != "preparing" {
        anyhow::bail!("application is not waiting for resume generation")
    }
    let stored_posting = get_posting(pool, account_id, &application.job_id)?
        .ok_or_else(|| anyhow::anyhow!("job not found"))?;
    let composed =
        resolve_composed_job_integrity_projection(pool, account_id, &stored_posting)?;
    if !original_source_projection_matches_application(&application, &composed.original_source)? {
        anyhow::bail!("original-source verification changed during resume generation")
    }
    let posting =
        posting_with_original_source_projection(&stored_posting, &composed.original_source);
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
    let mut eligibility = evaluate_job_eligibility_with_composed_projection(
        pool,
        account_id,
        &posting,
        &composed,
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
        "cover_letter_status".to_string(),
        json!(if application.cover_letter.is_empty() {
            "not_included"
        } else {
            "included"
        }),
    );
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

fn prepared_application_hold_context_sqlite_tx(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    application: &JobApplication,
    posting: &JobPosting,
    employer_domain: &OperationalHoldEmployerDomain,
) -> Result<OperationalHoldContext> {
    let application_json = to_json(application, "prepared application hold context")?;
    let mut context = OperationalHoldContext::new();
    add_application_context_values(
        &mut context,
        OperationalApplicationContextValues {
            account_id,
            posting,
            employer_domain: Some(employer_domain),
            application_json: Some(&application_json),
            runner_kind: None,
            model_provider: None,
            model: None,
        },
    )
    .map_err(anyhow::Error::new)?;
    let mut statement = tx.prepare(
        "SELECT membership.source_id, source.provider, source.source_key, source.track_id
           FROM jobs_discovery_memberships membership
           JOIN jobs_discovery_sources source
             ON source.id = membership.source_id
            AND source.account_id = membership.account_id
          WHERE membership.account_id = ?1 AND membership.job_id = ?2",
    )?;
    let rows = statement.query_map(params![account_id, posting.id], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
        ))
    })?;
    let mut has_managed_curated_membership = false;
    for row in rows {
        let (source_id, provider, source_key, track_id) = row?;
        has_managed_curated_membership |= provider == CURATED_DISCOVERY_PROVIDER
            && source_key == CURATED_DISCOVERY_SOURCE_KEY
            && track_id.is_empty();
        context
            .insert_scope(OperationalHoldScopeKind::DiscoverySource, &source_id)
            .map_err(anyhow::Error::new)?;
        if let Some(provider) = operational_known_ats_provider(&provider) {
            context
                .insert_scope(OperationalHoldScopeKind::AtsProvider, provider)
                .map_err(anyhow::Error::new)?;
        }
    }
    require_curated_discovery_membership(posting, has_managed_curated_membership)
        .map_err(anyhow::Error::new)?;
    Ok(context)
}

fn prepared_application_hold_context_postgres_tx_after_prelock(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    application: &JobApplication,
    posting: &JobPosting,
    employer_domain: &OperationalHoldEmployerDomain,
) -> Result<OperationalHoldContext> {
    let application_json = to_json(application, "prepared application hold context")?;
    let mut context = OperationalHoldContext::new();
    add_application_context_values(
        &mut context,
        OperationalApplicationContextValues {
            account_id,
            posting,
            employer_domain: Some(employer_domain),
            application_json: Some(&application_json),
            runner_kind: None,
            model_provider: None,
            model: None,
        },
    )
    .map_err(anyhow::Error::new)?;
    let rows = tx.query(
        "SELECT membership.source_id, source.provider, source.source_key, source.track_id
           FROM jobs_discovery_memberships membership
           JOIN jobs_discovery_sources source
             ON source.id = membership.source_id
            AND source.account_id = membership.account_id
          WHERE membership.account_id = $1 AND membership.job_id = $2
          ORDER BY membership.source_id, membership.external_id
          FOR SHARE OF membership, source",
        &[&account_id, &posting.id],
    )?;
    let mut has_managed_curated_membership = false;
    for row in rows {
        let source_id: String = row.get(0);
        let provider: String = row.get(1);
        let source_key: String = row.get(2);
        let track_id: String = row.get(3);
        has_managed_curated_membership |= provider == CURATED_DISCOVERY_PROVIDER
            && source_key == CURATED_DISCOVERY_SOURCE_KEY
            && track_id.is_empty();
        context
            .insert_scope(OperationalHoldScopeKind::DiscoverySource, &source_id)
            .map_err(anyhow::Error::new)?;
        if let Some(provider) = operational_known_ats_provider(&provider) {
            context
                .insert_scope(OperationalHoldScopeKind::AtsProvider, provider)
                .map_err(anyhow::Error::new)?;
        }
    }
    require_curated_discovery_membership(posting, has_managed_curated_membership)
        .map_err(anyhow::Error::new)?;
    Ok(context)
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
            crate::db::object_uploads::require_active_account_write_fence_sqlite_tx(
                &tx, account_id,
            )?;
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
            let (posting_id, posting_raw): (String, String) = tx
                .query_row(
                    "SELECT id, posting_json FROM jobs_postings
                      WHERE account_id = ?1 AND id = ?2",
                    params![account_id, application.job_id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()?
                .ok_or_else(|| anyhow::anyhow!("job not found during application finalization"))?;
            let mut stored_current_posting: JobPosting =
                parse_json(posting_raw, "Jobs posting during application finalization")?;
            stored_current_posting.id = posting_id;
            let composed = resolve_composed_job_integrity_projection_sqlite_tx(
                &tx,
                account_id,
                &stored_current_posting,
            )?;
            let approval_db_time_ms = composed.original_source.db_time_ms;
            if !original_source_projection_matches_application(
                application,
                &composed.original_source,
            )? {
                anyhow::bail!("original-source verification changed during resume generation")
            }
            let current_posting = posting_with_original_source_projection(
                &stored_current_posting,
                &composed.original_source,
            );
            if posting_snapshot_fingerprint(&current_posting)? != expected_posting_fingerprint {
                anyhow::bail!("job posting changed while the application packet was generated")
            }
            let mut authority_posting = current_posting.clone();
            if let Some(source) = composed.original_source.integrity_binding.as_ref() {
                authority_posting.canonical_url = source.canonical_application_url.clone();
                authority_posting.source = source.provider_family.clone();
            }
            authority_posting.discovery_evidence = composed.discovery_evidence.clone();
            let current_job_integrity = composed.job_integrity.authority.as_ref().filter(|_| {
                composed.job_integrity.status == JobIntegrityResolutionStatus::Verified
            });
            if let Some(authority) = current_job_integrity {
                application
                    .receipt
                    .as_object_mut()
                    .ok_or_else(|| anyhow::anyhow!("application receipt must be an object"))?
                    .insert(
                        "job_integrity".to_string(),
                        serde_json::to_value(job_integrity_receipt_projection(authority))?,
                    );
            } else if application.state == "queued" {
                anyhow::bail!("current signed job-integrity authority does not permit queueing")
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
                &authority_posting,
                &current_profile,
                &preferences,
                current_track.as_ref(),
                &reservations,
                ApplicationFinalizationAuthorities {
                    discovery: &authorities,
                    ats: composed.ats_certification.as_ref(),
                },
            )?;
            let current_track = current_track
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("Career Track was removed during generation"))?;
            let current_identity = current_identity
                .ok_or_else(|| anyhow::anyhow!("application identity changed during generation"))?;
            let current_experience =
                role_experience_evidence(&current_profile, Some(current_track), &authority_posting);
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
            let (resume, insert_resume) = if let Some(existing) = tx
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
                (existing, false)
            } else {
                let version_no: i64 = tx.query_row(
                    "SELECT COALESCE(MAX(version_no), 0) + 1 FROM jobs_resume_versions
                      WHERE account_id = ?1 AND job_id = ?2",
                    params![account_id, application.job_id],
                    |row| row.get(0),
                )?;
                let id = uuid::Uuid::new_v4().to_string();
                (
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
                    },
                    true,
                )
            };

            application.resume_version_id = Some(resume.id.clone());
            application
                .receipt
                .as_object_mut()
                .ok_or_else(|| anyhow::anyhow!("application receipt must be an object"))?
                .insert("resume_version_id".to_string(), json!(resume.id));
            if application.state == "queued" {
                let authorization = tx
                    .query_row(
                        "SELECT id, career_track_id, application_identity_id,
                                source_resume_asset_id, authority_fingerprint, revision_no,
                                authorized_at_ms, revoked_at_ms
                           FROM jobs_auto_submit_authorizations
                          WHERE account_id = ?1 AND career_track_id = ?2
                            AND revoked_at_ms IS NULL",
                        params![account_id, current_track.id],
                        stored_auto_submit_from_sqlite_row,
                    )
                    .optional()?
                    .ok_or_else(|| {
                        anyhow::anyhow!("current Auto-submit authorization is unavailable")
                    })?;
                if !auto_submit_authorization_matches_inputs(
                    &authorization,
                    &current_profile,
                    &current_facts,
                    current_track,
                    current_identity,
                    &preferences,
                )? {
                    anyhow::bail!("current Auto-submit authorization changed during generation")
                }
                let ats = composed
                    .ats_certification
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("current ATS certification is unavailable"))?;
                let integrity = current_job_integrity.ok_or_else(|| {
                    anyhow::anyhow!("current signed job-integrity authority is unavailable")
                })?;
                if !job_integrity_resolution_matches_application(
                    application,
                    &composed.job_integrity,
                )? {
                    anyhow::bail!("signed job-integrity authority changed during finalization")
                }
                let employer_domain =
                    OperationalHoldEmployerDomain::from_current_job_integrity_authority(integrity)
                        .map_err(anyhow::Error::new)?;
                let approved_execution = prepared_auto_submit_approved_execution(
                    account_id,
                    application,
                    &authority_posting,
                    &resume,
                    &current_profile,
                    current_identity,
                    &current_facts,
                    &authorization,
                    ats,
                    approval_db_time_ms,
                )?;
                application
                    .receipt
                    .as_object_mut()
                    .ok_or_else(|| anyhow::anyhow!("application receipt must be an object"))?
                    .insert("approved_execution".to_string(), approved_execution);
                approved_submission_snapshot(account_id, application)?;

                let (local_entitled, cloud_entitled): (i64, i64) = tx
                    .query_row(
                        "SELECT local_browser, cloud_browser FROM jobs_entitlements
                          WHERE account_id = ?1",
                        params![account_id],
                        |row| Ok((row.get(0)?, row.get(1)?)),
                    )
                    .optional()?
                    .ok_or_else(|| anyhow::anyhow!("Jobs runner entitlement is unavailable"))?;
                let base_hold_context = prepared_application_hold_context_sqlite_tx(
                    &tx,
                    account_id,
                    application,
                    &authority_posting,
                    &employer_domain,
                )?;
                let mut queue_authorized = false;
                let mut held = None;
                for (runner_name, runner, entitled) in [
                    (
                        "local",
                        ExecutionAuthorityRunner::Local,
                        local_entitled != 0,
                    ),
                    (
                        "cloud",
                        ExecutionAuthorityRunner::Cloud,
                        cloud_entitled != 0,
                    ),
                ] {
                    if !entitled
                        || !current_execution_authority_matches(
                            account_id,
                            application,
                            &current_profile,
                            &authority_posting,
                            current_track,
                            current_identity,
                            &current_facts,
                            Some(&authorization),
                            &preferences,
                            &reservations,
                            &authorities,
                            Some(ats),
                            runner,
                        )?
                    {
                        continue;
                    }
                    let mut hold_context = base_hold_context.clone();
                    hold_context
                        .insert_scope(OperationalHoldScopeKind::RunnerKind, runner_name)
                        .map_err(anyhow::Error::new)?;
                    match require_operational_capability_sqlite_tx(
                        &tx,
                        OperationalCapability::ApplicationQueue,
                        &hold_context,
                    ) {
                        Ok(()) => {
                            queue_authorized = true;
                            break;
                        }
                        Err(OperationalHoldError::Held(block)) => {
                            held.get_or_insert(block);
                        }
                        Err(error) => return Err(anyhow::Error::new(error)),
                    }
                }
                if !queue_authorized {
                    if let Some(block) = held {
                        return Err(anyhow::Error::new(OperationalHoldError::Held(block)));
                    }
                    anyhow::bail!("current Jobs authority does not permit application queueing")
                }
            }

            let stored_evidence =
                persist_evidence_revision_sqlite(&tx, account_id, &current_evidence)?;
            if insert_resume {
                tx.execute(
                    "INSERT INTO jobs_resume_versions (
                        id, account_id, job_id, version_no, mode, content_json,
                        diff_json, claim_ids_json, checksum, created_at_ms
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                    params![
                        resume.id,
                        account_id,
                        resume.job_id,
                        resume.version_no,
                        resume.mode,
                        content_json,
                        diff_json,
                        claim_ids_json,
                        resume.checksum,
                        resume.created_at_ms,
                    ],
                )?;
            }
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
            persist_claim_evidence_sqlite(&tx, account_id, &resume.id, claim_evidence)?;
            let persisted_application = prepared_application_persistence_projection(application)?;
            let payload = to_json(&persisted_application, "job application")?;
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
                        persisted_application.resume_version_id,
                        persisted_application.state,
                        payload,
                        persisted_application.updated_at_ms,
                        persisted_application.submitted_at_ms,
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
                        persisted_application.id,
                        account_id,
                        persisted_application.job_id,
                        persisted_application.resume_version_id,
                        persisted_application.state,
                        payload,
                        persisted_application.created_at_ms,
                        persisted_application.updated_at_ms,
                        persisted_application.submitted_at_ms,
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
            // READ COMMITTED is deliberate: the first advisory-lock SELECT can wait behind an
            // H/M/ATS/D publisher. A transaction-wide snapshot established before that wait
            // could retain the publisher's stale authority rows after the lock is granted.
            let mut tx = conn.transaction()?;
            lock_operational_hold_shared_postgres_tx(&mut tx).map_err(anyhow::Error::new)?;
            lock_managed_cloud_release_registry_shared_postgres_tx(&mut tx)
                .map_err(anyhow::Error::new)?;
            lock_postgres_ats_certification(&mut tx).map_err(anyhow::Error::new)?;
            lock_discovery_account_shared_postgres(&mut tx, account_id)?;
            crate::db::object_uploads::require_active_account_write_fence_postgres_tx(
                &mut tx, account_id,
            )?;
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
            let posting_row = tx
                .query_opt(
                    "SELECT id, posting_json FROM jobs_postings
                      WHERE account_id = $1 AND id = $2 FOR SHARE",
                    &[&account_id, &application.job_id],
                )?
                .ok_or_else(|| anyhow::anyhow!("job not found during application finalization"))?;
            let posting_id: String = posting_row.get(0);
            let mut stored_current_posting: JobPosting = parse_json(
                posting_row.get::<_, String>(1),
                "Jobs posting during application finalization",
            )?;
            stored_current_posting.id = posting_id;
            if let Some(expected) = expected {
                lock_expected_application_revision_postgres_tx(
                    &mut tx,
                    account_id,
                    &application.job_id,
                    expected,
                )?;
            }
            // Evidence revision numbers are serialized by this exact account/Track namespace.
            // Hold it before the first composed-authority clock so a wait cannot carry either a
            // Review-first receipt or queued approval past signed-authority expiry.
            lock_profile_evidence_revision_postgres_tx(
                &mut tx,
                account_id,
                &expected_evidence_revision.career_track_id,
            )?;
            let composed = resolve_composed_job_integrity_projection_postgres_tx_after_prelock(
                &mut tx,
                account_id,
                &stored_current_posting,
            )?;
            if !original_source_projection_matches_application(
                application,
                &composed.original_source,
            )? {
                anyhow::bail!("original-source verification changed during resume generation")
            }
            let current_posting = posting_with_original_source_projection(
                &stored_current_posting,
                &composed.original_source,
            );
            if posting_snapshot_fingerprint(&current_posting)? != expected_posting_fingerprint {
                anyhow::bail!("job posting changed while the application packet was generated")
            }
            let mut authority_posting = current_posting.clone();
            if let Some(source) = composed.original_source.integrity_binding.as_ref() {
                authority_posting.canonical_url = source.canonical_application_url.clone();
                authority_posting.source = source.provider_family.clone();
            }
            authority_posting.discovery_evidence = composed.discovery_evidence.clone();
            let current_job_integrity = composed.job_integrity.authority.as_ref().filter(|_| {
                composed.job_integrity.status == JobIntegrityResolutionStatus::Verified
            });
            if let Some(authority) = current_job_integrity {
                application
                    .receipt
                    .as_object_mut()
                    .ok_or_else(|| anyhow::anyhow!("application receipt must be an object"))?
                    .insert(
                        "job_integrity".to_string(),
                        serde_json::to_value(job_integrity_receipt_projection(authority))?,
                    );
            } else if application.state == "queued" {
                anyhow::bail!("current signed job-integrity authority does not permit queueing")
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
            // Reservation creation takes this row FOR UPDATE before counting capacity. Holding it
            // FOR SHARE before our reservation scan prevents a new capacity phantom while an
            // Auto-submit approval and queued transition are being frozen.
            let queue_entitlements = if application.state == "queued" {
                let entitlement = tx
                    .query_opt(
                        "SELECT local_browser, cloud_browser FROM jobs_entitlements
                          WHERE account_id = $1 FOR SHARE",
                        &[&account_id],
                    )?
                    .ok_or_else(|| anyhow::anyhow!("Jobs runner entitlement is unavailable"))?;
                Some((
                    entitlement.get::<_, i32>(0) != 0,
                    entitlement.get::<_, i32>(1) != 0,
                ))
            } else {
                None
            };
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
                &authority_posting,
                &current_profile,
                &preferences,
                current_track.as_ref(),
                &reservations,
                ApplicationFinalizationAuthorities {
                    discovery: &authorities,
                    ats: composed.ats_certification.as_ref(),
                },
            )?;
            let current_track = current_track
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("Career Track was removed during generation"))?;
            let current_identity = current_identity
                .ok_or_else(|| anyhow::anyhow!("application identity changed during generation"))?;
            let current_experience =
                role_experience_evidence(&current_profile, Some(current_track), &authority_posting);
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
            tx.query_one(
                "SELECT pg_advisory_xact_lock(hashtextextended($1, 0))",
                &[&format!(
                    "{account_id}:{}:{checksum}:prepared-resume",
                    application.job_id
                )],
            )?;
            let (resume, insert_resume) = if let Some(row) = tx.query_opt(
                "SELECT id, job_id, version_no, mode, content_json, diff_json,
                        claim_ids_json, checksum, created_at_ms
                   FROM jobs_resume_versions
                  WHERE account_id = $1 AND job_id = $2 AND checksum = $3",
                &[&account_id, &application.job_id, &checksum],
            )? {
                (resume_from_pg_row(row)?, false)
            } else {
                let version_no: i64 = tx
                    .query_one(
                        "SELECT COALESCE(MAX(version_no), 0) + 1 FROM jobs_resume_versions
                          WHERE account_id = $1 AND job_id = $2",
                        &[&account_id, &application.job_id],
                    )?
                    .get(0);
                let id = uuid::Uuid::new_v4().to_string();
                (
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
                    },
                    true,
                )
            };

            application.resume_version_id = Some(resume.id.clone());
            application
                .receipt
                .as_object_mut()
                .ok_or_else(|| anyhow::anyhow!("application receipt must be an object"))?
                .insert("resume_version_id".to_string(), json!(resume.id));
            if application.state == "queued" {
                lock_auto_submit_authority_postgres(&mut tx, account_id, &current_track.id, false)?;
                let authorization = tx
                    .query_opt(
                        "SELECT id, career_track_id, application_identity_id,
                                source_resume_asset_id, authority_fingerprint, revision_no,
                                authorized_at_ms, revoked_at_ms
                           FROM jobs_auto_submit_authorizations
                          WHERE account_id = $1 AND career_track_id = $2
                            AND revoked_at_ms IS NULL
                          FOR SHARE",
                        &[&account_id, &current_track.id],
                    )?
                    .map(stored_auto_submit_from_postgres_row)
                    .ok_or_else(|| {
                        anyhow::anyhow!("current Auto-submit authorization is unavailable")
                    })?;
                if !auto_submit_authorization_matches_inputs(
                    &authorization,
                    &current_profile,
                    &current_facts,
                    current_track,
                    current_identity,
                    &preferences,
                )? {
                    anyhow::bail!("current Auto-submit authorization changed during generation")
                }
                // Entitlement, reservation, and Auto-submit rows can all block behind an
                // effect-capable writer. Re-resolve the signed authority only after those locks
                // are held so expiry is evaluated at the final admission clock.
                let refreshed_composed =
                    resolve_composed_job_integrity_projection_postgres_tx_after_prelock(
                        &mut tx,
                        account_id,
                        &stored_current_posting,
                    )?;
                let approval_db_time_ms = refreshed_composed.original_source.db_time_ms;
                if !original_source_projection_matches_application(
                    application,
                    &refreshed_composed.original_source,
                )? {
                    anyhow::bail!(
                        "original-source verification expired during application finalization"
                    )
                }
                let refreshed_posting = posting_with_original_source_projection(
                    &stored_current_posting,
                    &refreshed_composed.original_source,
                );
                if posting_snapshot_fingerprint(&refreshed_posting)? != expected_posting_fingerprint
                {
                    anyhow::bail!("job posting changed during application finalization")
                }
                let mut authority_posting = refreshed_posting;
                if let Some(source) = refreshed_composed
                    .original_source
                    .integrity_binding
                    .as_ref()
                {
                    authority_posting.canonical_url = source.canonical_application_url.clone();
                    authority_posting.source = source.provider_family.clone();
                }
                authority_posting.discovery_evidence =
                    refreshed_composed.discovery_evidence.clone();
                let integrity = refreshed_composed
                    .job_integrity
                    .authority
                    .as_ref()
                    .filter(|_| {
                        refreshed_composed.job_integrity.status
                            == JobIntegrityResolutionStatus::Verified
                    })
                    .ok_or_else(|| {
                        anyhow::anyhow!("current signed job-integrity authority is unavailable")
                    })?;
                application
                    .receipt
                    .as_object_mut()
                    .ok_or_else(|| anyhow::anyhow!("application receipt must be an object"))?
                    .insert(
                        "job_integrity".to_string(),
                        serde_json::to_value(job_integrity_receipt_projection(integrity))?,
                    );
                let ats = refreshed_composed
                    .ats_certification
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("current ATS certification is unavailable"))?;
                if !job_integrity_resolution_matches_application(
                    application,
                    &refreshed_composed.job_integrity,
                )? {
                    anyhow::bail!("signed job-integrity authority changed during finalization")
                }
                let employer_domain =
                    OperationalHoldEmployerDomain::from_current_job_integrity_authority(integrity)
                        .map_err(anyhow::Error::new)?;
                let approved_execution = prepared_auto_submit_approved_execution(
                    account_id,
                    application,
                    &authority_posting,
                    &resume,
                    &current_profile,
                    current_identity,
                    &current_facts,
                    &authorization,
                    ats,
                    approval_db_time_ms,
                )?;
                application
                    .receipt
                    .as_object_mut()
                    .ok_or_else(|| anyhow::anyhow!("application receipt must be an object"))?
                    .insert("approved_execution".to_string(), approved_execution);
                approved_submission_snapshot(account_id, application)?;

                let (local_entitled, cloud_entitled) = queue_entitlements
                    .ok_or_else(|| anyhow::anyhow!("Jobs runner entitlement is unavailable"))?;
                let base_hold_context =
                    prepared_application_hold_context_postgres_tx_after_prelock(
                        &mut tx,
                        account_id,
                        application,
                        &authority_posting,
                        &employer_domain,
                )?;
                let mut queue_authorized = false;
                let mut held = None;
                for (runner_name, runner, entitled) in [
                    ("local", ExecutionAuthorityRunner::Local, local_entitled),
                    ("cloud", ExecutionAuthorityRunner::Cloud, cloud_entitled),
                ] {
                    if !entitled
                        || !current_execution_authority_matches(
                            account_id,
                            application,
                            &current_profile,
                            &authority_posting,
                            current_track,
                            current_identity,
                            &current_facts,
                            Some(&authorization),
                            &preferences,
                            &reservations,
                            &authorities,
                            Some(ats),
                            runner,
                        )?
                    {
                        continue;
                    }
                    let mut hold_context = base_hold_context.clone();
                    hold_context
                        .insert_scope(OperationalHoldScopeKind::RunnerKind, runner_name)
                        .map_err(anyhow::Error::new)?;
                    match require_operational_capability_postgres_tx_after_authority_prelock(
                        &mut tx,
                        OperationalCapability::ApplicationQueue,
                        &hold_context,
                    ) {
                        Ok(()) => {
                            queue_authorized = true;
                            break;
                        }
                        Err(OperationalHoldError::Held(block)) => {
                            held.get_or_insert(block);
                        }
                        Err(error) => return Err(anyhow::Error::new(error)),
                    }
                }
                if !queue_authorized {
                    if let Some(block) = held {
                        return Err(anyhow::Error::new(OperationalHoldError::Held(block)));
                    }
                    anyhow::bail!("current Jobs authority does not permit application queueing")
                }
            }

            let stored_evidence =
                persist_evidence_revision_postgres(&mut tx, account_id, &current_evidence)?;
            if insert_resume {
                tx.execute(
                    "INSERT INTO jobs_resume_versions (
                        id, account_id, job_id, version_no, mode, content_json,
                        diff_json, claim_ids_json, checksum, created_at_ms
                     ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
                    &[
                        &resume.id,
                        &account_id,
                        &resume.job_id,
                        &resume.version_no,
                        &resume.mode,
                        &content_json,
                        &diff_json,
                        &claim_ids_json,
                        &resume.checksum,
                        &resume.created_at_ms,
                    ],
                )?;
            }
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
            persist_claim_evidence_postgres(&mut tx, account_id, &resume.id, claim_evidence)?;
            let persisted_application = prepared_application_persistence_projection(application)?;
            let payload = to_json(&persisted_application, "job application")?;
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
                        &persisted_application.resume_version_id,
                        &persisted_application.state,
                        &payload,
                        &persisted_application.updated_at_ms,
                        &persisted_application.submitted_at_ms,
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
                        &persisted_application.id,
                        &account_id,
                        &persisted_application.job_id,
                        &persisted_application.resume_version_id,
                        &persisted_application.state,
                        &payload,
                        &persisted_application.created_at_ms,
                        &persisted_application.updated_at_ms,
                        &persisted_application.submitted_at_ms,
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

fn queue_admission_runner_kinds(
    application_state: &str,
    reserved_runner: Option<&str>,
    local_entitled: bool,
    cloud_entitled: bool,
) -> Vec<&'static str> {
    match reserved_runner {
        Some("local") if local_entitled => vec!["local"],
        Some("cloud") if cloud_entitled => vec!["cloud"],
        Some("local" | "cloud") => Vec::new(),
        Some("unassigned") | None if application_state == "queued" => {
            let mut runners = Vec::with_capacity(2);
            if local_entitled {
                runners.push("local");
            }
            if cloud_entitled {
                runners.push("cloud");
            }
            runners
        }
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod queue_admission_tests {
    use super::production_positive_authority_fixture::{
        install_production_positive_job_authorities, save_production_positive_verified_import,
    };
    use super::{
        application_job_integrity_receipt, approved_submission_checksum, default_profile,
        ensure_primary_application_identity, execution_browser_profile_id, get_application,
        get_resume_version, list_tracks, now_ms, persist_current_application_approval,
        prepare_application, prepared_application_persistence_projection,
        queue_admission_runner_kinds, reserve_application_attempt, save_preferences,
        save_resume_source_asset, set_entitlement_plan, update_application, upsert_track,
        CareerTrack, CareerTrackPolicy, DbPool, JobApplication, JobDiscoveryEvidence, JobPosting,
        JobPreferences, ResumeSourceAsset, PENDING_AUTO_QUEUE_APPROVAL_KEY,
    };
    use crate::db;
    use serde_json::json;

    fn commit_prepared_application_source() -> &'static str {
        include_str!("applications.rs")
            .split_once("fn commit_prepared_application(")
            .expect("prepared application commit implementation")
            .1
            .split_once("\nfn answers_for_posting(")
            .expect("bounded prepared application commit implementation")
            .0
    }

    fn assert_source_order(section: &str, needles: &[&str], label: &str) {
        let mut offset = 0;
        for needle in needles {
            let relative = section[offset..]
                .find(needle)
                .unwrap_or_else(|| panic!("missing {needle:?} in {label}"));
            offset += relative + needle.len();
        }
    }

    fn test_application(state: &str, submission_mode: &str) -> JobApplication {
        JobApplication {
            id: "application".to_string(),
            job_id: "job".to_string(),
            resume_version_id: Some("resume".to_string()),
            state: state.to_string(),
            submission_mode: submission_mode.to_string(),
            match_score: 100,
            answers: Vec::new(),
            cover_letter: String::new(),
            receipt: json!({}),
            run_id: None,
            created_at_ms: 1,
            updated_at_ms: 1,
            submitted_at_ms: None,
        }
    }

    #[test]
    fn auto_submit_preparation_persists_a_non_effect_capable_intermediate() {
        let application = test_application("queued", "auto_submit");

        let persisted = prepared_application_persistence_projection(&application).unwrap();

        assert_eq!(application.state, "queued");
        assert_eq!(persisted.state, "awaiting_review");
        assert_eq!(
            persisted
                .receipt
                .get(PENDING_AUTO_QUEUE_APPROVAL_KEY)
                .and_then(serde_json::Value::as_bool),
            Some(true)
        );
    }

    #[test]
    fn already_approved_auto_submit_preparation_keeps_queued_state() {
        let mut application = test_application("queued", "auto_submit");
        application.receipt["approved_execution"] = json!({});

        let persisted = prepared_application_persistence_projection(&application).unwrap();

        assert_eq!(persisted.state, "queued");
        assert!(persisted
            .receipt
            .get(PENDING_AUTO_QUEUE_APPROVAL_KEY)
            .is_none());
    }

    #[test]
    fn queued_unassigned_admission_uses_only_current_entitlements() {
        assert_eq!(
            queue_admission_runner_kinds("queued", Some("unassigned"), true, false),
            vec!["local"]
        );
        assert_eq!(
            queue_admission_runner_kinds("queued", None, false, true),
            vec!["cloud"]
        );
        assert!(queue_admission_runner_kinds("queued", None, false, false).is_empty());
    }

    #[test]
    fn reserved_runner_cannot_fall_over_to_a_different_entitlement() {
        assert!(queue_admission_runner_kinds("queued", Some("local"), false, true).is_empty());
        assert!(queue_admission_runner_kinds("queued", Some("cloud"), true, false).is_empty());
    }

    #[test]
    fn running_admission_requires_an_exact_entitled_runner() {
        assert!(queue_admission_runner_kinds("running", Some("unassigned"), true, true).is_empty());
        assert!(queue_admission_runner_kinds("running", None, true, true).is_empty());
        assert_eq!(
            queue_admission_runner_kinds("running", Some("local"), true, true),
            vec!["local"]
        );
    }

    #[test]
    fn preparation_eligibility_uses_one_composed_authority_without_an_ats_reread() {
        let eligibility_source = include_str!("eligibility.rs");
        let evaluator = eligibility_source
            .split_once("fn evaluate_job_eligibility_with_composed_projection(")
            .expect("composed eligibility evaluator")
            .1
            .split_once("\nfn apply_ats_certification_resolution(")
            .expect("bounded composed eligibility evaluator")
            .0;
        assert!(evaluator.contains(
            "authoritative_posting.discovery_evidence = composed.discovery_evidence.clone()"
        ));
        assert!(evaluator.contains("composed.ats_certification.as_ref()"));
        assert!(evaluator.contains("apply_ats_certification_resolution("));
        assert!(!evaluator.contains("resolve_ats_certification_for_posting"));

        let applications_source = include_str!("applications.rs");
        let prepare = applications_source
            .split_once("fn prepare_application_inner(")
            .expect("application preparation preflight")
            .1
            .split_once("\npub fn finalize_prepared_application(")
            .expect("bounded application preparation preflight")
            .0;
        let finalize = applications_source
            .split_once("pub fn finalize_prepared_application_kit(")
            .expect("application finalization preflight")
            .1
            .split_once("\nfn commit_prepared_application(")
            .expect("bounded application finalization preflight")
            .0;
        for (section, label) in [(prepare, "prepare"), (finalize, "finalize")] {
            assert_source_order(
                section,
                &[
                    "resolve_composed_job_integrity_projection(pool, account_id, &stored_posting)",
                    "posting_with_original_source_projection(&stored_posting, &composed.original_source)",
                    "evaluate_job_eligibility_with_composed_projection(",
                ],
                label,
            );
            assert!(!section.contains("resolve_original_source_verification_projection("));
            assert!(!section.contains("resolve_ats_certification_for_posting("));
        }
    }

    #[test]
    fn postgres_finalization_uses_lock_first_read_committed_authority() {
        let postgres = commit_prepared_application_source()
            .split_once("DbPool::Postgres(_) => {")
            .expect("PostgreSQL prepared application commit branch")
            .1;

        assert!(postgres.contains("let mut tx = conn.transaction()?;"));
        assert!(!postgres.contains("IsolationLevel::Serializable"));
        assert!(!postgres.contains("build_transaction()"));
        assert_source_order(
            postgres,
            &[
                "lock_operational_hold_shared_postgres_tx(&mut tx)",
                "lock_managed_cloud_release_registry_shared_postgres_tx(&mut tx)",
                "lock_postgres_ats_certification(&mut tx)",
                "lock_discovery_account_shared_postgres(&mut tx, account_id)",
                "require_active_account_write_fence_postgres_tx",
                "SELECT profile_json FROM jobs_profiles",
                "SELECT id, posting_json FROM jobs_postings",
                "lock_expected_application_revision_postgres_tx",
                "resolve_composed_job_integrity_projection_postgres_tx_after_prelock",
                "authority_posting.canonical_url = source.canonical_application_url.clone()",
                "SELECT local_browser, cloud_browser FROM jobs_entitlements",
                "FROM jobs_attempt_reservations",
                "lock_auto_submit_authority_postgres",
                "let refreshed_composed =",
                ".insert(\"approved_execution\".to_string(), approved_execution)",
                "prepared_application_hold_context_postgres_tx_after_prelock",
                "require_operational_capability_postgres_tx_after_authority_prelock",
                "persist_evidence_revision_postgres",
                "let changed = match expected",
                "tx.commit()?",
            ],
            "PostgreSQL atomic prepared application finalization",
        );
        for forbidden_relock in [
            "resolve_composed_job_integrity_projection(",
            "require_operational_capability_postgres_tx(",
            "resolve_ats_certification_for_posting(",
        ] {
            assert!(
                !postgres.contains(forbidden_relock),
                "PostgreSQL finalization uses relocking wrapper {forbidden_relock}"
            );
        }
    }

    #[test]
    fn postgres_prepared_finalization_prelocks_exact_evidence_before_authority_time() {
        let postgres = commit_prepared_application_source()
            .split_once("DbPool::Postgres(_) => {")
            .expect("PostgreSQL prepared application commit branch")
            .1;
        assert_source_order(
            postgres,
            &[
                "lock_expected_application_revision_postgres_tx",
                "lock_profile_evidence_revision_postgres_tx",
                "&expected_evidence_revision.career_track_id",
                "resolve_composed_job_integrity_projection_postgres_tx_after_prelock",
                "let refreshed_composed =",
                "persist_evidence_revision_postgres",
            ],
            "PostgreSQL prepared evidence/authority order",
        );

        let evidence_source = include_str!("evidence.rs");
        let namespace_lock = evidence_source
            .split_once("fn lock_profile_evidence_revision_postgres_tx(")
            .expect("PostgreSQL evidence namespace lock")
            .1
            .split_once("\nfn persist_evidence_revision_postgres(")
            .expect("bounded PostgreSQL evidence namespace lock")
            .0;
        assert!(namespace_lock.contains("'jobs-evidence:' || $1 || ':' || $2"));
        let persistence = evidence_source
            .split_once("fn persist_evidence_revision_postgres(")
            .expect("PostgreSQL evidence persistence")
            .1
            .split_once("\nfn persist_claim_evidence_sqlite(")
            .expect("bounded PostgreSQL evidence persistence")
            .0;
        assert_source_order(
            persistence,
            &[
                "lock_profile_evidence_revision_postgres_tx",
                "&evidence.career_track_id",
                "SELECT id, revision_no, snapshot_json, created_at_ms",
                "INSERT INTO jobs_profile_evidence_revisions",
            ],
            "PostgreSQL evidence persistence held-namespace assertion",
        );
    }

    #[test]
    fn postgres_packet_commit_locks_account_before_entitlement() {
        let postgres = include_str!("applications.rs")
            .rsplit_once("pub fn commit_packet(")
            .expect("packet commit implementation")
            .1
            .split_once("DbPool::Postgres(_) => {")
            .expect("PostgreSQL packet commit branch")
            .1;
        assert_source_order(
            postgres,
            &[
                "SELECT balance_cents FROM accounts WHERE id = $1 FOR UPDATE",
                "jobs-allowance:{account_id}",
                "SELECT used_packets, monthly_packet_limit, period_start_ms",
                "FROM jobs_entitlements",
                "FROM jobs_generation_allowance_reservations",
                "if amount_cents > 0",
                "UPDATE accounts SET balance_cents",
                "consume_credit_batches_pg_tx",
                "insert_balance_ledger_pg_tx",
            ],
            "PostgreSQL packet Account -> entitlement -> allowance -> credit order",
        );
        assert_eq!(
            postgres
                .matches("SELECT balance_cents FROM accounts WHERE id = $1 FOR UPDATE")
                .count(),
            1
        );
    }

    #[test]
    fn prepared_auto_submit_approval_and_queue_share_one_transaction() {
        let source = commit_prepared_application_source();
        let (sqlite, postgres) = source
            .split_once("DbPool::Postgres(_) => {")
            .expect("prepared application database branches");

        for (label, branch, hold_gate) in [
            ("SQLite", sqlite, "require_operational_capability_sqlite_tx"),
            (
                "PostgreSQL",
                postgres,
                "require_operational_capability_postgres_tx_after_authority_prelock",
            ),
        ] {
            assert_source_order(
                branch,
                &[
                    ".insert(\"approved_execution\".to_string(), approved_execution)",
                    "approved_submission_snapshot(account_id, application)",
                    hold_gate,
                    "persist_evidence_revision_",
                    "prepared_application_persistence_projection(application)",
                    "let changed = match expected",
                    "tx.commit()?",
                ],
                &format!("{label} approval and queue transaction"),
            );
            assert_eq!(
                branch.matches("tx.commit()?").count(),
                1,
                "{label} finalization must have one commit boundary"
            );
        }
    }

    #[test]
    fn prepared_auto_submit_approval_uses_the_composed_authority_timestamp() {
        let source = commit_prepared_application_source();
        assert_eq!(
            source
                .matches("let approval_db_time_ms = composed.original_source.db_time_ms;")
                .count(),
            1
        );
        assert_eq!(
            source
                .matches("let approval_db_time_ms = refreshed_composed.original_source.db_time_ms;")
                .count(),
            1
        );
        assert!(!source.contains("let approval_db_time_ms = original_source_db_now_sqlite"));
        assert!(!source.contains("let approval_db_time_ms = original_source_db_now_postgres"));
        assert_eq!(
            source
                .matches("prepared_auto_submit_approved_execution(")
                .count(),
            2
        );
        assert_eq!(source.matches("approval_db_time_ms,").count(), 2);
    }

    #[test]
    fn application_queue_holds_use_current_signed_employer_domain_without_relocking() {
        let source = include_str!("applications.rs");
        let sqlite = source
            .rsplit_once("fn require_application_queue_admission_sqlite_tx(")
            .expect("SQLite application queue admission")
            .1
            .split_once("fn require_application_queue_admission_postgres_tx(")
            .expect("bounded SQLite application queue admission")
            .0;
        assert_source_order(
            sqlite,
            &[
                "resolve_current_execution_authority_sqlite_after_prelock",
                "if !current.authorized",
                "current.employer_domain.as_ref()",
                "operational_hold_context_for_application_sqlite_tx_after_authority",
                "employer_domain",
                "require_operational_capability_sqlite_tx",
            ],
            "SQLite signed employer-domain application queue hold",
        );
        assert!(!sqlite.contains("operational_hold_context_for_application_sqlite_tx("));
        assert!(!sqlite.contains("current_execution_authorized_sqlite("));

        let postgres = source
            .rsplit_once("fn require_application_queue_admission_postgres_tx(")
            .expect("PostgreSQL application queue admission")
            .1
            .split_once("fn save_application(")
            .expect("bounded PostgreSQL application queue admission")
            .0;
        assert_source_order(
            postgres,
            &[
                "resolve_current_execution_authority_postgres_after_prelock",
                "if !current.authorized",
                "current.employer_domain.as_ref()",
                "operational_hold_context_for_application_postgres_tx_after_authority_prelock",
                "employer_domain",
                "require_operational_capability_postgres_tx_after_authority_prelock",
            ],
            "PostgreSQL signed employer-domain application queue hold",
        );
        for forbidden_relock in [
            "current_execution_authorized_postgres(",
            "operational_hold_context_for_application_postgres_tx(",
            "require_operational_capability_postgres_tx(",
            "resolve_original_source_verification_projection_postgres_tx(",
        ] {
            assert!(
                !postgres.contains(forbidden_relock),
                "PostgreSQL application queue admission relocks through {forbidden_relock}"
            );
        }
    }

    #[test]
    fn application_queue_persistence_prelocks_complete_authority_before_reads() {
        let save = include_str!("applications.rs")
            .rsplit_once("fn save_application(")
            .expect("application persistence implementation")
            .1
            .split_once("pub fn update_application(")
            .expect("bounded application persistence implementation")
            .0;
        let postgres = save
            .split_once("DbPool::Postgres(_) => {")
            .expect("PostgreSQL application persistence")
            .1;
        assert_source_order(
            postgres,
            &[
                "let mut tx = conn.transaction()?",
                "lock_operational_hold_shared_postgres_tx",
                "lock_managed_cloud_release_registry_shared_postgres_tx",
                "lock_postgres_ats_certification",
                "lock_discovery_account_shared_postgres",
                "require_active_account_write_fence_postgres_tx",
                "lock_expected_application_revision_postgres_tx",
                "require_application_queue_admission_postgres_tx",
                "UPDATE jobs_applications",
                "AND state = $9 AND updated_at_ms = $10 AND application_json = $11",
                "if changed != 1",
                "tx.commit()?",
            ],
            "PostgreSQL application queue persistence prelock and CAS",
        );
        assert!(
            !postgres.contains("postgres_original_source_verification_authority_for_account_tx")
        );

        let exact_lock = include_str!("applications.rs")
            .rsplit_once("fn lock_expected_application_revision_postgres_tx(")
            .expect("exact application revision lock")
            .1
            .split_once("fn save_application(")
            .expect("bounded exact application revision lock")
            .0;
        assert_source_order(
            exact_lock,
            &[
                "FROM jobs_applications application",
                "JOIN jobs_postings posting",
                "application.state = $4",
                "application.updated_at_ms = $5",
                "application.application_json = $6",
                "FOR UPDATE OF application FOR SHARE OF posting",
            ],
            "PostgreSQL exact application/posting row lock",
        );
    }

    #[test]
    fn queued_and_running_updates_delegate_to_the_current_composed_save_gate() {
        let update = include_str!("applications.rs")
            .rsplit_once("pub fn update_application(")
            .expect("application update implementation")
            .1
            .split_once("pub fn assign_application_run(")
            .expect("bounded application update implementation")
            .0;

        assert!(!update.contains("evaluate_job_eligibility"));
        assert_source_order(
            update,
            &[
                "validate_application_state(state)",
                "get_application_with_revision(pool, account_id, application_id)",
                "validate_application_transition(&application.state, state)",
                "application.state = state.to_string()",
                "save_application(pool, account_id, &application, &expected)",
            ],
            "application update current-authority delegation",
        );
    }

    fn production_positive_queue_pool() -> (DbPool, super::CareerProfile, JobPreferences) {
        let path = std::env::temp_dir().join(format!(
            "bluey-fix727-positive-{}-{}.sqlite3",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let pool = db::open_pool(&path).expect("open FIX-727 production-positive pool");
        db::run_migrations(&pool).expect("migrate FIX-727 production-positive pool");
        pool.get()
            .unwrap()
            .execute(
                "INSERT INTO accounts (id, email, password_hash, trial_seconds_remaining)
                 VALUES ('acct-fix727', 'fix727@example.com', 'hash', 0)",
                [],
            )
            .unwrap();
        let identity =
            ensure_primary_application_identity(&pool, "acct-fix727", "fix727@example.com")
                .unwrap();
        upsert_track(
            &pool,
            "acct-fix727",
            &CareerTrack {
                id: "track-fix727".to_string(),
                name: "Software engineering".to_string(),
                role: "Software Engineer".to_string(),
                locations: vec!["New York, NY".to_string()],
                remote_preference: "hybrid_ok".to_string(),
                application_identity_id: Some(identity.id),
                policy: CareerTrackPolicy {
                    role_family: "software_engineering".to_string(),
                    ..CareerTrackPolicy::default()
                },
                active: true,
                match_count: 0,
                created_at_ms: 0,
                updated_at_ms: 0,
            },
        )
        .unwrap();

        let now = now_ms();
        let source = ResumeSourceAsset {
            id: "resume-source-fix727".to_string(),
            file_name: "fixture-source-resume.pdf".to_string(),
            media_type: "application/pdf".to_string(),
            file_type: "pdf".to_string(),
            storage_key: "accounts/acct-fix727/jobs/fixture-source-resume.pdf".to_string(),
            sha256: "e".repeat(64),
            size_bytes: 1_024,
            page_count: Some(1),
            template_status: "converted_layout".to_string(),
            created_at_ms: now,
            updated_at_ms: now,
        };
        let mut profile = default_profile("fix727@example.com");
        profile.onboarding_complete = true;
        profile.source_resume_name = source.file_name.clone();
        profile.source_resume_asset_id = source.id.clone();
        profile.source_resume_sha256 = source.sha256.clone();
        profile.source_resume_media_type = source.media_type.clone();
        profile.source_resume_template_status = source.template_status.clone();
        let (_, profile) =
            save_resume_source_asset(&pool, "acct-fix727", &source, &profile).unwrap();
        let preferences = save_preferences(
            &pool,
            "acct-fix727",
            &JobPreferences {
                sponsorship: "not_required".to_string(),
                ..JobPreferences::default()
            },
        )
        .unwrap();
        let track = list_tracks(&pool, "acct-fix727")
            .unwrap()
            .into_iter()
            .find(|track| track.id == "track-fix727")
            .expect("FIX-727 Career Track exists");
        let track = upsert_track(&pool, "acct-fix727", &track).unwrap();
        assert_eq!(track.policy.authority.review_state, "approved");
        set_entitlement_plan(&pool, "acct-fix727", "pro").unwrap();
        (pool, profile, preferences)
    }

    #[test]
    fn production_signed_manual_approval_reserves_and_queues_without_legacy_sanitization() {
        let (pool, profile, preferences) = production_positive_queue_pool();
        let now = now_ms();
        let posting = JobPosting {
            id: String::new(),
            canonical_key: String::new(),
            source: "greenhouse_import".to_string(),
            external_id: "fix727-positive".to_string(),
            company: "Acme".to_string(),
            title: "Software Engineer".to_string(),
            location: "New York, NY".to_string(),
            workplace: "hybrid".to_string(),
            canonical_url: "https://boards.greenhouse.io/acme/jobs/fix727-positive".to_string(),
            description: "Build reliable products with Rust and TypeScript.".to_string(),
            compensation: "$170k-$200k".to_string(),
            employment_type: "full_time".to_string(),
            track_id: "track-fix727".to_string(),
            match_score: 0,
            matched_reasons: Vec::new(),
            missing_requirements: Vec::new(),
            posted_at_ms: Some(now - 60_000),
            last_verified_at_ms: Some(now),
            availability_status: "active".to_string(),
            status: "matched".to_string(),
            created_at_ms: 0,
            updated_at_ms: 0,
            discovery_evidence: JobDiscoveryEvidence::default(),
            eligibility: None,
        };
        let (posting, managed) = save_production_positive_verified_import(
            &pool,
            "acct-fix727",
            &posting,
            &profile,
            &preferences,
        );
        let installed = install_production_positive_job_authorities(
            &pool,
            "acct-fix727",
            &posting,
            &managed,
            "acme.com",
            "fix727-positive",
        );
        assert_ne!(installed.posting.source, "greenhouse_import");
        let composed_at_ms = installed.composed.original_source.db_time_ms;
        assert!(
            composed_at_ms
                < installed
                    .source
                    .integrity_binding
                    .as_ref()
                    .unwrap()
                    .source_expires_at_ms
        );
        assert!(composed_at_ms < installed.ats.active_binding.as_ref().unwrap().expires_at_ms);
        assert!(composed_at_ms < installed.integrity.effective_expires_at_ms);

        let (application, _) =
            prepare_application(&pool, "acct-fix727", &posting.id, "factual", "review_first")
                .unwrap();
        let authority = super::current_application_approval_authority(
            &pool,
            "acct-fix727",
            "fix727@example.com",
            &application.id,
        )
        .unwrap()
        .expect("current production-signed application authority");
        let resume = get_resume_version(
            &pool,
            "acct-fix727",
            authority
                .application
                .resume_version_id
                .as_deref()
                .expect("prepared application resume"),
        )
        .unwrap()
        .expect("prepared resume exists");
        let identity_id = authority
            .application
            .receipt
            .pointer("/application_identity/id")
            .and_then(serde_json::Value::as_str)
            .unwrap();
        let identity_email = authority
            .application
            .receipt
            .pointer("/application_identity/email")
            .and_then(serde_json::Value::as_str)
            .unwrap();
        let packet = json!({
            "applicationId": authority.application.id,
            "jobId": authority.posting.id,
            "resumeVersionId": resume.id,
            "resumeContent": resume.content,
            "coverLetterContent": authority.application.cover_letter,
            "answers": {},
            "verifiedClaimIds": [],
            "applicationIdentityId": identity_id,
            "applicationEmail": identity_email,
            "browserProfileId": execution_browser_profile_id("acct-fix727", identity_id),
        });
        let job = json!({
            "externalId": authority.posting.external_id,
            "canonicalUrl": authority.posting.canonical_url,
            "company": authority.posting.company,
            "title": authority.posting.title,
            "location": authority.posting.location,
            "workplace": authority.posting.workplace,
            "description": authority.posting.description,
            "source": authority.posting.source,
            "compensation": authority.posting.compensation,
        });
        let admission = json!({ "kind": "review_approval" });
        let approved_execution = json!({
            "schema_version": 2,
            "approved_at_ms": authority.evaluated_at_ms,
            "checksum": approved_submission_checksum(2, &packet, &job, Some(&admission)).unwrap(),
            "admission": admission,
            "packet": packet,
            "job": job,
        });
        let approved = persist_current_application_approval(
            &pool,
            "acct-fix727",
            "fix727@example.com",
            &authority,
            &approved_execution,
        )
        .unwrap()
        .expect("persist exact production-signed approval");
        assert_eq!(
            application_job_integrity_receipt(&approved)
                .unwrap()
                .as_ref(),
            Some(&authority.job_integrity)
        );

        let reservation =
            reserve_application_attempt(&pool, "acct-fix727", &approved.id, "local").unwrap();
        assert_eq!(reservation.runner, "local");
        let queued = update_application(&pool, "acct-fix727", &approved.id, "queued", None)
            .unwrap()
            .expect("queue production-signed application");
        assert_eq!(queued.state, "queued");
        assert_eq!(
            get_application(&pool, "acct-fix727", &approved.id)
                .unwrap()
                .unwrap()
                .state,
            "queued"
        );
    }

    #[test]
    fn postgres_read_committed_refreshes_authority_after_waiting_for_prelock() {
        let Ok(url) = std::env::var("BLUEY_TEST_POSTGRES_URL") else {
            return;
        };
        let suffix = uuid::Uuid::new_v4().simple().to_string();
        let schema = format!("fix727_{suffix}");
        let lock_key = format!("bluey-fix727-{suffix}");
        let reader_name = format!("bluey-fix727-reader-{suffix}");
        let mut setup = postgres::Client::connect(&url, postgres::NoTls)
            .expect("connect FIX-727 PostgreSQL setup client");
        setup
            .batch_execute(&format!(
                "CREATE SCHEMA {schema};
                 CREATE TABLE {schema}.authority (id integer PRIMARY KEY, denied boolean NOT NULL);
                 INSERT INTO {schema}.authority (id, denied) VALUES (1, FALSE);"
            ))
            .expect("create isolated FIX-727 authority fixture");

        let mut publisher = postgres::Client::connect(&url, postgres::NoTls)
            .expect("connect FIX-727 authority publisher");
        let mut publisher_tx = publisher
            .transaction()
            .expect("begin FIX-727 authority publication");
        publisher_tx
            .query_one(
                "SELECT pg_advisory_xact_lock(hashtextextended($1, 0))",
                &[&lock_key],
            )
            .expect("lock FIX-727 authority publication");
        publisher_tx
            .execute(
                &format!("UPDATE {schema}.authority SET denied = TRUE WHERE id = 1"),
                &[],
            )
            .expect("publish FIX-727 denial");

        let reader_url = url.clone();
        let reader_schema = schema.clone();
        let reader_lock_key = lock_key.clone();
        let reader_application_name = reader_name.clone();
        let (started_tx, started_rx) = std::sync::mpsc::channel();
        let reader = std::thread::spawn(move || -> std::result::Result<bool, String> {
            let mut client = postgres::Client::connect(&reader_url, postgres::NoTls)
                .map_err(|error| error.to_string())?;
            client
                .query_one(
                    "SELECT set_config('application_name', $1, false)",
                    &[&reader_application_name],
                )
                .map_err(|error| error.to_string())?;
            let mut tx = client.transaction().map_err(|error| error.to_string())?;
            started_tx.send(()).map_err(|error| error.to_string())?;
            tx.query_one(
                "SELECT pg_advisory_xact_lock_shared(hashtextextended($1, 0))",
                &[&reader_lock_key],
            )
            .map_err(|error| error.to_string())?;
            let isolation: String = tx
                .query_one("SHOW transaction_isolation", &[])
                .map_err(|error| error.to_string())?
                .get(0);
            if isolation != "read committed" {
                return Err(format!("unexpected transaction isolation {isolation}"));
            }
            let denied = tx
                .query_one(
                    &format!("SELECT denied FROM {reader_schema}.authority WHERE id = 1"),
                    &[],
                )
                .map_err(|error| error.to_string())?
                .get(0);
            tx.commit().map_err(|error| error.to_string())?;
            Ok(denied)
        });
        started_rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("FIX-727 reader started");

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let waited = loop {
            let waiting: i64 = publisher_tx
                .query_one(
                    "SELECT COUNT(*)::bigint
                       FROM pg_locks lock
                       JOIN pg_stat_activity activity ON activity.pid = lock.pid
                      WHERE lock.locktype = 'advisory' AND NOT lock.granted
                        AND activity.application_name = $1",
                    &[&reader_name],
                )
                .expect("observe FIX-727 reader advisory wait")
                .get(0);
            if waiting == 1 {
                break true;
            }
            if reader.is_finished() || std::time::Instant::now() >= deadline {
                break false;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        };
        publisher_tx
            .commit()
            .expect("commit FIX-727 denial publication");
        let observed_denial = reader
            .join()
            .expect("join FIX-727 authority reader")
            .expect("complete FIX-727 authority reader");
        setup
            .batch_execute(&format!("DROP SCHEMA {schema} CASCADE"))
            .expect("remove isolated FIX-727 authority fixture");

        assert!(
            waited,
            "reader did not block behind the authority publisher"
        );
        assert!(
            observed_denial,
            "lock-first READ COMMITTED reader retained stale pre-publication authority"
        );
    }

    #[test]
    fn postgres_application_first_row_order_prevents_reserve_save_cycle() {
        let Ok(url) = std::env::var("BLUEY_TEST_POSTGRES_URL") else {
            return;
        };
        let suffix = uuid::Uuid::new_v4().simple().to_string();
        let schema = format!("fix753_{suffix}");
        let reserve_name = format!("bluey-fix753-reserve-{suffix}");
        let mut setup = postgres::Client::connect(&url, postgres::NoTls)
            .expect("connect FIX-753 PostgreSQL setup client");
        setup
            .batch_execute(&format!(
                "CREATE SCHEMA {schema};
                 CREATE TABLE {schema}.application (id integer PRIMARY KEY, revision integer NOT NULL);
                 CREATE TABLE {schema}.entitlement (id integer PRIMARY KEY);
                 CREATE TABLE {schema}.reservation (
                     application_id integer PRIMARY KEY, status text NOT NULL
                 );
                 INSERT INTO {schema}.application (id, revision) VALUES (1, 1);
                 INSERT INTO {schema}.entitlement (id) VALUES (1);
                 INSERT INTO {schema}.reservation (application_id, status)
                 VALUES (1, 'reserved');"
            ))
            .expect("create isolated FIX-753 row-order fixture");

        let save_url = url.clone();
        let save_schema = schema.clone();
        let (application_locked_tx, application_locked_rx) = std::sync::mpsc::channel();
        let (continue_save_tx, continue_save_rx) = std::sync::mpsc::channel();
        let save = std::thread::spawn(move || -> std::result::Result<(), String> {
            let mut client = postgres::Client::connect(&save_url, postgres::NoTls)
                .map_err(|error| error.to_string())?;
            let mut tx = client.transaction().map_err(|error| error.to_string())?;
            tx.batch_execute("SET LOCAL lock_timeout = '5s'")
                .map_err(|error| error.to_string())?;
            tx.query_one(
                &format!("SELECT id FROM {save_schema}.application WHERE id = 1 FOR UPDATE"),
                &[],
            )
            .map_err(|error| error.to_string())?;
            application_locked_tx
                .send(())
                .map_err(|error| error.to_string())?;
            continue_save_rx
                .recv_timeout(std::time::Duration::from_secs(5))
                .map_err(|error| error.to_string())?;
            tx.query_one(
                &format!("SELECT id FROM {save_schema}.entitlement WHERE id = 1 FOR SHARE"),
                &[],
            )
            .map_err(|error| error.to_string())?;
            tx.query_one(
                &format!(
                    "SELECT application_id FROM {save_schema}.reservation
                      WHERE application_id = 1 FOR SHARE"
                ),
                &[],
            )
            .map_err(|error| error.to_string())?;
            tx.execute(
                &format!("UPDATE {save_schema}.application SET revision = 2 WHERE id = 1"),
                &[],
            )
            .map_err(|error| error.to_string())?;
            tx.commit().map_err(|error| error.to_string())
        });
        application_locked_rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("FIX-753 save locked the application first");

        let reserve_url = url.clone();
        let reserve_schema = schema.clone();
        let reserve_application_name = reserve_name.clone();
        let (reserve_started_tx, reserve_started_rx) = std::sync::mpsc::channel();
        let reserve = std::thread::spawn(move || -> std::result::Result<(), String> {
            let mut client = postgres::Client::connect(&reserve_url, postgres::NoTls)
                .map_err(|error| error.to_string())?;
            client
                .query_one(
                    "SELECT set_config('application_name', $1, false)",
                    &[&reserve_application_name],
                )
                .map_err(|error| error.to_string())?;
            let mut tx = client.transaction().map_err(|error| error.to_string())?;
            tx.batch_execute("SET LOCAL lock_timeout = '5s'")
                .map_err(|error| error.to_string())?;
            reserve_started_tx
                .send(())
                .map_err(|error| error.to_string())?;
            tx.query_one(
                &format!("SELECT id FROM {reserve_schema}.application WHERE id = 1 FOR SHARE"),
                &[],
            )
            .map_err(|error| error.to_string())?;
            tx.query_one(
                &format!("SELECT id FROM {reserve_schema}.entitlement WHERE id = 1 FOR UPDATE"),
                &[],
            )
            .map_err(|error| error.to_string())?;
            tx.query_one(
                &format!(
                    "SELECT application_id FROM {reserve_schema}.reservation
                      WHERE application_id = 1 FOR UPDATE"
                ),
                &[],
            )
            .map_err(|error| error.to_string())?;
            tx.execute(
                &format!(
                    "UPDATE {reserve_schema}.reservation SET status = 'running'
                      WHERE application_id = 1"
                ),
                &[],
            )
            .map_err(|error| error.to_string())?;
            tx.commit().map_err(|error| error.to_string())
        });
        reserve_started_rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("FIX-753 reserve transaction started");

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let reserve_waited_for_application = loop {
            let waiting: i64 = setup
                .query_one(
                    "SELECT COUNT(*)::bigint
                       FROM pg_locks lock
                       JOIN pg_stat_activity activity ON activity.pid = lock.pid
                      WHERE NOT lock.granted AND activity.application_name = $1",
                    &[&reserve_name],
                )
                .expect("observe FIX-753 canonical application wait")
                .get(0);
            if waiting > 0 {
                break true;
            }
            if reserve.is_finished() || std::time::Instant::now() >= deadline {
                break false;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        };
        continue_save_tx
            .send(())
            .expect("release FIX-753 canonical save transaction");
        let save_result = save.join().expect("join FIX-753 save transaction");
        let reserve_result = reserve.join().expect("join FIX-753 reserve transaction");
        setup
            .batch_execute(&format!("DROP SCHEMA {schema} CASCADE"))
            .expect("remove isolated FIX-753 row-order fixture");

        assert!(
            reserve_waited_for_application,
            "reserve did not wait at the canonical first application row"
        );
        save_result.expect("canonical save transaction completed without deadlock");
        reserve_result.expect("canonical reserve transaction completed without deadlock");
    }

    #[test]
    fn postgres_evidence_wait_expiry_leaves_prepared_rows_unmodified() {
        let Ok(url) = std::env::var("BLUEY_TEST_POSTGRES_URL") else {
            return;
        };
        let suffix = uuid::Uuid::new_v4().simple().to_string();
        let schema = format!("fix753_evidence_{suffix}");
        let account_id = format!("acct-fix753-evidence-{suffix}");
        let track_id = format!("track-fix753-evidence-{suffix}");
        let finalizer_name = format!(
            "bluey-fix753-evidence-finalizer-{}",
            &suffix[..16]
        );
        let mut setup = postgres::Client::connect(&url, postgres::NoTls)
            .expect("connect FIX-753 evidence setup client");
        setup
            .batch_execute(&format!(
                "CREATE SCHEMA {schema};
                 CREATE TABLE {schema}.authority (id integer PRIMARY KEY, expires_at_ms bigint NOT NULL);
                 CREATE TABLE {schema}.evidence (id integer PRIMARY KEY);
                 CREATE TABLE {schema}.resume (id integer PRIMARY KEY);
                 CREATE TABLE {schema}.application (id integer PRIMARY KEY, revision integer NOT NULL);
                 INSERT INTO {schema}.application (id, revision) VALUES (1, 1);"
            ))
            .expect("create isolated FIX-753 evidence fixture");
        let expires_at_ms: i64 = setup
            .query_one(
                "SELECT FLOOR(EXTRACT(EPOCH FROM clock_timestamp()) * 1000)::bigint + 500",
                &[],
            )
            .expect("sample FIX-753 evidence expiry")
            .get(0);
        setup
            .execute(
                &format!("INSERT INTO {schema}.authority (id, expires_at_ms) VALUES (1, $1)"),
                &[&expires_at_ms],
            )
            .expect("insert FIX-753 evidence authority");

        let mut blocker = postgres::Client::connect(&url, postgres::NoTls)
            .expect("connect FIX-753 evidence blocker");
        let mut blocker_tx = blocker
            .transaction()
            .expect("begin FIX-753 evidence blocker");
        blocker_tx
            .query_one(
                "SELECT pg_advisory_xact_lock(
                    hashtextextended('jobs-evidence:' || $1 || ':' || $2, 0)
                 )",
                &[&account_id, &track_id],
            )
            .expect("lock exact FIX-753 evidence namespace");

        let worker_url = url.clone();
        let worker_schema = schema.clone();
        let worker_account = account_id.clone();
        let worker_track = track_id.clone();
        let worker_name = finalizer_name.clone();
        let (started_tx, started_rx) = std::sync::mpsc::channel();
        let finalizer = std::thread::spawn(move || -> std::result::Result<bool, String> {
            let mut client = postgres::Client::connect(&worker_url, postgres::NoTls)
                .map_err(|error| error.to_string())?;
            client
                .query_one(
                    "SELECT set_config('application_name', $1, false)",
                    &[&worker_name],
                )
                .map_err(|error| error.to_string())?;
            let mut tx = client.transaction().map_err(|error| error.to_string())?;
            tx.batch_execute("SET LOCAL lock_timeout = '5s'")
                .map_err(|error| error.to_string())?;
            started_tx.send(()).map_err(|error| error.to_string())?;
            tx.query_one(
                "SELECT pg_advisory_xact_lock(
                    hashtextextended('jobs-evidence:' || $1 || ':' || $2, 0)
                 )",
                &[&worker_account, &worker_track],
            )
            .map_err(|error| error.to_string())?;
            let effect_now_ms: i64 = tx
                .query_one(
                    "SELECT FLOOR(EXTRACT(EPOCH FROM clock_timestamp()) * 1000)::bigint",
                    &[],
                )
                .map_err(|error| error.to_string())?
                .get(0);
            let expires_at_ms: i64 = tx
                .query_one(
                    &format!("SELECT expires_at_ms FROM {worker_schema}.authority WHERE id = 1"),
                    &[],
                )
                .map_err(|error| error.to_string())?
                .get(0);
            if expires_at_ms <= effect_now_ms {
                tx.rollback().map_err(|error| error.to_string())?;
                return Ok(false);
            }
            tx.execute(
                &format!("INSERT INTO {worker_schema}.evidence (id) VALUES (1)"),
                &[],
            )
            .map_err(|error| error.to_string())?;
            tx.execute(
                &format!("INSERT INTO {worker_schema}.resume (id) VALUES (1)"),
                &[],
            )
            .map_err(|error| error.to_string())?;
            tx.execute(
                &format!("UPDATE {worker_schema}.application SET revision = 2 WHERE id = 1"),
                &[],
            )
            .map_err(|error| error.to_string())?;
            tx.commit().map_err(|error| error.to_string())?;
            Ok(true)
        });
        started_rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("FIX-753 evidence finalizer started");

        let wait_deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let waited = loop {
            let waiting: i64 = setup
                .query_one(
                    "SELECT COUNT(*)::bigint
                       FROM pg_locks lock
                       JOIN pg_stat_activity activity ON activity.pid = lock.pid
                      WHERE lock.locktype = 'advisory' AND NOT lock.granted
                        AND activity.application_name = $1",
                    &[&finalizer_name],
                )
                .expect("observe FIX-753 evidence namespace wait")
                .get(0);
            if waiting == 1 {
                break true;
            }
            if finalizer.is_finished() || std::time::Instant::now() >= wait_deadline {
                break false;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        };
        loop {
            let observed_now_ms: i64 = setup
                .query_one(
                    "SELECT FLOOR(EXTRACT(EPOCH FROM clock_timestamp()) * 1000)::bigint",
                    &[],
                )
                .expect("observe FIX-753 evidence expiry")
                .get(0);
            if observed_now_ms > expires_at_ms {
                break;
            }
            assert!(
                std::time::Instant::now() < wait_deadline,
                "FIX-753 evidence authority did not expire within the bounded fixture"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        blocker_tx
            .commit()
            .expect("release FIX-753 evidence namespace");
        let authorized = finalizer
            .join()
            .expect("join FIX-753 evidence finalizer")
            .expect("complete FIX-753 evidence finalizer");
        let state = setup
            .query_one(
                &format!(
                    "SELECT
                        (SELECT COUNT(*) FROM {schema}.evidence)::bigint,
                        (SELECT COUNT(*) FROM {schema}.resume)::bigint,
                        (SELECT revision FROM {schema}.application WHERE id = 1)::bigint"
                ),
                &[],
            )
            .expect("query FIX-753 evidence zero-mutation state");
        setup
            .batch_execute(&format!("DROP SCHEMA {schema} CASCADE"))
            .expect("remove isolated FIX-753 evidence fixture");

        assert!(waited, "finalizer did not wait on the evidence namespace");
        assert!(
            !authorized,
            "expired authority authorized prepared persistence"
        );
        assert_eq!(state.get::<_, i64>(0), 0);
        assert_eq!(state.get::<_, i64>(1), 0);
        assert_eq!(state.get::<_, i64>(2), 1);
    }

    #[test]
    fn postgres_packet_account_first_avoids_account_entitlement_inversion() {
        let Ok(url) = std::env::var("BLUEY_TEST_POSTGRES_URL") else {
            return;
        };
        let suffix = uuid::Uuid::new_v4().simple().to_string();
        let schema = format!("fix753_packet_{suffix}");
        let packet_name = format!("bluey-fix753-packet-{suffix}");
        let mut setup = postgres::Client::connect(&url, postgres::NoTls)
            .expect("connect FIX-753 packet setup client");
        setup
            .batch_execute(&format!(
                "CREATE SCHEMA {schema};
                 CREATE TABLE {schema}.account (id integer PRIMARY KEY, balance integer NOT NULL);
                 CREATE TABLE {schema}.entitlement (id integer PRIMARY KEY, used integer NOT NULL);
                 INSERT INTO {schema}.account (id, balance) VALUES (1, 10);
                 INSERT INTO {schema}.entitlement (id, used) VALUES (1, 0);"
            ))
            .expect("create isolated FIX-753 packet fixture");

        let mut canonical_writer = postgres::Client::connect(&url, postgres::NoTls)
            .expect("connect FIX-753 canonical account writer");
        let mut writer_tx = canonical_writer
            .transaction()
            .expect("begin FIX-753 canonical account writer");
        writer_tx
            .query_one(
                &format!("SELECT id FROM {schema}.account WHERE id = 1 FOR UPDATE"),
                &[],
            )
            .expect("lock canonical Account first");

        let packet_url = url.clone();
        let packet_schema = schema.clone();
        let packet_application_name = packet_name.clone();
        let (started_tx, started_rx) = std::sync::mpsc::channel();
        let packet = std::thread::spawn(move || -> std::result::Result<(), String> {
            let mut client = postgres::Client::connect(&packet_url, postgres::NoTls)
                .map_err(|error| error.to_string())?;
            client
                .query_one(
                    "SELECT set_config('application_name', $1, false)",
                    &[&packet_application_name],
                )
                .map_err(|error| error.to_string())?;
            let mut tx = client.transaction().map_err(|error| error.to_string())?;
            tx.batch_execute("SET LOCAL lock_timeout = '5s'")
                .map_err(|error| error.to_string())?;
            started_tx.send(()).map_err(|error| error.to_string())?;
            tx.query_one(
                &format!("SELECT id FROM {packet_schema}.account WHERE id = 1 FOR UPDATE"),
                &[],
            )
            .map_err(|error| error.to_string())?;
            tx.query_one(
                &format!("SELECT id FROM {packet_schema}.entitlement WHERE id = 1 FOR UPDATE"),
                &[],
            )
            .map_err(|error| error.to_string())?;
            tx.execute(
                &format!("UPDATE {packet_schema}.account SET balance = balance - 1 WHERE id = 1"),
                &[],
            )
            .map_err(|error| error.to_string())?;
            tx.execute(
                &format!("UPDATE {packet_schema}.entitlement SET used = used + 1 WHERE id = 1"),
                &[],
            )
            .map_err(|error| error.to_string())?;
            tx.commit().map_err(|error| error.to_string())
        });
        started_rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("FIX-753 packet transaction started");

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let waited_for_account = loop {
            let waiting: i64 = setup
                .query_one(
                    "SELECT COUNT(*)::bigint
                       FROM pg_locks lock
                       JOIN pg_stat_activity activity ON activity.pid = lock.pid
                      WHERE NOT lock.granted AND activity.application_name = $1",
                    &[&packet_name],
                )
                .expect("observe FIX-753 packet Account wait")
                .get(0);
            if waiting > 0 {
                break true;
            }
            if packet.is_finished() || std::time::Instant::now() >= deadline {
                break false;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        };
        writer_tx
            .query_one(
                &format!("SELECT id FROM {schema}.entitlement WHERE id = 1 FOR UPDATE"),
                &[],
            )
            .expect("Account holder can lock entitlement without a cycle");
        writer_tx
            .commit()
            .expect("release canonical Account/entitlement writer");
        packet
            .join()
            .expect("join FIX-753 packet transaction")
            .expect("packet Account -> entitlement order completes without deadlock");
        let state = setup
            .query_one(
                &format!(
                    "SELECT
                        (SELECT balance FROM {schema}.account WHERE id = 1),
                        (SELECT used FROM {schema}.entitlement WHERE id = 1)"
                ),
                &[],
            )
            .expect("query FIX-753 packet state");
        setup
            .batch_execute(&format!("DROP SCHEMA {schema} CASCADE"))
            .expect("remove isolated FIX-753 packet fixture");

        assert!(
            waited_for_account,
            "packet did not wait on canonical Account"
        );
        assert_eq!(state.get::<_, i32>(0), 9);
        assert_eq!(state.get::<_, i32>(1), 1);
    }
}

fn add_candidate_queue_hold_scopes(
    context: &mut OperationalHoldContext,
    application: &JobApplication,
) -> Result<()> {
    let Some(certification) = application
        .receipt
        .pointer("/approved_execution/admission/ats_certification")
    else {
        return Ok(());
    };
    let certification = certification
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("invalid frozen Jobs ATS certification context"))?;
    let provider = certification
        .get("provider")
        .and_then(Value::as_str)
        .and_then(operational_known_ats_provider)
        .ok_or_else(|| anyhow::anyhow!("invalid frozen Jobs ATS provider context"))?;
    let adapter = certification
        .get("adapter_version")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| anyhow::anyhow!("invalid frozen Jobs ATS adapter context"))?;
    context
        .insert_scope(OperationalHoldScopeKind::AtsProvider, provider)
        .map_err(anyhow::Error::new)?;
    context
        .insert_scope(OperationalHoldScopeKind::AtsAdapter, adapter)
        .map_err(anyhow::Error::new)?;
    Ok(())
}

fn require_application_queue_admission_sqlite_tx(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    application: &JobApplication,
) -> Result<()> {
    let _approved_execution = approved_submission_snapshot(account_id, application)?;
    let (local_entitled, cloud_entitled): (i64, i64) = tx
        .query_row(
            "SELECT local_browser, cloud_browser FROM jobs_entitlements
              WHERE account_id = ?1",
            params![account_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?
        .ok_or_else(|| anyhow::anyhow!("Jobs runner entitlement is unavailable"))?;
    let reserved_runner = tx
        .query_row(
            "SELECT runner FROM jobs_attempt_reservations
              WHERE account_id = ?1 AND application_id = ?2
                AND status IN ('reserved', 'running', 'side_effect_unknown')",
            params![account_id, application.id],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    let runners = queue_admission_runner_kinds(
        &application.state,
        reserved_runner.as_deref(),
        local_entitled != 0,
        cloud_entitled != 0,
    );
    if runners.is_empty() {
        anyhow::bail!("current Jobs entitlement does not permit application queueing")
    }

    let mut held = None;
    for runner in runners {
        let execution_runner = match runner {
            "local" => ExecutionAuthorityRunner::Local,
            "cloud" => ExecutionAuthorityRunner::Cloud,
            _ => unreachable!("queue admission runners are closed above"),
        };
        let current = resolve_current_execution_authority_sqlite_after_prelock(
            tx,
            account_id,
            application,
            execution_runner,
        )?;
        if !current.authorized {
            continue;
        }
        let Some(employer_domain) = current.employer_domain.as_ref() else {
            continue;
        };
        let mut hold_context = operational_hold_context_for_application_sqlite_tx_after_authority(
            tx,
            account_id,
            &application.id,
            employer_domain,
            Some(runner),
            None,
            None,
        )
        .map_err(anyhow::Error::new)?;
        add_candidate_queue_hold_scopes(&mut hold_context, application)?;
        match require_operational_capability_sqlite_tx(
            tx,
            OperationalCapability::ApplicationQueue,
            &hold_context,
        ) {
            Ok(()) => {}
            Err(OperationalHoldError::Held(block)) => {
                held.get_or_insert(block);
                continue;
            }
            Err(error) => return Err(anyhow::Error::new(error)),
        }
        return Ok(());
    }
    if let Some(block) = held {
        return Err(anyhow::Error::new(OperationalHoldError::Held(block)));
    }
    anyhow::bail!("current Jobs authority does not permit application queueing")
}

fn require_application_queue_admission_postgres_tx(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    application: &JobApplication,
) -> Result<()> {
    let _approved_execution = approved_submission_snapshot(account_id, application)?;
    let entitlement = tx
        .query_opt(
            "SELECT local_browser, cloud_browser FROM jobs_entitlements
              WHERE account_id = $1 FOR SHARE",
            &[&account_id],
        )?
        .ok_or_else(|| anyhow::anyhow!("Jobs runner entitlement is unavailable"))?;
    let local_entitled = entitlement.get::<_, i32>(0) != 0;
    let cloud_entitled = entitlement.get::<_, i32>(1) != 0;
    let reserved_runner = tx
        .query_opt(
            "SELECT runner FROM jobs_attempt_reservations
              WHERE account_id = $1 AND application_id = $2
                AND status IN ('reserved', 'running', 'side_effect_unknown')
              FOR SHARE",
            &[&account_id, &application.id],
        )?
        .map(|row| row.get::<_, String>(0));
    let runners = queue_admission_runner_kinds(
        &application.state,
        reserved_runner.as_deref(),
        local_entitled,
        cloud_entitled,
    );
    if runners.is_empty() {
        anyhow::bail!("current Jobs entitlement does not permit application queueing")
    }

    let mut held = None;
    for runner in runners {
        let execution_runner = match runner {
            "local" => ExecutionAuthorityRunner::Local,
            "cloud" => ExecutionAuthorityRunner::Cloud,
            _ => unreachable!("queue admission runners are closed above"),
        };
        let current = resolve_current_execution_authority_postgres_after_prelock(
            tx,
            account_id,
            application,
            execution_runner,
        )?;
        if !current.authorized {
            continue;
        }
        let Some(employer_domain) = current.employer_domain.as_ref() else {
            continue;
        };
        let mut hold_context =
            operational_hold_context_for_application_postgres_tx_after_authority_prelock(
                tx,
                account_id,
                &application.id,
                employer_domain,
                Some(runner),
                None,
                None,
            )
            .map_err(anyhow::Error::new)?;
        add_candidate_queue_hold_scopes(&mut hold_context, application)?;
        match require_operational_capability_postgres_tx_after_authority_prelock(
            tx,
            OperationalCapability::ApplicationQueue,
            &hold_context,
        ) {
            Ok(()) => {}
            Err(OperationalHoldError::Held(block)) => {
                held.get_or_insert(block);
                continue;
            }
            Err(error) => return Err(anyhow::Error::new(error)),
        }
        return Ok(());
    }
    if let Some(block) = held {
        return Err(anyhow::Error::new(OperationalHoldError::Held(block)));
    }
    anyhow::bail!("current Jobs authority does not permit application queueing")
}

fn lock_expected_application_revision_postgres_tx(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    job_id: &str,
    expected: &ExpectedApplicationRevision,
) -> Result<()> {
    let locked = tx.query_opt(
        "SELECT application.id
           FROM jobs_applications application
           JOIN jobs_postings posting
             ON posting.account_id = application.account_id
            AND posting.id = application.job_id
          WHERE application.account_id = $1 AND application.id = $2
            AND application.job_id = $3 AND application.state = $4
            AND application.updated_at_ms = $5 AND application.application_json = $6
          FOR UPDATE OF application FOR SHARE OF posting",
        &[
            &account_id,
            &expected.id,
            &job_id,
            &expected.state,
            &expected.updated_at_ms,
            &expected.payload,
        ],
    )?;
    if locked.is_none() {
        anyhow::bail!("application changed before its update was committed")
    }
    Ok(())
}

fn save_application(
    pool: &DbPool,
    account_id: &str,
    application: &JobApplication,
    expected: &ExpectedApplicationRevision,
) -> Result<JobApplication> {
    validate_application_state(&application.state)?;
    if application.id != expected.id {
        anyhow::bail!("application update revision is invalid")
    }
    let payload = to_json(application, "job application")?;
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            crate::db::object_uploads::require_active_account_write_fence_sqlite_tx(
                &tx, account_id,
            )?;
            if matches!(application.state.as_str(), "queued" | "running") {
                require_application_queue_admission_sqlite_tx(&tx, account_id, application)?;
            }
            let changed = tx.execute(
                "UPDATE jobs_applications
                    SET resume_version_id = ?4, state = ?5, application_json = ?6,
                        updated_at_ms = ?7, submitted_at_ms = ?8
                  WHERE account_id = ?1 AND id = ?2 AND job_id = ?3
                    AND state = ?9 AND updated_at_ms = ?10 AND application_json = ?11",
                params![
                    account_id,
                    application.id,
                    application.job_id,
                    application.resume_version_id,
                    application.state,
                    payload,
                    application.updated_at_ms,
                    application.submitted_at_ms,
                    expected.state,
                    expected.updated_at_ms,
                    expected.payload,
                ],
            )?;
            if changed != 1 {
                anyhow::bail!("application changed before its update was committed")
            }
            tx.commit()?;
            Ok(application.clone())
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            if matches!(application.state.as_str(), "queued" | "running") {
                lock_operational_hold_shared_postgres_tx(&mut tx).map_err(anyhow::Error::new)?;
                lock_managed_cloud_release_registry_shared_postgres_tx(&mut tx)
                    .map_err(anyhow::Error::new)?;
                lock_postgres_ats_certification(&mut tx).map_err(anyhow::Error::new)?;
                lock_discovery_account_shared_postgres(&mut tx, account_id)?;
            }
            crate::db::object_uploads::require_active_account_write_fence_postgres_tx(
                &mut tx, account_id,
            )?;
            if matches!(application.state.as_str(), "queued" | "running") {
                lock_expected_application_revision_postgres_tx(
                    &mut tx,
                    account_id,
                    &application.job_id,
                    expected,
                )?;
                require_application_queue_admission_postgres_tx(&mut tx, account_id, application)?;
            }
            let changed = tx.execute(
                "UPDATE jobs_applications
                    SET resume_version_id = $4, state = $5, application_json = $6,
                        updated_at_ms = $7, submitted_at_ms = $8
                  WHERE account_id = $1 AND id = $2 AND job_id = $3
                    AND state = $9 AND updated_at_ms = $10 AND application_json = $11",
                &[
                    &account_id,
                    &application.id,
                    &application.job_id,
                    &application.resume_version_id,
                    &application.state,
                    &payload,
                    &application.updated_at_ms,
                    &application.submitted_at_ms,
                    &expected.state,
                    &expected.updated_at_ms,
                    &expected.payload,
                ],
            )?;
            if changed != 1 {
                anyhow::bail!("application changed before its update was committed")
            }
            tx.commit()?;
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
    let Some((mut application, expected)) =
        get_application_with_revision(pool, account_id, application_id)?
    else {
        return Ok(None);
    };
    if state == "submitted" {
        anyhow::bail!("only a verified runner receipt can finalize a submitted application")
    }
    validate_application_transition(&application.state, state)?;
    application.state = state.to_string();
    if let Some(mode) = submission_mode {
        if !matches!(mode, "review_first" | "auto_submit") {
            anyhow::bail!("invalid application submission mode");
        }
        application.submission_mode = mode.to_string();
    }
    application.updated_at_ms = now_ms();
    save_application(pool, account_id, &application, &expected).map(Some)
}

pub fn assign_application_run(
    pool: &DbPool,
    account_id: &str,
    application_id: &str,
    run_id: &str,
) -> Result<Option<JobApplication>> {
    let Some((mut application, expected)) =
        get_application_with_revision(pool, account_id, application_id)?
    else {
        return Ok(None);
    };
    application.run_id = Some(run_id.to_string());
    application.updated_at_ms = now_ms();
    save_application(pool, account_id, &application, &expected).map(Some)
}

fn application_approval_authority_matches(
    expected: &CurrentApplicationApprovalAuthority,
    current: &CurrentApplicationApprovalAuthority,
) -> Result<bool> {
    fn stable_posting_value(posting: &JobPosting) -> Result<Value> {
        let mut posting = posting.clone();
        if let Some(eligibility) = posting.eligibility.as_mut() {
            eligibility.evaluated_at_ms = 0;
        }
        serde_json::to_value(posting).context("encode stable application approval posting")
    }

    Ok(
        serde_json::to_value(&expected.application)? == serde_json::to_value(&current.application)?
            && stable_posting_value(&expected.posting)? == stable_posting_value(&current.posting)?
            && expected.ats_certification == current.ats_certification
            && expected.job_integrity == current.job_integrity,
    )
}

fn application_with_current_approved_execution(
    account_id: &str,
    authority: &CurrentApplicationApprovalAuthority,
    approved_execution: &Value,
) -> Result<JobApplication> {
    if authority.application.state != "awaiting_review" {
        anyhow::bail!("only an application waiting for review can freeze approval")
    }
    let mut application = authority.application.clone();
    let receipt = application
        .receipt
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("application receipt must be an object"))?;
    receipt.insert(
        "job_integrity".to_string(),
        serde_json::to_value(&authority.job_integrity)?,
    );
    receipt.insert("approved_execution".to_string(), approved_execution.clone());
    receipt.remove(PENDING_AUTO_QUEUE_APPROVAL_KEY);
    let approved = approved_submission_snapshot(account_id, &application)?;
    if approved.job.get("canonicalUrl").and_then(Value::as_str)
        != Some(authority.posting.canonical_url.as_str())
        || approved.job.get("source").and_then(Value::as_str)
            != Some(authority.posting.source.as_str())
        || application_job_integrity_receipt(&application)?.as_ref()
            != Some(&authority.job_integrity)
    {
        anyhow::bail!("approved execution does not match current signed job authority")
    }
    if application.submission_mode == "auto_submit" {
        let binding = authority
            .ats_certification
            .active_binding
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("current ATS certification is unavailable"))?;
        let expected = serde_json::to_value(
            ats_frozen_certification_admission_projection(binding)
                .context("project current ATS certification for application approval")?,
        )?;
        if application
            .receipt
            .pointer("/approved_execution/admission/ats_certification")
            != Some(&expected)
        {
            anyhow::bail!("approved execution does not match current ATS certification")
        }
    }
    application.updated_at_ms = authority
        .evaluated_at_ms
        .max(application.updated_at_ms.saturating_add(1));
    Ok(application)
}

pub fn persist_current_application_approval(
    pool: &DbPool,
    account_id: &str,
    account_email: &str,
    expected: &CurrentApplicationApprovalAuthority,
    approved_execution: &Value,
) -> Result<Option<JobApplication>> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            crate::db::object_uploads::require_active_account_write_fence_sqlite_tx(
                &tx, account_id,
            )?;
            let stored = tx
                .query_row(
                    "SELECT application.id, application.job_id, application.application_json,
                            posting.id, posting.posting_json
                       FROM jobs_applications application
                       JOIN jobs_postings posting
                         ON posting.account_id = application.account_id
                        AND posting.id = application.job_id
                      WHERE application.account_id = ?1 AND application.id = ?2",
                    params![account_id, expected.application.id],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, String>(3)?,
                            row.get::<_, String>(4)?,
                        ))
                    },
                )
                .optional()?;
            let Some((application_id, job_id, application_raw, posting_id, posting_raw)) = stored
            else {
                tx.commit()?;
                return Ok(None);
            };
            let application = parse_application_json(
                application_raw,
                &application_id,
                &job_id,
                "current application approval",
            )?;
            let mut posting: JobPosting =
                parse_json(posting_raw, "current application approval posting")?;
            posting.id = posting_id;
            let inputs = load_posting_representation_inputs_sqlite(&tx, account_id, account_email)?;
            let db_time_ms = representation_db_now_sqlite(&tx)?;
            let represented = represent_posting_page_from_inputs_sqlite(
                &tx,
                account_id,
                vec![posting.clone()],
                &inputs,
                db_time_ms,
            )?
            .pop()
            .ok_or_else(|| anyhow::anyhow!("application approval posting is unavailable"))?;
            let composed = resolve_composed_job_integrity_projection_sqlite_tx_at_ms(
                &tx, account_id, &posting, db_time_ms,
            )?;
            let current = current_application_approval_authority_from_composed(
                application,
                represented,
                composed,
            )?;
            if !application_approval_authority_matches(expected, &current)? {
                anyhow::bail!("application approval authority changed before it was frozen")
            }
            let application = application_with_current_approved_execution(
                account_id,
                &current,
                approved_execution,
            )?;
            let payload = to_json(&application, "current application approval")?;
            let changed = tx.execute(
                "UPDATE jobs_applications
                    SET application_json = ?3, updated_at_ms = ?4
                  WHERE account_id = ?1 AND id = ?2 AND state = 'awaiting_review'",
                params![
                    account_id,
                    application.id,
                    payload,
                    application.updated_at_ms
                ],
            )?;
            if changed != 1 {
                anyhow::bail!("application changed before its approval was frozen")
            }
            tx.commit()?;
            Ok(Some(application))
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            lock_operational_hold_shared_postgres_tx(&mut tx).map_err(anyhow::Error::new)?;
            lock_managed_cloud_release_registry_shared_postgres_tx(&mut tx)
                .map_err(anyhow::Error::new)?;
            lock_postgres_ats_certification(&mut tx).map_err(anyhow::Error::new)?;
            lock_discovery_account_shared_postgres(&mut tx, account_id)?;
            lock_job_integrity_publication_fence_shared_postgres_tx(&mut tx)
                .map_err(anyhow::Error::new)?;
            lock_account_policy_inputs_postgres(&mut tx, account_id, false)?;
            crate::db::object_uploads::require_active_account_write_fence_postgres_tx(
                &mut tx, account_id,
            )?;
            let mutable_inputs_sha256 =
                posting_representation_mutable_inputs_sha256_postgres(&mut tx, account_id)?;
            let stored = tx.query_opt(
                "SELECT application.id, application.job_id, application.application_json,
                        posting.id, posting.posting_json
                   FROM jobs_applications application
                   JOIN jobs_postings posting
                     ON posting.account_id = application.account_id
                    AND posting.id = application.job_id
                  WHERE application.account_id = $1 AND application.id = $2
                  FOR UPDATE OF application FOR SHARE OF posting",
                &[&account_id, &expected.application.id],
            )?;
            let Some(row) = stored else {
                tx.commit()?;
                return Ok(None);
            };
            let application_id: String = row.get(0);
            let job_id: String = row.get(1);
            let application = parse_application_json(
                row.get(2),
                &application_id,
                &job_id,
                "current application approval",
            )?;
            let posting_id: String = row.get(3);
            let mut posting: JobPosting =
                parse_json(row.get(4), "current application approval posting")?;
            posting.id = posting_id;
            let inputs =
                load_posting_representation_inputs_postgres(&mut tx, account_id, account_email)?;
            let db_time_ms = representation_db_now_postgres(&mut tx)?;
            let represented = represent_posting_page_from_inputs_postgres(
                &mut tx,
                account_id,
                vec![posting.clone()],
                &inputs,
                db_time_ms,
            )?
            .pop()
            .ok_or_else(|| anyhow::anyhow!("application approval posting is unavailable"))?;
            let composed =
                resolve_composed_job_integrity_projection_postgres_tx_after_prelock_at_ms(
                    &mut tx, account_id, &posting, db_time_ms,
                )?;
            let current = current_application_approval_authority_from_composed(
                application,
                represented,
                composed,
            )?;
            if !application_approval_authority_matches(expected, &current)? {
                anyhow::bail!("application approval authority changed before it was frozen")
            }
            if posting_representation_mutable_inputs_sha256_postgres(&mut tx, account_id)?
                != mutable_inputs_sha256
            {
                anyhow::bail!("application approval inputs changed before approval was frozen")
            }
            let application = application_with_current_approved_execution(
                account_id,
                &current,
                approved_execution,
            )?;
            let payload = to_json(&application, "current application approval")?;
            let changed = tx.execute(
                "UPDATE jobs_applications
                    SET application_json = $3, updated_at_ms = $4
                  WHERE account_id = $1 AND id = $2 AND state = 'awaiting_review'",
                &[
                    &account_id,
                    &application.id,
                    &payload,
                    &application.updated_at_ms,
                ],
            )?;
            if changed != 1 {
                anyhow::bail!("application changed before its approval was frozen")
            }
            tx.commit()?;
            Ok(Some(application))
        }
    })
}

pub fn clear_current_review_approval(
    pool: &DbPool,
    account_id: &str,
    application_id: &str,
    expected_checksum: &str,
) -> Result<bool> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            crate::db::object_uploads::require_active_account_write_fence_sqlite_tx(
                &tx, account_id,
            )?;
            let stored = tx
                .query_row(
                    "SELECT job_id, application_json FROM jobs_applications
                      WHERE account_id = ?1 AND id = ?2",
                    params![account_id, application_id],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
                )
                .optional()?;
            let Some((job_id, raw)) = stored else {
                tx.commit()?;
                return Ok(false);
            };
            let mut application =
                parse_application_json(raw, application_id, &job_id, "review approval cleanup")?;
            if application.state != "awaiting_review"
                || application
                    .receipt
                    .pointer("/approved_execution/checksum")
                    .and_then(Value::as_str)
                    != Some(expected_checksum)
            {
                tx.commit()?;
                return Ok(false);
            }
            let receipt = application
                .receipt
                .as_object_mut()
                .ok_or_else(|| anyhow::anyhow!("application receipt must be an object"))?;
            receipt.remove("approved_execution");
            if application.submission_mode == "auto_submit" {
                receipt.insert(PENDING_AUTO_QUEUE_APPROVAL_KEY.to_string(), json!(true));
            }
            application.updated_at_ms = original_source_db_now_sqlite(&tx)?
                .max(application.updated_at_ms.saturating_add(1));
            let payload = to_json(&application, "review approval cleanup")?;
            let changed = tx.execute(
                "UPDATE jobs_applications
                    SET application_json = ?3, updated_at_ms = ?4
                  WHERE account_id = ?1 AND id = ?2 AND state = 'awaiting_review'",
                params![
                    account_id,
                    application_id,
                    payload,
                    application.updated_at_ms
                ],
            )?;
            tx.commit()?;
            Ok(changed == 1)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            crate::db::object_uploads::require_active_account_write_fence_postgres_tx(
                &mut tx, account_id,
            )?;
            let stored = tx.query_opt(
                "SELECT job_id, application_json FROM jobs_applications
                  WHERE account_id = $1 AND id = $2 FOR UPDATE",
                &[&account_id, &application_id],
            )?;
            let Some(row) = stored else {
                tx.commit()?;
                return Ok(false);
            };
            let job_id: String = row.get(0);
            let mut application = parse_application_json(
                row.get(1),
                application_id,
                &job_id,
                "review approval cleanup",
            )?;
            if application.state != "awaiting_review"
                || application
                    .receipt
                    .pointer("/approved_execution/checksum")
                    .and_then(Value::as_str)
                    != Some(expected_checksum)
            {
                tx.commit()?;
                return Ok(false);
            }
            let receipt = application
                .receipt
                .as_object_mut()
                .ok_or_else(|| anyhow::anyhow!("application receipt must be an object"))?;
            receipt.remove("approved_execution");
            if application.submission_mode == "auto_submit" {
                receipt.insert(PENDING_AUTO_QUEUE_APPROVAL_KEY.to_string(), json!(true));
            }
            application.updated_at_ms = original_source_db_now_postgres(&mut tx)?
                .max(application.updated_at_ms.saturating_add(1));
            let payload = to_json(&application, "review approval cleanup")?;
            let changed = tx.execute(
                "UPDATE jobs_applications
                    SET application_json = $3, updated_at_ms = $4
                  WHERE account_id = $1 AND id = $2 AND state = 'awaiting_review'",
                &[
                    &account_id,
                    &application_id,
                    &payload,
                    &application.updated_at_ms,
                ],
            )?;
            tx.commit()?;
            Ok(changed == 1)
        }
    })
}

pub fn replace_application_receipt(
    pool: &DbPool,
    account_id: &str,
    application_id: &str,
    receipt: Value,
) -> Result<Option<JobApplication>> {
    let Some((mut application, expected)) =
        get_application_with_revision(pool, account_id, application_id)?
    else {
        return Ok(None);
    };
    let pending_auto_queue = application.state == "awaiting_review"
        && application.submission_mode == "auto_submit"
        && application
            .receipt
            .get(PENDING_AUTO_QUEUE_APPROVAL_KEY)
            .and_then(Value::as_bool)
            == Some(true);
    let approved_execution_added = receipt.get("approved_execution").is_some();
    application.receipt = receipt;
    if pending_auto_queue && approved_execution_added {
        validate_application_transition(&application.state, "queued")?;
        application.state = "queued".to_string();
        if let Some(receipt) = application.receipt.as_object_mut() {
            receipt.remove(PENDING_AUTO_QUEUE_APPROVAL_KEY);
        }
    } else if pending_auto_queue {
        application
            .receipt
            .as_object_mut()
            .ok_or_else(|| anyhow::anyhow!("application receipt must be an object"))?
            .insert(PENDING_AUTO_QUEUE_APPROVAL_KEY.to_string(), json!(true));
    }
    application.updated_at_ms = now_ms();
    save_application(pool, account_id, &application, &expected).map(Some)
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
            "awaiting_review" | "queued" | "running" | "side_effect_unknown" | "failed"
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
            let pre_reserved = allowance.as_ref().is_some_and(|(status, held_period)| {
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
            // Account balance writers acquire Account before any Jobs metering child. Hold
            // Account even for an included packet so allowance serialization and the entitlement
            // decision cannot create a child -> Account inversion with another account writer.
            let balance_before: i64 = tx
                .query_one(
                    "SELECT balance_cents FROM accounts WHERE id = $1 FOR UPDATE",
                    &[&account_id],
                )?
                .get(0);
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
            let pre_reserved = allowance.as_ref().is_some_and(|row| {
                row.get::<_, String>(0) == "reserved" && row.get::<_, i64>(1) == period_start
            });
            let included = pre_reserved || used < limit;
            let included_db = i32::from(included);
            let amount_cents = if included { 0 } else { PACKET_OVERAGE_CENTS };
            if amount_cents > 0 {
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
