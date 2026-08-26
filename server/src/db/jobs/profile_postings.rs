pub fn canonical_job_key(posting: &JobPosting) -> String {
    let normalized = format!(
        "{}|{}|{}|{}",
        posting.company.trim().to_lowercase(),
        posting.title.trim().to_lowercase(),
        posting.location.trim().to_lowercase(),
        canonical_url_for_job_key(&posting.canonical_url)
    );
    hex::encode(Sha256::digest(normalized.as_bytes()))
}

/// Keep public-job identity stable when an ATS link differs only by common
/// attribution parameters. Preserve other query parameters because some ATSs
/// use them to select a genuine job variant.
fn canonical_url_for_job_key(raw: &str) -> String {
    let Ok(mut url) = reqwest::Url::parse(raw.trim()) else {
        return raw.trim().trim_end_matches('/').to_lowercase();
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
    let path = url.path().trim_end_matches('/').to_string();
    url.set_path(if path.is_empty() { "/" } else { &path });
    url.to_string().trim_end_matches('/').to_lowercase()
}

fn is_tracking_query_key(key: &str) -> bool {
    matches!(
        key.to_ascii_lowercase().as_str(),
        "gh_src" | "lever-source" | "source" | "ref" | "referrer"
    ) || key.to_ascii_lowercase().starts_with("utm_")
}

pub fn default_profile(email: &str) -> CareerProfile {
    CareerProfile {
        email: email.to_string(),
        ..CareerProfile::default()
    }
}

pub fn get_profile(pool: &DbPool, account_id: &str, email: &str) -> Result<CareerProfile> {
    let mut profile = crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get()?;
            let raw: Option<String> = conn
                .query_row(
                    "SELECT profile_json FROM jobs_profiles WHERE account_id = ?1",
                    params![account_id],
                    |row| row.get(0),
                )
                .optional()?;
            raw.map(|value| parse_json(value, "Jobs profile"))
                .transpose()
                .map(|value| value.unwrap_or_else(|| default_profile(email)))
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let row = conn.query_opt(
                "SELECT profile_json FROM jobs_profiles WHERE account_id = $1",
                &[&account_id],
            )?;
            row.map(|value| parse_json(value.get(0), "Jobs profile"))
                .transpose()
                .map(|value| value.unwrap_or_else(|| default_profile(email)))
        }
    })?;
    profile.auto_submit_threshold = default_auto_submit_threshold();
    profile.daily_limit = default_daily_limit();
    Ok(profile)
}

pub fn save_profile(
    pool: &DbPool,
    account_id: &str,
    profile: &CareerProfile,
) -> Result<CareerProfile> {
    let mut value = profile.clone();
    value.onboarding_step = value.onboarding_step.clamp(0, 6);
    value.auto_submit_threshold = default_auto_submit_threshold();
    value.daily_limit = default_daily_limit();
    value.updated_at_ms = now_ms();
    let payload = to_json(&value, "Jobs profile")?;
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            tx.execute(
                "INSERT INTO jobs_profiles (
                    account_id, profile_json, onboarding_step, onboarding_complete,
                    created_at_ms, updated_at_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?5)
                 ON CONFLICT(account_id) DO UPDATE SET
                    profile_json = excluded.profile_json,
                    onboarding_step = excluded.onboarding_step,
                    onboarding_complete = excluded.onboarding_complete,
                    updated_at_ms = excluded.updated_at_ms",
                params![
                    account_id,
                    payload,
                    value.onboarding_step,
                    i64::from(value.onboarding_complete),
                    value.updated_at_ms
                ],
            )?;
            advance_account_input_generation_sqlite(
                &tx,
                account_id,
                "profile",
                "profile",
                value.updated_at_ms,
            )?;
            tx.commit()?;
            Ok(value)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            lock_discovery_account_postgres(&mut tx, account_id)?;
            lock_account_policy_inputs_postgres(&mut tx, account_id, true)?;
            let onboarding_complete = i32::from(value.onboarding_complete);
            tx.execute(
                "INSERT INTO jobs_profiles (
                    account_id, profile_json, onboarding_step, onboarding_complete,
                    created_at_ms, updated_at_ms
                 ) VALUES ($1, $2, $3, $4, $5, $5)
                 ON CONFLICT(account_id) DO UPDATE SET
                    profile_json = EXCLUDED.profile_json,
                    onboarding_step = EXCLUDED.onboarding_step,
                    onboarding_complete = EXCLUDED.onboarding_complete,
                    updated_at_ms = EXCLUDED.updated_at_ms",
                &[
                    &account_id,
                    &payload,
                    &value.onboarding_step,
                    &onboarding_complete,
                    &value.updated_at_ms,
                ],
            )?;
            advance_account_input_generation_postgres(
                &mut tx,
                account_id,
                "profile",
                "profile",
                value.updated_at_ms,
            )?;
            tx.commit()?;
            Ok(value)
        }
    })
}

pub fn list_facts(pool: &DbPool, account_id: &str) -> Result<Vec<CareerFact>> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get()?;
            let mut stmt = conn.prepare(
                "SELECT id, category, label, value_json, source, verification_status,
                        confirmed_at_ms, confirmed_by, schema_version, created_at_ms, updated_at_ms
                   FROM jobs_facts WHERE account_id = ?1
                  ORDER BY category, updated_at_ms DESC",
            )?;
            let facts = stmt
                .query_map(params![account_id], fact_from_sqlite_row)?
                .collect::<std::result::Result<Vec<_>, _>>()
                .context("list Jobs facts")?;
            Ok(facts)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            conn.query(
                "SELECT id, category, label, value_json, source, verification_status,
                        confirmed_at_ms, confirmed_by, schema_version, created_at_ms, updated_at_ms
                   FROM jobs_facts WHERE account_id = $1
                  ORDER BY category, updated_at_ms DESC",
                &[&account_id],
            )?
            .into_iter()
            .map(fact_from_pg_row)
            .collect()
        }
    })
}

fn fact_from_sqlite_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<CareerFact> {
    let raw: String = row.get(3)?;
    Ok(CareerFact {
        id: row.get(0)?,
        category: row.get(1)?,
        label: row.get(2)?,
        value: parse_json_lossy(&raw).unwrap_or(Value::Null),
        source: row.get(4)?,
        verification_status: row.get(5)?,
        confirmed_at_ms: row.get(6)?,
        confirmed_by: row.get(7)?,
        schema_version: row.get(8)?,
        created_at_ms: row.get(9)?,
        updated_at_ms: row.get(10)?,
    })
}

