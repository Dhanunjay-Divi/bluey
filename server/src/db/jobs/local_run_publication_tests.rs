use super::production_positive_authority_fixture::{
    install_production_positive_job_authorities, save_production_positive_verified_import,
};
use super::*;
use serde_json::json;
use std::sync::{Arc, Barrier};

struct PublicationFixture {
    application: JobApplication,
    posting: JobPosting,
    run_id: String,
}

fn approved_local_publication_fixture(pool: &DbPool, suffix: &str) -> PublicationFixture {
    let account_id = "acct-jobs";
    let email = "jobs@example.com";
    assert_eq!(
        crate::db::jobs_beta_access::evaluate_or_enroll_public_beta(pool, account_id)
            .expect("admit publication account through beta authority")
            .reason,
        crate::db::jobs_beta_access::PublicBetaAccessReason::Admitted
    );
    let profile = {
        let now = now_ms();
        let source = ResumeSourceAsset {
            id: format!("publication-source-{suffix}"),
            file_name: "publication-source.pdf".into(),
            media_type: "application/pdf".into(),
            file_type: "pdf".into(),
            storage_key: format!("accounts/{account_id}/jobs/publication-source-{suffix}.pdf"),
            sha256: "e".repeat(64),
            size_bytes: 1_024,
            page_count: Some(1),
            template_status: "converted_layout".into(),
            created_at_ms: now,
            updated_at_ms: now,
        };
        let mut profile = default_profile(email);
        profile.onboarding_complete = true;
        profile.source_resume_name = source.file_name.clone();
        profile.source_resume_asset_id = source.id.clone();
        profile.source_resume_sha256 = source.sha256.clone();
        profile.source_resume_media_type = source.media_type.clone();
        profile.source_resume_template_status = source.template_status.clone();
        save_resume_source_asset(pool, account_id, &source, &profile)
            .expect("save publication source resume")
            .1
    };
    let preferences = save_preferences(
        pool,
        account_id,
        &JobPreferences {
            sponsorship: "not_required".into(),
            ..JobPreferences::default()
        },
    )
    .expect("save publication preferences");
    let track = list_tracks(pool, account_id)
        .expect("list publication tracks")
        .into_iter()
        .find(|track| track.id == "track-default")
        .expect("default publication track");
    upsert_track(pool, account_id, &track).expect("confirm publication track authority");
    set_entitlement_plan(pool, account_id, "pro").expect("enable local publication entitlement");

    let now = now_ms();
    let input = JobPosting {
        id: String::new(),
        canonical_key: String::new(),
        source: "greenhouse_import".into(),
        external_id: format!("publication-{suffix}"),
        company: "Acme".into(),
        title: "Software Engineer".into(),
        location: "New York, NY".into(),
        workplace: "hybrid".into(),
        canonical_url: format!("https://boards.greenhouse.io/acme/jobs/publication-{suffix}"),
        description: "Build reliable products with Rust and TypeScript.".into(),
        compensation: "$170k-$200k".into(),
        employment_type: "full_time".into(),
        track_id: "track-default".into(),
        match_score: 0,
        matched_reasons: Vec::new(),
        missing_requirements: Vec::new(),
        posted_at_ms: Some(now.saturating_sub(60_000)),
        last_verified_at_ms: Some(now),
        availability_status: "active".into(),
        status: "matched".into(),
        created_at_ms: 0,
        updated_at_ms: 0,
        discovery_evidence: JobDiscoveryEvidence::default(),
        eligibility: None,
    };
    let (posting, managed) =
        save_production_positive_verified_import(pool, account_id, &input, &profile, &preferences);
    let posting = install_production_positive_job_authorities(
        pool, account_id, &posting, &managed, "acme.com", suffix,
    )
    .posting;
    let (application, _) =
        prepare_application(pool, account_id, &posting.id, "factual", "review_first")
            .expect("prepare publication application");
    let authority =
        current_application_approval_authority(pool, account_id, email, &application.id)
            .expect("load current publication authority")
            .expect("current publication authority exists");
    let resume = get_resume_version(
        pool,
        account_id,
        authority
            .application
            .resume_version_id
            .as_deref()
            .expect("publication resume id"),
    )
    .expect("load publication resume")
    .expect("publication resume exists");
    let identity_id = authority
        .application
        .receipt
        .pointer("/application_identity/id")
        .and_then(Value::as_str)
        .expect("publication identity")
        .to_string();
    let identity_email = authority
        .application
        .receipt
        .pointer("/application_identity/email")
        .and_then(Value::as_str)
        .expect("publication identity email");
    let packet = json!({
        "applicationId": authority.application.id, "jobId": authority.posting.id,
        "resumeVersionId": resume.id, "resumeContent": resume.content,
        "coverLetterContent": authority.application.cover_letter, "answers": {},
        "verifiedClaimIds": resume.claim_ids, "applicationIdentityId": identity_id,
        "applicationEmail": identity_email,
        "browserProfileId": execution_browser_profile_id(account_id, &identity_id),
    });
    let job = json!({
        "externalId": authority.posting.external_id, "canonicalUrl": authority.posting.canonical_url,
        "company": authority.posting.company, "title": authority.posting.title,
        "location": authority.posting.location, "workplace": authority.posting.workplace,
        "description": authority.posting.description, "source": authority.posting.source,
        "compensation": authority.posting.compensation,
    });
    let admission = json!({"kind":"review_approval"});
    let approved = json!({
        "schema_version": 2, "approved_at_ms": authority.evaluated_at_ms,
        "checksum": approved_submission_checksum(2, &packet, &job, Some(&admission))
            .expect("publication approved checksum"),
        "admission": admission, "packet": packet, "job": job,
    });
    let application =
        persist_current_application_approval(pool, account_id, email, &authority, &approved)
            .expect("persist publication approval")
            .expect("approved publication application");
    reserve_application_attempt(pool, account_id, &application.id, "local")
        .expect("reserve publication local attempt");
    // Packet metering is a normal pre-publication effect. Session/run/ticket publication remains
    // exclusively under the transaction being tested.
    commit_packet(pool, account_id, &application.id).expect("commit publication packet");
    let application = update_application(pool, account_id, &application.id, "queued", None)
        .expect("queue publication application")
        .expect("queued publication application");
    PublicationFixture {
        application,
        posting,
        run_id: format!("local-publication-{suffix}"),
    }
}

