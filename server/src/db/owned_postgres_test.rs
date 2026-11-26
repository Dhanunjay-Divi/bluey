//! Disposable PostgreSQL test custody shared by unit and integration cases.
//! This module is compiled for tests only; it never grants production authority.
pub(crate) const VERIFIED_SIGNUP_MANAGED_RECEIPT_CASE: &str = "jobs_workflow_dispatch::jobs_managed_request_start_postgres_test::postgres_managed_receipt_after_verified_signup";
pub(crate) const VERIFIED_SIGNUP_REVIEW_FIRST_APPROVAL_CASE: &str = "jobs_workflow_dispatch::jobs_managed_request_start_postgres_test::postgres_managed_review_first_approval_after_verified_signup";
pub(crate) const VERIFIED_SIGNUP_REVIEW_FIRST_MOCK_RECEIPT_CASE: &str = "jobs_workflow_dispatch::jobs_managed_request_start_postgres_test::postgres_review_first_mock_receipt_after_verified_signup";
const CASES: &[&str] = &[
    "db::usage_reservations::tests::postgres_completion_settlement_cache_atomic_when_owned",
    "db::jobs::postgres_local_authority_tests::postgres_selected_curated_lead_import_is_body_stale_safe_and_promotes_current_exact_row",
    "db::jobs::postgres_local_authority_tests::postgres_packet_commit_noncanonical_track_refuses_fresh_metering_but_replays_existing_owned",
    "db::jobs::canonical_track_policy_tests::postgres_workspace_representation_noncanonical_active_is_inactive_owned",
    "db::jobs::canonical_track_policy_tests::postgres_c2c_canonical_policy_and_beta_snapshot_owned",
    "db::jobs::tests::postgres_track_noncanonical_active_rejects_auto_submit_and_execution_authority_owned",
    "db::jobs::tests::postgres_stale_match_score_cannot_authorize_auto_submit_or_current_execution_owned",
    "db::jobs::tests::postgres_signed_undated_posting_cannot_reserve_owned",
    "db::jobs::tests::postgres_signed_c2c_posting_requires_candidate_submission_owned",
    "db::jobs::tests::postgres_verified_import_discovery_snapshot_owned",
    "db::jobs::tests::postgres_smartrecruiters_summary_hydration_owned",
    "db::jobs_mcp_read::database_cursor_tests::postgres_match_reader_cursor_uses_owned_database_when_configured",
    "db::jobs_mcp_operator::tls_tests::postgres_owned_mcp_operator_verified_tls_and_full_schema",
    "db::jobs_mcp_operator::tls_tests::postgres_owned_mcp_fragment_on_registered_schema",
    "db::jobs::original_source_parser_refresh_tests::postgres_selected_smartrecruiters_parser_refresh_owned",
    "db::jobs::original_source_signed_worker_handoff_tests::postgres_signed_successor_rebinds_inflight_source_assignment_owned",
    "db::jobs::original_source_signed_worker_handoff_tests::postgres_signed_smartrecruiters_v3_successor_rebinds_inflight_source_assignment_owned",
    "db::jobs::original_source_signed_worker_handoff_tests::postgres_smartrecruiters_worker_wire_uses_signed_successor_owned",
    "db::jobs::original_source_signed_worker_handoff_tests::postgres_smartrecruiters_worker_http_retries_committed_receipt_owned",
    "db::jobs::tests::postgres_workday_legacy_snapshot_rebind_owned",
    "db::jobs::tests::postgres_workday_reconciliation_hold_readers_owned",
    "db::jobs::postgres_local_authority_tests::postgres_operational_hold_writer_reader_ordering_owned",
    "db::jobs::tests::postgres_greenhouse_membership_undated_refresh_clears_date_owned",
    "db::jobs::tests::postgres_discovery_track_noncanonical_active_refuses_lease_owned",
    "db::jobs::tests::postgres_discovery_failure_lease_rotation_is_idempotent_owned",
    "db::jobs::tests::workflow_terminal_marker_replay_is_read_only_in_postgres_when_configured",
    "jobs_workflow_dispatch::jobs_managed_request_start_postgres_test::postgres_managed_start_dispatch_marks_delivery_unknown_when_configured",
    "jobs_workflow_dispatch::jobs_managed_request_start_postgres_test::postgres_managed_start_accepted_receipt_replay_is_read_only",
    "jobs_workflow_dispatch::jobs_managed_request_start_postgres_test::postgres_managed_receipt_replay_recovers_terminal_update_failure",
    "jobs_workflow_dispatch::jobs_managed_request_start_postgres_test::postgres_managed_receipt_grounds_authenticated_interview_prep",
    "jobs_workflow_dispatch::jobs_managed_request_start_postgres_test::postgres_managed_receipt_browser_downloads_interview_prep",
    VERIFIED_SIGNUP_MANAGED_RECEIPT_CASE,
    VERIFIED_SIGNUP_REVIEW_FIRST_APPROVAL_CASE,
    VERIFIED_SIGNUP_REVIEW_FIRST_MOCK_RECEIPT_CASE,
    "db::phase657_postgres_audience_cli_owned_tests_v1::postgres_owned_operator_process_check_forward_retry_when_configured",
    "db::phase657_postgres_selected_ledger_tests_v1::postgres_owned_selected_ledger_replays_fixed_migrations_when_configured",
    "db::phase657_postgres_selected_ledger_tests_v1::postgres_owned_selected_ledger_allows_absent_distinct_canonical_when_configured",
    "db::phase657_postgres_selected_ledger_tests_v1::postgres_owned_selected_ledger_and_entry_refuse_malformed_state_when_configured",
    "db::phase657_postgres_audience_shared_driver_tests_v3::postgres_owned_audience_shared_driver_rejects_recorded_legacy_and_retries_when_configured",
    "db::phase657_postgres_audience_shared_driver_tests_v3::postgres_owned_audience_shared_driver_preserves_custom_ledger_when_configured",
    "db::phase657_postgres_audience_replay_tests_v1::postgres_owned_signature_audience_historical_replay_preserves_successor_when_configured",
    "db::phase657_postgres_audience_replay_tests_v1::postgres_owned_signature_audience_replay_refuses_malformed_state_prebody_when_configured",
    "db::phase657_postgres_audience_replay_tests_v1::postgres_owned_signature_audience_049_late_ledger_failure_rolls_back_when_configured",
    "db::jobs::local_run_publication_tests::postgres_local_publication_owned_lifecycle",
    "db::jobs::local_run_publication_tests::postgres_local_publication_owned_concurrency",
    "db::jobs::local_run_publication_tests::postgres_local_publication_owned_late_rollback",
    "db::jobs::application_answer_revision_tests::postgres_answer_revision_owned_history_and_account_cascade",
    "db::jobs::answer_memory_writer_tests::postgres_verified_company_memory_lookup_save_replay_when_owned",
    "db::jobs::managed_cloud_provisioning_installation::tests::postgres_signed_provisioning_first_head_replay_and_conflicts_when_owned",
    "db::jobs::managed_cloud_provisioning_snapshot::sqlite_snapshot_tests::postgres_snapshot_null_head_canonical_and_unknown_manifest_when_owned",
    "db::jobs::managed_cloud_trust_rotation_revocation_tests::postgres_rotation_rejects_revoked_signer_and_accepts_clean_quorum_when_owned",
    "db::jobs::managed_cloud_trust_rotation_revocation_tests::postgres_rotation_rejects_revoked_predecessor_policy_when_owned",
    "db::jobs::managed_cloud_trust_rotation_revocation_tests::postgres_rotation_rejects_registered_revoked_successor_only_signer_when_owned",
    "db::jobs::managed_cloud_trust_rotation_revocation_tests::postgres_rotation_allows_future_effective_revocation_when_owned",
    "phase654_postgres::postgres_owned_circuit_projection_scope_cas_and_legacy_preflight",
    "phase654_postgres::postgres_owned_custom_ledger_full_bootstrap_and_replay",
    "phase654_postgres::postgres_owned_envfile_ledger_full_bootstrap_and_replay",
    "phase654_postgres::postgres_owned_rust_ambient_ledger_rollback_and_replay",
    "db::jobs::managed_cloud_provisioning_persistence::tests::c1_postgres_storage_upgrade_constraints_nonserving_and_replay_when_configured",
    "db::jobs::managed_cloud_provisioning_runtime::tests::provisioning_runtime_postgres_control_replay_and_parked_boot_when_configured",
    "db::jobs::managed_cloud_provisioning_runtime::tests::provisioning_runtime_postgres_storage_only_v2_ready_lowering_when_configured",
    "db::jobs::managed_cloud_provisioning_runtime::tests::provisioning_runtime_postgres_legacy_boot_partition_when_configured",
    "db::jobs::original_source_verification_tests::receipt_grant_binding_regression::postgres_provisioning_osv_receipt_rejects_only_grant_substitution_when_configured",
];

