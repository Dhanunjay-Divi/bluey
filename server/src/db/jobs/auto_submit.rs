#[derive(Debug, Clone)]
struct StoredAutoSubmitAuthorization {
    id: String,
    career_track_id: String,
    application_identity_id: String,
    source_resume_asset_id: String,
    authority_fingerprint: String,
    revision_no: i64,
    authorized_at_ms: i64,
    revoked_at_ms: Option<i64>,
}

fn lock_auto_submit_authority_postgres(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    track_id: &str,
    exclusive: bool,
) -> Result<()> {
    let sql = if exclusive {
        "SELECT pg_advisory_xact_lock(hashtextextended($1, 0))"
    } else {
        "SELECT pg_advisory_xact_lock_shared(hashtextextended($1, 0))"
    };
    tx.query_one(sql, &[&format!("{account_id}:{track_id}:auto-submit")])?;
    Ok(())
}

fn stored_auto_submit_from_sqlite_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<StoredAutoSubmitAuthorization> {
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
}

fn stored_auto_submit_from_postgres_row(row: postgres::Row) -> StoredAutoSubmitAuthorization {
    StoredAutoSubmitAuthorization {
        id: row.get(0),
        career_track_id: row.get(1),
        application_identity_id: row.get(2),
        source_resume_asset_id: row.get(3),
        authority_fingerprint: row.get(4),
        revision_no: row.get(5),
        authorized_at_ms: row.get(6),
        revoked_at_ms: row.get(7),
    }
}

fn auto_submit_authority_fingerprint(
    profile: &CareerProfile,
    facts: &[CareerFact],
    track: &CareerTrack,
    identity: &ApplicationIdentity,
    preferences: &JobPreferences,
) -> Result<String> {
    let snapshot = json!({
        "schema_version": 1,
        "career_track": {
            "id": track.id,
            "name": track.name,
            "role": track.role,
            "locations": track.locations,
            "remote_preference": track.remote_preference,
            "application_identity_id": track.application_identity_id,
            "policy": track.policy,
            "active": track.active,
        },
        "application_identity": {
            "id": identity.id,
            "email": identity.email,
            "label": identity.label,
            "verification_status": identity.verification_status,
        },
        "source_resume": {
            "asset_id": profile.source_resume_asset_id,
            "sha256": profile.source_resume_sha256,
            "media_type": profile.source_resume_media_type,
            "template_status": profile.source_resume_template_status,
        },
        "candidate_truth_fingerprint": candidate_truth_fingerprint(profile),
        "confirmed_facts_fingerprint": confirmed_facts_fingerprint(facts),
        "job_preferences": {
            "desired_roles": preferences.desired_roles,
            "desired_locations": preferences.desired_locations,
            "location_policy": preferences.location_policy,
            "remote_preference": preferences.remote_preference,
            "employment_types": preferences.employment_types,
            "engagement_types": preferences.engagement_types,
            "minimum_compensation": preferences.minimum_compensation,
            "sponsorship": preferences.sponsorship,
            "excluded_companies": preferences.excluded_companies,
            "excluded_titles": preferences.excluded_titles,
            "daily_limit": preferences.daily_limit,
            "apply_once_per_company": preferences.apply_once_per_company,
            "max_posting_age_days": preferences.max_posting_age_days,
            "time_zone_offset_minutes": preferences.time_zone_offset_minutes,
        },
        "taxonomy_descriptor": {
            "version": crate::jobs_taxonomy::taxonomy_version(),
            "sha256": crate::jobs_taxonomy::taxonomy_sha256(),
        },
    });
    let encoded = serde_json::to_vec(&snapshot).context("serialize Auto-submit authority")?;
    Ok(hex::encode(Sha256::digest(encoded)))
}

