const DAY_MS: i64 = 24 * 60 * 60 * 1_000;
const LIVE_VERIFICATION_MAX_AGE_MS: i64 = DAY_MS;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DiscoveryExecutionGate {
    can_prepare: bool,
    can_queue: bool,
}

fn apply_posting_discovery_evidence(
    posting: &JobPosting,
    at_ms: i64,
    hard_failures: &mut Vec<EligibilityReason>,
    review_reasons: &mut Vec<EligibilityReason>,
    passed_checks: &mut Vec<String>,
) -> DiscoveryExecutionGate {
    let evidence = &posting.discovery_evidence;
    let canonical_status = evidence.canonical_status.trim().to_ascii_lowercase();
    let employer_status = evidence
        .employer_verification_status
        .trim()
        .to_ascii_lowercase();
    let scam_status = evidence.scam_risk_status.trim().to_ascii_lowercase();
    let original_status = evidence.original_source_status.trim().to_ascii_lowercase();

    if matches!(
        canonical_status.as_str(),
        "duplicate" | "repost" | "invalid" | "malformed"
    ) {
        push_reason(
            hard_failures,
            "canonical_job_rejected",
            "Bluey rejected this duplicate, reposted, or malformed job record.",
        );
    }
    if matches!(employer_status.as_str(), "mismatch" | "impersonated") {
        push_reason(
            hard_failures,
            "employer_identity_mismatch",
            "The application destination does not match the verified employer.",
        );
    }
    if scam_status == "blocked" {
        push_reason(
            hard_failures,
            "scam_risk_blocked",
            "Bluey blocked this posting after an employer or job-risk check.",
        );
    }
    if matches!(
        original_status.as_str(),
        "closed"
            | "verified_closed"
            | "mismatch"
            | "identity_mismatch"
            | "materially_changed"
            | "source_untrusted"
            | "expired"
            | "quarantined"
            | "redirected_to_unknown"
    ) || !evidence.original_source_mismatched_fields.is_empty()
    {
        push_reason(
            hard_failures,
            "original_source_rejected",
            "The original employer posting is closed or no longer matches this job record.",
        );
    }

    let expected_canonical_job_id = posting.canonical_key.trim();
    let evidence_canonical_job_id = evidence
        .canonical_job_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let canonical_verified = canonical_status == "canonical"
        && !expected_canonical_job_id.is_empty()
        && evidence_canonical_job_id == Some(expected_canonical_job_id);
    if canonical_verified {
        passed_checks.push("canonical_job_verified".to_string());
    } else if canonical_status == "canonical" {
        push_reason(
            hard_failures,
            "canonical_evidence_mismatch",
            "The discovery evidence belongs to a different canonical job record.",
        );
    } else if !matches!(
        canonical_status.as_str(),
        "duplicate" | "repost" | "invalid" | "malformed"
    ) {
        push_reason(
            review_reasons,
            "canonical_job_unverified",
            "Bluey must canonicalize and deduplicate this job before preparing an application.",
        );
    }

    let canonical_url_host = reqwest::Url::parse(&posting.canonical_url)
        .ok()
        .and_then(|url| url.host_str().map(normalize_discovery_domain));
    let evidence_application_domain = evidence
        .application_domain
        .as_deref()
        .map(normalize_discovery_domain)
        .filter(|domain| !domain.is_empty());
    let application_domain_matches = canonical_url_host.is_some()
        && evidence_application_domain.as_ref() == canonical_url_host.as_ref();
    let ats_tenant_claimed = matches!(employer_status.as_str(), "verified" | "ats_tenant_verified");
    if ats_tenant_claimed && !application_domain_matches {
        push_reason(
            hard_failures,
            "application_domain_mismatch",
            "The verified application destination does not match this job URL.",
        );
    }

    let ats_tenant_bound = ats_tenant_claimed && application_domain_matches;
    let employer_identity_bound = evidence
        .employer_id
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty())
        && evidence
            .canonical_employer_domain
            .as_deref()
            .is_some_and(|value| !normalize_discovery_domain(value).is_empty());
    let employer_verified =
        employer_status == "verified" && employer_identity_bound && ats_tenant_bound;
    if employer_verified {
        passed_checks.push("employer_identity_verified".to_string());
    } else if ats_tenant_bound {
        passed_checks.push("ats_tenant_bound".to_string());
        push_reason(
            review_reasons,
            "employer_identity_review_required",
            "Bluey must independently verify the employer before Auto-submit.",
        );
    } else if !matches!(employer_status.as_str(), "mismatch" | "impersonated") {
        push_reason(
            review_reasons,
            "employer_identity_unverified",
            "Bluey must verify the employer and application destination before preparing this job.",
        );
    }

    let scam_screened = matches!(scam_status.as_str(), "clear" | "source_screened")
        && evidence.scam_signals.is_empty();
    let scam_clear = scam_status == "clear" && scam_screened;
    if scam_clear {
        passed_checks.push("job_risk_clear".to_string());
    } else if scam_screened {
        passed_checks.push("source_risk_screened".to_string());
        push_reason(
            review_reasons,
            "job_risk_review_required",
            "Bluey must finish the employer-risk review before Auto-submit.",
        );
    } else if scam_status != "blocked" {
        push_reason(
            review_reasons,
            "job_risk_review_required",
            "Bluey must finish the job-risk review before preparing an application.",
        );
    }

    let original_evidence_present = original_status == "verified_open"
        && evidence.original_source_checked_at_ms.is_some()
        && evidence
            .original_source_evidence_hash
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty());
    let original_evidence_current = original_evidence_present
        && evidence
            .original_source_snapshot_expires_at_ms
            .is_some_and(|expires_at| expires_at >= at_ms);
    if original_evidence_current {
        passed_checks.push("original_source_current".to_string());
    } else if original_evidence_present {
        push_reason(
            review_reasons,
            "original_source_refresh_required",
            "Bluey must refresh the original employer posting before a runner starts.",
        );
    } else if !matches!(
        original_status.as_str(),
        "closed"
            | "verified_closed"
            | "mismatch"
            | "identity_mismatch"
            | "materially_changed"
            | "source_untrusted"
            | "expired"
            | "quarantined"
            | "redirected_to_unknown"
    ) {
        push_reason(
            review_reasons,
            "original_source_unverified",
            "Bluey must verify this job on the original employer site before preparing it.",
        );
    }

    let original_source_provenance =
        evidence.provenance == "original_source" || test_fixture_original_source(evidence);
    if evidence.provenance == "external_feed" {
        push_reason(
            review_reasons,
            "external_feed_requires_revalidation",
            "This feed entry is a lead until Bluey verifies it on the original employer site.",
        );
    }
    if evidence.requires_original_revalidation {
        push_reason(
            review_reasons,
            "original_source_revalidation_required",
            "Bluey must revalidate this job on its original source before preparing it.",
        );
    }

    let hard_blocked = !hard_failures.is_empty();
    let reviewable_original = original_source_provenance
        && canonical_verified
        && ats_tenant_bound
        && scam_screened
        && original_evidence_current
        && !evidence.requires_original_revalidation;
    let unattended_original = reviewable_original && employer_verified && scam_clear;
    DiscoveryExecutionGate {
        can_prepare: !hard_blocked && reviewable_original,
        can_queue: !hard_blocked && unattended_original && original_evidence_current,
    }
}

fn normalize_discovery_domain(value: &str) -> String {
    let normalized = value.trim().trim_end_matches('.').to_ascii_lowercase();
    normalized
        .strip_prefix("www.")
        .unwrap_or(&normalized)
        .to_string()
}

pub fn get_job_discovery_authority(
    pool: &DbPool,
    account_id: &str,
    job_id: &str,
) -> Result<Vec<JobDiscoveryAuthority>> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get()?;
            let mut stmt = conn.prepare(
                "SELECT s.id, s.provider, s.status, s.health,
                        m.availability_status, m.last_seen_at_ms, m.last_seen_run_id
                   FROM jobs_discovery_memberships m
                   JOIN jobs_discovery_sources s ON s.id = m.source_id
                  WHERE m.account_id = ?1 AND m.job_id = ?2
                  ORDER BY m.last_seen_at_ms DESC",
            )?;
            let rows = stmt.query_map(params![account_id, job_id], |row| {
                Ok(JobDiscoveryAuthority {
                    source_id: row.get(0)?,
                    provider: row.get(1)?,
                    source_status: row.get(2)?,
                    source_health: row.get(3)?,
                    membership_status: row.get(4)?,
                    last_seen_at_ms: row.get(5)?,
                    last_seen_run_id: row.get(6)?,
                })
            })?;
            rows.collect::<std::result::Result<Vec<_>, _>>()
                .context("get Jobs discovery authority")
        }
        DbPool::Postgres(_) => Ok(pool
            .get_pg()?
            .query(
                "SELECT s.id, s.provider, s.status, s.health,
                        m.availability_status, m.last_seen_at_ms, m.last_seen_run_id
                   FROM jobs_discovery_memberships m
                   JOIN jobs_discovery_sources s ON s.id = m.source_id
                  WHERE m.account_id = $1 AND m.job_id = $2
                  ORDER BY m.last_seen_at_ms DESC",
                &[&account_id, &job_id],
            )?
            .into_iter()
            .map(|row| JobDiscoveryAuthority {
                source_id: row.get(0),
                provider: row.get(1),
                source_status: row.get(2),
                source_health: row.get(3),
                membership_status: row.get(4),
                last_seen_at_ms: row.get(5),
                last_seen_run_id: row.get(6),
            })
            .collect()),
    })
}

fn apply_discovery_authority(
    pool: &DbPool,
    account_id: &str,
    posting: &JobPosting,
    decision: &mut JobEligibilityDecision,
) -> Result<()> {
    let authorities = get_job_discovery_authority(pool, account_id, &posting.id)?;
    apply_discovery_authorities(&authorities, decision);
    Ok(())
}

fn apply_discovery_authorities(
    authorities: &[JobDiscoveryAuthority],
    decision: &mut JobEligibilityDecision,
) {
    if authorities.is_empty() {
        return;
    }
    let now = now_ms();
    let runnable = authorities.iter().any(|authority| {
        authority.source_status == "active"
            && authority.source_health == "healthy"
            && authority.membership_status == "active"
            && authority.last_seen_at_ms >= now - LIVE_VERIFICATION_MAX_AGE_MS
    });
    if runnable {
        decision
            .passed_checks
            .push("discovery_source_healthy".to_string());
        return;
    }

    decision.can_auto_submit = false;
    decision.can_queue_local = false;
    decision.can_queue_cloud = false;
    let message = if authorities
        .iter()
        .any(|authority| authority.source_health == "paused")
    {
        "This job source is paused after repeated failures. Bluey will not run applications until it recovers."
    } else if authorities
        .iter()
        .any(|authority| authority.source_health == "degraded")
    {
        "This job source is degraded. Bluey will keep the packet in Review until a healthy refresh succeeds."
    } else {
        "Bluey must refresh this job source before an application can enter a runner."
    };
    push_reason(
        &mut decision.review_reasons,
        "discovery_source_unhealthy",
        message,
    );
}

struct ApplicationFinalizationAuthorities<'a> {
    discovery: &'a [JobDiscoveryAuthority],
    ats: Option<&'a AtsCertificationPostingResolution>,
}

fn enforce_application_finalization_eligibility(
    application: &JobApplication,
    posting: &JobPosting,
    profile: &CareerProfile,
    preferences: &JobPreferences,
    track: Option<&CareerTrack>,
    reservations: &[AttemptReservation],
    authorities: ApplicationFinalizationAuthorities<'_>,
) -> Result<()> {
    let mut decision = build_job_eligibility(
        posting,
        profile,
        preferences,
        reservations,
        false,
        Some(application.id.as_str()),
        track,
    );
    apply_discovery_authorities(authorities.discovery, &mut decision);
    if let Some(resolution) = authorities.ats {
        apply_ats_certification_resolution(posting, &mut decision, resolution);
    }
    if !decision.can_prepare {
        anyhow::bail!("job eligibility changed while the application packet was generated")
    }
    if application.state == "queued" && !decision.can_auto_submit {
        anyhow::bail!("auto-submit eligibility changed while the application packet was generated")
    }
    Ok(())
}

