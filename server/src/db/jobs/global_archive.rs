#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlobalCandidateArchiveLease {
    pub candidate_id: String,
    pub canonical_key: String,
    pub content_hash: String,
    pub candidate_json: String,
    pub lease_owner: String,
}

#[derive(Debug)]
struct GlobalCandidateArchiveClaim {
    candidate_id: String,
    canonical_key: String,
    content_hash: String,
    candidate_json: String,
    attempt_count: i64,
}

fn global_candidate_content_hash(candidate_json: &str) -> Result<String> {
    let plaintext = decrypt_payload(candidate_json).context("decrypt global job candidate")?;
    serde_json::from_str::<DiscoveredJobInput>(&plaintext)
        .context("validate global job candidate")?;
    Ok(hex::encode(Sha256::digest(plaintext.as_bytes())))
}

fn validate_global_archive_request(owner: &str, limit: usize) -> Result<()> {
    if owner.trim().is_empty()
        || owner.chars().count() > 160
        || owner.bytes().any(|byte| byte.is_ascii_control())
    {
        anyhow::bail!("global candidate archive worker ID is invalid")
    }
    if limit == 0 || limit > 250 {
        anyhow::bail!("global candidate archive batch size is invalid")
    }
    Ok(())
}

fn global_archive_retry_delay_ms(attempt_count: i64) -> i64 {
    let exponent = attempt_count.clamp(0, 10) as u32;
    60_000_i64
        .saturating_mul(1_i64 << exponent)
        .min(24 * 60 * 60 * 1_000)
}

fn validate_global_archive_result(
    storage_key: &str,
    archive_sha256: &str,
    archive_size_bytes: i64,
) -> Result<()> {
    if storage_key.trim().is_empty()
        || storage_key.chars().count() > 1_024
        || storage_key.bytes().any(|byte| byte.is_ascii_control())
    {
        anyhow::bail!("global candidate archive storage key is invalid")
    }
    if archive_sha256.len() != 64
        || !archive_sha256
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        anyhow::bail!("global candidate archive SHA-256 is invalid")
    }
    if archive_size_bytes <= 0 {
        anyhow::bail!("global candidate archive size is invalid")
    }
    Ok(())
}

