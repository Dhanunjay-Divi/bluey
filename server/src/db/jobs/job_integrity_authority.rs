const JOB_INTEGRITY_ATTESTATION_AUDIENCE: &str = "bluey-jobs-job-integrity-attestation-v1";
const JOB_INTEGRITY_TRUST_POLICY_AUDIENCE: &str = "bluey-jobs-job-integrity-trust-policy-v1";
const JOB_INTEGRITY_REVOCATION_AUDIENCE: &str = "bluey-jobs-job-integrity-revocation-v1";
const JOB_INTEGRITY_AUTHORIZATION_AUDIENCE: &str = "bluey-jobs-job-integrity-authorization-v1";
const JOB_INTEGRITY_ROOT_AUTHORIZATION_AUDIENCE: &str =
    "bluey-jobs-job-integrity-root-policy-authorization-v1";
const JOB_INTEGRITY_TRANSITION_AUDIENCE: &str = "bluey-jobs-job-integrity-head-transition-v1";
const JOB_INTEGRITY_ROOT_TRUST_ANCHOR_ENV: &str = "BLUEY_JOBS_JOB_INTEGRITY_ROOT_TRUST_ANCHOR_JSON";
const JOB_INTEGRITY_MAX_CANONICAL_BYTES: usize = 64 * 1024;
const JOB_INTEGRITY_MAX_SAFE_INTEGER: i64 = 9_007_199_254_740_991;
const JOB_INTEGRITY_ROLES: [&str; 3] = ["employer_identity", "job_risk", "revocation"];
const JOB_INTEGRITY_PROVIDERS: [&str; 5] =
    ["ashby", "greenhouse", "lever", "smartrecruiters", "workday"];