fn publication_request(fixture: &PublicationFixture) -> PublishLocalApplicationRun {
    let account_id = "acct-jobs";
    let snapshot = approved_submission_snapshot(account_id, &fixture.application)
        .expect("approved publication snapshot");
    let schema = fixture.application.receipt["approved_execution"]["schema_version"]
        .as_i64()
        .expect("approved publication schema");
    let identity = fixture
        .application
        .receipt
        .pointer("/application_identity/id")
        .and_then(Value::as_str)
        .expect("publication request identity");
    let resume = fixture
        .application
        .resume_version_id
        .as_deref()
        .expect("publication request resume");
    let mut packet = snapshot.packet.clone();
    let values = packet.as_object_mut().expect("approved packet object");
    values.insert("approvedPacketChecksum".into(), json!(snapshot.checksum));
    values.insert("approvedExecutionSchemaVersion".into(), json!(schema));
    values.insert(
        "approvedExecutionAdmission".into(),
        fixture.application.receipt["approved_execution"]["admission"].clone(),
    );
    let session = BrowserSession {
        id: fixture.run_id.clone(),
        runner: "local".into(),
        status: "queued".into(),
        current_company: fixture.posting.company.clone(),
        current_step: "Waiting for Bluey Browser".into(),
        application_id: Some(fixture.application.id.clone()),
        takeover_url: None,
        created_at_ms: 0,
        updated_at_ms: 0,
    };
    PublishLocalApplicationRun {
        account_id: account_id.into(),
        expected_application: fixture.application.clone(),
        run_id: fixture.run_id.clone(),
        ticket_secret: "a".repeat(64),
        ticket_hash: hex::encode(Sha256::digest("a".repeat(64).as_bytes())),
        expires_at_ms: now_ms().saturating_add(60_000),
        browser_session: session,
        workflow_input: json!({
            "accountId": account_id, "applicationId": fixture.application.id,
            "jobId": fixture.posting.id, "canonicalJobKey": fixture.posting.canonical_key,
            "packetId": resume, "applicationIdentityId": identity,
            "browserProfileId": execution_browser_profile_id(account_id, identity),
            "packet": packet, "job": snapshot.job, "runner": "local",
            "url": fixture.posting.canonical_url, "idempotencyKey": fixture.run_id,
            "runId": fixture.run_id, "browserSessionId": fixture.run_id,
        }),
    }
}

