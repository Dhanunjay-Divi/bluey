
pub fn list_discovery_sources(pool: &DbPool, account_id: &str) -> Result<Vec<DiscoverySource>> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get()?;
            let mut stmt = conn.prepare(
                "SELECT id, account_id, track_id, provider, source_key, source_json,
                        status, health, consecutive_failures, run_interval_ms,
                        next_run_at_ms, last_success_at_ms, last_failure_at_ms,
                        last_error_code, lease_expires_at_ms, created_at_ms, updated_at_ms
                   FROM jobs_discovery_sources WHERE account_id = ?1
                  ORDER BY created_at_ms ASC",
            )?;
            let rows = stmt.query_map(params![account_id], discovery_source_from_sqlite_row)?;
            rows.collect::<std::result::Result<Vec<_>, _>>()
                .context("list Jobs discovery sources")
        }
        DbPool::Postgres(_) => pool
            .get_pg()?
            .query(
                "SELECT id, account_id, track_id, provider, source_key, source_json,
                        status, health, consecutive_failures, run_interval_ms,
                        next_run_at_ms, last_success_at_ms, last_failure_at_ms,
                        last_error_code, lease_expires_at_ms, created_at_ms, updated_at_ms
                   FROM jobs_discovery_sources WHERE account_id = $1
                  ORDER BY created_at_ms ASC",
                &[&account_id],
            )?
            .into_iter()
            .map(discovery_source_from_pg_row)
            .collect(),
    })
}

/// Return the authoritative job currently bound to one verified
/// source/external-ID pair. Workspace backfill uses this to repair sources
/// created by the pre-membership importer without rewriting healthy matches
/// on every read.
pub fn verified_import_discovery_membership_job_id(
    pool: &DbPool,
    account_id: &str,
    provider: &str,
    source_key: &str,
    external_id: &str,
) -> Result<Option<String>> {
    let provider = provider.trim().to_ascii_lowercase();
    let source_key = source_key.trim();
    let external_id = external_id.trim();
    if provider.is_empty() || source_key.is_empty() || external_id.is_empty() {
        return Ok(None);
    }
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => pool
            .get()?
            .query_row(
                "SELECT m.job_id
                   FROM jobs_discovery_memberships m
                   JOIN jobs_discovery_sources s ON s.id = m.source_id
                  WHERE s.account_id = ?1 AND s.provider = ?2 AND s.source_key = ?3
                    AND m.account_id = ?1 AND m.external_id = ?4",
                params![account_id, provider, source_key, external_id],
                |row| row.get(0),
            )
            .optional()
            .context("get verified-import discovery membership"),
        DbPool::Postgres(_) => pool
            .get_pg()?
            .query_opt(
                "SELECT m.job_id
                   FROM jobs_discovery_memberships m
                   JOIN jobs_discovery_sources s ON s.id = m.source_id
                  WHERE s.account_id = $1 AND s.provider = $2 AND s.source_key = $3
                    AND m.account_id = $1 AND m.external_id = $4",
                &[&account_id, &provider, &source_key, &external_id],
            )
            .map(|row| row.map(|row| row.get(0)))
            .context("get verified-import discovery membership"),
    })
}

pub fn get_discovery_source(pool: &DbPool, source_id: &str) -> Result<Option<DiscoverySource>> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => pool
            .get()?
            .query_row(
                "SELECT id, account_id, track_id, provider, source_key, source_json,
                        status, health, consecutive_failures, run_interval_ms,
                        next_run_at_ms, last_success_at_ms, last_failure_at_ms,
                        last_error_code, lease_expires_at_ms, created_at_ms, updated_at_ms
                   FROM jobs_discovery_sources WHERE id = ?1",
                params![source_id],
                discovery_source_from_sqlite_row,
            )
            .optional()
            .context("get Jobs discovery source"),
        DbPool::Postgres(_) => pool
            .get_pg()?
            .query_opt(
                "SELECT id, account_id, track_id, provider, source_key, source_json,
                        status, health, consecutive_failures, run_interval_ms,
                        next_run_at_ms, last_success_at_ms, last_failure_at_ms,
                        last_error_code, lease_expires_at_ms, created_at_ms, updated_at_ms
                   FROM jobs_discovery_sources WHERE id = $1",
                &[&source_id],
            )?
            .map(discovery_source_from_pg_row)
            .transpose(),
    })
}

pub fn upsert_discovery_source(
    pool: &DbPool,
    account_id: &str,
    input: &DiscoverySourceInput,
) -> Result<DiscoverySource> {
    // Keep this validation before the write: callers that save an imported
    // match must be able to reject an invalid track/board binding rather than
    // make persistence look successful and merely log a failed enrollment.
    validate_discovery_source_input(pool, account_id, input)?;
    let provider = input.provider.trim().to_ascii_lowercase();
    if !matches!(
        provider.as_str(),
        "greenhouse" | "lever" | "ashby" | "smartrecruiters" | "workday"
    ) {
        anyhow::bail!("unsupported Jobs discovery provider")
    }
    let requested_source_key = input.source_key.trim();
    if requested_source_key.is_empty()
        || requested_source_key.len() > 160
        || !requested_source_key
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'~'))
    {
        anyhow::bail!("discovery source key contains unsupported characters")
    }
    if provider != "workday" && requested_source_key.contains('~') {
        anyhow::bail!("discovery source key contains unsupported characters")
    }
    let company = input.company.trim();
    if company.is_empty() || company.chars().count() > 200 {
        anyhow::bail!("discovery source company is required")
    }
    let track_id = input.track_id.trim();
    if !track_id.is_empty()
        && !list_tracks(pool, account_id)?
            .iter()
            .any(|track| track.id == track_id)
    {
        anyhow::bail!("discovery source Career Track was not found")
    }
    let interval = input
        .run_interval_ms
        .clamp(DISCOVERY_MIN_INTERVAL_MS, DISCOVERY_MAX_INTERVAL_MS);
    let (source_key, config) = match provider.as_str() {
        "greenhouse" => (
            requested_source_key.to_string(),
            json!({
                "kind": "greenhouse",
                "boardToken": requested_source_key,
                "company": company,
            }),
        ),
        "lever" => (
            requested_source_key.to_string(),
            json!({
                "kind": "lever",
                "site": requested_source_key,
                "company": company,
            }),
        ),
        "ashby" => (
            requested_source_key.to_string(),
            json!({
                "kind": "ashby",
                "boardName": requested_source_key,
                "company": company,
            }),
        ),
        "smartrecruiters" => (
            requested_source_key.to_string(),
            json!({
                "kind": "smartrecruiters",
                "companyIdentifier": requested_source_key,
                "company": company,
            }),
        ),
        "workday" => {
            let identifiers = requested_source_key.split('~').collect::<Vec<_>>();
            if identifiers.len() != 3 || identifiers.iter().any(|value| value.is_empty()) {
                anyhow::bail!("Workday source key must use tenant~instance~site")
            }
            (
                requested_source_key.to_string(),
                json!({
                    "kind": "workday",
                    "tenant": identifiers[0],
                    "instance": identifiers[1],
                    "site": identifiers[2],
                    "locale": "en-US",
                    "company": company,
                }),
            )
        }
        _ => unreachable!(),
    };
    let digest = hex::encode(Sha256::digest(format!(
        "{account_id}\0{provider}\0{source_key}\0{track_id}"
    )));
    let id = format!("source-{}", &digest[..32]);
    let now = now_ms();
    let payload = to_json(&config, "Jobs discovery source")?;

    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            enforce_discovery_source_authority_sqlite(
                &tx,
                account_id,
                &provider,
                &source_key,
                track_id,
            )?;
            tx.execute(
                "INSERT INTO jobs_discovery_sources (
                    id, account_id, track_id, provider, source_key, source_json,
                    status, health, consecutive_failures, run_interval_ms,
                    next_run_at_ms, created_at_ms, updated_at_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'active', 'waiting', 0, ?7, ?8, ?8, ?8)
                 ON CONFLICT(account_id, provider, source_key, track_id) DO UPDATE SET
                    source_json = excluded.source_json,
                    run_interval_ms = excluded.run_interval_ms,
                    status = 'active',
                    health = CASE WHEN jobs_discovery_sources.health = 'paused'
                                  THEN CASE WHEN jobs_discovery_sources.last_success_at_ms IS NULL
                                            THEN 'waiting' ELSE 'degraded' END
                                  ELSE jobs_discovery_sources.health END,
                    next_run_at_ms = MIN(jobs_discovery_sources.next_run_at_ms, excluded.next_run_at_ms),
                    updated_at_ms = excluded.updated_at_ms",
                params![id, account_id, track_id, provider, source_key, payload, interval, now],
            )?;
            let source = tx
                .query_row(
                    "SELECT id, account_id, track_id, provider, source_key, source_json,
                        status, health, consecutive_failures, run_interval_ms,
                        next_run_at_ms, last_success_at_ms, last_failure_at_ms,
                        last_error_code, lease_expires_at_ms, created_at_ms, updated_at_ms
                   FROM jobs_discovery_sources
                  WHERE account_id = ?1 AND provider = ?2 AND source_key = ?3 AND track_id = ?4",
                    params![account_id, provider, source_key, track_id],
                    discovery_source_from_sqlite_row,
                )
                .context("upsert Jobs discovery source")?;
            tx.commit()?;
            Ok(source)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            // PostgreSQL does not take a predicate lock for an empty source
            // list, so serialize enrollment decisions per account. This keeps
            // the quota and one-board/one-track contract authoritative even
            // when two imports arrive at the same time.
            lock_discovery_account_postgres(&mut tx, account_id)?;
            enforce_discovery_source_authority_postgres(
                &mut tx,
                account_id,
                &provider,
                &source_key,
                track_id,
            )?;
            let row = tx.query_one(
                "INSERT INTO jobs_discovery_sources (
                    id, account_id, track_id, provider, source_key, source_json,
                    status, health, consecutive_failures, run_interval_ms,
                    next_run_at_ms, created_at_ms, updated_at_ms
                 ) VALUES ($1, $2, $3, $4, $5, $6, 'active', 'waiting', 0, $7, $8, $8, $8)
                 ON CONFLICT(account_id, provider, source_key, track_id) DO UPDATE SET
                    source_json = EXCLUDED.source_json,
                    run_interval_ms = EXCLUDED.run_interval_ms,
                    status = 'active',
                    health = CASE WHEN jobs_discovery_sources.health = 'paused'
                                  THEN CASE WHEN jobs_discovery_sources.last_success_at_ms IS NULL
                                            THEN 'waiting' ELSE 'degraded' END
                                  ELSE jobs_discovery_sources.health END,
                    next_run_at_ms = LEAST(jobs_discovery_sources.next_run_at_ms, EXCLUDED.next_run_at_ms),
                    updated_at_ms = EXCLUDED.updated_at_ms
                 RETURNING id, account_id, track_id, provider, source_key, source_json,
                           status, health, consecutive_failures, run_interval_ms,
                           next_run_at_ms, last_success_at_ms, last_failure_at_ms,
                           last_error_code, lease_expires_at_ms, created_at_ms, updated_at_ms",
                &[&id, &account_id, &track_id, &provider, &source_key, &payload, &interval, &now],
            )?;
            let source = discovery_source_from_pg_row(row)?;
            tx.commit()?;
            Ok(source)
        }
    })
}