pub fn claim_global_candidate_archive_jobs(
    pool: &DbPool,
    owner: &str,
    now: i64,
    stale_before_ms: i64,
    lease_ms: i64,
    limit: usize,
) -> Result<Vec<GlobalCandidateArchiveLease>> {
    validate_global_archive_request(owner, limit)?;
    let owner = owner.trim();
    let lease_expires_at_ms = now.saturating_add(lease_ms.max(1));
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let mut jobs = Vec::new();
            {
                let mut statement = tx.prepare(
                    "SELECT candidate.id, candidate.canonical_key, candidate.content_hash,
                            candidate.candidate_json, candidate.archive_attempt_count
                       FROM jobs_global_candidates candidate
                      WHERE candidate.availability_status = 'expired'
                        AND candidate.updated_at_ms <= ?1
                        AND (
                            (candidate.archive_state IN ('hot', 'retry')
                             AND candidate.archive_next_attempt_at_ms <= ?2)
                            OR
                            (candidate.archive_state = 'archiving'
                             AND COALESCE(candidate.archive_lease_expires_at_ms, 0) <= ?2)
                        )
                        AND NOT EXISTS (
                            SELECT 1 FROM jobs_global_candidate_memberships membership
                             WHERE membership.candidate_id = candidate.id
                               AND membership.availability_status <> 'expired'
                        )
                        AND NOT EXISTS (
                            SELECT 1 FROM jobs_global_candidate_materializations materialization
                             WHERE materialization.candidate_id = candidate.id
                        )
                      ORDER BY candidate.updated_at_ms, candidate.id
                      LIMIT ?3",
                )?;
                let rows = statement.query_map(
                    params![
                        stale_before_ms,
                        now,
                        i64::try_from(limit).unwrap_or(i64::MAX)
                    ],
                    |row| {
                        Ok(GlobalCandidateArchiveClaim {
                            candidate_id: row.get(0)?,
                            canonical_key: row.get(1)?,
                            content_hash: row.get(2)?,
                            candidate_json: row.get(3)?,
                            attempt_count: row.get(4)?,
                        })
                    },
                )?;
                for row in rows {
                    jobs.push(row?);
                }
            }
            let mut claimed = Vec::with_capacity(jobs.len());
            for mut job in jobs {
                if job.content_hash.is_empty() {
                    let content_hash = match global_candidate_content_hash(&job.candidate_json) {
                        Ok(content_hash) => content_hash,
                        Err(error) => {
                            let next_attempt_at_ms = now.saturating_add(
                                global_archive_retry_delay_ms(job.attempt_count),
                            );
                            tx.execute(
                                "UPDATE jobs_global_candidates
                                    SET archive_state = 'retry',
                                        archive_attempt_count = archive_attempt_count + 1,
                                        archive_next_attempt_at_ms = ?1,
                                        archive_lease_owner = NULL,
                                        archive_lease_expires_at_ms = NULL
                                  WHERE id = ?2 AND content_hash = ''
                                    AND availability_status = 'expired'",
                                params![next_attempt_at_ms, job.candidate_id],
                            )?;
                            tracing::warn!(
                                candidate_id = %job.candidate_id,
                                error = %error,
                                "legacy global candidate hash hydration failed"
                            );
                            continue;
                        }
                    };
                    job.content_hash = content_hash;
                }
                let changed = tx.execute(
                    "UPDATE jobs_global_candidates
                        SET content_hash = ?1, archive_state = 'archiving',
                            archive_lease_owner = ?2, archive_lease_expires_at_ms = ?3
                      WHERE id = ?4 AND content_hash IN ('', ?1)
                        AND availability_status = 'expired'
                        AND (
                            (archive_state IN ('hot', 'retry')
                             AND archive_next_attempt_at_ms <= ?5)
                            OR
                            (archive_state = 'archiving'
                             AND COALESCE(archive_lease_expires_at_ms, 0) <= ?5)
                        )
                        AND NOT EXISTS (
                            SELECT 1 FROM jobs_global_candidate_memberships membership
                             WHERE membership.candidate_id = jobs_global_candidates.id
                               AND membership.availability_status <> 'expired'
                        )
                        AND NOT EXISTS (
                            SELECT 1 FROM jobs_global_candidate_materializations materialization
                             WHERE materialization.candidate_id = jobs_global_candidates.id
                    )",
                    params![
                        job.content_hash,
                        owner,
                        lease_expires_at_ms,
                        job.candidate_id,
                        now
                    ],
                )?;
                if changed == 1 {
                    claimed.push(GlobalCandidateArchiveLease {
                        candidate_id: job.candidate_id,
                        canonical_key: job.canonical_key,
                        content_hash: job.content_hash,
                        candidate_json: job.candidate_json,
                        lease_owner: owner.to_string(),
                    });
                }
            }
            tx.commit()?;
            Ok(claimed)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            let rows = tx.query(
                "SELECT candidate.id, candidate.canonical_key, candidate.content_hash,
                        candidate.candidate_json, candidate.archive_attempt_count
                   FROM jobs_global_candidates candidate
                  WHERE candidate.availability_status = 'expired'
                    AND candidate.updated_at_ms <= $1
                    AND (
                        (candidate.archive_state IN ('hot', 'retry')
                         AND candidate.archive_next_attempt_at_ms <= $2)
                        OR
                        (candidate.archive_state = 'archiving'
                         AND COALESCE(candidate.archive_lease_expires_at_ms, 0) <= $2)
                    )
                    AND NOT EXISTS (
                        SELECT 1 FROM jobs_global_candidate_memberships membership
                         WHERE membership.candidate_id = candidate.id
                           AND membership.availability_status <> 'expired'
                    )
                    AND NOT EXISTS (
                        SELECT 1 FROM jobs_global_candidate_materializations materialization
                         WHERE materialization.candidate_id = candidate.id
                    )
                  ORDER BY candidate.updated_at_ms, candidate.id
                  LIMIT $3
                  FOR UPDATE SKIP LOCKED",
                &[
                    &stale_before_ms,
                    &now,
                    &i64::try_from(limit).unwrap_or(i64::MAX),
                ],
            )?;
            let jobs: Vec<GlobalCandidateArchiveClaim> = rows
                .into_iter()
                .map(|row| GlobalCandidateArchiveClaim {
                    candidate_id: row.get(0),
                    canonical_key: row.get(1),
                    content_hash: row.get(2),
                    candidate_json: row.get(3),
                    attempt_count: row.get(4),
                })
                .collect();
            let mut claimed = Vec::with_capacity(jobs.len());
            for mut job in jobs {
                if job.content_hash.is_empty() {
                    let content_hash = match global_candidate_content_hash(&job.candidate_json) {
                        Ok(content_hash) => content_hash,
                        Err(error) => {
                            let next_attempt_at_ms = now.saturating_add(
                                global_archive_retry_delay_ms(job.attempt_count),
                            );
                            tx.execute(
                                "UPDATE jobs_global_candidates
                                    SET archive_state = 'retry',
                                        archive_attempt_count = archive_attempt_count + 1,
                                        archive_next_attempt_at_ms = $1,
                                        archive_lease_owner = NULL,
                                        archive_lease_expires_at_ms = NULL
                                  WHERE id = $2 AND content_hash = ''
                                    AND availability_status = 'expired'",
                                &[&next_attempt_at_ms, &job.candidate_id],
                            )?;
                            tracing::warn!(
                                candidate_id = %job.candidate_id,
                                error = %error,
                                "legacy global candidate hash hydration failed"
                            );
                            continue;
                        }
                    };
                    job.content_hash = content_hash;
                }
                let changed = tx.execute(
                    "UPDATE jobs_global_candidates candidate
                        SET content_hash = $1, archive_state = 'archiving',
                            archive_lease_owner = $2, archive_lease_expires_at_ms = $3
                      WHERE candidate.id = $4
                        AND candidate.content_hash IN ('', $1)
                        AND candidate.availability_status = 'expired'
                        AND (
                            (candidate.archive_state IN ('hot', 'retry')
                             AND candidate.archive_next_attempt_at_ms <= $5)
                            OR
                            (candidate.archive_state = 'archiving'
                             AND COALESCE(candidate.archive_lease_expires_at_ms, 0) <= $5)
                        )
                        AND NOT EXISTS (
                            SELECT 1 FROM jobs_global_candidate_memberships membership
                             WHERE membership.candidate_id = candidate.id
                               AND membership.availability_status <> 'expired'
                        )
                        AND NOT EXISTS (
                            SELECT 1 FROM jobs_global_candidate_materializations materialization
                             WHERE materialization.candidate_id = candidate.id
                        )",
                    &[
                        &job.content_hash,
                        &owner,
                        &lease_expires_at_ms,
                        &job.candidate_id,
                        &now,
                    ],
                )?;
                if changed == 1 {
                    claimed.push(GlobalCandidateArchiveLease {
                        candidate_id: job.candidate_id,
                        canonical_key: job.canonical_key,
                        content_hash: job.content_hash,
                        candidate_json: job.candidate_json,
                        lease_owner: owner.to_string(),
                    });
                }
            }
            tx.commit()?;
            Ok(claimed)
        }
    })
}