pub fn posting_age_days(posting: &JobPosting, at_ms: i64) -> i64 {
    let published_or_first_seen = posting.posted_at_ms.unwrap_or(posting.created_at_ms);
    at_ms.saturating_sub(published_or_first_seen).max(0) / DAY_MS
}

pub fn evaluate_job_eligibility(
    pool: &DbPool,
    account_id: &str,
    posting: &JobPosting,
    require_live_verification: bool,
    existing_application_id: Option<&str>,
) -> Result<JobEligibilityDecision> {
    let composed = resolve_composed_job_integrity_projection(pool, account_id, posting)?;
    let posting = posting_with_original_source_projection(posting, &composed.original_source);
    evaluate_job_eligibility_with_composed_projection(
        pool,
        account_id,
        &posting,
        &composed,
        require_live_verification,
        existing_application_id,
    )
}

fn posting_with_original_source_projection(
    posting: &JobPosting,
    projection: &OriginalSourceVerificationProjection,
) -> JobPosting {
    let mut projected = posting.clone();
    // Posting JSON is mutable compatibility material. Never allow its legacy
    // execution-grade labels to stand in for relational employer/risk
    // authority, even while the v2 verifier release is inactive.
    sanitize_mutable_execution_labels(&mut projected.discovery_evidence);
    if projection.feature_active {
        merge_original_source_projection(
            &mut projected.discovery_evidence,
            projection.evidence.as_ref(),
        );
    }
    projected
}

fn sanitize_mutable_execution_labels(evidence: &mut JobDiscoveryEvidence) {
    if test_fixture_original_source(evidence) {
        return;
    }
    if evidence
        .employer_verification_status
        .trim()
        .eq_ignore_ascii_case("verified")
    {
        evidence.employer_verification_status = "unknown".to_string();
        evidence.employer_id = None;
        evidence.canonical_employer_domain = None;
    }
    if evidence
        .scam_risk_status
        .trim()
        .eq_ignore_ascii_case("clear")
    {
        evidence.scam_risk_status = "unknown".to_string();
    }
}

fn test_fixture_original_source(evidence: &JobDiscoveryEvidence) -> bool {
    #[cfg(test)]
    {
        evidence.provenance == "test_fixture_original_source"
    }
    #[cfg(not(test))]
    {
        let _ = evidence;
        false
    }
}

fn merge_original_source_projection(
    target: &mut JobDiscoveryEvidence,
    source: Option<&JobDiscoveryEvidence>,
) {
    let employer_status = target
        .employer_verification_status
        .trim()
        .to_ascii_lowercase();
    let employer_rejected = matches!(employer_status.as_str(), "mismatch" | "impersonated");
    let scam_blocked = target
        .scam_risk_status
        .trim()
        .eq_ignore_ascii_case("blocked");
    let canonical_status = target.canonical_status.trim().to_ascii_lowercase();
    let canonical_rejected = matches!(
        canonical_status.as_str(),
        "duplicate" | "repost" | "invalid" | "malformed"
    );
    let Some(source) = source else {
        let source_rejected = original_source_status_is_hard_denial(&target.original_source_status)
            || !target.original_source_mismatched_fields.is_empty();
        if !source_rejected {
            target.original_source_status = "unknown".to_string();
            target.original_source_checked_at_ms = None;
            target.original_source_snapshot_expires_at_ms = None;
            target.original_source_evidence_hash = None;
            target.original_source_mismatched_fields.clear();
        }
        target.requires_original_revalidation = true;
        return;
    };

    target.provenance = source.provenance.clone();
    if !canonical_rejected {
        target.canonical_status = source.canonical_status.clone();
        target.canonical_job_id = source.canonical_job_id.clone();
    }
    if !employer_rejected {
        target.employer_verification_status = source.employer_verification_status.clone();
        target.employer_id = source.employer_id.clone();
        target.canonical_employer_domain = source.canonical_employer_domain.clone();
    }
    target.application_domain = source.application_domain.clone();
    if !scam_blocked {
        target.scam_risk_status = source.scam_risk_status.clone();
    }
    target.original_source_status = source.original_source_status.clone();
    target.original_source_checked_at_ms = source.original_source_checked_at_ms;
    target.original_source_snapshot_expires_at_ms = source.original_source_snapshot_expires_at_ms;
    target.original_source_evidence_hash = source.original_source_evidence_hash.clone();
    target.original_source_mismatched_fields = source.original_source_mismatched_fields.clone();
    target.requires_original_revalidation = source.requires_original_revalidation;
}

#[cfg(test)]
mod original_source_projection_tests {
    use super::*;

    #[test]
    fn mutable_execution_labels_are_never_relational_authority() {
        let mut evidence = JobDiscoveryEvidence {
            employer_verification_status: " VERIFIED ".to_string(),
            employer_id: Some("mutable-employer".to_string()),
            canonical_employer_domain: Some("mutable.example".to_string()),
            scam_risk_status: "CLEAR".to_string(),
            ..JobDiscoveryEvidence::default()
        };

        sanitize_mutable_execution_labels(&mut evidence);

        assert_eq!(evidence.employer_verification_status, "unknown");
        assert_eq!(evidence.employer_id, None);
        assert_eq!(evidence.canonical_employer_domain, None);
        assert_eq!(evidence.scam_risk_status, "unknown");
    }

    fn source_bound_posting() -> JobPosting {
        let checked_at_ms = 1_800_000_000_000;
        JobPosting {
            id: "job-source-bound".to_string(),
            canonical_key: "canonical-job-source-bound".to_string(),
            source: "greenhouse_import".to_string(),
            external_id: "123".to_string(),
            company: "Acme".to_string(),
            title: "Platform Engineer".to_string(),
            location: "Remote".to_string(),
            workplace: "remote".to_string(),
            canonical_url: "https://boards.greenhouse.io/acme/jobs/123".to_string(),
            description: "Build reliable systems.".to_string(),
            compensation: String::new(),
            employment_type: "full_time".to_string(),
            track_id: "track-default".to_string(),
            match_score: 100,
            matched_reasons: Vec::new(),
            missing_requirements: Vec::new(),
            posted_at_ms: Some(checked_at_ms),
            last_verified_at_ms: Some(checked_at_ms),
            availability_status: "active".to_string(),
            status: "matched".to_string(),
            created_at_ms: checked_at_ms,
            updated_at_ms: checked_at_ms,
            discovery_evidence: JobDiscoveryEvidence {
                provenance: "original_source".to_string(),
                canonical_status: "canonical".to_string(),
                canonical_job_id: Some("canonical-job-source-bound".to_string()),
                employer_verification_status: "ats_tenant_verified".to_string(),
                employer_id: None,
                canonical_employer_domain: None,
                application_domain: Some("boards.greenhouse.io".to_string()),
                scam_risk_status: "source_screened".to_string(),
                scam_signals: Vec::new(),
                original_source_status: "verified_open".to_string(),
                original_source_checked_at_ms: Some(checked_at_ms),
                original_source_snapshot_expires_at_ms: Some(checked_at_ms + DAY_MS),
                original_source_evidence_hash: Some("a".repeat(64)),
                original_source_mismatched_fields: Vec::new(),
                requires_original_revalidation: false,
            },
            eligibility: None,
        }
    }

    #[test]
    fn ats_tenant_binding_is_review_first_without_employer_identity() {
        let posting = source_bound_posting();
        let mut hard_failures = Vec::new();
        let mut review_reasons = Vec::new();
        let mut passed_checks = Vec::new();

        let gate = apply_posting_discovery_evidence(
            &posting,
            1_800_000_000_000,
            &mut hard_failures,
            &mut review_reasons,
            &mut passed_checks,
        );

        assert!(gate.can_prepare);
        assert!(!gate.can_queue);
        assert!(hard_failures.is_empty());
        assert!(passed_checks.contains(&"ats_tenant_bound".to_string()));
        assert!(review_reasons
            .iter()
            .any(|reason| reason.code == "employer_identity_review_required"));
        assert!(review_reasons
            .iter()
            .any(|reason| reason.code == "job_risk_review_required"));
    }

    #[test]
    fn composed_employer_identity_does_not_require_the_ats_domain() {
        let mut posting = source_bound_posting();
        posting.discovery_evidence.employer_verification_status = "verified".to_string();
        posting.discovery_evidence.employer_id = Some("employer-acme".to_string());
        posting.discovery_evidence.canonical_employer_domain = Some("acme.example".to_string());
        posting.discovery_evidence.scam_risk_status = "clear".to_string();
        let mut hard_failures = Vec::new();
        let mut review_reasons = Vec::new();
        let mut passed_checks = Vec::new();

        let gate = apply_posting_discovery_evidence(
            &posting,
            1_800_000_000_000,
            &mut hard_failures,
            &mut review_reasons,
            &mut passed_checks,
        );

        assert!(gate.can_prepare);
        assert!(gate.can_queue);
        assert!(hard_failures.is_empty());
        assert!(review_reasons.is_empty());
        assert!(passed_checks.contains(&"employer_identity_verified".to_string()));
        assert!(passed_checks.contains(&"job_risk_clear".to_string()));
        assert_ne!(
            posting.discovery_evidence.application_domain,
            posting.discovery_evidence.canonical_employer_domain
        );
    }

    #[test]
    fn source_projection_never_erases_independent_hard_denials() {
        let mut target = JobDiscoveryEvidence {
            canonical_status: "invalid".to_string(),
            employer_verification_status: "impersonated".to_string(),
            scam_risk_status: "blocked".to_string(),
            scam_signals: vec![DiscoveryScamSignal {
                code: "lookalike_domain".to_string(),
                source: "risk_engine".to_string(),
            }],
            ..JobDiscoveryEvidence::default()
        };
        let source = JobDiscoveryEvidence::provider_verified_original_source(
            "canonical-job".to_string(),
            "employer".to_string(),
            Some("jobs.example.com".to_string()),
            1_800_000_000_000,
            "a".repeat(64),
        );

        merge_original_source_projection(&mut target, Some(&source));

        assert_eq!(target.canonical_status, "invalid");
        assert_eq!(target.employer_verification_status, "impersonated");
        assert_eq!(target.scam_risk_status, "blocked");
        assert_eq!(target.scam_signals.len(), 1);
        assert_eq!(
            target.application_domain.as_deref(),
            Some("jobs.example.com")
        );
        assert_eq!(target.original_source_status, "verified_open");
    }

    #[test]
    fn absent_relational_projection_never_erases_a_persisted_source_denial() {
        let mut target = JobDiscoveryEvidence {
            provenance: "original_source".to_string(),
            original_source_status: "identity_mismatch".to_string(),
            original_source_checked_at_ms: Some(100),
            original_source_snapshot_expires_at_ms: Some(200),
            original_source_evidence_hash: Some("a".repeat(64)),
            original_source_mismatched_fields: vec!["provider_record_id".to_string()],
            requires_original_revalidation: false,
            ..JobDiscoveryEvidence::default()
        };

        merge_original_source_projection(&mut target, None);

        assert_eq!(target.original_source_status, "identity_mismatch");
        assert_eq!(target.original_source_checked_at_ms, Some(100));
        assert_eq!(target.original_source_snapshot_expires_at_ms, Some(200));
        assert_eq!(target.original_source_evidence_hash, Some("a".repeat(64)));
        assert_eq!(
            target.original_source_mismatched_fields,
            vec!["provider_record_id"]
        );
        assert!(target.requires_original_revalidation);
    }

    #[test]
    fn relational_negative_source_projection_reaches_the_hard_eligibility_gate() {
        let at_ms = 1_800_000_000_000;
        let mut posting = source_bound_posting();
        let mut evidence = posting.discovery_evidence.clone();
        evidence.original_source_status = "closed".to_string();
        evidence.requires_original_revalidation = true;
        let projection = OriginalSourceVerificationProjection {
            feature_active: true,
            evidence: Some(evidence),
            expected_head: None,
            integrity_binding: None,
            db_time_ms: at_ms,
        };
        posting = posting_with_original_source_projection(&posting, &projection);
        let mut hard_failures = Vec::new();
        let mut review_reasons = Vec::new();
        let mut passed_checks = Vec::new();

        let gate = apply_posting_discovery_evidence(
            &posting,
            at_ms,
            &mut hard_failures,
            &mut review_reasons,
            &mut passed_checks,
        );

        assert!(!gate.can_prepare);
        assert!(!gate.can_queue);
        assert!(hard_failures
            .iter()
            .any(|reason| reason.code == "original_source_rejected"));
    }
}

