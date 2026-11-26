//! Backend-parity proof for a signed original-source verifier handoff.
//!
//! The scenario uses the ordinary managed release/runtime fixture and real
//! Greenhouse and SmartRecruiters discovery completions. It does not fabricate
//! a serving head or call a provider.

use super::*;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[path = "original_source_signed_worker_http_fixture.rs"]
mod worker_http;

const ACCOUNT_ID: &str = "acct-jobs";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SourceCase {
    Greenhouse,
    SmartRecruiters,
    SmartRecruitersWorkerWire,
    SmartRecruitersWorkerHttp,
}

impl SourceCase {
    fn uses_worker_fixture(self) -> bool {
        matches!(
            self,
            Self::SmartRecruitersWorkerWire | Self::SmartRecruitersWorkerHttp
        )
    }

    fn label(self) -> &'static str {
        match self {
            Self::Greenhouse => "greenhouse",
            Self::SmartRecruiters
            | Self::SmartRecruitersWorkerWire
            | Self::SmartRecruitersWorkerHttp => "smartrecruiters",
        }
    }

    fn expected_parser_version(self) -> &'static str {
        match self {
            Self::Greenhouse => "greenhouse.original_source.v2",
            Self::SmartRecruiters
            | Self::SmartRecruitersWorkerWire
            | Self::SmartRecruitersWorkerHttp => "smartrecruiters.original_source.v3",
        }
    }

    fn expected_application_domain(self) -> &'static str {
        match self {
            Self::Greenhouse => "boards.greenhouse.io",
            Self::SmartRecruiters
            | Self::SmartRecruitersWorkerWire
            | Self::SmartRecruitersWorkerHttp => "jobs.smartrecruiters.com",
        }
    }

    fn expected_parser_digest(self) -> String {
        original_source_sha256(
            format!(
                "bluey-jobs-original-source-parser-v1\0{}\0{}\n",
                self.label(),
                self.expected_parser_version()
            )
            .as_bytes(),
        )
    }
}

