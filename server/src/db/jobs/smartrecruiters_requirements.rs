//! Opaque, description-only SmartRecruiters hydration custody.
//!
//! This module deliberately does not fetch a provider, create a source, or
//! alter membership. A caller first captures one already-authoritative active
//! membership, then may commit only a bounded description after its separate
//! proof parser has finished.

use anyhow::{Context, Result};
use rusqlite::{params, TransactionBehavior};
use sha2::{Digest, Sha256};

use super::{
    canonical_discovery_track_active, canonical_public_discovery_url,
    lock_discovery_account_postgres, now_ms, original_source_subject_sha256, parse_json, to_json,
    CareerTrack, DbPool, JobPosting,
};
use crate::db::account_data::{
    account_write_fence_postgres_tx, account_write_fence_sqlite_tx, AccountWriteFence,
};

const MAX_DESCRIPTION_BYTES: usize = 64 * 1024;

const SELECT_CAPTURE_SQLITE: &str = "SELECT p.posting_json, p.source, p.canonical_key,
    p.canonical_url, s.id, s.source_key, m.external_id, m.canonical_key, m.job_id,
    m.content_hash, m.last_seen_run_id, s.status, s.health, s.updated_at_ms,
    t.track_json, t.active, m.missing_count, m.missing_since_at_ms
    FROM jobs_postings p
    JOIN jobs_discovery_memberships m ON m.account_id = p.account_id
        AND m.job_id = p.id AND m.canonical_key = p.canonical_key
    JOIN jobs_discovery_sources s ON s.id = m.source_id AND s.account_id = m.account_id
    JOIN jobs_tracks t ON t.id = ?3 AND t.account_id = p.account_id
    WHERE p.account_id = ?1 AND p.id = ?2 AND p.canonical_url IS NOT NULL
      AND m.availability_status = 'active' AND m.missing_count = 0
      AND m.missing_since_at_ms IS NULL
      AND s.provider = 'smartrecruiters' AND s.track_id = ?3 AND s.status = 'active'
      AND s.health IN ('waiting', 'healthy')
      AND t.active = 1
      AND NOT EXISTS (SELECT 1 FROM account_deletion_intents d
                      WHERE d.account_id = p.account_id)
    ORDER BY s.id, m.external_id LIMIT 2";

const SELECT_CAPTURE_POSTGRES: &str = "SELECT p.posting_json, p.source, p.canonical_key,
    p.canonical_url, s.id, s.source_key, m.external_id, m.canonical_key, m.job_id,
    m.content_hash, m.last_seen_run_id, s.status, s.health, s.updated_at_ms,
    t.track_json, t.active, m.missing_count, m.missing_since_at_ms
    FROM jobs_postings p
    JOIN jobs_discovery_memberships m ON m.account_id = p.account_id
        AND m.job_id = p.id AND m.canonical_key = p.canonical_key
    JOIN jobs_discovery_sources s ON s.id = m.source_id AND s.account_id = m.account_id
    JOIN jobs_tracks t ON t.id = $3 AND t.account_id = p.account_id
    WHERE p.account_id = $1 AND p.id = $2 AND p.canonical_url IS NOT NULL
      AND m.availability_status = 'active' AND m.missing_count = 0
      AND m.missing_since_at_ms IS NULL
      AND s.provider = 'smartrecruiters' AND s.track_id = $3 AND s.status = 'active'
      AND s.health IN ('waiting', 'healthy')
      AND t.active = 1
      AND NOT EXISTS (SELECT 1 FROM account_deletion_intents d
                      WHERE d.account_id = p.account_id)
    ORDER BY s.id, m.external_id LIMIT 2";

type CaptureRow = (
    String,
    String,
    String,
    String,
    String,
    String,
    String,
    String,
    String,
    String,
    String,
    String,
    String,
    i64,
    String,
    i32,
    i64,
    Option<i64>,
);

/// A server-captured immutable binding to one active SmartRecruiters member.
/// Its fields remain private so HTTP/provider input cannot manufacture it.
pub(crate) struct SmartRecruitersSummaryCapture {
    account_id: String,
    job_id: String,
    track_id: String,
    source_id: String,
    tenant: String,
    external_id: String,
    canonical_key: String,
    canonical_url: String,
    membership_content_hash: String,
    membership_last_seen_run_id: String,
    source_updated_at_ms: i64,
    track_sha256: [u8; 32],
    posting_sha256: [u8; 32],
}

