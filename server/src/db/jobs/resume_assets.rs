pub fn get_resume_source_asset(
    pool: &DbPool,
    account_id: &str,
) -> Result<Option<ResumeSourceAsset>> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get()?;
            conn.query_row(
                "SELECT id, file_name, media_type, file_type, storage_key, sha256,
                        size_bytes, page_count, template_status, created_at_ms, updated_at_ms
                   FROM jobs_resume_source_assets WHERE account_id = ?1",
                params![account_id],
                resume_source_asset_from_sqlite_row,
            )
            .optional()
            .context("get source resume asset")
        }
        DbPool::Postgres(_) => pool
            .get_pg()?
            .query_opt(
                "SELECT id, file_name, media_type, file_type, storage_key, sha256,
                        size_bytes, page_count, template_status, created_at_ms, updated_at_ms
                   FROM jobs_resume_source_assets WHERE account_id = $1",
                &[&account_id],
            )?
            .map(resume_source_asset_from_pg_row)
            .transpose(),
    })
}

/// Read the profile representation and its optimistic-concurrency revision in
/// one database snapshot before a source-resume upload begins object I/O.
pub fn get_resume_upload_profile(
    pool: &DbPool,
    account_id: &str,
    email: &str,
) -> Result<(CareerProfile, Option<String>)> {
    crate::db::run_blocking_db(|| {
        let raw = match pool {
            DbPool::Sqlite(_) => {
                let conn = pool.get()?;
                conn.query_row(
                    "SELECT profile_json FROM jobs_profiles WHERE account_id = ?1",
                    params![account_id],
                    |row| row.get::<_, String>(0),
                )
                .optional()?
            }
            DbPool::Postgres(_) => pool
                .get_pg()?
                .query_opt(
                    "SELECT profile_json FROM jobs_profiles WHERE account_id = $1",
                    &[&account_id],
                )?
                .map(|row| row.get::<_, String>(0)),
        };
        let mut profile = raw
            .as_deref()
            .map(|value| parse_json(value.to_string(), "Jobs profile"))
            .transpose()?
            .unwrap_or_else(|| default_profile(email));
        profile.auto_submit_threshold = default_auto_submit_threshold();
        profile.daily_limit = default_daily_limit();
        let revision = raw
            .is_some()
            .then(|| resume_profile_revision(&profile))
            .transpose()?;
        Ok((profile, revision))
    })
}

pub fn save_resume_source_asset(
    pool: &DbPool,
    account_id: &str,
    asset: &ResumeSourceAsset,
    profile: &CareerProfile,
) -> Result<(Option<ResumeSourceAsset>, CareerProfile)> {
    let publication = save_resume_source_asset_internal(pool, account_id, asset, profile, None)?;
    Ok((publication.previous, publication.profile))
}

#[derive(Debug, Clone)]
pub struct ResumeSourcePublication {
    pub previous: Option<ResumeSourceAsset>,
    pub asset: ResumeSourceAsset,
    pub profile: CareerProfile,
    pub replayed: bool,
}

pub fn publish_resume_source_asset(
    pool: &DbPool,
    account_id: &str,
    asset: &ResumeSourceAsset,
    profile: &CareerProfile,
    upload_id: &str,
) -> Result<ResumeSourcePublication> {
    save_resume_source_asset_internal(pool, account_id, asset, profile, Some(upload_id))
}

