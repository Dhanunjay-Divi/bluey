#[derive(Debug, Clone)]
struct NormalizedGlobalCandidate {
    external_id: String,
    canonical_key: String,
    candidate_id: String,
    candidate_json: String,
    content_hash: String,
    company: String,
    title: String,
    location: String,
    workplace: String,
    canonical_url: String,
    role_family: String,
    posted_at_ms: Option<i64>,
}

/// Synchronize one complete manifest of shared candidate-feed sources. Feed
/// rows are stored once globally; this function never writes account jobs.
pub fn sync_global_discovery_sources(
    pool: &DbPool,
    inputs: &[GlobalDiscoverySourceInput],
) -> Result<Vec<GlobalDiscoverySource>> {
    if inputs.is_empty() || inputs.len() > 256 {
        anyhow::bail!("global discovery source snapshot is invalid")
    }
    let mut normalized = Vec::with_capacity(inputs.len());
    let mut identities = BTreeSet::new();
    for input in inputs {
        validate_global_source_input(input)?;
        let identity = format!(
            "{}:{}",
            input.provider.trim().to_ascii_lowercase(),
            input.source_key.trim().to_ascii_lowercase()
        );
        if !identities.insert(identity.clone()) {
            anyhow::bail!("global discovery source snapshot contains duplicates")
        }
        let digest = hex::encode(Sha256::digest(identity.as_bytes()));
        let id = format!("global-source-{}", &digest[..32]);
        let config = json!({
            "sourceFamily": input.source_family.trim().to_ascii_lowercase(),
            "artifactUrl": input.artifact_url.trim(),
            "artifactSha256": input.artifact_sha256.trim().to_ascii_lowercase(),
            "expectedRows": input.expected_rows,
            "snapshotAtMs": input.snapshot_at_ms,
            "requiresOriginalRevalidation": true,
        });
        normalized.push((id, input.clone(), config));
    }
    let now = now_ms();
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            for (id, input, config) in &normalized {
                let provider = input.provider.trim().to_ascii_lowercase();
                let source_key = input.source_key.trim().to_ascii_lowercase();
                let stored_config = tx
                    .query_row(
                        "SELECT source_json FROM jobs_global_discovery_sources
                          WHERE provider = ?1 AND source_key = ?2",
                        params![&provider, &source_key],
                        |row| row.get::<_, String>(0),
                    )
                    .optional()?;
                let config_change = global_source_config_change(stored_config, config)?;
                let payload_changed = config_change.payload_changed as i64;
                let revision_changed = config_change.revision_changed as i64;
                let payload = to_json(config, "global discovery source")?;
                let run_interval_ms = input
                    .run_interval_ms
                    .clamp(DISCOVERY_MIN_INTERVAL_MS, DISCOVERY_MAX_INTERVAL_MS);
                tx.execute(
                    "INSERT INTO jobs_global_discovery_sources (
                        id, provider, source_key, source_json, status, health,
                        run_interval_ms, next_run_at_ms, created_at_ms, updated_at_ms
                     ) VALUES (?1, ?2, ?3, ?4, 'active', 'waiting', ?5, ?6, ?6, ?6)
                     ON CONFLICT(provider, source_key) DO UPDATE SET
                        source_json = CASE
                            WHEN jobs_global_discovery_sources.lease_expires_at_ms IS NULL
                              OR jobs_global_discovery_sources.lease_expires_at_ms <= ?6
                            THEN CASE WHEN ?7 = 1
                                THEN excluded.source_json
                                ELSE jobs_global_discovery_sources.source_json END
                            ELSE jobs_global_discovery_sources.source_json END,
                        status = 'active',
                        run_interval_ms = excluded.run_interval_ms,
                        next_run_at_ms = CASE
                            WHEN (jobs_global_discovery_sources.lease_expires_at_ms IS NULL
                               OR jobs_global_discovery_sources.lease_expires_at_ms <= ?6)
                              AND (?8 = 1 OR jobs_global_discovery_sources.status <> 'active')
                            THEN ?6 ELSE jobs_global_discovery_sources.next_run_at_ms END,
                        updated_at_ms = CASE
                            WHEN ?7 = 1
                              OR jobs_global_discovery_sources.status <> 'active'
                              OR jobs_global_discovery_sources.run_interval_ms <> excluded.run_interval_ms
                            THEN ?6 ELSE jobs_global_discovery_sources.updated_at_ms END",
                    params![
                        id,
                        provider,
                        source_key,
                        payload,
                        run_interval_ms,
                        now,
                        payload_changed,
                        revision_changed
                    ],
                )?;
            }
            let retained = normalized
                .iter()
                .map(|(_, input, _)| input.source_key.trim().to_ascii_lowercase())
                .collect::<Vec<_>>();
            let mut stmt = tx.prepare(
                "SELECT source_key FROM jobs_global_discovery_sources
                  WHERE provider = 'jobhive' AND status = 'active'",
            )?;
            let existing = stmt
                .query_map([], |row| row.get::<_, String>(0))?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            drop(stmt);
            for source_key in existing {
                if !retained.contains(&source_key) {
                    tx.execute(
                        "UPDATE jobs_global_discovery_sources
                            SET status = 'disabled', lease_owner = NULL, lease_token = NULL,
                                lease_expires_at_ms = NULL, updated_at_ms = ?2
                          WHERE provider = 'jobhive' AND source_key = ?1",
                        params![source_key, now],
                    )?;
                }
            }
            tx.commit()?;
            list_global_discovery_sources(pool)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            for (id, input, config) in &normalized {
                let provider = input.provider.trim().to_ascii_lowercase();
                let source_key = input.source_key.trim().to_ascii_lowercase();
                let stored_config = tx
                    .query_opt(
                        "SELECT source_json FROM jobs_global_discovery_sources
                          WHERE provider = $1 AND source_key = $2 FOR UPDATE",
                        &[&provider, &source_key],
                    )?
                    .map(|row| row.get::<_, String>(0));
                let config_change = global_source_config_change(stored_config, config)?;
                let payload = to_json(config, "global discovery source")?;
                let run_interval_ms = input
                    .run_interval_ms
                    .clamp(DISCOVERY_MIN_INTERVAL_MS, DISCOVERY_MAX_INTERVAL_MS);
                tx.execute(
                    "INSERT INTO jobs_global_discovery_sources (
                        id, provider, source_key, source_json, status, health,
                        run_interval_ms, next_run_at_ms, created_at_ms, updated_at_ms
                     ) VALUES ($1, $2, $3, $4, 'active', 'waiting', $5, $6, $6, $6)
                     ON CONFLICT(provider, source_key) DO UPDATE SET
                        source_json = CASE
                            WHEN jobs_global_discovery_sources.lease_expires_at_ms IS NULL
                              OR jobs_global_discovery_sources.lease_expires_at_ms <= $6
                            THEN CASE WHEN $7
                                THEN excluded.source_json
                                ELSE jobs_global_discovery_sources.source_json END
                            ELSE jobs_global_discovery_sources.source_json END,
                        status = 'active',
                        run_interval_ms = excluded.run_interval_ms,
                        next_run_at_ms = CASE
                            WHEN (jobs_global_discovery_sources.lease_expires_at_ms IS NULL
                               OR jobs_global_discovery_sources.lease_expires_at_ms <= $6)
                              AND ($8 OR jobs_global_discovery_sources.status <> 'active')
                            THEN $6 ELSE jobs_global_discovery_sources.next_run_at_ms END,
                        updated_at_ms = CASE
                            WHEN $7
                              OR jobs_global_discovery_sources.status <> 'active'
                              OR jobs_global_discovery_sources.run_interval_ms <> excluded.run_interval_ms
                            THEN $6 ELSE jobs_global_discovery_sources.updated_at_ms END",
                    &[
                        &id,
                        &provider,
                        &source_key,
                        &payload,
                        &run_interval_ms,
                        &now,
                        &config_change.payload_changed,
                        &config_change.revision_changed,
                    ],
                )?;
            }
            let retained = normalized
                .iter()
                .map(|(_, input, _)| input.source_key.trim().to_ascii_lowercase())
                .collect::<Vec<_>>();
            for row in tx.query(
                "SELECT source_key FROM jobs_global_discovery_sources
                  WHERE provider = 'jobhive' AND status = 'active' FOR UPDATE",
                &[],
            )? {
                let source_key = row.get::<_, String>(0);
                if !retained.contains(&source_key) {
                    tx.execute(
                        "UPDATE jobs_global_discovery_sources
                            SET status = 'disabled', lease_owner = NULL, lease_token = NULL,
                                lease_expires_at_ms = NULL, updated_at_ms = $2
                          WHERE provider = 'jobhive' AND source_key = $1",
                        &[&source_key, &now],
                    )?;
                }
            }
            tx.commit()?;
            list_global_discovery_sources(pool)
        }
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct GlobalSourceConfigChange {
    payload_changed: bool,
    revision_changed: bool,
}