fn application_original_source_expected_head(
    application: &JobApplication,
) -> Result<Option<OriginalSourceVerificationExpectedHead>> {
    let Some(value) = application.receipt.get("original_source_verification") else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }
    serde_json::from_value(value.clone())
        .map(Some)
        .context("decode frozen original-source verification head")
}

fn original_source_projection_matches_application(
    application: &JobApplication,
    projection: &OriginalSourceVerificationProjection,
) -> Result<bool> {
    let expected = application_original_source_expected_head(application)?;
    Ok(if projection.feature_active {
        projection.evidence.is_some()
            && projection.expected_head.as_ref().is_some_and(|current| {
                expected
                    .as_ref()
                    .is_some_and(|expected| expected == current)
            })
    } else {
        expected.is_none()
    })
}

fn evaluate_job_eligibility_with_composed_projection(
    pool: &DbPool,
    account_id: &str,
    posting: &JobPosting,
    composed: &ComposedJobIntegrityProjection,
    require_live_verification: bool,
    existing_application_id: Option<&str>,
) -> Result<JobEligibilityDecision> {
    let profile = get_profile(pool, account_id, "")?;
    let preferences = get_preferences(pool, account_id)?;
    let tracks = list_tracks(pool, account_id)?;
    let track = tracks.iter().find(|track| track.id == posting.track_id);
    let reservations = list_attempt_reservations(pool, account_id)?;
    let mut authoritative_posting = posting.clone();
    authoritative_posting.discovery_evidence = composed.discovery_evidence.clone();
    let mut decision = build_job_eligibility(
        &authoritative_posting,
        &profile,
        &preferences,
        &reservations,
        require_live_verification,
        existing_application_id,
        track,
    );
    apply_discovery_authority(pool, account_id, &authoritative_posting, &mut decision)?;
    if let Some(resolution) = composed.ats_certification.as_ref() {
        apply_ats_certification_resolution(&authoritative_posting, &mut decision, resolution);
    }
    Ok(decision)
}

fn apply_ats_certification_resolution(
    posting: &JobPosting,
    decision: &mut JobEligibilityDecision,
    resolution: &AtsCertificationPostingResolution,
) {
    apply_ats_certification_status(
        posting,
        decision,
        &resolution.status,
        resolution.active_binding.is_some(),
    );
}

fn apply_ats_certification_status(
    posting: &JobPosting,
    decision: &mut JobEligibilityDecision,
    status: &AtsCertificationTargetStatusProjection,
    has_active_binding: bool,
) {
    if status.status != "active" || !has_active_binding {
        let effective_status = if status.status == "active" {
            "drifted"
        } else {
            &status.status
        };
        decision.ats_certification.status = effective_status.to_string();
        decision.ats_certification.certified_runner_kinds.clear();
        decision.ats_certification.canary_available = false;
        decision.ats_certification.last_verified_at_ms = status.last_verified_at_ms;
        decision.ats_certification.expires_at_ms = status.expires_at_ms;
        let (reason, next_action) = inactive_ats_certification_copy(effective_status);
        decision.ats_certification.reason = reason.to_string();
        decision.ats_certification.next_action = next_action.to_string();
        return;
    }

    let mut runner_kinds = status
        .runner_kinds
        .iter()
        .filter(|runner| matches!(runner.as_str(), "local" | "cloud"))
        .cloned()
        .collect::<Vec<_>>();
    runner_kinds.sort();
    runner_kinds.dedup();
    if runner_kinds.is_empty()
        || status.adapter_version.is_none()
        || status.last_verified_at_ms.is_none()
        || status.expires_at_ms.is_none()
    {
        decision.can_auto_submit = false;
        decision.can_queue_local = false;
        decision.can_queue_cloud = false;
        decision.ats_certification.status = "drifted".to_string();
        decision.ats_certification.certified_runner_kinds.clear();
        decision.ats_certification.canary_available = false;
        let (reason, next_action) = inactive_ats_certification_copy("drifted");
        decision.ats_certification.reason = reason.to_string();
        decision.ats_certification.next_action = next_action.to_string();
        return;
    }

    let local_was_queueable = decision.can_queue_local;
    let cloud_was_queueable = decision.can_queue_cloud;
    decision.capability = "certified".to_string();
    decision
        .review_reasons
        .retain(|reason| reason.code != "ats_review_required");
    if !decision
        .passed_checks
        .iter()
        .any(|check| check == "ats_certified")
    {
        decision.passed_checks.push("ats_certified".to_string());
    }
    decision.can_queue_local =
        local_was_queueable && runner_kinds.iter().any(|runner| runner == "local");
    decision.can_queue_cloud =
        cloud_was_queueable && runner_kinds.iter().any(|runner| runner == "cloud");
    decision.can_auto_submit = (decision.can_queue_local || decision.can_queue_cloud)
        && decision.hard_failures.is_empty()
        && decision.review_reasons.is_empty()
        && posting.missing_requirements.is_empty();
    decision.ats_certification = AtsCertificationSummary {
        provider_label: ats_provider_label(&status.provider).to_string(),
        adapter_version: status.adapter_version.clone(),
        certified_runner_kinds: runner_kinds,
        status: "active".to_string(),
        last_verified_at_ms: status.last_verified_at_ms,
        expires_at_ms: status.expires_at_ms,
        reason: "This exact provider target has current server-owned certification.".to_string(),
        next_action: "Bluey will recheck the exact certification immediately before Submit."
            .to_string(),
        canary_available: status.canary_available,
    };
}

fn ats_provider_label(provider: &str) -> &'static str {
    match provider {
        "greenhouse" => "Greenhouse",
        "lever" => "Lever",
        _ => "Application site",
    }
}

fn inactive_ats_certification_copy(status: &str) -> (&'static str, &'static str) {
    match status {
        "expired" => (
            "The server-owned certification window for this exact provider target has expired.",
            "Review the packet while a new certification window is approved.",
        ),
        "suspended" => (
            "Automated submission is paused by a server safety circuit.",
            "Use Review first until the safety circuit is reviewed and closed.",
        ),
        "revoked" => (
            "Automated submission authority for this exact provider target was revoked.",
            "Use Review first until newer signed authority is approved.",
        ),
        "drifted" => (
            "The current provider layout or runner no longer matches certified evidence.",
            "Use Review first while Bluey verifies the changed provider surface.",
        ),
        _ => (
            "This exact provider target has no active ATS certification.",
            "Review the packet and approve the provider-specific final step.",
        ),
    }
}