fn save_resume_source_asset_internal(
    pool: &DbPool,
    account_id: &str,
    asset: &ResumeSourceAsset,
    profile: &CareerProfile,
    upload_id: Option<&str>,
) -> Result<ResumeSourcePublication> {
    let mut profile = profile.clone();
    if upload_id.is_none() {
        profile.onboarding_step = profile.onboarding_step.clamp(0, 6);
        profile.auto_submit_threshold = default_auto_submit_threshold();
        profile.daily_limit = default_daily_limit();
        profile.updated_at_ms = now_ms();
    }
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let published_upload = if let Some(upload_id) = upload_id {
                Some(crate::db::object_uploads::publish_account_object_sqlite_tx(
                    &tx,
                    upload_id,
                    account_id,
                    &asset.storage_key,
                    asset.updated_at_ms,
                )?)
            } else {
                crate::db::object_uploads::require_active_account_write_fence_sqlite_tx(
                    &tx, account_id,
                )?;
                None
            };
            let previous = tx
                .query_row(
                    "SELECT id, file_name, media_type, file_type, storage_key, sha256,
                            size_bytes, page_count, template_status, created_at_ms, updated_at_ms
                       FROM jobs_resume_source_assets WHERE account_id = ?1",
                    params![account_id],
                    resume_source_asset_from_sqlite_row,
                )
                .optional()?;
            let upload_authority = published_upload
                .as_ref()
                .map(|upload| validate_resume_source_upload(upload, asset))
                .transpose()?;
            if let Some(authority) = upload_authority.as_ref() {
                if let Some(current) = previous.as_ref().filter(|current| current.id == asset.id) {
                    if current != asset {
                        return Err(
                            crate::db::object_uploads::UploadControlError::IdempotencyConflict
                                .into(),
                        );
                    }
                    let stored_profile = load_resume_profile_sqlite_tx(&tx, account_id)?.ok_or(
                        crate::db::object_uploads::UploadControlError::IdempotencyConflict,
                    )?;
                    validate_resume_profile_binding(&stored_profile, current)?;
                    validate_current_resume_replay_semantic_ledger_sqlite(&tx, account_id)?;
                    tx.commit()?;
                    return Ok(ResumeSourcePublication {
                        previous: None,
                        asset: current.clone(),
                        profile: stored_profile,
                        replayed: true,
                    });
                }
                if previous.as_ref().map(|current| current.id.as_str())
                    != authority.expected_previous.as_deref()
                {
                    return Err(
                        crate::db::object_uploads::UploadControlError::IdempotencyConflict.into(),
                    );
                }
            }
            let current_profile = load_resume_profile_sqlite_tx(&tx, account_id)?;
            let committed_profile = match upload_authority.as_ref() {
                Some(authority) => {
                    prepare_resume_profile(current_profile.as_ref(), &profile, asset, authority)?
                }
                None => profile.clone(),
            };
            let profile_json = to_json(&committed_profile, "Jobs profile")?;
            tx.execute(
                "INSERT INTO jobs_resume_source_assets (
                    id, account_id, file_name, media_type, file_type, storage_key, sha256,
                    size_bytes, page_count, template_status, created_at_ms, updated_at_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
                 ON CONFLICT(account_id) DO UPDATE SET
                    id = excluded.id, file_name = excluded.file_name,
                    media_type = excluded.media_type, file_type = excluded.file_type,
                    storage_key = excluded.storage_key, sha256 = excluded.sha256,
                    size_bytes = excluded.size_bytes, page_count = excluded.page_count,
                    template_status = excluded.template_status,
                    created_at_ms = excluded.created_at_ms, updated_at_ms = excluded.updated_at_ms",
                params![
                    asset.id,
                    account_id,
                    asset.file_name,
                    asset.media_type,
                    asset.file_type,
                    asset.storage_key,
                    asset.sha256,
                    asset.size_bytes,
                    asset.page_count,
                    asset.template_status,
                    asset.created_at_ms,
                    asset.updated_at_ms,
                ],
            )?;
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
                    profile_json,
                    committed_profile.onboarding_step,
                    i64::from(committed_profile.onboarding_complete),
                    committed_profile.updated_at_ms,
                ],
            )?;
            if let Some(previous) = previous
                .as_ref()
                .filter(|previous| previous.storage_key != asset.storage_key)
            {
                let cleanup = crate::db::object_uploads::schedule_account_object_cleanup_sqlite_tx(
                    &tx,
                    account_id,
                    &previous.storage_key,
                    asset.updated_at_ms,
                )?;
                if cleanup.is_none() {
                    let adopted =
                        legacy_resume_source_upload(account_id, previous, asset.updated_at_ms);
                    crate::db::object_uploads::adopt_ready_account_object_sqlite_tx(&tx, &adopted)?;
                    if crate::db::object_uploads::schedule_account_object_cleanup_sqlite_tx(
                        &tx,
                        account_id,
                        &previous.storage_key,
                        asset.updated_at_ms,
                    )?
                    .is_none()
                    {
                        return Err(
                            crate::db::object_uploads::UploadControlError::IdempotencyConflict
                                .into(),
                        );
                    }
                }
            }
            advance_account_input_generation_sqlite(
                &tx,
                account_id,
                "resume",
                &asset.id,
                asset.updated_at_ms,
            )?;
            tx.commit()?;
            Ok(ResumeSourcePublication {
                previous,
                asset: asset.clone(),
                profile: committed_profile,
                replayed: false,
            })
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            lock_discovery_account_postgres(&mut tx, account_id)?;
            // The discovery-account advisory lock is the outer policy-write fence. Within it,
            // acquire the account's object-lifecycle write fence before any account child row.
            // Account deletion takes the same parent row before cascading through children, so
            // taking policy-input child locks first and later upgrading KEY SHARE to UPDATE can
            // deadlock instead of producing one serialized publication-or-deletion outcome.
            let published_upload = if let Some(upload_id) = upload_id {
                Some(
                    crate::db::object_uploads::publish_account_object_postgres_tx(
                        &mut tx,
                        upload_id,
                        account_id,
                        &asset.storage_key,
                        asset.updated_at_ms,
                    )?,
                )
            } else {
                crate::db::object_uploads::require_active_account_write_fence_postgres_tx(
                    &mut tx, account_id,
                )?;
                None
            };
            lock_account_policy_inputs_postgres(&mut tx, account_id, true)?;
            let previous = tx
                .query_opt(
                    "SELECT id, file_name, media_type, file_type, storage_key, sha256,
                            size_bytes, page_count, template_status, created_at_ms, updated_at_ms
                       FROM jobs_resume_source_assets WHERE account_id = $1 FOR UPDATE",
                    &[&account_id],
                )?
                .map(resume_source_asset_from_pg_row)
                .transpose()?;
            let upload_authority = published_upload
                .as_ref()
                .map(|upload| validate_resume_source_upload(upload, asset))
                .transpose()?;
            if let Some(authority) = upload_authority.as_ref() {
                if let Some(current) = previous.as_ref().filter(|current| current.id == asset.id) {
                    if current != asset {
                        return Err(
                            crate::db::object_uploads::UploadControlError::IdempotencyConflict
                                .into(),
                        );
                    }
                    let stored_profile = load_resume_profile_postgres_tx(&mut tx, account_id)?
                        .ok_or(
                            crate::db::object_uploads::UploadControlError::IdempotencyConflict,
                        )?;
                    validate_resume_profile_binding(&stored_profile, current)?;
                    validate_current_resume_replay_semantic_ledger_postgres(&mut tx, account_id)?;
                    tx.commit()?;
                    return Ok(ResumeSourcePublication {
                        previous: None,
                        asset: current.clone(),
                        profile: stored_profile,
                        replayed: true,
                    });
                }
                if previous.as_ref().map(|current| current.id.as_str())
                    != authority.expected_previous.as_deref()
                {
                    return Err(
                        crate::db::object_uploads::UploadControlError::IdempotencyConflict.into(),
                    );
                }
            }
            let current_profile = load_resume_profile_postgres_tx(&mut tx, account_id)?;
            let committed_profile = match upload_authority.as_ref() {
                Some(authority) => {
                    prepare_resume_profile(current_profile.as_ref(), &profile, asset, authority)?
                }
                None => profile.clone(),
            };
            let profile_json = to_json(&committed_profile, "Jobs profile")?;
            tx.execute(
                "INSERT INTO jobs_resume_source_assets (
                    id, account_id, file_name, media_type, file_type, storage_key, sha256,
                    size_bytes, page_count, template_status, created_at_ms, updated_at_ms
                 ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
                 ON CONFLICT(account_id) DO UPDATE SET
                    id = EXCLUDED.id, file_name = EXCLUDED.file_name,
                    media_type = EXCLUDED.media_type, file_type = EXCLUDED.file_type,
                    storage_key = EXCLUDED.storage_key, sha256 = EXCLUDED.sha256,
                    size_bytes = EXCLUDED.size_bytes, page_count = EXCLUDED.page_count,
                    template_status = EXCLUDED.template_status,
                    created_at_ms = EXCLUDED.created_at_ms, updated_at_ms = EXCLUDED.updated_at_ms",
                &[
                    &asset.id,
                    &account_id,
                    &asset.file_name,
                    &asset.media_type,
                    &asset.file_type,
                    &asset.storage_key,
                    &asset.sha256,
                    &asset.size_bytes,
                    &asset.page_count,
                    &asset.template_status,
                    &asset.created_at_ms,
                    &asset.updated_at_ms,
                ],
            )?;
            let onboarding_complete = i32::from(committed_profile.onboarding_complete);
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
                    &profile_json,
                    &committed_profile.onboarding_step,
                    &onboarding_complete,
                    &committed_profile.updated_at_ms,
                ],
            )?;
            if let Some(previous) = previous
                .as_ref()
                .filter(|previous| previous.storage_key != asset.storage_key)
            {
                let cleanup =
                    crate::db::object_uploads::schedule_account_object_cleanup_postgres_tx(
                        &mut tx,
                        account_id,
                        &previous.storage_key,
                        asset.updated_at_ms,
                    )?;
                if cleanup.is_none() {
                    let adopted =
                        legacy_resume_source_upload(account_id, previous, asset.updated_at_ms);
                    crate::db::object_uploads::adopt_ready_account_object_postgres_tx(
                        &mut tx, &adopted,
                    )?;
                    if crate::db::object_uploads::schedule_account_object_cleanup_postgres_tx(
                        &mut tx,
                        account_id,
                        &previous.storage_key,
                        asset.updated_at_ms,
                    )?
                    .is_none()
                    {
                        return Err(
                            crate::db::object_uploads::UploadControlError::IdempotencyConflict
                                .into(),
                        );
                    }
                }
            }
            advance_account_input_generation_postgres(
                &mut tx,
                account_id,
                "resume",
                &asset.id,
                asset.updated_at_ms,
            )?;
            tx.commit()?;
            Ok(ResumeSourcePublication {
                previous,
                asset: asset.clone(),
                profile: committed_profile,
                replayed: false,
            })
        }
    })
}

