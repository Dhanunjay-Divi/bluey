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

fn auto_submit_authority_fingerprint(
    profile: &CareerProfile,
    facts: &[CareerFact],
    track: &CareerTrack,
    identity: &ApplicationIdentity,
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
            "source_resume_asset_id": track.source_resume_asset_id,
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
    });
    let encoded = serde_json::to_vec(&snapshot).context("serialize Auto-submit authority")?;
    Ok(hex::encode(Sha256::digest(encoded)))
}

fn auto_submit_authority_inputs(
    pool: &DbPool,
    account_id: &str,
    account_email: &str,
    track_id: &str,
) -> Result<(
    CareerProfile,
    Vec<CareerFact>,
    CareerTrack,
    ApplicationIdentity,
)> {
    let profile = get_profile(pool, account_id, account_email)?;
    if !profile.onboarding_complete {
        anyhow::bail!("finish your Career Profile before enabling Auto-submit")
    }
    let track = list_tracks(pool, account_id)?
        .into_iter()
        .find(|track| track.id == track_id)
        .ok_or_else(|| anyhow::anyhow!("Career Track not found"))?;
    if !track.active {
        anyhow::bail!("activate this Career Track before enabling Auto-submit")
    }
    if track.source_resume_asset_id.trim().is_empty()
        || profile.source_resume_asset_id != track.source_resume_asset_id
        || profile.source_resume_sha256.trim().is_empty()
    {
        anyhow::bail!("review and save the current source resume on this Career Track")
    }
    let identity_id = track
        .application_identity_id
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| anyhow::anyhow!("choose a verified application email"))?;
    let identity = get_application_identity(pool, account_id, identity_id)?
        .ok_or_else(|| anyhow::anyhow!("application email not found"))?;
    if identity.verification_status != "verified" {
        anyhow::bail!("verify the application email before enabling Auto-submit")
    }
    let facts = list_facts(pool, account_id)?;
    Ok((profile, facts, track, identity))
}

fn stored_auto_submit_authorizations(
    pool: &DbPool,
    account_id: &str,
) -> Result<Vec<StoredAutoSubmitAuthorization>> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get()?;
            let mut stmt = conn.prepare(
                "SELECT id, career_track_id, application_identity_id,
                        source_resume_asset_id, authority_fingerprint, revision_no,
                        authorized_at_ms, revoked_at_ms
                   FROM jobs_auto_submit_authorizations
                  WHERE account_id = ?1
                  ORDER BY authorized_at_ms DESC",
            )?;
            let rows = stmt.query_map(params![account_id], |row| {
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
            })?;
            let values = rows.collect::<std::result::Result<Vec<_>, _>>()?;
            Ok(values)
        }
        DbPool::Postgres(_) => Ok(pool
            .get_pg()?
            .query(
                "SELECT id, career_track_id, application_identity_id,
                        source_resume_asset_id, authority_fingerprint, revision_no,
                        authorized_at_ms, revoked_at_ms
                   FROM jobs_auto_submit_authorizations
                  WHERE account_id = $1
                  ORDER BY authorized_at_ms DESC",
                &[&account_id],
            )?
            .into_iter()
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
            .collect()),
    })
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
    account_email: &str,
    track_id: &str,
) -> Result<Option<AutoSubmitAuthorization>> {
    let Some(stored) = stored_auto_submit_authorizations(pool, account_id)?
        .into_iter()
        .find(|item| item.career_track_id == track_id && item.revoked_at_ms.is_none())
    else {
        return Ok(None);
    };
    let Ok((profile, facts, track, identity)) =
        auto_submit_authority_inputs(pool, account_id, account_email, track_id)
    else {
        return Ok(Some(public_auto_submit_authorization(
            &stored,
            "needs_review",
        )));
    };
    let fingerprint = auto_submit_authority_fingerprint(&profile, &facts, &track, &identity)?;
    let status = if stored.application_identity_id == identity.id
        && stored.source_resume_asset_id == track.source_resume_asset_id
        && stored.authority_fingerprint == fingerprint
    {
        "active"
    } else {
        "needs_review"
    };
    Ok(Some(public_auto_submit_authorization(&stored, status)))
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
    account_email: &str,
    track_id: &str,
) -> Result<AutoSubmitAuthorization> {
    let (profile, facts, track, identity) =
        auto_submit_authority_inputs(pool, account_id, account_email, track_id)?;
    let fingerprint = auto_submit_authority_fingerprint(&profile, &facts, &track, &identity)?;
    let now = now_ms();
    let id = uuid::Uuid::new_v4().to_string();
    let source_resume_asset_id = track.source_resume_asset_id.clone();
    let application_identity_id = identity.id.clone();
    let revision_no = crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
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
                    application_identity_id,
                    source_resume_asset_id,
                    fingerprint,
                    revision_no,
                    now,
                ],
            )?;
            tx.commit()?;
            Ok::<i64, anyhow::Error>(revision_no)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            tx.query_one(
                "SELECT pg_advisory_xact_lock(hashtextextended($1, 0))",
                &[&format!("{account_id}:{track_id}:auto-submit")],
            )?;
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
                    &application_identity_id,
                    &source_resume_asset_id,
                    &fingerprint,
                    &revision_no,
                    &now,
                ],
            )?;
            tx.commit()?;
            Ok::<i64, anyhow::Error>(revision_no)
        }
    })?;
    let value = current_auto_submit_authorization(pool, account_id, account_email, track_id)?
        .ok_or_else(|| anyhow::anyhow!("Auto-submit authorization was not saved"))?;
    if value.status != "active" || value.revision_no != revision_no {
        anyhow::bail!("Career Track changed while enabling Auto-submit; review it and try again")
    }
    Ok(value)
}

pub fn revoke_auto_submit(
    pool: &DbPool,
    account_id: &str,
    track_id: &str,
) -> Result<bool> {
    let now = now_ms();
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => Ok(pool.get()?.execute(
            "UPDATE jobs_auto_submit_authorizations
                SET revoked_at_ms = ?3
              WHERE account_id = ?1 AND career_track_id = ?2
                AND revoked_at_ms IS NULL",
            params![account_id, track_id, now],
        )? > 0),
        DbPool::Postgres(_) => Ok(pool.get_pg()?.execute(
            "UPDATE jobs_auto_submit_authorizations
                SET revoked_at_ms = $3
              WHERE account_id = $1 AND career_track_id = $2
                AND revoked_at_ms IS NULL",
            &[&account_id, &track_id, &now],
        )? > 0),
    })
}

pub fn require_valid_auto_submit_authorization(
    pool: &DbPool,
    account_id: &str,
    account_email: &str,
    track_id: &str,
) -> Result<AutoSubmitAuthorization> {
    let authorization =
        current_auto_submit_authorization(pool, account_id, account_email, track_id)?
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "Enable Auto-submit on this Career Track before queuing applications."
                )
            })?;
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
) -> Result<bool> {
    if authorization.revoked_at_ms.is_some()
        || authorization.career_track_id != track.id
        || authorization.application_identity_id != identity.id
        || authorization.source_resume_asset_id != track.source_resume_asset_id
        || track.source_resume_asset_id != profile.source_resume_asset_id
    {
        return Ok(false);
    }
    Ok(authorization.authority_fingerprint
        == auto_submit_authority_fingerprint(profile, facts, track, identity)?)
}