fn build_job_eligibility(
    posting: &JobPosting,
    profile: &CareerProfile,
    preferences: &JobPreferences,
    reservations: &[AttemptReservation],
    require_live_verification: bool,
    existing_application_id: Option<&str>,
    track: Option<&CareerTrack>,
) -> JobEligibilityDecision {
    let now = now_ms();
    let capability = submission_capability(posting);
    let mut hard_failures = Vec::new();
    let mut review_reasons = Vec::new();
    let mut passed_checks = Vec::new();
    let mut track_policy_ready = false;
    let mut role_match_proven = false;
    let mut location_match_proven = false;
    let mut employment_type_match_proven = preferences.employment_types.is_empty();
    let mut engagement_type_match_proven = preferences.engagement_types.is_empty();
    let mut track_employment_type_match_proven = true;
    let mut track_engagement_type_match_proven = true;
    let experience_evidence = role_experience_evidence(profile, track, posting);
    let experience_requirement = experience_requirement(posting);

    match track {
        None => push_reason(
            &mut hard_failures,
            "career_track_missing",
            "Choose an active Career Track before Bluey prepares this application.",
        ),
        Some(track) if !track.active => push_reason(
            &mut hard_failures,
            "career_track_inactive",
            "This Career Track is paused.",
        ),
        Some(track) if track.application_identity_id.is_none() => push_reason(
            &mut hard_failures,
            "application_identity_required",
            "Choose a verified application identity for this Career Track.",
        ),
        Some(track) => {
            passed_checks.push("career_track_active".to_string());
            passed_checks.push("application_identity_bound".to_string());
            let authority = &track.policy.authority;
            let preferences_sha256 = job_preferences_policy_sha256(preferences).ok();
            track_policy_ready = authority.review_state == "approved"
                && authority.taxonomy_version == crate::jobs_taxonomy::taxonomy_version()
                && authority.taxonomy_sha256 == crate::jobs_taxonomy::taxonomy_sha256()
                && authority.source_resume_asset_id == profile.source_resume_asset_id
                && authority.source_resume_sha256 == profile.source_resume_sha256
                && preferences_sha256.as_deref() == Some(authority.job_preferences_sha256.as_str());
            if track_policy_ready {
                passed_checks.push("career_track_policy_current".to_string());
            } else {
                push_reason(
                    &mut review_reasons,
                    "career_track_policy_review_required",
                    "Review this Career Track after any role, location, identity, resume, or Jobs setting changes before a runner can use it.",
                );
            }
            if posting.track_id != track.id {
                push_reason(
                    &mut hard_failures,
                    "career_track_binding_changed",
                    "This job is not bound to the selected Career Track.",
                );
            } else {
                match crate::jobs_taxonomy::classify_posting_role(&posting.title) {
                    crate::jobs_taxonomy::PostingRoleClassification::Known {
                        family_id, ..
                    } if experience_evidence.role_family != ROLE_FAMILY_GENERIC
                        && family_id != experience_evidence.role_family =>
                    {
                        push_reason(
                            &mut hard_failures,
                            "role_family_mismatch",
                            "This role belongs to a different Career Track.",
                        );
                    }
                    crate::jobs_taxonomy::PostingRoleClassification::Known { .. } => {
                        role_match_proven = true;
                        passed_checks.push("role_family_aligned".to_string());
                    }
                    crate::jobs_taxonomy::PostingRoleClassification::Ambiguous { .. }
                    | crate::jobs_taxonomy::PostingRoleClassification::Unknown { .. } => {
                        push_reason(
                            &mut review_reasons,
                            "posting_role_review_required",
                            "Bluey could not prove this posting belongs to the selected canonical role family.",
                        );
                    }
                }
            }
        }
    }

    match posting.availability_status.as_str() {
        "active" => passed_checks.push("job_active".to_string()),
        "unknown" => push_reason(
            &mut review_reasons,
            "availability_unverified",
            "Bluey has not verified that this pasted job is still accepting applications.",
        ),
        _ => push_reason(
            &mut hard_failures,
            "job_closed",
            "This job is no longer accepting applications.",
        ),
    }

    let age_days = posting_age_days(posting, now);
    if age_days > preferences.max_posting_age_days {
        push_reason(
            &mut hard_failures,
            "job_too_old",
            &format!(
                "Posted {age_days} days ago; your limit is {} days.",
                preferences.max_posting_age_days
            ),
        );
    } else {
        passed_checks.push("job_fresh".to_string());
    }

    let live_verification_missing = posting
        .last_verified_at_ms
        .is_none_or(|verified_at| verified_at < now - LIVE_VERIFICATION_MAX_AGE_MS);
    if require_live_verification && live_verification_missing {
        push_reason(
            &mut review_reasons,
            "live_verification_required",
            "Bluey must confirm this job is still open before a runner starts.",
        );
    } else if !live_verification_missing {
        passed_checks.push("job_recently_verified".to_string());
    }

    let company = posting.company.to_lowercase();
    if preferences.excluded_companies.iter().any(|excluded| {
        let excluded = excluded.trim().to_lowercase();
        !excluded.is_empty() && (company.contains(&excluded) || excluded.contains(&company))
    }) {
        push_reason(
            &mut hard_failures,
            "company_excluded",
            "This company is excluded by your Jobs settings.",
        );
    } else {
        passed_checks.push("company_allowed".to_string());
    }

    let title = posting.title.to_lowercase();
    if preferences.excluded_titles.iter().any(|excluded| {
        let excluded = excluded.trim().to_lowercase();
        !excluded.is_empty() && title.contains(&excluded)
    }) {
        push_reason(
            &mut hard_failures,
            "title_excluded",
            "This title is excluded by your Jobs settings.",
        );
    } else {
        passed_checks.push("title_allowed".to_string());
    }

    if let Some(minimum) = preferences.minimum_compensation {
        match compensation_range(&posting.compensation) {
            Some((_, maximum)) if maximum < minimum => push_reason(
                &mut hard_failures,
                "salary_below_floor",
                "The listed compensation is below your minimum compensation setting.",
            ),
            Some(_) => passed_checks.push("salary_floor_passed".to_string()),
            None => push_reason(
                &mut review_reasons,
                "salary_not_listed",
                "Compensation is not clear enough to verify your salary floor.",
            ),
        }
    }

    match canonical_location_decision(posting, profile, preferences, track) {
        CanonicalLocationDecision::Allowed => {
            location_match_proven = true;
            passed_checks.push("location_allowed".to_string());
        }
        CanonicalLocationDecision::Denied(reason) if preferences.location_policy == "ask" => {
            push_reason(&mut review_reasons, "location_needs_confirmation", &reason);
        }
        CanonicalLocationDecision::Denied(reason) => {
            push_reason(&mut hard_failures, "location_mismatch", &reason);
        }
        CanonicalLocationDecision::ReviewRequired(reason) => {
            push_reason(
                &mut review_reasons,
                "location_taxonomy_review_required",
                &reason,
            );
        }
    }

    if !preferences.employment_types.is_empty() {
        match candidate_employment_type(posting) {
            CandidateCategoryClassification::Known(kind)
                if preferences
                    .employment_types
                    .iter()
                    .any(|allowed| normalize_candidate_employment_type(allowed) == Some(kind)) =>
            {
                employment_type_match_proven = true;
                passed_checks.push("employment_type_allowed".to_string());
            }
            CandidateCategoryClassification::Known(_) => push_reason(
                &mut hard_failures,
                "employment_type_mismatch",
                "This job uses an employment type you did not select.",
            ),
            CandidateCategoryClassification::Unknown(_) => push_reason(
                &mut review_reasons,
                "employment_type_unverified",
                "Bluey must confirm this job's employment type before Auto-submit.",
            ),
        }
    }

    if !preferences.engagement_types.is_empty() {
        match candidate_engagement_type(posting) {
            CandidateCategoryClassification::Known(kind)
                if preferences.engagement_types.iter().any(|allowed| {
                    normalize_candidate_engagement_type(allowed) == Some(kind)
                }) =>
            {
                engagement_type_match_proven = true;
                passed_checks.push("engagement_type_allowed".to_string());
            }
            CandidateCategoryClassification::Known(_) => push_reason(
                &mut hard_failures,
                "engagement_type_mismatch",
                "This job uses an engagement type you did not select.",
            ),
            CandidateCategoryClassification::Unknown(_) => push_reason(
                &mut review_reasons,
                "engagement_type_unverified",
                "Bluey must confirm whether this role is W2, C2C, 1099, or direct hire before Auto-submit.",
            ),
        }
    }

    match track_employment_type_decision(posting, track) {
        Some(CandidatePolicyFilterDecision::Allowed) => {
            passed_checks.push("track_employment_type_allowed".to_string());
        }
        Some(CandidatePolicyFilterDecision::Mismatch(reason)) => {
            track_employment_type_match_proven = false;
            push_reason(
                &mut hard_failures,
                "track_employment_type_mismatch",
                &reason,
            );
        }
        Some(CandidatePolicyFilterDecision::ReviewRequired(reason)) => {
            track_employment_type_match_proven = false;
            push_reason(
                &mut review_reasons,
                "track_employment_type_unverified",
                &reason,
            );
        }
        None => {}
    }
    match track_engagement_type_decision(posting, track) {
        Some(CandidatePolicyFilterDecision::Allowed) => {
            passed_checks.push("track_engagement_type_allowed".to_string());
        }
        Some(CandidatePolicyFilterDecision::Mismatch(reason)) => {
            track_engagement_type_match_proven = false;
            push_reason(
                &mut hard_failures,
                "track_engagement_type_mismatch",
                &reason,
            );
        }
        Some(CandidatePolicyFilterDecision::ReviewRequired(reason)) => {
            track_engagement_type_match_proven = false;
            push_reason(
                &mut review_reasons,
                "track_engagement_type_unverified",
                &reason,
            );
        }
        None => {}
    }
    if let Some(reason) = track_work_authorization_failure(profile, posting, track) {
        push_reason(&mut hard_failures, "work_authorization_mismatch", &reason);
    } else if track.is_some() {
        passed_checks.push("work_authorization_allowed".to_string());
    }

    let required_minimum = experience_requirement
        .required_min_months
        .into_iter()
        .chain(experience_requirement.title_floor_months)
        .max();
    let required_maximum = experience_requirement.required_max_months;
    let misses_required_minimum =
        required_minimum.is_some_and(|months| months > experience_evidence.target_max_months);
    let exceeds_required_maximum =
        required_maximum.is_some_and(|months| months < experience_evidence.target_min_months);
    if misses_required_minimum || exceeds_required_maximum {
        push_reason(
            &mut hard_failures,
            "experience_outside_target_range",
            &format!(
                "This role's required level is outside this Career Track's {}-{} year target range.",
                experience_evidence.target_min_months / 12,
                (experience_evidence.target_max_months + 11) / 12,
            ),
        );
    } else if required_minimum.is_some() || required_maximum.is_some() {
        passed_checks.push("experience_aligned".to_string());
    }
    if experience_requirement
        .preferred_min_months
        .is_some_and(|months| months > experience_evidence.target_max_months)
    {
        push_reason(
            &mut review_reasons,
            "preferred_experience_above_target",
            "The preferred experience is above this Career Track's target range.",
        );
    }

    match preferences.sponsorship.as_str() {
        "required" if clearly_blocks_sponsorship(posting) => push_reason(
            &mut hard_failures,
            "sponsorship_unavailable",
            "This job appears to reject sponsorship.",
        ),
        "required" if clearly_offers_sponsorship(posting) => {
            passed_checks.push("sponsorship_available".to_string());
        }
        "required" => push_reason(
            &mut review_reasons,
            "sponsorship_needs_confirmation",
            "Sponsorship support must be confirmed before Auto-submit.",
        ),
        "ask" if clearly_offers_sponsorship(posting) => {
            passed_checks.push("sponsorship_available".to_string());
        }
        "ask" => push_reason(
            &mut review_reasons,
            "sponsorship_answer_required",
            "Confirm the sponsorship answer before Auto-submit.",
        ),
        "not_required" | "any" => {
            passed_checks.push("sponsorship_policy_passed".to_string());
        }
        _ => push_reason(
            &mut hard_failures,
            "sponsorship_policy_invalid",
            "The saved sponsorship policy is invalid and must be reviewed.",
        ),
    }

    let active_company_key = normalize_company_key(&posting.company);
    let has_active_company_application = reservations.iter().any(|reservation| {
        existing_application_id.is_none_or(|existing_id| reservation.application_id != existing_id)
            && active_attempt_status(&reservation.status)
            && reservation.company_key == active_company_key
    });
    if has_active_company_application {
        push_reason(
            &mut hard_failures,
            "company_application_exists",
            "Bluey already has an in-progress or submitted application for this company. Another Career Track, resume, or application email does not create a second candidate.",
        );
    } else {
        passed_checks.push("company_application_clear".to_string());
    }

    let period_key = attempt_period_key(now, preferences.time_zone_offset_minutes);
    let daily_count = reservations
        .iter()
        .filter(|reservation| {
            existing_application_id
                .is_none_or(|existing_id| reservation.application_id != existing_id)
                && reservation.period_key == period_key
                && active_attempt_status(&reservation.status)
        })
        .count() as i64;
    if daily_count >= preferences.daily_limit.clamp(1, 50) {
        push_reason(
            &mut hard_failures,
            "daily_limit_reached",
            "Today's application limit has been reached.",
        );
    } else {
        passed_checks.push("daily_limit_available".to_string());
    }

    if !posting.missing_requirements.is_empty() {
        push_reason(
            &mut review_reasons,
            "missing_requirements",
            "This application still has required details to review.",
        );
    }
    if posting.match_score < profile.auto_submit_threshold.clamp(60, 100) {
        push_reason(
            &mut review_reasons,
            "below_auto_submit_threshold",
            "The match score is below your Auto-submit threshold.",
        );
    }

    match capability.as_str() {
        "certified" => passed_checks.push("ats_certified".to_string()),
        "beta_review" => push_reason(
            &mut review_reasons,
            "ats_review_required",
            "This application system is in beta and requires packet review.",
        ),
        "handoff" => push_reason(
            &mut review_reasons,
            "site_handoff_required",
            "Bluey can prepare the packet, but you complete submission on this site.",
        ),
        "unknown_review" => push_reason(
            &mut review_reasons,
            "unknown_ats_review_required",
            "This application system is not certified for runner submission.",
        ),
        _ => push_reason(
            &mut hard_failures,
            "site_blocked",
            "This job link cannot be opened by Bluey Jobs.",
        ),
    }

    let discovery_gate = apply_posting_discovery_evidence(
        posting,
        now,
        &mut hard_failures,
        &mut review_reasons,
        &mut passed_checks,
    );

    let can_prepare = capability != "blocked"
        && discovery_gate.can_prepare
        && hard_failures
            .iter()
            .all(|reason| reason.code == "daily_limit_reached")
        && (!require_live_verification || !live_verification_missing);
    let queue_capable = matches!(capability.as_str(), "certified" | "beta_review");
    let can_queue = can_prepare
        && discovery_gate.can_queue
        && hard_failures.is_empty()
        && queue_capable
        && track_policy_ready
        && role_match_proven
        && location_match_proven
        && employment_type_match_proven
        && engagement_type_match_proven
        && track_employment_type_match_proven
        && track_engagement_type_match_proven
        && !live_verification_missing;
    let can_auto_submit = can_queue
        && capability == "certified"
        && review_reasons.is_empty()
        && posting.missing_requirements.is_empty()
        && posting.match_score >= profile.auto_submit_threshold.clamp(60, 100);
    let ats_certification = review_ats_certification_summary(posting, &capability);

    JobEligibilityDecision {
        capability,
        can_prepare,
        can_auto_submit,
        can_queue_local: can_queue,
        can_queue_cloud: can_queue,
        hard_failures,
        review_reasons,
        passed_checks,
        base_profile_fit: posting.match_score,
        tailored_packet_coverage: None,
        experience_requirement,
        experience_evidence,
        career_track_id: track.map(|value| value.id.clone()).unwrap_or_default(),
        application_identity_id: track.and_then(|value| value.application_identity_id.clone()),
        evidence_revision_id: None,
        ats_certification,
        evaluated_at_ms: now,
    }
}

fn push_reason(reasons: &mut Vec<EligibilityReason>, code: &str, message: &str) {
    if reasons.iter().any(|reason| reason.code == code) {
        return;
    }
    reasons.push(EligibilityReason {
        code: code.to_string(),
        message: message.to_string(),
    });
}

fn submission_capability(posting: &JobPosting) -> String {
    let raw_url = posting.canonical_url.trim();
    let Ok(url) = reqwest::Url::parse(raw_url) else {
        return "blocked".to_string();
    };
    if !matches!(url.scheme(), "http" | "https") {
        return "blocked".to_string();
    }
    let Some(host) = url
        .host_str()
        .map(|value| value.trim_end_matches('.').to_ascii_lowercase())
    else {
        return "blocked".to_string();
    };

    if host_matches_domain(&host, "linkedin.com") || host_matches_domain(&host, "indeed.com") {
        return "handoff".to_string();
    }
    if crate::jobs_ats_target::parse_provider_application_target(
        raw_url,
        crate::jobs_ats_target::ProviderApplicationTargetPurpose::Submit,
    )
    .is_some()
    {
        return "beta_review".to_string();
    }
    "unknown_review".to_string()
}