fn global_source_config_change(
    stored: Option<String>,
    incoming: &Value,
) -> Result<GlobalSourceConfigChange> {
    let Some(stored) = stored else {
        return Ok(GlobalSourceConfigChange {
            payload_changed: true,
            revision_changed: true,
        });
    };
    let existing: Value = parse_json(stored, "global discovery source")?;
    Ok(GlobalSourceConfigChange {
        payload_changed: existing != *incoming,
        revision_changed: global_source_revision(&existing) != global_source_revision(incoming),
    })
}

fn global_source_revision(config: &Value) -> (&Value, &Value, &Value) {
    (
        &config["sourceFamily"],
        &config["artifactSha256"],
        &config["expectedRows"],
    )
}

pub fn list_global_discovery_sources(pool: &DbPool) -> Result<Vec<GlobalDiscoverySource>> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get()?;
            let mut stmt = conn.prepare(
                "SELECT id, provider, source_key, source_json, status, health,
                        consecutive_failures, run_interval_ms, next_run_at_ms,
                        last_success_at_ms, last_failure_at_ms, last_error_code,
                        lease_expires_at_ms, created_at_ms, updated_at_ms
                   FROM jobs_global_discovery_sources ORDER BY provider, source_key",
            )?;
            let sources = stmt.query_map([], global_source_from_sqlite_row)?
                .collect::<std::result::Result<Vec<_>, _>>()
                .map_err(Into::into);
            sources
        }
        DbPool::Postgres(_) => pool
            .get_pg()?
            .query(
                "SELECT id, provider, source_key, source_json, status, health,
                        consecutive_failures, run_interval_ms, next_run_at_ms,
                        last_success_at_ms, last_failure_at_ms, last_error_code,
                        lease_expires_at_ms, created_at_ms, updated_at_ms
                   FROM jobs_global_discovery_sources ORDER BY provider, source_key",
                &[],
            )?
            .into_iter()
            .map(global_source_from_pg_row)
            .collect(),
    })
}