impl SmartRecruitersSummaryCapture {
    pub(crate) fn canonical_url(&self) -> &str {
        &self.canonical_url
    }

    pub(crate) fn tenant(&self) -> &str {
        &self.tenant
    }

    pub(crate) fn external_id(&self) -> &str {
        &self.external_id
    }

    #[cfg(test)]
    pub(crate) fn track_id(&self) -> &str {
        &self.track_id
    }
}

/// Capture a single current active member. Missing, inactive, deleting, or
/// ambiguous membership intentionally looks like no eligible capture.
pub(crate) fn capture_smartrecruiters_summary(
    pool: &DbPool,
    account_id: &str,
    job_id: &str,
    expected_track_id: &str,
) -> Result<Option<SmartRecruitersSummaryCapture>> {
    if account_id.trim().is_empty()
        || job_id.trim().is_empty()
        || expected_track_id.trim().is_empty()
        || account_id != account_id.trim()
        || job_id != job_id.trim()
        || expected_track_id != expected_track_id.trim()
    {
        return Ok(None);
    }

    let rows = crate::db::run_blocking_db(|| -> Result<Vec<CaptureRow>> {
        match pool {
            DbPool::Sqlite(_) => {
                let conn = pool.get()?;
                let mut statement = conn.prepare(SELECT_CAPTURE_SQLITE)?;
                let rows = statement
                    .query_map(params![account_id, job_id, expected_track_id], |row| {
                        Ok((
                            row.get(0)?,
                            row.get(1)?,
                            row.get(2)?,
                            row.get(3)?,
                            row.get(4)?,
                            row.get(5)?,
                            row.get(6)?,
                            row.get(7)?,
                            row.get(8)?,
                            row.get(9)?,
                            row.get(10)?,
                            row.get(11)?,
                            row.get(12)?,
                            row.get(13)?,
                            row.get(14)?,
                            row.get(15)?,
                            row.get(16)?,
                            row.get(17)?,
                        ))
                    })?
                    .collect::<std::result::Result<Vec<_>, _>>()
                    .context("read SmartRecruiters summary capture")?;
                Ok(rows)
            }
            DbPool::Postgres(_) => Ok(pool
                .get_pg()?
                .query(
                    SELECT_CAPTURE_POSTGRES,
                    &[&account_id, &job_id, &expected_track_id],
                )?
                .into_iter()
                .map(|row| {
                    (
                        row.get(0),
                        row.get(1),
                        row.get(2),
                        row.get(3),
                        row.get(4),
                        row.get(5),
                        row.get(6),
                        row.get(7),
                        row.get(8),
                        row.get(9),
                        row.get(10),
                        row.get(11),
                        row.get(12),
                        row.get(13),
                        row.get(14),
                        row.get(15),
                        row.get(16),
                        row.get(17),
                    )
                })
                .collect()),
        }
    })?;
    let [row] = rows.as_slice() else {
        return Ok(None);
    };
    capture_from_row(account_id, job_id, expected_track_id, row).map(Some)
}

