//! One-shot, server-owned SmartRecruiters parser v1-to-v2 expedites.
//!
//! This is deliberately not a scanner or a requeue facility. The caller must
//! name one already-captured old head; the current subject, membership,
//! authority, assignment, and deletion fence are all rechecked in one
//! transaction before only its due time is moved to the database clock.

use rusqlite::{params, OptionalExtension, TransactionBehavior};

use super::{
    lock_discovery_account_postgres, lock_managed_cloud_release_registry_shared_postgres_tx,
    lock_operational_hold_shared_postgres_tx, original_source_append_event_postgres,
    original_source_append_event_sqlite, original_source_assignment_from_postgres,
    original_source_assignment_from_sqlite, original_source_canonical_subject,
    original_source_db_now_postgres, original_source_db_now_sqlite,
    original_source_parser_metadata, original_source_require_assignment_authority_postgres,
    original_source_require_assignment_authority_sqlite,
    original_source_require_operational_hold_clear_postgres,
    original_source_require_operational_hold_clear_sqlite, original_source_sha256,
    original_source_valid_id, original_source_valid_sha256,
    recheck_original_source_subject_postgres, recheck_original_source_subject_sqlite,
    resolve_original_source_verification_head_postgres_tx,
    resolve_original_source_verification_head_sqlite_tx, DbPool, JobPosting,
    OriginalSourceVerificationError, OriginalSourceVerificationResult,
    ORIGINAL_SOURCE_ASSIGNMENT_COLUMNS,
};

const SMARTRECRUITERS_PARSER_V1: &str = "smartrecruiters.original_source.v1";
const SMARTRECRUITERS_PARSER_V2: &str = "smartrecruiters.original_source.v2";
const PARSER_UPGRADE_V2_REASON: &str = "parser_upgrade_v2";

/// Caller-captured coordinates for exactly one stale SmartRecruiters head.
/// This type contains no provider payload and cannot request a broad retry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SmartRecruitersParserV2ExpediteInput {
    pub(crate) account_id: String,
    pub(crate) job_id: String,
    pub(crate) expected_head_revision: i64,
    pub(crate) expected_receipt_sha256: String,
}

/// Expedite at most one future idle assignment only while V2 is current.
///
/// The test module's current-version entrypoint is inert after V2. The
/// separate helper below can still replay the exact historical V2 oracle.
pub(crate) fn expedite_smartrecruiters_parser_v2(
    pool: &DbPool,
    input: &SmartRecruitersParserV2ExpediteInput,
) -> OriginalSourceVerificationResult<bool> {
    validate_input(input)?;
    expedite_smartrecruiters_parser_v2_with_context(
        pool,
        input,
        smartrecruiters_parser_v2_is_current(),
    )
}

/// Test-only historical V2 context for the retired selection oracle.
///
/// This does not assert that a signed V2 runtime exists or that a completed
/// verifier used V2: completion continues to use whatever current parser
/// version the ordinary fixture emits.
#[cfg(test)]
pub(crate) fn expedite_smartrecruiters_parser_v2_historical_test_oracle(
    pool: &DbPool,
    input: &SmartRecruitersParserV2ExpediteInput,
) -> OriginalSourceVerificationResult<bool> {
    validate_input(input)?;
    expedite_smartrecruiters_parser_v2_with_context(pool, input, true)
}

fn expedite_smartrecruiters_parser_v2_with_context(
    pool: &DbPool,
    input: &SmartRecruitersParserV2ExpediteInput,
    historical_v2_context: bool,
) -> OriginalSourceVerificationResult<bool> {
    if !historical_v2_context {
        return Ok(false);
    }
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => expedite_sqlite(pool, input),
        DbPool::Postgres(_) => expedite_postgres(pool, input),
    })
}

fn validate_input(
    input: &SmartRecruitersParserV2ExpediteInput,
) -> OriginalSourceVerificationResult<()> {
    if !original_source_valid_id(&input.account_id)
        || !original_source_valid_id(&input.job_id)
        || input.expected_head_revision < 1
        || !original_source_valid_sha256(&input.expected_receipt_sha256)
    {
        return Err(OriginalSourceVerificationError::InvalidInput(
            "invalid SmartRecruiters parser refresh coordinate".to_string(),
        ));
    }
    Ok(())
}