pub fn lease_due_global_discovery_source(
    pool: &DbPool,
    worker_id: &str,
) -> Result<Option<GlobalDiscoverySourceLease>> {
    if worker_id.trim().is_empty() || worker_id.chars().count() > 160 {
        anyhow::bail!("global discovery worker ID is invalid")
    }
    let now = now_ms();
    let lease_expires = now.saturating_add(GLOBAL_DISCOVERY_LEASE_MS);
    let mut token_bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut token_bytes);
    let lease_token = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(token_bytes);
    let token_hash = hex::encode(Sha256::digest(lease_token.as_bytes()));
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let raw = tx
                .query_row(
                    "SELECT id, provider, source_key, source_json, status, health,
                            consecutive_failures, run_interval_ms, next_run_at_ms,
                            last_success_at_ms, last_failure_at_ms, last_error_code,
                            lease_expires_at_ms, created_at_ms, updated_at_ms
                       FROM jobs_global_discovery_sources
                      WHERE status = 'active' AND next_run_at_ms <= ?1
                        AND (lease_expires_at_ms IS NULL OR lease_expires_at_ms <= ?1)
                      ORDER BY next_run_at_ms, id LIMIT 1",
                    params![now],
                    global_source_from_sqlite_row,
                )
                .optional()?;
            let Some(source) = raw else {
                tx.commit()?;
                return Ok(None);
            };
            let changed = tx.execute(
                "UPDATE jobs_global_discovery_sources
                    SET lease_owner = ?2, lease_token = ?3, lease_expires_at_ms = ?4,
                        health = 'running', updated_at_ms = ?1
                  WHERE id = ?5 AND status = 'active' AND next_run_at_ms = ?6
                    AND (lease_expires_at_ms IS NULL OR lease_expires_at_ms <= ?1)",
                params![
                    now,
                    worker_id.trim(),
                    token_hash,
                    lease_expires,
                    source.id,
                    source.next_run_at_ms,
                ],
            )?;
            if changed != 1 {
                tx.commit()?;
                return Ok(None);
            }
            let replay_key = global_replay_key(&source);
            ensure_global_run_sqlite(&tx, &source, &replay_key, now)?;
            tx.commit()?;
            Ok(Some(GlobalDiscoverySourceLease {
                scheduled_for_ms: source.next_run_at_ms,
                source,
                lease_token,
                replay_key,
            }))
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            let row = tx.query_opt(
                "SELECT id, provider, source_key, source_json, status, health,
                        consecutive_failures, run_interval_ms, next_run_at_ms,
                        last_success_at_ms, last_failure_at_ms, last_error_code,
                        lease_expires_at_ms, created_at_ms, updated_at_ms
                   FROM jobs_global_discovery_sources
                  WHERE status = 'active' AND next_run_at_ms <= $1
                    AND (lease_expires_at_ms IS NULL OR lease_expires_at_ms <= $1)
                  ORDER BY next_run_at_ms, id LIMIT 1 FOR UPDATE SKIP LOCKED",
                &[&now],
            )?;
            let Some(row) = row else {
                tx.commit()?;
                return Ok(None);
            };
            let source = global_source_from_pg_row(row)?;
            let changed = tx.execute(
                "UPDATE jobs_global_discovery_sources
                    SET lease_owner = $2, lease_token = $3, lease_expires_at_ms = $4,
                        health = 'running', updated_at_ms = $1
                  WHERE id = $5 AND status = 'active' AND next_run_at_ms = $6
                    AND (lease_expires_at_ms IS NULL OR lease_expires_at_ms <= $1)",
                &[
                    &now,
                    &worker_id.trim(),
                    &token_hash,
                    &lease_expires,
                    &source.id,
                    &source.next_run_at_ms,
                ],
            )?;
            if changed != 1 {
                tx.commit()?;
                return Ok(None);
            }
            let replay_key = global_replay_key(&source);
            ensure_global_run_postgres(&mut tx, &source, &replay_key, now)?;
            tx.commit()?;
            Ok(Some(GlobalDiscoverySourceLease {
                scheduled_for_ms: source.next_run_at_ms,
                source,
                lease_token,
                replay_key,
            }))
        }
    })
}

