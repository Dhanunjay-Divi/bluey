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

pub fn save_resume_source_asset(
    pool: &DbPool,
    account_id: &str,
    asset: &ResumeSourceAsset,
    profile: &CareerProfile,
) -> Result<(Option<ResumeSourceAsset>, CareerProfile)> {
    let mut profile = profile.clone();
    profile.onboarding_step = profile.onboarding_step.clamp(0, 6);
    profile.auto_submit_threshold = default_auto_submit_threshold();
    profile.daily_limit = default_daily_limit();
    profile.updated_at_ms = now_ms();
    let profile_json = to_json(&profile, "Jobs profile")?;
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let previous = tx
                .query_row(
                    "SELECT id, file_name, media_type, file_type, storage_key, sha256,
                            size_bytes, page_count, template_status, created_at_ms, updated_at_ms
                       FROM jobs_resume_source_assets WHERE account_id = ?1",
                    params![account_id],
                    resume_source_asset_from_sqlite_row,
                )
                .optional()?;
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
                    profile.onboarding_step,
                    i64::from(profile.onboarding_complete),
                    profile.updated_at_ms,
                ],
            )?;
            tx.commit()?;
            Ok((previous, profile))
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            let previous = tx
                .query_opt(
                    "SELECT id, file_name, media_type, file_type, storage_key, sha256,
                            size_bytes, page_count, template_status, created_at_ms, updated_at_ms
                       FROM jobs_resume_source_assets WHERE account_id = $1 FOR UPDATE",
                    &[&account_id],
                )?
                .map(resume_source_asset_from_pg_row)
                .transpose()?;
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
            let onboarding_complete = i32::from(profile.onboarding_complete);
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
                    &profile.onboarding_step,
                    &onboarding_complete,
                    &profile.updated_at_ms,
                ],
            )?;
            tx.commit()?;
            Ok((previous, profile))
        }
    })
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
