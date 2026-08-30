//! Shared test fixture for the production source -> ATS -> integrity path.
//!
//! The fixture deliberately publishes every authority through its public
//! lifecycle. Tests using it therefore cannot become green by fabricating
//! mutable `posting_json` evidence.

use super::*;

fn original_source_verifier_runtime_test_fixture(
    pool: &DbPool,
    account_id: &str,
) -> managed_cloud_release_authority_tests::OriginalSourceVerifierRuntimeTestFixture {
    managed_cloud_release_authority_tests::install_original_source_verifier_runtime_test_fixture(
        pool, account_id,
    )
}

pub(crate) struct ProductionPositiveAuthorityFixture {
    pub(crate) posting: JobPosting,
    pub(crate) source: OriginalSourceVerificationProjection,
    pub(crate) ats: AtsCertificationPostingResolution,
    pub(crate) integrity: JobIntegrityCurrentAuthority,
    pub(crate) composed: ComposedJobIntegrityProjection,
    pub(crate) runtime_target: AtsCertificationRuntimeTarget,
    pub(crate) surface: AtsObservedSurface,
}

pub(crate) fn save_production_positive_verified_import(
    pool: &DbPool,
    account_id: &str,
    posting: &JobPosting,
    profile: &CareerProfile,
    preferences: &JobPreferences,
) -> (
    JobPosting,
    managed_cloud_release_authority_tests::OriginalSourceVerifierRuntimeTestFixture,
) {
    let managed = original_source_verifier_runtime_test_fixture(pool, account_id);
    let saved = save_production_positive_verified_import_with_runtime(
        pool,
        account_id,
        posting,
        profile,
        preferences,
    );
    (saved, managed)
}

pub(crate) fn save_production_positive_verified_import_with_runtime(
    pool: &DbPool,
    account_id: &str,
    posting: &JobPosting,
    profile: &CareerProfile,
    preferences: &JobPreferences,
) -> JobPosting {
    let provider = original_source_provider(&posting.source)
        .expect("production-positive fixture requires a supported provider");
    let (_, source_key) = canonical_public_discovery_url(provider, &posting.canonical_url)
        .expect("production-positive fixture requires a canonical public job URL");
    let source_input = DiscoverySourceInput {
        track_id: posting.track_id.clone(),
        provider: provider.to_string(),
        source_key: source_key.clone(),
        company: posting.company.clone(),
        run_interval_ms: DISCOVERY_MIN_INTERVAL_MS,
    };
    let saved = save_verified_import_posting_with_source(
        pool,
        account_id,
        posting,
        &source_input,
        profile,
        preferences,
    )
    .expect("save production-positive verified import");
    let source_id = get_job_discovery_authority(pool, account_id, &saved.id)
        .expect("load production-positive discovery membership")
        .into_iter()
        .filter(|authority| authority.provider == provider)
        .find_map(|authority| {
            get_discovery_source(pool, &authority.source_id)
                .expect("load production-positive discovery source")
                .filter(|source| {
                    source.account_id == account_id
                        && source.provider == provider
                        && source.source_key == source_key
                        && source.track_id == posting.track_id
                })
                .map(|source| source.id)
        })
        .expect("production-positive discovery membership exists");
    let lease = lease_due_discovery_source_for_test(
        pool,
        &format!(
            "phase614b-positive-discovery-{}",
            &saved.canonical_key[..16]
        ),
        &source_id,
    )
    .expect("lease production-positive discovery source")
    .expect("production-positive discovery source is due");
    assert_eq!(lease.source.account_id, account_id);
    assert_eq!(lease.source.provider, provider);
    assert_eq!(lease.source.source_key, source_key);
    let source_snapshot = list_postings(pool, account_id)
        .expect("load production-positive discovery snapshot")
        .into_iter()
        .filter(|candidate| {
            get_job_discovery_authority(pool, account_id, &candidate.id)
                .expect("load production-positive discovery membership")
                .iter()
                .any(|authority| authority.source_id == lease.source.id)
        })
        .map(|candidate| DiscoveredJobInput {
            external_id: candidate.external_id,
            canonical_url: candidate.canonical_url,
            title: candidate.title,
            company: candidate.company,
            source_catalog_id: String::new(),
            requires_original_revalidation: false,
            location: candidate.location,
            workplace: candidate.workplace,
            description: candidate.description,
            compensation: candidate.compensation,
            employment_type: candidate.employment_type,
            engagement_type: String::new(),
            posted_at_ms: candidate.posted_at_ms,
        })
        .collect::<Vec<_>>();
    assert!(source_snapshot
        .iter()
        .any(|candidate| candidate.external_id == saved.external_id));
    let completed = complete_discovery_run(
        pool,
        &lease.source.id,
        &lease.lease_token,
        &lease.replay_key,
        lease.scheduled_for_ms,
        &source_snapshot,
        true,
    )
    .expect("complete production-positive discovery snapshot");
    assert_eq!(completed.status, "completed");
    assert_eq!(
        completed.discovered_count,
        i64::try_from(source_snapshot.len()).expect("production-positive snapshot count fits i64")
    );
    let saved = list_postings(pool, account_id)
        .expect("reload production-positive discovered posting")
        .into_iter()
        .find(|candidate| candidate.id == saved.id)
        .expect("production-positive discovered posting remains addressable");
    let authorities = get_job_discovery_authority(pool, account_id, &saved.id)
        .expect("load production-positive discovery authority");
    assert!(authorities.iter().any(|authority| {
        authority.source_status == "active"
            && authority.source_health == "healthy"
            && authority.membership_status == "active"
    }));
    saved
}