pub fn ingest_global_discovery_batch(
    pool: &DbPool,
    source_id: &str,
    input: &GlobalIngestionBatchInput,
) -> Result<GlobalIngestionBatchResult> {
    if source_id.trim().is_empty()
        || input.batch_index < 0
        || input.jobs.is_empty()
        || input.jobs.len() > GLOBAL_DISCOVERY_MAX_BATCH_ROWS
    {
        anyhow::bail!("global discovery batch is invalid")
    }
    let artifact_sha256 = normalized_sha256(&input.artifact_sha256)?;
    let payload = serde_json::to_vec(&input.jobs).context("serialize global discovery batch")?;
    let payload_sha256 = hex::encode(Sha256::digest(payload));
    let now = now_ms();
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let source = validate_global_lease_sqlite(
                &tx,
                source_id,
                &input.lease_token,
                &input.replay_key,
                input.scheduled_for_ms,
                &artifact_sha256,
                now,
            )?;
            let run_id = global_run_id(source_id, &input.replay_key);
            if let Some((stored_hash, row_count)) = tx
                .query_row(
                    "SELECT payload_sha256, row_count FROM jobs_global_ingestion_batches
                      WHERE run_id = ?1 AND batch_index = ?2",
                    params![run_id, input.batch_index],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
                )
                .optional()?
            {
                if stored_hash != payload_sha256 || row_count != input.jobs.len() as i64 {
                    anyhow::bail!("global discovery batch replay conflicts with stored payload")
                }
                let (received_rows, received_batches): (i64, i64) = tx.query_row(
                    "SELECT received_rows, received_batches FROM jobs_global_ingestion_runs
                      WHERE id = ?1",
                    params![run_id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )?;
                tx.commit()?;
                return Ok(GlobalIngestionBatchResult {
                    run_id,
                    batch_index: input.batch_index,
                    row_count,
                    received_rows,
                    received_batches,
                    replayed: true,
                });
            }
            let family = global_source_family(&source)?;
            for job in &input.jobs {
                let candidate = normalize_global_candidate(&source.provider, &family, job)?;
                upsert_global_candidate_sqlite(&tx, &source.id, &run_id, &candidate, now)?;
            }
            let row_count = input.jobs.len() as i64;
            tx.execute(
                "INSERT INTO jobs_global_ingestion_batches
                    (run_id, batch_index, payload_sha256, row_count, created_at_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![run_id, input.batch_index, payload_sha256, row_count, now],
            )?;
            tx.execute(
                "UPDATE jobs_global_ingestion_runs
                    SET received_rows = received_rows + ?2,
                        received_batches = received_batches + 1
                  WHERE id = ?1 AND status = 'running'",
                params![run_id, row_count],
            )?;
            let (received_rows, received_batches): (i64, i64) = tx.query_row(
                "SELECT received_rows, received_batches FROM jobs_global_ingestion_runs
                  WHERE id = ?1",
                params![run_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?;
            tx.commit()?;
            Ok(GlobalIngestionBatchResult {
                run_id,
                batch_index: input.batch_index,
                row_count,
                received_rows,
                received_batches,
                replayed: false,
            })
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            let source = validate_global_lease_postgres(
                &mut tx,
                source_id,
                &input.lease_token,
                &input.replay_key,
                input.scheduled_for_ms,
                &artifact_sha256,
                now,
            )?;
            let run_id = global_run_id(source_id, &input.replay_key);
            if let Some(row) = tx.query_opt(
                "SELECT payload_sha256, row_count FROM jobs_global_ingestion_batches
                  WHERE run_id = $1 AND batch_index = $2",
                &[&run_id, &input.batch_index],
            )? {
                let stored_hash = row.get::<_, String>(0);
                let row_count = row.get::<_, i64>(1);
                if stored_hash != payload_sha256 || row_count != input.jobs.len() as i64 {
                    anyhow::bail!("global discovery batch replay conflicts with stored payload")
                }
                let counters = tx.query_one(
                    "SELECT received_rows, received_batches FROM jobs_global_ingestion_runs
                      WHERE id = $1",
                    &[&run_id],
                )?;
                let result = GlobalIngestionBatchResult {
                    run_id,
                    batch_index: input.batch_index,
                    row_count,
                    received_rows: counters.get(0),
                    received_batches: counters.get(1),
                    replayed: true,
                };
                tx.commit()?;
                return Ok(result);
            }
            let family = global_source_family(&source)?;
            for job in &input.jobs {
                let candidate = normalize_global_candidate(&source.provider, &family, job)?;
                upsert_global_candidate_postgres(&mut tx, &source.id, &run_id, &candidate, now)?;
            }
            let row_count = input.jobs.len() as i64;
            tx.execute(
                "INSERT INTO jobs_global_ingestion_batches
                    (run_id, batch_index, payload_sha256, row_count, created_at_ms)
                 VALUES ($1, $2, $3, $4, $5)",
                &[&run_id, &input.batch_index, &payload_sha256, &row_count, &now],
            )?;
            let counters = tx.query_one(
                "UPDATE jobs_global_ingestion_runs
                    SET received_rows = received_rows + $2,
                        received_batches = received_batches + 1
                  WHERE id = $1 AND status = 'running'
                  RETURNING received_rows, received_batches",
                &[&run_id, &row_count],
            )?;
            let result = GlobalIngestionBatchResult {
                run_id,
                batch_index: input.batch_index,
                row_count,
                received_rows: counters.get(0),
                received_batches: counters.get(1),
                replayed: false,
            };
            tx.commit()?;
            Ok(result)
        }
    })
}

fn validate_global_source_input(input: &GlobalDiscoverySourceInput) -> Result<()> {
    let provider = input.provider.trim().to_ascii_lowercase();
    let source_key = input.source_key.trim();
    let family = input.source_family.trim();
    if provider != "jobhive"
        || source_key.is_empty()
        || source_key.chars().count() > 200
        || family.is_empty()
        || family.chars().count() > 120
        || !source_key
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
        || !family
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
    {
        anyhow::bail!("global discovery source identity is invalid")
    }
    normalized_sha256(&input.artifact_sha256)?;
    let url = reqwest::Url::parse(input.artifact_url.trim())
        .context("parse global discovery artifact URL")?;
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
        || url.host_str().is_none()
    {
        anyhow::bail!("global discovery artifact must use public default-port HTTPS")
    }
    let host = url.host_str().unwrap_or_default().to_ascii_lowercase();
    if host != "storage.stapply.ai" {
        anyhow::bail!("global discovery artifact host is not allowlisted")
    }
    let now = now_ms();
    if input.expected_rows <= 0
        || input.expected_rows > 5_000_000
        || input.snapshot_at_ms <= 0
        || input.snapshot_at_ms > now.saturating_add(10 * 60 * 1_000)
        || input.run_interval_ms < DISCOVERY_MIN_INTERVAL_MS
        || input.run_interval_ms > DISCOVERY_MAX_INTERVAL_MS
    {
        anyhow::bail!("global discovery source schedule is invalid")
    }
    Ok(())
}

fn normalized_sha256(value: &str) -> Result<String> {
    let value = value.trim().to_ascii_lowercase();
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        anyhow::bail!("artifact checksum must be SHA-256")
    }
    Ok(value)
}

