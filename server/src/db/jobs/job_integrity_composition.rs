#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JobIntegrityReceiptV1 {
    pub subject_sha256: String,
    pub source_material_sha256: String,
    pub attestation_sha256: String,
    pub attestation_generation: i64,
    pub head_revision: i64,
    pub head_transition_sha256: String,
    pub policy_sha256: String,
    pub employer_identity_authorization_sha256: String,
    pub job_risk_authorization_sha256: String,
    pub canonical_employer_id: String,
    pub canonical_employer_domain: String,
    pub risk_policy_sha256: String,
    pub expires_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComposedJobIntegrityProjection {
    pub original_source: OriginalSourceVerificationProjection,
    pub ats_certification: Option<AtsCertificationPostingResolution>,
    pub job_integrity: JobIntegrityResolution,
    pub discovery_evidence: JobDiscoveryEvidence,
}

pub fn job_integrity_receipt_projection(
    authority: &JobIntegrityCurrentAuthority,
) -> JobIntegrityReceiptV1 {
    JobIntegrityReceiptV1 {
        subject_sha256: authority.subject_sha256.clone(),
        source_material_sha256: authority.source_material_sha256.clone(),
        attestation_sha256: authority.attestation_sha256.clone(),
        attestation_generation: authority.attestation_generation,
        head_revision: authority.head_revision,
        head_transition_sha256: authority.head_transition_sha256.clone(),
        policy_sha256: authority.policy_sha256.clone(),
        employer_identity_authorization_sha256: authority.employer_authorization_sha256.clone(),
        job_risk_authorization_sha256: authority.risk_authorization_sha256.clone(),
        canonical_employer_id: authority.canonical_employer_id.clone(),
        canonical_employer_domain: authority.canonical_employer_domain.clone(),
        risk_policy_sha256: authority.risk_policy_sha256.clone(),
        expires_at_ms: authority.effective_expires_at_ms,
    }
}

pub fn application_job_integrity_receipt(
    application: &JobApplication,
) -> Result<Option<JobIntegrityReceiptV1>> {
    let Some(value) = application.receipt.get("job_integrity") else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }
    serde_json::from_value(value.clone())
        .map(Some)
        .context("decode frozen job-integrity receipt")
}

pub fn job_integrity_resolution_matches_application(
    application: &JobApplication,
    resolution: &JobIntegrityResolution,
) -> Result<bool> {
    if resolution.status != JobIntegrityResolutionStatus::Verified {
        return Ok(false);
    }
    let Some(authority) = resolution.authority.as_ref() else {
        return Ok(false);
    };
    let Some(receipt) = application_job_integrity_receipt(application)? else {
        return Ok(false);
    };
    Ok(receipt == job_integrity_receipt_projection(authority))
}

fn composed_job_integrity_resolution(
    status: JobIntegrityResolutionStatus,
    reason_code: &str,
) -> JobIntegrityResolution {
    JobIntegrityResolution {
        status,
        reason_code: reason_code.to_string(),
        signal_codes: Vec::new(),
        authority: None,
    }
}

fn original_source_composition_denial(
    source: &OriginalSourceVerificationProjection,
) -> JobIntegrityResolution {
    let reason_code = if source.feature_active {
        "original_source_verification_not_positive"
    } else {
        "original_source_verification_inactive"
    };
    composed_job_integrity_resolution(JobIntegrityResolutionStatus::ReviewRequired, reason_code)
}

fn ats_composition_denial(
    error: AtsCertificationAuthorityError,
) -> Result<JobIntegrityResolution> {
    match error {
        AtsCertificationAuthorityError::Storage(error) => {
            Err(error).context("resolve ATS authority for job-integrity composition")
        }
        AtsCertificationAuthorityError::ScopeMismatch
        | AtsCertificationAuthorityError::RuntimeMismatch
        | AtsCertificationAuthorityError::UnsupportedUrl => Ok(composed_job_integrity_resolution(
            JobIntegrityResolutionStatus::Mismatch,
            "ats_source_binding_mismatch",
        )),
        _ => Ok(composed_job_integrity_resolution(
            JobIntegrityResolutionStatus::ReviewRequired,
            "ats_certification_not_current",
        )),
    }
}

fn job_integrity_expected_source_from_composition(
    source: &OriginalSourceIntegrityBinding,
    ats: &AtsCertificationPostingResolution,
) -> std::result::Result<JobIntegrityExpectedSource, Box<JobIntegrityResolution>> {
    if ats.status.provider != source.provider_family {
        return Err(Box::new(composed_job_integrity_resolution(
            JobIntegrityResolutionStatus::Mismatch,
            "ats_provider_mismatch",
        )));
    }
    let Some(active_binding) = ats.active_binding.as_ref() else {
        return Err(Box::new(composed_job_integrity_resolution(
            JobIntegrityResolutionStatus::ReviewRequired,
            "ats_certification_inactive",
        )));
    };
    if active_binding.provider != source.provider_family {
        return Err(Box::new(composed_job_integrity_resolution(
            JobIntegrityResolutionStatus::Mismatch,
            "ats_binding_provider_mismatch",
        )));
    }
    if ats.status.status != "active" {
        return Err(Box::new(composed_job_integrity_resolution(
            JobIntegrityResolutionStatus::ReviewRequired,
            "ats_certification_inactive",
        )));
    }
    let target_key_sha256 = ats_certification_target_key_sha256(&active_binding.target_key).ok();
    if target_key_sha256.as_deref() != Some(ats.status.target_key_sha256.as_str()) {
        return Err(Box::new(composed_job_integrity_resolution(
            JobIntegrityResolutionStatus::Mismatch,
            "ats_target_binding_mismatch",
        )));
    }
    Ok(JobIntegrityExpectedSource {
        subject_sha256: source.subject_sha256.clone(),
        source_material_sha256: source.source_material_sha256.clone(),
        source_expires_at_ms: source.source_expires_at_ms,
        canonical_job_id: source.canonical_job_id.clone(),
        provider_family: source.provider_family.clone(),
        provider_record_id: source.provider_record_id.clone(),
        provider_host: source.provider_host.clone(),
        provider_tenant: source.provider_tenant.clone(),
        provider_job: source.provider_job.clone(),
        provider_variant: source.provider_variant.clone(),
        canonical_application_url: source.canonical_application_url.clone(),
        application_domain: source.application_domain.clone(),
        ats_tenant_binding_sha256: ats.status.target_key_sha256.clone(),
    })
}