fn publication_snapshot(pool: &DbPool, application_id: &str, run_id: &str) -> Value {
    let connection = pool.get().expect("publication snapshot connection");
    connection
        .query_row(
            "SELECT json_object(
             'application', (SELECT application_json FROM jobs_applications WHERE id=?1),
             'session', (SELECT session_json FROM jobs_browser_sessions WHERE id=?2),
             'ticket', (SELECT payload_json FROM jobs_local_run_tickets WHERE id=?2))",
            rusqlite::params![application_id, run_id],
            |row| row.get::<_, String>(0),
        )
        .map(|raw| serde_json::from_str(&raw).expect("publication snapshot JSON"))
        .expect("read publication snapshot")
}

#[test]
fn sqlite_local_publication_binds_exact_approved_snapshot_and_replays_without_reset() {
    let pool = tests::test_pool();
    let fixture = approved_local_publication_fixture(&pool, "success-replay");
    let request = publication_request(&fixture);
    let first = publish_local_application_run(&pool, &request).expect("publish exact local run");
    assert_eq!(
        first.application.run_id.as_deref(),
        Some(fixture.run_id.as_str())
    );
    assert_eq!(first.browser_session.id, fixture.run_id);
    assert_eq!(first.ticket.ticket_secret, request.ticket_secret);
    assert_eq!(first.ticket.status, "queued");
    assert!(matches!(
        publish_local_application_run(&pool, &request),
        Err(error) if matches!(
            error.downcast_ref::<LocalRunPublicationError>(),
            Some(LocalRunPublicationError::Conflict)
        )
    ));
    let mut replay_request = publication_request(&fixture);
    replay_request.expected_application = first.application.clone();
    replay_request.browser_session = first.browser_session.clone();
    let before = publication_snapshot(&pool, &fixture.application.id, &fixture.run_id);
    let replay = publish_local_application_run(&pool, &replay_request)
        .expect("replay fresh local publication snapshot");
    assert_eq!(replay.ticket.ticket_secret, first.ticket.ticket_secret);
    assert_eq!(replay.ticket.expires_at_ms, first.ticket.expires_at_ms);
    assert_eq!(
        publication_snapshot(&pool, &fixture.application.id, &fixture.run_id),
        before
    );
}

#[test]
fn sqlite_local_publication_rejects_changed_payload_session_run_and_stale_snapshot_without_writes()
{
    let pool = tests::test_pool();
    let fixture = approved_local_publication_fixture(&pool, "refusals");
    let baseline = publication_snapshot(&pool, &fixture.application.id, &fixture.run_id);
    for alter in ["payload", "session", "run", "snapshot"] {
        let mut candidate = publication_request(&fixture);
        match alter {
            "payload" => candidate.workflow_input["url"] = json!("https://invalid.example.test"),
            "session" => candidate.browser_session.id = "different-session".into(),
            "run" => candidate.run_id = "different-run".into(),
            "snapshot" => candidate
                .expected_application
                .answers
                .push(json!({"changed": true})),
            _ => unreachable!(),
        }
        assert!(matches!(
            publish_local_application_run(&pool, &candidate),
            Err(error) if matches!(
                error.downcast_ref::<LocalRunPublicationError>(),
                Some(LocalRunPublicationError::Conflict)
            )
        ));
        assert_eq!(
            publication_snapshot(&pool, &fixture.application.id, &fixture.run_id),
            baseline
        );
    }
}

