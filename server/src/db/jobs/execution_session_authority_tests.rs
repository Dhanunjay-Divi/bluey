use super::*;

fn resolver_command(
    pool: &DbPool,
    suffix: &str,
    start: bool,
) -> (
    JobApplication,
    String,
    String,
    String,
    Option<JobsWorkflowCommandLease>,
) {
    let fixture_suffix = format!("session-authority-{suffix}");
    let (application, run_id, profile_id) =
        super::tests::execution_lease_fixture(pool, &fixture_suffix);
    // The canonical source fixture installs its historical synthetic production head first.
    // install_valid_original_source_managed_head publishes at authority_now_ms - 5_000;
    // recover that exact fixture clock rather than drifting its shared policy/lifetime.
    let published_at_ms: i64 = pool.get().unwrap().query_row(
        "SELECT manifest.published_at_ms FROM jobs_managed_cloud_heads head
         JOIN jobs_managed_cloud_manifests manifest
           ON manifest.manifest_sha256=head.current_manifest_sha256
         WHERE head.environment='production' AND head.region='us-east-1' AND head.channel='general'",
        [], |row| row.get(0),
    ).expect("read the exact canonical source-fixture release head");
    let authority_now_ms = published_at_ms
        .checked_add(5_000)
        .expect("bounded fixture authority clock");
    let managed_scope = ManagedCloudScope {
        environment: "staging".into(),
        region: "us-east-1".into(),
        channel: "general".into(),
    };
    let worker_id = format!("session-authority-worker-{suffix}");
    let managed =
        super::managed_cloud_release_authority_tests::install_managed_runner_runtime_test_fixture(
            pool,
            &worker_id,
            managed_scope.clone(),
            authority_now_ms,
        );
    assert_eq!(managed.scope, managed_scope);
    // An empty synthetic fleet still needs the ordinary two-phase inventory and cutover
    // authorities before managed command admission; never bypass the distribution gate.
    super::runner_volume_purge_tests::mark_current_fleet_ready(pool, &fixture_suffix, now_ms());
    assert!(workflow_command_opaque_identifier(&run_id, 128));
    let identity_id = application
        .receipt
        .pointer("/application_identity/id")
        .and_then(Value::as_str)
        .expect("production fixture has a frozen identity")
        .to_string();
    let packet = application
        .receipt
        .pointer("/approved_execution/packet")
        .cloned()
        .expect("production fixture has a frozen approved packet");
    let session_id = format!("cloud-session-{suffix}");
    let workflow_input = json!({
        "accountId": "acct-jobs",
        "applicationId": application.id,
        "idempotencyKey": run_id,
        "runner": "cloud",
        "applicationIdentityId": identity_id,
        "browserProfileId": profile_id,
        "packet": packet,
    });
    let command = new_stage_cloud_workflow_start_command(&StageCloudWorkflowStart {
        account_id: "acct-jobs".into(),
        application_id: application.id.clone(),
        run_id: run_id.clone(),
        workflow_id: new_jobs_workflow_id(),
        idempotency_key: format!("session-authority-{suffix}"),
        workflow_input,
        browser_session: BrowserSession {
            id: session_id.clone(),
            runner: "cloud".into(),
            status: "queued".into(),
            current_company: "Acme".into(),
            current_step: "Waiting".into(),
            application_id: Some(application.id.clone()),
            takeover_url: None,
            created_at_ms: 0,
            updated_at_ms: 0,
        },
        now_ms: now_ms(),
    })
    .expect("canonical START command");
    let mut connection = pool.get().unwrap();
    let tx = connection
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    let admission = admit_jobs_workflow_command_sqlite_tx(&tx, &command, true).unwrap();
    bind_managed_cloud_workflow_sqlite_tx(
        &tx,
        &managed_cloud_binding_input(&admission, managed_scope),
    )
    .expect("bind signed managed authority before dispatcher claim");
    tx.commit().unwrap();
    let claimed = if start {
        let lease = claim_jobs_workflow_command(pool, &worker_id, now_ms(), 60_000)
            .unwrap()
            .expect("claim exact START");
        assert_eq!(lease.command.id, admission.command.id);
        let (_, request_start) =
            mark_jobs_workflow_command_request_started_with_managed_cloud(pool, &lease, now_ms())
                .expect("record actual managed dispatcher request-start");
        assert!(request_start.is_some());
        Some(lease)
    } else {
        None
    };
    (application, run_id, profile_id, session_id, claimed)
}

