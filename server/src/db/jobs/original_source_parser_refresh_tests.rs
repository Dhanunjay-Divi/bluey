//! Backend-parity tests for the selected SmartRecruiters parser refresh.
//!
//! Each scenario first creates an ordinary managed-runtime assignment and
//! completes a real current-parser positive head. The only synthetic step is
//! a tightly scoped, disposable-database substitution of that observation's
//! parser metadata to the historical v1 pair. This is a selection fixture,
//! not evidence that a signed v1 or v2 runtime was qualified or deployed.
//! Completion uses whichever current parser version the ordinary fixture emits;
//! the V2 oracle below is test-only.

use super::original_source_parser_refresh::{
    expedite_smartrecruiters_parser_v2, expedite_smartrecruiters_parser_v2_historical_test_oracle,
    SmartRecruitersParserV2ExpediteInput,
};
use super::*;

const ACCOUNT_ID: &str = "acct-jobs";
const FOREIGN_ACCOUNT_ID: &str = "acct-parser-refresh-foreign";
const SQLITE_UPDATE_TRIGGER: &str = "trg_jobs_original_source_observation_no_update";
const POSTGRES_UPDATE_TRIGGER: &str =
    "trg_jobs_original_source_verification_observations_no_update";

#[derive(Debug, PartialEq, Eq)]
struct RefreshSnapshot {
    head: OriginalSourceVerificationHead,
    assignment_state: String,
    attempt_count: i64,
    attempt_budget: i64,
    next_attempt_at_ms: i64,
    updated_at_ms: i64,
    active_attempt_id: Option<String>,
    last_error_code: Option<String>,
    attempt_rows: i64,
    observation_rows: i64,
    receipt_rows: i64,
    transition_rows: i64,
    head_rows: i64,
    event_rows: i64,
    parser_upgrade_events: i64,
}

fn smartrecruiters_posting(suffix: &str) -> JobPosting {
    let now = now_ms();
    let external_id = format!("parserrefresh{suffix}");
    JobPosting {
        id: String::new(),
        canonical_key: String::new(),
        source: "smartrecruiters_import".to_string(),
        external_id: external_id.clone(),
        company: "Parser Refresh Fixture".to_string(),
        title: "Platform Engineer".to_string(),
        location: "New York, NY".to_string(),
        workplace: "hybrid".to_string(),
        canonical_url: format!("https://jobs.smartrecruiters.com/ParserRefresh/{external_id}"),
        description: "Build reliable systems with Rust and SQL.".to_string(),
        compensation: "$120k-$160k".to_string(),
        employment_type: "full_time".to_string(),
        track_id: "track-default".to_string(),
        match_score: 0,
        matched_reasons: Vec::new(),
        missing_requirements: Vec::new(),
        posted_at_ms: Some(now.saturating_sub(60_000)),
        last_verified_at_ms: Some(now),
        availability_status: "active".to_string(),
        status: "matched".to_string(),
        created_at_ms: 0,
        updated_at_ms: 0,
        discovery_evidence: JobDiscoveryEvidence::default(),
        eligibility: None,
    }
}

fn insert_foreign_account(pool: &DbPool) {
    match pool {
        DbPool::Sqlite(_) => {
            pool.get()
                .unwrap()
                .execute(
                    "INSERT INTO accounts(id,email,password_hash,trial_seconds_remaining)
                     VALUES(?1,'parser-refresh-foreign@example.test','hash',0)",
                    rusqlite::params![FOREIGN_ACCOUNT_ID],
                )
                .unwrap();
        }
        DbPool::Postgres(_) => {
            pool.get_pg()
                .unwrap()
                .execute(
                    "INSERT INTO accounts(id,email,password_hash,trial_seconds_remaining)
                     VALUES($1,'parser-refresh-foreign@example.test','hash',0)",
                    &[&FOREIGN_ACCOUNT_ID],
                )
                .unwrap();
        }
    }
}

fn parser_v1_digest() -> String {
    original_source_sha256(
        b"bluey-jobs-original-source-parser-v1\0smartrecruiters\0smartrecruiters.original_source.v1\n",
    )
}