#[test]
fn sqlite_local_publication_refuses_terminal_or_reapproved_current_application_without_partial_rows(
) {
    let pool = tests::test_pool();
    let fixture = approved_local_publication_fixture(&pool, "terminal");
    let request = publication_request(&fixture);
    let terminal = update_application(&pool, "acct-jobs", &fixture.application.id, "failed", None)
        .expect("terminalize publication application")
        .expect("terminal publication application");
    let terminal_before = publication_snapshot(&pool, &fixture.application.id, &fixture.run_id);
    assert!(matches!(
        publish_local_application_run(&pool, &request),
        Err(error) if matches!(
            error.downcast_ref::<LocalRunPublicationError>(),
            Some(LocalRunPublicationError::Conflict)
        )
    ));
    assert_eq!(
        serde_json::to_value(
            get_application(&pool, "acct-jobs", &fixture.application.id)
                .expect("reload terminal publication application"),
        )
        .expect("serialize terminal publication application"),
        serde_json::to_value(Some(terminal)).expect("serialize expected terminal application")
    );
    assert_eq!(
        publication_snapshot(&pool, &fixture.application.id, &fixture.run_id),
        terminal_before
    );

    let pool = tests::test_pool();
    let fixture = approved_local_publication_fixture(&pool, "reapproval");
    let request = publication_request(&fixture);
    let awaiting = update_application(
        &pool,
        "acct-jobs",
        &fixture.application.id,
        "awaiting_review",
        None,
    )
    .expect("return application to review")
    .expect("review publication application");
    let mut changed_receipt = awaiting.receipt.clone();
    changed_receipt
        .as_object_mut()
        .expect("publication receipt object")
        .remove("approved_execution");
    replace_application_receipt(&pool, "acct-jobs", &fixture.application.id, changed_receipt)
        .expect("invalidate changed publication approval");
    let before = publication_snapshot(&pool, &fixture.application.id, &fixture.run_id);
    assert!(matches!(
        publish_local_application_run(&pool, &request),
        Err(error) if matches!(
            error.downcast_ref::<LocalRunPublicationError>(),
            Some(LocalRunPublicationError::Conflict)
        )
    ));
    assert_eq!(
        publication_snapshot(&pool, &fixture.application.id, &fixture.run_id),
        before
    );
}

#[test]
fn sqlite_local_publication_concurrent_exact_requests_publish_once_and_refuse_stale_contender() {
    let pool = Arc::new(tests::test_pool());
    let fixture = approved_local_publication_fixture(&pool, "concurrent");
    let barrier = Arc::new(Barrier::new(2));
    let mut workers = Vec::new();
    for request in [publication_request(&fixture), publication_request(&fixture)] {
        let pool = Arc::clone(&pool);
        let barrier = Arc::clone(&barrier);
        workers.push(std::thread::spawn(move || {
            barrier.wait();
            publish_local_application_run(&pool, &request)
        }));
    }
    let first = workers.remove(0).join().expect("first publication worker");
    let second = workers.remove(0).join().expect("second publication worker");
    let outcomes = [first, second];
    assert_eq!(outcomes.iter().filter(|outcome| outcome.is_ok()).count(), 1);
    assert!(outcomes.iter().any(|outcome| matches!(
        outcome,
        Err(error) if matches!(
            error.downcast_ref::<LocalRunPublicationError>(),
            Some(LocalRunPublicationError::Conflict)
        )
    )));
    let connection = pool
        .get()
        .expect("concurrent publication assertion connection");
    assert_eq!(
        connection
            .query_row(
                "SELECT COUNT(*) FROM jobs_local_run_tickets WHERE id=?1",
                rusqlite::params![fixture.run_id],
                |row| row.get::<_, i64>(0),
            )
            .expect("count concurrent publication tickets"),
        1
    );
}

