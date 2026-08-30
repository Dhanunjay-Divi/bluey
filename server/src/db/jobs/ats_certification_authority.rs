const ATS_CERTIFICATION_EVIDENCE_AUDIENCE: &str = "bluey-jobs-ats-certification-evidence-v1";
const ATS_CERTIFICATION_MANIFEST_AUDIENCE: &str = "bluey-jobs-ats-certification-manifest-v1";
const ATS_CERTIFICATION_ACTIVATION_AUDIENCE: &str = "bluey-jobs-ats-certification-activation-v1";
const ATS_CERTIFICATION_REVOCATION_AUDIENCE: &str = "bluey-jobs-ats-certification-revocation-v1";
const ATS_CERTIFICATION_QUARANTINE_AUDIENCE: &str = "bluey-jobs-ats-certification-quarantine-v1";
const ATS_CERTIFICATION_LAYOUT_OBSERVATION_AUDIENCE: &str = "bluey-jobs-ats-layout-observation-v1";
const ATS_CERTIFICATION_TRUST_POLICY_AUDIENCE: &str =
    "bluey-jobs-ats-certification-trust-policy-v1";
const ATS_CERTIFICATION_AUTHORIZATION_AUDIENCE: &str =
    "bluey-jobs-ats-certification-authorization-v1";
const ATS_CERTIFICATION_TRANSITION_AUDIENCE: &str =
    "bluey-jobs-ats-certification-head-transition-v1";
const ATS_CERTIFICATION_MAX_CANONICAL_BYTES: usize = 64 * 1024;
const ATS_CERTIFICATION_MAX_SAFE_INTEGER: i64 = 9_007_199_254_740_991;
const ATS_CERTIFICATION_TARGET_EVIDENCE_FRESHNESS_MS: i64 = 15 * 60 * 1_000;
const ATS_CERTIFICATION_APPLICATION_BINDING_TTL_MS: i64 = 15 * 60 * 1_000;
const ATS_CERTIFICATION_MAX_CANARY_ALLOWLIST_MEMBERS: usize = 10_000;
const ATS_CERTIFICATION_MAX_RUNTIME_LAYOUT_QUARANTINE_EVIDENCE: i64 = 64;
const ATS_CERTIFICATION_RUNTIME_LAYOUT_QUARANTINE_RECORDED_BY: &str = "ats-phase-b-runtime";
const ATS_GREENHOUSE_EXACT_ADAPTER_VERSION: &str = "2026.07.1-beta.1";
const ATS_LEVER_EXACT_ADAPTER_VERSION: &str = "2026.07.0-beta.1";
const ATS_GREENHOUSE_FINAL_SUBMIT_CONTROL_ID: &str = "greenhouse_submit_application";
const ATS_LEVER_FINAL_SUBMIT_CONTROL_ID: &str = "lever_application_submit";
const ATS_GREENHOUSE_ALLOWED_PROVIDER_HOSTS: [&str; 2] =
    ["boards.greenhouse.io", "job-boards.greenhouse.io"];
const ATS_LAYOUT_CHALLENGE_CATEGORIES: [&str; 5] = [
    "assessment",
    "captcha",
    "email_otp",
    "sms_otp",
    "two_factor",
];
const ATS_LAYOUT_CONFIRMATION_STATE_CATEGORIES: [&str; 2] =
    ["provider_confirmation", "provider_success"];
const ATS_CERTIFICATION_ROOT_TRUST_ANCHOR_ENV: &str =
    "BLUEY_JOBS_ATS_CERTIFICATION_ROOT_TRUST_ANCHOR_JSON";
const ATS_CERTIFICATION_REQUIRED_CHECK_IDS: [&str; 16] = [
    "ATS-AUTH-001",
    "ATS-CHALLENGE-001",
    "ATS-CONFIRM-001",
    "ATS-DOCS-001",
    "ATS-DRIFT-001",
    "ATS-DYNAMIC-001",
    "ATS-FAULT-001",
    "ATS-FIELDS-001",
    "ATS-LAYOUT-001",
    "ATS-NEGATIVE-001",
    "ATS-PRIVACY-001",
    "ATS-RECEIPT-001",
    "ATS-REPLAY-001",
    "ATS-RUNNER-001",
    "ATS-SUBMIT-001",
    "ATS-TARGET-001",
];

fn ats_certification_canary_utc_period_key(
    at_ms: i64,
) -> Result<String, AtsCertificationAuthorityError> {
    if !ats_certification_safe_integer(at_ms, false) {
        return Err(AtsCertificationAuthorityError::InvalidAuthority);
    }
    Utc.timestamp_millis_opt(at_ms)
        .single()
        .map(|value| value.format("%Y-%m-%d").to_string())
        .ok_or(AtsCertificationAuthorityError::InvalidAuthority)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AtsCertificationAuthorityEnvelope {
    pub canonical_base64url: String,
    pub authorization_base64url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AtsCertificationDetachedSignature {
    pub key_id: String,
    pub signature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AtsCertificationAuthorization {
    pub version: i64,
    pub audience: String,
    pub authorization_id: String,
    pub role: String,
    pub target_audience: String,
    pub target_sha256: String,
    pub signed_at_ms: i64,
    pub signatures: Vec<AtsCertificationDetachedSignature>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AtsCertificationTrustRole {
    pub threshold: i64,
    pub keys: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AtsCertificationTrustAnchor {
    pub roles: BTreeMap<String, AtsCertificationTrustRole>,
}

impl AtsCertificationTrustAnchor {
    pub fn new(
        roles: BTreeMap<String, AtsCertificationTrustRole>,
    ) -> Result<Self, AtsCertificationAuthorityError> {
        let anchor = Self { roles };
        validate_ats_certification_trust_anchor(&anchor)?;
        Ok(anchor)
    }

    pub fn sha256(&self) -> Result<String, AtsCertificationAuthorityError> {
        let canonical = ats_certification_canonical_json(self)
            .map_err(|_| AtsCertificationAuthorityError::InvalidTrustAnchor)?;
        Ok(ats_certification_sha256(&canonical))
    }
}

/// Offline root keys are configured by the server operator and authorize the
/// persisted delegated trust-policy chain. They never authorize evidence,
/// manifests, promotions, or incident commands directly.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AtsCertificationRootTrustAnchor {
    pub threshold: i64,
    pub keys: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AtsCertificationTrustPolicyAuthority {
    pub version: i64,
    pub audience: String,
    pub policy_id: String,
    pub trust_generation: i64,
    pub predecessor_policy_sha256: Option<String>,
    pub delegated_trust: AtsCertificationTrustAnchor,
    pub certification_requirements: AtsCertificationPolicyRequirements,
    pub issued_at_ms: i64,
    pub valid_from_ms: i64,
    pub expires_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AtsCertificationPolicyRequirements {
    pub allowed_providers: Vec<String>,
    pub suite_id: String,
    pub required_check_ids: Vec<String>,
    pub required_evidence_classes: Vec<String>,
    pub maximum_manifest_lifetime_ms: i64,
    pub maximum_clock_skew_ms: i64,
    pub maximum_manifest_size_bytes: i64,
    pub maximum_target_count: i64,
    pub maximum_observation_count: i64,
    pub maximum_evidence_object_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AtsCertificationZeroToleranceCounters {
    pub hard_filter_violations: i64,
    pub unsupported_factual_claims: i64,
    pub duplicate_submit_activations: i64,
    pub false_submitted_states: i64,
    pub incomplete_or_mismatched_receipts: i64,
    pub pii_bearing_observations: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AtsCertificationCheckResult {
    pub runner_target_sha256: String,
    pub check_id: String,
    pub evidence_class: String,
    pub passed_count: i64,
    pub failed_count: i64,
    pub skipped_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AtsManifestCertificationProfile {
    pub suite_id: String,
    pub suite_version: String,
    pub suite_manifest_sha256: String,
    pub layout_set_sha256: String,
    pub layout_observation_sha256s: Vec<String>,
    pub check_results: Vec<AtsCertificationCheckResult>,
    pub zero_tolerance: AtsCertificationZeroToleranceCounters,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AtsCertificationEvidenceAuthority {
    pub version: i64,
    pub audience: String,
    pub evidence_id: String,
    pub policy_sha256: String,
    pub provider: String,
    pub target_key: String,
    pub variant_key: String,
    pub surface_sha256: String,
    pub source_kind: String,
    pub object_key: String,
    pub object_sha256: String,
    pub object_size_bytes: i64,
    pub media_type: String,
    pub provenance_sha256: String,
    pub authorization_ref: String,
    pub sanitizer_version: String,
    pub captured_at_ms: i64,
    pub issued_at_ms: i64,
    pub expires_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AtsCertificationRuntimeTarget {
    pub runtime_kind: String,
    pub runtime_id: String,
    pub runtime_sha256: String,
    pub platform: String,
    pub architecture: String,
    pub automation_bundle_sha256: String,
    pub browser_release_manifest_sha256: Option<String>,
    pub browser_artifact_sha256: Option<String>,
    pub browser_build_descriptor_sha256: Option<String>,
    pub runner_build_id: Option<String>,
    pub runner_image_sha256: Option<String>,
    pub playwright_version: String,
    pub chromium_revision: String,
    pub chromium_executable_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AtsCertificationManifestAuthority {
    pub version: i64,
    pub audience: String,
    pub certification_id: String,
    pub policy_sha256: String,
    pub manifest_generation: i64,
    pub predecessor_manifest_sha256: Option<String>,
    pub provider: String,
    pub target_key: String,
    pub allowed_provider_hosts: Vec<String>,
    pub variant_key: String,
    pub surface_sha256: String,
    pub scope_sha256: String,
    pub adapter_version: String,
    pub final_submit_control_id: String,
    pub adapter_bundle_sha256: String,
    pub source_commit: String,
    pub layout_contract_version: i64,
    pub layout_contract_sha256: String,
    pub maximum_capability: String,
    pub certification_profile: AtsManifestCertificationProfile,
    pub evidence_sha256s: Vec<String>,
    pub runtime_targets: Vec<AtsCertificationRuntimeTarget>,
    pub tested_at_ms: i64,
    pub issued_at_ms: i64,
    pub not_before_ms: i64,
    pub expires_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AtsLayoutControlObservation {
    pub control_kind: String,
    pub required: bool,
    pub provider_attribute_sha256: String,
    pub option_count: i64,
    pub conditional_on_attribute_sha256: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AtsLayoutFormObservation {
    pub method: String,
    pub encoding: String,
    pub target_sha256: String,
    pub action_identity_sha256: String,
    pub submit_control_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AtsLayoutObservationAuthority {
    pub version: i64,
    pub audience: String,
    pub observation_id: String,
    pub policy_sha256: String,
    pub provider: String,
    pub target_fingerprint_sha256: String,
    pub page_variant: String,
    pub surface_sha256: String,
    pub adapter_version: String,
    pub runner_target_sha256: String,
    pub evidence_class: String,
    pub controls: Vec<AtsLayoutControlObservation>,
    pub form: AtsLayoutFormObservation,
    pub challenge_categories: Vec<String>,
    pub step_count: i64,
    pub confirmation_state_categories: Vec<String>,
    pub predecessor_observation_sha256: Option<String>,
    pub observed_at_ms: i64,
    pub issued_at_ms: i64,
    pub expires_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AtsCertificationActivationAuthority {
    pub version: i64,
    pub audience: String,
    pub activation_id: String,
    pub policy_sha256: String,
    pub activation_generation: i64,
    pub predecessor_activation_sha256: Option<String>,
    pub manifest_sha256: String,
    pub scope_sha256: String,
    pub channel: String,
    pub channel_sequence: i64,
    pub capability: String,
    pub account_allowlist_sha256: Option<String>,
    pub canary_max_submissions: i64,
    pub canary_account_cap: i64,
    pub canary_concurrency_cap: i64,
    pub canary_daily_side_effect_cap: i64,
    pub canary_evidence_manifest_sha256: Option<String>,
    pub approval_ref: String,
    pub issued_at_ms: i64,
    pub not_before_ms: i64,
    pub expires_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AtsCertificationRevocationAuthority {
    pub version: i64,
    pub audience: String,
    pub revocation_id: String,
    pub policy_sha256: String,
    pub revocation_generation: i64,
    pub predecessor_revocation_sha256: Option<String>,
    pub subject_kind: String,
    pub subject_id: String,
    pub subject_sha256: String,
    pub reason_ref: String,
    pub issued_at_ms: i64,
    pub effective_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AtsCertificationQuarantineAuthority {
    pub version: i64,
    pub audience: String,
    pub command_id: String,
    pub policy_sha256: String,
    pub command_generation: i64,
    pub scope_kind: String,
    pub scope_id: String,
    pub scope_sha256: String,
    pub command_sequence: i64,
    pub predecessor_command_sha256: Option<String>,
    pub action: String,
    pub reason_ref: String,
    pub issued_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AtsObservedSurface {
    pub variant_key: String,
    pub layout_contract_version: i64,
    pub surface_sha256: String,
}

/// Fresh, independently sourced target facts used before any employer page is
/// trusted as layout evidence. Every source must resolve to the exact target
/// derived from `canonical_url`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AtsCertificationFreshTargetEvidence {
    pub canonical_url: String,
    pub discovery_provider: String,
    pub discovery_target_key: String,
    pub discovery_observed_at_ms: i64,
    pub original_source_provider: String,
    pub original_source_target_key: String,
    pub original_source_observed_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AtsCertificationCanaryAllowlistImportRequest {
    pub schema_version: i64,
    pub allowlist_id: String,
    pub account_ids: Vec<String>,
    pub approval_ref: String,
    pub not_before_ms: i64,
    pub expires_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AtsCertificationCanaryAllowlistRevocationRequest {
    pub allowlist_sha256: String,
    pub revocation_ref: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AtsCertificationCanaryAllowlistImportResult {
    pub allowlist_id: String,
    pub allowlist_sha256: String,
    pub member_count: i64,
    pub not_before_ms: i64,
    pub expires_at_ms: i64,
    pub replayed: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AtsCertificationCanaryAllowlistRevocationResult {
    pub allowlist_sha256: String,
    pub revocation_ref: String,
    pub revoked_at_ms: i64,
    pub replayed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AtsCertificationCircuitEvent {
    pub event_id: String,
    pub scope_kind: String,
    pub subject_key: String,
    pub transition: String,
    pub trigger_kind: String,
    pub window_started_at_ms: i64,
    pub window_ended_at_ms: i64,
    pub failure_count: i64,
    pub sample_count: i64,
    pub threshold_count: i64,
    pub authority_ref: String,
    pub event_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AtsCertificationCircuitState {
    pub scope_kind: String,
    pub subject_key: String,
    pub head_revision: i64,
    pub current_event_id: String,
    pub current_event_sha256: String,
    pub state: String,
    pub replayed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AtsCertificationTargetStatusProjection {
    pub schema_version: i64,
    pub provider: String,
    pub target_key_sha256: String,
    pub status: String,
    pub adapter_version: Option<String>,
    pub manifest_sha256: Option<String>,
    pub activation_sha256: Option<String>,
    pub activation_generation: Option<i64>,
    pub layout_set_sha256: Option<String>,
    pub rollout_channel: Option<String>,
    pub runner_kinds: Vec<String>,
    pub runner_target_sha256s: Vec<String>,
    pub expires_at_ms: Option<i64>,
    pub last_verified_at_ms: Option<i64>,
    pub canary_available: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AtsCertificationPostingResolution {
    pub status: AtsCertificationTargetStatusProjection,
    pub active_binding: Option<AtsCertificationBinding>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AtsCertificationBinding {
    pub binding_version: i64,
    pub trust_policy_sha256: String,
    pub provider: String,
    pub target_key: String,
    pub allowed_provider_hosts: Vec<String>,
    pub variant_key: String,
    pub surface_sha256: String,
    pub scope_sha256: String,
    pub manifest_sha256: String,
    pub certification_id: String,
    pub manifest_generation: i64,
    pub activation_sha256: String,
    pub activation_id: String,
    pub activation_generation: i64,
    pub channel: String,
    pub channel_sequence: i64,
    pub channel_head_revision: i64,
    pub channel_transition_sha256: String,
    pub capability: String,
    pub account_allowlist_sha256: Option<String>,
    pub canary_max_submissions: i64,
    pub canary_account_cap: i64,
    pub canary_concurrency_cap: i64,
    pub canary_daily_side_effect_cap: i64,
    pub adapter_version: String,
    pub final_submit_control_id: String,
    pub adapter_bundle_sha256: String,
    pub source_commit: String,
    pub layout_contract_version: i64,
    pub layout_contract_sha256: String,
    pub layout_set_sha256: String,
    pub layout_observation_sha256s: Vec<String>,
    pub evidence_sha256s: Vec<String>,
    pub runtime_targets: Vec<AtsCertificationRuntimeTarget>,
    pub selected_runtime: Option<AtsCertificationRuntimeTarget>,
    pub not_before_ms: i64,
    pub expires_at_ms: i64,
    pub last_verified_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AtsApplicationCertificationBindingRequest {
    pub binding_id: String,
    pub account_id: String,
    pub application_id: String,
    pub run_id: String,
    pub application_attempt_id: String,
    pub browser_session_id: String,
    pub browser_profile_id: String,
    pub packet_checksum_sha256: String,
    pub auto_authorization_id: String,
    pub auto_authorization_revision: i64,
    pub auto_authorization_fingerprint_sha256: String,
    pub target_evidence: AtsCertificationFreshTargetEvidence,
    pub rollout_channel: String,
    pub runner_id: String,
    pub nonce_sha256: String,
    pub requested_expires_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "runtime_kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AtsCertificationRuntimeAttestation {
    Local {
        platform: String,
        architecture: String,
        browser_release_manifest_sha256: String,
        browser_artifact_sha256: String,
        browser_build_descriptor_sha256: String,
        automation_bundle_sha256: String,
        playwright_version: String,
        chromium_revision: String,
        chromium_executable_sha256: String,
    },
    Cloud {
        platform: String,
        architecture: String,
        runner_build_id: String,
        runner_image_sha256: String,
        automation_bundle_sha256: String,
        playwright_version: String,
        chromium_revision: String,
        chromium_executable_sha256: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct AtsCertificationPhaseAContextRequest {
    pub account_id: String,
    pub application_id: String,
    pub run_id: String,
    pub browser_session_id: String,
    pub browser_profile_id: String,
    pub runtime_attestation: AtsCertificationRuntimeAttestation,
    pub nonce_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AtsApplicationCertificationBindingAuthority {
    pub schema_version: i64,
    pub binding_id: String,
    pub account_id: String,
    pub application_id: String,
    pub run_id: String,
    pub application_attempt_id: String,
    pub browser_session_id: String,
    pub browser_profile_id: String,
    pub packet_checksum_sha256: String,
    pub auto_authorization_id: String,
    pub auto_authorization_revision: i64,
    pub auto_authorization_fingerprint_sha256: String,
    pub target_evidence: AtsCertificationFreshTargetEvidence,
    pub nonce_sha256: String,
    pub certification: AtsCertificationBinding,
    pub requested_expires_at_ms: i64,
    pub created_at_ms: i64,
    pub expires_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AtsApplicationCertificationBindingRecord {
    pub binding_sha256: String,
    pub authority: AtsApplicationCertificationBindingAuthority,
    pub phase: String,
    pub fence: i64,
    pub consumed_at_ms: Option<i64>,
    pub replayed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AtsCertificationPhaseBRequest {
    pub binding_id: String,
    pub account_id: String,
    pub application_id: String,
    pub run_id: String,
    pub application_attempt_id: String,
    pub packet_checksum_sha256: String,
    pub auto_authorization_id: String,
    pub auto_authorization_revision: i64,
    pub auto_authorization_fingerprint_sha256: String,
    pub target_evidence: AtsCertificationFreshTargetEvidence,
    pub runner_id: String,
    pub nonce_sha256: String,
    pub observed_surface: AtsObservedSurface,
    pub phase_b_request_id: String,
    pub metering_reservation_sha256: String,
    pub canary_reservation_id: String,
    pub period_key: String,
    pub expected_fence: i64,
    pub terminal_phase: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct AtsCertificationPhaseBContextRequest {
    pub account_id: String,
    pub application_id: String,
    pub run_id: String,
    pub runner_kind: String,
    pub observed_surface: AtsObservedSurface,
    pub terminal_phase: String,
}

/// Exact schema-three admission object persisted in
/// `approved_execution.admission.ats_certification`. Its snake_case transport
/// fields deliberately include the observed surface contract as part of the
/// frozen authority rather than relying on a second optional object.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct AtsFrozenCertificationAdmissionProjection {
    pub schema_version: i64,
    pub provider: String,
    pub adapter_version: String,
    pub manifest_sha256: String,
    pub activation_sha256: String,
    pub activation_generation: i64,
    pub target_key_sha256: String,
    pub layout_set_sha256: String,
    pub variant_key: String,
    pub layout_contract_version: i64,
    pub surface_sha256: String,
    pub adapter_bundle_sha256: String,
    pub runner_target_sha256s: Vec<String>,
    pub expires_at_ms: i64,
}

/// Exact schema consumed by the Jobs automation, Browser, and runner receipt
/// validators. Field names intentionally match their camelCase wire contract.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AtsCertifiedReceiptAuthority {
    pub schema_version: i64,
    pub account_id: String,
    pub application_id: String,
    pub run_id: String,
    pub provider: String,
    pub adapter: String,
    pub adapter_version: String,
    pub manifest_sha256: String,
    pub activation_sha256: String,
    pub activation_generation: i64,
    pub target_key_sha256: String,
    pub layout_set_sha256: String,
    pub layout_observation_sha256: String,
    pub observed_surface_sha256: String,
    pub adapter_bundle_sha256: String,
    pub runner_kind: String,
    pub runner_target_sha256: String,
    pub binding_sha256: String,
    pub binding_fence: i64,
    pub binding_consumed_at_ms: i64,
    pub application_attempt_id: String,
    pub phase_b_request_id: String,
    pub rollout_channel: String,
    pub canary_reservation_sha256: String,
    pub metering_reservation_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AtsCertificationPhaseBResult {
    pub ats_certified_receipt_authority: AtsCertifiedReceiptAuthority,
    pub terminal_phase: String,
}

/// A Phase B transaction may commit a safety-only denial. Callers that embed
/// Phase B in a larger irreversible-submit transaction must commit
/// `LayoutDriftQuarantined` before returning the denial, and must not write a
/// click marker, consume the application binding, or reserve capacity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AtsCertificationPhaseBTransactionOutcome {
    Authorized(Box<AtsCertificationPhaseBResult>),
    LayoutDriftQuarantined,
}

impl AtsCertificationPhaseBTransactionOutcome {
    fn into_result(self) -> Result<AtsCertificationPhaseBResult, AtsCertificationAuthorityError> {
        match self {
            Self::Authorized(result) => Ok(*result),
            Self::LayoutDriftQuarantined => Err(AtsCertificationAuthorityError::ScopeMismatch),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AtsCertificationRecoveryRequest {
    pub binding_id: String,
    pub account_id: String,
    pub application_id: String,
    pub run_id: String,
    pub application_attempt_id: String,
    pub nonce_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AtsCertificationBindingInvalidationRequest {
    pub binding_id: String,
    pub account_id: String,
    pub application_id: String,
    pub run_id: String,
    pub application_attempt_id: String,
    pub nonce_sha256: String,
    pub expected_fence: i64,
    pub invalidation_kind: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AtsCertificationImportResult {
    pub authority_kind: String,
    pub authority_id: String,
    pub authority_sha256: String,
    pub authorization_sha256: String,
    pub trust_policy_sha256: String,
    pub replayed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AtsCertificationManifestAggregateEnvelope {
    pub manifest: AtsCertificationAuthorityEnvelope,
    pub evidence: Vec<AtsCertificationAuthorityEnvelope>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AtsCertificationManifestAggregateImportResult {
    pub manifest: AtsCertificationImportResult,
    pub evidence: Vec<AtsCertificationImportResult>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AtsCertificationHeadResult {
    pub scope_sha256: String,
    pub channel: String,
    pub head_revision: i64,
    pub transition_sha256: String,
    pub activation_sha256: String,
    pub channel_sequence: i64,
    pub replayed: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AtsCertificationQuarantineHeadResult {
    pub scope_kind: String,
    pub scope_id: String,
    pub scope_sha256: String,
    pub head_revision: i64,
    pub command_sha256: String,
    pub command_sequence: i64,
    pub state: String,
    pub replayed: bool,
}

#[derive(Debug, Error)]
pub enum AtsCertificationAuthorityError {
    #[error("invalid ATS certification authority envelope")]
    InvalidEnvelope,
    #[error("invalid ATS certification authority")]
    InvalidAuthority,
    #[error("invalid ATS certification trust anchor")]
    InvalidTrustAnchor,
    #[error("invalid ATS certification trust policy")]
    InvalidTrustPolicy,
    #[error("ATS certification trust policy was not initialized")]
    TrustPolicyNotInitialized,
    #[error("ATS certification signature is invalid")]
    InvalidSignature,
    #[error("ATS certification signature threshold was not met")]
    SignatureThresholdNotMet,
    #[error("ATS certification authority was not found")]
    NotFound,
    #[error("ATS certification identity conflicts with stored authority")]
    IdentityConflict,
    #[error("ATS certification compare-and-swap failed")]
    CompareAndSwapConflict,
    #[error("ATS certification sequence regressed")]
    SequenceRegression,
    #[error("ATS certification is outside its validity window")]
    Expired,
    #[error("ATS certification authority is revoked")]
    Revoked,
    #[error("ATS certification authority is quarantined")]
    Quarantined,
    #[error("ATS certification circuit is open")]
    CircuitOpen,
    #[error("ATS certification canary capacity is unavailable")]
    CapacityUnavailable,
    #[error("synthetic-only evidence cannot authorize ATS submission")]
    SyntheticEvidence,
    #[error("ATS certification scope does not match")]
    ScopeMismatch,
    #[error("ATS certification runtime does not match")]
    RuntimeMismatch,
    #[error("unsupported ATS certification URL")]
    UnsupportedUrl,
    #[error("ATS certification storage failed: {0}")]
    Storage(#[source] anyhow::Error),
}

fn ats_certification_storage(error: impl Into<anyhow::Error>) -> AtsCertificationAuthorityError {
    AtsCertificationAuthorityError::Storage(error.into())
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct AtsCertificationAuthorizationPayload<'a> {
    version: i64,
    audience: &'static str,
    authorization_id: &'a str,
    role: &'a str,
    target_audience: &'a str,
    target_sha256: &'a str,
    signed_at_ms: i64,
}

#[derive(Debug, Clone)]
struct VerifiedAtsCertificationEnvelope<T> {
    authority: T,
    authority_sha256: String,
    authorization_sha256: String,
    trust_policy_sha256: String,
}

fn ats_certification_canonical_json<T: Serialize>(value: &T) -> serde_json::Result<Vec<u8>> {
    let mut bytes = serde_json::to_vec(value)?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn ats_certification_sha256(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

pub fn ats_certification_scope_sha256(
    provider: &str,
    target_key: &str,
    variant_key: &str,
    surface_sha256: &str,
) -> Result<String, AtsCertificationAuthorityError> {
    if !ats_certification_provider(provider)
        || !ats_certification_text(target_key, 1, 240)
        || !ats_certification_token(variant_key, 1, 120)
        || !ats_certification_hex64(surface_sha256)
    {
        return Err(AtsCertificationAuthorityError::InvalidAuthority);
    }
    let value = serde_json::json!({
        "provider": provider,
        "surfaceSha256": surface_sha256,
        "targetKey": target_key,
        "variantKey": variant_key,
    });
    let bytes = ats_certification_canonical_json(&value)
        .map_err(|_| AtsCertificationAuthorityError::InvalidAuthority)?;
    Ok(ats_certification_sha256(&bytes))
}

pub fn ats_certification_target_from_url(
    canonical_url: &str,
) -> Result<(String, String), AtsCertificationAuthorityError> {
    let target = crate::jobs_ats_target::parse_provider_application_target(
        canonical_url,
        crate::jobs_ats_target::ProviderApplicationTargetPurpose::Submit,
    )
    .ok_or(AtsCertificationAuthorityError::UnsupportedUrl)?;
    Ok((target.provider.to_string(), target.provider_job_key))
}

fn ats_certification_binding_matches_url(
    binding: &AtsCertificationBinding,
    canonical_url: &str,
) -> bool {
    crate::jobs_ats_target::parse_provider_application_target(
        canonical_url,
        crate::jobs_ats_target::ProviderApplicationTargetPurpose::Submit,
    )
    .is_some_and(|target| {
        binding.provider == target.provider
            && binding.target_key == target.provider_job_key
            && binding
                .allowed_provider_hosts
                .binary_search(&target.host)
                .is_ok()
            && ats_exact_final_submit_control(&binding.provider, &binding.final_submit_control_id)
    })
}

/// Builds the only target-evidence shape accepted by ATS certification from a
/// server-loaded posting. Caller-supplied provider, target, and timestamps are
/// intentionally not inputs.
pub fn ats_certification_fresh_target_evidence_from_posting(
    posting: &JobPosting,
    now_ms: i64,
) -> Result<AtsCertificationFreshTargetEvidence, AtsCertificationAuthorityError> {
    let target = crate::jobs_ats_target::parse_provider_application_target(
        &posting.canonical_url,
        crate::jobs_ats_target::ProviderApplicationTargetPurpose::Submit,
    )
    .ok_or(AtsCertificationAuthorityError::UnsupportedUrl)?;
    let evidence = &posting.discovery_evidence;
    let discovery_observed_at_ms = posting
        .last_verified_at_ms
        .ok_or(AtsCertificationAuthorityError::InvalidAuthority)?;
    let original_source_observed_at_ms = evidence
        .original_source_checked_at_ms
        .ok_or(AtsCertificationAuthorityError::InvalidAuthority)?;
    let evidence_domain = evidence
        .application_domain
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_ascii_lowercase);
    let external_target_matches = if reqwest::Url::parse(&posting.external_id).is_ok() {
        crate::jobs_ats_target::parse_provider_application_target(
            &posting.external_id,
            crate::jobs_ats_target::ProviderApplicationTargetPurpose::Submit,
        )
        .is_some_and(|external| {
            external.provider == target.provider
                && external.host == target.host
                && external.provider_job_key == target.provider_job_key
        })
    } else {
        true
    };
    if posting.source != target.provider
        || !external_target_matches
        || posting.canonical_key.trim().is_empty()
        || evidence.provenance != "original_source"
        || evidence.canonical_status != "canonical"
        || evidence.canonical_job_id.as_deref() != Some(posting.canonical_key.as_str())
        || evidence.original_source_status != "verified_open"
        || evidence.requires_original_revalidation
        || !evidence.original_source_mismatched_fields.is_empty()
        || evidence
            .original_source_snapshot_expires_at_ms
            .is_none_or(|expires_at_ms| expires_at_ms <= now_ms)
        || evidence
            .original_source_evidence_hash
            .as_deref()
            .is_none_or(|value| !ats_certification_hex64(value))
        || evidence_domain.as_deref() != Some(target.host.as_str())
    {
        return Err(AtsCertificationAuthorityError::ScopeMismatch);
    }
    let target_evidence = AtsCertificationFreshTargetEvidence {
        canonical_url: posting.canonical_url.clone(),
        discovery_provider: target.provider.to_string(),
        discovery_target_key: target.provider_job_key.clone(),
        discovery_observed_at_ms,
        original_source_provider: target.provider.to_string(),
        original_source_target_key: target.provider_job_key,
        original_source_observed_at_ms,
    };
    validate_ats_certification_fresh_target_evidence(&target_evidence, now_ms)?;
    Ok(target_evidence)
}

fn validate_ats_certification_fresh_target_evidence(
    evidence: &AtsCertificationFreshTargetEvidence,
    now_ms: i64,
) -> Result<(String, String), AtsCertificationAuthorityError> {
    if !ats_certification_safe_integer(now_ms, false)
        || !ats_certification_text(&evidence.canonical_url, 1, 2_048)
        || !ats_certification_safe_integer(evidence.discovery_observed_at_ms, false)
        || !ats_certification_safe_integer(evidence.original_source_observed_at_ms, false)
        || evidence.discovery_observed_at_ms > now_ms
        || evidence.original_source_observed_at_ms > now_ms
        || now_ms - evidence.discovery_observed_at_ms
            > ATS_CERTIFICATION_TARGET_EVIDENCE_FRESHNESS_MS
        || now_ms - evidence.original_source_observed_at_ms
            > ATS_CERTIFICATION_TARGET_EVIDENCE_FRESHNESS_MS
    {
        return Err(AtsCertificationAuthorityError::InvalidAuthority);
    }
    let (provider, target_key) = ats_certification_target_from_url(&evidence.canonical_url)?;
    if evidence.discovery_provider != provider
        || evidence.original_source_provider != provider
        || evidence.discovery_target_key != target_key
        || evidence.original_source_target_key != target_key
    {
        return Err(AtsCertificationAuthorityError::ScopeMismatch);
    }
    Ok((provider, target_key))
}

pub fn ats_certification_target_key_sha256(
    target_key: &str,
) -> Result<String, AtsCertificationAuthorityError> {
    if !ats_certification_text(target_key, 1, 240) {
        return Err(AtsCertificationAuthorityError::InvalidAuthority);
    }
    Ok(ats_certification_sha256(target_key.as_bytes()))
}

fn validate_ats_certification_trust_anchor(
    anchor: &AtsCertificationTrustAnchor,
) -> Result<(), AtsCertificationAuthorityError> {
    let required_roles = [
        "manifest",
        "evidence",
        "revocation",
        "layout_observation",
        "activation",
    ];
    if anchor.roles.len() != required_roles.len()
        || required_roles
            .iter()
            .any(|role| !anchor.roles.contains_key(*role))
    {
        return Err(AtsCertificationAuthorityError::InvalidTrustAnchor);
    }
    let mut key_ids = BTreeSet::new();
    let mut public_keys = BTreeSet::new();
    for (role, trust) in &anchor.roles {
        if !required_roles.contains(&role.as_str())
            || trust.keys.is_empty()
            || trust.keys.len() > 16
            || trust.threshold < 1
            || trust.threshold as usize > trust.keys.len()
        {
            return Err(AtsCertificationAuthorityError::InvalidTrustAnchor);
        }
        for (key_id, public_key) in &trust.keys {
            if !ats_certification_token(key_id, 1, 120)
                || ats_certification_decode_base64url_exact(public_key, 32).is_err()
                || !key_ids.insert(key_id.clone())
                || !public_keys.insert(public_key.clone())
            {
                return Err(AtsCertificationAuthorityError::InvalidTrustAnchor);
            }
        }
    }
    Ok(())
}

fn validate_ats_certification_root_trust_anchor(
    anchor: &AtsCertificationRootTrustAnchor,
) -> Result<(), AtsCertificationAuthorityError> {
    if anchor.keys.is_empty()
        || anchor.keys.len() > 16
        || anchor.threshold < 1
        || anchor.threshold as usize > anchor.keys.len()
    {
        return Err(AtsCertificationAuthorityError::InvalidTrustAnchor);
    }
    let mut public_keys = BTreeSet::new();
    for (key_id, public_key) in &anchor.keys {
        if !ats_certification_token(key_id, 1, 120)
            || ats_certification_decode_base64url_exact(public_key, 32).is_err()
            || !public_keys.insert(public_key)
        {
            return Err(AtsCertificationAuthorityError::InvalidTrustAnchor);
        }
    }
    Ok(())
}

fn ats_certification_root_anchor_sha256(
    anchor: &AtsCertificationRootTrustAnchor,
) -> Result<String, AtsCertificationAuthorityError> {
    validate_ats_certification_root_trust_anchor(anchor)?;
    let canonical = ats_certification_canonical_json(anchor)
        .map_err(|_| AtsCertificationAuthorityError::InvalidTrustAnchor)?;
    Ok(ats_certification_sha256(&canonical))
}

fn validate_ats_certification_trust_policy(
    policy: &AtsCertificationTrustPolicyAuthority,
    root_anchor: &AtsCertificationRootTrustAnchor,
) -> Result<(), AtsCertificationAuthorityError> {
    validate_ats_certification_root_trust_anchor(root_anchor)?;
    validate_ats_certification_trust_anchor(&policy.delegated_trust)?;
    let root_key_ids = root_anchor.keys.keys().collect::<BTreeSet<_>>();
    let root_public_keys = root_anchor.keys.values().collect::<BTreeSet<_>>();
    let delegated_overlap = policy.delegated_trust.roles.values().any(|role| {
        role.keys.keys().any(|key_id| root_key_ids.contains(key_id))
            || role
                .keys
                .values()
                .any(|public_key| root_public_keys.contains(public_key))
    });
    let requirements = &policy.certification_requirements;
    if policy.version != 1
        || policy.audience != ATS_CERTIFICATION_TRUST_POLICY_AUDIENCE
        || !ats_certification_token(&policy.policy_id, 1, 120)
        || !ats_certification_safe_integer(policy.trust_generation, true)
        || policy
            .predecessor_policy_sha256
            .as_deref()
            .is_some_and(|value| !ats_certification_hex64(value))
        || !ats_certification_safe_integer(policy.issued_at_ms, false)
        || policy.valid_from_ms < policy.issued_at_ms
        || policy.expires_at_ms <= policy.valid_from_ms
        || !ats_certification_safe_integer(policy.expires_at_ms, true)
        || requirements.allowed_providers.is_empty()
        || requirements.allowed_providers.len() > 5
        || !requirements
            .allowed_providers
            .iter()
            .all(|provider| ats_certification_provider(provider))
        || !requirements
            .allowed_providers
            .windows(2)
            .all(|pair| pair[0] < pair[1])
        || !ats_certification_token(&requirements.suite_id, 1, 120)
        || requirements.required_check_ids
            != ATS_CERTIFICATION_REQUIRED_CHECK_IDS.map(str::to_string)
        || requirements.required_evidence_classes.is_empty()
        || requirements.required_evidence_classes.len() > 3
        || !requirements
            .required_evidence_classes
            .iter()
            .all(|value| ats_certification_evidence_class(value))
        || !requirements
            .required_evidence_classes
            .windows(2)
            .all(|pair| pair[0] < pair[1])
        || !(60_000..=31_536_000_000).contains(&requirements.maximum_manifest_lifetime_ms)
        || !(0..=300_000).contains(&requirements.maximum_clock_skew_ms)
        || !(1_024..=ATS_CERTIFICATION_MAX_CANONICAL_BYTES as i64)
            .contains(&requirements.maximum_manifest_size_bytes)
        || !(1..=32).contains(&requirements.maximum_target_count)
        || !(1..=64).contains(&requirements.maximum_observation_count)
        || !(1..=64).contains(&requirements.maximum_evidence_object_count)
        || delegated_overlap
    {
        return Err(AtsCertificationAuthorityError::InvalidTrustPolicy);
    }
    Ok(())
}

fn verify_ats_certification_envelope<T>(
    envelope: &AtsCertificationAuthorityEnvelope,
    trust_anchor: &AtsCertificationTrustAnchor,
    role: &str,
    target_audience: &str,
    issued_at_ms: i64,
    verification_time_ms: i64,
) -> Result<VerifiedAtsCertificationEnvelope<T>, AtsCertificationAuthorityError>
where
    T: DeserializeOwned + Serialize,
{
    validate_ats_certification_trust_anchor(trust_anchor)?;
    let trust_policy_sha256 = trust_anchor.sha256()?;
    let trust = trust_anchor
        .roles
        .get(role)
        .ok_or(AtsCertificationAuthorityError::InvalidTrustAnchor)?;
    verify_ats_certification_envelope_with_role(
        envelope,
        trust,
        role,
        target_audience,
        issued_at_ms,
        verification_time_ms,
        trust_policy_sha256,
    )
}

fn verify_ats_certification_envelope_with_role<T>(
    envelope: &AtsCertificationAuthorityEnvelope,
    trust: &AtsCertificationTrustRole,
    role: &str,
    target_audience: &str,
    issued_at_ms: i64,
    verification_time_ms: i64,
    trust_policy_sha256: String,
) -> Result<VerifiedAtsCertificationEnvelope<T>, AtsCertificationAuthorityError>
where
    T: DeserializeOwned + Serialize,
{
    let authority_bytes = ats_certification_decode_base64url_bounded(
        &envelope.canonical_base64url,
        ATS_CERTIFICATION_MAX_CANONICAL_BYTES,
    )?;
    let authority: T = ats_certification_parse_canonical_json(&authority_bytes)?;
    let authorization_bytes = ats_certification_decode_base64url_bounded(
        &envelope.authorization_base64url,
        ATS_CERTIFICATION_MAX_CANONICAL_BYTES,
    )?;
    let authorization: AtsCertificationAuthorization =
        ats_certification_parse_canonical_json(&authorization_bytes)?;
    let authority_sha256 = ats_certification_sha256(&authority_bytes);
    if issued_at_ms < 0
        || issued_at_ms > verification_time_ms
        || authorization.version != 1
        || authorization.audience != ATS_CERTIFICATION_AUTHORIZATION_AUDIENCE
        || !ats_certification_token(&authorization.authorization_id, 1, 120)
        || authorization.role != role
        || authorization.target_audience != target_audience
        || authorization.target_sha256 != authority_sha256
        || authorization.signed_at_ms < issued_at_ms
        || authorization.signed_at_ms > verification_time_ms
        || authorization.signatures.is_empty()
        || authorization.signatures.len() > 16
        || !authorization
            .signatures
            .windows(2)
            .all(|pair| pair[0].key_id < pair[1].key_id)
    {
        return Err(AtsCertificationAuthorityError::InvalidAuthority);
    }
    let payload = AtsCertificationAuthorizationPayload {
        version: authorization.version,
        audience: ATS_CERTIFICATION_AUTHORIZATION_AUDIENCE,
        authorization_id: &authorization.authorization_id,
        role,
        target_audience,
        target_sha256: &authority_sha256,
        signed_at_ms: authorization.signed_at_ms,
    };
    let payload_bytes = ats_certification_canonical_json(&payload)
        .map_err(|_| AtsCertificationAuthorityError::InvalidAuthority)?;
    let mut verified = 0_i64;
    for detached in &authorization.signatures {
        let Some(encoded_key) = trust.keys.get(&detached.key_id) else {
            return Err(AtsCertificationAuthorityError::InvalidSignature);
        };
        let key: [u8; 32] = ats_certification_decode_base64url_exact(encoded_key, 32)?
            .try_into()
            .map_err(|_| AtsCertificationAuthorityError::InvalidSignature)?;
        let signature: [u8; 64] =
            ats_certification_decode_base64url_exact(&detached.signature, 64)?
                .try_into()
                .map_err(|_| AtsCertificationAuthorityError::InvalidSignature)?;
        let verifying_key = ed25519_dalek::VerifyingKey::from_bytes(&key)
            .map_err(|_| AtsCertificationAuthorityError::InvalidSignature)?;
        use ed25519_dalek::Verifier as _;
        verifying_key
            .verify(
                &payload_bytes,
                &ed25519_dalek::Signature::from_bytes(&signature),
            )
            .map_err(|_| AtsCertificationAuthorityError::InvalidSignature)?;
        verified += 1;
    }
    if verified < trust.threshold {
        return Err(AtsCertificationAuthorityError::SignatureThresholdNotMet);
    }
    Ok(VerifiedAtsCertificationEnvelope {
        authority,
        authority_sha256,
        authorization_sha256: ats_certification_sha256(&authorization_bytes),
        trust_policy_sha256,
    })
}

fn verify_ats_certification_trust_policy_envelope(
    envelope: &AtsCertificationAuthorityEnvelope,
    root_anchor: &AtsCertificationRootTrustAnchor,
    verification_time_ms: i64,
) -> Result<
    VerifiedAtsCertificationEnvelope<AtsCertificationTrustPolicyAuthority>,
    AtsCertificationAuthorityError,
> {
    let bytes = ats_certification_decode_base64url_bounded(
        &envelope.canonical_base64url,
        ATS_CERTIFICATION_MAX_CANONICAL_BYTES,
    )?;
    let policy: AtsCertificationTrustPolicyAuthority =
        ats_certification_parse_canonical_json(&bytes)?;
    validate_ats_certification_trust_policy(&policy, root_anchor)?;
    if verification_time_ms < policy.valid_from_ms || verification_time_ms >= policy.expires_at_ms {
        return Err(AtsCertificationAuthorityError::Expired);
    }
    let root_role = AtsCertificationTrustRole {
        threshold: root_anchor.threshold,
        keys: root_anchor.keys.clone(),
    };
    verify_ats_certification_envelope_with_role(
        envelope,
        &root_role,
        "root",
        ATS_CERTIFICATION_TRUST_POLICY_AUDIENCE,
        policy.issued_at_ms,
        verification_time_ms,
        ats_certification_root_anchor_sha256(root_anchor)?,
    )
}

fn ats_certification_parse_canonical_json<T>(
    bytes: &[u8],
) -> Result<T, AtsCertificationAuthorityError>
where
    T: DeserializeOwned + Serialize,
{
    if bytes.is_empty() || bytes.len() > ATS_CERTIFICATION_MAX_CANONICAL_BYTES {
        return Err(AtsCertificationAuthorityError::InvalidEnvelope);
    }
    let value: T = serde_json::from_slice(bytes)
        .map_err(|_| AtsCertificationAuthorityError::InvalidAuthority)?;
    let canonical = ats_certification_canonical_json(&value)
        .map_err(|_| AtsCertificationAuthorityError::InvalidAuthority)?;
    if canonical != bytes {
        return Err(AtsCertificationAuthorityError::InvalidAuthority);
    }
    Ok(value)
}

fn ats_certification_decode_base64url_bounded(
    value: &str,
    maximum_bytes: usize,
) -> Result<Vec<u8>, AtsCertificationAuthorityError> {
    if value.is_empty() || value.len() > maximum_bytes.saturating_mul(2) {
        return Err(AtsCertificationAuthorityError::InvalidEnvelope);
    }
    let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(value)
        .map_err(|_| AtsCertificationAuthorityError::InvalidEnvelope)?;
    if decoded.is_empty()
        || decoded.len() > maximum_bytes
        || base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&decoded) != value
    {
        return Err(AtsCertificationAuthorityError::InvalidEnvelope);
    }
    Ok(decoded)
}

fn ats_certification_decode_base64url_exact(
    value: &str,
    exact_bytes: usize,
) -> Result<Vec<u8>, AtsCertificationAuthorityError> {
    let decoded = ats_certification_decode_base64url_bounded(value, exact_bytes)?;
    if decoded.len() != exact_bytes {
        return Err(AtsCertificationAuthorityError::InvalidEnvelope);
    }
    Ok(decoded)
}

fn ats_certification_safe_integer(value: i64, positive: bool) -> bool {
    value >= i64::from(positive) && value <= ATS_CERTIFICATION_MAX_SAFE_INTEGER
}

fn ats_certification_text(value: &str, minimum: usize, maximum: usize) -> bool {
    value.len() >= minimum
        && value.len() <= maximum
        && value.trim() == value
        && !value
            .chars()
            .any(|character| matches!(character, '\0'..='\u{1f}' | '\u{7f}'))
}

fn ats_certification_token(value: &str, minimum: usize, maximum: usize) -> bool {
    ats_certification_text(value, minimum, maximum)
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"-_.:".contains(&byte)
        })
        && !value.contains("..")
        && !value.contains('*')
}

fn ats_certification_provider_host(value: &str) -> bool {
    value.len() <= 253
        && value == value.to_ascii_lowercase()
        && value.contains('.')
        && value.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        })
}

fn ats_certification_hex64(value: &str) -> bool {
    value.len() == 64
        && value == value.to_ascii_lowercase()
        && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn ats_certification_provider(value: &str) -> bool {
    matches!(
        value,
        "ashby" | "greenhouse" | "lever" | "smartrecruiters" | "workday"
    )
}

fn ats_exact_final_submit_control(provider: &str, control_id: &str) -> bool {
    matches!(
        (provider, control_id),
        ("greenhouse", ATS_GREENHOUSE_FINAL_SUBMIT_CONTROL_ID)
            | ("lever", ATS_LEVER_FINAL_SUBMIT_CONTROL_ID)
    )
}

fn ats_manifest_provider_hosts_match_target(
    provider: &str,
    target_key: &str,
    allowed_provider_hosts: &[String],
) -> bool {
    if allowed_provider_hosts.is_empty()
        || allowed_provider_hosts.len() > 8
        || !allowed_provider_hosts
            .iter()
            .all(|host| ats_certification_provider_host(host))
        || !allowed_provider_hosts
            .windows(2)
            .all(|pair| pair[0] < pair[1])
    {
        return false;
    }
    match provider {
        "greenhouse" => {
            allowed_provider_hosts
                == ATS_GREENHOUSE_ALLOWED_PROVIDER_HOSTS
                    .map(str::to_string)
                    .as_slice()
        }
        "lever" => {
            let pieces = target_key.split(':').collect::<Vec<_>>();
            pieces.len() == 4
                && allowed_provider_hosts.len() == 1
                && allowed_provider_hosts[0] == pieces[1]
                && matches!(pieces[1], "jobs.lever.co" | "jobs.eu.lever.co")
        }
        _ => true,
    }
}

fn ats_certification_evidence_class(value: &str) -> bool {
    matches!(
        value,
        "authorized_live" | "authorized_sandbox" | "synthetic"
    )
}

fn ats_certification_layout_set_sha256(
    observations: &[String],
) -> Result<String, AtsCertificationAuthorityError> {
    if observations.is_empty()
        || observations.len() > 64
        || !observations
            .iter()
            .all(|value| ats_certification_hex64(value))
        || !observations.windows(2).all(|pair| pair[0] < pair[1])
    {
        return Err(AtsCertificationAuthorityError::InvalidAuthority);
    }
    let value = serde_json::json!({ "layoutObservationSha256s": observations });
    let canonical = ats_certification_canonical_json(&value)
        .map_err(|_| AtsCertificationAuthorityError::InvalidAuthority)?;
    Ok(ats_certification_sha256(&canonical))
}

fn ats_certification_target_fingerprint_sha256(
    provider: &str,
    target_key: &str,
) -> Result<String, AtsCertificationAuthorityError> {
    if !ats_certification_provider(provider) || !ats_certification_text(target_key, 1, 240) {
        return Err(AtsCertificationAuthorityError::InvalidAuthority);
    }
    let value = serde_json::json!({ "provider": provider, "targetKey": target_key });
    let canonical = ats_certification_canonical_json(&value)
        .map_err(|_| AtsCertificationAuthorityError::InvalidAuthority)?;
    Ok(ats_certification_sha256(&canonical))
}

fn ats_certification_capability(value: &str) -> bool {
    matches!(
        value,
        "observe_only" | "reviewed_submit" | "unattended_submit"
    )
}

fn ats_certification_capability_rank(value: &str) -> Option<u8> {
    match value {
        "observe_only" => Some(0),
        "reviewed_submit" => Some(1),
        "unattended_submit" => Some(2),
        _ => None,
    }
}

fn validate_ats_observed_surface(
    surface: &AtsObservedSurface,
) -> Result<(), AtsCertificationAuthorityError> {
    if !ats_certification_token(&surface.variant_key, 1, 120)
        || !ats_certification_safe_integer(surface.layout_contract_version, true)
        || !ats_certification_hex64(&surface.surface_sha256)
    {
        return Err(AtsCertificationAuthorityError::InvalidAuthority);
    }
    Ok(())
}

fn validate_ats_runtime_target(
    runtime: &AtsCertificationRuntimeTarget,
) -> Result<(), AtsCertificationAuthorityError> {
    if !matches!(runtime.runtime_kind.as_str(), "cloud" | "local")
        || !ats_certification_token(&runtime.runtime_id, 1, 160)
        || !runtime
            .runtime_id
            .starts_with(&format!("{}:", runtime.runtime_kind))
        || !ats_certification_hex64(&runtime.runtime_sha256)
        || !matches!(runtime.platform.as_str(), "linux" | "macos" | "windows")
        || !matches!(runtime.architecture.as_str(), "arm64" | "x86_64")
        || !ats_certification_hex64(&runtime.automation_bundle_sha256)
        || !ats_certification_text(&runtime.playwright_version, 1, 80)
        || !ats_certification_text(&runtime.chromium_revision, 1, 80)
        || !ats_certification_hex64(&runtime.chromium_executable_sha256)
    {
        return Err(AtsCertificationAuthorityError::InvalidAuthority);
    }
    match runtime.runtime_kind.as_str() {
        "local"
            if runtime
                .browser_release_manifest_sha256
                .as_deref()
                .is_some_and(ats_certification_hex64)
                && runtime
                    .browser_artifact_sha256
                    .as_deref()
                    .is_some_and(ats_certification_hex64)
                && runtime
                    .browser_build_descriptor_sha256
                    .as_deref()
                    .is_some_and(ats_certification_hex64)
                && runtime.runner_build_id.is_none()
                && runtime.runner_image_sha256.is_none() => {}
        "cloud"
            if runtime.browser_release_manifest_sha256.is_none()
                && runtime.browser_artifact_sha256.is_none()
                && runtime.browser_build_descriptor_sha256.is_none()
                && runtime
                    .runner_build_id
                    .as_deref()
                    .is_some_and(|value| ats_certification_token(value, 1, 160))
                && runtime
                    .runner_image_sha256
                    .as_deref()
                    .is_some_and(ats_certification_hex64) => {}
        _ => return Err(AtsCertificationAuthorityError::InvalidAuthority),
    }
    Ok(())
}

fn validate_ats_evidence_authority(
    evidence: &AtsCertificationEvidenceAuthority,
) -> Result<(), AtsCertificationAuthorityError> {
    if evidence.version != 1
        || evidence.audience != ATS_CERTIFICATION_EVIDENCE_AUDIENCE
        || !ats_certification_token(&evidence.evidence_id, 1, 120)
        || !ats_certification_hex64(&evidence.policy_sha256)
        || !ats_certification_provider(&evidence.provider)
        || !ats_certification_text(&evidence.target_key, 1, 240)
        || (matches!(evidence.provider.as_str(), "greenhouse" | "lever")
            && !ats_exact_provider_target_key(&evidence.provider, &evidence.target_key))
        || !ats_certification_token(&evidence.variant_key, 1, 120)
        || !ats_certification_hex64(&evidence.surface_sha256)
        || !matches!(
            evidence.source_kind.as_str(),
            "authorized_canary" | "authorized_sandbox" | "fault_injection" | "synthetic"
        )
        || !ats_certification_text(&evidence.object_key, 1, 512)
        || evidence.object_key.starts_with('/')
        || evidence.object_key.contains("..")
        || evidence.object_key.contains("://")
        || !ats_certification_hex64(&evidence.object_sha256)
        || !(1..=1_073_741_824).contains(&evidence.object_size_bytes)
        || !ats_certification_text(&evidence.media_type, 1, 120)
        || !ats_certification_hex64(&evidence.provenance_sha256)
        || !ats_certification_text(&evidence.authorization_ref, 1, 240)
        || !ats_certification_token(&evidence.sanitizer_version, 1, 80)
        || !ats_certification_safe_integer(evidence.captured_at_ms, false)
        || evidence.issued_at_ms < evidence.captured_at_ms
        || evidence.expires_at_ms <= evidence.issued_at_ms
        || !ats_certification_safe_integer(evidence.expires_at_ms, true)
    {
        return Err(AtsCertificationAuthorityError::InvalidAuthority);
    }
    Ok(())
}

fn validate_ats_layout_observation_privacy(
    observation: &AtsLayoutObservationAuthority,
) -> Result<(), AtsCertificationAuthorityError> {
    if observation.version != 1
        || observation.audience != ATS_CERTIFICATION_LAYOUT_OBSERVATION_AUDIENCE
        || !ats_certification_token(&observation.observation_id, 1, 120)
        || !ats_certification_hex64(&observation.policy_sha256)
        || !ats_certification_provider(&observation.provider)
        || !ats_certification_hex64(&observation.target_fingerprint_sha256)
        || !ats_certification_token(&observation.page_variant, 1, 120)
        || !ats_certification_hex64(&observation.surface_sha256)
        || !ats_certification_text(&observation.adapter_version, 1, 120)
        || !ats_certification_hex64(&observation.runner_target_sha256)
        || !ats_certification_evidence_class(&observation.evidence_class)
        || observation.controls.is_empty()
        || observation.controls.len() > 128
        || !observation.controls.windows(2).all(|pair| {
            (&pair[0].provider_attribute_sha256, &pair[0].control_kind)
                < (&pair[1].provider_attribute_sha256, &pair[1].control_kind)
        })
        || observation.controls.iter().any(|control| {
            !matches!(
                control.control_kind.as_str(),
                "checkbox"
                    | "file"
                    | "hidden"
                    | "radio"
                    | "select"
                    | "submit"
                    | "text"
                    | "textarea"
            ) || !ats_certification_hex64(&control.provider_attribute_sha256)
                || !(0..=256).contains(&control.option_count)
                || control
                    .conditional_on_attribute_sha256
                    .as_deref()
                    .is_some_and(|value| !ats_certification_hex64(value))
        })
        || !matches!(observation.form.method.as_str(), "post")
        || !matches!(
            observation.form.encoding.as_str(),
            "application/x-www-form-urlencoded" | "multipart/form-data"
        )
        || !ats_certification_hex64(&observation.form.target_sha256)
        || !ats_certification_hex64(&observation.form.action_identity_sha256)
        || !ats_certification_hex64(&observation.form.submit_control_sha256)
        || observation.challenge_categories.len() > 16
        || !observation
            .challenge_categories
            .iter()
            .all(|value| ATS_LAYOUT_CHALLENGE_CATEGORIES.contains(&value.as_str()))
        || !observation
            .challenge_categories
            .windows(2)
            .all(|pair| pair[0] < pair[1])
        || !(1..=32).contains(&observation.step_count)
        || observation.confirmation_state_categories.is_empty()
        || observation.confirmation_state_categories.len() > 16
        || !observation
            .confirmation_state_categories
            .iter()
            .all(|value| ATS_LAYOUT_CONFIRMATION_STATE_CATEGORIES.contains(&value.as_str()))
        || !observation
            .confirmation_state_categories
            .windows(2)
            .all(|pair| pair[0] < pair[1])
        || observation
            .predecessor_observation_sha256
            .as_deref()
            .is_some_and(|value| !ats_certification_hex64(value))
        || !ats_certification_safe_integer(observation.observed_at_ms, false)
        || observation.issued_at_ms < observation.observed_at_ms
        || observation.expires_at_ms <= observation.issued_at_ms
        || !ats_certification_safe_integer(observation.expires_at_ms, true)
    {
        return Err(AtsCertificationAuthorityError::InvalidAuthority);
    }
    Ok(())
}

fn validate_ats_manifest_certification_profile(
    manifest: &AtsCertificationManifestAuthority,
) -> Result<(), AtsCertificationAuthorityError> {
    let profile = &manifest.certification_profile;
    let zero = &profile.zero_tolerance;
    if !ats_certification_token(&profile.suite_id, 1, 120)
        || !ats_certification_token(&profile.suite_version, 1, 80)
        || !ats_certification_hex64(&profile.suite_manifest_sha256)
        || profile.layout_observation_sha256s.is_empty()
        || profile.layout_observation_sha256s.len() > 64
        || ats_certification_layout_set_sha256(&profile.layout_observation_sha256s)?
            != profile.layout_set_sha256
        || profile.check_results.is_empty()
        || profile.check_results.len() > 1_536
        || !profile.check_results.windows(2).all(|pair| {
            (
                &pair[0].runner_target_sha256,
                &pair[0].check_id,
                &pair[0].evidence_class,
            ) < (
                &pair[1].runner_target_sha256,
                &pair[1].check_id,
                &pair[1].evidence_class,
            )
        })
        || profile.check_results.iter().any(|result| {
            !ats_certification_hex64(&result.runner_target_sha256)
                || !ATS_CERTIFICATION_REQUIRED_CHECK_IDS.contains(&result.check_id.as_str())
                || !ats_certification_evidence_class(&result.evidence_class)
                || !ats_certification_safe_integer(result.passed_count, true)
                || result.failed_count != 0
                || result.skipped_count != 0
        })
        || [
            zero.hard_filter_violations,
            zero.unsupported_factual_claims,
            zero.duplicate_submit_activations,
            zero.false_submitted_states,
            zero.incomplete_or_mismatched_receipts,
            zero.pii_bearing_observations,
        ]
        .into_iter()
        .any(|count| count != 0)
    {
        return Err(AtsCertificationAuthorityError::InvalidAuthority);
    }
    Ok(())
}

fn require_ats_manifest_policy(
    manifest: &AtsCertificationManifestAuthority,
    policy: &AtsCertificationTrustPolicyAuthority,
    verification_time_ms: i64,
) -> Result<(), AtsCertificationAuthorityError> {
    let requirements = &policy.certification_requirements;
    let profile = &manifest.certification_profile;
    if !requirements.allowed_providers.contains(&manifest.provider)
        || profile.suite_id != requirements.suite_id
        || profile.layout_observation_sha256s.len() as i64 > requirements.maximum_observation_count
        || manifest.runtime_targets.len() as i64 > requirements.maximum_target_count
        || manifest.evidence_sha256s.len() as i64 > requirements.maximum_evidence_object_count
        || manifest.expires_at_ms - manifest.tested_at_ms
            > requirements.maximum_manifest_lifetime_ms
        || manifest.tested_at_ms > verification_time_ms
        || manifest.issued_at_ms > verification_time_ms
    {
        return Err(AtsCertificationAuthorityError::InvalidTrustPolicy);
    }
    let mut expected = Vec::new();
    for runtime in &manifest.runtime_targets {
        for check_id in &requirements.required_check_ids {
            for evidence_class in &requirements.required_evidence_classes {
                expected.push((
                    runtime.runtime_sha256.as_str(),
                    check_id.as_str(),
                    evidence_class.as_str(),
                ));
            }
        }
    }
    expected.sort_unstable();
    let actual = profile
        .check_results
        .iter()
        .map(|result| {
            (
                result.runner_target_sha256.as_str(),
                result.check_id.as_str(),
                result.evidence_class.as_str(),
            )
        })
        .collect::<Vec<_>>();
    if actual != expected {
        return Err(AtsCertificationAuthorityError::InvalidAuthority);
    }
    if manifest.maximum_capability != "observe_only"
        && requirements
            .required_evidence_classes
            .iter()
            .any(|value| value == "synthetic")
    {
        return Err(AtsCertificationAuthorityError::SyntheticEvidence);
    }
    Ok(())
}

fn validate_ats_manifest_authority(
    manifest: &AtsCertificationManifestAuthority,
) -> Result<(), AtsCertificationAuthorityError> {
    let expected_scope = ats_certification_scope_sha256(
        &manifest.provider,
        &manifest.target_key,
        &manifest.variant_key,
        &manifest.surface_sha256,
    )?;
    validate_ats_manifest_certification_profile(manifest)?;
    if manifest.version != 1
        || manifest.audience != ATS_CERTIFICATION_MANIFEST_AUDIENCE
        || !ats_certification_token(&manifest.certification_id, 1, 120)
        || !ats_certification_hex64(&manifest.policy_sha256)
        || !ats_certification_safe_integer(manifest.manifest_generation, true)
        || manifest
            .predecessor_manifest_sha256
            .as_deref()
            .is_some_and(|value| !ats_certification_hex64(value))
        || !ats_manifest_provider_hosts_match_target(
            &manifest.provider,
            &manifest.target_key,
            &manifest.allowed_provider_hosts,
        )
        || manifest.scope_sha256 != expected_scope
        || !ats_certification_text(&manifest.adapter_version, 1, 120)
        || !ats_certification_token(&manifest.final_submit_control_id, 1, 120)
        || (matches!(manifest.provider.as_str(), "greenhouse" | "lever")
            && !ats_exact_final_submit_control(
                &manifest.provider,
                &manifest.final_submit_control_id,
            ))
        || !ats_certification_hex64(&manifest.adapter_bundle_sha256)
        || manifest.source_commit.len() != 40
        || !manifest
            .source_commit
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        || !ats_certification_safe_integer(manifest.layout_contract_version, true)
        || !ats_certification_hex64(&manifest.layout_contract_sha256)
        || !ats_certification_capability(&manifest.maximum_capability)
        || (matches!(manifest.provider.as_str(), "greenhouse" | "lever")
            && !ats_exact_provider_target_key(&manifest.provider, &manifest.target_key))
        || (manifest.maximum_capability != "observe_only"
            && (!ats_exact_submit_adapter(&manifest.provider, &manifest.adapter_version)
                || !ats_exact_provider_target_key(&manifest.provider, &manifest.target_key)))
        || manifest.evidence_sha256s.is_empty()
        || manifest.evidence_sha256s.len() > 64
        || !manifest
            .evidence_sha256s
            .iter()
            .all(|value| ats_certification_hex64(value))
        || !manifest
            .evidence_sha256s
            .windows(2)
            .all(|pair| pair[0] < pair[1])
        || manifest.runtime_targets.is_empty()
        || manifest.runtime_targets.len() > 32
        || !manifest.runtime_targets.windows(2).all(|pair| {
            (&pair[0].runtime_kind, &pair[0].runtime_id)
                < (&pair[1].runtime_kind, &pair[1].runtime_id)
        })
        || manifest
            .runtime_targets
            .iter()
            .any(|runtime| validate_ats_runtime_target(runtime).is_err())
        || !ats_certification_safe_integer(manifest.tested_at_ms, false)
        || !ats_certification_safe_integer(manifest.issued_at_ms, false)
        || manifest.tested_at_ms > manifest.issued_at_ms
        || manifest.not_before_ms < manifest.issued_at_ms
        || manifest.expires_at_ms <= manifest.not_before_ms
        || !ats_certification_safe_integer(manifest.expires_at_ms, true)
    {
        return Err(AtsCertificationAuthorityError::InvalidAuthority);
    }
    Ok(())
}

fn validate_ats_activation_authority(
    activation: &AtsCertificationActivationAuthority,
) -> Result<(), AtsCertificationAuthorityError> {
    if activation.version != 1
        || activation.audience != ATS_CERTIFICATION_ACTIVATION_AUDIENCE
        || !ats_certification_token(&activation.activation_id, 1, 120)
        || !ats_certification_hex64(&activation.policy_sha256)
        || !ats_certification_safe_integer(activation.activation_generation, true)
        || activation
            .predecessor_activation_sha256
            .as_deref()
            .is_some_and(|value| !ats_certification_hex64(value))
        || !ats_certification_hex64(&activation.manifest_sha256)
        || !ats_certification_hex64(&activation.scope_sha256)
        || !matches!(activation.channel.as_str(), "canary" | "general" | "shadow")
        || !ats_certification_safe_integer(activation.channel_sequence, true)
        || !ats_certification_capability(&activation.capability)
        || !ats_certification_safe_integer(activation.issued_at_ms, false)
        || activation.not_before_ms < activation.issued_at_ms
        || activation.expires_at_ms <= activation.not_before_ms
        || !ats_certification_safe_integer(activation.expires_at_ms, true)
        || !ats_certification_text(&activation.approval_ref, 1, 240)
    {
        return Err(AtsCertificationAuthorityError::InvalidAuthority);
    }
    match activation.channel.as_str() {
        "shadow"
            if activation.capability == "observe_only"
                && activation.account_allowlist_sha256.is_none()
                && activation.canary_max_submissions == 0
                && activation.canary_account_cap == 0
                && activation.canary_concurrency_cap == 0
                && activation.canary_daily_side_effect_cap == 0
                && activation.canary_evidence_manifest_sha256.is_none() => {}
        "canary"
            if activation
                .account_allowlist_sha256
                .as_deref()
                .is_some_and(ats_certification_hex64)
                && (1..=10_000).contains(&activation.canary_max_submissions)
                && (1..=10_000).contains(&activation.canary_account_cap)
                && (1..=1_000).contains(&activation.canary_concurrency_cap)
                && (1..=10_000).contains(&activation.canary_daily_side_effect_cap)
                && activation.canary_account_cap <= activation.canary_max_submissions
                && activation.canary_concurrency_cap <= activation.canary_max_submissions
                && activation.canary_daily_side_effect_cap <= activation.canary_max_submissions
                && activation
                    .canary_evidence_manifest_sha256
                    .as_deref()
                    .is_some_and(|value| {
                        ats_certification_hex64(value)
                            && value == activation.manifest_sha256.as_str()
                    }) => {}
        "general"
            if activation.account_allowlist_sha256.is_none()
                && activation.canary_max_submissions == 0
                && activation.canary_account_cap == 0
                && activation.canary_concurrency_cap == 0
                && activation.canary_daily_side_effect_cap == 0
                && activation.canary_evidence_manifest_sha256.is_none() => {}
        _ => return Err(AtsCertificationAuthorityError::InvalidAuthority),
    }
    Ok(())
}

fn validate_ats_revocation_authority(
    revocation: &AtsCertificationRevocationAuthority,
) -> Result<(), AtsCertificationAuthorityError> {
    if revocation.version != 1
        || revocation.audience != ATS_CERTIFICATION_REVOCATION_AUDIENCE
        || !ats_certification_token(&revocation.revocation_id, 1, 120)
        || !ats_certification_hex64(&revocation.policy_sha256)
        || !ats_certification_safe_integer(revocation.revocation_generation, true)
        || match (
            revocation.revocation_generation,
            revocation.predecessor_revocation_sha256.as_deref(),
        ) {
            (1, None) => false,
            (generation, Some(predecessor)) => {
                generation <= 1 || !ats_certification_hex64(predecessor)
            }
            _ => true,
        }
        || !matches!(
            revocation.subject_kind.as_str(),
            "activation"
                | "adapter_bundle"
                | "browser_release_manifest"
                | "evidence"
                | "layout_observation"
                | "manifest"
                | "policy"
                | "runner_build"
                | "runner_image"
                | "runtime"
                | "scope"
                | "target"
                | "trust_key"
        )
        || !ats_certification_text(&revocation.subject_id, 1, 240)
        || !ats_certification_hex64(&revocation.subject_sha256)
        || !ats_certification_text(&revocation.reason_ref, 1, 240)
        || !ats_certification_safe_integer(revocation.issued_at_ms, false)
        || revocation.effective_at_ms < revocation.issued_at_ms
        || !ats_certification_safe_integer(revocation.effective_at_ms, false)
    {
        return Err(AtsCertificationAuthorityError::InvalidAuthority);
    }
    Ok(())
}

fn validate_ats_quarantine_authority(
    command: &AtsCertificationQuarantineAuthority,
) -> Result<(), AtsCertificationAuthorityError> {
    if command.version != 1
        || command.audience != ATS_CERTIFICATION_QUARANTINE_AUDIENCE
        || !ats_certification_token(&command.command_id, 1, 120)
        || !ats_certification_hex64(&command.policy_sha256)
        || !ats_certification_safe_integer(command.command_generation, true)
        || !matches!(
            command.scope_kind.as_str(),
            "activation" | "adapter" | "provider" | "runtime" | "surface" | "target"
        )
        || !ats_certification_text(&command.scope_id, 1, 240)
        || !ats_certification_hex64(&command.scope_sha256)
        || !ats_certification_safe_integer(command.command_sequence, true)
        || command
            .predecessor_command_sha256
            .as_deref()
            .is_some_and(|value| !ats_certification_hex64(value))
        || !matches!(command.action.as_str(), "quarantine" | "release")
        || !ats_certification_text(&command.reason_ref, 1, 240)
        || !ats_certification_safe_integer(command.issued_at_ms, false)
    {
        return Err(AtsCertificationAuthorityError::InvalidAuthority);
    }
    Ok(())
}

fn validate_ats_recorded_by(value: &str) -> Result<(), AtsCertificationAuthorityError> {
    if !ats_certification_text(value, 1, 128) {
        return Err(AtsCertificationAuthorityError::InvalidAuthority);
    }
    Ok(())
}

fn verify_ats_evidence_envelope(
    envelope: &AtsCertificationAuthorityEnvelope,
    trust_anchor: &AtsCertificationTrustAnchor,
    verification_time_ms: i64,
) -> Result<
    VerifiedAtsCertificationEnvelope<AtsCertificationEvidenceAuthority>,
    AtsCertificationAuthorityError,
> {
    let bytes = ats_certification_decode_base64url_bounded(
        &envelope.canonical_base64url,
        ATS_CERTIFICATION_MAX_CANONICAL_BYTES,
    )?;
    let evidence: AtsCertificationEvidenceAuthority =
        ats_certification_parse_canonical_json(&bytes)?;
    validate_ats_evidence_authority(&evidence)?;
    if verification_time_ms >= evidence.expires_at_ms {
        return Err(AtsCertificationAuthorityError::Expired);
    }
    verify_ats_certification_envelope(
        envelope,
        trust_anchor,
        "evidence",
        ATS_CERTIFICATION_EVIDENCE_AUDIENCE,
        evidence.issued_at_ms,
        verification_time_ms,
    )
}

fn verify_ats_layout_observation_envelope(
    envelope: &AtsCertificationAuthorityEnvelope,
    trust_anchor: &AtsCertificationTrustAnchor,
    verification_time_ms: i64,
) -> Result<
    VerifiedAtsCertificationEnvelope<AtsLayoutObservationAuthority>,
    AtsCertificationAuthorityError,
> {
    let bytes = ats_certification_decode_base64url_bounded(
        &envelope.canonical_base64url,
        ATS_CERTIFICATION_MAX_CANONICAL_BYTES,
    )?;
    let observation: AtsLayoutObservationAuthority =
        ats_certification_parse_canonical_json(&bytes)?;
    validate_ats_layout_observation_privacy(&observation)?;
    if verification_time_ms >= observation.expires_at_ms {
        return Err(AtsCertificationAuthorityError::Expired);
    }
    verify_ats_certification_envelope(
        envelope,
        trust_anchor,
        "layout_observation",
        ATS_CERTIFICATION_LAYOUT_OBSERVATION_AUDIENCE,
        observation.issued_at_ms,
        verification_time_ms,
    )
}

fn verify_ats_manifest_envelope(
    envelope: &AtsCertificationAuthorityEnvelope,
    trust_anchor: &AtsCertificationTrustAnchor,
    verification_time_ms: i64,
) -> Result<
    VerifiedAtsCertificationEnvelope<AtsCertificationManifestAuthority>,
    AtsCertificationAuthorityError,
> {
    let bytes = ats_certification_decode_base64url_bounded(
        &envelope.canonical_base64url,
        ATS_CERTIFICATION_MAX_CANONICAL_BYTES,
    )?;
    let manifest: AtsCertificationManifestAuthority =
        ats_certification_parse_canonical_json(&bytes)?;
    validate_ats_manifest_authority(&manifest)?;
    if verification_time_ms >= manifest.expires_at_ms {
        return Err(AtsCertificationAuthorityError::Expired);
    }
    verify_ats_certification_envelope(
        envelope,
        trust_anchor,
        "manifest",
        ATS_CERTIFICATION_MANIFEST_AUDIENCE,
        manifest.issued_at_ms,
        verification_time_ms,
    )
}

fn verify_ats_activation_envelope(
    envelope: &AtsCertificationAuthorityEnvelope,
    trust_anchor: &AtsCertificationTrustAnchor,
    verification_time_ms: i64,
) -> Result<
    VerifiedAtsCertificationEnvelope<AtsCertificationActivationAuthority>,
    AtsCertificationAuthorityError,
> {
    let bytes = ats_certification_decode_base64url_bounded(
        &envelope.canonical_base64url,
        ATS_CERTIFICATION_MAX_CANONICAL_BYTES,
    )?;
    let activation: AtsCertificationActivationAuthority =
        ats_certification_parse_canonical_json(&bytes)?;
    validate_ats_activation_authority(&activation)?;
    if verification_time_ms >= activation.expires_at_ms {
        return Err(AtsCertificationAuthorityError::Expired);
    }
    verify_ats_certification_envelope(
        envelope,
        trust_anchor,
        "activation",
        ATS_CERTIFICATION_ACTIVATION_AUDIENCE,
        activation.issued_at_ms,
        verification_time_ms,
    )
}

fn verify_ats_revocation_envelope(
    envelope: &AtsCertificationAuthorityEnvelope,
    trust_anchor: &AtsCertificationTrustAnchor,
    verification_time_ms: i64,
) -> Result<
    VerifiedAtsCertificationEnvelope<AtsCertificationRevocationAuthority>,
    AtsCertificationAuthorityError,
> {
    let bytes = ats_certification_decode_base64url_bounded(
        &envelope.canonical_base64url,
        ATS_CERTIFICATION_MAX_CANONICAL_BYTES,
    )?;
    let revocation: AtsCertificationRevocationAuthority =
        ats_certification_parse_canonical_json(&bytes)?;
    validate_ats_revocation_authority(&revocation)?;
    verify_ats_certification_envelope(
        envelope,
        trust_anchor,
        "revocation",
        ATS_CERTIFICATION_REVOCATION_AUDIENCE,
        revocation.issued_at_ms,
        verification_time_ms,
    )
}

fn verify_ats_quarantine_envelope(
    envelope: &AtsCertificationAuthorityEnvelope,
    trust_anchor: &AtsCertificationTrustAnchor,
    verification_time_ms: i64,
) -> Result<
    VerifiedAtsCertificationEnvelope<AtsCertificationQuarantineAuthority>,
    AtsCertificationAuthorityError,
> {
    let bytes = ats_certification_decode_base64url_bounded(
        &envelope.canonical_base64url,
        ATS_CERTIFICATION_MAX_CANONICAL_BYTES,
    )?;
    let command: AtsCertificationQuarantineAuthority =
        ats_certification_parse_canonical_json(&bytes)?;
    validate_ats_quarantine_authority(&command)?;
    verify_ats_certification_envelope(
        envelope,
        trust_anchor,
        "revocation",
        ATS_CERTIFICATION_QUARANTINE_AUDIENCE,
        command.issued_at_ms,
        verification_time_ms,
    )
}

fn configured_ats_certification_root_trust_anchor(
) -> Result<AtsCertificationRootTrustAnchor, AtsCertificationAuthorityError> {
    let encoded = std::env::var(ATS_CERTIFICATION_ROOT_TRUST_ANCHOR_ENV)
        .map_err(|_| AtsCertificationAuthorityError::InvalidTrustAnchor)?;
    let anchor: AtsCertificationRootTrustAnchor = serde_json::from_str(&encoded)
        .map_err(|_| AtsCertificationAuthorityError::InvalidTrustAnchor)?;
    validate_ats_certification_root_trust_anchor(&anchor)?;
    Ok(anchor)
}

pub fn import_ats_certification_trust_policy(
    pool: &DbPool,
    envelope: &AtsCertificationAuthorityEnvelope,
    recorded_by: &str,
) -> Result<AtsCertificationImportResult, AtsCertificationAuthorityError> {
    let root_anchor = configured_ats_certification_root_trust_anchor()?;
    import_ats_certification_trust_policy_with_root_at(
        pool,
        envelope,
        &root_anchor,
        recorded_by,
        now_ms(),
    )
}

fn import_ats_certification_trust_policy_with_root_at(
    pool: &DbPool,
    envelope: &AtsCertificationAuthorityEnvelope,
    root_anchor: &AtsCertificationRootTrustAnchor,
    recorded_by: &str,
    recorded_at_ms: i64,
) -> Result<AtsCertificationImportResult, AtsCertificationAuthorityError> {
    validate_ats_recorded_by(recorded_by)?;
    let verified =
        verify_ats_certification_trust_policy_envelope(envelope, root_anchor, recorded_at_ms)?;
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get().map_err(ats_certification_storage)?;
            let tx = conn
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(ats_certification_storage)?;
            let existing: Option<(String, String, String, String)> = tx
                .query_row(
                    "SELECT policy_sha256, canonical_policy_base64url,
                            authorization_sha256, root_trust_anchor_sha256
                       FROM jobs_ats_certification_trust_policies
                      WHERE policy_id = ?1 OR policy_sha256 = ?2 OR trust_generation = ?3",
                    params![
                        verified.authority.policy_id,
                        verified.authority_sha256,
                        verified.authority.trust_generation,
                    ],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
                )
                .optional()
                .map_err(ats_certification_storage)?;
            if let Some(existing) = existing {
                require_exact_ats_trust_policy_replay(existing, &verified, envelope)?;
                tx.commit().map_err(ats_certification_storage)?;
                return Ok(ats_trust_policy_import_result(&verified, true));
            }
            let head: Option<(String, i64, i64, String)> = tx
                .query_row(
                    "SELECT current_policy_sha256, current_trust_generation,
                            head_revision, root_trust_anchor_sha256
                       FROM jobs_ats_certification_trust_head WHERE singleton_id = 1",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
                )
                .optional()
                .map_err(ats_certification_storage)?;
            require_ats_trust_policy_successor(&verified, head.as_ref())?;
            insert_sqlite_ats_trust_policy(&tx, &verified, envelope, recorded_by, recorded_at_ms)?;
            advance_sqlite_ats_trust_head(
                &tx,
                &verified,
                head.as_ref(),
                recorded_by,
                recorded_at_ms,
            )?;
            tx.commit().map_err(ats_certification_storage)?;
            Ok(ats_trust_policy_import_result(&verified, false))
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg().map_err(ats_certification_storage)?;
            let mut tx = conn.transaction().map_err(ats_certification_storage)?;
            lock_postgres_ats_certification(&mut tx)?;
            let existing = tx
                .query_opt(
                    "SELECT policy_sha256, canonical_policy_base64url,
                            authorization_sha256, root_trust_anchor_sha256
                       FROM jobs_ats_certification_trust_policies
                      WHERE policy_id = $1 OR policy_sha256 = $2 OR trust_generation = $3",
                    &[
                        &verified.authority.policy_id,
                        &verified.authority_sha256,
                        &verified.authority.trust_generation,
                    ],
                )
                .map_err(ats_certification_storage)?
                .map(|row| (row.get(0), row.get(1), row.get(2), row.get(3)));
            if let Some(existing) = existing {
                require_exact_ats_trust_policy_replay(existing, &verified, envelope)?;
                tx.commit().map_err(ats_certification_storage)?;
                return Ok(ats_trust_policy_import_result(&verified, true));
            }
            let head = tx
                .query_opt(
                    "SELECT current_policy_sha256, current_trust_generation,
                            head_revision, root_trust_anchor_sha256
                       FROM jobs_ats_certification_trust_head
                      WHERE singleton_id = 1 FOR UPDATE",
                    &[],
                )
                .map_err(ats_certification_storage)?
                .map(|row| (row.get(0), row.get(1), row.get(2), row.get(3)));
            require_ats_trust_policy_successor(&verified, head.as_ref())?;
            insert_postgres_ats_trust_policy(
                &mut tx,
                &verified,
                envelope,
                recorded_by,
                recorded_at_ms,
            )?;
            advance_postgres_ats_trust_head(
                &mut tx,
                &verified,
                head.as_ref(),
                recorded_by,
                recorded_at_ms,
            )?;
            tx.commit().map_err(ats_certification_storage)?;
            Ok(ats_trust_policy_import_result(&verified, false))
        }
    })
}

fn require_exact_ats_trust_policy_replay(
    existing: (String, String, String, String),
    verified: &VerifiedAtsCertificationEnvelope<AtsCertificationTrustPolicyAuthority>,
    envelope: &AtsCertificationAuthorityEnvelope,
) -> Result<(), AtsCertificationAuthorityError> {
    if existing.0 != verified.authority_sha256
        || existing.1 != envelope.canonical_base64url
        || existing.2 != verified.authorization_sha256
        || existing.3 != verified.trust_policy_sha256
    {
        return Err(AtsCertificationAuthorityError::IdentityConflict);
    }
    Ok(())
}

fn require_ats_trust_policy_successor(
    verified: &VerifiedAtsCertificationEnvelope<AtsCertificationTrustPolicyAuthority>,
    head: Option<&(String, i64, i64, String)>,
) -> Result<(), AtsCertificationAuthorityError> {
    match head {
        None if verified.authority.trust_generation == 1
            && verified.authority.predecessor_policy_sha256.is_none() => {}
        Some((policy_sha256, generation, _, root_sha256))
            if verified.authority.trust_generation == generation + 1
                && verified.authority.predecessor_policy_sha256.as_deref()
                    == Some(policy_sha256.as_str())
                && verified.trust_policy_sha256 == *root_sha256 => {}
        _ => return Err(AtsCertificationAuthorityError::SequenceRegression),
    }
    Ok(())
}

fn ats_trust_policy_import_result(
    verified: &VerifiedAtsCertificationEnvelope<AtsCertificationTrustPolicyAuthority>,
    replayed: bool,
) -> AtsCertificationImportResult {
    AtsCertificationImportResult {
        authority_kind: "trust_policy".to_string(),
        authority_id: verified.authority.policy_id.clone(),
        authority_sha256: verified.authority_sha256.clone(),
        authorization_sha256: verified.authorization_sha256.clone(),
        trust_policy_sha256: verified.authority_sha256.clone(),
        replayed,
    }
}

fn insert_sqlite_ats_trust_policy(
    tx: &rusqlite::Transaction<'_>,
    verified: &VerifiedAtsCertificationEnvelope<AtsCertificationTrustPolicyAuthority>,
    envelope: &AtsCertificationAuthorityEnvelope,
    recorded_by: &str,
    recorded_at_ms: i64,
) -> Result<(), AtsCertificationAuthorityError> {
    let policy = &verified.authority;
    tx.execute(
        "INSERT INTO jobs_ats_certification_trust_policies (
           policy_sha256, policy_id, trust_generation, predecessor_policy_sha256,
           maximum_clock_skew_ms, maximum_manifest_size_bytes,
           maximum_target_count, maximum_observation_count,
           maximum_evidence_object_count,
           canonical_policy_base64url, authorization_sha256, root_trust_anchor_sha256,
           canonical_authorization_base64url, issued_at_ms, valid_from_ms, expires_at_ms,
           recorded_by, recorded_at_ms
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12,
                   ?13, ?14, ?15, ?16, ?17, ?18)",
        params![
            verified.authority_sha256,
            policy.policy_id,
            policy.trust_generation,
            policy.predecessor_policy_sha256,
            policy.certification_requirements.maximum_clock_skew_ms,
            policy
                .certification_requirements
                .maximum_manifest_size_bytes,
            policy.certification_requirements.maximum_target_count,
            policy.certification_requirements.maximum_observation_count,
            policy
                .certification_requirements
                .maximum_evidence_object_count,
            envelope.canonical_base64url,
            verified.authorization_sha256,
            verified.trust_policy_sha256,
            envelope.authorization_base64url,
            policy.issued_at_ms,
            policy.valid_from_ms,
            policy.expires_at_ms,
            recorded_by,
            recorded_at_ms,
        ],
    )
    .map_err(ats_certification_storage)?;
    for (role, trust) in &policy.delegated_trust.roles {
        for (key_id, public_key) in &trust.keys {
            tx.execute(
                "INSERT INTO jobs_ats_certification_trust_keys (
                   policy_sha256, trust_generation, role, threshold, key_id,
                   public_key_base64url, key_state, valid_from_ms, expires_at_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'active', ?7, ?8)",
                params![
                    verified.authority_sha256,
                    policy.trust_generation,
                    role,
                    trust.threshold,
                    key_id,
                    public_key,
                    policy.valid_from_ms,
                    policy.expires_at_ms,
                ],
            )
            .map_err(ats_certification_storage)?;
        }
    }
    Ok(())
}

fn insert_postgres_ats_trust_policy(
    tx: &mut postgres::Transaction<'_>,
    verified: &VerifiedAtsCertificationEnvelope<AtsCertificationTrustPolicyAuthority>,
    envelope: &AtsCertificationAuthorityEnvelope,
    recorded_by: &str,
    recorded_at_ms: i64,
) -> Result<(), AtsCertificationAuthorityError> {
    let policy = &verified.authority;
    tx.execute(
        "INSERT INTO jobs_ats_certification_trust_policies (
           policy_sha256, policy_id, trust_generation, predecessor_policy_sha256,
           maximum_clock_skew_ms, maximum_manifest_size_bytes,
           maximum_target_count, maximum_observation_count,
           maximum_evidence_object_count,
           canonical_policy_base64url, authorization_sha256, root_trust_anchor_sha256,
           canonical_authorization_base64url, issued_at_ms, valid_from_ms, expires_at_ms,
           recorded_by, recorded_at_ms
         ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12,
                   $13, $14, $15, $16, $17, $18)",
        &[
            &verified.authority_sha256,
            &policy.policy_id,
            &policy.trust_generation,
            &policy.predecessor_policy_sha256,
            &policy.certification_requirements.maximum_clock_skew_ms,
            &policy
                .certification_requirements
                .maximum_manifest_size_bytes,
            &policy.certification_requirements.maximum_target_count,
            &policy.certification_requirements.maximum_observation_count,
            &policy
                .certification_requirements
                .maximum_evidence_object_count,
            &envelope.canonical_base64url,
            &verified.authorization_sha256,
            &verified.trust_policy_sha256,
            &envelope.authorization_base64url,
            &policy.issued_at_ms,
            &policy.valid_from_ms,
            &policy.expires_at_ms,
            &recorded_by,
            &recorded_at_ms,
        ],
    )
    .map_err(ats_certification_storage)?;
    for (role, trust) in &policy.delegated_trust.roles {
        for (key_id, public_key) in &trust.keys {
            tx.execute(
                "INSERT INTO jobs_ats_certification_trust_keys (
                   policy_sha256, trust_generation, role, threshold, key_id,
                   public_key_base64url, key_state, valid_from_ms, expires_at_ms
                 ) VALUES ($1, $2, $3, $4, $5, $6, 'active', $7, $8)",
                &[
                    &verified.authority_sha256,
                    &policy.trust_generation,
                    &role,
                    &trust.threshold,
                    &key_id,
                    &public_key,
                    &policy.valid_from_ms,
                    &policy.expires_at_ms,
                ],
            )
            .map_err(ats_certification_storage)?;
        }
    }
    Ok(())
}

fn advance_sqlite_ats_trust_head(
    tx: &rusqlite::Transaction<'_>,
    verified: &VerifiedAtsCertificationEnvelope<AtsCertificationTrustPolicyAuthority>,
    head: Option<&(String, i64, i64, String)>,
    recorded_by: &str,
    recorded_at_ms: i64,
) -> Result<(), AtsCertificationAuthorityError> {
    match head {
        Some((_, _, head_revision, _)) => tx.execute(
            "UPDATE jobs_ats_certification_trust_head SET
               current_policy_sha256 = ?1, current_trust_generation = ?2,
               head_revision = ?3, updated_by = ?4, updated_at_ms = ?5
             WHERE singleton_id = 1",
            params![
                verified.authority_sha256,
                verified.authority.trust_generation,
                head_revision + 1,
                recorded_by,
                recorded_at_ms,
            ],
        ),
        None => tx.execute(
            "INSERT INTO jobs_ats_certification_trust_head (
               singleton_id, current_policy_sha256, current_trust_generation,
               head_revision, root_trust_anchor_sha256, updated_by, updated_at_ms
             ) VALUES (1, ?1, ?2, 1, ?3, ?4, ?5)",
            params![
                verified.authority_sha256,
                verified.authority.trust_generation,
                verified.trust_policy_sha256,
                recorded_by,
                recorded_at_ms,
            ],
        ),
    }
    .map_err(ats_certification_storage)?;
    Ok(())
}

fn advance_postgres_ats_trust_head(
    tx: &mut postgres::Transaction<'_>,
    verified: &VerifiedAtsCertificationEnvelope<AtsCertificationTrustPolicyAuthority>,
    head: Option<&(String, i64, i64, String)>,
    recorded_by: &str,
    recorded_at_ms: i64,
) -> Result<(), AtsCertificationAuthorityError> {
    match head {
        Some((_, _, head_revision, _)) => tx.execute(
            "UPDATE jobs_ats_certification_trust_head SET
               current_policy_sha256 = $1, current_trust_generation = $2,
               head_revision = $3, updated_by = $4, updated_at_ms = $5
             WHERE singleton_id = 1",
            &[
                &verified.authority_sha256,
                &verified.authority.trust_generation,
                &(head_revision + 1),
                &recorded_by,
                &recorded_at_ms,
            ],
        ),
        None => tx.execute(
            "INSERT INTO jobs_ats_certification_trust_head (
               singleton_id, current_policy_sha256, current_trust_generation,
               head_revision, root_trust_anchor_sha256, updated_by, updated_at_ms
             ) VALUES (1, $1, $2, 1, $3, $4, $5)",
            &[
                &verified.authority_sha256,
                &verified.authority.trust_generation,
                &verified.trust_policy_sha256,
                &recorded_by,
                &recorded_at_ms,
            ],
        ),
    }
    .map_err(ats_certification_storage)?;
    Ok(())
}

fn load_current_ats_certification_trust_policy(
    pool: &DbPool,
    verification_time_ms: i64,
) -> Result<(String, AtsCertificationTrustPolicyAuthority), AtsCertificationAuthorityError> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get().map_err(ats_certification_storage)?;
            let row: Option<(String, i64, String)> = conn
                .query_row(
                    "SELECT policy.policy_sha256, head.current_trust_generation,
                            policy.canonical_policy_base64url
                       FROM jobs_ats_certification_trust_head head
                       JOIN jobs_ats_certification_trust_policies policy
                         ON policy.policy_sha256 = head.current_policy_sha256
                        AND policy.trust_generation = head.current_trust_generation
                      WHERE head.singleton_id = 1
                        AND policy.valid_from_ms <= ?1 AND policy.expires_at_ms > ?1",
                    params![verification_time_ms],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .optional()
                .map_err(ats_certification_storage)?;
            parse_stored_ats_trust_policy(row)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg().map_err(ats_certification_storage)?;
            let row = conn
                .query_opt(
                    "SELECT policy.policy_sha256, head.current_trust_generation,
                            policy.canonical_policy_base64url
                       FROM jobs_ats_certification_trust_head head
                       JOIN jobs_ats_certification_trust_policies policy
                         ON policy.policy_sha256 = head.current_policy_sha256
                        AND policy.trust_generation = head.current_trust_generation
                      WHERE head.singleton_id = 1
                        AND policy.valid_from_ms <= $1 AND policy.expires_at_ms > $1",
                    &[&verification_time_ms],
                )
                .map_err(ats_certification_storage)?
                .map(|row| (row.get(0), row.get(1), row.get(2)));
            parse_stored_ats_trust_policy(row)
        }
    })
}

fn parse_stored_ats_trust_policy(
    row: Option<(String, i64, String)>,
) -> Result<(String, AtsCertificationTrustPolicyAuthority), AtsCertificationAuthorityError> {
    let (policy_sha256, generation, canonical_base64url) =
        row.ok_or(AtsCertificationAuthorityError::TrustPolicyNotInitialized)?;
    let bytes = ats_certification_decode_base64url_bounded(
        &canonical_base64url,
        ATS_CERTIFICATION_MAX_CANONICAL_BYTES,
    )?;
    let policy: AtsCertificationTrustPolicyAuthority =
        ats_certification_parse_canonical_json(&bytes)?;
    validate_ats_certification_trust_anchor(&policy.delegated_trust)?;
    if ats_certification_sha256(&bytes) != policy_sha256
        || policy.trust_generation != generation
        || policy.audience != ATS_CERTIFICATION_TRUST_POLICY_AUDIENCE
    {
        return Err(AtsCertificationAuthorityError::InvalidTrustPolicy);
    }
    Ok((policy_sha256, policy))
}

fn load_current_sqlite_ats_certification_trust_policy_tx(
    tx: &rusqlite::Transaction<'_>,
    verification_time_ms: i64,
) -> Result<(String, AtsCertificationTrustPolicyAuthority), AtsCertificationAuthorityError> {
    let row = tx
        .query_row(
            "SELECT policy.policy_sha256, head.current_trust_generation,
                    policy.canonical_policy_base64url
               FROM jobs_ats_certification_trust_head head
               JOIN jobs_ats_certification_trust_policies policy
                 ON policy.policy_sha256 = head.current_policy_sha256
                AND policy.trust_generation = head.current_trust_generation
              WHERE head.singleton_id = 1
                AND policy.valid_from_ms <= ?1 AND policy.expires_at_ms > ?1",
            params![verification_time_ms],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()
        .map_err(ats_certification_storage)?;
    parse_stored_ats_trust_policy(row)
}

fn load_current_postgres_ats_certification_trust_policy_tx(
    tx: &mut postgres::Transaction<'_>,
    verification_time_ms: i64,
) -> Result<(String, AtsCertificationTrustPolicyAuthority), AtsCertificationAuthorityError> {
    let row = tx
        .query_opt(
            "SELECT policy.policy_sha256, head.current_trust_generation,
                    policy.canonical_policy_base64url
               FROM jobs_ats_certification_trust_head head
               JOIN jobs_ats_certification_trust_policies policy
                 ON policy.policy_sha256 = head.current_policy_sha256
                AND policy.trust_generation = head.current_trust_generation
              WHERE head.singleton_id = 1
                AND policy.valid_from_ms <= $1 AND policy.expires_at_ms > $1
              FOR SHARE",
            &[&verification_time_ms],
        )
        .map_err(ats_certification_storage)?
        .map(|row| (row.get(0), row.get(1), row.get(2)));
    parse_stored_ats_trust_policy(row)
}

fn require_current_sqlite_ats_trust_policy(
    tx: &rusqlite::Transaction<'_>,
    policy_sha256: &str,
    verification_time_ms: i64,
) -> Result<(), AtsCertificationAuthorityError> {
    let current: Option<i64> = tx
        .query_row(
            "SELECT 1 FROM jobs_ats_certification_trust_head head
              JOIN jobs_ats_certification_trust_policies policy
                ON policy.policy_sha256 = head.current_policy_sha256
             WHERE head.singleton_id = 1 AND head.current_policy_sha256 = ?1
               AND policy.valid_from_ms <= ?2 AND policy.expires_at_ms > ?2",
            params![policy_sha256, verification_time_ms],
            |row| row.get(0),
        )
        .optional()
        .map_err(ats_certification_storage)?;
    if current.is_none() {
        return Err(AtsCertificationAuthorityError::InvalidTrustPolicy);
    }
    Ok(())
}

fn require_current_postgres_ats_trust_policy(
    tx: &mut postgres::Transaction<'_>,
    policy_sha256: &str,
    verification_time_ms: i64,
) -> Result<(), AtsCertificationAuthorityError> {
    let current = tx
        .query_opt(
            "SELECT 1 FROM jobs_ats_certification_trust_head head
              JOIN jobs_ats_certification_trust_policies policy
                ON policy.policy_sha256 = head.current_policy_sha256
             WHERE head.singleton_id = 1 AND head.current_policy_sha256 = $1
               AND policy.valid_from_ms <= $2 AND policy.expires_at_ms > $2",
            &[&policy_sha256, &verification_time_ms],
        )
        .map_err(ats_certification_storage)?;
    if current.is_none() {
        return Err(AtsCertificationAuthorityError::InvalidTrustPolicy);
    }
    Ok(())
}

fn ats_certification_import_result(
    kind: &str,
    id: &str,
    verified: &VerifiedAtsCertificationEnvelope<impl Sized>,
    replayed: bool,
) -> AtsCertificationImportResult {
    AtsCertificationImportResult {
        authority_kind: kind.to_string(),
        authority_id: id.to_string(),
        authority_sha256: verified.authority_sha256.clone(),
        authorization_sha256: verified.authorization_sha256.clone(),
        trust_policy_sha256: verified.trust_policy_sha256.clone(),
        replayed,
    }
}

pub fn import_ats_layout_observation(
    pool: &DbPool,
    envelope: &AtsCertificationAuthorityEnvelope,
    recorded_by: &str,
) -> Result<AtsCertificationImportResult, AtsCertificationAuthorityError> {
    import_ats_layout_observation_at(pool, envelope, recorded_by, now_ms())
}

fn import_ats_layout_observation_at(
    pool: &DbPool,
    envelope: &AtsCertificationAuthorityEnvelope,
    recorded_by: &str,
    recorded_at_ms: i64,
) -> Result<AtsCertificationImportResult, AtsCertificationAuthorityError> {
    validate_ats_recorded_by(recorded_by)?;
    let (trust_policy_sha256, trust_policy) =
        load_current_ats_certification_trust_policy(pool, recorded_at_ms)?;
    let mut verified = verify_ats_layout_observation_envelope(
        envelope,
        &trust_policy.delegated_trust,
        recorded_at_ms,
    )?;
    if verified.authority.policy_sha256 != trust_policy_sha256 {
        return Err(AtsCertificationAuthorityError::InvalidTrustPolicy);
    }
    if !trust_policy
        .certification_requirements
        .allowed_providers
        .contains(&verified.authority.provider)
        || !trust_policy
            .certification_requirements
            .required_evidence_classes
            .contains(&verified.authority.evidence_class)
        || verified.authority.expires_at_ms > trust_policy.expires_at_ms
    {
        return Err(AtsCertificationAuthorityError::InvalidTrustPolicy);
    }
    require_ats_authorization_clock_skew(
        envelope,
        verified.authority.issued_at_ms,
        trust_policy
            .certification_requirements
            .maximum_clock_skew_ms,
    )?;
    verified.trust_policy_sha256 = trust_policy_sha256;
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get().map_err(ats_certification_storage)?;
            let tx = conn
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(ats_certification_storage)?;
            require_current_sqlite_ats_trust_policy(
                &tx,
                &verified.trust_policy_sha256,
                recorded_at_ms,
            )?;
            let existing: Option<(String, String, String, String)> = tx
                .query_row(
                    "SELECT observation_sha256, canonical_observation_base64url,
                            authorization_sha256, trust_policy_sha256
                       FROM jobs_ats_certification_layout_observations
                      WHERE observation_id = ?1 OR observation_sha256 = ?2",
                    params![verified.authority.observation_id, verified.authority_sha256],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
                )
                .optional()
                .map_err(ats_certification_storage)?;
            if let Some(existing) = existing {
                require_exact_ats_authority_replay(
                    existing,
                    &verified,
                    &envelope.canonical_base64url,
                )?;
                tx.commit().map_err(ats_certification_storage)?;
                return Ok(ats_certification_import_result(
                    "layout_observation",
                    &verified.authority.observation_id,
                    &verified,
                    true,
                ));
            }
            require_sqlite_ats_layout_predecessor(&tx, &verified.authority)?;
            insert_sqlite_ats_layout_observation(
                &tx,
                &verified,
                envelope,
                recorded_by,
                recorded_at_ms,
            )?;
            tx.commit().map_err(ats_certification_storage)?;
            Ok(ats_certification_import_result(
                "layout_observation",
                &verified.authority.observation_id,
                &verified,
                false,
            ))
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg().map_err(ats_certification_storage)?;
            let mut tx = conn.transaction().map_err(ats_certification_storage)?;
            lock_postgres_ats_certification(&mut tx)?;
            require_current_postgres_ats_trust_policy(
                &mut tx,
                &verified.trust_policy_sha256,
                recorded_at_ms,
            )?;
            let existing = tx
                .query_opt(
                    "SELECT observation_sha256, canonical_observation_base64url,
                            authorization_sha256, trust_policy_sha256
                       FROM jobs_ats_certification_layout_observations
                      WHERE observation_id = $1 OR observation_sha256 = $2",
                    &[
                        &verified.authority.observation_id,
                        &verified.authority_sha256,
                    ],
                )
                .map_err(ats_certification_storage)?
                .map(|row| (row.get(0), row.get(1), row.get(2), row.get(3)));
            if let Some(existing) = existing {
                require_exact_ats_authority_replay(
                    existing,
                    &verified,
                    &envelope.canonical_base64url,
                )?;
                tx.commit().map_err(ats_certification_storage)?;
                return Ok(ats_certification_import_result(
                    "layout_observation",
                    &verified.authority.observation_id,
                    &verified,
                    true,
                ));
            }
            require_postgres_ats_layout_predecessor(&mut tx, &verified.authority)?;
            insert_postgres_ats_layout_observation(
                &mut tx,
                &verified,
                envelope,
                recorded_by,
                recorded_at_ms,
            )?;
            tx.commit().map_err(ats_certification_storage)?;
            Ok(ats_certification_import_result(
                "layout_observation",
                &verified.authority.observation_id,
                &verified,
                false,
            ))
        }
    })
}

fn require_ats_layout_predecessor(
    observation: &AtsLayoutObservationAuthority,
    predecessor: Option<(String, String, String, String, String, String, i64)>,
) -> Result<(), AtsCertificationAuthorityError> {
    match (
        observation.predecessor_observation_sha256.as_deref(),
        predecessor,
    ) {
        (None, None) => Ok(()),
        (
            Some(expected),
            Some((sha256, provider, target, variant, adapter, runner, observed_at_ms)),
        ) if expected == sha256
            && provider == observation.provider
            && target == observation.target_fingerprint_sha256
            && variant == observation.page_variant
            && adapter == observation.adapter_version
            && runner == observation.runner_target_sha256
            && observed_at_ms < observation.observed_at_ms =>
        {
            Ok(())
        }
        _ => Err(AtsCertificationAuthorityError::SequenceRegression),
    }
}

fn require_sqlite_ats_layout_predecessor(
    tx: &rusqlite::Transaction<'_>,
    observation: &AtsLayoutObservationAuthority,
) -> Result<(), AtsCertificationAuthorityError> {
    let predecessor = match observation.predecessor_observation_sha256.as_deref() {
        Some(sha256) => tx
            .query_row(
                "SELECT observation_sha256, provider, target_fingerprint_sha256,
                        page_variant, adapter_version, runner_target_sha256, observed_at_ms
                   FROM jobs_ats_certification_layout_observations
                  WHERE observation_sha256 = ?1",
                params![sha256],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                        row.get(6)?,
                    ))
                },
            )
            .optional()
            .map_err(ats_certification_storage)?,
        None => None,
    };
    require_ats_layout_predecessor(observation, predecessor)
}

fn require_postgres_ats_layout_predecessor(
    tx: &mut postgres::Transaction<'_>,
    observation: &AtsLayoutObservationAuthority,
) -> Result<(), AtsCertificationAuthorityError> {
    let predecessor = match observation.predecessor_observation_sha256.as_deref() {
        Some(sha256) => tx
            .query_opt(
                "SELECT observation_sha256, provider, target_fingerprint_sha256,
                        page_variant, adapter_version, runner_target_sha256, observed_at_ms
                   FROM jobs_ats_certification_layout_observations
                  WHERE observation_sha256 = $1",
                &[&sha256],
            )
            .map_err(ats_certification_storage)?
            .map(|row| {
                (
                    row.get(0),
                    row.get(1),
                    row.get(2),
                    row.get(3),
                    row.get(4),
                    row.get(5),
                    row.get(6),
                )
            }),
        None => None,
    };
    require_ats_layout_predecessor(observation, predecessor)
}

fn insert_sqlite_ats_layout_observation(
    tx: &rusqlite::Transaction<'_>,
    verified: &VerifiedAtsCertificationEnvelope<AtsLayoutObservationAuthority>,
    envelope: &AtsCertificationAuthorityEnvelope,
    recorded_by: &str,
    recorded_at_ms: i64,
) -> Result<(), AtsCertificationAuthorityError> {
    let value = &verified.authority;
    tx.execute(
        "INSERT INTO jobs_ats_certification_layout_observations (
           observation_sha256, observation_id, provider, target_fingerprint_sha256,
           page_variant, surface_sha256, adapter_version, runner_target_sha256,
           evidence_class, predecessor_observation_sha256,
           canonical_observation_base64url, authorization_sha256, trust_policy_sha256,
           canonical_authorization_base64url, observed_at_ms, issued_at_ms, expires_at_ms,
           recorded_by, recorded_at_ms
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13,
                   ?14, ?15, ?16, ?17, ?18, ?19)",
        params![
            verified.authority_sha256,
            value.observation_id,
            value.provider,
            value.target_fingerprint_sha256,
            value.page_variant,
            value.surface_sha256,
            value.adapter_version,
            value.runner_target_sha256,
            value.evidence_class,
            value.predecessor_observation_sha256,
            envelope.canonical_base64url,
            verified.authorization_sha256,
            verified.trust_policy_sha256,
            envelope.authorization_base64url,
            value.observed_at_ms,
            value.issued_at_ms,
            value.expires_at_ms,
            recorded_by,
            recorded_at_ms,
        ],
    )
    .map_err(ats_certification_storage)?;
    Ok(())
}

fn insert_postgres_ats_layout_observation(
    tx: &mut postgres::Transaction<'_>,
    verified: &VerifiedAtsCertificationEnvelope<AtsLayoutObservationAuthority>,
    envelope: &AtsCertificationAuthorityEnvelope,
    recorded_by: &str,
    recorded_at_ms: i64,
) -> Result<(), AtsCertificationAuthorityError> {
    let value = &verified.authority;
    tx.execute(
        "INSERT INTO jobs_ats_certification_layout_observations (
           observation_sha256, observation_id, provider, target_fingerprint_sha256,
           page_variant, surface_sha256, adapter_version, runner_target_sha256,
           evidence_class, predecessor_observation_sha256,
           canonical_observation_base64url, authorization_sha256, trust_policy_sha256,
           canonical_authorization_base64url, observed_at_ms, issued_at_ms, expires_at_ms,
           recorded_by, recorded_at_ms
         ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13,
                   $14, $15, $16, $17, $18, $19)",
        &[
            &verified.authority_sha256,
            &value.observation_id,
            &value.provider,
            &value.target_fingerprint_sha256,
            &value.page_variant,
            &value.surface_sha256,
            &value.adapter_version,
            &value.runner_target_sha256,
            &value.evidence_class,
            &value.predecessor_observation_sha256,
            &envelope.canonical_base64url,
            &verified.authorization_sha256,
            &verified.trust_policy_sha256,
            &envelope.authorization_base64url,
            &value.observed_at_ms,
            &value.issued_at_ms,
            &value.expires_at_ms,
            &recorded_by,
            &recorded_at_ms,
        ],
    )
    .map_err(ats_certification_storage)?;
    Ok(())
}

pub fn import_ats_certification_evidence(
    pool: &DbPool,
    envelope: &AtsCertificationAuthorityEnvelope,
    recorded_by: &str,
) -> Result<AtsCertificationImportResult, AtsCertificationAuthorityError> {
    import_ats_certification_evidence_at(pool, envelope, recorded_by, now_ms())
}

fn import_ats_certification_evidence_at(
    pool: &DbPool,
    envelope: &AtsCertificationAuthorityEnvelope,
    recorded_by: &str,
    recorded_at_ms: i64,
) -> Result<AtsCertificationImportResult, AtsCertificationAuthorityError> {
    validate_ats_recorded_by(recorded_by)?;
    let (trust_policy_sha256, trust_policy) =
        load_current_ats_certification_trust_policy(pool, recorded_at_ms)?;
    let mut verified =
        verify_ats_evidence_envelope(envelope, &trust_policy.delegated_trust, recorded_at_ms)?;
    if verified.authority.policy_sha256 != trust_policy_sha256 {
        return Err(AtsCertificationAuthorityError::InvalidTrustPolicy);
    }
    require_ats_authorization_clock_skew(
        envelope,
        verified.authority.issued_at_ms,
        trust_policy
            .certification_requirements
            .maximum_clock_skew_ms,
    )?;
    verified.trust_policy_sha256 = trust_policy_sha256;
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get().map_err(ats_certification_storage)?;
            let tx = conn
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(ats_certification_storage)?;
            require_current_sqlite_ats_trust_policy(
                &tx,
                &verified.trust_policy_sha256,
                recorded_at_ms,
            )?;
            let existing: Option<(String, String, String, String)> = tx
                .query_row(
                    "SELECT evidence_sha256, canonical_evidence_base64url,
                            authorization_sha256, trust_policy_sha256
                       FROM jobs_ats_certification_evidence
                      WHERE evidence_id = ?1 OR evidence_sha256 = ?2",
                    params![verified.authority.evidence_id, verified.authority_sha256],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
                )
                .optional()
                .map_err(ats_certification_storage)?;
            if let Some(existing) = existing {
                require_exact_ats_authority_replay(
                    existing,
                    &verified,
                    &envelope.canonical_base64url,
                )?;
                tx.commit().map_err(ats_certification_storage)?;
                return Ok(ats_certification_import_result(
                    "evidence",
                    &verified.authority.evidence_id,
                    &verified,
                    true,
                ));
            }
            insert_sqlite_ats_evidence(&tx, &verified, envelope, recorded_by, recorded_at_ms)?;
            tx.commit().map_err(ats_certification_storage)?;
            Ok(ats_certification_import_result(
                "evidence",
                &verified.authority.evidence_id,
                &verified,
                false,
            ))
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg().map_err(ats_certification_storage)?;
            let mut tx = conn.transaction().map_err(ats_certification_storage)?;
            lock_postgres_ats_certification(&mut tx)?;
            require_current_postgres_ats_trust_policy(
                &mut tx,
                &verified.trust_policy_sha256,
                recorded_at_ms,
            )?;
            let existing = tx
                .query_opt(
                    "SELECT evidence_sha256, canonical_evidence_base64url,
                            authorization_sha256, trust_policy_sha256
                       FROM jobs_ats_certification_evidence
                      WHERE evidence_id = $1 OR evidence_sha256 = $2",
                    &[&verified.authority.evidence_id, &verified.authority_sha256],
                )
                .map_err(ats_certification_storage)?
                .map(|row| (row.get(0), row.get(1), row.get(2), row.get(3)));
            if let Some(existing) = existing {
                require_exact_ats_authority_replay(
                    existing,
                    &verified,
                    &envelope.canonical_base64url,
                )?;
                tx.commit().map_err(ats_certification_storage)?;
                return Ok(ats_certification_import_result(
                    "evidence",
                    &verified.authority.evidence_id,
                    &verified,
                    true,
                ));
            }
            insert_postgres_ats_evidence(
                &mut tx,
                &verified,
                envelope,
                recorded_by,
                recorded_at_ms,
            )?;
            tx.commit().map_err(ats_certification_storage)?;
            Ok(ats_certification_import_result(
                "evidence",
                &verified.authority.evidence_id,
                &verified,
                false,
            ))
        }
    })
}

fn require_exact_ats_authority_replay<T>(
    existing: (String, String, String, String),
    verified: &VerifiedAtsCertificationEnvelope<T>,
    canonical_base64url: &str,
) -> Result<(), AtsCertificationAuthorityError> {
    if existing.0 != verified.authority_sha256
        || existing.1 != canonical_base64url
        || existing.2 != verified.authorization_sha256
        || existing.3 != verified.trust_policy_sha256
    {
        return Err(AtsCertificationAuthorityError::IdentityConflict);
    }
    Ok(())
}

fn insert_sqlite_ats_evidence(
    tx: &rusqlite::Transaction<'_>,
    verified: &VerifiedAtsCertificationEnvelope<AtsCertificationEvidenceAuthority>,
    envelope: &AtsCertificationAuthorityEnvelope,
    recorded_by: &str,
    recorded_at_ms: i64,
) -> Result<(), AtsCertificationAuthorityError> {
    let evidence = &verified.authority;
    tx.execute(
        "INSERT INTO jobs_ats_certification_evidence (
           evidence_sha256, evidence_id, provider, target_key, variant_key,
           surface_sha256, source_kind, object_key, object_sha256,
           object_size_bytes, media_type, provenance_sha256, authorization_ref,
           sanitizer_version, canonical_evidence_base64url, authorization_sha256,
           trust_policy_sha256, canonical_authorization_base64url, captured_at_ms,
           issued_at_ms, expires_at_ms, recorded_by, recorded_at_ms
         ) VALUES (
           ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14,
           ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23
         )",
        params![
            verified.authority_sha256,
            evidence.evidence_id,
            evidence.provider,
            evidence.target_key,
            evidence.variant_key,
            evidence.surface_sha256,
            evidence.source_kind,
            evidence.object_key,
            evidence.object_sha256,
            evidence.object_size_bytes,
            evidence.media_type,
            evidence.provenance_sha256,
            evidence.authorization_ref,
            evidence.sanitizer_version,
            envelope.canonical_base64url,
            verified.authorization_sha256,
            verified.trust_policy_sha256,
            envelope.authorization_base64url,
            evidence.captured_at_ms,
            evidence.issued_at_ms,
            evidence.expires_at_ms,
            recorded_by,
            recorded_at_ms,
        ],
    )
    .map_err(ats_certification_storage)?;
    Ok(())
}

fn insert_postgres_ats_evidence(
    tx: &mut postgres::Transaction<'_>,
    verified: &VerifiedAtsCertificationEnvelope<AtsCertificationEvidenceAuthority>,
    envelope: &AtsCertificationAuthorityEnvelope,
    recorded_by: &str,
    recorded_at_ms: i64,
) -> Result<(), AtsCertificationAuthorityError> {
    let evidence = &verified.authority;
    tx.execute(
        "INSERT INTO jobs_ats_certification_evidence (
           evidence_sha256, evidence_id, provider, target_key, variant_key,
           surface_sha256, source_kind, object_key, object_sha256,
           object_size_bytes, media_type, provenance_sha256, authorization_ref,
           sanitizer_version, canonical_evidence_base64url, authorization_sha256,
           trust_policy_sha256, canonical_authorization_base64url, captured_at_ms,
           issued_at_ms, expires_at_ms, recorded_by, recorded_at_ms
         ) VALUES (
           $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14,
           $15, $16, $17, $18, $19, $20, $21, $22, $23
         )",
        &[
            &verified.authority_sha256,
            &evidence.evidence_id,
            &evidence.provider,
            &evidence.target_key,
            &evidence.variant_key,
            &evidence.surface_sha256,
            &evidence.source_kind,
            &evidence.object_key,
            &evidence.object_sha256,
            &evidence.object_size_bytes,
            &evidence.media_type,
            &evidence.provenance_sha256,
            &evidence.authorization_ref,
            &evidence.sanitizer_version,
            &envelope.canonical_base64url,
            &verified.authorization_sha256,
            &verified.trust_policy_sha256,
            &envelope.authorization_base64url,
            &evidence.captured_at_ms,
            &evidence.issued_at_ms,
            &evidence.expires_at_ms,
            &recorded_by,
            &recorded_at_ms,
        ],
    )
    .map_err(ats_certification_storage)?;
    Ok(())
}

fn lock_postgres_ats_certification(
    tx: &mut postgres::Transaction<'_>,
) -> Result<(), AtsCertificationAuthorityError> {
    tx.query_one(
        "SELECT pg_advisory_xact_lock(hashtextextended('jobs-ats-certification-authority', 0))",
        &[],
    )
    .map_err(ats_certification_storage)?;
    Ok(())
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AtsCertificationCanonicalCanaryAllowlist<'a> {
    schema_version: i64,
    allowlist_id: &'a str,
    account_ids: &'a [String],
    approval_ref: &'a str,
    not_before_ms: i64,
    expires_at_ms: i64,
}

fn canonical_ats_certification_canary_allowlist(
    request: &AtsCertificationCanaryAllowlistImportRequest,
    now_ms: i64,
) -> Result<(Vec<String>, Vec<u8>, String), AtsCertificationAuthorityError> {
    if request.schema_version != 1
        || !ats_certification_text(&request.allowlist_id, 1, 240)
        || !ats_certification_text(&request.approval_ref, 1, 240)
        || request.account_ids.is_empty()
        || request.account_ids.len() > ATS_CERTIFICATION_MAX_CANARY_ALLOWLIST_MEMBERS
        || !ats_certification_safe_integer(request.not_before_ms, false)
        || !ats_certification_safe_integer(request.expires_at_ms, true)
        || !ats_certification_safe_integer(now_ms, false)
        || request.expires_at_ms <= request.not_before_ms
        || request.expires_at_ms <= now_ms
    {
        return Err(AtsCertificationAuthorityError::InvalidAuthority);
    }
    let mut account_ids = request.account_ids.clone();
    if account_ids
        .iter()
        .any(|account_id| !ats_certification_text(account_id, 1, 240))
    {
        return Err(AtsCertificationAuthorityError::InvalidAuthority);
    }
    account_ids.sort();
    if account_ids.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err(AtsCertificationAuthorityError::IdentityConflict);
    }
    let canonical_authority = AtsCertificationCanonicalCanaryAllowlist {
        schema_version: request.schema_version,
        allowlist_id: &request.allowlist_id,
        account_ids: &account_ids,
        approval_ref: &request.approval_ref,
        not_before_ms: request.not_before_ms,
        expires_at_ms: request.expires_at_ms,
    };
    let canonical = ats_certification_canonical_json(&canonical_authority)
        .map_err(|_| AtsCertificationAuthorityError::InvalidAuthority)?;
    if canonical.len() > ATS_CERTIFICATION_MAX_CANONICAL_BYTES {
        return Err(AtsCertificationAuthorityError::InvalidAuthority);
    }
    let allowlist_sha256 = ats_certification_sha256(&canonical);
    Ok((account_ids, canonical, allowlist_sha256))
}

pub fn import_ats_certification_canary_allowlist(
    pool: &DbPool,
    request: &AtsCertificationCanaryAllowlistImportRequest,
    approved_by: &str,
    now_ms: i64,
) -> Result<AtsCertificationCanaryAllowlistImportResult, AtsCertificationAuthorityError> {
    validate_ats_recorded_by(approved_by)?;
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get().map_err(ats_certification_storage)?;
            let tx = conn
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(ats_certification_storage)?;
            let result = import_ats_certification_canary_allowlist_sqlite_tx(
                &tx,
                request,
                approved_by,
                now_ms,
            )?;
            tx.commit().map_err(ats_certification_storage)?;
            Ok(result)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg().map_err(ats_certification_storage)?;
            let mut tx = conn
                .build_transaction()
                .isolation_level(postgres::IsolationLevel::Serializable)
                .start()
                .map_err(ats_certification_storage)?;
            lock_postgres_ats_certification(&mut tx)?;
            let result = import_ats_certification_canary_allowlist_postgres_tx(
                &mut tx,
                request,
                approved_by,
                now_ms,
            )?;
            tx.commit().map_err(ats_certification_storage)?;
            Ok(result)
        }
    })
}

pub fn import_ats_certification_canary_allowlist_sqlite_tx(
    tx: &rusqlite::Transaction<'_>,
    request: &AtsCertificationCanaryAllowlistImportRequest,
    approved_by: &str,
    now_ms: i64,
) -> Result<AtsCertificationCanaryAllowlistImportResult, AtsCertificationAuthorityError> {
    validate_ats_recorded_by(approved_by)?;
    let (account_ids, canonical, allowlist_sha256) =
        canonical_ats_certification_canary_allowlist(request, now_ms)?;
    let canonical_base64url = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(canonical);
    let existing = tx
        .query_row(
            "SELECT allowlist_id, allowlist_sha256, canonical_allowlist_base64url,
                    member_count, not_before_ms, expires_at_ms
               FROM jobs_ats_certification_canary_allowlists
              WHERE allowlist_id = ?1 OR allowlist_sha256 = ?2",
            params![request.allowlist_id, allowlist_sha256],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, i64>(5)?,
                ))
            },
        )
        .optional()
        .map_err(ats_certification_storage)?;
    if let Some(existing) = existing {
        if existing.0 != request.allowlist_id
            || existing.1 != allowlist_sha256
            || existing.2 != canonical_base64url
            || existing.3 != account_ids.len() as i64
            || existing.4 != request.not_before_ms
            || existing.5 != request.expires_at_ms
        {
            return Err(AtsCertificationAuthorityError::IdentityConflict);
        }
        return Ok(AtsCertificationCanaryAllowlistImportResult {
            allowlist_id: existing.0,
            allowlist_sha256: existing.1,
            member_count: existing.3,
            not_before_ms: existing.4,
            expires_at_ms: existing.5,
            replayed: true,
        });
    }
    tx.execute(
        "INSERT INTO jobs_ats_certification_canary_allowlists (
           allowlist_sha256, allowlist_id, canonical_allowlist_base64url,
           member_count, approval_ref, not_before_ms, expires_at_ms,
           approved_by, approved_at_ms
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            allowlist_sha256,
            request.allowlist_id,
            canonical_base64url,
            account_ids.len() as i64,
            request.approval_ref,
            request.not_before_ms,
            request.expires_at_ms,
            approved_by,
            now_ms,
        ],
    )
    .map_err(ats_certification_storage)?;
    for (ordinal, account_id) in account_ids.iter().enumerate() {
        tx.execute(
            "INSERT INTO jobs_ats_certification_canary_allowlist_members (
               allowlist_sha256, ordinal, account_id
             ) VALUES (?1, ?2, ?3)",
            params![allowlist_sha256, ordinal as i64, account_id],
        )
        .map_err(ats_certification_storage)?;
    }
    Ok(AtsCertificationCanaryAllowlistImportResult {
        allowlist_id: request.allowlist_id.clone(),
        allowlist_sha256,
        member_count: account_ids.len() as i64,
        not_before_ms: request.not_before_ms,
        expires_at_ms: request.expires_at_ms,
        replayed: false,
    })
}

pub fn import_ats_certification_canary_allowlist_postgres_tx(
    tx: &mut postgres::Transaction<'_>,
    request: &AtsCertificationCanaryAllowlistImportRequest,
    approved_by: &str,
    now_ms: i64,
) -> Result<AtsCertificationCanaryAllowlistImportResult, AtsCertificationAuthorityError> {
    validate_ats_recorded_by(approved_by)?;
    lock_postgres_ats_certification(tx)?;
    let (account_ids, canonical, allowlist_sha256) =
        canonical_ats_certification_canary_allowlist(request, now_ms)?;
    let canonical_base64url = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(canonical);
    let existing = tx
        .query_opt(
            "SELECT allowlist_id, allowlist_sha256, canonical_allowlist_base64url,
                    member_count, not_before_ms, expires_at_ms
               FROM jobs_ats_certification_canary_allowlists
              WHERE allowlist_id = $1 OR allowlist_sha256 = $2 FOR UPDATE",
            &[&request.allowlist_id, &allowlist_sha256],
        )
        .map_err(ats_certification_storage)?;
    if let Some(row) = existing {
        let existing = (
            row.get::<_, String>(0),
            row.get::<_, String>(1),
            row.get::<_, String>(2),
            row.get::<_, i64>(3),
            row.get::<_, i64>(4),
            row.get::<_, i64>(5),
        );
        if existing.0 != request.allowlist_id
            || existing.1 != allowlist_sha256
            || existing.2 != canonical_base64url
            || existing.3 != account_ids.len() as i64
            || existing.4 != request.not_before_ms
            || existing.5 != request.expires_at_ms
        {
            return Err(AtsCertificationAuthorityError::IdentityConflict);
        }
        return Ok(AtsCertificationCanaryAllowlistImportResult {
            allowlist_id: existing.0,
            allowlist_sha256: existing.1,
            member_count: existing.3,
            not_before_ms: existing.4,
            expires_at_ms: existing.5,
            replayed: true,
        });
    }
    tx.execute(
        "INSERT INTO jobs_ats_certification_canary_allowlists (
           allowlist_sha256, allowlist_id, canonical_allowlist_base64url,
           member_count, approval_ref, not_before_ms, expires_at_ms,
           approved_by, approved_at_ms
         ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
        &[
            &allowlist_sha256,
            &request.allowlist_id,
            &canonical_base64url,
            &(account_ids.len() as i64),
            &request.approval_ref,
            &request.not_before_ms,
            &request.expires_at_ms,
            &approved_by,
            &now_ms,
        ],
    )
    .map_err(ats_certification_storage)?;
    for (ordinal, account_id) in account_ids.iter().enumerate() {
        tx.execute(
            "INSERT INTO jobs_ats_certification_canary_allowlist_members (
               allowlist_sha256, ordinal, account_id
             ) VALUES ($1, $2, $3)",
            &[&allowlist_sha256, &(ordinal as i64), account_id],
        )
        .map_err(ats_certification_storage)?;
    }
    Ok(AtsCertificationCanaryAllowlistImportResult {
        allowlist_id: request.allowlist_id.clone(),
        allowlist_sha256,
        member_count: account_ids.len() as i64,
        not_before_ms: request.not_before_ms,
        expires_at_ms: request.expires_at_ms,
        replayed: false,
    })
}

pub fn revoke_ats_certification_canary_allowlist(
    pool: &DbPool,
    request: &AtsCertificationCanaryAllowlistRevocationRequest,
    revoked_by: &str,
    now_ms: i64,
) -> Result<AtsCertificationCanaryAllowlistRevocationResult, AtsCertificationAuthorityError> {
    validate_ats_recorded_by(revoked_by)?;
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get().map_err(ats_certification_storage)?;
            let tx = conn
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(ats_certification_storage)?;
            let result = revoke_ats_certification_canary_allowlist_sqlite_tx(
                &tx, request, revoked_by, now_ms,
            )?;
            tx.commit().map_err(ats_certification_storage)?;
            Ok(result)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg().map_err(ats_certification_storage)?;
            let mut tx = conn
                .build_transaction()
                .isolation_level(postgres::IsolationLevel::Serializable)
                .start()
                .map_err(ats_certification_storage)?;
            lock_postgres_ats_certification(&mut tx)?;
            let result = revoke_ats_certification_canary_allowlist_postgres_tx(
                &mut tx, request, revoked_by, now_ms,
            )?;
            tx.commit().map_err(ats_certification_storage)?;
            Ok(result)
        }
    })
}

pub fn revoke_ats_certification_canary_allowlist_sqlite_tx(
    tx: &rusqlite::Transaction<'_>,
    request: &AtsCertificationCanaryAllowlistRevocationRequest,
    revoked_by: &str,
    now_ms: i64,
) -> Result<AtsCertificationCanaryAllowlistRevocationResult, AtsCertificationAuthorityError> {
    validate_ats_recorded_by(revoked_by)?;
    if !ats_certification_hex64(&request.allowlist_sha256)
        || !ats_certification_text(&request.revocation_ref, 1, 240)
        || !ats_certification_safe_integer(now_ms, false)
    {
        return Err(AtsCertificationAuthorityError::InvalidAuthority);
    }
    let exists: i64 = tx
        .query_row(
            "SELECT COUNT(*) FROM jobs_ats_certification_canary_allowlists
              WHERE allowlist_sha256 = ?1",
            params![request.allowlist_sha256],
            |row| row.get(0),
        )
        .map_err(ats_certification_storage)?;
    if exists != 1 {
        return Err(AtsCertificationAuthorityError::NotFound);
    }
    let existing = tx
        .query_row(
            "SELECT revocation_ref, revoked_at_ms
               FROM jobs_ats_certification_canary_allowlist_revocations
              WHERE allowlist_sha256 = ?1",
            params![request.allowlist_sha256],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
        )
        .optional()
        .map_err(ats_certification_storage)?;
    if let Some((revocation_ref, revoked_at_ms)) = existing {
        if revocation_ref != request.revocation_ref {
            return Err(AtsCertificationAuthorityError::IdentityConflict);
        }
        return Ok(AtsCertificationCanaryAllowlistRevocationResult {
            allowlist_sha256: request.allowlist_sha256.clone(),
            revocation_ref,
            revoked_at_ms,
            replayed: true,
        });
    }
    tx.execute(
        "INSERT INTO jobs_ats_certification_canary_allowlist_revocations (
           allowlist_sha256, revocation_ref, revoked_by, revoked_at_ms
         ) VALUES (?1, ?2, ?3, ?4)",
        params![
            request.allowlist_sha256,
            request.revocation_ref,
            revoked_by,
            now_ms,
        ],
    )
    .map_err(ats_certification_storage)?;
    Ok(AtsCertificationCanaryAllowlistRevocationResult {
        allowlist_sha256: request.allowlist_sha256.clone(),
        revocation_ref: request.revocation_ref.clone(),
        revoked_at_ms: now_ms,
        replayed: false,
    })
}

pub fn revoke_ats_certification_canary_allowlist_postgres_tx(
    tx: &mut postgres::Transaction<'_>,
    request: &AtsCertificationCanaryAllowlistRevocationRequest,
    revoked_by: &str,
    now_ms: i64,
) -> Result<AtsCertificationCanaryAllowlistRevocationResult, AtsCertificationAuthorityError> {
    validate_ats_recorded_by(revoked_by)?;
    lock_postgres_ats_certification(tx)?;
    if !ats_certification_hex64(&request.allowlist_sha256)
        || !ats_certification_text(&request.revocation_ref, 1, 240)
        || !ats_certification_safe_integer(now_ms, false)
    {
        return Err(AtsCertificationAuthorityError::InvalidAuthority);
    }
    if tx
        .query_opt(
            "SELECT allowlist_sha256 FROM jobs_ats_certification_canary_allowlists
              WHERE allowlist_sha256 = $1 FOR UPDATE",
            &[&request.allowlist_sha256],
        )
        .map_err(ats_certification_storage)?
        .is_none()
    {
        return Err(AtsCertificationAuthorityError::NotFound);
    }
    let existing = tx
        .query_opt(
            "SELECT revocation_ref, revoked_at_ms
               FROM jobs_ats_certification_canary_allowlist_revocations
              WHERE allowlist_sha256 = $1",
            &[&request.allowlist_sha256],
        )
        .map_err(ats_certification_storage)?;
    if let Some(row) = existing {
        let revocation_ref: String = row.get(0);
        let revoked_at_ms: i64 = row.get(1);
        if revocation_ref != request.revocation_ref {
            return Err(AtsCertificationAuthorityError::IdentityConflict);
        }
        return Ok(AtsCertificationCanaryAllowlistRevocationResult {
            allowlist_sha256: request.allowlist_sha256.clone(),
            revocation_ref,
            revoked_at_ms,
            replayed: true,
        });
    }
    tx.execute(
        "INSERT INTO jobs_ats_certification_canary_allowlist_revocations (
           allowlist_sha256, revocation_ref, revoked_by, revoked_at_ms
         ) VALUES ($1, $2, $3, $4)",
        &[
            &request.allowlist_sha256,
            &request.revocation_ref,
            &revoked_by,
            &now_ms,
        ],
    )
    .map_err(ats_certification_storage)?;
    Ok(AtsCertificationCanaryAllowlistRevocationResult {
        allowlist_sha256: request.allowlist_sha256.clone(),
        revocation_ref: request.revocation_ref.clone(),
        revoked_at_ms: now_ms,
        replayed: false,
    })
}

pub fn resolve_active_ats_certification_canary_allowlist_for_account(
    pool: &DbPool,
    account_id: &str,
    now_ms: i64,
) -> Result<Option<String>, AtsCertificationAuthorityError> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get().map_err(ats_certification_storage)?;
            let tx = conn
                .transaction_with_behavior(TransactionBehavior::Deferred)
                .map_err(ats_certification_storage)?;
            let result = resolve_active_ats_certification_canary_allowlist_for_account_sqlite_tx(
                &tx, account_id, now_ms,
            )?;
            tx.commit().map_err(ats_certification_storage)?;
            Ok(result)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg().map_err(ats_certification_storage)?;
            let mut tx = conn
                .build_transaction()
                .isolation_level(postgres::IsolationLevel::RepeatableRead)
                .read_only(true)
                .start()
                .map_err(ats_certification_storage)?;
            let result = resolve_active_ats_certification_canary_allowlist_for_account_postgres_tx(
                &mut tx, account_id, now_ms,
            )?;
            tx.commit().map_err(ats_certification_storage)?;
            Ok(result)
        }
    })
}

pub fn resolve_active_ats_certification_canary_allowlist_for_account_sqlite_tx(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    now_ms: i64,
) -> Result<Option<String>, AtsCertificationAuthorityError> {
    if !ats_certification_text(account_id, 1, 240) || !ats_certification_safe_integer(now_ms, false)
    {
        return Err(AtsCertificationAuthorityError::InvalidAuthority);
    }
    let mut stmt = tx
        .prepare(
            "SELECT allowlist.allowlist_sha256
               FROM jobs_ats_certification_canary_allowlist_members member
               JOIN jobs_ats_certification_canary_allowlists allowlist
                 ON allowlist.allowlist_sha256 = member.allowlist_sha256
              WHERE member.account_id = ?1 AND allowlist.not_before_ms <= ?2
                AND allowlist.expires_at_ms > ?2
                AND NOT EXISTS (
                  SELECT 1 FROM jobs_ats_certification_canary_allowlist_revocations revocation
                   WHERE revocation.allowlist_sha256 = allowlist.allowlist_sha256
                     AND revocation.revoked_at_ms <= ?2
                )
              ORDER BY allowlist.allowlist_sha256 LIMIT 2",
        )
        .map_err(ats_certification_storage)?;
    let rows = stmt
        .query_map(params![account_id, now_ms], |row| row.get::<_, String>(0))
        .map_err(ats_certification_storage)?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(ats_certification_storage)?;
    match rows.as_slice() {
        [] => Ok(None),
        [allowlist_sha256] => Ok(Some(allowlist_sha256.clone())),
        _ => Err(AtsCertificationAuthorityError::IdentityConflict),
    }
}

pub fn resolve_active_ats_certification_canary_allowlist_for_account_postgres_tx(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    now_ms: i64,
) -> Result<Option<String>, AtsCertificationAuthorityError> {
    if !ats_certification_text(account_id, 1, 240) || !ats_certification_safe_integer(now_ms, false)
    {
        return Err(AtsCertificationAuthorityError::InvalidAuthority);
    }
    let rows = tx
        .query(
            "SELECT allowlist.allowlist_sha256
               FROM jobs_ats_certification_canary_allowlist_members member
               JOIN jobs_ats_certification_canary_allowlists allowlist
                 ON allowlist.allowlist_sha256 = member.allowlist_sha256
              WHERE member.account_id = $1 AND allowlist.not_before_ms <= $2
                AND allowlist.expires_at_ms > $2
                AND NOT EXISTS (
                  SELECT 1 FROM jobs_ats_certification_canary_allowlist_revocations revocation
                   WHERE revocation.allowlist_sha256 = allowlist.allowlist_sha256
                     AND revocation.revoked_at_ms <= $2
                )
              ORDER BY allowlist.allowlist_sha256 LIMIT 2",
            &[&account_id, &now_ms],
        )
        .map_err(ats_certification_storage)?;
    match rows.as_slice() {
        [] => Ok(None),
        [row] => Ok(Some(row.get(0))),
        _ => Err(AtsCertificationAuthorityError::IdentityConflict),
    }
}

fn sqlite_ats_account_enrolled_in_allowlist(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    allowlist_sha256: &str,
    now_ms: i64,
) -> Result<bool, AtsCertificationAuthorityError> {
    let enrolled: i64 = tx
        .query_row(
            "SELECT EXISTS(
               SELECT 1 FROM jobs_ats_certification_canary_allowlists allowlist
               JOIN jobs_ats_certification_canary_allowlist_members member
                 ON member.allowlist_sha256 = allowlist.allowlist_sha256
                WHERE allowlist.allowlist_sha256 = ?1 AND member.account_id = ?2
                  AND allowlist.not_before_ms <= ?3 AND allowlist.expires_at_ms > ?3
                  AND NOT EXISTS (
                    SELECT 1 FROM jobs_ats_certification_canary_allowlist_revocations revocation
                     WHERE revocation.allowlist_sha256 = allowlist.allowlist_sha256
                       AND revocation.revoked_at_ms <= ?3
                  )
             )",
            params![allowlist_sha256, account_id, now_ms],
            |row| row.get(0),
        )
        .map_err(ats_certification_storage)?;
    Ok(enrolled != 0)
}

fn postgres_ats_account_enrolled_in_allowlist(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    allowlist_sha256: &str,
    now_ms: i64,
) -> Result<bool, AtsCertificationAuthorityError> {
    Ok(tx
        .query_one(
            "SELECT EXISTS(
               SELECT 1 FROM jobs_ats_certification_canary_allowlists allowlist
               JOIN jobs_ats_certification_canary_allowlist_members member
                 ON member.allowlist_sha256 = allowlist.allowlist_sha256
                WHERE allowlist.allowlist_sha256 = $1 AND member.account_id = $2
                  AND allowlist.not_before_ms <= $3 AND allowlist.expires_at_ms > $3
                  AND NOT EXISTS (
                    SELECT 1 FROM jobs_ats_certification_canary_allowlist_revocations revocation
                     WHERE revocation.allowlist_sha256 = allowlist.allowlist_sha256
                       AND revocation.revoked_at_ms <= $3
                  )
             )",
            &[&allowlist_sha256, &account_id, &now_ms],
        )
        .map_err(ats_certification_storage)?
        .get(0))
}

fn resolve_sqlite_ats_canary_allowlist_for_target_account(
    tx: &rusqlite::Transaction<'_>,
    target_evidence: &AtsCertificationFreshTargetEvidence,
    account_id: &str,
    now_ms: i64,
) -> Result<Option<String>, AtsCertificationAuthorityError> {
    let (provider, target_key) =
        validate_ats_certification_fresh_target_evidence(target_evidence, now_ms)?;
    let mut stmt = tx
        .prepare(
            "SELECT DISTINCT activation.account_allowlist_sha256
               FROM jobs_ats_certification_heads head
               JOIN jobs_ats_certification_activations activation
                 ON activation.activation_sha256 = head.current_activation_sha256
                AND activation.scope_sha256 = head.scope_sha256
                AND activation.channel = head.channel
               JOIN jobs_ats_certification_manifests manifest
                 ON manifest.manifest_sha256 = activation.manifest_sha256
               JOIN jobs_ats_certification_trust_head trust ON trust.singleton_id = 1
               JOIN jobs_ats_certification_canary_allowlists allowlist
                 ON allowlist.allowlist_sha256 = activation.account_allowlist_sha256
               JOIN jobs_ats_certification_canary_allowlist_members member
                 ON member.allowlist_sha256 = allowlist.allowlist_sha256
              WHERE head.channel = 'canary' AND manifest.provider = ?1
                AND manifest.target_key = ?2 AND member.account_id = ?3
                AND activation.trust_policy_sha256 = trust.current_policy_sha256
                AND manifest.trust_policy_sha256 = trust.current_policy_sha256
                AND activation.not_before_ms <= ?4 AND activation.expires_at_ms > ?4
                AND manifest.not_before_ms <= ?4 AND manifest.expires_at_ms > ?4
                AND allowlist.not_before_ms <= ?4 AND allowlist.expires_at_ms > ?4
                AND NOT EXISTS (
                  SELECT 1 FROM jobs_ats_certification_canary_allowlist_revocations revocation
                   WHERE revocation.allowlist_sha256 = allowlist.allowlist_sha256
                     AND revocation.revoked_at_ms <= ?4
                )
              ORDER BY activation.account_allowlist_sha256 LIMIT 2",
        )
        .map_err(ats_certification_storage)?;
    let values = stmt
        .query_map(params![provider, target_key, account_id, now_ms], |row| {
            row.get::<_, String>(0)
        })
        .map_err(ats_certification_storage)?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(ats_certification_storage)?;
    match values.as_slice() {
        [] => Ok(None),
        [value] => Ok(Some(value.clone())),
        _ => Err(AtsCertificationAuthorityError::IdentityConflict),
    }
}

fn resolve_postgres_ats_canary_allowlist_for_target_account(
    tx: &mut postgres::Transaction<'_>,
    target_evidence: &AtsCertificationFreshTargetEvidence,
    account_id: &str,
    now_ms: i64,
) -> Result<Option<String>, AtsCertificationAuthorityError> {
    let (provider, target_key) =
        validate_ats_certification_fresh_target_evidence(target_evidence, now_ms)?;
    let rows = tx
        .query(
            "SELECT DISTINCT activation.account_allowlist_sha256
               FROM jobs_ats_certification_heads head
               JOIN jobs_ats_certification_activations activation
                 ON activation.activation_sha256 = head.current_activation_sha256
                AND activation.scope_sha256 = head.scope_sha256
                AND activation.channel = head.channel
               JOIN jobs_ats_certification_manifests manifest
                 ON manifest.manifest_sha256 = activation.manifest_sha256
               JOIN jobs_ats_certification_trust_head trust ON trust.singleton_id = 1
               JOIN jobs_ats_certification_canary_allowlists allowlist
                 ON allowlist.allowlist_sha256 = activation.account_allowlist_sha256
               JOIN jobs_ats_certification_canary_allowlist_members member
                 ON member.allowlist_sha256 = allowlist.allowlist_sha256
              WHERE head.channel = 'canary' AND manifest.provider = $1
                AND manifest.target_key = $2 AND member.account_id = $3
                AND activation.trust_policy_sha256 = trust.current_policy_sha256
                AND manifest.trust_policy_sha256 = trust.current_policy_sha256
                AND activation.not_before_ms <= $4 AND activation.expires_at_ms > $4
                AND manifest.not_before_ms <= $4 AND manifest.expires_at_ms > $4
                AND allowlist.not_before_ms <= $4 AND allowlist.expires_at_ms > $4
                AND NOT EXISTS (
                  SELECT 1 FROM jobs_ats_certification_canary_allowlist_revocations revocation
                   WHERE revocation.allowlist_sha256 = allowlist.allowlist_sha256
                     AND revocation.revoked_at_ms <= $4
                )
              ORDER BY activation.account_allowlist_sha256 LIMIT 2",
            &[&provider, &target_key, &account_id, &now_ms],
        )
        .map_err(ats_certification_storage)?;
    match rows.as_slice() {
        [] => Ok(None),
        [row] => Ok(Some(row.get(0))),
        _ => Err(AtsCertificationAuthorityError::IdentityConflict),
    }
}

fn require_sqlite_ats_activation_canary_allowlist(
    tx: &rusqlite::Transaction<'_>,
    activation: &AtsCertificationActivationAuthority,
    now_ms: i64,
) -> Result<(), AtsCertificationAuthorityError> {
    if activation.channel != "canary" {
        return Ok(());
    }
    let allowlist_sha256 = activation
        .account_allowlist_sha256
        .as_deref()
        .ok_or(AtsCertificationAuthorityError::InvalidAuthority)?;
    let available: i64 = tx
        .query_row(
            "SELECT COUNT(*) FROM jobs_ats_certification_canary_allowlists allowlist
              WHERE allowlist.allowlist_sha256 = ?1
                AND allowlist.not_before_ms <= ?2 AND allowlist.expires_at_ms >= ?3
                AND allowlist.expires_at_ms > ?4
                AND NOT EXISTS (
                  SELECT 1 FROM jobs_ats_certification_canary_allowlist_revocations revocation
                   WHERE revocation.allowlist_sha256 = allowlist.allowlist_sha256
                     AND revocation.revoked_at_ms <= ?4
                )",
            params![
                allowlist_sha256,
                activation.not_before_ms,
                activation.expires_at_ms,
                now_ms,
            ],
            |row| row.get(0),
        )
        .map_err(ats_certification_storage)?;
    if available != 1 {
        return Err(AtsCertificationAuthorityError::ScopeMismatch);
    }
    Ok(())
}

fn require_postgres_ats_activation_canary_allowlist(
    tx: &mut postgres::Transaction<'_>,
    activation: &AtsCertificationActivationAuthority,
    now_ms: i64,
) -> Result<(), AtsCertificationAuthorityError> {
    if activation.channel != "canary" {
        return Ok(());
    }
    let allowlist_sha256 = activation
        .account_allowlist_sha256
        .as_deref()
        .ok_or(AtsCertificationAuthorityError::InvalidAuthority)?;
    let available: bool = tx
        .query_one(
            "SELECT EXISTS(
               SELECT 1 FROM jobs_ats_certification_canary_allowlists allowlist
                WHERE allowlist.allowlist_sha256 = $1
                  AND allowlist.not_before_ms <= $2 AND allowlist.expires_at_ms >= $3
                  AND allowlist.expires_at_ms > $4
                  AND NOT EXISTS (
                    SELECT 1 FROM jobs_ats_certification_canary_allowlist_revocations revocation
                     WHERE revocation.allowlist_sha256 = allowlist.allowlist_sha256
                       AND revocation.revoked_at_ms <= $4
                  )
             )",
            &[
                &allowlist_sha256,
                &activation.not_before_ms,
                &activation.expires_at_ms,
                &now_ms,
            ],
        )
        .map_err(ats_certification_storage)?
        .get(0);
    if !available {
        return Err(AtsCertificationAuthorityError::ScopeMismatch);
    }
    Ok(())
}

fn require_sqlite_ats_stored_activation_canary_allowlist(
    tx: &rusqlite::Transaction<'_>,
    activation: &StoredAtsActivationHeadAuthority,
    now_ms: i64,
) -> Result<(), AtsCertificationAuthorityError> {
    if activation.channel != "canary" {
        return Ok(());
    }
    let allowlist_sha256 = activation
        .account_allowlist_sha256
        .as_deref()
        .ok_or(AtsCertificationAuthorityError::InvalidAuthority)?;
    let available: i64 = tx
        .query_row(
            "SELECT COUNT(*) FROM jobs_ats_certification_canary_allowlists allowlist
              WHERE allowlist.allowlist_sha256 = ?1
                AND allowlist.not_before_ms <= ?2 AND allowlist.expires_at_ms > ?2
                AND NOT EXISTS (
                  SELECT 1 FROM jobs_ats_certification_canary_allowlist_revocations revocation
                   WHERE revocation.allowlist_sha256 = allowlist.allowlist_sha256
                     AND revocation.revoked_at_ms <= ?2
                )",
            params![allowlist_sha256, now_ms],
            |row| row.get(0),
        )
        .map_err(ats_certification_storage)?;
    if available != 1 {
        return Err(AtsCertificationAuthorityError::ScopeMismatch);
    }
    Ok(())
}

fn require_postgres_ats_stored_activation_canary_allowlist(
    tx: &mut postgres::Transaction<'_>,
    activation: &StoredAtsActivationHeadAuthority,
    now_ms: i64,
) -> Result<(), AtsCertificationAuthorityError> {
    if activation.channel != "canary" {
        return Ok(());
    }
    let allowlist_sha256 = activation
        .account_allowlist_sha256
        .as_deref()
        .ok_or(AtsCertificationAuthorityError::InvalidAuthority)?;
    let available: bool = tx
        .query_one(
            "SELECT EXISTS(
               SELECT 1 FROM jobs_ats_certification_canary_allowlists allowlist
                WHERE allowlist.allowlist_sha256 = $1
                  AND allowlist.not_before_ms <= $2 AND allowlist.expires_at_ms > $2
                  AND NOT EXISTS (
                    SELECT 1 FROM jobs_ats_certification_canary_allowlist_revocations revocation
                     WHERE revocation.allowlist_sha256 = allowlist.allowlist_sha256
                       AND revocation.revoked_at_ms <= $2
                  )
             )",
            &[&allowlist_sha256, &now_ms],
        )
        .map_err(ats_certification_storage)?
        .get(0);
    if !available {
        return Err(AtsCertificationAuthorityError::ScopeMismatch);
    }
    Ok(())
}

pub fn import_ats_certification_manifest(
    pool: &DbPool,
    envelope: &AtsCertificationAuthorityEnvelope,
    recorded_by: &str,
) -> Result<AtsCertificationImportResult, AtsCertificationAuthorityError> {
    import_ats_certification_manifest_at(pool, envelope, recorded_by, now_ms())
}

fn import_ats_certification_manifest_at(
    pool: &DbPool,
    envelope: &AtsCertificationAuthorityEnvelope,
    recorded_by: &str,
    recorded_at_ms: i64,
) -> Result<AtsCertificationImportResult, AtsCertificationAuthorityError> {
    validate_ats_recorded_by(recorded_by)?;
    let (trust_policy_sha256, trust_policy) =
        load_current_ats_certification_trust_policy(pool, recorded_at_ms)?;
    let mut verified =
        verify_ats_manifest_envelope(envelope, &trust_policy.delegated_trust, recorded_at_ms)?;
    if verified.authority.policy_sha256 != trust_policy_sha256 {
        return Err(AtsCertificationAuthorityError::InvalidTrustPolicy);
    }
    require_ats_manifest_policy(&verified.authority, &trust_policy, recorded_at_ms)?;
    require_ats_authorization_clock_skew(
        envelope,
        verified.authority.issued_at_ms,
        trust_policy
            .certification_requirements
            .maximum_clock_skew_ms,
    )?;
    let canonical_manifest_size = ats_certification_decode_base64url_bounded(
        &envelope.canonical_base64url,
        ATS_CERTIFICATION_MAX_CANONICAL_BYTES,
    )?
    .len() as i64;
    if canonical_manifest_size
        > trust_policy
            .certification_requirements
            .maximum_manifest_size_bytes
    {
        return Err(AtsCertificationAuthorityError::InvalidTrustPolicy);
    }
    verified.trust_policy_sha256 = trust_policy_sha256;
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get().map_err(ats_certification_storage)?;
            let tx = conn
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(ats_certification_storage)?;
            require_current_sqlite_ats_trust_policy(
                &tx,
                &verified.trust_policy_sha256,
                recorded_at_ms,
            )?;
            let existing: Option<(String, String, String, String)> = tx
                .query_row(
                    "SELECT manifest_sha256, canonical_manifest_base64url,
                            authorization_sha256, trust_policy_sha256
                       FROM jobs_ats_certification_manifests
                      WHERE certification_id = ?1 OR manifest_sha256 = ?2
                         OR (scope_sha256 = ?3 AND manifest_generation = ?4)",
                    params![
                        verified.authority.certification_id,
                        verified.authority_sha256,
                        verified.authority.scope_sha256,
                        verified.authority.manifest_generation,
                    ],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
                )
                .optional()
                .map_err(ats_certification_storage)?;
            if let Some(existing) = existing {
                require_exact_ats_authority_replay(
                    existing,
                    &verified,
                    &envelope.canonical_base64url,
                )?;
                require_exact_sqlite_ats_manifest_children(&tx, &verified.authority)?;
                tx.commit().map_err(ats_certification_storage)?;
                return Ok(ats_certification_import_result(
                    "manifest",
                    &verified.authority.certification_id,
                    &verified,
                    true,
                ));
            }
            require_sqlite_ats_manifest_evidence(
                &tx,
                &verified.authority,
                &verified.trust_policy_sha256,
            )?;
            require_sqlite_ats_manifest_predecessor(
                &tx,
                &verified.authority,
                &verified.trust_policy_sha256,
            )?;
            require_sqlite_ats_manifest_layouts(
                &tx,
                &verified.authority,
                &verified.trust_policy_sha256,
            )?;
            insert_sqlite_ats_manifest(&tx, &verified, envelope, recorded_by, recorded_at_ms)?;
            tx.commit().map_err(ats_certification_storage)?;
            Ok(ats_certification_import_result(
                "manifest",
                &verified.authority.certification_id,
                &verified,
                false,
            ))
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg().map_err(ats_certification_storage)?;
            let mut tx = conn.transaction().map_err(ats_certification_storage)?;
            lock_postgres_ats_certification(&mut tx)?;
            require_current_postgres_ats_trust_policy(
                &mut tx,
                &verified.trust_policy_sha256,
                recorded_at_ms,
            )?;
            let existing = tx
                .query_opt(
                    "SELECT manifest_sha256, canonical_manifest_base64url,
                            authorization_sha256, trust_policy_sha256
                       FROM jobs_ats_certification_manifests
                      WHERE certification_id = $1 OR manifest_sha256 = $2
                         OR (scope_sha256 = $3 AND manifest_generation = $4)",
                    &[
                        &verified.authority.certification_id,
                        &verified.authority_sha256,
                        &verified.authority.scope_sha256,
                        &verified.authority.manifest_generation,
                    ],
                )
                .map_err(ats_certification_storage)?
                .map(|row| (row.get(0), row.get(1), row.get(2), row.get(3)));
            if let Some(existing) = existing {
                require_exact_ats_authority_replay(
                    existing,
                    &verified,
                    &envelope.canonical_base64url,
                )?;
                require_exact_postgres_ats_manifest_children(&mut tx, &verified.authority)?;
                tx.commit().map_err(ats_certification_storage)?;
                return Ok(ats_certification_import_result(
                    "manifest",
                    &verified.authority.certification_id,
                    &verified,
                    true,
                ));
            }
            require_postgres_ats_manifest_evidence(
                &mut tx,
                &verified.authority,
                &verified.trust_policy_sha256,
            )?;
            require_postgres_ats_manifest_predecessor(
                &mut tx,
                &verified.authority,
                &verified.trust_policy_sha256,
            )?;
            require_postgres_ats_manifest_layouts(
                &mut tx,
                &verified.authority,
                &verified.trust_policy_sha256,
            )?;
            insert_postgres_ats_manifest(
                &mut tx,
                &verified,
                envelope,
                recorded_by,
                recorded_at_ms,
            )?;
            tx.commit().map_err(ats_certification_storage)?;
            Ok(ats_certification_import_result(
                "manifest",
                &verified.authority.certification_id,
                &verified,
                false,
            ))
        }
    })
}

fn require_ats_authorization_clock_skew(
    envelope: &AtsCertificationAuthorityEnvelope,
    issued_at_ms: i64,
    maximum_clock_skew_ms: i64,
) -> Result<(), AtsCertificationAuthorityError> {
    let bytes = ats_certification_decode_base64url_bounded(
        &envelope.authorization_base64url,
        ATS_CERTIFICATION_MAX_CANONICAL_BYTES,
    )?;
    let authorization: AtsCertificationAuthorization =
        ats_certification_parse_canonical_json(&bytes)?;
    if authorization
        .signed_at_ms
        .checked_sub(issued_at_ms)
        .is_none_or(|skew| skew < 0 || skew > maximum_clock_skew_ms)
    {
        return Err(AtsCertificationAuthorityError::InvalidTrustPolicy);
    }
    Ok(())
}

fn verify_ats_manifest_aggregate(
    aggregate: &AtsCertificationManifestAggregateEnvelope,
    trust_policy_sha256: &str,
    trust_policy: &AtsCertificationTrustPolicyAuthority,
    recorded_at_ms: i64,
) -> Result<
    (
        VerifiedAtsCertificationEnvelope<AtsCertificationManifestAuthority>,
        Vec<VerifiedAtsCertificationEnvelope<AtsCertificationEvidenceAuthority>>,
    ),
    AtsCertificationAuthorityError,
> {
    let requirements = &trust_policy.certification_requirements;
    if aggregate.evidence.is_empty()
        || aggregate.evidence.len() as i64 > requirements.maximum_evidence_object_count
    {
        return Err(AtsCertificationAuthorityError::InvalidAuthority);
    }
    let mut evidence = Vec::with_capacity(aggregate.evidence.len());
    for envelope in &aggregate.evidence {
        let mut verified =
            verify_ats_evidence_envelope(envelope, &trust_policy.delegated_trust, recorded_at_ms)?;
        if verified.authority.policy_sha256 != trust_policy_sha256
            || !requirements
                .allowed_providers
                .contains(&verified.authority.provider)
        {
            return Err(AtsCertificationAuthorityError::InvalidTrustPolicy);
        }
        require_ats_authorization_clock_skew(
            envelope,
            verified.authority.issued_at_ms,
            requirements.maximum_clock_skew_ms,
        )?;
        verified.trust_policy_sha256 = trust_policy_sha256.to_string();
        evidence.push(verified);
    }
    let mut manifest = verify_ats_manifest_envelope(
        &aggregate.manifest,
        &trust_policy.delegated_trust,
        recorded_at_ms,
    )?;
    if manifest.authority.policy_sha256 != trust_policy_sha256 {
        return Err(AtsCertificationAuthorityError::InvalidTrustPolicy);
    }
    require_ats_manifest_policy(&manifest.authority, trust_policy, recorded_at_ms)?;
    require_ats_authorization_clock_skew(
        &aggregate.manifest,
        manifest.authority.issued_at_ms,
        requirements.maximum_clock_skew_ms,
    )?;
    let canonical_manifest_size = ats_certification_decode_base64url_bounded(
        &aggregate.manifest.canonical_base64url,
        ATS_CERTIFICATION_MAX_CANONICAL_BYTES,
    )?
    .len() as i64;
    if canonical_manifest_size > requirements.maximum_manifest_size_bytes
        || manifest.authority.evidence_sha256s
            != evidence
                .iter()
                .map(|item| item.authority_sha256.clone())
                .collect::<Vec<_>>()
    {
        return Err(AtsCertificationAuthorityError::InvalidTrustPolicy);
    }
    manifest.trust_policy_sha256 = trust_policy_sha256.to_string();
    Ok((manifest, evidence))
}

pub fn import_ats_certification_manifest_aggregate(
    pool: &DbPool,
    aggregate: &AtsCertificationManifestAggregateEnvelope,
    recorded_by: &str,
) -> Result<AtsCertificationManifestAggregateImportResult, AtsCertificationAuthorityError> {
    import_ats_certification_manifest_aggregate_at(pool, aggregate, recorded_by, now_ms())
}

fn import_ats_certification_manifest_aggregate_at(
    pool: &DbPool,
    aggregate: &AtsCertificationManifestAggregateEnvelope,
    recorded_by: &str,
    recorded_at_ms: i64,
) -> Result<AtsCertificationManifestAggregateImportResult, AtsCertificationAuthorityError> {
    validate_ats_recorded_by(recorded_by)?;
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get().map_err(ats_certification_storage)?;
            let tx = conn
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(ats_certification_storage)?;
            let result = import_ats_certification_manifest_aggregate_sqlite_tx(
                &tx,
                aggregate,
                recorded_by,
                recorded_at_ms,
            )?;
            tx.commit().map_err(ats_certification_storage)?;
            Ok(result)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg().map_err(ats_certification_storage)?;
            let mut tx = conn
                .build_transaction()
                .isolation_level(postgres::IsolationLevel::Serializable)
                .start()
                .map_err(ats_certification_storage)?;
            lock_postgres_ats_certification(&mut tx)?;
            let result = import_ats_certification_manifest_aggregate_postgres_tx(
                &mut tx,
                aggregate,
                recorded_by,
                recorded_at_ms,
            )?;
            tx.commit().map_err(ats_certification_storage)?;
            Ok(result)
        }
    })
}

pub fn import_ats_certification_manifest_aggregate_sqlite_tx(
    tx: &rusqlite::Transaction<'_>,
    aggregate: &AtsCertificationManifestAggregateEnvelope,
    recorded_by: &str,
    recorded_at_ms: i64,
) -> Result<AtsCertificationManifestAggregateImportResult, AtsCertificationAuthorityError> {
    validate_ats_recorded_by(recorded_by)?;
    let (policy_sha256, policy) =
        load_current_sqlite_ats_certification_trust_policy_tx(tx, recorded_at_ms)?;
    let (manifest, evidence) =
        verify_ats_manifest_aggregate(aggregate, &policy_sha256, &policy, recorded_at_ms)?;
    require_current_sqlite_ats_trust_policy(tx, &policy_sha256, recorded_at_ms)?;
    let mut evidence_results = Vec::with_capacity(evidence.len());
    for (verified, envelope) in evidence.iter().zip(&aggregate.evidence) {
        let existing = tx
            .query_row(
                "SELECT evidence_sha256, canonical_evidence_base64url,
                        authorization_sha256, trust_policy_sha256
                   FROM jobs_ats_certification_evidence
                  WHERE evidence_id = ?1 OR evidence_sha256 = ?2",
                params![verified.authority.evidence_id, verified.authority_sha256],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .optional()
            .map_err(ats_certification_storage)?;
        let replayed = if let Some(existing) = existing {
            require_exact_ats_authority_replay(existing, verified, &envelope.canonical_base64url)?;
            true
        } else {
            insert_sqlite_ats_evidence(tx, verified, envelope, recorded_by, recorded_at_ms)?;
            false
        };
        evidence_results.push(ats_certification_import_result(
            "evidence",
            &verified.authority.evidence_id,
            verified,
            replayed,
        ));
    }
    let existing = tx
        .query_row(
            "SELECT manifest_sha256, canonical_manifest_base64url,
                    authorization_sha256, trust_policy_sha256
               FROM jobs_ats_certification_manifests
              WHERE certification_id = ?1 OR manifest_sha256 = ?2
                 OR (scope_sha256 = ?3 AND manifest_generation = ?4)",
            params![
                manifest.authority.certification_id,
                manifest.authority_sha256,
                manifest.authority.scope_sha256,
                manifest.authority.manifest_generation,
            ],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()
        .map_err(ats_certification_storage)?;
    let manifest_replayed = if let Some(existing) = existing {
        require_exact_ats_authority_replay(
            existing,
            &manifest,
            &aggregate.manifest.canonical_base64url,
        )?;
        require_exact_sqlite_ats_manifest_children(tx, &manifest.authority)?;
        true
    } else {
        require_sqlite_ats_manifest_evidence(tx, &manifest.authority, &policy_sha256)?;
        require_sqlite_ats_manifest_predecessor(tx, &manifest.authority, &policy_sha256)?;
        require_sqlite_ats_manifest_layouts(tx, &manifest.authority, &policy_sha256)?;
        insert_sqlite_ats_manifest(
            tx,
            &manifest,
            &aggregate.manifest,
            recorded_by,
            recorded_at_ms,
        )?;
        false
    };
    Ok(AtsCertificationManifestAggregateImportResult {
        manifest: ats_certification_import_result(
            "manifest",
            &manifest.authority.certification_id,
            &manifest,
            manifest_replayed,
        ),
        evidence: evidence_results,
    })
}

pub fn import_ats_certification_manifest_aggregate_postgres_tx(
    tx: &mut postgres::Transaction<'_>,
    aggregate: &AtsCertificationManifestAggregateEnvelope,
    recorded_by: &str,
    recorded_at_ms: i64,
) -> Result<AtsCertificationManifestAggregateImportResult, AtsCertificationAuthorityError> {
    validate_ats_recorded_by(recorded_by)?;
    lock_postgres_ats_certification(tx)?;
    let (policy_sha256, policy) =
        load_current_postgres_ats_certification_trust_policy_tx(tx, recorded_at_ms)?;
    let (manifest, evidence) =
        verify_ats_manifest_aggregate(aggregate, &policy_sha256, &policy, recorded_at_ms)?;
    require_current_postgres_ats_trust_policy(tx, &policy_sha256, recorded_at_ms)?;
    let mut evidence_results = Vec::with_capacity(evidence.len());
    for (verified, envelope) in evidence.iter().zip(&aggregate.evidence) {
        let existing = tx
            .query_opt(
                "SELECT evidence_sha256, canonical_evidence_base64url,
                        authorization_sha256, trust_policy_sha256
                   FROM jobs_ats_certification_evidence
                  WHERE evidence_id = $1 OR evidence_sha256 = $2",
                &[&verified.authority.evidence_id, &verified.authority_sha256],
            )
            .map_err(ats_certification_storage)?
            .map(|row| (row.get(0), row.get(1), row.get(2), row.get(3)));
        let replayed = if let Some(existing) = existing {
            require_exact_ats_authority_replay(existing, verified, &envelope.canonical_base64url)?;
            true
        } else {
            insert_postgres_ats_evidence(tx, verified, envelope, recorded_by, recorded_at_ms)?;
            false
        };
        evidence_results.push(ats_certification_import_result(
            "evidence",
            &verified.authority.evidence_id,
            verified,
            replayed,
        ));
    }
    let existing = tx
        .query_opt(
            "SELECT manifest_sha256, canonical_manifest_base64url,
                    authorization_sha256, trust_policy_sha256
               FROM jobs_ats_certification_manifests
              WHERE certification_id = $1 OR manifest_sha256 = $2
                 OR (scope_sha256 = $3 AND manifest_generation = $4)",
            &[
                &manifest.authority.certification_id,
                &manifest.authority_sha256,
                &manifest.authority.scope_sha256,
                &manifest.authority.manifest_generation,
            ],
        )
        .map_err(ats_certification_storage)?
        .map(|row| (row.get(0), row.get(1), row.get(2), row.get(3)));
    let manifest_replayed = if let Some(existing) = existing {
        require_exact_ats_authority_replay(
            existing,
            &manifest,
            &aggregate.manifest.canonical_base64url,
        )?;
        require_exact_postgres_ats_manifest_children(tx, &manifest.authority)?;
        true
    } else {
        require_postgres_ats_manifest_evidence(tx, &manifest.authority, &policy_sha256)?;
        require_postgres_ats_manifest_predecessor(tx, &manifest.authority, &policy_sha256)?;
        require_postgres_ats_manifest_layouts(tx, &manifest.authority, &policy_sha256)?;
        insert_postgres_ats_manifest(
            tx,
            &manifest,
            &aggregate.manifest,
            recorded_by,
            recorded_at_ms,
        )?;
        false
    };
    Ok(AtsCertificationManifestAggregateImportResult {
        manifest: ats_certification_import_result(
            "manifest",
            &manifest.authority.certification_id,
            &manifest,
            manifest_replayed,
        ),
        evidence: evidence_results,
    })
}

fn require_sqlite_ats_manifest_evidence(
    tx: &rusqlite::Transaction<'_>,
    manifest: &AtsCertificationManifestAuthority,
    trust_policy_sha256: &str,
) -> Result<(), AtsCertificationAuthorityError> {
    for evidence_sha256 in &manifest.evidence_sha256s {
        let row: Option<(String, String, String, String, i64, String)> = tx
            .query_row(
                "SELECT provider, target_key, variant_key, surface_sha256, expires_at_ms,
                        trust_policy_sha256
                   FROM jobs_ats_certification_evidence WHERE evidence_sha256 = ?1",
                params![evidence_sha256],
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
            .optional()
            .map_err(ats_certification_storage)?;
        require_ats_evidence_manifest_binding(row, manifest, trust_policy_sha256)?;
    }
    Ok(())
}

fn require_ats_manifest_predecessor(
    manifest: &AtsCertificationManifestAuthority,
    predecessor: Option<(i64, String, String, String, String)>,
) -> Result<(), AtsCertificationAuthorityError> {
    match predecessor {
        None if manifest.manifest_generation == 1
            && manifest.predecessor_manifest_sha256.is_none() =>
        {
            Ok(())
        }
        Some((generation, provider, target_key, variant_key, surface_sha256))
            if manifest.manifest_generation == generation + 1
                && provider == manifest.provider
                && target_key == manifest.target_key
                && variant_key == manifest.variant_key
                && surface_sha256 == manifest.surface_sha256 =>
        {
            Ok(())
        }
        _ => Err(AtsCertificationAuthorityError::SequenceRegression),
    }
}

fn require_sqlite_ats_manifest_predecessor(
    tx: &rusqlite::Transaction<'_>,
    manifest: &AtsCertificationManifestAuthority,
    trust_policy_sha256: &str,
) -> Result<(), AtsCertificationAuthorityError> {
    let predecessor = match manifest.predecessor_manifest_sha256.as_deref() {
        Some(sha256) => tx
            .query_row(
                "SELECT manifest_generation, provider, target_key, variant_key, surface_sha256
                   FROM jobs_ats_certification_manifests
                  WHERE manifest_sha256 = ?1 AND trust_policy_sha256 = ?2",
                params![sha256, trust_policy_sha256],
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
            .optional()
            .map_err(ats_certification_storage)?,
        None => None,
    };
    require_ats_manifest_predecessor(manifest, predecessor)
}

fn require_postgres_ats_manifest_predecessor(
    tx: &mut postgres::Transaction<'_>,
    manifest: &AtsCertificationManifestAuthority,
    trust_policy_sha256: &str,
) -> Result<(), AtsCertificationAuthorityError> {
    let predecessor = match manifest.predecessor_manifest_sha256.as_deref() {
        Some(sha256) => tx
            .query_opt(
                "SELECT manifest_generation, provider, target_key, variant_key, surface_sha256
                   FROM jobs_ats_certification_manifests
                  WHERE manifest_sha256 = $1 AND trust_policy_sha256 = $2",
                &[&sha256, &trust_policy_sha256],
            )
            .map_err(ats_certification_storage)?
            .map(|row| (row.get(0), row.get(1), row.get(2), row.get(3), row.get(4))),
        None => None,
    };
    require_ats_manifest_predecessor(manifest, predecessor)
}

fn require_postgres_ats_manifest_evidence(
    tx: &mut postgres::Transaction<'_>,
    manifest: &AtsCertificationManifestAuthority,
    trust_policy_sha256: &str,
) -> Result<(), AtsCertificationAuthorityError> {
    for evidence_sha256 in &manifest.evidence_sha256s {
        let row = tx
            .query_opt(
                "SELECT provider, target_key, variant_key, surface_sha256, expires_at_ms,
                        trust_policy_sha256
                   FROM jobs_ats_certification_evidence WHERE evidence_sha256 = $1",
                &[evidence_sha256],
            )
            .map_err(ats_certification_storage)?
            .map(|row| {
                (
                    row.get(0),
                    row.get(1),
                    row.get(2),
                    row.get(3),
                    row.get(4),
                    row.get(5),
                )
            });
        require_ats_evidence_manifest_binding(row, manifest, trust_policy_sha256)?;
    }
    Ok(())
}

fn require_ats_evidence_manifest_binding(
    row: Option<(String, String, String, String, i64, String)>,
    manifest: &AtsCertificationManifestAuthority,
    trust_policy_sha256: &str,
) -> Result<(), AtsCertificationAuthorityError> {
    let row = row.ok_or(AtsCertificationAuthorityError::NotFound)?;
    if row.0 != manifest.provider
        || row.1 != manifest.target_key
        || row.2 != manifest.variant_key
        || row.3 != manifest.surface_sha256
        || row.4 < manifest.expires_at_ms
        || row.5 != trust_policy_sha256
    {
        return Err(AtsCertificationAuthorityError::ScopeMismatch);
    }
    Ok(())
}

type StoredAtsLayoutBinding = (String, String, String, String, String, String, String, i64);

fn require_ats_layout_manifest_binding(
    row: Option<StoredAtsLayoutBinding>,
    manifest: &AtsCertificationManifestAuthority,
    trust_policy_sha256: &str,
) -> Result<(), AtsCertificationAuthorityError> {
    let (provider, target, variant, surface, adapter, runner, evidence_class, expires_at_ms) =
        row.ok_or(AtsCertificationAuthorityError::NotFound)?;
    let runner_matches = manifest
        .runtime_targets
        .iter()
        .any(|runtime| runtime.runtime_sha256 == runner);
    if provider != manifest.provider
        || target
            != ats_certification_target_fingerprint_sha256(
                &manifest.provider,
                &manifest.target_key,
            )?
        || variant != manifest.variant_key
        || surface != manifest.surface_sha256
        || adapter != manifest.adapter_version
        || !runner_matches
        || expires_at_ms < manifest.expires_at_ms
        || (manifest.maximum_capability != "observe_only" && evidence_class == "synthetic")
    {
        return Err(AtsCertificationAuthorityError::ScopeMismatch);
    }
    let _ = trust_policy_sha256;
    Ok(())
}

fn require_sqlite_ats_manifest_layouts(
    tx: &rusqlite::Transaction<'_>,
    manifest: &AtsCertificationManifestAuthority,
    trust_policy_sha256: &str,
) -> Result<(), AtsCertificationAuthorityError> {
    for observation_sha256 in &manifest.certification_profile.layout_observation_sha256s {
        let row = tx
            .query_row(
                "SELECT provider, target_fingerprint_sha256, page_variant, surface_sha256,
                        adapter_version, runner_target_sha256, evidence_class, expires_at_ms
                   FROM jobs_ats_certification_layout_observations
                  WHERE observation_sha256 = ?1 AND trust_policy_sha256 = ?2",
                params![observation_sha256, trust_policy_sha256],
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
            .optional()
            .map_err(ats_certification_storage)?;
        require_ats_layout_manifest_binding(row, manifest, trust_policy_sha256)?;
    }
    Ok(())
}

fn require_postgres_ats_manifest_layouts(
    tx: &mut postgres::Transaction<'_>,
    manifest: &AtsCertificationManifestAuthority,
    trust_policy_sha256: &str,
) -> Result<(), AtsCertificationAuthorityError> {
    for observation_sha256 in &manifest.certification_profile.layout_observation_sha256s {
        let row = tx
            .query_opt(
                "SELECT provider, target_fingerprint_sha256, page_variant, surface_sha256,
                        adapter_version, runner_target_sha256, evidence_class, expires_at_ms
                   FROM jobs_ats_certification_layout_observations
                  WHERE observation_sha256 = $1 AND trust_policy_sha256 = $2",
                &[&observation_sha256, &trust_policy_sha256],
            )
            .map_err(ats_certification_storage)?
            .map(|row| {
                (
                    row.get(0),
                    row.get(1),
                    row.get(2),
                    row.get(3),
                    row.get(4),
                    row.get(5),
                    row.get(6),
                    row.get(7),
                )
            });
        require_ats_layout_manifest_binding(row, manifest, trust_policy_sha256)?;
    }
    Ok(())
}

fn require_exact_sqlite_ats_manifest_children(
    tx: &rusqlite::Transaction<'_>,
    manifest: &AtsCertificationManifestAuthority,
) -> Result<(), AtsCertificationAuthorityError> {
    let mut evidence_stmt = tx
        .prepare(
            "SELECT evidence_sha256 FROM jobs_ats_certification_manifest_evidence
              WHERE manifest_sha256 = ?1 ORDER BY ordinal",
        )
        .map_err(ats_certification_storage)?;
    let evidence = evidence_stmt
        .query_map(params![ats_manifest_sha256(manifest)?], |row| row.get(0))
        .map_err(ats_certification_storage)?
        .collect::<std::result::Result<Vec<String>, _>>()
        .map_err(ats_certification_storage)?;
    let runtimes = sqlite_ats_runtime_targets(tx, &ats_manifest_sha256(manifest)?)?;
    let manifest_sha256 = ats_manifest_sha256(manifest)?;
    let layouts = tx
        .prepare(
            "SELECT observation_sha256 FROM jobs_ats_certification_manifest_layouts
              WHERE manifest_sha256 = ?1 ORDER BY ordinal",
        )
        .and_then(|mut stmt| {
            stmt.query_map(params![manifest_sha256], |row| row.get(0))?
                .collect::<std::result::Result<Vec<String>, _>>()
        })
        .map_err(ats_certification_storage)?;
    if evidence != manifest.evidence_sha256s
        || layouts != manifest.certification_profile.layout_observation_sha256s
        || runtimes != manifest.runtime_targets
    {
        return Err(AtsCertificationAuthorityError::IdentityConflict);
    }
    Ok(())
}

fn require_exact_postgres_ats_manifest_children(
    tx: &mut postgres::Transaction<'_>,
    manifest: &AtsCertificationManifestAuthority,
) -> Result<(), AtsCertificationAuthorityError> {
    let manifest_sha256 = ats_manifest_sha256(manifest)?;
    let evidence = tx
        .query(
            "SELECT evidence_sha256 FROM jobs_ats_certification_manifest_evidence
              WHERE manifest_sha256 = $1 ORDER BY ordinal",
            &[&manifest_sha256],
        )
        .map_err(ats_certification_storage)?
        .into_iter()
        .map(|row| row.get(0))
        .collect::<Vec<String>>();
    let runtimes = postgres_ats_runtime_targets(tx, &manifest_sha256)?;
    let layouts = tx
        .query(
            "SELECT observation_sha256 FROM jobs_ats_certification_manifest_layouts
              WHERE manifest_sha256 = $1 ORDER BY ordinal",
            &[&manifest_sha256],
        )
        .map_err(ats_certification_storage)?
        .into_iter()
        .map(|row| row.get(0))
        .collect::<Vec<String>>();
    if evidence != manifest.evidence_sha256s
        || layouts != manifest.certification_profile.layout_observation_sha256s
        || runtimes != manifest.runtime_targets
    {
        return Err(AtsCertificationAuthorityError::IdentityConflict);
    }
    Ok(())
}

fn ats_manifest_sha256(
    manifest: &AtsCertificationManifestAuthority,
) -> Result<String, AtsCertificationAuthorityError> {
    Ok(ats_certification_sha256(
        &ats_certification_canonical_json(manifest)
            .map_err(|_| AtsCertificationAuthorityError::InvalidAuthority)?,
    ))
}

fn insert_sqlite_ats_manifest(
    tx: &rusqlite::Transaction<'_>,
    verified: &VerifiedAtsCertificationEnvelope<AtsCertificationManifestAuthority>,
    envelope: &AtsCertificationAuthorityEnvelope,
    recorded_by: &str,
    recorded_at_ms: i64,
) -> Result<(), AtsCertificationAuthorityError> {
    let manifest = &verified.authority;
    let allowed_provider_hosts_json = serde_json::to_string(&manifest.allowed_provider_hosts)
        .map_err(|_| AtsCertificationAuthorityError::InvalidAuthority)?;
    tx.execute(
        "INSERT INTO jobs_ats_certification_manifests (
           manifest_sha256, certification_id, manifest_generation,
           predecessor_manifest_sha256, provider,
           target_key, variant_key, surface_sha256, scope_sha256, adapter_version,
           adapter_bundle_sha256, source_commit, layout_contract_version,
           layout_contract_sha256, maximum_capability, layout_set_sha256,
           suite_id, suite_version, suite_manifest_sha256, layout_observation_count,
           check_result_count, hard_filter_violations, unsupported_factual_claims,
           duplicate_submit_activations, false_submitted_states, incomplete_receipts,
           pii_bearing_observations, evidence_count,
           runtime_target_count, canonical_manifest_base64url, authorization_sha256,
           trust_policy_sha256, canonical_authorization_base64url,
           allowed_provider_hosts_json, final_submit_control_id, tested_at_ms,
           issued_at_ms, not_before_ms, expires_at_ms, recorded_by, recorded_at_ms
         ) VALUES (
           ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14,
           ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26,
           ?27, ?28, ?29, ?30, ?31, ?32, ?33, ?34, ?35, ?36, ?37, ?38,
           ?39, ?40, ?41
         )",
        params![
            verified.authority_sha256,
            manifest.certification_id,
            manifest.manifest_generation,
            manifest.predecessor_manifest_sha256,
            manifest.provider,
            manifest.target_key,
            manifest.variant_key,
            manifest.surface_sha256,
            manifest.scope_sha256,
            manifest.adapter_version,
            manifest.adapter_bundle_sha256,
            manifest.source_commit,
            manifest.layout_contract_version,
            manifest.layout_contract_sha256,
            manifest.maximum_capability,
            manifest.certification_profile.layout_set_sha256,
            manifest.certification_profile.suite_id,
            manifest.certification_profile.suite_version,
            manifest.certification_profile.suite_manifest_sha256,
            manifest
                .certification_profile
                .layout_observation_sha256s
                .len() as i64,
            manifest.certification_profile.check_results.len() as i64,
            manifest
                .certification_profile
                .zero_tolerance
                .hard_filter_violations,
            manifest
                .certification_profile
                .zero_tolerance
                .unsupported_factual_claims,
            manifest
                .certification_profile
                .zero_tolerance
                .duplicate_submit_activations,
            manifest
                .certification_profile
                .zero_tolerance
                .false_submitted_states,
            manifest
                .certification_profile
                .zero_tolerance
                .incomplete_or_mismatched_receipts,
            manifest
                .certification_profile
                .zero_tolerance
                .pii_bearing_observations,
            manifest.evidence_sha256s.len() as i64,
            manifest.runtime_targets.len() as i64,
            envelope.canonical_base64url,
            verified.authorization_sha256,
            verified.trust_policy_sha256,
            envelope.authorization_base64url,
            allowed_provider_hosts_json,
            manifest.final_submit_control_id,
            manifest.tested_at_ms,
            manifest.issued_at_ms,
            manifest.not_before_ms,
            manifest.expires_at_ms,
            recorded_by,
            recorded_at_ms,
        ],
    )
    .map_err(ats_certification_storage)?;
    for (ordinal, evidence_sha256) in manifest.evidence_sha256s.iter().enumerate() {
        tx.execute(
            "INSERT INTO jobs_ats_certification_manifest_evidence (
               manifest_sha256, evidence_sha256, ordinal
             ) VALUES (?1, ?2, ?3)",
            params![verified.authority_sha256, evidence_sha256, ordinal as i64],
        )
        .map_err(ats_certification_storage)?;
    }
    for (ordinal, observation_sha256) in manifest
        .certification_profile
        .layout_observation_sha256s
        .iter()
        .enumerate()
    {
        tx.execute(
            "INSERT INTO jobs_ats_certification_manifest_layouts (
               manifest_sha256, observation_sha256, ordinal
             ) VALUES (?1, ?2, ?3)",
            params![
                verified.authority_sha256,
                observation_sha256,
                ordinal as i64,
            ],
        )
        .map_err(ats_certification_storage)?;
    }
    for (ordinal, result) in manifest
        .certification_profile
        .check_results
        .iter()
        .enumerate()
    {
        tx.execute(
            "INSERT INTO jobs_ats_certification_manifest_check_results (
               manifest_sha256, runner_target_sha256, check_id, evidence_class,
               passed_count, failed_count, skipped_count, ordinal
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                verified.authority_sha256,
                result.runner_target_sha256,
                result.check_id,
                result.evidence_class,
                result.passed_count,
                result.failed_count,
                result.skipped_count,
                ordinal as i64,
            ],
        )
        .map_err(ats_certification_storage)?;
    }
    for (ordinal, runtime) in manifest.runtime_targets.iter().enumerate() {
        insert_sqlite_ats_runtime_target(tx, &verified.authority_sha256, runtime, ordinal as i64)?;
    }
    Ok(())
}

fn insert_postgres_ats_manifest(
    tx: &mut postgres::Transaction<'_>,
    verified: &VerifiedAtsCertificationEnvelope<AtsCertificationManifestAuthority>,
    envelope: &AtsCertificationAuthorityEnvelope,
    recorded_by: &str,
    recorded_at_ms: i64,
) -> Result<(), AtsCertificationAuthorityError> {
    let manifest = &verified.authority;
    let allowed_provider_hosts_json = serde_json::to_string(&manifest.allowed_provider_hosts)
        .map_err(|_| AtsCertificationAuthorityError::InvalidAuthority)?;
    tx.execute(
        "INSERT INTO jobs_ats_certification_manifests (
           manifest_sha256, certification_id, manifest_generation,
           predecessor_manifest_sha256, provider,
           target_key, variant_key, surface_sha256, scope_sha256, adapter_version,
           adapter_bundle_sha256, source_commit, layout_contract_version,
           layout_contract_sha256, maximum_capability, layout_set_sha256,
           suite_id, suite_version, suite_manifest_sha256, layout_observation_count,
           check_result_count, hard_filter_violations, unsupported_factual_claims,
           duplicate_submit_activations, false_submitted_states, incomplete_receipts,
           pii_bearing_observations, evidence_count,
           runtime_target_count, canonical_manifest_base64url, authorization_sha256,
           trust_policy_sha256, canonical_authorization_base64url,
           allowed_provider_hosts_json, final_submit_control_id, tested_at_ms,
           issued_at_ms, not_before_ms, expires_at_ms, recorded_by, recorded_at_ms
         ) VALUES (
           $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14,
           $15, $16, $17, $18, $19, $20, $21, $22, $23, $24, $25, $26,
           $27, $28, $29, $30, $31, $32, $33, $34, $35, $36, $37, $38,
           $39, $40, $41
         )",
        &[
            &verified.authority_sha256,
            &manifest.certification_id,
            &manifest.manifest_generation,
            &manifest.predecessor_manifest_sha256,
            &manifest.provider,
            &manifest.target_key,
            &manifest.variant_key,
            &manifest.surface_sha256,
            &manifest.scope_sha256,
            &manifest.adapter_version,
            &manifest.adapter_bundle_sha256,
            &manifest.source_commit,
            &manifest.layout_contract_version,
            &manifest.layout_contract_sha256,
            &manifest.maximum_capability,
            &manifest.certification_profile.layout_set_sha256,
            &manifest.certification_profile.suite_id,
            &manifest.certification_profile.suite_version,
            &manifest.certification_profile.suite_manifest_sha256,
            &(manifest
                .certification_profile
                .layout_observation_sha256s
                .len() as i64),
            &(manifest.certification_profile.check_results.len() as i64),
            &manifest
                .certification_profile
                .zero_tolerance
                .hard_filter_violations,
            &manifest
                .certification_profile
                .zero_tolerance
                .unsupported_factual_claims,
            &manifest
                .certification_profile
                .zero_tolerance
                .duplicate_submit_activations,
            &manifest
                .certification_profile
                .zero_tolerance
                .false_submitted_states,
            &manifest
                .certification_profile
                .zero_tolerance
                .incomplete_or_mismatched_receipts,
            &manifest
                .certification_profile
                .zero_tolerance
                .pii_bearing_observations,
            &(manifest.evidence_sha256s.len() as i64),
            &(manifest.runtime_targets.len() as i64),
            &envelope.canonical_base64url,
            &verified.authorization_sha256,
            &verified.trust_policy_sha256,
            &envelope.authorization_base64url,
            &allowed_provider_hosts_json,
            &manifest.final_submit_control_id,
            &manifest.tested_at_ms,
            &manifest.issued_at_ms,
            &manifest.not_before_ms,
            &manifest.expires_at_ms,
            &recorded_by,
            &recorded_at_ms,
        ],
    )
    .map_err(ats_certification_storage)?;
    for (ordinal, evidence_sha256) in manifest.evidence_sha256s.iter().enumerate() {
        tx.execute(
            "INSERT INTO jobs_ats_certification_manifest_evidence (
               manifest_sha256, evidence_sha256, ordinal
             ) VALUES ($1, $2, $3)",
            &[
                &verified.authority_sha256,
                evidence_sha256,
                &(ordinal as i64),
            ],
        )
        .map_err(ats_certification_storage)?;
    }
    for (ordinal, observation_sha256) in manifest
        .certification_profile
        .layout_observation_sha256s
        .iter()
        .enumerate()
    {
        tx.execute(
            "INSERT INTO jobs_ats_certification_manifest_layouts (
               manifest_sha256, observation_sha256, ordinal
             ) VALUES ($1, $2, $3)",
            &[
                &verified.authority_sha256,
                observation_sha256,
                &(ordinal as i64),
            ],
        )
        .map_err(ats_certification_storage)?;
    }
    for (ordinal, result) in manifest
        .certification_profile
        .check_results
        .iter()
        .enumerate()
    {
        tx.execute(
            "INSERT INTO jobs_ats_certification_manifest_check_results (
               manifest_sha256, runner_target_sha256, check_id, evidence_class,
               passed_count, failed_count, skipped_count, ordinal
             ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
            &[
                &verified.authority_sha256,
                &result.runner_target_sha256,
                &result.check_id,
                &result.evidence_class,
                &result.passed_count,
                &result.failed_count,
                &result.skipped_count,
                &(ordinal as i64),
            ],
        )
        .map_err(ats_certification_storage)?;
    }
    for (ordinal, runtime) in manifest.runtime_targets.iter().enumerate() {
        insert_postgres_ats_runtime_target(
            tx,
            &verified.authority_sha256,
            runtime,
            ordinal as i64,
        )?;
    }
    Ok(())
}

fn insert_sqlite_ats_runtime_target(
    tx: &rusqlite::Transaction<'_>,
    manifest_sha256: &str,
    runtime: &AtsCertificationRuntimeTarget,
    ordinal: i64,
) -> Result<(), AtsCertificationAuthorityError> {
    tx.execute(
        "INSERT INTO jobs_ats_certification_runtime_targets (
           manifest_sha256, runtime_kind, runtime_id, runtime_sha256,
           platform, architecture, automation_bundle_sha256,
           browser_release_manifest_sha256, browser_artifact_sha256,
           browser_build_descriptor_sha256, runner_build_id, runner_image_sha256,
           playwright_version, chromium_revision, chromium_executable_sha256, ordinal
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12,
                   ?13, ?14, ?15, ?16)",
        params![
            manifest_sha256,
            runtime.runtime_kind,
            runtime.runtime_id,
            runtime.runtime_sha256,
            runtime.platform,
            runtime.architecture,
            runtime.automation_bundle_sha256,
            runtime.browser_release_manifest_sha256,
            runtime.browser_artifact_sha256,
            runtime.browser_build_descriptor_sha256,
            runtime.runner_build_id,
            runtime.runner_image_sha256,
            runtime.playwright_version,
            runtime.chromium_revision,
            runtime.chromium_executable_sha256,
            ordinal,
        ],
    )
    .map_err(ats_certification_storage)?;
    Ok(())
}

fn insert_postgres_ats_runtime_target(
    tx: &mut postgres::Transaction<'_>,
    manifest_sha256: &str,
    runtime: &AtsCertificationRuntimeTarget,
    ordinal: i64,
) -> Result<(), AtsCertificationAuthorityError> {
    tx.execute(
        "INSERT INTO jobs_ats_certification_runtime_targets (
           manifest_sha256, runtime_kind, runtime_id, runtime_sha256,
           platform, architecture, automation_bundle_sha256,
           browser_release_manifest_sha256, browser_artifact_sha256,
           browser_build_descriptor_sha256, runner_build_id, runner_image_sha256,
           playwright_version, chromium_revision, chromium_executable_sha256, ordinal
         ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12,
                   $13, $14, $15, $16)",
        &[
            &manifest_sha256,
            &runtime.runtime_kind,
            &runtime.runtime_id,
            &runtime.runtime_sha256,
            &runtime.platform,
            &runtime.architecture,
            &runtime.automation_bundle_sha256,
            &runtime.browser_release_manifest_sha256,
            &runtime.browser_artifact_sha256,
            &runtime.browser_build_descriptor_sha256,
            &runtime.runner_build_id,
            &runtime.runner_image_sha256,
            &runtime.playwright_version,
            &runtime.chromium_revision,
            &runtime.chromium_executable_sha256,
            &ordinal,
        ],
    )
    .map_err(ats_certification_storage)?;
    Ok(())
}

fn sqlite_ats_runtime_targets(
    tx: &rusqlite::Transaction<'_>,
    manifest_sha256: &str,
) -> Result<Vec<AtsCertificationRuntimeTarget>, AtsCertificationAuthorityError> {
    let mut stmt = tx
        .prepare(
            "SELECT runtime_kind, runtime_id, runtime_sha256, platform, architecture,
                    automation_bundle_sha256, browser_release_manifest_sha256,
                    browser_artifact_sha256, browser_build_descriptor_sha256,
                    runner_build_id, runner_image_sha256, playwright_version,
                    chromium_revision, chromium_executable_sha256
               FROM jobs_ats_certification_runtime_targets
              WHERE manifest_sha256 = ?1 ORDER BY ordinal",
        )
        .map_err(ats_certification_storage)?;
    let rows = stmt
        .query_map(params![manifest_sha256], |row| {
            Ok(AtsCertificationRuntimeTarget {
                runtime_kind: row.get(0)?,
                runtime_id: row.get(1)?,
                runtime_sha256: row.get(2)?,
                platform: row.get(3)?,
                architecture: row.get(4)?,
                automation_bundle_sha256: row.get(5)?,
                browser_release_manifest_sha256: row.get(6)?,
                browser_artifact_sha256: row.get(7)?,
                browser_build_descriptor_sha256: row.get(8)?,
                runner_build_id: row.get(9)?,
                runner_image_sha256: row.get(10)?,
                playwright_version: row.get(11)?,
                chromium_revision: row.get(12)?,
                chromium_executable_sha256: row.get(13)?,
            })
        })
        .map_err(ats_certification_storage)?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(ats_certification_storage)?;
    Ok(rows)
}

fn postgres_ats_runtime_targets(
    tx: &mut postgres::Transaction<'_>,
    manifest_sha256: &str,
) -> Result<Vec<AtsCertificationRuntimeTarget>, AtsCertificationAuthorityError> {
    Ok(tx
        .query(
            "SELECT runtime_kind, runtime_id, runtime_sha256, platform, architecture,
                    automation_bundle_sha256, browser_release_manifest_sha256,
                    browser_artifact_sha256, browser_build_descriptor_sha256,
                    runner_build_id, runner_image_sha256, playwright_version,
                    chromium_revision, chromium_executable_sha256
               FROM jobs_ats_certification_runtime_targets
              WHERE manifest_sha256 = $1 ORDER BY ordinal",
            &[&manifest_sha256],
        )
        .map_err(ats_certification_storage)?
        .into_iter()
        .map(|row| AtsCertificationRuntimeTarget {
            runtime_kind: row.get(0),
            runtime_id: row.get(1),
            runtime_sha256: row.get(2),
            platform: row.get(3),
            architecture: row.get(4),
            automation_bundle_sha256: row.get(5),
            browser_release_manifest_sha256: row.get(6),
            browser_artifact_sha256: row.get(7),
            browser_build_descriptor_sha256: row.get(8),
            runner_build_id: row.get(9),
            runner_image_sha256: row.get(10),
            playwright_version: row.get(11),
            chromium_revision: row.get(12),
            chromium_executable_sha256: row.get(13),
        })
        .collect())
}

#[derive(Debug, Clone)]
struct StoredAtsActivationHeadAuthority {
    activation_sha256: String,
    activation_id: String,
    activation_generation: i64,
    manifest_sha256: String,
    scope_sha256: String,
    channel: String,
    channel_sequence: i64,
    capability: String,
    account_allowlist_sha256: Option<String>,
    canary_max_submissions: i64,
    canary_account_cap: i64,
    canary_concurrency_cap: i64,
    canary_daily_side_effect_cap: i64,
    not_before_ms: i64,
    expires_at_ms: i64,
    trust_policy_sha256: String,
}

#[derive(Debug, Clone)]
struct StoredAtsHead {
    head_revision: i64,
    transition_sha256: String,
    activation_sha256: String,
    channel_sequence: i64,
    updated_by: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct AtsCertificationHeadTransitionDigest<'a> {
    version: i64,
    audience: &'static str,
    scope_sha256: &'a str,
    channel: &'a str,
    head_revision: i64,
    previous_head_revision: i64,
    previous_transition_sha256: Option<&'a str>,
    previous_activation_sha256: Option<&'a str>,
    next_activation_sha256: &'a str,
    next_channel_sequence: i64,
    recorded_by: &'a str,
    recorded_at_ms: i64,
}

pub fn import_ats_certification_activation(
    pool: &DbPool,
    envelope: &AtsCertificationAuthorityEnvelope,
    recorded_by: &str,
) -> Result<AtsCertificationImportResult, AtsCertificationAuthorityError> {
    import_ats_certification_activation_at(pool, envelope, recorded_by, now_ms())
}

fn import_ats_certification_activation_at(
    pool: &DbPool,
    envelope: &AtsCertificationAuthorityEnvelope,
    recorded_by: &str,
    recorded_at_ms: i64,
) -> Result<AtsCertificationImportResult, AtsCertificationAuthorityError> {
    validate_ats_recorded_by(recorded_by)?;
    let (trust_policy_sha256, trust_policy) =
        load_current_ats_certification_trust_policy(pool, recorded_at_ms)?;
    let mut verified =
        verify_ats_activation_envelope(envelope, &trust_policy.delegated_trust, recorded_at_ms)?;
    if verified.authority.policy_sha256 != trust_policy_sha256 {
        return Err(AtsCertificationAuthorityError::InvalidTrustPolicy);
    }
    require_ats_authorization_clock_skew(
        envelope,
        verified.authority.issued_at_ms,
        trust_policy
            .certification_requirements
            .maximum_clock_skew_ms,
    )?;
    verified.trust_policy_sha256 = trust_policy_sha256;
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get().map_err(ats_certification_storage)?;
            let tx = conn
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(ats_certification_storage)?;
            require_current_sqlite_ats_trust_policy(
                &tx,
                &verified.trust_policy_sha256,
                recorded_at_ms,
            )?;
            let existing: Option<(String, String, String, String)> = tx
                .query_row(
                    "SELECT activation_sha256, canonical_activation_base64url,
                            authorization_sha256, trust_policy_sha256
                       FROM jobs_ats_certification_activations
                      WHERE activation_id = ?1 OR activation_sha256 = ?2
                         OR (scope_sha256 = ?3 AND channel = ?4 AND channel_sequence = ?5)",
                    params![
                        verified.authority.activation_id,
                        verified.authority_sha256,
                        verified.authority.scope_sha256,
                        verified.authority.channel,
                        verified.authority.channel_sequence,
                    ],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
                )
                .optional()
                .map_err(ats_certification_storage)?;
            if let Some(existing) = existing {
                require_exact_ats_authority_replay(
                    existing,
                    &verified,
                    &envelope.canonical_base64url,
                )?;
                tx.commit().map_err(ats_certification_storage)?;
                return Ok(ats_certification_import_result(
                    "activation",
                    &verified.authority.activation_id,
                    &verified,
                    true,
                ));
            }
            require_sqlite_ats_activation_manifest(
                &tx,
                &verified.authority,
                &verified.trust_policy_sha256,
            )?;
            require_sqlite_ats_activation_canary_allowlist(
                &tx,
                &verified.authority,
                recorded_at_ms,
            )?;
            require_sqlite_ats_activation_predecessor(
                &tx,
                &verified.authority,
                &verified.trust_policy_sha256,
            )?;
            insert_sqlite_ats_activation(&tx, &verified, envelope, recorded_by, recorded_at_ms)?;
            tx.commit().map_err(ats_certification_storage)?;
            Ok(ats_certification_import_result(
                "activation",
                &verified.authority.activation_id,
                &verified,
                false,
            ))
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg().map_err(ats_certification_storage)?;
            let mut tx = conn.transaction().map_err(ats_certification_storage)?;
            lock_postgres_ats_certification(&mut tx)?;
            require_current_postgres_ats_trust_policy(
                &mut tx,
                &verified.trust_policy_sha256,
                recorded_at_ms,
            )?;
            let existing = tx
                .query_opt(
                    "SELECT activation_sha256, canonical_activation_base64url,
                            authorization_sha256, trust_policy_sha256
                       FROM jobs_ats_certification_activations
                      WHERE activation_id = $1 OR activation_sha256 = $2
                         OR (scope_sha256 = $3 AND channel = $4 AND channel_sequence = $5)",
                    &[
                        &verified.authority.activation_id,
                        &verified.authority_sha256,
                        &verified.authority.scope_sha256,
                        &verified.authority.channel,
                        &verified.authority.channel_sequence,
                    ],
                )
                .map_err(ats_certification_storage)?
                .map(|row| (row.get(0), row.get(1), row.get(2), row.get(3)));
            if let Some(existing) = existing {
                require_exact_ats_authority_replay(
                    existing,
                    &verified,
                    &envelope.canonical_base64url,
                )?;
                tx.commit().map_err(ats_certification_storage)?;
                return Ok(ats_certification_import_result(
                    "activation",
                    &verified.authority.activation_id,
                    &verified,
                    true,
                ));
            }
            require_postgres_ats_activation_manifest(
                &mut tx,
                &verified.authority,
                &verified.trust_policy_sha256,
            )?;
            require_postgres_ats_activation_canary_allowlist(
                &mut tx,
                &verified.authority,
                recorded_at_ms,
            )?;
            require_postgres_ats_activation_predecessor(
                &mut tx,
                &verified.authority,
                &verified.trust_policy_sha256,
            )?;
            insert_postgres_ats_activation(
                &mut tx,
                &verified,
                envelope,
                recorded_by,
                recorded_at_ms,
            )?;
            tx.commit().map_err(ats_certification_storage)?;
            Ok(ats_certification_import_result(
                "activation",
                &verified.authority.activation_id,
                &verified,
                false,
            ))
        }
    })
}

#[derive(Debug, Clone)]
struct StoredAtsActivationManifestBinding {
    scope_sha256: String,
    maximum_capability: String,
    not_before_ms: i64,
    expires_at_ms: i64,
    evidence_count: i64,
    runtime_target_count: i64,
    provider: String,
    adapter_version: String,
    trust_policy_sha256: String,
}

fn require_ats_activation_manifest_binding(
    activation: &AtsCertificationActivationAuthority,
    row: Option<StoredAtsActivationManifestBinding>,
    authorized_evidence_count: i64,
    production_check_pair_count: i64,
    production_layout_pair_count: i64,
    synthetic_layout_count: i64,
    trust_policy_sha256: &str,
) -> Result<(), AtsCertificationAuthorityError> {
    let manifest = row.ok_or(AtsCertificationAuthorityError::NotFound)?;
    let requested_rank = ats_certification_capability_rank(&activation.capability)
        .ok_or(AtsCertificationAuthorityError::InvalidAuthority)?;
    let maximum_rank = ats_certification_capability_rank(&manifest.maximum_capability)
        .ok_or(AtsCertificationAuthorityError::InvalidAuthority)?;
    if activation.scope_sha256 != manifest.scope_sha256
        || activation.not_before_ms < manifest.not_before_ms
        || activation.expires_at_ms > manifest.expires_at_ms
        || requested_rank > maximum_rank
        || manifest.evidence_count < 1
        || manifest.trust_policy_sha256 != trust_policy_sha256
    {
        return Err(AtsCertificationAuthorityError::ScopeMismatch);
    }
    if activation.channel != "shadow"
        && (authorized_evidence_count < 1 || synthetic_layout_count != 0)
    {
        return Err(AtsCertificationAuthorityError::SyntheticEvidence);
    }
    let required_production_pairs = manifest
        .runtime_target_count
        .checked_mul(2)
        .ok_or(AtsCertificationAuthorityError::InvalidAuthority)?;
    if activation.channel != "shadow"
        && (production_check_pair_count != required_production_pairs
            || production_layout_pair_count != required_production_pairs)
    {
        return Err(AtsCertificationAuthorityError::ScopeMismatch);
    }
    if activation.channel != "shadow"
        && !ats_exact_submit_adapter(&manifest.provider, &manifest.adapter_version)
    {
        return Err(AtsCertificationAuthorityError::InvalidAuthority);
    }
    if activation.capability == "unattended_submit"
        && !ats_exact_submit_adapter(&manifest.provider, &manifest.adapter_version)
    {
        return Err(AtsCertificationAuthorityError::InvalidAuthority);
    }
    Ok(())
}

fn ats_exact_submit_adapter(provider: &str, adapter_version: &str) -> bool {
    matches!(
        (provider, adapter_version),
        ("greenhouse", ATS_GREENHOUSE_EXACT_ADAPTER_VERSION)
            | ("lever", ATS_LEVER_EXACT_ADAPTER_VERSION)
    )
}

fn ats_exact_provider_target_key(provider: &str, target_key: &str) -> bool {
    let pieces = target_key.split(':').collect::<Vec<_>>();
    match provider {
        "greenhouse" => {
            pieces.len() == 3
                && pieces[0] == "greenhouse"
                && ats_certification_token(pieces[1], 1, 160)
                && ats_certification_token(pieces[2], 1, 160)
        }
        "lever" => {
            pieces.len() == 4
                && pieces[0] == "lever"
                && matches!(pieces[1], "jobs.lever.co" | "jobs.eu.lever.co")
                && ats_certification_token(pieces[2], 1, 160)
                && ats_certification_token(pieces[3], 1, 160)
        }
        _ => false,
    }
}

fn require_sqlite_ats_activation_manifest(
    tx: &rusqlite::Transaction<'_>,
    activation: &AtsCertificationActivationAuthority,
    trust_policy_sha256: &str,
) -> Result<(), AtsCertificationAuthorityError> {
    let row = tx
        .query_row(
            "SELECT scope_sha256, maximum_capability, not_before_ms, expires_at_ms,
                    evidence_count, runtime_target_count, provider, adapter_version,
                    trust_policy_sha256
               FROM jobs_ats_certification_manifests WHERE manifest_sha256 = ?1",
            params![activation.manifest_sha256],
            |row| {
                Ok(StoredAtsActivationManifestBinding {
                    scope_sha256: row.get(0)?,
                    maximum_capability: row.get(1)?,
                    not_before_ms: row.get(2)?,
                    expires_at_ms: row.get(3)?,
                    evidence_count: row.get(4)?,
                    runtime_target_count: row.get(5)?,
                    provider: row.get(6)?,
                    adapter_version: row.get(7)?,
                    trust_policy_sha256: row.get(8)?,
                })
            },
        )
        .optional()
        .map_err(ats_certification_storage)?;
    let authorized_evidence_count = tx
        .query_row(
            "SELECT COUNT(*)
               FROM jobs_ats_certification_manifest_evidence binding
               JOIN jobs_ats_certification_evidence evidence
                 ON evidence.evidence_sha256 = binding.evidence_sha256
              WHERE binding.manifest_sha256 = ?1
                AND evidence.source_kind IN ('authorized_canary', 'authorized_sandbox')",
            params![activation.manifest_sha256],
            |row| row.get(0),
        )
        .map_err(ats_certification_storage)?;
    let production_check_pair_count = tx
        .query_row(
            "SELECT COUNT(*)
               FROM (
                 SELECT DISTINCT result.runner_target_sha256, result.evidence_class
                   FROM jobs_ats_certification_manifest_check_results result
                  WHERE result.manifest_sha256 = ?1
                    AND result.evidence_class IN ('authorized_sandbox', 'authorized_live')
               ) required_pairs",
            params![activation.manifest_sha256],
            |row| row.get(0),
        )
        .map_err(ats_certification_storage)?;
    let production_layout_pair_count = tx
        .query_row(
            "SELECT COUNT(*)
               FROM (
                 SELECT DISTINCT observation.runner_target_sha256,
                                 observation.evidence_class
                   FROM jobs_ats_certification_manifest_layouts binding
                   JOIN jobs_ats_certification_layout_observations observation
                     ON observation.observation_sha256 = binding.observation_sha256
                  WHERE binding.manifest_sha256 = ?1
                    AND observation.evidence_class IN (
                      'authorized_sandbox', 'authorized_live'
                    )
               ) required_pairs",
            params![activation.manifest_sha256],
            |row| row.get(0),
        )
        .map_err(ats_certification_storage)?;
    let synthetic_layout_count = tx
        .query_row(
            "SELECT COUNT(*)
               FROM jobs_ats_certification_manifest_layouts binding
               JOIN jobs_ats_certification_layout_observations observation
                 ON observation.observation_sha256 = binding.observation_sha256
              WHERE binding.manifest_sha256 = ?1
                AND observation.evidence_class = 'synthetic'",
            params![activation.manifest_sha256],
            |row| row.get(0),
        )
        .map_err(ats_certification_storage)?;
    require_ats_activation_manifest_binding(
        activation,
        row,
        authorized_evidence_count,
        production_check_pair_count,
        production_layout_pair_count,
        synthetic_layout_count,
        trust_policy_sha256,
    )
}

fn require_postgres_ats_activation_manifest(
    tx: &mut postgres::Transaction<'_>,
    activation: &AtsCertificationActivationAuthority,
    trust_policy_sha256: &str,
) -> Result<(), AtsCertificationAuthorityError> {
    let row = tx
        .query_opt(
            "SELECT scope_sha256, maximum_capability, not_before_ms, expires_at_ms,
                    evidence_count, runtime_target_count, provider, adapter_version,
                    trust_policy_sha256
               FROM jobs_ats_certification_manifests WHERE manifest_sha256 = $1",
            &[&activation.manifest_sha256],
        )
        .map_err(ats_certification_storage)?
        .map(|row| StoredAtsActivationManifestBinding {
            scope_sha256: row.get(0),
            maximum_capability: row.get(1),
            not_before_ms: row.get(2),
            expires_at_ms: row.get(3),
            evidence_count: row.get(4),
            runtime_target_count: row.get(5),
            provider: row.get(6),
            adapter_version: row.get(7),
            trust_policy_sha256: row.get(8),
        });
    let authorized_evidence_count: i64 = tx
        .query_one(
            "SELECT COUNT(*)
               FROM jobs_ats_certification_manifest_evidence binding
               JOIN jobs_ats_certification_evidence evidence
                 ON evidence.evidence_sha256 = binding.evidence_sha256
              WHERE binding.manifest_sha256 = $1
                AND evidence.source_kind IN ('authorized_canary', 'authorized_sandbox')",
            &[&activation.manifest_sha256],
        )
        .map_err(ats_certification_storage)?
        .get(0);
    let production_check_pair_count: i64 = tx
        .query_one(
            "SELECT COUNT(*)
               FROM (
                 SELECT DISTINCT result.runner_target_sha256, result.evidence_class
                   FROM jobs_ats_certification_manifest_check_results result
                  WHERE result.manifest_sha256 = $1
                    AND result.evidence_class IN ('authorized_sandbox', 'authorized_live')
               ) required_pairs",
            &[&activation.manifest_sha256],
        )
        .map_err(ats_certification_storage)?
        .get(0);
    let production_layout_pair_count: i64 = tx
        .query_one(
            "SELECT COUNT(*)
               FROM (
                 SELECT DISTINCT observation.runner_target_sha256,
                                 observation.evidence_class
                   FROM jobs_ats_certification_manifest_layouts binding
                   JOIN jobs_ats_certification_layout_observations observation
                     ON observation.observation_sha256 = binding.observation_sha256
                  WHERE binding.manifest_sha256 = $1
                    AND observation.evidence_class IN (
                      'authorized_sandbox', 'authorized_live'
                    )
               ) required_pairs",
            &[&activation.manifest_sha256],
        )
        .map_err(ats_certification_storage)?
        .get(0);
    let synthetic_layout_count: i64 = tx
        .query_one(
            "SELECT COUNT(*)
               FROM jobs_ats_certification_manifest_layouts binding
               JOIN jobs_ats_certification_layout_observations observation
                 ON observation.observation_sha256 = binding.observation_sha256
              WHERE binding.manifest_sha256 = $1
                AND observation.evidence_class = 'synthetic'",
            &[&activation.manifest_sha256],
        )
        .map_err(ats_certification_storage)?
        .get(0);
    require_ats_activation_manifest_binding(
        activation,
        row,
        authorized_evidence_count,
        production_check_pair_count,
        production_layout_pair_count,
        synthetic_layout_count,
        trust_policy_sha256,
    )
}

fn require_ats_activation_predecessor(
    activation: &AtsCertificationActivationAuthority,
    predecessor: Option<(i64, String, String, i64)>,
) -> Result<(), AtsCertificationAuthorityError> {
    match predecessor {
        None if activation.activation_generation == 1
            && activation.channel_sequence == 1
            && activation.predecessor_activation_sha256.is_none() =>
        {
            Ok(())
        }
        Some((generation, scope_sha256, channel, channel_sequence))
            if activation.activation_generation == generation + 1
                && scope_sha256 == activation.scope_sha256
                && channel == activation.channel
                && activation.channel_sequence == channel_sequence + 1 =>
        {
            Ok(())
        }
        _ => Err(AtsCertificationAuthorityError::SequenceRegression),
    }
}

fn require_sqlite_ats_activation_predecessor(
    tx: &rusqlite::Transaction<'_>,
    activation: &AtsCertificationActivationAuthority,
    trust_policy_sha256: &str,
) -> Result<(), AtsCertificationAuthorityError> {
    let predecessor = match activation.predecessor_activation_sha256.as_deref() {
        Some(sha256) => tx
            .query_row(
                "SELECT activation_generation, scope_sha256, channel, channel_sequence
                   FROM jobs_ats_certification_activations
                  WHERE activation_sha256 = ?1 AND trust_policy_sha256 = ?2",
                params![sha256, trust_policy_sha256],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .optional()
            .map_err(ats_certification_storage)?,
        None => None,
    };
    require_ats_activation_predecessor(activation, predecessor)
}

fn require_postgres_ats_activation_predecessor(
    tx: &mut postgres::Transaction<'_>,
    activation: &AtsCertificationActivationAuthority,
    trust_policy_sha256: &str,
) -> Result<(), AtsCertificationAuthorityError> {
    let predecessor = match activation.predecessor_activation_sha256.as_deref() {
        Some(sha256) => tx
            .query_opt(
                "SELECT activation_generation, scope_sha256, channel, channel_sequence
                   FROM jobs_ats_certification_activations
                  WHERE activation_sha256 = $1 AND trust_policy_sha256 = $2",
                &[&sha256, &trust_policy_sha256],
            )
            .map_err(ats_certification_storage)?
            .map(|row| (row.get(0), row.get(1), row.get(2), row.get(3))),
        None => None,
    };
    require_ats_activation_predecessor(activation, predecessor)
}

fn insert_sqlite_ats_activation(
    tx: &rusqlite::Transaction<'_>,
    verified: &VerifiedAtsCertificationEnvelope<AtsCertificationActivationAuthority>,
    envelope: &AtsCertificationAuthorityEnvelope,
    recorded_by: &str,
    recorded_at_ms: i64,
) -> Result<(), AtsCertificationAuthorityError> {
    let activation = &verified.authority;
    tx.execute(
        "INSERT INTO jobs_ats_certification_activations (
           activation_sha256, activation_id, activation_generation,
           predecessor_activation_sha256, manifest_sha256,
           provider, adapter_version, scope_sha256, channel, channel_sequence, capability,
           account_allowlist_sha256, canary_max_submissions, canary_account_cap,
           canary_concurrency_cap, canary_daily_side_effect_cap,
           canary_evidence_manifest_sha256, approval_ref,
           canonical_activation_base64url, authorization_sha256, trust_policy_sha256,
           canonical_authorization_base64url, issued_at_ms, not_before_ms,
           expires_at_ms, recorded_by, recorded_at_ms
         ) VALUES (
           ?1, ?2, ?3, ?4, ?5,
           (SELECT provider FROM jobs_ats_certification_manifests WHERE manifest_sha256 = ?5),
           (SELECT adapter_version FROM jobs_ats_certification_manifests WHERE manifest_sha256 = ?5),
           ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18,
           ?19, ?20, ?21, ?22, ?23, ?24, ?25
         )",
        params![
            verified.authority_sha256,
            activation.activation_id,
            activation.activation_generation,
            activation.predecessor_activation_sha256,
            activation.manifest_sha256,
            activation.scope_sha256,
            activation.channel,
            activation.channel_sequence,
            activation.capability,
            activation.account_allowlist_sha256,
            activation.canary_max_submissions,
            activation.canary_account_cap,
            activation.canary_concurrency_cap,
            activation.canary_daily_side_effect_cap,
            activation.canary_evidence_manifest_sha256,
            activation.approval_ref,
            envelope.canonical_base64url,
            verified.authorization_sha256,
            verified.trust_policy_sha256,
            envelope.authorization_base64url,
            activation.issued_at_ms,
            activation.not_before_ms,
            activation.expires_at_ms,
            recorded_by,
            recorded_at_ms,
        ],
    )
    .map_err(ats_certification_storage)?;
    Ok(())
}

fn insert_postgres_ats_activation(
    tx: &mut postgres::Transaction<'_>,
    verified: &VerifiedAtsCertificationEnvelope<AtsCertificationActivationAuthority>,
    envelope: &AtsCertificationAuthorityEnvelope,
    recorded_by: &str,
    recorded_at_ms: i64,
) -> Result<(), AtsCertificationAuthorityError> {
    let activation = &verified.authority;
    tx.execute(
        "INSERT INTO jobs_ats_certification_activations (
           activation_sha256, activation_id, activation_generation,
           predecessor_activation_sha256, manifest_sha256,
           provider, adapter_version, scope_sha256, channel, channel_sequence, capability,
           account_allowlist_sha256, canary_max_submissions, canary_account_cap,
           canary_concurrency_cap, canary_daily_side_effect_cap,
           canary_evidence_manifest_sha256, approval_ref,
           canonical_activation_base64url, authorization_sha256, trust_policy_sha256,
           canonical_authorization_base64url, issued_at_ms, not_before_ms,
           expires_at_ms, recorded_by, recorded_at_ms
         ) VALUES (
           $1, $2, $3, $4, $5,
           (SELECT provider FROM jobs_ats_certification_manifests WHERE manifest_sha256 = $5),
           (SELECT adapter_version FROM jobs_ats_certification_manifests WHERE manifest_sha256 = $5),
           $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18,
           $19, $20, $21, $22, $23, $24, $25
         )",
        &[
            &verified.authority_sha256,
            &activation.activation_id,
            &activation.activation_generation,
            &activation.predecessor_activation_sha256,
            &activation.manifest_sha256,
            &activation.scope_sha256,
            &activation.channel,
            &activation.channel_sequence,
            &activation.capability,
            &activation.account_allowlist_sha256,
            &activation.canary_max_submissions,
            &activation.canary_account_cap,
            &activation.canary_concurrency_cap,
            &activation.canary_daily_side_effect_cap,
            &activation.canary_evidence_manifest_sha256,
            &activation.approval_ref,
            &envelope.canonical_base64url,
            &verified.authorization_sha256,
            &verified.trust_policy_sha256,
            &envelope.authorization_base64url,
            &activation.issued_at_ms,
            &activation.not_before_ms,
            &activation.expires_at_ms,
            &recorded_by,
            &recorded_at_ms,
        ],
    )
    .map_err(ats_certification_storage)?;
    Ok(())
}

pub fn apply_ats_certification_activation(
    pool: &DbPool,
    activation_sha256: &str,
    expected_head_revision: i64,
    expected_transition_sha256: Option<&str>,
    recorded_by: &str,
) -> Result<AtsCertificationHeadResult, AtsCertificationAuthorityError> {
    apply_ats_certification_activation_at(
        pool,
        activation_sha256,
        expected_head_revision,
        expected_transition_sha256,
        recorded_by,
        now_ms(),
    )
}

fn apply_ats_certification_activation_at(
    pool: &DbPool,
    activation_sha256: &str,
    expected_head_revision: i64,
    expected_transition_sha256: Option<&str>,
    recorded_by: &str,
    recorded_at_ms: i64,
) -> Result<AtsCertificationHeadResult, AtsCertificationAuthorityError> {
    validate_ats_recorded_by(recorded_by)?;
    if !ats_certification_hex64(activation_sha256)
        || !ats_certification_safe_integer(expected_head_revision, false)
        || expected_transition_sha256.is_some_and(|value| !ats_certification_hex64(value))
        || (expected_head_revision == 0) != expected_transition_sha256.is_none()
    {
        return Err(AtsCertificationAuthorityError::InvalidAuthority);
    }
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get().map_err(ats_certification_storage)?;
            let tx = conn
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(ats_certification_storage)?;
            let activation = sqlite_ats_activation_for_head(&tx, activation_sha256)?
                .ok_or(AtsCertificationAuthorityError::NotFound)?;
            require_current_sqlite_ats_trust_policy(
                &tx,
                &activation.trust_policy_sha256,
                recorded_at_ms,
            )?;
            let current = sqlite_ats_head(&tx, &activation.scope_sha256, &activation.channel)?;
            let replay_predecessor_transition_sha256 = match current
                .as_ref()
                .filter(|head| head.activation_sha256 == activation.activation_sha256)
            {
                Some(head) => sqlite_ats_head_transition_predecessor(&tx, &activation, head)?,
                None => None,
            };
            if let Some(replay) = ats_activation_head_replay(
                current.as_ref(),
                &activation,
                expected_head_revision,
                expected_transition_sha256,
                replay_predecessor_transition_sha256.as_deref(),
                recorded_by,
            )? {
                tx.commit().map_err(ats_certification_storage)?;
                return Ok(replay);
            }
            require_expected_ats_head(
                current.as_ref(),
                expected_head_revision,
                expected_transition_sha256,
                activation.channel_sequence,
            )?;
            ensure_sqlite_ats_activation_available(&tx, &activation, recorded_at_ms)?;
            let result = apply_sqlite_ats_head(
                &tx,
                &activation,
                current.as_ref(),
                recorded_by,
                recorded_at_ms,
            )?;
            tx.commit().map_err(ats_certification_storage)?;
            Ok(result)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg().map_err(ats_certification_storage)?;
            let mut tx = conn.transaction().map_err(ats_certification_storage)?;
            lock_postgres_ats_certification(&mut tx)?;
            let activation = postgres_ats_activation_for_head(&mut tx, activation_sha256)?
                .ok_or(AtsCertificationAuthorityError::NotFound)?;
            require_current_postgres_ats_trust_policy(
                &mut tx,
                &activation.trust_policy_sha256,
                recorded_at_ms,
            )?;
            let current =
                postgres_ats_head(&mut tx, &activation.scope_sha256, &activation.channel)?;
            let replay_predecessor_transition_sha256 = match current
                .as_ref()
                .filter(|head| head.activation_sha256 == activation.activation_sha256)
            {
                Some(head) => postgres_ats_head_transition_predecessor(&mut tx, &activation, head)?,
                None => None,
            };
            if let Some(replay) = ats_activation_head_replay(
                current.as_ref(),
                &activation,
                expected_head_revision,
                expected_transition_sha256,
                replay_predecessor_transition_sha256.as_deref(),
                recorded_by,
            )? {
                tx.commit().map_err(ats_certification_storage)?;
                return Ok(replay);
            }
            require_expected_ats_head(
                current.as_ref(),
                expected_head_revision,
                expected_transition_sha256,
                activation.channel_sequence,
            )?;
            ensure_postgres_ats_activation_available(&mut tx, &activation, recorded_at_ms)?;
            let result = apply_postgres_ats_head(
                &mut tx,
                &activation,
                current.as_ref(),
                recorded_by,
                recorded_at_ms,
            )?;
            tx.commit().map_err(ats_certification_storage)?;
            Ok(result)
        }
    })
}

fn ats_activation_head_replay(
    current: Option<&StoredAtsHead>,
    activation: &StoredAtsActivationHeadAuthority,
    expected_head_revision: i64,
    expected_transition_sha256: Option<&str>,
    stored_predecessor_transition_sha256: Option<&str>,
    recorded_by: &str,
) -> Result<Option<AtsCertificationHeadResult>, AtsCertificationAuthorityError> {
    let Some(current) = current else {
        return Ok(None);
    };
    if current.activation_sha256 != activation.activation_sha256 {
        return Ok(None);
    }
    if current.head_revision != expected_head_revision + 1
        || current.updated_by != recorded_by
        || (expected_head_revision > 0
            && expected_transition_sha256 != stored_predecessor_transition_sha256)
    {
        return Err(AtsCertificationAuthorityError::CompareAndSwapConflict);
    }
    Ok(Some(AtsCertificationHeadResult {
        scope_sha256: activation.scope_sha256.clone(),
        channel: activation.channel.clone(),
        head_revision: current.head_revision,
        transition_sha256: current.transition_sha256.clone(),
        activation_sha256: current.activation_sha256.clone(),
        channel_sequence: current.channel_sequence,
        replayed: true,
    }))
}

fn require_expected_ats_head(
    current: Option<&StoredAtsHead>,
    expected_head_revision: i64,
    expected_transition_sha256: Option<&str>,
    next_sequence: i64,
) -> Result<(), AtsCertificationAuthorityError> {
    match current {
        None if expected_head_revision == 0 && expected_transition_sha256.is_none() => Ok(()),
        Some(current)
            if current.head_revision == expected_head_revision
                && expected_transition_sha256 == Some(current.transition_sha256.as_str())
                && next_sequence > current.channel_sequence =>
        {
            Ok(())
        }
        Some(current) if next_sequence <= current.channel_sequence => {
            Err(AtsCertificationAuthorityError::SequenceRegression)
        }
        _ => Err(AtsCertificationAuthorityError::CompareAndSwapConflict),
    }
}

fn sqlite_ats_activation_for_head(
    tx: &rusqlite::Transaction<'_>,
    activation_sha256: &str,
) -> Result<Option<StoredAtsActivationHeadAuthority>, AtsCertificationAuthorityError> {
    tx.query_row(
        "SELECT activation_sha256, activation_id, activation_generation,
                manifest_sha256, scope_sha256, channel, channel_sequence, capability,
                account_allowlist_sha256, canary_max_submissions, canary_account_cap,
                canary_concurrency_cap, canary_daily_side_effect_cap, not_before_ms,
                expires_at_ms, trust_policy_sha256
           FROM jobs_ats_certification_activations WHERE activation_sha256 = ?1",
        params![activation_sha256],
        |row| {
            Ok(StoredAtsActivationHeadAuthority {
                activation_sha256: row.get(0)?,
                activation_id: row.get(1)?,
                activation_generation: row.get(2)?,
                manifest_sha256: row.get(3)?,
                scope_sha256: row.get(4)?,
                channel: row.get(5)?,
                channel_sequence: row.get(6)?,
                capability: row.get(7)?,
                account_allowlist_sha256: row.get(8)?,
                canary_max_submissions: row.get(9)?,
                canary_account_cap: row.get(10)?,
                canary_concurrency_cap: row.get(11)?,
                canary_daily_side_effect_cap: row.get(12)?,
                not_before_ms: row.get(13)?,
                expires_at_ms: row.get(14)?,
                trust_policy_sha256: row.get(15)?,
            })
        },
    )
    .optional()
    .map_err(ats_certification_storage)
}

fn postgres_ats_activation_for_head(
    tx: &mut postgres::Transaction<'_>,
    activation_sha256: &str,
) -> Result<Option<StoredAtsActivationHeadAuthority>, AtsCertificationAuthorityError> {
    Ok(tx
        .query_opt(
            "SELECT activation_sha256, activation_id, activation_generation,
                    manifest_sha256, scope_sha256, channel, channel_sequence, capability,
                    account_allowlist_sha256, canary_max_submissions, canary_account_cap,
                    canary_concurrency_cap, canary_daily_side_effect_cap, not_before_ms,
                    expires_at_ms, trust_policy_sha256
               FROM jobs_ats_certification_activations WHERE activation_sha256 = $1",
            &[&activation_sha256],
        )
        .map_err(ats_certification_storage)?
        .map(|row| StoredAtsActivationHeadAuthority {
            activation_sha256: row.get(0),
            activation_id: row.get(1),
            activation_generation: row.get(2),
            manifest_sha256: row.get(3),
            scope_sha256: row.get(4),
            channel: row.get(5),
            channel_sequence: row.get(6),
            capability: row.get(7),
            account_allowlist_sha256: row.get(8),
            canary_max_submissions: row.get(9),
            canary_account_cap: row.get(10),
            canary_concurrency_cap: row.get(11),
            canary_daily_side_effect_cap: row.get(12),
            not_before_ms: row.get(13),
            expires_at_ms: row.get(14),
            trust_policy_sha256: row.get(15),
        }))
}

fn sqlite_ats_head(
    tx: &rusqlite::Transaction<'_>,
    scope_sha256: &str,
    channel: &str,
) -> Result<Option<StoredAtsHead>, AtsCertificationAuthorityError> {
    tx.query_row(
        "SELECT head_revision, current_transition_sha256, current_activation_sha256,
                current_channel_sequence, updated_by
           FROM jobs_ats_certification_heads
          WHERE scope_sha256 = ?1 AND channel = ?2",
        params![scope_sha256, channel],
        |row| {
            Ok(StoredAtsHead {
                head_revision: row.get(0)?,
                transition_sha256: row.get(1)?,
                activation_sha256: row.get(2)?,
                channel_sequence: row.get(3)?,
                updated_by: row.get(4)?,
            })
        },
    )
    .optional()
    .map_err(ats_certification_storage)
}

fn postgres_ats_head(
    tx: &mut postgres::Transaction<'_>,
    scope_sha256: &str,
    channel: &str,
) -> Result<Option<StoredAtsHead>, AtsCertificationAuthorityError> {
    Ok(tx
        .query_opt(
            "SELECT head_revision, current_transition_sha256, current_activation_sha256,
                    current_channel_sequence, updated_by
               FROM jobs_ats_certification_heads
              WHERE scope_sha256 = $1 AND channel = $2",
            &[&scope_sha256, &channel],
        )
        .map_err(ats_certification_storage)?
        .map(|row| StoredAtsHead {
            head_revision: row.get(0),
            transition_sha256: row.get(1),
            activation_sha256: row.get(2),
            channel_sequence: row.get(3),
            updated_by: row.get(4),
        }))
}

fn sqlite_ats_head_transition_predecessor(
    tx: &rusqlite::Transaction<'_>,
    activation: &StoredAtsActivationHeadAuthority,
    current: &StoredAtsHead,
) -> Result<Option<String>, AtsCertificationAuthorityError> {
    tx.query_row(
        "SELECT previous_transition_sha256
           FROM jobs_ats_certification_head_transitions
          WHERE transition_sha256 = ?1 AND scope_sha256 = ?2 AND channel = ?3
            AND head_revision = ?4 AND next_activation_sha256 = ?5
            AND next_channel_sequence = ?6",
        params![
            current.transition_sha256,
            activation.scope_sha256,
            activation.channel,
            current.head_revision,
            activation.activation_sha256,
            current.channel_sequence,
        ],
        |row| row.get(0),
    )
    .map_err(ats_certification_storage)
}

fn postgres_ats_head_transition_predecessor(
    tx: &mut postgres::Transaction<'_>,
    activation: &StoredAtsActivationHeadAuthority,
    current: &StoredAtsHead,
) -> Result<Option<String>, AtsCertificationAuthorityError> {
    tx.query_one(
        "SELECT previous_transition_sha256
           FROM jobs_ats_certification_head_transitions
          WHERE transition_sha256 = $1 AND scope_sha256 = $2 AND channel = $3
            AND head_revision = $4 AND next_activation_sha256 = $5
            AND next_channel_sequence = $6",
        &[
            &current.transition_sha256,
            &activation.scope_sha256,
            &activation.channel,
            &current.head_revision,
            &activation.activation_sha256,
            &current.channel_sequence,
        ],
    )
    .map(|row| row.get(0))
    .map_err(ats_certification_storage)
}

fn ensure_ats_activation_time(
    activation: &StoredAtsActivationHeadAuthority,
    now_ms: i64,
) -> Result<(), AtsCertificationAuthorityError> {
    if now_ms < activation.not_before_ms || now_ms >= activation.expires_at_ms {
        return Err(AtsCertificationAuthorityError::Expired);
    }
    Ok(())
}

fn ensure_sqlite_ats_activation_available(
    tx: &rusqlite::Transaction<'_>,
    activation: &StoredAtsActivationHeadAuthority,
    now_ms: i64,
) -> Result<(), AtsCertificationAuthorityError> {
    ensure_ats_activation_time(activation, now_ms)?;
    let unavailable: i64 = tx
        .query_row(
            "SELECT COUNT(*)
               FROM jobs_ats_certification_manifests manifest
              WHERE manifest.manifest_sha256 = ?1
                AND (manifest.trust_policy_sha256 <> ?5
                  OR manifest.not_before_ms > ?2 OR manifest.expires_at_ms <= ?2
                  OR EXISTS (
                    SELECT 1 FROM jobs_ats_certification_revocations revocation
                     WHERE revocation.effective_at_ms <= ?2
                       AND revocation.trust_policy_sha256 = ?5 AND (
                       (revocation.subject_kind = 'activation'
                         AND revocation.subject_id = ?3
                         AND revocation.subject_sha256 = ?4)
                       OR (revocation.subject_kind = 'manifest'
                         AND revocation.subject_id = manifest.certification_id
                         AND revocation.subject_sha256 = manifest.manifest_sha256)
                       OR (revocation.subject_kind = 'policy'
                         AND revocation.subject_sha256 = manifest.trust_policy_sha256)
                       OR (revocation.subject_kind = 'trust_key' AND EXISTS (
                         SELECT 1 FROM jobs_ats_certification_trust_keys trust_key
                          WHERE trust_key.policy_sha256 = manifest.trust_policy_sha256
                            AND trust_key.key_id = revocation.subject_id
                       ))
                       OR (revocation.subject_kind = 'layout_observation' AND EXISTS (
                         SELECT 1 FROM jobs_ats_certification_manifest_layouts layout_binding
                         JOIN jobs_ats_certification_layout_observations observation
                           ON observation.observation_sha256 = layout_binding.observation_sha256
                        WHERE layout_binding.manifest_sha256 = manifest.manifest_sha256
                          AND observation.observation_id = revocation.subject_id
                          AND observation.observation_sha256 = revocation.subject_sha256
                       ))
                       OR (revocation.subject_kind = 'target'
                         AND revocation.subject_id = manifest.target_key)
                       OR (revocation.subject_kind = 'adapter_bundle'
                         AND revocation.subject_id = manifest.adapter_version
                         AND revocation.subject_sha256 = manifest.adapter_bundle_sha256)
                       OR (revocation.subject_kind = 'scope'
                         AND revocation.subject_id = manifest.scope_sha256
                         AND revocation.subject_sha256 = manifest.scope_sha256)
                       OR (revocation.subject_kind = 'evidence' AND EXISTS (
                         SELECT 1 FROM jobs_ats_certification_manifest_evidence binding
                          WHERE binding.manifest_sha256 = manifest.manifest_sha256
                            AND binding.evidence_sha256 = revocation.subject_sha256
                       ))
                     )
                  )
                )",
            params![
                activation.manifest_sha256,
                now_ms,
                activation.activation_id,
                activation.activation_sha256,
                activation.trust_policy_sha256,
            ],
            |row| row.get(0),
        )
        .map_err(ats_certification_storage)?;
    if unavailable != 0 {
        return Err(AtsCertificationAuthorityError::Revoked);
    }
    if sqlite_ats_base_quarantined(tx, activation, now_ms)? {
        return Err(AtsCertificationAuthorityError::Quarantined);
    }
    Ok(())
}

fn ensure_postgres_ats_activation_available(
    tx: &mut postgres::Transaction<'_>,
    activation: &StoredAtsActivationHeadAuthority,
    now_ms: i64,
) -> Result<(), AtsCertificationAuthorityError> {
    ensure_ats_activation_time(activation, now_ms)?;
    let unavailable: i64 = tx
        .query_one(
            "SELECT COUNT(*)
               FROM jobs_ats_certification_manifests manifest
              WHERE manifest.manifest_sha256 = $1
                AND (manifest.trust_policy_sha256 <> $5
                  OR manifest.not_before_ms > $2 OR manifest.expires_at_ms <= $2
                  OR EXISTS (
                    SELECT 1 FROM jobs_ats_certification_revocations revocation
                     WHERE revocation.effective_at_ms <= $2
                       AND revocation.trust_policy_sha256 = $5 AND (
                       (revocation.subject_kind = 'activation'
                         AND revocation.subject_id = $3
                         AND revocation.subject_sha256 = $4)
                       OR (revocation.subject_kind = 'manifest'
                         AND revocation.subject_id = manifest.certification_id
                         AND revocation.subject_sha256 = manifest.manifest_sha256)
                       OR (revocation.subject_kind = 'policy'
                         AND revocation.subject_sha256 = manifest.trust_policy_sha256)
                       OR (revocation.subject_kind = 'trust_key' AND EXISTS (
                         SELECT 1 FROM jobs_ats_certification_trust_keys trust_key
                          WHERE trust_key.policy_sha256 = manifest.trust_policy_sha256
                            AND trust_key.key_id = revocation.subject_id
                       ))
                       OR (revocation.subject_kind = 'layout_observation' AND EXISTS (
                         SELECT 1 FROM jobs_ats_certification_manifest_layouts layout_binding
                         JOIN jobs_ats_certification_layout_observations observation
                           ON observation.observation_sha256 = layout_binding.observation_sha256
                        WHERE layout_binding.manifest_sha256 = manifest.manifest_sha256
                          AND observation.observation_id = revocation.subject_id
                          AND observation.observation_sha256 = revocation.subject_sha256
                       ))
                       OR (revocation.subject_kind = 'target'
                         AND revocation.subject_id = manifest.target_key)
                       OR (revocation.subject_kind = 'adapter_bundle'
                         AND revocation.subject_id = manifest.adapter_version
                         AND revocation.subject_sha256 = manifest.adapter_bundle_sha256)
                       OR (revocation.subject_kind = 'scope'
                         AND revocation.subject_id = manifest.scope_sha256
                         AND revocation.subject_sha256 = manifest.scope_sha256)
                       OR (revocation.subject_kind = 'evidence' AND EXISTS (
                         SELECT 1 FROM jobs_ats_certification_manifest_evidence binding
                          WHERE binding.manifest_sha256 = manifest.manifest_sha256
                            AND binding.evidence_sha256 = revocation.subject_sha256
                       ))
                     )
                  )
                )",
            &[
                &activation.manifest_sha256,
                &now_ms,
                &activation.activation_id,
                &activation.activation_sha256,
                &activation.trust_policy_sha256,
            ],
        )
        .map_err(ats_certification_storage)?
        .get(0);
    if unavailable != 0 {
        return Err(AtsCertificationAuthorityError::Revoked);
    }
    if postgres_ats_base_quarantined(tx, activation, now_ms)? {
        return Err(AtsCertificationAuthorityError::Quarantined);
    }
    Ok(())
}

fn sqlite_ats_base_quarantined(
    tx: &rusqlite::Transaction<'_>,
    activation: &StoredAtsActivationHeadAuthority,
    _now_ms: i64,
) -> Result<bool, AtsCertificationAuthorityError> {
    let row: Option<(String, String, String, String, String, String)> = tx
        .query_row(
            "SELECT provider, target_key, variant_key, scope_sha256, adapter_version,
                    adapter_bundle_sha256
               FROM jobs_ats_certification_manifests WHERE manifest_sha256 = ?1",
            params![activation.manifest_sha256],
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
        .optional()
        .map_err(ats_certification_storage)?;
    let row = row.ok_or(AtsCertificationAuthorityError::NotFound)?;
    let targets = ats_base_quarantine_targets(&row, activation)?;
    for (kind, id, sha256) in targets {
        let state: Option<String> = tx
            .query_row(
                "SELECT head.state FROM jobs_ats_certification_quarantine_heads head
                  JOIN jobs_ats_certification_quarantine_commands command
                    ON command.command_sha256 = head.current_command_sha256
                 WHERE head.scope_kind = ?1 AND head.scope_id = ?2 AND head.scope_sha256 = ?3
                   AND command.trust_policy_sha256 = ?4",
                params![kind, id, sha256, activation.trust_policy_sha256],
                |row| row.get(0),
            )
            .optional()
            .map_err(ats_certification_storage)?;
        if state.as_deref() == Some("quarantined") {
            return Ok(true);
        }
    }
    Ok(false)
}

fn postgres_ats_base_quarantined(
    tx: &mut postgres::Transaction<'_>,
    activation: &StoredAtsActivationHeadAuthority,
    _now_ms: i64,
) -> Result<bool, AtsCertificationAuthorityError> {
    let row = tx
        .query_opt(
            "SELECT provider, target_key, variant_key, scope_sha256, adapter_version,
                    adapter_bundle_sha256
               FROM jobs_ats_certification_manifests WHERE manifest_sha256 = $1",
            &[&activation.manifest_sha256],
        )
        .map_err(ats_certification_storage)?
        .map(|row| {
            (
                row.get(0),
                row.get(1),
                row.get(2),
                row.get(3),
                row.get(4),
                row.get(5),
            )
        })
        .ok_or(AtsCertificationAuthorityError::NotFound)?;
    let targets = ats_base_quarantine_targets(&row, activation)?;
    for (kind, id, sha256) in targets {
        let state = tx
            .query_opt(
                "SELECT head.state FROM jobs_ats_certification_quarantine_heads head
                  JOIN jobs_ats_certification_quarantine_commands command
                    ON command.command_sha256 = head.current_command_sha256
                 WHERE head.scope_kind = $1 AND head.scope_id = $2 AND head.scope_sha256 = $3
                   AND command.trust_policy_sha256 = $4",
                &[&kind, &id, &sha256, &activation.trust_policy_sha256],
            )
            .map_err(ats_certification_storage)?
            .map(|row| row.get::<_, String>(0));
        if state.as_deref() == Some("quarantined") {
            return Ok(true);
        }
    }
    Ok(false)
}

fn ats_base_quarantine_targets(
    manifest: &(String, String, String, String, String, String),
    activation: &StoredAtsActivationHeadAuthority,
) -> Result<Vec<(String, String, String)>, AtsCertificationAuthorityError> {
    Ok(vec![
        (
            "provider".to_string(),
            manifest.0.clone(),
            ats_named_quarantine_scope_sha256("provider", &manifest.0)?,
        ),
        (
            "target".to_string(),
            manifest.1.clone(),
            ats_named_quarantine_scope_sha256("target", &manifest.1)?,
        ),
        (
            "surface".to_string(),
            manifest.2.clone(),
            manifest.3.clone(),
        ),
        (
            "adapter".to_string(),
            manifest.4.clone(),
            manifest.5.clone(),
        ),
        (
            "activation".to_string(),
            activation.activation_id.clone(),
            activation.activation_sha256.clone(),
        ),
    ])
}

pub fn ats_named_quarantine_scope_sha256(
    scope_kind: &str,
    scope_id: &str,
) -> Result<String, AtsCertificationAuthorityError> {
    if !matches!(scope_kind, "provider" | "target") || !ats_certification_text(scope_id, 1, 240) {
        return Err(AtsCertificationAuthorityError::InvalidAuthority);
    }
    let value = serde_json::json!({"scopeId": scope_id, "scopeKind": scope_kind});
    Ok(ats_certification_sha256(
        &ats_certification_canonical_json(&value)
            .map_err(|_| AtsCertificationAuthorityError::InvalidAuthority)?,
    ))
}

fn apply_sqlite_ats_head(
    tx: &rusqlite::Transaction<'_>,
    activation: &StoredAtsActivationHeadAuthority,
    current: Option<&StoredAtsHead>,
    recorded_by: &str,
    recorded_at_ms: i64,
) -> Result<AtsCertificationHeadResult, AtsCertificationAuthorityError> {
    let transition_sha256 =
        ats_head_transition_sha256(activation, current, recorded_by, recorded_at_ms)?;
    let head_revision = current.map_or(1, |head| head.head_revision + 1);
    tx.execute(
        "INSERT INTO jobs_ats_certification_head_transitions (
           transition_sha256, scope_sha256, channel, head_revision,
           previous_head_revision, previous_transition_sha256,
           previous_activation_sha256, next_activation_sha256,
           next_channel_sequence, recorded_by, recorded_at_ms
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![
            transition_sha256,
            activation.scope_sha256,
            activation.channel,
            head_revision,
            current.map_or(0, |head| head.head_revision),
            current.map(|head| head.transition_sha256.as_str()),
            current.map(|head| head.activation_sha256.as_str()),
            activation.activation_sha256,
            activation.channel_sequence,
            recorded_by,
            recorded_at_ms,
        ],
    )
    .map_err(ats_certification_storage)?;
    if let Some(current) = current {
        let changed = tx
            .execute(
                "UPDATE jobs_ats_certification_heads SET
                   head_revision = ?1, current_transition_sha256 = ?2,
                   current_activation_sha256 = ?3, current_channel_sequence = ?4,
                   updated_by = ?5, updated_at_ms = ?6
                 WHERE scope_sha256 = ?7 AND channel = ?8
                   AND head_revision = ?9 AND current_transition_sha256 = ?10",
                params![
                    head_revision,
                    transition_sha256,
                    activation.activation_sha256,
                    activation.channel_sequence,
                    recorded_by,
                    recorded_at_ms,
                    activation.scope_sha256,
                    activation.channel,
                    current.head_revision,
                    current.transition_sha256,
                ],
            )
            .map_err(ats_certification_storage)?;
        if changed != 1 {
            return Err(AtsCertificationAuthorityError::CompareAndSwapConflict);
        }
    } else {
        tx.execute(
            "INSERT INTO jobs_ats_certification_heads (
               scope_sha256, channel, head_revision, current_transition_sha256,
               current_activation_sha256, current_channel_sequence, updated_by,
               updated_at_ms
             ) VALUES (?1, ?2, 1, ?3, ?4, ?5, ?6, ?7)",
            params![
                activation.scope_sha256,
                activation.channel,
                transition_sha256,
                activation.activation_sha256,
                activation.channel_sequence,
                recorded_by,
                recorded_at_ms,
            ],
        )
        .map_err(ats_certification_storage)?;
    }
    Ok(AtsCertificationHeadResult {
        scope_sha256: activation.scope_sha256.clone(),
        channel: activation.channel.clone(),
        head_revision,
        transition_sha256,
        activation_sha256: activation.activation_sha256.clone(),
        channel_sequence: activation.channel_sequence,
        replayed: false,
    })
}

fn apply_postgres_ats_head(
    tx: &mut postgres::Transaction<'_>,
    activation: &StoredAtsActivationHeadAuthority,
    current: Option<&StoredAtsHead>,
    recorded_by: &str,
    recorded_at_ms: i64,
) -> Result<AtsCertificationHeadResult, AtsCertificationAuthorityError> {
    let transition_sha256 =
        ats_head_transition_sha256(activation, current, recorded_by, recorded_at_ms)?;
    let head_revision = current.map_or(1, |head| head.head_revision + 1);
    tx.execute(
        "INSERT INTO jobs_ats_certification_head_transitions (
           transition_sha256, scope_sha256, channel, head_revision,
           previous_head_revision, previous_transition_sha256,
           previous_activation_sha256, next_activation_sha256,
           next_channel_sequence, recorded_by, recorded_at_ms
         ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
        &[
            &transition_sha256,
            &activation.scope_sha256,
            &activation.channel,
            &head_revision,
            &current.map_or(0, |head| head.head_revision),
            &current.map(|head| head.transition_sha256.as_str()),
            &current.map(|head| head.activation_sha256.as_str()),
            &activation.activation_sha256,
            &activation.channel_sequence,
            &recorded_by,
            &recorded_at_ms,
        ],
    )
    .map_err(ats_certification_storage)?;
    if let Some(current) = current {
        let changed = tx
            .execute(
                "UPDATE jobs_ats_certification_heads SET
                   head_revision = $1, current_transition_sha256 = $2,
                   current_activation_sha256 = $3, current_channel_sequence = $4,
                   updated_by = $5, updated_at_ms = $6
                 WHERE scope_sha256 = $7 AND channel = $8
                   AND head_revision = $9 AND current_transition_sha256 = $10",
                &[
                    &head_revision,
                    &transition_sha256,
                    &activation.activation_sha256,
                    &activation.channel_sequence,
                    &recorded_by,
                    &recorded_at_ms,
                    &activation.scope_sha256,
                    &activation.channel,
                    &current.head_revision,
                    &current.transition_sha256,
                ],
            )
            .map_err(ats_certification_storage)?;
        if changed != 1 {
            return Err(AtsCertificationAuthorityError::CompareAndSwapConflict);
        }
    } else {
        tx.execute(
            "INSERT INTO jobs_ats_certification_heads (
               scope_sha256, channel, head_revision, current_transition_sha256,
               current_activation_sha256, current_channel_sequence, updated_by,
               updated_at_ms
             ) VALUES ($1, $2, 1, $3, $4, $5, $6, $7)",
            &[
                &activation.scope_sha256,
                &activation.channel,
                &transition_sha256,
                &activation.activation_sha256,
                &activation.channel_sequence,
                &recorded_by,
                &recorded_at_ms,
            ],
        )
        .map_err(ats_certification_storage)?;
    }
    Ok(AtsCertificationHeadResult {
        scope_sha256: activation.scope_sha256.clone(),
        channel: activation.channel.clone(),
        head_revision,
        transition_sha256,
        activation_sha256: activation.activation_sha256.clone(),
        channel_sequence: activation.channel_sequence,
        replayed: false,
    })
}

fn ats_head_transition_sha256(
    activation: &StoredAtsActivationHeadAuthority,
    current: Option<&StoredAtsHead>,
    recorded_by: &str,
    recorded_at_ms: i64,
) -> Result<String, AtsCertificationAuthorityError> {
    let transition = AtsCertificationHeadTransitionDigest {
        version: 1,
        audience: ATS_CERTIFICATION_TRANSITION_AUDIENCE,
        scope_sha256: &activation.scope_sha256,
        channel: &activation.channel,
        head_revision: current.map_or(1, |head| head.head_revision + 1),
        previous_head_revision: current.map_or(0, |head| head.head_revision),
        previous_transition_sha256: current.map(|head| head.transition_sha256.as_str()),
        previous_activation_sha256: current.map(|head| head.activation_sha256.as_str()),
        next_activation_sha256: &activation.activation_sha256,
        next_channel_sequence: activation.channel_sequence,
        recorded_by,
        recorded_at_ms,
    };
    Ok(ats_certification_sha256(
        &ats_certification_canonical_json(&transition)
            .map_err(|_| AtsCertificationAuthorityError::InvalidAuthority)?,
    ))
}

pub fn import_ats_certification_revocation(
    pool: &DbPool,
    envelope: &AtsCertificationAuthorityEnvelope,
    recorded_by: &str,
) -> Result<AtsCertificationImportResult, AtsCertificationAuthorityError> {
    import_ats_certification_revocation_at(pool, envelope, recorded_by, now_ms())
}

fn import_ats_certification_revocation_at(
    pool: &DbPool,
    envelope: &AtsCertificationAuthorityEnvelope,
    recorded_by: &str,
    recorded_at_ms: i64,
) -> Result<AtsCertificationImportResult, AtsCertificationAuthorityError> {
    validate_ats_recorded_by(recorded_by)?;
    let (trust_policy_sha256, trust_policy) =
        load_current_ats_certification_trust_policy(pool, recorded_at_ms)?;
    let mut verified =
        verify_ats_revocation_envelope(envelope, &trust_policy.delegated_trust, recorded_at_ms)?;
    if verified.authority.policy_sha256 != trust_policy_sha256 {
        return Err(AtsCertificationAuthorityError::InvalidTrustPolicy);
    }
    require_ats_authorization_clock_skew(
        envelope,
        verified.authority.issued_at_ms,
        trust_policy
            .certification_requirements
            .maximum_clock_skew_ms,
    )?;
    verified.trust_policy_sha256 = trust_policy_sha256;
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get().map_err(ats_certification_storage)?;
            let tx = conn
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(ats_certification_storage)?;
            require_current_sqlite_ats_trust_policy(
                &tx,
                &verified.trust_policy_sha256,
                recorded_at_ms,
            )?;
            let existing: Option<(String, String, String, String)> = tx
                .query_row(
                    "SELECT revocation_sha256, canonical_revocation_base64url,
                            authorization_sha256, trust_policy_sha256
                       FROM jobs_ats_certification_revocations
                      WHERE revocation_id = ?1 OR revocation_sha256 = ?2
                         OR (subject_kind = ?3 AND subject_id = ?4 AND subject_sha256 = ?5)",
                    params![
                        verified.authority.revocation_id,
                        verified.authority_sha256,
                        verified.authority.subject_kind,
                        verified.authority.subject_id,
                        verified.authority.subject_sha256,
                    ],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
                )
                .optional()
                .map_err(ats_certification_storage)?;
            if let Some(existing) = existing {
                require_exact_ats_authority_replay(
                    existing,
                    &verified,
                    &envelope.canonical_base64url,
                )?;
                tx.commit().map_err(ats_certification_storage)?;
                return Ok(ats_certification_import_result(
                    "revocation",
                    &verified.authority.revocation_id,
                    &verified,
                    true,
                ));
            }
            require_sqlite_ats_revocation_predecessor(&tx, &verified.authority)?;
            if !sqlite_ats_revocation_subject_exists(
                &tx,
                &verified.authority,
                &verified.trust_policy_sha256,
            )? {
                return Err(AtsCertificationAuthorityError::NotFound);
            }
            insert_sqlite_ats_revocation(&tx, &verified, envelope, recorded_by, recorded_at_ms)?;
            tx.commit().map_err(ats_certification_storage)?;
            Ok(ats_certification_import_result(
                "revocation",
                &verified.authority.revocation_id,
                &verified,
                false,
            ))
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg().map_err(ats_certification_storage)?;
            let mut tx = conn.transaction().map_err(ats_certification_storage)?;
            lock_postgres_ats_certification(&mut tx)?;
            require_current_postgres_ats_trust_policy(
                &mut tx,
                &verified.trust_policy_sha256,
                recorded_at_ms,
            )?;
            let existing = tx
                .query_opt(
                    "SELECT revocation_sha256, canonical_revocation_base64url,
                            authorization_sha256, trust_policy_sha256
                       FROM jobs_ats_certification_revocations
                      WHERE revocation_id = $1 OR revocation_sha256 = $2
                         OR (subject_kind = $3 AND subject_id = $4 AND subject_sha256 = $5)",
                    &[
                        &verified.authority.revocation_id,
                        &verified.authority_sha256,
                        &verified.authority.subject_kind,
                        &verified.authority.subject_id,
                        &verified.authority.subject_sha256,
                    ],
                )
                .map_err(ats_certification_storage)?
                .map(|row| (row.get(0), row.get(1), row.get(2), row.get(3)));
            if let Some(existing) = existing {
                require_exact_ats_authority_replay(
                    existing,
                    &verified,
                    &envelope.canonical_base64url,
                )?;
                tx.commit().map_err(ats_certification_storage)?;
                return Ok(ats_certification_import_result(
                    "revocation",
                    &verified.authority.revocation_id,
                    &verified,
                    true,
                ));
            }
            require_postgres_ats_revocation_predecessor(&mut tx, &verified.authority)?;
            if !postgres_ats_revocation_subject_exists(
                &mut tx,
                &verified.authority,
                &verified.trust_policy_sha256,
            )? {
                return Err(AtsCertificationAuthorityError::NotFound);
            }
            insert_postgres_ats_revocation(
                &mut tx,
                &verified,
                envelope,
                recorded_by,
                recorded_at_ms,
            )?;
            tx.commit().map_err(ats_certification_storage)?;
            Ok(ats_certification_import_result(
                "revocation",
                &verified.authority.revocation_id,
                &verified,
                false,
            ))
        }
    })
}

fn require_ats_revocation_predecessor(
    revocation: &AtsCertificationRevocationAuthority,
    predecessor: Option<(i64, String)>,
) -> Result<(), AtsCertificationAuthorityError> {
    match predecessor {
        None if revocation.revocation_generation == 1
            && revocation.predecessor_revocation_sha256.is_none() => {}
        Some((generation, sha256))
            if revocation.revocation_generation == generation + 1
                && revocation.predecessor_revocation_sha256.as_deref() == Some(sha256.as_str()) => {
        }
        _ => return Err(AtsCertificationAuthorityError::SequenceRegression),
    }
    Ok(())
}

fn require_sqlite_ats_revocation_predecessor(
    tx: &rusqlite::Transaction<'_>,
    revocation: &AtsCertificationRevocationAuthority,
) -> Result<(), AtsCertificationAuthorityError> {
    let predecessor = tx
        .query_row(
            "SELECT revocation_generation, revocation_sha256
               FROM jobs_ats_certification_revocations
              WHERE trust_policy_sha256 = ?1
              ORDER BY revocation_generation DESC LIMIT 1",
            params![revocation.policy_sha256],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(ats_certification_storage)?;
    require_ats_revocation_predecessor(revocation, predecessor)
}

fn require_postgres_ats_revocation_predecessor(
    tx: &mut postgres::Transaction<'_>,
    revocation: &AtsCertificationRevocationAuthority,
) -> Result<(), AtsCertificationAuthorityError> {
    let predecessor = tx
        .query_opt(
            "SELECT revocation_generation, revocation_sha256
               FROM jobs_ats_certification_revocations
              WHERE trust_policy_sha256 = $1
              ORDER BY revocation_generation DESC LIMIT 1",
            &[&revocation.policy_sha256],
        )
        .map_err(ats_certification_storage)?
        .map(|row| (row.get(0), row.get(1)));
    require_ats_revocation_predecessor(revocation, predecessor)
}

fn sqlite_ats_revocation_subject_exists(
    tx: &rusqlite::Transaction<'_>,
    revocation: &AtsCertificationRevocationAuthority,
    trust_policy_sha256: &str,
) -> Result<bool, AtsCertificationAuthorityError> {
    if revocation.subject_kind == "trust_key" {
        let public_key: Option<String> = tx
            .query_row(
                "SELECT public_key_base64url
                   FROM jobs_ats_certification_trust_keys
                  WHERE policy_sha256 = ?1 AND key_id = ?2",
                params![trust_policy_sha256, revocation.subject_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(ats_certification_storage)?;
        return ats_revocation_trust_key_matches(public_key.as_deref(), &revocation.subject_sha256);
    }
    if revocation.subject_kind == "target" {
        if ats_certification_target_key_sha256(&revocation.subject_id)? != revocation.subject_sha256
        {
            return Ok(false);
        }
        return tx
            .query_row(
                "SELECT 1 FROM jobs_ats_certification_manifests
                  WHERE target_key = ?1 AND trust_policy_sha256 = ?2 LIMIT 1",
                params![revocation.subject_id, trust_policy_sha256],
                |_| Ok(()),
            )
            .optional()
            .map(|value| value.is_some())
            .map_err(ats_certification_storage);
    }
    if revocation.subject_kind == "runner_build" {
        if ats_revocation_subject_key_sha256(&revocation.subject_id)? != revocation.subject_sha256 {
            return Ok(false);
        }
        return tx
            .query_row(
                "SELECT 1 FROM jobs_ats_certification_runtime_targets runtime
                  JOIN jobs_ats_certification_manifests manifest
                    ON manifest.manifest_sha256 = runtime.manifest_sha256
                 WHERE runtime.runner_build_id = ?1
                   AND manifest.trust_policy_sha256 = ?2 LIMIT 1",
                params![revocation.subject_id, trust_policy_sha256],
                |_| Ok(()),
            )
            .optional()
            .map(|value| value.is_some())
            .map_err(ats_certification_storage);
    }
    let (sql, id, sha256) = ats_revocation_subject_query(revocation, false)?;
    tx.query_row(&sql, params![id, sha256, trust_policy_sha256], |_| Ok(()))
        .optional()
        .map(|value| value.is_some())
        .map_err(ats_certification_storage)
}

fn postgres_ats_revocation_subject_exists(
    tx: &mut postgres::Transaction<'_>,
    revocation: &AtsCertificationRevocationAuthority,
    trust_policy_sha256: &str,
) -> Result<bool, AtsCertificationAuthorityError> {
    if revocation.subject_kind == "trust_key" {
        let public_key = tx
            .query_opt(
                "SELECT public_key_base64url
                   FROM jobs_ats_certification_trust_keys
                  WHERE policy_sha256 = $1 AND key_id = $2",
                &[&trust_policy_sha256, &revocation.subject_id],
            )
            .map_err(ats_certification_storage)?
            .map(|row| row.get::<_, String>(0));
        return ats_revocation_trust_key_matches(public_key.as_deref(), &revocation.subject_sha256);
    }
    if revocation.subject_kind == "target" {
        if ats_certification_target_key_sha256(&revocation.subject_id)? != revocation.subject_sha256
        {
            return Ok(false);
        }
        return tx
            .query_opt(
                "SELECT 1 FROM jobs_ats_certification_manifests
                  WHERE target_key = $1 AND trust_policy_sha256 = $2 LIMIT 1",
                &[&revocation.subject_id, &trust_policy_sha256],
            )
            .map(|value| value.is_some())
            .map_err(ats_certification_storage);
    }
    if revocation.subject_kind == "runner_build" {
        if ats_revocation_subject_key_sha256(&revocation.subject_id)? != revocation.subject_sha256 {
            return Ok(false);
        }
        return tx
            .query_opt(
                "SELECT 1 FROM jobs_ats_certification_runtime_targets runtime
                  JOIN jobs_ats_certification_manifests manifest
                    ON manifest.manifest_sha256 = runtime.manifest_sha256
                 WHERE runtime.runner_build_id = $1
                   AND manifest.trust_policy_sha256 = $2 LIMIT 1",
                &[&revocation.subject_id, &trust_policy_sha256],
            )
            .map(|value| value.is_some())
            .map_err(ats_certification_storage);
    }
    let (sql, id, sha256) = ats_revocation_subject_query(revocation, true)?;
    tx.query_opt(&sql, &[&id, &sha256, &trust_policy_sha256])
        .map(|value| value.is_some())
        .map_err(ats_certification_storage)
}

fn ats_revocation_subject_query(
    revocation: &AtsCertificationRevocationAuthority,
    postgres: bool,
) -> Result<(String, String, String), AtsCertificationAuthorityError> {
    let (first, second, policy) = if postgres {
        ("$1", "$2", "$3")
    } else {
        ("?1", "?2", "?3")
    };
    let sql = match revocation.subject_kind.as_str() {
        "activation" => format!(
            "SELECT 1 FROM jobs_ats_certification_activations
              WHERE activation_id = {first} AND activation_sha256 = {second}
                AND trust_policy_sha256 = {policy}"
        ),
        "adapter_bundle" => format!(
            "SELECT 1 FROM jobs_ats_certification_manifests
              WHERE adapter_version = {first} AND adapter_bundle_sha256 = {second}
                AND trust_policy_sha256 = {policy} LIMIT 1"
        ),
        "browser_release_manifest" if revocation.subject_id == revocation.subject_sha256 => {
            format!(
                "SELECT 1 FROM jobs_ats_certification_runtime_targets runtime
                  JOIN jobs_ats_certification_manifests manifest
                    ON manifest.manifest_sha256 = runtime.manifest_sha256
                 WHERE runtime.browser_release_manifest_sha256 = {first}
                   AND runtime.browser_release_manifest_sha256 = {second}
                   AND manifest.trust_policy_sha256 = {policy} LIMIT 1"
            )
        }
        "evidence" => format!(
            "SELECT 1 FROM jobs_ats_certification_evidence
              WHERE evidence_id = {first} AND evidence_sha256 = {second}
                AND trust_policy_sha256 = {policy}"
        ),
        "manifest" => format!(
            "SELECT 1 FROM jobs_ats_certification_manifests
              WHERE certification_id = {first} AND manifest_sha256 = {second}
                AND trust_policy_sha256 = {policy}"
        ),
        "layout_observation" => format!(
            "SELECT 1 FROM jobs_ats_certification_layout_observations
              WHERE observation_id = {first} AND observation_sha256 = {second}
                AND trust_policy_sha256 = {policy}"
        ),
        "policy" => format!(
            "SELECT 1 FROM jobs_ats_certification_trust_policies
              WHERE policy_id = {first} AND policy_sha256 = {second}
                AND policy_sha256 = {policy}"
        ),
        "runtime" => format!(
            "SELECT 1 FROM jobs_ats_certification_runtime_targets runtime
              JOIN jobs_ats_certification_manifests manifest
                ON manifest.manifest_sha256 = runtime.manifest_sha256
             WHERE runtime.runtime_id = {first} AND runtime.runtime_sha256 = {second}
               AND manifest.trust_policy_sha256 = {policy} LIMIT 1"
        ),
        "runner_image" if revocation.subject_id == revocation.subject_sha256 => format!(
            "SELECT 1 FROM jobs_ats_certification_runtime_targets runtime
              JOIN jobs_ats_certification_manifests manifest
                ON manifest.manifest_sha256 = runtime.manifest_sha256
             WHERE runtime.runner_image_sha256 = {first}
               AND runtime.runner_image_sha256 = {second}
               AND manifest.trust_policy_sha256 = {policy} LIMIT 1"
        ),
        "scope" if revocation.subject_id == revocation.subject_sha256 => format!(
            "SELECT 1 FROM jobs_ats_certification_manifests
              WHERE scope_sha256 = {first} AND scope_sha256 = {second}
                AND trust_policy_sha256 = {policy} LIMIT 1"
        ),
        _ => return Err(AtsCertificationAuthorityError::InvalidAuthority),
    };
    Ok((
        sql,
        revocation.subject_id.clone(),
        revocation.subject_sha256.clone(),
    ))
}

fn ats_revocation_trust_key_matches(
    public_key_base64url: Option<&str>,
    expected_sha256: &str,
) -> Result<bool, AtsCertificationAuthorityError> {
    let Some(public_key_base64url) = public_key_base64url else {
        return Ok(false);
    };
    let public_key = ats_certification_decode_base64url_exact(public_key_base64url, 32)
        .map_err(|_| AtsCertificationAuthorityError::InvalidTrustPolicy)?;
    Ok(ats_certification_sha256(&public_key) == expected_sha256)
}

fn ats_revocation_subject_key_sha256(
    subject_key: &str,
) -> Result<String, AtsCertificationAuthorityError> {
    if !ats_certification_text(subject_key, 1, 240) {
        return Err(AtsCertificationAuthorityError::InvalidAuthority);
    }
    Ok(ats_certification_sha256(subject_key.as_bytes()))
}

fn insert_sqlite_ats_revocation(
    tx: &rusqlite::Transaction<'_>,
    verified: &VerifiedAtsCertificationEnvelope<AtsCertificationRevocationAuthority>,
    envelope: &AtsCertificationAuthorityEnvelope,
    recorded_by: &str,
    recorded_at_ms: i64,
) -> Result<(), AtsCertificationAuthorityError> {
    let revocation = &verified.authority;
    tx.execute(
        "INSERT INTO jobs_ats_certification_revocations (
           revocation_sha256, revocation_id, revocation_generation,
           predecessor_revocation_sha256, subject_kind, subject_id, subject_sha256,
           reason_ref, canonical_revocation_base64url,
           authorization_sha256, trust_policy_sha256,
           canonical_authorization_base64url, issued_at_ms, effective_at_ms,
           recorded_by, recorded_at_ms
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
        params![
            verified.authority_sha256,
            revocation.revocation_id,
            revocation.revocation_generation,
            revocation.predecessor_revocation_sha256,
            revocation.subject_kind,
            revocation.subject_id,
            revocation.subject_sha256,
            revocation.reason_ref,
            envelope.canonical_base64url,
            verified.authorization_sha256,
            verified.trust_policy_sha256,
            envelope.authorization_base64url,
            revocation.issued_at_ms,
            revocation.effective_at_ms,
            recorded_by,
            recorded_at_ms,
        ],
    )
    .map_err(ats_certification_storage)?;
    Ok(())
}

fn insert_postgres_ats_revocation(
    tx: &mut postgres::Transaction<'_>,
    verified: &VerifiedAtsCertificationEnvelope<AtsCertificationRevocationAuthority>,
    envelope: &AtsCertificationAuthorityEnvelope,
    recorded_by: &str,
    recorded_at_ms: i64,
) -> Result<(), AtsCertificationAuthorityError> {
    let revocation = &verified.authority;
    tx.execute(
        "INSERT INTO jobs_ats_certification_revocations (
           revocation_sha256, revocation_id, revocation_generation,
           predecessor_revocation_sha256, subject_kind, subject_id, subject_sha256,
           reason_ref, canonical_revocation_base64url,
           authorization_sha256, trust_policy_sha256,
           canonical_authorization_base64url, issued_at_ms, effective_at_ms,
           recorded_by, recorded_at_ms
         ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16)",
        &[
            &verified.authority_sha256,
            &revocation.revocation_id,
            &revocation.revocation_generation,
            &revocation.predecessor_revocation_sha256,
            &revocation.subject_kind,
            &revocation.subject_id,
            &revocation.subject_sha256,
            &revocation.reason_ref,
            &envelope.canonical_base64url,
            &verified.authorization_sha256,
            &verified.trust_policy_sha256,
            &envelope.authorization_base64url,
            &revocation.issued_at_ms,
            &revocation.effective_at_ms,
            &recorded_by,
            &recorded_at_ms,
        ],
    )
    .map_err(ats_certification_storage)?;
    Ok(())
}

#[derive(Debug, Clone)]
struct StoredAtsQuarantineCommand {
    command_sha256: String,
    scope_kind: String,
    scope_id: String,
    scope_sha256: String,
    command_sequence: i64,
    predecessor_command_sha256: Option<String>,
    action: String,
    trust_policy_sha256: String,
}

#[derive(Debug, Clone)]
struct StoredAtsQuarantineHead {
    head_revision: i64,
    command_sha256: String,
    command_sequence: i64,
    state: String,
    updated_by: String,
}

pub fn import_ats_certification_quarantine(
    pool: &DbPool,
    envelope: &AtsCertificationAuthorityEnvelope,
    recorded_by: &str,
) -> Result<AtsCertificationImportResult, AtsCertificationAuthorityError> {
    import_ats_certification_quarantine_at(pool, envelope, recorded_by, now_ms())
}

fn import_ats_certification_quarantine_at(
    pool: &DbPool,
    envelope: &AtsCertificationAuthorityEnvelope,
    recorded_by: &str,
    recorded_at_ms: i64,
) -> Result<AtsCertificationImportResult, AtsCertificationAuthorityError> {
    validate_ats_recorded_by(recorded_by)?;
    let (trust_policy_sha256, trust_policy) =
        load_current_ats_certification_trust_policy(pool, recorded_at_ms)?;
    let mut verified =
        verify_ats_quarantine_envelope(envelope, &trust_policy.delegated_trust, recorded_at_ms)?;
    if verified.authority.policy_sha256 != trust_policy_sha256 {
        return Err(AtsCertificationAuthorityError::InvalidTrustPolicy);
    }
    require_ats_authorization_clock_skew(
        envelope,
        verified.authority.issued_at_ms,
        trust_policy
            .certification_requirements
            .maximum_clock_skew_ms,
    )?;
    verified.trust_policy_sha256 = trust_policy_sha256;
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get().map_err(ats_certification_storage)?;
            let tx = conn
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(ats_certification_storage)?;
            require_current_sqlite_ats_trust_policy(
                &tx,
                &verified.trust_policy_sha256,
                recorded_at_ms,
            )?;
            if let Some(existing) = sqlite_ats_quarantine_identity(&tx, &verified)? {
                require_exact_ats_authority_replay(
                    existing,
                    &verified,
                    &envelope.canonical_base64url,
                )?;
                tx.commit().map_err(ats_certification_storage)?;
                return Ok(ats_certification_import_result(
                    "quarantine",
                    &verified.authority.command_id,
                    &verified,
                    true,
                ));
            }
            require_sqlite_ats_quarantine_scope(
                &tx,
                &verified.authority,
                &verified.trust_policy_sha256,
            )?;
            require_sqlite_ats_quarantine_predecessor(
                &tx,
                &verified.authority,
                &verified.trust_policy_sha256,
            )?;
            insert_sqlite_ats_quarantine(&tx, &verified, envelope, recorded_by, recorded_at_ms)?;
            tx.commit().map_err(ats_certification_storage)?;
            Ok(ats_certification_import_result(
                "quarantine",
                &verified.authority.command_id,
                &verified,
                false,
            ))
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg().map_err(ats_certification_storage)?;
            let mut tx = conn.transaction().map_err(ats_certification_storage)?;
            lock_postgres_ats_certification(&mut tx)?;
            require_current_postgres_ats_trust_policy(
                &mut tx,
                &verified.trust_policy_sha256,
                recorded_at_ms,
            )?;
            if let Some(existing) = postgres_ats_quarantine_identity(&mut tx, &verified)? {
                require_exact_ats_authority_replay(
                    existing,
                    &verified,
                    &envelope.canonical_base64url,
                )?;
                tx.commit().map_err(ats_certification_storage)?;
                return Ok(ats_certification_import_result(
                    "quarantine",
                    &verified.authority.command_id,
                    &verified,
                    true,
                ));
            }
            require_postgres_ats_quarantine_scope(
                &mut tx,
                &verified.authority,
                &verified.trust_policy_sha256,
            )?;
            require_postgres_ats_quarantine_predecessor(
                &mut tx,
                &verified.authority,
                &verified.trust_policy_sha256,
            )?;
            insert_postgres_ats_quarantine(
                &mut tx,
                &verified,
                envelope,
                recorded_by,
                recorded_at_ms,
            )?;
            tx.commit().map_err(ats_certification_storage)?;
            Ok(ats_certification_import_result(
                "quarantine",
                &verified.authority.command_id,
                &verified,
                false,
            ))
        }
    })
}

fn sqlite_ats_quarantine_identity(
    tx: &rusqlite::Transaction<'_>,
    verified: &VerifiedAtsCertificationEnvelope<AtsCertificationQuarantineAuthority>,
) -> Result<Option<(String, String, String, String)>, AtsCertificationAuthorityError> {
    tx.query_row(
        "SELECT command_sha256, canonical_command_base64url, authorization_sha256,
                trust_policy_sha256
           FROM jobs_ats_certification_quarantine_commands
          WHERE command_id = ?1 OR command_sha256 = ?2
             OR (scope_kind = ?3 AND scope_id = ?4 AND scope_sha256 = ?5
                 AND command_sequence = ?6)",
        params![
            verified.authority.command_id,
            verified.authority_sha256,
            verified.authority.scope_kind,
            verified.authority.scope_id,
            verified.authority.scope_sha256,
            verified.authority.command_sequence,
        ],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
    )
    .optional()
    .map_err(ats_certification_storage)
}

fn postgres_ats_quarantine_identity(
    tx: &mut postgres::Transaction<'_>,
    verified: &VerifiedAtsCertificationEnvelope<AtsCertificationQuarantineAuthority>,
) -> Result<Option<(String, String, String, String)>, AtsCertificationAuthorityError> {
    Ok(tx
        .query_opt(
            "SELECT command_sha256, canonical_command_base64url, authorization_sha256,
                    trust_policy_sha256
               FROM jobs_ats_certification_quarantine_commands
              WHERE command_id = $1 OR command_sha256 = $2
                 OR (scope_kind = $3 AND scope_id = $4 AND scope_sha256 = $5
                     AND command_sequence = $6)",
            &[
                &verified.authority.command_id,
                &verified.authority_sha256,
                &verified.authority.scope_kind,
                &verified.authority.scope_id,
                &verified.authority.scope_sha256,
                &verified.authority.command_sequence,
            ],
        )
        .map_err(ats_certification_storage)?
        .map(|row| (row.get(0), row.get(1), row.get(2), row.get(3))))
}

fn require_sqlite_ats_quarantine_scope(
    tx: &rusqlite::Transaction<'_>,
    command: &AtsCertificationQuarantineAuthority,
    trust_policy_sha256: &str,
) -> Result<(), AtsCertificationAuthorityError> {
    let (sql, id, sha256) = ats_quarantine_scope_query(command, false)?;
    if tx
        .query_row(&sql, params![id, sha256, trust_policy_sha256], |_| Ok(()))
        .optional()
        .map_err(ats_certification_storage)?
        .is_none()
    {
        return Err(AtsCertificationAuthorityError::NotFound);
    }
    Ok(())
}

fn require_postgres_ats_quarantine_scope(
    tx: &mut postgres::Transaction<'_>,
    command: &AtsCertificationQuarantineAuthority,
    trust_policy_sha256: &str,
) -> Result<(), AtsCertificationAuthorityError> {
    let (sql, id, sha256) = ats_quarantine_scope_query(command, true)?;
    if tx
        .query_opt(&sql, &[&id, &sha256, &trust_policy_sha256])
        .map_err(ats_certification_storage)?
        .is_none()
    {
        return Err(AtsCertificationAuthorityError::NotFound);
    }
    Ok(())
}

fn ats_quarantine_scope_query(
    command: &AtsCertificationQuarantineAuthority,
    postgres: bool,
) -> Result<(String, String, String), AtsCertificationAuthorityError> {
    let (first, second, policy) = if postgres {
        ("$1", "$2", "$3")
    } else {
        ("?1", "?2", "?3")
    };
    let sql = match command.scope_kind.as_str() {
        "provider"
            if command.scope_sha256
                == ats_named_quarantine_scope_sha256("provider", &command.scope_id)? =>
        {
            format!(
                "SELECT 1 FROM jobs_ats_certification_manifests
                  WHERE provider = {first} AND {second} = {second}
                    AND trust_policy_sha256 = {policy} LIMIT 1"
            )
        }
        "target"
            if command.scope_sha256
                == ats_named_quarantine_scope_sha256("target", &command.scope_id)? =>
        {
            format!(
                "SELECT 1 FROM jobs_ats_certification_manifests
                  WHERE target_key = {first} AND {second} = {second}
                    AND trust_policy_sha256 = {policy} LIMIT 1"
            )
        }
        "surface" => format!(
            "SELECT 1 FROM jobs_ats_certification_manifests
              WHERE variant_key = {first} AND scope_sha256 = {second}
                AND trust_policy_sha256 = {policy} LIMIT 1"
        ),
        "adapter" => format!(
            "SELECT 1 FROM jobs_ats_certification_manifests
              WHERE adapter_version = {first} AND adapter_bundle_sha256 = {second}
                AND trust_policy_sha256 = {policy} LIMIT 1"
        ),
        "runtime" => format!(
            "SELECT 1 FROM jobs_ats_certification_runtime_targets runtime
              JOIN jobs_ats_certification_manifests manifest
                ON manifest.manifest_sha256 = runtime.manifest_sha256
             WHERE runtime.runtime_id = {first} AND runtime.runtime_sha256 = {second}
               AND manifest.trust_policy_sha256 = {policy} LIMIT 1"
        ),
        "activation" => format!(
            "SELECT 1 FROM jobs_ats_certification_activations
              WHERE activation_id = {first} AND activation_sha256 = {second}
                AND trust_policy_sha256 = {policy}"
        ),
        _ => return Err(AtsCertificationAuthorityError::InvalidAuthority),
    };
    Ok((sql, command.scope_id.clone(), command.scope_sha256.clone()))
}

fn require_ats_quarantine_predecessor(
    command: &AtsCertificationQuarantineAuthority,
    predecessor: Option<(String, i64, String, String, String)>,
) -> Result<(), AtsCertificationAuthorityError> {
    match predecessor {
        None if command.command_sequence == 1
            && command.predecessor_command_sha256.is_none()
            && command.action == "quarantine" =>
        {
            Ok(())
        }
        Some((sha256, sequence, kind, id, scope_sha256))
            if command.command_sequence == sequence + 1
                && command.predecessor_command_sha256.as_deref() == Some(sha256.as_str())
                && command.scope_kind == kind
                && command.scope_id == id
                && command.scope_sha256 == scope_sha256 =>
        {
            Ok(())
        }
        _ => Err(AtsCertificationAuthorityError::SequenceRegression),
    }
}

fn require_sqlite_ats_quarantine_predecessor(
    tx: &rusqlite::Transaction<'_>,
    command: &AtsCertificationQuarantineAuthority,
    trust_policy_sha256: &str,
) -> Result<(), AtsCertificationAuthorityError> {
    let predecessor = command
        .predecessor_command_sha256
        .as_deref()
        .map(|sha256| {
            tx.query_row(
                "SELECT command_sha256, command_sequence, scope_kind, scope_id, scope_sha256
                   FROM jobs_ats_certification_quarantine_commands
                  WHERE command_sha256 = ?1 AND trust_policy_sha256 = ?2",
                params![sha256, trust_policy_sha256],
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
            .optional()
            .map_err(ats_certification_storage)
        })
        .transpose()?
        .flatten();
    require_ats_quarantine_predecessor(command, predecessor)
}

fn require_postgres_ats_quarantine_predecessor(
    tx: &mut postgres::Transaction<'_>,
    command: &AtsCertificationQuarantineAuthority,
    trust_policy_sha256: &str,
) -> Result<(), AtsCertificationAuthorityError> {
    let predecessor = match command.predecessor_command_sha256.as_deref() {
        Some(sha256) => tx
            .query_opt(
                "SELECT command_sha256, command_sequence, scope_kind, scope_id, scope_sha256
                   FROM jobs_ats_certification_quarantine_commands
                  WHERE command_sha256 = $1 AND trust_policy_sha256 = $2",
                &[&sha256, &trust_policy_sha256],
            )
            .map_err(ats_certification_storage)?
            .map(|row| (row.get(0), row.get(1), row.get(2), row.get(3), row.get(4))),
        None => None,
    };
    require_ats_quarantine_predecessor(command, predecessor)
}

fn insert_sqlite_ats_quarantine(
    tx: &rusqlite::Transaction<'_>,
    verified: &VerifiedAtsCertificationEnvelope<AtsCertificationQuarantineAuthority>,
    envelope: &AtsCertificationAuthorityEnvelope,
    recorded_by: &str,
    recorded_at_ms: i64,
) -> Result<(), AtsCertificationAuthorityError> {
    let command = &verified.authority;
    tx.execute(
        "INSERT INTO jobs_ats_certification_quarantine_commands (
           command_sha256, command_id, command_generation, scope_kind, scope_id,
           scope_sha256, command_sequence, predecessor_command_sha256, action,
           reason_ref, canonical_command_base64url, authorization_sha256,
           trust_policy_sha256, canonical_authorization_base64url, issued_at_ms,
           recorded_by, recorded_at_ms
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14,
                   ?15, ?16, ?17)",
        params![
            verified.authority_sha256,
            command.command_id,
            command.command_generation,
            command.scope_kind,
            command.scope_id,
            command.scope_sha256,
            command.command_sequence,
            command.predecessor_command_sha256,
            command.action,
            command.reason_ref,
            envelope.canonical_base64url,
            verified.authorization_sha256,
            verified.trust_policy_sha256,
            envelope.authorization_base64url,
            command.issued_at_ms,
            recorded_by,
            recorded_at_ms,
        ],
    )
    .map_err(ats_certification_storage)?;
    Ok(())
}

fn insert_postgres_ats_quarantine(
    tx: &mut postgres::Transaction<'_>,
    verified: &VerifiedAtsCertificationEnvelope<AtsCertificationQuarantineAuthority>,
    envelope: &AtsCertificationAuthorityEnvelope,
    recorded_by: &str,
    recorded_at_ms: i64,
) -> Result<(), AtsCertificationAuthorityError> {
    let command = &verified.authority;
    tx.execute(
        "INSERT INTO jobs_ats_certification_quarantine_commands (
           command_sha256, command_id, command_generation, scope_kind, scope_id,
           scope_sha256, command_sequence, predecessor_command_sha256, action,
           reason_ref, canonical_command_base64url, authorization_sha256,
           trust_policy_sha256, canonical_authorization_base64url, issued_at_ms,
           recorded_by, recorded_at_ms
         ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14,
                   $15, $16, $17)",
        &[
            &verified.authority_sha256,
            &command.command_id,
            &command.command_generation,
            &command.scope_kind,
            &command.scope_id,
            &command.scope_sha256,
            &command.command_sequence,
            &command.predecessor_command_sha256,
            &command.action,
            &command.reason_ref,
            &envelope.canonical_base64url,
            &verified.authorization_sha256,
            &verified.trust_policy_sha256,
            &envelope.authorization_base64url,
            &command.issued_at_ms,
            &recorded_by,
            &recorded_at_ms,
        ],
    )
    .map_err(ats_certification_storage)?;
    Ok(())
}

pub fn apply_ats_certification_quarantine(
    pool: &DbPool,
    command_sha256: &str,
    expected_head_revision: i64,
    expected_command_sha256: Option<&str>,
    recorded_by: &str,
) -> Result<AtsCertificationQuarantineHeadResult, AtsCertificationAuthorityError> {
    apply_ats_certification_quarantine_at(
        pool,
        command_sha256,
        expected_head_revision,
        expected_command_sha256,
        recorded_by,
        now_ms(),
    )
}

fn apply_ats_certification_quarantine_at(
    pool: &DbPool,
    command_sha256: &str,
    expected_head_revision: i64,
    expected_command_sha256: Option<&str>,
    recorded_by: &str,
    recorded_at_ms: i64,
) -> Result<AtsCertificationQuarantineHeadResult, AtsCertificationAuthorityError> {
    validate_ats_recorded_by(recorded_by)?;
    if !ats_certification_hex64(command_sha256)
        || !ats_certification_safe_integer(expected_head_revision, false)
        || expected_command_sha256.is_some_and(|value| !ats_certification_hex64(value))
        || (expected_head_revision == 0) != expected_command_sha256.is_none()
    {
        return Err(AtsCertificationAuthorityError::InvalidAuthority);
    }
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get().map_err(ats_certification_storage)?;
            let tx = conn
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(ats_certification_storage)?;
            let command = sqlite_ats_quarantine_command(&tx, command_sha256)?
                .ok_or(AtsCertificationAuthorityError::NotFound)?;
            require_current_sqlite_ats_trust_policy(
                &tx,
                &command.trust_policy_sha256,
                recorded_at_ms,
            )?;
            let current = sqlite_ats_quarantine_head(&tx, &command)?;
            if let Some(result) = ats_quarantine_replay(
                current.as_ref(),
                &command,
                expected_head_revision,
                expected_command_sha256,
                recorded_by,
            )? {
                tx.commit().map_err(ats_certification_storage)?;
                return Ok(result);
            }
            require_expected_ats_quarantine_head(
                current.as_ref(),
                &command,
                expected_head_revision,
                expected_command_sha256,
            )?;
            let result = apply_sqlite_ats_quarantine_head(
                &tx,
                &command,
                current.as_ref(),
                recorded_by,
                recorded_at_ms,
            )?;
            tx.commit().map_err(ats_certification_storage)?;
            Ok(result)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg().map_err(ats_certification_storage)?;
            let mut tx = conn.transaction().map_err(ats_certification_storage)?;
            lock_postgres_ats_certification(&mut tx)?;
            let command = postgres_ats_quarantine_command(&mut tx, command_sha256)?
                .ok_or(AtsCertificationAuthorityError::NotFound)?;
            require_current_postgres_ats_trust_policy(
                &mut tx,
                &command.trust_policy_sha256,
                recorded_at_ms,
            )?;
            let current = postgres_ats_quarantine_head(&mut tx, &command)?;
            if let Some(result) = ats_quarantine_replay(
                current.as_ref(),
                &command,
                expected_head_revision,
                expected_command_sha256,
                recorded_by,
            )? {
                tx.commit().map_err(ats_certification_storage)?;
                return Ok(result);
            }
            require_expected_ats_quarantine_head(
                current.as_ref(),
                &command,
                expected_head_revision,
                expected_command_sha256,
            )?;
            let result = apply_postgres_ats_quarantine_head(
                &mut tx,
                &command,
                current.as_ref(),
                recorded_by,
                recorded_at_ms,
            )?;
            tx.commit().map_err(ats_certification_storage)?;
            Ok(result)
        }
    })
}

fn sqlite_ats_quarantine_command(
    tx: &rusqlite::Transaction<'_>,
    command_sha256: &str,
) -> Result<Option<StoredAtsQuarantineCommand>, AtsCertificationAuthorityError> {
    tx.query_row(
        "SELECT command_sha256, scope_kind, scope_id, scope_sha256,
                command_sequence, predecessor_command_sha256, action, trust_policy_sha256
           FROM jobs_ats_certification_quarantine_commands WHERE command_sha256 = ?1",
        params![command_sha256],
        |row| {
            Ok(StoredAtsQuarantineCommand {
                command_sha256: row.get(0)?,
                scope_kind: row.get(1)?,
                scope_id: row.get(2)?,
                scope_sha256: row.get(3)?,
                command_sequence: row.get(4)?,
                predecessor_command_sha256: row.get(5)?,
                action: row.get(6)?,
                trust_policy_sha256: row.get(7)?,
            })
        },
    )
    .optional()
    .map_err(ats_certification_storage)
}

fn postgres_ats_quarantine_command(
    tx: &mut postgres::Transaction<'_>,
    command_sha256: &str,
) -> Result<Option<StoredAtsQuarantineCommand>, AtsCertificationAuthorityError> {
    Ok(tx
        .query_opt(
            "SELECT command_sha256, scope_kind, scope_id, scope_sha256,
                    command_sequence, predecessor_command_sha256, action, trust_policy_sha256
               FROM jobs_ats_certification_quarantine_commands WHERE command_sha256 = $1",
            &[&command_sha256],
        )
        .map_err(ats_certification_storage)?
        .map(|row| StoredAtsQuarantineCommand {
            command_sha256: row.get(0),
            scope_kind: row.get(1),
            scope_id: row.get(2),
            scope_sha256: row.get(3),
            command_sequence: row.get(4),
            predecessor_command_sha256: row.get(5),
            action: row.get(6),
            trust_policy_sha256: row.get(7),
        }))
}

fn sqlite_ats_quarantine_head(
    tx: &rusqlite::Transaction<'_>,
    command: &StoredAtsQuarantineCommand,
) -> Result<Option<StoredAtsQuarantineHead>, AtsCertificationAuthorityError> {
    tx.query_row(
        "SELECT head_revision, current_command_sha256, current_command_sequence,
                state, updated_by
           FROM jobs_ats_certification_quarantine_heads
          WHERE scope_kind = ?1 AND scope_id = ?2 AND scope_sha256 = ?3",
        params![command.scope_kind, command.scope_id, command.scope_sha256],
        |row| {
            Ok(StoredAtsQuarantineHead {
                head_revision: row.get(0)?,
                command_sha256: row.get(1)?,
                command_sequence: row.get(2)?,
                state: row.get(3)?,
                updated_by: row.get(4)?,
            })
        },
    )
    .optional()
    .map_err(ats_certification_storage)
}

fn postgres_ats_quarantine_head(
    tx: &mut postgres::Transaction<'_>,
    command: &StoredAtsQuarantineCommand,
) -> Result<Option<StoredAtsQuarantineHead>, AtsCertificationAuthorityError> {
    Ok(tx
        .query_opt(
            "SELECT head_revision, current_command_sha256, current_command_sequence,
                    state, updated_by
               FROM jobs_ats_certification_quarantine_heads
              WHERE scope_kind = $1 AND scope_id = $2 AND scope_sha256 = $3",
            &[
                &command.scope_kind,
                &command.scope_id,
                &command.scope_sha256,
            ],
        )
        .map_err(ats_certification_storage)?
        .map(|row| StoredAtsQuarantineHead {
            head_revision: row.get(0),
            command_sha256: row.get(1),
            command_sequence: row.get(2),
            state: row.get(3),
            updated_by: row.get(4),
        }))
}

fn ats_quarantine_replay(
    current: Option<&StoredAtsQuarantineHead>,
    command: &StoredAtsQuarantineCommand,
    expected_head_revision: i64,
    expected_command_sha256: Option<&str>,
    recorded_by: &str,
) -> Result<Option<AtsCertificationQuarantineHeadResult>, AtsCertificationAuthorityError> {
    let Some(current) = current else {
        return Ok(None);
    };
    if current.command_sha256 != command.command_sha256 {
        return Ok(None);
    }
    if current.head_revision != expected_head_revision + 1
        || current.updated_by != recorded_by
        || current.state != ats_quarantine_state(&command.action)?
        || current.command_sequence != command.command_sequence
        || expected_command_sha256 != command.predecessor_command_sha256.as_deref()
    {
        return Err(AtsCertificationAuthorityError::CompareAndSwapConflict);
    }
    Ok(Some(ats_quarantine_head_result(
        command,
        current.head_revision,
        true,
    )?))
}

fn require_expected_ats_quarantine_head(
    current: Option<&StoredAtsQuarantineHead>,
    command: &StoredAtsQuarantineCommand,
    expected_head_revision: i64,
    expected_command_sha256: Option<&str>,
) -> Result<(), AtsCertificationAuthorityError> {
    match current {
        None if expected_head_revision == 0
            && expected_command_sha256.is_none()
            && command.command_sequence == 1
            && command.predecessor_command_sha256.is_none() =>
        {
            Ok(())
        }
        Some(current)
            if current.head_revision == expected_head_revision
                && expected_command_sha256 == Some(current.command_sha256.as_str())
                && command.predecessor_command_sha256.as_deref()
                    == Some(current.command_sha256.as_str())
                && command.command_sequence == current.command_sequence + 1 =>
        {
            Ok(())
        }
        Some(current) if command.command_sequence <= current.command_sequence => {
            Err(AtsCertificationAuthorityError::SequenceRegression)
        }
        _ => Err(AtsCertificationAuthorityError::CompareAndSwapConflict),
    }
}

fn ats_quarantine_state(action: &str) -> Result<&'static str, AtsCertificationAuthorityError> {
    match action {
        "quarantine" => Ok("quarantined"),
        "release" => Ok("released"),
        _ => Err(AtsCertificationAuthorityError::InvalidAuthority),
    }
}

fn ats_quarantine_head_result(
    command: &StoredAtsQuarantineCommand,
    head_revision: i64,
    replayed: bool,
) -> Result<AtsCertificationQuarantineHeadResult, AtsCertificationAuthorityError> {
    Ok(AtsCertificationQuarantineHeadResult {
        scope_kind: command.scope_kind.clone(),
        scope_id: command.scope_id.clone(),
        scope_sha256: command.scope_sha256.clone(),
        head_revision,
        command_sha256: command.command_sha256.clone(),
        command_sequence: command.command_sequence,
        state: ats_quarantine_state(&command.action)?.to_string(),
        replayed,
    })
}

fn apply_sqlite_ats_quarantine_head(
    tx: &rusqlite::Transaction<'_>,
    command: &StoredAtsQuarantineCommand,
    current: Option<&StoredAtsQuarantineHead>,
    recorded_by: &str,
    recorded_at_ms: i64,
) -> Result<AtsCertificationQuarantineHeadResult, AtsCertificationAuthorityError> {
    let state = ats_quarantine_state(&command.action)?;
    let head_revision = current.map_or(1, |head| head.head_revision + 1);
    if let Some(current) = current {
        let changed = tx
            .execute(
                "UPDATE jobs_ats_certification_quarantine_heads SET
                   head_revision = ?1, current_command_sha256 = ?2,
                   current_command_sequence = ?3, state = ?4, updated_by = ?5,
                   updated_at_ms = ?6
                 WHERE scope_kind = ?7 AND scope_id = ?8 AND scope_sha256 = ?9
                   AND head_revision = ?10 AND current_command_sha256 = ?11",
                params![
                    head_revision,
                    command.command_sha256,
                    command.command_sequence,
                    state,
                    recorded_by,
                    recorded_at_ms,
                    command.scope_kind,
                    command.scope_id,
                    command.scope_sha256,
                    current.head_revision,
                    current.command_sha256,
                ],
            )
            .map_err(ats_certification_storage)?;
        if changed != 1 {
            return Err(AtsCertificationAuthorityError::CompareAndSwapConflict);
        }
    } else {
        tx.execute(
            "INSERT INTO jobs_ats_certification_quarantine_heads (
               scope_kind, scope_id, scope_sha256, head_revision,
               current_command_sha256, current_command_sequence, state, updated_by,
               updated_at_ms
             ) VALUES (?1, ?2, ?3, 1, ?4, ?5, ?6, ?7, ?8)",
            params![
                command.scope_kind,
                command.scope_id,
                command.scope_sha256,
                command.command_sha256,
                command.command_sequence,
                state,
                recorded_by,
                recorded_at_ms,
            ],
        )
        .map_err(ats_certification_storage)?;
    }
    ats_quarantine_head_result(command, head_revision, false)
}

fn apply_postgres_ats_quarantine_head(
    tx: &mut postgres::Transaction<'_>,
    command: &StoredAtsQuarantineCommand,
    current: Option<&StoredAtsQuarantineHead>,
    recorded_by: &str,
    recorded_at_ms: i64,
) -> Result<AtsCertificationQuarantineHeadResult, AtsCertificationAuthorityError> {
    let state = ats_quarantine_state(&command.action)?;
    let head_revision = current.map_or(1, |head| head.head_revision + 1);
    if let Some(current) = current {
        let changed = tx
            .execute(
                "UPDATE jobs_ats_certification_quarantine_heads SET
                   head_revision = $1, current_command_sha256 = $2,
                   current_command_sequence = $3, state = $4, updated_by = $5,
                   updated_at_ms = $6
                 WHERE scope_kind = $7 AND scope_id = $8 AND scope_sha256 = $9
                   AND head_revision = $10 AND current_command_sha256 = $11",
                &[
                    &head_revision,
                    &command.command_sha256,
                    &command.command_sequence,
                    &state,
                    &recorded_by,
                    &recorded_at_ms,
                    &command.scope_kind,
                    &command.scope_id,
                    &command.scope_sha256,
                    &current.head_revision,
                    &current.command_sha256,
                ],
            )
            .map_err(ats_certification_storage)?;
        if changed != 1 {
            return Err(AtsCertificationAuthorityError::CompareAndSwapConflict);
        }
    } else {
        tx.execute(
            "INSERT INTO jobs_ats_certification_quarantine_heads (
               scope_kind, scope_id, scope_sha256, head_revision,
               current_command_sha256, current_command_sequence, state, updated_by,
               updated_at_ms
             ) VALUES ($1, $2, $3, 1, $4, $5, $6, $7, $8)",
            &[
                &command.scope_kind,
                &command.scope_id,
                &command.scope_sha256,
                &command.command_sha256,
                &command.command_sequence,
                &state,
                &recorded_by,
                &recorded_at_ms,
            ],
        )
        .map_err(ats_certification_storage)?;
    }
    ats_quarantine_head_result(command, head_revision, false)
}

fn validate_ats_certification_circuit_event(
    event: &AtsCertificationCircuitEvent,
    recorded_at_ms: i64,
) -> Result<(), AtsCertificationAuthorityError> {
    let scope_valid = matches!(
        event.scope_kind.as_str(),
        "activation" | "adapter" | "provider" | "runtime" | "target"
    );
    let trigger_valid = matches!(
        event.trigger_kind.as_str(),
        "confirmation_ambiguity"
            | "error_threshold"
            | "evidence_failure"
            | "false_state_risk"
            | "layout_drift"
            | "newer_activation"
            | "reviewed_close"
            | "side_effect_unknown"
    );
    let transition_valid = match event.transition.as_str() {
        "closed" => matches!(
            event.trigger_kind.as_str(),
            "newer_activation" | "reviewed_close"
        ),
        "opened" | "held" => !matches!(
            event.trigger_kind.as_str(),
            "newer_activation" | "reviewed_close"
        ),
        _ => false,
    };
    if !ats_certification_token(&event.event_id, 1, 120)
        || !scope_valid
        || !ats_certification_text(&event.subject_key, 1, 240)
        || !trigger_valid
        || !transition_valid
        || !ats_certification_safe_integer(event.window_started_at_ms, false)
        || event.window_ended_at_ms < event.window_started_at_ms
        || event.event_at_ms < event.window_ended_at_ms
        || event.event_at_ms > recorded_at_ms
        || !ats_certification_safe_integer(recorded_at_ms, false)
        || event.failure_count < 0
        || event.sample_count < event.failure_count
        || event.threshold_count < 0
        || !ats_certification_safe_integer(event.sample_count, false)
        || !ats_certification_text(&event.authority_ref, 1, 240)
    {
        return Err(AtsCertificationAuthorityError::InvalidAuthority);
    }
    Ok(())
}

fn ats_certification_circuit_event_identity(
    event: &AtsCertificationCircuitEvent,
) -> Result<(String, String), AtsCertificationAuthorityError> {
    let canonical = ats_certification_canonical_json(event)
        .map_err(|_| AtsCertificationAuthorityError::InvalidAuthority)?;
    Ok((
        ats_certification_sha256(&canonical),
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(canonical),
    ))
}

fn require_ats_circuit_transition(
    current: Option<(&str, i64, i64)>,
    event: &AtsCertificationCircuitEvent,
    expected_head_revision: i64,
    expected_event_id: Option<&str>,
) -> Result<(), AtsCertificationAuthorityError> {
    match current {
        None if expected_head_revision == 0
            && expected_event_id.is_none()
            && event.transition == "opened" =>
        {
            Ok(())
        }
        Some((state, revision, previous_event_at_ms))
            if revision == expected_head_revision
                && expected_event_id.is_some()
                && event.event_at_ms > previous_event_at_ms
                && matches!(
                    (state, event.transition.as_str()),
                    ("closed", "opened")
                        | ("opened", "held")
                        | ("opened", "closed")
                        | ("held", "held")
                        | ("held", "closed")
                ) =>
        {
            Ok(())
        }
        _ => Err(AtsCertificationAuthorityError::CompareAndSwapConflict),
    }
}

fn require_sqlite_ats_newer_activation_circuit_close(
    tx: &rusqlite::Transaction<'_>,
    event: &AtsCertificationCircuitEvent,
    previous_event_at_ms: i64,
    recorded_at_ms: i64,
) -> Result<(), AtsCertificationAuthorityError> {
    if event.trigger_kind != "newer_activation" {
        return Ok(());
    }
    if !ats_certification_hex64(&event.authority_ref) {
        return Err(AtsCertificationAuthorityError::InvalidAuthority);
    }
    if event.scope_kind != "activation" {
        return Err(AtsCertificationAuthorityError::InvalidAuthority);
    }
    let activation = sqlite_ats_activation_for_head(tx, &event.authority_ref)?
        .ok_or(AtsCertificationAuthorityError::InvalidAuthority)?;
    require_current_sqlite_ats_trust_policy(tx, &activation.trust_policy_sha256, recorded_at_ms)?;
    ensure_sqlite_ats_activation_available(tx, &activation, recorded_at_ms)?;
    require_sqlite_ats_stored_activation_canary_allowlist(tx, &activation, recorded_at_ms)?;
    let runtimes = sqlite_ats_runtime_targets(tx, &activation.manifest_sha256)?;
    let available = sqlite_available_ats_runtimes(
        tx,
        runtimes.clone(),
        &activation.trust_policy_sha256,
        recorded_at_ms,
        false,
    )?;
    require_ats_newer_activation_runtime_authority(&runtimes, &available)?;
    let verified: i64 = tx
        .query_row(
            "SELECT EXISTS(
               SELECT 1
                 FROM jobs_ats_certification_heads head
                 JOIN jobs_ats_certification_head_transitions transition
                   ON transition.transition_sha256 = head.current_transition_sha256
                  AND transition.scope_sha256 = head.scope_sha256
                  AND transition.channel = head.channel
                  AND transition.head_revision = head.head_revision
                  AND transition.next_activation_sha256 = head.current_activation_sha256
                 JOIN jobs_ats_certification_activations activation
                   ON activation.activation_sha256 = head.current_activation_sha256
                 JOIN jobs_ats_certification_activations predecessor
                   ON predecessor.activation_sha256 = transition.previous_activation_sha256
                WHERE activation.activation_sha256 = ?1
                  AND activation.predecessor_activation_sha256 = predecessor.activation_sha256
                  AND activation.scope_sha256 = predecessor.scope_sha256
                  AND activation.channel = predecessor.channel
                  AND activation.activation_generation > predecessor.activation_generation
                  AND activation.channel_sequence > predecessor.channel_sequence
                  AND transition.scope_sha256 = activation.scope_sha256
                  AND transition.channel = activation.channel
                  AND transition.recorded_at_ms > ?2
                  AND transition.recorded_at_ms <= ?3
                  AND activation.not_before_ms <= ?3
                  AND activation.expires_at_ms > ?3
                  AND ?4 = 'activation'
                  AND transition.previous_activation_sha256 = ?5
             )",
            params![
                event.authority_ref,
                previous_event_at_ms,
                event.event_at_ms,
                event.scope_kind,
                event.subject_key,
            ],
            |row| row.get(0),
        )
        .map_err(ats_certification_storage)?;
    if verified == 0 {
        return Err(AtsCertificationAuthorityError::InvalidAuthority);
    }
    Ok(())
}

fn require_postgres_ats_newer_activation_circuit_close(
    tx: &mut postgres::Transaction<'_>,
    event: &AtsCertificationCircuitEvent,
    previous_event_at_ms: i64,
    recorded_at_ms: i64,
) -> Result<(), AtsCertificationAuthorityError> {
    if event.trigger_kind != "newer_activation" {
        return Ok(());
    }
    if !ats_certification_hex64(&event.authority_ref) {
        return Err(AtsCertificationAuthorityError::InvalidAuthority);
    }
    if event.scope_kind != "activation" {
        return Err(AtsCertificationAuthorityError::InvalidAuthority);
    }
    let activation = postgres_ats_activation_for_head(tx, &event.authority_ref)?
        .ok_or(AtsCertificationAuthorityError::InvalidAuthority)?;
    require_current_postgres_ats_trust_policy(tx, &activation.trust_policy_sha256, recorded_at_ms)?;
    ensure_postgres_ats_activation_available(tx, &activation, recorded_at_ms)?;
    require_postgres_ats_stored_activation_canary_allowlist(tx, &activation, recorded_at_ms)?;
    let runtimes = postgres_ats_runtime_targets(tx, &activation.manifest_sha256)?;
    let available = postgres_available_ats_runtimes(
        tx,
        runtimes.clone(),
        &activation.trust_policy_sha256,
        recorded_at_ms,
        false,
    )?;
    require_ats_newer_activation_runtime_authority(&runtimes, &available)?;
    let verified: bool = tx
        .query_one(
            "SELECT EXISTS(
               SELECT 1
                 FROM jobs_ats_certification_heads head
                 JOIN jobs_ats_certification_head_transitions transition
                   ON transition.transition_sha256 = head.current_transition_sha256
                  AND transition.scope_sha256 = head.scope_sha256
                  AND transition.channel = head.channel
                  AND transition.head_revision = head.head_revision
                  AND transition.next_activation_sha256 = head.current_activation_sha256
                 JOIN jobs_ats_certification_activations activation
                   ON activation.activation_sha256 = head.current_activation_sha256
                 JOIN jobs_ats_certification_activations predecessor
                   ON predecessor.activation_sha256 = transition.previous_activation_sha256
                WHERE activation.activation_sha256 = $1
                  AND activation.predecessor_activation_sha256 = predecessor.activation_sha256
                  AND activation.scope_sha256 = predecessor.scope_sha256
                  AND activation.channel = predecessor.channel
                  AND activation.activation_generation > predecessor.activation_generation
                  AND activation.channel_sequence > predecessor.channel_sequence
                  AND transition.scope_sha256 = activation.scope_sha256
                  AND transition.channel = activation.channel
                  AND transition.recorded_at_ms > $2
                  AND transition.recorded_at_ms <= $3
                  AND activation.not_before_ms <= $3
                  AND activation.expires_at_ms > $3
                  AND $4::TEXT = 'activation'
                  AND transition.previous_activation_sha256 = $5
             )",
            &[
                &event.authority_ref,
                &previous_event_at_ms,
                &event.event_at_ms,
                &event.scope_kind,
                &event.subject_key,
            ],
        )
        .map_err(ats_certification_storage)?
        .get(0);
    if !verified {
        return Err(AtsCertificationAuthorityError::InvalidAuthority);
    }
    Ok(())
}

fn require_ats_newer_activation_runtime_authority(
    runtimes: &[AtsCertificationRuntimeTarget],
    available: &[AtsCertificationRuntimeTarget],
) -> Result<(), AtsCertificationAuthorityError> {
    if runtimes.is_empty() || runtimes != available {
        return Err(AtsCertificationAuthorityError::InvalidAuthority);
    }
    Ok(())
}

pub fn append_ats_certification_circuit_event(
    pool: &DbPool,
    event: &AtsCertificationCircuitEvent,
    expected_head_revision: i64,
    expected_event_id: Option<&str>,
    recorded_by: &str,
) -> Result<AtsCertificationCircuitState, AtsCertificationAuthorityError> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get().map_err(ats_certification_storage)?;
            let tx = conn
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(ats_certification_storage)?;
            let recorded_at_ms = now_ms();
            let result = append_ats_certification_circuit_event_sqlite_tx(
                &tx,
                event,
                expected_head_revision,
                expected_event_id,
                recorded_by,
                recorded_at_ms,
            )?;
            tx.commit().map_err(ats_certification_storage)?;
            Ok(result)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg().map_err(ats_certification_storage)?;
            let mut tx = conn.transaction().map_err(ats_certification_storage)?;
            lock_postgres_ats_certification(&mut tx)?;
            let recorded_at_ms = now_ms();
            let result = append_ats_certification_circuit_event_postgres_tx(
                &mut tx,
                event,
                expected_head_revision,
                expected_event_id,
                recorded_by,
                recorded_at_ms,
            )?;
            tx.commit().map_err(ats_certification_storage)?;
            Ok(result)
        }
    })
}

#[allow(clippy::too_many_arguments)]
pub fn append_ats_certification_circuit_event_sqlite_tx(
    tx: &rusqlite::Transaction<'_>,
    event: &AtsCertificationCircuitEvent,
    expected_head_revision: i64,
    expected_event_id: Option<&str>,
    recorded_by: &str,
    recorded_at_ms: i64,
) -> Result<AtsCertificationCircuitState, AtsCertificationAuthorityError> {
    validate_ats_recorded_by(recorded_by)?;
    validate_ats_certification_circuit_event(event, recorded_at_ms)?;
    let (event_sha256, canonical_event_base64url) =
        ats_certification_circuit_event_identity(event)?;
    let replay = tx
        .query_row(
            "SELECT event_sha256, canonical_event_base64url, recorded_by
               FROM jobs_ats_certification_circuit_events WHERE event_id = ?1",
            params![event.event_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            },
        )
        .optional()
        .map_err(ats_certification_storage)?;
    if let Some((stored_sha256, stored_canonical, stored_by)) = replay {
        if stored_sha256 != event_sha256
            || stored_canonical != canonical_event_base64url
            || stored_by != recorded_by
        {
            return Err(AtsCertificationAuthorityError::IdentityConflict);
        }
        let state = tx
            .query_row(
                "SELECT head_revision, current_event_id, state
                   FROM jobs_ats_certification_circuit_heads
                  WHERE scope_kind = ?1 AND subject_key = ?2",
                params![event.scope_kind, event.subject_key],
                |row| {
                    Ok(AtsCertificationCircuitState {
                        scope_kind: event.scope_kind.clone(),
                        subject_key: event.subject_key.clone(),
                        head_revision: row.get(0)?,
                        current_event_id: row.get(1)?,
                        current_event_sha256: event_sha256.clone(),
                        state: row.get(2)?,
                        replayed: true,
                    })
                },
            )
            .optional()
            .map_err(ats_certification_storage)?
            .ok_or(AtsCertificationAuthorityError::CompareAndSwapConflict)?;
        if state.current_event_id != event.event_id
            || state.head_revision != expected_head_revision + 1
        {
            return Err(AtsCertificationAuthorityError::CompareAndSwapConflict);
        }
        return Ok(state);
    }
    let current = tx
        .query_row(
            "SELECT head.state, head.head_revision, event.event_at_ms, head.current_event_id
               FROM jobs_ats_certification_circuit_heads head
               JOIN jobs_ats_certification_circuit_events event
                 ON event.event_id = head.current_event_id
              WHERE head.scope_kind = ?1 AND head.subject_key = ?2",
            params![event.scope_kind, event.subject_key],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, String>(3)?,
                ))
            },
        )
        .optional()
        .map_err(ats_certification_storage)?;
    if current
        .as_ref()
        .is_some_and(|current| expected_event_id != Some(current.3.as_str()))
    {
        return Err(AtsCertificationAuthorityError::CompareAndSwapConflict);
    }
    require_ats_circuit_transition(
        current
            .as_ref()
            .map(|current| (current.0.as_str(), current.1, current.2)),
        event,
        expected_head_revision,
        expected_event_id,
    )?;
    if let Some(current) = current.as_ref() {
        require_sqlite_ats_newer_activation_circuit_close(tx, event, current.2, recorded_at_ms)?;
    }
    tx.execute(
        "INSERT INTO jobs_ats_certification_circuit_events (
           event_id, event_sha256, canonical_event_base64url, scope_kind, subject_key,
           transition, trigger_kind, window_started_at_ms, window_ended_at_ms,
           failure_count, sample_count, threshold_count, authority_ref, event_at_ms,
           recorded_by, recorded_at_ms
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
        params![
            event.event_id,
            event_sha256,
            canonical_event_base64url,
            event.scope_kind,
            event.subject_key,
            event.transition,
            event.trigger_kind,
            event.window_started_at_ms,
            event.window_ended_at_ms,
            event.failure_count,
            event.sample_count,
            event.threshold_count,
            event.authority_ref,
            event.event_at_ms,
            recorded_by,
            recorded_at_ms,
        ],
    )
    .map_err(ats_certification_storage)?;
    let head_revision = expected_head_revision + 1;
    if current.is_some() {
        let changed = tx
            .execute(
                "UPDATE jobs_ats_certification_circuit_heads
                    SET head_revision = ?1, current_event_id = ?2, state = ?3,
                        updated_at_ms = ?4
                  WHERE scope_kind = ?5 AND subject_key = ?6
                    AND head_revision = ?7 AND current_event_id = ?8",
                params![
                    head_revision,
                    event.event_id,
                    event.transition,
                    recorded_at_ms,
                    event.scope_kind,
                    event.subject_key,
                    expected_head_revision,
                    expected_event_id,
                ],
            )
            .map_err(ats_certification_storage)?;
        if changed != 1 {
            return Err(AtsCertificationAuthorityError::CompareAndSwapConflict);
        }
    } else {
        tx.execute(
            "INSERT INTO jobs_ats_certification_circuit_heads (
               scope_kind, subject_key, head_revision, current_event_id, state, updated_at_ms
             ) VALUES (?1, ?2, 1, ?3, ?4, ?5)",
            params![
                event.scope_kind,
                event.subject_key,
                event.event_id,
                event.transition,
                recorded_at_ms,
            ],
        )
        .map_err(ats_certification_storage)?;
    }
    Ok(AtsCertificationCircuitState {
        scope_kind: event.scope_kind.clone(),
        subject_key: event.subject_key.clone(),
        head_revision,
        current_event_id: event.event_id.clone(),
        current_event_sha256: event_sha256,
        state: event.transition.clone(),
        replayed: false,
    })
}

#[allow(clippy::too_many_arguments)]
pub fn append_ats_certification_circuit_event_postgres_tx(
    tx: &mut postgres::Transaction<'_>,
    event: &AtsCertificationCircuitEvent,
    expected_head_revision: i64,
    expected_event_id: Option<&str>,
    recorded_by: &str,
    recorded_at_ms: i64,
) -> Result<AtsCertificationCircuitState, AtsCertificationAuthorityError> {
    validate_ats_recorded_by(recorded_by)?;
    validate_ats_certification_circuit_event(event, recorded_at_ms)?;
    let (event_sha256, canonical_event_base64url) =
        ats_certification_circuit_event_identity(event)?;
    if let Some(row) = tx
        .query_opt(
            "SELECT event_sha256, canonical_event_base64url, recorded_by
               FROM jobs_ats_certification_circuit_events WHERE event_id = $1",
            &[&event.event_id],
        )
        .map_err(ats_certification_storage)?
    {
        if row.get::<_, String>(0) != event_sha256
            || row.get::<_, String>(1) != canonical_event_base64url
            || row.get::<_, String>(2) != recorded_by
        {
            return Err(AtsCertificationAuthorityError::IdentityConflict);
        }
        let row = tx
            .query_opt(
                "SELECT head_revision, current_event_id, state
                   FROM jobs_ats_certification_circuit_heads
                  WHERE scope_kind = $1 AND subject_key = $2",
                &[&event.scope_kind, &event.subject_key],
            )
            .map_err(ats_certification_storage)?
            .ok_or(AtsCertificationAuthorityError::CompareAndSwapConflict)?;
        let state = AtsCertificationCircuitState {
            scope_kind: event.scope_kind.clone(),
            subject_key: event.subject_key.clone(),
            head_revision: row.get(0),
            current_event_id: row.get(1),
            current_event_sha256: event_sha256,
            state: row.get(2),
            replayed: true,
        };
        if state.current_event_id != event.event_id
            || state.head_revision != expected_head_revision + 1
        {
            return Err(AtsCertificationAuthorityError::CompareAndSwapConflict);
        }
        return Ok(state);
    }
    let current = tx
        .query_opt(
            "SELECT head.state, head.head_revision, event.event_at_ms, head.current_event_id
               FROM jobs_ats_certification_circuit_heads head
               JOIN jobs_ats_certification_circuit_events event
                 ON event.event_id = head.current_event_id
              WHERE head.scope_kind = $1 AND head.subject_key = $2 FOR UPDATE",
            &[&event.scope_kind, &event.subject_key],
        )
        .map_err(ats_certification_storage)?
        .map(|row| {
            (
                row.get::<_, String>(0),
                row.get::<_, i64>(1),
                row.get::<_, i64>(2),
                row.get::<_, String>(3),
            )
        });
    if current
        .as_ref()
        .is_some_and(|current| expected_event_id != Some(current.3.as_str()))
    {
        return Err(AtsCertificationAuthorityError::CompareAndSwapConflict);
    }
    require_ats_circuit_transition(
        current
            .as_ref()
            .map(|current| (current.0.as_str(), current.1, current.2)),
        event,
        expected_head_revision,
        expected_event_id,
    )?;
    if let Some(current) = current.as_ref() {
        require_postgres_ats_newer_activation_circuit_close(tx, event, current.2, recorded_at_ms)?;
    }
    tx.execute(
        "INSERT INTO jobs_ats_certification_circuit_events (
           event_id, event_sha256, canonical_event_base64url, scope_kind, subject_key,
           transition, trigger_kind, window_started_at_ms, window_ended_at_ms,
           failure_count, sample_count, threshold_count, authority_ref, event_at_ms,
           recorded_by, recorded_at_ms
         ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16)",
        &[
            &event.event_id,
            &event_sha256,
            &canonical_event_base64url,
            &event.scope_kind,
            &event.subject_key,
            &event.transition,
            &event.trigger_kind,
            &event.window_started_at_ms,
            &event.window_ended_at_ms,
            &event.failure_count,
            &event.sample_count,
            &event.threshold_count,
            &event.authority_ref,
            &event.event_at_ms,
            &recorded_by,
            &recorded_at_ms,
        ],
    )
    .map_err(ats_certification_storage)?;
    let head_revision = expected_head_revision + 1;
    if current.is_some() {
        let changed = tx
            .execute(
                "UPDATE jobs_ats_certification_circuit_heads
                    SET head_revision = $1, current_event_id = $2, state = $3,
                        updated_at_ms = $4
                  WHERE scope_kind = $5 AND subject_key = $6
                    AND head_revision = $7 AND current_event_id = $8",
                &[
                    &head_revision,
                    &event.event_id,
                    &event.transition,
                    &recorded_at_ms,
                    &event.scope_kind,
                    &event.subject_key,
                    &expected_head_revision,
                    &expected_event_id,
                ],
            )
            .map_err(ats_certification_storage)?;
        if changed != 1 {
            return Err(AtsCertificationAuthorityError::CompareAndSwapConflict);
        }
    } else {
        tx.execute(
            "INSERT INTO jobs_ats_certification_circuit_heads (
               scope_kind, subject_key, head_revision, current_event_id, state, updated_at_ms
             ) VALUES ($1, $2, 1, $3, $4, $5)",
            &[
                &event.scope_kind,
                &event.subject_key,
                &event.event_id,
                &event.transition,
                &recorded_at_ms,
            ],
        )
        .map_err(ats_certification_storage)?;
    }
    Ok(AtsCertificationCircuitState {
        scope_kind: event.scope_kind.clone(),
        subject_key: event.subject_key.clone(),
        head_revision,
        current_event_id: event.event_id.clone(),
        current_event_sha256: event_sha256,
        state: event.transition.clone(),
        replayed: false,
    })
}

#[derive(Debug, Clone)]
struct StoredAtsBindingRow {
    activation: StoredAtsActivationHeadAuthority,
    provider: String,
    target_key: String,
    allowed_provider_hosts_json: String,
    variant_key: String,
    surface_sha256: String,
    certification_id: String,
    manifest_generation: i64,
    adapter_version: String,
    final_submit_control_id: String,
    adapter_bundle_sha256: String,
    source_commit: String,
    layout_contract_version: i64,
    layout_contract_sha256: String,
    layout_set_sha256: String,
    manifest_not_before_ms: i64,
    manifest_expires_at_ms: i64,
    last_verified_at_ms: i64,
    head: StoredAtsHead,
}

/// Resolves target status from strict URL plus fresh discovery/original-source
/// facts before a live page is accepted as layout evidence. `runner = None`
/// returns the bounded certified runtime matrix in the binding.
pub fn resolve_ats_certification_target_status(
    pool: &DbPool,
    target_evidence: &AtsCertificationFreshTargetEvidence,
    runner: Option<&str>,
    channel: &str,
    expected_account_allowlist_sha256: Option<&str>,
    now_ms: i64,
) -> Result<Option<AtsCertificationBinding>, AtsCertificationAuthorityError> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get().map_err(ats_certification_storage)?;
            let tx = conn
                .transaction_with_behavior(TransactionBehavior::Deferred)
                .map_err(ats_certification_storage)?;
            let result = resolve_ats_certification_target_status_sqlite_tx(
                &tx,
                target_evidence,
                runner,
                channel,
                expected_account_allowlist_sha256,
                now_ms,
            )?;
            tx.commit().map_err(ats_certification_storage)?;
            Ok(result)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg().map_err(ats_certification_storage)?;
            let mut tx = conn
                .build_transaction()
                .isolation_level(postgres::IsolationLevel::RepeatableRead)
                .read_only(true)
                .start()
                .map_err(ats_certification_storage)?;
            let result = resolve_ats_certification_target_status_postgres_tx(
                &mut tx,
                target_evidence,
                runner,
                channel,
                expected_account_allowlist_sha256,
                now_ms,
            )?;
            tx.commit().map_err(ats_certification_storage)?;
            Ok(result)
        }
    })
}

#[derive(Debug)]
struct AtsCertificationInactiveStatusRow {
    activation_sha256: String,
    activation_id: String,
    manifest_sha256: String,
    certification_id: String,
    scope_sha256: String,
    adapter_version: String,
    adapter_bundle_sha256: String,
    activation_policy_sha256: String,
    manifest_policy_sha256: String,
    account_allowlist_sha256: Option<String>,
    activation_not_before_ms: i64,
    activation_expires_at_ms: i64,
    manifest_not_before_ms: i64,
    manifest_expires_at_ms: i64,
    manifest_tested_at_ms: i64,
}

#[derive(Debug, Clone)]
struct AtsCertificationInactiveStatus {
    status: String,
    expires_at_ms: Option<i64>,
    last_verified_at_ms: Option<i64>,
}

impl AtsCertificationInactiveStatus {
    fn without_authority(status: &str) -> Self {
        Self {
            status: status.to_string(),
            expires_at_ms: None,
            last_verified_at_ms: None,
        }
    }

    fn from_row(status: &str, row: &AtsCertificationInactiveStatusRow) -> Self {
        Self {
            status: status.to_string(),
            expires_at_ms: Some(row.activation_expires_at_ms.min(row.manifest_expires_at_ms)),
            last_verified_at_ms: Some(row.manifest_tested_at_ms),
        }
    }
}

fn sqlite_ats_inactive_target_status(
    tx: &rusqlite::Transaction<'_>,
    provider: &str,
    target_key: &str,
    runner_target_sha256: Option<&str>,
    channel: &str,
    server_account_allowlist_sha256: Option<&str>,
    now_ms: i64,
) -> Result<AtsCertificationInactiveStatus, AtsCertificationAuthorityError> {
    let mut stmt = tx
        .prepare(
            "SELECT activation.activation_sha256, activation.activation_id,
                    activation.manifest_sha256, manifest.certification_id,
                    activation.scope_sha256, manifest.adapter_version,
                    manifest.adapter_bundle_sha256, activation.trust_policy_sha256,
                    manifest.trust_policy_sha256, activation.account_allowlist_sha256,
                    activation.not_before_ms, activation.expires_at_ms,
                    manifest.not_before_ms, manifest.expires_at_ms,
                    manifest.tested_at_ms
               FROM jobs_ats_certification_heads head
               JOIN jobs_ats_certification_activations activation
                 ON activation.activation_sha256 = head.current_activation_sha256
               JOIN jobs_ats_certification_manifests manifest
                 ON manifest.manifest_sha256 = activation.manifest_sha256
              WHERE head.channel = ?1 AND manifest.provider = ?2
                AND manifest.target_key = ?3 LIMIT 2",
        )
        .map_err(ats_certification_storage)?;
    let rows = stmt
        .query_map(params![channel, provider, target_key], |row| {
            Ok(AtsCertificationInactiveStatusRow {
                activation_sha256: row.get(0)?,
                activation_id: row.get(1)?,
                manifest_sha256: row.get(2)?,
                certification_id: row.get(3)?,
                scope_sha256: row.get(4)?,
                adapter_version: row.get(5)?,
                adapter_bundle_sha256: row.get(6)?,
                activation_policy_sha256: row.get(7)?,
                manifest_policy_sha256: row.get(8)?,
                account_allowlist_sha256: row.get(9)?,
                activation_not_before_ms: row.get(10)?,
                activation_expires_at_ms: row.get(11)?,
                manifest_not_before_ms: row.get(12)?,
                manifest_expires_at_ms: row.get(13)?,
                manifest_tested_at_ms: row.get(14)?,
            })
        })
        .map_err(ats_certification_storage)?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(ats_certification_storage)?;
    if rows.is_empty() {
        return Ok(AtsCertificationInactiveStatus::without_authority(
            "review_only",
        ));
    }
    if rows.len() != 1 {
        return Ok(AtsCertificationInactiveStatus::without_authority("drifted"));
    }
    let row = rows
        .into_iter()
        .next()
        .ok_or(AtsCertificationAuthorityError::IdentityConflict)?;
    if match channel {
        "canary" => row.account_allowlist_sha256.as_deref() != server_account_allowlist_sha256,
        "general" | "shadow" => server_account_allowlist_sha256.is_some(),
        _ => true,
    } {
        return Ok(AtsCertificationInactiveStatus::from_row(
            "review_only",
            &row,
        ));
    }
    let trust = tx
        .query_row(
            "SELECT head.current_policy_sha256, policy.valid_from_ms, policy.expires_at_ms
               FROM jobs_ats_certification_trust_head head
               JOIN jobs_ats_certification_trust_policies policy
                 ON policy.policy_sha256 = head.current_policy_sha256
              WHERE head.singleton_id = 1",
            [],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            },
        )
        .optional()
        .map_err(ats_certification_storage)?;
    let Some((policy_sha256, policy_not_before_ms, policy_expires_at_ms)) = trust else {
        return Ok(AtsCertificationInactiveStatus::from_row(
            "review_only",
            &row,
        ));
    };
    let revoked: i64 = tx
        .query_row(
            "SELECT EXISTS(
               SELECT 1 FROM jobs_ats_certification_revocations revocation
                WHERE revocation.effective_at_ms <= ?1
                  AND revocation.trust_policy_sha256 = ?10 AND (
                  (revocation.subject_kind = 'activation'
                    AND revocation.subject_id = ?2 AND revocation.subject_sha256 = ?3)
                  OR (revocation.subject_kind = 'manifest'
                    AND revocation.subject_id = ?4 AND revocation.subject_sha256 = ?5)
                  OR (revocation.subject_kind = 'policy'
                    AND revocation.subject_sha256 = ?10)
                  OR (revocation.subject_kind = 'trust_key' AND EXISTS (
                    SELECT 1 FROM jobs_ats_certification_trust_keys trust_key
                     WHERE trust_key.policy_sha256 = ?10
                       AND trust_key.key_id = revocation.subject_id
                  ))
                  OR (revocation.subject_kind = 'layout_observation' AND EXISTS (
                    SELECT 1 FROM jobs_ats_certification_manifest_layouts layout_binding
                    JOIN jobs_ats_certification_layout_observations observation
                      ON observation.observation_sha256 = layout_binding.observation_sha256
                   WHERE layout_binding.manifest_sha256 = ?5
                     AND observation.observation_id = revocation.subject_id
                     AND observation.observation_sha256 = revocation.subject_sha256
                  ))
                  OR (revocation.subject_kind = 'target'
                    AND revocation.subject_id = ?11)
                  OR (revocation.subject_kind = 'scope'
                    AND revocation.subject_id = ?6 AND revocation.subject_sha256 = ?6)
                  OR (revocation.subject_kind = 'adapter_bundle'
                    AND revocation.subject_id = ?7 AND revocation.subject_sha256 = ?8)
                  OR (revocation.subject_kind = 'browser_release_manifest'
                    AND ?9 IS NOT NULL AND EXISTS (
                      SELECT 1 FROM jobs_ats_certification_runtime_targets runtime
                       WHERE runtime.manifest_sha256 = ?5 AND runtime.runtime_sha256 = ?9
                         AND runtime.browser_release_manifest_sha256 = revocation.subject_sha256
                         AND revocation.subject_id = revocation.subject_sha256
                    ))
                  OR (revocation.subject_kind = 'runner_image'
                    AND ?9 IS NOT NULL AND EXISTS (
                      SELECT 1 FROM jobs_ats_certification_runtime_targets runtime
                       WHERE runtime.manifest_sha256 = ?5 AND runtime.runtime_sha256 = ?9
                         AND runtime.runner_image_sha256 = revocation.subject_sha256
                         AND revocation.subject_id = revocation.subject_sha256
                    ))
                  OR (revocation.subject_kind = 'runner_build'
                    AND ?9 IS NOT NULL AND EXISTS (
                      SELECT 1 FROM jobs_ats_certification_runtime_targets runtime
                       WHERE runtime.manifest_sha256 = ?5 AND runtime.runtime_sha256 = ?9
                         AND runtime.runner_build_id = revocation.subject_id
                    ))
                  OR (revocation.subject_kind = 'runtime' AND ?9 IS NOT NULL
                    AND revocation.subject_sha256 = ?9)
                )
             )",
            params![
                now_ms,
                row.activation_id,
                row.activation_sha256,
                row.certification_id,
                row.manifest_sha256,
                row.scope_sha256,
                row.adapter_version,
                row.adapter_bundle_sha256,
                runner_target_sha256,
                policy_sha256,
                target_key,
            ],
            |row| row.get(0),
        )
        .map_err(ats_certification_storage)?;
    if revoked != 0 {
        return Ok(AtsCertificationInactiveStatus::from_row("revoked", &row));
    }
    if row.activation_policy_sha256 != policy_sha256 || row.manifest_policy_sha256 != policy_sha256
    {
        return Ok(AtsCertificationInactiveStatus::from_row("drifted", &row));
    }
    if now_ms < policy_not_before_ms
        || now_ms >= policy_expires_at_ms
        || now_ms < row.activation_not_before_ms
        || now_ms >= row.activation_expires_at_ms
        || now_ms < row.manifest_not_before_ms
        || now_ms >= row.manifest_expires_at_ms
    {
        return Ok(AtsCertificationInactiveStatus::from_row("expired", &row));
    }
    let suspended: i64 = tx
        .query_row(
            "SELECT EXISTS(
                 SELECT 1 FROM jobs_ats_certification_circuit_heads circuit
                  WHERE circuit.state <> 'closed' AND (
                    (circuit.scope_kind = 'provider' AND circuit.subject_key = ?1)
                    OR (circuit.scope_kind = 'target' AND circuit.subject_key = ?2)
                    OR (circuit.scope_kind = 'activation' AND circuit.subject_key = ?3)
                    OR (circuit.scope_kind = 'adapter' AND circuit.subject_key = ?4)
                    OR (circuit.scope_kind = 'runtime' AND ?5 IS NOT NULL
                      AND circuit.subject_key = ?5)
                  )
               )",
            params![
                provider,
                target_key,
                row.activation_sha256,
                row.adapter_bundle_sha256,
                runner_target_sha256,
            ],
            |row| row.get(0),
        )
        .map_err(ats_certification_storage)?;
    if suspended != 0 {
        return Ok(AtsCertificationInactiveStatus::from_row("suspended", &row));
    }
    let _quarantined: i64 = tx
        .query_row(
            "SELECT EXISTS(
                 SELECT 1 FROM jobs_ats_certification_quarantine_heads quarantine
                  WHERE quarantine.state = 'quarantined' AND (
                    (quarantine.scope_kind = 'activation' AND quarantine.scope_id = ?1
                      AND quarantine.scope_sha256 = ?2)
                    OR (quarantine.scope_kind = 'adapter' AND quarantine.scope_id = ?3
                      AND quarantine.scope_sha256 = ?4)
                    OR (quarantine.scope_kind = 'runtime' AND ?5 IS NOT NULL
                      AND quarantine.scope_sha256 = ?5)
                  )
               )",
            params![
                row.activation_id,
                row.activation_sha256,
                row.adapter_version,
                row.adapter_bundle_sha256,
                runner_target_sha256,
            ],
            |row| row.get(0),
        )
        .map_err(ats_certification_storage)?;
    Ok(AtsCertificationInactiveStatus::from_row("drifted", &row))
}

fn postgres_ats_inactive_target_status(
    tx: &mut postgres::Transaction<'_>,
    provider: &str,
    target_key: &str,
    runner_target_sha256: Option<&str>,
    channel: &str,
    server_account_allowlist_sha256: Option<&str>,
    now_ms: i64,
) -> Result<AtsCertificationInactiveStatus, AtsCertificationAuthorityError> {
    let rows = tx
        .query(
            "SELECT activation.activation_sha256, activation.activation_id,
                    activation.manifest_sha256, manifest.certification_id,
                    activation.scope_sha256, manifest.adapter_version,
                    manifest.adapter_bundle_sha256, activation.trust_policy_sha256,
                    manifest.trust_policy_sha256, activation.account_allowlist_sha256,
                    activation.not_before_ms, activation.expires_at_ms,
                    manifest.not_before_ms, manifest.expires_at_ms,
                    manifest.tested_at_ms
               FROM jobs_ats_certification_heads head
               JOIN jobs_ats_certification_activations activation
                 ON activation.activation_sha256 = head.current_activation_sha256
               JOIN jobs_ats_certification_manifests manifest
                 ON manifest.manifest_sha256 = activation.manifest_sha256
              WHERE head.channel = $1 AND manifest.provider = $2
                AND manifest.target_key = $3 LIMIT 2",
            &[&channel, &provider, &target_key],
        )
        .map_err(ats_certification_storage)?;
    if rows.is_empty() {
        return Ok(AtsCertificationInactiveStatus::without_authority(
            "review_only",
        ));
    }
    if rows.len() != 1 {
        return Ok(AtsCertificationInactiveStatus::without_authority("drifted"));
    }
    let stored = &rows[0];
    let row = AtsCertificationInactiveStatusRow {
        activation_sha256: stored.get(0),
        activation_id: stored.get(1),
        manifest_sha256: stored.get(2),
        certification_id: stored.get(3),
        scope_sha256: stored.get(4),
        adapter_version: stored.get(5),
        adapter_bundle_sha256: stored.get(6),
        activation_policy_sha256: stored.get(7),
        manifest_policy_sha256: stored.get(8),
        account_allowlist_sha256: stored.get(9),
        activation_not_before_ms: stored.get(10),
        activation_expires_at_ms: stored.get(11),
        manifest_not_before_ms: stored.get(12),
        manifest_expires_at_ms: stored.get(13),
        manifest_tested_at_ms: stored.get(14),
    };
    if match channel {
        "canary" => row.account_allowlist_sha256.as_deref() != server_account_allowlist_sha256,
        "general" | "shadow" => server_account_allowlist_sha256.is_some(),
        _ => true,
    } {
        return Ok(AtsCertificationInactiveStatus::from_row(
            "review_only",
            &row,
        ));
    }
    let trust = tx
        .query_opt(
            "SELECT head.current_policy_sha256, policy.valid_from_ms, policy.expires_at_ms
               FROM jobs_ats_certification_trust_head head
               JOIN jobs_ats_certification_trust_policies policy
                 ON policy.policy_sha256 = head.current_policy_sha256
              WHERE head.singleton_id = 1",
            &[],
        )
        .map_err(ats_certification_storage)?;
    let Some(trust) = trust else {
        return Ok(AtsCertificationInactiveStatus::from_row(
            "review_only",
            &row,
        ));
    };
    let policy_sha256: String = trust.get(0);
    let policy_not_before_ms: i64 = trust.get(1);
    let policy_expires_at_ms: i64 = trust.get(2);
    let revoked: bool = tx
        .query_one(
            "SELECT EXISTS(
               SELECT 1 FROM jobs_ats_certification_revocations revocation
                WHERE revocation.effective_at_ms <= $1
                  AND revocation.trust_policy_sha256 = $10 AND (
                  (revocation.subject_kind = 'activation'
                    AND revocation.subject_id = $2 AND revocation.subject_sha256 = $3)
                  OR (revocation.subject_kind = 'manifest'
                    AND revocation.subject_id = $4 AND revocation.subject_sha256 = $5)
                  OR (revocation.subject_kind = 'policy'
                    AND revocation.subject_sha256 = $10)
                  OR (revocation.subject_kind = 'trust_key' AND EXISTS (
                    SELECT 1 FROM jobs_ats_certification_trust_keys trust_key
                     WHERE trust_key.policy_sha256 = $10
                       AND trust_key.key_id = revocation.subject_id
                  ))
                  OR (revocation.subject_kind = 'layout_observation' AND EXISTS (
                    SELECT 1 FROM jobs_ats_certification_manifest_layouts layout_binding
                    JOIN jobs_ats_certification_layout_observations observation
                      ON observation.observation_sha256 = layout_binding.observation_sha256
                   WHERE layout_binding.manifest_sha256 = $5
                     AND observation.observation_id = revocation.subject_id
                     AND observation.observation_sha256 = revocation.subject_sha256
                  ))
                  OR (revocation.subject_kind = 'target'
                    AND revocation.subject_id = $11)
                  OR (revocation.subject_kind = 'scope'
                    AND revocation.subject_id = $6 AND revocation.subject_sha256 = $6)
                  OR (revocation.subject_kind = 'adapter_bundle'
                    AND revocation.subject_id = $7 AND revocation.subject_sha256 = $8)
                  OR (revocation.subject_kind = 'browser_release_manifest'
                    AND $9::TEXT IS NOT NULL AND EXISTS (
                      SELECT 1 FROM jobs_ats_certification_runtime_targets runtime
                       WHERE runtime.manifest_sha256 = $5 AND runtime.runtime_sha256 = $9
                         AND runtime.browser_release_manifest_sha256 = revocation.subject_sha256
                         AND revocation.subject_id = revocation.subject_sha256
                    ))
                  OR (revocation.subject_kind = 'runner_image'
                    AND $9::TEXT IS NOT NULL AND EXISTS (
                      SELECT 1 FROM jobs_ats_certification_runtime_targets runtime
                       WHERE runtime.manifest_sha256 = $5 AND runtime.runtime_sha256 = $9
                         AND runtime.runner_image_sha256 = revocation.subject_sha256
                         AND revocation.subject_id = revocation.subject_sha256
                    ))
                  OR (revocation.subject_kind = 'runner_build'
                    AND $9::TEXT IS NOT NULL AND EXISTS (
                      SELECT 1 FROM jobs_ats_certification_runtime_targets runtime
                       WHERE runtime.manifest_sha256 = $5 AND runtime.runtime_sha256 = $9
                         AND runtime.runner_build_id = revocation.subject_id
                    ))
                  OR (revocation.subject_kind = 'runtime' AND $9::TEXT IS NOT NULL
                    AND revocation.subject_sha256 = $9)
                )
             )",
            &[
                &now_ms,
                &row.activation_id,
                &row.activation_sha256,
                &row.certification_id,
                &row.manifest_sha256,
                &row.scope_sha256,
                &row.adapter_version,
                &row.adapter_bundle_sha256,
                &runner_target_sha256,
                &policy_sha256,
                &target_key,
            ],
        )
        .map_err(ats_certification_storage)?
        .get(0);
    if revoked {
        return Ok(AtsCertificationInactiveStatus::from_row("revoked", &row));
    }
    if row.activation_policy_sha256 != policy_sha256 || row.manifest_policy_sha256 != policy_sha256
    {
        return Ok(AtsCertificationInactiveStatus::from_row("drifted", &row));
    }
    if now_ms < policy_not_before_ms
        || now_ms >= policy_expires_at_ms
        || now_ms < row.activation_not_before_ms
        || now_ms >= row.activation_expires_at_ms
        || now_ms < row.manifest_not_before_ms
        || now_ms >= row.manifest_expires_at_ms
    {
        return Ok(AtsCertificationInactiveStatus::from_row("expired", &row));
    }
    let suspended: bool = tx
        .query_one(
            "SELECT EXISTS(
                 SELECT 1 FROM jobs_ats_certification_circuit_heads circuit
                  WHERE circuit.state <> 'closed' AND (
                    (circuit.scope_kind = 'provider' AND circuit.subject_key = $1)
                    OR (circuit.scope_kind = 'target' AND circuit.subject_key = $2)
                    OR (circuit.scope_kind = 'activation' AND circuit.subject_key = $3)
                    OR (circuit.scope_kind = 'adapter' AND circuit.subject_key = $4)
                    OR (circuit.scope_kind = 'runtime' AND $5::TEXT IS NOT NULL
                      AND circuit.subject_key = $5)
                  )
               )",
            &[
                &provider,
                &target_key,
                &row.activation_sha256,
                &row.adapter_bundle_sha256,
                &runner_target_sha256,
            ],
        )
        .map_err(ats_certification_storage)?
        .get(0);
    if suspended {
        return Ok(AtsCertificationInactiveStatus::from_row("suspended", &row));
    }
    let _quarantined: bool = tx
        .query_one(
            "SELECT EXISTS(
                 SELECT 1 FROM jobs_ats_certification_quarantine_heads quarantine
                  WHERE quarantine.state = 'quarantined' AND (
                    (quarantine.scope_kind = 'activation' AND quarantine.scope_id = $1
                      AND quarantine.scope_sha256 = $2)
                    OR (quarantine.scope_kind = 'adapter' AND quarantine.scope_id = $3
                      AND quarantine.scope_sha256 = $4)
                    OR (quarantine.scope_kind = 'runtime' AND $5::TEXT IS NOT NULL
                      AND quarantine.scope_sha256 = $5)
                  )
               )",
            &[
                &row.activation_id,
                &row.activation_sha256,
                &row.adapter_version,
                &row.adapter_bundle_sha256,
                &runner_target_sha256,
            ],
        )
        .map_err(ats_certification_storage)?
        .get(0);
    Ok(AtsCertificationInactiveStatus::from_row("drifted", &row))
}

fn ats_inactive_target_status(
    pool: &DbPool,
    target_evidence: &AtsCertificationFreshTargetEvidence,
    runner_target_sha256: Option<&str>,
    channel: &str,
    server_account_allowlist_sha256: Option<&str>,
    now_ms: i64,
) -> Result<AtsCertificationInactiveStatus, AtsCertificationAuthorityError> {
    let (provider, target_key) =
        validate_ats_certification_fresh_target_evidence(target_evidence, now_ms)?;
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get().map_err(ats_certification_storage)?;
            let tx = conn
                .transaction_with_behavior(TransactionBehavior::Deferred)
                .map_err(ats_certification_storage)?;
            let status = sqlite_ats_inactive_target_status(
                &tx,
                &provider,
                &target_key,
                runner_target_sha256,
                channel,
                server_account_allowlist_sha256,
                now_ms,
            )?;
            tx.commit().map_err(ats_certification_storage)?;
            Ok(status)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg().map_err(ats_certification_storage)?;
            let mut tx = conn
                .build_transaction()
                .isolation_level(postgres::IsolationLevel::RepeatableRead)
                .read_only(true)
                .start()
                .map_err(ats_certification_storage)?;
            let status = postgres_ats_inactive_target_status(
                &mut tx,
                &provider,
                &target_key,
                runner_target_sha256,
                channel,
                server_account_allowlist_sha256,
                now_ms,
            )?;
            tx.commit().map_err(ats_certification_storage)?;
            Ok(status)
        }
    })
}

pub fn get_ats_certification_target_status_projection(
    pool: &DbPool,
    target_evidence: &AtsCertificationFreshTargetEvidence,
    runner_target_sha256: Option<&str>,
    channel: &str,
    server_account_allowlist_sha256: Option<&str>,
    now_ms: i64,
) -> Result<AtsCertificationTargetStatusProjection, AtsCertificationAuthorityError> {
    let (provider, target_key) =
        validate_ats_certification_fresh_target_evidence(target_evidence, now_ms)?;
    let target_key_sha256 = ats_certification_target_key_sha256(&target_key)?;
    let binding = resolve_ats_certification_target_status(
        pool,
        target_evidence,
        runner_target_sha256,
        channel,
        server_account_allowlist_sha256,
        now_ms,
    )?;
    let Some(binding) = binding else {
        let inactive = ats_inactive_target_status(
            pool,
            target_evidence,
            runner_target_sha256,
            channel,
            server_account_allowlist_sha256,
            now_ms,
        )?;
        return Ok(ats_target_status_projection_for_inactive(
            provider,
            target_key_sha256,
            inactive,
        ));
    };
    let mut runner_kinds = binding
        .runtime_targets
        .iter()
        .map(|runtime| runtime.runtime_kind.clone())
        .collect::<Vec<_>>();
    runner_kinds.sort();
    runner_kinds.dedup();
    let mut runner_target_sha256s = binding
        .runtime_targets
        .iter()
        .map(|runtime| runtime.runtime_sha256.clone())
        .collect::<Vec<_>>();
    runner_target_sha256s.sort();
    Ok(AtsCertificationTargetStatusProjection {
        schema_version: 1,
        provider,
        target_key_sha256,
        status: if binding.channel == "shadow" {
            "review_only".to_string()
        } else {
            "active".to_string()
        },
        adapter_version: Some(binding.adapter_version),
        manifest_sha256: Some(binding.manifest_sha256),
        activation_sha256: Some(binding.activation_sha256),
        activation_generation: Some(binding.activation_generation),
        layout_set_sha256: Some(binding.layout_set_sha256),
        rollout_channel: Some(binding.channel.clone()),
        runner_kinds,
        runner_target_sha256s,
        expires_at_ms: Some(binding.expires_at_ms),
        last_verified_at_ms: Some(binding.last_verified_at_ms),
        canary_available: binding.channel == "canary",
    })
}

fn ats_target_status_projection_for_binding(
    provider: String,
    target_key_sha256: String,
    binding: &AtsCertificationBinding,
) -> AtsCertificationTargetStatusProjection {
    let mut runner_kinds = binding
        .runtime_targets
        .iter()
        .map(|runtime| runtime.runtime_kind.clone())
        .collect::<Vec<_>>();
    runner_kinds.sort();
    runner_kinds.dedup();
    let mut runner_target_sha256s = binding
        .runtime_targets
        .iter()
        .map(|runtime| runtime.runtime_sha256.clone())
        .collect::<Vec<_>>();
    runner_target_sha256s.sort();
    AtsCertificationTargetStatusProjection {
        schema_version: 1,
        provider,
        target_key_sha256,
        status: if binding.channel == "shadow" {
            "review_only".to_string()
        } else {
            "active".to_string()
        },
        adapter_version: Some(binding.adapter_version.clone()),
        manifest_sha256: Some(binding.manifest_sha256.clone()),
        activation_sha256: Some(binding.activation_sha256.clone()),
        activation_generation: Some(binding.activation_generation),
        layout_set_sha256: Some(binding.layout_set_sha256.clone()),
        rollout_channel: Some(binding.channel.clone()),
        runner_kinds,
        runner_target_sha256s,
        expires_at_ms: Some(binding.expires_at_ms),
        last_verified_at_ms: Some(binding.last_verified_at_ms),
        canary_available: binding.channel == "canary",
    }
}

fn empty_ats_target_status_projection(
    provider: String,
    target_key_sha256: String,
    status: String,
) -> AtsCertificationTargetStatusProjection {
    AtsCertificationTargetStatusProjection {
        schema_version: 1,
        provider,
        target_key_sha256,
        status,
        adapter_version: None,
        manifest_sha256: None,
        activation_sha256: None,
        activation_generation: None,
        layout_set_sha256: None,
        rollout_channel: None,
        runner_kinds: Vec::new(),
        runner_target_sha256s: Vec::new(),
        expires_at_ms: None,
        last_verified_at_ms: None,
        canary_available: false,
    }
}

fn ats_target_status_projection_for_inactive(
    provider: String,
    target_key_sha256: String,
    inactive: AtsCertificationInactiveStatus,
) -> AtsCertificationTargetStatusProjection {
    let mut projection =
        empty_ats_target_status_projection(provider, target_key_sha256, inactive.status);
    projection.expires_at_ms = inactive.expires_at_ms;
    projection.last_verified_at_ms = inactive.last_verified_at_ms;
    projection
}

fn sqlite_select_ats_posting_runtime(
    tx: &rusqlite::Transaction<'_>,
    target_evidence: &AtsCertificationFreshTargetEvidence,
    matrix: AtsCertificationBinding,
    channel: &str,
    account_allowlist_sha256: Option<&str>,
    runtime_attestation: Option<&AtsCertificationRuntimeAttestation>,
    now_ms: i64,
) -> Result<Option<AtsCertificationBinding>, AtsCertificationAuthorityError> {
    let Some(attestation) = runtime_attestation else {
        return Ok(Some(matrix));
    };
    let runtime = match ats_certification_runtime_target_from_attestation(&matrix, attestation) {
        Ok(runtime) => runtime,
        Err(AtsCertificationAuthorityError::RuntimeMismatch) => return Ok(None),
        Err(error) => return Err(error),
    };
    resolve_ats_certification_target_status_sqlite_tx(
        tx,
        target_evidence,
        Some(&runtime.runtime_sha256),
        channel,
        account_allowlist_sha256,
        now_ms,
    )
}

fn postgres_select_ats_posting_runtime(
    tx: &mut postgres::Transaction<'_>,
    target_evidence: &AtsCertificationFreshTargetEvidence,
    matrix: AtsCertificationBinding,
    channel: &str,
    account_allowlist_sha256: Option<&str>,
    runtime_attestation: Option<&AtsCertificationRuntimeAttestation>,
    now_ms: i64,
) -> Result<Option<AtsCertificationBinding>, AtsCertificationAuthorityError> {
    let Some(attestation) = runtime_attestation else {
        return Ok(Some(matrix));
    };
    let runtime = match ats_certification_runtime_target_from_attestation(&matrix, attestation) {
        Ok(runtime) => runtime,
        Err(AtsCertificationAuthorityError::RuntimeMismatch) => return Ok(None),
        Err(error) => return Err(error),
    };
    resolve_ats_certification_target_status_postgres_tx(
        tx,
        target_evidence,
        Some(&runtime.runtime_sha256),
        channel,
        account_allowlist_sha256,
        now_ms,
    )
}

pub fn resolve_ats_certification_for_posting(
    pool: &DbPool,
    account_id: &str,
    posting: &JobPosting,
    runtime_attestation: Option<&AtsCertificationRuntimeAttestation>,
    now_ms: i64,
) -> Result<AtsCertificationPostingResolution, AtsCertificationAuthorityError> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get().map_err(ats_certification_storage)?;
            let tx = conn
                .transaction_with_behavior(TransactionBehavior::Deferred)
                .map_err(ats_certification_storage)?;
            let result = resolve_ats_certification_for_posting_sqlite_tx(
                &tx,
                account_id,
                posting,
                runtime_attestation,
                now_ms,
            )?;
            tx.commit().map_err(ats_certification_storage)?;
            Ok(result)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg().map_err(ats_certification_storage)?;
            let mut tx = conn
                .build_transaction()
                .isolation_level(postgres::IsolationLevel::RepeatableRead)
                .read_only(true)
                .start()
                .map_err(ats_certification_storage)?;
            let result = resolve_ats_certification_for_posting_postgres_tx(
                &mut tx,
                account_id,
                posting,
                runtime_attestation,
                now_ms,
            )?;
            tx.commit().map_err(ats_certification_storage)?;
            Ok(result)
        }
    })
}

pub fn resolve_ats_certification_for_posting_sqlite_tx(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    posting: &JobPosting,
    runtime_attestation: Option<&AtsCertificationRuntimeAttestation>,
    now_ms: i64,
) -> Result<AtsCertificationPostingResolution, AtsCertificationAuthorityError> {
    if !ats_certification_text(account_id, 1, 240) {
        return Err(AtsCertificationAuthorityError::InvalidAuthority);
    }
    if let Some(attestation) = runtime_attestation {
        validate_ats_runtime_attestation(attestation)?;
    }
    let target_evidence = ats_certification_fresh_target_evidence_from_posting(posting, now_ms)?;
    let (provider, target_key) =
        validate_ats_certification_fresh_target_evidence(&target_evidence, now_ms)?;
    let target_key_sha256 = ats_certification_target_key_sha256(&target_key)?;
    if let Some(matrix) = resolve_ats_certification_target_status_sqlite_tx(
        tx,
        &target_evidence,
        None,
        "general",
        None,
        now_ms,
    )? {
        if let Some(binding) = sqlite_select_ats_posting_runtime(
            tx,
            &target_evidence,
            matrix,
            "general",
            None,
            runtime_attestation,
            now_ms,
        )? {
            return Ok(AtsCertificationPostingResolution {
                status: ats_target_status_projection_for_binding(
                    provider,
                    target_key_sha256,
                    &binding,
                ),
                active_binding: Some(binding),
            });
        }
        return Ok(AtsCertificationPostingResolution {
            status: empty_ats_target_status_projection(
                provider,
                target_key_sha256,
                "drifted".to_string(),
            ),
            active_binding: None,
        });
    }
    let allowlist_sha256 = resolve_sqlite_ats_canary_allowlist_for_target_account(
        tx,
        &target_evidence,
        account_id,
        now_ms,
    )?;
    if let Some(allowlist_sha256) = allowlist_sha256.as_deref() {
        if let Some(matrix) = resolve_ats_certification_target_status_sqlite_tx(
            tx,
            &target_evidence,
            None,
            "canary",
            Some(allowlist_sha256),
            now_ms,
        )? {
            if let Some(binding) = sqlite_select_ats_posting_runtime(
                tx,
                &target_evidence,
                matrix,
                "canary",
                Some(allowlist_sha256),
                runtime_attestation,
                now_ms,
            )? {
                return Ok(AtsCertificationPostingResolution {
                    status: ats_target_status_projection_for_binding(
                        provider,
                        target_key_sha256,
                        &binding,
                    ),
                    active_binding: Some(binding),
                });
            }
            return Ok(AtsCertificationPostingResolution {
                status: empty_ats_target_status_projection(
                    provider,
                    target_key_sha256,
                    "drifted".to_string(),
                ),
                active_binding: None,
            });
        }
    }
    let general_status = sqlite_ats_inactive_target_status(
        tx,
        &provider,
        &target_key,
        None,
        "general",
        None,
        now_ms,
    )?;
    let status = if general_status.status != "review_only" {
        general_status
    } else if let Some(allowlist_sha256) = allowlist_sha256.as_deref() {
        sqlite_ats_inactive_target_status(
            tx,
            &provider,
            &target_key,
            None,
            "canary",
            Some(allowlist_sha256),
            now_ms,
        )?
    } else {
        general_status
    };
    Ok(AtsCertificationPostingResolution {
        status: ats_target_status_projection_for_inactive(provider, target_key_sha256, status),
        active_binding: None,
    })
}

pub fn resolve_ats_certification_for_posting_postgres_tx(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    posting: &JobPosting,
    runtime_attestation: Option<&AtsCertificationRuntimeAttestation>,
    now_ms: i64,
) -> Result<AtsCertificationPostingResolution, AtsCertificationAuthorityError> {
    if !ats_certification_text(account_id, 1, 240) {
        return Err(AtsCertificationAuthorityError::InvalidAuthority);
    }
    if let Some(attestation) = runtime_attestation {
        validate_ats_runtime_attestation(attestation)?;
    }
    let target_evidence = ats_certification_fresh_target_evidence_from_posting(posting, now_ms)?;
    let (provider, target_key) =
        validate_ats_certification_fresh_target_evidence(&target_evidence, now_ms)?;
    let target_key_sha256 = ats_certification_target_key_sha256(&target_key)?;
    if let Some(matrix) = resolve_ats_certification_target_status_postgres_tx(
        tx,
        &target_evidence,
        None,
        "general",
        None,
        now_ms,
    )? {
        if let Some(binding) = postgres_select_ats_posting_runtime(
            tx,
            &target_evidence,
            matrix,
            "general",
            None,
            runtime_attestation,
            now_ms,
        )? {
            return Ok(AtsCertificationPostingResolution {
                status: ats_target_status_projection_for_binding(
                    provider,
                    target_key_sha256,
                    &binding,
                ),
                active_binding: Some(binding),
            });
        }
        return Ok(AtsCertificationPostingResolution {
            status: empty_ats_target_status_projection(
                provider,
                target_key_sha256,
                "drifted".to_string(),
            ),
            active_binding: None,
        });
    }
    let allowlist_sha256 = resolve_postgres_ats_canary_allowlist_for_target_account(
        tx,
        &target_evidence,
        account_id,
        now_ms,
    )?;
    if let Some(allowlist_sha256) = allowlist_sha256.as_deref() {
        if let Some(matrix) = resolve_ats_certification_target_status_postgres_tx(
            tx,
            &target_evidence,
            None,
            "canary",
            Some(allowlist_sha256),
            now_ms,
        )? {
            if let Some(binding) = postgres_select_ats_posting_runtime(
                tx,
                &target_evidence,
                matrix,
                "canary",
                Some(allowlist_sha256),
                runtime_attestation,
                now_ms,
            )? {
                return Ok(AtsCertificationPostingResolution {
                    status: ats_target_status_projection_for_binding(
                        provider,
                        target_key_sha256,
                        &binding,
                    ),
                    active_binding: Some(binding),
                });
            }
            return Ok(AtsCertificationPostingResolution {
                status: empty_ats_target_status_projection(
                    provider,
                    target_key_sha256,
                    "drifted".to_string(),
                ),
                active_binding: None,
            });
        }
    }
    let general_status = postgres_ats_inactive_target_status(
        tx,
        &provider,
        &target_key,
        None,
        "general",
        None,
        now_ms,
    )?;
    let status = if general_status.status != "review_only" {
        general_status
    } else if let Some(allowlist_sha256) = allowlist_sha256.as_deref() {
        postgres_ats_inactive_target_status(
            tx,
            &provider,
            &target_key,
            None,
            "canary",
            Some(allowlist_sha256),
            now_ms,
        )?
    } else {
        general_status
    };
    Ok(AtsCertificationPostingResolution {
        status: ats_target_status_projection_for_inactive(provider, target_key_sha256, status),
        active_binding: None,
    })
}

pub fn ats_certification_admission_projection(
    binding: &AtsCertificationBinding,
) -> Result<AtsFinalSubmitCertificationProof, AtsCertificationAuthorityError> {
    let frozen = ats_frozen_certification_admission_projection(binding)?;
    Ok(AtsFinalSubmitCertificationProof {
        schema_version: frozen.schema_version,
        provider: frozen.provider,
        adapter_version: frozen.adapter_version,
        manifest_sha256: frozen.manifest_sha256,
        activation_sha256: frozen.activation_sha256,
        activation_generation: frozen.activation_generation,
        target_key_sha256: frozen.target_key_sha256,
        layout_set_sha256: frozen.layout_set_sha256,
        adapter_bundle_sha256: frozen.adapter_bundle_sha256,
        runner_target_sha256s: frozen.runner_target_sha256s,
        expires_at_ms: frozen.expires_at_ms,
    })
}

pub fn ats_frozen_certification_admission_projection(
    binding: &AtsCertificationBinding,
) -> Result<AtsFrozenCertificationAdmissionProjection, AtsCertificationAuthorityError> {
    if binding.binding_version != 1
        || !matches!(binding.channel.as_str(), "canary" | "general")
        || binding.capability != "unattended_submit"
        || binding.runtime_targets.is_empty()
        || !ats_manifest_provider_hosts_match_target(
            &binding.provider,
            &binding.target_key,
            &binding.allowed_provider_hosts,
        )
        || !ats_exact_final_submit_control(&binding.provider, &binding.final_submit_control_id)
    {
        return Err(AtsCertificationAuthorityError::ScopeMismatch);
    }
    let mut runner_target_sha256s = binding
        .runtime_targets
        .iter()
        .map(|runtime| runtime.runtime_sha256.clone())
        .collect::<Vec<_>>();
    if runner_target_sha256s
        .iter()
        .any(|value| !ats_certification_hex64(value))
    {
        return Err(AtsCertificationAuthorityError::RuntimeMismatch);
    }
    runner_target_sha256s.sort();
    runner_target_sha256s.dedup();
    if runner_target_sha256s.len() != binding.runtime_targets.len() {
        return Err(AtsCertificationAuthorityError::RuntimeMismatch);
    }
    Ok(AtsFrozenCertificationAdmissionProjection {
        schema_version: 1,
        provider: binding.provider.clone(),
        adapter_version: binding.adapter_version.clone(),
        manifest_sha256: binding.manifest_sha256.clone(),
        activation_sha256: binding.activation_sha256.clone(),
        activation_generation: binding.activation_generation,
        target_key_sha256: ats_certification_target_key_sha256(&binding.target_key)?,
        layout_set_sha256: binding.layout_set_sha256.clone(),
        variant_key: binding.variant_key.clone(),
        layout_contract_version: binding.layout_contract_version,
        surface_sha256: binding.surface_sha256.clone(),
        adapter_bundle_sha256: binding.adapter_bundle_sha256.clone(),
        runner_target_sha256s,
        expires_at_ms: binding.expires_at_ms,
    })
}

fn validate_ats_target_status_request(
    runner: Option<&str>,
    channel: &str,
    expected_account_allowlist_sha256: Option<&str>,
) -> Result<(), AtsCertificationAuthorityError> {
    if !matches!(channel, "canary" | "general" | "shadow")
        || runner.is_some_and(|value| !ats_certification_hex64(value))
        || expected_account_allowlist_sha256.is_some_and(|value| !ats_certification_hex64(value))
    {
        return Err(AtsCertificationAuthorityError::InvalidAuthority);
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn resolve_ats_certification_target_status_sqlite_tx(
    tx: &rusqlite::Transaction<'_>,
    target_evidence: &AtsCertificationFreshTargetEvidence,
    runner: Option<&str>,
    channel: &str,
    expected_account_allowlist_sha256: Option<&str>,
    now_ms: i64,
) -> Result<Option<AtsCertificationBinding>, AtsCertificationAuthorityError> {
    validate_ats_target_status_request(runner, channel, expected_account_allowlist_sha256)?;
    let (provider, target_key) =
        validate_ats_certification_fresh_target_evidence(target_evidence, now_ms)?;
    let mut stmt = tx
        .prepare(
            "SELECT activation.activation_sha256, activation.activation_id,
                    activation.activation_generation, activation.manifest_sha256,
                    activation.scope_sha256, activation.channel,
                    activation.channel_sequence, activation.capability,
                    activation.account_allowlist_sha256,
                    activation.canary_max_submissions, activation.canary_account_cap,
                    activation.canary_concurrency_cap,
                    activation.canary_daily_side_effect_cap,
                    activation.not_before_ms, activation.expires_at_ms,
                    manifest.provider, manifest.target_key, manifest.variant_key,
                    manifest.surface_sha256, manifest.certification_id,
                    manifest.manifest_generation, manifest.adapter_version,
                    manifest.adapter_bundle_sha256, manifest.source_commit,
                    manifest.layout_contract_version, manifest.layout_contract_sha256,
                    manifest.not_before_ms, manifest.expires_at_ms, head.head_revision,
                    head.current_transition_sha256, head.current_activation_sha256,
                    head.current_channel_sequence, head.updated_by,
                    activation.trust_policy_sha256, manifest.layout_set_sha256,
                    manifest.allowed_provider_hosts_json,
                    manifest.final_submit_control_id, manifest.tested_at_ms
               FROM jobs_ats_certification_heads head
               JOIN jobs_ats_certification_activations activation
                 ON activation.activation_sha256 = head.current_activation_sha256
               JOIN jobs_ats_certification_manifests manifest
                 ON manifest.manifest_sha256 = activation.manifest_sha256
                AND manifest.trust_policy_sha256 = activation.trust_policy_sha256
               JOIN jobs_ats_certification_trust_head trust_head
                 ON trust_head.singleton_id = 1
                AND trust_head.current_policy_sha256 = activation.trust_policy_sha256
               JOIN jobs_ats_certification_trust_policies trust_policy
                 ON trust_policy.policy_sha256 = trust_head.current_policy_sha256
              WHERE head.channel = ?1 AND manifest.provider = ?2
                AND manifest.target_key = ?3
                AND trust_policy.valid_from_ms <= ?4 AND trust_policy.expires_at_ms > ?4
              ORDER BY manifest.manifest_generation DESC LIMIT 2",
        )
        .map_err(ats_certification_storage)?;
    let rows = stmt
        .query_map(
            params![channel, provider, target_key, now_ms],
            sqlite_ats_binding_row,
        )
        .map_err(ats_certification_storage)?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(ats_certification_storage)?;
    if rows.len() != 1 {
        return Ok(None);
    }
    let row = rows
        .into_iter()
        .next()
        .ok_or(AtsCertificationAuthorityError::IdentityConflict)?;
    let surface = AtsObservedSurface {
        variant_key: row.variant_key.clone(),
        layout_contract_version: row.layout_contract_version,
        surface_sha256: row.surface_sha256.clone(),
    };
    let resolved = resolve_sqlite_ats_certification_tx(
        tx,
        &provider,
        &target_key,
        &row.activation.scope_sha256,
        &surface,
        channel,
        expected_account_allowlist_sha256,
        runner,
        now_ms,
    )?;
    Ok(resolved.filter(|binding| {
        ats_certification_binding_matches_url(binding, &target_evidence.canonical_url)
    }))
}

#[allow(clippy::too_many_arguments)]
pub fn resolve_ats_certification_target_status_postgres_tx(
    tx: &mut postgres::Transaction<'_>,
    target_evidence: &AtsCertificationFreshTargetEvidence,
    runner: Option<&str>,
    channel: &str,
    expected_account_allowlist_sha256: Option<&str>,
    now_ms: i64,
) -> Result<Option<AtsCertificationBinding>, AtsCertificationAuthorityError> {
    validate_ats_target_status_request(runner, channel, expected_account_allowlist_sha256)?;
    let (provider, target_key) =
        validate_ats_certification_fresh_target_evidence(target_evidence, now_ms)?;
    let rows = tx
        .query(
            "SELECT activation.activation_sha256, activation.activation_id,
                    activation.activation_generation, activation.manifest_sha256,
                    activation.scope_sha256, activation.channel,
                    activation.channel_sequence, activation.capability,
                    activation.account_allowlist_sha256,
                    activation.canary_max_submissions, activation.canary_account_cap,
                    activation.canary_concurrency_cap,
                    activation.canary_daily_side_effect_cap,
                    activation.not_before_ms, activation.expires_at_ms,
                    manifest.provider, manifest.target_key, manifest.variant_key,
                    manifest.surface_sha256, manifest.certification_id,
                    manifest.manifest_generation, manifest.adapter_version,
                    manifest.adapter_bundle_sha256, manifest.source_commit,
                    manifest.layout_contract_version, manifest.layout_contract_sha256,
                    manifest.not_before_ms, manifest.expires_at_ms, head.head_revision,
                    head.current_transition_sha256, head.current_activation_sha256,
                    head.current_channel_sequence, head.updated_by,
                    activation.trust_policy_sha256, manifest.layout_set_sha256,
                    manifest.allowed_provider_hosts_json,
                    manifest.final_submit_control_id, manifest.tested_at_ms
               FROM jobs_ats_certification_heads head
               JOIN jobs_ats_certification_activations activation
                 ON activation.activation_sha256 = head.current_activation_sha256
               JOIN jobs_ats_certification_manifests manifest
                 ON manifest.manifest_sha256 = activation.manifest_sha256
                AND manifest.trust_policy_sha256 = activation.trust_policy_sha256
               JOIN jobs_ats_certification_trust_head trust_head
                 ON trust_head.singleton_id = 1
                AND trust_head.current_policy_sha256 = activation.trust_policy_sha256
               JOIN jobs_ats_certification_trust_policies trust_policy
                 ON trust_policy.policy_sha256 = trust_head.current_policy_sha256
              WHERE head.channel = $1 AND manifest.provider = $2
                AND manifest.target_key = $3
                AND trust_policy.valid_from_ms <= $4 AND trust_policy.expires_at_ms > $4
              ORDER BY manifest.manifest_generation DESC LIMIT 2",
            &[&channel, &provider, &target_key, &now_ms],
        )
        .map_err(ats_certification_storage)?;
    if rows.len() != 1 {
        return Ok(None);
    }
    let row = postgres_ats_binding_row(
        rows.into_iter()
            .next()
            .ok_or(AtsCertificationAuthorityError::IdentityConflict)?,
    );
    let surface = AtsObservedSurface {
        variant_key: row.variant_key.clone(),
        layout_contract_version: row.layout_contract_version,
        surface_sha256: row.surface_sha256.clone(),
    };
    let resolved = resolve_postgres_ats_certification_tx(
        tx,
        &provider,
        &target_key,
        &row.activation.scope_sha256,
        &surface,
        channel,
        expected_account_allowlist_sha256,
        runner,
        now_ms,
    )?;
    Ok(resolved.filter(|binding| {
        ats_certification_binding_matches_url(binding, &target_evidence.canonical_url)
    }))
}

pub fn resolve_active_ats_certification(
    pool: &DbPool,
    canonical_url: &str,
    runner: Option<&str>,
    observed_surface: Option<&AtsObservedSurface>,
    now_ms: i64,
) -> Result<Option<AtsCertificationBinding>, AtsCertificationAuthorityError> {
    resolve_active_ats_certification_for_channel(
        pool,
        canonical_url,
        runner,
        observed_surface,
        "general",
        None,
        now_ms,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn resolve_active_ats_certification_for_channel(
    pool: &DbPool,
    canonical_url: &str,
    runner: Option<&str>,
    observed_surface: Option<&AtsObservedSurface>,
    channel: &str,
    expected_account_allowlist_sha256: Option<&str>,
    now_ms: i64,
) -> Result<Option<AtsCertificationBinding>, AtsCertificationAuthorityError> {
    if !matches!(channel, "canary" | "general" | "shadow")
        || !ats_certification_safe_integer(now_ms, false)
        || runner.is_some_and(|value| !ats_certification_hex64(value))
        || expected_account_allowlist_sha256.is_some_and(|value| !ats_certification_hex64(value))
    {
        return Err(AtsCertificationAuthorityError::InvalidAuthority);
    }
    let Some(surface) = observed_surface else {
        return Ok(None);
    };
    validate_ats_observed_surface(surface)?;
    let (provider, target_key) = ats_certification_target_from_url(canonical_url)?;
    let scope_sha256 = ats_certification_scope_sha256(
        &provider,
        &target_key,
        &surface.variant_key,
        &surface.surface_sha256,
    )?;
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get().map_err(ats_certification_storage)?;
            let tx = conn
                .transaction_with_behavior(TransactionBehavior::Deferred)
                .map_err(ats_certification_storage)?;
            let result = resolve_sqlite_ats_certification_tx(
                &tx,
                &provider,
                &target_key,
                &scope_sha256,
                surface,
                channel,
                expected_account_allowlist_sha256,
                runner,
                now_ms,
            )?;
            tx.commit().map_err(ats_certification_storage)?;
            Ok(result
                .filter(|binding| ats_certification_binding_matches_url(binding, canonical_url)))
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg().map_err(ats_certification_storage)?;
            let mut tx = conn
                .build_transaction()
                .isolation_level(postgres::IsolationLevel::RepeatableRead)
                .read_only(true)
                .start()
                .map_err(ats_certification_storage)?;
            let result = resolve_postgres_ats_certification_tx(
                &mut tx,
                &provider,
                &target_key,
                &scope_sha256,
                surface,
                channel,
                expected_account_allowlist_sha256,
                runner,
                now_ms,
            )?;
            tx.commit().map_err(ats_certification_storage)?;
            Ok(result
                .filter(|binding| ats_certification_binding_matches_url(binding, canonical_url)))
        }
    })
}

#[allow(clippy::too_many_arguments)]
fn resolve_sqlite_ats_certification_tx(
    tx: &rusqlite::Transaction<'_>,
    provider: &str,
    target_key: &str,
    scope_sha256: &str,
    surface: &AtsObservedSurface,
    channel: &str,
    expected_account_allowlist_sha256: Option<&str>,
    runner: Option<&str>,
    now_ms: i64,
) -> Result<Option<AtsCertificationBinding>, AtsCertificationAuthorityError> {
    let row = tx
        .query_row(
            "SELECT activation.activation_sha256, activation.activation_id,
                    activation.activation_generation, activation.manifest_sha256,
                    activation.scope_sha256, activation.channel,
                    activation.channel_sequence, activation.capability,
                    activation.account_allowlist_sha256,
                    activation.canary_max_submissions, activation.canary_account_cap,
                    activation.canary_concurrency_cap,
                    activation.canary_daily_side_effect_cap,
                    activation.not_before_ms, activation.expires_at_ms,
                    manifest.provider, manifest.target_key,
                    manifest.variant_key, manifest.surface_sha256,
                    manifest.certification_id, manifest.manifest_generation,
                    manifest.adapter_version, manifest.adapter_bundle_sha256,
                    manifest.source_commit, manifest.layout_contract_version,
                    manifest.layout_contract_sha256, manifest.not_before_ms,
                    manifest.expires_at_ms, head.head_revision,
                    head.current_transition_sha256, head.current_activation_sha256,
                    head.current_channel_sequence, head.updated_by,
                    activation.trust_policy_sha256, manifest.layout_set_sha256,
                    manifest.allowed_provider_hosts_json,
                    manifest.final_submit_control_id, manifest.tested_at_ms
               FROM jobs_ats_certification_heads head
               JOIN jobs_ats_certification_activations activation
                 ON activation.activation_sha256 = head.current_activation_sha256
               JOIN jobs_ats_certification_manifests manifest
                 ON manifest.manifest_sha256 = activation.manifest_sha256
                AND manifest.trust_policy_sha256 = activation.trust_policy_sha256
               JOIN jobs_ats_certification_trust_head trust_head
                 ON trust_head.singleton_id = 1
                AND trust_head.current_policy_sha256 = activation.trust_policy_sha256
               JOIN jobs_ats_certification_trust_policies trust_policy
                 ON trust_policy.policy_sha256 = trust_head.current_policy_sha256
              WHERE head.scope_sha256 = ?1 AND head.channel = ?2
                AND manifest.provider = ?3 AND manifest.target_key = ?4
                AND manifest.variant_key = ?5 AND manifest.surface_sha256 = ?6
                AND manifest.layout_contract_version = ?7
                AND trust_policy.valid_from_ms <= ?8 AND trust_policy.expires_at_ms > ?8",
            params![
                scope_sha256,
                channel,
                provider,
                target_key,
                surface.variant_key,
                surface.surface_sha256,
                surface.layout_contract_version,
                now_ms,
            ],
            sqlite_ats_binding_row,
        )
        .optional()
        .map_err(ats_certification_storage)?;
    let Some(row) = row else {
        return Ok(None);
    };
    resolve_sqlite_ats_binding_row(tx, row, expected_account_allowlist_sha256, runner, now_ms)
}

fn sqlite_ats_binding_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<StoredAtsBindingRow> {
    Ok(StoredAtsBindingRow {
        activation: StoredAtsActivationHeadAuthority {
            activation_sha256: row.get(0)?,
            activation_id: row.get(1)?,
            activation_generation: row.get(2)?,
            manifest_sha256: row.get(3)?,
            scope_sha256: row.get(4)?,
            channel: row.get(5)?,
            channel_sequence: row.get(6)?,
            capability: row.get(7)?,
            account_allowlist_sha256: row.get(8)?,
            canary_max_submissions: row.get(9)?,
            canary_account_cap: row.get(10)?,
            canary_concurrency_cap: row.get(11)?,
            canary_daily_side_effect_cap: row.get(12)?,
            not_before_ms: row.get(13)?,
            expires_at_ms: row.get(14)?,
            trust_policy_sha256: row.get(33)?,
        },
        provider: row.get(15)?,
        target_key: row.get(16)?,
        allowed_provider_hosts_json: row.get(35)?,
        variant_key: row.get(17)?,
        surface_sha256: row.get(18)?,
        certification_id: row.get(19)?,
        manifest_generation: row.get(20)?,
        adapter_version: row.get(21)?,
        final_submit_control_id: row.get(36)?,
        adapter_bundle_sha256: row.get(22)?,
        source_commit: row.get(23)?,
        layout_contract_version: row.get(24)?,
        layout_contract_sha256: row.get(25)?,
        layout_set_sha256: row.get(34)?,
        last_verified_at_ms: row.get(37)?,
        manifest_not_before_ms: row.get(26)?,
        manifest_expires_at_ms: row.get(27)?,
        head: StoredAtsHead {
            head_revision: row.get(28)?,
            transition_sha256: row.get(29)?,
            activation_sha256: row.get(30)?,
            channel_sequence: row.get(31)?,
            updated_by: row.get(32)?,
        },
    })
}

fn resolve_sqlite_ats_binding_row(
    tx: &rusqlite::Transaction<'_>,
    row: StoredAtsBindingRow,
    expected_account_allowlist_sha256: Option<&str>,
    runner: Option<&str>,
    now_ms: i64,
) -> Result<Option<AtsCertificationBinding>, AtsCertificationAuthorityError> {
    if !ats_binding_row_channel_allowed(&row, expected_account_allowlist_sha256, now_ms) {
        return Ok(None);
    }
    if let Err(error) = ensure_sqlite_ats_activation_available(tx, &row.activation, now_ms) {
        return ats_unavailable_resolution(error);
    }
    if sqlite_ats_binding_circuit_open(tx, &row)? {
        return Ok(None);
    }
    let evidence_sha256s = sqlite_ats_manifest_evidence(tx, &row.activation.manifest_sha256)?;
    let layout_observation_sha256s =
        sqlite_ats_manifest_layouts(tx, &row.activation.manifest_sha256)?;
    let runtimes = sqlite_ats_runtime_targets(tx, &row.activation.manifest_sha256)?;
    let runtimes = sqlite_available_ats_runtimes(
        tx,
        runtimes,
        &row.activation.trust_policy_sha256,
        now_ms,
        true,
    )?;
    ats_binding_from_row(
        row,
        evidence_sha256s,
        layout_observation_sha256s,
        runtimes,
        runner,
    )
}

#[allow(clippy::too_many_arguments)]
fn resolve_postgres_ats_certification_tx(
    tx: &mut postgres::Transaction<'_>,
    provider: &str,
    target_key: &str,
    scope_sha256: &str,
    surface: &AtsObservedSurface,
    channel: &str,
    expected_account_allowlist_sha256: Option<&str>,
    runner: Option<&str>,
    now_ms: i64,
) -> Result<Option<AtsCertificationBinding>, AtsCertificationAuthorityError> {
    let row = tx
        .query_opt(
            "SELECT activation.activation_sha256, activation.activation_id,
                    activation.activation_generation, activation.manifest_sha256,
                    activation.scope_sha256, activation.channel,
                    activation.channel_sequence, activation.capability,
                    activation.account_allowlist_sha256,
                    activation.canary_max_submissions, activation.canary_account_cap,
                    activation.canary_concurrency_cap,
                    activation.canary_daily_side_effect_cap,
                    activation.not_before_ms, activation.expires_at_ms,
                    manifest.provider, manifest.target_key,
                    manifest.variant_key, manifest.surface_sha256,
                    manifest.certification_id, manifest.manifest_generation,
                    manifest.adapter_version, manifest.adapter_bundle_sha256,
                    manifest.source_commit, manifest.layout_contract_version,
                    manifest.layout_contract_sha256, manifest.not_before_ms,
                    manifest.expires_at_ms, head.head_revision,
                    head.current_transition_sha256, head.current_activation_sha256,
                    head.current_channel_sequence, head.updated_by,
                    activation.trust_policy_sha256, manifest.layout_set_sha256,
                    manifest.allowed_provider_hosts_json,
                    manifest.final_submit_control_id, manifest.tested_at_ms
               FROM jobs_ats_certification_heads head
               JOIN jobs_ats_certification_activations activation
                 ON activation.activation_sha256 = head.current_activation_sha256
               JOIN jobs_ats_certification_manifests manifest
                 ON manifest.manifest_sha256 = activation.manifest_sha256
                AND manifest.trust_policy_sha256 = activation.trust_policy_sha256
               JOIN jobs_ats_certification_trust_head trust_head
                 ON trust_head.singleton_id = 1
                AND trust_head.current_policy_sha256 = activation.trust_policy_sha256
               JOIN jobs_ats_certification_trust_policies trust_policy
                 ON trust_policy.policy_sha256 = trust_head.current_policy_sha256
              WHERE head.scope_sha256 = $1 AND head.channel = $2
                AND manifest.provider = $3 AND manifest.target_key = $4
                AND manifest.variant_key = $5 AND manifest.surface_sha256 = $6
                AND manifest.layout_contract_version = $7
                AND trust_policy.valid_from_ms <= $8 AND trust_policy.expires_at_ms > $8",
            &[
                &scope_sha256,
                &channel,
                &provider,
                &target_key,
                &surface.variant_key,
                &surface.surface_sha256,
                &surface.layout_contract_version,
                &now_ms,
            ],
        )
        .map_err(ats_certification_storage)?
        .map(postgres_ats_binding_row);
    let Some(row) = row else {
        return Ok(None);
    };
    if !ats_binding_row_channel_allowed(&row, expected_account_allowlist_sha256, now_ms) {
        return Ok(None);
    }
    if let Err(error) = ensure_postgres_ats_activation_available(tx, &row.activation, now_ms) {
        return ats_unavailable_resolution(error);
    }
    if postgres_ats_binding_circuit_open(tx, &row)? {
        return Ok(None);
    }
    let evidence_sha256s = postgres_ats_manifest_evidence(tx, &row.activation.manifest_sha256)?;
    let layout_observation_sha256s =
        postgres_ats_manifest_layouts(tx, &row.activation.manifest_sha256)?;
    let runtimes = postgres_ats_runtime_targets(tx, &row.activation.manifest_sha256)?;
    let runtimes = postgres_available_ats_runtimes(
        tx,
        runtimes,
        &row.activation.trust_policy_sha256,
        now_ms,
        true,
    )?;
    ats_binding_from_row(
        row,
        evidence_sha256s,
        layout_observation_sha256s,
        runtimes,
        runner,
    )
}

fn postgres_ats_binding_row(row: postgres::Row) -> StoredAtsBindingRow {
    StoredAtsBindingRow {
        activation: StoredAtsActivationHeadAuthority {
            activation_sha256: row.get(0),
            activation_id: row.get(1),
            activation_generation: row.get(2),
            manifest_sha256: row.get(3),
            scope_sha256: row.get(4),
            channel: row.get(5),
            channel_sequence: row.get(6),
            capability: row.get(7),
            account_allowlist_sha256: row.get(8),
            canary_max_submissions: row.get(9),
            canary_account_cap: row.get(10),
            canary_concurrency_cap: row.get(11),
            canary_daily_side_effect_cap: row.get(12),
            not_before_ms: row.get(13),
            expires_at_ms: row.get(14),
            trust_policy_sha256: row.get(33),
        },
        provider: row.get(15),
        target_key: row.get(16),
        allowed_provider_hosts_json: row.get(35),
        variant_key: row.get(17),
        surface_sha256: row.get(18),
        certification_id: row.get(19),
        manifest_generation: row.get(20),
        adapter_version: row.get(21),
        final_submit_control_id: row.get(36),
        adapter_bundle_sha256: row.get(22),
        source_commit: row.get(23),
        layout_contract_version: row.get(24),
        layout_contract_sha256: row.get(25),
        layout_set_sha256: row.get(34),
        last_verified_at_ms: row.get(37),
        manifest_not_before_ms: row.get(26),
        manifest_expires_at_ms: row.get(27),
        head: StoredAtsHead {
            head_revision: row.get(28),
            transition_sha256: row.get(29),
            activation_sha256: row.get(30),
            channel_sequence: row.get(31),
            updated_by: row.get(32),
        },
    }
}

fn ats_binding_row_channel_allowed(
    row: &StoredAtsBindingRow,
    expected_account_allowlist_sha256: Option<&str>,
    now_ms: i64,
) -> bool {
    now_ms >= row.activation.not_before_ms
        && now_ms < row.activation.expires_at_ms
        && now_ms >= row.manifest_not_before_ms
        && now_ms < row.manifest_expires_at_ms
        && row.head.activation_sha256 == row.activation.activation_sha256
        && row.head.channel_sequence == row.activation.channel_sequence
        && match row.activation.channel.as_str() {
            "canary" => {
                expected_account_allowlist_sha256
                    == row.activation.account_allowlist_sha256.as_deref()
            }
            "general" | "shadow" => expected_account_allowlist_sha256.is_none(),
            _ => false,
        }
        && (row.activation.channel == "shadow"
            || (ats_exact_submit_adapter(&row.provider, &row.adapter_version)
                && ats_exact_provider_target_key(&row.provider, &row.target_key)))
}

fn ats_binding_from_row(
    row: StoredAtsBindingRow,
    evidence_sha256s: Vec<String>,
    layout_observation_sha256s: Vec<String>,
    runtime_targets: Vec<AtsCertificationRuntimeTarget>,
    runner: Option<&str>,
) -> Result<Option<AtsCertificationBinding>, AtsCertificationAuthorityError> {
    if evidence_sha256s.is_empty() || runtime_targets.is_empty() {
        return Ok(None);
    }
    let allowed_provider_hosts =
        serde_json::from_str::<Vec<String>>(&row.allowed_provider_hosts_json)
            .map_err(|_| AtsCertificationAuthorityError::InvalidAuthority)?;
    if serde_json::to_string(&allowed_provider_hosts)
        .map_err(|_| AtsCertificationAuthorityError::InvalidAuthority)?
        != row.allowed_provider_hosts_json
        || !ats_manifest_provider_hosts_match_target(
            &row.provider,
            &row.target_key,
            &allowed_provider_hosts,
        )
        || !ats_exact_final_submit_control(&row.provider, &row.final_submit_control_id)
    {
        return Err(AtsCertificationAuthorityError::InvalidAuthority);
    }
    let selected_runtime = match runner {
        Some(runtime_target_sha256) => match runtime_targets
            .iter()
            .find(|runtime| runtime.runtime_sha256 == runtime_target_sha256)
        {
            Some(runtime) => Some(runtime.clone()),
            None => return Ok(None),
        },
        None => None,
    };
    let not_before_ms = row.activation.not_before_ms.max(row.manifest_not_before_ms);
    let expires_at_ms = row.activation.expires_at_ms.min(row.manifest_expires_at_ms);
    if row.last_verified_at_ms > expires_at_ms {
        return Err(AtsCertificationAuthorityError::InvalidAuthority);
    }
    Ok(Some(AtsCertificationBinding {
        binding_version: 1,
        trust_policy_sha256: row.activation.trust_policy_sha256,
        provider: row.provider,
        target_key: row.target_key,
        allowed_provider_hosts,
        variant_key: row.variant_key,
        surface_sha256: row.surface_sha256,
        scope_sha256: row.activation.scope_sha256,
        manifest_sha256: row.activation.manifest_sha256,
        certification_id: row.certification_id,
        manifest_generation: row.manifest_generation,
        activation_sha256: row.activation.activation_sha256,
        activation_id: row.activation.activation_id,
        activation_generation: row.activation.activation_generation,
        channel: row.activation.channel,
        channel_sequence: row.activation.channel_sequence,
        channel_head_revision: row.head.head_revision,
        channel_transition_sha256: row.head.transition_sha256,
        capability: row.activation.capability,
        account_allowlist_sha256: row.activation.account_allowlist_sha256,
        canary_max_submissions: row.activation.canary_max_submissions,
        canary_account_cap: row.activation.canary_account_cap,
        canary_concurrency_cap: row.activation.canary_concurrency_cap,
        canary_daily_side_effect_cap: row.activation.canary_daily_side_effect_cap,
        adapter_version: row.adapter_version,
        final_submit_control_id: row.final_submit_control_id,
        adapter_bundle_sha256: row.adapter_bundle_sha256,
        source_commit: row.source_commit,
        layout_contract_version: row.layout_contract_version,
        layout_contract_sha256: row.layout_contract_sha256,
        layout_set_sha256: row.layout_set_sha256,
        layout_observation_sha256s,
        evidence_sha256s,
        runtime_targets,
        selected_runtime,
        not_before_ms,
        expires_at_ms,
        last_verified_at_ms: row.last_verified_at_ms,
    }))
}

fn ats_unavailable_resolution(
    error: AtsCertificationAuthorityError,
) -> Result<Option<AtsCertificationBinding>, AtsCertificationAuthorityError> {
    match error {
        AtsCertificationAuthorityError::Expired
        | AtsCertificationAuthorityError::Quarantined
        | AtsCertificationAuthorityError::Revoked
        | AtsCertificationAuthorityError::CircuitOpen => Ok(None),
        other => Err(other),
    }
}

fn sqlite_ats_manifest_evidence(
    tx: &rusqlite::Transaction<'_>,
    manifest_sha256: &str,
) -> Result<Vec<String>, AtsCertificationAuthorityError> {
    let mut stmt = tx
        .prepare(
            "SELECT evidence_sha256 FROM jobs_ats_certification_manifest_evidence
              WHERE manifest_sha256 = ?1 ORDER BY ordinal",
        )
        .map_err(ats_certification_storage)?;
    let values = stmt
        .query_map(params![manifest_sha256], |row| row.get(0))
        .map_err(ats_certification_storage)?
        .collect::<std::result::Result<Vec<String>, _>>()
        .map_err(ats_certification_storage)?;
    Ok(values)
}

fn postgres_ats_manifest_evidence(
    tx: &mut postgres::Transaction<'_>,
    manifest_sha256: &str,
) -> Result<Vec<String>, AtsCertificationAuthorityError> {
    Ok(tx
        .query(
            "SELECT evidence_sha256 FROM jobs_ats_certification_manifest_evidence
              WHERE manifest_sha256 = $1 ORDER BY ordinal",
            &[&manifest_sha256],
        )
        .map_err(ats_certification_storage)?
        .into_iter()
        .map(|row| row.get(0))
        .collect())
}

fn sqlite_ats_manifest_layouts(
    tx: &rusqlite::Transaction<'_>,
    manifest_sha256: &str,
) -> Result<Vec<String>, AtsCertificationAuthorityError> {
    let mut stmt = tx
        .prepare(
            "SELECT observation_sha256 FROM jobs_ats_certification_manifest_layouts
              WHERE manifest_sha256 = ?1 ORDER BY ordinal",
        )
        .map_err(ats_certification_storage)?;
    let values = stmt
        .query_map(params![manifest_sha256], |row| row.get(0))
        .map_err(ats_certification_storage)?
        .collect::<std::result::Result<Vec<String>, _>>()
        .map_err(ats_certification_storage)?;
    Ok(values)
}

fn postgres_ats_manifest_layouts(
    tx: &mut postgres::Transaction<'_>,
    manifest_sha256: &str,
) -> Result<Vec<String>, AtsCertificationAuthorityError> {
    Ok(tx
        .query(
            "SELECT observation_sha256 FROM jobs_ats_certification_manifest_layouts
              WHERE manifest_sha256 = $1 ORDER BY ordinal",
            &[&manifest_sha256],
        )
        .map_err(ats_certification_storage)?
        .into_iter()
        .map(|row| row.get(0))
        .collect())
}

fn sqlite_ats_binding_circuit_open(
    tx: &rusqlite::Transaction<'_>,
    row: &StoredAtsBindingRow,
) -> Result<bool, AtsCertificationAuthorityError> {
    let open: i64 = tx
        .query_row(
            "SELECT EXISTS(
               SELECT 1 FROM jobs_ats_certification_circuit_heads
                WHERE state <> 'closed' AND (
                  (scope_kind = 'provider' AND subject_key = ?1)
                  OR (scope_kind = 'target' AND subject_key = ?2)
                  OR (scope_kind = 'adapter' AND subject_key = ?3)
                  OR (scope_kind = 'activation' AND subject_key = ?4)
                )
             )",
            params![
                row.provider,
                row.target_key,
                row.adapter_bundle_sha256,
                row.activation.activation_sha256,
            ],
            |row| row.get(0),
        )
        .map_err(ats_certification_storage)?;
    Ok(open != 0)
}

fn postgres_ats_binding_circuit_open(
    tx: &mut postgres::Transaction<'_>,
    row: &StoredAtsBindingRow,
) -> Result<bool, AtsCertificationAuthorityError> {
    Ok(tx
        .query_one(
            "SELECT EXISTS(
               SELECT 1 FROM jobs_ats_certification_circuit_heads
                WHERE state <> 'closed' AND (
                  (scope_kind = 'provider' AND subject_key = $1)
                  OR (scope_kind = 'target' AND subject_key = $2)
                  OR (scope_kind = 'adapter' AND subject_key = $3)
                  OR (scope_kind = 'activation' AND subject_key = $4)
                )
             )",
            &[
                &row.provider,
                &row.target_key,
                &row.adapter_bundle_sha256,
                &row.activation.activation_sha256,
            ],
        )
        .map_err(ats_certification_storage)?
        .get(0))
}

fn sqlite_available_ats_runtimes(
    tx: &rusqlite::Transaction<'_>,
    runtimes: Vec<AtsCertificationRuntimeTarget>,
    trust_policy_sha256: &str,
    now_ms: i64,
    include_circuit_state: bool,
) -> Result<Vec<AtsCertificationRuntimeTarget>, AtsCertificationAuthorityError> {
    let mut available = Vec::with_capacity(runtimes.len());
    for runtime in runtimes {
        let runner_build_sha256 = runtime
            .runner_build_id
            .as_deref()
            .map(|runner_build_id| ats_certification_sha256(runner_build_id.as_bytes()));
        let unavailable: i64 = tx
            .query_row(
                "SELECT
                   EXISTS(SELECT 1 FROM jobs_ats_certification_revocations
                     WHERE effective_at_ms <= ?3 AND trust_policy_sha256 = ?4 AND (
                       (subject_kind = 'runtime' AND subject_id = ?1
                         AND subject_sha256 = ?2)
                       OR (subject_kind = 'browser_release_manifest' AND ?5 IS NOT NULL
                         AND subject_id = ?5 AND subject_sha256 = ?5)
                       OR (subject_kind = 'runner_image' AND ?6 IS NOT NULL
                         AND subject_id = ?6 AND subject_sha256 = ?6)
                       OR (subject_kind = 'runner_build' AND ?7 IS NOT NULL
                         AND subject_id = ?7 AND subject_sha256 = ?8)
                     ))
                   OR EXISTS(
                     SELECT 1 FROM jobs_ats_certification_quarantine_heads head
                     JOIN jobs_ats_certification_quarantine_commands command
                       ON command.command_sha256 = head.current_command_sha256
                    WHERE head.scope_kind = 'runtime' AND head.scope_id = ?1
                      AND head.scope_sha256 = ?2 AND head.state = 'quarantined'
                      AND command.trust_policy_sha256 = ?4)
                   OR (?9 AND EXISTS(
                     SELECT 1 FROM jobs_ats_certification_circuit_heads head
                      WHERE head.scope_kind = 'runtime'
                        AND head.subject_key IN (?1, ?2)
                        AND head.state <> 'closed'))",
                params![
                    runtime.runtime_id,
                    runtime.runtime_sha256,
                    now_ms,
                    trust_policy_sha256,
                    runtime.browser_release_manifest_sha256.as_deref(),
                    runtime.runner_image_sha256.as_deref(),
                    runtime.runner_build_id.as_deref(),
                    runner_build_sha256,
                    include_circuit_state,
                ],
                |row| row.get(0),
            )
            .map_err(ats_certification_storage)?;
        if unavailable == 0 {
            available.push(runtime);
        }
    }
    Ok(available)
}

fn postgres_available_ats_runtimes(
    tx: &mut postgres::Transaction<'_>,
    runtimes: Vec<AtsCertificationRuntimeTarget>,
    trust_policy_sha256: &str,
    now_ms: i64,
    include_circuit_state: bool,
) -> Result<Vec<AtsCertificationRuntimeTarget>, AtsCertificationAuthorityError> {
    let mut available = Vec::with_capacity(runtimes.len());
    for runtime in runtimes {
        let runner_build_sha256 = runtime
            .runner_build_id
            .as_deref()
            .map(|runner_build_id| ats_certification_sha256(runner_build_id.as_bytes()));
        let unavailable: bool = tx
            .query_one(
                "SELECT
                   EXISTS(SELECT 1 FROM jobs_ats_certification_revocations
                     WHERE effective_at_ms <= $3 AND trust_policy_sha256 = $4 AND (
                       (subject_kind = 'runtime' AND subject_id = $1
                         AND subject_sha256 = $2)
                       OR (subject_kind = 'browser_release_manifest' AND $5::TEXT IS NOT NULL
                         AND subject_id = $5 AND subject_sha256 = $5)
                       OR (subject_kind = 'runner_image' AND $6::TEXT IS NOT NULL
                         AND subject_id = $6 AND subject_sha256 = $6)
                       OR (subject_kind = 'runner_build' AND $7::TEXT IS NOT NULL
                         AND subject_id = $7 AND subject_sha256 = $8)
                     ))
                   OR EXISTS(
                     SELECT 1 FROM jobs_ats_certification_quarantine_heads head
                     JOIN jobs_ats_certification_quarantine_commands command
                       ON command.command_sha256 = head.current_command_sha256
                    WHERE head.scope_kind = 'runtime' AND head.scope_id = $1
                      AND head.scope_sha256 = $2 AND head.state = 'quarantined'
                      AND command.trust_policy_sha256 = $4)
                   OR ($9 AND EXISTS(
                     SELECT 1 FROM jobs_ats_certification_circuit_heads head
                      WHERE head.scope_kind = 'runtime'
                        AND head.subject_key IN ($1, $2)
                        AND head.state <> 'closed'))",
                &[
                    &runtime.runtime_id,
                    &runtime.runtime_sha256,
                    &now_ms,
                    &trust_policy_sha256,
                    &runtime.browser_release_manifest_sha256,
                    &runtime.runner_image_sha256,
                    &runtime.runner_build_id,
                    &runner_build_sha256,
                    &include_circuit_state,
                ],
            )
            .map_err(ats_certification_storage)?
            .get(0);
        if !unavailable {
            available.push(runtime);
        }
    }
    Ok(available)
}

fn validate_ats_application_binding_request(
    request: &AtsApplicationCertificationBindingRequest,
    now_ms: i64,
) -> Result<(), AtsCertificationAuthorityError> {
    for value in [
        request.binding_id.as_str(),
        request.account_id.as_str(),
        request.application_id.as_str(),
        request.run_id.as_str(),
        request.application_attempt_id.as_str(),
        request.browser_session_id.as_str(),
        request.browser_profile_id.as_str(),
        request.auto_authorization_id.as_str(),
    ] {
        if !ats_certification_text(value, 1, 240) {
            return Err(AtsCertificationAuthorityError::InvalidAuthority);
        }
    }
    if !matches!(request.rollout_channel.as_str(), "canary" | "general")
        || !ats_certification_hex64(&request.runner_id)
        || !ats_certification_hex64(&request.packet_checksum_sha256)
        || !ats_certification_hex64(&request.auto_authorization_fingerprint_sha256)
        || !ats_certification_hex64(&request.nonce_sha256)
        || request.auto_authorization_revision < 1
        || !ats_certification_safe_integer(request.auto_authorization_revision, true)
        || request.requested_expires_at_ms <= now_ms
        || !ats_certification_safe_integer(request.requested_expires_at_ms, true)
    {
        return Err(AtsCertificationAuthorityError::InvalidAuthority);
    }
    validate_ats_certification_fresh_target_evidence(&request.target_evidence, now_ms)?;
    Ok(())
}

fn ats_application_binding_request_matches(
    authority: &AtsApplicationCertificationBindingAuthority,
    request: &AtsApplicationCertificationBindingRequest,
    server_account_allowlist_sha256: Option<&str>,
) -> bool {
    authority.schema_version == 1
        && authority.binding_id == request.binding_id
        && authority.account_id == request.account_id
        && authority.application_id == request.application_id
        && authority.run_id == request.run_id
        && authority.application_attempt_id == request.application_attempt_id
        && authority.browser_session_id == request.browser_session_id
        && authority.browser_profile_id == request.browser_profile_id
        && authority.packet_checksum_sha256 == request.packet_checksum_sha256
        && authority.auto_authorization_id == request.auto_authorization_id
        && authority.auto_authorization_revision == request.auto_authorization_revision
        && authority.auto_authorization_fingerprint_sha256
            == request.auto_authorization_fingerprint_sha256
        && authority.target_evidence == request.target_evidence
        && authority.nonce_sha256 == request.nonce_sha256
        && authority.requested_expires_at_ms == request.requested_expires_at_ms
        && authority.certification.channel == request.rollout_channel
        && authority.certification.account_allowlist_sha256.as_deref()
            == server_account_allowlist_sha256
        && authority
            .certification
            .selected_runtime
            .as_ref()
            .is_some_and(|runtime| runtime.runtime_sha256 == request.runner_id)
}

fn decode_ats_application_binding_authority(
    binding_sha256: &str,
    frozen_base64url: &str,
) -> Result<AtsApplicationCertificationBindingAuthority, AtsCertificationAuthorityError> {
    if !ats_certification_hex64(binding_sha256) {
        return Err(AtsCertificationAuthorityError::InvalidAuthority);
    }
    let canonical = ats_certification_decode_base64url_bounded(
        frozen_base64url,
        ATS_CERTIFICATION_MAX_CANONICAL_BYTES,
    )?;
    if ats_certification_sha256(&canonical) != binding_sha256 {
        return Err(AtsCertificationAuthorityError::IdentityConflict);
    }
    ats_certification_parse_canonical_json(&canonical)
}

fn ats_application_binding_record(
    binding_sha256: String,
    frozen_base64url: String,
    phase: String,
    fence: i64,
    consumed_at_ms: Option<i64>,
    replayed: bool,
) -> Result<AtsApplicationCertificationBindingRecord, AtsCertificationAuthorityError> {
    let authority = decode_ats_application_binding_authority(&binding_sha256, &frozen_base64url)?;
    Ok(AtsApplicationCertificationBindingRecord {
        binding_sha256,
        authority,
        phase,
        fence,
        consumed_at_ms,
        replayed,
    })
}

fn build_ats_application_binding_authority(
    request: &AtsApplicationCertificationBindingRequest,
    certification: AtsCertificationBinding,
    created_at_ms: i64,
) -> Result<
    (AtsApplicationCertificationBindingAuthority, String, String),
    AtsCertificationAuthorityError,
> {
    let expires_at_ms = request
        .requested_expires_at_ms
        .min(certification.expires_at_ms);
    if expires_at_ms <= created_at_ms || certification.selected_runtime.is_none() {
        return Err(AtsCertificationAuthorityError::Expired);
    }
    let authority = AtsApplicationCertificationBindingAuthority {
        schema_version: 1,
        binding_id: request.binding_id.clone(),
        account_id: request.account_id.clone(),
        application_id: request.application_id.clone(),
        run_id: request.run_id.clone(),
        application_attempt_id: request.application_attempt_id.clone(),
        browser_session_id: request.browser_session_id.clone(),
        browser_profile_id: request.browser_profile_id.clone(),
        packet_checksum_sha256: request.packet_checksum_sha256.clone(),
        auto_authorization_id: request.auto_authorization_id.clone(),
        auto_authorization_revision: request.auto_authorization_revision,
        auto_authorization_fingerprint_sha256: request
            .auto_authorization_fingerprint_sha256
            .clone(),
        target_evidence: request.target_evidence.clone(),
        nonce_sha256: request.nonce_sha256.clone(),
        certification,
        requested_expires_at_ms: request.requested_expires_at_ms,
        created_at_ms,
        expires_at_ms,
    };
    let canonical = ats_certification_canonical_json(&authority)
        .map_err(|_| AtsCertificationAuthorityError::InvalidAuthority)?;
    let binding_sha256 = ats_certification_sha256(&canonical);
    let frozen_base64url = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(canonical);
    Ok((authority, binding_sha256, frozen_base64url))
}

#[derive(Debug)]
struct AtsFrozenApplicationAdmission {
    packet_checksum_sha256: String,
    auto_authorization_id: String,
    auto_authorization_revision: i64,
    auto_authorization_fingerprint_sha256: String,
    career_track_id: String,
    certification: AtsFrozenCertificationAdmissionProjection,
}

fn ats_frozen_application_admission(
    application: &JobApplication,
    now_ms: i64,
) -> Result<AtsFrozenApplicationAdmission, AtsCertificationAuthorityError> {
    if application.submission_mode != "auto_submit" {
        return Err(AtsCertificationAuthorityError::ScopeMismatch);
    }
    let approved = application
        .receipt
        .get("approved_execution")
        .and_then(Value::as_object)
        .ok_or(AtsCertificationAuthorityError::ScopeMismatch)?;
    if approved.get("schema_version").and_then(Value::as_i64) != Some(3) {
        return Err(AtsCertificationAuthorityError::ScopeMismatch);
    }
    let packet = approved
        .get("packet")
        .filter(|value| value.is_object())
        .ok_or(AtsCertificationAuthorityError::ScopeMismatch)?;
    let job = approved
        .get("job")
        .filter(|value| value.is_object())
        .ok_or(AtsCertificationAuthorityError::ScopeMismatch)?;
    let admission = approved
        .get("admission")
        .and_then(Value::as_object)
        .ok_or(AtsCertificationAuthorityError::ScopeMismatch)?;
    validate_approved_submission_admission(application, 3, Some(&Value::Object(admission.clone())))
        .map_err(|_| AtsCertificationAuthorityError::ScopeMismatch)?;
    let packet_checksum_sha256 = approved
        .get("checksum")
        .and_then(Value::as_str)
        .filter(|value| ats_certification_hex64(value))
        .ok_or(AtsCertificationAuthorityError::ScopeMismatch)?
        .to_string();
    let expected_checksum =
        approved_submission_checksum(3, packet, job, Some(&Value::Object(admission.clone())))
            .map_err(ats_certification_storage)?;
    if packet_checksum_sha256 != expected_checksum {
        return Err(AtsCertificationAuthorityError::ScopeMismatch);
    }
    let auto_authorization_id = admission
        .get("authorization_id")
        .and_then(Value::as_str)
        .filter(|value| ats_certification_text(value, 1, 240))
        .ok_or(AtsCertificationAuthorityError::ScopeMismatch)?
        .to_string();
    let auto_authorization_revision = admission
        .get("revision_no")
        .and_then(Value::as_i64)
        .filter(|value| ats_certification_safe_integer(*value, true))
        .ok_or(AtsCertificationAuthorityError::ScopeMismatch)?;
    let auto_authorization_fingerprint_sha256 = admission
        .get("authority_fingerprint")
        .and_then(Value::as_str)
        .filter(|value| ats_certification_hex64(value))
        .ok_or(AtsCertificationAuthorityError::ScopeMismatch)?
        .to_string();
    let career_track_id = admission
        .get("career_track_id")
        .and_then(Value::as_str)
        .filter(|value| ats_certification_text(value, 1, 240))
        .ok_or(AtsCertificationAuthorityError::ScopeMismatch)?
        .to_string();
    let certification = serde_json::from_value::<AtsFrozenCertificationAdmissionProjection>(
        admission
            .get("ats_certification")
            .cloned()
            .ok_or(AtsCertificationAuthorityError::ScopeMismatch)?,
    )
    .map_err(|_| AtsCertificationAuthorityError::ScopeMismatch)?;
    if certification.schema_version != 1 || certification.expires_at_ms <= now_ms {
        return Err(AtsCertificationAuthorityError::Expired);
    }
    Ok(AtsFrozenApplicationAdmission {
        packet_checksum_sha256,
        auto_authorization_id,
        auto_authorization_revision,
        auto_authorization_fingerprint_sha256,
        career_track_id,
        certification,
    })
}

fn validate_ats_runtime_attestation(
    attestation: &AtsCertificationRuntimeAttestation,
) -> Result<(), AtsCertificationAuthorityError> {
    match attestation {
        AtsCertificationRuntimeAttestation::Local {
            platform,
            architecture,
            browser_release_manifest_sha256,
            browser_artifact_sha256,
            browser_build_descriptor_sha256,
            automation_bundle_sha256,
            playwright_version,
            chromium_revision,
            chromium_executable_sha256,
        } => {
            if !matches!(platform.as_str(), "linux" | "macos" | "windows")
                || !matches!(architecture.as_str(), "arm64" | "x86_64")
                || !ats_certification_hex64(browser_release_manifest_sha256)
                || !ats_certification_hex64(browser_artifact_sha256)
                || !ats_certification_hex64(browser_build_descriptor_sha256)
                || !ats_certification_hex64(automation_bundle_sha256)
                || !ats_certification_text(playwright_version, 1, 120)
                || !ats_certification_text(chromium_revision, 1, 120)
                || !ats_certification_hex64(chromium_executable_sha256)
            {
                return Err(AtsCertificationAuthorityError::InvalidAuthority);
            }
        }
        AtsCertificationRuntimeAttestation::Cloud {
            platform,
            architecture,
            runner_build_id,
            runner_image_sha256,
            automation_bundle_sha256,
            playwright_version,
            chromium_revision,
            chromium_executable_sha256,
        } => {
            if !matches!(platform.as_str(), "linux" | "macos" | "windows")
                || !matches!(architecture.as_str(), "arm64" | "x86_64")
                || !ats_certification_text(runner_build_id, 1, 240)
                || !ats_certification_hex64(runner_image_sha256)
                || !ats_certification_hex64(automation_bundle_sha256)
                || !ats_certification_text(playwright_version, 1, 120)
                || !ats_certification_text(chromium_revision, 1, 120)
                || !ats_certification_hex64(chromium_executable_sha256)
            {
                return Err(AtsCertificationAuthorityError::InvalidAuthority);
            }
        }
    }
    Ok(())
}

fn ats_runtime_matches_attestation(
    runtime: &AtsCertificationRuntimeTarget,
    attestation: &AtsCertificationRuntimeAttestation,
) -> bool {
    match attestation {
        AtsCertificationRuntimeAttestation::Local {
            platform,
            architecture,
            browser_release_manifest_sha256,
            browser_artifact_sha256,
            browser_build_descriptor_sha256,
            automation_bundle_sha256,
            playwright_version,
            chromium_revision,
            chromium_executable_sha256,
        } => {
            runtime.runtime_kind == "local"
                && runtime.platform == *platform
                && runtime.architecture == *architecture
                && runtime.browser_release_manifest_sha256.as_deref()
                    == Some(browser_release_manifest_sha256)
                && runtime.browser_artifact_sha256.as_deref() == Some(browser_artifact_sha256)
                && runtime.browser_build_descriptor_sha256.as_deref()
                    == Some(browser_build_descriptor_sha256)
                && runtime.automation_bundle_sha256 == *automation_bundle_sha256
                && runtime.playwright_version == *playwright_version
                && runtime.chromium_revision == *chromium_revision
                && runtime.chromium_executable_sha256 == *chromium_executable_sha256
        }
        AtsCertificationRuntimeAttestation::Cloud {
            platform,
            architecture,
            runner_build_id,
            runner_image_sha256,
            automation_bundle_sha256,
            playwright_version,
            chromium_revision,
            chromium_executable_sha256,
        } => {
            runtime.runtime_kind == "cloud"
                && runtime.platform == *platform
                && runtime.architecture == *architecture
                && runtime.runner_build_id.as_deref() == Some(runner_build_id)
                && runtime.runner_image_sha256.as_deref() == Some(runner_image_sha256)
                && runtime.automation_bundle_sha256 == *automation_bundle_sha256
                && runtime.playwright_version == *playwright_version
                && runtime.chromium_revision == *chromium_revision
                && runtime.chromium_executable_sha256 == *chromium_executable_sha256
        }
    }
}

pub fn ats_certification_runtime_target_from_attestation(
    binding: &AtsCertificationBinding,
    attestation: &AtsCertificationRuntimeAttestation,
) -> Result<AtsCertificationRuntimeTarget, AtsCertificationAuthorityError> {
    validate_ats_runtime_attestation(attestation)?;
    let mut matches = binding
        .runtime_targets
        .iter()
        .filter(|runtime| ats_runtime_matches_attestation(runtime, attestation));
    let runtime = matches
        .next()
        .cloned()
        .ok_or(AtsCertificationAuthorityError::RuntimeMismatch)?;
    if matches.next().is_some() {
        return Err(AtsCertificationAuthorityError::RuntimeMismatch);
    }
    Ok(runtime)
}

fn require_frozen_ats_binding_matches_admission(
    binding: AtsCertificationBinding,
    admission: &AtsFrozenCertificationAdmissionProjection,
    attestation: &AtsCertificationRuntimeAttestation,
) -> Result<AtsCertificationBinding, AtsCertificationAuthorityError> {
    if ats_frozen_certification_admission_projection(&binding)? != *admission
        || binding
            .selected_runtime
            .as_ref()
            .is_none_or(|runtime| !ats_runtime_matches_attestation(runtime, attestation))
    {
        return Err(AtsCertificationAuthorityError::RuntimeMismatch);
    }
    Ok(binding)
}

fn ats_phase_a_binding_id(
    request: &AtsCertificationPhaseAContextRequest,
    attempt_id: &str,
) -> String {
    let digest = ats_certification_sha256(
        format!(
            "ats-phase-a\0{}\0{}\0{}\0{}\0{}",
            request.account_id,
            request.application_id,
            request.run_id,
            attempt_id,
            request.nonce_sha256,
        )
        .as_bytes(),
    );
    format!("ats-binding-{}", &digest[..32])
}

fn resolve_sqlite_ats_phase_a_certification(
    tx: &rusqlite::Transaction<'_>,
    target_evidence: &AtsCertificationFreshTargetEvidence,
    admission: &AtsFrozenCertificationAdmissionProjection,
    attestation: &AtsCertificationRuntimeAttestation,
    account_id: &str,
    now_ms: i64,
) -> Result<AtsCertificationBinding, AtsCertificationAuthorityError> {
    let mut matches = Vec::with_capacity(2);
    if let Some(matrix) = resolve_ats_certification_target_status_sqlite_tx(
        tx,
        target_evidence,
        None,
        "general",
        None,
        now_ms,
    )? {
        if ats_frozen_certification_admission_projection(&matrix)? == *admission {
            let runtime = ats_certification_runtime_target_from_attestation(&matrix, attestation)?;
            let selected = resolve_ats_certification_target_status_sqlite_tx(
                tx,
                target_evidence,
                Some(&runtime.runtime_sha256),
                "general",
                None,
                now_ms,
            )?
            .ok_or(AtsCertificationAuthorityError::RuntimeMismatch)?;
            matches.push(require_frozen_ats_binding_matches_admission(
                selected,
                admission,
                attestation,
            )?);
        }
    }
    if let Some(allowlist_sha256) = resolve_sqlite_ats_canary_allowlist_for_target_account(
        tx,
        target_evidence,
        account_id,
        now_ms,
    )? {
        if let Some(matrix) = resolve_ats_certification_target_status_sqlite_tx(
            tx,
            target_evidence,
            None,
            "canary",
            Some(&allowlist_sha256),
            now_ms,
        )? {
            if ats_frozen_certification_admission_projection(&matrix)? == *admission {
                let runtime =
                    ats_certification_runtime_target_from_attestation(&matrix, attestation)?;
                let selected = resolve_ats_certification_target_status_sqlite_tx(
                    tx,
                    target_evidence,
                    Some(&runtime.runtime_sha256),
                    "canary",
                    Some(&allowlist_sha256),
                    now_ms,
                )?
                .ok_or(AtsCertificationAuthorityError::RuntimeMismatch)?;
                matches.push(require_frozen_ats_binding_matches_admission(
                    selected,
                    admission,
                    attestation,
                )?);
            }
        }
    }
    if matches.len() != 1 {
        return Err(AtsCertificationAuthorityError::ScopeMismatch);
    }
    matches
        .pop()
        .ok_or(AtsCertificationAuthorityError::ScopeMismatch)
}

fn resolve_postgres_ats_phase_a_certification(
    tx: &mut postgres::Transaction<'_>,
    target_evidence: &AtsCertificationFreshTargetEvidence,
    admission: &AtsFrozenCertificationAdmissionProjection,
    attestation: &AtsCertificationRuntimeAttestation,
    account_id: &str,
    now_ms: i64,
) -> Result<AtsCertificationBinding, AtsCertificationAuthorityError> {
    let mut matches = Vec::with_capacity(2);
    if let Some(matrix) = resolve_ats_certification_target_status_postgres_tx(
        tx,
        target_evidence,
        None,
        "general",
        None,
        now_ms,
    )? {
        if ats_frozen_certification_admission_projection(&matrix)? == *admission {
            let runtime = ats_certification_runtime_target_from_attestation(&matrix, attestation)?;
            let selected = resolve_ats_certification_target_status_postgres_tx(
                tx,
                target_evidence,
                Some(&runtime.runtime_sha256),
                "general",
                None,
                now_ms,
            )?
            .ok_or(AtsCertificationAuthorityError::RuntimeMismatch)?;
            matches.push(require_frozen_ats_binding_matches_admission(
                selected,
                admission,
                attestation,
            )?);
        }
    }
    if let Some(allowlist_sha256) = resolve_postgres_ats_canary_allowlist_for_target_account(
        tx,
        target_evidence,
        account_id,
        now_ms,
    )? {
        if let Some(matrix) = resolve_ats_certification_target_status_postgres_tx(
            tx,
            target_evidence,
            None,
            "canary",
            Some(&allowlist_sha256),
            now_ms,
        )? {
            if ats_frozen_certification_admission_projection(&matrix)? == *admission {
                let runtime =
                    ats_certification_runtime_target_from_attestation(&matrix, attestation)?;
                let selected = resolve_ats_certification_target_status_postgres_tx(
                    tx,
                    target_evidence,
                    Some(&runtime.runtime_sha256),
                    "canary",
                    Some(&allowlist_sha256),
                    now_ms,
                )?
                .ok_or(AtsCertificationAuthorityError::RuntimeMismatch)?;
                matches.push(require_frozen_ats_binding_matches_admission(
                    selected,
                    admission,
                    attestation,
                )?);
            }
        }
    }
    if matches.len() != 1 {
        return Err(AtsCertificationAuthorityError::ScopeMismatch);
    }
    matches
        .pop()
        .ok_or(AtsCertificationAuthorityError::ScopeMismatch)
}

fn validate_ats_phase_a_context_request(
    request: &AtsCertificationPhaseAContextRequest,
    now_ms: i64,
) -> Result<(), AtsCertificationAuthorityError> {
    for value in [
        request.account_id.as_str(),
        request.application_id.as_str(),
        request.run_id.as_str(),
        request.browser_session_id.as_str(),
        request.browser_profile_id.as_str(),
    ] {
        if !ats_certification_text(value, 1, 240) {
            return Err(AtsCertificationAuthorityError::InvalidAuthority);
        }
    }
    if !ats_certification_hex64(&request.nonce_sha256)
        || !ats_certification_safe_integer(now_ms, false)
    {
        return Err(AtsCertificationAuthorityError::InvalidAuthority);
    }
    validate_ats_runtime_attestation(&request.runtime_attestation)?;
    Ok(())
}

pub fn create_ats_application_certification_binding_from_context_sqlite_tx(
    tx: &rusqlite::Transaction<'_>,
    request: &AtsCertificationPhaseAContextRequest,
    now_ms: i64,
) -> Result<AtsApplicationCertificationBindingRecord, AtsCertificationAuthorityError> {
    validate_ats_phase_a_context_request(request, now_ms)?;
    let (job_id, application_json, stored_state) = tx
        .query_row(
            "SELECT job_id, application_json, state FROM jobs_applications
              WHERE account_id = ?1 AND id = ?2",
            params![request.account_id, request.application_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            },
        )
        .optional()
        .map_err(ats_certification_storage)?
        .ok_or(AtsCertificationAuthorityError::NotFound)?;
    let application = parse_application_json(
        application_json,
        &request.application_id,
        &job_id,
        "ATS Phase A application",
    )
    .map_err(ats_certification_storage)?;
    if application.state != stored_state
        || !matches!(stored_state.as_str(), "queued" | "running")
        || application.run_id.as_deref() != Some(request.run_id.as_str())
    {
        return Err(AtsCertificationAuthorityError::ScopeMismatch);
    }
    let (posting_id, posting_json) = tx
        .query_row(
            "SELECT id, posting_json FROM jobs_postings
              WHERE account_id = ?1 AND id = ?2",
            params![request.account_id, job_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()
        .map_err(ats_certification_storage)?
        .ok_or(AtsCertificationAuthorityError::NotFound)?;
    let mut posting: JobPosting =
        parse_json(posting_json, "ATS Phase A posting").map_err(ats_certification_storage)?;
    posting.id = posting_id;
    let target_evidence = ats_certification_fresh_target_evidence_from_posting(&posting, now_ms)?;
    let admission = ats_frozen_application_admission(&application, now_ms)?;
    let authorization_matches: i64 = tx
        .query_row(
            "SELECT COUNT(*) FROM jobs_auto_submit_authorizations
              WHERE id = ?1 AND account_id = ?2 AND career_track_id = ?3
                AND revision_no = ?4 AND authority_fingerprint = ?5
                AND revoked_at_ms IS NULL",
            params![
                admission.auto_authorization_id,
                request.account_id,
                admission.career_track_id,
                admission.auto_authorization_revision,
                admission.auto_authorization_fingerprint_sha256,
            ],
            |row| row.get(0),
        )
        .map_err(ats_certification_storage)?;
    if authorization_matches != 1 {
        return Err(AtsCertificationAuthorityError::ScopeMismatch);
    }
    let (attempt_id, attempt_runner, attempt_status) = tx
        .query_row(
            "SELECT id, runner, status FROM jobs_attempt_reservations
              WHERE account_id = ?1 AND application_id = ?2",
            params![request.account_id, request.application_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            },
        )
        .optional()
        .map_err(ats_certification_storage)?
        .ok_or(AtsCertificationAuthorityError::NotFound)?;
    if !matches!(attempt_status.as_str(), "reserved" | "running") {
        return Err(AtsCertificationAuthorityError::ScopeMismatch);
    }
    let certification = resolve_sqlite_ats_phase_a_certification(
        tx,
        &target_evidence,
        &admission.certification,
        &request.runtime_attestation,
        &request.account_id,
        now_ms,
    )?;
    let runtime = certification
        .selected_runtime
        .as_ref()
        .ok_or(AtsCertificationAuthorityError::RuntimeMismatch)?;
    if attempt_runner != runtime.runtime_kind {
        return Err(AtsCertificationAuthorityError::RuntimeMismatch);
    }
    let raw_request = AtsApplicationCertificationBindingRequest {
        binding_id: ats_phase_a_binding_id(request, &attempt_id),
        account_id: request.account_id.clone(),
        application_id: request.application_id.clone(),
        run_id: request.run_id.clone(),
        application_attempt_id: attempt_id,
        browser_session_id: request.browser_session_id.clone(),
        browser_profile_id: request.browser_profile_id.clone(),
        packet_checksum_sha256: admission.packet_checksum_sha256,
        auto_authorization_id: admission.auto_authorization_id,
        auto_authorization_revision: admission.auto_authorization_revision,
        auto_authorization_fingerprint_sha256: admission.auto_authorization_fingerprint_sha256,
        target_evidence,
        rollout_channel: certification.channel,
        runner_id: runtime.runtime_sha256.clone(),
        nonce_sha256: request.nonce_sha256.clone(),
        requested_expires_at_ms: now_ms
            .saturating_add(ATS_CERTIFICATION_APPLICATION_BINDING_TTL_MS),
    };
    create_ats_application_certification_binding_sqlite_tx(tx, &raw_request, now_ms)
}

pub fn create_ats_application_certification_binding_from_context_postgres_tx(
    tx: &mut postgres::Transaction<'_>,
    request: &AtsCertificationPhaseAContextRequest,
    now_ms: i64,
) -> Result<AtsApplicationCertificationBindingRecord, AtsCertificationAuthorityError> {
    lock_postgres_ats_certification(tx)?;
    create_ats_application_certification_binding_from_context_postgres_tx_after_prelock(
        tx, request, now_ms,
    )
}

/// Create Phase A authority after the caller has already acquired the common ATS advisory
/// prelock. This preserves all row validation and CAS semantics without reacquiring ATS after D.
pub(crate) fn create_ats_application_certification_binding_from_context_postgres_tx_after_prelock(
    tx: &mut postgres::Transaction<'_>,
    request: &AtsCertificationPhaseAContextRequest,
    now_ms: i64,
) -> Result<AtsApplicationCertificationBindingRecord, AtsCertificationAuthorityError> {
    validate_ats_phase_a_context_request(request, now_ms)?;
    let row = tx
        .query_opt(
            "SELECT job_id, application_json, state FROM jobs_applications
              WHERE account_id = $1 AND id = $2 FOR UPDATE",
            &[&request.account_id, &request.application_id],
        )
        .map_err(ats_certification_storage)?
        .ok_or(AtsCertificationAuthorityError::NotFound)?;
    let job_id: String = row.get(0);
    let application_json: String = row.get(1);
    let stored_state: String = row.get(2);
    let application = parse_application_json(
        application_json,
        &request.application_id,
        &job_id,
        "ATS Phase A application",
    )
    .map_err(ats_certification_storage)?;
    if application.state != stored_state
        || !matches!(stored_state.as_str(), "queued" | "running")
        || application.run_id.as_deref() != Some(request.run_id.as_str())
    {
        return Err(AtsCertificationAuthorityError::ScopeMismatch);
    }
    let row = tx
        .query_opt(
            "SELECT id, posting_json FROM jobs_postings
              WHERE account_id = $1 AND id = $2 FOR UPDATE",
            &[&request.account_id, &job_id],
        )
        .map_err(ats_certification_storage)?
        .ok_or(AtsCertificationAuthorityError::NotFound)?;
    let mut posting: JobPosting =
        parse_json(row.get(1), "ATS Phase A posting").map_err(ats_certification_storage)?;
    posting.id = row.get(0);
    let target_evidence = ats_certification_fresh_target_evidence_from_posting(&posting, now_ms)?;
    let admission = ats_frozen_application_admission(&application, now_ms)?;
    if tx
        .query_one(
            "SELECT COUNT(*) FROM jobs_auto_submit_authorizations
              WHERE id = $1 AND account_id = $2 AND career_track_id = $3
                AND revision_no = $4 AND authority_fingerprint = $5
                AND revoked_at_ms IS NULL",
            &[
                &admission.auto_authorization_id,
                &request.account_id,
                &admission.career_track_id,
                &admission.auto_authorization_revision,
                &admission.auto_authorization_fingerprint_sha256,
            ],
        )
        .map_err(ats_certification_storage)?
        .get::<_, i64>(0)
        != 1
    {
        return Err(AtsCertificationAuthorityError::ScopeMismatch);
    }
    let row = tx
        .query_opt(
            "SELECT id, runner, status FROM jobs_attempt_reservations
              WHERE account_id = $1 AND application_id = $2 FOR UPDATE",
            &[&request.account_id, &request.application_id],
        )
        .map_err(ats_certification_storage)?
        .ok_or(AtsCertificationAuthorityError::NotFound)?;
    let attempt_id: String = row.get(0);
    let attempt_runner: String = row.get(1);
    let attempt_status: String = row.get(2);
    if !matches!(attempt_status.as_str(), "reserved" | "running") {
        return Err(AtsCertificationAuthorityError::ScopeMismatch);
    }
    let certification = resolve_postgres_ats_phase_a_certification(
        tx,
        &target_evidence,
        &admission.certification,
        &request.runtime_attestation,
        &request.account_id,
        now_ms,
    )?;
    let runtime = certification
        .selected_runtime
        .as_ref()
        .ok_or(AtsCertificationAuthorityError::RuntimeMismatch)?;
    if attempt_runner != runtime.runtime_kind {
        return Err(AtsCertificationAuthorityError::RuntimeMismatch);
    }
    let raw_request = AtsApplicationCertificationBindingRequest {
        binding_id: ats_phase_a_binding_id(request, &attempt_id),
        account_id: request.account_id.clone(),
        application_id: request.application_id.clone(),
        run_id: request.run_id.clone(),
        application_attempt_id: attempt_id,
        browser_session_id: request.browser_session_id.clone(),
        browser_profile_id: request.browser_profile_id.clone(),
        packet_checksum_sha256: admission.packet_checksum_sha256,
        auto_authorization_id: admission.auto_authorization_id,
        auto_authorization_revision: admission.auto_authorization_revision,
        auto_authorization_fingerprint_sha256: admission.auto_authorization_fingerprint_sha256,
        target_evidence,
        rollout_channel: certification.channel,
        runner_id: runtime.runtime_sha256.clone(),
        nonce_sha256: request.nonce_sha256.clone(),
        requested_expires_at_ms: now_ms
            .saturating_add(ATS_CERTIFICATION_APPLICATION_BINDING_TTL_MS),
    };
    create_ats_application_certification_binding_postgres_tx_after_prelock(tx, &raw_request, now_ms)
}

pub fn create_ats_application_certification_binding(
    pool: &DbPool,
    request: &AtsApplicationCertificationBindingRequest,
    now_ms: i64,
) -> Result<AtsApplicationCertificationBindingRecord, AtsCertificationAuthorityError> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get().map_err(ats_certification_storage)?;
            let tx = conn
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(ats_certification_storage)?;
            let result =
                create_ats_application_certification_binding_sqlite_tx(&tx, request, now_ms)?;
            tx.commit().map_err(ats_certification_storage)?;
            Ok(result)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg().map_err(ats_certification_storage)?;
            let mut tx = conn
                .build_transaction()
                .isolation_level(postgres::IsolationLevel::Serializable)
                .start()
                .map_err(ats_certification_storage)?;
            lock_postgres_ats_certification(&mut tx)?;
            let result = create_ats_application_certification_binding_postgres_tx_after_prelock(
                &mut tx, request, now_ms,
            )?;
            tx.commit().map_err(ats_certification_storage)?;
            Ok(result)
        }
    })
}

pub fn create_ats_application_certification_binding_sqlite_tx(
    tx: &rusqlite::Transaction<'_>,
    request: &AtsApplicationCertificationBindingRequest,
    now_ms: i64,
) -> Result<AtsApplicationCertificationBindingRecord, AtsCertificationAuthorityError> {
    validate_ats_application_binding_request(request, now_ms)?;
    let account_allowlist_sha256 = if request.rollout_channel == "canary" {
        Some(
            resolve_sqlite_ats_canary_allowlist_for_target_account(
                tx,
                &request.target_evidence,
                &request.account_id,
                now_ms,
            )?
            .ok_or(AtsCertificationAuthorityError::ScopeMismatch)?,
        )
    } else {
        None
    };
    let mut stmt = tx
        .prepare(
            "SELECT binding_sha256, frozen_certification_base64url, phase, fence,
                    consumed_at_ms
               FROM jobs_application_ats_certification_bindings
              WHERE binding_id = ?1 OR nonce_sha256 = ?2
                 OR (account_id = ?3 AND application_id = ?4 AND attempt_id = ?5)
                 OR (account_id = ?3 AND application_id = ?4 AND run_id = ?6)
              LIMIT 2",
        )
        .map_err(ats_certification_storage)?;
    let existing = stmt
        .query_map(
            params![
                request.binding_id,
                request.nonce_sha256,
                request.account_id,
                request.application_id,
                request.application_attempt_id,
                request.run_id,
            ],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, Option<i64>>(4)?,
                ))
            },
        )
        .map_err(ats_certification_storage)?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(ats_certification_storage)?;
    if !existing.is_empty() {
        if existing.len() != 1 {
            return Err(AtsCertificationAuthorityError::IdentityConflict);
        }
        let existing = existing
            .into_iter()
            .next()
            .ok_or(AtsCertificationAuthorityError::IdentityConflict)?;
        let record = ats_application_binding_record(
            existing.0, existing.1, existing.2, existing.3, existing.4, true,
        )?;
        if record.phase != "preflight"
            || record.fence != 0
            || record.authority.expires_at_ms <= now_ms
            || !ats_application_binding_request_matches(
                &record.authority,
                request,
                account_allowlist_sha256.as_deref(),
            )
        {
            return Err(AtsCertificationAuthorityError::IdentityConflict);
        }
        return Ok(record);
    }
    let certification = resolve_ats_certification_target_status_sqlite_tx(
        tx,
        &request.target_evidence,
        Some(&request.runner_id),
        &request.rollout_channel,
        account_allowlist_sha256.as_deref(),
        now_ms,
    )?
    .ok_or(AtsCertificationAuthorityError::ScopeMismatch)?;
    let (authority, binding_sha256, frozen_base64url) =
        build_ats_application_binding_authority(request, certification, now_ms)?;
    let runtime = authority
        .certification
        .selected_runtime
        .as_ref()
        .ok_or(AtsCertificationAuthorityError::RuntimeMismatch)?;
    tx.execute(
        "INSERT INTO jobs_application_ats_certification_bindings (
           binding_id, binding_sha256, account_id, application_id, run_id, attempt_id,
           browser_session_id, browser_profile_id, packet_checksum_sha256,
           auto_authorization_id, auto_authorization_revision,
           auto_authorization_fingerprint_sha256, provider, target_key,
           manifest_sha256, activation_sha256, layout_set_sha256,
           adapter_bundle_sha256, runner_target_sha256, platform, architecture,
           automation_bundle_sha256, browser_release_manifest_sha256,
           browser_artifact_sha256, browser_build_descriptor_sha256, runner_build_id,
           runner_image_sha256, browser_runtime_sha256, chromium_executable_sha256,
           nonce_sha256, frozen_certification_base64url, expires_at_ms,
           phase, fence, created_at_ms
         ) VALUES (
           ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14,
           ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26,
           ?27, ?28, ?29, ?30, ?31, ?32, 'preflight', 0, ?33
         )",
        params![
            request.binding_id,
            binding_sha256,
            request.account_id,
            request.application_id,
            request.run_id,
            request.application_attempt_id,
            request.browser_session_id,
            request.browser_profile_id,
            request.packet_checksum_sha256,
            request.auto_authorization_id,
            request.auto_authorization_revision,
            request.auto_authorization_fingerprint_sha256,
            authority.certification.provider,
            authority.certification.target_key,
            authority.certification.manifest_sha256,
            authority.certification.activation_sha256,
            authority.certification.layout_set_sha256,
            authority.certification.adapter_bundle_sha256,
            runtime.runtime_sha256,
            runtime.platform,
            runtime.architecture,
            runtime.automation_bundle_sha256,
            runtime.browser_release_manifest_sha256,
            runtime.browser_artifact_sha256,
            runtime.browser_build_descriptor_sha256,
            runtime.runner_build_id,
            runtime.runner_image_sha256,
            runtime.runtime_sha256,
            runtime.chromium_executable_sha256,
            request.nonce_sha256,
            frozen_base64url,
            authority.expires_at_ms,
            now_ms,
        ],
    )
    .map_err(ats_certification_storage)?;
    Ok(AtsApplicationCertificationBindingRecord {
        binding_sha256,
        authority,
        phase: "preflight".to_string(),
        fence: 0,
        consumed_at_ms: None,
        replayed: false,
    })
}

pub fn create_ats_application_certification_binding_postgres_tx(
    tx: &mut postgres::Transaction<'_>,
    request: &AtsApplicationCertificationBindingRequest,
    now_ms: i64,
) -> Result<AtsApplicationCertificationBindingRecord, AtsCertificationAuthorityError> {
    lock_postgres_ats_certification(tx)?;
    create_ats_application_certification_binding_postgres_tx_after_prelock(tx, request, now_ms)
}

/// Persist or replay a Phase A binding after the caller has acquired ATS. This helper deliberately
/// performs no advisory locking so callers that already hold H -> M -> ATS -> D cannot invert the
/// common authority order.
pub(crate) fn create_ats_application_certification_binding_postgres_tx_after_prelock(
    tx: &mut postgres::Transaction<'_>,
    request: &AtsApplicationCertificationBindingRequest,
    now_ms: i64,
) -> Result<AtsApplicationCertificationBindingRecord, AtsCertificationAuthorityError> {
    validate_ats_application_binding_request(request, now_ms)?;
    let account_allowlist_sha256 = if request.rollout_channel == "canary" {
        Some(
            resolve_postgres_ats_canary_allowlist_for_target_account(
                tx,
                &request.target_evidence,
                &request.account_id,
                now_ms,
            )?
            .ok_or(AtsCertificationAuthorityError::ScopeMismatch)?,
        )
    } else {
        None
    };
    let existing = tx
        .query(
            "SELECT binding_sha256, frozen_certification_base64url, phase, fence,
                    consumed_at_ms
               FROM jobs_application_ats_certification_bindings
              WHERE binding_id = $1 OR nonce_sha256 = $2
                 OR (account_id = $3 AND application_id = $4 AND attempt_id = $5)
                 OR (account_id = $3 AND application_id = $4 AND run_id = $6)
              LIMIT 2 FOR UPDATE",
            &[
                &request.binding_id,
                &request.nonce_sha256,
                &request.account_id,
                &request.application_id,
                &request.application_attempt_id,
                &request.run_id,
            ],
        )
        .map_err(ats_certification_storage)?;
    if !existing.is_empty() {
        if existing.len() != 1 {
            return Err(AtsCertificationAuthorityError::IdentityConflict);
        }
        let row = &existing[0];
        let record = ats_application_binding_record(
            row.get(0),
            row.get(1),
            row.get(2),
            row.get(3),
            row.get(4),
            true,
        )?;
        if record.phase != "preflight"
            || record.fence != 0
            || record.authority.expires_at_ms <= now_ms
            || !ats_application_binding_request_matches(
                &record.authority,
                request,
                account_allowlist_sha256.as_deref(),
            )
        {
            return Err(AtsCertificationAuthorityError::IdentityConflict);
        }
        return Ok(record);
    }
    let certification = resolve_ats_certification_target_status_postgres_tx(
        tx,
        &request.target_evidence,
        Some(&request.runner_id),
        &request.rollout_channel,
        account_allowlist_sha256.as_deref(),
        now_ms,
    )?
    .ok_or(AtsCertificationAuthorityError::ScopeMismatch)?;
    let (authority, binding_sha256, frozen_base64url) =
        build_ats_application_binding_authority(request, certification, now_ms)?;
    let runtime = authority
        .certification
        .selected_runtime
        .as_ref()
        .ok_or(AtsCertificationAuthorityError::RuntimeMismatch)?;
    tx.execute(
        "INSERT INTO jobs_application_ats_certification_bindings (
           binding_id, binding_sha256, account_id, application_id, run_id, attempt_id,
           browser_session_id, browser_profile_id, packet_checksum_sha256,
           auto_authorization_id, auto_authorization_revision,
           auto_authorization_fingerprint_sha256, provider, target_key,
           manifest_sha256, activation_sha256, layout_set_sha256,
           adapter_bundle_sha256, runner_target_sha256, platform, architecture,
           automation_bundle_sha256, browser_release_manifest_sha256,
           browser_artifact_sha256, browser_build_descriptor_sha256, runner_build_id,
           runner_image_sha256, browser_runtime_sha256, chromium_executable_sha256,
           nonce_sha256, frozen_certification_base64url, expires_at_ms,
           phase, fence, created_at_ms
         ) VALUES (
           $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14,
           $15, $16, $17, $18, $19, $20, $21, $22, $23, $24, $25, $26,
           $27, $28, $29, $30, $31, $32, 'preflight', 0, $33
         )",
        &[
            &request.binding_id,
            &binding_sha256,
            &request.account_id,
            &request.application_id,
            &request.run_id,
            &request.application_attempt_id,
            &request.browser_session_id,
            &request.browser_profile_id,
            &request.packet_checksum_sha256,
            &request.auto_authorization_id,
            &request.auto_authorization_revision,
            &request.auto_authorization_fingerprint_sha256,
            &authority.certification.provider,
            &authority.certification.target_key,
            &authority.certification.manifest_sha256,
            &authority.certification.activation_sha256,
            &authority.certification.layout_set_sha256,
            &authority.certification.adapter_bundle_sha256,
            &runtime.runtime_sha256,
            &runtime.platform,
            &runtime.architecture,
            &runtime.automation_bundle_sha256,
            &runtime.browser_release_manifest_sha256,
            &runtime.browser_artifact_sha256,
            &runtime.browser_build_descriptor_sha256,
            &runtime.runner_build_id,
            &runtime.runner_image_sha256,
            &runtime.runtime_sha256,
            &runtime.chromium_executable_sha256,
            &request.nonce_sha256,
            &frozen_base64url,
            &authority.expires_at_ms,
            &now_ms,
        ],
    )
    .map_err(ats_certification_storage)?;
    Ok(AtsApplicationCertificationBindingRecord {
        binding_sha256,
        authority,
        phase: "preflight".to_string(),
        fence: 0,
        consumed_at_ms: None,
        replayed: false,
    })
}

fn validate_ats_phase_b_request(
    request: &AtsCertificationPhaseBRequest,
    now_ms: i64,
) -> Result<String, AtsCertificationAuthorityError> {
    for value in [
        request.binding_id.as_str(),
        request.account_id.as_str(),
        request.application_id.as_str(),
        request.run_id.as_str(),
        request.application_attempt_id.as_str(),
        request.auto_authorization_id.as_str(),
        request.phase_b_request_id.as_str(),
        request.canary_reservation_id.as_str(),
        request.period_key.as_str(),
    ] {
        if !ats_certification_text(value, 1, 240) {
            return Err(AtsCertificationAuthorityError::InvalidAuthority);
        }
    }
    if !ats_certification_hex64(&request.runner_id)
        || !ats_certification_hex64(&request.packet_checksum_sha256)
        || !ats_certification_hex64(&request.auto_authorization_fingerprint_sha256)
        || !ats_certification_hex64(&request.nonce_sha256)
        || !ats_certification_hex64(&request.metering_reservation_sha256)
        || request.auto_authorization_revision < 1
        || request.expected_fence != 0
        || !matches!(
            request.terminal_phase.as_str(),
            "consumed" | "side_effect_unknown"
        )
    {
        return Err(AtsCertificationAuthorityError::InvalidAuthority);
    }
    validate_ats_observed_surface(&request.observed_surface)?;
    validate_ats_certification_fresh_target_evidence(&request.target_evidence, now_ms)?;
    let canonical = ats_certification_canonical_json(request)
        .map_err(|_| AtsCertificationAuthorityError::InvalidAuthority)?;
    Ok(ats_certification_sha256(&canonical))
}

fn require_ats_phase_b_binding_match(
    record: &AtsApplicationCertificationBindingRecord,
    request: &AtsCertificationPhaseBRequest,
    now_ms: i64,
) -> Result<(), AtsCertificationAuthorityError> {
    let authority = &record.authority;
    let certification = &authority.certification;
    let (provider, target_key) =
        validate_ats_certification_fresh_target_evidence(&request.target_evidence, now_ms)?;
    if record.phase != "preflight"
        || record.fence != request.expected_fence
        || record.consumed_at_ms.is_some()
        || authority.expires_at_ms <= now_ms
        || authority.binding_id != request.binding_id
        || authority.account_id != request.account_id
        || authority.application_id != request.application_id
        || authority.run_id != request.run_id
        || authority.application_attempt_id != request.application_attempt_id
        || authority.packet_checksum_sha256 != request.packet_checksum_sha256
        || authority.auto_authorization_id != request.auto_authorization_id
        || authority.auto_authorization_revision != request.auto_authorization_revision
        || authority.auto_authorization_fingerprint_sha256
            != request.auto_authorization_fingerprint_sha256
        || authority.nonce_sha256 != request.nonce_sha256
        || certification.provider != provider
        || certification.target_key != target_key
        || certification.channel == "shadow"
        || certification
            .selected_runtime
            .as_ref()
            .is_none_or(|runtime| runtime.runtime_sha256 != request.runner_id)
    {
        return Err(AtsCertificationAuthorityError::ScopeMismatch);
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct AtsRuntimeLayoutQuarantineEvidence {
    schema_version: i64,
    evidence_kind: String,
    activation_sha256: String,
    manifest_sha256: String,
    scope_sha256: String,
    adapter_bundle_sha256: String,
    runtime_sha256: String,
    layout_set_sha256: String,
    observed_variant_key: String,
    observed_layout_contract_version: i64,
    observed_surface_sha256: String,
}

fn ats_runtime_layout_quarantine_evidence(
    authority: &AtsApplicationCertificationBindingAuthority,
    observed_surface: &AtsObservedSurface,
    overflow: bool,
) -> Result<AtsRuntimeLayoutQuarantineEvidence, AtsCertificationAuthorityError> {
    let certification = &authority.certification;
    let runtime = certification
        .selected_runtime
        .as_ref()
        .ok_or(AtsCertificationAuthorityError::RuntimeMismatch)?;
    validate_ats_observed_surface(observed_surface)?;
    Ok(AtsRuntimeLayoutQuarantineEvidence {
        schema_version: 1,
        evidence_kind: if overflow {
            "layout_drift_overflow".to_string()
        } else {
            "layout_drift".to_string()
        },
        activation_sha256: certification.activation_sha256.clone(),
        manifest_sha256: certification.manifest_sha256.clone(),
        scope_sha256: certification.scope_sha256.clone(),
        adapter_bundle_sha256: certification.adapter_bundle_sha256.clone(),
        runtime_sha256: runtime.runtime_sha256.clone(),
        layout_set_sha256: certification.layout_set_sha256.clone(),
        observed_variant_key: if overflow {
            "bounded-overflow".to_string()
        } else {
            observed_surface.variant_key.clone()
        },
        observed_layout_contract_version: if overflow {
            1
        } else {
            observed_surface.layout_contract_version
        },
        observed_surface_sha256: if overflow {
            "0".repeat(64)
        } else {
            observed_surface.surface_sha256.clone()
        },
    })
}

fn ats_runtime_layout_quarantine_evidence_identity(
    evidence: &AtsRuntimeLayoutQuarantineEvidence,
) -> Result<(String, String), AtsCertificationAuthorityError> {
    let canonical = ats_certification_canonical_json(evidence)
        .map_err(|_| AtsCertificationAuthorityError::InvalidAuthority)?;
    Ok((
        ats_certification_sha256(&canonical),
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(canonical),
    ))
}

fn ats_runtime_layout_drift_circuit_event(
    evidence_sha256: &str,
    runtime_sha256: &str,
    transition: &str,
    head_revision: i64,
    event_at_ms: i64,
) -> AtsCertificationCircuitEvent {
    AtsCertificationCircuitEvent {
        event_id: format!(
            "ats-layout-drift-{}-{head_revision}",
            &evidence_sha256[..24]
        ),
        scope_kind: "runtime".to_string(),
        subject_key: runtime_sha256.to_string(),
        transition: transition.to_string(),
        trigger_kind: "layout_drift".to_string(),
        window_started_at_ms: event_at_ms,
        window_ended_at_ms: event_at_ms,
        failure_count: 1,
        sample_count: 1,
        threshold_count: 1,
        authority_ref: format!("runtime-layout-quarantine:{evidence_sha256}"),
        event_at_ms,
    }
}

fn sqlite_ats_runtime_layout_evidence_exists(
    tx: &rusqlite::Transaction<'_>,
    evidence_sha256: &str,
    canonical_evidence_base64url: &str,
) -> Result<bool, AtsCertificationAuthorityError> {
    let stored = tx
        .query_row(
            "SELECT canonical_evidence_base64url, recorded_by
               FROM jobs_ats_certification_runtime_layout_quarantine_evidence
              WHERE evidence_sha256 = ?1",
            params![evidence_sha256],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()
        .map_err(ats_certification_storage)?;
    if let Some((stored_canonical, stored_by)) = stored {
        if stored_canonical != canonical_evidence_base64url
            || stored_by != ATS_CERTIFICATION_RUNTIME_LAYOUT_QUARANTINE_RECORDED_BY
        {
            return Err(AtsCertificationAuthorityError::IdentityConflict);
        }
        return Ok(true);
    }
    Ok(false)
}

fn insert_sqlite_ats_runtime_layout_quarantine_evidence(
    tx: &rusqlite::Transaction<'_>,
    evidence: &AtsRuntimeLayoutQuarantineEvidence,
    evidence_sha256: &str,
    canonical_evidence_base64url: &str,
    recorded_at_ms: i64,
) -> Result<bool, AtsCertificationAuthorityError> {
    let inserted = tx
        .execute(
            "INSERT INTO jobs_ats_certification_runtime_layout_quarantine_evidence (
               evidence_sha256, evidence_kind, activation_sha256, manifest_sha256,
               scope_sha256, adapter_bundle_sha256, runtime_sha256, layout_set_sha256,
               observed_variant_key, observed_layout_contract_version,
               observed_surface_sha256, canonical_evidence_base64url, recorded_by,
               recorded_at_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
             ON CONFLICT(evidence_sha256) DO NOTHING",
            params![
                evidence_sha256,
                evidence.evidence_kind,
                evidence.activation_sha256,
                evidence.manifest_sha256,
                evidence.scope_sha256,
                evidence.adapter_bundle_sha256,
                evidence.runtime_sha256,
                evidence.layout_set_sha256,
                evidence.observed_variant_key,
                evidence.observed_layout_contract_version,
                evidence.observed_surface_sha256,
                canonical_evidence_base64url,
                ATS_CERTIFICATION_RUNTIME_LAYOUT_QUARANTINE_RECORDED_BY,
                recorded_at_ms,
            ],
        )
        .map_err(ats_certification_storage)?;
    if inserted == 0
        && !sqlite_ats_runtime_layout_evidence_exists(
            tx,
            evidence_sha256,
            canonical_evidence_base64url,
        )?
    {
        return Err(AtsCertificationAuthorityError::IdentityConflict);
    }
    Ok(inserted == 1)
}

fn record_sqlite_ats_runtime_layout_drift(
    tx: &rusqlite::Transaction<'_>,
    authority: &AtsApplicationCertificationBindingAuthority,
    observed_surface: &AtsObservedSurface,
    now_ms: i64,
) -> Result<(), AtsCertificationAuthorityError> {
    let exact = ats_runtime_layout_quarantine_evidence(authority, observed_surface, false)?;
    let (exact_sha256, exact_canonical) = ats_runtime_layout_quarantine_evidence_identity(&exact)?;
    let exact_exists =
        sqlite_ats_runtime_layout_evidence_exists(tx, &exact_sha256, &exact_canonical)?;
    let (evidence, evidence_sha256, canonical_evidence_base64url) = if exact_exists {
        (exact, exact_sha256, exact_canonical)
    } else {
        let exact_count: i64 = tx
            .query_row(
                "SELECT COUNT(*)
                   FROM jobs_ats_certification_runtime_layout_quarantine_evidence
                  WHERE activation_sha256 = ?1 AND runtime_sha256 = ?2
                    AND evidence_kind = 'layout_drift'",
                params![exact.activation_sha256, exact.runtime_sha256],
                |row| row.get(0),
            )
            .map_err(ats_certification_storage)?;
        if exact_count < ATS_CERTIFICATION_MAX_RUNTIME_LAYOUT_QUARANTINE_EVIDENCE {
            (exact, exact_sha256, exact_canonical)
        } else {
            let overflow =
                ats_runtime_layout_quarantine_evidence(authority, observed_surface, true)?;
            let (overflow_sha256, overflow_canonical) =
                ats_runtime_layout_quarantine_evidence_identity(&overflow)?;
            (overflow, overflow_sha256, overflow_canonical)
        }
    };
    let inserted = insert_sqlite_ats_runtime_layout_quarantine_evidence(
        tx,
        &evidence,
        &evidence_sha256,
        &canonical_evidence_base64url,
        now_ms,
    )?;
    let current = tx
        .query_row(
            "SELECT head.state, head.head_revision, head.current_event_id, event.event_at_ms
               FROM jobs_ats_certification_circuit_heads head
               JOIN jobs_ats_certification_circuit_events event
                 ON event.event_id = head.current_event_id
              WHERE head.scope_kind = 'runtime' AND head.subject_key = ?1",
            params![evidence.runtime_sha256],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            },
        )
        .optional()
        .map_err(ats_certification_storage)?;
    if !inserted
        && current
            .as_ref()
            .is_some_and(|current| current.0 != "closed")
    {
        return Ok(());
    }
    let next_revision = current.as_ref().map_or(1, |current| current.1 + 1);
    let event_at_ms = current
        .as_ref()
        .map_or(now_ms, |current| now_ms.max(current.3.saturating_add(1)));
    let transition = if current.as_ref().is_none_or(|current| current.0 == "closed") {
        "opened"
    } else {
        "held"
    };
    let event = ats_runtime_layout_drift_circuit_event(
        &evidence_sha256,
        &evidence.runtime_sha256,
        transition,
        next_revision,
        event_at_ms,
    );
    append_ats_certification_circuit_event_sqlite_tx(
        tx,
        &event,
        next_revision - 1,
        current.as_ref().map(|current| current.2.as_str()),
        ATS_CERTIFICATION_RUNTIME_LAYOUT_QUARANTINE_RECORDED_BY,
        event_at_ms,
    )?;
    Ok(())
}

fn postgres_ats_runtime_layout_evidence_exists(
    tx: &mut postgres::Transaction<'_>,
    evidence_sha256: &str,
    canonical_evidence_base64url: &str,
) -> Result<bool, AtsCertificationAuthorityError> {
    let stored = tx
        .query_opt(
            "SELECT canonical_evidence_base64url, recorded_by
               FROM jobs_ats_certification_runtime_layout_quarantine_evidence
              WHERE evidence_sha256 = $1",
            &[&evidence_sha256],
        )
        .map_err(ats_certification_storage)?;
    if let Some(row) = stored {
        if row.get::<_, String>(0) != canonical_evidence_base64url
            || row.get::<_, String>(1) != ATS_CERTIFICATION_RUNTIME_LAYOUT_QUARANTINE_RECORDED_BY
        {
            return Err(AtsCertificationAuthorityError::IdentityConflict);
        }
        return Ok(true);
    }
    Ok(false)
}

fn insert_postgres_ats_runtime_layout_quarantine_evidence(
    tx: &mut postgres::Transaction<'_>,
    evidence: &AtsRuntimeLayoutQuarantineEvidence,
    evidence_sha256: &str,
    canonical_evidence_base64url: &str,
    recorded_at_ms: i64,
) -> Result<bool, AtsCertificationAuthorityError> {
    let inserted = tx
        .execute(
            "INSERT INTO jobs_ats_certification_runtime_layout_quarantine_evidence (
               evidence_sha256, evidence_kind, activation_sha256, manifest_sha256,
               scope_sha256, adapter_bundle_sha256, runtime_sha256, layout_set_sha256,
               observed_variant_key, observed_layout_contract_version,
               observed_surface_sha256, canonical_evidence_base64url, recorded_by,
               recorded_at_ms
             ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
             ON CONFLICT(evidence_sha256) DO NOTHING",
            &[
                &evidence_sha256,
                &evidence.evidence_kind,
                &evidence.activation_sha256,
                &evidence.manifest_sha256,
                &evidence.scope_sha256,
                &evidence.adapter_bundle_sha256,
                &evidence.runtime_sha256,
                &evidence.layout_set_sha256,
                &evidence.observed_variant_key,
                &evidence.observed_layout_contract_version,
                &evidence.observed_surface_sha256,
                &canonical_evidence_base64url,
                &ATS_CERTIFICATION_RUNTIME_LAYOUT_QUARANTINE_RECORDED_BY,
                &recorded_at_ms,
            ],
        )
        .map_err(ats_certification_storage)?;
    if inserted == 0
        && !postgres_ats_runtime_layout_evidence_exists(
            tx,
            evidence_sha256,
            canonical_evidence_base64url,
        )?
    {
        return Err(AtsCertificationAuthorityError::IdentityConflict);
    }
    Ok(inserted == 1)
}

fn record_postgres_ats_runtime_layout_drift(
    tx: &mut postgres::Transaction<'_>,
    authority: &AtsApplicationCertificationBindingAuthority,
    observed_surface: &AtsObservedSurface,
    now_ms: i64,
) -> Result<(), AtsCertificationAuthorityError> {
    let exact = ats_runtime_layout_quarantine_evidence(authority, observed_surface, false)?;
    let (exact_sha256, exact_canonical) = ats_runtime_layout_quarantine_evidence_identity(&exact)?;
    let exact_exists =
        postgres_ats_runtime_layout_evidence_exists(tx, &exact_sha256, &exact_canonical)?;
    let (evidence, evidence_sha256, canonical_evidence_base64url) = if exact_exists {
        (exact, exact_sha256, exact_canonical)
    } else {
        let exact_count: i64 = tx
            .query_one(
                "SELECT COUNT(*)
                   FROM jobs_ats_certification_runtime_layout_quarantine_evidence
                  WHERE activation_sha256 = $1 AND runtime_sha256 = $2
                    AND evidence_kind = 'layout_drift'",
                &[&exact.activation_sha256, &exact.runtime_sha256],
            )
            .map_err(ats_certification_storage)?
            .get(0);
        if exact_count < ATS_CERTIFICATION_MAX_RUNTIME_LAYOUT_QUARANTINE_EVIDENCE {
            (exact, exact_sha256, exact_canonical)
        } else {
            let overflow =
                ats_runtime_layout_quarantine_evidence(authority, observed_surface, true)?;
            let (overflow_sha256, overflow_canonical) =
                ats_runtime_layout_quarantine_evidence_identity(&overflow)?;
            (overflow, overflow_sha256, overflow_canonical)
        }
    };
    let inserted = insert_postgres_ats_runtime_layout_quarantine_evidence(
        tx,
        &evidence,
        &evidence_sha256,
        &canonical_evidence_base64url,
        now_ms,
    )?;
    let current = tx
        .query_opt(
            "SELECT head.state, head.head_revision, head.current_event_id, event.event_at_ms
               FROM jobs_ats_certification_circuit_heads head
               JOIN jobs_ats_certification_circuit_events event
                 ON event.event_id = head.current_event_id
              WHERE head.scope_kind = 'runtime' AND head.subject_key = $1 FOR UPDATE",
            &[&evidence.runtime_sha256],
        )
        .map_err(ats_certification_storage)?
        .map(|row| {
            (
                row.get::<_, String>(0),
                row.get::<_, i64>(1),
                row.get::<_, String>(2),
                row.get::<_, i64>(3),
            )
        });
    if !inserted
        && current
            .as_ref()
            .is_some_and(|current| current.0 != "closed")
    {
        return Ok(());
    }
    let next_revision = current.as_ref().map_or(1, |current| current.1 + 1);
    let event_at_ms = current
        .as_ref()
        .map_or(now_ms, |current| now_ms.max(current.3.saturating_add(1)));
    let transition = if current.as_ref().is_none_or(|current| current.0 == "closed") {
        "opened"
    } else {
        "held"
    };
    let event = ats_runtime_layout_drift_circuit_event(
        &evidence_sha256,
        &evidence.runtime_sha256,
        transition,
        next_revision,
        event_at_ms,
    );
    append_ats_certification_circuit_event_postgres_tx(
        tx,
        &event,
        next_revision - 1,
        current.as_ref().map(|current| current.2.as_str()),
        ATS_CERTIFICATION_RUNTIME_LAYOUT_QUARANTINE_RECORDED_BY,
        event_at_ms,
    )?;
    Ok(())
}

fn sqlite_ats_phase_b_observed_surface(
    tx: &rusqlite::Transaction<'_>,
    authority: &AtsApplicationCertificationBindingAuthority,
    observed_surface: &AtsObservedSurface,
    now_ms: i64,
) -> Result<Option<(String, AtsObservedSurface)>, AtsCertificationAuthorityError> {
    let certification = &authority.certification;
    let runtime = certification
        .selected_runtime
        .as_ref()
        .ok_or(AtsCertificationAuthorityError::RuntimeMismatch)?;
    let target_fingerprint = ats_certification_target_fingerprint_sha256(
        &certification.provider,
        &certification.target_key,
    )?;
    if observed_surface.layout_contract_version != certification.layout_contract_version {
        return Ok(None);
    }
    tx.query_row(
        "SELECT observation.observation_sha256, observation.page_variant,
                observation.surface_sha256
           FROM jobs_ats_certification_manifest_layouts binding
           JOIN jobs_ats_certification_layout_observations observation
             ON observation.observation_sha256 = binding.observation_sha256
          WHERE binding.manifest_sha256 = ?1 AND observation.provider = ?2
            AND observation.target_fingerprint_sha256 = ?3
            AND observation.page_variant = ?4 AND observation.surface_sha256 = ?5
            AND observation.adapter_version = ?6
            AND observation.runner_target_sha256 = ?7
            AND observation.evidence_class IN ('authorized_sandbox', 'authorized_live')
            AND observation.trust_policy_sha256 = ?8
            AND observation.expires_at_ms > ?9
          ORDER BY binding.ordinal, observation.observation_sha256 LIMIT 1",
        params![
            certification.manifest_sha256,
            certification.provider,
            target_fingerprint,
            observed_surface.variant_key,
            observed_surface.surface_sha256,
            certification.adapter_version,
            runtime.runtime_sha256,
            certification.trust_policy_sha256,
            now_ms,
        ],
        |row| {
            Ok((
                row.get(0)?,
                AtsObservedSurface {
                    variant_key: row.get(1)?,
                    layout_contract_version: certification.layout_contract_version,
                    surface_sha256: row.get(2)?,
                },
            ))
        },
    )
    .optional()
    .map_err(ats_certification_storage)
}

fn postgres_ats_phase_b_observed_surface(
    tx: &mut postgres::Transaction<'_>,
    authority: &AtsApplicationCertificationBindingAuthority,
    observed_surface: &AtsObservedSurface,
    now_ms: i64,
) -> Result<Option<(String, AtsObservedSurface)>, AtsCertificationAuthorityError> {
    let certification = &authority.certification;
    let runtime = certification
        .selected_runtime
        .as_ref()
        .ok_or(AtsCertificationAuthorityError::RuntimeMismatch)?;
    let target_fingerprint = ats_certification_target_fingerprint_sha256(
        &certification.provider,
        &certification.target_key,
    )?;
    if observed_surface.layout_contract_version != certification.layout_contract_version {
        return Ok(None);
    }
    let row = tx
        .query_opt(
            "SELECT observation.observation_sha256, observation.page_variant,
                    observation.surface_sha256
               FROM jobs_ats_certification_manifest_layouts binding
               JOIN jobs_ats_certification_layout_observations observation
                 ON observation.observation_sha256 = binding.observation_sha256
              WHERE binding.manifest_sha256 = $1 AND observation.provider = $2
                AND observation.target_fingerprint_sha256 = $3
                AND observation.page_variant = $4 AND observation.surface_sha256 = $5
                AND observation.adapter_version = $6
                AND observation.runner_target_sha256 = $7
                AND observation.evidence_class IN ('authorized_sandbox', 'authorized_live')
                AND observation.trust_policy_sha256 = $8
                AND observation.expires_at_ms > $9
              ORDER BY binding.ordinal, observation.observation_sha256 LIMIT 1",
            &[
                &certification.manifest_sha256,
                &certification.provider,
                &target_fingerprint,
                &observed_surface.variant_key,
                &observed_surface.surface_sha256,
                &certification.adapter_version,
                &runtime.runtime_sha256,
                &certification.trust_policy_sha256,
                &now_ms,
            ],
        )
        .map_err(ats_certification_storage)?;
    Ok(row.map(|row| {
        (
            row.get(0),
            AtsObservedSurface {
                variant_key: row.get(1),
                layout_contract_version: certification.layout_contract_version,
                surface_sha256: row.get(2),
            },
        )
    }))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AtsCanaryReservationAuthority<'a> {
    version: i64,
    reservation_id: &'a str,
    binding_sha256: &'a str,
    activation_sha256: &'a str,
    manifest_sha256: &'a str,
    target_key_sha256: &'a str,
    runner_target_sha256: &'a str,
    account_id: &'a str,
    application_id: &'a str,
    run_id: &'a str,
    application_attempt_id: &'a str,
    period_key: &'a str,
    rollout_channel: &'a str,
    metering_reservation_sha256: &'a str,
    reserved_at_ms: i64,
}

fn ats_canary_reservation_sha256(
    record: &AtsApplicationCertificationBindingRecord,
    request: &AtsCertificationPhaseBRequest,
    reserved_at_ms: i64,
) -> Result<String, AtsCertificationAuthorityError> {
    let certification = &record.authority.certification;
    let runtime = certification
        .selected_runtime
        .as_ref()
        .ok_or(AtsCertificationAuthorityError::RuntimeMismatch)?;
    let target_key_sha256 = ats_certification_target_key_sha256(&certification.target_key)?;
    let authority = AtsCanaryReservationAuthority {
        version: 1,
        reservation_id: &request.canary_reservation_id,
        binding_sha256: &record.binding_sha256,
        activation_sha256: &certification.activation_sha256,
        manifest_sha256: &certification.manifest_sha256,
        target_key_sha256: &target_key_sha256,
        runner_target_sha256: &runtime.runtime_sha256,
        account_id: &request.account_id,
        application_id: &request.application_id,
        run_id: &request.run_id,
        application_attempt_id: &request.application_attempt_id,
        period_key: &request.period_key,
        rollout_channel: &certification.channel,
        metering_reservation_sha256: &request.metering_reservation_sha256,
        reserved_at_ms,
    };
    let canonical = ats_certification_canonical_json(&authority)
        .map_err(|_| AtsCertificationAuthorityError::InvalidAuthority)?;
    Ok(ats_certification_sha256(&canonical))
}

fn ats_certified_receipt_authority(
    record: &AtsApplicationCertificationBindingRecord,
    request: &AtsCertificationPhaseBRequest,
    phase_b_request_sha256: &str,
    layout_observation_sha256: &str,
    canary_reservation_sha256: &str,
    consumed_at_ms: i64,
) -> Result<AtsCertifiedReceiptAuthority, AtsCertificationAuthorityError> {
    let certification = &record.authority.certification;
    let runtime = certification
        .selected_runtime
        .as_ref()
        .ok_or(AtsCertificationAuthorityError::RuntimeMismatch)?;
    if !ats_certification_hex64(phase_b_request_sha256) {
        return Err(AtsCertificationAuthorityError::InvalidAuthority);
    }
    Ok(AtsCertifiedReceiptAuthority {
        schema_version: 1,
        account_id: record.authority.account_id.clone(),
        application_id: record.authority.application_id.clone(),
        run_id: record.authority.run_id.clone(),
        provider: certification.provider.clone(),
        adapter: certification.provider.clone(),
        adapter_version: certification.adapter_version.clone(),
        manifest_sha256: certification.manifest_sha256.clone(),
        activation_sha256: certification.activation_sha256.clone(),
        activation_generation: certification.activation_generation,
        target_key_sha256: ats_certification_target_key_sha256(&certification.target_key)?,
        layout_set_sha256: certification.layout_set_sha256.clone(),
        layout_observation_sha256: layout_observation_sha256.to_string(),
        observed_surface_sha256: request.observed_surface.surface_sha256.clone(),
        adapter_bundle_sha256: certification.adapter_bundle_sha256.clone(),
        runner_kind: runtime.runtime_kind.clone(),
        runner_target_sha256: runtime.runtime_sha256.clone(),
        binding_sha256: record.binding_sha256.clone(),
        binding_fence: 1,
        binding_consumed_at_ms: consumed_at_ms,
        application_attempt_id: record.authority.application_attempt_id.clone(),
        phase_b_request_id: request.phase_b_request_id.clone(),
        rollout_channel: certification.channel.clone(),
        canary_reservation_sha256: canary_reservation_sha256.to_string(),
        metering_reservation_sha256: request.metering_reservation_sha256.clone(),
    })
}

fn require_sqlite_ats_binding_canary_membership(
    tx: &rusqlite::Transaction<'_>,
    record: &AtsApplicationCertificationBindingRecord,
    now_ms: i64,
) -> Result<(), AtsCertificationAuthorityError> {
    let certification = &record.authority.certification;
    if certification.channel != "canary" {
        return Ok(());
    }
    let allowlist_sha256 = certification
        .account_allowlist_sha256
        .as_deref()
        .ok_or(AtsCertificationAuthorityError::ScopeMismatch)?;
    if !sqlite_ats_account_enrolled_in_allowlist(
        tx,
        &record.authority.account_id,
        allowlist_sha256,
        now_ms,
    )? {
        return Err(AtsCertificationAuthorityError::ScopeMismatch);
    }
    Ok(())
}

fn require_postgres_ats_binding_canary_membership(
    tx: &mut postgres::Transaction<'_>,
    record: &AtsApplicationCertificationBindingRecord,
    now_ms: i64,
) -> Result<(), AtsCertificationAuthorityError> {
    let certification = &record.authority.certification;
    if certification.channel != "canary" {
        return Ok(());
    }
    let allowlist_sha256 = certification
        .account_allowlist_sha256
        .as_deref()
        .ok_or(AtsCertificationAuthorityError::ScopeMismatch)?;
    if !postgres_ats_account_enrolled_in_allowlist(
        tx,
        &record.authority.account_id,
        allowlist_sha256,
        now_ms,
    )? {
        return Err(AtsCertificationAuthorityError::ScopeMismatch);
    }
    Ok(())
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AtsCertificationMeteringReservationAuthority<'a> {
    schema_version: i64,
    account_id: &'a str,
    application_id: &'a str,
    run_id: &'a str,
    application_attempt_id: &'a str,
    company_key: &'a str,
    period_key: &'a str,
    runner: &'a str,
    attempt_status: &'a str,
    reserved_at_ms: i64,
    updated_at_ms: i64,
    binding_sha256: &'a str,
}

#[allow(clippy::too_many_arguments)]
fn ats_phase_b_server_ids(
    record: &AtsApplicationCertificationBindingRecord,
    company_key: &str,
    period_key: &str,
    attempt_runner: &str,
    attempt_status: &str,
    reserved_at_ms: i64,
    updated_at_ms: i64,
) -> Result<(String, String, String), AtsCertificationAuthorityError> {
    let authority = &record.authority;
    let metering = AtsCertificationMeteringReservationAuthority {
        schema_version: 1,
        account_id: &authority.account_id,
        application_id: &authority.application_id,
        run_id: &authority.run_id,
        application_attempt_id: &authority.application_attempt_id,
        company_key,
        period_key,
        runner: attempt_runner,
        attempt_status,
        reserved_at_ms,
        updated_at_ms,
        binding_sha256: &record.binding_sha256,
    };
    let canonical = ats_certification_canonical_json(&metering)
        .map_err(|_| AtsCertificationAuthorityError::InvalidAuthority)?;
    let metering_reservation_sha256 = ats_certification_sha256(&canonical);
    let phase_b_request_id = format!("ats-phase-b-{}", &metering_reservation_sha256[..32]);
    let canary_reservation_id = format!("ats-capacity-{}", &record.binding_sha256[..32]);
    Ok((
        metering_reservation_sha256,
        phase_b_request_id,
        canary_reservation_id,
    ))
}

fn validate_ats_phase_b_context_request(
    request: &AtsCertificationPhaseBContextRequest,
    now_ms: i64,
) -> Result<(), AtsCertificationAuthorityError> {
    for value in [
        request.account_id.as_str(),
        request.application_id.as_str(),
        request.run_id.as_str(),
    ] {
        if !ats_certification_text(value, 1, 240) {
            return Err(AtsCertificationAuthorityError::InvalidAuthority);
        }
    }
    if !matches!(request.runner_kind.as_str(), "local" | "cloud")
        || !matches!(
            request.terminal_phase.as_str(),
            "consumed" | "side_effect_unknown"
        )
        || !ats_certification_safe_integer(now_ms, false)
    {
        return Err(AtsCertificationAuthorityError::InvalidAuthority);
    }
    validate_ats_observed_surface(&request.observed_surface)
}

pub fn validate_consume_reserve_ats_application_certification_from_context_sqlite_tx(
    tx: &rusqlite::Transaction<'_>,
    request: &AtsCertificationPhaseBContextRequest,
    now_ms: i64,
) -> Result<AtsCertificationPhaseBTransactionOutcome, AtsCertificationAuthorityError> {
    validate_ats_phase_b_context_request(request, now_ms)?;
    let mut stmt = tx
        .prepare(
            "SELECT binding_sha256, frozen_certification_base64url, phase, fence,
                    consumed_at_ms
               FROM jobs_application_ats_certification_bindings
              WHERE account_id = ?1 AND application_id = ?2 AND run_id = ?3
                AND phase = 'preflight' LIMIT 2",
        )
        .map_err(ats_certification_storage)?;
    let rows = stmt
        .query_map(
            params![request.account_id, request.application_id, request.run_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, Option<i64>>(4)?,
                ))
            },
        )
        .map_err(ats_certification_storage)?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(ats_certification_storage)?;
    if rows.len() != 1 {
        return Err(if rows.is_empty() {
            AtsCertificationAuthorityError::NotFound
        } else {
            AtsCertificationAuthorityError::IdentityConflict
        });
    }
    let row = rows
        .into_iter()
        .next()
        .ok_or(AtsCertificationAuthorityError::NotFound)?;
    let record = ats_application_binding_record(row.0, row.1, row.2, row.3, row.4, false)?;
    let runtime = record
        .authority
        .certification
        .selected_runtime
        .as_ref()
        .ok_or(AtsCertificationAuthorityError::RuntimeMismatch)?;
    if runtime.runtime_kind != request.runner_kind {
        return Err(AtsCertificationAuthorityError::RuntimeMismatch);
    }
    let (job_id, application_json, state) = tx
        .query_row(
            "SELECT job_id, application_json, state FROM jobs_applications
              WHERE account_id = ?1 AND id = ?2",
            params![request.account_id, request.application_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            },
        )
        .optional()
        .map_err(ats_certification_storage)?
        .ok_or(AtsCertificationAuthorityError::NotFound)?;
    let application = parse_application_json(
        application_json,
        &request.application_id,
        &job_id,
        "ATS Phase B application",
    )
    .map_err(ats_certification_storage)?;
    if application.state != state
        || !matches!(state.as_str(), "queued" | "running")
        || application.run_id.as_deref() != Some(request.run_id.as_str())
    {
        return Err(AtsCertificationAuthorityError::ScopeMismatch);
    }
    let admission = ats_frozen_application_admission(&application, now_ms)?;
    if admission.packet_checksum_sha256 != record.authority.packet_checksum_sha256
        || admission.auto_authorization_id != record.authority.auto_authorization_id
        || admission.auto_authorization_revision != record.authority.auto_authorization_revision
        || admission.auto_authorization_fingerprint_sha256
            != record.authority.auto_authorization_fingerprint_sha256
        || admission.certification
            != ats_frozen_certification_admission_projection(&record.authority.certification)?
    {
        return Err(AtsCertificationAuthorityError::ScopeMismatch);
    }
    let (posting_id, posting_json) = tx
        .query_row(
            "SELECT id, posting_json FROM jobs_postings
              WHERE account_id = ?1 AND id = ?2",
            params![request.account_id, job_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()
        .map_err(ats_certification_storage)?
        .ok_or(AtsCertificationAuthorityError::NotFound)?;
    let mut posting: JobPosting =
        parse_json(posting_json, "ATS Phase B posting").map_err(ats_certification_storage)?;
    posting.id = posting_id;
    let target_evidence = ats_certification_fresh_target_evidence_from_posting(&posting, now_ms)?;
    if target_evidence != record.authority.target_evidence {
        return Err(AtsCertificationAuthorityError::ScopeMismatch);
    }
    let attempt = tx
        .query_row(
            "SELECT company_key, period_key, runner, status, reserved_at_ms, updated_at_ms
               FROM jobs_attempt_reservations
              WHERE id = ?1 AND account_id = ?2 AND application_id = ?3",
            params![
                record.authority.application_attempt_id,
                request.account_id,
                request.application_id,
            ],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, i64>(5)?,
                ))
            },
        )
        .optional()
        .map_err(ats_certification_storage)?
        .ok_or(AtsCertificationAuthorityError::NotFound)?;
    if !matches!(attempt.3.as_str(), "reserved" | "running") || attempt.2 != runtime.runtime_kind {
        return Err(AtsCertificationAuthorityError::ScopeMismatch);
    }
    let (metering_reservation_sha256, phase_b_request_id, canary_reservation_id) =
        ats_phase_b_server_ids(
            &record, &attempt.0, &attempt.1, &attempt.2, &attempt.3, attempt.4, attempt.5,
        )?;
    let raw_request = AtsCertificationPhaseBRequest {
        binding_id: record.authority.binding_id.clone(),
        account_id: request.account_id.clone(),
        application_id: request.application_id.clone(),
        run_id: request.run_id.clone(),
        application_attempt_id: record.authority.application_attempt_id.clone(),
        packet_checksum_sha256: record.authority.packet_checksum_sha256.clone(),
        auto_authorization_id: record.authority.auto_authorization_id.clone(),
        auto_authorization_revision: record.authority.auto_authorization_revision,
        auto_authorization_fingerprint_sha256: record
            .authority
            .auto_authorization_fingerprint_sha256
            .clone(),
        target_evidence,
        runner_id: runtime.runtime_sha256.clone(),
        nonce_sha256: record.authority.nonce_sha256.clone(),
        observed_surface: request.observed_surface.clone(),
        phase_b_request_id,
        metering_reservation_sha256,
        canary_reservation_id,
        period_key: ats_certification_canary_utc_period_key(now_ms)?,
        expected_fence: record.fence,
        terminal_phase: request.terminal_phase.clone(),
    };
    validate_consume_reserve_ats_application_certification_sqlite_tx(tx, &raw_request, now_ms)
}

pub fn validate_consume_reserve_ats_application_certification_from_context_postgres_tx(
    tx: &mut postgres::Transaction<'_>,
    request: &AtsCertificationPhaseBContextRequest,
    now_ms: i64,
) -> Result<AtsCertificationPhaseBTransactionOutcome, AtsCertificationAuthorityError> {
    lock_postgres_ats_certification(tx)?;
    validate_consume_reserve_ats_application_certification_from_context_postgres_tx_after_prelock(
        tx, request, now_ms,
    )
}

pub(crate) fn validate_consume_reserve_ats_application_certification_from_context_postgres_tx_after_prelock(
    tx: &mut postgres::Transaction<'_>,
    request: &AtsCertificationPhaseBContextRequest,
    now_ms: i64,
) -> Result<AtsCertificationPhaseBTransactionOutcome, AtsCertificationAuthorityError> {
    validate_ats_phase_b_context_request(request, now_ms)?;
    let rows = tx
        .query(
            "SELECT binding_sha256, frozen_certification_base64url, phase, fence,
                    consumed_at_ms
               FROM jobs_application_ats_certification_bindings
              WHERE account_id = $1 AND application_id = $2 AND run_id = $3
                AND phase = 'preflight' LIMIT 2 FOR UPDATE",
            &[
                &request.account_id,
                &request.application_id,
                &request.run_id,
            ],
        )
        .map_err(ats_certification_storage)?;
    if rows.len() != 1 {
        return Err(if rows.is_empty() {
            AtsCertificationAuthorityError::NotFound
        } else {
            AtsCertificationAuthorityError::IdentityConflict
        });
    }
    let row = &rows[0];
    let record = ats_application_binding_record(
        row.get(0),
        row.get(1),
        row.get(2),
        row.get(3),
        row.get(4),
        false,
    )?;
    let runtime = record
        .authority
        .certification
        .selected_runtime
        .as_ref()
        .ok_or(AtsCertificationAuthorityError::RuntimeMismatch)?;
    if runtime.runtime_kind != request.runner_kind {
        return Err(AtsCertificationAuthorityError::RuntimeMismatch);
    }
    let row = tx
        .query_opt(
            "SELECT job_id, application_json, state FROM jobs_applications
              WHERE account_id = $1 AND id = $2 FOR UPDATE",
            &[&request.account_id, &request.application_id],
        )
        .map_err(ats_certification_storage)?
        .ok_or(AtsCertificationAuthorityError::NotFound)?;
    let job_id: String = row.get(0);
    let application = parse_application_json(
        row.get(1),
        &request.application_id,
        &job_id,
        "ATS Phase B application",
    )
    .map_err(ats_certification_storage)?;
    let state: String = row.get(2);
    if application.state != state
        || !matches!(state.as_str(), "queued" | "running")
        || application.run_id.as_deref() != Some(request.run_id.as_str())
    {
        return Err(AtsCertificationAuthorityError::ScopeMismatch);
    }
    let admission = ats_frozen_application_admission(&application, now_ms)?;
    if admission.packet_checksum_sha256 != record.authority.packet_checksum_sha256
        || admission.auto_authorization_id != record.authority.auto_authorization_id
        || admission.auto_authorization_revision != record.authority.auto_authorization_revision
        || admission.auto_authorization_fingerprint_sha256
            != record.authority.auto_authorization_fingerprint_sha256
        || admission.certification
            != ats_frozen_certification_admission_projection(&record.authority.certification)?
    {
        return Err(AtsCertificationAuthorityError::ScopeMismatch);
    }
    let row = tx
        .query_opt(
            "SELECT id, posting_json FROM jobs_postings
              WHERE account_id = $1 AND id = $2 FOR UPDATE",
            &[&request.account_id, &job_id],
        )
        .map_err(ats_certification_storage)?
        .ok_or(AtsCertificationAuthorityError::NotFound)?;
    let mut posting: JobPosting =
        parse_json(row.get(1), "ATS Phase B posting").map_err(ats_certification_storage)?;
    posting.id = row.get(0);
    let target_evidence = ats_certification_fresh_target_evidence_from_posting(&posting, now_ms)?;
    if target_evidence != record.authority.target_evidence {
        return Err(AtsCertificationAuthorityError::ScopeMismatch);
    }
    let row = tx
        .query_opt(
            "SELECT company_key, period_key, runner, status, reserved_at_ms, updated_at_ms
               FROM jobs_attempt_reservations
              WHERE id = $1 AND account_id = $2 AND application_id = $3 FOR UPDATE",
            &[
                &record.authority.application_attempt_id,
                &request.account_id,
                &request.application_id,
            ],
        )
        .map_err(ats_certification_storage)?
        .ok_or(AtsCertificationAuthorityError::NotFound)?;
    let attempt = (
        row.get::<_, String>(0),
        row.get::<_, String>(1),
        row.get::<_, String>(2),
        row.get::<_, String>(3),
        row.get::<_, i64>(4),
        row.get::<_, i64>(5),
    );
    if !matches!(attempt.3.as_str(), "reserved" | "running") || attempt.2 != runtime.runtime_kind {
        return Err(AtsCertificationAuthorityError::ScopeMismatch);
    }
    let (metering_reservation_sha256, phase_b_request_id, canary_reservation_id) =
        ats_phase_b_server_ids(
            &record, &attempt.0, &attempt.1, &attempt.2, &attempt.3, attempt.4, attempt.5,
        )?;
    let raw_request = AtsCertificationPhaseBRequest {
        binding_id: record.authority.binding_id.clone(),
        account_id: request.account_id.clone(),
        application_id: request.application_id.clone(),
        run_id: request.run_id.clone(),
        application_attempt_id: record.authority.application_attempt_id.clone(),
        packet_checksum_sha256: record.authority.packet_checksum_sha256.clone(),
        auto_authorization_id: record.authority.auto_authorization_id.clone(),
        auto_authorization_revision: record.authority.auto_authorization_revision,
        auto_authorization_fingerprint_sha256: record
            .authority
            .auto_authorization_fingerprint_sha256
            .clone(),
        target_evidence,
        runner_id: runtime.runtime_sha256.clone(),
        nonce_sha256: record.authority.nonce_sha256.clone(),
        observed_surface: request.observed_surface.clone(),
        phase_b_request_id,
        metering_reservation_sha256,
        canary_reservation_id,
        period_key: ats_certification_canary_utc_period_key(now_ms)?,
        expected_fence: record.fence,
        terminal_phase: request.terminal_phase.clone(),
    };
    validate_consume_reserve_ats_application_certification_postgres_tx_after_prelock(
        tx,
        &raw_request,
        now_ms,
    )
}

pub fn validate_consume_reserve_ats_application_certification(
    pool: &DbPool,
    request: &AtsCertificationPhaseBRequest,
    now_ms: i64,
) -> Result<AtsCertificationPhaseBResult, AtsCertificationAuthorityError> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get().map_err(ats_certification_storage)?;
            let tx = conn
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(ats_certification_storage)?;
            let outcome = validate_consume_reserve_ats_application_certification_sqlite_tx(
                &tx, request, now_ms,
            )?;
            tx.commit().map_err(ats_certification_storage)?;
            outcome.into_result()
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg().map_err(ats_certification_storage)?;
            let mut tx = conn
                .build_transaction()
                .isolation_level(postgres::IsolationLevel::Serializable)
                .start()
                .map_err(ats_certification_storage)?;
            lock_postgres_ats_certification(&mut tx)?;
            let outcome =
                validate_consume_reserve_ats_application_certification_postgres_tx_after_prelock(
                    &mut tx, request, now_ms,
                )?;
            tx.commit().map_err(ats_certification_storage)?;
            outcome.into_result()
        }
    })
}

pub fn validate_consume_reserve_ats_application_certification_sqlite_tx(
    tx: &rusqlite::Transaction<'_>,
    request: &AtsCertificationPhaseBRequest,
    now_ms: i64,
) -> Result<AtsCertificationPhaseBTransactionOutcome, AtsCertificationAuthorityError> {
    let phase_b_request_sha256 = validate_ats_phase_b_request(request, now_ms)?;
    let row = tx
        .query_row(
            "SELECT binding_sha256, frozen_certification_base64url, phase, fence,
                    consumed_at_ms
               FROM jobs_application_ats_certification_bindings
              WHERE binding_id = ?1",
            params![request.binding_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, Option<i64>>(4)?,
                ))
            },
        )
        .optional()
        .map_err(ats_certification_storage)?
        .ok_or(AtsCertificationAuthorityError::NotFound)?;
    let record = ats_application_binding_record(row.0, row.1, row.2, row.3, row.4, false)?;
    require_ats_phase_b_binding_match(&record, request, now_ms)?;
    let observed = sqlite_ats_phase_b_observed_surface(
        tx,
        &record.authority,
        &request.observed_surface,
        now_ms,
    )?;
    let Some((layout_observation_sha256, surface)) = observed else {
        record_sqlite_ats_runtime_layout_drift(
            tx,
            &record.authority,
            &request.observed_surface,
            now_ms,
        )?;
        return Ok(AtsCertificationPhaseBTransactionOutcome::LayoutDriftQuarantined);
    };
    if surface != request.observed_surface {
        record_sqlite_ats_runtime_layout_drift(
            tx,
            &record.authority,
            &request.observed_surface,
            now_ms,
        )?;
        return Ok(AtsCertificationPhaseBTransactionOutcome::LayoutDriftQuarantined);
    }
    validate_frozen_ats_certification_sqlite_tx(
        tx,
        &record.authority.certification,
        &request.target_evidence.canonical_url,
        &request.runner_id,
        &surface,
        now_ms,
    )?;
    require_sqlite_ats_binding_canary_membership(tx, &record, now_ms)?;
    require_sqlite_ats_canary_capacity(tx, &record, request, now_ms)?;
    let reservation_sha256 = ats_canary_reservation_sha256(&record, request, now_ms)?;
    insert_sqlite_ats_certification_reservation(tx, &record, request, &reservation_sha256, now_ms)?;
    let changed = tx
        .execute(
            "UPDATE jobs_application_ats_certification_bindings
                SET layout_observation_sha256 = ?1, phase_b_request_id = ?2,
                    phase_b_request_sha256 = ?3, canary_reservation_sha256 = ?4,
                    metering_reservation_sha256 = ?5, observed_surface_sha256 = ?6,
                    phase = ?7, fence = 1, consumed_at_ms = ?8
              WHERE binding_id = ?9 AND phase = 'preflight' AND fence = ?10
                AND consumed_at_ms IS NULL AND expires_at_ms > ?8",
            params![
                layout_observation_sha256,
                request.phase_b_request_id,
                phase_b_request_sha256,
                reservation_sha256,
                request.metering_reservation_sha256,
                request.observed_surface.surface_sha256,
                request.terminal_phase,
                now_ms,
                request.binding_id,
                request.expected_fence,
            ],
        )
        .map_err(ats_certification_storage)?;
    if changed != 1 {
        return Err(AtsCertificationAuthorityError::CompareAndSwapConflict);
    }
    let receipt = ats_certified_receipt_authority(
        &record,
        request,
        &phase_b_request_sha256,
        &layout_observation_sha256,
        &reservation_sha256,
        now_ms,
    )?;
    Ok(AtsCertificationPhaseBTransactionOutcome::Authorized(
        Box::new(AtsCertificationPhaseBResult {
            ats_certified_receipt_authority: receipt,
            terminal_phase: request.terminal_phase.clone(),
        }),
    ))
}

pub fn validate_consume_reserve_ats_application_certification_postgres_tx(
    tx: &mut postgres::Transaction<'_>,
    request: &AtsCertificationPhaseBRequest,
    now_ms: i64,
) -> Result<AtsCertificationPhaseBTransactionOutcome, AtsCertificationAuthorityError> {
    lock_postgres_ats_certification(tx)?;
    validate_consume_reserve_ats_application_certification_postgres_tx_after_prelock(
        tx, request, now_ms,
    )
}

pub(crate) fn validate_consume_reserve_ats_application_certification_postgres_tx_after_prelock(
    tx: &mut postgres::Transaction<'_>,
    request: &AtsCertificationPhaseBRequest,
    now_ms: i64,
) -> Result<AtsCertificationPhaseBTransactionOutcome, AtsCertificationAuthorityError> {
    let phase_b_request_sha256 = validate_ats_phase_b_request(request, now_ms)?;
    let row = tx
        .query_opt(
            "SELECT binding_sha256, frozen_certification_base64url, phase, fence,
                    consumed_at_ms
               FROM jobs_application_ats_certification_bindings
              WHERE binding_id = $1 FOR UPDATE",
            &[&request.binding_id],
        )
        .map_err(ats_certification_storage)?
        .ok_or(AtsCertificationAuthorityError::NotFound)?;
    let record = ats_application_binding_record(
        row.get(0),
        row.get(1),
        row.get(2),
        row.get(3),
        row.get(4),
        false,
    )?;
    require_ats_phase_b_binding_match(&record, request, now_ms)?;
    let observed = postgres_ats_phase_b_observed_surface(
        tx,
        &record.authority,
        &request.observed_surface,
        now_ms,
    )?;
    let Some((layout_observation_sha256, surface)) = observed else {
        record_postgres_ats_runtime_layout_drift(
            tx,
            &record.authority,
            &request.observed_surface,
            now_ms,
        )?;
        return Ok(AtsCertificationPhaseBTransactionOutcome::LayoutDriftQuarantined);
    };
    if surface != request.observed_surface {
        record_postgres_ats_runtime_layout_drift(
            tx,
            &record.authority,
            &request.observed_surface,
            now_ms,
        )?;
        return Ok(AtsCertificationPhaseBTransactionOutcome::LayoutDriftQuarantined);
    }
    validate_frozen_ats_certification_postgres_tx(
        tx,
        &record.authority.certification,
        &request.target_evidence.canonical_url,
        &request.runner_id,
        &surface,
        now_ms,
    )?;
    require_postgres_ats_binding_canary_membership(tx, &record, now_ms)?;
    require_postgres_ats_canary_capacity(tx, &record, request, now_ms)?;
    let reservation_sha256 = ats_canary_reservation_sha256(&record, request, now_ms)?;
    insert_postgres_ats_certification_reservation(
        tx,
        &record,
        request,
        &reservation_sha256,
        now_ms,
    )?;
    let changed = tx
        .execute(
            "UPDATE jobs_application_ats_certification_bindings
                SET layout_observation_sha256 = $1, phase_b_request_id = $2,
                    phase_b_request_sha256 = $3, canary_reservation_sha256 = $4,
                    metering_reservation_sha256 = $5, observed_surface_sha256 = $6,
                    phase = $7, fence = 1, consumed_at_ms = $8
              WHERE binding_id = $9 AND phase = 'preflight' AND fence = $10
                AND consumed_at_ms IS NULL AND expires_at_ms > $8",
            &[
                &layout_observation_sha256,
                &request.phase_b_request_id,
                &phase_b_request_sha256,
                &reservation_sha256,
                &request.metering_reservation_sha256,
                &request.observed_surface.surface_sha256,
                &request.terminal_phase,
                &now_ms,
                &request.binding_id,
                &request.expected_fence,
            ],
        )
        .map_err(ats_certification_storage)?;
    if changed != 1 {
        return Err(AtsCertificationAuthorityError::CompareAndSwapConflict);
    }
    let receipt = ats_certified_receipt_authority(
        &record,
        request,
        &phase_b_request_sha256,
        &layout_observation_sha256,
        &reservation_sha256,
        now_ms,
    )?;
    Ok(AtsCertificationPhaseBTransactionOutcome::Authorized(
        Box::new(AtsCertificationPhaseBResult {
            ats_certified_receipt_authority: receipt,
            terminal_phase: request.terminal_phase.clone(),
        }),
    ))
}

fn require_sqlite_ats_canary_capacity(
    tx: &rusqlite::Transaction<'_>,
    record: &AtsApplicationCertificationBindingRecord,
    request: &AtsCertificationPhaseBRequest,
    now_ms: i64,
) -> Result<(), AtsCertificationAuthorityError> {
    let certification = &record.authority.certification;
    if certification.channel != "canary" {
        return Ok(());
    }
    let period_key = ats_certification_canary_utc_period_key(now_ms)?;
    if request.period_key != period_key {
        return Err(AtsCertificationAuthorityError::ScopeMismatch);
    }
    let (total_count, daily_count, live_count, account_count, account_seen): (
        i64,
        i64,
        i64,
        i64,
        i64,
    ) = tx
        .query_row(
            "SELECT
                    COUNT(*),
                    COALESCE(SUM(CASE WHEN reservation.period_key = ?1 THEN 1 ELSE 0 END), 0),
                    COALESCE(SUM(CASE WHEN attempt.status IS NULL OR attempt.status IN (
                                      'reserved', 'running', 'side_effect_unknown'
                                    ) THEN 1 ELSE 0 END), 0),
                    COUNT(DISTINCT reservation.account_id),
                    COALESCE(MAX(CASE WHEN reservation.account_id = ?2 THEN 1 ELSE 0 END), 0)
               FROM jobs_ats_certification_canary_reservations reservation
               LEFT JOIN jobs_attempt_reservations attempt
                 ON attempt.id = reservation.attempt_id
                AND attempt.account_id = reservation.account_id
                AND attempt.application_id = reservation.application_id
              WHERE reservation.activation_sha256 = ?3",
            params![
                period_key,
                request.account_id,
                certification.activation_sha256,
            ],
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
        .map_err(ats_certification_storage)?;
    if total_count >= certification.canary_max_submissions
        || daily_count >= certification.canary_daily_side_effect_cap
        || live_count >= certification.canary_concurrency_cap
        || (account_seen == 0 && account_count >= certification.canary_account_cap)
    {
        return Err(AtsCertificationAuthorityError::CapacityUnavailable);
    }
    Ok(())
}

fn require_postgres_ats_canary_capacity(
    tx: &mut postgres::Transaction<'_>,
    record: &AtsApplicationCertificationBindingRecord,
    request: &AtsCertificationPhaseBRequest,
    now_ms: i64,
) -> Result<(), AtsCertificationAuthorityError> {
    let certification = &record.authority.certification;
    if certification.channel != "canary" {
        return Ok(());
    }
    let period_key = ats_certification_canary_utc_period_key(now_ms)?;
    if request.period_key != period_key {
        return Err(AtsCertificationAuthorityError::ScopeMismatch);
    }
    let row = tx
        .query_one(
            "SELECT
                    COUNT(*),
                    COUNT(*) FILTER (WHERE reservation.period_key = $1),
                    COUNT(*) FILTER (WHERE attempt.status IS NULL OR attempt.status IN (
                                      'reserved', 'running', 'side_effect_unknown'
                                    )),
                    COUNT(DISTINCT reservation.account_id),
                    COALESCE(BOOL_OR(reservation.account_id = $2), FALSE)
               FROM jobs_ats_certification_canary_reservations reservation
               LEFT JOIN jobs_attempt_reservations attempt
                 ON attempt.id = reservation.attempt_id
                AND attempt.account_id = reservation.account_id
                AND attempt.application_id = reservation.application_id
              WHERE reservation.activation_sha256 = $3",
            &[
                &period_key,
                &request.account_id,
                &certification.activation_sha256,
            ],
        )
        .map_err(ats_certification_storage)?;
    let total_count: i64 = row.get(0);
    let daily_count: i64 = row.get(1);
    let live_count: i64 = row.get(2);
    let account_count: i64 = row.get(3);
    let account_seen: bool = row.get(4);
    if total_count >= certification.canary_max_submissions
        || daily_count >= certification.canary_daily_side_effect_cap
        || live_count >= certification.canary_concurrency_cap
        || (!account_seen && account_count >= certification.canary_account_cap)
    {
        return Err(AtsCertificationAuthorityError::CapacityUnavailable);
    }
    Ok(())
}

fn insert_sqlite_ats_certification_reservation(
    tx: &rusqlite::Transaction<'_>,
    record: &AtsApplicationCertificationBindingRecord,
    request: &AtsCertificationPhaseBRequest,
    reservation_sha256: &str,
    now_ms: i64,
) -> Result<(), AtsCertificationAuthorityError> {
    let certification = &record.authority.certification;
    let runtime = certification
        .selected_runtime
        .as_ref()
        .ok_or(AtsCertificationAuthorityError::RuntimeMismatch)?;
    tx.execute(
        "INSERT INTO jobs_ats_certification_canary_reservations (
           reservation_id, reservation_sha256, binding_id, activation_sha256,
           manifest_sha256, target_key, runner_target_sha256, account_id,
           application_id, run_id, attempt_id, period_key, status, fence,
                   reserved_at_ms, consumed_at_ms, released_at_ms,
           metering_reservation_sha256
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12,
                   'reserved', 1, ?13, ?13, NULL, ?14)",
        params![
            request.canary_reservation_id,
            reservation_sha256,
            request.binding_id,
            certification.activation_sha256,
            certification.manifest_sha256,
            certification.target_key,
            runtime.runtime_sha256,
            request.account_id,
            request.application_id,
            request.run_id,
            request.application_attempt_id,
            request.period_key,
            now_ms,
            request.metering_reservation_sha256,
        ],
    )
    .map_err(ats_certification_storage)?;
    Ok(())
}

fn insert_postgres_ats_certification_reservation(
    tx: &mut postgres::Transaction<'_>,
    record: &AtsApplicationCertificationBindingRecord,
    request: &AtsCertificationPhaseBRequest,
    reservation_sha256: &str,
    now_ms: i64,
) -> Result<(), AtsCertificationAuthorityError> {
    let certification = &record.authority.certification;
    let runtime = certification
        .selected_runtime
        .as_ref()
        .ok_or(AtsCertificationAuthorityError::RuntimeMismatch)?;
    tx.execute(
        "INSERT INTO jobs_ats_certification_canary_reservations (
           reservation_id, reservation_sha256, binding_id, activation_sha256,
           manifest_sha256, target_key, runner_target_sha256, account_id,
           application_id, run_id, attempt_id, period_key, status, fence,
           reserved_at_ms, consumed_at_ms, released_at_ms,
           metering_reservation_sha256
         ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12,
                   'reserved', 1, $13, $13, NULL, $14)",
        &[
            &request.canary_reservation_id,
            &reservation_sha256,
            &request.binding_id,
            &certification.activation_sha256,
            &certification.manifest_sha256,
            &certification.target_key,
            &runtime.runtime_sha256,
            &request.account_id,
            &request.application_id,
            &request.run_id,
            &request.application_attempt_id,
            &request.period_key,
            &now_ms,
            &request.metering_reservation_sha256,
        ],
    )
    .map_err(ats_certification_storage)?;
    Ok(())
}

fn validate_ats_recovery_request(
    request: &AtsCertificationRecoveryRequest,
) -> Result<(), AtsCertificationAuthorityError> {
    for value in [
        request.binding_id.as_str(),
        request.account_id.as_str(),
        request.application_id.as_str(),
        request.run_id.as_str(),
        request.application_attempt_id.as_str(),
    ] {
        if !ats_certification_text(value, 1, 240) {
            return Err(AtsCertificationAuthorityError::InvalidAuthority);
        }
    }
    if !ats_certification_hex64(&request.nonce_sha256) {
        return Err(AtsCertificationAuthorityError::InvalidAuthority);
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn ats_recovered_phase_b_result(
    record: AtsApplicationCertificationBindingRecord,
    request: &AtsCertificationRecoveryRequest,
    layout_observation_sha256: String,
    observed_surface_sha256: String,
    phase_b_request_id: String,
    phase_b_request_sha256: String,
    canary_reservation_sha256: String,
    metering_reservation_sha256: String,
) -> Result<AtsCertificationPhaseBResult, AtsCertificationAuthorityError> {
    if record.authority.binding_id != request.binding_id
        || record.authority.account_id != request.account_id
        || record.authority.application_id != request.application_id
        || record.authority.run_id != request.run_id
        || record.authority.application_attempt_id != request.application_attempt_id
        || record.authority.nonce_sha256 != request.nonce_sha256
        || !matches!(record.phase.as_str(), "consumed" | "side_effect_unknown")
        || record.fence != 1
        || record.consumed_at_ms.is_none()
        || !ats_certification_hex64(&layout_observation_sha256)
        || !ats_certification_hex64(&observed_surface_sha256)
        || !ats_certification_text(&phase_b_request_id, 1, 240)
        || !ats_certification_hex64(&phase_b_request_sha256)
        || !ats_certification_hex64(&canary_reservation_sha256)
        || !ats_certification_hex64(&metering_reservation_sha256)
    {
        return Err(AtsCertificationAuthorityError::ScopeMismatch);
    }
    let certification = &record.authority.certification;
    let runtime = certification
        .selected_runtime
        .as_ref()
        .ok_or(AtsCertificationAuthorityError::RuntimeMismatch)?;
    let consumed_at_ms = record
        .consumed_at_ms
        .ok_or(AtsCertificationAuthorityError::ScopeMismatch)?;
    let receipt = AtsCertifiedReceiptAuthority {
        schema_version: 1,
        account_id: record.authority.account_id.clone(),
        application_id: record.authority.application_id.clone(),
        run_id: record.authority.run_id.clone(),
        provider: certification.provider.clone(),
        adapter: certification.provider.clone(),
        adapter_version: certification.adapter_version.clone(),
        manifest_sha256: certification.manifest_sha256.clone(),
        activation_sha256: certification.activation_sha256.clone(),
        activation_generation: certification.activation_generation,
        target_key_sha256: ats_certification_target_key_sha256(&certification.target_key)?,
        layout_set_sha256: certification.layout_set_sha256.clone(),
        layout_observation_sha256,
        observed_surface_sha256,
        adapter_bundle_sha256: certification.adapter_bundle_sha256.clone(),
        runner_kind: runtime.runtime_kind.clone(),
        runner_target_sha256: runtime.runtime_sha256.clone(),
        binding_sha256: record.binding_sha256,
        binding_fence: record.fence,
        binding_consumed_at_ms: consumed_at_ms,
        application_attempt_id: record.authority.application_attempt_id,
        phase_b_request_id,
        rollout_channel: certification.channel.clone(),
        canary_reservation_sha256,
        metering_reservation_sha256,
    };
    Ok(AtsCertificationPhaseBResult {
        ats_certified_receipt_authority: receipt,
        terminal_phase: record.phase,
    })
}

pub fn recover_ats_application_certification(
    pool: &DbPool,
    request: &AtsCertificationRecoveryRequest,
) -> Result<AtsCertificationPhaseBResult, AtsCertificationAuthorityError> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get().map_err(ats_certification_storage)?;
            let tx = conn
                .transaction_with_behavior(TransactionBehavior::Deferred)
                .map_err(ats_certification_storage)?;
            let result = recover_ats_application_certification_sqlite_tx(&tx, request)?;
            tx.commit().map_err(ats_certification_storage)?;
            Ok(result)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg().map_err(ats_certification_storage)?;
            let mut tx = conn
                .build_transaction()
                .isolation_level(postgres::IsolationLevel::RepeatableRead)
                .read_only(true)
                .start()
                .map_err(ats_certification_storage)?;
            let result = recover_ats_application_certification_postgres_tx(&mut tx, request)?;
            tx.commit().map_err(ats_certification_storage)?;
            Ok(result)
        }
    })
}

pub fn recover_ats_application_certification_sqlite_tx(
    tx: &rusqlite::Transaction<'_>,
    request: &AtsCertificationRecoveryRequest,
) -> Result<AtsCertificationPhaseBResult, AtsCertificationAuthorityError> {
    validate_ats_recovery_request(request)?;
    let row = tx
        .query_row(
            "SELECT binding_sha256, frozen_certification_base64url, phase, fence,
                    consumed_at_ms, layout_observation_sha256, observed_surface_sha256,
                    phase_b_request_id, phase_b_request_sha256,
                    canary_reservation_sha256, metering_reservation_sha256
               FROM jobs_application_ats_certification_bindings
              WHERE binding_id = ?1 AND account_id = ?2 AND application_id = ?3
                AND run_id = ?4 AND attempt_id = ?5 AND nonce_sha256 = ?6
                AND phase IN ('consumed', 'side_effect_unknown')",
            params![
                request.binding_id,
                request.account_id,
                request.application_id,
                request.run_id,
                request.application_attempt_id,
                request.nonce_sha256,
            ],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, Option<i64>>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, String>(7)?,
                    row.get::<_, String>(8)?,
                    row.get::<_, String>(9)?,
                    row.get::<_, String>(10)?,
                ))
            },
        )
        .optional()
        .map_err(ats_certification_storage)?
        .ok_or(AtsCertificationAuthorityError::NotFound)?;
    let record = ats_application_binding_record(row.0, row.1, row.2, row.3, row.4, true)?;
    ats_recovered_phase_b_result(record, request, row.5, row.6, row.7, row.8, row.9, row.10)
}

pub fn recover_ats_application_certification_postgres_tx(
    tx: &mut postgres::Transaction<'_>,
    request: &AtsCertificationRecoveryRequest,
) -> Result<AtsCertificationPhaseBResult, AtsCertificationAuthorityError> {
    validate_ats_recovery_request(request)?;
    let row = tx
        .query_opt(
            "SELECT binding_sha256, frozen_certification_base64url, phase, fence,
                    consumed_at_ms, layout_observation_sha256, observed_surface_sha256,
                    phase_b_request_id, phase_b_request_sha256,
                    canary_reservation_sha256, metering_reservation_sha256
               FROM jobs_application_ats_certification_bindings
              WHERE binding_id = $1 AND account_id = $2 AND application_id = $3
                AND run_id = $4 AND attempt_id = $5 AND nonce_sha256 = $6
                AND phase IN ('consumed', 'side_effect_unknown')",
            &[
                &request.binding_id,
                &request.account_id,
                &request.application_id,
                &request.run_id,
                &request.application_attempt_id,
                &request.nonce_sha256,
            ],
        )
        .map_err(ats_certification_storage)?
        .ok_or(AtsCertificationAuthorityError::NotFound)?;
    let record = ats_application_binding_record(
        row.get(0),
        row.get(1),
        row.get(2),
        row.get(3),
        row.get(4),
        true,
    )?;
    ats_recovered_phase_b_result(
        record,
        request,
        row.get(5),
        row.get(6),
        row.get(7),
        row.get(8),
        row.get(9),
        row.get(10),
    )
}

pub fn lookup_ats_certification_terminal_receipt_authority(
    pool: &DbPool,
    account_id: &str,
    application_id: &str,
    run_id: &str,
    presented: &AtsCertifiedReceiptAuthority,
) -> Result<AtsCertifiedReceiptAuthority, AtsCertificationAuthorityError> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get().map_err(ats_certification_storage)?;
            let tx = conn
                .transaction_with_behavior(TransactionBehavior::Deferred)
                .map_err(ats_certification_storage)?;
            let result = lookup_ats_certification_terminal_receipt_authority_sqlite_tx(
                &tx,
                account_id,
                application_id,
                run_id,
                presented,
            )?;
            tx.commit().map_err(ats_certification_storage)?;
            Ok(result)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg().map_err(ats_certification_storage)?;
            let mut tx = conn
                .build_transaction()
                .isolation_level(postgres::IsolationLevel::RepeatableRead)
                .read_only(true)
                .start()
                .map_err(ats_certification_storage)?;
            let result = lookup_ats_certification_terminal_receipt_authority_postgres_tx(
                &mut tx,
                account_id,
                application_id,
                run_id,
                presented,
            )?;
            tx.commit().map_err(ats_certification_storage)?;
            Ok(result)
        }
    })
}

pub fn lookup_ats_certification_terminal_receipt_authority_sqlite_tx(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    application_id: &str,
    run_id: &str,
    presented: &AtsCertifiedReceiptAuthority,
) -> Result<AtsCertifiedReceiptAuthority, AtsCertificationAuthorityError> {
    for value in [account_id, application_id, run_id] {
        if !ats_certification_text(value, 1, 240) {
            return Err(AtsCertificationAuthorityError::InvalidAuthority);
        }
    }
    let mut stmt = tx
        .prepare(
            "SELECT binding_sha256, frozen_certification_base64url, phase, fence,
                    consumed_at_ms, layout_observation_sha256, observed_surface_sha256,
                    phase_b_request_id, phase_b_request_sha256,
                    canary_reservation_sha256, metering_reservation_sha256
               FROM jobs_application_ats_certification_bindings
              WHERE account_id = ?1 AND application_id = ?2 AND run_id = ?3
                AND phase IN ('consumed', 'side_effect_unknown') LIMIT 2",
        )
        .map_err(ats_certification_storage)?;
    let rows = stmt
        .query_map(params![account_id, application_id, run_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, Option<i64>>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, String>(8)?,
                row.get::<_, String>(9)?,
                row.get::<_, String>(10)?,
            ))
        })
        .map_err(ats_certification_storage)?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(ats_certification_storage)?;
    if rows.len() != 1 {
        return Err(if rows.is_empty() {
            AtsCertificationAuthorityError::NotFound
        } else {
            AtsCertificationAuthorityError::IdentityConflict
        });
    }
    let row = rows
        .into_iter()
        .next()
        .ok_or(AtsCertificationAuthorityError::NotFound)?;
    let record = ats_application_binding_record(row.0, row.1, row.2, row.3, row.4, true)?;
    let recovery = AtsCertificationRecoveryRequest {
        binding_id: record.authority.binding_id.clone(),
        account_id: account_id.to_string(),
        application_id: application_id.to_string(),
        run_id: run_id.to_string(),
        application_attempt_id: record.authority.application_attempt_id.clone(),
        nonce_sha256: record.authority.nonce_sha256.clone(),
    };
    let result =
        ats_recovered_phase_b_result(record, &recovery, row.5, row.6, row.7, row.8, row.9, row.10)?;
    if result.ats_certified_receipt_authority != *presented {
        return Err(AtsCertificationAuthorityError::ScopeMismatch);
    }
    Ok(result.ats_certified_receipt_authority)
}

pub fn lookup_ats_certification_terminal_receipt_authority_postgres_tx(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    application_id: &str,
    run_id: &str,
    presented: &AtsCertifiedReceiptAuthority,
) -> Result<AtsCertifiedReceiptAuthority, AtsCertificationAuthorityError> {
    for value in [account_id, application_id, run_id] {
        if !ats_certification_text(value, 1, 240) {
            return Err(AtsCertificationAuthorityError::InvalidAuthority);
        }
    }
    let rows = tx
        .query(
            "SELECT binding_sha256, frozen_certification_base64url, phase, fence,
                    consumed_at_ms, layout_observation_sha256, observed_surface_sha256,
                    phase_b_request_id, phase_b_request_sha256,
                    canary_reservation_sha256, metering_reservation_sha256
               FROM jobs_application_ats_certification_bindings
              WHERE account_id = $1 AND application_id = $2 AND run_id = $3
                AND phase IN ('consumed', 'side_effect_unknown') LIMIT 2",
            &[&account_id, &application_id, &run_id],
        )
        .map_err(ats_certification_storage)?;
    if rows.len() != 1 {
        return Err(if rows.is_empty() {
            AtsCertificationAuthorityError::NotFound
        } else {
            AtsCertificationAuthorityError::IdentityConflict
        });
    }
    let row = &rows[0];
    let record = ats_application_binding_record(
        row.get(0),
        row.get(1),
        row.get(2),
        row.get(3),
        row.get(4),
        true,
    )?;
    let recovery = AtsCertificationRecoveryRequest {
        binding_id: record.authority.binding_id.clone(),
        account_id: account_id.to_string(),
        application_id: application_id.to_string(),
        run_id: run_id.to_string(),
        application_attempt_id: record.authority.application_attempt_id.clone(),
        nonce_sha256: record.authority.nonce_sha256.clone(),
    };
    let result = ats_recovered_phase_b_result(
        record,
        &recovery,
        row.get(5),
        row.get(6),
        row.get(7),
        row.get(8),
        row.get(9),
        row.get(10),
    )?;
    if result.ats_certified_receipt_authority != *presented {
        return Err(AtsCertificationAuthorityError::ScopeMismatch);
    }
    Ok(result.ats_certified_receipt_authority)
}

fn validate_ats_binding_invalidation_request(
    request: &AtsCertificationBindingInvalidationRequest,
    now_ms: i64,
) -> Result<(), AtsCertificationAuthorityError> {
    for value in [
        request.binding_id.as_str(),
        request.account_id.as_str(),
        request.application_id.as_str(),
        request.run_id.as_str(),
        request.application_attempt_id.as_str(),
    ] {
        if !ats_certification_text(value, 1, 240) {
            return Err(AtsCertificationAuthorityError::InvalidAuthority);
        }
    }
    if !ats_certification_hex64(&request.nonce_sha256)
        || request.expected_fence != 0
        || !matches!(
            request.invalidation_kind.as_str(),
            "auto_authorization_changed"
                | "claim_lost"
                | "expired"
                | "packet_changed"
                | "run_cancelled"
        )
        || !ats_certification_safe_integer(now_ms, false)
    {
        return Err(AtsCertificationAuthorityError::InvalidAuthority);
    }
    Ok(())
}

fn require_ats_invalidation_scope(
    record: &AtsApplicationCertificationBindingRecord,
    request: &AtsCertificationBindingInvalidationRequest,
) -> Result<(), AtsCertificationAuthorityError> {
    let authority = &record.authority;
    if authority.binding_id != request.binding_id
        || authority.account_id != request.account_id
        || authority.application_id != request.application_id
        || authority.run_id != request.run_id
        || authority.application_attempt_id != request.application_attempt_id
        || authority.nonce_sha256 != request.nonce_sha256
    {
        return Err(AtsCertificationAuthorityError::ScopeMismatch);
    }
    Ok(())
}

pub fn invalidate_ats_application_certification_binding(
    pool: &DbPool,
    request: &AtsCertificationBindingInvalidationRequest,
    now_ms: i64,
) -> Result<AtsApplicationCertificationBindingRecord, AtsCertificationAuthorityError> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get().map_err(ats_certification_storage)?;
            let tx = conn
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(ats_certification_storage)?;
            let result =
                invalidate_ats_application_certification_binding_sqlite_tx(&tx, request, now_ms)?;
            tx.commit().map_err(ats_certification_storage)?;
            Ok(result)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg().map_err(ats_certification_storage)?;
            let mut tx = conn
                .build_transaction()
                .isolation_level(postgres::IsolationLevel::Serializable)
                .start()
                .map_err(ats_certification_storage)?;
            let result = invalidate_ats_application_certification_binding_postgres_tx(
                &mut tx, request, now_ms,
            )?;
            tx.commit().map_err(ats_certification_storage)?;
            Ok(result)
        }
    })
}

pub fn invalidate_ats_application_certification_binding_sqlite_tx(
    tx: &rusqlite::Transaction<'_>,
    request: &AtsCertificationBindingInvalidationRequest,
    now_ms: i64,
) -> Result<AtsApplicationCertificationBindingRecord, AtsCertificationAuthorityError> {
    validate_ats_binding_invalidation_request(request, now_ms)?;
    let row = tx
        .query_row(
            "SELECT binding_sha256, frozen_certification_base64url, phase, fence,
                    consumed_at_ms, invalidation_kind
               FROM jobs_application_ats_certification_bindings
              WHERE binding_id = ?1",
            params![request.binding_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, Option<i64>>(4)?,
                    row.get::<_, Option<String>>(5)?,
                ))
            },
        )
        .optional()
        .map_err(ats_certification_storage)?
        .ok_or(AtsCertificationAuthorityError::NotFound)?;
    let mut record = ats_application_binding_record(row.0, row.1, row.2, row.3, row.4, false)?;
    require_ats_invalidation_scope(&record, request)?;
    if record.phase == "invalidated"
        && record.fence == 1
        && row.5.as_deref() == Some(request.invalidation_kind.as_str())
    {
        record.replayed = true;
        return Ok(record);
    }
    if record.phase != "preflight" || record.fence != request.expected_fence {
        return Err(AtsCertificationAuthorityError::CompareAndSwapConflict);
    }
    let changed = tx
        .execute(
            "UPDATE jobs_application_ats_certification_bindings
                SET phase = 'invalidated', fence = 1, invalidation_kind = ?1,
                    invalidated_at_ms = ?2
              WHERE binding_id = ?3 AND phase = 'preflight' AND fence = ?4",
            params![
                request.invalidation_kind,
                now_ms,
                request.binding_id,
                request.expected_fence,
            ],
        )
        .map_err(ats_certification_storage)?;
    if changed != 1 {
        return Err(AtsCertificationAuthorityError::CompareAndSwapConflict);
    }
    record.phase = "invalidated".to_string();
    record.fence = 1;
    Ok(record)
}

pub fn invalidate_ats_application_certification_binding_postgres_tx(
    tx: &mut postgres::Transaction<'_>,
    request: &AtsCertificationBindingInvalidationRequest,
    now_ms: i64,
) -> Result<AtsApplicationCertificationBindingRecord, AtsCertificationAuthorityError> {
    validate_ats_binding_invalidation_request(request, now_ms)?;
    let row = tx
        .query_opt(
            "SELECT binding_sha256, frozen_certification_base64url, phase, fence,
                    consumed_at_ms, invalidation_kind
               FROM jobs_application_ats_certification_bindings
              WHERE binding_id = $1 FOR UPDATE",
            &[&request.binding_id],
        )
        .map_err(ats_certification_storage)?
        .ok_or(AtsCertificationAuthorityError::NotFound)?;
    let mut record = ats_application_binding_record(
        row.get(0),
        row.get(1),
        row.get(2),
        row.get(3),
        row.get(4),
        false,
    )?;
    require_ats_invalidation_scope(&record, request)?;
    if record.phase == "invalidated"
        && record.fence == 1
        && row.get::<_, Option<String>>(5).as_deref() == Some(request.invalidation_kind.as_str())
    {
        record.replayed = true;
        return Ok(record);
    }
    if record.phase != "preflight" || record.fence != request.expected_fence {
        return Err(AtsCertificationAuthorityError::CompareAndSwapConflict);
    }
    let changed = tx
        .execute(
            "UPDATE jobs_application_ats_certification_bindings
                SET phase = 'invalidated', fence = 1, invalidation_kind = $1,
                    invalidated_at_ms = $2
              WHERE binding_id = $3 AND phase = 'preflight' AND fence = $4",
            &[
                &request.invalidation_kind,
                &now_ms,
                &request.binding_id,
                &request.expected_fence,
            ],
        )
        .map_err(ats_certification_storage)?;
    if changed != 1 {
        return Err(AtsCertificationAuthorityError::CompareAndSwapConflict);
    }
    record.phase = "invalidated".to_string();
    record.fence = 1;
    Ok(record)
}

pub fn validate_frozen_ats_certification(
    pool: &DbPool,
    binding: &AtsCertificationBinding,
    canonical_url: &str,
    runner: &str,
    observed_surface: &AtsObservedSurface,
    now_ms: i64,
) -> Result<(), AtsCertificationAuthorityError> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get().map_err(ats_certification_storage)?;
            let tx = conn
                .transaction_with_behavior(TransactionBehavior::Deferred)
                .map_err(ats_certification_storage)?;
            validate_frozen_ats_certification_sqlite_tx(
                &tx,
                binding,
                canonical_url,
                runner,
                observed_surface,
                now_ms,
            )?;
            tx.commit().map_err(ats_certification_storage)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg().map_err(ats_certification_storage)?;
            let mut tx = conn.transaction().map_err(ats_certification_storage)?;
            validate_frozen_ats_certification_postgres_tx(
                &mut tx,
                binding,
                canonical_url,
                runner,
                observed_surface,
                now_ms,
            )?;
            tx.commit().map_err(ats_certification_storage)
        }
    })
}

pub fn validate_frozen_ats_certification_sqlite_tx(
    tx: &rusqlite::Transaction<'_>,
    binding: &AtsCertificationBinding,
    canonical_url: &str,
    runner: &str,
    observed_surface: &AtsObservedSurface,
    now_ms: i64,
) -> Result<(), AtsCertificationAuthorityError> {
    validate_ats_frozen_request(binding, canonical_url, runner, observed_surface, now_ms)?;
    let resolved = resolve_sqlite_ats_certification_tx(
        tx,
        &binding.provider,
        &binding.target_key,
        &binding.scope_sha256,
        observed_surface,
        &binding.channel,
        binding.account_allowlist_sha256.as_deref(),
        Some(runner),
        now_ms,
    )?
    .ok_or(AtsCertificationAuthorityError::ScopeMismatch)?;
    if &resolved != binding {
        return Err(AtsCertificationAuthorityError::ScopeMismatch);
    }
    Ok(())
}

pub fn validate_frozen_ats_certification_postgres_tx(
    tx: &mut postgres::Transaction<'_>,
    binding: &AtsCertificationBinding,
    canonical_url: &str,
    runner: &str,
    observed_surface: &AtsObservedSurface,
    now_ms: i64,
) -> Result<(), AtsCertificationAuthorityError> {
    validate_ats_frozen_request(binding, canonical_url, runner, observed_surface, now_ms)?;
    let resolved = resolve_postgres_ats_certification_tx(
        tx,
        &binding.provider,
        &binding.target_key,
        &binding.scope_sha256,
        observed_surface,
        &binding.channel,
        binding.account_allowlist_sha256.as_deref(),
        Some(runner),
        now_ms,
    )?
    .ok_or(AtsCertificationAuthorityError::ScopeMismatch)?;
    if &resolved != binding {
        return Err(AtsCertificationAuthorityError::ScopeMismatch);
    }
    Ok(())
}

fn validate_ats_frozen_request(
    binding: &AtsCertificationBinding,
    canonical_url: &str,
    runner: &str,
    observed_surface: &AtsObservedSurface,
    now_ms: i64,
) -> Result<(), AtsCertificationAuthorityError> {
    validate_ats_observed_surface(observed_surface)?;
    let (provider, target_key) = ats_certification_target_from_url(canonical_url)?;
    let scope_sha256 = ats_certification_scope_sha256(
        &provider,
        &target_key,
        &observed_surface.variant_key,
        &observed_surface.surface_sha256,
    )?;
    if binding.binding_version != 1
        || !ats_certification_hex64(&binding.trust_policy_sha256)
        || !ats_certification_binding_matches_url(binding, canonical_url)
        || binding.provider != provider
        || binding.target_key != target_key
        || binding.variant_key != observed_surface.variant_key
        || binding.surface_sha256 != observed_surface.surface_sha256
        || binding.layout_contract_version != observed_surface.layout_contract_version
        || binding.scope_sha256 != scope_sha256
        || now_ms < binding.not_before_ms
        || now_ms >= binding.expires_at_ms
        || binding
            .selected_runtime
            .as_ref()
            .is_none_or(|runtime| runtime.runtime_sha256 != runner)
        || !binding
            .runtime_targets
            .iter()
            .any(|runtime| Some(runtime) == binding.selected_runtime.as_ref())
        || (binding.channel != "shadow"
            && (!ats_exact_submit_adapter(&binding.provider, &binding.adapter_version)
                || !ats_exact_provider_target_key(&binding.provider, &binding.target_key)))
    {
        return Err(AtsCertificationAuthorityError::ScopeMismatch);
    }
    Ok(())
}

#[cfg(any(test, feature = "integration-test-support"))]
#[cfg_attr(feature = "integration-test-support", allow(dead_code))]
pub(super) mod ats_certification_authority_tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    const TEST_NOW_MS: i64 = 2_000_000_000_000;

    #[test]
    fn fix_728_ats_after_prelock_phase_helpers_never_reacquire_ats() {
        let source = include_str!("ats_certification_authority.rs");
        for (legacy_start, after_prelock_start, end) in [
            (
                "pub fn create_ats_application_certification_binding_from_context_postgres_tx(",
                "pub(crate) fn create_ats_application_certification_binding_from_context_postgres_tx_after_prelock(",
                "pub fn create_ats_application_certification_binding(",
            ),
            (
                "pub fn create_ats_application_certification_binding_postgres_tx(",
                "pub(crate) fn create_ats_application_certification_binding_postgres_tx_after_prelock(",
                "fn validate_ats_phase_b_request(",
            ),
            (
                "pub fn validate_consume_reserve_ats_application_certification_from_context_postgres_tx(",
                "pub(crate) fn validate_consume_reserve_ats_application_certification_from_context_postgres_tx_after_prelock(",
                "pub fn validate_consume_reserve_ats_application_certification(",
            ),
            (
                "pub fn validate_consume_reserve_ats_application_certification_postgres_tx(",
                "pub(crate) fn validate_consume_reserve_ats_application_certification_postgres_tx_after_prelock(",
                "fn require_sqlite_ats_canary_capacity(",
            ),
        ] {
            let legacy = source
                .split(legacy_start)
                .nth(1)
                .unwrap_or_else(|| panic!("missing legacy ATS helper {legacy_start}"))
                .split(after_prelock_start)
                .next()
                .expect("bounded legacy ATS helper");
            assert!(legacy.contains("lock_postgres_ats_certification(tx)"));

            let after_prelock = source
                .split(after_prelock_start)
                .nth(1)
                .unwrap_or_else(|| panic!("missing after-prelock ATS helper {after_prelock_start}"))
                .split(end)
                .next()
                .expect("bounded after-prelock ATS helper");
            assert!(!after_prelock.contains("lock_postgres_ats_certification"));
        }
    }

    struct TestAuthority {
        anchor: AtsCertificationTrustAnchor,
        root_anchor: AtsCertificationRootTrustAnchor,
        keys: BTreeMap<String, SigningKey>,
        key_ids: BTreeMap<String, String>,
    }

    fn test_pool() -> DbPool {
        let path = std::env::temp_dir().join(format!(
            "bluey-ats-certification-{}-{}.sqlite3",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let pool = crate::db::open_pool(&path).unwrap();
        crate::db::run_migrations(&pool).unwrap();
        pool
    }

    fn postgres_test_pool() -> Option<DbPool> {
        let database_url = std::env::var("BLUEY_TEST_POSTGRES_URL").ok()?;
        let pool = crate::db::open_postgres_pool(&database_url).unwrap();
        crate::db::run_migrations(&pool).unwrap();
        Some(pool)
    }

    fn postgres_test_pool_or_skip(test_name: &str) -> Option<DbPool> {
        let pool = postgres_test_pool();
        if pool.is_none() {
            eprintln!("skipped {test_name}: BLUEY_TEST_POSTGRES_URL is not set");
        }
        pool
    }

    fn next_postgres_revocation_generation(
        pool: &DbPool,
        policy_sha256: &str,
    ) -> (i64, Option<String>) {
        let mut conn = pool.get_pg().unwrap();
        conn.query_opt(
            "SELECT revocation_generation, revocation_sha256
               FROM jobs_ats_certification_revocations
              WHERE trust_policy_sha256 = $1
              ORDER BY revocation_generation DESC LIMIT 1",
            &[&policy_sha256],
        )
        .unwrap()
        .map_or((1, None), |row| {
            (row.get::<_, i64>(0) + 1, Some(row.get::<_, String>(1)))
        })
    }

    fn signing_key(index: usize) -> SigningKey {
        let seed = std::array::from_fn(|offset| ((index * 41 + offset + 7) % 256) as u8);
        SigningKey::from_bytes(&seed)
    }

    fn test_authority() -> TestAuthority {
        let mut roles = BTreeMap::new();
        let mut keys = BTreeMap::new();
        let mut key_ids = BTreeMap::new();
        for (index, role) in [
            "manifest",
            "evidence",
            "revocation",
            "layout_observation",
            "activation",
        ]
        .into_iter()
        .enumerate()
        {
            let key = signing_key(index);
            let key_id = format!("{role}-key-v1");
            roles.insert(
                role.to_string(),
                AtsCertificationTrustRole {
                    threshold: 1,
                    keys: BTreeMap::from([(
                        key_id.clone(),
                        base64::engine::general_purpose::URL_SAFE_NO_PAD
                            .encode(key.verifying_key().as_bytes()),
                    )]),
                },
            );
            keys.insert(role.to_string(), key);
            key_ids.insert(role.to_string(), key_id);
        }
        let root_key = signing_key(10);
        let root_anchor = AtsCertificationRootTrustAnchor {
            threshold: 1,
            keys: BTreeMap::from([(
                "root-key-v1".to_string(),
                base64::engine::general_purpose::URL_SAFE_NO_PAD
                    .encode(root_key.verifying_key().as_bytes()),
            )]),
        };
        keys.insert("root".to_string(), root_key);
        key_ids.insert("root".to_string(), "root-key-v1".to_string());
        TestAuthority {
            anchor: AtsCertificationTrustAnchor::new(roles).unwrap(),
            root_anchor,
            keys,
            key_ids,
        }
    }

    fn rotated_test_authority(root: &TestAuthority) -> TestAuthority {
        let mut roles = BTreeMap::new();
        let mut keys = BTreeMap::from([("root".to_string(), root.keys["root"].clone())]);
        let mut key_ids = BTreeMap::from([("root".to_string(), root.key_ids["root"].clone())]);
        for (index, role) in [
            "manifest",
            "evidence",
            "revocation",
            "layout_observation",
            "activation",
        ]
        .into_iter()
        .enumerate()
        {
            let key = signing_key(index + 20);
            let key_id = format!("{role}-key-v2");
            roles.insert(
                role.to_string(),
                AtsCertificationTrustRole {
                    threshold: 1,
                    keys: BTreeMap::from([(
                        key_id.clone(),
                        base64::engine::general_purpose::URL_SAFE_NO_PAD
                            .encode(key.verifying_key().as_bytes()),
                    )]),
                },
            );
            keys.insert(role.to_string(), key);
            key_ids.insert(role.to_string(), key_id);
        }
        TestAuthority {
            anchor: AtsCertificationTrustAnchor::new(roles).unwrap(),
            root_anchor: root.root_anchor.clone(),
            keys,
            key_ids,
        }
    }

    fn test_trust_policy(
        authority: &TestAuthority,
        generation: i64,
        predecessor_policy_sha256: Option<String>,
    ) -> AtsCertificationTrustPolicyAuthority {
        AtsCertificationTrustPolicyAuthority {
            version: 1,
            audience: ATS_CERTIFICATION_TRUST_POLICY_AUDIENCE.to_string(),
            policy_id: format!("ats-trust-policy-{generation}"),
            trust_generation: generation,
            predecessor_policy_sha256,
            delegated_trust: authority.anchor.clone(),
            certification_requirements: AtsCertificationPolicyRequirements {
                allowed_providers: vec!["greenhouse".to_string(), "lever".to_string()],
                suite_id: "bluey-ats-complete-v1".to_string(),
                required_check_ids: ATS_CERTIFICATION_REQUIRED_CHECK_IDS
                    .map(str::to_string)
                    .to_vec(),
                required_evidence_classes: vec![
                    "authorized_live".to_string(),
                    "authorized_sandbox".to_string(),
                ],
                maximum_manifest_lifetime_ms: 10_000_000,
                maximum_clock_skew_ms: 60_000,
                maximum_manifest_size_bytes: ATS_CERTIFICATION_MAX_CANONICAL_BYTES as i64,
                maximum_target_count: 8,
                maximum_observation_count: 16,
                maximum_evidence_object_count: 32,
            },
            issued_at_ms: TEST_NOW_MS - 4_000,
            valid_from_ms: TEST_NOW_MS - 3_000,
            expires_at_ms: TEST_NOW_MS + 2_000_000,
        }
    }

    fn initialize_test_trust_policy(pool: &DbPool, authority: &TestAuthority) -> String {
        let policy = test_trust_policy(authority, 1, None);
        let policy_envelope = envelope(
            &policy,
            "root",
            ATS_CERTIFICATION_TRUST_POLICY_AUDIENCE,
            policy.issued_at_ms,
            authority,
            "authorize-trust-policy-1",
        );
        let policy_sha256 = envelope_sha256(&policy_envelope);
        import_ats_certification_trust_policy_with_root_at(
            pool,
            &policy_envelope,
            &authority.root_anchor,
            "root-operator",
            TEST_NOW_MS,
        )
        .unwrap();
        policy_sha256
    }

    fn envelope<T: Serialize>(
        authority: &T,
        role: &str,
        target_audience: &str,
        issued_at_ms: i64,
        test_authority: &TestAuthority,
        authorization_id: &str,
    ) -> AtsCertificationAuthorityEnvelope {
        let canonical = ats_certification_canonical_json(authority).unwrap();
        let target_sha256 = ats_certification_sha256(&canonical);
        let key_id = test_authority.key_ids[role].clone();
        let payload = AtsCertificationAuthorizationPayload {
            version: 1,
            audience: ATS_CERTIFICATION_AUTHORIZATION_AUDIENCE,
            authorization_id,
            role,
            target_audience,
            target_sha256: &target_sha256,
            signed_at_ms: issued_at_ms,
        };
        let payload = ats_certification_canonical_json(&payload).unwrap();
        let signature = test_authority.keys[role].sign(&payload);
        let authorization = AtsCertificationAuthorization {
            version: 1,
            audience: ATS_CERTIFICATION_AUTHORIZATION_AUDIENCE.to_string(),
            authorization_id: authorization_id.to_string(),
            role: role.to_string(),
            target_audience: target_audience.to_string(),
            target_sha256,
            signed_at_ms: issued_at_ms,
            signatures: vec![AtsCertificationDetachedSignature {
                key_id,
                signature: base64::engine::general_purpose::URL_SAFE_NO_PAD
                    .encode(signature.to_bytes()),
            }],
        };
        AtsCertificationAuthorityEnvelope {
            canonical_base64url: base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(canonical),
            authorization_base64url: base64::engine::general_purpose::URL_SAFE_NO_PAD
                .encode(ats_certification_canonical_json(&authorization).unwrap()),
        }
    }

    fn envelope_sha256(envelope: &AtsCertificationAuthorityEnvelope) -> String {
        ats_certification_sha256(
            &base64::engine::general_purpose::URL_SAFE_NO_PAD
                .decode(&envelope.canonical_base64url)
                .unwrap(),
        )
    }

    fn test_surface() -> AtsObservedSurface {
        AtsObservedSurface {
            variant_key: "greenhouse_public".to_string(),
            layout_contract_version: 1,
            surface_sha256: "a".repeat(64),
        }
    }

    fn test_target_evidence() -> AtsCertificationFreshTargetEvidence {
        AtsCertificationFreshTargetEvidence {
            canonical_url: "https://boards.greenhouse.io/acme/jobs/123".to_string(),
            discovery_provider: "greenhouse".to_string(),
            discovery_target_key: "greenhouse:acme:123".to_string(),
            discovery_observed_at_ms: TEST_NOW_MS - 100,
            original_source_provider: "greenhouse".to_string(),
            original_source_target_key: "greenhouse:acme:123".to_string(),
            original_source_observed_at_ms: TEST_NOW_MS - 50,
        }
    }

    fn test_posting() -> JobPosting {
        JobPosting {
            id: "posting-1".to_string(),
            canonical_key: "canonical-job-1".to_string(),
            source: "greenhouse".to_string(),
            external_id: "123".to_string(),
            company: "Acme".to_string(),
            title: "Engineer".to_string(),
            location: "Remote".to_string(),
            workplace: "remote".to_string(),
            canonical_url: "https://boards.greenhouse.io/acme/jobs/123".to_string(),
            description: "Build reliable systems.".to_string(),
            compensation: String::new(),
            employment_type: "full_time".to_string(),
            track_id: "track-1".to_string(),
            match_score: 90,
            matched_reasons: Vec::new(),
            missing_requirements: Vec::new(),
            posted_at_ms: Some(TEST_NOW_MS - 10_000),
            last_verified_at_ms: Some(TEST_NOW_MS - 100),
            availability_status: "active".to_string(),
            status: "new".to_string(),
            created_at_ms: TEST_NOW_MS - 20_000,
            updated_at_ms: TEST_NOW_MS - 100,
            discovery_evidence: JobDiscoveryEvidence {
                provenance: "original_source".to_string(),
                canonical_status: "canonical".to_string(),
                canonical_job_id: Some("canonical-job-1".to_string()),
                employer_verification_status: "ats_tenant_verified".to_string(),
                employer_id: Some("employer-1".to_string()),
                canonical_employer_domain: Some("acme.example".to_string()),
                application_domain: Some("boards.greenhouse.io".to_string()),
                scam_risk_status: "source_screened".to_string(),
                scam_signals: Vec::new(),
                original_source_status: "verified_open".to_string(),
                original_source_checked_at_ms: Some(TEST_NOW_MS - 50),
                original_source_snapshot_expires_at_ms: Some(TEST_NOW_MS + 10_000),
                original_source_evidence_hash: Some("a".repeat(64)),
                original_source_mismatched_fields: Vec::new(),
                requires_original_revalidation: false,
            },
            eligibility: None,
        }
    }

    fn local_runtime_attestation() -> AtsCertificationRuntimeAttestation {
        AtsCertificationRuntimeAttestation::Local {
            platform: "macos".to_string(),
            architecture: "arm64".to_string(),
            browser_release_manifest_sha256: "3".repeat(64),
            browser_artifact_sha256: "4".repeat(64),
            browser_build_descriptor_sha256: "5".repeat(64),
            automation_bundle_sha256: "2".repeat(64),
            playwright_version: "1.55.0".to_string(),
            chromium_revision: "1187".to_string(),
            chromium_executable_sha256: "6".repeat(64),
        }
    }

    fn cloud_runtime_attestation() -> AtsCertificationRuntimeAttestation {
        AtsCertificationRuntimeAttestation::Cloud {
            platform: "linux".to_string(),
            architecture: "x86_64".to_string(),
            runner_build_id: "runner-604.1".to_string(),
            runner_image_sha256: "3".repeat(64),
            automation_bundle_sha256: "2".repeat(64),
            playwright_version: "1.55.0".to_string(),
            chromium_revision: "1187".to_string(),
            chromium_executable_sha256: "6".repeat(64),
        }
    }

    fn test_binding_request(
        binding_id: &str,
        nonce_byte: char,
    ) -> AtsApplicationCertificationBindingRequest {
        AtsApplicationCertificationBindingRequest {
            binding_id: binding_id.to_string(),
            account_id: "account-1".to_string(),
            application_id: "application-1".to_string(),
            run_id: format!("run-{binding_id}"),
            application_attempt_id: format!("attempt-{binding_id}"),
            browser_session_id: "browser-session-1".to_string(),
            browser_profile_id: "browser-profile-1".to_string(),
            packet_checksum_sha256: "7".repeat(64),
            auto_authorization_id: "auto-authorization-1".to_string(),
            auto_authorization_revision: 1,
            auto_authorization_fingerprint_sha256: "8".repeat(64),
            target_evidence: test_target_evidence(),
            rollout_channel: "general".to_string(),
            runner_id: "1".repeat(64),
            nonce_sha256: nonce_byte.to_string().repeat(64),
            requested_expires_at_ms: TEST_NOW_MS + 100_000,
        }
    }

    fn test_phase_b_request(
        binding: &AtsApplicationCertificationBindingRequest,
        request_id: &str,
    ) -> AtsCertificationPhaseBRequest {
        AtsCertificationPhaseBRequest {
            binding_id: binding.binding_id.clone(),
            account_id: binding.account_id.clone(),
            application_id: binding.application_id.clone(),
            run_id: binding.run_id.clone(),
            application_attempt_id: binding.application_attempt_id.clone(),
            packet_checksum_sha256: binding.packet_checksum_sha256.clone(),
            auto_authorization_id: binding.auto_authorization_id.clone(),
            auto_authorization_revision: binding.auto_authorization_revision,
            auto_authorization_fingerprint_sha256: binding
                .auto_authorization_fingerprint_sha256
                .clone(),
            target_evidence: binding.target_evidence.clone(),
            runner_id: binding.runner_id.clone(),
            nonce_sha256: binding.nonce_sha256.clone(),
            observed_surface: test_surface(),
            phase_b_request_id: request_id.to_string(),
            metering_reservation_sha256: ats_certification_sha256(
                format!("metering-{request_id}").as_bytes(),
            ),
            canary_reservation_id: format!("reservation-{request_id}"),
            period_key: "2033-05-18".to_string(),
            expected_fence: 0,
            terminal_phase: "consumed".to_string(),
        }
    }

    fn test_canary_binding_request(
        binding_id: &str,
        account_id: &str,
        application_id: &str,
        nonce_byte: char,
    ) -> AtsApplicationCertificationBindingRequest {
        let mut request = test_binding_request(binding_id, nonce_byte);
        request.account_id = account_id.to_string();
        request.application_id = application_id.to_string();
        request.run_id = format!("run-{binding_id}");
        request.application_attempt_id = format!("attempt-{binding_id}");
        request.browser_session_id = format!("session-{binding_id}");
        request.browser_profile_id = format!("profile-{binding_id}");
        request.rollout_channel = "canary".to_string();
        request
    }

    fn imported_canary_fixture(
        max_submissions: i64,
        account_cap: i64,
        concurrency_cap: i64,
        daily_side_effect_cap: i64,
    ) -> ImportedFixture {
        let mut fixture = imported_fixture();
        let allowlist = import_ats_certification_canary_allowlist(
            &fixture.pool,
            &AtsCertificationCanaryAllowlistImportRequest {
                schema_version: 1,
                allowlist_id: format!(
                    "capacity-{max_submissions}-{account_cap}-{concurrency_cap}-{daily_side_effect_cap}"
                ),
                account_ids: vec!["account-1".to_string(), "account-2".to_string()],
                approval_ref: "round-604-capacity-matrix".to_string(),
                not_before_ms: TEST_NOW_MS - 700,
                expires_at_ms: TEST_NOW_MS + 900_000,
            },
            "capacity-operator",
            TEST_NOW_MS,
        )
        .unwrap();
        let mut activation = test_activation(
            fixture.manifest_sha256.clone(),
            fixture.manifest.scope_sha256.clone(),
            &fixture.policy_sha256,
        );
        activation.activation_id = format!(
            "capacity-{max_submissions}-{account_cap}-{concurrency_cap}-{daily_side_effect_cap}"
        );
        activation.channel = "canary".to_string();
        activation.account_allowlist_sha256 = Some(allowlist.allowlist_sha256);
        activation.canary_max_submissions = max_submissions;
        activation.canary_account_cap = account_cap;
        activation.canary_concurrency_cap = concurrency_cap;
        activation.canary_daily_side_effect_cap = daily_side_effect_cap;
        activation.canary_evidence_manifest_sha256 = Some(fixture.manifest_sha256.clone());
        activation.approval_ref = "round-604-capacity-matrix".to_string();
        let activation_envelope = envelope(
            &activation,
            "activation",
            ATS_CERTIFICATION_ACTIVATION_AUDIENCE,
            activation.issued_at_ms,
            &fixture.authority,
            &format!("authorize-{}", activation.activation_id),
        );
        import_ats_certification_activation_at(
            &fixture.pool,
            &activation_envelope,
            "capacity-promoter",
            TEST_NOW_MS,
        )
        .unwrap();
        let activation_sha256 = envelope_sha256(&activation_envelope);
        let head = apply_ats_certification_activation_at(
            &fixture.pool,
            &activation_sha256,
            0,
            None,
            "capacity-promoter",
            TEST_NOW_MS,
        )
        .unwrap();
        fixture.activation = activation;
        fixture.activation_sha256 = activation_sha256;
        fixture.head = head;
        fixture
    }

    fn insert_capacity_attempt_status(
        pool: &DbPool,
        request: &AtsApplicationCertificationBindingRequest,
        status: &str,
    ) {
        let conn = pool.get().unwrap();
        let job_id = format!("job-{}", request.binding_id);
        conn.execute(
            "INSERT INTO accounts (id, email, password_hash, trial_seconds_remaining)
             VALUES (?1, ?2, 'hash', 0)",
            params![
                request.account_id,
                format!("{}@capacity.test", request.account_id)
            ],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO jobs_postings (
                id, account_id, canonical_key, posting_json, source, canonical_url,
                company, title, location, match_score, status, created_at_ms, updated_at_ms
             ) VALUES (?1, ?2, ?3, '{}', 'greenhouse', NULL, 'Acme', 'Engineer',
                       NULL, 100, 'matched', ?4, ?4)",
            params![
                job_id,
                request.account_id,
                format!("capacity:{}", request.binding_id),
                TEST_NOW_MS - 10,
            ],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO jobs_applications (
                id, account_id, job_id, state, application_json, created_at_ms, updated_at_ms
             ) VALUES (?1, ?2, ?3, 'running', '{}', ?4, ?4)",
            params![
                request.application_id,
                request.account_id,
                job_id,
                TEST_NOW_MS - 10,
            ],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO jobs_attempt_reservations (
                id, account_id, application_id, company_key, period_key, runner, status,
                reserved_at_ms, updated_at_ms
             ) VALUES (?1, ?2, ?3, ?4, '2033-05-18', 'local', ?5, ?6, ?6)",
            params![
                request.application_attempt_id,
                request.account_id,
                request.application_id,
                format!("company-{}", request.binding_id),
                status,
                TEST_NOW_MS,
            ],
        )
        .unwrap();
    }

    fn assert_canary_capacity_denial_is_atomic(
        caps: (i64, i64, i64, i64),
        first_attempt_status: Option<&str>,
        second_account_id: &str,
    ) {
        let fixture = imported_canary_fixture(caps.0, caps.1, caps.2, caps.3);
        let first_binding =
            test_canary_binding_request("capacity-first", "account-1", "application-1", 'c');
        create_ats_application_certification_binding(&fixture.pool, &first_binding, TEST_NOW_MS)
            .unwrap();
        let mut first_phase_b = test_phase_b_request(&first_binding, "capacity-first");
        first_phase_b.period_key =
            ats_certification_canary_utc_period_key(TEST_NOW_MS + 1).unwrap();
        validate_consume_reserve_ats_application_certification(
            &fixture.pool,
            &first_phase_b,
            TEST_NOW_MS + 1,
        )
        .unwrap();
        if let Some(status) = first_attempt_status {
            insert_capacity_attempt_status(&fixture.pool, &first_binding, status);
        }

        let second_binding =
            test_canary_binding_request("capacity-second", second_account_id, "application-2", 'd');
        create_ats_application_certification_binding(
            &fixture.pool,
            &second_binding,
            TEST_NOW_MS + 2,
        )
        .unwrap();
        let mut second_phase_b = test_phase_b_request(&second_binding, "capacity-second");
        second_phase_b.period_key =
            ats_certification_canary_utc_period_key(TEST_NOW_MS + 3).unwrap();
        assert!(matches!(
            validate_consume_reserve_ats_application_certification(
                &fixture.pool,
                &second_phase_b,
                TEST_NOW_MS + 3,
            ),
            Err(AtsCertificationAuthorityError::CapacityUnavailable)
        ));

        let conn = fixture.pool.get().unwrap();
        let (phase, fence): (String, i64) = conn
            .query_row(
                "SELECT phase, fence
                   FROM jobs_application_ats_certification_bindings
                  WHERE binding_id = ?1",
                params![second_binding.binding_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        let reservation_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM jobs_ats_certification_canary_reservations",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!((phase.as_str(), fence), ("preflight", 0));
        assert_eq!(reservation_count, 1);
    }

    fn test_evidence(source_kind: &str, policy_sha256: &str) -> AtsCertificationEvidenceAuthority {
        AtsCertificationEvidenceAuthority {
            version: 1,
            audience: ATS_CERTIFICATION_EVIDENCE_AUDIENCE.to_string(),
            evidence_id: format!("evidence-{source_kind}"),
            policy_sha256: policy_sha256.to_string(),
            provider: "greenhouse".to_string(),
            target_key: "greenhouse:acme:123".to_string(),
            variant_key: "greenhouse_public".to_string(),
            surface_sha256: "a".repeat(64),
            source_kind: source_kind.to_string(),
            object_key: format!("ats-certification/evidence-{source_kind}.json"),
            object_sha256: "b".repeat(64),
            object_size_bytes: 4_096,
            media_type: "application/vnd.bluey.ats-certification-evidence+json".to_string(),
            provenance_sha256: "c".repeat(64),
            authorization_ref: "sandbox-authorization-2026-08".to_string(),
            sanitizer_version: "sanitizer-1".to_string(),
            captured_at_ms: TEST_NOW_MS - 2_000,
            issued_at_ms: TEST_NOW_MS - 1_000,
            expires_at_ms: TEST_NOW_MS + 1_000_000,
        }
    }

    fn test_layout_observation(
        evidence_class: &str,
        policy_sha256: &str,
    ) -> AtsLayoutObservationAuthority {
        AtsLayoutObservationAuthority {
            version: 1,
            audience: ATS_CERTIFICATION_LAYOUT_OBSERVATION_AUDIENCE.to_string(),
            observation_id: format!("greenhouse-layout-{evidence_class}-1"),
            policy_sha256: policy_sha256.to_string(),
            provider: "greenhouse".to_string(),
            target_fingerprint_sha256: ats_certification_target_fingerprint_sha256(
                "greenhouse",
                "greenhouse:acme:123",
            )
            .unwrap(),
            page_variant: "greenhouse_public".to_string(),
            surface_sha256: "a".repeat(64),
            adapter_version: ATS_GREENHOUSE_EXACT_ADAPTER_VERSION.to_string(),
            runner_target_sha256: "1".repeat(64),
            evidence_class: evidence_class.to_string(),
            controls: vec![AtsLayoutControlObservation {
                control_kind: "submit".to_string(),
                required: true,
                provider_attribute_sha256: "4".repeat(64),
                option_count: 0,
                conditional_on_attribute_sha256: None,
            }],
            form: AtsLayoutFormObservation {
                method: "post".to_string(),
                encoding: "multipart/form-data".to_string(),
                target_sha256: "5".repeat(64),
                action_identity_sha256: "6".repeat(64),
                submit_control_sha256: "7".repeat(64),
            },
            challenge_categories: vec!["captcha".to_string()],
            step_count: 1,
            confirmation_state_categories: vec!["provider_confirmation".to_string()],
            predecessor_observation_sha256: None,
            observed_at_ms: TEST_NOW_MS - 2_000,
            issued_at_ms: TEST_NOW_MS - 1_500,
            expires_at_ms: TEST_NOW_MS + 1_000_000,
        }
    }

    fn import_test_evidence_object(
        pool: &DbPool,
        authority: &TestAuthority,
        policy_sha256: &str,
        suffix: &str,
    ) -> String {
        let mut evidence = test_evidence("authorized_sandbox", policy_sha256);
        evidence.evidence_id = format!("test-evidence-{suffix}");
        evidence.object_key = format!("ats-certification/test-evidence-{suffix}.json");
        evidence.object_sha256 =
            ats_certification_sha256(format!("test-evidence-object-{suffix}").as_bytes());
        evidence.provenance_sha256 =
            ats_certification_sha256(format!("test-evidence-provenance-{suffix}").as_bytes());
        let evidence_envelope = envelope(
            &evidence,
            "evidence",
            ATS_CERTIFICATION_EVIDENCE_AUDIENCE,
            evidence.issued_at_ms,
            authority,
            &format!("authorize-test-evidence-{suffix}"),
        );
        import_ats_certification_evidence_at(pool, &evidence_envelope, "certifier", TEST_NOW_MS)
            .unwrap();
        envelope_sha256(&evidence_envelope)
    }

    fn import_test_layout_observation(
        pool: &DbPool,
        authority: &TestAuthority,
        policy_sha256: &str,
        runner_target_sha256: &str,
        evidence_class: &str,
        suffix: &str,
    ) -> String {
        let mut layout = test_layout_observation(evidence_class, policy_sha256);
        layout.observation_id = format!("test-layout-{suffix}");
        layout.runner_target_sha256 = runner_target_sha256.to_string();
        let layout_envelope = envelope(
            &layout,
            "layout_observation",
            ATS_CERTIFICATION_LAYOUT_OBSERVATION_AUDIENCE,
            layout.issued_at_ms,
            authority,
            &format!("authorize-test-layout-{suffix}"),
        );
        import_ats_layout_observation_at(pool, &layout_envelope, "observer", TEST_NOW_MS).unwrap();
        envelope_sha256(&layout_envelope)
    }

    fn test_check_results(
        runtime_targets: &[AtsCertificationRuntimeTarget],
        evidence_classes: &[&str],
    ) -> Vec<AtsCertificationCheckResult> {
        let mut results = runtime_targets
            .iter()
            .flat_map(|runtime| {
                ATS_CERTIFICATION_REQUIRED_CHECK_IDS
                    .iter()
                    .flat_map(move |check_id| {
                        evidence_classes.iter().map(move |evidence_class| {
                            AtsCertificationCheckResult {
                                runner_target_sha256: runtime.runtime_sha256.clone(),
                                check_id: (*check_id).to_string(),
                                evidence_class: (*evidence_class).to_string(),
                                passed_count: 1,
                                failed_count: 0,
                                skipped_count: 0,
                            }
                        })
                    })
            })
            .collect::<Vec<_>>();
        results.sort_by(|left, right| {
            (
                &left.runner_target_sha256,
                &left.check_id,
                &left.evidence_class,
            )
                .cmp(&(
                    &right.runner_target_sha256,
                    &right.check_id,
                    &right.evidence_class,
                ))
        });
        results
    }

    fn test_manifest(
        evidence_sha256: String,
        layout_observation_sha256: String,
        policy_sha256: &str,
    ) -> AtsCertificationManifestAuthority {
        let surface = test_surface();
        AtsCertificationManifestAuthority {
            version: 1,
            audience: ATS_CERTIFICATION_MANIFEST_AUDIENCE.to_string(),
            certification_id: "greenhouse-acme-123-cert-1".to_string(),
            policy_sha256: policy_sha256.to_string(),
            manifest_generation: 1,
            predecessor_manifest_sha256: None,
            provider: "greenhouse".to_string(),
            target_key: "greenhouse:acme:123".to_string(),
            allowed_provider_hosts: ATS_GREENHOUSE_ALLOWED_PROVIDER_HOSTS
                .map(str::to_string)
                .to_vec(),
            variant_key: surface.variant_key.clone(),
            surface_sha256: surface.surface_sha256.clone(),
            scope_sha256: ats_certification_scope_sha256(
                "greenhouse",
                "greenhouse:acme:123",
                &surface.variant_key,
                &surface.surface_sha256,
            )
            .unwrap(),
            adapter_version: ATS_GREENHOUSE_EXACT_ADAPTER_VERSION.to_string(),
            final_submit_control_id: ATS_GREENHOUSE_FINAL_SUBMIT_CONTROL_ID.to_string(),
            adapter_bundle_sha256: "d".repeat(64),
            source_commit: "e".repeat(40),
            layout_contract_version: surface.layout_contract_version,
            layout_contract_sha256: "f".repeat(64),
            maximum_capability: "unattended_submit".to_string(),
            certification_profile: AtsManifestCertificationProfile {
                suite_id: "bluey-ats-complete-v1".to_string(),
                suite_version: "1.0.0".to_string(),
                suite_manifest_sha256: "8".repeat(64),
                layout_set_sha256: ats_certification_layout_set_sha256(std::slice::from_ref(
                    &layout_observation_sha256,
                ))
                .unwrap(),
                layout_observation_sha256s: vec![layout_observation_sha256],
                check_results: ATS_CERTIFICATION_REQUIRED_CHECK_IDS
                    .iter()
                    .flat_map(|check_id| {
                        ["authorized_live", "authorized_sandbox"].map(|evidence_class| {
                            AtsCertificationCheckResult {
                                runner_target_sha256: "1".repeat(64),
                                check_id: (*check_id).to_string(),
                                evidence_class: evidence_class.to_string(),
                                passed_count: 1,
                                failed_count: 0,
                                skipped_count: 0,
                            }
                        })
                    })
                    .collect(),
                zero_tolerance: AtsCertificationZeroToleranceCounters {
                    hard_filter_violations: 0,
                    unsupported_factual_claims: 0,
                    duplicate_submit_activations: 0,
                    false_submitted_states: 0,
                    incomplete_or_mismatched_receipts: 0,
                    pii_bearing_observations: 0,
                },
            },
            evidence_sha256s: vec![evidence_sha256],
            runtime_targets: vec![AtsCertificationRuntimeTarget {
                runtime_kind: "local".to_string(),
                runtime_id: "local:browser-release-1".to_string(),
                runtime_sha256: "1".repeat(64),
                platform: "macos".to_string(),
                architecture: "arm64".to_string(),
                automation_bundle_sha256: "2".repeat(64),
                browser_release_manifest_sha256: Some("3".repeat(64)),
                browser_artifact_sha256: Some("4".repeat(64)),
                browser_build_descriptor_sha256: Some("5".repeat(64)),
                runner_build_id: None,
                runner_image_sha256: None,
                playwright_version: "1.55.0".to_string(),
                chromium_revision: "1187".to_string(),
                chromium_executable_sha256: "6".repeat(64),
            }],
            tested_at_ms: TEST_NOW_MS - 1_000,
            issued_at_ms: TEST_NOW_MS - 900,
            not_before_ms: TEST_NOW_MS - 800,
            expires_at_ms: TEST_NOW_MS + 900_000,
        }
    }

    fn test_cloud_manifest(
        evidence_sha256: String,
        layout_observation_sha256: String,
        policy_sha256: &str,
    ) -> AtsCertificationManifestAuthority {
        let mut manifest = test_manifest(evidence_sha256, layout_observation_sha256, policy_sha256);
        manifest.runtime_targets = vec![AtsCertificationRuntimeTarget {
            runtime_kind: "cloud".to_string(),
            runtime_id: "cloud:runner-604.1".to_string(),
            runtime_sha256: "1".repeat(64),
            platform: "linux".to_string(),
            architecture: "x86_64".to_string(),
            automation_bundle_sha256: "2".repeat(64),
            browser_release_manifest_sha256: None,
            browser_artifact_sha256: None,
            browser_build_descriptor_sha256: None,
            runner_build_id: Some("runner-604.1".to_string()),
            runner_image_sha256: Some("3".repeat(64)),
            playwright_version: "1.55.0".to_string(),
            chromium_revision: "1187".to_string(),
            chromium_executable_sha256: "6".repeat(64),
        }];
        manifest
    }

    fn test_activation(
        manifest_sha256: String,
        scope_sha256: String,
        policy_sha256: &str,
    ) -> AtsCertificationActivationAuthority {
        AtsCertificationActivationAuthority {
            version: 1,
            audience: ATS_CERTIFICATION_ACTIVATION_AUDIENCE.to_string(),
            activation_id: "greenhouse-acme-123-general-1".to_string(),
            policy_sha256: policy_sha256.to_string(),
            activation_generation: 1,
            predecessor_activation_sha256: None,
            manifest_sha256,
            scope_sha256,
            channel: "general".to_string(),
            channel_sequence: 1,
            capability: "unattended_submit".to_string(),
            account_allowlist_sha256: None,
            canary_max_submissions: 0,
            canary_account_cap: 0,
            canary_concurrency_cap: 0,
            canary_daily_side_effect_cap: 0,
            canary_evidence_manifest_sha256: None,
            approval_ref: "approval-general-1".to_string(),
            issued_at_ms: TEST_NOW_MS - 700,
            not_before_ms: TEST_NOW_MS - 600,
            expires_at_ms: TEST_NOW_MS + 800_000,
        }
    }

    fn configure_test_canary_activation(
        pool: &DbPool,
        activation: &mut AtsCertificationActivationAuthority,
        suffix: &str,
        account_ids: Vec<String>,
        caps: (i64, i64, i64, i64),
    ) {
        let approval_ref = format!("test-canary-approval-{suffix}");
        let allowlist = import_ats_certification_canary_allowlist(
            pool,
            &AtsCertificationCanaryAllowlistImportRequest {
                schema_version: 1,
                allowlist_id: format!("test-canary-allowlist-{suffix}"),
                account_ids,
                approval_ref: approval_ref.clone(),
                not_before_ms: TEST_NOW_MS - 700,
                expires_at_ms: TEST_NOW_MS + 900_000,
            },
            "test-canary-operator",
            TEST_NOW_MS,
        )
        .unwrap();
        activation.activation_id = format!("test-canary-activation-{suffix}");
        activation.channel = "canary".to_string();
        activation.account_allowlist_sha256 = Some(allowlist.allowlist_sha256);
        activation.canary_max_submissions = caps.0;
        activation.canary_account_cap = caps.1;
        activation.canary_concurrency_cap = caps.2;
        activation.canary_daily_side_effect_cap = caps.3;
        activation.canary_evidence_manifest_sha256 = Some(activation.manifest_sha256.clone());
        activation.approval_ref = approval_ref;
    }

    struct ImportedFixture {
        pool: DbPool,
        authority: TestAuthority,
        manifest: AtsCertificationManifestAuthority,
        manifest_sha256: String,
        activation: AtsCertificationActivationAuthority,
        activation_sha256: String,
        head: AtsCertificationHeadResult,
        policy_sha256: String,
    }

    #[derive(Debug, Clone)]
    pub(super) struct InstalledAtsAuthorityFixture {
        pub(super) target_evidence: AtsCertificationFreshTargetEvidence,
        pub(super) surface: AtsObservedSurface,
        pub(super) manifest_sha256: String,
        pub(super) activation_sha256: String,
        pub(super) runtime_target: AtsCertificationRuntimeTarget,
    }

    struct InstalledFixtureHead {
        head_revision: i64,
        transition_sha256: String,
        activation_sha256: String,
        activation_generation: i64,
        channel_sequence: i64,
        manifest_sha256: String,
        manifest_generation: i64,
    }

    fn installed_fixture_head(
        pool: &DbPool,
        scope_sha256: &str,
        channel: &str,
    ) -> Result<Option<InstalledFixtureHead>, AtsCertificationAuthorityError> {
        match pool {
            DbPool::Sqlite(_) => {
                let conn = pool.get().map_err(ats_certification_storage)?;
                conn.query_row(
                    "SELECT head.head_revision, head.current_transition_sha256,
                            head.current_activation_sha256, activation.activation_generation,
                            head.current_channel_sequence, manifest.manifest_sha256,
                            manifest.manifest_generation
                       FROM jobs_ats_certification_heads head
                       JOIN jobs_ats_certification_activations activation
                         ON activation.activation_sha256 = head.current_activation_sha256
                       JOIN jobs_ats_certification_manifests manifest
                         ON manifest.manifest_sha256 = activation.manifest_sha256
                      WHERE head.scope_sha256 = ?1 AND head.channel = ?2",
                    params![scope_sha256, channel],
                    |row| {
                        Ok(InstalledFixtureHead {
                            head_revision: row.get(0)?,
                            transition_sha256: row.get(1)?,
                            activation_sha256: row.get(2)?,
                            activation_generation: row.get(3)?,
                            channel_sequence: row.get(4)?,
                            manifest_sha256: row.get(5)?,
                            manifest_generation: row.get(6)?,
                        })
                    },
                )
                .optional()
                .map_err(ats_certification_storage)
            }
            DbPool::Postgres(_) => {
                let mut conn = pool.get_pg().map_err(ats_certification_storage)?;
                Ok(conn
                    .query_opt(
                        "SELECT head.head_revision, head.current_transition_sha256,
                                head.current_activation_sha256,
                                activation.activation_generation,
                                head.current_channel_sequence, manifest.manifest_sha256,
                                manifest.manifest_generation
                           FROM jobs_ats_certification_heads head
                           JOIN jobs_ats_certification_activations activation
                             ON activation.activation_sha256 = head.current_activation_sha256
                           JOIN jobs_ats_certification_manifests manifest
                             ON manifest.manifest_sha256 = activation.manifest_sha256
                          WHERE head.scope_sha256 = $1 AND head.channel = $2",
                        &[&scope_sha256, &channel],
                    )
                    .map_err(ats_certification_storage)?
                    .map(|row| InstalledFixtureHead {
                        head_revision: row.get(0),
                        transition_sha256: row.get(1),
                        activation_sha256: row.get(2),
                        activation_generation: row.get(3),
                        channel_sequence: row.get(4),
                        manifest_sha256: row.get(5),
                        manifest_generation: row.get(6),
                    }))
            }
        }
    }

    pub(super) fn install_signed_ats_authority_fixture(
        pool: &DbPool,
        target_evidence: AtsCertificationFreshTargetEvidence,
        surface: AtsObservedSurface,
        runtime_target: AtsCertificationRuntimeTarget,
        now_ms: i64,
    ) -> Result<InstalledAtsAuthorityFixture, AtsCertificationAuthorityError> {
        let authority = test_authority();
        let (provider, target_key) =
            validate_ats_certification_fresh_target_evidence(&target_evidence, now_ms)?;
        validate_ats_observed_surface(&surface)?;
        validate_ats_runtime_target(&runtime_target)?;
        if provider != "greenhouse" {
            return Err(AtsCertificationAuthorityError::UnsupportedUrl);
        }

        let fixture_identity = serde_json::json!({
            "nowMs": now_ms,
            "runtimeTarget": runtime_target,
            "surface": surface,
            "targetEvidence": target_evidence,
        });
        let fixture_id = ats_certification_sha256(
            &ats_certification_canonical_json(&fixture_identity)
                .map_err(|_| AtsCertificationAuthorityError::InvalidAuthority)?,
        );
        let (policy_sha256, policy_expires_at_ms) =
            match load_current_ats_certification_trust_policy(pool, now_ms) {
            Ok((policy_sha256, current_policy)) => {
                let expected_policy = test_trust_policy(&authority, 1, None);
                if current_policy.delegated_trust != expected_policy.delegated_trust
                    || current_policy.certification_requirements
                        != expected_policy.certification_requirements
                {
                    return Err(AtsCertificationAuthorityError::InvalidTrustPolicy);
                }
                (policy_sha256, current_policy.expires_at_ms)
            }
            Err(AtsCertificationAuthorityError::TrustPolicyNotInitialized) => {
                let policy_issued_at_ms = now_ms.saturating_sub(4_000);
                let policy_valid_from_ms = now_ms.saturating_sub(3_000);
                let policy_expires_at_ms = now_ms
                    .checked_add(2_000_000)
                    .ok_or(AtsCertificationAuthorityError::InvalidAuthority)?;
                let mut policy = test_trust_policy(&authority, 1, None);
                policy.policy_id = format!("ats-fixture-policy-{fixture_id}");
                policy.issued_at_ms = policy_issued_at_ms;
                policy.valid_from_ms = policy_valid_from_ms;
                policy.expires_at_ms = policy_expires_at_ms;
                let policy_envelope = envelope(
                    &policy,
                    "root",
                    ATS_CERTIFICATION_TRUST_POLICY_AUDIENCE,
                    policy.issued_at_ms,
                    &authority,
                    &format!("authorize-fixture-policy-{fixture_id}"),
                );
                let policy_sha256 = envelope_sha256(&policy_envelope);
                import_ats_certification_trust_policy_with_root_at(
                    pool,
                    &policy_envelope,
                    &authority.root_anchor,
                    "fixture-root-operator",
                    now_ms,
                )?;
                (policy_sha256, policy_expires_at_ms)
            }
            Err(error) => return Err(error),
        };

        let evidence_expires_at_ms = now_ms
            .checked_add(1_000_000)
            .ok_or(AtsCertificationAuthorityError::InvalidAuthority)?
            .min(policy_expires_at_ms);
        let mut evidence = test_evidence("authorized_sandbox", &policy_sha256);
        evidence.evidence_id = format!("fixture-evidence-{fixture_id}");
        evidence.provider = provider.clone();
        evidence.target_key = target_key.clone();
        evidence.variant_key = surface.variant_key.clone();
        evidence.surface_sha256 = surface.surface_sha256.clone();
        evidence.object_key = format!("ats-certification/fixture-{fixture_id}.json");
        evidence.object_sha256 =
            ats_certification_sha256(format!("fixture-evidence-object-{fixture_id}").as_bytes());
        evidence.provenance_sha256 = ats_certification_sha256(
            format!("fixture-evidence-provenance-{fixture_id}").as_bytes(),
        );
        evidence.captured_at_ms = now_ms.saturating_sub(2_000);
        evidence.issued_at_ms = now_ms.saturating_sub(1_000);
        evidence.expires_at_ms = evidence_expires_at_ms;
        let evidence_envelope = envelope(
            &evidence,
            "evidence",
            ATS_CERTIFICATION_EVIDENCE_AUDIENCE,
            evidence.issued_at_ms,
            &authority,
            &format!("authorize-fixture-evidence-{fixture_id}"),
        );
        import_ats_certification_evidence_at(
            pool,
            &evidence_envelope,
            "fixture-certifier",
            now_ms,
        )?;

        let mut layout = test_layout_observation("authorized_sandbox", &policy_sha256);
        layout.observation_id = format!("fixture-layout-{fixture_id}");
        layout.provider = provider.clone();
        layout.target_fingerprint_sha256 =
            ats_certification_target_fingerprint_sha256(&provider, &target_key)?;
        layout.page_variant = surface.variant_key.clone();
        layout.surface_sha256 = surface.surface_sha256.clone();
        layout.runner_target_sha256 = runtime_target.runtime_sha256.clone();
        layout.observed_at_ms = now_ms.saturating_sub(2_000);
        layout.issued_at_ms = now_ms.saturating_sub(1_500);
        layout.expires_at_ms = evidence_expires_at_ms;
        let layout_envelope = envelope(
            &layout,
            "layout_observation",
            ATS_CERTIFICATION_LAYOUT_OBSERVATION_AUDIENCE,
            layout.issued_at_ms,
            &authority,
            &format!("authorize-fixture-layout-{fixture_id}"),
        );
        import_ats_layout_observation_at(pool, &layout_envelope, "fixture-observer", now_ms)?;
        let mut live_layout = layout.clone();
        live_layout.observation_id = format!("fixture-live-layout-{fixture_id}");
        live_layout.evidence_class = "authorized_live".to_string();
        let live_layout_envelope = envelope(
            &live_layout,
            "layout_observation",
            ATS_CERTIFICATION_LAYOUT_OBSERVATION_AUDIENCE,
            live_layout.issued_at_ms,
            &authority,
            &format!("authorize-fixture-live-layout-{fixture_id}"),
        );
        import_ats_layout_observation_at(
            pool,
            &live_layout_envelope,
            "fixture-live-observer",
            now_ms,
        )?;

        let manifest_expires_at_ms = now_ms
            .checked_add(900_000)
            .ok_or(AtsCertificationAuthorityError::InvalidAuthority)?
            .min(evidence_expires_at_ms);
        let mut manifest = test_manifest(
            envelope_sha256(&evidence_envelope),
            envelope_sha256(&layout_envelope),
            &policy_sha256,
        );
        manifest.certification_id = format!("fixture-certification-{fixture_id}");
        manifest.provider = provider.clone();
        manifest.target_key = target_key.clone();
        manifest.variant_key = surface.variant_key.clone();
        manifest.surface_sha256 = surface.surface_sha256.clone();
        manifest.scope_sha256 = ats_certification_scope_sha256(
            &provider,
            &target_key,
            &surface.variant_key,
            &surface.surface_sha256,
        )?;
        manifest.layout_contract_version = surface.layout_contract_version;
        manifest.adapter_bundle_sha256 =
            ats_certification_sha256(format!("fixture-adapter-bundle-{fixture_id}").as_bytes());
        manifest.runtime_targets = vec![runtime_target.clone()];
        manifest.certification_profile.layout_observation_sha256s = vec![
            envelope_sha256(&layout_envelope),
            envelope_sha256(&live_layout_envelope),
        ];
        manifest
            .certification_profile
            .layout_observation_sha256s
            .sort_unstable();
        manifest.certification_profile.layout_set_sha256 = ats_certification_layout_set_sha256(
            &manifest.certification_profile.layout_observation_sha256s,
        )?;
        manifest.certification_profile.check_results = ATS_CERTIFICATION_REQUIRED_CHECK_IDS
            .iter()
            .flat_map(|check_id| {
                ["authorized_live", "authorized_sandbox"].map(|evidence_class| {
                    AtsCertificationCheckResult {
                        runner_target_sha256: runtime_target.runtime_sha256.clone(),
                        check_id: (*check_id).to_string(),
                        evidence_class: evidence_class.to_string(),
                        passed_count: 1,
                        failed_count: 0,
                        skipped_count: 0,
                    }
                })
            })
            .collect();
        manifest.tested_at_ms = now_ms.saturating_sub(1_000);
        manifest.issued_at_ms = now_ms.saturating_sub(900);
        manifest.not_before_ms = now_ms.saturating_sub(800);
        manifest.expires_at_ms = manifest_expires_at_ms;
        let current_head = installed_fixture_head(pool, &manifest.scope_sha256, "general")?;
        if let Some(head) = current_head.as_ref() {
            manifest.manifest_generation = head
                .manifest_generation
                .checked_add(1)
                .ok_or(AtsCertificationAuthorityError::InvalidAuthority)?;
            manifest.predecessor_manifest_sha256 = Some(head.manifest_sha256.clone());
        }
        let manifest_envelope = envelope(
            &manifest,
            "manifest",
            ATS_CERTIFICATION_MANIFEST_AUDIENCE,
            manifest.issued_at_ms,
            &authority,
            &format!("authorize-fixture-manifest-{fixture_id}"),
        );
        import_ats_certification_manifest_at(
            pool,
            &manifest_envelope,
            "fixture-certifier",
            now_ms,
        )?;
        let manifest_sha256 = envelope_sha256(&manifest_envelope);

        let mut activation = test_activation(
            manifest_sha256.clone(),
            manifest.scope_sha256.clone(),
            &policy_sha256,
        );
        if let Some(head) = current_head.as_ref() {
            activation.activation_generation = head
                .activation_generation
                .checked_add(1)
                .ok_or(AtsCertificationAuthorityError::InvalidAuthority)?;
            activation.predecessor_activation_sha256 = Some(head.activation_sha256.clone());
            activation.channel_sequence = head
                .channel_sequence
                .checked_add(1)
                .ok_or(AtsCertificationAuthorityError::InvalidAuthority)?;
        }
        activation.activation_id = format!("fixture-activation-{fixture_id}");
        activation.approval_ref = format!("fixture-approval-{fixture_id}");
        activation.issued_at_ms = now_ms.saturating_sub(700);
        activation.not_before_ms = now_ms.saturating_sub(600);
        activation.expires_at_ms = now_ms
            .checked_add(800_000)
            .ok_or(AtsCertificationAuthorityError::InvalidAuthority)?
            .min(manifest_expires_at_ms);
        let activation_envelope = envelope(
            &activation,
            "activation",
            ATS_CERTIFICATION_ACTIVATION_AUDIENCE,
            activation.issued_at_ms,
            &authority,
            &format!("authorize-fixture-activation-{fixture_id}"),
        );
        import_ats_certification_activation_at(
            pool,
            &activation_envelope,
            "fixture-promoter",
            now_ms,
        )?;
        let activation_sha256 = envelope_sha256(&activation_envelope);
        let expected_head_revision = current_head
            .as_ref()
            .map_or(0, |head| head.head_revision);
        let expected_transition_sha256 = current_head
            .as_ref()
            .map(|head| head.transition_sha256.as_str());
        apply_ats_certification_activation_at(
            pool,
            &activation_sha256,
            expected_head_revision,
            expected_transition_sha256,
            "fixture-promoter",
            now_ms,
        )?;

        Ok(InstalledAtsAuthorityFixture {
            target_evidence,
            surface,
            manifest_sha256,
            activation_sha256,
            runtime_target,
        })
    }

    fn imported_fixture() -> ImportedFixture {
        imported_fixture_for_runtime(false)
    }

    fn imported_cloud_fixture() -> ImportedFixture {
        imported_fixture_for_runtime(true)
    }

    fn imported_fixture_for_runtime(cloud_runtime: bool) -> ImportedFixture {
        imported_fixture_for_runtime_in(test_pool(), cloud_runtime)
    }

    fn imported_fixture_for_runtime_in(pool: DbPool, cloud_runtime: bool) -> ImportedFixture {
        let authority = test_authority();
        let policy_sha256 = initialize_test_trust_policy(&pool, &authority);
        let evidence = test_evidence("authorized_sandbox", &policy_sha256);
        let evidence_envelope = envelope(
            &evidence,
            "evidence",
            ATS_CERTIFICATION_EVIDENCE_AUDIENCE,
            evidence.issued_at_ms,
            &authority,
            "authorize-evidence-1",
        );
        import_ats_certification_evidence_at(&pool, &evidence_envelope, "certifier", TEST_NOW_MS)
            .unwrap();
        let layout = test_layout_observation("authorized_sandbox", &policy_sha256);
        let layout_envelope = envelope(
            &layout,
            "layout_observation",
            ATS_CERTIFICATION_LAYOUT_OBSERVATION_AUDIENCE,
            layout.issued_at_ms,
            &authority,
            "authorize-layout-1",
        );
        import_ats_layout_observation_at(&pool, &layout_envelope, "observer", TEST_NOW_MS).unwrap();
        let live_layout = test_layout_observation("authorized_live", &policy_sha256);
        let live_layout_envelope = envelope(
            &live_layout,
            "layout_observation",
            ATS_CERTIFICATION_LAYOUT_OBSERVATION_AUDIENCE,
            live_layout.issued_at_ms,
            &authority,
            "authorize-live-layout-1",
        );
        import_ats_layout_observation_at(
            &pool,
            &live_layout_envelope,
            "live-observer",
            TEST_NOW_MS,
        )
        .unwrap();
        let evidence_sha256 = envelope_sha256(&evidence_envelope);
        let layout_sha256 = envelope_sha256(&layout_envelope);
        let mut manifest = if cloud_runtime {
            test_cloud_manifest(evidence_sha256, layout_sha256, &policy_sha256)
        } else {
            test_manifest(evidence_sha256, layout_sha256, &policy_sha256)
        };
        manifest
            .certification_profile
            .layout_observation_sha256s
            .push(envelope_sha256(&live_layout_envelope));
        manifest
            .certification_profile
            .layout_observation_sha256s
            .sort_unstable();
        manifest.certification_profile.layout_set_sha256 = ats_certification_layout_set_sha256(
            &manifest.certification_profile.layout_observation_sha256s,
        )
        .unwrap();
        let manifest_envelope = envelope(
            &manifest,
            "manifest",
            ATS_CERTIFICATION_MANIFEST_AUDIENCE,
            manifest.issued_at_ms,
            &authority,
            "authorize-manifest-1",
        );
        import_ats_certification_manifest_at(&pool, &manifest_envelope, "certifier", TEST_NOW_MS)
            .unwrap();
        let manifest_sha256 = envelope_sha256(&manifest_envelope);
        let activation = test_activation(
            manifest_sha256.clone(),
            manifest.scope_sha256.clone(),
            &policy_sha256,
        );
        let activation_envelope = envelope(
            &activation,
            "activation",
            ATS_CERTIFICATION_ACTIVATION_AUDIENCE,
            activation.issued_at_ms,
            &authority,
            "authorize-activation-1",
        );
        import_ats_certification_activation_at(
            &pool,
            &activation_envelope,
            "promoter",
            TEST_NOW_MS,
        )
        .unwrap();
        let activation_sha256 = envelope_sha256(&activation_envelope);
        let head = apply_ats_certification_activation_at(
            &pool,
            &activation_sha256,
            0,
            None,
            "promoter",
            TEST_NOW_MS,
        )
        .unwrap();
        ImportedFixture {
            pool,
            authority,
            manifest,
            manifest_sha256,
            activation,
            activation_sha256,
            head,
            policy_sha256,
        }
    }

    struct IsolatedPostgresFixture {
        pool: DbPool,
        authority: TestAuthority,
        policy_sha256: String,
        target_evidence: AtsCertificationFreshTargetEvidence,
        surface: AtsObservedSurface,
        manifest: AtsCertificationManifestAuthority,
        manifest_sha256: String,
        activation_sha256: String,
    }

    fn isolated_postgres_fixture(
        pool: DbPool,
        suffix: &str,
        runtime_count: usize,
        apply_head: bool,
    ) -> IsolatedPostgresFixture {
        assert!((1..=2).contains(&runtime_count));
        let authority = test_authority();
        let policy_sha256 = initialize_test_trust_policy(&pool, &authority);
        let target_key = format!("greenhouse:acme:{suffix}");
        let surface = AtsObservedSurface {
            variant_key: "greenhouse_public".to_string(),
            layout_contract_version: 1,
            surface_sha256: ats_certification_sha256(
                format!("round604-surface-{suffix}").as_bytes(),
            ),
        };
        let target_evidence = AtsCertificationFreshTargetEvidence {
            canonical_url: format!("https://boards.greenhouse.io/acme/jobs/{suffix}"),
            discovery_provider: "greenhouse".to_string(),
            discovery_target_key: target_key.clone(),
            discovery_observed_at_ms: TEST_NOW_MS - 100,
            original_source_provider: "greenhouse".to_string(),
            original_source_target_key: target_key.clone(),
            original_source_observed_at_ms: TEST_NOW_MS - 50,
        };
        let runtime_targets = (0..runtime_count)
            .map(|index| {
                let mut runtime = test_manifest("0".repeat(64), "9".repeat(64), &policy_sha256)
                    .runtime_targets
                    .remove(0);
                runtime.runtime_id = format!("local:round604-{suffix}-{index}");
                runtime.runtime_sha256 = ats_certification_sha256(
                    format!("round604-runtime-{suffix}-{index}").as_bytes(),
                );
                runtime
            })
            .collect::<Vec<_>>();

        let mut evidence = test_evidence("authorized_sandbox", &policy_sha256);
        evidence.evidence_id = format!("round604-evidence-{suffix}");
        evidence.target_key = target_key.clone();
        evidence.surface_sha256 = surface.surface_sha256.clone();
        evidence.object_key = format!("ats-certification/round604-evidence-{suffix}.json");
        evidence.object_sha256 =
            ats_certification_sha256(format!("round604-object-{suffix}").as_bytes());
        evidence.provenance_sha256 =
            ats_certification_sha256(format!("round604-provenance-{suffix}").as_bytes());
        let evidence_envelope = envelope(
            &evidence,
            "evidence",
            ATS_CERTIFICATION_EVIDENCE_AUDIENCE,
            evidence.issued_at_ms,
            &authority,
            &format!("authorize-round604-evidence-{suffix}"),
        );
        import_ats_certification_evidence_at(&pool, &evidence_envelope, "certifier", TEST_NOW_MS)
            .unwrap();

        let mut layout_sha256s = Vec::new();
        for (runtime_index, runtime) in runtime_targets.iter().enumerate() {
            for evidence_class in ["authorized_live", "authorized_sandbox"] {
                let mut layout = test_layout_observation(evidence_class, &policy_sha256);
                layout.observation_id =
                    format!("round604-layout-{suffix}-{runtime_index}-{evidence_class}");
                layout.target_fingerprint_sha256 =
                    ats_certification_target_fingerprint_sha256("greenhouse", &target_key).unwrap();
                layout.surface_sha256 = surface.surface_sha256.clone();
                layout.runner_target_sha256 = runtime.runtime_sha256.clone();
                let layout_envelope = envelope(
                    &layout,
                    "layout_observation",
                    ATS_CERTIFICATION_LAYOUT_OBSERVATION_AUDIENCE,
                    layout.issued_at_ms,
                    &authority,
                    &format!("authorize-round604-layout-{suffix}-{runtime_index}-{evidence_class}"),
                );
                import_ats_layout_observation_at(&pool, &layout_envelope, "observer", TEST_NOW_MS)
                    .unwrap();
                layout_sha256s.push(envelope_sha256(&layout_envelope));
            }
        }
        layout_sha256s.sort_unstable();

        let mut manifest = test_manifest(
            envelope_sha256(&evidence_envelope),
            layout_sha256s[0].clone(),
            &policy_sha256,
        );
        manifest.certification_id = format!("round604-certification-{suffix}");
        manifest.target_key = target_key;
        manifest.variant_key = surface.variant_key.clone();
        manifest.surface_sha256 = surface.surface_sha256.clone();
        manifest.scope_sha256 = ats_certification_scope_sha256(
            &manifest.provider,
            &manifest.target_key,
            &manifest.variant_key,
            &manifest.surface_sha256,
        )
        .unwrap();
        manifest.adapter_bundle_sha256 =
            ats_certification_sha256(format!("round604-adapter-{suffix}").as_bytes());
        manifest.runtime_targets = runtime_targets;
        manifest.certification_profile.layout_observation_sha256s = layout_sha256s;
        manifest.certification_profile.layout_set_sha256 = ats_certification_layout_set_sha256(
            &manifest.certification_profile.layout_observation_sha256s,
        )
        .unwrap();
        manifest.certification_profile.check_results = manifest
            .runtime_targets
            .iter()
            .flat_map(|runtime| {
                ATS_CERTIFICATION_REQUIRED_CHECK_IDS
                    .iter()
                    .flat_map(move |check_id| {
                        ["authorized_live", "authorized_sandbox"].map(|evidence_class| {
                            AtsCertificationCheckResult {
                                runner_target_sha256: runtime.runtime_sha256.clone(),
                                check_id: (*check_id).to_string(),
                                evidence_class: evidence_class.to_string(),
                                passed_count: 1,
                                failed_count: 0,
                                skipped_count: 0,
                            }
                        })
                    })
            })
            .collect();
        manifest
            .certification_profile
            .check_results
            .sort_by(|left, right| {
                (
                    &left.runner_target_sha256,
                    &left.check_id,
                    &left.evidence_class,
                )
                    .cmp(&(
                        &right.runner_target_sha256,
                        &right.check_id,
                        &right.evidence_class,
                    ))
            });
        let manifest_envelope = envelope(
            &manifest,
            "manifest",
            ATS_CERTIFICATION_MANIFEST_AUDIENCE,
            manifest.issued_at_ms,
            &authority,
            &format!("authorize-round604-manifest-{suffix}"),
        );
        import_ats_certification_manifest_at(&pool, &manifest_envelope, "certifier", TEST_NOW_MS)
            .unwrap();
        let manifest_sha256 = envelope_sha256(&manifest_envelope);

        let mut activation = test_activation(
            manifest_sha256.clone(),
            manifest.scope_sha256.clone(),
            &policy_sha256,
        );
        activation.activation_id = format!("round604-activation-{suffix}");
        activation.approval_ref = format!("round604-approval-{suffix}");
        let activation_envelope = envelope(
            &activation,
            "activation",
            ATS_CERTIFICATION_ACTIVATION_AUDIENCE,
            activation.issued_at_ms,
            &authority,
            &format!("authorize-round604-activation-{suffix}"),
        );
        import_ats_certification_activation_at(
            &pool,
            &activation_envelope,
            "promoter",
            TEST_NOW_MS,
        )
        .unwrap();
        let activation_sha256 = envelope_sha256(&activation_envelope);
        if apply_head {
            apply_ats_certification_activation_at(
                &pool,
                &activation_sha256,
                0,
                None,
                "promoter",
                TEST_NOW_MS,
            )
            .unwrap();
        }
        IsolatedPostgresFixture {
            pool,
            authority,
            policy_sha256,
            target_evidence,
            surface,
            manifest,
            manifest_sha256,
            activation_sha256,
        }
    }

    fn install_isolated_canary_activation(
        fixture: &IsolatedPostgresFixture,
        suffix: &str,
        account_ids: Vec<String>,
        caps: (i64, i64, i64, i64),
    ) -> String {
        let mut activation = test_activation(
            fixture.manifest_sha256.clone(),
            fixture.manifest.scope_sha256.clone(),
            &fixture.policy_sha256,
        );
        configure_test_canary_activation(&fixture.pool, &mut activation, suffix, account_ids, caps);
        let activation_envelope = envelope(
            &activation,
            "activation",
            ATS_CERTIFICATION_ACTIVATION_AUDIENCE,
            activation.issued_at_ms,
            &fixture.authority,
            &format!("authorize-test-canary-activation-{suffix}"),
        );
        import_ats_certification_activation_at(
            &fixture.pool,
            &activation_envelope,
            "test-canary-promoter",
            TEST_NOW_MS,
        )
        .unwrap();
        let activation_sha256 = envelope_sha256(&activation_envelope);
        apply_ats_certification_activation_at(
            &fixture.pool,
            &activation_sha256,
            0,
            None,
            "test-canary-promoter",
            TEST_NOW_MS,
        )
        .unwrap();
        activation_sha256
    }

    fn assert_sequence_two_activation_replay_requires_exact_predecessor(
        fixture: &IsolatedPostgresFixture,
        suffix: &str,
    ) {
        let first_head = apply_ats_certification_activation_at(
            &fixture.pool,
            &fixture.activation_sha256,
            0,
            None,
            "promoter",
            TEST_NOW_MS + 1,
        )
        .unwrap();
        assert!(first_head.replayed);

        let mut activation = test_activation(
            fixture.manifest_sha256.clone(),
            fixture.manifest.scope_sha256.clone(),
            &fixture.policy_sha256,
        );
        activation.activation_id = format!("round604-activation-{suffix}-sequence-2");
        activation.activation_generation = 2;
        activation.predecessor_activation_sha256 = Some(fixture.activation_sha256.clone());
        activation.channel_sequence = 2;
        activation.approval_ref = format!("round604-approval-{suffix}-sequence-2");
        let activation_envelope = envelope(
            &activation,
            "activation",
            ATS_CERTIFICATION_ACTIVATION_AUDIENCE,
            activation.issued_at_ms,
            &fixture.authority,
            &format!("authorize-round604-activation-{suffix}-sequence-2"),
        );
        import_ats_certification_activation_at(
            &fixture.pool,
            &activation_envelope,
            "promoter",
            TEST_NOW_MS + 1,
        )
        .unwrap();
        let activation_sha256 = envelope_sha256(&activation_envelope);
        let second_head = apply_ats_certification_activation_at(
            &fixture.pool,
            &activation_sha256,
            1,
            Some(&first_head.transition_sha256),
            "promoter",
            TEST_NOW_MS + 1,
        )
        .unwrap();
        assert!(!second_head.replayed);
        assert_eq!(second_head.head_revision, 2);

        let replay = apply_ats_certification_activation_at(
            &fixture.pool,
            &activation_sha256,
            1,
            Some(&first_head.transition_sha256),
            "promoter",
            TEST_NOW_MS + 2,
        )
        .unwrap();
        assert!(replay.replayed);
        assert_eq!(replay.transition_sha256, second_head.transition_sha256);

        let wrong_predecessor =
            ats_certification_sha256(format!("wrong-activation-predecessor-{suffix}").as_bytes());
        assert_ne!(wrong_predecessor, first_head.transition_sha256);
        assert_ne!(wrong_predecessor, second_head.transition_sha256);
        for rejected_transition in [&wrong_predecessor, &second_head.transition_sha256] {
            assert!(matches!(
                apply_ats_certification_activation_at(
                    &fixture.pool,
                    &activation_sha256,
                    1,
                    Some(rejected_transition),
                    "promoter",
                    TEST_NOW_MS + 3,
                ),
                Err(AtsCertificationAuthorityError::CompareAndSwapConflict)
            ));
        }
    }

    fn assert_sequence_two_quarantine_replay_requires_exact_predecessor(
        fixture: &IsolatedPostgresFixture,
        suffix: &str,
    ) {
        let command = AtsCertificationQuarantineAuthority {
            version: 1,
            audience: ATS_CERTIFICATION_QUARANTINE_AUDIENCE.to_string(),
            command_id: format!("round604-quarantine-{suffix}-sequence-1"),
            policy_sha256: fixture.policy_sha256.clone(),
            command_generation: 1,
            scope_kind: "activation".to_string(),
            scope_id: format!("round604-activation-{suffix}"),
            scope_sha256: fixture.activation_sha256.clone(),
            command_sequence: 1,
            predecessor_command_sha256: None,
            action: "quarantine".to_string(),
            reason_ref: "round604-replay-regression".to_string(),
            issued_at_ms: TEST_NOW_MS + 1,
        };
        let command_envelope = envelope(
            &command,
            "revocation",
            ATS_CERTIFICATION_QUARANTINE_AUDIENCE,
            command.issued_at_ms,
            &fixture.authority,
            &format!("authorize-round604-quarantine-{suffix}-sequence-1"),
        );
        import_ats_certification_quarantine_at(
            &fixture.pool,
            &command_envelope,
            "round604-incident-responder",
            TEST_NOW_MS + 1,
        )
        .unwrap();
        let command_sha256 = envelope_sha256(&command_envelope);
        apply_ats_certification_quarantine_at(
            &fixture.pool,
            &command_sha256,
            0,
            None,
            "round604-incident-responder",
            TEST_NOW_MS + 1,
        )
        .unwrap();

        let release = AtsCertificationQuarantineAuthority {
            version: 1,
            audience: ATS_CERTIFICATION_QUARANTINE_AUDIENCE.to_string(),
            command_id: format!("round604-quarantine-{suffix}-sequence-2"),
            policy_sha256: fixture.policy_sha256.clone(),
            command_generation: 2,
            scope_kind: "activation".to_string(),
            scope_id: command.scope_id,
            scope_sha256: fixture.activation_sha256.clone(),
            command_sequence: 2,
            predecessor_command_sha256: Some(command_sha256.clone()),
            action: "release".to_string(),
            reason_ref: "round604-replay-regression-resolved".to_string(),
            issued_at_ms: TEST_NOW_MS + 2,
        };
        let release_envelope = envelope(
            &release,
            "revocation",
            ATS_CERTIFICATION_QUARANTINE_AUDIENCE,
            release.issued_at_ms,
            &fixture.authority,
            &format!("authorize-round604-quarantine-{suffix}-sequence-2"),
        );
        import_ats_certification_quarantine_at(
            &fixture.pool,
            &release_envelope,
            "round604-incident-responder",
            TEST_NOW_MS + 2,
        )
        .unwrap();
        let release_sha256 = envelope_sha256(&release_envelope);
        let second_head = apply_ats_certification_quarantine_at(
            &fixture.pool,
            &release_sha256,
            1,
            Some(&command_sha256),
            "round604-incident-responder",
            TEST_NOW_MS + 2,
        )
        .unwrap();
        assert!(!second_head.replayed);
        assert_eq!(second_head.head_revision, 2);

        let replay = apply_ats_certification_quarantine_at(
            &fixture.pool,
            &release_sha256,
            1,
            Some(&command_sha256),
            "round604-incident-responder",
            TEST_NOW_MS + 3,
        )
        .unwrap();
        assert!(replay.replayed);

        let wrong_predecessor =
            ats_certification_sha256(format!("wrong-quarantine-predecessor-{suffix}").as_bytes());
        assert_ne!(wrong_predecessor, command_sha256);
        assert_ne!(wrong_predecessor, release_sha256);
        for rejected_command in [&wrong_predecessor, &release_sha256] {
            assert!(matches!(
                apply_ats_certification_quarantine_at(
                    &fixture.pool,
                    &release_sha256,
                    1,
                    Some(rejected_command),
                    "round604-incident-responder",
                    TEST_NOW_MS + 4,
                ),
                Err(AtsCertificationAuthorityError::CompareAndSwapConflict)
            ));
        }
    }

    fn isolated_postgres_binding_request(
        fixture: &IsolatedPostgresFixture,
        identity: &str,
        contender: &str,
        runtime_index: usize,
    ) -> AtsApplicationCertificationBindingRequest {
        let binding_id = format!("round604-binding-{identity}-{contender}");
        let mut request = test_binding_request(&binding_id, 'a');
        request.account_id = format!("round604-account-{identity}");
        request.application_id = format!("round604-application-{identity}");
        request.run_id = format!("round604-run-{identity}-{contender}");
        request.application_attempt_id = format!("round604-attempt-{identity}");
        request.browser_session_id = format!("round604-session-{identity}-{contender}");
        request.browser_profile_id = format!("round604-profile-{identity}-{contender}");
        request.packet_checksum_sha256 =
            ats_certification_sha256(format!("round604-packet-{identity}").as_bytes());
        request.auto_authorization_id = format!("round604-auto-authorization-{identity}");
        request.auto_authorization_fingerprint_sha256 =
            ats_certification_sha256(format!("round604-auto-{identity}").as_bytes());
        request.target_evidence = fixture.target_evidence.clone();
        request.runner_id = fixture.manifest.runtime_targets[runtime_index]
            .runtime_sha256
            .clone();
        request.nonce_sha256 =
            ats_certification_sha256(format!("round604-nonce-{identity}-{contender}").as_bytes());
        request
    }

    #[test]
    fn signed_fixture_installer_uses_caller_clock_and_runtime() {
        let pool = test_pool();
        let runtime_target = test_manifest("0".repeat(64), "9".repeat(64), &"8".repeat(64))
            .runtime_targets
            .remove(0);
        let fixture = install_signed_ats_authority_fixture(
            &pool,
            test_target_evidence(),
            test_surface(),
            runtime_target.clone(),
            TEST_NOW_MS,
        )
        .unwrap();
        assert_eq!(fixture.target_evidence, test_target_evidence());
        assert_eq!(fixture.surface, test_surface());
        assert_eq!(fixture.runtime_target, runtime_target);
        assert!(ats_certification_hex64(&fixture.manifest_sha256));
        assert!(ats_certification_hex64(&fixture.activation_sha256));
        let active = resolve_active_ats_certification(
            &pool,
            &fixture.target_evidence.canonical_url,
            Some(&fixture.runtime_target.runtime_sha256),
            Some(&fixture.surface),
            TEST_NOW_MS,
        )
        .unwrap()
        .expect("installed authority must resolve at the caller clock");
        assert_eq!(active.manifest_sha256, fixture.manifest_sha256);
        assert_eq!(active.activation_sha256, fixture.activation_sha256);
    }

    #[test]
    fn signed_fixture_installer_advances_an_existing_exact_scope_head() {
        let pool = test_pool();
        let runtime_target = test_manifest("0".repeat(64), "9".repeat(64), &"8".repeat(64))
            .runtime_targets
            .remove(0);
        let first = install_signed_ats_authority_fixture(
            &pool,
            test_target_evidence(),
            test_surface(),
            runtime_target.clone(),
            TEST_NOW_MS,
        )
        .unwrap();
        let mut successor_runtime = runtime_target;
        successor_runtime.runtime_id = "local:fixture-successor".to_string();
        successor_runtime.runtime_sha256 = ats_certification_sha256(b"fixture-successor-runtime");
        successor_runtime.automation_bundle_sha256 =
            ats_certification_sha256(b"fixture-successor-automation");
        let successor = install_signed_ats_authority_fixture(
            &pool,
            test_target_evidence(),
            test_surface(),
            successor_runtime.clone(),
            TEST_NOW_MS + 1,
        )
        .unwrap();
        assert_ne!(successor.activation_sha256, first.activation_sha256);
        let active = resolve_active_ats_certification(
            &pool,
            &successor.target_evidence.canonical_url,
            Some(&successor_runtime.runtime_sha256),
            Some(&successor.surface),
            TEST_NOW_MS + 1,
        )
        .unwrap()
        .expect("successor authority must own the exact scope head");
        assert_eq!(active.activation_generation, 2);
        assert_eq!(active.activation_sha256, successor.activation_sha256);
        assert_eq!(active.runtime_targets, vec![successor_runtime]);
    }

    #[test]
    fn signed_fixture_installer_caps_successor_lifetime_to_current_policy() {
        let pool = test_pool();
        let runtime_target = test_manifest("0".repeat(64), "9".repeat(64), &"8".repeat(64))
            .runtime_targets
            .remove(0);
        install_signed_ats_authority_fixture(
            &pool,
            test_target_evidence(),
            test_surface(),
            runtime_target.clone(),
            TEST_NOW_MS,
        )
        .unwrap();

        let successor_now_ms = TEST_NOW_MS + 1_500_001;
        let mut successor_target_evidence = test_target_evidence();
        successor_target_evidence.discovery_observed_at_ms = successor_now_ms - 100;
        successor_target_evidence.original_source_observed_at_ms = successor_now_ms - 50;
        let successor = install_signed_ats_authority_fixture(
            &pool,
            successor_target_evidence,
            test_surface(),
            runtime_target.clone(),
            successor_now_ms,
        )
        .unwrap();
        let active = resolve_active_ats_certification(
            &pool,
            &successor.target_evidence.canonical_url,
            Some(&runtime_target.runtime_sha256),
            Some(&successor.surface),
            successor_now_ms,
        )
        .unwrap()
        .expect("successor authority must remain bounded by the current policy");
        assert_eq!(active.activation_generation, 2);
        assert_eq!(active.expires_at_ms, TEST_NOW_MS + 2_000_000);
    }

    #[test]
    fn exact_target_parser_rejects_board_wide_and_ambiguous_urls() {
        assert_eq!(
            ats_certification_target_from_url("https://boards.greenhouse.io/acme/jobs/123")
                .unwrap(),
            ("greenhouse".to_string(), "greenhouse:acme:123".to_string())
        );
        assert_eq!(
            ats_certification_target_from_url(
                "https://jobs.eu.lever.co/acme/01234567-89ab-cdef-0123-456789abcdef"
            )
            .unwrap(),
            (
                "lever".to_string(),
                "lever:jobs.eu.lever.co:acme:01234567-89ab-cdef-0123-456789abcdef".to_string(),
            )
        );
        assert!(ats_certification_target_from_url("https://boards.greenhouse.io/acme").is_err());
        assert!(
            ats_certification_target_from_url("https://boards.greenhouse.io/acme//jobs/123")
                .is_err()
        );
    }

    #[test]
    fn manifest_binds_test_time_exact_hosts_and_provider_submit_control() {
        let manifest = test_manifest("0".repeat(64), "9".repeat(64), &"8".repeat(64));
        assert!(validate_ats_manifest_authority(&manifest).is_ok());
        let canonical =
            String::from_utf8(ats_certification_canonical_json(&manifest).unwrap()).unwrap();
        assert!(canonical.contains("\"testedAtMs\""));
        assert!(canonical.contains(
            "\"allowedProviderHosts\":[\"boards.greenhouse.io\",\"job-boards.greenhouse.io\"]"
        ));
        assert!(canonical.contains("\"finalSubmitControlId\":\"greenhouse_submit_application\""));

        let mut future_test = manifest.clone();
        future_test.tested_at_ms = future_test.issued_at_ms + 1;
        assert!(validate_ats_manifest_authority(&future_test).is_err());

        let mut spoofed_host = manifest.clone();
        spoofed_host.allowed_provider_hosts = vec!["boards.greenhouse.io.evil.example".to_string()];
        assert!(validate_ats_manifest_authority(&spoofed_host).is_err());

        let mut reordered_hosts = manifest.clone();
        reordered_hosts.allowed_provider_hosts.reverse();
        assert!(validate_ats_manifest_authority(&reordered_hosts).is_err());

        let mut changed_control = manifest;
        changed_control.final_submit_control_id = "greenhouse_generic_submit".to_string();
        assert!(validate_ats_manifest_authority(&changed_control).is_err());
    }

    #[test]
    fn expired_signed_non_policy_authorities_fail_import_closed() {
        let pool = test_pool();
        let authority = test_authority();
        let policy_sha256 = initialize_test_trust_policy(&pool, &authority);

        let mut expired_evidence = test_evidence("authorized_sandbox", &policy_sha256);
        expired_evidence.evidence_id = "expired-evidence".to_string();
        expired_evidence.expires_at_ms = TEST_NOW_MS;
        let expired_evidence_envelope = envelope(
            &expired_evidence,
            "evidence",
            ATS_CERTIFICATION_EVIDENCE_AUDIENCE,
            expired_evidence.issued_at_ms,
            &authority,
            "authorize-expired-evidence",
        );
        assert!(matches!(
            import_ats_certification_evidence_at(
                &pool,
                &expired_evidence_envelope,
                "certifier",
                TEST_NOW_MS,
            ),
            Err(AtsCertificationAuthorityError::Expired)
        ));

        let evidence = test_evidence("authorized_sandbox", &policy_sha256);
        let evidence_envelope = envelope(
            &evidence,
            "evidence",
            ATS_CERTIFICATION_EVIDENCE_AUDIENCE,
            evidence.issued_at_ms,
            &authority,
            "authorize-current-evidence",
        );
        import_ats_certification_evidence_at(&pool, &evidence_envelope, "certifier", TEST_NOW_MS)
            .unwrap();

        let mut expired_layout = test_layout_observation("authorized_sandbox", &policy_sha256);
        expired_layout.observation_id = "expired-layout".to_string();
        expired_layout.expires_at_ms = TEST_NOW_MS;
        let expired_layout_envelope = envelope(
            &expired_layout,
            "layout_observation",
            ATS_CERTIFICATION_LAYOUT_OBSERVATION_AUDIENCE,
            expired_layout.issued_at_ms,
            &authority,
            "authorize-expired-layout",
        );
        assert!(matches!(
            import_ats_layout_observation_at(
                &pool,
                &expired_layout_envelope,
                "observer",
                TEST_NOW_MS,
            ),
            Err(AtsCertificationAuthorityError::Expired)
        ));

        let layout = test_layout_observation("authorized_sandbox", &policy_sha256);
        let layout_envelope = envelope(
            &layout,
            "layout_observation",
            ATS_CERTIFICATION_LAYOUT_OBSERVATION_AUDIENCE,
            layout.issued_at_ms,
            &authority,
            "authorize-current-layout",
        );
        import_ats_layout_observation_at(&pool, &layout_envelope, "observer", TEST_NOW_MS).unwrap();

        let mut expired_manifest = test_manifest(
            envelope_sha256(&evidence_envelope),
            envelope_sha256(&layout_envelope),
            &policy_sha256,
        );
        expired_manifest.certification_id = "expired-manifest".to_string();
        expired_manifest.expires_at_ms = TEST_NOW_MS;
        let expired_manifest_envelope = envelope(
            &expired_manifest,
            "manifest",
            ATS_CERTIFICATION_MANIFEST_AUDIENCE,
            expired_manifest.issued_at_ms,
            &authority,
            "authorize-expired-manifest",
        );
        assert!(matches!(
            import_ats_certification_manifest_at(
                &pool,
                &expired_manifest_envelope,
                "certifier",
                TEST_NOW_MS,
            ),
            Err(AtsCertificationAuthorityError::Expired)
        ));

        let manifest = test_manifest(
            envelope_sha256(&evidence_envelope),
            envelope_sha256(&layout_envelope),
            &policy_sha256,
        );
        let manifest_envelope = envelope(
            &manifest,
            "manifest",
            ATS_CERTIFICATION_MANIFEST_AUDIENCE,
            manifest.issued_at_ms,
            &authority,
            "authorize-current-manifest",
        );
        import_ats_certification_manifest_at(&pool, &manifest_envelope, "certifier", TEST_NOW_MS)
            .unwrap();

        let mut expired_activation = test_activation(
            envelope_sha256(&manifest_envelope),
            manifest.scope_sha256,
            &policy_sha256,
        );
        expired_activation.activation_id = "expired-activation".to_string();
        expired_activation.expires_at_ms = TEST_NOW_MS;
        let expired_activation_envelope = envelope(
            &expired_activation,
            "activation",
            ATS_CERTIFICATION_ACTIVATION_AUDIENCE,
            expired_activation.issued_at_ms,
            &authority,
            "authorize-expired-activation",
        );
        assert!(matches!(
            import_ats_certification_activation_at(
                &pool,
                &expired_activation_envelope,
                "promoter",
                TEST_NOW_MS,
            ),
            Err(AtsCertificationAuthorityError::Expired)
        ));
    }

    #[test]
    fn layout_observation_rejects_otp_token_cookie_and_credential_shaped_categories() {
        let policy_sha256 = "8".repeat(64);
        let valid = test_layout_observation("authorized_sandbox", &policy_sha256);
        assert!(validate_ats_layout_observation_privacy(&valid).is_ok());
        let mut shared_vector_category = valid.clone();
        shared_vector_category.confirmation_state_categories = vec!["provider_success".to_string()];
        assert!(validate_ats_layout_observation_privacy(&shared_vector_category).is_ok());

        for unsafe_category in [
            "123456",
            "bearer:0123456789abcdef0123456789abcdef0123456789abcdef",
            "session_cookie",
            "client_secret",
            "credential",
        ] {
            let mut observation = valid.clone();
            observation.challenge_categories = vec![unsafe_category.to_string()];
            assert!(
                validate_ats_layout_observation_privacy(&observation).is_err(),
                "challenge category {unsafe_category} must fail closed",
            );

            observation = valid.clone();
            observation.confirmation_state_categories = vec![unsafe_category.to_string()];
            assert!(
                validate_ats_layout_observation_privacy(&observation).is_err(),
                "confirmation category {unsafe_category} must fail closed",
            );
        }
    }

    #[test]
    fn authority_envelopes_reject_complete_canonical_negative_matrix() {
        let authority = test_authority();
        let evidence = test_evidence("authorized_sandbox", &"8".repeat(64));
        let valid = envelope(
            &evidence,
            "evidence",
            ATS_CERTIFICATION_EVIDENCE_AUDIENCE,
            evidence.issued_at_ms,
            &authority,
            "authorize-canonical-matrix",
        );
        assert!(verify_ats_evidence_envelope(&valid, &authority.anchor, TEST_NOW_MS).is_ok());

        let canonical = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(&valid.canonical_base64url)
            .unwrap();
        let canonical_text = String::from_utf8(canonical.clone()).unwrap();
        for malformed in [
            canonical_text.replacen('{', "{\"unexpected\":true,", 1),
            canonical_text.replacen('{', "{\"version\":1,", 1),
            format!(" {canonical_text}"),
            canonical_text.trim_end().to_string(),
        ] {
            let mut envelope = valid.clone();
            envelope.canonical_base64url =
                base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(malformed.as_bytes());
            assert!(
                verify_ats_evidence_envelope(&envelope, &authority.anchor, TEST_NOW_MS).is_err()
            );
        }

        let mut invalid_authority_encoding = valid.clone();
        invalid_authority_encoding.canonical_base64url.push('=');
        assert!(matches!(
            verify_ats_evidence_envelope(
                &invalid_authority_encoding,
                &authority.anchor,
                TEST_NOW_MS,
            ),
            Err(AtsCertificationAuthorityError::InvalidEnvelope)
        ));
        let mut invalid_authorization_encoding = valid.clone();
        invalid_authorization_encoding.authorization_base64url = "%not-base64url".to_string();
        assert!(matches!(
            verify_ats_evidence_envelope(
                &invalid_authorization_encoding,
                &authority.anchor,
                TEST_NOW_MS,
            ),
            Err(AtsCertificationAuthorityError::InvalidEnvelope)
        ));

        let second_key = signing_key(99);
        let mut threshold_anchor = authority.anchor.clone();
        let evidence_role = threshold_anchor.roles.get_mut("evidence").unwrap();
        evidence_role.threshold = 2;
        evidence_role.keys.insert(
            "evidence-key-v2".to_string(),
            base64::engine::general_purpose::URL_SAFE_NO_PAD
                .encode(second_key.verifying_key().as_bytes()),
        );
        assert!(matches!(
            verify_ats_evidence_envelope(&valid, &threshold_anchor, TEST_NOW_MS),
            Err(AtsCertificationAuthorityError::SignatureThresholdNotMet)
        ));

        let mut reused_key_anchor = authority.anchor.clone();
        let reused_public_key = reused_key_anchor.roles["evidence"]
            .keys
            .values()
            .next()
            .unwrap()
            .clone();
        reused_key_anchor
            .roles
            .get_mut("manifest")
            .unwrap()
            .keys
            .insert("manifest-reused-key".to_string(), reused_public_key);
        assert!(matches!(
            validate_ats_certification_trust_anchor(&reused_key_anchor),
            Err(AtsCertificationAuthorityError::InvalidTrustAnchor)
        ));

        let mut future = evidence;
        future.issued_at_ms = TEST_NOW_MS + 1;
        future.expires_at_ms = TEST_NOW_MS + 10_000;
        let future_envelope = envelope(
            &future,
            "evidence",
            ATS_CERTIFICATION_EVIDENCE_AUDIENCE,
            future.issued_at_ms,
            &authority,
            "authorize-future-evidence",
        );
        assert!(
            verify_ats_evidence_envelope(&future_envelope, &authority.anchor, TEST_NOW_MS).is_err()
        );

        let mut manifest = test_manifest("0".repeat(64), "9".repeat(64), &"8".repeat(64));
        manifest.manifest_generation = 2;
        manifest.predecessor_manifest_sha256 = Some("1".repeat(64));
        assert!(matches!(
            require_ats_manifest_predecessor(&manifest, None),
            Err(AtsCertificationAuthorityError::SequenceRegression)
        ));
        let mut activation = test_activation(
            "2".repeat(64),
            manifest.scope_sha256.clone(),
            &"8".repeat(64),
        );
        activation.activation_generation = 2;
        activation.channel_sequence = 2;
        activation.predecessor_activation_sha256 = Some("3".repeat(64));
        assert!(matches!(
            require_ats_activation_predecessor(&activation, None),
            Err(AtsCertificationAuthorityError::SequenceRegression)
        ));
        let mut layout = test_layout_observation("authorized_sandbox", &"8".repeat(64));
        layout.predecessor_observation_sha256 = Some("4".repeat(64));
        assert!(matches!(
            require_ats_layout_predecessor(&layout, None),
            Err(AtsCertificationAuthorityError::SequenceRegression)
        ));
    }

    #[test]
    fn authority_roles_are_not_interchangeable() {
        let authority = test_authority();
        let policy_sha256 = "8".repeat(64);
        let activation = test_activation("1".repeat(64), "2".repeat(64), &policy_sha256);
        let manifest_signed_activation = envelope(
            &activation,
            "manifest",
            ATS_CERTIFICATION_ACTIVATION_AUDIENCE,
            activation.issued_at_ms,
            &authority,
            "manifest-cannot-activate",
        );
        assert!(verify_ats_activation_envelope(
            &manifest_signed_activation,
            &authority.anchor,
            TEST_NOW_MS,
        )
        .is_err());

        let evidence = test_evidence("authorized_sandbox", &policy_sha256);
        let activation_signed_evidence = envelope(
            &evidence,
            "activation",
            ATS_CERTIFICATION_EVIDENCE_AUDIENCE,
            evidence.issued_at_ms,
            &authority,
            "activation-cannot-create-evidence",
        );
        assert!(verify_ats_evidence_envelope(
            &activation_signed_evidence,
            &authority.anchor,
            TEST_NOW_MS,
        )
        .is_err());

        let observation_signed_activation = envelope(
            &activation,
            "layout_observation",
            ATS_CERTIFICATION_ACTIVATION_AUDIENCE,
            activation.issued_at_ms,
            &authority,
            "observation-cannot-promote",
        );
        assert!(verify_ats_activation_envelope(
            &observation_signed_activation,
            &authority.anchor,
            TEST_NOW_MS,
        )
        .is_err());

        let revocation = AtsCertificationRevocationAuthority {
            version: 1,
            audience: ATS_CERTIFICATION_REVOCATION_AUDIENCE.to_string(),
            revocation_id: "wrong-role-revocation".to_string(),
            policy_sha256,
            revocation_generation: 1,
            predecessor_revocation_sha256: None,
            subject_kind: "manifest".to_string(),
            subject_id: "manifest-1".to_string(),
            subject_sha256: "3".repeat(64),
            reason_ref: "incident".to_string(),
            issued_at_ms: TEST_NOW_MS - 100,
            effective_at_ms: TEST_NOW_MS,
        };
        let activation_signed_revocation = envelope(
            &revocation,
            "activation",
            ATS_CERTIFICATION_REVOCATION_AUDIENCE,
            revocation.issued_at_ms,
            &authority,
            "only-incident-authority-can-revoke",
        );
        assert!(verify_ats_revocation_envelope(
            &activation_signed_revocation,
            &authority.anchor,
            TEST_NOW_MS,
        )
        .is_err());
    }

    #[test]
    fn fresh_posting_evidence_rejects_hash_expiry_source_and_redirect_mutations() {
        let posting = test_posting();
        let evidence =
            ats_certification_fresh_target_evidence_from_posting(&posting, TEST_NOW_MS).unwrap();
        assert_eq!(evidence, test_target_evidence());

        let mut boundary = posting.clone();
        boundary.last_verified_at_ms =
            Some(TEST_NOW_MS - ATS_CERTIFICATION_TARGET_EVIDENCE_FRESHNESS_MS);
        boundary.discovery_evidence.original_source_checked_at_ms =
            Some(TEST_NOW_MS - ATS_CERTIFICATION_TARGET_EVIDENCE_FRESHNESS_MS);
        assert!(
            ats_certification_fresh_target_evidence_from_posting(&boundary, TEST_NOW_MS).is_ok()
        );
        boundary.last_verified_at_ms =
            Some(TEST_NOW_MS - ATS_CERTIFICATION_TARGET_EVIDENCE_FRESHNESS_MS - 1);
        assert!(matches!(
            ats_certification_fresh_target_evidence_from_posting(&boundary, TEST_NOW_MS),
            Err(AtsCertificationAuthorityError::InvalidAuthority)
        ));

        let mut expired_snapshot = posting.clone();
        expired_snapshot
            .discovery_evidence
            .original_source_snapshot_expires_at_ms = Some(TEST_NOW_MS);
        assert!(matches!(
            ats_certification_fresh_target_evidence_from_posting(&expired_snapshot, TEST_NOW_MS),
            Err(AtsCertificationAuthorityError::ScopeMismatch)
        ));

        let mut uppercase_hash = posting.clone();
        uppercase_hash
            .discovery_evidence
            .original_source_evidence_hash = Some("A".repeat(64));
        assert!(
            ats_certification_fresh_target_evidence_from_posting(&uppercase_hash, TEST_NOW_MS)
                .is_err()
        );

        let mut source_suffix = posting.clone();
        source_suffix.source = "greenhouse-production".to_string();
        assert!(
            ats_certification_fresh_target_evidence_from_posting(&source_suffix, TEST_NOW_MS)
                .is_err()
        );

        let mut redirected_external_id = posting;
        redirected_external_id.external_id =
            "https://job-boards.greenhouse.io/acme/jobs/123".to_string();
        assert!(ats_certification_fresh_target_evidence_from_posting(
            &redirected_external_id,
            TEST_NOW_MS
        )
        .is_err());
    }

    #[test]
    fn imports_replay_exactly_and_conflicting_identity_fails() {
        let pool = test_pool();
        let authority = test_authority();
        let policy_sha256 = initialize_test_trust_policy(&pool, &authority);
        let evidence = test_evidence("authorized_sandbox", &policy_sha256);
        let first = envelope(
            &evidence,
            "evidence",
            ATS_CERTIFICATION_EVIDENCE_AUDIENCE,
            evidence.issued_at_ms,
            &authority,
            "authorize-evidence-replay",
        );
        assert!(
            !import_ats_certification_evidence_at(&pool, &first, "certifier", TEST_NOW_MS,)
                .unwrap()
                .replayed
        );
        assert!(
            import_ats_certification_evidence_at(&pool, &first, "other-certifier", TEST_NOW_MS,)
                .unwrap()
                .replayed
        );

        let mut conflicting = evidence;
        conflicting.object_sha256 = "9".repeat(64);
        let conflicting = envelope(
            &conflicting,
            "evidence",
            ATS_CERTIFICATION_EVIDENCE_AUDIENCE,
            conflicting.issued_at_ms,
            &authority,
            "authorize-evidence-conflict",
        );
        assert!(matches!(
            import_ats_certification_evidence_at(&pool, &conflicting, "certifier", TEST_NOW_MS,),
            Err(AtsCertificationAuthorityError::IdentityConflict)
        ));
    }

    #[test]
    fn production_activation_requires_sandbox_and_live_layouts_for_general_and_canary() {
        for channel in ["general", "canary"] {
            for present_layout_class in ["authorized_live", "authorized_sandbox"] {
                let suffix = format!("{channel}-{present_layout_class}");
                let pool = test_pool();
                let authority = test_authority();
                let policy_sha256 = initialize_test_trust_policy(&pool, &authority);
                let evidence_sha256 =
                    import_test_evidence_object(&pool, &authority, &policy_sha256, &suffix);
                let layout_sha256 = import_test_layout_observation(
                    &pool,
                    &authority,
                    &policy_sha256,
                    &"1".repeat(64),
                    present_layout_class,
                    &suffix,
                );
                let mut manifest = test_manifest(evidence_sha256, layout_sha256, &policy_sha256);
                manifest.certification_id = format!("production-layout-negative-{suffix}");
                let manifest_envelope = envelope(
                    &manifest,
                    "manifest",
                    ATS_CERTIFICATION_MANIFEST_AUDIENCE,
                    manifest.issued_at_ms,
                    &authority,
                    &format!("authorize-production-layout-negative-{suffix}"),
                );
                import_ats_certification_manifest_at(
                    &pool,
                    &manifest_envelope,
                    "certifier",
                    TEST_NOW_MS,
                )
                .unwrap();
                let mut activation = test_activation(
                    envelope_sha256(&manifest_envelope),
                    manifest.scope_sha256.clone(),
                    &policy_sha256,
                );
                activation.activation_id = format!("production-layout-negative-{suffix}");
                activation.approval_ref = format!("production-layout-negative-{suffix}");
                if channel == "canary" {
                    configure_test_canary_activation(
                        &pool,
                        &mut activation,
                        &suffix,
                        vec!["account-1".to_string()],
                        (1, 1, 1, 1),
                    );
                }
                let activation_envelope = envelope(
                    &activation,
                    "activation",
                    ATS_CERTIFICATION_ACTIVATION_AUDIENCE,
                    activation.issued_at_ms,
                    &authority,
                    &format!("authorize-production-activation-negative-{suffix}"),
                );
                assert!(matches!(
                    import_ats_certification_activation_at(
                        &pool,
                        &activation_envelope,
                        "promoter",
                        TEST_NOW_MS,
                    ),
                    Err(AtsCertificationAuthorityError::ScopeMismatch)
                ));
            }
        }
    }

    #[test]
    fn production_manifest_rejects_each_missing_required_check_class() {
        for missing_class in ["authorized_live", "authorized_sandbox"] {
            let suffix = format!("missing-check-{missing_class}");
            let pool = test_pool();
            let authority = test_authority();
            let policy_sha256 = initialize_test_trust_policy(&pool, &authority);
            let evidence_sha256 =
                import_test_evidence_object(&pool, &authority, &policy_sha256, &suffix);
            let mut layout_sha256s = ["authorized_live", "authorized_sandbox"]
                .into_iter()
                .map(|evidence_class| {
                    import_test_layout_observation(
                        &pool,
                        &authority,
                        &policy_sha256,
                        &"1".repeat(64),
                        evidence_class,
                        &format!("{suffix}-{evidence_class}"),
                    )
                })
                .collect::<Vec<_>>();
            layout_sha256s.sort_unstable();
            let mut manifest =
                test_manifest(evidence_sha256, layout_sha256s[0].clone(), &policy_sha256);
            manifest.certification_id = format!("production-{suffix}");
            manifest.certification_profile.layout_observation_sha256s = layout_sha256s;
            manifest.certification_profile.layout_set_sha256 = ats_certification_layout_set_sha256(
                &manifest.certification_profile.layout_observation_sha256s,
            )
            .unwrap();
            manifest
                .certification_profile
                .check_results
                .retain(|result| result.evidence_class != missing_class);
            let manifest_envelope = envelope(
                &manifest,
                "manifest",
                ATS_CERTIFICATION_MANIFEST_AUDIENCE,
                manifest.issued_at_ms,
                &authority,
                &format!("authorize-production-{suffix}"),
            );
            assert!(matches!(
                import_ats_certification_manifest_at(
                    &pool,
                    &manifest_envelope,
                    "certifier",
                    TEST_NOW_MS,
                ),
                Err(AtsCertificationAuthorityError::InvalidAuthority)
            ));
        }
    }

    #[test]
    fn one_class_policy_cannot_weaken_general_or_canary_production_evidence() {
        for channel in ["general", "canary"] {
            for required_class in ["authorized_live", "authorized_sandbox"] {
                let suffix = format!("{channel}-{required_class}");
                let pool = test_pool();
                let authority = test_authority();
                let mut policy = test_trust_policy(&authority, 1, None);
                policy.certification_requirements.required_evidence_classes =
                    vec![required_class.to_string()];
                let policy_envelope = envelope(
                    &policy,
                    "root",
                    ATS_CERTIFICATION_TRUST_POLICY_AUDIENCE,
                    policy.issued_at_ms,
                    &authority,
                    &format!("authorize-one-class-policy-{suffix}"),
                );
                let policy_sha256 = envelope_sha256(&policy_envelope);
                import_ats_certification_trust_policy_with_root_at(
                    &pool,
                    &policy_envelope,
                    &authority.root_anchor,
                    "root-operator",
                    TEST_NOW_MS,
                )
                .unwrap();
                let evidence_sha256 =
                    import_test_evidence_object(&pool, &authority, &policy_sha256, &suffix);
                let layout_sha256 = import_test_layout_observation(
                    &pool,
                    &authority,
                    &policy_sha256,
                    &"1".repeat(64),
                    required_class,
                    &suffix,
                );
                let mut manifest = test_manifest(evidence_sha256, layout_sha256, &policy_sha256);
                manifest.certification_id = format!("one-class-policy-{suffix}");
                manifest.certification_profile.check_results =
                    test_check_results(&manifest.runtime_targets, &[required_class]);
                let manifest_envelope = envelope(
                    &manifest,
                    "manifest",
                    ATS_CERTIFICATION_MANIFEST_AUDIENCE,
                    manifest.issued_at_ms,
                    &authority,
                    &format!("authorize-one-class-manifest-{suffix}"),
                );
                import_ats_certification_manifest_at(
                    &pool,
                    &manifest_envelope,
                    "certifier",
                    TEST_NOW_MS,
                )
                .unwrap();
                let mut activation = test_activation(
                    envelope_sha256(&manifest_envelope),
                    manifest.scope_sha256.clone(),
                    &policy_sha256,
                );
                activation.activation_id = format!("one-class-policy-{suffix}");
                activation.approval_ref = format!("one-class-policy-{suffix}");
                if channel == "canary" {
                    configure_test_canary_activation(
                        &pool,
                        &mut activation,
                        &suffix,
                        vec!["account-1".to_string()],
                        (1, 1, 1, 1),
                    );
                }
                let activation_envelope = envelope(
                    &activation,
                    "activation",
                    ATS_CERTIFICATION_ACTIVATION_AUDIENCE,
                    activation.issued_at_ms,
                    &authority,
                    &format!("authorize-one-class-activation-{suffix}"),
                );
                assert!(matches!(
                    import_ats_certification_activation_at(
                        &pool,
                        &activation_envelope,
                        "promoter",
                        TEST_NOW_MS,
                    ),
                    Err(AtsCertificationAuthorityError::ScopeMismatch)
                ));
            }
        }
    }

    #[test]
    fn two_runtime_manifest_requires_each_live_and_sandbox_check_and_layout_pair() {
        for missing_pair_kind in ["check", "layout"] {
            for missing_class in ["authorized_live", "authorized_sandbox"] {
                let suffix = format!("two-runtime-{missing_pair_kind}-{missing_class}");
                let pool = test_pool();
                let authority = test_authority();
                let policy_sha256 = initialize_test_trust_policy(&pool, &authority);
                let evidence_sha256 =
                    import_test_evidence_object(&pool, &authority, &policy_sha256, &suffix);
                let mut manifest = test_manifest(evidence_sha256, "0".repeat(64), &policy_sha256);
                manifest.certification_id = suffix.clone();
                let mut second_runtime = manifest.runtime_targets[0].clone();
                second_runtime.runtime_id = "local:browser-release-2".to_string();
                second_runtime.runtime_sha256 = "2".repeat(64);
                manifest.runtime_targets.push(second_runtime);
                let missing_runner_sha256 = manifest.runtime_targets[1].runtime_sha256.clone();

                let mut layout_sha256s = Vec::new();
                for (runtime_index, runtime) in manifest.runtime_targets.iter().enumerate() {
                    for evidence_class in ["authorized_live", "authorized_sandbox"] {
                        if missing_pair_kind == "layout"
                            && runtime_index == 1
                            && evidence_class == missing_class
                        {
                            continue;
                        }
                        layout_sha256s.push(import_test_layout_observation(
                            &pool,
                            &authority,
                            &policy_sha256,
                            &runtime.runtime_sha256,
                            evidence_class,
                            &format!("{suffix}-{runtime_index}-{evidence_class}"),
                        ));
                    }
                }
                layout_sha256s.sort_unstable();
                manifest.certification_profile.layout_observation_sha256s = layout_sha256s;
                manifest.certification_profile.layout_set_sha256 =
                    ats_certification_layout_set_sha256(
                        &manifest.certification_profile.layout_observation_sha256s,
                    )
                    .unwrap();
                manifest.certification_profile.check_results = test_check_results(
                    &manifest.runtime_targets,
                    &["authorized_live", "authorized_sandbox"],
                );
                if missing_pair_kind == "check" {
                    manifest
                        .certification_profile
                        .check_results
                        .retain(|result| {
                            result.runner_target_sha256 != missing_runner_sha256
                                || result.evidence_class != missing_class
                        });
                }
                let manifest_envelope = envelope(
                    &manifest,
                    "manifest",
                    ATS_CERTIFICATION_MANIFEST_AUDIENCE,
                    manifest.issued_at_ms,
                    &authority,
                    &format!("authorize-{suffix}"),
                );
                if missing_pair_kind == "check" {
                    assert!(matches!(
                        import_ats_certification_manifest_at(
                            &pool,
                            &manifest_envelope,
                            "certifier",
                            TEST_NOW_MS,
                        ),
                        Err(AtsCertificationAuthorityError::InvalidAuthority)
                    ));
                    continue;
                }

                import_ats_certification_manifest_at(
                    &pool,
                    &manifest_envelope,
                    "certifier",
                    TEST_NOW_MS,
                )
                .unwrap();
                let mut activation = test_activation(
                    envelope_sha256(&manifest_envelope),
                    manifest.scope_sha256.clone(),
                    &policy_sha256,
                );
                activation.activation_id = format!("activation-{suffix}");
                activation.approval_ref = format!("approval-{suffix}");
                let activation_envelope = envelope(
                    &activation,
                    "activation",
                    ATS_CERTIFICATION_ACTIVATION_AUDIENCE,
                    activation.issued_at_ms,
                    &authority,
                    &format!("authorize-activation-{suffix}"),
                );
                assert!(matches!(
                    import_ats_certification_activation_at(
                        &pool,
                        &activation_envelope,
                        "promoter",
                        TEST_NOW_MS,
                    ),
                    Err(AtsCertificationAuthorityError::ScopeMismatch)
                ));
            }
        }
    }

    #[test]
    fn general_activation_resolves_exact_runtime_and_validates_in_transaction() {
        let fixture = imported_fixture();
        let binding = resolve_active_ats_certification(
            &fixture.pool,
            "https://boards.greenhouse.io/acme/jobs/123",
            Some(&"1".repeat(64)),
            Some(&test_surface()),
            TEST_NOW_MS,
        )
        .unwrap()
        .unwrap();
        assert_eq!(binding.target_key, "greenhouse:acme:123");
        assert_eq!(binding.activation_sha256, fixture.activation_sha256);
        assert_eq!(binding.channel_head_revision, 1);
        assert_eq!(binding.capability, "unattended_submit");
        assert_eq!(binding.last_verified_at_ms, fixture.manifest.tested_at_ms);
        assert_eq!(
            binding.allowed_provider_hosts,
            ATS_GREENHOUSE_ALLOWED_PROVIDER_HOSTS.map(str::to_string)
        );
        assert_eq!(
            binding.final_submit_control_id,
            ATS_GREENHOUSE_FINAL_SUBMIT_CONTROL_ID
        );
        validate_frozen_ats_certification(
            &fixture.pool,
            &binding,
            "https://boards.greenhouse.io/acme/jobs/123",
            &"1".repeat(64),
            &test_surface(),
            TEST_NOW_MS,
        )
        .unwrap();
        assert!(resolve_active_ats_certification(
            &fixture.pool,
            "https://boards.greenhouse.io/acme/jobs/124",
            Some(&"1".repeat(64)),
            Some(&test_surface()),
            TEST_NOW_MS,
        )
        .unwrap()
        .is_none());
        assert!(validate_frozen_ats_certification(
            &fixture.pool,
            &binding,
            "https://boards.greenhouse.io/acme/jobs/124",
            &"1".repeat(64),
            &test_surface(),
            TEST_NOW_MS,
        )
        .is_err());

        let replay = apply_ats_certification_activation_at(
            &fixture.pool,
            &fixture.activation_sha256,
            0,
            None,
            "promoter",
            TEST_NOW_MS + 1,
        )
        .unwrap();
        assert!(replay.replayed);
        assert_eq!(replay.transition_sha256, fixture.head.transition_sha256);
    }

    #[test]
    fn sqlite_sequence_two_activation_replay_requires_immutable_predecessor_transition() {
        let suffix = uuid::Uuid::new_v4().simple().to_string();
        let fixture = isolated_postgres_fixture(test_pool(), &suffix, 1, true);
        assert_sequence_two_activation_replay_requires_exact_predecessor(&fixture, &suffix);
    }

    #[test]
    fn sqlite_sequence_two_quarantine_replay_requires_exact_predecessor_command() {
        let suffix = uuid::Uuid::new_v4().simple().to_string();
        let fixture = isolated_postgres_fixture(test_pool(), &suffix, 1, true);
        assert_sequence_two_quarantine_replay_requires_exact_predecessor(&fixture, &suffix);
    }

    #[test]
    fn runtime_attestation_requires_one_complete_signed_tuple() {
        let fixture = imported_fixture();
        let mut binding = resolve_active_ats_certification(
            &fixture.pool,
            "https://boards.greenhouse.io/acme/jobs/123",
            None,
            Some(&test_surface()),
            TEST_NOW_MS,
        )
        .unwrap()
        .unwrap();
        let attestation = local_runtime_attestation();
        assert_eq!(
            ats_certification_runtime_target_from_attestation(&binding, &attestation)
                .unwrap()
                .runtime_sha256,
            "1".repeat(64)
        );

        let mut changed = attestation.clone();
        let AtsCertificationRuntimeAttestation::Local {
            automation_bundle_sha256,
            ..
        } = &mut changed
        else {
            unreachable!();
        };
        *automation_bundle_sha256 = "9".repeat(64);
        assert!(matches!(
            ats_certification_runtime_target_from_attestation(&binding, &changed),
            Err(AtsCertificationAuthorityError::RuntimeMismatch)
        ));

        let mut duplicate = binding.runtime_targets[0].clone();
        duplicate.runtime_id = "local:browser-release-duplicate".to_string();
        duplicate.runtime_sha256 = "a".repeat(64);
        binding.runtime_targets.push(duplicate);
        assert!(matches!(
            ats_certification_runtime_target_from_attestation(&binding, &attestation),
            Err(AtsCertificationAuthorityError::RuntimeMismatch)
        ));
        binding.runtime_targets.clear();
        assert!(matches!(
            ats_certification_runtime_target_from_attestation(&binding, &attestation),
            Err(AtsCertificationAuthorityError::RuntimeMismatch)
        ));
    }

    #[test]
    fn synthetic_evidence_cannot_import_general_activation() {
        let pool = test_pool();
        let authority = test_authority();
        let policy_sha256 = initialize_test_trust_policy(&pool, &authority);
        let evidence = test_evidence("synthetic", &policy_sha256);
        let evidence_envelope = envelope(
            &evidence,
            "evidence",
            ATS_CERTIFICATION_EVIDENCE_AUDIENCE,
            evidence.issued_at_ms,
            &authority,
            "authorize-synthetic",
        );
        import_ats_certification_evidence_at(&pool, &evidence_envelope, "certifier", TEST_NOW_MS)
            .unwrap();
        let layout = test_layout_observation("authorized_sandbox", &policy_sha256);
        let layout_envelope = envelope(
            &layout,
            "layout_observation",
            ATS_CERTIFICATION_LAYOUT_OBSERVATION_AUDIENCE,
            layout.issued_at_ms,
            &authority,
            "authorize-synthetic-layout",
        );
        import_ats_layout_observation_at(&pool, &layout_envelope, "observer", TEST_NOW_MS).unwrap();
        let manifest = test_manifest(
            envelope_sha256(&evidence_envelope),
            envelope_sha256(&layout_envelope),
            &policy_sha256,
        );
        let manifest_envelope = envelope(
            &manifest,
            "manifest",
            ATS_CERTIFICATION_MANIFEST_AUDIENCE,
            manifest.issued_at_ms,
            &authority,
            "authorize-synthetic-manifest",
        );
        import_ats_certification_manifest_at(&pool, &manifest_envelope, "certifier", TEST_NOW_MS)
            .unwrap();
        let activation = test_activation(
            envelope_sha256(&manifest_envelope),
            manifest.scope_sha256.clone(),
            &policy_sha256,
        );
        let activation_envelope = envelope(
            &activation,
            "activation",
            ATS_CERTIFICATION_ACTIVATION_AUDIENCE,
            activation.issued_at_ms,
            &authority,
            "authorize-synthetic-activation",
        );
        assert!(matches!(
            import_ats_certification_activation_at(
                &pool,
                &activation_envelope,
                "promoter",
                TEST_NOW_MS,
            ),
            Err(AtsCertificationAuthorityError::SyntheticEvidence)
        ));
    }

    #[test]
    fn synthetic_layout_observation_is_shadow_only_and_cannot_activate_production() {
        let pool = test_pool();
        let authority = test_authority();
        let mut policy = test_trust_policy(&authority, 1, None);
        policy.certification_requirements.required_evidence_classes =
            vec!["authorized_sandbox".to_string(), "synthetic".to_string()];
        let policy_envelope = envelope(
            &policy,
            "root",
            ATS_CERTIFICATION_TRUST_POLICY_AUDIENCE,
            policy.issued_at_ms,
            &authority,
            "authorize-synthetic-layout-policy",
        );
        let policy_sha256 = envelope_sha256(&policy_envelope);
        import_ats_certification_trust_policy_with_root_at(
            &pool,
            &policy_envelope,
            &authority.root_anchor,
            "root-operator",
            TEST_NOW_MS,
        )
        .unwrap();

        let evidence = test_evidence("authorized_sandbox", &policy_sha256);
        let evidence_envelope = envelope(
            &evidence,
            "evidence",
            ATS_CERTIFICATION_EVIDENCE_AUDIENCE,
            evidence.issued_at_ms,
            &authority,
            "authorize-production-evidence-for-synthetic-layout",
        );
        import_ats_certification_evidence_at(&pool, &evidence_envelope, "certifier", TEST_NOW_MS)
            .unwrap();

        let layout = test_layout_observation("synthetic", &policy_sha256);
        assert!(validate_ats_layout_observation_privacy(&layout).is_ok());
        let layout_envelope = envelope(
            &layout,
            "layout_observation",
            ATS_CERTIFICATION_LAYOUT_OBSERVATION_AUDIENCE,
            layout.issued_at_ms,
            &authority,
            "authorize-synthetic-layout-observation",
        );
        import_ats_layout_observation_at(&pool, &layout_envelope, "observer", TEST_NOW_MS).unwrap();

        let mut manifest = test_manifest(
            envelope_sha256(&evidence_envelope),
            envelope_sha256(&layout_envelope),
            &policy_sha256,
        );
        manifest.maximum_capability = "observe_only".to_string();
        manifest.certification_profile.check_results = ATS_CERTIFICATION_REQUIRED_CHECK_IDS
            .iter()
            .flat_map(|check_id| {
                ["authorized_sandbox", "synthetic"].map(|evidence_class| {
                    AtsCertificationCheckResult {
                        runner_target_sha256: "1".repeat(64),
                        check_id: (*check_id).to_string(),
                        evidence_class: evidence_class.to_string(),
                        passed_count: 1,
                        failed_count: 0,
                        skipped_count: 0,
                    }
                })
            })
            .collect();
        let manifest_envelope = envelope(
            &manifest,
            "manifest",
            ATS_CERTIFICATION_MANIFEST_AUDIENCE,
            manifest.issued_at_ms,
            &authority,
            "authorize-synthetic-layout-shadow-manifest",
        );
        import_ats_certification_manifest_at(&pool, &manifest_envelope, "certifier", TEST_NOW_MS)
            .unwrap();
        let manifest_sha256 = envelope_sha256(&manifest_envelope);

        let mut shadow = test_activation(
            manifest_sha256.clone(),
            manifest.scope_sha256.clone(),
            &policy_sha256,
        );
        shadow.activation_id = "synthetic-layout-shadow-activation".to_string();
        shadow.channel = "shadow".to_string();
        shadow.capability = "observe_only".to_string();
        shadow.approval_ref = "shadow-only".to_string();
        let shadow_envelope = envelope(
            &shadow,
            "activation",
            ATS_CERTIFICATION_ACTIVATION_AUDIENCE,
            shadow.issued_at_ms,
            &authority,
            "authorize-synthetic-layout-shadow-activation",
        );
        import_ats_certification_activation_at(&pool, &shadow_envelope, "promoter", TEST_NOW_MS)
            .unwrap();
        apply_ats_certification_activation_at(
            &pool,
            &envelope_sha256(&shadow_envelope),
            0,
            None,
            "promoter",
            TEST_NOW_MS,
        )
        .unwrap();

        let mut general = shadow;
        general.activation_id = "synthetic-layout-general-activation".to_string();
        general.channel = "general".to_string();
        general.approval_ref = "production-denied".to_string();
        let general_envelope = envelope(
            &general,
            "activation",
            ATS_CERTIFICATION_ACTIVATION_AUDIENCE,
            general.issued_at_ms,
            &authority,
            "deny-synthetic-layout-general-activation",
        );
        assert!(matches!(
            import_ats_certification_activation_at(
                &pool,
                &general_envelope,
                "promoter",
                TEST_NOW_MS,
            ),
            Err(AtsCertificationAuthorityError::SyntheticEvidence)
        ));
        assert!(resolve_active_ats_certification(
            &pool,
            &test_target_evidence().canonical_url,
            Some(&"1".repeat(64)),
            Some(&test_surface()),
            TEST_NOW_MS,
        )
        .unwrap()
        .is_none());
    }

    #[test]
    fn canary_activation_requires_manifest_bound_evidence_and_bounded_limits() {
        let mut activation = test_activation("a".repeat(64), "b".repeat(64), &"c".repeat(64));
        activation.channel = "canary".to_string();
        activation.account_allowlist_sha256 = Some("d".repeat(64));
        activation.canary_max_submissions = 8;
        activation.canary_account_cap = 3;
        activation.canary_concurrency_cap = 2;
        activation.canary_daily_side_effect_cap = 4;
        activation.canary_evidence_manifest_sha256 = Some("e".repeat(64));
        assert!(matches!(
            validate_ats_activation_authority(&activation),
            Err(AtsCertificationAuthorityError::InvalidAuthority)
        ));

        activation.canary_evidence_manifest_sha256 = Some(activation.manifest_sha256.clone());
        assert!(validate_ats_activation_authority(&activation).is_ok());

        activation.canary_account_cap = 9;
        assert!(matches!(
            validate_ats_activation_authority(&activation),
            Err(AtsCertificationAuthorityError::InvalidAuthority)
        ));
        activation.canary_account_cap = 3;
        activation.canary_concurrency_cap = 9;
        assert!(matches!(
            validate_ats_activation_authority(&activation),
            Err(AtsCertificationAuthorityError::InvalidAuthority)
        ));
        activation.canary_concurrency_cap = 2;
        activation.canary_daily_side_effect_cap = 9;
        assert!(matches!(
            validate_ats_activation_authority(&activation),
            Err(AtsCertificationAuthorityError::InvalidAuthority)
        ));
    }

    #[test]
    fn canary_capacity_limits_deny_atomically_across_activation_day_account_and_concurrency() {
        assert_canary_capacity_denial_is_atomic((1, 1, 1, 1), Some("submitted"), "account-1");
        assert_canary_capacity_denial_is_atomic((2, 1, 2, 1), None, "account-1");
        assert_canary_capacity_denial_is_atomic((2, 1, 2, 2), None, "account-2");
        assert_canary_capacity_denial_is_atomic((2, 1, 1, 2), None, "account-1");
    }

    #[test]
    fn canary_capacity_rejects_non_utc_period_without_consuming_the_binding() {
        let fixture = imported_canary_fixture(4, 2, 2, 2);
        let binding =
            test_canary_binding_request("capacity-wrong-day", "account-1", "application-1", 'f');
        create_ats_application_certification_binding(&fixture.pool, &binding, TEST_NOW_MS).unwrap();
        let mut phase_b = test_phase_b_request(&binding, "capacity-wrong-day");
        phase_b.period_key = "2033-05-17".to_string();
        assert!(matches!(
            validate_consume_reserve_ats_application_certification(
                &fixture.pool,
                &phase_b,
                TEST_NOW_MS + 1,
            ),
            Err(AtsCertificationAuthorityError::ScopeMismatch)
        ));
        let conn = fixture.pool.get().unwrap();
        let (phase, fence, reservation_count): (String, i64, i64) = conn
            .query_row(
                "SELECT binding.phase, binding.fence,
                        (SELECT COUNT(*)
                           FROM jobs_ats_certification_canary_reservations)
                   FROM jobs_application_ats_certification_bindings binding
                  WHERE binding.binding_id = ?1",
                params![binding.binding_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(
            (phase.as_str(), fence, reservation_count),
            ("preflight", 0, 0)
        );
    }

    #[test]
    fn canary_period_is_one_server_owned_utc_day() {
        let before_midnight = Utc
            .with_ymd_and_hms(2033, 5, 18, 23, 59, 59)
            .single()
            .unwrap()
            .timestamp_millis();
        let after_midnight = before_midnight + 1_000;
        assert_eq!(
            ats_certification_canary_utc_period_key(before_midnight).unwrap(),
            "2033-05-18"
        );
        assert_eq!(
            ats_certification_canary_utc_period_key(after_midnight).unwrap(),
            "2033-05-19"
        );
    }

    #[test]
    fn target_scoped_canary_resolution_tolerates_membership_in_multiple_lists() {
        let fixture = imported_fixture();
        let first_request = AtsCertificationCanaryAllowlistImportRequest {
            schema_version: 1,
            allowlist_id: "allowlist-greenhouse-acme".to_string(),
            account_ids: vec!["account-2".to_string(), "account-1".to_string()],
            approval_ref: "canary-approval-greenhouse".to_string(),
            not_before_ms: TEST_NOW_MS - 700,
            expires_at_ms: TEST_NOW_MS + 900_000,
        };
        let first = import_ats_certification_canary_allowlist(
            &fixture.pool,
            &first_request,
            "canary-operator",
            TEST_NOW_MS,
        )
        .unwrap();
        assert!(!first.replayed);
        let mut reordered = first_request.clone();
        reordered.account_ids.reverse();
        let replay = import_ats_certification_canary_allowlist(
            &fixture.pool,
            &reordered,
            "canary-operator",
            TEST_NOW_MS,
        )
        .unwrap();
        assert!(replay.replayed);
        assert_eq!(replay.allowlist_sha256, first.allowlist_sha256);

        let mut conflicting = first_request.clone();
        conflicting.approval_ref = "different-approval".to_string();
        assert!(matches!(
            import_ats_certification_canary_allowlist(
                &fixture.pool,
                &conflicting,
                "canary-operator",
                TEST_NOW_MS,
            ),
            Err(AtsCertificationAuthorityError::IdentityConflict)
        ));

        let second = import_ats_certification_canary_allowlist(
            &fixture.pool,
            &AtsCertificationCanaryAllowlistImportRequest {
                schema_version: 1,
                allowlist_id: "allowlist-lever-other-target".to_string(),
                account_ids: vec!["account-1".to_string(), "account-3".to_string()],
                approval_ref: "canary-approval-lever".to_string(),
                not_before_ms: TEST_NOW_MS - 700,
                expires_at_ms: TEST_NOW_MS + 900_000,
            },
            "canary-operator",
            TEST_NOW_MS,
        )
        .unwrap();
        assert!(matches!(
            resolve_active_ats_certification_canary_allowlist_for_account(
                &fixture.pool,
                "account-1",
                TEST_NOW_MS,
            ),
            Err(AtsCertificationAuthorityError::IdentityConflict)
        ));

        let mut activation = test_activation(
            fixture.manifest_sha256.clone(),
            fixture.manifest.scope_sha256.clone(),
            &fixture.policy_sha256,
        );
        activation.activation_id = "greenhouse-acme-123-canary-1".to_string();
        activation.channel = "canary".to_string();
        activation.account_allowlist_sha256 = Some(first.allowlist_sha256.clone());
        activation.canary_max_submissions = 1;
        activation.canary_account_cap = 1;
        activation.canary_concurrency_cap = 1;
        activation.canary_daily_side_effect_cap = 1;
        activation.canary_evidence_manifest_sha256 = Some(fixture.manifest_sha256.clone());
        activation.approval_ref = "approval-canary-1".to_string();
        let activation_envelope = envelope(
            &activation,
            "activation",
            ATS_CERTIFICATION_ACTIVATION_AUDIENCE,
            activation.issued_at_ms,
            &fixture.authority,
            "authorize-canary-activation-1",
        );
        import_ats_certification_activation_at(
            &fixture.pool,
            &activation_envelope,
            "promoter",
            TEST_NOW_MS,
        )
        .unwrap();
        apply_ats_certification_activation_at(
            &fixture.pool,
            &envelope_sha256(&activation_envelope),
            0,
            None,
            "promoter",
            TEST_NOW_MS,
        )
        .unwrap();

        let mut conn = fixture.pool.get().unwrap();
        let tx = conn.transaction().unwrap();
        assert_eq!(
            resolve_sqlite_ats_canary_allowlist_for_target_account(
                &tx,
                &test_target_evidence(),
                "account-1",
                TEST_NOW_MS,
            )
            .unwrap(),
            Some(first.allowlist_sha256.clone())
        );
        assert!(sqlite_ats_account_enrolled_in_allowlist(
            &tx,
            "account-1",
            &first.allowlist_sha256,
            TEST_NOW_MS,
        )
        .unwrap());
        tx.commit().unwrap();

        let revocation = AtsCertificationCanaryAllowlistRevocationRequest {
            allowlist_sha256: first.allowlist_sha256.clone(),
            revocation_ref: "incident-canary-list-1".to_string(),
        };
        assert!(
            !revoke_ats_certification_canary_allowlist(
                &fixture.pool,
                &revocation,
                "incident-responder",
                TEST_NOW_MS + 1,
            )
            .unwrap()
            .replayed
        );
        assert!(
            revoke_ats_certification_canary_allowlist(
                &fixture.pool,
                &revocation,
                "incident-responder",
                TEST_NOW_MS + 2,
            )
            .unwrap()
            .replayed
        );

        let mut conn = fixture.pool.get().unwrap();
        let tx = conn.transaction().unwrap();
        assert_eq!(
            resolve_sqlite_ats_canary_allowlist_for_target_account(
                &tx,
                &test_target_evidence(),
                "account-1",
                TEST_NOW_MS + 2,
            )
            .unwrap(),
            None
        );
        tx.commit().unwrap();
        assert_eq!(
            resolve_active_ats_certification_canary_allowlist_for_account(
                &fixture.pool,
                "account-1",
                TEST_NOW_MS + 2,
            )
            .unwrap(),
            Some(second.allowlist_sha256)
        );
    }

    #[test]
    fn revocation_and_quarantine_remove_active_authority() {
        let fixture = imported_fixture();
        let command = AtsCertificationQuarantineAuthority {
            version: 1,
            audience: ATS_CERTIFICATION_QUARANTINE_AUDIENCE.to_string(),
            command_id: "quarantine-activation-1".to_string(),
            policy_sha256: fixture.policy_sha256.clone(),
            command_generation: 1,
            scope_kind: "activation".to_string(),
            scope_id: fixture.activation.activation_id.clone(),
            scope_sha256: fixture.activation_sha256.clone(),
            command_sequence: 1,
            predecessor_command_sha256: None,
            action: "quarantine".to_string(),
            reason_ref: "incident-1".to_string(),
            issued_at_ms: TEST_NOW_MS,
        };
        let command_envelope = envelope(
            &command,
            "revocation",
            ATS_CERTIFICATION_QUARANTINE_AUDIENCE,
            command.issued_at_ms,
            &fixture.authority,
            "authorize-quarantine-1",
        );
        import_ats_certification_quarantine_at(
            &fixture.pool,
            &command_envelope,
            "incident-responder",
            TEST_NOW_MS,
        )
        .unwrap();
        let command_sha256 = envelope_sha256(&command_envelope);
        apply_ats_certification_quarantine_at(
            &fixture.pool,
            &command_sha256,
            0,
            None,
            "incident-responder",
            TEST_NOW_MS,
        )
        .unwrap();
        assert!(resolve_active_ats_certification(
            &fixture.pool,
            "https://boards.greenhouse.io/acme/jobs/123",
            Some(&"1".repeat(64)),
            Some(&test_surface()),
            TEST_NOW_MS + 1,
        )
        .unwrap()
        .is_none());
        let quarantined_status = get_ats_certification_target_status_projection(
            &fixture.pool,
            &test_target_evidence(),
            Some(&"1".repeat(64)),
            "general",
            None,
            TEST_NOW_MS + 1,
        )
        .unwrap();
        assert_eq!(quarantined_status.status, "drifted");
        assert_eq!(
            quarantined_status.last_verified_at_ms,
            Some(fixture.manifest.tested_at_ms)
        );

        let release = AtsCertificationQuarantineAuthority {
            version: 1,
            audience: ATS_CERTIFICATION_QUARANTINE_AUDIENCE.to_string(),
            command_id: "release-activation-2".to_string(),
            policy_sha256: fixture.policy_sha256.clone(),
            command_generation: 2,
            scope_kind: "activation".to_string(),
            scope_id: fixture.activation.activation_id.clone(),
            scope_sha256: fixture.activation_sha256.clone(),
            command_sequence: 2,
            predecessor_command_sha256: Some(command_sha256.clone()),
            action: "release".to_string(),
            reason_ref: "incident-1-resolved".to_string(),
            issued_at_ms: TEST_NOW_MS + 2,
        };
        let release_envelope = envelope(
            &release,
            "revocation",
            ATS_CERTIFICATION_QUARANTINE_AUDIENCE,
            release.issued_at_ms,
            &fixture.authority,
            "authorize-release-2",
        );
        import_ats_certification_quarantine_at(
            &fixture.pool,
            &release_envelope,
            "incident-responder",
            TEST_NOW_MS + 2,
        )
        .unwrap();
        let release_sha256 = envelope_sha256(&release_envelope);
        apply_ats_certification_quarantine_at(
            &fixture.pool,
            &release_sha256,
            1,
            Some(&command_sha256),
            "incident-responder",
            TEST_NOW_MS + 2,
        )
        .unwrap();
        assert!(resolve_active_ats_certification(
            &fixture.pool,
            "https://boards.greenhouse.io/acme/jobs/123",
            Some(&"1".repeat(64)),
            Some(&test_surface()),
            TEST_NOW_MS + 3,
        )
        .unwrap()
        .is_some());

        let revocation = AtsCertificationRevocationAuthority {
            version: 1,
            audience: ATS_CERTIFICATION_REVOCATION_AUDIENCE.to_string(),
            revocation_id: "revoke-manifest-1".to_string(),
            policy_sha256: fixture.policy_sha256.clone(),
            revocation_generation: 1,
            predecessor_revocation_sha256: None,
            subject_kind: "manifest".to_string(),
            subject_id: fixture.manifest.certification_id.clone(),
            subject_sha256: fixture.manifest_sha256.clone(),
            reason_ref: "provider-drift-1".to_string(),
            issued_at_ms: TEST_NOW_MS + 4,
            effective_at_ms: TEST_NOW_MS + 4,
        };
        let revocation_envelope = envelope(
            &revocation,
            "revocation",
            ATS_CERTIFICATION_REVOCATION_AUDIENCE,
            revocation.issued_at_ms,
            &fixture.authority,
            "authorize-revocation-1",
        );
        import_ats_certification_revocation_at(
            &fixture.pool,
            &revocation_envelope,
            "incident-responder",
            TEST_NOW_MS + 4,
        )
        .unwrap();
        assert!(resolve_active_ats_certification(
            &fixture.pool,
            "https://boards.greenhouse.io/acme/jobs/123",
            Some(&"1".repeat(64)),
            Some(&test_surface()),
            TEST_NOW_MS + 5,
        )
        .unwrap()
        .is_none());
        let revoked_status = get_ats_certification_target_status_projection(
            &fixture.pool,
            &test_target_evidence(),
            Some(&"1".repeat(64)),
            "general",
            None,
            TEST_NOW_MS + 5,
        )
        .unwrap();
        assert_eq!(revoked_status.status, "revoked");
        assert_eq!(
            revoked_status.last_verified_at_ms,
            Some(fixture.manifest.tested_at_ms)
        );
    }

    #[test]
    fn root_policy_replays_exactly_and_rejects_role_confusion_and_expiry() {
        let pool = test_pool();
        let authority = test_authority();
        let policy = test_trust_policy(&authority, 1, None);
        let policy_envelope = envelope(
            &policy,
            "root",
            ATS_CERTIFICATION_TRUST_POLICY_AUDIENCE,
            policy.issued_at_ms,
            &authority,
            "authorize-policy-replay-1",
        );
        let first = import_ats_certification_trust_policy_with_root_at(
            &pool,
            &policy_envelope,
            &authority.root_anchor,
            "root-operator",
            TEST_NOW_MS,
        )
        .unwrap();
        assert!(!first.replayed);
        assert!(
            import_ats_certification_trust_policy_with_root_at(
                &pool,
                &policy_envelope,
                &authority.root_anchor,
                "other-root-operator",
                TEST_NOW_MS,
            )
            .unwrap()
            .replayed
        );

        let other_pool = test_pool();
        let role_confused = envelope(
            &policy,
            "evidence",
            ATS_CERTIFICATION_TRUST_POLICY_AUDIENCE,
            policy.issued_at_ms,
            &authority,
            "authorize-policy-with-evidence-key",
        );
        assert!(matches!(
            import_ats_certification_trust_policy_with_root_at(
                &other_pool,
                &role_confused,
                &authority.root_anchor,
                "root-operator",
                TEST_NOW_MS,
            ),
            Err(AtsCertificationAuthorityError::InvalidAuthority)
        ));

        let mut expired = policy;
        expired.policy_id = "expired-trust-policy".to_string();
        expired.expires_at_ms = TEST_NOW_MS;
        let expired_envelope = envelope(
            &expired,
            "root",
            ATS_CERTIFICATION_TRUST_POLICY_AUDIENCE,
            expired.issued_at_ms,
            &authority,
            "authorize-expired-policy",
        );
        assert!(matches!(
            import_ats_certification_trust_policy_with_root_at(
                &other_pool,
                &expired_envelope,
                &authority.root_anchor,
                "root-operator",
                TEST_NOW_MS,
            ),
            Err(AtsCertificationAuthorityError::Expired)
        ));
    }

    #[test]
    fn policy_rotation_fences_old_keys_active_heads_and_cross_policy_evidence() {
        let fixture = imported_fixture();
        let first_policy_sha256 = fixture
            .pool
            .get()
            .unwrap()
            .query_row(
                "SELECT current_policy_sha256 FROM jobs_ats_certification_trust_head
                  WHERE singleton_id = 1",
                [],
                |row| row.get::<_, String>(0),
            )
            .unwrap();
        let rotated = rotated_test_authority(&fixture.authority);
        let policy = test_trust_policy(&rotated, 2, Some(first_policy_sha256));
        let policy_envelope = envelope(
            &policy,
            "root",
            ATS_CERTIFICATION_TRUST_POLICY_AUDIENCE,
            policy.issued_at_ms,
            &rotated,
            "authorize-trust-policy-2",
        );
        let second_policy_sha256 = envelope_sha256(&policy_envelope);
        import_ats_certification_trust_policy_with_root_at(
            &fixture.pool,
            &policy_envelope,
            &rotated.root_anchor,
            "root-operator",
            TEST_NOW_MS,
        )
        .unwrap();

        assert!(resolve_active_ats_certification(
            &fixture.pool,
            "https://boards.greenhouse.io/acme/jobs/123",
            Some(&"1".repeat(64)),
            Some(&test_surface()),
            TEST_NOW_MS,
        )
        .unwrap()
        .is_none());

        let mut old_key_evidence = test_evidence("authorized_canary", &second_policy_sha256);
        old_key_evidence.evidence_id = "old-key-evidence-after-rotation".to_string();
        old_key_evidence.object_key = "ats-certification/old-key-after-rotation.json".to_string();
        let old_key_envelope = envelope(
            &old_key_evidence,
            "evidence",
            ATS_CERTIFICATION_EVIDENCE_AUDIENCE,
            old_key_evidence.issued_at_ms,
            &fixture.authority,
            "authorize-old-key-after-rotation",
        );
        assert!(matches!(
            import_ats_certification_evidence_at(
                &fixture.pool,
                &old_key_envelope,
                "certifier",
                TEST_NOW_MS,
            ),
            Err(AtsCertificationAuthorityError::InvalidSignature)
        ));

        let mut cross_policy_manifest = test_manifest(
            fixture.manifest.evidence_sha256s[0].clone(),
            fixture
                .manifest
                .certification_profile
                .layout_observation_sha256s[0]
                .clone(),
            &second_policy_sha256,
        );
        cross_policy_manifest.certification_id = "cross-policy-manifest-2".to_string();
        cross_policy_manifest.manifest_generation = 2;
        let cross_policy_envelope = envelope(
            &cross_policy_manifest,
            "manifest",
            ATS_CERTIFICATION_MANIFEST_AUDIENCE,
            cross_policy_manifest.issued_at_ms,
            &rotated,
            "authorize-cross-policy-manifest-2",
        );
        assert!(import_ats_certification_manifest_at(
            &fixture.pool,
            &cross_policy_envelope,
            "certifier",
            TEST_NOW_MS,
        )
        .is_err());
    }

    #[test]
    fn only_greenhouse_and_lever_manifests_can_claim_submission_capability() {
        let greenhouse = test_manifest("0".repeat(64), "9".repeat(64), &"8".repeat(64));
        assert!(validate_ats_manifest_authority(&greenhouse).is_ok());

        let mut lever = greenhouse.clone();
        lever.certification_id = "lever-acme-job-cert-1".to_string();
        lever.provider = "lever".to_string();
        lever.target_key =
            "lever:jobs.lever.co:acme:01234567-89ab-cdef-0123-456789abcdef".to_string();
        lever.allowed_provider_hosts = vec!["jobs.lever.co".to_string()];
        lever.adapter_version = ATS_LEVER_EXACT_ADAPTER_VERSION.to_string();
        lever.final_submit_control_id = ATS_LEVER_FINAL_SUBMIT_CONTROL_ID.to_string();
        lever.scope_sha256 = ats_certification_scope_sha256(
            &lever.provider,
            &lever.target_key,
            &lever.variant_key,
            &lever.surface_sha256,
        )
        .unwrap();
        assert!(validate_ats_manifest_authority(&lever).is_ok());

        for (provider, target_key, adapter_version) in [
            ("workday", "workday:tenant:posting", "workday-semantic-1"),
            ("ashby", "ashby:tenant:posting", "ashby-semantic-1"),
            (
                "smartrecruiters",
                "smartrecruiters:tenant:posting",
                "smartrecruiters-semantic-1",
            ),
        ] {
            let mut manifest = greenhouse.clone();
            manifest.certification_id = format!("{provider}-shadow-cert-1");
            manifest.provider = provider.to_string();
            manifest.target_key = target_key.to_string();
            manifest.adapter_version = adapter_version.to_string();
            manifest.scope_sha256 = ats_certification_scope_sha256(
                &manifest.provider,
                &manifest.target_key,
                &manifest.variant_key,
                &manifest.surface_sha256,
            )
            .unwrap();
            assert!(
                validate_ats_manifest_authority(&manifest).is_err(),
                "{provider} must not claim unattended submit",
            );
            manifest.maximum_capability = "observe_only".to_string();
            assert!(
                validate_ats_manifest_authority(&manifest).is_ok(),
                "{provider} should remain structurally observable in shadow",
            );
        }

        for provider in ["semantic", "protected_portal", "unknown"] {
            let mut manifest = greenhouse.clone();
            manifest.provider = provider.to_string();
            manifest.maximum_capability = "observe_only".to_string();
            assert!(validate_ats_manifest_authority(&manifest).is_err());
        }
    }

    #[test]
    fn phase_a_nonce_scope_packet_and_exact_replay_fail_closed() {
        let fixture = imported_fixture();
        let request = test_binding_request("phase-a-single-use", 'a');
        let first =
            create_ats_application_certification_binding(&fixture.pool, &request, TEST_NOW_MS)
                .unwrap();
        assert!(!first.replayed);
        let replay =
            create_ats_application_certification_binding(&fixture.pool, &request, TEST_NOW_MS + 1)
                .unwrap();
        assert!(replay.replayed);
        assert_eq!(replay.binding_sha256, first.binding_sha256);

        let mut changed_nonce = request.clone();
        changed_nonce.nonce_sha256 = "b".repeat(64);
        assert!(matches!(
            create_ats_application_certification_binding(
                &fixture.pool,
                &changed_nonce,
                TEST_NOW_MS + 1,
            ),
            Err(AtsCertificationAuthorityError::IdentityConflict)
        ));

        let mut cross_account = test_binding_request("phase-a-cross-account", 'a');
        cross_account.account_id = "account-2".to_string();
        cross_account.application_id = "application-2".to_string();
        assert!(matches!(
            create_ats_application_certification_binding(
                &fixture.pool,
                &cross_account,
                TEST_NOW_MS + 1,
            ),
            Err(AtsCertificationAuthorityError::IdentityConflict)
        ));

        let mut cross_run = request.clone();
        cross_run.binding_id = "phase-a-cross-run".to_string();
        cross_run.run_id = "different-run".to_string();
        cross_run.nonce_sha256 = "c".repeat(64);
        assert!(matches!(
            create_ats_application_certification_binding(
                &fixture.pool,
                &cross_run,
                TEST_NOW_MS + 1,
            ),
            Err(AtsCertificationAuthorityError::IdentityConflict)
        ));

        let mut stale_packet = request;
        stale_packet.binding_id = "phase-a-stale-packet".to_string();
        stale_packet.run_id = "stale-packet-run".to_string();
        stale_packet.nonce_sha256 = "d".repeat(64);
        stale_packet.packet_checksum_sha256 = "e".repeat(64);
        assert!(matches!(
            create_ats_application_certification_binding(
                &fixture.pool,
                &stale_packet,
                TEST_NOW_MS + 1,
            ),
            Err(AtsCertificationAuthorityError::ScopeMismatch)
                | Err(AtsCertificationAuthorityError::IdentityConflict)
        ));
    }

    #[test]
    fn every_revocable_authority_fences_new_phase_a_and_existing_phase_b() {
        for (subject_kind, cloud_runtime) in [
            ("activation", false),
            ("adapter_bundle", false),
            ("browser_release_manifest", false),
            ("evidence", false),
            ("layout_observation", false),
            ("manifest", false),
            ("policy", false),
            ("runner_build", true),
            ("runner_image", true),
            ("runtime", false),
            ("scope", false),
            ("target", false),
            ("trust_key", false),
        ] {
            let fixture = if cloud_runtime {
                imported_cloud_fixture()
            } else {
                imported_fixture()
            };
            let binding_request =
                test_binding_request(&format!("binding-before-{subject_kind}-revocation"), 'a');
            create_ats_application_certification_binding(
                &fixture.pool,
                &binding_request,
                TEST_NOW_MS,
            )
            .unwrap();

            let (subject_id, subject_sha256) = match subject_kind {
                "activation" => (
                    fixture.activation.activation_id.clone(),
                    fixture.activation_sha256.clone(),
                ),
                "adapter_bundle" => (
                    fixture.manifest.adapter_version.clone(),
                    fixture.manifest.adapter_bundle_sha256.clone(),
                ),
                "browser_release_manifest" => {
                    let sha256 = fixture.manifest.runtime_targets[0]
                        .browser_release_manifest_sha256
                        .clone()
                        .unwrap();
                    (sha256.clone(), sha256)
                }
                "evidence" => (
                    "evidence-authorized_sandbox".to_string(),
                    fixture.manifest.evidence_sha256s[0].clone(),
                ),
                "layout_observation" => (
                    "greenhouse-layout-authorized_sandbox-1".to_string(),
                    fixture
                        .manifest
                        .certification_profile
                        .layout_observation_sha256s[0]
                        .clone(),
                ),
                "manifest" => (
                    fixture.manifest.certification_id.clone(),
                    fixture.manifest_sha256.clone(),
                ),
                "policy" => (
                    "ats-trust-policy-1".to_string(),
                    fixture.policy_sha256.clone(),
                ),
                "runner_build" => {
                    let runner_build_id = fixture.manifest.runtime_targets[0]
                        .runner_build_id
                        .clone()
                        .unwrap();
                    let sha256 = ats_revocation_subject_key_sha256(&runner_build_id).unwrap();
                    (runner_build_id, sha256)
                }
                "runner_image" => {
                    let sha256 = fixture.manifest.runtime_targets[0]
                        .runner_image_sha256
                        .clone()
                        .unwrap();
                    (sha256.clone(), sha256)
                }
                "runtime" => (
                    fixture.manifest.runtime_targets[0].runtime_id.clone(),
                    fixture.manifest.runtime_targets[0].runtime_sha256.clone(),
                ),
                "scope" => (
                    fixture.manifest.scope_sha256.clone(),
                    fixture.manifest.scope_sha256.clone(),
                ),
                "target" => (
                    fixture.manifest.target_key.clone(),
                    ats_certification_target_key_sha256(&fixture.manifest.target_key).unwrap(),
                ),
                "trust_key" => {
                    let key_id = fixture.authority.key_ids["manifest"].clone();
                    let public_key_base64url =
                        &fixture.authority.anchor.roles["manifest"].keys[&key_id];
                    let public_key = base64::engine::general_purpose::URL_SAFE_NO_PAD
                        .decode(public_key_base64url)
                        .unwrap();
                    (key_id, ats_certification_sha256(&public_key))
                }
                _ => unreachable!(),
            };
            let revocation = AtsCertificationRevocationAuthority {
                version: 1,
                audience: ATS_CERTIFICATION_REVOCATION_AUDIENCE.to_string(),
                revocation_id: format!("revoke-{subject_kind}-before-phase-b"),
                policy_sha256: fixture.policy_sha256.clone(),
                revocation_generation: 1,
                predecessor_revocation_sha256: None,
                subject_kind: subject_kind.to_string(),
                subject_id,
                subject_sha256,
                reason_ref: "round-604-fail-closed-matrix".to_string(),
                issued_at_ms: TEST_NOW_MS + 1,
                effective_at_ms: TEST_NOW_MS + 1,
            };
            let revocation_envelope = envelope(
                &revocation,
                "revocation",
                ATS_CERTIFICATION_REVOCATION_AUDIENCE,
                revocation.issued_at_ms,
                &fixture.authority,
                &format!("authorize-{subject_kind}-revocation"),
            );
            import_ats_certification_revocation_at(
                &fixture.pool,
                &revocation_envelope,
                "incident-responder",
                TEST_NOW_MS + 1,
            )
            .unwrap();

            if subject_kind != "evidence" {
                let status = get_ats_certification_target_status_projection(
                    &fixture.pool,
                    &test_target_evidence(),
                    Some(&"1".repeat(64)),
                    "general",
                    None,
                    TEST_NOW_MS + 2,
                )
                .unwrap();
                assert_eq!(
                    status.status, "revoked",
                    "{subject_kind} revocation must project revoked status",
                );
            }

            let new_binding =
                test_binding_request(&format!("binding-after-{subject_kind}-revocation"), 'b');
            assert!(
                create_ats_application_certification_binding(
                    &fixture.pool,
                    &new_binding,
                    TEST_NOW_MS + 2,
                )
                .is_err(),
                "{subject_kind} revocation must fence new Phase A",
            );
            assert!(
                validate_consume_reserve_ats_application_certification(
                    &fixture.pool,
                    &test_phase_b_request(
                        &binding_request,
                        &format!("phase-b-after-{subject_kind}-revocation"),
                    ),
                    TEST_NOW_MS + 2,
                )
                .is_err(),
                "{subject_kind} revocation must fence existing Phase B",
            );
        }

        let fixture = imported_fixture();
        let mut binding_request = test_binding_request("binding-before-authority-expiry", 'c');
        binding_request.requested_expires_at_ms = fixture.activation.expires_at_ms + 1;
        create_ats_application_certification_binding(&fixture.pool, &binding_request, TEST_NOW_MS)
            .unwrap();
        let mut new_binding = test_binding_request("binding-after-authority-expiry", 'd');
        new_binding.requested_expires_at_ms = fixture.activation.expires_at_ms + 1;
        assert!(create_ats_application_certification_binding(
            &fixture.pool,
            &new_binding,
            fixture.activation.expires_at_ms,
        )
        .is_err());
        assert!(validate_consume_reserve_ats_application_certification(
            &fixture.pool,
            &test_phase_b_request(&binding_request, "phase-b-after-authority-expiry"),
            fixture.activation.expires_at_ms,
        )
        .is_err());
    }

    #[test]
    fn revocation_chain_is_predecessor_bound_monotonic_and_replay_safe() {
        let fixture = imported_cloud_fixture();
        let first = AtsCertificationRevocationAuthority {
            version: 1,
            audience: ATS_CERTIFICATION_REVOCATION_AUDIENCE.to_string(),
            revocation_id: "revoke-target-chain-1".to_string(),
            policy_sha256: fixture.policy_sha256.clone(),
            revocation_generation: 1,
            predecessor_revocation_sha256: None,
            subject_kind: "target".to_string(),
            subject_id: fixture.manifest.target_key.clone(),
            subject_sha256: ats_certification_target_key_sha256(&fixture.manifest.target_key)
                .unwrap(),
            reason_ref: "chain-test-target".to_string(),
            issued_at_ms: TEST_NOW_MS + 1,
            effective_at_ms: TEST_NOW_MS + 1,
        };
        let first_envelope = envelope(
            &first,
            "revocation",
            ATS_CERTIFICATION_REVOCATION_AUDIENCE,
            first.issued_at_ms,
            &fixture.authority,
            "authorize-chain-revocation-1",
        );
        let first_result = import_ats_certification_revocation_at(
            &fixture.pool,
            &first_envelope,
            "incident-responder",
            TEST_NOW_MS + 1,
        )
        .unwrap();
        assert!(!first_result.replayed);
        assert!(
            import_ats_certification_revocation_at(
                &fixture.pool,
                &first_envelope,
                "other-incident-responder",
                TEST_NOW_MS + 1,
            )
            .unwrap()
            .replayed
        );

        let mut conflicting_replay = first.clone();
        conflicting_replay.reason_ref = "changed-chain-test-target".to_string();
        let conflicting_envelope = envelope(
            &conflicting_replay,
            "revocation",
            ATS_CERTIFICATION_REVOCATION_AUDIENCE,
            conflicting_replay.issued_at_ms,
            &fixture.authority,
            "authorize-conflicting-chain-revocation-1",
        );
        assert!(matches!(
            import_ats_certification_revocation_at(
                &fixture.pool,
                &conflicting_envelope,
                "incident-responder",
                TEST_NOW_MS + 1,
            ),
            Err(AtsCertificationAuthorityError::IdentityConflict)
        ));

        let mut regression = first.clone();
        regression.revocation_id = "revoke-layout-regression".to_string();
        regression.subject_kind = "layout_observation".to_string();
        regression.subject_id = "greenhouse-layout-authorized_sandbox-1".to_string();
        regression.subject_sha256 = fixture
            .manifest
            .certification_profile
            .layout_observation_sha256s[0]
            .clone();
        let regression_envelope = envelope(
            &regression,
            "revocation",
            ATS_CERTIFICATION_REVOCATION_AUDIENCE,
            regression.issued_at_ms,
            &fixture.authority,
            "authorize-chain-regression",
        );
        assert!(matches!(
            import_ats_certification_revocation_at(
                &fixture.pool,
                &regression_envelope,
                "incident-responder",
                TEST_NOW_MS + 1,
            ),
            Err(AtsCertificationAuthorityError::SequenceRegression)
        ));

        let mut wrong_predecessor = regression.clone();
        wrong_predecessor.revocation_id = "revoke-layout-wrong-predecessor".to_string();
        wrong_predecessor.revocation_generation = 2;
        wrong_predecessor.predecessor_revocation_sha256 = Some("f".repeat(64));
        let wrong_predecessor_envelope = envelope(
            &wrong_predecessor,
            "revocation",
            ATS_CERTIFICATION_REVOCATION_AUDIENCE,
            wrong_predecessor.issued_at_ms,
            &fixture.authority,
            "authorize-chain-wrong-predecessor",
        );
        assert!(matches!(
            import_ats_certification_revocation_at(
                &fixture.pool,
                &wrong_predecessor_envelope,
                "incident-responder",
                TEST_NOW_MS + 1,
            ),
            Err(AtsCertificationAuthorityError::SequenceRegression)
        ));

        let mut second = wrong_predecessor;
        second.revocation_id = "revoke-layout-chain-2".to_string();
        second.predecessor_revocation_sha256 = Some(first_result.authority_sha256.clone());
        let second_envelope = envelope(
            &second,
            "revocation",
            ATS_CERTIFICATION_REVOCATION_AUDIENCE,
            second.issued_at_ms,
            &fixture.authority,
            "authorize-chain-revocation-2",
        );
        let second_result = import_ats_certification_revocation_at(
            &fixture.pool,
            &second_envelope,
            "incident-responder",
            TEST_NOW_MS + 1,
        )
        .unwrap();

        let runner_build_id = fixture.manifest.runtime_targets[0]
            .runner_build_id
            .clone()
            .unwrap();
        let wrong_scope = AtsCertificationRevocationAuthority {
            version: 1,
            audience: ATS_CERTIFICATION_REVOCATION_AUDIENCE.to_string(),
            revocation_id: "revoke-runner-build-wrong-scope".to_string(),
            policy_sha256: fixture.policy_sha256.clone(),
            revocation_generation: 3,
            predecessor_revocation_sha256: Some(second_result.authority_sha256.clone()),
            subject_kind: "runner_build".to_string(),
            subject_id: runner_build_id,
            subject_sha256: "e".repeat(64),
            reason_ref: "chain-test-wrong-runner-build-digest".to_string(),
            issued_at_ms: TEST_NOW_MS + 1,
            effective_at_ms: TEST_NOW_MS + 1,
        };
        let wrong_scope_envelope = envelope(
            &wrong_scope,
            "revocation",
            ATS_CERTIFICATION_REVOCATION_AUDIENCE,
            wrong_scope.issued_at_ms,
            &fixture.authority,
            "authorize-chain-wrong-scope",
        );
        assert!(matches!(
            import_ats_certification_revocation_at(
                &fixture.pool,
                &wrong_scope_envelope,
                "incident-responder",
                TEST_NOW_MS + 1,
            ),
            Err(AtsCertificationAuthorityError::NotFound)
        ));

        let conn = fixture.pool.get().unwrap();
        let stored: (i64, Option<String>) = conn
            .query_row(
                "SELECT revocation_generation, predecessor_revocation_sha256
                   FROM jobs_ats_certification_revocations
                  WHERE revocation_sha256 = ?1",
                params![second_result.authority_sha256],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(stored, (2, Some(first_result.authority_sha256.clone())));
        assert!(conn
            .execute(
                "INSERT INTO jobs_ats_certification_revocations (
                   revocation_sha256, revocation_id, revocation_generation,
                   predecessor_revocation_sha256, subject_kind, subject_id, subject_sha256,
                   reason_ref, canonical_revocation_base64url, authorization_sha256,
                   trust_policy_sha256, canonical_authorization_base64url, issued_at_ms,
                   effective_at_ms, recorded_by, recorded_at_ms
                 )
                 SELECT ?1, 'direct-invalid-chain', 4, ?2, 'runtime',
                        'direct-invalid-runtime', ?3, reason_ref,
                        canonical_revocation_base64url, authorization_sha256,
                        trust_policy_sha256, canonical_authorization_base64url, issued_at_ms,
                        effective_at_ms, recorded_by, recorded_at_ms
                   FROM jobs_ats_certification_revocations
                  WHERE revocation_sha256 = ?4",
                params![
                    "0".repeat(64),
                    first_result.authority_sha256,
                    "b".repeat(64),
                    second_result.authority_sha256,
                ],
            )
            .is_err());
    }

    #[test]
    fn manifest_rejects_missing_duplicate_extra_failed_and_skipped_required_checks() {
        let authority = test_authority();
        let policy = test_trust_policy(&authority, 1, None);
        let manifest = test_manifest("0".repeat(64), "9".repeat(64), &"8".repeat(64));
        assert!(require_ats_manifest_policy(&manifest, &policy, TEST_NOW_MS).is_ok());

        let mut missing = manifest.clone();
        missing.certification_profile.check_results.pop();
        assert!(require_ats_manifest_policy(&missing, &policy, TEST_NOW_MS).is_err());

        let mut duplicate = manifest.clone();
        duplicate
            .certification_profile
            .check_results
            .insert(1, duplicate.certification_profile.check_results[0].clone());
        assert!(validate_ats_manifest_authority(&duplicate).is_err());

        let mut extra = manifest.clone();
        extra.certification_profile.check_results[0].check_id = "ATS-UNKNOWN-001".to_string();
        assert!(validate_ats_manifest_authority(&extra).is_err());

        let mut failed = manifest.clone();
        failed.certification_profile.check_results[0].failed_count = 1;
        assert!(validate_ats_manifest_authority(&failed).is_err());

        let mut skipped = manifest;
        skipped.certification_profile.check_results[0].skipped_count = 1;
        assert!(validate_ats_manifest_authority(&skipped).is_err());
    }

    #[test]
    fn manifest_rejects_each_nonzero_zero_tolerance_counter() {
        let manifest = test_manifest("0".repeat(64), "9".repeat(64), &"8".repeat(64));
        for name in [
            "hard_filter_violations",
            "unsupported_factual_claims",
            "duplicate_submit_activations",
            "false_submitted_states",
            "incomplete_or_mismatched_receipts",
            "pii_bearing_observations",
        ] {
            let mut unsafe_manifest = manifest.clone();
            let zero = &mut unsafe_manifest.certification_profile.zero_tolerance;
            match name {
                "hard_filter_violations" => zero.hard_filter_violations = 1,
                "unsupported_factual_claims" => zero.unsupported_factual_claims = 1,
                "duplicate_submit_activations" => zero.duplicate_submit_activations = 1,
                "false_submitted_states" => zero.false_submitted_states = 1,
                "incomplete_or_mismatched_receipts" => {
                    zero.incomplete_or_mismatched_receipts = 1;
                }
                "pii_bearing_observations" => zero.pii_bearing_observations = 1,
                _ => unreachable!(),
            }
            assert!(
                validate_ats_manifest_authority(&unsafe_manifest).is_err(),
                "nonzero {name} must fail closed",
            );
        }
    }

    #[test]
    fn fresh_target_status_resolves_without_live_layout_and_rejects_stale_sources() {
        let fixture = imported_fixture();
        let status = resolve_ats_certification_target_status(
            &fixture.pool,
            &test_target_evidence(),
            None,
            "general",
            None,
            TEST_NOW_MS,
        )
        .unwrap()
        .unwrap();
        assert_eq!(status.manifest_sha256, fixture.manifest_sha256);
        assert!(status.selected_runtime.is_none());
        assert_eq!(status.runtime_targets.len(), 1);
        let admission = ats_certification_admission_projection(&status).unwrap();
        assert_eq!(admission.manifest_sha256, fixture.manifest_sha256);
        assert_eq!(admission.runner_target_sha256s, vec!["1".repeat(64)]);
        let frozen = ats_frozen_certification_admission_projection(&status).unwrap();
        assert_eq!(frozen.variant_key, test_surface().variant_key);
        assert_eq!(
            frozen.layout_contract_version,
            test_surface().layout_contract_version
        );
        assert_eq!(frozen.surface_sha256, test_surface().surface_sha256);
        let frozen_json = serde_json::to_value(&frozen).unwrap();
        assert!(frozen_json.get("variant_key").is_some());
        assert!(frozen_json.get("layout_contract_version").is_some());
        assert!(frozen_json.get("surface_sha256").is_some());
        assert!(frozen_json.get("variantKey").is_none());
        let projection = get_ats_certification_target_status_projection(
            &fixture.pool,
            &test_target_evidence(),
            Some(&"1".repeat(64)),
            "general",
            None,
            TEST_NOW_MS,
        )
        .unwrap();
        assert_eq!(projection.status, "active");
        assert_eq!(projection.runner_target_sha256s, vec!["1".repeat(64)]);
        assert_eq!(projection.target_key_sha256.len(), 64);
        assert_eq!(
            projection.last_verified_at_ms,
            Some(fixture.manifest.tested_at_ms)
        );

        let mut stale = test_target_evidence();
        stale.original_source_observed_at_ms =
            TEST_NOW_MS - ATS_CERTIFICATION_TARGET_EVIDENCE_FRESHNESS_MS - 1;
        assert!(matches!(
            resolve_ats_certification_target_status(
                &fixture.pool,
                &stale,
                None,
                "general",
                None,
                TEST_NOW_MS,
            ),
            Err(AtsCertificationAuthorityError::InvalidAuthority)
        ));
    }

    #[test]
    fn posting_resolver_derives_target_and_runtime_without_client_authority_fields() {
        let fixture = imported_fixture();
        let resolution = resolve_ats_certification_for_posting(
            &fixture.pool,
            "account-1",
            &test_posting(),
            Some(&local_runtime_attestation()),
            TEST_NOW_MS,
        )
        .unwrap();
        assert_eq!(resolution.status.status, "active");
        let binding = resolution.active_binding.unwrap();
        assert_eq!(binding.manifest_sha256, fixture.manifest_sha256);
        assert_eq!(
            binding
                .selected_runtime
                .as_ref()
                .map(|runtime| runtime.runtime_sha256.as_str()),
            Some("1111111111111111111111111111111111111111111111111111111111111111")
        );

        let mut wrong_runtime = local_runtime_attestation();
        let AtsCertificationRuntimeAttestation::Local {
            chromium_executable_sha256,
            ..
        } = &mut wrong_runtime
        else {
            unreachable!();
        };
        *chromium_executable_sha256 = "f".repeat(64);
        let drifted = resolve_ats_certification_for_posting(
            &fixture.pool,
            "account-1",
            &test_posting(),
            Some(&wrong_runtime),
            TEST_NOW_MS,
        )
        .unwrap();
        assert_eq!(drifted.status.status, "drifted");
        assert!(drifted.active_binding.is_none());
    }

    #[test]
    fn target_status_reports_expired_at_the_activation_boundary() {
        let fixture = imported_fixture();
        let expired = get_ats_certification_target_status_projection(
            &fixture.pool,
            &test_target_evidence(),
            Some(&"1".repeat(64)),
            "general",
            None,
            fixture.activation.expires_at_ms,
        )
        .unwrap();
        assert_eq!(expired.status, "expired");
        assert_eq!(
            expired.last_verified_at_ms,
            Some(fixture.manifest.tested_at_ms)
        );
        assert_eq!(
            expired.expires_at_ms,
            Some(fixture.activation.expires_at_ms)
        );
    }

    #[test]
    fn circuit_open_fences_resolution_and_only_reviewed_close_restores_it() {
        let fixture = imported_fixture();
        let open = AtsCertificationCircuitEvent {
            event_id: "target-circuit-open-1".to_string(),
            scope_kind: "target".to_string(),
            subject_key: "greenhouse:acme:123".to_string(),
            transition: "opened".to_string(),
            trigger_kind: "layout_drift".to_string(),
            window_started_at_ms: TEST_NOW_MS,
            window_ended_at_ms: TEST_NOW_MS,
            failure_count: 1,
            sample_count: 1,
            threshold_count: 1,
            authority_ref: "layout-drift-incident-1".to_string(),
            event_at_ms: TEST_NOW_MS + 1,
        };
        let mut conn = fixture.pool.get().unwrap();
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        let opened = append_ats_certification_circuit_event_sqlite_tx(
            &tx,
            &open,
            0,
            None,
            "incident-responder",
            TEST_NOW_MS + 1,
        )
        .unwrap();
        tx.commit().unwrap();
        assert_eq!(opened.state, "opened");
        assert!(resolve_ats_certification_target_status(
            &fixture.pool,
            &test_target_evidence(),
            Some(&"1".repeat(64)),
            "general",
            None,
            TEST_NOW_MS + 1,
        )
        .unwrap()
        .is_none());
        let suspended_status = get_ats_certification_target_status_projection(
            &fixture.pool,
            &test_target_evidence(),
            Some(&"1".repeat(64)),
            "general",
            None,
            TEST_NOW_MS + 1,
        )
        .unwrap();
        assert_eq!(suspended_status.status, "suspended");
        assert_eq!(
            suspended_status.last_verified_at_ms,
            Some(fixture.manifest.tested_at_ms)
        );

        let invalid_close = AtsCertificationCircuitEvent {
            event_id: "target-circuit-invalid-close-2".to_string(),
            transition: "closed".to_string(),
            trigger_kind: "error_threshold".to_string(),
            event_at_ms: TEST_NOW_MS + 2,
            window_ended_at_ms: TEST_NOW_MS + 2,
            ..open.clone()
        };
        let mut conn = fixture.pool.get().unwrap();
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        assert!(matches!(
            append_ats_certification_circuit_event_sqlite_tx(
                &tx,
                &invalid_close,
                1,
                Some(&open.event_id),
                "incident-responder",
                TEST_NOW_MS + 2,
            ),
            Err(AtsCertificationAuthorityError::InvalidAuthority)
        ));
        tx.rollback().unwrap();

        let close = AtsCertificationCircuitEvent {
            event_id: "target-circuit-reviewed-close-2".to_string(),
            transition: "closed".to_string(),
            trigger_kind: "reviewed_close".to_string(),
            authority_ref: "review-ats-circuit-2".to_string(),
            event_at_ms: TEST_NOW_MS + 2,
            window_ended_at_ms: TEST_NOW_MS + 2,
            ..open.clone()
        };
        let mut conn = fixture.pool.get().unwrap();
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        append_ats_certification_circuit_event_sqlite_tx(
            &tx,
            &close,
            1,
            Some(&open.event_id),
            "incident-responder",
            TEST_NOW_MS + 2,
        )
        .unwrap();
        tx.commit().unwrap();
        assert!(resolve_ats_certification_target_status(
            &fixture.pool,
            &test_target_evidence(),
            Some(&"1".repeat(64)),
            "general",
            None,
            TEST_NOW_MS + 2,
        )
        .unwrap()
        .is_some());
    }

    #[test]
    fn newer_activation_circuit_close_requires_exact_activation_scope() {
        let fixture = imported_fixture();
        let scopes = [
            ("activation", fixture.activation_sha256.clone()),
            ("adapter", fixture.manifest.adapter_bundle_sha256.clone()),
            ("provider", fixture.manifest.provider.clone()),
            (
                "runtime",
                fixture.manifest.runtime_targets[0].runtime_sha256.clone(),
            ),
            ("target", fixture.manifest.target_key.clone()),
            ("provider", "lever".to_string()),
        ];
        let mut opened = Vec::with_capacity(scopes.len());
        for (index, (scope_kind, subject_key)) in scopes.iter().enumerate() {
            let event = AtsCertificationCircuitEvent {
                event_id: format!("newer-activation-{scope_kind}-open-{index}"),
                scope_kind: (*scope_kind).to_string(),
                subject_key: subject_key.clone(),
                transition: "opened".to_string(),
                trigger_kind: "evidence_failure".to_string(),
                window_started_at_ms: TEST_NOW_MS + 1,
                window_ended_at_ms: TEST_NOW_MS + 1,
                failure_count: 1,
                sample_count: 1,
                threshold_count: 1,
                authority_ref: format!("newer-activation-incident-{index}"),
                event_at_ms: TEST_NOW_MS + 1,
            };
            let mut conn = fixture.pool.get().unwrap();
            let tx = conn
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .unwrap();
            append_ats_certification_circuit_event_sqlite_tx(
                &tx,
                &event,
                0,
                None,
                "incident-responder",
                TEST_NOW_MS + 1,
            )
            .unwrap();
            tx.commit().unwrap();
            opened.push(event);
        }

        let invalid_authorities = [
            (
                "forged",
                ats_certification_sha256(b"forged-newer-activation"),
            ),
            ("stale", fixture.activation_sha256.clone()),
        ];
        for (kind, authority_ref) in invalid_authorities {
            let close = AtsCertificationCircuitEvent {
                event_id: format!("newer-activation-{kind}-close"),
                transition: "closed".to_string(),
                trigger_kind: "newer_activation".to_string(),
                authority_ref,
                window_ended_at_ms: TEST_NOW_MS + 2,
                event_at_ms: TEST_NOW_MS + 2,
                ..opened[0].clone()
            };
            let mut conn = fixture.pool.get().unwrap();
            let tx = conn
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .unwrap();
            assert!(matches!(
                append_ats_certification_circuit_event_sqlite_tx(
                    &tx,
                    &close,
                    1,
                    Some(&opened[0].event_id),
                    "incident-responder",
                    TEST_NOW_MS + 2,
                ),
                Err(AtsCertificationAuthorityError::InvalidAuthority)
            ));
            tx.rollback().unwrap();
        }

        let mut successor = fixture.activation.clone();
        successor.activation_id = "greenhouse-acme-123-general-2".to_string();
        successor.activation_generation = 2;
        successor.predecessor_activation_sha256 = Some(fixture.activation_sha256.clone());
        successor.channel_sequence = 2;
        successor.approval_ref = "approval-general-2".to_string();
        let successor_envelope = envelope(
            &successor,
            "activation",
            ATS_CERTIFICATION_ACTIVATION_AUDIENCE,
            successor.issued_at_ms,
            &fixture.authority,
            "authorize-activation-2",
        );
        import_ats_certification_activation_at(
            &fixture.pool,
            &successor_envelope,
            "promoter",
            TEST_NOW_MS + 2,
        )
        .unwrap();
        let successor_sha256 = envelope_sha256(&successor_envelope);
        let imported_only_close = AtsCertificationCircuitEvent {
            event_id: "newer-activation-imported-only-close".to_string(),
            transition: "closed".to_string(),
            trigger_kind: "newer_activation".to_string(),
            authority_ref: successor_sha256.clone(),
            window_ended_at_ms: TEST_NOW_MS + 2,
            event_at_ms: TEST_NOW_MS + 2,
            ..opened[0].clone()
        };
        let mut conn = fixture.pool.get().unwrap();
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        assert!(matches!(
            append_ats_certification_circuit_event_sqlite_tx(
                &tx,
                &imported_only_close,
                1,
                Some(&opened[0].event_id),
                "incident-responder",
                TEST_NOW_MS + 2,
            ),
            Err(AtsCertificationAuthorityError::InvalidAuthority)
        ));
        tx.rollback().unwrap();
        apply_ats_certification_activation_at(
            &fixture.pool,
            &successor_sha256,
            fixture.head.head_revision,
            Some(&fixture.head.transition_sha256),
            "promoter",
            TEST_NOW_MS + 2,
        )
        .unwrap();

        for (index, open) in opened.iter().skip(1).enumerate() {
            let close = AtsCertificationCircuitEvent {
                event_id: format!("newer-activation-broad-scope-close-{index}"),
                transition: "closed".to_string(),
                trigger_kind: "newer_activation".to_string(),
                authority_ref: successor_sha256.clone(),
                window_ended_at_ms: TEST_NOW_MS + 3,
                event_at_ms: TEST_NOW_MS + 3,
                ..open.clone()
            };
            let mut conn = fixture.pool.get().unwrap();
            let tx = conn
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .unwrap();
            assert!(matches!(
                append_ats_certification_circuit_event_sqlite_tx(
                    &tx,
                    &close,
                    1,
                    Some(&open.event_id),
                    "incident-responder",
                    TEST_NOW_MS + 3,
                ),
                Err(AtsCertificationAuthorityError::InvalidAuthority)
            ));
            tx.rollback().unwrap();

            let reviewed_close = AtsCertificationCircuitEvent {
                event_id: format!("reviewed-broad-scope-close-{index}"),
                transition: "closed".to_string(),
                trigger_kind: "reviewed_close".to_string(),
                authority_ref: format!("reviewed-broad-scope-authority-{index}"),
                window_ended_at_ms: TEST_NOW_MS + 4,
                event_at_ms: TEST_NOW_MS + 4,
                ..open.clone()
            };
            let mut conn = fixture.pool.get().unwrap();
            let tx = conn
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .unwrap();
            let closed = append_ats_certification_circuit_event_sqlite_tx(
                &tx,
                &reviewed_close,
                1,
                Some(&open.event_id),
                "incident-responder",
                TEST_NOW_MS + 4,
            )
            .unwrap();
            tx.commit().unwrap();
            assert_eq!(closed.state, "closed");
        }

        let activation_close = AtsCertificationCircuitEvent {
            event_id: "newer-activation-exact-activation-close".to_string(),
            transition: "closed".to_string(),
            trigger_kind: "newer_activation".to_string(),
            authority_ref: successor_sha256,
            window_ended_at_ms: TEST_NOW_MS + 3,
            event_at_ms: TEST_NOW_MS + 3,
            ..opened[0].clone()
        };
        let mut conn = fixture.pool.get().unwrap();
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        let closed = append_ats_certification_circuit_event_sqlite_tx(
            &tx,
            &activation_close,
            1,
            Some(&opened[0].event_id),
            "incident-responder",
            TEST_NOW_MS + 3,
        )
        .unwrap();
        tx.commit().unwrap();
        assert_eq!(closed.state, "closed");
    }

    #[test]
    fn newer_activation_circuit_close_rejects_shadow_and_canary_for_target_scope() {
        for channel in ["shadow", "canary"] {
            let fixture = imported_fixture();
            let mut initial = fixture.activation.clone();
            initial.activation_id = format!("narrow-{channel}-activation-1");
            initial.channel = channel.to_string();
            initial.approval_ref = format!("narrow-{channel}-approval-1");
            if channel == "shadow" {
                initial.capability = "observe_only".to_string();
            } else {
                configure_test_canary_activation(
                    &fixture.pool,
                    &mut initial,
                    "narrow-target-close",
                    vec!["account-1".to_string()],
                    (1, 1, 1, 1),
                );
            }
            let initial_envelope = envelope(
                &initial,
                "activation",
                ATS_CERTIFICATION_ACTIVATION_AUDIENCE,
                initial.issued_at_ms,
                &fixture.authority,
                &format!("authorize-narrow-{channel}-activation-1"),
            );
            import_ats_certification_activation_at(
                &fixture.pool,
                &initial_envelope,
                "promoter",
                TEST_NOW_MS,
            )
            .unwrap();
            let initial_sha256 = envelope_sha256(&initial_envelope);
            let initial_head = apply_ats_certification_activation_at(
                &fixture.pool,
                &initial_sha256,
                0,
                None,
                "promoter",
                TEST_NOW_MS,
            )
            .unwrap();

            let open = AtsCertificationCircuitEvent {
                event_id: format!("narrow-{channel}-target-open"),
                scope_kind: "target".to_string(),
                subject_key: fixture.manifest.target_key.clone(),
                transition: "opened".to_string(),
                trigger_kind: "evidence_failure".to_string(),
                window_started_at_ms: TEST_NOW_MS + 1,
                window_ended_at_ms: TEST_NOW_MS + 1,
                failure_count: 1,
                sample_count: 1,
                threshold_count: 1,
                authority_ref: format!("narrow-{channel}-target-incident"),
                event_at_ms: TEST_NOW_MS + 1,
            };
            let mut conn = fixture.pool.get().unwrap();
            let tx = conn
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .unwrap();
            append_ats_certification_circuit_event_sqlite_tx(
                &tx,
                &open,
                0,
                None,
                "incident-responder",
                TEST_NOW_MS + 1,
            )
            .unwrap();
            tx.commit().unwrap();

            let mut successor = initial;
            successor.activation_id = format!("narrow-{channel}-activation-2");
            successor.activation_generation = 2;
            successor.predecessor_activation_sha256 = Some(initial_sha256);
            successor.channel_sequence = 2;
            successor.approval_ref = format!("narrow-{channel}-approval-2");
            let successor_envelope = envelope(
                &successor,
                "activation",
                ATS_CERTIFICATION_ACTIVATION_AUDIENCE,
                successor.issued_at_ms,
                &fixture.authority,
                &format!("authorize-narrow-{channel}-activation-2"),
            );
            import_ats_certification_activation_at(
                &fixture.pool,
                &successor_envelope,
                "promoter",
                TEST_NOW_MS + 2,
            )
            .unwrap();
            let successor_sha256 = envelope_sha256(&successor_envelope);
            apply_ats_certification_activation_at(
                &fixture.pool,
                &successor_sha256,
                initial_head.head_revision,
                Some(&initial_head.transition_sha256),
                "promoter",
                TEST_NOW_MS + 2,
            )
            .unwrap();

            let close = AtsCertificationCircuitEvent {
                event_id: format!("narrow-{channel}-target-close"),
                transition: "closed".to_string(),
                trigger_kind: "newer_activation".to_string(),
                authority_ref: successor_sha256,
                window_ended_at_ms: TEST_NOW_MS + 3,
                event_at_ms: TEST_NOW_MS + 3,
                ..open.clone()
            };
            let mut conn = fixture.pool.get().unwrap();
            let tx = conn
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .unwrap();
            assert!(matches!(
                append_ats_certification_circuit_event_sqlite_tx(
                    &tx,
                    &close,
                    1,
                    Some(&open.event_id),
                    "incident-responder",
                    TEST_NOW_MS + 3,
                ),
                Err(AtsCertificationAuthorityError::InvalidAuthority)
            ));
            tx.rollback().unwrap();
            let state: (String, i64) = fixture
                .pool
                .get()
                .unwrap()
                .query_row(
                    "SELECT state, head_revision FROM jobs_ats_certification_circuit_heads
                      WHERE scope_kind = 'target' AND subject_key = ?1",
                    params![fixture.manifest.target_key],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .unwrap();
            assert_eq!(state, ("opened".to_string(), 1));
        }
    }

    #[test]
    fn newer_activation_circuit_close_requires_current_successor_authority() {
        for unavailable_kind in ["activation", "runtime", "expired"] {
            let fixture = imported_fixture();
            let scope_kind = "activation";
            let subject_key = fixture.activation_sha256.clone();
            let open = AtsCertificationCircuitEvent {
                event_id: format!("current-successor-{unavailable_kind}-open"),
                scope_kind: scope_kind.to_string(),
                subject_key: subject_key.clone(),
                transition: "opened".to_string(),
                trigger_kind: "evidence_failure".to_string(),
                window_started_at_ms: TEST_NOW_MS + 1,
                window_ended_at_ms: TEST_NOW_MS + 1,
                failure_count: 1,
                sample_count: 1,
                threshold_count: 1,
                authority_ref: format!("current-successor-{unavailable_kind}-incident"),
                event_at_ms: TEST_NOW_MS + 1,
            };
            let mut conn = fixture.pool.get().unwrap();
            let tx = conn
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .unwrap();
            append_ats_certification_circuit_event_sqlite_tx(
                &tx,
                &open,
                0,
                None,
                "incident-responder",
                TEST_NOW_MS + 1,
            )
            .unwrap();
            tx.commit().unwrap();

            let mut successor = fixture.activation.clone();
            successor.activation_id = format!("current-successor-{unavailable_kind}-2");
            successor.activation_generation = 2;
            successor.predecessor_activation_sha256 = Some(fixture.activation_sha256.clone());
            successor.channel_sequence = 2;
            successor.approval_ref = format!("current-successor-{unavailable_kind}-approval");
            let successor_envelope = envelope(
                &successor,
                "activation",
                ATS_CERTIFICATION_ACTIVATION_AUDIENCE,
                successor.issued_at_ms,
                &fixture.authority,
                &format!("authorize-current-successor-{unavailable_kind}"),
            );
            import_ats_certification_activation_at(
                &fixture.pool,
                &successor_envelope,
                "promoter",
                TEST_NOW_MS + 2,
            )
            .unwrap();
            let successor_sha256 = envelope_sha256(&successor_envelope);
            apply_ats_certification_activation_at(
                &fixture.pool,
                &successor_sha256,
                fixture.head.head_revision,
                Some(&fixture.head.transition_sha256),
                "promoter",
                TEST_NOW_MS + 2,
            )
            .unwrap();

            let recorded_at_ms = if unavailable_kind == "expired" {
                successor.expires_at_ms
            } else {
                let (subject_id, subject_sha256) = if unavailable_kind == "activation" {
                    (successor.activation_id.clone(), successor_sha256.clone())
                } else {
                    (
                        fixture.manifest.runtime_targets[0].runtime_id.clone(),
                        fixture.manifest.runtime_targets[0].runtime_sha256.clone(),
                    )
                };
                let revocation = AtsCertificationRevocationAuthority {
                    version: 1,
                    audience: ATS_CERTIFICATION_REVOCATION_AUDIENCE.to_string(),
                    revocation_id: format!("revoke-current-successor-{unavailable_kind}"),
                    policy_sha256: fixture.policy_sha256.clone(),
                    revocation_generation: 1,
                    predecessor_revocation_sha256: None,
                    subject_kind: unavailable_kind.to_string(),
                    subject_id,
                    subject_sha256,
                    reason_ref: "current-successor-authority-test".to_string(),
                    issued_at_ms: TEST_NOW_MS + 3,
                    effective_at_ms: TEST_NOW_MS + 3,
                };
                let revocation_envelope = envelope(
                    &revocation,
                    "revocation",
                    ATS_CERTIFICATION_REVOCATION_AUDIENCE,
                    revocation.issued_at_ms,
                    &fixture.authority,
                    &format!("authorize-current-successor-{unavailable_kind}-revocation"),
                );
                import_ats_certification_revocation_at(
                    &fixture.pool,
                    &revocation_envelope,
                    "incident-responder",
                    TEST_NOW_MS + 3,
                )
                .unwrap();
                TEST_NOW_MS + 4
            };

            let close = AtsCertificationCircuitEvent {
                event_id: format!("current-successor-{unavailable_kind}-close"),
                transition: "closed".to_string(),
                trigger_kind: "newer_activation".to_string(),
                authority_ref: successor_sha256,
                window_ended_at_ms: TEST_NOW_MS + 2,
                event_at_ms: TEST_NOW_MS + 2,
                ..open.clone()
            };
            let mut conn = fixture.pool.get().unwrap();
            let tx = conn
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .unwrap();
            let result = append_ats_certification_circuit_event_sqlite_tx(
                &tx,
                &close,
                1,
                Some(&open.event_id),
                "incident-responder",
                recorded_at_ms,
            );
            match unavailable_kind {
                "activation" => assert!(matches!(
                    result,
                    Err(AtsCertificationAuthorityError::Revoked)
                )),
                "runtime" => assert!(matches!(
                    result,
                    Err(AtsCertificationAuthorityError::InvalidAuthority)
                )),
                "expired" => assert!(matches!(
                    result,
                    Err(AtsCertificationAuthorityError::Expired)
                )),
                _ => unreachable!(),
            }
            tx.rollback().unwrap();

            let state: (String, i64, i64) = fixture
                .pool
                .get()
                .unwrap()
                .query_row(
                    "SELECT head.state, head.head_revision,
                            (SELECT COUNT(*) FROM jobs_ats_certification_circuit_events event
                              WHERE event.scope_kind = head.scope_kind
                                AND event.subject_key = head.subject_key)
                       FROM jobs_ats_certification_circuit_heads head
                      WHERE head.scope_kind = ?1 AND head.subject_key = ?2",
                    params![scope_kind, subject_key],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .unwrap();
            assert_eq!(state, ("opened".to_string(), 1, 1));
        }
    }

    #[test]
    fn newer_activation_circuit_close_requires_current_canary_allowlist() {
        let fixture = imported_canary_fixture(2, 2, 1, 2);
        let open = AtsCertificationCircuitEvent {
            event_id: "current-canary-allowlist-open".to_string(),
            scope_kind: "activation".to_string(),
            subject_key: fixture.activation_sha256.clone(),
            transition: "opened".to_string(),
            trigger_kind: "evidence_failure".to_string(),
            window_started_at_ms: TEST_NOW_MS + 1,
            window_ended_at_ms: TEST_NOW_MS + 1,
            failure_count: 1,
            sample_count: 1,
            threshold_count: 1,
            authority_ref: "current-canary-allowlist-incident".to_string(),
            event_at_ms: TEST_NOW_MS + 1,
        };
        let mut conn = fixture.pool.get().unwrap();
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        append_ats_certification_circuit_event_sqlite_tx(
            &tx,
            &open,
            0,
            None,
            "incident-responder",
            TEST_NOW_MS + 1,
        )
        .unwrap();
        tx.commit().unwrap();

        let mut successor = fixture.activation.clone();
        successor.activation_id = "current-canary-allowlist-successor".to_string();
        successor.activation_generation = 2;
        successor.predecessor_activation_sha256 = Some(fixture.activation_sha256.clone());
        successor.channel_sequence = 2;
        successor.approval_ref = "current-canary-allowlist-successor-approval".to_string();
        let successor_envelope = envelope(
            &successor,
            "activation",
            ATS_CERTIFICATION_ACTIVATION_AUDIENCE,
            successor.issued_at_ms,
            &fixture.authority,
            "authorize-current-canary-allowlist-successor",
        );
        import_ats_certification_activation_at(
            &fixture.pool,
            &successor_envelope,
            "promoter",
            TEST_NOW_MS + 2,
        )
        .unwrap();
        let successor_sha256 = envelope_sha256(&successor_envelope);
        apply_ats_certification_activation_at(
            &fixture.pool,
            &successor_sha256,
            fixture.head.head_revision,
            Some(&fixture.head.transition_sha256),
            "promoter",
            TEST_NOW_MS + 2,
        )
        .unwrap();
        revoke_ats_certification_canary_allowlist(
            &fixture.pool,
            &AtsCertificationCanaryAllowlistRevocationRequest {
                allowlist_sha256: successor.account_allowlist_sha256.clone().unwrap(),
                revocation_ref: "current-canary-allowlist-revocation".to_string(),
            },
            "incident-responder",
            TEST_NOW_MS + 3,
        )
        .unwrap();

        let close = AtsCertificationCircuitEvent {
            event_id: "current-canary-allowlist-close".to_string(),
            transition: "closed".to_string(),
            trigger_kind: "newer_activation".to_string(),
            authority_ref: successor_sha256,
            window_ended_at_ms: TEST_NOW_MS + 2,
            event_at_ms: TEST_NOW_MS + 2,
            ..open.clone()
        };
        let mut conn = fixture.pool.get().unwrap();
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        assert!(matches!(
            append_ats_certification_circuit_event_sqlite_tx(
                &tx,
                &close,
                1,
                Some(&open.event_id),
                "incident-responder",
                TEST_NOW_MS + 4,
            ),
            Err(AtsCertificationAuthorityError::ScopeMismatch)
        ));
        tx.rollback().unwrap();
        let state: (String, i64) = fixture
            .pool
            .get()
            .unwrap()
            .query_row(
                "SELECT state, head_revision FROM jobs_ats_certification_circuit_heads
                  WHERE scope_kind = 'activation' AND subject_key = ?1",
                params![fixture.activation_sha256],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(state, ("opened".to_string(), 1));
    }

    #[test]
    fn certified_cloud_context_runs_phase_a_then_atomic_phase_b() {
        let fixture = imported_cloud_fixture();
        let account_id = "account-1";
        let application_id = "application-cloud-1";
        let run_id = "run-cloud-1";
        let attempt_id = "attempt-cloud-1";
        let browser_profile_id = "browser-profile-cloud-1";
        let auto_authorization_id = "auto-authorization-cloud-1";
        let auto_authorization_fingerprint = "8".repeat(64);
        let surface = test_surface();
        let active = resolve_active_ats_certification(
            &fixture.pool,
            &test_target_evidence().canonical_url,
            None,
            Some(&surface),
            TEST_NOW_MS,
        )
        .unwrap()
        .expect("active signed cloud certification");
        assert_eq!(active.runtime_targets.len(), 1);
        assert_eq!(active.runtime_targets[0].runtime_kind, "cloud");
        let frozen_certification = ats_frozen_certification_admission_projection(&active).unwrap();
        let packet = json!({
            "applicationId": application_id,
            "browserProfileId": browser_profile_id,
        });
        let job = json!({
            "canonicalUrl": test_target_evidence().canonical_url,
        });
        let admission = json!({
            "kind": "track_auto_submit",
            "authorization_id": auto_authorization_id,
            "career_track_id": "track-1",
            "revision_no": 1,
            "authority_fingerprint": auto_authorization_fingerprint,
            "ats_certification": frozen_certification,
        });
        let packet_checksum = approved_submission_checksum(3, &packet, &job, Some(&admission))
            .expect("hash frozen certified packet");
        let posting = test_posting();
        let application = JobApplication {
            id: application_id.to_string(),
            job_id: posting.id.clone(),
            resume_version_id: None,
            state: "running".to_string(),
            submission_mode: "auto_submit".to_string(),
            match_score: 90,
            answers: Vec::new(),
            cover_letter: String::new(),
            receipt: json!({
                "approved_execution": {
                    "schema_version": 3,
                    "approved_at_ms": TEST_NOW_MS - 10,
                    "checksum": packet_checksum,
                    "admission": admission,
                    "packet": packet,
                    "job": job,
                },
            }),
            run_id: Some(run_id.to_string()),
            created_at_ms: TEST_NOW_MS - 100,
            updated_at_ms: TEST_NOW_MS - 10,
            submitted_at_ms: None,
        };
        let session = BrowserSession {
            id: run_id.to_string(),
            runner: "cloud".to_string(),
            status: "running".to_string(),
            current_company: "Acme".to_string(),
            current_step: "Ready to submit".to_string(),
            application_id: Some(application_id.to_string()),
            takeover_url: None,
            created_at_ms: TEST_NOW_MS - 100,
            updated_at_ms: TEST_NOW_MS - 10,
        };
        let conn = fixture.pool.get().unwrap();
        conn.execute(
            "INSERT INTO accounts (id, email, password_hash, trial_seconds_remaining)
             VALUES (?1, 'certified-cloud@example.test', 'hash', 0)",
            params![account_id],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO jobs_tracks (
                id, account_id, track_json, active, created_at_ms, updated_at_ms
             ) VALUES ('track-1', ?1, '{}', 1, ?2, ?2)",
            params![account_id, TEST_NOW_MS - 100],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO jobs_postings (
                id, account_id, canonical_key, posting_json, source, canonical_url,
                company, title, location, match_score, status, created_at_ms, updated_at_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                posting.id,
                account_id,
                posting.canonical_key,
                serde_json::to_string(&posting).unwrap(),
                posting.source,
                posting.canonical_url,
                posting.company,
                posting.title,
                posting.location,
                posting.match_score,
                posting.status,
                posting.created_at_ms,
                posting.updated_at_ms,
            ],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO jobs_applications (
                id, account_id, job_id, resume_version_id, state, application_json,
                created_at_ms, updated_at_ms, submitted_at_ms
             ) VALUES (?1, ?2, ?3, NULL, 'running', ?4, ?5, ?6, NULL)",
            params![
                application_id,
                account_id,
                application.job_id,
                serde_json::to_string(&application).unwrap(),
                application.created_at_ms,
                application.updated_at_ms,
            ],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO jobs_browser_sessions (
                id, account_id, runner, status, session_json, created_at_ms, updated_at_ms
             ) VALUES (?1, ?2, 'cloud', 'running', ?3, ?4, ?5)",
            params![
                run_id,
                account_id,
                serde_json::to_string(&session).unwrap(),
                session.created_at_ms,
                session.updated_at_ms,
            ],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO jobs_auto_submit_authorizations (
                id, account_id, career_track_id, application_identity_id,
                source_resume_asset_id, authority_fingerprint, revision_no,
                authorized_at_ms, revoked_at_ms
             ) VALUES (?1, ?2, 'track-1', 'identity-cloud-1', 'resume-source-cloud-1',
                       ?3, 1, ?4, NULL)",
            params![
                auto_authorization_id,
                account_id,
                auto_authorization_fingerprint,
                TEST_NOW_MS - 20,
            ],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO jobs_attempt_reservations (
                id, account_id, application_id, company_key, period_key, runner, status,
                reserved_at_ms, updated_at_ms
             ) VALUES (?1, ?2, ?3, 'acme', '2033-05-18', 'cloud', 'running', ?4, ?4)",
            params![attempt_id, account_id, application_id, TEST_NOW_MS - 10],
        )
        .unwrap();
        drop(conn);

        let phase_a_request = AtsCertificationPhaseAContextRequest {
            account_id: account_id.to_string(),
            application_id: application_id.to_string(),
            run_id: run_id.to_string(),
            browser_session_id: run_id.to_string(),
            browser_profile_id: browser_profile_id.to_string(),
            runtime_attestation: cloud_runtime_attestation(),
            nonce_sha256: "9".repeat(64),
        };
        let phase_a = {
            let mut conn = fixture.pool.get().unwrap();
            let tx = conn
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .unwrap();
            let result = create_ats_application_certification_binding_from_context_sqlite_tx(
                &tx,
                &phase_a_request,
                TEST_NOW_MS,
            )
            .expect("certified cloud Phase A");
            tx.commit().unwrap();
            result
        };
        let selected_runtime = phase_a
            .authority
            .certification
            .selected_runtime
            .as_ref()
            .expect("Phase A selected exact cloud runtime");
        assert_eq!(selected_runtime.runtime_kind, "cloud");
        assert_eq!(
            selected_runtime.runner_build_id.as_deref(),
            Some("runner-604.1")
        );
        assert_eq!(
            selected_runtime.runner_image_sha256.as_deref(),
            Some("3333333333333333333333333333333333333333333333333333333333333333")
        );
        assert_eq!((phase_a.phase.as_str(), phase_a.fence), ("preflight", 0));

        let phase_b_request = AtsCertificationPhaseBContextRequest {
            account_id: account_id.to_string(),
            application_id: application_id.to_string(),
            run_id: run_id.to_string(),
            runner_kind: "cloud".to_string(),
            observed_surface: surface,
            terminal_phase: "consumed".to_string(),
        };
        let phase_b = {
            let mut conn = fixture.pool.get().unwrap();
            let tx = conn
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .unwrap();
            let result =
                validate_consume_reserve_ats_application_certification_from_context_sqlite_tx(
                    &tx,
                    &phase_b_request,
                    TEST_NOW_MS + 1,
                )
                .expect("certified cloud Phase B transaction")
                .into_result()
                .expect("certified cloud Phase B");
            tx.commit().unwrap();
            result
        };
        let receipt = phase_b.ats_certified_receipt_authority.clone();
        assert_eq!(receipt.runner_kind, "cloud");
        assert_eq!(receipt.runner_target_sha256, "1".repeat(64));
        assert_eq!(receipt.binding_sha256, phase_a.binding_sha256);
        assert_eq!(receipt.binding_fence, 1);
        let (phase, fence, reservation_count): (String, i64, i64) = fixture
            .pool
            .get()
            .unwrap()
            .query_row(
                "SELECT binding.phase, binding.fence,
                        (SELECT COUNT(*) FROM jobs_ats_certification_canary_reservations)
                   FROM jobs_application_ats_certification_bindings binding
                  WHERE binding.binding_id = ?1",
                params![phase_a.authority.binding_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(
            (phase.as_str(), fence, reservation_count),
            ("consumed", 1, 1)
        );

        let revocation = AtsCertificationRevocationAuthority {
            version: 1,
            audience: ATS_CERTIFICATION_REVOCATION_AUDIENCE.to_string(),
            revocation_id: "revoke-cloud-manifest-after-consume".to_string(),
            policy_sha256: fixture.policy_sha256.clone(),
            revocation_generation: 1,
            predecessor_revocation_sha256: None,
            subject_kind: "manifest".to_string(),
            subject_id: fixture.manifest.certification_id.clone(),
            subject_sha256: fixture.manifest_sha256.clone(),
            reason_ref: "post-consume-runtime-revocation".to_string(),
            issued_at_ms: TEST_NOW_MS + 2,
            effective_at_ms: TEST_NOW_MS + 2,
        };
        let revocation_envelope = envelope(
            &revocation,
            "revocation",
            ATS_CERTIFICATION_REVOCATION_AUDIENCE,
            revocation.issued_at_ms,
            &fixture.authority,
            "authorize-cloud-revocation-after-consume",
        );
        import_ats_certification_revocation_at(
            &fixture.pool,
            &revocation_envelope,
            "incident-responder",
            TEST_NOW_MS + 2,
        )
        .unwrap();

        assert!(resolve_active_ats_certification(
            &fixture.pool,
            &test_target_evidence().canonical_url,
            None,
            Some(&test_surface()),
            TEST_NOW_MS + 3,
        )
        .unwrap()
        .is_none());
        assert!(matches!(
            create_ats_application_certification_binding(
                &fixture.pool,
                &test_binding_request("binding-after-cloud-revocation", 'd'),
                TEST_NOW_MS + 3,
            ),
            Err(AtsCertificationAuthorityError::ScopeMismatch)
        ));

        let recovered = recover_ats_application_certification(
            &fixture.pool,
            &AtsCertificationRecoveryRequest {
                binding_id: phase_a.authority.binding_id,
                account_id: phase_a.authority.account_id,
                application_id: phase_a.authority.application_id,
                run_id: phase_a.authority.run_id,
                application_attempt_id: phase_a.authority.application_attempt_id,
                nonce_sha256: phase_a.authority.nonce_sha256,
            },
        )
        .expect("post-revocation exact terminal recovery");
        assert_eq!(recovered, phase_b);
        assert_eq!(recovered.ats_certified_receipt_authority, receipt);
    }

    #[test]
    fn phase_a_b_and_recovery_preserve_signed_observation_and_surface_separately() {
        let fixture = imported_fixture();
        let binding_request = test_binding_request("binding-1", '9');
        let first = create_ats_application_certification_binding(
            &fixture.pool,
            &binding_request,
            TEST_NOW_MS,
        )
        .unwrap();
        assert!(!first.replayed);
        assert_eq!(first.phase, "preflight");
        let replay = create_ats_application_certification_binding(
            &fixture.pool,
            &binding_request,
            TEST_NOW_MS + 1,
        )
        .unwrap();
        assert!(replay.replayed);
        assert_eq!(replay.binding_sha256, first.binding_sha256);

        let observation_sha256 = fixture
            .manifest
            .certification_profile
            .layout_observation_sha256s[0]
            .clone();
        assert_ne!(observation_sha256, test_surface().surface_sha256);
        let phase_b = AtsCertificationPhaseBRequest {
            binding_id: binding_request.binding_id.clone(),
            account_id: binding_request.account_id.clone(),
            application_id: binding_request.application_id.clone(),
            run_id: binding_request.run_id.clone(),
            application_attempt_id: binding_request.application_attempt_id.clone(),
            packet_checksum_sha256: binding_request.packet_checksum_sha256.clone(),
            auto_authorization_id: binding_request.auto_authorization_id.clone(),
            auto_authorization_revision: binding_request.auto_authorization_revision,
            auto_authorization_fingerprint_sha256: binding_request
                .auto_authorization_fingerprint_sha256
                .clone(),
            target_evidence: test_target_evidence(),
            runner_id: binding_request.runner_id.clone(),
            nonce_sha256: binding_request.nonce_sha256.clone(),
            observed_surface: test_surface(),
            phase_b_request_id: "phase-b-request-1".to_string(),
            metering_reservation_sha256: "a".repeat(64),
            canary_reservation_id: "reservation-1".to_string(),
            period_key: "2033-05-18".to_string(),
            expected_fence: 0,
            terminal_phase: "consumed".to_string(),
        };
        let consumed = validate_consume_reserve_ats_application_certification(
            &fixture.pool,
            &phase_b,
            TEST_NOW_MS + 2,
        )
        .unwrap();
        let receipt = &consumed.ats_certified_receipt_authority;
        assert_eq!(receipt.layout_observation_sha256, observation_sha256);
        assert_eq!(
            receipt.observed_surface_sha256,
            test_surface().surface_sha256
        );
        assert_ne!(
            receipt.layout_observation_sha256,
            receipt.observed_surface_sha256
        );
        assert_eq!(
            lookup_ats_certification_terminal_receipt_authority(
                &fixture.pool,
                &binding_request.account_id,
                &binding_request.application_id,
                &binding_request.run_id,
                receipt,
            )
            .unwrap(),
            receipt.clone()
        );
        let mut mismatched_receipt = receipt.clone();
        mismatched_receipt.metering_reservation_sha256 = "f".repeat(64);
        assert!(matches!(
            lookup_ats_certification_terminal_receipt_authority(
                &fixture.pool,
                &binding_request.account_id,
                &binding_request.application_id,
                &binding_request.run_id,
                &mismatched_receipt,
            ),
            Err(AtsCertificationAuthorityError::ScopeMismatch)
        ));
        assert!(validate_consume_reserve_ats_application_certification(
            &fixture.pool,
            &phase_b,
            TEST_NOW_MS + 3,
        )
        .is_err());

        let recovered = recover_ats_application_certification(
            &fixture.pool,
            &AtsCertificationRecoveryRequest {
                binding_id: binding_request.binding_id,
                account_id: binding_request.account_id,
                application_id: binding_request.application_id,
                run_id: binding_request.run_id,
                application_attempt_id: binding_request.application_attempt_id,
                nonce_sha256: binding_request.nonce_sha256,
            },
        )
        .unwrap();
        assert_eq!(recovered, consumed);
        let reservation_count: i64 = fixture
            .pool
            .get()
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM jobs_ats_certification_canary_reservations",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(reservation_count, 1);
    }

    #[test]
    fn phase_b_surface_mismatch_commits_quarantine_and_circuit_only() {
        let fixture = imported_fixture();
        let binding_request = test_binding_request("binding-mismatch", 'b');
        create_ats_application_certification_binding(&fixture.pool, &binding_request, TEST_NOW_MS)
            .unwrap();
        let mut observed_surface = test_surface();
        observed_surface.surface_sha256 = "c".repeat(64);
        let phase_b = AtsCertificationPhaseBRequest {
            binding_id: binding_request.binding_id.clone(),
            account_id: binding_request.account_id.clone(),
            application_id: binding_request.application_id.clone(),
            run_id: binding_request.run_id.clone(),
            application_attempt_id: binding_request.application_attempt_id.clone(),
            packet_checksum_sha256: binding_request.packet_checksum_sha256.clone(),
            auto_authorization_id: binding_request.auto_authorization_id.clone(),
            auto_authorization_revision: binding_request.auto_authorization_revision,
            auto_authorization_fingerprint_sha256: binding_request
                .auto_authorization_fingerprint_sha256
                .clone(),
            target_evidence: test_target_evidence(),
            runner_id: binding_request.runner_id.clone(),
            nonce_sha256: binding_request.nonce_sha256.clone(),
            observed_surface,
            phase_b_request_id: "phase-b-request-mismatch".to_string(),
            metering_reservation_sha256: "d".repeat(64),
            canary_reservation_id: "reservation-mismatch".to_string(),
            period_key: "2033-05-18".to_string(),
            expected_fence: 0,
            terminal_phase: "consumed".to_string(),
        };
        assert!(matches!(
            validate_consume_reserve_ats_application_certification(
                &fixture.pool,
                &phase_b,
                TEST_NOW_MS + 1,
            ),
            Err(AtsCertificationAuthorityError::ScopeMismatch)
        ));
        let conn = fixture.pool.get().unwrap();
        let state: (
            String,
            i64,
            Option<i64>,
            Option<String>,
            i64,
            i64,
            i64,
            String,
            i64,
        ) = conn
            .query_row(
                "SELECT binding.phase, binding.fence, binding.consumed_at_ms,
                        binding.phase_b_request_id,
                        (SELECT COUNT(*) FROM jobs_ats_certification_canary_reservations),
                        (SELECT COUNT(*)
                           FROM jobs_ats_certification_runtime_layout_quarantine_evidence),
                        (SELECT COUNT(*) FROM jobs_ats_certification_circuit_events
                          WHERE scope_kind = 'runtime' AND subject_key = ?2),
                        (SELECT state FROM jobs_ats_certification_circuit_heads
                          WHERE scope_kind = 'runtime' AND subject_key = ?2),
                        (SELECT head_revision FROM jobs_ats_certification_circuit_heads
                          WHERE scope_kind = 'runtime' AND subject_key = ?2)
                   FROM jobs_application_ats_certification_bindings binding
                  WHERE binding.binding_id = ?1",
                params![binding_request.binding_id, binding_request.runner_id],
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
                    ))
                },
            )
            .unwrap();
        assert_eq!(
            state,
            (
                "preflight".to_string(),
                0,
                None,
                None,
                0,
                1,
                1,
                "opened".to_string(),
                1,
            )
        );
        assert!(matches!(
            validate_consume_reserve_ats_application_certification(
                &fixture.pool,
                &phase_b,
                TEST_NOW_MS + 2,
            ),
            Err(AtsCertificationAuthorityError::ScopeMismatch)
        ));
        let replay_counts: (i64, i64) = conn
            .query_row(
                "SELECT
                    (SELECT COUNT(*)
                       FROM jobs_ats_certification_runtime_layout_quarantine_evidence),
                    (SELECT COUNT(*) FROM jobs_ats_certification_circuit_events
                      WHERE scope_kind = 'runtime' AND subject_key = ?1)",
                params![binding_request.runner_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(replay_counts, (1, 1));
    }

    #[test]
    #[serial_test::serial]
    fn postgres_newer_activation_circuit_close_requires_exact_applied_successor() {
        let test_name = "postgres_newer_activation_circuit_close_requires_exact_applied_successor";
        let Some(pool) = postgres_test_pool_or_skip(test_name) else {
            return;
        };
        let suffix = uuid::Uuid::new_v4().simple().to_string();
        let fixture = isolated_postgres_fixture(pool, &suffix, 1, true);
        let first_head = apply_ats_certification_activation_at(
            &fixture.pool,
            &fixture.activation_sha256,
            0,
            None,
            "promoter",
            TEST_NOW_MS + 1,
        )
        .unwrap();
        assert!(first_head.replayed);

        let activation_open = AtsCertificationCircuitEvent {
            event_id: format!("postgres-activation-circuit-open-{suffix}"),
            scope_kind: "activation".to_string(),
            subject_key: fixture.activation_sha256.clone(),
            transition: "opened".to_string(),
            trigger_kind: "evidence_failure".to_string(),
            window_started_at_ms: TEST_NOW_MS + 1,
            window_ended_at_ms: TEST_NOW_MS + 1,
            failure_count: 1,
            sample_count: 1,
            threshold_count: 1,
            authority_ref: format!("postgres-activation-incident-{suffix}"),
            event_at_ms: TEST_NOW_MS + 1,
        };
        let broad_opens = [
            ("provider", fixture.manifest.provider.clone()),
            ("target", fixture.manifest.target_key.clone()),
            ("adapter", fixture.manifest.adapter_bundle_sha256.clone()),
            (
                "runtime",
                fixture.manifest.runtime_targets[0].runtime_sha256.clone(),
            ),
        ]
        .into_iter()
        .enumerate()
        .map(
            |(index, (scope_kind, subject_key))| AtsCertificationCircuitEvent {
                event_id: format!("postgres-broad-circuit-open-{index}-{suffix}"),
                scope_kind: scope_kind.to_string(),
                subject_key,
                authority_ref: format!("postgres-broad-incident-{index}-{suffix}"),
                ..activation_open.clone()
            },
        )
        .collect::<Vec<_>>();
        for open in std::iter::once(&activation_open).chain(broad_opens.iter()) {
            let mut conn = fixture.pool.get_pg().unwrap();
            let mut tx = conn.transaction().unwrap();
            append_ats_certification_circuit_event_postgres_tx(
                &mut tx,
                open,
                0,
                None,
                "incident-responder",
                TEST_NOW_MS + 1,
            )
            .unwrap();
            tx.commit().unwrap();
        }

        let forged_close = AtsCertificationCircuitEvent {
            event_id: format!("postgres-forged-newer-activation-close-{suffix}"),
            transition: "closed".to_string(),
            trigger_kind: "newer_activation".to_string(),
            authority_ref: ats_certification_sha256(
                format!("postgres-forged-newer-activation-{suffix}").as_bytes(),
            ),
            window_ended_at_ms: TEST_NOW_MS + 2,
            event_at_ms: TEST_NOW_MS + 2,
            ..activation_open.clone()
        };
        let mut conn = fixture.pool.get_pg().unwrap();
        let mut tx = conn.transaction().unwrap();
        assert!(matches!(
            append_ats_certification_circuit_event_postgres_tx(
                &mut tx,
                &forged_close,
                1,
                Some(&activation_open.event_id),
                "incident-responder",
                TEST_NOW_MS + 2,
            ),
            Err(AtsCertificationAuthorityError::InvalidAuthority)
        ));
        tx.rollback().unwrap();

        let mut successor = test_activation(
            fixture.manifest_sha256.clone(),
            fixture.manifest.scope_sha256.clone(),
            &fixture.policy_sha256,
        );
        successor.activation_id = format!("round604-activation-{suffix}-successor");
        successor.activation_generation = 2;
        successor.predecessor_activation_sha256 = Some(fixture.activation_sha256.clone());
        successor.channel_sequence = 2;
        successor.approval_ref = format!("round604-approval-{suffix}-successor");
        let successor_envelope = envelope(
            &successor,
            "activation",
            ATS_CERTIFICATION_ACTIVATION_AUDIENCE,
            successor.issued_at_ms,
            &fixture.authority,
            &format!("authorize-round604-activation-{suffix}-successor"),
        );
        import_ats_certification_activation_at(
            &fixture.pool,
            &successor_envelope,
            "promoter",
            TEST_NOW_MS + 2,
        )
        .unwrap();
        let successor_sha256 = envelope_sha256(&successor_envelope);
        let imported_only_close = AtsCertificationCircuitEvent {
            event_id: format!("postgres-imported-only-activation-close-{suffix}"),
            transition: "closed".to_string(),
            trigger_kind: "newer_activation".to_string(),
            authority_ref: successor_sha256.clone(),
            window_ended_at_ms: TEST_NOW_MS + 2,
            event_at_ms: TEST_NOW_MS + 2,
            ..activation_open.clone()
        };
        let mut conn = fixture.pool.get_pg().unwrap();
        let mut tx = conn.transaction().unwrap();
        assert!(matches!(
            append_ats_certification_circuit_event_postgres_tx(
                &mut tx,
                &imported_only_close,
                1,
                Some(&activation_open.event_id),
                "incident-responder",
                TEST_NOW_MS + 2,
            ),
            Err(AtsCertificationAuthorityError::InvalidAuthority)
        ));
        tx.rollback().unwrap();
        apply_ats_certification_activation_at(
            &fixture.pool,
            &successor_sha256,
            first_head.head_revision,
            Some(&first_head.transition_sha256),
            "promoter",
            TEST_NOW_MS + 2,
        )
        .unwrap();

        for (index, open) in broad_opens.iter().enumerate() {
            let automatic_close = AtsCertificationCircuitEvent {
                event_id: format!("postgres-broad-automatic-close-{index}-{suffix}"),
                transition: "closed".to_string(),
                trigger_kind: "newer_activation".to_string(),
                authority_ref: successor_sha256.clone(),
                window_ended_at_ms: TEST_NOW_MS + 3,
                event_at_ms: TEST_NOW_MS + 3,
                ..open.clone()
            };
            let mut conn = fixture.pool.get_pg().unwrap();
            let mut tx = conn.transaction().unwrap();
            assert!(matches!(
                append_ats_certification_circuit_event_postgres_tx(
                    &mut tx,
                    &automatic_close,
                    1,
                    Some(&open.event_id),
                    "incident-responder",
                    TEST_NOW_MS + 3,
                ),
                Err(AtsCertificationAuthorityError::InvalidAuthority)
            ));
            tx.rollback().unwrap();

            let reviewed_close = AtsCertificationCircuitEvent {
                event_id: format!("postgres-broad-reviewed-close-{index}-{suffix}"),
                transition: "closed".to_string(),
                trigger_kind: "reviewed_close".to_string(),
                authority_ref: format!("postgres-reviewed-close-{index}-{suffix}"),
                window_ended_at_ms: TEST_NOW_MS + 4,
                event_at_ms: TEST_NOW_MS + 4,
                ..open.clone()
            };
            let mut conn = fixture.pool.get_pg().unwrap();
            let mut tx = conn.transaction().unwrap();
            let closed = append_ats_certification_circuit_event_postgres_tx(
                &mut tx,
                &reviewed_close,
                1,
                Some(&open.event_id),
                "incident-responder",
                TEST_NOW_MS + 4,
            )
            .unwrap();
            tx.commit().unwrap();
            assert_eq!(closed.state, "closed");
        }

        let activation_close = AtsCertificationCircuitEvent {
            event_id: format!("postgres-valid-newer-activation-close-{suffix}"),
            transition: "closed".to_string(),
            trigger_kind: "newer_activation".to_string(),
            authority_ref: successor_sha256,
            window_ended_at_ms: TEST_NOW_MS + 3,
            event_at_ms: TEST_NOW_MS + 3,
            ..activation_open.clone()
        };
        let mut conn = fixture.pool.get_pg().unwrap();
        let mut tx = conn.transaction().unwrap();
        let closed = append_ats_certification_circuit_event_postgres_tx(
            &mut tx,
            &activation_close,
            1,
            Some(&activation_open.event_id),
            "incident-responder",
            TEST_NOW_MS + 3,
        )
        .unwrap();
        tx.commit().unwrap();
        assert_eq!(closed.state, "closed");
    }

    #[test]
    #[serial_test::serial]
    fn postgres_newer_activation_circuit_close_requires_current_runtime_authority() {
        let test_name =
            "postgres_newer_activation_circuit_close_requires_current_runtime_authority";
        let Some(pool) = postgres_test_pool_or_skip(test_name) else {
            return;
        };
        let suffix = uuid::Uuid::new_v4().simple().to_string();
        let fixture = isolated_postgres_fixture(pool, &suffix, 1, true);
        let first_head = apply_ats_certification_activation_at(
            &fixture.pool,
            &fixture.activation_sha256,
            0,
            None,
            "promoter",
            TEST_NOW_MS + 1,
        )
        .unwrap();
        assert!(first_head.replayed);
        let activation_open = AtsCertificationCircuitEvent {
            event_id: format!("postgres-current-runtime-activation-open-{suffix}"),
            scope_kind: "activation".to_string(),
            subject_key: fixture.activation_sha256.clone(),
            transition: "opened".to_string(),
            trigger_kind: "evidence_failure".to_string(),
            window_started_at_ms: TEST_NOW_MS + 1,
            window_ended_at_ms: TEST_NOW_MS + 1,
            failure_count: 1,
            sample_count: 1,
            threshold_count: 1,
            authority_ref: format!("postgres-current-runtime-incident-{suffix}"),
            event_at_ms: TEST_NOW_MS + 1,
        };
        let mut conn = fixture.pool.get_pg().unwrap();
        let mut tx = conn.transaction().unwrap();
        append_ats_certification_circuit_event_postgres_tx(
            &mut tx,
            &activation_open,
            0,
            None,
            "incident-responder",
            TEST_NOW_MS + 1,
        )
        .unwrap();
        tx.commit().unwrap();

        let mut successor = test_activation(
            fixture.manifest_sha256.clone(),
            fixture.manifest.scope_sha256.clone(),
            &fixture.policy_sha256,
        );
        successor.activation_id = format!("postgres-current-runtime-successor-{suffix}");
        successor.activation_generation = 2;
        successor.predecessor_activation_sha256 = Some(fixture.activation_sha256.clone());
        successor.channel_sequence = 2;
        successor.approval_ref = format!("postgres-current-runtime-approval-{suffix}");
        let successor_envelope = envelope(
            &successor,
            "activation",
            ATS_CERTIFICATION_ACTIVATION_AUDIENCE,
            successor.issued_at_ms,
            &fixture.authority,
            &format!("authorize-postgres-current-runtime-successor-{suffix}"),
        );
        import_ats_certification_activation_at(
            &fixture.pool,
            &successor_envelope,
            "promoter",
            TEST_NOW_MS + 2,
        )
        .unwrap();
        let successor_sha256 = envelope_sha256(&successor_envelope);
        apply_ats_certification_activation_at(
            &fixture.pool,
            &successor_sha256,
            first_head.head_revision,
            Some(&first_head.transition_sha256),
            "promoter",
            TEST_NOW_MS + 2,
        )
        .unwrap();

        let runtime = &fixture.manifest.runtime_targets[0];
        let (revocation_generation, predecessor_revocation_sha256) =
            next_postgres_revocation_generation(&fixture.pool, &fixture.policy_sha256);
        let runtime_revocation = AtsCertificationRevocationAuthority {
            version: 1,
            audience: ATS_CERTIFICATION_REVOCATION_AUDIENCE.to_string(),
            revocation_id: format!("postgres-revoke-current-runtime-{suffix}"),
            policy_sha256: fixture.policy_sha256.clone(),
            revocation_generation,
            predecessor_revocation_sha256,
            subject_kind: "runtime".to_string(),
            subject_id: runtime.runtime_id.clone(),
            subject_sha256: runtime.runtime_sha256.clone(),
            reason_ref: "postgres-current-successor-authority-test".to_string(),
            issued_at_ms: TEST_NOW_MS + 4,
            effective_at_ms: TEST_NOW_MS + 4,
        };
        let runtime_revocation_envelope = envelope(
            &runtime_revocation,
            "revocation",
            ATS_CERTIFICATION_REVOCATION_AUDIENCE,
            runtime_revocation.issued_at_ms,
            &fixture.authority,
            &format!("authorize-postgres-current-runtime-revocation-{suffix}"),
        );
        import_ats_certification_revocation_at(
            &fixture.pool,
            &runtime_revocation_envelope,
            "incident-responder",
            TEST_NOW_MS + 4,
        )
        .unwrap();
        let backdated_activation_close = AtsCertificationCircuitEvent {
            event_id: format!("postgres-backdated-activation-close-{suffix}"),
            transition: "closed".to_string(),
            trigger_kind: "newer_activation".to_string(),
            authority_ref: successor_sha256,
            window_ended_at_ms: TEST_NOW_MS + 3,
            event_at_ms: TEST_NOW_MS + 3,
            ..activation_open.clone()
        };
        let mut conn = fixture.pool.get_pg().unwrap();
        let mut tx = conn.transaction().unwrap();
        assert!(append_ats_certification_circuit_event_postgres_tx(
            &mut tx,
            &backdated_activation_close,
            1,
            Some(&activation_open.event_id),
            "incident-responder",
            TEST_NOW_MS + 5,
        )
        .is_err());
        tx.rollback().unwrap();
        let mut conn = fixture.pool.get_pg().unwrap();
        let state = conn
            .query_one(
                "SELECT state, head_revision FROM jobs_ats_certification_circuit_heads
                  WHERE scope_kind = 'activation' AND subject_key = $1",
                &[&activation_open.subject_key],
            )
            .unwrap();
        assert_eq!(state.get::<_, String>(0), "opened");
        assert_eq!(state.get::<_, i64>(1), 1);
    }

    #[test]
    #[serial_test::serial]
    fn postgres_sequence_two_replays_require_exact_predecessors() {
        let test_name = "postgres_sequence_two_replays_require_exact_predecessors";
        let Some(pool) = postgres_test_pool_or_skip(test_name) else {
            return;
        };
        let suffix = uuid::Uuid::new_v4().simple().to_string();
        let fixture = isolated_postgres_fixture(pool, &suffix, 1, true);
        assert_sequence_two_activation_replay_requires_exact_predecessor(&fixture, &suffix);
        assert_sequence_two_quarantine_replay_requires_exact_predecessor(&fixture, &suffix);
    }

    #[test]
    #[serial_test::serial]
    fn postgres_manifest_activation_head_cas_has_one_mutating_winner() {
        let test_name = "postgres_manifest_activation_head_cas_has_one_mutating_winner";
        let Some(pool) = postgres_test_pool_or_skip(test_name) else {
            return;
        };
        let suffix = uuid::Uuid::new_v4().simple().to_string();
        let fixture = isolated_postgres_fixture(pool, &suffix, 1, false);
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(3));
        let contenders = (0..2)
            .map(|_| {
                let pool = fixture.pool.clone();
                let activation_sha256 = fixture.activation_sha256.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    apply_ats_certification_activation_at(
                        &pool,
                        &activation_sha256,
                        0,
                        None,
                        "round604-head-promoter",
                        TEST_NOW_MS,
                    )
                })
            })
            .collect::<Vec<_>>();
        barrier.wait();
        let outcomes = contenders
            .into_iter()
            .map(|contender| contender.join().expect("head contender did not panic"))
            .collect::<Vec<_>>();
        assert!(
            outcomes.iter().all(Result::is_ok),
            "exact head CAS retry must be replay-safe: {outcomes:?}",
        );
        let results = outcomes.into_iter().map(Result::unwrap).collect::<Vec<_>>();
        assert_eq!(results.iter().filter(|result| !result.replayed).count(), 1);
        assert_eq!(results.iter().filter(|result| result.replayed).count(), 1);
        assert_eq!(results[0].transition_sha256, results[1].transition_sha256);

        let mut conn = fixture.pool.get_pg().unwrap();
        let row = conn
            .query_one(
                "SELECT head.head_revision, head.current_activation_sha256,
                        (SELECT COUNT(*) FROM jobs_ats_certification_head_transitions transition
                          WHERE transition.scope_sha256 = head.scope_sha256
                            AND transition.channel = head.channel)
                   FROM jobs_ats_certification_heads head
                  WHERE head.scope_sha256 = $1 AND head.channel = 'general'",
                &[&fixture.manifest.scope_sha256],
            )
            .unwrap();
        assert_eq!(row.get::<_, i64>(0), 1);
        assert_eq!(row.get::<_, String>(1), fixture.activation_sha256);
        assert_eq!(row.get::<_, i64>(2), 1);
    }

    #[test]
    #[serial_test::serial]
    fn postgres_concurrent_canary_phase_b_enforces_signed_concurrency_cap_atomically() {
        let test_name =
            "postgres_concurrent_canary_phase_b_enforces_signed_concurrency_cap_atomically";
        let Some(pool) = postgres_test_pool_or_skip(test_name) else {
            return;
        };
        let suffix = uuid::Uuid::new_v4().simple().to_string();
        let fixture = isolated_postgres_fixture(pool, &suffix, 1, true);
        let identities = [format!("{suffix}-first"), format!("{suffix}-second")];
        let mut binding_requests = identities
            .iter()
            .enumerate()
            .map(|(index, identity)| {
                let mut request = isolated_postgres_binding_request(
                    &fixture,
                    identity,
                    &format!("canary-{index}"),
                    0,
                );
                request.rollout_channel = "canary".to_string();
                request
            })
            .collect::<Vec<_>>();
        let canary_activation_sha256 = install_isolated_canary_activation(
            &fixture,
            &suffix,
            binding_requests
                .iter()
                .map(|request| request.account_id.clone())
                .collect(),
            (2, 2, 1, 2),
        );
        for request in &binding_requests {
            let binding =
                create_ats_application_certification_binding(&fixture.pool, request, TEST_NOW_MS)
                    .unwrap();
            assert_eq!(binding.authority.certification.channel, "canary");
            assert_eq!(binding.authority.certification.canary_max_submissions, 2);
            assert_eq!(binding.authority.certification.canary_account_cap, 2);
            assert_eq!(binding.authority.certification.canary_concurrency_cap, 1);
            assert_eq!(
                binding.authority.certification.canary_daily_side_effect_cap,
                2
            );
        }

        let phase_b_now_ms = TEST_NOW_MS + 1;
        let server_period_key = ats_certification_canary_utc_period_key(phase_b_now_ms).unwrap();
        let phase_b_requests = binding_requests
            .drain(..)
            .enumerate()
            .map(|(index, binding)| {
                let mut request =
                    test_phase_b_request(&binding, &format!("postgres-canary-{suffix}-{index}"));
                request.observed_surface = fixture.surface.clone();
                request.period_key = server_period_key.clone();
                request
            })
            .collect::<Vec<_>>();
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(3));
        let contenders = phase_b_requests
            .iter()
            .cloned()
            .map(|request| {
                let pool = fixture.pool.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    validate_consume_reserve_ats_application_certification(
                        &pool,
                        &request,
                        phase_b_now_ms,
                    )
                })
            })
            .collect::<Vec<_>>();
        barrier.wait();
        let outcomes = contenders
            .into_iter()
            .map(|contender| contender.join().expect("canary contender did not panic"))
            .collect::<Vec<_>>();
        assert_eq!(
            outcomes.iter().filter(|outcome| outcome.is_ok()).count(),
            1,
            "exactly one Phase B may reserve the signed concurrency slot: {outcomes:?}",
        );
        assert_eq!(
            outcomes
                .iter()
                .filter(|outcome| {
                    matches!(
                        outcome,
                        Err(AtsCertificationAuthorityError::CapacityUnavailable)
                    )
                })
                .count(),
            1,
            "the serialized loser must observe signed canary capacity: {outcomes:?}",
        );
        let denied_index = outcomes
            .iter()
            .position(|outcome| {
                matches!(
                    outcome,
                    Err(AtsCertificationAuthorityError::CapacityUnavailable)
                )
            })
            .unwrap();
        let denied_request = &phase_b_requests[denied_index];

        let mut conn = fixture.pool.get_pg().unwrap();
        let binding_row = conn
            .query_one(
                "SELECT phase, fence, layout_observation_sha256, phase_b_request_id,
                        phase_b_request_sha256, canary_reservation_sha256,
                        metering_reservation_sha256, observed_surface_sha256, consumed_at_ms
                   FROM jobs_application_ats_certification_bindings
                  WHERE binding_id = $1",
                &[&denied_request.binding_id],
            )
            .unwrap();
        assert_eq!(binding_row.get::<_, String>(0), "preflight");
        assert_eq!(binding_row.get::<_, i64>(1), 0);
        for column in 2..8 {
            assert_eq!(binding_row.get::<_, Option<String>>(column), None);
        }
        assert_eq!(binding_row.get::<_, Option<i64>>(8), None);

        let reservation_row = conn
            .query_one(
                "SELECT COUNT(*),
                        COUNT(*) FILTER (WHERE binding_id = $2),
                        COUNT(*) FILTER (
                          WHERE period_key = $3 AND status = 'reserved' AND fence = 1
                            AND released_at_ms IS NULL
                        ),
                        MIN(period_key)
                   FROM jobs_ats_certification_canary_reservations
                  WHERE activation_sha256 = $1",
                &[
                    &canary_activation_sha256,
                    &denied_request.binding_id,
                    &server_period_key,
                ],
            )
            .unwrap();
        assert_eq!(reservation_row.get::<_, i64>(0), 1);
        assert_eq!(reservation_row.get::<_, i64>(1), 0);
        assert_eq!(reservation_row.get::<_, i64>(2), 1);
        assert_eq!(
            reservation_row.get::<_, Option<String>>(3).as_deref(),
            Some(server_period_key.as_str())
        );
    }

    #[test]
    #[serial_test::serial]
    fn postgres_phase_a_two_runner_binding_race_has_one_winner() {
        let test_name = "postgres_phase_a_two_runner_binding_race_has_one_winner";
        let Some(pool) = postgres_test_pool_or_skip(test_name) else {
            return;
        };
        let suffix = uuid::Uuid::new_v4().simple().to_string();
        let fixture = isolated_postgres_fixture(pool, &suffix, 2, true);
        let first = isolated_postgres_binding_request(&fixture, &suffix, "first", 0);
        let second = isolated_postgres_binding_request(&fixture, &suffix, "second", 1);
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(3));
        let contenders = [first, second]
            .into_iter()
            .map(|request| {
                let pool = fixture.pool.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    create_ats_application_certification_binding(&pool, &request, TEST_NOW_MS)
                })
            })
            .collect::<Vec<_>>();
        barrier.wait();
        let outcomes = contenders
            .into_iter()
            .map(|contender| contender.join().expect("Phase A contender did not panic"))
            .collect::<Vec<_>>();
        assert_eq!(
            outcomes.iter().filter(|outcome| outcome.is_ok()).count(),
            1,
            "exactly one runner may bind the application attempt: {outcomes:?}",
        );
        assert_eq!(
            outcomes.iter().filter(|outcome| outcome.is_err()).count(),
            1
        );
        let winner = outcomes
            .into_iter()
            .find_map(Result::ok)
            .expect("one Phase A contender must win");
        assert!(!winner.replayed);

        let mut conn = fixture.pool.get_pg().unwrap();
        let row = conn
            .query_one(
                "SELECT COUNT(*), MIN(runner_target_sha256)
                   FROM jobs_application_ats_certification_bindings
                  WHERE account_id = $1 AND application_id = $2 AND attempt_id = $3",
                &[
                    &winner.authority.account_id,
                    &winner.authority.application_id,
                    &winner.authority.application_attempt_id,
                ],
            )
            .unwrap();
        assert_eq!(row.get::<_, i64>(0), 1);
        assert_eq!(
            row.get::<_, Option<String>>(1).as_deref(),
            winner
                .authority
                .certification
                .selected_runtime
                .as_ref()
                .map(|runtime| runtime.runtime_sha256.as_str()),
        );
    }

    #[test]
    #[serial_test::serial]
    fn postgres_phase_b_consume_recovery_and_revocation_match_sqlite() {
        let test_name = "postgres_phase_b_consume_recovery_and_revocation_match_sqlite";
        let Some(pool) = postgres_test_pool_or_skip(test_name) else {
            return;
        };
        let suffix = uuid::Uuid::new_v4().simple().to_string();
        let fixture = isolated_postgres_fixture(pool, &suffix, 1, true);
        let binding = isolated_postgres_binding_request(&fixture, &suffix, "consume", 0);
        create_ats_application_certification_binding(&fixture.pool, &binding, TEST_NOW_MS).unwrap();
        let mut phase_b = test_phase_b_request(&binding, &format!("round604-phase-b-{suffix}"));
        phase_b.observed_surface = fixture.surface.clone();
        let consumed = validate_consume_reserve_ats_application_certification(
            &fixture.pool,
            &phase_b,
            TEST_NOW_MS + 1,
        )
        .unwrap();
        assert_eq!(consumed.terminal_phase, "consumed");
        assert_eq!(consumed.ats_certified_receipt_authority.binding_fence, 1);

        let recovery = AtsCertificationRecoveryRequest {
            binding_id: binding.binding_id.clone(),
            account_id: binding.account_id.clone(),
            application_id: binding.application_id.clone(),
            run_id: binding.run_id.clone(),
            application_attempt_id: binding.application_attempt_id.clone(),
            nonce_sha256: binding.nonce_sha256.clone(),
        };
        assert_eq!(
            recover_ats_application_certification(&fixture.pool, &recovery).unwrap(),
            consumed,
        );
        let receipt = consumed.ats_certified_receipt_authority.clone();
        assert_eq!(
            lookup_ats_certification_terminal_receipt_authority(
                &fixture.pool,
                &binding.account_id,
                &binding.application_id,
                &binding.run_id,
                &receipt,
            )
            .unwrap(),
            receipt,
        );

        let mut conn = fixture.pool.get_pg().unwrap();
        let row = conn
            .query_one(
                "SELECT binding.phase, binding.fence, binding.consumed_at_ms,
                        (SELECT COUNT(*) FROM jobs_ats_certification_canary_reservations
                          WHERE binding_id = $1)
                   FROM jobs_application_ats_certification_bindings binding
                  WHERE binding.binding_id = $1",
                &[&binding.binding_id],
            )
            .unwrap();
        assert_eq!(row.get::<_, String>(0), "consumed");
        assert_eq!(row.get::<_, i64>(1), 1);
        assert_eq!(row.get::<_, Option<i64>>(2), Some(TEST_NOW_MS + 1));
        assert_eq!(row.get::<_, i64>(3), 1);
        drop(conn);

        let pending_identity = format!("{suffix}-pending");
        let pending = isolated_postgres_binding_request(&fixture, &pending_identity, "phase-b", 0);
        create_ats_application_certification_binding(&fixture.pool, &pending, TEST_NOW_MS + 1)
            .unwrap();
        let mut pending_phase_b =
            test_phase_b_request(&pending, &format!("round604-pending-phase-b-{suffix}"));
        pending_phase_b.observed_surface = fixture.surface.clone();

        let (revocation_generation, predecessor_revocation_sha256) =
            next_postgres_revocation_generation(&fixture.pool, &fixture.policy_sha256);
        let revocation = AtsCertificationRevocationAuthority {
            version: 1,
            audience: ATS_CERTIFICATION_REVOCATION_AUDIENCE.to_string(),
            revocation_id: format!("round604-revoke-target-{suffix}"),
            policy_sha256: fixture.policy_sha256.clone(),
            revocation_generation,
            predecessor_revocation_sha256: predecessor_revocation_sha256.clone(),
            subject_kind: "target".to_string(),
            subject_id: fixture.manifest.target_key.clone(),
            subject_sha256: ats_certification_target_key_sha256(&fixture.manifest.target_key)
                .unwrap(),
            reason_ref: "round604-post-consume-parity".to_string(),
            issued_at_ms: TEST_NOW_MS + 2,
            effective_at_ms: TEST_NOW_MS + 2,
        };
        let revocation_envelope = envelope(
            &revocation,
            "revocation",
            ATS_CERTIFICATION_REVOCATION_AUDIENCE,
            revocation.issued_at_ms,
            &fixture.authority,
            &format!("authorize-round604-revocation-{suffix}"),
        );
        let revocation_result = import_ats_certification_revocation_at(
            &fixture.pool,
            &revocation_envelope,
            "incident-responder",
            TEST_NOW_MS + 2,
        )
        .unwrap();
        assert!(!revocation_result.replayed);
        assert!(
            import_ats_certification_revocation_at(
                &fixture.pool,
                &revocation_envelope,
                "other-incident-responder",
                TEST_NOW_MS + 2,
            )
            .unwrap()
            .replayed
        );
        let mut conn = fixture.pool.get_pg().unwrap();
        let row = conn
            .query_one(
                "SELECT revocation_generation, predecessor_revocation_sha256
                   FROM jobs_ats_certification_revocations
                  WHERE revocation_sha256 = $1",
                &[&revocation_result.authority_sha256],
            )
            .unwrap();
        assert_eq!(row.get::<_, i64>(0), revocation_generation);
        assert_eq!(
            row.get::<_, Option<String>>(1),
            predecessor_revocation_sha256
        );
        drop(conn);

        let status = get_ats_certification_target_status_projection(
            &fixture.pool,
            &fixture.target_evidence,
            Some(&fixture.manifest.runtime_targets[0].runtime_sha256),
            "general",
            None,
            TEST_NOW_MS + 3,
        )
        .unwrap();
        assert_eq!(status.status, "revoked");
        assert!(validate_consume_reserve_ats_application_certification(
            &fixture.pool,
            &pending_phase_b,
            TEST_NOW_MS + 3,
        )
        .is_err());

        let post_revocation_identity = format!("{suffix}-post-revocation");
        let post_revocation =
            isolated_postgres_binding_request(&fixture, &post_revocation_identity, "new", 0);
        assert!(create_ats_application_certification_binding(
            &fixture.pool,
            &post_revocation,
            TEST_NOW_MS + 3,
        )
        .is_err());
        assert_eq!(
            recover_ats_application_certification(&fixture.pool, &recovery).unwrap(),
            consumed,
        );
        assert_eq!(
            lookup_ats_certification_terminal_receipt_authority(
                &fixture.pool,
                &binding.account_id,
                &binding.application_id,
                &binding.run_id,
                &receipt,
            )
            .unwrap(),
            receipt,
        );
        let mut mismatched_receipt = receipt;
        mismatched_receipt.metering_reservation_sha256 = "f".repeat(64);
        assert!(matches!(
            lookup_ats_certification_terminal_receipt_authority(
                &fixture.pool,
                &binding.account_id,
                &binding.application_id,
                &binding.run_id,
                &mismatched_receipt,
            ),
            Err(AtsCertificationAuthorityError::ScopeMismatch)
        ));
    }

    #[test]
    #[serial_test::serial]
    fn postgres_phase_b_surface_mismatch_commits_quarantine_and_circuit_only() {
        let test_name = "postgres_phase_b_surface_mismatch_commits_quarantine_and_circuit_only";
        let Some(pool) = postgres_test_pool_or_skip(test_name) else {
            return;
        };
        let fixture = imported_fixture_for_runtime_in(pool, false);
        let suffix = uuid::Uuid::new_v4().simple().to_string();
        let binding_id = format!("binding-postgres-drift-{suffix}");
        let mut binding_request = test_binding_request(&binding_id, 'e');
        binding_request.nonce_sha256 = ats_certification_sha256(suffix.as_bytes());
        create_ats_application_certification_binding(&fixture.pool, &binding_request, TEST_NOW_MS)
            .unwrap();
        let observed_surface_sha256 =
            ats_certification_sha256(format!("postgres-unknown-layout-{suffix}").as_bytes());
        let mut observed_surface = test_surface();
        observed_surface.surface_sha256 = observed_surface_sha256.clone();
        let phase_b = AtsCertificationPhaseBRequest {
            binding_id: binding_request.binding_id.clone(),
            account_id: binding_request.account_id.clone(),
            application_id: binding_request.application_id.clone(),
            run_id: binding_request.run_id.clone(),
            application_attempt_id: binding_request.application_attempt_id.clone(),
            packet_checksum_sha256: binding_request.packet_checksum_sha256.clone(),
            auto_authorization_id: binding_request.auto_authorization_id.clone(),
            auto_authorization_revision: binding_request.auto_authorization_revision,
            auto_authorization_fingerprint_sha256: binding_request
                .auto_authorization_fingerprint_sha256
                .clone(),
            target_evidence: test_target_evidence(),
            runner_id: binding_request.runner_id.clone(),
            nonce_sha256: binding_request.nonce_sha256.clone(),
            observed_surface,
            phase_b_request_id: format!("phase-b-postgres-drift-{suffix}"),
            metering_reservation_sha256: ats_certification_sha256(
                format!("postgres-metering-{suffix}").as_bytes(),
            ),
            canary_reservation_id: format!("reservation-postgres-drift-{suffix}"),
            period_key: "2033-05-18".to_string(),
            expected_fence: 0,
            terminal_phase: "consumed".to_string(),
        };
        assert!(matches!(
            validate_consume_reserve_ats_application_certification(
                &fixture.pool,
                &phase_b,
                TEST_NOW_MS + 1,
            ),
            Err(AtsCertificationAuthorityError::ScopeMismatch)
        ));
        let mut conn = fixture.pool.get_pg().unwrap();
        let row = conn
            .query_one(
                "SELECT binding.phase, binding.fence, binding.consumed_at_ms,
                        binding.phase_b_request_id,
                        (SELECT COUNT(*) FROM jobs_ats_certification_canary_reservations
                          WHERE binding_id = $1),
                        (SELECT COUNT(*)
                           FROM jobs_ats_certification_runtime_layout_quarantine_evidence
                          WHERE activation_sha256 = $2 AND runtime_sha256 = $3
                            AND observed_surface_sha256 = $4),
                        (SELECT state FROM jobs_ats_certification_circuit_heads
                          WHERE scope_kind = 'runtime' AND subject_key = $3)
                   FROM jobs_application_ats_certification_bindings binding
                  WHERE binding.binding_id = $1",
                &[
                    &binding_request.binding_id,
                    &fixture.activation_sha256,
                    &binding_request.runner_id,
                    &observed_surface_sha256,
                ],
            )
            .unwrap();
        assert_eq!(row.get::<_, String>(0), "preflight");
        assert_eq!(row.get::<_, i64>(1), 0);
        assert_eq!(row.get::<_, Option<i64>>(2), None);
        assert_eq!(row.get::<_, Option<String>>(3), None);
        assert_eq!(row.get::<_, i64>(4), 0);
        assert_eq!(row.get::<_, i64>(5), 1);
        assert!(matches!(
            row.get::<_, String>(6).as_str(),
            "opened" | "held"
        ));

        let evidence_sha256: String = conn
            .query_one(
                "SELECT evidence_sha256
                   FROM jobs_ats_certification_runtime_layout_quarantine_evidence
                  WHERE activation_sha256 = $1 AND runtime_sha256 = $2
                    AND observed_surface_sha256 = $3",
                &[
                    &fixture.activation_sha256,
                    &binding_request.runner_id,
                    &observed_surface_sha256,
                ],
            )
            .unwrap()
            .get(0);
        assert!(conn
            .execute(
                "UPDATE jobs_ats_certification_runtime_layout_quarantine_evidence
                    SET recorded_by = 'mutated' WHERE evidence_sha256 = $1",
                &[&evidence_sha256],
            )
            .is_err());
        assert!(conn
            .execute(
                "DELETE FROM jobs_ats_certification_runtime_layout_quarantine_evidence
                  WHERE evidence_sha256 = $1",
                &[&evidence_sha256],
            )
            .is_err());
    }

    #[test]
    fn phase_b_runtime_layout_quarantine_is_append_only_and_bounded() {
        let fixture = imported_fixture();
        let binding_request = test_binding_request("binding-bounded-drift", 'd');
        create_ats_application_certification_binding(&fixture.pool, &binding_request, TEST_NOW_MS)
            .unwrap();
        let mut phase_b = AtsCertificationPhaseBRequest {
            binding_id: binding_request.binding_id.clone(),
            account_id: binding_request.account_id.clone(),
            application_id: binding_request.application_id.clone(),
            run_id: binding_request.run_id.clone(),
            application_attempt_id: binding_request.application_attempt_id.clone(),
            packet_checksum_sha256: binding_request.packet_checksum_sha256.clone(),
            auto_authorization_id: binding_request.auto_authorization_id.clone(),
            auto_authorization_revision: binding_request.auto_authorization_revision,
            auto_authorization_fingerprint_sha256: binding_request
                .auto_authorization_fingerprint_sha256
                .clone(),
            target_evidence: test_target_evidence(),
            runner_id: binding_request.runner_id.clone(),
            nonce_sha256: binding_request.nonce_sha256.clone(),
            observed_surface: test_surface(),
            phase_b_request_id: String::new(),
            metering_reservation_sha256: String::new(),
            canary_reservation_id: String::new(),
            period_key: "2033-05-18".to_string(),
            expected_fence: 0,
            terminal_phase: "consumed".to_string(),
        };
        for index in 0..(ATS_CERTIFICATION_MAX_RUNTIME_LAYOUT_QUARANTINE_EVIDENCE + 2) {
            phase_b.observed_surface.surface_sha256 =
                ats_certification_sha256(format!("unknown-layout-{index}").as_bytes());
            phase_b.phase_b_request_id = format!("phase-b-bounded-{index}");
            phase_b.metering_reservation_sha256 =
                ats_certification_sha256(format!("metering-{index}").as_bytes());
            phase_b.canary_reservation_id = format!("reservation-bounded-{index}");
            assert!(matches!(
                validate_consume_reserve_ats_application_certification(
                    &fixture.pool,
                    &phase_b,
                    TEST_NOW_MS + 1,
                ),
                Err(AtsCertificationAuthorityError::ScopeMismatch)
            ));
        }
        let conn = fixture.pool.get().unwrap();
        let state: (String, i64, i64, i64, i64, String, i64) = conn
            .query_row(
                "SELECT binding.phase, binding.fence,
                        (SELECT COUNT(*) FROM jobs_ats_certification_canary_reservations),
                        (SELECT COUNT(*)
                           FROM jobs_ats_certification_runtime_layout_quarantine_evidence
                          WHERE evidence_kind = 'layout_drift'),
                        (SELECT COUNT(*)
                           FROM jobs_ats_certification_runtime_layout_quarantine_evidence
                          WHERE evidence_kind = 'layout_drift_overflow'),
                        (SELECT state FROM jobs_ats_certification_circuit_heads
                          WHERE scope_kind = 'runtime' AND subject_key = ?2),
                        (SELECT head_revision FROM jobs_ats_certification_circuit_heads
                          WHERE scope_kind = 'runtime' AND subject_key = ?2)
                   FROM jobs_application_ats_certification_bindings binding
                  WHERE binding.binding_id = ?1",
                params![binding_request.binding_id, binding_request.runner_id],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                        row.get(6)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(
            state,
            (
                "preflight".to_string(),
                0,
                0,
                ATS_CERTIFICATION_MAX_RUNTIME_LAYOUT_QUARANTINE_EVIDENCE,
                1,
                "held".to_string(),
                ATS_CERTIFICATION_MAX_RUNTIME_LAYOUT_QUARANTINE_EVIDENCE + 1,
            )
        );
        let evidence_sha256: String = conn
            .query_row(
                "SELECT evidence_sha256
                   FROM jobs_ats_certification_runtime_layout_quarantine_evidence
                  ORDER BY evidence_sha256 LIMIT 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(conn
            .execute(
                "UPDATE jobs_ats_certification_runtime_layout_quarantine_evidence
                    SET recorded_by = 'mutated' WHERE evidence_sha256 = ?1",
                params![evidence_sha256],
            )
            .is_err());
        assert!(conn
            .execute(
                "DELETE FROM jobs_ats_certification_runtime_layout_quarantine_evidence
                  WHERE evidence_sha256 = ?1",
                params![evidence_sha256],
            )
            .is_err());
    }

    #[test]
    fn preflight_invalidation_is_replay_safe_and_fences_consume_and_recovery() {
        let fixture = imported_fixture();
        let binding_request = test_binding_request("binding-invalidated", 'c');
        create_ats_application_certification_binding(&fixture.pool, &binding_request, TEST_NOW_MS)
            .unwrap();
        let invalidation = AtsCertificationBindingInvalidationRequest {
            binding_id: binding_request.binding_id.clone(),
            account_id: binding_request.account_id.clone(),
            application_id: binding_request.application_id.clone(),
            run_id: binding_request.run_id.clone(),
            application_attempt_id: binding_request.application_attempt_id.clone(),
            nonce_sha256: binding_request.nonce_sha256.clone(),
            expected_fence: 0,
            invalidation_kind: "packet_changed".to_string(),
        };
        let invalidated = invalidate_ats_application_certification_binding(
            &fixture.pool,
            &invalidation,
            TEST_NOW_MS + 1,
        )
        .unwrap();
        assert_eq!(
            (invalidated.phase.as_str(), invalidated.fence),
            ("invalidated", 1)
        );
        assert!(!invalidated.replayed);
        assert!(
            invalidate_ats_application_certification_binding(
                &fixture.pool,
                &invalidation,
                TEST_NOW_MS + 2,
            )
            .unwrap()
            .replayed
        );

        let mut conflicting = invalidation.clone();
        conflicting.invalidation_kind = "claim_lost".to_string();
        assert!(matches!(
            invalidate_ats_application_certification_binding(
                &fixture.pool,
                &conflicting,
                TEST_NOW_MS + 2,
            ),
            Err(AtsCertificationAuthorityError::CompareAndSwapConflict)
        ));

        let phase_b = AtsCertificationPhaseBRequest {
            binding_id: binding_request.binding_id.clone(),
            account_id: binding_request.account_id.clone(),
            application_id: binding_request.application_id.clone(),
            run_id: binding_request.run_id.clone(),
            application_attempt_id: binding_request.application_attempt_id.clone(),
            packet_checksum_sha256: binding_request.packet_checksum_sha256.clone(),
            auto_authorization_id: binding_request.auto_authorization_id.clone(),
            auto_authorization_revision: binding_request.auto_authorization_revision,
            auto_authorization_fingerprint_sha256: binding_request
                .auto_authorization_fingerprint_sha256
                .clone(),
            target_evidence: test_target_evidence(),
            runner_id: binding_request.runner_id.clone(),
            nonce_sha256: binding_request.nonce_sha256.clone(),
            observed_surface: test_surface(),
            phase_b_request_id: "phase-b-invalidated".to_string(),
            metering_reservation_sha256: "d".repeat(64),
            canary_reservation_id: "reservation-invalidated".to_string(),
            period_key: "2033-05-18".to_string(),
            expected_fence: 0,
            terminal_phase: "consumed".to_string(),
        };
        assert!(validate_consume_reserve_ats_application_certification(
            &fixture.pool,
            &phase_b,
            TEST_NOW_MS + 3,
        )
        .is_err());
        assert!(matches!(
            recover_ats_application_certification(
                &fixture.pool,
                &AtsCertificationRecoveryRequest {
                    binding_id: binding_request.binding_id,
                    account_id: binding_request.account_id,
                    application_id: binding_request.application_id,
                    run_id: binding_request.run_id,
                    application_attempt_id: binding_request.application_attempt_id,
                    nonce_sha256: binding_request.nonce_sha256,
                },
            ),
            Err(AtsCertificationAuthorityError::NotFound)
        ));
    }

    #[test]
    fn signed_policy_rejects_unbounded_manifest_target_and_evidence_limits() {
        let authority = test_authority();
        let mut policy = test_trust_policy(&authority, 1, None);
        policy
            .certification_requirements
            .maximum_manifest_size_bytes = ATS_CERTIFICATION_MAX_CANONICAL_BYTES as i64 + 1;
        assert!(validate_ats_certification_trust_policy(&policy, &authority.root_anchor).is_err());
        policy
            .certification_requirements
            .maximum_manifest_size_bytes = 4_096;
        policy.certification_requirements.maximum_target_count = 0;
        assert!(validate_ats_certification_trust_policy(&policy, &authority.root_anchor).is_err());
        policy.certification_requirements.maximum_target_count = 1;
        policy
            .certification_requirements
            .maximum_evidence_object_count = 65;
        assert!(validate_ats_certification_trust_policy(&policy, &authority.root_anchor).is_err());
    }

    #[test]
    fn aggregate_manifest_import_commits_all_or_rolls_back_all_evidence() {
        let pool = test_pool();
        let authority = test_authority();
        let policy_sha256 = initialize_test_trust_policy(&pool, &authority);
        let evidence = test_evidence("authorized_sandbox", &policy_sha256);
        let evidence_envelope = envelope(
            &evidence,
            "evidence",
            ATS_CERTIFICATION_EVIDENCE_AUDIENCE,
            evidence.issued_at_ms,
            &authority,
            "authorize-aggregate-evidence-1",
        );
        let missing_layout_sha256 = "9".repeat(64);
        let manifest = test_manifest(
            envelope_sha256(&evidence_envelope),
            missing_layout_sha256,
            &policy_sha256,
        );
        let manifest_envelope = envelope(
            &manifest,
            "manifest",
            ATS_CERTIFICATION_MANIFEST_AUDIENCE,
            manifest.issued_at_ms,
            &authority,
            "authorize-aggregate-manifest-1",
        );
        let aggregate = AtsCertificationManifestAggregateEnvelope {
            manifest: manifest_envelope,
            evidence: vec![evidence_envelope],
        };
        assert!(import_ats_certification_manifest_aggregate_at(
            &pool,
            &aggregate,
            "certifier",
            TEST_NOW_MS,
        )
        .is_err());
        let conn = pool.get().unwrap();
        let counts: (i64, i64) = conn
            .query_row(
                "SELECT
                   (SELECT COUNT(*) FROM jobs_ats_certification_evidence),
                   (SELECT COUNT(*) FROM jobs_ats_certification_manifests)",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(counts, (0, 0));
        drop(conn);

        let layout = test_layout_observation("authorized_sandbox", &policy_sha256);
        let layout_envelope = envelope(
            &layout,
            "layout_observation",
            ATS_CERTIFICATION_LAYOUT_OBSERVATION_AUDIENCE,
            layout.issued_at_ms,
            &authority,
            "authorize-aggregate-layout-1",
        );
        import_ats_layout_observation_at(&pool, &layout_envelope, "observer", TEST_NOW_MS).unwrap();
        let evidence = test_evidence("authorized_sandbox", &policy_sha256);
        let evidence_envelope = envelope(
            &evidence,
            "evidence",
            ATS_CERTIFICATION_EVIDENCE_AUDIENCE,
            evidence.issued_at_ms,
            &authority,
            "authorize-aggregate-evidence-2",
        );
        let mut manifest = test_manifest(
            envelope_sha256(&evidence_envelope),
            envelope_sha256(&layout_envelope),
            &policy_sha256,
        );
        manifest.certification_id = "greenhouse-acme-123-aggregate-2".to_string();
        let manifest_envelope = envelope(
            &manifest,
            "manifest",
            ATS_CERTIFICATION_MANIFEST_AUDIENCE,
            manifest.issued_at_ms,
            &authority,
            "authorize-aggregate-manifest-2",
        );
        let aggregate = AtsCertificationManifestAggregateEnvelope {
            manifest: manifest_envelope,
            evidence: vec![evidence_envelope],
        };
        let imported = import_ats_certification_manifest_aggregate_at(
            &pool,
            &aggregate,
            "certifier",
            TEST_NOW_MS,
        )
        .unwrap();
        assert!(!imported.manifest.replayed);
        assert_eq!(imported.evidence.len(), 1);
        let replay = import_ats_certification_manifest_aggregate_at(
            &pool,
            &aggregate,
            "certifier",
            TEST_NOW_MS,
        )
        .unwrap();
        assert!(replay.manifest.replayed);
        assert!(replay.evidence[0].replayed);
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct SharedAuthorityVectors {
        schema_version: i64,
        vectors: Vec<SharedAuthorityVector>,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct SharedAuthorityVector {
        name: String,
        kind: String,
        authority: serde_json::Value,
        sha256: String,
    }

    fn assert_shared_authority_vector<T>(vector: &SharedAuthorityVector)
    where
        T: serde::de::DeserializeOwned + Serialize,
    {
        let parsed: T = serde_json::from_value(vector.authority.clone())
            .unwrap_or_else(|error| panic!("{} should parse strictly: {error}", vector.name));
        let canonical = serde_json::to_vec(&parsed)
            .unwrap_or_else(|error| panic!("{} should serialize: {error}", vector.name));
        assert_eq!(
            ats_certification_sha256(&canonical),
            vector.sha256,
            "{} canonical digest drifted from Node",
            vector.name
        );

        let canonical_base64url =
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&canonical);
        assert!(!canonical_base64url.contains('='));
        assert_eq!(
            base64::engine::general_purpose::URL_SAFE_NO_PAD
                .decode(canonical_base64url)
                .unwrap(),
            canonical
        );

        let mut unknown = vector.authority.clone();
        unknown
            .as_object_mut()
            .expect("shared authority must be an object")
            .insert("unexpected".to_string(), serde_json::Value::Bool(true));
        assert!(
            serde_json::from_value::<T>(unknown).is_err(),
            "{} accepted an unknown field",
            vector.name
        );
    }

    #[test]
    fn node_and_rust_share_canonical_authority_vectors() {
        let fixture: SharedAuthorityVectors = serde_json::from_str(include_str!(
            "../../../../jobs/automation/tests/fixtures/ats-certification-authority-vectors.json"
        ))
        .expect("shared ATS authority vectors should parse");
        assert_eq!(fixture.schema_version, 1);
        assert_eq!(fixture.vectors.len(), 5);

        for vector in &fixture.vectors {
            match vector.kind.as_str() {
                "trustPolicy" => {
                    assert_shared_authority_vector::<AtsCertificationTrustPolicyAuthority>(vector)
                }
                "layoutObservation" => {
                    assert_shared_authority_vector::<AtsLayoutObservationAuthority>(vector)
                }
                "manifest" => {
                    assert_shared_authority_vector::<AtsCertificationManifestAuthority>(vector)
                }
                "activation" => {
                    assert_shared_authority_vector::<AtsCertificationActivationAuthority>(vector)
                }
                "revocation" => {
                    assert_shared_authority_vector::<AtsCertificationRevocationAuthority>(vector)
                }
                kind => panic!("unknown shared ATS authority vector kind: {kind}"),
            }
        }
    }

    #[test]
    fn empty_database_has_zero_active_production_certifications() {
        let pool = test_pool();
        assert!(resolve_active_ats_certification(
            &pool,
            "https://boards.greenhouse.io/acme/jobs/123",
            None,
            Some(&test_surface()),
            TEST_NOW_MS,
        )
        .unwrap()
        .is_none());
        let conn = pool.get().unwrap();
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM jobs_ats_certification_heads WHERE channel = 'general'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 0);
    }
}