fn original_source_projected_ats_posting(
    posting: &JobPosting,
    projection: &OriginalSourceVerificationProjection,
    source: &OriginalSourceIntegrityBinding,
) -> JobPosting {
    let mut projected = posting_with_original_source_projection(posting, projection);
    // Phase 614 accepts canonical discovery aliases such as `greenhouse_import`, while the ATS
    // authority intentionally accepts only its canonical provider family. The signed source head,
    // not the mutable posting row, owns that provider identity at this composition boundary.
    projected.source = source.provider_family.clone();
    projected.canonical_url = source.canonical_application_url.clone();
    projected.discovery_evidence.application_domain = Some(source.application_domain.clone());
    projected.last_verified_at_ms = projection
        .evidence
        .as_ref()
        .and_then(|evidence| evidence.original_source_checked_at_ms);
    projected
}

fn overlay_verified_job_integrity(
    evidence: &mut JobDiscoveryEvidence,
    authority: &JobIntegrityCurrentAuthority,
) {
    let employer_status = evidence
        .employer_verification_status
        .trim()
        .to_ascii_lowercase();
    if !matches!(employer_status.as_str(), "mismatch" | "impersonated") {
        evidence.employer_verification_status = "verified".to_string();
        evidence.employer_id = Some(authority.canonical_employer_id.clone());
        evidence.canonical_employer_domain = Some(authority.canonical_employer_domain.clone());
    }
    if !evidence
        .scam_risk_status
        .trim()
        .eq_ignore_ascii_case("blocked")
    {
        evidence.scam_risk_status = "clear".to_string();
        evidence.scam_signals.clear();
    }
}

fn overlay_authoritative_integrity_denial(
    evidence: &mut JobDiscoveryEvidence,
    resolution: &JobIntegrityResolution,
) {
    if resolution.status == JobIntegrityResolutionStatus::Mismatch {
        let employer_status = evidence
            .employer_verification_status
            .trim()
            .to_ascii_lowercase();
        if employer_status != "impersonated" {
            evidence.employer_verification_status = "mismatch".to_string();
            evidence.employer_id = None;
            evidence.canonical_employer_domain = None;
        }
    }
    evidence.scam_risk_status = "blocked".to_string();
    for code in &resolution.signal_codes {
        if !evidence
            .scam_signals
            .iter()
            .any(|signal| signal.code == *code && signal.source == "job_integrity_authority")
        {
            evidence.scam_signals.push(DiscoveryScamSignal {
                code: code.clone(),
                source: "job_integrity_authority".to_string(),
            });
        }
    }
}

fn compose_job_integrity_projection(
    posting: &JobPosting,
    original_source: OriginalSourceVerificationProjection,
    ats_certification: Option<AtsCertificationPostingResolution>,
    mut job_integrity: JobIntegrityResolution,
    signed_integrity_evaluated: bool,
) -> ComposedJobIntegrityProjection {
    let mut discovery_evidence =
        posting_with_original_source_projection(posting, &original_source).discovery_evidence;
    if signed_integrity_evaluated && job_integrity.status == JobIntegrityResolutionStatus::Verified
    {
        if let Some(authority) = job_integrity.authority.as_ref() {
            overlay_verified_job_integrity(&mut discovery_evidence, authority);
        } else {
            job_integrity = composed_job_integrity_resolution(
                JobIntegrityResolutionStatus::ReviewRequired,
                "job_integrity_authority_missing",
            );
        }
    } else if matches!(
        job_integrity.status,
        JobIntegrityResolutionStatus::Blocked | JobIntegrityResolutionStatus::Mismatch
    )
    {
        // Hard denials can originate either from the signed job-integrity
        // authority or from the authoritative ATS/source-binding composition
        // that precedes it. Both must survive into discovery evidence because
        // workspace eligibility is derived from that public projection.
        overlay_authoritative_integrity_denial(&mut discovery_evidence, &job_integrity);
        job_integrity.authority = None;
    } else {
        // A malformed negative carrying an authority object must never become
        // a positive projection at this composition boundary.
        job_integrity.authority = None;
    }
    ComposedJobIntegrityProjection {
        original_source,
        ats_certification,
        job_integrity,
        discovery_evidence,
    }
}

pub(crate) fn resolve_composed_job_integrity_projection_sqlite_tx(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    posting: &JobPosting,
) -> Result<ComposedJobIntegrityProjection> {
    let db_time_ms = original_source_db_now_sqlite(tx)
        .context("sample database time for job-integrity composition")?;
    resolve_composed_job_integrity_projection_sqlite_tx_at_ms(
        tx,
        account_id,
        posting,
        db_time_ms,
    )
}

pub(crate) fn resolve_composed_job_integrity_projection_sqlite_tx_at_ms(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    posting: &JobPosting,
    db_time_ms: i64,
) -> Result<ComposedJobIntegrityProjection> {
    let original_source = resolve_original_source_verification_projection_sqlite_tx_at_ms(
        tx, account_id, posting, db_time_ms,
    )
    .context("resolve original-source authority for job-integrity composition")?;
    let Some(source_binding) = original_source.integrity_binding.clone() else {
        let denial = original_source_composition_denial(&original_source);
        return Ok(compose_job_integrity_projection(
            posting,
            original_source,
            None,
            denial,
            false,
        ));
    };
    let ats_posting =
        original_source_projected_ats_posting(posting, &original_source, &source_binding);
    let ats_certification = match resolve_ats_certification_for_posting_sqlite_tx(
        tx,
        account_id,
        &ats_posting,
        None,
        original_source.db_time_ms,
    ) {
        Ok(certification) => certification,
        Err(error) => {
            let denial = ats_composition_denial(error)?;
            return Ok(compose_job_integrity_projection(
                posting,
                original_source,
                None,
                denial,
                false,
            ));
        }
    };
    let expected =
        match job_integrity_expected_source_from_composition(&source_binding, &ats_certification) {
            Ok(expected) => expected,
            Err(denial) => {
                return Ok(compose_job_integrity_projection(
                    posting,
                    original_source,
                    Some(ats_certification),
                    *denial,
                    false,
                ));
            }
        };
    let job_integrity =
        resolve_current_job_integrity_authority_sqlite_tx_at_ms(tx, &expected, db_time_ms)
        .context("resolve signed job-integrity authority")?;
    Ok(compose_job_integrity_projection(
        posting,
        original_source,
        Some(ats_certification),
        job_integrity,
        true,
    ))
}