#[test]
fn sqlite_local_publication_late_ticket_failure_rolls_back_application_and_session() {
    let pool = tests::test_pool();
    let fixture = approved_local_publication_fixture(&pool, "late-rollback");
    let request = publication_request(&fixture);
    let before = publication_snapshot(&pool, &fixture.application.id, &fixture.run_id);
    pool.get()
        .expect("late rollback trigger connection")
        .execute_batch(
            "CREATE TRIGGER phase962_ticket_insert_failure
               BEFORE INSERT ON jobs_local_run_tickets
               BEGIN SELECT RAISE(FAIL, 'phase962 synthetic late ticket failure'); END",
        )
        .expect("install disposable late ticket failure trigger");
    assert!(publish_local_application_run(&pool, &request).is_err());
    assert_eq!(
        publication_snapshot(&pool, &fixture.application.id, &fixture.run_id),
        before
    );
}

#[test]
fn sqlite_local_publication_refuses_orphan_noncurrent_ticket_statuses() {
    let pool = tests::test_pool();
    let fixture = approved_local_publication_fixture(&pool, "orphan-ticket");
    let orphan_run = "orphan_local_publication_ticket";
    save_local_run_ticket(
        &pool,
        "acct-jobs",
        &fixture.application.id,
        orphan_run,
        &"1".repeat(64),
        &"x".repeat(32),
        json!({"negativeFixture": true}),
        now_ms().saturating_sub(1),
    )
    .expect("seed untrusted orphan ticket negative fixture");
    for status in [
        "queued",
        "claimed",
        "needs_input",
        "click_started",
        "side_effect_unknown",
        "complete",
        "unknown",
    ] {
        pool.get()
            .expect("orphan ticket fixture connection")
            .execute(
                "UPDATE jobs_local_run_tickets SET status=?2 WHERE id=?1",
                rusqlite::params![orphan_run, status],
            )
            .expect("set untrusted orphan ticket status");
        let before = publication_snapshot(&pool, &fixture.application.id, &fixture.run_id);
        assert!(matches!(
            publish_local_application_run(&pool, &publication_request(&fixture)),
            Err(error) if matches!(
                error.downcast_ref::<LocalRunPublicationError>(),
                Some(LocalRunPublicationError::Conflict)
            )
        ));
        assert_eq!(
            publication_snapshot(&pool, &fixture.application.id, &fixture.run_id),
            before
        );
    }
}

#[test]
fn sqlite_local_publication_refuses_fresh_replay_after_updated_only_application_drift() {
    let pool = tests::test_pool();
    let fixture = approved_local_publication_fixture(&pool, "updated-only");
    let first = publish_local_application_run(&pool, &publication_request(&fixture))
        .expect("publish updated-only fixture");
    let mut replay = publication_request(&fixture);
    replay.expected_application = first.application;
    replay.browser_session = first.browser_session;
    pool.get()
        .expect("updated-only drift connection")
        .execute(
            "UPDATE jobs_applications SET updated_at_ms=updated_at_ms+1 WHERE id=?1",
            rusqlite::params![fixture.application.id],
        )
        .expect("create synthetic updated-only application drift");
    let before = publication_snapshot(&pool, &fixture.application.id, &fixture.run_id);
    assert!(matches!(
        publish_local_application_run(&pool, &replay),
        Err(error) if matches!(
            error.downcast_ref::<LocalRunPublicationError>(),
            Some(LocalRunPublicationError::Conflict)
        )
    ));
    assert_eq!(
        publication_snapshot(&pool, &fixture.application.id, &fixture.run_id),
        before
    );
}