fn smartrecruiters_parser_v1_digest() -> String {
    original_source_sha256(
        format!(
            "bluey-jobs-original-source-parser-v1\0smartrecruiters\0{SMARTRECRUITERS_PARSER_V1}\n"
        )
        .as_bytes(),
    )
}

/// This selected v1-to-v2 bridge is inert once the server parser advances.
fn smartrecruiters_parser_v2_is_current() -> bool {
    let (version, _) = original_source_parser_metadata("smartrecruiters");
    version == SMARTRECRUITERS_PARSER_V2
}

fn active_current_membership_sqlite(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    posting: &JobPosting,
) -> OriginalSourceVerificationResult<bool> {
    if posting.availability_status != "active" {
        return Ok(false);
    }
    let subject = original_source_canonical_subject(posting)?;
    tx.query_row(
        "SELECT 1 FROM jobs_discovery_memberships membership
           JOIN jobs_discovery_sources source
             ON source.id=membership.source_id
            AND source.account_id=membership.account_id
          WHERE membership.account_id=?1 AND membership.job_id=?2
            AND membership.external_id=?3 AND membership.canonical_key=?4
            AND source.provider=?5 AND source.source_key=?6
            AND membership.availability_status='active'
            AND membership.missing_count=0 AND membership.missing_since_at_ms IS NULL",
        params![
            account_id,
            posting.id,
            posting.external_id,
            posting.canonical_key,
            subject.provider_family,
            subject.discovery_source_key,
        ],
        |_| Ok(()),
    )
    .optional()
    .map_err(super::original_source_storage)
    .map(|row| row.is_some())
}

fn active_current_membership_postgres(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    posting: &JobPosting,
) -> OriginalSourceVerificationResult<bool> {
    if posting.availability_status != "active" {
        return Ok(false);
    }
    let subject = original_source_canonical_subject(posting)?;
    tx.query_opt(
        "SELECT 1 FROM jobs_discovery_memberships membership
           JOIN jobs_discovery_sources source
             ON source.id=membership.source_id
            AND source.account_id=membership.account_id
          WHERE membership.account_id=$1 AND membership.job_id=$2
            AND membership.external_id=$3 AND membership.canonical_key=$4
            AND source.provider=$5 AND source.source_key=$6
            AND membership.availability_status='active'
            AND membership.missing_count=0 AND membership.missing_since_at_ms IS NULL
          FOR SHARE OF membership,source",
        &[
            &account_id,
            &posting.id,
            &posting.external_id,
            &posting.canonical_key,
            &subject.provider_family,
            &subject.discovery_source_key,
        ],
    )
    .map_err(super::original_source_storage)
    .map(|row| row.is_some())
}

fn eligible_head(
    input: &SmartRecruitersParserV2ExpediteInput,
    head: &super::OriginalSourceVerificationHead,
) -> bool {
    let parser_v1_digest = smartrecruiters_parser_v1_digest();
    head.account_id == input.account_id
        && head.job_id == input.job_id
        && head.head_revision == input.expected_head_revision
        && head.receipt_sha256 == input.expected_receipt_sha256
        && head.assurance == "original_verified"
        && head.result == "verified_open"
        && head.parser_version.as_deref() == Some(SMARTRECRUITERS_PARSER_V1)
        && head.parser_digest.as_deref() == Some(parser_v1_digest.as_str())
}

fn assignment_is_eligible(
    assignment: &super::OriginalSourceVerificationAssignment,
    head: &super::OriginalSourceVerificationHead,
    now: i64,
) -> bool {
    assignment.assignment_id == head.assignment_id
        && assignment.account_id == head.account_id
        && assignment.job_id == head.job_id
        && assignment.subject_sha256 == head.subject_sha256
        && assignment.state == "idle"
        && assignment.circuit_state == "closed"
        && assignment.attempt_count < assignment.attempt_budget
        && assignment.not_before_at_ms <= now
        && assignment.expires_at_ms > now
        && assignment.next_attempt_at_ms > now
}