struct ResumeUploadAuthority {
    expected_previous: Option<String>,
    profile_mode: String,
    base_profile_sha256: Option<String>,
    requested_profile_sha256: Option<String>,
}

fn validate_resume_source_upload(
    upload: &crate::db::object_uploads::ObjectUpload,
    asset: &ResumeSourceAsset,
) -> Result<ResumeUploadAuthority> {
    let metadata = serde_json::from_str::<Value>(&upload.metadata_json)
        .context("parse resume source upload metadata")?;
    let Some(metadata) = metadata.as_object() else {
        return Err(crate::db::object_uploads::UploadControlError::IdempotencyConflict.into());
    };
    let base_profile_sha256 = match metadata.get("base_profile_sha256") {
        Some(Value::Null) => None,
        Some(Value::String(value)) => {
            let valid = value.len() == 64
                && value
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte));
            if !valid {
                return Err(
                    crate::db::object_uploads::UploadControlError::IdempotencyConflict.into(),
                );
            }
            Some(value.clone())
        }
        _ => return Err(crate::db::object_uploads::UploadControlError::IdempotencyConflict.into()),
    };
    let profile_mode = metadata
        .get("profile_mode")
        .and_then(Value::as_str)
        .filter(|value| matches!(*value, "replace" | "merge_source"))
        .ok_or(crate::db::object_uploads::UploadControlError::IdempotencyConflict)?;
    if profile_mode == "merge_source" && base_profile_sha256.is_some() {
        return Err(crate::db::object_uploads::UploadControlError::IdempotencyConflict.into());
    }
    let requested_profile_sha256 = match metadata.get("requested_profile_sha256") {
        Some(Value::Null) => None,
        Some(Value::String(value)) => {
            let valid = value.len() == 64
                && value
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte));
            if !valid {
                return Err(
                    crate::db::object_uploads::UploadControlError::IdempotencyConflict.into(),
                );
            }
            Some(value.clone())
        }
        _ => return Err(crate::db::object_uploads::UploadControlError::IdempotencyConflict.into()),
    };
    if (profile_mode == "replace" && requested_profile_sha256.is_none())
        || (profile_mode == "merge_source" && requested_profile_sha256.is_some())
    {
        return Err(crate::db::object_uploads::UploadControlError::IdempotencyConflict.into());
    }
    let expected_previous = match metadata.get("replaces_source_asset_id") {
        Some(Value::Null) => None,
        Some(Value::String(value)) if !value.is_empty() => Some(value.clone()),
        _ => return Err(crate::db::object_uploads::UploadControlError::IdempotencyConflict.into()),
    };
    if metadata.len() != 12
        || metadata.get("artifact_class").and_then(Value::as_str) != Some("jobs_resume_source")
        || metadata
            .get("jobs_resume_source_asset_id")
            .and_then(Value::as_str)
            != Some(asset.id.as_str())
        || metadata.get("request_id").and_then(Value::as_str) != Some(asset.id.as_str())
        || metadata.get("file_name").and_then(Value::as_str) != Some(asset.file_name.as_str())
        || metadata.get("file_type").and_then(Value::as_str) != Some(asset.file_type.as_str())
        || metadata.get("media_type").and_then(Value::as_str) != Some(asset.media_type.as_str())
        || metadata.get("page_count").and_then(Value::as_i64) != asset.page_count
        || metadata.get("retention_policy").and_then(Value::as_str)
            != Some("account_lifetime_until_deletion")
        || upload.logical_id != format!("jobs-resume-source:{}", asset.id)
        || upload.object_key != asset.storage_key
        || !upload.sha256.eq_ignore_ascii_case(&asset.sha256)
        || upload.size_bytes != asset.size_bytes
        || upload.content_type != asset.media_type
        || upload.expires_at_ms != i64::MAX
        || upload.state != "ready"
    {
        return Err(crate::db::object_uploads::UploadControlError::IdempotencyConflict.into());
    }
    Ok(ResumeUploadAuthority {
        expected_previous,
        profile_mode: profile_mode.to_string(),
        base_profile_sha256,
        requested_profile_sha256,
    })
}

