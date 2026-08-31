#[cfg(test)]
mod tests {
    use super::production_positive_authority_fixture::{
        install_production_positive_job_authorities,
        install_production_positive_job_authorities_for_runner,
        install_production_positive_job_authorities_with_runtime_surface,
        save_production_positive_verified_import,
    };
    use super::*;
    use crate::db;
    use crate::db::object_uploads::{
        ApplicationObjectBinding, NewObjectUpload, NewSubmissionEvidenceCapacity, ObjectKind,
        StorageScope, UploadControlError,
    };
    use crate::object_storage::UploadLimits;

    pub(super) fn test_pool() -> DbPool {
        let path = std::env::temp_dir().join(format!(
            "bluey-jobs-test-{}-{}.sqlite3",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let pool = db::open_pool(&path).unwrap();
        db::run_migrations(&pool).unwrap();
        let conn = pool.get().unwrap();
        conn.execute(
            "INSERT INTO accounts (
                id, email, password_hash, trial_seconds_remaining, email_verified_at
             ) VALUES (
                'acct-jobs', 'jobs@example.com', 'hash', 0, '2026-08-30T00:00:00Z'
             )",
            [],
        )
        .unwrap();
        conn.execute(
            "UPDATE jobs_public_beta_cohorts
                SET state = 'closed_to_new', hard_cap = 1, assigned_count = 1, revision = 1
              WHERE id = 'public-v1'",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO jobs_public_beta_enrollments (
                cohort_id, account_id, source, admitted_at_ms
             ) VALUES ('public-v1', 'acct-jobs', 'admin', 1)",
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

    fn execution_policy_fixture(
        pool: &DbPool,
        account_id: &str,
        email: &str,
        track_id: &str,
    ) -> (CareerProfile, JobPreferences) {
        let now = now_ms();
        let source = get_resume_source_asset(pool, account_id)
            .unwrap()
            .unwrap_or_else(|| ResumeSourceAsset {
                id: format!("resume-source-{account_id}"),
                file_name: "fixture-source-resume.pdf".to_string(),
                media_type: "application/pdf".to_string(),
                file_type: "pdf".to_string(),
                storage_key: format!("accounts/{account_id}/jobs/fixture-source-resume.pdf"),
                sha256: "e".repeat(64),
                size_bytes: 1_024,
                page_count: Some(1),
                template_status: "converted_layout".to_string(),
                created_at_ms: now,
                updated_at_ms: now,
            });
        let mut profile = default_profile(email);
        profile.onboarding_complete = true;
        profile.source_resume_name = source.file_name.clone();
        profile.source_resume_asset_id = source.id.clone();
        profile.source_resume_sha256 = source.sha256.clone();
        profile.source_resume_media_type = source.media_type.clone();
        profile.source_resume_template_status = source.template_status.clone();
        let (_, profile) = save_resume_source_asset(pool, account_id, &source, &profile).unwrap();
        let preferences = JobPreferences {
            sponsorship: "not_required".to_string(),
            ..JobPreferences::default()
        };
        let preferences = save_preferences(pool, account_id, &preferences).unwrap();
        let track = list_tracks(pool, account_id)
            .unwrap()
            .into_iter()
            .find(|track| track.id == track_id)
            .expect("execution fixture Career Track exists");
        let track = upsert_track(pool, account_id, &track).unwrap();
        assert_eq!(track.policy.authority.review_state, "approved");
        assert!(track.policy.authority.policy_revision_no > 0);
        (profile, preferences)
    }

    fn append_account_operational_hold(
        pool: &DbPool,
        capability: OperationalCapability,
        event_id: &str,
        transition: OperationalHoldTransition,
        revision: i64,
        predecessor: Option<&str>,
    ) {
        append_operational_hold_for_scope(
            pool,
            capability,
            OperationalHoldScopeKind::Account,
            "acct-jobs",
            event_id,
            transition,
            revision,
            predecessor,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn append_operational_hold_for_scope(
        pool: &DbPool,
        capability: OperationalCapability,
        scope_kind: OperationalHoldScopeKind,
        scope_id: &str,
        event_id: &str,
        transition: OperationalHoldTransition,
        revision: i64,
        predecessor: Option<&str>,
    ) {
        append_operational_hold_event(
            pool,
            &AppendOperationalHoldEventRequest {
                event_id: event_id.to_string(),
                capability,
                scope_kind,
                scope_id: scope_id.to_string(),
                transition,
                reason_code: if transition == OperationalHoldTransition::Held {
                    OperationalHoldReasonCode::Incident
                } else {
                    OperationalHoldReasonCode::ManualRelease
                },
                reason_ref: Some("INC-606".to_string()),
                expected_head_revision: revision,
                expected_current_event_id: predecessor.map(str::to_string),
            },
            "admin-606",
        )
        .unwrap();
    }

    fn assert_account_deletion_fence<T>(result: Result<T>) {
        match result {
            Err(error) => assert!(matches!(
                error.downcast_ref::<UploadControlError>(),
                Some(UploadControlError::AccountDeleting)
            )),
            Ok(_) => panic!("account-deletion fence unexpectedly allowed the operation"),
        }
    }

    fn reserve_verified_jobs_account_object(
        pool: &DbPool,
        logical_id: &str,
        object_key: &str,
        sha256: &str,
        content_type: &str,
        size_bytes: i64,
        metadata_json: serde_json::Value,
    ) -> crate::db::object_uploads::ObjectUpload {
        let now = now_ms();
        let reservation = crate::db::object_uploads::reserve_account_object_upload(
            pool,
            &NewObjectUpload {
                account_id: "acct-jobs".to_string(),
                object_kind: ObjectKind::Artifact,
                logical_id: logical_id.to_string(),
                session_id: None,
                storage_scope: StorageScope::Artifact,
                object_key: object_key.to_string(),
                size_bytes,
                sha256: sha256.to_string(),
                content_type: content_type.to_string(),
                expires_at_ms: i64::MAX,
                metadata_json,
                now_ms: now,
                limits: UploadLimits {
                    max_object_bytes: 1024 * 1024,
                    max_account_bytes: 16 * 1024 * 1024,
                    max_daily_bytes: 16 * 1024 * 1024,
                    max_account_objects: 100,
                },
            },
        )
        .unwrap();
        assert!(reservation.needs_put);
        crate::db::object_uploads::begin_upload_put(pool, &reservation.upload.id, now).unwrap();
        crate::db::object_uploads::release_verified_upload_put(pool, &reservation.upload.id, now)
            .unwrap();
        assert_eq!(
            crate::db::object_uploads::artifact_upload(pool, "acct-jobs", logical_id)
                .unwrap()
                .unwrap()
                .state,
            "pending"
        );
        reservation.upload
    }

    fn reserve_verified_resume_source_upload_with_requested_digest(
        pool: &DbPool,
        asset_id: &str,
        file_name: &str,
        sha256: &str,
        size_bytes: i64,
        profile_digests: (Option<&str>, &str),
        replaces_source_asset_id: Option<&str>,
    ) -> (crate::db::object_uploads::ObjectUpload, ResumeSourceAsset) {
        let (base_profile_sha256, requested_profile_sha256) = profile_digests;
        let logical_id = format!("jobs-resume-source:{asset_id}");
        let object_key = format!("accounts/acct-jobs/jobs/resume-sources/{asset_id}.pdf");
        let upload = reserve_verified_jobs_account_object(
            pool,
            &logical_id,
            &object_key,
            sha256,
            "application/pdf",
            size_bytes,
            json!({
                "artifact_class": "jobs_resume_source",
                "jobs_resume_source_asset_id": asset_id,
                "request_id": asset_id,
                "profile_mode": "replace",
                "base_profile_sha256": base_profile_sha256,
                "requested_profile_sha256": requested_profile_sha256,
                "replaces_source_asset_id": replaces_source_asset_id,
                "file_name": file_name,
                "file_type": "pdf",
                "media_type": "application/pdf",
                "page_count": 2,
                "retention_policy": "account_lifetime_until_deletion",
            }),
        );
        let asset = ResumeSourceAsset {
            id: asset_id.to_string(),
            file_name: file_name.to_string(),
            media_type: upload.content_type.clone(),
            file_type: "pdf".to_string(),
            storage_key: upload.object_key.clone(),
            sha256: upload.sha256.clone(),
            size_bytes: upload.size_bytes,
            page_count: Some(2),
            template_status: "converted_layout".to_string(),
            created_at_ms: upload.created_at_ms,
            updated_at_ms: upload.created_at_ms,
        };
        (upload, asset)
    }

    fn reserve_verified_resume_source_upload(
        pool: &DbPool,
        asset_id: &str,
        file_name: &str,
        sha256: &str,
        size_bytes: i64,
        profiles: (Option<&CareerProfile>, &CareerProfile),
        replaces_source_asset_id: Option<&str>,
    ) -> (crate::db::object_uploads::ObjectUpload, ResumeSourceAsset) {
        let (base_profile, requested_profile) = profiles;
        let base_profile_sha256 = base_profile
            .map(resume_profile_revision)
            .transpose()
            .unwrap();
        let requested_profile_sha256 = resume_requested_profile_sha256(requested_profile).unwrap();
        reserve_verified_resume_source_upload_with_requested_digest(
            pool,
            asset_id,
            file_name,
            sha256,
            size_bytes,
            (base_profile_sha256.as_deref(), &requested_profile_sha256),
            replaces_source_asset_id,
        )
    }

    fn reserve_verified_browser_profile_upload(
        pool: &DbPool,
        application_id: &str,
        run_id: &str,
        browser_profile_id: &str,
        writer_fence: i64,
        generations: (i64, i64),
        object: (&str, &str, i64, i64),
    ) -> crate::db::object_uploads::ObjectUpload {
        let (expected_generation, next_generation) = generations;
        let (object_key, sha256, size_bytes, envelope_version) = object;
        let logical_id = format!(
            "jobs-browser-profile:{browser_profile_id}:{next_generation}:{envelope_version}:{sha256}"
        );
        reserve_verified_jobs_account_object(
            pool,
            &logical_id,
            object_key,
            sha256,
            "application/vnd.bluey.browser-profile+encrypted",
            size_bytes,
            json!({
                "artifact_class": "jobs_browser_profile_snapshot",
                "jobs_browser_profile_id": browser_profile_id,
                "jobs_application_id": application_id,
                "jobs_run_id": run_id,
                "generation": next_generation,
                "expected_generation": expected_generation,
                "writer_fence": writer_fence,
                "envelope_version": envelope_version,
                "retention_policy": "account_lifetime_until_deletion",
            }),
        )
    }

    fn requested_resume_profile(headline: &str) -> CareerProfile {
        let mut profile = default_profile("jobs@example.com");
        profile.headline = headline.to_string();
        profile.onboarding_complete = true;
        profile
    }

    fn assert_resume_idempotency_conflict(result: Result<ResumeSourcePublication>) {
        let error = result.expect_err("publication must fail closed");
        assert_eq!(
            error.downcast_ref::<UploadControlError>(),
            Some(&UploadControlError::IdempotencyConflict)
        );
    }

    fn upload_lifecycle(pool: &DbPool, upload_id: &str) -> (String, String, i64) {
        pool.get()
            .unwrap()
            .query_row(
                "SELECT upload.state, put.state,
                        (SELECT COUNT(*) FROM object_storage_outbox deletion
                          WHERE deletion.upload_id = upload.id
                            AND deletion.operation = 'delete')
                   FROM object_uploads upload
                   JOIN object_storage_outbox put
                     ON put.upload_id = upload.id AND put.operation = 'put'
                  WHERE upload.id = ?1",
                params![upload_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap()
    }

    fn test_posting(url: &str, posted_at_ms: i64, last_verified_at_ms: i64) -> JobPosting {
        verified_test_posting(
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
                discovery_evidence: JobDiscoveryEvidence::default(),
                eligibility: None,
            },
            last_verified_at_ms,
        )
    }

    fn verified_test_posting(mut posting: JobPosting, checked_at_ms: i64) -> JobPosting {
        posting.canonical_key = canonical_job_key(&posting);
        let application_domain = reqwest::Url::parse(&posting.canonical_url)
            .ok()
            .and_then(|parsed| parsed.host_str().map(str::to_string));
        posting.discovery_evidence = JobDiscoveryEvidence::provider_verified_original_source(
            posting.canonical_key.clone(),
            format!("{}:test", posting.source),
            application_domain,
            checked_at_ms,
            "a".repeat(64),
        );
        posting
    }

    fn discovery_decision(
        posting: &JobPosting,
        require_live_verification: bool,
    ) -> JobEligibilityDecision {
        let profile = default_profile("jobs@example.com");
        let preferences = JobPreferences::default();
        let mut track = CareerTrack {
            id: "track-default".to_string(),
            name: "Software engineering".to_string(),
            role: "Software Engineer".to_string(),
            locations: vec!["New York, NY".to_string()],
            remote_preference: "hybrid_ok".to_string(),
            application_identity_id: Some("identity-primary".to_string()),
            policy: CareerTrackPolicy {
                role_family: "software_engineering".to_string(),
                ..CareerTrackPolicy::default()
            },
            active: true,
            match_count: 0,
            created_at_ms: 0,
            updated_at_ms: 0,
        };
        track.policy.authority.review_state = "approved".to_string();
        track.policy.authority.taxonomy_version = crate::jobs_taxonomy::taxonomy_version().into();
        track.policy.authority.taxonomy_sha256 = crate::jobs_taxonomy::taxonomy_sha256();
        track.policy.authority.source_resume_asset_id = profile.source_resume_asset_id.clone();
        track.policy.authority.source_resume_sha256 = profile.source_resume_sha256.clone();
        track.policy.authority.job_preferences_sha256 =
            job_preferences_policy_sha256(&preferences).expect("canonical test preferences");
        build_job_eligibility(
            posting,
            &profile,
            &preferences,
            &[],
            require_live_verification,
            None,
            Some(&track),
        )
    }

    #[test]
    fn discovery_writers_share_one_postgres_account_lock_namespace() {
        assert_eq!(
            DISCOVERY_ACCOUNT_LOCK_SQL,
            "SELECT pg_advisory_xact_lock(hashtextextended('jobs-discovery-account:' || $1, 0))"
        );
    }

    #[test]
    fn postgres_auto_submit_execution_and_revocation_share_one_lock_order() {
        fn assert_ordered(source: &str, needles: &[&str], label: &str) {
            let mut previous = 0;
            for needle in needles {
                let position = source
                    .find(needle)
                    .unwrap_or_else(|| panic!("missing {needle:?} in {label}"));
                assert!(position >= previous, "{label} lock order is inverted");
                previous = position;
            }
        }

        let execution_source = include_str!("execution_authority.rs");
        let prelock = execution_source
            .split("fn lock_current_execution_authority_postgres_after_prelock(")
            .nth(1)
            .expect("PostgreSQL execution authority prelock implementation")
            .split("fn resolve_current_execution_authority_postgres_after_prelock_at_ms(")
            .next()
            .expect("bounded PostgreSQL execution authority prelock implementation");
        for forbidden_relock in [
            "lock_operational_hold_shared_postgres_tx",
            "lock_managed_cloud_release_registry_shared_postgres_tx",
            "lock_postgres_ats_certification",
            "lock_discovery_account_shared_postgres",
        ] {
            assert!(
                !prelock.contains(forbidden_relock),
                "execution authority prelock reacquires {forbidden_relock}"
            );
        }
        assert_ordered(
            prelock,
            &[
                "lock_job_integrity_publication_fence_shared_postgres_tx(tx)",
                "lock_account_policy_inputs_postgres(tx, account_id, false)",
                "lock_auto_submit_authority_postgres(tx, account_id, &track.id, false)",
            ],
            "PostgreSQL execution authority prelock",
        );

        let resolver = execution_source
            .split("fn resolve_current_execution_authority_postgres_after_prelock(")
            .nth(1)
            .expect("PostgreSQL execution authority resolver")
            .split("fn lock_current_execution_authority_postgres_after_prelock(")
            .next()
            .expect("bounded PostgreSQL execution authority resolver");
        assert_ordered(
            resolver,
            &[
                "lock_current_execution_authority_postgres_after_prelock(",
                "original_source_db_now_postgres(tx)",
                "resolve_current_execution_authority_postgres_after_prelock_at_ms(",
            ],
            "PostgreSQL execution authority clock",
        );

        let at_ms = execution_source
            .split("fn resolve_current_execution_authority_postgres_after_prelock_at_ms(")
            .nth(1)
            .expect("PostgreSQL execution authority at-ms resolver")
            .split("fn current_execution_authority_matches(")
            .next()
            .expect("bounded PostgreSQL execution authority at-ms resolver");
        for forbidden_lock in [
            "lock_job_integrity_publication_fence_shared_postgres_tx",
            "lock_account_policy_inputs_postgres",
            "lock_auto_submit_authority_postgres",
        ] {
            assert!(!at_ms.contains(forbidden_lock));
        }
        assert!(!at_ms.contains("FOR SHARE"));
        assert!(!at_ms.contains("FOR UPDATE"));
        assert!(at_ms.contains("validate_track_policy_ledger_postgres"));
        assert!(
            !at_ms.contains("resolve_composed_job_integrity_projection_postgres_tx_after_prelock(")
        );
        assert!(at_ms.contains(
            "resolve_composed_job_integrity_projection_postgres_tx_after_prelock_at_ms("
        ));
        let authorization_query = at_ms
            .split("let auto_submit_authorization =")
            .nth(1)
            .expect("bounded PostgreSQL active Auto-submit authority query")
            .split("let preferences =")
            .next()
            .expect("bounded PostgreSQL active Auto-submit authority query");
        assert!(authorization_query.contains("FROM jobs_auto_submit_authorizations"));
        assert!(!authorization_query.contains("FOR SHARE"));

        let revoke = include_str!("auto_submit.rs")
            .split("pub fn revoke_auto_submit(")
            .nth(1)
            .expect("Auto-submit revocation implementation")
            .split("pub fn require_valid_auto_submit_authorization(")
            .next()
            .expect("bounded Auto-submit revocation implementation")
            .split("DbPool::Postgres(_) =>")
            .nth(1)
            .expect("PostgreSQL Auto-submit revocation implementation");
        assert_ordered(
            revoke,
            &[
                "let mut tx = conn.transaction()?",
                "lock_discovery_account_shared_postgres(&mut tx, account_id)",
                "lock_account_policy_inputs_postgres(&mut tx, account_id, false)",
                "lock_auto_submit_authority_postgres(&mut tx, account_id, track_id, true)",
                "UPDATE jobs_auto_submit_authorizations",
                "tx.commit()?",
            ],
            "PostgreSQL Auto-submit revocation",
        );
    }

    #[test]
    fn postgres_browser_release_readers_lock_registry_before_authority_checks() {
        let claim_source = include_str!("browser_release_authority.rs");
        let claim = claim_source
            .split("fn postgres_claim_local_run_with_browser_release")
            .nth(1)
            .expect("PostgreSQL Browser claim implementation")
            .split("fn postgres_lock_browser_release_registry_shared")
            .next()
            .expect("bounded PostgreSQL Browser claim implementation");
        let ticket = claim
            .find("postgres_local_run_authority_prelock")
            .expect("ticket authority before registry lock");
        let shared_lock = claim
            .find("postgres_lock_browser_release_registry_shared(tx)")
            .expect("shared registry lock");
        let db_time = claim
            .find("local_run_claim_db_now_postgres(tx)")
            .expect("claim database time after registry lock");
        let current = claim
            .find("postgres_local_run_authority_after_prelock_at_ms")
            .expect("current ticket authority after database time");
        let release = claim
            .find("postgres_browser_release_for_claim_tx")
            .expect("release authority after registry lock");
        assert!(ticket < shared_lock && shared_lock < db_time);
        assert!(db_time < current && current < release);

        let submit_source = include_str!("local_runner.rs");
        let submit = submit_source
            .rsplit("fn local_run_submit_authorization_inner(")
            .next()
            .expect("pre-click submit implementation")
            .split("fn local_click_started_ticket_matches(")
            .next()
            .expect("bounded pre-click submit implementation")
            .split("DbPool::Postgres(_) =>")
            .nth(1)
            .expect("PostgreSQL pre-click submit implementation");
        let ticket = submit
            .find("postgres_local_run_authority_prelock")
            .expect("ticket authority before registry lock");
        let ticket_identity = submit
            .find("SELECT account_id, status FROM jobs_local_run_tickets")
            .expect("local submit ticket identity");
        let public_beta = submit
            .find("public_beta_effect_authorized_postgres_tx")
            .expect("local submit public-beta cohort/account prelock");
        let discovery_lock = submit
            .find("lock_discovery_account_shared_postgres(&mut tx, &capacity.account_id)")
            .expect("shared discovery-account lock");
        let account_fence = submit
            .find("require_active_account_write_fence_postgres_tx")
            .expect("active account write fence");
        let shared_lock = submit
            .find("postgres_lock_browser_release_registry_shared(&mut tx)")
            .expect("shared registry lock");
        let capacity_prelock = submit
            .find("prelock_local_submission_evidence_capacity_postgres_tx")
            .expect("capacity prelock after release registry lock");
        let db_time = submit
            .find("local_run_claim_db_now_postgres(&mut tx)")
            .expect("submit database time after authority prelocks");
        let current = submit
            .find("postgres_local_run_authority_after_prelock_at_ms")
            .expect("current submit authority after database time");
        let release = submit
            .find("postgres_bound_browser_release_submit_allowed_at_ms")
            .expect("bound release authority after registry lock");
        let capacity = submit
            .find("reserve_submission_evidence_capacity_postgres_tx")
            .expect("capacity reservation after release authority");
        assert!(ticket_identity < public_beta && public_beta < discovery_lock);
        assert!(discovery_lock < account_fence && account_fence < ticket);
        assert!(ticket < shared_lock && shared_lock < capacity_prelock);
        assert!(capacity_prelock < db_time && db_time < current);
        assert!(current < release && release < capacity);

        let replay = submit_source
            .rsplit("fn postgres_local_click_started_submit_replay")
            .next()
            .expect("PostgreSQL click-started replay implementation")
            .split("fn local_ats_observed_surface")
            .next()
            .expect("bounded PostgreSQL click-started replay implementation");
        let ticket_lock = replay
            .find("FOR UPDATE")
            .expect("click-started ticket lock");
        let shared_lock = replay
            .find("postgres_lock_browser_release_registry_shared(tx)")
            .expect("click-started release-registry lock");
        let release = replay
            .find("postgres_local_click_started_release_matches")
            .expect("click-started frozen release authority");
        let ats = replay
            .find("recover_terminal_ats_authority_postgres_tx")
            .expect("click-started terminal ATS authority");
        let application = replay
            .find("SELECT job_id, application_json, state FROM jobs_applications")
            .expect("click-started application lock");
        let session = replay
            .find("SELECT session_json, runner, status FROM jobs_browser_sessions")
            .expect("click-started session lock");
        let capacity = replay
            .find("postgres_local_click_started_capacity_expires_at_ms")
            .expect("click-started capacity authority");
        let db_time = replay
            .find("local_run_claim_db_now_postgres")
            .expect("click-started database time");
        let expiry = replay
            .find("capacity_expires_at_ms <= now")
            .expect("click-started capacity expiry check");
        assert!(
            ticket_lock < shared_lock
                && shared_lock < release
                && release < ats
                && ats < application
                && application < session
                && session < capacity
                && capacity < db_time
                && db_time < expiry
        );

        let availability = claim_source
            .split("pub fn local_browser_release_availability")
            .nth(1)
            .expect("Browser release availability implementation")
            .split("fn sqlite_local_browser_release_availability")
            .next()
            .expect("bounded Browser release availability implementation");
        assert!(availability.contains("transaction_with_behavior(TransactionBehavior::Immediate)",));
        let shared_lock = availability
            .find("postgres_lock_browser_release_registry_shared(&mut transaction)")
            .expect("availability shared registry lock");
        let release = availability
            .find("postgres_local_browser_release_availability(")
            .expect("availability query after registry lock");
        assert!(shared_lock < release);
    }

    #[test]
    fn browser_claim_phase_a_is_atomic_and_has_dialect_parity() {
        let source = include_str!("browser_release_authority.rs");
        for (start, end, helper) in [
            (
                "fn sqlite_claim_local_run_with_browser_release",
                "fn sqlite_browser_claim_replay",
                "create_ats_application_certification_binding_from_context_sqlite_tx",
            ),
            (
                "fn postgres_claim_local_run_with_browser_release",
                "fn postgres_lock_browser_release_registry_shared",
                "create_ats_application_certification_binding_from_context_postgres_tx",
            ),
        ] {
            let claim = source
                .split(start)
                .nth(1)
                .expect("Browser claim implementation")
                .split(end)
                .next()
                .expect("bounded Browser claim implementation");
            let state_mutation = claim
                .find("UPDATE jobs_browser_sessions SET status = 'running'")
                .expect("claim state mutation");
            let phase_a = claim.find(helper).expect("certified Phase A helper");
            let claim_event = claim
                .find("'local_browser_claimed'")
                .expect("claim event after Phase A");
            assert!(state_mutation < phase_a && phase_a < claim_event);
        }
    }

    #[test]
    fn postgres_ats_lock_precedes_all_embedded_phase_a_and_phase_b_row_locks() {
        let execution_source = include_str!("execution_leases.rs");
        let cloud_phase_a = execution_source
            .split("fn claim_execution_lease_inner(")
            .nth(1)
            .expect("cloud execution-lease claim")
            .split("fn execution_lease_from_runner_volume_error")
            .next()
            .expect("bounded cloud execution-lease claim");
        let cloud_phase_a_lock = cloud_phase_a
            .find("lock_postgres_ats_certification(&mut tx)")
            .expect("cloud Phase A ATS advisory lock");
        let cloud_phase_a_first_row = cloud_phase_a
            .find("prepare_runner_volume_lease_binding_postgres_tx")
            .expect("cloud Phase A runner row authority");
        assert!(cloud_phase_a_lock < cloud_phase_a_first_row);

        let cloud_phase_b = execution_source
            .split("pub fn start_irreversible_submission(")
            .nth(1)
            .expect("cloud irreversible transition")
            .split("fn execution_finish_allowed")
            .next()
            .expect("bounded cloud irreversible transition");
        let cloud_phase_b_lock = cloud_phase_b
            .find("lock_postgres_ats_certification(&mut tx)")
            .expect("cloud Phase B ATS advisory lock");
        let cloud_phase_b_first_row = cloud_phase_b
            .find("require_active_account_write_fence_postgres_tx")
            .expect("cloud Phase B account row authority");
        assert!(cloud_phase_b_lock < cloud_phase_b_first_row);

        let local_claim_source = include_str!("browser_release_authority.rs");
        let local_phase_a = local_claim_source
            .split("fn claim_local_run_with_browser_release_inner")
            .nth(1)
            .expect("local Browser claim transaction")
            .split("fn browser_claim_request_sha256")
            .next()
            .expect("bounded local Browser claim transaction");
        let local_phase_a_lock = local_phase_a
            .find("lock_postgres_ats_certification(&mut transaction)")
            .expect("local Phase A ATS advisory lock");
        let local_phase_a_first_row = local_phase_a
            .find("postgres_claim_local_run_with_browser_release")
            .expect("local Phase A row authority");
        assert!(local_phase_a_lock < local_phase_a_first_row);

        let local_submit_source = include_str!("local_runner.rs");
        let local_phase_b = local_submit_source
            .rsplit("fn local_run_submit_authorization_inner(")
            .next()
            .expect("local pre-click transaction")
            .split("fn local_ats_observed_surface")
            .next()
            .expect("bounded local pre-click transaction");
        let local_phase_b_lock = local_phase_b
            .find("lock_postgres_ats_certification(&mut tx)")
            .expect("local Phase B ATS advisory lock");
        let local_phase_b_first_row = local_phase_b
            .find("SELECT account_id, status FROM jobs_local_run_tickets")
            .expect("local Phase B ticket row authority");
        assert!(local_phase_b_lock < local_phase_b_first_row);
    }

    #[test]
    fn postgres_operational_hold_lock_is_first_in_every_protected_admission() {
        fn operation<'a>(source: &'a str, start: &str, end: &str, label: &str) -> &'a str {
            source
                .split(start)
                .nth(1)
                .unwrap_or_else(|| panic!("missing {label} start"))
                .split(end)
                .next()
                .unwrap_or_else(|| panic!("missing {label} end"))
        }

        fn assert_first_lock(
            operation: &str,
            begin: &str,
            hold_lock: &str,
            first_protected_lock: &str,
            label: &str,
        ) {
            let begin_position = operation
                .find(begin)
                .unwrap_or_else(|| panic!("missing {label} PostgreSQL transaction"));
            let after_begin = begin_position + begin.len();
            let hold = after_begin
                + operation[after_begin..]
                    .find(hold_lock)
                    .unwrap_or_else(|| panic!("missing {label} operational-hold lock"));
            let prefix = &operation[after_begin..hold];
            assert!(
                prefix.trim().is_empty(),
                "{label} operational-hold lock is not the exact first transaction operation: \
                 {prefix:?}"
            );
            let protected = after_begin
                + operation[after_begin..]
                    .find(first_protected_lock)
                    .unwrap_or_else(|| panic!("missing {label} protected lock"));
            assert!(hold < protected, "{label} lock order is inverted");
        }

        fn assert_managed_prelock_is_first_statement(
            operation: &str,
            fresh_path_start: &str,
            label: &str,
        ) {
            let start_position = operation
                .find(fresh_path_start)
                .unwrap_or_else(|| panic!("missing {label} fresh-path start"));
            let after_start = start_position + fresh_path_start.len();
            let managed_prelock = after_start
                + operation[after_start..]
                    .find("lock_managed_cloud_workflow_admission_postgres_tx")
                    .unwrap_or_else(|| panic!("missing {label} managed-cloud prelock"));
            let prefix = &operation[after_start..managed_prelock];
            assert!(
                !prefix.contains(';'),
                "{label} performs work before prelock"
            );
            for forbidden in ["lock_", ".query_", ".execute(", "FOR UPDATE", "FOR SHARE"] {
                assert!(
                    !prefix.contains(forbidden),
                    "{label} acquires protected authority before managed prelock"
                );
            }
        }

        let managed_cloud = include_str!("managed_cloud_release_authority.rs");
        let managed_prelock = operation(
            managed_cloud,
            "pub(crate) fn lock_managed_cloud_workflow_admission_postgres_tx(",
            "fn sqlite_managed_cloud_command_marker(",
            "managed-cloud workflow admission prelock",
        );
        let managed_prelock_body = managed_prelock
            .split_once('{')
            .expect("managed-cloud workflow admission prelock body")
            .1;
        let hold = managed_prelock_body
            .find("lock_operational_hold_shared_postgres_tx")
            .expect("managed-cloud prelock operational hold");
        let registry = managed_prelock_body
            .find("lock_managed_cloud_release_registry_postgres_tx")
            .expect("managed-cloud prelock registry");
        let ats = managed_prelock_body
            .find("lock_postgres_ats_certification")
            .expect("managed-cloud prelock ATS");
        let fleet = managed_prelock_body
            .find("jobs_runner_volume_fleet_state")
            .expect("managed-cloud prelock fleet");
        for forbidden in ["lock_", ".query_", ".execute(", "FOR UPDATE", "FOR SHARE"] {
            assert!(
                !managed_prelock_body[..hold].contains(forbidden),
                "managed-cloud prelock acquires protected authority before its hold lock"
            );
        }
        assert!(hold < registry && registry < ats && ats < fleet);

        let browser = include_str!("browser_release_authority.rs");
        assert_first_lock(
            operation(
                browser,
                "fn claim_local_run_with_browser_release_inner",
                "fn browser_claim_request_sha256",
                "local Browser claim",
            ),
            "let mut transaction = connection.transaction()?;",
            "lock_operational_hold_shared_postgres_tx(&mut transaction)",
            "lock_postgres_ats_certification(&mut transaction)",
            "local Browser claim",
        );

        let discovery = include_str!("discovery.rs");
        assert_first_lock(
            operation(
                discovery,
                "pub fn lease_due_discovery_source",
                "fn discovery_lease_token_hash",
                "direct discovery lease",
            ),
            "let mut tx = conn.transaction()?;",
            "lock_operational_hold_shared_postgres_tx(&mut tx)",
            "SELECT id, account_id, track_id",
            "direct discovery lease",
        );

        let global_discovery = include_str!("global_discovery.rs");
        assert_first_lock(
            operation(
                global_discovery,
                "pub fn lease_due_global_discovery_source",
                "pub fn ingest_global_discovery_batch",
                "global discovery lease",
            ),
            "let mut tx = conn.transaction()?;",
            "lock_operational_hold_shared_postgres_tx(&mut tx)",
            "SELECT id, provider, source_key",
            "global discovery lease",
        );

        let local_runner = include_str!("local_runner.rs");
        let local_final_submit = local_runner
            .rsplit_once("fn local_run_submit_authorization_inner(")
            .expect("local final-submit admission start")
            .1
            .split_once("fn local_click_started_ticket_matches(")
            .expect("local final-submit admission end")
            .0;
        assert_first_lock(
            local_final_submit,
            "let mut tx = conn.transaction()?;",
            "lock_operational_hold_shared_postgres_tx(&mut tx)",
            "lock_postgres_ats_certification(&mut tx)",
            "local final-submit admission",
        );
        let local_submit = local_final_submit;
        let (local_submit_sqlite, local_submit_postgres) = local_submit
            .split_once("DbPool::Postgres(_) =>")
            .expect("local final-submit SQLite/PostgreSQL branches");
        for (branch, replay, hold_evaluation) in [
            (
                local_submit_sqlite,
                "sqlite_local_click_started_submit_replay",
                "require_local_run_operational_capability_sqlite_after_authority",
            ),
            (
                local_submit_postgres,
                "postgres_local_click_started_submit_replay",
                "require_local_run_operational_capability_postgres_after_authority_prelock",
            ),
        ] {
            assert!(
                branch.find(replay).unwrap() < branch.find(hold_evaluation).unwrap(),
                "durable local click-started replay must remain before hold evaluation"
            );
        }

        let execution = include_str!("execution_leases.rs");
        assert_managed_prelock_is_first_statement(
            operation(
                execution,
                "fn claim_execution_lease_inner(",
                "fn execution_lease_from_runner_volume_error",
                "cloud runner claim",
            ),
            "let mut tx = conn.transaction()?;",
            "cloud runner claim",
        );
        let cloud_submit = operation(
            execution,
            "pub fn start_irreversible_submission(",
            "fn execution_finish_allowed",
            "cloud final-submit admission",
        );
        assert_managed_prelock_is_first_statement(
            cloud_submit,
            "if let Some((Some(input), _)) = managed_cloud_context {",
            "cloud final-submit admission",
        );
        let cloud_submit_postgres = cloud_submit
            .split("DbPool::Postgres(_) =>")
            .nth(1)
            .expect("PostgreSQL cloud final-submit admission");
        assert!(
            cloud_submit_postgres.find("if discovered_lease").unwrap()
                < cloud_submit_postgres
                    .find("operational_hold_context_for_application_postgres_tx")
                    .unwrap(),
            "durable cloud click-started replay must remain before hold evaluation"
        );

        let eligibility = include_str!("eligibility.rs");
        assert_first_lock(
            operation(
                eligibility,
                "pub fn reserve_application_attempt(",
                "pub fn update_attempt_reservation_status(",
                "application reservation",
            ),
            "let mut tx = conn.transaction()?;",
            "lock_operational_hold_shared_postgres_tx(&mut tx)",
            "SELECT account_id FROM jobs_entitlements",
            "application reservation",
        );

        let provider_cost = include_str!("../jobs_provider_cost_holds.rs");
        assert_first_lock(
            operation(
                provider_cost,
                "pub fn reserve(",
                "pub fn settle_with_usage(",
                "managed generation provider reservation",
            ),
            "let mut tx = conn.transaction()?;",
            "super::jobs::lock_operational_hold_shared_postgres_tx(&mut tx)",
            "SELECT reservation_token, status, job_id",
            "managed generation provider reservation",
        );

        let communication = include_str!("communication_actions.rs");
        assert_first_lock(
            operation(
                communication,
                "pub fn claim_communication_action(",
                "fn validate_communication_lease_binding(",
                "communication dispatch claim",
            ),
            "let mut tx = conn.transaction()?;",
            "lock_operational_hold_shared_postgres_tx(&mut tx)",
            "SELECT id, account_id FROM jobs_communication_actions",
            "communication dispatch claim",
        );
        let request_start = operation(
            communication,
            "pub fn mark_communication_action_request_started(",
            "pub fn finish_communication_action(",
            "communication request-start marker",
        );
        assert_first_lock(
            request_start,
            "let mut tx = conn.transaction()?;",
            "lock_operational_hold_shared_postgres_tx(&mut tx)",
            "require_active_account_write_fence_postgres_tx",
            "communication request-start marker",
        );
        let request_start_postgres = request_start
            .split("DbPool::Postgres(_) =>")
            .nth(1)
            .expect("PostgreSQL communication request-start marker");
        assert!(
            request_start_postgres
                .find("existing_evidence_sha256")
                .unwrap()
                < request_start_postgres
                    .find("communication_dispatch_is_held_postgres_tx")
                    .unwrap(),
            "durable communication request-start replay must remain before hold evaluation"
        );
    }

    #[test]
    fn postgres_operational_context_account_fence_covers_scope_snapshot_and_writers() {
        let jobs_source = include_str!("../jobs.rs");
        assert!(jobs_source.contains(
            "pg_advisory_xact_lock(hashtextextended('jobs-discovery-account:' || $1, 0))"
        ));
        assert!(jobs_source.contains(
            "pg_advisory_xact_lock_shared(hashtextextended('jobs-discovery-account:' || $1, 0))"
        ));

        let holds = include_str!("operational_holds.rs");
        let section = |start: &str, end: &str, label: &str| {
            holds
                .split_once(start)
                .unwrap_or_else(|| panic!("missing {label} start"))
                .1
                .split_once(end)
                .unwrap_or_else(|| panic!("missing {label} end"))
                .0
        };
        for (legacy_start, after_prelock_start, implementation_start, end, label) in [
            (
                "pub(crate) fn operational_hold_context_for_application_postgres_tx(",
                "pub fn operational_hold_context_for_application_postgres_tx_after_authority_prelock(",
                "fn operational_hold_context_for_application_postgres_tx_impl(",
                "pub(crate) fn operational_hold_context_for_job_sqlite_tx(",
                "application context",
            ),
            (
                "pub(crate) fn operational_hold_context_for_job_postgres_tx(",
                "pub fn operational_hold_context_for_job_postgres_tx_after_authority_prelock(",
                "fn operational_hold_context_for_job_postgres_tx_impl(",
                "pub(crate) fn operational_hold_context_for_mailbox_sqlite_tx(",
                "job context",
            ),
        ] {
            let legacy = section(
                legacy_start,
                after_prelock_start,
                &format!("legacy PostgreSQL {label}"),
            );
            let fence = legacy
                .find("lock_discovery_account_shared_postgres")
                .unwrap_or_else(|| panic!("missing shared account fence in legacy {label}"));
            let implementation_call = legacy
                .find(implementation_start.trim_start_matches("fn ").trim_end_matches('('))
                .unwrap_or_else(|| panic!("missing implementation call in legacy {label}"));
            assert!(
                fence < implementation_call,
                "legacy {label} delegates before its account fence"
            );

            let after_prelock = section(
                after_prelock_start,
                implementation_start,
                &format!("after-prelock PostgreSQL {label}"),
            );
            assert!(after_prelock.contains("Some(employer_domain)"));
            assert!(!after_prelock.contains("lock_discovery_account_shared_postgres("));
            assert!(!after_prelock.contains("lock_operational_hold_shared_postgres_tx("));

            let implementation = section(
                implementation_start,
                end,
                &format!("PostgreSQL {label} implementation"),
            );
            assert!(!implementation.contains("lock_discovery_account_shared_postgres("));
            assert!(!implementation.contains("lock_operational_hold_shared_postgres_tx("));
            assert!(
                implementation.contains("FROM jobs_postings"),
                "missing posting snapshot in {label}"
            );
            assert!(implementation.contains("FOR SHARE"));
            assert!(!implementation.contains("FOR UPDATE OF membership, source"));
        }

        let materialization = include_str!("global_materialization.rs")
            .split("fn persist_global_materialization(")
            .nth(1)
            .expect("global materialization persistence")
            .split("fn remove_global_materialization(")
            .next()
            .expect("bounded global materialization persistence");
        let postgres_materialization = materialization
            .split("DbPool::Postgres(_) =>")
            .nth(1)
            .expect("PostgreSQL global materialization persistence");
        let writer_fence = postgres_materialization
            .find("lock_discovery_account_postgres(&mut tx, account_id)")
            .expect("global materialization discovery-account writer fence");
        let membership_write = postgres_materialization
            .find("INSERT INTO jobs_discovery_memberships")
            .expect("global materialization membership write");
        assert!(writer_fence < membership_write);

        let eligibility = include_str!("eligibility.rs");
        let attempt_inputs = eligibility
            .split_once("fn load_attempt_authority_inputs_postgres_tx(")
            .expect("attempt authority PostgreSQL input loader")
            .1
            .split_once("fn require_attempt_runner_capability_sqlite_tx(")
            .expect("bounded attempt authority PostgreSQL input loader")
            .0;
        assert!(attempt_inputs.contains("FOR SHARE OF application, posting"));
        assert!(!attempt_inputs.contains("FOR UPDATE"));
    }

    #[test]
    fn postgres_intervention_answer_uses_pre_click_lock_order() {
        let source = include_str!("customer_data.rs");
        let answer = source
            .split("pub fn resolve_intervention_answer_for_review(")
            .nth(1)
            .expect("intervention answer transaction")
            .split("pub fn normalize_answer_memory_key")
            .next()
            .expect("bounded intervention answer transaction");
        let postgres = answer
            .split("DbPool::Postgres(_) =>")
            .nth(1)
            .expect("PostgreSQL intervention answer transaction");
        let advisory = postgres
            .find("lock_postgres_ats_certification(&mut tx)")
            .expect("ATS advisory lock");
        let first_row_lock = postgres.find("FOR UPDATE").expect("first row lock");
        let lease = postgres
            .find("SELECT phase FROM jobs_execution_leases")
            .expect("cloud lease lock");
        let local_ticket = postgres
            .find("SELECT status FROM jobs_local_run_tickets")
            .expect("local ticket lock");
        let binding = postgres
            .find("FROM jobs_application_ats_certification_bindings")
            .expect("ATS binding lock");
        let application = postgres
            .find("SELECT job_id, application_json FROM jobs_applications")
            .expect("application lock");
        let intervention = postgres
            .find("SELECT application_id, intervention_json FROM jobs_interventions")
            .expect("intervention lock");
        let invalidation = postgres
            .find("invalidate_ats_application_certification_binding_postgres_tx")
            .expect("ATS binding invalidation");
        assert!(advisory < lease && lease < first_row_lock && first_row_lock < local_ticket);
        assert!(
            advisory < lease
                && lease < local_ticket
                && local_ticket < binding
                && binding < application
                && application < intervention
                && intervention < invalidation
        );
        for (start, end, name) in [
            (lease, local_ticket, "cloud lease"),
            (local_ticket, binding, "local ticket"),
            (binding, application, "ATS binding"),
            (application, intervention, "application"),
            (intervention, invalidation, "intervention"),
        ] {
            assert!(
                postgres[start..end].contains("FOR UPDATE"),
                "{name} must be acquired with a row lock"
            );
        }
    }

    #[test]
    fn layout_drift_denial_commits_before_irreversible_submit_writes() {
        let local_source = include_str!("local_runner.rs");
        let local_submit = local_source
            .rsplit("fn local_run_submit_authorization_inner(")
            .next()
            .expect("local submit transaction")
            .split("fn local_click_started_ticket_matches")
            .next()
            .expect("bounded local submit transaction");
        let (local_sqlite, local_postgres) = local_submit
            .split_once("DbPool::Postgres(_) =>")
            .expect("local SQLite/PostgreSQL submit branches");
        for (branch, phase_b, capacity, proof, marker) in [
            (
                local_sqlite,
                "consume_local_ats_certification_sqlite_tx",
                "reserve_submission_evidence_capacity_sqlite_tx",
                "bind_final_submit_proof_sqlite_tx",
                "UPDATE jobs_local_run_tickets",
            ),
            (
                local_postgres,
                "consume_local_ats_certification_postgres_tx",
                "reserve_submission_evidence_capacity_postgres_tx",
                "bind_final_submit_proof_postgres_tx",
                "UPDATE jobs_local_run_tickets",
            ),
        ] {
            let phase_b = branch.find(phase_b).expect("local ATS Phase B");
            let denial = branch
                .find("LocalAtsCertificationConsume::LayoutDriftQuarantined")
                .expect("local committed layout-drift denial");
            let commit = denial
                + branch[denial..]
                    .find("tx.commit()?")
                    .expect("local safety commit");
            let capacity = branch.find(capacity).expect("local evidence capacity");
            let proof = branch.find(proof).expect("local final-submit proof");
            let marker = branch.find(marker).expect("local click marker");
            assert!(
                phase_b < denial
                    && denial < commit
                    && commit < capacity
                    && commit < proof
                    && capacity < marker
                    && proof < marker
            );
        }

        let cloud_source = include_str!("execution_leases.rs");
        let cloud_submit = cloud_source
            .split("pub fn start_irreversible_submission(")
            .nth(1)
            .expect("cloud submit transaction")
            .split("fn execution_finish_allowed")
            .next()
            .expect("bounded cloud submit transaction");
        let (cloud_sqlite, cloud_postgres) = cloud_submit
            .split_once("DbPool::Postgres(_) =>")
            .expect("cloud SQLite/PostgreSQL submit branches");
        for (branch, phase_b, capacity, proof, marker) in [
            (
                cloud_sqlite,
                "validate_consume_reserve_ats_application_certification_from_context_sqlite_tx",
                "reserve_submission_evidence_capacity_sqlite_tx",
                "bind_final_submit_proof_sqlite_tx",
                "UPDATE jobs_execution_leases",
            ),
            (
                cloud_postgres,
                "validate_consume_reserve_ats_application_certification_from_context_postgres_tx",
                "reserve_submission_evidence_capacity_postgres_tx",
                "bind_final_submit_proof_postgres_tx",
                "UPDATE jobs_execution_leases",
            ),
        ] {
            let phase_b = branch.find(phase_b).expect("cloud ATS Phase B");
            let denial = branch
                .find("AtsCertificationPhaseBTransactionOutcome::LayoutDriftQuarantined")
                .expect("cloud committed layout-drift denial");
            let commit = denial
                + branch[denial..]
                    .find("tx.commit()?")
                    .expect("cloud safety commit");
            let capacity = branch.find(capacity).expect("cloud evidence capacity");
            let proof = branch.find(proof).expect("cloud final-submit proof");
            let marker = branch.find(marker).expect("cloud click marker");
            assert!(
                phase_b < denial
                    && denial < commit
                    && commit < capacity
                    && commit < proof
                    && capacity < marker
                    && proof < marker
            );
        }
    }

    #[test]
    fn browser_release_origin_and_artifact_contract_have_sqlite_postgres_source_parity() {
        let authority = include_str!("browser_release_authority.rs");
        for (start, end, contract, origin) in [
            (
                "fn sqlite_local_browser_release_availability",
                "fn postgres_local_browser_release_availability",
                "browser_portal_artifact_set_complete",
                "browser_portal_policy_artifact_origin",
            ),
            (
                "fn postgres_local_browser_release_availability",
                "fn browser_portal_policy_artifact_origin",
                "browser_portal_artifact_set_complete",
                "browser_portal_policy_artifact_origin",
            ),
            (
                "fn sqlite_browser_release_for_claim_tx",
                "fn postgres_browser_release_for_claim_tx",
                "browser_release_claim_artifact_set_matches_policy",
                "browser_release_claim_policy_artifact_origin",
            ),
            (
                "fn postgres_browser_release_for_claim_tx",
                "fn browser_activation_accepts_server",
                "browser_release_claim_artifact_set_matches_policy",
                "browser_release_claim_policy_artifact_origin",
            ),
        ] {
            let operation = authority
                .split(start)
                .nth(1)
                .expect("Browser release authority implementation")
                .split(end)
                .next()
                .expect("bounded Browser release authority implementation");
            assert!(operation.contains("app_content_sha256"));
            assert!(operation.contains(contract));
            assert!(operation.contains(origin));
        }

        let registry = include_str!("browser_release_registry.rs");
        for (start, end) in [
            (
                "fn sqlite_browser_release_artifact_set_complete",
                "fn postgres_browser_release_artifact_set_complete",
            ),
            (
                "fn postgres_browser_release_artifact_set_complete",
                "fn sqlite_browser_release_channel_status_tx",
            ),
        ] {
            let operation = registry
                .split(start)
                .nth(1)
                .expect("Browser registry artifact implementation")
                .split(end)
                .next()
                .expect("bounded Browser registry artifact implementation");
            assert!(operation.contains("app_content_sha256"));
            assert!(operation.contains("artifact_url"));
            assert!(operation.contains("browser_artifact_contract_complete"));
        }
        assert_eq!(
            registry
                .matches(
                    "require_stored_browser_release_manifest_artifact_origin(&manifest, &policy)?",
                )
                .count(),
            4,
            "fresh SQLite/PostgreSQL activation import/apply paths must share the origin gate",
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
    fn replacing_source_resume_requires_auto_submit_review_again() {
        let pool = test_pool();
        let now = now_ms();
        let first_asset = ResumeSourceAsset {
            id: "resume-source-one".to_string(),
            file_name: "software-engineer-resume.docx".to_string(),
            media_type: "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
                .to_string(),
            file_type: "docx".to_string(),
            storage_key: "jobs/acct-jobs/resume-source-one".to_string(),
            sha256: "a".repeat(64),
            size_bytes: 1_024,
            page_count: Some(2),
            template_status: "exact_docx".to_string(),
            created_at_ms: now,
            updated_at_ms: now,
        };
        let mut profile = default_profile("jobs@example.com");
        profile.onboarding_complete = true;
        profile.source_resume_name = first_asset.file_name.clone();
        profile.source_resume_asset_id = first_asset.id.clone();
        profile.source_resume_sha256 = first_asset.sha256.clone();
        profile.source_resume_media_type = first_asset.media_type.clone();
        profile.source_resume_template_status = first_asset.template_status.clone();
        let (_, profile) =
            save_resume_source_asset(&pool, "acct-jobs", &first_asset, &profile).unwrap();
        let track = list_tracks(&pool, "acct-jobs").unwrap().remove(0);
        let track = upsert_track(&pool, "acct-jobs", &track).unwrap();
        assert_eq!(track.policy.authority.review_state, "approved");
        assert_eq!(track.policy.authority.policy_revision_no, 1);

        let authorization =
            authorize_auto_submit(&pool, "acct-jobs", "jobs@example.com", "track-default").unwrap();
        assert_eq!(authorization.status, "active");
        assert_eq!(authorization.source_resume_asset_id, first_asset.id);

        let mut changed_preferences = get_preferences(&pool, "acct-jobs").unwrap();
        changed_preferences
            .excluded_titles
            .push("Staffing-only role".to_string());
        save_preferences(&pool, "acct-jobs", &changed_preferences).unwrap();
        let changed_policy =
            list_auto_submit_authorizations(&pool, "acct-jobs", "jobs@example.com").unwrap();
        assert_eq!(changed_policy[0].status, "needs_review");
        assert!(
            authorize_auto_submit(&pool, "acct-jobs", "jobs@example.com", "track-default").is_err()
        );
        let changed_track = list_tracks(&pool, "acct-jobs").unwrap().remove(0);
        assert_eq!(changed_track.policy.authority.review_state, "needs_review");
        let changed_track = upsert_track(&pool, "acct-jobs", &changed_track).unwrap();
        assert_eq!(changed_track.policy.authority.review_state, "approved");
        assert_eq!(changed_track.policy.authority.policy_revision_no, 2);
        let conn = pool.get().unwrap();
        let (compatibility, review_state): (String, String) = conn
            .query_row(
                "SELECT compatibility_classification, review_state
                   FROM jobs_track_policy_revisions
                  WHERE account_id = ?1 AND career_track_id = ?2 AND revision_no = 2",
                rusqlite::params!["acct-jobs", "track-default"],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(compatibility, "review_required");
        assert_eq!(review_state, "approved");
        let approved_receipts: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM jobs_track_policy_review_receipts
                  WHERE account_id = ?1 AND career_track_id = ?2
                    AND policy_revision_no = 2 AND decision = 'approved'",
                rusqlite::params!["acct-jobs", "track-default"],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(approved_receipts, 1);
        drop(conn);
        let same_track = upsert_track(&pool, "acct-jobs", &changed_track).unwrap();
        assert_eq!(same_track.policy.authority.policy_revision_no, 2);
        let revision_count: i64 = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM jobs_track_policy_revisions
                  WHERE account_id = ?1 AND career_track_id = ?2",
                rusqlite::params!["acct-jobs", "track-default"],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(revision_count, 2, "no-op policy saves must reuse the head");
        let reauthorized =
            authorize_auto_submit(&pool, "acct-jobs", "jobs@example.com", "track-default").unwrap();
        assert_eq!(reauthorized.status, "active");
        assert_eq!(reauthorized.revision_no, authorization.revision_no + 1);

        let replacement_asset = ResumeSourceAsset {
            id: "resume-source-two".to_string(),
            file_name: "software-engineer-resume-v2.pdf".to_string(),
            media_type: "application/pdf".to_string(),
            file_type: "pdf".to_string(),
            storage_key: "jobs/acct-jobs/resume-source-two".to_string(),
            sha256: "b".repeat(64),
            size_bytes: 2_048,
            page_count: Some(2),
            template_status: "converted_layout".to_string(),
            created_at_ms: now + 1,
            updated_at_ms: now + 1,
        };
        let mut replacement_profile = profile;
        replacement_profile.source_resume_name = replacement_asset.file_name.clone();
        replacement_profile.source_resume_asset_id = replacement_asset.id.clone();
        replacement_profile.source_resume_sha256 = replacement_asset.sha256.clone();
        replacement_profile.source_resume_media_type = replacement_asset.media_type.clone();
        replacement_profile.source_resume_template_status =
            replacement_asset.template_status.clone();
        save_resume_source_asset(&pool, "acct-jobs", &replacement_asset, &replacement_profile)
            .unwrap();

        let authorizations =
            list_auto_submit_authorizations(&pool, "acct-jobs", "jobs@example.com").unwrap();
        assert_eq!(authorizations.len(), 1);
        assert_eq!(authorizations[0].status, "needs_review");
        assert!(require_valid_auto_submit_authorization(
            &pool,
            "acct-jobs",
            "jobs@example.com",
            "track-default",
        )
        .is_err());

        assert!(revoke_auto_submit(&pool, "acct-jobs", "track-default").unwrap());
        assert!(
            list_auto_submit_authorizations(&pool, "acct-jobs", "jobs@example.com")
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn resume_source_publication_commits_pointer_profile_and_prior_cleanup_atomically() {
        let pool = test_pool();
        let prior_profile = requested_resume_profile("Prior profile");
        let (prior_upload, prior_asset) = reserve_verified_resume_source_upload(
            &pool,
            "atomic-prior",
            "prior.pdf",
            &"a".repeat(64),
            128,
            (None, &prior_profile),
            None,
        );
        let prior_publication = publish_resume_source_asset(
            &pool,
            "acct-jobs",
            &prior_asset,
            &prior_profile,
            &prior_upload.id,
        )
        .unwrap();
        assert!(!prior_publication.replayed);

        let mut next_profile = prior_publication.profile.clone();
        next_profile.headline = "Next profile".to_string();
        let (next_upload, next_asset) = reserve_verified_resume_source_upload(
            &pool,
            "atomic-next",
            "next.pdf",
            &"b".repeat(64),
            256,
            (Some(&prior_publication.profile), &next_profile),
            Some(&prior_asset.id),
        );
        publish_resume_source_asset(
            &pool,
            "acct-jobs",
            &next_asset,
            &next_profile,
            &next_upload.id,
        )
        .unwrap();

        assert_eq!(
            get_resume_source_asset(&pool, "acct-jobs").unwrap(),
            Some(next_asset.clone())
        );
        let stored_profile = get_profile(&pool, "acct-jobs", "jobs@example.com").unwrap();
        assert_eq!(stored_profile.source_resume_asset_id, next_asset.id);
        assert_eq!(stored_profile.source_resume_sha256, next_asset.sha256);
        assert_eq!(stored_profile.headline, "Next profile");

        let prior_lifecycle: (String, String, String) = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT upload.state, put.state, deletion.state
                   FROM object_uploads upload
                   JOIN object_storage_outbox put
                     ON put.upload_id = upload.id AND put.operation = 'put'
                   JOIN object_storage_outbox deletion
                     ON deletion.upload_id = upload.id AND deletion.operation = 'delete'
                  WHERE upload.id = ?1",
                params![prior_upload.id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(
            prior_lifecycle,
            (
                "delete_pending".into(),
                "completed".into(),
                "pending".into()
            )
        );
        let next_lifecycle: (String, String, Option<i64>, i64) = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT upload.state, put.state, upload.uploaded_at_ms,
                        (SELECT COUNT(*) FROM object_storage_outbox deletion
                          WHERE deletion.upload_id = upload.id
                            AND deletion.operation = 'delete')
                   FROM object_uploads upload
                   JOIN object_storage_outbox put
                     ON put.upload_id = upload.id AND put.operation = 'put'
                  WHERE upload.id = ?1 AND upload.object_key = ?2
                    AND upload.sha256 = ?3 AND upload.size_bytes = ?4
                    AND upload.content_type = ?5",
                params![
                    next_upload.id,
                    next_asset.storage_key,
                    next_asset.sha256,
                    next_asset.size_bytes,
                    next_asset.media_type,
                ],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap();
        assert_eq!(next_lifecycle.0, "ready");
        assert_eq!(next_lifecycle.1, "completed");
        assert!(next_lifecycle.2.is_some());
        assert_eq!(next_lifecycle.3, 0);
    }

    #[test]
    fn resume_source_metadata_failure_rolls_back_publication_and_pointer_replacement() {
        let pool = test_pool();
        let prior_profile = requested_resume_profile("Prior profile");
        let (prior_upload, prior_asset) = reserve_verified_resume_source_upload(
            &pool,
            "rollback-prior",
            "prior.pdf",
            &"c".repeat(64),
            128,
            (None, &prior_profile),
            None,
        );
        let prior_publication = publish_resume_source_asset(
            &pool,
            "acct-jobs",
            &prior_asset,
            &prior_profile,
            &prior_upload.id,
        )
        .unwrap();

        let mut next_profile = prior_publication.profile.clone();
        next_profile.headline = "Next profile".to_string();
        let (next_upload, next_asset) = reserve_verified_resume_source_upload(
            &pool,
            "rollback-next",
            "next.pdf",
            &"d".repeat(64),
            256,
            (Some(&prior_publication.profile), &next_profile),
            Some(&prior_asset.id),
        );
        pool.get()
            .unwrap()
            .execute_batch(
                "CREATE TRIGGER fail_resume_profile_metadata_update
                 BEFORE UPDATE ON jobs_profiles
                 WHEN NEW.account_id = 'acct-jobs'
                 BEGIN
                   SELECT RAISE(ABORT, 'forced resume profile metadata failure');
                 END;",
            )
            .unwrap();

        assert!(publish_resume_source_asset(
            &pool,
            "acct-jobs",
            &next_asset,
            &next_profile,
            &next_upload.id,
        )
        .is_err());

        assert_eq!(
            get_resume_source_asset(&pool, "acct-jobs").unwrap(),
            Some(prior_asset.clone())
        );
        let stored_profile = get_profile(&pool, "acct-jobs", "jobs@example.com").unwrap();
        assert_eq!(stored_profile.source_resume_asset_id, prior_asset.id);
        assert_eq!(stored_profile.headline, "Prior profile");
        let lifecycle: (String, String, String, String, i64, i64) = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT prior.state, prior_put.state, next.state, next_put.state,
                        (SELECT COUNT(*) FROM object_storage_outbox deletion
                          WHERE deletion.upload_id = prior.id
                            AND deletion.operation = 'delete'),
                        (SELECT COUNT(*) FROM object_storage_outbox deletion
                          WHERE deletion.upload_id = next.id
                            AND deletion.operation = 'delete')
                   FROM object_uploads prior
                   JOIN object_storage_outbox prior_put
                     ON prior_put.upload_id = prior.id AND prior_put.operation = 'put'
                   JOIN object_uploads next ON next.id = ?2
                   JOIN object_storage_outbox next_put
                     ON next_put.upload_id = next.id AND next_put.operation = 'put'
                  WHERE prior.id = ?1",
                params![prior_upload.id, next_upload.id],
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
        assert_eq!(
            lifecycle,
            (
                "ready".into(),
                "completed".into(),
                "pending".into(),
                "retry".into(),
                0,
                0,
            )
        );
    }

    #[test]
    fn concurrent_first_resume_publications_commit_once_and_leave_the_loser_pending() {
        let pool = test_pool();
        let first_profile = requested_resume_profile("First profile");
        let second_profile = requested_resume_profile("Second profile");
        let (first_upload, first_asset) = reserve_verified_resume_source_upload(
            &pool,
            "concurrent-first",
            "first.pdf",
            &"e".repeat(64),
            128,
            (None, &first_profile),
            None,
        );
        let (second_upload, second_asset) = reserve_verified_resume_source_upload(
            &pool,
            "concurrent-second",
            "second.pdf",
            &"f".repeat(64),
            192,
            (None, &second_profile),
            None,
        );
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(3));
        let workers = [
            (first_asset.clone(), first_profile, first_upload.id.clone()),
            (
                second_asset.clone(),
                second_profile,
                second_upload.id.clone(),
            ),
        ]
        .into_iter()
        .map(|(asset, profile, upload_id)| {
            let pool = pool.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                publish_resume_source_asset(&pool, "acct-jobs", &asset, &profile, &upload_id)
                    .map(|_| ())
            })
        })
        .collect::<Vec<_>>();
        barrier.wait();
        let results = workers
            .into_iter()
            .map(|worker| worker.join().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
        let conflict = results
            .iter()
            .find_map(|result| result.as_ref().err())
            .expect("one concurrent first publication must lose its predecessor fence");
        assert_eq!(
            conflict.downcast_ref::<UploadControlError>(),
            Some(&UploadControlError::IdempotencyConflict)
        );

        let current = get_resume_source_asset(&pool, "acct-jobs")
            .unwrap()
            .unwrap();
        assert!(current == first_asset || current == second_asset);
        let (winner, loser, expected_headline) = if current == first_asset {
            (&first_upload, &second_upload, "First profile")
        } else {
            (&second_upload, &first_upload, "Second profile")
        };
        assert_eq!(
            get_profile(&pool, "acct-jobs", "jobs@example.com")
                .unwrap()
                .headline,
            expected_headline
        );
        let pointer_count: i64 = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM jobs_resume_source_assets WHERE account_id = ?1",
                params!["acct-jobs"],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(pointer_count, 1);
        let winner_lifecycle = upload_lifecycle(&pool, &winner.id);
        assert_eq!(winner_lifecycle, ("ready".into(), "completed".into(), 0));
        let loser_lifecycle = upload_lifecycle(&pool, &loser.id);
        assert_eq!(
            loser_lifecycle,
            ("pending".into(), "retry".into(), 0),
            "the losing verified object remains retryable and unpublished"
        );
    }

    #[test]
    fn same_resume_request_replay_preserves_newer_same_bound_profile() {
        let pool = test_pool();
        let requested_profile = requested_resume_profile("Original profile");
        let (upload, asset) = reserve_verified_resume_source_upload(
            &pool,
            "replay-stable",
            "resume.pdf",
            &"1".repeat(64),
            128,
            (None, &requested_profile),
            None,
        );
        let first =
            publish_resume_source_asset(&pool, "acct-jobs", &asset, &requested_profile, &upload.id)
                .unwrap();
        assert!(!first.replayed);

        let mut newer_profile = first.profile.clone();
        newer_profile.headline = "Edited after upload".to_string();
        newer_profile.summary = "Keep this later profile edit.".to_string();
        let newer_profile = save_profile(&pool, "acct-jobs", &newer_profile).unwrap();
        assert_eq!(newer_profile.source_resume_asset_id, asset.id);

        let replay =
            publish_resume_source_asset(&pool, "acct-jobs", &asset, &requested_profile, &upload.id)
                .unwrap();
        assert!(replay.replayed);
        assert_eq!(replay.previous, None);
        assert_eq!(replay.asset, asset);
        assert_eq!(replay.profile.headline, "Edited after upload");
        assert_eq!(replay.profile.summary, "Keep this later profile edit.");
        assert_eq!(replay.profile.source_resume_asset_id, replay.asset.id);

        let stored = get_profile(&pool, "acct-jobs", "jobs@example.com").unwrap();
        assert_eq!(stored.headline, newer_profile.headline);
        assert_eq!(stored.summary, newer_profile.summary);
        assert_eq!(stored.source_resume_asset_id, replay.asset.id);
        assert_eq!(
            upload_lifecycle(&pool, &upload.id),
            ("ready".into(), "completed".into(), 0)
        );
    }

    #[test]
    fn delayed_resume_request_cannot_replace_a_newer_successor() {
        let pool = test_pool();
        let base_request = requested_resume_profile("Base profile");
        let (base_upload, base_asset) = reserve_verified_resume_source_upload(
            &pool,
            "delayed-base",
            "base.pdf",
            &"2".repeat(64),
            128,
            (None, &base_request),
            None,
        );
        let base_publication = publish_resume_source_asset(
            &pool,
            "acct-jobs",
            &base_asset,
            &base_request,
            &base_upload.id,
        )
        .unwrap();

        let mut request_a = base_publication.profile.clone();
        request_a.headline = "Delayed A".to_string();
        let (upload_a, asset_a) = reserve_verified_resume_source_upload(
            &pool,
            "delayed-a",
            "a.pdf",
            &"3".repeat(64),
            160,
            (Some(&base_publication.profile), &request_a),
            Some(&base_asset.id),
        );
        let mut request_b = base_publication.profile.clone();
        request_b.headline = "Committed B".to_string();
        let (upload_b, asset_b) = reserve_verified_resume_source_upload(
            &pool,
            "delayed-b",
            "b.pdf",
            &"4".repeat(64),
            192,
            (Some(&base_publication.profile), &request_b),
            Some(&base_asset.id),
        );

        let publication_b =
            publish_resume_source_asset(&pool, "acct-jobs", &asset_b, &request_b, &upload_b.id)
                .unwrap();
        assert_eq!(publication_b.previous, Some(base_asset));
        assert_resume_idempotency_conflict(publish_resume_source_asset(
            &pool,
            "acct-jobs",
            &asset_a,
            &request_a,
            &upload_a.id,
        ));

        assert_eq!(
            get_resume_source_asset(&pool, "acct-jobs").unwrap(),
            Some(asset_b.clone())
        );
        let stored = get_profile(&pool, "acct-jobs", "jobs@example.com").unwrap();
        assert_eq!(stored.headline, "Committed B");
        assert_eq!(stored.source_resume_asset_id, asset_b.id);
        assert_eq!(
            upload_lifecycle(&pool, &base_upload.id),
            ("delete_pending".into(), "completed".into(), 1)
        );
        assert_eq!(
            upload_lifecycle(&pool, &upload_b.id),
            ("ready".into(), "completed".into(), 0)
        );
        assert_eq!(
            upload_lifecycle(&pool, &upload_a.id),
            ("pending".into(), "retry".into(), 0)
        );
    }

    #[test]
    fn resume_publication_conflicts_when_profile_base_changes_during_upload() {
        let pool = test_pool();
        let base_request = requested_resume_profile("Base profile");
        let (base_upload, base_asset) = reserve_verified_resume_source_upload(
            &pool,
            "base-revision-current",
            "base.pdf",
            &"5".repeat(64),
            128,
            (None, &base_request),
            None,
        );
        let base_publication = publish_resume_source_asset(
            &pool,
            "acct-jobs",
            &base_asset,
            &base_request,
            &base_upload.id,
        )
        .unwrap();

        let mut requested_replacement = base_publication.profile.clone();
        requested_replacement.headline = "Upload request".to_string();
        let (pending_upload, pending_asset) = reserve_verified_resume_source_upload(
            &pool,
            "base-revision-pending",
            "pending.pdf",
            &"6".repeat(64),
            160,
            (Some(&base_publication.profile), &requested_replacement),
            Some(&base_asset.id),
        );

        let mut edited_profile = base_publication.profile.clone();
        edited_profile.headline = "Saved while upload was pending".to_string();
        let edited_profile = save_profile(&pool, "acct-jobs", &edited_profile).unwrap();
        assert_resume_idempotency_conflict(publish_resume_source_asset(
            &pool,
            "acct-jobs",
            &pending_asset,
            &requested_replacement,
            &pending_upload.id,
        ));

        assert_eq!(
            get_resume_source_asset(&pool, "acct-jobs").unwrap(),
            Some(base_asset.clone())
        );
        let stored = get_profile(&pool, "acct-jobs", "jobs@example.com").unwrap();
        assert_eq!(stored.headline, edited_profile.headline);
        assert_eq!(stored.source_resume_asset_id, base_asset.id);
        assert_eq!(
            upload_lifecycle(&pool, &base_upload.id),
            ("ready".into(), "completed".into(), 0)
        );
        assert_eq!(
            upload_lifecycle(&pool, &pending_upload.id),
            ("pending".into(), "retry".into(), 0)
        );
    }

    #[test]
    fn resume_publication_rejects_requested_profile_digest_mismatch() {
        let pool = test_pool();
        let requested_profile = requested_resume_profile("Requested profile");
        let different_profile = requested_resume_profile("Different profile");
        let requested_digest = resume_requested_profile_sha256(&requested_profile).unwrap();
        let different_digest = resume_requested_profile_sha256(&different_profile).unwrap();
        assert_ne!(requested_digest, different_digest);
        let (upload, asset) = reserve_verified_resume_source_upload_with_requested_digest(
            &pool,
            "digest-mismatch",
            "resume.pdf",
            &"7".repeat(64),
            128,
            (None, &different_digest),
            None,
        );

        assert_resume_idempotency_conflict(publish_resume_source_asset(
            &pool,
            "acct-jobs",
            &asset,
            &requested_profile,
            &upload.id,
        ));
        assert_eq!(get_resume_source_asset(&pool, "acct-jobs").unwrap(), None);
        let stored = get_profile(&pool, "acct-jobs", "jobs@example.com").unwrap();
        assert!(stored.source_resume_asset_id.is_empty());
        assert!(stored.headline.is_empty());
        assert_eq!(
            upload_lifecycle(&pool, &upload.id),
            ("pending".into(), "retry".into(), 0)
        );
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
        let (profile, preferences) =
            execution_policy_fixture(pool, "acct-jobs", "jobs@example.com", "track-default");
        set_entitlement_plan(pool, "acct-jobs", "cloud").unwrap();
        let greenhouse_tenant = format!("acme-{suffix}");
        let mut posting_input = test_posting(
            &format!("https://boards.greenhouse.io/{greenhouse_tenant}/jobs/{suffix}"),
            now_ms(),
            now_ms(),
        );
        posting_input.company = format!("Acme {suffix}");
        posting_input.source = "greenhouse_import".to_string();
        posting_input.external_id = suffix.to_string();
        posting_input.description =
            "Build reliable products with Rust and TypeScript. Requires 1+ years of software engineering experience."
                .to_string();
        posting_input.canonical_key = canonical_job_key(&posting_input);
        posting_input.discovery_evidence = JobDiscoveryEvidence::default();
        let (posting, managed) = save_production_positive_verified_import(
            pool,
            "acct-jobs",
            &posting_input,
            &profile,
            &preferences,
        );
        let authority_fixture = install_production_positive_job_authorities_for_runner(
            pool,
            "acct-jobs",
            &posting,
            &managed,
            &format!("{greenhouse_tenant}.example"),
            suffix,
            "cloud",
        );
        let (application, _) =
            prepare_application(pool, "acct-jobs", &posting.id, "factual", "review_first").unwrap();
        let run_id = format!("cloud-run-{suffix}");
        upsert_browser_session(
            pool,
            "acct-jobs",
            &BrowserSession {
                id: run_id.clone(),
                runner: "cloud".to_string(),
                status: "queued".to_string(),
                current_company: authority_fixture.posting.company.clone(),
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
        let authority = current_application_approval_authority(
            pool,
            "acct-jobs",
            "jobs@example.com",
            &application.id,
        )
        .unwrap()
        .expect("current production-positive application approval authority");
        assert_eq!(authority.posting.id, authority_fixture.posting.id);
        let identity_id = authority
            .application
            .receipt
            .pointer("/application_identity/id")
            .and_then(Value::as_str)
            .unwrap()
            .to_string();
        let identity_email = authority
            .application
            .receipt
            .pointer("/application_identity/email")
            .and_then(Value::as_str)
            .unwrap()
            .to_string();
        let browser_profile_id = execution_browser_profile_id("acct-jobs", &identity_id);
        let resume = get_resume_version(
            pool,
            "acct-jobs",
            authority
                .application
                .resume_version_id
                .as_deref()
                .expect("fixture application has a resume"),
        )
        .unwrap()
        .unwrap();
        let approved_packet = json!({
            "applicationId": authority.application.id,
            "jobId": authority.posting.id,
            "resumeVersionId": resume.id,
            "resumeContent": resume.content,
            "coverLetterContent": authority.application.cover_letter,
            "answers": {},
            "verifiedClaimIds": resume.claim_ids,
            "applicationIdentityId": identity_id,
            "applicationEmail": identity_email,
            "browserProfileId": browser_profile_id,
        });
        let approved_job = json!({
            "externalId": authority.posting.external_id,
            "canonicalUrl": authority.posting.canonical_url,
            "company": authority.posting.company,
            "title": authority.posting.title,
            "location": authority.posting.location,
            "workplace": authority.posting.workplace,
            "description": authority.posting.description,
            "source": authority.posting.source,
            "compensation": authority.posting.compensation,
        });
        let admission = json!({ "kind": "review_approval" });
        let checksum =
            approved_submission_checksum(2, &approved_packet, &approved_job, Some(&admission))
                .unwrap();
        let approved_execution = json!({
            "schema_version": 2,
            "approved_at_ms": authority.evaluated_at_ms,
            "checksum": checksum,
            "admission": admission,
            "packet": approved_packet,
            "job": approved_job,
        });
        let application = persist_current_application_approval(
            pool,
            "acct-jobs",
            "jobs@example.com",
            &authority,
            &approved_execution,
        )
        .unwrap()
        .expect("persist production-positive application approval");
        reserve_application_attempt(pool, "acct-jobs", &application.id, "cloud").unwrap();
        let application =
            update_application(pool, "acct-jobs", &application.id, "queued", None).unwrap();
        (
            application.expect("queue production-positive application"),
            run_id,
            browser_profile_id,
        )
    }

    struct FinalSubmissionFixture {
        application: JobApplication,
        run_id: String,
        lease_token: String,
        lease_fence: i64,
        fingerprint: String,
        receipt: Value,
        evidence: Vec<ApplicationEvidence>,
        object_uploads: Vec<ApplicationObjectBinding>,
        session: BrowserSession,
    }

    fn test_submission_evidence_capacity(
        application_id: &str,
        run_id: &str,
    ) -> NewSubmissionEvidenceCapacity {
        test_submission_evidence_capacity_with_object_cap(
            application_id,
            run_id,
            SUBMISSION_RECEIPT_BUNDLE_MAX_RESERVED_BYTES,
        )
    }

    fn test_submission_evidence_capacity_with_object_cap(
        application_id: &str,
        run_id: &str,
        max_object_bytes: i64,
    ) -> NewSubmissionEvidenceCapacity {
        let now = now_ms();
        NewSubmissionEvidenceCapacity {
            account_id: "acct-jobs".to_string(),
            application_id: application_id.to_string(),
            run_id: run_id.to_string(),
            runner: "cloud".to_string(),
            reserved_bytes: submission_evidence_reserved_bytes(max_object_bytes).unwrap(),
            reserved_objects: SUBMISSION_EVIDENCE_RESERVED_OBJECTS,
            expires_at_ms: now.saturating_add(SUBMISSION_RECONCILIATION_GRACE_MS),
            now_ms: now,
            limits: UploadLimits {
                max_object_bytes,
                max_account_bytes: 512 * 1024 * 1024,
                max_daily_bytes: 512 * 1024 * 1024,
                max_account_objects: 1_000,
            },
        }
    }

    #[test]
    fn submission_evidence_capacity_uses_the_final_database_time_window() {
        let capacity = test_submission_evidence_capacity("application", "run");
        let database_now_ms = capacity.now_ms.saturating_add(37);
        for rebound in [
            submission_evidence_capacity_at_ms(&capacity, database_now_ms),
            local_submission_evidence_capacity_at_ms(&capacity, database_now_ms),
        ] {
            assert_eq!(rebound.now_ms, database_now_ms);
            assert_eq!(
                rebound.expires_at_ms,
                database_now_ms.saturating_add(SUBMISSION_RECONCILIATION_GRACE_MS)
            );
        }
    }

    fn test_final_submit_proof(application: &JobApplication) -> FinalSubmitProof {
        let canonical_url = application
            .receipt
            .pointer("/approved_execution/job/canonicalUrl")
            .and_then(Value::as_str)
            .expect("test application has a frozen canonical job URL");
        let resume_version_id = application
            .receipt
            .pointer("/approved_execution/packet/resumeVersionId")
            .and_then(Value::as_str)
            .or(application.resume_version_id.as_deref())
            .expect("test application has a frozen resume version");
        let has_cover_letter = application
            .receipt
            .pointer("/approved_execution/packet/coverLetterContent")
            .and_then(Value::as_str)
            .is_some_and(|value| !value.trim().is_empty());
        test_final_submit_proof_for_shape(canonical_url, resume_version_id, has_cover_letter)
    }

    fn test_final_submit_proof_for_shape(
        canonical_url: &str,
        resume_version_id: &str,
        has_cover_letter: bool,
    ) -> FinalSubmitProof {
        let mut documents = Vec::new();
        if has_cover_letter {
            documents.push(FinalSubmitDocumentProof {
                kind: "cover_letter".to_string(),
                version_id: None,
                sha256: "c".repeat(64),
            });
        }
        documents.push(FinalSubmitDocumentProof {
            kind: "resume".to_string(),
            version_id: Some(resume_version_id.to_string()),
            sha256: "b".repeat(64),
        });
        let provider_job_key = final_submit_provider_job_key("greenhouse", canonical_url)
            .expect("test final-submit URL has a provider job key");
        let files = documents
            .iter()
            .map(|document| FinalSubmitFileProof {
                field_name: document.kind.clone(),
                name: format!(
                    "{}-{}.pdf",
                    if document.kind == "resume" {
                        "resume"
                    } else {
                        "cover-letter"
                    },
                    document.sha256
                ),
                byte_length: 1_024,
                sha256: document.sha256.clone(),
            })
            .collect::<Vec<_>>();
        let part_order = std::iter::once(FinalSubmitPartOrderProof {
            kind: "field".to_string(),
            index: 0,
        })
        .chain(
            files
                .iter()
                .enumerate()
                .map(|(index, _)| FinalSubmitPartOrderProof {
                    kind: "file".to_string(),
                    index: i64::try_from(index).unwrap(),
                }),
        )
        .collect();
        FinalSubmitProof {
            schema_version: 3,
            adapter: "greenhouse".to_string(),
            adapter_version: "2026.07.1-beta.1".to_string(),
            control: "greenhouse_submit_application".to_string(),
            job: FinalSubmitJobProof {
                approved_canonical_url: canonical_url.to_string(),
                page_url: canonical_url.to_string(),
            },
            target: FinalSubmitTargetProof {
                action_url: canonical_url.to_string(),
                method: "post".to_string(),
                enctype: "multipart/form-data".to_string(),
                form_target: "_self".to_string(),
                provider_job_key,
                form_identity: r#"[0,"application-form","","","","",""]"#.to_string(),
            },
            files,
            fields: vec![FinalSubmitFieldProof {
                field_name: "candidate_name".to_string(),
                value_byte_length: 0,
                value_sha256: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
                    .to_string(),
            }],
            part_order,
            documents,
            certification: None,
            observed_surface: None,
        }
    }

    pub(super) struct CertifiedApplicationFixture {
        pub(super) application: JobApplication,
        pub(super) run_id: String,
        pub(super) browser_profile_id: String,
        target_evidence: AtsCertificationFreshTargetEvidence,
        runtime_target: AtsCertificationRuntimeTarget,
        surface: AtsObservedSurface,
        managed_authority_now_ms: i64,
        ticket_hash: Option<String>,
        proof: FinalSubmitProof,
    }

    fn certified_final_submit_surface(cover_letter: &str) -> AtsObservedSurface {
        let proof = test_final_submit_proof_for_shape(
            "https://boards.greenhouse.io/bluey-certified/jobs/surface",
            "certified-surface-resume",
            !cover_letter.trim().is_empty(),
        );
        AtsObservedSurface {
            variant_key: "greenhouse_public".to_string(),
            layout_contract_version: 1,
            surface_sha256: final_submit_surface_sha256(&proof).unwrap(),
        }
    }

    fn certified_cloud_runtime(suffix: &str) -> RunnerProcessRuntimeAttestation {
        RunnerProcessRuntimeAttestation {
            runner_image_sha256: hex::encode(Sha256::digest(
                format!("certified-cloud-image-{suffix}").as_bytes(),
            )),
            runner_build_id: format!("runner-614b-{suffix}"),
            platform: "linux".to_string(),
            architecture: "x86_64".to_string(),
            automation_bundle_sha256: hex::encode(Sha256::digest(
                format!("certified-cloud-automation-{suffix}").as_bytes(),
            )),
            playwright_version: "1.61.1".to_string(),
            chromium_revision: "123456".to_string(),
            chromium_executable_sha256: hex::encode(Sha256::digest(
                format!("certified-cloud-chromium-{suffix}").as_bytes(),
            )),
        }
    }

    pub(super) fn certified_cloud_runtime_target(
        runtime: &RunnerProcessRuntimeAttestation,
    ) -> AtsCertificationRuntimeTarget {
        AtsCertificationRuntimeTarget {
            runtime_kind: "cloud".to_string(),
            runtime_id: format!("cloud:{}", runtime.runner_build_id),
            runtime_sha256: runner_process_runtime_sha256(runtime).unwrap(),
            platform: runtime.platform.clone(),
            architecture: runtime.architecture.clone(),
            automation_bundle_sha256: runtime.automation_bundle_sha256.clone(),
            browser_release_manifest_sha256: None,
            browser_artifact_sha256: None,
            browser_build_descriptor_sha256: None,
            runner_build_id: Some(runtime.runner_build_id.clone()),
            runner_image_sha256: Some(runtime.runner_image_sha256.clone()),
            playwright_version: runtime.playwright_version.clone(),
            chromium_revision: runtime.chromium_revision.clone(),
            chromium_executable_sha256: runtime.chromium_executable_sha256.clone(),
        }
    }

    pub(super) fn certified_application_fixture(
        pool: &DbPool,
        suffix: &str,
        runner: &str,
        runtime_target: AtsCertificationRuntimeTarget,
        cover_letter: &str,
    ) -> CertifiedApplicationFixture {
        assert_eq!(runtime_target.runtime_kind, runner);
        let (mut profile, preferences) =
            execution_policy_fixture(pool, "acct-jobs", "jobs@example.com", "track-default");
        set_entitlement_plan(
            pool,
            "acct-jobs",
            if runner == "cloud" { "cloud" } else { "pro" },
        )
        .unwrap();
        profile.skills = vec![
            "Rust".to_string(),
            "TypeScript".to_string(),
            "PostgreSQL".to_string(),
        ];
        let profile = save_profile(pool, "acct-jobs", &profile).unwrap();
        let track = list_tracks(pool, "acct-jobs")
            .unwrap()
            .into_iter()
            .find(|track| track.id == "track-default")
            .expect("certified application Career Track exists");
        let track = upsert_track(pool, "acct-jobs", &track).unwrap();
        assert_eq!(track.policy.authority.review_state, "approved");
        assert!(track.policy.authority.policy_revision_no > 0);
        let greenhouse_tenant = format!("bluey-{runner}-{suffix}");
        let mut posting_input = test_posting(
            &format!("https://boards.greenhouse.io/{greenhouse_tenant}/jobs/{suffix}"),
            now_ms(),
            now_ms(),
        );
        posting_input.company = format!("Acme {suffix}");
        posting_input.source = "greenhouse_import".to_string();
        posting_input.external_id = suffix.to_string();
        posting_input.description =
            "Build reliable products with Rust, TypeScript, and PostgreSQL.".to_string();
        posting_input.canonical_key = canonical_job_key(&posting_input);
        posting_input.discovery_evidence = JobDiscoveryEvidence::default();
        let (posting, managed) = save_production_positive_verified_import(
            pool,
            "acct-jobs",
            &posting_input,
            &profile,
            &preferences,
        );
        let managed_authority_now_ms = managed.now_ms;
        assert!(
            posting.match_score >= profile.auto_submit_threshold,
            "production-positive posting must meet the public Auto-submit threshold"
        );
        assert!(
            posting.missing_requirements.is_empty(),
            "production-positive posting must have no unresolved eligibility requirements"
        );
        let surface = certified_final_submit_surface(cover_letter);
        let authority_fixture = install_production_positive_job_authorities_with_runtime_surface(
            pool,
            "acct-jobs",
            &posting,
            &managed,
            &format!("{greenhouse_tenant}.example"),
            suffix,
            runtime_target.clone(),
            surface.clone(),
        );
        assert_eq!(authority_fixture.runtime_target, runtime_target);
        assert_eq!(authority_fixture.surface, surface);
        let authorization =
            authorize_auto_submit(pool, "acct-jobs", "jobs@example.com", "track-default").unwrap();
        let prepared =
            prepare_application_draft(pool, "acct-jobs", &posting.id, "factual", "auto_submit")
                .expect(
                    "prepare certified Auto-submit application through the production lifecycle",
                );
        let eligibility: JobEligibilityDecision =
            serde_json::from_value(prepared.application.receipt["eligibility"].clone()).unwrap();
        assert!(eligibility.can_auto_submit, "{eligibility:#?}");
        assert_eq!(
            eligibility.ats_certification.certified_runner_kinds,
            vec![runner.to_string()],
        );
        let (application, _) = finalize_prepared_application_kit(
            pool,
            "acct-jobs",
            &prepared,
            prepared.baseline_resume.content.clone(),
            prepared.baseline_resume.diff.clone(),
            cover_letter.to_string(),
            json!({
                "status": "deterministic",
                "provider": "bluey-evidence-planner",
                "claims_added": 0,
            }),
        )
        .expect("freeze certified Auto-submit admission through the production lifecycle");
        assert_eq!(application.state, "queued");
        assert_eq!(application.submission_mode, "auto_submit");
        assert_eq!(
            application
                .receipt
                .pointer("/approved_execution/admission/authorization_id")
                .and_then(Value::as_str),
            Some(authorization.id.as_str())
        );
        assert_eq!(
            application_job_integrity_receipt(&application)
                .unwrap()
                .as_ref(),
            Some(&job_integrity_receipt_projection(
                &authority_fixture.integrity
            ))
        );
        let active = authority_fixture
            .ats
            .active_binding
            .as_ref()
            .expect("certified application has one active ATS binding");
        assert_eq!(
            application
                .receipt
                .pointer("/approved_execution/admission/ats_certification"),
            Some(
                &serde_json::to_value(
                    ats_frozen_certification_admission_projection(active).unwrap()
                )
                .unwrap()
            )
        );

        let base_proof = test_final_submit_proof(&application);
        assert_eq!(
            final_submit_surface_sha256(&base_proof).unwrap(),
            surface.surface_sha256
        );
        let certification = ats_certification_admission_projection(active).unwrap();
        let proof = FinalSubmitProof {
            schema_version: 4,
            certification: Some(certification),
            observed_surface: Some(AtsFinalSubmitObservedSurfaceProof {
                schema_version: 1,
                variant_key: surface.variant_key,
                layout_contract_version: surface.layout_contract_version,
                surface_sha256: surface.surface_sha256,
            }),
            ..base_proof
        };
        assert_eq!(
            final_submit_surface_sha256(&proof).unwrap(),
            proof
                .observed_surface
                .as_ref()
                .expect("schema-four surface")
                .surface_sha256
        );
        let identity_id = application
            .receipt
            .pointer("/application_identity/id")
            .and_then(Value::as_str)
            .expect("certified application identity")
            .to_string();
        let browser_profile_id = execution_browser_profile_id("acct-jobs", &identity_id);
        let run_id = format!("{runner}-run-{suffix}");
        upsert_browser_session(
            pool,
            "acct-jobs",
            &BrowserSession {
                id: run_id.clone(),
                runner: runner.to_string(),
                status: "queued".to_string(),
                current_company: authority_fixture.posting.company.clone(),
                current_step: format!("Waiting for the {runner} runner"),
                application_id: Some(application.id.clone()),
                takeover_url: None,
                created_at_ms: 0,
                updated_at_ms: 0,
            },
        )
        .unwrap();
        let application = assign_application_run(pool, "acct-jobs", &application.id, &run_id)
            .unwrap()
            .expect("assign certified application run");
        reserve_application_attempt(pool, "acct-jobs", &application.id, runner).unwrap();
        let ticket_hash = (runner == "local").then(|| {
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
                    "jobId": authority_fixture.posting.id,
                    "applicationIdentityId": identity_id,
                    "browserProfileId": browser_profile_id,
                    "runner": "local",
                    "url": authority_fixture.posting.canonical_url,
                    "runId": run_id,
                }),
                now_ms() + 60_000,
            )
            .unwrap();
            ticket_hash
        });
        let target_evidence = ats_certification_fresh_target_evidence_from_posting(
            &authority_fixture.posting,
            authority_fixture.source.db_time_ms,
        )
        .expect("certified application exposes exact ATS target evidence");
        CertifiedApplicationFixture {
            application,
            run_id,
            browser_profile_id,
            target_evidence,
            runtime_target: authority_fixture.runtime_target,
            surface: authority_fixture.surface,
            managed_authority_now_ms,
            ticket_hash,
            proof,
        }
    }

    fn install_certified_ats_successor(
        pool: &DbPool,
        fixture: &CertifiedApplicationFixture,
        suffix: &str,
    ) {
        let mut runtime_target = fixture.runtime_target.clone();
        runtime_target.runtime_id =
            format!("{}:ats-successor-{suffix}", runtime_target.runtime_kind);
        runtime_target.runtime_sha256 =
            ats_certification_sha256(format!("ats-successor-runtime-{suffix}").as_bytes());
        runtime_target.automation_bundle_sha256 =
            ats_certification_sha256(format!("ats-successor-automation-{suffix}").as_bytes());
        runtime_target.chromium_executable_sha256 =
            ats_certification_sha256(format!("ats-successor-chromium-{suffix}").as_bytes());
        if runtime_target.runtime_kind == "cloud" {
            runtime_target.runner_build_id = Some(format!("runner-ats-successor-{suffix}"));
            runtime_target.runner_image_sha256 = Some(ats_certification_sha256(
                format!("ats-successor-image-{suffix}").as_bytes(),
            ));
        }
        ats_certification_authority_tests::install_signed_ats_authority_fixture(
            pool,
            fixture.target_evidence.clone(),
            fixture.surface.clone(),
            runtime_target,
            now_ms().saturating_add(1),
        )
        .expect("publish signed ATS successor for the exact certified scope");
    }

    fn certified_reservation(pool: &DbPool, application_id: &str) -> AttemptReservation {
        list_attempt_reservations(pool, "acct-jobs")
            .unwrap()
            .into_iter()
            .find(|reservation| reservation.application_id == application_id)
            .expect("certified application reservation exists")
    }

    fn assert_current_execution_authority_denied(error: &anyhow::Error) {
        assert_eq!(
            error.downcast_ref::<ApplicationAttemptAdmissionError>(),
            Some(&ApplicationAttemptAdmissionError::ReviewRequired {
                reason_code: "current_execution_authority_denied".to_string(),
            })
        );
    }

    fn assert_prepared_auto_submit_finalization_employer_hold_is_typed_and_atomic(
        pool: &DbPool,
        suffix: &str,
    ) {
        let runtime = certified_cloud_runtime(suffix);
        let fixture = certified_application_fixture(
            pool,
            suffix,
            "cloud",
            certified_cloud_runtime_target(&runtime),
            "",
        );
        let prepared = prepare_application_draft(
            pool,
            "acct-jobs",
            &fixture.application.job_id,
            "factual",
            "auto_submit",
        )
        .expect("prepare a second certified Auto-submit packet before the queue hold");
        let mut content = prepared.baseline_resume.content.clone();
        content["provenance"]["resume_generation"] = json!({
            "kind": "model",
            "schema_version": 1,
            "truth_guard": "passed",
            "claims_added": 0,
        });
        let generation = content["provenance"]["resume_generation"].clone();
        let employer_domain = application_job_integrity_receipt(&fixture.application)
            .unwrap()
            .expect("certified application has a frozen job-integrity receipt")
            .canonical_employer_domain;
        let held_event_id = format!("fix758-prepared-held-{suffix}");
        let released_event_id = format!("fix758-prepared-released-{suffix}");

        let application_before = serde_json::to_value(
            get_application(pool, "acct-jobs", &fixture.application.id)
                .unwrap()
                .expect("certified application remains stored"),
        )
        .unwrap();
        let resumes_before =
            serde_json::to_value(list_resume_versions(pool, "acct-jobs").unwrap()).unwrap();
        let reservations_before = list_attempt_reservations(pool, "acct-jobs").unwrap();
        let browser_sessions_before =
            serde_json::to_value(list_browser_sessions(pool, "acct-jobs").unwrap()).unwrap();
        let run_events_before =
            serde_json::to_value(list_run_events(pool, "acct-jobs", &fixture.run_id).unwrap())
                .unwrap();

        append_operational_hold_for_scope(
            pool,
            OperationalCapability::ApplicationQueue,
            OperationalHoldScopeKind::EmployerDomain,
            &employer_domain,
            &held_event_id,
            OperationalHoldTransition::Held,
            0,
            None,
        );
        let finalization = finalize_prepared_application_kit(
            pool,
            "acct-jobs",
            &prepared,
            content,
            prepared.baseline_resume.diff.clone(),
            String::new(),
            generation,
        );
        let application_after = serde_json::to_value(
            get_application(pool, "acct-jobs", &fixture.application.id)
                .unwrap()
                .expect("held prepared finalization preserves the application"),
        )
        .unwrap();
        let resumes_after =
            serde_json::to_value(list_resume_versions(pool, "acct-jobs").unwrap()).unwrap();
        let reservations_after = list_attempt_reservations(pool, "acct-jobs").unwrap();
        let browser_sessions_after =
            serde_json::to_value(list_browser_sessions(pool, "acct-jobs").unwrap()).unwrap();
        let run_events_after =
            serde_json::to_value(list_run_events(pool, "acct-jobs", &fixture.run_id).unwrap())
                .unwrap();
        append_operational_hold_for_scope(
            pool,
            OperationalCapability::ApplicationQueue,
            OperationalHoldScopeKind::EmployerDomain,
            &employer_domain,
            &released_event_id,
            OperationalHoldTransition::Released,
            1,
            Some(&held_event_id),
        );

        let error = finalization.expect_err("the current employer hold must block finalization");
        let block = match error.downcast_ref::<OperationalHoldError>() {
            Some(OperationalHoldError::Held(block)) => block,
            other => panic!("prepared finalization lost its typed hold: {other:?}"),
        };
        assert_eq!(block.capability, OperationalCapability::ApplicationQueue);
        assert_eq!(block.scope_kind, OperationalHoldScopeKind::EmployerDomain);
        assert_eq!(block.scope_id, employer_domain);
        assert_eq!(block.reason_code, OperationalHoldReasonCode::Incident);
        assert_eq!(block.head_revision, 1);
        assert_eq!(application_after, application_before);
        assert_eq!(resumes_after, resumes_before);
        assert_eq!(reservations_after, reservations_before);
        assert_eq!(browser_sessions_after, browser_sessions_before);
        assert_eq!(run_events_after, run_events_before);
    }

    #[test]
    fn prepared_auto_submit_finalization_preserves_typed_employer_hold_and_mutates_nothing() {
        let pool = test_pool();
        assert_prepared_auto_submit_finalization_employer_hold_is_typed_and_atomic(
            &pool,
            "sqlite-prepared-hold",
        );
    }

    #[test]
    fn ats_head_replacement_cannot_reserve_frozen_auto_submit_capacity() {
        let pool = test_pool();
        let runtime = certified_cloud_runtime("ats-head-reserve");
        let fixture = certified_application_fixture(
            &pool,
            "ats-head-reserve",
            "cloud",
            certified_cloud_runtime_target(&runtime),
            "",
        );
        assert!(update_attempt_reservation_status(
            &pool,
            "acct-jobs",
            &fixture.application.id,
            "released",
        )
        .unwrap());
        let before = certified_reservation(&pool, &fixture.application.id);
        assert_eq!(before.status, "released");
        install_certified_ats_successor(&pool, &fixture, "reserve");

        let error =
            reserve_application_attempt(&pool, "acct-jobs", &fixture.application.id, "cloud")
                .unwrap_err();
        assert_current_execution_authority_denied(&error);
        assert_eq!(
            certified_reservation(&pool, &fixture.application.id),
            before,
            "ATS head drift must not reserve or rewrite capacity",
        );
    }

    #[test]
    fn ats_head_replacement_cannot_start_or_persist_a_frozen_auto_submit_run() {
        let pool = test_pool();
        let runtime = certified_cloud_runtime("ats-head-running");
        let fixture = certified_application_fixture(
            &pool,
            "ats-head-running",
            "cloud",
            certified_cloud_runtime_target(&runtime),
            "",
        );
        let reservation_before = certified_reservation(&pool, &fixture.application.id);
        assert_eq!(reservation_before.status, "reserved");
        let (application_before, expected_revision) =
            get_application_with_revision(&pool, "acct-jobs", &fixture.application.id)
                .unwrap()
                .expect("certified application exists");
        install_certified_ats_successor(&pool, &fixture, "running");

        let error = update_attempt_reservation_status(
            &pool,
            "acct-jobs",
            &fixture.application.id,
            "running",
        )
        .unwrap_err();
        assert_current_execution_authority_denied(&error);
        assert_eq!(
            certified_reservation(&pool, &fixture.application.id),
            reservation_before,
            "ATS head drift must not start a reserved attempt",
        );

        let mut attempted = application_before.clone();
        attempted.state = "running".to_string();
        attempted.updated_at_ms = attempted.updated_at_ms.saturating_add(1);
        assert!(save_application(&pool, "acct-jobs", &attempted, &expected_revision,).is_err());
        let application_after = get_application(&pool, "acct-jobs", &fixture.application.id)
            .unwrap()
            .expect("certified application remains stored");
        assert_eq!(
            serde_json::to_value(application_after).unwrap(),
            serde_json::to_value(application_before).unwrap(),
            "ATS head drift must not persist a running application",
        );
    }

    #[test]
    #[serial_test::serial]
    fn postgres_ats_head_replacement_cannot_reserve_or_start_when_configured() {
        let Ok(database_url) = std::env::var("BLUEY_TEST_POSTGRES_URL") else {
            return;
        };
        let pool = db::open_postgres_pool(&database_url).unwrap();
        db::run_migrations(&pool).unwrap();
        db::run_migrations(&pool).unwrap();
        pool.get_pg()
            .unwrap()
            .execute("DELETE FROM accounts WHERE id = 'acct-jobs'", &[])
            .unwrap();
        let suffix = uuid::Uuid::new_v4().simple().to_string();
        let suffix = &suffix[..16];

        let test_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut conn = pool.get_pg().unwrap();
            conn.execute(
                "INSERT INTO accounts (id, email, password_hash, trial_seconds_remaining)
                 VALUES ('acct-jobs', 'jobs@example.com', 'hash', 0)",
                &[],
            )
            .unwrap();
            drop(conn);
            let identity =
                ensure_primary_application_identity(&pool, "acct-jobs", "jobs@example.com")
                    .unwrap();
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

            let reserve_suffix = format!("postgres-ats-head-reserve-{suffix}");
            let reserve_runtime = certified_cloud_runtime(&reserve_suffix);
            let reserve_fixture = certified_application_fixture(
                &pool,
                &reserve_suffix,
                "cloud",
                certified_cloud_runtime_target(&reserve_runtime),
                "",
            );
            assert!(update_attempt_reservation_status(
                &pool,
                "acct-jobs",
                &reserve_fixture.application.id,
                "released",
            )
            .unwrap());
            let reservation_before = certified_reservation(&pool, &reserve_fixture.application.id);
            install_certified_ats_successor(
                &pool,
                &reserve_fixture,
                &format!("postgres-reserve-{suffix}"),
            );
            let reserve_error = reserve_application_attempt(
                &pool,
                "acct-jobs",
                &reserve_fixture.application.id,
                "cloud",
            )
            .unwrap_err();
            assert_current_execution_authority_denied(&reserve_error);
            assert_eq!(
                certified_reservation(&pool, &reserve_fixture.application.id),
                reservation_before
            );

            let running_suffix = format!("postgres-ats-head-running-{suffix}");
            let running_runtime = certified_cloud_runtime(&running_suffix);
            let running_fixture = certified_application_fixture(
                &pool,
                &running_suffix,
                "cloud",
                certified_cloud_runtime_target(&running_runtime),
                "",
            );
            let reservation_before = certified_reservation(&pool, &running_fixture.application.id);
            let (application_before, expected_revision) =
                get_application_with_revision(&pool, "acct-jobs", &running_fixture.application.id)
                    .unwrap()
                    .expect("PostgreSQL certified application exists");
            install_certified_ats_successor(
                &pool,
                &running_fixture,
                &format!("postgres-running-{suffix}"),
            );
            let running_error = update_attempt_reservation_status(
                &pool,
                "acct-jobs",
                &running_fixture.application.id,
                "running",
            )
            .unwrap_err();
            assert_current_execution_authority_denied(&running_error);
            assert_eq!(
                certified_reservation(&pool, &running_fixture.application.id),
                reservation_before
            );

            let mut attempted = application_before.clone();
            attempted.state = "running".to_string();
            attempted.updated_at_ms = attempted.updated_at_ms.saturating_add(1);
            assert!(save_application(&pool, "acct-jobs", &attempted, &expected_revision,).is_err());
            let application_after =
                get_application(&pool, "acct-jobs", &running_fixture.application.id)
                    .unwrap()
                    .expect("PostgreSQL certified application remains stored");
            assert_eq!(
                serde_json::to_value(application_after).unwrap(),
                serde_json::to_value(application_before).unwrap()
            );
        }));

        pool.get_pg()
            .unwrap()
            .execute("DELETE FROM accounts WHERE id = 'acct-jobs'", &[])
            .unwrap();
        if let Err(panic) = test_result {
            std::panic::resume_unwind(panic);
        }
    }

    #[test]
    #[serial_test::serial]
    fn postgres_prepared_auto_submit_finalization_preserves_typed_employer_hold_when_configured() {
        let Ok(database_url) = std::env::var("BLUEY_TEST_POSTGRES_URL") else {
            return;
        };
        let pool = db::open_postgres_pool(&database_url).unwrap();
        db::run_migrations(&pool).unwrap();
        db::run_migrations(&pool).unwrap();
        pool.get_pg()
            .unwrap()
            .execute("DELETE FROM accounts WHERE id = 'acct-jobs'", &[])
            .unwrap();
        let suffix = uuid::Uuid::new_v4().simple().to_string();
        let suffix = &suffix[..16];

        let test_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut conn = pool.get_pg().unwrap();
            conn.execute(
                "INSERT INTO accounts (id, email, password_hash, trial_seconds_remaining)
                 VALUES ('acct-jobs', 'jobs@example.com', 'hash', 0)",
                &[],
            )
            .unwrap();
            drop(conn);
            let identity =
                ensure_primary_application_identity(&pool, "acct-jobs", "jobs@example.com")
                    .unwrap();
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

            assert_prepared_auto_submit_finalization_employer_hold_is_typed_and_atomic(
                &pool,
                &format!("pg-prepared-hold-{suffix}"),
            );
        }));

        pool.get_pg()
            .unwrap()
            .execute("DELETE FROM accounts WHERE id = 'acct-jobs'", &[])
            .unwrap();
        if let Err(panic) = test_result {
            std::panic::resume_unwind(panic);
        }
    }

    fn proof_with_unknown_layout(mut proof: FinalSubmitProof) -> FinalSubmitProof {
        proof.fields[0].field_name = "candidate_email".to_string();
        let surface_sha256 = final_submit_surface_sha256(&proof).unwrap();
        proof
            .observed_surface
            .as_mut()
            .expect("certified proof surface")
            .surface_sha256 = surface_sha256;
        proof
    }

    fn require_frozen_cover_letter(
        pool: &DbPool,
        mut application: JobApplication,
    ) -> JobApplication {
        application.receipt["approved_execution"]["packet"]["coverLetterContent"] =
            json!("Dear Acme, I am excited to apply.");
        let approved = &application.receipt["approved_execution"];
        let checksum = approved_submission_checksum(
            approved["schema_version"].as_i64().unwrap(),
            &approved["packet"],
            &approved["job"],
            approved.get("admission"),
        )
        .unwrap();
        application.receipt["approved_execution"]["checksum"] = json!(checksum);
        replace_application_receipt(
            pool,
            "acct-jobs",
            &application.id,
            application.receipt.clone(),
        )
        .unwrap()
        .unwrap()
    }

    fn store_test_final_submit_proof(
        pool: &DbPool,
        mut application: JobApplication,
        proof: &FinalSubmitProof,
    ) -> JobApplication {
        application.receipt[FINAL_SUBMIT_PROOF_KEY] = serde_json::to_value(proof).unwrap();
        replace_application_receipt(
            pool,
            "acct-jobs",
            &application.id,
            application.receipt.clone(),
        )
        .unwrap()
        .unwrap()
    }

    fn install_final_submit_proof_order_triggers(pool: &DbPool) {
        pool.get()
            .unwrap()
            .execute_batch(
                "CREATE TABLE test_final_submit_proof_writes (
                    application_id TEXT PRIMARY KEY
                 );
                 CREATE TRIGGER test_record_final_submit_proof_write
                   AFTER UPDATE OF application_json ON jobs_applications
                  BEGIN
                    INSERT OR REPLACE INTO test_final_submit_proof_writes(application_id)
                    VALUES (NEW.id);
                  END;
                 CREATE TRIGGER test_cloud_proof_precedes_click
                   BEFORE UPDATE OF phase ON jobs_execution_leases
                   WHEN NEW.phase = 'click_started'
                    AND NOT EXISTS (
                        SELECT 1 FROM test_final_submit_proof_writes
                         WHERE application_id = NEW.application_id
                    )
                  BEGIN
                    SELECT RAISE(ABORT, 'final submit proof missing before cloud click');
                  END;
                 CREATE TRIGGER test_local_proof_precedes_click
                   BEFORE UPDATE OF status ON jobs_local_run_tickets
                   WHEN NEW.status = 'click_started'
                    AND NOT EXISTS (
                        SELECT 1 FROM test_final_submit_proof_writes
                         WHERE application_id = NEW.application_id
                    )
                  BEGIN
                    SELECT RAISE(ABORT, 'final submit proof missing before local click');
                  END;",
            )
            .unwrap();
    }

    fn submission_capacity_count(pool: &DbPool, application_id: &str, run_id: &str) -> i64 {
        pool.get()
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM jobs_submission_evidence_capacity
                  WHERE account_id = 'acct-jobs' AND application_id = ?1 AND run_id = ?2",
                params![application_id, run_id],
                |row| row.get(0),
            )
            .unwrap()
    }

    fn invalid_final_submit_proofs(
        valid: &FinalSubmitProof,
    ) -> Vec<(&'static str, FinalSubmitProof)> {
        let resume = valid
            .documents
            .iter()
            .find(|document| document.kind == "resume")
            .unwrap()
            .clone();

        let mut wrong_adapter = valid.clone();
        wrong_adapter.adapter = "lever".to_string();
        let mut wrong_version = valid.clone();
        wrong_version.adapter_version = "2026.07.1-beta.0".to_string();
        let mut wrong_control = valid.clone();
        wrong_control.control = "generic_submit".to_string();
        let mut wrong_approved_job = valid.clone();
        wrong_approved_job.job.approved_canonical_url =
            "https://boards.greenhouse.io/acme/jobs/another".to_string();
        let mut wrong_live_job = valid.clone();
        wrong_live_job.job.page_url = "https://boards.greenhouse.io/acme/jobs/another".to_string();
        let mut ambiguous_live_job = valid.clone();
        ambiguous_live_job.job.page_url =
            format!("{}?gh_jid=another", valid.job.approved_canonical_url);
        let mut legacy_schema = valid.clone();
        legacy_schema.schema_version = 2;
        let mut wrong_target_job = valid.clone();
        wrong_target_job.target.action_url =
            "https://boards.greenhouse.io/acme/jobs/another".to_string();
        let mut wrong_target_key = valid.clone();
        wrong_target_key.target.provider_job_key = "greenhouse:acme:another".to_string();
        let mut wrong_target_origin = valid.clone();
        wrong_target_origin.target.action_url =
            valid
                .target
                .action_url
                .replacen("boards.greenhouse.io", "job-boards.greenhouse.io", 1);
        let mut malformed_target_url = valid.clone();
        malformed_target_url.target.action_url = "not a URL".to_string();
        let mut confirmation_target_url = valid.clone();
        confirmation_target_url.target.action_url = format!(
            "{}/confirmation",
            valid.target.action_url.trim_end_matches('/'),
        );
        let mut wrong_target_method = valid.clone();
        wrong_target_method.target.method = "get".to_string();
        let mut wrong_target_enctype = valid.clone();
        wrong_target_enctype.target.enctype = "application/x-www-form-urlencoded".to_string();
        let mut wrong_form_target = valid.clone();
        wrong_form_target.target.form_target = "_blank".to_string();
        let mut invalid_form_identity = valid.clone();
        invalid_form_identity.target.form_identity.clear();
        let mut missing_file_evidence = valid.clone();
        missing_file_evidence.files.clear();
        let mut duplicate_file_evidence = valid.clone();
        duplicate_file_evidence.files[1] = duplicate_file_evidence.files[0].clone();
        let mut mismatched_file_hash = valid.clone();
        mismatched_file_hash.files[0].sha256 = "d".repeat(64);
        let mut invalid_file_field = valid.clone();
        invalid_file_field.files[0].field_name.clear();
        let mut oversized_file = valid.clone();
        oversized_file.files[0].byte_length = 12 * 1024 * 1024 + 1;
        let mut missing_field_evidence = valid.clone();
        missing_field_evidence.fields.clear();
        let mut invalid_field_name = valid.clone();
        invalid_field_name.fields[0].field_name.clear();
        let mut non_ascii_field_name = valid.clone();
        non_ascii_field_name.fields[0].field_name = "candidate_💸".to_string();
        let mut cross_type_field_overlap = valid.clone();
        cross_type_field_overlap.fields[0].field_name = valid.files[0].field_name.clone();
        let mut missing_part_order = valid.clone();
        missing_part_order.part_order.clear();
        let mut duplicate_part_order_index = valid.clone();
        duplicate_part_order_index.part_order[1] = duplicate_part_order_index.part_order[0].clone();
        let mut out_of_range_part_order_index = valid.clone();
        out_of_range_part_order_index.part_order[0].index = i64::MAX;
        let mut negative_part_order_index = valid.clone();
        negative_part_order_index.part_order[0].index = -1;
        let mut invalid_part_order_kind = valid.clone();
        invalid_part_order_kind.part_order[0].kind = "document".to_string();
        let mut oversized_field_value = valid.clone();
        oversized_field_value.fields[0].value_byte_length = 65_537;
        let mut invalid_field_hash = valid.clone();
        invalid_field_hash.fields[0].value_sha256 = "F".repeat(64);
        let mut too_many_fields = valid.clone();
        too_many_fields.fields = vec![too_many_fields.fields[0].clone(); 257];
        let mut unsorted = valid.clone();
        unsorted.documents.reverse();
        let mut duplicate = valid.clone();
        duplicate.documents = vec![resume.clone(), resume.clone()];
        let mut missing_documents = valid.clone();
        missing_documents.documents.clear();
        let mut uppercase_hash = valid.clone();
        uppercase_hash
            .documents
            .iter_mut()
            .find(|document| document.kind == "resume")
            .unwrap()
            .sha256 = "B".repeat(64);
        let mut wrong_resume_version = valid.clone();
        wrong_resume_version
            .documents
            .iter_mut()
            .find(|document| document.kind == "resume")
            .unwrap()
            .version_id = Some("wrong-resume-version".to_string());
        let mut missing_required_cover = valid.clone();
        missing_required_cover.documents = vec![resume];

        vec![
            ("wrong adapter", wrong_adapter),
            ("wrong adapter version", wrong_version),
            ("wrong control", wrong_control),
            ("wrong approved job URL", wrong_approved_job),
            ("wrong live job URL", wrong_live_job),
            ("ambiguous live job URL", ambiguous_live_job),
            ("legacy proof schema", legacy_schema),
            ("same-provider cross-job target", wrong_target_job),
            ("wrong provider job key", wrong_target_key),
            ("same-job cross-origin target", wrong_target_origin),
            ("malformed target URL", malformed_target_url),
            ("confirmation target URL", confirmation_target_url),
            ("wrong target method", wrong_target_method),
            ("wrong target enctype", wrong_target_enctype),
            ("wrong form target", wrong_form_target),
            ("invalid target form identity", invalid_form_identity),
            ("missing outgoing file evidence", missing_file_evidence),
            ("duplicate outgoing file evidence", duplicate_file_evidence),
            ("outgoing file hash mismatch", mismatched_file_hash),
            ("invalid outgoing file field", invalid_file_field),
            ("oversized outgoing file", oversized_file),
            ("missing outgoing field evidence", missing_field_evidence),
            ("invalid outgoing field name", invalid_field_name),
            ("non-ASCII outgoing field name", non_ascii_field_name),
            (
                "cross-type outgoing field overlap",
                cross_type_field_overlap,
            ),
            ("missing global part order", missing_part_order),
            ("duplicate global part index", duplicate_part_order_index),
            (
                "out-of-range global part index",
                out_of_range_part_order_index,
            ),
            ("negative global part index", negative_part_order_index),
            ("invalid global part kind", invalid_part_order_kind),
            ("oversized outgoing field value", oversized_field_value),
            ("invalid outgoing field hash", invalid_field_hash),
            ("too many outgoing fields", too_many_fields),
            ("unsorted documents", unsorted),
            ("duplicate documents", duplicate),
            ("missing documents", missing_documents),
            ("uppercase document hash", uppercase_hash),
            ("resume version mismatch", wrong_resume_version),
            ("required cover omission", missing_required_cover),
        ]
    }

    #[test]
    fn final_submit_job_binding_matches_only_the_exact_provider_job() {
        assert_eq!(
            final_submit_provider_job_key(
                "greenhouse",
                "https://boards.greenhouse.io/acme/jobs/123?gh_jid=123&JOB_ID=123#app",
            )
            .unwrap(),
            final_submit_provider_job_key(
                "greenhouse",
                "https://job-boards.greenhouse.io/embed/job_app?FOR=acme&for=acme&token=123",
            )
            .unwrap(),
        );
        assert_eq!(
            final_submit_provider_job_key(
                "lever",
                "https://jobs.lever.co/acme/posting-123?Lever_Job_Id=posting-123",
            )
            .unwrap(),
            final_submit_provider_job_key(
                "lever",
                "https://jobs.lever.co/acme/posting-123/apply?lever-origin=applied",
            )
            .unwrap(),
        );
        assert_eq!(
            final_submit_provider_job_key(
                "greenhouse",
                "https://boards.greenhouse.io/acme/jobs/123",
            )
            .unwrap(),
            final_submit_confirmation_provider_job_key(
                "greenhouse",
                "https://boards.greenhouse.io/acme/jobs/123/confirmation?Posting_Id=123",
            )
            .unwrap(),
        );
        assert_eq!(
            final_submit_provider_job_key("lever", "https://jobs.lever.co/acme/posting-123/apply",)
                .unwrap(),
            final_submit_confirmation_provider_job_key(
                "lever",
                "https://jobs.lever.co/acme/posting-123/confirmation?JOBID=posting-123",
            )
            .unwrap(),
        );
        assert!(final_submit_confirmation_provider_job_key(
            "greenhouse",
            "https://boards.greenhouse.io/acme/confirmation",
        )
        .is_err());
        assert!(final_submit_confirmation_provider_job_key(
            "greenhouse",
            "https://boards.greenhouse.io/acme/jobs/123",
        )
        .is_err());
        assert!(final_submit_provider_job_key(
            "greenhouse",
            "https://boards.greenhouse.io/acme/jobs/123/confirmation",
        )
        .is_err());
        assert!(final_submit_provider_job_key(
            "greenhouse",
            "https://boards.greenhouse.io/acme/jobs/456",
        )
        .is_ok());
        assert_ne!(
            final_submit_provider_job_key(
                "greenhouse",
                "https://boards.greenhouse.io/acme/jobs/123",
            )
            .unwrap(),
            final_submit_provider_job_key(
                "greenhouse",
                "https://boards.greenhouse.io/acme/jobs/456",
            )
            .unwrap(),
        );
        assert!(final_submit_provider_job_key(
            "greenhouse",
            "https://boards.greenhouse.io/acme/jobs/123?gh_jid=456",
        )
        .is_err());
        assert!(final_submit_provider_job_key(
            "greenhouse",
            "https://boards.greenhouse.io/acme/jobs/123?gh_jid=123&Gh_Jid=456",
        )
        .is_err());
        assert!(final_submit_provider_job_key(
            "greenhouse",
            "https://boards.greenhouse.io/acme/jobs/123?postingid=456",
        )
        .is_err());
        assert!(final_submit_provider_job_key(
            "lever",
            "https://jobs.lever.co/acme/posting-123?posting_id=posting-456",
        )
        .is_err());
        assert!(final_submit_confirmation_provider_job_key(
            "lever",
            "https://jobs.lever.co/acme/posting-123/confirmation?lever_job_id=posting-456",
        )
        .is_err());
        assert!(final_submit_confirmation_provider_job_key(
            "lever",
            "https://jobs.lever.co/acme/posting-123/apply",
        )
        .is_err());
        assert!(final_submit_confirmation_provider_job_key(
            "lever",
            "https://jobs.lever.co/acme/posting-123",
        )
        .is_err());
        assert!(final_submit_provider_job_key(
            "lever",
            "https://jobs.lever.co/acme/posting-123/another",
        )
        .is_err());
    }

    #[test]
    fn final_submit_proof_wire_requires_exact_target_fields_and_denies_extras() {
        let pool = test_pool();
        let (application, _, _) = execution_lease_fixture(&pool, "proof-wire-target");
        let valid = serde_json::to_value(test_final_submit_proof(&application)).unwrap();

        let mut missing_target = valid.clone();
        missing_target.as_object_mut().unwrap().remove("target");
        assert!(serde_json::from_value::<FinalSubmitProof>(missing_target).is_err());

        let mut missing_enctype = valid.clone();
        missing_enctype["target"]
            .as_object_mut()
            .unwrap()
            .remove("enctype");
        assert!(serde_json::from_value::<FinalSubmitProof>(missing_enctype).is_err());

        let mut missing_form_target = valid.clone();
        missing_form_target["target"]
            .as_object_mut()
            .unwrap()
            .remove("formTarget");
        assert!(serde_json::from_value::<FinalSubmitProof>(missing_form_target).is_err());

        let mut extra_target_field = valid.clone();
        extra_target_field["target"]["unexpected"] = json!(true);
        assert!(serde_json::from_value::<FinalSubmitProof>(extra_target_field).is_err());

        let mut missing_files = valid.clone();
        missing_files.as_object_mut().unwrap().remove("files");
        assert!(serde_json::from_value::<FinalSubmitProof>(missing_files).is_err());

        let mut extra_file_field = valid.clone();
        extra_file_field["files"][0]["unexpected"] = json!(true);
        assert!(serde_json::from_value::<FinalSubmitProof>(extra_file_field).is_err());

        let mut missing_fields = valid.clone();
        missing_fields.as_object_mut().unwrap().remove("fields");
        assert!(serde_json::from_value::<FinalSubmitProof>(missing_fields).is_err());

        let mut missing_part_order = valid.clone();
        missing_part_order
            .as_object_mut()
            .unwrap()
            .remove("partOrder");
        assert!(serde_json::from_value::<FinalSubmitProof>(missing_part_order).is_err());

        let mut extra_part_order_key = valid.clone();
        extra_part_order_key["partOrder"][0]["unexpected"] = json!(true);
        assert!(serde_json::from_value::<FinalSubmitProof>(extra_part_order_key).is_err());

        let mut extra_field_evidence_key = valid.clone();
        extra_field_evidence_key["fields"][0]["unexpected"] = json!(true);
        assert!(serde_json::from_value::<FinalSubmitProof>(extra_field_evidence_key).is_err());

        let mut extra_proof_field = valid;
        extra_proof_field["unexpected"] = json!(true);
        assert!(serde_json::from_value::<FinalSubmitProof>(extra_proof_field).is_err());
    }

    #[test]
    fn frozen_certification_is_distinct_from_track_auto_review_authority() {
        let pool = test_pool();
        let (mut application, _, _) = execution_lease_fixture(&pool, "proof-certified-admission");
        application.submission_mode = "auto_submit".to_string();
        application.receipt["approved_execution"]["admission"] = json!({
            "kind": "track_auto_submit",
            "authorization_id": "authorization-1",
            "career_track_id": "track-1",
            "revision_no": 1,
            "authority_fingerprint": "a".repeat(64),
        });
        assert!(!application_has_frozen_ats_certification(&application));

        application.receipt["approved_execution"]["schema_version"] = json!(3);
        application.receipt["approved_execution"]["admission"]["ats_certification"] = json!({
            "schema_version": 1,
        });
        assert!(application_has_frozen_ats_certification(&application));

        application.receipt["approved_execution"]["admission"]["kind"] = json!("review_approval");
        assert!(!application_has_frozen_ats_certification(&application));
    }

    #[test]
    fn final_submit_surface_digest_matches_shared_automation_vectors() {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../../../jobs/automation/tests/fixtures/final-submit-surface-vectors.json"
        ))
        .unwrap();
        for vector in fixture["vectors"].as_array().unwrap() {
            let proof: FinalSubmitProof = serde_json::from_value(vector["proof"].clone()).unwrap();
            assert_eq!(
                final_submit_surface_sha256(&proof).unwrap(),
                vector["expectedSurfaceSha256"].as_str().unwrap(),
                "{}",
                vector["name"].as_str().unwrap()
            );
        }
    }

    #[test]
    fn cloud_final_submit_proof_is_strict_and_atomic_with_click_and_capacity() {
        let pool = test_pool();
        let (application, run_id, browser_profile_id) =
            execution_lease_fixture(&pool, "proof-cloud-strict");
        let application = require_frozen_cover_letter(&pool, application);
        let lease = claim_execution_lease(
            &pool,
            "acct-jobs",
            &application.id,
            &run_id,
            &browser_profile_id,
            "proof-cloud-worker",
        )
        .unwrap();
        install_final_submit_proof_order_triggers(&pool);
        let valid = test_final_submit_proof(&application);
        assert_eq!(
            valid
                .documents
                .iter()
                .map(|document| document.kind.as_str())
                .collect::<Vec<_>>(),
            vec!["cover_letter", "resume"]
        );
        let capacity = test_submission_evidence_capacity(&application.id, &run_id);

        for (label, invalid) in invalid_final_submit_proofs(&valid) {
            assert!(
                matches!(
                    start_irreversible_submission(
                        &pool,
                        "acct-jobs",
                        &application.id,
                        &run_id,
                        &lease.lease_token,
                        lease.fence,
                        &invalid,
                        &capacity,
                    ),
                    Err(ExecutionLeaseError::InvalidRequest)
                ),
                "{label} must fail closed"
            );
            let stored = get_application(&pool, "acct-jobs", &application.id)
                .unwrap()
                .unwrap();
            assert!(
                stored.receipt.get(FINAL_SUBMIT_PROOF_KEY).is_none(),
                "{label}"
            );
            assert_eq!(
                submission_capacity_count(&pool, &application.id, &run_id),
                0
            );
            let phase: String = pool
                .get()
                .unwrap()
                .query_row(
                    "SELECT phase FROM jobs_execution_leases WHERE run_id = ?1",
                    params![run_id],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(phase, "prepared", "{label}");
        }

        let mut quota_blocked = capacity.clone();
        quota_blocked.limits.max_account_bytes = quota_blocked.reserved_bytes;
        quota_blocked.limits.max_account_objects = quota_blocked.reserved_objects;
        pool.get()
            .unwrap()
            .execute(
                "INSERT INTO object_uploads (
                    id, account_id, object_kind, logical_id, storage_scope, object_key,
                    size_bytes, sha256, content_type, expires_at_ms, state, metadata_json,
                    created_at_ms, updated_at_ms
                 ) VALUES (
                    'proof-cloud-existing-object', 'acct-jobs', 'session_audit',
                    'proof-cloud-existing-object', 'audit',
                    'objects/accounts/acct-jobs/proof-cloud-existing-object', 1, ?1,
                    'application/json', ?2, 'ready', '{}', ?3, ?3
                 )",
                params!["d".repeat(64), i64::MAX, now_ms()],
            )
            .unwrap();
        assert!(start_irreversible_submission(
            &pool,
            "acct-jobs",
            &application.id,
            &run_id,
            &lease.lease_token,
            lease.fence,
            &valid,
            &quota_blocked,
        )
        .is_err());
        let stored = get_application(&pool, "acct-jobs", &application.id)
            .unwrap()
            .unwrap();
        assert!(stored.receipt.get(FINAL_SUBMIT_PROOF_KEY).is_none());
        assert_eq!(
            submission_capacity_count(&pool, &application.id, &run_id),
            0
        );
        assert_eq!(
            pool.get()
                .unwrap()
                .query_row(
                    "SELECT phase FROM jobs_execution_leases WHERE run_id = ?1",
                    params![run_id],
                    |row| row.get::<_, String>(0),
                )
                .unwrap(),
            "prepared"
        );
        pool.get()
            .unwrap()
            .execute(
                "DELETE FROM object_uploads WHERE id = 'proof-cloud-existing-object'",
                [],
            )
            .unwrap();

        let started = start_irreversible_submission(
            &pool,
            "acct-jobs",
            &application.id,
            &run_id,
            &lease.lease_token,
            lease.fence,
            &valid,
            &quota_blocked,
        )
        .unwrap();
        assert_eq!(started.phase, "click_started");
        let stored = get_application(&pool, "acct-jobs", &application.id)
            .unwrap()
            .unwrap();
        assert_eq!(stored_final_submit_proof(&stored).unwrap(), valid);
        assert_eq!(
            submission_capacity_count(&pool, &application.id, &run_id),
            1
        );
    }

    #[test]
    fn cloud_final_submit_proof_exact_replay_requires_pre_click_authority() {
        let pool = test_pool();
        let (application, run_id, browser_profile_id) =
            execution_lease_fixture(&pool, "proof-cloud-replay");
        let proof = test_final_submit_proof(&application);
        let application = store_test_final_submit_proof(&pool, application, &proof);
        let lease = claim_execution_lease(
            &pool,
            "acct-jobs",
            &application.id,
            &run_id,
            &browser_profile_id,
            "proof-replay-worker",
        )
        .unwrap();
        install_final_submit_proof_order_triggers(&pool);
        let capacity = test_submission_evidence_capacity(&application.id, &run_id);
        start_irreversible_submission(
            &pool,
            "acct-jobs",
            &application.id,
            &run_id,
            &lease.lease_token,
            lease.fence,
            &proof,
            &capacity,
        )
        .unwrap();
        assert!(matches!(
            start_irreversible_submission(
                &pool,
                "acct-jobs",
                &application.id,
                &run_id,
                &lease.lease_token,
                lease.fence,
                &proof,
                &capacity,
            ),
            Err(ExecutionLeaseError::Conflict)
        ));
        assert_eq!(
            submission_capacity_count(&pool, &application.id, &run_id),
            1
        );

        let changed_pool = test_pool();
        let (changed_application, changed_run_id, changed_profile_id) =
            execution_lease_fixture(&changed_pool, "proof-cloud-conflict");
        let original = test_final_submit_proof(&changed_application);
        let changed_application =
            store_test_final_submit_proof(&changed_pool, changed_application, &original);
        let changed_lease = claim_execution_lease(
            &changed_pool,
            "acct-jobs",
            &changed_application.id,
            &changed_run_id,
            &changed_profile_id,
            "proof-conflict-worker",
        )
        .unwrap();
        let mut changed = original.clone();
        changed.fields[0].value_sha256 = "e".repeat(64);
        assert!(matches!(
            start_irreversible_submission(
                &changed_pool,
                "acct-jobs",
                &changed_application.id,
                &changed_run_id,
                &changed_lease.lease_token,
                changed_lease.fence,
                &changed,
                &test_submission_evidence_capacity(&changed_application.id, &changed_run_id,),
            ),
            Err(ExecutionLeaseError::Conflict)
        ));
        let stored = get_application(&changed_pool, "acct-jobs", &changed_application.id)
            .unwrap()
            .unwrap();
        assert_eq!(stored_final_submit_proof(&stored).unwrap(), original);
        assert_eq!(
            submission_capacity_count(&changed_pool, &changed_application.id, &changed_run_id,),
            0
        );
        assert_eq!(
            changed_pool
                .get()
                .unwrap()
                .query_row(
                    "SELECT phase FROM jobs_execution_leases WHERE run_id = ?1",
                    params![changed_run_id],
                    |row| row.get::<_, String>(0),
                )
                .unwrap(),
            "prepared"
        );
    }

    #[test]
    fn local_final_submit_proof_and_capacity_commit_before_click_or_roll_back_together() {
        let pool = test_pool();
        let (application, run_id, ticket_hash, _) =
            local_run_authority_fixture(&pool, "proof-local-atomic");
        assert!(
            claim_authorized_local_run_ticket(&pool, &run_id, &ticket_hash)
                .unwrap()
                .is_some()
        );
        let application = update_application(&pool, "acct-jobs", &application.id, "running", None)
            .unwrap()
            .unwrap();
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
        install_final_submit_proof_order_triggers(&pool);
        let proof = test_final_submit_proof(&application);
        let mut invalid = proof.clone();
        invalid.control = "generic_submit".to_string();
        let mut capacity = test_submission_evidence_capacity(&application.id, &run_id);
        capacity.runner = "local".to_string();
        let application_before_invalid = serde_json::to_value(&application).unwrap();

        assert!(
            !local_run_submit_authorized(&pool, &run_id, &ticket_hash, &invalid, &capacity,)
                .unwrap()
        );
        let stored = get_application(&pool, "acct-jobs", &application.id)
            .unwrap()
            .unwrap();
        assert_eq!(
            serde_json::to_value(&stored).unwrap(),
            application_before_invalid
        );
        assert!(stored.receipt.get(FINAL_SUBMIT_PROOF_KEY).is_none());
        assert_eq!(
            submission_capacity_count(&pool, &application.id, &run_id),
            0
        );
        assert_eq!(
            pool.get()
                .unwrap()
                .query_row(
                    "SELECT status FROM jobs_local_run_tickets WHERE id = ?1",
                    params![run_id],
                    |row| row.get::<_, String>(0),
                )
                .unwrap(),
            "claimed"
        );

        let application = store_test_final_submit_proof(&pool, stored, &proof);
        let application_before_conflict = serde_json::to_value(&application).unwrap();
        let mut conflicting = proof.clone();
        let conflicting_sha256 = "e".repeat(64);
        conflicting
            .documents
            .iter_mut()
            .find(|document| document.kind == "resume")
            .unwrap()
            .sha256 = conflicting_sha256.clone();
        let conflicting_file = conflicting
            .files
            .iter_mut()
            .find(|file| file.name.starts_with("resume-"))
            .unwrap();
        conflicting_file.name = format!("resume-{conflicting_sha256}.pdf");
        conflicting_file.sha256 = conflicting_sha256;
        assert!(!local_run_submit_authorized(
            &pool,
            &run_id,
            &ticket_hash,
            &conflicting,
            &capacity,
        )
        .unwrap());
        let stored = get_application(&pool, "acct-jobs", &application.id)
            .unwrap()
            .unwrap();
        assert_eq!(
            serde_json::to_value(&stored).unwrap(),
            application_before_conflict
        );
        assert_eq!(stored_final_submit_proof(&stored).unwrap(), proof);
        assert_eq!(
            submission_capacity_count(&pool, &application.id, &run_id),
            0
        );
        assert_eq!(
            pool.get()
                .unwrap()
                .query_row(
                    "SELECT status FROM jobs_local_run_tickets WHERE id = ?1",
                    params![run_id],
                    |row| row.get::<_, String>(0),
                )
                .unwrap(),
            "claimed"
        );

        assert!(
            local_run_submit_authorized(&pool, &run_id, &ticket_hash, &proof, &capacity,).unwrap()
        );
        let stored = get_application(&pool, "acct-jobs", &application.id)
            .unwrap()
            .unwrap();
        assert_eq!(stored_final_submit_proof(&stored).unwrap(), proof);
        assert_eq!(
            submission_capacity_count(&pool, &application.id, &run_id),
            1
        );
        assert_eq!(
            pool.get()
                .unwrap()
                .query_row(
                    "SELECT status FROM jobs_local_run_tickets WHERE id = ?1",
                    params![run_id],
                    |row| row.get::<_, String>(0),
                )
                .unwrap(),
            "click_started"
        );
        assert!(
            !local_run_submit_authorized(&pool, &run_id, &ticket_hash, &proof, &capacity,).unwrap()
        );
    }

    #[test]
    fn submission_evidence_capacity_uses_the_exact_storage_bundle_cap() {
        let one_mib = 1024 * 1024;
        let one_mib_capacity =
            test_submission_evidence_capacity_with_object_cap("application", "run", one_mib);
        assert_eq!(
            one_mib_capacity.reserved_bytes,
            SUBMISSION_EVIDENCE_PAYLOAD_RESERVED_BYTES + one_mib
        );
        validate_submission_evidence_capacity_binding(
            "acct-jobs",
            "application",
            "run",
            &one_mib_capacity,
            one_mib_capacity.now_ms,
        )
        .unwrap();

        let larger_than_bundle_cap = 16 * 1024 * 1024;
        let capped_capacity = test_submission_evidence_capacity_with_object_cap(
            "application",
            "run",
            larger_than_bundle_cap,
        );
        assert_eq!(
            capped_capacity.reserved_bytes,
            SUBMISSION_EVIDENCE_PAYLOAD_RESERVED_BYTES
                + SUBMISSION_RECEIPT_BUNDLE_MAX_RESERVED_BYTES
        );
        validate_submission_evidence_capacity_binding(
            "acct-jobs",
            "application",
            "run",
            &capped_capacity,
            capped_capacity.now_ms,
        )
        .unwrap();

        let mut oversized = one_mib_capacity;
        oversized.reserved_bytes += 1;
        assert!(matches!(
            validate_submission_evidence_capacity_binding(
                "acct-jobs",
                "application",
                "run",
                &oversized,
                oversized.now_ms,
            ),
            Err(ExecutionLeaseError::InvalidRequest)
        ));
    }

    #[allow(clippy::too_many_arguments)]
    fn reserve_submission_object(
        pool: &DbPool,
        application_id: &str,
        run_id: &str,
        runner: &str,
        evidence_kind: &str,
        object_key: &str,
        size_bytes: i64,
        sha256: &str,
        content_type: &str,
    ) -> ApplicationObjectBinding {
        let now = now_ms();
        let reservation = crate::db::object_uploads::reserve_application_object_upload(
            pool,
            application_id,
            &NewObjectUpload {
                account_id: "acct-jobs".to_string(),
                object_kind: ObjectKind::Artifact,
                logical_id: format!("jobs-submission:{application_id}:{evidence_kind}"),
                session_id: None,
                storage_scope: StorageScope::Artifact,
                object_key: object_key.to_string(),
                size_bytes,
                sha256: sha256.to_string(),
                content_type: content_type.to_string(),
                expires_at_ms: now + 86_400_000,
                metadata_json: json!({
                    "artifact_class": "jobs_submission_evidence",
                    "jobs_application_id": application_id,
                    "jobs_run_id": run_id,
                    "jobs_runner": runner,
                    "evidence_kind": evidence_kind,
                }),
                now_ms: now,
                limits: UploadLimits {
                    max_object_bytes: 1_024,
                    max_account_bytes: 4_096,
                    max_daily_bytes: 4_096,
                    max_account_objects: 10,
                },
            },
        )
        .unwrap();
        assert!(reservation.needs_put);
        ApplicationObjectBinding {
            upload_id: reservation.upload.id,
            object_key: reservation.upload.object_key,
            size_bytes: reservation.upload.size_bytes,
            sha256: reservation.upload.sha256,
            content_type: reservation.upload.content_type,
        }
    }

    fn final_submission_fixture(pool: &DbPool, suffix: &str) -> FinalSubmissionFixture {
        set_entitlement_plan(pool, "acct-jobs", "pro").unwrap();
        let (mut application, run_id, browser_profile_id) = execution_lease_fixture(pool, suffix);
        let posting = get_posting(pool, "acct-jobs", &application.job_id)
            .unwrap()
            .unwrap();
        let resume = get_resume_version(
            pool,
            "acct-jobs",
            application
                .resume_version_id
                .as_deref()
                .expect("fixture application has a resume"),
        )
        .unwrap()
        .unwrap();
        let identity_id = application
            .receipt
            .pointer("/application_identity/id")
            .and_then(Value::as_str)
            .unwrap()
            .to_string();
        let identity_email = application
            .receipt
            .pointer("/application_identity/email")
            .and_then(Value::as_str)
            .unwrap()
            .to_string();
        let confirmation_url = format!(
            "{}/confirmation",
            posting.canonical_url.trim_end_matches('/')
        );
        let approved_at_ms = now_ms();
        let approved_packet = json!({
            "applicationId": application.id,
            "jobId": application.job_id,
            "resumeVersionId": resume.id,
            "resumeContent": resume.content,
            "coverLetterContent": application.cover_letter,
            "answers": {
                "application_email": identity_email,
                "sponsorship_required": "No",
            },
            "verifiedClaimIds": resume.claim_ids,
            "applicationIdentityId": identity_id,
            "applicationEmail": identity_email,
            "browserProfileId": browser_profile_id,
        });
        let approved_job = json!({
            "externalId": posting.external_id,
            "canonicalUrl": posting.canonical_url,
            "company": posting.company,
            "title": posting.title,
            "location": posting.location,
            "workplace": posting.workplace,
            "description": posting.description,
            "source": "greenhouse",
            "compensation": posting.compensation,
        });
        let admission = json!({ "kind": "review_approval" });
        let checksum =
            approved_submission_checksum(2, &approved_packet, &approved_job, Some(&admission))
                .unwrap();
        application.receipt["approved_execution"] = json!({
            "schema_version": 2,
            "approved_at_ms": approved_at_ms,
            "checksum": checksum,
            "admission": admission,
            "packet": approved_packet,
            "job": approved_job,
        });
        application.receipt["packet_revisions"] = json!([{
            "revision_no": 2,
            "reason": "intervention_answer_changed",
            "created_at_ms": approved_at_ms,
        }]);
        application = replace_application_receipt(
            pool,
            "acct-jobs",
            &application.id,
            application.receipt.clone(),
        )
        .unwrap()
        .unwrap();
        application = update_application(pool, "acct-jobs", &application.id, "running", None)
            .unwrap()
            .unwrap();
        reserve_application_attempt(pool, "acct-jobs", &application.id, "cloud").unwrap();
        update_attempt_reservation_status(pool, "acct-jobs", &application.id, "running").unwrap();
        let lease = claim_execution_lease(
            pool,
            "acct-jobs",
            &application.id,
            &run_id,
            &browser_profile_id,
            &format!("{suffix}-worker"),
        )
        .unwrap();
        let capacity = test_submission_evidence_capacity(&application.id, &run_id);
        let final_submit_proof = test_final_submit_proof(&application);
        start_irreversible_submission(
            pool,
            "acct-jobs",
            &application.id,
            &run_id,
            &lease.lease_token,
            lease.fence,
            &final_submit_proof,
            &capacity,
        )
        .unwrap();
        application = get_application(pool, "acct-jobs", &application.id)
            .unwrap()
            .expect("irreversible start keeps the application");
        assert_eq!(
            stored_final_submit_proof(&application).unwrap(),
            final_submit_proof
        );
        finish_execution_lease(
            pool,
            "acct-jobs",
            &application.id,
            &run_id,
            &lease.lease_token,
            lease.fence,
            "submitted",
        )
        .unwrap();

        let fingerprint = "a".repeat(64);
        let resume_key = format!("accounts/acct-jobs/jobs/{suffix}/resume.pdf");
        let confirmation_key = format!("accounts/acct-jobs/jobs/{suffix}/confirmation.png");
        let receipt_key = format!("accounts/acct-jobs/jobs/{suffix}/receipt.json");
        let resume_sha256 = "b".repeat(64);
        let confirmation_sha256 = "c".repeat(64);
        let receipt_sha256 = "d".repeat(64);
        let receipt_size_bytes = 123;
        let mut receipt = json!({
            "schemaVersion": 1,
            "receiptId": format!("receipt-{suffix}"),
            "accountId": "acct-jobs",
            "applicationId": application.id,
            "runId": run_id,
            "runner": "cloud",
            "applicationIdentityId": identity_id,
            "browserProfileId": browser_profile_id,
            "adapter": "greenhouse",
            "adapterVersion": "2026.07.1-beta.1",
            "job": application.receipt["approved_execution"]["job"],
            "packet": {
                "jobId": application.receipt["approved_execution"]["packet"]["jobId"],
                "resumeVersionId": application.receipt["approved_execution"]["packet"]["resumeVersionId"],
                "approvedPacketChecksum": application.receipt["approved_execution"]["checksum"],
                "answers": application.receipt["approved_execution"]["packet"]["answers"],
                "verifiedClaimIds": application.receipt["approved_execution"]["packet"]["verifiedClaimIds"],
                "applicationEmail": application.receipt["approved_execution"]["packet"]["applicationEmail"],
            },
            "_bluey_server_submission_fingerprint_v1": fingerprint,
            "documents": [{
                "kind": "resume",
                "versionId": application.resume_version_id,
                "storageKey": resume_key,
                "sha256": resume_sha256,
                "mediaType": "application/pdf",
            }],
            "events": [],
            "result": {
                "status": "submitted",
                "submitHttpStatus": 302,
                "confirmationText": "Application received",
                "confirmationUrl": confirmation_url,
                "submittedAt": "2026-08-04T12:00:00.000Z",
                "issues": [],
            },
            "finalUrl": confirmation_url,
            "screenshotKeys": [confirmation_key],
            "evidenceObjects": [{
                "kind": "screenshot",
                "storageKey": confirmation_key,
                "sha256": confirmation_sha256,
                "mediaType": "image/png",
                "sizeBytes": 84,
            }, {
                "kind": "resume",
                "storageKey": resume_key,
                "sha256": resume_sha256,
                "mediaType": "application/pdf",
                "sizeBytes": 42,
            }],
            "receiptObject": {
                "storageKey": receipt_key,
                "sha256": receipt_sha256,
                "mediaType": "application/json",
                "sizeBytes": receipt_size_bytes,
                "schemaVersion": 1,
            },
        });
        let pre_submission_receipt_sha256 =
            submission_pre_receipt_sha256(&application.receipt).unwrap();
        receipt.as_object_mut().unwrap().insert(
            SERVER_SUBMISSION_AUTHORITY_KEY.to_string(),
            json!({
                "schemaVersion": 1,
                "preSubmissionReceipt": application.receipt,
                "preSubmissionReceiptSha256": pre_submission_receipt_sha256,
                "executionAuthority": {
                    "kind": "cloud_execution_lease",
                    "ownerId": format!("{suffix}-worker"),
                    "leaseTokenSha256": execution_lease_token_hash(&lease.lease_token),
                    "fence": lease.fence,
                    "phase": "submitted",
                },
            }),
        );
        let evidence = vec![
            ApplicationEvidence {
                id: String::new(),
                application_id: application.id.clone(),
                kind: "resume".to_string(),
                label: "Resume submitted".to_string(),
                provider: "greenhouse".to_string(),
                file_name: "resume.pdf".to_string(),
                media_type: "application/pdf".to_string(),
                storage_key: resume_key.clone(),
                sha256: resume_sha256.clone(),
                resume_version_id: application.resume_version_id.clone(),
                occurred_at_ms: 0,
                metadata: json!({ "size_bytes": 42 }),
                created_at_ms: 0,
            },
            ApplicationEvidence {
                id: String::new(),
                application_id: application.id.clone(),
                kind: "application_receipt".to_string(),
                label: "Application receipt bundle".to_string(),
                provider: "greenhouse".to_string(),
                file_name: format!("receipt-{suffix}.json"),
                media_type: "application/json".to_string(),
                storage_key: receipt_key.clone(),
                sha256: receipt_sha256.clone(),
                resume_version_id: application.resume_version_id.clone(),
                occurred_at_ms: 0,
                metadata: json!({
                    "receipt_id": format!("receipt-{suffix}"),
                    "schema_version": 1,
                    "immutable": true,
                    "size_bytes": receipt_size_bytes,
                }),
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
                storage_key: confirmation_key.clone(),
                sha256: confirmation_sha256.clone(),
                resume_version_id: application.resume_version_id.clone(),
                occurred_at_ms: 0,
                metadata: json!({
                    "immutable": true,
                    "confirmation": "Application received",
                    "evidence_strength": "browser_confirmed",
                    "receipt_id": format!("receipt-{suffix}"),
                    "screenshot_keys": [confirmation_key],
                    "screenshot_index": 1,
                    "screenshot_count": 1,
                    "size_bytes": 84,
                }),
                created_at_ms: 0,
            },
        ];
        let object_uploads = vec![
            reserve_submission_object(
                pool,
                &application.id,
                &run_id,
                "cloud",
                "resume",
                &resume_key,
                42,
                &resume_sha256,
                "application/pdf",
            ),
            reserve_submission_object(
                pool,
                &application.id,
                &run_id,
                "cloud",
                "submission_confirmation",
                &confirmation_key,
                84,
                &confirmation_sha256,
                "image/png",
            ),
            reserve_submission_object(
                pool,
                &application.id,
                &run_id,
                "cloud",
                "application_receipt",
                &receipt_key,
                receipt_size_bytes,
                &receipt_sha256,
                "application/json",
            ),
        ];
        let mut session = list_browser_sessions(pool, "acct-jobs")
            .unwrap()
            .into_iter()
            .find(|session| session.id == run_id)
            .unwrap();
        session.status = "complete".to_string();

        FinalSubmissionFixture {
            application,
            run_id,
            lease_token: lease.lease_token,
            lease_fence: lease.fence,
            fingerprint,
            receipt,
            evidence,
            object_uploads,
            session,
        }
    }

    fn local_run_authority_fixture_unbound(
        pool: &DbPool,
        suffix: &str,
    ) -> (JobApplication, String, String, String) {
        let (profile, preferences) =
            execution_policy_fixture(pool, "acct-jobs", "jobs@example.com", "track-default");
        set_entitlement_plan(pool, "acct-jobs", "pro").unwrap();
        let mut posting_input = test_posting(
            &format!("https://boards.greenhouse.io/acme/jobs/{suffix}"),
            now_ms(),
            now_ms(),
        );
        posting_input.source = "greenhouse_import".to_string();
        posting_input.external_id = suffix.to_string();
        posting_input.match_score = 90;
        posting_input.canonical_key = canonical_job_key(&posting_input);
        posting_input.discovery_evidence = JobDiscoveryEvidence::default();
        let (posting, managed) = save_production_positive_verified_import(
            pool,
            "acct-jobs",
            &posting_input,
            &profile,
            &preferences,
        );
        let authority_fixture = install_production_positive_job_authorities(
            pool,
            "acct-jobs",
            &posting,
            &managed,
            "acme.com",
            suffix,
        );
        let (application, _) =
            prepare_application(pool, "acct-jobs", &posting.id, "factual", "review_first").unwrap();
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
        let authority = current_application_approval_authority(
            pool,
            "acct-jobs",
            "jobs@example.com",
            &application.id,
        )
        .unwrap()
        .expect("current production-positive local application approval authority");
        assert_eq!(authority.posting.id, authority_fixture.posting.id);
        let identity_id = authority
            .application
            .receipt
            .pointer("/application_identity/id")
            .and_then(Value::as_str)
            .unwrap()
            .to_string();
        let browser_profile_id = execution_browser_profile_id("acct-jobs", &identity_id);
        let identity_email = authority
            .application
            .receipt
            .pointer("/application_identity/email")
            .and_then(Value::as_str)
            .unwrap()
            .to_string();
        let resume = get_resume_version(
            pool,
            "acct-jobs",
            authority
                .application
                .resume_version_id
                .as_deref()
                .expect("fixture application has a resume"),
        )
        .unwrap()
        .unwrap();
        let approved_packet = json!({
            "applicationId": authority.application.id,
            "jobId": authority.posting.id,
            "resumeVersionId": resume.id,
            "resumeContent": resume.content,
            "coverLetterContent": authority.application.cover_letter,
            "answers": {},
            "verifiedClaimIds": resume.claim_ids,
            "applicationIdentityId": identity_id,
            "applicationEmail": identity_email,
            "browserProfileId": browser_profile_id,
        });
        let approved_job = json!({
            "externalId": authority.posting.external_id,
            "canonicalUrl": authority.posting.canonical_url,
            "company": authority.posting.company,
            "title": authority.posting.title,
            "location": authority.posting.location,
            "workplace": authority.posting.workplace,
            "description": authority.posting.description,
            "source": authority.posting.source,
            "compensation": authority.posting.compensation,
        });
        let admission = json!({ "kind": "review_approval" });
        let checksum =
            approved_submission_checksum(2, &approved_packet, &approved_job, Some(&admission))
                .unwrap();
        let approved_execution = json!({
            "schema_version": 2,
            "approved_at_ms": authority.evaluated_at_ms,
            "checksum": checksum,
            "admission": admission,
            "packet": approved_packet,
            "job": approved_job,
        });
        let application = persist_current_application_approval(
            pool,
            "acct-jobs",
            "jobs@example.com",
            &authority,
            &approved_execution,
        )
        .unwrap()
        .expect("persist production-positive local application approval");
        reserve_application_attempt(pool, "acct-jobs", &application.id, "local").unwrap();
        let application = update_application(pool, "acct-jobs", &application.id, "queued", None)
            .unwrap()
            .expect("queue production-positive local application");
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
                "url": authority.posting.canonical_url,
                "runId": run_id,
            }),
            now_ms() + 60_000,
        )
        .unwrap();
        (application, run_id, ticket_hash, identity_id)
    }

    fn local_run_authority_fixture(
        pool: &DbPool,
        suffix: &str,
    ) -> (JobApplication, String, String, String) {
        let fixture = local_run_authority_fixture_unbound(pool, suffix);
        seed_test_local_browser_release_binding(pool, &fixture.0.id, &fixture.1);
        fixture
    }

    #[derive(Debug, Clone)]
    struct TestBrowserReleaseAuthority {
        manifest_sha256: String,
        artifact_sha256: String,
        descriptor_sha256: String,
        activation_sha256: String,
        manifest_signature_set_sha256: String,
        activation_signature_set_sha256: String,
        policy_sha256: String,
        policy_signature_set_sha256: String,
        transition_sha256: String,
        assignment_sha256: String,
        signature: String,
    }

    fn test_browser_release_policy_fixture() -> (String, String) {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../../../jobs/browser/fixtures/release-authority-v1.json"
        ))
        .expect("parse shared Browser release fixture");
        let canonical = fixture
            .pointer("/trustPolicy/canonical")
            .and_then(Value::as_str)
            .expect("shared Browser release policy canonical bytes");
        let sha256 = fixture
            .pointer("/trustPolicy/sha256")
            .and_then(Value::as_str)
            .expect("shared Browser release policy digest");
        (canonical.to_string(), sha256.to_string())
    }

    fn test_browser_release_authority_ids() -> TestBrowserReleaseAuthority {
        let (_, policy_sha256) = test_browser_release_policy_fixture();
        TestBrowserReleaseAuthority {
            manifest_sha256: "1".repeat(64),
            artifact_sha256: "2".repeat(64),
            descriptor_sha256: "3".repeat(64),
            activation_sha256: "4".repeat(64),
            manifest_signature_set_sha256: "5".repeat(64),
            activation_signature_set_sha256: "b".repeat(64),
            policy_sha256,
            policy_signature_set_sha256: "a".repeat(64),
            transition_sha256: "c".repeat(64),
            assignment_sha256: "6".repeat(64),
            signature: "A".repeat(86),
        }
    }

    fn seed_test_browser_release_authority_rows(pool: &DbPool) -> TestBrowserReleaseAuthority {
        let authority = test_browser_release_authority_ids();
        let (canonical_policy_base64url, _) = test_browser_release_policy_fixture();
        let connection = pool.get().unwrap();

        for (
            signature_set_id,
            signature_set_sha256,
            role,
            target_audience,
            target_sha256,
            signer_key_id,
        ) in [
            (
                "test-policy-signatures",
                authority.policy_signature_set_sha256.as_str(),
                "root",
                "bluey-jobs-browser-release-trust-policy-v1",
                authority.policy_sha256.as_str(),
                "test-root-key",
            ),
            (
                "test-manifest-signatures",
                authority.manifest_signature_set_sha256.as_str(),
                "release",
                "bluey-jobs-browser-release-manifest-v1",
                authority.manifest_sha256.as_str(),
                "test-build-key",
            ),
            (
                "test-activation-signatures",
                authority.activation_signature_set_sha256.as_str(),
                "promotion",
                "bluey-jobs-browser-release-activation-v1",
                authority.activation_sha256.as_str(),
                "test-promotion-key",
            ),
        ] {
            connection
                .execute(
                    "INSERT OR IGNORE INTO jobs_browser_release_signature_sets (
                        signature_set_sha256, signature_set_id, trust_generation,
                        role, target_audience, target_sha256, signed_at_ms,
                        signature_count, canonical_signature_set_base64url,
                        recorded_by, recorded_at_ms
                     ) VALUES (
                        ?1, ?2, 1, ?3, ?4, ?5, 1, 1, 'dGVzdA',
                        'test-suite', 1
                     )",
                    params![
                        signature_set_sha256,
                        signature_set_id,
                        role,
                        target_audience,
                        target_sha256,
                    ],
                )
                .unwrap();
            connection
                .execute(
                    "INSERT OR IGNORE INTO jobs_browser_release_signatures (
                        signature_set_sha256, key_id, signature_base64url
                     ) VALUES (?1, ?2, ?3)",
                    params![signature_set_sha256, signer_key_id, authority.signature],
                )
                .unwrap();
        }

        connection
            .execute(
                "INSERT OR IGNORE INTO jobs_browser_release_trust_policies (
                    policy_sha256, policy_id, trust_generation,
                    predecessor_policy_sha256, predecessor_trust_generation,
                    root_threshold, release_threshold, promotion_threshold,
                    incident_threshold, key_count, canonical_policy_base64url,
                    authorization_signature_set_sha256, issued_at_ms,
                    valid_from_ms, expires_at_ms, recorded_by, recorded_at_ms
                 ) VALUES (
                    ?1, 'test-policy', 1, NULL, 0, 1, 1, 1, 1, 4,
                    ?2, ?3, 1, 0, 9007199254740991, 'test-suite', 1
                 )",
                params![
                    authority.policy_sha256,
                    canonical_policy_base64url,
                    authority.policy_signature_set_sha256
                ],
            )
            .unwrap();
        for (key_id, role) in [
            ("test-root-key", "root"),
            ("test-build-key", "release"),
            ("test-promotion-key", "promotion"),
            ("test-incident-key", "incident"),
        ] {
            connection
                .execute(
                    "INSERT OR IGNORE INTO jobs_browser_release_trust_keys (
                        policy_sha256, trust_generation, key_id, role,
                        public_key_base64url, state, valid_from_ms, valid_until_ms,
                        minimum_trust_generation, maximum_trust_generation
                     ) VALUES (
                        ?1, 1, ?2, ?3, ?4, 'active', 0, 9007199254740991,
                        1, 9007199254740991
                     )",
                    params![authority.policy_sha256, key_id, role, "A".repeat(43)],
                )
                .unwrap();
        }

        connection
            .execute(
                "INSERT OR IGNORE INTO jobs_browser_release_manifests (
                    manifest_sha256, manifest_id, manifest_generation, release_id,
                    release_sequence, build_id, app_version, protocol_version,
                    source_commit, electron_version, playwright_version,
                    chromium_revision, release_notes_url, artifact_count,
                    canonical_manifest_base64url,
                    authorization_signature_set_sha256, published_at_ms,
                    recorded_by, recorded_at_ms
                 ) VALUES (
                    ?1, 'test-manifest', 1, 'test-release', 1, 'browser-1.0',
                    '1.0.0', 1, ?2, '43.1.0', '1.61.1', '1228',
                    'https://bluey.sh/jobs/browser/releases/test-release/RELEASE.md',
                    5, 'dGVzdA', ?3, 1, 'test-suite', 1
                 )",
                params![
                    authority.manifest_sha256,
                    "a".repeat(40),
                    authority.manifest_signature_set_sha256,
                ],
            )
            .unwrap();
        seed_test_additional_browser_release_artifacts(
            &connection,
            &authority.manifest_sha256,
            &authority.signature,
        );
        connection
            .execute(
                "INSERT OR IGNORE INTO jobs_browser_release_artifacts (
                    artifact_id, manifest_sha256, platform, architecture,
                    package_kind, build_descriptor_sha256,
                    build_descriptor_base64url,
                    build_descriptor_signature_base64url,
                    build_descriptor_signing_key_id, artifact_url,
                    artifact_filename, artifact_size_bytes, artifact_sha256,
                    app_content_sha256, verification_evidence_sha256,
                    native_signature_kind, native_signer_identity, recorded_at_ms
                 ) VALUES (
                    'test-artifact', ?1, 'darwin', 'arm64', 'darwin-dmg', ?2,
                    'dGVzdA', ?3, 'test-build-key',
                    'https://bluey.sh/jobs/browser/releases/test-release/Bluey-Browser.dmg',
                    'Bluey-Browser.dmg', 1, ?4, ?5, ?6,
                    'apple-developer-id', 'TESTTEAM', 1
                 )",
                params![
                    authority.manifest_sha256,
                    authority.descriptor_sha256,
                    authority.signature,
                    authority.artifact_sha256,
                    "8".repeat(64),
                    "9".repeat(64),
                ],
            )
            .unwrap();
        connection
            .execute(
                "INSERT OR IGNORE INTO jobs_browser_release_artifact_runtime_components (
                    manifest_sha256, artifact_id, build_descriptor_sha256,
                    artifact_sha256, platform, architecture, package_kind,
                    automation_bundle_sha256, chromium_executable_sha256,
                    recorded_at_ms
                 )
                 SELECT manifest_sha256, artifact_id, build_descriptor_sha256,
                        artifact_sha256, platform, architecture, package_kind,
                        CASE
                          WHEN platform = 'darwin' AND architecture = 'arm64' THEN ?2
                          WHEN platform = 'darwin' AND architecture = 'x64' THEN ?3
                          ELSE ?4
                        END,
                        CASE
                          WHEN platform = 'darwin' AND architecture = 'arm64' THEN ?5
                          WHEN platform = 'darwin' AND architecture = 'x64' THEN ?6
                          ELSE ?7
                        END,
                        1
                   FROM jobs_browser_release_artifacts
                  WHERE manifest_sha256 = ?1",
                params![
                    authority.manifest_sha256,
                    "4".repeat(64),
                    "5".repeat(64),
                    "6".repeat(64),
                    "7".repeat(64),
                    "8".repeat(64),
                    "9".repeat(64),
                ],
            )
            .unwrap();
        connection
            .execute(
                "INSERT OR IGNORE INTO jobs_browser_release_activations (
                    activation_sha256, activation_id, activation_generation,
                    trust_generation, channel, channel_sequence, manifest_sha256,
                    manifest_signature_set_sha256,
                    authorization_signature_set_sha256,
                    accepted_server_release_ids_json, canary_evidence_sha256,
                    canonical_activation_base64url, issued_at_ms, expires_at_ms,
                    recorded_by, recorded_at_ms
                 ) VALUES (
                    ?1, 'test-activation', 1, 1, 'beta', 1, ?2, ?3, ?4,
                    '[\"alternate-test-server\",\"test-server\"]', ?5, 'dGVzdA', 1,
                    9007199254740991, 'test-suite', 1
                 )",
                params![
                    authority.activation_sha256,
                    authority.manifest_sha256,
                    authority.manifest_signature_set_sha256,
                    authority.activation_signature_set_sha256,
                    "c".repeat(64),
                ],
            )
            .unwrap();
        connection
            .execute(
                "INSERT OR IGNORE INTO jobs_browser_release_channel_transitions (
                    transition_sha256, channel, head_revision,
                    previous_head_revision, previous_transition_sha256,
                    previous_activation_sha256, previous_manifest_sha256,
                    previous_trust_generation, previous_channel_sequence,
                    next_activation_sha256, next_manifest_sha256,
                    next_trust_generation, next_channel_sequence,
                    transition_kind, authority_sha256,
                    rollback_authority_sha256, recorded_by, recorded_at_ms
                 ) VALUES (
                    ?1, 'beta', 1, 0, NULL, NULL, NULL, NULL, NULL,
                    ?2, ?3, 1, 1, 'activation', ?2, NULL, 'test-suite', 1
                 )",
                params![
                    authority.transition_sha256,
                    authority.activation_sha256,
                    authority.manifest_sha256,
                ],
            )
            .unwrap();
        connection
            .execute(
                "INSERT OR IGNORE INTO jobs_browser_release_channel_heads (
                    channel, head_revision, current_transition_sha256,
                    current_activation_sha256, current_manifest_sha256,
                    current_trust_generation, current_channel_sequence,
                    updated_at_ms
                 ) VALUES ('beta', 1, ?1, ?2, ?3, 1, 1, 1)",
                params![
                    authority.transition_sha256,
                    authority.activation_sha256,
                    authority.manifest_sha256,
                ],
            )
            .unwrap();
        connection
            .execute(
                "INSERT OR IGNORE INTO jobs_browser_account_channel_assignments (
                    assignment_sha256, account_id, assignment_generation,
                    predecessor_assignment_sha256, predecessor_generation, channel,
                    reason_ref, assigned_by, assigned_at_ms
                 ) VALUES (?1, 'acct-jobs', 1, NULL, 0, 'beta',
                    'test-fixture', 'test-suite', 1)",
                params![authority.assignment_sha256],
            )
            .unwrap();
        authority
    }

    fn rotate_test_browser_release_artifact_origin(pool: &DbPool, artifact_origin: &str) {
        let authority = test_browser_release_authority_ids();
        let (canonical_policy_base64url, _) = test_browser_release_policy_fixture();
        let canonical_policy = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(canonical_policy_base64url)
            .expect("decode test Browser policy");
        let mut policy = parse_canonical_browser_release_trust_policy(&canonical_policy)
            .expect("parse test Browser policy");
        policy.policy_id = "test-policy-origin-2".to_string();
        policy.trust_generation = 2;
        policy.predecessor_policy_sha256 = Some(authority.policy_sha256.clone());
        policy.artifact_origin = artifact_origin.to_string();
        policy.issued_at_ms += 1;
        let canonical = canonical_browser_release_json(&policy).expect("canonical origin policy");
        let policy_sha256 = browser_release_authority_sha256(&canonical);
        let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(canonical);
        let root_threshold = policy
            .roles
            .iter()
            .find(|role| role.role == "root")
            .expect("root role")
            .threshold;
        let release_threshold = policy
            .roles
            .iter()
            .find(|role| role.role == "release")
            .expect("release role")
            .threshold;
        let promotion_threshold = policy
            .roles
            .iter()
            .find(|role| role.role == "promotion")
            .expect("promotion role")
            .threshold;
        let incident_threshold = policy
            .roles
            .iter()
            .find(|role| role.role == "incident")
            .expect("incident role")
            .threshold;
        let connection = pool.get().expect("get test origin rotation connection");
        connection
            .execute(
                "INSERT INTO jobs_browser_release_trust_policies (
                    policy_sha256, policy_id, trust_generation,
                    predecessor_policy_sha256, predecessor_trust_generation,
                    root_threshold, release_threshold, promotion_threshold,
                    incident_threshold, key_count, canonical_policy_base64url,
                    authorization_signature_set_sha256, issued_at_ms,
                    valid_from_ms, expires_at_ms, recorded_by, recorded_at_ms
                 ) VALUES (?1, ?2, 2, ?3, 1, ?4, ?5, ?6, ?7, ?8, ?9, ?10,
                           ?11, ?12, ?13, 'test-suite', ?14)",
                params![
                    policy_sha256,
                    policy.policy_id,
                    authority.policy_sha256,
                    root_threshold,
                    release_threshold,
                    promotion_threshold,
                    incident_threshold,
                    i64::try_from(policy.keys.len()).unwrap(),
                    encoded,
                    authority.policy_signature_set_sha256,
                    policy.issued_at_ms,
                    policy.valid_from_ms,
                    policy.expires_at_ms,
                    now_ms(),
                ],
            )
            .expect("insert current origin policy");
        for key in &policy.keys {
            connection
                .execute(
                    "INSERT INTO jobs_browser_release_trust_keys (
                        policy_sha256, trust_generation, key_id, role,
                        public_key_base64url, state, valid_from_ms, valid_until_ms,
                        minimum_trust_generation, maximum_trust_generation
                     ) VALUES (?1, 2, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                    params![
                        policy_sha256,
                        key.key_id,
                        key.role,
                        key.public_key,
                        key.state,
                        key.valid_from_ms,
                        key.valid_until_ms,
                        key.minimum_trust_generation,
                        key.maximum_trust_generation,
                    ],
                )
                .expect("insert current origin policy key");
        }

        let activation_sha256 = hex::encode(Sha256::digest(b"test-origin-activation-2"));
        let transition_sha256 = hex::encode(Sha256::digest(b"test-origin-transition-2"));
        connection
            .execute(
                "INSERT INTO jobs_browser_release_activations (
                    activation_sha256, activation_id, activation_generation,
                    trust_generation, channel, channel_sequence, manifest_sha256,
                    manifest_signature_set_sha256,
                    authorization_signature_set_sha256,
                    accepted_server_release_ids_json, canary_evidence_sha256,
                    canonical_activation_base64url, issued_at_ms, expires_at_ms,
                    recorded_by, recorded_at_ms
                 ) VALUES (?1, 'test-origin-activation-2', 2, 2, 'beta', 2, ?2,
                           ?3, ?4, '[\"test-server\"]', ?5, 'dGVzdA', ?6,
                           9007199254740991, 'test-suite', ?7)",
                params![
                    activation_sha256,
                    authority.manifest_sha256,
                    authority.manifest_signature_set_sha256,
                    authority.activation_signature_set_sha256,
                    "d".repeat(64),
                    policy.issued_at_ms,
                    now_ms(),
                ],
            )
            .expect("insert current origin activation");
        connection
            .execute(
                "INSERT INTO jobs_browser_release_channel_transitions (
                    transition_sha256, channel, head_revision,
                    previous_head_revision, previous_transition_sha256,
                    previous_activation_sha256, previous_manifest_sha256,
                    previous_trust_generation, previous_channel_sequence,
                    next_activation_sha256, next_manifest_sha256,
                    next_trust_generation, next_channel_sequence,
                    transition_kind, authority_sha256, rollback_authority_sha256,
                    recorded_by, recorded_at_ms
                 ) VALUES (?1, 'beta', 2, 1, ?2, ?3, ?4, 1, 1, ?5, ?4, 2, 2,
                           'activation', ?5, NULL, 'test-suite', ?6)",
                params![
                    transition_sha256,
                    authority.transition_sha256,
                    authority.activation_sha256,
                    authority.manifest_sha256,
                    activation_sha256,
                    now_ms(),
                ],
            )
            .expect("insert current origin transition");
        connection
            .execute(
                "UPDATE jobs_browser_release_channel_heads
                    SET head_revision = 2, current_transition_sha256 = ?1,
                        current_activation_sha256 = ?2, current_trust_generation = 2,
                        current_channel_sequence = 2, updated_at_ms = ?3
                  WHERE channel = 'beta' AND head_revision = 1",
                params![transition_sha256, activation_sha256, now_ms()],
            )
            .expect("advance current origin head");
    }

    fn advance_test_browser_release_channel_head(pool: &DbPool) {
        let authority = test_browser_release_authority_ids();
        let next_activation_sha256 = hex::encode(Sha256::digest(b"test-next-activation"));
        let next_signature_set_sha256 =
            hex::encode(Sha256::digest(b"test-next-activation-signatures"));
        let next_transition_sha256 = hex::encode(Sha256::digest(b"test-next-transition"));
        let connection = pool.get().unwrap();
        connection
            .execute(
                "INSERT INTO jobs_browser_release_signature_sets (
                    signature_set_sha256, signature_set_id, trust_generation,
                    role, target_audience, target_sha256, signed_at_ms,
                    signature_count, canonical_signature_set_base64url,
                    recorded_by, recorded_at_ms
                 ) VALUES (?1, 'test-next-activation-signatures', 1,
                    'promotion', 'bluey-jobs-browser-release-activation-v1',
                    ?2, 2, 1, 'dGVzdA', 'test-suite', 2)",
                params![next_signature_set_sha256, next_activation_sha256],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO jobs_browser_release_signatures (
                    signature_set_sha256, key_id, signature_base64url
                 ) VALUES (?1, 'test-promotion-key', ?2)",
                params![next_signature_set_sha256, authority.signature],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO jobs_browser_release_activations (
                    activation_sha256, activation_id, activation_generation,
                    trust_generation, channel, channel_sequence, manifest_sha256,
                    manifest_signature_set_sha256,
                    authorization_signature_set_sha256,
                    accepted_server_release_ids_json, canary_evidence_sha256,
                    canonical_activation_base64url, issued_at_ms, expires_at_ms,
                    recorded_by, recorded_at_ms
                 ) VALUES (?1, 'test-next-activation', 2, 1, 'beta', 2,
                    ?2, ?3, ?4, '[\"test-server\"]', ?5, 'dGVzdA', 2,
                    9007199254740991, 'test-suite', 2)",
                params![
                    next_activation_sha256,
                    authority.manifest_sha256,
                    authority.manifest_signature_set_sha256,
                    next_signature_set_sha256,
                    "d".repeat(64),
                ],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO jobs_browser_release_channel_transitions (
                    transition_sha256, channel, head_revision,
                    previous_head_revision, previous_transition_sha256,
                    previous_activation_sha256, previous_manifest_sha256,
                    previous_trust_generation, previous_channel_sequence,
                    next_activation_sha256, next_manifest_sha256,
                    next_trust_generation, next_channel_sequence,
                    transition_kind, authority_sha256,
                    rollback_authority_sha256, recorded_by, recorded_at_ms
                 ) VALUES (?1, 'beta', 2, 1, ?2, ?3, ?4, 1, 1,
                    ?5, ?4, 1, 2, 'activation', ?5, NULL, 'test-suite', 2)",
                params![
                    next_transition_sha256,
                    authority.transition_sha256,
                    authority.activation_sha256,
                    authority.manifest_sha256,
                    next_activation_sha256,
                ],
            )
            .unwrap();
        assert_eq!(
            connection
                .execute(
                    "UPDATE jobs_browser_release_channel_heads
                        SET head_revision = 2, current_transition_sha256 = ?1,
                            current_activation_sha256 = ?2,
                            current_manifest_sha256 = ?3,
                            current_trust_generation = 1,
                            current_channel_sequence = 2, updated_at_ms = 2
                      WHERE channel = 'beta' AND head_revision = 1
                        AND current_transition_sha256 = ?4
                        AND current_activation_sha256 = ?5
                        AND current_manifest_sha256 = ?3",
                    params![
                        next_transition_sha256,
                        next_activation_sha256,
                        authority.manifest_sha256,
                        authority.transition_sha256,
                        authority.activation_sha256,
                    ],
                )
                .unwrap(),
            1,
            "test channel-head transition must win its exact compare-and-swap"
        );
    }

    fn advance_test_browser_account_assignment(pool: &DbPool) {
        let authority = test_browser_release_authority_ids();
        let next_assignment_sha256 =
            hex::encode(Sha256::digest(b"test-browser-account-assignment-2"));
        pool.get()
            .unwrap()
            .execute(
                "INSERT INTO jobs_browser_account_channel_assignments (
                    assignment_sha256, account_id, assignment_generation,
                    predecessor_assignment_sha256, predecessor_generation, channel,
                    reason_ref, assigned_by, assigned_at_ms
                 ) VALUES (?1, 'acct-jobs', 2, ?2, 1, 'beta',
                    'test-reassignment', 'test-suite', 2)",
                params![next_assignment_sha256, authority.assignment_sha256],
            )
            .unwrap();
    }

    fn seed_test_local_browser_release_binding(pool: &DbPool, application_id: &str, run_id: &str) {
        let authority = seed_test_browser_release_authority_rows(pool);
        let binding_sha256 = hex::encode(Sha256::digest(
            format!("test-local-release-binding:{run_id}").as_bytes(),
        ));
        pool.get()
            .unwrap()
            .execute(
                "INSERT INTO jobs_local_run_release_bindings (
                    run_id, account_id, application_id, binding_sha256,
                    account_channel_assignment_sha256,
                    account_channel_assignment_generation, channel,
                    channel_head_revision, channel_transition_sha256,
                    activation_sha256, activation_generation, trust_generation,
                    trust_policy_sha256, channel_sequence,
                    manifest_signature_set_sha256,
                    activation_authorization_signature_set_sha256,
                    manifest_sha256, artifact_id, release_id, build_id,
                    app_version, protocol_version, platform, architecture,
                    package_kind, build_descriptor_sha256, artifact_sha256,
                    bound_at_ms
                 ) VALUES (
                    ?1, 'acct-jobs', ?2, ?3, ?4, 1, 'beta', 1, ?5, ?6,
                    1, 1, ?7, 1, ?8, ?9, ?10, 'test-artifact',
                    'test-release', 'browser-1.0', '1.0.0', 1, 'darwin',
                    'arm64', 'darwin-dmg', ?11, ?12, 1
                 )",
                params![
                    run_id,
                    application_id,
                    binding_sha256,
                    authority.assignment_sha256,
                    authority.transition_sha256,
                    authority.activation_sha256,
                    authority.policy_sha256,
                    authority.manifest_signature_set_sha256,
                    authority.activation_signature_set_sha256,
                    authority.manifest_sha256,
                    authority.descriptor_sha256,
                    authority.artifact_sha256,
                ],
            )
            .unwrap();
    }

    fn seed_test_local_browser_release_authority(pool: &DbPool) -> VerifiedBrowserBuildDescriptor {
        seed_test_browser_release_authority_rows(pool);
        test_browser_release_descriptor()
    }

    fn test_browser_release_descriptor() -> VerifiedBrowserBuildDescriptor {
        VerifiedBrowserBuildDescriptor {
            release_id: "test-release".to_string(),
            build_id: "browser-1.0".to_string(),
            app_version: "1.0.0".to_string(),
            app_id: "sh.bluey.jobs.browser".to_string(),
            protocol_version: 1,
            source_commit: "a".repeat(40),
            platform: "darwin".to_string(),
            architecture: "arm64".to_string(),
            electron_version: "43.1.0".to_string(),
            playwright_version: "1.61.1".to_string(),
            chromium_revision: "1228".to_string(),
            issued_at_ms: 1,
            signing_key_id: "test-build-key".to_string(),
            descriptor_base64url: "dGVzdA".to_string(),
            signature_base64url: "A".repeat(86),
            descriptor_sha256: "3".repeat(64),
        }
    }

    #[test]
    fn browser_build_descriptor_rejects_reserved_release_ids_case_insensitively() {
        for release_id in [
            "beta", "CURRENT", "Download", "internal", "LATEST", "Stable",
        ] {
            let descriptor = format!(
                concat!(
                    "version=1\n",
                    "audience=bluey-jobs-browser-build-v1\n",
                    "release_id={}\n",
                    "build_id=browser-1.0\n",
                    "app_version=1.0.0\n",
                    "app_id=sh.bluey.jobs.browser\n",
                    "protocol_version=1\n",
                    "source_commit={}\n",
                    "platform=darwin\n",
                    "architecture=arm64\n",
                    "electron_version=43.1.0\n",
                    "playwright_version=1.61.1\n",
                    "chromium_revision=1228\n",
                    "issued_at_ms=1\n",
                    "signing_key_id=test-build-key\n"
                ),
                release_id,
                "a".repeat(40),
            );
            assert_eq!(
                parse_browser_build_descriptor_bytes(descriptor.as_bytes()),
                Err(BrowserReleaseAuthorityError::InvalidBuildProof),
                "reserved release ID {release_id} must not be mutable authority"
            );
        }

        let oversized_revision = format!(
            concat!(
                "version=1\n",
                "audience=bluey-jobs-browser-build-v1\n",
                "release_id=test-release\n",
                "build_id=browser-1.0\n",
                "app_version=1.0.0\n",
                "app_id=sh.bluey.jobs.browser\n",
                "protocol_version=1\n",
                "source_commit={}\n",
                "platform=darwin\n",
                "architecture=arm64\n",
                "electron_version=43.1.0\n",
                "playwright_version=1.61.1\n",
                "chromium_revision=12345678901234\n",
                "issued_at_ms=1\n",
                "signing_key_id=test-build-key\n"
            ),
            "a".repeat(40),
        );
        assert_eq!(
            parse_browser_build_descriptor_bytes(oversized_revision.as_bytes()),
            Err(BrowserReleaseAuthorityError::InvalidBuildProof),
            "Chromium revisions longer than 13 decimal digits must be rejected"
        );
    }

    fn seed_test_additional_browser_release_artifacts(
        connection: &rusqlite::Connection,
        manifest_sha256: &str,
        signature: &str,
    ) {
        for (
            artifact_id,
            platform,
            architecture,
            package_kind,
            descriptor_sha256,
            file_name,
            artifact_sha256,
            native_signature_kind,
            signer_identity,
        ) in [
            (
                "test-artifact-darwin-arm64-zip",
                "darwin",
                "arm64",
                "darwin-zip",
                "3".repeat(64),
                "Bluey-Browser-arm64.zip",
                "d".repeat(64),
                "apple-developer-id",
                "TESTTEAM",
            ),
            (
                "test-artifact-darwin-x64-dmg",
                "darwin",
                "x64",
                "darwin-dmg",
                "4".repeat(64),
                "Bluey-Browser-x64.dmg",
                "e".repeat(64),
                "apple-developer-id",
                "TESTTEAM",
            ),
            (
                "test-artifact-darwin-x64-zip",
                "darwin",
                "x64",
                "darwin-zip",
                "4".repeat(64),
                "Bluey-Browser-x64.zip",
                "f".repeat(64),
                "apple-developer-id",
                "TESTTEAM",
            ),
            (
                "test-artifact-windows-x64-nsis",
                "windows",
                "x64",
                "windows-nsis",
                "5".repeat(64),
                "Bluey-Browser-x64.exe",
                "0".repeat(64),
                "microsoft-authenticode",
                "TEST PUBLISHER",
            ),
        ] {
            let artifact_url =
                format!("https://bluey.sh/jobs/browser/releases/test-release/{file_name}");
            connection
                .execute(
                    "INSERT OR IGNORE INTO jobs_browser_release_artifacts (
                        artifact_id, manifest_sha256, platform, architecture,
                        package_kind, build_descriptor_sha256,
                        build_descriptor_base64url,
                        build_descriptor_signature_base64url,
                        build_descriptor_signing_key_id, artifact_url,
                        artifact_filename, artifact_size_bytes, artifact_sha256,
                        app_content_sha256, verification_evidence_sha256,
                        native_signature_kind, native_signer_identity, recorded_at_ms
                     ) VALUES (
                        ?1, ?2, ?3, ?4, ?5, ?6, 'dGVzdA', ?7,
                        'test-build-key', ?8, ?9, 1, ?10, ?11, ?12, ?13, ?14, 1
                     )",
                    params![
                        artifact_id,
                        manifest_sha256,
                        platform,
                        architecture,
                        package_kind,
                        descriptor_sha256,
                        signature,
                        artifact_url,
                        file_name,
                        artifact_sha256,
                        "8".repeat(64),
                        "9".repeat(64),
                        native_signature_kind,
                        signer_identity,
                    ],
                )
                .unwrap();
        }
    }

    fn seed_test_browser_release_revocation(
        pool: &DbPool,
        revocation_sha256: &str,
        revocation_id: &str,
        subject_kind: &str,
        subject_id: &str,
        subject_sha256: &str,
    ) {
        let signature_set_sha256 = hex::encode(Sha256::digest(
            format!("test-revocation-signatures:{revocation_id}").as_bytes(),
        ));
        let signature_set_id = format!("{revocation_id}-signatures");
        let connection = pool.get().unwrap();
        connection
            .execute(
                "INSERT INTO jobs_browser_release_signature_sets (
                    signature_set_sha256, signature_set_id, trust_generation,
                    role, target_audience, target_sha256, signed_at_ms,
                    signature_count, canonical_signature_set_base64url,
                    recorded_by, recorded_at_ms
                 ) VALUES (?1, ?2, 1, 'incident',
                    'bluey-jobs-browser-release-revocation-v1', ?3, 2, 1,
                    'dGVzdA', 'test-suite', 2)",
                params![signature_set_sha256, signature_set_id, revocation_sha256],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO jobs_browser_release_signatures (
                    signature_set_sha256, key_id, signature_base64url
                 ) VALUES (?1, 'test-incident-key', ?2)",
                params![signature_set_sha256, "A".repeat(86)],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO jobs_browser_release_revocations (
                    revocation_sha256, revocation_id, revocation_generation,
                    trust_generation, subject_kind, subject_id, subject_sha256,
                    reason_ref, canonical_revocation_base64url,
                    authorization_signature_set_sha256, issued_at_ms,
                    recorded_by, recorded_at_ms
                 ) VALUES (?1, ?2, 1, 1, ?3, ?4, ?5, 'incident-test',
                    'dGVzdA', ?6, 2, 'test-suite', 2)",
                params![
                    revocation_sha256,
                    revocation_id,
                    subject_kind,
                    subject_id,
                    subject_sha256,
                    signature_set_sha256,
                ],
            )
            .unwrap();
    }

    fn local_claim_state(
        pool: &DbPool,
        application_id: &str,
        run_id: &str,
    ) -> (String, String, String, String, i64, i64) {
        pool.get()
            .unwrap()
            .query_row(
                "SELECT ticket.status, application.state, reservation.status,
                        session.status,
                        (SELECT COUNT(*) FROM jobs_run_events event
                          WHERE event.run_id = ticket.id
                            AND event.event_type = 'local_browser_claimed'),
                        (SELECT COUNT(*) FROM jobs_local_run_release_bindings binding
                          WHERE binding.run_id = ticket.id)
                   FROM jobs_local_run_tickets ticket
                   JOIN jobs_applications application
                     ON application.id = ticket.application_id
                   JOIN jobs_attempt_reservations reservation
                     ON reservation.application_id = ticket.application_id
                    AND reservation.account_id = ticket.account_id
                   JOIN jobs_browser_sessions session ON session.id = ticket.id
                  WHERE ticket.id = ?1 AND ticket.application_id = ?2",
                params![run_id, application_id],
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
            .unwrap()
    }

    fn local_click_started_fixture(
        pool: &DbPool,
        suffix: &str,
    ) -> (JobApplication, String, String) {
        let (application, run_id, ticket_hash, _) = local_run_authority_fixture(pool, suffix);
        reserve_application_attempt(pool, "acct-jobs", &application.id, "local").unwrap();
        assert!(
            claim_authorized_local_run_ticket(pool, &run_id, &ticket_hash)
                .unwrap()
                .is_some()
        );
        let application = update_application(pool, "acct-jobs", &application.id, "running", None)
            .unwrap()
            .unwrap();
        upsert_browser_session(
            pool,
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
        let mut capacity = test_submission_evidence_capacity(&application.id, &run_id);
        capacity.runner = "local".to_string();
        assert!(local_run_submit_authorized(
            pool,
            &run_id,
            &ticket_hash,
            &test_final_submit_proof(&application),
            &capacity,
        )
        .unwrap());
        let application = get_application(pool, "acct-jobs", &application.id)
            .unwrap()
            .unwrap();
        (application, run_id, ticket_hash)
    }

    fn answer_intervention_fixture(
        pool: &DbPool,
        application_id: &str,
        suffix: &str,
    ) -> Intervention {
        save_intervention(
            pool,
            "acct-jobs",
            &Intervention {
                id: format!("answer-intervention-{suffix}"),
                application_id: Some(application_id.to_string()),
                kind: "unknown_question".to_string(),
                status: "open".to_string(),
                title: "Are you willing to travel?".to_string(),
                detail: "The employer requires an answer before continuing.".to_string(),
                choices: vec!["Yes".to_string(), "No".to_string()],
                resolution_kind: "answer".to_string(),
                resume_after_resolution: true,
                provider: "greenhouse".to_string(),
                provider_message_id: format!("provider-message-{suffix}"),
                expires_at_ms: None,
                metadata: json!({
                    "receipt": {
                        "intervention": {
                            "field": "Are you willing to travel?"
                        }
                    }
                }),
                created_at_ms: 0,
                resolved_at_ms: None,
            },
        )
        .unwrap()
    }

    fn certified_local_intervention_fixture(
        pool: &DbPool,
        suffix: &str,
    ) -> (JobApplication, String, String, FinalSubmitProof) {
        let browser = browser_release_registry_tests::install_signed_browser_release_fixture(
            pool,
            "acct-jobs",
        );
        let fixture = certified_application_fixture(
            pool,
            suffix,
            "local",
            browser.runtime_target.clone(),
            "Dear Acme, I am excited to apply.",
        );
        let run_id = fixture.run_id;
        let ticket_hash = fixture
            .ticket_hash
            .expect("certified local fixture has a ticket hash");
        let proof = fixture.proof;
        let descriptor = browser.descriptor;
        assert_eq!(proof.documents.len(), 2);
        let claim = claim_local_run_with_browser_release(
            pool,
            &run_id,
            &ticket_hash,
            &"c".repeat(64),
            &descriptor,
            "test-server",
            |ticket, _| Ok(json!({ "runId": ticket.id })),
        )
        .unwrap();
        assert!(matches!(claim, BrowserLocalRunClaimDisposition::Success(_)));
        let application = get_application(pool, "acct-jobs", &fixture.application.id)
            .unwrap()
            .unwrap();
        assert_eq!(application.state, "running");
        (application, run_id, ticket_hash, proof)
    }

    struct CertifiedCloudExecutionFixture {
        application: JobApplication,
        run_id: String,
        lease_token: String,
        lease_fence: i64,
        proof: FinalSubmitProof,
        runtime_target: AtsCertificationRuntimeTarget,
    }

    fn certified_cloud_execution_fixture(
        pool: &DbPool,
        suffix: &str,
    ) -> CertifiedCloudExecutionFixture {
        let owner_id = format!("certified-cloud-owner-{suffix}");
        let runtime = certified_cloud_runtime(suffix);
        let runtime_target = certified_cloud_runtime_target(&runtime);
        let fixture = certified_application_fixture(
            pool,
            suffix,
            "cloud",
            runtime_target.clone(),
            "Dear Acme, I am excited to apply.",
        );
        assert_eq!(fixture.proof.documents.len(), 2);
        let fixture_now = now_ms();
        let installed = super::runner_volume_purge_tests::install_certified_cloud_runtime_fixture(
            pool,
            "acct-jobs",
            &fixture.application.id,
            &fixture.run_id,
            &fixture.browser_profile_id,
            &owner_id,
            runtime,
            fixture_now,
        )
        .expect("install exact certified cloud runtime fixture");
        assert_eq!(installed.runtime_target, runtime_target);
        let grant = claim_execution_lease_for_runner_volume_authorized(
            pool,
            "acct-jobs",
            &fixture.application.id,
            &fixture.run_id,
            &fixture.browser_profile_id,
            &owner_id,
            &installed.binding_request,
            &installed.runtime_grant_id,
            &installed.runtime_sha256,
            &installed.verified_authority,
        )
        .expect("claim exact certified cloud runtime");
        assert_eq!(grant.lease.phase, "prepared");
        assert_eq!(grant.runtime_grant_id, installed.runtime_grant_id);
        assert_eq!(grant.runtime_sha256, installed.runtime_sha256);
        update_attempt_reservation_status(pool, "acct-jobs", &fixture.application.id, "running")
            .unwrap();
        let application =
            update_application(pool, "acct-jobs", &fixture.application.id, "running", None)
                .unwrap()
                .unwrap();
        CertifiedCloudExecutionFixture {
            application,
            run_id: fixture.run_id,
            lease_token: grant.lease.lease_token,
            lease_fence: grant.lease.fence,
            proof: fixture.proof,
            runtime_target,
        }
    }

    struct ManagedCloudAdmissionEnvironment {
        prior: [(&'static str, Option<std::ffi::OsString>); 7],
    }

    impl ManagedCloudAdmissionEnvironment {
        fn staging() -> Self {
            let prior = [
                "BLUEY_JOBS_MANAGED_CLOUD_ENVIRONMENT",
                "BLUEY_JOBS_MANAGED_CLOUD_REGION",
                "BLUEY_JOBS_MANAGED_CLOUD_CHANNEL",
                "BLUEY_JOBS_CLOUD_BROWSER_DISTRIBUTION_ENABLED",
                "BLUEY_JOBS_WORKFLOW_COMMAND_DISPATCH_ENABLED",
                "BLUEY_JOBS_WORKFLOW_ORIGIN",
                "BLUEY_JOBS_WORKFLOW_TOKEN",
            ]
            .map(|name| (name, std::env::var_os(name)));
            std::env::set_var("BLUEY_JOBS_MANAGED_CLOUD_ENVIRONMENT", "staging");
            std::env::set_var("BLUEY_JOBS_MANAGED_CLOUD_REGION", "us-east-1");
            std::env::set_var("BLUEY_JOBS_MANAGED_CLOUD_CHANNEL", "general");
            std::env::set_var("BLUEY_JOBS_CLOUD_BROWSER_DISTRIBUTION_ENABLED", "1");
            std::env::set_var("BLUEY_JOBS_WORKFLOW_COMMAND_DISPATCH_ENABLED", "1");
            std::env::set_var("BLUEY_JOBS_WORKFLOW_ORIGIN", "https://workflow.example.com");
            std::env::set_var("BLUEY_JOBS_WORKFLOW_TOKEN", "x".repeat(32));
            Self { prior }
        }
    }

    impl Drop for ManagedCloudAdmissionEnvironment {
        fn drop(&mut self) {
            for (name, value) in &self.prior {
                match value {
                    Some(value) => std::env::set_var(*name, value),
                    None => std::env::remove_var(*name),
                }
            }
        }
    }

    struct ManagedFinalSubmitExecutionFixture {
        cloud: CertifiedCloudExecutionFixture,
        managed_cloud: ManagedCloudExecutionLeaseClaimInput,
        worker_id: String,
    }

    fn sqlite_test_db_now(pool: &DbPool) -> i64 {
        pool.get()
            .unwrap()
            .query_row(
                "SELECT CAST(unixepoch('subsec') * 1000 AS INTEGER)",
                [],
                |row| row.get(0),
            )
            .unwrap()
    }

    fn managed_final_submit_execution_fixture(
        pool: &DbPool,
        suffix: &str,
    ) -> ManagedFinalSubmitExecutionFixture {
        let owner_id = format!("certified-cloud-owner-{suffix}");
        let runtime = certified_cloud_runtime(suffix);
        let runtime_target = certified_cloud_runtime_target(&runtime);
        let application_fixture = certified_application_fixture(
            pool,
            suffix,
            "cloud",
            runtime_target.clone(),
            "Dear Acme, I am excited to apply.",
        );
        assert_eq!(application_fixture.proof.documents.len(), 2);
        let fixture_now = now_ms();
        let installed = super::runner_volume_purge_tests::install_certified_cloud_runtime_fixture(
            pool,
            "acct-jobs",
            &application_fixture.application.id,
            &application_fixture.run_id,
            &application_fixture.browser_profile_id,
            &owner_id,
            runtime,
            fixture_now,
        )
        .expect("install exact certified cloud runtime fixture");
        assert_eq!(installed.runtime_target, runtime_target);
        let worker_id = installed.binding_request.worker_id.clone();
        let scope = ManagedCloudScope {
            environment: "staging".to_string(),
            region: "us-east-1".to_string(),
            channel: "general".to_string(),
        };
        let managed_runtime = super::managed_cloud_release_authority_tests::
            install_managed_runner_runtime_test_fixture(
                pool,
                &worker_id,
                scope.clone(),
                application_fixture.managed_authority_now_ms,
            );
        let now = sqlite_test_db_now(pool);
        let workflow_id = new_jobs_workflow_id();
        let mut command = NewJobsWorkflowCommand {
            account_id: "acct-jobs".to_string(),
            application_id: application_fixture.application.id.clone(),
            run_id: application_fixture.run_id.clone(),
            workflow_id,
            intervention_id: None,
            command_kind: JobsWorkflowCommandKind::Start,
            idempotency_key: format!("managed-final-submit-{suffix}"),
            request: Value::Null,
            payload: JobsWorkflowCommandPayload::Start(JobsWorkflowStartMaterial {
                workflow_input: json!({ "fixture": suffix }),
                browser_session_id: application_fixture.run_id.clone(),
                result_request_id: format!("managed-final-submit-result-{suffix}"),
            }),
            now_ms: now,
        };
        command.request = stage_workflow_command_request(&command);
        let (admission, binding) = {
            let mut connection = pool.get().unwrap();
            let transaction = connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .unwrap();
            let admission =
                admit_jobs_workflow_command_sqlite_tx(&transaction, &command, true).unwrap();
            let binding = bind_managed_cloud_workflow_sqlite_tx(
                &transaction,
                &managed_cloud_binding_input(&admission, scope.clone()),
            )
            .unwrap();
            transaction.commit().unwrap();
            (admission, binding)
        };
        let command_lease =
            claim_jobs_workflow_command(pool, &worker_id, sqlite_test_db_now(pool), 60_000)
                .unwrap()
                .expect("claim managed workflow command");
        assert_eq!(command_lease.command.id, admission.command.id);
        let (_, request_start) = mark_jobs_workflow_command_request_started_with_managed_cloud(
            pool,
            &command_lease,
            sqlite_test_db_now(pool),
        )
        .expect("mark exact managed workflow request-start");
        assert!(request_start.is_some());
        let (managed_cloud_release, _, _, managed_cloud_release_sha256) =
            managed_cloud_release_memo(&binding.binding_sha256, &binding.admission).unwrap();
        assert_eq!(managed_cloud_release_sha256, binding.release_memo_sha256);
        let managed_cloud = ManagedCloudExecutionLeaseClaimInput {
            workflow_request_id: command_lease.command.request_id,
            managed_cloud_release,
            managed_cloud_release_sha256,
            managed_cloud_runtime_instance_id: managed_runtime.runtime_instance_id,
            managed_cloud_runtime_instance_epoch: managed_runtime.runtime_instance_epoch,
        };
        assert_eq!(managed_runtime.scope, scope);
        let grant = claim_managed_execution_lease_for_runner_volume_authorized(
            pool,
            "acct-jobs",
            &application_fixture.application.id,
            &application_fixture.run_id,
            &application_fixture.browser_profile_id,
            &owner_id,
            &installed.binding_request,
            &installed.runtime_grant_id,
            &installed.runtime_sha256,
            &installed.verified_authority,
            Some(&managed_cloud),
            &worker_id,
            &worker_id,
        )
        .expect("claim exact managed certified cloud runtime");
        assert!(grant.managed_cloud.is_some());
        update_attempt_reservation_status(
            pool,
            "acct-jobs",
            &application_fixture.application.id,
            "running",
        )
        .unwrap();
        let application = update_application(
            pool,
            "acct-jobs",
            &application_fixture.application.id,
            "running",
            None,
        )
        .unwrap()
        .unwrap();
        let cloud = CertifiedCloudExecutionFixture {
            application,
            run_id: application_fixture.run_id,
            lease_token: grant.grant.lease.lease_token,
            lease_fence: grant.grant.lease.fence,
            proof: application_fixture.proof,
            runtime_target,
        };
        assert_eq!(
            heartbeat_execution_lease(
                pool,
                "acct-jobs",
                &cloud.application.id,
                &cloud.run_id,
                &cloud.lease_token,
                cloud.lease_fence,
            )
            .unwrap()
            .phase,
            "prepared"
        );
        ManagedFinalSubmitExecutionFixture {
            cloud,
            managed_cloud,
            worker_id,
        }
    }

    #[derive(Debug, PartialEq, Eq)]
    struct ManagedFinalSubmitBoundaryState {
        lease_phase: String,
        certification_phase: String,
        certification_fence: i64,
        phase_b_request_sha256: Option<String>,
        canary_reservation_count: i64,
        capacity_count: i64,
        managed_receipt_count: i64,
        final_submit_proof_present: bool,
    }

    fn managed_final_submit_boundary_state(
        pool: &DbPool,
        application_id: &str,
        run_id: &str,
    ) -> ManagedFinalSubmitBoundaryState {
        let application = get_application(pool, "acct-jobs", application_id)
            .unwrap()
            .unwrap();
        let mut state = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT lease.phase, binding.phase, binding.fence,
                        binding.phase_b_request_sha256,
                        (SELECT COUNT(*) FROM jobs_ats_certification_canary_reservations
                          WHERE binding_id = binding.binding_id),
                        (SELECT COUNT(*) FROM jobs_submission_evidence_capacity
                          WHERE account_id = lease.account_id
                            AND application_id = lease.application_id
                            AND run_id = lease.run_id),
                        (SELECT COUNT(*)
                           FROM jobs_managed_cloud_irreversible_effect_receipts
                          WHERE account_id = lease.account_id
                            AND application_id = lease.application_id
                            AND run_id = lease.run_id)
                   FROM jobs_execution_leases lease
                   JOIN jobs_application_ats_certification_bindings binding
                     ON binding.account_id = lease.account_id
                    AND binding.application_id = lease.application_id
                    AND binding.run_id = lease.run_id
                  WHERE lease.account_id = 'acct-jobs'
                    AND lease.application_id = ?1 AND lease.run_id = ?2",
                params![application_id, run_id],
                |row| {
                    Ok(ManagedFinalSubmitBoundaryState {
                        lease_phase: row.get(0)?,
                        certification_phase: row.get(1)?,
                        certification_fence: row.get(2)?,
                        phase_b_request_sha256: row.get(3)?,
                        canary_reservation_count: row.get(4)?,
                        capacity_count: row.get(5)?,
                        managed_receipt_count: row.get(6)?,
                        final_submit_proof_present: false,
                    })
                },
            )
            .unwrap();
        state.final_submit_proof_present =
            application.receipt.get(FINAL_SUBMIT_PROOF_KEY).is_some();
        state
    }

    fn put_application_in_answer_intervention(
        pool: &DbPool,
        application: &JobApplication,
        run_id: &str,
        suffix: &str,
    ) -> (JobApplication, Intervention) {
        let application =
            update_application(pool, "acct-jobs", &application.id, "needs_input", None)
                .unwrap()
                .unwrap();
        let mut session = list_browser_sessions(pool, "acct-jobs")
            .unwrap()
            .into_iter()
            .find(|session| session.id == run_id)
            .expect("certified intervention Browser session");
        session.status = "needs_input".to_string();
        session.current_step = "Waiting for an application answer".to_string();
        upsert_browser_session(pool, "acct-jobs", &session).unwrap();
        let intervention = answer_intervention_fixture(pool, &application.id, suffix);
        (application, intervention)
    }

    fn certified_binding_state(
        pool: &DbPool,
        application_id: &str,
        run_id: &str,
    ) -> (String, i64, Option<String>, Option<i64>, Option<i64>) {
        pool.get()
            .unwrap()
            .query_row(
                "SELECT phase, fence, invalidation_kind, invalidated_at_ms, consumed_at_ms
                   FROM jobs_application_ats_certification_bindings
                  WHERE account_id = 'acct-jobs' AND application_id = ?1 AND run_id = ?2",
                params![application_id, run_id],
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
            .unwrap()
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

    fn prepare_archivable_global_candidate(pool: &DbPool, id: &str) -> DiscoveredJobInput {
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
    fn final_submission_requires_one_valid_immutable_receipt() {
        let application_id = "app-final-evidence";
        let resume_version_id = Some("resume-final-evidence".to_string());
        let resume = ApplicationEvidence {
            id: String::new(),
            application_id: application_id.to_string(),
            kind: "resume".to_string(),
            label: "Resume submitted".to_string(),
            provider: "greenhouse".to_string(),
            file_name: "resume.pdf".to_string(),
            media_type: "application/pdf".to_string(),
            storage_key: "request-owned/resume".to_string(),
            sha256: "a".repeat(64),
            resume_version_id: resume_version_id.clone(),
            occurred_at_ms: 0,
            metadata: json!({}),
            created_at_ms: 0,
        };
        let confirmation = ApplicationEvidence {
            id: String::new(),
            application_id: application_id.to_string(),
            kind: "submission_confirmation".to_string(),
            label: "Application received".to_string(),
            provider: "greenhouse".to_string(),
            file_name: "confirmation.png".to_string(),
            media_type: "image/png".to_string(),
            storage_key: "request-owned/confirmation".to_string(),
            sha256: "b".repeat(64),
            resume_version_id: resume_version_id.clone(),
            occurred_at_ms: 0,
            metadata: json!({ "confirmation": "Application received" }),
            created_at_ms: 0,
        };
        let receipt = ApplicationEvidence {
            id: String::new(),
            application_id: application_id.to_string(),
            kind: "application_receipt".to_string(),
            label: "Application receipt bundle".to_string(),
            provider: "greenhouse".to_string(),
            file_name: "receipt-final-evidence.json".to_string(),
            media_type: "application/json".to_string(),
            storage_key: "request-owned/receipt".to_string(),
            sha256: "c".repeat(64),
            resume_version_id,
            occurred_at_ms: 0,
            metadata: json!({
                "receipt_id": "receipt-final-evidence",
                "schema_version": 1,
                "immutable": true,
                "size_bytes": 123
            }),
            created_at_ms: 0,
        };

        let missing_receipt = prepare_submission_evidence(
            application_id,
            &"d".repeat(64),
            &[resume.clone(), confirmation.clone()],
            1,
        )
        .err()
        .expect("submission without an immutable receipt must fail");
        assert!(missing_receipt.to_string().contains(
            "final submission needs one resume, one to four confirmations, and one receipt"
        ));

        let mut mutable_receipt = receipt.clone();
        mutable_receipt.metadata["immutable"] = json!(false);
        let mutable_receipt_error = prepare_submission_evidence(
            application_id,
            &"e".repeat(64),
            &[resume.clone(), confirmation.clone(), mutable_receipt],
            1,
        )
        .err()
        .expect("submission with mutable receipt metadata must fail");
        assert!(mutable_receipt_error
            .to_string()
            .contains("application receipt metadata is incomplete"));

        assert_eq!(
            prepare_submission_evidence(
                application_id,
                &"f".repeat(64),
                &[resume, confirmation, receipt],
                1,
            )
            .unwrap()
            .len(),
            3
        );
    }

    #[test]
    fn final_submission_commits_exact_authority_and_durable_object_bindings() {
        let pool = test_pool();
        let fixture = final_submission_fixture(&pool, "receipt-commit");
        assert_eq!(
            fixture.receipt.pointer(&format!(
                "/{SERVER_SUBMISSION_AUTHORITY_KEY}/preSubmissionReceipt"
            )),
            Some(&fixture.application.receipt)
        );
        assert!(fixture
            .receipt
            .pointer(&format!(
                "/{SERVER_SUBMISSION_AUTHORITY_KEY}/preSubmissionReceipt/approved_execution"
            ))
            .is_some());
        assert!(fixture
            .receipt
            .pointer(&format!(
                "/{SERVER_SUBMISSION_AUTHORITY_KEY}/preSubmissionReceipt/packet_revisions/0"
            ))
            .is_some());
        let expected_pre_submission_receipt_sha256 =
            submission_pre_receipt_sha256(&fixture.application.receipt).unwrap();
        assert_eq!(
            fixture
                .receipt
                .pointer(&format!(
                    "/{SERVER_SUBMISSION_AUTHORITY_KEY}/preSubmissionReceiptSha256"
                ))
                .and_then(Value::as_str),
            Some(expected_pre_submission_receipt_sha256.as_str())
        );

        let mut stale_authority = fixture.receipt.clone();
        stale_authority[SERVER_SUBMISSION_AUTHORITY_KEY]["preSubmissionReceipt"]
            ["approved_execution"]["checksum"] = json!("0".repeat(64));
        let error = validate_submission_authority_snapshot(
            "acct-jobs",
            &fixture.application,
            &fixture.run_id,
            &stale_authority,
            "cloud",
        )
        .unwrap_err();
        assert!(error
            .to_string()
            .contains("submission authority does not match the approved packet"));

        let finalized = finalize_submission(
            &pool,
            "acct-jobs",
            &fixture.application.id,
            &fixture.run_id,
            "cloud",
            fixture.receipt.clone(),
            &fixture.fingerprint,
            &fixture.evidence,
            &fixture.object_uploads,
            &fixture.session,
            None,
        )
        .unwrap();
        let SubmissionFinalizeResult::Committed(application) = finalized else {
            panic!("first finalization must commit");
        };
        assert_eq!(application.state, "submitted");
        assert_eq!(application.receipt, fixture.receipt);
        assert_eq!(
            list_application_evidence(&pool, "acct-jobs", Some(&application.id))
                .unwrap()
                .len(),
            3
        );
        let conn = pool.get().unwrap();
        for binding in &fixture.object_uploads {
            let state: String = conn
                .query_row(
                    "SELECT state FROM object_uploads WHERE id = ?1",
                    params![binding.upload_id],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(state, "ready");
            let outbox_state: String = conn
                .query_row(
                    "SELECT state FROM object_storage_outbox
                      WHERE upload_id = ?1 AND operation = 'put'",
                    params![binding.upload_id],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(outbox_state, "completed");
        }
        let (capacity_state, capacity_expiry, capacity_completed_at_ms): (
            String,
            i64,
            Option<i64>,
        ) = conn
            .query_row(
                "SELECT state, expires_at_ms, completed_at_ms
                   FROM jobs_submission_evidence_capacity
                  WHERE account_id = 'acct-jobs' AND application_id = ?1 AND run_id = ?2",
                params![application.id, fixture.run_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(capacity_state, "committed");
        assert_eq!(
            capacity_expiry,
            SUBMISSION_EVIDENCE_ACCOUNT_LIFETIME_EXPIRES_AT_MS
        );
        assert!(capacity_completed_at_ms.is_some());
    }

    #[test]
    fn fix_728_submitted_domain_requires_an_authentic_final_receipt_envelope() {
        let pool = test_pool();
        let fixture = final_submission_fixture(&pool, "submitted-domain-authority");
        let expected_domain = application_job_integrity_receipt(&fixture.application)
            .unwrap()
            .expect("pre-submission application has signed job-integrity authority")
            .canonical_employer_domain;
        assert!(submitted_execution_employer_domain("acct-jobs", &fixture.application).is_err());

        let finalized = finalize_submission(
            &pool,
            "acct-jobs",
            &fixture.application.id,
            &fixture.run_id,
            "cloud",
            fixture.receipt.clone(),
            &fixture.fingerprint,
            &fixture.evidence,
            &fixture.object_uploads,
            &fixture.session,
            None,
        )
        .unwrap();
        let application = match finalized {
            SubmissionFinalizeResult::Committed(application)
            | SubmissionFinalizeResult::Replayed(application) => application,
        };
        let employer_domain =
            submitted_execution_employer_domain("acct-jobs", &application).unwrap();
        assert_eq!(employer_domain.as_str(), expected_domain);
        assert!(matches!(
            finalize_submission(
                &pool,
                "acct-jobs",
                &fixture.application.id,
                &fixture.run_id,
                "cloud",
                fixture.receipt.clone(),
                &fixture.fingerprint,
                &fixture.evidence,
                &fixture.object_uploads,
                &fixture.session,
                None,
            )
            .unwrap(),
            SubmissionFinalizeResult::Replayed(_)
        ));

        let mut tampered = application.clone();
        tampered.receipt[SERVER_SUBMISSION_AUTHORITY_KEY]["preSubmissionReceipt"]
            ["approved_execution"]["checksum"] = json!("0".repeat(64));
        assert!(submitted_execution_employer_domain("acct-jobs", &tampered).is_err());

        let mut domain_tampered = application.clone();
        domain_tampered.receipt[SERVER_SUBMISSION_AUTHORITY_KEY]["preSubmissionReceipt"]
            ["job_integrity"]["canonicalEmployerDomain"] = json!("lookalike.example");
        assert!(submitted_execution_employer_domain("acct-jobs", &domain_tampered).is_err());
        let domain_tampered_payload =
            to_json(&domain_tampered, "tampered submitted application").unwrap();
        pool.get()
            .unwrap()
            .execute(
                "UPDATE jobs_applications SET application_json = ?2 WHERE id = ?1",
                params![domain_tampered.id, domain_tampered_payload],
            )
            .unwrap();
        let stored_before_replay: String = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT application_json FROM jobs_applications WHERE id = ?1",
                params![fixture.application.id],
                |row| row.get(0),
            )
            .unwrap();
        assert!(finalize_submission(
            &pool,
            "acct-jobs",
            &fixture.application.id,
            &fixture.run_id,
            "cloud",
            fixture.receipt,
            &fixture.fingerprint,
            &fixture.evidence,
            &fixture.object_uploads,
            &fixture.session,
            None,
        )
        .is_err());
        let stored_after_replay: String = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT application_json FROM jobs_applications WHERE id = ?1",
                params![fixture.application.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(stored_after_replay, stored_before_replay);

        let mut wrong_run = application;
        wrong_run.receipt["runId"] = json!("different-submitted-run");
        assert!(submitted_execution_employer_domain("acct-jobs", &wrong_run).is_err());
    }

    #[test]
    fn concurrent_identical_final_submission_calls_commit_once_and_replay_exactly() {
        let pool = test_pool();
        let fixture = final_submission_fixture(&pool, "receipt-concurrent-identical");
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(3));
        let mut workers = Vec::new();

        for _ in 0..2 {
            let pool = pool.clone();
            let application_id = fixture.application.id.clone();
            let run_id = fixture.run_id.clone();
            let receipt = fixture.receipt.clone();
            let fingerprint = fixture.fingerprint.clone();
            let evidence = fixture.evidence.clone();
            let object_uploads = fixture.object_uploads.clone();
            let session = fixture.session.clone();
            let barrier = barrier.clone();
            workers.push(std::thread::spawn(move || {
                barrier.wait();
                finalize_submission(
                    &pool,
                    "acct-jobs",
                    &application_id,
                    &run_id,
                    "cloud",
                    receipt,
                    &fingerprint,
                    &evidence,
                    &object_uploads,
                    &session,
                    None,
                )
            }));
        }

        barrier.wait();
        let results = workers
            .into_iter()
            .map(|worker| worker.join().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            results
                .iter()
                .filter(|result| { matches!(result, Ok(SubmissionFinalizeResult::Committed(_))) })
                .count(),
            1
        );
        assert_eq!(
            results
                .iter()
                .filter(|result| { matches!(result, Ok(SubmissionFinalizeResult::Replayed(_))) })
                .count(),
            1
        );
        for result in results {
            let application = match result.unwrap() {
                SubmissionFinalizeResult::Committed(application)
                | SubmissionFinalizeResult::Replayed(application) => application,
            };
            assert_eq!(application.state, "submitted");
            assert_eq!(application.receipt, fixture.receipt);
        }
        assert_eq!(
            list_application_evidence(&pool, "acct-jobs", Some(&fixture.application.id))
                .unwrap()
                .len(),
            3
        );
    }

    #[test]
    fn concurrent_different_final_submission_payloads_have_one_exact_winner() {
        let pool = test_pool();
        let fixture = final_submission_fixture(&pool, "receipt-concurrent-conflict");
        let conflicting_fingerprint = "e".repeat(64);
        let mut conflicting_receipt = fixture.receipt.clone();
        conflicting_receipt["_bluey_server_submission_fingerprint_v1"] =
            json!(conflicting_fingerprint);
        conflicting_receipt["result"]["confirmationText"] = json!("Your application was submitted");
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(3));
        let mut workers = Vec::new();

        for (receipt, fingerprint) in [
            (fixture.receipt.clone(), fixture.fingerprint.clone()),
            (conflicting_receipt.clone(), conflicting_fingerprint.clone()),
        ] {
            let pool = pool.clone();
            let application_id = fixture.application.id.clone();
            let run_id = fixture.run_id.clone();
            let evidence = fixture.evidence.clone();
            let object_uploads = fixture.object_uploads.clone();
            let session = fixture.session.clone();
            let barrier = barrier.clone();
            workers.push(std::thread::spawn(move || {
                barrier.wait();
                finalize_submission(
                    &pool,
                    "acct-jobs",
                    &application_id,
                    &run_id,
                    "cloud",
                    receipt,
                    &fingerprint,
                    &evidence,
                    &object_uploads,
                    &session,
                    None,
                )
            }));
        }

        barrier.wait();
        let results = workers
            .into_iter()
            .map(|worker| worker.join().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            results
                .iter()
                .filter(|result| { matches!(result, Ok(SubmissionFinalizeResult::Committed(_))) })
                .count(),
            1
        );
        assert_eq!(
            results
                .iter()
                .filter(|result| {
                    result.as_ref().err().is_some_and(|error| {
                        error
                            .to_string()
                            .contains("application already has a different final receipt")
                    })
                })
                .count(),
            1
        );
        let stored = get_application(&pool, "acct-jobs", &fixture.application.id)
            .unwrap()
            .unwrap();
        assert_eq!(stored.state, "submitted");
        assert!(stored.receipt == fixture.receipt || stored.receipt == conflicting_receipt);
        assert_eq!(
            list_application_evidence(&pool, "acct-jobs", Some(&fixture.application.id))
                .unwrap()
                .len(),
            3
        );
    }

    #[test]
    fn final_submit_proof_and_claims_remain_frozen_after_mutable_fact_deletion() {
        let pool = test_pool();
        let fact = upsert_fact(
            &pool,
            "acct-jobs",
            &CareerFact {
                id: "proof-frozen-fact".to_string(),
                category: "achievement".to_string(),
                label: "Reliability improvement".to_string(),
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
        let mut fixture = final_submission_fixture(&pool, "proof-frozen-fact");
        fixture.application.receipt["approved_execution"]["packet"]["verifiedClaimIds"] =
            json!([fact.id.clone()]);
        let approved = &fixture.application.receipt["approved_execution"];
        let checksum = approved_submission_checksum(
            approved["schema_version"].as_i64().unwrap(),
            &approved["packet"],
            &approved["job"],
            approved.get("admission"),
        )
        .unwrap();
        fixture.application.receipt["approved_execution"]["checksum"] = json!(checksum);
        fixture.application = replace_application_receipt(
            &pool,
            "acct-jobs",
            &fixture.application.id,
            fixture.application.receipt.clone(),
        )
        .unwrap()
        .unwrap();
        fixture.receipt["packet"]["verifiedClaimIds"] = json!([fact.id.clone()]);
        fixture.receipt["packet"]["approvedPacketChecksum"] = json!(checksum);
        fixture.receipt[SERVER_SUBMISSION_AUTHORITY_KEY]["preSubmissionReceipt"] =
            fixture.application.receipt.clone();
        assert!(fixture
            .application
            .receipt
            .pointer("/approved_execution/packet/verifiedClaimIds")
            .and_then(Value::as_array)
            .is_some_and(|claims| {
                claims
                    .iter()
                    .any(|claim| claim.as_str() == Some(fact.id.as_str()))
            }));
        let proof = stored_final_submit_proof(&fixture.application).unwrap();

        assert_eq!(
            pool.get()
                .unwrap()
                .execute(
                    "DELETE FROM jobs_facts WHERE account_id = 'acct-jobs' AND id = ?1",
                    params![fact.id],
                )
                .unwrap(),
            1
        );
        let finalized = finalize_submission(
            &pool,
            "acct-jobs",
            &fixture.application.id,
            &fixture.run_id,
            "cloud",
            fixture.receipt,
            &fixture.fingerprint,
            &fixture.evidence,
            &fixture.object_uploads,
            &fixture.session,
            None,
        )
        .unwrap();
        let SubmissionFinalizeResult::Committed(application) = finalized else {
            panic!("frozen receipt must commit after mutable fact deletion");
        };
        assert_eq!(stored_final_submit_proof(&application).unwrap(), proof);
    }

    #[test]
    fn final_submission_revalidates_checksum_packet_and_job_at_the_db_boundary() {
        let pool = test_pool();
        let fixture = final_submission_fixture(&pool, "receipt-approval-binding");
        validate_submission_authority_snapshot(
            "acct-jobs",
            &fixture.application,
            &fixture.run_id,
            &fixture.receipt,
            "cloud",
        )
        .unwrap();

        let mut invalid_application = fixture.application.clone();
        invalid_application.receipt["approved_execution"]["checksum"] = json!("f".repeat(64));
        let mut invalid_checksum_receipt = fixture.receipt.clone();
        invalid_checksum_receipt[SERVER_SUBMISSION_AUTHORITY_KEY]["preSubmissionReceipt"] =
            invalid_application.receipt.clone();
        invalid_checksum_receipt["packet"]["approvedPacketChecksum"] = json!("f".repeat(64));
        let error = validate_submission_authority_snapshot(
            "acct-jobs",
            &invalid_application,
            &fixture.run_id,
            &invalid_checksum_receipt,
            "cloud",
        )
        .unwrap_err();
        assert!(error
            .to_string()
            .contains("approved execution checksum does not match"));

        let mut changed_answers = fixture.receipt.clone();
        changed_answers["packet"]["answers"]["sponsorship_required"] = json!("Yes");
        let error = validate_submission_authority_snapshot(
            "acct-jobs",
            &fixture.application,
            &fixture.run_id,
            &changed_answers,
            "cloud",
        )
        .unwrap_err();
        assert!(error
            .to_string()
            .contains("submission authority does not match the approved packet"));

        let mut changed_claims = fixture.receipt.clone();
        changed_claims["packet"]["verifiedClaimIds"] = json!(["unapproved-claim"]);
        assert!(validate_submission_authority_snapshot(
            "acct-jobs",
            &fixture.application,
            &fixture.run_id,
            &changed_claims,
            "cloud",
        )
        .is_err());

        let mut changed_job = fixture.receipt.clone();
        changed_job["job"]["canonicalUrl"] = json!("https://lookalike.invalid/job");
        assert!(validate_submission_authority_snapshot(
            "acct-jobs",
            &fixture.application,
            &fixture.run_id,
            &changed_job,
            "cloud",
        )
        .is_err());
    }

    #[test]
    fn final_submission_schema_two_certified_envelope_is_closed_and_exact() {
        let pool = test_pool();
        let mut fixture = final_submission_fixture(&pool, "receipt-schema-two-certified");
        fixture.application.submission_mode = "auto_submit".to_string();
        let packet = fixture.application.receipt["approved_execution"]["packet"].clone();
        let job = fixture.application.receipt["approved_execution"]["job"].clone();
        let admission = json!({
            "kind": "track_auto_submit",
            "authorization_id": "schema-two-authorization",
            "career_track_id": "track-default",
            "revision_no": 1,
            "authority_fingerprint": "a".repeat(64),
            "ats_certification": {
                "schema_version": 1,
                "provider": "greenhouse",
                "adapter_version": "2026.07.1-beta.1",
                "manifest_sha256": "1".repeat(64),
                "activation_sha256": "2".repeat(64),
                "activation_generation": 1,
                "target_key_sha256": "3".repeat(64),
                "layout_set_sha256": "4".repeat(64),
                "variant_key": "greenhouse_public",
                "layout_contract_version": 1,
                "surface_sha256": "5".repeat(64),
                "adapter_bundle_sha256": "6".repeat(64),
                "runner_target_sha256s": ["7".repeat(64)],
                "expires_at_ms": now_ms() + 60_000,
            },
        });
        let checksum = approved_submission_checksum(3, &packet, &job, Some(&admission)).unwrap();
        fixture.application.receipt["approved_execution"] = json!({
            "schema_version": 3,
            "approved_at_ms": now_ms(),
            "checksum": checksum.clone(),
            "admission": admission,
            "packet": packet,
            "job": job,
        });
        fixture.receipt["schemaVersion"] = json!(2);
        fixture.receipt["packet"]["approvedPacketChecksum"] = json!(checksum);
        fixture.receipt["packet"]["approvedExecutionSchemaVersion"] = json!(3);
        fixture.receipt["packet"]["approvedExecutionAdmission"] =
            fixture.application.receipt["approved_execution"]["admission"].clone();
        fixture.receipt["atsCertifiedReceiptAuthority"] =
            serde_json::to_value(AtsCertifiedReceiptAuthority {
                schema_version: 1,
                account_id: "acct-jobs".to_string(),
                application_id: fixture.application.id.clone(),
                run_id: fixture.run_id.clone(),
                provider: "greenhouse".to_string(),
                adapter: "greenhouse".to_string(),
                adapter_version: "2026.07.1-beta.1".to_string(),
                manifest_sha256: "1".repeat(64),
                activation_sha256: "2".repeat(64),
                activation_generation: 1,
                target_key_sha256: "3".repeat(64),
                layout_set_sha256: "4".repeat(64),
                layout_observation_sha256: "8".repeat(64),
                observed_surface_sha256: "5".repeat(64),
                adapter_bundle_sha256: "6".repeat(64),
                runner_kind: "cloud".to_string(),
                runner_target_sha256: "7".repeat(64),
                binding_sha256: "9".repeat(64),
                binding_fence: 1,
                binding_consumed_at_ms: now_ms(),
                application_attempt_id: "schema-two-attempt".to_string(),
                phase_b_request_id: "schema-two-phase-b".to_string(),
                rollout_channel: "general".to_string(),
                canary_reservation_sha256: "b".repeat(64),
                metering_reservation_sha256: "c".repeat(64),
            })
            .unwrap();
        fixture.receipt["receiptObject"]["schemaVersion"] = json!(2);
        fixture.receipt[SERVER_SUBMISSION_AUTHORITY_KEY]["preSubmissionReceipt"] =
            fixture.application.receipt.clone();
        fixture.receipt[SERVER_SUBMISSION_AUTHORITY_KEY]["preSubmissionReceiptSha256"] =
            json!(submission_pre_receipt_sha256(&fixture.application.receipt).unwrap());

        validate_submission_authority_snapshot(
            "acct-jobs",
            &fixture.application,
            &fixture.run_id,
            &fixture.receipt,
            "cloud",
        )
        .unwrap();
        let prepared = prepare_submission_evidence(
            &fixture.application.id,
            &fixture.fingerprint,
            &fixture.evidence,
            now_ms(),
        )
        .unwrap();
        validate_submission_receipt_evidence(&fixture.receipt, &prepared).unwrap();

        let mut downgraded = fixture.receipt.clone();
        downgraded["schemaVersion"] = json!(1);
        assert!(validate_submission_authority_snapshot(
            "acct-jobs",
            &fixture.application,
            &fixture.run_id,
            &downgraded,
            "cloud",
        )
        .is_err());

        let mut missing_terminal_authority = fixture.receipt;
        missing_terminal_authority
            .as_object_mut()
            .unwrap()
            .remove("atsCertifiedReceiptAuthority");
        assert!(validate_submission_authority_snapshot(
            "acct-jobs",
            &fixture.application,
            &fixture.run_id,
            &missing_terminal_authority,
            "cloud",
        )
        .is_err());
    }

    #[test]
    fn final_submission_requires_one_to_one_evidence_and_object_bindings() {
        let pool = test_pool();
        let fixture = final_submission_fixture(&pool, "receipt-object-binding");
        let prepared = prepare_submission_evidence(
            &fixture.application.id,
            &fixture.fingerprint,
            &fixture.evidence,
            now_ms(),
        )
        .unwrap();
        validate_submission_receipt_evidence(&fixture.receipt, &prepared).unwrap();
        validate_submission_object_bindings(&fixture.receipt, &prepared, &fixture.object_uploads)
            .unwrap();

        let mut duplicate_key_bindings = fixture.object_uploads.clone();
        duplicate_key_bindings[1].object_key = duplicate_key_bindings[0].object_key.clone();
        duplicate_key_bindings[1].size_bytes = duplicate_key_bindings[0].size_bytes;
        duplicate_key_bindings[1].sha256 = duplicate_key_bindings[0].sha256.clone();
        duplicate_key_bindings[1].content_type = duplicate_key_bindings[0].content_type.clone();
        let error = validate_submission_object_bindings(
            &fixture.receipt,
            &prepared,
            &duplicate_key_bindings,
        )
        .unwrap_err();
        assert!(error
            .to_string()
            .contains("durable object binding is invalid"));

        let mut mismatched_evidence = fixture.evidence.clone();
        mismatched_evidence
            .iter_mut()
            .find(|item| item.kind == "submission_confirmation")
            .unwrap()
            .sha256 = "e".repeat(64);
        let mismatched_prepared = prepare_submission_evidence(
            &fixture.application.id,
            &fixture.fingerprint,
            &mismatched_evidence,
            now_ms(),
        )
        .unwrap();
        let error = validate_submission_object_bindings(
            &fixture.receipt,
            &mismatched_prepared,
            &fixture.object_uploads,
        )
        .unwrap_err();
        assert!(error
            .to_string()
            .contains("evidence does not match its durable object"));

        let mut mismatched_manifest = fixture.receipt.clone();
        mismatched_manifest["evidenceObjects"][0]["sizeBytes"] = json!(85);
        let error = validate_submission_object_bindings(
            &mismatched_manifest,
            &prepared,
            &fixture.object_uploads,
        )
        .unwrap_err();
        assert!(error
            .to_string()
            .contains("durable object binding is invalid"));
    }

    #[test]
    fn final_submission_requires_exact_multi_screenshot_evidence_coverage() {
        let pool = test_pool();
        let mut fixture = final_submission_fixture(&pool, "receipt-multi-screenshot");
        let first_confirmation = fixture
            .evidence
            .iter()
            .position(|item| item.kind == "submission_confirmation")
            .unwrap();
        let first_key = fixture.evidence[first_confirmation].storage_key.clone();
        let second_key =
            "accounts/acct-jobs/jobs/receipt-multi-screenshot/confirmation2.png".to_string();
        let second_sha256 = "e".repeat(64);
        let screenshot_keys = json!([first_key, second_key]);
        fixture.receipt["screenshotKeys"] = screenshot_keys.clone();
        fixture.receipt["evidenceObjects"]
            .as_array_mut()
            .unwrap()
            .push(json!({
                "kind": "screenshot",
                "storageKey": second_key,
                "sha256": second_sha256,
                "mediaType": "image/png",
                "sizeBytes": 85,
            }));
        fixture.receipt["evidenceObjects"]
            .as_array_mut()
            .unwrap()
            .sort_by(|left, right| {
                left["storageKey"]
                    .as_str()
                    .unwrap()
                    .cmp(right["storageKey"].as_str().unwrap())
            });
        fixture.evidence[first_confirmation].file_name =
            "submission-confirmation-1-of-2.png".to_string();
        fixture.evidence[first_confirmation].metadata["screenshot_keys"] = screenshot_keys.clone();
        fixture.evidence[first_confirmation].metadata["screenshot_count"] = json!(2);
        let mut second_confirmation = fixture.evidence[first_confirmation].clone();
        second_confirmation.file_name = "submission-confirmation-2-of-2.png".to_string();
        second_confirmation.storage_key = second_key.clone();
        second_confirmation.sha256 = second_sha256.clone();
        second_confirmation.metadata["screenshot_index"] = json!(2);
        second_confirmation.metadata["size_bytes"] = json!(85);
        fixture.evidence.push(second_confirmation);
        fixture.object_uploads.push(ApplicationObjectBinding {
            upload_id: "multi-screenshot-upload-2".to_string(),
            object_key: second_key,
            size_bytes: 85,
            sha256: second_sha256,
            content_type: "image/png".to_string(),
        });

        let prepared = prepare_submission_evidence(
            &fixture.application.id,
            &fixture.fingerprint,
            &fixture.evidence,
            now_ms(),
        )
        .unwrap();
        validate_submission_receipt_evidence(&fixture.receipt, &prepared).unwrap();
        validate_submission_object_bindings(&fixture.receipt, &prepared, &fixture.object_uploads)
            .unwrap();

        let mut wrong_resume = fixture.evidence.clone();
        wrong_resume.last_mut().unwrap().resume_version_id = Some("another-resume".to_string());
        let prepared_wrong_resume = prepare_submission_evidence(
            &fixture.application.id,
            &fixture.fingerprint,
            &wrong_resume,
            now_ms(),
        )
        .unwrap();
        assert!(validate_submission_evidence_resume_bindings(
            &prepared_wrong_resume,
            fixture.application.resume_version_id.as_deref().unwrap(),
        )
        .unwrap_err()
        .to_string()
        .contains("confirmation does not match"));

        let mut too_many = fixture.evidence.clone();
        too_many.extend([
            fixture.evidence.last().unwrap().clone(),
            fixture.evidence.last().unwrap().clone(),
            fixture.evidence.last().unwrap().clone(),
        ]);
        let too_many_error = prepare_submission_evidence(
            &fixture.application.id,
            &fixture.fingerprint,
            &too_many,
            now_ms(),
        )
        .err()
        .expect("five confirmation records must fail");
        assert!(too_many_error
            .to_string()
            .contains("one to four confirmations"));

        let mut missing = fixture.evidence.clone();
        missing.pop();
        let prepared_missing = prepare_submission_evidence(
            &fixture.application.id,
            &fixture.fingerprint,
            &missing,
            now_ms(),
        )
        .unwrap();
        assert!(
            validate_submission_receipt_evidence(&fixture.receipt, &prepared_missing)
                .unwrap_err()
                .to_string()
                .contains("count is incomplete")
        );

        let mut duplicate_index = fixture.evidence.clone();
        duplicate_index.last_mut().unwrap().metadata["screenshot_index"] = json!(1);
        let prepared_duplicate = prepare_submission_evidence(
            &fixture.application.id,
            &fixture.fingerprint,
            &duplicate_index,
            now_ms(),
        )
        .unwrap();
        assert!(
            validate_submission_receipt_evidence(&fixture.receipt, &prepared_duplicate)
                .unwrap_err()
                .to_string()
                .contains("immutable object")
        );

        let mut duplicate_key_receipt = fixture.receipt.clone();
        duplicate_key_receipt["screenshotKeys"][1] =
            duplicate_key_receipt["screenshotKeys"][0].clone();
        assert!(
            validate_submission_receipt_evidence(&duplicate_key_receipt, &prepared)
                .unwrap_err()
                .to_string()
                .contains("keys are duplicated")
        );

        let mut mismatched_key = fixture.evidence.clone();
        mismatched_key.last_mut().unwrap().storage_key = "unbound-screenshot.png".to_string();
        let prepared_mismatched = prepare_submission_evidence(
            &fixture.application.id,
            &fixture.fingerprint,
            &mismatched_key,
            now_ms(),
        )
        .unwrap();
        assert!(
            validate_submission_receipt_evidence(&fixture.receipt, &prepared_mismatched)
                .unwrap_err()
                .to_string()
                .contains("immutable object")
        );
    }

    #[test]
    fn final_submission_authority_matches_cloud_lease_and_local_ticket_exactly() {
        let pool = test_pool();
        let fixture = final_submission_fixture(&pool, "receipt-execution-authority");
        let authority = execution_receipt_authority(
            &pool,
            "acct-jobs",
            &fixture.application.id,
            &fixture.run_id,
            &fixture.lease_token,
            fixture.lease_fence,
        )
        .unwrap();
        assert_eq!(authority.fence, fixture.lease_fence);
        assert_eq!(authority.phase, "submitted");
        assert_eq!(
            authority.lease_token_sha256,
            execution_lease_token_hash(&fixture.lease_token)
        );

        let mut conn = pool.get().unwrap();
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        validate_submission_execution_authority_sqlite_tx(
            &tx,
            "acct-jobs",
            &fixture.application.id,
            &fixture.run_id,
            "cloud",
            &fixture.receipt,
            None,
            now_ms(),
        )
        .unwrap();
        for (field, value) in [
            ("ownerId", json!("replacement-owner")),
            ("leaseTokenSha256", json!("0".repeat(64))),
            ("fence", json!(fixture.lease_fence + 1)),
            ("phase", json!("side_effect_unknown")),
        ] {
            let mut changed = fixture.receipt.clone();
            changed[SERVER_SUBMISSION_AUTHORITY_KEY]["executionAuthority"][field] = value;
            assert!(validate_submission_execution_authority_sqlite_tx(
                &tx,
                "acct-jobs",
                &fixture.application.id,
                &fixture.run_id,
                "cloud",
                &changed,
                None,
                now_ms(),
            )
            .is_err());
        }
        drop(tx);

        let (application, run_id, ticket_hash, _) =
            local_run_authority_fixture(&pool, "receipt-local-authority");
        claim_local_run_ticket(&pool, &run_id, &ticket_hash)
            .unwrap()
            .unwrap();
        let receipt = json!({
            "_bluey_server_submission_authority_v1": {
                "executionAuthority": {
                    "kind": "local_run_ticket",
                    "ticketHash": ticket_hash,
                    "runId": run_id,
                },
            },
        });
        let mut conn = pool.get().unwrap();
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        validate_submission_execution_authority_sqlite_tx(
            &tx,
            "acct-jobs",
            &application.id,
            &run_id,
            "local",
            &receipt,
            Some(&ticket_hash),
            now_ms(),
        )
        .unwrap();
        assert!(validate_submission_execution_authority_sqlite_tx(
            &tx,
            "acct-jobs",
            &application.id,
            &run_id,
            "local",
            &receipt,
            Some("stale-ticket-hash"),
            now_ms(),
        )
        .is_err());
    }

    #[test]
    fn execution_receipt_authority_rejects_nonterminal_and_stale_leases() {
        let pool = test_pool();
        let (application, run_id, browser_profile_id) =
            execution_lease_fixture(&pool, "receipt-authority-stale");
        let first = claim_execution_lease(
            &pool,
            "acct-jobs",
            &application.id,
            &run_id,
            &browser_profile_id,
            "receipt-owner-one",
        )
        .unwrap();
        assert!(matches!(
            execution_receipt_authority(
                &pool,
                "acct-jobs",
                &application.id,
                &run_id,
                &first.lease_token,
                first.fence,
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
        let rotated = claim_execution_lease(
            &pool,
            "acct-jobs",
            &application.id,
            &run_id,
            &browser_profile_id,
            "receipt-owner-two",
        )
        .unwrap();
        assert!(matches!(
            execution_receipt_authority(
                &pool,
                "acct-jobs",
                &application.id,
                &run_id,
                &first.lease_token,
                first.fence,
            ),
            Err(ExecutionLeaseError::Conflict)
        ));
        let rotated_capacity = test_submission_evidence_capacity(&application.id, &run_id);
        start_irreversible_submission(
            &pool,
            "acct-jobs",
            &application.id,
            &run_id,
            &rotated.lease_token,
            rotated.fence,
            &test_final_submit_proof(&application),
            &rotated_capacity,
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
        let authority = execution_receipt_authority(
            &pool,
            "acct-jobs",
            &application.id,
            &run_id,
            &rotated.lease_token,
            rotated.fence,
        )
        .unwrap();
        assert_eq!(authority.owner_id, "receipt-owner-two");
        assert_eq!(authority.fence, rotated.fence);
        assert_eq!(authority.phase, "submitted");
        assert!(matches!(
            execution_receipt_authority(
                &pool,
                "acct-jobs",
                &application.id,
                &run_id,
                "wrong-token",
                rotated.fence,
            ),
            Err(ExecutionLeaseError::Conflict)
        ));
        assert!(matches!(
            execution_receipt_authority(
                &pool,
                "acct-jobs",
                &application.id,
                &run_id,
                &rotated.lease_token,
                rotated.fence + 1,
            ),
            Err(ExecutionLeaseError::Conflict)
        ));

        let (unknown_application, unknown_run_id, unknown_profile_id) =
            execution_lease_fixture(&pool, "receipt-authority-expired-reconciliation");
        let unknown = claim_execution_lease(
            &pool,
            "acct-jobs",
            &unknown_application.id,
            &unknown_run_id,
            &unknown_profile_id,
            "receipt-unknown-owner",
        )
        .unwrap();
        let unknown_capacity =
            test_submission_evidence_capacity(&unknown_application.id, &unknown_run_id);
        start_irreversible_submission(
            &pool,
            "acct-jobs",
            &unknown_application.id,
            &unknown_run_id,
            &unknown.lease_token,
            unknown.fence,
            &test_final_submit_proof(&unknown_application),
            &unknown_capacity,
        )
        .unwrap();
        finish_execution_lease(
            &pool,
            "acct-jobs",
            &unknown_application.id,
            &unknown_run_id,
            &unknown.lease_token,
            unknown.fence,
            "side_effect_unknown",
        )
        .unwrap();
        execution_receipt_authority(
            &pool,
            "acct-jobs",
            &unknown_application.id,
            &unknown_run_id,
            &unknown.lease_token,
            unknown.fence,
        )
        .unwrap();
        pool.get()
            .unwrap()
            .execute(
                "UPDATE jobs_execution_leases
                    SET finished_at_ms = ?2
                  WHERE run_id = ?1",
                params![
                    unknown_run_id,
                    now_ms() - SUBMISSION_RECONCILIATION_GRACE_MS - 1
                ],
            )
            .unwrap();
        assert!(matches!(
            execution_receipt_authority(
                &pool,
                "acct-jobs",
                &unknown_application.id,
                &unknown_run_id,
                &unknown.lease_token,
                unknown.fence,
            ),
            Err(ExecutionLeaseError::Conflict)
        ));
    }

    #[test]
    fn final_submission_is_fenced_once_account_deletion_begins() {
        let pool = test_pool();
        let fixture = final_submission_fixture(&pool, "receipt-account-delete-fence");
        pool.get()
            .unwrap()
            .execute(
                "INSERT INTO account_deletion_intents (
                    account_id, requested_at_ms, last_checked_at_ms,
                    fresh_upload_cutoff_ms, fresh_in_flight_puts
                 ) VALUES ('acct-jobs', ?1, ?1, ?1, 3)",
                params![now_ms()],
            )
            .unwrap();
        let error = finalize_submission(
            &pool,
            "acct-jobs",
            &fixture.application.id,
            &fixture.run_id,
            "cloud",
            fixture.receipt,
            &fixture.fingerprint,
            &fixture.evidence,
            &fixture.object_uploads,
            &fixture.session,
            None,
        )
        .unwrap_err();
        assert!(error
            .to_string()
            .contains("account deletion has fenced final submission"));
        assert!(
            list_application_evidence(&pool, "acct-jobs", Some(&fixture.application.id))
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn final_submission_transaction_rolls_back_if_the_bound_session_disappears() {
        let pool = test_pool();
        let fixture = final_submission_fixture(&pool, "receipt-rollback");
        pool.get()
            .unwrap()
            .execute(
                "DELETE FROM jobs_browser_sessions WHERE account_id = ?1 AND id = ?2",
                params!["acct-jobs", &fixture.run_id],
            )
            .unwrap();
        let error = finalize_submission(
            &pool,
            "acct-jobs",
            &fixture.application.id,
            &fixture.run_id,
            "cloud",
            fixture.receipt.clone(),
            &fixture.fingerprint,
            &fixture.evidence,
            &fixture.object_uploads,
            &fixture.session,
            None,
        )
        .unwrap_err();
        assert!(
            error.to_string().contains("browser session not found"),
            "unexpected finalization error: {error:#}"
        );
        assert!(
            list_application_evidence(&pool, "acct-jobs", Some(&fixture.application.id))
                .unwrap()
                .is_empty()
        );
        let stored = get_application(&pool, "acct-jobs", &fixture.application.id)
            .unwrap()
            .unwrap();
        assert_eq!(stored.state, "running");
        assert_eq!(stored.receipt, fixture.application.receipt);
        let reservation = list_attempt_reservations(&pool, "acct-jobs")
            .unwrap()
            .into_iter()
            .find(|reservation| reservation.application_id == fixture.application.id)
            .unwrap();
        assert_eq!(reservation.status, "running");
        assert!(list_browser_sessions(&pool, "acct-jobs")
            .unwrap()
            .into_iter()
            .all(|session| session.id != fixture.run_id));
        let conn = pool.get().unwrap();
        for binding in &fixture.object_uploads {
            let state: String = conn
                .query_row(
                    "SELECT state FROM object_uploads WHERE id = ?1",
                    params![binding.upload_id],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(state, "pending");
            let outbox_state: String = conn
                .query_row(
                    "SELECT state FROM object_storage_outbox
                      WHERE upload_id = ?1 AND operation = 'put'",
                    params![binding.upload_id],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(outbox_state, "processing");
        }
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
    fn track_readback_uses_the_relational_activation_authority() {
        let pool = test_pool();
        let track = list_tracks(&pool, "acct-jobs").unwrap().remove(0);
        let mut stale_projection = track.clone();
        stale_projection.active = true;
        let stale_json = to_json(&stale_projection, "stale Career Track").unwrap();
        pool.get()
            .unwrap()
            .execute(
                "UPDATE jobs_tracks SET track_json = ?2, active = 0 WHERE id = ?1",
                params![track.id, stale_json],
            )
            .unwrap();

        let projected = list_tracks(&pool, "acct-jobs").unwrap().remove(0);
        assert!(!projected.active);

        let mut stale_projection = projected;
        stale_projection.active = false;
        let stale_json = to_json(&stale_projection, "stale Career Track").unwrap();
        pool.get()
            .unwrap()
            .execute(
                "UPDATE jobs_tracks SET track_json = ?2, active = 1 WHERE id = ?1",
                params![stale_projection.id, stale_json],
            )
            .unwrap();

        assert!(list_tracks(&pool, "acct-jobs").unwrap().remove(0).active);
    }

    #[test]
    fn track_id_collision_cannot_return_a_cross_account_phantom_save() {
        let pool = test_pool();
        pool.get()
            .unwrap()
            .execute(
                "INSERT INTO accounts (id, email, password_hash, trial_seconds_remaining)
                 VALUES ('acct-other', 'other@example.com', 'hash', 0)",
                [],
            )
            .unwrap();
        let existing = list_tracks(&pool, "acct-jobs").unwrap().remove(0);

        let error = upsert_track(&pool, "acct-other", &existing).unwrap_err();
        assert!(error
            .to_string()
            .contains("already assigned to another account"));
        assert!(list_tracks(&pool, "acct-other").unwrap().is_empty());
        assert_eq!(list_tracks(&pool, "acct-jobs").unwrap().len(), 1);
    }

    #[test]
    fn concurrent_track_creates_cannot_exceed_the_active_plan_limit() {
        let pool = test_pool();
        let template = list_tracks(&pool, "acct-jobs").unwrap().remove(0);
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(3));
        let mut handles = Vec::new();
        for index in 0..2 {
            let pool = pool.clone();
            let barrier = barrier.clone();
            let mut track = template.clone();
            track.id = format!("concurrent-track-{index}");
            track.name = format!("Concurrent track {index}");
            track.created_at_ms = 0;
            track.updated_at_ms = 0;
            handles.push(std::thread::spawn(move || {
                barrier.wait();
                upsert_track_with_limit(&pool, "acct-jobs", &track, 2)
            }));
        }
        barrier.wait();

        let results = handles
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
        let error = results
            .into_iter()
            .find_map(Result::err)
            .expect("one concurrent create must lose the plan-limit race");
        assert_eq!(
            error.downcast_ref::<CareerTrackLimitExceeded>(),
            Some(&CareerTrackLimitExceeded)
        );
        assert_eq!(
            list_tracks(&pool, "acct-jobs")
                .unwrap()
                .into_iter()
                .filter(|track| track.active)
                .count(),
            2
        );
    }

    #[test]
    fn curated_discovery_lease_freezes_track_mutations_until_terminal_result() {
        let pool = test_pool();
        let source = ensure_managed_curated_discovery_source(&pool, "acct-jobs")
            .unwrap()
            .unwrap();
        let lease = lease_due_discovery_source(&pool, "curated-track-freeze-worker")
            .unwrap()
            .unwrap();
        assert_eq!(lease.source.id, source.id);

        let mut existing = list_tracks(&pool, "acct-jobs").unwrap().remove(0);
        existing.name = "Changed during lease".to_string();
        assert!(upsert_track(&pool, "acct-jobs", &existing)
            .unwrap_err()
            .to_string()
            .contains("active discovery lease"));

        let mut added = existing.clone();
        added.id = "track-added-during-lease".to_string();
        added.created_at_ms = 0;
        added.updated_at_ms = 0;
        assert!(upsert_track(&pool, "acct-jobs", &added)
            .unwrap_err()
            .to_string()
            .contains("active discovery lease"));
        assert!(delete_track(&pool, "acct-jobs", "track-default")
            .unwrap_err()
            .to_string()
            .contains("active discovery lease"));

        complete_discovery_run(
            &pool,
            &source.id,
            &lease.lease_token,
            &lease.replay_key,
            lease.scheduled_for_ms,
            &[],
            true,
        )
        .unwrap();
        upsert_track(&pool, "acct-jobs", &existing).unwrap();

        pool.get()
            .unwrap()
            .execute(
                "UPDATE jobs_discovery_sources SET next_run_at_ms = ?2 WHERE id = ?1",
                params![source.id, now_ms() - 1],
            )
            .unwrap();
        let retry = lease_due_discovery_source(&pool, "curated-track-failure-worker")
            .unwrap()
            .unwrap();
        fail_discovery_run(
            &pool,
            &source.id,
            &retry.lease_token,
            &retry.replay_key,
            retry.scheduled_for_ms,
            "timeout",
        )
        .unwrap();
        upsert_track(&pool, "acct-jobs", &added).unwrap();
        assert!(delete_track(&pool, "acct-jobs", &added.id).unwrap());
    }

    #[test]
    fn inactive_held_track_is_not_admitted_or_selected_by_curated_discovery() {
        let pool = test_pool();
        let profile = default_profile("jobs@example.com");
        let preferences = JobPreferences::default();
        let mut inactive = list_tracks(&pool, "acct-jobs").unwrap().remove(0);
        inactive.id = "track-inactive-held".to_string();
        inactive.name = "Inactive held track".to_string();
        inactive.active = false;
        inactive.created_at_ms = 0;
        inactive.updated_at_ms = 0;
        upsert_track(&pool, "acct-jobs", &inactive).unwrap();
        append_operational_hold_for_scope(
            &pool,
            OperationalCapability::Discovery,
            OperationalHoldScopeKind::CareerTrack,
            &inactive.id,
            "inactive-curated-track-hold",
            OperationalHoldTransition::Held,
            0,
            None,
        );

        let candidate = test_posting(
            "https://jobs.lever.co/acme/inactive-held-track",
            now_ms(),
            now_ms(),
        );
        assert!(best_curated_discovery_track(
            &candidate,
            &profile,
            &preferences,
            std::slice::from_ref(&inactive),
        )
        .is_none());

        let source = ensure_managed_curated_discovery_source(&pool, "acct-jobs")
            .unwrap()
            .unwrap();
        let lease = lease_due_discovery_source(&pool, "inactive-held-track-worker")
            .unwrap()
            .unwrap();
        let result = complete_discovery_run(
            &pool,
            &source.id,
            &lease.lease_token,
            &lease.replay_key,
            lease.scheduled_for_ms,
            &[curated_discovered_job(
                "inactive-held-track-lead",
                "https://jobs.lever.co/acme/inactive-held-track",
            )],
            true,
        )
        .unwrap();
        assert_eq!(result.upserted_count, 1);
        assert_eq!(
            list_postings(&pool, "acct-jobs").unwrap()[0].track_id,
            "track-default"
        );
    }

    #[test]
    fn discovery_track_active_projection_mismatch_fails_closed() {
        let pool = test_pool();
        ensure_managed_curated_discovery_source(&pool, "acct-jobs")
            .unwrap()
            .unwrap();
        let track = list_tracks(&pool, "acct-jobs").unwrap().remove(0);
        let track_json = to_json(&track, "active Career Track").unwrap();
        pool.get()
            .unwrap()
            .execute(
                "UPDATE jobs_tracks SET track_json = ?2, active = 0 WHERE id = ?1",
                params![track.id, track_json],
            )
            .unwrap();
        assert!(
            lease_due_discovery_source(&pool, "active-projection-worker")
                .unwrap_err()
                .to_string()
                .contains("projection changed")
        );

        let mut inactive = track;
        inactive.active = false;
        let inactive_json = to_json(&inactive, "inactive Career Track").unwrap();
        pool.get()
            .unwrap()
            .execute(
                "UPDATE jobs_tracks SET track_json = ?2, active = 1 WHERE id = ?1",
                params![inactive.id, inactive_json],
            )
            .unwrap();
        assert!(
            lease_due_discovery_source(&pool, "inactive-projection-worker")
                .unwrap_err()
                .to_string()
                .contains("projection changed")
        );
    }

    #[test]
    fn inactive_bound_track_keeps_its_region_hold_scope() {
        let pool = test_pool();
        let mut track = list_tracks(&pool, "acct-jobs").unwrap().remove(0);
        track.active = false;
        upsert_track(&pool, "acct-jobs", &track).unwrap();
        upsert_discovery_source(
            &pool,
            "acct-jobs",
            &DiscoverySourceInput {
                track_id: track.id,
                provider: "greenhouse".to_string(),
                source_key: "inactive-bound-region".to_string(),
                company: "Acme".to_string(),
                run_interval_ms: DISCOVERY_MIN_INTERVAL_MS,
            },
        )
        .unwrap();
        append_operational_hold_for_scope(
            &pool,
            OperationalCapability::Discovery,
            OperationalHoldScopeKind::Region,
            "new york, ny",
            "inactive-bound-region-hold",
            OperationalHoldTransition::Held,
            0,
            None,
        );

        assert!(
            lease_due_discovery_source(&pool, "inactive-bound-region-worker")
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn curated_posting_without_managed_membership_fails_closed_until_repaired() {
        let pool = test_pool();
        let profile = default_profile("jobs@example.com");
        let preferences = JobPreferences {
            sponsorship: "not_required".to_string(),
            ..JobPreferences::default()
        };
        save_profile(&pool, "acct-jobs", &profile).unwrap();
        save_preferences(&pool, "acct-jobs", &preferences).unwrap();
        let posting = upsert_posting(
            &pool,
            "acct-jobs",
            &test_posting(
                "https://jobs.lever.co/acme/curated-authority-gap",
                now_ms(),
                now_ms(),
            ),
            &profile,
            &preferences,
        )
        .unwrap();
        let (application, _) =
            prepare_application(&pool, "acct-jobs", &posting.id, "factual", "review_first")
                .unwrap();
        let mut curated = posting.clone();
        curated.source = "curated_feed:feed-simplify-new-grad".to_string();
        let curated_json = to_json(&curated, "curated authority-gap posting").unwrap();
        pool.get()
            .unwrap()
            .execute(
                "UPDATE jobs_postings SET posting_json = ?2, source = ?3 WHERE id = ?1",
                params![posting.id, curated_json, curated.source],
            )
            .unwrap();

        {
            let mut conn = pool.get().unwrap();
            let tx = conn.transaction().unwrap();
            for error in [
                operational_hold_context_for_job_sqlite_tx(
                    &tx,
                    "acct-jobs",
                    &posting.id,
                    None,
                    None,
                )
                .unwrap_err(),
                operational_hold_context_for_application_sqlite_tx(
                    &tx,
                    "acct-jobs",
                    &application.id,
                    None,
                    None,
                    None,
                )
                .unwrap_err(),
            ] {
                match error {
                    OperationalHoldError::Storage(source) => assert!(source
                        .to_string()
                        .contains("no managed discovery authority")),
                    other => panic!("unexpected curated authority error: {other:?}"),
                }
            }
            tx.commit().unwrap();
        }

        let source = ensure_managed_curated_discovery_source(&pool, "acct-jobs")
            .unwrap()
            .unwrap();
        let observed_at_ms = now_ms();
        pool.get()
            .unwrap()
            .execute(
                "INSERT INTO jobs_discovery_memberships (
                    source_id, account_id, canonical_key, external_id, job_id, content_hash,
                    first_seen_at_ms, last_seen_at_ms, last_seen_run_id, availability_status
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7, ?8, 'active')",
                params![
                    source.id,
                    "acct-jobs",
                    posting.canonical_key,
                    "curated-authority-gap",
                    posting.id,
                    "a".repeat(64),
                    observed_at_ms,
                    "repaired-authority-gap",
                ],
            )
            .unwrap();

        let mut conn = pool.get().unwrap();
        let tx = conn.transaction().unwrap();
        let job_context =
            operational_hold_context_for_job_sqlite_tx(&tx, "acct-jobs", &posting.id, None, None)
                .unwrap();
        let application_context = operational_hold_context_for_application_sqlite_tx(
            &tx,
            "acct-jobs",
            &application.id,
            None,
            None,
            None,
        )
        .unwrap();
        assert!(job_context
            .scopes
            .get(&OperationalHoldScopeKind::DiscoverySource)
            .is_some_and(|ids| ids.contains(&source.id)));
        assert!(application_context
            .scopes
            .get(&OperationalHoldScopeKind::DiscoverySource)
            .is_some_and(|ids| ids.contains(&source.id)));
        tx.commit().unwrap();
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
                params![run_id, source.id, replay_key, "a".repeat(64), started_at_ms,],
            )
            .unwrap();
        }
        let tx = conn.transaction().unwrap();
        upsert_global_candidate_sqlite(&tx, &source.id, "run-first", &first, first_seen_at_ms)
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
        upsert_global_candidate_sqlite(&tx, &source.id, "run-second", &second, second_seen_at_ms)
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
                params![run_id, source.id, replay_key, "a".repeat(64), started_at_ms,],
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
        upsert_global_candidate_sqlite(&tx, &source.id, "run-after-archive", &rediscovered, 20_000)
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

        let replayed = complete_global_discovery_ingestion(&pool, &source_id, &completion).unwrap();
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
        let leases =
            claim_global_candidate_archive_jobs(&pool, "archive-worker", 100_000, 10, 60_000, 10)
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

        let lease =
            claim_global_candidate_archive_jobs(&pool, "archive-worker", 100_000, 10, 60_000, 1)
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

        let leases =
            claim_global_candidate_archive_jobs(&pool, "archive-worker", 100_000, 10, 60_000, 10)
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

        let leases =
            claim_global_candidate_archive_jobs(&pool, "archive-worker", 100_000, 10, 60_000, 10)
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

        let leases =
            claim_global_candidate_archive_jobs(&pool, "archive-worker", 100_000, 10, 60_000, 10)
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
        let lease =
            claim_global_candidate_archive_jobs(&pool, "archive-worker", 100_000, 10, 60_000, 1)
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
        let verifier_assignments: i64 = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM jobs_original_source_verification_assignments",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            verifier_assignments, 0,
            "feature-off v1 imports must not schedule v2 verifier work"
        );
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
        let manual = verified_test_posting(manual, now_ms());
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
            assert_eq!(
                refreshed.discovery_evidence.provenance, "original_source",
                "{provider}"
            );
            assert_eq!(
                refreshed.discovery_evidence.employer_verification_status, "ats_tenant_verified",
                "{provider}"
            );
            assert_eq!(
                refreshed.discovery_evidence.scam_risk_status, "source_screened",
                "{provider}"
            );
            let active_eligibility =
                evaluate_job_eligibility(&pool, "acct-jobs", &refreshed, true, None).unwrap();
            assert!(active_eligibility.can_prepare, "{provider}");
            assert!(!active_eligibility.can_queue_local, "{provider}");
            assert!(!active_eligibility.can_queue_cloud, "{provider}");
            assert!(
                active_eligibility
                    .review_reasons
                    .iter()
                    .any(|reason| reason.code == "employer_identity_review_required"),
                "{provider}"
            );
            assert!(
                active_eligibility
                    .review_reasons
                    .iter()
                    .any(|reason| reason.code == "job_risk_review_required"),
                "{provider}"
            );
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
    fn terminal_receipt_authority_retains_exact_protected_capacity() {
        let pool = test_pool();
        let (submitted_application, submitted_run, submitted_profile) =
            execution_lease_fixture(&pool, "capacity-submitted");
        let submitted_lease = claim_execution_lease(
            &pool,
            "acct-jobs",
            &submitted_application.id,
            &submitted_run,
            &submitted_profile,
            "capacity-submitted-owner",
        )
        .unwrap();
        let submitted_capacity = test_submission_evidence_capacity_with_object_cap(
            &submitted_application.id,
            &submitted_run,
            1024 * 1024,
        );
        start_irreversible_submission(
            &pool,
            "acct-jobs",
            &submitted_application.id,
            &submitted_run,
            &submitted_lease.lease_token,
            submitted_lease.fence,
            &test_final_submit_proof(&submitted_application),
            &submitted_capacity,
        )
        .unwrap();
        finish_execution_lease(
            &pool,
            "acct-jobs",
            &submitted_application.id,
            &submitted_run,
            &submitted_lease.lease_token,
            submitted_lease.fence,
            "submitted",
        )
        .unwrap();
        let (submitted_state, submitted_expiry): (String, i64) = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT state, expires_at_ms
                   FROM jobs_submission_evidence_capacity
                  WHERE account_id = 'acct-jobs' AND application_id = ?1 AND run_id = ?2",
                params![submitted_application.id, submitted_run],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(submitted_state, "active");
        assert_eq!(
            submitted_expiry,
            SUBMISSION_EVIDENCE_ACCOUNT_LIFETIME_EXPIRES_AT_MS
        );
        execution_receipt_authority(
            &pool,
            "acct-jobs",
            &submitted_application.id,
            &submitted_run,
            &submitted_lease.lease_token,
            submitted_lease.fence,
        )
        .unwrap();

        let (unknown_application, unknown_run, unknown_profile) =
            execution_lease_fixture(&pool, "capacity-side-effect-unknown");
        let unknown_lease = claim_execution_lease(
            &pool,
            "acct-jobs",
            &unknown_application.id,
            &unknown_run,
            &unknown_profile,
            "capacity-unknown-owner",
        )
        .unwrap();
        let unknown_capacity = test_submission_evidence_capacity_with_object_cap(
            &unknown_application.id,
            &unknown_run,
            16 * 1024 * 1024,
        );
        start_irreversible_submission(
            &pool,
            "acct-jobs",
            &unknown_application.id,
            &unknown_run,
            &unknown_lease.lease_token,
            unknown_lease.fence,
            &test_final_submit_proof(&unknown_application),
            &unknown_capacity,
        )
        .unwrap();
        let shortened_expiry = now_ms().saturating_add(1_000);
        pool.get()
            .unwrap()
            .execute(
                "UPDATE jobs_submission_evidence_capacity
                    SET expires_at_ms = ?3
                  WHERE account_id = 'acct-jobs' AND application_id = ?1 AND run_id = ?2",
                params![unknown_application.id, unknown_run, shortened_expiry],
            )
            .unwrap();
        finish_execution_lease(
            &pool,
            "acct-jobs",
            &unknown_application.id,
            &unknown_run,
            &unknown_lease.lease_token,
            unknown_lease.fence,
            "side_effect_unknown",
        )
        .unwrap();
        let (unknown_state, unknown_expiry, finished_at_ms): (String, i64, i64) = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT c.state, c.expires_at_ms, l.finished_at_ms
                   FROM jobs_submission_evidence_capacity c
                   JOIN jobs_execution_leases l
                     ON l.account_id = c.account_id
                    AND l.application_id = c.application_id
                    AND l.run_id = c.run_id
                  WHERE c.account_id = 'acct-jobs'
                    AND c.application_id = ?1 AND c.run_id = ?2",
                params![unknown_application.id, unknown_run],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(unknown_state, "active");
        assert!(unknown_expiry > shortened_expiry);
        assert_eq!(
            unknown_expiry,
            finished_at_ms.saturating_add(SUBMISSION_RECONCILIATION_GRACE_MS)
        );
        execution_receipt_authority(
            &pool,
            "acct-jobs",
            &unknown_application.id,
            &unknown_run,
            &unknown_lease.lease_token,
            unknown_lease.fence,
        )
        .unwrap();
    }

    #[test]
    fn safe_terminal_lease_outcomes_release_unused_protected_capacity() {
        let pool = test_pool();
        for outcome in ["failed", "released"] {
            let suffix = format!("capacity-{outcome}");
            let (application, run_id, browser_profile_id) = execution_lease_fixture(&pool, &suffix);
            let lease = claim_execution_lease(
                &pool,
                "acct-jobs",
                &application.id,
                &run_id,
                &browser_profile_id,
                &format!("capacity-{outcome}-owner"),
            )
            .unwrap();
            let capacity = test_submission_evidence_capacity(&application.id, &run_id);
            crate::db::object_uploads::reserve_submission_evidence_capacity(&pool, &capacity)
                .unwrap();
            finish_execution_lease(
                &pool,
                "acct-jobs",
                &application.id,
                &run_id,
                &lease.lease_token,
                lease.fence,
                outcome,
            )
            .unwrap();
            let (state, completed_at_ms): (String, Option<i64>) = pool
                .get()
                .unwrap()
                .query_row(
                    "SELECT state, completed_at_ms
                       FROM jobs_submission_evidence_capacity
                      WHERE account_id = 'acct-jobs' AND application_id = ?1 AND run_id = ?2",
                    params![application.id, run_id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .unwrap();
            assert_eq!(state, "released");
            assert!(completed_at_ms.is_some());
        }
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
        let attempt_runner: String = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT runner FROM jobs_attempt_reservations
                  WHERE account_id = 'acct-jobs' AND application_id = ?1",
                params![application.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(attempt_runner, "cloud");
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
        let capacity = test_submission_evidence_capacity(&application.id, &run_id);
        let started = start_irreversible_submission(
            &pool,
            "acct-jobs",
            &application.id,
            &run_id,
            &rotated.lease_token,
            rotated.fence,
            &test_final_submit_proof(&application),
            &capacity,
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
    fn execution_lease_requires_an_active_cloud_compatible_attempt() {
        let pool = test_pool();
        let (application, run_id, browser_profile_id) =
            execution_lease_fixture(&pool, "missing-attempt");
        pool.get()
            .unwrap()
            .execute(
                "DELETE FROM jobs_attempt_reservations
                  WHERE account_id = 'acct-jobs' AND application_id = ?1",
                params![application.id],
            )
            .unwrap();
        assert!(matches!(
            claim_execution_lease(
                &pool,
                "acct-jobs",
                &application.id,
                &run_id,
                &browser_profile_id,
                "missing-attempt-worker",
            ),
            Err(ExecutionLeaseError::Conflict)
        ));

        let pool = test_pool();
        let (application, run_id, browser_profile_id) =
            execution_lease_fixture(&pool, "local-attempt");
        pool.get()
            .unwrap()
            .execute(
                "UPDATE jobs_attempt_reservations SET runner = 'local'
                  WHERE account_id = 'acct-jobs' AND application_id = ?1",
                params![application.id],
            )
            .unwrap();
        assert!(matches!(
            claim_execution_lease(
                &pool,
                "acct-jobs",
                &application.id,
                &run_id,
                &browser_profile_id,
                "local-attempt-worker",
            ),
            Err(ExecutionLeaseError::Conflict)
        ));
    }

    #[test]
    fn execution_lease_claim_is_fenced_before_account_child_mutation() {
        let pool = test_pool();
        let (application, run_id, browser_profile_id) =
            execution_lease_fixture(&pool, "account-delete-claim-fence");
        pool.get()
            .unwrap()
            .execute(
                "UPDATE jobs_attempt_reservations SET runner = 'unassigned'
                  WHERE account_id = 'acct-jobs' AND application_id = ?1",
                params![&application.id],
            )
            .unwrap();
        let attempt_runner_before: String = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT runner FROM jobs_attempt_reservations
                  WHERE account_id = 'acct-jobs' AND application_id = ?1",
                params![&application.id],
                |row| row.get(0),
            )
            .unwrap();
        pool.get()
            .unwrap()
            .execute(
                "INSERT INTO account_deletion_intents (
                    account_id, requested_at_ms, last_checked_at_ms,
                    fresh_upload_cutoff_ms, fresh_in_flight_puts
                 ) VALUES ('acct-jobs', ?1, ?1, ?1, 0)",
                params![now_ms()],
            )
            .unwrap();

        let error = claim_execution_lease(
            &pool,
            "acct-jobs",
            &application.id,
            &run_id,
            &browser_profile_id,
            "fenced-worker",
        )
        .unwrap_err();
        assert!(matches!(error, ExecutionLeaseError::Conflict));

        let conn = pool.get().unwrap();
        let lease_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM jobs_execution_leases WHERE run_id = ?1",
                params![run_id],
                |row| row.get(0),
            )
            .unwrap();
        let attempt_runner: String = conn
            .query_row(
                "SELECT runner FROM jobs_attempt_reservations
                  WHERE account_id = 'acct-jobs' AND application_id = ?1",
                params![application.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(lease_count, 0);
        assert_eq!(attempt_runner, attempt_runner_before);
    }

    #[test]
    fn account_deletion_fence_blocks_active_lease_and_profile_operations() {
        fn assert_account_deleting<T>(result: ExecutionLeaseResult<T>) {
            match result {
                Err(ExecutionLeaseError::Storage(error)) => assert!(matches!(
                    error.downcast_ref::<UploadControlError>(),
                    Some(UploadControlError::AccountDeleting)
                )),
                Err(other) => panic!("expected account-deletion storage fence, got {other:?}"),
                Ok(_) => panic!("account-deletion fence unexpectedly allowed the operation"),
            }
        }

        let pool = test_pool();
        let (application, run_id, browser_profile_id) =
            execution_lease_fixture(&pool, "account-delete-active-fence");
        let lease = claim_execution_lease(
            &pool,
            "acct-jobs",
            &application.id,
            &run_id,
            &browser_profile_id,
            "active-fenced-worker",
        )
        .unwrap();
        let lease_before: (String, i64) = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT phase, lease_expires_at_ms
                   FROM jobs_execution_leases
                  WHERE run_id = ?1",
                params![run_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        pool.get()
            .unwrap()
            .execute(
                "INSERT INTO account_deletion_intents (
                    account_id, requested_at_ms, last_checked_at_ms,
                    fresh_upload_cutoff_ms, fresh_in_flight_puts
                 ) VALUES ('acct-jobs', ?1, ?1, ?1, 0)",
                params![now_ms()],
            )
            .unwrap();

        assert_account_deleting(heartbeat_execution_lease(
            &pool,
            "acct-jobs",
            &application.id,
            &run_id,
            &lease.lease_token,
            lease.fence,
        ));
        assert!(matches!(
            start_irreversible_submission(
                &pool,
                "acct-jobs",
                &application.id,
                &run_id,
                &lease.lease_token,
                lease.fence,
                &test_final_submit_proof(&application),
                &test_submission_evidence_capacity(&application.id, &run_id),
            ),
            Err(ExecutionLeaseError::Conflict)
        ));
        let object_key = format!(
            "accounts/acct-jobs/jobs/browser-profiles/{browser_profile_id}/generation/1.enc"
        );
        assert_account_deleting(authorize_browser_profile_snapshot_store(
            &pool,
            "acct-jobs",
            &application.id,
            &run_id,
            &browser_profile_id,
            &lease.lease_token,
            lease.fence,
            0,
            &object_key,
            &"a".repeat(64),
            128,
            1,
        ));
        assert_account_deleting(get_browser_profile_snapshot_for_lease(
            &pool,
            "acct-jobs",
            &application.id,
            &run_id,
            &browser_profile_id,
            &lease.lease_token,
            lease.fence,
        ));

        let lease_after: (String, i64) = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT phase, lease_expires_at_ms
                   FROM jobs_execution_leases
                  WHERE run_id = ?1",
                params![run_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(lease_after, lease_before);
        let evidence_capacity_count: i64 = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM jobs_submission_evidence_capacity
                  WHERE account_id = 'acct-jobs' AND application_id = ?1 AND run_id = ?2",
                params![application.id, run_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(evidence_capacity_count, 0);
    }

    #[test]
    fn safe_worker_checkpoint_releases_all_execution_authority_atomically() {
        let pool = test_pool();
        let (application, run_id, browser_profile_id) =
            execution_lease_fixture(&pool, "safe-checkpoint");
        reserve_application_attempt(&pool, "acct-jobs", &application.id, "cloud").unwrap();
        let lease = claim_execution_lease(
            &pool,
            "acct-jobs",
            &application.id,
            &run_id,
            &browser_profile_id,
            "safe-owner",
        )
        .unwrap();
        let capacity = test_submission_evidence_capacity(&application.id, &run_id);
        crate::db::object_uploads::reserve_submission_evidence_capacity(&pool, &capacity).unwrap();

        for _ in 0..2 {
            let reconciled = reconcile_execution_lease_checkpoint(
                &pool,
                "acct-jobs",
                &application.id,
                &run_id,
                "safe-owner",
                Some(&lease.lease_token),
                lease.fence,
                2,
                "prepared",
            )
            .unwrap();
            assert_eq!(reconciled.phase, "released");
        }
        assert!(matches!(
            reconcile_execution_lease_checkpoint(
                &pool,
                "acct-jobs",
                &application.id,
                &run_id,
                "safe-owner",
                Some(&lease.lease_token),
                lease.fence,
                2,
                "needs_input",
            ),
            Err(ExecutionLeaseError::Conflict)
        ));

        let stored = get_application(&pool, "acct-jobs", &application.id)
            .unwrap()
            .unwrap();
        assert_eq!(stored.state, "failed");
        assert_eq!(
            stored.receipt.pointer("/cloud_recovery/status"),
            Some(&json!("released"))
        );
        let session = list_browser_sessions(&pool, "acct-jobs")
            .unwrap()
            .into_iter()
            .find(|session| session.id == run_id)
            .unwrap();
        assert_eq!(session.status, "failed");
        assert_eq!(
            session.current_step,
            "Browser run stopped before submission"
        );
        let attempt = list_attempt_reservations(&pool, "acct-jobs")
            .unwrap()
            .into_iter()
            .find(|attempt| attempt.application_id == application.id)
            .unwrap();
        assert_eq!(attempt.status, "released");
        let capacity_state: String = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT state FROM jobs_submission_evidence_capacity
                  WHERE account_id = 'acct-jobs' AND application_id = ?1 AND run_id = ?2",
                params![application.id, run_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(capacity_state, "released");
    }

    #[test]
    fn unsafe_worker_checkpoint_never_invents_a_submitted_application() {
        let pool = test_pool();
        let (application, run_id, browser_profile_id) =
            execution_lease_fixture(&pool, "unsafe-checkpoint");
        reserve_application_attempt(&pool, "acct-jobs", &application.id, "cloud").unwrap();
        let lease = claim_execution_lease(
            &pool,
            "acct-jobs",
            &application.id,
            &run_id,
            &browser_profile_id,
            "unsafe-owner",
        )
        .unwrap();
        let capacity = test_submission_evidence_capacity(&application.id, &run_id);
        start_irreversible_submission(
            &pool,
            "acct-jobs",
            &application.id,
            &run_id,
            &lease.lease_token,
            lease.fence,
            &test_final_submit_proof(&application),
            &capacity,
        )
        .unwrap();

        for _ in 0..2 {
            let reconciled = reconcile_execution_lease_checkpoint(
                &pool,
                "acct-jobs",
                &application.id,
                &run_id,
                "unsafe-owner",
                Some(&lease.lease_token),
                lease.fence,
                2,
                "final_submit_started",
            )
            .unwrap();
            assert_eq!(reconciled.phase, "side_effect_unknown");
        }

        let stored = get_application(&pool, "acct-jobs", &application.id)
            .unwrap()
            .unwrap();
        assert_eq!(stored.state, "side_effect_unknown");
        assert_eq!(stored.submitted_at_ms, None);
        assert_eq!(
            stored.receipt.pointer("/cloud_recovery/checkpoint_phase"),
            Some(&json!("final_submit_started"))
        );
        assert!(
            list_application_evidence(&pool, "acct-jobs", Some(&application.id))
                .unwrap()
                .is_empty()
        );
        let session = list_browser_sessions(&pool, "acct-jobs")
            .unwrap()
            .into_iter()
            .find(|session| session.id == run_id)
            .unwrap();
        assert_eq!(session.status, "needs_input");
        assert_eq!(session.takeover_url, None);
        let attempt = list_attempt_reservations(&pool, "acct-jobs")
            .unwrap()
            .into_iter()
            .find(|attempt| attempt.application_id == application.id)
            .unwrap();
        assert_eq!(attempt.status, "side_effect_unknown");
        let (capacity_state, capacity_expiry, finished_at_ms): (String, i64, i64) = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT c.state, c.expires_at_ms, l.finished_at_ms
                   FROM jobs_submission_evidence_capacity c
                   JOIN jobs_execution_leases l
                     ON l.account_id = c.account_id
                    AND l.application_id = c.application_id
                    AND l.run_id = c.run_id
                  WHERE c.account_id = 'acct-jobs'
                    AND c.application_id = ?1 AND c.run_id = ?2",
                params![application.id, run_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(capacity_state, "active");
        assert_eq!(
            capacity_expiry,
            finished_at_ms.saturating_add(SUBMISSION_RECONCILIATION_GRACE_MS)
        );
    }

    #[test]
    fn submitted_worker_lease_without_receipt_still_requires_reconciliation() {
        let pool = test_pool();
        let (application, run_id, browser_profile_id) =
            execution_lease_fixture(&pool, "submitted-no-receipt");
        reserve_application_attempt(&pool, "acct-jobs", &application.id, "cloud").unwrap();
        let lease = claim_execution_lease(
            &pool,
            "acct-jobs",
            &application.id,
            &run_id,
            &browser_profile_id,
            "submitted-owner",
        )
        .unwrap();
        let capacity = test_submission_evidence_capacity(&application.id, &run_id);
        start_irreversible_submission(
            &pool,
            "acct-jobs",
            &application.id,
            &run_id,
            &lease.lease_token,
            lease.fence,
            &test_final_submit_proof(&application),
            &capacity,
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

        let reconciled = reconcile_execution_lease_checkpoint(
            &pool,
            "acct-jobs",
            &application.id,
            &run_id,
            "submitted-owner",
            Some(&lease.lease_token),
            lease.fence,
            2,
            "final_submit_activated",
        )
        .unwrap();
        assert_eq!(reconciled.phase, "side_effect_unknown");
        let stored = get_application(&pool, "acct-jobs", &application.id)
            .unwrap()
            .unwrap();
        assert_eq!(stored.state, "side_effect_unknown");
        assert_eq!(stored.submitted_at_ms, None);
        let (capacity_state, capacity_expiry, finished_at_ms): (String, i64, i64) = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT c.state, c.expires_at_ms, l.finished_at_ms
                   FROM jobs_submission_evidence_capacity c
                   JOIN jobs_execution_leases l
                     ON l.account_id = c.account_id
                    AND l.application_id = c.application_id
                    AND l.run_id = c.run_id
                  WHERE c.account_id = 'acct-jobs'
                    AND c.application_id = ?1 AND c.run_id = ?2",
                params![application.id, run_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(capacity_state, "active");
        assert_ne!(
            capacity_expiry,
            SUBMISSION_EVIDENCE_ACCOUNT_LIFETIME_EXPIRES_AT_MS
        );
        assert_eq!(
            capacity_expiry,
            finished_at_ms.saturating_add(SUBMISSION_RECONCILIATION_GRACE_MS)
        );
    }

    #[test]
    fn checkpoint_reconciliation_rejects_wrong_token_and_v2_without_token() {
        let pool = test_pool();
        let (application, run_id, browser_profile_id) =
            execution_lease_fixture(&pool, "checkpoint-token");
        let lease = claim_execution_lease(
            &pool,
            "acct-jobs",
            &application.id,
            &run_id,
            &browser_profile_id,
            "token-owner",
        )
        .unwrap();

        for token in [Some("wrong-token"), None] {
            assert!(matches!(
                reconcile_execution_lease_checkpoint(
                    &pool,
                    "acct-jobs",
                    &application.id,
                    &run_id,
                    "token-owner",
                    token,
                    lease.fence,
                    2,
                    "prepared",
                ),
                Err(ExecutionLeaseError::InvalidRequest | ExecutionLeaseError::Conflict)
            ));
        }
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
        let final_submit_proof = test_final_submit_proof(&application);
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(3));
        let mut workers = Vec::new();
        for _ in 0..2 {
            let pool = pool.clone();
            let application_id = application.id.clone();
            let run_id = run_id.clone();
            let token = lease.lease_token.clone();
            let fence = lease.fence;
            let final_submit_proof = final_submit_proof.clone();
            let barrier = barrier.clone();
            workers.push(std::thread::spawn(move || {
                barrier.wait();
                let capacity = test_submission_evidence_capacity(&application_id, &run_id);
                start_irreversible_submission(
                    &pool,
                    "acct-jobs",
                    &application_id,
                    &run_id,
                    &token,
                    fence,
                    &final_submit_proof,
                    &capacity,
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
                &final_submit_proof,
                &test_submission_evidence_capacity(&application.id, &run_id),
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
        let current_track = list_tracks(&pool, "acct-jobs").unwrap().remove(0);
        assert_eq!(
            first_application
                .receipt
                .pointer("/career_track_policy_authority"),
            Some(&serde_json::to_value(&current_track.policy.authority).unwrap()),
            "a second fixture must not mutate the first application's policy authority"
        );
        let first_posting = get_posting(&pool, "acct-jobs", &first_application.job_id)
            .unwrap()
            .unwrap();
        let current_profile = get_profile(&pool, "acct-jobs", "jobs@example.com").unwrap();
        let current_facts = list_facts(&pool, "acct-jobs").unwrap();
        let identity_id = first_application
            .receipt
            .pointer("/application_identity/id")
            .and_then(Value::as_str)
            .unwrap();
        let current_identity = get_application_identity(&pool, "acct-jobs", identity_id)
            .unwrap()
            .unwrap();
        let experience =
            role_experience_evidence(&current_profile, Some(&current_track), &first_posting);
        let current_evidence = build_profile_evidence_revision(
            "acct-jobs",
            &current_profile,
            &current_facts,
            &current_track,
            &current_identity,
            &experience,
        )
        .unwrap();
        assert_eq!(
            first_application
                .receipt
                .pointer("/evidence_revision_id")
                .and_then(Value::as_str),
            Some(current_evidence.id.as_str()),
            "a second fixture must not change the first application's evidence revision"
        );
        assert_eq!(
            first_application
                .receipt
                .pointer("/evidence_content_hash")
                .and_then(Value::as_str),
            Some(current_evidence.content_hash.as_str()),
            "a second fixture must not change the first application's evidence content"
        );
        let mut authority_conn = pool.get().unwrap();
        let authority_tx = authority_conn.transaction().unwrap();
        assert!(
            stored_execution_evidence_matches_sqlite(
                &authority_tx,
                "acct-jobs",
                &first_application,
            )
            .unwrap(),
            "the first application must retain its immutable resume/evidence binding"
        );
        assert!(
            current_execution_authorized_sqlite(
                &authority_tx,
                "acct-jobs",
                &first_application,
                ExecutionAuthorityRunner::Cloud,
            )
            .unwrap(),
            "the first application must retain exact execution authority"
        );
        authority_tx.commit().unwrap();
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
            discovery_evidence: JobDiscoveryEvidence::default(),
            eligibility: None,
        };
        let mut b = a.clone();
        b.company = "ACME".to_string();
        b.canonical_url =
            "https://boards.example/jobs/1?ref=feed&utm_source=newsletter".to_string();
        assert_eq!(canonical_job_key(&a), canonical_job_key(&b));
    }

    #[test]
    fn discovery_evidence_fails_closed_until_original_source_is_verified() {
        let mut posting = test_posting(
            "https://boards.greenhouse.io/acme/jobs/unverified",
            now_ms(),
            now_ms(),
        );
        posting.discovery_evidence = JobDiscoveryEvidence::default();
        let unverified = discovery_decision(&posting, true);
        assert!(!unverified.can_prepare);
        assert!(!unverified.can_queue_local);
        assert!(!unverified.can_queue_cloud);
        assert!(unverified
            .review_reasons
            .iter()
            .any(|reason| reason.code == "original_source_unverified"));

        posting.discovery_evidence =
            JobDiscoveryEvidence::external_feed_lead(posting.canonical_key.clone());
        let feed_lead = discovery_decision(&posting, true);
        assert!(!feed_lead.can_prepare);
        assert!(feed_lead
            .review_reasons
            .iter()
            .any(|reason| reason.code == "external_feed_requires_revalidation"));
    }

    #[test]
    fn current_provider_source_evidence_allows_preparation_but_not_queueing() {
        let posting = test_posting(
            "https://boards.greenhouse.io/acme/jobs/current-evidence",
            now_ms(),
            now_ms(),
        );
        let decision = discovery_decision(&posting, true);
        assert!(decision.can_prepare, "{decision:?}");
        assert!(!decision.can_queue_local, "{decision:?}");
        assert!(!decision.can_queue_cloud, "{decision:?}");
        for reason in [
            "employer_identity_review_required",
            "job_risk_review_required",
        ] {
            assert!(
                decision
                    .review_reasons
                    .iter()
                    .any(|candidate| candidate.code == reason),
                "missing {reason}: {decision:#?}"
            );
        }
    }

    #[test]
    fn hosted_ats_snapshot_is_reviewable_but_cannot_queue_unattended() {
        let mut posting = test_posting(
            "https://boards.greenhouse.io/acme/jobs/provider-evidence",
            now_ms(),
            now_ms(),
        );
        posting.discovery_evidence = JobDiscoveryEvidence::provider_verified_original_source(
            posting.canonical_key.clone(),
            "greenhouse:acme".to_string(),
            Some("boards.greenhouse.io".to_string()),
            now_ms(),
            "a".repeat(64),
        );

        let decision = discovery_decision(&posting, true);
        assert!(decision.can_prepare, "{decision:?}");
        assert!(!decision.can_queue_local, "{decision:?}");
        assert!(!decision.can_queue_cloud, "{decision:?}");
        assert!(decision
            .review_reasons
            .iter()
            .any(|reason| reason.code == "employer_identity_review_required"));
        assert!(decision
            .review_reasons
            .iter()
            .any(|reason| reason.code == "job_risk_review_required"));
    }

    #[test]
    fn stale_original_source_evidence_requires_refresh_before_preparation() {
        let posting = test_posting(
            "https://boards.greenhouse.io/acme/jobs/stale-evidence",
            now_ms(),
            now_ms() - 2 * DAY_MS,
        );
        let decision = discovery_decision(&posting, false);
        assert!(!decision.can_prepare, "{decision:?}");
        assert!(!decision.can_queue_local);
        assert!(!decision.can_queue_cloud);
        assert!(decision
            .review_reasons
            .iter()
            .any(|reason| reason.code == "original_source_refresh_required"));
    }

    #[test]
    fn original_source_verdict_vocabulary_fails_closed_consistently() {
        for status in [
            "closed",
            "verified_closed",
            "mismatch",
            "identity_mismatch",
            "materially_changed",
            "source_untrusted",
            "expired",
            "quarantined",
            "redirected_to_unknown",
        ] {
            let mut posting = test_posting(
                "https://boards.greenhouse.io/acme/jobs/rejected-evidence",
                now_ms(),
                now_ms(),
            );
            posting.discovery_evidence.original_source_status = status.to_string();
            let decision = discovery_decision(&posting, false);
            assert!(!decision.can_prepare, "status={status} {decision:?}");
            assert!(!decision.can_queue_local, "status={status} {decision:?}");
            assert!(decision
                .hard_failures
                .iter()
                .any(|reason| reason.code == "original_source_rejected"));
        }
    }

    #[test]
    fn discovery_evidence_is_bound_to_exact_job_and_application_domain() {
        let mut posting = test_posting(
            "https://boards.greenhouse.io/acme/jobs/bound-evidence",
            now_ms(),
            now_ms(),
        );
        posting.discovery_evidence.canonical_job_id = Some("different-job".to_string());
        let canonical_mismatch = discovery_decision(&posting, true);
        assert!(!canonical_mismatch.can_prepare);
        assert!(canonical_mismatch
            .hard_failures
            .iter()
            .any(|reason| reason.code == "canonical_evidence_mismatch"));

        posting.discovery_evidence.canonical_job_id = Some(posting.canonical_key.clone());
        posting.discovery_evidence.application_domain = Some("attacker.example".to_string());
        let domain_mismatch = discovery_decision(&posting, true);
        assert!(!domain_mismatch.can_prepare);
        assert!(domain_mismatch
            .hard_failures
            .iter()
            .any(|reason| reason.code == "application_domain_mismatch"));
    }

    #[test]
    fn scam_and_employer_mismatch_evidence_are_hard_blocks() {
        let mut posting = test_posting(
            "https://boards.greenhouse.io/acme/jobs/risk-evidence",
            now_ms(),
            now_ms(),
        );
        posting.discovery_evidence.scam_risk_status = "blocked".to_string();
        posting.discovery_evidence.scam_signals = vec![DiscoveryScamSignal {
            code: "impersonated_domain".to_string(),
            source: "domain_verifier".to_string(),
        }];
        let scam_block = discovery_decision(&posting, true);
        assert!(!scam_block.can_prepare);
        assert!(scam_block
            .hard_failures
            .iter()
            .any(|reason| reason.code == "scam_risk_blocked"));

        posting.discovery_evidence.scam_risk_status = "clear".to_string();
        posting.discovery_evidence.scam_signals.clear();
        posting.discovery_evidence.employer_verification_status = "mismatch".to_string();
        let employer_block = discovery_decision(&posting, true);
        assert!(!employer_block.can_prepare);
        assert!(employer_block
            .hard_failures
            .iter()
            .any(|reason| reason.code == "employer_identity_mismatch"));
    }

    #[test]
    fn upsert_never_synthesizes_discovery_verification_from_active_status() {
        let pool = test_pool();
        let profile = default_profile("jobs@example.com");
        save_profile(&pool, "acct-jobs", &profile).unwrap();
        let mut posting = test_posting(
            "https://boards.greenhouse.io/acme/jobs/no-synthetic-evidence",
            now_ms(),
            now_ms(),
        );
        posting.discovery_evidence = JobDiscoveryEvidence::default();
        posting.last_verified_at_ms = None;
        let saved = upsert_posting(
            &pool,
            "acct-jobs",
            &posting,
            &profile,
            &JobPreferences::default(),
        )
        .unwrap();
        assert!(saved.last_verified_at_ms.is_none());
        assert_eq!(saved.discovery_evidence.original_source_status, "unknown");
        assert!(!saved.eligibility.unwrap().can_prepare);
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
    fn queueing_rechecks_source_freshness_and_requires_independent_review_authority() {
        let pool = test_pool();
        let (profile, preferences) =
            execution_policy_fixture(&pool, "acct-jobs", "jobs@example.com", "track-default");
        let mut posting = upsert_posting(
            &pool,
            "acct-jobs",
            &test_posting(
                "https://boards.greenhouse.io/acme/jobs/recheck",
                now_ms() - 2 * DAY_MS,
                now_ms(),
            ),
            &profile,
            &preferences,
        )
        .unwrap();
        let (application, _) =
            prepare_application(&pool, "acct-jobs", &posting.id, "factual", "review_first")
                .unwrap();

        let stale_at_ms = now_ms() - 2 * DAY_MS;
        posting.last_verified_at_ms = Some(stale_at_ms);
        posting = verified_test_posting(posting, stale_at_ms);
        posting = upsert_posting(&pool, "acct-jobs", &posting, &profile, &preferences).unwrap();
        let stale_decision =
            evaluate_job_eligibility(&pool, "acct-jobs", &posting, true, Some(&application.id))
                .unwrap();
        assert!(!stale_decision.can_queue_local);
        assert!(!stale_decision.can_queue_cloud);
        assert!(stale_decision.review_reasons.iter().any(|reason| {
            matches!(
                reason.code.as_str(),
                "live_verification_required" | "original_source_refresh_required"
            )
        }));
        update_application(&pool, "acct-jobs", &application.id, "queued", None).unwrap_err();

        posting.last_verified_at_ms = Some(now_ms());
        posting = verified_test_posting(posting, now_ms());
        posting = upsert_posting(&pool, "acct-jobs", &posting, &profile, &preferences).unwrap();
        let current_decision =
            evaluate_job_eligibility(&pool, "acct-jobs", &posting, true, Some(&application.id))
                .unwrap();
        assert!(!current_decision.can_queue_local);
        assert!(!current_decision.can_queue_cloud);
        assert!(current_decision.review_reasons.iter().any(|reason| {
            matches!(
                reason.code.as_str(),
                "employer_identity_review_required" | "job_risk_review_required"
            )
        }));
        update_application(&pool, "acct-jobs", &application.id, "queued", None).unwrap_err();
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
        let (profile, preferences) =
            execution_policy_fixture(&pool, "acct-jobs", "jobs@example.com", "track-default");
        let posting = upsert_posting(
            &pool,
            "acct-jobs",
            &test_posting(
                "https://boards.greenhouse.io/acme/jobs/evidence",
                now_ms() - DAY_MS,
                now_ms(),
            ),
            &profile,
            &preferences,
        )
        .unwrap();
        let (application, resume) =
            prepare_application(&pool, "acct-jobs", &posting.id, "factual", "review_first")
                .unwrap();

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
        assert!(error
            .to_string()
            .contains("only a verified runner receipt can finalize"));
        let unchanged = get_application(&pool, "acct-jobs", &application.id)
            .unwrap()
            .unwrap();
        assert_eq!(unchanged.state, "awaiting_review");
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
            &verified_test_posting(
                JobPosting {
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
                    discovery_evidence: JobDiscoveryEvidence::default(),
                    eligibility: None,
                },
                now_ms(),
            ),
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
        let cloud_job = verified_test_posting(cloud_job, now_ms());
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
        let frontend_job = verified_test_posting(frontend_job, now_ms());
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
        let sde_job = verified_test_posting(sde_job, now_ms());
        let sde_job =
            upsert_posting(&pool, "acct-jobs", &sde_job, &profile, &saved_preferences).unwrap();
        let (sde_application, _) =
            prepare_application(&pool, "acct-jobs", &sde_job.id, "factual", "review_first")
                .unwrap();
        seed_active_reservation_for_quota_eligibility(
            &pool,
            &sde_application,
            &sde_job,
            &saved_preferences,
        );
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
        let data_job = verified_test_posting(data_job, now_ms());
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
            &verified_test_posting(
                JobPosting {
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
                    discovery_evidence: JobDiscoveryEvidence::default(),
                    eligibility: None,
                },
                now_ms(),
            ),
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
            crate::db::jobs_generation::reserve(&pool, "acct-jobs", &posting.id, generation_key)
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
            crate::db::jobs_generation::reserve(&pool, "acct-jobs", &posting.id, generation_key)
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
            &verified_test_posting(
                JobPosting {
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
                    employment_type: "full_time".to_string(),
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
                    discovery_evidence: JobDiscoveryEvidence::default(),
                    eligibility: None,
                },
                now_ms(),
            ),
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
        let cover_letter =
            "Dear Hiring Team,\n\nI built reliable systems.\n\nSincerely,\nCandidate".to_string();
        let (application, resume) = finalize_prepared_application_kit(
            &pool,
            "acct-jobs",
            &prepared,
            content.clone(),
            baseline.diff.clone(),
            cover_letter.clone(),
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
        assert_eq!(application.cover_letter, cover_letter);
        assert_eq!(
            application.receipt.pointer("/cover_letter_status"),
            Some(&json!("included"))
        );
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
    fn submission_capability_requires_exact_provider_hosts() {
        let now = now_ms();
        for url in [
            "https://boards.greenhouse.io/acme/jobs/1",
            "https://job-boards.greenhouse.io/acme/jobs/1",
            "https://jobs.lever.co/acme/1",
            "https://jobs.eu.lever.co/acme/1",
        ] {
            assert_eq!(
                submission_capability(&test_posting(url, now, now)),
                "beta_review"
            );
        }

        for url in [
            "https://acme.wd5.myworkdayjobs.com/en-US/jobs/job/1",
            "https://jobs.ashbyhq.com/acme/1",
            "https://jobs.smartrecruiters.com/Acme/1",
            "https://jobs.acme.example/1",
            "https://boards.greenhouse.io/acme",
            "https://boards.greenhouse.io:443/acme/jobs/1",
            "https://jobs.lever.co/acme/1/confirmation",
            "https://boards.greenhouse.io.attacker.example/jobs/1",
            "https://evil.example/jobs?next=https://jobs.lever.co/acme/1",
            "https://notindeed.com/viewjob/1",
        ] {
            assert_eq!(
                submission_capability(&test_posting(url, now, now)),
                "unknown_review"
            );
        }

        for url in [
            "https://www.linkedin.com/jobs/view/1",
            "https://subdomain.indeed.com/viewjob/1",
        ] {
            assert_eq!(
                submission_capability(&test_posting(url, now, now)),
                "handoff"
            );
        }

        for url in ["file:///etc/passwd", "not a URL"] {
            assert_eq!(
                submission_capability(&test_posting(url, now, now)),
                "blocked"
            );
        }
    }

    #[test]
    fn review_only_eligibility_exposes_bounded_fail_closed_ats_truth() {
        let now = now_ms();
        let posting = test_posting("https://jobs.eu.lever.co/acme/posting-1", now, now);
        let decision = discovery_decision(&posting, false);

        assert_eq!(decision.capability, "beta_review");
        assert_eq!(decision.ats_certification.provider_label, "Lever");
        assert_eq!(
            decision.ats_certification.adapter_version.as_deref(),
            Some("2026.07.0-beta.1")
        );
        assert_eq!(decision.ats_certification.status, "review_only");
        assert!(decision.ats_certification.certified_runner_kinds.is_empty());
        assert!(!decision.ats_certification.canary_available);
        assert!(decision.ats_certification.last_verified_at_ms.is_none());
        assert!(decision.ats_certification.expires_at_ms.is_none());
    }

    #[test]
    fn active_ats_status_cannot_elevate_an_unreviewed_track_or_uncertified_runner() {
        let now = now_ms();
        let posting = test_posting("https://boards.greenhouse.io/acme/jobs/posting-1", now, now);
        let profile = default_profile("jobs@example.com");
        let preferences = JobPreferences {
            sponsorship: "not_required".to_string(),
            ..JobPreferences::default()
        };
        let track = CareerTrack {
            id: "track-default".to_string(),
            name: "Software engineering".to_string(),
            role: "Software Engineer".to_string(),
            locations: vec!["New York, NY".to_string()],
            remote_preference: "hybrid_ok".to_string(),
            application_identity_id: Some("identity-primary".to_string()),
            policy: CareerTrackPolicy {
                role_family: "software_engineering".to_string(),
                ..CareerTrackPolicy::default()
            },
            active: true,
            match_count: 0,
            created_at_ms: 0,
            updated_at_ms: 0,
        };
        let baseline = build_job_eligibility(
            &posting,
            &profile,
            &preferences,
            &[],
            false,
            None,
            Some(&track),
        );
        assert!(!baseline.can_auto_submit);
        assert!(baseline
            .review_reasons
            .iter()
            .any(|reason| reason.code == "ats_review_required"));

        let status = AtsCertificationTargetStatusProjection {
            schema_version: 1,
            provider: "greenhouse".to_string(),
            target_key_sha256: "a".repeat(64),
            status: "active".to_string(),
            adapter_version: Some("2026.07.1-beta.1".to_string()),
            manifest_sha256: Some("b".repeat(64)),
            activation_sha256: Some("c".repeat(64)),
            activation_generation: Some(1),
            layout_set_sha256: Some("d".repeat(64)),
            rollout_channel: Some("general".to_string()),
            runner_kinds: vec!["local".to_string()],
            runner_target_sha256s: vec!["e".repeat(64)],
            expires_at_ms: Some(now + 60_000),
            last_verified_at_ms: Some(now - 1_000),
            canary_available: false,
        };

        let mut certified = baseline.clone();
        apply_ats_certification_status(&posting, &mut certified, &status, true);
        assert_eq!(certified.capability, "certified");
        assert!(!certified.can_auto_submit);
        assert!(!certified.can_queue_local);
        assert!(!certified.can_queue_cloud);
        assert!(!certified
            .review_reasons
            .iter()
            .any(|reason| reason.code == "ats_review_required"));
        assert!(certified
            .review_reasons
            .iter()
            .any(|reason| reason.code == "career_track_policy_review_required"));
        assert_eq!(
            certified.ats_certification.certified_runner_kinds,
            vec!["local"]
        );

        let mut missing_binding = baseline;
        apply_ats_certification_status(&posting, &mut missing_binding, &status, false);
        assert_eq!(missing_binding.capability, "beta_review");
        assert!(!missing_binding.can_auto_submit);
        assert_eq!(missing_binding.ats_certification.status, "drifted");
        assert!(missing_binding
            .ats_certification
            .certified_runner_kinds
            .is_empty());
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
        assert!(!decision.can_queue_local);
        assert!(!decision.can_queue_cloud);
        assert!(decision
            .review_reasons
            .iter()
            .any(|reason| reason.code == "engagement_type_unverified"));
        assert!(decision
            .review_reasons
            .iter()
            .any(|reason| reason.code == "track_engagement_type_unverified"));
        assert!(!decision
            .passed_checks
            .iter()
            .any(|check| check == "track_engagement_type_allowed"));
    }

    #[test]
    fn typed_job_categories_preserve_positive_controls_without_minting_queue_authority() {
        let pool = test_pool();
        let (profile, _) =
            execution_policy_fixture(&pool, "acct-jobs", "jobs@example.com", "track-default");
        let preferences = JobPreferences {
            desired_locations: vec!["New York, NY".to_string()],
            employment_types: vec!["full_time".to_string()],
            engagement_types: vec!["w2".to_string()],
            sponsorship: "not_required".to_string(),
            ..JobPreferences::default()
        };
        save_preferences(&pool, "acct-jobs", &preferences).unwrap();
        let mut track = list_tracks(&pool, "acct-jobs").unwrap().remove(0);
        track.policy.employment_types = vec!["full_time".to_string()];
        track.policy.engagement_types = vec!["w2".to_string()];
        let track = upsert_track(&pool, "acct-jobs", &track).unwrap();
        assert_eq!(track.policy.authority.review_state, "approved");

        let mut exact = test_posting(
            "https://boards.greenhouse.io/acme/jobs/exact-full-time-w2",
            now_ms(),
            now_ms(),
        );
        exact.employment_type = "full_time w2".to_string();
        let exact = upsert_posting(&pool, "acct-jobs", &exact, &profile, &preferences).unwrap();
        let exact_decision =
            evaluate_job_eligibility(&pool, "acct-jobs", &exact, true, None).unwrap();
        assert!(exact_decision.can_prepare, "{exact_decision:#?}");
        assert!(!exact_decision.can_queue_local, "{exact_decision:#?}");
        assert!(!exact_decision.can_queue_cloud, "{exact_decision:#?}");
        for check in [
            "employment_type_allowed",
            "engagement_type_allowed",
            "track_employment_type_allowed",
            "track_engagement_type_allowed",
        ] {
            assert!(
                exact_decision
                    .passed_checks
                    .iter()
                    .any(|value| value == check),
                "missing {check}: {exact_decision:#?}"
            );
        }

        let mut unknown_employment = test_posting(
            "https://boards.greenhouse.io/acme/jobs/unknown-employment-w2",
            now_ms(),
            now_ms(),
        );
        unknown_employment.employment_type = "w2".to_string();
        let unknown_employment = upsert_posting(
            &pool,
            "acct-jobs",
            &unknown_employment,
            &profile,
            &preferences,
        )
        .unwrap();
        let unknown_decision =
            evaluate_job_eligibility(&pool, "acct-jobs", &unknown_employment, true, None).unwrap();
        assert!(unknown_decision.can_prepare, "{unknown_decision:#?}");
        assert!(!unknown_decision.can_queue_local);
        assert!(!unknown_decision.can_queue_cloud);
        for code in [
            "employment_type_unverified",
            "track_employment_type_unverified",
        ] {
            assert!(
                unknown_decision
                    .review_reasons
                    .iter()
                    .any(|reason| reason.code == code),
                "missing {code}: {unknown_decision:#?}"
            );
        }
        assert!(!unknown_decision
            .passed_checks
            .iter()
            .any(|check| check == "track_employment_type_allowed"));

        let mut negated_internship = test_posting(
            "https://boards.greenhouse.io/acme/jobs/internship-not-offered",
            now_ms(),
            now_ms(),
        );
        negated_internship.employment_type = "internship".to_string();
        negated_internship.description =
            "Internship is not offered. Build reliable products.".to_string();
        let negated_internship = upsert_posting(
            &pool,
            "acct-jobs",
            &negated_internship,
            &profile,
            &preferences,
        )
        .unwrap();
        let negated_decision =
            evaluate_job_eligibility(&pool, "acct-jobs", &negated_internship, true, None).unwrap();
        assert!(!negated_decision.can_queue_local);
        assert!(!negated_decision.can_queue_cloud);
        for code in [
            "employment_type_unverified",
            "track_employment_type_unverified",
        ] {
            assert!(
                negated_decision
                    .review_reasons
                    .iter()
                    .any(|reason| reason.code == code),
                "missing {code}: {negated_decision:#?}"
            );
        }
    }

    #[test]
    fn negated_c2c_evidence_never_satisfies_c2c_only_track() {
        let pool = test_pool();
        let profile = default_profile("jobs@example.com");
        save_profile(&pool, "acct-jobs", &profile).unwrap();
        let preferences = JobPreferences {
            desired_locations: vec!["New York, NY".to_string()],
            employment_types: vec!["contract".to_string()],
            engagement_types: vec!["c2c".to_string()],
            sponsorship: "not_required".to_string(),
            ..JobPreferences::default()
        };
        save_preferences(&pool, "acct-jobs", &preferences).unwrap();
        let mut track = list_tracks(&pool, "acct-jobs").unwrap().remove(0);
        track.policy.employment_types = vec!["contract".to_string()];
        track.policy.engagement_types = vec!["c2c".to_string()];
        upsert_track(&pool, "acct-jobs", &track).unwrap();

        for (suffix, description) in [
            ("no-c2c-w2-only", "No C2C; W2 only."),
            ("c2c-do-not-accept", "We do not accept C2C."),
            ("c2c-not-supported", "C2C is not supported."),
        ] {
            let mut posting = test_posting(
                &format!("https://boards.greenhouse.io/acme/jobs/{suffix}"),
                now_ms(),
                now_ms(),
            );
            posting.employment_type = "contract c2c".to_string();
            posting.description = format!("{description} Build reliable products.");
            let posting =
                upsert_posting(&pool, "acct-jobs", &posting, &profile, &preferences).unwrap();
            let decision =
                evaluate_job_eligibility(&pool, "acct-jobs", &posting, true, None).unwrap();
            assert!(decision.can_prepare, "{decision:#?}");
            assert!(!decision.can_queue_local);
            assert!(!decision.can_queue_cloud);
            for code in [
                "engagement_type_unverified",
                "track_engagement_type_unverified",
            ] {
                assert!(
                    decision
                        .review_reasons
                        .iter()
                        .any(|reason| reason.code == code),
                    "missing {code}: {decision:#?}"
                );
            }
            assert!(!decision
                .passed_checks
                .iter()
                .any(|check| check == "track_engagement_type_allowed"));
        }
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
    fn workplace_evidence_cannot_bypass_remote_only_queue_authority() {
        let pool = test_pool();
        let profile = default_profile("jobs@example.com");
        save_profile(&pool, "acct-jobs", &profile).unwrap();
        let preferences = JobPreferences {
            desired_locations: vec!["Remote - United States".to_string()],
            location_policy: "remote_only".to_string(),
            remote_preference: "remote_only".to_string(),
            sponsorship: "not_required".to_string(),
            ..JobPreferences::default()
        };
        save_preferences(&pool, "acct-jobs", &preferences).unwrap();
        let mut track = list_tracks(&pool, "acct-jobs").unwrap().remove(0);
        track.locations = vec!["Remote - United States".to_string()];
        track.remote_preference = "remote_only".to_string();
        upsert_track(&pool, "acct-jobs", &track).unwrap();

        let mut hybrid = test_posting(
            "https://boards.greenhouse.io/acme/jobs/hybrid-not-remote",
            now_ms(),
            now_ms(),
        );
        hybrid.location = "United States".to_string();
        hybrid.workplace = "hybrid".to_string();
        let hybrid = upsert_posting(&pool, "acct-jobs", &hybrid, &profile, &preferences).unwrap();
        let hybrid_decision =
            evaluate_job_eligibility(&pool, "acct-jobs", &hybrid, true, None).unwrap();
        assert!(!hybrid_decision.can_queue_local);
        assert!(!hybrid_decision.can_queue_cloud);
        assert!(hybrid_decision
            .hard_failures
            .iter()
            .any(|reason| reason.code == "location_mismatch"));

        for (suffix, workplace) in [
            ("negated", "not remote"),
            ("cannot-be-remote", "cannot be remote"),
            ("remote-disallowed", "does not allow remote"),
            ("mixed", "remote or hybrid"),
            ("unknown", "flexible"),
        ] {
            let mut posting = test_posting(
                &format!("https://boards.greenhouse.io/acme/jobs/{suffix}-workplace"),
                now_ms(),
                now_ms(),
            );
            posting.location = "United States".to_string();
            posting.workplace = workplace.to_string();
            let posting =
                upsert_posting(&pool, "acct-jobs", &posting, &profile, &preferences).unwrap();
            let decision =
                evaluate_job_eligibility(&pool, "acct-jobs", &posting, true, None).unwrap();
            assert!(
                !decision.can_queue_local,
                "unexpected local queue for {workplace}"
            );
            assert!(
                !decision.can_queue_cloud,
                "unexpected cloud queue for {workplace}"
            );
            assert!(
                decision
                    .review_reasons
                    .iter()
                    .any(|reason| reason.code == "location_taxonomy_review_required"),
                "missing typed workplace review for {workplace}: {decision:#?}"
            );
        }

        let mut excluded = test_posting(
            "https://boards.greenhouse.io/acme/jobs/remote-outside-us",
            now_ms(),
            now_ms(),
        );
        excluded.location = "Remote outside United States".to_string();
        excluded.workplace = "remote".to_string();
        let excluded =
            upsert_posting(&pool, "acct-jobs", &excluded, &profile, &preferences).unwrap();
        let excluded_decision =
            evaluate_job_eligibility(&pool, "acct-jobs", &excluded, true, None).unwrap();
        assert!(!excluded_decision.can_queue_local);
        assert!(!excluded_decision.can_queue_cloud);
        assert!(excluded_decision
            .review_reasons
            .iter()
            .any(|reason| reason.code == "location_taxonomy_review_required"));
        assert!(!excluded_decision
            .passed_checks
            .iter()
            .any(|check| check == "location_allowed"));

        let mut remote = test_posting(
            "https://boards.greenhouse.io/acme/jobs/proven-remote",
            now_ms(),
            now_ms(),
        );
        remote.location = "United States".to_string();
        remote.workplace = "remote".to_string();
        let remote = upsert_posting(&pool, "acct-jobs", &remote, &profile, &preferences).unwrap();
        let remote_decision =
            evaluate_job_eligibility(&pool, "acct-jobs", &remote, true, None).unwrap();
        assert!(remote_decision
            .passed_checks
            .iter()
            .any(|check| check == "location_allowed"));
    }

    fn seed_active_reservation_for_quota_eligibility(
        pool: &DbPool,
        application: &JobApplication,
        posting: &JobPosting,
        preferences: &JobPreferences,
    ) {
        let at_ms = now_ms();
        let period_key = attempt_period_key(at_ms, preferences.time_zone_offset_minutes);
        pool.get()
            .unwrap()
            .execute(
                "INSERT INTO jobs_attempt_reservations (
                    id, account_id, application_id, company_key, period_key, runner, status,
                    reserved_at_ms, updated_at_ms
                 ) VALUES (?1, 'acct-jobs', ?2, ?3, ?4, 'local', 'reserved', ?5, ?5)",
                params![
                    format!("quota-reservation-{}", application.id),
                    application.id,
                    normalize_company_key(&posting.company),
                    period_key,
                    at_ms,
                ],
            )
            .unwrap();
    }

    #[test]
    fn eligibility_enforces_company_and_daily_limits_from_active_reservations() {
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
        seed_active_reservation_for_quota_eligibility(&pool, &application, &first, &preferences);

        let mut second = test_posting(
            "https://boards.greenhouse.io/acme/jobs/two",
            now_ms(),
            now_ms(),
        );
        second.title = "Backend Engineer".to_string();
        let second = verified_test_posting(second, now_ms());
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
            let posting = verified_test_posting(posting, now_ms());
            let posting =
                upsert_posting(&pool, "acct-jobs", &posting, &profile, &preferences).unwrap();
            let (application, _) =
                prepare_application(&pool, "acct-jobs", &posting.id, "factual", "review_first")
                    .unwrap();
            seed_active_reservation_for_quota_eligibility(
                &pool,
                &application,
                &posting,
                &preferences,
            );
        }

        let mut final_posting = test_posting(
            "https://boards.greenhouse.io/globex/jobs/final",
            now_ms(),
            now_ms(),
        );
        final_posting.company = "Globex".to_string();
        final_posting.title = "Platform Engineer".to_string();
        let final_posting = verified_test_posting(final_posting, now_ms());
        let final_posting =
            upsert_posting(&pool, "acct-jobs", &final_posting, &profile, &preferences).unwrap();
        assert!(prepare_application(
            &pool,
            "acct-jobs",
            &final_posting.id,
            "factual",
            "review_first",
        )
        .unwrap_err()
        .to_string()
        .contains("Today's application limit"));

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
        let (profile, preferences) =
            execution_policy_fixture(&pool, "acct-jobs", "jobs@example.com", "track-default");
        let now = now_ms();
        let posting = upsert_posting(
            &pool,
            "acct-jobs",
            &test_posting(
                "https://boards.greenhouse.io/acme/jobs/state-machine",
                now,
                now,
            ),
            &profile,
            &preferences,
        )
        .unwrap();
        let eligibility =
            evaluate_job_eligibility(&pool, "acct-jobs", &posting, true, None).unwrap();
        assert!(eligibility.can_prepare, "{eligibility:#?}");
        assert!(!eligibility.can_queue_local, "{eligibility:#?}");
        assert!(!eligibility.can_queue_cloud, "{eligibility:#?}");
        assert!(!eligibility.can_auto_submit, "{eligibility:#?}");
        for reason in [
            "employer_identity_review_required",
            "job_risk_review_required",
        ] {
            assert!(
                eligibility
                    .review_reasons
                    .iter()
                    .any(|candidate| candidate.code == reason),
                "missing {reason}: {eligibility:#?}"
            );
        }
        let (application, _) =
            prepare_application(&pool, "acct-jobs", &posting.id, "factual", "review_first")
                .unwrap();

        let queue_without_approved_execution = update_application(
            &pool,
            "acct-jobs",
            &application.id,
            "queued",
            Some("auto_submit"),
        );
        assert!(queue_without_approved_execution.is_err());

        let invalid_transition =
            update_application(&pool, "acct-jobs", &application.id, "submitted", None);
        assert!(invalid_transition.is_err());

        let invalid_mode = update_application(
            &pool,
            "acct-jobs",
            &application.id,
            "awaiting_review",
            Some("surprise_me"),
        );
        assert!(invalid_mode.is_err());

        let unchanged = get_application(&pool, "acct-jobs", &application.id)
            .unwrap()
            .unwrap();
        assert_eq!(unchanged.state, "awaiting_review");
        assert_eq!(unchanged.submission_mode, "review_first");
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
            connection_id: None,
            authorization_purpose: String::new(),
            requested_scopes: Vec::new(),
            requested_capabilities: Vec::new(),
            expected_grant_revision: 0,
            return_path: "/jobs/settings".to_string(),
            expires_at_ms: now + 60_000,
            created_at_ms: now,
        };
        save_jobs_oauth_state(&pool, "acct-jobs", token, &state).unwrap();
        assert!(save_jobs_oauth_state(&pool, "acct-jobs", token, &state).is_err());
        assert!(save_jobs_oauth_state(
            &pool,
            "acct-jobs",
            " state-token-with-more-than-thirty-two-random-characters",
            &state,
        )
        .is_err());
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
    fn oauth_state_creation_obeys_the_account_deletion_write_fence() {
        let pool = test_pool();
        pool.get()
            .unwrap()
            .execute(
                "INSERT INTO account_deletion_intents (
                    account_id, requested_at_ms, last_checked_at_ms,
                    fresh_upload_cutoff_ms, fresh_in_flight_puts
                 ) VALUES ('acct-jobs', 1, 1, 0, 0)",
                [],
            )
            .unwrap();
        let now = now_ms();
        assert_account_deletion_fence(save_jobs_oauth_state(
            &pool,
            "acct-jobs",
            "oauth-state-token-that-is-long-enough-for-the-fence-test",
            &JobsOAuthState {
                provider: "gmail".to_string(),
                code_verifier: "oauth-code-verifier".to_string(),
                connection_id: None,
                authorization_purpose: "mailbox_read".to_string(),
                requested_scopes: Vec::new(),
                requested_capabilities: Vec::new(),
                expected_grant_revision: 0,
                return_path: "/jobs/settings".to_string(),
                expires_at_ms: now + 60_000,
                created_at_ms: now,
            },
        ));
        let stored: i64 = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM jobs_oauth_states WHERE account_id = 'acct-jobs'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(stored, 0);
    }

    #[test]
    fn oauth_state_creation_serializes_with_account_deletion() {
        let pool = test_pool();
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(3));
        let state = JobsOAuthState {
            provider: "gmail".to_string(),
            code_verifier: "concurrent-oauth-code-verifier".to_string(),
            connection_id: None,
            authorization_purpose: "mailbox_read".to_string(),
            requested_scopes: Vec::new(),
            requested_capabilities: Vec::new(),
            expected_grant_revision: 0,
            return_path: "/jobs/settings".to_string(),
            expires_at_ms: now_ms() + 60_000,
            created_at_ms: now_ms(),
        };
        let save_pool = pool.clone();
        let save_barrier = barrier.clone();
        let save = std::thread::spawn(move || {
            save_barrier.wait();
            save_jobs_oauth_state(
                &save_pool,
                "acct-jobs",
                "concurrent-oauth-state-token-that-is-long-enough",
                &state,
            )
        });
        let deletion_pool = pool.clone();
        let deletion_barrier = barrier.clone();
        let deletion = std::thread::spawn(move || {
            deletion_barrier.wait();
            crate::db::account_data::begin_account_deletion(&deletion_pool, "acct-jobs", now_ms())
        });
        barrier.wait();
        let saved = save.join().unwrap();
        assert!(deletion.join().unwrap().unwrap().is_some());
        if let Err(error) = saved {
            assert!(matches!(
                error.downcast_ref::<UploadControlError>(),
                Some(UploadControlError::AccountDeleting)
            ));
        }
        assert_account_deletion_fence(save_jobs_oauth_state(
            &pool,
            "acct-jobs",
            "post-deletion-oauth-state-token-that-is-long-enough",
            &JobsOAuthState {
                provider: "gmail".to_string(),
                code_verifier: "post-deletion-code-verifier".to_string(),
                connection_id: None,
                authorization_purpose: "mailbox_read".to_string(),
                requested_scopes: Vec::new(),
                requested_capabilities: Vec::new(),
                expected_grant_revision: 0,
                return_path: "/jobs/settings".to_string(),
                expires_at_ms: now_ms() + 60_000,
                created_at_ms: now_ms(),
            },
        ));
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
            capabilities: Vec::new(),
            grant_revision: 0,
            grant_sha256: String::new(),
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
    fn provider_refresh_cas_preserves_the_winning_rotated_refresh_token() {
        let pool = test_pool();
        let now = now_ms();
        let (mailbox, expected) = save_mailbox_connection_with_credential(
            &pool,
            "acct-jobs",
            &MailboxConnection {
                id: String::new(),
                provider: "gmail".to_string(),
                status: "connected".to_string(),
                account_label: "jobs@example.com".to_string(),
                aliases: Vec::new(),
                capabilities: Vec::new(),
                created_at_ms: now,
                updated_at_ms: now,
            },
            &JobsProviderCredential {
                connection_id: String::new(),
                provider: "gmail".to_string(),
                provider_subject: "google-subject-refresh-cas".to_string(),
                access_token: "dummy-initial-access-token".to_string(),
                refresh_token: "dummy-initial-refresh-token".to_string(),
                scopes: vec!["gmail.readonly".to_string()],
                capabilities: Vec::new(),
                grant_revision: 0,
                grant_sha256: String::new(),
                expires_at_ms: now + 3_600_000,
                created_at_ms: now,
                updated_at_ms: now,
            },
        )
        .unwrap();
        let winner = refresh_jobs_provider_credential_cas(
            &pool,
            "acct-jobs",
            &expected,
            "dummy-winner-access-token",
            Some("dummy-winner-rotated-refresh-token"),
            now + 7_200_000,
        )
        .unwrap();
        assert_eq!(winner.refresh_token, "dummy-winner-rotated-refresh-token");
        assert!(refresh_jobs_provider_credential_cas(
            &pool,
            "acct-jobs",
            &expected,
            "dummy-stale-access-token",
            Some("dummy-stale-refresh-token"),
            now + 7_200_000,
        )
        .unwrap_err()
        .to_string()
        .contains("lost CAS"));
        let stored = jobs_provider_credential(&pool, "acct-jobs", &mailbox.id)
            .unwrap()
            .unwrap();
        assert_eq!(stored.access_token, "dummy-winner-access-token");
        assert_eq!(stored.refresh_token, "dummy-winner-rotated-refresh-token");
        assert_eq!(stored.grant_revision, expected.grant_revision);
        assert_eq!(stored.grant_sha256, expected.grant_sha256);
    }

    #[test]
    fn accepted_provider_token_boundary_remains_refreshable() {
        let pool = test_pool();
        let now = now_ms();
        let token_bytes = crate::jobs_provider_auth::MAX_PROVIDER_TOKEN_BYTES;
        let initial_access = "a".repeat(token_bytes);
        let initial_refresh = "b".repeat(token_bytes);
        let next_access = "c".repeat(token_bytes);
        let next_refresh = "d".repeat(token_bytes);
        let (mailbox, expected) = save_mailbox_connection_with_credential(
            &pool,
            "acct-jobs",
            &MailboxConnection {
                id: String::new(),
                provider: "gmail".to_string(),
                status: "connected".to_string(),
                account_label: "jobs@example.com".to_string(),
                aliases: Vec::new(),
                capabilities: Vec::new(),
                created_at_ms: now,
                updated_at_ms: now,
            },
            &JobsProviderCredential {
                connection_id: String::new(),
                provider: "gmail".to_string(),
                provider_subject: "google-subject-token-boundary".to_string(),
                access_token: initial_access,
                refresh_token: initial_refresh,
                scopes: vec!["gmail.readonly".to_string()],
                capabilities: Vec::new(),
                grant_revision: 0,
                grant_sha256: String::new(),
                expires_at_ms: now + 3_600_000,
                created_at_ms: now,
                updated_at_ms: now,
            },
        )
        .unwrap();

        let refreshed = refresh_jobs_provider_credential_cas(
            &pool,
            "acct-jobs",
            &expected,
            &next_access,
            Some(&next_refresh),
            now + 7_200_000,
        )
        .unwrap();
        assert_eq!(refreshed.connection_id, mailbox.id);
        assert_eq!(refreshed.access_token.len(), token_bytes);
        assert_eq!(refreshed.refresh_token.len(), token_bytes);
        assert!(refresh_jobs_provider_credential_cas(
            &pool,
            "acct-jobs",
            &refreshed,
            &"e".repeat(token_bytes + 1),
            None,
            now + 10_800_000,
        )
        .is_err());
    }

    #[test]
    fn stale_provider_refresh_cannot_overwrite_a_grant_upgrade() {
        let pool = test_pool();
        let now = now_ms();
        let (mailbox, stale) = save_mailbox_connection_with_credential(
            &pool,
            "acct-jobs",
            &MailboxConnection {
                id: String::new(),
                provider: "gmail".to_string(),
                status: "connected".to_string(),
                account_label: "jobs@example.com".to_string(),
                aliases: Vec::new(),
                capabilities: vec!["recruiter_reply".to_string()],
                created_at_ms: now,
                updated_at_ms: now,
            },
            &JobsProviderCredential {
                connection_id: String::new(),
                provider: "gmail".to_string(),
                provider_subject: "google-subject-grant-upgrade".to_string(),
                access_token: "dummy-grant-one-access-token".to_string(),
                refresh_token: "dummy-grant-one-refresh-token".to_string(),
                scopes: vec!["https://www.googleapis.com/auth/gmail.send".to_string()],
                capabilities: vec!["recruiter_reply".to_string()],
                grant_revision: 1,
                grant_sha256: String::new(),
                expires_at_ms: now + 3_600_000,
                created_at_ms: now,
                updated_at_ms: now,
            },
        )
        .unwrap();
        let upgraded_mailbox = MailboxConnection {
            capabilities: vec![
                "recruiter_reply".to_string(),
                "interview_calendar".to_string(),
            ],
            ..mailbox.clone()
        };
        let upgraded_credential = JobsProviderCredential {
            access_token: "dummy-grant-two-access-token".to_string(),
            refresh_token: "dummy-grant-two-refresh-token".to_string(),
            scopes: vec![
                "https://www.googleapis.com/auth/gmail.send".to_string(),
                "https://www.googleapis.com/auth/calendar.events".to_string(),
            ],
            capabilities: upgraded_mailbox.capabilities.clone(),
            grant_revision: 2,
            grant_sha256: String::new(),
            expires_at_ms: now + 7_200_000,
            ..stale.clone()
        };
        let (_, upgraded) = save_mailbox_connection_with_credential_cas(
            &pool,
            "acct-jobs",
            &upgraded_mailbox,
            &upgraded_credential,
            1,
        )
        .unwrap();
        assert_eq!(upgraded.grant_revision, 2);
        assert!(
            upgraded.updated_at_ms > stale.updated_at_ms,
            "a grant upgrade must advance the refresh CAS authority even within one clock tick"
        );
        assert!(refresh_jobs_provider_credential_cas(
            &pool,
            "acct-jobs",
            &stale,
            "stale-refresh-access-token",
            Some("dummy-stale-refresh-token"),
            now + 7_200_000,
        )
        .unwrap_err()
        .to_string()
        .contains("lost CAS"));
        let stored = jobs_provider_credential(&pool, "acct-jobs", &mailbox.id)
            .unwrap()
            .unwrap();
        assert_eq!(stored.grant_revision, 2);
        assert_eq!(stored.grant_sha256, upgraded.grant_sha256);
        assert_eq!(stored.refresh_token, "dummy-grant-two-refresh-token");
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
                capabilities: Vec::new(),
                grant_revision: 0,
                grant_sha256: String::new(),
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
        assert!(
            claim_mailbox_sync(&pool, "acct-jobs", &mailbox.id, "single-worker", 60_000)
                .unwrap()
                .is_none()
        );
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
        pool.get()
            .unwrap()
            .execute(
                "INSERT INTO accounts (id, email, password_hash, trial_seconds_remaining)
                 VALUES ('acct-other', 'reconnect-other@example.com', 'hash', 0)",
                [],
            )
            .unwrap();
        assert!(!mark_mailbox_reauthorization_required(&pool, "acct-other", &mailbox.id).unwrap());
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
    fn provider_message_identity_is_scoped_to_the_exact_mailbox() {
        let pool = test_pool();
        set_entitlement_plan(&pool, "acct-jobs", "pro").unwrap();
        let first_mailbox = save_mailbox_connection(
            &pool,
            "acct-jobs",
            &MailboxConnection {
                id: String::new(),
                provider: "gmail".to_string(),
                status: "connected".to_string(),
                account_label: "first@example.com".to_string(),
                aliases: Vec::new(),
                capabilities: Vec::new(),
                created_at_ms: 0,
                updated_at_ms: 0,
            },
            "google-subject-message-scope-first",
        )
        .unwrap();
        let second_mailbox = save_mailbox_connection(
            &pool,
            "acct-jobs",
            &MailboxConnection {
                id: String::new(),
                provider: "gmail".to_string(),
                status: "connected".to_string(),
                account_label: "second@example.com".to_string(),
                aliases: Vec::new(),
                capabilities: Vec::new(),
                created_at_ms: 0,
                updated_at_ms: 0,
            },
            "google-subject-message-scope-second",
        )
        .unwrap();
        let first_message = JobsProviderMessage {
            id: String::new(),
            connection_id: first_mailbox.id.clone(),
            provider: "gmail".to_string(),
            external_id: "same-provider-object-id".to_string(),
            sender: "first-recruiter@example.org".to_string(),
            recipients: vec![first_mailbox.account_label.clone()],
            subject: "First mailbox".to_string(),
            body_text: "First mailbox body".to_string(),
            received_at_ms: now_ms(),
            application_id: None,
            processing_status: "received".to_string(),
            classification: String::new(),
            confidence: 0.0,
            metadata: json!({"thread_id": "first-thread"}),
            processed_at_ms: None,
            created_at_ms: 0,
            updated_at_ms: 0,
        };
        let mut second_message = first_message.clone();
        second_message.connection_id = second_mailbox.id.clone();
        second_message.sender = "second-recruiter@example.org".to_string();
        second_message.recipients = vec![second_mailbox.account_label.clone()];
        second_message.subject = "Second mailbox".to_string();
        second_message.metadata = json!({"thread_id": "second-thread"});

        let (first_stored, first_inserted) =
            save_provider_message(&pool, "acct-jobs", &first_message).unwrap();
        let (second_stored, second_inserted) =
            save_provider_message(&pool, "acct-jobs", &second_message).unwrap();
        assert!(first_inserted);
        assert!(second_inserted);
        assert_ne!(first_stored.id, second_stored.id);
        assert_eq!(first_stored.connection_id, first_mailbox.id);
        assert_eq!(second_stored.connection_id, second_mailbox.id);
        assert_eq!(
            list_provider_messages(&pool, "acct-jobs", Some(&first_mailbox.id), 20)
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            list_provider_messages(&pool, "acct-jobs", Some(&second_mailbox.id), 20)
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn provider_message_legacy_identity_migrates_without_duplication() {
        let pool = test_pool();
        let mailbox = save_mailbox_connection(
            &pool,
            "acct-jobs",
            &MailboxConnection {
                id: String::new(),
                provider: "gmail".to_string(),
                status: "connected".to_string(),
                account_label: "legacy@example.com".to_string(),
                aliases: Vec::new(),
                capabilities: Vec::new(),
                created_at_ms: 0,
                updated_at_ms: 0,
            },
            "google-subject-message-legacy",
        )
        .unwrap();
        let message = JobsProviderMessage {
            id: String::new(),
            connection_id: mailbox.id.clone(),
            provider: "gmail".to_string(),
            external_id: "legacy-provider-message-id".to_string(),
            sender: "legacy-recruiter@example.org".to_string(),
            recipients: vec![mailbox.account_label.clone()],
            subject: "Legacy identity".to_string(),
            body_text: "Legacy body".to_string(),
            received_at_ms: now_ms(),
            application_id: None,
            processing_status: "received".to_string(),
            classification: String::new(),
            confidence: 0.0,
            metadata: json!({}),
            processed_at_ms: None,
            created_at_ms: 0,
            updated_at_ms: 0,
        };
        let (stored, inserted) = save_provider_message(&pool, "acct-jobs", &message).unwrap();
        assert!(inserted);
        let legacy_hash =
            private_lookup_hash("jobs-provider-message:gmail", message.external_id.as_str())
                .unwrap();
        pool.get()
            .unwrap()
            .execute(
                "UPDATE jobs_provider_messages SET provider_message_hash = ?2 WHERE id = ?1",
                params![stored.id, legacy_hash],
            )
            .unwrap();

        let mut enriched_message = message.clone();
        enriched_message.metadata = json!({
            "thread_id": "legacy-thread",
            "rfc_message_id": "<legacy-provider-message-id@example.org>",
            "reply_target": "legacy-recruiter@example.org",
        });
        let (replayed, inserted_again) =
            save_provider_message(&pool, "acct-jobs", &enriched_message).unwrap();
        assert!(!inserted_again);
        assert_eq!(replayed.id, stored.id);
        assert_eq!(replayed.metadata["thread_id"], "legacy-thread");
        assert_eq!(
            replayed.metadata["rfc_message_id"],
            "<legacy-provider-message-id@example.org>"
        );
        assert_eq!(
            replayed.metadata["reply_target"],
            "legacy-recruiter@example.org"
        );
        let processed = update_provider_message_processing(
            &pool,
            "acct-jobs",
            &stored.id,
            None,
            "needs_input",
            "interview",
            0.9,
            json!({"correlation_score": 90}),
        )
        .unwrap()
        .unwrap();
        assert_eq!(processed.metadata["thread_id"], "legacy-thread");
        assert_eq!(
            processed.metadata["reply_target"],
            "legacy-recruiter@example.org"
        );
        assert_eq!(processed.metadata["correlation_score"], 90);
        let mut conflicting = enriched_message;
        conflicting.metadata["thread_id"] = json!("changed-thread");
        assert!(save_provider_message(&pool, "acct-jobs", &conflicting).is_err());
        let expected_hash = private_lookup_hash(
            &format!("jobs-provider-message:gmail:{}", mailbox.id),
            message.external_id.as_str(),
        )
        .unwrap();
        let (count, migrated_hash): (i64, String) = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT COUNT(*), MIN(provider_message_hash)
                   FROM jobs_provider_messages WHERE account_id = 'acct-jobs'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(count, 1);
        assert_eq!(migrated_hash, expected_hash);
    }

    fn communication_test_application(pool: &DbPool) -> JobApplication {
        let fixture = final_submission_fixture(pool, "communication-action");
        let finalized = finalize_submission(
            pool,
            "acct-jobs",
            &fixture.application.id,
            &fixture.run_id,
            "cloud",
            fixture.receipt,
            &fixture.fingerprint,
            &fixture.evidence,
            &fixture.object_uploads,
            &fixture.session,
            None,
        )
        .unwrap();
        match finalized {
            SubmissionFinalizeResult::Committed(application)
            | SubmissionFinalizeResult::Replayed(application) => application,
        }
    }

    fn communication_test_application_mailbox_and_message(
        pool: &DbPool,
    ) -> (JobApplication, MailboxConnection, JobsProviderMessage) {
        let application = communication_test_application(pool);
        let mailbox = save_mailbox_connection(
            pool,
            "acct-jobs",
            &MailboxConnection {
                id: String::new(),
                provider: "gmail".to_string(),
                status: "connected".to_string(),
                account_label: "jobs@example.com".to_string(),
                aliases: Vec::new(),
                capabilities: vec![
                    "recruiter_reply".to_string(),
                    "interview_calendar".to_string(),
                ],
                created_at_ms: 0,
                updated_at_ms: 0,
            },
            "google-subject-communication-action",
        )
        .unwrap();
        let credential = JobsProviderCredential {
            connection_id: mailbox.id.clone(),
            provider: "gmail".to_string(),
            provider_subject: "google-subject-communication-action".to_string(),
            access_token: "test-access-token".to_string(),
            refresh_token: "test-refresh-token".to_string(),
            scopes: vec![
                "https://www.googleapis.com/auth/gmail.send".to_string(),
                "https://www.googleapis.com/auth/calendar.events".to_string(),
            ],
            capabilities: mailbox.capabilities.clone(),
            grant_revision: 1,
            grant_sha256: String::new(),
            expires_at_ms: now_ms() + 3_600_000,
            created_at_ms: now_ms(),
            updated_at_ms: now_ms(),
        };
        save_jobs_provider_credential(pool, "acct-jobs", &credential).unwrap();
        let message = communication_test_source_message(
            pool,
            &application,
            &mailbox,
            "gmail-message-communication-action",
        );
        (application, mailbox, message)
    }

    fn communication_test_source_message(
        pool: &DbPool,
        application: &JobApplication,
        mailbox: &MailboxConnection,
        external_id: &str,
    ) -> JobsProviderMessage {
        save_provider_message(
            pool,
            "acct-jobs",
            &JobsProviderMessage {
                id: String::new(),
                connection_id: mailbox.id.clone(),
                provider: mailbox.provider.clone(),
                external_id: external_id.to_string(),
                sender: "recruiter@example.org".to_string(),
                recipients: vec![mailbox.account_label.clone()],
                subject: "Interview availability".to_string(),
                body_text: "Please share a few interview times.".to_string(),
                received_at_ms: now_ms(),
                application_id: Some(application.id.clone()),
                processing_status: "needs_input".to_string(),
                classification: "interview".to_string(),
                confidence: 0.98,
                metadata: if mailbox.provider == "gmail" {
                    json!({
                        "thread_id": format!("thread-{external_id}"),
                        "rfc_message_id": format!("<{external_id}@example.org>"),
                        "reply_target": "recruiter@example.org"
                    })
                } else {
                    json!({
                        "provider_id": format!("provider-{external_id}"),
                        "conversation_id": format!("conversation-{external_id}"),
                        "rfc_message_id": format!("<{external_id}@example.org>"),
                        "reply_target": "recruiter@example.org"
                    })
                },
                processed_at_ms: None,
                created_at_ms: 0,
                updated_at_ms: 0,
            },
        )
        .unwrap()
        .0
    }

    fn communication_test_action(
        application: &JobApplication,
        mailbox: &MailboxConnection,
        source_message: &JobsProviderMessage,
        idempotency_key: &str,
    ) -> JobsCommunicationAction {
        JobsCommunicationAction {
            id: String::new(),
            application_id: application.id.clone(),
            connection_id: mailbox.id.clone(),
            source_message_id: Some(source_message.id.clone()),
            kind: "reply".to_string(),
            provider: "gmail".to_string(),
            idempotency_key: idempotency_key.to_string(),
            payload: json!({
                "to": "recruiter@example.org",
                "subject": "Interview availability",
                "body_text": "Tuesday afternoon works for me."
            }),
            payload_sha256: String::new(),
            authority_sha256: String::new(),
            status: String::new(),
            provider_object_id: String::new(),
            lease_owner: None,
            lease_kind: None,
            lease_expires_at_ms: None,
            active_attempt_id: None,
            next_attempt_at_ms: 0,
            attempt_count: 0,
            reconciliation_count: 0,
            action_revision: 1,
            approval_revision: 0,
            approved_authority_sha256: String::new(),
            approved_grant_revision: 0,
            approved_grant_sha256: String::new(),
            approved_at_ms: None,
            dispatched_at_ms: None,
            created_at_ms: 0,
            updated_at_ms: 0,
        }
    }

    static COMMUNICATION_FLAG_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    struct CommunicationFlagGuard {
        _lock: std::sync::MutexGuard<'static, ()>,
        dispatch: Option<std::ffi::OsString>,
        reconciliation: Option<std::ffi::OsString>,
    }

    impl CommunicationFlagGuard {
        fn enabled() -> Self {
            Self::with_enabled(true)
        }

        fn disabled() -> Self {
            Self::with_enabled(false)
        }

        fn with_enabled(enabled: bool) -> Self {
            let lock = COMMUNICATION_FLAG_ENV_LOCK
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let guard = Self {
                _lock: lock,
                dispatch: std::env::var_os("BLUEY_JOBS_COMMUNICATION_DISPATCH_ENABLED"),
                reconciliation: std::env::var_os("BLUEY_JOBS_COMMUNICATION_RECONCILIATION_ENABLED"),
            };
            if enabled {
                std::env::set_var("BLUEY_JOBS_COMMUNICATION_DISPATCH_ENABLED", "true");
                std::env::set_var("BLUEY_JOBS_COMMUNICATION_RECONCILIATION_ENABLED", "true");
            } else {
                std::env::remove_var("BLUEY_JOBS_COMMUNICATION_DISPATCH_ENABLED");
                std::env::remove_var("BLUEY_JOBS_COMMUNICATION_RECONCILIATION_ENABLED");
            }
            guard
        }
    }

    impl Drop for CommunicationFlagGuard {
        fn drop(&mut self) {
            if let Some(value) = self.dispatch.take() {
                std::env::set_var("BLUEY_JOBS_COMMUNICATION_DISPATCH_ENABLED", value);
            } else {
                std::env::remove_var("BLUEY_JOBS_COMMUNICATION_DISPATCH_ENABLED");
            }
            if let Some(value) = self.reconciliation.take() {
                std::env::set_var("BLUEY_JOBS_COMMUNICATION_RECONCILIATION_ENABLED", value);
            } else {
                std::env::remove_var("BLUEY_JOBS_COMMUNICATION_RECONCILIATION_ENABLED");
            }
        }
    }

    fn communication_lease_access(
        lease: &JobsCommunicationActionLease,
        owner_id: &str,
    ) -> JobsCommunicationLeaseAccess {
        JobsCommunicationLeaseAccess {
            account_id: lease.account_id.clone(),
            action_id: lease.action.id.clone(),
            attempt_id: lease.attempt_id.clone(),
            owner_id: owner_id.to_string(),
            lease_token: lease.lease_token.clone(),
            fence: lease.fence,
            authority_sha256: lease.authority_sha256.clone(),
            approval_revision: lease.approval_revision,
            grant_revision: lease.grant_revision,
            grant_sha256: lease.grant_sha256.clone(),
        }
    }

    fn approve_communication_action(
        pool: &DbPool,
        account_id: &str,
        action_id: &str,
    ) -> Result<Option<JobsCommunicationAction>> {
        let action = super::communication_action(pool, account_id, action_id)?
            .ok_or_else(|| anyhow::anyhow!("communication action not found"))?;
        super::approve_communication_action(
            pool,
            account_id,
            action_id,
            action.action_revision,
            &action.payload_sha256,
        )
    }

    fn cancel_communication_action(
        pool: &DbPool,
        account_id: &str,
        action_id: &str,
    ) -> Result<Option<JobsCommunicationAction>> {
        let action = super::communication_action(pool, account_id, action_id)?
            .ok_or_else(|| anyhow::anyhow!("communication action not found"))?;
        super::cancel_communication_action(
            pool,
            account_id,
            action_id,
            action.action_revision,
            &action.payload_sha256,
        )
    }

    fn communication_success_evidence(
        pool: &DbPool,
        lease: &JobsCommunicationActionLease,
        provider_object_id: &str,
    ) -> Value {
        let action = &lease.action;
        let mut evidence = json!({
            "provider": action.provider,
            "provider_object_id": provider_object_id,
            "payload_sha256": action.payload_sha256,
            "action_id_sha256": hex::encode(Sha256::digest(action.id.as_bytes())),
            "provider_operation_key_sha256": hex::encode(Sha256::digest(
                lease.provider_operation_key.as_bytes()
            )),
        });
        match action.provider.as_str() {
            "gmail" | "outlook_email" => {
                evidence["operation_message_id"] = json!(communication_operation_message_id(
                    &lease.provider_operation_key
                ));
                let source = provider_message(
                    pool,
                    &lease.account_id,
                    &lease.action.connection_id,
                    lease.action.source_message_id.as_deref().unwrap(),
                )
                .unwrap()
                .unwrap();
                let (metadata_key, evidence_key) = if action.provider == "gmail" {
                    ("thread_id", "thread_sha256")
                } else {
                    ("conversation_id", "conversation_sha256")
                };
                let source_identity = source.metadata[metadata_key].as_str().unwrap();
                evidence[evidence_key] =
                    json!(hex::encode(Sha256::digest(source_identity.as_bytes())));
            }
            "google_calendar" => {
                evidence["deterministic_event_id"] =
                    json!(communication_google_event_id(&lease.provider_operation_key));
                evidence["private_marker_sha256"] = json!(hex::encode(Sha256::digest(
                    lease.provider_operation_key.as_bytes()
                )));
            }
            "outlook_calendar" => {
                evidence["transaction_id"] = json!(communication_microsoft_transaction_id(
                    &lease.provider_operation_key
                ));
                evidence["extended_property_sha256"] = json!(hex::encode(Sha256::digest(
                    lease.provider_operation_key.as_bytes()
                )));
            }
            _ => {}
        }
        evidence
    }

    fn communication_success_finish(
        pool: &DbPool,
        lease: &JobsCommunicationActionLease,
        owner_id: &str,
        provider_object_id: &str,
    ) -> JobsCommunicationActionFinish {
        JobsCommunicationActionFinish {
            lease: communication_lease_access(lease, owner_id),
            outcome: if lease.action.kind == "reply" {
                "sent".to_string()
            } else {
                "calendar_created".to_string()
            },
            provider_object_id: provider_object_id.to_string(),
            evidence: communication_success_evidence(pool, lease, provider_object_id),
        }
    }

    fn communication_reconciliation_evidence(
        lease: &JobsCommunicationActionLease,
        evidence: Value,
    ) -> Value {
        let mut object = evidence.as_object().cloned().unwrap_or_default();
        object.insert(
            "provider".to_string(),
            Value::String(lease.action.provider.clone()),
        );
        object.insert(
            "action_id_sha256".to_string(),
            Value::String(hex::encode(Sha256::digest(lease.action.id.as_bytes()))),
        );
        object.insert(
            "payload_sha256".to_string(),
            Value::String(lease.action.payload_sha256.clone()),
        );
        object.insert(
            "provider_operation_key_sha256".to_string(),
            Value::String(hex::encode(Sha256::digest(
                lease.provider_operation_key.as_bytes(),
            ))),
        );
        Value::Object(object)
    }

    #[test]
    #[serial_test::serial]
    fn fix_728_communication_dispatch_authenticates_submitted_domain_before_effect() {
        let _flags = CommunicationFlagGuard::enabled();
        let positive_pool = test_pool();
        let (positive_application, positive_mailbox, positive_message) =
            communication_test_application_mailbox_and_message(&positive_pool);
        assert_eq!(positive_application.state, "submitted");
        let positive = communication_test_action(
            &positive_application,
            &positive_mailbox,
            &positive_message,
            "fix-728-submitted-communication-positive",
        );
        let (positive, _) =
            create_communication_action(&positive_pool, "acct-jobs", &positive).unwrap();
        approve_communication_action(&positive_pool, "acct-jobs", &positive.id)
            .unwrap()
            .unwrap();
        let lease =
            claim_communication_action(&positive_pool, "fix-728-submitted-communication-worker")
                .unwrap()
                .expect("validated submitted communication claim");
        let access = communication_lease_access(&lease, "fix-728-submitted-communication-worker");
        let started = mark_communication_action_request_started(&positive_pool, &access)
            .unwrap()
            .expect("fresh communication request-start authority");
        assert_eq!(started.status, "dispatching");
        let replayed = mark_communication_action_request_started(&positive_pool, &access)
            .unwrap()
            .expect("exact communication request-start replay");
        assert_eq!(replayed.id, started.id);
        assert_eq!(
            positive_pool
                .get()
                .unwrap()
                .query_row(
                    "SELECT COUNT(*) FROM jobs_communication_action_attempt_evidence
                      WHERE attempt_id = ?1 AND event_kind = 'request_started'",
                    params![lease.attempt_id],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
            1
        );
        let mut replay_tampered_application = positive_application.clone();
        replay_tampered_application.receipt[SERVER_SUBMISSION_AUTHORITY_KEY]
            ["preSubmissionReceipt"]["job_integrity"]["canonicalEmployerDomain"] =
            json!("replay-lookalike.example");
        positive_pool
            .get()
            .unwrap()
            .execute(
                "UPDATE jobs_applications SET application_json = ?2 WHERE id = ?1",
                params![
                    replay_tampered_application.id,
                    to_json(
                        &replay_tampered_application,
                        "tampered communication replay application",
                    )
                    .unwrap(),
                ],
            )
            .unwrap();
        assert!(mark_communication_action_request_started(&positive_pool, &access).is_err());
        assert_eq!(
            positive_pool
                .get()
                .unwrap()
                .query_row(
                    "SELECT COUNT(*) FROM jobs_communication_action_attempt_evidence
                      WHERE attempt_id = ?1 AND event_kind = 'request_started'",
                    params![lease.attempt_id],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
            1
        );
        let replay_unchanged = communication_action(&positive_pool, "acct-jobs", &started.id)
            .unwrap()
            .unwrap();
        assert_eq!(replay_unchanged.status, "dispatching");
        assert_eq!(
            replay_unchanged.active_attempt_id.as_deref(),
            Some(lease.attempt_id.as_str())
        );

        let tampered_pool = test_pool();
        let (mut tampered_application, tampered_mailbox, tampered_message) =
            communication_test_application_mailbox_and_message(&tampered_pool);
        let tampered = communication_test_action(
            &tampered_application,
            &tampered_mailbox,
            &tampered_message,
            "fix-728-submitted-communication-tampered",
        );
        let (tampered, _) =
            create_communication_action(&tampered_pool, "acct-jobs", &tampered).unwrap();
        let tampered = approve_communication_action(&tampered_pool, "acct-jobs", &tampered.id)
            .unwrap()
            .unwrap();
        tampered_application.receipt[SERVER_SUBMISSION_AUTHORITY_KEY]["preSubmissionReceipt"]
            ["job_integrity"]["canonicalEmployerDomain"] = json!("claim-lookalike.example");
        let tampered_payload =
            to_json(&tampered_application, "tampered communication application").unwrap();
        tampered_pool
            .get()
            .unwrap()
            .execute(
                "UPDATE jobs_applications SET application_json = ?2 WHERE id = ?1",
                params![tampered_application.id, tampered_payload],
            )
            .unwrap();
        assert!(claim_communication_action(
            &tampered_pool,
            "fix-728-tampered-communication-worker",
        )
        .is_err());
        let unchanged = communication_action(&tampered_pool, "acct-jobs", &tampered.id)
            .unwrap()
            .unwrap();
        assert_eq!(unchanged.status, "approved");
        assert_eq!(unchanged.attempt_count, 0);
        assert!(unchanged.active_attempt_id.is_none());
        assert_eq!(
            tampered_pool
                .get()
                .unwrap()
                .query_row(
                    "SELECT COUNT(*) FROM jobs_communication_action_attempts WHERE action_id = ?1",
                    params![tampered.id],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
            0
        );
    }

    #[test]
    #[serial_test::serial]
    fn processed_provider_messages_keep_exact_reply_authority_through_dispatch() {
        let _flags = CommunicationFlagGuard::enabled();
        let pool = test_pool();
        set_entitlement_plan(&pool, "acct-jobs", "pro").unwrap();
        let application = communication_test_application(&pool);
        for (connection_provider, action_provider, subject, scopes, metadata) in [
            (
                "gmail",
                "gmail",
                "google-subject-processed-reply",
                vec!["https://www.googleapis.com/auth/gmail.send".to_string()],
                json!({
                    "thread_id": "processed-gmail-thread",
                    "rfc_message_id": "<processed-gmail@example.org>",
                    "reply_target": "gmail-recruiter@example.org",
                }),
            ),
            (
                "outlook",
                "outlook_email",
                "microsoft-subject-processed-reply",
                vec!["Mail.Send".to_string()],
                json!({
                    "provider_id": "processed-outlook-message",
                    "conversation_id": "processed-outlook-conversation",
                    "reply_target": "outlook-recruiter@example.org",
                }),
            ),
        ] {
            let mailbox = save_mailbox_connection(
                &pool,
                "acct-jobs",
                &MailboxConnection {
                    id: String::new(),
                    provider: connection_provider.to_string(),
                    status: "connected".to_string(),
                    account_label: format!("{connection_provider}@example.com"),
                    aliases: Vec::new(),
                    capabilities: vec!["recruiter_reply".to_string()],
                    created_at_ms: 0,
                    updated_at_ms: 0,
                },
                subject,
            )
            .unwrap();
            save_jobs_provider_credential(
                &pool,
                "acct-jobs",
                &JobsProviderCredential {
                    connection_id: mailbox.id.clone(),
                    provider: connection_provider.to_string(),
                    provider_subject: subject.to_string(),
                    access_token: format!("{connection_provider}-processed-access-token"),
                    refresh_token: format!("{connection_provider}-processed-refresh-token"),
                    scopes,
                    capabilities: vec!["recruiter_reply".to_string()],
                    grant_revision: 1,
                    grant_sha256: String::new(),
                    expires_at_ms: now_ms() + 3_600_000,
                    created_at_ms: now_ms(),
                    updated_at_ms: now_ms(),
                },
            )
            .unwrap();
            let reply_target = metadata["reply_target"].as_str().unwrap().to_string();
            let external_id = if connection_provider == "outlook" {
                "<processed-outlook@example.org>".to_string()
            } else {
                "gmail-processed-message".to_string()
            };
            let message = save_provider_message(
                &pool,
                "acct-jobs",
                &JobsProviderMessage {
                    id: String::new(),
                    connection_id: mailbox.id.clone(),
                    provider: connection_provider.to_string(),
                    external_id,
                    sender: reply_target.clone(),
                    recipients: vec![mailbox.account_label.clone()],
                    subject: "Processed interview reply".to_string(),
                    body_text: "Please confirm a time.".to_string(),
                    received_at_ms: now_ms(),
                    application_id: Some(application.id.clone()),
                    processing_status: "received".to_string(),
                    classification: String::new(),
                    confidence: 0.0,
                    metadata,
                    processed_at_ms: None,
                    created_at_ms: 0,
                    updated_at_ms: 0,
                },
            )
            .unwrap()
            .0;
            let processed = update_provider_message_processing(
                &pool,
                "acct-jobs",
                &message.id,
                Some(&application.id),
                "needs_input",
                "interview",
                0.95,
                json!({"correlation_score": 95, "intervention_id": "review-1"}),
            )
            .unwrap()
            .unwrap();
            assert_eq!(processed.metadata["reply_target"], reply_target);
            assert_eq!(processed.metadata["correlation_score"], 95);

            let mut action = communication_test_action(
                &application,
                &mailbox,
                &processed,
                &format!("processed-{action_provider}-reply"),
            );
            action.provider = action_provider.to_string();
            action.payload["to"] = json!(reply_target);
            let (action, _) = create_communication_action(&pool, "acct-jobs", &action).unwrap();
            approve_communication_action(&pool, "acct-jobs", &action.id)
                .unwrap()
                .unwrap();
            let lease =
                claim_communication_action(&pool, &format!("processed-{action_provider}-worker"))
                    .unwrap()
                    .unwrap();
            assert_eq!(lease.action.id, action.id);
            let owner = format!("processed-{action_provider}-worker");
            mark_communication_action_request_started(
                &pool,
                &communication_lease_access(&lease, &owner),
            )
            .unwrap()
            .expect("fresh communication request-start authority");
            assert!(matches!(
                finish_communication_action(
                    &pool,
                    &communication_success_finish(
                        &pool,
                        &lease,
                        &owner,
                        &format!("{action_provider}-processed-sent"),
                    ),
                )
                .unwrap()
                .status
                .as_str(),
                "sent"
            ));
        }
    }

    #[test]
    fn communication_actions_are_idempotent_encrypted_and_tenant_scoped() {
        let pool = test_pool();
        pool.get()
            .unwrap()
            .execute(
                "INSERT INTO accounts (id, email, password_hash, trial_seconds_remaining)
                 VALUES ('acct-other', 'other@example.com', 'hash', 0)",
                [],
            )
            .unwrap();
        let (application, mailbox, message) =
            communication_test_application_mailbox_and_message(&pool);
        let action =
            communication_test_action(&application, &mailbox, &message, "reply-interview-1");

        let (stored, inserted) = create_communication_action(&pool, "acct-jobs", &action).unwrap();
        assert!(inserted);
        assert_eq!(stored.status, "awaiting_approval");
        let (replayed, inserted_again) =
            create_communication_action(&pool, "acct-jobs", &action).unwrap();
        assert!(!inserted_again);
        assert_eq!(stored.id, replayed.id);
        assert!(communication_action(&pool, "acct-other", &stored.id)
            .unwrap()
            .is_none());
        assert!(list_communication_actions(&pool, "acct-other", None, 20)
            .unwrap()
            .is_empty());

        let raw: String = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT action_json FROM jobs_communication_actions WHERE id = ?1",
                params![stored.id],
                |row| row.get(0),
            )
            .unwrap();
        assert!(!raw.contains("Tuesday afternoon works for me"));
        assert!(!raw.contains("recruiter@example.org"));

        let mut conflicting = action;
        conflicting.payload["body_text"] = json!("Wednesday morning instead.");
        assert!(create_communication_action(&pool, "acct-jobs", &conflicting).is_err());
    }

    #[test]
    fn communication_payload_hash_matches_shared_portal_vectors() {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../../../jobs/portal/src/fixtures/communication-payload-hash-vectors.json"
        ))
        .unwrap();
        assert_eq!(fixture["schema_version"], 1);
        let vectors = fixture["vectors"].as_array().unwrap();
        assert_eq!(vectors.len(), 2);
        for vector in vectors {
            let kind = vector["kind"].as_str().unwrap();
            let payload = &vector["payload"];
            validate_communication_payload(kind, payload).unwrap();
            let canonical = serde_json::to_string(&canonical_communication_value(payload)).unwrap();
            assert_eq!(canonical, vector["canonical_json"].as_str().unwrap());
            assert_eq!(
                communication_payload_sha256(payload).unwrap(),
                vector["sha256"].as_str().unwrap(),
                "{}",
                vector["name"].as_str().unwrap()
            );
        }
    }

    #[test]
    fn communication_review_text_rejects_invisible_controls_with_body_allowlist() {
        let valid_reply = json!({
            "to": "recruiter@example.org",
            "subject": "Reviewed subject",
            "body_text": "Line one\n\tLine two 👩\u{200d}💻 क्\u{200d}ष",
        });
        validate_communication_payload("reply", &valid_reply).unwrap();

        for unsafe_character in ['\u{202e}', '\u{2066}', '\u{001b}', '\u{0008}'] {
            let mut reply = valid_reply.clone();
            reply["body_text"] = json!(format!("before{unsafe_character}after"));
            assert!(validate_communication_payload("reply", &reply).is_err());
        }
        for unsafe_character in ['\u{202e}', '\u{2066}', '\u{2028}', '\u{2029}'] {
            let mut reply = valid_reply.clone();
            reply["subject"] = json!(format!("before{unsafe_character}after"));
            assert!(validate_communication_payload("reply", &reply).is_err());
            assert!(
                normalize_communication_email(&format!("local{unsafe_character}@example.org"))
                    .is_err()
            );
        }

        let mut calendar = json!({
            "title": "Reviewed interview",
            "starts_at_ms": 2_000_000_000_123_i64,
            "ends_at_ms": 2_000_003_600_123_i64,
            "time_zone": "Europe/Paris",
            "attendees": ["candidate@example.org"],
        });
        calendar["title"] = json!("Interview \u{202e}hidden");
        assert!(validate_communication_payload("calendar", &calendar).is_err());
        assert!(serde_json::from_str::<Value>(r#"{"body_text":"\uD800"}"#).is_err());
    }

    #[test]
    fn communication_replies_require_a_bound_source_message() {
        let pool = test_pool();
        let (application, mailbox, message) =
            communication_test_application_mailbox_and_message(&pool);
        let mut action =
            communication_test_action(&application, &mailbox, &message, "reply-without-source");
        action.source_message_id = None;

        let error = create_communication_action(&pool, "acct-jobs", &action)
            .expect_err("a reply without an inbound provider message must fail");
        assert!(error
            .to_string()
            .contains("communication replies require a source mailbox message"));
    }

    #[test]
    #[serial_test::serial]
    fn communication_execution_flag_fails_closed_for_readiness_and_writes() {
        let _flags = CommunicationFlagGuard::disabled();
        let pool = test_pool();
        let (application, mailbox, message) =
            communication_test_application_mailbox_and_message(&pool);
        let action = communication_test_action(
            &application,
            &mailbox,
            &message,
            "communication-dispatch-disabled",
        );
        let (stored, _) = create_communication_action(&pool, "acct-jobs", &action).unwrap();
        let (ready, reason) =
            communication_action_execution_readiness(&pool, "acct-jobs", &stored).unwrap();
        assert!(!ready);
        assert!(reason.contains("not enabled"));
        assert!(approve_communication_action(&pool, "acct-jobs", &stored.id)
            .unwrap_err()
            .to_string()
            .contains("not enabled"));
        assert!(claim_communication_action(&pool, "disabled-worker")
            .unwrap_err()
            .to_string()
            .contains("not enabled"));
    }

    #[test]
    #[serial_test::serial]
    fn approved_communication_actions_wait_for_mailbox_reauthorization() {
        let _flags = CommunicationFlagGuard::enabled();
        let pool = test_pool();
        let (application, mailbox, message) =
            communication_test_application_mailbox_and_message(&pool);
        let action = communication_test_action(
            &application,
            &mailbox,
            &message,
            "reply-after-reauthorization",
        );
        let (stored, _) = create_communication_action(&pool, "acct-jobs", &action).unwrap();
        approve_communication_action(&pool, "acct-jobs", &stored.id)
            .unwrap()
            .unwrap();

        assert!(mark_mailbox_reauthorization_required(&pool, "acct-jobs", &mailbox.id).unwrap());
        assert!(claim_communication_action(&pool, "mail-worker")
            .unwrap()
            .is_none());

        let reconnected = save_mailbox_connection(
            &pool,
            "acct-jobs",
            &MailboxConnection {
                status: "connected".to_string(),
                ..mailbox.clone()
            },
            "google-subject-communication-action",
        )
        .unwrap();
        assert_eq!(reconnected.id, mailbox.id);

        let lease = claim_communication_action(&pool, "mail-worker")
            .unwrap()
            .expect("the approved reply may dispatch after reconnection");
        assert_eq!(lease.action.id, stored.id);
    }

    #[test]
    #[serial_test::serial]
    fn mailbox_reauthorization_refuses_an_unresolved_dispatch() {
        let _flags = CommunicationFlagGuard::enabled();
        let pool = test_pool();
        let (application, mailbox, message) =
            communication_test_application_mailbox_and_message(&pool);
        let action = communication_test_action(
            &application,
            &mailbox,
            &message,
            "reauthorization-during-dispatch",
        );
        let (stored, _) = create_communication_action(&pool, "acct-jobs", &action).unwrap();
        approve_communication_action(&pool, "acct-jobs", &stored.id)
            .unwrap()
            .unwrap();
        let lease = claim_communication_action(&pool, "mail-worker")
            .unwrap()
            .unwrap();

        let error = mark_mailbox_reauthorization_required(&pool, "acct-jobs", &mailbox.id)
            .expect_err("an unresolved provider attempt must fence mailbox status changes");
        assert!(error.to_string().contains("communication is unresolved"));
        assert_eq!(
            mailbox_connection(&pool, "acct-jobs", &mailbox.id)
                .unwrap()
                .unwrap()
                .status,
            "connected"
        );
        assert_eq!(
            communication_action(&pool, "acct-jobs", &lease.action.id)
                .unwrap()
                .unwrap()
                .status,
            "dispatching"
        );
    }

    #[test]
    #[serial_test::serial]
    fn unresolved_dispatch_rejects_generic_mailbox_reconnect_and_grant_downgrade() {
        let _flags = CommunicationFlagGuard::enabled();
        let pool = test_pool();
        let (application, mailbox, message) =
            communication_test_application_mailbox_and_message(&pool);
        let action = communication_test_action(
            &application,
            &mailbox,
            &message,
            "generic-reconnect-during-dispatch",
        );
        let (stored, _) = create_communication_action(&pool, "acct-jobs", &action).unwrap();
        approve_communication_action(&pool, "acct-jobs", &stored.id)
            .unwrap()
            .unwrap();
        claim_communication_action(&pool, "generic-reconnect-worker")
            .unwrap()
            .unwrap();
        let current = jobs_provider_credential(&pool, "acct-jobs", &mailbox.id)
            .unwrap()
            .unwrap();
        let downgraded_mailbox = MailboxConnection {
            capabilities: vec![
                "status_sync".to_string(),
                "application_correlation".to_string(),
                "review_interventions".to_string(),
            ],
            ..mailbox.clone()
        };
        let downgraded = JobsProviderCredential {
            access_token: "dummy-downgraded-access-token".to_string(),
            refresh_token: "dummy-downgraded-refresh-token".to_string(),
            scopes: vec!["gmail.readonly".to_string()],
            capabilities: downgraded_mailbox.capabilities.clone(),
            grant_revision: 0,
            grant_sha256: String::new(),
            expires_at_ms: now_ms() + 3_600_000,
            ..current.clone()
        };
        assert!(save_mailbox_connection_with_credential(
            &pool,
            "acct-jobs",
            &downgraded_mailbox,
            &downgraded,
        )
        .unwrap_err()
        .to_string()
        .contains("communication is unresolved"));
        assert!(
            save_jobs_provider_credential(&pool, "acct-jobs", &downgraded)
                .unwrap_err()
                .to_string()
                .contains("communication is unresolved")
        );
        let preserved = jobs_provider_credential(&pool, "acct-jobs", &mailbox.id)
            .unwrap()
            .unwrap();
        assert_eq!(preserved.grant_revision, current.grant_revision);
        assert_eq!(preserved.grant_sha256, current.grant_sha256);
        assert_eq!(preserved.refresh_token, current.refresh_token);
        assert_eq!(
            mailbox_connection(&pool, "acct-jobs", &mailbox.id)
                .unwrap()
                .unwrap()
                .capabilities,
            mailbox.capabilities
        );
    }

    #[test]
    #[serial_test::serial]
    fn mailbox_reauthorization_and_dispatch_claim_have_one_consistent_winner() {
        let _flags = CommunicationFlagGuard::enabled();
        let pool = test_pool();
        let (application, mailbox, message) =
            communication_test_application_mailbox_and_message(&pool);
        let action = communication_test_action(
            &application,
            &mailbox,
            &message,
            "reauthorization-dispatch-race",
        );
        let (stored, _) = create_communication_action(&pool, "acct-jobs", &action).unwrap();
        approve_communication_action(&pool, "acct-jobs", &stored.id)
            .unwrap()
            .unwrap();
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(3));
        let claim_pool = pool.clone();
        let claim_barrier = barrier.clone();
        let claim = std::thread::spawn(move || {
            claim_barrier.wait();
            claim_communication_action(&claim_pool, "race-mail-worker")
        });
        let mark_pool = pool.clone();
        let mark_barrier = barrier.clone();
        let connection_id = mailbox.id.clone();
        let mark = std::thread::spawn(move || {
            mark_barrier.wait();
            mark_mailbox_reauthorization_required(&mark_pool, "acct-jobs", &connection_id)
        });
        barrier.wait();
        let claimed = claim.join().unwrap();
        let marked = mark.join().unwrap();
        let final_mailbox = mailbox_connection(&pool, "acct-jobs", &mailbox.id)
            .unwrap()
            .unwrap();
        let final_action = communication_action(&pool, "acct-jobs", &stored.id)
            .unwrap()
            .unwrap();

        match (claimed, marked) {
            (Ok(Some(_)), Err(error)) => {
                assert!(error.to_string().contains("communication is unresolved"));
                assert_eq!(final_mailbox.status, "connected");
                assert_eq!(final_action.status, "dispatching");
            }
            (Ok(None), Ok(true)) => {
                assert_eq!(final_mailbox.status, "reauthorization_required");
                assert_eq!(final_action.status, "approved");
            }
            (claimed, marked) => {
                panic!("inconsistent dispatch/reauthorization race: {claimed:?}, {marked:?}")
            }
        }
    }

    #[test]
    fn outlook_email_and_calendar_actions_use_the_outlook_mailbox_connection() {
        let pool = test_pool();
        let application = communication_test_application(&pool);
        let mailbox = save_mailbox_connection(
            &pool,
            "acct-jobs",
            &MailboxConnection {
                id: String::new(),
                provider: "outlook".to_string(),
                status: "connected".to_string(),
                account_label: "candidate@outlook.com".to_string(),
                aliases: Vec::new(),
                capabilities: vec![
                    "recruiter_reply".to_string(),
                    "interview_calendar".to_string(),
                ],
                created_at_ms: 0,
                updated_at_ms: 0,
            },
            "microsoft-subject-communication-action",
        )
        .unwrap();
        save_jobs_provider_credential(
            &pool,
            "acct-jobs",
            &JobsProviderCredential {
                connection_id: mailbox.id.clone(),
                provider: "outlook".to_string(),
                provider_subject: "microsoft-subject-communication-action".to_string(),
                access_token: "dummy-outlook-access-token".to_string(),
                refresh_token: "dummy-outlook-refresh-token".to_string(),
                scopes: vec!["Mail.Send".to_string(), "Calendars.ReadWrite".to_string()],
                capabilities: mailbox.capabilities.clone(),
                grant_revision: 1,
                grant_sha256: String::new(),
                expires_at_ms: now_ms() + 3_600_000,
                created_at_ms: now_ms(),
                updated_at_ms: now_ms(),
            },
        )
        .unwrap();
        let message = communication_test_source_message(
            &pool,
            &application,
            &mailbox,
            "outlook-message-communication-action",
        );

        let mut reply =
            communication_test_action(&application, &mailbox, &message, "outlook-reply-1");
        reply.provider = "outlook_email".to_string();
        let (stored_reply, inserted_reply) =
            create_communication_action(&pool, "acct-jobs", &reply).unwrap();
        assert!(inserted_reply);
        assert_eq!(stored_reply.provider, "outlook_email");

        let mut calendar =
            communication_test_action(&application, &mailbox, &message, "outlook-event-1");
        calendar.kind = "calendar".to_string();
        calendar.provider = "outlook_calendar".to_string();
        calendar.source_message_id = None;
        calendar.payload = json!({
            "title": "Interview with Acme",
            "starts_at_ms": 2_000_000_000_000_i64,
            "ends_at_ms": 2_000_003_600_000_i64,
            "time_zone": "America/New_York",
            "attendees": ["candidate@outlook.com", "recruiter@example.org"]
        });
        let (stored_calendar, inserted_calendar) =
            create_communication_action(&pool, "acct-jobs", &calendar).unwrap();
        assert!(inserted_calendar);
        assert_eq!(stored_calendar.provider, "outlook_calendar");

        let reply_to_message = save_provider_message(
            &pool,
            "acct-jobs",
            &JobsProviderMessage {
                id: String::new(),
                connection_id: mailbox.id.clone(),
                provider: "outlook".to_string(),
                external_id: "<outlook-reply-to@example.org>".to_string(),
                sender: "sender@example.org".to_string(),
                recipients: vec![mailbox.account_label.clone()],
                subject: "Reply-To authority".to_string(),
                body_text: "Please reply to our recruiting team.".to_string(),
                received_at_ms: now_ms(),
                application_id: Some(application.id.clone()),
                processing_status: "needs_input".to_string(),
                classification: "interview".to_string(),
                confidence: 0.99,
                metadata: json!({
                    "provider_id": "immutable-outlook-reply-to",
                    "conversation_id": "outlook-reply-to-conversation",
                    "rfc_message_id": "<outlook-reply-to@example.org>",
                    "reply_target": "talent@example.org",
                }),
                processed_at_ms: None,
                created_at_ms: 0,
                updated_at_ms: 0,
            },
        )
        .unwrap()
        .0;
        let mut reply_to_action = communication_test_action(
            &application,
            &mailbox,
            &reply_to_message,
            "outlook-reply-to-authority",
        );
        reply_to_action.provider = "outlook_email".to_string();
        reply_to_action.payload["to"] = json!("talent@example.org");
        assert!(
            create_communication_action(&pool, "acct-jobs", &reply_to_action)
                .unwrap()
                .1
        );

        let mut sender_action = reply_to_action;
        sender_action.idempotency_key = "outlook-sender-is-not-reply-target".to_string();
        sender_action.payload["to"] = json!("sender@example.org");
        assert!(create_communication_action(&pool, "acct-jobs", &sender_action).is_err());
    }

    #[test]
    #[serial_test::serial]
    fn communication_actions_require_approval_and_fenced_provider_evidence() {
        let _flags = CommunicationFlagGuard::enabled();
        let pool = test_pool();
        let (application, mailbox, message) =
            communication_test_application_mailbox_and_message(&pool);
        let action =
            communication_test_action(&application, &mailbox, &message, "reply-interview-2");
        let (stored, _) = create_communication_action(&pool, "acct-jobs", &action).unwrap();

        assert!(claim_communication_action(&pool, "mail-worker")
            .unwrap()
            .is_none());
        let approved = approve_communication_action(&pool, "acct-jobs", &stored.id)
            .unwrap()
            .unwrap();
        assert_eq!(approved.status, "approved");
        assert!(approved.approved_at_ms.is_some());

        let lease = claim_communication_action(&pool, "mail-worker")
            .unwrap()
            .unwrap();
        assert_eq!(lease.action.id, stored.id);
        assert_eq!(lease.action.status, "dispatching");
        assert_eq!(lease.action.attempt_count, 1);
        let access = communication_lease_access(&lease, "mail-worker");
        mark_communication_action_request_started(&pool, &access)
            .unwrap()
            .expect("fresh communication request-start authority");
        let mut wrong_token =
            communication_success_finish(&pool, &lease, "mail-worker", "gmail-message-1");
        wrong_token.lease.lease_token = "wrong-token".to_string();
        assert!(finish_communication_action(&pool, &wrong_token).is_err());
        let missing_object = communication_success_finish(&pool, &lease, "mail-worker", "");
        assert!(finish_communication_action(&pool, &missing_object).is_err());
        let mut missing_source_proof =
            communication_success_finish(&pool, &lease, "mail-worker", "gmail-message-1");
        missing_source_proof
            .evidence
            .as_object_mut()
            .unwrap()
            .remove("thread_sha256");
        assert!(finish_communication_action(&pool, &missing_source_proof).is_err());
        let mut mismatched_source_proof =
            communication_success_finish(&pool, &lease, "mail-worker", "gmail-message-1");
        mismatched_source_proof.evidence["thread_sha256"] = json!("f".repeat(64));
        assert!(finish_communication_action(&pool, &mismatched_source_proof).is_err());

        let completed = finish_communication_action(
            &pool,
            &communication_success_finish(&pool, &lease, "mail-worker", "gmail-message-1"),
        )
        .unwrap();
        assert_eq!(completed.status, "sent");
        assert_eq!(completed.provider_object_id, "gmail-message-1");
        assert!(completed.lease_owner.is_none());
        assert!(completed.lease_expires_at_ms.is_none());
        let lease_secret: Option<String> = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT lease_token_sha256 FROM jobs_communication_actions WHERE id = ?1",
                params![stored.id],
                |row| row.get(0),
            )
            .unwrap();
        assert!(lease_secret.is_none());
    }

    #[test]
    #[serial_test::serial]
    fn communication_request_start_is_singleton_and_replay_exact() {
        let _flags = CommunicationFlagGuard::enabled();
        let pool = test_pool();
        let (application, mailbox, message) =
            communication_test_application_mailbox_and_message(&pool);
        let action =
            communication_test_action(&application, &mailbox, &message, "request-start-singleton");
        let (stored, _) = create_communication_action(&pool, "acct-jobs", &action).unwrap();
        approve_communication_action(&pool, "acct-jobs", &stored.id)
            .unwrap()
            .unwrap();
        let lease = claim_communication_action(&pool, "request-start-worker")
            .unwrap()
            .unwrap();
        let access = communication_lease_access(&lease, "request-start-worker");
        mark_communication_action_request_started(&pool, &access)
            .unwrap()
            .expect("fresh communication request-start authority");
        mark_communication_action_request_started(&pool, &access)
            .unwrap()
            .expect("exact communication request-start replay");
        let count: i64 = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM jobs_communication_action_attempt_evidence
                  WHERE attempt_id = ?1 AND event_kind = 'request_started'",
                params![lease.attempt_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);

        pool.get()
            .unwrap()
            .execute(
                "UPDATE jobs_communication_action_attempt_evidence
                    SET evidence_sha256 = ?2
                  WHERE attempt_id = ?1 AND event_kind = 'request_started'",
                params![lease.attempt_id, "f".repeat(64)],
            )
            .unwrap();
        assert!(mark_communication_action_request_started(&pool, &access)
            .unwrap_err()
            .to_string()
            .contains("evidence changed"));
        let count: i64 = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM jobs_communication_action_attempt_evidence
                  WHERE attempt_id = ?1 AND event_kind = 'request_started'",
                params![lease.attempt_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    #[serial_test::serial]
    fn communication_request_start_replay_and_completion_survive_later_matching_hold() {
        let _flags = CommunicationFlagGuard::enabled();
        let pool = test_pool();
        let (application, mailbox, message) =
            communication_test_application_mailbox_and_message(&pool);
        let action = communication_test_action(
            &application,
            &mailbox,
            &message,
            "request-start-later-operational-hold",
        );
        let (stored, _) = create_communication_action(&pool, "acct-jobs", &action).unwrap();
        approve_communication_action(&pool, "acct-jobs", &stored.id)
            .unwrap()
            .unwrap();
        let lease = claim_communication_action(&pool, "request-start-hold-worker")
            .unwrap()
            .unwrap();
        let access = communication_lease_access(&lease, "request-start-hold-worker");
        let started = mark_communication_action_request_started(&pool, &access)
            .unwrap()
            .expect("fresh communication request-start authority");

        append_operational_hold_for_scope(
            &pool,
            OperationalCapability::CommunicationDispatch,
            OperationalHoldScopeKind::MailboxProvider,
            "gmail",
            "communication-request-start-later-hold",
            OperationalHoldTransition::Held,
            0,
            None,
        );
        {
            let mut conn = pool.get().unwrap();
            let tx = conn.transaction().unwrap();
            let context =
                operational_hold_context_for_mailbox_sqlite_tx(&tx, "acct-jobs", &mailbox.id)
                    .unwrap();
            assert!(matches!(
                evaluate_operational_capability_sqlite_tx(
                    &tx,
                    OperationalCapability::CommunicationDispatch,
                    &context,
                )
                .unwrap(),
                OperationalCapabilityEvaluation::Held(_)
            ));
            tx.commit().unwrap();
        }

        let replayed = mark_communication_action_request_started(&pool, &access)
            .unwrap()
            .expect("exact communication request-start replay");
        assert_eq!(replayed, started);
        let request_started_count: i64 = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM jobs_communication_action_attempt_evidence
                  WHERE attempt_id = ?1 AND event_kind = 'request_started'",
                params![lease.attempt_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(request_started_count, 1);

        let completed = finish_communication_action(
            &pool,
            &communication_success_finish(
                &pool,
                &lease,
                "request-start-hold-worker",
                "gmail-message-after-operational-hold",
            ),
        )
        .unwrap();
        assert_eq!(completed.status, "sent");
        assert_eq!(
            completed.provider_object_id,
            "gmail-message-after-operational-hold"
        );
        let completion_count: i64 = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM jobs_communication_action_attempt_evidence
                  WHERE attempt_id = ?1 AND event_kind = 'sent'",
                params![lease.attempt_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(completion_count, 1);
    }

    #[test]
    #[serial_test::serial]
    fn communication_dispatch_hold_skips_held_candidate_for_later_allowed_mailbox() {
        let _flags = CommunicationFlagGuard::enabled();
        let pool = test_pool();
        set_entitlement_plan(&pool, "acct-jobs", "pro").unwrap();
        let (application, gmail, gmail_message) =
            communication_test_application_mailbox_and_message(&pool);
        let outlook = save_mailbox_connection(
            &pool,
            "acct-jobs",
            &MailboxConnection {
                id: String::new(),
                provider: "outlook".to_string(),
                status: "connected".to_string(),
                account_label: "outlook@example.com".to_string(),
                aliases: Vec::new(),
                capabilities: vec!["recruiter_reply".to_string()],
                created_at_ms: 0,
                updated_at_ms: 0,
            },
            "microsoft-subject-communication-dispatch-hold",
        )
        .unwrap();
        save_jobs_provider_credential(
            &pool,
            "acct-jobs",
            &JobsProviderCredential {
                connection_id: outlook.id.clone(),
                provider: "outlook".to_string(),
                provider_subject: "microsoft-subject-communication-dispatch-hold".to_string(),
                access_token: "dummy-outlook-dispatch-hold-access-token".to_string(),
                refresh_token: "dummy-outlook-dispatch-hold-refresh-token".to_string(),
                scopes: vec!["Mail.Send".to_string()],
                capabilities: vec!["recruiter_reply".to_string()],
                grant_revision: 1,
                grant_sha256: String::new(),
                expires_at_ms: now_ms() + 3_600_000,
                created_at_ms: now_ms(),
                updated_at_ms: now_ms(),
            },
        )
        .unwrap();
        let outlook_message = communication_test_source_message(
            &pool,
            &application,
            &outlook,
            "outlook-message-communication-dispatch-hold",
        );

        let gmail_action = communication_test_action(
            &application,
            &gmail,
            &gmail_message,
            "held-gmail-communication-dispatch",
        );
        let (gmail_action, _) =
            create_communication_action(&pool, "acct-jobs", &gmail_action).unwrap();
        let gmail_action = approve_communication_action(&pool, "acct-jobs", &gmail_action.id)
            .unwrap()
            .unwrap();
        let mut outlook_action = communication_test_action(
            &application,
            &outlook,
            &outlook_message,
            "allowed-outlook-communication-dispatch",
        );
        outlook_action.provider = "outlook_email".to_string();
        let (outlook_action, _) =
            create_communication_action(&pool, "acct-jobs", &outlook_action).unwrap();
        let outlook_action = approve_communication_action(&pool, "acct-jobs", &outlook_action.id)
            .unwrap()
            .unwrap();
        let due_now = now_ms();
        pool.get()
            .unwrap()
            .execute(
                "UPDATE jobs_communication_actions
                    SET next_attempt_at_ms = CASE id
                      WHEN ?1 THEN ?3 - 2 ELSE ?3 - 1 END
                  WHERE account_id = 'acct-jobs' AND id IN (?1, ?2)",
                params![gmail_action.id, outlook_action.id, due_now],
            )
            .unwrap();
        append_operational_hold_for_scope(
            &pool,
            OperationalCapability::CommunicationDispatch,
            OperationalHoldScopeKind::MailboxProvider,
            "gmail",
            "held-communication-dispatch-mailbox-1",
            OperationalHoldTransition::Held,
            0,
            None,
        );

        let lease = claim_communication_action(&pool, "communication-hold-skip-worker")
            .unwrap()
            .unwrap();
        assert_eq!(lease.action.id, outlook_action.id);
        assert_eq!(lease.action.provider, "outlook_email");
        let held = communication_action(&pool, "acct-jobs", &gmail_action.id)
            .unwrap()
            .unwrap();
        assert_eq!(held.status, "approved");
        assert_eq!(held.attempt_count, 0);
        assert!(held.active_attempt_id.is_none());
        assert!(held.lease_owner.is_none());
    }

    #[test]
    #[serial_test::serial]
    fn communication_held_candidate_scan_is_bounded_and_advances_on_the_next_call() {
        let _flags = CommunicationFlagGuard::enabled();
        let pool = test_pool();
        set_entitlement_plan(&pool, "acct-jobs", "pro").unwrap();
        let (application, gmail, gmail_message) =
            communication_test_application_mailbox_and_message(&pool);
        let outlook = save_mailbox_connection(
            &pool,
            "acct-jobs",
            &MailboxConnection {
                id: String::new(),
                provider: "outlook".to_string(),
                status: "connected".to_string(),
                account_label: "bounded-outlook@example.com".to_string(),
                aliases: Vec::new(),
                capabilities: vec!["recruiter_reply".to_string()],
                created_at_ms: 0,
                updated_at_ms: 0,
            },
            "microsoft-subject-bounded-communication-dispatch",
        )
        .unwrap();
        save_jobs_provider_credential(
            &pool,
            "acct-jobs",
            &JobsProviderCredential {
                connection_id: outlook.id.clone(),
                provider: "outlook".to_string(),
                provider_subject: "microsoft-subject-bounded-communication-dispatch".to_string(),
                access_token: "dummy-bounded-outlook-dispatch-access-token".to_string(),
                refresh_token: "dummy-bounded-outlook-dispatch-refresh-token".to_string(),
                scopes: vec!["Mail.Send".to_string()],
                capabilities: vec!["recruiter_reply".to_string()],
                grant_revision: 1,
                grant_sha256: String::new(),
                expires_at_ms: now_ms() + 3_600_000,
                created_at_ms: now_ms(),
                updated_at_ms: now_ms(),
            },
        )
        .unwrap();
        let outlook_message = communication_test_source_message(
            &pool,
            &application,
            &outlook,
            "bounded-outlook-communication-message",
        );

        let mut held_actions = Vec::new();
        for index in 0..OPERATIONAL_HOLD_CANDIDATE_SCAN_LIMIT {
            let action = communication_test_action(
                &application,
                &gmail,
                &gmail_message,
                &format!("bounded-held-gmail-{index:02}"),
            );
            let (action, _) = create_communication_action(&pool, "acct-jobs", &action).unwrap();
            held_actions.push(
                approve_communication_action(&pool, "acct-jobs", &action.id)
                    .unwrap()
                    .unwrap(),
            );
        }
        let mut allowed_action = communication_test_action(
            &application,
            &outlook,
            &outlook_message,
            "bounded-allowed-outlook",
        );
        allowed_action.provider = "outlook_email".to_string();
        let (allowed_action, _) =
            create_communication_action(&pool, "acct-jobs", &allowed_action).unwrap();
        let allowed_action = approve_communication_action(&pool, "acct-jobs", &allowed_action.id)
            .unwrap()
            .unwrap();
        let schedule = now_ms().saturating_sub(100_000);
        let conn = pool.get().unwrap();
        for (index, action) in held_actions.iter().enumerate() {
            conn.execute(
                "UPDATE jobs_communication_actions SET next_attempt_at_ms = ?2 WHERE id = ?1",
                params![action.id, schedule + index as i64],
            )
            .unwrap();
        }
        conn.execute(
            "UPDATE jobs_communication_actions SET next_attempt_at_ms = ?2 WHERE id = ?1",
            params![
                allowed_action.id,
                schedule + OPERATIONAL_HOLD_CANDIDATE_SCAN_LIMIT as i64
            ],
        )
        .unwrap();
        drop(conn);
        append_operational_hold_for_scope(
            &pool,
            OperationalCapability::CommunicationDispatch,
            OperationalHoldScopeKind::MailboxProvider,
            "gmail",
            "bounded-held-communication-mailbox",
            OperationalHoldTransition::Held,
            0,
            None,
        );

        let owner_id = "bounded-communication-held-scan-worker";
        assert!(claim_communication_action(&pool, owner_id)
            .unwrap()
            .is_none());
        let lease = claim_communication_action(&pool, owner_id)
            .unwrap()
            .unwrap();
        assert_eq!(lease.action.id, allowed_action.id);
        assert!(held_actions.iter().all(|action| {
            let action = communication_action(&pool, "acct-jobs", &action.id)
                .unwrap()
                .unwrap();
            action.status == "approved"
                && action.attempt_count == 0
                && action.active_attempt_id.is_none()
                && action.lease_owner.is_none()
        }));
    }

    #[test]
    #[serial_test::serial]
    fn communication_dispatch_hold_after_claim_demotes_without_request_started_evidence() {
        let _flags = CommunicationFlagGuard::enabled();
        let pool = test_pool();
        let (application, mailbox, message) =
            communication_test_application_mailbox_and_message(&pool);
        let action = communication_test_action(
            &application,
            &mailbox,
            &message,
            "communication-dispatch-hold-after-claim",
        );
        let (action, _) = create_communication_action(&pool, "acct-jobs", &action).unwrap();
        approve_communication_action(&pool, "acct-jobs", &action.id)
            .unwrap()
            .unwrap();
        let lease = claim_communication_action(&pool, "communication-hold-race-worker")
            .unwrap()
            .unwrap();
        append_operational_hold_for_scope(
            &pool,
            OperationalCapability::CommunicationDispatch,
            OperationalHoldScopeKind::MailboxProvider,
            "gmail",
            "held-communication-dispatch-after-claim-1",
            OperationalHoldTransition::Held,
            0,
            None,
        );

        let request_started = mark_communication_action_request_started(
            &pool,
            &communication_lease_access(&lease, "communication-hold-race-worker"),
        )
        .unwrap();
        assert!(request_started.is_none());
        let demoted = communication_action(&pool, "acct-jobs", &action.id)
            .unwrap()
            .unwrap();
        assert_eq!(demoted.status, "needs_input");
        assert_eq!(demoted.action_revision, lease.action.action_revision + 1);
        assert!(demoted.lease_owner.is_none());
        assert!(demoted.lease_kind.is_none());
        assert!(demoted.lease_expires_at_ms.is_none());
        assert!(demoted.active_attempt_id.is_none());
        assert!(demoted.approved_authority_sha256.is_empty());
        assert_eq!(demoted.approved_grant_revision, 0);
        assert!(demoted.approved_grant_sha256.is_empty());
        assert!(demoted.approved_at_ms.is_none());
        let (lease_token_sha256, evidence_count): (Option<String>, i64) = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT action.lease_token_sha256,
                        (SELECT COUNT(*)
                           FROM jobs_communication_action_attempt_evidence evidence
                          WHERE evidence.account_id = action.account_id
                            AND evidence.action_id = action.id
                            AND evidence.attempt_id = ?2
                            AND evidence.event_kind = 'request_started')
                   FROM jobs_communication_actions action
                  WHERE action.account_id = 'acct-jobs' AND action.id = ?1",
                params![action.id, lease.attempt_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert!(lease_token_sha256.is_none());
        assert_eq!(evidence_count, 0);
    }

    #[test]
    #[serial_test::serial]
    fn communication_audit_schema_rejects_cross_authority_rows() {
        let _flags = CommunicationFlagGuard::enabled();
        let pool = test_pool();
        set_entitlement_plan(&pool, "acct-jobs", "pro").unwrap();
        pool.get()
            .unwrap()
            .execute(
                "INSERT INTO accounts (id, email, password_hash, trial_seconds_remaining)
                 VALUES ('acct-other', 'other@example.com', 'hash', 0)",
                [],
            )
            .unwrap();
        let (application, mailbox, message) =
            communication_test_application_mailbox_and_message(&pool);
        let first =
            communication_test_action(&application, &mailbox, &message, "audit-authority-first");
        let (first, _) = create_communication_action(&pool, "acct-jobs", &first).unwrap();
        let second =
            communication_test_action(&application, &mailbox, &message, "audit-authority-second");
        let (second, _) = create_communication_action(&pool, "acct-jobs", &second).unwrap();
        approve_communication_action(&pool, "acct-jobs", &first.id)
            .unwrap()
            .unwrap();
        let lease = claim_communication_action(&pool, "audit-authority-worker")
            .unwrap()
            .unwrap();
        let other_mailbox = save_mailbox_connection(
            &pool,
            "acct-jobs",
            &MailboxConnection {
                id: String::new(),
                provider: "gmail".to_string(),
                status: "connected".to_string(),
                account_label: "other-mailbox@example.com".to_string(),
                aliases: Vec::new(),
                capabilities: Vec::new(),
                created_at_ms: 0,
                updated_at_ms: 0,
            },
            "google-subject-audit-other-mailbox",
        )
        .unwrap();
        let other_source = communication_test_source_message(
            &pool,
            &application,
            &other_mailbox,
            "audit-authority-other-source",
        );
        let conn = pool.get().unwrap();
        assert!(conn
            .execute(
                "UPDATE jobs_communication_actions SET account_id = 'acct-other' WHERE id = ?1",
                params![second.id],
            )
            .is_err());
        assert!(conn
            .execute(
                "UPDATE jobs_communication_actions SET connection_id = ?2 WHERE id = ?1",
                params![second.id, other_mailbox.id],
            )
            .is_err());
        assert!(conn
            .execute(
                "UPDATE jobs_communication_actions SET source_message_id = ?2 WHERE id = ?1",
                params![second.id, other_source.id],
            )
            .is_err());
        assert!(conn
            .execute(
                "UPDATE jobs_communication_actions SET provider = 'google_calendar' WHERE id = ?1",
                params![second.id],
            )
            .is_err());
        let wrong_connection = conn.execute(
            "INSERT INTO jobs_communication_action_attempts (
                id, account_id, action_id, connection_id, provider, dispatch_no, fence,
                approval_revision, authority_sha256, grant_revision, grant_sha256,
                provider_operation_key, created_at_ms
             ) VALUES (?1, 'acct-jobs', ?2, ?3, 'gmail', 99, 99, 1, ?4, 1, ?5, ?6, ?7)",
            params![
                uuid::Uuid::new_v4().to_string(),
                first.id,
                other_mailbox.id,
                first.authority_sha256,
                lease.grant_sha256,
                "bluey-cross-connection-operation",
                now_ms(),
            ],
        );
        assert!(wrong_connection.is_err());
        let wrong_account = conn.execute(
            "INSERT INTO jobs_communication_action_attempts (
                id, account_id, action_id, connection_id, provider, dispatch_no, fence,
                approval_revision, authority_sha256, grant_revision, grant_sha256,
                provider_operation_key, created_at_ms
             ) VALUES (?1, 'acct-other', ?2, ?3, 'gmail', 100, 100, 1, ?4, 1, ?5, ?6, ?7)",
            params![
                uuid::Uuid::new_v4().to_string(),
                first.id,
                mailbox.id,
                first.authority_sha256,
                lease.grant_sha256,
                "bluey-cross-account-operation",
                now_ms(),
            ],
        );
        assert!(wrong_account.is_err());
        let wrong_provider = conn.execute(
            "INSERT INTO jobs_communication_action_attempts (
                id, account_id, action_id, connection_id, provider, dispatch_no, fence,
                approval_revision, authority_sha256, grant_revision, grant_sha256,
                provider_operation_key, created_at_ms
             ) VALUES (?1, 'acct-jobs', ?2, ?3, 'outlook_email', 101, 101, 1, ?4, 1, ?5, ?6, ?7)",
            params![
                uuid::Uuid::new_v4().to_string(),
                first.id,
                mailbox.id,
                first.authority_sha256,
                lease.grant_sha256,
                "bluey-cross-provider-operation",
                now_ms(),
            ],
        );
        assert!(wrong_provider.is_err());
        assert!(conn
            .execute(
                "UPDATE jobs_communication_actions
                    SET status = 'sent', provider_object_id = ?2
                  WHERE id = ?1",
                params![second.id, "provider\u{0085}object"],
            )
            .is_err());
        let cross_action_evidence = conn.execute(
            "INSERT INTO jobs_communication_action_attempt_evidence (
                id, account_id, action_id, attempt_id, event_kind, evidence_sha256,
                evidence_json, recorded_at_ms
             ) VALUES (?1, 'acct-jobs', ?2, ?3, 'request_started', ?4, 'encrypted', ?5)",
            params![
                uuid::Uuid::new_v4().to_string(),
                second.id,
                lease.attempt_id,
                "a".repeat(64),
                now_ms(),
            ],
        );
        assert!(cross_action_evidence.is_err());
        let cross_account_reconciliation = conn.execute(
            "INSERT INTO jobs_communication_action_reconciliations (
                id, account_id, action_id, attempt_id, fence, resolution,
                evidence_sha256, evidence_json, recorded_at_ms
             ) VALUES (?1, 'acct-other', ?2, ?3, 101, 'inconclusive', ?4, 'encrypted', ?5)",
            params![
                uuid::Uuid::new_v4().to_string(),
                first.id,
                lease.attempt_id,
                "b".repeat(64),
                now_ms(),
            ],
        );
        assert!(cross_account_reconciliation.is_err());
    }

    #[test]
    #[serial_test::serial]
    fn account_deletion_fence_stops_new_dispatch_and_allows_exact_finish() {
        let _flags = CommunicationFlagGuard::enabled();
        let pool = test_pool();
        let (application, mailbox, message) =
            communication_test_application_mailbox_and_message(&pool);
        for idempotency_key in ["account-drain-first", "account-drain-second"] {
            let action =
                communication_test_action(&application, &mailbox, &message, idempotency_key);
            let (action, _) = create_communication_action(&pool, "acct-jobs", &action).unwrap();
            approve_communication_action(&pool, "acct-jobs", &action.id)
                .unwrap()
                .unwrap();
        }
        let lease = claim_communication_action(&pool, "account-drain-worker")
            .unwrap()
            .unwrap();
        mark_communication_action_request_started(
            &pool,
            &communication_lease_access(&lease, "account-drain-worker"),
        )
        .unwrap()
        .expect("fresh communication request-start authority");

        assert!(matches!(
            crate::db::account_data::begin_account_deletion(&pool, "acct-jobs", now_ms())
                .unwrap(),
            Some(
                crate::db::account_data::BeginAccountDeletionResult::WaitingForIrreversibleCommunications {
                    active_actions: 1
                }
            )
        ));
        let fence_count: i64 = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM jobs_communication_write_fences
                  WHERE account_id = 'acct-jobs' AND connection_id = ''
                    AND reason = 'account_deletion'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(fence_count, 1);
        assert!(claim_communication_action(&pool, "blocked-account-worker")
            .unwrap()
            .is_none());

        let finished = finish_communication_action(
            &pool,
            &communication_success_finish(
                &pool,
                &lease,
                "account-drain-worker",
                "gmail-account-drain-sent",
            ),
        )
        .unwrap();
        assert_eq!(finished.status, "sent");
    }

    #[test]
    #[serial_test::serial]
    fn account_deletion_and_request_start_race_preserves_one_serialized_authority() {
        let _flags = CommunicationFlagGuard::enabled();
        let pool = test_pool();
        let (application, mailbox, message) =
            communication_test_application_mailbox_and_message(&pool);
        let action = communication_test_action(
            &application,
            &mailbox,
            &message,
            "account-drain-request-start-race",
        );
        let (action, _) = create_communication_action(&pool, "acct-jobs", &action).unwrap();
        approve_communication_action(&pool, "acct-jobs", &action.id)
            .unwrap()
            .unwrap();

        let barrier = std::sync::Arc::new(std::sync::Barrier::new(3));
        let claim_pool = pool.clone();
        let claim_barrier = std::sync::Arc::clone(&barrier);
        let claim = std::thread::spawn(move || {
            claim_barrier.wait();
            let lease = claim_communication_action(&claim_pool, "account-drain-race-worker")
                .ok()
                .flatten();
            let started = lease.as_ref().is_some_and(|lease| {
                mark_communication_action_request_started(
                    &claim_pool,
                    &communication_lease_access(lease, "account-drain-race-worker"),
                )
                .ok()
                .flatten()
                .is_some()
            });
            (lease, started)
        });
        let deletion_pool = pool.clone();
        let deletion_barrier = std::sync::Arc::clone(&barrier);
        let deletion = std::thread::spawn(move || {
            deletion_barrier.wait();
            crate::db::account_data::begin_account_deletion(&deletion_pool, "acct-jobs", now_ms())
        });
        barrier.wait();
        let (lease, request_started) = claim.join().unwrap();
        let deletion = deletion.join().unwrap().unwrap().unwrap();

        let fence_count: i64 = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM jobs_communication_write_fences
                  WHERE account_id = 'acct-jobs' AND connection_id = ''",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(fence_count, 1);
        assert!(claim_communication_action(&pool, "post-race-worker")
            .unwrap_or(None)
            .is_none());
        if lease.is_some() {
            assert!(matches!(
                deletion,
                crate::db::account_data::BeginAccountDeletionResult::WaitingForIrreversibleCommunications {
                    active_actions: 1
                }
            ));
        } else {
            assert!(!request_started);
            assert!(matches!(
                deletion,
                crate::db::account_data::BeginAccountDeletionResult::Ready(_)
                    | crate::db::account_data::BeginAccountDeletionResult::WaitingForUploads(_)
            ));
        }
        if request_started {
            let lease = lease.unwrap();
            assert_eq!(
                finish_communication_action(
                    &pool,
                    &communication_success_finish(
                        &pool,
                        &lease,
                        "account-drain-race-worker",
                        "gmail-account-drain-race-sent",
                    ),
                )
                .unwrap()
                .status,
                "sent"
            );
        }
    }

    #[test]
    #[serial_test::serial]
    fn mailbox_disconnect_fence_drains_exact_attempt_then_cancels_pending_action() {
        let _flags = CommunicationFlagGuard::enabled();
        let pool = test_pool();
        let (application, mailbox, message) =
            communication_test_application_mailbox_and_message(&pool);
        let mut actions = Vec::new();
        for idempotency_key in ["mailbox-drain-first", "mailbox-drain-second"] {
            let action =
                communication_test_action(&application, &mailbox, &message, idempotency_key);
            let (action, _) = create_communication_action(&pool, "acct-jobs", &action).unwrap();
            actions.push(
                approve_communication_action(&pool, "acct-jobs", &action.id)
                    .unwrap()
                    .unwrap(),
            );
        }
        let lease = claim_communication_action(&pool, "mailbox-drain-worker")
            .unwrap()
            .unwrap();
        mark_communication_action_request_started(
            &pool,
            &communication_lease_access(&lease, "mailbox-drain-worker"),
        )
        .unwrap()
        .expect("fresh communication request-start authority");
        let pending = actions
            .iter()
            .find(|action| action.id != lease.action.id)
            .unwrap()
            .clone();

        assert!(delete_mailbox_connection(&pool, "acct-jobs", &mailbox.id).is_err());
        let fence_count: i64 = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM jobs_communication_write_fences
                  WHERE account_id = 'acct-jobs' AND connection_id = ?1
                    AND reason = 'mailbox_disconnect'",
                params![mailbox.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(fence_count, 1);
        assert!(claim_communication_action(&pool, "blocked-mailbox-worker")
            .unwrap()
            .is_none());
        assert_eq!(
            finish_communication_action(
                &pool,
                &communication_success_finish(
                    &pool,
                    &lease,
                    "mailbox-drain-worker",
                    "gmail-mailbox-drain-sent",
                ),
            )
            .unwrap()
            .status,
            "sent"
        );

        assert!(delete_mailbox_connection(&pool, "acct-jobs", &mailbox.id).unwrap());
        let cancelled = communication_action(&pool, "acct-jobs", &pending.id)
            .unwrap()
            .unwrap();
        assert_eq!(cancelled.status, "cancelled");
        assert_eq!(cancelled.action_revision, pending.action_revision + 1);

        let mut reconnected = mailbox.clone();
        reconnected.status = "connected".to_string();
        save_mailbox_connection_with_credential(
            &pool,
            "acct-jobs",
            &reconnected,
            &JobsProviderCredential {
                connection_id: mailbox.id.clone(),
                provider: "gmail".to_string(),
                provider_subject: "google-subject-communication-action".to_string(),
                access_token: "dummy-reconnected-access-token".to_string(),
                refresh_token: "dummy-reconnected-refresh-token".to_string(),
                scopes: vec!["https://www.googleapis.com/auth/gmail.send".to_string()],
                capabilities: vec!["recruiter_reply".to_string()],
                grant_revision: 1,
                grant_sha256: String::new(),
                expires_at_ms: now_ms() + 3_600_000,
                created_at_ms: now_ms(),
                updated_at_ms: now_ms(),
            },
        )
        .unwrap();
        let fence_count: i64 = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM jobs_communication_write_fences
                  WHERE account_id = 'acct-jobs' AND connection_id = ?1",
                params![mailbox.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(fence_count, 0);
    }

    #[test]
    #[serial_test::serial]
    fn communication_revision_and_timestamp_advance_exactly_across_dispatch() {
        let _flags = CommunicationFlagGuard::enabled();
        let pool = test_pool();
        let (application, mailbox, message) =
            communication_test_application_mailbox_and_message(&pool);
        let action = communication_test_action(
            &application,
            &mailbox,
            &message,
            "revision-monotonic-dispatch",
        );
        let (created, _) = create_communication_action(&pool, "acct-jobs", &action).unwrap();
        assert_eq!(created.action_revision, 1);
        pool.get()
            .unwrap()
            .execute(
                "UPDATE jobs_communication_actions SET updated_at_ms = ?2 WHERE id = ?1",
                params![created.id, created.updated_at_ms + 10_000],
            )
            .unwrap();
        let before_approval = communication_action(&pool, "acct-jobs", &created.id)
            .unwrap()
            .unwrap();
        let approved = approve_communication_action(&pool, "acct-jobs", &created.id)
            .unwrap()
            .unwrap();
        assert_eq!(approved.action_revision, 2);
        assert_eq!(approved.updated_at_ms, before_approval.updated_at_ms + 1);
        assert_eq!(approved.approved_at_ms, Some(approved.updated_at_ms));

        let lease = claim_communication_action(&pool, "revision-monotonic-worker")
            .unwrap()
            .unwrap();
        assert_eq!(lease.action.action_revision, 3);
        assert!(lease.action.updated_at_ms > approved.updated_at_ms);
        mark_communication_action_request_started(
            &pool,
            &communication_lease_access(&lease, "revision-monotonic-worker"),
        )
        .unwrap()
        .expect("fresh communication request-start authority");
        let finished = finish_communication_action(
            &pool,
            &communication_success_finish(
                &pool,
                &lease,
                "revision-monotonic-worker",
                "gmail-revision-monotonic-sent",
            ),
        )
        .unwrap();
        assert_eq!(finished.action_revision, 4);
        assert!(finished.updated_at_ms > lease.action.updated_at_ms);
    }

    #[test]
    #[serial_test::serial]
    fn communication_mutations_reject_stale_or_noncanonical_review_snapshots() {
        let _flags = CommunicationFlagGuard::enabled();
        let pool = test_pool();
        let (application, mailbox, message) =
            communication_test_application_mailbox_and_message(&pool);
        let action =
            communication_test_action(&application, &mailbox, &message, "review-snapshot-cas");
        let (created, _) = create_communication_action(&pool, "acct-jobs", &action).unwrap();
        assert!(super::approve_communication_action(
            &pool,
            "acct-jobs",
            &created.id,
            created.action_revision,
            &created.payload_sha256.to_ascii_uppercase(),
        )
        .is_err());
        let approved = super::approve_communication_action(
            &pool,
            "acct-jobs",
            &created.id,
            created.action_revision,
            &created.payload_sha256,
        )
        .unwrap()
        .unwrap();
        assert_eq!(approved.action_revision, created.action_revision + 1);
        assert!(super::cancel_communication_action(
            &pool,
            "acct-jobs",
            &created.id,
            created.action_revision,
            &created.payload_sha256,
        )
        .is_err());
        let unchanged = communication_action(&pool, "acct-jobs", &created.id)
            .unwrap()
            .unwrap();
        assert_eq!(unchanged.status, "approved");
        assert_eq!(unchanged.action_revision, approved.action_revision);
        let cancelled = super::cancel_communication_action(
            &pool,
            "acct-jobs",
            &created.id,
            approved.action_revision,
            &approved.payload_sha256,
        )
        .unwrap()
        .unwrap();
        assert_eq!(cancelled.status, "cancelled");
        assert_eq!(cancelled.action_revision, approved.action_revision + 1);
    }

    #[test]
    #[serial_test::serial]
    fn communication_revision_exhaustion_fails_closed_without_mutation() {
        let _flags = CommunicationFlagGuard::enabled();
        let pool = test_pool();
        let (application, mailbox, message) =
            communication_test_application_mailbox_and_message(&pool);
        let action =
            communication_test_action(&application, &mailbox, &message, "revision-exhaustion");
        let (created, _) = create_communication_action(&pool, "acct-jobs", &action).unwrap();
        let conn = pool.get().unwrap();
        conn.execute(
            "UPDATE jobs_communication_actions SET action_revision = ?2 WHERE id = ?1",
            params![created.id, COMMUNICATION_ACTION_REVISION_MAX],
        )
        .unwrap();
        assert!(conn
            .execute(
                "UPDATE jobs_communication_actions SET action_revision = ?2 WHERE id = ?1",
                params![created.id, COMMUNICATION_ACTION_REVISION_MAX + 1],
            )
            .is_err());
        drop(conn);
        let exhausted = communication_action(&pool, "acct-jobs", &created.id)
            .unwrap()
            .unwrap();
        let (available, reason) =
            communication_action_execution_readiness(&pool, "acct-jobs", &exhausted).unwrap();
        assert!(!available);
        assert!(reason.contains("revision authority"));
        assert!(super::cancel_communication_action(
            &pool,
            "acct-jobs",
            &created.id,
            COMMUNICATION_ACTION_REVISION_MAX,
            &created.payload_sha256,
        )
        .is_err());
        let unchanged = communication_action(&pool, "acct-jobs", &created.id)
            .unwrap()
            .unwrap();
        assert_eq!(unchanged.status, "awaiting_approval");
        assert_eq!(unchanged.action_revision, COMMUNICATION_ACTION_REVISION_MAX);
    }

    #[test]
    #[serial_test::serial]
    fn parent_cleanup_preserves_unknown_and_failed_audit_until_authorized_account_purge() {
        let _flags = CommunicationFlagGuard::enabled();
        let pool = test_pool();
        let (application, mailbox, message) =
            communication_test_application_mailbox_and_message(&pool);
        let action = communication_test_action(
            &application,
            &mailbox,
            &message,
            "parent-cleanup-audit-retention",
        );
        let (action, _) = create_communication_action(&pool, "acct-jobs", &action).unwrap();
        approve_communication_action(&pool, "acct-jobs", &action.id)
            .unwrap()
            .unwrap();
        let lease = claim_communication_action(&pool, "parent-cleanup-worker")
            .unwrap()
            .unwrap();
        mark_communication_action_request_started(
            &pool,
            &communication_lease_access(&lease, "parent-cleanup-worker"),
        )
        .unwrap()
        .expect("fresh communication request-start authority");
        let conn = pool.get().unwrap();
        assert!(conn
            .execute(
                "DELETE FROM jobs_provider_messages WHERE id = ?1",
                params![message.id],
            )
            .is_err());
        conn.execute(
            "INSERT INTO jobs_communication_write_fences (
                account_id, connection_id, reason, created_at_ms
             ) VALUES ('acct-jobs', ?1, 'mailbox_disconnect', ?2)",
            params![mailbox.id, now_ms()],
        )
        .unwrap();
        conn.execute(
            "UPDATE jobs_communication_actions SET lease_expires_at_ms = ?2 WHERE id = ?1",
            params![action.id, now_ms() - 1],
        )
        .unwrap();
        drop(conn);
        assert!(claim_communication_action(&pool, "parent-cleanup-expiry")
            .unwrap()
            .is_none());
        assert_eq!(
            communication_action(&pool, "acct-jobs", &action.id)
                .unwrap()
                .unwrap()
                .status,
            "side_effect_unknown"
        );
        assert!(pool
            .get()
            .unwrap()
            .execute(
                "DELETE FROM jobs_provider_messages WHERE id = ?1",
                params![message.id],
            )
            .is_err());
        pool.get()
            .unwrap()
            .execute(
                "UPDATE jobs_communication_actions
                    SET status = 'failed', lease_owner = NULL, lease_kind = NULL,
                        lease_token_sha256 = NULL, lease_expires_at_ms = NULL
                  WHERE id = ?1",
                params![action.id],
            )
            .unwrap();
        assert!(pool
            .get()
            .unwrap()
            .execute(
                "DELETE FROM jobs_provider_messages WHERE id = ?1",
                params![message.id],
            )
            .is_err());

        assert!(matches!(
            crate::db::account_data::begin_account_deletion(&pool, "acct-jobs", now_ms()).unwrap(),
            Some(crate::db::account_data::BeginAccountDeletionResult::Ready(
                _
            )) | Some(crate::db::account_data::BeginAccountDeletionResult::WaitingForUploads(_))
        ));
        {
            let mut conn = pool.get().unwrap();
            let tx = conn
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .unwrap();
            // The production account purge now also requires runner/workflow
            // proofs outside this communication unit. Remove only this
            // terminal action and its deletion-intent fixture before teardown
            // so every production hard-delete guard remains exact.
            tx.execute(
                "DELETE FROM jobs_communication_actions WHERE id = ?1",
                params![&action.id],
            )
            .unwrap();
            tx.execute(
                "DELETE FROM account_deletion_intents WHERE account_id = 'acct-jobs'",
                [],
            )
            .unwrap();
            tx.execute("DELETE FROM accounts WHERE id = 'acct-jobs'", [])
                .unwrap();
            tx.execute(
                "DELETE FROM jobs_workflow_cleanup_hard_delete_cascade_tokens
                  WHERE account_id = 'acct-jobs'",
                [],
            )
            .unwrap();
            tx.commit().unwrap();
        }
        let remaining: i64 = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM jobs_communication_actions WHERE id = ?1",
                params![action.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(remaining, 0);
    }

    #[test]
    #[serial_test::serial]
    fn expired_communication_dispatch_requires_reconciliation_before_retry() {
        let _flags = CommunicationFlagGuard::enabled();
        let pool = test_pool();
        let (application, mailbox, message) =
            communication_test_application_mailbox_and_message(&pool);
        let action =
            communication_test_action(&application, &mailbox, &message, "reply-interview-3");
        let (stored, _) = create_communication_action(&pool, "acct-jobs", &action).unwrap();
        approve_communication_action(&pool, "acct-jobs", &stored.id)
            .unwrap()
            .unwrap();
        let lease = claim_communication_action(&pool, "mail-worker")
            .unwrap()
            .unwrap();
        mark_communication_action_request_started(
            &pool,
            &communication_lease_access(&lease, "mail-worker"),
        )
        .unwrap()
        .expect("fresh communication request-start authority");
        pool.get()
            .unwrap()
            .execute(
                "UPDATE jobs_communication_actions SET lease_expires_at_ms = ?2 WHERE id = ?1",
                params![stored.id, now_ms() - 1],
            )
            .unwrap();

        assert!(claim_communication_action(&pool, "replacement-worker")
            .unwrap()
            .is_none());
        let unknown = communication_action(&pool, "acct-jobs", &stored.id)
            .unwrap()
            .unwrap();
        assert_eq!(unknown.status, "side_effect_unknown");
        assert!(finish_communication_action(
            &pool,
            &communication_success_finish(&pool, &lease, "mail-worker", "gmail-message-late"),
        )
        .is_err());
        assert_eq!(
            communication_action(&pool, "acct-jobs", &stored.id)
                .unwrap()
                .unwrap()
                .status,
            "side_effect_unknown"
        );
    }

    #[test]
    #[serial_test::serial]
    fn communication_dispatch_rechecks_public_beta_before_refresh_and_keeps_reconciliation() {
        let _flags = CommunicationFlagGuard::enabled();
        let pool = test_pool();
        let (application, mailbox, message) =
            communication_test_application_mailbox_and_message(&pool);
        let mut refresh_eligible = jobs_provider_credential(&pool, "acct-jobs", &mailbox.id)
            .unwrap()
            .unwrap();
        refresh_eligible.expires_at_ms = now_ms();
        refresh_eligible.updated_at_ms = now_ms();
        save_jobs_provider_credential(&pool, "acct-jobs", &refresh_eligible).unwrap();
        assert!(
            jobs_provider_credential(&pool, "acct-jobs", &mailbox.id)
                .unwrap()
                .unwrap()
                .expires_at_ms
                <= now_ms().saturating_add(60_000)
        );
        let action = communication_test_action(
            &application,
            &mailbox,
            &message,
            "phase-622-public-beta-communication-fence",
        );
        let (stored, _) = create_communication_action(&pool, "acct-jobs", &action).unwrap();
        approve_communication_action(&pool, "acct-jobs", &stored.id)
            .unwrap()
            .unwrap();

        pool.get()
            .unwrap()
            .execute(
                "INSERT INTO jobs_public_beta_overrides (
                    cohort_id, account_id, denied, revision, created_at_ms, updated_at_ms
                 ) VALUES ('public-v1', 'acct-jobs', 1, 1, 1, 1)",
                [],
            )
            .unwrap();
        assert!(claim_communication_action(&pool, "phase-622-denied-worker")
            .unwrap()
            .is_none());
        let denied = communication_action(&pool, "acct-jobs", &stored.id)
            .unwrap()
            .unwrap();
        assert_eq!(denied.status, "approved");
        assert_eq!(denied.attempt_count, 0);

        pool.get()
            .unwrap()
            .execute(
                "UPDATE jobs_public_beta_overrides
                    SET denied = 0, revision = 2, updated_at_ms = 2
                  WHERE cohort_id = 'public-v1' AND account_id = 'acct-jobs'",
                [],
            )
            .unwrap();
        let suspended_lease = claim_communication_action(&pool, "phase-622-suspend-worker")
            .unwrap()
            .expect("clearing the account denial permits a fresh dispatch claim");
        pool.get()
            .unwrap()
            .execute(
                "UPDATE jobs_public_beta_cohorts
                    SET state = 'suspended', revision = revision + 1
                  WHERE id = 'public-v1'",
                [],
            )
            .unwrap();
        assert!(mark_communication_action_request_started(
            &pool,
            &communication_lease_access(&suspended_lease, "phase-622-suspend-worker"),
        )
        .unwrap()
        .is_none());
        let suspended = communication_action(&pool, "acct-jobs", &stored.id)
            .unwrap()
            .unwrap();
        assert_eq!(suspended.status, "needs_input");
        assert!(suspended.active_attempt_id.is_none());
        assert_eq!(suspended.approved_grant_revision, 0);
        assert_eq!(
            pool.get()
                .unwrap()
                .query_row(
                    "SELECT COUNT(*) FROM jobs_communication_action_attempt_evidence
                      WHERE action_id = ?1 AND event_kind = 'request_started'",
                    params![stored.id],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
            0
        );

        pool.get()
            .unwrap()
            .execute(
                "UPDATE jobs_public_beta_cohorts
                    SET state = 'closed_to_new', revision = revision + 1
                  WHERE id = 'public-v1'",
                [],
            )
            .unwrap();
        approve_communication_action(&pool, "acct-jobs", &stored.id)
            .unwrap()
            .unwrap();
        let dispatch = claim_communication_action(&pool, "phase-622-dispatch-worker")
            .unwrap()
            .expect("fresh cohort authority permits an approved communication claim");
        mark_communication_action_request_started(
            &pool,
            &communication_lease_access(&dispatch, "phase-622-dispatch-worker"),
        )
        .unwrap()
        .expect("fresh public-beta authority permits request start");
        let unknown = finish_communication_action(
            &pool,
            &JobsCommunicationActionFinish {
                lease: communication_lease_access(&dispatch, "phase-622-dispatch-worker"),
                outcome: "side_effect_unknown".to_string(),
                provider_object_id: String::new(),
                evidence: json!({
                    "provider": "gmail",
                    "result": "transport_interrupted"
                }),
            },
        )
        .unwrap();
        assert_eq!(unknown.status, "side_effect_unknown");

        pool.get()
            .unwrap()
            .execute_batch(
                "UPDATE jobs_public_beta_overrides
                    SET denied = 1, revision = 3, updated_at_ms = 3
                  WHERE cohort_id = 'public-v1' AND account_id = 'acct-jobs';
                 UPDATE jobs_public_beta_cohorts
                    SET state = 'suspended', revision = revision + 1
                  WHERE id = 'public-v1';",
            )
            .unwrap();
        let reconciliation =
            claim_communication_action_reconciliation(&pool, "phase-622-lookup-only-reconciler")
                .unwrap()
                .expect(
                    "lookup-only reconciliation remains available after effect authority closes",
                );
        assert_eq!(reconciliation.action.status, "side_effect_unknown");
        assert_eq!(reconciliation.lease_kind, "reconcile");
    }

    #[test]
    #[serial_test::serial]
    fn expired_dispatch_transition_obeys_the_account_deletion_fence() {
        let _flags = CommunicationFlagGuard::enabled();
        let pool = test_pool();
        let (application, mailbox, message) =
            communication_test_application_mailbox_and_message(&pool);
        let action = communication_test_action(
            &application,
            &mailbox,
            &message,
            "expired-dispatch-deletion-fence",
        );
        let (stored, _) = create_communication_action(&pool, "acct-jobs", &action).unwrap();
        approve_communication_action(&pool, "acct-jobs", &stored.id)
            .unwrap()
            .unwrap();
        claim_communication_action(&pool, "deletion-race-worker")
            .unwrap()
            .unwrap();
        pool.get()
            .unwrap()
            .execute(
                "UPDATE jobs_communication_actions SET lease_expires_at_ms = ?2
                  WHERE account_id = 'acct-jobs' AND id = ?1",
                params![stored.id, now_ms() - 1],
            )
            .unwrap();
        pool.get()
            .unwrap()
            .execute(
                "INSERT INTO account_deletion_intents (
                    account_id, requested_at_ms, last_checked_at_ms,
                    fresh_upload_cutoff_ms, fresh_in_flight_puts
                 ) VALUES ('acct-jobs', 1, 1, 0, 0)",
                [],
            )
            .unwrap();

        assert_account_deletion_fence(claim_communication_action(
            &pool,
            "replacement-deletion-race-worker",
        ));
        assert_eq!(
            communication_action(&pool, "acct-jobs", &stored.id)
                .unwrap()
                .unwrap()
                .status,
            "dispatching"
        );
    }

    #[test]
    #[serial_test::serial]
    fn authoritative_absence_requires_age_and_three_confirmations_for_one_attempt() {
        let _flags = CommunicationFlagGuard::enabled();
        let pool = test_pool();
        let (application, mailbox, message) =
            communication_test_application_mailbox_and_message(&pool);
        let action = communication_test_action(
            &application,
            &mailbox,
            &message,
            "reply-authoritative-absence-threshold",
        );
        let (stored, _) = create_communication_action(&pool, "acct-jobs", &action).unwrap();
        approve_communication_action(&pool, "acct-jobs", &stored.id)
            .unwrap()
            .unwrap();
        let dispatch = claim_communication_action(&pool, "absence-dispatch-worker")
            .unwrap()
            .unwrap();
        mark_communication_action_request_started(
            &pool,
            &communication_lease_access(&dispatch, "absence-dispatch-worker"),
        )
        .unwrap()
        .expect("fresh communication request-start authority");
        let unknown = finish_communication_action(
            &pool,
            &JobsCommunicationActionFinish {
                lease: communication_lease_access(&dispatch, "absence-dispatch-worker"),
                outcome: "side_effect_unknown".to_string(),
                provider_object_id: String::new(),
                evidence: json!({"provider": "gmail", "result": "transport_interrupted"}),
            },
        )
        .unwrap();
        assert_eq!(unknown.status, "side_effect_unknown");

        pool.get()
            .unwrap()
            .execute(
                "UPDATE jobs_communication_actions
                    SET lease_kind = 'dispatch', lease_expires_at_ms = ?2,
                        next_attempt_at_ms = 0
                  WHERE account_id = 'acct-jobs' AND id = ?1",
                params![stored.id, now_ms() + 60_000],
            )
            .unwrap();
        assert!(
            claim_communication_action_reconciliation(&pool, "absence-reconciler")
                .unwrap()
                .is_none()
        );
        pool.get()
            .unwrap()
            .execute(
                "UPDATE jobs_communication_actions SET lease_expires_at_ms = ?2
                  WHERE account_id = 'acct-jobs' AND id = ?1",
                params![stored.id, now_ms() - 1],
            )
            .unwrap();

        let first_lease = claim_communication_action_reconciliation(&pool, "absence-reconciler")
            .unwrap()
            .unwrap();
        pool.get()
            .unwrap()
            .execute(
                "UPDATE jobs_communication_actions SET lease_expires_at_ms = ?2
                  WHERE account_id = 'acct-jobs' AND id = ?1",
                params![stored.id, now_ms() - 1],
            )
            .unwrap();
        assert!(reconcile_communication_action(
            &pool,
            &JobsCommunicationActionReconciliation {
                lease: communication_lease_access(&first_lease, "absence-reconciler"),
                resolution: "inconclusive".to_string(),
                provider_object_id: String::new(),
                evidence: communication_reconciliation_evidence(
                    &first_lease,
                    json!({"check": "expired-reconciliation-lease"}),
                ),
            },
        )
        .is_err());
        pool.get()
            .unwrap()
            .execute(
                "UPDATE jobs_communication_actions SET lease_expires_at_ms = ?2
                  WHERE account_id = 'acct-jobs' AND id = ?1",
                params![stored.id, now_ms() + 60_000],
            )
            .unwrap();
        let mut wrong_binding = communication_reconciliation_evidence(
            &first_lease,
            json!({"authoritative_absence": true, "check": "wrong-binding"}),
        );
        wrong_binding["payload_sha256"] = json!("0".repeat(64));
        assert!(reconcile_communication_action(
            &pool,
            &JobsCommunicationActionReconciliation {
                lease: communication_lease_access(&first_lease, "absence-reconciler"),
                resolution: "confirmed_absent".to_string(),
                provider_object_id: String::new(),
                evidence: wrong_binding,
            },
        )
        .unwrap_err()
        .to_string()
        .contains("exact attempt"));
        let too_early = JobsCommunicationActionReconciliation {
            lease: communication_lease_access(&first_lease, "absence-reconciler"),
            resolution: "confirmed_absent".to_string(),
            provider_object_id: String::new(),
            evidence: communication_reconciliation_evidence(
                &first_lease,
                json!({"authoritative_absence": true, "check": 0}),
            ),
        };
        assert!(reconcile_communication_action(&pool, &too_early)
            .unwrap_err()
            .to_string()
            .contains("minimum age"));
        pool.get()
            .unwrap()
            .execute(
                "UPDATE jobs_communication_actions
                    SET dispatched_at_ms = ?2
                  WHERE account_id = 'acct-jobs' AND id = ?1",
                params![stored.id, now_ms() - 16 * 60_000],
            )
            .unwrap();

        let inconclusive = reconcile_communication_action(
            &pool,
            &JobsCommunicationActionReconciliation {
                lease: communication_lease_access(&first_lease, "absence-reconciler"),
                resolution: "inconclusive".to_string(),
                provider_object_id: String::new(),
                evidence: communication_reconciliation_evidence(
                    &first_lease,
                    json!({"check": "identical-inconclusive"}),
                ),
            },
        )
        .unwrap();
        assert_eq!(inconclusive.status, "side_effect_unknown");

        let mut last = inconclusive;
        for resolution in [
            "confirmed_absent",
            "inconclusive",
            "confirmed_absent",
            "confirmed_absent",
        ] {
            pool.get()
                .unwrap()
                .execute(
                    "UPDATE jobs_communication_actions
                        SET next_attempt_at_ms = 0
                      WHERE account_id = 'acct-jobs' AND id = ?1",
                    params![stored.id],
                )
                .unwrap();
            let lease = claim_communication_action_reconciliation(&pool, "absence-reconciler")
                .unwrap()
                .unwrap();
            last = reconcile_communication_action(
                &pool,
                &JobsCommunicationActionReconciliation {
                    lease: communication_lease_access(&lease, "absence-reconciler"),
                    resolution: resolution.to_string(),
                    provider_object_id: String::new(),
                    evidence: communication_reconciliation_evidence(
                        &lease,
                        if resolution == "confirmed_absent" {
                            json!({
                                "authoritative_absence": true,
                                "check": "identical-absence"
                            })
                        } else {
                            json!({"check": "identical-inconclusive"})
                        },
                    ),
                },
            )
            .unwrap();
        }
        assert_eq!(last.status, "needs_input");
        assert!(last.active_attempt_id.is_none());
        assert_eq!(last.approved_grant_revision, 0);
        let (absences, inconclusive): (i64, i64) = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT
                    SUM(CASE WHEN resolution = 'confirmed_absent' THEN 1 ELSE 0 END),
                    SUM(CASE WHEN resolution = 'inconclusive' THEN 1 ELSE 0 END)
                   FROM jobs_communication_action_reconciliations
                  WHERE account_id = 'acct-jobs' AND action_id = ?1 AND attempt_id = ?2",
                params![stored.id, dispatch.attempt_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(absences, 3);
        assert_eq!(inconclusive, 2);
    }

    #[test]
    #[serial_test::serial]
    fn cancelled_communication_actions_never_dispatch() {
        let _flags = CommunicationFlagGuard::enabled();
        let pool = test_pool();
        let (application, mailbox, message) =
            communication_test_application_mailbox_and_message(&pool);
        let action =
            communication_test_action(&application, &mailbox, &message, "reply-interview-4");
        let (stored, _) = create_communication_action(&pool, "acct-jobs", &action).unwrap();
        let cancelled = cancel_communication_action(&pool, "acct-jobs", &stored.id)
            .unwrap()
            .unwrap();
        assert_eq!(cancelled.status, "cancelled");
        assert!(claim_communication_action(&pool, "mail-worker")
            .unwrap()
            .is_none());
        assert!(approve_communication_action(&pool, "acct-jobs", &stored.id).is_err());
    }

    #[test]
    #[serial_test::serial]
    fn communication_cancellation_obeys_the_account_deletion_fence() {
        let _flags = CommunicationFlagGuard::enabled();
        let pool = test_pool();
        let (application, mailbox, message) =
            communication_test_application_mailbox_and_message(&pool);
        let action =
            communication_test_action(&application, &mailbox, &message, "cancel-deletion-fence");
        let (stored, _) = create_communication_action(&pool, "acct-jobs", &action).unwrap();
        pool.get()
            .unwrap()
            .execute(
                "INSERT INTO account_deletion_intents (
                    account_id, requested_at_ms, last_checked_at_ms,
                    fresh_upload_cutoff_ms, fresh_in_flight_puts
                 ) VALUES ('acct-jobs', 1, 1, 0, 0)",
                [],
            )
            .unwrap();

        assert_account_deletion_fence(cancel_communication_action(&pool, "acct-jobs", &stored.id));
        assert_eq!(
            communication_action(&pool, "acct-jobs", &stored.id)
                .unwrap()
                .unwrap()
                .status,
            "awaiting_approval"
        );
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
                metadata: json!({
                    "provider_id": "outlook-message-recovery",
                    "conversation_id": "conversation-1",
                    "reply_target": "recruiter@example.org",
                }),
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
        assert_eq!(updated.metadata["provider_id"], "outlook-message-recovery");
        assert_eq!(updated.metadata["conversation_id"], "conversation-1");
        assert_eq!(updated.metadata["reply_target"], "recruiter@example.org");
        assert!(updated.processed_at_ms.is_some());
        assert!(update_provider_message_processing(
            &pool,
            "acct-jobs",
            &stored.id,
            None,
            "processed",
            "acknowledgment",
            0.99,
            json!({"conversation_id": "attacker-controlled"}),
        )
        .is_err());
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
            &verified_test_posting(
                JobPosting {
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
                    discovery_evidence: JobDiscoveryEvidence::default(),
                    eligibility: None,
                },
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
        let frozen_policy = application
            .receipt
            .pointer("/career_track_policy_authority")
            .expect("application receipt must freeze the exact Track policy authority");
        assert_eq!(
            resume
                .content
                .pointer("/provenance/career_track_policy_authority"),
            Some(frozen_policy)
        );
        assert_eq!(
            frozen_policy
                .get("taxonomy_version")
                .and_then(Value::as_str),
            Some(crate::jobs_taxonomy::taxonomy_version())
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
    fn channel_head_change_after_claim_fences_submit_but_preserves_binding_and_replay() {
        let pool = test_pool();
        let (application, run_id, ticket_hash, _) =
            local_run_authority_fixture_unbound(&pool, "release-head-change");
        reserve_application_attempt(&pool, "acct-jobs", &application.id, "local").unwrap();
        let descriptor = seed_test_local_browser_release_authority(&pool);
        let nonce = "a".repeat(64);
        let first = claim_local_run_with_browser_release(
            &pool,
            &run_id,
            &ticket_hash,
            &nonce,
            &descriptor,
            "test-server",
            |ticket, release| {
                Ok(json!({
                    "runId": ticket.id,
                    "bindingSha256": browser_release_binding_sha256(release),
                }))
            },
        )
        .unwrap();
        let BrowserLocalRunClaimDisposition::Success(first) = first else {
            panic!("initial Browser release claim did not succeed")
        };
        let frozen_binding = first.release.clone();
        let frozen_receipt = browser_release_receipt_authority(&frozen_binding);
        assert!(browser_release_receipt_authority_valid(&frozen_receipt));

        advance_test_browser_release_channel_head(&pool);
        assert_eq!(
            get_local_run_browser_release_binding(&pool, "acct-jobs", &run_id)
                .unwrap()
                .as_ref(),
            Some(&frozen_binding),
            "the append-only run binding must survive a later channel transition"
        );
        let replay = claim_local_run_with_browser_release(
            &pool,
            &run_id,
            &ticket_hash,
            &nonce,
            &descriptor,
            "test-server",
            |_, _| anyhow::bail!("an exact replay must not issue fresh capabilities"),
        )
        .unwrap();
        let BrowserLocalRunClaimDisposition::Success(replay) = replay else {
            panic!("exact claim replay was invalidated by a channel transition")
        };
        assert!(replay.replayed);
        assert_eq!(
            replay.response_json.as_bytes(),
            first.response_json.as_bytes()
        );
        assert_eq!(replay.release, frozen_binding);
        assert!(browser_release_receipt_authority_valid(&frozen_receipt));

        let running = get_application(&pool, "acct-jobs", &application.id)
            .unwrap()
            .unwrap();
        let mut capacity = test_submission_evidence_capacity(&application.id, &run_id);
        capacity.runner = "local".to_string();
        assert!(!local_run_submit_authorized(
            &pool,
            &run_id,
            &ticket_hash,
            &test_final_submit_proof(&running),
            &capacity,
        )
        .unwrap());
        assert_eq!(
            submission_capacity_count(&pool, &application.id, &run_id),
            0
        );
        let (head_revision, binding_count, replay_count): (i64, i64, i64) = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT head.head_revision,
                        (SELECT COUNT(*) FROM jobs_local_run_release_bindings
                          WHERE run_id = ?1),
                        (SELECT COUNT(*) FROM jobs_local_run_claim_replays
                          WHERE run_id = ?1)
                   FROM jobs_browser_release_channel_heads head
                  WHERE head.channel = 'beta'",
                params![run_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!((head_revision, binding_count, replay_count), (2, 1, 1));
    }

    #[test]
    fn account_channel_reassignment_after_claim_fences_submit() {
        let pool = test_pool();
        let (application, run_id, ticket_hash, _) =
            local_run_authority_fixture_unbound(&pool, "release-assignment-change");
        reserve_application_attempt(&pool, "acct-jobs", &application.id, "local").unwrap();
        let descriptor = seed_test_local_browser_release_authority(&pool);
        assert!(matches!(
            claim_local_run_with_browser_release(
                &pool,
                &run_id,
                &ticket_hash,
                &"a".repeat(64),
                &descriptor,
                "test-server",
                |ticket, _| Ok(json!({ "runId": ticket.id })),
            )
            .unwrap(),
            BrowserLocalRunClaimDisposition::Success(_)
        ));

        advance_test_browser_account_assignment(&pool);
        let running = get_application(&pool, "acct-jobs", &application.id)
            .unwrap()
            .unwrap();
        let mut capacity = test_submission_evidence_capacity(&application.id, &run_id);
        capacity.runner = "local".to_string();
        assert!(!local_run_submit_authorized(
            &pool,
            &run_id,
            &ticket_hash,
            &test_final_submit_proof(&running),
            &capacity,
        )
        .unwrap());
        assert_eq!(
            submission_capacity_count(&pool, &application.id, &run_id),
            0
        );
        assert_eq!(
            local_claim_state(&pool, &application.id, &run_id).0,
            "claimed"
        );
    }

    #[test]
    fn co_descriptor_artifact_revocation_fences_darwin_claim_and_submit() {
        let pool = test_pool();
        let (application, run_id, ticket_hash, _) =
            local_run_authority_fixture_unbound(&pool, "release-sibling-revoked-existing");
        reserve_application_attempt(&pool, "acct-jobs", &application.id, "local").unwrap();
        let descriptor = seed_test_local_browser_release_authority(&pool);
        assert!(matches!(
            claim_local_run_with_browser_release(
                &pool,
                &run_id,
                &ticket_hash,
                &"a".repeat(64),
                &descriptor,
                "test-server",
                |ticket, _| Ok(json!({ "runId": ticket.id })),
            )
            .unwrap(),
            BrowserLocalRunClaimDisposition::Success(_)
        ));

        seed_test_browser_release_revocation(
            &pool,
            &hex::encode(Sha256::digest(b"test-darwin-zip-revocation")),
            "test-darwin-zip-revocation",
            "artifact",
            "test-artifact-darwin-arm64-zip",
            &"d".repeat(64),
        );
        let running = get_application(&pool, "acct-jobs", &application.id)
            .unwrap()
            .unwrap();
        let mut capacity = test_submission_evidence_capacity(&application.id, &run_id);
        capacity.runner = "local".to_string();
        assert!(!local_run_submit_authorized(
            &pool,
            &run_id,
            &ticket_hash,
            &test_final_submit_proof(&running),
            &capacity,
        )
        .unwrap());
        assert_eq!(
            submission_capacity_count(&pool, &application.id, &run_id),
            0
        );
        assert_eq!(
            local_claim_state(&pool, &application.id, &run_id).0,
            "claimed"
        );

        update_attempt_reservation_status(&pool, "acct-jobs", &application.id, "released").unwrap();
        let (new_application, new_run_id, new_ticket_hash, _) =
            local_run_authority_fixture_unbound(&pool, "release-sibling-revoked-new");
        reserve_application_attempt(&pool, "acct-jobs", &new_application.id, "local").unwrap();
        let before = local_claim_state(&pool, &new_application.id, &new_run_id);
        assert!(matches!(
            claim_local_run_with_browser_release(
                &pool,
                &new_run_id,
                &new_ticket_hash,
                &"b".repeat(64),
                &descriptor,
                "test-server",
                |_, _| Ok(json!({ "unexpected": true })),
            )
            .unwrap(),
            BrowserLocalRunClaimDisposition::ReleaseUnavailable
        ));
        assert_eq!(
            local_claim_state(&pool, &new_application.id, &new_run_id),
            before
        );
    }

    #[test]
    fn exact_release_revocation_blocks_portal_claim_and_preclick_but_not_receipt_shape() {
        let pool = test_pool();
        let (application, run_id, ticket_hash, _) =
            local_run_authority_fixture_unbound(&pool, "release-revoked-existing");
        reserve_application_attempt(&pool, "acct-jobs", &application.id, "local").unwrap();
        let descriptor = seed_test_local_browser_release_authority(&pool);
        let first = claim_local_run_with_browser_release(
            &pool,
            &run_id,
            &ticket_hash,
            &"a".repeat(64),
            &descriptor,
            "test-server",
            |ticket, _| Ok(json!({ "runId": ticket.id })),
        )
        .unwrap();
        let BrowserLocalRunClaimDisposition::Success(first) = first else {
            panic!("initial Browser release claim did not succeed")
        };
        let frozen_receipt = browser_release_receipt_authority(&first.release);
        assert!(browser_release_receipt_authority_valid(&frozen_receipt));
        assert!(matches!(
            local_browser_release_availability(&pool, "acct-jobs", "test-server").unwrap(),
            LocalBrowserReleaseAvailability::Available { .. }
        ));

        let release_sha256 = hex::encode(Sha256::digest(b"test-release"));
        seed_test_browser_release_revocation(
            &pool,
            &hex::encode(Sha256::digest(b"test-release-revocation")),
            "test-release-revocation",
            "release",
            "test-release",
            &release_sha256,
        );
        assert!(matches!(
            local_browser_release_availability(&pool, "acct-jobs", "test-server").unwrap(),
            LocalBrowserReleaseAvailability::Unavailable { .. }
        ));
        assert!(browser_release_receipt_authority_valid(&frozen_receipt));

        let running = get_application(&pool, "acct-jobs", &application.id)
            .unwrap()
            .unwrap();
        let mut capacity = test_submission_evidence_capacity(&application.id, &run_id);
        capacity.runner = "local".to_string();
        assert!(!local_run_submit_authorized(
            &pool,
            &run_id,
            &ticket_hash,
            &test_final_submit_proof(&running),
            &capacity,
        )
        .unwrap());
        assert_eq!(
            submission_capacity_count(&pool, &application.id, &run_id),
            0
        );

        update_attempt_reservation_status(&pool, "acct-jobs", &application.id, "released").unwrap();

        let (new_application, new_run_id, new_ticket_hash, _) =
            local_run_authority_fixture_unbound(&pool, "release-revoked-new");
        reserve_application_attempt(&pool, "acct-jobs", &new_application.id, "local").unwrap();
        let before = local_claim_state(&pool, &new_application.id, &new_run_id);
        assert!(matches!(
            claim_local_run_with_browser_release(
                &pool,
                &new_run_id,
                &new_ticket_hash,
                &"b".repeat(64),
                &descriptor,
                "test-server",
                |_, _| Ok(json!({ "unexpected": true })),
            )
            .unwrap(),
            BrowserLocalRunClaimDisposition::ReleaseUnavailable
        ));
        assert_eq!(
            local_claim_state(&pool, &new_application.id, &new_run_id),
            before
        );
        assert!(browser_release_receipt_authority_valid(&frozen_receipt));
    }

    #[test]
    fn certified_local_phase_a_uses_signed_runtime_and_rolls_back_with_its_transaction() {
        let pool = test_pool();
        let (mut application, run_id, _ticket_hash, _) =
            local_run_authority_fixture_unbound(&pool, "certified-phase-a-rollback");
        application.submission_mode = "auto_submit".to_string();
        application.receipt["approved_execution"]["schema_version"] = json!(3);
        application.receipt["approved_execution"]["admission"] = json!({
            "kind": "track_auto_submit",
            "authorization_id": "missing-authorization",
            "career_track_id": "track-default",
            "revision_no": 1,
            "authority_fingerprint": "a".repeat(64),
            "ats_certification": { "schema_version": 1 },
        });
        assert!(application_has_frozen_ats_certification(&application));
        reserve_application_attempt(&pool, "acct-jobs", &application.id, "local").unwrap();
        let descriptor = seed_test_local_browser_release_authority(&pool);
        let release =
            browser_release_for_claim(&pool, "acct-jobs", &descriptor, "test-server", now_ms())
                .unwrap()
                .expect("exact signed Browser release binding");
        let ticket = get_local_run_ticket(&pool, "acct-jobs", &run_id)
            .unwrap()
            .expect("local Browser ticket");
        let nonce = "a".repeat(64);
        let context = browser_release_phase_a_context(
            &application,
            &ticket,
            &run_id,
            &hex::encode(Sha256::digest(nonce.as_bytes())),
            &descriptor,
            &release,
        )
        .unwrap()
        .expect("certified Auto-submit Phase A context");
        let AtsCertificationRuntimeAttestation::Local {
            platform,
            architecture,
            browser_release_manifest_sha256,
            browser_artifact_sha256,
            browser_build_descriptor_sha256,
            automation_bundle_sha256,
            playwright_version,
            chromium_revision,
            chromium_executable_sha256,
        } = &context.runtime_attestation
        else {
            panic!("local Browser claim constructed a cloud runtime attestation")
        };
        assert_eq!(platform.as_str(), "macos");
        assert_eq!(architecture.as_str(), "arm64");
        assert_eq!(browser_release_manifest_sha256, &"1".repeat(64));
        assert_eq!(browser_artifact_sha256, &"2".repeat(64));
        assert_eq!(browser_build_descriptor_sha256, &"3".repeat(64));
        assert_eq!(automation_bundle_sha256, &"4".repeat(64));
        assert_eq!(playwright_version.as_str(), "1.61.1");
        assert_eq!(chromium_revision.as_str(), "1228");
        assert_eq!(chromium_executable_sha256, &"7".repeat(64));
        let before = local_claim_state(&pool, &application.id, &run_id);
        let mut connection = pool.get().unwrap();
        let tx = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        tx.execute(
            "UPDATE jobs_local_run_tickets SET status = 'claimed' WHERE id = ?1",
            params![run_id],
        )
        .unwrap();
        let error = create_ats_application_certification_binding_from_context_sqlite_tx(
            &tx,
            &context,
            now_ms(),
        )
        .expect_err("invalid frozen ATS authority must reject Phase A");
        assert!(matches!(
            error,
            AtsCertificationAuthorityError::ScopeMismatch
        ));
        drop(tx);
        drop(connection);
        assert_eq!(local_claim_state(&pool, &application.id, &run_id), before);
        let (replay_count, phase_a_count): (i64, i64) = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT
                    (SELECT COUNT(*) FROM jobs_local_run_claim_replays WHERE run_id = ?1),
                    (SELECT COUNT(*) FROM jobs_application_ats_certification_bindings
                      WHERE run_id = ?1)",
                params![run_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!((replay_count, phase_a_count), (0, 0));
    }

    #[test]
    fn certified_local_production_path_is_single_submit_even_after_response_loss() {
        let pool = test_pool();
        let browser = browser_release_registry_tests::install_signed_browser_release_fixture(
            &pool,
            "acct-jobs",
        );
        assert_eq!(
            browser.binding.build_descriptor_sha256,
            browser.descriptor.descriptor_sha256
        );
        let runtime_target = browser.runtime_target.clone();
        let fixture = certified_application_fixture(
            &pool,
            "certified-local-production",
            "local",
            runtime_target.clone(),
            "",
        );
        let application = fixture.application;
        let run_id = fixture.run_id;
        let ticket_hash = fixture
            .ticket_hash
            .expect("certified local fixture has a ticket hash");
        let proof = fixture.proof;
        let descriptor = browser.descriptor;
        let nonce = "a".repeat(64);
        let claim = claim_local_run_with_browser_release(
            &pool,
            &run_id,
            &ticket_hash,
            &nonce,
            &descriptor,
            "test-server",
            |ticket, release| {
                Ok(json!({
                    "runId": ticket.id,
                    "bindingSha256": browser_release_binding_sha256(release),
                }))
            },
        )
        .unwrap();
        let claim = match claim {
            BrowserLocalRunClaimDisposition::Success(claim) => claim,
            disposition => {
                panic!("certified local production claim did not succeed: {disposition:?}")
            }
        };
        assert!(!claim.replayed);

        let phase_a: (String, i64, String, String, String, String, String, String) = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT phase, fence, runner_target_sha256, platform, architecture,
                        browser_release_manifest_sha256, browser_artifact_sha256,
                        browser_build_descriptor_sha256
                   FROM jobs_application_ats_certification_bindings
                  WHERE account_id = 'acct-jobs' AND application_id = ?1 AND run_id = ?2",
                params![application.id, run_id],
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
                    ))
                },
            )
            .unwrap();
        assert_eq!(
            phase_a,
            (
                "preflight".to_string(),
                0,
                runtime_target.runtime_sha256,
                runtime_target.platform,
                runtime_target.architecture,
                runtime_target
                    .browser_release_manifest_sha256
                    .expect("signed local runtime has a Browser release manifest"),
                runtime_target
                    .browser_artifact_sha256
                    .expect("signed local runtime has a Browser artifact"),
                runtime_target
                    .browser_build_descriptor_sha256
                    .expect("signed local runtime has a Browser build descriptor"),
            )
        );

        let mut capacity = test_submission_evidence_capacity(&application.id, &run_id);
        capacity.runner = "local".to_string();
        assert!(
            local_run_submit_authorized(&pool, &run_id, &ticket_hash, &proof, &capacity).unwrap()
        );
        let after_submit: (String, i64, String, i64, i64, i64) = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT binding.phase, binding.fence, ticket.status,
                        (SELECT COUNT(*) FROM jobs_ats_certification_canary_reservations
                          WHERE binding_id = binding.binding_id),
                        (SELECT COUNT(*) FROM jobs_submission_evidence_capacity
                          WHERE account_id = 'acct-jobs' AND application_id = ?1
                            AND run_id = ?2),
                        (SELECT COUNT(*) FROM jobs_local_run_claim_replays WHERE run_id = ?2)
                   FROM jobs_application_ats_certification_bindings binding
                   JOIN jobs_local_run_tickets ticket ON ticket.id = binding.run_id
                  WHERE binding.application_id = ?1 AND binding.run_id = ?2",
                params![application.id, run_id],
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
        assert_eq!(
            after_submit,
            (
                "consumed".to_string(),
                1,
                "click_started".to_string(),
                1,
                1,
                1
            )
        );
        let stored = get_application(&pool, "acct-jobs", &application.id)
            .unwrap()
            .unwrap();
        assert_eq!(stored_final_submit_proof(&stored).unwrap(), proof);

        let replay = claim_local_run_with_browser_release(
            &pool,
            &run_id,
            &ticket_hash,
            &nonce,
            &descriptor,
            "test-server",
            |_, _| anyhow::bail!("an exact claim replay must not issue fresh authority"),
        )
        .unwrap();
        let BrowserLocalRunClaimDisposition::Success(replay) = replay else {
            panic!("exact local claim replay did not recover")
        };
        assert!(replay.replayed);
        assert!(
            local_run_submit_authorized(&pool, &run_id, &ticket_hash, &proof, &capacity,).unwrap()
        );

        let session = list_browser_sessions(&pool, "acct-jobs")
            .unwrap()
            .into_iter()
            .find(|session| session.id == run_id)
            .unwrap();
        let unknown = finalize_local_side_effect_unknown(
            &pool,
            "acct-jobs",
            &application.id,
            &run_id,
            &ticket_hash,
            &capacity,
            json!({
                "status": "side_effect_unknown",
                "issues": [{
                    "field": "submission",
                    "message": "Submit was sent but its response was lost."
                }]
            }),
            &session,
        )
        .unwrap();
        assert_eq!(unknown.state, "side_effect_unknown");
        assert!(
            !local_run_submit_authorized(&pool, &run_id, &ticket_hash, &proof, &capacity,).unwrap()
        );
        let final_state: (String, String, i64, i64, i64) = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT ticket.status, application.state,
                        (SELECT COUNT(*) FROM jobs_application_ats_certification_bindings
                          WHERE application_id = ?1 AND run_id = ?2),
                        (SELECT COUNT(*) FROM jobs_ats_certification_canary_reservations),
                        (SELECT COUNT(*) FROM jobs_submission_evidence_capacity
                          WHERE account_id = 'acct-jobs' AND application_id = ?1
                            AND run_id = ?2)
                   FROM jobs_local_run_tickets ticket
                   JOIN jobs_applications application
                     ON application.account_id = ticket.account_id
                    AND application.id = ticket.application_id
                  WHERE ticket.id = ?2",
                params![application.id, run_id],
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
        assert_eq!(
            final_state,
            (
                "side_effect_unknown".to_string(),
                "side_effect_unknown".to_string(),
                1,
                1,
                1
            )
        );
    }

    #[test]
    fn certified_local_layout_drift_quarantines_before_any_submit_write() {
        let pool = test_pool();
        let browser = browser_release_registry_tests::install_signed_browser_release_fixture(
            &pool,
            "acct-jobs",
        );
        let fixture = certified_application_fixture(
            &pool,
            "certified-local-drift",
            "local",
            browser.runtime_target.clone(),
            "",
        );
        let application = fixture.application;
        let run_id = fixture.run_id;
        let ticket_hash = fixture
            .ticket_hash
            .expect("certified local fixture has a ticket hash");
        let proof = fixture.proof;
        let descriptor = browser.descriptor;
        let nonce = "b".repeat(64);
        assert!(matches!(
            claim_local_run_with_browser_release(
                &pool,
                &run_id,
                &ticket_hash,
                &nonce,
                &descriptor,
                "test-server",
                |_, _| Ok(json!({ "claim": "accepted" })),
            )
            .unwrap(),
            BrowserLocalRunClaimDisposition::Success(_)
        ));
        let mut capacity = test_submission_evidence_capacity(&application.id, &run_id);
        capacity.runner = "local".to_string();
        let unknown_layout = proof_with_unknown_layout(proof);
        for _ in 0..2 {
            assert!(!local_run_submit_authorized(
                &pool,
                &run_id,
                &ticket_hash,
                &unknown_layout,
                &capacity,
            )
            .unwrap());
        }

        let stored = get_application(&pool, "acct-jobs", &application.id)
            .unwrap()
            .unwrap();
        assert!(stored.receipt.get(FINAL_SUBMIT_PROOF_KEY).is_none());
        let state: (String, i64, String, i64, i64, i64) = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT binding.phase, binding.fence, ticket.status,
                        (SELECT COUNT(*)
                           FROM jobs_ats_certification_runtime_layout_quarantine_evidence),
                        (SELECT COUNT(*) FROM jobs_ats_certification_circuit_events
                          WHERE scope_kind = 'runtime'
                            AND subject_key = binding.runner_target_sha256),
                        (SELECT COUNT(*) FROM jobs_submission_evidence_capacity
                          WHERE account_id = 'acct-jobs' AND application_id = ?1
                            AND run_id = ?2)
                   FROM jobs_application_ats_certification_bindings binding
                   JOIN jobs_local_run_tickets ticket ON ticket.id = binding.run_id
                  WHERE binding.application_id = ?1 AND binding.run_id = ?2",
                params![application.id, run_id],
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
        assert_eq!(
            state,
            ("preflight".to_string(), 0, "claimed".to_string(), 1, 1, 0)
        );
    }

    #[test]
    fn certified_cloud_production_path_is_single_submit_even_after_response_loss() {
        let pool = test_pool();
        let fixture = certified_cloud_execution_fixture(&pool, "certified-cloud-production");
        let runtime = &fixture.runtime_target;
        let phase_a: (
            String,
            i64,
            String,
            String,
            String,
            String,
            Option<String>,
            Option<String>,
        ) = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT phase, fence, runner_target_sha256, platform, architecture,
                        automation_bundle_sha256, runner_build_id, runner_image_sha256
                   FROM jobs_application_ats_certification_bindings
                  WHERE account_id = 'acct-jobs' AND application_id = ?1 AND run_id = ?2",
                params![fixture.application.id, fixture.run_id],
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
                    ))
                },
            )
            .unwrap();
        assert_eq!(
            phase_a,
            (
                "preflight".to_string(),
                0,
                runtime.runtime_sha256.clone(),
                runtime.platform.clone(),
                runtime.architecture.clone(),
                runtime.automation_bundle_sha256.clone(),
                runtime.runner_build_id.clone(),
                runtime.runner_image_sha256.clone(),
            )
        );

        let capacity = test_submission_evidence_capacity(&fixture.application.id, &fixture.run_id);
        let started = start_irreversible_submission(
            &pool,
            "acct-jobs",
            &fixture.application.id,
            &fixture.run_id,
            &fixture.lease_token,
            fixture.lease_fence,
            &fixture.proof,
            &capacity,
        )
        .unwrap();
        let started_authority = started
            .ats_certified_receipt_authority
            .as_ref()
            .expect("certified cloud receipt authority");
        assert_eq!(started.phase, "click_started");
        assert_eq!(started_authority.runner_kind, "cloud");
        assert_eq!(
            started_authority.runner_target_sha256,
            runtime.runtime_sha256
        );
        assert_eq!(started_authority.binding_fence, 1);
        let stored = get_application(&pool, "acct-jobs", &fixture.application.id)
            .unwrap()
            .unwrap();
        assert_eq!(stored_final_submit_proof(&stored).unwrap(), fixture.proof);

        let recovered = start_irreversible_submission(
            &pool,
            "acct-jobs",
            &fixture.application.id,
            &fixture.run_id,
            &fixture.lease_token,
            fixture.lease_fence,
            &fixture.proof,
            &capacity,
        )
        .unwrap();
        assert_eq!(
            recovered.ats_certified_receipt_authority,
            started.ats_certified_receipt_authority
        );
        assert_eq!(
            certified_binding_state(&pool, &fixture.application.id, &fixture.run_id).0,
            "consumed"
        );
        assert_eq!(
            submission_capacity_count(&pool, &fixture.application.id, &fixture.run_id),
            1
        );

        finish_execution_lease(
            &pool,
            "acct-jobs",
            &fixture.application.id,
            &fixture.run_id,
            &fixture.lease_token,
            fixture.lease_fence,
            "side_effect_unknown",
        )
        .unwrap();
        assert!(matches!(
            start_irreversible_submission(
                &pool,
                "acct-jobs",
                &fixture.application.id,
                &fixture.run_id,
                &fixture.lease_token,
                fixture.lease_fence,
                &fixture.proof,
                &capacity,
            ),
            Err(ExecutionLeaseError::Conflict)
        ));
        let final_state: (String, String, i64, String, i64, i64) = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT lease.phase, binding.phase, binding.fence, capacity.state,
                        (SELECT COUNT(*) FROM jobs_ats_certification_canary_reservations
                          WHERE binding_id = binding.binding_id),
                        (SELECT COUNT(*) FROM jobs_submission_evidence_capacity
                          WHERE account_id = lease.account_id
                            AND application_id = lease.application_id
                            AND run_id = lease.run_id)
                   FROM jobs_execution_leases lease
                   JOIN jobs_application_ats_certification_bindings binding
                     ON binding.account_id = lease.account_id
                    AND binding.application_id = lease.application_id
                    AND binding.run_id = lease.run_id
                   JOIN jobs_submission_evidence_capacity capacity
                     ON capacity.account_id = lease.account_id
                    AND capacity.application_id = lease.application_id
                    AND capacity.run_id = lease.run_id
                  WHERE lease.application_id = ?1 AND lease.run_id = ?2",
                params![fixture.application.id, fixture.run_id],
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
        assert_eq!(
            final_state,
            (
                "side_effect_unknown".to_string(),
                "consumed".to_string(),
                1,
                "active".to_string(),
                1,
                1,
            )
        );
    }

    #[test]
    fn unmanaged_cloud_worker_submit_and_replay_accept_absent_managed_authority() {
        let pool = test_pool();
        let fixture =
            certified_cloud_execution_fixture(&pool, "unmanaged-worker-final-submit-pairing");
        let capacity = test_submission_evidence_capacity(&fixture.application.id, &fixture.run_id);
        let worker_id = "unmanaged-cloud-worker-final-submit";

        let started = start_irreversible_submission_authorized(
            &pool,
            "acct-jobs",
            &fixture.application.id,
            &fixture.run_id,
            &fixture.lease_token,
            fixture.lease_fence,
            &fixture.proof,
            &capacity,
            None,
            worker_id,
        )
        .expect("unmanaged authenticated worker may start with an absent managed tuple");
        assert_eq!(started.record.phase, "click_started");
        assert!(started.managed_cloud.is_none());

        let replay = start_irreversible_submission_authorized(
            &pool,
            "acct-jobs",
            &fixture.application.id,
            &fixture.run_id,
            &fixture.lease_token,
            fixture.lease_fence,
            &fixture.proof,
            &capacity,
            None,
            worker_id,
        )
        .expect("unmanaged authenticated worker may replay the exact stored outcome");
        assert_eq!(replay, started);
        assert_eq!(
            submission_capacity_count(&pool, &fixture.application.id, &fixture.run_id),
            1
        );
    }

    #[test]
    #[serial_test::serial]
    fn managed_cloud_final_submit_boundary_denies_omission_and_wrong_worker_without_mutation() {
        let _managed_cloud_environment = ManagedCloudAdmissionEnvironment::staging();
        let pool = test_pool();
        let fixture =
            managed_final_submit_execution_fixture(&pool, "managed-worker-final-submit-pairing");
        let cloud = &fixture.cloud;
        let capacity = test_submission_evidence_capacity(&cloud.application.id, &cloud.run_id);
        let prepared =
            managed_final_submit_boundary_state(&pool, &cloud.application.id, &cloud.run_id);
        assert_eq!(prepared.lease_phase, "prepared");
        assert_eq!(prepared.certification_phase, "preflight");
        assert_eq!(prepared.certification_fence, 0);
        assert!(prepared.phase_b_request_sha256.is_none());
        assert_eq!(prepared.canary_reservation_count, 0);
        assert_eq!(prepared.capacity_count, 0);
        assert_eq!(prepared.managed_receipt_count, 0);
        assert!(!prepared.final_submit_proof_present);

        assert!(matches!(
            start_irreversible_submission_authorized(
                &pool,
                "acct-jobs",
                &cloud.application.id,
                &cloud.run_id,
                &cloud.lease_token,
                cloud.lease_fence,
                &cloud.proof,
                &capacity,
                None,
                &fixture.worker_id,
            ),
            Err(ExecutionLeaseError::Conflict)
        ));
        assert_eq!(
            managed_final_submit_boundary_state(&pool, &cloud.application.id, &cloud.run_id,),
            prepared
        );

        let wrong_worker_id = "wrong-managed-worker-final-submit";
        assert!(matches!(
            start_irreversible_submission_authorized(
                &pool,
                "acct-jobs",
                &cloud.application.id,
                &cloud.run_id,
                &cloud.lease_token,
                cloud.lease_fence,
                &cloud.proof,
                &capacity,
                Some(&fixture.managed_cloud),
                wrong_worker_id,
            ),
            Err(ExecutionLeaseError::Conflict)
        ));
        assert_eq!(
            managed_final_submit_boundary_state(&pool, &cloud.application.id, &cloud.run_id,),
            prepared
        );

        let started = start_irreversible_submission_authorized(
            &pool,
            "acct-jobs",
            &cloud.application.id,
            &cloud.run_id,
            &cloud.lease_token,
            cloud.lease_fence,
            &cloud.proof,
            &capacity,
            Some(&fixture.managed_cloud),
            &fixture.worker_id,
        )
        .expect("exact managed authority starts the irreversible effect");
        assert_eq!(started.record.phase, "click_started");
        assert_eq!(
            started
                .managed_cloud
                .as_ref()
                .expect("managed fresh effect authority")
                .managed_cloud_worker_id,
            fixture.worker_id
        );
        let click_started =
            managed_final_submit_boundary_state(&pool, &cloud.application.id, &cloud.run_id);
        assert_eq!(click_started.lease_phase, "click_started");
        assert_eq!(click_started.certification_phase, "consumed");
        assert_eq!(click_started.certification_fence, 1);
        assert!(click_started.phase_b_request_sha256.is_some());
        assert_eq!(click_started.canary_reservation_count, 1);
        assert_eq!(click_started.capacity_count, 1);
        assert_eq!(click_started.managed_receipt_count, 1);
        assert!(click_started.final_submit_proof_present);

        assert!(matches!(
            start_irreversible_submission_authorized(
                &pool,
                "acct-jobs",
                &cloud.application.id,
                &cloud.run_id,
                &cloud.lease_token,
                cloud.lease_fence,
                &cloud.proof,
                &capacity,
                None,
                &fixture.worker_id,
            ),
            Err(ExecutionLeaseError::Conflict)
        ));
        assert_eq!(
            managed_final_submit_boundary_state(&pool, &cloud.application.id, &cloud.run_id,),
            click_started
        );

        assert!(matches!(
            start_irreversible_submission_authorized(
                &pool,
                "acct-jobs",
                &cloud.application.id,
                &cloud.run_id,
                &cloud.lease_token,
                cloud.lease_fence,
                &cloud.proof,
                &capacity,
                Some(&fixture.managed_cloud),
                wrong_worker_id,
            ),
            Err(ExecutionLeaseError::Conflict)
        ));
        assert_eq!(
            managed_final_submit_boundary_state(&pool, &cloud.application.id, &cloud.run_id,),
            click_started
        );

        let replay = start_irreversible_submission_authorized(
            &pool,
            "acct-jobs",
            &cloud.application.id,
            &cloud.run_id,
            &cloud.lease_token,
            cloud.lease_fence,
            &cloud.proof,
            &capacity,
            Some(&fixture.managed_cloud),
            &fixture.worker_id,
        )
        .expect("exact managed authority replays the stored irreversible effect");
        assert_eq!(replay, started);
        assert_eq!(
            managed_final_submit_boundary_state(&pool, &cloud.application.id, &cloud.run_id,),
            click_started
        );
    }

    #[test]
    fn execution_lease_claim_rechecks_public_beta_before_any_claim_mutation() {
        let pool = test_pool();
        let (application, run_id, browser_profile_id) =
            execution_lease_fixture(&pool, "phase-622-public-beta-claim-fence");
        pool.get()
            .unwrap()
            .execute(
                "INSERT INTO jobs_public_beta_overrides (
                    cohort_id, account_id, denied, revision, created_at_ms, updated_at_ms
                 ) VALUES ('public-v1', 'acct-jobs', 1, 1, 1, 1)",
                [],
            )
            .unwrap();

        assert!(matches!(
            claim_execution_lease(
                &pool,
                "acct-jobs",
                &application.id,
                &run_id,
                &browser_profile_id,
                "phase-622-denied-claim-worker",
            ),
            Err(ExecutionLeaseError::Conflict)
        ));
        assert_eq!(
            pool.get()
                .unwrap()
                .query_row(
                    "SELECT COUNT(*) FROM jobs_execution_leases WHERE run_id = ?1",
                    params![run_id],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
            0
        );

        pool.get()
            .unwrap()
            .execute(
                "UPDATE jobs_public_beta_overrides
                    SET denied = 0, revision = 2, updated_at_ms = 2
                  WHERE cohort_id = 'public-v1' AND account_id = 'acct-jobs'",
                [],
            )
            .unwrap();
        claim_execution_lease(
            &pool,
            "acct-jobs",
            &application.id,
            &run_id,
            &browser_profile_id,
            "phase-622-authorized-claim-worker",
        )
        .expect("restored public-beta authority permits a fresh execution lease claim");
    }

    #[test]
    #[serial_test::serial]
    fn managed_effect_rechecks_public_beta_before_new_authority_and_submit() {
        let _managed_cloud_environment = ManagedCloudAdmissionEnvironment::staging();
        let pool = test_pool();
        let fixture =
            managed_final_submit_execution_fixture(&pool, "phase-622-public-beta-effect-authority");
        let cloud = &fixture.cloud;
        let capacity = test_submission_evidence_capacity(&cloud.application.id, &cloud.run_id);
        let prepared =
            managed_final_submit_boundary_state(&pool, &cloud.application.id, &cloud.run_id);

        pool.get()
            .unwrap()
            .execute(
                "INSERT INTO jobs_public_beta_overrides (
                    cohort_id, account_id, denied, revision, created_at_ms, updated_at_ms
                 ) VALUES ('public-v1', 'acct-jobs', 1, 1, 1, 1)",
                [],
            )
            .unwrap();
        assert!(matches!(
            authorize_managed_execution_effect(
                &pool,
                "acct-jobs",
                &cloud.application.id,
                &cloud.run_id,
                &cloud.lease_token,
                cloud.lease_fence,
                &fixture.managed_cloud,
                &fixture.worker_id,
            ),
            Err(ExecutionLeaseError::Conflict)
        ));
        assert_eq!(
            managed_final_submit_boundary_state(&pool, &cloud.application.id, &cloud.run_id,),
            prepared
        );

        pool.get()
            .unwrap()
            .execute(
                "UPDATE jobs_public_beta_overrides
                    SET denied = 0, revision = 2, updated_at_ms = 2
                  WHERE cohort_id = 'public-v1' AND account_id = 'acct-jobs'",
                [],
            )
            .unwrap();
        authorize_managed_execution_effect(
            &pool,
            "acct-jobs",
            &cloud.application.id,
            &cloud.run_id,
            &cloud.lease_token,
            cloud.lease_fence,
            &fixture.managed_cloud,
            &fixture.worker_id,
        )
        .expect("an admitted account retains the exact managed-effect authority");

        pool.get()
            .unwrap()
            .execute(
                "UPDATE jobs_public_beta_cohorts
                    SET state = 'suspended', revision = revision + 1
                  WHERE id = 'public-v1'",
                [],
            )
            .unwrap();
        assert!(matches!(
            start_irreversible_submission_authorized(
                &pool,
                "acct-jobs",
                &cloud.application.id,
                &cloud.run_id,
                &cloud.lease_token,
                cloud.lease_fence,
                &cloud.proof,
                &capacity,
                Some(&fixture.managed_cloud),
                &fixture.worker_id,
            ),
            Err(ExecutionLeaseError::Conflict)
        ));
        assert_eq!(
            managed_final_submit_boundary_state(&pool, &cloud.application.id, &cloud.run_id,),
            prepared
        );

        pool.get()
            .unwrap()
            .execute(
                "UPDATE jobs_public_beta_cohorts
                    SET state = 'closed_to_new', revision = revision + 1
                  WHERE id = 'public-v1'",
                [],
            )
            .unwrap();
        let started = start_irreversible_submission_authorized(
            &pool,
            "acct-jobs",
            &cloud.application.id,
            &cloud.run_id,
            &cloud.lease_token,
            cloud.lease_fence,
            &cloud.proof,
            &capacity,
            Some(&fixture.managed_cloud),
            &fixture.worker_id,
        )
        .expect("fresh public-beta authority permits the exact irreversible transition");
        assert_eq!(started.record.phase, "click_started");

        pool.get()
            .unwrap()
            .execute_batch(
                "UPDATE jobs_public_beta_overrides
                    SET denied = 1, revision = 3, updated_at_ms = 3
                  WHERE cohort_id = 'public-v1' AND account_id = 'acct-jobs';
                 UPDATE jobs_public_beta_cohorts
                    SET state = 'suspended', revision = revision + 1
                  WHERE id = 'public-v1';",
            )
            .unwrap();
        let replay = start_irreversible_submission_authorized(
            &pool,
            "acct-jobs",
            &cloud.application.id,
            &cloud.run_id,
            &cloud.lease_token,
            cloud.lease_fence,
            &cloud.proof,
            &capacity,
            Some(&fixture.managed_cloud),
            &fixture.worker_id,
        )
        .expect("an exact click-started replay is not a new employer effect");
        assert_eq!(replay, started);
    }

    #[test]
    fn local_claim_rechecks_public_beta_without_stranding_exact_release_replay() {
        let pool = test_pool();
        let (application, run_id, ticket_hash, _) =
            local_run_authority_fixture_unbound(&pool, "phase-622-local-claim-beta-fence");
        reserve_application_attempt(&pool, "acct-jobs", &application.id, "local").unwrap();
        let descriptor = seed_test_local_browser_release_authority(&pool);
        let nonce = "a".repeat(64);
        let queued = local_claim_state(&pool, &application.id, &run_id);

        pool.get()
            .unwrap()
            .execute(
                "UPDATE jobs_public_beta_cohorts
                    SET state = 'suspended', revision = revision + 1
                  WHERE id = 'public-v1'",
                [],
            )
            .unwrap();
        assert!(claim_authorized_local_run_ticket(&pool, &run_id, &ticket_hash)
            .unwrap()
            .is_none());
        assert_eq!(local_claim_state(&pool, &application.id, &run_id), queued);
        assert!(matches!(
            claim_local_run_with_browser_release(
                &pool,
                &run_id,
                &ticket_hash,
                &nonce,
                &descriptor,
                "test-server",
                |_, _| Ok(json!({ "unexpected": true })),
            )
            .unwrap(),
            BrowserLocalRunClaimDisposition::Rejected
        ));
        assert_eq!(local_claim_state(&pool, &application.id, &run_id), queued);

        pool.get()
            .unwrap()
            .execute(
                "UPDATE jobs_public_beta_cohorts
                    SET state = 'closed_to_new', revision = revision + 1
                  WHERE id = 'public-v1'",
                [],
            )
            .unwrap();
        let first = claim_local_run_with_browser_release(
            &pool,
            &run_id,
            &ticket_hash,
            &nonce,
            &descriptor,
            "test-server",
            |ticket, _| Ok(json!({ "runId": ticket.id })),
        )
        .unwrap();
        let BrowserLocalRunClaimDisposition::Success(first) = first else {
            panic!("restored public-beta authority did not permit the fresh local claim")
        };
        assert!(!first.replayed);
        let claimed = local_claim_state(&pool, &application.id, &run_id);

        pool.get()
            .unwrap()
            .execute_batch(
                "INSERT INTO jobs_public_beta_overrides (
                    cohort_id, account_id, denied, revision, created_at_ms, updated_at_ms
                 ) VALUES ('public-v1', 'acct-jobs', 1, 1, 1, 1);
                 UPDATE jobs_public_beta_cohorts
                    SET state = 'suspended', revision = revision + 1
                  WHERE id = 'public-v1';",
            )
            .unwrap();
        let replay = claim_local_run_with_browser_release(
            &pool,
            &run_id,
            &ticket_hash,
            &nonce,
            &descriptor,
            "test-server",
            |_, _| anyhow::bail!("exact local claim replay must not issue fresh authority"),
        )
        .unwrap();
        let BrowserLocalRunClaimDisposition::Success(replay) = replay else {
            panic!("public-beta closure stranded an exact local claim replay")
        };
        assert!(replay.replayed);
        assert_eq!(replay.response_json, first.response_json);
        assert_eq!(local_claim_state(&pool, &application.id, &run_id), claimed);
    }

    #[test]
    fn local_submit_rechecks_public_beta_without_stranding_click_started_replay() {
        let pool = test_pool();
        let (application, run_id, ticket_hash, proof) =
            certified_local_intervention_fixture(&pool, "phase-622-local-submit-beta-fence");
        let mut capacity = test_submission_evidence_capacity(&application.id, &run_id);
        capacity.runner = "local".to_string();
        let claimed = local_claim_state(&pool, &application.id, &run_id);

        pool.get()
            .unwrap()
            .execute(
                "INSERT INTO jobs_public_beta_overrides (
                    cohort_id, account_id, denied, revision, created_at_ms, updated_at_ms
                 ) VALUES ('public-v1', 'acct-jobs', 1, 1, 1, 1)",
                [],
            )
            .unwrap();
        assert!(!local_run_submit_authorized(
            &pool,
            &run_id,
            &ticket_hash,
            &proof,
            &capacity,
        )
        .unwrap());
        assert_eq!(local_claim_state(&pool, &application.id, &run_id), claimed);
        assert_eq!(submission_capacity_count(&pool, &application.id, &run_id), 0);
        assert!(get_application(&pool, "acct-jobs", &application.id)
            .unwrap()
            .unwrap()
            .receipt
            .get(FINAL_SUBMIT_PROOF_KEY)
            .is_none());

        pool.get()
            .unwrap()
            .execute(
                "UPDATE jobs_public_beta_overrides
                    SET denied = 0, revision = 2, updated_at_ms = 2
                  WHERE cohort_id = 'public-v1' AND account_id = 'acct-jobs'",
                [],
            )
            .unwrap();
        let started = local_run_submit_authorization_inner(
            &pool,
            &run_id,
            &ticket_hash,
            "test-server",
            &proof,
            &capacity,
            false,
        )
        .unwrap()
        .expect("restored public-beta authority permits the fresh pre-click transition");

        pool.get()
            .unwrap()
            .execute_batch(
                "UPDATE jobs_public_beta_overrides
                    SET denied = 1, revision = 3, updated_at_ms = 3
                  WHERE cohort_id = 'public-v1' AND account_id = 'acct-jobs';
                 UPDATE jobs_public_beta_cohorts
                    SET state = 'suspended', revision = revision + 1
                  WHERE id = 'public-v1';
                 INSERT INTO account_deletion_intents (
                    account_id, requested_at_ms, last_checked_at_ms,
                    fresh_upload_cutoff_ms, fresh_in_flight_puts
                 ) VALUES ('acct-jobs', 1, 1, 0, 0);",
            )
            .unwrap();
        let replay = local_run_submit_authorization_for_distribution(
            &pool,
            &run_id,
            &ticket_hash,
            "test-server",
            &proof,
            &capacity,
        )
        .unwrap()
        .expect("public-beta closure must not strand exact click-started replay");
        assert_eq!(replay, started);
        assert_eq!(submission_capacity_count(&pool, &application.id, &run_id), 1);
    }

    #[test]
    fn certified_cloud_layout_drift_quarantines_before_any_submit_write() {
        let pool = test_pool();
        let fixture = certified_cloud_execution_fixture(&pool, "certified-cloud-drift");
        let capacity = test_submission_evidence_capacity(&fixture.application.id, &fixture.run_id);
        let unknown_layout = proof_with_unknown_layout(fixture.proof);
        for _ in 0..2 {
            assert!(matches!(
                start_irreversible_submission(
                    &pool,
                    "acct-jobs",
                    &fixture.application.id,
                    &fixture.run_id,
                    &fixture.lease_token,
                    fixture.lease_fence,
                    &unknown_layout,
                    &capacity,
                ),
                Err(ExecutionLeaseError::Conflict)
            ));
        }

        let stored = get_application(&pool, "acct-jobs", &fixture.application.id)
            .unwrap()
            .unwrap();
        assert!(stored.receipt.get(FINAL_SUBMIT_PROOF_KEY).is_none());
        let state: (String, i64, String, i64, i64, i64) = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT binding.phase, binding.fence, lease.phase,
                        (SELECT COUNT(*)
                           FROM jobs_ats_certification_runtime_layout_quarantine_evidence),
                        (SELECT COUNT(*) FROM jobs_ats_certification_circuit_events
                          WHERE scope_kind = 'runtime'
                            AND subject_key = binding.runner_target_sha256),
                        (SELECT COUNT(*) FROM jobs_submission_evidence_capacity
                          WHERE account_id = 'acct-jobs' AND application_id = ?1
                            AND run_id = ?2)
                   FROM jobs_application_ats_certification_bindings binding
                   JOIN jobs_execution_leases lease ON lease.run_id = binding.run_id
                  WHERE binding.application_id = ?1 AND binding.run_id = ?2",
                params![fixture.application.id, fixture.run_id],
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
        assert_eq!(
            state,
            ("preflight".to_string(), 0, "prepared".to_string(), 1, 1, 0,)
        );
    }

    #[test]
    fn local_release_claim_is_atomic_exactly_replayable_and_revocation_fences_submit() {
        let pool = test_pool();
        let (application, run_id, ticket_hash, _) =
            local_run_authority_fixture_unbound(&pool, "release-claim-atomic");
        reserve_application_attempt(&pool, "acct-jobs", &application.id, "local").unwrap();
        let descriptor = test_browser_release_descriptor();
        let nonce = "a".repeat(64);
        let queued = local_claim_state(&pool, &application.id, &run_id);

        let distribution_unavailable = claim_local_run_with_browser_release_for_distribution(
            &pool,
            BrowserLocalRunDistributionClaim {
                run_id: &run_id,
                ticket_hash: &ticket_hash,
                claim_nonce: &nonce,
                descriptor: &descriptor,
                server_release_id: "test-server",
                distribution_enabled: false,
            },
            |_, _| Ok(json!({ "claim": "accepted" })),
        )
        .unwrap();
        assert!(matches!(
            distribution_unavailable,
            BrowserLocalRunClaimDisposition::DistributionUnavailable
        ));
        assert_eq!(local_claim_state(&pool, &application.id, &run_id), queued);

        let unavailable = claim_local_run_with_browser_release(
            &pool,
            &run_id,
            &ticket_hash,
            &nonce,
            &descriptor,
            "test-server",
            |_, _| Ok(json!({ "claim": "accepted" })),
        )
        .unwrap();
        assert!(
            matches!(
                &unavailable,
                BrowserLocalRunClaimDisposition::ReleaseUnavailable
            ),
            "unexpected claim disposition: {unavailable:?}; initial state: {queued:?}"
        );
        assert_eq!(local_claim_state(&pool, &application.id, &run_id), queued);

        seed_test_local_browser_release_authority(&pool);
        let mut wrong_protocol = descriptor.clone();
        wrong_protocol.protocol_version = 2;
        let unavailable = claim_local_run_with_browser_release(
            &pool,
            &run_id,
            &ticket_hash,
            &nonce,
            &wrong_protocol,
            "test-server",
            |_, _| Ok(json!({ "claim": "accepted" })),
        )
        .unwrap();
        assert!(
            matches!(
                &unavailable,
                BrowserLocalRunClaimDisposition::ReleaseUnavailable
            ),
            "unexpected wrong-protocol disposition: {unavailable:?}"
        );
        assert_eq!(local_claim_state(&pool, &application.id, &run_id), queued);

        let issue_failure = claim_local_run_with_browser_release(
            &pool,
            &run_id,
            &ticket_hash,
            &nonce,
            &descriptor,
            "test-server",
            |_, _| anyhow::bail!("simulated capability issuance failure"),
        );
        assert!(issue_failure.is_err());
        assert_eq!(local_claim_state(&pool, &application.id, &run_id), queued);

        let first = claim_local_run_with_browser_release(
            &pool,
            &run_id,
            &ticket_hash,
            &nonce,
            &descriptor,
            "test-server",
            |ticket, release| {
                Ok(json!({
                    "runId": ticket.id,
                    "descriptorSha256": release.build_descriptor_sha256,
                    "nonceBoundCapability": "capability-with-random-nonce",
                }))
            },
        )
        .unwrap();
        let BrowserLocalRunClaimDisposition::Success(first) = first else {
            panic!("valid Browser release claim did not succeed")
        };
        assert!(!first.replayed);
        assert_eq!(
            local_claim_state(&pool, &application.id, &run_id),
            (
                "claimed".to_string(),
                "running".to_string(),
                "running".to_string(),
                "running".to_string(),
                1,
                1,
            )
        );

        let replay = claim_local_run_with_browser_release(
            &pool,
            &run_id,
            &ticket_hash,
            &nonce,
            &descriptor,
            "test-server",
            |_, _| anyhow::bail!("replay must not issue new capabilities"),
        )
        .unwrap();
        let BrowserLocalRunClaimDisposition::Success(replay) = replay else {
            panic!("exact Browser release claim replay did not succeed")
        };
        assert!(replay.replayed);
        assert_eq!(
            replay.response_json.as_bytes(),
            first.response_json.as_bytes()
        );
        assert_eq!(local_claim_state(&pool, &application.id, &run_id).4, 1);

        let hidden_replay = claim_local_run_with_browser_release(
            &pool,
            &run_id,
            "wrong-ticket-hash",
            &"b".repeat(64),
            &descriptor,
            "test-server",
            |_, _| Ok(json!({ "claim": "different" })),
        )
        .unwrap();
        assert!(matches!(
            hidden_replay,
            BrowserLocalRunClaimDisposition::Rejected
        ));

        let conflict = claim_local_run_with_browser_release(
            &pool,
            &run_id,
            &ticket_hash,
            &"b".repeat(64),
            &descriptor,
            "test-server",
            |_, _| Ok(json!({ "claim": "different" })),
        )
        .unwrap();
        assert!(matches!(
            conflict,
            BrowserLocalRunClaimDisposition::ConflictingReplay
        ));

        let running = get_application(&pool, "acct-jobs", &application.id)
            .unwrap()
            .unwrap();
        let mut capacity = test_submission_evidence_capacity(&application.id, &run_id);
        capacity.runner = "local".to_string();
        assert!(!local_run_submit_authorized_for_distribution(
            &pool,
            &run_id,
            &ticket_hash,
            "test-server",
            &test_final_submit_proof(&running),
            &capacity,
        )
        .unwrap());
        assert!(!local_run_submit_authorized_for_server(
            &pool,
            &run_id,
            &ticket_hash,
            "different-server",
            &test_final_submit_proof(&running),
            &capacity,
        )
        .unwrap());
        assert_eq!(
            submission_capacity_count(&pool, &application.id, &run_id),
            0
        );

        seed_test_browser_release_revocation(
            &pool,
            &"d".repeat(64),
            "test-artifact-revocation",
            "artifact",
            "test-artifact",
            &"2".repeat(64),
        );
        assert!(!local_run_submit_authorized(
            &pool,
            &run_id,
            &ticket_hash,
            &test_final_submit_proof(&running),
            &capacity,
        )
        .unwrap());
        assert_eq!(
            submission_capacity_count(&pool, &application.id, &run_id),
            0
        );
        assert_eq!(
            local_claim_state(&pool, &application.id, &run_id).0,
            "claimed"
        );
    }

    #[test]
    fn local_browser_release_metadata_requires_complete_non_revoked_artifact_set() {
        let pool = test_pool();
        seed_test_local_browser_release_authority(&pool);
        assert!(matches!(
            local_browser_release_availability_for_distribution(&pool, "acct-jobs", "test-server",)
                .unwrap(),
            LocalBrowserReleaseAvailability::Disabled { .. }
        ));
        let availability =
            local_browser_release_availability(&pool, "acct-jobs", "test-server").unwrap();
        let serialized = serde_json::to_value(&availability).unwrap();
        assert_eq!(serialized["release_id"], "test-release");
        assert_eq!(serialized["artifact_origin"], "https://bluey.sh");
        assert!(serialized.get("releaseId").is_none());
        assert!(serialized.get("artifactOrigin").is_none());
        let LocalBrowserReleaseAvailability::Available {
            channel,
            release_id,
            artifact_origin,
            manifest_sha256,
            artifacts,
            ..
        } = availability
        else {
            panic!("complete Browser release was not available")
        };
        assert_eq!(channel, "beta");
        assert_eq!(release_id, "test-release");
        assert_eq!(artifact_origin, "https://bluey.sh");
        assert_eq!(manifest_sha256, "1".repeat(64));
        assert_eq!(artifacts.len(), 5);
        assert_eq!(
            artifacts
                .iter()
                .filter(|artifact| artifact.role == "installer")
                .count(),
            3
        );
        assert!(artifacts.iter().all(|artifact| {
            artifact
                .url
                .starts_with("https://bluey.sh/jobs/browser/releases/test-release/")
                && !artifact.url.contains("/download")
        }));
        let connection = pool.get().unwrap();
        connection
            .execute_batch("DROP TRIGGER trg_jobs_browser_release_artifacts_no_update")
            .unwrap();
        connection
            .execute(
                "UPDATE jobs_browser_release_artifacts
                    SET artifact_url = replace(artifact_url, 'https://bluey.sh',
                                               'https://artifacts.example')
                  WHERE artifact_id = 'test-artifact'",
                [],
            )
            .unwrap();
        drop(connection);
        assert!(matches!(
            local_browser_release_availability(&pool, "acct-jobs", "test-server").unwrap(),
            LocalBrowserReleaseAvailability::Unavailable { .. }
        ));
        pool.get()
            .unwrap()
            .execute(
                "UPDATE jobs_browser_release_artifacts
                    SET artifact_url = replace(artifact_url, 'https://artifacts.example',
                                               'https://bluey.sh')
                  WHERE artifact_id = 'test-artifact'",
                [],
            )
            .unwrap();
        assert!(matches!(
            local_browser_release_availability(&pool, "acct-jobs", "test-server").unwrap(),
            LocalBrowserReleaseAvailability::Available { .. }
        ));
        pool.get()
            .unwrap()
            .execute(
                "UPDATE jobs_browser_release_artifacts
                    SET artifact_url = replace(artifact_url, '.dmg', '.zip'),
                        artifact_filename = replace(artifact_filename, '.dmg', '.zip')
                  WHERE artifact_id = 'test-artifact'",
                [],
            )
            .unwrap();
        assert!(matches!(
            local_browser_release_availability(&pool, "acct-jobs", "test-server").unwrap(),
            LocalBrowserReleaseAvailability::Unavailable { .. }
        ));
        pool.get()
            .unwrap()
            .execute(
                "UPDATE jobs_browser_release_artifacts
                    SET artifact_url = replace(artifact_url, '.zip', '.dmg'),
                        artifact_filename = replace(artifact_filename, '.zip', '.dmg')
                  WHERE artifact_id = 'test-artifact'",
                [],
            )
            .unwrap();
        pool.get()
            .unwrap()
            .execute(
                "UPDATE jobs_browser_release_artifacts SET app_content_sha256 = ?1
                  WHERE artifact_id = 'test-artifact-darwin-arm64-zip'",
                params!["7".repeat(64)],
            )
            .unwrap();
        assert!(matches!(
            local_browser_release_availability(&pool, "acct-jobs", "test-server").unwrap(),
            LocalBrowserReleaseAvailability::Unavailable { .. }
        ));
        pool.get()
            .unwrap()
            .execute(
                "UPDATE jobs_browser_release_artifacts SET app_content_sha256 = ?1
                  WHERE artifact_id = 'test-artifact-darwin-arm64-zip'",
                params!["8".repeat(64)],
            )
            .unwrap();
        pool.get()
            .unwrap()
            .execute(
                "UPDATE jobs_browser_release_artifacts
                    SET artifact_url =
                          'https://bluey.sh/jobs/browser/releases/test-release/Bluey-Browser.dmg',
                        artifact_filename = 'Bluey-Browser.dmg'
                  WHERE artifact_id = 'test-artifact-darwin-x64-dmg'",
                [],
            )
            .unwrap();
        assert!(matches!(
            local_browser_release_availability(&pool, "acct-jobs", "test-server").unwrap(),
            LocalBrowserReleaseAvailability::Unavailable { .. }
        ));
        pool.get()
            .unwrap()
            .execute(
                "UPDATE jobs_browser_release_artifacts
                    SET artifact_url =
                          'https://bluey.sh/jobs/browser/releases/test-release/Bluey-Browser-x64.dmg',
                        artifact_filename = 'Bluey-Browser-x64.dmg'
                  WHERE artifact_id = 'test-artifact-darwin-x64-dmg'",
                [],
            )
            .unwrap();
        assert!(matches!(
            local_browser_release_availability(&pool, "acct-jobs", "test-server").unwrap(),
            LocalBrowserReleaseAvailability::Available { .. }
        ));
        seed_test_browser_release_revocation(
            &pool,
            &"d".repeat(64),
            "test-manifest-revocation",
            "manifest",
            "test-manifest",
            &"1".repeat(64),
        );
        assert!(matches!(
            local_browser_release_availability(&pool, "acct-jobs", "test-server").unwrap(),
            LocalBrowserReleaseAvailability::Unavailable { .. }
        ));
    }

    #[test]
    fn current_policy_artifact_origin_fences_availability_and_new_claims_atomically() {
        let pool = test_pool();
        let (application, run_id, ticket_hash, _) =
            local_run_authority_fixture_unbound(&pool, "current-artifact-origin");
        reserve_application_attempt(&pool, "acct-jobs", &application.id, "local").unwrap();
        let descriptor = seed_test_local_browser_release_authority(&pool);
        rotate_test_browser_release_artifact_origin(&pool, "https://artifacts.example");

        assert!(matches!(
            local_browser_release_availability(&pool, "acct-jobs", "test-server").unwrap(),
            LocalBrowserReleaseAvailability::Unavailable { .. }
        ));
        let before = local_claim_state(&pool, &application.id, &run_id);
        assert!(matches!(
            claim_local_run_with_browser_release(
                &pool,
                &run_id,
                &ticket_hash,
                &"a".repeat(64),
                &descriptor,
                "test-server",
                |_, _| Ok(json!({ "unexpected": true })),
            )
            .unwrap(),
            BrowserLocalRunClaimDisposition::ReleaseUnavailable
        ));
        assert_eq!(local_claim_state(&pool, &application.id, &run_id), before);
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
        let mut capacity = test_submission_evidence_capacity(&application.id, &run_id);
        capacity.runner = "local".to_string();
        set_entitlement_plan(&pool, "acct-jobs", "free").unwrap();
        let final_submit_proof = test_final_submit_proof(&application);
        assert!(!local_run_submit_authorized(
            &pool,
            &run_id,
            &ticket_hash,
            &final_submit_proof,
            &capacity,
        )
        .unwrap());
        set_entitlement_plan(&pool, "acct-jobs", "pro").unwrap();
        assert!(local_run_submit_authorized(
            &pool,
            &run_id,
            &ticket_hash,
            &final_submit_proof,
            &capacity,
        )
        .unwrap());
        let (ticket_status, capacity_state): (String, String) = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT ticket.status, capacity.state
                   FROM jobs_local_run_tickets ticket
                   JOIN jobs_submission_evidence_capacity capacity
                     ON capacity.account_id = ticket.account_id
                    AND capacity.application_id = ticket.application_id
                    AND capacity.run_id = ticket.id
                  WHERE ticket.id = ?1",
                params![run_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(ticket_status, "click_started");
        assert_eq!(capacity_state, "active");
        assert!(!local_run_submit_authorized(
            &pool,
            &run_id,
            &ticket_hash,
            &final_submit_proof,
            &capacity,
        )
        .unwrap());

        pool.get()
            .unwrap()
            .execute(
                "DELETE FROM jobs_application_identities WHERE account_id = ?1 AND id = ?2",
                params!["acct-jobs", identity_id],
            )
            .unwrap();
        assert!(!local_run_submit_authorized(
            &pool,
            &run_id,
            &ticket_hash,
            &final_submit_proof,
            &capacity,
        )
        .unwrap());
    }

    #[test]
    fn local_click_started_ticket_rejects_retryable_status_downgrades() {
        for status in ["failed", "needs_input"] {
            let pool = test_pool();
            let suffix = format!("click-downgrade-{status}");
            let (application, run_id, ticket_hash) = local_click_started_fixture(&pool, &suffix);
            let application_before = serde_json::to_value(&application).unwrap();

            assert!(
                !update_local_run_ticket_status(&pool, &run_id, &ticket_hash, status).unwrap(),
                "click_started must not become {status}"
            );
            let (ticket_status, capacity_state): (String, String) = pool
                .get()
                .unwrap()
                .query_row(
                    "SELECT ticket.status, capacity.state
                       FROM jobs_local_run_tickets ticket
                       JOIN jobs_submission_evidence_capacity capacity
                         ON capacity.account_id = ticket.account_id
                        AND capacity.application_id = ticket.application_id
                        AND capacity.run_id = ticket.id
                      WHERE ticket.id = ?1",
                    params![run_id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .unwrap();
            assert_eq!(ticket_status, "click_started");
            assert_eq!(capacity_state, "active");
            let stored = get_application(&pool, "acct-jobs", &application.id)
                .unwrap()
                .unwrap();
            assert_eq!(serde_json::to_value(stored).unwrap(), application_before);
        }
    }

    #[test]
    fn expired_click_started_result_enters_side_effect_unknown_within_grace() {
        let pool = test_pool();
        let (application, run_id, ticket_hash) =
            local_click_started_fixture(&pool, "expired-click-reconciliation");
        let expired_at_ms = now_ms().saturating_sub(1);
        pool.get()
            .unwrap()
            .execute(
                "UPDATE jobs_local_run_tickets SET expires_at_ms = ?2 WHERE id = ?1",
                params![run_id, expired_at_ms],
            )
            .unwrap();
        let session = list_browser_sessions(&pool, "acct-jobs")
            .unwrap()
            .into_iter()
            .find(|candidate| candidate.id == run_id)
            .unwrap();
        let mut capacity = test_submission_evidence_capacity(&application.id, &run_id);
        capacity.runner = "local".to_string();
        let finalized = finalize_local_side_effect_unknown(
            &pool,
            "acct-jobs",
            &application.id,
            &run_id,
            &ticket_hash,
            &capacity,
            json!({
                "status": "side_effect_unknown",
                "issues": [{
                    "field": "submission",
                    "message": "The Browser restarted after Submit without an employer response."
                }]
            }),
            &session,
        )
        .unwrap();
        assert_eq!(finalized.state, "side_effect_unknown");
        let (ticket_status, attempt_status, session_status): (String, String, String) = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT ticket.status, attempt.status, session.status
                   FROM jobs_local_run_tickets ticket
                   JOIN jobs_attempt_reservations attempt
                     ON attempt.account_id = ticket.account_id
                    AND attempt.application_id = ticket.application_id
                   JOIN jobs_browser_sessions session
                     ON session.account_id = ticket.account_id
                    AND session.id = ticket.id
                  WHERE ticket.id = ?1",
                params![run_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(ticket_status, "side_effect_unknown");
        assert_eq!(attempt_status, "side_effect_unknown");
        assert_eq!(session_status, "needs_input");
    }

    #[test]
    fn local_unknown_before_authorize_reserves_capacity_and_replays_exactly() {
        for initial_ticket_status in ["claimed", "needs_input"] {
            let pool = test_pool();
            let suffix = format!("unknown-before-authorize-{initial_ticket_status}");
            let (application, run_id, ticket_hash, _) = local_run_authority_fixture(&pool, &suffix);
            reserve_application_attempt(&pool, "acct-jobs", &application.id, "local").unwrap();
            assert!(
                claim_authorized_local_run_ticket(&pool, &run_id, &ticket_hash)
                    .unwrap()
                    .is_some()
            );
            let mut application =
                update_application(&pool, "acct-jobs", &application.id, "running", None)
                    .unwrap()
                    .unwrap();
            let mut session = BrowserSession {
                id: run_id.clone(),
                runner: "local".to_string(),
                status: "running".to_string(),
                current_company: "Acme".to_string(),
                current_step: "Posting local result".to_string(),
                application_id: Some(application.id.clone()),
                takeover_url: None,
                created_at_ms: 0,
                updated_at_ms: 0,
            };
            if initial_ticket_status == "needs_input" {
                application =
                    update_application(&pool, "acct-jobs", &application.id, "needs_input", None)
                        .unwrap()
                        .unwrap();
                session.status = "needs_input".to_string();
                assert!(update_local_run_ticket_status(
                    &pool,
                    &run_id,
                    &ticket_hash,
                    "needs_input",
                )
                .unwrap());
            }
            upsert_browser_session(&pool, "acct-jobs", &session).unwrap();
            let capacity_count: i64 = pool
                .get()
                .unwrap()
                .query_row(
                    "SELECT COUNT(*) FROM jobs_submission_evidence_capacity
                      WHERE account_id = 'acct-jobs' AND application_id = ?1 AND run_id = ?2",
                    params![application.id, run_id],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(capacity_count, 0);

            let mut capacity = test_submission_evidence_capacity(&application.id, &run_id);
            capacity.runner = "local".to_string();
            let receipt = json!({
                "status": "side_effect_unknown",
                "issues": [{
                    "field": "submission",
                    "message": "The submit response was lost."
                }]
            });
            let finalized = finalize_local_side_effect_unknown(
                &pool,
                "acct-jobs",
                &application.id,
                &run_id,
                &ticket_hash,
                &capacity,
                receipt.clone(),
                &session,
            )
            .unwrap();
            assert_eq!(finalized.state, "side_effect_unknown");
            let finalized_value = serde_json::to_value(&finalized).unwrap();
            let (
                ticket_status,
                application_state,
                attempt_status,
                session_status,
                capacity_runner,
                reserved_bytes,
                reserved_objects,
                capacity_state,
                capacity_expiry,
                ticket_expiry,
            ): (
                String,
                String,
                String,
                String,
                String,
                i64,
                i64,
                String,
                i64,
                i64,
            ) = pool
                .get()
                .unwrap()
                .query_row(
                    "SELECT ticket.status, application.state, attempt.status, session.status,
                            capacity.runner, capacity.reserved_bytes,
                            capacity.reserved_objects, capacity.state,
                            capacity.expires_at_ms, ticket.expires_at_ms
                       FROM jobs_local_run_tickets ticket
                       JOIN jobs_applications application
                         ON application.account_id = ticket.account_id
                        AND application.id = ticket.application_id
                       JOIN jobs_attempt_reservations attempt
                         ON attempt.account_id = ticket.account_id
                        AND attempt.application_id = ticket.application_id
                       JOIN jobs_browser_sessions session
                         ON session.account_id = ticket.account_id AND session.id = ticket.id
                       JOIN jobs_submission_evidence_capacity capacity
                         ON capacity.account_id = ticket.account_id
                        AND capacity.application_id = ticket.application_id
                        AND capacity.run_id = ticket.id
                      WHERE ticket.id = ?1",
                    params![run_id],
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
                        ))
                    },
                )
                .unwrap();
            assert_eq!(ticket_status, "side_effect_unknown");
            assert_eq!(application_state, "side_effect_unknown");
            assert_eq!(attempt_status, "side_effect_unknown");
            assert_eq!(session_status, "needs_input");
            assert_eq!(capacity_runner, "local");
            assert_eq!(reserved_bytes, capacity.reserved_bytes);
            assert_eq!(reserved_objects, capacity.reserved_objects);
            assert_eq!(capacity_state, "active");
            assert_eq!(
                capacity_expiry,
                ticket_expiry.saturating_add(SUBMISSION_RECONCILIATION_GRACE_MS)
            );

            let shortened_expiry = now_ms().saturating_add(10_000);
            pool.get()
                .unwrap()
                .execute(
                    "UPDATE jobs_submission_evidence_capacity SET expires_at_ms = ?3
                      WHERE account_id = 'acct-jobs' AND application_id = ?1 AND run_id = ?2",
                    params![application.id, run_id, shortened_expiry],
                )
                .unwrap();
            let terminal_session = list_browser_sessions(&pool, "acct-jobs")
                .unwrap()
                .into_iter()
                .find(|candidate| candidate.id == run_id)
                .unwrap();
            let replayed = finalize_local_side_effect_unknown(
                &pool,
                "acct-jobs",
                &application.id,
                &run_id,
                &ticket_hash,
                &capacity,
                receipt,
                &terminal_session,
            )
            .unwrap();
            assert_eq!(serde_json::to_value(replayed).unwrap(), finalized_value);
            let replay_expiry: i64 = pool
                .get()
                .unwrap()
                .query_row(
                    "SELECT expires_at_ms FROM jobs_submission_evidence_capacity
                      WHERE account_id = 'acct-jobs' AND application_id = ?1 AND run_id = ?2",
                    params![application.id, run_id],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(replay_expiry, capacity_expiry);
        }
    }

    #[test]
    fn local_unknown_capacity_failure_rolls_back_every_lifecycle_row() {
        let pool = test_pool();
        let (application, run_id, ticket_hash, _) =
            local_run_authority_fixture(&pool, "unknown-capacity-failure");
        reserve_application_attempt(&pool, "acct-jobs", &application.id, "local").unwrap();
        assert!(
            claim_authorized_local_run_ticket(&pool, &run_id, &ticket_hash)
                .unwrap()
                .is_some()
        );
        let application = update_application(&pool, "acct-jobs", &application.id, "running", None)
            .unwrap()
            .unwrap();
        let session = BrowserSession {
            id: run_id.clone(),
            runner: "local".to_string(),
            status: "running".to_string(),
            current_company: "Acme".to_string(),
            current_step: "Posting local result".to_string(),
            application_id: Some(application.id.clone()),
            takeover_url: None,
            created_at_ms: 0,
            updated_at_ms: 0,
        };
        upsert_browser_session(&pool, "acct-jobs", &session).unwrap();
        let application_before = serde_json::to_value(&application).unwrap();
        reserve_verified_browser_profile_upload(
            &pool,
            &application.id,
            &run_id,
            "unknown-capacity-profile",
            1,
            (0, 1),
            (
                "accounts/acct-jobs/artifacts/unknown-capacity-existing-object",
                &"d".repeat(64),
                1,
                1,
            ),
        );
        let mut capacity = test_submission_evidence_capacity(&application.id, &run_id);
        capacity.runner = "local".to_string();
        capacity.limits.max_account_bytes = capacity.reserved_bytes;

        let error = finalize_local_side_effect_unknown(
            &pool,
            "acct-jobs",
            &application.id,
            &run_id,
            &ticket_hash,
            &capacity,
            json!({ "status": "side_effect_unknown" }),
            &session,
        )
        .unwrap_err();
        assert_eq!(
            error.downcast_ref::<UploadControlError>(),
            Some(&UploadControlError::AccountBytesQuotaExceeded)
        );
        let stored = get_application(&pool, "acct-jobs", &application.id)
            .unwrap()
            .unwrap();
        assert_eq!(serde_json::to_value(stored).unwrap(), application_before);
        let (ticket_status, attempt_status, session_status, capacity_count): (
            String,
            String,
            String,
            i64,
        ) = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT ticket.status, attempt.status, session.status,
                        (SELECT COUNT(*) FROM jobs_submission_evidence_capacity capacity
                          WHERE capacity.account_id = ticket.account_id
                            AND capacity.application_id = ticket.application_id
                            AND capacity.run_id = ticket.id)
                   FROM jobs_local_run_tickets ticket
                   JOIN jobs_attempt_reservations attempt
                     ON attempt.account_id = ticket.account_id
                    AND attempt.application_id = ticket.application_id
                   JOIN jobs_browser_sessions session
                     ON session.account_id = ticket.account_id AND session.id = ticket.id
                  WHERE ticket.id = ?1",
                params![run_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap();
        assert_eq!(ticket_status, "claimed");
        assert_eq!(attempt_status, "reserved");
        assert_eq!(session_status, "running");
        assert_eq!(capacity_count, 0);
    }

    #[test]
    fn local_click_started_unknown_requires_and_retains_exact_capacity() {
        let pool = test_pool();
        let (application, run_id, ticket_hash) =
            local_click_started_fixture(&pool, "unknown-after-click");
        reserve_application_attempt(&pool, "acct-jobs", &application.id, "local").unwrap();
        let mut capacity = test_submission_evidence_capacity(&application.id, &run_id);
        capacity.runner = "local".to_string();
        let shortened_expiry = now_ms().saturating_add(10_000);
        pool.get()
            .unwrap()
            .execute(
                "UPDATE jobs_submission_evidence_capacity SET expires_at_ms = ?3
                  WHERE account_id = 'acct-jobs' AND application_id = ?1 AND run_id = ?2",
                params![application.id, run_id, shortened_expiry],
            )
            .unwrap();
        let session = list_browser_sessions(&pool, "acct-jobs")
            .unwrap()
            .into_iter()
            .find(|candidate| candidate.id == run_id)
            .unwrap();
        finalize_local_side_effect_unknown(
            &pool,
            "acct-jobs",
            &application.id,
            &run_id,
            &ticket_hash,
            &capacity,
            json!({ "status": "side_effect_unknown" }),
            &session,
        )
        .unwrap();
        let (ticket_status, reserved_bytes, reserved_objects, capacity_expiry, ticket_expiry): (
            String,
            i64,
            i64,
            i64,
            i64,
        ) = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT ticket.status, capacity.reserved_bytes, capacity.reserved_objects,
                        capacity.expires_at_ms, ticket.expires_at_ms
                   FROM jobs_local_run_tickets ticket
                   JOIN jobs_submission_evidence_capacity capacity
                     ON capacity.account_id = ticket.account_id
                    AND capacity.application_id = ticket.application_id
                    AND capacity.run_id = ticket.id
                  WHERE ticket.id = ?1",
                params![run_id],
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
        assert_eq!(ticket_status, "side_effect_unknown");
        assert_eq!(reserved_bytes, capacity.reserved_bytes);
        assert_eq!(reserved_objects, capacity.reserved_objects);
        assert_eq!(
            capacity_expiry,
            ticket_expiry.saturating_add(SUBMISSION_RECONCILIATION_GRACE_MS)
        );

        let missing_pool = test_pool();
        let (application, run_id, ticket_hash) =
            local_click_started_fixture(&missing_pool, "unknown-after-click-missing");
        reserve_application_attempt(&missing_pool, "acct-jobs", &application.id, "local").unwrap();
        missing_pool
            .get()
            .unwrap()
            .execute(
                "DELETE FROM jobs_submission_evidence_capacity
                  WHERE account_id = 'acct-jobs' AND application_id = ?1 AND run_id = ?2",
                params![application.id, run_id],
            )
            .unwrap();
        let session = list_browser_sessions(&missing_pool, "acct-jobs")
            .unwrap()
            .into_iter()
            .find(|candidate| candidate.id == run_id)
            .unwrap();
        let mut capacity = test_submission_evidence_capacity(&application.id, &run_id);
        capacity.runner = "local".to_string();
        let error = finalize_local_side_effect_unknown(
            &missing_pool,
            "acct-jobs",
            &application.id,
            &run_id,
            &ticket_hash,
            &capacity,
            json!({ "status": "side_effect_unknown" }),
            &session,
        )
        .unwrap_err();
        assert!(error
            .to_string()
            .contains("local submission evidence capacity is missing"));
        let (ticket_status, application_state, attempt_status, session_status): (
            String,
            String,
            String,
            String,
        ) = missing_pool
            .get()
            .unwrap()
            .query_row(
                "SELECT ticket.status, application.state, attempt.status, session.status
                   FROM jobs_local_run_tickets ticket
                   JOIN jobs_applications application
                     ON application.account_id = ticket.account_id
                    AND application.id = ticket.application_id
                   JOIN jobs_attempt_reservations attempt
                     ON attempt.account_id = ticket.account_id
                    AND attempt.application_id = ticket.application_id
                   JOIN jobs_browser_sessions session
                     ON session.account_id = ticket.account_id AND session.id = ticket.id
                  WHERE ticket.id = ?1",
                params![run_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap();
        assert_eq!(ticket_status, "click_started");
        assert_eq!(application_state, "running");
        assert_eq!(attempt_status, "reserved");
        assert_eq!(session_status, "running");
    }

    #[test]
    fn local_unknown_replay_survives_durable_capacity_config_drift() {
        let pool = test_pool();
        let (application, run_id, ticket_hash) =
            local_click_started_fixture(&pool, "unknown-config-drift");
        reserve_application_attempt(&pool, "acct-jobs", &application.id, "local").unwrap();
        let durable_expiry = now_ms().saturating_add(2 * 86_400_000);
        pool.get()
            .unwrap()
            .execute(
                "UPDATE jobs_submission_evidence_capacity SET expires_at_ms = ?3
                  WHERE account_id = 'acct-jobs' AND application_id = ?1 AND run_id = ?2",
                params![application.id, run_id, durable_expiry],
            )
            .unwrap();

        let drifted_capacity = |durable: crate::db::object_uploads::SubmissionEvidenceCapacity| {
            NewSubmissionEvidenceCapacity {
                account_id: durable.account_id,
                application_id: durable.application_id,
                run_id: durable.run_id,
                runner: durable.runner,
                reserved_bytes: durable.reserved_bytes,
                reserved_objects: durable.reserved_objects,
                expires_at_ms: durable.expires_at_ms,
                now_ms: now_ms(),
                limits: UploadLimits {
                    max_object_bytes: 1,
                    max_account_bytes: durable.reserved_bytes.saturating_sub(1),
                    max_daily_bytes: 1,
                    max_account_objects: durable.reserved_objects.saturating_sub(1),
                },
            }
        };
        let durable = crate::db::object_uploads::get_submission_evidence_capacity(
            &pool,
            "acct-jobs",
            &application.id,
            &run_id,
        )
        .unwrap()
        .expect("durable click-started capacity");
        let capacity = drifted_capacity(durable);
        assert!(capacity.reserved_bytes > capacity.limits.max_account_bytes);
        assert!(capacity.reserved_objects > capacity.limits.max_account_objects);
        assert!(capacity.expires_at_ms > capacity.now_ms.saturating_add(86_400_000));

        let session = list_browser_sessions(&pool, "acct-jobs")
            .unwrap()
            .into_iter()
            .find(|candidate| candidate.id == run_id)
            .unwrap();
        let receipt = json!({
            "status": "side_effect_unknown",
            "issues": [{
                "field": "submission",
                "message": "The submit response was lost after the durable click boundary."
            }]
        });
        let finalized = finalize_local_side_effect_unknown(
            &pool,
            "acct-jobs",
            &application.id,
            &run_id,
            &ticket_hash,
            &capacity,
            receipt.clone(),
            &session,
        )
        .unwrap();
        assert_eq!(finalized.state, "side_effect_unknown");

        let durable = crate::db::object_uploads::get_submission_evidence_capacity(
            &pool,
            "acct-jobs",
            &application.id,
            &run_id,
        )
        .unwrap()
        .expect("durable side-effect-unknown capacity");
        let replay_capacity = drifted_capacity(durable);
        let terminal_session = list_browser_sessions(&pool, "acct-jobs")
            .unwrap()
            .into_iter()
            .find(|candidate| candidate.id == run_id)
            .unwrap();
        let replayed = finalize_local_side_effect_unknown(
            &pool,
            "acct-jobs",
            &application.id,
            &run_id,
            &ticket_hash,
            &replay_capacity,
            receipt,
            &terminal_session,
        )
        .unwrap();
        assert_eq!(
            serde_json::to_value(replayed).unwrap(),
            serde_json::to_value(finalized).unwrap()
        );
        let (capacity_count, stored_expiry): (i64, i64) = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT COUNT(*), MAX(expires_at_ms)
                   FROM jobs_submission_evidence_capacity
                  WHERE account_id = 'acct-jobs' AND application_id = ?1 AND run_id = ?2",
                params![application.id, run_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(capacity_count, 1);
        assert_eq!(stored_expiry, durable_expiry);
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
    fn account_deletion_fence_blocks_local_launch_and_jobs_mutations() {
        let pool = test_pool();
        let (application, run_id, ticket_hash, _) =
            local_run_authority_fixture(&pool, "account-delete-local-fence");
        pool.get()
            .unwrap()
            .execute(
                "INSERT INTO account_deletion_intents (
                    account_id, requested_at_ms, last_checked_at_ms,
                    fresh_upload_cutoff_ms, fresh_in_flight_puts
                 ) VALUES ('acct-jobs', ?1, ?1, ?1, 0)",
                params![now_ms()],
            )
            .unwrap();

        assert!(
            claim_authorized_local_run_ticket(&pool, &run_id, &ticket_hash)
                .unwrap()
                .is_none()
        );
        assert_account_deletion_fence(update_local_run_ticket_status(
            &pool,
            &run_id,
            &ticket_hash,
            "failed",
        ));
        assert_account_deletion_fence(save_local_run_ticket(
            &pool,
            "acct-jobs",
            &application.id,
            "fenced-new-local-run",
            "fenced-new-local-ticket-hash",
            "fenced-new-local-ticket-secret",
            json!({ "runId": "fenced-new-local-run" }),
            now_ms() + 60_000,
        ));
        assert_account_deletion_fence(update_application(
            &pool,
            "acct-jobs",
            &application.id,
            "running",
            None,
        ));
        assert_account_deletion_fence(update_attempt_reservation_status(
            &pool,
            "acct-jobs",
            &application.id,
            "running",
        ));
        assert_account_deletion_fence(save_run_event(
            &pool,
            "acct-jobs",
            &run_id,
            "fenced_event",
            json!({ "application_id": application.id.clone() }),
        ));
        assert_account_deletion_fence(save_intervention(
            &pool,
            "acct-jobs",
            &Intervention {
                id: "fenced-intervention".to_string(),
                application_id: Some(application.id.clone()),
                kind: "browser_takeover".to_string(),
                status: "open".to_string(),
                title: "Fenced intervention".to_string(),
                detail: "Must not be written after deletion starts".to_string(),
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
        ));

        let ticket_status: String = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT status FROM jobs_local_run_tickets WHERE id = ?1",
                params![run_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(ticket_status, "queued");
        let forbidden_rows: i64 = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT
                    (SELECT COUNT(*) FROM jobs_run_events WHERE event_type = 'fenced_event')
                    +
                    (SELECT COUNT(*) FROM jobs_interventions
                      WHERE id = 'fenced-intervention')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(forbidden_rows, 0);
    }

    #[test]
    fn account_deletion_fence_blocks_local_resume_approval_and_consumption() {
        let pool = test_pool();
        let (application, run_id, ticket_hash, _) =
            local_run_authority_fixture(&pool, "account-delete-resume-fence");
        assert!(
            claim_authorized_local_run_ticket(&pool, &run_id, &ticket_hash)
                .unwrap()
                .is_some()
        );
        update_application(&pool, "acct-jobs", &application.id, "running", None).unwrap();
        update_application(&pool, "acct-jobs", &application.id, "needs_input", None).unwrap();
        assert!(
            update_local_run_ticket_status(&pool, &run_id, &ticket_hash, "needs_input",).unwrap()
        );
        let intervention = save_intervention(
            &pool,
            "acct-jobs",
            &Intervention {
                id: String::new(),
                application_id: Some(application.id.clone()),
                kind: "browser_takeover".to_string(),
                status: "approved".to_string(),
                title: "Review the application".to_string(),
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
        approve_local_run_resume_action(
            &pool,
            "acct-jobs",
            &application.id,
            &run_id,
            &intervention.id,
        )
        .unwrap()
        .unwrap();
        pool.get()
            .unwrap()
            .execute(
                "INSERT INTO account_deletion_intents (
                    account_id, requested_at_ms, last_checked_at_ms,
                    fresh_upload_cutoff_ms, fresh_in_flight_puts
                 ) VALUES ('acct-jobs', ?1, ?1, ?1, 0)",
                params![now_ms()],
            )
            .unwrap();

        assert_account_deletion_fence(approve_local_run_resume_action(
            &pool,
            "acct-jobs",
            &application.id,
            &run_id,
            &intervention.id,
        ));
        assert_account_deletion_fence(consume_local_run_resume_action(
            &pool,
            &run_id,
            &ticket_hash,
        ));
        let action_status: (String, Option<i64>) = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT status, consumed_at_ms FROM jobs_local_run_resume_actions
                  WHERE run_id = ?1",
                params![run_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(action_status, ("approved".to_string(), None));
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
        assert!(
            claim_local_run_ticket(&pool, &run_id, "local-resume-ticket-hash")
                .unwrap()
                .is_some()
        );
        assert!(update_local_run_ticket_status(
            &pool,
            &run_id,
            "local-resume-ticket-hash",
            "needs_input",
        )
        .unwrap());
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
        let (profile, preferences) =
            execution_policy_fixture(&pool, "acct-jobs", "jobs@example.com", "track-default");
        let posting = upsert_posting(
            &pool,
            "acct-jobs",
            &test_posting(
                "https://boards.greenhouse.io/acme/jobs/export",
                now_ms(),
                now_ms(),
            ),
            &profile,
            &preferences,
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
                "screenshotKeys": ["accounts/acct-jobs/jobs/export/confirmation.png"],
                "receiptObject": {
                    "storageKey": "accounts/acct-jobs/jobs/export/receipt.json",
                    "sha256": "c".repeat(64),
                    "mediaType": "application/json",
                    "sizeBytes": 84,
                    "schemaVersion": 1
                }
            }),
        )
        .unwrap();

        let export = account_export(&pool, "acct-jobs", "jobs@example.com")
            .unwrap()
            .unwrap();
        let workspace = export.workspace.as_ref().unwrap();
        assert_eq!(export.resume_versions.len(), 1);
        assert_eq!(workspace.application_evidence.len(), 1);
        assert_eq!(export.canonical_track_policy_ledger.revisions.len(), 1);
        assert_eq!(
            export.canonical_track_policy_ledger.review_receipts.len(),
            1
        );
        assert_eq!(export.canonical_track_policy_ledger.heads.len(), 1);
        assert!(export.canonical_track_policy_ledger.revisions[0]
            .canonical_policy_json
            .contains("New York, NY"));
        assert!(workspace.browser_sessions[0].takeover_url.is_none());
        let serialized = serde_json::to_string(&export).unwrap();
        assert!(!serialized.contains(ENCRYPTED_PAYLOAD_PREFIX));
        assert!(!serialized.contains("secret-capability"));
        assert!(!serialized.contains("export-ticket-secret"));
        assert!(!serialized.contains("must-not-export"));

        let refs = crate::db::account_data::artifact_object_refs(&pool, "acct-jobs").unwrap();
        assert_eq!(refs.len(), 5);
        assert!(refs.iter().any(|reference| {
            reference.object_key == "accounts/acct-jobs/jobs/fixture-source-resume.pdf"
                && reference.content_type.as_deref() == Some("application/pdf")
                && reference.size_bytes == Some(1_024)
        }));
        assert!(refs.iter().any(|reference| reference.object_key
            == "accounts/acct-jobs/jobs/export/resume.pdf"
            && reference.size_bytes == Some(42)));
        assert!(refs.iter().any(|reference| {
            reference.object_key == "accounts/acct-jobs/jobs/export/cover-letter.pdf"
        }));
        assert!(refs.iter().any(|reference| {
            reference.object_key == "accounts/acct-jobs/jobs/export/confirmation.png"
        }));
        assert!(refs.iter().any(|reference| {
            reference.object_key == "accounts/acct-jobs/jobs/export/receipt.json"
                && reference.content_type.as_deref() == Some("application/json")
                && reference.size_bytes == Some(84)
        }));
    }

    #[test]
    fn browser_profile_snapshot_is_fenced_versioned_and_idempotent() {
        let pool = test_pool();
        let (application, run_id, browser_profile_id) =
            execution_lease_fixture(&pool, "profile-snapshot");
        let lease = claim_execution_lease(
            &pool,
            "acct-jobs",
            &application.id,
            &run_id,
            &browser_profile_id,
            "profile-worker",
        )
        .unwrap();
        assert!(get_browser_profile_snapshot_for_lease(
            &pool,
            "acct-jobs",
            &application.id,
            &run_id,
            &browser_profile_id,
            &lease.lease_token,
            lease.fence,
        )
        .unwrap()
        .is_none());

        let object_key = format!(
            "accounts/acct-jobs/jobs/browser-profiles/{browser_profile_id}/generation/1.enc"
        );
        let first = commit_browser_profile_snapshot_for_lease(
            &pool,
            "acct-jobs",
            &application.id,
            &run_id,
            &browser_profile_id,
            &lease.lease_token,
            lease.fence,
            0,
            &object_key,
            &"a".repeat(64),
            128,
            1,
        )
        .unwrap();
        assert_eq!(first.generation, 1);
        let replay = commit_browser_profile_snapshot_for_lease(
            &pool,
            "acct-jobs",
            &application.id,
            &run_id,
            &browser_profile_id,
            &lease.lease_token,
            lease.fence,
            0,
            &object_key,
            &"a".repeat(64),
            128,
            1,
        )
        .unwrap();
        assert_eq!(replay, first);
        assert!(matches!(
            commit_browser_profile_snapshot_for_lease(
                &pool,
                "acct-jobs",
                &application.id,
                &run_id,
                &browser_profile_id,
                &lease.lease_token,
                lease.fence,
                0,
                &format!("{object_key}.different"),
                &"b".repeat(64),
                129,
                1,
            ),
            Err(ExecutionLeaseError::Conflict)
        ));
    }

    #[test]
    fn browser_profile_publication_replays_exactly_and_schedules_the_prior_generation() {
        let pool = test_pool();
        let (application, run_id, browser_profile_id) =
            execution_lease_fixture(&pool, "profile-atomic-publish");
        let lease = claim_execution_lease(
            &pool,
            "acct-jobs",
            &application.id,
            &run_id,
            &browser_profile_id,
            "profile-atomic-worker",
        )
        .unwrap();
        let first_key = format!(
            "accounts/acct-jobs/jobs/browser-profiles/{browser_profile_id}/generation/1-a.enc"
        );
        let first_upload = reserve_verified_browser_profile_upload(
            &pool,
            &application.id,
            &run_id,
            &browser_profile_id,
            lease.fence,
            (0, 1),
            (&first_key, &"a".repeat(64), 128, 1),
        );
        let first = publish_browser_profile_snapshot_for_lease(
            &pool,
            "acct-jobs",
            &application.id,
            &run_id,
            &browser_profile_id,
            &lease.lease_token,
            lease.fence,
            0,
            &first_key,
            &first_upload.sha256,
            first_upload.size_bytes,
            1,
            &first_upload.id,
        )
        .unwrap();
        let replay = publish_browser_profile_snapshot_for_lease(
            &pool,
            "acct-jobs",
            &application.id,
            &run_id,
            &browser_profile_id,
            &lease.lease_token,
            lease.fence,
            0,
            &first_key,
            &first_upload.sha256,
            first_upload.size_bytes,
            1,
            &first_upload.id,
        )
        .unwrap();
        assert_eq!(replay, first);
        assert_eq!(first.generation, 1);

        let second_key = format!(
            "accounts/acct-jobs/jobs/browser-profiles/{browser_profile_id}/generation/2-b.enc"
        );
        let second_upload = reserve_verified_browser_profile_upload(
            &pool,
            &application.id,
            &run_id,
            &browser_profile_id,
            lease.fence,
            (1, 2),
            (&second_key, &"b".repeat(64), 192, 1),
        );
        let second = publish_browser_profile_snapshot_for_lease(
            &pool,
            "acct-jobs",
            &application.id,
            &run_id,
            &browser_profile_id,
            &lease.lease_token,
            lease.fence,
            1,
            &second_key,
            &second_upload.sha256,
            second_upload.size_bytes,
            1,
            &second_upload.id,
        )
        .unwrap();
        assert_eq!(second.generation, 2);
        assert_eq!(second.object_key, second_key);
        assert_eq!(
            get_browser_profile_snapshot_for_lease(
                &pool,
                "acct-jobs",
                &application.id,
                &run_id,
                &browser_profile_id,
                &lease.lease_token,
                lease.fence,
            )
            .unwrap(),
            Some(second.clone())
        );

        let first_lifecycle: (String, String, String) = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT upload.state, put.state, deletion.state
                   FROM object_uploads upload
                   JOIN object_storage_outbox put
                     ON put.upload_id = upload.id AND put.operation = 'put'
                   JOIN object_storage_outbox deletion
                     ON deletion.upload_id = upload.id AND deletion.operation = 'delete'
                  WHERE upload.id = ?1",
                params![first_upload.id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(
            first_lifecycle,
            (
                "delete_pending".into(),
                "completed".into(),
                "pending".into()
            )
        );
        let second_lifecycle: (String, String, i64) = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT upload.state, put.state,
                        (SELECT COUNT(*) FROM object_storage_outbox deletion
                          WHERE deletion.upload_id = upload.id
                            AND deletion.operation = 'delete')
                   FROM object_uploads upload
                   JOIN object_storage_outbox put
                     ON put.upload_id = upload.id AND put.operation = 'put'
                  WHERE upload.id = ?1",
                params![second_upload.id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(second_lifecycle, ("ready".into(), "completed".into(), 0));
    }

    #[test]
    fn losing_browser_profile_cas_leaves_verified_upload_pending_and_current_ready() {
        let pool = test_pool();
        let (application, run_id, browser_profile_id) =
            execution_lease_fixture(&pool, "profile-atomic-cas");
        let lease = claim_execution_lease(
            &pool,
            "acct-jobs",
            &application.id,
            &run_id,
            &browser_profile_id,
            "profile-cas-worker",
        )
        .unwrap();
        let current_key = format!(
            "accounts/acct-jobs/jobs/browser-profiles/{browser_profile_id}/generation/1-winner.enc"
        );
        let current_upload = reserve_verified_browser_profile_upload(
            &pool,
            &application.id,
            &run_id,
            &browser_profile_id,
            lease.fence,
            (0, 1),
            (&current_key, &"c".repeat(64), 128, 1),
        );
        let current = publish_browser_profile_snapshot_for_lease(
            &pool,
            "acct-jobs",
            &application.id,
            &run_id,
            &browser_profile_id,
            &lease.lease_token,
            lease.fence,
            0,
            &current_key,
            &current_upload.sha256,
            current_upload.size_bytes,
            1,
            &current_upload.id,
        )
        .unwrap();

        let losing_key = format!(
            "accounts/acct-jobs/jobs/browser-profiles/{browser_profile_id}/generation/1-loser.enc"
        );
        let losing_upload = reserve_verified_browser_profile_upload(
            &pool,
            &application.id,
            &run_id,
            &browser_profile_id,
            lease.fence,
            (0, 1),
            (&losing_key, &"d".repeat(64), 160, 1),
        );
        assert!(matches!(
            publish_browser_profile_snapshot_for_lease(
                &pool,
                "acct-jobs",
                &application.id,
                &run_id,
                &browser_profile_id,
                &lease.lease_token,
                lease.fence,
                0,
                &losing_key,
                &losing_upload.sha256,
                losing_upload.size_bytes,
                1,
                &losing_upload.id,
            ),
            Err(ExecutionLeaseError::Conflict)
        ));
        assert_eq!(
            get_browser_profile_snapshot_for_lease(
                &pool,
                "acct-jobs",
                &application.id,
                &run_id,
                &browser_profile_id,
                &lease.lease_token,
                lease.fence,
            )
            .unwrap(),
            Some(current)
        );

        let current_lifecycle: (String, String, i64) = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT upload.state, put.state,
                        (SELECT COUNT(*) FROM object_storage_outbox deletion
                          WHERE deletion.upload_id = upload.id
                            AND deletion.operation = 'delete')
                   FROM object_uploads upload
                   JOIN object_storage_outbox put
                     ON put.upload_id = upload.id AND put.operation = 'put'
                  WHERE upload.id = ?1",
                params![current_upload.id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(current_lifecycle, ("ready".into(), "completed".into(), 0));
        let losing_lifecycle: (String, String, i64) = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT upload.state, put.state,
                        (SELECT COUNT(*) FROM object_storage_outbox deletion
                          WHERE deletion.upload_id = upload.id
                            AND deletion.operation = 'delete')
                   FROM object_uploads upload
                   JOIN object_storage_outbox put
                     ON put.upload_id = upload.id AND put.operation = 'put'
                  WHERE upload.id = ?1",
                params![losing_upload.id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(losing_lifecycle, ("pending".into(), "retry".into(), 0));
    }

    #[test]
    fn browser_profile_snapshot_rejects_stale_and_cross_profile_workers() {
        let pool = test_pool();
        let (application, run_id, browser_profile_id) =
            execution_lease_fixture(&pool, "profile-stale");
        let stale = claim_execution_lease(
            &pool,
            "acct-jobs",
            &application.id,
            &run_id,
            &browser_profile_id,
            "stale-worker",
        )
        .unwrap();
        assert!(matches!(
            get_browser_profile_snapshot_for_lease(
                &pool,
                "acct-jobs",
                &application.id,
                &run_id,
                "another-profile",
                &stale.lease_token,
                stale.fence,
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
        let replacement = claim_execution_lease(
            &pool,
            "acct-jobs",
            &application.id,
            &run_id,
            &browser_profile_id,
            "replacement-worker",
        )
        .unwrap();
        assert!(matches!(
            commit_browser_profile_snapshot_for_lease(
                &pool,
                "acct-jobs",
                &application.id,
                &run_id,
                &browser_profile_id,
                &stale.lease_token,
                stale.fence,
                0,
                "accounts/acct-jobs/jobs/browser-profiles/stale.enc",
                &"c".repeat(64),
                64,
                1,
            ),
            Err(ExecutionLeaseError::Conflict)
        ));
        let stored = commit_browser_profile_snapshot_for_lease(
            &pool,
            "acct-jobs",
            &application.id,
            &run_id,
            &browser_profile_id,
            &replacement.lease_token,
            replacement.fence,
            0,
            "accounts/acct-jobs/jobs/browser-profiles/replacement.enc",
            &"d".repeat(64),
            96,
            1,
        )
        .unwrap();
        assert_eq!(stored.writer_run_id, run_id);
        assert_eq!(stored.writer_fence, replacement.fence);
    }

    #[test]
    fn certified_local_answer_revision_atomically_invalidates_document_packet_preflight() {
        let pool = test_pool();
        let (application, run_id, _ticket_hash, proof) =
            certified_local_intervention_fixture(&pool, "certified-answer-local");
        assert_eq!(proof.schema_version, 4);
        assert!(proof
            .documents
            .iter()
            .any(|document| document.kind == "resume"));
        assert!(proof
            .documents
            .iter()
            .any(|document| document.kind == "cover_letter"));
        let approved_checksum = application.receipt["approved_execution"]["checksum"]
            .as_str()
            .unwrap()
            .to_string();
        let (application, intervention) = put_application_in_answer_intervention(
            &pool,
            &application,
            &run_id,
            "certified-answer-local",
        );
        assert_eq!(
            certified_binding_state(&pool, &application.id, &run_id),
            ("preflight".to_string(), 0, None, None, None)
        );

        pool.get()
            .unwrap()
            .execute_batch(
                "CREATE TRIGGER fail_certified_intervention_revision
                   BEFORE UPDATE OF application_json ON jobs_applications
                   WHEN NEW.state = 'awaiting_review'
                 BEGIN
                   SELECT RAISE(ABORT, 'forced certified intervention rollback');
                 END;",
            )
            .unwrap();
        assert!(resolve_intervention_answer_for_review(
            &pool,
            "acct-jobs",
            &intervention.id,
            "Yes, up to 25%.",
        )
        .is_err());

        assert_eq!(
            certified_binding_state(&pool, &application.id, &run_id),
            ("preflight".to_string(), 0, None, None, None),
            "binding invalidation must roll back with the packet revision"
        );
        let rolled_back: (String, String, String, String, String) = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT ticket.status, application.state, intervention.status,
                        reservation.status, session.status
                   FROM jobs_local_run_tickets ticket
                   JOIN jobs_applications application
                     ON application.account_id = ticket.account_id
                    AND application.id = ticket.application_id
                   JOIN jobs_interventions intervention
                     ON intervention.account_id = application.account_id
                    AND intervention.application_id = application.id
                   JOIN jobs_attempt_reservations reservation
                     ON reservation.account_id = application.account_id
                    AND reservation.application_id = application.id
                   JOIN jobs_browser_sessions session
                     ON session.account_id = application.account_id
                    AND session.id = ticket.id
                  WHERE application.id = ?1 AND ticket.id = ?2
                    AND intervention.id = ?3",
                params![application.id, run_id, intervention.id],
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
        assert_eq!(
            rolled_back,
            (
                "claimed".to_string(),
                "needs_input".to_string(),
                "open".to_string(),
                "running".to_string(),
                "needs_input".to_string(),
            )
        );

        pool.get()
            .unwrap()
            .execute_batch("DROP TRIGGER fail_certified_intervention_revision;")
            .unwrap();
        let revision = resolve_intervention_answer_for_review(
            &pool,
            "acct-jobs",
            &intervention.id,
            "Yes, up to 25%.",
        )
        .unwrap();

        assert_eq!(revision.application.state, "awaiting_review");
        assert_eq!(revision.application.run_id, None);
        assert!(revision
            .application
            .receipt
            .get("approved_execution")
            .is_none());
        assert_eq!(
            revision
                .application
                .receipt
                .pointer("/packet_revision/invalidated_packet_checksum")
                .and_then(Value::as_str),
            Some(approved_checksum.as_str())
        );
        assert_eq!(
            revision
                .application
                .receipt
                .pointer("/final_answers/0/value")
                .and_then(Value::as_str),
            Some("Yes, up to 25%.")
        );
        let invalidated = certified_binding_state(&pool, &application.id, &run_id);
        assert_eq!(
            (&invalidated.0, invalidated.1, invalidated.2.as_deref()),
            (&"invalidated".to_string(), 1, Some("packet_changed"))
        );
        assert!(invalidated.3.is_some());
        assert_eq!(invalidated.4, None);
        let committed: (String, String, String, String, i64, i64) = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT ticket.status, intervention.status, reservation.status, session.status,
                        (SELECT COUNT(*) FROM jobs_ats_certification_canary_reservations
                          WHERE binding_id = binding.binding_id),
                        (SELECT COUNT(*) FROM jobs_submission_evidence_capacity
                          WHERE account_id = application.account_id
                            AND application_id = application.id AND run_id = ticket.id)
                   FROM jobs_local_run_tickets ticket
                   JOIN jobs_applications application
                     ON application.account_id = ticket.account_id
                    AND application.id = ticket.application_id
                   JOIN jobs_interventions intervention
                     ON intervention.account_id = application.account_id
                    AND intervention.application_id = application.id
                   JOIN jobs_attempt_reservations reservation
                     ON reservation.account_id = application.account_id
                    AND reservation.application_id = application.id
                   JOIN jobs_browser_sessions session
                     ON session.account_id = application.account_id
                    AND session.id = ticket.id
                   JOIN jobs_application_ats_certification_bindings binding
                     ON binding.account_id = application.account_id
                    AND binding.application_id = application.id
                    AND binding.run_id = ticket.id
                  WHERE application.id = ?1 AND ticket.id = ?2
                    AND intervention.id = ?3",
                params![application.id, run_id, intervention.id],
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
        assert_eq!(
            committed,
            (
                "failed".to_string(),
                "resolved".to_string(),
                "released".to_string(),
                "paused".to_string(),
                0,
                0,
            )
        );
    }

    #[test]
    fn certified_local_answer_revision_after_click_marker_stays_in_reconciliation() {
        let pool = test_pool();
        let (application, run_id, ticket_hash, proof) =
            certified_local_intervention_fixture(&pool, "certified-answer-local-after-click");
        let mut capacity = test_submission_evidence_capacity(&application.id, &run_id);
        capacity.runner = "local".to_string();
        assert!(
            local_run_submit_authorized(&pool, &run_id, &ticket_hash, &proof, &capacity).unwrap()
        );
        let application = get_application(&pool, "acct-jobs", &application.id)
            .unwrap()
            .unwrap();
        assert_eq!(stored_final_submit_proof(&application).unwrap(), proof);
        let (application, intervention) = put_application_in_answer_intervention(
            &pool,
            &application,
            &run_id,
            "certified-answer-local-after-click",
        );

        let error =
            resolve_intervention_answer_for_review(&pool, "acct-jobs", &intervention.id, "No")
                .unwrap_err();
        assert!(error.to_string().contains("awaiting reconciliation"));

        let consumed = certified_binding_state(&pool, &application.id, &run_id);
        assert_eq!(
            (&consumed.0, consumed.1, consumed.2.as_deref(), consumed.3),
            (&"consumed".to_string(), 1, None, None)
        );
        assert!(consumed.4.is_some());
        let stored = get_application(&pool, "acct-jobs", &application.id)
            .unwrap()
            .unwrap();
        assert_eq!(stored.state, "needs_input");
        assert_eq!(stored.run_id.as_deref(), Some(run_id.as_str()));
        assert_eq!(stored_final_submit_proof(&stored).unwrap(), proof);
        assert!(stored.receipt.get("approved_execution").is_some());
        assert!(stored.receipt.get("packet_revision").is_none());
        let terminal: (String, String, String, String, i64, i64) = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT ticket.status, intervention.status, reservation.status, capacity.state,
                        (SELECT COUNT(*) FROM jobs_ats_certification_canary_reservations
                          WHERE binding_id = binding.binding_id),
                        (SELECT COUNT(*) FROM jobs_submission_evidence_capacity
                          WHERE account_id = application.account_id
                            AND application_id = application.id AND run_id = ticket.id)
                   FROM jobs_local_run_tickets ticket
                   JOIN jobs_applications application
                     ON application.account_id = ticket.account_id
                    AND application.id = ticket.application_id
                   JOIN jobs_interventions intervention
                     ON intervention.account_id = application.account_id
                    AND intervention.application_id = application.id
                   JOIN jobs_attempt_reservations reservation
                     ON reservation.account_id = application.account_id
                    AND reservation.application_id = application.id
                   JOIN jobs_submission_evidence_capacity capacity
                     ON capacity.account_id = application.account_id
                    AND capacity.application_id = application.id
                    AND capacity.run_id = ticket.id
                   JOIN jobs_application_ats_certification_bindings binding
                     ON binding.account_id = application.account_id
                    AND binding.application_id = application.id
                    AND binding.run_id = ticket.id
                  WHERE application.id = ?1 AND ticket.id = ?2
                    AND intervention.id = ?3",
                params![application.id, run_id, intervention.id],
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
        assert_eq!(
            terminal,
            (
                "click_started".to_string(),
                "open".to_string(),
                "running".to_string(),
                "active".to_string(),
                1,
                1,
            )
        );
    }

    #[test]
    fn certified_cloud_answer_revision_atomically_invalidates_document_packet_preflight() {
        let pool = test_pool();
        let fixture = certified_cloud_execution_fixture(&pool, "certified-answer-cloud");
        let application = fixture.application;
        let run_id = fixture.run_id;
        let proof = fixture.proof;
        assert_eq!(proof.schema_version, 4);
        assert!(proof
            .documents
            .iter()
            .any(|document| document.kind == "resume"));
        assert!(proof
            .documents
            .iter()
            .any(|document| document.kind == "cover_letter"));
        let approved_checksum = application.receipt["approved_execution"]["checksum"]
            .as_str()
            .unwrap()
            .to_string();
        let (application, intervention) = put_application_in_answer_intervention(
            &pool,
            &application,
            &run_id,
            "certified-answer-cloud",
        );
        assert_eq!(
            certified_binding_state(&pool, &application.id, &run_id),
            ("preflight".to_string(), 0, None, None, None)
        );

        pool.get()
            .unwrap()
            .execute_batch(
                "CREATE TRIGGER fail_certified_cloud_intervention_revision
                   BEFORE UPDATE OF application_json ON jobs_applications
                   WHEN NEW.state = 'awaiting_review'
                 BEGIN
                   SELECT RAISE(ABORT, 'forced certified cloud intervention rollback');
                 END;",
            )
            .unwrap();
        assert!(resolve_intervention_answer_for_review(
            &pool,
            "acct-jobs",
            &intervention.id,
            "Yes, up to 25%.",
        )
        .is_err());

        assert_eq!(
            certified_binding_state(&pool, &application.id, &run_id),
            ("preflight".to_string(), 0, None, None, None),
            "binding invalidation must roll back with the packet revision"
        );
        let rolled_back: (String, String, String, String, String) = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT lease.phase, application.state, intervention.status,
                        reservation.status, session.status
                   FROM jobs_execution_leases lease
                   JOIN jobs_applications application
                     ON application.account_id = lease.account_id
                    AND application.id = lease.application_id
                   JOIN jobs_interventions intervention
                     ON intervention.account_id = application.account_id
                    AND intervention.application_id = application.id
                   JOIN jobs_attempt_reservations reservation
                     ON reservation.account_id = application.account_id
                    AND reservation.application_id = application.id
                   JOIN jobs_browser_sessions session
                     ON session.account_id = application.account_id
                    AND session.id = lease.run_id
                  WHERE application.id = ?1 AND lease.run_id = ?2
                    AND intervention.id = ?3",
                params![application.id, run_id, intervention.id],
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
        assert_eq!(
            rolled_back,
            (
                "prepared".to_string(),
                "needs_input".to_string(),
                "open".to_string(),
                "running".to_string(),
                "needs_input".to_string(),
            )
        );

        pool.get()
            .unwrap()
            .execute_batch("DROP TRIGGER fail_certified_cloud_intervention_revision;")
            .unwrap();
        let revision = resolve_intervention_answer_for_review(
            &pool,
            "acct-jobs",
            &intervention.id,
            "Yes, up to 25%.",
        )
        .unwrap();

        assert_eq!(revision.application.state, "awaiting_review");
        assert_eq!(revision.application.run_id, None);
        assert!(revision
            .application
            .receipt
            .get("approved_execution")
            .is_none());
        assert_eq!(
            revision
                .application
                .receipt
                .pointer("/packet_revision/invalidated_packet_checksum")
                .and_then(Value::as_str),
            Some(approved_checksum.as_str())
        );
        assert_eq!(
            revision
                .application
                .receipt
                .pointer("/final_answers/0/value")
                .and_then(Value::as_str),
            Some("Yes, up to 25%.")
        );
        let invalidated = certified_binding_state(&pool, &application.id, &run_id);
        assert_eq!(
            (
                invalidated.0.as_str(),
                invalidated.1,
                invalidated.2.as_deref()
            ),
            ("invalidated", 1, Some("packet_changed"))
        );
        assert!(invalidated.3.is_some());
        assert_eq!(invalidated.4, None);
        let committed: (String, String, String, String, i64, i64) = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT lease.phase, intervention.status, reservation.status, session.status,
                        (SELECT COUNT(*) FROM jobs_ats_certification_canary_reservations
                          WHERE binding_id = binding.binding_id),
                        (SELECT COUNT(*) FROM jobs_submission_evidence_capacity
                          WHERE account_id = application.account_id
                            AND application_id = application.id AND run_id = lease.run_id)
                   FROM jobs_execution_leases lease
                   JOIN jobs_applications application
                     ON application.account_id = lease.account_id
                    AND application.id = lease.application_id
                   JOIN jobs_interventions intervention
                     ON intervention.account_id = application.account_id
                    AND intervention.application_id = application.id
                   JOIN jobs_attempt_reservations reservation
                     ON reservation.account_id = application.account_id
                    AND reservation.application_id = application.id
                   JOIN jobs_browser_sessions session
                     ON session.account_id = application.account_id
                    AND session.id = lease.run_id
                   JOIN jobs_application_ats_certification_bindings binding
                     ON binding.account_id = application.account_id
                    AND binding.application_id = application.id
                    AND binding.run_id = lease.run_id
                  WHERE application.id = ?1 AND lease.run_id = ?2
                    AND intervention.id = ?3",
                params![application.id, run_id, intervention.id],
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
        assert_eq!(
            committed,
            (
                "released".to_string(),
                "resolved".to_string(),
                "released".to_string(),
                "paused".to_string(),
                0,
                0,
            )
        );
    }

    #[test]
    fn certified_cloud_answer_revision_after_click_marker_stays_in_reconciliation() {
        let pool = test_pool();
        let fixture =
            certified_cloud_execution_fixture(&pool, "certified-answer-cloud-after-click");
        let application = fixture.application;
        let run_id = fixture.run_id;
        let lease_token = fixture.lease_token;
        let lease_fence = fixture.lease_fence;
        let proof = fixture.proof;
        let capacity = test_submission_evidence_capacity(&application.id, &run_id);
        start_irreversible_submission(
            &pool,
            "acct-jobs",
            &application.id,
            &run_id,
            &lease_token,
            lease_fence,
            &proof,
            &capacity,
        )
        .unwrap();
        let application = get_application(&pool, "acct-jobs", &application.id)
            .unwrap()
            .unwrap();
        assert_eq!(stored_final_submit_proof(&application).unwrap(), proof);
        let (application, intervention) = put_application_in_answer_intervention(
            &pool,
            &application,
            &run_id,
            "certified-answer-cloud-after-click",
        );

        let error =
            resolve_intervention_answer_for_review(&pool, "acct-jobs", &intervention.id, "No")
                .unwrap_err();
        assert!(error.to_string().contains("awaiting reconciliation"));

        let consumed = certified_binding_state(&pool, &application.id, &run_id);
        assert_eq!(
            (
                consumed.0.as_str(),
                consumed.1,
                consumed.2.as_deref(),
                consumed.3
            ),
            ("consumed", 1, None, None)
        );
        assert!(consumed.4.is_some());
        let stored = get_application(&pool, "acct-jobs", &application.id)
            .unwrap()
            .unwrap();
        assert_eq!(stored.state, "needs_input");
        assert_eq!(stored.run_id.as_deref(), Some(run_id.as_str()));
        assert_eq!(stored_final_submit_proof(&stored).unwrap(), proof);
        assert!(stored.receipt.get("approved_execution").is_some());
        assert!(stored.receipt.get("packet_revision").is_none());
        let terminal: (String, String, String, String, i64, i64) = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT lease.phase, intervention.status, reservation.status, capacity.state,
                        (SELECT COUNT(*) FROM jobs_ats_certification_canary_reservations
                          WHERE binding_id = binding.binding_id),
                        (SELECT COUNT(*) FROM jobs_submission_evidence_capacity
                          WHERE account_id = application.account_id
                            AND application_id = application.id AND run_id = lease.run_id)
                   FROM jobs_execution_leases lease
                   JOIN jobs_applications application
                     ON application.account_id = lease.account_id
                    AND application.id = lease.application_id
                   JOIN jobs_interventions intervention
                     ON intervention.account_id = application.account_id
                    AND intervention.application_id = application.id
                   JOIN jobs_attempt_reservations reservation
                     ON reservation.account_id = application.account_id
                    AND reservation.application_id = application.id
                   JOIN jobs_submission_evidence_capacity capacity
                     ON capacity.account_id = application.account_id
                    AND capacity.application_id = application.id
                    AND capacity.run_id = lease.run_id
                   JOIN jobs_application_ats_certification_bindings binding
                     ON binding.account_id = application.account_id
                    AND binding.application_id = application.id
                    AND binding.run_id = lease.run_id
                  WHERE application.id = ?1 AND lease.run_id = ?2
                    AND intervention.id = ?3",
                params![application.id, run_id, intervention.id],
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
        assert_eq!(
            terminal,
            (
                "click_started".to_string(),
                "open".to_string(),
                "running".to_string(),
                "active".to_string(),
                1,
                1,
            )
        );
    }

    #[test]
    fn intervention_answer_revises_cloud_packet_and_requires_review() {
        let pool = test_pool();
        let (mut application, run_id, browser_profile_id) =
            execution_lease_fixture(&pool, "answer-cloud");
        let approved_checksum = application
            .receipt
            .pointer("/approved_execution/checksum")
            .and_then(Value::as_str)
            .expect("fixture approved execution checksum")
            .to_string();
        let lease = claim_execution_lease(
            &pool,
            "acct-jobs",
            &application.id,
            &run_id,
            &browser_profile_id,
            "answer-worker",
        )
        .unwrap();
        assert_eq!(lease.phase, "prepared");
        assert!(
            update_attempt_reservation_status(&pool, "acct-jobs", &application.id, "running",)
                .unwrap()
        );
        assert_eq!(
            list_attempt_reservations(&pool, "acct-jobs").unwrap()[0].status,
            "running"
        );
        application = update_application(&pool, "acct-jobs", &application.id, "running", None)
            .unwrap()
            .unwrap();
        application = update_application(&pool, "acct-jobs", &application.id, "needs_input", None)
            .unwrap()
            .unwrap();
        upsert_browser_session(
            &pool,
            "acct-jobs",
            &BrowserSession {
                id: run_id.clone(),
                runner: "cloud".to_string(),
                status: "needs_input".to_string(),
                current_company: "Acme".to_string(),
                current_step: "Waiting for an application answer".to_string(),
                application_id: Some(application.id.clone()),
                takeover_url: Some("https://takeover.example.test/session".to_string()),
                created_at_ms: 0,
                updated_at_ms: 0,
            },
        )
        .unwrap();
        let intervention = answer_intervention_fixture(&pool, &application.id, "answer-cloud");

        let revision = resolve_intervention_answer_for_review(
            &pool,
            "acct-jobs",
            &intervention.id,
            "Yes, up to 25%.",
        )
        .unwrap();

        assert_eq!(revision.application.state, "awaiting_review");
        assert_eq!(revision.application.run_id, None);
        assert_eq!(revision.question, "Are you willing to travel?");
        assert!(revision
            .application
            .receipt
            .get("approved_execution")
            .is_none());
        assert_eq!(
            revision
                .application
                .receipt
                .pointer("/packet_revision/invalidated_packet_checksum")
                .and_then(Value::as_str),
            Some(approved_checksum.as_str())
        );
        assert_eq!(
            revision
                .application
                .receipt
                .pointer("/packet_revision/reapproval_required")
                .and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(
            revision
                .application
                .receipt
                .pointer("/final_answers/0/value")
                .and_then(Value::as_str),
            Some("Yes, up to 25%.")
        );
        assert_eq!(
            revision
                .application
                .receipt
                .pointer("/packet_revisions/0/intervention_id")
                .and_then(Value::as_str),
            Some(intervention.id.as_str())
        );

        let stored = get_application(&pool, "acct-jobs", &application.id)
            .unwrap()
            .unwrap();
        assert_eq!(stored.state, "awaiting_review");
        assert_eq!(stored.run_id, None);
        let interventions = list_interventions(&pool, "acct-jobs").unwrap();
        let stored_intervention = interventions
            .iter()
            .find(|item| item.id == intervention.id)
            .unwrap();
        assert_eq!(stored_intervention.status, "resolved");
        assert_eq!(
            stored_intervention
                .metadata
                .get("resolved_answer")
                .and_then(Value::as_str),
            Some("Yes, up to 25%.")
        );
        let lease_phase: String = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT phase FROM jobs_execution_leases WHERE run_id = ?1",
                params![run_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(lease_phase, "released");
        assert_eq!(
            list_attempt_reservations(&pool, "acct-jobs").unwrap()[0].status,
            "released"
        );
        let session = list_browser_sessions(&pool, "acct-jobs")
            .unwrap()
            .into_iter()
            .find(|item| item.id == run_id)
            .unwrap();
        assert_eq!(session.status, "paused");
        assert_eq!(
            session.current_step,
            "Application kit changed; review required"
        );
    }

    #[test]
    fn intervention_answer_revises_local_packet_and_invalidates_ticket() {
        let pool = test_pool();
        let (mut application, run_id, ticket_hash, _) =
            local_run_authority_fixture(&pool, "answer-local");
        assert!(claim_local_run_ticket(&pool, &run_id, &ticket_hash)
            .unwrap()
            .is_some());
        assert!(
            update_local_run_ticket_status(&pool, &run_id, &ticket_hash, "needs_input").unwrap()
        );
        application = update_application(&pool, "acct-jobs", &application.id, "running", None)
            .unwrap()
            .unwrap();
        application = update_application(&pool, "acct-jobs", &application.id, "needs_input", None)
            .unwrap()
            .unwrap();
        upsert_browser_session(
            &pool,
            "acct-jobs",
            &BrowserSession {
                id: run_id.clone(),
                runner: "local".to_string(),
                status: "needs_input".to_string(),
                current_company: "Acme".to_string(),
                current_step: "Waiting for an application answer".to_string(),
                application_id: Some(application.id.clone()),
                takeover_url: None,
                created_at_ms: 0,
                updated_at_ms: 0,
            },
        )
        .unwrap();
        let intervention = answer_intervention_fixture(&pool, &application.id, "answer-local");

        let revision =
            resolve_intervention_answer_for_review(&pool, "acct-jobs", &intervention.id, "No")
                .unwrap();

        assert_eq!(revision.application.state, "awaiting_review");
        assert_eq!(revision.application.run_id, None);
        assert!(revision
            .application
            .receipt
            .get("approved_execution")
            .is_none());
        let ticket_status: String = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT status FROM jobs_local_run_tickets WHERE id = ?1",
                params![run_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(ticket_status, "failed");
        let session = list_browser_sessions(&pool, "acct-jobs")
            .unwrap()
            .into_iter()
            .find(|item| item.id == run_id)
            .unwrap();
        assert_eq!(session.status, "paused");
    }

    #[test]
    fn local_intervention_answer_is_rejected_after_submission_click_started() {
        let pool = test_pool();
        let (application, run_id, ticket_hash) =
            local_click_started_fixture(&pool, "answer-local-after-click");
        let application =
            update_application(&pool, "acct-jobs", &application.id, "needs_input", None)
                .unwrap()
                .unwrap();
        let proof = stored_final_submit_proof(&application).unwrap();
        let intervention =
            answer_intervention_fixture(&pool, &application.id, "answer-local-after-click");

        let error =
            resolve_intervention_answer_for_review(&pool, "acct-jobs", &intervention.id, "Yes")
                .unwrap_err();
        assert!(error.to_string().contains("awaiting reconciliation"));

        let stored = get_application(&pool, "acct-jobs", &application.id)
            .unwrap()
            .unwrap();
        assert_eq!(stored.state, "needs_input");
        assert_eq!(stored.run_id.as_deref(), Some(run_id.as_str()));
        assert_eq!(stored_final_submit_proof(&stored).unwrap(), proof);
        assert!(stored.receipt.get("approved_execution").is_some());
        assert!(stored.receipt.get("packet_revision").is_none());
        let stored_intervention = list_interventions(&pool, "acct-jobs")
            .unwrap()
            .into_iter()
            .find(|item| item.id == intervention.id)
            .unwrap();
        assert_eq!(stored_intervention.status, "open");
        let (ticket_status, capacity_state): (String, String) = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT ticket.status, capacity.state
                   FROM jobs_local_run_tickets ticket
                   JOIN jobs_submission_evidence_capacity capacity
                     ON capacity.account_id = ticket.account_id
                    AND capacity.application_id = ticket.application_id
                    AND capacity.run_id = ticket.id
                  WHERE ticket.id = ?1 AND ticket.ticket_hash = ?2",
                params![run_id, ticket_hash],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(ticket_status, "click_started");
        assert_eq!(capacity_state, "active");
    }

    #[test]
    fn intervention_answer_is_rejected_after_submission_click_started() {
        let pool = test_pool();
        let (mut application, run_id, browser_profile_id) =
            execution_lease_fixture(&pool, "answer-after-click");
        let lease = claim_execution_lease(
            &pool,
            "acct-jobs",
            &application.id,
            &run_id,
            &browser_profile_id,
            "click-worker",
        )
        .unwrap();
        update_attempt_reservation_status(&pool, "acct-jobs", &application.id, "running").unwrap();
        application = update_application(&pool, "acct-jobs", &application.id, "running", None)
            .unwrap()
            .unwrap();
        start_irreversible_submission(
            &pool,
            "acct-jobs",
            &application.id,
            &run_id,
            &lease.lease_token,
            lease.fence,
            &test_final_submit_proof(&application),
            &test_submission_evidence_capacity(&application.id, &run_id),
        )
        .unwrap();
        application = update_application(&pool, "acct-jobs", &application.id, "needs_input", None)
            .unwrap()
            .unwrap();
        upsert_browser_session(
            &pool,
            "acct-jobs",
            &BrowserSession {
                id: run_id.clone(),
                runner: "cloud".to_string(),
                status: "needs_input".to_string(),
                current_company: "Acme".to_string(),
                current_step: "Submission outcome needs reconciliation".to_string(),
                application_id: Some(application.id.clone()),
                takeover_url: None,
                created_at_ms: 0,
                updated_at_ms: 0,
            },
        )
        .unwrap();
        let intervention =
            answer_intervention_fixture(&pool, &application.id, "answer-after-click");

        let error =
            resolve_intervention_answer_for_review(&pool, "acct-jobs", &intervention.id, "Yes")
                .unwrap_err();
        assert!(error.to_string().contains("awaiting reconciliation"));

        let stored = get_application(&pool, "acct-jobs", &application.id)
            .unwrap()
            .unwrap();
        assert_eq!(stored.state, "needs_input");
        assert_eq!(stored.run_id.as_deref(), Some(run_id.as_str()));
        assert!(stored.receipt.get("approved_execution").is_some());
        assert!(stored.receipt.get("packet_revision").is_none());
        let stored_intervention = list_interventions(&pool, "acct-jobs")
            .unwrap()
            .into_iter()
            .find(|item| item.id == intervention.id)
            .unwrap();
        assert_eq!(stored_intervention.status, "open");
        let lease_phase: String = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT phase FROM jobs_execution_leases WHERE run_id = ?1",
                params![run_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(lease_phase, "click_started");
        assert_eq!(
            list_attempt_reservations(&pool, "acct-jobs").unwrap()[0].status,
            "running"
        );
    }

    #[test]
    fn application_queue_hold_blocks_even_active_reservation_replay_until_release() {
        let pool = test_pool();
        let (application, run_id, _) = execution_lease_fixture(&pool, "held-queue");
        let application_before = serde_json::to_value(
            get_application(&pool, "acct-jobs", &application.id)
                .unwrap()
                .unwrap(),
        )
        .unwrap();
        let reservations_before = list_attempt_reservations(&pool, "acct-jobs").unwrap();
        let reserved_runner = reservations_before
            .iter()
            .find(|reservation| reservation.application_id == application.id)
            .expect("held queue fixture has an active reservation")
            .runner
            .clone();
        let browser_sessions_before =
            serde_json::to_value(list_browser_sessions(&pool, "acct-jobs").unwrap()).unwrap();
        let run_events_before =
            serde_json::to_value(list_run_events(&pool, "acct-jobs", &run_id).unwrap()).unwrap();
        append_account_operational_hold(
            &pool,
            OperationalCapability::ApplicationQueue,
            "held-queue-1",
            OperationalHoldTransition::Held,
            0,
            None,
        );

        let error =
            reserve_application_attempt(&pool, "acct-jobs", &application.id, &reserved_runner)
                .unwrap_err();
        assert!(matches!(
            error.downcast_ref::<OperationalHoldError>(),
            Some(OperationalHoldError::Held(_))
        ));
        let persistence_error =
            update_application(&pool, "acct-jobs", &application.id, "queued", None).unwrap_err();
        assert!(matches!(
            persistence_error.downcast_ref::<OperationalHoldError>(),
            Some(OperationalHoldError::Held(_))
        ));
        let stored_application = get_application(&pool, "acct-jobs", &application.id)
            .unwrap()
            .unwrap();
        assert_eq!(stored_application.state, "queued");
        assert_eq!(stored_application.run_id.as_deref(), Some(run_id.as_str()));
        assert_eq!(
            serde_json::to_value(stored_application).unwrap(),
            application_before
        );
        assert_eq!(
            list_attempt_reservations(&pool, "acct-jobs").unwrap(),
            reservations_before
        );
        assert_eq!(
            serde_json::to_value(list_browser_sessions(&pool, "acct-jobs").unwrap()).unwrap(),
            browser_sessions_before
        );
        assert_eq!(
            serde_json::to_value(list_run_events(&pool, "acct-jobs", &run_id).unwrap()).unwrap(),
            run_events_before
        );

        append_account_operational_hold(
            &pool,
            OperationalCapability::ApplicationQueue,
            "held-queue-2",
            OperationalHoldTransition::Released,
            1,
            Some("held-queue-1"),
        );
        assert_eq!(
            reserve_application_attempt(&pool, "acct-jobs", &application.id, &reserved_runner,)
                .unwrap()
                .runner,
            reserved_runner
        );
    }

    #[test]
    fn runner_claim_hold_creates_no_cloud_lease_until_release() {
        let pool = test_pool();
        let (application, run_id, browser_profile_id) =
            execution_lease_fixture(&pool, "held-runner-claim");
        append_account_operational_hold(
            &pool,
            OperationalCapability::RunnerClaim,
            "held-runner-claim-1",
            OperationalHoldTransition::Held,
            0,
            None,
        );

        assert!(matches!(
            claim_execution_lease(
                &pool,
                "acct-jobs",
                &application.id,
                &run_id,
                &browser_profile_id,
                "held-owner",
            ),
            Err(ExecutionLeaseError::Conflict)
        ));
        assert_eq!(
            pool.get()
                .unwrap()
                .query_row("SELECT COUNT(*) FROM jobs_execution_leases", [], |row| {
                    row.get::<_, i64>(0)
                })
                .unwrap(),
            0
        );

        append_account_operational_hold(
            &pool,
            OperationalCapability::RunnerClaim,
            "held-runner-claim-2",
            OperationalHoldTransition::Released,
            1,
            Some("held-runner-claim-1"),
        );
        assert_eq!(
            claim_execution_lease(
                &pool,
                "acct-jobs",
                &application.id,
                &run_id,
                &browser_profile_id,
                "held-owner",
            )
            .unwrap()
            .phase,
            "prepared"
        );
    }

    #[test]
    fn local_runner_hold_blocks_new_claim_and_exact_reissue_until_release() {
        let pool = test_pool();
        let (application, run_id, ticket_hash, _) =
            local_run_authority_fixture_unbound(&pool, "held-local-claim");
        reserve_application_attempt(&pool, "acct-jobs", &application.id, "local").unwrap();
        let descriptor = seed_test_local_browser_release_authority(&pool);
        let nonce = "d".repeat(64);
        append_account_operational_hold(
            &pool,
            OperationalCapability::RunnerClaim,
            "held-local-claim-1",
            OperationalHoldTransition::Held,
            0,
            None,
        );

        assert!(matches!(
            claim_local_run_with_browser_release(
                &pool,
                &run_id,
                &ticket_hash,
                &nonce,
                &descriptor,
                "test-server",
                |_, _| anyhow::bail!("held claim must not mint a response"),
            )
            .unwrap(),
            BrowserLocalRunClaimDisposition::DistributionUnavailable
        ));
        let (ticket_status, replay_count): (String, i64) = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT ticket.status,
                        (SELECT COUNT(*) FROM jobs_local_run_claim_replays replay
                          WHERE replay.run_id = ticket.id)
                   FROM jobs_local_run_tickets ticket WHERE ticket.id = ?1",
                params![run_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(ticket_status, "queued");
        assert_eq!(replay_count, 0);

        append_account_operational_hold(
            &pool,
            OperationalCapability::RunnerClaim,
            "held-local-claim-2",
            OperationalHoldTransition::Released,
            1,
            Some("held-local-claim-1"),
        );
        let first = claim_local_run_with_browser_release(
            &pool,
            &run_id,
            &ticket_hash,
            &nonce,
            &descriptor,
            "test-server",
            |ticket, _| Ok(json!({ "runId": ticket.id })),
        )
        .unwrap();
        let BrowserLocalRunClaimDisposition::Success(first) = first else {
            panic!("released local claim did not succeed")
        };
        append_account_operational_hold(
            &pool,
            OperationalCapability::RunnerClaim,
            "held-local-claim-3",
            OperationalHoldTransition::Held,
            2,
            Some("held-local-claim-2"),
        );
        assert!(matches!(
            claim_local_run_with_browser_release(
                &pool,
                &run_id,
                &ticket_hash,
                &nonce,
                &descriptor,
                "test-server",
                |_, _| anyhow::bail!("held replay must not mint a fresh response"),
            )
            .unwrap(),
            BrowserLocalRunClaimDisposition::DistributionUnavailable
        ));
        append_account_operational_hold(
            &pool,
            OperationalCapability::RunnerClaim,
            "held-local-claim-4",
            OperationalHoldTransition::Released,
            3,
            Some("held-local-claim-3"),
        );
        let replay = claim_local_run_with_browser_release(
            &pool,
            &run_id,
            &ticket_hash,
            &nonce,
            &descriptor,
            "test-server",
            |_, _| anyhow::bail!("exact replay must not mint a fresh response"),
        )
        .unwrap();
        let BrowserLocalRunClaimDisposition::Success(replay) = replay else {
            panic!("released exact local claim replay did not recover")
        };
        assert!(replay.replayed);
        assert_eq!(replay.response_json, first.response_json);
    }

    #[test]
    fn fix_728_employer_domain_runner_claim_holds_leave_local_and_cloud_unmodified() {
        let local_pool = test_pool();
        let (local_application, local_run_id, local_ticket_hash, _) =
            local_run_authority_fixture_unbound(&local_pool, "employer-domain-local-claim");
        reserve_application_attempt(&local_pool, "acct-jobs", &local_application.id, "local")
            .unwrap();
        let descriptor = seed_test_local_browser_release_authority(&local_pool);
        let local_domain = application_job_integrity_receipt(&local_application)
            .unwrap()
            .expect("local claim has signed job-integrity authority")
            .canonical_employer_domain;
        append_operational_hold_for_scope(
            &local_pool,
            OperationalCapability::RunnerClaim,
            OperationalHoldScopeKind::EmployerDomain,
            &local_domain,
            "fix-728-local-domain-claim-held",
            OperationalHoldTransition::Held,
            0,
            None,
        );
        let local_before = (
            local_claim_state(&local_pool, &local_application.id, &local_run_id),
            local_pool
                .get()
                .unwrap()
                .query_row(
                    "SELECT COUNT(*) FROM jobs_local_run_claim_replays WHERE run_id = ?1",
                    params![local_run_id],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
            local_pool
                .get()
                .unwrap()
                .query_row(
                    "SELECT COUNT(*) FROM jobs_application_ats_certification_bindings
                      WHERE application_id = ?1 AND run_id = ?2",
                    params![local_application.id, local_run_id],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
        );
        assert!(matches!(
            claim_local_run_with_browser_release(
                &local_pool,
                &local_run_id,
                &local_ticket_hash,
                &"7".repeat(64),
                &descriptor,
                "test-server",
                |_, _| anyhow::bail!("held local claim must not mint a response"),
            )
            .unwrap(),
            BrowserLocalRunClaimDisposition::DistributionUnavailable
        ));
        let local_after = (
            local_claim_state(&local_pool, &local_application.id, &local_run_id),
            local_pool
                .get()
                .unwrap()
                .query_row(
                    "SELECT COUNT(*) FROM jobs_local_run_claim_replays WHERE run_id = ?1",
                    params![local_run_id],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
            local_pool
                .get()
                .unwrap()
                .query_row(
                    "SELECT COUNT(*) FROM jobs_application_ats_certification_bindings
                      WHERE application_id = ?1 AND run_id = ?2",
                    params![local_application.id, local_run_id],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
        );
        assert_eq!(local_after, local_before);

        let cloud_pool = test_pool();
        let (cloud_application, cloud_run_id, cloud_browser_profile_id) =
            execution_lease_fixture(&cloud_pool, "employer-domain-cloud-claim");
        let cloud_domain = application_job_integrity_receipt(&cloud_application)
            .unwrap()
            .expect("cloud claim has signed job-integrity authority")
            .canonical_employer_domain;
        let cloud_state = |pool: &DbPool| {
            pool.get()
                .unwrap()
                .query_row(
                    "SELECT application.state, reservation.runner, reservation.status,
                            session.status,
                            (SELECT COUNT(*) FROM jobs_execution_leases
                              WHERE application_id = application.id),
                            (SELECT COUNT(*) FROM jobs_application_ats_certification_bindings
                              WHERE application_id = application.id),
                            (SELECT COUNT(*) FROM jobs_submission_evidence_capacity
                              WHERE application_id = application.id)
                       FROM jobs_applications application
                       JOIN jobs_attempt_reservations reservation
                         ON reservation.account_id = application.account_id
                        AND reservation.application_id = application.id
                       JOIN jobs_browser_sessions session
                         ON session.account_id = application.account_id AND session.id = ?2
                      WHERE application.account_id = 'acct-jobs' AND application.id = ?1",
                    params![cloud_application.id, cloud_run_id],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, String>(3)?,
                            row.get::<_, i64>(4)?,
                            row.get::<_, i64>(5)?,
                            row.get::<_, i64>(6)?,
                        ))
                    },
                )
                .unwrap()
        };
        append_operational_hold_for_scope(
            &cloud_pool,
            OperationalCapability::RunnerClaim,
            OperationalHoldScopeKind::EmployerDomain,
            &cloud_domain,
            "fix-728-cloud-domain-claim-held",
            OperationalHoldTransition::Held,
            0,
            None,
        );
        let cloud_before = cloud_state(&cloud_pool);
        assert!(matches!(
            claim_execution_lease(
                &cloud_pool,
                "acct-jobs",
                &cloud_application.id,
                &cloud_run_id,
                &cloud_browser_profile_id,
                "fix-728-held-cloud-worker",
            ),
            Err(ExecutionLeaseError::Conflict)
        ));
        assert_eq!(cloud_state(&cloud_pool), cloud_before);

        let spoof_pool = test_pool();
        let (mut spoof_application, spoof_run_id, spoof_ticket_hash, _) =
            local_run_authority_fixture_unbound(&spoof_pool, "employer-domain-active-spoof");
        reserve_application_attempt(&spoof_pool, "acct-jobs", &spoof_application.id, "local")
            .unwrap();
        let spoof_descriptor = seed_test_local_browser_release_authority(&spoof_pool);
        spoof_application.receipt["job_integrity"]["canonicalEmployerDomain"] =
            json!("mutable-spoof.example");
        let spoof_payload = to_json(&spoof_application, "spoofed local claim application").unwrap();
        spoof_pool
            .get()
            .unwrap()
            .execute(
                "UPDATE jobs_applications SET application_json = ?2 WHERE id = ?1",
                params![spoof_application.id, spoof_payload],
            )
            .unwrap();
        let spoof_before = local_claim_state(&spoof_pool, &spoof_application.id, &spoof_run_id);
        assert!(matches!(
            claim_local_run_with_browser_release(
                &spoof_pool,
                &spoof_run_id,
                &spoof_ticket_hash,
                &"8".repeat(64),
                &spoof_descriptor,
                "test-server",
                |_, _| anyhow::bail!("spoofed authority must not mint a response"),
            )
            .unwrap(),
            BrowserLocalRunClaimDisposition::Rejected
        ));
        assert_eq!(
            local_claim_state(&spoof_pool, &spoof_application.id, &spoof_run_id),
            spoof_before
        );
    }

    #[test]
    fn fix_728_employer_domain_final_submit_holds_leave_local_and_cloud_unmodified() {
        let local_pool = test_pool();
        let (local_application, local_run_id, local_ticket_hash, local_proof) =
            certified_local_intervention_fixture(&local_pool, "employer-domain-local-submit");
        let local_domain = application_job_integrity_receipt(&local_application)
            .unwrap()
            .expect("local submit has signed job-integrity authority")
            .canonical_employer_domain;
        let mut local_capacity =
            test_submission_evidence_capacity(&local_application.id, &local_run_id);
        local_capacity.runner = "local".to_string();
        append_operational_hold_for_scope(
            &local_pool,
            OperationalCapability::FinalSubmit,
            OperationalHoldScopeKind::EmployerDomain,
            &local_domain,
            "fix-728-local-domain-submit-held",
            OperationalHoldTransition::Held,
            0,
            None,
        );
        let local_before_application = serde_json::to_value(
            get_application(&local_pool, "acct-jobs", &local_application.id)
                .unwrap()
                .unwrap(),
        )
        .unwrap();
        let local_boundary_state = |pool: &DbPool| {
            pool.get()
                .unwrap()
                .query_row(
                    "SELECT ticket.status, reservation.status, binding.phase, binding.fence,
                            binding.phase_b_request_sha256,
                            (SELECT COUNT(*) FROM jobs_ats_certification_canary_reservations
                              WHERE binding_id = binding.binding_id),
                            (SELECT COUNT(*) FROM jobs_submission_evidence_capacity
                              WHERE application_id = binding.application_id
                                AND run_id = binding.run_id)
                       FROM jobs_local_run_tickets ticket
                       JOIN jobs_attempt_reservations reservation
                         ON reservation.account_id = ticket.account_id
                        AND reservation.application_id = ticket.application_id
                       JOIN jobs_application_ats_certification_bindings binding
                         ON binding.account_id = ticket.account_id
                        AND binding.application_id = ticket.application_id
                        AND binding.run_id = ticket.id
                      WHERE ticket.id = ?1",
                    params![local_run_id],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, i64>(3)?,
                            row.get::<_, Option<String>>(4)?,
                            row.get::<_, i64>(5)?,
                            row.get::<_, i64>(6)?,
                        ))
                    },
                )
                .unwrap()
        };
        let local_before = local_boundary_state(&local_pool);
        assert!(!local_run_submit_authorized(
            &local_pool,
            &local_run_id,
            &local_ticket_hash,
            &local_proof,
            &local_capacity,
        )
        .unwrap());
        assert_eq!(local_boundary_state(&local_pool), local_before);
        assert_eq!(
            serde_json::to_value(
                get_application(&local_pool, "acct-jobs", &local_application.id)
                    .unwrap()
                    .unwrap()
            )
            .unwrap(),
            local_before_application
        );

        let cloud_pool = test_pool();
        let cloud = certified_cloud_execution_fixture(&cloud_pool, "employer-domain-cloud-submit");
        let cloud_domain = application_job_integrity_receipt(&cloud.application)
            .unwrap()
            .expect("cloud submit has signed job-integrity authority")
            .canonical_employer_domain;
        let cloud_capacity =
            test_submission_evidence_capacity(&cloud.application.id, &cloud.run_id);
        append_operational_hold_for_scope(
            &cloud_pool,
            OperationalCapability::FinalSubmit,
            OperationalHoldScopeKind::EmployerDomain,
            &cloud_domain,
            "fix-728-cloud-domain-submit-held",
            OperationalHoldTransition::Held,
            0,
            None,
        );
        let cloud_before_application = serde_json::to_value(
            get_application(&cloud_pool, "acct-jobs", &cloud.application.id)
                .unwrap()
                .unwrap(),
        )
        .unwrap();
        let cloud_boundary_state = |pool: &DbPool| {
            pool.get()
                .unwrap()
                .query_row(
                    "SELECT lease.phase, reservation.status, binding.phase, binding.fence,
                            binding.phase_b_request_sha256,
                            (SELECT COUNT(*) FROM jobs_ats_certification_canary_reservations
                              WHERE binding_id = binding.binding_id),
                            (SELECT COUNT(*) FROM jobs_submission_evidence_capacity
                              WHERE application_id = binding.application_id
                                AND run_id = binding.run_id)
                       FROM jobs_execution_leases lease
                       JOIN jobs_attempt_reservations reservation
                         ON reservation.account_id = lease.account_id
                        AND reservation.application_id = lease.application_id
                       JOIN jobs_application_ats_certification_bindings binding
                         ON binding.account_id = lease.account_id
                        AND binding.application_id = lease.application_id
                        AND binding.run_id = lease.run_id
                      WHERE lease.run_id = ?1",
                    params![cloud.run_id],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, i64>(3)?,
                            row.get::<_, Option<String>>(4)?,
                            row.get::<_, i64>(5)?,
                            row.get::<_, i64>(6)?,
                        ))
                    },
                )
                .unwrap()
        };
        let cloud_before = cloud_boundary_state(&cloud_pool);
        assert!(matches!(
            start_irreversible_submission(
                &cloud_pool,
                "acct-jobs",
                &cloud.application.id,
                &cloud.run_id,
                &cloud.lease_token,
                cloud.lease_fence,
                &cloud.proof,
                &cloud_capacity,
            ),
            Err(ExecutionLeaseError::Conflict)
        ));
        assert_eq!(cloud_boundary_state(&cloud_pool), cloud_before);
        assert_eq!(
            serde_json::to_value(
                get_application(&cloud_pool, "acct-jobs", &cloud.application.id)
                    .unwrap()
                    .unwrap()
            )
            .unwrap(),
            cloud_before_application
        );
    }

    #[test]
    fn final_submit_hold_blocks_before_marker_but_not_exact_post_marker_replay() {
        let pool = test_pool();
        let fixture = certified_cloud_execution_fixture(&pool, "held-final-submit");
        let capacity = test_submission_evidence_capacity(&fixture.application.id, &fixture.run_id);
        append_account_operational_hold(
            &pool,
            OperationalCapability::FinalSubmit,
            "held-final-submit-1",
            OperationalHoldTransition::Held,
            0,
            None,
        );

        assert!(matches!(
            start_irreversible_submission(
                &pool,
                "acct-jobs",
                &fixture.application.id,
                &fixture.run_id,
                &fixture.lease_token,
                fixture.lease_fence,
                &fixture.proof,
                &capacity,
            ),
            Err(ExecutionLeaseError::Conflict)
        ));
        let (phase, capacity_count): (String, i64) = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT lease.phase,
                        (SELECT COUNT(*) FROM jobs_submission_evidence_capacity capacity
                          WHERE capacity.account_id = lease.account_id
                            AND capacity.application_id = lease.application_id
                            AND capacity.run_id = lease.run_id)
                   FROM jobs_execution_leases lease WHERE lease.run_id = ?1",
                params![fixture.run_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(phase, "prepared");
        assert_eq!(capacity_count, 0);

        append_account_operational_hold(
            &pool,
            OperationalCapability::FinalSubmit,
            "held-final-submit-2",
            OperationalHoldTransition::Released,
            1,
            Some("held-final-submit-1"),
        );
        let first = start_irreversible_submission(
            &pool,
            "acct-jobs",
            &fixture.application.id,
            &fixture.run_id,
            &fixture.lease_token,
            fixture.lease_fence,
            &fixture.proof,
            &capacity,
        )
        .unwrap();
        append_account_operational_hold(
            &pool,
            OperationalCapability::FinalSubmit,
            "held-final-submit-3",
            OperationalHoldTransition::Held,
            2,
            Some("held-final-submit-2"),
        );
        let replay = start_irreversible_submission(
            &pool,
            "acct-jobs",
            &fixture.application.id,
            &fixture.run_id,
            &fixture.lease_token,
            fixture.lease_fence,
            &fixture.proof,
            &capacity,
        )
        .unwrap();
        assert_eq!(replay, first);
    }

    #[test]
    fn local_final_submit_hold_blocks_new_marker_but_not_exact_click_started_replay() {
        let pool = test_pool();
        let (application, run_id, ticket_hash, proof) =
            certified_local_intervention_fixture(&pool, "held-local-submit");
        let mut capacity = test_submission_evidence_capacity(&application.id, &run_id);
        capacity.runner = "local".to_string();
        append_account_operational_hold(
            &pool,
            OperationalCapability::FinalSubmit,
            "held-local-submit-1",
            OperationalHoldTransition::Held,
            0,
            None,
        );

        assert!(
            !local_run_submit_authorized(&pool, &run_id, &ticket_hash, &proof, &capacity,).unwrap()
        );
        let ticket_status: String = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT status FROM jobs_local_run_tickets WHERE id = ?1",
                params![run_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(ticket_status, "claimed");
        assert_eq!(
            submission_capacity_count(&pool, &application.id, &run_id),
            0
        );

        append_account_operational_hold(
            &pool,
            OperationalCapability::FinalSubmit,
            "held-local-submit-2",
            OperationalHoldTransition::Released,
            1,
            Some("held-local-submit-1"),
        );
        let first = local_run_submit_authorization_inner(
            &pool,
            &run_id,
            &ticket_hash,
            "test-server",
            &proof,
            &capacity,
            false,
        )
        .unwrap()
        .expect("released local pre-click authority");
        assert!(first.ats_certified_receipt_authority.is_some());
        assert_eq!(
            pool.get()
                .unwrap()
                .query_row(
                    "SELECT status FROM jobs_local_run_tickets WHERE id = ?1",
                    params![run_id],
                    |row| row.get::<_, String>(0),
                )
                .unwrap(),
            "click_started"
        );
        let stable_state = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT binding.phase, binding.fence, binding.phase_b_request_sha256,
                        (SELECT COUNT(*) FROM jobs_ats_certification_canary_reservations
                          WHERE binding_id = binding.binding_id),
                        (SELECT COUNT(*) FROM jobs_submission_evidence_capacity capacity
                          WHERE capacity.account_id = binding.account_id
                            AND capacity.application_id = binding.application_id
                            AND capacity.run_id = binding.run_id)
                   FROM jobs_application_ats_certification_bindings binding
                  WHERE binding.account_id = 'acct-jobs'
                    AND binding.application_id = ?1 AND binding.run_id = ?2",
                params![application.id, run_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, i64>(3)?,
                        row.get::<_, i64>(4)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(stable_state.0, "consumed");
        assert_eq!(stable_state.1, 1);
        assert_eq!(stable_state.2.len(), 64);
        assert_eq!((stable_state.3, stable_state.4), (1, 1));

        append_account_operational_hold(
            &pool,
            OperationalCapability::FinalSubmit,
            "held-local-submit-3",
            OperationalHoldTransition::Held,
            2,
            Some("held-local-submit-2"),
        );
        let replay = local_run_submit_authorization_inner(
            &pool,
            &run_id,
            &ticket_hash,
            "test-server",
            &proof,
            &capacity,
            false,
        )
        .unwrap()
        .expect("held exact click-started replay");
        assert_eq!(replay, first);

        let replay_after_accepted_server_deploy = local_run_submit_authorization_inner(
            &pool,
            &run_id,
            &ticket_hash,
            "alternate-test-server",
            &proof,
            &capacity,
            false,
        )
        .unwrap()
        .expect("accepted server deployment must not strand click-started recovery");
        assert_eq!(replay_after_accepted_server_deploy, first);

        let mut changed_proof = proof.clone();
        changed_proof.adapter_version = "changed-adapter-version".to_string();
        assert!(local_run_submit_authorization_inner(
            &pool,
            &run_id,
            &ticket_hash,
            "test-server",
            &changed_proof,
            &capacity,
            false,
        )
        .unwrap()
        .is_none());

        let mut changed_capacity = capacity.clone();
        changed_capacity.reserved_bytes += 1;
        assert!(local_run_submit_authorization_inner(
            &pool,
            &run_id,
            &ticket_hash,
            "test-server",
            &proof,
            &changed_capacity,
            false,
        )
        .unwrap()
        .is_none());
        assert!(local_run_submit_authorization_inner(
            &pool,
            &run_id,
            &ticket_hash,
            "changed-server-release",
            &proof,
            &capacity,
            false,
        )
        .unwrap()
        .is_none());

        let after_replays = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT binding.phase, binding.fence, binding.phase_b_request_sha256,
                        (SELECT COUNT(*) FROM jobs_ats_certification_canary_reservations
                          WHERE binding_id = binding.binding_id),
                        (SELECT COUNT(*) FROM jobs_submission_evidence_capacity capacity
                          WHERE capacity.account_id = binding.account_id
                            AND capacity.application_id = binding.application_id
                            AND capacity.run_id = binding.run_id)
                   FROM jobs_application_ats_certification_bindings binding
                  WHERE binding.account_id = 'acct-jobs'
                    AND binding.application_id = ?1 AND binding.run_id = ?2",
                params![application.id, run_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, i64>(3)?,
                        row.get::<_, i64>(4)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(after_replays, stable_state);
    }

    #[test]
    fn local_distribution_gate_blocks_new_submit_but_allows_exact_click_started_replay() {
        let pool = test_pool();
        let (application, run_id, ticket_hash, proof) =
            certified_local_intervention_fixture(&pool, "disabled-local-submit");
        let mut capacity = test_submission_evidence_capacity(&application.id, &run_id);
        capacity.runner = "local".to_string();

        assert!(!local_run_submit_authorized_for_distribution(
            &pool,
            &run_id,
            &ticket_hash,
            "test-server",
            &proof,
            &capacity,
        )
        .unwrap());
        assert_eq!(
            pool.get()
                .unwrap()
                .query_row(
                    "SELECT status FROM jobs_local_run_tickets WHERE id = ?1",
                    params![run_id],
                    |row| row.get::<_, String>(0),
                )
                .unwrap(),
            "claimed"
        );
        assert_eq!(
            submission_capacity_count(&pool, &application.id, &run_id),
            0
        );

        let first = local_run_submit_authorization_inner(
            &pool,
            &run_id,
            &ticket_hash,
            "test-server",
            &proof,
            &capacity,
            false,
        )
        .unwrap()
        .expect("server-side pre-click authority");
        let replay = local_run_submit_authorization_for_distribution(
            &pool,
            &run_id,
            &ticket_hash,
            "test-server",
            &proof,
            &capacity,
        )
        .unwrap()
        .expect("distribution-disabled exact click-started replay");
        assert_eq!(replay, first);
        assert_eq!(
            submission_capacity_count(&pool, &application.id, &run_id),
            1
        );
    }

    #[test]
    fn mailbox_provider_hold_skips_held_candidate_without_starving_allowed_provider() {
        let pool = test_pool();
        set_entitlement_plan(&pool, "acct-jobs", "pro").unwrap();
        let gmail = save_mailbox_connection(
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
            "google-subject-held-mailbox",
        )
        .unwrap();
        initialize_mailbox_sync_state(&pool, "acct-jobs", &gmail.id, "gmail").unwrap();
        let outlook = save_mailbox_connection(
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
            "microsoft-subject-allowed-mailbox",
        )
        .unwrap();
        initialize_mailbox_sync_state(&pool, "acct-jobs", &outlook.id, "outlook").unwrap();
        let due_now = now_ms();
        pool.get()
            .unwrap()
            .execute(
                "UPDATE jobs_provider_sync_state
                    SET next_sync_at_ms = CASE connection_id
                      WHEN ?1 THEN ?3 - 2 ELSE ?3 - 1 END
                  WHERE account_id = 'acct-jobs' AND connection_id IN (?1, ?2)",
                params![gmail.id, outlook.id, due_now],
            )
            .unwrap();
        append_operational_hold_for_scope(
            &pool,
            OperationalCapability::MailboxSync,
            OperationalHoldScopeKind::MailboxProvider,
            "gmail",
            "held-mailbox-1",
            OperationalHoldTransition::Held,
            0,
            None,
        );

        assert!(
            claim_mailbox_sync(&pool, "acct-jobs", &gmail.id, "direct-worker", 60_000,)
                .unwrap()
                .is_none()
        );
        let claimed = claim_due_mailbox_syncs(&pool, "batch-worker", 60_000, 1).unwrap();
        assert_eq!(claimed.len(), 1);
        assert_eq!(claimed[0].1.connection_id, outlook.id);

        append_operational_hold_for_scope(
            &pool,
            OperationalCapability::MailboxSync,
            OperationalHoldScopeKind::MailboxProvider,
            "gmail",
            "held-mailbox-2",
            OperationalHoldTransition::Released,
            1,
            Some("held-mailbox-1"),
        );
        assert!(
            claim_mailbox_sync(&pool, "acct-jobs", &gmail.id, "direct-worker", 60_000,)
                .unwrap()
                .is_some()
        );
    }
}