fn enforce_discovery_source_authority_sqlite(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    provider: &str,
    source_key: &str,
    track_id: &str,
) -> Result<()> {
    if !track_id.is_empty()
        && tx
            .query_row(
                "SELECT 1 FROM jobs_tracks WHERE account_id = ?1 AND id = ?2",
                params![account_id, track_id],
                |_| Ok(()),
            )
            .optional()?
            .is_none()
    {
        anyhow::bail!("discovery source Career Track was not found")
    }
    let bound_track: Option<String> = tx
        .query_row(
            "SELECT track_id FROM jobs_discovery_sources
              WHERE account_id = ?1 AND provider = ?2 AND source_key = ?3",
            params![account_id, provider, source_key],
            |row| row.get(0),
        )
        .optional()?;
    if bound_track
        .as_deref()
        .is_some_and(|bound| bound != track_id)
    {
        anyhow::bail!("discovery board is already bound to another Career Track")
    }
    if bound_track.is_none() {
        let account_count: i64 = tx.query_row(
            "SELECT COUNT(*) FROM jobs_discovery_sources WHERE account_id = ?1",
            params![account_id],
            |row| row.get(0),
        )?;
        if account_count as usize >= DISCOVERY_MAX_SOURCES_PER_ACCOUNT {
            anyhow::bail!("discovery source limit reached for this account")
        }
        let count: i64 = tx.query_row(
            "SELECT COUNT(*) FROM jobs_discovery_sources WHERE account_id = ?1 AND track_id = ?2",
            params![account_id, track_id],
            |row| row.get(0),
        )?;
        if count as usize >= DISCOVERY_MAX_SOURCES_PER_TRACK {
            anyhow::bail!("discovery source limit reached for this Career Track")
        }
    }
    Ok(())
}

fn enforce_discovery_source_authority_postgres(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    provider: &str,
    source_key: &str,
    track_id: &str,
) -> Result<()> {
    if !track_id.is_empty()
        && tx
            .query_opt(
                "SELECT 1 FROM jobs_tracks WHERE account_id = $1 AND id = $2",
                &[&account_id, &track_id],
            )?
            .is_none()
    {
        anyhow::bail!("discovery source Career Track was not found")
    }
    let bound_track = tx
        .query_opt(
            "SELECT track_id FROM jobs_discovery_sources
              WHERE account_id = $1 AND provider = $2 AND source_key = $3",
            &[&account_id, &provider, &source_key],
        )?
        .map(|row| row.get::<_, String>(0));
    if bound_track
        .as_deref()
        .is_some_and(|bound| bound != track_id)
    {
        anyhow::bail!("discovery board is already bound to another Career Track")
    }
    if bound_track.is_none() {
        let account_count: i64 = tx
            .query_one(
                "SELECT COUNT(*) FROM jobs_discovery_sources WHERE account_id = $1",
                &[&account_id],
            )?
            .get(0);
        if account_count as usize >= DISCOVERY_MAX_SOURCES_PER_ACCOUNT {
            anyhow::bail!("discovery source limit reached for this account")
        }
        let count: i64 = tx
            .query_one(
                "SELECT COUNT(*) FROM jobs_discovery_sources WHERE account_id = $1 AND track_id = $2",
                &[&account_id, &track_id],
            )?
            .get(0);
        if count as usize >= DISCOVERY_MAX_SOURCES_PER_TRACK {
            anyhow::bail!("discovery source limit reached for this Career Track")
        }
    }
    Ok(())
}

/// Validate an automatic public-ATS board binding before any job data is
/// persisted. A board deliberately belongs to one Career Track per account;
/// multi-track discovery will require an explicit join model rather than
/// silently overwriting a posting's track.
pub fn validate_discovery_source_input(
    pool: &DbPool,
    account_id: &str,
    input: &DiscoverySourceInput,
) -> Result<()> {
    let provider = input.provider.trim().to_ascii_lowercase();
    if !matches!(
        provider.as_str(),
        "greenhouse" | "lever" | "ashby" | "smartrecruiters" | "workday"
    ) {
        anyhow::bail!("unsupported Jobs discovery provider")
    }
    let source_key = input.source_key.trim();
    if source_key.is_empty()
        || source_key.len() > 160
        || !source_key
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'~'))
    {
        anyhow::bail!("discovery source key contains unsupported characters")
    }
    if provider != "workday" && source_key.contains('~') {
        anyhow::bail!("discovery source key contains unsupported characters")
    }
    if provider == "workday" {
        let identifiers = source_key.split('~').collect::<Vec<_>>();
        if identifiers.len() != 3 || identifiers.iter().any(|value| value.is_empty()) {
            anyhow::bail!("Workday source key must use tenant~instance~site")
        }
    }
    let company = input.company.trim();
    if company.is_empty() || company.chars().count() > 200 {
        anyhow::bail!("discovery source company is required")
    }
    let track_id = input.track_id.trim();
    if !track_id.is_empty()
        && !list_tracks(pool, account_id)?
            .iter()
            .any(|track| track.id == track_id)
    {
        anyhow::bail!("discovery source Career Track was not found")
    }

    let sources = list_discovery_sources(pool, account_id)?;
    if sources.iter().any(|source| {
        source.provider == provider
            && source.source_key == source_key
            && source.track_id != track_id
    }) {
        anyhow::bail!("discovery board is already bound to another Career Track")
    }
    let exists = sources.iter().any(|source| {
        source.provider == provider
            && source.source_key == source_key
            && source.track_id == track_id
    });
    if !exists {
        if sources.len() >= DISCOVERY_MAX_SOURCES_PER_ACCOUNT {
            anyhow::bail!("discovery source limit reached for this account")
        }
        if sources
            .iter()
            .filter(|source| source.track_id == track_id)
            .count()
            >= DISCOVERY_MAX_SOURCES_PER_TRACK
        {
            anyhow::bail!("discovery source limit reached for this Career Track")
        }
    }
    Ok(())
}

fn normalized_discovery_source_values(
    account_id: &str,
    input: &DiscoverySourceInput,
) -> Result<(String, String, String, i64, String, String)> {
    let provider = input.provider.trim().to_ascii_lowercase();
    let source_key = input.source_key.trim().to_string();
    let track_id = input.track_id.trim().to_string();
    let company = input.company.trim();
    let interval = input
        .run_interval_ms
        .clamp(DISCOVERY_MIN_INTERVAL_MS, DISCOVERY_MAX_INTERVAL_MS);
    let config = match provider.as_str() {
        "greenhouse" => {
            json!({ "kind": "greenhouse", "boardToken": source_key, "company": company })
        }
        "lever" => json!({ "kind": "lever", "site": source_key, "company": company }),
        "ashby" => json!({ "kind": "ashby", "boardName": source_key, "company": company }),
        "smartrecruiters" => {
            json!({ "kind": "smartrecruiters", "companyIdentifier": source_key, "company": company })
        }
        "workday" => {
            let identifiers = source_key.split('~').collect::<Vec<_>>();
            if identifiers.len() != 3 || identifiers.iter().any(|value| value.is_empty()) {
                anyhow::bail!("Workday source key must use tenant~instance~site")
            }
            json!({
                "kind": "workday", "tenant": identifiers[0], "instance": identifiers[1],
                "site": identifiers[2], "locale": "en-US", "company": company,
            })
        }
        _ => anyhow::bail!("unsupported Jobs discovery provider"),
    };
    let digest = hex::encode(Sha256::digest(format!(
        "{account_id}\0{provider}\0{source_key}\0{track_id}"
    )));
    Ok((
        provider,
        source_key,
        track_id,
        interval,
        to_json(&config, "Jobs discovery source")?,
        format!("source-{}", &digest[..32]),
    ))
}

fn verified_import_membership_content_hash(
    provider: &str,
    source_key: &str,
    posting: &JobPosting,
) -> String {
    let payload = json!({
        "provider": provider,
        "source_key": source_key,
        "external_id": posting.external_id.trim(),
        "canonical_url": posting.canonical_url.trim(),
        "company": posting.company.trim(),
        "title": posting.title.trim(),
        "location": posting.location.trim(),
        "workplace": posting.workplace.trim(),
        "description": posting.description.trim(),
        "compensation": posting.compensation.trim(),
        "posted_at_ms": posting.posted_at_ms,
    });
    hex::encode(Sha256::digest(
        serde_json::to_vec(&payload).unwrap_or_default(),
    ))
}

fn verified_import_membership_run_id(
    account_id: &str,
    source_id: &str,
    external_id: &str,
) -> String {
    let digest = hex::encode(Sha256::digest(format!(
        "{account_id}\0{source_id}\0{external_id}"
    )));
    format!("verified-import-{}", &digest[..32])
}