fn fact_from_pg_row(row: postgres::Row) -> Result<CareerFact> {
    let raw: String = row.get(3);
    Ok(CareerFact {
        id: row.get(0),
        category: row.get(1),
        label: row.get(2),
        value: parse_json(raw, "Jobs fact value")?,
        source: row.get(4),
        verification_status: row.get(5),
        confirmed_at_ms: row.get(6),
        confirmed_by: row.get(7),
        schema_version: row.get(8),
        created_at_ms: row.get(9),
        updated_at_ms: row.get(10),
    })
}

pub fn upsert_fact(pool: &DbPool, account_id: &str, fact: &CareerFact) -> Result<CareerFact> {
    let mut value = fact.clone();
    if value.id.trim().is_empty() {
        value.id = uuid::Uuid::new_v4().to_string();
    }
    let now = now_ms();
    if value.created_at_ms == 0 {
        value.created_at_ms = now;
    }
    value.updated_at_ms = now;
    if value.verification_status == "confirmed" && value.confirmed_at_ms.is_none() {
        value.confirmed_at_ms = Some(now);
        value.confirmed_by = Some("user".to_string());
    }
    let payload = to_json(&value.value, "Jobs fact value")?;
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let changed = tx.execute(
                "INSERT INTO jobs_facts (
                    id, account_id, category, label, value_json, source,
                    verification_status, confirmed_at_ms, confirmed_by,
                    schema_version, created_at_ms, updated_at_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
                 ON CONFLICT(id) DO UPDATE SET
                    category = excluded.category,
                    label = excluded.label,
                    value_json = excluded.value_json,
                    source = excluded.source,
                    verification_status = excluded.verification_status,
                    confirmed_at_ms = excluded.confirmed_at_ms,
                    confirmed_by = excluded.confirmed_by,
                    schema_version = excluded.schema_version,
                    updated_at_ms = excluded.updated_at_ms
                 WHERE jobs_facts.account_id = excluded.account_id",
                params![
                    value.id,
                    account_id,
                    value.category,
                    value.label,
                    payload,
                    value.source,
                    value.verification_status,
                    value.confirmed_at_ms,
                    value.confirmed_by,
                    value.schema_version,
                    value.created_at_ms,
                    value.updated_at_ms,
                ],
            )?;
            anyhow::ensure!(changed == 1, "career fact belongs to another account");
            advance_account_input_generation_sqlite(
                &tx,
                account_id,
                "fact",
                &value.id,
                value.updated_at_ms,
            )?;
            tx.commit()?;
            Ok(value)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            lock_discovery_account_postgres(&mut tx, account_id)?;
            lock_account_policy_inputs_postgres(&mut tx, account_id, true)?;
            let changed = tx.execute(
                "INSERT INTO jobs_facts (
                    id, account_id, category, label, value_json, source,
                    verification_status, confirmed_at_ms, confirmed_by,
                    schema_version, created_at_ms, updated_at_ms
                 ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
                 ON CONFLICT(id) DO UPDATE SET
                    category = EXCLUDED.category,
                    label = EXCLUDED.label,
                    value_json = EXCLUDED.value_json,
                    source = EXCLUDED.source,
                    verification_status = EXCLUDED.verification_status,
                    confirmed_at_ms = EXCLUDED.confirmed_at_ms,
                    confirmed_by = EXCLUDED.confirmed_by,
                    schema_version = EXCLUDED.schema_version,
                    updated_at_ms = EXCLUDED.updated_at_ms
                 WHERE jobs_facts.account_id = EXCLUDED.account_id",
                &[
                    &value.id,
                    &account_id,
                    &value.category,
                    &value.label,
                    &payload,
                    &value.source,
                    &value.verification_status,
                    &value.confirmed_at_ms,
                    &value.confirmed_by,
                    &value.schema_version,
                    &value.created_at_ms,
                    &value.updated_at_ms,
                ],
            )?;
            anyhow::ensure!(changed == 1, "career fact belongs to another account");
            advance_account_input_generation_postgres(
                &mut tx,
                account_id,
                "fact",
                &value.id,
                value.updated_at_ms,
            )?;
            tx.commit()?;
            Ok(value)
        }
    })
}