/// Compose source, ATS, and signed integrity after the caller has acquired
/// `H -> M -> ATS -> D`. This function acquires the integrity publication
/// fence next, then samples one database timestamp for all three authorities.
/// It must not reacquire any of the caller-owned common locks.
pub(crate) fn resolve_composed_job_integrity_projection_postgres_tx_after_prelock(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    posting: &JobPosting,
) -> Result<ComposedJobIntegrityProjection> {
    lock_job_integrity_publication_fence_shared_postgres_tx(tx)
        .map_err(anyhow::Error::new)
        .context("lock signed job-integrity publication fence")?;
    let db_time_ms = original_source_db_now_postgres(tx)
        .context("sample database time for job-integrity composition")?;
    resolve_composed_job_integrity_projection_postgres_tx_after_prelock_at_ms(
        tx,
        account_id,
        posting,
        db_time_ms,
    )
}

/// Compose at one representation timestamp after the caller has acquired
/// `H -> M -> ATS -> D -> integrity publication fence` in that order.
pub(crate) fn resolve_composed_job_integrity_projection_postgres_tx_after_prelock_at_ms(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    posting: &JobPosting,
    db_time_ms: i64,
) -> Result<ComposedJobIntegrityProjection> {
    let original_source =
        resolve_original_source_verification_projection_postgres_tx_after_prelock_at_ms(
            tx, account_id, posting, db_time_ms,
        )
        .context("resolve original-source authority for job-integrity composition")?;
    resolve_composed_job_integrity_projection_postgres_tx_with_source(
        tx,
        account_id,
        posting,
        original_source,
        db_time_ms,
    )
}

fn resolve_composed_job_integrity_projection_postgres_tx_with_source(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    posting: &JobPosting,
    original_source: OriginalSourceVerificationProjection,
    integrity_time_ms: i64,
) -> Result<ComposedJobIntegrityProjection> {
    let Some(source_binding) = original_source.integrity_binding.clone() else {
        let denial = original_source_composition_denial(&original_source);
        return Ok(compose_job_integrity_projection(
            posting,
            original_source,
            None,
            denial,
            false,
        ));
    };
    let ats_posting =
        original_source_projected_ats_posting(posting, &original_source, &source_binding);
    let ats_certification = match resolve_ats_certification_for_posting_postgres_tx(
        tx,
        account_id,
        &ats_posting,
        None,
        original_source.db_time_ms,
    ) {
        Ok(certification) => certification,
        Err(error) => {
            let denial = ats_composition_denial(error)?;
            return Ok(compose_job_integrity_projection(
                posting,
                original_source,
                None,
                denial,
                false,
            ));
        }
    };
    let expected =
        match job_integrity_expected_source_from_composition(&source_binding, &ats_certification) {
            Ok(expected) => expected,
            Err(denial) => {
                return Ok(compose_job_integrity_projection(
                    posting,
                    original_source,
                    Some(ats_certification),
                    *denial,
                    false,
                ));
            }
        };
    let job_integrity =
        resolve_current_job_integrity_authority_postgres_tx_after_publication_fence_at_ms(
            tx,
            &expected,
            integrity_time_ms,
        )
        .context("resolve signed job-integrity authority")?;
    Ok(compose_job_integrity_projection(
        posting,
        original_source,
        Some(ats_certification),
        job_integrity,
        true,
    ))
}

pub fn resolve_composed_job_integrity_projection(
    pool: &DbPool,
    account_id: &str,
    posting: &JobPosting,
) -> Result<ComposedJobIntegrityProjection> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut connection = pool.get()?;
            let tx = connection.transaction_with_behavior(TransactionBehavior::Deferred)?;
            let projection =
                resolve_composed_job_integrity_projection_sqlite_tx(&tx, account_id, posting)?;
            tx.commit()?;
            Ok(projection)
        }
        DbPool::Postgres(_) => {
            let mut connection = pool.get_pg()?;
            // READ COMMITTED is deliberate: the first advisory-lock SELECT
            // can wait behind an H/M/ATS/D publisher. A transaction-wide
            // snapshot established before that wait could retain the
            // publisher's stale authority rows after the lock is granted.
            let mut tx = connection.transaction()?;
            lock_operational_hold_shared_postgres_tx(&mut tx)?;
            lock_managed_cloud_release_registry_shared_postgres_tx(&mut tx)?;
            lock_postgres_ats_certification(&mut tx)?;
            lock_discovery_account_shared_postgres(&mut tx, account_id)?;
            let projection = resolve_composed_job_integrity_projection_postgres_tx_after_prelock(
                &mut tx, account_id, posting,
            )?;
            tx.commit()?;
            Ok(projection)
        }
    })
}

/// Resolve an administrative status from server-owned posting and authority
/// rows. The HTTP boundary must never construct `JobIntegrityExpectedSource`
/// from caller-supplied expiry, destination, or ATS coordinates.
pub fn resolve_composed_job_integrity_projection_for_posting(
    pool: &DbPool,
    account_id: &str,
    job_id: &str,
) -> Result<Option<ComposedJobIntegrityProjection>> {
    if account_id.trim().is_empty()
        || account_id.len() > 240
        || job_id.trim().is_empty()
        || job_id.len() > 240
    {
        anyhow::bail!("invalid Jobs integrity status identity");
    }
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut connection = pool.get()?;
            let tx = connection.transaction_with_behavior(TransactionBehavior::Deferred)?;
            let stored = tx
                .query_row(
                    "SELECT id, posting_json FROM jobs_postings
                      WHERE account_id=?1 AND id=?2",
                    params![account_id, job_id],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
                )
                .optional()?;
            let projection = stored
                .map(|(relational_id, raw)| -> Result<ComposedJobIntegrityProjection> {
                    let mut posting: JobPosting = parse_json(raw, "job posting")?;
                    posting.id = relational_id;
                    resolve_composed_job_integrity_projection_sqlite_tx(
                        &tx,
                        account_id,
                        &posting,
                    )
                })
                .transpose()?;
            tx.commit()?;
            Ok(projection)
        }
        DbPool::Postgres(_) => {
            let mut connection = pool.get_pg()?;
            let mut tx = connection.transaction()?;
            lock_operational_hold_shared_postgres_tx(&mut tx).map_err(anyhow::Error::new)?;
            lock_managed_cloud_release_registry_shared_postgres_tx(&mut tx)?;
            lock_postgres_ats_certification(&mut tx).map_err(anyhow::Error::new)?;
            lock_discovery_account_shared_postgres(&mut tx, account_id)?;
            let stored = tx.query_opt(
                "SELECT id, posting_json FROM jobs_postings
                  WHERE account_id=$1 AND id=$2",
                &[&account_id, &job_id],
            )?;
            let projection = stored
                .map(|row| -> Result<ComposedJobIntegrityProjection> {
                    let relational_id: String = row.get(0);
                    let mut posting: JobPosting = parse_json(row.get(1), "job posting")?;
                    posting.id = relational_id;
                    resolve_composed_job_integrity_projection_postgres_tx_after_prelock(
                        &mut tx,
                        account_id,
                        &posting,
                    )
                })
                .transpose()?;
            tx.commit()?;
            Ok(projection)
        }
    })
}