/// Commit a proof-parser-validated description without changing identity,
/// source, membership, lifecycle, or any provider-facing state.
pub(crate) fn commit_smartrecruiters_summary_description(
    pool: &DbPool,
    account_id: &str,
    capture: &SmartRecruitersSummaryCapture,
    description: &str,
) -> Result<JobPosting> {
    validate_description(description)?;
    if capture.account_id != account_id {
        anyhow::bail!("SmartRecruiters summary capture belongs to another account")
    }
    let description = description.to_string();
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            require_active_account_sqlite(&tx, account_id)?;
            let mut statement = tx.prepare(SELECT_CAPTURE_SQLITE)?;
            let rows = statement
                .query_map(
                    params![account_id, &capture.job_id, &capture.track_id],
                    |row| {
                        Ok((
                            row.get(0)?,
                            row.get(1)?,
                            row.get(2)?,
                            row.get(3)?,
                            row.get(4)?,
                            row.get(5)?,
                            row.get(6)?,
                            row.get(7)?,
                            row.get(8)?,
                            row.get(9)?,
                            row.get(10)?,
                            row.get(11)?,
                            row.get(12)?,
                            row.get(13)?,
                            row.get(14)?,
                            row.get(15)?,
                            row.get(16)?,
                            row.get(17)?,
                        ))
                    },
                )?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            let row = match rows.as_slice() {
                [row] => Some(row.clone()),
                _ => None,
            };
            drop(statement);
            let mut posting = require_current_capture(capture, row)?;
            update_description(&mut posting, &description);
            let payload = to_json(&posting, "SmartRecruiters summary posting")?;
            let changed = tx.execute(
                "UPDATE jobs_postings SET posting_json = ?3, updated_at_ms = ?4
                   WHERE account_id = ?1 AND id = ?2
                     AND NOT EXISTS (SELECT 1 FROM account_deletion_intents d
                                     WHERE d.account_id = ?1)",
                params![account_id, &capture.job_id, payload, posting.updated_at_ms],
            )?;
            if changed != 1 {
                anyhow::bail!("SmartRecruiters summary posting changed during commit")
            }
            tx.commit()?;
            Ok(posting)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            lock_discovery_account_postgres(&mut tx, account_id)?;
            require_active_account_postgres(&mut tx, account_id)?;
            let sql = format!("{SELECT_CAPTURE_POSTGRES} FOR UPDATE OF p, m, s, t");
            let rows = tx.query(&sql, &[&account_id, &capture.job_id, &capture.track_id])?;
            let row = match rows.as_slice() {
                [row] => Some((
                    row.get(0),
                    row.get(1),
                    row.get(2),
                    row.get(3),
                    row.get(4),
                    row.get(5),
                    row.get(6),
                    row.get(7),
                    row.get(8),
                    row.get(9),
                    row.get(10),
                    row.get(11),
                    row.get(12),
                    row.get(13),
                    row.get(14),
                    row.get(15),
                    row.get(16),
                    row.get(17),
                )),
                _ => None,
            };
            let mut posting = require_current_capture(capture, row)?;
            update_description(&mut posting, &description);
            let payload = to_json(&posting, "SmartRecruiters summary posting")?;
            let changed = tx.execute(
                "UPDATE jobs_postings SET posting_json = $3, updated_at_ms = $4
                   WHERE account_id = $1 AND id = $2
                     AND NOT EXISTS (SELECT 1 FROM account_deletion_intents d
                                     WHERE d.account_id = $1)",
                &[
                    &account_id,
                    &capture.job_id,
                    &payload,
                    &posting.updated_at_ms,
                ],
            )?;
            if changed != 1 {
                anyhow::bail!("SmartRecruiters summary posting changed during commit")
            }
            tx.commit()?;
            Ok(posting)
        }
    })
}

fn capture_from_row(
    account_id: &str,
    job_id: &str,
    expected_track_id: &str,
    row: &CaptureRow,
) -> Result<SmartRecruitersSummaryCapture> {
    let (
        raw,
        source,
        key,
        url,
        source_id,
        tenant,
        external_id,
        member_key,
        member_job_id,
        membership_content_hash,
        membership_last_seen_run_id,
        source_status,
        source_health,
        source_updated_at_ms,
        track_json,
        raw_track_active,
        missing_count,
        missing_since_at_ms,
    ) = row;
    let posting = parse_json::<JobPosting>(raw.clone(), "SmartRecruiters summary posting")?;
    let canonical_target =
        canonical_public_discovery_url("smartrecruiters", &posting.canonical_url);
    let canonical_target_matches = match &canonical_target {
        Ok((canonical_url, canonical_tenant)) => {
            canonical_url == &posting.canonical_url && canonical_tenant == tenant
        }
        Err(_) => false,
    };
    if !matches!(
        source.as_str(),
        "smartrecruiters" | "smartrecruiters_import"
    ) || posting.id != job_id
        || posting.source != source.as_str()
        || posting.canonical_key != key.as_str()
        || posting.canonical_url != url.as_str()
        || posting.track_id != expected_track_id
        || posting.availability_status != "active"
        || !posting.description.trim().is_empty()
        || posting.external_id != external_id.as_str()
        || member_key != key
        || member_job_id != job_id
        || membership_content_hash.trim().is_empty()
        || membership_last_seen_run_id.trim().is_empty()
        || source_status != "active"
        || !matches!(source_health.as_str(), "waiting" | "healthy")
        || key.trim().is_empty()
        || url.trim().is_empty()
        || tenant.trim().is_empty()
        || external_id.trim().is_empty()
        || source_id.trim().is_empty()
        || *missing_count != 0
        || missing_since_at_ms.is_some()
        || !strict_smartrecruiters_target(&posting.canonical_url, tenant, external_id)
        || validate_track_projection(expected_track_id, track_json, i64::from(*raw_track_active))
            .is_err()
        || !canonical_target_matches
        || original_source_subject_sha256(&posting).is_err()
    {
        anyhow::bail!("SmartRecruiters summary capture does not bind an active posting")
    }
    Ok(SmartRecruitersSummaryCapture {
        account_id: account_id.to_string(),
        job_id: job_id.to_string(),
        track_id: expected_track_id.to_string(),
        source_id: source_id.to_string(),
        tenant: tenant.to_string(),
        external_id: external_id.to_string(),
        canonical_key: key.to_string(),
        canonical_url: url.to_string(),
        membership_content_hash: membership_content_hash.to_string(),
        membership_last_seen_run_id: membership_last_seen_run_id.to_string(),
        source_updated_at_ms: *source_updated_at_ms,
        track_sha256: Sha256::digest(track_json.as_bytes()).into(),
        posting_sha256: Sha256::digest(raw.as_bytes()).into(),
    })
}

