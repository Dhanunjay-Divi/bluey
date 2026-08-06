const BROWSER_RELEASE_MANIFEST_AUDIENCE: &str = "bluey-jobs-browser-release-manifest-v1";
const BROWSER_RELEASE_ACTIVATION_AUDIENCE: &str = "bluey-jobs-browser-release-activation-v1";
const BROWSER_RELEASE_ROLLBACK_AUDIENCE: &str = "bluey-jobs-browser-release-rollback-v1";
const BROWSER_RELEASE_REVOCATION_AUDIENCE: &str = "bluey-jobs-browser-release-revocation-v1";
const BROWSER_RELEASE_TRUST_POLICY_AUDIENCE: &str = "bluey-jobs-browser-release-trust-policy-v1";
const BROWSER_RELEASE_SIGNATURE_SET_AUDIENCE: &str = "bluey-jobs-browser-release-signature-set-v1";
const BROWSER_RELEASE_ROOT_TRUST_ANCHOR_ENV: &str = "BLUEY_JOBS_BROWSER_ROOT_TRUST_ANCHOR_JSON";

const BROWSER_RELEASE_MAX_MANIFEST_BYTES: usize = 32 * 1024;
const BROWSER_RELEASE_MAX_POLICY_BYTES: usize = 64 * 1024;
const BROWSER_RELEASE_MAX_SIGNATURE_SET_BYTES: usize = 32 * 1024;
const BROWSER_RELEASE_MAX_AUTHORITY_BYTES: usize = 16 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BrowserReleaseArtifactAuthority {
    pub artifact_id: String,
    pub platform: String,
    pub architecture: String,
    pub package_kind: String,
    pub build_descriptor_sha256: String,
    pub url: String,
    pub size_bytes: i64,
    pub sha256: String,
    pub app_content_sha256: String,
    pub verification_evidence_sha256: String,
    pub native_signature_kind: String,
    pub native_signer_identity: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BrowserReleaseManifestAuthority {
    pub version: i64,
    pub audience: String,
    pub manifest_id: String,
    pub manifest_generation: i64,
    pub release_id: String,
    pub release_sequence: i64,
    pub build_id: String,
    pub app_version: String,
    pub protocol_version: i64,
    pub source_commit: String,
    pub electron_version: String,
    pub playwright_version: String,
    pub chromium_revision: String,
    pub published_at_ms: i64,
    pub release_notes_url: String,
    pub artifacts: Vec<BrowserReleaseArtifactAuthority>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BrowserReleaseActivationAuthority {
    pub version: i64,
    pub audience: String,
    pub activation_id: String,
    pub activation_generation: i64,
    pub trust_generation: i64,
    pub channel: String,
    pub channel_sequence: i64,
    pub manifest_sha256: String,
    pub signature_set_sha256: String,
    pub accepted_server_release_ids: Vec<String>,
    pub canary_evidence_sha256: String,
    pub issued_at_ms: i64,
    pub expires_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BrowserReleaseRollbackAuthority {
    pub version: i64,
    pub audience: String,
    pub rollback_id: String,
    pub rollback_generation: i64,
    pub trust_generation: i64,
    pub channel: String,
    pub from_activation_sha256: String,
    pub from_manifest_sha256: String,
    pub to_manifest_sha256: String,
    pub to_activation_sha256: String,
    pub canary_evidence_sha256: String,
    pub reason_ref: String,
    pub issued_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BrowserReleaseRevocationAuthority {
    pub version: i64,
    pub audience: String,
    pub revocation_id: String,
    pub revocation_generation: i64,
    pub trust_generation: i64,
    pub subject_kind: String,
    pub subject_id: String,
    pub subject_sha256: String,
    pub reason_ref: String,
    pub issued_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BrowserReleaseTrustRoleAuthority {
    pub role: String,
    pub threshold: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BrowserReleaseTrustKeyAuthority {
    pub key_id: String,
    pub role: String,
    pub public_key: String,
    pub state: String,
    pub valid_from_ms: i64,
    pub valid_until_ms: i64,
    pub minimum_trust_generation: i64,
    pub maximum_trust_generation: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BrowserReleaseTrustPolicyAuthority {
    pub version: i64,
    pub audience: String,
    pub policy_id: String,
    pub trust_generation: i64,
    pub predecessor_policy_sha256: Option<String>,
    pub artifact_origin: String,
    pub issued_at_ms: i64,
    pub valid_from_ms: i64,
    pub expires_at_ms: i64,
    pub roles: Vec<BrowserReleaseTrustRoleAuthority>,
    pub keys: Vec<BrowserReleaseTrustKeyAuthority>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BrowserReleaseDetachedSignatureAuthority {
    pub key_id: String,
    pub signature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BrowserReleaseSignatureSetAuthority {
    pub version: i64,
    pub audience: String,
    pub signature_set_id: String,
    pub trust_generation: i64,
    pub role: String,
    pub target_audience: String,
    pub target_sha256: String,
    pub signed_at_ms: i64,
    pub signatures: Vec<BrowserReleaseDetachedSignatureAuthority>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct BrowserReleaseSignaturePayloadAuthority<'a> {
    version: i64,
    audience: &'a str,
    signature_set_id: &'a str,
    trust_generation: i64,
    role: &'a str,
    target_audience: &'a str,
    target_sha256: &'a str,
    signed_at_ms: i64,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BrowserReleaseRootTrustAnchor {
    pub threshold: i64,
    pub keys: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Copy, Error, PartialEq, Eq)]
pub enum BrowserReleaseTrustError {
    #[error("invalid Browser release manifest")]
    InvalidManifest,
    #[error("invalid Browser release activation")]
    InvalidActivation,
    #[error("invalid Browser release rollback")]
    InvalidRollback,
    #[error("invalid Browser release revocation")]
    InvalidRevocation,
    #[error("invalid Browser release trust policy")]
    InvalidTrustPolicy,
    #[error("invalid Browser release signature set")]
    InvalidSignatureSet,
    #[error("invalid Browser release signature")]
    InvalidSignature,
    #[error("unknown Browser release signing key")]
    UnknownSigningKey,
    #[error("wrong Browser release signing role")]
    WrongSigningRole,
    #[error("Browser release signature threshold not met")]
    SignatureThresholdNotMet,
    #[error("Browser release signing key is not authorized")]
    KeyNotAuthorized,
    #[error("invalid Browser release trust-policy rotation")]
    TrustRotationInvalid,
    #[error("Browser release authority is outside its validity window")]
    AuthorityExpired,
    #[error("Browser release binding mismatch")]
    BindingMismatch,
}

pub fn browser_release_root_trust_anchor_from_environment(
) -> Result<BrowserReleaseRootTrustAnchor, BrowserReleaseTrustError> {
    let raw = std::env::var(BROWSER_RELEASE_ROOT_TRUST_ANCHOR_ENV)
        .map_err(|_| BrowserReleaseTrustError::InvalidTrustPolicy)?;
    let anchor: BrowserReleaseRootTrustAnchor =
        serde_json::from_str(&raw).map_err(|_| BrowserReleaseTrustError::InvalidTrustPolicy)?;
    validate_browser_release_root_trust_anchor(&anchor)?;
    Ok(anchor)
}

pub fn parse_canonical_browser_release_manifest(
    bytes: &[u8],
) -> Result<BrowserReleaseManifestAuthority, BrowserReleaseTrustError> {
    let manifest = parse_canonical_browser_release_json(
        bytes,
        BROWSER_RELEASE_MAX_MANIFEST_BYTES,
        BrowserReleaseTrustError::InvalidManifest,
    )?;
    validate_browser_release_manifest(&manifest)?;
    Ok(manifest)
}

pub fn parse_canonical_browser_release_activation(
    bytes: &[u8],
) -> Result<BrowserReleaseActivationAuthority, BrowserReleaseTrustError> {
    let activation = parse_canonical_browser_release_json(
        bytes,
        BROWSER_RELEASE_MAX_AUTHORITY_BYTES,
        BrowserReleaseTrustError::InvalidActivation,
    )?;
    validate_browser_release_activation(&activation)?;
    Ok(activation)
}

pub fn parse_canonical_browser_release_rollback(
    bytes: &[u8],
) -> Result<BrowserReleaseRollbackAuthority, BrowserReleaseTrustError> {
    let rollback = parse_canonical_browser_release_json(
        bytes,
        BROWSER_RELEASE_MAX_AUTHORITY_BYTES,
        BrowserReleaseTrustError::InvalidRollback,
    )?;
    validate_browser_release_rollback(&rollback)?;
    Ok(rollback)
}

pub fn parse_canonical_browser_release_revocation(
    bytes: &[u8],
) -> Result<BrowserReleaseRevocationAuthority, BrowserReleaseTrustError> {
    let revocation = parse_canonical_browser_release_json(
        bytes,
        BROWSER_RELEASE_MAX_AUTHORITY_BYTES,
        BrowserReleaseTrustError::InvalidRevocation,
    )?;
    validate_browser_release_revocation(&revocation)?;
    Ok(revocation)
}

pub fn parse_canonical_browser_release_trust_policy(
    bytes: &[u8],
) -> Result<BrowserReleaseTrustPolicyAuthority, BrowserReleaseTrustError> {
    let policy = parse_canonical_browser_release_json(
        bytes,
        BROWSER_RELEASE_MAX_POLICY_BYTES,
        BrowserReleaseTrustError::InvalidTrustPolicy,
    )?;
    validate_browser_release_trust_policy(&policy)?;
    Ok(policy)
}

pub fn parse_canonical_browser_release_signature_set(
    bytes: &[u8],
) -> Result<BrowserReleaseSignatureSetAuthority, BrowserReleaseTrustError> {
    let signature_set = parse_canonical_browser_release_json(
        bytes,
        BROWSER_RELEASE_MAX_SIGNATURE_SET_BYTES,
        BrowserReleaseTrustError::InvalidSignatureSet,
    )?;
    validate_browser_release_signature_set(&signature_set)?;
    Ok(signature_set)
}

pub fn browser_release_authority_sha256(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

pub fn browser_release_signature_set_sha256(
    signature_set: &BrowserReleaseSignatureSetAuthority,
) -> Result<String, BrowserReleaseTrustError> {
    Ok(browser_release_authority_sha256(
        &canonical_browser_release_json(signature_set)
            .map_err(|_| BrowserReleaseTrustError::InvalidSignatureSet)?,
    ))
}

pub fn verify_browser_release_trust_policy_authority(
    policy_bytes: &[u8],
    signature_set_bytes: &[u8],
    predecessor_bytes: Option<&[u8]>,
    bootstrap: Option<&BrowserReleaseRootTrustAnchor>,
    verification_time_ms: i64,
) -> Result<BrowserReleaseTrustPolicyAuthority, BrowserReleaseTrustError> {
    let policy = parse_canonical_browser_release_trust_policy(policy_bytes)?;
    let signature_set = parse_canonical_browser_release_signature_set(signature_set_bytes)?;
    require_browser_release_policy_current(&policy, verification_time_ms)?;
    if policy.issued_at_ms > verification_time_ms
        || signature_set.signed_at_ms > verification_time_ms
    {
        return Err(BrowserReleaseTrustError::AuthorityExpired);
    }
    require_browser_release_signature_binding(
        &signature_set,
        "root",
        BROWSER_RELEASE_TRUST_POLICY_AUDIENCE,
        &browser_release_authority_sha256(policy_bytes),
        policy.trust_generation,
        policy.issued_at_ms,
    )?;

    let predecessor = predecessor_bytes
        .map(parse_canonical_browser_release_trust_policy)
        .transpose()?;
    if policy.trust_generation == 1 {
        if predecessor.is_some()
            || bootstrap.is_none()
            || policy.predecessor_policy_sha256.is_some()
        {
            return Err(BrowserReleaseTrustError::TrustRotationInvalid);
        }
    } else {
        let previous = predecessor
            .as_ref()
            .ok_or(BrowserReleaseTrustError::TrustRotationInvalid)?;
        let previous_bytes =
            predecessor_bytes.ok_or(BrowserReleaseTrustError::TrustRotationInvalid)?;
        let previous_sha256 = browser_release_authority_sha256(previous_bytes);
        if policy.trust_generation != previous.trust_generation + 1
            || policy.predecessor_policy_sha256.as_deref() != Some(previous_sha256.as_str())
            || policy.issued_at_ms <= previous.issued_at_ms
            || policy.issued_at_ms < previous.valid_from_ms
            || policy.issued_at_ms >= previous.expires_at_ms
        {
            return Err(BrowserReleaseTrustError::TrustRotationInvalid);
        }
        validate_browser_release_policy_transition(previous, &policy)?;
    }
    verify_browser_release_root_rotation(&signature_set, &policy, predecessor.as_ref(), bootstrap)?;
    Ok(policy)
}

pub fn verify_browser_release_manifest_authority(
    manifest_bytes: &[u8],
    signature_set_bytes: &[u8],
    policy: &BrowserReleaseTrustPolicyAuthority,
    verification_time_ms: i64,
) -> Result<BrowserReleaseManifestAuthority, BrowserReleaseTrustError> {
    let manifest = parse_canonical_browser_release_manifest(manifest_bytes)?;
    let signature_set = parse_canonical_browser_release_signature_set(signature_set_bytes)?;
    verify_browser_release_authority_signature_set(
        manifest_bytes,
        &signature_set,
        policy,
        BrowserReleaseSignatureVerification {
            role: "release",
            target_audience: BROWSER_RELEASE_MANIFEST_AUDIENCE,
            target_issued_at_ms: manifest.published_at_ms,
            target_trust_generation: None,
            verification_time_ms,
        },
    )?;
    require_browser_release_manifest_artifact_origin(&manifest, policy)?;
    Ok(manifest)
}

pub fn verify_browser_release_activation_authority(
    activation_bytes: &[u8],
    signature_set_bytes: &[u8],
    policy: &BrowserReleaseTrustPolicyAuthority,
    verification_time_ms: i64,
) -> Result<BrowserReleaseActivationAuthority, BrowserReleaseTrustError> {
    let activation = parse_canonical_browser_release_activation(activation_bytes)?;
    let signature_set = parse_canonical_browser_release_signature_set(signature_set_bytes)?;
    verify_browser_release_authority_signature_set(
        activation_bytes,
        &signature_set,
        policy,
        BrowserReleaseSignatureVerification {
            role: "promotion",
            target_audience: BROWSER_RELEASE_ACTIVATION_AUDIENCE,
            target_issued_at_ms: activation.issued_at_ms,
            target_trust_generation: Some(activation.trust_generation),
            verification_time_ms,
        },
    )?;
    Ok(activation)
}

pub fn verify_browser_release_rollback_authority(
    rollback_bytes: &[u8],
    signature_set_bytes: &[u8],
    policy: &BrowserReleaseTrustPolicyAuthority,
    verification_time_ms: i64,
) -> Result<BrowserReleaseRollbackAuthority, BrowserReleaseTrustError> {
    let rollback = parse_canonical_browser_release_rollback(rollback_bytes)?;
    let signature_set = parse_canonical_browser_release_signature_set(signature_set_bytes)?;
    verify_browser_release_authority_signature_set(
        rollback_bytes,
        &signature_set,
        policy,
        BrowserReleaseSignatureVerification {
            role: "promotion",
            target_audience: BROWSER_RELEASE_ROLLBACK_AUDIENCE,
            target_issued_at_ms: rollback.issued_at_ms,
            target_trust_generation: Some(rollback.trust_generation),
            verification_time_ms,
        },
    )?;
    Ok(rollback)
}

pub fn verify_browser_release_revocation_authority(
    revocation_bytes: &[u8],
    signature_set_bytes: &[u8],
    policy: &BrowserReleaseTrustPolicyAuthority,
    verification_time_ms: i64,
) -> Result<BrowserReleaseRevocationAuthority, BrowserReleaseTrustError> {
    let revocation = parse_canonical_browser_release_revocation(revocation_bytes)?;
    let signature_set = parse_canonical_browser_release_signature_set(signature_set_bytes)?;
    verify_browser_release_authority_signature_set(
        revocation_bytes,
        &signature_set,
        policy,
        BrowserReleaseSignatureVerification {
            role: "incident",
            target_audience: BROWSER_RELEASE_REVOCATION_AUDIENCE,
            target_issued_at_ms: revocation.issued_at_ms,
            target_trust_generation: Some(revocation.trust_generation),
            verification_time_ms,
        },
    )?;
    require_browser_release_revocation_subject_policy_binding(&revocation, policy)?;
    Ok(revocation)
}

fn require_browser_release_revocation_subject_policy_binding(
    revocation: &BrowserReleaseRevocationAuthority,
    policy: &BrowserReleaseTrustPolicyAuthority,
) -> Result<(), BrowserReleaseTrustError> {
    if revocation.subject_kind != "signing-key" {
        return Ok(());
    }
    for key in policy.keys.iter().filter(|key| key.role == "root") {
        let raw_key = browser_release_decode_base64url_exact(&key.public_key, 32)
            .map_err(|_| BrowserReleaseTrustError::InvalidTrustPolicy)?;
        if key.key_id == revocation.subject_id
            || browser_release_authority_sha256(&raw_key) == revocation.subject_sha256
        {
            return Err(BrowserReleaseTrustError::InvalidRevocation);
        }
    }
    let subject = policy
        .keys
        .iter()
        .find(|key| key.key_id == revocation.subject_id)
        .ok_or(BrowserReleaseTrustError::InvalidRevocation)?;
    let raw_key = browser_release_decode_base64url_exact(&subject.public_key, 32)
        .map_err(|_| BrowserReleaseTrustError::InvalidTrustPolicy)?;
    if subject.role == "root"
        || browser_release_authority_sha256(&raw_key) != revocation.subject_sha256
    {
        return Err(BrowserReleaseTrustError::InvalidRevocation);
    }
    Ok(())
}

pub fn verify_browser_build_proof_against_release_policy(
    proof: &BrowserBuildProof,
    policy: &BrowserReleaseTrustPolicyAuthority,
) -> Result<VerifiedBrowserBuildDescriptor, BrowserReleaseTrustError> {
    let descriptor_bytes =
        browser_release_decode_base64url_bounded(&proof.descriptor, BROWSER_MAX_DESCRIPTOR_BYTES)
            .map_err(|_| BrowserReleaseTrustError::BindingMismatch)?;
    let descriptor = parse_browser_build_descriptor_bytes(&descriptor_bytes)
        .map_err(|_| BrowserReleaseTrustError::BindingMismatch)?;
    let key = policy
        .keys
        .iter()
        .find(|key| key.key_id == descriptor.signing_key_id)
        .ok_or(BrowserReleaseTrustError::UnknownSigningKey)?;
    if key.role != "release" {
        return Err(BrowserReleaseTrustError::WrongSigningRole);
    }
    if key.state != "active"
        || !browser_release_key_authorizes(key, policy.trust_generation, descriptor.issued_at_ms)
    {
        return Err(BrowserReleaseTrustError::KeyNotAuthorized);
    }
    let ring = BrowserBuildVerifyingKeyRing::new(BTreeMap::from([(
        key.key_id.clone(),
        key.public_key.clone(),
    )]))
    .map_err(|_| BrowserReleaseTrustError::InvalidSignature)?;
    verify_browser_build_proof(proof, &ring).map_err(|_| BrowserReleaseTrustError::InvalidSignature)
}

fn parse_canonical_browser_release_json<T>(
    bytes: &[u8],
    maximum_bytes: usize,
    invalid: BrowserReleaseTrustError,
) -> Result<T, BrowserReleaseTrustError>
where
    T: DeserializeOwned + Serialize,
{
    if bytes.is_empty() || bytes.len() > maximum_bytes {
        return Err(invalid);
    }
    let value: T = serde_json::from_slice(bytes).map_err(|_| invalid)?;
    let canonical = canonical_browser_release_json(&value).map_err(|_| invalid)?;
    if canonical != bytes {
        return Err(invalid);
    }
    Ok(value)
}

fn canonical_browser_release_json<T: Serialize>(value: &T) -> serde_json::Result<Vec<u8>> {
    let mut bytes = serde_json::to_vec(value)?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn validate_browser_release_manifest(
    manifest: &BrowserReleaseManifestAuthority,
) -> Result<(), BrowserReleaseTrustError> {
    if manifest.version != 1
        || manifest.audience != BROWSER_RELEASE_MANIFEST_AUDIENCE
        || !browser_release_safe_id(&manifest.manifest_id)
        || !browser_release_safe_integer(manifest.manifest_generation, true)
        || !browser_release_valid_release_id(&manifest.release_id)
        || !browser_release_safe_integer(manifest.release_sequence, true)
        || !browser_release_valid_build_id(&manifest.build_id)
        || !browser_release_valid_semver(&manifest.app_version)
        || !browser_release_safe_integer(manifest.protocol_version, true)
        || !browser_release_valid_source_commit(&manifest.source_commit)
        || !browser_release_valid_semver(&manifest.electron_version)
        || !browser_release_valid_semver(&manifest.playwright_version)
        || !browser_release_valid_decimal_revision(&manifest.chromium_revision)
        || !browser_release_safe_integer(manifest.published_at_ms, false)
        || !browser_release_immutable_url(&manifest.release_notes_url, &manifest.release_id)
        || manifest.artifacts.len() != 5
    {
        return Err(BrowserReleaseTrustError::InvalidManifest);
    }
    let mut targets = Vec::with_capacity(5);
    let mut artifact_ids = BTreeSet::new();
    let mut artifact_hashes = BTreeSet::new();
    let mut artifact_urls = BTreeSet::new();
    let mut descriptor_by_target = BTreeMap::<String, String>::new();
    let mut app_content_by_target = BTreeMap::<String, String>::new();
    for artifact in &manifest.artifacts {
        validate_browser_release_artifact(artifact, &manifest.release_id)?;
        targets.push(format!(
            "{}:{}:{}",
            artifact.platform, artifact.architecture, artifact.package_kind
        ));
        if !artifact_ids.insert(artifact.artifact_id.clone())
            || !artifact_hashes.insert(artifact.sha256.clone())
            || !artifact_urls.insert(artifact.url.clone())
        {
            return Err(BrowserReleaseTrustError::InvalidManifest);
        }
        let target = format!("{}:{}", artifact.platform, artifact.architecture);
        if descriptor_by_target
            .insert(target.clone(), artifact.build_descriptor_sha256.clone())
            .is_some_and(|previous| previous != artifact.build_descriptor_sha256)
        {
            return Err(BrowserReleaseTrustError::InvalidManifest);
        }
        if app_content_by_target
            .insert(target, artifact.app_content_sha256.clone())
            .is_some_and(|previous| previous != artifact.app_content_sha256)
        {
            return Err(BrowserReleaseTrustError::InvalidManifest);
        }
    }
    let expected = [
        "darwin:arm64:darwin-dmg",
        "darwin:arm64:darwin-zip",
        "darwin:x64:darwin-dmg",
        "darwin:x64:darwin-zip",
        "windows:x64:windows-nsis",
    ];
    let descriptor_digests = descriptor_by_target.values().collect::<BTreeSet<_>>();
    if targets.iter().map(String::as_str).ne(expected)
        || descriptor_by_target.len() != 3
        || descriptor_digests.len() != 3
    {
        return Err(BrowserReleaseTrustError::InvalidManifest);
    }
    Ok(())
}

fn validate_browser_release_artifact(
    artifact: &BrowserReleaseArtifactAuthority,
    release_id: &str,
) -> Result<(), BrowserReleaseTrustError> {
    let platform_architecture_valid = matches!(
        (artifact.platform.as_str(), artifact.architecture.as_str()),
        ("darwin", "arm64") | ("darwin", "x64") | ("windows", "x64")
    );
    let package_valid = matches!(
        (
            artifact.platform.as_str(),
            artifact.package_kind.as_str(),
            artifact.native_signature_kind.as_str()
        ),
        ("darwin", "darwin-dmg" | "darwin-zip", "apple-developer-id")
            | ("windows", "windows-nsis", "microsoft-authenticode")
    );
    if !browser_release_safe_id(&artifact.artifact_id)
        || !platform_architecture_valid
        || !package_valid
        || !browser_release_hex64(&artifact.build_descriptor_sha256)
        || !browser_release_immutable_artifact_url(&artifact.url, release_id)
        || !browser_release_artifact_url_matches_package_kind(
            &artifact.url,
            &artifact.package_kind,
        )
        || !browser_release_safe_integer(artifact.size_bytes, true)
        || !browser_release_hex64(&artifact.sha256)
        || !browser_release_hex64(&artifact.app_content_sha256)
        || !browser_release_hex64(&artifact.verification_evidence_sha256)
        || !browser_release_bounded_text(&artifact.native_signer_identity, 3, 256)
    {
        return Err(BrowserReleaseTrustError::InvalidManifest);
    }
    Ok(())
}

fn validate_browser_release_activation(
    activation: &BrowserReleaseActivationAuthority,
) -> Result<(), BrowserReleaseTrustError> {
    if activation.version != 1
        || activation.audience != BROWSER_RELEASE_ACTIVATION_AUDIENCE
        || !browser_release_safe_id(&activation.activation_id)
        || !browser_release_safe_integer(activation.activation_generation, true)
        || !browser_release_safe_integer(activation.trust_generation, true)
        || !matches!(activation.channel.as_str(), "internal" | "beta" | "stable")
        || !browser_release_safe_integer(activation.channel_sequence, true)
        || !browser_release_hex64(&activation.manifest_sha256)
        || !browser_release_hex64(&activation.signature_set_sha256)
        || !browser_release_sorted_safe_ids(&activation.accepted_server_release_ids, 1, 32)
        || !browser_release_hex64(&activation.canary_evidence_sha256)
        || !browser_release_safe_integer(activation.issued_at_ms, false)
        || !browser_release_safe_integer(activation.expires_at_ms, false)
        || activation.expires_at_ms <= activation.issued_at_ms
    {
        return Err(BrowserReleaseTrustError::InvalidActivation);
    }
    Ok(())
}

fn validate_browser_release_rollback(
    rollback: &BrowserReleaseRollbackAuthority,
) -> Result<(), BrowserReleaseTrustError> {
    if rollback.version != 1
        || rollback.audience != BROWSER_RELEASE_ROLLBACK_AUDIENCE
        || !browser_release_safe_id(&rollback.rollback_id)
        || !browser_release_safe_integer(rollback.rollback_generation, true)
        || !browser_release_safe_integer(rollback.trust_generation, true)
        || !matches!(rollback.channel.as_str(), "internal" | "beta" | "stable")
        || !browser_release_hex64(&rollback.from_activation_sha256)
        || !browser_release_hex64(&rollback.from_manifest_sha256)
        || !browser_release_hex64(&rollback.to_manifest_sha256)
        || !browser_release_hex64(&rollback.to_activation_sha256)
        || rollback.from_activation_sha256 == rollback.to_activation_sha256
        || rollback.from_manifest_sha256 == rollback.to_manifest_sha256
        || !browser_release_hex64(&rollback.canary_evidence_sha256)
        || !browser_release_safe_id(&rollback.reason_ref)
        || !browser_release_safe_integer(rollback.issued_at_ms, false)
    {
        return Err(BrowserReleaseTrustError::InvalidRollback);
    }
    Ok(())
}

fn validate_browser_release_revocation(
    revocation: &BrowserReleaseRevocationAuthority,
) -> Result<(), BrowserReleaseTrustError> {
    if revocation.version != 1
        || revocation.audience != BROWSER_RELEASE_REVOCATION_AUDIENCE
        || !browser_release_safe_id(&revocation.revocation_id)
        || !browser_release_safe_integer(revocation.revocation_generation, true)
        || !browser_release_safe_integer(revocation.trust_generation, true)
        || !matches!(
            revocation.subject_kind.as_str(),
            "artifact" | "build-descriptor" | "manifest" | "release" | "signing-key"
        )
        || !browser_release_safe_id(&revocation.subject_id)
        || !browser_release_hex64(&revocation.subject_sha256)
        || !browser_release_safe_id(&revocation.reason_ref)
        || !browser_release_safe_integer(revocation.issued_at_ms, false)
    {
        return Err(BrowserReleaseTrustError::InvalidRevocation);
    }
    Ok(())
}

fn validate_browser_release_trust_policy(
    policy: &BrowserReleaseTrustPolicyAuthority,
) -> Result<(), BrowserReleaseTrustError> {
    if policy.version != 1
        || policy.audience != BROWSER_RELEASE_TRUST_POLICY_AUDIENCE
        || !browser_release_safe_id(&policy.policy_id)
        || !browser_release_safe_integer(policy.trust_generation, true)
        || policy
            .predecessor_policy_sha256
            .as_ref()
            .is_some_and(|digest| !browser_release_hex64(digest))
        || !browser_release_valid_artifact_origin(&policy.artifact_origin)
        || !browser_release_safe_integer(policy.issued_at_ms, false)
        || !browser_release_safe_integer(policy.valid_from_ms, false)
        || !browser_release_safe_integer(policy.expires_at_ms, false)
        || policy.valid_from_ms > policy.issued_at_ms
        || policy.issued_at_ms >= policy.expires_at_ms
        || (policy.trust_generation == 1) != policy.predecessor_policy_sha256.is_none()
        || policy.roles.len() != 4
        || policy.keys.len() < 4
        || policy.keys.len() > 64
    {
        return Err(BrowserReleaseTrustError::InvalidTrustPolicy);
    }
    let expected_roles = ["incident", "promotion", "release", "root"];
    if policy
        .roles
        .iter()
        .map(|role| role.role.as_str())
        .ne(expected_roles)
    {
        return Err(BrowserReleaseTrustError::InvalidTrustPolicy);
    }
    for role in &policy.roles {
        if !browser_release_safe_integer(role.threshold, true) {
            return Err(BrowserReleaseTrustError::InvalidTrustPolicy);
        }
    }
    let mut previous_key_id: Option<&str> = None;
    for key in &policy.keys {
        if previous_key_id.is_some_and(|previous| previous >= key.key_id.as_str())
            || !browser_release_safe_id(&key.key_id)
            || !expected_roles.contains(&key.role.as_str())
            || browser_release_decode_base64url_exact(&key.public_key, 32).is_err()
            || !matches!(key.state.as_str(), "active" | "retired" | "revoked")
            || !browser_release_safe_integer(key.valid_from_ms, false)
            || !browser_release_safe_integer(key.valid_until_ms, false)
            || key.valid_until_ms < key.valid_from_ms
            || !browser_release_safe_integer(key.minimum_trust_generation, true)
            || !browser_release_safe_integer(key.maximum_trust_generation, true)
            || key.maximum_trust_generation < key.minimum_trust_generation
        {
            return Err(BrowserReleaseTrustError::InvalidTrustPolicy);
        }
        previous_key_id = Some(&key.key_id);
    }
    for role in &policy.roles {
        let active = policy
            .keys
            .iter()
            .filter(|key| {
                key.role == role.role
                    && key.state == "active"
                    && browser_release_key_authorizes(
                        key,
                        policy.trust_generation,
                        policy.issued_at_ms,
                    )
            })
            .count();
        if active < role.threshold as usize {
            return Err(BrowserReleaseTrustError::InvalidTrustPolicy);
        }
    }
    Ok(())
}

fn validate_browser_release_signature_set(
    signature_set: &BrowserReleaseSignatureSetAuthority,
) -> Result<(), BrowserReleaseTrustError> {
    if signature_set.version != 1
        || signature_set.audience != BROWSER_RELEASE_SIGNATURE_SET_AUDIENCE
        || !browser_release_safe_id(&signature_set.signature_set_id)
        || !browser_release_safe_integer(signature_set.trust_generation, true)
        || !matches!(
            signature_set.role.as_str(),
            "incident" | "promotion" | "release" | "root"
        )
        || !matches!(
            signature_set.target_audience.as_str(),
            BROWSER_RELEASE_ACTIVATION_AUDIENCE
                | BROWSER_RELEASE_MANIFEST_AUDIENCE
                | BROWSER_RELEASE_REVOCATION_AUDIENCE
                | BROWSER_RELEASE_ROLLBACK_AUDIENCE
                | BROWSER_RELEASE_TRUST_POLICY_AUDIENCE
        )
        || !browser_release_hex64(&signature_set.target_sha256)
        || !browser_release_safe_integer(signature_set.signed_at_ms, false)
        || signature_set.signatures.is_empty()
        || signature_set.signatures.len() > 32
    {
        return Err(BrowserReleaseTrustError::InvalidSignatureSet);
    }
    let mut previous_key_id: Option<&str> = None;
    for signature in &signature_set.signatures {
        if previous_key_id.is_some_and(|previous| previous >= signature.key_id.as_str())
            || !browser_release_safe_id(&signature.key_id)
            || browser_release_decode_base64url_exact(&signature.signature, 64).is_err()
        {
            return Err(BrowserReleaseTrustError::InvalidSignatureSet);
        }
        previous_key_id = Some(&signature.key_id);
    }
    Ok(())
}

#[derive(Debug, Clone, Copy)]
struct BrowserReleaseSignatureVerification<'a> {
    role: &'a str,
    target_audience: &'a str,
    target_issued_at_ms: i64,
    target_trust_generation: Option<i64>,
    verification_time_ms: i64,
}

fn verify_browser_release_authority_signature_set(
    target_bytes: &[u8],
    signature_set: &BrowserReleaseSignatureSetAuthority,
    policy: &BrowserReleaseTrustPolicyAuthority,
    verification: BrowserReleaseSignatureVerification<'_>,
) -> Result<(), BrowserReleaseTrustError> {
    require_browser_release_policy_current(policy, verification.verification_time_ms)?;
    if verification.target_issued_at_ms > verification.verification_time_ms
        || signature_set.signed_at_ms > verification.verification_time_ms
    {
        return Err(BrowserReleaseTrustError::AuthorityExpired);
    }
    if let Some(generation) = verification.target_trust_generation {
        if generation != policy.trust_generation || signature_set.trust_generation != generation {
            return Err(BrowserReleaseTrustError::KeyNotAuthorized);
        }
    } else if signature_set.trust_generation > policy.trust_generation {
        return Err(BrowserReleaseTrustError::KeyNotAuthorized);
    }
    require_browser_release_signature_binding(
        signature_set,
        verification.role,
        verification.target_audience,
        &browser_release_authority_sha256(target_bytes),
        signature_set.trust_generation,
        verification.target_issued_at_ms,
    )?;
    let threshold = policy
        .roles
        .iter()
        .find(|candidate| candidate.role == verification.role)
        .ok_or(BrowserReleaseTrustError::WrongSigningRole)?
        .threshold;
    let key_map = policy
        .keys
        .iter()
        .map(|key| (key.key_id.as_str(), key))
        .collect::<BTreeMap<_, _>>();
    let message = canonical_browser_release_signature_payload(signature_set)?;
    let mut authorized = 0_i64;
    for detached in &signature_set.signatures {
        let key = key_map
            .get(detached.key_id.as_str())
            .ok_or(BrowserReleaseTrustError::UnknownSigningKey)?;
        if key.role != verification.role {
            return Err(BrowserReleaseTrustError::WrongSigningRole);
        }
        if key.state != "active"
            || !browser_release_key_authorizes(
                key,
                signature_set.trust_generation,
                signature_set.signed_at_ms,
            )
        {
            return Err(BrowserReleaseTrustError::KeyNotAuthorized);
        }
        verify_browser_release_detached_signature(&key.public_key, &message, &detached.signature)?;
        authorized += 1;
    }
    if authorized < threshold {
        return Err(BrowserReleaseTrustError::SignatureThresholdNotMet);
    }
    Ok(())
}

fn require_browser_release_signature_binding(
    signature_set: &BrowserReleaseSignatureSetAuthority,
    role: &str,
    target_audience: &str,
    target_sha256: &str,
    trust_generation: i64,
    signed_at_ms: i64,
) -> Result<(), BrowserReleaseTrustError> {
    if signature_set.role != role
        || signature_set.target_audience != target_audience
        || signature_set.target_sha256 != target_sha256
        || signature_set.trust_generation != trust_generation
        || signature_set.signed_at_ms != signed_at_ms
    {
        return Err(BrowserReleaseTrustError::InvalidSignatureSet);
    }
    Ok(())
}

fn verify_browser_release_root_rotation(
    signature_set: &BrowserReleaseSignatureSetAuthority,
    successor: &BrowserReleaseTrustPolicyAuthority,
    predecessor: Option<&BrowserReleaseTrustPolicyAuthority>,
    bootstrap: Option<&BrowserReleaseRootTrustAnchor>,
) -> Result<(), BrowserReleaseTrustError> {
    let successor_roots = browser_release_eligible_roots(
        successor,
        signature_set.trust_generation,
        signature_set.signed_at_ms,
    );
    let (predecessor_threshold, predecessor_roots) = if let Some(previous) = predecessor {
        let threshold = previous
            .roles
            .iter()
            .find(|role| role.role == "root")
            .ok_or(BrowserReleaseTrustError::WrongSigningRole)?
            .threshold;
        (
            threshold,
            browser_release_eligible_roots(
                previous,
                signature_set.trust_generation,
                signature_set.signed_at_ms,
            ),
        )
    } else {
        let anchor = bootstrap.ok_or(BrowserReleaseTrustError::TrustRotationInvalid)?;
        validate_browser_release_root_trust_anchor(anchor)?;
        (anchor.threshold, anchor.keys.clone())
    };
    let successor_threshold = successor
        .roles
        .iter()
        .find(|role| role.role == "root")
        .ok_or(BrowserReleaseTrustError::WrongSigningRole)?
        .threshold;
    for (key_id, predecessor_key) in &predecessor_roots {
        if successor_roots
            .get(key_id)
            .is_some_and(|successor_key| successor_key != predecessor_key)
        {
            return Err(BrowserReleaseTrustError::TrustRotationInvalid);
        }
    }
    let allowed = predecessor_roots
        .iter()
        .chain(successor_roots.iter())
        .map(|(key_id, key)| (key_id.as_str(), key.as_str()))
        .collect::<BTreeMap<_, _>>();
    let message = canonical_browser_release_signature_payload(signature_set)?;
    let mut signed = BTreeSet::new();
    for detached in &signature_set.signatures {
        let key = allowed
            .get(detached.key_id.as_str())
            .ok_or(BrowserReleaseTrustError::KeyNotAuthorized)?;
        verify_browser_release_detached_signature(key, &message, &detached.signature)?;
        signed.insert(detached.key_id.as_str());
    }
    let predecessor_count = predecessor_roots
        .keys()
        .filter(|key_id| signed.contains(key_id.as_str()))
        .count() as i64;
    let successor_count = successor_roots
        .keys()
        .filter(|key_id| signed.contains(key_id.as_str()))
        .count() as i64;
    if predecessor_count < predecessor_threshold || successor_count < successor_threshold {
        return Err(BrowserReleaseTrustError::SignatureThresholdNotMet);
    }
    Ok(())
}

fn validate_browser_release_policy_transition(
    predecessor: &BrowserReleaseTrustPolicyAuthority,
    successor: &BrowserReleaseTrustPolicyAuthority,
) -> Result<(), BrowserReleaseTrustError> {
    let successor_keys = successor
        .keys
        .iter()
        .map(|key| (key.key_id.as_str(), key))
        .collect::<BTreeMap<_, _>>();
    for previous in &predecessor.keys {
        let next = successor_keys
            .get(previous.key_id.as_str())
            .ok_or(BrowserReleaseTrustError::TrustRotationInvalid)?;
        let state_valid = match previous.state.as_str() {
            "revoked" => next.state == "revoked",
            "retired" => next.state != "active",
            "active" => true,
            _ => false,
        };
        if next.public_key != previous.public_key
            || next.role != previous.role
            || next.valid_from_ms != previous.valid_from_ms
            || next.valid_until_ms > previous.valid_until_ms
            || next.minimum_trust_generation != previous.minimum_trust_generation
            || next.maximum_trust_generation > previous.maximum_trust_generation
            || !state_valid
        {
            return Err(BrowserReleaseTrustError::TrustRotationInvalid);
        }
    }
    Ok(())
}

fn browser_release_eligible_roots(
    policy: &BrowserReleaseTrustPolicyAuthority,
    trust_generation: i64,
    signed_at_ms: i64,
) -> BTreeMap<String, String> {
    policy
        .keys
        .iter()
        .filter(|key| {
            key.role == "root"
                && key.state == "active"
                && browser_release_key_authorizes(key, trust_generation, signed_at_ms)
        })
        .map(|key| (key.key_id.clone(), key.public_key.clone()))
        .collect()
}

fn validate_browser_release_root_trust_anchor(
    anchor: &BrowserReleaseRootTrustAnchor,
) -> Result<(), BrowserReleaseTrustError> {
    if anchor.threshold < 1
        || anchor.threshold > 32
        || anchor.keys.len() < anchor.threshold as usize
        || anchor.keys.len() > 32
        || anchor.keys.iter().any(|(key_id, key)| {
            !browser_release_safe_id(key_id)
                || browser_release_decode_base64url_exact(key, 32).is_err()
        })
    {
        return Err(BrowserReleaseTrustError::InvalidTrustPolicy);
    }
    Ok(())
}

fn require_browser_release_policy_current(
    policy: &BrowserReleaseTrustPolicyAuthority,
    verification_time_ms: i64,
) -> Result<(), BrowserReleaseTrustError> {
    if !browser_release_safe_integer(verification_time_ms, false)
        || verification_time_ms < policy.valid_from_ms
        || verification_time_ms >= policy.expires_at_ms
    {
        return Err(BrowserReleaseTrustError::AuthorityExpired);
    }
    Ok(())
}

fn canonical_browser_release_signature_payload(
    signature_set: &BrowserReleaseSignatureSetAuthority,
) -> Result<Vec<u8>, BrowserReleaseTrustError> {
    canonical_browser_release_json(&BrowserReleaseSignaturePayloadAuthority {
        version: signature_set.version,
        audience: &signature_set.audience,
        signature_set_id: &signature_set.signature_set_id,
        trust_generation: signature_set.trust_generation,
        role: &signature_set.role,
        target_audience: &signature_set.target_audience,
        target_sha256: &signature_set.target_sha256,
        signed_at_ms: signature_set.signed_at_ms,
    })
    .map_err(|_| BrowserReleaseTrustError::InvalidSignatureSet)
}

fn verify_browser_release_detached_signature(
    public_key_base64url: &str,
    message: &[u8],
    signature_base64url: &str,
) -> Result<(), BrowserReleaseTrustError> {
    let public_key = browser_release_decode_base64url_exact(public_key_base64url, 32)
        .map_err(|_| BrowserReleaseTrustError::InvalidSignature)?;
    let public_key: [u8; 32] = public_key
        .try_into()
        .map_err(|_| BrowserReleaseTrustError::InvalidSignature)?;
    let verifying_key = ed25519_dalek::VerifyingKey::from_bytes(&public_key)
        .map_err(|_| BrowserReleaseTrustError::InvalidSignature)?;
    let signature = browser_release_decode_base64url_exact(signature_base64url, 64)
        .map_err(|_| BrowserReleaseTrustError::InvalidSignature)?;
    let signature: [u8; 64] = signature
        .try_into()
        .map_err(|_| BrowserReleaseTrustError::InvalidSignature)?;
    use ed25519_dalek::Verifier as _;
    verifying_key
        .verify(message, &ed25519_dalek::Signature::from_bytes(&signature))
        .map_err(|_| BrowserReleaseTrustError::InvalidSignature)
}

fn browser_release_key_authorizes(
    key: &BrowserReleaseTrustKeyAuthority,
    trust_generation: i64,
    signed_at_ms: i64,
) -> bool {
    trust_generation >= key.minimum_trust_generation
        && trust_generation <= key.maximum_trust_generation
        && signed_at_ms >= key.valid_from_ms
        && signed_at_ms <= key.valid_until_ms
}

fn browser_release_safe_integer(value: i64, positive: bool) -> bool {
    value >= i64::from(positive) && value <= BROWSER_MAX_SAFE_INTEGER as i64
}

fn browser_release_hex64(value: &str) -> bool {
    value.len() == 64
        && value == value.to_ascii_lowercase()
        && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn browser_release_bounded_text(value: &str, minimum: usize, maximum: usize) -> bool {
    let bytes = value.as_bytes();
    bytes.len() >= minimum
        && bytes.len() <= maximum
        && value.trim() == value
        && !value
            .chars()
            .any(|character| matches!(character, '\0'..='\u{1f}' | '\u{7f}'))
}

fn browser_release_valid_build_id(value: &str) -> bool {
    let Some(rest) = value.strip_prefix("browser-") else {
        return false;
    };
    let Some((generation, revision)) = rest.split_once('.') else {
        return false;
    };
    browser_release_canonical_decimal(generation, 9)
        && browser_release_canonical_decimal(revision, 9)
}

fn browser_release_valid_semver(value: &str) -> bool {
    let parts = value.split('.').collect::<Vec<_>>();
    parts.len() == 3
        && parts
            .iter()
            .all(|part| browser_release_canonical_decimal(part, 9))
}

fn browser_release_valid_decimal_revision(value: &str) -> bool {
    browser_release_canonical_decimal(value, 13)
}

fn browser_release_canonical_decimal(value: &str, maximum_digits: usize) -> bool {
    !value.is_empty()
        && value.len() <= maximum_digits
        && value.bytes().all(|byte| byte.is_ascii_digit())
        && (value == "0" || !value.starts_with('0'))
}

fn browser_release_valid_source_commit(value: &str) -> bool {
    value.len() == 40
        && value == value.to_ascii_lowercase()
        && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn browser_release_sorted_safe_ids(values: &[String], minimum: usize, maximum: usize) -> bool {
    values.len() >= minimum
        && values.len() <= maximum
        && values.iter().all(|value| browser_release_safe_id(value))
        && values.windows(2).all(|pair| pair[0] < pair[1])
}

fn browser_release_valid_release_id(value: &str) -> bool {
    browser_release_safe_id(value)
        && ![
            "beta", "current", "download", "internal", "latest", "stable",
        ]
        .iter()
        .any(|reserved| value.eq_ignore_ascii_case(reserved))
}

fn browser_release_immutable_url(value: &str, release_id: &str) -> bool {
    if !browser_release_bounded_text(value, 1, 2_048) {
        return false;
    }
    let Ok(url) = reqwest::Url::parse(value) else {
        return false;
    };
    let segments = url.path().split('/').collect::<Vec<_>>();
    url.scheme() == "https"
        && url.host_str().is_some_and(|host| !host.is_empty())
        && url.username().is_empty()
        && url.password().is_none()
        && url.query().is_none()
        && url.fragment().is_none()
        && url.as_str() == value
        && segments.len() >= 6
        && segments[..5] == ["", "jobs", "browser", "releases", release_id]
        && segments[5..].iter().all(|segment| {
            !segment.is_empty() && *segment != "." && *segment != ".." && !segment.contains('%')
        })
}

fn browser_release_immutable_artifact_url(value: &str, release_id: &str) -> bool {
    browser_release_immutable_url(value, release_id)
        && reqwest::Url::parse(value).is_ok_and(|url| url.path().split('/').count() == 6)
}

fn browser_release_artifact_url_matches_package_kind(value: &str, package_kind: &str) -> bool {
    let expected_extension = match package_kind {
        "darwin-dmg" => "dmg",
        "darwin-zip" => "zip",
        "windows-nsis" => "exe",
        _ => return false,
    };
    reqwest::Url::parse(value)
        .ok()
        .and_then(|url| url.path_segments()?.next_back().map(str::to_string))
        .and_then(|filename| {
            filename
                .rsplit_once('.')
                .map(|(stem, extension)| !stem.is_empty() && extension == expected_extension)
        })
        .unwrap_or(false)
}

fn browser_release_valid_artifact_origin(value: &str) -> bool {
    if !browser_release_bounded_text(value, 1, 2_048) {
        return false;
    }
    let Ok(url) = reqwest::Url::parse(value) else {
        return false;
    };
    url.scheme() == "https"
        && url.host_str().is_some_and(|host| !host.is_empty())
        && url.username().is_empty()
        && url.password().is_none()
        && url.port().is_none()
        && url.path() == "/"
        && url.query().is_none()
        && url.fragment().is_none()
        && url.origin().ascii_serialization() == value
}

pub fn require_browser_release_manifest_artifact_origin(
    manifest: &BrowserReleaseManifestAuthority,
    policy: &BrowserReleaseTrustPolicyAuthority,
) -> Result<(), BrowserReleaseTrustError> {
    if manifest.artifacts.iter().all(|artifact| {
        reqwest::Url::parse(&artifact.url)
            .is_ok_and(|url| url.origin().ascii_serialization() == policy.artifact_origin)
    }) {
        Ok(())
    } else {
        Err(BrowserReleaseTrustError::BindingMismatch)
    }
}

#[cfg(test)]
mod browser_release_trust_tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    #[derive(Debug, Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct SignedAuthorityFixture {
        canonical: String,
        sha256: String,
        signature_set: String,
        signature_set_sha256: String,
    }

    fn deterministic_signing_key(index: usize) -> SigningKey {
        let seed = std::array::from_fn(|offset| ((index * 37 + offset) % 256) as u8);
        SigningKey::from_bytes(&seed)
    }

    fn signed_signing_key_revocation(
        subject_id: &str,
        subject_key_index: usize,
        issued_at_ms: i64,
    ) -> (Vec<u8>, Vec<u8>) {
        let revocation = BrowserReleaseRevocationAuthority {
            version: 1,
            audience: BROWSER_RELEASE_REVOCATION_AUDIENCE.to_string(),
            revocation_id: format!("browser-revocation-{subject_id}"),
            revocation_generation: 1,
            trust_generation: 1,
            subject_kind: "signing-key".to_string(),
            subject_id: subject_id.to_string(),
            subject_sha256: browser_release_authority_sha256(
                &deterministic_signing_key(subject_key_index)
                    .verifying_key()
                    .to_bytes(),
            ),
            reason_ref: "incident-BR-root-boundary".to_string(),
            issued_at_ms,
        };
        let revocation_bytes = canonical_browser_release_json(&revocation).unwrap();
        let mut signature_set = BrowserReleaseSignatureSetAuthority {
            version: 1,
            audience: BROWSER_RELEASE_SIGNATURE_SET_AUDIENCE.to_string(),
            signature_set_id: format!("browser-revocation-{subject_id}-signatures"),
            trust_generation: 1,
            role: "incident".to_string(),
            target_audience: BROWSER_RELEASE_REVOCATION_AUDIENCE.to_string(),
            target_sha256: browser_release_authority_sha256(&revocation_bytes),
            signed_at_ms: issued_at_ms,
            signatures: Vec::new(),
        };
        let payload = canonical_browser_release_signature_payload(&signature_set).unwrap();
        signature_set
            .signatures
            .push(BrowserReleaseDetachedSignatureAuthority {
                key_id: "incident-key-1".to_string(),
                signature: base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(
                    deterministic_signing_key(1)
                        .sign(&payload)
                        .to_bytes(),
                ),
            });
        (
            revocation_bytes,
            canonical_browser_release_json(&signature_set).unwrap(),
        )
    }

    #[derive(Debug, Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct ReleaseAuthorityFixture {
        descriptor: String,
        signature: String,
        descriptor_sha256: String,
        manifest: SignedAuthorityFixture,
        activation: SignedAuthorityFixture,
        rollback: SignedAuthorityFixture,
        revocation: SignedAuthorityFixture,
        trust_policy: SignedAuthorityFixture,
    }

    fn fixture() -> ReleaseAuthorityFixture {
        serde_json::from_str(include_str!(
            "../../../../jobs/browser/fixtures/release-authority-v1.json"
        ))
        .expect("parse shared Browser release fixture")
    }

    fn decode(encoded: &str) -> Vec<u8> {
        base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(encoded)
            .expect("decode canonical fixture")
    }

    fn bootstrap(policy: &BrowserReleaseTrustPolicyAuthority) -> BrowserReleaseRootTrustAnchor {
        BrowserReleaseRootTrustAnchor {
            threshold: policy
                .roles
                .iter()
                .find(|role| role.role == "root")
                .expect("root role")
                .threshold,
            keys: policy
                .keys
                .iter()
                .filter(|key| key.role == "root")
                .map(|key| (key.key_id.clone(), key.public_key.clone()))
                .collect(),
        }
    }

    #[test]
    fn rust_verifies_all_node_release_authority_vectors() {
        let fixture = fixture();
        let policy_bytes = decode(&fixture.trust_policy.canonical);
        let policy_set_bytes = decode(&fixture.trust_policy.signature_set);
        let parsed_policy = parse_canonical_browser_release_trust_policy(&policy_bytes).unwrap();
        let anchor = bootstrap(&parsed_policy);
        let policy = verify_browser_release_trust_policy_authority(
            &policy_bytes,
            &policy_set_bytes,
            None,
            Some(&anchor),
            parsed_policy.issued_at_ms,
        )
        .unwrap();
        assert_eq!(
            browser_release_authority_sha256(&policy_bytes),
            fixture.trust_policy.sha256
        );
        assert_eq!(
            browser_release_authority_sha256(&policy_set_bytes),
            fixture.trust_policy.signature_set_sha256
        );
        let build_proof = BrowserBuildProof {
            descriptor: fixture.descriptor.clone(),
            signature: fixture.signature.clone(),
        };
        let verified_build =
            verify_browser_build_proof_against_release_policy(&build_proof, &policy).unwrap();
        assert_eq!(verified_build.descriptor_sha256, fixture.descriptor_sha256);

        let mut retired_policy = policy.clone();
        retired_policy
            .keys
            .iter_mut()
            .find(|key| key.key_id == verified_build.signing_key_id)
            .expect("descriptor release key")
            .state = "retired".to_string();
        assert_eq!(
            verify_browser_build_proof_against_release_policy(&build_proof, &retired_policy,),
            Err(BrowserReleaseTrustError::KeyNotAuthorized)
        );

        let manifest_bytes = decode(&fixture.manifest.canonical);
        let manifest_set_bytes = decode(&fixture.manifest.signature_set);
        let parsed_manifest = parse_canonical_browser_release_manifest(&manifest_bytes).unwrap();
        let manifest = verify_browser_release_manifest_authority(
            &manifest_bytes,
            &manifest_set_bytes,
            &policy,
            parsed_manifest.published_at_ms,
        )
        .unwrap();
        assert_eq!(manifest.artifacts.len(), 5);
        assert_eq!(
            browser_release_authority_sha256(&manifest_bytes),
            fixture.manifest.sha256
        );
        assert_eq!(
            browser_release_authority_sha256(&manifest_set_bytes),
            fixture.manifest.signature_set_sha256
        );

        let mut backdated_retired_policy = policy.clone();
        backdated_retired_policy.trust_generation = 2;
        backdated_retired_policy.issued_at_ms = manifest.published_at_ms + 1_000;
        backdated_retired_policy.valid_from_ms = backdated_retired_policy.issued_at_ms;
        for key in &mut backdated_retired_policy.keys {
            if key.role == "release" {
                key.state = "retired".to_string();
            }
        }
        assert_eq!(
            verify_browser_build_proof_against_release_policy(
                &build_proof,
                &backdated_retired_policy,
            ),
            Err(BrowserReleaseTrustError::KeyNotAuthorized)
        );
        assert_eq!(
            verify_browser_release_manifest_authority(
                &manifest_bytes,
                &manifest_set_bytes,
                &backdated_retired_policy,
                backdated_retired_policy.issued_at_ms,
            ),
            Err(BrowserReleaseTrustError::KeyNotAuthorized)
        );

        let activation_bytes = decode(&fixture.activation.canonical);
        let activation_set_bytes = decode(&fixture.activation.signature_set);
        let parsed_activation =
            parse_canonical_browser_release_activation(&activation_bytes).unwrap();
        let activation = verify_browser_release_activation_authority(
            &activation_bytes,
            &activation_set_bytes,
            &policy,
            parsed_activation.issued_at_ms,
        )
        .unwrap();
        assert_eq!(activation.manifest_sha256, fixture.manifest.sha256);
        assert_eq!(
            activation.signature_set_sha256,
            fixture.manifest.signature_set_sha256
        );
        assert_eq!(
            browser_release_authority_sha256(&activation_bytes),
            fixture.activation.sha256
        );

        let rollback_bytes = decode(&fixture.rollback.canonical);
        let rollback_set_bytes = decode(&fixture.rollback.signature_set);
        let parsed_rollback = parse_canonical_browser_release_rollback(&rollback_bytes).unwrap();
        verify_browser_release_rollback_authority(
            &rollback_bytes,
            &rollback_set_bytes,
            &policy,
            parsed_rollback.issued_at_ms,
        )
        .unwrap();
        assert_eq!(
            browser_release_authority_sha256(&rollback_bytes),
            fixture.rollback.sha256
        );

        let revocation_bytes = decode(&fixture.revocation.canonical);
        let revocation_set_bytes = decode(&fixture.revocation.signature_set);
        let parsed_revocation =
            parse_canonical_browser_release_revocation(&revocation_bytes).unwrap();
        let revocation = verify_browser_release_revocation_authority(
            &revocation_bytes,
            &revocation_set_bytes,
            &policy,
            parsed_revocation.issued_at_ms,
        )
        .unwrap();
        assert_eq!(revocation.subject_kind, "release");
        assert_eq!(
            browser_release_authority_sha256(&revocation_bytes),
            fixture.revocation.sha256
        );
    }

    #[test]
    fn trust_rotation_without_predecessor_fails_closed_without_panicking() {
        let fixture = fixture();
        let predecessor_bytes = decode(&fixture.trust_policy.canonical);
        let mut successor =
            parse_canonical_browser_release_trust_policy(&predecessor_bytes).unwrap();
        successor.policy_id = "browser-trust-policy-missing-predecessor-2".to_string();
        successor.trust_generation = 2;
        successor.predecessor_policy_sha256 =
            Some(browser_release_authority_sha256(&predecessor_bytes));
        successor.issued_at_ms += 500_000;
        let successor_bytes = canonical_browser_release_json(&successor).unwrap();
        let mut signatures = BrowserReleaseSignatureSetAuthority {
            version: 1,
            audience: BROWSER_RELEASE_SIGNATURE_SET_AUDIENCE.to_string(),
            signature_set_id: "browser-trust-policy-missing-predecessor-signatures-2".to_string(),
            trust_generation: 2,
            role: "root".to_string(),
            target_audience: BROWSER_RELEASE_TRUST_POLICY_AUDIENCE.to_string(),
            target_sha256: browser_release_authority_sha256(&successor_bytes),
            signed_at_ms: successor.issued_at_ms,
            signatures: Vec::new(),
        };
        let payload = canonical_browser_release_signature_payload(&signatures).unwrap();
        for (key_id, key_index) in [("root-key-1", 10_usize), ("root-key-2", 11_usize)] {
            signatures
                .signatures
                .push(BrowserReleaseDetachedSignatureAuthority {
                    key_id: key_id.to_string(),
                    signature: base64::engine::general_purpose::URL_SAFE_NO_PAD
                        .encode(deterministic_signing_key(key_index).sign(&payload).to_bytes()),
                });
        }
        let signature_bytes = canonical_browser_release_json(&signatures).unwrap();

        assert_eq!(
            verify_browser_release_trust_policy_authority(
                &successor_bytes,
                &signature_bytes,
                None,
                None,
                successor.issued_at_ms,
            ),
            Err(BrowserReleaseTrustError::TrustRotationInvalid)
        );
    }

    #[test]
    fn rust_pins_canonical_artifact_origin_and_exact_release_path_segments() {
        let fixture = fixture();
        let policy_bytes = decode(&fixture.trust_policy.canonical);
        let policy = parse_canonical_browser_release_trust_policy(&policy_bytes).unwrap();
        let manifest_bytes = decode(&fixture.manifest.canonical);
        let manifest = parse_canonical_browser_release_manifest(&manifest_bytes).unwrap();
        assert_eq!(policy.artifact_origin, "https://bluey.sh");
        require_browser_release_manifest_artifact_origin(&manifest, &policy).unwrap();

        let mut different_current_origin = policy.clone();
        different_current_origin.artifact_origin = "https://artifacts.example".to_string();
        assert_eq!(
            verify_browser_release_manifest_authority(
                &manifest_bytes,
                &decode(&fixture.manifest.signature_set),
                &different_current_origin,
                manifest.published_at_ms,
            ),
            Err(BrowserReleaseTrustError::BindingMismatch)
        );

        for invalid_origin in [
            "http://bluey.sh",
            "https://bluey.sh:443",
            "https://user@bluey.sh",
            "https://bluey.sh/",
            "https://bluey.sh/path",
            "https://bluey.sh?mirror=1",
            "https://bluey.sh#mirror",
        ] {
            let mut invalid_policy = policy.clone();
            invalid_policy.artifact_origin = invalid_origin.to_string();
            let invalid_bytes = canonical_browser_release_json(&invalid_policy).unwrap();
            assert_eq!(
                parse_canonical_browser_release_trust_policy(&invalid_bytes),
                Err(BrowserReleaseTrustError::InvalidTrustPolicy)
            );
        }

        for host in ["artifacts.example", "bluey.sh.evil.example"] {
            let mut foreign_manifest = manifest.clone();
            foreign_manifest.artifacts[0].url = foreign_manifest.artifacts[0]
                .url
                .replacen("bluey.sh", host, 1);
            assert_eq!(
                require_browser_release_manifest_artifact_origin(&foreign_manifest, &policy),
                Err(BrowserReleaseTrustError::BindingMismatch)
            );
        }

        let valid_url = manifest.artifacts[0].url.clone();
        for confusing_url in [
            valid_url.replacen("/jobs/", "/mirror/jobs/", 1),
            valid_url.replacen("/browser-release-603-1/", "/%62rowser-release-603-1/", 1),
            valid_url.replacen("/browser-release-603-1/", "/browser-release-603-1//", 1),
            valid_url.replacen("darwin-arm64-dmg.dmg", "darwin%2Farm64-dmg.dmg", 1),
            valid_url.replacen("darwin-arm64-dmg.dmg", "nested/darwin-arm64-dmg.dmg", 1),
        ] {
            let mut confusing_manifest = manifest.clone();
            confusing_manifest.artifacts[0].url = confusing_url;
            let confusing_bytes = canonical_browser_release_json(&confusing_manifest).unwrap();
            assert_eq!(
                parse_canonical_browser_release_manifest(&confusing_bytes),
                Err(BrowserReleaseTrustError::InvalidManifest)
            );
        }

        for wrong_filename in ["darwin-arm64-dmg.zip", "darwin-arm64-dmg.DMG"] {
            let mut wrong_extension = manifest.clone();
            wrong_extension.artifacts[0].url = wrong_extension.artifacts[0]
                .url
                .replace("darwin-arm64-dmg.dmg", wrong_filename);
            let bytes = canonical_browser_release_json(&wrong_extension).unwrap();
            assert_eq!(
                parse_canonical_browser_release_manifest(&bytes),
                Err(BrowserReleaseTrustError::InvalidManifest)
            );
        }

        let mut split_macos_app = manifest.clone();
        assert_eq!(
            (
                split_macos_app.artifacts[0].platform.as_str(),
                split_macos_app.artifacts[0].architecture.as_str()
            ),
            ("darwin", "arm64")
        );
        assert_eq!(
            (
                split_macos_app.artifacts[1].platform.as_str(),
                split_macos_app.artifacts[1].architecture.as_str()
            ),
            ("darwin", "arm64")
        );
        split_macos_app.artifacts[1].app_content_sha256 = "f".repeat(64);
        let split_macos_app_bytes = canonical_browser_release_json(&split_macos_app).unwrap();
        assert_eq!(
            parse_canonical_browser_release_manifest(&split_macos_app_bytes),
            Err(BrowserReleaseTrustError::InvalidManifest)
        );

        let mut duplicate_artifact_url = manifest.clone();
        duplicate_artifact_url.artifacts[2].url =
            duplicate_artifact_url.artifacts[0].url.clone();
        let duplicate_artifact_url_bytes =
            canonical_browser_release_json(&duplicate_artifact_url).unwrap();
        assert_eq!(
            parse_canonical_browser_release_manifest(&duplicate_artifact_url_bytes),
            Err(BrowserReleaseTrustError::InvalidManifest)
        );

        let mut nested_release_notes = manifest.clone();
        nested_release_notes.release_notes_url = nested_release_notes
            .release_notes_url
            .replace("RELEASE.md", "notes/RELEASE.md");
        let nested_release_notes_bytes =
            canonical_browser_release_json(&nested_release_notes).unwrap();
        assert!(parse_canonical_browser_release_manifest(&nested_release_notes_bytes).is_ok());
    }

    #[test]
    fn rust_revocation_verifier_requires_exact_delegated_key_and_rejects_root_authority() {
        let fixture = fixture();
        let policy = parse_canonical_browser_release_trust_policy(&decode(
            &fixture.trust_policy.canonical,
        ))
        .unwrap();
        let issued_at_ms = policy.issued_at_ms + 300_001;

        let (delegated, delegated_signatures) =
            signed_signing_key_revocation("promotion-key-1", 2, issued_at_ms);
        assert!(verify_browser_release_revocation_authority(
            &delegated,
            &delegated_signatures,
            &policy,
            issued_at_ms,
        )
        .is_ok());

        for (subject_id, subject_key_index) in
            [("unknown-key-1", 2_usize), ("promotion-key-1", 3_usize)]
        {
            let (invalid, invalid_signatures) = signed_signing_key_revocation(
                subject_id,
                subject_key_index,
                issued_at_ms,
            );
            assert_eq!(
                verify_browser_release_revocation_authority(
                    &invalid,
                    &invalid_signatures,
                    &policy,
                    issued_at_ms,
                ),
                Err(BrowserReleaseTrustError::InvalidRevocation)
            );
        }

        let (root_id, root_id_signatures) =
            signed_signing_key_revocation("root-key-1", 10, issued_at_ms);
        assert_eq!(
            verify_browser_release_revocation_authority(
                &root_id,
                &root_id_signatures,
                &policy,
                issued_at_ms,
            ),
            Err(BrowserReleaseTrustError::InvalidRevocation)
        );

        let (root_material, root_material_signatures) =
            signed_signing_key_revocation("promotion-key-1", 10, issued_at_ms);
        assert_eq!(
            verify_browser_release_revocation_authority(
                &root_material,
                &root_material_signatures,
                &policy,
                issued_at_ms,
            ),
            Err(BrowserReleaseTrustError::InvalidRevocation)
        );
    }

    #[test]
    fn rust_rejects_case_insensitive_mutable_release_path_identities() {
        let fixture = fixture();
        let manifest_bytes = decode(&fixture.manifest.canonical);
        let manifest = parse_canonical_browser_release_manifest(&manifest_bytes).unwrap();
        for release_id in [
            "beta", "current", "download", "internal", "latest", "stable", "LATEST", "Stable",
        ] {
            let mut reserved = manifest.clone();
            reserved.release_notes_url = reserved
                .release_notes_url
                .replace(&reserved.release_id, release_id);
            for artifact in &mut reserved.artifacts {
                artifact.url = artifact.url.replace(&reserved.release_id, release_id);
            }
            reserved.release_id = release_id.to_string();
            let reserved_bytes = canonical_browser_release_json(&reserved).unwrap();
            assert_eq!(
                parse_canonical_browser_release_manifest(&reserved_bytes),
                Err(BrowserReleaseTrustError::InvalidManifest)
            );
        }
    }

    #[test]
    fn rust_rejects_noncanonical_tampered_and_cross_audience_authority() {
        let fixture = fixture();
        let policy_bytes = decode(&fixture.trust_policy.canonical);
        let policy_set_bytes = decode(&fixture.trust_policy.signature_set);
        let parsed_policy = parse_canonical_browser_release_trust_policy(&policy_bytes).unwrap();
        let policy = verify_browser_release_trust_policy_authority(
            &policy_bytes,
            &policy_set_bytes,
            None,
            Some(&bootstrap(&parsed_policy)),
            parsed_policy.issued_at_ms,
        )
        .unwrap();

        let mut manifest_bytes = decode(&fixture.manifest.canonical);
        manifest_bytes.insert(1, b' ');
        assert_eq!(
            parse_canonical_browser_release_manifest(&manifest_bytes),
            Err(BrowserReleaseTrustError::InvalidManifest)
        );

        let activation_bytes = decode(&fixture.activation.canonical);
        let activation = parse_canonical_browser_release_activation(&activation_bytes).unwrap();
        let manifest_set_bytes = decode(&fixture.manifest.signature_set);
        assert_eq!(
            verify_browser_release_activation_authority(
                &activation_bytes,
                &manifest_set_bytes,
                &policy,
                activation.issued_at_ms,
            ),
            Err(BrowserReleaseTrustError::InvalidSignatureSet)
        );

        let manifest_bytes = decode(&fixture.manifest.canonical);
        let mut manifest_set_bytes = decode(&fixture.manifest.signature_set);
        let signature = manifest_set_bytes
            .iter_mut()
            .rev()
            .find(|byte| **byte != b'\n' && **byte != b'}')
            .expect("signature-set byte");
        *signature = if *signature == b'A' { b'B' } else { b'A' };
        assert!(verify_browser_release_manifest_authority(
            &manifest_bytes,
            &manifest_set_bytes,
            &policy,
            policy.issued_at_ms,
        )
        .is_err());

        let mut reused_ids = bootstrap(&parsed_policy);
        let root_ids = reused_ids.keys.keys().cloned().collect::<Vec<_>>();
        assert_eq!(root_ids.len(), 2);
        let first = reused_ids.keys[&root_ids[0]].clone();
        let second = reused_ids.keys[&root_ids[1]].clone();
        reused_ids.keys.insert(root_ids[0].clone(), second);
        reused_ids.keys.insert(root_ids[1].clone(), first);
        assert_eq!(
            verify_browser_release_trust_policy_authority(
                &policy_bytes,
                &policy_set_bytes,
                None,
                Some(&reused_ids),
                parsed_policy.issued_at_ms,
            ),
            Err(BrowserReleaseTrustError::TrustRotationInvalid)
        );
    }

    #[test]
    fn rust_rejects_future_dated_release_authority() {
        let fixture = fixture();
        let policy_bytes = decode(&fixture.trust_policy.canonical);
        let policy_set_bytes = decode(&fixture.trust_policy.signature_set);
        let parsed_policy = parse_canonical_browser_release_trust_policy(&policy_bytes).unwrap();
        assert_eq!(
            verify_browser_release_trust_policy_authority(
                &policy_bytes,
                &policy_set_bytes,
                None,
                Some(&bootstrap(&parsed_policy)),
                parsed_policy.issued_at_ms - 1,
            ),
            Err(BrowserReleaseTrustError::AuthorityExpired)
        );

        let policy = verify_browser_release_trust_policy_authority(
            &policy_bytes,
            &policy_set_bytes,
            None,
            Some(&bootstrap(&parsed_policy)),
            parsed_policy.issued_at_ms,
        )
        .unwrap();
        let manifest_bytes = decode(&fixture.manifest.canonical);
        let manifest_set_bytes = decode(&fixture.manifest.signature_set);
        let manifest = parse_canonical_browser_release_manifest(&manifest_bytes).unwrap();
        assert_eq!(
            verify_browser_release_manifest_authority(
                &manifest_bytes,
                &manifest_set_bytes,
                &policy,
                manifest.published_at_ms - 1,
            ),
            Err(BrowserReleaseTrustError::AuthorityExpired)
        );

        let mut future_signature_set =
            parse_canonical_browser_release_signature_set(&manifest_set_bytes).unwrap();
        future_signature_set.signed_at_ms = manifest.published_at_ms + 1;
        let future_signature_set_bytes =
            canonical_browser_release_json(&future_signature_set).unwrap();
        assert_eq!(
            verify_browser_release_manifest_authority(
                &manifest_bytes,
                &future_signature_set_bytes,
                &policy,
                manifest.published_at_ms,
            ),
            Err(BrowserReleaseTrustError::AuthorityExpired)
        );

        let activation_bytes = decode(&fixture.activation.canonical);
        let activation_set_bytes = decode(&fixture.activation.signature_set);
        let activation = parse_canonical_browser_release_activation(&activation_bytes).unwrap();
        assert_eq!(
            verify_browser_release_activation_authority(
                &activation_bytes,
                &activation_set_bytes,
                &policy,
                activation.issued_at_ms - 1,
            ),
            Err(BrowserReleaseTrustError::AuthorityExpired)
        );

        let rollback_bytes = decode(&fixture.rollback.canonical);
        let rollback_set_bytes = decode(&fixture.rollback.signature_set);
        let rollback = parse_canonical_browser_release_rollback(&rollback_bytes).unwrap();
        assert_eq!(
            verify_browser_release_rollback_authority(
                &rollback_bytes,
                &rollback_set_bytes,
                &policy,
                rollback.issued_at_ms - 1,
            ),
            Err(BrowserReleaseTrustError::AuthorityExpired)
        );

        let revocation_bytes = decode(&fixture.revocation.canonical);
        let revocation_set_bytes = decode(&fixture.revocation.signature_set);
        let revocation = parse_canonical_browser_release_revocation(&revocation_bytes).unwrap();
        assert_eq!(
            verify_browser_release_revocation_authority(
                &revocation_bytes,
                &revocation_set_bytes,
                &policy,
                revocation.issued_at_ms - 1,
            ),
            Err(BrowserReleaseTrustError::AuthorityExpired)
        );
    }
}