pub fn upsert_user_fact(
    pool: &DbPool,
    account_id: &str,
    fact_id: Option<&str>,
    category: &str,
    label: &str,
    fact_value: Value,
) -> Result<CareerFact> {
    let requested_id = fact_id.map(str::trim).filter(|id| !id.is_empty());
    let now = now_ms();
    let payload = to_json(&fact_value, "Jobs fact value")?;
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let existing = requested_id
                .map(|id| {
                    tx.query_row(
                        "SELECT id, category, label, value_json, source, verification_status,
                                confirmed_at_ms, confirmed_by, schema_version, created_at_ms, updated_at_ms
                           FROM jobs_facts WHERE account_id = ?1 AND id = ?2",
                        params![account_id, id],
                        fact_from_sqlite_row,
                    )
                    .optional()
                })
                .transpose()?
                .flatten();
            if requested_id.is_some() && existing.is_none() {
                anyhow::bail!("career fact not found")
            }
            if existing
                .as_ref()
                .is_some_and(|fact| fact.source != "user_entry")
            {
                anyhow::bail!("imported career facts require a dedicated confirmation flow")
            }
            let id = existing
                .as_ref()
                .map(|fact| fact.id.clone())
                .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
            let created_at_ms = existing
                .as_ref()
                .map(|fact| fact.created_at_ms)
                .unwrap_or(now);
            tx.execute(
                "INSERT INTO jobs_facts (
                    id, account_id, category, label, value_json, source,
                    verification_status, confirmed_at_ms, confirmed_by,
                    schema_version, created_at_ms, updated_at_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, 'user_entry',
                           'confirmed', ?6, 'user', 1, ?7, ?6)
                 ON CONFLICT(id) DO UPDATE SET
                    category = excluded.category,
                    label = excluded.label,
                    value_json = excluded.value_json,
                    verification_status = 'confirmed',
                    confirmed_at_ms = excluded.confirmed_at_ms,
                    confirmed_by = 'user',
                    schema_version = 1,
                    updated_at_ms = excluded.updated_at_ms
                 WHERE jobs_facts.account_id = excluded.account_id
                   AND jobs_facts.source = 'user_entry'",
                params![id, account_id, category, label, payload, now, created_at_ms],
            )?;
            advance_account_input_generation_sqlite(&tx, account_id, "fact", &id, now)?;
            tx.commit()?;
            Ok(CareerFact {
                id,
                category: category.to_string(),
                label: label.to_string(),
                value: fact_value,
                source: "user_entry".to_string(),
                verification_status: "confirmed".to_string(),
                confirmed_at_ms: Some(now),
                confirmed_by: Some("user".to_string()),
                schema_version: 1,
                created_at_ms,
                updated_at_ms: now,
            })
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            lock_discovery_account_postgres(&mut tx, account_id)?;
            lock_account_policy_inputs_postgres(&mut tx, account_id, true)?;
            let existing = if let Some(id) = requested_id {
                tx.query_opt(
                        "SELECT id, category, label, value_json, source, verification_status,
                                confirmed_at_ms, confirmed_by, schema_version, created_at_ms, updated_at_ms
                           FROM jobs_facts WHERE account_id = $1 AND id = $2 FOR UPDATE",
                        &[&account_id, &id],
                    )?
                    .map(fact_from_pg_row)
                    .transpose()?
            } else {
                None
            };
            if requested_id.is_some() && existing.is_none() {
                anyhow::bail!("career fact not found")
            }
            if existing
                .as_ref()
                .is_some_and(|fact| fact.source != "user_entry")
            {
                anyhow::bail!("imported career facts require a dedicated confirmation flow")
            }
            let id = existing
                .as_ref()
                .map(|fact| fact.id.clone())
                .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
            let created_at_ms = existing
                .as_ref()
                .map(|fact| fact.created_at_ms)
                .unwrap_or(now);
            tx.execute(
                "INSERT INTO jobs_facts (
                    id, account_id, category, label, value_json, source,
                    verification_status, confirmed_at_ms, confirmed_by,
                    schema_version, created_at_ms, updated_at_ms
                 ) VALUES ($1, $2, $3, $4, $5, 'user_entry',
                           'confirmed', $6, 'user', 1, $7, $6)
                 ON CONFLICT(id) DO UPDATE SET
                    category = EXCLUDED.category,
                    label = EXCLUDED.label,
                    value_json = EXCLUDED.value_json,
                    verification_status = 'confirmed',
                    confirmed_at_ms = EXCLUDED.confirmed_at_ms,
                    confirmed_by = 'user',
                    schema_version = 1,
                    updated_at_ms = EXCLUDED.updated_at_ms
                 WHERE jobs_facts.account_id = EXCLUDED.account_id
                   AND jobs_facts.source = 'user_entry'",
                &[
                    &id,
                    &account_id,
                    &category,
                    &label,
                    &payload,
                    &now,
                    &created_at_ms,
                ],
            )?;
            advance_account_input_generation_postgres(&mut tx, account_id, "fact", &id, now)?;
            tx.commit()?;
            Ok(CareerFact {
                id,
                category: category.to_string(),
                label: label.to_string(),
                value: fact_value,
                source: "user_entry".to_string(),
                verification_status: "confirmed".to_string(),
                confirmed_at_ms: Some(now),
                confirmed_by: Some("user".to_string()),
                schema_version: 1,
                created_at_ms,
                updated_at_ms: now,
            })
        }
    })
}

pub fn delete_fact(pool: &DbPool, account_id: &str, fact_id: &str) -> Result<bool> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let changed = tx.execute(
                "DELETE FROM jobs_facts WHERE account_id = ?1 AND id = ?2",
                params![account_id, fact_id],
            )? > 0;
            if changed {
                advance_account_input_generation_sqlite(
                    &tx,
                    account_id,
                    "fact",
                    fact_id,
                    now_ms(),
                )?;
            }
            tx.commit()?;
            Ok(changed)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            lock_discovery_account_postgres(&mut tx, account_id)?;
            lock_account_policy_inputs_postgres(&mut tx, account_id, true)?;
            let changed = tx.execute(
                "DELETE FROM jobs_facts WHERE account_id = $1 AND id = $2",
                &[&account_id, &fact_id],
            )? > 0;
            if changed {
                advance_account_input_generation_postgres(
                    &mut tx,
                    account_id,
                    "fact",
                    fact_id,
                    now_ms(),
                )?;
            }
            tx.commit()?;
            Ok(changed)
        }
    })
}

pub fn get_preferences(pool: &DbPool, account_id: &str) -> Result<JobPreferences> {
    let value = crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get()?;
            let raw: Option<String> = conn
                .query_row(
                    "SELECT preferences_json FROM jobs_preferences WHERE account_id = ?1",
                    params![account_id],
                    |row| row.get(0),
                )
                .optional()?;
            raw.map(|value| parse_json::<JobPreferences>(value, "Jobs preferences"))
                .transpose()
                .map(|value| value.unwrap_or_default())
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let row = conn.query_opt(
                "SELECT preferences_json FROM jobs_preferences WHERE account_id = $1",
                &[&account_id],
            )?;
            row.map(|value| parse_json::<JobPreferences>(value.get(0), "Jobs preferences"))
                .transpose()
                .map(|value| value.unwrap_or_default())
        }
    })?;
    Ok(enforce_job_preference_safety(value))
}

fn enforce_job_preference_safety(mut value: JobPreferences) -> JobPreferences {
    // An application email is an alias for one candidate, not a second
    // identity that can bypass employer-level submission safeguards.
    value.apply_once_per_company = true;
    value.daily_limit = default_daily_limit();
    value.max_posting_age_days = default_max_posting_age_days();
    value
}