#[cfg(test)]
mod job_integrity_composition_tests {
    use super::*;
    use crate::db;

    fn digest(byte: char) -> String {
        byte.to_string().repeat(64)
    }

    fn fixture_ats_target_key_sha256() -> String {
        ats_certification_target_key_sha256("acme").unwrap()
    }

    fn posting(evidence: JobDiscoveryEvidence) -> JobPosting {
        JobPosting {
            id: "job-composition".to_string(),
            canonical_key: "canonical-job-composition".to_string(),
            source: "greenhouse_import".to_string(),
            external_id: "123".to_string(),
            company: "Acme".to_string(),
            title: "Platform Engineer".to_string(),
            location: "Remote".to_string(),
            workplace: "remote".to_string(),
            canonical_url: "https://boards.greenhouse.io/acme/jobs/123".to_string(),
            description: "Build reliable systems.".to_string(),
            compensation: "$150,000".to_string(),
            employment_type: "full_time".to_string(),
            track_id: "track-composition".to_string(),
            match_score: 100,
            matched_reasons: Vec::new(),
            missing_requirements: Vec::new(),
            posted_at_ms: Some(10),
            last_verified_at_ms: Some(100),
            availability_status: "active".to_string(),
            status: "matched".to_string(),
            created_at_ms: 10,
            updated_at_ms: 100,
            discovery_evidence: evidence,
            eligibility: None,
        }
    }

    fn source_binding() -> OriginalSourceIntegrityBinding {
        OriginalSourceIntegrityBinding {
            subject_sha256: digest('1'),
            source_material_sha256: digest('2'),
            source_expires_at_ms: 1_000,
            canonical_job_id: "canonical-job-composition".to_string(),
            provider_family: "greenhouse".to_string(),
            provider_record_id: "greenhouse:acme:123".to_string(),
            provider_host: "boards.greenhouse.io".to_string(),
            provider_tenant: "acme".to_string(),
            provider_job: "123".to_string(),
            provider_variant: "greenhouse_public".to_string(),
            canonical_application_url: "https://boards.greenhouse.io/acme/jobs/123".to_string(),
            application_domain: "boards.greenhouse.io".to_string(),
        }
    }

    fn source_projection() -> OriginalSourceVerificationProjection {
        let binding = source_binding();
        OriginalSourceVerificationProjection {
            feature_active: true,
            evidence: Some(JobDiscoveryEvidence {
                provenance: "original_source".to_string(),
                canonical_status: "canonical".to_string(),
                canonical_job_id: Some(binding.canonical_job_id.clone()),
                employer_verification_status: "ats_tenant_verified".to_string(),
                employer_id: None,
                canonical_employer_domain: None,
                application_domain: Some(binding.application_domain.clone()),
                scam_risk_status: "source_screened".to_string(),
                scam_signals: Vec::new(),
                original_source_status: "verified_open".to_string(),
                original_source_checked_at_ms: Some(100),
                original_source_snapshot_expires_at_ms: Some(binding.source_expires_at_ms),
                original_source_evidence_hash: Some(digest('3')),
                original_source_mismatched_fields: Vec::new(),
                requires_original_revalidation: false,
            }),
            expected_head: Some(OriginalSourceVerificationExpectedHead {
                subject_sha256: binding.subject_sha256.clone(),
                material_generation: 1,
                material_sha256: binding.source_material_sha256.clone(),
                receipt_sha256: digest('3'),
                expires_at_ms: binding.source_expires_at_ms,
                managed_authority_sha256: digest('4'),
            }),
            integrity_binding: Some(binding),
            db_time_ms: 100,
        }
    }

    fn runtime_target() -> AtsCertificationRuntimeTarget {
        AtsCertificationRuntimeTarget {
            runtime_kind: "local".to_string(),
            runtime_id: "local-macos-arm64".to_string(),
            runtime_sha256: digest('5'),
            platform: "macos".to_string(),
            architecture: "arm64".to_string(),
            automation_bundle_sha256: digest('6'),
            browser_release_manifest_sha256: Some(digest('7')),
            browser_artifact_sha256: Some(digest('8')),
            browser_build_descriptor_sha256: Some(digest('9')),
            runner_build_id: None,
            runner_image_sha256: None,
            playwright_version: "1.55.0".to_string(),
            chromium_revision: "chromium-140".to_string(),
            chromium_executable_sha256: digest('a'),
        }
    }

    fn active_ats_binding(provider: &str) -> AtsCertificationBinding {
        let runtime = runtime_target();
        AtsCertificationBinding {
            binding_version: 1,
            trust_policy_sha256: digest('b'),
            provider: provider.to_string(),
            target_key: "acme".to_string(),
            allowed_provider_hosts: vec!["boards.greenhouse.io".to_string()],
            variant_key: "greenhouse_public".to_string(),
            surface_sha256: digest('c'),
            scope_sha256: digest('d'),
            manifest_sha256: digest('e'),
            certification_id: "certification-composition".to_string(),
            manifest_generation: 1,
            activation_sha256: digest('f'),
            activation_id: "activation-composition".to_string(),
            activation_generation: 1,
            channel: "general".to_string(),
            channel_sequence: 1,
            channel_head_revision: 1,
            channel_transition_sha256: digest('0'),
            capability: "unattended_submit".to_string(),
            account_allowlist_sha256: None,
            canary_max_submissions: 0,
            canary_account_cap: 0,
            canary_concurrency_cap: 0,
            canary_daily_side_effect_cap: 0,
            adapter_version: "greenhouse-v1".to_string(),
            final_submit_control_id: "greenhouse-submit".to_string(),
            adapter_bundle_sha256: digest('1'),
            source_commit: digest('2'),
            layout_contract_version: 1,
            layout_contract_sha256: digest('3'),
            layout_set_sha256: digest('4'),
            layout_observation_sha256s: vec![digest('5')],
            evidence_sha256s: vec![digest('6')],
            runtime_targets: vec![runtime.clone()],
            selected_runtime: Some(runtime),
            not_before_ms: 1,
            expires_at_ms: 900,
            last_verified_at_ms: 90,
        }
    }