fn require_current_capture(
    capture: &SmartRecruitersSummaryCapture,
    row: Option<CaptureRow>,
) -> Result<JobPosting> {
    let Some(row) = row else {
        anyhow::bail!("SmartRecruiters summary membership is no longer eligible")
    };
    let current = capture_from_row(
        &capture.account_id,
        &capture.job_id,
        &capture.track_id,
        &row,
    )?;
    if current.source_id != capture.source_id
        || current.tenant != capture.tenant
        || current.external_id != capture.external_id
        || current.canonical_key != capture.canonical_key
        || current.canonical_url != capture.canonical_url
        || current.membership_content_hash != capture.membership_content_hash
        || current.membership_last_seen_run_id != capture.membership_last_seen_run_id
        || current.source_updated_at_ms != capture.source_updated_at_ms
        || current.track_sha256 != capture.track_sha256
        || current.posting_sha256 != capture.posting_sha256
    {
        anyhow::bail!("SmartRecruiters summary capture changed before commit")
    }
    parse_json(row.0, "SmartRecruiters summary posting")
}

fn validate_description(description: &str) -> Result<()> {
    if description.trim().is_empty()
        || description.len() > MAX_DESCRIPTION_BYTES
        || description.chars().any(|character| {
            character == '\0'
                || (character.is_control() && !matches!(character, '\n' | '\r' | '\t'))
        })
    {
        anyhow::bail!("SmartRecruiters summary description is invalid")
    }
    Ok(())
}

fn validate_track_projection(track_id: &str, raw: &str, raw_active: i64) -> Result<()> {
    let active = canonical_discovery_track_active(raw_active)?;
    let track: CareerTrack = parse_json(raw.to_string(), "SmartRecruiters Career Track")?;
    if !active || track.id != track_id || track.active != active {
        anyhow::bail!("SmartRecruiters Career Track projection is not current")
    }
    Ok(())
}

fn strict_smartrecruiters_target(url: &str, tenant: &str, external_id: &str) -> bool {
    let Ok(parsed) = reqwest::Url::parse(url) else {
        return false;
    };
    if parsed.scheme() != "https"
        || parsed.host_str() != Some("jobs.smartrecruiters.com")
        || parsed.port().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
    {
        return false;
    }
    let segments = parsed
        .path_segments()
        .map(|segments| segments.collect::<Vec<_>>())
        .unwrap_or_default();
    let Some((actual_tenant, record_id)) = segments.first().zip(segments.get(1)) else {
        return false;
    };
    segments.len() == 2
        && *actual_tenant == tenant
        && (*record_id == external_id
            || record_id
                .strip_prefix(external_id)
                .is_some_and(|suffix| suffix.starts_with('-') && suffix.len() > 1))
}

fn require_active_account_sqlite(tx: &rusqlite::Transaction<'_>, account_id: &str) -> Result<()> {
    match account_write_fence_sqlite_tx(tx, account_id)? {
        AccountWriteFence::Active => Ok(()),
        AccountWriteFence::DeletionRequested => {
            anyhow::bail!("account deletion has fenced summary hydration")
        }
        AccountWriteFence::Missing => anyhow::bail!("summary hydration account is missing"),
    }
}

fn require_active_account_postgres(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
) -> Result<()> {
    match account_write_fence_postgres_tx(tx, account_id)? {
        AccountWriteFence::Active => Ok(()),
        AccountWriteFence::DeletionRequested => {
            anyhow::bail!("account deletion has fenced summary hydration")
        }
        AccountWriteFence::Missing => anyhow::bail!("summary hydration account is missing"),
    }
}