const JOB_INTEGRITY_MISMATCH_SIGNALS: [&str; 3] = [
    "ats_tenant_mismatch",
    "employer_domain_lookalike",
    "employer_identity_mismatch",
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JobIntegrityDetachedSignatureV1 {
    pub key_id: String,
    pub signature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JobIntegrityAuthorizationV1 {
    pub version: i64,
    pub audience: String,
    pub authorization_id: String,
    pub role: String,
    pub policy_sha256: String,
    pub target_audience: String,
    pub target_sha256: String,
    pub signed_at_ms: i64,
    pub signatures: Vec<JobIntegrityDetachedSignatureV1>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JobIntegrityRootTrustAnchorV1 {
    pub threshold: i64,
    pub keys: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JobIntegrityTrustKeyV1 {
    pub public_key_base64url: String,
    pub valid_from_ms: i64,
    pub expires_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JobIntegrityTrustRoleV1 {
    pub threshold: i64,
    pub keys: BTreeMap<String, JobIntegrityTrustKeyV1>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JobIntegrityPolicyRequirementsV1 {
    pub allowed_providers: Vec<String>,
    pub required_identity_methods: Vec<String>,
    pub required_identity_evidence_classes: Vec<String>,
    pub required_risk_evidence_classes: Vec<String>,
    pub allowed_identity_evidence_classes: Vec<String>,
    pub allowed_risk_evidence_classes: Vec<String>,
    pub allowed_risk_signal_codes: Vec<String>,
    pub allowed_risk_policy_sha256s: Vec<String>,
    pub maximum_positive_lifetime_ms: i64,
    pub maximum_nonpositive_lifetime_ms: i64,
    pub maximum_clock_skew_ms: i64,
    pub maximum_canonical_bytes: i64,
    pub maximum_identity_evidence_count: i64,
    pub maximum_risk_evidence_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JobIntegrityTrustPolicyV1 {
    pub version: i64,
    pub audience: String,
    pub policy_id: String,
    pub trust_generation: i64,
    pub predecessor_policy_sha256: Option<String>,
    pub delegated_roles: BTreeMap<String, JobIntegrityTrustRoleV1>,
    pub requirements: JobIntegrityPolicyRequirementsV1,
    pub issued_at_ms: i64,
    pub valid_from_ms: i64,
    pub expires_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JobIntegrityTrustPolicyPackageV1 {
    pub canonical_policy_base64url: String,
    pub canonical_root_authorization_base64url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JobIntegrityProviderTargetV1 {
    pub host: String,
    pub tenant: String,
    pub job: String,
    pub variant: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JobIntegritySourceV1 {
    pub provider_family: String,
    pub provider_record_id: String,
    pub target: JobIntegrityProviderTargetV1,
    pub canonical_application_url: String,
    pub application_domain: String,
    pub ats_tenant_binding_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JobIntegrityEvidenceV1 {
    pub class: String,
    pub kind: String,
    pub sha256: String,
    pub observed_at_ms: i64,
    pub expires_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JobIntegrityEmployerV1 {
    pub status: String,
    pub canonical_employer_id: String,
    pub canonical_employer_domain: String,
    pub verification_methods: Vec<String>,
    pub evidence: Vec<JobIntegrityEvidenceV1>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JobIntegrityRiskV1 {
    pub status: String,
    pub signal_codes: Vec<String>,
    pub policy_sha256: String,
    pub input_sha256: String,
    pub engine_release_sha256: String,
    pub evidence: Vec<JobIntegrityEvidenceV1>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JobIntegrityAttestationV1 {
    pub version: i64,
    pub audience: String,
    pub attestation_id: String,
    pub policy_sha256: String,
    pub subject_sha256: String,
    pub source_material_sha256: String,
    pub attestation_generation: i64,
    pub predecessor_attestation_sha256: Option<String>,
    pub canonical_job_id: String,
    pub source: JobIntegritySourceV1,
    pub employer: JobIntegrityEmployerV1,
    pub risk: JobIntegrityRiskV1,
    pub assessed_at_ms: i64,
    pub issued_at_ms: i64,
    pub not_before_ms: i64,
    pub expires_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JobIntegrityAttestationPackageV1 {
    pub canonical_attestation_base64url: String,
    pub canonical_employer_identity_authorization_base64url: String,
    pub canonical_job_risk_authorization_base64url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JobIntegrityRevocationV1 {
    pub version: i64,
    pub audience: String,
    pub revocation_id: String,
    pub policy_sha256: String,
    pub revocation_generation: i64,
    pub predecessor_revocation_sha256: Option<String>,
    pub subject_kind: String,
    pub subject_id: String,
    pub subject_sha256: String,
    pub reason_code: String,
    pub reason_ref: String,
    pub effective_at_ms: i64,
    pub issued_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JobIntegrityRevocationPackageV1 {
    pub canonical_revocation_base64url: String,
    pub canonical_authorization_base64url: String,
}

/// Exact Phase 614 source authority projected by a server-owned caller. The
/// resolver never accepts source expiry inside a signed integrity attestation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JobIntegrityExpectedSource {
    pub subject_sha256: String,
    pub source_material_sha256: String,
    pub source_expires_at_ms: i64,
    pub canonical_job_id: String,
    pub provider_family: String,
    pub provider_record_id: String,
    pub provider_host: String,
    pub provider_tenant: String,
    pub provider_job: String,
    pub provider_variant: String,
    pub canonical_application_url: String,
    pub application_domain: String,
    pub ats_tenant_binding_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum JobIntegrityResolutionStatus {
    Absent,
    Verified,
    ReviewRequired,
    Blocked,
    Mismatch,
    Expired,
    Revoked,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JobIntegrityCurrentAuthority {
    pub subject_sha256: String,
    pub source_material_sha256: String,
    pub attestation_sha256: String,
    pub attestation_generation: i64,
    pub head_revision: i64,
    pub head_transition_sha256: String,
    pub policy_sha256: String,
    #[serde(rename = "employerIdentityAuthorizationSha256")]
    pub employer_authorization_sha256: String,
    #[serde(rename = "jobRiskAuthorizationSha256")]
    pub risk_authorization_sha256: String,
    pub canonical_employer_id: String,
    pub canonical_employer_domain: String,
    pub risk_policy_sha256: String,
    #[serde(rename = "expiresAtMs")]
    pub effective_expires_at_ms: i64,
    pub canonical_job_id: String,
    pub provider_family: String,
    pub provider_record_id: String,
    pub provider_host: String,
    pub provider_tenant: String,
    pub provider_job: String,
    pub provider_variant: String,
    pub canonical_application_url: String,
    pub application_domain: String,
    pub ats_tenant_binding_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JobIntegrityResolution {
    pub status: JobIntegrityResolutionStatus,
    pub reason_code: String,
    pub signal_codes: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authority: Option<JobIntegrityCurrentAuthority>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JobIntegrityImportResult {
    pub object_sha256: String,
    pub generation: i64,
    pub replayed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub head_revision: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub head_transition_sha256: Option<String>,
}

#[derive(Debug, Error)]
pub enum JobIntegrityAuthorityError {
    #[error("invalid job-integrity envelope")]
    InvalidEnvelope,
    #[error("invalid job-integrity authority")]
    InvalidAuthority,
    #[error("invalid job-integrity root trust anchor")]
    InvalidTrustAnchor,
    #[error("invalid job-integrity trust policy")]
    InvalidTrustPolicy,
    #[error("job-integrity trust policy is not initialized")]
    NotInitialized,
    #[error("job-integrity signature is invalid")]
    InvalidSignature,
    #[error("job-integrity signature threshold was not met")]
    ThresholdNotMet,
    #[error("job-integrity authority was not found")]
    NotFound,
    #[error("job-integrity identity conflicts with stored authority")]
    IdentityConflict,
    #[error("job-integrity compare-and-swap failed")]
    CompareAndSwapConflict,
    #[error("job-integrity sequence regressed")]
    SequenceRegression,
    #[error("job-integrity authority is outside its validity window")]
    Expired,
    #[error("job-integrity signing authority is revoked")]
    Revoked,
    #[error("job-integrity source binding does not match")]
    SourceMismatch,
    #[error("job-integrity storage failed: {0}")]
    Storage(#[source] anyhow::Error),
}

pub type JobIntegrityResult<T> = std::result::Result<T, JobIntegrityAuthorityError>;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct JobIntegrityAuthorizationPayload<'a> {
    version: i64,
    audience: &'a str,
    authorization_id: &'a str,
    role: &'a str,
    policy_sha256: &'a str,
    target_audience: &'a str,
    target_sha256: &'a str,
    signed_at_ms: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct JobIntegrityHeadTransitionCanonical<'a> {
    version: i64,
    audience: &'static str,
    subject_sha256: &'a str,
    head_revision: i64,
    previous_head_revision: i64,
    predecessor_transition_sha256: Option<&'a str>,
    previous_attestation_sha256: Option<&'a str>,
    attestation_sha256: &'a str,
    attestation_generation: i64,
    policy_sha256: &'a str,
    transition_actor: &'a str,
    transitioned_at_ms: i64,
}

#[derive(Debug, Clone)]
struct JobIntegrityVerifiedAuthorization {
    authorization_id: String,
    sha256: String,
    key_ids: Vec<String>,
    effective_key_expires_at_ms: i64,
}

struct JobIntegrityAuthorizationVerification<'a> {
    encoded: &'a str,
    authorization_audience: &'a str,
    role_name: &'a str,
    role: &'a JobIntegrityTrustRoleV1,
    policy_sha256: &'a str,
    target_audience: &'a str,
    target_sha256: &'a str,
    minimum_signed_at_ms: i64,
    verification_time_ms: i64,
    maximum_clock_skew_ms: i64,
}

struct JobIntegrityVerifiedAttestation {
    attestation: JobIntegrityAttestationV1,
    sha256: String,
    employer_authorization: JobIntegrityVerifiedAuthorization,
    risk_authorization: JobIntegrityVerifiedAuthorization,
    effective_expires_at_ms: i64,
}

struct ParsedJobIntegrityTrustPolicyPackage {
    policy_bytes: Vec<u8>,
    authorization_bytes: Vec<u8>,
    policy: JobIntegrityTrustPolicyV1,
    authorization: JobIntegrityAuthorizationV1,
}

struct ParsedJobIntegrityAttestationPackage {
    attestation_bytes: Vec<u8>,
    attestation: JobIntegrityAttestationV1,
    employer_authorization_bytes: Vec<u8>,
    employer_authorization: JobIntegrityAuthorizationV1,
    risk_authorization_bytes: Vec<u8>,
    risk_authorization: JobIntegrityAuthorizationV1,
}

struct ParsedJobIntegrityRevocationPackage {
    revocation_bytes: Vec<u8>,
    revocation: JobIntegrityRevocationV1,
    authorization_bytes: Vec<u8>,
    authorization: JobIntegrityAuthorizationV1,
}

struct JobIntegrityExistingAttestation {
    sha256: String,
    canonical_attestation_base64url: String,
    canonical_employer_authorization_base64url: String,
    canonical_risk_authorization_base64url: String,
    generation: i64,
    head_revision: i64,
    head_transition_sha256: String,
}

fn job_integrity_storage(error: impl Into<anyhow::Error>) -> JobIntegrityAuthorityError {
    JobIntegrityAuthorityError::Storage(error.into())
}

fn job_integrity_canonical_json<T: Serialize>(value: &T) -> JobIntegrityResult<Vec<u8>> {
    let mut bytes =
        serde_json::to_vec(value).map_err(|_| JobIntegrityAuthorityError::InvalidAuthority)?;
    bytes.push(b'\n');
    if bytes.len() > JOB_INTEGRITY_MAX_CANONICAL_BYTES {
        return Err(JobIntegrityAuthorityError::InvalidEnvelope);
    }
    Ok(bytes)
}

fn job_integrity_sha256(bytes: impl AsRef<[u8]>) -> String {
    hex::encode(Sha256::digest(bytes.as_ref()))
}

fn job_integrity_safe_integer(value: i64, positive: bool) -> bool {
    value >= i64::from(positive) && value <= JOB_INTEGRITY_MAX_SAFE_INTEGER
}

fn job_integrity_hex64(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn job_integrity_token(value: &str, minimum: usize, maximum: usize) -> bool {
    (minimum..=maximum).contains(&value.len())
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':' | b'/')
        })
}

fn job_integrity_text(value: &str, minimum: usize, maximum: usize) -> bool {
    (minimum..=maximum).contains(&value.len())
        && value.trim() == value
        && !value.chars().any(char::is_control)
}

fn job_integrity_sorted_unique(values: &[String], maximum: usize) -> bool {
    !values.is_empty()
        && values.len() <= maximum
        && values.windows(2).all(|pair| pair[0] < pair[1])
        && values
            .iter()
            .all(|value| job_integrity_token(value, 1, 120))
}

fn job_integrity_domain(value: &str) -> bool {
    if value.is_empty()
        || value.len() > 253
        || value != value.to_ascii_lowercase()
        || !value.is_ascii()
        || !value.contains('.')
        || value.parse::<std::net::IpAddr>().is_ok()
        || value == "localhost"
        || value.ends_with(".localhost")
    {
        return false;
    }
    let labels_valid = value.split('.').all(|label| {
        !label.is_empty()
            && label.len() <= 63
            && !label.starts_with('-')
            && !label.ends_with('-')
            && label
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    });
    labels_valid
        && reqwest::Url::parse(&format!("https://{value}/"))
            .ok()
            .and_then(|url| url.domain().map(str::to_string))
            .is_some_and(|domain| domain == value)
}

fn job_integrity_canonical_percent_encoding(value: &str) -> bool {
    let bytes = value.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'%' {
            index += 1;
            continue;
        }
        let Some(encoded) = bytes.get(index + 1..index + 3) else {
            return false;
        };
        let hexadecimal_value = |byte: u8| match byte {
            b'0'..=b'9' => Some(byte - b'0'),
            b'A'..=b'F' => Some(byte - b'A' + 10),
            _ => None,
        };
        let (Some(high), Some(low)) =
            (hexadecimal_value(encoded[0]), hexadecimal_value(encoded[1]))
        else {
            return false;
        };
        let decoded = high << 4 | low;
        if decoded.is_ascii_alphanumeric() || matches!(decoded, b'-' | b'.' | b'_' | b'~') {
            return false;
        }
        index += 3;
    }
    true
}

fn job_integrity_canonical_https_url(value: &str, expected_domain: &str) -> bool {
    reqwest::Url::parse(value).ok().is_some_and(|url| {
        url.as_str() == value
            && url.scheme() == "https"
            && url.username().is_empty()
            && url.password().is_none()
            && url.fragment().is_none()
            && url.port().is_none()
            && url.host_str() == Some(expected_domain)
            && job_integrity_canonical_percent_encoding(url.path())
            && url
                .query()
                .is_none_or(job_integrity_canonical_percent_encoding)
    })
}

fn job_integrity_parse_canonical_json<T>(bytes: &[u8]) -> JobIntegrityResult<T>
where
    T: DeserializeOwned + Serialize,
{
    if bytes.is_empty() || bytes.len() > JOB_INTEGRITY_MAX_CANONICAL_BYTES {
        return Err(JobIntegrityAuthorityError::InvalidEnvelope);
    }
    let value: T =
        serde_json::from_slice(bytes).map_err(|_| JobIntegrityAuthorityError::InvalidAuthority)?;
    if job_integrity_canonical_json(&value)? != bytes {
        return Err(JobIntegrityAuthorityError::InvalidAuthority);
    }
    Ok(value)
}

fn job_integrity_decode_base64url_bounded(value: &str) -> JobIntegrityResult<Vec<u8>> {
    if value.is_empty() || value.len() > JOB_INTEGRITY_MAX_CANONICAL_BYTES * 2 {
        return Err(JobIntegrityAuthorityError::InvalidEnvelope);
    }
    let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(value)
        .map_err(|_| JobIntegrityAuthorityError::InvalidEnvelope)?;
    if decoded.is_empty()
        || decoded.len() > JOB_INTEGRITY_MAX_CANONICAL_BYTES
        || base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&decoded) != value
    {
        return Err(JobIntegrityAuthorityError::InvalidEnvelope);
    }
    Ok(decoded)
}

fn job_integrity_decode_exact(value: &str, expected: usize) -> JobIntegrityResult<Vec<u8>> {
    let decoded = job_integrity_decode_base64url_bounded(value)?;
    if decoded.len() != expected {
        return Err(JobIntegrityAuthorityError::InvalidSignature);
    }
    Ok(decoded)
}

fn job_integrity_strong_verifying_key(value: &str) -> Option<ed25519_dalek::VerifyingKey> {
    let public_key: [u8; 32] = job_integrity_decode_exact(value, 32)
        .ok()?
        .try_into()
        .ok()?;
    let verifying_key = ed25519_dalek::VerifyingKey::from_bytes(&public_key).ok()?;
    (!verifying_key.is_weak()).then_some(verifying_key)
}

fn job_integrity_db_now_sqlite(tx: &rusqlite::Transaction<'_>) -> JobIntegrityResult<i64> {
    tx.query_row(
        "SELECT CAST((julianday('now') - 2440587.5) * 86400000 AS INTEGER)",
        [],
        |row| row.get(0),
    )
    .map_err(job_integrity_storage)
}

fn job_integrity_db_now_postgres(tx: &mut postgres::Transaction<'_>) -> JobIntegrityResult<i64> {
    tx.query_one(
        "SELECT floor(extract(epoch FROM clock_timestamp()) * 1000)::bigint",
        &[],
    )
    .map(|row| row.get(0))
    .map_err(job_integrity_storage)
}

fn validate_job_integrity_root_anchor(
    anchor: &JobIntegrityRootTrustAnchorV1,
) -> JobIntegrityResult<()> {
    if anchor.keys.is_empty()
        || anchor.keys.len() > 16
        || anchor.threshold < 1
        || anchor.threshold as usize > anchor.keys.len()
    {
        return Err(JobIntegrityAuthorityError::InvalidTrustAnchor);
    }
    let mut public_keys = BTreeSet::new();
    for (key_id, public_key) in &anchor.keys {
        if !job_integrity_token(key_id, 1, 120)
            || job_integrity_strong_verifying_key(public_key).is_none()
            || !public_keys.insert(public_key)
        {
            return Err(JobIntegrityAuthorityError::InvalidTrustAnchor);
        }
    }
    Ok(())
}

fn job_integrity_root_anchor_sha256(
    anchor: &JobIntegrityRootTrustAnchorV1,
) -> JobIntegrityResult<String> {
    validate_job_integrity_root_anchor(anchor)?;
    Ok(job_integrity_sha256(job_integrity_canonical_json(anchor)?))
}

fn job_integrity_root_anchor_from_env() -> JobIntegrityResult<JobIntegrityRootTrustAnchorV1> {
    let raw = std::env::var(JOB_INTEGRITY_ROOT_TRUST_ANCHOR_ENV)
        .map_err(|_| JobIntegrityAuthorityError::InvalidTrustAnchor)?;
    if raw.len() > JOB_INTEGRITY_MAX_CANONICAL_BYTES {
        return Err(JobIntegrityAuthorityError::InvalidTrustAnchor);
    }
    let anchor: JobIntegrityRootTrustAnchorV1 =
        serde_json::from_str(&raw).map_err(|_| JobIntegrityAuthorityError::InvalidTrustAnchor)?;
    validate_job_integrity_root_anchor(&anchor)?;
    Ok(anchor)
}

fn validate_job_integrity_policy(
    policy: &JobIntegrityTrustPolicyV1,
    root_anchor: &JobIntegrityRootTrustAnchorV1,
) -> JobIntegrityResult<()> {
    validate_job_integrity_root_anchor(root_anchor)?;
    if policy.version != 1
        || policy.audience != JOB_INTEGRITY_TRUST_POLICY_AUDIENCE
        || !job_integrity_token(&policy.policy_id, 1, 120)
        || !job_integrity_safe_integer(policy.trust_generation, true)
        || policy
            .predecessor_policy_sha256
            .as_deref()
            .is_some_and(|value| !job_integrity_hex64(value))
        || !job_integrity_safe_integer(policy.issued_at_ms, false)
        || policy.valid_from_ms < policy.issued_at_ms
        || policy.expires_at_ms <= policy.valid_from_ms
        || !job_integrity_safe_integer(policy.expires_at_ms, true)
        || policy.delegated_roles.len() != JOB_INTEGRITY_ROLES.len()
        || !JOB_INTEGRITY_ROLES
            .iter()
            .all(|role| policy.delegated_roles.contains_key(*role))
    {
        return Err(JobIntegrityAuthorityError::InvalidTrustPolicy);
    }
    let requirements = &policy.requirements;
    if requirements.allowed_providers != JOB_INTEGRITY_PROVIDERS.map(str::to_string)
        || !job_integrity_sorted_unique(&requirements.required_identity_methods, 16)
        || !job_integrity_sorted_unique(&requirements.required_identity_evidence_classes, 16)
        || !job_integrity_sorted_unique(&requirements.required_risk_evidence_classes, 16)
        || !job_integrity_sorted_unique(&requirements.allowed_identity_evidence_classes, 32)
        || !job_integrity_sorted_unique(&requirements.allowed_risk_evidence_classes, 32)
        || !job_integrity_sorted_unique(&requirements.allowed_risk_signal_codes, 64)
        || requirements.allowed_risk_policy_sha256s.is_empty()
        || requirements.allowed_risk_policy_sha256s.len() > 32
        || !requirements
            .allowed_risk_policy_sha256s
            .windows(2)
            .all(|pair| pair[0] < pair[1])
        || !requirements
            .allowed_risk_policy_sha256s
            .iter()
            .all(|value| job_integrity_hex64(value))
        || !requirements
            .required_identity_evidence_classes
            .iter()
            .all(|value| {
                requirements
                    .allowed_identity_evidence_classes
                    .contains(value)
            })
        || !requirements
            .required_risk_evidence_classes
            .iter()
            .all(|value| requirements.allowed_risk_evidence_classes.contains(value))
        || !(60_000..=31_536_000_000).contains(&requirements.maximum_positive_lifetime_ms)
        || !(60_000..=31_536_000_000).contains(&requirements.maximum_nonpositive_lifetime_ms)
        || !(0..=300_000).contains(&requirements.maximum_clock_skew_ms)
        || !(1_024..=JOB_INTEGRITY_MAX_CANONICAL_BYTES as i64)
            .contains(&requirements.maximum_canonical_bytes)
        || !(1..=64).contains(&requirements.maximum_identity_evidence_count)
        || !(1..=64).contains(&requirements.maximum_risk_evidence_count)
    {
        return Err(JobIntegrityAuthorityError::InvalidTrustPolicy);
    }
    let root_ids = root_anchor.keys.keys().collect::<BTreeSet<_>>();
    let root_keys = root_anchor.keys.values().collect::<BTreeSet<_>>();
    let mut delegated_ids = BTreeMap::<String, String>::new();
    let mut delegated_keys = BTreeMap::<String, String>::new();
    for (role_name, role) in &policy.delegated_roles {
        if !JOB_INTEGRITY_ROLES.contains(&role_name.as_str())
            || role.keys.is_empty()
            || role.keys.len() > 16
            || role.threshold < 1
            || role.threshold as usize > role.keys.len()
        {
            return Err(JobIntegrityAuthorityError::InvalidTrustPolicy);
        }
        for (key_id, key) in &role.keys {
            if !job_integrity_token(key_id, 1, 120)
                || root_ids.contains(key_id)
                || root_keys.contains(&key.public_key_base64url)
                || job_integrity_strong_verifying_key(&key.public_key_base64url).is_none()
                || !job_integrity_safe_integer(key.valid_from_ms, false)
                || key.expires_at_ms <= key.valid_from_ms
                || key.valid_from_ms < policy.valid_from_ms
                || key.expires_at_ms > policy.expires_at_ms
            {
                return Err(JobIntegrityAuthorityError::InvalidTrustPolicy);
            }
            if delegated_ids
                .insert(key_id.clone(), role_name.clone())
                .is_some()
                || delegated_keys
                    .insert(key.public_key_base64url.clone(), role_name.clone())
                    .is_some()
            {
                return Err(JobIntegrityAuthorityError::InvalidTrustPolicy);
            }
        }
    }
    Ok(())
}

fn validate_job_integrity_evidence(
    evidence: &[JobIntegrityEvidenceV1],
    allowed_classes: &[String],
    required_classes: &[String],
    maximum_count: i64,
    assessed_at_ms: i64,
) -> JobIntegrityResult<i64> {
    if evidence.is_empty()
        || evidence.len() as i64 > maximum_count
        || !evidence.windows(2).all(|pair| {
            (
                &pair[0].class,
                &pair[0].kind,
                &pair[0].sha256,
                pair[0].observed_at_ms,
                pair[0].expires_at_ms,
            ) < (
                &pair[1].class,
                &pair[1].kind,
                &pair[1].sha256,
                pair[1].observed_at_ms,
                pair[1].expires_at_ms,
            )
        })
    {
        return Err(JobIntegrityAuthorityError::InvalidAuthority);
    }
    let mut identities = BTreeSet::new();
    let mut seen_classes = BTreeSet::new();
    let mut effective_expiry = JOB_INTEGRITY_MAX_SAFE_INTEGER;
    for item in evidence {
        if !job_integrity_token(&item.class, 1, 120)
            || !allowed_classes.contains(&item.class)
            || !job_integrity_token(&item.kind, 1, 120)
            || !job_integrity_hex64(&item.sha256)
            || !job_integrity_safe_integer(item.observed_at_ms, false)
            || item.observed_at_ms > assessed_at_ms
            || item.expires_at_ms <= item.observed_at_ms
            || !job_integrity_safe_integer(item.expires_at_ms, true)
            || !identities.insert((item.class.clone(), item.kind.clone(), item.sha256.clone()))
        {
            return Err(JobIntegrityAuthorityError::InvalidAuthority);
        }
        seen_classes.insert(item.class.clone());
        effective_expiry = effective_expiry.min(item.expires_at_ms);
    }
    if !required_classes
        .iter()
        .all(|required| seen_classes.contains(required))
    {
        return Err(JobIntegrityAuthorityError::InvalidAuthority);
    }
    Ok(effective_expiry)
}

fn validate_job_integrity_attestation(
    attestation: &JobIntegrityAttestationV1,
    policy: &JobIntegrityTrustPolicyV1,
    canonical_size: usize,
    verification_time_ms: i64,
) -> JobIntegrityResult<i64> {
    let requirements = &policy.requirements;
    let source = &attestation.source;
    if attestation.version != 1
        || attestation.audience != JOB_INTEGRITY_ATTESTATION_AUDIENCE
        || !job_integrity_token(&attestation.attestation_id, 1, 120)
        || attestation.policy_sha256.is_empty()
        || !job_integrity_hex64(&attestation.policy_sha256)
        || !job_integrity_hex64(&attestation.subject_sha256)
        || !job_integrity_hex64(&attestation.source_material_sha256)
        || !job_integrity_safe_integer(attestation.attestation_generation, true)
        || attestation
            .predecessor_attestation_sha256
            .as_deref()
            .is_some_and(|value| !job_integrity_hex64(value))
        || !job_integrity_text(&attestation.canonical_job_id, 1, 240)
        || !requirements
            .allowed_providers
            .contains(&source.provider_family)
        || !job_integrity_text(&source.provider_record_id, 1, 512)
        || !job_integrity_domain(&source.target.host)
        || !job_integrity_token(&source.target.tenant, 1, 240)
        || !job_integrity_token(&source.target.job, 1, 512)
        || !job_integrity_token(&source.target.variant, 1, 120)
        || !job_integrity_text(&source.canonical_application_url, 8, 4096)
        || !job_integrity_canonical_https_url(
            &source.canonical_application_url,
            &source.application_domain,
        )
        || !job_integrity_domain(&source.application_domain)
        || !job_integrity_hex64(&source.ats_tenant_binding_sha256)
        || !job_integrity_token(&attestation.employer.canonical_employer_id, 1, 240)
        || !job_integrity_domain(&attestation.employer.canonical_employer_domain)
        || !matches!(
            attestation.employer.status.as_str(),
            "verified" | "unverified" | "mismatch"
        )
        || !matches!(
            attestation.risk.status.as_str(),
            "clear" | "review_required" | "blocked"
        )
        || !job_integrity_hex64(&attestation.risk.policy_sha256)
        || !requirements
            .allowed_risk_policy_sha256s
            .contains(&attestation.risk.policy_sha256)
        || !job_integrity_hex64(&attestation.risk.input_sha256)
        || !job_integrity_hex64(&attestation.risk.engine_release_sha256)
        || canonical_size > requirements.maximum_canonical_bytes as usize
        || !job_integrity_safe_integer(attestation.assessed_at_ms, false)
        || attestation.issued_at_ms < attestation.assessed_at_ms
        || attestation.not_before_ms < attestation.issued_at_ms
        || attestation.expires_at_ms <= attestation.not_before_ms
        || !job_integrity_safe_integer(attestation.expires_at_ms, true)
        || attestation.issued_at_ms
            > verification_time_ms.saturating_add(requirements.maximum_clock_skew_ms)
        || attestation.not_before_ms
            > verification_time_ms.saturating_add(requirements.maximum_clock_skew_ms)
        || attestation.expires_at_ms <= verification_time_ms
    {
        return Err(JobIntegrityAuthorityError::InvalidAuthority);
    }
    let positive = attestation.employer.status == "verified" && attestation.risk.status == "clear";
    let maximum_lifetime = if positive {
        requirements.maximum_positive_lifetime_ms
    } else {
        requirements.maximum_nonpositive_lifetime_ms
    };
    if attestation.expires_at_ms - attestation.not_before_ms > maximum_lifetime {
        return Err(JobIntegrityAuthorityError::InvalidAuthority);
    }
    let methods = &attestation.employer.verification_methods;
    if !job_integrity_sorted_unique(methods, 16)
        || !methods
            .iter()
            .all(|method| requirements.required_identity_methods.contains(method))
        || (attestation.employer.status == "verified"
            && !requirements
                .required_identity_methods
                .iter()
                .all(|required| methods.contains(required)))
    {
        return Err(JobIntegrityAuthorityError::InvalidAuthority);
    }
    let identity_expiry = validate_job_integrity_evidence(
        &attestation.employer.evidence,
        &requirements.allowed_identity_evidence_classes,
        &requirements.required_identity_evidence_classes,
        requirements.maximum_identity_evidence_count,
        attestation.assessed_at_ms,
    )?;
    let risk_expiry = validate_job_integrity_evidence(
        &attestation.risk.evidence,
        &requirements.allowed_risk_evidence_classes,
        &requirements.required_risk_evidence_classes,
        requirements.maximum_risk_evidence_count,
        attestation.assessed_at_ms,
    )?;
    let signals = &attestation.risk.signal_codes;
    if (!signals.is_empty() && !job_integrity_sorted_unique(signals, 64))
        || !signals
            .iter()
            .all(|signal| requirements.allowed_risk_signal_codes.contains(signal))
        || (attestation.risk.status == "clear" && !signals.is_empty())
        || (attestation.risk.status != "clear" && signals.is_empty())
        || (attestation.risk.status == "clear" && attestation.employer.status != "verified")
        || (attestation.employer.status == "mismatch"
            && (attestation.risk.status != "blocked"
                || !signals
                    .iter()
                    .any(|signal| JOB_INTEGRITY_MISMATCH_SIGNALS.contains(&signal.as_str()))))
    {
        return Err(JobIntegrityAuthorityError::InvalidAuthority);
    }
    Ok(attestation
        .expires_at_ms
        .min(policy.expires_at_ms)
        .min(identity_expiry)
        .min(risk_expiry))
}

fn validate_job_integrity_revocation(
    revocation: &JobIntegrityRevocationV1,
    current_policy_sha256: &str,
    verification_time_ms: i64,
    maximum_clock_skew_ms: i64,
) -> JobIntegrityResult<()> {
    const KINDS: [&str; 9] = [
        "attestation",
        "canonical_employer",
        "identity_evidence",
        "risk_engine_release",
        "risk_evidence",
        "risk_policy",
        "subject",
        "trust_key",
        "trust_policy",
    ];
    if revocation.version != 1
        || revocation.audience != JOB_INTEGRITY_REVOCATION_AUDIENCE
        || !job_integrity_token(&revocation.revocation_id, 1, 120)
        || revocation.policy_sha256 != current_policy_sha256
        || !job_integrity_safe_integer(revocation.revocation_generation, true)
        || revocation
            .predecessor_revocation_sha256
            .as_deref()
            .is_some_and(|value| !job_integrity_hex64(value))
        || !KINDS.contains(&revocation.subject_kind.as_str())
        || !job_integrity_text(&revocation.subject_id, 1, 512)
        || !job_integrity_hex64(&revocation.subject_sha256)
        || !job_integrity_token(&revocation.reason_code, 1, 64)
        || !job_integrity_text(&revocation.reason_ref, 1, 240)
        || !job_integrity_safe_integer(revocation.effective_at_ms, false)
        || !job_integrity_safe_integer(revocation.issued_at_ms, false)
        || revocation.issued_at_ms > verification_time_ms.saturating_add(maximum_clock_skew_ms)
        || revocation.effective_at_ms < revocation.issued_at_ms
    {
        return Err(JobIntegrityAuthorityError::InvalidAuthority);
    }
    Ok(())
}

fn verify_job_integrity_authorization(
    request: JobIntegrityAuthorizationVerification<'_>,
) -> JobIntegrityResult<JobIntegrityVerifiedAuthorization> {
    let bytes = job_integrity_decode_base64url_bounded(request.encoded)?;
    let authorization: JobIntegrityAuthorizationV1 = job_integrity_parse_canonical_json(&bytes)?;
    if authorization.version != 1
        || authorization.audience != request.authorization_audience
        || !job_integrity_token(&authorization.authorization_id, 1, 120)
        || authorization.role != request.role_name
        || authorization.policy_sha256 != request.policy_sha256
        || authorization.target_audience != request.target_audience
        || authorization.target_sha256 != request.target_sha256
        || authorization.signed_at_ms < request.minimum_signed_at_ms
        || authorization.signed_at_ms
            > request
                .verification_time_ms
                .saturating_add(request.maximum_clock_skew_ms)
        || authorization.signatures.is_empty()
        || authorization.signatures.len() > 16
        || !authorization
            .signatures
            .windows(2)
            .all(|pair| pair[0].key_id < pair[1].key_id)
    {
        return Err(JobIntegrityAuthorityError::InvalidAuthority);
    }
    let payload = JobIntegrityAuthorizationPayload {
        version: 1,
        audience: request.authorization_audience,
        authorization_id: &authorization.authorization_id,
        role: request.role_name,
        policy_sha256: request.policy_sha256,
        target_audience: request.target_audience,
        target_sha256: request.target_sha256,
        signed_at_ms: authorization.signed_at_ms,
    };
    let payload = job_integrity_canonical_json(&payload)?;
    let mut key_ids = Vec::new();
    let mut effective_expiry = JOB_INTEGRITY_MAX_SAFE_INTEGER;
    for detached in &authorization.signatures {
        let key = request
            .role
            .keys
            .get(&detached.key_id)
            .ok_or(JobIntegrityAuthorityError::InvalidSignature)?;
        if authorization.signed_at_ms < key.valid_from_ms
            || authorization.signed_at_ms >= key.expires_at_ms
        {
            return Err(JobIntegrityAuthorityError::InvalidSignature);
        }
        let signature: [u8; 64] = job_integrity_decode_exact(&detached.signature, 64)?
            .try_into()
            .map_err(|_| JobIntegrityAuthorityError::InvalidSignature)?;
        let verifying_key = job_integrity_strong_verifying_key(&key.public_key_base64url)
            .ok_or(JobIntegrityAuthorityError::InvalidSignature)?;
        verifying_key
            .verify_strict(&payload, &ed25519_dalek::Signature::from_bytes(&signature))
            .map_err(|_| JobIntegrityAuthorityError::InvalidSignature)?;
        key_ids.push(detached.key_id.clone());
        effective_expiry = effective_expiry.min(key.expires_at_ms);
    }
    if key_ids.len() < request.role.threshold as usize {
        return Err(JobIntegrityAuthorityError::ThresholdNotMet);
    }
    Ok(JobIntegrityVerifiedAuthorization {
        authorization_id: authorization.authorization_id,
        sha256: job_integrity_sha256(bytes),
        key_ids,
        effective_key_expires_at_ms: effective_expiry,
    })
}

fn verify_job_integrity_root_authorization(
    encoded: &str,
    anchor: &JobIntegrityRootTrustAnchorV1,
    target_sha256: &str,
    policy: &JobIntegrityTrustPolicyV1,
    verification_time_ms: i64,
) -> JobIntegrityResult<JobIntegrityVerifiedAuthorization> {
    let role = JobIntegrityTrustRoleV1 {
        threshold: anchor.threshold,
        keys: anchor
            .keys
            .iter()
            .map(|(key_id, public_key)| {
                (
                    key_id.clone(),
                    JobIntegrityTrustKeyV1 {
                        public_key_base64url: public_key.clone(),
                        valid_from_ms: 0,
                        expires_at_ms: JOB_INTEGRITY_MAX_SAFE_INTEGER,
                    },
                )
            })
            .collect(),
    };
    verify_job_integrity_authorization(JobIntegrityAuthorizationVerification {
        encoded,
        authorization_audience: JOB_INTEGRITY_ROOT_AUTHORIZATION_AUDIENCE,
        role_name: "root",
        role: &role,
        policy_sha256: target_sha256,
        target_audience: JOB_INTEGRITY_TRUST_POLICY_AUDIENCE,
        target_sha256,
        minimum_signed_at_ms: policy.issued_at_ms,
        verification_time_ms,
        maximum_clock_skew_ms: 0,
    })
}

fn job_integrity_policy_from_base64(
    canonical_policy_base64url: &str,
) -> JobIntegrityResult<JobIntegrityTrustPolicyV1> {
    let bytes = job_integrity_decode_base64url_bounded(canonical_policy_base64url)?;
    job_integrity_parse_canonical_json(&bytes)
}

fn job_integrity_authorization_from_base64(
    canonical_authorization_base64url: &str,
) -> JobIntegrityResult<(Vec<u8>, JobIntegrityAuthorizationV1)> {
    let bytes = job_integrity_decode_base64url_bounded(canonical_authorization_base64url)?;
    let authorization = job_integrity_parse_canonical_json(&bytes)?;
    Ok((bytes, authorization))
}

#[derive(Debug, Clone)]
struct JobIntegrityControl {
    control_revision: i64,
    policy_sha256: Option<String>,
    trust_generation: i64,
    revocation_sha256: Option<String>,
    revocation_generation: i64,
}

#[derive(Debug, Clone)]
struct JobIntegrityHead {
    head_revision: i64,
    transition_sha256: String,
    attestation_sha256: String,
    attestation_generation: i64,
}

fn job_integrity_any_revocation_sqlite(
    tx: &rusqlite::Transaction<'_>,
    now_ms: i64,
    candidates: &[(&str, &str, &str)],
) -> JobIntegrityResult<bool> {
    for (subject_kind, subject_id, subject_sha256) in candidates {
        if tx
            .query_row(
                "SELECT 1 FROM jobs_job_integrity_revocations
                  WHERE subject_kind=?1 AND subject_id=?2 AND subject_sha256=?3
                    AND effective_at_ms<=?4 LIMIT 1",
                params![subject_kind, subject_id, subject_sha256, now_ms],
                |_| Ok(()),
            )
            .optional()
            .map_err(job_integrity_storage)?
            .is_some()
        {
            return Ok(true);
        }
    }
    Ok(false)
}

fn job_integrity_any_revocation_postgres(
    tx: &mut postgres::Transaction<'_>,
    now_ms: i64,
    candidates: &[(&str, &str, &str)],
) -> JobIntegrityResult<bool> {
    for (subject_kind, subject_id, subject_sha256) in candidates {
        if tx
            .query_opt(
                "SELECT 1 FROM jobs_job_integrity_revocations
                  WHERE subject_kind=$1 AND subject_id=$2 AND subject_sha256=$3
                    AND effective_at_ms<=$4 LIMIT 1 FOR SHARE",
                &[subject_kind, subject_id, subject_sha256, &now_ms],
            )
            .map_err(job_integrity_storage)?
            .is_some()
        {
            return Ok(true);
        }
    }
    Ok(false)
}

fn job_integrity_signing_revocation_candidates<'a>(
    policy_sha256: &str,
    policy: &'a JobIntegrityTrustPolicyV1,
    role_name: &str,
    signer_key_ids: &'a [String],
) -> JobIntegrityResult<Vec<(&'static str, &'a str, String)>> {
    let role = policy
        .delegated_roles
        .get(role_name)
        .ok_or(JobIntegrityAuthorityError::InvalidTrustPolicy)?;
    let mut candidates = vec![(
        "trust_policy",
        policy.policy_id.as_str(),
        policy_sha256.to_string(),
    )];
    for key_id in signer_key_ids {
        let key = role
            .keys
            .get(key_id)
            .ok_or(JobIntegrityAuthorityError::InvalidSignature)?;
        let key_sha256 =
            job_integrity_sha256(job_integrity_decode_exact(&key.public_key_base64url, 32)?);
        candidates.push(("trust_key", key_id.as_str(), key_sha256));
    }
    Ok(candidates)
}

fn job_integrity_signing_authority_revoked_sqlite(
    tx: &rusqlite::Transaction<'_>,
    now_ms: i64,
    policy_sha256: &str,
    policy: &JobIntegrityTrustPolicyV1,
    role_name: &str,
    signer_key_ids: &[String],
) -> JobIntegrityResult<bool> {
    let candidates = job_integrity_signing_revocation_candidates(
        policy_sha256,
        policy,
        role_name,
        signer_key_ids,
    )?;
    let borrowed = candidates
        .iter()
        .map(|(kind, id, sha256)| (*kind, *id, sha256.as_str()))
        .collect::<Vec<_>>();
    job_integrity_any_revocation_sqlite(tx, now_ms, &borrowed)
}

fn job_integrity_signing_authority_revoked_postgres(
    tx: &mut postgres::Transaction<'_>,
    now_ms: i64,
    policy_sha256: &str,
    policy: &JobIntegrityTrustPolicyV1,
    role_name: &str,
    signer_key_ids: &[String],
) -> JobIntegrityResult<bool> {
    let candidates = job_integrity_signing_revocation_candidates(
        policy_sha256,
        policy,
        role_name,
        signer_key_ids,
    )?;
    let borrowed = candidates
        .iter()
        .map(|(kind, id, sha256)| (*kind, *id, sha256.as_str()))
        .collect::<Vec<_>>();
    job_integrity_any_revocation_postgres(tx, now_ms, &borrowed)
}

fn load_job_integrity_control_sqlite(
    tx: &rusqlite::Transaction<'_>,
) -> JobIntegrityResult<JobIntegrityControl> {
    tx.query_row(
        "SELECT control_revision, current_policy_sha256, current_trust_generation,
                current_revocation_sha256, current_revocation_generation
           FROM jobs_job_integrity_control WHERE singleton_id=1",
        [],
        |row| {
            Ok(JobIntegrityControl {
                control_revision: row.get(0)?,
                policy_sha256: row.get(1)?,
                trust_generation: row.get(2)?,
                revocation_sha256: row.get(3)?,
                revocation_generation: row.get(4)?,
            })
        },
    )
    .map_err(job_integrity_storage)
}

fn load_job_integrity_control_postgres(
    tx: &mut postgres::Transaction<'_>,
    exclusive: bool,
) -> JobIntegrityResult<JobIntegrityControl> {
    let lock = if exclusive { "FOR UPDATE" } else { "FOR SHARE" };
    let query = format!(
        "SELECT control_revision, current_policy_sha256, current_trust_generation,
                current_revocation_sha256, current_revocation_generation
           FROM jobs_job_integrity_control WHERE singleton_id=1 {lock}"
    );
    let row = tx.query_one(&query, &[]).map_err(job_integrity_storage)?;
    Ok(JobIntegrityControl {
        control_revision: row.get(0),
        policy_sha256: row.get(1),
        trust_generation: row.get(2),
        revocation_sha256: row.get(3),
        revocation_generation: row.get(4),
    })
}

/// Freeze every integrity publication while a multi-posting representation is
/// assembled. Policy, revocation, and attestation publishers all acquire this
/// singleton row exclusively before changing any integrity authority row, so
/// one shared lock also covers subjects whose head does not exist yet.
pub(crate) fn lock_job_integrity_publication_fence_shared_postgres_tx(
    tx: &mut postgres::Transaction<'_>,
) -> JobIntegrityResult<()> {
    load_job_integrity_control_postgres(tx, false).map(|_| ())
}

fn load_job_integrity_control_postgres_after_publication_fence(
    tx: &mut postgres::Transaction<'_>,
) -> JobIntegrityResult<JobIntegrityControl> {
    // The caller already owns the singleton control row FOR SHARE. Avoid a
    // per-posting row-lock re-entry while reading its frozen policy pointers.
    let row = tx
        .query_one(
            "SELECT control_revision, current_policy_sha256, current_trust_generation,
                    current_revocation_sha256, current_revocation_generation
               FROM jobs_job_integrity_control WHERE singleton_id=1",
            &[],
        )
        .map_err(job_integrity_storage)?;
    Ok(JobIntegrityControl {
        control_revision: row.get(0),
        policy_sha256: row.get(1),
        trust_generation: row.get(2),
        revocation_sha256: row.get(3),
        revocation_generation: row.get(4),
    })
}

fn load_job_integrity_policy_sqlite(
    tx: &rusqlite::Transaction<'_>,
    policy_sha256: &str,
) -> JobIntegrityResult<JobIntegrityTrustPolicyV1> {
    let encoded: String = tx
        .query_row(
            "SELECT canonical_policy_base64url
               FROM jobs_job_integrity_trust_policies WHERE policy_sha256=?1",
            params![policy_sha256],
            |row| row.get(0),
        )
        .optional()
        .map_err(job_integrity_storage)?
        .ok_or(JobIntegrityAuthorityError::NotInitialized)?;
    let policy = job_integrity_policy_from_base64(&encoded)?;
    let bytes = job_integrity_decode_base64url_bounded(&encoded)?;
    if job_integrity_sha256(bytes) != policy_sha256 {
        return Err(JobIntegrityAuthorityError::IdentityConflict);
    }
    Ok(policy)
}

fn load_job_integrity_policy_postgres(
    tx: &mut postgres::Transaction<'_>,
    policy_sha256: &str,
) -> JobIntegrityResult<JobIntegrityTrustPolicyV1> {
    let row = tx
        .query_opt(
            "SELECT canonical_policy_base64url
               FROM jobs_job_integrity_trust_policies
              WHERE policy_sha256=$1 FOR SHARE",
            &[&policy_sha256],
        )
        .map_err(job_integrity_storage)?
        .ok_or(JobIntegrityAuthorityError::NotInitialized)?;
    let encoded: String = row.get(0);
    let policy = job_integrity_policy_from_base64(&encoded)?;
    let bytes = job_integrity_decode_base64url_bounded(&encoded)?;
    if job_integrity_sha256(bytes) != policy_sha256 {
        return Err(JobIntegrityAuthorityError::IdentityConflict);
    }
    Ok(policy)
}

fn load_job_integrity_policy_postgres_after_publication_fence(
    tx: &mut postgres::Transaction<'_>,
    policy_sha256: &str,
) -> JobIntegrityResult<JobIntegrityTrustPolicyV1> {
    let row = tx
        .query_opt(
            "SELECT canonical_policy_base64url
               FROM jobs_job_integrity_trust_policies
              WHERE policy_sha256=$1",
            &[&policy_sha256],
        )
        .map_err(job_integrity_storage)?
        .ok_or(JobIntegrityAuthorityError::NotInitialized)?;
    let encoded: String = row.get(0);
    let policy = job_integrity_policy_from_base64(&encoded)?;
    let bytes = job_integrity_decode_base64url_bounded(&encoded)?;
    if job_integrity_sha256(bytes) != policy_sha256 {
        return Err(JobIntegrityAuthorityError::IdentityConflict);
    }
    Ok(policy)
}

fn current_job_integrity_policy_sqlite(
    tx: &rusqlite::Transaction<'_>,
    control: &JobIntegrityControl,
) -> JobIntegrityResult<(String, JobIntegrityTrustPolicyV1)> {
    let sha256 = control
        .policy_sha256
        .clone()
        .ok_or(JobIntegrityAuthorityError::NotInitialized)?;
    let policy = load_job_integrity_policy_sqlite(tx, &sha256)?;
    if policy.trust_generation != control.trust_generation {
        return Err(JobIntegrityAuthorityError::IdentityConflict);
    }
    Ok((sha256, policy))
}

fn current_job_integrity_policy_postgres(
    tx: &mut postgres::Transaction<'_>,
    control: &JobIntegrityControl,
) -> JobIntegrityResult<(String, JobIntegrityTrustPolicyV1)> {
    let sha256 = control
        .policy_sha256
        .clone()
        .ok_or(JobIntegrityAuthorityError::NotInitialized)?;
    let policy = load_job_integrity_policy_postgres(tx, &sha256)?;
    if policy.trust_generation != control.trust_generation {
        return Err(JobIntegrityAuthorityError::IdentityConflict);
    }
    Ok((sha256, policy))
}

fn ensure_job_integrity_key_role_history_sqlite(
    tx: &rusqlite::Transaction<'_>,
    policy: &JobIntegrityTrustPolicyV1,
) -> JobIntegrityResult<()> {
    for (role_name, role) in &policy.delegated_roles {
        for (key_id, key) in &role.keys {
            let conflict = tx
                .query_row(
                    "SELECT 1 FROM jobs_job_integrity_trust_keys
                      WHERE (key_id=?1 OR public_key_base64url=?2)
                        AND (role<>?3 OR key_id<>?1 OR public_key_base64url<>?2)
                      LIMIT 1",
                    params![key_id, key.public_key_base64url, role_name],
                    |_| Ok(()),
                )
                .optional()
                .map_err(job_integrity_storage)?
                .is_some();
            if conflict {
                return Err(JobIntegrityAuthorityError::InvalidTrustPolicy);
            }
        }
    }
    Ok(())
}

fn ensure_job_integrity_key_role_history_postgres(
    tx: &mut postgres::Transaction<'_>,
    policy: &JobIntegrityTrustPolicyV1,
) -> JobIntegrityResult<()> {
    for (role_name, role) in &policy.delegated_roles {
        for (key_id, key) in &role.keys {
            if tx
                .query_opt(
                    "SELECT 1 FROM jobs_job_integrity_trust_keys
                      WHERE (key_id=$1 OR public_key_base64url=$2)
                        AND (role<>$3 OR key_id<>$1 OR public_key_base64url<>$2)
                      LIMIT 1 FOR SHARE",
                    &[&key_id, &key.public_key_base64url, &role_name],
                )
                .map_err(job_integrity_storage)?
                .is_some()
            {
                return Err(JobIntegrityAuthorityError::InvalidTrustPolicy);
            }
        }
    }
    Ok(())
}

fn job_integrity_policy_package_bytes(
    package: &JobIntegrityTrustPolicyPackageV1,
) -> JobIntegrityResult<ParsedJobIntegrityTrustPolicyPackage> {
    let policy_bytes = job_integrity_decode_base64url_bounded(&package.canonical_policy_base64url)?;
    let policy: JobIntegrityTrustPolicyV1 = job_integrity_parse_canonical_json(&policy_bytes)?;
    let (authorization_bytes, authorization) =
        job_integrity_authorization_from_base64(&package.canonical_root_authorization_base64url)?;
    Ok(ParsedJobIntegrityTrustPolicyPackage {
        policy_bytes,
        authorization_bytes,
        policy,
        authorization,
    })
}

pub fn import_job_integrity_trust_policy(
    pool: &DbPool,
    package: &JobIntegrityTrustPolicyPackageV1,
    recorded_by: &str,
) -> JobIntegrityResult<JobIntegrityImportResult> {
    crate::db::run_blocking_db(|| {
        let anchor = job_integrity_root_anchor_from_env()?;
        import_job_integrity_trust_policy_with_root(pool, package, &anchor, recorded_by)
    })
}

fn import_job_integrity_trust_policy_with_root(
    pool: &DbPool,
    package: &JobIntegrityTrustPolicyPackageV1,
    root_anchor: &JobIntegrityRootTrustAnchorV1,
    recorded_by: &str,
) -> JobIntegrityResult<JobIntegrityImportResult> {
    if !job_integrity_text(recorded_by, 1, 240) {
        return Err(JobIntegrityAuthorityError::InvalidAuthority);
    }
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get().map_err(job_integrity_storage)?;
            let tx = conn
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(job_integrity_storage)?;
            let result = import_job_integrity_trust_policy_sqlite_tx(
                &tx,
                package,
                root_anchor,
                recorded_by,
            )?;
            tx.commit().map_err(job_integrity_storage)?;
            Ok(result)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg().map_err(job_integrity_storage)?;
            let mut tx = conn.transaction().map_err(job_integrity_storage)?;
            let result = import_job_integrity_trust_policy_postgres_tx(
                &mut tx,
                package,
                root_anchor,
                recorded_by,
            )?;
            tx.commit().map_err(job_integrity_storage)?;
            Ok(result)
        }
    })
}

fn import_job_integrity_trust_policy_sqlite_tx(
    tx: &rusqlite::Transaction<'_>,
    package: &JobIntegrityTrustPolicyPackageV1,
    root_anchor: &JobIntegrityRootTrustAnchorV1,
    recorded_by: &str,
) -> JobIntegrityResult<JobIntegrityImportResult> {
    let parsed = job_integrity_policy_package_bytes(package)?;
    let ParsedJobIntegrityTrustPolicyPackage {
        policy_bytes,
        authorization_bytes,
        policy,
        authorization: raw_authorization,
    } = parsed;
    validate_job_integrity_policy(&policy, root_anchor)?;
    let policy_sha256 = job_integrity_sha256(&policy_bytes);
    let root_anchor_sha256 = job_integrity_root_anchor_sha256(root_anchor)?;
    let control = load_job_integrity_control_sqlite(tx)?;
    if let Some(current_policy_sha256) = control.policy_sha256.as_deref() {
        let stored_root_anchor_sha256: String = tx
            .query_row(
                "SELECT root_anchor_sha256 FROM jobs_job_integrity_trust_policies
                  WHERE policy_sha256=?1",
                params![current_policy_sha256],
                |row| row.get(0),
            )
            .map_err(job_integrity_storage)?;
        if stored_root_anchor_sha256 != root_anchor_sha256 {
            return Err(JobIntegrityAuthorityError::InvalidTrustAnchor);
        }
    }
    let raw_authorization_sha256 = job_integrity_sha256(authorization_bytes);
    let existing = tx
        .query_row(
            "SELECT policy_sha256, canonical_policy_base64url,
                    canonical_root_authorization_base64url, trust_generation
               FROM jobs_job_integrity_trust_policies
              WHERE policy_sha256=?1 OR policy_id=?2 OR trust_generation=?3
                 OR root_authorization_sha256=?4
                 OR root_authorization_id=?5
                 OR (?6 IS NOT NULL AND predecessor_policy_sha256=?6) LIMIT 1",
            params![
                policy_sha256,
                policy.policy_id,
                policy.trust_generation,
                raw_authorization_sha256,
                raw_authorization.authorization_id,
                policy.predecessor_policy_sha256,
            ],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            },
        )
        .optional()
        .map_err(job_integrity_storage)?;
    if let Some(existing) = existing {
        if existing.0 == policy_sha256
            && existing.1 == package.canonical_policy_base64url
            && existing.2 == package.canonical_root_authorization_base64url
            && existing.3 == policy.trust_generation
        {
            return Ok(JobIntegrityImportResult {
                object_sha256: policy_sha256,
                generation: policy.trust_generation,
                replayed: true,
                head_revision: None,
                head_transition_sha256: None,
            });
        }
        return Err(JobIntegrityAuthorityError::IdentityConflict);
    }
    let now_ms = job_integrity_db_now_sqlite(tx)?;
    let authorization = verify_job_integrity_root_authorization(
        &package.canonical_root_authorization_base64url,
        root_anchor,
        &policy_sha256,
        &policy,
        now_ms,
    )?;
    if authorization.authorization_id != raw_authorization.authorization_id
        || authorization.sha256 != raw_authorization_sha256
    {
        return Err(JobIntegrityAuthorityError::IdentityConflict);
    }
    if policy.trust_generation <= control.trust_generation {
        return Err(JobIntegrityAuthorityError::SequenceRegression);
    }
    if policy.trust_generation != control.trust_generation + 1
        || policy.predecessor_policy_sha256 != control.policy_sha256
    {
        return Err(JobIntegrityAuthorityError::CompareAndSwapConflict);
    }
    if now_ms < policy.valid_from_ms || now_ms >= policy.expires_at_ms {
        return Err(JobIntegrityAuthorityError::Expired);
    }
    ensure_job_integrity_key_role_history_sqlite(tx, &policy)?;
    insert_job_integrity_policy_sqlite(
        tx,
        package,
        &policy,
        &policy_sha256,
        &root_anchor_sha256,
        &authorization.authorization_id,
        &authorization.sha256,
        recorded_by,
        now_ms,
    )?;
    let changed = tx
        .execute(
            "UPDATE jobs_job_integrity_control
                SET control_revision=?1, current_policy_sha256=?2,
                    current_trust_generation=?3, updated_by=?4, updated_at_ms=?5
              WHERE singleton_id=1 AND control_revision=?6
                AND current_trust_generation=?7
                AND current_policy_sha256 IS ?8",
            params![
                control.control_revision + 1,
                policy_sha256,
                policy.trust_generation,
                recorded_by,
                now_ms,
                control.control_revision,
                control.trust_generation,
                control.policy_sha256,
            ],
        )
        .map_err(job_integrity_storage)?;
    if changed != 1 {
        return Err(JobIntegrityAuthorityError::CompareAndSwapConflict);
    }
    Ok(JobIntegrityImportResult {
        object_sha256: policy_sha256,
        generation: policy.trust_generation,
        replayed: false,
        head_revision: None,
        head_transition_sha256: None,
    })
}

#[allow(clippy::too_many_arguments)]
fn insert_job_integrity_policy_sqlite(
    tx: &rusqlite::Transaction<'_>,
    package: &JobIntegrityTrustPolicyPackageV1,
    policy: &JobIntegrityTrustPolicyV1,
    policy_sha256: &str,
    root_anchor_sha256: &str,
    authorization_id: &str,
    authorization_sha256: &str,
    recorded_by: &str,
    recorded_at_ms: i64,
) -> JobIntegrityResult<()> {
    let requirements = &policy.requirements;
    tx.execute(
        "INSERT INTO jobs_job_integrity_trust_policies(
           policy_sha256,policy_id,trust_generation,predecessor_policy_sha256,
           root_anchor_sha256,canonical_policy_base64url,root_authorization_id,
           root_authorization_sha256,canonical_root_authorization_base64url,
           employer_identity_threshold,
           job_risk_threshold,revocation_threshold,maximum_positive_lifetime_ms,
           maximum_nonpositive_lifetime_ms,maximum_clock_skew_ms,maximum_canonical_bytes,
           maximum_identity_evidence_count,maximum_risk_evidence_count,
           allowed_providers_json,required_identity_methods_json,
           required_identity_evidence_classes_json,required_risk_evidence_classes_json,
           allowed_identity_evidence_classes_json,allowed_risk_evidence_classes_json,
           allowed_risk_signal_codes_json,allowed_risk_policy_sha256s_json,
           issued_at_ms,valid_from_ms,expires_at_ms,recorded_by,recorded_at_ms)
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,
                ?18,?19,?20,?21,?22,?23,?24,?25,?26,?27,?28,?29,?30,?31)",
        params![
            policy_sha256,
            policy.policy_id,
            policy.trust_generation,
            policy.predecessor_policy_sha256,
            root_anchor_sha256,
            package.canonical_policy_base64url,
            authorization_id,
            authorization_sha256,
            package.canonical_root_authorization_base64url,
            policy.delegated_roles["employer_identity"].threshold,
            policy.delegated_roles["job_risk"].threshold,
            policy.delegated_roles["revocation"].threshold,
            requirements.maximum_positive_lifetime_ms,
            requirements.maximum_nonpositive_lifetime_ms,
            requirements.maximum_clock_skew_ms,
            requirements.maximum_canonical_bytes,
            requirements.maximum_identity_evidence_count,
            requirements.maximum_risk_evidence_count,
            serde_json::to_string(&requirements.allowed_providers)
                .map_err(job_integrity_storage)?,
            serde_json::to_string(&requirements.required_identity_methods)
                .map_err(job_integrity_storage)?,
            serde_json::to_string(&requirements.required_identity_evidence_classes)
                .map_err(job_integrity_storage)?,
            serde_json::to_string(&requirements.required_risk_evidence_classes)
                .map_err(job_integrity_storage)?,
            serde_json::to_string(&requirements.allowed_identity_evidence_classes)
                .map_err(job_integrity_storage)?,
            serde_json::to_string(&requirements.allowed_risk_evidence_classes)
                .map_err(job_integrity_storage)?,
            serde_json::to_string(&requirements.allowed_risk_signal_codes)
                .map_err(job_integrity_storage)?,
            serde_json::to_string(&requirements.allowed_risk_policy_sha256s)
                .map_err(job_integrity_storage)?,
            policy.issued_at_ms,
            policy.valid_from_ms,
            policy.expires_at_ms,
            recorded_by,
            recorded_at_ms,
        ],
    )
    .map_err(job_integrity_storage)?;
    for (role_name, role) in &policy.delegated_roles {
        for (key_id, key) in &role.keys {
            tx.execute(
                "INSERT INTO jobs_job_integrity_trust_keys(
                   policy_sha256,trust_generation,role,threshold,key_id,
                   public_key_base64url,key_sha256,valid_from_ms,expires_at_ms)
                 VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",
                params![
                    policy_sha256,
                    policy.trust_generation,
                    role_name,
                    role.threshold,
                    key_id,
                    key.public_key_base64url,
                    job_integrity_sha256(job_integrity_decode_exact(
                        &key.public_key_base64url,
                        32
                    )?),
                    key.valid_from_ms,
                    key.expires_at_ms,
                ],
            )
            .map_err(job_integrity_storage)?;
        }
    }
    Ok(())
}

fn import_job_integrity_trust_policy_postgres_tx(
    tx: &mut postgres::Transaction<'_>,
    package: &JobIntegrityTrustPolicyPackageV1,
    root_anchor: &JobIntegrityRootTrustAnchorV1,
    recorded_by: &str,
) -> JobIntegrityResult<JobIntegrityImportResult> {
    let parsed = job_integrity_policy_package_bytes(package)?;
    let ParsedJobIntegrityTrustPolicyPackage {
        policy_bytes,
        authorization_bytes,
        policy,
        authorization: raw_authorization,
    } = parsed;
    validate_job_integrity_policy(&policy, root_anchor)?;
    let policy_sha256 = job_integrity_sha256(&policy_bytes);
    let root_anchor_sha256 = job_integrity_root_anchor_sha256(root_anchor)?;
    let control = load_job_integrity_control_postgres(tx, true)?;
    if let Some(current_policy_sha256) = control.policy_sha256.as_deref() {
        let row = tx
            .query_one(
                "SELECT root_anchor_sha256 FROM jobs_job_integrity_trust_policies
                  WHERE policy_sha256=$1 FOR SHARE",
                &[&current_policy_sha256],
            )
            .map_err(job_integrity_storage)?;
        if row.get::<_, String>(0) != root_anchor_sha256 {
            return Err(JobIntegrityAuthorityError::InvalidTrustAnchor);
        }
    }
    let raw_authorization_sha256 = job_integrity_sha256(authorization_bytes);
    if let Some(row) = tx
        .query_opt(
            "SELECT policy_sha256, canonical_policy_base64url,
                    canonical_root_authorization_base64url, trust_generation
               FROM jobs_job_integrity_trust_policies
              WHERE policy_sha256=$1 OR policy_id=$2 OR trust_generation=$3
                 OR root_authorization_sha256=$4
                 OR root_authorization_id=$5
                 OR ($6::text IS NOT NULL AND predecessor_policy_sha256=$6)
              LIMIT 1 FOR SHARE",
            &[
                &policy_sha256,
                &policy.policy_id,
                &policy.trust_generation,
                &raw_authorization_sha256,
                &raw_authorization.authorization_id,
                &policy.predecessor_policy_sha256,
            ],
        )
        .map_err(job_integrity_storage)?
    {
        if row.get::<_, String>(0) == policy_sha256
            && row.get::<_, String>(1) == package.canonical_policy_base64url
            && row.get::<_, String>(2) == package.canonical_root_authorization_base64url
            && row.get::<_, i64>(3) == policy.trust_generation
        {
            return Ok(JobIntegrityImportResult {
                object_sha256: policy_sha256,
                generation: policy.trust_generation,
                replayed: true,
                head_revision: None,
                head_transition_sha256: None,
            });
        }
        return Err(JobIntegrityAuthorityError::IdentityConflict);
    }
    let now_ms = job_integrity_db_now_postgres(tx)?;
    let authorization = verify_job_integrity_root_authorization(
        &package.canonical_root_authorization_base64url,
        root_anchor,
        &policy_sha256,
        &policy,
        now_ms,
    )?;
    if authorization.authorization_id != raw_authorization.authorization_id
        || authorization.sha256 != raw_authorization_sha256
    {
        return Err(JobIntegrityAuthorityError::IdentityConflict);
    }
    if policy.trust_generation <= control.trust_generation {
        return Err(JobIntegrityAuthorityError::SequenceRegression);
    }
    if policy.trust_generation != control.trust_generation + 1
        || policy.predecessor_policy_sha256 != control.policy_sha256
    {
        return Err(JobIntegrityAuthorityError::CompareAndSwapConflict);
    }
    if now_ms < policy.valid_from_ms || now_ms >= policy.expires_at_ms {
        return Err(JobIntegrityAuthorityError::Expired);
    }
    ensure_job_integrity_key_role_history_postgres(tx, &policy)?;
    insert_job_integrity_policy_postgres(
        tx,
        package,
        &policy,
        &policy_sha256,
        &root_anchor_sha256,
        &authorization.authorization_id,
        &authorization.sha256,
        recorded_by,
        now_ms,
    )?;
    let changed = tx
        .execute(
            "UPDATE jobs_job_integrity_control
                SET control_revision=$1, current_policy_sha256=$2,
                    current_trust_generation=$3, updated_by=$4, updated_at_ms=$5
              WHERE singleton_id=1 AND control_revision=$6
                AND current_trust_generation=$7
                AND current_policy_sha256 IS NOT DISTINCT FROM $8",
            &[
                &(control.control_revision + 1),
                &policy_sha256,
                &policy.trust_generation,
                &recorded_by,
                &now_ms,
                &control.control_revision,
                &control.trust_generation,
                &control.policy_sha256,
            ],
        )
        .map_err(job_integrity_storage)?;
    if changed != 1 {
        return Err(JobIntegrityAuthorityError::CompareAndSwapConflict);
    }
    Ok(JobIntegrityImportResult {
        object_sha256: policy_sha256,
        generation: policy.trust_generation,
        replayed: false,
        head_revision: None,
        head_transition_sha256: None,
    })
}

#[allow(clippy::too_many_arguments)]
fn insert_job_integrity_policy_postgres(
    tx: &mut postgres::Transaction<'_>,
    package: &JobIntegrityTrustPolicyPackageV1,
    policy: &JobIntegrityTrustPolicyV1,
    policy_sha256: &str,
    root_anchor_sha256: &str,
    authorization_id: &str,
    authorization_sha256: &str,
    recorded_by: &str,
    recorded_at_ms: i64,
) -> JobIntegrityResult<()> {
    let requirements = &policy.requirements;
    let values = [
        serde_json::to_string(&requirements.allowed_providers).map_err(job_integrity_storage)?,
        serde_json::to_string(&requirements.required_identity_methods)
            .map_err(job_integrity_storage)?,
        serde_json::to_string(&requirements.required_identity_evidence_classes)
            .map_err(job_integrity_storage)?,
        serde_json::to_string(&requirements.required_risk_evidence_classes)
            .map_err(job_integrity_storage)?,
        serde_json::to_string(&requirements.allowed_identity_evidence_classes)
            .map_err(job_integrity_storage)?,
        serde_json::to_string(&requirements.allowed_risk_evidence_classes)
            .map_err(job_integrity_storage)?,
        serde_json::to_string(&requirements.allowed_risk_signal_codes)
            .map_err(job_integrity_storage)?,
        serde_json::to_string(&requirements.allowed_risk_policy_sha256s)
            .map_err(job_integrity_storage)?,
    ];
    tx.execute(
        "INSERT INTO jobs_job_integrity_trust_policies(
           policy_sha256,policy_id,trust_generation,predecessor_policy_sha256,
           root_anchor_sha256,canonical_policy_base64url,root_authorization_id,
           root_authorization_sha256,canonical_root_authorization_base64url,
           employer_identity_threshold,
           job_risk_threshold,revocation_threshold,maximum_positive_lifetime_ms,
           maximum_nonpositive_lifetime_ms,maximum_clock_skew_ms,maximum_canonical_bytes,
           maximum_identity_evidence_count,maximum_risk_evidence_count,
           allowed_providers_json,required_identity_methods_json,
           required_identity_evidence_classes_json,required_risk_evidence_classes_json,
           allowed_identity_evidence_classes_json,allowed_risk_evidence_classes_json,
           allowed_risk_signal_codes_json,allowed_risk_policy_sha256s_json,
           issued_at_ms,valid_from_ms,expires_at_ms,recorded_by,recorded_at_ms)
         VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,
                $18,$19,$20,$21,$22,$23,$24,$25,$26,$27,$28,$29,$30,$31)",
        &[
            &policy_sha256,
            &policy.policy_id,
            &policy.trust_generation,
            &policy.predecessor_policy_sha256,
            &root_anchor_sha256,
            &package.canonical_policy_base64url,
            &authorization_id,
            &authorization_sha256,
            &package.canonical_root_authorization_base64url,
            &policy.delegated_roles["employer_identity"].threshold,
            &policy.delegated_roles["job_risk"].threshold,
            &policy.delegated_roles["revocation"].threshold,
            &requirements.maximum_positive_lifetime_ms,
            &requirements.maximum_nonpositive_lifetime_ms,
            &requirements.maximum_clock_skew_ms,
            &requirements.maximum_canonical_bytes,
            &requirements.maximum_identity_evidence_count,
            &requirements.maximum_risk_evidence_count,
            &values[0],
            &values[1],
            &values[2],
            &values[3],
            &values[4],
            &values[5],
            &values[6],
            &values[7],
            &policy.issued_at_ms,
            &policy.valid_from_ms,
            &policy.expires_at_ms,
            &recorded_by,
            &recorded_at_ms,
        ],
    )
    .map_err(job_integrity_storage)?;
    for (role_name, role) in &policy.delegated_roles {
        for (key_id, key) in &role.keys {
            let key_sha256 =
                job_integrity_sha256(job_integrity_decode_exact(&key.public_key_base64url, 32)?);
            tx.execute(
                "INSERT INTO jobs_job_integrity_trust_keys(
                   policy_sha256,trust_generation,role,threshold,key_id,
                   public_key_base64url,key_sha256,valid_from_ms,expires_at_ms)
                 VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)",
                &[
                    &policy_sha256,
                    &policy.trust_generation,
                    &role_name,
                    &role.threshold,
                    &key_id,
                    &key.public_key_base64url,
                    &key_sha256,
                    &key.valid_from_ms,
                    &key.expires_at_ms,
                ],
            )
            .map_err(job_integrity_storage)?;
        }
    }
    Ok(())
}

fn job_integrity_attestation_package_bytes(
    package: &JobIntegrityAttestationPackageV1,
) -> JobIntegrityResult<ParsedJobIntegrityAttestationPackage> {
    let bytes = job_integrity_decode_base64url_bounded(&package.canonical_attestation_base64url)?;
    let attestation = job_integrity_parse_canonical_json(&bytes)?;
    let (employer_bytes, employer_authorization) = job_integrity_authorization_from_base64(
        &package.canonical_employer_identity_authorization_base64url,
    )?;
    let (risk_bytes, risk_authorization) = job_integrity_authorization_from_base64(
        &package.canonical_job_risk_authorization_base64url,
    )?;
    Ok(ParsedJobIntegrityAttestationPackage {
        attestation_bytes: bytes,
        attestation,
        employer_authorization_bytes: employer_bytes,
        employer_authorization,
        risk_authorization_bytes: risk_bytes,
        risk_authorization,
    })
}

fn verify_job_integrity_attestation_package(
    package: &JobIntegrityAttestationPackageV1,
    policy_sha256: &str,
    policy: &JobIntegrityTrustPolicyV1,
    now_ms: i64,
) -> JobIntegrityResult<JobIntegrityVerifiedAttestation> {
    let parsed = job_integrity_attestation_package_bytes(package)?;
    let ParsedJobIntegrityAttestationPackage {
        attestation_bytes: bytes,
        attestation,
        employer_authorization: raw_employer_authorization,
        risk_authorization: raw_risk_authorization,
        ..
    } = parsed;
    if attestation.policy_sha256 != policy_sha256 {
        return Err(JobIntegrityAuthorityError::InvalidAuthority);
    }
    let effective_expiry =
        validate_job_integrity_attestation(&attestation, policy, bytes.len(), now_ms)?;
    let sha256 = job_integrity_sha256(&bytes);
    let employer_authorization =
        verify_job_integrity_authorization(JobIntegrityAuthorizationVerification {
            encoded: &package.canonical_employer_identity_authorization_base64url,
            authorization_audience: JOB_INTEGRITY_AUTHORIZATION_AUDIENCE,
            role_name: "employer_identity",
            role: &policy.delegated_roles["employer_identity"],
            policy_sha256,
            target_audience: JOB_INTEGRITY_ATTESTATION_AUDIENCE,
            target_sha256: &sha256,
            minimum_signed_at_ms: attestation.issued_at_ms,
            verification_time_ms: now_ms,
            maximum_clock_skew_ms: policy.requirements.maximum_clock_skew_ms,
        })?;
    let risk_authorization =
        verify_job_integrity_authorization(JobIntegrityAuthorizationVerification {
            encoded: &package.canonical_job_risk_authorization_base64url,
            authorization_audience: JOB_INTEGRITY_AUTHORIZATION_AUDIENCE,
            role_name: "job_risk",
            role: &policy.delegated_roles["job_risk"],
            policy_sha256,
            target_audience: JOB_INTEGRITY_ATTESTATION_AUDIENCE,
            target_sha256: &sha256,
            minimum_signed_at_ms: attestation.issued_at_ms,
            verification_time_ms: now_ms,
            maximum_clock_skew_ms: policy.requirements.maximum_clock_skew_ms,
        })?;
    if employer_authorization.authorization_id != raw_employer_authorization.authorization_id
        || risk_authorization.authorization_id != raw_risk_authorization.authorization_id
        || employer_authorization
            .key_ids
            .iter()
            .any(|key| risk_authorization.key_ids.contains(key))
    {
        return Err(JobIntegrityAuthorityError::InvalidSignature);
    }
    Ok(JobIntegrityVerifiedAttestation {
        attestation,
        sha256,
        effective_expires_at_ms: effective_expiry
            .min(employer_authorization.effective_key_expires_at_ms)
            .min(risk_authorization.effective_key_expires_at_ms),
        employer_authorization,
        risk_authorization,
    })
}

pub fn import_job_integrity_attestation(
    pool: &DbPool,
    package: &JobIntegrityAttestationPackageV1,
    recorded_by: &str,
) -> JobIntegrityResult<JobIntegrityImportResult> {
    if !job_integrity_text(recorded_by, 1, 240) {
        return Err(JobIntegrityAuthorityError::InvalidAuthority);
    }
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get().map_err(job_integrity_storage)?;
            let tx = conn
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(job_integrity_storage)?;
            let result = import_job_integrity_attestation_sqlite_tx(&tx, package, recorded_by)?;
            tx.commit().map_err(job_integrity_storage)?;
            Ok(result)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg().map_err(job_integrity_storage)?;
            let mut tx = conn.transaction().map_err(job_integrity_storage)?;
            let result =
                import_job_integrity_attestation_postgres_tx(&mut tx, package, recorded_by)?;
            tx.commit().map_err(job_integrity_storage)?;
            Ok(result)
        }
    })
}