    fn ats_resolution(
        provider: &str,
        status: &str,
        active: bool,
    ) -> AtsCertificationPostingResolution {
        let binding = active.then(|| active_ats_binding(provider));
        AtsCertificationPostingResolution {
            status: AtsCertificationTargetStatusProjection {
                schema_version: 1,
                provider: provider.to_string(),
                target_key_sha256: fixture_ats_target_key_sha256(),
                status: status.to_string(),
                adapter_version: active.then(|| "greenhouse-v1".to_string()),
                manifest_sha256: active.then(|| digest('e')),
                activation_sha256: active.then(|| digest('f')),
                activation_generation: active.then_some(1),
                layout_set_sha256: active.then(|| digest('4')),
                rollout_channel: active.then(|| "general".to_string()),
                runner_kinds: if active {
                    vec!["local".to_string()]
                } else {
                    Vec::new()
                },
                runner_target_sha256s: if active {
                    vec![digest('5')]
                } else {
                    Vec::new()
                },
                expires_at_ms: active.then_some(900),
                last_verified_at_ms: active.then_some(90),
                canary_available: false,
            },
            active_binding: binding,
        }
    }

    fn authority() -> JobIntegrityCurrentAuthority {
        let binding = source_binding();
        JobIntegrityCurrentAuthority {
            subject_sha256: binding.subject_sha256,
            source_material_sha256: binding.source_material_sha256,
            attestation_sha256: digest('8'),
            attestation_generation: 1,
            head_revision: 1,
            head_transition_sha256: digest('9'),
            policy_sha256: digest('a'),
            employer_authorization_sha256: digest('b'),
            risk_authorization_sha256: digest('c'),
            canonical_employer_id: "employer-acme".to_string(),
            canonical_employer_domain: "acme.example".to_string(),
            risk_policy_sha256: digest('d'),
            effective_expires_at_ms: 800,
            canonical_job_id: binding.canonical_job_id,
            provider_family: binding.provider_family,
            provider_record_id: binding.provider_record_id,
            provider_host: binding.provider_host,
            provider_tenant: binding.provider_tenant,
            provider_job: binding.provider_job,
            provider_variant: binding.provider_variant,
            canonical_application_url: binding.canonical_application_url,
            application_domain: binding.application_domain,
            ats_tenant_binding_sha256: fixture_ats_target_key_sha256(),
        }
    }

    fn verified_resolution() -> JobIntegrityResolution {
        JobIntegrityResolution {
            status: JobIntegrityResolutionStatus::Verified,
            reason_code: "job_integrity_verified".to_string(),
            signal_codes: Vec::new(),
            authority: Some(authority()),
        }
    }

    fn application(receipt: Value) -> JobApplication {
        JobApplication {
            id: "application-composition".to_string(),
            job_id: "job-composition".to_string(),
            resume_version_id: Some("resume-composition".to_string()),
            state: "awaiting_review".to_string(),
            submission_mode: "review_first".to_string(),
            match_score: 100,
            answers: Vec::new(),
            cover_letter: String::new(),
            receipt,
            run_id: None,
            created_at_ms: 1,
            updated_at_ms: 1,
            submitted_at_ms: None,
        }
    }

    #[test]
    fn expected_source_uses_exact_phase614_binding_and_ats_target_sha256() {
        let source = source_binding();
        let ats = ats_resolution("greenhouse", "active", true);

        let expected = job_integrity_expected_source_from_composition(&source, &ats).unwrap();

        assert_eq!(expected.subject_sha256, source.subject_sha256);
        assert_eq!(
            expected.source_material_sha256,
            source.source_material_sha256
        );
        assert_eq!(expected.source_expires_at_ms, source.source_expires_at_ms);
        assert_eq!(expected.canonical_job_id, source.canonical_job_id);
        assert_eq!(expected.provider_family, source.provider_family);
        assert_eq!(expected.provider_record_id, source.provider_record_id);
        assert_eq!(expected.provider_host, source.provider_host);
        assert_eq!(expected.provider_tenant, source.provider_tenant);
        assert_eq!(expected.provider_job, source.provider_job);
        assert_eq!(expected.provider_variant, source.provider_variant);
        assert_eq!(
            expected.canonical_application_url,
            source.canonical_application_url
        );
        assert_eq!(expected.application_domain, source.application_domain);
        assert_eq!(
            expected.ats_tenant_binding_sha256,
            ats.status.target_key_sha256
        );
    }

    #[test]
    fn smartrecruiters_composition_preserves_distinct_provider_and_application_hosts() {
        let mut source = source_binding();
        source.provider_family = "smartrecruiters".to_string();
        source.provider_record_id =
            "smartrecruiters:jobs.smartrecruiters.com:acme:abc".to_string();
        source.provider_host = "jobs.smartrecruiters.com".to_string();
        source.provider_job = "abc".to_string();
        source.provider_variant = "smartrecruiters_posting".to_string();
        source.canonical_application_url =
            "https://www.smartrecruiters.com/acme/abc-platform-engineer".to_string();
        source.application_domain = "www.smartrecruiters.com".to_string();
        let ats = ats_resolution("smartrecruiters", "active", true);

        let expected = job_integrity_expected_source_from_composition(&source, &ats)
            .expect("valid Phase 614 SmartRecruiters cross-host binding composes");

        assert_eq!(expected.provider_host, "jobs.smartrecruiters.com");
        assert_eq!(expected.application_domain, "www.smartrecruiters.com");
        assert_eq!(
            expected.canonical_application_url,
            "https://www.smartrecruiters.com/acme/abc-platform-engineer"
        );
    }