fn resolve_verified_import_identity_sqlite(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    source_id: &str,
    external_id: &str,
    canonical_key: &str,
) -> Result<Option<JobPosting>> {
    let membership_job_id: Option<String> = tx
        .query_row(
            "SELECT job_id FROM jobs_discovery_memberships
              WHERE source_id = ?1 AND external_id = ?2",
            params![source_id, external_id],
            |row| row.get(0),
        )
        .optional()?;
    let membership_posting = match membership_job_id {
        Some(job_id) => tx
            .query_row(
                "SELECT posting_json FROM jobs_postings
                  WHERE account_id = ?1 AND id = ?2",
                params![account_id, job_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .map(|raw| parse_json::<JobPosting>(raw, "job posting"))
            .transpose()?
            .ok_or_else(|| anyhow::anyhow!("discovery membership refers to a missing job"))
            .map(Some)?,
        None => None,
    };
    let canonical_posting = tx
        .query_row(
            "SELECT posting_json FROM jobs_postings
              WHERE account_id = ?1 AND canonical_key = ?2",
            params![account_id, canonical_key],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        .map(|raw| parse_json::<JobPosting>(raw, "job posting"))
        .transpose()?;
    if membership_posting
        .as_ref()
        .zip(canonical_posting.as_ref())
        .is_some_and(|(membership, canonical)| membership.id != canonical.id)
    {
        anyhow::bail!("discovery membership and canonical job disagree")
    }
    Ok(membership_posting.or(canonical_posting))
}

fn resolve_verified_import_identity_postgres(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    source_id: &str,
    external_id: &str,
    canonical_key: &str,
) -> Result<Option<JobPosting>> {
    let membership_job_id = tx
        .query_opt(
            "SELECT job_id FROM jobs_discovery_memberships
              WHERE source_id = $1 AND external_id = $2 FOR UPDATE",
            &[&source_id, &external_id],
        )?
        .map(|row| row.get::<_, String>(0));
    let membership_posting = match membership_job_id {
        Some(job_id) => tx
            .query_opt(
                "SELECT posting_json FROM jobs_postings
                  WHERE account_id = $1 AND id = $2 FOR UPDATE",
                &[&account_id, &job_id],
            )?
            .map(|row| parse_json::<JobPosting>(row.get(0), "job posting"))
            .transpose()?
            .ok_or_else(|| anyhow::anyhow!("discovery membership refers to a missing job"))
            .map(Some)?,
        None => None,
    };
    let canonical_posting = tx
        .query_opt(
            "SELECT posting_json FROM jobs_postings
              WHERE account_id = $1 AND canonical_key = $2 FOR UPDATE",
            &[&account_id, &canonical_key],
        )?
        .map(|row| parse_json::<JobPosting>(row.get(0), "job posting"))
        .transpose()?;
    if membership_posting
        .as_ref()
        .zip(canonical_posting.as_ref())
        .is_some_and(|(membership, canonical)| membership.id != canonical.id)
    {
        anyhow::bail!("discovery membership and canonical job disagree")
    }
    Ok(membership_posting.or(canonical_posting))
}

/// Atomically accept a verified public import and its required board binding.
/// Manual and unsupported links intentionally continue through `upsert_posting`
/// alone. For a verified import, neither a source nor a match becomes visible
/// unless both writes succeed under the same account/source authority lock.
pub fn save_verified_import_posting_with_source(
    pool: &DbPool,
    account_id: &str,
    posting: &JobPosting,
    source: &DiscoverySourceInput,
    profile: &CareerProfile,
    preferences: &JobPreferences,
) -> Result<JobPosting> {
    validate_discovery_source_input(pool, account_id, source)?;
    let (provider, source_key, track_id, interval, source_payload, source_id) =
        normalized_discovery_source_values(account_id, source)?;
    if posting.track_id.trim() != track_id {
        anyhow::bail!("verified import discovery source does not match the job Career Track")
    }
    let external_id = posting.external_id.trim();
    if external_id.is_empty() || external_id.chars().count() > 240 {
        anyhow::bail!("verified import has no valid external job ID")
    }
    let (_, posting_source_key) =
        canonical_public_discovery_url(&provider, &posting.canonical_url)?;
    if posting_source_key != source_key {
        anyhow::bail!("verified import URL does not belong to its discovery source")
    }
    let applications = list_applications(pool, account_id)?;
    let reservations = list_attempt_reservations(pool, account_id)?;
    let tracks = list_tracks(pool, account_id)?;
    let track = tracks.iter().find(|track| track.id == track_id);
    let now = now_ms();
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            enforce_discovery_source_authority_sqlite(
                &tx,
                account_id,
                &provider,
                &source_key,
                &track_id,
            )?;
            tx.execute(
                "INSERT INTO jobs_discovery_sources (
                    id, account_id, track_id, provider, source_key, source_json,
                    status, health, consecutive_failures, run_interval_ms,
                    next_run_at_ms, created_at_ms, updated_at_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'active', 'waiting', 0, ?7, ?8, ?8, ?8)
                 ON CONFLICT(account_id, provider, source_key, track_id) DO UPDATE SET
                    source_json = excluded.source_json, run_interval_ms = excluded.run_interval_ms,
                    status = 'active',
                    health = CASE WHEN jobs_discovery_sources.health = 'paused'
                                  THEN CASE WHEN jobs_discovery_sources.last_success_at_ms IS NULL
                                            THEN 'waiting' ELSE 'degraded' END
                                  ELSE jobs_discovery_sources.health END,
                    next_run_at_ms = MIN(jobs_discovery_sources.next_run_at_ms, excluded.next_run_at_ms),
                    updated_at_ms = excluded.updated_at_ms",
                params![source_id, account_id, track_id, provider, source_key, source_payload, interval, now],
            )?;
            let actual_source_id: String = tx.query_row(
                "SELECT id FROM jobs_discovery_sources
                  WHERE account_id = ?1 AND provider = ?2 AND source_key = ?3",
                params![account_id, provider, source_key],
                |row| row.get(0),
            )?;
            let existing = resolve_verified_import_identity_sqlite(
                &tx,
                account_id,
                &actual_source_id,
                external_id,
                &canonical_job_key(posting),
            )?;
            let saved = prepare_snapshot_posting(
                posting,
                existing.clone(),
                &PostingSnapshotContext {
                    profile,
                    preferences,
                    applications: &applications,
                    reservations: &reservations,
                    track,
                    observed_at_ms: now,
                },
            )?;
            let payload = to_json(&saved, "job posting")?;
            if existing.is_some() {
                let updated = tx.execute(
                    "UPDATE jobs_postings SET canonical_key = ?3, posting_json = ?4,
                        source = ?5, canonical_url = ?6, company = ?7, title = ?8,
                        location = ?9, match_score = ?10, status = ?11, updated_at_ms = ?12
                      WHERE account_id = ?1 AND id = ?2",
                    params![
                        account_id,
                        saved.id,
                        saved.canonical_key,
                        payload,
                        saved.source,
                        saved.canonical_url,
                        saved.company,
                        saved.title,
                        saved.location,
                        saved.match_score,
                        saved.status,
                        saved.updated_at_ms,
                    ],
                )?;
                if updated != 1 {
                    anyhow::bail!("verified import job identity is stale")
                }
            } else {
                tx.execute(
                    "INSERT INTO jobs_postings (
                        id, account_id, canonical_key, posting_json, source, canonical_url,
                        company, title, location, match_score, status, created_at_ms, updated_at_ms
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                    params![
                        saved.id,
                        account_id,
                        saved.canonical_key,
                        payload,
                        saved.source,
                        saved.canonical_url,
                        saved.company,
                        saved.title,
                        saved.location,
                        saved.match_score,
                        saved.status,
                        saved.created_at_ms,
                        saved.updated_at_ms,
                    ],
                )?;
            }
            let content_hash =
                verified_import_membership_content_hash(&provider, &source_key, &saved);
            let import_run_id =
                verified_import_membership_run_id(account_id, &actual_source_id, external_id);
            tx.execute(
                "INSERT INTO jobs_discovery_memberships (
                    source_id, account_id, canonical_key, external_id, job_id,
                    content_hash, first_seen_at_ms, last_seen_at_ms,
                    last_seen_run_id, availability_status
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7, ?8, 'pending')
                 ON CONFLICT(source_id, external_id) DO UPDATE SET
                    canonical_key = excluded.canonical_key, job_id = excluded.job_id",
                params![
                    actual_source_id,
                    account_id,
                    saved.canonical_key,
                    external_id,
                    saved.id,
                    content_hash,
                    now,
                    import_run_id,
                ],
            )?;
            tx.commit()?;
            Ok(saved)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            lock_discovery_account_postgres(&mut tx, account_id)?;
            enforce_discovery_source_authority_postgres(
                &mut tx,
                account_id,
                &provider,
                &source_key,
                &track_id,
            )?;
            tx.execute(
                "INSERT INTO jobs_discovery_sources (
                    id, account_id, track_id, provider, source_key, source_json,
                    status, health, consecutive_failures, run_interval_ms,
                    next_run_at_ms, created_at_ms, updated_at_ms
                 ) VALUES ($1, $2, $3, $4, $5, $6, 'active', 'waiting', 0, $7, $8, $8, $8)
                 ON CONFLICT(account_id, provider, source_key, track_id) DO UPDATE SET
                    source_json = EXCLUDED.source_json, run_interval_ms = EXCLUDED.run_interval_ms,
                    status = 'active',
                    health = CASE WHEN jobs_discovery_sources.health = 'paused'
                                  THEN CASE WHEN jobs_discovery_sources.last_success_at_ms IS NULL
                                            THEN 'waiting' ELSE 'degraded' END
                                  ELSE jobs_discovery_sources.health END,
                    next_run_at_ms = LEAST(jobs_discovery_sources.next_run_at_ms, EXCLUDED.next_run_at_ms),
                    updated_at_ms = EXCLUDED.updated_at_ms",
                &[&source_id, &account_id, &track_id, &provider, &source_key, &source_payload, &interval, &now],
            )?;
            let actual_source_id: String = tx
                .query_one(
                    "SELECT id FROM jobs_discovery_sources
                      WHERE account_id = $1 AND provider = $2 AND source_key = $3
                      FOR UPDATE",
                    &[&account_id, &provider, &source_key],
                )?
                .get(0);
            let existing = resolve_verified_import_identity_postgres(
                &mut tx,
                account_id,
                &actual_source_id,
                external_id,
                &canonical_job_key(posting),
            )?;
            let saved = prepare_snapshot_posting(
                posting,
                existing.clone(),
                &PostingSnapshotContext {
                    profile,
                    preferences,
                    applications: &applications,
                    reservations: &reservations,
                    track,
                    observed_at_ms: now,
                },
            )?;
            let payload = to_json(&saved, "job posting")?;
            if existing.is_some() {
                let updated = tx.execute(
                    "UPDATE jobs_postings SET canonical_key = $3, posting_json = $4,
                        source = $5, canonical_url = $6, company = $7, title = $8,
                        location = $9, match_score = $10, status = $11, updated_at_ms = $12
                      WHERE account_id = $1 AND id = $2",
                    &[
                        &account_id,
                        &saved.id,
                        &saved.canonical_key,
                        &payload,
                        &saved.source,
                        &saved.canonical_url,
                        &saved.company,
                        &saved.title,
                        &saved.location,
                        &saved.match_score,
                        &saved.status,
                        &saved.updated_at_ms,
                    ],
                )?;
                if updated != 1 {
                    anyhow::bail!("verified import job identity is stale")
                }
            } else {
                tx.execute(
                    "INSERT INTO jobs_postings (
                        id, account_id, canonical_key, posting_json, source, canonical_url,
                        company, title, location, match_score, status, created_at_ms, updated_at_ms
                     ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)",
                    &[
                        &saved.id,
                        &account_id,
                        &saved.canonical_key,
                        &payload,
                        &saved.source,
                        &saved.canonical_url,
                        &saved.company,
                        &saved.title,
                        &saved.location,
                        &saved.match_score,
                        &saved.status,
                        &saved.created_at_ms,
                        &saved.updated_at_ms,
                    ],
                )?;
            }
            let content_hash =
                verified_import_membership_content_hash(&provider, &source_key, &saved);
            let import_run_id =
                verified_import_membership_run_id(account_id, &actual_source_id, external_id);
            tx.execute(
                "INSERT INTO jobs_discovery_memberships (
                    source_id, account_id, canonical_key, external_id, job_id,
                    content_hash, first_seen_at_ms, last_seen_at_ms,
                    last_seen_run_id, availability_status
                 ) VALUES ($1, $2, $3, $4, $5, $6, $7, $7, $8, 'pending')
                 ON CONFLICT(source_id, external_id) DO UPDATE SET
                    canonical_key = EXCLUDED.canonical_key, job_id = EXCLUDED.job_id",
                &[
                    &actual_source_id,
                    &account_id,
                    &saved.canonical_key,
                    &external_id,
                    &saved.id,
                    &content_hash,
                    &now,
                    &import_run_id,
                ],
            )?;
            tx.commit()?;
            Ok(saved)
        }
    })
}

pub fn set_discovery_source_status(
    pool: &DbPool,
    account_id: &str,
    source_id: &str,
    status: &str,
) -> Result<Option<DiscoverySource>> {
    if !matches!(status, "active" | "paused") {
        anyhow::bail!("invalid discovery source status")
    }
    let now = now_ms();
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let changed = tx.execute(
                "UPDATE jobs_discovery_sources
                    SET status = ?3,
                        health = CASE WHEN ?3 = 'paused' THEN 'paused'
                                      WHEN last_success_at_ms IS NULL THEN 'waiting'
                                      ELSE 'degraded' END,
                        next_run_at_ms = CASE WHEN ?3 = 'active' THEN ?4 ELSE next_run_at_ms END,
                        lease_owner = NULL, lease_token = NULL, lease_expires_at_ms = NULL,
                        updated_at_ms = ?4
                  WHERE account_id = ?1 AND id = ?2",
                params![account_id, source_id, status, now],
            )?;
            if changed == 0 {
                tx.commit()?;
                return Ok(None);
            }
            let source = tx
                .query_row(
                    "SELECT id, account_id, track_id, provider, source_key, source_json,
                        status, health, consecutive_failures, run_interval_ms,
                        next_run_at_ms, last_success_at_ms, last_failure_at_ms,
                        last_error_code, lease_expires_at_ms, created_at_ms, updated_at_ms
                   FROM jobs_discovery_sources WHERE account_id = ?1 AND id = ?2",
                    params![account_id, source_id],
                    discovery_source_from_sqlite_row,
                )
                .optional()
                .context("update Jobs discovery source")?;
            tx.commit()?;
            Ok(source)
        }
        DbPool::Postgres(_) => pool
            .get_pg()?
            .query_opt(
                "UPDATE jobs_discovery_sources
                    SET status = $3,
                        health = CASE WHEN $3 = 'paused' THEN 'paused'
                                      WHEN last_success_at_ms IS NULL THEN 'waiting'
                                      ELSE 'degraded' END,
                        next_run_at_ms = CASE WHEN $3 = 'active' THEN $4 ELSE next_run_at_ms END,
                        lease_owner = NULL, lease_token = NULL, lease_expires_at_ms = NULL,
                        updated_at_ms = $4
                  WHERE account_id = $1 AND id = $2
              RETURNING id, account_id, track_id, provider, source_key, source_json,
                        status, health, consecutive_failures, run_interval_ms,
                        next_run_at_ms, last_success_at_ms, last_failure_at_ms,
                        last_error_code, lease_expires_at_ms, created_at_ms, updated_at_ms",
                &[&account_id, &source_id, &status, &now],
            )?
            .map(discovery_source_from_pg_row)
            .transpose(),
    })
}

