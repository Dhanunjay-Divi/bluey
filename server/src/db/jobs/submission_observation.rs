//! Read-only, version-bound input for the provider-yield receipt verifier.
//!
//! This module deliberately does not validate a receipt or read object storage.
//! It only returns a bounded, tenant-scoped database snapshot whose raw stored
//! application digest can be compared with the digest observed by the report
//! adapter before a later object readback.

use anyhow::{anyhow, Result};
use rusqlite::{params, OptionalExtension, TransactionBehavior};
use sha2::{Digest, Sha256};

use super::{parse_application_json, parse_json, ApplicationEvidence, JobApplication};
use crate::db::DbPool;

const MAX_APPLICATION_BYTES: i64 = 8 * 1024 * 1024;
const MAX_EVIDENCE_BYTES: i64 = 256 * 1024;
const MAX_EVIDENCE_ROWS: usize = 5;
const MAX_ID_BYTES: usize = 256;

#[derive(Debug, Clone)]
pub(crate) struct StoredSubmissionObservation {
    pub application: JobApplication,
    pub evidence: Vec<ApplicationEvidence>,
    pub evidence_revision_sha256: String,
}

pub(crate) fn read_submission_observation(
    pool: &DbPool,
    account_id: &str,
    application_id: &str,
    submitted_at_ms: i64,
    updated_at_ms: i64,
    stored_application_sha256: &str,
) -> Result<Option<StoredSubmissionObservation>> {
    if !valid_identifier(account_id)
        || !valid_identifier(application_id)
        || submitted_at_ms <= 0
        || updated_at_ms < submitted_at_ms
        || !valid_sha256(stored_application_sha256)
    {
        return Ok(None);
    }
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut connection = pool.get()?;
            let transaction =
                connection.transaction_with_behavior(TransactionBehavior::Deferred)?;
            let observation = read_sqlite_tx(
                &transaction,
                account_id,
                application_id,
                submitted_at_ms,
                updated_at_ms,
                stored_application_sha256,
            )?;
            transaction.commit()?;
            Ok(observation)
        }
        DbPool::Postgres(_) => {
            let mut connection = pool.get_pg()?;
            let mut transaction = connection
                .build_transaction()
                .read_only(true)
                .isolation_level(postgres::IsolationLevel::RepeatableRead)
                .start()?;
            let observation = read_postgres_tx(
                &mut transaction,
                account_id,
                application_id,
                submitted_at_ms,
                updated_at_ms,
                stored_application_sha256,
            )?;
            transaction.commit()?;
            Ok(observation)
        }
    })
}

fn read_sqlite_tx(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    application_id: &str,
    submitted_at_ms: i64,
    updated_at_ms: i64,
    stored_application_sha256: &str,
) -> Result<Option<StoredSubmissionObservation>> {
    let deletion_pending: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM account_deletion_intents WHERE account_id = ?1)",
        params![account_id],
        |row| row.get(0),
    )?;
    if deletion_pending {
        return Ok(None);
    }
    let application = tx
        .query_row(
            "SELECT id, job_id, state, submitted_at_ms, updated_at_ms, application_json
               FROM jobs_applications
              WHERE account_id = ?1 AND id = ?2 AND state = 'submitted'
                AND submitted_at_ms = ?3 AND updated_at_ms = ?4
                AND length(CAST(application_json AS BLOB)) <= ?5",
            params![
                account_id,
                application_id,
                submitted_at_ms,
                updated_at_ms,
                MAX_APPLICATION_BYTES
            ],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, String>(5)?,
                ))
            },
        )
        .optional()?;
    let Some((id, job_id, state, stored_submitted_at_ms, stored_updated_at_ms, raw)) = application
    else {
        return Ok(None);
    };
    if sha256_hex(raw.as_bytes()) != stored_application_sha256 {
        return Ok(None);
    }
    let evidence = read_sqlite_evidence(tx, account_id, application_id)?;
    materialize_observation(
        id,
        job_id,
        state,
        stored_submitted_at_ms,
        stored_updated_at_ms,
        raw,
        evidence,
        account_id,
        application_id,
        submitted_at_ms,
        updated_at_ms,
    )
}

fn read_postgres_tx(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    application_id: &str,
    submitted_at_ms: i64,
    updated_at_ms: i64,
    stored_application_sha256: &str,
) -> Result<Option<StoredSubmissionObservation>> {
    let deletion_pending: bool = tx
        .query_one(
            "SELECT EXISTS(SELECT 1 FROM account_deletion_intents WHERE account_id = $1)",
            &[&account_id],
        )?
        .get(0);
    if deletion_pending {
        return Ok(None);
    }
    let application = tx.query_opt(
        "SELECT id, job_id, state, submitted_at_ms, updated_at_ms, application_json
           FROM jobs_applications
          WHERE account_id = $1 AND id = $2 AND state = 'submitted'
            AND submitted_at_ms = $3 AND updated_at_ms = $4
            AND octet_length(application_json) <= $5::BIGINT",
        &[
            &account_id,
            &application_id,
            &submitted_at_ms,
            &updated_at_ms,
            &MAX_APPLICATION_BYTES,
        ],
    )?;
    let Some(row) = application else {
        return Ok(None);
    };
    let id: String = row.get(0);
    let job_id: String = row.get(1);
    let state: String = row.get(2);
    let stored_submitted_at_ms: i64 = row.get(3);
    let stored_updated_at_ms: i64 = row.get(4);
    let raw: String = row.get(5);
    if sha256_hex(raw.as_bytes()) != stored_application_sha256 {
        return Ok(None);
    }
    let evidence = read_postgres_evidence(tx, account_id, application_id)?;
    materialize_observation(
        id,
        job_id,
        state,
        stored_submitted_at_ms,
        stored_updated_at_ms,
        raw,
        evidence,
        account_id,
        application_id,
        submitted_at_ms,
        updated_at_ms,
    )
}

