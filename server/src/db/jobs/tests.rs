#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    fn test_pool() -> DbPool {
        let path = std::env::temp_dir().join(format!(
            "bluey-jobs-test-{}-{}.sqlite3",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let pool = db::open_pool(&path).unwrap();
        db::run_migrations(&pool).unwrap();
        let conn = pool.get().unwrap();
        conn.execute(
            "INSERT INTO accounts (id, email, password_hash, trial_seconds_remaining)
             VALUES ('acct-jobs', 'jobs@example.com', 'hash', 0)",
            [],
        )
        .unwrap();
        drop(conn);
        let identity =
            ensure_primary_application_identity(&pool, "acct-jobs", "jobs@example.com").unwrap();
        upsert_track(
            &pool,
            "acct-jobs",
            &CareerTrack {
                id: "track-default".to_string(),
                name: "Software engineering".to_string(),
                role: "Software Engineer".to_string(),
                locations: vec!["New York, NY".to_string()],
                remote_preference: "hybrid_ok".to_string(),
                application_identity_id: Some(identity.id),
                policy: CareerTrackPolicy {
                    role_family: "software_engineering".to_string(),
                    ..CareerTrackPolicy::default()
                },
                active: true,
                match_count: 0,
                created_at_ms: 0,
                updated_at_ms: 0,
            },
        )
        .unwrap();
        pool
    }

    fn test_posting(url: &str, posted_at_ms: i64, last_verified_at_ms: i64) -> JobPosting {
        JobPosting {
            id: String::new(),
            canonical_key: String::new(),
            source: "greenhouse".to_string(),
            external_id: url.to_string(),
            company: "Acme".to_string(),
            title: "Software Engineer".to_string(),
            location: "New York, NY".to_string(),
            workplace: "hybrid".to_string(),
            canonical_url: url.to_string(),
            description: "Build reliable products with Rust and TypeScript.".to_string(),
            compensation: "$170k-$200k".to_string(),
            employment_type: "full_time".to_string(),
            track_id: "track-default".to_string(),
            match_score: 90,
            matched_reasons: vec!["Skills fit".to_string()],
            missing_requirements: Vec::new(),
            posted_at_ms: Some(posted_at_ms),
            last_verified_at_ms: Some(last_verified_at_ms),
            availability_status: "active".to_string(),
            status: "matched".to_string(),
            created_at_ms: 0,
            updated_at_ms: 0,
            eligibility: None,
        }
    }

    #[test]
    fn discovery_writers_share_one_postgres_account_lock_namespace() {
        assert_eq!(
            DISCOVERY_ACCOUNT_LOCK_SQL,
            "SELECT pg_advisory_xact_lock(hashtextextended('jobs-discovery-account:' || $1, 0))"
        );
    }

    #[test]
    fn sponsorship_detection_never_treats_explicit_rejections_as_offers() {
        let mut posting = test_posting("https://jobs.example.com/role", now_ms(), now_ms());
        for rejection in [
            "This position is not eligible for visa sponsorship.",
            "No visa sponsorship available for this role.",
            "No visa sponsorship is available for this role.",
            "This position is ineligible for sponsorship.",
            "Visa sponsorship is not available.",
        ] {
            posting.description = rejection.to_string();
            assert!(clearly_blocks_sponsorship(&posting), "{rejection}");
            assert!(!clearly_offers_sponsorship(&posting), "{rejection}");
        }

        posting.description = "This position is eligible for visa sponsorship.".to_string();
        assert!(!clearly_blocks_sponsorship(&posting));
        assert!(clearly_offers_sponsorship(&posting));
    }

    #[test]
    fn sponsorship_rejections_remain_fail_closed_for_required_and_ask_policies() {
        let pool = test_pool();
        let profile = default_profile("jobs@example.com");
        save_profile(&pool, "acct-jobs", &profile).unwrap();
        let mut preferences = JobPreferences {
            sponsorship: "required".to_string(),
            ..JobPreferences::default()
        };
        save_preferences(&pool, "acct-jobs", &preferences).unwrap();

        let mut posting = test_posting(
            "https://jobs.example.com/sponsorship-policy",
            now_ms(),
            now_ms(),
        );
        posting.description = "No visa sponsorship is available for this role.".to_string();
        let posting = upsert_posting(&pool, "acct-jobs", &posting, &profile, &preferences).unwrap();
        let required = evaluate_job_eligibility(&pool, "acct-jobs", &posting, true, None).unwrap();
        assert!(required
            .hard_failures
            .iter()
            .any(|reason| reason.code == "sponsorship_unavailable"));
        assert!(!required
            .passed_checks
            .contains(&"sponsorship_available".to_string()));

        preferences.sponsorship = "ask".to_string();
        save_preferences(&pool, "acct-jobs", &preferences).unwrap();
        let ask = evaluate_job_eligibility(&pool, "acct-jobs", &posting, true, None).unwrap();
        assert!(ask
            .review_reasons
            .iter()
            .any(|reason| reason.code == "sponsorship_answer_required"));
        assert!(!ask
            .passed_checks
            .contains(&"sponsorship_available".to_string()));
    }

    #[test]
    fn search_pace_and_auto_submit_gate_are_server_owned() {
        let pool = test_pool();
        let mut profile = default_profile("jobs@example.com");
        profile.auto_submit_threshold = 99;
        profile.daily_limit = 42;
        let saved_profile = save_profile(&pool, "acct-jobs", &profile).unwrap();
        assert_eq!(saved_profile.auto_submit_threshold, 80);
        assert_eq!(saved_profile.daily_limit, 10);

        let preferences = JobPreferences {
            daily_limit: 42,
            max_posting_age_days: 60,
            ..JobPreferences::default()
        };
        let saved_preferences = save_preferences(&pool, "acct-jobs", &preferences).unwrap();
        assert_eq!(saved_preferences.daily_limit, 10);
        assert_eq!(saved_preferences.max_posting_age_days, 14);
    }

    #[test]
    fn experience_fit_is_derived_from_profile_dates_and_posting_requirements() {
        let mut profile = default_profile("jobs@example.com");
        profile.employment = vec![EmploymentEntry {
            id: "employment-software".to_string(),
            company: "Example Company".to_string(),
            title: "Software Engineer".to_string(),
            start_date: "2022-01".to_string(),
            end_date: "2023-12".to_string(),
            ..EmploymentEntry::default()
        }];
        let track = CareerTrack {
            id: "track-software".to_string(),
            name: "Software engineering".to_string(),
            role: "Software Engineer".to_string(),
            locations: vec!["New York, NY".to_string()],
            remote_preference: "hybrid_ok".to_string(),
            application_identity_id: Some("identity-primary".to_string()),
            policy: CareerTrackPolicy {
                role_family: "software_engineering".to_string(),
                relevant_employment_ids: vec!["employment-software".to_string()],
                ..CareerTrackPolicy::default()
            },
            active: true,
            match_count: 0,
            created_at_ms: 0,
            updated_at_ms: 0,
        };
        let evidence = role_experience_evidence(
            &profile,
            Some(&track),
            &test_posting(
                "https://boards.greenhouse.io/example/jobs/evidence",
                now_ms(),
                now_ms(),
            ),
        );
        assert_eq!(evidence.total_months, 24);
        assert_eq!(evidence.target_min_months, 12);
        assert_eq!(evidence.target_max_months, 48);

        let preferences = JobPreferences {
            sponsorship: "not_required".to_string(),
            ..JobPreferences::default()
        };
        let mut aligned = test_posting(
            "https://boards.greenhouse.io/example/jobs/aligned",
            now_ms(),
            now_ms(),
        );
        aligned.track_id = track.id.clone();
        aligned.description = "Requires 4+ years of software engineering experience.".to_string();
        assert_eq!(
            experience_requirement(&aligned).required_min_months,
            Some(48)
        );
        let aligned_decision = build_job_eligibility(
            &aligned,
            &profile,
            &preferences,
            &[],
            false,
            None,
            Some(&track),
        );
        assert!(aligned_decision
            .passed_checks
            .iter()
            .any(|check| check == "experience_aligned"));

        let mut too_senior = aligned;
        too_senior.description =
            "Requires at least 5 years of software engineering experience.".to_string();
        let blocked = build_job_eligibility(
            &too_senior,
            &profile,
            &preferences,
            &[],
            false,
            None,
            Some(&track),
        );
        assert!(blocked
            .hard_failures
            .iter()
            .any(|reason| reason.code == "experience_outside_target_range"));

        let mut title_only_senior = test_posting(
            "https://boards.greenhouse.io/example/jobs/title-only-senior",
            now_ms(),
            now_ms(),
        );
        title_only_senior.track_id = track.id.clone();
        title_only_senior.title = "Senior Software Engineer".to_string();
        title_only_senior.description = "Build reliable products with Rust.".to_string();
        let blocked = build_job_eligibility(
            &title_only_senior,
            &profile,
            &preferences,
            &[],
            false,
            None,
            Some(&track),
        );
        assert!(blocked
            .hard_failures
            .iter()
            .any(|reason| reason.code == "experience_outside_target_range"));
    }

    fn execution_lease_fixture(pool: &DbPool, suffix: &str) -> (JobApplication, String, String) {
        let profile = default_profile("jobs@example.com");
        save_profile(pool, "acct-jobs", &profile).unwrap();
        let posting = upsert_posting(
            pool,
            "acct-jobs",
            &test_posting(
                &format!("https://boards.greenhouse.io/acme/jobs/{suffix}"),
                now_ms(),
                now_ms(),
            ),
            &profile,
            &JobPreferences::default(),
        )
        .unwrap();
        let (application, _) =
            prepare_application(pool, "acct-jobs", &posting.id, "factual", "review_first").unwrap();
        let application = update_application(
            pool,
            "acct-jobs",
            &application.id,
            "queued",
            Some("review_first"),
        )
        .unwrap()
        .unwrap();
        let run_id = format!("cloud-run-{suffix}");
        upsert_browser_session(
            pool,
            "acct-jobs",
            &BrowserSession {
                id: run_id.clone(),
                runner: "cloud".to_string(),
                status: "queued".to_string(),
                current_company: "Acme".to_string(),
                current_step: "Waiting for a browser".to_string(),
                application_id: Some(application.id.clone()),
                takeover_url: None,
                created_at_ms: 0,
                updated_at_ms: 0,
            },
        )
        .unwrap();
        let application = assign_application_run(pool, "acct-jobs", &application.id, &run_id)
            .unwrap()
            .unwrap();
        let identity_id = application
            .receipt
            .pointer("/application_identity/id")
            .and_then(Value::as_str)
            .unwrap();
        let browser_profile_id = execution_browser_profile_id("acct-jobs", identity_id);
        (application, run_id, browser_profile_id)
    }

    fn local_run_authority_fixture(
        pool: &DbPool,
        suffix: &str,
    ) -> (JobApplication, String, String, String) {
        let profile = default_profile("jobs@example.com");
        save_profile(pool, "acct-jobs", &profile).unwrap();
        set_entitlement_plan(pool, "acct-jobs", "pro").unwrap();
        let posting = upsert_posting(
            pool,
            "acct-jobs",
            &test_posting(
                &format!("https://jobs.ashbyhq.com/acme/{suffix}"),
                now_ms(),
                now_ms(),
            ),
            &profile,
            &JobPreferences::default(),
        )
        .unwrap();
        let (application, _) =
            prepare_application(pool, "acct-jobs", &posting.id, "factual", "review_first").unwrap();
        let application = update_application(
            pool,
            "acct-jobs",
            &application.id,
            "queued",
            Some("review_first"),
        )
        .unwrap()
        .unwrap();
        let run_id = format!("local-run-{suffix}");
        upsert_browser_session(
            pool,
            "acct-jobs",
            &BrowserSession {
                id: run_id.clone(),
                runner: "local".to_string(),
                status: "queued".to_string(),
                current_company: "Acme".to_string(),
                current_step: "Waiting for Bluey Browser".to_string(),
                application_id: Some(application.id.clone()),
                takeover_url: None,
                created_at_ms: 0,
                updated_at_ms: 0,
            },
        )
        .unwrap();
        let application = assign_application_run(pool, "acct-jobs", &application.id, &run_id)
            .unwrap()
            .unwrap();
        let identity_id = application
            .receipt
            .pointer("/application_identity/id")
            .and_then(Value::as_str)
            .unwrap()
            .to_string();
        let browser_profile_id = execution_browser_profile_id("acct-jobs", &identity_id);
        let ticket_hash = format!("ticket-hash-{suffix}");
        save_local_run_ticket(
            pool,
            "acct-jobs",
            &application.id,
            &run_id,
            &ticket_hash,
            &format!("ticket-secret-{suffix}"),
            json!({
                "accountId": "acct-jobs",
                "applicationId": application.id,
                "jobId": posting.id,
                "applicationIdentityId": identity_id,
                "browserProfileId": browser_profile_id,
                "runner": "local",
                "url": posting.canonical_url,
                "runId": run_id,
            }),
            now_ms() + 60_000,
        )
        .unwrap();
        (application, run_id, ticket_hash, identity_id)
    }

    fn discovered_job(external_id: &str, title: &str) -> DiscoveredJobInput {
        DiscoveredJobInput {
            external_id: external_id.to_string(),
            canonical_url: format!(
                "https://boards.greenhouse.io/acme/jobs/{external_id}?utm_source=test"
            ),
            company: String::new(),
            source_catalog_id: String::new(),
            requires_original_revalidation: false,
            title: title.to_string(),
            location: "New York, NY".to_string(),
            workplace: "hybrid".to_string(),
            description: "Build reliable products with Rust and TypeScript.".to_string(),
            compensation: "$170k-$200k".to_string(),
            employment_type: "full_time".to_string(),
            engagement_type: "direct_hire".to_string(),
            posted_at_ms: Some(now_ms() - DAY_MS),
        }
    }

    fn curated_discovered_job(external_id: &str, canonical_url: &str) -> DiscoveredJobInput {
        DiscoveredJobInput {
            external_id: external_id.to_string(),
            canonical_url: canonical_url.to_string(),
            company: "Acme".to_string(),
            source_catalog_id: "feed-simplify-new-grad".to_string(),
            requires_original_revalidation: true,
            title: "Software Engineer".to_string(),
            location: "New York, NY".to_string(),
            workplace: "hybrid".to_string(),
            description: "Build reliable products with Rust and TypeScript.".to_string(),
            compensation: "$170k-$200k".to_string(),
            employment_type: "full_time".to_string(),
            engagement_type: "direct_hire".to_string(),
            posted_at_ms: Some(now_ms() - DAY_MS),
        }
    }

    fn global_completion_evidence(
        expected_rows: i64,
        accepted_rows: Option<i64>,
        rejected_rows: i64,
        rejection_reasons: BTreeMap<String, i64>,
    ) -> GlobalIngestionCompleteInput {
        GlobalIngestionCompleteInput {
            lease_token: "lease-token".to_string(),
            replay_key: "replay-key".to_string(),
            scheduled_for_ms: 1,
            artifact_sha256: "a".repeat(64),
            expected_rows,
            accepted_rows,
            rejected_rows,
            rejection_reasons,
            expected_batches: 1,
            complete_snapshot: true,
        }
    }

    fn insert_global_candidate(
        pool: &DbPool,
        id: &str,
        external_id: &str,
        canonical_url: &str,
        updated_at_ms: i64,
    ) -> DiscoveredJobInput {
        insert_global_candidate_with_run_status(
            pool,
            id,
            external_id,
            canonical_url,
            updated_at_ms,
            "completed",
        )
    }

    fn insert_global_candidate_with_run_status(
        pool: &DbPool,
        id: &str,
        external_id: &str,
        canonical_url: &str,
        updated_at_ms: i64,
        run_status: &str,
    ) -> DiscoveredJobInput {
        let input = DiscoveredJobInput {
            external_id: external_id.to_string(),
            canonical_url: canonical_url.to_string(),
            company: "Acme".to_string(),
            source_catalog_id: "jobhive:lever".to_string(),
            requires_original_revalidation: true,
            title: "Software Engineer".to_string(),
            location: "New York, NY".to_string(),
            workplace: "hybrid".to_string(),
            description: "Build reliable products with Rust and TypeScript.".to_string(),
            compensation: "$170k-$200k".to_string(),
            employment_type: "full_time".to_string(),
            engagement_type: "direct_hire".to_string(),
            posted_at_ms: Some(updated_at_ms - DAY_MS),
        };
        let mut posting = global_candidate_posting(&input);
        posting.canonical_key = canonical_job_key(&posting);
        let source_id = format!("source-{id}");
        let run_id = format!("run-{id}");
        let completed_at_ms = (run_status == "completed").then_some(updated_at_ms);
        let conn = pool.get().unwrap();
        conn.execute(
            "INSERT INTO jobs_global_discovery_sources (
                id, provider, source_key, source_json, status, health,
                run_interval_ms, next_run_at_ms, created_at_ms, updated_at_ms
             ) VALUES (?1, 'jobhive', ?2, '{}', 'active', 'waiting',
                       14400000, ?3, ?3, ?3)",
            params![source_id, format!("fixture-{id}"), updated_at_ms],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO jobs_global_ingestion_runs (
                id, source_id, replay_key, status, expected_rows, received_rows,
                received_batches, artifact_sha256, started_at_ms, completed_at_ms
             ) VALUES (?1, ?2, ?3, ?4, 1, 1, 1, ?5, ?6, ?7)",
            params![
                run_id,
                source_id,
                format!("replay-{id}"),
                run_status,
                "a".repeat(64),
                updated_at_ms,
                completed_at_ms,
            ],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO jobs_global_candidates (
                    id, canonical_key, candidate_json, company, title, location, workplace,
                    canonical_url, role_family, posted_at_ms, availability_status,
                    first_seen_at_ms, last_seen_at_ms, updated_at_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'software_engineering',
                           ?9, 'active', ?10, ?10, ?10)",
            params![
                id,
                posting.canonical_key,
                to_json(&input, "global candidate input").unwrap(),
                input.company,
                input.title,
                input.location,
                input.workplace,
                input.canonical_url,
                input.posted_at_ms,
                updated_at_ms,
            ],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO jobs_global_candidate_memberships (
                source_id, external_id, candidate_id, content_hash, first_seen_at_ms,
                last_seen_at_ms, last_seen_run_id, availability_status
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?5, ?6, 'active')",
            params![
                source_id,
                external_id,
                id,
                format!("content-{id}"),
                updated_at_ms,
                run_id,
            ],
        )
        .unwrap();
        input
    }

    fn prepare_archivable_global_candidate(
        pool: &DbPool,
        id: &str,
    ) -> DiscoveredJobInput {
        let input = insert_global_candidate(
            pool,
            id,
            &format!("external-{id}"),
            &format!("https://jobs.lever.co/acme/{id}"),
            1,
        );
        let content_hash = "a".repeat(64);
        let conn = pool.get().unwrap();
        conn.execute(
            "UPDATE jobs_global_candidates
                SET availability_status = 'expired', content_hash = ?2,
                    updated_at_ms = 1
              WHERE id = ?1",
            params![id, content_hash],
        )
        .unwrap();
        conn.execute(
            "UPDATE jobs_global_candidate_memberships
                SET availability_status = 'expired'
              WHERE candidate_id = ?1",
            params![id],
        )
        .unwrap();
        input
    }

    #[test]
    fn resume_import_facts_must_be_confirmed_before_entering_resume_claims() {
        let pool = test_pool();
        let profile = default_profile("jobs@example.com");
        save_profile(&pool, "acct-jobs", &profile).unwrap();
        let imported = upsert_fact(
            &pool,
            "acct-jobs",
            &CareerFact {
                id: "imported-fact".to_string(),
                category: "employment".to_string(),
                label: "Imported achievement".to_string(),
                value: json!("Increased reliability"),
                source: "resume_import".to_string(),
                verification_status: "needs_confirmation".to_string(),
                confirmed_at_ms: Some(1),
                confirmed_by: Some("forged-caller".to_string()),
                schema_version: 99,
                created_at_ms: 1,
                updated_at_ms: 1,
            },
        )
        .unwrap();
        assert!(upsert_user_fact(
            &pool,
            "acct-jobs",
            Some(&imported.id),
            "employment",
            "Imported achievement",
            json!("Changed value"),
        )
        .is_err());
        let posting = upsert_posting(
            &pool,
            "acct-jobs",
            &test_posting(
                "https://boards.greenhouse.io/acme/jobs/unconfirmed-import",
                now_ms(),
                now_ms(),
            ),
            &profile,
            &JobPreferences::default(),
        )
        .unwrap();
        let (_, resume) =
            prepare_application(&pool, "acct-jobs", &posting.id, "factual", "review_first")
                .unwrap();
        assert!(!resume.claim_ids.contains(&imported.id));
        assert_eq!(
            resume
                .content
                .pointer("/provenance/confirmed_fact_ids")
                .and_then(Value::as_array)
                .map(Vec::len),
            Some(0)
        );
    }

    #[test]
    fn final_submission_transaction_rolls_back_if_the_bound_session_disappears() {
        let pool = test_pool();
        let (application, run_id, browser_profile_id) =
            execution_lease_fixture(&pool, "receipt-rollback");
        let application = update_application(&pool, "acct-jobs", &application.id, "running", None)
            .unwrap()
            .unwrap();
        reserve_application_attempt(&pool, "acct-jobs", &application.id, "cloud").unwrap();
        let lease = claim_execution_lease(
            &pool,
            "acct-jobs",
            &application.id,
            &run_id,
            &browser_profile_id,
            "rollback-worker",
        )
        .unwrap();
        start_irreversible_submission(
            &pool,
            "acct-jobs",
            &application.id,
            &run_id,
            &lease.lease_token,
            lease.fence,
        )
        .unwrap();
        finish_execution_lease(
            &pool,
            "acct-jobs",
            &application.id,
            &run_id,
            &lease.lease_token,
            lease.fence,
            "submitted",
        )
        .unwrap();
        let fingerprint = "a".repeat(64);
        let receipt = json!({
            "receiptId": "receipt-rollback",
            "_bluey_server_submission_fingerprint_v1": fingerprint,
        });
        let evidence = vec![
            ApplicationEvidence {
                id: String::new(),
                application_id: application.id.clone(),
                kind: "resume".to_string(),
                label: "Resume submitted".to_string(),
                provider: "greenhouse".to_string(),
                file_name: "resume.pdf".to_string(),
                media_type: "application/pdf".to_string(),
                storage_key: "request-owned/resume".to_string(),
                sha256: "b".repeat(64),
                resume_version_id: application.resume_version_id.clone(),
                occurred_at_ms: 0,
                metadata: json!({}),
                created_at_ms: 0,
            },
            ApplicationEvidence {
                id: String::new(),
                application_id: application.id.clone(),
                kind: "submission_confirmation".to_string(),
                label: "Application received".to_string(),
                provider: "greenhouse".to_string(),
                file_name: "confirmation.png".to_string(),
                media_type: "image/png".to_string(),
                storage_key: "request-owned/confirmation".to_string(),
                sha256: "c".repeat(64),
                resume_version_id: application.resume_version_id.clone(),
                occurred_at_ms: 0,
                metadata: json!({ "confirmation": "Application received" }),
                created_at_ms: 0,
            },
        ];
        let mut session = list_browser_sessions(&pool, "acct-jobs")
            .unwrap()
            .into_iter()
            .find(|session| session.id == run_id)
            .unwrap();
        session.status = "complete".to_string();
        pool.get()
            .unwrap()
            .execute(
                "DELETE FROM jobs_browser_sessions WHERE account_id = ?1 AND id = ?2",
                params!["acct-jobs", &run_id],
            )
            .unwrap();
        let error = finalize_submission(
            &pool,
            "acct-jobs",
            &application.id,
            &run_id,
            "cloud",
            receipt,
            &fingerprint,
            &evidence,
            &session,
            None,
        )
        .unwrap_err();
        assert!(error.to_string().contains("browser session not found"));
        assert!(
            list_application_evidence(&pool, "acct-jobs", Some(&application.id))
                .unwrap()
                .is_empty()
        );
        let stored = get_application(&pool, "acct-jobs", &application.id)
            .unwrap()
            .unwrap();
        assert_eq!(stored.state, "running");
        assert_ne!(
            stored.receipt.get("receiptId"),
            Some(&json!("receipt-rollback"))
        );
        let reservation = list_attempt_reservations(&pool, "acct-jobs")
            .unwrap()
            .into_iter()
            .find(|reservation| reservation.application_id == application.id)
            .unwrap();
        assert_eq!(reservation.status, "reserved");
        assert!(list_browser_sessions(&pool, "acct-jobs")
            .unwrap()
            .into_iter()
            .all(|session| session.id != run_id));
    }

    #[test]
    fn discovery_snapshots_are_exclusive_replay_safe_and_close_missing_jobs() {
        let pool = test_pool();
        let source = upsert_discovery_source(
            &pool,
            "acct-jobs",
            &DiscoverySourceInput {
                track_id: String::new(),
                provider: "greenhouse".to_string(),
                source_key: "acme".to_string(),
                company: "Acme".to_string(),
                run_interval_ms: DISCOVERY_MIN_INTERVAL_MS,
            },
        )
        .unwrap();
        let lease = lease_due_discovery_source(&pool, "worker-one")
            .unwrap()
            .unwrap();
        assert_eq!(lease.source.id, source.id);
        assert!(lease_due_discovery_source(&pool, "worker-two")
            .unwrap()
            .is_none());

        let jobs = vec![
            discovered_job("100", "Software Engineer"),
            discovered_job("200", "Platform Engineer"),
        ];
        let completed = complete_discovery_run(
            &pool,
            &source.id,
            &lease.lease_token,
            &lease.replay_key,
            lease.scheduled_for_ms,
            &jobs,
            true,
        )
        .unwrap();
        assert_eq!(completed.discovered_count, 2);
        assert_eq!(completed.upserted_count, 2);
        assert_eq!(completed.closed_count, 0);
        assert!(!completed.replayed);

        let replayed = complete_discovery_run(
            &pool,
            &source.id,
            &lease.lease_token,
            &lease.replay_key,
            lease.scheduled_for_ms,
            &jobs,
            true,
        )
        .unwrap();
        assert!(replayed.replayed);
        assert_eq!(replayed.run_id, completed.run_id);

        let postings = list_postings(&pool, "acct-jobs").unwrap();
        assert_eq!(postings.len(), 2);
        assert!(postings.iter().all(|posting| posting.company == "Acme"));
        assert!(postings
            .iter()
            .all(|posting| !posting.canonical_url.contains("utm_source")));
        let conn = pool.get().unwrap();
        let (snapshot_hash, content_hash): (String, String) = conn
            .query_row(
                "SELECT r.snapshot_hash, m.content_hash
                   FROM jobs_discovery_runs r
                   JOIN jobs_discovery_memberships m ON m.source_id = r.source_id
                  WHERE r.id = ?1 LIMIT 1",
                params![completed.run_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(snapshot_hash.len(), 64);
        assert_eq!(content_hash.len(), 64);
        assert_ne!(content_hash, "a".repeat(64));
        conn.execute(
            "UPDATE jobs_discovery_sources SET next_run_at_ms = ?2 WHERE id = ?1",
            params![source.id, now_ms() - 1_000],
        )
        .unwrap();
        drop(conn);

        let next = lease_due_discovery_source(&pool, "worker-two")
            .unwrap()
            .unwrap();
        let closed = complete_discovery_run(
            &pool,
            &source.id,
            &next.lease_token,
            &next.replay_key,
            next.scheduled_for_ms,
            &[],
            true,
        )
        .unwrap();
        assert_eq!(closed.closed_count, 0);
        assert!(list_postings(&pool, "acct-jobs")
            .unwrap()
            .iter()
            .all(|posting| posting.availability_status == "active"));
        let conn = pool.get().unwrap();
        let last_seen_run_id: String = conn
            .query_row(
                "SELECT last_seen_run_id FROM jobs_discovery_memberships
                  WHERE source_id = ?1 AND external_id = '100'",
                params![source.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(last_seen_run_id, completed.run_id);
        conn.execute(
            "UPDATE jobs_discovery_memberships SET missing_since_at_ms = ?1",
            params![now_ms() - 31 * 60 * 1_000],
        )
        .unwrap();
        conn.execute(
            "UPDATE jobs_discovery_sources SET next_run_at_ms = ?2 WHERE id = ?1",
            params![source.id, now_ms() - 2_000],
        )
        .unwrap();
        drop(conn);
        let final_lease = lease_due_discovery_source(&pool, "worker-three")
            .unwrap()
            .unwrap();
        let final_snapshot = complete_discovery_run(
            &pool,
            &source.id,
            &final_lease.lease_token,
            &final_lease.replay_key,
            final_lease.scheduled_for_ms,
            &[],
            true,
        )
        .unwrap();
        assert_eq!(final_snapshot.closed_count, 2);
        assert!(list_postings(&pool, "acct-jobs")
            .unwrap()
            .iter()
            .all(|posting| posting.availability_status == "expired"));
        assert_eq!(
            get_discovery_source(&pool, &source.id)
                .unwrap()
                .unwrap()
                .health,
            "healthy"
        );
    }

    #[test]
    fn managed_curated_discovery_is_one_account_level_source() {
        let pool = test_pool();
        let first = ensure_managed_curated_discovery_source(&pool, "acct-jobs")
            .unwrap()
            .unwrap();
        let second = ensure_managed_curated_discovery_source(&pool, "acct-jobs")
            .unwrap()
            .unwrap();

        assert_eq!(first.id, second.id);
        assert_eq!(first.provider, CURATED_DISCOVERY_PROVIDER);
        assert_eq!(first.source_key, CURATED_DISCOVERY_SOURCE_KEY);
        assert!(first.track_id.is_empty());
        assert_eq!(
            first.config.get("feedIds"),
            Some(&json!(CURATED_DISCOVERY_CATALOG_IDS))
        );
        assert_eq!(
            list_discovery_sources(&pool, "acct-jobs")
                .unwrap()
                .into_iter()
                .filter(|source| source.provider == CURATED_DISCOVERY_PROVIDER)
                .count(),
            1
        );
    }

    #[test]
    fn managed_curated_discovery_requires_and_tracks_the_last_career_track() {
        let pool = test_pool();
        assert!(ensure_managed_curated_discovery_source(&pool, "acct-jobs")
            .unwrap()
            .is_some());

        assert!(delete_track(&pool, "acct-jobs", "track-default").unwrap());
        assert!(list_tracks(&pool, "acct-jobs").unwrap().is_empty());
        assert!(list_discovery_sources(&pool, "acct-jobs")
            .unwrap()
            .is_empty());
        assert!(ensure_managed_curated_discovery_source(&pool, "acct-jobs")
            .unwrap()
            .is_none());
    }

    #[test]
    fn global_candidates_materialize_once_as_track_scoped_review_leads() {
        let pool = test_pool();
        let mut profile = default_profile("jobs@example.com");
        profile.skills = vec!["Rust".to_string(), "TypeScript".to_string()];
        save_profile(&pool, "acct-jobs", &profile).unwrap();
        save_preferences(
            &pool,
            "acct-jobs",
            &JobPreferences {
                sponsorship: "not_required".to_string(),
                ..JobPreferences::default()
            },
        )
        .unwrap();
        insert_global_candidate(
            &pool,
            "candidate-global-1",
            "lead-1",
            "https://jobs.lever.co/acme/software-engineer",
            now_ms(),
        );

        let first =
            materialize_global_candidates_for_account(&pool, "acct-jobs", "jobs@example.com")
                .unwrap();
        assert_eq!(first.materialized_count, 1);
        assert_eq!(first.refreshed_count, 0);

        let postings = list_postings(&pool, "acct-jobs").unwrap();
        assert_eq!(postings.len(), 1);
        assert_eq!(postings[0].track_id, "track-default");
        assert_eq!(postings[0].source, "curated_feed:jobhive:lever");
        assert_eq!(postings[0].availability_status, "unknown");
        assert!(postings[0].last_verified_at_ms.is_none());
        let decision =
            evaluate_job_eligibility(&pool, "acct-jobs", &postings[0], true, None).unwrap();
        assert!(!decision.can_prepare);
        assert!(!decision.can_queue_local);
        assert!(!decision.can_queue_cloud);

        let second =
            materialize_global_candidates_for_account(&pool, "acct-jobs", "jobs@example.com")
                .unwrap();
        assert_eq!(second, empty_global_materialization_result());
        assert_eq!(list_postings(&pool, "acct-jobs").unwrap().len(), 1);
    }

    #[test]
    fn global_candidates_wait_for_their_ingestion_run_to_complete() {
        let pool = test_pool();
        save_profile(&pool, "acct-jobs", &default_profile("jobs@example.com")).unwrap();
        insert_global_candidate_with_run_status(
            &pool,
            "candidate-global-running",
            "lead-running",
            "https://jobs.lever.co/acme/running-ingestion",
            now_ms(),
            "running",
        );

        let hidden =
            materialize_global_candidates_for_account(&pool, "acct-jobs", "jobs@example.com")
                .unwrap();
        assert_eq!(hidden, empty_global_materialization_result());
        assert!(list_postings(&pool, "acct-jobs").unwrap().is_empty());

        let completed_at_ms = now_ms();
        pool.get()
            .unwrap()
            .execute(
                "UPDATE jobs_global_ingestion_runs
                    SET status = 'completed', completed_at_ms = ?2
                  WHERE id = ?1",
                params!["run-candidate-global-running", completed_at_ms],
            )
            .unwrap();

        let visible =
            materialize_global_candidates_for_account(&pool, "acct-jobs", "jobs@example.com")
                .unwrap();
        assert_eq!(visible.materialized_count, 1);
        assert_eq!(list_postings(&pool, "acct-jobs").unwrap().len(), 1);
    }

    #[test]
    fn unchanged_global_manifest_preserves_the_completed_run_schedule() {
        let pool = test_pool();
        let snapshot_at_ms = now_ms() - 60_000;
        let input = GlobalDiscoverySourceInput {
            provider: "jobhive".to_string(),
            source_key: "lever-schedule".to_string(),
            source_family: "lever".to_string(),
            artifact_url: "https://storage.stapply.ai/lever.csv".to_string(),
            artifact_sha256: "a".repeat(64),
            expected_rows: 10,
            snapshot_at_ms,
            run_interval_ms: DISCOVERY_MIN_INTERVAL_MS,
        };
        let source = sync_global_discovery_sources(&pool, std::slice::from_ref(&input))
            .unwrap()
            .remove(0);
        let scheduled_at_ms = now_ms() + DAY_MS;
        let stable_updated_at_ms = now_ms() - 30_000;
        pool.get()
            .unwrap()
            .execute(
                "UPDATE jobs_global_discovery_sources
                    SET next_run_at_ms = ?2, updated_at_ms = ?3
                  WHERE id = ?1",
                params![source.id, scheduled_at_ms, stable_updated_at_ms],
            )
            .unwrap();

        let unchanged = sync_global_discovery_sources(&pool, std::slice::from_ref(&input))
            .unwrap()
            .remove(0);
        assert_eq!(unchanged.next_run_at_ms, scheduled_at_ms);
        assert_eq!(unchanged.updated_at_ms, stable_updated_at_ms);

        let mut metadata_only = input.clone();
        metadata_only.artifact_url = "https://storage.stapply.ai/lever-current.csv".to_string();
        metadata_only.snapshot_at_ms += 1_000;
        let refreshed = sync_global_discovery_sources(&pool, &[metadata_only.clone()])
            .unwrap()
            .remove(0);
        assert_eq!(refreshed.next_run_at_ms, scheduled_at_ms);
        assert_eq!(
            refreshed.config["artifactUrl"],
            json!("https://storage.stapply.ai/lever-current.csv")
        );

        let changed_at_ms = now_ms();
        metadata_only.artifact_sha256 = "b".repeat(64);
        let changed = sync_global_discovery_sources(&pool, &[metadata_only])
            .unwrap()
            .remove(0);
        assert!(changed.next_run_at_ms >= changed_at_ms);
        assert!(changed.next_run_at_ms <= now_ms());
    }

    #[test]
    fn unchanged_global_candidate_does_not_rewrite_the_canonical_payload() {
        let pool = test_pool();
        let source = sync_global_discovery_sources(
            &pool,
            &[GlobalDiscoverySourceInput {
                provider: "jobhive".to_string(),
                source_key: "lever-noop-candidate".to_string(),
                source_family: "lever".to_string(),
                artifact_url: "https://storage.stapply.ai/lever.csv".to_string(),
                artifact_sha256: "a".repeat(64),
                expected_rows: 1,
                snapshot_at_ms: now_ms(),
                run_interval_ms: DISCOVERY_MIN_INTERVAL_MS,
            }],
        )
        .unwrap()
        .remove(0);
        let input = curated_discovered_job(
            "lever-noop-candidate",
            "https://jobs.lever.co/acme/lever-noop-candidate",
        );
        let first = normalize_global_candidate("jobhive", "lever", &input).unwrap();
        let second = normalize_global_candidate("jobhive", "lever", &input).unwrap();
        assert_eq!(first.content_hash, second.content_hash);
        assert_ne!(first.candidate_json, second.candidate_json);

        let first_seen_at_ms = now_ms() - 2_000;
        let second_seen_at_ms = first_seen_at_ms + 1_000;
        let mut conn = pool.get().unwrap();
        for (run_id, replay_key, started_at_ms) in [
            ("run-first", "replay-first", first_seen_at_ms),
            ("run-second", "replay-second", second_seen_at_ms),
        ] {
            conn.execute(
                "INSERT INTO jobs_global_ingestion_runs (
                    id, source_id, replay_key, status, expected_rows, received_rows,
                    received_batches, artifact_sha256, started_at_ms
                 ) VALUES (?1, ?2, ?3, 'running', 1, 0, 0, ?4, ?5)",
                params![
                    run_id,
                    source.id,
                    replay_key,
                    "a".repeat(64),
                    started_at_ms,
                ],
            )
            .unwrap();
        }
        let tx = conn.transaction().unwrap();
        upsert_global_candidate_sqlite(
            &tx,
            &source.id,
            "run-first",
            &first,
            first_seen_at_ms,
        )
        .unwrap();
        tx.commit().unwrap();
        let (stored_payload, stored_updated_at_ms): (String, i64) = conn
            .query_row(
                "SELECT candidate_json, updated_at_ms
                   FROM jobs_global_candidates
                  WHERE canonical_key = ?1",
                params![first.canonical_key],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();

        let tx = conn.transaction().unwrap();
        upsert_global_candidate_sqlite(
            &tx,
            &source.id,
            "run-second",
            &second,
            second_seen_at_ms,
        )
        .unwrap();
        tx.commit().unwrap();
        let (payload_after_second_run, updated_at_after_second_run): (String, i64) = conn
            .query_row(
                "SELECT candidate_json, updated_at_ms
                   FROM jobs_global_candidates
                  WHERE canonical_key = ?1",
                params![first.canonical_key],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        let (membership_seen_at_ms, membership_run_id): (i64, String) = conn
            .query_row(
                "SELECT last_seen_at_ms, last_seen_run_id
                   FROM jobs_global_candidate_memberships
                  WHERE source_id = ?1 AND external_id = ?2",
                params![source.id, first.external_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();

        assert_eq!(stored_payload, payload_after_second_run);
        assert_eq!(stored_updated_at_ms, first_seen_at_ms);
        assert_eq!(updated_at_after_second_run, first_seen_at_ms);
        assert_eq!(membership_seen_at_ms, second_seen_at_ms);
        assert_eq!(membership_run_id, "run-second");
    }

    #[test]
    fn rediscovered_archived_global_candidate_restores_the_hot_payload() {
        let pool = test_pool();
        let source = sync_global_discovery_sources(
            &pool,
            &[GlobalDiscoverySourceInput {
                provider: "jobhive".to_string(),
                source_key: "lever-rediscovered-candidate".to_string(),
                source_family: "lever".to_string(),
                artifact_url: "https://storage.stapply.ai/lever.csv".to_string(),
                artifact_sha256: "a".repeat(64),
                expected_rows: 1,
                snapshot_at_ms: now_ms(),
                run_interval_ms: DISCOVERY_MIN_INTERVAL_MS,
            }],
        )
        .unwrap()
        .remove(0);
        let input = curated_discovered_job(
            "lever-rediscovered-candidate",
            "https://jobs.lever.co/acme/lever-rediscovered-candidate",
        );
        let first = normalize_global_candidate("jobhive", "lever", &input).unwrap();
        let rediscovered = normalize_global_candidate("jobhive", "lever", &input).unwrap();
        let mut conn = pool.get().unwrap();
        for (run_id, replay_key, started_at_ms) in [
            ("run-before-archive", "replay-before-archive", 10_000),
            ("run-after-archive", "replay-after-archive", 20_000),
        ] {
            conn.execute(
                "INSERT INTO jobs_global_ingestion_runs (
                    id, source_id, replay_key, status, expected_rows, received_rows,
                    received_batches, artifact_sha256, started_at_ms
                 ) VALUES (?1, ?2, ?3, 'running', 1, 0, 0, ?4, ?5)",
                params![
                    run_id,
                    source.id,
                    replay_key,
                    "a".repeat(64),
                    started_at_ms,
                ],
            )
            .unwrap();
        }
        let tx = conn.transaction().unwrap();
        upsert_global_candidate_sqlite(&tx, &source.id, "run-before-archive", &first, 10_000)
            .unwrap();
        tx.commit().unwrap();
        conn.execute(
            "UPDATE jobs_global_candidates
                SET candidate_json = '{}', availability_status = 'expired',
                    archive_state = 'archived', archive_storage_key = 'archive.json',
                    archive_sha256 = ?2, archive_size_bytes = 1024, archived_at_ms = 15_000
              WHERE canonical_key = ?1",
            params![first.canonical_key, "b".repeat(64)],
        )
        .unwrap();
        conn.execute(
            "UPDATE jobs_global_candidate_memberships
                SET availability_status = 'expired'
              WHERE source_id = ?1 AND external_id = ?2",
            params![source.id, first.external_id],
        )
        .unwrap();

        let tx = conn.transaction().unwrap();
        upsert_global_candidate_sqlite(
            &tx,
            &source.id,
            "run-after-archive",
            &rediscovered,
            20_000,
        )
        .unwrap();
        tx.commit().unwrap();

        let restored: (String, String, String, Option<String>, Option<i64>) = conn
            .query_row(
                "SELECT candidate_json, availability_status, archive_state,
                        archive_storage_key, archived_at_ms
                   FROM jobs_global_candidates
                  WHERE canonical_key = ?1",
                params![first.canonical_key],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                    ))
                },
            )
            .unwrap();
        let membership_status: String = conn
            .query_row(
                "SELECT availability_status
                   FROM jobs_global_candidate_memberships
                  WHERE source_id = ?1 AND external_id = ?2",
                params![source.id, first.external_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(restored.0, rediscovered.candidate_json);
        assert_eq!(restored.1, "unknown");
        assert_eq!(restored.2, "hot");
        assert!(restored.3.is_none());
        assert!(restored.4.is_none());
        assert_eq!(membership_status, "active");
    }

    #[test]
    fn global_ingestion_recovers_after_completion_response_is_lost() {
        let pool = test_pool();
        let artifact_sha256 = "a".repeat(64);
        let sources = sync_global_discovery_sources(
            &pool,
            &[GlobalDiscoverySourceInput {
                provider: "jobhive".to_string(),
                source_key: "lever-recovery".to_string(),
                source_family: "lever".to_string(),
                artifact_url: "https://storage.stapply.ai/lever.csv".to_string(),
                artifact_sha256: artifact_sha256.clone(),
                expected_rows: 1,
                snapshot_at_ms: now_ms(),
                run_interval_ms: DISCOVERY_MIN_INTERVAL_MS,
            }],
        )
        .unwrap();
        let source_id = sources[0].id.clone();
        let first = lease_due_global_discovery_source(&pool, "first-worker")
            .unwrap()
            .unwrap();
        let batch = GlobalIngestionBatchInput {
            lease_token: first.lease_token.clone(),
            replay_key: first.replay_key.clone(),
            scheduled_for_ms: first.scheduled_for_ms,
            batch_index: 0,
            artifact_sha256: artifact_sha256.clone(),
            jobs: vec![curated_discovered_job(
                "lever-recovery-job",
                "https://jobs.lever.co/acme/lever-recovery-job",
            )],
        };
        let uploaded = ingest_global_discovery_batch(&pool, &source_id, &batch).unwrap();
        assert!(!uploaded.replayed);

        pool.get()
            .unwrap()
            .execute(
                "UPDATE jobs_global_discovery_sources
                    SET lease_expires_at_ms = ?2 WHERE id = ?1",
                params![source_id, now_ms() - 1],
            )
            .unwrap();
        let second = lease_due_global_discovery_source(&pool, "recovery-worker")
            .unwrap()
            .unwrap();
        assert_eq!(second.replay_key, first.replay_key);
        assert_eq!(second.scheduled_for_ms, first.scheduled_for_ms);

        let replayed = ingest_global_discovery_batch(
            &pool,
            &source_id,
            &GlobalIngestionBatchInput {
                lease_token: second.lease_token.clone(),
                ..batch
            },
        )
        .unwrap();
        assert!(replayed.replayed);
        assert_eq!(replayed.received_rows, 1);
        assert_eq!(replayed.received_batches, 1);

        let completed = complete_global_discovery_ingestion(
            &pool,
            &source_id,
            &GlobalIngestionCompleteInput {
                lease_token: second.lease_token.clone(),
                replay_key: second.replay_key.clone(),
                scheduled_for_ms: second.scheduled_for_ms,
                artifact_sha256: artifact_sha256.clone(),
                expected_rows: 1,
                accepted_rows: None,
                rejected_rows: 0,
                rejection_reasons: BTreeMap::new(),
                expected_batches: 1,
                complete_snapshot: true,
            },
        )
        .unwrap();
        assert_eq!(completed.status, "completed");
        assert!(!completed.replayed);

        let completion_replay = complete_global_discovery_ingestion(
            &pool,
            &source_id,
            &GlobalIngestionCompleteInput {
                lease_token: second.lease_token,
                replay_key: second.replay_key,
                scheduled_for_ms: second.scheduled_for_ms,
                artifact_sha256,
                expected_rows: 1,
                accepted_rows: None,
                rejected_rows: 0,
                rejection_reasons: BTreeMap::new(),
                expected_batches: 1,
                complete_snapshot: true,
            },
        )
        .unwrap();
        assert!(completion_replay.replayed);

        let conn = pool.get().unwrap();
        let candidates: i64 = conn
            .query_row("SELECT COUNT(*) FROM jobs_global_candidates", [], |row| {
                row.get(0)
            })
            .unwrap();
        let memberships: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM jobs_global_candidate_memberships",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(candidates, 1);
        assert_eq!(memberships, 1);
    }

    #[test]
    fn global_ingestion_rejection_evidence_is_bounded_and_typed() {
        let legacy = normalized_global_rejection_evidence(&global_completion_evidence(
            1,
            None,
            0,
            BTreeMap::new(),
        ))
        .unwrap();
        assert_eq!(legacy.accepted_rows, 1);
        assert_eq!(legacy.rejected_rows, 0);

        let accepted = normalized_global_rejection_evidence(&global_completion_evidence(
            1_000,
            Some(999),
            1,
            BTreeMap::from([("missing_identity".to_string(), 1)]),
        ))
        .unwrap();
        assert_eq!(accepted.accepted_rows, 999);
        assert_eq!(accepted.rejected_rows, 1);

        let unknown_reason = normalized_global_rejection_evidence(&global_completion_evidence(
            1_000,
            Some(999),
            1,
            BTreeMap::from([("parse_error".to_string(), 1)]),
        ))
        .unwrap_err();
        assert!(unknown_reason
            .to_string()
            .contains("rejection reason is invalid"));

        let excessive = normalized_global_rejection_evidence(&global_completion_evidence(
            1_000,
            Some(998),
            2,
            BTreeMap::from([("missing_identity".to_string(), 2)]),
        ))
        .unwrap_err();
        assert!(excessive.to_string().contains("rejected too many rows"));

        let mismatched = normalized_global_rejection_evidence(&global_completion_evidence(
            1_000,
            Some(999),
            1,
            BTreeMap::from([("missing_identity".to_string(), 2)]),
        ))
        .unwrap_err();
        assert!(mismatched
            .to_string()
            .contains("rejection reasons do not reconcile"));
    }

    #[test]
    fn global_ingestion_commits_one_quarantine_with_replay_safe_evidence() {
        let pool = test_pool();
        let artifact_sha256 = "b".repeat(64);
        let sources = sync_global_discovery_sources(
            &pool,
            &[GlobalDiscoverySourceInput {
                provider: "jobhive".to_string(),
                source_key: "ashby-quarantine".to_string(),
                source_family: "ashby".to_string(),
                artifact_url: "https://storage.stapply.ai/ashby.csv".to_string(),
                artifact_sha256: artifact_sha256.clone(),
                expected_rows: 2,
                snapshot_at_ms: now_ms(),
                run_interval_ms: DISCOVERY_MIN_INTERVAL_MS,
            }],
        )
        .unwrap();
        let source_id = sources[0].id.clone();
        let lease = lease_due_global_discovery_source(&pool, "quarantine-worker")
            .unwrap()
            .unwrap();
        ingest_global_discovery_batch(
            &pool,
            &source_id,
            &GlobalIngestionBatchInput {
                lease_token: lease.lease_token.clone(),
                replay_key: lease.replay_key.clone(),
                scheduled_for_ms: lease.scheduled_for_ms,
                batch_index: 0,
                artifact_sha256: artifact_sha256.clone(),
                jobs: vec![curated_discovered_job(
                    "ashby-valid-job",
                    "https://jobs.ashbyhq.com/acme/ashby-valid-job",
                )],
            },
        )
        .unwrap();
        let completion = GlobalIngestionCompleteInput {
            lease_token: lease.lease_token.clone(),
            replay_key: lease.replay_key.clone(),
            scheduled_for_ms: lease.scheduled_for_ms,
            artifact_sha256: artifact_sha256.clone(),
            expected_rows: 2,
            accepted_rows: Some(1),
            rejected_rows: 1,
            rejection_reasons: BTreeMap::from([("missing_identity".to_string(), 1)]),
            expected_batches: 1,
            complete_snapshot: true,
        };

        let committed =
            complete_global_discovery_ingestion(&pool, &source_id, &completion).unwrap();
        assert_eq!(committed.status, "completed");
        assert_eq!(committed.received_rows, 1);
        assert_eq!(committed.rejected_rows, 1);
        assert_eq!(
            committed.rejection_reasons,
            BTreeMap::from([("missing_identity".to_string(), 1)])
        );
        assert!(!committed.replayed);

        let replayed =
            complete_global_discovery_ingestion(&pool, &source_id, &completion).unwrap();
        assert!(replayed.replayed);
        assert_eq!(replayed.rejected_rows, 1);

        let conflicting = complete_global_discovery_ingestion(
            &pool,
            &source_id,
            &GlobalIngestionCompleteInput {
                accepted_rows: Some(2),
                rejected_rows: 0,
                rejection_reasons: BTreeMap::new(),
                ..completion
            },
        )
        .unwrap_err();
        assert!(conflicting
            .to_string()
            .contains("completion replay conflicts with stored evidence"));

        let conn = pool.get().unwrap();
        let stored: (i64, String) = conn
            .query_row(
                "SELECT rejected_rows, rejection_summary_json
                   FROM jobs_global_ingestion_runs WHERE id = ?1",
                params![committed.run_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(stored.0, 1);
        assert_eq!(
            serde_json::from_str::<BTreeMap<String, i64>>(&stored.1).unwrap(),
            BTreeMap::from([("missing_identity".to_string(), 1)])
        );
        let source_health: String = conn
            .query_row(
                "SELECT health FROM jobs_global_discovery_sources WHERE id = ?1",
                params![source_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(source_health, "healthy");
    }

    #[test]
    fn global_candidates_never_replace_direct_employer_records() {
        let pool = test_pool();
        let profile = default_profile("jobs@example.com");
        let preferences = JobPreferences::default();
        save_profile(&pool, "acct-jobs", &profile).unwrap();
        let url = "https://jobs.lever.co/acme/software-engineer";
        let direct = upsert_posting(
            &pool,
            "acct-jobs",
            &test_posting(url, now_ms() - DAY_MS, now_ms()),
            &profile,
            &preferences,
        )
        .unwrap();
        insert_global_candidate(
            &pool,
            "candidate-global-direct",
            "lead-direct",
            url,
            now_ms(),
        );

        let result =
            materialize_global_candidates_for_account(&pool, "acct-jobs", "jobs@example.com")
                .unwrap();
        assert_eq!(result.materialized_count, 0);
        assert_eq!(result.skipped_count, 1);
        let postings = list_postings(&pool, "acct-jobs").unwrap();
        assert_eq!(postings.len(), 1);
        assert_eq!(postings[0].id, direct.id);
        assert_eq!(postings[0].source, "greenhouse");
        assert_eq!(postings[0].availability_status, "active");
    }

    #[test]
    fn expired_global_candidates_leave_the_account_review_queue() {
        let pool = test_pool();
        save_profile(&pool, "acct-jobs", &default_profile("jobs@example.com")).unwrap();
        insert_global_candidate(
            &pool,
            "candidate-global-expired",
            "lead-expired",
            "https://jobs.lever.co/acme/software-engineer",
            now_ms(),
        );
        materialize_global_candidates_for_account(&pool, "acct-jobs", "jobs@example.com").unwrap();

        pool.get()
            .unwrap()
            .execute(
                "UPDATE jobs_global_candidates
                    SET availability_status = 'expired', updated_at_ms = ?2
                  WHERE id = ?1",
                params!["candidate-global-expired", now_ms() + 1_000],
            )
            .unwrap();
        materialize_global_candidates_for_account(&pool, "acct-jobs", "jobs@example.com").unwrap();

        let postings = list_postings(&pool, "acct-jobs").unwrap();
        assert_eq!(postings.len(), 1);
        assert_eq!(postings[0].availability_status, "expired");
        let materialization_count: i64 = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM jobs_global_candidate_materializations
                  WHERE account_id = ?1",
                params!["acct-jobs"],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(materialization_count, 0);
    }

    #[test]
    fn verified_archive_completion_replaces_only_heavy_candidate_fields() {
        let pool = test_pool();
        let input = prepare_archivable_global_candidate(&pool, "candidate-archive-complete");
        let leases = claim_global_candidate_archive_jobs(
            &pool,
            "archive-worker",
            100_000,
            10,
            60_000,
            10,
        )
        .unwrap();
        assert_eq!(leases.len(), 1);

        assert!(complete_global_candidate_archive(
            &pool,
            &leases[0],
            "bluey-cloud/global/jobs/candidates/candidate-archive-complete/archive.json",
            &"b".repeat(64),
            1_024,
            100_001,
        )
        .unwrap());

        let conn = pool.get().unwrap();
        let archived: (String, String, String, i64, Option<i64>, String) = conn
            .query_row(
                "SELECT archive_state, archive_storage_key, archive_sha256,
                        archive_size_bytes, archived_at_ms, candidate_json
                   FROM jobs_global_candidates
                  WHERE id = ?1",
                params!["candidate-archive-complete"],
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
            .unwrap();
        assert_eq!(archived.0, "archived");
        assert!(archived.1.ends_with("/archive.json"));
        assert_eq!(archived.2, "b".repeat(64));
        assert_eq!(archived.3, 1_024);
        assert_eq!(archived.4, Some(100_001));
        let tombstone: DiscoveredJobInput =
            parse_json(archived.5, "archived global candidate tombstone").unwrap();
        assert_eq!(tombstone.title, input.title);
        assert_eq!(tombstone.company, input.company);
        assert_eq!(tombstone.canonical_url, input.canonical_url);
        assert!(tombstone.description.is_empty());
        assert!(tombstone.compensation.is_empty());
    }

    #[test]
    fn legacy_expired_candidate_hashes_before_archive() {
        let pool = test_pool();
        let candidate_id = "candidate-archive-legacy";
        insert_global_candidate(
            &pool,
            candidate_id,
            "external-archive-legacy",
            "https://jobs.lever.co/acme/candidate-archive-legacy",
            1,
        );
        let conn = pool.get().unwrap();
        conn.execute(
            "UPDATE jobs_global_candidates
                SET availability_status = 'expired', content_hash = '', updated_at_ms = 1
              WHERE id = ?1",
            params![candidate_id],
        )
        .unwrap();
        conn.execute(
            "UPDATE jobs_global_candidate_memberships
                SET availability_status = 'expired'
              WHERE candidate_id = ?1",
            params![candidate_id],
        )
        .unwrap();
        let candidate_json: String = conn
            .query_row(
                "SELECT candidate_json FROM jobs_global_candidates WHERE id = ?1",
                params![candidate_id],
                |row| row.get(0),
            )
            .unwrap();
        drop(conn);

        let lease = claim_global_candidate_archive_jobs(
            &pool,
            "archive-worker",
            100_000,
            10,
            60_000,
            1,
        )
        .unwrap()
        .remove(0);

        assert_eq!(lease.candidate_id, candidate_id);
        assert_eq!(
            lease.content_hash,
            global_candidate_content_hash(&candidate_json).unwrap()
        );
        let stored_hash: String = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT content_hash FROM jobs_global_candidates WHERE id = ?1",
                params![candidate_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(stored_hash, lease.content_hash);
    }

    #[test]
    fn corrupt_legacy_candidate_retries_without_blocking_valid_candidate() {
        let pool = test_pool();
        prepare_archivable_global_candidate(&pool, "candidate-archive-valid");
        let candidate_id = "candidate-archive-corrupt";
        insert_global_candidate(
            &pool,
            candidate_id,
            "external-archive-corrupt",
            "https://jobs.lever.co/acme/candidate-archive-corrupt",
            1,
        );
        let corrupt_payload = "not-json";
        let conn = pool.get().unwrap();
        conn.execute(
            "UPDATE jobs_global_candidates
                SET availability_status = 'expired', content_hash = '',
                    candidate_json = ?2, updated_at_ms = 1
              WHERE id = ?1",
            params![candidate_id, corrupt_payload],
        )
        .unwrap();
        conn.execute(
            "UPDATE jobs_global_candidate_memberships
                SET availability_status = 'expired'
              WHERE candidate_id = ?1",
            params![candidate_id],
        )
        .unwrap();
        drop(conn);

        let leases = claim_global_candidate_archive_jobs(
            &pool,
            "archive-worker",
            100_000,
            10,
            60_000,
            10,
        )
        .unwrap();

        assert_eq!(leases.len(), 1);
        assert_eq!(leases[0].candidate_id, "candidate-archive-valid");
        let retry: (String, i64, i64, String) = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT archive_state, archive_attempt_count,
                        archive_next_attempt_at_ms, candidate_json
                   FROM jobs_global_candidates
                  WHERE id = ?1",
                params![candidate_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap();
        assert_eq!(retry.0, "retry");
        assert_eq!(retry.1, 1);
        assert!(retry.2 > 100_000);
        assert_eq!(retry.3, corrupt_payload);
    }

    #[test]
    fn active_membership_blocks_global_candidate_archive() {
        let pool = test_pool();
        insert_global_candidate(
            &pool,
            "candidate-archive-active",
            "external-active",
            "https://jobs.lever.co/acme/candidate-archive-active",
            1,
        );
        pool.get()
            .unwrap()
            .execute(
                "UPDATE jobs_global_candidates
                    SET availability_status = 'expired', content_hash = ?2,
                        updated_at_ms = 1
                  WHERE id = ?1",
                params!["candidate-archive-active", "a".repeat(64)],
            )
            .unwrap();

        let leases = claim_global_candidate_archive_jobs(
            &pool,
            "archive-worker",
            100_000,
            10,
            60_000,
            10,
        )
        .unwrap();
        assert!(leases.is_empty());
    }

    #[test]
    fn account_materialization_blocks_global_candidate_archive() {
        let pool = test_pool();
        save_profile(&pool, "acct-jobs", &default_profile("jobs@example.com")).unwrap();
        insert_global_candidate(
            &pool,
            "candidate-archive-materialized",
            "external-materialized",
            "https://jobs.lever.co/acme/candidate-archive-materialized",
            now_ms(),
        );
        let result =
            materialize_global_candidates_for_account(&pool, "acct-jobs", "jobs@example.com")
                .unwrap();
        assert_eq!(result.materialized_count, 1);
        let conn = pool.get().unwrap();
        conn.execute(
            "UPDATE jobs_global_candidates
                SET availability_status = 'expired', content_hash = ?2,
                    updated_at_ms = 1
              WHERE id = ?1",
            params!["candidate-archive-materialized", "a".repeat(64)],
        )
        .unwrap();
        conn.execute(
            "UPDATE jobs_global_candidate_memberships
                SET availability_status = 'expired'
              WHERE candidate_id = ?1",
            params!["candidate-archive-materialized"],
        )
        .unwrap();

        let leases = claim_global_candidate_archive_jobs(
            &pool,
            "archive-worker",
            100_000,
            10,
            60_000,
            10,
        )
        .unwrap();
        assert!(leases.is_empty());
    }

    #[test]
    fn failed_global_candidate_archive_keeps_payload_and_schedules_retry() {
        let pool = test_pool();
        prepare_archivable_global_candidate(&pool, "candidate-archive-retry");
        let original_payload: String = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT candidate_json FROM jobs_global_candidates WHERE id = ?1",
                params!["candidate-archive-retry"],
                |row| row.get(0),
            )
            .unwrap();
        let lease = claim_global_candidate_archive_jobs(
            &pool,
            "archive-worker",
            100_000,
            10,
            60_000,
            1,
        )
        .unwrap()
        .remove(0);

        assert!(fail_global_candidate_archive(&pool, &lease, 100_001).unwrap());

        let retry: (String, i64, i64, String, Option<String>) = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT archive_state, archive_attempt_count,
                        archive_next_attempt_at_ms, candidate_json, archive_lease_owner
                   FROM jobs_global_candidates
                  WHERE id = ?1",
                params!["candidate-archive-retry"],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(retry.0, "retry");
        assert_eq!(retry.1, 1);
        assert!(retry.2 > 100_001);
        assert_eq!(retry.3, original_payload);
        assert!(retry.4.is_none());
    }

    #[test]
    fn curated_discovery_persists_verify_first_leads_on_the_best_track() {
        let pool = test_pool();
        let source = ensure_managed_curated_discovery_source(&pool, "acct-jobs")
            .unwrap()
            .unwrap();
        let lease = lease_due_discovery_source(&pool, "curated-worker")
            .unwrap()
            .unwrap();
        assert_eq!(lease.source.id, source.id);

        let result = complete_discovery_run(
            &pool,
            &source.id,
            &lease.lease_token,
            &lease.replay_key,
            lease.scheduled_for_ms,
            &[curated_discovered_job(
                "lead-1",
                "https://jobs.lever.co/acme/software-engineer?utm_source=feed",
            )],
            true,
        )
        .unwrap();
        assert_eq!(result.discovered_count, 1);
        assert_eq!(result.upserted_count, 1);

        let postings = list_postings(&pool, "acct-jobs").unwrap();
        assert_eq!(postings.len(), 1);
        let posting = &postings[0];
        assert_eq!(posting.company, "Acme");
        assert_eq!(posting.track_id, "track-default");
        assert_eq!(posting.source, "curated_feed:feed-simplify-new-grad");
        assert_eq!(posting.availability_status, "unknown");
        assert!(posting.last_verified_at_ms.is_none());
        assert!(!posting.canonical_url.contains("utm_source"));

        let decision = evaluate_job_eligibility(&pool, "acct-jobs", posting, true, None).unwrap();
        assert!(!decision.can_prepare);
        assert!(!decision.can_queue_local);
        assert!(!decision.can_queue_cloud);
        assert!(decision
            .review_reasons
            .iter()
            .any(|reason| reason.code == "availability_unverified"));
        assert!(decision
            .review_reasons
            .iter()
            .any(|reason| reason.code == "live_verification_required"));
    }

    #[test]
    fn verified_employer_import_upgrades_matching_feed_url_without_duplication() {
        let pool = test_pool();
        let profile = default_profile("jobs@example.com");
        let preferences = JobPreferences::default();
        let direct_url = "https://jobs.lever.co/acme/software-engineer";
        let source = ensure_managed_curated_discovery_source(&pool, "acct-jobs")
            .unwrap()
            .unwrap();
        let lease = lease_due_discovery_source(&pool, "curated-worker")
            .unwrap()
            .unwrap();
        complete_discovery_run(
            &pool,
            &source.id,
            &lease.lease_token,
            &lease.replay_key,
            lease.scheduled_for_ms,
            &[curated_discovered_job("feed-lead", direct_url)],
            true,
        )
        .unwrap();
        let feed_lead = list_postings(&pool, "acct-jobs").unwrap().remove(0);

        let mut employer_posting = test_posting(direct_url, now_ms() - DAY_MS, now_ms());
        employer_posting.source = "lever_import".to_string();
        employer_posting.external_id = "software-engineer".to_string();
        employer_posting.company = "Acme Incorporated".to_string();
        employer_posting.title = "Platform Software Engineer".to_string();
        employer_posting.location = "United States".to_string();
        employer_posting.workplace = "remote".to_string();
        let verified = save_verified_import_posting_with_source(
            &pool,
            "acct-jobs",
            &employer_posting,
            &DiscoverySourceInput {
                track_id: "track-default".to_string(),
                provider: "lever".to_string(),
                source_key: "acme".to_string(),
                company: "Acme Incorporated".to_string(),
                run_interval_ms: DISCOVERY_MIN_INTERVAL_MS,
            },
            &profile,
            &preferences,
        )
        .unwrap();

        assert_eq!(verified.id, feed_lead.id);
        assert_eq!(verified.source, "lever_import");
        assert_eq!(verified.company, "Acme Incorporated");
        assert_eq!(verified.title, "Platform Software Engineer");
        assert_eq!(verified.availability_status, "active");
        assert!(verified.last_verified_at_ms.is_some());
        let postings = list_postings(&pool, "acct-jobs").unwrap();
        assert_eq!(postings.len(), 1);
        assert_eq!(postings[0].id, feed_lead.id);
        let membership_job_ids = pool
            .get()
            .unwrap()
            .prepare(
                "SELECT DISTINCT job_id FROM jobs_discovery_memberships
                  WHERE account_id = ?1 ORDER BY job_id",
            )
            .unwrap()
            .query_map(params!["acct-jobs"], |row| row.get::<_, String>(0))
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        assert_eq!(membership_job_ids, vec![feed_lead.id]);
    }

    #[test]
    fn curated_discovery_never_downgrades_a_verified_employer_record() {
        let pool = test_pool();
        let profile = default_profile("jobs@example.com");
        let preferences = JobPreferences::default();
        let direct_url = "https://jobs.lever.co/acme/software-engineer";
        let mut direct = test_posting(direct_url, now_ms() - DAY_MS, now_ms());
        direct.source = "lever".to_string();
        let direct = upsert_posting(&pool, "acct-jobs", &direct, &profile, &preferences).unwrap();
        assert_eq!(direct.availability_status, "active");
        assert!(direct.last_verified_at_ms.is_some());

        let source = ensure_managed_curated_discovery_source(&pool, "acct-jobs")
            .unwrap()
            .unwrap();
        let lease = lease_due_discovery_source(&pool, "curated-worker")
            .unwrap()
            .unwrap();
        complete_discovery_run(
            &pool,
            &source.id,
            &lease.lease_token,
            &lease.replay_key,
            lease.scheduled_for_ms,
            &[curated_discovered_job("lead-verified", direct_url)],
            true,
        )
        .unwrap();

        let postings = list_postings(&pool, "acct-jobs").unwrap();
        assert_eq!(postings.len(), 1);
        assert_eq!(postings[0].id, direct.id);
        assert_eq!(postings[0].source, "lever");
        assert_eq!(postings[0].availability_status, "active");
        assert_eq!(postings[0].last_verified_at_ms, direct.last_verified_at_ms);
    }

    #[test]
    fn discovery_sources_support_only_canonical_five_ats_identifiers() {
        let pool = test_pool();
        let cases = [
            ("greenhouse", "acme", "greenhouse"),
            ("lever", "atlas", "lever"),
            ("ashby", "orbit", "ashby"),
            ("smartrecruiters", "northstar", "smartrecruiters"),
            ("workday", "contoso~wd5~careers", "workday"),
        ];

        for (provider, source_key, expected_kind) in cases {
            let source = upsert_discovery_source(
                &pool,
                "acct-jobs",
                &DiscoverySourceInput {
                    track_id: String::new(),
                    provider: provider.to_string(),
                    source_key: source_key.to_string(),
                    company: "Example Company".to_string(),
                    run_interval_ms: DISCOVERY_MIN_INTERVAL_MS,
                },
            )
            .unwrap();

            assert_eq!(source.provider, provider);
            assert_eq!(source.source_key, source_key);
            assert_eq!(source.config["kind"], expected_kind);
            assert_eq!(source.config["company"], "Example Company");
            if provider == "workday" {
                assert_eq!(source.config["tenant"], "contoso");
                assert_eq!(source.config["instance"], "wd5");
                assert_eq!(source.config["site"], "careers");
                assert_eq!(source.config["locale"], "en-US");
            }
        }

        let invalid_workday = upsert_discovery_source(
            &pool,
            "acct-jobs",
            &DiscoverySourceInput {
                track_id: String::new(),
                provider: "workday".to_string(),
                source_key: "contoso~wd5".to_string(),
                company: "Example Company".to_string(),
                run_interval_ms: DISCOVERY_MIN_INTERVAL_MS,
            },
        )
        .unwrap_err();
        assert!(invalid_workday.to_string().contains("tenant~instance~site"));

        let invalid_non_workday = upsert_discovery_source(
            &pool,
            "acct-jobs",
            &DiscoverySourceInput {
                track_id: String::new(),
                provider: "greenhouse".to_string(),
                source_key: "acme~careers".to_string(),
                company: "Example Company".to_string(),
                run_interval_ms: DISCOVERY_MIN_INTERVAL_MS,
            },
        )
        .unwrap_err();
        assert!(invalid_non_workday
            .to_string()
            .contains("unsupported characters"));
    }

    #[test]
    fn verified_imports_resolve_to_their_original_public_ats_board() {
        let cases = [
            (
                "greenhouse_import",
                "https://boards.greenhouse.io/acme/jobs/123?gh_src=feed",
                "greenhouse",
                "acme",
            ),
            (
                "lever_import",
                "https://jobs.lever.co/atlas/job-123?lever-source=feed",
                "lever",
                "atlas",
            ),
            (
                "ashby_import",
                "https://jobs.ashbyhq.com/orbit/job-123?utm_source=feed",
                "ashby",
                "orbit",
            ),
            (
                "smartrecruiters_import",
                "https://jobs.smartrecruiters.com/northstar/744000123456789-platform-engineer",
                "smartrecruiters",
                "northstar",
            ),
            (
                "smartrecruiters_import",
                "https://jobs.smartrecruiters.com/Experian/744000138411689",
                "smartrecruiters",
                "Experian",
            ),
            (
                "workday_import",
                "https://contoso.wd5.myworkdayjobs.com/en-US/Careers/job/Austin/Software-Engineer_R12345",
                "workday",
                "contoso~wd5~Careers",
            ),
            (
                "workday_import",
                "https://workday.wd5.myworkdayjobs.com/en-US/Workday/job/Ireland-Dublin/Senior-Software-Engineer_JR-0107796",
                "workday",
                "workday~wd5~Workday",
            ),
        ];

        for (imported_source, url, provider, source_key) in cases {
            let input = discovery_source_input_from_verified_import(
                imported_source,
                url,
                "Example Company",
                "track-engineering",
            )
            .unwrap()
            .unwrap();
            assert_eq!(input.provider, provider);
            assert_eq!(input.source_key, source_key);
            assert_eq!(input.track_id, "track-engineering");
            assert_eq!(input.company, "Example Company");
        }
    }

    #[test]
    fn discovery_url_canonicalization_rejects_cross_source_and_hostile_targets() {
        let pool = test_pool();
        let ashby = upsert_discovery_source(
            &pool,
            "acct-jobs",
            &DiscoverySourceInput {
                track_id: String::new(),
                provider: "ashby".to_string(),
                source_key: "acme".to_string(),
                company: "Acme".to_string(),
                run_interval_ms: DISCOVERY_MIN_INTERVAL_MS,
            },
        )
        .unwrap();
        let smartrecruiters = upsert_discovery_source(
            &pool,
            "acct-jobs",
            &DiscoverySourceInput {
                track_id: String::new(),
                provider: "smartrecruiters".to_string(),
                source_key: "acme".to_string(),
                company: "Acme".to_string(),
                run_interval_ms: DISCOVERY_MIN_INTERVAL_MS,
            },
        )
        .unwrap();
        let workday = upsert_discovery_source(
            &pool,
            "acct-jobs",
            &DiscoverySourceInput {
                track_id: String::new(),
                provider: "workday".to_string(),
                source_key: "acme~wd5~Careers".to_string(),
                company: "Acme".to_string(),
                run_interval_ms: DISCOVERY_MIN_INTERVAL_MS,
            },
        )
        .unwrap();

        assert_eq!(
            canonicalize_discovered_url(
                &ashby,
                "https://jobs.ashbyhq.com/acme/job-123?utm_source=feed#details",
            )
            .unwrap(),
            "https://jobs.ashbyhq.com/acme/job-123"
        );
        assert_eq!(
            canonicalize_discovered_url(
                &smartrecruiters,
                "https://jobs.smartrecruiters.com/acme/744000123456789-platform-engineer",
            )
            .unwrap(),
            "https://jobs.smartrecruiters.com/acme/744000123456789-platform-engineer"
        );
        assert_eq!(
            canonicalize_discovered_url(
                &workday,
                "https://acme.wd5.myworkdayjobs.com/en-US/Careers/job/Austin/Software-Engineer_R12345",
            )
            .unwrap(),
            "https://acme.wd5.myworkdayjobs.com/en-US/Careers/job/Austin/Software-Engineer_R12345"
        );

        for raw in [
            "https://jobs.ashbyhq.com/other/job-123",
            "https://jobs.smartrecruiters.com/acme/744000123456789-platform-engineer",
            "https://jobs.ashbyhq.com@127.0.0.1/acme/job-123",
            "https://jobs.ashbyhq.com.evil.example/acme/job-123",
            "https://jobs.ashbyhq.com:444/acme/job-123",
            "https://127.0.0.1/acme/job-123",
        ] {
            assert!(canonicalize_discovered_url(&ashby, raw).is_err(), "{raw}");
        }
        for raw in [
            "https://jobs.smartrecruiters.com/other/744000123456789-platform-engineer",
            "https://sub.jobs.smartrecruiters.com/acme/744000123456789-platform-engineer",
            "https://jobs.smartrecruiters.com@169.254.169.254/acme/job-123",
        ] {
            assert!(
                canonicalize_discovered_url(&smartrecruiters, raw).is_err(),
                "{raw}"
            );
        }
        for raw in [
            "https://other.wd5.myworkdayjobs.com/en-US/Careers/job/Austin/Software-Engineer_R12345",
            "https://acme.wd5.myworkdayjobs.com.evil.example/en-US/Careers/job/Austin/Software-Engineer_R12345",
            "https://acme.wd5.myworkdayjobs.com/en-US/Other/job/Austin/Software-Engineer_R12345",
            "https://acme.wd6.myworkdayjobs.com/en-US/Careers/job/Austin/Software-Engineer_R12345",
        ] {
            assert!(canonicalize_discovered_url(&workday, raw).is_err(), "{raw}");
        }
    }

    #[test]
    fn non_public_or_unknown_imports_never_resolve_to_discovery_sources() {
        for (source, url) in [
            ("linkedin_import", "https://www.linkedin.com/jobs/view/123"),
            ("indeed_import", "https://www.indeed.com/viewjob?jk=123"),
            (
                "ziprecruiter_import",
                "https://www.ziprecruiter.com/jobs/123",
            ),
            ("dice_import", "https://www.dice.com/job-detail/123"),
            ("pasted_link", "https://jobs.ashbyhq.com/acme/job-123"),
            ("unknown", "https://127.0.0.1/private"),
        ] {
            assert!(
                discovery_source_input_from_verified_import(source, url, "Acme", "track")
                    .unwrap()
                    .is_none()
            );
        }
        assert!(discovery_source_input_from_verified_import(
            "ashby_import",
            "https://jobs.ashbyhq.com@127.0.0.1/acme/job-123",
            "Acme",
            "track",
        )
        .is_err());
    }

    #[test]
    fn discovery_rejects_forged_leases_and_pauses_after_three_failures() {
        let pool = test_pool();
        let source = upsert_discovery_source(
            &pool,
            "acct-jobs",
            &DiscoverySourceInput {
                track_id: String::new(),
                provider: "greenhouse".to_string(),
                source_key: "acme".to_string(),
                company: "Acme".to_string(),
                run_interval_ms: DISCOVERY_MIN_INTERVAL_MS,
            },
        )
        .unwrap();

        let retry_schedule_base = now_ms() - 10_000;
        for attempt in 0..3 {
            if attempt > 0 {
                pool.get()
                    .unwrap()
                    .execute(
                        "UPDATE jobs_discovery_sources SET next_run_at_ms = ?2 WHERE id = ?1",
                        params![source.id, retry_schedule_base - attempt],
                    )
                    .unwrap();
            }
            let lease = lease_due_discovery_source(&pool, "failure-worker")
                .unwrap()
                .unwrap();
            if attempt == 0 {
                let error = fail_discovery_run(
                    &pool,
                    &source.id,
                    &"f".repeat(43),
                    &lease.replay_key,
                    lease.scheduled_for_ms,
                    "timeout",
                )
                .unwrap_err();
                assert!(error.to_string().contains("stale"));
            }
            fail_discovery_run(
                &pool,
                &source.id,
                &lease.lease_token,
                &lease.replay_key,
                lease.scheduled_for_ms,
                "timeout",
            )
            .unwrap();
        }

        let paused = get_discovery_source(&pool, &source.id).unwrap().unwrap();
        assert_eq!(paused.consecutive_failures, 3);
        assert_eq!(paused.health, "paused");
        assert!(lease_due_discovery_source(&pool, "another-worker")
            .unwrap()
            .is_none());
    }

    #[test]
    fn stale_discovery_worker_cannot_mutate_and_changed_replay_conflicts() {
        let pool = test_pool();
        let source = upsert_discovery_source(
            &pool,
            "acct-jobs",
            &DiscoverySourceInput {
                track_id: String::new(),
                provider: "greenhouse".to_string(),
                source_key: "acme".to_string(),
                company: "Acme".to_string(),
                run_interval_ms: DISCOVERY_MIN_INTERVAL_MS,
            },
        )
        .unwrap();
        let stale = lease_due_discovery_source(&pool, "stale-worker")
            .unwrap()
            .unwrap();
        pool.get()
            .unwrap()
            .execute(
                "UPDATE jobs_discovery_sources SET lease_expires_at_ms = ?2 WHERE id = ?1",
                params![source.id, now_ms() - 1],
            )
            .unwrap();
        let current = lease_due_discovery_source(&pool, "current-worker")
            .unwrap()
            .unwrap();
        assert_eq!(stale.replay_key, current.replay_key);
        let snapshot = vec![discovered_job("stale-fence", "Platform Engineer")];

        let stale_error = complete_discovery_run(
            &pool,
            &source.id,
            &stale.lease_token,
            &stale.replay_key,
            stale.scheduled_for_ms,
            &snapshot,
            true,
        )
        .unwrap_err();
        assert!(stale_error.to_string().contains("stale"));
        assert!(list_postings(&pool, "acct-jobs").unwrap().is_empty());

        let completed = complete_discovery_run(
            &pool,
            &source.id,
            &current.lease_token,
            &current.replay_key,
            current.scheduled_for_ms,
            &snapshot,
            true,
        )
        .unwrap();
        assert!(!completed.replayed);
        let replayed = complete_discovery_run(
            &pool,
            &source.id,
            &current.lease_token,
            &current.replay_key,
            current.scheduled_for_ms,
            &snapshot,
            true,
        )
        .unwrap();
        assert!(replayed.replayed);

        let changed = vec![discovered_job("stale-fence", "Changed title")];
        let changed_error = complete_discovery_run(
            &pool,
            &source.id,
            &current.lease_token,
            &current.replay_key,
            current.scheduled_for_ms,
            &changed,
            true,
        )
        .unwrap_err();
        assert!(changed_error.to_string().contains("replay payload"));
    }

    #[test]
    fn concurrent_discovery_completion_and_failure_have_one_terminal_writer() {
        let pool = test_pool();
        let source = upsert_discovery_source(
            &pool,
            "acct-jobs",
            &DiscoverySourceInput {
                track_id: String::new(),
                provider: "greenhouse".to_string(),
                source_key: "acme".to_string(),
                company: "Acme".to_string(),
                run_interval_ms: DISCOVERY_MIN_INTERVAL_MS,
            },
        )
        .unwrap();
        let lease = lease_due_discovery_source(&pool, "race-worker")
            .unwrap()
            .unwrap();
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(3));
        let complete_pool = pool.clone();
        let complete_barrier = barrier.clone();
        let complete_source = source.id.clone();
        let complete_token = lease.lease_token.clone();
        let complete_replay = lease.replay_key.clone();
        let complete = std::thread::spawn(move || {
            complete_barrier.wait();
            complete_discovery_run(
                &complete_pool,
                &complete_source,
                &complete_token,
                &complete_replay,
                lease.scheduled_for_ms,
                &[discovered_job(
                    "completion-failure-race",
                    "Platform Engineer",
                )],
                true,
            )
        });
        let fail_pool = pool.clone();
        let fail_barrier = barrier.clone();
        let fail_source = source.id.clone();
        let fail_token = lease.lease_token.clone();
        let fail_replay = lease.replay_key.clone();
        let fail = std::thread::spawn(move || {
            fail_barrier.wait();
            fail_discovery_run(
                &fail_pool,
                &fail_source,
                &fail_token,
                &fail_replay,
                lease.scheduled_for_ms,
                "timeout",
            )
        });
        barrier.wait();
        let results = [complete.join().unwrap(), fail.join().unwrap()];
        assert_eq!(
            results
                .iter()
                .filter(|result| result.as_ref().is_ok_and(|value| !value.replayed))
                .count(),
            1
        );
        let status: String = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT status FROM jobs_discovery_runs WHERE source_id = ?1 AND replay_key = ?2",
                params![source.id, lease.replay_key],
                |row| row.get(0),
            )
            .unwrap();
        match status.as_str() {
            "completed" => assert_eq!(list_postings(&pool, "acct-jobs").unwrap().len(), 1),
            "failed" => assert!(list_postings(&pool, "acct-jobs").unwrap().is_empty()),
            _ => panic!("unexpected terminal run status: {status}"),
        }
    }

    #[test]
    fn concurrent_replay_payloads_commit_once_without_mixing_snapshots() {
        let pool = test_pool();
        let source = upsert_discovery_source(
            &pool,
            "acct-jobs",
            &DiscoverySourceInput {
                track_id: String::new(),
                provider: "greenhouse".to_string(),
                source_key: "acme".to_string(),
                company: "Acme".to_string(),
                run_interval_ms: DISCOVERY_MIN_INTERVAL_MS,
            },
        )
        .unwrap();
        let lease = lease_due_discovery_source(&pool, "replay-race-worker")
            .unwrap()
            .unwrap();
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(3));
        let workers = ["One", "Two"]
            .into_iter()
            .map(|title| {
                let pool = pool.clone();
                let source_id = source.id.clone();
                let token = lease.lease_token.clone();
                let replay = lease.replay_key.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    complete_discovery_run(
                        &pool,
                        &source_id,
                        &token,
                        &replay,
                        lease.scheduled_for_ms,
                        &[discovered_job("replay-race", title)],
                        true,
                    )
                })
            })
            .collect::<Vec<_>>();
        barrier.wait();
        let results = workers
            .into_iter()
            .map(|worker| worker.join().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            results
                .iter()
                .filter(|result| result.as_ref().is_ok_and(|value| !value.replayed))
                .count(),
            1
        );
        assert_eq!(results.iter().filter(|result| result.is_err()).count(), 1);
        let postings = list_postings(&pool, "acct-jobs").unwrap();
        assert_eq!(postings.len(), 1);
        assert!(matches!(postings[0].title.as_str(), "One" | "Two"));
    }

    #[test]
    fn concurrent_identical_replays_commit_once_and_remain_replayable() {
        let pool = test_pool();
        let source = upsert_discovery_source(
            &pool,
            "acct-jobs",
            &DiscoverySourceInput {
                track_id: String::new(),
                provider: "greenhouse".to_string(),
                source_key: "acme".to_string(),
                company: "Acme".to_string(),
                run_interval_ms: DISCOVERY_MIN_INTERVAL_MS,
            },
        )
        .unwrap();
        let lease = lease_due_discovery_source(&pool, "identical-replay-worker")
            .unwrap()
            .unwrap();
        let snapshot = vec![discovered_job("identical-replay", "Same title")];
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(3));
        let workers = (0..2)
            .map(|_| {
                let pool = pool.clone();
                let source_id = source.id.clone();
                let token = lease.lease_token.clone();
                let replay = lease.replay_key.clone();
                let snapshot = snapshot.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    complete_discovery_run(
                        &pool,
                        &source_id,
                        &token,
                        &replay,
                        lease.scheduled_for_ms,
                        &snapshot,
                        true,
                    )
                })
            })
            .collect::<Vec<_>>();
        barrier.wait();
        let results = workers
            .into_iter()
            .map(|worker| worker.join().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            results
                .iter()
                .filter(|result| result.as_ref().is_ok_and(|value| !value.replayed))
                .count(),
            1
        );
        let replayed = complete_discovery_run(
            &pool,
            &source.id,
            &lease.lease_token,
            &lease.replay_key,
            lease.scheduled_for_ms,
            &snapshot,
            true,
        )
        .unwrap();
        assert!(replayed.replayed);
        assert_eq!(list_postings(&pool, "acct-jobs").unwrap().len(), 1);
    }

    #[test]
    fn concurrent_discovery_and_ordinary_save_keep_one_posting_identity() {
        let pool = test_pool();
        let profile = default_profile("jobs@example.com");
        save_profile(&pool, "acct-jobs", &profile).unwrap();
        let source = upsert_discovery_source(
            &pool,
            "acct-jobs",
            &DiscoverySourceInput {
                track_id: "track-default".to_string(),
                provider: "greenhouse".to_string(),
                source_key: "acme".to_string(),
                company: "Acme".to_string(),
                run_interval_ms: DISCOVERY_MIN_INTERVAL_MS,
            },
        )
        .unwrap();
        let lease = lease_due_discovery_source(&pool, "posting-race-worker")
            .unwrap()
            .unwrap();
        let mut manual = test_posting(
            "https://boards.greenhouse.io/acme/jobs/posting-race?utm_source=manual",
            now_ms(),
            now_ms(),
        );
        manual.title = "Platform Engineer".to_string();
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(3));
        let discovery_pool = pool.clone();
        let discovery_barrier = barrier.clone();
        let discovery_source = source.id.clone();
        let discovery_token = lease.lease_token.clone();
        let discovery_replay = lease.replay_key.clone();
        let discover = std::thread::spawn(move || {
            discovery_barrier.wait();
            complete_discovery_run(
                &discovery_pool,
                &discovery_source,
                &discovery_token,
                &discovery_replay,
                lease.scheduled_for_ms,
                &[discovered_job("posting-race", "Platform Engineer")],
                true,
            )
        });
        let ordinary_pool = pool.clone();
        let ordinary_barrier = barrier.clone();
        let ordinary_profile = profile.clone();
        let ordinary = std::thread::spawn(move || {
            ordinary_barrier.wait();
            upsert_posting(
                &ordinary_pool,
                "acct-jobs",
                &manual,
                &ordinary_profile,
                &JobPreferences::default(),
            )
        });
        barrier.wait();
        assert!(discover.join().unwrap().is_ok());
        assert!(ordinary.join().unwrap().is_ok());
        let posting = list_postings(&pool, "acct-jobs").unwrap().pop().unwrap();
        let raw: String = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT posting_json FROM jobs_postings WHERE account_id = ?1 AND id = ?2",
                params!["acct-jobs", posting.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            parse_json::<JobPosting>(raw, "job posting").unwrap().id,
            posting.id
        );
        let membership_job_id: String = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT job_id FROM jobs_discovery_memberships WHERE source_id = ?1",
                params![source.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(membership_job_id, posting.id);
        let (application, _) =
            prepare_application(&pool, "acct-jobs", &posting.id, "factual", "review_first")
                .unwrap();
        assert_eq!(application.job_id, posting.id);
    }

    #[test]
    fn snapshot_publish_rolls_back_mid_commit_and_legacy_stale_commits_recover() {
        let pool = test_pool();
        let source = upsert_discovery_source(
            &pool,
            "acct-jobs",
            &DiscoverySourceInput {
                track_id: String::new(),
                provider: "greenhouse".to_string(),
                source_key: "acme".to_string(),
                company: "Acme".to_string(),
                run_interval_ms: DISCOVERY_MIN_INTERVAL_MS,
            },
        )
        .unwrap();
        let lease = lease_due_discovery_source(&pool, "recovery-worker")
            .unwrap()
            .unwrap();
        let conn = pool.get().unwrap();
        conn.execute_batch(
            "CREATE TRIGGER fail_discovery_membership_insert
             BEFORE INSERT ON jobs_discovery_memberships
             BEGIN SELECT RAISE(FAIL, 'injected membership failure'); END;",
        )
        .unwrap();
        drop(conn);

        let snapshot = vec![discovered_job("commit-recovery", "Platform Engineer")];
        assert!(complete_discovery_run(
            &pool,
            &source.id,
            &lease.lease_token,
            &lease.replay_key,
            lease.scheduled_for_ms,
            &snapshot,
            true,
        )
        .is_err());
        let conn = pool.get().unwrap();
        let status: String = conn
            .query_row(
                "SELECT status FROM jobs_discovery_runs WHERE source_id = ?1 AND replay_key = ?2",
                params![source.id, lease.replay_key],
                |row| row.get(0),
            )
            .unwrap();
        // The trigger fires after a candidate membership write. The single
        // publish transaction rolls that posting, membership, and run-fence
        // update back together instead of exposing a partial snapshot.
        assert_eq!(status, "running");
        assert!(list_postings(&pool, "acct-jobs").unwrap().is_empty());
        let memberships: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM jobs_discovery_memberships WHERE source_id = ?1",
                params![source.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(memberships, 0);
        conn.execute_batch("DROP TRIGGER fail_discovery_membership_insert;")
            .unwrap();
        // This state can only be left by the pre-atomic release. Preserve the
        // recovery path for it without depending on it for new failures.
        conn.execute(
            "UPDATE jobs_discovery_runs SET status = 'committing' WHERE source_id = ?1 AND replay_key = ?2",
            params![source.id, lease.replay_key],
        )
        .unwrap();
        conn.execute(
            "UPDATE jobs_discovery_sources SET lease_expires_at_ms = ?2 WHERE id = ?1",
            params![source.id, now_ms() - 1],
        )
        .unwrap();
        drop(conn);

        let recovered = lease_due_discovery_source(&pool, "recovery-worker-two")
            .unwrap()
            .unwrap();
        let conn = pool.get().unwrap();
        let (status, error): (String, Option<String>) = conn
            .query_row(
                "SELECT status, error_code FROM jobs_discovery_runs WHERE source_id = ?1 AND replay_key = ?2",
                params![source.id, lease.replay_key],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(status, "failed");
        assert_eq!(error.as_deref(), Some("commit_timeout"));
        drop(conn);
        complete_discovery_run(
            &pool,
            &source.id,
            &recovered.lease_token,
            &recovered.replay_key,
            recovered.scheduled_for_ms,
            &snapshot,
            true,
        )
        .unwrap();
        assert_eq!(list_postings(&pool, "acct-jobs").unwrap().len(), 1);
        assert_eq!(
            get_discovery_source(&pool, &source.id)
                .unwrap()
                .unwrap()
                .health,
            "healthy"
        );
    }

    #[test]
    fn discovery_membership_preserves_identity_when_a_job_key_drifts() {
        let pool = test_pool();
        let source = upsert_discovery_source(
            &pool,
            "acct-jobs",
            &DiscoverySourceInput {
                track_id: String::new(),
                provider: "greenhouse".to_string(),
                source_key: "acme".to_string(),
                company: "Acme".to_string(),
                run_interval_ms: DISCOVERY_MIN_INTERVAL_MS,
            },
        )
        .unwrap();
        let first_lease = lease_due_discovery_source(&pool, "key-drift-first")
            .unwrap()
            .unwrap();
        complete_discovery_run(
            &pool,
            &source.id,
            &first_lease.lease_token,
            &first_lease.replay_key,
            first_lease.scheduled_for_ms,
            &[discovered_job("key-drift", "Original title")],
            true,
        )
        .unwrap();
        let first = list_postings(&pool, "acct-jobs").unwrap().pop().unwrap();
        pool.get()
            .unwrap()
            .execute(
                "UPDATE jobs_discovery_sources SET next_run_at_ms = ?2 WHERE id = ?1",
                params![source.id, now_ms() - 1],
            )
            .unwrap();
        let second_lease = lease_due_discovery_source(&pool, "key-drift-second")
            .unwrap()
            .unwrap();
        complete_discovery_run(
            &pool,
            &source.id,
            &second_lease.lease_token,
            &second_lease.replay_key,
            second_lease.scheduled_for_ms,
            &[discovered_job("key-drift", "Changed title")],
            true,
        )
        .unwrap();
        let postings = list_postings(&pool, "acct-jobs").unwrap();
        assert_eq!(postings.len(), 1);
        assert_eq!(postings[0].id, first.id);
        assert_eq!(postings[0].title, "Changed title");
        let membership_job_id: String = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT job_id FROM jobs_discovery_memberships WHERE source_id = ?1 AND external_id = ?2",
                params![source.id, "key-drift"],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(membership_job_id, first.id);
    }

    #[test]
    fn discovery_disagreement_between_membership_and_canonical_job_rolls_back() {
        let pool = test_pool();
        let profile = default_profile("jobs@example.com");
        let preferences = JobPreferences::default();
        let source = upsert_discovery_source(
            &pool,
            "acct-jobs",
            &DiscoverySourceInput {
                track_id: String::new(),
                provider: "greenhouse".to_string(),
                source_key: "acme".to_string(),
                company: "Acme".to_string(),
                run_interval_ms: DISCOVERY_MIN_INTERVAL_MS,
            },
        )
        .unwrap();
        let member = upsert_posting(
            &pool,
            "acct-jobs",
            &test_posting(
                "https://boards.greenhouse.io/acme/jobs/membership-old",
                now_ms(),
                now_ms(),
            ),
            &profile,
            &preferences,
        )
        .unwrap();
        let mut canonical = test_posting(
            "https://boards.greenhouse.io/acme/jobs/disagreement?utm_source=test",
            now_ms(),
            now_ms(),
        );
        canonical.title = "Target title".to_string();
        let canonical =
            upsert_posting(&pool, "acct-jobs", &canonical, &profile, &preferences).unwrap();
        pool.get()
            .unwrap()
            .execute(
                "INSERT INTO jobs_discovery_memberships (
                    source_id, account_id, canonical_key, external_id, job_id, content_hash,
                    first_seen_at_ms, last_seen_at_ms, last_seen_run_id, availability_status
                 ) VALUES (?1, ?2, ?3, ?4, ?5, 'old', ?6, ?6, 'old-run', 'active')",
                params![
                    source.id,
                    "acct-jobs",
                    member.canonical_key,
                    "disagreement",
                    member.id,
                    now_ms(),
                ],
            )
            .unwrap();
        let lease = lease_due_discovery_source(&pool, "disagreement-worker")
            .unwrap()
            .unwrap();
        let error = complete_discovery_run(
            &pool,
            &source.id,
            &lease.lease_token,
            &lease.replay_key,
            lease.scheduled_for_ms,
            &[discovered_job("disagreement", "Target title")],
            true,
        )
        .unwrap_err();
        assert!(error
            .to_string()
            .contains("membership and canonical job disagree"));
        let membership_job_id: String = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT job_id FROM jobs_discovery_memberships WHERE source_id = ?1 AND external_id = ?2",
                params![source.id, "disagreement"],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(membership_job_id, member.id);
        assert_eq!(
            get_posting(&pool, "acct-jobs", &canonical.id)
                .unwrap()
                .unwrap()
                .title,
            "Target title"
        );
        let run_status: String = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT status FROM jobs_discovery_runs WHERE source_id = ?1 AND replay_key = ?2",
                params![source.id, lease.replay_key],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(run_status, "running");
    }

    #[test]
    fn discovery_source_limits_and_track_cleanup_are_enforced() {
        let pool = test_pool();
        let track = upsert_track(
            &pool,
            "acct-jobs",
            &CareerTrack {
                id: "track-discovery".to_string(),
                name: "Engineering".to_string(),
                role: "Engineer".to_string(),
                locations: vec![],
                remote_preference: "hybrid_ok".to_string(),
                application_identity_id: None,
                policy: CareerTrackPolicy::default(),
                active: true,
                match_count: 0,
                created_at_ms: 0,
                updated_at_ms: 0,
            },
        )
        .unwrap();
        for index in 0..DISCOVERY_MAX_SOURCES_PER_TRACK {
            upsert_discovery_source(
                &pool,
                "acct-jobs",
                &DiscoverySourceInput {
                    track_id: track.id.clone(),
                    provider: "greenhouse".to_string(),
                    source_key: format!("limit-board-{index}"),
                    company: "Acme".to_string(),
                    run_interval_ms: 0,
                },
            )
            .unwrap();
        }
        // Updating an already-counted binding remains idempotent and does not
        // consume an additional slot.
        upsert_discovery_source(
            &pool,
            "acct-jobs",
            &DiscoverySourceInput {
                track_id: track.id.clone(),
                provider: "greenhouse".to_string(),
                source_key: "limit-board-0".to_string(),
                company: "Acme updated".to_string(),
                run_interval_ms: 0,
            },
        )
        .unwrap();
        let limit_error = upsert_discovery_source(
            &pool,
            "acct-jobs",
            &DiscoverySourceInput {
                track_id: track.id.clone(),
                provider: "greenhouse".to_string(),
                source_key: "limit-board-overflow".to_string(),
                company: "Acme".to_string(),
                run_interval_ms: 0,
            },
        )
        .unwrap_err();
        assert!(limit_error.to_string().contains("source limit"));
        let delete_error = delete_track(&pool, "acct-jobs", &track.id).unwrap_err();
        assert!(delete_error
            .to_string()
            .contains("still has Jobs matches or discovery sources"));
        assert_eq!(
            list_discovery_sources(&pool, "acct-jobs").unwrap().len(),
            DISCOVERY_MAX_SOURCES_PER_TRACK
        );
    }

    #[test]
    fn concurrent_source_enrollment_has_one_quota_winner() {
        let pool = test_pool();
        for index in 0..(DISCOVERY_MAX_SOURCES_PER_TRACK - 1) {
            upsert_discovery_source(
                &pool,
                "acct-jobs",
                &DiscoverySourceInput {
                    track_id: String::new(),
                    provider: "lever".to_string(),
                    source_key: format!("quota-existing-{index}"),
                    company: "Acme".to_string(),
                    run_interval_ms: 0,
                },
            )
            .unwrap();
        }
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(3));
        let workers = ["quota-race-a", "quota-race-b"]
            .into_iter()
            .map(|source_key| {
                let pool = pool.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    upsert_discovery_source(
                        &pool,
                        "acct-jobs",
                        &DiscoverySourceInput {
                            track_id: String::new(),
                            provider: "lever".to_string(),
                            source_key: source_key.to_string(),
                            company: "Acme".to_string(),
                            run_interval_ms: 0,
                        },
                    )
                })
            })
            .collect::<Vec<_>>();
        barrier.wait();
        let results = workers
            .into_iter()
            .map(|worker| worker.join().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
        assert_eq!(
            list_discovery_sources(&pool, "acct-jobs").unwrap().len(),
            DISCOVERY_MAX_SOURCES_PER_TRACK
        );
    }

    #[test]
    fn concurrent_verified_imports_bind_a_final_board_slot_to_one_track() {
        let pool = test_pool();
        let profile = default_profile("jobs@example.com");
        let preferences = JobPreferences::default();
        for index in 0..3 {
            upsert_track(
                &pool,
                "acct-jobs",
                &CareerTrack {
                    id: format!("filler-final-{index}"),
                    name: format!("Filler {index}"),
                    role: "Engineer".to_string(),
                    locations: Vec::new(),
                    remote_preference: "hybrid_ok".to_string(),
                    application_identity_id: None,
                    policy: CareerTrackPolicy::default(),
                    active: true,
                    match_count: 0,
                    created_at_ms: 0,
                    updated_at_ms: 0,
                },
            )
            .unwrap();
        }
        for index in 0..(DISCOVERY_MAX_SOURCES_PER_ACCOUNT - 1) {
            upsert_discovery_source(
                &pool,
                "acct-jobs",
                &DiscoverySourceInput {
                    track_id: format!("filler-final-{}", index / DISCOVERY_MAX_SOURCES_PER_TRACK),
                    provider: "lever".to_string(),
                    source_key: format!("account-final-slot-{index}"),
                    company: "Acme".to_string(),
                    run_interval_ms: 0,
                },
            )
            .unwrap();
        }
        let tracks = ["track-final-a", "track-final-b"]
            .into_iter()
            .map(|id| {
                upsert_track(
                    &pool,
                    "acct-jobs",
                    &CareerTrack {
                        id: id.to_string(),
                        name: id.to_string(),
                        role: "Engineer".to_string(),
                        locations: Vec::new(),
                        remote_preference: "hybrid_ok".to_string(),
                        application_identity_id: None,
                        policy: CareerTrackPolicy::default(),
                        active: true,
                        match_count: 0,
                        created_at_ms: 0,
                        updated_at_ms: 0,
                    },
                )
                .unwrap()
            })
            .collect::<Vec<_>>();
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(3));
        let workers = tracks
            .into_iter()
            .enumerate()
            .map(|(index, track)| {
                let pool = pool.clone();
                let profile = profile.clone();
                let preferences = preferences.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    let source = DiscoverySourceInput {
                        track_id: track.id.clone(),
                        provider: "ashby".to_string(),
                        source_key: "shared-final-board".to_string(),
                        company: "Acme".to_string(),
                        run_interval_ms: 0,
                    };
                    let mut posting = test_posting(
                        &format!("https://jobs.ashbyhq.com/shared-final-board/job-{index}"),
                        now_ms(),
                        now_ms(),
                    );
                    posting.source = "ashby".to_string();
                    posting.track_id = track.id;
                    barrier.wait();
                    save_verified_import_posting_with_source(
                        &pool,
                        "acct-jobs",
                        &posting,
                        &source,
                        &profile,
                        &preferences,
                    )
                })
            })
            .collect::<Vec<_>>();
        barrier.wait();
        let results = workers
            .into_iter()
            .map(|worker| worker.join().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
        let sources = list_discovery_sources(&pool, "acct-jobs").unwrap();
        assert_eq!(sources.len(), DISCOVERY_MAX_SOURCES_PER_ACCOUNT);
        let bound = sources
            .iter()
            .find(|source| source.provider == "ashby" && source.source_key == "shared-final-board")
            .unwrap();
        assert!(matches!(
            bound.track_id.as_str(),
            "track-final-a" | "track-final-b"
        ));
        let postings = list_postings(&pool, "acct-jobs").unwrap();
        assert_eq!(postings.len(), 1);
        assert_eq!(postings[0].track_id, bound.track_id);
    }

    #[test]
    fn verified_import_source_and_posting_commit_or_roll_back_together() {
        let profile = default_profile("jobs@example.com");
        let preferences = JobPreferences::default();
        let source = DiscoverySourceInput {
            track_id: String::new(),
            provider: "ashby".to_string(),
            source_key: "atomic-board".to_string(),
            company: "Acme".to_string(),
            run_interval_ms: 0,
        };
        let posting = test_posting(
            "https://jobs.ashbyhq.com/atomic-board/job-123",
            now_ms(),
            now_ms(),
        );

        let source_failure_pool = test_pool();
        source_failure_pool
            .get()
            .unwrap()
            .execute_batch(
                "CREATE TRIGGER fail_verified_source_insert
                 BEFORE INSERT ON jobs_discovery_sources
                 BEGIN SELECT RAISE(FAIL, 'injected source failure'); END;",
            )
            .unwrap();
        assert!(save_verified_import_posting_with_source(
            &source_failure_pool,
            "acct-jobs",
            &posting,
            &source,
            &profile,
            &preferences,
        )
        .is_err());
        assert!(list_discovery_sources(&source_failure_pool, "acct-jobs")
            .unwrap()
            .is_empty());
        assert!(list_postings(&source_failure_pool, "acct-jobs")
            .unwrap()
            .is_empty());

        let posting_failure_pool = test_pool();
        posting_failure_pool
            .get()
            .unwrap()
            .execute_batch(
                "CREATE TRIGGER fail_verified_posting_insert
                 BEFORE INSERT ON jobs_postings
                 BEGIN SELECT RAISE(FAIL, 'injected posting failure'); END;",
            )
            .unwrap();
        assert!(save_verified_import_posting_with_source(
            &posting_failure_pool,
            "acct-jobs",
            &posting,
            &source,
            &profile,
            &preferences,
        )
        .is_err());
        assert!(list_discovery_sources(&posting_failure_pool, "acct-jobs")
            .unwrap()
            .is_empty());
        assert!(list_postings(&posting_failure_pool, "acct-jobs")
            .unwrap()
            .is_empty());
    }

    #[test]
    fn verified_import_membership_preserves_job_identity_across_ats_url_aliases() {
        let cases = [
            (
                "greenhouse",
                "aliasboard",
                "https://job-boards.greenhouse.io/aliasboard/jobs/alias-601",
                "https://boards.greenhouse.io/aliasboard/jobs/alias-601",
                "alias-601",
            ),
            (
                "lever",
                "aliasboard",
                "https://jobs.eu.lever.co/aliasboard/lever-601",
                "https://jobs.lever.co/aliasboard/lever-601",
                "lever-601",
            ),
            (
                "smartrecruiters",
                "AliasCo",
                "https://jobs.smartrecruiters.com/AliasCo/7440001-platform-engineer",
                "https://jobs.smartrecruiters.com/AliasCo/7440001",
                "7440001",
            ),
            (
                "workday",
                "aliasco~wd5~Careers",
                "https://aliasco.wd5.myworkdayjobs.com/en-GB/Careers/job/London/Engineer_R601",
                "https://aliasco.wd5.myworkdayjobs.com/en-US/Careers/job/London/Engineer_R601",
                "R601",
            ),
        ];

        for (provider, source_key, import_url, snapshot_url, external_id) in cases {
            let pool = test_pool();
            let profile = default_profile("jobs@example.com");
            let preferences = JobPreferences::default();
            let source = DiscoverySourceInput {
                track_id: "track-default".to_string(),
                provider: provider.to_string(),
                source_key: source_key.to_string(),
                company: "Alias Co".to_string(),
                run_interval_ms: DISCOVERY_MIN_INTERVAL_MS,
            };
            let mut imported = test_posting(import_url, now_ms(), now_ms());
            imported.source = format!("{provider}_import");
            imported.company = "Alias Co".to_string();
            imported.external_id = external_id.to_string();
            let saved = save_verified_import_posting_with_source(
                &pool,
                "acct-jobs",
                &imported,
                &source,
                &profile,
                &preferences,
            )
            .unwrap();

            let stored_source = list_discovery_sources(&pool, "acct-jobs")
                .unwrap()
                .into_iter()
                .next()
                .unwrap();
            let (membership_job_id, membership_status): (String, String) = pool
                .get()
                .unwrap()
                .query_row(
                    "SELECT job_id, availability_status FROM jobs_discovery_memberships
                      WHERE source_id = ?1 AND external_id = ?2",
                    params![stored_source.id, external_id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .unwrap();
            assert_eq!(membership_job_id, saved.id, "{provider}");
            assert_eq!(membership_status, "pending", "{provider}");

            let pending_eligibility =
                evaluate_job_eligibility(&pool, "acct-jobs", &saved, true, None).unwrap();
            assert!(!pending_eligibility.can_queue_local, "{provider}");
            assert!(!pending_eligibility.can_queue_cloud, "{provider}");
            assert!(
                pending_eligibility
                    .review_reasons
                    .iter()
                    .any(|reason| reason.code == "discovery_source_unhealthy"),
                "{provider}"
            );
            assert!(
                !pending_eligibility
                    .passed_checks
                    .contains(&"discovery_source_healthy".to_string()),
                "{provider}"
            );

            let lease = lease_due_discovery_source(&pool, "alias-worker")
                .unwrap()
                .unwrap();
            complete_discovery_run(
                &pool,
                &stored_source.id,
                &lease.lease_token,
                &lease.replay_key,
                lease.scheduled_for_ms,
                &[DiscoveredJobInput {
                    external_id: external_id.to_string(),
                    canonical_url: snapshot_url.to_string(),
                    company: String::new(),
                    source_catalog_id: String::new(),
                    requires_original_revalidation: false,
                    title: imported.title.clone(),
                    location: imported.location.clone(),
                    workplace: imported.workplace.clone(),
                    description: imported.description.clone(),
                    compensation: imported.compensation.clone(),
                    employment_type: imported.employment_type.clone(),
                    engagement_type: String::new(),
                    posted_at_ms: imported.posted_at_ms,
                }],
                true,
            )
            .unwrap();

            let postings = list_postings(&pool, "acct-jobs").unwrap();
            assert_eq!(postings.len(), 1, "{provider}");
            assert_eq!(postings[0].id, saved.id, "{provider}");
            assert_eq!(postings[0].canonical_url, snapshot_url, "{provider}");
            let membership_status: String = pool
                .get()
                .unwrap()
                .query_row(
                    "SELECT availability_status FROM jobs_discovery_memberships
                      WHERE source_id = ?1 AND external_id = ?2",
                    params![stored_source.id, external_id],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(membership_status, "active", "{provider}");
            let refreshed = list_postings(&pool, "acct-jobs").unwrap().remove(0);
            let active_eligibility =
                evaluate_job_eligibility(&pool, "acct-jobs", &refreshed, true, None).unwrap();
            assert!(
                active_eligibility
                    .passed_checks
                    .contains(&"discovery_source_healthy".to_string()),
                "{provider}"
            );
        }
    }

    #[test]
    fn track_deletion_rejects_a_posting_without_a_discovery_source() {
        let pool = test_pool();
        let track = upsert_track(
            &pool,
            "acct-jobs",
            &CareerTrack {
                id: "track-with-posting".to_string(),
                name: "Engineering".to_string(),
                role: "Engineer".to_string(),
                locations: Vec::new(),
                remote_preference: "hybrid_ok".to_string(),
                application_identity_id: None,
                policy: CareerTrackPolicy::default(),
                active: true,
                match_count: 0,
                created_at_ms: 0,
                updated_at_ms: 0,
            },
        )
        .unwrap();
        let mut posting = test_posting(
            "https://boards.greenhouse.io/acme/jobs/track-bound-posting",
            now_ms(),
            now_ms(),
        );
        posting.track_id = track.id.clone();
        upsert_posting(
            &pool,
            "acct-jobs",
            &posting,
            &default_profile("jobs@example.com"),
            &JobPreferences::default(),
        )
        .unwrap();

        let error = delete_track(&pool, "acct-jobs", &track.id).unwrap_err();
        assert!(error
            .to_string()
            .contains("still has Jobs matches or discovery sources"));
        assert!(list_tracks(&pool, "acct-jobs")
            .unwrap()
            .iter()
            .any(|item| item.id == track.id));
    }

    #[test]
    fn workspace_discovery_sources_serialize_only_the_public_summary() {
        let pool = test_pool();
        save_profile(&pool, "acct-jobs", &default_profile("jobs@example.com")).unwrap();
        let source = upsert_discovery_source(
            &pool,
            "acct-jobs",
            &DiscoverySourceInput {
                track_id: String::new(),
                provider: "greenhouse".to_string(),
                source_key: "private-board-token".to_string(),
                company: "Acme".to_string(),
                run_interval_ms: DISCOVERY_MIN_INTERVAL_MS,
            },
        )
        .unwrap();
        let value =
            serde_json::to_value(workspace(&pool, "acct-jobs", "jobs@example.com").unwrap())
                .unwrap();
        let public_source = value["discovery_sources"][0].as_object().unwrap();
        let keys = public_source.keys().map(String::as_str).collect::<Vec<_>>();
        assert_eq!(
            keys,
            vec![
                "config",
                "health",
                "id",
                "last_success_at_ms",
                "provider",
                "status"
            ]
        );
        assert_eq!(public_source["id"], source.id);
        assert_eq!(public_source["config"], json!({ "company": "Acme" }));
        let serialized = serde_json::to_string(public_source).unwrap();
        assert!(!serialized.contains("acct-jobs"));
        assert!(!serialized.contains("private-board-token"));
        assert!(!serialized.contains("next_run_at_ms"));
        assert!(!serialized.contains("last_error_code"));
        assert!(!serialized.contains("lease_expires_at_ms"));
    }

    #[test]
    fn execution_lease_expiry_rotation_and_finish_are_replay_safe() {
        let pool = test_pool();
        let (application, run_id, browser_profile_id) =
            execution_lease_fixture(&pool, "lease-expiry");
        let first = claim_execution_lease(
            &pool,
            "acct-jobs",
            &application.id,
            &run_id,
            &browser_profile_id,
            "owner-one",
        )
        .unwrap();
        let stored_hash: String = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT lease_token_sha256 FROM jobs_execution_leases WHERE run_id = ?1",
                params![run_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(stored_hash, execution_lease_token_hash(&first.lease_token));
        assert_ne!(stored_hash, first.lease_token);
        heartbeat_execution_lease(
            &pool,
            "acct-jobs",
            &application.id,
            &run_id,
            &first.lease_token,
            first.fence,
        )
        .unwrap();
        pool.get()
            .unwrap()
            .execute(
                "UPDATE jobs_execution_leases SET lease_expires_at_ms = ?2 WHERE run_id = ?1",
                params![run_id, now_ms() - 1],
            )
            .unwrap();
        let rotated = claim_execution_lease(
            &pool,
            "acct-jobs",
            &application.id,
            &run_id,
            &browser_profile_id,
            "owner-two",
        )
        .unwrap();
        assert!(rotated.fence > first.fence);
        assert_ne!(rotated.lease_token, first.lease_token);
        assert!(matches!(
            heartbeat_execution_lease(
                &pool,
                "acct-jobs",
                &application.id,
                &run_id,
                &first.lease_token,
                first.fence,
            ),
            Err(ExecutionLeaseError::Conflict)
        ));
        let started = start_irreversible_submission(
            &pool,
            "acct-jobs",
            &application.id,
            &run_id,
            &rotated.lease_token,
            rotated.fence,
        )
        .unwrap();
        assert_eq!(started.phase, "click_started");
        assert!(matches!(
            finish_execution_lease(
                &pool,
                "acct-jobs",
                &application.id,
                &run_id,
                &rotated.lease_token,
                rotated.fence,
                "failed",
            ),
            Err(ExecutionLeaseError::Conflict)
        ));
        pool.get()
            .unwrap()
            .execute(
                "UPDATE jobs_execution_leases SET lease_expires_at_ms = ?2 WHERE run_id = ?1",
                params![run_id, now_ms() - 1],
            )
            .unwrap();
        assert!(matches!(
            claim_execution_lease(
                &pool,
                "acct-jobs",
                &application.id,
                &run_id,
                &browser_profile_id,
                "owner-two",
            ),
            Err(ExecutionLeaseError::Conflict)
        ));
        finish_execution_lease(
            &pool,
            "acct-jobs",
            &application.id,
            &run_id,
            &rotated.lease_token,
            rotated.fence,
            "submitted",
        )
        .unwrap();
        finish_execution_lease(
            &pool,
            "acct-jobs",
            &application.id,
            &run_id,
            &rotated.lease_token,
            rotated.fence,
            "submitted",
        )
        .unwrap();
        assert!(matches!(
            finish_execution_lease(
                &pool,
                "acct-jobs",
                &application.id,
                &run_id,
                &rotated.lease_token,
                rotated.fence,
                "failed",
            ),
            Err(ExecutionLeaseError::Conflict)
        ));
    }

    #[test]
    fn execution_lease_irreversible_transition_has_one_race_winner() {
        let pool = test_pool();
        let (application, run_id, browser_profile_id) =
            execution_lease_fixture(&pool, "lease-race");
        let lease = claim_execution_lease(
            &pool,
            "acct-jobs",
            &application.id,
            &run_id,
            &browser_profile_id,
            "race-owner",
        )
        .unwrap();
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(3));
        let mut workers = Vec::new();
        for _ in 0..2 {
            let pool = pool.clone();
            let application_id = application.id.clone();
            let run_id = run_id.clone();
            let token = lease.lease_token.clone();
            let fence = lease.fence;
            let barrier = barrier.clone();
            workers.push(std::thread::spawn(move || {
                barrier.wait();
                start_irreversible_submission(
                    &pool,
                    "acct-jobs",
                    &application_id,
                    &run_id,
                    &token,
                    fence,
                )
            }));
        }
        barrier.wait();
        let results = workers
            .into_iter()
            .map(|worker| worker.join().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
        assert_eq!(
            results
                .iter()
                .filter(|result| matches!(result, Err(ExecutionLeaseError::Conflict)))
                .count(),
            1
        );
        assert!(matches!(
            start_irreversible_submission(
                &pool,
                "acct-jobs",
                &application.id,
                &run_id,
                &lease.lease_token,
                lease.fence,
            ),
            Err(ExecutionLeaseError::Conflict)
        ));
    }

    #[test]
    fn execution_lease_excludes_parallel_runs_for_one_browser_profile() {
        let pool = test_pool();
        let (first_application, first_run, first_profile) =
            execution_lease_fixture(&pool, "profile-first");
        let (second_application, second_run, second_profile) =
            execution_lease_fixture(&pool, "profile-second");
        assert_eq!(first_profile, second_profile);
        let first = claim_execution_lease(
            &pool,
            "acct-jobs",
            &first_application.id,
            &first_run,
            &first_profile,
            "profile-owner-one",
        )
        .unwrap();
        assert!(matches!(
            claim_execution_lease(
                &pool,
                "acct-jobs",
                &second_application.id,
                &second_run,
                &second_profile,
                "profile-owner-two",
            ),
            Err(ExecutionLeaseError::Conflict)
        ));
        finish_execution_lease(
            &pool,
            "acct-jobs",
            &first_application.id,
            &first_run,
            &first.lease_token,
            first.fence,
            "released",
        )
        .unwrap();
        let second = claim_execution_lease(
            &pool,
            "acct-jobs",
            &second_application.id,
            &second_run,
            &second_profile,
            "profile-owner-two",
        )
        .unwrap();
        assert!(second.fence > first.fence);
    }

    #[test]
    fn canonical_job_key_deduplicates_url_slash_and_case() {
        let a = JobPosting {
            id: String::new(),
            canonical_key: String::new(),
            source: "greenhouse".to_string(),
            external_id: String::new(),
            company: "Acme".to_string(),
            title: "Product Designer".to_string(),
            location: "New York, NY".to_string(),
            workplace: "hybrid".to_string(),
            canonical_url: "https://boards.example/jobs/1/".to_string(),
            description: String::new(),
            compensation: String::new(),
            employment_type: String::new(),
            track_id: String::new(),
            match_score: 0,
            matched_reasons: Vec::new(),
            missing_requirements: Vec::new(),
            posted_at_ms: Some(now_ms()),
            last_verified_at_ms: Some(now_ms()),
            availability_status: "active".to_string(),
            status: "matched".to_string(),
            created_at_ms: 0,
            updated_at_ms: 0,
            eligibility: None,
        };
        let mut b = a.clone();
        b.company = "ACME".to_string();
        b.canonical_url =
            "https://boards.example/jobs/1?ref=feed&utm_source=newsletter".to_string();
        assert_eq!(canonical_job_key(&a), canonical_job_key(&b));
    }

    #[test]
    fn stale_jobs_cannot_produce_application_packets() {
        let pool = test_pool();
        let profile = default_profile("jobs@example.com");
        save_profile(&pool, "acct-jobs", &profile).unwrap();
        let posting = upsert_posting(
            &pool,
            "acct-jobs",
            &test_posting(
                "https://boards.example/jobs/stale",
                now_ms() - 31 * DAY_MS,
                now_ms(),
            ),
            &profile,
            &JobPreferences::default(),
        )
        .unwrap();

        let error = prepare_application(&pool, "acct-jobs", &posting.id, "factual", "review_first")
            .unwrap_err();
        assert!(error.to_string().contains("days ago"));
    }

    #[test]
    fn queueing_rechecks_that_a_recent_job_is_still_open() {
        let pool = test_pool();
        let profile = default_profile("jobs@example.com");
        save_profile(&pool, "acct-jobs", &profile).unwrap();
        let mut posting = upsert_posting(
            &pool,
            "acct-jobs",
            &test_posting(
                "https://boards.greenhouse.io/acme/jobs/recheck",
                now_ms() - 2 * DAY_MS,
                now_ms() - 2 * DAY_MS,
            ),
            &profile,
            &JobPreferences::default(),
        )
        .unwrap();
        let (application, _) =
            prepare_application(&pool, "acct-jobs", &posting.id, "factual", "review_first")
                .unwrap();

        let stale_verification =
            update_application(&pool, "acct-jobs", &application.id, "queued", None).unwrap_err();
        assert!(stale_verification.to_string().contains("still open"));

        posting.last_verified_at_ms = Some(now_ms());
        upsert_posting(
            &pool,
            "acct-jobs",
            &posting,
            &profile,
            &JobPreferences::default(),
        )
        .unwrap();
        let queued = update_application(&pool, "acct-jobs", &application.id, "queued", None)
            .unwrap()
            .unwrap();
        assert_eq!(queued.state, "queued");
    }

    #[test]
    fn email_otp_interventions_store_only_provider_references() {
        let pool = test_pool();
        let intervention = Intervention {
            id: String::new(),
            application_id: None,
            kind: "two_factor".to_string(),
            status: "open".to_string(),
            title: "Email code ready".to_string(),
            detail: "Approve the matching code from your connected inbox.".to_string(),
            choices: Vec::new(),
            resolution_kind: "email_otp_approval".to_string(),
            resume_after_resolution: true,
            provider: "gmail".to_string(),
            provider_message_id: "gmail-message-1".to_string(),
            expires_at_ms: Some(now_ms() + 10 * 60 * 1_000),
            metadata: json!({ "destination": "j•••@example.com" }),
            created_at_ms: 0,
            resolved_at_ms: None,
        };
        let saved = save_intervention(&pool, "acct-jobs", &intervention).unwrap();
        assert_eq!(saved.provider_message_id, "gmail-message-1");
        assert_eq!(saved.resolution_kind, "email_otp_approval");

        let mut unsafe_intervention = intervention;
        unsafe_intervention.id.clear();
        unsafe_intervention.metadata = json!({ "otp": "824193" });
        let error = save_intervention(&pool, "acct-jobs", &unsafe_intervention).unwrap_err();
        assert!(error.to_string().contains("cannot be stored"));
    }

    #[test]
    fn submitted_applications_require_verified_runner_finalization() {
        let pool = test_pool();
        let profile = default_profile("jobs@example.com");
        save_profile(&pool, "acct-jobs", &profile).unwrap();
        let posting = upsert_posting(
            &pool,
            "acct-jobs",
            &test_posting(
                "https://boards.greenhouse.io/acme/jobs/evidence",
                now_ms() - DAY_MS,
                now_ms(),
            ),
            &profile,
            &JobPreferences::default(),
        )
        .unwrap();
        let (application, resume) =
            prepare_application(&pool, "acct-jobs", &posting.id, "factual", "review_first")
                .unwrap();
        update_application(&pool, "acct-jobs", &application.id, "queued", None).unwrap();
        update_application(&pool, "acct-jobs", &application.id, "running", None).unwrap();

        let resume_evidence = ApplicationEvidence {
            id: "resume-evidence".to_string(),
            application_id: application.id.clone(),
            kind: "resume".to_string(),
            label: "Resume submitted".to_string(),
            provider: "greenhouse".to_string(),
            file_name: "Taylor-Rivera-Acme-Software-Engineer.pdf".to_string(),
            media_type: "application/pdf".to_string(),
            storage_key: "jobs/application/resume.pdf".to_string(),
            sha256: "a".repeat(64),
            resume_version_id: Some(resume.id.clone()),
            occurred_at_ms: now_ms(),
            metadata: json!({ "attached_to_submission": true }),
            created_at_ms: 0,
        };
        save_application_evidence(&pool, "acct-jobs", &resume_evidence).unwrap();
        save_application_evidence(&pool, "acct-jobs", &resume_evidence).unwrap();
        assert_eq!(
            list_application_evidence(&pool, "acct-jobs", Some(&application.id))
                .unwrap()
                .iter()
                .filter(|item| item.kind == "resume")
                .count(),
            1
        );
        save_application_evidence(
            &pool,
            "acct-jobs",
            &ApplicationEvidence {
                id: "confirmation-evidence".to_string(),
                application_id: application.id.clone(),
                kind: "submission_confirmation".to_string(),
                label: "Application received".to_string(),
                provider: "greenhouse".to_string(),
                file_name: String::new(),
                media_type: String::new(),
                storage_key: String::new(),
                sha256: String::new(),
                resume_version_id: None,
                occurred_at_ms: now_ms(),
                metadata: json!({
                    "external_id": "greenhouse-confirmation-1",
                    "confirmation": "Application received"
                }),
                created_at_ms: 0,
            },
        )
        .unwrap();
        let error =
            update_application(&pool, "acct-jobs", &application.id, "submitted", None).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("only a verified runner receipt can finalize")
        );
        let unchanged = get_application(&pool, "acct-jobs", &application.id)
            .unwrap()
            .unwrap();
        assert_eq!(unchanged.state, "running");
        assert!(unchanged.submitted_at_ms.is_none());
    }

    #[test]
    fn packet_resume_is_job_specific_and_idempotent() {
        let pool = test_pool();
        let mut profile = default_profile("jobs@example.com");
        profile.full_name = "Taylor Rivera".to_string();
        profile.skills = vec!["Rust".to_string(), "TypeScript".to_string()];
        profile.summary = "Builds reliable customer products.".to_string();
        save_profile(&pool, "acct-jobs", &profile).unwrap();
        let posting = upsert_posting(
            &pool,
            "acct-jobs",
            &JobPosting {
                id: String::new(),
                canonical_key: String::new(),
                source: "pasted_link".to_string(),
                external_id: String::new(),
                company: "Northstar".to_string(),
                title: "Software Product Engineer".to_string(),
                location: "Remote".to_string(),
                workplace: "remote".to_string(),
                canonical_url: "https://example.com/jobs/42".to_string(),
                description: "Rust and TypeScript".to_string(),
                compensation: String::new(),
                employment_type: String::new(),
                track_id: "track-default".to_string(),
                match_score: 86,
                matched_reasons: vec!["Skills fit".to_string()],
                missing_requirements: Vec::new(),
                posted_at_ms: Some(now_ms()),
                last_verified_at_ms: Some(now_ms()),
                availability_status: "active".to_string(),
                status: "matched".to_string(),
                created_at_ms: 0,
                updated_at_ms: 0,
                eligibility: None,
            },
            &profile,
            &JobPreferences::default(),
        )
        .unwrap();
        let (application_a, resume_a) =
            prepare_application(&pool, "acct-jobs", &posting.id, "factual", "review_first")
                .unwrap();
        let (application_b, resume_b) =
            prepare_application(&pool, "acct-jobs", &posting.id, "factual", "review_first")
                .unwrap();
        assert_eq!(application_a.id, application_b.id);
        assert_eq!(resume_a.id, resume_b.id);
        assert_eq!(resume_a.job_id, posting.id);
    }

    #[test]
    fn job_specific_packets_emphasize_different_existing_evidence() {
        let pool = test_pool();
        let mut profile = default_profile("jobs@example.com");
        profile.full_name = "Taylor Rivera".to_string();
        profile.headline = "Software Engineer".to_string();
        profile.summary = "Builds reliable customer products.".to_string();
        profile.skills = vec![
            "React.js".to_string(),
            "Amazon Web Services".to_string(),
            "PostgreSQL".to_string(),
        ];
        profile.employment = vec![EmploymentEntry {
            id: "employment-1".to_string(),
            company: "Northstar".to_string(),
            title: "Software Engineer".to_string(),
            highlights: vec![
                "Built React interfaces for customer workflows.".to_string(),
                "Designed AWS data services backed by Postgres.".to_string(),
            ],
            ..EmploymentEntry::default()
        }];
        save_profile(&pool, "acct-jobs", &profile).unwrap();

        let mut cloud_job = test_posting(
            "https://boards.greenhouse.io/cloudco/jobs/cloud-engineer",
            now_ms(),
            now_ms(),
        );
        cloud_job.company = "Cloudco".to_string();
        cloud_job.title = "Cloud Engineer".to_string();
        cloud_job.description = "Build AWS services backed by PostgreSQL.".to_string();
        let cloud_job = upsert_posting(
            &pool,
            "acct-jobs",
            &cloud_job,
            &profile,
            &JobPreferences::default(),
        )
        .unwrap();

        let mut frontend_job = test_posting(
            "https://boards.greenhouse.io/webco/jobs/frontend-engineer",
            now_ms(),
            now_ms(),
        );
        frontend_job.company = "Webco".to_string();
        frontend_job.title = "Frontend Engineer".to_string();
        frontend_job.description = "Build customer interfaces with React.".to_string();
        let frontend_job = upsert_posting(
            &pool,
            "acct-jobs",
            &frontend_job,
            &profile,
            &JobPreferences::default(),
        )
        .unwrap();

        let (cloud_application, cloud_resume) =
            prepare_application(&pool, "acct-jobs", &cloud_job.id, "factual", "review_first")
                .unwrap();
        let (frontend_application, frontend_resume) = prepare_application(
            &pool,
            "acct-jobs",
            &frontend_job.id,
            "factual",
            "review_first",
        )
        .unwrap();

        assert_eq!(cloud_application.state, "awaiting_review");
        assert_eq!(frontend_application.state, "awaiting_review");
        assert_ne!(cloud_resume.id, frontend_resume.id);
        assert!(cloud_resume.content["employment"][0]["highlights"][0]
            .as_str()
            .unwrap()
            .contains("AWS"));
        assert!(frontend_resume.content["employment"][0]["highlights"][0]
            .as_str()
            .unwrap()
            .contains("React"));
        assert_eq!(cloud_resume.diff["claims_added"], json!([]));
        assert_eq!(frontend_resume.diff["claims_added"], json!([]));
    }

    #[test]
    fn enhanced_resume_never_invents_an_empty_headline_or_summary() {
        let pool = test_pool();
        let mut profile = default_profile("jobs@example.com");
        profile.full_name = "Taylor Rivera".to_string();
        save_profile(&pool, "acct-jobs", &profile).unwrap();
        let posting = upsert_posting(
            &pool,
            "acct-jobs",
            &test_posting(
                "https://boards.greenhouse.io/acme/jobs/no-fabrication",
                now_ms(),
                now_ms(),
            ),
            &profile,
            &JobPreferences::default(),
        )
        .unwrap();

        let (application, resume) =
            prepare_application(&pool, "acct-jobs", &posting.id, "enhance", "review_first")
                .unwrap();

        assert_eq!(resume.content["headline"], "");
        assert_eq!(resume.content["summary"], "");
        assert_eq!(resume.diff["claims_added"], json!([]));
        let fingerprint = resume
            .content
            .pointer("/provenance/candidate_truth_fingerprint")
            .and_then(Value::as_str)
            .unwrap();
        assert_eq!(fingerprint.len(), 64);
        assert_eq!(
            application
                .receipt
                .get("candidate_truth_fingerprint")
                .and_then(Value::as_str),
            Some(fingerprint)
        );
    }

    #[test]
    fn application_email_and_track_cannot_bypass_company_guard() {
        let pool = test_pool();
        let mut profile = default_profile("jobs@example.com");
        profile.full_name = "Taylor Rivera".to_string();
        profile.headline = "Software Engineer".to_string();
        profile.summary = "Builds reliable data products.".to_string();
        profile.employment = vec![EmploymentEntry {
            id: "employment-1".to_string(),
            company: "Northstar".to_string(),
            title: "Software Engineer".to_string(),
            start_date: "2022-01".to_string(),
            current: true,
            ..EmploymentEntry::default()
        }];
        save_profile(&pool, "acct-jobs", &profile).unwrap();

        let primary =
            ensure_primary_application_identity(&pool, "acct-jobs", "jobs@example.com").unwrap();
        let data_email = save_application_identity(
            &pool,
            "acct-jobs",
            &ApplicationIdentity {
                id: String::new(),
                email: "data-jobs@example.com".to_string(),
                label: "Data applications".to_string(),
                verification_status: "verified".to_string(),
                is_default: false,
                created_at_ms: 0,
                updated_at_ms: 0,
            },
        )
        .unwrap();
        let sde_track = upsert_track(
            &pool,
            "acct-jobs",
            &CareerTrack {
                id: String::new(),
                name: "SDE".to_string(),
                role: "Software Development Engineer".to_string(),
                locations: vec!["New York, NY".to_string()],
                remote_preference: "hybrid_ok".to_string(),
                application_identity_id: Some(primary.id),
                policy: CareerTrackPolicy::default(),
                active: true,
                match_count: 0,
                created_at_ms: 0,
                updated_at_ms: 0,
            },
        )
        .unwrap();
        let data_track = upsert_track(
            &pool,
            "acct-jobs",
            &CareerTrack {
                id: String::new(),
                name: "Data Engineering".to_string(),
                role: "Data Engineer".to_string(),
                locations: vec!["New York, NY".to_string()],
                remote_preference: "hybrid_ok".to_string(),
                application_identity_id: Some(data_email.id),
                policy: CareerTrackPolicy {
                    role_family: "data_engineering".to_string(),
                    ..CareerTrackPolicy::default()
                },
                active: true,
                match_count: 0,
                created_at_ms: 0,
                updated_at_ms: 0,
            },
        )
        .unwrap();

        let unsafe_preferences = JobPreferences {
            apply_once_per_company: false,
            daily_limit: 10,
            ..JobPreferences::default()
        };
        let saved_preferences = save_preferences(&pool, "acct-jobs", &unsafe_preferences).unwrap();
        assert!(saved_preferences.apply_once_per_company);

        let mut sde_job = test_posting(
            "https://boards.greenhouse.io/acme/jobs/sde",
            now_ms(),
            now_ms(),
        );
        sde_job.company = "Acme, Inc.".to_string();
        sde_job.title = "Software Development Engineer".to_string();
        sde_job.track_id = sde_track.id;
        let sde_job =
            upsert_posting(&pool, "acct-jobs", &sde_job, &profile, &saved_preferences).unwrap();
        let (sde_application, _) =
            prepare_application(&pool, "acct-jobs", &sde_job.id, "factual", "review_first")
                .unwrap();
        reserve_application_attempt(&pool, "acct-jobs", &sde_application.id, "local").unwrap();
        update_attempt_reservation_status(&pool, "acct-jobs", &sde_application.id, "submitted")
            .unwrap();

        let mut data_job = test_posting(
            "https://boards.greenhouse.io/acme/jobs/data-engineer",
            now_ms(),
            now_ms(),
        );
        data_job.company = "The Acme LLC".to_string();
        data_job.title = "Data Engineer".to_string();
        data_job.track_id = data_track.id;
        let data_job =
            upsert_posting(&pool, "acct-jobs", &data_job, &profile, &saved_preferences).unwrap();

        let decision = data_job.eligibility.as_ref().unwrap();
        assert!(!decision.can_prepare);
        assert!(!decision.can_auto_submit);
        assert!(decision
            .hard_failures
            .iter()
            .any(|reason| reason.code == "company_application_exists"));
        let error = prepare_application(&pool, "acct-jobs", &data_job.id, "enhance", "auto_submit")
            .unwrap_err();
        assert!(error
            .to_string()
            .contains("does not create a second candidate"));
        assert_eq!(
            normalize_company_key("Acme, Inc."),
            normalize_company_key("The Acme LLC")
        );
    }

    #[test]
    fn candidate_truth_fingerprint_ignores_only_non_fact_profile_fields() {
        let mut profile = default_profile("jobs@example.com");
        profile.full_name = "Taylor Rivera".to_string();
        profile.summary = "Builds reliable distributed systems.".to_string();
        profile.skills = vec!["Rust".to_string()];
        profile.employment = vec![EmploymentEntry {
            company: "Northstar".to_string(),
            title: "Software Engineer".to_string(),
            start_date: "2022-01".to_string(),
            current: true,
            highlights: vec!["Reduced p99 latency by 30%.".to_string()],
            ..EmploymentEntry::default()
        }];
        let baseline = candidate_truth_fingerprint(&profile);

        profile.email = "another-alias@example.com".to_string();
        profile.updated_at_ms += 1;
        assert_eq!(candidate_truth_fingerprint(&profile), baseline);

        profile.summary = "Builds reliable payment systems.".to_string();
        assert_ne!(candidate_truth_fingerprint(&profile), baseline);
        profile.summary = "Builds reliable distributed systems.".to_string();

        profile.employment[0].highlights[0] = "Reduced p99 latency by 50%.".to_string();
        assert_ne!(candidate_truth_fingerprint(&profile), baseline);
        profile.employment[0].highlights[0] = "Reduced p99 latency by 30%.".to_string();

        profile.employment[0].title = "Data Engineer".to_string();
        assert_ne!(candidate_truth_fingerprint(&profile), baseline);
    }

    #[test]
    fn resume_contact_email_never_bootstraps_a_verified_application_identity() {
        let pool = test_pool();
        let mut profile = default_profile("resume-contact@example.com");
        profile.full_name = "Taylor Rivera".to_string();
        profile.headline = "Software Engineer".to_string();
        save_profile(&pool, "acct-jobs", &profile).unwrap();

        let posting = upsert_posting(
            &pool,
            "acct-jobs",
            &test_posting(
                "https://boards.greenhouse.io/acme/jobs/contact-email",
                now_ms(),
                now_ms(),
            ),
            &profile,
            &JobPreferences::default(),
        )
        .unwrap();
        let (application, resume) =
            prepare_application(&pool, "acct-jobs", &posting.id, "factual", "review_first")
                .unwrap();

        assert_eq!(
            get_profile(&pool, "acct-jobs", "").unwrap().email,
            "resume-contact@example.com"
        );
        let identities = list_application_identities(&pool, "acct-jobs").unwrap();
        assert_eq!(identities.len(), 1);
        assert_eq!(identities[0].email, "jobs@example.com");
        assert_eq!(identities[0].verification_status, "verified");
        assert_eq!(resume.content["contact"]["email"], "jobs@example.com");
        assert_eq!(
            application.receipt["application_identity"]["email"],
            "jobs@example.com"
        );
    }

    #[test]
    fn packet_metering_only_counts_a_job_once() {
        let pool = test_pool();
        let profile = default_profile("jobs@example.com");
        save_profile(&pool, "acct-jobs", &profile).unwrap();
        let posting = upsert_posting(
            &pool,
            "acct-jobs",
            &JobPosting {
                id: String::new(),
                canonical_key: String::new(),
                source: "pasted_link".to_string(),
                external_id: String::new(),
                company: "Acme".to_string(),
                title: "Engineer".to_string(),
                location: "Remote".to_string(),
                workplace: "remote".to_string(),
                canonical_url: "https://example.com/jobs/1".to_string(),
                description: String::new(),
                compensation: String::new(),
                employment_type: String::new(),
                track_id: "track-default".to_string(),
                match_score: 80,
                matched_reasons: Vec::new(),
                missing_requirements: Vec::new(),
                posted_at_ms: Some(now_ms()),
                last_verified_at_ms: Some(now_ms()),
                availability_status: "active".to_string(),
                status: "matched".to_string(),
                created_at_ms: 0,
                updated_at_ms: 0,
                eligibility: None,
            },
            &profile,
            &JobPreferences::default(),
        )
        .unwrap();
        let (application, _) =
            prepare_application(&pool, "acct-jobs", &posting.id, "factual", "review_first")
                .unwrap();
        let first = commit_packet(&pool, "acct-jobs", &application.id).unwrap();
        let second = commit_packet(&pool, "acct-jobs", &application.id).unwrap();
        assert!(first.newly_metered);
        assert!(!second.newly_metered);
        assert_eq!(first.used_packets, second.used_packets);
    }

    #[test]
    fn packet_commit_reuses_the_same_period_generation_allowance() {
        let pool = test_pool();
        let profile = default_profile("jobs@example.com");
        save_profile(&pool, "acct-jobs", &profile).unwrap();
        let posting = upsert_posting(
            &pool,
            "acct-jobs",
            &test_posting(
                "https://boards.greenhouse.io/acme/jobs/same-period",
                now_ms(),
                now_ms(),
            ),
            &profile,
            &JobPreferences::default(),
        )
        .unwrap();
        let (application, _) =
            prepare_application(&pool, "acct-jobs", &posting.id, "factual", "review_first")
                .unwrap();
        let generation_key = "generation-same-period";
        let crate::db::jobs_generation::ResumeGenerationReservation::Start(generation) =
            crate::db::jobs_generation::reserve(
                &pool,
                "acct-jobs",
                &posting.id,
                generation_key,
            )
            .unwrap()
        else {
            panic!("generation reservation must start")
        };
        assert_eq!(
            crate::db::jobs_generation_allowance::reserve(
                &pool,
                "acct-jobs",
                &posting.id,
                generation_key,
                &generation.reservation_token,
            )
            .unwrap(),
            crate::db::jobs_generation_allowance::AllowanceReservation::Reserved
        );

        let committed = commit_packet(&pool, "acct-jobs", &application.id).unwrap();
        assert!(committed.newly_metered);
        assert!(committed.included);
        assert_eq!(committed.amount_cents, 0);
        assert_eq!(committed.used_packets, 1);

        let (used_packets, allowance_status): (i64, String) = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT e.used_packets, r.status
                   FROM jobs_entitlements e
                   JOIN jobs_generation_allowance_reservations r
                     ON r.account_id = e.account_id
                  WHERE e.account_id = ?1 AND r.job_id = ?2",
                params!["acct-jobs", posting.id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(used_packets, 1);
        assert_eq!(allowance_status, "committed");
    }

    #[test]
    fn packet_commit_after_period_rollover_counts_the_new_period_once() {
        let pool = test_pool();
        let profile = default_profile("jobs@example.com");
        save_profile(&pool, "acct-jobs", &profile).unwrap();
        let posting = upsert_posting(
            &pool,
            "acct-jobs",
            &test_posting(
                "https://boards.greenhouse.io/acme/jobs/period-rollover",
                now_ms(),
                now_ms(),
            ),
            &profile,
            &JobPreferences::default(),
        )
        .unwrap();
        let (application, _) =
            prepare_application(&pool, "acct-jobs", &posting.id, "factual", "review_first")
                .unwrap();
        let generation_key = "generation-before-rollover";
        let crate::db::jobs_generation::ResumeGenerationReservation::Start(generation) =
            crate::db::jobs_generation::reserve(
                &pool,
                "acct-jobs",
                &posting.id,
                generation_key,
            )
            .unwrap()
        else {
            panic!("generation reservation must start")
        };
        assert_eq!(
            crate::db::jobs_generation_allowance::reserve(
                &pool,
                "acct-jobs",
                &posting.id,
                generation_key,
                &generation.reservation_token,
            )
            .unwrap(),
            crate::db::jobs_generation_allowance::AllowanceReservation::Reserved
        );

        let old_period: i64 = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT period_start_ms FROM jobs_entitlements WHERE account_id = ?1",
                ["acct-jobs"],
                |row| row.get(0),
            )
            .unwrap();
        pool.get()
            .unwrap()
            .execute(
                "UPDATE jobs_entitlements
                    SET used_packets = 0, period_start_ms = ?2, period_end_ms = ?3
                  WHERE account_id = ?1",
                params!["acct-jobs", old_period + 1, now_ms() + 60_000],
            )
            .unwrap();

        let committed = commit_packet(&pool, "acct-jobs", &application.id).unwrap();
        assert!(committed.newly_metered);
        assert!(committed.included);
        assert_eq!(committed.amount_cents, 0);
        assert_eq!(committed.used_packets, 1);
        let (used_packets, allowance_status): (i64, String) = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT e.used_packets, r.status
                   FROM jobs_entitlements e
                   JOIN jobs_generation_allowance_reservations r
                     ON r.account_id = e.account_id
                  WHERE e.account_id = ?1 AND r.job_id = ?2",
                params!["acct-jobs", posting.id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(used_packets, 1);
        assert_eq!(allowance_status, "committed");
    }

    #[test]
    fn restricted_sites_never_enter_background_auto_submit() {
        let pool = test_pool();
        let mut profile = default_profile("jobs@example.com");
        profile.auto_submit_threshold = 80;
        save_profile(&pool, "acct-jobs", &profile).unwrap();
        let posting = upsert_posting(
            &pool,
            "acct-jobs",
            &JobPosting {
                id: String::new(),
                canonical_key: String::new(),
                source: "linkedin_handoff".to_string(),
                external_id: String::new(),
                company: "Northstar".to_string(),
                title: "Software Engineer".to_string(),
                location: "Remote".to_string(),
                workplace: "remote".to_string(),
                canonical_url: "https://linkedin.com/jobs/view/123".to_string(),
                description: "Distributed systems".to_string(),
                compensation: String::new(),
                employment_type: String::new(),
                track_id: "track-default".to_string(),
                match_score: 96,
                matched_reasons: Vec::new(),
                missing_requirements: Vec::new(),
                posted_at_ms: Some(now_ms()),
                last_verified_at_ms: Some(now_ms()),
                availability_status: "active".to_string(),
                status: "matched".to_string(),
                created_at_ms: 0,
                updated_at_ms: 0,
                eligibility: None,
            },
            &profile,
            &JobPreferences::default(),
        )
        .unwrap();
        let (application, _) =
            prepare_application(&pool, "acct-jobs", &posting.id, "factual", "auto_submit").unwrap();
        assert_eq!(application.state, "awaiting_review");
    }

    #[test]
    fn resume_generation_draft_is_not_runnable_or_visible_until_finalized() {
        let pool = test_pool();
        let profile = default_profile("jobs@example.com");
        save_profile(&pool, "acct-jobs", &profile).unwrap();
        let posting = upsert_posting(
            &pool,
            "acct-jobs",
            &test_posting(
                "https://boards.greenhouse.io/acme/jobs/generated-resume",
                now_ms(),
                now_ms(),
            ),
            &profile,
            &JobPreferences::default(),
        )
        .unwrap();
        let prepared =
            prepare_application_draft(&pool, "acct-jobs", &posting.id, "factual", "auto_submit")
                .unwrap();
        let draft = &prepared.application;
        let baseline = &prepared.baseline_resume;
        assert_eq!(draft.state, "preparing");
        assert!(draft.resume_version_id.is_none());
        assert!(baseline.id.is_empty());
        assert!(list_applications(&pool, "acct-jobs").unwrap().is_empty());
        assert!(list_resume_versions(&pool, "acct-jobs").unwrap().is_empty());
        assert_eq!(
            draft.receipt.pointer("/metering/status"),
            Some(&json!("pending_generation"))
        );

        let mut content = baseline.content.clone();
        content["provenance"]["resume_generation"] = json!({
            "kind": "model",
            "schema_version": 1,
            "truth_guard": "passed",
            "claims_added": 0,
        });
        let generation = content["provenance"]["resume_generation"].clone();
        let (application, resume) = finalize_prepared_application(
            &pool,
            "acct-jobs",
            &prepared,
            content.clone(),
            baseline.diff.clone(),
            generation,
        )
        .unwrap();
        assert_eq!(application.state, "awaiting_review");
        assert_eq!(
            application.resume_version_id.as_deref(),
            Some(resume.id.as_str())
        );
        assert_eq!(resume.version_no, 1);
        assert_eq!(resume.content, content);
        assert_eq!(
            application.receipt.pointer("/resume_generation/kind"),
            Some(&json!("model"))
        );
        assert_eq!(list_resume_versions(&pool, "acct-jobs").unwrap().len(), 1);
    }

    #[test]
    fn resume_generation_finalization_rejects_a_changed_truth_snapshot() {
        let pool = test_pool();
        let profile = default_profile("jobs@example.com");
        save_profile(&pool, "acct-jobs", &profile).unwrap();
        let posting = upsert_posting(
            &pool,
            "acct-jobs",
            &test_posting(
                "https://boards.greenhouse.io/acme/jobs/tampered-resume",
                now_ms(),
                now_ms(),
            ),
            &profile,
            &JobPreferences::default(),
        )
        .unwrap();
        let prepared =
            prepare_application_draft(&pool, "acct-jobs", &posting.id, "factual", "review_first")
                .unwrap();
        let draft = &prepared.application;
        let baseline = &prepared.baseline_resume;
        let mut content = baseline.content.clone();
        content["provenance"]["candidate_truth_fingerprint"] = json!("tampered");
        let error = finalize_prepared_application(
            &pool,
            "acct-jobs",
            &prepared,
            content,
            baseline.diff.clone(),
            json!({"kind": "model"}),
        )
        .unwrap_err();
        assert!(error
            .to_string()
            .contains("does not match the candidate truth snapshot"));
        assert!(get_application(&pool, "acct-jobs", &draft.id)
            .unwrap()
            .is_none());
        assert!(list_resume_versions(&pool, "acct-jobs").unwrap().is_empty());
    }

    #[test]
    fn resume_generation_finalization_rejects_a_concurrent_profile_edit() {
        let pool = test_pool();
        let mut profile = default_profile("jobs@example.com");
        profile.summary = "Builds reliable distributed systems.".to_string();
        profile.skills = vec!["Rust".to_string(), "PostgreSQL".to_string()];
        profile.employment = vec![EmploymentEntry {
            company: "Northstar".to_string(),
            title: "Software Engineer".to_string(),
            highlights: vec!["Reduced p99 latency by 30%.".to_string()],
            ..EmploymentEntry::default()
        }];
        save_profile(&pool, "acct-jobs", &profile).unwrap();
        let posting = upsert_posting(
            &pool,
            "acct-jobs",
            &test_posting(
                "https://boards.greenhouse.io/acme/jobs/stale-profile-resume",
                now_ms(),
                now_ms(),
            ),
            &profile,
            &JobPreferences::default(),
        )
        .unwrap();
        let prepared =
            prepare_application_draft(&pool, "acct-jobs", &posting.id, "factual", "review_first")
                .unwrap();

        profile.summary = "Builds high-throughput payment systems.".to_string();
        profile.skills.push("Kafka".to_string());
        profile.employment[0].highlights[0] = "Reduced p99 latency by 50%.".to_string();
        save_profile(&pool, "acct-jobs", &profile).unwrap();

        let error = finalize_prepared_application(
            &pool,
            "acct-jobs",
            &prepared,
            prepared.baseline_resume.content.clone(),
            prepared.baseline_resume.diff.clone(),
            json!({"kind": "model"}),
        )
        .unwrap_err();
        assert!(error
            .to_string()
            .contains("candidate profile changed while the application packet was generated"));
        assert!(
            get_application(&pool, "acct-jobs", &prepared.application.id)
                .unwrap()
                .is_none()
        );
        assert!(list_resume_versions(&pool, "acct-jobs").unwrap().is_empty());
    }

    #[test]
    fn resume_generation_finalization_rejects_a_changed_confirmed_fact() {
        let pool = test_pool();
        let profile = default_profile("jobs@example.com");
        save_profile(&pool, "acct-jobs", &profile).unwrap();
        let mut fact = upsert_fact(
            &pool,
            "acct-jobs",
            &CareerFact {
                id: String::new(),
                category: "achievement".to_string(),
                label: "Latency reduction".to_string(),
                value: json!({"metric": "30%", "system": "checkout"}),
                source: "user_entry".to_string(),
                verification_status: "confirmed".to_string(),
                confirmed_at_ms: None,
                confirmed_by: None,
                schema_version: 1,
                created_at_ms: 0,
                updated_at_ms: 0,
            },
        )
        .unwrap();
        let posting = upsert_posting(
            &pool,
            "acct-jobs",
            &test_posting(
                "https://boards.greenhouse.io/acme/jobs/stale-fact-resume",
                now_ms(),
                now_ms(),
            ),
            &profile,
            &JobPreferences::default(),
        )
        .unwrap();
        let prepared =
            prepare_application_draft(&pool, "acct-jobs", &posting.id, "factual", "review_first")
                .unwrap();

        fact.value = json!({"metric": "50%", "system": "checkout"});
        upsert_fact(&pool, "acct-jobs", &fact).unwrap();

        let error = finalize_prepared_application(
            &pool,
            "acct-jobs",
            &prepared,
            prepared.baseline_resume.content.clone(),
            prepared.baseline_resume.diff.clone(),
            json!({"kind": "model"}),
        )
        .unwrap_err();
        assert!(error
            .to_string()
            .contains("confirmed candidate facts changed during resume generation"));
        assert!(list_applications(&pool, "acct-jobs").unwrap().is_empty());
        assert!(list_resume_versions(&pool, "acct-jobs").unwrap().is_empty());
    }

    #[test]
    fn resume_generation_finalization_rejects_a_new_confirmed_fact() {
        let pool = test_pool();
        let profile = default_profile("jobs@example.com");
        save_profile(&pool, "acct-jobs", &profile).unwrap();
        let posting = upsert_posting(
            &pool,
            "acct-jobs",
            &test_posting(
                "https://boards.greenhouse.io/acme/jobs/new-fact-resume",
                now_ms(),
                now_ms(),
            ),
            &profile,
            &JobPreferences::default(),
        )
        .unwrap();
        let prepared =
            prepare_application_draft(&pool, "acct-jobs", &posting.id, "factual", "review_first")
                .unwrap();

        upsert_fact(
            &pool,
            "acct-jobs",
            &CareerFact {
                id: String::new(),
                category: "achievement".to_string(),
                label: "Newly confirmed reliability result".to_string(),
                value: json!({"metric": "99.99%", "system": "payments"}),
                source: "user_entry".to_string(),
                verification_status: "confirmed".to_string(),
                confirmed_at_ms: None,
                confirmed_by: None,
                schema_version: 1,
                created_at_ms: 0,
                updated_at_ms: 0,
            },
        )
        .unwrap();

        let error = finalize_prepared_application(
            &pool,
            "acct-jobs",
            &prepared,
            prepared.baseline_resume.content.clone(),
            prepared.baseline_resume.diff.clone(),
            json!({"kind": "model"}),
        )
        .unwrap_err();
        assert!(error
            .to_string()
            .contains("confirmed candidate facts changed during resume generation"));
        assert!(list_applications(&pool, "acct-jobs").unwrap().is_empty());
        assert!(list_resume_versions(&pool, "acct-jobs").unwrap().is_empty());
    }

    #[test]
    fn resume_generation_finalization_rejects_a_changed_track_identity() {
        let pool = test_pool();
        let profile = default_profile("jobs@example.com");
        save_profile(&pool, "acct-jobs", &profile).unwrap();
        let primary =
            ensure_primary_application_identity(&pool, "acct-jobs", "jobs@example.com").unwrap();
        let alternate = save_application_identity(
            &pool,
            "acct-jobs",
            &ApplicationIdentity {
                id: String::new(),
                email: "applications@example.com".to_string(),
                label: "Applications".to_string(),
                verification_status: "pending".to_string(),
                is_default: false,
                created_at_ms: 0,
                updated_at_ms: 0,
            },
        )
        .unwrap();
        save_identity_verification(&pool, "acct-jobs", &alternate.id, "602314", 60_000).unwrap();
        let alternate =
            verify_application_identity(&pool, "acct-jobs", &alternate.id, "602314").unwrap();
        let mut track = upsert_track(
            &pool,
            "acct-jobs",
            &CareerTrack {
                id: String::new(),
                name: "Engineering".to_string(),
                role: "Software Engineer".to_string(),
                locations: Vec::new(),
                remote_preference: "hybrid_ok".to_string(),
                application_identity_id: Some(primary.id),
                policy: CareerTrackPolicy::default(),
                active: true,
                match_count: 0,
                created_at_ms: 0,
                updated_at_ms: 0,
            },
        )
        .unwrap();
        let mut posting = test_posting(
            "https://boards.greenhouse.io/acme/jobs/stale-track-identity",
            now_ms(),
            now_ms(),
        );
        posting.track_id = track.id.clone();
        let posting = upsert_posting(
            &pool,
            "acct-jobs",
            &posting,
            &profile,
            &JobPreferences::default(),
        )
        .unwrap();
        let prepared =
            prepare_application_draft(&pool, "acct-jobs", &posting.id, "factual", "review_first")
                .unwrap();

        track.application_identity_id = Some(alternate.id);
        upsert_track(&pool, "acct-jobs", &track).unwrap();

        let error = finalize_prepared_application(
            &pool,
            "acct-jobs",
            &prepared,
            prepared.baseline_resume.content.clone(),
            prepared.baseline_resume.diff.clone(),
            json!({"kind": "model"}),
        )
        .unwrap_err();
        assert!(error
            .to_string()
            .contains("verified application identity changed during generation"));
        assert!(list_applications(&pool, "acct-jobs").unwrap().is_empty());
        assert!(list_resume_versions(&pool, "acct-jobs").unwrap().is_empty());
    }

    #[test]
    fn cancelled_generation_preserves_the_prior_review_packet() {
        let pool = test_pool();
        let profile = default_profile("jobs@example.com");
        save_profile(&pool, "acct-jobs", &profile).unwrap();
        let posting = upsert_posting(
            &pool,
            "acct-jobs",
            &test_posting(
                "https://boards.greenhouse.io/acme/jobs/preserved-packet",
                now_ms(),
                now_ms(),
            ),
            &profile,
            &JobPreferences::default(),
        )
        .unwrap();
        let (prior, prior_resume) =
            prepare_application(&pool, "acct-jobs", &posting.id, "factual", "review_first")
                .unwrap();
        assert_eq!(prior.state, "awaiting_review");

        let pending =
            prepare_application_draft(&pool, "acct-jobs", &posting.id, "enhance", "review_first")
                .unwrap();
        assert_eq!(pending.application.state, "preparing");
        drop(pending); // Request cancellation/restart before generation finishes.

        let current = get_application(&pool, "acct-jobs", &prior.id)
            .unwrap()
            .unwrap();
        assert_eq!(
            serde_json::to_value(&current).unwrap(),
            serde_json::to_value(&prior).unwrap(),
            "cancellation must not mutate any part of the committed packet"
        );
        assert_eq!(current.state, "awaiting_review");
        assert_eq!(current.resume_version_id, Some(prior_resume.id));
        assert_eq!(current.receipt, prior.receipt);
        assert_eq!(list_resume_versions(&pool, "acct-jobs").unwrap().len(), 1);
    }

    #[test]
    fn legacy_embedded_application_identity_is_normalized_to_authoritative_columns() {
        let pool = test_pool();
        let profile = default_profile("jobs@example.com");
        save_profile(&pool, "acct-jobs", &profile).unwrap();
        let posting = upsert_posting(
            &pool,
            "acct-jobs",
            &test_posting(
                "https://boards.greenhouse.io/acme/jobs/legacy-embedded-identity",
                now_ms(),
                now_ms(),
            ),
            &profile,
            &JobPreferences::default(),
        )
        .unwrap();
        let (prior, _) =
            prepare_application(&pool, "acct-jobs", &posting.id, "factual", "review_first")
                .unwrap();
        let mut legacy_payload = prior.clone();
        legacy_payload.id = "payload-only-id".into();
        legacy_payload.job_id = "payload-only-job".into();
        pool.get()
            .unwrap()
            .execute(
                "UPDATE jobs_applications SET application_json = ?3 \
                 WHERE account_id = ?1 AND id = ?2",
                params![
                    "acct-jobs",
                    prior.id,
                    serde_json::to_string(&legacy_payload).unwrap()
                ],
            )
            .unwrap();

        let prepared =
            prepare_application_draft(&pool, "acct-jobs", &posting.id, "factual", "review_first")
                .unwrap();
        assert_eq!(prepared.application.id, prior.id);
        assert_eq!(prepared.application.job_id, posting.id);

        let mut content = prepared.baseline_resume.content.clone();
        content["provenance"]["resume_generation"] = json!({
            "kind": "deterministic_fallback",
            "schema_version": 2,
            "truth_guard": "deterministic",
            "claims_added": 0,
        });
        let generation = content["provenance"]["resume_generation"].clone();
        let (application, resume) = finalize_prepared_application(
            &pool,
            "acct-jobs",
            &prepared,
            content,
            prepared.baseline_resume.diff.clone(),
            generation,
        )
        .unwrap();
        assert_eq!(application.id, prior.id);
        assert_eq!(application.job_id, posting.id);
        assert_eq!(resume.job_id, posting.id);
        assert!(get_application(&pool, "acct-jobs", "payload-only-id")
            .unwrap()
            .is_none());
    }

    #[test]
    fn concurrent_prepare_finalization_is_cas_fenced_and_atomic() {
        let pool = test_pool();
        let profile = default_profile("jobs@example.com");
        save_profile(&pool, "acct-jobs", &profile).unwrap();
        let posting = upsert_posting(
            &pool,
            "acct-jobs",
            &test_posting(
                "https://boards.greenhouse.io/acme/jobs/concurrent-generation",
                now_ms(),
                now_ms(),
            ),
            &profile,
            &JobPreferences::default(),
        )
        .unwrap();
        let (prior, _) =
            prepare_application(&pool, "acct-jobs", &posting.id, "factual", "review_first")
                .unwrap();
        let first =
            prepare_application_draft(&pool, "acct-jobs", &posting.id, "factual", "review_first")
                .unwrap();
        let second =
            prepare_application_draft(&pool, "acct-jobs", &posting.id, "factual", "review_first")
                .unwrap();

        let mut first_content = first.baseline_resume.content.clone();
        first_content["provenance"]["resume_generation"] = json!({"kind":"model","attempt":1});
        let (winner, winner_resume) = finalize_prepared_application(
            &pool,
            "acct-jobs",
            &first,
            first_content,
            first.baseline_resume.diff.clone(),
            json!({"kind":"model","attempt":1}),
        )
        .unwrap();
        let resume_count_after_winner = list_resume_versions(&pool, "acct-jobs").unwrap().len();

        let mut stale_content = second.baseline_resume.content.clone();
        stale_content["provenance"]["resume_generation"] = json!({"kind":"model","attempt":2});
        let error = finalize_prepared_application(
            &pool,
            "acct-jobs",
            &second,
            stale_content,
            second.baseline_resume.diff.clone(),
            json!({"kind":"model","attempt":2}),
        )
        .unwrap_err();
        assert!(error
            .to_string()
            .contains("application changed while resume generation was in progress"));
        assert_eq!(
            list_resume_versions(&pool, "acct-jobs").unwrap().len(),
            resume_count_after_winner,
            "the losing CAS must roll back its resume insert"
        );
        let current = get_application(&pool, "acct-jobs", &prior.id)
            .unwrap()
            .unwrap();
        assert_eq!(current.id, winner.id);
        assert_eq!(current.resume_version_id, Some(winner_resume.id));
        assert_eq!(
            current.receipt.pointer("/resume_generation/attempt"),
            Some(&json!(1))
        );
    }

    #[test]
    fn duplicate_first_prepare_creates_only_one_visible_packet() {
        let pool = test_pool();
        let profile = default_profile("jobs@example.com");
        save_profile(&pool, "acct-jobs", &profile).unwrap();
        let posting = upsert_posting(
            &pool,
            "acct-jobs",
            &test_posting(
                "https://boards.greenhouse.io/acme/jobs/first-packet-race",
                now_ms(),
                now_ms(),
            ),
            &profile,
            &JobPreferences::default(),
        )
        .unwrap();
        let first =
            prepare_application_draft(&pool, "acct-jobs", &posting.id, "factual", "review_first")
                .unwrap();
        let second =
            prepare_application_draft(&pool, "acct-jobs", &posting.id, "factual", "review_first")
                .unwrap();
        assert!(list_applications(&pool, "acct-jobs").unwrap().is_empty());

        let (winner, _) = finalize_prepared_application(
            &pool,
            "acct-jobs",
            &first,
            first.baseline_resume.content.clone(),
            first.baseline_resume.diff.clone(),
            json!({"kind":"deterministic"}),
        )
        .unwrap();
        let error = finalize_prepared_application(
            &pool,
            "acct-jobs",
            &second,
            second.baseline_resume.content.clone(),
            second.baseline_resume.diff.clone(),
            json!({"kind":"deterministic"}),
        )
        .unwrap_err();
        assert!(error
            .to_string()
            .contains("application changed while resume generation was in progress"));
        assert_eq!(list_applications(&pool, "acct-jobs").unwrap().len(), 1);
        assert_eq!(list_resume_versions(&pool, "acct-jobs").unwrap().len(), 1);
        assert_eq!(
            get_application(&pool, "acct-jobs", &winner.id)
                .unwrap()
                .unwrap()
                .id,
            winner.id
        );
    }

    #[test]
    fn unknown_public_sites_never_enter_background_auto_submit() {
        let pool = test_pool();
        let mut profile = default_profile("jobs@example.com");
        profile.auto_submit_threshold = 80;
        save_profile(&pool, "acct-jobs", &profile).unwrap();
        let mut posting = test_posting("https://jobs.acme.com/openings/123", now_ms(), now_ms());
        posting.source = "semantic".to_string();
        posting.match_score = 98;
        let posting = upsert_posting(
            &pool,
            "acct-jobs",
            &posting,
            &profile,
            &JobPreferences::default(),
        )
        .unwrap();
        let (application, _) =
            prepare_application(&pool, "acct-jobs", &posting.id, "factual", "auto_submit").unwrap();
        assert_eq!(application.state, "awaiting_review");
    }

    #[test]
    fn source_suffix_cannot_self_certify_a_site() {
        let pool = test_pool();
        let mut profile = default_profile("jobs@example.com");
        profile.auto_submit_threshold = 80;
        save_profile(&pool, "acct-jobs", &profile).unwrap();
        let preferences = JobPreferences {
            sponsorship: "not_required".to_string(),
            ..JobPreferences::default()
        };
        save_preferences(&pool, "acct-jobs", &preferences).unwrap();
        let mut posting = test_posting(
            "https://boards.greenhouse.io/acme/jobs/certified",
            now_ms(),
            now_ms(),
        );
        posting.source = "greenhouse_certified".to_string();
        posting.match_score = 98;
        let posting = upsert_posting(&pool, "acct-jobs", &posting, &profile, &preferences).unwrap();
        let (application, _) =
            prepare_application(&pool, "acct-jobs", &posting.id, "factual", "auto_submit").unwrap();
        assert_eq!(application.state, "awaiting_review");
        assert_eq!(
            application.receipt.pointer("/eligibility/can_auto_submit"),
            Some(&json!(false))
        );
        assert_eq!(
            application.receipt.pointer("/eligibility/capability"),
            Some(&json!("beta_review"))
        );
    }

    #[test]
    fn hard_filters_block_ineligible_packets() {
        let pool = test_pool();
        let profile = default_profile("jobs@example.com");
        save_profile(&pool, "acct-jobs", &profile).unwrap();
        let mut preferences = JobPreferences {
            excluded_companies: vec!["Acme".to_string()],
            excluded_titles: vec!["Intern".to_string()],
            minimum_compensation: Some(150_000),
            employment_types: vec!["full_time".to_string()],
            sponsorship: "required".to_string(),
            ..JobPreferences::default()
        };
        save_preferences(&pool, "acct-jobs", &preferences).unwrap();

        let excluded_company = upsert_posting(
            &pool,
            "acct-jobs",
            &test_posting(
                "https://boards.greenhouse.io/acme/jobs/excluded",
                now_ms(),
                now_ms(),
            ),
            &profile,
            &preferences,
        )
        .unwrap();
        assert!(prepare_application(
            &pool,
            "acct-jobs",
            &excluded_company.id,
            "factual",
            "review_first"
        )
        .unwrap_err()
        .to_string()
        .contains("company is excluded"));

        preferences.excluded_companies.clear();
        save_preferences(&pool, "acct-jobs", &preferences).unwrap();
        let mut excluded_title = test_posting(
            "https://boards.greenhouse.io/acme/jobs/intern",
            now_ms(),
            now_ms(),
        );
        excluded_title.title = "Software Engineer Intern".to_string();
        let excluded_title =
            upsert_posting(&pool, "acct-jobs", &excluded_title, &profile, &preferences).unwrap();
        assert!(prepare_application(
            &pool,
            "acct-jobs",
            &excluded_title.id,
            "factual",
            "review_first"
        )
        .unwrap_err()
        .to_string()
        .contains("title is excluded"));

        preferences.excluded_titles.clear();
        save_preferences(&pool, "acct-jobs", &preferences).unwrap();
        let mut low_salary = test_posting(
            "https://boards.greenhouse.io/acme/jobs/salary",
            now_ms(),
            now_ms(),
        );
        low_salary.compensation = "$90k-$120k".to_string();
        let low_salary =
            upsert_posting(&pool, "acct-jobs", &low_salary, &profile, &preferences).unwrap();
        assert!(prepare_application(
            &pool,
            "acct-jobs",
            &low_salary.id,
            "factual",
            "review_first"
        )
        .unwrap_err()
        .to_string()
        .contains("minimum compensation"));

        preferences.minimum_compensation = None;
        save_preferences(&pool, "acct-jobs", &preferences).unwrap();
        let mut contract = test_posting(
            "https://boards.greenhouse.io/acme/jobs/contract",
            now_ms(),
            now_ms(),
        );
        contract.title = "Software Engineer".to_string();
        contract.employment_type = "contract".to_string();
        let contract =
            upsert_posting(&pool, "acct-jobs", &contract, &profile, &preferences).unwrap();
        assert!(
            prepare_application(&pool, "acct-jobs", &contract.id, "factual", "review_first")
                .unwrap_err()
                .to_string()
                .contains("employment type")
        );

        preferences.employment_types = vec!["contract".to_string(), "full_time".to_string()];
        save_preferences(&pool, "acct-jobs", &preferences).unwrap();
        let mut sponsorship = test_posting(
            "https://boards.greenhouse.io/acme/jobs/sponsor",
            now_ms(),
            now_ms(),
        );
        sponsorship.description = "We are unable to sponsor visas.".to_string();
        let sponsorship =
            upsert_posting(&pool, "acct-jobs", &sponsorship, &profile, &preferences).unwrap();
        let sponsorship_error = prepare_application(
            &pool,
            "acct-jobs",
            &sponsorship.id,
            "factual",
            "review_first",
        )
        .unwrap_err()
        .to_string();
        assert!(
            sponsorship_error.contains("sponsorship"),
            "unexpected hard-filter reason: {sponsorship_error}"
        );

        let mut sponsorship_available = test_posting(
            "https://jobs.lever.co/ifm-us/1454349c-eb2b-480b-9a57-edfbb2aeeffe",
            now_ms(),
            now_ms(),
        );
        sponsorship_available.source = "lever_import".to_string();
        sponsorship_available.description =
            "Visa Sponsorship\nThis position is eligible for visa sponsorship.".to_string();
        sponsorship_available.compensation = "USD 150000-450000 per-year-salary".to_string();
        let sponsorship_available = upsert_posting(
            &pool,
            "acct-jobs",
            &sponsorship_available,
            &profile,
            &preferences,
        )
        .unwrap();
        let eligibility =
            evaluate_job_eligibility(&pool, "acct-jobs", &sponsorship_available, true, None)
                .unwrap();
        assert!(eligibility
            .passed_checks
            .contains(&"sponsorship_available".to_string()));
        assert!(!eligibility
            .review_reasons
            .iter()
            .any(|reason| reason.code.starts_with("sponsorship_")));
    }

    #[test]
    fn engagement_preferences_block_known_mismatches() {
        let pool = test_pool();
        let profile = default_profile("jobs@example.com");
        save_profile(&pool, "acct-jobs", &profile).unwrap();
        let preferences = JobPreferences {
            employment_types: vec!["contract".to_string()],
            engagement_types: vec!["w2".to_string()],
            sponsorship: "not_required".to_string(),
            ..JobPreferences::default()
        };
        save_preferences(&pool, "acct-jobs", &preferences).unwrap();
        let mut posting = test_posting(
            "https://boards.greenhouse.io/acme/jobs/c2c-contract",
            now_ms(),
            now_ms(),
        );
        posting.employment_type = "contract c2c".to_string();
        let posting = upsert_posting(&pool, "acct-jobs", &posting, &profile, &preferences).unwrap();

        let decision = evaluate_job_eligibility(&pool, "acct-jobs", &posting, true, None).unwrap();
        assert!(!decision.can_prepare);
        assert!(decision
            .hard_failures
            .iter()
            .any(|reason| reason.code == "engagement_type_mismatch"));
    }

    #[test]
    fn unknown_contract_engagement_requires_review_instead_of_guessing() {
        let pool = test_pool();
        let profile = default_profile("jobs@example.com");
        save_profile(&pool, "acct-jobs", &profile).unwrap();
        let mut track = list_tracks(&pool, "acct-jobs").unwrap().remove(0);
        track.policy.employment_types = vec!["contract".to_string()];
        track.policy.engagement_types = vec!["w2".to_string()];
        upsert_track(&pool, "acct-jobs", &track).unwrap();
        let preferences = JobPreferences {
            employment_types: vec!["contract".to_string()],
            engagement_types: vec!["w2".to_string()],
            sponsorship: "not_required".to_string(),
            ..JobPreferences::default()
        };
        save_preferences(&pool, "acct-jobs", &preferences).unwrap();
        let mut posting = test_posting(
            "https://boards.greenhouse.io/acme/jobs/unknown-contract",
            now_ms(),
            now_ms(),
        );
        posting.employment_type = "contract".to_string();
        let posting = upsert_posting(&pool, "acct-jobs", &posting, &profile, &preferences).unwrap();

        let decision = evaluate_job_eligibility(&pool, "acct-jobs", &posting, true, None).unwrap();
        assert!(decision.can_prepare, "unexpected decision: {decision:#?}");
        assert!(!decision.can_auto_submit);
        assert!(decision
            .review_reasons
            .iter()
            .any(|reason| reason.code == "engagement_type_unverified"));
    }

    #[test]
    fn scheduled_discovery_accepts_only_canonical_job_categories() {
        for employment_type in [
            "",
            "full_time",
            "part_time",
            "contract",
            "temporary",
            "internship",
            "apprenticeship",
            "seasonal",
            "per_diem",
        ] {
            assert!(is_canonical_discovered_employment_type(employment_type));
        }
        for engagement_type in ["", "w2", "c2c", "1099", "direct_hire"] {
            assert!(is_canonical_discovered_engagement_type(engagement_type));
        }
        for invalid in ["full time", "intern", "permanent", "freelance"] {
            assert!(!is_canonical_discovered_employment_type(invalid));
        }
        for invalid in ["W-2", "corp-to-corp", "independent_contractor"] {
            assert!(!is_canonical_discovered_engagement_type(invalid));
        }
    }

    #[test]
    fn stale_imported_job_cannot_prepare_or_enter_a_runner() {
        let pool = test_pool();
        let profile = default_profile("jobs@example.com");
        save_profile(&pool, "acct-jobs", &profile).unwrap();
        let preferences = JobPreferences {
            max_posting_age_days: 14,
            sponsorship: "not_required".to_string(),
            ..JobPreferences::default()
        };
        save_preferences(&pool, "acct-jobs", &preferences).unwrap();
        let mut stale = test_posting(
            "https://jobs.lever.co/acme/stale-job",
            now_ms() - (45 * DAY_MS),
            now_ms(),
        );
        stale.source = "lever_import".to_string();
        let stale = upsert_posting(&pool, "acct-jobs", &stale, &profile, &preferences).unwrap();

        let decision = evaluate_job_eligibility(&pool, "acct-jobs", &stale, true, None).unwrap();
        assert!(!decision.can_prepare);
        assert!(!decision.can_queue_local);
        assert!(!decision.can_queue_cloud);
        assert!(decision
            .hard_failures
            .iter()
            .any(|reason| reason.code == "job_too_old"));
        assert!(
            prepare_application(&pool, "acct-jobs", &stale.id, "factual", "review_first")
                .unwrap_err()
                .to_string()
                .contains("Posted 45 days ago")
        );
    }

    #[test]
    fn shared_eligibility_enforces_location_and_survives_into_receipt() {
        let pool = test_pool();
        let mut profile = default_profile("jobs@example.com");
        profile.current_location = "New York, NY".to_string();
        profile.summary = "Builds reliable products.".to_string();
        profile.skills = vec!["Rust".to_string(), "TypeScript".to_string()];
        profile.auto_submit_threshold = 80;
        save_profile(&pool, "acct-jobs", &profile).unwrap();

        let preferences = JobPreferences {
            desired_locations: vec!["New York, NY".to_string()],
            location_policy: "local".to_string(),
            remote_preference: "remote_or_hybrid".to_string(),
            sponsorship: "not_required".to_string(),
            ..JobPreferences::default()
        };
        save_preferences(&pool, "acct-jobs", &preferences).unwrap();

        let mut blocked = test_posting(
            "https://boards.greenhouse.io/acme/jobs/sf-onsite",
            now_ms(),
            now_ms(),
        );
        blocked.location = "San Francisco, CA".to_string();
        blocked.workplace = "on-site".to_string();
        let blocked = upsert_posting(&pool, "acct-jobs", &blocked, &profile, &preferences).unwrap();
        let blocked_decision =
            evaluate_job_eligibility(&pool, "acct-jobs", &blocked, true, None).unwrap();
        assert!(!blocked_decision.can_prepare);
        assert!(blocked_decision
            .hard_failures
            .iter()
            .any(|reason| reason.code == "location_mismatch"));

        let allowed = test_posting(
            "https://boards.greenhouse.io/acme/jobs/ny-hybrid",
            now_ms(),
            now_ms(),
        );
        let allowed = upsert_posting(&pool, "acct-jobs", &allowed, &profile, &preferences).unwrap();
        let (application, resume) =
            prepare_application(&pool, "acct-jobs", &allowed.id, "enhance", "auto_submit").unwrap();
        assert_eq!(application.state, "awaiting_review");
        assert_eq!(
            application.receipt.pointer("/eligibility/capability"),
            Some(&json!("beta_review"))
        );
        assert_eq!(
            resume.diff.pointer("/summary/before"),
            Some(&json!("Builds reliable products."))
        );
        assert!(resume.diff.pointer("/summary/after").is_some());

        let workspace = workspace(&pool, "acct-jobs", "jobs@example.com").unwrap();
        let workspace_allowed = workspace
            .matches
            .iter()
            .find(|posting| posting.id == allowed.id)
            .unwrap();
        assert_eq!(
            workspace_allowed
                .eligibility
                .as_ref()
                .map(|decision| decision.capability.as_str()),
            Some("beta_review")
        );
    }

    #[test]
    fn attempt_reservations_enforce_company_and_daily_limits_atomically() {
        let pool = test_pool();
        let profile = default_profile("jobs@example.com");
        save_profile(&pool, "acct-jobs", &profile).unwrap();
        let preferences = JobPreferences::default();
        save_preferences(&pool, "acct-jobs", &preferences).unwrap();

        let first = upsert_posting(
            &pool,
            "acct-jobs",
            &test_posting(
                "https://boards.greenhouse.io/acme/jobs/one",
                now_ms(),
                now_ms(),
            ),
            &profile,
            &preferences,
        )
        .unwrap();
        let (application, _) =
            prepare_application(&pool, "acct-jobs", &first.id, "factual", "review_first").unwrap();
        assert_eq!(application.state, "awaiting_review");
        reserve_application_attempt(&pool, "acct-jobs", &application.id, "local").unwrap();

        let mut second = test_posting(
            "https://boards.greenhouse.io/acme/jobs/two",
            now_ms(),
            now_ms(),
        );
        second.title = "Backend Engineer".to_string();
        let second = upsert_posting(&pool, "acct-jobs", &second, &profile, &preferences).unwrap();
        assert!(
            prepare_application(&pool, "acct-jobs", &second.id, "factual", "review_first")
                .unwrap_err()
                .to_string()
                .contains("does not create a second candidate")
        );

        for index in 2..=10 {
            let mut posting = test_posting(
                &format!("https://boards.greenhouse.io/company-{index}/jobs/{index}"),
                now_ms(),
                now_ms(),
            );
            posting.company = format!("Company {index}");
            posting.title = format!("Platform Engineer {index}");
            let posting =
                upsert_posting(&pool, "acct-jobs", &posting, &profile, &preferences).unwrap();
            let (application, _) =
                prepare_application(&pool, "acct-jobs", &posting.id, "factual", "review_first")
                    .unwrap();
            reserve_application_attempt(&pool, "acct-jobs", &application.id, "local").unwrap();
        }

        let mut final_posting = test_posting(
            "https://boards.greenhouse.io/globex/jobs/final",
            now_ms(),
            now_ms(),
        );
        final_posting.company = "Globex".to_string();
        final_posting.title = "Platform Engineer".to_string();
        let final_posting =
            upsert_posting(&pool, "acct-jobs", &final_posting, &profile, &preferences).unwrap();
        let (final_application, _) = prepare_application(
            &pool,
            "acct-jobs",
            &final_posting.id,
            "factual",
            "review_first",
        )
        .unwrap();
        assert!(
            reserve_application_attempt(&pool, "acct-jobs", &final_application.id, "local")
                .unwrap_err()
                .to_string()
                .contains("attempt limit")
        );

        let reservations = list_attempt_reservations(&pool, "acct-jobs").unwrap();
        assert_eq!(reservations.len(), 10);
        assert!(reservations
            .iter()
            .any(|reservation| reservation.application_id == application.id));
    }

    #[test]
    fn career_profile_payload_is_encrypted_at_rest() {
        let pool = test_pool();
        let mut profile = default_profile("jobs@example.com");
        profile.full_name = "Taylor Rivera".to_string();
        profile.street_address = "123 Example Street".to_string();
        save_profile(&pool, "acct-jobs", &profile).unwrap();

        let stored: String = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT profile_json FROM jobs_profiles WHERE account_id = ?1",
                params!["acct-jobs"],
                |row| row.get(0),
            )
            .unwrap();
        assert!(stored.starts_with(ENCRYPTED_PAYLOAD_PREFIX));
        assert!(!stored.contains("Example Street"));

        let restored = get_profile(&pool, "acct-jobs", "jobs@example.com").unwrap();
        assert_eq!(restored.street_address, profile.street_address);
    }

    #[test]
    fn application_updates_enforce_state_machine_and_submission_mode() {
        let pool = test_pool();
        let profile = default_profile("jobs@example.com");
        save_profile(&pool, "acct-jobs", &profile).unwrap();
        let posting = upsert_posting(
            &pool,
            "acct-jobs",
            &JobPosting {
                id: String::new(),
                canonical_key: String::new(),
                source: "greenhouse".to_string(),
                external_id: String::new(),
                company: "Acme".to_string(),
                title: "Engineer".to_string(),
                location: "Remote".to_string(),
                workplace: "remote".to_string(),
                canonical_url: "https://boards.greenhouse.io/acme/jobs/state-machine".to_string(),
                description: String::new(),
                compensation: String::new(),
                employment_type: String::new(),
                track_id: "track-default".to_string(),
                match_score: 84,
                matched_reasons: Vec::new(),
                missing_requirements: Vec::new(),
                posted_at_ms: Some(now_ms()),
                last_verified_at_ms: Some(now_ms()),
                availability_status: "active".to_string(),
                status: "matched".to_string(),
                created_at_ms: 0,
                updated_at_ms: 0,
                eligibility: None,
            },
            &profile,
            &JobPreferences::default(),
        )
        .unwrap();
        let (application, _) =
            prepare_application(&pool, "acct-jobs", &posting.id, "factual", "review_first")
                .unwrap();

        let queued = update_application(
            &pool,
            "acct-jobs",
            &application.id,
            "queued",
            Some("auto_submit"),
        )
        .unwrap()
        .unwrap();
        assert_eq!(queued.state, "queued");
        assert_eq!(queued.submission_mode, "auto_submit");

        let invalid_transition =
            update_application(&pool, "acct-jobs", &application.id, "submitted", None);
        assert!(invalid_transition.is_err());

        let invalid_mode = update_application(
            &pool,
            "acct-jobs",
            &application.id,
            "running",
            Some("surprise_me"),
        );
        assert!(invalid_mode.is_err());

        let unchanged = get_application(&pool, "acct-jobs", &application.id)
            .unwrap()
            .unwrap();
        assert_eq!(unchanged.state, "queued");
        assert_eq!(unchanged.submission_mode, "auto_submit");
    }

    #[test]
    fn application_emails_are_verified_defaultable_and_plan_limited() {
        let pool = test_pool();
        let primary =
            ensure_primary_application_identity(&pool, "acct-jobs", "jobs@example.com").unwrap();
        assert!(primary.is_default);
        assert_eq!(primary.verification_status, "verified");

        let alternate = save_application_identity(
            &pool,
            "acct-jobs",
            &ApplicationIdentity {
                id: String::new(),
                email: "career@example.com".to_string(),
                label: "Career address".to_string(),
                verification_status: "pending".to_string(),
                is_default: false,
                created_at_ms: 0,
                updated_at_ms: 0,
            },
        )
        .unwrap();
        save_identity_verification(&pool, "acct-jobs", &alternate.id, "483921", 60_000).unwrap();
        let mut verified =
            verify_application_identity(&pool, "acct-jobs", &alternate.id, "483921").unwrap();
        verified.is_default = true;
        let verified = save_application_identity(&pool, "acct-jobs", &verified).unwrap();
        assert!(verified.is_default);
        assert!(
            !get_application_identity(&pool, "acct-jobs", &primary.id)
                .unwrap()
                .unwrap()
                .is_default
        );

        let third = save_application_identity(
            &pool,
            "acct-jobs",
            &ApplicationIdentity {
                id: String::new(),
                email: "third@example.com".to_string(),
                label: String::new(),
                verification_status: "pending".to_string(),
                is_default: false,
                created_at_ms: 0,
                updated_at_ms: 0,
            },
        );
        assert!(third
            .unwrap_err()
            .to_string()
            .contains("application email limit"));
    }

    #[test]
    fn mailbox_connections_allow_multiple_provider_accounts_with_plan_limits() {
        let pool = test_pool();
        let mailbox = |email: &str| MailboxConnection {
            id: String::new(),
            provider: "gmail".to_string(),
            status: "pending".to_string(),
            account_label: email.to_string(),
            aliases: Vec::new(),
            capabilities: Vec::new(),
            created_at_ms: 0,
            updated_at_ms: 0,
        };
        save_mailbox_connection(
            &pool,
            "acct-jobs",
            &mailbox("jobs@example.com"),
            "google-subject-1",
        )
        .unwrap();
        let free_limit = save_mailbox_connection(
            &pool,
            "acct-jobs",
            &mailbox("career@example.com"),
            "google-subject-2",
        );
        assert!(free_limit
            .unwrap_err()
            .to_string()
            .contains("connected inbox limit"));

        set_entitlement_plan(&pool, "acct-jobs", "pro").unwrap();
        save_mailbox_connection(
            &pool,
            "acct-jobs",
            &mailbox("career@example.com"),
            "google-subject-2",
        )
        .unwrap();
        let mailboxes = list_mailbox_connections(&pool, "acct-jobs").unwrap();
        assert_eq!(mailboxes.len(), 2);
        assert!(mailboxes.iter().all(|item| item.provider == "gmail"));
    }

    #[test]
    fn mailbox_connections_only_advertise_implemented_capabilities() {
        let pool = test_pool();
        let mailbox = save_mailbox_connection(
            &pool,
            "acct-jobs",
            &MailboxConnection {
                id: String::new(),
                provider: "gmail".to_string(),
                status: "connected".to_string(),
                account_label: "jobs@example.com".to_string(),
                aliases: Vec::new(),
                capabilities: Vec::new(),
                created_at_ms: 0,
                updated_at_ms: 0,
            },
            "google-subject-capabilities",
        )
        .unwrap();

        assert_eq!(
            mailbox.capabilities,
            vec![
                "status_sync",
                "application_correlation",
                "review_interventions"
            ]
        );
    }

    #[test]
    fn oauth_state_is_encrypted_single_use_and_expires() {
        let pool = test_pool();
        let now = now_ms();
        let token = "state-token-with-more-than-thirty-two-random-characters";
        let state = JobsOAuthState {
            provider: "gmail".to_string(),
            code_verifier: "pkce-secret-that-must-not-be-visible-at-rest".to_string(),
            return_path: "/jobs/settings".to_string(),
            expires_at_ms: now + 60_000,
            created_at_ms: now,
        };
        save_jobs_oauth_state(&pool, "acct-jobs", token, &state).unwrap();
        let raw: String = pool
            .get()
            .unwrap()
            .query_row("SELECT state_json FROM jobs_oauth_states", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert!(!raw.contains("pkce-secret"));
        let (account_id, consumed) = consume_jobs_oauth_state(&pool, token)
            .unwrap()
            .expect("state exists");
        assert_eq!(account_id, "acct-jobs");
        assert_eq!(consumed, state);
        assert!(consume_jobs_oauth_state(&pool, token).unwrap().is_none());

        let expired_token = "another-state-token-with-more-than-thirty-two-characters";
        save_jobs_oauth_state(
            &pool,
            "acct-jobs",
            expired_token,
            &JobsOAuthState {
                expires_at_ms: now - 1,
                ..consumed
            },
        )
        .unwrap();
        assert!(consume_jobs_oauth_state(&pool, expired_token)
            .unwrap()
            .is_none());
    }

    #[test]
    fn provider_credentials_are_encrypted_and_tenant_scoped() {
        let pool = test_pool();
        pool.get()
            .unwrap()
            .execute(
                "INSERT INTO accounts (id, email, password_hash, trial_seconds_remaining)
                 VALUES ('acct-other', 'other@example.com', 'hash', 0)",
                [],
            )
            .unwrap();
        let mailbox = save_mailbox_connection(
            &pool,
            "acct-jobs",
            &MailboxConnection {
                id: String::new(),
                provider: "gmail".to_string(),
                status: "connected".to_string(),
                account_label: "jobs@example.com".to_string(),
                aliases: Vec::new(),
                capabilities: Vec::new(),
                created_at_ms: 0,
                updated_at_ms: 0,
            },
            "google-subject-credential-test",
        )
        .unwrap();
        let now = now_ms();
        let credential = JobsProviderCredential {
            connection_id: mailbox.id.clone(),
            provider: "gmail".to_string(),
            provider_subject: "google-subject-credential-test".to_string(),
            access_token: "dummy-provider-access-token".to_string(),
            refresh_token: "dummy-provider-refresh-token".to_string(),
            scopes: vec!["gmail.readonly".to_string()],
            expires_at_ms: now + 3_600_000,
            created_at_ms: now,
            updated_at_ms: now,
        };
        save_jobs_provider_credential(&pool, "acct-jobs", &credential).unwrap();
        let raw: String = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT credential_json FROM jobs_provider_credentials WHERE connection_id = ?1",
                params![mailbox.id],
                |row| row.get(0),
            )
            .unwrap();
        assert!(!raw.contains("dummy-provider-access-token"));
        assert_eq!(
            jobs_provider_credential(&pool, "acct-jobs", &credential.connection_id)
                .unwrap()
                .unwrap(),
            credential
        );
        assert!(
            jobs_provider_credential(&pool, "acct-other", &credential.connection_id)
                .unwrap()
                .is_none()
        );
        assert!(delete_mailbox_connection(&pool, "acct-jobs", &mailbox.id).unwrap());
        assert!(
            jobs_provider_credential(&pool, "acct-jobs", &credential.connection_id)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn oauth_mailbox_setup_atomically_initializes_sync_state() {
        let pool = test_pool();
        let now = now_ms();
        let (mailbox, credential) = save_mailbox_connection_with_credential(
            &pool,
            "acct-jobs",
            &MailboxConnection {
                id: String::new(),
                provider: "gmail".to_string(),
                status: "connected".to_string(),
                account_label: "jobs@example.com".to_string(),
                aliases: vec!["jobs+applications@example.com".to_string()],
                capabilities: Vec::new(),
                created_at_ms: now,
                updated_at_ms: now,
            },
            &JobsProviderCredential {
                connection_id: String::new(),
                provider: "gmail".to_string(),
                provider_subject: "google-subject-atomic-setup".to_string(),
                access_token: "dummy-atomic-access-token".to_string(),
                refresh_token: "dummy-atomic-refresh-token".to_string(),
                scopes: vec!["gmail.readonly".to_string()],
                expires_at_ms: now + 3_600_000,
                created_at_ms: now,
                updated_at_ms: now,
            },
        )
        .unwrap();

        assert_eq!(mailbox.id, credential.connection_id);
        assert_eq!(
            jobs_provider_credential(&pool, "acct-jobs", &mailbox.id)
                .unwrap()
                .unwrap()
                .refresh_token,
            "dummy-atomic-refresh-token"
        );
        let sync = mailbox_sync_state(&pool, "acct-jobs", &mailbox.id)
            .unwrap()
            .expect("OAuth setup creates the durable sync state");
        assert_eq!(sync.provider, "gmail");
        assert!(sync
            .cursor
            .as_object()
            .is_some_and(|cursor| cursor.is_empty()));
        assert!(sync.lease_owner.is_none());
    }

    #[test]
    fn mailbox_sync_lease_is_exclusive_and_recovers_after_expiry() {
        let pool = test_pool();
        let mailbox = save_mailbox_connection(
            &pool,
            "acct-jobs",
            &MailboxConnection {
                id: String::new(),
                provider: "outlook".to_string(),
                status: "connected".to_string(),
                account_label: "jobs@example.com".to_string(),
                aliases: Vec::new(),
                capabilities: Vec::new(),
                created_at_ms: 0,
                updated_at_ms: 0,
            },
            "microsoft-subject-lease-test",
        )
        .unwrap();
        initialize_mailbox_sync_state(&pool, "acct-jobs", &mailbox.id, "outlook").unwrap();

        let first = claim_mailbox_sync(&pool, "acct-jobs", &mailbox.id, "worker-a", 60_000)
            .unwrap()
            .expect("first worker claims the mailbox");
        assert_eq!(first.lease_owner.as_deref(), Some("worker-a"));
        assert!(
            claim_mailbox_sync(&pool, "acct-jobs", &mailbox.id, "worker-b", 60_000)
                .unwrap()
                .is_none()
        );
        assert!(!finish_mailbox_sync(
            &pool,
            "acct-jobs",
            &mailbox.id,
            "worker-b",
            json!({"delta_link": "wrong-owner"}),
            "",
            None,
        )
        .unwrap());

        pool.get()
            .unwrap()
            .execute(
                "UPDATE jobs_provider_sync_state
                    SET lease_expires_at_ms = ?2
                  WHERE connection_id = ?1",
                params![mailbox.id, now_ms() - 1],
            )
            .unwrap();
        let recovered = claim_mailbox_sync(&pool, "acct-jobs", &mailbox.id, "worker-b", 60_000)
            .unwrap()
            .expect("expired lease can be recovered");
        assert_eq!(recovered.lease_owner.as_deref(), Some("worker-b"));
        assert!(finish_mailbox_sync(
            &pool,
            "acct-jobs",
            &mailbox.id,
            "worker-b",
            json!({"delta_link": "next"}),
            "",
            Some(now_ms() + 120_000),
        )
        .unwrap());
        let finished = mailbox_sync_state(&pool, "acct-jobs", &mailbox.id)
            .unwrap()
            .unwrap();
        assert_eq!(finished.cursor["delta_link"], "next");
        assert!(finished.last_synced_at_ms.is_some());
        assert!(finished.lease_owner.is_none());
    }

    #[test]
    fn reconnect_required_mailboxes_are_not_claimed_until_reconnected() {
        let pool = test_pool();
        let mailbox = save_mailbox_connection(
            &pool,
            "acct-jobs",
            &MailboxConnection {
                id: String::new(),
                provider: "gmail".to_string(),
                status: "connected".to_string(),
                account_label: "jobs@example.com".to_string(),
                aliases: Vec::new(),
                capabilities: Vec::new(),
                created_at_ms: 0,
                updated_at_ms: 0,
            },
            "google-subject-reconnect-required",
        )
        .unwrap();
        initialize_mailbox_sync_state(&pool, "acct-jobs", &mailbox.id, "gmail").unwrap();

        assert!(mark_mailbox_reauthorization_required(&pool, "acct-jobs", &mailbox.id).unwrap());
        assert_eq!(
            mailbox_connection(&pool, "acct-jobs", &mailbox.id)
                .unwrap()
                .unwrap()
                .status,
            "reauthorization_required"
        );
        assert!(claim_mailbox_sync(
            &pool,
            "acct-jobs",
            &mailbox.id,
            "single-worker",
            60_000
        )
        .unwrap()
        .is_none());
        assert!(claim_due_mailbox_syncs(&pool, "batch-worker", 60_000, 10)
            .unwrap()
            .is_empty());

        let reconnected = save_mailbox_connection(
            &pool,
            "acct-jobs",
            &MailboxConnection {
                status: "connected".to_string(),
                ..mailbox.clone()
            },
            "google-subject-reconnect-required",
        )
        .unwrap();
        assert_eq!(reconnected.id, mailbox.id);
        assert!(claim_due_mailbox_syncs(&pool, "batch-worker", 60_000, 10)
            .unwrap()
            .iter()
            .any(|(account_id, state)| {
                account_id == "acct-jobs" && state.connection_id == mailbox.id
            }));
        assert!(!mark_mailbox_reauthorization_required(&pool, "acct-other", &mailbox.id)
            .unwrap());
    }

    #[test]
    fn provider_messages_are_idempotent_encrypted_and_tenant_scoped() {
        let pool = test_pool();
        pool.get()
            .unwrap()
            .execute(
                "INSERT INTO accounts (id, email, password_hash, trial_seconds_remaining)
                 VALUES ('acct-other', 'other@example.com', 'hash', 0)",
                [],
            )
            .unwrap();
        let mailbox = save_mailbox_connection(
            &pool,
            "acct-jobs",
            &MailboxConnection {
                id: String::new(),
                provider: "gmail".to_string(),
                status: "connected".to_string(),
                account_label: "jobs@example.com".to_string(),
                aliases: Vec::new(),
                capabilities: Vec::new(),
                created_at_ms: 0,
                updated_at_ms: 0,
            },
            "google-subject-message-test",
        )
        .unwrap();
        let message = JobsProviderMessage {
            id: String::new(),
            connection_id: mailbox.id.clone(),
            provider: "gmail".to_string(),
            external_id: "gmail-message-123".to_string(),
            sender: "recruiter@example.org".to_string(),
            recipients: vec!["jobs@example.com".to_string()],
            subject: "Interview availability".to_string(),
            body_text: "Please share a few interview times.".to_string(),
            received_at_ms: now_ms(),
            application_id: None,
            processing_status: "needs_input".to_string(),
            classification: "interview".to_string(),
            confidence: 0.98,
            metadata: json!({"thread_id": "gmail-thread-456"}),
            processed_at_ms: None,
            created_at_ms: 0,
            updated_at_ms: 0,
        };
        let (stored, inserted) = save_provider_message(&pool, "acct-jobs", &message).unwrap();
        assert!(inserted);
        let (redelivered, inserted_again) =
            save_provider_message(&pool, "acct-jobs", &message).unwrap();
        assert!(!inserted_again);
        assert_eq!(stored.id, redelivered.id);
        assert_eq!(
            list_provider_messages(&pool, "acct-jobs", Some(&mailbox.id), 20)
                .unwrap()
                .len(),
            1
        );
        assert!(list_provider_messages(&pool, "acct-other", None, 20)
            .unwrap()
            .is_empty());

        let raw: String = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT message_json FROM jobs_provider_messages WHERE id = ?1",
                params![stored.id],
                |row| row.get(0),
            )
            .unwrap();
        assert!(!raw.contains("Please share a few interview times"));
        assert!(!raw.contains("recruiter@example.org"));
    }

    #[test]
    fn mailbox_sync_can_be_scheduled_immediately_without_stealing_a_lease() {
        let pool = test_pool();
        let mailbox = save_mailbox_connection(
            &pool,
            "acct-jobs",
            &MailboxConnection {
                id: String::new(),
                provider: "gmail".to_string(),
                status: "connected".to_string(),
                account_label: "jobs@example.com".to_string(),
                aliases: Vec::new(),
                capabilities: Vec::new(),
                created_at_ms: 0,
                updated_at_ms: 0,
            },
            "google-subject-sync-now",
        )
        .unwrap();
        initialize_mailbox_sync_state(&pool, "acct-jobs", &mailbox.id, "gmail").unwrap();
        pool.get()
            .unwrap()
            .execute(
                "UPDATE jobs_provider_sync_state
                    SET next_sync_at_ms = ?2
                  WHERE connection_id = ?1",
                params![mailbox.id, now_ms() + 3_600_000],
            )
            .unwrap();
        let claimed = claim_mailbox_sync(
            &pool,
            "acct-jobs",
            &mailbox.id,
            "active-sync-worker",
            60_000,
        )
        .unwrap()
        .unwrap();

        let scheduled = schedule_mailbox_sync_now(&pool, "acct-jobs", &mailbox.id)
            .unwrap()
            .unwrap();
        assert!(scheduled.next_sync_at_ms <= now_ms());
        assert_eq!(scheduled.lease_owner.as_deref(), Some("active-sync-worker"));
        assert_eq!(scheduled.lease_expires_at_ms, claimed.lease_expires_at_ms);
    }

    #[test]
    fn pending_provider_messages_survive_restart_and_transition_once() {
        let pool = test_pool();
        pool.get()
            .unwrap()
            .execute(
                "INSERT INTO accounts (id, email, password_hash, trial_seconds_remaining)
                 VALUES ('acct-other', 'other@example.com', 'hash', 0)",
                [],
            )
            .unwrap();
        let mailbox = save_mailbox_connection(
            &pool,
            "acct-jobs",
            &MailboxConnection {
                id: String::new(),
                provider: "outlook".to_string(),
                status: "connected".to_string(),
                account_label: "jobs@example.com".to_string(),
                aliases: Vec::new(),
                capabilities: Vec::new(),
                created_at_ms: 0,
                updated_at_ms: 0,
            },
            "microsoft-subject-recovery",
        )
        .unwrap();
        let (stored, inserted) = save_provider_message(
            &pool,
            "acct-jobs",
            &JobsProviderMessage {
                id: String::new(),
                connection_id: mailbox.id.clone(),
                provider: "outlook".to_string(),
                external_id: "outlook-message-recovery".to_string(),
                sender: "recruiter@example.org".to_string(),
                recipients: vec!["jobs@example.com".to_string()],
                subject: "Application update".to_string(),
                body_text: "We received your application.".to_string(),
                received_at_ms: now_ms(),
                application_id: None,
                processing_status: "received".to_string(),
                classification: String::new(),
                confidence: 0.0,
                metadata: json!({"conversation_id": "conversation-1"}),
                processed_at_ms: None,
                created_at_ms: 0,
                updated_at_ms: 0,
            },
        )
        .unwrap();
        assert!(inserted);
        assert_eq!(
            list_pending_provider_messages(&pool, "acct-jobs", &mailbox.id, 10)
                .unwrap()
                .iter()
                .map(|message| message.id.as_str())
                .collect::<Vec<_>>(),
            vec![stored.id.as_str()]
        );

        assert!(update_provider_message_processing(
            &pool,
            "acct-other",
            &stored.id,
            None,
            "processed",
            "acknowledgment",
            0.99,
            json!({"correlation": "none"}),
        )
        .unwrap()
        .is_none());
        let updated = update_provider_message_processing(
            &pool,
            "acct-jobs",
            &stored.id,
            None,
            "processed",
            "acknowledgment",
            0.99,
            json!({"correlation": "unmatched"}),
        )
        .unwrap()
        .unwrap();
        assert_eq!(updated.processing_status, "processed");
        assert_eq!(updated.classification, "acknowledgment");
        assert_eq!(updated.metadata["correlation"], "unmatched");
        assert!(updated.processed_at_ms.is_some());
        assert!(
            list_pending_provider_messages(&pool, "acct-jobs", &mailbox.id, 10)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn career_track_email_is_frozen_into_resume_and_receipt() {
        let pool = test_pool();
        let mut profile = default_profile("jobs@example.com");
        profile.full_name = "Taylor Rivera".to_string();
        save_profile(&pool, "acct-jobs", &profile).unwrap();
        ensure_primary_application_identity(&pool, "acct-jobs", "jobs@example.com").unwrap();
        let alternate = save_application_identity(
            &pool,
            "acct-jobs",
            &ApplicationIdentity {
                id: String::new(),
                email: "applications@example.com".to_string(),
                label: "Applications".to_string(),
                verification_status: "pending".to_string(),
                is_default: false,
                created_at_ms: 0,
                updated_at_ms: 0,
            },
        )
        .unwrap();
        save_identity_verification(&pool, "acct-jobs", &alternate.id, "602314", 60_000).unwrap();
        let alternate =
            verify_application_identity(&pool, "acct-jobs", &alternate.id, "602314").unwrap();
        let track = upsert_track(
            &pool,
            "acct-jobs",
            &CareerTrack {
                id: String::new(),
                name: "Engineering".to_string(),
                role: "Product Engineer".to_string(),
                locations: vec!["New York, NY".to_string()],
                remote_preference: "hybrid_ok".to_string(),
                application_identity_id: Some(alternate.id.clone()),
                policy: CareerTrackPolicy::default(),
                active: true,
                match_count: 0,
                created_at_ms: 0,
                updated_at_ms: 0,
            },
        )
        .unwrap();
        let posting = upsert_posting(
            &pool,
            "acct-jobs",
            &JobPosting {
                id: String::new(),
                canonical_key: String::new(),
                source: "greenhouse".to_string(),
                external_id: "email-test".to_string(),
                company: "Northstar".to_string(),
                title: "Product Engineer".to_string(),
                location: "New York, NY".to_string(),
                workplace: "hybrid".to_string(),
                canonical_url: "https://example.com/jobs/email-test".to_string(),
                description: "Product engineering".to_string(),
                compensation: String::new(),
                employment_type: String::new(),
                track_id: track.id,
                match_score: 88,
                matched_reasons: Vec::new(),
                missing_requirements: Vec::new(),
                posted_at_ms: Some(now_ms()),
                last_verified_at_ms: Some(now_ms()),
                availability_status: "active".to_string(),
                status: "matched".to_string(),
                created_at_ms: 0,
                updated_at_ms: 0,
                eligibility: None,
            },
            &profile,
            &JobPreferences::default(),
        )
        .unwrap();
        let (application, resume) =
            prepare_application(&pool, "acct-jobs", &posting.id, "factual", "review_first")
                .unwrap();
        assert_eq!(
            resume
                .content
                .pointer("/contact/email")
                .and_then(Value::as_str),
            Some("applications@example.com")
        );
        assert_eq!(
            application
                .receipt
                .pointer("/application_identity/email")
                .and_then(Value::as_str),
            Some("applications@example.com")
        );
    }

    #[test]
    fn answer_memory_is_encrypted_scoped_and_idempotent() {
        let pool = test_pool();
        pool.get()
            .unwrap()
            .execute(
                "INSERT INTO accounts (id, email, password_hash, trial_seconds_remaining)
                 VALUES ('acct-other', 'other@example.com', 'hash', 0)",
                [],
            )
            .unwrap();
        let answer = AnswerMemory {
            id: String::new(),
            key: String::new(),
            question: "Why are you interested in this role?".to_string(),
            value: "I enjoy building reliable customer workflows.".to_string(),
            scope: "account".to_string(),
            scope_id: None,
            confirmed: true,
            source: "settings".to_string(),
            created_at_ms: 0,
            updated_at_ms: 0,
            last_used_at_ms: None,
            use_count: 0,
        };
        let first = save_answer_memory(&pool, "acct-jobs", &answer).unwrap();
        let second = save_answer_memory(
            &pool,
            "acct-jobs",
            &AnswerMemory {
                value: "I build dependable products for customers.".to_string(),
                ..answer
            },
        )
        .unwrap();
        assert_eq!(first.id, second.id);
        let saved = list_answer_memory(&pool, "acct-jobs").unwrap();
        assert_eq!(saved.len(), 1);
        assert_eq!(saved[0].value, "I build dependable products for customers.");
        assert!(list_answer_memory(&pool, "acct-other").unwrap().is_empty());

        let raw: String = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT answer_json FROM jobs_answer_memory WHERE id = ?1",
                params![first.id],
                |row| row.get(0),
            )
            .unwrap();
        assert!(!raw.contains("dependable products"));
        assert!(delete_answer_memory(&pool, "acct-jobs", &second.id).unwrap());
        assert!(list_answer_memory(&pool, "acct-jobs").unwrap().is_empty());
    }

    #[test]
    fn candidate_events_are_encrypted_tenant_scoped_and_append_only() {
        let pool = test_pool();
        pool.get()
            .unwrap()
            .execute(
                "INSERT INTO accounts (id, email, password_hash, trial_seconds_remaining)
                 VALUES ('acct-other', 'other@example.com', 'hash', 0)",
                [],
            )
            .unwrap();
        let profile = default_profile("jobs@example.com");
        save_profile(&pool, "acct-jobs", &profile).unwrap();
        let posting = upsert_posting(
            &pool,
            "acct-jobs",
            &test_posting(
                "https://boards.greenhouse.io/acme/jobs/candidate-events",
                now_ms(),
                now_ms(),
            ),
            &profile,
            &JobPreferences::default(),
        )
        .unwrap();
        let (application, _) =
            prepare_application(&pool, "acct-jobs", &posting.id, "factual", "review_first")
                .unwrap();
        let base = CandidateEvent {
            id: String::new(),
            event_type: "match_feedback".to_string(),
            job_id: Some(posting.id.clone()),
            application_id: None,
            action: "pass".to_string(),
            reasons: vec!["location".to_string()],
            note: "The commute is too long.".to_string(),
            status: String::new(),
            created_at_ms: 0,
            updated_at_ms: 0,
        };
        let passed = save_candidate_event(&pool, "acct-jobs", &base).unwrap();
        let restored = save_candidate_event(
            &pool,
            "acct-jobs",
            &CandidateEvent {
                action: "restore".to_string(),
                reasons: Vec::new(),
                note: String::new(),
                ..base.clone()
            },
        )
        .unwrap();
        assert_ne!(passed.id, restored.id);

        let issue = save_candidate_event(
            &pool,
            "acct-jobs",
            &CandidateEvent {
                event_type: "application_issue".to_string(),
                job_id: None,
                application_id: Some(application.id.clone()),
                action: "site_problem".to_string(),
                reasons: Vec::new(),
                note: "The employer form did not accept the attachment.".to_string(),
                ..base.clone()
            },
        )
        .unwrap();
        assert_eq!(issue.job_id.as_deref(), Some(posting.id.as_str()));
        assert_eq!(issue.status, "open");
        let outcome = save_candidate_event(
            &pool,
            "acct-jobs",
            &CandidateEvent {
                event_type: "application_outcome".to_string(),
                job_id: Some(posting.id.clone()),
                application_id: Some(application.id.clone()),
                action: "interview".to_string(),
                reasons: Vec::new(),
                note: "Recruiter screen next week.".to_string(),
                ..base
            },
        )
        .unwrap();
        assert_eq!(outcome.status, "confirmed");
        assert_eq!(list_candidate_events(&pool, "acct-jobs").unwrap().len(), 4);
        assert!(list_candidate_events(&pool, "acct-other")
            .unwrap()
            .is_empty());

        let raw: String = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT event_json FROM jobs_candidate_events WHERE id = ?1",
                params![issue.id],
                |row| row.get(0),
            )
            .unwrap();
        assert!(raw.starts_with(ENCRYPTED_PAYLOAD_PREFIX));
        assert!(!raw.contains("employer form"));

        let cross_account = save_candidate_event(
            &pool,
            "acct-other",
            &CandidateEvent {
                id: String::new(),
                event_type: "application_issue".to_string(),
                job_id: None,
                application_id: Some(application.id),
                action: "other".to_string(),
                reasons: Vec::new(),
                note: String::new(),
                status: String::new(),
                created_at_ms: 0,
                updated_at_ms: 0,
            },
        );
        assert!(cross_account
            .unwrap_err()
            .to_string()
            .contains("application not found"));
    }

    #[test]
    fn application_packets_reuse_answer_memory_with_company_precedence() {
        let pool = test_pool();
        let profile = default_profile("jobs@example.com");
        save_profile(&pool, "acct-jobs", &profile).unwrap();
        let posting = upsert_posting(
            &pool,
            "acct-jobs",
            &test_posting(
                "https://boards.greenhouse.io/acme/jobs/answer-memory",
                now_ms(),
                now_ms(),
            ),
            &profile,
            &JobPreferences::default(),
        )
        .unwrap();
        let base = AnswerMemory {
            id: String::new(),
            key: String::new(),
            question: "Why are you interested in this role?".to_string(),
            value: "I enjoy building reliable products.".to_string(),
            scope: "account".to_string(),
            scope_id: None,
            confirmed: true,
            source: "settings".to_string(),
            created_at_ms: 0,
            updated_at_ms: 0,
            last_used_at_ms: None,
            use_count: 0,
        };
        save_answer_memory(&pool, "acct-jobs", &base).unwrap();
        save_answer_memory(
            &pool,
            "acct-jobs",
            &AnswerMemory {
                value: "Acme's reliability work matches my experience.".to_string(),
                scope: "company".to_string(),
                scope_id: Some("acme".to_string()),
                ..base
            },
        )
        .unwrap();

        let (application, _) =
            prepare_application(&pool, "acct-jobs", &posting.id, "factual", "review_first")
                .unwrap();
        let remembered = application
            .answers
            .iter()
            .find(|answer| {
                answer.get("key").and_then(Value::as_str)
                    == Some("why are you interested in this role")
            })
            .expect("remembered answer");
        assert_eq!(
            remembered.get("value").and_then(Value::as_str),
            Some("Acme's reliability work matches my experience.")
        );
        assert_eq!(
            remembered.get("scope").and_then(Value::as_str),
            Some("company")
        );
    }

    #[test]
    fn local_browser_ticket_is_encrypted_scoped_and_claimable() {
        let pool = test_pool();
        let profile = default_profile("jobs@example.com");
        save_profile(&pool, "acct-jobs", &profile).unwrap();
        let posting = upsert_posting(
            &pool,
            "acct-jobs",
            &test_posting(
                "https://boards.greenhouse.io/acme/jobs/local-run",
                now_ms(),
                now_ms(),
            ),
            &profile,
            &JobPreferences::default(),
        )
        .unwrap();
        let (application, _) =
            prepare_application(&pool, "acct-jobs", &posting.id, "factual", "review_first")
                .unwrap();
        let payload = json!({ "applicationId": application.id, "answer": "private value" });
        let saved = save_local_run_ticket(
            &pool,
            "acct-jobs",
            &application.id,
            "local-run-1",
            "ticket-hash",
            "ticket-secret",
            payload.clone(),
            now_ms() + 60_000,
        )
        .unwrap();
        assert_eq!(saved.status, "queued");

        let stored: (String, String) = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT ticket_secret, payload_json FROM jobs_local_run_tickets WHERE id = ?1",
                params!["local-run-1"],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert!(!stored.0.contains("ticket-secret"));
        assert!(!stored.1.contains("private value"));
        assert!(claim_local_run_ticket(&pool, "local-run-1", "wrong-hash")
            .unwrap()
            .is_none());
        let claimed = claim_local_run_ticket(&pool, "local-run-1", "ticket-hash")
            .unwrap()
            .unwrap();
        assert_eq!(claimed.ticket_secret, "ticket-secret");
        assert_eq!(claimed.payload, payload);
        assert_eq!(claimed.status, "claimed");
        assert!(
            update_local_run_ticket_status(&pool, "local-run-1", "ticket-hash", "complete",)
                .unwrap()
        );
        assert!(claim_local_run_ticket(&pool, "local-run-1", "ticket-hash")
            .unwrap()
            .is_none());
    }

    #[test]
    fn local_run_authority_rechecks_entitlement_and_verified_identity_before_submit() {
        let pool = test_pool();
        let (application, run_id, ticket_hash, identity_id) =
            local_run_authority_fixture(&pool, "live-authority");
        assert!(
            claim_authorized_local_run_ticket(&pool, &run_id, &ticket_hash)
                .unwrap()
                .is_some()
        );
        assert!(
            claim_authorized_local_run_ticket(&pool, &run_id, &ticket_hash)
                .unwrap()
                .is_none()
        );
        update_application(&pool, "acct-jobs", &application.id, "running", None).unwrap();
        upsert_browser_session(
            &pool,
            "acct-jobs",
            &BrowserSession {
                id: run_id.clone(),
                runner: "local".to_string(),
                status: "running".to_string(),
                current_company: "Acme".to_string(),
                current_step: "Ready to submit".to_string(),
                application_id: Some(application.id.clone()),
                takeover_url: None,
                created_at_ms: 0,
                updated_at_ms: 0,
            },
        )
        .unwrap();
        assert!(local_run_submit_authorized(&pool, &run_id, &ticket_hash).unwrap());

        set_entitlement_plan(&pool, "acct-jobs", "free").unwrap();
        assert!(!local_run_submit_authorized(&pool, &run_id, &ticket_hash).unwrap());
        set_entitlement_plan(&pool, "acct-jobs", "pro").unwrap();
        assert!(local_run_submit_authorized(&pool, &run_id, &ticket_hash).unwrap());

        pool.get()
            .unwrap()
            .execute(
                "DELETE FROM jobs_application_identities WHERE account_id = ?1 AND id = ?2",
                params!["acct-jobs", identity_id],
            )
            .unwrap();
        assert!(!local_run_submit_authorized(&pool, &run_id, &ticket_hash).unwrap());
    }

    #[test]
    fn local_run_claim_fails_closed_after_plan_or_identity_revocation() {
        let downgraded = test_pool();
        let (_, run_id, ticket_hash, _) = local_run_authority_fixture(&downgraded, "downgraded");
        set_entitlement_plan(&downgraded, "acct-jobs", "free").unwrap();
        assert!(
            claim_authorized_local_run_ticket(&downgraded, &run_id, &ticket_hash)
                .unwrap()
                .is_none()
        );

        let deleted = test_pool();
        let (_, run_id, ticket_hash, identity_id) =
            local_run_authority_fixture(&deleted, "identity-deleted");
        deleted
            .get()
            .unwrap()
            .execute(
                "DELETE FROM jobs_application_identities WHERE account_id = ?1 AND id = ?2",
                params!["acct-jobs", identity_id],
            )
            .unwrap();
        assert!(
            claim_authorized_local_run_ticket(&deleted, &run_id, &ticket_hash)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn concurrent_authorized_local_claims_have_exactly_one_winner() {
        let pool = test_pool();
        let (_, run_id, ticket_hash, _) = local_run_authority_fixture(&pool, "claim-race");
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
        let handles = (0..2)
            .map(|_| {
                let pool = pool.clone();
                let run_id = run_id.clone();
                let ticket_hash = ticket_hash.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    claim_authorized_local_run_ticket(&pool, &run_id, &ticket_hash)
                        .unwrap()
                        .is_some()
                })
            })
            .collect::<Vec<_>>();
        assert_eq!(
            handles
                .into_iter()
                .map(|handle| handle.join().unwrap())
                .filter(|won| *won)
                .count(),
            1
        );
    }

    #[test]
    fn local_submission_resume_is_retrievable_after_first_consumption_until_terminal() {
        let pool = test_pool();
        let (application, run_id, _) = execution_lease_fixture(&pool, "local-resume");
        update_application(&pool, "acct-jobs", &application.id, "running", None).unwrap();
        update_application(&pool, "acct-jobs", &application.id, "needs_input", None).unwrap();
        save_local_run_ticket(
            &pool,
            "acct-jobs",
            &application.id,
            &run_id,
            "local-resume-ticket-hash",
            "local-resume-ticket-secret",
            json!({ "runId": run_id }),
            now_ms() + 60_000,
        )
        .unwrap();
        update_local_run_ticket_status(&pool, &run_id, "local-resume-ticket-hash", "needs_input")
            .unwrap();
        assert!(
            claim_local_run_ticket(&pool, &run_id, "local-resume-ticket-hash")
                .unwrap()
                .is_none()
        );
        assert!(
            consume_local_run_resume_action(&pool, &run_id, "local-resume-ticket-hash")
                .unwrap()
                .is_none()
        );

        let intervention = save_intervention(
            &pool,
            "acct-jobs",
            &Intervention {
                id: String::new(),
                application_id: Some(application.id.clone()),
                kind: "browser_takeover".to_string(),
                status: "approved".to_string(),
                title: "Review the Greenhouse application".to_string(),
                detail: "Review the form".to_string(),
                choices: Vec::new(),
                resolution_kind: "browser_takeover".to_string(),
                resume_after_resolution: true,
                provider: String::new(),
                provider_message_id: String::new(),
                expires_at_ms: None,
                metadata: json!({}),
                created_at_ms: 0,
                resolved_at_ms: None,
            },
        )
        .unwrap();
        let approved = approve_local_run_resume_action(
            &pool,
            "acct-jobs",
            &application.id,
            &run_id,
            &intervention.id,
        )
        .unwrap()
        .unwrap();
        assert_eq!(approved.action, "approve_submission");
        assert!(
            consume_local_run_resume_action(&pool, &run_id, "wrong-ticket-hash")
                .unwrap()
                .is_none()
        );
        let consumed = consume_local_run_resume_action(&pool, &run_id, "local-resume-ticket-hash")
            .unwrap()
            .unwrap();
        assert_eq!(consumed.intervention_id, intervention.id);
        assert!(consumed.first_consumption);
        let consumed_at_ms: i64 = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT consumed_at_ms FROM jobs_local_run_resume_actions WHERE run_id = ?1",
                params![run_id],
                |row| row.get(0),
            )
            .unwrap();
        let recovered = consume_local_run_resume_action(&pool, &run_id, "local-resume-ticket-hash")
            .unwrap()
            .unwrap();
        assert_eq!(recovered.intervention_id, intervention.id);
        assert!(!recovered.first_consumption);
        let recovered_at_ms: i64 = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT consumed_at_ms FROM jobs_local_run_resume_actions WHERE run_id = ?1",
                params![run_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(recovered_at_ms, consumed_at_ms);
        assert!(
            local_submission_approval_consumed(&pool, "acct-jobs", &application.id, &run_id,)
                .unwrap()
        );
        update_local_run_ticket_status(&pool, &run_id, "local-resume-ticket-hash", "failed")
            .unwrap();
        assert!(
            consume_local_run_resume_action(&pool, &run_id, "local-resume-ticket-hash")
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn jobs_export_includes_durable_data_but_omits_ephemeral_secrets() {
        let pool = test_pool();
        let profile = default_profile("jobs@example.com");
        save_profile(&pool, "acct-jobs", &profile).unwrap();
        let posting = upsert_posting(
            &pool,
            "acct-jobs",
            &test_posting(
                "https://boards.greenhouse.io/acme/jobs/export",
                now_ms(),
                now_ms(),
            ),
            &profile,
            &JobPreferences::default(),
        )
        .unwrap();
        let (application, resume) =
            prepare_application(&pool, "acct-jobs", &posting.id, "factual", "review_first")
                .unwrap();
        upsert_browser_session(
            &pool,
            "acct-jobs",
            &BrowserSession {
                id: "session-export".to_string(),
                runner: "cloud".to_string(),
                status: "needs_input".to_string(),
                current_company: "Acme".to_string(),
                current_step: "question".to_string(),
                application_id: Some(application.id.clone()),
                takeover_url: Some("https://takeover.example/secret-capability".to_string()),
                created_at_ms: 0,
                updated_at_ms: 0,
            },
        )
        .unwrap();
        save_local_run_ticket(
            &pool,
            "acct-jobs",
            &application.id,
            "export-local-run",
            "export-ticket-hash",
            "export-ticket-secret",
            json!({ "private_packet": "must-not-export" }),
            now_ms() + 60_000,
        )
        .unwrap();
        save_application_evidence(
            &pool,
            "acct-jobs",
            &ApplicationEvidence {
                id: "export-resume-evidence".to_string(),
                application_id: application.id.clone(),
                kind: "resume".to_string(),
                label: "Submitted resume".to_string(),
                provider: "greenhouse".to_string(),
                file_name: "resume.pdf".to_string(),
                media_type: "application/pdf".to_string(),
                storage_key: "accounts/acct-jobs/jobs/export/resume.pdf".to_string(),
                sha256: "a".repeat(64),
                resume_version_id: Some(resume.id.clone()),
                occurred_at_ms: now_ms(),
                metadata: json!({ "size_bytes": 42 }),
                created_at_ms: 0,
            },
        )
        .unwrap();
        replace_application_receipt(
            &pool,
            "acct-jobs",
            &application.id,
            json!({
                "documents": [
                    {
                        "kind": "resume",
                        "fileName": "resume.pdf",
                        "mediaType": "application/pdf",
                        "storageKey": "accounts/acct-jobs/jobs/export/resume.pdf",
                        "sha256": "a".repeat(64)
                    },
                    {
                        "kind": "cover_letter",
                        "fileName": "cover-letter.pdf",
                        "mediaType": "application/pdf",
                        "storageKey": "accounts/acct-jobs/jobs/export/cover-letter.pdf",
                        "sha256": "b".repeat(64)
                    }
                ],
                "screenshotKeys": ["accounts/acct-jobs/jobs/export/confirmation.png"]
            }),
        )
        .unwrap();

        let export = account_export(&pool, "acct-jobs", "jobs@example.com")
            .unwrap()
            .unwrap();
        assert_eq!(export.resume_versions.len(), 1);
        assert_eq!(export.workspace.application_evidence.len(), 1);
        assert!(export.workspace.browser_sessions[0].takeover_url.is_none());
        let serialized = serde_json::to_string(&export).unwrap();
        assert!(!serialized.contains("secret-capability"));
        assert!(!serialized.contains("export-ticket-secret"));
        assert!(!serialized.contains("must-not-export"));

        let refs = crate::db::account_data::artifact_object_refs(&pool, "acct-jobs").unwrap();
        assert_eq!(refs.len(), 3);
        assert!(refs.iter().any(|reference| reference.object_key
            == "accounts/acct-jobs/jobs/export/resume.pdf"
            && reference.size_bytes == Some(42)));
        assert!(refs.iter().any(|reference| {
            reference.object_key == "accounts/acct-jobs/jobs/export/cover-letter.pdf"
        }));
        assert!(refs.iter().any(|reference| {
            reference.object_key == "accounts/acct-jobs/jobs/export/confirmation.png"
        }));
    }
}