#[cfg(unix)]
use postgres::{Client, NoTls};
#[cfg(unix)]
use std::{
    fs,
    io::Read,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    process::Command,
};

#[cfg(not(unix))]
pub(crate) fn owned_url(case: &str) -> Option<String> {
    assert!(CASES.contains(&case), "unknown owned PostgreSQL case");
    let args: Vec<String> = std::env::args().skip(1).collect();
    if std::env::var("BLUEY_JOBS_REQUIRE_POSTGRES_TESTS").as_deref() != Ok("1")
        || !args
            .iter()
            .map(String::as_str)
            .eq([case, "--exact", "--nocapture", "--test-threads=1"])
    {
        eprintln!("PostgreSQL case not selected; exact isolated wrapper required");
        return None;
    }
    panic!("owned PostgreSQL qualification requires the Unix canonical wrapper");
}

#[cfg(unix)]
pub(crate) fn private_file(path: &Path, uid: u32) -> String {
    let metadata = fs::symlink_metadata(path).expect("owned fixture metadata");
    assert!(metadata.is_file() && !metadata.file_type().is_symlink());
    assert_eq!(metadata.uid(), uid);
    assert_eq!(metadata.mode() & 0o7777, 0o600);
    assert_eq!(metadata.nlink(), 1);
    assert!(metadata.len() <= 4096);
    let mut file = fs::File::open(path).expect("owned fixture open");
    let opened = file.metadata().expect("opened fixture metadata");
    assert_eq!(
        (
            opened.dev(),
            opened.ino(),
            opened.uid(),
            opened.mode(),
            opened.nlink(),
            opened.len()
        ),
        (
            metadata.dev(),
            metadata.ino(),
            metadata.uid(),
            metadata.mode(),
            metadata.nlink(),
            metadata.len()
        )
    );
    let mut value = String::new();
    Read::by_ref(&mut file)
        .take(4097)
        .read_to_string(&mut value)
        .expect("owned fixture read");
    assert_eq!(value.len() as u64, metadata.len());
    let after = file.metadata().expect("fixture final metadata");
    let current = fs::symlink_metadata(path).expect("fixture final path");
    for item in [after, current] {
        assert_eq!(
            (
                item.dev(),
                item.ino(),
                item.uid(),
                item.mode(),
                item.nlink(),
                item.len(),
                item.mtime(),
                item.mtime_nsec()
            ),
            (
                metadata.dev(),
                metadata.ino(),
                metadata.uid(),
                metadata.mode(),
                metadata.nlink(),
                metadata.len(),
                metadata.mtime(),
                metadata.mtime_nsec()
            )
        );
    }
    value
}