fn existing_job_integrity_attestation_sqlite(
    tx: &rusqlite::Transaction<'_>,
    attestation: &JobIntegrityAttestationV1,
    attestation_sha256: &str,
    employer_authorization_sha256: &str,
    employer_authorization_id: &str,
    risk_authorization_sha256: &str,
    risk_authorization_id: &str,
) -> JobIntegrityResult<Option<JobIntegrityExistingAttestation>> {
    tx.query_row(
        "SELECT attestation.attestation_sha256,
                attestation.canonical_attestation_base64url,
                attestation.canonical_employer_authorization_base64url,
                attestation.canonical_risk_authorization_base64url,
                attestation.attestation_generation,
                COALESCE(transition.head_revision,0),
                COALESCE(transition.transition_sha256,'')
           FROM jobs_job_integrity_attestations attestation
           LEFT JOIN jobs_job_integrity_head_transitions transition
             ON transition.attestation_sha256=attestation.attestation_sha256
          WHERE attestation.attestation_sha256=?1 OR attestation.attestation_id=?2
             OR (attestation.subject_sha256=?3 AND attestation.attestation_generation=?4)
             OR attestation.employer_authorization_sha256=?5
             OR attestation.employer_identity_authorization_id=?6
             OR attestation.risk_authorization_sha256=?7
             OR attestation.job_risk_authorization_id=?8
             OR (?9 IS NOT NULL AND attestation.predecessor_attestation_sha256=?9)
          LIMIT 1",
        params![
            attestation_sha256,
            attestation.attestation_id,
            attestation.subject_sha256,
            attestation.attestation_generation,
            employer_authorization_sha256,
            employer_authorization_id,
            risk_authorization_sha256,
            risk_authorization_id,
            attestation.predecessor_attestation_sha256,
        ],
        |row| {
            Ok(JobIntegrityExistingAttestation {
                sha256: row.get(0)?,
                canonical_attestation_base64url: row.get(1)?,
                canonical_employer_authorization_base64url: row.get(2)?,
                canonical_risk_authorization_base64url: row.get(3)?,
                generation: row.get(4)?,
                head_revision: row.get(5)?,
                head_transition_sha256: row.get(6)?,
            })
        },
    )
    .optional()
    .map_err(job_integrity_storage)
}

fn load_job_integrity_head_sqlite(
    tx: &rusqlite::Transaction<'_>,
    subject_sha256: &str,
) -> JobIntegrityResult<Option<JobIntegrityHead>> {
    tx.query_row(
        "SELECT head_revision,transition_sha256,attestation_sha256,attestation_generation
           FROM jobs_job_integrity_heads WHERE subject_sha256=?1",
        params![subject_sha256],
        |row| {
            Ok(JobIntegrityHead {
                head_revision: row.get(0)?,
                transition_sha256: row.get(1)?,
                attestation_sha256: row.get(2)?,
                attestation_generation: row.get(3)?,
            })
        },
    )
    .optional()
    .map_err(job_integrity_storage)
}

fn import_job_integrity_attestation_sqlite_tx(
    tx: &rusqlite::Transaction<'_>,
    package: &JobIntegrityAttestationPackageV1,
    recorded_by: &str,
) -> JobIntegrityResult<JobIntegrityImportResult> {
    let parsed = job_integrity_attestation_package_bytes(package)?;
    let ParsedJobIntegrityAttestationPackage {
        attestation_bytes: raw_bytes,
        attestation: raw_attestation,
        employer_authorization_bytes: raw_employer_authorization_bytes,
        employer_authorization: raw_employer_authorization,
        risk_authorization_bytes: raw_risk_authorization_bytes,
        risk_authorization: raw_risk_authorization,
    } = parsed;
    let raw_sha256 = job_integrity_sha256(&raw_bytes);
    let raw_employer_authorization_sha256 = job_integrity_sha256(raw_employer_authorization_bytes);
    let raw_risk_authorization_sha256 = job_integrity_sha256(raw_risk_authorization_bytes);
    let control = load_job_integrity_control_sqlite(tx)?;
    let (policy_sha256, policy) = current_job_integrity_policy_sqlite(tx, &control)?;
    let head = load_job_integrity_head_sqlite(tx, &raw_attestation.subject_sha256)?;
    if let Some(existing) = existing_job_integrity_attestation_sqlite(
        tx,
        &raw_attestation,
        &raw_sha256,
        &raw_employer_authorization_sha256,
        &raw_employer_authorization.authorization_id,
        &raw_risk_authorization_sha256,
        &raw_risk_authorization.authorization_id,
    )? {
        if existing.sha256 == raw_sha256
            && existing.canonical_attestation_base64url == package.canonical_attestation_base64url
            && existing.canonical_employer_authorization_base64url
                == package.canonical_employer_identity_authorization_base64url
            && existing.canonical_risk_authorization_base64url
                == package.canonical_job_risk_authorization_base64url
        {
            return Ok(JobIntegrityImportResult {
                object_sha256: raw_sha256,
                generation: existing.generation,
                replayed: true,
                head_revision: (existing.head_revision > 0).then_some(existing.head_revision),
                head_transition_sha256: (!existing.head_transition_sha256.is_empty())
                    .then_some(existing.head_transition_sha256),
            });
        }
        return Err(JobIntegrityAuthorityError::IdentityConflict);
    }
    let now_ms = job_integrity_db_now_sqlite(tx)?;
    if now_ms < policy.valid_from_ms || now_ms >= policy.expires_at_ms {
        return Err(JobIntegrityAuthorityError::Expired);
    }
    let verified =
        verify_job_integrity_attestation_package(package, &policy_sha256, &policy, now_ms)?;
    let JobIntegrityVerifiedAttestation {
        attestation,
        sha256: attestation_sha256,
        employer_authorization: employer_auth,
        risk_authorization: risk_auth,
        effective_expires_at_ms: effective_expiry,
    } = verified;
    if effective_expiry <= now_ms {
        return Err(JobIntegrityAuthorityError::Expired);
    }
    if job_integrity_signing_authority_revoked_sqlite(
        tx,
        now_ms,
        &policy_sha256,
        &policy,
        "employer_identity",
        &employer_auth.key_ids,
    )? || job_integrity_signing_authority_revoked_sqlite(
        tx,
        now_ms,
        &policy_sha256,
        &policy,
        "job_risk",
        &risk_auth.key_ids,
    )? {
        return Err(JobIntegrityAuthorityError::Revoked);
    }
    if attestation.employer.status == "verified" && attestation.risk.status == "clear" {
        let candidates = job_integrity_revocation_candidates_for_attestation(
            &policy,
            &policy_sha256,
            &attestation,
            &attestation_sha256,
            &employer_auth.key_ids,
            &risk_auth.key_ids,
        )?;
        let borrowed = candidates
            .iter()
            .map(|(kind, id, sha256)| (kind.as_str(), id.as_str(), sha256.as_str()))
            .collect::<Vec<_>>();
        if job_integrity_any_revocation_sqlite(tx, now_ms, &borrowed)? {
            return Err(JobIntegrityAuthorityError::Revoked);
        }
    }
    if attestation.subject_sha256 != raw_attestation.subject_sha256 {
        return Err(JobIntegrityAuthorityError::IdentityConflict);
    }
    let expected_generation = head
        .as_ref()
        .map_or(1, |value| value.attestation_generation + 1);
    if attestation.attestation_generation < expected_generation {
        return Err(JobIntegrityAuthorityError::SequenceRegression);
    }
    if attestation.attestation_generation != expected_generation
        || attestation.predecessor_attestation_sha256
            != head.as_ref().map(|value| value.attestation_sha256.clone())
    {
        return Err(JobIntegrityAuthorityError::CompareAndSwapConflict);
    }
    insert_job_integrity_attestation_sqlite(
        tx,
        package,
        &attestation,
        &attestation_sha256,
        &employer_auth.authorization_id,
        &employer_auth.sha256,
        &risk_auth.authorization_id,
        &risk_auth.sha256,
        effective_expiry,
        recorded_by,
        now_ms,
    )?;
    let head_revision = head.as_ref().map_or(1, |value| value.head_revision + 1);
    let previous_head_revision = head.as_ref().map_or(0, |value| value.head_revision);
    let transition = JobIntegrityHeadTransitionCanonical {
        version: 1,
        audience: JOB_INTEGRITY_TRANSITION_AUDIENCE,
        subject_sha256: &attestation.subject_sha256,
        head_revision,
        previous_head_revision,
        predecessor_transition_sha256: head.as_ref().map(|value| value.transition_sha256.as_str()),
        previous_attestation_sha256: head.as_ref().map(|value| value.attestation_sha256.as_str()),
        attestation_sha256: &attestation_sha256,
        attestation_generation: attestation.attestation_generation,
        policy_sha256: &policy_sha256,
        transition_actor: recorded_by,
        transitioned_at_ms: now_ms,
    };
    let transition_sha256 = job_integrity_sha256(job_integrity_canonical_json(&transition)?);
    tx.execute(
        "INSERT INTO jobs_job_integrity_head_transitions(
           transition_sha256,subject_sha256,head_revision,previous_head_revision,
           predecessor_transition_sha256,previous_attestation_sha256,
           attestation_sha256,attestation_generation,policy_sha256,
           transition_actor,transitioned_at_ms)
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
        params![
            transition_sha256,
            attestation.subject_sha256,
            head_revision,
            previous_head_revision,
            head.as_ref().map(|value| value.transition_sha256.as_str()),
            head.as_ref().map(|value| value.attestation_sha256.as_str()),
            attestation_sha256,
            attestation.attestation_generation,
            policy_sha256,
            recorded_by,
            now_ms,
        ],
    )
    .map_err(job_integrity_storage)?;
    let changed = if let Some(head) = head {
        tx.execute(
            "UPDATE jobs_job_integrity_heads
                SET head_revision=?1,transition_sha256=?2,attestation_sha256=?3,
                    attestation_generation=?4,policy_sha256=?5,updated_at_ms=?6
              WHERE subject_sha256=?7 AND head_revision=?8 AND transition_sha256=?9
                AND attestation_sha256=?10",
            params![
                head_revision,
                transition_sha256,
                attestation_sha256,
                attestation.attestation_generation,
                policy_sha256,
                now_ms,
                attestation.subject_sha256,
                head.head_revision,
                head.transition_sha256,
                head.attestation_sha256,
            ],
        )
        .map_err(job_integrity_storage)?
    } else {
        tx.execute(
            "INSERT INTO jobs_job_integrity_heads(
               subject_sha256,head_revision,transition_sha256,attestation_sha256,
               attestation_generation,policy_sha256,updated_at_ms)
             VALUES(?1,?2,?3,?4,?5,?6,?7)",
            params![
                attestation.subject_sha256,
                head_revision,
                transition_sha256,
                attestation_sha256,
                attestation.attestation_generation,
                policy_sha256,
                now_ms,
            ],
        )
        .map_err(job_integrity_storage)?
    };
    if changed != 1 {
        return Err(JobIntegrityAuthorityError::CompareAndSwapConflict);
    }
    Ok(JobIntegrityImportResult {
        object_sha256: attestation_sha256,
        generation: attestation.attestation_generation,
        replayed: false,
        head_revision: Some(head_revision),
        head_transition_sha256: Some(transition_sha256),
    })
}

