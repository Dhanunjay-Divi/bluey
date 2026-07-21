const GLOBAL_DISCOVERY_MAX_CONSIDERED_PER_ACCOUNT: usize = 2_000;

#[derive(Debug, Clone)]
struct GlobalCandidateMaterializationRow {
    id: String,
    candidate_json: String,
    updated_at_ms: i64,
}

#[derive(Debug, Clone)]
struct ExistingGlobalMaterialization {
    job_id: String,
    track_id: String,
    source_updated_at_ms: i64,
    materialized_at_ms: i64,
}

#[derive(Debug)]
struct RankedGlobalCandidate {
    row: GlobalCandidateMaterializationRow,
    input: DiscoveredJobInput,
    posting: JobPosting,
    track_id: String,
    score: i64,
}

/// Project the shared candidate index into one account's bounded, Track-aware
/// review queue. Shared-feed rows remain unverified discovery leads: the
/// account source deliberately stays in `waiting` health until the original
/// employer page is revalidated by a certified importer.
pub fn materialize_global_candidates_for_account(
    pool: &DbPool,
    account_id: &str,
    email: &str,
) -> Result<GlobalMaterializationResult> {
    let source = match ensure_managed_curated_discovery_source(pool, account_id)? {
        Some(source) => source,
        None => return Ok(empty_global_materialization_result()),
    };
    let profile = get_profile(pool, account_id, email)?;
    let preferences = get_preferences(pool, account_id)?;
    let tracks = list_tracks(pool, account_id)?
        .into_iter()
        .filter(|track| track.active)
        .collect::<Vec<_>>();
    if tracks.is_empty() {
        return Ok(empty_global_materialization_result());
    }

    let profile_revision_at_ms = tracks
        .iter()
        .map(|track| track.updated_at_ms)
        .chain([profile.updated_at_ms, preferences.updated_at_ms])
        .max()
        .unwrap_or_default();
    let global_updated_at_ms = global_candidate_index_revision(pool)?;
    if global_updated_at_ms == 0
        || global_materialization_is_current(
            pool,
            account_id,
            global_updated_at_ms,
            profile_revision_at_ms,
        )?
    {
        return Ok(empty_global_materialization_result());
    }

    let mut existing_materializations =
        load_existing_global_materializations(pool, account_id)?;
    let mut postings_by_canonical = list_postings(pool, account_id)?
        .into_iter()
        .map(|posting| (posting.canonical_key.clone(), posting))
        .collect::<BTreeMap<_, _>>();
    let mut result = empty_global_materialization_result();

    for (row, job_id) in load_expired_global_materializations(pool, account_id)? {
        result.considered_count += 1;
        if let Some(mut posting) = get_posting(pool, account_id, &job_id)? {
            if is_curated_materialized_source(&posting.source) {
                posting.availability_status = "expired".to_string();
                let saved = upsert_posting(
                    pool,
                    account_id,
                    &posting,
                    &profile,
                    &preferences,
                )?;
                postings_by_canonical.insert(saved.canonical_key.clone(), saved);
            }
        }
        let input: DiscoveredJobInput =
            parse_json(row.candidate_json, "expired global job candidate")?;
        remove_global_materialization(
            pool,
            account_id,
            &source.id,
            &row.id,
            &namespaced_global_external_id(&input),
        )?;
        existing_materializations.remove(&row.id);
    }

    let maximum_age_days = preferences.max_posting_age_days.clamp(1, 90);
    let cutoff_at_ms = now_ms().saturating_sub(maximum_age_days.saturating_mul(DAY_MS));
    let candidates = load_recent_global_candidates(
        pool,
        cutoff_at_ms,
        GLOBAL_DISCOVERY_MAX_CONSIDERED_PER_ACCOUNT,
    )?;
    let mut ranked = Vec::with_capacity(candidates.len());
    for row in candidates {
        result.considered_count += 1;
        let mut input: DiscoveredJobInput =
            parse_json(row.candidate_json.clone(), "global job candidate")?;
        input.external_id = namespaced_global_external_id(&input);
        let mut posting = global_candidate_posting(&input);
        posting.canonical_key = canonical_job_key(&posting);

        if let Some(existing) = postings_by_canonical.get(&posting.canonical_key) {
            if !is_curated_materialized_source(&existing.source) {
                if existing_materializations.contains_key(&row.id) {
                    remove_global_materialization(
                        pool,
                        account_id,
                        &source.id,
                        &row.id,
                        &input.external_id,
                    )?;
                    existing_materializations.remove(&row.id);
                }
                result.skipped_count += 1;
                continue;
            }
        }

        let Some(track) = best_curated_discovery_track(
            &posting,
            &profile,
            &preferences,
            &tracks,
        ) else {
            result.skipped_count += 1;
            continue;
        };
        posting.track_id = track.id.clone();
        let (score, _, _) = score_posting(&posting, &profile, &preferences, Some(track));
        ranked.push(RankedGlobalCandidate {
            row,
            input,
            posting,
            track_id: track.id.clone(),
            score,
        });
    }
    ranked.sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then_with(|| {
                right
                    .posting
                    .posted_at_ms
                    .cmp(&left.posting.posted_at_ms)
            })
            .then_with(|| right.row.updated_at_ms.cmp(&left.row.updated_at_ms))
            .then_with(|| left.row.id.cmp(&right.row.id))
    });

    let mut active_materializations = existing_materializations.len();
    for mut candidate in ranked {
        let existing_mapping = existing_materializations.get(&candidate.row.id).cloned();
        if let Some(existing) = &existing_mapping {
            if existing.track_id != candidate.track_id {
                result.skipped_count += 1;
                continue;
            }
            if existing.source_updated_at_ms >= candidate.row.updated_at_ms
                && existing.materialized_at_ms >= profile_revision_at_ms
            {
                result.skipped_count += 1;
                continue;
            }
            candidate.posting.id = existing.job_id.clone();
        } else if active_materializations >= GLOBAL_DISCOVERY_MAX_MATERIALIZED_PER_ACCOUNT {
            result.skipped_count += 1;
            continue;
        }

        if let Some(existing) = postings_by_canonical.get(&candidate.posting.canonical_key) {
            if existing.track_id != candidate.track_id {
                result.skipped_count += 1;
                continue;
            }
            candidate.posting.id = existing.id.clone();
        }
        let saved = upsert_posting(
            pool,
            account_id,
            &candidate.posting,
            &profile,
            &preferences,
        )?;
        let content_hash = discovered_job_content_hash(
            &source,
            &candidate.input,
            &candidate.input.company,
            &saved.canonical_url,
        );
        persist_global_materialization(
            pool,
            account_id,
            GlobalMaterializationPersist {
                source_id: &source.id,
                candidate_id: &candidate.row.id,
                external_id: &candidate.input.external_id,
                posting: &saved,
                content_hash: &content_hash,
                source_updated_at_ms: candidate.row.updated_at_ms,
            },
        )?;
        postings_by_canonical.insert(saved.canonical_key.clone(), saved);
        if existing_mapping.is_some() {
            result.refreshed_count += 1;
        } else {
            active_materializations += 1;
            result.materialized_count += 1;
        }
    }

    save_global_materialization_state(
        pool,
        account_id,
        global_updated_at_ms,
        profile_revision_at_ms,
    )?;
    Ok(result)
}