pub fn save_preferences(
    pool: &DbPool,
    account_id: &str,
    preferences: &JobPreferences,
) -> Result<JobPreferences> {
    let mut value = preferences.clone();
    value.daily_limit = default_daily_limit();
    value.max_posting_age_days = default_max_posting_age_days();
    value.apply_once_per_company = true;
    value.updated_at_ms = now_ms();
    let payload = to_json(&value, "Jobs preferences")?;
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            tx.execute(
                "INSERT INTO jobs_preferences(account_id, preferences_json, updated_at_ms)
                 VALUES (?1, ?2, ?3)
                 ON CONFLICT(account_id) DO UPDATE SET
                    preferences_json = excluded.preferences_json,
                    updated_at_ms = excluded.updated_at_ms",
                params![account_id, payload, value.updated_at_ms],
            )?;
            advance_account_input_generation_sqlite(
                &tx,
                account_id,
                "preferences",
                "preferences",
                value.updated_at_ms,
            )?;
            tx.commit()?;
            Ok(value)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            lock_discovery_account_postgres(&mut tx, account_id)?;
            lock_account_policy_inputs_postgres(&mut tx, account_id, true)?;
            tx.execute(
                "INSERT INTO jobs_preferences(account_id, preferences_json, updated_at_ms)
                 VALUES ($1, $2, $3)
                 ON CONFLICT(account_id) DO UPDATE SET
                    preferences_json = EXCLUDED.preferences_json,
                    updated_at_ms = EXCLUDED.updated_at_ms",
                &[&account_id, &payload, &value.updated_at_ms],
            )?;
            advance_account_input_generation_postgres(
                &mut tx,
                account_id,
                "preferences",
                "preferences",
                value.updated_at_ms,
            )?;
            tx.commit()?;
            Ok(value)
        }
    })
}

pub fn list_tracks(pool: &DbPool, account_id: &str) -> Result<Vec<CareerTrack>> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction()?;
            let mut tracks = {
                let mut stmt = tx.prepare(
                    "SELECT id, track_json, active FROM jobs_tracks
                      WHERE account_id = ?1
                      ORDER BY active DESC, updated_at_ms DESC",
                )?;
                let rows = stmt
                    .query_map(params![account_id], |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, i64>(2)? != 0,
                        ))
                    })?
                    .collect::<std::result::Result<Vec<_>, _>>()?;
                rows.into_iter()
                    .map(|(authoritative_id, raw, authoritative_active)| {
                        let mut track = parse_json::<CareerTrack>(raw, "Career Track")?;
                        track.id = authoritative_id;
                        let activation_drift = track.active != authoritative_active;
                        track.active = authoritative_active;
                        if track.policy.authority.review_state == "approved"
                            && (activation_drift
                                || validate_track_policy_ledger_sqlite(&tx, account_id, &track)
                                    .is_err())
                        {
                            downgrade_track_policy_ledger(&mut track);
                        }
                        Ok(track)
                    })
                    .collect::<Result<Vec<_>>>()?
            };
            tx.commit()?;
            tracks.shrink_to_fit();
            Ok(tracks)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            lock_discovery_account_shared_postgres(&mut tx, account_id)?;
            let rows = tx.query(
                "SELECT id, track_json, active FROM jobs_tracks
                  WHERE account_id = $1
                  ORDER BY active DESC, updated_at_ms DESC
                  FOR SHARE",
                &[&account_id],
            )?;
            let mut tracks = Vec::with_capacity(rows.len());
            for row in rows {
                let authoritative_id: String = row.get(0);
                let authoritative_active = row.get::<_, i32>(2) != 0;
                let mut track: CareerTrack = parse_json(row.get(1), "Career Track")?;
                track.id = authoritative_id;
                let activation_drift = track.active != authoritative_active;
                track.active = authoritative_active;
                if track.policy.authority.review_state == "approved"
                    && (activation_drift
                        || validate_track_policy_ledger_postgres(&mut tx, account_id, &track)
                            .is_err())
                {
                    downgrade_track_policy_ledger(&mut track);
                }
                tracks.push(track);
            }
            tx.commit()?;
            Ok(tracks)
        }
    })
}

fn downgrade_track_policy_ledger(track: &mut CareerTrack) {
    track.policy.authority.review_state = "needs_review".to_string();
    track.policy.authority.review_reason_codes = vec!["policy_ledger_review_required".to_string()];
}

fn require_track_mutation_unleased_sqlite(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    track_id: &str,
) -> Result<()> {
    let leased: bool = tx.query_row(
        "SELECT EXISTS(
            SELECT 1 FROM jobs_discovery_sources
             WHERE account_id = ?1 AND (provider = ?2 OR track_id = ?3)
               AND lease_token IS NOT NULL AND lease_expires_at_ms > ?4
         )",
        params![account_id, CURATED_DISCOVERY_PROVIDER, track_id, now_ms()],
        |row| row.get(0),
    )?;
    if leased {
        anyhow::bail!("Career Track changes wait for the active discovery lease to finish")
    }
    Ok(())
}

fn require_track_mutation_unleased_postgres(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    track_id: &str,
) -> Result<()> {
    let rows = tx.query(
        "SELECT lease_token, lease_expires_at_ms FROM jobs_discovery_sources
          WHERE account_id = $1 AND (provider = $2 OR track_id = $3)
          ORDER BY id FOR UPDATE",
        &[&account_id, &CURATED_DISCOVERY_PROVIDER, &track_id],
    )?;
    let now = now_ms();
    if rows.iter().any(|row| {
        row.get::<_, Option<String>>(0).is_some()
            && row
                .get::<_, Option<i64>>(1)
                .is_some_and(|expires_at_ms| expires_at_ms > now)
    }) {
        anyhow::bail!("Career Track changes wait for the active discovery lease to finish")
    }
    Ok(())
}

#[derive(Debug, Error, PartialEq, Eq)]
#[error("Career Track active limit exceeded")]
pub struct CareerTrackLimitExceeded;

fn require_active_track_limit_sqlite(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    track_id: &str,
    activating: bool,
    active_track_limit: i64,
) -> Result<()> {
    if !activating {
        return Ok(());
    }
    let existing_active = tx
        .query_row(
            "SELECT active FROM jobs_tracks WHERE account_id = ?1 AND id = ?2",
            params![account_id, track_id],
            |row| row.get::<_, i64>(0),
        )
        .optional()?;
    if existing_active == Some(1) {
        return Ok(());
    }
    let active_count: i64 = tx.query_row(
        "SELECT COUNT(*) FROM jobs_tracks WHERE account_id = ?1 AND active = 1",
        params![account_id],
        |row| row.get(0),
    )?;
    if active_count >= active_track_limit.max(0) {
        return Err(CareerTrackLimitExceeded.into());
    }
    Ok(())
}