fn read_sqlite_evidence(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    application_id: &str,
) -> Result<Vec<(String, String, String)>> {
    let oversized: bool = tx.query_row(
        "SELECT EXISTS(
             SELECT 1 FROM jobs_application_evidence
              WHERE account_id = ?1 AND application_id = ?2
                AND kind IN ('application_receipt', 'submission_confirmation')
                AND length(CAST(evidence_json AS BLOB)) > ?3
         )",
        params![account_id, application_id, MAX_EVIDENCE_BYTES],
        |row| row.get(0),
    )?;
    if oversized {
        return Err(anyhow!(
            "submission observation evidence exceeds byte bound"
        ));
    }
    let mut statement = tx.prepare(
        "SELECT id, kind, evidence_json FROM jobs_application_evidence
          WHERE account_id = ?1 AND application_id = ?2
            AND kind IN ('application_receipt', 'submission_confirmation')
            AND length(CAST(evidence_json AS BLOB)) <= ?3
          ORDER BY id ASC LIMIT ?4",
    )?;
    let rows = statement
        .query_map(
            params![
                account_id,
                application_id,
                MAX_EVIDENCE_BYTES,
                (MAX_EVIDENCE_ROWS + 1) as i64
            ],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    bounded_evidence_rows(rows)
}

fn read_postgres_evidence(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    application_id: &str,
) -> Result<Vec<(String, String, String)>> {
    let oversized: bool = tx
        .query_one(
            "SELECT EXISTS(
                 SELECT 1 FROM jobs_application_evidence
                  WHERE account_id = $1 AND application_id = $2
                    AND kind IN ('application_receipt', 'submission_confirmation')
                AND octet_length(evidence_json) > $3::BIGINT
             )",
            &[&account_id, &application_id, &MAX_EVIDENCE_BYTES],
        )?
        .get(0);
    if oversized {
        return Err(anyhow!(
            "submission observation evidence exceeds byte bound"
        ));
    }
    let rows = tx.query(
        "SELECT id, kind, evidence_json FROM jobs_application_evidence
          WHERE account_id = $1 AND application_id = $2
            AND kind IN ('application_receipt', 'submission_confirmation')
            AND octet_length(evidence_json) <= $3::BIGINT
          ORDER BY id ASC LIMIT $4::BIGINT",
        &[
            &account_id,
            &application_id,
            &MAX_EVIDENCE_BYTES,
            &((MAX_EVIDENCE_ROWS + 1) as i64),
        ],
    )?;
    bounded_evidence_rows(
        rows.into_iter()
            .map(|row| (row.get(0), row.get(1), row.get(2)))
            .collect(),
    )
}

fn bounded_evidence_rows(
    rows: Vec<(String, String, String)>,
) -> Result<Vec<(String, String, String)>> {
    if rows.len() > MAX_EVIDENCE_ROWS {
        return Err(anyhow!("submission observation evidence exceeds bound"));
    }
    Ok(rows)
}

#[allow(clippy::too_many_arguments)]
fn materialize_observation(
    id: String,
    job_id: String,
    state: String,
    stored_submitted_at_ms: i64,
    stored_updated_at_ms: i64,
    raw: String,
    raw_evidence: Vec<(String, String, String)>,
    account_id: &str,
    application_id: &str,
    submitted_at_ms: i64,
    updated_at_ms: i64,
) -> Result<Option<StoredSubmissionObservation>> {
    if id != application_id
        || state != "submitted"
        || stored_submitted_at_ms != submitted_at_ms
        || stored_updated_at_ms != updated_at_ms
    {
        return Ok(None);
    }
    let application =
        parse_application_json(raw, &id, &job_id, "submission observation application")?;
    if application.id != application_id
        || application.state != "submitted"
        || application.submitted_at_ms != Some(submitted_at_ms)
        || application.updated_at_ms != updated_at_ms
    {
        return Ok(None);
    }

    let mut evidence_revision = Sha256::new();
    let mut evidence = Vec::with_capacity(raw_evidence.len());
    for (physical_id, physical_kind, raw) in raw_evidence {
        if !valid_identifier(&physical_id)
            || !matches!(
                physical_kind.as_str(),
                "application_receipt" | "submission_confirmation"
            )
        {
            return Ok(None);
        }
        let parsed: ApplicationEvidence =
            parse_json(raw.clone(), "submission observation evidence")?;
        if parsed.id != physical_id
            || parsed.kind != physical_kind
            || parsed.application_id != application_id
        {
            return Ok(None);
        }
        append_length_prefixed(&mut evidence_revision, physical_id.as_bytes());
        append_length_prefixed(&mut evidence_revision, physical_kind.as_bytes());
        append_length_prefixed(&mut evidence_revision, raw.as_bytes());
        evidence.push(parsed);
    }
    // Account scope is included even though every query is tenant-filtered so a
    // digest cannot be repurposed across tenants by a later caller.
    append_length_prefixed(&mut evidence_revision, account_id.as_bytes());
    Ok(Some(StoredSubmissionObservation {
        application,
        evidence,
        evidence_revision_sha256: hex::encode(evidence_revision.finalize()),
    }))
}

fn append_length_prefixed(hasher: &mut Sha256, value: &[u8]) {
    hasher.update((value.len() as u64).to_be_bytes());
    hasher.update(value);
}

fn sha256_hex(value: &[u8]) -> String {
    hex::encode(Sha256::digest(value))
}

fn valid_identifier(value: &str) -> bool {
    !value.is_empty() && value.len() <= MAX_ID_BYTES
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .as_bytes()
            .iter()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(byte))
}
