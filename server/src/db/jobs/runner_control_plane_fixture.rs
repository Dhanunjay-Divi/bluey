// Debug/integration-feature-only protocol fixtures. Synthetic runtime/storage evidence is not
// a measurement, deployment, browser execution, Temporal acceptance, or employer submission.
use super::*;

#[derive(Clone, Copy, PartialEq, Eq)]
enum FixtureMode {
    ReviewFirst,
    AutoSubmit,
}

impl FixtureMode {
    fn from_submission_mode(value: &str) -> Result<Self> {
        match value {
            "review_first" => Ok(Self::ReviewFirst),
            "auto_submit" => Ok(Self::AutoSubmit),
            _ => anyhow::bail!("fixture checkpoint submission mode"),
        }
    }

    fn bootstrap_operation(self) -> &'static str {
        match self {
            Self::ReviewFirst => "bootstrap",
            Self::AutoSubmit => "bootstrap_auto_submit",
        }
    }

    fn post_queue_operation(self) -> &'static str {
        match self {
            Self::ReviewFirst => "post_queue",
            Self::AutoSubmit => "post_queue_auto_submit",
        }
    }

    fn submission_mode(self) -> &'static str {
        match self {
            Self::ReviewFirst => "review_first",
            Self::AutoSubmit => "auto_submit",
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunnerControlPlaneBootstrapRequest {
    pub version: u8,
    pub operation: String,
    pub account_id: String,
    pub track_id: String,
    pub worker_id: String,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RunnerControlPlaneVolume {
    pub volume_id: String,
    pub enrollment_epoch: i64,
    pub process_instance_id: String,
    pub key_fingerprint: String,
    pub runtime_grant_id: String,
    pub runtime_sha256: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunnerControlPlanePrivateVolumeKey {
    pub version: u8,
    pub worker_id: String,
    pub volume: RunnerControlPlaneVolume,
    pub public_key_base64url: String,
    pub signing_seed_base64url: String,
}

// Match the SDK's closed camelCase claim descriptor without changing the production type.
#[derive(Serialize, Deserialize)]
#[serde(
    remote = "ManagedCloudRuntimeInstance",
    rename_all = "camelCase",
    deny_unknown_fields
)]
struct RuntimeInstanceWire {
    grant_id: String,
    runtime_instance_id: String,
    runtime_identity_sha256: String,
    worker_id: String,
    scope: ManagedCloudScope,
    activation_sha256: String,
    manifest_sha256: String,
    component_id: String,
    role: String,
    head_revision: i64,
    transition_sha256: String,
    artifact_sha256: String,
    config_schema_sha256: String,
    migration_set_sha256: String,
    protocol_set_sha256: String,
    task_queue_sha256: String,
    failure_converter_sha256: String,
    dependency_evidence_sha256: String,
    activation_expires_at_ms: i64,
    instance_epoch: i64,
    next_heartbeat_sequence: i64,
    claimed_at_ms: i64,
    replayed: bool,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunnerControlPlaneRuntimeCredentials {
    #[serde(with = "RuntimeInstanceWire")]
    pub instance: ManagedCloudRuntimeInstance,
    pub session_token: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunnerControlPlaneBootstrapResult {
    pub version: u8,
    pub operation: String,
    pub account_id: String,
    pub track_id: String,
    pub job_id: String,
    pub managed_release_fixture_version: u8,
    pub scope: ManagedCloudScope,
    pub volume: RunnerControlPlaneVolume,
    pub runtimes: Vec<RunnerControlPlaneRuntimeCredentials>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunnerControlPlanePostQueueRequest {
    pub version: u8,
    pub operation: String,
    pub account_id: String,
    pub application_id: String,
    pub run_id: String,
    pub browser_session_id: String,
    pub browser_profile_id: String,
    pub command_id: String,
    pub request_id: String,
    pub workflow_id: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunnerControlPlanePostQueueResult {
    pub version: u8,
    pub operation: String,
    pub account_id: String,
    pub application_id: String,
    pub run_id: String,
    pub browser_session_id: String,
    pub browser_profile_id: String,
    pub command_id: String,
    pub request_id: String,
    pub workflow_id: String,
    pub submission_mode: String,
    pub managed_cloud_release: ManagedCloudReleaseMemoAuthority,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_summary: Option<RunnerControlPlaneAutoSummary>,
}

#[derive(Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RunnerControlPlaneAutoSummary {
    pub attempt_id: String,
    pub auto_authorization_id: String,
    pub auto_authorization_revision: i64,
    pub auto_authorization_fingerprint_sha256: String,
    pub packet_checksum_sha256: String,
    pub provider: String,
    pub target_key_sha256: String,
    pub manifest_sha256: String,
    pub activation_sha256: String,
    pub layout_set_sha256: String,
    pub adapter_bundle_sha256: String,
    pub runner_target_sha256: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunnerControlPlaneInspectAutoSubmitClaimRequest {
    pub version: u8,
    pub operation: String,
    pub account_id: String,
    pub application_id: String,
    pub run_id: String,
    pub browser_session_id: String,
    pub browser_profile_id: String,
    pub lease_token_sha256: String,
    pub lease_fence: i64,
}

#[derive(Serialize)]
pub struct RunnerControlPlaneInspectAutoSubmitClaimResult {
    pub version: u8,
    pub operation: &'static str,
    pub account_id: String,
    pub application_id: String,
    pub run_id: String,
    pub browser_session_id: String,
    pub browser_profile_id: String,
    pub lease_token_sha256: String,
    pub lease_fence: i64,
    pub ats_account_count: i64,
    pub ats_application_count: i64,
    pub ats_run_count: i64,
    pub binding_sha256: String,
    pub nonce_sha256: String,
    pub phase: String,
    pub binding_fence: i64,
    pub expires_future: bool,
    pub exact_frozen_authority: bool,
    pub exact_prepared_lease: bool,
    pub no_phase_b_or_invalidation: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RunnerControlPlaneCheckpointStage {
    BeforeRecovery,
    AfterRecovery,
    AfterExactReplay,
}

impl RunnerControlPlaneCheckpointStage {
    fn is_before(self) -> bool {
        self == Self::BeforeRecovery
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunnerControlPlaneInspectCheckpointRecoveryRequest {
    pub version: u8,
    pub operation: String,
    pub stage: RunnerControlPlaneCheckpointStage,
    pub account_id: String,
    pub application_id: String,
    pub run_id: String,
    pub browser_session_id: String,
    pub browser_profile_id: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RunnerControlPlaneCloudRecoverySummary {
    pub schema_version: i64,
    pub status: String,
    pub checkpoint_version: i64,
    pub checkpoint_phase: String,
    pub lease_phase: String,
    pub recorded_at_ms: i64,
    pub run_id: String,
}

#[derive(Serialize)]
pub struct RunnerControlPlaneInspectCheckpointRecoveryResult {
    pub version: u8,
    pub operation: &'static str,
    pub stage: RunnerControlPlaneCheckpointStage,
    pub account_id: String,
    pub application_id: String,
    pub run_id: String,
    pub browser_session_id: String,
    pub browser_profile_id: String,
    pub submission_mode: String,
    pub application_stable_sha256: String,
    pub session_stable_sha256: String,
    pub application_state: String,
    pub session_status: String,
    pub session_current_step: String,
    pub submitted_absent: bool,
    pub takeover_absent: bool,
    pub lease_phase: String,
    pub attempt_status: String,
    pub cloud_recovery: Option<RunnerControlPlaneCloudRecoverySummary>,
    pub cloud_recovery_sha256: Option<String>,
}

fn fixture_scope() -> ManagedCloudScope {
    ManagedCloudScope {
        environment: "staging".into(),
        region: "us-east-1".into(),
        channel: "general".into(),
    }
}

fn digest(value: &str) -> String {
    hex::encode(Sha256::digest(value.as_bytes()))
}

fn bounded_id(value: &str, minimum: usize, maximum: usize) -> bool {
    (minimum..=maximum).contains(&value.len())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

fn clock(pool: &DbPool) -> Result<i64> {
    Ok(pool
        .get_pg()?
        .query_one(
            "SELECT floor(extract(epoch FROM clock_timestamp()) * 1000)::bigint",
            &[],
        )?
        .get(0))
}

fn synthetic_account_id(value: &str) -> bool {
    value.strip_prefix("ui_fixture_").is_some_and(|suffix| {
        uuid::Uuid::parse_str(suffix)
            .is_ok_and(|id| id.get_version_num() == 4 && id.to_string() == suffix)
    })
}

fn runtime_model(label: &str) -> RunnerProcessRuntimeAttestation {
    RunnerProcessRuntimeAttestation {
        runner_image_sha256: digest(&format!("synthetic-image:{label}")),
        // Canonical protocol build token, not a claim that this build/image was executed.
        runner_build_id: "runner-638.0".into(),
        platform: "linux".into(),
        architecture: "x86_64".into(),
        automation_bundle_sha256: digest(&format!("synthetic-automation:{label}")),
        playwright_version: "1.61.1".into(),
        chromium_revision: "1228".into(),
        chromium_executable_sha256: digest(&format!("synthetic-chromium:{label}")),
    }
}

fn runtime_target(
    runtime: &RunnerProcessRuntimeAttestation,
) -> Result<AtsCertificationRuntimeTarget> {
    Ok(AtsCertificationRuntimeTarget {
        runtime_kind: "cloud".into(),
        runtime_id: format!("cloud:{}", runtime.runner_build_id),
        runtime_sha256: runner_process_runtime_sha256(runtime)?,
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
    })
}

pub fn bootstrap(
    pool: &DbPool,
    request: &RunnerControlPlaneBootstrapRequest,
    volume_seed: [u8; 32],
) -> Result<(
    RunnerControlPlaneBootstrapResult,
    RunnerControlPlanePrivateVolumeKey,
)> {
    bootstrap_with_mode(pool, request, volume_seed, FixtureMode::ReviewFirst)
}

pub fn bootstrap_auto_submit(
    pool: &DbPool,
    request: &RunnerControlPlaneBootstrapRequest,
    volume_seed: [u8; 32],
) -> Result<(
    RunnerControlPlaneBootstrapResult,
    RunnerControlPlanePrivateVolumeKey,
)> {
    bootstrap_with_mode(pool, request, volume_seed, FixtureMode::AutoSubmit)
}

fn bootstrap_with_mode(
    pool: &DbPool,
    request: &RunnerControlPlaneBootstrapRequest,
    volume_seed: [u8; 32],
    mode: FixtureMode,
) -> Result<(
    RunnerControlPlaneBootstrapResult,
    RunnerControlPlanePrivateVolumeKey,
)> {
    anyhow::ensure!(
        matches!(pool, DbPool::Postgres(_)),
        "fixture requires private PostgreSQL"
    );
    anyhow::ensure!(
        request.version == 1 && request.operation == mode.bootstrap_operation(),
        "fixture request version"
    );
    anyhow::ensure!(
        synthetic_account_id(&request.account_id)
            && bounded_id(&request.track_id, 1, 120)
            && bounded_id(&request.worker_id, 20, 128),
        "fixture identifiers"
    );
    let account = crate::db::accounts::Account::fetch_by_id(pool, &request.account_id)?
        .context("fixture account missing")?;
    anyhow::ensure!(
        account.email_verified_at.is_some() && account.email.ends_with("@example.test"),
        "fixture requires synthetic verified account"
    );
    let saved = pool.get_pg()?.query_one(
        "SELECT EXISTS(SELECT 1 FROM jobs_profiles WHERE account_id=$1),
         EXISTS(SELECT 1 FROM jobs_preferences WHERE account_id=$1)",
        &[&account.id],
    )?;
    anyhow::ensure!(
        saved.get::<_, bool>(0) && saved.get::<_, bool>(1),
        "fixture requires saved candidate"
    );
    let profile = get_profile(pool, &account.id, &account.email)?;
    let preferences = get_preferences(pool, &account.id)?;
    anyhow::ensure!(
        profile.onboarding_complete,
        "fixture requires saved onboarding"
    );
    let track = list_tracks(pool, &account.id)?
        .into_iter()
        .find(|track| track.id == request.track_id && track.active)
        .context("fixture track missing")?;
    let identity_id = track
        .application_identity_id
        .as_ref()
        .context("fixture track identity missing")?;
    anyhow::ensure!(
        list_application_identities(pool, &account.id)?
            .iter()
            .any(|identity| identity.id == *identity_id
                && identity.verification_status == "verified"),
        "fixture track requires verified identity"
    );
    anyhow::ensure!(
        list_applications(pool, &account.id)?.is_empty()
            && list_browser_sessions(pool, &account.id)?.is_empty()
            && list_attempt_reservations(pool, &account.id)?.is_empty(),
        "fixture account is not fresh"
    );
    let fleet = runner_volume_fleet_status(pool)?;
    anyhow::ensure!(
        fleet.non_destroyed_volume_count == 0
            && fleet.destruction_count == 0
            && fleet.unresolved_legacy_volume_count == 0,
        "fixture fleet is not fresh"
    );
    let label = uuid::Uuid::new_v4().simple().to_string();
    let runtime = runtime_model(&label);
    let target = runtime_target(&runtime)?;
    validate_ats_runtime_target(&target)?;
    let plan = set_entitlement_plan(pool, &account.id, "cloud")
        .context("fixture_bootstrap_entitlement")?;
    anyhow::ensure!(
        plan.cloud_browser && plan.used_packets == 0,
        "fixture cloud entitlement"
    );
    let now = clock(pool)?;
    let mut posting = JobPosting {
        id: String::new(), canonical_key: String::new(), source: "greenhouse".into(),
        external_id: format!("phase638-{label}"), company: "Synthetic Protocol Employer".into(),
        title: "Software Engineer".into(), location: "New York, NY".into(), workplace: "hybrid".into(),
        canonical_url: format!("https://boards.greenhouse.io/blueynonprod/jobs/phase638-{label}"),
        description: "Full-time W2 employment. Hybrid work in New York, NY. Visa sponsorship available. Build reliable software systems with Rust and TypeScript.".into(),
        compensation: "$170k-$200k".into(), employment_type: "full_time w2".into(),
        track_id: track.id, match_score: 99, matched_reasons: Vec::new(), missing_requirements: Vec::new(),
        posted_at_ms: Some(now), last_verified_at_ms: Some(now), availability_status: "active".into(),
        status: "matched".into(), created_at_ms: 0, updated_at_ms: 0,
        discovery_evidence: JobDiscoveryEvidence::default(), eligibility: None,
    };
    if mode == FixtureMode::AutoSubmit {
        // Auto-only synthetic facts must match the caller's uploaded source/profile.
        // This adds the third skill needed to meet the unchanged default score of 80.
        posting.description = posting
            .description
            .replace("Rust and TypeScript", "Rust, TypeScript, and PostgreSQL");
    }
    posting.canonical_key = canonical_job_key(&posting);
    // Existing source-verifier fixture retains its historical synthetic production scope ONLY
    // inside the physically guarded private DB. New runner/admission scope is always staging.
    let (posting, source_runtime) =
        production_positive_authority_fixture::save_production_positive_verified_import(
            pool,
            &account.id,
            &posting,
            &profile,
            &preferences,
        );
    let positive = production_positive_authority_fixture::install_production_positive_job_authorities_with_runtime_surface(
        pool, &account.id, &posting, &source_runtime, "phase638-employer.example.test",
        &format!("phase638-{label}"), target,
        AtsObservedSurface { variant_key: "greenhouse_public".into(), layout_contract_version: 1,
            surface_sha256: digest(&format!("synthetic-surface:{label}")) },
    );
    anyhow::ensure!(
        positive.composed.job_integrity.status == JobIntegrityResolutionStatus::Verified,
        "fixture signed job authority unavailable"
    );
    anyhow::ensure!(
        evaluate_job_eligibility(pool, &account.id, &posting, false, None)
            .context("fixture_bootstrap_eligibility")?
            .can_prepare,
        "fixture signed posting not preparable"
    );
    if mode == FixtureMode::AutoSubmit {
        let eligibility = evaluate_job_eligibility(pool, &account.id, &posting, false, None)
            .context("fixture_bootstrap_auto_eligibility")?;
        anyhow::ensure!(
            eligibility.can_auto_submit
                && eligibility.can_queue_cloud
                && eligibility.hard_failures.is_empty()
                && eligibility.review_reasons.is_empty()
                && posting.missing_requirements.is_empty()
                && posting.match_score >= profile.auto_submit_threshold,
            "fixture auto-submit posting not eligible"
        );
    }
    let scope = fixture_scope();
    let managed =
        managed_cloud_release_authority_tests::install_managed_runner_runtime_test_fixture(
            pool,
            &request.worker_id,
            scope.clone(),
            source_runtime.now_ms,
        );
    let runtimes: Vec<_> = managed
        .runtime_credentials
        .into_iter()
        .map(
            |(instance, session_token)| RunnerControlPlaneRuntimeCredentials {
                instance,
                session_token,
            },
        )
        .collect();
    validate_runtime_set(&runtimes, &request.worker_id)
        .context("fixture_bootstrap_runtime_descriptors")?;
    let (volume, private_key) =
        provision_volume(pool, &request.worker_id, volume_seed, runtime, &label)
            .context("fixture_bootstrap_volume")?;
    let readiness = resolve_managed_cloud_readiness(
        pool,
        &ManagedCloudReadinessQuery {
            scope: scope.clone(),
            account_id: Some(account.id.clone()),
        },
    )
    .context("fixture_bootstrap_readiness")?;
    anyhow::ensure!(
        readiness.status.customer_admission
            && readiness.missing_roles.is_empty()
            && readiness.stale_roles.is_empty(),
        "fixture managed readiness unavailable"
    );
    Ok((
        RunnerControlPlaneBootstrapResult {
            version: 1,
            operation: mode.bootstrap_operation().into(),
            account_id: account.id,
            track_id: request.track_id.clone(),
            job_id: posting.id,
            managed_release_fixture_version: 2,
            scope,
            volume,
            runtimes,
        },
        private_key,
    ))
}

fn validate_runtime_set(
    runtimes: &[RunnerControlPlaneRuntimeCredentials],
    worker_id: &str,
) -> Result<()> {
    let expected: BTreeSet<_> = [
        "jobs_api",
        "managed_runner",
        "original_source_verifier",
        "workflow_cleanup_dispatcher",
        "workflow_command_dispatcher",
        "workflow_gateway",
        "workflow_worker",
    ]
    .into_iter()
    .collect();
    let actual: BTreeSet<_> = runtimes
        .iter()
        .map(|entry| entry.instance.role.as_str())
        .collect();
    anyhow::ensure!(
        runtimes.len() == 7 && actual == expected,
        "fixture requires exactly seven runtime roles"
    );
    for entry in runtimes {
        anyhow::ensure!(
            entry.instance.scope == fixture_scope()
                && entry.instance.next_heartbeat_sequence >= 2
                && decode_base64url_exact(&entry.session_token, 32).is_ok(),
            "fixture runtime descriptor"
        );
        if entry.instance.role == "managed_runner" {
            anyhow::ensure!(
                entry.instance.worker_id == worker_id,
                "fixture managed worker binding"
            );
        }
    }
    Ok(())
}

fn random_token() -> String {
    use rand::RngCore;
    let mut bytes = [0_u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    encode_base64url(&bytes)
}

fn provision_volume(
    pool: &DbPool,
    worker_id: &str,
    seed: [u8; 32],
    runtime: RunnerProcessRuntimeAttestation,
    label: &str,
) -> Result<(RunnerControlPlaneVolume, RunnerControlPlanePrivateVolumeKey)> {
    let signing_key = Ed25519SigningKey::from_bytes(&seed);
    let public_key_base64url = encode_base64url(signing_key.verifying_key().as_bytes());
    let volume_id = runner_volume_id_from_public_key(&public_key_base64url)?;
    let key_fingerprint = runner_volume_key_fingerprint(&public_key_base64url)?;
    let process_instance_id = random_token();
    let provider_resource_id = format!("synthetic-protocol-resource-{label}");
    let resource_fingerprint = digest(&provider_resource_id);
    let token = random_token();
    let now = clock(pool)?;
    let expires_at_ms = now.checked_add(900_000).context("fixture clock overflow")?;
    let grant = NewRunnerVolumeAdmissionGrant {
        grant_id: format!("volume-grant-{label}"),
        token: token.clone(),
        expected_worker_id: worker_id.into(),
        provider: "test-provider".into(),
        provider_resource_id: provider_resource_id.clone(),
        resource_fingerprint: resource_fingerprint.clone(),
        authorization_ref: format!("synthetic-protocol-{label}"),
        created_by: "phase638-protocol-fixture".into(),
        expires_at_ms,
        created_at_ms: now,
    };
    create_runner_volume_admission_grant(pool, &grant).context("fixture_volume_admission")?;
    let proof = sign_runner_volume_enrollment_proof(
        &signing_key,
        NewRunnerVolumeEnrollmentProof {
            admission_grant_id: grant.grant_id,
            volume_id: volume_id.clone(),
            worker_id: worker_id.into(),
            provider: grant.provider,
            provider_resource_id,
            resource_fingerprint: resource_fingerprint.clone(),
            enrollment_epoch: 1,
            public_key_base64url: public_key_base64url.clone(),
            key_fingerprint: key_fingerprint.clone(),
            legacy_artifact_count: 0,
            requested_at_ms: now,
        },
    )?;
    enroll_runner_volume(
        pool,
        &EnrollRunnerVolumeRequest {
            grant_token: token,
            proof,
            enrolled_at_ms: now,
        },
    )
    .context("fixture_volume_enrollment")?;
    let runtime_grant_id = format!("runtime-grant-{label}");
    let runtime_token = random_token();
    let runtime_sha256 = runner_process_runtime_sha256(&runtime)?;
    let runtime_grant = create_runner_process_runtime_grant(
        pool,
        &NewRunnerProcessRuntimeGrant {
            grant_id: runtime_grant_id.clone(),
            token: runtime_token.clone(),
            expected_worker_id: worker_id.into(),
            runtime: runtime.clone(),
            authorization_ref: format!("synthetic-runtime-{label}"),
            created_by: "phase638-protocol-fixture".into(),
            expires_at_ms,
            created_at_ms: now,
        },
    )
    .context("fixture_volume_runtime_grant")?;
    anyhow::ensure!(
        runtime_grant.runtime_sha256 == runtime_sha256,
        "fixture runtime grant binding"
    );
    let path = format!("/api/jobs/internal/runner-volumes/{volume_id}/instances/claim");
    let token_sha256 = digest(&runtime_token);
    let payload = crate::api::jobs_runner_volumes::runner_volume_http_payload_sha256(
        &path,
        worker_id,
        &[
            ("runtime_grant_id", runtime_grant_id.as_str()),
            ("runtime_grant_token_sha256", token_sha256.as_str()),
            ("runtime_sha256", runtime_sha256.as_str()),
        ],
    );
    let proof = sign_runner_volume_authority_proof(
        &signing_key,
        NewRunnerVolumeAuthorityProof {
            operation: "instance_claim".into(),
            request_id: format!("instance-claim-{label}"),
            volume_id: volume_id.clone(),
            enrollment_epoch: 1,
            process_instance_id: process_instance_id.clone(),
            issued_at_ms: now,
            payload_sha256: payload.clone(),
        },
    )?;
    let authority = verify_runner_volume_authority_proof(
        pool,
        &proof,
        "instance_claim",
        &payload,
        now,
        90_000,
    )?;
    let claimed = claim_runner_volume_instance_authorized(
        pool,
        &RunnerVolumeInstanceLeaseRequest {
            volume_id: &volume_id,
            enrollment_epoch: 1,
            process_instance_id: &process_instance_id,
            now_ms: now,
            lease_expires_at_ms: expires_at_ms,
        },
        &RunnerProcessRuntimeGrantClaim {
            grant_id: runtime_grant_id.clone(),
            grant_token: runtime_token,
            runtime: runtime.clone(),
        },
        &authority,
    )
    .context("fixture_volume_instance_claim")?;
    anyhow::ensure!(
        claimed.runtime_grant_id.as_deref() == Some(runtime_grant_id.as_str())
            && claimed.runtime_sha256.as_deref() == Some(runtime_sha256.as_str()),
        "fixture runtime claim binding"
    );
    let current =
        lookup_runner_volume(pool, &volume_id)?.context("fixture enrolled volume missing")?;
    let predecessor = runner_volume_storage_attestation_predecessor(
        pool,
        &volume_id,
        1,
        &process_instance_id,
        now,
    )?;
    // Empty synthetic storage authority, not evidence from a filesystem or a VM.
    let attestation = sign_runner_volume_storage_attestation(
        &signing_key,
        NewRunnerVolumeStorageAttestation {
            attestation_id: format!("synthetic-storage-{label}"),
            volume_id: volume_id.clone(),
            volume_key_fingerprint: key_fingerprint.clone(),
            resource_fingerprint,
            enrollment_epoch: 1,
            enrollment_generation: current.enrollment_generation,
            process_instance_id: process_instance_id.clone(),
            predecessor_attestation_generation: predecessor.predecessor_attestation_generation,
            predecessor_attestation_sha256: predecessor.predecessor_attestation_sha256,
            required_tombstone_generation: current.required_tombstone_generation,
            reconciled_tombstone_generation: current.reconciled_tombstone_generation,
            storage_evidence_version: RUNNER_PURGE_STORAGE_EVIDENCE_VERSION,
            subject_storage_layout_version: RUNNER_SUBJECT_STORAGE_LAYOUT_VERSION,
            root_device_id: "unix:638:synthetic-empty-root".into(),
            root_link_count: 7,
            root_entry_count: 0,
            root_file_bytes: "0".into(),
            root_sha256: EMPTY_RUNNER_INVENTORY_SHA256.into(),
            subject_storage_subject_count: 0,
            subject_storage_subject_set_sha256: EMPTY_RUNNER_SUBJECT_STORAGE_INVENTORY_SHA256
                .into(),
            subject_storage_scope_count: 0,
            subject_storage_complete_root_entry_count: 0,
            subject_storage_complete_root_file_bytes: "0".into(),
            subject_storage_complete_root_sha256: EMPTY_RUNNER_SUBJECT_STORAGE_INVENTORY_SHA256
                .into(),
            locator_count: 0,
            resident_locator_count: 0,
            locator_set_sha256: EMPTY_RUNNER_INVENTORY_SHA256.into(),
            legacy_inventory_version: RUNNER_LEGACY_INVENTORY_VERSION,
            legacy_artifact_count: 0,
            legacy_artifact_bytes: "0".into(),
            legacy_artifact_set_sha256: EMPTY_RUNNER_LEGACY_ARTIFACT_SET_SHA256.into(),
            unclassified_root_count: 0,
            runner_build_id: runtime.runner_build_id.clone(),
            observed_at_ms: now,
        },
    )
    .context("fixture_volume_sign_attestation")?;
    let payload = attestation.attestation_sha256()?;
    let proof = sign_runner_volume_authority_proof(
        &signing_key,
        NewRunnerVolumeAuthorityProof {
            operation: "storage_attestation".into(),
            request_id: format!("storage-proof-{label}"),
            volume_id: volume_id.clone(),
            enrollment_epoch: 1,
            process_instance_id: process_instance_id.clone(),
            issued_at_ms: now,
            payload_sha256: payload.clone(),
        },
    )?;
    let authority = verify_runner_volume_authority_proof(
        pool,
        &proof,
        "storage_attestation",
        &payload,
        now,
        90_000,
    )?;
    record_runner_volume_storage_attestation(
        pool,
        &attestation,
        &runtime.runner_build_id,
        now,
        &authority,
    )
    .context("fixture_volume_record_attestation")?;
    activate_reconciled_runner_volume(pool, &volume_id, 1, &process_instance_id, now)
        .context("fixture_volume_activation")?;
    prepare_fleet(pool, label, clock(pool)?).context("fixture_volume_fleet")?;
    let volume = RunnerControlPlaneVolume {
        volume_id,
        enrollment_epoch: 1,
        process_instance_id,
        key_fingerprint,
        runtime_grant_id,
        runtime_sha256,
    };
    let private = RunnerControlPlanePrivateVolumeKey {
        version: 1,
        worker_id: worker_id.into(),
        volume: volume.clone(),
        public_key_base64url,
        signing_seed_base64url: encode_base64url(&seed),
    };
    Ok((volume, private))
}

fn prepare_fleet(pool: &DbPool, label: &str, now: i64) -> Result<()> {
    let status = runner_volume_fleet_status(pool).context("fixture_fleet_initial_status")?;
    anyhow::ensure!(
        status.legacy_inventory_state == "unknown",
        "fixture legacy inventory already initialized"
    );
    let reconciling = record_runner_legacy_inventory_authority(
        pool,
        &RecordRunnerLegacyInventoryAuthorityRequest {
            reconciliation_id: format!("synthetic-inventory-{label}"),
            authority_state: "reconciling".into(),
            expected_predecessor_generation: status.legacy_inventory_generation,
            expected_predecessor_authority_id: status.legacy_inventory_authority_id,
            expected_predecessor_authority_sha256: status.legacy_inventory_authority_sha256,
            root_count: 0,
            root_set_sha256: EMPTY_RUNNER_LEGACY_ROOT_SET_SHA256.into(),
            scope_ref: "all-managed-runner-storage-roots".into(),
            evidence_ref: format!("synthetic-inventory-start-{label}"),
            evidence_sha256: digest(&format!("synthetic-inventory-start:{label}")),
            authorized_by: "phase638-protocol-fixture".into(),
            recorded_at_ms: now,
        },
    )
    .context("fixture_fleet_inventory_begin")?
    .authority;
    record_runner_legacy_inventory_authority(
        pool,
        &RecordRunnerLegacyInventoryAuthorityRequest {
            reconciliation_id: reconciling.reconciliation_id,
            authority_state: "ready".into(),
            expected_predecessor_generation: reconciling.authority_generation,
            expected_predecessor_authority_id: Some(reconciling.authority_id),
            expected_predecessor_authority_sha256: Some(reconciling.authority_sha256),
            root_count: reconciling.root_count,
            root_set_sha256: reconciling.root_set_sha256,
            scope_ref: reconciling.scope_ref,
            evidence_ref: format!("synthetic-inventory-ready-{label}"),
            evidence_sha256: digest(&format!("synthetic-inventory-ready:{label}")),
            authorized_by: "phase638-protocol-fixture".into(),
            recorded_at_ms: now,
        },
    )
    .context("fixture_fleet_inventory_ready")?;
    let status = runner_volume_fleet_status(pool).context("fixture_fleet_inventory_status")?;
    let mut request = RecordRunnerVolumeFleetCutoverRequest {
        cutover_state: "reconciling".into(),
        expected_enrollment_generation: status.enrollment_generation,
        expected_purge_generation: status.purge_generation,
        expected_tombstone_generation: status.tombstone_generation,
        expected_destruction_generation: status.destruction_generation,
        expected_legacy_reconciliation_generation: status.legacy_reconciliation_generation,
        expected_storage_attestation_generation: status.storage_attestation_generation,
        expected_storage_attestation_count: status.storage_attestation_count,
        expected_storage_attestation_set_sha256: status.storage_attestation_set_sha256,
        expected_legacy_inventory_generation: status.legacy_inventory_generation,
        expected_legacy_inventory_reconciliation_id: status
            .legacy_inventory_reconciliation_id
            .context("fixture inventory missing")?,
        expected_legacy_inventory_authority_id: status
            .legacy_inventory_authority_id
            .context("fixture inventory missing")?,
        expected_legacy_inventory_authority_sha256: status
            .legacy_inventory_authority_sha256
            .context("fixture inventory missing")?,
        expected_legacy_inventory_root_count: status
            .legacy_inventory_root_count
            .context("fixture inventory missing")?,
        expected_legacy_inventory_root_set_sha256: status
            .legacy_inventory_root_set_sha256
            .context("fixture inventory missing")?,
        expected_non_destroyed_volume_count: status.non_destroyed_volume_count,
        expected_destruction_count: status.destruction_count,
        expected_unresolved_legacy_volume_count: status.unresolved_legacy_volume_count,
        evidence_ref: format!("synthetic-cutover-{label}"),
        evidence_sha256: digest(&format!("synthetic-cutover:{label}")),
        authorized_by: "phase638-protocol-fixture".into(),
        cutover_at_ms: now,
        now_ms: now,
    };
    record_runner_volume_fleet_cutover(pool, &request).context("fixture_fleet_cutover_begin")?;
    request.cutover_state = "ready".into();
    record_runner_volume_fleet_cutover(pool, &request).context("fixture_fleet_cutover_ready")?;
    let final_status = runner_volume_fleet_status(pool).context("fixture_fleet_final_status")?;
    anyhow::ensure!(
        final_status.cutover_state == "ready"
            && final_status.legacy_inventory_state == "ready"
            && final_status.non_destroyed_volume_count == 1
            && final_status.attested_reconciled_volume_count == 1,
        "fixture fleet not ready"
    );
    Ok(())
}

// No command claim, request-start publication, session alias, residency or lease creation.
// PostgreSQL itself enforces read-only operation for this entire handoff snapshot.
pub fn post_queue(
    pool: &DbPool,
    request: &RunnerControlPlanePostQueueRequest,
    bootstrap: &RunnerControlPlaneBootstrapResult,
    private_key: &RunnerControlPlanePrivateVolumeKey,
) -> Result<RunnerControlPlanePostQueueResult> {
    post_queue_with_mode(
        pool,
        request,
        bootstrap,
        private_key,
        FixtureMode::ReviewFirst,
    )
}

pub fn post_queue_auto_submit(
    pool: &DbPool,
    request: &RunnerControlPlanePostQueueRequest,
    bootstrap: &RunnerControlPlaneBootstrapResult,
    private_key: &RunnerControlPlanePrivateVolumeKey,
) -> Result<RunnerControlPlanePostQueueResult> {
    post_queue_with_mode(
        pool,
        request,
        bootstrap,
        private_key,
        FixtureMode::AutoSubmit,
    )
}

fn validate_bootstrap_binding(
    mode: FixtureMode,
    account_id: &str,
    bootstrap: &RunnerControlPlaneBootstrapResult,
    private_key: &RunnerControlPlanePrivateVolumeKey,
) -> Result<()> {
    anyhow::ensure!(
        bootstrap.version == 1
            && bootstrap.operation == mode.bootstrap_operation()
            && bootstrap.managed_release_fixture_version == 2
            && bootstrap.scope == fixture_scope()
            && bootstrap.account_id == account_id
            && synthetic_account_id(account_id)
            && private_key.version == 1
            && private_key.volume == bootstrap.volume,
        "fixture bootstrap binding"
    );
    validate_runtime_set(&bootstrap.runtimes, &private_key.worker_id)?;
    let seed: [u8; 32] = decode_base64url_exact(&private_key.signing_seed_base64url, 32)?
        .try_into()
        .map_err(|_| anyhow::anyhow!("fixture private key length"))?;
    let signing_key = Ed25519SigningKey::from_bytes(&seed);
    anyhow::ensure!(
        encode_base64url(signing_key.verifying_key().as_bytes())
            == private_key.public_key_base64url
            && runner_volume_id_from_public_key(&private_key.public_key_base64url)?
                == bootstrap.volume.volume_id
            && runner_volume_key_fingerprint(&private_key.public_key_base64url)?
                == bootstrap.volume.key_fingerprint,
        "fixture private volume binding"
    );
    Ok(())
}

fn post_queue_with_mode(
    pool: &DbPool,
    request: &RunnerControlPlanePostQueueRequest,
    bootstrap: &RunnerControlPlaneBootstrapResult,
    private_key: &RunnerControlPlanePrivateVolumeKey,
    mode: FixtureMode,
) -> Result<RunnerControlPlanePostQueueResult> {
    anyhow::ensure!(
        matches!(pool, DbPool::Postgres(_))
            && request.version == 1
            && request.operation == mode.post_queue_operation(),
        "fixture post-queue request"
    );
    for id in [
        &request.account_id,
        &request.application_id,
        &request.run_id,
        &request.browser_session_id,
        &request.command_id,
        &request.request_id,
        &request.workflow_id,
    ] {
        anyhow::ensure!(bounded_id(id, 1, 240), "fixture post-queue identifiers");
    }
    anyhow::ensure!(
        validate_execution_binding(&request.browser_profile_id, 160),
        "fixture browser profile"
    );
    validate_bootstrap_binding(mode, &request.account_id, bootstrap, private_key)?;
    let mut connection = pool.get_pg()?;
    let mut tx = connection
        .build_transaction()
        .isolation_level(postgres::IsolationLevel::RepeatableRead)
        .read_only(true)
        .start()?;
    let command_row = tx.query_opt(&format!(
        "SELECT {WORKFLOW_COMMAND_SELECT} FROM jobs_workflow_commands WHERE account_id=$1 AND id=$2"),
        &[&request.account_id, &request.command_id])?.context("fixture actual command missing")?;
    let command = workflow_command_from_stored(postgres_workflow_command_row(&command_row))?;
    anyhow::ensure!(
        command.application_id == request.application_id
            && command.run_id == request.run_id
            && command.workflow_id == request.workflow_id
            && command.request_id == request.request_id
            && command.command_kind == JobsWorkflowCommandKind::Start
            && command.protocol_version == 2
            && command.state == JobsWorkflowCommandState::DeliveryUnknown
            && command.first_request_started_at_ms.is_some()
            && command.temporal_run_id.is_none()
            && command.accepted_at_ms.is_none(),
        "fixture actual request-start command binding"
    );
    let JobsWorkflowCommandPayload::Start(material) = &command.envelope.payload else {
        anyhow::bail!("fixture command is not start");
    };
    anyhow::ensure!(
        material.browser_session_id == request.browser_session_id
            && material.result_request_id == request.request_id
            && material
                .workflow_input
                .get("accountId")
                .and_then(Value::as_str)
                == Some(request.account_id.as_str())
            && material
                .workflow_input
                .get("applicationId")
                .and_then(Value::as_str)
                == Some(request.application_id.as_str())
            && material
                .workflow_input
                .get("idempotencyKey")
                .and_then(Value::as_str)
                == Some(request.run_id.as_str())
            && material
                .workflow_input
                .get("browserProfileId")
                .and_then(Value::as_str)
                == Some(request.browser_profile_id.as_str())
            && material
                .workflow_input
                .get("runner")
                .and_then(Value::as_str)
                == Some("cloud"),
        "fixture actual workflow material binding"
    );
    let request_starts: i64 = tx.query_one(
        "SELECT count(*) FROM jobs_managed_cloud_request_start_authorities a
         JOIN jobs_workflow_command_attempt_events e ON e.command_id=a.command_id AND e.attempt_id=a.attempt_id
         WHERE a.account_id=$1 AND a.command_id=$2 AND e.event_kind='request_started'",
        &[&request.account_id, &request.command_id])?.get(0);
    anyhow::ensure!(
        request_starts == 1 && command.attempt_count == 1,
        "fixture requires exactly one real dispatcher request-start"
    );
    let row = tx
        .query_opt(
            "SELECT job_id,application_json FROM jobs_applications WHERE account_id=$1 AND id=$2",
            &[&request.account_id, &request.application_id],
        )?
        .context("fixture actual application missing")?;
    let job_id: String = row.get(0);
    let application = parse_application_json(
        row.get(1),
        &request.application_id,
        &job_id,
        "fixture application",
    )?;
    anyhow::ensure!(
        job_id == bootstrap.job_id
            && application.state == "queued"
            && application.submission_mode == mode.submission_mode()
            && application.run_id.as_deref() == Some(request.run_id.as_str())
            && application.submitted_at_ms.is_none(),
        "fixture ordinary queued application binding"
    );
    approved_submission_snapshot(&request.account_id, &application)?;
    let identity_id = application
        .receipt
        .pointer("/application_identity/id")
        .and_then(Value::as_str)
        .context("fixture approved identity missing")?;
    anyhow::ensure!(
        execution_browser_profile_id(&request.account_id, identity_id)
            == request.browser_profile_id
            && application
                .receipt
                .get("career_track_id")
                .and_then(Value::as_str)
                == Some(bootstrap.track_id.as_str()),
        "fixture approved track/profile binding"
    );
    let row = tx
        .query_opt(
            "SELECT session_json FROM jobs_browser_sessions WHERE account_id=$1 AND id=$2",
            &[&request.account_id, &request.browser_session_id],
        )?
        .context("fixture ordinary session missing")?;
    let session: BrowserSession = parse_json(row.get(0), "fixture browser session")?;
    anyhow::ensure!(
        session.id == request.browser_session_id
            && session.runner == "cloud"
            && session.status == "queued"
            && session.application_id.as_deref() == Some(request.application_id.as_str())
            && session.id == format!("cloud-{}", request.application_id)
            && session.id != request.run_id,
        "fixture ordinary session identity binding"
    );
    let aliases: i64 = tx
        .query_one(
            "SELECT count(*) FROM jobs_browser_sessions WHERE account_id=$1 AND id=$2",
            &[&request.account_id, &request.run_id],
        )?
        .get(0);
    let leases: i64 = tx
        .query_one(
            "SELECT count(*) FROM jobs_execution_leases WHERE account_id=$1 AND run_id=$2",
            &[&request.account_id, &request.run_id],
        )?
        .get(0);
    anyhow::ensure!(
        aliases == 0 && leases == 0,
        "fixture must not fabricate alias or preclaim"
    );
    let volume = tx.query_opt(
        "SELECT v.worker_id,v.current_epoch,v.active_instance_id,v.status,k.key_fingerprint,
         b.grant_id,b.runtime_sha256 FROM jobs_runner_volumes v
         JOIN jobs_runner_volume_keys k ON k.volume_id=v.volume_id AND k.enrollment_epoch=v.current_epoch
         JOIN jobs_runner_process_runtime_bindings b ON b.volume_id=v.volume_id
          AND b.enrollment_epoch=v.current_epoch AND b.process_instance_id=v.active_instance_id
         WHERE v.volume_id=$1", &[&bootstrap.volume.volume_id])?.context("fixture active volume missing")?;
    anyhow::ensure!(
        volume.get::<_, String>(0) == private_key.worker_id
            && volume.get::<_, i64>(1) == bootstrap.volume.enrollment_epoch
            && volume.get::<_, Option<String>>(2).as_deref()
                == Some(bootstrap.volume.process_instance_id.as_str())
            && volume.get::<_, String>(3) == "active"
            && volume.get::<_, String>(4) == bootstrap.volume.key_fingerprint
            && volume.get::<_, String>(5) == bootstrap.volume.runtime_grant_id
            && volume.get::<_, String>(6) == bootstrap.volume.runtime_sha256,
        "fixture durable volume binding"
    );
    let memo = postgres_managed_cloud_workflow_release_memo(&mut tx, &command)?
        .context("fixture actual managed memo missing")?;
    let stored_start = postgres_managed_cloud_stored_request_start_authority(&mut tx, &command.id)?
        .context("fixture actual request-start authority missing")?;
    let gateway_bytes = managed_cloud_decode_base64url(&stored_start.gateway_authority_base64url)?;
    anyhow::ensure!(
        managed_cloud_sha256(&gateway_bytes) == stored_start.gateway_authority_sha256
            && stored_start.account_id == request.account_id
            && stored_start.command_id == request.command_id,
        "fixture stored request-start authority binding"
    );
    let gateway: ManagedCloudGatewayAuthority = managed_cloud_parse_canonical(&gateway_bytes)?;
    let binding = postgres_managed_cloud_workflow_binding(&mut tx, &command.id)?
        .context("fixture actual command release binding missing")?;
    let binding = require_exact_managed_cloud_workflow_binding(
        &binding,
        &ManagedCloudWorkflowBindingInput {
            command_id: request.command_id.clone(),
            account_id: request.account_id.clone(),
            application_id: request.application_id.clone(),
            run_id: request.run_id.clone(),
            workflow_id: request.workflow_id.clone(),
            scope: fixture_scope(),
        },
    )?;
    validate_managed_cloud_gateway_authority(&gateway, &binding)?;
    anyhow::ensure!(
        gateway.execution == memo.execution
            && stored_start.binding_sha256 == memo.execution.binding_sha256,
        "fixture request-start memo equality"
    );
    let managed = bootstrap
        .runtimes
        .iter()
        .find(|entry| entry.instance.role == "managed_runner")
        .context("fixture managed runner missing")?;
    anyhow::ensure!(
        memo.version == 1
            && memo.execution.admission.scope == fixture_scope()
            && memo.execution.admission.activation_sha256 == managed.instance.activation_sha256
            && memo.execution.admission.manifest_sha256 == managed.instance.manifest_sha256,
        "fixture exact staging release memo binding"
    );
    let auto_summary = if mode == FixtureMode::AutoSubmit {
        let now = original_source_db_now_postgres(&mut tx)?;
        Some(read_auto_summary(
            &mut tx,
            &request.account_id,
            &application,
            bootstrap,
            now,
        )?)
    } else {
        None
    };
    tx.commit()?;
    Ok(RunnerControlPlanePostQueueResult {
        version: 1,
        operation: mode.post_queue_operation().into(),
        account_id: request.account_id.clone(),
        application_id: request.application_id.clone(),
        run_id: request.run_id.clone(),
        browser_session_id: request.browser_session_id.clone(),
        browser_profile_id: request.browser_profile_id.clone(),
        command_id: request.command_id.clone(),
        request_id: request.request_id.clone(),
        workflow_id: request.workflow_id.clone(),
        submission_mode: application.submission_mode.clone(),
        managed_cloud_release: memo,
        auto_summary,
    })
}

// Inspect only frozen packet authority and its exact stored authorization row. The ordinary
// queue/claim paths own current-authority resolution; their locking resolvers cannot be called
// from this database-enforced READ ONLY handoff.
fn read_auto_summary(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    application: &JobApplication,
    bootstrap: &RunnerControlPlaneBootstrapResult,
    now: i64,
) -> Result<RunnerControlPlaneAutoSummary> {
    let admission = ats_frozen_application_admission(application, now)?;
    let identity_id = application
        .receipt
        .pointer("/application_identity/id")
        .and_then(Value::as_str)
        .context("fixture auto identity missing")?;
    let source_id = application
        .receipt
        .pointer("/career_track_policy_authority/source_resume_asset_id")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .context("fixture auto source missing")?;
    anyhow::ensure!(
        admission.career_track_id == bootstrap.track_id
            && admission.certification.runner_target_sha256s
                == vec![bootstrap.volume.runtime_sha256.clone()],
        "fixture auto frozen track/runtime"
    );
    let authorization_count: i64 = tx
        .query_one(
            "SELECT count(*) FROM jobs_auto_submit_authorizations
         WHERE account_id=$1 AND id=$2 AND career_track_id=$3
          AND revision_no=$4 AND authority_fingerprint=$5 AND revoked_at_ms IS NULL
          AND application_identity_id=$6 AND source_resume_asset_id=$7",
            &[
                &account_id,
                &admission.auto_authorization_id,
                &bootstrap.track_id,
                &admission.auto_authorization_revision,
                &admission.auto_authorization_fingerprint_sha256,
                &identity_id,
                &source_id,
            ],
        )?
        .get(0);
    anyhow::ensure!(
        authorization_count == 1,
        "fixture exact active auto authorization"
    );
    let attempt = tx
        .query_opt(
            "SELECT id,runner,status FROM jobs_attempt_reservations
         WHERE account_id=$1 AND application_id=$2",
            &[&account_id, &application.id],
        )?
        .context("fixture auto attempt missing")?;
    anyhow::ensure!(
        attempt.get::<_, String>(1) == "cloud"
            && matches!(attempt.get::<_, String>(2).as_str(), "reserved" | "running"),
        "fixture auto attempt binding"
    );
    Ok(RunnerControlPlaneAutoSummary {
        attempt_id: attempt.get(0),
        auto_authorization_id: admission.auto_authorization_id,
        auto_authorization_revision: admission.auto_authorization_revision,
        auto_authorization_fingerprint_sha256: admission.auto_authorization_fingerprint_sha256,
        packet_checksum_sha256: admission.packet_checksum_sha256,
        provider: admission.certification.provider,
        target_key_sha256: admission.certification.target_key_sha256,
        manifest_sha256: admission.certification.manifest_sha256,
        activation_sha256: admission.certification.activation_sha256,
        layout_set_sha256: admission.certification.layout_set_sha256,
        adapter_bundle_sha256: admission.certification.adapter_bundle_sha256,
        runner_target_sha256: bootstrap.volume.runtime_sha256.clone(),
    })
}

fn validate_inspect_request(
    request: &RunnerControlPlaneInspectAutoSubmitClaimRequest,
) -> Result<()> {
    anyhow::ensure!(
        request.version == 1
            && request.operation == "inspect_auto_submit_claim"
            && synthetic_account_id(&request.account_id)
            && [
                &request.application_id,
                &request.run_id,
                &request.browser_session_id
            ]
            .into_iter()
            .all(|id| bounded_id(id, 1, 240))
            && request.browser_session_id != request.run_id
            && validate_execution_binding(&request.browser_profile_id, 160)
            && ats_certification_hex64(&request.lease_token_sha256)
            && (1..=9_007_199_254_740_991).contains(&request.lease_fence),
        "fixture auto inspection request"
    );
    Ok(())
}

/// One post-first-claim observation. Never creates, renews, consumes, or repairs authority.
pub fn inspect_auto_submit_claim(
    pool: &DbPool,
    request: &RunnerControlPlaneInspectAutoSubmitClaimRequest,
    bootstrap: &RunnerControlPlaneBootstrapResult,
    post_queue: &RunnerControlPlanePostQueueResult,
    private_key: &RunnerControlPlanePrivateVolumeKey,
) -> Result<RunnerControlPlaneInspectAutoSubmitClaimResult> {
    anyhow::ensure!(
        matches!(pool, DbPool::Postgres(_)),
        "fixture inspection PostgreSQL"
    );
    validate_inspect_request(request)?;
    validate_bootstrap_binding(
        FixtureMode::AutoSubmit,
        &request.account_id,
        bootstrap,
        private_key,
    )?;
    anyhow::ensure!(
        post_queue.version == 1
            && post_queue.operation == "post_queue_auto_submit"
            && post_queue.submission_mode == "auto_submit"
            && post_queue.account_id == request.account_id
            && post_queue.application_id == request.application_id
            && post_queue.run_id == request.run_id
            && post_queue.browser_session_id == request.browser_session_id
            && post_queue.browser_profile_id == request.browser_profile_id,
        "fixture inspection post-queue binding"
    );
    let expected = post_queue
        .auto_summary
        .as_ref()
        .context("fixture auto summary missing")?;
    let mut connection = pool.get_pg()?;
    let mut tx = connection
        .build_transaction()
        .isolation_level(postgres::IsolationLevel::RepeatableRead)
        .read_only(true)
        .start()?;
    let now = original_source_db_now_postgres(&mut tx)?;
    let row = tx
        .query_opt(
            "SELECT job_id,application_json FROM jobs_applications WHERE account_id=$1 AND id=$2",
            &[&request.account_id, &request.application_id],
        )?
        .context("fixture inspection application missing")?;
    let job_id: String = row.get(0);
    let application = parse_application_json(
        row.get(1),
        &request.application_id,
        &job_id,
        "fixture inspected application",
    )?;
    anyhow::ensure!(
        job_id == bootstrap.job_id
            && application.state == "queued"
            && application.submission_mode == "auto_submit"
            && application.run_id.as_deref() == Some(request.run_id.as_str())
            && application.submitted_at_ms.is_none(),
        "fixture inspected application binding"
    );
    approved_submission_snapshot(&request.account_id, &application)?;
    let observed = read_auto_summary(&mut tx, &request.account_id, &application, bootstrap, now)?;
    anyhow::ensure!(
        observed == *expected,
        "fixture inspected auto summary changed"
    );
    let identity_id = application
        .receipt
        .pointer("/application_identity/id")
        .and_then(Value::as_str)
        .context("fixture inspected identity missing")?;
    anyhow::ensure!(
        execution_browser_profile_id(&request.account_id, identity_id)
            == request.browser_profile_id,
        "fixture inspected identity/profile"
    );
    let row = tx
        .query_opt(
            "SELECT session_json FROM jobs_browser_sessions WHERE account_id=$1 AND id=$2",
            &[&request.account_id, &request.browser_session_id],
        )?
        .context("fixture inspected session missing")?;
    let session: BrowserSession = parse_json(row.get(0), "fixture inspected session")?;
    anyhow::ensure!(
        session.id == request.browser_session_id
            && session.id == format!("cloud-{}", application.id)
            && session.runner == "cloud"
            && session.status == "queued"
            && session.application_id.as_deref() == Some(application.id.as_str()),
        "fixture inspected durable session"
    );
    let lease = tx.query_opt(
        "SELECT account_id,application_id,browser_profile_id,owner_id,lease_token_sha256,
          fence,phase,lease_expires_at_ms,finished_at_ms FROM jobs_execution_leases WHERE run_id=$1",
        &[&request.run_id],
    )?.context("fixture prepared lease missing")?;
    anyhow::ensure!(
        lease.get::<_, String>(0) == request.account_id
            && lease.get::<_, String>(1) == request.application_id
            && lease.get::<_, String>(2) == request.browser_profile_id
            && lease.get::<_, String>(3) == private_key.worker_id
            && lease.get::<_, String>(4) == request.lease_token_sha256
            && lease.get::<_, i64>(5) == request.lease_fence
            && lease.get::<_, String>(6) == "prepared"
            && lease.get::<_, i64>(7) > now
            && lease.get::<_, Option<i64>>(8).is_none(),
        "fixture exact prepared lease"
    );
    let counts = tx.query_one(
        "SELECT (SELECT count(*) FROM jobs_application_ats_certification_bindings WHERE account_id=$1),
          (SELECT count(*) FROM jobs_application_ats_certification_bindings WHERE application_id=$2),
          (SELECT count(*) FROM jobs_application_ats_certification_bindings WHERE run_id=$3),
          (SELECT count(*) FROM jobs_browser_sessions WHERE account_id=$1 AND id=$3)",
        &[&request.account_id, &request.application_id, &request.run_id],
    )?;
    anyhow::ensure!(
        counts.get::<_, i64>(0) == 1
            && counts.get::<_, i64>(1) == 1
            && counts.get::<_, i64>(2) == 1
            && counts.get::<_, i64>(3) == 0,
        "fixture exact Phase A cardinality/no alias"
    );
    let row = tx.query_one(
        "SELECT * FROM jobs_application_ats_certification_bindings
         WHERE account_id=$1 AND application_id=$2 AND run_id=$3",
        &[
            &request.account_id,
            &request.application_id,
            &request.run_id,
        ],
    )?;
    let binding_sha256: String = row.get("binding_sha256");
    let authority = decode_ats_application_binding_authority(
        &binding_sha256,
        row.get::<_, String>("frozen_certification_base64url")
            .as_str(),
    )?;
    let phase: String = row.get("phase");
    let binding_fence: i64 = row.get("fence");
    for field in [
        "layout_observation_sha256",
        "observed_surface_sha256",
        "phase_b_request_id",
        "phase_b_request_sha256",
        "canary_reservation_sha256",
        "metering_reservation_sha256",
        "invalidation_kind",
    ] {
        anyhow::ensure!(
            row.get::<_, Option<String>>(field).is_none(),
            "fixture Phase B/invalidation absent"
        );
    }
    anyhow::ensure!(
        phase == "preflight"
            && binding_fence == 0
            && row.get::<_, Option<i64>>("consumed_at_ms").is_none()
            && row.get::<_, Option<i64>>("invalidated_at_ms").is_none()
            && row.get::<_, i64>("expires_at_ms") == authority.expires_at_ms
            && row.get::<_, i64>("created_at_ms") == authority.created_at_ms
            && authority.expires_at_ms > now
            && authority.requested_expires_at_ms >= authority.expires_at_ms,
        "fixture unconsumed future Phase A"
    );
    let trusted_row = tx.query_opt(&format!(
        "SELECT {TRUSTED_PROCESS_RUNTIME_COLUMNS} FROM jobs_execution_lease_process_runtime_bindings e
         JOIN jobs_runner_process_runtime_bindings b ON b.grant_id=e.runtime_grant_id
          AND b.worker_id=e.worker_id AND b.volume_id=e.volume_id AND b.enrollment_epoch=e.enrollment_epoch
          AND b.process_instance_id=e.process_instance_id AND b.runtime_sha256=e.runtime_sha256
         JOIN jobs_runner_process_runtime_grants g ON g.grant_id=b.grant_id
         WHERE e.run_id=$1 AND e.fence=$2"), &[&request.run_id, &request.lease_fence])?
        .context("fixture execution runtime binding missing")?;
    let trusted = trusted_process_runtime_from_pg_row(trusted_row);
    validate_trusted_process_runtime(&trusted)?;
    anyhow::ensure!(
        exact_trusted_process_binding(
            &trusted,
            &bootstrap.volume.runtime_grant_id,
            &private_key.worker_id,
            &bootstrap.volume.volume_id,
            bootstrap.volume.enrollment_epoch,
            &bootstrap.volume.process_instance_id,
            &bootstrap.volume.runtime_sha256
        ),
        "fixture exact execution runtime"
    );
    let runtime = runtime_target(&trusted.runtime)?;
    let posting_row = tx.query_one(
        "SELECT posting_json FROM jobs_postings WHERE account_id=$1 AND id=$2",
        &[&request.account_id, &job_id],
    )?;
    let posting: JobPosting = parse_json(posting_row.get(0), "fixture Phase A posting")?;
    let target = ats_certification_fresh_target_evidence_from_posting(&posting, now)?;
    let admission = ats_frozen_application_admission(&application, now)?;
    let context = AtsCertificationPhaseAContextRequest {
        account_id: request.account_id.clone(),
        application_id: request.application_id.clone(),
        run_id: request.run_id.clone(),
        browser_session_id: request.browser_session_id.clone(),
        browser_profile_id: request.browser_profile_id.clone(),
        runtime_attestation: cloud_ats_runtime_attestation(&trusted),
        nonce_sha256: request.lease_token_sha256.clone(),
    };
    anyhow::ensure!(
        authority.schema_version == 1
            && authority.binding_id == row.get::<_, String>("binding_id")
            && authority.binding_id == ats_phase_a_binding_id(&context, &expected.attempt_id)
            && authority.account_id == request.account_id
            && authority.application_id == request.application_id
            && authority.run_id == request.run_id
            && authority.application_attempt_id == expected.attempt_id
            && authority.browser_session_id == request.browser_session_id
            && authority.browser_profile_id == request.browser_profile_id
            && authority.packet_checksum_sha256 == expected.packet_checksum_sha256
            && authority.auto_authorization_id == expected.auto_authorization_id
            && authority.auto_authorization_revision == expected.auto_authorization_revision
            && authority.auto_authorization_fingerprint_sha256
                == expected.auto_authorization_fingerprint_sha256
            && authority.nonce_sha256 == request.lease_token_sha256
            && authority.target_evidence == target
            && authority.created_at_ms <= now
            && authority.certification.channel == "general"
            && authority.certification.selected_runtime.as_ref() == Some(&runtime)
            && ats_frozen_certification_admission_projection(&authority.certification)?
                == admission.certification,
        "fixture exact frozen Phase A authority"
    );
    for (field, value) in [
        ("account_id", request.account_id.as_str()),
        ("application_id", request.application_id.as_str()),
        ("run_id", request.run_id.as_str()),
        ("attempt_id", expected.attempt_id.as_str()),
        ("browser_session_id", request.browser_session_id.as_str()),
        ("browser_profile_id", request.browser_profile_id.as_str()),
        (
            "packet_checksum_sha256",
            expected.packet_checksum_sha256.as_str(),
        ),
        (
            "auto_authorization_id",
            expected.auto_authorization_id.as_str(),
        ),
        (
            "auto_authorization_fingerprint_sha256",
            expected.auto_authorization_fingerprint_sha256.as_str(),
        ),
        ("provider", expected.provider.as_str()),
        ("target_key", authority.certification.target_key.as_str()),
        ("manifest_sha256", expected.manifest_sha256.as_str()),
        ("activation_sha256", expected.activation_sha256.as_str()),
        ("layout_set_sha256", expected.layout_set_sha256.as_str()),
        (
            "adapter_bundle_sha256",
            expected.adapter_bundle_sha256.as_str(),
        ),
        (
            "runner_target_sha256",
            expected.runner_target_sha256.as_str(),
        ),
        ("browser_runtime_sha256", runtime.runtime_sha256.as_str()),
        ("platform", runtime.platform.as_str()),
        ("architecture", runtime.architecture.as_str()),
        (
            "automation_bundle_sha256",
            runtime.automation_bundle_sha256.as_str(),
        ),
        (
            "chromium_executable_sha256",
            runtime.chromium_executable_sha256.as_str(),
        ),
        ("nonce_sha256", request.lease_token_sha256.as_str()),
    ] {
        anyhow::ensure!(
            row.get::<_, String>(field) == value,
            "fixture Phase A stored projection"
        );
    }
    anyhow::ensure!(
        row.get::<_, i64>("auto_authorization_revision") == expected.auto_authorization_revision
            && row.get::<_, Option<String>>("runner_build_id") == runtime.runner_build_id
            && row.get::<_, Option<String>>("runner_image_sha256") == runtime.runner_image_sha256
            && row
                .get::<_, Option<String>>("browser_release_manifest_sha256")
                .is_none()
            && row
                .get::<_, Option<String>>("browser_artifact_sha256")
                .is_none()
            && row
                .get::<_, Option<String>>("browser_build_descriptor_sha256")
                .is_none(),
        "fixture Phase A stored cloud runtime"
    );
    tx.commit()?;
    Ok(RunnerControlPlaneInspectAutoSubmitClaimResult {
        version: 1,
        operation: "inspect_auto_submit_claim",
        account_id: request.account_id.clone(),
        application_id: request.application_id.clone(),
        run_id: request.run_id.clone(),
        browser_session_id: request.browser_session_id.clone(),
        browser_profile_id: request.browser_profile_id.clone(),
        lease_token_sha256: request.lease_token_sha256.clone(),
        lease_fence: request.lease_fence,
        ats_account_count: 1,
        ats_application_count: 1,
        ats_run_count: 1,
        binding_sha256,
        nonce_sha256: authority.nonce_sha256,
        phase,
        binding_fence,
        expires_future: true,
        exact_frozen_authority: true,
        exact_prepared_lease: true,
        no_phase_b_or_invalidation: true,
    })
}

fn validate_checkpoint_inspection_request(
    request: &RunnerControlPlaneInspectCheckpointRecoveryRequest,
) -> Result<()> {
    anyhow::ensure!(
        request.version == 1
            && request.operation == "inspect_checkpoint_recovery"
            && synthetic_account_id(&request.account_id)
            && [
                &request.application_id,
                &request.run_id,
                &request.browser_session_id
            ]
            .into_iter()
            .all(|id| bounded_id(id, 1, 240))
            && request.browser_session_id != request.run_id
            && validate_execution_binding(&request.browser_profile_id, 160),
        "fixture checkpoint inspection request"
    );
    Ok(())
}

fn canonical_fixture_value(value: &Value) -> Value {
    match value {
        Value::Array(values) => Value::Array(values.iter().map(canonical_fixture_value).collect()),
        Value::Object(values) => Value::Object(
            values
                .iter()
                .map(|(key, value)| (key.clone(), canonical_fixture_value(value)))
                .collect::<BTreeMap<_, _>>()
                .into_iter()
                .collect(),
        ),
        other => other.clone(),
    }
}

fn canonical_fixture_sha256(value: &Value) -> Result<String> {
    Ok(hex::encode(Sha256::digest(serde_json::to_vec(
        &canonical_fixture_value(value),
    )?)))
}

fn stable_application_sha256(application: &JobApplication) -> Result<String> {
    let mut value = serde_json::to_value(application)?;
    let object = value
        .as_object_mut()
        .context("fixture checkpoint application encoding")?;
    anyhow::ensure!(
        object.remove("state").is_some() && object.remove("updated_at_ms").is_some(),
        "fixture checkpoint application mutable fields"
    );
    object
        .get_mut("receipt")
        .and_then(Value::as_object_mut)
        .context("fixture checkpoint application receipt")?
        .remove("cloud_recovery");
    canonical_fixture_sha256(&value)
}

fn stable_session_sha256(session: &BrowserSession) -> Result<String> {
    let mut value = serde_json::to_value(session)?;
    let object = value
        .as_object_mut()
        .context("fixture checkpoint session encoding")?;
    for field in ["status", "current_step", "updated_at_ms", "takeover_url"] {
        anyhow::ensure!(
            object.remove(field).is_some(),
            "fixture checkpoint session mutable fields"
        );
    }
    canonical_fixture_sha256(&value)
}

fn checkpoint_cloud_recovery(
    stage: RunnerControlPlaneCheckpointStage,
    application: &JobApplication,
) -> Result<(
    Option<RunnerControlPlaneCloudRecoverySummary>,
    Option<String>,
)> {
    let Some(value) = application.receipt.get("cloud_recovery") else {
        anyhow::ensure!(stage.is_before(), "fixture checkpoint recovery missing");
        return Ok((None, None));
    };
    anyhow::ensure!(!stage.is_before(), "fixture checkpoint recovery premature");
    let summary: RunnerControlPlaneCloudRecoverySummary = serde_json::from_value(value.clone())?;
    anyhow::ensure!(
        summary.schema_version == 1
            && summary.status == "released"
            && summary.checkpoint_version == 2
            && summary.checkpoint_phase == "prepared"
            && summary.lease_phase == "prepared"
            && (1..=9_007_199_254_740_991).contains(&summary.recorded_at_ms)
            && summary.run_id == application.run_id.as_deref().unwrap_or_default(),
        "fixture checkpoint recovery summary"
    );
    let sha256 = canonical_fixture_sha256(&serde_json::to_value(&summary)?)?;
    Ok((Some(summary), Some(sha256)))
}

/// Read-only proof that checkpoint recovery changed only the permitted application/session
/// lifecycle fields. Existing Node snapshots remain authoritative for all other table families.
pub fn inspect_checkpoint_recovery(
    pool: &DbPool,
    request: &RunnerControlPlaneInspectCheckpointRecoveryRequest,
    bootstrap: &RunnerControlPlaneBootstrapResult,
    post_queue: &RunnerControlPlanePostQueueResult,
    private_key: &RunnerControlPlanePrivateVolumeKey,
) -> Result<RunnerControlPlaneInspectCheckpointRecoveryResult> {
    anyhow::ensure!(
        matches!(pool, DbPool::Postgres(_)),
        "fixture checkpoint inspection PostgreSQL"
    );
    validate_checkpoint_inspection_request(request)?;
    let mut connection = pool.get_pg()?;
    let mut tx = connection
        .build_transaction()
        .isolation_level(postgres::IsolationLevel::RepeatableRead)
        .read_only(true)
        .start()?;
    let row = tx
        .query_opt(
            "SELECT job_id,application_json FROM jobs_applications
             WHERE account_id=$1 AND id=$2",
            &[&request.account_id, &request.application_id],
        )?
        .context("fixture checkpoint application missing")?;
    let job_id: String = row.get(0);
    let application = parse_application_json(
        row.get(1),
        &request.application_id,
        &job_id,
        "fixture checkpoint application",
    )?;
    let mode = FixtureMode::from_submission_mode(&application.submission_mode)?;
    validate_bootstrap_binding(mode, &request.account_id, bootstrap, private_key)?;
    anyhow::ensure!(
        bootstrap.job_id == job_id
            && post_queue.version == 1
            && post_queue.operation == mode.post_queue_operation()
            && post_queue.account_id == request.account_id
            && post_queue.application_id == request.application_id
            && post_queue.run_id == request.run_id
            && post_queue.browser_session_id == request.browser_session_id
            && post_queue.browser_profile_id == request.browser_profile_id
            && post_queue.submission_mode == mode.submission_mode()
            && post_queue.auto_summary.is_some() == (mode == FixtureMode::AutoSubmit)
            && application.id == request.application_id
            && application.run_id.as_deref() == Some(request.run_id.as_str())
            && application.submitted_at_ms.is_none(),
        "fixture checkpoint file/application binding"
    );
    approved_submission_snapshot(&request.account_id, &application)?;
    let identity_id = application
        .receipt
        .pointer("/application_identity/id")
        .and_then(Value::as_str)
        .context("fixture checkpoint identity missing")?;
    anyhow::ensure!(
        execution_browser_profile_id(&request.account_id, identity_id)
            == request.browser_profile_id,
        "fixture checkpoint identity/profile"
    );

    let row = tx
        .query_opt(
            "SELECT session_json FROM jobs_browser_sessions WHERE account_id=$1 AND id=$2",
            &[&request.account_id, &request.browser_session_id],
        )?
        .context("fixture checkpoint session missing")?;
    let session: BrowserSession = parse_json(row.get(0), "fixture checkpoint session")?;
    anyhow::ensure!(
        session.id == request.browser_session_id
            && session.id == format!("cloud-{}", application.id)
            && session.id != request.run_id
            && session.runner == "cloud"
            && session.application_id.as_deref() == Some(request.application_id.as_str())
            && session.takeover_url.is_none(),
        "fixture checkpoint session binding"
    );

    let lease = tx
        .query_opt(
            "SELECT account_id,application_id,browser_profile_id,owner_id,phase
             FROM jobs_execution_leases WHERE run_id=$1",
            &[&request.run_id],
        )?
        .context("fixture checkpoint lease missing")?;
    let lease_phase: String = lease.get(4);
    anyhow::ensure!(
        lease.get::<_, String>(0) == request.account_id
            && lease.get::<_, String>(1) == request.application_id
            && lease.get::<_, String>(2) == request.browser_profile_id
            && lease.get::<_, String>(3) == private_key.worker_id,
        "fixture checkpoint lease binding"
    );
    let attempt = tx
        .query_opt(
            "SELECT runner,status FROM jobs_attempt_reservations
             WHERE account_id=$1 AND application_id=$2",
            &[&request.account_id, &request.application_id],
        )?
        .context("fixture checkpoint attempt missing")?;
    let attempt_status: String = attempt.get(1);
    anyhow::ensure!(
        attempt.get::<_, String>(0) == "cloud",
        "fixture checkpoint attempt binding"
    );

    if request.stage.is_before() {
        anyhow::ensure!(
            matches!(application.state.as_str(), "queued" | "running")
                && matches!(session.status.as_str(), "queued" | "running")
                && lease_phase == "prepared"
                && matches!(attempt_status.as_str(), "reserved" | "running"),
            "fixture checkpoint before state"
        );
    } else {
        anyhow::ensure!(
            application.state == "failed"
                && session.status == "failed"
                && session.current_step == "Browser run stopped before submission"
                && lease_phase == "released"
                && attempt_status == "released",
            "fixture checkpoint recovered state"
        );
    }
    let (cloud_recovery, cloud_recovery_sha256) =
        checkpoint_cloud_recovery(request.stage, &application)?;
    let application_stable_sha256 = stable_application_sha256(&application)?;
    let session_stable_sha256 = stable_session_sha256(&session)?;
    tx.commit()?;
    Ok(RunnerControlPlaneInspectCheckpointRecoveryResult {
        version: 1,
        operation: "inspect_checkpoint_recovery",
        stage: request.stage,
        account_id: request.account_id.clone(),
        application_id: request.application_id.clone(),
        run_id: request.run_id.clone(),
        browser_session_id: request.browser_session_id.clone(),
        browser_profile_id: request.browser_profile_id.clone(),
        submission_mode: application.submission_mode,
        application_stable_sha256,
        session_stable_sha256,
        application_state: application.state,
        session_status: session.status,
        session_current_step: session.current_step,
        submitted_absent: true,
        takeover_absent: true,
        lease_phase,
        attempt_status,
        cloud_recovery,
        cloud_recovery_sha256,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn checkpoint_application() -> JobApplication {
        JobApplication {
            id: "synthetic-application".into(),
            job_id: "synthetic-job".into(),
            resume_version_id: Some("synthetic-resume".into()),
            state: "running".into(),
            submission_mode: "review_first".into(),
            match_score: 93,
            answers: vec![json!({"question":"authorized","answer":true})],
            cover_letter: "synthetic cover letter".into(),
            receipt: json!({
                "application_identity": {"id":"synthetic-identity"},
                "packet": {"checksum":"packet-a","resume_version_id":"synthetic-resume"}
            }),
            run_id: Some("synthetic-run".into()),
            created_at_ms: 10,
            updated_at_ms: 20,
            submitted_at_ms: None,
        }
    }

    fn checkpoint_session() -> BrowserSession {
        BrowserSession {
            id: "cloud-synthetic-application".into(),
            runner: "cloud".into(),
            status: "running".into(),
            current_company: "Synthetic employer".into(),
            current_step: "Filling application".into(),
            application_id: Some("synthetic-application".into()),
            takeover_url: None,
            created_at_ms: 10,
            updated_at_ms: 20,
        }
    }

    #[test]
    fn checkpoint_inspection_schema_is_closed_to_exact_eight_fields() {
        let value = json!({"version":1,"operation":"inspect_checkpoint_recovery",
            "stage":"before_recovery","account_id":format!("ui_fixture_{}", uuid::Uuid::new_v4()),
            "application_id":"synthetic-application","run_id":"synthetic-run",
            "browser_session_id":"cloud-synthetic-application",
            "browser_profile_id":"synthetic-profile"});
        let request: RunnerControlPlaneInspectCheckpointRecoveryRequest =
            serde_json::from_value(value.clone()).unwrap();
        assert!(validate_checkpoint_inspection_request(&request).is_ok());
        assert_eq!(value.as_object().unwrap().len(), 8);
        for field in [
            "lease_token",
            "data_key",
            "candidate",
            "resume_content",
            "table",
        ] {
            let mut invalid = value.clone();
            invalid[field] = json!("forbidden");
            assert!(
                serde_json::from_value::<RunnerControlPlaneInspectCheckpointRecoveryRequest>(
                    invalid
                )
                .is_err()
            );
        }
        let mut wrong_stage = value;
        wrong_stage["stage"] = json!("recovered");
        assert!(
            serde_json::from_value::<RunnerControlPlaneInspectCheckpointRecoveryRequest>(
                wrong_stage
            )
            .is_err()
        );
    }

    #[test]
    fn checkpoint_stable_hashes_ignore_only_permitted_lifecycle_fields() {
        let application = checkpoint_application();
        let expected_application = stable_application_sha256(&application).unwrap();
        let mut recovered = application.clone();
        recovered.state = "failed".into();
        recovered.updated_at_ms = 30;
        recovered.receipt["cloud_recovery"] = json!({
            "schema_version":1,"status":"released","checkpoint_version":2,
            "checkpoint_phase":"prepared","lease_phase":"prepared",
            "recorded_at_ms":30,"run_id":"synthetic-run"
        });
        assert_eq!(
            stable_application_sha256(&recovered).unwrap(),
            expected_application
        );
        let mut answer_changed = application.clone();
        answer_changed.answers[0]["answer"] = json!(false);
        assert_ne!(
            stable_application_sha256(&answer_changed).unwrap(),
            expected_application
        );
        let mut packet_changed = application.clone();
        packet_changed.receipt["packet"]["checksum"] = json!("packet-b");
        assert_ne!(
            stable_application_sha256(&packet_changed).unwrap(),
            expected_application
        );
        let mut identity_changed = application.clone();
        identity_changed.receipt["application_identity"]["id"] = json!("other-identity");
        assert_ne!(
            stable_application_sha256(&identity_changed).unwrap(),
            expected_application
        );
        let mut submitted_changed = application;
        submitted_changed.submitted_at_ms = Some(31);
        assert_ne!(
            stable_application_sha256(&submitted_changed).unwrap(),
            expected_application
        );
        let mut run_changed = checkpoint_application();
        run_changed.run_id = Some("other-run".into());
        assert_ne!(
            stable_application_sha256(&run_changed).unwrap(),
            expected_application
        );
        let mut created_changed = checkpoint_application();
        created_changed.created_at_ms = 11;
        assert_ne!(
            stable_application_sha256(&created_changed).unwrap(),
            expected_application
        );

        let session = checkpoint_session();
        let expected_session = stable_session_sha256(&session).unwrap();
        let mut stopped = session.clone();
        stopped.status = "failed".into();
        stopped.current_step = "Browser run stopped before submission".into();
        stopped.updated_at_ms = 30;
        stopped.takeover_url = Some("https://not-exported.invalid".into());
        assert_eq!(stable_session_sha256(&stopped).unwrap(), expected_session);
        let mut rebound = session;
        rebound.application_id = Some("other-application".into());
        assert_ne!(stable_session_sha256(&rebound).unwrap(), expected_session);
    }

    #[test]
    fn checkpoint_cloud_recovery_requires_exact_closed_seven_field_summary() {
        let value = json!({"schema_version":1,"status":"released","checkpoint_version":2,
            "checkpoint_phase":"prepared","lease_phase":"prepared","recorded_at_ms":30,
            "run_id":"synthetic-run"});
        assert_eq!(value.as_object().unwrap().len(), 7);
        assert!(
            serde_json::from_value::<RunnerControlPlaneCloudRecoverySummary>(value.clone()).is_ok()
        );
        let mut invalid = value;
        invalid["lease_token"] = json!("forbidden");
        assert!(serde_json::from_value::<RunnerControlPlaneCloudRecoverySummary>(invalid).is_err());
    }

    fn inspection_request() -> RunnerControlPlaneInspectAutoSubmitClaimRequest {
        RunnerControlPlaneInspectAutoSubmitClaimRequest {
            version: 1,
            operation: "inspect_auto_submit_claim".into(),
            account_id: format!("ui_fixture_{}", uuid::Uuid::new_v4()),
            application_id: "synthetic-application".into(),
            run_id: "synthetic-run".into(),
            browser_session_id: "cloud-synthetic-application".into(),
            browser_profile_id: "synthetic-profile".into(),
            lease_token_sha256: "a".repeat(64),
            lease_fence: 1,
        }
    }

    #[test]
    fn auto_inspection_requires_closed_operation_hash_fence_and_distinct_session() {
        assert!(validate_inspect_request(&inspection_request()).is_ok());
        for change in 0..8 {
            let mut request = inspection_request();
            match change {
                0 => request.version = 2,
                1 => request.operation = "post_queue_auto_submit".into(),
                2 => request.account_id = "owner".into(),
                3 => request.browser_session_id = request.run_id.clone(),
                4 => request.lease_token_sha256 = "A".repeat(64),
                5 => request.lease_token_sha256 = "a".repeat(63),
                6 => request.lease_fence = 0,
                7 => request.lease_fence = 9_007_199_254_740_992,
                _ => unreachable!(),
            }
            assert!(validate_inspect_request(&request).is_err());
        }
        let value = json!({"version":1,"operation":"inspect_auto_submit_claim",
            "account_id":"a","application_id":"p","run_id":"r",
            "browser_session_id":"s","browser_profile_id":"b",
            "lease_token_sha256":"a".repeat(64),"lease_fence":1});
        assert!(
            serde_json::from_value::<RunnerControlPlaneInspectAutoSubmitClaimRequest>(
                value.clone()
            )
            .is_ok()
        );
        for extra in ["lease_token", "claim", "environment", "frozen_authority"] {
            let mut invalid = value.clone();
            invalid[extra] = json!("forbidden");
            assert!(
                serde_json::from_value::<RunnerControlPlaneInspectAutoSubmitClaimRequest>(invalid)
                    .is_err()
            );
        }
    }

    #[test]
    fn review_first_handoff_wire_round_trip_omits_auto_summary() {
        let h = "a".repeat(64);
        let original = json!({"version":1,"operation":"post_queue","account_id":"a",
            "application_id":"p","run_id":"r","browser_session_id":"s",
            "browser_profile_id":"b","command_id":"c","request_id":"q","workflow_id":"w",
            "submission_mode":"review_first","managed_cloud_release":{
                "version":1,"bindingSha256":h,"scope":fixture_scope(),"headRevision":1,
                "transitionSha256":h,"activationSha256":h,"manifestSha256":h,"cohortSha256":h,
                "trustGeneration":1,"channelSequence":1,"releaseId":"synthetic-release",
                "releaseSequence":1,"taskQueueSha256":h,"failureConverterSha256":h,
                "readinessSha256":h,"activationExpiresAtMs":2,"resolvedAtMs":1}});
        let result: RunnerControlPlanePostQueueResult =
            serde_json::from_value(original.clone()).unwrap();
        assert!(result.auto_summary.is_none());
        assert_eq!(serde_json::to_value(result).unwrap(), original);
        assert_eq!(FixtureMode::ReviewFirst.bootstrap_operation(), "bootstrap");
        assert_eq!(
            FixtureMode::ReviewFirst.post_queue_operation(),
            "post_queue"
        );
        assert_eq!(FixtureMode::ReviewFirst.submission_mode(), "review_first");
        assert_eq!(
            FixtureMode::AutoSubmit.bootstrap_operation(),
            "bootstrap_auto_submit"
        );
        assert_eq!(
            FixtureMode::AutoSubmit.post_queue_operation(),
            "post_queue_auto_submit"
        );
        assert_eq!(FixtureMode::AutoSubmit.submission_mode(), "auto_submit");
    }

    #[test]
    fn auto_summary_is_closed_and_has_no_missing_field_fallback() {
        let value = json!({"attempt_id":"attempt","auto_authorization_id":"authorization",
            "auto_authorization_revision":1,"auto_authorization_fingerprint_sha256":"a".repeat(64),
            "packet_checksum_sha256":"b".repeat(64),"provider":"greenhouse","target_key_sha256":"c".repeat(64),
            "manifest_sha256":"d".repeat(64),"activation_sha256":"e".repeat(64),"layout_set_sha256":"f".repeat(64),
            "adapter_bundle_sha256":"1".repeat(64),"runner_target_sha256":"2".repeat(64)});
        assert!(serde_json::from_value::<RunnerControlPlaneAutoSummary>(value.clone()).is_ok());
        for field in value.as_object().unwrap().keys() {
            let mut invalid = value.clone();
            invalid.as_object_mut().unwrap().remove(field);
            assert!(serde_json::from_value::<RunnerControlPlaneAutoSummary>(invalid).is_err());
        }
        let mut invalid = value;
        invalid["resume_content"] = json!("must not be exported");
        assert!(serde_json::from_value::<RunnerControlPlaneAutoSummary>(invalid).is_err());
    }

    #[test]
    fn synthetic_runtime_uses_existing_canonical_build_and_ats_contracts() {
        let runtime = runtime_model("synthetic-unit-only");
        validate_runner_process_runtime(&runtime).expect("valid protocol runtime metadata");
        assert!(runner_build_id_is_canonical(&runtime.runner_build_id));
        let target = runtime_target(&runtime).expect("derive runtime target");
        validate_ats_runtime_target(&target).expect("valid cloud ATS target");
        assert_eq!(target.runtime_kind, "cloud");
        assert!(target.browser_release_manifest_sha256.is_none());
        assert_eq!(fixture_scope().environment, "staging");
    }

    #[test]
    fn synthetic_account_requires_exact_generated_uuid_namespace() {
        let id = uuid::Uuid::new_v4().to_string();
        assert!(synthetic_account_id(&format!("ui_fixture_{id}")));
        for value in [
            id,
            "ui_fixture_owner".into(),
            "owner".into(),
            "ui_fixture_00000000-0000-0000-0000-000000000000".into(),
        ] {
            assert!(!synthetic_account_id(&value));
        }
    }
}