fn global_source_from_sqlite_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<GlobalDiscoverySource> {
    let config: String = row.get(3)?;
    Ok(GlobalDiscoverySource {
        id: row.get(0)?,
        provider: row.get(1)?,
        source_key: row.get(2)?,
        config: parse_json_lossy(&config).unwrap_or_else(|| json!({})),
        status: row.get(4)?,
        health: row.get(5)?,
        consecutive_failures: row.get(6)?,
        run_interval_ms: row.get(7)?,
        next_run_at_ms: row.get(8)?,
        last_success_at_ms: row.get(9)?,
        last_failure_at_ms: row.get(10)?,
        last_error_code: row.get(11)?,
        lease_expires_at_ms: row.get(12)?,
        created_at_ms: row.get(13)?,
        updated_at_ms: row.get(14)?,
    })
}

fn global_source_from_pg_row(row: postgres::Row) -> Result<GlobalDiscoverySource> {
    let config: String = row.get(3);
    Ok(GlobalDiscoverySource {
        id: row.get(0),
        provider: row.get(1),
        source_key: row.get(2),
        config: parse_json(config, "global discovery source")?,
        status: row.get(4),
        health: row.get(5),
        consecutive_failures: row.get(6),
        run_interval_ms: row.get(7),
        next_run_at_ms: row.get(8),
        last_success_at_ms: row.get(9),
        last_failure_at_ms: row.get(10),
        last_error_code: row.get(11),
        lease_expires_at_ms: row.get(12),
        created_at_ms: row.get(13),
        updated_at_ms: row.get(14),
    })
}

fn global_source_family(source: &GlobalDiscoverySource) -> Result<String> {
    source
        .config
        .get("sourceFamily")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .context("global discovery source has no source family")
}

fn global_artifact_sha256(source: &GlobalDiscoverySource) -> Result<String> {
    let raw = source
        .config
        .get("artifactSha256")
        .and_then(Value::as_str)
        .context("global discovery source has no artifact checksum")?;
    normalized_sha256(raw)
}

fn global_expected_rows(source: &GlobalDiscoverySource) -> Result<i64> {
    source
        .config
        .get("expectedRows")
        .and_then(Value::as_i64)
        .filter(|value| *value > 0 && *value <= 5_000_000)
        .context("global discovery source has an invalid row count")
}

fn global_replay_key(source: &GlobalDiscoverySource) -> String {
    let artifact = source
        .config
        .get("artifactSha256")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let snapshot = source
        .config
        .get("snapshotAtMs")
        .and_then(Value::as_i64)
        .unwrap_or_default();
    let value = format!("{}|{}|{}|{}", source.id, artifact, snapshot, source.next_run_at_ms);
    hex::encode(Sha256::digest(value.as_bytes()))
}

fn global_run_id(source_id: &str, replay_key: &str) -> String {
    let digest = hex::encode(Sha256::digest(format!("{source_id}|{replay_key}").as_bytes()));
    format!("global-run-{}", &digest[..32])
}