fn review_ats_certification_summary(
    posting: &JobPosting,
    capability: &str,
) -> AtsCertificationSummary {
    let target = crate::jobs_ats_target::parse_provider_application_target(
        posting.canonical_url.trim(),
        crate::jobs_ats_target::ProviderApplicationTargetPurpose::Submit,
    );
    let (provider_label, adapter_version) = match target.as_ref().map(|target| target.provider) {
        Some("greenhouse") => (
            "Greenhouse".to_string(),
            Some("2026.07.1-beta.1".to_string()),
        ),
        Some("lever") => ("Lever".to_string(), Some("2026.07.0-beta.1".to_string())),
        _ => ("Application site".to_string(), None),
    };
    let (reason, next_action) = match capability {
        "beta_review" => (
            "This exact provider target has no active ATS certification.".to_string(),
            "Review the packet and approve the provider-specific final step.".to_string(),
        ),
        "handoff" => (
            "This provider requires a user-controlled handoff.".to_string(),
            "Review the packet and complete submission on the provider site.".to_string(),
        ),
        "blocked" => (
            "This application URL is not a valid runner target.".to_string(),
            "Replace the job link with the current original employer application URL.".to_string(),
        ),
        _ => (
            "No provider-specific runner certification is available for this target.".to_string(),
            "Review the packet and use the supported review or takeover path.".to_string(),
        ),
    };
    AtsCertificationSummary {
        provider_label,
        adapter_version,
        certified_runner_kinds: Vec::new(),
        status: "review_only".to_string(),
        last_verified_at_ms: None,
        expires_at_ms: None,
        reason,
        next_action,
        canary_available: false,
    }
}

fn host_matches_domain(host: &str, domain: &str) -> bool {
    host == domain
        || host
            .strip_suffix(domain)
            .is_some_and(|prefix| prefix.ends_with('.'))
}

enum CanonicalLocationDecision {
    Allowed,
    Denied(String),
    ReviewRequired(String),
}

fn canonical_location_decision(
    posting: &JobPosting,
    profile: &CareerProfile,
    preferences: &JobPreferences,
    track: Option<&CareerTrack>,
) -> CanonicalLocationDecision {
    let workplace =
        crate::jobs_taxonomy::classify_posting_workplace(&posting.workplace, &posting.location);
    let workplace_kind = match workplace {
        crate::jobs_taxonomy::WorkplaceClassification::Known { kind, .. } => kind,
        crate::jobs_taxonomy::WorkplaceClassification::Unknown { .. } => {
            return CanonicalLocationDecision::ReviewRequired(
                "Bluey could not prove one unambiguous posting workplace type.".to_string(),
            );
        }
    };
    let remote_preference = track
        .map(|track| track.remote_preference.as_str())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(preferences.remote_preference.as_str());
    if !remote_preference.trim().is_empty()
        && !matches!(
            remote_preference,
            "remote_only" | "remote_or_hybrid" | "hybrid_ok" | "onsite_ok" | "any"
        )
    {
        return CanonicalLocationDecision::ReviewRequired(
            "Bluey could not prove this Career Track's workplace preference.".to_string(),
        );
    }

    if (preferences.location_policy == "remote_only" || remote_preference == "remote_only")
        && workplace_kind != crate::jobs_taxonomy::WorkplaceKind::Remote
    {
        return CanonicalLocationDecision::Denied(
            "Your Career Track allows remote jobs only.".to_string(),
        );
    }
    if remote_preference == "remote_or_hybrid"
        && workplace_kind == crate::jobs_taxonomy::WorkplaceKind::Onsite
    {
        return CanonicalLocationDecision::Denied(
            "Your Career Track allows remote or hybrid jobs, not on-site-only jobs.".to_string(),
        );
    }

    let mut allowed_locations = track
        .filter(|track| !track.locations.is_empty())
        .map(|track| track.locations.clone())
        .unwrap_or_else(|| preferences.desired_locations.clone());
    if track.is_none()
        && preferences.location_policy == "local"
        && !profile.current_location.trim().is_empty()
    {
        allowed_locations.push(profile.current_location.clone());
    }
    match crate::jobs_taxonomy::geography_allows(
        &posting.location,
        &allowed_locations,
        &posting.workplace,
    ) {
        crate::jobs_taxonomy::GeographyMatchDecision::Allowed { .. } => {
            CanonicalLocationDecision::Allowed
        }
        crate::jobs_taxonomy::GeographyMatchDecision::Denied { .. } => {
            CanonicalLocationDecision::Denied(format!(
                "{} is outside your selected job locations.",
                if posting.location.trim().is_empty() {
                    "This location"
                } else {
                    posting.location.trim()
                }
            ))
        }
        crate::jobs_taxonomy::GeographyMatchDecision::ReviewRequired { .. } => {
            CanonicalLocationDecision::ReviewRequired(format!(
                "Bluey could not prove that {} belongs to this Career Track's typed geography.",
                if posting.location.trim().is_empty() {
                    "this posting location"
                } else {
                    posting.location.trim()
                }
            ))
        }
    }
}

fn eligibility_error_message(decision: &JobEligibilityDecision) -> String {
    decision
        .hard_failures
        .first()
        .or_else(|| decision.review_reasons.first())
        .map(|reason| reason.message.clone())
        .unwrap_or_else(|| "This application is not eligible for that action.".to_string())
}

fn normalize_company_key(company: &str) -> String {
    const LEGAL_SUFFIXES: &[&str] = &[
        "ag",
        "co",
        "company",
        "corp",
        "corporation",
        "gmbh",
        "inc",
        "incorporated",
        "limited",
        "llc",
        "ltd",
        "plc",
        "pte",
        "pty",
    ];
    let mut tokens: Vec<String> = company
        .to_ascii_lowercase()
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(str::to_string)
        .collect();
    let fallback = tokens.concat();
    if tokens.first().is_some_and(|token| token == "the") {
        tokens.remove(0);
    }
    while tokens
        .last()
        .is_some_and(|token| LEGAL_SUFFIXES.contains(&token.as_str()))
    {
        tokens.pop();
    }
    let normalized = tokens.concat();
    if normalized.is_empty() {
        fallback
    } else {
        normalized
    }
}

fn active_attempt_status(status: &str) -> bool {
    matches!(
        status,
        "reserved" | "running" | "side_effect_unknown" | "submitted"
    )
}

fn attempt_period_key(at_ms: i64, offset_minutes: i64) -> String {
    let adjusted = at_ms.saturating_add(offset_minutes.clamp(-840, 840) * 60_000);
    Utc.timestamp_millis_opt(adjusted)
        .single()
        .map(|timestamp| {
            format!(
                "{:04}-{:02}-{:02}",
                timestamp.year(),
                timestamp.month(),
                timestamp.day()
            )
        })
        .unwrap_or_else(|| "1970-01-01".to_string())
}

fn compensation_range(compensation: &str) -> Option<(i64, i64)> {
    let normalized = compensation.replace([',', '$'], "").to_lowercase();
    let mut amounts = Vec::new();
    for token in normalized
        .split(|character: char| !(character.is_ascii_digit() || character == 'k'))
        .filter(|token| !token.is_empty())
    {
        if let Some(raw) = token.strip_suffix('k') {
            if let Ok(value) = raw.parse::<i64>() {
                amounts.push(value * 1_000);
            }
        } else if let Ok(value) = token.parse::<i64>() {
            amounts.push(if value < 1_000 { value * 1_000 } else { value });
        }
    }
    if amounts.is_empty() {
        None
    } else {
        Some((
            *amounts.iter().min().unwrap_or(&0),
            *amounts.iter().max().unwrap_or(&0),
        ))
    }
}

fn clearly_blocks_sponsorship(posting: &JobPosting) -> bool {
    let text = format!("{} {}", posting.title, posting.description).to_lowercase();
    [
        "no sponsorship",
        "not sponsor",
        "does not sponsor",
        "do not sponsor",
        "don't sponsor",
        "not able to sponsor",
        "without sponsorship",
        "must be authorized to work",
        "will not sponsor",
        "cannot sponsor",
        "unable to sponsor",
        "not eligible for visa sponsorship",
        "not eligible for sponsorship",
        "ineligible for visa sponsorship",
        "ineligible for sponsorship",
        "no visa sponsorship available",
        "no visa sponsorship is available",
        "no sponsorship available",
        "no sponsorship is available",
        "visa sponsorship is not available",
        "visa sponsorship not available",
        "sponsorship is not available",
        "sponsorship not available",
        "sponsorship unavailable",
        "u.s. citizenship required",
        "us citizenship required",
        "must be a u.s. citizen",
        "must be a us citizen",
        "must be u.s. citizen",
        "must be us citizen",
    ]
    .iter()
    .any(|phrase| text.contains(phrase))
}

fn clearly_offers_sponsorship(posting: &JobPosting) -> bool {
    if clearly_blocks_sponsorship(posting) {
        return false;
    }
    let text = format!("{} {}", posting.title, posting.description).to_lowercase();
    [
        "eligible for visa sponsorship",
        "eligible for sponsorship",
        "visa sponsorship is available",
        "visa sponsorship available",
        "sponsorship is available",
        "sponsorship available",
        "we provide visa sponsorship",
        "we offer visa sponsorship",
        "will sponsor visas",
        "can sponsor visas",
    ]
    .iter()
    .any(|phrase| text.contains(phrase))
}

fn score_posting(
    posting: &JobPosting,
    profile: &CareerProfile,
    preferences: &JobPreferences,
    track: Option<&CareerTrack>,
) -> (i64, Vec<String>, Vec<String>) {
    let mut score = 25i64;
    let mut reasons = Vec::new();
    let mut missing = Vec::new();

    let target_roles: Vec<_> = track
        .map(|value| vec![value.role.as_str()])
        .unwrap_or_else(|| {
            preferences
                .desired_roles
                .iter()
                .map(String::as_str)
                .collect()
        })
        .into_iter()
        .filter_map(
            |role| match crate::jobs_taxonomy::resolve_target_role(role) {
                crate::jobs_taxonomy::TargetRoleResolution::Known {
                    role_id, family_id, ..
                } => Some((role_id, family_id)),
                crate::jobs_taxonomy::TargetRoleResolution::Ambiguous { .. }
                | crate::jobs_taxonomy::TargetRoleResolution::CustomReview { .. } => None,
            },
        )
        .collect();
    let posting_role = crate::jobs_taxonomy::classify_posting_role(&posting.title);
    let (exact_role_match, family_role_match, proven_role_mismatch) = match &posting_role {
        crate::jobs_taxonomy::PostingRoleClassification::Known {
            family_id,
            matched_role_ids,
            ..
        } => (
            target_roles
                .iter()
                .any(|(role_id, _)| matched_role_ids.contains(role_id)),
            target_roles
                .iter()
                .any(|(_, target_family_id)| target_family_id == family_id),
            !target_roles.is_empty()
                && target_roles
                    .iter()
                    .all(|(_, target_family_id)| target_family_id != family_id),
        ),
        crate::jobs_taxonomy::PostingRoleClassification::Ambiguous { .. }
        | crate::jobs_taxonomy::PostingRoleClassification::Unknown { .. } => (false, false, false),
    };
    if exact_role_match {
        score += 25;
        reasons.push("Role matches this Career Track".to_string());
    } else if family_role_match {
        score += 15;
        reasons.push("Role family matches this Career Track".to_string());
    } else if proven_role_mismatch {
        score -= 20;
        missing.push("Role belongs to a different Career Track".to_string());
    }

    let posting_skill_text = format!("{}\n{}", posting.title, posting.description);
    let matching_skills: Vec<String> = profile
        .skills
        .iter()
        .filter(|skill| crate::jobs_taxonomy::skill_matches_text(&posting_skill_text, skill))
        .take(5)
        .cloned()
        .collect();
    if !matching_skills.is_empty() {
        score += (matching_skills.len() as i64 * 5).min(25);
        reasons.push(format!("Matches {} profile skills", matching_skills.len()));
    } else if !profile.skills.is_empty() && !posting.description.is_empty() {
        missing.push("No direct skill overlap found yet".to_string());
    }

    if matches!(
        canonical_location_decision(posting, profile, preferences, track),
        CanonicalLocationDecision::Allowed
    ) {
        score += 15;
        reasons.push("Location preference fits".to_string());
    }

    let evidence = role_experience_evidence(profile, track, posting);
    let requirement = experience_requirement(posting);
    let required_minimum = requirement
        .required_min_months
        .into_iter()
        .chain(requirement.title_floor_months)
        .max();
    let required_maximum = requirement.required_max_months;
    let required_aligned = required_minimum
        .is_none_or(|months| months <= evidence.target_max_months)
        && required_maximum.is_none_or(|months| months >= evidence.target_min_months);
    if required_minimum.is_some() || required_maximum.is_some() {
        if required_aligned {
            score += 15;
            reasons.push(format!(
                "Required experience fits this Track's {}-{} year range",
                evidence.target_min_months / 12,
                (evidence.target_max_months + 11) / 12,
            ));
        } else {
            score -= 25;
            missing.push(format!(
                "Required experience is outside this Track's {}-{} year range",
                evidence.target_min_months / 12,
                (evidence.target_max_months + 11) / 12,
            ));
        }
    }
    if requirement
        .preferred_min_months
        .is_some_and(|months| months <= evidence.target_max_months)
    {
        score += 5;
        reasons.push("Preferred experience also fits".to_string());
    }

    if posting.compensation.is_empty() || preferences.minimum_compensation.is_none() {
        reasons.push("Compensation needs confirmation".to_string());
    }

    (score.clamp(0, 99), reasons, missing)
}