#[allow(clippy::too_many_arguments)]
pub fn complete_global_candidate_archive(
    pool: &DbPool,
    lease: &GlobalCandidateArchiveLease,
    storage_key: &str,
    archive_sha256: &str,
    archive_size_bytes: i64,
    now: i64,
) -> Result<bool> {
    validate_global_archive_result(storage_key, archive_sha256, archive_size_bytes)?;
    let mut tombstone: DiscoveredJobInput =
        parse_json(lease.candidate_json.clone(), "global job candidate")?;
    tombstone.description.clear();
    tombstone.compensation.clear();
    let tombstone_json = to_json(&tombstone, "global job candidate tombstone")?;
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let changed = pool.get()?.execute(
                "UPDATE jobs_global_candidates
                    SET candidate_json = ?1, archive_state = 'archived',
                        archive_storage_key = ?2, archive_sha256 = ?3,
                        archive_size_bytes = ?4, archived_at_ms = ?5,
                        archive_attempt_count = 0, archive_next_attempt_at_ms = 0,
                        archive_lease_owner = NULL, archive_lease_expires_at_ms = NULL
                  WHERE id = ?6 AND content_hash = ?7 AND archive_state = 'archiving'
                    AND archive_lease_owner = ?8
                    AND availability_status = 'expired'
                    AND NOT EXISTS (
                        SELECT 1 FROM jobs_global_candidate_memberships membership
                         WHERE membership.candidate_id = jobs_global_candidates.id
                           AND membership.availability_status <> 'expired'
                    )
                    AND NOT EXISTS (
                        SELECT 1 FROM jobs_global_candidate_materializations materialization
                         WHERE materialization.candidate_id = jobs_global_candidates.id
                    )",
                params![
                    tombstone_json,
                    storage_key,
                    archive_sha256,
                    archive_size_bytes,
                    now,
                    lease.candidate_id,
                    lease.content_hash,
                    lease.lease_owner,
                ],
            )?;
            Ok(changed == 1)
        }
        DbPool::Postgres(_) => {
            let changed = pool.get_pg()?.execute(
                "UPDATE jobs_global_candidates candidate
                    SET candidate_json = $1, archive_state = 'archived',
                        archive_storage_key = $2, archive_sha256 = $3,
                        archive_size_bytes = $4, archived_at_ms = $5,
                        archive_attempt_count = 0, archive_next_attempt_at_ms = 0,
                        archive_lease_owner = NULL, archive_lease_expires_at_ms = NULL
                  WHERE candidate.id = $6 AND candidate.content_hash = $7
                    AND candidate.archive_state = 'archiving'
                    AND candidate.archive_lease_owner = $8
                    AND candidate.availability_status = 'expired'
                    AND NOT EXISTS (
                        SELECT 1 FROM jobs_global_candidate_memberships membership
                         WHERE membership.candidate_id = candidate.id
                           AND membership.availability_status <> 'expired'
                    )
                    AND NOT EXISTS (
                        SELECT 1 FROM jobs_global_candidate_materializations materialization
                         WHERE materialization.candidate_id = candidate.id
                    )",
                &[
                    &tombstone_json,
                    &storage_key,
                    &archive_sha256,
                    &archive_size_bytes,
                    &now,
                    &lease.candidate_id,
                    &lease.content_hash,
                    &lease.lease_owner,
                ],
            )?;
            Ok(changed == 1)
        }
    })
}