fn ensure_global_run_sqlite(
    tx: &rusqlite::Transaction<'_>,
    source: &GlobalDiscoverySource,
    replay_key: &str,
    now: i64,
) -> Result<()> {
    let id = global_run_id(&source.id, replay_key);
    let expected_rows = global_expected_rows(source)?;
    let artifact = global_artifact_sha256(source)?;
    tx.execute(
        "INSERT INTO jobs_global_ingestion_runs
            (id, source_id, replay_key, status, expected_rows, artifact_sha256, started_at_ms)
         VALUES (?1, ?2, ?3, 'running', ?4, ?5, ?6)
         ON CONFLICT(source_id, replay_key) DO NOTHING",
        params![id, source.id, replay_key, expected_rows, artifact, now],
    )?;
    let stored: (String, i64, String) = tx.query_row(
        "SELECT status, expected_rows, artifact_sha256
           FROM jobs_global_ingestion_runs WHERE id = ?1",
        params![id],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?;
    if stored.0 == "completed" || stored.1 != expected_rows || stored.2 != artifact {
        anyhow::bail!("global discovery run cannot be leased with conflicting evidence")
    }
    Ok(())
}

fn ensure_global_run_postgres(
    tx: &mut postgres::Transaction<'_>,
    source: &GlobalDiscoverySource,
    replay_key: &str,
    now: i64,
) -> Result<()> {
    let id = global_run_id(&source.id, replay_key);
    let expected_rows = global_expected_rows(source)?;
    let artifact = global_artifact_sha256(source)?;
    tx.execute(
        "INSERT INTO jobs_global_ingestion_runs
            (id, source_id, replay_key, status, expected_rows, artifact_sha256, started_at_ms)
         VALUES ($1, $2, $3, 'running', $4, $5, $6)
         ON CONFLICT(source_id, replay_key) DO NOTHING",
        &[&id, &source.id, &replay_key, &expected_rows, &artifact, &now],
    )?;
    let row = tx.query_one(
        "SELECT status, expected_rows, artifact_sha256
           FROM jobs_global_ingestion_runs WHERE id = $1",
        &[&id],
    )?;
    let status = row.get::<_, String>(0);
    if status == "completed"
        || row.get::<_, i64>(1) != expected_rows
        || row.get::<_, String>(2) != artifact
    {
        anyhow::bail!("global discovery run cannot be leased with conflicting evidence")
    }
    Ok(())
}

fn validate_global_lease_sqlite(
    tx: &rusqlite::Transaction<'_>,
    source_id: &str,
    lease_token: &str,
    replay_key: &str,
    scheduled_for_ms: i64,
    artifact_sha256: &str,
    now: i64,
) -> Result<GlobalDiscoverySource> {
    let row = tx
        .query_row(
            "SELECT id, provider, source_key, source_json, status, health,
                    consecutive_failures, run_interval_ms, next_run_at_ms,
                    last_success_at_ms, last_failure_at_ms, last_error_code,
                    lease_expires_at_ms, created_at_ms, updated_at_ms, lease_token
               FROM jobs_global_discovery_sources WHERE id = ?1",
            params![source_id],
            |row| {
                let source = global_source_from_sqlite_row(row)?;
                let token: Option<String> = row.get(15)?;
                Ok((source, token))
            },
        )
        .optional()?
        .context("global discovery source is not leased")?;
    validate_global_lease_values(
        &row.0,
        row.1.as_deref(),
        lease_token,
        replay_key,
        scheduled_for_ms,
        artifact_sha256,
        now,
    )?;
    Ok(row.0)
}

fn validate_global_lease_postgres(
    tx: &mut postgres::Transaction<'_>,
    source_id: &str,
    lease_token: &str,
    replay_key: &str,
    scheduled_for_ms: i64,
    artifact_sha256: &str,
    now: i64,
) -> Result<GlobalDiscoverySource> {
    let row = tx
        .query_opt(
            "SELECT id, provider, source_key, source_json, status, health,
                    consecutive_failures, run_interval_ms, next_run_at_ms,
                    last_success_at_ms, last_failure_at_ms, last_error_code,
                    lease_expires_at_ms, created_at_ms, updated_at_ms, lease_token
               FROM jobs_global_discovery_sources WHERE id = $1 FOR UPDATE",
            &[&source_id],
        )?
        .context("global discovery source is not leased")?;
    let token = row.get::<_, Option<String>>(15);
    let source = global_source_from_pg_row(row)?;
    validate_global_lease_values(
        &source,
        token.as_deref(),
        lease_token,
        replay_key,
        scheduled_for_ms,
        artifact_sha256,
        now,
    )?;
    Ok(source)
}

fn validate_global_lease_values(
    source: &GlobalDiscoverySource,
    stored_token_hash: Option<&str>,
    lease_token: &str,
    replay_key: &str,
    scheduled_for_ms: i64,
    artifact_sha256: &str,
    now: i64,
) -> Result<()> {
    let presented_hash = hex::encode(Sha256::digest(lease_token.as_bytes()));
    let stored = stored_token_hash.unwrap_or_default().as_bytes();
    if stored.len() != presented_hash.len()
        || stored.ct_eq(presented_hash.as_bytes()).unwrap_u8() != 1
        || source.status != "active"
        || source.lease_expires_at_ms.is_none_or(|expires| expires <= now)
        || source.next_run_at_ms != scheduled_for_ms
        || global_replay_key(source) != replay_key
        || global_artifact_sha256(source)? != artifact_sha256
    {
        anyhow::bail!("global discovery lease is invalid or expired")
    }
    Ok(())
}

fn normalize_global_candidate(
    provider: &str,
    family: &str,
    input: &DiscoveredJobInput,
) -> Result<NormalizedGlobalCandidate> {
    let external_id = bounded_global_text(&input.external_id, "external job ID", 1, 1024)?;
    let company = bounded_global_text(&input.company, "company", 1, 300)?;
    let title = bounded_global_text(&input.title, "job title", 1, 500)?;
    let location = bounded_global_text(&input.location, "job location", 0, 1000)?;
    let workplace = normalize_global_workplace(&input.workplace)?;
    let description = bounded_global_text(&input.description, "job description", 0, 1_000_000)?;
    let compensation = bounded_global_text(&input.compensation, "job compensation", 0, 4096)?;
    let employment_type = normalize_global_employment_type(&input.employment_type)?;
    let engagement_type = normalize_global_engagement_type(&input.engagement_type)?;
    let canonical_url = canonicalize_curated_lead_url(&input.canonical_url)?;
    if let Some(posted_at_ms) = input.posted_at_ms {
        if posted_at_ms <= 0 || posted_at_ms > now_ms().saturating_add(24 * 60 * 60 * 1_000) {
            anyhow::bail!("job posting timestamp is invalid")
        }
    }
    let source_catalog_id = format!("{provider}:{family}");
    let normalized = DiscoveredJobInput {
        external_id: external_id.clone(),
        canonical_url: canonical_url.clone(),
        title: title.clone(),
        company: company.clone(),
        source_catalog_id,
        requires_original_revalidation: true,
        location: location.clone(),
        workplace: workplace.clone(),
        description,
        compensation,
        employment_type,
        engagement_type,
        posted_at_ms: input.posted_at_ms,
    };
    let posting = JobPosting {
        id: String::new(),
        canonical_key: String::new(),
        source: String::new(),
        external_id: external_id.clone(),
        company: company.clone(),
        title: title.clone(),
        location: location.clone(),
        workplace: workplace.clone(),
        canonical_url: canonical_url.clone(),
        description: String::new(),
        compensation: String::new(),
        employment_type: String::new(),
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
        discovery_evidence: JobDiscoveryEvidence::default(),
        eligibility: None,
    };
    let canonical_key = canonical_job_key(&posting);
    let candidate_id = format!("global-candidate-{}", &canonical_key[..32]);
    let candidate_plaintext =
        serde_json::to_string(&normalized).context("serialize global job candidate")?;
    let content_hash = hex::encode(Sha256::digest(candidate_plaintext.as_bytes()));
    let candidate_json =
        encrypt_payload(&candidate_plaintext).context("encrypt global job candidate")?;
    let role_family = infer_role_family(&title);
    Ok(NormalizedGlobalCandidate {
        external_id,
        canonical_key,
        candidate_id,
        candidate_json,
        content_hash,
        company,
        title,
        location,
        workplace,
        canonical_url,
        role_family,
        posted_at_ms: input.posted_at_ms,
    })
}

fn bounded_global_text(value: &str, label: &str, minimum: usize, maximum: usize) -> Result<String> {
    let value = value.trim();
    let length = value.chars().count();
    if length < minimum || length > maximum || value.contains('\0') {
        anyhow::bail!("{label} is invalid")
    }
    Ok(value.to_string())
}

fn normalize_global_workplace(value: &str) -> Result<String> {
    let normalized = value
        .trim()
        .to_ascii_lowercase()
        .replace(['-', ' '], "_");
    match normalized.as_str() {
        "" | "onsite" | "on_site" | "hybrid" | "remote" => Ok(normalized),
        _ => anyhow::bail!("job workplace is not canonical"),
    }
}

fn normalize_global_employment_type(value: &str) -> Result<String> {
    let normalized = value
        .trim()
        .to_ascii_lowercase()
        .replace(['-', ' '], "_");
    let canonical = match normalized.as_str() {
        "" => "",
        "fulltime" | "full_time" | "permanent" => "full_time",
        "parttime" | "part_time" => "part_time",
        "contract" | "contractor" => "contract",
        "temporary" | "temp" => "temporary",
        "intern" | "internship" => "internship",
        "apprentice" | "apprenticeship" => "apprenticeship",
        "seasonal" => "seasonal",
        "per_diem" => "per_diem",
        _ => anyhow::bail!("job employment type is not canonical"),
    };
    Ok(canonical.to_string())
}

fn normalize_global_engagement_type(value: &str) -> Result<String> {
    let normalized = value
        .trim()
        .to_ascii_lowercase()
        .replace(['-', ' '], "_");
    let canonical = match normalized.as_str() {
        "" => "",
        "w2" => "w2",
        "c2c" | "corp_to_corp" => "c2c",
        "1099" | "independent_contractor" => "1099",
        "direct" | "direct_hire" => "direct_hire",
        _ => anyhow::bail!("job engagement type is not canonical"),
    };
    Ok(canonical.to_string())
}

fn upsert_global_candidate_sqlite(
    tx: &rusqlite::Transaction<'_>,
    source_id: &str,
    run_id: &str,
    candidate: &NormalizedGlobalCandidate,
    now: i64,
) -> Result<()> {
    tx.execute(
        "INSERT INTO jobs_global_candidates (
            id, canonical_key, candidate_json, content_hash, company, title, location, workplace,
            canonical_url, role_family, posted_at_ms, availability_status,
            first_seen_at_ms, last_seen_at_ms, updated_at_ms
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 'unknown',
                   ?12, ?12, ?12)
         ON CONFLICT(canonical_key) DO UPDATE SET
            candidate_json = excluded.candidate_json, content_hash = excluded.content_hash,
            company = excluded.company,
            title = excluded.title, location = excluded.location, workplace = excluded.workplace,
            canonical_url = excluded.canonical_url, role_family = excluded.role_family,
            posted_at_ms = excluded.posted_at_ms, availability_status = 'unknown',
            last_seen_at_ms = excluded.last_seen_at_ms, updated_at_ms = excluded.updated_at_ms,
            archive_state = 'hot', archive_storage_key = NULL, archive_sha256 = NULL,
            archive_size_bytes = NULL, archived_at_ms = NULL, archive_attempt_count = 0,
            archive_next_attempt_at_ms = 0, archive_lease_owner = NULL,
            archive_lease_expires_at_ms = NULL
         WHERE jobs_global_candidates.content_hash <> excluded.content_hash
            OR jobs_global_candidates.availability_status = 'expired'
            OR jobs_global_candidates.archive_state <> 'hot'",
        params![
            candidate.candidate_id,
            candidate.canonical_key,
            candidate.candidate_json,
            candidate.content_hash,
            candidate.company,
            candidate.title,
            candidate.location,
            candidate.workplace,
            candidate.canonical_url,
            candidate.role_family,
            candidate.posted_at_ms,
            now,
        ],
    )?;
    let candidate_id: String = tx.query_row(
        "SELECT id FROM jobs_global_candidates WHERE canonical_key = ?1",
        params![candidate.canonical_key],
        |row| row.get(0),
    )?;
    tx.execute(
        "INSERT INTO jobs_global_candidate_memberships (
            source_id, external_id, candidate_id, content_hash, first_seen_at_ms,
            last_seen_at_ms, last_seen_run_id, availability_status, missing_count
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?5, ?6, 'active', 0)
         ON CONFLICT(source_id, external_id) DO UPDATE SET
            candidate_id = excluded.candidate_id, content_hash = excluded.content_hash,
            last_seen_at_ms = excluded.last_seen_at_ms, last_seen_run_id = excluded.last_seen_run_id,
            availability_status = 'active', missing_count = 0, missing_since_at_ms = NULL",
        params![source_id, candidate.external_id, candidate_id, candidate.content_hash, now, run_id],
    )?;
    Ok(())
}

