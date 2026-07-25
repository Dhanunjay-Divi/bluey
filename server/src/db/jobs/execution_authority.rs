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

    let authorized = current_execution_authority_matches(
        account_id,
        application,
        &profile,
        &posting,
        &track,
        &identity,
        &facts,
        &preferences,
        &reservations,
        &authorities,
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

    let authorized = current_execution_authority_matches(
        account_id,
        application,
        &profile,
        &posting,
        &track,
        &identity,
        &facts,
        &preferences,
        &reservations,
        &authorities,
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
    preferences: &JobPreferences,
    reservations: &[AttemptReservation],
    authorities: &[JobDiscoveryAuthority],
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

    let context = EligibilityContext {
        profile,
        preferences,
        reservations,
        require_live_verification: true,
        existing_application_id: Some(application.id.as_str()),
        track: Some(track),
        identity: Some(identity),
    };
    let mut decision = build_job_eligibility(posting, &context);
    apply_discovery_authorities(authorities, &mut decision);
    let runner_authorized = match runner {
        ExecutionAuthorityRunner::Local => decision.can_queue_local,
        ExecutionAuthorityRunner::Cloud => decision.can_queue_cloud,
    };
    if !runner_authorized
        || (application.submission_mode == "auto_submit" && !decision.can_auto_submit)
    {
        return Ok(false);
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
    let stored_evidence: Option<(String, String, String)> = tx
        .query_row(
            "SELECT career_track_id, content_hash, snapshot_json
               FROM jobs_profile_evidence_revisions
              WHERE account_id = ?1 AND id = ?2",
            params![account_id, evidence_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    let resume_claim_ids = tx
        .query_row(
            "SELECT claim_ids_json
               FROM jobs_resume_versions
              WHERE account_id = ?1 AND id = ?2 AND job_id = ?3",
            params![account_id, resume_id, application.job_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        .map(|raw| parse_json::<Vec<String>>(raw, "Jobs resume claim ids"))
        .transpose()?;
    let mut claim_stmt = tx.prepare(
        "SELECT claim_id, evidence_revision_id
           FROM jobs_resume_claim_evidence
          WHERE account_id = ?1 AND resume_version_id = ?2",
    )?;
    let claim_rows = claim_stmt
        .query_map(params![account_id, resume_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(stored_evidence.is_some_and(|(track_id, content_hash, snapshot_raw)| {
        frozen_receipt_string(application, "/career_track_id").as_deref()
            == Some(track_id.as_str())
            && content_hash == evidence_hash
            && parse_json::<Value>(snapshot_raw, "Jobs evidence revision")
                .and_then(|snapshot| evidence_snapshot_content_hash(&snapshot))
                .is_ok_and(|recomputed| recomputed == content_hash)
    }) && claim_sets_match_evidence(resume_claim_ids, &claim_rows, &evidence_id))
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
        "SELECT career_track_id, content_hash, snapshot_json
           FROM jobs_profile_evidence_revisions
          WHERE account_id = $1 AND id = $2",
        &[&account_id, &evidence_id],
    )?;
    let resume_claim_ids = tx
        .query_opt(
            "SELECT claim_ids_json
               FROM jobs_resume_versions
              WHERE account_id = $1 AND id = $2 AND job_id = $3",
            &[&account_id, &resume_id, &application.job_id],
        )?
        .map(|row| parse_json::<Vec<String>>(row.get(0), "Jobs resume claim ids"))
        .transpose()?;
    let claim_rows = tx
        .query(
            "SELECT claim_id, evidence_revision_id
           FROM jobs_resume_claim_evidence
          WHERE account_id = $1 AND resume_version_id = $2",
            &[&account_id, &resume_id],
        )?
        .into_iter()
        .map(|row| (row.get::<_, String>(0), row.get::<_, String>(1)))
        .collect::<Vec<_>>();
    Ok(stored_evidence.is_some_and(|row| {
        let track_id: String = row.get(0);
        let content_hash: String = row.get(1);
        let snapshot_raw: String = row.get(2);
        frozen_receipt_string(application, "/career_track_id").as_deref()
            == Some(track_id.as_str())
            && content_hash == evidence_hash
            && parse_json::<Value>(snapshot_raw, "Jobs evidence revision")
                .and_then(|snapshot| evidence_snapshot_content_hash(&snapshot))
                .is_ok_and(|recomputed| recomputed == content_hash)
    }) && claim_sets_match_evidence(resume_claim_ids, &claim_rows, &evidence_id))
}

fn claim_sets_match_evidence(
    resume_claim_ids: Option<Vec<String>>,
    claim_rows: &[(String, String)],
    evidence_id: &str,
) -> bool {
    let Some(resume_claim_ids) = resume_claim_ids.and_then(normalized_nonempty_claim_ids) else {
        return false;
    };
    if claim_rows
        .iter()
        .any(|(_, evidence_revision_id)| evidence_revision_id != evidence_id)
    {
        return false;
    }
    normalized_nonempty_claim_ids(
        claim_rows
            .iter()
            .map(|(claim_id, _)| claim_id.clone())
            .collect(),
    )
    .is_some_and(|evidence_claim_ids| evidence_claim_ids == resume_claim_ids)
}

fn normalized_nonempty_claim_ids(claim_ids: Vec<String>) -> Option<Vec<String>> {
    let mut claim_ids = claim_ids
        .into_iter()
        .map(|claim_id| claim_id.trim().to_string())
        .filter(|claim_id| !claim_id.is_empty())
        .collect::<Vec<_>>();
    claim_ids.sort();
    claim_ids.dedup();
    (!claim_ids.is_empty()).then_some(claim_ids)
}

fn frozen_receipt_string(application: &JobApplication, pointer: &str) -> Option<String> {
    application
        .receipt
        .pointer(pointer)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
}

#[cfg(test)]
mod execution_authority_tests {
    use super::claim_sets_match_evidence;

    #[test]
    fn claim_sets_require_an_exact_nonempty_revision_bound_match() {
        let rows = vec![
            ("claim-b".to_string(), "evidence-1".to_string()),
            ("claim-a".to_string(), "evidence-1".to_string()),
        ];

        assert!(claim_sets_match_evidence(
            Some(vec![
                " claim-a ".to_string(),
                "claim-b".to_string(),
                "claim-a".to_string(),
            ]),
            &rows,
            "evidence-1",
        ));
        assert!(!claim_sets_match_evidence(None, &rows, "evidence-1"));
        assert!(!claim_sets_match_evidence(
            Some(Vec::new()),
            &rows,
            "evidence-1",
        ));
        assert!(!claim_sets_match_evidence(
            Some(vec!["claim-a".to_string()]),
            &rows,
            "evidence-1",
        ));
        assert!(!claim_sets_match_evidence(
            Some(vec![
                "claim-a".to_string(),
                "claim-b".to_string(),
                "claim-c".to_string(),
            ]),
            &rows,
            "evidence-1",
        ));

        let wrong_revision = vec![
            ("claim-a".to_string(), "evidence-1".to_string()),
            ("claim-b".to_string(), "evidence-2".to_string()),
        ];
        assert!(!claim_sets_match_evidence(
            Some(vec!["claim-a".to_string(), "claim-b".to_string()]),
            &wrong_revision,
            "evidence-1",
        ));
    }
}