fn candidate_truth_fingerprint(profile: &CareerProfile) -> String {
    // Fingerprint the exact candidate snapshot used to build the packet. The
    // login/application email is intentionally excluded because the verified
    // application identity is fenced separately, and `updated_at_ms` is not a
    // candidate fact. Everything else can affect resume prose, form answers,
    // eligibility, or user-visible contact data and must invalidate stale work.
    let mut snapshot = profile.clone();
    snapshot.email.clear();
    snapshot.updated_at_ms = 0;
    let encoded = serde_json::to_vec(&snapshot)
        .expect("CareerProfile contains no serialization-fallible values");
    hex::encode(Sha256::digest(encoded))
}

fn confirmed_facts_fingerprint(facts: &[CareerFact]) -> String {
    let mut confirmed = facts
        .iter()
        .filter(|fact| fact.verification_status == "confirmed")
        .cloned()
        .collect::<Vec<_>>();
    confirmed.sort_by(|left, right| left.id.cmp(&right.id));
    let encoded = serde_json::to_vec(&confirmed)
        .expect("CareerFact contains no serialization-fallible values");
    hex::encode(Sha256::digest(encoded))
}

fn selected_application_identity<'a>(
    track_identity_id: Option<&str>,
    identities: &'a [ApplicationIdentity],
) -> Option<&'a ApplicationIdentity> {
    track_identity_id
        .and_then(|identity_id| identities.iter().find(|item| item.id == identity_id))
        .or_else(|| identities.iter().find(|item| item.is_default))
        .filter(|item| item.verification_status == "verified")
}

fn posting_snapshot_fingerprint(posting: &JobPosting) -> Result<String> {
    let encoded = serde_json::to_vec(posting).context("encode Jobs posting snapshot")?;
    Ok(hex::encode(Sha256::digest(encoded)))
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ApplicationAttemptAdmissionError {
    #[error("application attempt requires current authority review ({reason_code})")]
    ReviewRequired { reason_code: String },
    #[error("application attempt is blocked by current authority ({reason_code})")]
    Blocked { reason_code: String },
}

fn attempt_review_required(reason_code: &str) -> anyhow::Error {
    anyhow::Error::new(ApplicationAttemptAdmissionError::ReviewRequired {
        reason_code: reason_code.to_string(),
    })
}

fn attempt_blocked(reason_code: &str) -> anyhow::Error {
    anyhow::Error::new(ApplicationAttemptAdmissionError::Blocked {
        reason_code: reason_code.to_string(),
    })
}

fn current_attempt_employer_domain(
    application: &JobApplication,
    composed: &ComposedJobIntegrityProjection,
) -> Result<OperationalHoldEmployerDomain> {
    match composed.job_integrity.status {
        JobIntegrityResolutionStatus::Verified => {}
        JobIntegrityResolutionStatus::Blocked
        | JobIntegrityResolutionStatus::Mismatch
        | JobIntegrityResolutionStatus::Revoked => {
            return Err(attempt_blocked(&composed.job_integrity.reason_code));
        }
        JobIntegrityResolutionStatus::Absent
        | JobIntegrityResolutionStatus::ReviewRequired
        | JobIntegrityResolutionStatus::Expired => {
            return Err(attempt_review_required(&composed.job_integrity.reason_code));
        }
    }
    if !original_source_projection_matches_application(application, &composed.original_source)? {
        return Err(attempt_blocked("original_source_receipt_mismatch"));
    }
    if !job_integrity_resolution_matches_application(application, &composed.job_integrity)? {
        return Err(attempt_blocked("job_integrity_receipt_mismatch"));
    }
    let authority = composed
        .job_integrity
        .authority
        .as_ref()
        .ok_or_else(|| attempt_review_required("job_integrity_authority_missing"))?;
    OperationalHoldEmployerDomain::from_current_job_integrity_authority(authority)
        .map_err(anyhow::Error::new)
}

fn load_attempt_authority_inputs_sqlite_tx(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    application_id: &str,
) -> Result<(JobApplication, JobPosting)> {
    let (job_id, application_json, posting_id, posting_json) = tx
        .query_row(
            "SELECT application.job_id, application.application_json,
                    posting.id, posting.posting_json
               FROM jobs_applications application
               JOIN jobs_postings posting
                 ON posting.account_id = application.account_id
                AND posting.id = application.job_id
              WHERE application.account_id = ?1 AND application.id = ?2",
            params![account_id, application_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            },
        )
        .optional()?
        .ok_or_else(|| anyhow::anyhow!("application or job not found"))?;
    let application = parse_application_json(
        application_json,
        application_id,
        &job_id,
        "Jobs attempt authority application",
    )?;
    let mut posting: JobPosting = parse_json(posting_json, "Jobs attempt authority posting")?;
    posting.id = posting_id;
    Ok((application, posting))
}

fn load_attempt_authority_inputs_postgres_tx(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    application_id: &str,
) -> Result<(JobApplication, JobPosting)> {
    let row = tx
        .query_opt(
            "SELECT application.job_id, application.application_json,
                    posting.id, posting.posting_json
               FROM jobs_applications application
               JOIN jobs_postings posting
                 ON posting.account_id = application.account_id
                AND posting.id = application.job_id
              WHERE application.account_id = $1 AND application.id = $2
              FOR SHARE OF application, posting",
            &[&account_id, &application_id],
        )?
        .ok_or_else(|| anyhow::anyhow!("application or job not found"))?;
    let job_id = row.get::<_, String>(0);
    let application = parse_application_json(
        row.get(1),
        application_id,
        &job_id,
        "Jobs attempt authority application",
    )?;
    let mut posting: JobPosting = parse_json(row.get(3), "Jobs attempt authority posting")?;
    posting.id = row.get(2);
    Ok((application, posting))
}

fn require_attempt_runner_capability_sqlite_tx(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    application: &JobApplication,
    employer_domain: &OperationalHoldEmployerDomain,
    runner: &'static str,
    runner_authority: ExecutionAuthorityRunner,
    entitled: bool,
) -> Result<()> {
    if !entitled {
        return Err(attempt_review_required("runner_entitlement_unavailable"));
    }
    let mut hold_context = operational_hold_context_for_application_sqlite_tx_after_authority(
        tx,
        account_id,
        &application.id,
        employer_domain,
        Some(runner),
        None,
        None,
    )
    .map_err(anyhow::Error::new)?;
    add_candidate_queue_hold_scopes(&mut hold_context, application)?;
    require_operational_capability_sqlite_tx(
        tx,
        OperationalCapability::ApplicationQueue,
        &hold_context,
    )
    .map_err(anyhow::Error::new)?;
    if !current_execution_authorized_sqlite(tx, account_id, application, runner_authority)? {
        return Err(attempt_review_required(
            "current_execution_authority_denied",
        ));
    }
    Ok(())
}

fn require_attempt_account_hold_clear_sqlite_tx(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
) -> Result<()> {
    let context = OperationalHoldContext::new()
        .with_scope(OperationalHoldScopeKind::Account, account_id)
        .map_err(anyhow::Error::new)?;
    require_operational_capability_sqlite_tx(tx, OperationalCapability::ApplicationQueue, &context)
        .map_err(anyhow::Error::new)
}

fn require_attempt_account_hold_clear_postgres_tx(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
) -> Result<()> {
    let context = OperationalHoldContext::new()
        .with_scope(OperationalHoldScopeKind::Account, account_id)
        .map_err(anyhow::Error::new)?;
    require_operational_capability_postgres_tx_after_authority_prelock(
        tx,
        OperationalCapability::ApplicationQueue,
        &context,
    )
    .map_err(anyhow::Error::new)
}

fn require_attempt_runner_capability_postgres_tx_after_prelock(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    application: &JobApplication,
    employer_domain: &OperationalHoldEmployerDomain,
    runner: &'static str,
    runner_authority: ExecutionAuthorityRunner,
    entitled: bool,
) -> Result<()> {
    if !entitled {
        return Err(attempt_review_required("runner_entitlement_unavailable"));
    }
    let mut hold_context =
        operational_hold_context_for_application_postgres_tx_after_authority_prelock(
            tx,
            account_id,
            &application.id,
            employer_domain,
            Some(runner),
            None,
            None,
        )
        .map_err(anyhow::Error::new)?;
    add_candidate_queue_hold_scopes(&mut hold_context, application)?;
    require_operational_capability_postgres_tx_after_authority_prelock(
        tx,
        OperationalCapability::ApplicationQueue,
        &hold_context,
    )
    .map_err(anyhow::Error::new)?;
    if !current_execution_authorized_postgres_after_prelock(
        tx,
        account_id,
        application,
        runner_authority,
    )? {
        return Err(attempt_review_required(
            "current_execution_authority_denied",
        ));
    }
    Ok(())
}

fn require_requested_attempt_runner_sqlite_tx(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    application: &JobApplication,
    employer_domain: &OperationalHoldEmployerDomain,
    runner: &str,
    entitlements: (bool, bool),
) -> Result<()> {
    match runner {
        "local" => require_attempt_runner_capability_sqlite_tx(
            tx,
            account_id,
            application,
            employer_domain,
            "local",
            ExecutionAuthorityRunner::Local,
            entitlements.0,
        ),
        "cloud" => require_attempt_runner_capability_sqlite_tx(
            tx,
            account_id,
            application,
            employer_domain,
            "cloud",
            ExecutionAuthorityRunner::Cloud,
            entitlements.1,
        ),
        "unassigned" => {
            let local = require_attempt_runner_capability_sqlite_tx(
                tx,
                account_id,
                application,
                employer_domain,
                "local",
                ExecutionAuthorityRunner::Local,
                entitlements.0,
            );
            if local.is_ok() {
                return Ok(());
            }
            require_attempt_runner_capability_sqlite_tx(
                tx,
                account_id,
                application,
                employer_domain,
                "cloud",
                ExecutionAuthorityRunner::Cloud,
                entitlements.1,
            )
            .or(local)
        }
        _ => anyhow::bail!("application runner is invalid"),
    }
}

fn require_requested_attempt_runner_postgres_tx_after_prelock(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    application: &JobApplication,
    employer_domain: &OperationalHoldEmployerDomain,
    runner: &str,
    entitlements: (bool, bool),
) -> Result<()> {
    match runner {
        "local" => require_attempt_runner_capability_postgres_tx_after_prelock(
            tx,
            account_id,
            application,
            employer_domain,
            "local",
            ExecutionAuthorityRunner::Local,
            entitlements.0,
        ),
        "cloud" => require_attempt_runner_capability_postgres_tx_after_prelock(
            tx,
            account_id,
            application,
            employer_domain,
            "cloud",
            ExecutionAuthorityRunner::Cloud,
            entitlements.1,
        ),
        "unassigned" => {
            let local = require_attempt_runner_capability_postgres_tx_after_prelock(
                tx,
                account_id,
                application,
                employer_domain,
                "local",
                ExecutionAuthorityRunner::Local,
                entitlements.0,
            );
            if local.is_ok() {
                return Ok(());
            }
            require_attempt_runner_capability_postgres_tx_after_prelock(
                tx,
                account_id,
                application,
                employer_domain,
                "cloud",
                ExecutionAuthorityRunner::Cloud,
                entitlements.1,
            )
            .or(local)
        }
        _ => anyhow::bail!("application runner is invalid"),
    }
}

pub fn list_attempt_reservations(
    pool: &DbPool,
    account_id: &str,
) -> Result<Vec<AttemptReservation>> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get()?;
            let mut stmt = conn.prepare(
                "SELECT id, application_id, company_key, period_key, runner, status,
                        reserved_at_ms, updated_at_ms
                   FROM jobs_attempt_reservations
                  WHERE account_id = ?1 ORDER BY reserved_at_ms DESC",
            )?;
            let rows = stmt
                .query_map(params![account_id], |row| {
                    Ok(AttemptReservation {
                        id: row.get(0)?,
                        application_id: row.get(1)?,
                        company_key: row.get(2)?,
                        period_key: row.get(3)?,
                        runner: row.get(4)?,
                        status: row.get(5)?,
                        reserved_at_ms: row.get(6)?,
                        updated_at_ms: row.get(7)?,
                    })
                })?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            Ok(rows)
        }
        DbPool::Postgres(_) => Ok(pool
            .get_pg()?
            .query(
                "SELECT id, application_id, company_key, period_key, runner, status,
                        reserved_at_ms, updated_at_ms
                   FROM jobs_attempt_reservations
                  WHERE account_id = $1 ORDER BY reserved_at_ms DESC",
                &[&account_id],
            )?
            .into_iter()
            .map(|row| AttemptReservation {
                id: row.get(0),
                application_id: row.get(1),
                company_key: row.get(2),
                period_key: row.get(3),
                runner: row.get(4),
                status: row.get(5),
                reserved_at_ms: row.get(6),
                updated_at_ms: row.get(7),
            })
            .collect()),
    })
}