pub fn fail_global_candidate_archive(
    pool: &DbPool,
    lease: &GlobalCandidateArchiveLease,
    now: i64,
) -> Result<bool> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let attempt_count = tx
                .query_row(
                    "SELECT archive_attempt_count
                       FROM jobs_global_candidates
                      WHERE id = ?1 AND content_hash = ?2 AND archive_state = 'archiving'
                        AND archive_lease_owner = ?3",
                    params![
                        lease.candidate_id,
                        lease.content_hash,
                        lease.lease_owner
                    ],
                    |row| row.get::<_, i64>(0),
                )
                .optional()?;
            let Some(attempt_count) = attempt_count else {
                tx.commit()?;
                return Ok(false);
            };
            let next_attempt_at_ms =
                now.saturating_add(global_archive_retry_delay_ms(attempt_count));
            let changed = tx.execute(
                "UPDATE jobs_global_candidates
                    SET archive_state = 'retry',
                        archive_attempt_count = archive_attempt_count + 1,
                        archive_next_attempt_at_ms = ?1,
                        archive_lease_owner = NULL, archive_lease_expires_at_ms = NULL
                  WHERE id = ?2 AND content_hash = ?3 AND archive_state = 'archiving'
                    AND archive_lease_owner = ?4",
                params![
                    next_attempt_at_ms,
                    lease.candidate_id,
                    lease.content_hash,
                    lease.lease_owner
                ],
            )?;
            tx.commit()?;
            Ok(changed == 1)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            let attempt_count = tx
                .query_opt(
                    "SELECT archive_attempt_count
                       FROM jobs_global_candidates
                      WHERE id = $1 AND content_hash = $2 AND archive_state = 'archiving'
                        AND archive_lease_owner = $3
                      FOR UPDATE",
                    &[
                        &lease.candidate_id,
                        &lease.content_hash,
                        &lease.lease_owner,
                    ],
                )?
                .map(|row| row.get::<_, i64>(0));
            let Some(attempt_count) = attempt_count else {
                tx.commit()?;
                return Ok(false);
            };
            let next_attempt_at_ms =
                now.saturating_add(global_archive_retry_delay_ms(attempt_count));
            let changed = tx.execute(
                "UPDATE jobs_global_candidates
                    SET archive_state = 'retry',
                        archive_attempt_count = archive_attempt_count + 1,
                        archive_next_attempt_at_ms = $1,
                        archive_lease_owner = NULL, archive_lease_expires_at_ms = NULL
                  WHERE id = $2 AND content_hash = $3 AND archive_state = 'archiving'
                    AND archive_lease_owner = $4",
                &[
                    &next_attempt_at_ms,
                    &lease.candidate_id,
                    &lease.content_hash,
                    &lease.lease_owner,
                ],
            )?;
            tx.commit()?;
            Ok(changed == 1)
        }
    })
}