fn empty_global_materialization_result() -> GlobalMaterializationResult {
    GlobalMaterializationResult {
        considered_count: 0,
        materialized_count: 0,
        refreshed_count: 0,
        skipped_count: 0,
    }
}

fn global_candidate_posting(input: &DiscoveredJobInput) -> JobPosting {
    JobPosting {
        id: String::new(),
        canonical_key: String::new(),
        source: format!(
            "{CURATED_DISCOVERY_PROVIDER}:{}",
            input.source_catalog_id.trim()
        ),
        external_id: input.external_id.clone(),
        company: input.company.trim().to_string(),
        title: input.title.trim().to_string(),
        location: input.location.trim().to_string(),
        workplace: input.workplace.trim().to_string(),
        canonical_url: input.canonical_url.trim().to_string(),
        description: input.description.trim().to_string(),
        compensation: input.compensation.trim().to_string(),
        employment_type: [input.employment_type.trim(), input.engagement_type.trim()]
            .into_iter()
            .filter(|value| !value.is_empty())
            .collect::<Vec<_>>()
            .join(" "),
        track_id: String::new(),
        match_score: 0,
        matched_reasons: Vec::new(),
        missing_requirements: Vec::new(),
        posted_at_ms: input.posted_at_ms,
        last_verified_at_ms: None,
        availability_status: "unknown".to_string(),
        status: default_match_status(),
        created_at_ms: 0,
        updated_at_ms: 0,
        eligibility: None,
    }
}

fn namespaced_global_external_id(input: &DiscoveredJobInput) -> String {
    let namespace = input.source_catalog_id.trim();
    let external_id = input.external_id.trim();
    if external_id.starts_with(&format!("{namespace}:")) {
        external_id.to_string()
    } else {
        format!("{namespace}:{external_id}")
    }
}