#[cfg(unix)]
// Validate the wrapper's explicitly owned cluster before invoking migrations
// or any mutation. No owner database URL or default connection is accepted.
pub(crate) fn owned_url(case: &str) -> Option<String> {
    assert!(CASES.contains(&case), "unknown owned PostgreSQL case");
    let args: Vec<String> = std::env::args().skip(1).collect();
    if std::env::var("BLUEY_JOBS_REQUIRE_POSTGRES_TESTS").as_deref() != Ok("1")
        || !args
            .iter()
            .map(String::as_str)
            .eq([case, "--exact", "--nocapture", "--test-threads=1"])
    {
        eprintln!("phase654 PostgreSQL case not selected; exact isolated wrapper required");
        return None;
    }
    assert!(std::env::var_os("BLUEY_DATABASE_URL").is_none());
    let raw = std::env::var("BLUEY_TEST_POSTGRES_URL")
        .unwrap_or_else(|_| panic!("owned PostgreSQL URL required"));
    let url = reqwest::Url::parse(&raw).unwrap_or_else(|_| panic!("invalid owned URL"));
    assert!(url.scheme() == "postgresql" && url.host_str() == Some("127.0.0.1"));
    assert_eq!(url.username(), "bluey_jobs_test");
    let database = url.path().strip_prefix('/').expect("case database path");
    let suffix = database
        .strip_prefix("bluey_jobs_case_")
        .expect("isolated case database");
    assert!(
        suffix.len() == 24
            && suffix
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    );
    assert_eq!(url.query(), Some("sslmode=disable"));
    assert!(url.fragment().is_none());
    let port = url.port().expect("owned PostgreSQL port");
    assert!(port > 1024 && port != 5432);
    let password = url.password().expect("owned PostgreSQL password");
    assert!(
        password.len() == 64
            && password
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    );

    let root = PathBuf::from(std::env::var_os("BLUEY_TEST_RUN_ROOT").expect("owned run root"));
    assert_eq!(fs::canonicalize(&root).expect("canonical run root"), root);
    let parent = root.parent().expect("run parent");
    let temp_root = fs::canonicalize("/tmp").expect("canonical local temporary root");
    assert_eq!(parent.parent(), Some(temp_root.as_path()));
    assert!(parent
        .file_name()
        .and_then(|s| s.to_str())
        .is_some_and(|s| s.starts_with("bluey-test-runs-") && s.len() > "bluey-test-runs-".len()));
    let root_meta = fs::symlink_metadata(&root).expect("run metadata");
    let parent_meta = fs::symlink_metadata(parent).expect("parent metadata");
    let id = Command::new("/usr/bin/id")
        .arg("-u")
        .env_clear()
        .output()
        .expect("current UID");
    assert!(id.status.success());
    let uid: u32 = std::str::from_utf8(&id.stdout)
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    assert!(root_meta.is_dir() && !root_meta.file_type().is_symlink());
    assert!(parent_meta.is_dir() && !parent_meta.file_type().is_symlink());
    assert_eq!(root_meta.uid(), uid);
    assert_eq!(root_meta.uid(), parent_meta.uid());
    assert_eq!(root_meta.mode() & 0o7777, 0o700);
    assert_eq!(parent_meta.mode() & 0o7777, 0o700);
    let marker = private_file(&root.join(".bluey-test-run-v2"), root_meta.uid());
    let lines: Vec<_> = marker.lines().collect();
    assert_eq!(lines.len(), 9);
    assert_eq!(lines[0], "magic=bluey-test-run-v2");
    assert_eq!(lines[1], format!("run_root={}", root.display()));
    assert_eq!(
        lines[2],
        format!("run_name={}", root.file_name().unwrap().to_str().unwrap())
    );
    assert_eq!(lines[3], format!("owner_uid={}", root_meta.uid()));
    assert_eq!(lines[8], "state=running");
    for (line, prefix) in lines[4..8].iter().zip([
        "owner_pid=",
        "process_group_id=",
        "launcher_group_id=",
        "created_epoch=",
    ]) {
        let n = line.strip_prefix(prefix).expect("marker field");
        assert!(!n.is_empty() && !n.starts_with('0') && n.bytes().all(|b| b.is_ascii_digit()));
    }
    let name = root.file_name().unwrap().to_str().unwrap();
    let parts: Vec<_> = name.split('.').collect();
    assert!(
        parts.len() == 4
            && parts[0] == "run"
            && parts[1] == &lines[7]["created_epoch=".len()..]
            && parts[2] == &lines[4]["owner_pid=".len()..]
            && !parts[3].is_empty()
            && parts[3].len() <= 64
            && parts[3].bytes().all(|b| b.is_ascii_alphanumeric())
    );
    let password_file = private_file(&root.join("nonprod-postgres/password"), root_meta.uid());
    assert!(
        password_file == format!("{password}\n"),
        "owned credential binding mismatch"
    );
    let data = root.join("nonprod-postgres/data");
    assert_eq!(fs::canonicalize(&data).expect("owned data directory"), data);
    let data_meta = fs::symlink_metadata(&data).expect("data metadata");
    assert!(
        data_meta.is_dir()
            && data_meta.uid() == root_meta.uid()
            && data_meta.mode() & 0o7777 == 0o700
    );
    let pid_file = private_file(&data.join("postmaster.pid"), root_meta.uid());
    let parts: Vec<_> = pid_file.lines().collect();
    assert!(parts.len() >= 4 && parts[0].bytes().all(|b| b.is_ascii_digit()));
    assert_eq!(parts[1], data.to_str().expect("data path"));
    assert_eq!(parts[3], port.to_string());

    let mut conn = Client::connect(&raw, NoTls)
        .unwrap_or_else(|_| panic!("owned PostgreSQL connection failed"));
    let row = conn
        .query_one(
            "SELECT current_setting('data_directory'), host(inet_server_addr()), \
            inet_server_port(), current_database()::text, current_user::text",
            &[],
        )
        .unwrap_or_else(|_| panic!("owned PostgreSQL identity query failed"));
    assert_eq!(row.get::<_, String>(0), data.to_str().unwrap());
    assert_eq!(row.get::<_, String>(1), "127.0.0.1");
    assert_eq!(row.get::<_, i32>(2), i32::from(port));
    assert_eq!(row.get::<_, String>(3), database);
    assert_eq!(row.get::<_, String>(4), "bluey_jobs_test");
    Some(raw)
}