#[allow(clippy::too_many_arguments)]
fn insert_job_integrity_attestation_sqlite(
    tx: &rusqlite::Transaction<'_>,
    package: &JobIntegrityAttestationPackageV1,
    attestation: &JobIntegrityAttestationV1,
    attestation_sha256: &str,
    employer_authorization_id: &str,
    employer_authorization_sha256: &str,
    risk_authorization_id: &str,
    risk_authorization_sha256: &str,
    effective_expires_at_ms: i64,
    recorded_by: &str,
    recorded_at_ms: i64,
) -> JobIntegrityResult<()> {
    tx.execute(
        "INSERT INTO jobs_job_integrity_attestations(
           attestation_sha256,attestation_id,policy_sha256,subject_sha256,
           source_material_sha256,attestation_generation,predecessor_attestation_sha256,
           canonical_job_id,provider_family,provider_record_id,provider_host,
           provider_tenant,provider_job,provider_variant,canonical_application_url,
           application_domain,ats_tenant_binding_sha256,employer_status,
           canonical_employer_id,canonical_employer_domain,verification_methods_json,
           identity_evidence_json,risk_status,risk_signal_codes_json,risk_policy_sha256,
           risk_input_sha256,risk_engine_release_sha256,risk_evidence_json,
           canonical_attestation_base64url,employer_identity_authorization_id,
           employer_authorization_sha256,canonical_employer_authorization_base64url,
           job_risk_authorization_id,risk_authorization_sha256,
           canonical_risk_authorization_base64url,assessed_at_ms,issued_at_ms,
           not_before_ms,expires_at_ms,effective_expires_at_ms,recorded_by,recorded_at_ms)
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,
                ?18,?19,?20,?21,?22,?23,?24,?25,?26,?27,?28,?29,?30,?31,?32,
                ?33,?34,?35,?36,?37,?38,?39,?40,?41,?42)",
        params![
            attestation_sha256,
            attestation.attestation_id,
            attestation.policy_sha256,
            attestation.subject_sha256,
            attestation.source_material_sha256,
            attestation.attestation_generation,
            attestation.predecessor_attestation_sha256,
            attestation.canonical_job_id,
            attestation.source.provider_family,
            attestation.source.provider_record_id,
            attestation.source.target.host,
            attestation.source.target.tenant,
            attestation.source.target.job,
            attestation.source.target.variant,
            attestation.source.canonical_application_url,
            attestation.source.application_domain,
            attestation.source.ats_tenant_binding_sha256,
            attestation.employer.status,
            attestation.employer.canonical_employer_id,
            attestation.employer.canonical_employer_domain,
            serde_json::to_string(&attestation.employer.verification_methods)
                .map_err(job_integrity_storage)?,
            serde_json::to_string(&attestation.employer.evidence).map_err(job_integrity_storage)?,
            attestation.risk.status,
            serde_json::to_string(&attestation.risk.signal_codes).map_err(job_integrity_storage)?,
            attestation.risk.policy_sha256,
            attestation.risk.input_sha256,
            attestation.risk.engine_release_sha256,
            serde_json::to_string(&attestation.risk.evidence).map_err(job_integrity_storage)?,
            package.canonical_attestation_base64url,
            employer_authorization_id,
            employer_authorization_sha256,
            package.canonical_employer_identity_authorization_base64url,
            risk_authorization_id,
            risk_authorization_sha256,
            package.canonical_job_risk_authorization_base64url,
            attestation.assessed_at_ms,
            attestation.issued_at_ms,
            attestation.not_before_ms,
            attestation.expires_at_ms,
            effective_expires_at_ms,
            recorded_by,
            recorded_at_ms,
        ],
    )
    .map_err(job_integrity_storage)?;
    Ok(())
}

fn existing_job_integrity_attestation_postgres(
    tx: &mut postgres::Transaction<'_>,
    attestation: &JobIntegrityAttestationV1,
    attestation_sha256: &str,
    employer_authorization_sha256: &str,
    employer_authorization_id: &str,
    risk_authorization_sha256: &str,
    risk_authorization_id: &str,
) -> JobIntegrityResult<Option<JobIntegrityExistingAttestation>> {
    tx.query_opt(
        "SELECT attestation.attestation_sha256,
                attestation.canonical_attestation_base64url,
                attestation.canonical_employer_authorization_base64url,
                attestation.canonical_risk_authorization_base64url,
                attestation.attestation_generation,
                COALESCE(transition.head_revision,0),
                COALESCE(transition.transition_sha256,'')
           FROM jobs_job_integrity_attestations attestation
           LEFT JOIN jobs_job_integrity_head_transitions transition
             ON transition.attestation_sha256=attestation.attestation_sha256
          WHERE attestation.attestation_sha256=$1 OR attestation.attestation_id=$2
             OR (attestation.subject_sha256=$3 AND attestation.attestation_generation=$4)
             OR attestation.employer_authorization_sha256=$5
             OR attestation.employer_identity_authorization_id=$6
             OR attestation.risk_authorization_sha256=$7
             OR attestation.job_risk_authorization_id=$8
             OR ($9::text IS NOT NULL AND attestation.predecessor_attestation_sha256=$9)
          LIMIT 1 FOR SHARE OF attestation",
        &[
            &attestation_sha256,
            &attestation.attestation_id,
            &attestation.subject_sha256,
            &attestation.attestation_generation,
            &employer_authorization_sha256,
            &employer_authorization_id,
            &risk_authorization_sha256,
            &risk_authorization_id,
            &attestation.predecessor_attestation_sha256,
        ],
    )
    .map(|row| {
        row.map(|row| JobIntegrityExistingAttestation {
            sha256: row.get(0),
            canonical_attestation_base64url: row.get(1),
            canonical_employer_authorization_base64url: row.get(2),
            canonical_risk_authorization_base64url: row.get(3),
            generation: row.get(4),
            head_revision: row.get(5),
            head_transition_sha256: row.get(6),
        })
    })
    .map_err(job_integrity_storage)
}

fn load_job_integrity_head_postgres(
    tx: &mut postgres::Transaction<'_>,
    subject_sha256: &str,
) -> JobIntegrityResult<Option<JobIntegrityHead>> {
    tx.query_opt(
        "SELECT head_revision,transition_sha256,attestation_sha256,attestation_generation
           FROM jobs_job_integrity_heads WHERE subject_sha256=$1 FOR UPDATE",
        &[&subject_sha256],
    )
    .map(|row| {
        row.map(|row| JobIntegrityHead {
            head_revision: row.get(0),
            transition_sha256: row.get(1),
            attestation_sha256: row.get(2),
            attestation_generation: row.get(3),
        })
    })
    .map_err(job_integrity_storage)
}

fn lock_job_integrity_attestation_authorization_ids_postgres(
    tx: &mut postgres::Transaction<'_>,
    employer_authorization_id: &str,
    risk_authorization_id: &str,
) -> JobIntegrityResult<()> {
    // Cross-subject writes do not contend on a head row. Serialize each
    // role-scoped authorization identity before the collision read and clock
    // sample so a concurrent insert cannot turn the later UNIQUE check into a
    // post-sample blocking point. The fixed role order prevents lock cycles;
    // hash collisions only add safe serialization.
    for (role, authorization_id) in [
        ("employer_identity", employer_authorization_id),
        ("job_risk", risk_authorization_id),
    ] {
        let lock_identity = format!("jobs-job-integrity:{role}:{authorization_id}");
        tx.query_one(
            "SELECT pg_advisory_xact_lock(hashtextextended($1, 0))",
            &[&lock_identity],
        )
        .map_err(job_integrity_storage)?;
    }
    Ok(())
}

fn import_job_integrity_attestation_postgres_tx(
    tx: &mut postgres::Transaction<'_>,
    package: &JobIntegrityAttestationPackageV1,
    recorded_by: &str,
) -> JobIntegrityResult<JobIntegrityImportResult> {
    let parsed = job_integrity_attestation_package_bytes(package)?;
    let ParsedJobIntegrityAttestationPackage {
        attestation_bytes: raw_bytes,
        attestation: raw_attestation,
        employer_authorization_bytes: raw_employer_authorization_bytes,
        employer_authorization: raw_employer_authorization,
        risk_authorization_bytes: raw_risk_authorization_bytes,
        risk_authorization: raw_risk_authorization,
    } = parsed;
    let raw_sha256 = job_integrity_sha256(&raw_bytes);
    let raw_employer_authorization_sha256 = job_integrity_sha256(raw_employer_authorization_bytes);
    let raw_risk_authorization_sha256 = job_integrity_sha256(raw_risk_authorization_bytes);
    // The singleton control row is the integrity-publication fence. Taking it
    // exclusively makes a representation-wide shared lock freeze both
    // existing head advancement and insertion of a previously missing head.
    let control = load_job_integrity_control_postgres(tx, true)?;
    let (policy_sha256, policy) = current_job_integrity_policy_postgres(tx, &control)?;
    let head = load_job_integrity_head_postgres(tx, &raw_attestation.subject_sha256)?;
    lock_job_integrity_attestation_authorization_ids_postgres(
        tx,
        &raw_employer_authorization.authorization_id,
        &raw_risk_authorization.authorization_id,
    )?;
    if let Some(existing) = existing_job_integrity_attestation_postgres(
        tx,
        &raw_attestation,
        &raw_sha256,
        &raw_employer_authorization_sha256,
        &raw_employer_authorization.authorization_id,
        &raw_risk_authorization_sha256,
        &raw_risk_authorization.authorization_id,
    )? {
        if existing.sha256 == raw_sha256
            && existing.canonical_attestation_base64url == package.canonical_attestation_base64url
            && existing.canonical_employer_authorization_base64url
                == package.canonical_employer_identity_authorization_base64url
            && existing.canonical_risk_authorization_base64url
                == package.canonical_job_risk_authorization_base64url
        {
            return Ok(JobIntegrityImportResult {
                object_sha256: raw_sha256,
                generation: existing.generation,
                replayed: true,
                head_revision: (existing.head_revision > 0).then_some(existing.head_revision),
                head_transition_sha256: (!existing.head_transition_sha256.is_empty())
                    .then_some(existing.head_transition_sha256),
            });
        }
        return Err(JobIntegrityAuthorityError::IdentityConflict);
    }
    let now_ms = job_integrity_db_now_postgres(tx)?;
    if now_ms < policy.valid_from_ms || now_ms >= policy.expires_at_ms {
        return Err(JobIntegrityAuthorityError::Expired);
    }
    let verified =
        verify_job_integrity_attestation_package(package, &policy_sha256, &policy, now_ms)?;
    let JobIntegrityVerifiedAttestation {
        attestation,
        sha256: attestation_sha256,
        employer_authorization: employer_auth,
        risk_authorization: risk_auth,
        effective_expires_at_ms: effective_expiry,
    } = verified;
    if effective_expiry <= now_ms {
        return Err(JobIntegrityAuthorityError::Expired);
    }
    if job_integrity_signing_authority_revoked_postgres(
        tx,
        now_ms,
        &policy_sha256,
        &policy,
        "employer_identity",
        &employer_auth.key_ids,
    )? || job_integrity_signing_authority_revoked_postgres(
        tx,
        now_ms,
        &policy_sha256,
        &policy,
        "job_risk",
        &risk_auth.key_ids,
    )? {
        return Err(JobIntegrityAuthorityError::Revoked);
    }
    if attestation.employer.status == "verified" && attestation.risk.status == "clear" {
        let candidates = job_integrity_revocation_candidates_for_attestation(
            &policy,
            &policy_sha256,
            &attestation,
            &attestation_sha256,
            &employer_auth.key_ids,
            &risk_auth.key_ids,
        )?;
        let borrowed = candidates
            .iter()
            .map(|(kind, id, sha256)| (kind.as_str(), id.as_str(), sha256.as_str()))
            .collect::<Vec<_>>();
        if job_integrity_any_revocation_postgres(tx, now_ms, &borrowed)? {
            return Err(JobIntegrityAuthorityError::Revoked);
        }
    }
    if attestation.subject_sha256 != raw_attestation.subject_sha256 {
        return Err(JobIntegrityAuthorityError::IdentityConflict);
    }
    let expected_generation = head
        .as_ref()
        .map_or(1, |value| value.attestation_generation + 1);
    if attestation.attestation_generation < expected_generation {
        return Err(JobIntegrityAuthorityError::SequenceRegression);
    }
    if attestation.attestation_generation != expected_generation
        || attestation.predecessor_attestation_sha256
            != head.as_ref().map(|value| value.attestation_sha256.clone())
    {
        return Err(JobIntegrityAuthorityError::CompareAndSwapConflict);
    }
    insert_job_integrity_attestation_postgres(
        tx,
        package,
        &attestation,
        &attestation_sha256,
        &employer_auth.authorization_id,
        &employer_auth.sha256,
        &risk_auth.authorization_id,
        &risk_auth.sha256,
        effective_expiry,
        recorded_by,
        now_ms,
    )?;
    let head_revision = head.as_ref().map_or(1, |value| value.head_revision + 1);
    let previous_head_revision = head.as_ref().map_or(0, |value| value.head_revision);
    let transition = JobIntegrityHeadTransitionCanonical {
        version: 1,
        audience: JOB_INTEGRITY_TRANSITION_AUDIENCE,
        subject_sha256: &attestation.subject_sha256,
        head_revision,
        previous_head_revision,
        predecessor_transition_sha256: head.as_ref().map(|value| value.transition_sha256.as_str()),
        previous_attestation_sha256: head.as_ref().map(|value| value.attestation_sha256.as_str()),
        attestation_sha256: &attestation_sha256,
        attestation_generation: attestation.attestation_generation,
        policy_sha256: &policy_sha256,
        transition_actor: recorded_by,
        transitioned_at_ms: now_ms,
    };
    let transition_sha256 = job_integrity_sha256(job_integrity_canonical_json(&transition)?);
    tx.execute(
        "INSERT INTO jobs_job_integrity_head_transitions(
           transition_sha256,subject_sha256,head_revision,previous_head_revision,
           predecessor_transition_sha256,previous_attestation_sha256,
           attestation_sha256,attestation_generation,policy_sha256,
           transition_actor,transitioned_at_ms)
         VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)",
        &[
            &transition_sha256,
            &attestation.subject_sha256,
            &head_revision,
            &previous_head_revision,
            &head.as_ref().map(|value| value.transition_sha256.as_str()),
            &head.as_ref().map(|value| value.attestation_sha256.as_str()),
            &attestation_sha256,
            &attestation.attestation_generation,
            &policy_sha256,
            &recorded_by,
            &now_ms,
        ],
    )
    .map_err(job_integrity_storage)?;
    let changed = if let Some(head) = head {
        tx.execute(
            "UPDATE jobs_job_integrity_heads
                SET head_revision=$1,transition_sha256=$2,attestation_sha256=$3,
                    attestation_generation=$4,policy_sha256=$5,updated_at_ms=$6
              WHERE subject_sha256=$7 AND head_revision=$8 AND transition_sha256=$9
                AND attestation_sha256=$10",
            &[
                &head_revision,
                &transition_sha256,
                &attestation_sha256,
                &attestation.attestation_generation,
                &policy_sha256,
                &now_ms,
                &attestation.subject_sha256,
                &head.head_revision,
                &head.transition_sha256,
                &head.attestation_sha256,
            ],
        )
        .map_err(job_integrity_storage)?
    } else {
        tx.execute(
            "INSERT INTO jobs_job_integrity_heads(
               subject_sha256,head_revision,transition_sha256,attestation_sha256,
               attestation_generation,policy_sha256,updated_at_ms)
             VALUES($1,$2,$3,$4,$5,$6,$7)",
            &[
                &attestation.subject_sha256,
                &head_revision,
                &transition_sha256,
                &attestation_sha256,
                &attestation.attestation_generation,
                &policy_sha256,
                &now_ms,
            ],
        )
        .map_err(job_integrity_storage)?
    };
    if changed != 1 {
        return Err(JobIntegrityAuthorityError::CompareAndSwapConflict);
    }
    Ok(JobIntegrityImportResult {
        object_sha256: attestation_sha256,
        generation: attestation.attestation_generation,
        replayed: false,
        head_revision: Some(head_revision),
        head_transition_sha256: Some(transition_sha256),
    })
}

#[allow(clippy::too_many_arguments)]
fn insert_job_integrity_attestation_postgres(
    tx: &mut postgres::Transaction<'_>,
    package: &JobIntegrityAttestationPackageV1,
    attestation: &JobIntegrityAttestationV1,
    attestation_sha256: &str,
    employer_authorization_id: &str,
    employer_authorization_sha256: &str,
    risk_authorization_id: &str,
    risk_authorization_sha256: &str,
    effective_expires_at_ms: i64,
    recorded_by: &str,
    recorded_at_ms: i64,
) -> JobIntegrityResult<()> {
    let verification_methods = serde_json::to_string(&attestation.employer.verification_methods)
        .map_err(job_integrity_storage)?;
    let identity_evidence =
        serde_json::to_string(&attestation.employer.evidence).map_err(job_integrity_storage)?;
    let signal_codes =
        serde_json::to_string(&attestation.risk.signal_codes).map_err(job_integrity_storage)?;
    let risk_evidence =
        serde_json::to_string(&attestation.risk.evidence).map_err(job_integrity_storage)?;
    tx.execute(
        "INSERT INTO jobs_job_integrity_attestations(
           attestation_sha256,attestation_id,policy_sha256,subject_sha256,
           source_material_sha256,attestation_generation,predecessor_attestation_sha256,
           canonical_job_id,provider_family,provider_record_id,provider_host,
           provider_tenant,provider_job,provider_variant,canonical_application_url,
           application_domain,ats_tenant_binding_sha256,employer_status,
           canonical_employer_id,canonical_employer_domain,verification_methods_json,
           identity_evidence_json,risk_status,risk_signal_codes_json,risk_policy_sha256,
           risk_input_sha256,risk_engine_release_sha256,risk_evidence_json,
           canonical_attestation_base64url,employer_identity_authorization_id,
           employer_authorization_sha256,canonical_employer_authorization_base64url,
           job_risk_authorization_id,risk_authorization_sha256,
           canonical_risk_authorization_base64url,assessed_at_ms,issued_at_ms,
           not_before_ms,expires_at_ms,effective_expires_at_ms,recorded_by,recorded_at_ms)
         VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,
                $18,$19,$20,$21,$22,$23,$24,$25,$26,$27,$28,$29,$30,$31,$32,
                $33,$34,$35,$36,$37,$38,$39,$40,$41,$42)",
        &[
            &attestation_sha256,
            &attestation.attestation_id,
            &attestation.policy_sha256,
            &attestation.subject_sha256,
            &attestation.source_material_sha256,
            &attestation.attestation_generation,
            &attestation.predecessor_attestation_sha256,
            &attestation.canonical_job_id,
            &attestation.source.provider_family,
            &attestation.source.provider_record_id,
            &attestation.source.target.host,
            &attestation.source.target.tenant,
            &attestation.source.target.job,
            &attestation.source.target.variant,
            &attestation.source.canonical_application_url,
            &attestation.source.application_domain,
            &attestation.source.ats_tenant_binding_sha256,
            &attestation.employer.status,
            &attestation.employer.canonical_employer_id,
            &attestation.employer.canonical_employer_domain,
            &verification_methods,
            &identity_evidence,
            &attestation.risk.status,
            &signal_codes,
            &attestation.risk.policy_sha256,
            &attestation.risk.input_sha256,
            &attestation.risk.engine_release_sha256,
            &risk_evidence,
            &package.canonical_attestation_base64url,
            &employer_authorization_id,
            &employer_authorization_sha256,
            &package.canonical_employer_identity_authorization_base64url,
            &risk_authorization_id,
            &risk_authorization_sha256,
            &package.canonical_job_risk_authorization_base64url,
            &attestation.assessed_at_ms,
            &attestation.issued_at_ms,
            &attestation.not_before_ms,
            &attestation.expires_at_ms,
            &effective_expires_at_ms,
            &recorded_by,
            &recorded_at_ms,
        ],
    )
    .map_err(job_integrity_storage)?;
    Ok(())
}

fn job_integrity_revocation_package_bytes(
    package: &JobIntegrityRevocationPackageV1,
) -> JobIntegrityResult<ParsedJobIntegrityRevocationPackage> {
    let bytes = job_integrity_decode_base64url_bounded(&package.canonical_revocation_base64url)?;
    let revocation = job_integrity_parse_canonical_json(&bytes)?;
    let (authorization_bytes, authorization) =
        job_integrity_authorization_from_base64(&package.canonical_authorization_base64url)?;
    Ok(ParsedJobIntegrityRevocationPackage {
        revocation_bytes: bytes,
        revocation,
        authorization_bytes,
        authorization,
    })
}

pub fn import_job_integrity_revocation(
    pool: &DbPool,
    package: &JobIntegrityRevocationPackageV1,
    recorded_by: &str,
) -> JobIntegrityResult<JobIntegrityImportResult> {
    if !job_integrity_text(recorded_by, 1, 240) {
        return Err(JobIntegrityAuthorityError::InvalidAuthority);
    }
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get().map_err(job_integrity_storage)?;
            let tx = conn
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(job_integrity_storage)?;
            let result = import_job_integrity_revocation_sqlite_tx(&tx, package, recorded_by)?;
            tx.commit().map_err(job_integrity_storage)?;
            Ok(result)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg().map_err(job_integrity_storage)?;
            let mut tx = conn.transaction().map_err(job_integrity_storage)?;
            let result =
                import_job_integrity_revocation_postgres_tx(&mut tx, package, recorded_by)?;
            tx.commit().map_err(job_integrity_storage)?;
            Ok(result)
        }
    })
}

fn import_job_integrity_revocation_sqlite_tx(
    tx: &rusqlite::Transaction<'_>,
    package: &JobIntegrityRevocationPackageV1,
    recorded_by: &str,
) -> JobIntegrityResult<JobIntegrityImportResult> {
    let parsed = job_integrity_revocation_package_bytes(package)?;
    let ParsedJobIntegrityRevocationPackage {
        revocation_bytes: bytes,
        revocation,
        authorization_bytes,
        authorization: raw_authorization,
    } = parsed;
    let revocation_sha256 = job_integrity_sha256(bytes);
    let authorization_sha256 = job_integrity_sha256(authorization_bytes);
    let control = load_job_integrity_control_sqlite(tx)?;
    let (policy_sha256, policy) = current_job_integrity_policy_sqlite(tx, &control)?;
    if let Some(existing) = tx
        .query_row(
            "SELECT revocation_sha256,canonical_revocation_base64url,
                    canonical_authorization_base64url,revocation_generation
               FROM jobs_job_integrity_revocations
              WHERE revocation_sha256=?1 OR revocation_id=?2
                 OR revocation_generation=?3 OR authorization_sha256=?4
                 OR authorization_id=?5
                 OR (?6 IS NOT NULL AND predecessor_revocation_sha256=?6) LIMIT 1",
            params![
                revocation_sha256,
                revocation.revocation_id,
                revocation.revocation_generation,
                authorization_sha256,
                raw_authorization.authorization_id,
                revocation.predecessor_revocation_sha256,
            ],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            },
        )
        .optional()
        .map_err(job_integrity_storage)?
    {
        if existing.0 == revocation_sha256
            && existing.1 == package.canonical_revocation_base64url
            && existing.2 == package.canonical_authorization_base64url
        {
            return Ok(JobIntegrityImportResult {
                object_sha256: revocation_sha256,
                generation: existing.3,
                replayed: true,
                head_revision: None,
                head_transition_sha256: None,
            });
        }
        return Err(JobIntegrityAuthorityError::IdentityConflict);
    }
    let now_ms = job_integrity_db_now_sqlite(tx)?;
    if now_ms < policy.valid_from_ms || now_ms >= policy.expires_at_ms {
        return Err(JobIntegrityAuthorityError::Expired);
    }
    validate_job_integrity_revocation(
        &revocation,
        &policy_sha256,
        now_ms,
        policy.requirements.maximum_clock_skew_ms,
    )?;
    if revocation.revocation_generation <= control.revocation_generation {
        return Err(JobIntegrityAuthorityError::SequenceRegression);
    }
    if revocation.revocation_generation != control.revocation_generation + 1
        || revocation.predecessor_revocation_sha256 != control.revocation_sha256
    {
        return Err(JobIntegrityAuthorityError::CompareAndSwapConflict);
    }
    let authorization =
        verify_job_integrity_authorization(JobIntegrityAuthorizationVerification {
            encoded: &package.canonical_authorization_base64url,
            authorization_audience: JOB_INTEGRITY_AUTHORIZATION_AUDIENCE,
            role_name: "revocation",
            role: &policy.delegated_roles["revocation"],
            policy_sha256: &policy_sha256,
            target_audience: JOB_INTEGRITY_REVOCATION_AUDIENCE,
            target_sha256: &revocation_sha256,
            minimum_signed_at_ms: revocation.issued_at_ms,
            verification_time_ms: now_ms,
            maximum_clock_skew_ms: policy.requirements.maximum_clock_skew_ms,
        })?;
    if authorization.authorization_id != raw_authorization.authorization_id {
        return Err(JobIntegrityAuthorityError::IdentityConflict);
    }
    if authorization.effective_key_expires_at_ms <= now_ms
        || job_integrity_signing_authority_revoked_sqlite(
            tx,
            now_ms,
            &policy_sha256,
            &policy,
            "revocation",
            &authorization.key_ids,
        )?
    {
        return Err(JobIntegrityAuthorityError::Revoked);
    }
    tx.execute(
        "INSERT INTO jobs_job_integrity_revocations(
           revocation_sha256,revocation_id,revocation_generation,
           predecessor_revocation_sha256,policy_sha256,subject_kind,subject_id,
           subject_sha256,reason_code,reason_ref,effective_at_ms,issued_at_ms,
           canonical_revocation_base64url,authorization_id,authorization_sha256,
           canonical_authorization_base64url,recorded_by,recorded_at_ms)
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18)",
        params![
            revocation_sha256,
            revocation.revocation_id,
            revocation.revocation_generation,
            revocation.predecessor_revocation_sha256,
            policy_sha256,
            revocation.subject_kind,
            revocation.subject_id,
            revocation.subject_sha256,
            revocation.reason_code,
            revocation.reason_ref,
            revocation.effective_at_ms,
            revocation.issued_at_ms,
            package.canonical_revocation_base64url,
            authorization.authorization_id,
            authorization.sha256,
            package.canonical_authorization_base64url,
            recorded_by,
            now_ms,
        ],
    )
    .map_err(job_integrity_storage)?;
    let changed = tx
        .execute(
            "UPDATE jobs_job_integrity_control
                SET control_revision=?1,current_revocation_sha256=?2,
                    current_revocation_generation=?3,updated_by=?4,updated_at_ms=?5
              WHERE singleton_id=1 AND control_revision=?6
                AND current_revocation_generation=?7
                AND current_revocation_sha256 IS ?8
                AND current_policy_sha256=?9",
            params![
                control.control_revision + 1,
                revocation_sha256,
                revocation.revocation_generation,
                recorded_by,
                now_ms,
                control.control_revision,
                control.revocation_generation,
                control.revocation_sha256,
                policy_sha256,
            ],
        )
        .map_err(job_integrity_storage)?;
    if changed != 1 {
        return Err(JobIntegrityAuthorityError::CompareAndSwapConflict);
    }
    Ok(JobIntegrityImportResult {
        object_sha256: revocation_sha256,
        generation: revocation.revocation_generation,
        replayed: false,
        head_revision: None,
        head_transition_sha256: None,
    })
}