fn bootstrap_owned_postgres_publication_pool(url: &str) -> DbPool {
    use crate::db::{accounts::Account, jobs_beta_access as beta};

    let pool = crate::db::open_postgres_pool(url).expect("open owned publication PostgreSQL pool");
    crate::db::run_migrations(&pool).expect("migrate owned publication PostgreSQL pool");
    let mut connection = pool
        .get_pg()
        .expect("owned publication PostgreSQL connection");
    connection
        .execute(
            "INSERT INTO accounts (id,email,password_hash,trial_seconds_remaining)
             VALUES ('acct-jobs','jobs@example.com','synthetic-test-hash',0)",
            &[],
        )
        .expect("create owned publication account");
    drop(connection);
    assert_eq!(
        Account::mark_email_verified(&pool, "acct-jobs").expect("verify publication account"),
        1
    );
    let admin = Account::create_with_admin(
        &pool,
        &format!(
            "publication-beta-admin-{}@example.test",
            uuid::Uuid::new_v4().simple()
        ),
        "synthetic-admin-test-hash",
        true,
    )
    .expect("create owned publication beta administrator");
    assert_eq!(
        Account::mark_email_verified(&pool, &admin.id)
            .expect("verify publication beta administrator"),
        1
    );
    let cohort = beta::get_public_beta_cohort(&pool).expect("load publication beta cohort");
    let now = now_ms();
    beta::update_public_beta_cohort_audited(
        &pool,
        beta::PublicBetaCohortUpdate {
            expected_revision: cohort.revision,
            state: beta::PublicBetaCohortState::Open,
            opens_at_ms: Some(now.saturating_sub(60_000)),
            closes_at_ms: Some(now.saturating_add(3_600_000)),
            hard_cap: cohort.hard_cap.max(1),
        },
        &admin.id,
    )
    .expect("open owned publication beta cohort");
    assert_eq!(
        beta::evaluate_or_enroll_public_beta(&pool, "acct-jobs")
            .expect("admit owned publication account")
            .reason,
        beta::PublicBetaAccessReason::Admitted
    );
    let identity = ensure_primary_application_identity(&pool, "acct-jobs", "jobs@example.com")
        .expect("create owned publication identity");
    upsert_track(
        &pool,
        "acct-jobs",
        &CareerTrack {
            id: "track-default".into(),
            name: "Software engineering".into(),
            role: "Software Engineer".into(),
            locations: vec!["New York, NY".into()],
            remote_preference: "hybrid_ok".into(),
            application_identity_id: Some(identity.id),
            policy: CareerTrackPolicy {
                role_family: "software_engineering".into(),
                ..CareerTrackPolicy::default()
            },
            active: true,
            match_count: 0,
            created_at_ms: 0,
            updated_at_ms: 0,
        },
    )
    .expect("create owned publication track");
    pool
}

#[test]
fn postgres_local_publication_owned_lifecycle() {
    let Some(url) = crate::db::owned_postgres_test::owned_url(
        "db::jobs::local_run_publication_tests::postgres_local_publication_owned_lifecycle",
    ) else {
        return;
    };
    let pool = Arc::new(bootstrap_owned_postgres_publication_pool(&url));
    let fixture = approved_local_publication_fixture(&pool, "pg-lifecycle");
    let first_request = publication_request(&fixture);
    let first = publish_local_application_run(&pool, &first_request)
        .expect("publish owned PostgreSQL local run");
    assert!(publish_local_application_run(&pool, &first_request).is_err());
    let mut replay_request = publication_request(&fixture);
    replay_request.expected_application = first.application.clone();
    replay_request.browser_session = first.browser_session.clone();
    let replay = publish_local_application_run(&pool, &replay_request)
        .expect("replay fresh owned PostgreSQL local run");
    assert_eq!(replay.ticket.ticket_secret, first.ticket.ticket_secret);
    assert_eq!(replay.ticket.status, first.ticket.status);
    assert_eq!(replay.ticket.expires_at_ms, first.ticket.expires_at_ms);
    println!("phase656_owned_local_publication_postgres_lifecycle_complete");
}