fn require_active_track_limit_postgres(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    track_id: &str,
    activating: bool,
    active_track_limit: i64,
) -> Result<()> {
    if !activating {
        return Ok(());
    }
    let existing_active = tx
        .query_opt(
            "SELECT active FROM jobs_tracks WHERE account_id = $1 AND id = $2 FOR UPDATE",
            &[&account_id, &track_id],
        )?
        .map(|row| row.get::<_, i32>(0));
    if existing_active == Some(1) {
        return Ok(());
    }
    let active_count = tx
        .query_one(
            "SELECT COUNT(*) FROM jobs_tracks WHERE account_id = $1 AND active = 1",
            &[&account_id],
        )?
        .get::<_, i64>(0);
    if active_count >= active_track_limit.max(0) {
        return Err(CareerTrackLimitExceeded.into());
    }
    Ok(())
}

pub fn upsert_track(pool: &DbPool, account_id: &str, track: &CareerTrack) -> Result<CareerTrack> {
    upsert_track_with_optional_limit(pool, account_id, track, None)
}

pub fn upsert_track_with_limit(
    pool: &DbPool,
    account_id: &str,
    track: &CareerTrack,
    active_track_limit: i64,
) -> Result<CareerTrack> {
    upsert_track_with_optional_limit(pool, account_id, track, Some(active_track_limit))
}

fn upsert_track_with_optional_limit(
    pool: &DbPool,
    account_id: &str,
    track: &CareerTrack,
    active_track_limit: Option<i64>,
) -> Result<CareerTrack> {
    let mut value = normalize_canonical_track(track)?;
    if value.id.trim().is_empty() {
        value.id = uuid::Uuid::new_v4().to_string();
    }
    let now = now_ms();
    if value.created_at_ms == 0 {
        value.created_at_ms = now;
    }
    value.updated_at_ms = now;
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            require_track_mutation_unleased_sqlite(&tx, account_id, &value.id)?;
            if let Some(limit) = active_track_limit {
                require_active_track_limit_sqlite(&tx, account_id, &value.id, value.active, limit)?;
            }
            let payload = to_json(&value, "Career Track")?;
            let changed = tx.execute(
                "INSERT INTO jobs_tracks(id, account_id, track_json, active, created_at_ms, updated_at_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT(id) DO UPDATE SET
                    track_json = excluded.track_json,
                    active = excluded.active,
                    updated_at_ms = excluded.updated_at_ms
                 WHERE jobs_tracks.account_id = excluded.account_id",
                params![
                    value.id,
                    account_id,
                    payload,
                    i64::from(value.active),
                    value.created_at_ms,
                    value.updated_at_ms,
                ],
            )?;
            if changed != 1 {
                anyhow::bail!("Career Track ID is already assigned to another account")
            }
            let account_generation = ensure_account_input_generation_sqlite(&tx, account_id, now)?;
            let track_generation =
                advance_track_input_generation_sqlite(&tx, account_id, &value, now)?;
            let generations = CanonicalPolicyGenerations {
                activation: load_taxonomy_activation_sqlite(&tx)?,
                account: account_generation,
                track: track_generation,
            };
            let (profile, preferences, identity, resume_asset_verified) =
                canonical_track_policy_inputs_sqlite(&tx, account_id, &value)?;
            let record = prepare_canonical_track_policy_record(
                account_id,
                &mut value,
                &profile,
                &preferences,
                identity.as_ref(),
                resume_asset_verified,
                &generations,
            )?;
            if let Some(record) = record {
                let head =
                    persist_track_policy_revision_sqlite(&tx, account_id, &value.id, &record, now)?;
                value.policy.authority.policy_revision_id = head.revision_id;
                value.policy.authority.policy_revision_no = head.revision_no;
                value.policy.authority.canonical_policy_sha256 = head.canonical_policy_sha256;
                value.policy.authority.policy_head_generation = head.head_generation;
                value.policy.authority.policy_head_transition_sha256 = head.head_transition_sha256;
                value.policy.authority.policy_review_receipt_id = head.review_receipt_id;
                value.policy.authority.policy_review_receipt_sha256 = head.review_receipt_sha256;
                value.policy.authority.taxonomy_activation_epoch = record.taxonomy_activation_epoch;
                value.policy.authority.canonicalizer_schema_version =
                    record.canonicalizer_schema_version;
                value.policy.authority.canonicalizer_sha256 = record.canonicalizer_sha256.clone();
                value.policy.authority.account_input_generation = record.account_input_generation;
                value.policy.authority.account_input_transition_sha256 =
                    record.account_input_transition_sha256.clone();
                value.policy.authority.account_input_semantic_sha256 =
                    record.account_semantic_sha256.clone();
                value.policy.authority.track_input_generation = record.track_input_generation;
                value.policy.authority.track_input_transition_sha256 =
                    record.track_input_transition_sha256.clone();
                value.policy.authority.track_semantic_sha256 = record.track_semantic_sha256.clone();
                value.policy.authority.review_state = "approved".to_string();
                value.policy.authority.review_reason_codes.clear();
            }
            let authoritative_payload = to_json(&value, "Career Track authority")?;
            let changed = tx.execute(
                "UPDATE jobs_tracks SET track_json = ?3, updated_at_ms = ?4
                  WHERE account_id = ?1 AND id = ?2",
                params![
                    account_id,
                    value.id,
                    authoritative_payload,
                    value.updated_at_ms
                ],
            )?;
            if changed != 1 {
                anyhow::bail!("Career Track changed while policy authority was recorded")
            }
            tx.commit()?;
            Ok(value)
        }
        DbPool::Postgres(_) => {
            let active = i32::from(value.active);
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            lock_discovery_account_postgres(&mut tx, account_id)?;
            lock_account_policy_inputs_postgres(&mut tx, account_id, true)?;
            require_track_mutation_unleased_postgres(&mut tx, account_id, &value.id)?;
            if let Some(limit) = active_track_limit {
                require_active_track_limit_postgres(
                    &mut tx,
                    account_id,
                    &value.id,
                    value.active,
                    limit,
                )?;
            }
            let payload = to_json(&value, "Career Track")?;
            let changed = tx.execute(
                "INSERT INTO jobs_tracks(id, account_id, track_json, active, created_at_ms, updated_at_ms)
                 VALUES ($1, $2, $3, $4, $5, $6)
                 ON CONFLICT(id) DO UPDATE SET
                    track_json = EXCLUDED.track_json,
                    active = EXCLUDED.active,
                    updated_at_ms = EXCLUDED.updated_at_ms
                 WHERE jobs_tracks.account_id = EXCLUDED.account_id",
                &[
                    &value.id,
                    &account_id,
                    &payload,
                    &active,
                    &value.created_at_ms,
                    &value.updated_at_ms,
                ],
            )?;
            if changed != 1 {
                anyhow::bail!("Career Track ID is already assigned to another account")
            }
            let account_generation =
                ensure_account_input_generation_postgres(&mut tx, account_id, now)?;
            let track_generation =
                advance_track_input_generation_postgres(&mut tx, account_id, &value, now)?;
            let generations = CanonicalPolicyGenerations {
                activation: load_taxonomy_activation_postgres(&mut tx)?,
                account: account_generation,
                track: track_generation,
            };
            let (profile, preferences, identity, resume_asset_verified) =
                canonical_track_policy_inputs_postgres(&mut tx, account_id, &value)?;
            let record = prepare_canonical_track_policy_record(
                account_id,
                &mut value,
                &profile,
                &preferences,
                identity.as_ref(),
                resume_asset_verified,
                &generations,
            )?;
            if let Some(record) = record {
                let head = persist_track_policy_revision_postgres(
                    &mut tx, account_id, &value.id, &record, now,
                )?;
                value.policy.authority.policy_revision_id = head.revision_id;
                value.policy.authority.policy_revision_no = head.revision_no;
                value.policy.authority.canonical_policy_sha256 = head.canonical_policy_sha256;
                value.policy.authority.policy_head_generation = head.head_generation;
                value.policy.authority.policy_head_transition_sha256 = head.head_transition_sha256;
                value.policy.authority.policy_review_receipt_id = head.review_receipt_id;
                value.policy.authority.policy_review_receipt_sha256 = head.review_receipt_sha256;
                value.policy.authority.taxonomy_activation_epoch = record.taxonomy_activation_epoch;
                value.policy.authority.canonicalizer_schema_version =
                    record.canonicalizer_schema_version;
                value.policy.authority.canonicalizer_sha256 = record.canonicalizer_sha256.clone();
                value.policy.authority.account_input_generation = record.account_input_generation;
                value.policy.authority.account_input_transition_sha256 =
                    record.account_input_transition_sha256.clone();
                value.policy.authority.account_input_semantic_sha256 =
                    record.account_semantic_sha256.clone();
                value.policy.authority.track_input_generation = record.track_input_generation;
                value.policy.authority.track_input_transition_sha256 =
                    record.track_input_transition_sha256.clone();
                value.policy.authority.track_semantic_sha256 = record.track_semantic_sha256.clone();
                value.policy.authority.review_state = "approved".to_string();
                value.policy.authority.review_reason_codes.clear();
            }
            let authoritative_payload = to_json(&value, "Career Track authority")?;
            let changed = tx.execute(
                "UPDATE jobs_tracks SET track_json = $3, updated_at_ms = $4
                  WHERE account_id = $1 AND id = $2",
                &[
                    &account_id,
                    &value.id,
                    &authoritative_payload,
                    &value.updated_at_ms,
                ],
            )?;
            if changed != 1 {
                anyhow::bail!("Career Track changed while policy authority was recorded")
            }
            tx.commit()?;
            Ok(value)
        }
    })
}

