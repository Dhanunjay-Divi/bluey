#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::db::object_uploads::{
        ApplicationObjectBinding, NewObjectUpload, NewSubmissionEvidenceCapacity, ObjectKind,
        StorageScope, UploadControlError,
    };
    use crate::object_storage::UploadLimits;

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
        posting.discovery_evidence = JobDiscoveryEvidence::verified_original_source(
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
        build_job_eligibility(
            posting,
            &profile,
            &JobPreferences::default(),
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
            .find("postgres_local_run_authority")
            .expect("ticket authority before registry lock");
        let reservation = claim
            .find("reservation_status")
            .expect("reservation authority before registry lock");
        let shared_lock = claim
            .find("postgres_lock_browser_release_registry_shared(tx)")
            .expect("shared registry lock");
        let release = claim
            .find("postgres_browser_release_for_claim_tx")
            .expect("release authority after registry lock");
        assert!(ticket < reservation && reservation < shared_lock && shared_lock < release);

        let submit_source = include_str!("local_runner.rs");
        let submit = submit_source
            .split("pub fn claim_authorized_local_run_ticket")
            .nth(1)
            .expect("pre-click submit implementation")
            .split("fn sqlite_local_run_authority")
            .next()
            .expect("bounded pre-click submit implementation");
        let ticket = submit
            .find("postgres_local_run_authority")
            .expect("ticket authority before registry lock");
        let shared_lock = submit
            .find("postgres_lock_browser_release_registry_shared(&mut tx)")
            .expect("shared registry lock");
        let release = submit
            .find("postgres_bound_browser_release_submit_allowed")
            .expect("bound release authority after registry lock");
        let capacity = submit
            .find("reserve_submission_evidence_capacity_postgres_tx")
            .expect("capacity reservation after release authority");
        assert!(ticket < shared_lock && shared_lock < release && release < capacity);

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

        let authorization =
            authorize_auto_submit(&pool, "acct-jobs", "jobs@example.com", "track-default").unwrap();
        assert_eq!(authorization.status, "active");
        assert_eq!(authorization.source_resume_asset_id, first_asset.id);

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
        let profile = default_profile("jobs@example.com");
        save_profile(pool, "acct-jobs", &profile).unwrap();
        let preferences = JobPreferences {
            sponsorship: "not_required".to_string(),
            ..JobPreferences::default()
        };
        save_preferences(pool, "acct-jobs", &preferences).unwrap();
        let posting = upsert_posting(
            pool,
            "acct-jobs",
            &test_posting(
                &format!("https://boards.greenhouse.io/acme/jobs/{suffix}"),
                now_ms(),
                now_ms(),
            ),
            &profile,
            &preferences,
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
        let mut application = assign_application_run(pool, "acct-jobs", &application.id, &run_id)
            .unwrap()
            .unwrap();
        let now = now_ms();
        pool.get()
            .unwrap()
            .execute(
                "INSERT INTO jobs_attempt_reservations (
                    id, account_id, application_id, company_key, period_key,
                    runner, status, reserved_at_ms, updated_at_ms
                 ) VALUES (?1, 'acct-jobs', ?2, ?3, 'test-period',
                           'unassigned', 'reserved', ?4, ?4)",
                params![
                    format!("attempt-{}", application.id),
                    application.id,
                    format!("test-company-{suffix}"),
                    now,
                ],
            )
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
        let browser_profile_id = execution_browser_profile_id("acct-jobs", &identity_id);
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
        let approved_packet = json!({
            "applicationId": application.id,
            "jobId": application.job_id,
            "resumeVersionId": resume.id,
            "resumeContent": resume.content,
            "coverLetterContent": application.cover_letter,
            "answers": {},
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
            "source": posting.source,
            "compensation": posting.compensation,
        });
        let admission = json!({ "kind": "review_approval" });
        let checksum =
            approved_submission_checksum(2, &approved_packet, &approved_job, Some(&admission))
                .unwrap();
        application.receipt["approved_execution"] = json!({
            "schema_version": 2,
            "approved_at_ms": now_ms(),
            "checksum": checksum,
            "admission": admission,
            "packet": approved_packet,
            "job": approved_job,
        });
        application = replace_application_receipt(
            pool,
            "acct-jobs",
            &application.id,
            application.receipt.clone(),
        )
        .unwrap()
        .unwrap();
        (application, run_id, browser_profile_id)
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
        let mut documents = Vec::new();
        if application
            .receipt
            .pointer("/approved_execution/packet/coverLetterContent")
            .and_then(Value::as_str)
            .is_some_and(|value| !value.trim().is_empty())
        {
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
        }
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
        receipt.as_object_mut().unwrap().insert(
            SERVER_SUBMISSION_AUTHORITY_KEY.to_string(),
            json!({
                "schemaVersion": 1,
                "preSubmissionReceipt": application.receipt,
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
        let profile = default_profile("jobs@example.com");
        save_profile(pool, "acct-jobs", &profile).unwrap();
        let preferences = JobPreferences {
            sponsorship: "not_required".to_string(),
            ..JobPreferences::default()
        };
        save_preferences(pool, "acct-jobs", &preferences).unwrap();
        set_entitlement_plan(pool, "acct-jobs", "pro").unwrap();
        let posting = upsert_posting(
            pool,
            "acct-jobs",
            &test_posting(
                &format!("https://boards.greenhouse.io/acme/jobs/{suffix}"),
                now_ms(),
                now_ms(),
            ),
            &profile,
            &preferences,
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
        let mut application = assign_application_run(pool, "acct-jobs", &application.id, &run_id)
            .unwrap()
            .unwrap();
        let identity_id = application
            .receipt
            .pointer("/application_identity/id")
            .and_then(Value::as_str)
            .unwrap()
            .to_string();
        let browser_profile_id = execution_browser_profile_id("acct-jobs", &identity_id);
        let identity_email = application
            .receipt
            .pointer("/application_identity/email")
            .and_then(Value::as_str)
            .unwrap()
            .to_string();
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
        let approved_packet = json!({
            "applicationId": application.id,
            "jobId": application.job_id,
            "resumeVersionId": resume.id,
            "resumeContent": resume.content,
            "coverLetterContent": application.cover_letter,
            "answers": {},
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
            "source": posting.source,
            "compensation": posting.compensation,
        });
        let admission = json!({ "kind": "review_approval" });
        let checksum =
            approved_submission_checksum(2, &approved_packet, &approved_job, Some(&admission))
                .unwrap();
        application.receipt["approved_execution"] = json!({
            "schema_version": 2,
            "approved_at_ms": now_ms(),
            "checksum": checksum,
            "admission": admission,
            "packet": approved_packet,
            "job": approved_job,
        });
        application = replace_application_receipt(
            pool,
            "acct-jobs",
            &application.id,
            application.receipt.clone(),
        )
        .unwrap()
        .unwrap();
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
                    '[\"test-server\"]', ?5, 'dGVzdA', 1,
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
        match error {
            ExecutionLeaseError::Storage(error) => assert!(matches!(
                error.downcast_ref::<UploadControlError>(),
                Some(UploadControlError::AccountDeleting)
            )),
            other => panic!("expected account-deletion storage fence, got {other:?}"),
        }

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
        assert_eq!(attempt_runner, "unassigned");
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
        assert_account_deleting(start_irreversible_submission(
            &pool,
            "acct-jobs",
            &application.id,
            &run_id,
            &lease.lease_token,
            lease.fence,
            &test_final_submit_proof(&application),
            &test_submission_evidence_capacity(&application.id, &run_id),
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
    fn current_original_source_evidence_allows_preparation_and_queueing() {
        let posting = test_posting(
            "https://boards.greenhouse.io/acme/jobs/current-evidence",
            now_ms(),
            now_ms(),
        );
        let decision = discovery_decision(&posting, true);
        assert!(decision.can_prepare, "{decision:?}");
        assert!(decision.can_queue_local, "{decision:?}");
        assert!(decision.can_queue_cloud, "{decision:?}");
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
    fn stale_original_source_evidence_requires_refresh_before_queueing() {
        let posting = test_posting(
            "https://boards.greenhouse.io/acme/jobs/stale-evidence",
            now_ms(),
            now_ms() - 2 * DAY_MS,
        );
        let decision = discovery_decision(&posting, false);
        assert!(decision.can_prepare, "{decision:?}");
        assert!(!decision.can_queue_local);
        assert!(!decision.can_queue_cloud);
        assert!(decision
            .review_reasons
            .iter()
            .any(|reason| reason.code == "original_source_refresh_required"));
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
        posting = verified_test_posting(posting, now_ms());
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
        assert!(error
            .to_string()
            .contains("only a verified runner receipt can finalize"));
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
            reserve_application_attempt(&pool, "acct-jobs", &application.id, "local").unwrap();
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
        let profile = default_profile("jobs@example.com");
        save_profile(&pool, "acct-jobs", &profile).unwrap();
        let posting = upsert_posting(
            &pool,
            "acct-jobs",
            &verified_test_posting(
                JobPosting {
                    id: String::new(),
                    canonical_key: String::new(),
                    source: "greenhouse".to_string(),
                    external_id: String::new(),
                    company: "Acme".to_string(),
                    title: "Engineer".to_string(),
                    location: "Remote".to_string(),
                    workplace: "remote".to_string(),
                    canonical_url: "https://boards.greenhouse.io/acme/jobs/state-machine"
                        .to_string(),
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

    fn communication_test_application(pool: &DbPool) -> JobApplication {
        let profile = default_profile("jobs@example.com");
        save_profile(pool, "acct-jobs", &profile).unwrap();
        let posting = upsert_posting(
            pool,
            "acct-jobs",
            &test_posting(
                "https://boards.greenhouse.io/acme/jobs/communication-action",
                now_ms(),
                now_ms(),
            ),
            &profile,
            &JobPreferences::default(),
        )
        .unwrap();
        let (application, _) =
            prepare_application(pool, "acct-jobs", &posting.id, "factual", "review_first").unwrap();
        application
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
                capabilities: vec!["reply".to_string(), "calendar".to_string()],
                created_at_ms: 0,
                updated_at_ms: 0,
            },
            "google-subject-communication-action",
        )
        .unwrap();
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
                metadata: json!({"thread_id": format!("thread-{external_id}")}),
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
            status: String::new(),
            provider_object_id: String::new(),
            lease_owner: None,
            lease_expires_at_ms: None,
            next_attempt_at_ms: 0,
            attempt_count: 0,
            approved_at_ms: None,
            dispatched_at_ms: None,
            created_at_ms: 0,
            updated_at_ms: 0,
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
    fn approved_communication_actions_wait_for_mailbox_reauthorization() {
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
                capabilities: vec!["reply".to_string(), "calendar".to_string()],
                created_at_ms: 0,
                updated_at_ms: 0,
            },
            "microsoft-subject-communication-action",
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
            "attendees": ["candidate@outlook.com", "recruiter@example.org"]
        });
        let (stored_calendar, inserted_calendar) =
            create_communication_action(&pool, "acct-jobs", &calendar).unwrap();
        assert!(inserted_calendar);
        assert_eq!(stored_calendar.provider, "outlook_calendar");
    }

    #[test]
    fn communication_actions_require_approval_and_fenced_provider_evidence() {
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
        assert!(finish_communication_action(
            &pool,
            "acct-jobs",
            &stored.id,
            "mail-worker",
            "wrong-token",
            lease.fence,
            "sent",
            Some("gmail-message-1"),
        )
        .is_err());
        assert!(finish_communication_action(
            &pool,
            "acct-jobs",
            &stored.id,
            "mail-worker",
            &lease.lease_token,
            lease.fence,
            "sent",
            None,
        )
        .is_err());

        let completed = finish_communication_action(
            &pool,
            "acct-jobs",
            &stored.id,
            "mail-worker",
            &lease.lease_token,
            lease.fence,
            "sent",
            Some("gmail-message-1"),
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
    fn expired_communication_dispatch_requires_reconciliation_before_retry() {
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
            "acct-jobs",
            &stored.id,
            "mail-worker",
            &lease.lease_token,
            lease.fence,
            "sent",
            Some("gmail-message-late"),
        )
        .is_ok());
    }

    #[test]
    fn cancelled_communication_actions_never_dispatch() {
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
            &run_id,
            &ticket_hash,
            &nonce,
            &descriptor,
            "test-server",
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

        assert_account_deletion_fence(claim_authorized_local_run_ticket(
            &pool,
            &run_id,
            &ticket_hash,
        ));
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
        assert_eq!(export.resume_versions.len(), 1);
        assert_eq!(export.workspace.application_evidence.len(), 1);
        assert!(export.workspace.browser_sessions[0].takeover_url.is_none());
        let serialized = serde_json::to_string(&export).unwrap();
        assert!(!serialized.contains("secret-capability"));
        assert!(!serialized.contains("export-ticket-secret"));
        assert!(!serialized.contains("must-not-export"));

        let refs = crate::db::account_data::artifact_object_refs(&pool, "acct-jobs").unwrap();
        assert_eq!(refs.len(), 4);
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
    fn intervention_answer_revises_cloud_packet_and_requires_review() {
        let pool = test_pool();
        let (mut application, run_id, browser_profile_id) =
            execution_lease_fixture(&pool, "answer-cloud");
        application.receipt.as_object_mut().unwrap().insert(
            "approved_execution".to_string(),
            json!({"schema_version": 2, "checksum": "approved-cloud-checksum"}),
        );
        application =
            replace_application_receipt(&pool, "acct-jobs", &application.id, application.receipt)
                .unwrap()
                .unwrap();
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
        update_attempt_reservation_status(&pool, "acct-jobs", &application.id, "running").unwrap();
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
            Some("approved-cloud-checksum")
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
        application.receipt.as_object_mut().unwrap().insert(
            "approved_execution".to_string(),
            json!({"schema_version": 2, "checksum": "approved-local-checksum"}),
        );
        application =
            replace_application_receipt(&pool, "acct-jobs", &application.id, application.receipt)
                .unwrap()
                .unwrap();
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
}