fn prepare_resume_profile(
    current: Option<&CareerProfile>,
    requested: &CareerProfile,
    asset: &ResumeSourceAsset,
    authority: &ResumeUploadAuthority,
) -> Result<CareerProfile> {
    let mut profile = match authority.profile_mode.as_str() {
        "replace" => {
            let current_revision = current.map(resume_profile_revision).transpose()?;
            let requested_revision = resume_requested_profile_sha256(requested)?;
            if current_revision.as_deref() != authority.base_profile_sha256.as_deref()
                || Some(requested_revision.as_str())
                    != authority.requested_profile_sha256.as_deref()
            {
                return Err(
                    crate::db::object_uploads::UploadControlError::IdempotencyConflict.into(),
                );
            }
            requested.clone()
        }
        "merge_source" => current.cloned().unwrap_or_else(|| requested.clone()),
        _ => return Err(crate::db::object_uploads::UploadControlError::IdempotencyConflict.into()),
    };
    profile.source_resume_name = asset.file_name.clone();
    profile.source_resume_asset_id = asset.id.clone();
    profile.source_resume_sha256 = asset.sha256.clone();
    profile.source_resume_media_type = asset.media_type.clone();
    profile.source_resume_template_status = asset.template_status.clone();
    profile.onboarding_step = profile.onboarding_step.clamp(0, 6);
    profile.auto_submit_threshold = default_auto_submit_threshold();
    profile.daily_limit = default_daily_limit();
    profile.updated_at_ms = now_ms()
        .max(asset.updated_at_ms)
        .max(current.map_or(0, |profile| profile.updated_at_ms));
    Ok(profile)
}

