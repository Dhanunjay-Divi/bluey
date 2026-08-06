#[derive(Debug, Clone, Copy)]
enum ExecutionAuthorityRunner {
    Local,
    Cloud,
}

fn current_execution_authorized_sqlite(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    application: &JobApplication,
    runner: ExecutionAuthorityRunner,
) -> Result<bool> {
    let Some(profile) = tx
        .query_row(
            "SELECT profile_json FROM jobs_profiles WHERE account_id = ?1",
            params![account_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        .map(|raw| parse_json::<CareerProfile>(raw, "Jobs execution profile"))
        .transpose()?
    else {
        return Ok(false);
    };
    let Some(posting) = tx
        .query_row(
            "SELECT posting_json FROM jobs_postings WHERE account_id = ?1 AND id = ?2",
            params![account_id, application.job_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        .map(|raw| parse_json::<JobPosting>(raw, "Jobs execution posting"))
        .transpose()?
    else {
        return Ok(false);
    };
    let Some(track) = tx
        .query_row(
            "SELECT track_json FROM jobs_tracks WHERE account_id = ?1 AND id = ?2",
            params![account_id, posting.track_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        .map(|raw| parse_json::<CareerTrack>(raw, "Jobs execution Career Track"))
        .transpose()?
    else {
        return Ok(false);
    };
    let Some(identity_id) = frozen_receipt_string(application, "/application_identity/id") else {
        return Ok(false);
    };
    let Some((identity_raw, identity_status, is_default)) = tx
        .query_row(
            "SELECT identity_json, verification_status, is_default
               FROM jobs_application_identities
              WHERE account_id = ?1 AND id = ?2",
            params![account_id, identity_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)? != 0,
                ))
            },
        )
        .optional()?
    else {
        return Ok(false);
    };
    let identity = parse_application_identity_row(identity_raw, identity_status, is_default)?;
    let mut fact_stmt = tx.prepare(
        "SELECT id, category, label, value_json, source, verification_status,
                confirmed_at_ms, confirmed_by, schema_version, created_at_ms, updated_at_ms
           FROM jobs_facts
          WHERE account_id = ?1 AND verification_status = 'confirmed'",
    )?;
    let facts = fact_stmt
        .query_map(params![account_id], fact_from_sqlite_row)?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let auto_submit_authorization = if application.submission_mode == "auto_submit" {
        tx.query_row(
            "SELECT id, career_track_id, application_identity_id,
                    source_resume_asset_id, authority_fingerprint, revision_no,
                    authorized_at_ms, revoked_at_ms
               FROM jobs_auto_submit_authorizations
              WHERE account_id = ?1 AND career_track_id = ?2
                AND revoked_at_ms IS NULL",
            params![account_id, track.id],
            |row| {
                Ok(StoredAutoSubmitAuthorization {
                    id: row.get(0)?,
                    career_track_id: row.get(1)?,
                    application_identity_id: row.get(2)?,
                    source_resume_asset_id: row.get(3)?,
                    authority_fingerprint: row.get(4)?,
                    revision_no: row.get(5)?,
                    authorized_at_ms: row.get(6)?,
                    revoked_at_ms: row.get(7)?,
                })
            },
        )
        .optional()?
    } else {
        None
    };
    let preferences = tx
        .query_row(
            "SELECT preferences_json FROM jobs_preferences WHERE account_id = ?1",
            params![account_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        .map(|raw| parse_json::<JobPreferences>(raw, "Jobs execution preferences"))
        .transpose()?
        .map(enforce_job_preference_safety)
        .unwrap_or_default();
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
    let ats_certification = match resolve_ats_certification_for_posting_sqlite_tx(
        tx,
        account_id,
        &posting,
        None,
        now_ms(),
    ) {
        Ok(resolution) => Some(resolution),
        Err(AtsCertificationAuthorityError::Storage(error)) => return Err(error),
        Err(_) => None,
    };

    let authorized = current_execution_authority_matches(
        account_id,
        application,
        &profile,
        &posting,
        &track,
        &identity,
        &facts,
        auto_submit_authorization.as_ref(),
        &preferences,
        &reservations,
        &authorities,
        ats_certification.as_ref(),
        runner,
    )? && stored_execution_evidence_matches_sqlite(tx, account_id, application)?;
    Ok(authorized)
}

fn current_execution_authorized_postgres(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    application: &JobApplication,
    runner: ExecutionAuthorityRunner,
) -> Result<bool> {
    let Some(profile_row) = tx.query_opt(
        "SELECT profile_json FROM jobs_profiles WHERE account_id = $1",
        &[&account_id],
    )? else {
        return Ok(false);
    };
    let profile: CareerProfile = parse_json(profile_row.get(0), "Jobs execution profile")?;
    let Some(posting_row) = tx.query_opt(
        "SELECT posting_json FROM jobs_postings WHERE account_id = $1 AND id = $2",
        &[&account_id, &application.job_id],
    )? else {
        return Ok(false);
    };
    let posting: JobPosting = parse_json(posting_row.get(0), "Jobs execution posting")?;
    let Some(track_row) = tx.query_opt(
        "SELECT track_json FROM jobs_tracks WHERE account_id = $1 AND id = $2",
        &[&account_id, &posting.track_id],
    )? else {
        return Ok(false);
    };
    let track: CareerTrack = parse_json(track_row.get(0), "Jobs execution Career Track")?;
    let Some(identity_id) = frozen_receipt_string(application, "/application_identity/id") else {
        return Ok(false);
    };
    let Some(identity_row) = tx.query_opt(
        "SELECT identity_json, verification_status, is_default
           FROM jobs_application_identities
          WHERE account_id = $1 AND id = $2",
        &[&account_id, &identity_id],
    )? else {
        return Ok(false);
    };
    let identity = parse_application_identity_row(
        identity_row.get(0),
        identity_row.get(1),
        identity_row.get::<_, i32>(2) != 0,
    )?;
    let facts = tx
        .query(
            "SELECT id, category, label, value_json, source, verification_status,
                    confirmed_at_ms, confirmed_by, schema_version, created_at_ms, updated_at_ms
               FROM jobs_facts
              WHERE account_id = $1 AND verification_status = 'confirmed'",
            &[&account_id],
        )?
        .into_iter()
        .map(fact_from_pg_row)
        .collect::<Result<Vec<_>>>()?;
    let auto_submit_authorization = if application.submission_mode == "auto_submit" {
        tx.query_opt(
            "SELECT id, career_track_id, application_identity_id,
                    source_resume_asset_id, authority_fingerprint, revision_no,
                    authorized_at_ms, revoked_at_ms
               FROM jobs_auto_submit_authorizations
              WHERE account_id = $1 AND career_track_id = $2
                AND revoked_at_ms IS NULL",
            &[&account_id, &track.id],
        )?
        .map(|row| StoredAutoSubmitAuthorization {
            id: row.get(0),
            career_track_id: row.get(1),
            application_identity_id: row.get(2),
            source_resume_asset_id: row.get(3),
            authority_fingerprint: row.get(4),
            revision_no: row.get(5),
            authorized_at_ms: row.get(6),
            revoked_at_ms: row.get(7),
        })
    } else {
        None
    };
    let preferences = tx
        .query_opt(
            "SELECT preferences_json FROM jobs_preferences WHERE account_id = $1",
            &[&account_id],
        )?
        .map(|row| parse_json::<JobPreferences>(row.get(0), "Jobs execution preferences"))
        .transpose()?
        .map(enforce_job_preference_safety)
        .unwrap_or_default();
    let reservations = tx
        .query(
            "SELECT id, application_id, company_key, period_key, runner, status,
                    reserved_at_ms, updated_at_ms
               FROM jobs_attempt_reservations WHERE account_id = $1",
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
              WHERE m.account_id = $1 AND m.job_id = $2",
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
    let ats_certification = match resolve_ats_certification_for_posting_postgres_tx(
        tx,
        account_id,
        &posting,
        None,
        now_ms(),
    ) {
        Ok(resolution) => Some(resolution),
        Err(AtsCertificationAuthorityError::Storage(error)) => return Err(error),
        Err(_) => None,
    };

    let authorized = current_execution_authority_matches(
        account_id,
        application,
        &profile,
        &posting,
        &track,
        &identity,
        &facts,
        auto_submit_authorization.as_ref(),
        &preferences,
        &reservations,
        &authorities,
        ats_certification.as_ref(),
        runner,
    )? && stored_execution_evidence_matches_postgres(tx, account_id, application)?;
    Ok(authorized)
}

#[allow(clippy::too_many_arguments)]
fn current_execution_authority_matches(
    account_id: &str,
    application: &JobApplication,
    profile: &CareerProfile,
    posting: &JobPosting,
    track: &CareerTrack,
    identity: &ApplicationIdentity,
    facts: &[CareerFact],
    auto_submit_authorization: Option<&StoredAutoSubmitAuthorization>,
    preferences: &JobPreferences,
    reservations: &[AttemptReservation],
    authorities: &[JobDiscoveryAuthority],
    ats_certification: Option<&AtsCertificationPostingResolution>,
    runner: ExecutionAuthorityRunner,
) -> Result<bool> {
    let frozen_track = frozen_receipt_string(application, "/career_track_id");
    let frozen_identity = frozen_receipt_string(application, "/application_identity/id");
    let frozen_email = frozen_receipt_string(application, "/application_identity/email");
    let frozen_evidence_id = frozen_receipt_string(application, "/evidence_revision_id")
        .or_else(|| frozen_receipt_string(application, "/evidence_revision/id"));
    let frozen_evidence_hash = frozen_receipt_string(application, "/evidence_content_hash")
        .or_else(|| frozen_receipt_string(application, "/evidence_revision/content_hash"));
    if frozen_track.as_deref() != Some(track.id.as_str())
        || posting.track_id != track.id
        || !track.active
        || track.application_identity_id.as_deref() != Some(identity.id.as_str())
        || frozen_identity.as_deref() != Some(identity.id.as_str())
        || frozen_email.as_deref() != Some(identity.email.as_str())
        || identity.verification_status != "verified"
        || application.resume_version_id.is_none()
    {
        return Ok(false);
    }

    let mut decision = build_job_eligibility(
        posting,
        profile,
        preferences,
        reservations,
        true,
        Some(application.id.as_str()),
        Some(track),
    );
    apply_discovery_authorities(authorities, &mut decision);
    if let Some(resolution) = ats_certification {
        apply_ats_certification_resolution(posting, &mut decision, resolution);
    }
    let runner_authorized = match runner {
        ExecutionAuthorityRunner::Local => decision.can_queue_local,
        ExecutionAuthorityRunner::Cloud => decision.can_queue_cloud,
    };
    if !runner_authorized
        || (application.submission_mode == "auto_submit" && !decision.can_auto_submit)
    {
        return Ok(false);
    }
    if application.submission_mode == "auto_submit" {
        let Some(authorization) = auto_submit_authorization else {
            return Ok(false);
        };
        if !auto_submit_authorization_matches_inputs(
            authorization,
            profile,
            facts,
            track,
            identity,
        )? {
            return Ok(false);
        }
        let admission = application.receipt.pointer("/approved_execution/admission");
        let admission_matches = admission.is_some_and(|value| {
            value.get("kind").and_then(Value::as_str) == Some("track_auto_submit")
                && value.get("authorization_id").and_then(Value::as_str)
                    == Some(authorization.id.as_str())
                && value.get("career_track_id").and_then(Value::as_str)
                    == Some(track.id.as_str())
                && value.get("revision_no").and_then(Value::as_i64)
                    == Some(authorization.revision_no)
                && value
                    .get("authority_fingerprint")
                    .and_then(Value::as_str)
                    == Some(authorization.authority_fingerprint.as_str())
        });
        if !admission_matches {
            return Ok(false);
        }
    }

    let experience = role_experience_evidence(profile, Some(track), posting);
    let evidence = build_profile_evidence_revision(
        account_id,
        profile,
        facts,
        track,
        identity,
        &experience,
    )?;
    Ok(frozen_evidence_id.as_deref() == Some(evidence.id.as_str())
        && frozen_evidence_hash.as_deref() == Some(evidence.content_hash.as_str()))
}

fn stored_execution_evidence_matches_sqlite(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    application: &JobApplication,
) -> Result<bool> {
    let Some(resume_id) = application.resume_version_id.as_deref() else {
        return Ok(false);
    };
    let Some(evidence_id) = frozen_receipt_string(application, "/evidence_revision_id")
        .or_else(|| frozen_receipt_string(application, "/evidence_revision/id"))
    else {
        return Ok(false);
    };
    let Some(evidence_hash) = frozen_receipt_string(application, "/evidence_content_hash")
        .or_else(|| frozen_receipt_string(application, "/evidence_revision/content_hash"))
    else {
        return Ok(false);
    };
    let stored_evidence: Option<(String, String)> = tx
        .query_row(
            "SELECT career_track_id, content_hash FROM jobs_profile_evidence_revisions
              WHERE account_id = ?1 AND id = ?2",
            params![account_id, evidence_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let resume_exists: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM jobs_resume_versions
          WHERE account_id = ?1 AND id = ?2 AND job_id = ?3)",
        params![account_id, resume_id, application.job_id],
        |row| row.get(0),
    )?;
    let (claim_count, wrong_revision): (i64, i64) = tx.query_row(
        "SELECT COUNT(*), COALESCE(SUM(CASE WHEN evidence_revision_id <> ?3 THEN 1 ELSE 0 END), 0)
           FROM jobs_resume_claim_evidence
          WHERE account_id = ?1 AND resume_version_id = ?2",
        params![account_id, resume_id, evidence_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    Ok(stored_evidence.is_some_and(|(track_id, content_hash)| {
        frozen_receipt_string(application, "/career_track_id").as_deref()
            == Some(track_id.as_str())
            && content_hash == evidence_hash
    }) && resume_exists
        && claim_count > 0
        && wrong_revision == 0)
}

fn stored_execution_evidence_matches_postgres(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    application: &JobApplication,
) -> Result<bool> {
    let Some(resume_id) = application.resume_version_id.as_deref() else {
        return Ok(false);
    };
    let Some(evidence_id) = frozen_receipt_string(application, "/evidence_revision_id")
        .or_else(|| frozen_receipt_string(application, "/evidence_revision/id"))
    else {
        return Ok(false);
    };
    let Some(evidence_hash) = frozen_receipt_string(application, "/evidence_content_hash")
        .or_else(|| frozen_receipt_string(application, "/evidence_revision/content_hash"))
    else {
        return Ok(false);
    };
    let stored_evidence = tx.query_opt(
        "SELECT career_track_id, content_hash FROM jobs_profile_evidence_revisions
          WHERE account_id = $1 AND id = $2",
        &[&account_id, &evidence_id],
    )?;
    let resume_exists: bool = tx
        .query_one(
            "SELECT EXISTS(SELECT 1 FROM jobs_resume_versions
              WHERE account_id = $1 AND id = $2 AND job_id = $3)",
            &[&account_id, &resume_id, &application.job_id],
        )?
        .get(0);
    let claim_row = tx.query_one(
        "SELECT COUNT(*), COALESCE(SUM(CASE WHEN evidence_revision_id <> $3 THEN 1 ELSE 0 END), 0)
           FROM jobs_resume_claim_evidence
          WHERE account_id = $1 AND resume_version_id = $2",
        &[&account_id, &resume_id, &evidence_id],
    )?;
    let claim_count: i64 = claim_row.get(0);
    let wrong_revision: i64 = claim_row.get(1);
    Ok(stored_evidence.is_some_and(|row| {
        let track_id: String = row.get(0);
        let content_hash: String = row.get(1);
        frozen_receipt_string(application, "/career_track_id").as_deref()
            == Some(track_id.as_str())
            && content_hash == evidence_hash
    }) && resume_exists
        && claim_count > 0
        && wrong_revision == 0)
}

fn frozen_receipt_string(application: &JobApplication, pointer: &str) -> Option<String> {
    application
        .receipt
        .pointer(pointer)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
}