#[test]
#[serial_test::serial]
fn sqlite_resolves_distinct_durable_start_session_after_actual_request_start() {
    let _environment = super::tests::ManagedCloudAdmissionEnvironment::staging();
    let pool = super::tests::test_pool();
    let (application, run_id, _profile_id, session_id, _lease) =
        resolver_command(&pool, "distinct", true);
    let mut connection = pool.get().unwrap();
    let tx = connection.transaction().unwrap();
    let resolved = sqlite_managed_execution_browser_session_id(
        &tx,
        "acct-jobs",
        &application.id,
        &run_id,
        None,
    )
    .expect("resolve durable START session");
    assert_eq!(resolved, session_id);
    assert_ne!(resolved, run_id);
}

#[test]
#[serial_test::serial]
fn sqlite_rejects_unstarted_durable_start() {
    let _environment = super::tests::ManagedCloudAdmissionEnvironment::staging();
    let pool = super::tests::test_pool();
    let (application, run_id, _profile_id, _session_id, _lease) =
        resolver_command(&pool, "unstarted", false);
    let mut connection = pool.get().unwrap();
    let tx = connection.transaction().unwrap();
    assert!(sqlite_managed_execution_browser_session_id(
        &tx,
        "acct-jobs",
        &application.id,
        &run_id,
        None,
    )
    .is_err());
}