    #[test]
    fn ats_lookup_uses_phase614_destination_instead_of_mutable_posting_url() {
        let mut raw = posting(JobDiscoveryEvidence::default());
        raw.source = "greenhouse_import".to_string();
        raw.canonical_url = "https://jobs.lever.co/attacker/wrong-job".to_string();
        raw.last_verified_at_ms = Some(899);
        raw.discovery_evidence.application_domain = Some("jobs.lever.co".to_string());
        let source = source_projection();
        let binding = source.integrity_binding.as_ref().unwrap();

        let projected = original_source_projected_ats_posting(&raw, &source, binding);

        assert_eq!(projected.source, binding.provider_family);
        assert_ne!(projected.source, raw.source);
        assert_eq!(projected.last_verified_at_ms, Some(100));
        assert_ne!(projected.last_verified_at_ms, raw.last_verified_at_ms);
        assert_eq!(projected.canonical_url, binding.canonical_application_url);
        assert_eq!(
            projected.discovery_evidence.application_domain.as_deref(),
            Some(binding.application_domain.as_str())
        );
        assert_ne!(projected.canonical_url, raw.canonical_url);

        raw.last_verified_at_ms = None;
        assert_eq!(
            original_source_projected_ats_posting(&raw, &source, binding).last_verified_at_ms,
            Some(100)
        );
    }

    #[test]
    fn provider_mismatch_and_inactive_ats_are_typed_non_authority() {
        let source = source_binding();
        let mismatch = ats_resolution("lever", "active", true);
        let mismatch = job_integrity_expected_source_from_composition(&source, &mismatch)
            .expect_err("provider mismatch must deny composition");
        assert_eq!(mismatch.status, JobIntegrityResolutionStatus::Mismatch);
        assert!(mismatch.authority.is_none());

        let inactive = ats_resolution("greenhouse", "review_only", false);
        let inactive = job_integrity_expected_source_from_composition(&source, &inactive)
            .expect_err("inactive ATS must deny composition");
        assert_eq!(
            inactive.status,
            JobIntegrityResolutionStatus::ReviewRequired
        );
        assert_eq!(inactive.reason_code, "ats_certification_inactive");
        assert!(inactive.authority.is_none());

        let mut target_drift = ats_resolution("greenhouse", "active", true);
        target_drift.status.target_key_sha256 = digest('7');
        let target_drift = job_integrity_expected_source_from_composition(&source, &target_drift)
            .expect_err("ATS target hash drift must deny composition");
        assert_eq!(target_drift.status, JobIntegrityResolutionStatus::Mismatch);
        assert_eq!(target_drift.reason_code, "ats_target_binding_mismatch");
        assert!(target_drift.authority.is_none());
    }

    #[test]
    fn ats_target_evidence_errors_are_typed_denials_while_storage_bubbles() {
        let mismatch = ats_composition_denial(AtsCertificationAuthorityError::ScopeMismatch)
            .expect("scope mismatch is a typed denial");
        assert_eq!(mismatch.status, JobIntegrityResolutionStatus::Mismatch);
        assert_eq!(mismatch.reason_code, "ats_source_binding_mismatch");

        let unavailable = ats_composition_denial(AtsCertificationAuthorityError::Expired)
            .expect("expired ATS authority is a typed denial");
        assert_eq!(
            unavailable.status,
            JobIntegrityResolutionStatus::ReviewRequired
        );
        assert_eq!(unavailable.reason_code, "ats_certification_not_current");

        assert!(ats_composition_denial(AtsCertificationAuthorityError::Storage(
            anyhow::anyhow!("storage unavailable"),
        ))
        .is_err());
    }

    #[test]
    fn ats_mismatch_projects_a_hard_denial_before_signed_integrity_evaluation() {
        let denial = ats_composition_denial(AtsCertificationAuthorityError::ScopeMismatch)
            .expect("scope mismatch is a typed denial");
        let mismatch = compose_job_integrity_projection(
            &posting(JobDiscoveryEvidence::default()),
            source_projection(),
            None,
            denial,
            false,
        );

        assert_eq!(
            mismatch.job_integrity.status,
            JobIntegrityResolutionStatus::Mismatch
        );
        assert_eq!(
            mismatch.discovery_evidence.employer_verification_status,
            "mismatch"
        );
        assert!(mismatch.discovery_evidence.employer_id.is_none());
        assert!(mismatch
            .discovery_evidence
            .canonical_employer_domain
            .is_none());
        assert_eq!(mismatch.discovery_evidence.scam_risk_status, "blocked");
    }

    #[test]
    fn verified_authority_overlays_identity_and_risk_but_preserves_hard_denials() {
        let normal = compose_job_integrity_projection(
            &posting(JobDiscoveryEvidence::default()),
            source_projection(),
            Some(ats_resolution("greenhouse", "active", true)),
            verified_resolution(),
            true,
        );
        assert_eq!(
            normal.discovery_evidence.employer_verification_status,
            "verified"
        );
        assert_eq!(
            normal.discovery_evidence.employer_id.as_deref(),
            Some("employer-acme")
        );
        assert_eq!(
            normal
                .discovery_evidence
                .canonical_employer_domain
                .as_deref(),
            Some("acme.example")
        );
        assert_eq!(normal.discovery_evidence.scam_risk_status, "clear");
        assert!(normal.discovery_evidence.scam_signals.is_empty());

        let hard_denials = JobDiscoveryEvidence {
            canonical_status: "invalid".to_string(),
            employer_verification_status: "impersonated".to_string(),
            employer_id: Some("rejected-employer".to_string()),
            canonical_employer_domain: Some("lookalike.example".to_string()),
            scam_risk_status: "blocked".to_string(),
            scam_signals: vec![DiscoveryScamSignal {
                code: "lookalike_domain".to_string(),
                source: "risk_engine".to_string(),
            }],
            ..JobDiscoveryEvidence::default()
        };
        let denied = compose_job_integrity_projection(
            &posting(hard_denials),
            source_projection(),
            Some(ats_resolution("greenhouse", "active", true)),
            verified_resolution(),
            true,
        );
        assert_eq!(denied.discovery_evidence.canonical_status, "invalid");
        assert_eq!(
            denied.discovery_evidence.employer_verification_status,
            "impersonated"
        );
        assert_eq!(
            denied.discovery_evidence.employer_id.as_deref(),
            Some("rejected-employer")
        );
        assert_eq!(
            denied
                .discovery_evidence
                .canonical_employer_domain
                .as_deref(),
            Some("lookalike.example")
        );
        assert_eq!(denied.discovery_evidence.scam_risk_status, "blocked");
        assert_eq!(denied.discovery_evidence.scam_signals.len(), 1);
    }