fn validate_auto_submit_authority_inputs(
    profile: &CareerProfile,
    track: &CareerTrack,
    identity: &ApplicationIdentity,
) -> Result<()> {
    anyhow::ensure!(
        profile.onboarding_complete,
        "finish your Career Profile before enabling Auto-submit"
    );
    anyhow::ensure!(
        track.active,
        "activate this Career Track before enabling Auto-submit"
    );
    let policy = &track.policy.authority;
    anyhow::ensure!(
        policy.review_state == "approved"
            && policy.policy_revision_no >= 1
            && !policy.policy_revision_id.is_empty()
            && policy.canonical_policy_sha256.len() == 64
            && policy.policy_head_generation == policy.policy_revision_no
            && policy.policy_head_transition_sha256.len() == 64
            && !policy.policy_review_receipt_id.is_empty()
            && policy.policy_review_receipt_sha256.len() == 64,
        "review and approve the current canonical Career Track policy before enabling Auto-submit"
    );
    anyhow::ensure!(
        !profile.source_resume_asset_id.trim().is_empty()
            && !profile.source_resume_sha256.trim().is_empty(),
        "review and save the current source resume before enabling Auto-submit"
    );
    anyhow::ensure!(
        identity.verification_status == "verified",
        "verify the application email before enabling Auto-submit"
    );
    Ok(())
}