fn upsert_global_candidate_postgres(
    tx: &mut postgres::Transaction<'_>,
    source_id: &str,
    run_id: &str,
    candidate: &NormalizedGlobalCandidate,
    now: i64,
) -> Result<()> {
    let row = tx.query_one(
        "WITH upserted AS (
            INSERT INTO jobs_global_candidates (
                id, canonical_key, candidate_json, content_hash, company, title, location,
                workplace, canonical_url, role_family, posted_at_ms, availability_status,
                first_seen_at_ms, last_seen_at_ms, updated_at_ms
             ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, 'unknown',
                       $12, $12, $12)
             ON CONFLICT(canonical_key) DO UPDATE SET
                candidate_json = excluded.candidate_json,
                content_hash = excluded.content_hash,
                company = excluded.company, title = excluded.title,
                location = excluded.location, workplace = excluded.workplace,
                canonical_url = excluded.canonical_url, role_family = excluded.role_family,
                posted_at_ms = excluded.posted_at_ms, availability_status = 'unknown',
                last_seen_at_ms = excluded.last_seen_at_ms,
                updated_at_ms = excluded.updated_at_ms,
                archive_state = 'hot', archive_storage_key = NULL, archive_sha256 = NULL,
                archive_size_bytes = NULL, archived_at_ms = NULL, archive_attempt_count = 0,
                archive_next_attempt_at_ms = 0, archive_lease_owner = NULL,
                archive_lease_expires_at_ms = NULL
             WHERE jobs_global_candidates.content_hash IS DISTINCT FROM excluded.content_hash
                OR jobs_global_candidates.availability_status = 'expired'
                OR jobs_global_candidates.archive_state <> 'hot'
             RETURNING id
         )
         SELECT id FROM upserted
         UNION ALL
         SELECT id FROM jobs_global_candidates WHERE canonical_key = $2
         LIMIT 1",
        &[
            &candidate.candidate_id,
            &candidate.canonical_key,
            &candidate.candidate_json,
            &candidate.content_hash,
            &candidate.company,
            &candidate.title,
            &candidate.location,
            &candidate.workplace,
            &candidate.canonical_url,
            &candidate.role_family,
            &candidate.posted_at_ms,
            &now,
        ],
    )?;
    let candidate_id = row.get::<_, String>(0);
    tx.execute(
        "INSERT INTO jobs_global_candidate_memberships (
            source_id, external_id, candidate_id, content_hash, first_seen_at_ms,
            last_seen_at_ms, last_seen_run_id, availability_status, missing_count
         ) VALUES ($1, $2, $3, $4, $5, $5, $6, 'active', 0)
         ON CONFLICT(source_id, external_id) DO UPDATE SET
            candidate_id = excluded.candidate_id, content_hash = excluded.content_hash,
            last_seen_at_ms = excluded.last_seen_at_ms, last_seen_run_id = excluded.last_seen_run_id,
            availability_status = 'active', missing_count = 0, missing_since_at_ms = NULL",
        &[&source_id, &candidate.external_id, &candidate_id, &candidate.content_hash, &now, &run_id],
    )?;
    Ok(())
}