fn update_description(posting: &mut JobPosting, description: &str) {
    posting.description = description.to_string();
    posting.updated_at_ms = now_ms();
    // No cached eligibility or freshness can describe the new body. Preserve
    // hard denials, but clear the positive source state that could project.
    posting.last_verified_at_ms = None;
    posting.eligibility = None;
    if posting.discovery_evidence.original_source_status == "verified_open" {
        posting.discovery_evidence.original_source_status = "unknown".to_string();
        posting.discovery_evidence.original_source_checked_at_ms = None;
        posting
            .discovery_evidence
            .original_source_snapshot_expires_at_ms = None;
        posting.discovery_evidence.original_source_evidence_hash = None;
        posting
            .discovery_evidence
            .original_source_mismatched_fields
            .clear();
        posting.discovery_evidence.requires_original_revalidation = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn posting_with_original_status(status: &str, revalidation: bool) -> JobPosting {
        let mut posting: JobPosting = serde_json::from_value(serde_json::json!({
            "id": "job", "company": "Company", "title": "Title", "description": "old"
        }))
        .expect("minimal posting");
        posting.discovery_evidence.original_source_status = status.to_string();
        posting.discovery_evidence.requires_original_revalidation = revalidation;
        posting.discovery_evidence.original_source_evidence_hash = Some("proof".to_string());
        posting.last_verified_at_ms = Some(1);
        posting
    }

    fn stored_posting_json(pool: &DbPool, account_id: &str, job_id: &str) -> String {
        pool.get()
            .unwrap()
            .query_row(
                "SELECT posting_json FROM jobs_postings WHERE account_id = ?1 AND id = ?2",
                params![account_id, job_id],
                |row| row.get(0),
            )
            .unwrap()
    }

    #[test]
    fn summary_description_refuses_empty_oversized_and_control_values() {
        assert!(validate_description(" \n").is_err());
        assert!(validate_description(&"x".repeat(MAX_DESCRIPTION_BYTES + 1)).is_err());
        assert!(validate_description("valid\u{0000}body").is_err());
        assert!(validate_description("valid\nbody").is_ok());
    }

    #[test]
    fn changed_description_clears_cached_projection_and_preserves_hard_denial() {
        let mut positive = posting_with_original_status("verified_open", false);
        positive.eligibility = Some(Default::default());
        update_description(&mut positive, "new");
        assert_eq!(
            positive.discovery_evidence.original_source_status,
            "unknown"
        );
        assert!(positive.discovery_evidence.requires_original_revalidation);
        assert!(positive
            .discovery_evidence
            .original_source_evidence_hash
            .is_none());
        assert!(positive.last_verified_at_ms.is_none());
        assert!(positive.eligibility.is_none());

        let mut expired_positive = posting_with_original_status("verified_open", true);
        update_description(&mut expired_positive, "new");
        assert_eq!(
            expired_positive.discovery_evidence.original_source_status,
            "unknown"
        );
        assert!(expired_positive
            .discovery_evidence
            .original_source_evidence_hash
            .is_none());

        let mut denial = posting_with_original_status("closed", true);
        denial.eligibility = Some(Default::default());
        update_description(&mut denial, "new");
        assert_eq!(denial.discovery_evidence.original_source_status, "closed");
        assert_eq!(
            denial
                .discovery_evidence
                .original_source_evidence_hash
                .as_deref(),
            Some("proof")
        );
        assert!(denial.last_verified_at_ms.is_none());
        assert!(denial.eligibility.is_none());
    }

    #[test]
    fn scheduled_blank_summary_capture_refuses_stale_membership_and_commits_description_only() {
        let pool = super::super::tests::test_pool();
        let account_id = "acct-jobs";
        let job_id = "smartrecruiters-summary-job";
        let track_id = "track-default";
        let source_id = "smartrecruiters-summary-source";
        let external_id = "posting-123";
        let mut posting: JobPosting = serde_json::from_value(serde_json::json!({
            "id": job_id,
            "source": "smartrecruiters",
            "external_id": external_id,
            "company": "Synthetic Company",
            "title": "Synthetic Role",
            "canonical_url": "https://jobs.smartrecruiters.com/acme/posting-123",
            "description": "",
            "track_id": track_id,
            "availability_status": "active",
            "status": "matched",
            "created_at_ms": 1,
            "updated_at_ms": 1
        }))
        .expect("synthetic SmartRecruiters posting");
        posting.canonical_key = super::super::canonical_job_key(&posting);
        let canonical_key = posting.canonical_key.clone();
        let stored = to_json(&posting, "synthetic SmartRecruiters posting").unwrap();
        let before_posting_json = stored.clone();
        let conn = pool.get().unwrap();
        conn.execute(
            "INSERT INTO jobs_postings (
                id, account_id, canonical_key, posting_json, source, canonical_url,
                company, title, location, match_score, status, created_at_ms, updated_at_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, '', 0, ?9, 1, 1)",
            params![
                job_id,
                account_id,
                canonical_key,
                stored,
                posting.source,
                posting.canonical_url,
                posting.company,
                posting.title,
                posting.status,
            ],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO jobs_discovery_sources (
                id, account_id, track_id, provider, source_key, source_json, status, health,
                next_run_at_ms, created_at_ms, updated_at_ms
             ) VALUES (?1, ?2, ?3, 'smartrecruiters', 'acme', '{}', 'active', 'healthy', 1, 1, 1)",
            params![source_id, account_id, track_id],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO jobs_discovery_memberships (
                source_id, account_id, external_id, canonical_key, job_id, content_hash,
                first_seen_at_ms, last_seen_at_ms, last_seen_run_id, availability_status,
                missing_count, missing_since_at_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, 'first', 1, 1, 'run-first', 'active', 0, NULL)",
            params![
                source_id,
                account_id,
                external_id,
                posting.canonical_key,
                job_id
            ],
        )
        .unwrap();
        drop(conn);

        let stale = capture_smartrecruiters_summary(&pool, account_id, job_id, track_id)
            .unwrap()
            .expect("blank scheduled SmartRecruiters capture");
        pool.get()
            .unwrap()
            .execute(
                "UPDATE jobs_discovery_memberships SET content_hash = 'second'
                  WHERE source_id = ?1 AND external_id = ?2",
                params![source_id, external_id],
            )
            .unwrap();
        assert!(commit_smartrecruiters_summary_description(
            &pool,
            account_id,
            &stale,
            "Synthetic requirements."
        )
        .is_err());
        assert_eq!(
            stored_posting_json(&pool, account_id, job_id),
            before_posting_json,
            "membership revision drift must not write the posting"
        );

        pool.get()
            .unwrap()
            .execute(
                "UPDATE jobs_discovery_memberships SET external_id = 'posting-456'
                  WHERE source_id = ?1 AND external_id = ?2",
                params![source_id, external_id],
            )
            .unwrap();
        assert!(commit_smartrecruiters_summary_description(
            &pool,
            account_id,
            &stale,
            "Synthetic requirements."
        )
        .is_err());
        assert_eq!(
            stored_posting_json(&pool, account_id, job_id),
            before_posting_json,
            "external-ID coordinate drift must not write the posting"
        );
        pool.get()
            .unwrap()
            .execute(
                "UPDATE jobs_discovery_memberships SET external_id = ?2
                  WHERE source_id = ?1 AND external_id = 'posting-456'",
                params![source_id, external_id],
            )
            .unwrap();

        let fresh = capture_smartrecruiters_summary(&pool, account_id, job_id, track_id)
            .unwrap()
            .expect("fresh scheduled SmartRecruiters capture");
        let saved = commit_smartrecruiters_summary_description(
            &pool,
            account_id,
            &fresh,
            "Synthetic requirements.",
        )
        .unwrap();
        assert_eq!(saved.description, "Synthetic requirements.");
        assert_eq!(saved.id, job_id);
        assert_eq!(saved.title, "Synthetic Role");
        assert_eq!(saved.external_id, external_id);
        assert_eq!(saved.canonical_key, posting.canonical_key);
        assert_eq!(saved.canonical_url, posting.canonical_url);
        assert_eq!(saved.source, posting.source);
        assert_eq!(saved.track_id, track_id);
        assert_eq!(saved.availability_status, "active");
        assert!(saved.last_verified_at_ms.is_none());
        assert!(saved.eligibility.is_none());
        let readback: JobPosting = parse_json(
            stored_posting_json(&pool, account_id, job_id),
            "synthetic SmartRecruiters readback",
        )
        .unwrap();
        assert_eq!(readback.description, "Synthetic requirements.");
        assert_eq!(readback.id, job_id);
        assert_eq!(readback.external_id, external_id);
        assert_eq!(readback.canonical_key, posting.canonical_key);
        assert_eq!(readback.canonical_url, posting.canonical_url);
        assert_eq!(readback.source, posting.source);
        assert_eq!(readback.track_id, track_id);
        assert!(readback.eligibility.is_none());
        assert!(capture_smartrecruiters_summary(&pool, account_id, job_id, track_id).is_err());
    }
}