pub fn delete_track(pool: &DbPool, account_id: &str, track_id: &str) -> Result<bool> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            require_track_mutation_unleased_sqlite(&tx, account_id, track_id)?;
            let exists = tx
                .query_row(
                    "SELECT 1 FROM jobs_tracks WHERE account_id = ?1 AND id = ?2",
                    params![account_id, track_id],
                    |_| Ok(()),
                )
                .optional()?
                .is_some();
            if exists {
                let source_count: i64 = tx.query_row(
                    "SELECT COUNT(*) FROM jobs_discovery_sources WHERE account_id = ?1 AND track_id = ?2",
                    params![account_id, track_id],
                    |row| row.get(0),
                )?;
                let mut stmt =
                    tx.prepare("SELECT posting_json FROM jobs_postings WHERE account_id = ?1")?;
                let bound_posting = stmt
                    .query_map(params![account_id], |row| row.get::<_, String>(0))?
                    .collect::<std::result::Result<Vec<_>, _>>()?
                    .into_iter()
                    .map(|raw| parse_json::<JobPosting>(raw, "job posting"))
                    .collect::<Result<Vec<_>>>()?
                    .iter()
                    .any(|posting| posting.track_id == track_id);
                drop(stmt);
                if source_count > 0 || bound_posting {
                    anyhow::bail!("Career Track still has Jobs matches or discovery sources")
                }
                tx.execute(
                    "DELETE FROM jobs_tracks WHERE account_id = ?1 AND id = ?2",
                    params![account_id, track_id],
                )?;
                let remaining_tracks: i64 = tx.query_row(
                    "SELECT COUNT(*) FROM jobs_tracks WHERE account_id = ?1",
                    params![account_id],
                    |row| row.get(0),
                )?;
                if remaining_tracks == 0 {
                    tx.execute(
                        "DELETE FROM jobs_discovery_sources
                          WHERE account_id = ?1 AND provider = ?2
                            AND source_key = ?3 AND track_id = ''",
                        params![
                            account_id,
                            CURATED_DISCOVERY_PROVIDER,
                            CURATED_DISCOVERY_SOURCE_KEY,
                        ],
                    )?;
                }
            }
            tx.commit()?;
            Ok(exists)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            lock_discovery_account_postgres(&mut tx, account_id)?;
            lock_account_policy_inputs_postgres(&mut tx, account_id, true)?;
            require_track_mutation_unleased_postgres(&mut tx, account_id, track_id)?;
            let exists = tx
                .query_opt(
                    "SELECT 1 FROM jobs_tracks WHERE account_id = $1 AND id = $2 FOR UPDATE",
                    &[&account_id, &track_id],
                )?
                .is_some();
            if exists {
                let source_count: i64 = tx.query_one(
                    "SELECT COUNT(*) FROM jobs_discovery_sources WHERE account_id = $1 AND track_id = $2",
                    &[&account_id, &track_id],
                )?.get(0);
                let bound_posting = tx
                    .query(
                        "SELECT posting_json FROM jobs_postings WHERE account_id = $1",
                        &[&account_id],
                    )?
                    .into_iter()
                    .map(|row| parse_json::<JobPosting>(row.get(0), "job posting"))
                    .collect::<Result<Vec<_>>>()?
                    .iter()
                    .any(|posting| posting.track_id == track_id);
                if source_count > 0 || bound_posting {
                    anyhow::bail!("Career Track still has Jobs matches or discovery sources")
                }
                tx.execute(
                    "DELETE FROM jobs_tracks WHERE account_id = $1 AND id = $2",
                    &[&account_id, &track_id],
                )?;
                let remaining_tracks: i64 = tx
                    .query_one(
                        "SELECT COUNT(*) FROM jobs_tracks WHERE account_id = $1",
                        &[&account_id],
                    )?
                    .get(0);
                if remaining_tracks == 0 {
                    tx.execute(
                        "DELETE FROM jobs_discovery_sources
                          WHERE account_id = $1 AND provider = $2
                            AND source_key = $3 AND track_id = ''",
                        &[
                            &account_id,
                            &CURATED_DISCOVERY_PROVIDER,
                            &CURATED_DISCOVERY_SOURCE_KEY,
                        ],
                    )?;
                }
            }
            tx.commit()?;
            Ok(exists)
        }
    })
}