pub fn reserve_application_attempt(
    pool: &DbPool,
    account_id: &str,
    application_id: &str,
    runner: &str,
) -> Result<AttemptReservation> {
    if !matches!(runner, "cloud" | "local" | "unassigned") {
        anyhow::bail!("application runner is invalid")
    }

    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
            crate::db::object_uploads::require_active_account_write_fence_sqlite_tx(
                &tx, account_id,
            )?;
            require_attempt_account_hold_clear_sqlite_tx(&tx, account_id)?;
            let (application, posting) =
                load_attempt_authority_inputs_sqlite_tx(&tx, account_id, application_id)?;
            let employer_domain = ensure_original_source_application_authority_sqlite_tx(
                &tx,
                account_id,
                &application,
                &posting,
            )?;
            let entitlements = tx
                .query_row(
                    "SELECT local_browser, cloud_browser FROM jobs_entitlements
                      WHERE account_id = ?1",
                    params![account_id],
                    |row| Ok((row.get::<_, i64>(0)? != 0, row.get::<_, i64>(1)? != 0)),
                )
                .optional()?
                .ok_or_else(|| attempt_review_required("runner_entitlement_unavailable"))?;
            require_requested_attempt_runner_sqlite_tx(
                &tx,
                account_id,
                &application,
                &employer_domain,
                runner,
                entitlements,
            )?;

            let now = now_ms();
            let preferences_json: String = tx.query_row(
                "SELECT preferences_json FROM jobs_preferences WHERE account_id = ?1",
                params![account_id],
                |row| row.get(0),
            )?;
            let preferences: JobPreferences =
                parse_json(preferences_json, "Jobs reservation preferences")?;
            let period_key = attempt_period_key(now, preferences.time_zone_offset_minutes);
            let daily_limit = preferences.daily_limit.clamp(1, 50);
            let company_key = normalize_company_key(&posting.company);
            let id = format!("attempt-{application_id}");

            if let Some(existing) = tx
                .query_row(
                    "SELECT id, application_id, company_key, period_key, runner, status,
                            reserved_at_ms, updated_at_ms
                       FROM jobs_attempt_reservations
                      WHERE account_id = ?1 AND application_id = ?2",
                    params![account_id, application_id],
                    |row| {
                        Ok(AttemptReservation {
                            id: row.get(0)?,
                            application_id: row.get(1)?,
                            company_key: row.get(2)?,
                            period_key: row.get(3)?,
                            runner: row.get(4)?,
                            status: row.get(5)?,
                            reserved_at_ms: row.get(6)?,
                            updated_at_ms: row.get(7)?,
                        })
                    },
                )
                .optional()?
            {
                if active_attempt_status(&existing.status) {
                    if existing.runner == runner {
                        tx.commit()?;
                        return Ok(existing);
                    }
                    if existing.status != "reserved" {
                        anyhow::bail!("an active application attempt cannot change runners")
                    }
                    if tx.execute(
                        "UPDATE jobs_attempt_reservations SET runner = ?3, updated_at_ms = ?4
                          WHERE account_id = ?1 AND application_id = ?2
                            AND runner = ?5 AND status = 'reserved'",
                        params![account_id, application_id, runner, now, existing.runner],
                    )? != 1
                    {
                        anyhow::bail!("application attempt changed before runner assignment")
                    }
                    tx.commit()?;
                    return Ok(AttemptReservation {
                        runner: runner.to_string(),
                        updated_at_ms: now,
                        ..existing
                    });
                }
            }
            let used: i64 = tx.query_row(
                "SELECT COUNT(*) FROM jobs_attempt_reservations
                  WHERE account_id = ?1 AND period_key = ?2
                    AND status IN ('reserved', 'running', 'side_effect_unknown', 'submitted')",
                params![account_id, period_key],
                |row| row.get(0),
            )?;
            if used >= daily_limit {
                anyhow::bail!("today's application attempt limit has been reached")
            }
            let company_in_use: i64 = tx.query_row(
                "SELECT COUNT(*) FROM jobs_attempt_reservations
                  WHERE account_id = ?1 AND company_key = ?2 AND application_id <> ?3
                    AND status IN ('reserved', 'running', 'side_effect_unknown', 'submitted')",
                params![account_id, company_key, application_id],
                |row| row.get(0),
            )?;
            if company_in_use > 0 {
                anyhow::bail!(
                    "an in-progress or submitted application already exists for this company"
                )
            }
            tx.execute(
                "INSERT INTO jobs_attempt_reservations (
                    id, account_id, application_id, company_key, period_key, runner, status,
                    reserved_at_ms, updated_at_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'reserved', ?7, ?7)
                 ON CONFLICT(account_id, application_id) DO UPDATE SET
                    company_key = excluded.company_key, period_key = excluded.period_key,
                    runner = excluded.runner, status = 'reserved', reserved_at_ms = excluded.reserved_at_ms,
                    updated_at_ms = excluded.updated_at_ms",
                params![id, account_id, application_id, company_key, period_key, runner, now],
            )?;
            tx.commit()?;
            Ok(AttemptReservation {
                id,
                application_id: application_id.to_string(),
                company_key,
                period_key,
                runner: runner.to_string(),
                status: "reserved".to_string(),
                reserved_at_ms: now,
                updated_at_ms: now,
            })
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            lock_operational_hold_shared_postgres_tx(&mut tx).map_err(anyhow::Error::new)?;
            lock_managed_cloud_release_registry_shared_postgres_tx(&mut tx)
                .map_err(anyhow::Error::new)?;
            lock_postgres_ats_certification(&mut tx).map_err(anyhow::Error::new)?;
            lock_discovery_account_shared_postgres(&mut tx, account_id)?;
            require_attempt_account_hold_clear_postgres_tx(&mut tx, account_id)?;
            crate::db::object_uploads::require_active_account_write_fence_postgres_tx(
                &mut tx, account_id,
            )?;
            let (application, posting) =
                load_attempt_authority_inputs_postgres_tx(&mut tx, account_id, application_id)?;
            tx.query_one(
                "SELECT account_id FROM jobs_entitlements WHERE account_id = $1 FOR UPDATE",
                &[&account_id],
            )?;
            let entitlement = tx
                .query_opt(
                    "SELECT local_browser, cloud_browser FROM jobs_entitlements
                      WHERE account_id = $1",
                    &[&account_id],
                )?
                .ok_or_else(|| attempt_review_required("runner_entitlement_unavailable"))?;
            let entitlements = (
                entitlement.get::<_, i32>(0) != 0,
                entitlement.get::<_, i32>(1) != 0,
            );

            let existing = tx
                .query_opt(
                    "SELECT id, application_id, company_key, period_key, runner, status,
                        reserved_at_ms, updated_at_ms
                   FROM jobs_attempt_reservations
                  WHERE account_id = $1 AND application_id = $2 FOR UPDATE",
                    &[&account_id, &application_id],
                )?
                .map(|row| AttemptReservation {
                    id: row.get(0),
                    application_id: row.get(1),
                    company_key: row.get(2),
                    period_key: row.get(3),
                    runner: row.get(4),
                    status: row.get(5),
                    reserved_at_ms: row.get(6),
                    updated_at_ms: row.get(7),
                });
            let employer_domain = ensure_original_source_application_authority_postgres_tx(
                &mut tx,
                account_id,
                &application,
                &posting,
            )?;
            require_requested_attempt_runner_postgres_tx_after_prelock(
                &mut tx,
                account_id,
                &application,
                &employer_domain,
                runner,
                entitlements,
            )?;
            let now = now_ms();
            let preferences_json: String = tx
                .query_one(
                    "SELECT preferences_json FROM jobs_preferences WHERE account_id = $1",
                    &[&account_id],
                )?
                .get(0);
            let preferences: JobPreferences =
                parse_json(preferences_json, "Jobs reservation preferences")?;
            let period_key = attempt_period_key(now, preferences.time_zone_offset_minutes);
            let daily_limit = preferences.daily_limit.clamp(1, 50);
            let company_key = normalize_company_key(&posting.company);
            let id = format!("attempt-{application_id}");
            if let Some(existing) = existing {
                if active_attempt_status(&existing.status) {
                    if existing.runner == runner {
                        tx.commit()?;
                        return Ok(existing);
                    }
                    if existing.status != "reserved" {
                        anyhow::bail!("an active application attempt cannot change runners")
                    }
                    if tx.execute(
                        "UPDATE jobs_attempt_reservations SET runner = $3, updated_at_ms = $4
                          WHERE account_id = $1 AND application_id = $2
                            AND runner = $5 AND status = 'reserved'",
                        &[
                            &account_id,
                            &application_id,
                            &runner,
                            &now,
                            &existing.runner,
                        ],
                    )? != 1
                    {
                        anyhow::bail!("application attempt changed before runner assignment")
                    }
                    tx.commit()?;
                    return Ok(AttemptReservation {
                        runner: runner.to_string(),
                        updated_at_ms: now,
                        ..existing
                    });
                }
            }
            let used: i64 = tx
                .query_one(
                    "SELECT COUNT(*) FROM jobs_attempt_reservations
                      WHERE account_id = $1 AND period_key = $2
                        AND status IN ('reserved', 'running', 'side_effect_unknown', 'submitted')",
                    &[&account_id, &period_key],
                )?
                .get(0);
            if used >= daily_limit {
                anyhow::bail!("today's application attempt limit has been reached")
            }
            let company_in_use: i64 = tx
                .query_one(
                    "SELECT COUNT(*) FROM jobs_attempt_reservations
                      WHERE account_id = $1 AND company_key = $2 AND application_id <> $3
                        AND status IN ('reserved', 'running', 'side_effect_unknown', 'submitted')",
                    &[&account_id, &company_key, &application_id],
                )?
                .get(0);
            if company_in_use > 0 {
                anyhow::bail!(
                    "an in-progress or submitted application already exists for this company"
                )
            }
            tx.execute(
                "INSERT INTO jobs_attempt_reservations (
                    id, account_id, application_id, company_key, period_key, runner, status,
                    reserved_at_ms, updated_at_ms
                 ) VALUES ($1, $2, $3, $4, $5, $6, 'reserved', $7, $7)
                 ON CONFLICT(account_id, application_id) DO UPDATE SET
                    company_key = EXCLUDED.company_key, period_key = EXCLUDED.period_key,
                    runner = EXCLUDED.runner, status = 'reserved', reserved_at_ms = EXCLUDED.reserved_at_ms,
                    updated_at_ms = EXCLUDED.updated_at_ms",
                &[&id, &account_id, &application_id, &company_key, &period_key, &runner, &now],
            )?;
            tx.commit()?;
            Ok(AttemptReservation {
                id,
                application_id: application_id.to_string(),
                company_key,
                period_key,
                runner: runner.to_string(),
                status: "reserved".to_string(),
                reserved_at_ms: now,
                updated_at_ms: now,
            })
        }
    })
}