fn substitute_parser_metadata(
    pool: &DbPool,
    receipt_id: &str,
    parser_version: &str,
    parser_digest: &str,
) {
    match pool {
        DbPool::Sqlite(_) => {
            let mut connection = pool.get().unwrap();
            let transaction = connection
                .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
                .unwrap();
            transaction
                .execute_batch(&format!("DROP TRIGGER {SQLITE_UPDATE_TRIGGER};"))
                .unwrap();
            assert_eq!(
                transaction
                    .execute(
                        "UPDATE jobs_original_source_verification_observations
                            SET parser_version=?2, parser_digest=?3
                          WHERE observation_id=(
                            SELECT observation_id
                              FROM jobs_original_source_verification_receipts
                             WHERE receipt_id=?1)",
                        rusqlite::params![receipt_id, parser_version, parser_digest],
                    )
                    .unwrap(),
                1
            );
            transaction
                .execute_batch(&format!(
                    "CREATE TRIGGER {SQLITE_UPDATE_TRIGGER}
                     BEFORE UPDATE ON jobs_original_source_verification_observations
                     BEGIN SELECT RAISE(ABORT, 'original-source observations are immutable'); END;"
                ))
                .unwrap();
            transaction.commit().unwrap();
            let connection = pool.get().unwrap();
            assert!(connection
                .execute(
                    "UPDATE jobs_original_source_verification_observations
                        SET parser_version=parser_version
                      WHERE observation_id=(
                        SELECT observation_id
                          FROM jobs_original_source_verification_receipts
                         WHERE receipt_id=?1)",
                    rusqlite::params![receipt_id],
                )
                .is_err());
        }
        DbPool::Postgres(_) => {
            let mut connection = pool.get_pg().unwrap();
            let mut transaction = connection.transaction().unwrap();
            transaction
                .batch_execute(&format!(
                    "ALTER TABLE jobs_original_source_verification_observations
                       DISABLE TRIGGER {POSTGRES_UPDATE_TRIGGER};"
                ))
                .unwrap();
            assert_eq!(
                transaction
                    .execute(
                        "UPDATE jobs_original_source_verification_observations
                            SET parser_version=$2, parser_digest=$3
                          WHERE observation_id=(
                            SELECT observation_id
                              FROM jobs_original_source_verification_receipts
                             WHERE receipt_id=$1)",
                        &[&receipt_id, &parser_version, &parser_digest],
                    )
                    .unwrap(),
                1
            );
            transaction
                .batch_execute(&format!(
                    "ALTER TABLE jobs_original_source_verification_observations
                       ENABLE TRIGGER {POSTGRES_UPDATE_TRIGGER};"
                ))
                .unwrap();
            transaction.commit().unwrap();
            let mut connection = pool.get_pg().unwrap();
            let mut transaction = connection.transaction().unwrap();
            assert!(transaction
                .execute(
                    "UPDATE jobs_original_source_verification_observations
                        SET parser_version=parser_version
                      WHERE observation_id=(
                        SELECT observation_id
                          FROM jobs_original_source_verification_receipts
                         WHERE receipt_id=$1)",
                    &[&receipt_id],
                )
                .is_err());
            transaction.rollback().unwrap();
        }
    }
}