fn discovery_source_from_sqlite_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<DiscoverySource> {
    let config: String = row.get(5)?;
    Ok(DiscoverySource {
        id: row.get(0)?,
        account_id: row.get(1)?,
        track_id: row.get(2)?,
        provider: row.get(3)?,
        source_key: row.get(4)?,
        config: parse_json_lossy(&config).unwrap_or_else(|| json!({})),
        status: row.get(6)?,
        health: row.get(7)?,
        consecutive_failures: row.get(8)?,
        run_interval_ms: row.get(9)?,
        next_run_at_ms: row.get(10)?,
        last_success_at_ms: row.get(11)?,
        last_failure_at_ms: row.get(12)?,
        last_error_code: row.get(13)?,
        lease_expires_at_ms: row.get(14)?,
        created_at_ms: row.get(15)?,
        updated_at_ms: row.get(16)?,
    })
}

fn discovery_source_from_pg_row(row: postgres::Row) -> Result<DiscoverySource> {
    let config: String = row.get(5);
    Ok(DiscoverySource {
        id: row.get(0),
        account_id: row.get(1),
        track_id: row.get(2),
        provider: row.get(3),
        source_key: row.get(4),
        config: parse_json(config, "Jobs discovery source")?,
        status: row.get(6),
        health: row.get(7),
        consecutive_failures: row.get(8),
        run_interval_ms: row.get(9),
        next_run_at_ms: row.get(10),
        last_success_at_ms: row.get(11),
        last_failure_at_ms: row.get(12),
        last_error_code: row.get(13),
        lease_expires_at_ms: row.get(14),
        created_at_ms: row.get(15),
        updated_at_ms: row.get(16),
    })
}

pub fn lease_due_discovery_source(
    pool: &DbPool,
    worker_id: &str,
) -> Result<Option<DiscoverySourceLease>> {
    let worker_id = worker_id.trim();
    if worker_id.len() < 3
        || worker_id.len() > 160
        || !worker_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b':' | b'_' | b'-' | b'.'))
    {
        anyhow::bail!("invalid discovery worker ID")
    }
    let mut token_bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut token_bytes);
    let lease_token = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(token_bytes);
    let lease_token_hash = discovery_lease_token_hash(&lease_token);
    let now = now_ms();
    let lease_expires = now + DISCOVERY_LEASE_MS;

    recover_legacy_stale_discovery_commits(pool, now)?;

    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
            let source = tx
                .query_row(
                    "SELECT id, account_id, track_id, provider, source_key, source_json,
                            status, health, consecutive_failures, run_interval_ms,
                            next_run_at_ms, last_success_at_ms, last_failure_at_ms,
                            last_error_code, lease_expires_at_ms, created_at_ms, updated_at_ms
                       FROM jobs_discovery_sources
                      WHERE status = 'active' AND health <> 'paused'
                        AND next_run_at_ms <= ?1
                        AND (lease_expires_at_ms IS NULL OR lease_expires_at_ms <= ?1)
                        AND NOT EXISTS (
                            SELECT 1 FROM jobs_discovery_runs r
                             WHERE r.source_id = jobs_discovery_sources.id
                               AND r.status = 'committing'
                        )
                      ORDER BY next_run_at_ms ASC, id ASC LIMIT 1",
                    params![now],
                    discovery_source_from_sqlite_row,
                )
                .optional()?;
            let Some(source) = source else {
                tx.commit()?;
                return Ok(None);
            };
            let scheduled_for_ms = source.next_run_at_ms;
            let replay_key = discovery_replay_key(&source.id, scheduled_for_ms);
            let run_id = discovery_run_id(&source.id, &replay_key);
            tx.execute(
                "UPDATE jobs_discovery_sources
                    SET lease_owner = ?2, lease_token = ?3, lease_expires_at_ms = ?4,
                        updated_at_ms = ?1
                  WHERE id = ?5",
                params![now, worker_id, lease_token_hash, lease_expires, source.id],
            )?;
            tx.execute(
                "INSERT INTO jobs_discovery_runs (
                    id, account_id, source_id, replay_key, status, started_at_ms
                 ) VALUES (?1, ?2, ?3, ?4, 'running', ?5)
                 ON CONFLICT(source_id, replay_key) DO NOTHING",
                params![run_id, source.account_id, source.id, replay_key, now],
            )?;
            tx.commit()?;
            Ok(Some(DiscoverySourceLease {
                source: DiscoverySource {
                    lease_expires_at_ms: Some(lease_expires),
                    updated_at_ms: now,
                    ..source
                },
                lease_token,
                replay_key,
                scheduled_for_ms,
            }))
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            let row = tx.query_opt(
                "SELECT id, account_id, track_id, provider, source_key, source_json,
                        status, health, consecutive_failures, run_interval_ms,
                        next_run_at_ms, last_success_at_ms, last_failure_at_ms,
                        last_error_code, lease_expires_at_ms, created_at_ms, updated_at_ms
                   FROM jobs_discovery_sources
                  WHERE status = 'active' AND health <> 'paused'
                    AND next_run_at_ms <= $1
                    AND (lease_expires_at_ms IS NULL OR lease_expires_at_ms <= $1)
                    AND NOT EXISTS (
                        SELECT 1 FROM jobs_discovery_runs r
                         WHERE r.source_id = jobs_discovery_sources.id
                           AND r.status = 'committing'
                    )
                  ORDER BY next_run_at_ms ASC, id ASC
                  FOR UPDATE SKIP LOCKED LIMIT 1",
                &[&now],
            )?;
            let Some(row) = row else {
                tx.commit()?;
                return Ok(None);
            };
            let source = discovery_source_from_pg_row(row)?;
            let scheduled_for_ms = source.next_run_at_ms;
            let replay_key = discovery_replay_key(&source.id, scheduled_for_ms);
            let run_id = discovery_run_id(&source.id, &replay_key);
            tx.execute(
                "UPDATE jobs_discovery_sources
                    SET lease_owner = $2, lease_token = $3, lease_expires_at_ms = $4,
                        updated_at_ms = $1
                  WHERE id = $5",
                &[
                    &now,
                    &worker_id,
                    &lease_token_hash,
                    &lease_expires,
                    &source.id,
                ],
            )?;
            tx.execute(
                "INSERT INTO jobs_discovery_runs (
                    id, account_id, source_id, replay_key, status, started_at_ms
                 ) VALUES ($1, $2, $3, $4, 'running', $5)
                 ON CONFLICT(source_id, replay_key) DO NOTHING",
                &[&run_id, &source.account_id, &source.id, &replay_key, &now],
            )?;
            tx.commit()?;
            Ok(Some(DiscoverySourceLease {
                source: DiscoverySource {
                    lease_expires_at_ms: Some(lease_expires),
                    updated_at_ms: now,
                    ..source
                },
                lease_token,
                replay_key,
                scheduled_for_ms,
            }))
        }
    })
}

fn discovery_lease_token_hash(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

fn discovery_replay_key(source_id: &str, scheduled_for_ms: i64) -> String {
    let digest = hex::encode(Sha256::digest(format!("{source_id}\0{scheduled_for_ms}")));
    format!("discovery-{}", &digest[..40])
}

fn discovery_run_id(source_id: &str, replay_key: &str) -> String {
    let digest = hex::encode(Sha256::digest(format!("{source_id}\0{replay_key}")));
    format!("discovery-run-{}", &digest[..32])
}

/// Compatibility-only cleanup for rows written by the pre-atomic publisher.
/// New publication never enters `committing`; this releases an old stranded
/// row after its lease expires so it cannot block a source forever.
fn recover_legacy_stale_discovery_commits(pool: &DbPool, now: i64) -> Result<()> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let recovered = tx.execute(
                "UPDATE jobs_discovery_runs
                    SET status = 'failed', error_code = 'commit_timeout', completed_at_ms = ?1
                  WHERE status = 'committing'
                    AND EXISTS (
                        SELECT 1 FROM jobs_discovery_sources s
                         WHERE s.id = jobs_discovery_runs.source_id
                           AND (s.lease_expires_at_ms IS NULL OR s.lease_expires_at_ms <= ?1)
                    )",
                params![now],
            )?;
            if recovered > 0 {
                tx.execute(
                    "UPDATE jobs_discovery_sources
                        SET health = CASE WHEN consecutive_failures + 1 >= 3 THEN 'paused' ELSE 'degraded' END,
                            consecutive_failures = consecutive_failures + 1,
                            last_failure_at_ms = ?1, last_error_code = 'commit_timeout',
                            next_run_at_ms = ?1, lease_owner = NULL, lease_token = NULL,
                            lease_expires_at_ms = NULL, updated_at_ms = ?1
                      WHERE EXISTS (
                          SELECT 1 FROM jobs_discovery_runs r
                           WHERE r.source_id = jobs_discovery_sources.id
                             AND r.status = 'failed' AND r.error_code = 'commit_timeout'
                             AND r.completed_at_ms = ?1
                      )",
                    params![now],
                )?;
            }
            tx.commit()?;
            Ok(())
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            let recovered = tx.execute(
                "UPDATE jobs_discovery_runs r
                    SET status = 'failed', error_code = 'commit_timeout', completed_at_ms = $1
                   FROM jobs_discovery_sources s
                  WHERE r.source_id = s.id AND r.status = 'committing'
                    AND (s.lease_expires_at_ms IS NULL OR s.lease_expires_at_ms <= $1)",
                &[&now],
            )?;
            if recovered > 0 {
                tx.execute(
                    "UPDATE jobs_discovery_sources s
                        SET health = CASE WHEN s.consecutive_failures + 1 >= 3 THEN 'paused' ELSE 'degraded' END,
                            consecutive_failures = s.consecutive_failures + 1,
                            last_failure_at_ms = $1, last_error_code = 'commit_timeout',
                            next_run_at_ms = $1, lease_owner = NULL, lease_token = NULL,
                            lease_expires_at_ms = NULL, updated_at_ms = $1
                      WHERE EXISTS (
                          SELECT 1 FROM jobs_discovery_runs r
                           WHERE r.source_id = s.id AND r.status = 'failed'
                             AND r.error_code = 'commit_timeout' AND r.completed_at_ms = $1
                      )",
                    &[&now],
                )?;
            }
            tx.commit()?;
            Ok(())
        }
    })
}