    #[test]
    fn signed_blocked_and_mismatch_project_hard_denials_without_weakening_existing_ones() {
        let blocked_resolution = JobIntegrityResolution {
            status: JobIntegrityResolutionStatus::Blocked,
            reason_code: "job_risk_blocked".to_string(),
            signal_codes: vec!["known_scam_signal".to_string()],
            authority: None,
        };
        let blocked = compose_job_integrity_projection(
            &posting(JobDiscoveryEvidence::default()),
            source_projection(),
            Some(ats_resolution("greenhouse", "active", true)),
            blocked_resolution,
            true,
        );
        assert_eq!(blocked.discovery_evidence.scam_risk_status, "blocked");
        assert!(blocked
            .discovery_evidence
            .scam_signals
            .iter()
            .any(|signal| {
                signal.code == "known_scam_signal" && signal.source == "job_integrity_authority"
            }));

        let mismatch_resolution = JobIntegrityResolution {
            status: JobIntegrityResolutionStatus::Mismatch,
            reason_code: "employer_identity_mismatch".to_string(),
            signal_codes: vec!["employer_identity_mismatch".to_string()],
            authority: None,
        };
        let mismatch = compose_job_integrity_projection(
            &posting(JobDiscoveryEvidence::default()),
            source_projection(),
            Some(ats_resolution("greenhouse", "active", true)),
            mismatch_resolution.clone(),
            true,
        );
        assert_eq!(
            mismatch.discovery_evidence.employer_verification_status,
            "mismatch"
        );
        assert!(mismatch.discovery_evidence.employer_id.is_none());
        assert!(mismatch
            .discovery_evidence
            .canonical_employer_domain
            .is_none());
        assert_eq!(mismatch.discovery_evidence.scam_risk_status, "blocked");

        let stronger = JobDiscoveryEvidence {
            employer_verification_status: "impersonated".to_string(),
            scam_risk_status: "blocked".to_string(),
            scam_signals: vec![DiscoveryScamSignal {
                code: "lookalike_domain".to_string(),
                source: "risk_engine".to_string(),
            }],
            ..JobDiscoveryEvidence::default()
        };
        let stronger = compose_job_integrity_projection(
            &posting(stronger),
            source_projection(),
            Some(ats_resolution("greenhouse", "active", true)),
            mismatch_resolution,
            true,
        );
        assert_eq!(
            stronger.discovery_evidence.employer_verification_status,
            "impersonated"
        );
        assert_eq!(stronger.discovery_evidence.scam_risk_status, "blocked");
        assert!(stronger
            .discovery_evidence
            .scam_signals
            .iter()
            .any(|signal| signal.code == "lookalike_domain"));
        assert!(stronger
            .discovery_evidence
            .scam_signals
            .iter()
            .any(|signal| {
                signal.code == "employer_identity_mismatch"
                    && signal.source == "job_integrity_authority"
            }));
    }

    #[test]
    fn receipt_is_exact_strict_and_drift_sensitive() {
        let current = verified_resolution();
        let receipt = job_integrity_receipt_projection(current.authority.as_ref().unwrap());
        let value = serde_json::to_value(&receipt).unwrap();
        let keys = value
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<BTreeSet<_>>();
        assert_eq!(
            keys,
            BTreeSet::from([
                "attestationGeneration",
                "attestationSha256",
                "canonicalEmployerDomain",
                "canonicalEmployerId",
                "employerIdentityAuthorizationSha256",
                "expiresAtMs",
                "headRevision",
                "headTransitionSha256",
                "jobRiskAuthorizationSha256",
                "policySha256",
                "riskPolicySha256",
                "sourceMaterialSha256",
                "subjectSha256",
            ])
        );
        let exact_application = application(json!({"job_integrity": value.clone()}));
        assert!(
            job_integrity_resolution_matches_application(&exact_application, &current).unwrap()
        );

        let missing = application(json!({}));
        assert!(!job_integrity_resolution_matches_application(&missing, &current).unwrap());

        let mut drifted = current.clone();
        drifted.authority.as_mut().unwrap().head_revision += 1;
        assert!(
            !job_integrity_resolution_matches_application(&exact_application, &drifted).unwrap()
        );

        let mut legacy = value;
        let object = legacy.as_object_mut().unwrap();
        let employer = object
            .remove("employerIdentityAuthorizationSha256")
            .unwrap();
        let risk = object.remove("jobRiskAuthorizationSha256").unwrap();
        object.insert("employerAuthorizationSha256".to_string(), employer);
        object.insert("riskAuthorizationSha256".to_string(), risk);
        let legacy_application = application(json!({"job_integrity": legacy}));
        assert!(application_job_integrity_receipt(&legacy_application).is_err());

        let mut negative = current;
        negative.status = JobIntegrityResolutionStatus::Blocked;
        negative.reason_code = "job_risk_blocked".to_string();
        assert!(
            !job_integrity_resolution_matches_application(&exact_application, &negative).unwrap()
        );
    }