#[test]
#[serial_test::serial]
fn sqlite_persists_idempotent_synthetic_start_receipt_once_after_fresh_readback() {
    let _environment = super::tests::ManagedCloudAdmissionEnvironment::staging();
    let pool = super::tests::test_pool();
    let (application, run_id, _profile_id, _session_id, lease) =
        resolver_command(&pool, "accepted-replay", true);
    let lease = lease.expect("started resolver fixture retains its exact claim");
    let temporal_run_id = "synthetic-temporal-run-accepted-replay".to_string();
    let receipt = JobsWorkflowAcceptanceReceipt {
        // This is the runner idempotency reply, not a second dispatch.
        outcome: JobsWorkflowAcceptedOutcome::AlreadyAccepted,
        request_id: lease.command.request_id.clone(),
        payload_hmac_sha256: lease.command.payload_hmac_sha256.clone(),
        workflow_id: lease.command.workflow_id.clone(),
        intervention_id: lease.command.intervention_id.clone(),
        temporal_run_id: temporal_run_id.clone(),
    };
    let replay = receipt.clone();
    let completed = complete_jobs_workflow_command(
        &pool,
        &lease,
        JobsWorkflowCommandCompletion::Accepted(receipt),
        now_ms(),
        None,
    )
    .expect("persist exact synthetic acceptance receipt");
    let replay_error = complete_jobs_workflow_command(
        &pool,
        &lease,
        JobsWorkflowCommandCompletion::Accepted(replay),
        now_ms(),
        None,
    )
    .expect_err("terminal command rejects duplicate completion on its stale lease");
    assert!(matches!(
        replay_error.downcast_ref::<JobsWorkflowCommandError>(),
        Some(JobsWorkflowCommandError::StaleLease)
    ));
    assert_eq!(completed.state, JobsWorkflowCommandState::Accepted);
    assert_eq!(
        completed.last_outcome_code.as_deref(),
        Some("already_accepted")
    );
    assert_eq!(completed.attempt_count, 1);
    assert_eq!(
        completed.temporal_run_id.as_deref(),
        Some(temporal_run_id.as_str())
    );

    let fresh = get_jobs_workflow_command(&pool, "acct-jobs", &lease.command.id)
        .expect("fresh command lookup")
        .expect("accepted command remains durable");
    assert_eq!(fresh.state, JobsWorkflowCommandState::Accepted);
    assert_eq!(fresh.attempt_count, 1);
    assert_eq!(
        fresh.temporal_run_id.as_deref(),
        Some(temporal_run_id.as_str())
    );
    assert!(claim_jobs_workflow_command(
        &pool,
        "session-authority-worker-replay",
        now_ms(),
        60_000
    )
    .expect("accepted command cannot be re-claimed")
    .is_none());
    let connection = pool.get().unwrap();
    let (attempts, terminals): (i64, i64) = connection
        .query_row(
            r#"SELECT
                (SELECT COUNT(*) FROM jobs_workflow_command_attempts WHERE command_id=?1),
                (SELECT COUNT(*) FROM jobs_workflow_command_attempt_events
                   WHERE command_id=?1 AND event_phase='terminal' AND event_kind='already_accepted')"#,
            params![lease.command.id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!((attempts, terminals), (1, 1));
    assert_eq!(fresh.application_id, application.id);
    assert_eq!(fresh.run_id, run_id);
}

#[test]
#[serial_test::serial]
fn sqlite_requires_the_exact_started_command_scope() {
    let _environment = super::tests::ManagedCloudAdmissionEnvironment::staging();
    let pool = super::tests::test_pool();
    let (application, run_id, _profile_id, session_id, _lease) =
        resolver_command(&pool, "scope", true);
    let mut connection = pool.get().unwrap();
    let tx = connection.transaction().unwrap();
    assert_eq!(
        sqlite_managed_execution_browser_session_id(
            &tx,
            "acct-jobs",
            &application.id,
            &run_id,
            None,
        )
        .unwrap(),
        session_id
    );
    for (account_id, application_id, candidate_run_id) in [
        ("foreign-account", application.id.as_str(), run_id.as_str()),
        ("acct-jobs", "foreign-application", run_id.as_str()),
        ("acct-jobs", application.id.as_str(), "foreign-run"),
    ] {
        assert!(sqlite_managed_execution_browser_session_id(
            &tx,
            account_id,
            application_id,
            candidate_run_id,
            None,
        )
        .is_err());
    }
}

#[test]
#[serial_test::serial]
fn sqlite_historical_reader_uses_exact_started_command_and_fails_closed_on_missing_material() {
    let _environment = super::tests::ManagedCloudAdmissionEnvironment::staging();
    let pool = super::tests::test_pool();
    let (application, run_id, _, session_id, _lease) = resolver_command(&pool, "history", true);
    let mut connection = pool.get().unwrap();
    let tx = connection.transaction().unwrap();
    let command_id: String = tx
        .query_row(
            "SELECT id FROM jobs_workflow_commands
        WHERE account_id='acct-jobs' AND application_id=?1 AND run_id=?2 AND command_kind='start'",
            params![application.id, run_id],
            |row| row.get(0),
        )
        .unwrap();
    let history =
        sqlite_historical_session_command(&tx, "acct-jobs", &application.id, &run_id, &command_id)
            .unwrap();
    let material = historical_execution_session_material(
        &history.command,
        "acct-jobs",
        &application.id,
        &run_id,
        history.has_request_started_event,
    )
    .unwrap();
    assert_eq!(material.0, session_id);
    assert!(history.managed_required);
    assert!(require_unmanaged_cloud_execution_sqlite_tx(
        &tx,
        "acct-jobs",
        &application.id,
        &run_id
    )
    .is_err());
    for (account, application, run, command) in [
        (
            "foreign",
            application.id.as_str(),
            run_id.as_str(),
            command_id.as_str(),
        ),
        (
            "acct-jobs",
            application.id.as_str(),
            run_id.as_str(),
            "missing-command",
        ),
        (
            "acct-jobs",
            application.id.as_str(),
            "foreign-run",
            command_id.as_str(),
        ),
    ] {
        assert!(
            sqlite_historical_session_command(&tx, account, application, run, command).is_err()
        );
    }
    // Corrupt only the in-memory row to exercise the real decryptor. No immutable DB payload
    // is rewritten and no substitute command/parser is accepted by the historical resolver.
    let mut row = tx
        .query_row(
            &format!("SELECT {WORKFLOW_COMMAND_SELECT} FROM jobs_workflow_commands WHERE id=?1"),
            params![command_id],
            sqlite_workflow_command_row,
        )
        .unwrap();
    row.command_json = "bluey-jobs:v1:missing-historical-key-material".into();
    assert!(workflow_command_from_stored(row).is_err());
}

#[test]
#[serial_test::serial]
fn sqlite_checkpoint_legacy_session_contract_requires_no_managed_history() {
    let pool = super::tests::test_pool();
    let (application, run_id, profile_id) =
        super::tests::execution_lease_fixture(&pool, "checkpoint-legacy-history");
    let grant = claim_execution_lease(
        &pool,
        "acct-jobs",
        &application.id,
        &run_id,
        &profile_id,
        "legacy-owner",
    )
    .unwrap();
    let mut connection = pool.get().unwrap();
    let tx = connection.transaction().unwrap();
    let lease = tx.query_row("SELECT run_id,account_id,application_id,browser_profile_id,
        owner_id,lease_token_sha256,fence,phase,lease_expires_at_ms FROM jobs_execution_leases WHERE run_id=?1",
        params![run_id], execution_lease_from_sqlite_row).unwrap();
    assert_eq!(lease.fence, grant.fence);
    assert_eq!(
        sqlite_checkpoint_browser_session_id(&tx, "acct-jobs", &application, &run_id, &lease)
            .unwrap(),
        run_id
    );
}

#[test]
#[serial_test::serial]
fn sqlite_receipt_session_companion_rejects_nonterminal_lease_and_foreign_account() {
    let pool = super::tests::test_pool();
    let (application, run_id, profile_id) =
        super::tests::execution_lease_fixture(&pool, "receipt-session-legacy-scope");
    let grant = claim_execution_lease(
        &pool,
        "acct-jobs",
        &application.id,
        &run_id,
        &profile_id,
        "legacy-owner",
    )
    .unwrap();
    let authority = ExecutionReceiptAuthority {
        owner_id: "legacy-owner".into(),
        lease_token_sha256: managed_cloud_sha256(grant.lease_token.as_bytes()),
        fence: grant.fence,
        phase: "submitted".into(),
    };
    assert!(matches!(
        execution_receipt_browser_session_id(
            &pool,
            "acct-jobs",
            &application.id,
            &run_id,
            &authority,
        ),
        Err(ExecutionLeaseError::Conflict),
    ));
    assert!(execution_receipt_browser_session_id(
        &pool,
        "foreign-account",
        &application.id,
        &run_id,
        &authority,
    )
    .is_err());
    assert!(get_browser_session(&pool, "foreign-account", &run_id)
        .unwrap()
        .is_none());
    let session = get_browser_session(&pool, "acct-jobs", &run_id)
        .unwrap()
        .unwrap();
    assert_eq!(session.id, run_id);
    assert_eq!(
        session.application_id.as_deref(),
        Some(application.id.as_str())
    );
    let mut connection = pool.get().unwrap();
    let tx = connection.transaction().unwrap();
    let lease = sqlite_execution_session_lease(&tx, "acct-jobs", &application.id, &run_id).unwrap();
    assert_eq!(
        sqlite_checkpoint_browser_session_id(&tx, "acct-jobs", &application, &run_id, &lease,)
            .unwrap(),
        run_id
    );
}