fn import_job_integrity_revocation_postgres_tx(
    tx: &mut postgres::Transaction<'_>,
    package: &JobIntegrityRevocationPackageV1,
    recorded_by: &str,
) -> JobIntegrityResult<JobIntegrityImportResult> {
    let parsed = job_integrity_revocation_package_bytes(package)?;
    let ParsedJobIntegrityRevocationPackage {
        revocation_bytes: bytes,
        revocation,
        authorization_bytes,
        authorization: raw_authorization,
    } = parsed;
    let revocation_sha256 = job_integrity_sha256(bytes);
    let authorization_sha256 = job_integrity_sha256(authorization_bytes);
    let control = load_job_integrity_control_postgres(tx, true)?;
    let (policy_sha256, policy) = current_job_integrity_policy_postgres(tx, &control)?;
    if let Some(row) = tx
        .query_opt(
            "SELECT revocation_sha256,canonical_revocation_base64url,
                    canonical_authorization_base64url,revocation_generation
               FROM jobs_job_integrity_revocations
              WHERE revocation_sha256=$1 OR revocation_id=$2
                 OR revocation_generation=$3 OR authorization_sha256=$4
                 OR authorization_id=$5
                 OR ($6::text IS NOT NULL AND predecessor_revocation_sha256=$6)
              LIMIT 1 FOR SHARE",
            &[
                &revocation_sha256,
                &revocation.revocation_id,
                &revocation.revocation_generation,
                &authorization_sha256,
                &raw_authorization.authorization_id,
                &revocation.predecessor_revocation_sha256,
            ],
        )
        .map_err(job_integrity_storage)?
    {
        if row.get::<_, String>(0) == revocation_sha256
            && row.get::<_, String>(1) == package.canonical_revocation_base64url
            && row.get::<_, String>(2) == package.canonical_authorization_base64url
        {
            return Ok(JobIntegrityImportResult {
                object_sha256: revocation_sha256,
                generation: row.get(3),
                replayed: true,
                head_revision: None,
                head_transition_sha256: None,
            });
        }
        return Err(JobIntegrityAuthorityError::IdentityConflict);
    }
    let now_ms = job_integrity_db_now_postgres(tx)?;
    if now_ms < policy.valid_from_ms || now_ms >= policy.expires_at_ms {
        return Err(JobIntegrityAuthorityError::Expired);
    }
    validate_job_integrity_revocation(
        &revocation,
        &policy_sha256,
        now_ms,
        policy.requirements.maximum_clock_skew_ms,
    )?;
    if revocation.revocation_generation <= control.revocation_generation {
        return Err(JobIntegrityAuthorityError::SequenceRegression);
    }
    if revocation.revocation_generation != control.revocation_generation + 1
        || revocation.predecessor_revocation_sha256 != control.revocation_sha256
    {
        return Err(JobIntegrityAuthorityError::CompareAndSwapConflict);
    }
    let authorization =
        verify_job_integrity_authorization(JobIntegrityAuthorizationVerification {
            encoded: &package.canonical_authorization_base64url,
            authorization_audience: JOB_INTEGRITY_AUTHORIZATION_AUDIENCE,
            role_name: "revocation",
            role: &policy.delegated_roles["revocation"],
            policy_sha256: &policy_sha256,
            target_audience: JOB_INTEGRITY_REVOCATION_AUDIENCE,
            target_sha256: &revocation_sha256,
            minimum_signed_at_ms: revocation.issued_at_ms,
            verification_time_ms: now_ms,
            maximum_clock_skew_ms: policy.requirements.maximum_clock_skew_ms,
        })?;
    if authorization.authorization_id != raw_authorization.authorization_id {
        return Err(JobIntegrityAuthorityError::IdentityConflict);
    }
    if authorization.effective_key_expires_at_ms <= now_ms
        || job_integrity_signing_authority_revoked_postgres(
            tx,
            now_ms,
            &policy_sha256,
            &policy,
            "revocation",
            &authorization.key_ids,
        )?
    {
        return Err(JobIntegrityAuthorityError::Revoked);
    }
    tx.execute(
        "INSERT INTO jobs_job_integrity_revocations(
           revocation_sha256,revocation_id,revocation_generation,
           predecessor_revocation_sha256,policy_sha256,subject_kind,subject_id,
           subject_sha256,reason_code,reason_ref,effective_at_ms,issued_at_ms,
           canonical_revocation_base64url,authorization_id,authorization_sha256,
           canonical_authorization_base64url,recorded_by,recorded_at_ms)
         VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18)",
        &[
            &revocation_sha256,
            &revocation.revocation_id,
            &revocation.revocation_generation,
            &revocation.predecessor_revocation_sha256,
            &policy_sha256,
            &revocation.subject_kind,
            &revocation.subject_id,
            &revocation.subject_sha256,
            &revocation.reason_code,
            &revocation.reason_ref,
            &revocation.effective_at_ms,
            &revocation.issued_at_ms,
            &package.canonical_revocation_base64url,
            &authorization.authorization_id,
            &authorization.sha256,
            &package.canonical_authorization_base64url,
            &recorded_by,
            &now_ms,
        ],
    )
    .map_err(job_integrity_storage)?;
    let changed = tx
        .execute(
            "UPDATE jobs_job_integrity_control
                SET control_revision=$1,current_revocation_sha256=$2,
                    current_revocation_generation=$3,updated_by=$4,updated_at_ms=$5
              WHERE singleton_id=1 AND control_revision=$6
                AND current_revocation_generation=$7
                AND current_revocation_sha256 IS NOT DISTINCT FROM $8
                AND current_policy_sha256=$9",
            &[
                &(control.control_revision + 1),
                &revocation_sha256,
                &revocation.revocation_generation,
                &recorded_by,
                &now_ms,
                &control.control_revision,
                &control.revocation_generation,
                &control.revocation_sha256,
                &policy_sha256,
            ],
        )
        .map_err(job_integrity_storage)?;
    if changed != 1 {
        return Err(JobIntegrityAuthorityError::CompareAndSwapConflict);
    }
    Ok(JobIntegrityImportResult {
        object_sha256: revocation_sha256,
        generation: revocation.revocation_generation,
        replayed: false,
        head_revision: None,
        head_transition_sha256: None,
    })
}

#[derive(Debug, Clone)]
struct StoredJobIntegrityAuthority {
    head_revision: i64,
    transition_sha256: String,
    attestation_sha256: String,
    attestation_generation: i64,
    policy_sha256: String,
    canonical_attestation_base64url: String,
    employer_authorization_id: String,
    employer_authorization_sha256: String,
    employer_authorization_base64url: String,
    risk_authorization_id: String,
    risk_authorization_sha256: String,
    risk_authorization_base64url: String,
    effective_expires_at_ms: i64,
    previous_head_revision: i64,
    predecessor_transition_sha256: Option<String>,
    previous_attestation_sha256: Option<String>,
    transition_actor: String,
    transitioned_at_ms: i64,
}

fn job_integrity_expected_source_valid(expected: &JobIntegrityExpectedSource) -> bool {
    job_integrity_hex64(&expected.subject_sha256)
        && job_integrity_hex64(&expected.source_material_sha256)
        && job_integrity_safe_integer(expected.source_expires_at_ms, true)
        && job_integrity_text(&expected.canonical_job_id, 1, 240)
        && JOB_INTEGRITY_PROVIDERS.contains(&expected.provider_family.as_str())
        && job_integrity_text(&expected.provider_record_id, 1, 512)
        && job_integrity_domain(&expected.provider_host)
        && job_integrity_token(&expected.provider_tenant, 1, 240)
        && job_integrity_token(&expected.provider_job, 1, 512)
        && job_integrity_token(&expected.provider_variant, 1, 120)
        && job_integrity_text(&expected.canonical_application_url, 8, 4096)
        && job_integrity_domain(&expected.application_domain)
        && job_integrity_hex64(&expected.ats_tenant_binding_sha256)
        && job_integrity_canonical_https_url(
            &expected.canonical_application_url,
            &expected.application_domain,
        )
}

fn load_stored_job_integrity_authority_sqlite(
    tx: &rusqlite::Transaction<'_>,
    subject_sha256: &str,
) -> JobIntegrityResult<Option<StoredJobIntegrityAuthority>> {
    let Some((
        head_revision,
        transition_sha256,
        attestation_sha256,
        attestation_generation,
        policy_sha256,
    )) = tx
        .query_row(
            "SELECT head_revision,transition_sha256,attestation_sha256,
                    attestation_generation,policy_sha256
               FROM jobs_job_integrity_heads WHERE subject_sha256=?1",
            params![subject_sha256],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, String>(4)?,
                ))
            },
        )
        .optional()
        .map_err(job_integrity_storage)?
    else {
        return Ok(None);
    };
    let row = tx
        .query_row(
            "SELECT
                attestation.canonical_attestation_base64url,
                attestation.employer_identity_authorization_id,
                attestation.employer_authorization_sha256,
                attestation.canonical_employer_authorization_base64url,
                attestation.job_risk_authorization_id,
                attestation.risk_authorization_sha256,
                attestation.canonical_risk_authorization_base64url,
                attestation.effective_expires_at_ms,
                transition.previous_head_revision,
                transition.predecessor_transition_sha256,
                transition.previous_attestation_sha256,
                transition.transition_actor,transition.transitioned_at_ms,
                transition.subject_sha256,transition.head_revision,
                transition.attestation_sha256,transition.attestation_generation,
                transition.policy_sha256
           FROM jobs_job_integrity_attestations attestation
           JOIN jobs_job_integrity_head_transitions transition
             ON transition.transition_sha256=?2
          WHERE attestation.attestation_sha256=?1",
            params![attestation_sha256, transition_sha256],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, i64>(7)?,
                    row.get::<_, i64>(8)?,
                    row.get::<_, Option<String>>(9)?,
                    row.get::<_, Option<String>>(10)?,
                    row.get::<_, String>(11)?,
                    row.get::<_, i64>(12)?,
                    row.get::<_, String>(13)?,
                    row.get::<_, i64>(14)?,
                    row.get::<_, String>(15)?,
                    row.get::<_, i64>(16)?,
                    row.get::<_, String>(17)?,
                ))
            },
        )
        .optional()
        .map_err(job_integrity_storage)?
        .ok_or(JobIntegrityAuthorityError::IdentityConflict)?;
    if row.13 != subject_sha256
        || row.14 != head_revision
        || row.15 != attestation_sha256
        || row.16 != attestation_generation
        || row.17 != policy_sha256
    {
        return Err(JobIntegrityAuthorityError::IdentityConflict);
    }
    Ok(Some(StoredJobIntegrityAuthority {
        head_revision,
        transition_sha256,
        attestation_sha256,
        attestation_generation,
        policy_sha256,
        canonical_attestation_base64url: row.0,
        employer_authorization_id: row.1,
        employer_authorization_sha256: row.2,
        employer_authorization_base64url: row.3,
        risk_authorization_id: row.4,
        risk_authorization_sha256: row.5,
        risk_authorization_base64url: row.6,
        effective_expires_at_ms: row.7,
        previous_head_revision: row.8,
        predecessor_transition_sha256: row.9,
        previous_attestation_sha256: row.10,
        transition_actor: row.11,
        transitioned_at_ms: row.12,
    }))
}

fn load_stored_job_integrity_authority_postgres(
    tx: &mut postgres::Transaction<'_>,
    subject_sha256: &str,
) -> JobIntegrityResult<Option<StoredJobIntegrityAuthority>> {
    let Some(head) = tx
        .query_opt(
            "SELECT head_revision,transition_sha256,attestation_sha256,
                attestation_generation,policy_sha256
           FROM jobs_job_integrity_heads WHERE subject_sha256=$1 FOR SHARE",
            &[&subject_sha256],
        )
        .map_err(job_integrity_storage)?
    else {
        return Ok(None);
    };
    let head_revision: i64 = head.get(0);
    let transition_sha256: String = head.get(1);
    let attestation_sha256: String = head.get(2);
    let attestation_generation: i64 = head.get(3);
    let policy_sha256: String = head.get(4);
    let row = tx
        .query_opt(
            "SELECT
                attestation.canonical_attestation_base64url,
                attestation.employer_identity_authorization_id,
                attestation.employer_authorization_sha256,
                attestation.canonical_employer_authorization_base64url,
                attestation.job_risk_authorization_id,
                attestation.risk_authorization_sha256,
                attestation.canonical_risk_authorization_base64url,
                attestation.effective_expires_at_ms,
                transition.previous_head_revision,
                transition.predecessor_transition_sha256,
                transition.previous_attestation_sha256,
                transition.transition_actor,transition.transitioned_at_ms,
                transition.subject_sha256,transition.head_revision,
                transition.attestation_sha256,transition.attestation_generation,
                transition.policy_sha256
           FROM jobs_job_integrity_attestations attestation
           JOIN jobs_job_integrity_head_transitions transition
             ON transition.transition_sha256=$2
          WHERE attestation.attestation_sha256=$1
          FOR SHARE OF attestation,transition",
            &[&attestation_sha256, &transition_sha256],
        )
        .map_err(job_integrity_storage)?
        .ok_or(JobIntegrityAuthorityError::IdentityConflict)?;
    if row.get::<_, String>(13) != subject_sha256
        || row.get::<_, i64>(14) != head_revision
        || row.get::<_, String>(15) != attestation_sha256
        || row.get::<_, i64>(16) != attestation_generation
        || row.get::<_, String>(17) != policy_sha256
    {
        return Err(JobIntegrityAuthorityError::IdentityConflict);
    }
    Ok(Some(StoredJobIntegrityAuthority {
        head_revision,
        transition_sha256,
        attestation_sha256,
        attestation_generation,
        policy_sha256,
        canonical_attestation_base64url: row.get(0),
        employer_authorization_id: row.get(1),
        employer_authorization_sha256: row.get(2),
        employer_authorization_base64url: row.get(3),
        risk_authorization_id: row.get(4),
        risk_authorization_sha256: row.get(5),
        risk_authorization_base64url: row.get(6),
        effective_expires_at_ms: row.get(7),
        previous_head_revision: row.get(8),
        predecessor_transition_sha256: row.get(9),
        previous_attestation_sha256: row.get(10),
        transition_actor: row.get(11),
        transitioned_at_ms: row.get(12),
    }))
}

fn job_integrity_source_matches(
    attestation: &JobIntegrityAttestationV1,
    expected: &JobIntegrityExpectedSource,
) -> bool {
    attestation.subject_sha256 == expected.subject_sha256
        && attestation.source_material_sha256 == expected.source_material_sha256
        && attestation.canonical_job_id == expected.canonical_job_id
        && attestation.source.provider_family == expected.provider_family
        && attestation.source.provider_record_id == expected.provider_record_id
        && attestation.source.target.host == expected.provider_host
        && attestation.source.target.tenant == expected.provider_tenant
        && attestation.source.target.job == expected.provider_job
        && attestation.source.target.variant == expected.provider_variant
        && attestation.source.canonical_application_url == expected.canonical_application_url
        && attestation.source.application_domain == expected.application_domain
        && attestation.source.ats_tenant_binding_sha256 == expected.ats_tenant_binding_sha256
}

fn job_integrity_resolution(
    status: JobIntegrityResolutionStatus,
    reason_code: &str,
    signal_codes: Vec<String>,
    authority: Option<JobIntegrityCurrentAuthority>,
) -> JobIntegrityResolution {
    JobIntegrityResolution {
        status,
        reason_code: reason_code.to_string(),
        signal_codes,
        authority,
    }
}

fn job_integrity_nonpositive_resolution(
    attestation: &JobIntegrityAttestationV1,
    reason_override: Option<&str>,
) -> JobIntegrityResolution {
    let (status, default_reason) = if attestation.employer.status == "mismatch" {
        (
            JobIntegrityResolutionStatus::Mismatch,
            "employer_identity_mismatch",
        )
    } else if attestation.risk.status == "blocked" {
        (JobIntegrityResolutionStatus::Blocked, "job_risk_blocked")
    } else {
        (
            JobIntegrityResolutionStatus::ReviewRequired,
            "job_integrity_review_required",
        )
    };
    job_integrity_resolution(
        status,
        reason_override.unwrap_or(default_reason),
        attestation.risk.signal_codes.clone(),
        None,
    )
}

fn job_integrity_revocation_candidates_for_attestation<'a>(
    policy: &'a JobIntegrityTrustPolicyV1,
    policy_sha256: &'a str,
    attestation: &'a JobIntegrityAttestationV1,
    attestation_sha256: &'a str,
    employer_signers: &'a [String],
    risk_signers: &'a [String],
) -> JobIntegrityResult<Vec<(String, String, String)>> {
    let mut candidates = vec![
        (
            "trust_policy".to_string(),
            policy.policy_id.clone(),
            policy_sha256.to_string(),
        ),
        (
            "attestation".to_string(),
            attestation.attestation_id.clone(),
            attestation_sha256.to_string(),
        ),
        (
            "subject".to_string(),
            attestation.canonical_job_id.clone(),
            attestation.subject_sha256.clone(),
        ),
        (
            "canonical_employer".to_string(),
            attestation.employer.canonical_employer_id.clone(),
            job_integrity_sha256(job_integrity_canonical_json(&json!({
                "canonicalEmployerDomain": attestation.employer.canonical_employer_domain,
                "canonicalEmployerId": attestation.employer.canonical_employer_id,
            }))?),
        ),
        (
            "risk_policy".to_string(),
            attestation.risk.policy_sha256.clone(),
            attestation.risk.policy_sha256.clone(),
        ),
        (
            "risk_engine_release".to_string(),
            attestation.risk.engine_release_sha256.clone(),
            attestation.risk.engine_release_sha256.clone(),
        ),
    ];
    for evidence in &attestation.employer.evidence {
        candidates.push((
            "identity_evidence".to_string(),
            evidence.kind.clone(),
            evidence.sha256.clone(),
        ));
    }
    for evidence in &attestation.risk.evidence {
        candidates.push((
            "risk_evidence".to_string(),
            evidence.kind.clone(),
            evidence.sha256.clone(),
        ));
    }
    for (role_name, signers) in [
        ("employer_identity", employer_signers),
        ("job_risk", risk_signers),
    ] {
        let role = &policy.delegated_roles[role_name];
        for key_id in signers {
            let key = role
                .keys
                .get(key_id)
                .ok_or(JobIntegrityAuthorityError::InvalidSignature)?;
            candidates.push((
                "trust_key".to_string(),
                key_id.clone(),
                job_integrity_sha256(job_integrity_decode_exact(&key.public_key_base64url, 32)?),
            ));
        }
    }
    Ok(candidates)
}

fn evaluate_job_integrity_authority<F>(
    stored: StoredJobIntegrityAuthority,
    current_policy_sha256: &str,
    attestation_policy: &JobIntegrityTrustPolicyV1,
    expected: &JobIntegrityExpectedSource,
    now_ms: i64,
    mut revoked: F,
) -> JobIntegrityResult<JobIntegrityResolution>
where
    F: FnMut(&[(&str, &str, &str)]) -> JobIntegrityResult<bool>,
{
    let bytes = job_integrity_decode_base64url_bounded(&stored.canonical_attestation_base64url)?;
    if job_integrity_sha256(&bytes) != stored.attestation_sha256 {
        return Err(JobIntegrityAuthorityError::IdentityConflict);
    }
    let attestation: JobIntegrityAttestationV1 = job_integrity_parse_canonical_json(&bytes)?;
    if attestation.attestation_generation != stored.attestation_generation
        || attestation.policy_sha256 != stored.policy_sha256
        || attestation.subject_sha256 != expected.subject_sha256
        || stored.head_revision != stored.previous_head_revision + 1
        || stored.head_revision != stored.attestation_generation
        || (stored.head_revision == 1
            && (stored.predecessor_transition_sha256.is_some()
                || stored.previous_attestation_sha256.is_some()))
        || (stored.head_revision > 1
            && (stored.predecessor_transition_sha256.is_none()
                || stored.previous_attestation_sha256.as_deref()
                    != attestation.predecessor_attestation_sha256.as_deref()))
    {
        return Err(JobIntegrityAuthorityError::IdentityConflict);
    }
    let transition = JobIntegrityHeadTransitionCanonical {
        version: 1,
        audience: JOB_INTEGRITY_TRANSITION_AUDIENCE,
        subject_sha256: &attestation.subject_sha256,
        head_revision: stored.head_revision,
        previous_head_revision: stored.previous_head_revision,
        predecessor_transition_sha256: stored.predecessor_transition_sha256.as_deref(),
        previous_attestation_sha256: stored.previous_attestation_sha256.as_deref(),
        attestation_sha256: &stored.attestation_sha256,
        attestation_generation: stored.attestation_generation,
        policy_sha256: &stored.policy_sha256,
        transition_actor: &stored.transition_actor,
        transitioned_at_ms: stored.transitioned_at_ms,
    };
    if job_integrity_sha256(job_integrity_canonical_json(&transition)?) != stored.transition_sha256
    {
        return Err(JobIntegrityAuthorityError::IdentityConflict);
    }
    if !job_integrity_source_matches(&attestation, expected) {
        return Ok(job_integrity_resolution(
            JobIntegrityResolutionStatus::Mismatch,
            "original_source_binding_mismatch",
            Vec::new(),
            None,
        ));
    }
    let positive = attestation.employer.status == "verified" && attestation.risk.status == "clear";
    let canonical_validation_time = attestation.not_before_ms;
    let recomputed_base_expiry = validate_job_integrity_attestation(
        &attestation,
        attestation_policy,
        bytes.len(),
        canonical_validation_time,
    )?;
    let attestation_sha256 = stored.attestation_sha256.clone();
    let employer = verify_job_integrity_authorization(JobIntegrityAuthorizationVerification {
        encoded: &stored.employer_authorization_base64url,
        authorization_audience: JOB_INTEGRITY_AUTHORIZATION_AUDIENCE,
        role_name: "employer_identity",
        role: &attestation_policy.delegated_roles["employer_identity"],
        policy_sha256: &stored.policy_sha256,
        target_audience: JOB_INTEGRITY_ATTESTATION_AUDIENCE,
        target_sha256: &attestation_sha256,
        minimum_signed_at_ms: attestation.issued_at_ms,
        verification_time_ms: now_ms,
        maximum_clock_skew_ms: attestation_policy.requirements.maximum_clock_skew_ms,
    })?;
    let risk = verify_job_integrity_authorization(JobIntegrityAuthorizationVerification {
        encoded: &stored.risk_authorization_base64url,
        authorization_audience: JOB_INTEGRITY_AUTHORIZATION_AUDIENCE,
        role_name: "job_risk",
        role: &attestation_policy.delegated_roles["job_risk"],
        policy_sha256: &stored.policy_sha256,
        target_audience: JOB_INTEGRITY_ATTESTATION_AUDIENCE,
        target_sha256: &attestation_sha256,
        minimum_signed_at_ms: attestation.issued_at_ms,
        verification_time_ms: now_ms,
        maximum_clock_skew_ms: attestation_policy.requirements.maximum_clock_skew_ms,
    })?;
    if employer.authorization_id != stored.employer_authorization_id
        || employer.sha256 != stored.employer_authorization_sha256
        || risk.authorization_id != stored.risk_authorization_id
        || risk.sha256 != stored.risk_authorization_sha256
        || employer
            .key_ids
            .iter()
            .any(|key| risk.key_ids.contains(key))
    {
        return Err(JobIntegrityAuthorityError::IdentityConflict);
    }
    let recomputed_stored_expiry = recomputed_base_expiry
        .min(employer.effective_key_expires_at_ms)
        .min(risk.effective_key_expires_at_ms);
    if recomputed_stored_expiry != stored.effective_expires_at_ms {
        return Err(JobIntegrityAuthorityError::IdentityConflict);
    }
    let candidates = job_integrity_revocation_candidates_for_attestation(
        attestation_policy,
        &stored.policy_sha256,
        &attestation,
        &attestation_sha256,
        &employer.key_ids,
        &risk.key_ids,
    )?;
    let borrowed = candidates
        .iter()
        .map(|(kind, id, sha256)| (kind.as_str(), id.as_str(), sha256.as_str()))
        .collect::<Vec<_>>();
    if revoked(&borrowed)? {
        return Ok(if positive {
            job_integrity_resolution(
                JobIntegrityResolutionStatus::Revoked,
                "job_integrity_authority_revoked",
                Vec::new(),
                None,
            )
        } else {
            job_integrity_resolution(
                JobIntegrityResolutionStatus::ReviewRequired,
                "job_integrity_negative_authority_revoked",
                attestation.risk.signal_codes.clone(),
                None,
            )
        });
    }
    if stored.policy_sha256 != current_policy_sha256 {
        return Ok(if positive {
            job_integrity_resolution(
                JobIntegrityResolutionStatus::Expired,
                "trust_policy_not_current",
                Vec::new(),
                None,
            )
        } else {
            job_integrity_nonpositive_resolution(&attestation, Some("trust_policy_not_current"))
        });
    }
    if !positive {
        return Ok(job_integrity_nonpositive_resolution(&attestation, None));
    }
    let effective_expires_at_ms = stored
        .effective_expires_at_ms
        .min(expected.source_expires_at_ms);
    if now_ms < attestation.not_before_ms || now_ms >= effective_expires_at_ms {
        return Ok(job_integrity_resolution(
            JobIntegrityResolutionStatus::Expired,
            "job_integrity_authority_expired",
            Vec::new(),
            None,
        ));
    }
    Ok(job_integrity_resolution(
        JobIntegrityResolutionStatus::Verified,
        "job_integrity_verified",
        Vec::new(),
        Some(JobIntegrityCurrentAuthority {
            subject_sha256: attestation.subject_sha256,
            source_material_sha256: attestation.source_material_sha256,
            attestation_sha256,
            attestation_generation: attestation.attestation_generation,
            head_revision: stored.head_revision,
            head_transition_sha256: stored.transition_sha256,
            policy_sha256: stored.policy_sha256,
            employer_authorization_sha256: stored.employer_authorization_sha256,
            risk_authorization_sha256: stored.risk_authorization_sha256,
            canonical_employer_id: attestation.employer.canonical_employer_id,
            canonical_employer_domain: attestation.employer.canonical_employer_domain,
            risk_policy_sha256: attestation.risk.policy_sha256,
            effective_expires_at_ms,
            canonical_job_id: attestation.canonical_job_id,
            provider_family: attestation.source.provider_family,
            provider_record_id: attestation.source.provider_record_id,
            provider_host: attestation.source.target.host,
            provider_tenant: attestation.source.target.tenant,
            provider_job: attestation.source.target.job,
            provider_variant: attestation.source.target.variant,
            canonical_application_url: attestation.source.canonical_application_url,
            application_domain: attestation.source.application_domain,
            ats_tenant_binding_sha256: attestation.source.ats_tenant_binding_sha256,
        }),
    ))
}

pub fn resolve_current_job_integrity_authority(
    pool: &DbPool,
    expected: &JobIntegrityExpectedSource,
) -> JobIntegrityResult<JobIntegrityResolution> {
    if !job_integrity_expected_source_valid(expected) {
        return Err(JobIntegrityAuthorityError::InvalidAuthority);
    }
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get().map_err(job_integrity_storage)?;
            let tx = conn.transaction().map_err(job_integrity_storage)?;
            let result = resolve_current_job_integrity_authority_sqlite_tx(&tx, expected)?;
            tx.commit().map_err(job_integrity_storage)?;
            Ok(result)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg().map_err(job_integrity_storage)?;
            let mut tx = conn.transaction().map_err(job_integrity_storage)?;
            let result = resolve_current_job_integrity_authority_postgres_tx(&mut tx, expected)?;
            tx.commit().map_err(job_integrity_storage)?;
            Ok(result)
        }
    })
}

pub(crate) fn resolve_current_job_integrity_authority_sqlite_tx(
    tx: &rusqlite::Transaction<'_>,
    expected: &JobIntegrityExpectedSource,
) -> JobIntegrityResult<JobIntegrityResolution> {
    if !job_integrity_expected_source_valid(expected) {
        return Err(JobIntegrityAuthorityError::InvalidAuthority);
    }
    let control = load_job_integrity_control_sqlite(tx)?;
    let Some(current_policy_sha256) = control.policy_sha256.as_deref() else {
        return Ok(job_integrity_resolution(
            JobIntegrityResolutionStatus::Absent,
            "job_integrity_policy_absent",
            Vec::new(),
            None,
        ));
    };
    let current_policy = load_job_integrity_policy_sqlite(tx, current_policy_sha256)?;
    let Some(stored) = load_stored_job_integrity_authority_sqlite(tx, &expected.subject_sha256)?
    else {
        return Ok(job_integrity_resolution(
            JobIntegrityResolutionStatus::Absent,
            "job_integrity_attestation_absent",
            Vec::new(),
            None,
        ));
    };
    let now_ms = job_integrity_db_now_sqlite(tx)?;
    evaluate_stored_job_integrity_authority_sqlite(
        tx,
        expected,
        current_policy_sha256,
        &current_policy,
        stored,
        now_ms,
    )
}

pub(crate) fn resolve_current_job_integrity_authority_sqlite_tx_at_ms(
    tx: &rusqlite::Transaction<'_>,
    expected: &JobIntegrityExpectedSource,
    now_ms: i64,
) -> JobIntegrityResult<JobIntegrityResolution> {
    if !job_integrity_expected_source_valid(expected) {
        return Err(JobIntegrityAuthorityError::InvalidAuthority);
    }
    let control = load_job_integrity_control_sqlite(tx)?;
    let Some(current_policy_sha256) = control.policy_sha256.as_deref() else {
        return Ok(job_integrity_resolution(
            JobIntegrityResolutionStatus::Absent,
            "job_integrity_policy_absent",
            Vec::new(),
            None,
        ));
    };
    let current_policy = load_job_integrity_policy_sqlite(tx, current_policy_sha256)?;
    let Some(stored) = load_stored_job_integrity_authority_sqlite(tx, &expected.subject_sha256)?
    else {
        return Ok(job_integrity_resolution(
            JobIntegrityResolutionStatus::Absent,
            "job_integrity_attestation_absent",
            Vec::new(),
            None,
        ));
    };
    evaluate_stored_job_integrity_authority_sqlite(
        tx,
        expected,
        current_policy_sha256,
        &current_policy,
        stored,
        now_ms,
    )
}

fn evaluate_stored_job_integrity_authority_sqlite(
    tx: &rusqlite::Transaction<'_>,
    expected: &JobIntegrityExpectedSource,
    current_policy_sha256: &str,
    current_policy: &JobIntegrityTrustPolicyV1,
    stored: StoredJobIntegrityAuthority,
    now_ms: i64,
) -> JobIntegrityResult<JobIntegrityResolution> {
    let historical_policy = (stored.policy_sha256 != current_policy_sha256)
        .then(|| load_job_integrity_policy_sqlite(tx, &stored.policy_sha256))
        .transpose()?;
    let attestation_policy = historical_policy.as_ref().unwrap_or(current_policy);
    evaluate_job_integrity_authority(
        stored,
        current_policy_sha256,
        attestation_policy,
        expected,
        now_ms,
        |candidates| job_integrity_any_revocation_sqlite(tx, now_ms, candidates),
    )
}

pub(crate) fn resolve_current_job_integrity_authority_postgres_tx(
    tx: &mut postgres::Transaction<'_>,
    expected: &JobIntegrityExpectedSource,
) -> JobIntegrityResult<JobIntegrityResolution> {
    if !job_integrity_expected_source_valid(expected) {
        return Err(JobIntegrityAuthorityError::InvalidAuthority);
    }
    // Composition lock order: after D, shared global control then exact head.
    let control = load_job_integrity_control_postgres(tx, false)?;
    let Some(current_policy_sha256) = control.policy_sha256.as_deref() else {
        return Ok(job_integrity_resolution(
            JobIntegrityResolutionStatus::Absent,
            "job_integrity_policy_absent",
            Vec::new(),
            None,
        ));
    };
    let current_policy = load_job_integrity_policy_postgres(tx, current_policy_sha256)?;
    let Some(stored) = load_stored_job_integrity_authority_postgres(tx, &expected.subject_sha256)?
    else {
        return Ok(job_integrity_resolution(
            JobIntegrityResolutionStatus::Absent,
            "job_integrity_attestation_absent",
            Vec::new(),
            None,
        ));
    };
    let now_ms = job_integrity_db_now_postgres(tx)?;
    evaluate_stored_job_integrity_authority_postgres(
        tx,
        expected,
        current_policy_sha256,
        &current_policy,
        stored,
        now_ms,
        false,
    )
}