#[allow(clippy::too_many_arguments)]
fn publish_discovery_snapshot(
    pool: &DbPool,
    source: &DiscoverySource,
    lease_token: &str,
    replay_key: &str,
    scheduled_for_ms: i64,
    normalized: &BTreeMap<String, (JobPosting, String)>,
    snapshot_hash: &str,
    profile: &CareerProfile,
    preferences: &JobPreferences,
    applications: &[JobApplication],
    reservations: &[AttemptReservation],
    track: Option<&CareerTrack>,
    observed_at_ms: i64,
) -> Result<DiscoveryRunResult> {
    let token_hash = discovery_lease_token_hash(lease_token);
    let run_id = discovery_run_id(&source.id, replay_key);
    let next_run_at = discovery_next_run_at(source, observed_at_ms, 0);
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let tx_now = now_ms();
            let fresh = tx
                .query_row(
                    "SELECT provider, source_key, track_id, source_json, status, updated_at_ms,
                            next_run_at_ms, lease_expires_at_ms, lease_token
                       FROM jobs_discovery_sources WHERE id = ?1 AND account_id = ?2",
                    params![source.id, source.account_id],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, String>(3)?,
                            row.get::<_, String>(4)?,
                            row.get::<_, i64>(5)?,
                            row.get::<_, i64>(6)?,
                            row.get::<_, Option<i64>>(7)?,
                            row.get::<_, Option<String>>(8)?,
                        ))
                    },
                )
                .optional()?;
            let Some((
                provider,
                source_key,
                track_id,
                raw_config,
                status,
                updated_at_ms,
                next_run,
                lease_expires,
                stored_token,
            )) = fresh
            else {
                anyhow::bail!("discovery lease is stale")
            };
            if provider != source.provider
                || source_key != source.source_key
                || track_id != source.track_id
                || parse_json::<Value>(raw_config, "Jobs discovery source")? != source.config
                || status != "active"
                || updated_at_ms != source.updated_at_ms
                || next_run != scheduled_for_ms
                || lease_expires.is_none_or(|expiry| expiry <= tx_now)
                || stored_token.as_deref() != Some(token_hash.as_str())
            {
                anyhow::bail!("discovery lease is stale")
            }
            let run = tx
                .query_row(
                    "SELECT id, status, discovered_count, upserted_count, closed_count, snapshot_hash
                       FROM jobs_discovery_runs WHERE source_id = ?1 AND replay_key = ?2",
                    params![source.id, replay_key],
                    |row| Ok((
                        row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, i64>(2)?,
                        row.get::<_, i64>(3)?, row.get::<_, i64>(4)?, row.get::<_, Option<String>>(5)?,
                    )),
                )
                .optional()?
                .ok_or_else(|| anyhow::anyhow!("discovery lease is stale"))?;
            if matches!(run.1.as_str(), "completed" | "failed") {
                if run.1 == "completed" && run.5.as_deref() != Some(snapshot_hash) {
                    anyhow::bail!("discovery replay payload does not match the completed snapshot")
                }
                return Ok(DiscoveryRunResult {
                    run_id: run.0,
                    replay_key: replay_key.to_string(),
                    status: run.1,
                    discovered_count: run.2,
                    upserted_count: run.3,
                    closed_count: run.4,
                    replayed: true,
                });
            }
            if run.1 != "running" || run.5.as_deref().is_some_and(|hash| hash != snapshot_hash) {
                anyhow::bail!("discovery lease is stale")
            }
            tx.execute(
                "UPDATE jobs_discovery_runs SET snapshot_hash = ?3
                  WHERE source_id = ?1 AND replay_key = ?2 AND status = 'running'",
                params![source.id, replay_key, snapshot_hash],
            )?;

            let mut seen = Vec::with_capacity(normalized.len());
            for (external_id, (posting, content_hash)) in normalized {
                let membership_job_id: Option<String> = tx
                    .query_row(
                        "SELECT job_id FROM jobs_discovery_memberships
                          WHERE source_id = ?1 AND external_id = ?2",
                        params![source.id, external_id],
                        |row| row.get(0),
                    )
                    .optional()?;
                let membership_posting = match membership_job_id {
                    Some(job_id) => tx
                        .query_row(
                            "SELECT posting_json FROM jobs_postings
                              WHERE account_id = ?1 AND id = ?2",
                            params![source.account_id, job_id],
                            |row| row.get::<_, String>(0),
                        )
                        .optional()?
                        .map(|raw| parse_json::<JobPosting>(raw, "job posting"))
                        .transpose()?
                        .ok_or_else(|| {
                            anyhow::anyhow!("discovery membership refers to a missing job")
                        })
                        .map(Some)?,
                    None => None,
                };
                let canonical_posting = tx
                    .query_row(
                        "SELECT posting_json FROM jobs_postings
                          WHERE account_id = ?1 AND canonical_key = ?2",
                        params![source.account_id, posting.canonical_key],
                        |row| row.get::<_, String>(0),
                    )
                    .optional()?
                    .map(|raw| parse_json::<JobPosting>(raw, "job posting"))
                    .transpose()?;
                if membership_posting
                    .as_ref()
                    .zip(canonical_posting.as_ref())
                    .is_some_and(|(membership, canonical)| membership.id != canonical.id)
                {
                    anyhow::bail!("discovery membership and canonical job disagree")
                }
                let existing = membership_posting.or(canonical_posting);
                let saved = prepare_snapshot_posting(
                    posting,
                    existing.as_ref().cloned(),
                    &PostingSnapshotContext {
                        profile,
                        preferences,
                        applications,
                        reservations,
                        track,
                        observed_at_ms,
                    },
                )?;
                let payload = to_json(&saved, "job posting")?;
                if existing.is_some() {
                    tx.execute(
                        "UPDATE jobs_postings SET canonical_key = ?3, posting_json = ?4,
                            source = ?5, canonical_url = ?6, company = ?7, title = ?8,
                            location = ?9, match_score = ?10, status = ?11, updated_at_ms = ?12
                          WHERE account_id = ?1 AND id = ?2",
                        params![
                            source.account_id,
                            saved.id,
                            saved.canonical_key,
                            payload,
                            saved.source,
                            saved.canonical_url,
                            saved.company,
                            saved.title,
                            saved.location,
                            saved.match_score,
                            saved.status,
                            saved.updated_at_ms,
                        ],
                    )?;
                } else {
                    tx.execute(
                        "INSERT INTO jobs_postings (
                            id, account_id, canonical_key, posting_json, source, canonical_url,
                            company, title, location, match_score, status, created_at_ms, updated_at_ms
                         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                        params![
                            saved.id, source.account_id, saved.canonical_key, payload,
                            saved.source, saved.canonical_url, saved.company, saved.title,
                            saved.location, saved.match_score, saved.status, saved.created_at_ms,
                            saved.updated_at_ms,
                        ],
                    )?;
                }
                tx.execute(
                    "INSERT INTO jobs_discovery_memberships (
                        source_id, account_id, canonical_key, external_id, job_id,
                        content_hash, first_seen_at_ms, last_seen_at_ms,
                        last_seen_run_id, availability_status
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7, ?8, 'active')
                     ON CONFLICT(source_id, external_id) DO UPDATE SET
                        canonical_key = excluded.canonical_key, job_id = excluded.job_id,
                        content_hash = excluded.content_hash, last_seen_at_ms = excluded.last_seen_at_ms,
                        last_seen_run_id = excluded.last_seen_run_id, availability_status = 'active',
                        missing_count = 0, missing_since_at_ms = NULL",
                    params![
                        source.id, source.account_id, saved.canonical_key, external_id, saved.id,
                        content_hash, observed_at_ms, run_id,
                    ],
                )?;
                seen.push(external_id.clone());
            }
            let closed_count = close_missing_snapshot_memberships_sqlite(
                &tx,
                source,
                &run_id,
                observed_at_ms,
                &seen,
                profile,
                preferences,
                applications,
                reservations,
                track,
            )?;
            let source_changed = tx.execute(
                "UPDATE jobs_discovery_sources
                    SET health = 'healthy', consecutive_failures = 0,
                        last_success_at_ms = ?4, last_error_code = NULL,
                        next_run_at_ms = ?5, lease_owner = NULL, lease_token = NULL,
                        lease_expires_at_ms = NULL, updated_at_ms = ?4
                  WHERE id = ?1 AND account_id = ?2 AND lease_token = ?3",
                params![
                    source.id,
                    source.account_id,
                    token_hash,
                    observed_at_ms,
                    next_run_at
                ],
            )?;
            if source_changed != 1 {
                anyhow::bail!("discovery lease is stale")
            }
            let completed = tx.execute(
                "UPDATE jobs_discovery_runs
                    SET status = 'completed', discovered_count = ?3, upserted_count = ?4,
                        closed_count = ?5, error_code = NULL, snapshot_hash = ?7,
                        completed_at_ms = ?6
                  WHERE source_id = ?1 AND replay_key = ?2 AND status = 'running'
                    AND snapshot_hash = ?7",
                params![
                    source.id,
                    replay_key,
                    normalized.len() as i64,
                    seen.len() as i64,
                    closed_count,
                    observed_at_ms,
                    snapshot_hash,
                ],
            )?;
            if completed != 1 {
                anyhow::bail!("discovery lease is stale")
            }
            tx.commit()?;
            Ok(DiscoveryRunResult {
                run_id,
                replay_key: replay_key.to_string(),
                status: "completed".to_string(),
                discovered_count: normalized.len() as i64,
                upserted_count: seen.len() as i64,
                closed_count,
                replayed: false,
            })
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            lock_discovery_account_postgres(&mut tx, &source.account_id)?;
            let tx_now = now_ms();
            let fresh = tx.query_opt(
                "SELECT provider, source_key, track_id, source_json, status, updated_at_ms,
                        next_run_at_ms, lease_expires_at_ms, lease_token
                   FROM jobs_discovery_sources
                  WHERE id = $1 AND account_id = $2 FOR UPDATE",
                &[&source.id, &source.account_id],
            )?;
            let Some(fresh) = fresh else {
                anyhow::bail!("discovery lease is stale")
            };
            let provider: String = fresh.get(0);
            let source_key: String = fresh.get(1);
            let track_id: String = fresh.get(2);
            let raw_config: String = fresh.get(3);
            let status: String = fresh.get(4);
            let updated_at_ms: i64 = fresh.get(5);
            let next_run: i64 = fresh.get(6);
            let lease_expires: Option<i64> = fresh.get(7);
            let stored_token: Option<String> = fresh.get(8);
            if provider != source.provider
                || source_key != source.source_key
                || track_id != source.track_id
                || parse_json::<Value>(raw_config, "Jobs discovery source")? != source.config
                || status != "active"
                || updated_at_ms != source.updated_at_ms
                || next_run != scheduled_for_ms
                || lease_expires.is_none_or(|expiry| expiry <= tx_now)
                || stored_token.as_deref() != Some(token_hash.as_str())
            {
                anyhow::bail!("discovery lease is stale")
            }
            let run = tx.query_opt(
                "SELECT id, status, discovered_count, upserted_count, closed_count, snapshot_hash
                   FROM jobs_discovery_runs
                  WHERE source_id = $1 AND replay_key = $2 FOR UPDATE",
                &[&source.id, &replay_key],
            )?.ok_or_else(|| anyhow::anyhow!("discovery lease is stale"))?;
            let run_id: String = run.get(0);
            let run_status: String = run.get(1);
            let discovered_count: i64 = run.get(2);
            let upserted_count: i64 = run.get(3);
            let closed_count: i64 = run.get(4);
            let stored_hash: Option<String> = run.get(5);
            if matches!(run_status.as_str(), "completed" | "failed") {
                if run_status == "completed" && stored_hash.as_deref() != Some(snapshot_hash) {
                    anyhow::bail!("discovery replay payload does not match the completed snapshot")
                }
                return Ok(DiscoveryRunResult {
                    run_id,
                    replay_key: replay_key.to_string(),
                    status: run_status,
                    discovered_count,
                    upserted_count,
                    closed_count,
                    replayed: true,
                });
            }
            if run_status != "running"
                || stored_hash
                    .as_deref()
                    .is_some_and(|hash| hash != snapshot_hash)
            {
                anyhow::bail!("discovery lease is stale")
            }
            tx.execute(
                "UPDATE jobs_discovery_runs SET snapshot_hash = $3
                  WHERE source_id = $1 AND replay_key = $2 AND status = 'running'",
                &[&source.id, &replay_key, &snapshot_hash],
            )?;

            let mut seen = Vec::with_capacity(normalized.len());
            for (external_id, (posting, content_hash)) in normalized {
                let membership_job_id = tx
                    .query_opt(
                        "SELECT job_id FROM jobs_discovery_memberships
                          WHERE source_id = $1 AND external_id = $2 FOR UPDATE",
                        &[&source.id, &external_id],
                    )?
                    .map(|row| row.get::<_, String>(0));
                let membership_posting = match membership_job_id {
                    Some(job_id) => tx
                        .query_opt(
                            "SELECT posting_json FROM jobs_postings
                              WHERE account_id = $1 AND id = $2 FOR UPDATE",
                            &[&source.account_id, &job_id],
                        )?
                        .map(|row| parse_json::<JobPosting>(row.get(0), "job posting"))
                        .transpose()?
                        .ok_or_else(|| {
                            anyhow::anyhow!("discovery membership refers to a missing job")
                        })
                        .map(Some)?,
                    None => None,
                };
                let canonical_posting = tx
                    .query_opt(
                        "SELECT posting_json FROM jobs_postings
                          WHERE account_id = $1 AND canonical_key = $2 FOR UPDATE",
                        &[&source.account_id, &posting.canonical_key],
                    )?
                    .map(|row| parse_json::<JobPosting>(row.get(0), "job posting"))
                    .transpose()?;
                if membership_posting
                    .as_ref()
                    .zip(canonical_posting.as_ref())
                    .is_some_and(|(membership, canonical)| membership.id != canonical.id)
                {
                    anyhow::bail!("discovery membership and canonical job disagree")
                }
                let existing = membership_posting.or(canonical_posting);
                let saved = prepare_snapshot_posting(
                    posting,
                    existing.as_ref().cloned(),
                    &PostingSnapshotContext {
                        profile,
                        preferences,
                        applications,
                        reservations,
                        track,
                        observed_at_ms,
                    },
                )?;
                let payload = to_json(&saved, "job posting")?;
                if existing.is_some() {
                    tx.execute(
                        "UPDATE jobs_postings SET canonical_key = $3, posting_json = $4,
                            source = $5, canonical_url = $6, company = $7, title = $8,
                            location = $9, match_score = $10, status = $11, updated_at_ms = $12
                          WHERE account_id = $1 AND id = $2",
                        &[
                            &source.account_id,
                            &saved.id,
                            &saved.canonical_key,
                            &payload,
                            &saved.source,
                            &saved.canonical_url,
                            &saved.company,
                            &saved.title,
                            &saved.location,
                            &saved.match_score,
                            &saved.status,
                            &saved.updated_at_ms,
                        ],
                    )?;
                } else {
                    tx.execute(
                        "INSERT INTO jobs_postings (
                            id, account_id, canonical_key, posting_json, source, canonical_url,
                            company, title, location, match_score, status, created_at_ms, updated_at_ms
                         ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)",
                        &[
                            &saved.id,
                            &source.account_id,
                            &saved.canonical_key,
                            &payload,
                            &saved.source,
                            &saved.canonical_url,
                            &saved.company,
                            &saved.title,
                            &saved.location,
                            &saved.match_score,
                            &saved.status,
                            &saved.created_at_ms,
                            &saved.updated_at_ms,
                        ],
                    )?;
                }
                tx.execute(
                    "INSERT INTO jobs_discovery_memberships (
                        source_id, account_id, canonical_key, external_id, job_id,
                        content_hash, first_seen_at_ms, last_seen_at_ms,
                        last_seen_run_id, availability_status
                     ) VALUES ($1, $2, $3, $4, $5, $6, $7, $7, $8, 'active')
                     ON CONFLICT(source_id, external_id) DO UPDATE SET
                        canonical_key = EXCLUDED.canonical_key, job_id = EXCLUDED.job_id,
                        content_hash = EXCLUDED.content_hash, last_seen_at_ms = EXCLUDED.last_seen_at_ms,
                        last_seen_run_id = EXCLUDED.last_seen_run_id, availability_status = 'active',
                        missing_count = 0, missing_since_at_ms = NULL",
                    &[
                        &source.id, &source.account_id, &saved.canonical_key, &external_id,
                        &saved.id, &content_hash, &observed_at_ms, &run_id,
                    ],
                )?;
                seen.push(external_id.clone());
            }
            let closed_count = close_missing_snapshot_memberships_postgres(
                &mut tx,
                source,
                &run_id,
                observed_at_ms,
                &seen,
                profile,
                preferences,
                applications,
                reservations,
                track,
            )?;
            let source_changed = tx.execute(
                "UPDATE jobs_discovery_sources
                    SET health = 'healthy', consecutive_failures = 0,
                        last_success_at_ms = $4, last_error_code = NULL,
                        next_run_at_ms = $5, lease_owner = NULL, lease_token = NULL,
                        lease_expires_at_ms = NULL, updated_at_ms = $4
                  WHERE id = $1 AND account_id = $2 AND lease_token = $3",
                &[
                    &source.id,
                    &source.account_id,
                    &token_hash,
                    &observed_at_ms,
                    &next_run_at,
                ],
            )?;
            if source_changed != 1 {
                anyhow::bail!("discovery lease is stale")
            }
            let completed = tx.execute(
                "UPDATE jobs_discovery_runs
                    SET status = 'completed', discovered_count = $3, upserted_count = $4,
                        closed_count = $5, error_code = NULL, snapshot_hash = $7,
                        completed_at_ms = $6
                  WHERE source_id = $1 AND replay_key = $2 AND status = 'running'
                    AND snapshot_hash = $7",
                &[
                    &source.id,
                    &replay_key,
                    &(normalized.len() as i64),
                    &(seen.len() as i64),
                    &closed_count,
                    &observed_at_ms,
                    &snapshot_hash,
                ],
            )?;
            if completed != 1 {
                anyhow::bail!("discovery lease is stale")
            }
            tx.commit()?;
            Ok(DiscoveryRunResult {
                run_id,
                replay_key: replay_key.to_string(),
                status: "completed".to_string(),
                discovered_count: normalized.len() as i64,
                upserted_count: seen.len() as i64,
                closed_count,
                replayed: false,
            })
        }
    })
}