fn expedite_sqlite(
    pool: &DbPool,
    input: &SmartRecruitersParserV2ExpediteInput,
) -> OriginalSourceVerificationResult<bool> {
    let mut conn = pool.get().map_err(super::original_source_storage)?;
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(super::original_source_storage)?;
    crate::db::object_uploads::require_active_account_write_fence_sqlite_tx(&tx, &input.account_id)
        .map_err(super::original_source_storage)?;
    let now = original_source_db_now_sqlite(&tx)?;
    let Some(head) =
        resolve_original_source_verification_head_sqlite_tx(&tx, &input.account_id, &input.job_id)?
    else {
        tx.commit().map_err(super::original_source_storage)?;
        return Ok(false);
    };
    if !eligible_head(input, &head) {
        tx.commit().map_err(super::original_source_storage)?;
        return Ok(false);
    }
    let assignment = tx
        .query_row(
            &format!(
                "SELECT {ORIGINAL_SOURCE_ASSIGNMENT_COLUMNS}
                   FROM jobs_original_source_verification_assignments
                  WHERE assignment_id = ?1"
            ),
            params![&head.assignment_id],
            original_source_assignment_from_sqlite,
        )
        .optional()
        .map_err(super::original_source_storage)?;
    let Some(assignment) = assignment else {
        tx.commit().map_err(super::original_source_storage)?;
        return Ok(false);
    };
    if !assignment_is_eligible(&assignment, &head, now) {
        tx.commit().map_err(super::original_source_storage)?;
        return Ok(false);
    }
    let authority = original_source_require_assignment_authority_sqlite(
        &tx,
        &assignment.assignment_id,
        &input.account_id,
    )?;
    if head.checked_at_ms > now
        || head.expires_at_ms <= now
        || head.managed_authority_sha256 != authority.sha256
        || head.canonical_managed_authority_json != authority.canonical_json
    {
        tx.commit().map_err(super::original_source_storage)?;
        return Ok(false);
    }
    original_source_require_operational_hold_clear_sqlite(&tx, &input.account_id, &input.job_id)?;
    let posting = recheck_original_source_subject_sqlite(
        &tx,
        &input.account_id,
        &input.job_id,
        &assignment.subject_sha256,
    )?;
    if !matches!(
        posting.source.as_str(),
        "smartrecruiters" | "smartrecruiters_import"
    ) || posting.description.trim().is_empty()
        || !active_current_membership_sqlite(&tx, &input.account_id, &posting)?
    {
        tx.commit().map_err(super::original_source_storage)?;
        return Ok(false);
    }
    let changed = tx
        .execute(
            "UPDATE jobs_original_source_verification_assignments
                SET next_attempt_at_ms = ?2, updated_at_ms = MAX(updated_at_ms, ?2)
              WHERE assignment_id = ?1 AND account_id = ?3 AND job_id = ?4
                AND subject_sha256 = ?5 AND state = 'idle' AND circuit_state = 'closed'
                AND active_attempt_id IS NULL AND lease_owner IS NULL
                AND lease_token_sha256 IS NULL AND lease_expires_at_ms IS NULL
                AND hard_deadline_at_ms IS NULL AND attempt_count < attempt_budget
                AND not_before_at_ms <= ?2 AND expires_at_ms > ?2 AND next_attempt_at_ms > ?2
                AND COALESCE(last_error_code, '') <> 'operational_hold'
                AND NOT EXISTS (
                    SELECT 1
                      FROM jobs_original_source_verification_events AS event
                     WHERE event.assignment_id = ?1
                       AND event.event_kind = 'scheduled'
                       AND event.reason_code = 'parser_upgrade_v2'
                )",
            params![
                &assignment.assignment_id,
                now,
                &input.account_id,
                &input.job_id,
                &assignment.subject_sha256,
            ],
        )
        .map_err(super::original_source_storage)?;
    if changed != 1 {
        tx.commit().map_err(super::original_source_storage)?;
        return Ok(false);
    }
    original_source_append_event_sqlite(
        &tx,
        &assignment.assignment_id,
        None,
        "scheduled",
        None,
        None,
        None,
        None,
        None,
        Some(PARSER_UPGRADE_V2_REASON),
        now,
    )?;
    tx.commit().map_err(super::original_source_storage)?;
    Ok(true)
}