#[test]
fn postgres_local_publication_owned_concurrency() {
    let Some(url) = crate::db::owned_postgres_test::owned_url(
        "db::jobs::local_run_publication_tests::postgres_local_publication_owned_concurrency",
    ) else {
        return;
    };
    let pool = Arc::new(bootstrap_owned_postgres_publication_pool(&url));
    let concurrent = approved_local_publication_fixture(&pool, "pg-concurrent");
    let barrier = Arc::new(Barrier::new(2));
    let mut workers = Vec::new();
    for candidate in [
        publication_request(&concurrent),
        publication_request(&concurrent),
    ] {
        let pool = Arc::clone(&pool);
        let barrier = Arc::clone(&barrier);
        workers.push(std::thread::spawn(move || {
            barrier.wait();
            publish_local_application_run(&pool, &candidate)
        }));
    }
    let first_concurrent = workers
        .remove(0)
        .join()
        .expect("first owned PostgreSQL worker");
    let second_concurrent = workers
        .remove(0)
        .join()
        .expect("second owned PostgreSQL worker");
    let outcomes = [first_concurrent, second_concurrent];
    assert_eq!(outcomes.iter().filter(|outcome| outcome.is_ok()).count(), 1);
    assert!(outcomes.iter().any(|outcome| matches!(
        outcome,
        Err(error) if matches!(
            error.downcast_ref::<LocalRunPublicationError>(),
            Some(LocalRunPublicationError::Conflict)
        )
    )));
    println!("phase656_owned_local_publication_postgres_concurrency_complete");
}

#[test]
fn postgres_local_publication_owned_late_rollback() {
    let Some(url) = crate::db::owned_postgres_test::owned_url(
        "db::jobs::local_run_publication_tests::postgres_local_publication_owned_late_rollback",
    ) else {
        return;
    };
    let pool = Arc::new(bootstrap_owned_postgres_publication_pool(&url));
    let rollback = approved_local_publication_fixture(&pool, "pg-late-rollback");
    let rollback_request = publication_request(&rollback);
    let mut connection = pool
        .get_pg()
        .expect("owned PostgreSQL late rollback connection");
    connection
        .batch_execute(
            "CREATE OR REPLACE FUNCTION phase962_ticket_insert_failure() RETURNS trigger
               LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'phase962 synthetic late ticket failure'; END $$;
             CREATE TRIGGER phase962_ticket_insert_failure
               BEFORE INSERT ON jobs_local_run_tickets
               FOR EACH ROW EXECUTE FUNCTION phase962_ticket_insert_failure();",
        )
        .expect("install owned PostgreSQL late ticket failure trigger");
    drop(connection);
    assert!(publish_local_application_run(&pool, &rollback_request).is_err());
    let mut connection = pool
        .get_pg()
        .expect("owned PostgreSQL rollback assertion connection");
    let counts: (i64, i64) = connection
        .query_one(
            "SELECT
               (SELECT COUNT(*) FROM jobs_browser_sessions WHERE id=$1),
               (SELECT COUNT(*) FROM jobs_local_run_tickets WHERE id=$1)",
            &[&rollback.run_id],
        )
        .map(|row| (row.get(0), row.get(1)))
        .expect("read owned PostgreSQL rollback rows");
    assert_eq!(counts, (0, 0));
    assert_eq!(
        serde_json::to_value(
            get_application(&pool, "acct-jobs", &rollback.application.id)
                .expect("reload owned PostgreSQL rollback application")
                .expect("owned PostgreSQL rollback application"),
        )
        .expect("serialize owned PostgreSQL rollback application"),
        serde_json::to_value(rollback.application)
            .expect("serialize expected owned PostgreSQL rollback application")
    );
    println!("phase656_owned_local_publication_postgres_late_rollback_complete");
}