#[allow(clippy::too_many_arguments)]
fn close_missing_snapshot_memberships_sqlite(
    tx: &rusqlite::Transaction<'_>,
    source: &DiscoverySource,
    _run_id: &str,
    observed_at_ms: i64,
    seen: &[String],
    profile: &CareerProfile,
    preferences: &JobPreferences,
    applications: &[JobApplication],
    reservations: &[AttemptReservation],
    track: Option<&CareerTrack>,
) -> Result<i64> {
    const MISSING_GRACE_MS: i64 = 30 * 60 * 1_000;
    let mut stmt = tx.prepare(
        "SELECT external_id, job_id, missing_count, missing_since_at_ms
           FROM jobs_discovery_memberships
          WHERE source_id = ?1 AND availability_status = 'active'",
    )?;
    let active = stmt
        .query_map(params![source.id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, Option<i64>>(3)?,
            ))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    drop(stmt);
    let seen = seen.iter().map(String::as_str).collect::<BTreeSet<_>>();
    let mut closed = 0;
    for (external_id, job_id, missing_count, missing_since_at_ms) in active {
        if seen.contains(external_id.as_str()) {
            continue;
        }
        let missing_since = missing_since_at_ms.unwrap_or(observed_at_ms);
        let next_missing_count = missing_count + 1;
        let should_close = next_missing_count >= 2
            && observed_at_ms.saturating_sub(missing_since) >= MISSING_GRACE_MS;
        let availability = if should_close { "expired" } else { "active" };
        let changed = tx.execute(
            "UPDATE jobs_discovery_memberships
                SET availability_status = ?4, missing_count = ?5, missing_since_at_ms = ?3
              WHERE source_id = ?1 AND external_id = ?2 AND availability_status = 'active'",
            params![
                source.id,
                external_id,
                missing_since,
                availability,
                next_missing_count,
            ],
        )?;
        if changed == 0 || !should_close {
            continue;
        }
        let still_active: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM jobs_discovery_memberships
              WHERE account_id = ?1 AND job_id = ?2 AND availability_status = 'active')",
            params![source.account_id, job_id],
            |row| row.get(0),
        )?;
        if still_active {
            continue;
        }
        let raw: Option<String> = tx
            .query_row(
                "SELECT posting_json FROM jobs_postings WHERE account_id = ?1 AND id = ?2",
                params![source.account_id, job_id],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(raw) = raw {
            let mut posting: JobPosting = parse_json(raw, "job posting")?;
            posting.availability_status = "expired".to_string();
            posting.last_verified_at_ms = Some(observed_at_ms);
            posting.updated_at_ms = observed_at_ms;
            let existing_application_id = applications
                .iter()
                .find(|application| application.job_id == posting.id)
                .map(|application| application.id.as_str());
            posting.eligibility = Some(build_job_eligibility(
                &posting,
                profile,
                preferences,
                reservations,
                true,
                existing_application_id,
                track,
            ));
            let payload = to_json(&posting, "job posting")?;
            tx.execute(
                "UPDATE jobs_postings SET posting_json = ?3, updated_at_ms = ?4
                  WHERE account_id = ?1 AND id = ?2",
                params![source.account_id, job_id, payload, observed_at_ms],
            )?;
        }
        closed += 1;
    }
    Ok(closed)
}

#[allow(clippy::too_many_arguments)]
fn close_missing_snapshot_memberships_postgres(
    tx: &mut postgres::Transaction<'_>,
    source: &DiscoverySource,
    _run_id: &str,
    observed_at_ms: i64,
    seen: &[String],
    profile: &CareerProfile,
    preferences: &JobPreferences,
    applications: &[JobApplication],
    reservations: &[AttemptReservation],
    track: Option<&CareerTrack>,
) -> Result<i64> {
    const MISSING_GRACE_MS: i64 = 30 * 60 * 1_000;
    let active = tx
        .query(
            "SELECT external_id, job_id, missing_count, missing_since_at_ms
               FROM jobs_discovery_memberships
              WHERE source_id = $1 AND availability_status = 'active' FOR UPDATE",
            &[&source.id],
        )?
        .into_iter()
        .map(|row| {
            (
                row.get::<_, String>(0),
                row.get::<_, String>(1),
                row.get::<_, i64>(2),
                row.get::<_, Option<i64>>(3),
            )
        })
        .collect::<Vec<_>>();
    let seen = seen.iter().map(String::as_str).collect::<BTreeSet<_>>();
    let mut closed = 0;
    for (external_id, job_id, missing_count, missing_since_at_ms) in active {
        if seen.contains(external_id.as_str()) {
            continue;
        }
        let missing_since = missing_since_at_ms.unwrap_or(observed_at_ms);
        let next_missing_count = missing_count + 1;
        let should_close = next_missing_count >= 2
            && observed_at_ms.saturating_sub(missing_since) >= MISSING_GRACE_MS;
        let availability = if should_close { "expired" } else { "active" };
        let changed = tx.execute(
            "UPDATE jobs_discovery_memberships
                SET availability_status = $4, missing_count = $5, missing_since_at_ms = $3
              WHERE source_id = $1 AND external_id = $2 AND availability_status = 'active'",
            &[
                &source.id,
                &external_id,
                &missing_since,
                &availability,
                &next_missing_count,
            ],
        )?;
        if changed == 0 || !should_close {
            continue;
        }
        let still_active: bool = tx
            .query_one(
                "SELECT EXISTS(SELECT 1 FROM jobs_discovery_memberships
              WHERE account_id = $1 AND job_id = $2 AND availability_status = 'active')",
                &[&source.account_id, &job_id],
            )?
            .get(0);
        if still_active {
            continue;
        }
        let raw = tx.query_opt(
            "SELECT posting_json FROM jobs_postings WHERE account_id = $1 AND id = $2 FOR UPDATE",
            &[&source.account_id, &job_id],
        )?.map(|row| row.get::<_, String>(0));
        if let Some(raw) = raw {
            let mut posting: JobPosting = parse_json(raw, "job posting")?;
            posting.availability_status = "expired".to_string();
            posting.last_verified_at_ms = Some(observed_at_ms);
            posting.updated_at_ms = observed_at_ms;
            let existing_application_id = applications
                .iter()
                .find(|application| application.job_id == posting.id)
                .map(|application| application.id.as_str());
            posting.eligibility = Some(build_job_eligibility(
                &posting,
                profile,
                preferences,
                reservations,
                true,
                existing_application_id,
                track,
            ));
            let payload = to_json(&posting, "job posting")?;
            tx.execute(
                "UPDATE jobs_postings SET posting_json = $3, updated_at_ms = $4
                  WHERE account_id = $1 AND id = $2",
                &[&source.account_id, &job_id, &payload, &observed_at_ms],
            )?;
        }
        closed += 1;
    }
    Ok(closed)
}