fn is_curated_materialized_source(source: &str) -> bool {
    source == CURATED_DISCOVERY_PROVIDER
        || source.starts_with(&format!("{CURATED_DISCOVERY_PROVIDER}:"))
}

fn global_candidate_index_revision(pool: &DbPool) -> Result<i64> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => pool
            .get()?
            .query_row(
                "SELECT COALESCE(MAX(updated_at_ms), 0) FROM jobs_global_candidates",
                [],
                |row| row.get(0),
            )
            .map_err(Into::into),
        DbPool::Postgres(_) => pool
            .get_pg()?
            .query_one(
                "SELECT COALESCE(MAX(updated_at_ms), 0)::BIGINT FROM jobs_global_candidates",
                &[],
            )
            .map(|row| row.get(0))
            .map_err(Into::into),
    })
}

fn global_materialization_is_current(
    pool: &DbPool,
    account_id: &str,
    global_updated_at_ms: i64,
    profile_revision_at_ms: i64,
) -> Result<bool> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => pool
            .get()?
            .query_row(
                "SELECT global_updated_at_ms >= ?2 AND profile_revision_at_ms >= ?3
                   FROM jobs_global_materialization_state WHERE account_id = ?1",
                params![account_id, global_updated_at_ms, profile_revision_at_ms],
                |row| row.get(0),
            )
            .optional()
            .map(|value| value.unwrap_or(false))
            .map_err(Into::into),
        DbPool::Postgres(_) => pool
            .get_pg()?
            .query_opt(
                "SELECT global_updated_at_ms >= $2 AND profile_revision_at_ms >= $3
                   FROM jobs_global_materialization_state WHERE account_id = $1",
                &[&account_id, &global_updated_at_ms, &profile_revision_at_ms],
            )
            .map(|row| row.map(|row| row.get(0)).unwrap_or(false))
            .map_err(Into::into),
    })
}

fn load_recent_global_candidates(
    pool: &DbPool,
    cutoff_at_ms: i64,
    limit: usize,
) -> Result<Vec<GlobalCandidateMaterializationRow>> {
    let limit = i64::try_from(limit).context("global materialization limit")?;
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get()?;
            let mut stmt = conn.prepare(
                "SELECT id, candidate_json, updated_at_ms
                   FROM jobs_global_candidates
                  WHERE availability_status <> 'expired'
                    AND (posted_at_ms >= ?1 OR (posted_at_ms IS NULL AND updated_at_ms >= ?1))
                  ORDER BY posted_at_ms DESC, updated_at_ms DESC, id LIMIT ?2",
            )?;
            let values = stmt.query_map(params![cutoff_at_ms, limit], |row| {
                Ok(GlobalCandidateMaterializationRow {
                    id: row.get(0)?,
                    candidate_json: row.get(1)?,
                    updated_at_ms: row.get(2)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into);
            values
        }
        DbPool::Postgres(_) => pool
            .get_pg()?
            .query(
                "SELECT id, candidate_json, updated_at_ms
                   FROM jobs_global_candidates
                  WHERE availability_status <> 'expired'
                    AND (posted_at_ms >= $1 OR (posted_at_ms IS NULL AND updated_at_ms >= $1))
                  ORDER BY posted_at_ms DESC NULLS LAST, updated_at_ms DESC, id LIMIT $2",
                &[&cutoff_at_ms, &limit],
            )?
            .into_iter()
            .map(|row| {
                Ok(GlobalCandidateMaterializationRow {
                    id: row.get(0),
                    candidate_json: row.get(1),
                    updated_at_ms: row.get(2),
                })
            })
            .collect(),
    })
}

fn load_existing_global_materializations(
    pool: &DbPool,
    account_id: &str,
) -> Result<BTreeMap<String, ExistingGlobalMaterialization>> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get()?;
            let mut stmt = conn.prepare(
                "SELECT candidate_id, job_id, track_id, source_updated_at_ms, materialized_at_ms
                   FROM jobs_global_candidate_materializations WHERE account_id = ?1",
            )?;
            let values = stmt.query_map(params![account_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    ExistingGlobalMaterialization {
                        job_id: row.get(1)?,
                        track_id: row.get(2)?,
                        source_updated_at_ms: row.get(3)?,
                        materialized_at_ms: row.get(4)?,
                    },
                ))
            })?
            .collect::<std::result::Result<BTreeMap<_, _>, _>>()
            .map_err(Into::into);
            values
        }
        DbPool::Postgres(_) => {
            let values = pool
                .get_pg()?
                .query(
                "SELECT candidate_id, job_id, track_id, source_updated_at_ms, materialized_at_ms
                   FROM jobs_global_candidate_materializations WHERE account_id = $1",
                &[&account_id],
                )?
                .into_iter()
                .map(|row| {
                    (
                        row.get::<_, String>(0),
                        ExistingGlobalMaterialization {
                            job_id: row.get(1),
                            track_id: row.get(2),
                            source_updated_at_ms: row.get(3),
                            materialized_at_ms: row.get(4),
                        },
                    )
                })
                .collect::<BTreeMap<_, _>>();
            Ok(values)
        }
    })
}