pub fn list_postings(pool: &DbPool, account_id: &str) -> Result<Vec<JobPosting>> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get()?;
            let mut stmt = conn.prepare(
                "SELECT posting_json FROM jobs_postings
                  WHERE account_id = ?1
                  ORDER BY match_score DESC, updated_at_ms DESC",
            )?;
            let raws = stmt
                .query_map(params![account_id], |row| row.get::<_, String>(0))?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            raws.into_iter()
                .map(|raw| parse_json(raw, "job posting"))
                .collect()
        }
        DbPool::Postgres(_) => pool
            .get_pg()?
            .query(
                "SELECT posting_json FROM jobs_postings
                  WHERE account_id = $1
                  ORDER BY match_score DESC, updated_at_ms DESC",
                &[&account_id],
            )?
            .into_iter()
            .map(|row| parse_json(row.get(0), "job posting"))
            .collect(),
    })
}

pub fn get_posting(pool: &DbPool, account_id: &str, job_id: &str) -> Result<Option<JobPosting>> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get()?;
            let raw: Option<String> = conn
                .query_row(
                    "SELECT posting_json FROM jobs_postings WHERE account_id = ?1 AND id = ?2",
                    params![account_id, job_id],
                    |row| row.get(0),
                )
                .optional()?;
            raw.map(|value| parse_json(value, "job posting"))
                .transpose()
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            conn.query_opt(
                "SELECT posting_json FROM jobs_postings WHERE account_id = $1 AND id = $2",
                &[&account_id, &job_id],
            )?
            .map(|row| parse_json(row.get(0), "job posting"))
            .transpose()
        }
    })
}

pub fn upsert_posting(
    pool: &DbPool,
    account_id: &str,
    posting: &JobPosting,
    profile: &CareerProfile,
    preferences: &JobPreferences,
) -> Result<JobPosting> {
    let mut value = posting.clone();
    if value.id.trim().is_empty() {
        value.id = uuid::Uuid::new_v4().to_string();
    }
    value.source = if value.source.trim().is_empty() {
        "pasted_link".to_string()
    } else {
        value.source.trim().to_lowercase()
    };
    value.status = if value.status.trim().is_empty() {
        default_match_status()
    } else {
        value.status.trim().to_lowercase()
    };
    value.availability_status = if value.availability_status.trim().is_empty() {
        default_active_availability()
    } else {
        value.availability_status.trim().to_lowercase()
    };
    if !matches!(
        value.availability_status.as_str(),
        "active" | "expired" | "unknown"
    ) {
        anyhow::bail!("invalid job availability status")
    }
    value.canonical_key = canonical_job_key(&value);
    let tracks = list_tracks(pool, account_id)?;
    let track = tracks.iter().find(|track| track.id == value.track_id);
    let (score, reasons, missing) = score_posting(&value, profile, preferences, track);
    value.match_score = score;
    value.matched_reasons = reasons;
    value.missing_requirements = missing;
    let now = now_ms();
    if value.created_at_ms == 0 {
        value.created_at_ms = now;
    }
    value.updated_at_ms = now;
    let applications = list_applications(pool, account_id)?;
    let reservations = list_attempt_reservations(pool, account_id)?;
    let existing_application_id = applications
        .iter()
        .find(|application| application.job_id == value.id)
        .map(|application| application.id.as_str());
    value.eligibility = Some(build_job_eligibility(
        &value,
        profile,
        preferences,
        &reservations,
        true,
        existing_application_id,
        track,
    ));
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let candidate = prepare_snapshot_posting(
                &value,
                None,
                &PostingSnapshotContext {
                    profile,
                    preferences,
                    applications: &applications,
                    reservations: &reservations,
                    track,
                    observed_at_ms: now,
                },
            )?;
            let candidate_payload = to_json(&candidate, "job posting")?;
            tx.execute(
                "INSERT INTO jobs_postings (
                    id, account_id, canonical_key, posting_json, source, canonical_url,
                    company, title, location, match_score, status, created_at_ms, updated_at_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
                 ON CONFLICT(account_id, canonical_key) DO NOTHING",
                params![
                    candidate.id,
                    account_id,
                    candidate.canonical_key,
                    candidate_payload,
                    candidate.source,
                    candidate.canonical_url,
                    candidate.company,
                    candidate.title,
                    candidate.location,
                    candidate.match_score,
                    candidate.status,
                    candidate.created_at_ms,
                    candidate.updated_at_ms,
                ],
            )?;
            let raw: String = tx.query_row(
                "SELECT posting_json FROM jobs_postings WHERE account_id = ?1 AND canonical_key = ?2",
                params![account_id, candidate.canonical_key],
                |row| row.get(0),
            )?;
            let actual: JobPosting = parse_json(raw, "job posting")?;
            let saved = prepare_snapshot_posting(
                &value,
                Some(actual),
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
            tx.execute(
                "UPDATE jobs_postings SET posting_json = ?3, source = ?4, canonical_url = ?5,
                    company = ?6, title = ?7, location = ?8, match_score = ?9, status = ?10,
                    updated_at_ms = ?11 WHERE account_id = ?1 AND canonical_key = ?2",
                params![
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
                    saved.updated_at_ms,
                ],
            )?;
            tx.commit()?;
            Ok(saved)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            lock_discovery_account_postgres(&mut tx, account_id)?;
            let candidate = prepare_snapshot_posting(
                &value,
                None,
                &PostingSnapshotContext {
                    profile,
                    preferences,
                    applications: &applications,
                    reservations: &reservations,
                    track,
                    observed_at_ms: now,
                },
            )?;
            let candidate_payload = to_json(&candidate, "job posting")?;
            tx.execute(
                "INSERT INTO jobs_postings (
                    id, account_id, canonical_key, posting_json, source, canonical_url,
                    company, title, location, match_score, status, created_at_ms, updated_at_ms
                 ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
                 ON CONFLICT(account_id, canonical_key) DO NOTHING",
                &[
                    &candidate.id,
                    &account_id,
                    &candidate.canonical_key,
                    &candidate_payload,
                    &candidate.source,
                    &candidate.canonical_url,
                    &candidate.company,
                    &candidate.title,
                    &candidate.location,
                    &candidate.match_score,
                    &candidate.status,
                    &candidate.created_at_ms,
                    &candidate.updated_at_ms,
                ],
            )?;
            let actual: JobPosting = parse_json(
                tx.query_one(
                    "SELECT posting_json FROM jobs_postings
                      WHERE account_id = $1 AND canonical_key = $2 FOR UPDATE",
                    &[&account_id, &candidate.canonical_key],
                )?
                .get(0),
                "job posting",
            )?;
            let saved = prepare_snapshot_posting(
                &value,
                Some(actual),
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
            tx.execute(
                "UPDATE jobs_postings SET posting_json = $3, source = $4, canonical_url = $5,
                    company = $6, title = $7, location = $8, match_score = $9, status = $10,
                    updated_at_ms = $11 WHERE account_id = $1 AND canonical_key = $2",
                &[
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
                    &saved.updated_at_ms,
                ],
            )?;
            tx.commit()?;
            Ok(saved)
        }
    })
}