fn auto_submit_authority_inputs_sqlite_tx(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    track_id: &str,
) -> Result<(
    CareerProfile,
    Vec<CareerFact>,
    CareerTrack,
    ApplicationIdentity,
    JobPreferences,
)> {
    let profile: CareerProfile = parse_json(
        tx.query_row(
            "SELECT profile_json FROM jobs_profiles WHERE account_id = ?1",
            params![account_id],
            |row| row.get::<_, String>(0),
        )?,
        "Auto-submit profile",
    )?;
    let (track_row_id, active, track_json): (String, i64, String) = tx
        .query_row(
            "SELECT id, active, track_json FROM jobs_tracks
              WHERE account_id = ?1 AND id = ?2",
            params![account_id, track_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .context("Career Track not found")?;
    let mut track: CareerTrack = parse_json(track_json, "Auto-submit Career Track")?;
    track.id = track_row_id;
    anyhow::ensure!(
        track.active == (active != 0),
        "Career Track projection is stale"
    );
    validate_track_policy_ledger_sqlite(tx, account_id, &track)?;
    let identity_id = track
        .application_identity_id
        .as_deref()
        .context("choose a verified application email")?;
    let identity_row = tx
        .query_row(
            "SELECT identity_json, verification_status, is_default
               FROM jobs_application_identities
              WHERE account_id = ?1 AND id = ?2",
            params![account_id, identity_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            },
        )
        .context("application email not found")?;
    let identity =
        parse_application_identity_row(identity_row.0, identity_row.1, identity_row.2 != 0)?;
    let facts = {
        let mut stmt = tx.prepare(
            "SELECT id, category, label, value_json, source, verification_status,
                    confirmed_at_ms, confirmed_by, schema_version, created_at_ms, updated_at_ms
               FROM jobs_facts WHERE account_id = ?1 ORDER BY id",
        )?;
        let values = stmt
            .query_map(params![account_id], fact_from_sqlite_row)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        values
    };
    let preferences = tx
        .query_row(
            "SELECT preferences_json FROM jobs_preferences WHERE account_id = ?1",
            params![account_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        .map(|raw| parse_json::<JobPreferences>(raw, "Auto-submit preferences"))
        .transpose()?
        .map(enforce_job_preference_safety)
        .unwrap_or_default();
    validate_auto_submit_authority_inputs(&profile, &track, &identity)?;
    Ok((profile, facts, track, identity, preferences))
}

fn auto_submit_authority_inputs_postgres_tx(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    track_id: &str,
) -> Result<(
    CareerProfile,
    Vec<CareerFact>,
    CareerTrack,
    ApplicationIdentity,
    JobPreferences,
)> {
    let profile: CareerProfile = parse_json(
        tx.query_one(
            "SELECT profile_json FROM jobs_profiles WHERE account_id = $1",
            &[&account_id],
        )?
        .get(0),
        "Auto-submit profile",
    )?;
    let row = tx
        .query_opt(
            "SELECT id, active, track_json FROM jobs_tracks
              WHERE account_id = $1 AND id = $2",
            &[&account_id, &track_id],
        )?
        .context("Career Track not found")?;
    let mut track: CareerTrack = parse_json(row.get(2), "Auto-submit Career Track")?;
    track.id = row.get(0);
    anyhow::ensure!(
        track.active == (row.get::<_, i32>(1) != 0),
        "Career Track projection is stale"
    );
    validate_track_policy_ledger_postgres(tx, account_id, &track)?;
    let identity_id = track
        .application_identity_id
        .as_deref()
        .context("choose a verified application email")?;
    let row = tx
        .query_opt(
            "SELECT identity_json, verification_status, is_default
               FROM jobs_application_identities
              WHERE account_id = $1 AND id = $2",
            &[&account_id, &identity_id],
        )?
        .context("application email not found")?;
    let identity =
        parse_application_identity_row(row.get(0), row.get(1), row.get::<_, i32>(2) != 0)?;
    let facts = tx
        .query(
            "SELECT id, category, label, value_json, source, verification_status,
                    confirmed_at_ms, confirmed_by, schema_version, created_at_ms, updated_at_ms
               FROM jobs_facts WHERE account_id = $1 ORDER BY id",
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
        .map(|row| parse_json::<JobPreferences>(row.get(0), "Auto-submit preferences"))
        .transpose()?
        .map(enforce_job_preference_safety)
        .unwrap_or_default();
    validate_auto_submit_authority_inputs(&profile, &track, &identity)?;
    Ok((profile, facts, track, identity, preferences))
}

fn public_auto_submit_authorization(
    stored: &StoredAutoSubmitAuthorization,
    status: &str,
) -> AutoSubmitAuthorization {
    AutoSubmitAuthorization {
        id: stored.id.clone(),
        career_track_id: stored.career_track_id.clone(),
        application_identity_id: stored.application_identity_id.clone(),
        source_resume_asset_id: stored.source_resume_asset_id.clone(),
        revision_no: stored.revision_no,
        authorized_at_ms: stored.authorized_at_ms,
        revoked_at_ms: stored.revoked_at_ms,
        status: status.to_string(),
        authority_fingerprint: stored.authority_fingerprint.clone(),
    }
}

fn current_auto_submit_authorization(
    pool: &DbPool,
    account_id: &str,
    _account_email: &str,
    track_id: &str,
) -> Result<Option<AutoSubmitAuthorization>> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let stored = tx
                .query_row(
                    "SELECT id, career_track_id, application_identity_id,
                            source_resume_asset_id, authority_fingerprint, revision_no,
                            authorized_at_ms, revoked_at_ms
                       FROM jobs_auto_submit_authorizations
                      WHERE account_id = ?1 AND career_track_id = ?2
                        AND revoked_at_ms IS NULL
                      ORDER BY authorized_at_ms DESC LIMIT 1",
                    params![account_id, track_id],
                    stored_auto_submit_from_sqlite_row,
                )
                .optional()?;
            let Some(stored) = stored else {
                tx.commit()?;
                return Ok(None);
            };
            let inputs = auto_submit_authority_inputs_sqlite_tx(&tx, account_id, track_id);
            let status = match inputs {
                Ok((profile, facts, track, identity, preferences)) => {
                    let fingerprint = auto_submit_authority_fingerprint(
                        &profile,
                        &facts,
                        &track,
                        &identity,
                        &preferences,
                    )?;
                    if stored.application_identity_id == identity.id
                        && stored.source_resume_asset_id == profile.source_resume_asset_id
                        && stored.authority_fingerprint == fingerprint
                    {
                        "active"
                    } else {
                        "needs_review"
                    }
                }
                Err(_) => "needs_review",
            };
            let value = public_auto_submit_authorization(&stored, status);
            tx.commit()?;
            Ok(Some(value))
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            lock_discovery_account_shared_postgres(&mut tx, account_id)?;
            lock_account_policy_inputs_postgres(&mut tx, account_id, false)?;
            lock_auto_submit_authority_postgres(&mut tx, account_id, track_id, false)?;
            let stored = tx
                .query_opt(
                    "SELECT id, career_track_id, application_identity_id,
                            source_resume_asset_id, authority_fingerprint, revision_no,
                            authorized_at_ms, revoked_at_ms
                       FROM jobs_auto_submit_authorizations
                      WHERE account_id = $1 AND career_track_id = $2
                        AND revoked_at_ms IS NULL
                      ORDER BY authorized_at_ms DESC LIMIT 1
                      FOR SHARE",
                    &[&account_id, &track_id],
                )?
                .map(stored_auto_submit_from_postgres_row);
            let Some(stored) = stored else {
                tx.commit()?;
                return Ok(None);
            };
            let inputs = auto_submit_authority_inputs_postgres_tx(&mut tx, account_id, track_id);
            let status = match inputs {
                Ok((profile, facts, track, identity, preferences)) => {
                    let fingerprint = auto_submit_authority_fingerprint(
                        &profile,
                        &facts,
                        &track,
                        &identity,
                        &preferences,
                    )?;
                    if stored.application_identity_id == identity.id
                        && stored.source_resume_asset_id == profile.source_resume_asset_id
                        && stored.authority_fingerprint == fingerprint
                    {
                        "active"
                    } else {
                        "needs_review"
                    }
                }
                Err(_) => "needs_review",
            };
            let value = public_auto_submit_authorization(&stored, status);
            tx.commit()?;
            Ok(Some(value))
        }
    })
}

pub fn list_auto_submit_authorizations(
    pool: &DbPool,
    account_id: &str,
    account_email: &str,
) -> Result<Vec<AutoSubmitAuthorization>> {
    let track_ids = list_tracks(pool, account_id)?
        .into_iter()
        .map(|track| track.id)
        .collect::<Vec<_>>();
    track_ids
        .iter()
        .map(|track_id| {
            current_auto_submit_authorization(pool, account_id, account_email, track_id)
        })
        .filter_map(|result| match result {
            Ok(Some(value)) => Some(Ok(value)),
            Ok(None) => None,
            Err(error) => Some(Err(error)),
        })
        .collect()
}

pub fn authorize_auto_submit(
    pool: &DbPool,
    account_id: &str,
    _account_email: &str,
    track_id: &str,
) -> Result<AutoSubmitAuthorization> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let (profile, facts, track, identity, preferences) =
                auto_submit_authority_inputs_sqlite_tx(&tx, account_id, track_id)?;
            let fingerprint = auto_submit_authority_fingerprint(
                &profile,
                &facts,
                &track,
                &identity,
                &preferences,
            )?;
            let now = now_ms();
            let id = uuid::Uuid::new_v4().to_string();
            let revision_no = tx.query_row(
                "SELECT COALESCE(MAX(revision_no), 0) + 1
                   FROM jobs_auto_submit_authorizations
                  WHERE account_id = ?1 AND career_track_id = ?2",
                params![account_id, track_id],
                |row| row.get::<_, i64>(0),
            )?;
            tx.execute(
                "UPDATE jobs_auto_submit_authorizations
                    SET revoked_at_ms = ?3
                  WHERE account_id = ?1 AND career_track_id = ?2
                    AND revoked_at_ms IS NULL",
                params![account_id, track_id, now],
            )?;
            tx.execute(
                "INSERT INTO jobs_auto_submit_authorizations(
                    id, account_id, career_track_id, application_identity_id,
                    source_resume_asset_id, authority_fingerprint, revision_no,
                    authorized_at_ms, revoked_at_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, NULL)",
                params![
                    id,
                    account_id,
                    track_id,
                    identity.id,
                    profile.source_resume_asset_id,
                    fingerprint,
                    revision_no,
                    now,
                ],
            )?;
            tx.commit()?;
            Ok(public_auto_submit_authorization(
                &StoredAutoSubmitAuthorization {
                    id,
                    career_track_id: track_id.to_string(),
                    application_identity_id: identity.id,
                    source_resume_asset_id: profile.source_resume_asset_id,
                    authority_fingerprint: fingerprint,
                    revision_no,
                    authorized_at_ms: now,
                    revoked_at_ms: None,
                },
                "active",
            ))
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            lock_discovery_account_shared_postgres(&mut tx, account_id)?;
            lock_account_policy_inputs_postgres(&mut tx, account_id, false)?;
            lock_auto_submit_authority_postgres(&mut tx, account_id, track_id, true)?;
            let (profile, facts, track, identity, preferences) =
                auto_submit_authority_inputs_postgres_tx(&mut tx, account_id, track_id)?;
            let fingerprint = auto_submit_authority_fingerprint(
                &profile,
                &facts,
                &track,
                &identity,
                &preferences,
            )?;
            let now = now_ms();
            let id = uuid::Uuid::new_v4().to_string();
            let revision_no = tx
                .query_one(
                    "SELECT COALESCE(MAX(revision_no), 0) + 1
                       FROM jobs_auto_submit_authorizations
                      WHERE account_id = $1 AND career_track_id = $2",
                    &[&account_id, &track_id],
                )?
                .get::<_, i64>(0);
            tx.execute(
                "UPDATE jobs_auto_submit_authorizations
                    SET revoked_at_ms = $3
                  WHERE account_id = $1 AND career_track_id = $2
                    AND revoked_at_ms IS NULL",
                &[&account_id, &track_id, &now],
            )?;
            tx.execute(
                "INSERT INTO jobs_auto_submit_authorizations(
                    id, account_id, career_track_id, application_identity_id,
                    source_resume_asset_id, authority_fingerprint, revision_no,
                    authorized_at_ms, revoked_at_ms
                 ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, NULL)",
                &[
                    &id,
                    &account_id,
                    &track_id,
                    &identity.id,
                    &profile.source_resume_asset_id,
                    &fingerprint,
                    &revision_no,
                    &now,
                ],
            )?;
            tx.commit()?;
            Ok(public_auto_submit_authorization(
                &StoredAutoSubmitAuthorization {
                    id,
                    career_track_id: track_id.to_string(),
                    application_identity_id: identity.id,
                    source_resume_asset_id: profile.source_resume_asset_id,
                    authority_fingerprint: fingerprint,
                    revision_no,
                    authorized_at_ms: now,
                    revoked_at_ms: None,
                },
                "active",
            ))
        }
    })
}