#[derive(Debug, PartialEq, Eq)]
struct AssignmentSnapshot {
    assignment_id: String,
    generation: i64,
    assignment_sha256: String,
    predecessor_assignment_sha256: Option<String>,
    managed_authority_sha256: String,
    subject_sha256: String,
    state: String,
    attempt_count: i64,
    active_attempt_id: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
struct AttemptSnapshot {
    attempt_id: String,
    assignment_id: String,
    account_id: String,
    job_id: String,
    subject_sha256: String,
    attempt_no: i64,
    worker_id: String,
    runtime_instance_id: String,
    runtime_instance_epoch: i64,
    runtime_authority_sha256: String,
    runtime_session_token_sha256: String,
    lease_token_sha256: String,
    claimed_at_ms: i64,
    initial_lease_expires_at_ms: i64,
    hard_deadline_at_ms: i64,
}

#[derive(Debug, PartialEq, Eq)]
struct RejectedPredecessorSnapshot {
    assignment: AssignmentSnapshot,
    attempt: AttemptSnapshot,
    attempt_rows: i64,
    observation_rows: i64,
    receipt_rows: i64,
    head_rows: i64,
    transition_rows: i64,
    event_rows: i64,
}

fn greenhouse_posting(suffix: &str) -> JobPosting {
    let now = now_ms();
    let external_id = format!("signed-handoff-{suffix}");
    JobPosting {
        id: String::new(),
        canonical_key: String::new(),
        source: "greenhouse_import".to_string(),
        external_id: external_id.clone(),
        company: "Signed Handoff Fixture".to_string(),
        title: "Platform Engineer".to_string(),
        location: "New York, NY".to_string(),
        workplace: "hybrid".to_string(),
        canonical_url: format!(
            "https://boards.greenhouse.io/signedhandofffixture/jobs/{external_id}"
        ),
        description: "Build reliable systems with Rust and SQL.".to_string(),
        compensation: "$120k-$160k".to_string(),
        employment_type: "full_time direct_hire".to_string(),
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

fn smartrecruiters_posting(suffix: &str) -> JobPosting {
    let now = now_ms();
    let external_id = format!("signed-handoff-{suffix}");
    JobPosting {
        id: String::new(),
        canonical_key: String::new(),
        source: "smartrecruiters_import".to_string(),
        external_id: external_id.clone(),
        company: "Signed Handoff Fixture".to_string(),
        title: "Platform Engineer".to_string(),
        location: "New York, NY".to_string(),
        workplace: "hybrid".to_string(),
        canonical_url: format!(
            "https://jobs.smartrecruiters.com/SignedHandoffFixture/{external_id}-platform-engineer"
        ),
        description: "Build reliable systems with Rust and SQL.".to_string(),
        compensation: "$120k-$160k".to_string(),
        employment_type: "full_time direct_hire".to_string(),
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

fn source_posting(case: SourceCase, suffix: &str) -> JobPosting {
    match case {
        SourceCase::Greenhouse => greenhouse_posting(suffix),
        SourceCase::SmartRecruiters => smartrecruiters_posting(suffix),
        SourceCase::SmartRecruitersWorkerWire | SourceCase::SmartRecruitersWorkerHttp => {
            JobPosting {
                external_id: "smart-1".to_string(),
                company: "Acme".to_string(),
                location: "Austin, TX".to_string(),
                canonical_url: "https://jobs.smartrecruiters.com/acme/smart-1-platform-engineer"
                    .to_string(),
                description: "Build reliable systems. SQL & Rust.".to_string(),
                compensation: "USD 120000-160000 year".to_string(),
                employment_type: "full_time".to_string(),
                posted_at_ms: None,
                ..smartrecruiters_posting("unused-wire-defaults")
            }
        }
    }
}

fn expected_application_url(case: SourceCase, posting: &JobPosting) -> String {
    match case {
        SourceCase::Greenhouse => posting.canonical_url.clone(),
        SourceCase::SmartRecruiters => format!(
            "https://jobs.smartrecruiters.com/SignedHandoffFixture/{}?oga=true",
            posting.external_id
        ),
        SourceCase::SmartRecruitersWorkerWire | SourceCase::SmartRecruitersWorkerHttp => {
            "https://jobs.smartrecruiters.com/acme/smart-1-platform-engineer?oga=true".to_string()
        }
    }
}

fn expected_observed_url(case: SourceCase, posting: &JobPosting) -> String {
    match case {
        SourceCase::Greenhouse => posting.canonical_url.clone(),
        SourceCase::SmartRecruiters => format!(
            "https://jobs.smartrecruiters.com/SignedHandoffFixture/{}",
            posting.external_id
        ),
        SourceCase::SmartRecruitersWorkerWire | SourceCase::SmartRecruitersWorkerHttp => {
            "https://jobs.smartrecruiters.com/acme/smart-1".to_string()
        }
    }
}

fn runtime_binding(
    fixture: &super::managed_cloud_release_authority_tests::
        OriginalSourceVerifierRuntimeTestFixture,
) -> OriginalSourceVerifierBinding {
    OriginalSourceVerifierBinding {
        worker_id: fixture.worker_id.clone(),
        runtime_instance_id: fixture.runtime_instance_id.clone(),
        runtime_instance_epoch: fixture.runtime_instance_epoch,
        runtime_authority_sha256: fixture.runtime_authority_sha256.clone(),
        runtime_session_token: fixture.runtime_session_token.clone(),
        boot_id: None,
        boot_generation: None,
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WorkerWireTrace {
    heartbeat_bodies: [String; 2],
    completion_body: String,
}

fn worker_wire_completion(
    pool: &DbPool,
    lease: &OriginalSourceVerificationLease,
    binding: &OriginalSourceVerifierBinding,
    runtime_identity_sha256: &str,
) -> OriginalSourceVerificationCompletionRequest {
    let run_root = std::env::var_os("BLUEY_TEST_RUN_ROOT").expect("canonical launcher required");
    let node = worker_wire_node().expect("owned fixture requires the pinned Node executable");
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let fixture = root.join("jobs/scripts/fixtures/nonprod-smartrecruiters-worker-wire.mjs");
    assert!(
        root.starts_with(&run_root),
        "worker wire requires staged source"
    );
    assert!(Path::new(&node).is_absolute());
    let mut child = Command::new(node)
        .current_dir(root)
        .env_clear()
        .env("BLUEY_TEST_RUN_ROOT", run_root)
        .arg(fixture)
        .arg("--synthetic-lease-wire")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start isolated worker wire fixture");
    let input = serde_json::json!({
        "binding": binding,
        "lease": lease,
        "runtimeIdentitySha256": runtime_identity_sha256,
    });
    child
        .stdin
        .take()
        .unwrap()
        .write_all(serde_json::to_string(&input).unwrap().as_bytes())
        .expect("deliver one synthetic worker lease");
    let output = child.wait_with_output().expect("worker wire result");
    assert!(
        output.status.success() && output.stdout.len() <= 131_072,
        "worker wire fixture did not finish: {:?}",
        output.status.code()
    );
    let trace: WorkerWireTrace =
        serde_json::from_slice(&output.stdout).expect("exact worker wire trace");
    for (index, body) in trace.heartbeat_bodies.iter().enumerate() {
        let request: OriginalSourceVerificationHeartbeatRequest =
            serde_json::from_str(body).expect("actual worker heartbeat body");
        assert!(
            &request.binding == binding
                && request.assignment_id == lease.assignment_id
                && request.attempt_id == lease.attempt_id
                && request.fence == lease.fence
                && request.lease_token == lease.lease_token
        );
        assert_eq!(
            request.heartbeat_sequence,
            lease.heartbeat_sequence + index as i64 + 1
        );
        let result = heartbeat_original_source_verification(pool, &request)
            .expect("live attempt accepts the worker heartbeat");
        assert!(!result.replayed);
        assert_eq!(result.heartbeat_sequence, request.heartbeat_sequence);
    }
    let completion: OriginalSourceVerificationCompletionRequest =
        serde_json::from_str(&trace.completion_body).expect("actual worker completion body");
    assert!(
        &completion.binding == binding
            && completion.assignment_id == lease.assignment_id
            && completion.attempt_id == lease.attempt_id
            && completion.fence == lease.fence
            && completion.lease_token == lease.lease_token
    );
    assert_eq!(
        completion.observation.worker_runtime_identity_sha256,
        runtime_identity_sha256
    );
    let request_id = format!(
        "bluey-jobs-original-source-terminal-request-v1\0{}\0{}\0{}",
        lease.assignment_id, lease.attempt_id, lease.fence
    );
    assert_eq!(
        completion.request_id,
        format!(
            "osv-terminal-{}",
            original_source_sha256(request_id.as_bytes())
        )
    );
    // The worker digest is consumed as received. Rust never repairs or reseals it.
    completion
}

fn worker_wire_effect_census(pool: &DbPool) -> Vec<i64> {
    const TABLES: &[&str] = &[
        "jobs_applications",
        "jobs_resume_versions",
        "jobs_resume_generations",
        "jobs_local_run_tickets",
        "jobs_attempt_reservations",
        "jobs_execution_leases",
        "jobs_browser_sessions",
        "jobs_workflow_commands",
        "jobs_submission_evidence_capacity",
        "jobs_application_evidence",
        "jobs_run_events",
        "jobs_packet_metering",
        "jobs_managed_cloud_irreversible_effect_receipts",
    ];
    TABLES
        .iter()
        .map(|table| {
            let sql = format!("SELECT COUNT(*) FROM {table}");
            match pool {
                DbPool::Sqlite(_) => pool
                    .get()
                    .unwrap()
                    .query_row(&sql, [], |r| r.get(0))
                    .unwrap(),
                DbPool::Postgres(_) => pool.get_pg().unwrap().query_one(&sql, &[]).unwrap().get(0),
            }
        })
        .collect()
}

fn worker_wire_node() -> Option<PathBuf> {
    let run_root = PathBuf::from(std::env::var_os("BLUEY_TEST_RUN_ROOT")?);
    let stage = Path::new(env!("CARGO_MANIFEST_DIR")).parent()?;
    if stage != run_root.join("source") {
        return None;
    }
    let runner = run_root.join("worker-wire-node");
    if !runner.symlink_metadata().ok()?.is_symlink() {
        return None;
    }
    runner.canonicalize().ok()
}

fn assignment_snapshot(pool: &DbPool, assignment_id: &str) -> AssignmentSnapshot {
    const SQL: &str = "SELECT assignment_id,assignment_generation,assignment_sha256,
                predecessor_assignment_sha256,managed_authority_sha256,subject_sha256,
                state,attempt_count,active_attempt_id
           FROM jobs_original_source_verification_assignments
          WHERE assignment_id=";
    match pool {
        DbPool::Sqlite(_) => pool
            .get()
            .unwrap()
            .query_row(
                &format!("{SQL}?1"),
                rusqlite::params![assignment_id],
                |row| {
                    Ok(AssignmentSnapshot {
                        assignment_id: row.get(0)?,
                        generation: row.get(1)?,
                        assignment_sha256: row.get(2)?,
                        predecessor_assignment_sha256: row.get(3)?,
                        managed_authority_sha256: row.get(4)?,
                        subject_sha256: row.get(5)?,
                        state: row.get(6)?,
                        attempt_count: row.get(7)?,
                        active_attempt_id: row.get(8)?,
                    })
                },
            )
            .unwrap(),
        DbPool::Postgres(_) => {
            let row = pool
                .get_pg()
                .unwrap()
                .query_one(&format!("{SQL}$1"), &[&assignment_id])
                .unwrap();
            AssignmentSnapshot {
                assignment_id: row.get(0),
                generation: row.get(1),
                assignment_sha256: row.get(2),
                predecessor_assignment_sha256: row.get(3),
                managed_authority_sha256: row.get(4),
                subject_sha256: row.get(5),
                state: row.get(6),
                attempt_count: row.get(7),
                active_attempt_id: row.get(8),
            }
        }
    }
}

fn current_assignment_snapshot(
    pool: &DbPool,
    account_id: &str,
    job_id: &str,
) -> AssignmentSnapshot {
    let assignment_id: String = match pool {
        DbPool::Sqlite(_) => pool
            .get()
            .unwrap()
            .query_row(
                "SELECT assignment_id
                   FROM jobs_original_source_verification_assignments
                  WHERE account_id=?1 AND job_id=?2
                  ORDER BY assignment_generation DESC LIMIT 1",
                rusqlite::params![account_id, job_id],
                |row| row.get(0),
            )
            .unwrap(),
        DbPool::Postgres(_) => pool
            .get_pg()
            .unwrap()
            .query_one(
                "SELECT assignment_id
                   FROM jobs_original_source_verification_assignments
                  WHERE account_id=$1 AND job_id=$2
                  ORDER BY assignment_generation DESC LIMIT 1",
                &[&account_id, &job_id],
            )
            .unwrap()
            .get(0),
    };
    assignment_snapshot(pool, &assignment_id)
}

fn attempt_snapshot(pool: &DbPool, attempt_id: &str) -> AttemptSnapshot {
    const SQL: &str = "SELECT attempt_id,assignment_id,account_id,job_id,subject_sha256,attempt_no,
                worker_id,runtime_instance_id,runtime_instance_epoch,runtime_authority_sha256,
                runtime_session_token_sha256,lease_token_sha256,claimed_at_ms,
                initial_lease_expires_at_ms,hard_deadline_at_ms
           FROM jobs_original_source_verification_attempts
          WHERE attempt_id=";
    match pool {
        DbPool::Sqlite(_) => pool
            .get()
            .unwrap()
            .query_row(&format!("{SQL}?1"), rusqlite::params![attempt_id], |row| {
                Ok(AttemptSnapshot {
                    attempt_id: row.get(0)?,
                    assignment_id: row.get(1)?,
                    account_id: row.get(2)?,
                    job_id: row.get(3)?,
                    subject_sha256: row.get(4)?,
                    attempt_no: row.get(5)?,
                    worker_id: row.get(6)?,
                    runtime_instance_id: row.get(7)?,
                    runtime_instance_epoch: row.get(8)?,
                    runtime_authority_sha256: row.get(9)?,
                    runtime_session_token_sha256: row.get(10)?,
                    lease_token_sha256: row.get(11)?,
                    claimed_at_ms: row.get(12)?,
                    initial_lease_expires_at_ms: row.get(13)?,
                    hard_deadline_at_ms: row.get(14)?,
                })
            })
            .unwrap(),
        DbPool::Postgres(_) => {
            let row = pool
                .get_pg()
                .unwrap()
                .query_one(&format!("{SQL}$1"), &[&attempt_id])
                .unwrap();
            AttemptSnapshot {
                attempt_id: row.get(0),
                assignment_id: row.get(1),
                account_id: row.get(2),
                job_id: row.get(3),
                subject_sha256: row.get(4),
                attempt_no: row.get(5),
                worker_id: row.get(6),
                runtime_instance_id: row.get(7),
                runtime_instance_epoch: row.get(8),
                runtime_authority_sha256: row.get(9),
                runtime_session_token_sha256: row.get(10),
                lease_token_sha256: row.get(11),
                claimed_at_ms: row.get(12),
                initial_lease_expires_at_ms: row.get(13),
                hard_deadline_at_ms: row.get(14),
            }
        }
    }
}

fn rejected_predecessor_snapshot(
    pool: &DbPool,
    assignment_id: &str,
    attempt_id: &str,
) -> RejectedPredecessorSnapshot {
    let assignment = assignment_snapshot(pool, assignment_id);
    let attempt = attempt_snapshot(pool, attempt_id);
    let counts = match pool {
        DbPool::Sqlite(_) => pool
            .get()
            .unwrap()
            .query_row(
                "SELECT
                    (SELECT COUNT(*) FROM jobs_original_source_verification_attempts
                      WHERE assignment_id=?1),
                    (SELECT COUNT(*) FROM jobs_original_source_verification_observations
                      WHERE assignment_id=?1),
                    (SELECT COUNT(*) FROM jobs_original_source_verification_receipts
                      WHERE assignment_id=?1),
                    (SELECT COUNT(*) FROM jobs_original_source_verification_heads
                      WHERE account_id=?2 AND job_id=?3),
                    (SELECT COUNT(*) FROM jobs_original_source_verification_transitions
                      WHERE account_id=?2 AND job_id=?3),
                    (SELECT COUNT(*) FROM jobs_original_source_verification_events
                      WHERE assignment_id=?1)",
                rusqlite::params![assignment_id, attempt.account_id, attempt.job_id],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                    ))
                },
            )
            .unwrap(),
        DbPool::Postgres(_) => {
            let row = pool
                .get_pg()
                .unwrap()
                .query_one(
                    "SELECT
                        (SELECT COUNT(*) FROM jobs_original_source_verification_attempts
                          WHERE assignment_id=$1),
                        (SELECT COUNT(*) FROM jobs_original_source_verification_observations
                          WHERE assignment_id=$1),
                        (SELECT COUNT(*) FROM jobs_original_source_verification_receipts
                          WHERE assignment_id=$1),
                        (SELECT COUNT(*) FROM jobs_original_source_verification_heads
                          WHERE account_id=$2 AND job_id=$3),
                        (SELECT COUNT(*) FROM jobs_original_source_verification_transitions
                          WHERE account_id=$2 AND job_id=$3),
                        (SELECT COUNT(*) FROM jobs_original_source_verification_events
                          WHERE assignment_id=$1)",
                    &[&assignment_id, &attempt.account_id, &attempt.job_id],
                )
                .unwrap();
            (
                row.get(0),
                row.get(1),
                row.get(2),
                row.get(3),
                row.get(4),
                row.get(5),
            )
        }
    };
    RejectedPredecessorSnapshot {
        assignment,
        attempt,
        attempt_rows: counts.0,
        observation_rows: counts.1,
        receipt_rows: counts.2,
        head_rows: counts.3,
        transition_rows: counts.4,
        event_rows: counts.5,
    }
}

fn managed_authority_changed_event_count(pool: &DbPool, assignment_id: &str) -> i64 {
    match pool {
        DbPool::Sqlite(_) => pool
            .get()
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM jobs_original_source_verification_events
                  WHERE assignment_id=?1 AND event_kind='superseded'
                    AND reason_code='managed_authority_changed'",
                rusqlite::params![assignment_id],
                |row| row.get(0),
            )
            .unwrap(),
        DbPool::Postgres(_) => pool
            .get_pg()
            .unwrap()
            .query_one(
                "SELECT COUNT(*) FROM jobs_original_source_verification_events
                  WHERE assignment_id=$1 AND event_kind='superseded'
                    AND reason_code='managed_authority_changed'",
                &[&assignment_id],
            )
            .unwrap()
            .get(0),
    }
}

fn scenario(pool: &DbPool, case: SourceCase, suffix: &str) {
    let profile = default_profile("jobs@example.com");
    let preferences = JobPreferences::default();
    let input = source_posting(case, suffix);
    let expected_application_url = expected_application_url(case, &input);
    let expected_observed_url = expected_observed_url(case, &input);
    let expected_parser_digest = case.expected_parser_digest();
    let (saved, predecessor) =
        super::production_positive_authority_fixture::save_production_positive_verified_import(
            pool,
            ACCOUNT_ID,
            &input,
            &profile,
            &preferences,
        );
    assert!(list_applications(pool, ACCOUNT_ID).unwrap().is_empty());

    let predecessor_authority =
        original_source_verification_authority_for_account(pool, ACCOUNT_ID)
            .unwrap()
            .expect("predecessor managed source authority");
    let predecessor_binding = runtime_binding(&predecessor);
    let predecessor_lease = lease_original_source_verification(pool, &predecessor_binding)
        .unwrap()
        .expect("predecessor claims the initial assignment");
    let predecessor_assignment = assignment_snapshot(pool, &predecessor_lease.assignment_id);
    assert_eq!(predecessor_assignment.state, "leased");
    assert_eq!(
        predecessor_assignment.active_attempt_id.as_deref(),
        Some(predecessor_lease.attempt_id.as_str())
    );
    let predecessor_request_suffix = format!("signed-worker-handoff-{}-predecessor", case.label());
    let predecessor_completion =
        super::original_source_verification_tests::public_positive_source_completion_request(
            &saved,
            &predecessor_lease,
            predecessor_binding.clone(),
            &predecessor.runtime_identity_sha256,
            &predecessor_request_suffix,
        );
    assert_eq!(
        predecessor_completion.observation.parser_version,
        case.expected_parser_version()
    );
    assert_eq!(
        predecessor_completion.observation.parser_digest,
        expected_parser_digest
    );
    if !case.uses_worker_fixture() {
        assert_eq!(
            predecessor_completion
                .observation
                .canonical_application_url
                .as_deref(),
            Some(expected_application_url.as_str())
        );
    }
    assert_eq!(
        predecessor_completion
            .observation
            .canonical_observed_url
            .as_deref(),
        Some(expected_observed_url.as_str())
    );
    assert_eq!(
        predecessor_completion
            .observation
            .application_domain
            .as_deref(),
        Some(case.expected_application_domain())
    );
    assert!(
        resolve_original_source_verification_head(pool, ACCOUNT_ID, &saved.id)
            .unwrap()
            .is_none()
    );

    let successor = super::managed_cloud_release_authority_tests::
        install_original_source_verifier_runtime_successor_test_fixture(
            pool,
            ACCOUNT_ID,
            &predecessor,
        );
    let successor_authority = original_source_verification_authority_for_account(pool, ACCOUNT_ID)
        .unwrap()
        .expect("successor managed source authority");
    assert_eq!(
        successor_authority.head_revision,
        predecessor_authority.head_revision + 1
    );
    assert_ne!(
        successor_authority.transition_sha256,
        predecessor_authority.transition_sha256
    );
    assert_ne!(
        successor_authority.activation_sha256,
        predecessor_authority.activation_sha256
    );
    assert_ne!(
        successor_authority.manifest_sha256,
        predecessor_authority.manifest_sha256
    );
    assert_eq!(
        successor_authority.release_sequence,
        predecessor_authority.release_sequence + 1
    );
    assert_ne!(
        successor.runtime_identity_sha256,
        predecessor.runtime_identity_sha256
    );
    assert_ne!(successor.worker_id, predecessor.worker_id);
    assert_ne!(
        successor.runtime_instance_id,
        predecessor.runtime_instance_id
    );

    let rediscovered = super::production_positive_authority_fixture::
        save_production_positive_verified_import_with_runtime(
            pool,
            ACCOUNT_ID,
            &input,
            &profile,
            &preferences,
        );
    assert_eq!(rediscovered.id, saved.id);
    assert_eq!(rediscovered.canonical_key, saved.canonical_key);
    assert_eq!(rediscovered.external_id, saved.external_id);
    assert_eq!(rediscovered.canonical_url, saved.canonical_url);
    assert_eq!(rediscovered.title, saved.title);
    assert_eq!(rediscovered.description, saved.description);

    let retired = assignment_snapshot(pool, &predecessor_assignment.assignment_id);
    let current = current_assignment_snapshot(pool, ACCOUNT_ID, &saved.id);
    assert_eq!(retired.state, "superseded");
    assert_eq!(retired.active_attempt_id, None);
    assert_eq!(current.generation, predecessor_assignment.generation + 1);
    assert_eq!(
        current.predecessor_assignment_sha256.as_deref(),
        Some(predecessor_assignment.assignment_sha256.as_str())
    );
    assert_eq!(
        current.subject_sha256,
        predecessor_assignment.subject_sha256
    );
    assert_ne!(
        current.managed_authority_sha256,
        predecessor_assignment.managed_authority_sha256
    );
    let (_, _, active_authority_sha256) =
        original_source_managed_authority_binding(&successor_authority).unwrap();
    assert_eq!(current.managed_authority_sha256, active_authority_sha256);
    assert_eq!(
        managed_authority_changed_event_count(pool, &predecessor_assignment.assignment_id),
        1
    );

    let projection =
        resolve_original_source_verification_projection(pool, ACCOUNT_ID, &rediscovered).unwrap();
    assert!(projection.feature_active);
    assert_eq!(projection.evidence, None);
    assert_eq!(projection.expected_head, None);
    let pending = evaluate_job_eligibility(pool, ACCOUNT_ID, &rediscovered, true, None).unwrap();
    assert!(!pending.can_prepare, "pending source proof: {pending:#?}");
    assert!(!pending.can_queue_local && !pending.can_queue_cloud && !pending.can_auto_submit);
    assert!(
        resolve_original_source_verification_head(pool, ACCOUNT_ID, &saved.id)
            .unwrap()
            .is_none()
    );

    let before_rejected_predecessor = rejected_predecessor_snapshot(
        pool,
        &predecessor_assignment.assignment_id,
        &predecessor_lease.attempt_id,
    );
    assert_eq!(before_rejected_predecessor.attempt_rows, 1);
    assert_eq!(before_rejected_predecessor.observation_rows, 0);
    assert_eq!(before_rejected_predecessor.receipt_rows, 0);
    assert_eq!(before_rejected_predecessor.head_rows, 0);
    assert!(matches!(
        lease_original_source_verification(pool, &predecessor_binding),
        Err(OriginalSourceVerificationError::ManagedRuntimeAuthorityUnavailable)
    ));
    assert!(matches!(
        complete_original_source_verification(pool, &predecessor_completion),
        Err(OriginalSourceVerificationError::ManagedRuntimeAuthorityUnavailable)
    ));
    assert_eq!(
        rejected_predecessor_snapshot(
            pool,
            &predecessor_assignment.assignment_id,
            &predecessor_lease.attempt_id,
        ),
        before_rejected_predecessor
    );

    let successor_binding = runtime_binding(&successor);
    if case == SourceCase::SmartRecruitersWorkerHttp {
        worker_http::exercise(
            pool,
            &rediscovered,
            &current,
            &successor_binding,
            &successor.runtime_identity_sha256,
        );
        assert_eq!(
            attempt_snapshot(pool, &predecessor_lease.attempt_id),
            before_rejected_predecessor.attempt
        );
        return;
    }
    let effect_counts =
        (case == SourceCase::SmartRecruitersWorkerWire).then(|| worker_wire_effect_census(pool));
    let successor_lease = lease_original_source_verification(pool, &successor_binding)
        .unwrap()
        .expect("successor claims the replacement assignment");
    assert_eq!(successor_lease.assignment_id, current.assignment_id);
    let successor_attempt = attempt_snapshot(pool, &successor_lease.attempt_id);
    assert_eq!(successor_attempt.worker_id, successor.worker_id);
    assert_eq!(
        successor_attempt.runtime_instance_id,
        successor.runtime_instance_id
    );
    assert_eq!(
        successor_attempt.runtime_authority_sha256,
        successor.runtime_authority_sha256
    );
    let successor_request_suffix = format!("signed-worker-handoff-{}-successor", case.label());
    let successor_completion = if case == SourceCase::SmartRecruitersWorkerWire {
        worker_wire_completion(
            pool,
            &successor_lease,
            &successor_binding,
            &successor.runtime_identity_sha256,
        )
    } else {
        super::original_source_verification_tests::public_positive_source_completion_request(
            &rediscovered,
            &successor_lease,
            successor_binding,
            &successor.runtime_identity_sha256,
            &successor_request_suffix,
        )
    };
    assert_eq!(
        successor_completion.observation.parser_version,
        case.expected_parser_version()
    );
    assert_eq!(
        successor_completion.observation.parser_digest,
        expected_parser_digest
    );
    assert_eq!(
        successor_completion
            .observation
            .canonical_application_url
            .as_deref(),
        Some(expected_application_url.as_str())
    );
    assert_eq!(
        successor_completion
            .observation
            .canonical_observed_url
            .as_deref(),
        Some(expected_observed_url.as_str())
    );
    assert_eq!(
        successor_completion
            .observation
            .application_domain
            .as_deref(),
        Some(case.expected_application_domain())
    );
    if case == SourceCase::SmartRecruitersWorkerWire {
        let before = rejected_predecessor_snapshot(
            pool,
            &current.assignment_id,
            &successor_lease.attempt_id,
        );
        let mut tampered = successor_completion.clone();
        tampered.observation.evidence_sha256 = "a".repeat(64);
        assert!(matches!(
            complete_original_source_verification(pool, &tampered),
            Err(OriginalSourceVerificationError::InvalidInput(message))
                if message.contains("evidence hash differs")
        ));
        assert_eq!(
            rejected_predecessor_snapshot(
                pool,
                &current.assignment_id,
                &successor_lease.attempt_id
            ),
            before
        );
    }
    let terminal = complete_original_source_verification(pool, &successor_completion).unwrap();
    assert_eq!(terminal.assignment_id, current.assignment_id);
    assert_eq!(terminal.state, "idle");
    assert!(!terminal.replayed);
    let head = terminal
        .head
        .expect("successor publishes the positive head");
    assert_eq!(head.assignment_id, current.assignment_id);
    assert_eq!(head.subject_sha256, current.subject_sha256);
    assert_eq!(head.result, "verified_open");
    assert_eq!(head.assurance, "original_verified");
    assert_eq!(
        head.canonical_application_url.as_deref(),
        Some(expected_application_url.as_str())
    );
    assert_eq!(
        head.application_domain.as_deref(),
        Some(case.expected_application_domain())
    );
    assert_eq!(
        head.parser_version.as_deref(),
        Some(case.expected_parser_version())
    );
    assert_eq!(
        head.parser_digest.as_deref(),
        Some(expected_parser_digest.as_str())
    );
    if case == SourceCase::SmartRecruitersWorkerWire {
        let saved = rejected_predecessor_snapshot(
            pool,
            &current.assignment_id,
            &successor_lease.attempt_id,
        );
        let replay = complete_original_source_verification(pool, &successor_completion).unwrap();
        assert!(replay.replayed);
        assert_eq!(replay.head.unwrap().receipt_sha256, head.receipt_sha256);
        assert_eq!(
            rejected_predecessor_snapshot(
                pool,
                &current.assignment_id,
                &successor_lease.attempt_id
            ),
            saved
        );
    }
    let completed_projection =
        resolve_original_source_verification_projection(pool, ACCOUNT_ID, &rediscovered).unwrap();
    assert_eq!(
        completed_projection
            .evidence
            .as_ref()
            .map(|evidence| evidence.original_source_status.as_str()),
        Some("verified_open")
    );
    assert!(completed_projection.expected_head.is_some());
    // A verified receipt may support review under the ordinary eligibility
    // rules. It cannot certify this fixture for an unattended submission.
    let eligibility =
        evaluate_job_eligibility(pool, ACCOUNT_ID, &rediscovered, true, None).unwrap();
    assert!(!eligibility.can_queue_local, "{eligibility:#?}");
    assert!(!eligibility.can_queue_cloud, "{eligibility:#?}");
    assert!(!eligibility.can_auto_submit, "{eligibility:#?}");
    assert_eq!(
        attempt_snapshot(pool, &predecessor_lease.attempt_id),
        before_rejected_predecessor.attempt
    );
    assert!(list_applications(pool, ACCOUNT_ID).unwrap().is_empty());
    if let Some(before) = effect_counts {
        assert_eq!(worker_wire_effect_census(pool), before);
    }
}

#[test]
fn sqlite_signed_successor_rebinds_inflight_source_assignment() {
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    for case in [SourceCase::Greenhouse, SourceCase::SmartRecruiters] {
        let pool = super::tests::test_pool();
        scenario(&pool, case, &format!("{}-{suffix}", case.label()));
    }
}

#[test]
#[serial_test::serial]
fn postgres_signed_successor_rebinds_inflight_source_assignment_owned() {
    const CASE: &str = concat!(
        "db::jobs::original_source_signed_worker_handoff_tests::",
        "postgres_signed_successor_rebinds_inflight_source_assignment_owned"
    );
    let Some(_owned_url) = crate::db::owned_postgres_test::owned_url(CASE) else {
        return;
    };
    let pool = super::tests::postgres_authenticated_worker_pool_or_skip(CASE)
        .expect("verified owned PostgreSQL must be available");
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    scenario(
        &pool,
        SourceCase::Greenhouse,
        &format!("greenhouse-{suffix}"),
    );
    println!("owned_postgres_original_source_signed_worker_handoff_complete");
}

#[test]
#[serial_test::serial]
fn postgres_signed_smartrecruiters_v3_successor_rebinds_inflight_source_assignment_owned() {
    const CASE: &str = concat!(
        "db::jobs::original_source_signed_worker_handoff_tests::",
        "postgres_signed_smartrecruiters_v3_successor_rebinds_inflight_source_assignment_owned"
    );
    let Some(_owned_url) = crate::db::owned_postgres_test::owned_url(CASE) else {
        return;
    };
    let pool = super::tests::postgres_authenticated_worker_pool_or_skip(CASE)
        .expect("verified owned PostgreSQL must be available");
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    scenario(
        &pool,
        SourceCase::SmartRecruiters,
        &format!("smartrecruiters-{suffix}"),
    );
    println!("owned_postgres_smartrecruiters_v3_signed_worker_handoff_complete");
}

#[test]
fn sqlite_smartrecruiters_worker_wire_uses_signed_successor() {
    if worker_wire_node().is_none() {
        return;
    }
    let pool = super::tests::test_pool();
    scenario(&pool, SourceCase::SmartRecruitersWorkerWire, "worker-wire");
    println!("owned_sqlite_smartrecruiters_worker_wire_complete");
}

#[test]
#[serial_test::serial]
fn postgres_smartrecruiters_worker_wire_uses_signed_successor_owned() {
    const CASE: &str = concat!(
        "db::jobs::original_source_signed_worker_handoff_tests::",
        "postgres_smartrecruiters_worker_wire_uses_signed_successor_owned"
    );
    let Some(_owned_url) = crate::db::owned_postgres_test::owned_url(CASE) else {
        return;
    };
    let pool = super::tests::postgres_authenticated_worker_pool_or_skip(CASE)
        .expect("verified owned PostgreSQL must be available");
    scenario(&pool, SourceCase::SmartRecruitersWorkerWire, "worker-wire");
    println!("owned_postgres_smartrecruiters_worker_wire_complete");
}

#[test]
#[serial_test::serial]
fn sqlite_smartrecruiters_worker_http_retries_committed_receipt() {
    if worker_wire_node().is_none() {
        return;
    }
    let pool = super::tests::test_pool();
    scenario(&pool, SourceCase::SmartRecruitersWorkerHttp, "worker-http");
    println!("owned_sqlite_smartrecruiters_worker_http_complete");
}

#[test]
#[serial_test::serial]
fn postgres_smartrecruiters_worker_http_retries_committed_receipt_owned() {
    const CASE: &str = concat!(
        "db::jobs::original_source_signed_worker_handoff_tests::",
        "postgres_smartrecruiters_worker_http_retries_committed_receipt_owned"
    );
    let Some(_owned_url) = crate::db::owned_postgres_test::owned_url(CASE) else {
        return;
    };
    let pool = super::tests::postgres_authenticated_worker_pool_or_skip(CASE)
        .expect("verified owned PostgreSQL must be available");
    scenario(&pool, SourceCase::SmartRecruitersWorkerHttp, "worker-http");
    println!("owned_postgres_smartrecruiters_worker_http_complete");
}