fn load_expired_global_materializations(
    pool: &DbPool,
    account_id: &str,
) -> Result<Vec<(GlobalCandidateMaterializationRow, String)>> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get()?;
            let mut stmt = conn.prepare(
                "SELECT candidate.id, candidate.candidate_json, candidate.availability_status,
                        candidate.updated_at_ms, materialization.job_id
                   FROM jobs_global_candidate_materializations materialization
                   JOIN jobs_global_candidates candidate ON candidate.id = materialization.candidate_id
                  WHERE materialization.account_id = ?1
                    AND candidate.availability_status = 'expired'",
            )?;
            let values = stmt.query_map(params![account_id], |row| {
                Ok((
                    GlobalCandidateMaterializationRow {
                        id: row.get(0)?,
                        candidate_json: row.get(1)?,
                        updated_at_ms: row.get(3)?,
                    },
                    row.get(4)?,
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into);
            values
        }
        DbPool::Postgres(_) => {
            let values = pool
                .get_pg()?
                .query(
                "SELECT candidate.id, candidate.candidate_json, candidate.availability_status,
                        candidate.updated_at_ms, materialization.job_id
                   FROM jobs_global_candidate_materializations materialization
                   JOIN jobs_global_candidates candidate ON candidate.id = materialization.candidate_id
                  WHERE materialization.account_id = $1
                    AND candidate.availability_status = 'expired'",
                &[&account_id],
                )?
                .into_iter()
                .map(|row| {
                    (
                        GlobalCandidateMaterializationRow {
                            id: row.get(0),
                            candidate_json: row.get(1),
                            updated_at_ms: row.get(3),
                        },
                        row.get(4),
                    )
                })
                .collect::<Vec<_>>();
            Ok(values)
        }
    })
}

struct GlobalMaterializationPersist<'a> {
    source_id: &'a str,
    candidate_id: &'a str,
    external_id: &'a str,
    posting: &'a JobPosting,
    content_hash: &'a str,
    source_updated_at_ms: i64,
}

fn persist_global_materialization(
    pool: &DbPool,
    account_id: &str,
    materialization: GlobalMaterializationPersist<'_>,
) -> Result<()> {
    let GlobalMaterializationPersist {
        source_id,
        candidate_id,
        external_id,
        posting,
        content_hash,
        source_updated_at_ms,
    } = materialization;
    let now = now_ms();
    let run_id = format!("global-materialize:{candidate_id}:{source_updated_at_ms}");
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            tx.execute(
                "INSERT INTO jobs_global_candidate_materializations (
                    account_id, candidate_id, job_id, track_id, source_updated_at_ms,
                    materialized_at_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT(account_id, candidate_id) DO UPDATE SET
                    job_id = excluded.job_id, track_id = excluded.track_id,
                    source_updated_at_ms = excluded.source_updated_at_ms,
                    materialized_at_ms = excluded.materialized_at_ms",
                params![account_id, candidate_id, posting.id, posting.track_id, source_updated_at_ms, now],
            )?;
            tx.execute(
                "INSERT INTO jobs_discovery_memberships (
                    source_id, account_id, external_id, canonical_key, job_id, content_hash,
                    first_seen_at_ms, last_seen_at_ms, last_seen_run_id,
                    availability_status, missing_count
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7, ?8, 'active', 0)
                 ON CONFLICT(source_id, external_id) DO UPDATE SET
                    canonical_key = excluded.canonical_key, job_id = excluded.job_id,
                    content_hash = excluded.content_hash, last_seen_at_ms = excluded.last_seen_at_ms,
                    last_seen_run_id = excluded.last_seen_run_id, availability_status = 'active',
                    missing_count = 0, missing_since_at_ms = NULL",
                params![source_id, account_id, external_id, posting.canonical_key, posting.id, content_hash, now, run_id],
            )?;
            tx.commit()?;
            Ok(())
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            tx.execute(
                "INSERT INTO jobs_global_candidate_materializations (
                    account_id, candidate_id, job_id, track_id, source_updated_at_ms,
                    materialized_at_ms
                 ) VALUES ($1, $2, $3, $4, $5, $6)
                 ON CONFLICT(account_id, candidate_id) DO UPDATE SET
                    job_id = excluded.job_id, track_id = excluded.track_id,
                    source_updated_at_ms = excluded.source_updated_at_ms,
                    materialized_at_ms = excluded.materialized_at_ms",
                &[&account_id, &candidate_id, &posting.id, &posting.track_id, &source_updated_at_ms, &now],
            )?;
            tx.execute(
                "INSERT INTO jobs_discovery_memberships (
                    source_id, account_id, external_id, canonical_key, job_id, content_hash,
                    first_seen_at_ms, last_seen_at_ms, last_seen_run_id,
                    availability_status, missing_count
                 ) VALUES ($1, $2, $3, $4, $5, $6, $7, $7, $8, 'active', 0)
                 ON CONFLICT(source_id, external_id) DO UPDATE SET
                    canonical_key = excluded.canonical_key, job_id = excluded.job_id,
                    content_hash = excluded.content_hash, last_seen_at_ms = excluded.last_seen_at_ms,
                    last_seen_run_id = excluded.last_seen_run_id, availability_status = 'active',
                    missing_count = 0, missing_since_at_ms = NULL",
                &[&source_id, &account_id, &external_id, &posting.canonical_key, &posting.id, &content_hash, &now, &run_id],
            )?;
            tx.commit()?;
            Ok(())
        }
    })
}