fn expedite_postgres(
    pool: &DbPool,
    input: &SmartRecruitersParserV2ExpediteInput,
) -> OriginalSourceVerificationResult<bool> {
    let mut conn = pool.get_pg().map_err(super::original_source_storage)?;
    let mut tx = conn.transaction().map_err(super::original_source_storage)?;
    // Canonical mutation order is H -> M -> D -> account/assignment rows.
    lock_operational_hold_shared_postgres_tx(&mut tx).map_err(super::original_source_storage)?;
    lock_managed_cloud_release_registry_shared_postgres_tx(&mut tx)
        .map_err(super::original_source_storage)?;
    lock_discovery_account_postgres(&mut tx, &input.account_id)
        .map_err(super::original_source_storage)?;
    crate::db::object_uploads::require_active_account_write_fence_postgres_tx(
        &mut tx,
        &input.account_id,
    )
    .map_err(super::original_source_storage)?;
    let now = original_source_db_now_postgres(&mut tx)?;
    let Some(head) = resolve_original_source_verification_head_postgres_tx(
        &mut tx,
        &input.account_id,
        &input.job_id,
    )?
    else {
        tx.commit().map_err(super::original_source_storage)?;
        return Ok(false);
    };
    if !eligible_head(input, &head) {
        tx.commit().map_err(super::original_source_storage)?;
        return Ok(false);
    }
    let assignment = tx
        .query_opt(
            &format!(
                "SELECT {ORIGINAL_SOURCE_ASSIGNMENT_COLUMNS}
                   FROM jobs_original_source_verification_assignments
                  WHERE assignment_id = $1 FOR UPDATE"
            ),
            &[&head.assignment_id],
        )
        .map_err(super::original_source_storage)?
        .map(|row| original_source_assignment_from_postgres(&row));
    let Some(assignment) = assignment else {
        tx.commit().map_err(super::original_source_storage)?;
        return Ok(false);
    };
    if !assignment_is_eligible(&assignment, &head, now) {
        tx.commit().map_err(super::original_source_storage)?;
        return Ok(false);
    }
    let authority = original_source_require_assignment_authority_postgres(
        &mut tx,
        &assignment.assignment_id,
        &input.account_id,
    )?;
    if head.checked_at_ms > now
        || head.expires_at_ms <= now
        || head.managed_authority_sha256 != authority.sha256
        || head.canonical_managed_authority_json != authority.canonical_json
    {
        tx.commit().map_err(super::original_source_storage)?;
        return Ok(false);
    }
    original_source_require_operational_hold_clear_postgres(
        &mut tx,
        &input.account_id,
        &input.job_id,
    )?;
    let posting = recheck_original_source_subject_postgres(
        &mut tx,
        &input.account_id,
        &input.job_id,
        &assignment.subject_sha256,
    )?;
    if !matches!(
        posting.source.as_str(),
        "smartrecruiters" | "smartrecruiters_import"
    ) || posting.description.trim().is_empty()
        || !active_current_membership_postgres(&mut tx, &input.account_id, &posting)?
    {
        tx.commit().map_err(super::original_source_storage)?;
        return Ok(false);
    }
    let changed = tx
        .execute(
            "UPDATE jobs_original_source_verification_assignments
                SET next_attempt_at_ms = $2, updated_at_ms = GREATEST(updated_at_ms, $2)
              WHERE assignment_id = $1 AND account_id = $3 AND job_id = $4
                AND subject_sha256 = $5 AND state = 'idle' AND circuit_state = 'closed'
                AND active_attempt_id IS NULL AND lease_owner IS NULL
                AND lease_token_sha256 IS NULL AND lease_expires_at_ms IS NULL
                AND hard_deadline_at_ms IS NULL AND attempt_count < attempt_budget
                AND not_before_at_ms <= $2 AND expires_at_ms > $2 AND next_attempt_at_ms > $2
                AND last_error_code IS DISTINCT FROM 'operational_hold'
                AND NOT EXISTS (
                    SELECT 1
                      FROM jobs_original_source_verification_events AS event
                     WHERE event.assignment_id = $1
                       AND event.event_kind = 'scheduled'
                       AND event.reason_code = 'parser_upgrade_v2'
                )",
            &[
                &assignment.assignment_id,
                &now,
                &input.account_id,
                &input.job_id,
                &assignment.subject_sha256,
            ],
        )
        .map_err(super::original_source_storage)?;
    if changed != 1 {
        tx.commit().map_err(super::original_source_storage)?;
        return Ok(false);
    }
    original_source_append_event_postgres(
        &mut tx,
        &assignment.assignment_id,
        None,
        "scheduled",
        None,
        None,
        None,
        None,
        None,
        Some(PARSER_UPGRADE_V2_REASON),
        now,
    )?;
    tx.commit().map_err(super::original_source_storage)?;
    Ok(true)
}