/// Resolve at the representation's single database timestamp. The caller must
/// already hold `lock_job_integrity_publication_fence_shared_postgres_tx`,
/// acquired after `H -> M -> ATS -> D`; that fence makes both existing and
/// missing subject heads stable before this function performs per-subject reads.
pub(crate) fn resolve_current_job_integrity_authority_postgres_tx_after_publication_fence_at_ms(
    tx: &mut postgres::Transaction<'_>,
    expected: &JobIntegrityExpectedSource,
    now_ms: i64,
) -> JobIntegrityResult<JobIntegrityResolution> {
    if !job_integrity_expected_source_valid(expected) {
        return Err(JobIntegrityAuthorityError::InvalidAuthority);
    }
    let control = load_job_integrity_control_postgres_after_publication_fence(tx)?;
    let Some(current_policy_sha256) = control.policy_sha256.as_deref() else {
        return Ok(job_integrity_resolution(
            JobIntegrityResolutionStatus::Absent,
            "job_integrity_policy_absent",
            Vec::new(),
            None,
        ));
    };
    let current_policy =
        load_job_integrity_policy_postgres_after_publication_fence(tx, current_policy_sha256)?;
    let Some(stored) = load_stored_job_integrity_authority_postgres(tx, &expected.subject_sha256)?
    else {
        return Ok(job_integrity_resolution(
            JobIntegrityResolutionStatus::Absent,
            "job_integrity_attestation_absent",
            Vec::new(),
            None,
        ));
    };
    evaluate_stored_job_integrity_authority_postgres(
        tx,
        expected,
        current_policy_sha256,
        &current_policy,
        stored,
        now_ms,
        true,
    )
}

fn evaluate_stored_job_integrity_authority_postgres(
    tx: &mut postgres::Transaction<'_>,
    expected: &JobIntegrityExpectedSource,
    current_policy_sha256: &str,
    current_policy: &JobIntegrityTrustPolicyV1,
    stored: StoredJobIntegrityAuthority,
    now_ms: i64,
    publication_fence_held: bool,
) -> JobIntegrityResult<JobIntegrityResolution> {
    let historical_policy = (stored.policy_sha256 != current_policy_sha256)
        .then(|| {
            if publication_fence_held {
                load_job_integrity_policy_postgres_after_publication_fence(
                    tx,
                    &stored.policy_sha256,
                )
            } else {
                load_job_integrity_policy_postgres(tx, &stored.policy_sha256)
            }
        })
        .transpose()?;
    let attestation_policy = historical_policy.as_ref().unwrap_or(current_policy);
    // The closure needs the same transaction after the head read. Evaluation
    // is synchronous and performs only bounded indexed revocation lookups.
    let mut candidate_copy: Vec<(String, String, String)> = Vec::new();
    let preliminary = evaluate_job_integrity_authority(
        stored,
        current_policy_sha256,
        attestation_policy,
        expected,
        now_ms,
        |candidates| {
            candidate_copy = candidates
                .iter()
                .map(|(kind, id, sha256)| {
                    (
                        (*kind).to_string(),
                        (*id).to_string(),
                        (*sha256).to_string(),
                    )
                })
                .collect();
            Ok(false)
        },
    )?;
    if candidate_copy.is_empty() {
        return Ok(preliminary);
    }
    let borrowed = candidate_copy
        .iter()
        .map(|(kind, id, sha256)| (kind.as_str(), id.as_str(), sha256.as_str()))
        .collect::<Vec<_>>();
    if job_integrity_any_revocation_postgres(tx, now_ms, &borrowed)? {
        return Ok(
            if matches!(
                preliminary.status,
                JobIntegrityResolutionStatus::ReviewRequired
                    | JobIntegrityResolutionStatus::Blocked
                    | JobIntegrityResolutionStatus::Mismatch
            ) {
                job_integrity_resolution(
                    JobIntegrityResolutionStatus::ReviewRequired,
                    "job_integrity_negative_authority_revoked",
                    preliminary.signal_codes,
                    None,
                )
            } else {
                job_integrity_resolution(
                    JobIntegrityResolutionStatus::Revoked,
                    "job_integrity_authority_revoked",
                    Vec::new(),
                    None,
                )
            },
        );
    }
    Ok(preliminary)
}