fn resume_profile_revision(profile: &CareerProfile) -> Result<String> {
    let mut canonical = profile.clone();
    canonical.auto_submit_threshold = default_auto_submit_threshold();
    canonical.daily_limit = default_daily_limit();
    Ok(hex::encode(Sha256::digest(serde_json::to_vec(&canonical)?)))
}

fn resume_requested_profile_sha256(profile: &CareerProfile) -> Result<String> {
    Ok(hex::encode(Sha256::digest(serde_json::to_vec(profile)?)))
}

fn validate_resume_profile_binding(
    profile: &CareerProfile,
    asset: &ResumeSourceAsset,
) -> Result<()> {
    if profile.source_resume_asset_id != asset.id
        || profile.source_resume_name != asset.file_name
        || !profile
            .source_resume_sha256
            .eq_ignore_ascii_case(&asset.sha256)
        || profile.source_resume_media_type != asset.media_type
        || profile.source_resume_template_status != asset.template_status
    {
        return Err(crate::db::object_uploads::UploadControlError::IdempotencyConflict.into());
    }
    Ok(())
}

fn validate_current_resume_replay_semantic_ledger_sqlite(
    conn: &rusqlite::Connection,
    account_id: &str,
) -> Result<()> {
    let head = validate_account_input_head_sqlite(conn, account_id)
        .context("validate exact resume replay semantic-input head")?;
    let current_semantic_sha256 = account_semantic_sha256_sqlite(conn, account_id)?;
    anyhow::ensure!(
        head.semantic_sha256.as_deref() == Some(current_semantic_sha256.as_str()),
        "account semantic-input generation is stale"
    );
    Ok(())
}