/// Build the posting document used by the atomic discovery publisher. Database
/// reads happen inside that publisher's transaction; this pure step keeps the
/// same canonical-key, scoring, eligibility, and cross-track rules as normal
/// job saves without opening a nested connection.
struct PostingSnapshotContext<'a> {
    profile: &'a CareerProfile,
    preferences: &'a JobPreferences,
    applications: &'a [JobApplication],
    reservations: &'a [AttemptReservation],
    track: Option<&'a CareerTrack>,
    observed_at_ms: i64,
}

fn prepare_snapshot_posting(
    posting: &JobPosting,
    existing: Option<JobPosting>,
    context: &PostingSnapshotContext<'_>,
) -> Result<JobPosting> {
    let mut value = posting.clone();
    if value.id.trim().is_empty() {
        value.id = uuid::Uuid::new_v4().to_string();
    }
    value.source = if value.source.trim().is_empty() {
        "pasted_link".to_string()
    } else {
        value.source.trim().to_lowercase()
    };
    value.status = if value.status.trim().is_empty() {
        default_match_status()
    } else {
        value.status.trim().to_lowercase()
    };
    value.availability_status = if value.availability_status.trim().is_empty() {
        default_active_availability()
    } else {
        value.availability_status.trim().to_lowercase()
    };
    if !matches!(
        value.availability_status.as_str(),
        "active" | "expired" | "unknown"
    ) {
        anyhow::bail!("invalid job availability status")
    }
    value.canonical_key = canonical_job_key(&value);
    if let Some(existing) = existing {
        if existing.track_id != value.track_id {
            anyhow::bail!("job is already bound to another Career Track")
        }
        value.id = existing.id;
        value.created_at_ms = existing.created_at_ms;
        if value.posted_at_ms.is_none() {
            value.posted_at_ms = existing.posted_at_ms;
        }
    }
    let (score, reasons, missing) =
        score_posting(&value, context.profile, context.preferences, context.track);
    value.match_score = score;
    value.matched_reasons = reasons;
    value.missing_requirements = missing;
    if value.created_at_ms == 0 {
        value.created_at_ms = context.observed_at_ms;
    }
    value.updated_at_ms = context.observed_at_ms;
    let existing_application_id = context
        .applications
        .iter()
        .find(|application| application.job_id == value.id)
        .map(|application| application.id.as_str());
    value.eligibility = Some(build_job_eligibility(
        &value,
        context.profile,
        context.preferences,
        context.reservations,
        true,
        existing_application_id,
        context.track,
    ));
    Ok(value)
}

#[cfg(test)]
mod profile_postings_p3_tests {
    use super::*;

    #[test]
    fn unapproved_track_reason_codes_are_persisted() {
        let path = std::env::temp_dir().join(format!(
            "bluey-track-reasons-test-{}-{}.sqlite3",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let pool = crate::db::open_pool(&path).expect("open Track reason test pool");
        crate::db::run_migrations(&pool).expect("migrate Track reason test pool");
        pool.get()
            .expect("Track reason test connection")
            .execute(
                "INSERT INTO accounts (id, email, password_hash, trial_seconds_remaining)
                 VALUES ('acct-track-reasons', 'track-reasons@example.com', 'hash', 0)",
                [],
            )
            .expect("insert Track reason test account");

        let track = upsert_track(
            &pool,
            "acct-track-reasons",
            &CareerTrack {
                id: "track-needs-review".to_string(),
                name: "Software engineering".to_string(),
                role: "Software Engineer".to_string(),
                locations: vec!["New York, NY".to_string()],
                remote_preference: "hybrid_ok".to_string(),
                application_identity_id: None,
                policy: CareerTrackPolicy::default(),
                active: true,
                match_count: 0,
                created_at_ms: 0,
                updated_at_ms: 0,
            },
        )
        .expect("save unapproved Career Track");
        let expected = vec![
            "application_identity_required".to_string(),
            "source_resume_review_required".to_string(),
        ];
        assert_eq!(track.policy.authority.review_state, "needs_review");
        assert_eq!(track.policy.authority.review_reason_codes, expected);

        let raw: String = pool
            .get()
            .expect("stored Track connection")
            .query_row(
                "SELECT track_json FROM jobs_tracks
                  WHERE account_id = ?1 AND id = ?2",
                params!["acct-track-reasons", "track-needs-review"],
                |row| row.get(0),
            )
            .expect("load stored Track payload");
        let stored: CareerTrack =
            parse_json(raw, "stored unapproved Career Track").expect("parse stored Track payload");
        assert_eq!(stored.policy.authority.review_state, "needs_review");
        assert_eq!(stored.policy.authority.review_reason_codes, expected);

        let listed = list_tracks(&pool, "acct-track-reasons")
            .expect("list unapproved Career Track")
            .remove(0);
        assert_eq!(listed.policy.authority.review_reason_codes, expected);
    }
}