    #[test]
    fn sqlite_absent_source_remains_review_first_without_ats_or_integrity_authority() {
        let database_path = std::env::temp_dir().join(format!(
            "bluey-job-integrity-composition-{}-{}.sqlite3",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let pool = db::open_pool(&database_path).unwrap();
        db::run_migrations(&pool).unwrap();
        let mut connection = pool.get().unwrap();
        let tx = connection
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .unwrap();

        let projection = resolve_composed_job_integrity_projection_sqlite_tx(
            &tx,
            "acct-composition",
            &posting(JobDiscoveryEvidence::default()),
        )
        .unwrap();
        tx.commit().unwrap();

        assert!(!projection.original_source.feature_active);
        assert!(projection.ats_certification.is_none());
        assert_eq!(
            projection.job_integrity.status,
            JobIntegrityResolutionStatus::ReviewRequired
        );
        assert_eq!(
            projection.job_integrity.reason_code,
            "original_source_verification_inactive"
        );
        assert!(projection.job_integrity.authority.is_none());

        drop(connection);
        drop(pool);
        let _ = std::fs::remove_file(&database_path);
        let _ = std::fs::remove_file(database_path.with_extension("sqlite3-wal"));
        let _ = std::fs::remove_file(database_path.with_extension("sqlite3-shm"));
    }

    #[test]
    fn postgres_composition_static_order_is_h_m_ats_d_then_source_ats_integrity() {
        fn section<'a>(source: &'a str, start: &str, end: &str) -> &'a str {
            source
                .split_once(start)
                .unwrap_or_else(|| panic!("missing section start {start}"))
                .1
                .split_once(end)
                .unwrap_or_else(|| panic!("missing section end {end}"))
                .0
        }

        fn assert_ordered(source: &str, needles: &[&str]) {
            let mut prior = 0;
            for needle in needles {
                let position = source
                    .find(needle)
                    .unwrap_or_else(|| panic!("missing ordered operation {needle}"));
                assert!(position >= prior, "operation order inverted at {needle}");
                prior = position;
            }
        }

        let composition = include_str!("job_integrity_composition.rs");
        let prelocked = section(
            composition,
            "pub(crate) fn resolve_composed_job_integrity_projection_postgres_tx_after_prelock(",
            "pub fn resolve_composed_job_integrity_projection(",
        );
        assert_ordered(
            prelocked,
            &[
                "lock_job_integrity_publication_fence_shared_postgres_tx",
                "original_source_db_now_postgres",
                "resolve_original_source_verification_projection_postgres_tx_after_prelock_at_ms",
                "resolve_ats_certification_for_posting_postgres_tx",
                "resolve_current_job_integrity_authority_postgres_tx_after_publication_fence_at_ms",
            ],
        );
        assert!(!prelocked.contains("integrity_time_ms: Option<i64>"));
        let public = section(
            composition,
            "pub fn resolve_composed_job_integrity_projection(",
            "/// Resolve an administrative status from server-owned posting",
        );
        let status_boundary = section(
            composition,
            "pub fn resolve_composed_job_integrity_projection_for_posting(",
            "#[cfg(test)]",
        );
        for boundary in [public, status_boundary] {
            assert_ordered(
                boundary,
                &[
                    "lock_operational_hold_shared_postgres_tx",
                    "lock_managed_cloud_release_registry_shared_postgres_tx",
                    "lock_postgres_ats_certification",
                    "lock_discovery_account_shared_postgres",
                    "resolve_composed_job_integrity_projection_postgres_tx_after_prelock",
                ],
            );
            assert_eq!(
                boundary.matches("lock_postgres_ats_certification").count(),
                1,
                "each public composition boundary must take the ATS fence exactly once",
            );
            assert!(!boundary.contains("read_only(true)"));
            assert!(!boundary.contains("IsolationLevel::RepeatableRead"));
        }

        let managed = include_str!("managed_cloud_release_authority.rs");
        let managed_after_prelock = section(
            managed,
            "pub(crate) fn postgres_original_source_verification_authority_for_account_tx_after_prelock(",
            "pub(crate) fn postgres_original_source_verification_feature_active_tx(",
        );
        assert!(!managed_after_prelock
            .contains("lock_managed_cloud_release_registry_shared_postgres_tx"));
    }

    #[test]
    fn postgres_lock_first_read_committed_observes_waited_writer_when_configured() {
        let Ok(database_url) = std::env::var("BLUEY_TEST_POSTGRES_URL") else {
            eprintln!(
                "skipped PostgreSQL composition snapshot test: \
                 BLUEY_TEST_POSTGRES_URL is unavailable"
            );
            return;
        };
        let pool = crate::db::open_postgres_pool(&database_url)
            .expect("open PostgreSQL composition snapshot pool");
        crate::db::run_migrations(&pool).expect("migrate PostgreSQL composition snapshot pool");
        let suffix = uuid::Uuid::new_v4().simple().to_string();
        let account_id = format!("acct-composition-snapshot-{suffix}");
        let before_email = format!("before-{suffix}@example.com");
        let after_email = format!("after-{suffix}@example.com");
        pool.get_pg()
            .expect("PostgreSQL composition snapshot seed connection")
            .execute(
                "INSERT INTO accounts (id,email,password_hash,trial_seconds_remaining)
                 VALUES ($1,$2,'hash',0)",
                &[&account_id, &before_email],
            )
            .expect("insert PostgreSQL composition snapshot account");

        let result = (|| -> Result<()> {
            let mut writer_connection = pool.get_pg()?;
            let mut writer = writer_connection.transaction()?;
            writer.query_one(POSTGRES_OPERATIONAL_HOLD_EXCLUSIVE_LOCK_SQL, &[])?;
            writer.execute(
                "UPDATE accounts SET email=$2 WHERE id=$1",
                &[&account_id, &after_email],
            )?;

            let reader_pool = pool.clone();
            let reader_account_id = account_id.clone();
            let (started_tx, started_rx) = std::sync::mpsc::channel();
            let reader = std::thread::spawn(move || -> Result<String> {
                let mut reader_connection = reader_pool.get_pg()?;
                let mut reader_tx = reader_connection.transaction()?;
                started_tx
                    .send(())
                    .map_err(|_| anyhow::anyhow!("signal PostgreSQL composition reader"))?;
                lock_operational_hold_shared_postgres_tx(&mut reader_tx)
                    .map_err(anyhow::Error::new)?;
                let email = reader_tx
                    .query_one(
                        "SELECT email FROM accounts WHERE id=$1",
                        &[&reader_account_id],
                    )?
                    .get::<_, String>(0);
                reader_tx.commit()?;
                Ok(email)
            });
            started_rx
                .recv()
                .map_err(|_| anyhow::anyhow!("wait for PostgreSQL composition reader"))?;
            std::thread::sleep(std::time::Duration::from_millis(100));
            writer.commit()?;
            let observed = reader
                .join()
                .map_err(|_| anyhow::anyhow!("PostgreSQL composition reader panicked"))??;
            anyhow::ensure!(
                observed == after_email,
                "lock-first READ COMMITTED reader retained a stale pre-publication snapshot"
            );
            Ok(())
        })();

        pool.get_pg()
            .expect("PostgreSQL composition snapshot cleanup connection")
            .execute("DELETE FROM accounts WHERE id=$1", &[&account_id])
            .expect("delete PostgreSQL composition snapshot account");
        result.expect("exercise PostgreSQL composition lock-first snapshot");
    }
}