fn remove_global_materialization(
    pool: &DbPool,
    account_id: &str,
    source_id: &str,
    candidate_id: &str,
    external_id: &str,
) -> Result<()> {
    let now = now_ms();
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            tx.execute(
                "DELETE FROM jobs_global_candidate_materializations
                  WHERE account_id = ?1 AND candidate_id = ?2",
                params![account_id, candidate_id],
            )?;
            tx.execute(
                "UPDATE jobs_discovery_memberships
                    SET availability_status = 'expired', missing_count = MAX(missing_count, 2),
                        missing_since_at_ms = COALESCE(missing_since_at_ms, ?4)
                  WHERE source_id = ?1 AND account_id = ?2 AND external_id = ?3",
                params![source_id, account_id, external_id, now],
            )?;
            tx.commit()?;
            Ok(())
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            tx.execute(
                "DELETE FROM jobs_global_candidate_materializations
                  WHERE account_id = $1 AND candidate_id = $2",
                &[&account_id, &candidate_id],
            )?;
            tx.execute(
                "UPDATE jobs_discovery_memberships
                    SET availability_status = 'expired', missing_count = GREATEST(missing_count, 2),
                        missing_since_at_ms = COALESCE(missing_since_at_ms, $4)
                  WHERE source_id = $1 AND account_id = $2 AND external_id = $3",
                &[&source_id, &account_id, &external_id, &now],
            )?;
            tx.commit()?;
            Ok(())
        }
    })
}

fn save_global_materialization_state(
    pool: &DbPool,
    account_id: &str,
    global_updated_at_ms: i64,
    profile_revision_at_ms: i64,
) -> Result<()> {
    let now = now_ms();
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            pool.get()?.execute(
                "INSERT INTO jobs_global_materialization_state (
                    account_id, global_updated_at_ms, profile_revision_at_ms, last_run_at_ms
                 ) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(account_id) DO UPDATE SET
                    global_updated_at_ms = excluded.global_updated_at_ms,
                    profile_revision_at_ms = excluded.profile_revision_at_ms,
                    last_run_at_ms = excluded.last_run_at_ms",
                params![account_id, global_updated_at_ms, profile_revision_at_ms, now],
            )?;
            Ok(())
        }
        DbPool::Postgres(_) => {
            pool.get_pg()?.execute(
                "INSERT INTO jobs_global_materialization_state (
                    account_id, global_updated_at_ms, profile_revision_at_ms, last_run_at_ms
                 ) VALUES ($1, $2, $3, $4)
                 ON CONFLICT(account_id) DO UPDATE SET
                    global_updated_at_ms = excluded.global_updated_at_ms,
                    profile_revision_at_ms = excluded.profile_revision_at_ms,
                    last_run_at_ms = excluded.last_run_at_ms",
                &[&account_id, &global_updated_at_ms, &profile_revision_at_ms, &now],
            )?;
            Ok(())
        }
    })
}