fn snapshot(pool: &DbPool, account_id: &str, job_id: &str) -> RefreshSnapshot {
    let head = resolve_original_source_verification_head(pool, account_id, job_id)
        .unwrap()
        .unwrap();
    let assignment_id = head.assignment_id.clone();
    let values = match pool {
        DbPool::Sqlite(_) => pool
            .get()
            .unwrap()
            .query_row(
                "SELECT assignment.state, assignment.attempt_count,
                        assignment.attempt_budget, assignment.next_attempt_at_ms,
                        assignment.updated_at_ms, assignment.active_attempt_id,
                        assignment.last_error_code,
                        (SELECT COUNT(*) FROM jobs_original_source_verification_attempts
                          WHERE assignment_id=assignment.assignment_id),
                        (SELECT COUNT(*) FROM jobs_original_source_verification_observations
                          WHERE assignment_id=assignment.assignment_id),
                        (SELECT COUNT(*) FROM jobs_original_source_verification_receipts
                          WHERE assignment_id=assignment.assignment_id),
                        (SELECT COUNT(*) FROM jobs_original_source_verification_transitions
                          WHERE account_id=assignment.account_id AND job_id=assignment.job_id),
                        (SELECT COUNT(*) FROM jobs_original_source_verification_heads
                          WHERE account_id=assignment.account_id AND job_id=assignment.job_id),
                        (SELECT COUNT(*) FROM jobs_original_source_verification_events
                          WHERE assignment_id=assignment.assignment_id),
                        (SELECT COUNT(*) FROM jobs_original_source_verification_events
                          WHERE assignment_id=assignment.assignment_id
                            AND event_kind='scheduled' AND reason_code='parser_upgrade_v2')
                   FROM jobs_original_source_verification_assignments assignment
                  WHERE assignment.assignment_id=?1",
                rusqlite::params![assignment_id],
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
                    ))
                },
            )
            .unwrap(),
        DbPool::Postgres(_) => {
            let row = pool
                .get_pg()
                .unwrap()
                .query_one(
                    "SELECT assignment.state, assignment.attempt_count,
                            assignment.attempt_budget, assignment.next_attempt_at_ms,
                            assignment.updated_at_ms, assignment.active_attempt_id,
                            assignment.last_error_code,
                            (SELECT COUNT(*) FROM jobs_original_source_verification_attempts
                              WHERE assignment_id=assignment.assignment_id),
                            (SELECT COUNT(*) FROM jobs_original_source_verification_observations
                              WHERE assignment_id=assignment.assignment_id),
                            (SELECT COUNT(*) FROM jobs_original_source_verification_receipts
                              WHERE assignment_id=assignment.assignment_id),
                            (SELECT COUNT(*) FROM jobs_original_source_verification_transitions
                              WHERE account_id=assignment.account_id AND job_id=assignment.job_id),
                            (SELECT COUNT(*) FROM jobs_original_source_verification_heads
                              WHERE account_id=assignment.account_id AND job_id=assignment.job_id),
                            (SELECT COUNT(*) FROM jobs_original_source_verification_events
                              WHERE assignment_id=assignment.assignment_id),
                            (SELECT COUNT(*) FROM jobs_original_source_verification_events
                              WHERE assignment_id=assignment.assignment_id
                                AND event_kind='scheduled' AND reason_code='parser_upgrade_v2')
                       FROM jobs_original_source_verification_assignments assignment
                      WHERE assignment.assignment_id=$1",
                    &[&assignment_id],
                )
                .unwrap();
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
            )
        }
    };
    RefreshSnapshot {
        head,
        assignment_state: values.0,
        attempt_count: values.1,
        attempt_budget: values.2,
        next_attempt_at_ms: values.3,
        updated_at_ms: values.4,
        active_attempt_id: values.5,
        last_error_code: values.6,
        attempt_rows: values.7,
        observation_rows: values.8,
        receipt_rows: values.9,
        transition_rows: values.10,
        head_rows: values.11,
        event_rows: values.12,
        parser_upgrade_events: values.13,
    }
}