#[cfg(any(test, feature = "integration-test-support"))]
#[cfg_attr(feature = "integration-test-support", allow(dead_code))]
pub(crate) mod job_integrity_authority_tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    struct Fixture {
        pool: DbPool,
        root_anchor: JobIntegrityRootTrustAnchorV1,
        keys: BTreeMap<String, SigningKey>,
        key_ids: BTreeMap<String, String>,
        policy: JobIntegrityTrustPolicyV1,
        policy_sha256: String,
        now_ms: i64,
    }

    fn signing_key(index: usize) -> SigningKey {
        let seed = std::array::from_fn(|offset| ((index * 37 + offset + 11) % 256) as u8);
        SigningKey::from_bytes(&seed)
    }

    fn test_pool() -> DbPool {
        let path = std::env::temp_dir().join(format!(
            "bluey-job-integrity-{}-{}.sqlite3",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let pool = crate::db::open_pool(&path).expect("open job-integrity test pool");
        crate::db::run_migrations(&pool).expect("migrate job-integrity test pool");
        pool
    }

    fn test_now(pool: &DbPool) -> i64 {
        match pool {
            DbPool::Sqlite(_) => pool
                .get()
                .unwrap()
                .query_row(
                    "SELECT CAST((julianday('now') - 2440587.5) * 86400000 AS INTEGER)",
                    [],
                    |row| row.get(0),
                )
                .unwrap(),
            DbPool::Postgres(_) => crate::db::run_blocking_db(|| {
                pool.get_pg()
                    .unwrap()
                    .query_one(
                        "SELECT floor(extract(epoch FROM clock_timestamp()) * 1000)::bigint",
                        &[],
                    )
                    .unwrap()
                    .get(0)
            }),
        }
    }

    fn role(key_id: &str, key: &SigningKey, now_ms: i64) -> JobIntegrityTrustRoleV1 {
        JobIntegrityTrustRoleV1 {
            threshold: 1,
            keys: BTreeMap::from([(
                key_id.to_string(),
                JobIntegrityTrustKeyV1 {
                    public_key_base64url: base64::engine::general_purpose::URL_SAFE_NO_PAD
                        .encode(key.verifying_key().as_bytes()),
                    valid_from_ms: now_ms - 9_000,
                    expires_at_ms: now_ms + 3_600_000,
                },
            )]),
        }
    }

    struct TestAuthorizationRequest<'a> {
        role_name: &'a str,
        authorization_audience: &'a str,
        target_audience: &'a str,
        target_sha256: &'a str,
        policy_sha256: &'a str,
        signed_at_ms: i64,
        authorization_id: &'a str,
    }

    fn authorization(
        request: TestAuthorizationRequest<'_>,
        key_id: &str,
        key: &SigningKey,
    ) -> String {
        authorization_with_signers(request, &[(key_id, key)])
    }

    fn authorization_with_signers(
        request: TestAuthorizationRequest<'_>,
        signers: &[(&str, &SigningKey)],
    ) -> String {
        let payload = JobIntegrityAuthorizationPayload {
            version: 1,
            audience: request.authorization_audience,
            authorization_id: request.authorization_id,
            role: request.role_name,
            policy_sha256: request.policy_sha256,
            target_audience: request.target_audience,
            target_sha256: request.target_sha256,
            signed_at_ms: request.signed_at_ms,
        };
        let payload_bytes = job_integrity_canonical_json(&payload).unwrap();
        let authorization = JobIntegrityAuthorizationV1 {
            version: 1,
            audience: request.authorization_audience.to_string(),
            authorization_id: request.authorization_id.to_string(),
            role: request.role_name.to_string(),
            policy_sha256: request.policy_sha256.to_string(),
            target_audience: request.target_audience.to_string(),
            target_sha256: request.target_sha256.to_string(),
            signed_at_ms: request.signed_at_ms,
            signatures: signers
                .iter()
                .map(|(key_id, key)| JobIntegrityDetachedSignatureV1 {
                    key_id: (*key_id).to_string(),
                    signature: base64::engine::general_purpose::URL_SAFE_NO_PAD
                        .encode(key.sign(&payload_bytes).to_bytes()),
                })
                .collect(),
        };
        base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(job_integrity_canonical_json(&authorization).unwrap())
    }

    fn fixture() -> Fixture {
        fixture_with_pool(test_pool())
    }

    fn fixture_with_pool(pool: DbPool) -> Fixture {
        let now_ms = test_now(&pool);
        let root = signing_key(1);
        let employer = signing_key(2);
        let risk = signing_key(3);
        let revocation = signing_key(4);
        let key_ids = BTreeMap::from([
            ("root".to_string(), "root-key-v1".to_string()),
            (
                "employer_identity".to_string(),
                "employer-identity-key-v1".to_string(),
            ),
            ("job_risk".to_string(), "job-risk-key-v1".to_string()),
            ("revocation".to_string(), "revocation-key-v1".to_string()),
        ]);
        let keys = BTreeMap::from([
            ("root".to_string(), root),
            ("employer_identity".to_string(), employer),
            ("job_risk".to_string(), risk),
            ("revocation".to_string(), revocation),
        ]);
        let root_anchor = JobIntegrityRootTrustAnchorV1 {
            threshold: 1,
            keys: BTreeMap::from([(
                key_ids["root"].clone(),
                base64::engine::general_purpose::URL_SAFE_NO_PAD
                    .encode(keys["root"].verifying_key().as_bytes()),
            )]),
        };
        let policy = JobIntegrityTrustPolicyV1 {
            version: 1,
            audience: JOB_INTEGRITY_TRUST_POLICY_AUDIENCE.to_string(),
            policy_id: "job-integrity-policy-v1".to_string(),
            trust_generation: 1,
            predecessor_policy_sha256: None,
            delegated_roles: BTreeMap::from([
                (
                    "employer_identity".to_string(),
                    role(
                        &key_ids["employer_identity"],
                        &keys["employer_identity"],
                        now_ms,
                    ),
                ),
                (
                    "job_risk".to_string(),
                    role(&key_ids["job_risk"], &keys["job_risk"], now_ms),
                ),
                (
                    "revocation".to_string(),
                    role(&key_ids["revocation"], &keys["revocation"], now_ms),
                ),
            ]),
            requirements: JobIntegrityPolicyRequirementsV1 {
                allowed_providers: JOB_INTEGRITY_PROVIDERS.map(str::to_string).to_vec(),
                required_identity_methods: vec![
                    "corporate_domain_control".to_string(),
                    "provider_tenant_binding".to_string(),
                ],
                required_identity_evidence_classes: vec![
                    "corporate_domain".to_string(),
                    "provider_tenant".to_string(),
                ],
                required_risk_evidence_classes: vec![
                    "risk_engine".to_string(),
                    "source_consistency".to_string(),
                ],
                allowed_identity_evidence_classes: vec![
                    "corporate_domain".to_string(),
                    "provider_tenant".to_string(),
                ],
                allowed_risk_evidence_classes: vec![
                    "risk_engine".to_string(),
                    "source_consistency".to_string(),
                ],
                allowed_risk_signal_codes: vec![
                    "ats_tenant_mismatch".to_string(),
                    "employer_domain_lookalike".to_string(),
                    "employer_identity_mismatch".to_string(),
                    "identity_unverified".to_string(),
                    "known_scam_signal".to_string(),
                ],
                allowed_risk_policy_sha256s: vec!["5".repeat(64)],
                maximum_positive_lifetime_ms: 1_800_000,
                maximum_nonpositive_lifetime_ms: 1_800_000,
                maximum_clock_skew_ms: 60_000,
                maximum_canonical_bytes: 65_536,
                maximum_identity_evidence_count: 8,
                maximum_risk_evidence_count: 8,
            },
            issued_at_ms: now_ms - 10_000,
            valid_from_ms: now_ms - 9_000,
            expires_at_ms: now_ms + 3_600_000,
        };
        let policy_bytes = job_integrity_canonical_json(&policy).unwrap();
        let policy_sha256 = job_integrity_sha256(&policy_bytes);
        let package = JobIntegrityTrustPolicyPackageV1 {
            canonical_policy_base64url: base64::engine::general_purpose::URL_SAFE_NO_PAD
                .encode(&policy_bytes),
            canonical_root_authorization_base64url: authorization(
                TestAuthorizationRequest {
                    role_name: "root",
                    authorization_audience: JOB_INTEGRITY_ROOT_AUTHORIZATION_AUDIENCE,
                    target_audience: JOB_INTEGRITY_TRUST_POLICY_AUDIENCE,
                    target_sha256: &policy_sha256,
                    policy_sha256: &policy_sha256,
                    signed_at_ms: policy.issued_at_ms,
                    authorization_id: "authorize-job-integrity-policy-v1",
                },
                &key_ids["root"],
                &keys["root"],
            ),
        };
        let imported = import_job_integrity_trust_policy_with_root(
            &pool,
            &package,
            &root_anchor,
            "test-root-operator",
        )
        .unwrap();
        assert!(!imported.replayed);
        let replay = import_job_integrity_trust_policy_with_root(
            &pool,
            &package,
            &root_anchor,
            "test-root-operator",
        )
        .unwrap();
        assert!(replay.replayed);
        Fixture {
            pool,
            root_anchor,
            keys,
            key_ids,
            policy,
            policy_sha256,
            now_ms,
        }
    }

    fn successor_policy(fixture: &Fixture, suffix: &str) -> JobIntegrityTrustPolicyV1 {
        let mut policy = fixture.policy.clone();
        policy.policy_id = format!("job-integrity-policy-{suffix}");
        policy.trust_generation = fixture.policy.trust_generation + 1;
        policy.predecessor_policy_sha256 = Some(fixture.policy_sha256.clone());
        policy.issued_at_ms = fixture.now_ms - 8_000;
        policy.valid_from_ms = fixture.now_ms - 7_000;
        policy.expires_at_ms = fixture.now_ms + 3_000_000;
        for role in policy.delegated_roles.values_mut() {
            for key in role.keys.values_mut() {
                key.valid_from_ms = policy.valid_from_ms;
                key.expires_at_ms = policy.expires_at_ms;
            }
        }
        policy
    }

    fn policy_package(
        policy: &JobIntegrityTrustPolicyV1,
        root_key_id: &str,
        root_key: &SigningKey,
        authorization_id: &str,
    ) -> (JobIntegrityTrustPolicyPackageV1, String) {
        let bytes = job_integrity_canonical_json(policy).unwrap();
        let sha256 = job_integrity_sha256(&bytes);
        (
            JobIntegrityTrustPolicyPackageV1 {
                canonical_policy_base64url: base64::engine::general_purpose::URL_SAFE_NO_PAD
                    .encode(bytes),
                canonical_root_authorization_base64url: authorization(
                    TestAuthorizationRequest {
                        role_name: "root",
                        authorization_audience: JOB_INTEGRITY_ROOT_AUTHORIZATION_AUDIENCE,
                        target_audience: JOB_INTEGRITY_TRUST_POLICY_AUDIENCE,
                        target_sha256: &sha256,
                        policy_sha256: &sha256,
                        signed_at_ms: policy.issued_at_ms,
                        authorization_id,
                    },
                    root_key_id,
                    root_key,
                ),
            },
            sha256,
        )
    }

    fn evidence(class: &str, kind: &str, byte: char, now_ms: i64) -> JobIntegrityEvidenceV1 {
        JobIntegrityEvidenceV1 {
            class: class.to_string(),
            kind: kind.to_string(),
            sha256: byte.to_string().repeat(64),
            observed_at_ms: now_ms - 500,
            expires_at_ms: now_ms + 900_000,
        }
    }

    fn attestation(
        fixture: &Fixture,
        suffix: &str,
        subject_byte: char,
        employer_status: &str,
        risk_status: &str,
        signal_codes: Vec<String>,
    ) -> JobIntegrityAttestationV1 {
        JobIntegrityAttestationV1 {
            version: 1,
            audience: JOB_INTEGRITY_ATTESTATION_AUDIENCE.to_string(),
            attestation_id: format!("job-integrity-attestation-{suffix}"),
            policy_sha256: fixture.policy_sha256.clone(),
            subject_sha256: subject_byte.to_string().repeat(64),
            source_material_sha256: "2".repeat(64),
            attestation_generation: 1,
            predecessor_attestation_sha256: None,
            canonical_job_id: format!("greenhouse:acme:{suffix}"),
            source: JobIntegritySourceV1 {
                provider_family: "greenhouse".to_string(),
                provider_record_id: suffix.to_string(),
                target: JobIntegrityProviderTargetV1 {
                    host: "boards.greenhouse.io".to_string(),
                    tenant: "acme".to_string(),
                    job: suffix.to_string(),
                    variant: "greenhouse_hosted".to_string(),
                },
                canonical_application_url: format!(
                    "https://boards.greenhouse.io/acme/jobs/{suffix}"
                ),
                application_domain: "boards.greenhouse.io".to_string(),
                ats_tenant_binding_sha256: "3".repeat(64),
            },
            employer: JobIntegrityEmployerV1 {
                status: employer_status.to_string(),
                canonical_employer_id: "employer-acme".to_string(),
                canonical_employer_domain: "acme.com".to_string(),
                verification_methods: vec![
                    "corporate_domain_control".to_string(),
                    "provider_tenant_binding".to_string(),
                ],
                evidence: vec![
                    evidence("corporate_domain", "dns_control", '6', fixture.now_ms),
                    evidence("provider_tenant", "tenant_binding", '7', fixture.now_ms),
                ],
            },
            risk: JobIntegrityRiskV1 {
                status: risk_status.to_string(),
                signal_codes,
                policy_sha256: "5".repeat(64),
                input_sha256: "8".repeat(64),
                engine_release_sha256: "9".repeat(64),
                evidence: vec![
                    evidence("risk_engine", "risk_evaluation", 'a', fixture.now_ms),
                    evidence("source_consistency", "source_match", 'b', fixture.now_ms),
                ],
            },
            assessed_at_ms: fixture.now_ms - 300,
            issued_at_ms: fixture.now_ms - 200,
            not_before_ms: fixture.now_ms - 100,
            expires_at_ms: fixture.now_ms + 800_000,
        }
    }

    fn attestation_package(
        fixture: &Fixture,
        attestation: &JobIntegrityAttestationV1,
    ) -> JobIntegrityAttestationPackageV1 {
        let bytes = job_integrity_canonical_json(attestation).unwrap();
        let sha256 = job_integrity_sha256(&bytes);
        JobIntegrityAttestationPackageV1 {
            canonical_attestation_base64url: base64::engine::general_purpose::URL_SAFE_NO_PAD
                .encode(bytes),
            canonical_employer_identity_authorization_base64url: authorization(
                TestAuthorizationRequest {
                    role_name: "employer_identity",
                    authorization_audience: JOB_INTEGRITY_AUTHORIZATION_AUDIENCE,
                    target_audience: JOB_INTEGRITY_ATTESTATION_AUDIENCE,
                    target_sha256: &sha256,
                    policy_sha256: &fixture.policy_sha256,
                    signed_at_ms: attestation.issued_at_ms,
                    authorization_id: &format!(
                        "authorize-employer-{}",
                        attestation.attestation_id
                    ),
                },
                &fixture.key_ids["employer_identity"],
                &fixture.keys["employer_identity"],
            ),
            canonical_job_risk_authorization_base64url: authorization(
                TestAuthorizationRequest {
                    role_name: "job_risk",
                    authorization_audience: JOB_INTEGRITY_AUTHORIZATION_AUDIENCE,
                    target_audience: JOB_INTEGRITY_ATTESTATION_AUDIENCE,
                    target_sha256: &sha256,
                    policy_sha256: &fixture.policy_sha256,
                    signed_at_ms: attestation.issued_at_ms,
                    authorization_id: &format!(
                        "authorize-risk-{}",
                        attestation.attestation_id
                    ),
                },
                &fixture.key_ids["job_risk"],
                &fixture.keys["job_risk"],
            ),
        }
    }

    fn successor_attestation(
        predecessor: &JobIntegrityAttestationV1,
        predecessor_sha256: &str,
    ) -> JobIntegrityAttestationV1 {
        let mut successor = predecessor.clone();
        successor.attestation_id = format!("{}-successor", predecessor.attestation_id);
        successor.attestation_generation = predecessor.attestation_generation + 1;
        successor.predecessor_attestation_sha256 = Some(predecessor_sha256.to_string());
        successor.assessed_at_ms += 1;
        successor.issued_at_ms += 1;
        successor.not_before_ms += 1;
        successor
    }

    fn shared_fixture_with_pool(pool: &DbPool) -> Fixture {
        let stored = crate::db::run_blocking_db(|| -> anyhow::Result<Option<(String, String)>> {
            match pool {
                DbPool::Sqlite(_) => pool
                    .get()?
                    .query_row(
                        "SELECT policy.policy_sha256, policy.canonical_policy_base64url
                           FROM jobs_job_integrity_control control
                           JOIN jobs_job_integrity_trust_policies policy
                             ON policy.policy_sha256=control.current_policy_sha256
                          WHERE control.singleton_id=1",
                        [],
                        |row| Ok((row.get(0)?, row.get(1)?)),
                    )
                    .optional()
                    .map_err(anyhow::Error::new),
                DbPool::Postgres(_) => Ok(pool
                    .get_pg()?
                    .query_opt(
                        "SELECT policy.policy_sha256, policy.canonical_policy_base64url
                           FROM jobs_job_integrity_control control
                           JOIN jobs_job_integrity_trust_policies policy
                             ON policy.policy_sha256=control.current_policy_sha256
                          WHERE control.singleton_id=1",
                        &[],
                    )?
                    .map(|row| (row.get(0), row.get(1)))),
            }
        })
        .expect("load shared signed job-integrity fixture policy");
        let Some((policy_sha256, canonical_policy_base64url)) = stored else {
            return fixture_with_pool(pool.clone());
        };

        let policy_bytes = job_integrity_decode_base64url_bounded(&canonical_policy_base64url)
            .expect("decode shared signed job-integrity fixture policy");
        assert_eq!(job_integrity_sha256(&policy_bytes), policy_sha256);
        let policy: JobIntegrityTrustPolicyV1 = serde_json::from_slice(&policy_bytes)
            .expect("parse shared signed job-integrity fixture policy");
        let root = signing_key(1);
        let employer = signing_key(2);
        let risk = signing_key(3);
        let revocation = signing_key(4);
        let key_ids = BTreeMap::from([
            ("root".to_string(), "root-key-v1".to_string()),
            (
                "employer_identity".to_string(),
                "employer-identity-key-v1".to_string(),
            ),
            ("job_risk".to_string(), "job-risk-key-v1".to_string()),
            ("revocation".to_string(), "revocation-key-v1".to_string()),
        ]);
        let keys = BTreeMap::from([
            ("root".to_string(), root),
            ("employer_identity".to_string(), employer),
            ("job_risk".to_string(), risk),
            ("revocation".to_string(), revocation),
        ]);
        let root_anchor = JobIntegrityRootTrustAnchorV1 {
            threshold: 1,
            keys: BTreeMap::from([(
                key_ids["root"].clone(),
                base64::engine::general_purpose::URL_SAFE_NO_PAD
                    .encode(keys["root"].verifying_key().as_bytes()),
            )]),
        };
        for (role_name, key_name) in [
            ("employer_identity", "employer_identity"),
            ("job_risk", "job_risk"),
            ("revocation", "revocation"),
        ] {
            let stored_key = &policy.delegated_roles[role_name].keys[&key_ids[key_name]];
            assert_eq!(
                stored_key.public_key_base64url,
                base64::engine::general_purpose::URL_SAFE_NO_PAD
                    .encode(keys[key_name].verifying_key().as_bytes()),
                "shared signed fixture policy must use the deterministic test key",
            );
        }
        Fixture {
            pool: pool.clone(),
            root_anchor,
            keys,
            key_ids,
            policy,
            policy_sha256,
            now_ms: test_now(pool),
        }
    }

    pub(crate) fn install_signed_job_integrity_positive_fixture(
        pool: &DbPool,
        expected_source: &JobIntegrityExpectedSource,
        canonical_employer_domain: &str,
    ) -> JobIntegrityCurrentAuthority {
        let fixture = shared_fixture_with_pool(pool);
        let suffix = &expected_source.subject_sha256[..24];
        let mut attestation = attestation(
            &fixture,
            suffix,
            '1',
            "verified",
            "clear",
            Vec::new(),
        );
        attestation.subject_sha256 = expected_source.subject_sha256.clone();
        attestation.source_material_sha256 = expected_source.source_material_sha256.clone();
        attestation.canonical_job_id = expected_source.canonical_job_id.clone();
        attestation.source.provider_family = expected_source.provider_family.clone();
        attestation.source.provider_record_id = expected_source.provider_record_id.clone();
        attestation.source.target.host = expected_source.provider_host.clone();
        attestation.source.target.tenant = expected_source.provider_tenant.clone();
        attestation.source.target.job = expected_source.provider_job.clone();
        attestation.source.target.variant = expected_source.provider_variant.clone();
        attestation.source.canonical_application_url =
            expected_source.canonical_application_url.clone();
        attestation.source.application_domain = expected_source.application_domain.clone();
        attestation.source.ats_tenant_binding_sha256 =
            expected_source.ats_tenant_binding_sha256.clone();
        attestation.employer.canonical_employer_domain = canonical_employer_domain.to_string();
        assert!(expected_source.source_expires_at_ms > attestation.not_before_ms);
        attestation.expires_at_ms = attestation
            .expires_at_ms
            .min(expected_source.source_expires_at_ms);

        import_job_integrity_attestation(
            pool,
            &attestation_package(&fixture, &attestation),
            "production-positive-fixture",
        )
        .expect("import dual-signed positive job-integrity fixture");
        let resolution = resolve_current_job_integrity_authority(pool, expected_source)
            .expect("resolve dual-signed positive job-integrity fixture");
        assert_eq!(resolution.status, JobIntegrityResolutionStatus::Verified);
        resolution.authority.expect("positive signed fixture authority")
    }

    fn expected(attestation: &JobIntegrityAttestationV1) -> JobIntegrityExpectedSource {
        JobIntegrityExpectedSource {
            subject_sha256: attestation.subject_sha256.clone(),
            source_material_sha256: attestation.source_material_sha256.clone(),
            source_expires_at_ms: attestation.expires_at_ms,
            canonical_job_id: attestation.canonical_job_id.clone(),
            provider_family: attestation.source.provider_family.clone(),
            provider_record_id: attestation.source.provider_record_id.clone(),
            provider_host: attestation.source.target.host.clone(),
            provider_tenant: attestation.source.target.tenant.clone(),
            provider_job: attestation.source.target.job.clone(),
            provider_variant: attestation.source.target.variant.clone(),
            canonical_application_url: attestation.source.canonical_application_url.clone(),
            application_domain: attestation.source.application_domain.clone(),
            ats_tenant_binding_sha256: attestation.source.ats_tenant_binding_sha256.clone(),
        }
    }

    fn revocation_package(
        fixture: &Fixture,
        generation: i64,
        predecessor: Option<String>,
        kind: &str,
        id: &str,
        sha256: &str,
    ) -> JobIntegrityRevocationPackageV1 {
        let revocation = JobIntegrityRevocationV1 {
            version: 1,
            audience: JOB_INTEGRITY_REVOCATION_AUDIENCE.to_string(),
            revocation_id: format!("job-integrity-revocation-{generation}"),
            policy_sha256: fixture.policy_sha256.clone(),
            revocation_generation: generation,
            predecessor_revocation_sha256: predecessor,
            subject_kind: kind.to_string(),
            subject_id: id.to_string(),
            subject_sha256: sha256.to_string(),
            reason_code: "authority_revoked".to_string(),
            reason_ref: format!("public-incident-ref-{generation}"),
            effective_at_ms: fixture.now_ms - 20,
            issued_at_ms: fixture.now_ms - 30,
        };
        let bytes = job_integrity_canonical_json(&revocation).unwrap();
        let target_sha256 = job_integrity_sha256(&bytes);
        JobIntegrityRevocationPackageV1 {
            canonical_revocation_base64url: base64::engine::general_purpose::URL_SAFE_NO_PAD
                .encode(bytes),
            canonical_authorization_base64url: authorization(
                TestAuthorizationRequest {
                    role_name: "revocation",
                    authorization_audience: JOB_INTEGRITY_AUTHORIZATION_AUDIENCE,
                    target_audience: JOB_INTEGRITY_REVOCATION_AUDIENCE,
                    target_sha256: &target_sha256,
                    policy_sha256: &fixture.policy_sha256,
                    signed_at_ms: revocation.issued_at_ms,
                    authorization_id: &format!("authorize-revocation-{generation}"),
                },
                &fixture.key_ids["revocation"],
                &fixture.keys["revocation"],
            ),
        }
    }

    #[test]
    fn signed_positive_lifecycle_replays_and_fails_closed_on_source_or_expiry() {
        let fixture = fixture();
        let attestation = attestation(&fixture, "123", '1', "verified", "clear", Vec::new());
        let package = attestation_package(&fixture, &attestation);
        let imported =
            import_job_integrity_attestation(&fixture.pool, &package, "test-integrity-importer")
                .unwrap();
        assert_eq!(imported.head_revision, Some(1));
        assert!(!imported.replayed);
        let replay =
            import_job_integrity_attestation(&fixture.pool, &package, "test-integrity-importer")
                .unwrap();
        assert_eq!(
            replay,
            JobIntegrityImportResult {
                replayed: true,
                ..imported.clone()
            }
        );
        let source = expected(&attestation);
        let resolved = resolve_current_job_integrity_authority(&fixture.pool, &source).unwrap();
        assert_eq!(resolved.status, JobIntegrityResolutionStatus::Verified);
        let authority = resolved.authority.unwrap();
        assert_eq!(authority.attestation_sha256, imported.object_sha256);
        assert_eq!(authority.head_revision, 1);
        let serialized = serde_json::to_value(&authority).unwrap();
        let object = serialized.as_object().unwrap();
        let mut receipt_keys = object.keys().map(String::as_str).collect::<Vec<_>>();
        receipt_keys.sort_unstable();
        assert_eq!(
            receipt_keys,
            [
                "applicationDomain",
                "atsTenantBindingSha256",
                "attestationGeneration",
                "attestationSha256",
                "canonicalApplicationUrl",
                "canonicalEmployerDomain",
                "canonicalEmployerId",
                "canonicalJobId",
                "employerIdentityAuthorizationSha256",
                "expiresAtMs",
                "headRevision",
                "headTransitionSha256",
                "jobRiskAuthorizationSha256",
                "policySha256",
                "providerFamily",
                "providerHost",
                "providerJob",
                "providerRecordId",
                "providerTenant",
                "providerVariant",
                "riskPolicySha256",
                "sourceMaterialSha256",
                "subjectSha256",
            ]
        );
        assert_eq!(
            serde_json::from_value::<JobIntegrityCurrentAuthority>(serialized.clone()).unwrap(),
            authority
        );
        let mut missing_field = serialized.clone();
        missing_field
            .as_object_mut()
            .unwrap()
            .remove("jobRiskAuthorizationSha256");
        assert!(serde_json::from_value::<JobIntegrityCurrentAuthority>(missing_field).is_err());
        let mut unknown_field = serialized;
        unknown_field
            .as_object_mut()
            .unwrap()
            .insert("effectiveExpiresAtMs".to_string(), json!(fixture.now_ms));
        assert!(serde_json::from_value::<JobIntegrityCurrentAuthority>(unknown_field).is_err());

        let successor = successor_attestation(&attestation, &imported.object_sha256);
        let successor_import = import_job_integrity_attestation(
            &fixture.pool,
            &attestation_package(&fixture, &successor),
            "test-integrity-importer",
        )
        .unwrap();
        assert_eq!(successor_import.head_revision, Some(2));
        let historical_replay =
            import_job_integrity_attestation(&fixture.pool, &package, "test-integrity-importer")
                .unwrap();
        assert_eq!(
            historical_replay,
            JobIntegrityImportResult {
                replayed: true,
                ..imported.clone()
            },
            "exact replay must preserve its original immutable head-transition result after a successor"
        );

        let mut drifted = source.clone();
        drifted.source_material_sha256 = "f".repeat(64);
        assert_eq!(
            resolve_current_job_integrity_authority(&fixture.pool, &drifted)
                .unwrap()
                .status,
            JobIntegrityResolutionStatus::Mismatch
        );
        let mut expired = source;
        expired.source_expires_at_ms = fixture.now_ms - 1;
        assert_eq!(
            resolve_current_job_integrity_authority(&fixture.pool, &expired)
                .unwrap()
                .status,
            JobIntegrityResolutionStatus::Expired
        );
    }

    #[test]
    fn smartrecruiters_cross_host_authority_resolves_with_independent_exact_bindings() {
        let fixture = fixture();
        let mut attestation = attestation(
            &fixture,
            "smartrecruiters-cross-host",
            'c',
            "verified",
            "clear",
            Vec::new(),
        );
        attestation.canonical_job_id = "smartrecruiters:acme:abc".to_string();
        attestation.source.provider_family = "smartrecruiters".to_string();
        attestation.source.provider_record_id =
            "smartrecruiters:jobs.smartrecruiters.com:acme:abc".to_string();
        attestation.source.target.host = "jobs.smartrecruiters.com".to_string();
        attestation.source.target.tenant = "acme".to_string();
        attestation.source.target.job = "abc".to_string();
        attestation.source.target.variant = "smartrecruiters_posting".to_string();
        attestation.source.canonical_application_url =
            "https://www.smartrecruiters.com/acme/abc-platform-engineer".to_string();
        attestation.source.application_domain = "www.smartrecruiters.com".to_string();

        let imported = import_job_integrity_attestation(
            &fixture.pool,
            &attestation_package(&fixture, &attestation),
            "smartrecruiters-cross-host-test",
        )
        .expect("valid SmartRecruiters cross-host authority imports");
        assert_eq!(imported.head_revision, Some(1));

        let exact = expected(&attestation);
        let resolved = resolve_current_job_integrity_authority(&fixture.pool, &exact)
            .expect("valid SmartRecruiters cross-host authority resolves");
        assert_eq!(resolved.status, JobIntegrityResolutionStatus::Verified);
        let authority = resolved.authority.expect("positive authority");
        assert_eq!(authority.provider_host, "jobs.smartrecruiters.com");
        assert_eq!(authority.application_domain, "www.smartrecruiters.com");
        assert_eq!(
            authority.canonical_application_url,
            "https://www.smartrecruiters.com/acme/abc-platform-engineer"
        );

        let mut provider_host_drift = exact;
        provider_host_drift.provider_host = "www.smartrecruiters.com".to_string();
        assert_eq!(
            resolve_current_job_integrity_authority(&fixture.pool, &provider_host_drift)
                .expect("binding drift is a typed denial")
                .status,
            JobIntegrityResolutionStatus::Mismatch
        );
    }

    #[test]
    fn sqlite_storage_rejects_revocation_predecessor_generation_jump() {
        let fixture = fixture();
        import_job_integrity_revocation(
            &fixture.pool,
            &revocation_package(
                &fixture,
                1,
                None,
                "risk_policy",
                "risk-policy-v1",
                &"5".repeat(64),
            ),
            "sqlite-revocation-chain-test",
        )
        .unwrap();
        let error = fixture
            .pool
            .get()
            .unwrap()
            .execute(
                "INSERT INTO jobs_job_integrity_revocations(
                    revocation_sha256,revocation_id,revocation_generation,
                    predecessor_revocation_sha256,policy_sha256,subject_kind,subject_id,
                    subject_sha256,reason_code,reason_ref,effective_at_ms,issued_at_ms,
                    canonical_revocation_base64url,authorization_id,authorization_sha256,
                    canonical_authorization_base64url,recorded_by,recorded_at_ms)
                 SELECT ?1,'revocation-generation-jump',100,revocation_sha256,policy_sha256,
                    subject_kind,'generation-jump-subject',subject_sha256,reason_code,reason_ref,
                    effective_at_ms,issued_at_ms,canonical_revocation_base64url,
                    'revocation-generation-jump-authorization',?2,
                    canonical_authorization_base64url,recorded_by,recorded_at_ms
                   FROM jobs_job_integrity_revocations WHERE revocation_generation=1",
                params!["0f".repeat(32), "1e".repeat(32)],
            )
            .expect_err("storage must reject a nonadjacent revocation predecessor");
        assert!(error
            .to_string()
            .contains("job-integrity revocation predecessor binding is invalid"));
    }

    #[test]
    fn postgres_positive_lifecycle_when_configured() {
        let Ok(database_url) = std::env::var("BLUEY_TEST_POSTGRES_URL") else {
            return;
        };
        crate::db::run_blocking_db(|| {
            let schema = format!("phase614b_core_{}", uuid::Uuid::new_v4().simple());
            let mut admin = postgres::Client::connect(&database_url, postgres::NoTls).unwrap();
            admin
                .batch_execute(&format!(
                    "CREATE SCHEMA {schema}; SET search_path TO {schema}, public; {}",
                    crate::db::POSTGRES_JOBS_SIGNED_JOB_INTEGRITY_AUTHORITY,
                ))
                .unwrap();
            admin.batch_execute("SET search_path TO public").unwrap();
            let separator = if database_url.contains('?') { '&' } else { '?' };
            let scoped_url =
                format!("{database_url}{separator}options=-csearch_path%3D{schema}%2Cpublic");
            let lifecycle = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let fixture =
                    fixture_with_pool(crate::db::open_postgres_pool(&scoped_url).unwrap());
                let attestation =
                    attestation(&fixture, "postgres", '9', "verified", "clear", Vec::new());
                let package = attestation_package(&fixture, &attestation);
                let imported = import_job_integrity_attestation(
                    &fixture.pool,
                    &package,
                    "postgres-integrity-test",
                )
                .unwrap();
                assert!(!imported.replayed);
                assert!(
                    import_job_integrity_attestation(
                        &fixture.pool,
                        &package,
                        "postgres-integrity-test",
                    )
                    .unwrap()
                    .replayed
                );
                let successor = successor_attestation(&attestation, &imported.object_sha256);
                let successor_package = attestation_package(&fixture, &successor);
                let successor_import = import_job_integrity_attestation(
                    &fixture.pool,
                    &successor_package,
                    "postgres-integrity-test",
                )
                .unwrap();
                assert_eq!(successor_import.head_revision, Some(2));
                assert_eq!(
                    import_job_integrity_attestation(
                        &fixture.pool,
                        &package,
                        "postgres-integrity-test",
                    )
                    .unwrap(),
                    JobIntegrityImportResult {
                        replayed: true,
                        ..imported.clone()
                    },
                    "PostgreSQL exact replay must retain its original immutable transition"
                );
                assert_eq!(
                    resolve_current_job_integrity_authority(
                        &fixture.pool,
                        &expected(&attestation),
                    )
                    .unwrap()
                    .status,
                    JobIntegrityResolutionStatus::Verified,
                );
                import_job_integrity_revocation(
                    &fixture.pool,
                    &revocation_package(
                        &fixture,
                        1,
                        None,
                        "attestation",
                        &successor.attestation_id,
                        &successor_import.object_sha256,
                    ),
                    "postgres-integrity-test",
                )
                .unwrap();
                let error = fixture
                    .pool
                    .get_pg()
                    .unwrap()
                    .execute(
                        "INSERT INTO jobs_job_integrity_revocations(
                            revocation_sha256,revocation_id,revocation_generation,
                            predecessor_revocation_sha256,policy_sha256,subject_kind,subject_id,
                            subject_sha256,reason_code,reason_ref,effective_at_ms,issued_at_ms,
                            canonical_revocation_base64url,authorization_id,authorization_sha256,
                            canonical_authorization_base64url,recorded_by,recorded_at_ms)
                         SELECT $1,'revocation-generation-jump',100,revocation_sha256,policy_sha256,
                            subject_kind,'generation-jump-subject',subject_sha256,reason_code,
                            reason_ref,effective_at_ms,issued_at_ms,
                            canonical_revocation_base64url,
                            'revocation-generation-jump-authorization',$2,
                            canonical_authorization_base64url,recorded_by,recorded_at_ms
                           FROM jobs_job_integrity_revocations
                          WHERE revocation_generation=1",
                        &[&"0f".repeat(32), &"1e".repeat(32)],
                    )
                    .expect_err("PostgreSQL must reject a nonadjacent revocation predecessor");
                assert_eq!(error.code().map(|code| code.code()), Some("P0001"));
                assert_eq!(
                    error.as_db_error().map(|error| error.message()),
                    Some("job-integrity revocation predecessor binding is invalid")
                );
                assert_eq!(
                    resolve_current_job_integrity_authority(
                        &fixture.pool,
                        &expected(&attestation),
                    )
                    .unwrap()
                    .status,
                    JobIntegrityResolutionStatus::Revoked,
                );
            }));
            admin
                .batch_execute(&format!("DROP SCHEMA {schema} CASCADE"))
                .unwrap();
            if let Err(payload) = lifecycle {
                std::panic::resume_unwind(payload);
            }
        });
    }

    #[test]
    fn postgres_representation_fence_blocks_integrity_publication_when_configured() {
        let Ok(database_url) = std::env::var("BLUEY_TEST_POSTGRES_URL") else {
            return;
        };
        crate::db::run_blocking_db(|| {
            let schema = format!("phase614b_fence_{}", uuid::Uuid::new_v4().simple());
            let mut admin = postgres::Client::connect(&database_url, postgres::NoTls).unwrap();
            admin
                .batch_execute(&format!(
                    "CREATE SCHEMA {schema}; SET search_path TO {schema}, public; {}",
                    crate::db::POSTGRES_JOBS_SIGNED_JOB_INTEGRITY_AUTHORITY,
                ))
                .unwrap();
            admin.batch_execute("SET search_path TO public").unwrap();
            let separator = if database_url.contains('?') { '&' } else { '?' };
            let scoped_url =
                format!("{database_url}{separator}options=-csearch_path%3D{schema}%2Cpublic");
            let lifecycle = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let mut reader = postgres::Client::connect(&scoped_url, postgres::NoTls).unwrap();
                let mut reader_tx = reader.transaction().unwrap();
                lock_job_integrity_publication_fence_shared_postgres_tx(&mut reader_tx).unwrap();

                let writer_url = scoped_url.clone();
                let blocked = std::thread::spawn(move || {
                    let mut writer =
                        postgres::Client::connect(&writer_url, postgres::NoTls).unwrap();
                    writer.batch_execute("SET lock_timeout = '250ms'").unwrap();
                    writer
                        .query_one(
                            "SELECT singleton_id FROM jobs_job_integrity_control
                              WHERE singleton_id=1 FOR UPDATE",
                            &[],
                        )
                        .expect_err("publication UPDATE must wait behind representation fence")
                })
                .join()
                .unwrap();
                assert_eq!(blocked.code().map(|code| code.code()), Some("55P03"));

                reader_tx.rollback().unwrap();
                let mut writer =
                    postgres::Client::connect(&scoped_url, postgres::NoTls).unwrap();
                writer
                    .query_one(
                        "SELECT singleton_id FROM jobs_job_integrity_control
                          WHERE singleton_id=1 FOR UPDATE",
                        &[],
                    )
                    .expect("publication UPDATE proceeds after representation fence releases");
            }));
            admin
                .batch_execute(&format!("DROP SCHEMA {schema} CASCADE"))
                .unwrap();
            if let Err(payload) = lifecycle {
                std::panic::resume_unwind(payload);
            }
        });
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn postgres_public_integrity_apis_enter_blocking_boundary_when_configured() {
        let Ok(database_url) = std::env::var("BLUEY_TEST_POSTGRES_URL") else {
            return;
        };
        let schema = format!("phase614b_async_{}", uuid::Uuid::new_v4().simple());
        let scoped_url = crate::db::run_blocking_db(|| {
            let mut admin = postgres::Client::connect(&database_url, postgres::NoTls).unwrap();
            admin
                .batch_execute(&format!(
                    "CREATE SCHEMA {schema}; SET search_path TO {schema}, public; {}",
                    crate::db::POSTGRES_JOBS_SIGNED_JOB_INTEGRITY_AUTHORITY,
                ))
                .unwrap();
            let separator = if database_url.contains('?') { '&' } else { '?' };
            format!("{database_url}{separator}options=-csearch_path%3D{schema}%2Cpublic")
        });
        let fixture = crate::db::run_blocking_db(|| {
            fixture_with_pool(crate::db::open_postgres_pool(&scoped_url).unwrap())
        });
        let attestation = attestation(
            &fixture,
            "async-public",
            '8',
            "verified",
            "clear",
            Vec::new(),
        );
        let package = attestation_package(&fixture, &attestation);
        let lifecycle = (|| -> JobIntegrityResult<()> {
            let imported = import_job_integrity_attestation(
                &fixture.pool,
                &package,
                "postgres-async-integrity-test",
            )?;
            if resolve_current_job_integrity_authority(
                &fixture.pool,
                &expected(&attestation),
            )?
            .status
                != JobIntegrityResolutionStatus::Verified
            {
                return Err(JobIntegrityAuthorityError::InvalidAuthority);
            }
            import_job_integrity_revocation(
                &fixture.pool,
                &revocation_package(
                    &fixture,
                    1,
                    None,
                    "attestation",
                    &attestation.attestation_id,
                    &imported.object_sha256,
                ),
                "postgres-async-integrity-test",
            )?;
            if resolve_current_job_integrity_authority(
                &fixture.pool,
                &expected(&attestation),
            )?
            .status
                != JobIntegrityResolutionStatus::Revoked
            {
                return Err(JobIntegrityAuthorityError::InvalidAuthority);
            }
            Ok(())
        })();
        drop(fixture);
        crate::db::run_blocking_db(|| {
            let mut admin = postgres::Client::connect(&database_url, postgres::NoTls).unwrap();
            admin
                .batch_execute(&format!("DROP SCHEMA IF EXISTS {schema} CASCADE"))
                .unwrap();
        });
        lifecycle.unwrap();
    }

    #[test]
    fn signed_nonpositive_heads_preserve_review_block_and_mismatch() {
        for (suffix, subject, employer, risk, signals, expected_status) in [
            (
                "review",
                'c',
                "unverified",
                "review_required",
                vec!["identity_unverified".to_string()],
                JobIntegrityResolutionStatus::ReviewRequired,
            ),
            (
                "blocked",
                'd',
                "verified",
                "blocked",
                vec!["known_scam_signal".to_string()],
                JobIntegrityResolutionStatus::Blocked,
            ),
            (
                "mismatch",
                'e',
                "mismatch",
                "blocked",
                vec!["employer_identity_mismatch".to_string()],
                JobIntegrityResolutionStatus::Mismatch,
            ),
        ] {
            let case_fixture = fixture();
            let attestation = attestation(&case_fixture, suffix, subject, employer, risk, signals);
            import_job_integrity_attestation(
                &case_fixture.pool,
                &attestation_package(&case_fixture, &attestation),
                "test-integrity-importer",
            )
            .unwrap();
            assert_eq!(
                resolve_current_job_integrity_authority(
                    &case_fixture.pool,
                    &expected(&attestation),
                )
                .unwrap()
                .status,
                expected_status
            );
        }
    }

    #[test]
    fn positive_revoked_material_cannot_publish_and_revoked_negative_never_falls_back() {
        let positive_fixture = fixture();
        let positive = attestation(
            &positive_fixture,
            "positive",
            '4',
            "verified",
            "clear",
            Vec::new(),
        );
        let risk_policy = positive.risk.policy_sha256.clone();
        import_job_integrity_revocation(
            &positive_fixture.pool,
            &revocation_package(
                &positive_fixture,
                1,
                None,
                "risk_policy",
                &risk_policy,
                &risk_policy,
            ),
            "test-revocation-operator",
        )
        .unwrap();
        assert!(matches!(
            import_job_integrity_attestation(
                &positive_fixture.pool,
                &attestation_package(&positive_fixture, &positive),
                "test-integrity-importer"
            ),
            Err(JobIntegrityAuthorityError::Revoked)
        ));
        let head_count: i64 = positive_fixture
            .pool
            .get()
            .unwrap()
            .query_row("SELECT count(*) FROM jobs_job_integrity_heads", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(head_count, 0);

        let negative_fixture = fixture();
        let negative = attestation(
            &negative_fixture,
            "negative",
            '5',
            "unverified",
            "review_required",
            vec!["identity_unverified".to_string()],
        );
        let imported = import_job_integrity_attestation(
            &negative_fixture.pool,
            &attestation_package(&negative_fixture, &negative),
            "test-integrity-importer",
        )
        .unwrap();
        import_job_integrity_revocation(
            &negative_fixture.pool,
            &revocation_package(
                &negative_fixture,
                1,
                None,
                "attestation",
                &negative.attestation_id,
                &imported.object_sha256,
            ),
            "test-revocation-operator",
        )
        .unwrap();
        let resolved =
            resolve_current_job_integrity_authority(&negative_fixture.pool, &expected(&negative))
                .unwrap();
        assert_eq!(
            resolved.status,
            JobIntegrityResolutionStatus::ReviewRequired
        );
        assert_eq!(
            resolved.reason_code,
            "job_integrity_negative_authority_revoked"
        );
        assert!(resolved.authority.is_none());
    }

    #[test]
    fn attestation_generation_forks_and_identity_changes_are_zero_mutation_conflicts() {
        let fixture = fixture();
        let first = attestation(&fixture, "chain", '6', "verified", "clear", Vec::new());
        let first_package = attestation_package(&fixture, &first);
        let imported = import_job_integrity_attestation(
            &fixture.pool,
            &first_package,
            "test-integrity-importer",
        )
        .unwrap();
        let mut changed_identity = first.clone();
        changed_identity.source_material_sha256 = "c".repeat(64);
        assert!(matches!(
            import_job_integrity_attestation(
                &fixture.pool,
                &attestation_package(&fixture, &changed_identity),
                "test-integrity-importer"
            ),
            Err(JobIntegrityAuthorityError::IdentityConflict)
        ));
        let mut gap = first;
        gap.attestation_id = "job-integrity-attestation-chain-gap".to_string();
        gap.attestation_generation = 3;
        gap.predecessor_attestation_sha256 = Some(imported.object_sha256);
        assert!(matches!(
            import_job_integrity_attestation(
                &fixture.pool,
                &attestation_package(&fixture, &gap),
                "test-integrity-importer"
            ),
            Err(JobIntegrityAuthorityError::CompareAndSwapConflict)
        ));
        let count: i64 = fixture
            .pool
            .get()
            .unwrap()
            .query_row(
                "SELECT count(*) FROM jobs_job_integrity_attestations",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[derive(Debug, Clone, Copy, Deserialize)]
    #[serde(rename_all = "snake_case")]
    enum SharedJobIntegrityCanonicalMutation {
        RemoveTerminalNewline,
        AppendTerminalNewline,
        PrependAsciiSpace,
        DuplicateTopLevelVersion,
        AppendUnknownTopLevelField,
        SwapVersionAndAudience,
    }

    #[derive(Debug, Clone, Copy, Deserialize)]
    #[serde(rename_all = "snake_case")]
    enum SharedJobIntegrityExpectedError {
        #[serde(rename = "invalid_authority")]
        Authority,
        #[serde(rename = "invalid_envelope")]
        Envelope,
        #[serde(rename = "invalid_trust_anchor")]
        TrustAnchor,
        #[serde(rename = "invalid_signature")]
        Signature,
    }

    #[derive(Debug, Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct SharedJobIntegrityCanonicalRejection {
        name: String,
        mutation: SharedJobIntegrityCanonicalMutation,
        expected_error: SharedJobIntegrityExpectedError,
    }

    #[derive(Debug, Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct SharedJobIntegrityKeyRejection {
        name: String,
        public_key_base64url: String,
        expected_error: SharedJobIntegrityExpectedError,
    }

    #[derive(Debug, Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct SharedJobIntegritySignatureRejection {
        name: String,
        signature: String,
        expected_error: SharedJobIntegrityExpectedError,
    }

    // Field order intentionally mirrors JobIntegrityAuthorizationPayload exactly.
    #[derive(Debug, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct SharedJobIntegrityAuthorizationPayload {
        version: i64,
        audience: String,
        authorization_id: String,
        role: String,
        policy_sha256: String,
        target_audience: String,
        target_sha256: String,
        signed_at_ms: i64,
    }

    #[derive(Debug, Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct SharedJobIntegrityAuthorizationVector {
        public_key_base64url: String,
        payload: SharedJobIntegrityAuthorizationPayload,
        payload_sha256: String,
        envelope: JobIntegrityAuthorizationV1,
        envelope_sha256: String,
    }

    #[derive(Debug, Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct SharedJobIntegrityAuthorityFixture {
        schema_version: i64,
        attestation: JobIntegrityAttestationV1,
        attestation_sha256: String,
        authorization: SharedJobIntegrityAuthorizationVector,
        canonical_rejections: Vec<SharedJobIntegrityCanonicalRejection>,
        key_rejections: Vec<SharedJobIntegrityKeyRejection>,
        signature_rejections: Vec<SharedJobIntegritySignatureRejection>,
    }

    fn mutate_shared_job_integrity_attestation(
        canonical: &[u8],
        mutation: SharedJobIntegrityCanonicalMutation,
    ) -> Vec<u8> {
        match mutation {
            SharedJobIntegrityCanonicalMutation::RemoveTerminalNewline => {
                assert_eq!(canonical.last(), Some(&b'\n'));
                canonical[..canonical.len() - 1].to_vec()
            }
            SharedJobIntegrityCanonicalMutation::AppendTerminalNewline => {
                let mut mutated = canonical.to_vec();
                mutated.push(b'\n');
                mutated
            }
            SharedJobIntegrityCanonicalMutation::PrependAsciiSpace => {
                let mut mutated = Vec::with_capacity(canonical.len() + 1);
                mutated.push(b' ');
                mutated.extend_from_slice(canonical);
                mutated
            }
            SharedJobIntegrityCanonicalMutation::DuplicateTopLevelVersion => {
                let text = std::str::from_utf8(canonical).expect("canonical fixture UTF-8");
                let mutated =
                    text.replacen("{\"version\":1,", "{\"version\":1,\"version\":1,", 1);
                assert_ne!(mutated, text);
                mutated.into_bytes()
            }
            SharedJobIntegrityCanonicalMutation::AppendUnknownTopLevelField => {
                let mut text = std::str::from_utf8(canonical)
                    .expect("canonical fixture UTF-8")
                    .to_string();
                assert!(text.ends_with("}\n"));
                text.truncate(text.len() - 2);
                text.push_str(",\"unexpected\":true}\n");
                text.into_bytes()
            }
            SharedJobIntegrityCanonicalMutation::SwapVersionAndAudience => {
                let text = std::str::from_utf8(canonical).expect("canonical fixture UTF-8");
                let original = concat!(
                    "{\"version\":1,\"audience\":",
                    "\"bluey-jobs-job-integrity-attestation-v1\","
                );
                let swapped = concat!(
                    "{\"audience\":\"bluey-jobs-job-integrity-attestation-v1\",",
                    "\"version\":1,"
                );
                let mutated = text.replacen(original, swapped, 1);
                assert_ne!(mutated, text);
                mutated.into_bytes()
            }
        }
    }

    fn assert_shared_job_integrity_error<T: std::fmt::Debug>(
        result: JobIntegrityResult<T>,
        expected: SharedJobIntegrityExpectedError,
        name: &str,
    ) {
        let error = result.expect_err(name);
        let matched = matches!(
            (expected, &error),
            (
                SharedJobIntegrityExpectedError::Authority,
                JobIntegrityAuthorityError::InvalidAuthority
            )
            | (
                SharedJobIntegrityExpectedError::Envelope,
                JobIntegrityAuthorityError::InvalidEnvelope
            )
            | (
                SharedJobIntegrityExpectedError::TrustAnchor,
                JobIntegrityAuthorityError::InvalidTrustAnchor
            )
            | (
                SharedJobIntegrityExpectedError::Signature,
                JobIntegrityAuthorityError::InvalidSignature
            )
        );
        assert!(matched, "{name} expected {expected:?}, got {error:?}");
    }

    #[test]
    fn node_and_rust_share_job_integrity_canonical_and_strict_signature_vectors() {
        let fixture: SharedJobIntegrityAuthorityFixture = serde_json::from_str(include_str!(
            "../../../../jobs/automation/tests/fixtures/job-integrity-authority-vectors.json"
        ))
        .expect("shared job-integrity fixture should parse strictly");
        assert_eq!(fixture.schema_version, 1);

        let attestation_bytes = job_integrity_canonical_json(&fixture.attestation)
            .expect("canonical attestation fixture");
        assert_eq!(attestation_bytes.last(), Some(&b'\n'));
        assert_ne!(attestation_bytes.get(attestation_bytes.len() - 2), Some(&b'\n'));
        assert_eq!(
            job_integrity_sha256(&attestation_bytes),
            fixture.attestation_sha256
        );

        let payload_bytes = job_integrity_canonical_json(&fixture.authorization.payload)
            .expect("canonical authorization payload fixture");
        assert_eq!(
            job_integrity_sha256(&payload_bytes),
            fixture.authorization.payload_sha256
        );
        assert_eq!(
            fixture.authorization.payload.target_sha256,
            fixture.attestation_sha256
        );

        let envelope = &fixture.authorization.envelope;
        let production_payload = JobIntegrityAuthorizationPayload {
            version: envelope.version,
            audience: &envelope.audience,
            authorization_id: &envelope.authorization_id,
            role: &envelope.role,
            policy_sha256: &envelope.policy_sha256,
            target_audience: &envelope.target_audience,
            target_sha256: &envelope.target_sha256,
            signed_at_ms: envelope.signed_at_ms,
        };
        assert_eq!(
            job_integrity_canonical_json(&production_payload)
                .expect("production authorization payload"),
            payload_bytes
        );
        let envelope_bytes =
            job_integrity_canonical_json(envelope).expect("canonical authorization envelope fixture");
        assert_eq!(
            job_integrity_sha256(&envelope_bytes),
            fixture.authorization.envelope_sha256
        );

        let detached = envelope.signatures.first().expect("one vector signature");
        assert_eq!(envelope.signatures.len(), 1);
        let role = JobIntegrityTrustRoleV1 {
            threshold: 1,
            keys: BTreeMap::from([(
                detached.key_id.clone(),
                JobIntegrityTrustKeyV1 {
                    public_key_base64url: fixture.authorization.public_key_base64url.clone(),
                    valid_from_ms: envelope.signed_at_ms - 1,
                    expires_at_ms: envelope.signed_at_ms + 1,
                },
            )]),
        };
        let encoded_envelope =
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&envelope_bytes);
        let verified = verify_job_integrity_authorization(JobIntegrityAuthorizationVerification {
            encoded: &encoded_envelope,
            authorization_audience: &fixture.authorization.payload.audience,
            role_name: &fixture.authorization.payload.role,
            role: &role,
            policy_sha256: &fixture.authorization.payload.policy_sha256,
            target_audience: &fixture.authorization.payload.target_audience,
            target_sha256: &fixture.authorization.payload.target_sha256,
            minimum_signed_at_ms: fixture.authorization.payload.signed_at_ms,
            verification_time_ms: fixture.authorization.payload.signed_at_ms,
            maximum_clock_skew_ms: 0,
        })
        .expect("Rust should verify the Node Ed25519 vector strictly");
        assert_eq!(verified.authorization_id, envelope.authorization_id);
        assert_eq!(verified.sha256, fixture.authorization.envelope_sha256);
        assert_eq!(verified.key_ids, vec![detached.key_id.clone()]);

        for vector in &fixture.canonical_rejections {
            let mutated =
                mutate_shared_job_integrity_attestation(&attestation_bytes, vector.mutation);
            assert_shared_job_integrity_error(
                job_integrity_parse_canonical_json::<JobIntegrityAttestationV1>(&mutated),
                vector.expected_error,
                &vector.name,
            );
        }

        for vector in &fixture.key_rejections {
            let anchor = JobIntegrityRootTrustAnchorV1 {
                threshold: 1,
                keys: BTreeMap::from([(
                    format!("{}-vector", vector.name),
                    vector.public_key_base64url.clone(),
                )]),
            };
            assert_shared_job_integrity_error(
                validate_job_integrity_root_anchor(&anchor),
                vector.expected_error,
                &vector.name,
            );
        }

        for vector in &fixture.signature_rejections {
            let mut rejected_envelope = envelope.clone();
            rejected_envelope.signatures[0].signature = vector.signature.clone();
            let rejected_bytes = job_integrity_canonical_json(&rejected_envelope)
                .expect("canonical rejected authorization envelope");
            let rejected_encoded =
                base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(rejected_bytes);
            assert_shared_job_integrity_error(
                verify_job_integrity_authorization(JobIntegrityAuthorizationVerification {
                    encoded: &rejected_encoded,
                    authorization_audience: &fixture.authorization.payload.audience,
                    role_name: &fixture.authorization.payload.role,
                    role: &role,
                    policy_sha256: &fixture.authorization.payload.policy_sha256,
                    target_audience: &fixture.authorization.payload.target_audience,
                    target_sha256: &fixture.authorization.payload.target_sha256,
                    minimum_signed_at_ms: fixture.authorization.payload.signed_at_ms,
                    verification_time_ms: fixture.authorization.payload.signed_at_ms,
                    maximum_clock_skew_ms: 0,
                }),
                vector.expected_error,
                &vector.name,
            );
        }
    }

    #[test]
    fn strict_ed25519_rejects_weak_keys_and_malleable_signature_vectors() {
        let malformed_anchor = JobIntegrityRootTrustAnchorV1 {
            threshold: 1,
            keys: BTreeMap::from([("malformed-root".to_string(), "AAAA".to_string())]),
        };
        assert!(matches!(
            validate_job_integrity_root_anchor(&malformed_anchor),
            Err(JobIntegrityAuthorityError::InvalidTrustAnchor)
        ));
        let weak_key_bytes = {
            let mut bytes = [0_u8; 32];
            bytes[0] = 1;
            bytes
        };
        let weak_key = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(weak_key_bytes);
        let weak_anchor = JobIntegrityRootTrustAnchorV1 {
            threshold: 1,
            keys: BTreeMap::from([("weak-root".to_string(), weak_key.clone())]),
        };
        assert!(matches!(
            validate_job_integrity_root_anchor(&weak_anchor),
            Err(JobIntegrityAuthorityError::InvalidTrustAnchor)
        ));

        let fixture = fixture();
        let mut weak_policy = fixture.policy.clone();
        weak_policy
            .delegated_roles
            .get_mut("employer_identity")
            .unwrap()
            .keys
            .values_mut()
            .next()
            .unwrap()
            .public_key_base64url = weak_key;
        assert!(matches!(
            validate_job_integrity_policy(&weak_policy, &fixture.root_anchor),
            Err(JobIntegrityAuthorityError::InvalidTrustPolicy)
        ));

        let attestation = attestation(
            &fixture,
            "strict-signature",
            'a',
            "verified",
            "clear",
            vec![],
        );
        let package = attestation_package(&fixture, &attestation);
        let mut torsion_r = package.clone();
        let (_, mut authorization) = job_integrity_authorization_from_base64(
            &torsion_r.canonical_employer_identity_authorization_base64url,
        )
        .unwrap();
        let mut signature =
            job_integrity_decode_exact(&authorization.signatures[0].signature, 64).unwrap();
        signature[..32].fill(0);
        signature[0] = 1;
        authorization.signatures[0].signature =
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(signature);
        torsion_r.canonical_employer_identity_authorization_base64url =
            base64::engine::general_purpose::URL_SAFE_NO_PAD
                .encode(job_integrity_canonical_json(&authorization).unwrap());
        assert!(matches!(
            import_job_integrity_attestation(&fixture.pool, &torsion_r, "strict-vector"),
            Err(JobIntegrityAuthorityError::InvalidSignature)
        ));

        let mut noncanonical_s = package;
        let (_, mut authorization) = job_integrity_authorization_from_base64(
            &noncanonical_s.canonical_job_risk_authorization_base64url,
        )
        .unwrap();
        let mut signature =
            job_integrity_decode_exact(&authorization.signatures[0].signature, 64).unwrap();
        const GROUP_ORDER: [u8; 32] = [
            0xed, 0xd3, 0xf5, 0x5c, 0x1a, 0x63, 0x12, 0x58, 0xd6, 0x9c, 0xf7, 0xa2, 0xde, 0xf9,
            0xde, 0x14, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x10,
        ];
        let mut carry = 0_u16;
        for (scalar, order) in signature[32..].iter_mut().zip(GROUP_ORDER) {
            let sum = u16::from(*scalar) + u16::from(order) + carry;
            *scalar = sum as u8;
            carry = sum >> 8;
        }
        authorization.signatures[0].signature =
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(signature);
        noncanonical_s.canonical_job_risk_authorization_base64url =
            base64::engine::general_purpose::URL_SAFE_NO_PAD
                .encode(job_integrity_canonical_json(&authorization).unwrap());
        assert!(matches!(
            import_job_integrity_attestation(&fixture.pool, &noncanonical_s, "strict-vector"),
            Err(JobIntegrityAuthorityError::InvalidSignature)
        ));

        let mut alternate_encoding = attestation_package(&fixture, &attestation);
        let (_, mut authorization) = job_integrity_authorization_from_base64(
            &alternate_encoding.canonical_job_risk_authorization_base64url,
        )
        .unwrap();
        authorization.signatures[0].signature.push('=');
        alternate_encoding.canonical_job_risk_authorization_base64url =
            base64::engine::general_purpose::URL_SAFE_NO_PAD
                .encode(job_integrity_canonical_json(&authorization).unwrap());
        assert!(import_job_integrity_attestation(
            &fixture.pool,
            &alternate_encoding,
            "strict-vector",
        )
        .is_err());
    }

    #[test]
    fn authorization_threshold_role_and_expiry_are_fail_closed() {
        let now_ms = 1_900_000_000_000_i64;
        let first = signing_key(30);
        let second = signing_key(31);
        let role = JobIntegrityTrustRoleV1 {
            threshold: 2,
            keys: BTreeMap::from([
                (
                    "threshold-key-1".to_string(),
                    JobIntegrityTrustKeyV1 {
                        public_key_base64url: base64::engine::general_purpose::URL_SAFE_NO_PAD
                            .encode(first.verifying_key().as_bytes()),
                        valid_from_ms: now_ms - 1_000,
                        expires_at_ms: now_ms + 1_000,
                    },
                ),
                (
                    "threshold-key-2".to_string(),
                    JobIntegrityTrustKeyV1 {
                        public_key_base64url: base64::engine::general_purpose::URL_SAFE_NO_PAD
                            .encode(second.verifying_key().as_bytes()),
                        valid_from_ms: now_ms - 1_000,
                        expires_at_ms: now_ms + 1_000,
                    },
                ),
            ]),
        };
        let policy_sha256 = "1".repeat(64);
        let target_sha256 = "2".repeat(64);
        let one_signature = authorization(
            TestAuthorizationRequest {
                role_name: "employer_identity",
                authorization_audience: JOB_INTEGRITY_AUTHORIZATION_AUDIENCE,
                target_audience: JOB_INTEGRITY_ATTESTATION_AUDIENCE,
                target_sha256: &target_sha256,
                policy_sha256: &policy_sha256,
                signed_at_ms: now_ms,
                authorization_id: "threshold-one",
            },
            "threshold-key-1",
            &first,
        );
        let request = |encoded: &str, role_name: &str, role: &JobIntegrityTrustRoleV1| {
            verify_job_integrity_authorization(JobIntegrityAuthorizationVerification {
                encoded,
                authorization_audience: JOB_INTEGRITY_AUTHORIZATION_AUDIENCE,
                role_name,
                role,
                policy_sha256: &policy_sha256,
                target_audience: JOB_INTEGRITY_ATTESTATION_AUDIENCE,
                target_sha256: &target_sha256,
                minimum_signed_at_ms: now_ms - 10,
                verification_time_ms: now_ms,
                maximum_clock_skew_ms: 0,
            })
        };
        assert!(matches!(
            request(&one_signature, "employer_identity", &role),
            Err(JobIntegrityAuthorityError::ThresholdNotMet)
        ));
        let two_signatures = authorization_with_signers(
            TestAuthorizationRequest {
                role_name: "employer_identity",
                authorization_audience: JOB_INTEGRITY_AUTHORIZATION_AUDIENCE,
                target_audience: JOB_INTEGRITY_ATTESTATION_AUDIENCE,
                target_sha256: &target_sha256,
                policy_sha256: &policy_sha256,
                signed_at_ms: now_ms,
                authorization_id: "threshold-two",
            },
            &[("threshold-key-1", &first), ("threshold-key-2", &second)],
        );
        assert_eq!(
            request(&two_signatures, "employer_identity", &role)
                .unwrap()
                .key_ids
                .len(),
            2
        );
        let wrong_role = authorization(
            TestAuthorizationRequest {
                role_name: "job_risk",
                authorization_audience: JOB_INTEGRITY_AUTHORIZATION_AUDIENCE,
                target_audience: JOB_INTEGRITY_ATTESTATION_AUDIENCE,
                target_sha256: &target_sha256,
                policy_sha256: &policy_sha256,
                signed_at_ms: now_ms,
                authorization_id: "wrong-role",
            },
            "threshold-key-1",
            &first,
        );
        assert!(matches!(
            request(&wrong_role, "employer_identity", &role),
            Err(JobIntegrityAuthorityError::InvalidAuthority)
        ));

        let mut expired_role = role;
        for key in expired_role.keys.values_mut() {
            key.expires_at_ms = now_ms - 1;
        }
        let historical = authorization_with_signers(
            TestAuthorizationRequest {
                role_name: "employer_identity",
                authorization_audience: JOB_INTEGRITY_AUTHORIZATION_AUDIENCE,
                target_audience: JOB_INTEGRITY_ATTESTATION_AUDIENCE,
                target_sha256: &target_sha256,
                policy_sha256: &policy_sha256,
                signed_at_ms: now_ms - 2,
                authorization_id: "expired-authority",
            },
            &[("threshold-key-1", &first), ("threshold-key-2", &second)],
        );
        assert!(
            request(&historical, "employer_identity", &expired_role)
                .unwrap()
                .effective_key_expires_at_ms
                <= now_ms
        );
    }

    #[test]
    fn policy_successors_freeze_root_and_delegated_role_history() {
        let mut valid_fixture = fixture();
        let mut successor = successor_policy(&valid_fixture, "v2-valid");
        let rotated = signing_key(40);
        successor
            .delegated_roles
            .get_mut("employer_identity")
            .unwrap()
            .keys = BTreeMap::from([(
            "employer-identity-key-v2".to_string(),
            JobIntegrityTrustKeyV1 {
                public_key_base64url: base64::engine::general_purpose::URL_SAFE_NO_PAD
                    .encode(rotated.verifying_key().as_bytes()),
                valid_from_ms: successor.valid_from_ms,
                expires_at_ms: successor.expires_at_ms,
            },
        )]);
        let (package, successor_sha256) = policy_package(
            &successor,
            &valid_fixture.key_ids["root"],
            &valid_fixture.keys["root"],
            "authorize-job-integrity-policy-v2-valid",
        );
        let imported = import_job_integrity_trust_policy_with_root(
            &valid_fixture.pool,
            &package,
            &valid_fixture.root_anchor,
            "root-rotation-test",
        )
        .unwrap();
        assert_eq!(imported.generation, 2);
        valid_fixture.policy = successor;
        valid_fixture.policy_sha256 = successor_sha256;
        let third_generation = successor_policy(&valid_fixture, "v3-valid");
        let (third_package, _) = policy_package(
            &third_generation,
            &valid_fixture.key_ids["root"],
            &valid_fixture.keys["root"],
            "authorize-job-integrity-policy-v3-valid",
        );
        assert_eq!(
            import_job_integrity_trust_policy_with_root(
                &valid_fixture.pool,
                &third_package,
                &valid_fixture.root_anchor,
                "root-rotation-test",
            )
            .unwrap()
            .generation,
            3,
        );
        let distinct_roots: i64 = valid_fixture
            .pool
            .get()
            .unwrap()
            .query_row(
                "SELECT count(DISTINCT root_anchor_sha256)
                   FROM jobs_job_integrity_trust_policies",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(distinct_roots, 1);

        let changed_root_fixture = fixture();
        let successor = successor_policy(&changed_root_fixture, "v2-root-change");
        let changed_root_key = signing_key(41);
        let changed_root_id = "root-key-v2";
        let changed_anchor = JobIntegrityRootTrustAnchorV1 {
            threshold: 1,
            keys: BTreeMap::from([(
                changed_root_id.to_string(),
                base64::engine::general_purpose::URL_SAFE_NO_PAD
                    .encode(changed_root_key.verifying_key().as_bytes()),
            )]),
        };
        let (package, _) = policy_package(
            &successor,
            changed_root_id,
            &changed_root_key,
            "authorize-job-integrity-policy-v2-root-change",
        );
        assert!(matches!(
            import_job_integrity_trust_policy_with_root(
                &changed_root_fixture.pool,
                &package,
                &changed_anchor,
                "root-rotation-test",
            ),
            Err(JobIntegrityAuthorityError::InvalidTrustAnchor)
        ));

        let history_fixture = fixture();
        let mut successor = successor_policy(&history_fixture, "v2-role-reuse");
        let new_employer = signing_key(42);
        successor
            .delegated_roles
            .get_mut("employer_identity")
            .unwrap()
            .keys = BTreeMap::from([(
            "employer-identity-key-v2".to_string(),
            JobIntegrityTrustKeyV1 {
                public_key_base64url: base64::engine::general_purpose::URL_SAFE_NO_PAD
                    .encode(new_employer.verifying_key().as_bytes()),
                valid_from_ms: successor.valid_from_ms,
                expires_at_ms: successor.expires_at_ms,
            },
        )]);
        let prior_employer = history_fixture.policy.delegated_roles["employer_identity"]
            .keys
            .clone();
        successor.delegated_roles.get_mut("job_risk").unwrap().keys = prior_employer;
        let (package, _) = policy_package(
            &successor,
            &history_fixture.key_ids["root"],
            &history_fixture.keys["root"],
            "authorize-job-integrity-policy-v2-role-reuse",
        );
        assert!(matches!(
            import_job_integrity_trust_policy_with_root(
                &history_fixture.pool,
                &package,
                &history_fixture.root_anchor,
                "root-rotation-test",
            ),
            Err(JobIntegrityAuthorityError::InvalidTrustPolicy)
        ));
    }

    #[test]
    fn sqlite_policy_waiter_samples_time_after_acquiring_the_writer_lock() {
        let fixture = fixture();
        let start_time_ms = test_now(&fixture.pool);
        let mut successor = successor_policy(&fixture, "v2-lock-expiry");
        successor.issued_at_ms = start_time_ms - 1_000;
        successor.valid_from_ms = start_time_ms - 900;
        successor.expires_at_ms = start_time_ms + 300;
        for role in successor.delegated_roles.values_mut() {
            for key in role.keys.values_mut() {
                key.valid_from_ms = successor.valid_from_ms;
                key.expires_at_ms = successor.expires_at_ms;
            }
        }
        let (package, _) = policy_package(
            &successor,
            &fixture.key_ids["root"],
            &fixture.keys["root"],
            "authorize-job-integrity-policy-v2-lock-expiry",
        );

        let mut lock_connection = fixture.pool.get().unwrap();
        let lock = lock_connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        let worker_pool = fixture.pool.clone();
        let root_anchor = fixture.root_anchor.clone();
        let (ready_sender, ready_receiver) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            ready_sender.send(()).unwrap();
            let started = std::time::Instant::now();
            let result = import_job_integrity_trust_policy_with_root(
                &worker_pool,
                &package,
                &root_anchor,
                "post-lock-time-test",
            );
            (started.elapsed(), result)
        });
        ready_receiver.recv().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(900));
        lock.commit().unwrap();
        let (waited, result) = worker.join().unwrap();
        assert!(waited >= std::time::Duration::from_millis(700));
        assert!(matches!(result, Err(JobIntegrityAuthorityError::Expired)));
        let (policy_count, trust_generation): (i64, i64) = fixture
            .pool
            .get()
            .unwrap()
            .query_row(
                "SELECT count(*), control.current_trust_generation
                   FROM jobs_job_integrity_trust_policies policy
                   CROSS JOIN jobs_job_integrity_control control
                  WHERE control.singleton_id=1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!((policy_count, trust_generation), (1, 1));
    }

    #[test]
    fn authorization_ids_are_role_scoped_persisted_and_collision_checked() {
        let fixture = fixture();
        let first = attestation(&fixture, "role-scope", 'b', "verified", "clear", vec![]);
        let mut first_package = attestation_package(&fixture, &first);
        let first_bytes = job_integrity_canonical_json(&first).unwrap();
        let first_sha256 = job_integrity_sha256(first_bytes);
        let shared_id = "shared-across-disjoint-roles";
        first_package.canonical_employer_identity_authorization_base64url = authorization(
            TestAuthorizationRequest {
                role_name: "employer_identity",
                authorization_audience: JOB_INTEGRITY_AUTHORIZATION_AUDIENCE,
                target_audience: JOB_INTEGRITY_ATTESTATION_AUDIENCE,
                target_sha256: &first_sha256,
                policy_sha256: &fixture.policy_sha256,
                signed_at_ms: first.issued_at_ms,
                authorization_id: shared_id,
            },
            &fixture.key_ids["employer_identity"],
            &fixture.keys["employer_identity"],
        );
        first_package.canonical_job_risk_authorization_base64url = authorization(
            TestAuthorizationRequest {
                role_name: "job_risk",
                authorization_audience: JOB_INTEGRITY_AUTHORIZATION_AUDIENCE,
                target_audience: JOB_INTEGRITY_ATTESTATION_AUDIENCE,
                target_sha256: &first_sha256,
                policy_sha256: &fixture.policy_sha256,
                signed_at_ms: first.issued_at_ms,
                authorization_id: shared_id,
            },
            &fixture.key_ids["job_risk"],
            &fixture.keys["job_risk"],
        );
        import_job_integrity_attestation(&fixture.pool, &first_package, "role-scope-test").unwrap();
        let stored: (String, String) = fixture
            .pool
            .get()
            .unwrap()
            .query_row(
                "SELECT employer_identity_authorization_id,job_risk_authorization_id
                   FROM jobs_job_integrity_attestations WHERE attestation_id=?1",
                params![first.attestation_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(stored, (shared_id.to_string(), shared_id.to_string()));

        let second = attestation(&fixture, "role-collision", 'c', "verified", "clear", vec![]);
        let mut second_package = attestation_package(&fixture, &second);
        let second_sha256 = job_integrity_sha256(job_integrity_canonical_json(&second).unwrap());
        second_package.canonical_employer_identity_authorization_base64url = authorization(
            TestAuthorizationRequest {
                role_name: "employer_identity",
                authorization_audience: JOB_INTEGRITY_AUTHORIZATION_AUDIENCE,
                target_audience: JOB_INTEGRITY_ATTESTATION_AUDIENCE,
                target_sha256: &second_sha256,
                policy_sha256: &fixture.policy_sha256,
                signed_at_ms: second.issued_at_ms,
                authorization_id: shared_id,
            },
            &fixture.key_ids["employer_identity"],
            &fixture.keys["employer_identity"],
        );
        assert!(matches!(
            import_job_integrity_attestation(&fixture.pool, &second_package, "role-scope-test"),
            Err(JobIntegrityAuthorityError::IdentityConflict)
        ));
        let count: i64 = fixture
            .pool
            .get()
            .unwrap()
            .query_row(
                "SELECT count(*) FROM jobs_job_integrity_attestations",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn revoked_old_policy_negative_head_degrades_to_review_required() {
        for (case_index, revocation_kind) in [
            "attestation",
            "trust_policy",
            "identity_evidence",
            "trust_key",
        ]
        .into_iter()
        .enumerate()
        {
            let mut fixture = fixture();
            let negative = attestation(
                &fixture,
                &format!("old-negative-{case_index}"),
                ['d', 'e', 'f', '0'][case_index],
                "verified",
                "blocked",
                vec!["known_scam_signal".to_string()],
            );
            let imported = import_job_integrity_attestation(
                &fixture.pool,
                &attestation_package(&fixture, &negative),
                "old-policy-test",
            )
            .unwrap();
            let old_policy_sha256 = fixture.policy_sha256.clone();
            let old_policy_id = fixture.policy.policy_id.clone();
            let (subject_id, subject_sha256) = match revocation_kind {
                "attestation" => (
                    negative.attestation_id.clone(),
                    imported.object_sha256.clone(),
                ),
                "trust_policy" => (old_policy_id, old_policy_sha256),
                "identity_evidence" => (
                    negative.employer.evidence[0].kind.clone(),
                    negative.employer.evidence[0].sha256.clone(),
                ),
                "trust_key" => {
                    let key_id = fixture.key_ids["employer_identity"].clone();
                    let key = &fixture.policy.delegated_roles["employer_identity"].keys[&key_id];
                    (
                        key_id,
                        job_integrity_sha256(
                            job_integrity_decode_exact(&key.public_key_base64url, 32).unwrap(),
                        ),
                    )
                }
                _ => unreachable!(),
            };
            let successor = successor_policy(&fixture, &format!("v2-old-negative-{case_index}"));
            let (package, successor_sha256) = policy_package(
                &successor,
                &fixture.key_ids["root"],
                &fixture.keys["root"],
                &format!("authorize-job-integrity-policy-v2-old-negative-{case_index}"),
            );
            import_job_integrity_trust_policy_with_root(
                &fixture.pool,
                &package,
                &fixture.root_anchor,
                "old-policy-test",
            )
            .unwrap();
            assert_eq!(
                resolve_current_job_integrity_authority(&fixture.pool, &expected(&negative))
                    .unwrap()
                    .status,
                JobIntegrityResolutionStatus::Blocked
            );
            fixture.policy = successor;
            fixture.policy_sha256 = successor_sha256;
            import_job_integrity_revocation(
                &fixture.pool,
                &revocation_package(
                    &fixture,
                    1,
                    None,
                    revocation_kind,
                    &subject_id,
                    &subject_sha256,
                ),
                "old-policy-test",
            )
            .unwrap();
            let resolved =
                resolve_current_job_integrity_authority(&fixture.pool, &expected(&negative))
                    .unwrap();
            assert_eq!(
                resolved.status,
                JobIntegrityResolutionStatus::ReviewRequired,
                "{revocation_kind} revocation did not degrade the negative head",
            );
            assert_eq!(
                resolved.reason_code,
                "job_integrity_negative_authority_revoked"
            );
        }
    }

    #[test]
    fn canonical_role_domain_and_numeric_validation_is_closed() {
        let fixture = fixture();
        let mut duplicate_key_policy = fixture.policy.clone();
        let duplicate = duplicate_key_policy.delegated_roles["employer_identity"]
            .keys
            .values()
            .next()
            .unwrap()
            .clone();
        duplicate_key_policy
            .delegated_roles
            .get_mut("job_risk")
            .unwrap()
            .keys
            .insert("different-id-same-key".to_string(), duplicate);
        assert!(matches!(
            validate_job_integrity_policy(&duplicate_key_policy, &fixture.root_anchor),
            Err(JobIntegrityAuthorityError::InvalidTrustPolicy)
        ));

        for domain in [
            "127.0.0.1",
            "localhost",
            "jobs.localhost",
            "example.com.",
            "ExAmple.com",
            "éxample.com",
            "xn--.com",
        ] {
            assert!(!job_integrity_domain(domain), "accepted domain {domain}");
        }
        assert!(job_integrity_domain("xn--bcher-kva.example"));

        let mut direct_host = attestation(&fixture, "direct", '7', "verified", "clear", Vec::new());
        direct_host.source.target.host = "jobs.acme.com".to_string();
        direct_host.source.application_domain = "jobs.acme.com".to_string();
        direct_host.source.canonical_application_url =
            "https://jobs.acme.com/acme/jobs/direct".to_string();
        direct_host.employer.canonical_employer_domain = "jobs.acme.com".to_string();
        let bytes = job_integrity_canonical_json(&direct_host).unwrap();
        assert!(validate_job_integrity_attestation(
            &direct_host,
            &fixture.policy,
            bytes.len(),
            fixture.now_ms
        )
        .is_ok());

        let mut unsorted_evidence = direct_host.clone();
        unsorted_evidence.employer.evidence.reverse();
        let bytes = job_integrity_canonical_json(&unsorted_evidence).unwrap();
        assert!(matches!(
            validate_job_integrity_attestation(
                &unsorted_evidence,
                &fixture.policy,
                bytes.len(),
                fixture.now_ms,
            ),
            Err(JobIntegrityAuthorityError::InvalidAuthority)
        ));

        let mut duplicate_evidence = direct_host.clone();
        duplicate_evidence
            .risk
            .evidence
            .insert(1, duplicate_evidence.risk.evidence[0].clone());
        let bytes = job_integrity_canonical_json(&duplicate_evidence).unwrap();
        assert!(matches!(
            validate_job_integrity_attestation(
                &duplicate_evidence,
                &fixture.policy,
                bytes.len(),
                fixture.now_ms,
            ),
            Err(JobIntegrityAuthorityError::InvalidAuthority)
        ));

        for noncanonical_url in [
            "https://jobs.acme.com:443/acme/jobs/direct",
            "https://JOBS.acme.com/acme/jobs/direct",
            "https://jobs.acme.com/acme/../jobs/direct",
            "https://jobs.acme.com/acme/jobs/%64irect",
            "https://jobs.acme.com/acme/jobs/%2fdirect",
            " https://jobs.acme.com/acme/jobs/direct",
            "https://jobs.acme.com/acme/jobs/direct ",
        ] {
            let mut noncanonical = direct_host.clone();
            noncanonical.source.canonical_application_url = noncanonical_url.to_string();
            let bytes = job_integrity_canonical_json(&noncanonical).unwrap();
            assert!(matches!(
                validate_job_integrity_attestation(
                    &noncanonical,
                    &fixture.policy,
                    bytes.len(),
                    fixture.now_ms,
                ),
                Err(JobIntegrityAuthorityError::InvalidAuthority)
            ));
        }

        let mut unsafe_number = direct_host;
        unsafe_number.attestation_generation = JOB_INTEGRITY_MAX_SAFE_INTEGER + 1;
        let bytes = job_integrity_canonical_json(&unsafe_number).unwrap();
        assert!(matches!(
            validate_job_integrity_attestation(
                &unsafe_number,
                &fixture.policy,
                bytes.len(),
                fixture.now_ms
            ),
            Err(JobIntegrityAuthorityError::InvalidAuthority)
        ));

        let canonical = job_integrity_canonical_json(&fixture.policy).unwrap();
        let mut no_newline = canonical.clone();
        no_newline.pop();
        assert!(
            job_integrity_parse_canonical_json::<JobIntegrityTrustPolicyV1>(&no_newline).is_err()
        );
        let unknown = serde_json::json!({"version":1,"unknown":true});
        let unknown = job_integrity_canonical_json(&unknown).unwrap();
        assert!(job_integrity_parse_canonical_json::<JobIntegrityTrustPolicyV1>(&unknown).is_err());
    }

    #[test]
    fn sqlite_migration_replay_and_replace_bypasses_are_rejected() {
        let fixture = fixture();
        let attestation = attestation(&fixture, "replace", '8', "verified", "clear", Vec::new());
        let imported = import_job_integrity_attestation(
            &fixture.pool,
            &attestation_package(&fixture, &attestation),
            "test-integrity-importer",
        )
        .unwrap();
        import_job_integrity_revocation(
            &fixture.pool,
            &revocation_package(
                &fixture,
                1,
                None,
                "attestation",
                &attestation.attestation_id,
                &imported.object_sha256,
            ),
            "test-revocation-operator",
        )
        .unwrap();
        let conn = fixture.pool.get().unwrap();
        conn.execute_batch(crate::db::SQLITE_JOBS_SIGNED_JOB_INTEGRITY_AUTHORITY)
            .expect("job-integrity migration must replay exactly");
        for table in [
            "jobs_job_integrity_trust_policies",
            "jobs_job_integrity_trust_keys",
            "jobs_job_integrity_attestations",
            "jobs_job_integrity_revocations",
            "jobs_job_integrity_head_transitions",
            "jobs_job_integrity_heads",
            "jobs_job_integrity_control",
        ] {
            let sql = format!("INSERT OR REPLACE INTO {table} SELECT * FROM {table} LIMIT 1");
            assert!(
                conn.execute_batch(&sql).is_err(),
                "REPLACE bypassed {table}"
            );
        }
    }
}