pub(crate) fn install_production_positive_job_authorities(
    pool: &DbPool,
    account_id: &str,
    posting: &JobPosting,
    managed: &managed_cloud_release_authority_tests::OriginalSourceVerifierRuntimeTestFixture,
    canonical_employer_domain: &str,
    suffix: &str,
) -> ProductionPositiveAuthorityFixture {
    install_production_positive_job_authorities_for_runner(
        pool,
        account_id,
        posting,
        managed,
        canonical_employer_domain,
        suffix,
        "local",
    )
}

pub(crate) fn install_production_positive_job_authorities_for_runner(
    pool: &DbPool,
    account_id: &str,
    posting: &JobPosting,
    managed: &managed_cloud_release_authority_tests::OriginalSourceVerifierRuntimeTestFixture,
    canonical_employer_domain: &str,
    suffix: &str,
    runner_kind: &str,
) -> ProductionPositiveAuthorityFixture {
    assert!(matches!(runner_kind, "local" | "cloud"));
    let fixture_sha256 = ats_certification_sha256(
        format!("phase614b-production-positive:{account_id}:{suffix}:{runner_kind}").as_bytes(),
    );
    let surface = AtsObservedSurface {
        variant_key: "greenhouse_public".to_string(),
        layout_contract_version: 1,
        surface_sha256: ats_certification_sha256(
            format!("phase614b-production-positive-surface:{fixture_sha256}").as_bytes(),
        ),
    };
    let runtime_target = match runner_kind {
        "local" => AtsCertificationRuntimeTarget {
            runtime_kind: "local".to_string(),
            runtime_id: format!("local:phase614b-positive:{}", &fixture_sha256[..24]),
            runtime_sha256: ats_certification_sha256(
                format!("phase614b-production-positive-runtime:{fixture_sha256}").as_bytes(),
            ),
            platform: "macos".to_string(),
            architecture: "arm64".to_string(),
            automation_bundle_sha256: "4".repeat(64),
            browser_release_manifest_sha256: Some("1".repeat(64)),
            browser_artifact_sha256: Some("2".repeat(64)),
            browser_build_descriptor_sha256: Some("3".repeat(64)),
            runner_build_id: None,
            runner_image_sha256: None,
            playwright_version: "1.61.1".to_string(),
            chromium_revision: "1228".to_string(),
            chromium_executable_sha256: "7".repeat(64),
        },
        "cloud" => AtsCertificationRuntimeTarget {
            runtime_kind: "cloud".to_string(),
            runtime_id: format!("cloud:phase614b-positive:{}", &fixture_sha256[..24]),
            runtime_sha256: ats_certification_sha256(
                format!("phase614b-production-positive-runtime:{fixture_sha256}").as_bytes(),
            ),
            platform: "linux".to_string(),
            architecture: "x86_64".to_string(),
            automation_bundle_sha256: "4".repeat(64),
            browser_release_manifest_sha256: None,
            browser_artifact_sha256: None,
            browser_build_descriptor_sha256: None,
            runner_build_id: Some(format!("phase614b-positive-{}", &fixture_sha256[..24])),
            runner_image_sha256: Some("5".repeat(64)),
            playwright_version: "1.61.1".to_string(),
            chromium_revision: "1228".to_string(),
            chromium_executable_sha256: "7".repeat(64),
        },
        _ => unreachable!("runner kind was validated above"),
    };
    install_production_positive_job_authorities_with_runtime_surface(
        pool,
        account_id,
        posting,
        managed,
        canonical_employer_domain,
        suffix,
        runtime_target,
        surface,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn install_production_positive_job_authorities_with_runtime_surface(
    pool: &DbPool,
    account_id: &str,
    posting: &JobPosting,
    managed: &managed_cloud_release_authority_tests::OriginalSourceVerifierRuntimeTestFixture,
    canonical_employer_domain: &str,
    suffix: &str,
    runtime_target: AtsCertificationRuntimeTarget,
    surface: AtsObservedSurface,
) -> ProductionPositiveAuthorityFixture {
    original_source_verification_tests::complete_public_original_source_positive_fixture(
        pool, posting, managed, suffix,
    );
    let source = resolve_original_source_verification_projection(pool, account_id, posting)
        .expect("resolve production-positive original-source authority");
    assert!(source.feature_active);
    let source_binding = source
        .integrity_binding
        .as_ref()
        .expect("production-positive source exposes an integrity binding");
    let projected_posting = original_source_projected_ats_posting(posting, &source, source_binding);
    let now_ms = source.db_time_ms;
    let target_evidence =
        ats_certification_fresh_target_evidence_from_posting(&projected_posting, now_ms)
            .expect("derive exact ATS target from signed source authority");
    ats_certification_authority_tests::install_signed_ats_authority_fixture(
        pool,
        target_evidence,
        surface.clone(),
        runtime_target.clone(),
        now_ms,
    )
    .expect("install exact signed ATS authority");
    let ats =
        resolve_ats_certification_for_posting(pool, account_id, &projected_posting, None, now_ms)
            .expect("resolve exact signed ATS authority");
    let expected = job_integrity_expected_source_from_composition(source_binding, &ats)
        .unwrap_or_else(|denial| {
            panic!(
                "positive source and ATS authorities must compose: {}",
                denial.reason_code
            )
        });
    let integrity = job_integrity_authority_tests::install_signed_job_integrity_positive_fixture(
        pool,
        &expected,
        canonical_employer_domain,
    );
    let composed = resolve_composed_job_integrity_projection(pool, account_id, posting)
        .expect("resolve production-positive composed job authority");
    assert_eq!(
        composed.job_integrity.status,
        JobIntegrityResolutionStatus::Verified
    );
    assert_eq!(composed.job_integrity.authority.as_ref(), Some(&integrity));
    assert_eq!(
        composed.discovery_evidence.employer_verification_status,
        "verified"
    );
    assert_eq!(composed.discovery_evidence.scam_risk_status, "clear");

    ProductionPositiveAuthorityFixture {
        posting: projected_posting,
        source,
        ats,
        integrity,
        composed,
        runtime_target,
        surface,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pool() -> DbPool {
        let path = std::env::temp_dir().join(format!(
            "bluey-production-positive-authority-{}-{}.sqlite3",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let pool = crate::db::open_pool(&path).expect("open production-positive fixture pool");
        crate::db::run_migrations(&pool).expect("migrate production-positive fixture pool");
        pool.get()
            .unwrap()
            .execute(
                "INSERT INTO accounts (id, email, password_hash, trial_seconds_remaining)
                 VALUES ('acct-jobs', 'jobs@example.com', 'hash', 0)",
                [],
            )
            .unwrap();
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

    #[test]
    fn public_lifecycles_resolve_one_production_positive_composition() {
        let pool = pool();
        let profile = default_profile("jobs@example.com");
        let preferences = JobPreferences::default();
        let now_ms = now_ms();
        let posting = JobPosting {
            id: String::new(),
            canonical_key: String::new(),
            source: "greenhouse_import".to_string(),
            external_id: "phase614b-positive".to_string(),
            company: "Acme".to_string(),
            title: "Software Engineer".to_string(),
            location: "New York, NY".to_string(),
            workplace: "hybrid".to_string(),
            canonical_url: "https://boards.greenhouse.io/acme/jobs/phase614b-positive".to_string(),
            description: "Build reliable products with Rust and TypeScript.".to_string(),
            compensation: "$170k-$200k".to_string(),
            employment_type: "full_time".to_string(),
            track_id: "track-default".to_string(),
            match_score: 0,
            matched_reasons: Vec::new(),
            missing_requirements: Vec::new(),
            posted_at_ms: Some(now_ms - 60_000),
            last_verified_at_ms: Some(now_ms),
            availability_status: "active".to_string(),
            status: "matched".to_string(),
            created_at_ms: 0,
            updated_at_ms: 0,
            discovery_evidence: JobDiscoveryEvidence::default(),
            eligibility: None,
        };
        let (posting, managed) = save_production_positive_verified_import(
            &pool,
            "acct-jobs",
            &posting,
            &profile,
            &preferences,
        );
        let fixture = install_production_positive_job_authorities(
            &pool,
            "acct-jobs",
            &posting,
            &managed,
            "acme.com",
            "phase614b-positive",
        );
        assert_eq!(fixture.posting.source, "greenhouse");
        assert!(fixture.source.integrity_binding.is_some());
        assert_eq!(fixture.ats.status.status, "active");
        assert_eq!(fixture.integrity.canonical_employer_domain, "acme.com");
        assert_eq!(
            fixture
                .composed
                .discovery_evidence
                .canonical_employer_domain,
            Some("acme.com".to_string())
        );
        assert_eq!(fixture.runtime_target.runtime_kind, "local");
        assert_eq!(fixture.surface.variant_key, "greenhouse_public");
    }
}
