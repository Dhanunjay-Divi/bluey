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
            let conn = pool.get()?;
            conn.execute(
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
            Ok(value)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let onboarding_complete = i32::from(value.onboarding_complete);
            conn.execute(
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
            let conn = pool.get()?;
            conn.execute(
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
            Ok(value)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            conn.execute(
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
        DbPool::Sqlite(_) => Ok(pool.get()?.execute(
            "DELETE FROM jobs_facts WHERE account_id = ?1 AND id = ?2",
            params![account_id, fact_id],
        )? > 0),
        DbPool::Postgres(_) => Ok(pool.get_pg()?.execute(
            "DELETE FROM jobs_facts WHERE account_id = $1 AND id = $2",
            &[&account_id, &fact_id],
        )? > 0),
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
            pool.get()?.execute(
                "INSERT INTO jobs_preferences(account_id, preferences_json, updated_at_ms)
                 VALUES (?1, ?2, ?3)
                 ON CONFLICT(account_id) DO UPDATE SET
                    preferences_json = excluded.preferences_json,
                    updated_at_ms = excluded.updated_at_ms",
                params![account_id, payload, value.updated_at_ms],
            )?;
            Ok(value)
        }
        DbPool::Postgres(_) => {
            pool.get_pg()?.execute(
                "INSERT INTO jobs_preferences(account_id, preferences_json, updated_at_ms)
                 VALUES ($1, $2, $3)
                 ON CONFLICT(account_id) DO UPDATE SET
                    preferences_json = EXCLUDED.preferences_json,
                    updated_at_ms = EXCLUDED.updated_at_ms",
                &[&account_id, &payload, &value.updated_at_ms],
            )?;
            Ok(value)
        }
    })
}

pub fn list_tracks(pool: &DbPool, account_id: &str) -> Result<Vec<CareerTrack>> {
    crate::db::run_blocking_db(|| {
        match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get()?;
            let mut stmt = conn.prepare(
                "SELECT track_json FROM jobs_tracks WHERE account_id = ?1 ORDER BY active DESC, updated_at_ms DESC",
            )?;
            let raws = stmt
                .query_map(params![account_id], |row| row.get::<_, String>(0))?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            raws.into_iter()
                .map(|raw| parse_json(raw, "Career Track"))
                .collect()
        }
        DbPool::Postgres(_) => pool
            .get_pg()?
            .query(
                "SELECT track_json FROM jobs_tracks WHERE account_id = $1 ORDER BY active DESC, updated_at_ms DESC",
                &[&account_id],
            )?
            .into_iter()
            .map(|row| parse_json(row.get(0), "Career Track"))
            .collect(),
    }
    })
}

pub fn upsert_track(pool: &DbPool, account_id: &str, track: &CareerTrack) -> Result<CareerTrack> {
    let mut value = track.clone();
    if let Some(identity_id) = value.application_identity_id.as_deref() {
        let identity = get_application_identity(pool, account_id, identity_id)?
            .ok_or_else(|| anyhow::anyhow!("application email not found"))?;
        if identity.verification_status != "verified" {
            anyhow::bail!("verify the application email before using it on a Career Track")
        }
    }
    if value.id.trim().is_empty() {
        value.id = uuid::Uuid::new_v4().to_string();
    }
    let now = now_ms();
    if value.created_at_ms == 0 {
        value.created_at_ms = now;
    }
    value.updated_at_ms = now;
    let payload = to_json(&value, "Career Track")?;
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            pool.get()?.execute(
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
            Ok(value)
        }
        DbPool::Postgres(_) => {
            let active = i32::from(value.active);
            pool.get_pg()?.execute(
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
            Ok(value)
        }
    })
}

pub fn delete_track(pool: &DbPool, account_id: &str, track_id: &str) -> Result<bool> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
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
            }
            tx.commit()?;
            Ok(exists)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            lock_discovery_account_postgres(&mut tx, account_id)?;
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
    if value.match_score == 0 {
        let (score, reasons, missing) = score_posting(&value, profile, preferences);
        value.match_score = score;
        value.matched_reasons = reasons;
        value.missing_requirements = missing;
    }
    let now = now_ms();
    if value.created_at_ms == 0 {
        value.created_at_ms = now;
    }
    if value.availability_status == "active" && value.last_verified_at_ms.is_none() {
        value.last_verified_at_ms = Some(now);
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
    ));
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let candidate = prepare_snapshot_posting(
                &value,
                None,
                profile,
                preferences,
                &applications,
                &reservations,
                now,
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
                profile,
                preferences,
                &applications,
                &reservations,
                now,
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
                profile,
                preferences,
                &applications,
                &reservations,
                now,
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
                profile,
                preferences,
                &applications,
                &reservations,
                now,
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
fn prepare_snapshot_posting(
    posting: &JobPosting,
    existing: Option<JobPosting>,
    profile: &CareerProfile,
    preferences: &JobPreferences,
    applications: &[JobApplication],
    reservations: &[AttemptReservation],
    observed_at_ms: i64,
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
    if value.match_score == 0 {
        let (score, reasons, missing) = score_posting(&value, profile, preferences);
        value.match_score = score;
        value.matched_reasons = reasons;
        value.missing_requirements = missing;
    }
    if value.created_at_ms == 0 {
        value.created_at_ms = observed_at_ms;
    }
    if value.availability_status == "active" && value.last_verified_at_ms.is_none() {
        value.last_verified_at_ms = Some(observed_at_ms);
    }
    value.updated_at_ms = observed_at_ms;
    let existing_application_id = applications
        .iter()
        .find(|application| application.job_id == value.id)
        .map(|application| application.id.as_str());
    value.eligibility = Some(build_job_eligibility(
        &value,
        profile,
        preferences,
        reservations,
        true,
        existing_application_id,
    ));
    Ok(value)
}