fn validate_current_resume_replay_semantic_ledger_postgres<C: postgres::GenericClient>(
    client: &mut C,
    account_id: &str,
) -> Result<()> {
    let head = validate_account_input_head_postgres(client, account_id)
        .context("validate exact resume replay semantic-input head")?;
    let current_semantic_sha256 = account_semantic_sha256_postgres(client, account_id)?;
    anyhow::ensure!(
        head.semantic_sha256.as_deref() == Some(current_semantic_sha256.as_str()),
        "account semantic-input generation is stale"
    );
    Ok(())
}

fn load_resume_profile_sqlite_tx(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
) -> Result<Option<CareerProfile>> {
    let raw = tx
        .query_row(
            "SELECT profile_json FROM jobs_profiles WHERE account_id = ?1",
            params![account_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    raw.map(|value| parse_json(value, "Jobs profile"))
        .transpose()
}

fn load_resume_profile_postgres_tx(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
) -> Result<Option<CareerProfile>> {
    let raw = tx
        .query_opt(
            "SELECT profile_json FROM jobs_profiles WHERE account_id = $1 FOR SHARE",
            &[&account_id],
        )?
        .map(|row| row.get::<_, String>(0));
    raw.map(|value| parse_json(value, "Jobs profile"))
        .transpose()
}

fn legacy_resume_source_upload(
    account_id: &str,
    asset: &ResumeSourceAsset,
    now_ms: i64,
) -> crate::db::object_uploads::NewObjectUpload {
    crate::db::object_uploads::NewObjectUpload {
        account_id: account_id.to_string(),
        object_kind: crate::db::object_uploads::ObjectKind::Artifact,
        logical_id: format!("jobs-resume-source:{}", asset.id),
        session_id: None,
        storage_scope: crate::db::object_uploads::StorageScope::Artifact,
        object_key: asset.storage_key.clone(),
        size_bytes: asset.size_bytes,
        sha256: asset.sha256.to_ascii_lowercase(),
        content_type: asset.media_type.clone(),
        expires_at_ms: i64::MAX,
        metadata_json: json!({
            "artifact_class": "jobs_resume_source",
            "jobs_resume_source_asset_id": asset.id,
            "file_name": asset.file_name,
            "file_type": asset.file_type,
            "media_type": asset.media_type,
            "page_count": asset.page_count,
            "retention_policy": "account_lifetime_until_deletion",
        }),
        now_ms,
        limits: crate::object_storage::UploadLimits {
            max_object_bytes: i64::MAX,
            max_account_bytes: i64::MAX,
            max_daily_bytes: i64::MAX,
            max_account_objects: i64::MAX,
        },
    }
}

fn resume_source_asset_from_sqlite_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<ResumeSourceAsset> {
    Ok(ResumeSourceAsset {
        id: row.get(0)?,
        file_name: row.get(1)?,
        media_type: row.get(2)?,
        file_type: row.get(3)?,
        storage_key: row.get(4)?,
        sha256: row.get(5)?,
        size_bytes: row.get(6)?,
        page_count: row.get(7)?,
        template_status: row.get(8)?,
        created_at_ms: row.get(9)?,
        updated_at_ms: row.get(10)?,
    })
}

fn resume_source_asset_from_pg_row(row: postgres::Row) -> Result<ResumeSourceAsset> {
    Ok(ResumeSourceAsset {
        id: row.try_get(0)?,
        file_name: row.try_get(1)?,
        media_type: row.try_get(2)?,
        file_type: row.try_get(3)?,
        storage_key: row.try_get(4)?,
        sha256: row.try_get(5)?,
        size_bytes: row.try_get(6)?,
        page_count: row.try_get(7)?,
        template_status: row.try_get(8)?,
        created_at_ms: row.try_get(9)?,
        updated_at_ms: row.try_get(10)?,
    })
}

#[cfg(test)]
mod resume_assets_p3_tests {
    use super::*;

    #[test]
    fn exact_resume_replay_requires_current_semantic_input_ledger() {
        let path = std::env::temp_dir().join(format!(
            "bluey-resume-replay-ledger-test-{}-{}.sqlite3",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let pool = crate::db::open_pool(&path).expect("open resume replay ledger test pool");
        crate::db::run_migrations(&pool).expect("migrate resume replay ledger test pool");
        pool.get()
            .expect("resume replay ledger test connection")
            .execute(
                "INSERT INTO accounts (id, email, password_hash, trial_seconds_remaining)
                 VALUES ('acct-resume-replay', 'resume-replay@example.com', 'hash', 0)",
                [],
            )
            .expect("insert resume replay ledger test account");

        let mut requested_profile = default_profile("resume-replay@example.com");
        requested_profile.headline = "Original profile".to_string();
        requested_profile.onboarding_complete = true;
        let requested_profile_sha256 =
            resume_requested_profile_sha256(&requested_profile).expect("hash requested profile");
        let now = now_ms();
        let asset_id = "resume-replay-ledger";
        let logical_id = format!("jobs-resume-source:{asset_id}");
        let object_key = format!("accounts/acct-resume-replay/jobs/resume-sources/{asset_id}.pdf");
        let reservation = crate::db::object_uploads::reserve_account_object_upload(
            &pool,
            &crate::db::object_uploads::NewObjectUpload {
                account_id: "acct-resume-replay".to_string(),
                object_kind: crate::db::object_uploads::ObjectKind::Artifact,
                logical_id: logical_id.clone(),
                session_id: None,
                storage_scope: crate::db::object_uploads::StorageScope::Artifact,
                object_key: object_key.clone(),
                size_bytes: 128,
                sha256: "a".repeat(64),
                content_type: "application/pdf".to_string(),
                expires_at_ms: i64::MAX,
                metadata_json: json!({
                    "artifact_class": "jobs_resume_source",
                    "jobs_resume_source_asset_id": asset_id,
                    "request_id": asset_id,
                    "profile_mode": "replace",
                    "base_profile_sha256": null,
                    "requested_profile_sha256": requested_profile_sha256,
                    "replaces_source_asset_id": null,
                    "file_name": "resume.pdf",
                    "file_type": "pdf",
                    "media_type": "application/pdf",
                    "page_count": 2,
                    "retention_policy": "account_lifetime_until_deletion",
                }),
                now_ms: now,
                limits: crate::object_storage::UploadLimits {
                    max_object_bytes: 1024 * 1024,
                    max_account_bytes: 16 * 1024 * 1024,
                    max_daily_bytes: 16 * 1024 * 1024,
                    max_account_objects: 100,
                },
            },
        )
        .expect("reserve resume replay upload");
        crate::db::object_uploads::begin_upload_put(&pool, &reservation.upload.id, now)
            .expect("begin resume replay upload");
        crate::db::object_uploads::release_verified_upload_put(&pool, &reservation.upload.id, now)
            .expect("verify resume replay upload");
        let asset = ResumeSourceAsset {
            id: asset_id.to_string(),
            file_name: "resume.pdf".to_string(),
            media_type: "application/pdf".to_string(),
            file_type: "pdf".to_string(),
            storage_key: object_key,
            sha256: "a".repeat(64),
            size_bytes: 128,
            page_count: Some(2),
            template_status: "converted_layout".to_string(),
            created_at_ms: reservation.upload.created_at_ms,
            updated_at_ms: reservation.upload.created_at_ms,
        };

        let first = publish_resume_source_asset(
            &pool,
            "acct-resume-replay",
            &asset,
            &requested_profile,
            &reservation.upload.id,
        )
        .expect("publish source resume");
        assert!(!first.replayed);
        let replay = publish_resume_source_asset(
            &pool,
            "acct-resume-replay",
            &asset,
            &requested_profile,
            &reservation.upload.id,
        )
        .expect("replay source resume against current ledger");
        assert!(replay.replayed);

        let mut tampered_profile = first.profile;
        tampered_profile.headline = "Unrecorded semantic edit".to_string();
        let tampered_payload =
            to_json(&tampered_profile, "tampered resume replay profile").expect("serialize tamper");
        pool.get()
            .expect("tamper connection")
            .execute(
                "UPDATE jobs_profiles SET profile_json = ?2 WHERE account_id = ?1",
                params!["acct-resume-replay", tampered_payload],
            )
            .expect("tamper profile without advancing its semantic ledger");

        let error = publish_resume_source_asset(
            &pool,
            "acct-resume-replay",
            &asset,
            &requested_profile,
            &reservation.upload.id,
        )
        .expect_err("exact replay must reject a stale semantic-input ledger");
        assert!(
            format!("{error:#}").contains("account semantic-input generation is stale"),
            "unexpected exact replay error: {error:#}"
        );
    }
}