pub fn update_attempt_reservation_status(
    pool: &DbPool,
    account_id: &str,
    application_id: &str,
    status: &str,
) -> Result<bool> {
    if !matches!(
        status,
        "reserved" | "running" | "released" | "side_effect_unknown" | "submitted"
    ) {
        anyhow::bail!("invalid application attempt status")
    }
    let requires_running_capability = matches!(status, "reserved" | "running");
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
            crate::db::object_uploads::require_active_account_write_fence_sqlite_tx(
                &tx, account_id,
            )?;
            let existing = tx
                .query_row(
                    "SELECT id, application_id, company_key, period_key, runner, status,
                            reserved_at_ms, updated_at_ms
                       FROM jobs_attempt_reservations
                      WHERE account_id = ?1 AND application_id = ?2",
                    params![account_id, application_id],
                    |row| {
                        Ok(AttemptReservation {
                            id: row.get(0)?,
                            application_id: row.get(1)?,
                            company_key: row.get(2)?,
                            period_key: row.get(3)?,
                            runner: row.get(4)?,
                            status: row.get(5)?,
                            reserved_at_ms: row.get(6)?,
                            updated_at_ms: row.get(7)?,
                        })
                    },
                )
                .optional()?;
            let Some(existing) = existing else {
                tx.commit()?;
                return Ok(false);
            };
            if requires_running_capability {
                require_attempt_account_hold_clear_sqlite_tx(&tx, account_id)?;
                if status == "running" && existing.runner == "unassigned" {
                    return Err(attempt_review_required("exact_runner_assignment_required"));
                }
                if status == "running"
                    && !matches!(existing.status.as_str(), "reserved" | "running")
                {
                    anyhow::bail!("only a reserved application attempt can start running")
                }
                let (application, posting) =
                    load_attempt_authority_inputs_sqlite_tx(&tx, account_id, application_id)?;
                let employer_domain = ensure_original_source_application_authority_sqlite_tx(
                    &tx,
                    account_id,
                    &application,
                    &posting,
                )?;
                let entitlements = tx
                    .query_row(
                        "SELECT local_browser, cloud_browser FROM jobs_entitlements
                          WHERE account_id = ?1",
                        params![account_id],
                        |row| Ok((row.get::<_, i64>(0)? != 0, row.get::<_, i64>(1)? != 0)),
                    )
                    .optional()?
                    .ok_or_else(|| attempt_review_required("runner_entitlement_unavailable"))?;
                require_requested_attempt_runner_sqlite_tx(
                    &tx,
                    account_id,
                    &application,
                    &employer_domain,
                    &existing.runner,
                    entitlements,
                )?;
            }
            if existing.status == status {
                tx.commit()?;
                return Ok(true);
            }
            let now = now_ms();
            let changed = tx.execute(
                "UPDATE jobs_attempt_reservations SET status = ?3, updated_at_ms = ?4
                  WHERE account_id = ?1 AND application_id = ?2
                    AND runner = ?5 AND status = ?6",
                params![
                    account_id,
                    application_id,
                    status,
                    now,
                    existing.runner,
                    existing.status
                ],
            )? > 0;
            tx.commit()?;
            Ok(changed)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            if requires_running_capability {
                lock_operational_hold_shared_postgres_tx(&mut tx).map_err(anyhow::Error::new)?;
                lock_managed_cloud_release_registry_shared_postgres_tx(&mut tx)
                    .map_err(anyhow::Error::new)?;
                lock_postgres_ats_certification(&mut tx).map_err(anyhow::Error::new)?;
                lock_discovery_account_shared_postgres(&mut tx, account_id)?;
                require_attempt_account_hold_clear_postgres_tx(&mut tx, account_id)?;
            }
            crate::db::object_uploads::require_active_account_write_fence_postgres_tx(
                &mut tx, account_id,
            )?;
            let current_authority = if requires_running_capability {
                let (application, posting) =
                    load_attempt_authority_inputs_postgres_tx(&mut tx, account_id, application_id)?;
                let entitlement = tx
                    .query_opt(
                        "SELECT local_browser, cloud_browser FROM jobs_entitlements
                          WHERE account_id = $1 FOR UPDATE",
                        &[&account_id],
                    )?
                    .ok_or_else(|| attempt_review_required("runner_entitlement_unavailable"))?;
                Some((
                    application,
                    posting,
                    (
                        entitlement.get::<_, i32>(0) != 0,
                        entitlement.get::<_, i32>(1) != 0,
                    ),
                ))
            } else {
                None
            };
            let existing = tx.query_opt(
                "SELECT id, application_id, company_key, period_key, runner, status,
                        reserved_at_ms, updated_at_ms
                   FROM jobs_attempt_reservations
                  WHERE account_id = $1 AND application_id = $2 FOR UPDATE",
                &[&account_id, &application_id],
            )?;
            let Some(existing) = existing.map(|row| AttemptReservation {
                id: row.get(0),
                application_id: row.get(1),
                company_key: row.get(2),
                period_key: row.get(3),
                runner: row.get(4),
                status: row.get(5),
                reserved_at_ms: row.get(6),
                updated_at_ms: row.get(7),
            }) else {
                tx.commit()?;
                return Ok(false);
            };
            if requires_running_capability {
                if status == "running" && existing.runner == "unassigned" {
                    return Err(attempt_review_required("exact_runner_assignment_required"));
                }
                if status == "running"
                    && !matches!(existing.status.as_str(), "reserved" | "running")
                {
                    anyhow::bail!("only a reserved application attempt can start running")
                }
                let (application, posting, entitlements) =
                    current_authority.as_ref().ok_or_else(|| {
                        anyhow::anyhow!("current application attempt authority is unavailable")
                    })?;
                let employer_domain = ensure_original_source_application_authority_postgres_tx(
                    &mut tx,
                    account_id,
                    application,
                    posting,
                )?;
                require_requested_attempt_runner_postgres_tx_after_prelock(
                    &mut tx,
                    account_id,
                    application,
                    &employer_domain,
                    &existing.runner,
                    *entitlements,
                )?;
            }
            if existing.status == status {
                tx.commit()?;
                return Ok(true);
            }
            let now = now_ms();
            let changed = tx.execute(
                "UPDATE jobs_attempt_reservations SET status = $3, updated_at_ms = $4
                  WHERE account_id = $1 AND application_id = $2
                    AND runner = $5 AND status = $6",
                &[
                    &account_id,
                    &application_id,
                    &status,
                    &now,
                    &existing.runner,
                    &existing.status,
                ],
            )? > 0;
            tx.commit()?;
            Ok(changed)
        }
    })
}

fn ensure_original_source_application_authority_sqlite_tx(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    application: &JobApplication,
    posting: &JobPosting,
) -> Result<OperationalHoldEmployerDomain> {
    approved_submission_snapshot(account_id, application)?;
    let composed = resolve_composed_job_integrity_projection_sqlite_tx(tx, account_id, posting)?;
    current_attempt_employer_domain(application, &composed)
}

fn ensure_original_source_application_authority_postgres_tx(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    application: &JobApplication,
    posting: &JobPosting,
) -> Result<OperationalHoldEmployerDomain> {
    approved_submission_snapshot(account_id, application)?;
    let composed = resolve_composed_job_integrity_projection_postgres_tx_after_prelock(
        tx, account_id, posting,
    )?;
    current_attempt_employer_domain(application, &composed)
}

#[cfg(test)]
mod application_attempt_admission_tests {
    use super::*;

    fn section<'a>(source: &'a str, start: &str, end: &str) -> &'a str {
        source
            .split(start)
            .nth(1)
            .unwrap_or_else(|| panic!("missing section start: {start}"))
            .split(end)
            .next()
            .unwrap_or_else(|| panic!("missing section end: {end}"))
    }

    fn assert_ordered(source: &str, needles: &[&str]) {
        let mut previous = None;
        for needle in needles {
            let position = source
                .find(needle)
                .unwrap_or_else(|| panic!("missing ordered marker: {needle}"));
            if let Some(previous) = previous {
                assert!(previous < position, "authority or mutation order inverted");
            }
            previous = Some(position);
        }
    }

    #[test]
    fn attempt_denials_preserve_review_required_and_blocked_types() {
        let review = attempt_review_required("ats_certification_inactive");
        assert_eq!(
            review.downcast_ref::<ApplicationAttemptAdmissionError>(),
            Some(&ApplicationAttemptAdmissionError::ReviewRequired {
                reason_code: "ats_certification_inactive".to_string(),
            })
        );

        let blocked = attempt_blocked("job_risk_blocked");
        assert_eq!(
            blocked.downcast_ref::<ApplicationAttemptAdmissionError>(),
            Some(&ApplicationAttemptAdmissionError::Blocked {
                reason_code: "job_risk_blocked".to_string(),
            })
        );
    }

    #[test]
    fn reservation_and_running_mutations_follow_complete_current_authority() {
        let source = include_str!("eligibility.rs");
        let reservation = section(
            source,
            "pub fn reserve_application_attempt(",
            "pub fn update_attempt_reservation_status(",
        );
        let (reservation_sqlite, reservation_postgres) = reservation
            .split_once("DbPool::Postgres(_) =>")
            .expect("reservation backend branches");
        assert_ordered(
            reservation_sqlite,
            &[
                "ensure_original_source_application_authority_sqlite_tx",
                "require_requested_attempt_runner_sqlite_tx",
                "UPDATE jobs_attempt_reservations",
                "INSERT INTO jobs_attempt_reservations",
            ],
        );
        assert_ordered(
            reservation_postgres,
            &[
                "lock_operational_hold_shared_postgres_tx",
                "lock_managed_cloud_release_registry_shared_postgres_tx",
                "lock_postgres_ats_certification",
                "lock_discovery_account_shared_postgres",
                "load_attempt_authority_inputs_postgres_tx",
                "SELECT account_id FROM jobs_entitlements",
                "FROM jobs_attempt_reservations",
                "ensure_original_source_application_authority_postgres_tx",
                "require_requested_attempt_runner_postgres_tx_after_prelock",
                "UPDATE jobs_attempt_reservations",
                "INSERT INTO jobs_attempt_reservations",
            ],
        );

        let running = section(
            source,
            "pub fn update_attempt_reservation_status(",
            "fn ensure_original_source_application_authority_sqlite_tx(",
        );
        let (running_sqlite, running_postgres) = running
            .split_once("DbPool::Postgres(_) =>")
            .expect("running backend branches");
        assert!(running.contains(
            "let requires_running_capability = matches!(status, \"reserved\" | \"running\")"
        ));
        assert_ordered(
            running_sqlite,
            &[
                "ensure_original_source_application_authority_sqlite_tx",
                "require_requested_attempt_runner_sqlite_tx",
                "UPDATE jobs_attempt_reservations",
            ],
        );
        assert_ordered(
            running_postgres,
            &[
                "lock_operational_hold_shared_postgres_tx",
                "lock_managed_cloud_release_registry_shared_postgres_tx",
                "lock_postgres_ats_certification",
                "lock_discovery_account_shared_postgres",
                "load_attempt_authority_inputs_postgres_tx",
                "SELECT local_browser, cloud_browser FROM jobs_entitlements",
                "FROM jobs_attempt_reservations",
                "ensure_original_source_application_authority_postgres_tx",
                "require_requested_attempt_runner_postgres_tx_after_prelock",
                "UPDATE jobs_attempt_reservations",
            ],
        );

        let postgres_capability = section(
            source,
            "fn require_attempt_runner_capability_postgres_tx_after_prelock(",
            "fn require_requested_attempt_runner_sqlite_tx(",
        );
        assert!(postgres_capability.contains("current_execution_authorized_postgres_after_prelock"));
        assert!(!postgres_capability.contains("current_execution_authorized_postgres("));
    }
}