fn assignment_is_due(pool: &DbPool, assignment_id: &str) -> bool {
    match pool {
        DbPool::Sqlite(_) => {
            pool.get()
                .unwrap()
                .query_row(
                    "SELECT next_attempt_at_ms <= CAST(unixepoch('subsec') * 1000 AS INTEGER)
                   FROM jobs_original_source_verification_assignments
                  WHERE assignment_id=?1",
                    rusqlite::params![assignment_id],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap()
                == 1
        }
        DbPool::Postgres(_) => pool
            .get_pg()
            .unwrap()
            .query_one(
                "SELECT next_attempt_at_ms <=
                        CAST(EXTRACT(EPOCH FROM clock_timestamp()) * 1000 AS BIGINT)
                   FROM jobs_original_source_verification_assignments
                  WHERE assignment_id=$1",
                &[&assignment_id],
            )
            .unwrap()
            .get(0),
    }
}

fn set_fixture_due_time(pool: &DbPool, assignment_id: &str, due_ms: i64) {
    let changed = match pool {
        DbPool::Sqlite(_) => pool
            .get()
            .unwrap()
            .execute(
                "UPDATE jobs_original_source_verification_assignments
                    SET next_attempt_at_ms=?2 WHERE assignment_id=?1",
                rusqlite::params![assignment_id, due_ms],
            )
            .unwrap() as u64,
        DbPool::Postgres(_) => pool
            .get_pg()
            .unwrap()
            .execute(
                "UPDATE jobs_original_source_verification_assignments
                    SET next_attempt_at_ms=$2 WHERE assignment_id=$1",
                &[&assignment_id, &due_ms],
            )
            .unwrap(),
    };
    assert_eq!(changed, 1);
}

fn set_fixture_membership_missing(pool: &DbPool, job_id: &str, missing_since: Option<i64>) {
    let missing_count = i64::from(missing_since.is_some());
    let changed = match pool {
        DbPool::Sqlite(_) => pool
            .get()
            .unwrap()
            .execute(
                "UPDATE jobs_discovery_memberships
                    SET missing_count=?3, missing_since_at_ms=?4
                  WHERE account_id=?1 AND job_id=?2",
                rusqlite::params![ACCOUNT_ID, job_id, missing_count, missing_since],
            )
            .unwrap() as u64,
        DbPool::Postgres(_) => pool
            .get_pg()
            .unwrap()
            .execute(
                "UPDATE jobs_discovery_memberships
                    SET missing_count=$3, missing_since_at_ms=$4
                  WHERE account_id=$1 AND job_id=$2",
                &[&ACCOUNT_ID, &job_id, &missing_count, &missing_since],
            )
            .unwrap(),
    };
    assert_eq!(changed, 1);
}

fn scenario(pool: &DbPool, suffix: &str) {
    insert_foreign_account(pool);
    let posting = smartrecruiters_posting(suffix);
    let profile = default_profile("jobs@example.com");
    let preferences = JobPreferences::default();
    let (posting, managed) =
        super::production_positive_authority_fixture::save_production_positive_verified_import(
            pool,
            ACCOUNT_ID,
            &posting,
            &profile,
            &preferences,
        );
    let current =
        super::original_source_verification_tests::complete_public_original_source_positive_fixture(
            pool,
            &posting,
            &managed,
            &format!("parser-refresh-{suffix}"),
        );
    assert_eq!(current.result, "verified_open");
    assert_eq!(current.assignment_state, "idle");
    assert_ne!(
        current.parser_version.as_deref(),
        Some("smartrecruiters.original_source.v1")
    );

    let input = SmartRecruitersParserV2ExpediteInput {
        account_id: ACCOUNT_ID.to_string(),
        job_id: posting.id.clone(),
        expected_head_revision: current.head_revision,
        expected_receipt_sha256: current.receipt_sha256.clone(),
    };

    substitute_parser_metadata(
        pool,
        &current.receipt_id,
        "smartrecruiters.original_source.v1",
        &"0".repeat(64),
    );
    let wrong_digest_before = snapshot(pool, ACCOUNT_ID, &posting.id);
    assert!(!expedite_smartrecruiters_parser_v2_historical_test_oracle(pool, &input).unwrap());
    assert_eq!(snapshot(pool, ACCOUNT_ID, &posting.id), wrong_digest_before);

    substitute_parser_metadata(
        pool,
        &current.receipt_id,
        "smartrecruiters.original_source.v1",
        &parser_v1_digest(),
    );
    let legacy = snapshot(pool, ACCOUNT_ID, &posting.id);
    assert_eq!(legacy.head.assignment_state, "idle");
    assert_eq!(
        legacy.head.parser_version.as_deref(),
        Some("smartrecruiters.original_source.v1")
    );
    assert!(!assignment_is_due(pool, &legacy.head.assignment_id));

    // The current-version wrapper remains tied to the actual parser. At V3
    // it cannot schedule this virtual V2 head; only the historical oracle
    // below may exercise the retired path.
    let (current_version, _) = original_source_parser_metadata("smartrecruiters");
    assert_eq!(current_version, "smartrecruiters.original_source.v3");
    assert!(!expedite_smartrecruiters_parser_v2(pool, &input).unwrap());
    assert_eq!(snapshot(pool, ACCOUNT_ID, &posting.id), legacy);

    let mut wrong_receipt = input.clone();
    wrong_receipt.expected_receipt_sha256 = "f".repeat(64);
    assert!(
        !expedite_smartrecruiters_parser_v2_historical_test_oracle(pool, &wrong_receipt).unwrap()
    );
    assert_eq!(snapshot(pool, ACCOUNT_ID, &posting.id), legacy);

    let mut foreign = input.clone();
    foreign.account_id = FOREIGN_ACCOUNT_ID.to_string();
    assert!(!expedite_smartrecruiters_parser_v2_historical_test_oracle(pool, &foreign).unwrap());
    assert_eq!(snapshot(pool, ACCOUNT_ID, &posting.id), legacy);

    set_fixture_membership_missing(pool, &posting.id, Some(now_ms()));
    assert!(!expedite_smartrecruiters_parser_v2_historical_test_oracle(pool, &input).unwrap());
    assert_eq!(snapshot(pool, ACCOUNT_ID, &posting.id), legacy);
    set_fixture_membership_missing(pool, &posting.id, None);

    assert!(expedite_smartrecruiters_parser_v2_historical_test_oracle(pool, &input).unwrap());
    let expedited = snapshot(pool, ACCOUNT_ID, &posting.id);
    assert_eq!(expedited.head, legacy.head);
    assert_eq!(expedited.assignment_state, legacy.assignment_state);
    assert_eq!(expedited.attempt_count, legacy.attempt_count);
    assert_eq!(expedited.attempt_budget, legacy.attempt_budget);
    assert_eq!(expedited.active_attempt_id, legacy.active_attempt_id);
    assert_eq!(expedited.last_error_code, legacy.last_error_code);
    assert_eq!(expedited.attempt_rows, legacy.attempt_rows);
    assert_eq!(expedited.observation_rows, legacy.observation_rows);
    assert_eq!(expedited.receipt_rows, legacy.receipt_rows);
    assert_eq!(expedited.transition_rows, legacy.transition_rows);
    assert_eq!(expedited.head_rows, legacy.head_rows);
    assert!(expedited.next_attempt_at_ms < legacy.next_attempt_at_ms);
    assert_eq!(expedited.event_rows, legacy.event_rows + 1);
    assert_eq!(
        expedited.parser_upgrade_events,
        legacy.parser_upgrade_events + 1
    );
    assert!(assignment_is_due(pool, &expedited.head.assignment_id));

    assert!(!expedite_smartrecruiters_parser_v2_historical_test_oracle(pool, &input).unwrap());
    assert_eq!(snapshot(pool, ACCOUNT_ID, &posting.id), expedited);

    // Model the due time moving forward again, e.g. after normal retry
    // scheduling. The audit event must still fence an expedite replay.
    set_fixture_due_time(
        pool,
        &expedited.head.assignment_id,
        legacy.next_attempt_at_ms,
    );
    assert!(!assignment_is_due(pool, &expedited.head.assignment_id));
    let retry_window = snapshot(pool, ACCOUNT_ID, &posting.id);
    assert!(!expedite_smartrecruiters_parser_v2_historical_test_oracle(pool, &input).unwrap());
    assert_eq!(snapshot(pool, ACCOUNT_ID, &posting.id), retry_window);

    set_fixture_due_time(
        pool,
        &expedited.head.assignment_id,
        expedited.next_attempt_at_ms,
    );
    let binding = OriginalSourceVerifierBinding {
        worker_id: managed.worker_id.clone(),
        runtime_instance_id: managed.runtime_instance_id.clone(),
        runtime_instance_epoch: managed.runtime_instance_epoch,
        runtime_authority_sha256: managed.runtime_authority_sha256.clone(),
        runtime_session_token: managed.runtime_session_token.clone(),
        boot_id: None,
        boot_generation: None,
    };
    let lease = lease_original_source_verification(pool, &binding)
        .unwrap()
        .expect("ordinary verifier lease can claim the existing expedited assignment");
    assert_eq!(lease.assignment_id, legacy.head.assignment_id);
    let leased = snapshot(pool, ACCOUNT_ID, &posting.id);
    assert_eq!(leased.attempt_count, expedited.attempt_count + 1);
    assert_eq!(leased.attempt_rows, expedited.attempt_rows + 1);
    assert_eq!(leased.attempt_budget, expedited.attempt_budget);
    assert_eq!(leased.head.receipt_sha256, expedited.head.receipt_sha256);
}

#[test]
fn sqlite_selected_smartrecruiters_parser_refresh() {
    let pool = super::tests::test_pool();
    scenario(&pool, &uuid::Uuid::new_v4().simple().to_string());
}

#[test]
#[serial_test::serial]
fn postgres_selected_smartrecruiters_parser_refresh_owned() {
    const CASE: &str = "db::jobs::original_source_parser_refresh_tests::postgres_selected_smartrecruiters_parser_refresh_owned";
    let Some(_owned_url) = crate::db::owned_postgres_test::owned_url(CASE) else {
        return;
    };
    let pool = super::tests::postgres_authenticated_worker_pool_or_skip(CASE)
        .expect("verified owned PostgreSQL must be available");
    scenario(&pool, &uuid::Uuid::new_v4().simple().to_string());
    println!("owned_postgres_smartrecruiters_parser_refresh_complete");
}