pub fn complete_discovery_run(
    pool: &DbPool,
    source_id: &str,
    lease_token: &str,
    replay_key: &str,
    scheduled_for_ms: i64,
    jobs: &[DiscoveredJobInput],
    complete_snapshot: bool,
) -> Result<DiscoveryRunResult> {
    if !complete_snapshot {
        anyhow::bail!("discovery completion requires a complete provider snapshot")
    }
    if jobs.len() > 10_000 {
        anyhow::bail!("discovery snapshot exceeded the job limit")
    }
    let source = get_discovery_source(pool, source_id)?
        .ok_or_else(|| anyhow::anyhow!("discovery source not found"))?;

    let company = source
        .config
        .get("company")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("discovery source company is missing"))?;
    let profile = get_profile(pool, &source.account_id, "")?;
    let preferences = get_preferences(pool, &source.account_id)?;
    let applications = list_applications(pool, &source.account_id)?;
    let reservations = list_attempt_reservations(pool, &source.account_id)?;
    let tracks = list_tracks(pool, &source.account_id)?;
    let track = tracks.iter().find(|track| track.id == source.track_id);
    let fetched_at_ms = now_ms();
    let mut normalized = BTreeMap::<String, (JobPosting, String)>::new();
    for input in jobs {
        validate_discovered_job(&source, input)?;
        let canonical_url = canonicalize_discovered_url(&source, &input.canonical_url)?;
        let content_hash = discovered_job_content_hash(&source, input, company, &canonical_url);
        let mut posting = JobPosting {
            id: String::new(),
            canonical_key: String::new(),
            source: source.provider.clone(),
            external_id: input.external_id.trim().to_string(),
            company: company.to_string(),
            title: input.title.trim().to_string(),
            location: input.location.trim().to_string(),
            workplace: input.workplace.trim().to_string(),
            canonical_url,
            description: input.description.trim().to_string(),
            compensation: input.compensation.trim().to_string(),
            employment_type: [
                input.employment_type.trim(),
                input.engagement_type.trim(),
            ]
            .into_iter()
            .filter(|value| !value.is_empty())
            .collect::<Vec<_>>()
            .join(" "),
            track_id: source.track_id.clone(),
            match_score: 0,
            matched_reasons: Vec::new(),
            missing_requirements: Vec::new(),
            posted_at_ms: input
                .posted_at_ms
                .filter(|value| *value <= fetched_at_ms + DAY_MS),
            last_verified_at_ms: Some(fetched_at_ms),
            availability_status: "active".to_string(),
            status: "matched".to_string(),
            created_at_ms: 0,
            updated_at_ms: 0,
            eligibility: None,
        };
        posting.canonical_key = canonical_job_key(&posting);
        let external_id = posting.external_id.clone();
        match normalized.get(&external_id) {
            Some((existing, existing_hash))
                if existing_hash == &content_hash
                    && existing.canonical_key == posting.canonical_key => {}
            Some(_) => {
                anyhow::bail!("discovery snapshot contains a conflicting external job ID")
            }
            None => {
                normalized.insert(external_id, (posting, content_hash));
            }
        }
    }

    let snapshot_hash = discovery_snapshot_hash(&source, replay_key, &normalized);
    if let Some(existing) =
        completed_discovery_run(pool, source_id, replay_key, Some(&snapshot_hash))?
    {
        return Ok(DiscoveryRunResult {
            replayed: true,
            ..existing
        });
    }
    publish_discovery_snapshot(
        pool,
        &source,
        lease_token,
        replay_key,
        scheduled_for_ms,
        &normalized,
        &snapshot_hash,
        &profile,
        &preferences,
        &applications,
        &reservations,
        track,
        fetched_at_ms,
    )
}

pub fn fail_discovery_run(
    pool: &DbPool,
    source_id: &str,
    lease_token: &str,
    replay_key: &str,
    scheduled_for_ms: i64,
    error_code: &str,
) -> Result<DiscoveryRunResult> {
    if !matches!(
        error_code,
        "throttled"
            | "timeout"
            | "unavailable"
            | "invalid_response"
            | "unauthorized"
            | "provider_error"
            | "unsupported_provider"
    ) {
        anyhow::bail!("invalid discovery failure code")
    }
    let now = now_ms();
    let token_hash = discovery_lease_token_hash(lease_token);
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let (source, stored_token) = tx
                .query_row(
                    "SELECT id, account_id, track_id, provider, source_key, source_json,
                            status, health, consecutive_failures, run_interval_ms,
                            next_run_at_ms, last_success_at_ms, last_failure_at_ms,
                            last_error_code, lease_expires_at_ms, created_at_ms, updated_at_ms,
                            lease_token
                       FROM jobs_discovery_sources WHERE id = ?1",
                    params![source_id],
                    |row| {
                        Ok((
                            discovery_source_from_sqlite_row(row)?,
                            row.get::<_, Option<String>>(17)?,
                        ))
                    },
                )
                .optional()?
                .ok_or_else(|| anyhow::anyhow!("discovery source not found"))?;
            let run = tx
                .query_row(
                    "SELECT id, status, discovered_count, upserted_count, closed_count
                       FROM jobs_discovery_runs WHERE source_id = ?1 AND replay_key = ?2",
                    params![source.id, replay_key],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, i64>(2)?,
                            row.get::<_, i64>(3)?,
                            row.get::<_, i64>(4)?,
                        ))
                    },
                )
                .optional()?
                .ok_or_else(|| anyhow::anyhow!("discovery lease is stale"))?;
            if matches!(run.1.as_str(), "completed" | "failed") {
                tx.commit()?;
                return Ok(DiscoveryRunResult {
                    run_id: run.0,
                    replay_key: replay_key.to_string(),
                    status: run.1,
                    discovered_count: run.2,
                    upserted_count: run.3,
                    closed_count: run.4,
                    replayed: true,
                });
            }
            if lease_token.len() < 32
                || replay_key != discovery_replay_key(&source.id, scheduled_for_ms)
                || scheduled_for_ms != source.next_run_at_ms
                || source.status != "active"
                || source
                    .lease_expires_at_ms
                    .is_none_or(|expiry| expiry <= now)
                || stored_token.as_deref() != Some(token_hash.as_str())
                || run.1 != "running"
            {
                anyhow::bail!("discovery lease is stale")
            }
            let failures = source.consecutive_failures + 1;
            let health = if failures >= 3 { "paused" } else { "degraded" };
            let next_run_at = discovery_next_run_at(&source, now, failures);
            let source_changed = tx.execute(
                "UPDATE jobs_discovery_sources
                    SET health = ?4, consecutive_failures = ?5,
                        last_failure_at_ms = ?6, last_error_code = ?7,
                        next_run_at_ms = ?8, lease_owner = NULL, lease_token = NULL,
                        lease_expires_at_ms = NULL, updated_at_ms = ?6
                  WHERE id = ?1 AND account_id = ?2 AND lease_token = ?3",
                params![
                    source.id,
                    source.account_id,
                    token_hash,
                    health,
                    failures,
                    now,
                    error_code,
                    next_run_at
                ],
            )?;
            if source_changed != 1 {
                anyhow::bail!("discovery lease is stale")
            }
            let run_changed = tx.execute(
                "UPDATE jobs_discovery_runs
                    SET status = 'failed', error_code = ?3, completed_at_ms = ?4
                  WHERE source_id = ?1 AND replay_key = ?2 AND status = 'running'",
                params![source.id, replay_key, error_code, now],
            )?;
            if run_changed != 1 {
                anyhow::bail!("discovery lease is stale")
            }
            tx.commit()?;
            Ok(DiscoveryRunResult {
                run_id: run.0,
                replay_key: replay_key.to_string(),
                status: "failed".to_string(),
                discovered_count: 0,
                upserted_count: 0,
                closed_count: 0,
                replayed: false,
            })
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            let account_id: String = tx
                .query_opt(
                    "SELECT account_id FROM jobs_discovery_sources WHERE id = $1",
                    &[&source_id],
                )?
                .map(|row| row.get(0))
                .ok_or_else(|| anyhow::anyhow!("discovery source not found"))?;
            lock_discovery_account_postgres(&mut tx, &account_id)?;
            let row = tx
                .query_opt(
                    "SELECT id, account_id, track_id, provider, source_key, source_json,
                            status, health, consecutive_failures, run_interval_ms,
                            next_run_at_ms, last_success_at_ms, last_failure_at_ms,
                            last_error_code, lease_expires_at_ms, created_at_ms, updated_at_ms,
                            lease_token
                       FROM jobs_discovery_sources WHERE id = $1 FOR UPDATE",
                    &[&source_id],
                )?
                .ok_or_else(|| anyhow::anyhow!("discovery source not found"))?;
            let stored_token: Option<String> = row.get(17);
            let source = discovery_source_from_pg_row(row)?;
            let run = tx
                .query_opt(
                    "SELECT id, status, discovered_count, upserted_count, closed_count
                       FROM jobs_discovery_runs
                      WHERE source_id = $1 AND replay_key = $2 FOR UPDATE",
                    &[&source.id, &replay_key],
                )?
                .map(|row| {
                    (
                        row.get::<_, String>(0),
                        row.get::<_, String>(1),
                        row.get::<_, i64>(2),
                        row.get::<_, i64>(3),
                        row.get::<_, i64>(4),
                    )
                })
                .ok_or_else(|| anyhow::anyhow!("discovery lease is stale"))?;
            if matches!(run.1.as_str(), "completed" | "failed") {
                tx.commit()?;
                return Ok(DiscoveryRunResult {
                    run_id: run.0,
                    replay_key: replay_key.to_string(),
                    status: run.1,
                    discovered_count: run.2,
                    upserted_count: run.3,
                    closed_count: run.4,
                    replayed: true,
                });
            }
            if lease_token.len() < 32
                || replay_key != discovery_replay_key(&source.id, scheduled_for_ms)
                || scheduled_for_ms != source.next_run_at_ms
                || source.status != "active"
                || source
                    .lease_expires_at_ms
                    .is_none_or(|expiry| expiry <= now)
                || stored_token.as_deref() != Some(token_hash.as_str())
                || run.1 != "running"
            {
                anyhow::bail!("discovery lease is stale")
            }
            let failures = source.consecutive_failures + 1;
            let health = if failures >= 3 { "paused" } else { "degraded" };
            let next_run_at = discovery_next_run_at(&source, now, failures);
            let source_changed = tx.execute(
                "UPDATE jobs_discovery_sources
                    SET health = $4, consecutive_failures = $5,
                        last_failure_at_ms = $6, last_error_code = $7,
                        next_run_at_ms = $8, lease_owner = NULL, lease_token = NULL,
                        lease_expires_at_ms = NULL, updated_at_ms = $6
                  WHERE id = $1 AND account_id = $2 AND lease_token = $3",
                &[
                    &source.id,
                    &source.account_id,
                    &token_hash,
                    &health,
                    &failures,
                    &now,
                    &error_code,
                    &next_run_at,
                ],
            )?;
            if source_changed != 1 {
                anyhow::bail!("discovery lease is stale")
            }
            let run_changed = tx.execute(
                "UPDATE jobs_discovery_runs
                    SET status = 'failed', error_code = $3, completed_at_ms = $4
                  WHERE source_id = $1 AND replay_key = $2 AND status = 'running'",
                &[&source.id, &replay_key, &error_code, &now],
            )?;
            if run_changed != 1 {
                anyhow::bail!("discovery lease is stale")
            }
            tx.commit()?;
            Ok(DiscoveryRunResult {
                run_id: run.0,
                replay_key: replay_key.to_string(),
                status: "failed".to_string(),
                discovered_count: 0,
                upserted_count: 0,
                closed_count: 0,
                replayed: false,
            })
        }
    })
}