pub fn revoke_auto_submit(pool: &DbPool, account_id: &str, track_id: &str) -> Result<bool> {
    let now = now_ms();
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let revoked = tx.execute(
                "UPDATE jobs_auto_submit_authorizations
                    SET revoked_at_ms = ?3
                  WHERE account_id = ?1 AND career_track_id = ?2
                    AND revoked_at_ms IS NULL",
                params![account_id, track_id, now],
            )? > 0;
            tx.commit()?;
            Ok(revoked)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            lock_discovery_account_shared_postgres(&mut tx, account_id)?;
            lock_account_policy_inputs_postgres(&mut tx, account_id, false)?;
            lock_auto_submit_authority_postgres(&mut tx, account_id, track_id, true)?;
            let revoked = tx.execute(
                "UPDATE jobs_auto_submit_authorizations
                    SET revoked_at_ms = $3
                  WHERE account_id = $1 AND career_track_id = $2
                    AND revoked_at_ms IS NULL",
                &[&account_id, &track_id, &now],
            )? > 0;
            tx.commit()?;
            Ok(revoked)
        }
    })
}

pub fn require_valid_auto_submit_authorization(
    pool: &DbPool,
    account_id: &str,
    account_email: &str,
    track_id: &str,
) -> Result<AutoSubmitAuthorization> {
    let authorization =
        current_auto_submit_authorization(pool, account_id, account_email, track_id)?.ok_or_else(
            || {
                anyhow::anyhow!(
                    "Enable Auto-submit on this Career Track before queuing applications."
                )
            },
        )?;
    if authorization.status != "active" {
        anyhow::bail!(
            "Career Track details changed. Review the identity and resume, then enable Auto-submit again."
        )
    }
    Ok(authorization)
}

fn auto_submit_authorization_matches_inputs(
    authorization: &StoredAutoSubmitAuthorization,
    profile: &CareerProfile,
    facts: &[CareerFact],
    track: &CareerTrack,
    identity: &ApplicationIdentity,
    preferences: &JobPreferences,
) -> Result<bool> {
    if authorization.revoked_at_ms.is_some()
        || authorization.career_track_id != track.id
        || authorization.application_identity_id != identity.id
        || authorization.source_resume_asset_id != profile.source_resume_asset_id
    {
        return Ok(false);
    }
    Ok(authorization.authority_fingerprint
        == auto_submit_authority_fingerprint(profile, facts, track, identity, preferences)?)
}