fn validate_discovered_job(source: &DiscoverySource, input: &DiscoveredJobInput) -> Result<()> {
    if input.external_id.trim().is_empty()
        || input.external_id.chars().count() > 240
        || input.title.trim().is_empty()
        || input.title.chars().count() > 500
        || input.description.chars().count() > 200_000
        || input.employment_type.chars().count() > 80
        || input.engagement_type.chars().count() > 80
    {
        anyhow::bail!("discovery job is invalid")
    }
    if !is_canonical_discovered_employment_type(&input.employment_type)
        || !is_canonical_discovered_engagement_type(&input.engagement_type)
    {
        anyhow::bail!("discovery job category is invalid")
    }
    canonicalize_discovered_url(source, &input.canonical_url)?;
    Ok(())
}

fn is_canonical_discovered_employment_type(value: &str) -> bool {
    matches!(
        value.trim(),
        ""
            | "full_time"
            | "part_time"
            | "contract"
            | "temporary"
            | "internship"
            | "apprenticeship"
            | "seasonal"
            | "per_diem"
    )
}

fn is_canonical_discovered_engagement_type(value: &str) -> bool {
    matches!(value.trim(), "" | "w2" | "c2c" | "1099" | "direct_hire")
}

fn canonicalize_discovered_url(source: &DiscoverySource, raw: &str) -> Result<String> {
    let (canonical_url, source_key) = canonical_public_discovery_url(&source.provider, raw)?;
    if source_key != source.source_key {
        anyhow::bail!("discovery job URL does not belong to the configured source")
    }
    Ok(canonical_url)
}

/// Derive a scheduled public-ATS source only from a job that the server's
/// allowlisted importer has already verified. Unknown or manual imports never
/// become scheduled sources.
pub fn discovery_source_input_from_verified_import(
    imported_source: &str,
    canonical_url: &str,
    company: &str,
    track_id: &str,
) -> Result<Option<DiscoverySourceInput>> {
    let provider = match imported_source {
        "greenhouse_import" => "greenhouse",
        "lever_import" => "lever",
        "ashby_import" => "ashby",
        "smartrecruiters_import" => "smartrecruiters",
        "workday_import" => "workday",
        _ => return Ok(None),
    };
    let (_, source_key) = canonical_public_discovery_url(provider, canonical_url)?;
    let company = company.trim();
    if company.is_empty() || company.chars().count() > 200 {
        anyhow::bail!("verified job import has no valid company")
    }

    Ok(Some(DiscoverySourceInput {
        track_id: track_id.to_string(),
        provider: provider.to_string(),
        source_key,
        company: company.to_string(),
        run_interval_ms: default_discovery_interval_ms(),
    }))
}

/// Parse one of the public ATS links used by both the server importer and the
/// scheduler. Keeping one parser prevents an import from being saved in a form
/// that discovery later refuses to enroll.
pub fn canonical_public_discovery_url(provider: &str, raw: &str) -> Result<(String, String)> {
    let mut url = reqwest::Url::parse(raw.trim()).context("parse discovered job URL")?;
    if url.scheme() != "https" || !url.username().is_empty() || url.password().is_some() {
        anyhow::bail!("discovery job URL must be public HTTPS without credentials")
    }
    if url.port().is_some() {
        anyhow::bail!("discovery job URL must use the default HTTPS port")
    }
    let host = url.host_str().unwrap_or_default().to_ascii_lowercase();
    let segments = url
        .path_segments()
        .map(|segments| segments.collect::<Vec<_>>())
        .unwrap_or_default();
    let source_key = match provider {
        "greenhouse"
            if matches!(
                host.as_str(),
                "boards.greenhouse.io" | "job-boards.greenhouse.io"
            ) =>
        {
            source_key_before_job_segment(&segments, "jobs", "Greenhouse")?
        }
        "lever" if matches!(host.as_str(), "jobs.lever.co" | "jobs.eu.lever.co") => {
            direct_board_source_key(&segments, "Lever")?
        }
        "ashby" if host == "jobs.ashbyhq.com" => direct_board_source_key(&segments, "Ashby")?,
        "smartrecruiters" if host == "jobs.smartrecruiters.com" => {
            direct_board_source_key(&segments, "SmartRecruiters")?
        }
        "workday" => workday_source_key(&host, &segments)?,
        _ => anyhow::bail!("discovery job URL does not belong to the configured provider"),
    };
    let mut retained_query = url
        .query_pairs()
        .filter(|(key, _)| !is_tracking_query_key(key))
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect::<Vec<_>>();
    retained_query.sort();
    url.set_query(None);
    if !retained_query.is_empty() {
        let mut query = url.query_pairs_mut();
        for (key, value) in retained_query {
            query.append_pair(&key, &value);
        }
    }
    url.set_fragment(None);
    Ok((
        url.to_string().trim_end_matches('/').to_string(),
        source_key,
    ))
}

fn direct_board_source_key(segments: &[&str], provider: &str) -> Result<String> {
    let Some((source_key, job_id)) = segments.first().zip(segments.get(1)) else {
        anyhow::bail!("discovery {provider} URL must include a board and job ID")
    };
    checked_discovery_identifier(source_key, "board identifier")?;
    checked_discovery_identifier(job_id, "job identifier")?;
    Ok((*source_key).to_string())
}

fn source_key_before_job_segment(
    segments: &[&str],
    job_segment: &str,
    provider: &str,
) -> Result<String> {
    let mut matches = segments
        .iter()
        .enumerate()
        .filter(|(_, segment)| **segment == job_segment);
    let Some((job_index, _)) = matches.next() else {
        anyhow::bail!("discovery {provider} URL must include a job ID")
    };
    if matches.next().is_some() || job_index == 0 || segments.get(job_index + 1).is_none() {
        anyhow::bail!("discovery {provider} URL is ambiguous")
    }
    let source_key = segments[job_index - 1];
    checked_discovery_identifier(source_key, "board identifier")?;
    checked_discovery_identifier(segments[job_index + 1], "job identifier")?;
    Ok(source_key.to_string())
}

fn workday_source_key(host: &str, segments: &[&str]) -> Result<String> {
    let host_parts = host.split('.').collect::<Vec<_>>();
    if host_parts.len() != 4 || host_parts[2] != "myworkdayjobs" || host_parts[3] != "com" {
        anyhow::bail!("discovery job URL does not belong to the configured provider")
    }
    let tenant = host_parts[0];
    let instance = host_parts[1];
    checked_discovery_identifier(tenant, "Workday tenant")?;
    checked_discovery_identifier(instance, "Workday instance")?;
    if !instance.starts_with("wd") {
        anyhow::bail!("discovery Workday URL has an invalid instance")
    }
    let mut matches = segments
        .iter()
        .enumerate()
        .filter(|(_, segment)| **segment == "job");
    let Some((job_index, _)) = matches.next() else {
        anyhow::bail!("discovery Workday URL must include a job requisition")
    };
    if matches.next().is_some() || job_index == 0 || segments.len() <= job_index + 1 {
        anyhow::bail!("discovery Workday URL is ambiguous")
    }
    let site = segments[job_index - 1];
    checked_discovery_identifier(site, "Workday site")?;
    if segments[job_index + 1..]
        .iter()
        .any(|segment| checked_discovery_identifier(segment, "Workday job path").is_err())
    {
        anyhow::bail!("discovery Workday URL has an invalid job path")
    }
    Ok(format!("{tenant}~{instance}~{site}"))
}

fn checked_discovery_identifier(value: &str, label: &str) -> Result<()> {
    if value.is_empty()
        || value.len() > 160
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        anyhow::bail!("discovery {label} contains unsupported characters")
    }
    Ok(())
}

fn discovered_job_content_hash(
    source: &DiscoverySource,
    input: &DiscoveredJobInput,
    company: &str,
    canonical_url: &str,
) -> String {
    let payload = json!({
        "provider": source.provider,
        "source_key": source.source_key,
        "external_id": input.external_id.trim(),
        "canonical_url": canonical_url,
        "company": company,
        "title": input.title.trim(),
        "location": input.location.trim(),
        "workplace": input.workplace.trim(),
        "description": input.description.trim(),
        "compensation": input.compensation.trim(),
        "employment_type": input.employment_type.trim(),
        "engagement_type": input.engagement_type.trim(),
        "posted_at_ms": input.posted_at_ms,
    });
    hex::encode(Sha256::digest(
        serde_json::to_vec(&payload).unwrap_or_default(),
    ))
}

fn discovery_snapshot_hash(
    source: &DiscoverySource,
    replay_key: &str,
    jobs: &BTreeMap<String, (JobPosting, String)>,
) -> String {
    let entries = jobs
        .iter()
        .map(|(external_id, (posting, content_hash))| {
            json!({
                "external_id": external_id,
                "canonical_key": posting.canonical_key,
                "content_hash": content_hash,
            })
        })
        .collect::<Vec<_>>();
    let payload = json!({
        "source_id": source.id,
        "replay_key": replay_key,
        "entries": entries,
    });
    hex::encode(Sha256::digest(
        serde_json::to_vec(&payload).unwrap_or_default(),
    ))
}

fn completed_discovery_run(
    pool: &DbPool,
    source_id: &str,
    replay_key: &str,
    expected_snapshot_hash: Option<&str>,
) -> Result<Option<DiscoveryRunResult>> {
    let completed = crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => pool
            .get()?
            .query_row(
                "SELECT id, replay_key, status, discovered_count, upserted_count,
                        closed_count, snapshot_hash
                   FROM jobs_discovery_runs
                  WHERE source_id = ?1 AND replay_key = ?2
                    AND status IN ('completed', 'failed')",
                params![source_id, replay_key],
                |row| {
                    Ok((
                        DiscoveryRunResult {
                            run_id: row.get(0)?,
                            replay_key: row.get(1)?,
                            status: row.get(2)?,
                            discovered_count: row.get(3)?,
                            upserted_count: row.get(4)?,
                            closed_count: row.get(5)?,
                            replayed: false,
                        },
                        row.get::<_, Option<String>>(6)?,
                    ))
                },
            )
            .optional()
            .context("get completed discovery run"),
        DbPool::Postgres(_) => pool
            .get_pg()?
            .query_opt(
                "SELECT id, replay_key, status, discovered_count, upserted_count,
                        closed_count, snapshot_hash
                   FROM jobs_discovery_runs
                  WHERE source_id = $1 AND replay_key = $2
                    AND status IN ('completed', 'failed')",
                &[&source_id, &replay_key],
            )?
            .map(|row| {
                (
                    DiscoveryRunResult {
                        run_id: row.get(0),
                        replay_key: row.get(1),
                        status: row.get(2),
                        discovered_count: row.get(3),
                        upserted_count: row.get(4),
                        closed_count: row.get(5),
                        replayed: false,
                    },
                    row.get::<_, Option<String>>(6),
                )
            })
            .map(Ok)
            .transpose(),
    })?;
    if let (Some(expected), Some((result, stored))) = (expected_snapshot_hash, completed.as_ref()) {
        if result.status == "completed" && stored.as_deref() != Some(expected) {
            anyhow::bail!("discovery replay payload does not match the completed snapshot")
        }
    }
    Ok(completed.map(|(result, _)| result))
}
