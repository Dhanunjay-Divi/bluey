const BROWSER_RELEASE_MAX_ENVELOPE_BYTES: usize = 64 * 1024;
const BROWSER_RELEASE_TRANSITION_AUDIENCE: &str =
    "bluey-jobs-browser-release-channel-transition-v1";
const BROWSER_RELEASE_ASSIGNMENT_AUDIENCE: &str =
    "bluey-jobs-browser-account-channel-assignment-v1";

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BrowserReleaseAuthorityEnvelope {
    pub canonical_base64url: String,
    pub signature_set_base64url: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BrowserReleaseManifestImportRequest {
    pub canonical_base64url: String,
    pub signature_set_base64url: String,
    pub build_proofs: Vec<BrowserBuildProof>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ApplyBrowserReleaseActivationRequest {
    pub activation_sha256: String,
    pub expected_head_revision: i64,
    pub expected_transition_sha256: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AssignBrowserReleaseChannelRequest {
    pub assignment_generation: i64,
    pub predecessor_assignment_sha256: Option<String>,
    pub channel: String,
    pub reason_ref: String,
    pub assigned_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BrowserReleaseImportResult {
    pub authority_kind: String,
    pub authority_id: String,
    pub authority_sha256: String,
    pub signature_set_sha256: String,
    pub trust_policy_sha256: String,
    pub manifest_authorization_signature_set_sha256: Option<String>,
    pub activation_authorization_signature_set_sha256: Option<String>,
    pub replayed: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BrowserReleaseChannelStatus {
    pub channel: String,
    pub available: bool,
    pub unavailability_reason: Option<String>,
    pub trust_policy_sha256: Option<String>,
    pub trust_generation: Option<i64>,
    pub head_revision: i64,
    pub transition_sha256: Option<String>,
    pub activation_sha256: Option<String>,
    pub activation_authorization_signature_set_sha256: Option<String>,
    pub manifest_sha256: Option<String>,
    pub manifest_authorization_signature_set_sha256: Option<String>,
    pub channel_sequence: Option<i64>,
    pub release_sequence: Option<i64>,
    pub release_id: Option<String>,
    pub build_id: Option<String>,
    pub app_version: Option<String>,
    pub protocol_version: Option<i64>,
    pub activation_expires_at_ms: Option<i64>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BrowserReleaseAccountChannelAssignment {
    pub assignment_sha256: String,
    pub account_id: String,
    pub assignment_generation: i64,
    pub predecessor_assignment_sha256: Option<String>,
    pub channel: String,
    pub reason_ref: String,
    pub assigned_by: String,
    pub assigned_at_ms: i64,
    pub replayed: bool,
}

#[derive(Debug, Error)]
pub enum BrowserReleaseRegistryError {
    #[error("invalid Browser release authority envelope")]
    InvalidEnvelope,
    #[error("invalid Browser release authority")]
    InvalidAuthority,
    #[error("invalid Browser release registry request")]
    InvalidRequest,
    #[error("Browser release authority was not found")]
    NotFound,
    #[error("Browser release identity conflicts with stored authority")]
    IdentityConflict,
    #[error("Browser release compare-and-swap failed")]
    CompareAndSwapConflict,
    #[error("Browser release sequence regressed")]
    SequenceRegression,
    #[error("Browser release downgrade requires rollback authority")]
    DowngradeRequiresRollback,
    #[error("Browser release authority is revoked")]
    Revoked,
    #[error("Browser release storage failed: {0}")]
    Storage(#[source] anyhow::Error),
}

impl From<BrowserReleaseTrustError> for BrowserReleaseRegistryError {
    fn from(_: BrowserReleaseTrustError) -> Self {
        Self::InvalidAuthority
    }
}

fn browser_release_registry_storage(
    error: impl Into<anyhow::Error>,
) -> BrowserReleaseRegistryError {
    BrowserReleaseRegistryError::Storage(error.into())
}

fn decode_browser_release_authority_envelope(
    envelope: &BrowserReleaseAuthorityEnvelope,
) -> Result<(Vec<u8>, Vec<u8>), BrowserReleaseRegistryError> {
    Ok((
        decode_browser_release_registry_base64url(&envelope.canonical_base64url)?,
        decode_browser_release_registry_base64url(&envelope.signature_set_base64url)?,
    ))
}

fn decode_browser_release_registry_base64url(
    value: &str,
) -> Result<Vec<u8>, BrowserReleaseRegistryError> {
    if value.is_empty() || value.len() > BROWSER_RELEASE_MAX_ENVELOPE_BYTES * 2 {
        return Err(BrowserReleaseRegistryError::InvalidEnvelope);
    }
    let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(value)
        .map_err(|_| BrowserReleaseRegistryError::InvalidEnvelope)?;
    if decoded.is_empty()
        || decoded.len() > BROWSER_RELEASE_MAX_ENVELOPE_BYTES
        || base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&decoded) != value
    {
        return Err(BrowserReleaseRegistryError::InvalidEnvelope);
    }
    Ok(decoded)
}

fn browser_release_registry_channel(value: &str) -> bool {
    matches!(value, "internal" | "beta" | "stable")
}

fn browser_release_registry_sha256(value: &str) -> bool {
    value.len() == 64
        && value == value.to_ascii_lowercase()
        && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn browser_release_registry_positive_integer(value: i64) -> bool {
    (1..=9_007_199_254_740_991).contains(&value)
}

fn browser_release_registry_non_negative_integer(value: i64) -> bool {
    (0..=9_007_199_254_740_991).contains(&value)
}

fn browser_release_registry_text(value: &str, maximum_bytes: usize) -> bool {
    !value.is_empty()
        && value.len() <= maximum_bytes
        && value.trim() == value
        && !value.chars().any(char::is_control)
}

fn validate_browser_release_recorded_by(
    recorded_by: &str,
) -> Result<(), BrowserReleaseRegistryError> {
    if !browser_release_registry_text(recorded_by, 128) {
        return Err(BrowserReleaseRegistryError::InvalidRequest);
    }
    Ok(())
}

fn browser_release_registry_digest<T: Serialize>(
    value: &T,
) -> Result<String, BrowserReleaseRegistryError> {
    let mut bytes = serde_json::to_vec(value).map_err(browser_release_registry_storage)?;
    bytes.push(b'\n');
    Ok(browser_release_authority_sha256(&bytes))
}

#[derive(Debug, Clone)]
struct StoredBrowserReleaseChannelHead {
    channel: String,
    head_revision: i64,
    transition_sha256: String,
    activation_sha256: String,
    manifest_sha256: String,
    trust_generation: i64,
    channel_sequence: i64,
}

#[derive(Debug, Clone)]
struct BrowserReleaseChannelTransition {
    transition_sha256: String,
    channel: String,
    head_revision: i64,
    previous_head_revision: i64,
    previous_transition_sha256: Option<String>,
    previous_activation_sha256: Option<String>,
    previous_manifest_sha256: Option<String>,
    previous_trust_generation: Option<i64>,
    previous_channel_sequence: Option<i64>,
    next_activation_sha256: String,
    next_manifest_sha256: String,
    next_trust_generation: i64,
    next_channel_sequence: i64,
    transition_kind: String,
    authority_sha256: String,
    rollback_authority_sha256: Option<String>,
    recorded_by: String,
    recorded_at_ms: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BrowserReleaseChannelTransitionDigest<'a> {
    version: i64,
    audience: &'static str,
    channel: &'a str,
    head_revision: i64,
    previous_head_revision: i64,
    previous_transition_sha256: Option<&'a str>,
    previous_activation_sha256: Option<&'a str>,
    previous_manifest_sha256: Option<&'a str>,
    previous_trust_generation: Option<i64>,
    previous_channel_sequence: Option<i64>,
    next_activation_sha256: &'a str,
    next_manifest_sha256: &'a str,
    next_trust_generation: i64,
    next_channel_sequence: i64,
    transition_kind: &'a str,
    authority_sha256: &'a str,
    rollback_authority_sha256: Option<&'a str>,
    recorded_at_ms: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BrowserReleaseAssignmentDigest<'a> {
    version: i64,
    audience: &'static str,
    account_id: &'a str,
    assignment_generation: i64,
    predecessor_assignment_sha256: Option<&'a str>,
    channel: &'a str,
    reason_ref: &'a str,
    assigned_by: &'a str,
    assigned_at_ms: i64,
}

#[derive(Debug, Clone)]
struct StoredBrowserReleaseRollback {
    rollback_sha256: String,
    rollback_id: String,
    rollback_generation: i64,
    trust_generation: i64,
    channel: String,
    from_activation_sha256: String,
    from_manifest_sha256: String,
    to_activation_sha256: String,
    to_manifest_sha256: String,
    canonical_rollback_base64url: String,
    authorization_signature_set_sha256: String,
}

#[derive(Debug, Clone)]
struct StoredBrowserReleaseRevocation {
    revocation_sha256: String,
    revocation_id: String,
    revocation_generation: i64,
    trust_generation: i64,
    subject_kind: String,
    subject_id: String,
    subject_sha256: String,
    canonical_revocation_base64url: String,
    authorization_signature_set_sha256: String,
}

#[derive(Debug, Clone)]
struct StoredBrowserReleaseAssignment {
    assignment_sha256: String,
    account_id: String,
    assignment_generation: i64,
    predecessor_assignment_sha256: Option<String>,
    channel: String,
    reason_ref: String,
    assigned_by: String,
    assigned_at_ms: i64,
}

impl BrowserReleaseChannelTransition {
    fn new(
        previous: Option<&StoredBrowserReleaseChannelHead>,
        activation: &StoredBrowserReleaseActivation,
        transition_kind: &str,
        authority_sha256: &str,
        rollback_authority_sha256: Option<&str>,
        recorded_by: &str,
        recorded_at_ms: i64,
    ) -> Result<Self, BrowserReleaseRegistryError> {
        let previous_head_revision = previous.map_or(0, |head| head.head_revision);
        let head_revision = previous_head_revision
            .checked_add(1)
            .filter(|value| browser_release_registry_positive_integer(*value))
            .ok_or(BrowserReleaseRegistryError::SequenceRegression)?;
        let mut transition = Self {
            transition_sha256: String::new(),
            channel: activation.channel.clone(),
            head_revision,
            previous_head_revision,
            previous_transition_sha256: previous.map(|head| head.transition_sha256.clone()),
            previous_activation_sha256: previous.map(|head| head.activation_sha256.clone()),
            previous_manifest_sha256: previous.map(|head| head.manifest_sha256.clone()),
            previous_trust_generation: previous.map(|head| head.trust_generation),
            previous_channel_sequence: previous.map(|head| head.channel_sequence),
            next_activation_sha256: activation.activation_sha256.clone(),
            next_manifest_sha256: activation.manifest_sha256.clone(),
            next_trust_generation: activation.trust_generation,
            next_channel_sequence: activation.channel_sequence,
            transition_kind: transition_kind.to_string(),
            authority_sha256: authority_sha256.to_string(),
            rollback_authority_sha256: rollback_authority_sha256.map(str::to_string),
            recorded_by: recorded_by.to_string(),
            recorded_at_ms,
        };
        transition.transition_sha256 = transition.digest()?;
        Ok(transition)
    }

    fn digest(&self) -> Result<String, BrowserReleaseRegistryError> {
        browser_release_registry_digest(&BrowserReleaseChannelTransitionDigest {
            version: 1,
            audience: BROWSER_RELEASE_TRANSITION_AUDIENCE,
            channel: &self.channel,
            head_revision: self.head_revision,
            previous_head_revision: self.previous_head_revision,
            previous_transition_sha256: self.previous_transition_sha256.as_deref(),
            previous_activation_sha256: self.previous_activation_sha256.as_deref(),
            previous_manifest_sha256: self.previous_manifest_sha256.as_deref(),
            previous_trust_generation: self.previous_trust_generation,
            previous_channel_sequence: self.previous_channel_sequence,
            next_activation_sha256: &self.next_activation_sha256,
            next_manifest_sha256: &self.next_manifest_sha256,
            next_trust_generation: self.next_trust_generation,
            next_channel_sequence: self.next_channel_sequence,
            transition_kind: &self.transition_kind,
            authority_sha256: &self.authority_sha256,
            rollback_authority_sha256: self.rollback_authority_sha256.as_deref(),
            recorded_at_ms: self.recorded_at_ms,
        })
    }
}

#[derive(Debug, Clone)]
struct StoredBrowserReleaseTrustPolicy {
    policy_sha256: String,
    trust_generation: i64,
    canonical_policy_base64url: String,
    authorization_signature_set_sha256: String,
    policy: BrowserReleaseTrustPolicyAuthority,
    canonical_bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StoredBrowserReleaseSignatureSet {
    signature_set_sha256: String,
    signature_set_id: String,
    trust_generation: i64,
    role: String,
    target_audience: String,
    target_sha256: String,
    signed_at_ms: i64,
    signature_count: i64,
    canonical_signature_set_base64url: String,
}

#[derive(Debug, Clone)]
struct VerifiedBrowserReleaseManifestImport {
    manifest: BrowserReleaseManifestAuthority,
    signature_set: BrowserReleaseSignatureSetAuthority,
    manifest_sha256: String,
    signature_set_sha256: String,
    descriptors: BTreeMap<String, VerifiedBrowserBuildDescriptor>,
}

#[derive(Debug, Clone)]
struct StoredBrowserReleaseManifest {
    manifest_sha256: String,
    manifest_id: String,
    canonical_manifest_base64url: String,
    authorization_signature_set_sha256: String,
    release_id: String,
    release_sequence: i64,
    build_id: String,
    app_version: String,
    protocol_version: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StoredBrowserReleaseArtifact {
    artifact_id: String,
    platform: String,
    architecture: String,
    package_kind: String,
    build_descriptor_sha256: String,
    build_descriptor_base64url: String,
    build_descriptor_signature_base64url: String,
    build_descriptor_signing_key_id: String,
    artifact_url: String,
    artifact_filename: String,
    artifact_size_bytes: i64,
    artifact_sha256: String,
    app_content_sha256: String,
    automation_bundle_sha256: String,
    chromium_executable_sha256: String,
    verification_evidence_sha256: String,
    native_signature_kind: String,
    native_signer_identity: String,
}

fn parse_browser_release_manifest_replay_candidate(
    request: &BrowserReleaseManifestImportRequest,
) -> Result<VerifiedBrowserReleaseManifestImport, BrowserReleaseRegistryError> {
    if request.build_proofs.len() != 3 {
        return Err(BrowserReleaseRegistryError::InvalidRequest);
    }
    let envelope = BrowserReleaseAuthorityEnvelope {
        canonical_base64url: request.canonical_base64url.clone(),
        signature_set_base64url: request.signature_set_base64url.clone(),
    };
    let (manifest_bytes, signature_set_bytes) =
        decode_browser_release_authority_envelope(&envelope)?;
    let manifest = parse_canonical_browser_release_manifest(&manifest_bytes)?;
    let signature_set = parse_canonical_browser_release_signature_set(&signature_set_bytes)?;
    let mut descriptors = BTreeMap::new();
    for proof in &request.build_proofs {
        let verified = parse_browser_build_proof_for_claim(proof)
            .map_err(|_| BrowserReleaseRegistryError::InvalidAuthority)?;
        let target = format!("{}:{}", verified.platform, verified.architecture);
        if descriptors.insert(target, verified).is_some() {
            return Err(BrowserReleaseRegistryError::InvalidRequest);
        }
    }
    if descriptors
        .keys()
        .map(String::as_str)
        .ne(["darwin:arm64", "darwin:x64", "windows:x64"])
    {
        return Err(BrowserReleaseRegistryError::InvalidRequest);
    }
    Ok(VerifiedBrowserReleaseManifestImport {
        manifest,
        signature_set,
        manifest_sha256: browser_release_authority_sha256(&manifest_bytes),
        signature_set_sha256: browser_release_authority_sha256(&signature_set_bytes),
        descriptors,
    })
}

fn verify_browser_release_manifest_import(
    request: &BrowserReleaseManifestImportRequest,
    policy: &StoredBrowserReleaseTrustPolicy,
    verification_time_ms: i64,
) -> Result<VerifiedBrowserReleaseManifestImport, BrowserReleaseRegistryError> {
    if request.build_proofs.len() != 3 {
        return Err(BrowserReleaseRegistryError::InvalidRequest);
    }
    let envelope = BrowserReleaseAuthorityEnvelope {
        canonical_base64url: request.canonical_base64url.clone(),
        signature_set_base64url: request.signature_set_base64url.clone(),
    };
    let (manifest_bytes, signature_set_bytes) =
        decode_browser_release_authority_envelope(&envelope)?;
    let manifest = verify_browser_release_manifest_authority(
        &manifest_bytes,
        &signature_set_bytes,
        &policy.policy,
        verification_time_ms,
    )?;
    let signature_set = parse_canonical_browser_release_signature_set(&signature_set_bytes)?;
    let mut descriptors = BTreeMap::new();
    for proof in &request.build_proofs {
        let descriptor = verify_browser_build_proof_against_release_policy(proof, &policy.policy)?;
        let target = format!("{}:{}", descriptor.platform, descriptor.architecture);
        if descriptors.insert(target, descriptor).is_some() {
            return Err(BrowserReleaseRegistryError::InvalidRequest);
        }
    }
    let targets = descriptors.keys().cloned().collect::<Vec<_>>();
    if targets != ["darwin:arm64", "darwin:x64", "windows:x64"] {
        return Err(BrowserReleaseRegistryError::InvalidRequest);
    }
    for descriptor in descriptors.values() {
        if descriptor.release_id != manifest.release_id
            || descriptor.build_id != manifest.build_id
            || descriptor.app_version != manifest.app_version
            || descriptor.protocol_version != manifest.protocol_version
            || descriptor.source_commit != manifest.source_commit
            || descriptor.electron_version != manifest.electron_version
            || descriptor.playwright_version != manifest.playwright_version
            || descriptor.chromium_revision != manifest.chromium_revision
            || descriptor.issued_at_ms > manifest.published_at_ms
            || descriptor.issued_at_ms > verification_time_ms
        {
            return Err(BrowserReleaseRegistryError::InvalidAuthority);
        }
    }
    if manifest.published_at_ms > verification_time_ms
        || signature_set.signed_at_ms > verification_time_ms
    {
        return Err(BrowserReleaseRegistryError::InvalidAuthority);
    }
    for artifact in &manifest.artifacts {
        let target = format!("{}:{}", artifact.platform, artifact.architecture);
        let descriptor = descriptors
            .get(&target)
            .ok_or(BrowserReleaseRegistryError::InvalidAuthority)?;
        if artifact.build_descriptor_sha256 != descriptor.descriptor_sha256 {
            return Err(BrowserReleaseRegistryError::InvalidAuthority);
        }
    }
    Ok(VerifiedBrowserReleaseManifestImport {
        manifest,
        manifest_sha256: browser_release_authority_sha256(&manifest_bytes),
        signature_set_sha256: browser_release_authority_sha256(&signature_set_bytes),
        signature_set,
        descriptors,
    })
}

pub fn import_browser_release_trust_policy(
    pool: &DbPool,
    envelope: &BrowserReleaseAuthorityEnvelope,
    recorded_by: &str,
) -> Result<BrowserReleaseImportResult, BrowserReleaseRegistryError> {
    validate_browser_release_recorded_by(recorded_by)?;
    let (policy_bytes, signature_set_bytes) = decode_browser_release_authority_envelope(envelope)?;
    let policy = parse_canonical_browser_release_trust_policy(&policy_bytes)?;
    let signature_set = parse_canonical_browser_release_signature_set(&signature_set_bytes)?;
    let policy_sha256 = browser_release_authority_sha256(&policy_bytes);
    let signature_set_sha256 = browser_release_authority_sha256(&signature_set_bytes);
    let recorded_at_ms = now_ms();
    if policy.issued_at_ms > recorded_at_ms || signature_set.signed_at_ms > recorded_at_ms {
        return Err(BrowserReleaseRegistryError::InvalidAuthority);
    }

    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut connection = pool.get().map_err(browser_release_registry_storage)?;
            let transaction = connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(browser_release_registry_storage)?;
            let existing = sqlite_browser_release_policy_identity(
                &transaction,
                &policy.policy_id,
                policy.trust_generation,
            )?;
            if let Some(existing) = existing {
                require_exact_browser_release_policy_replay(
                    &existing,
                    &policy_sha256,
                    &envelope.canonical_base64url,
                    &signature_set_sha256,
                )?;
                require_exact_sqlite_browser_release_signature_set(
                    &transaction,
                    &signature_set,
                    &signature_set_sha256,
                    &envelope.signature_set_base64url,
                )?;
                transaction
                    .commit()
                    .map_err(browser_release_registry_storage)?;
                return Ok(browser_release_policy_import_result(
                    &policy,
                    &policy_sha256,
                    &signature_set_sha256,
                    true,
                ));
            }

            let predecessor = sqlite_latest_browser_release_policy(&transaction)?;
            verify_new_browser_release_policy(
                &policy,
                &policy_bytes,
                &signature_set_bytes,
                predecessor.as_ref(),
                recorded_at_ms,
            )?;
            ensure_sqlite_browser_release_signature_keys_not_revoked(&transaction, &signature_set)?;
            insert_sqlite_browser_release_signature_set(
                &transaction,
                &signature_set,
                &signature_set_sha256,
                &envelope.signature_set_base64url,
                recorded_by,
                recorded_at_ms,
            )?;
            insert_sqlite_browser_release_policy(
                &transaction,
                &policy,
                &policy_sha256,
                &envelope.canonical_base64url,
                &signature_set_sha256,
                recorded_by,
                recorded_at_ms,
            )?;
            transaction
                .commit()
                .map_err(browser_release_registry_storage)?;
            Ok(browser_release_policy_import_result(
                &policy,
                &policy_sha256,
                &signature_set_sha256,
                false,
            ))
        }
        DbPool::Postgres(_) => {
            let mut connection = pool.get_pg().map_err(browser_release_registry_storage)?;
            let mut transaction = connection
                .transaction()
                .map_err(browser_release_registry_storage)?;
            transaction
                .query_one(
                    "SELECT pg_advisory_xact_lock( \
                     hashtextextended('jobs-browser-release-registry', 0))",
                    &[],
                )
                .map_err(browser_release_registry_storage)?;
            let existing = postgres_browser_release_policy_identity(
                &mut transaction,
                &policy.policy_id,
                policy.trust_generation,
            )?;
            if let Some(existing) = existing {
                require_exact_browser_release_policy_replay(
                    &existing,
                    &policy_sha256,
                    &envelope.canonical_base64url,
                    &signature_set_sha256,
                )?;
                require_exact_postgres_browser_release_signature_set(
                    &mut transaction,
                    &signature_set,
                    &signature_set_sha256,
                    &envelope.signature_set_base64url,
                )?;
                transaction
                    .commit()
                    .map_err(browser_release_registry_storage)?;
                return Ok(browser_release_policy_import_result(
                    &policy,
                    &policy_sha256,
                    &signature_set_sha256,
                    true,
                ));
            }

            let predecessor = postgres_latest_browser_release_policy(&mut transaction)?;
            verify_new_browser_release_policy(
                &policy,
                &policy_bytes,
                &signature_set_bytes,
                predecessor.as_ref(),
                recorded_at_ms,
            )?;
            ensure_postgres_browser_release_signature_keys_not_revoked(
                &mut transaction,
                &signature_set,
            )?;
            insert_postgres_browser_release_signature_set(
                &mut transaction,
                &signature_set,
                &signature_set_sha256,
                &envelope.signature_set_base64url,
                recorded_by,
                recorded_at_ms,
            )?;
            insert_postgres_browser_release_policy(
                &mut transaction,
                &policy,
                &policy_sha256,
                &envelope.canonical_base64url,
                &signature_set_sha256,
                recorded_by,
                recorded_at_ms,
            )?;
            transaction
                .commit()
                .map_err(browser_release_registry_storage)?;
            Ok(browser_release_policy_import_result(
                &policy,
                &policy_sha256,
                &signature_set_sha256,
                false,
            ))
        }
    })
}

fn verify_new_browser_release_policy(
    policy: &BrowserReleaseTrustPolicyAuthority,
    policy_bytes: &[u8],
    signature_set_bytes: &[u8],
    predecessor: Option<&StoredBrowserReleaseTrustPolicy>,
    verification_time_ms: i64,
) -> Result<(), BrowserReleaseRegistryError> {
    match predecessor {
        Some(previous) => {
            if policy.trust_generation != previous.trust_generation + 1 {
                return Err(BrowserReleaseRegistryError::SequenceRegression);
            }
            verify_browser_release_trust_policy_authority(
                policy_bytes,
                signature_set_bytes,
                Some(&previous.canonical_bytes),
                None,
                verification_time_ms,
            )?;
        }
        None => {
            if policy.trust_generation != 1 {
                return Err(BrowserReleaseRegistryError::SequenceRegression);
            }
            let bootstrap = browser_release_root_trust_anchor_from_environment()?;
            verify_browser_release_trust_policy_authority(
                policy_bytes,
                signature_set_bytes,
                None,
                Some(&bootstrap),
                verification_time_ms,
            )?;
        }
    }
    Ok(())
}

fn browser_release_policy_import_result(
    policy: &BrowserReleaseTrustPolicyAuthority,
    policy_sha256: &str,
    signature_set_sha256: &str,
    replayed: bool,
) -> BrowserReleaseImportResult {
    BrowserReleaseImportResult {
        authority_kind: "trust-policy".to_string(),
        authority_id: policy.policy_id.clone(),
        authority_sha256: policy_sha256.to_string(),
        signature_set_sha256: signature_set_sha256.to_string(),
        trust_policy_sha256: policy_sha256.to_string(),
        manifest_authorization_signature_set_sha256: None,
        activation_authorization_signature_set_sha256: None,
        replayed,
    }
}

fn require_exact_browser_release_policy_replay(
    existing: &StoredBrowserReleaseTrustPolicy,
    policy_sha256: &str,
    canonical_policy_base64url: &str,
    signature_set_sha256: &str,
) -> Result<(), BrowserReleaseRegistryError> {
    if existing.policy_sha256 != policy_sha256
        || existing.canonical_policy_base64url != canonical_policy_base64url
        || existing.authorization_signature_set_sha256 != signature_set_sha256
    {
        return Err(BrowserReleaseRegistryError::IdentityConflict);
    }
    Ok(())
}

fn stored_browser_release_policy(
    policy_sha256: String,
    policy_id: String,
    trust_generation: i64,
    canonical_policy_base64url: String,
    authorization_signature_set_sha256: String,
) -> Result<StoredBrowserReleaseTrustPolicy, BrowserReleaseRegistryError> {
    let canonical_bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(&canonical_policy_base64url)
        .map_err(browser_release_registry_storage)?;
    let policy = parse_canonical_browser_release_trust_policy(&canonical_bytes)
        .map_err(|error| browser_release_registry_storage(anyhow::anyhow!(error)))?;
    if browser_release_authority_sha256(&canonical_bytes) != policy_sha256
        || policy.policy_id != policy_id
        || policy.trust_generation != trust_generation
    {
        return Err(browser_release_registry_storage(anyhow::anyhow!(
            "stored Browser trust policy does not match its relational identity"
        )));
    }
    Ok(StoredBrowserReleaseTrustPolicy {
        policy_sha256,
        trust_generation,
        canonical_policy_base64url,
        authorization_signature_set_sha256,
        policy,
        canonical_bytes,
    })
}

fn sqlite_browser_release_policy_identity(
    transaction: &rusqlite::Transaction<'_>,
    policy_id: &str,
    trust_generation: i64,
) -> Result<Option<StoredBrowserReleaseTrustPolicy>, BrowserReleaseRegistryError> {
    transaction
        .query_row(
            "SELECT policy_sha256, policy_id, trust_generation, \
                    canonical_policy_base64url, authorization_signature_set_sha256 \
             FROM jobs_browser_release_trust_policies \
             WHERE policy_id = ?1 OR trust_generation = ?2 \
             LIMIT 1",
            params![policy_id, trust_generation],
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
        .map_err(browser_release_registry_storage)?
        .map(|(sha256, id, generation, canonical, signature_set)| {
            stored_browser_release_policy(sha256, id, generation, canonical, signature_set)
        })
        .transpose()
}

fn sqlite_latest_browser_release_policy(
    transaction: &rusqlite::Transaction<'_>,
) -> Result<Option<StoredBrowserReleaseTrustPolicy>, BrowserReleaseRegistryError> {
    transaction
        .query_row(
            "SELECT policy_sha256, policy_id, trust_generation, \
                    canonical_policy_base64url, authorization_signature_set_sha256 \
             FROM jobs_browser_release_trust_policies \
             ORDER BY trust_generation DESC LIMIT 1",
            [],
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
        .map_err(browser_release_registry_storage)?
        .map(|(sha256, id, generation, canonical, signature_set)| {
            stored_browser_release_policy(sha256, id, generation, canonical, signature_set)
        })
        .transpose()
}

fn postgres_browser_release_policy_identity(
    transaction: &mut postgres::Transaction<'_>,
    policy_id: &str,
    trust_generation: i64,
) -> Result<Option<StoredBrowserReleaseTrustPolicy>, BrowserReleaseRegistryError> {
    transaction
        .query_opt(
            "SELECT policy_sha256, policy_id, trust_generation, \
                    canonical_policy_base64url, authorization_signature_set_sha256 \
             FROM jobs_browser_release_trust_policies \
             WHERE policy_id = $1 OR trust_generation = $2 \
             LIMIT 1",
            &[&policy_id, &trust_generation],
        )
        .map_err(browser_release_registry_storage)?
        .map(|row| {
            stored_browser_release_policy(
                row.get(0),
                row.get(1),
                row.get(2),
                row.get(3),
                row.get(4),
            )
        })
        .transpose()
}

fn postgres_latest_browser_release_policy(
    transaction: &mut postgres::Transaction<'_>,
) -> Result<Option<StoredBrowserReleaseTrustPolicy>, BrowserReleaseRegistryError> {
    transaction
        .query_opt(
            "SELECT policy_sha256, policy_id, trust_generation, \
                    canonical_policy_base64url, authorization_signature_set_sha256 \
             FROM jobs_browser_release_trust_policies \
             ORDER BY trust_generation DESC LIMIT 1",
            &[],
        )
        .map_err(browser_release_registry_storage)?
        .map(|row| {
            stored_browser_release_policy(
                row.get(0),
                row.get(1),
                row.get(2),
                row.get(3),
                row.get(4),
            )
        })
        .transpose()
}

fn require_exact_sqlite_browser_release_signature_set(
    transaction: &rusqlite::Transaction<'_>,
    signature_set: &BrowserReleaseSignatureSetAuthority,
    signature_set_sha256: &str,
    canonical_signature_set_base64url: &str,
) -> Result<(), BrowserReleaseRegistryError> {
    let existing = transaction
        .query_row(
            "SELECT signature_set_sha256, signature_set_id, trust_generation, role, \
                    target_audience, target_sha256, signed_at_ms, signature_count, \
                    canonical_signature_set_base64url \
             FROM jobs_browser_release_signature_sets \
             WHERE signature_set_sha256 = ?1 OR signature_set_id = ?2 \
             LIMIT 1",
            params![signature_set_sha256, signature_set.signature_set_id],
            |row| {
                Ok(StoredBrowserReleaseSignatureSet {
                    signature_set_sha256: row.get(0)?,
                    signature_set_id: row.get(1)?,
                    trust_generation: row.get(2)?,
                    role: row.get(3)?,
                    target_audience: row.get(4)?,
                    target_sha256: row.get(5)?,
                    signed_at_ms: row.get(6)?,
                    signature_count: row.get(7)?,
                    canonical_signature_set_base64url: row.get(8)?,
                })
            },
        )
        .optional()
        .map_err(browser_release_registry_storage)?;
    let expected = StoredBrowserReleaseSignatureSet {
        signature_set_sha256: signature_set_sha256.to_string(),
        signature_set_id: signature_set.signature_set_id.clone(),
        trust_generation: signature_set.trust_generation,
        role: signature_set.role.clone(),
        target_audience: signature_set.target_audience.clone(),
        target_sha256: signature_set.target_sha256.clone(),
        signed_at_ms: signature_set.signed_at_ms,
        signature_count: i64::try_from(signature_set.signatures.len())
            .map_err(|_| BrowserReleaseRegistryError::InvalidAuthority)?,
        canonical_signature_set_base64url: canonical_signature_set_base64url.to_string(),
    };
    if existing.as_ref() != Some(&expected) {
        return Err(BrowserReleaseRegistryError::IdentityConflict);
    }
    let mut statement = transaction
        .prepare(
            r#"SELECT key_id, signature_base64url
                 FROM jobs_browser_release_signatures
                WHERE signature_set_sha256 = ?1
                ORDER BY key_id"#,
        )
        .map_err(browser_release_registry_storage)?;
    let stored_signatures = statement
        .query_map(params![signature_set_sha256], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(browser_release_registry_storage)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(browser_release_registry_storage)?;
    let expected_signatures = signature_set
        .signatures
        .iter()
        .map(|signature| (signature.key_id.clone(), signature.signature.clone()))
        .collect::<Vec<_>>();
    if stored_signatures != expected_signatures {
        return Err(BrowserReleaseRegistryError::IdentityConflict);
    }
    Ok(())
}

fn require_exact_postgres_browser_release_signature_set(
    transaction: &mut postgres::Transaction<'_>,
    signature_set: &BrowserReleaseSignatureSetAuthority,
    signature_set_sha256: &str,
    canonical_signature_set_base64url: &str,
) -> Result<(), BrowserReleaseRegistryError> {
    let existing = transaction
        .query_opt(
            "SELECT signature_set_sha256, signature_set_id, trust_generation, role, \
                    target_audience, target_sha256, signed_at_ms, signature_count, \
                    canonical_signature_set_base64url \
             FROM jobs_browser_release_signature_sets \
             WHERE signature_set_sha256 = $1 OR signature_set_id = $2 \
             LIMIT 1",
            &[&signature_set_sha256, &signature_set.signature_set_id],
        )
        .map_err(browser_release_registry_storage)?;
    let existing = existing.map(|row| StoredBrowserReleaseSignatureSet {
        signature_set_sha256: row.get(0),
        signature_set_id: row.get(1),
        trust_generation: row.get(2),
        role: row.get(3),
        target_audience: row.get(4),
        target_sha256: row.get(5),
        signed_at_ms: row.get(6),
        signature_count: row.get(7),
        canonical_signature_set_base64url: row.get(8),
    });
    let expected = StoredBrowserReleaseSignatureSet {
        signature_set_sha256: signature_set_sha256.to_string(),
        signature_set_id: signature_set.signature_set_id.clone(),
        trust_generation: signature_set.trust_generation,
        role: signature_set.role.clone(),
        target_audience: signature_set.target_audience.clone(),
        target_sha256: signature_set.target_sha256.clone(),
        signed_at_ms: signature_set.signed_at_ms,
        signature_count: i64::try_from(signature_set.signatures.len())
            .map_err(|_| BrowserReleaseRegistryError::InvalidAuthority)?,
        canonical_signature_set_base64url: canonical_signature_set_base64url.to_string(),
    };
    if existing.as_ref() != Some(&expected) {
        return Err(BrowserReleaseRegistryError::IdentityConflict);
    }
    let stored_signatures = transaction
        .query(
            r#"SELECT key_id, signature_base64url
                 FROM jobs_browser_release_signatures
                WHERE signature_set_sha256 = $1
                ORDER BY key_id"#,
            &[&signature_set_sha256],
        )
        .map_err(browser_release_registry_storage)?
        .into_iter()
        .map(|row| (row.get::<_, String>(0), row.get::<_, String>(1)))
        .collect::<Vec<_>>();
    let expected_signatures = signature_set
        .signatures
        .iter()
        .map(|signature| (signature.key_id.clone(), signature.signature.clone()))
        .collect::<Vec<_>>();
    if stored_signatures != expected_signatures {
        return Err(BrowserReleaseRegistryError::IdentityConflict);
    }
    Ok(())
}

fn insert_sqlite_browser_release_signature_set(
    transaction: &rusqlite::Transaction<'_>,
    signature_set: &BrowserReleaseSignatureSetAuthority,
    signature_set_sha256: &str,
    canonical_signature_set_base64url: &str,
    recorded_by: &str,
    recorded_at_ms: i64,
) -> Result<(), BrowserReleaseRegistryError> {
    let existing: Option<String> = transaction
        .query_row(
            "SELECT signature_set_sha256 FROM jobs_browser_release_signature_sets \
             WHERE signature_set_sha256 = ?1 OR signature_set_id = ?2 LIMIT 1",
            params![signature_set_sha256, signature_set.signature_set_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(browser_release_registry_storage)?;
    if existing.is_some() {
        return require_exact_sqlite_browser_release_signature_set(
            transaction,
            signature_set,
            signature_set_sha256,
            canonical_signature_set_base64url,
        );
    }
    transaction
        .execute(
            "INSERT INTO jobs_browser_release_signature_sets ( \
               signature_set_sha256, signature_set_id, trust_generation, role, \
               target_audience, target_sha256, signed_at_ms, signature_count, \
               canonical_signature_set_base64url, recorded_by, recorded_at_ms \
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                signature_set_sha256,
                signature_set.signature_set_id,
                signature_set.trust_generation,
                signature_set.role,
                signature_set.target_audience,
                signature_set.target_sha256,
                signature_set.signed_at_ms,
                i64::try_from(signature_set.signatures.len())
                    .map_err(|_| BrowserReleaseRegistryError::InvalidAuthority)?,
                canonical_signature_set_base64url,
                recorded_by,
                recorded_at_ms,
            ],
        )
        .map_err(browser_release_registry_storage)?;
    for signature in &signature_set.signatures {
        transaction
            .execute(
                "INSERT INTO jobs_browser_release_signatures ( \
                   signature_set_sha256, key_id, signature_base64url \
                 ) VALUES (?1, ?2, ?3)",
                params![signature_set_sha256, signature.key_id, signature.signature],
            )
            .map_err(browser_release_registry_storage)?;
    }
    Ok(())
}

fn insert_postgres_browser_release_signature_set(
    transaction: &mut postgres::Transaction<'_>,
    signature_set: &BrowserReleaseSignatureSetAuthority,
    signature_set_sha256: &str,
    canonical_signature_set_base64url: &str,
    recorded_by: &str,
    recorded_at_ms: i64,
) -> Result<(), BrowserReleaseRegistryError> {
    let existing = transaction
        .query_opt(
            "SELECT signature_set_sha256 FROM jobs_browser_release_signature_sets \
             WHERE signature_set_sha256 = $1 OR signature_set_id = $2 LIMIT 1",
            &[&signature_set_sha256, &signature_set.signature_set_id],
        )
        .map_err(browser_release_registry_storage)?;
    if existing.is_some() {
        return require_exact_postgres_browser_release_signature_set(
            transaction,
            signature_set,
            signature_set_sha256,
            canonical_signature_set_base64url,
        );
    }
    let signature_count = i64::try_from(signature_set.signatures.len())
        .map_err(|_| BrowserReleaseRegistryError::InvalidAuthority)?;
    transaction
        .execute(
            "INSERT INTO jobs_browser_release_signature_sets ( \
               signature_set_sha256, signature_set_id, trust_generation, role, \
               target_audience, target_sha256, signed_at_ms, signature_count, \
               canonical_signature_set_base64url, recorded_by, recorded_at_ms \
             ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
            &[
                &signature_set_sha256,
                &signature_set.signature_set_id,
                &signature_set.trust_generation,
                &signature_set.role,
                &signature_set.target_audience,
                &signature_set.target_sha256,
                &signature_set.signed_at_ms,
                &signature_count,
                &canonical_signature_set_base64url,
                &recorded_by,
                &recorded_at_ms,
            ],
        )
        .map_err(browser_release_registry_storage)?;
    for signature in &signature_set.signatures {
        transaction
            .execute(
                "INSERT INTO jobs_browser_release_signatures ( \
                   signature_set_sha256, key_id, signature_base64url \
                 ) VALUES ($1, $2, $3)",
                &[
                    &signature_set_sha256,
                    &signature.key_id,
                    &signature.signature,
                ],
            )
            .map_err(browser_release_registry_storage)?;
    }
    Ok(())
}

fn insert_sqlite_browser_release_policy(
    transaction: &rusqlite::Transaction<'_>,
    policy: &BrowserReleaseTrustPolicyAuthority,
    policy_sha256: &str,
    canonical_policy_base64url: &str,
    signature_set_sha256: &str,
    recorded_by: &str,
    recorded_at_ms: i64,
) -> Result<(), BrowserReleaseRegistryError> {
    let thresholds = browser_release_policy_thresholds(policy)?;
    transaction
        .execute(
            "INSERT INTO jobs_browser_release_trust_policies ( \
               policy_sha256, policy_id, trust_generation, predecessor_policy_sha256, \
               predecessor_trust_generation, root_threshold, release_threshold, \
               promotion_threshold, incident_threshold, key_count, \
               canonical_policy_base64url, authorization_signature_set_sha256, \
               issued_at_ms, valid_from_ms, expires_at_ms, recorded_by, recorded_at_ms \
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, \
                       ?13, ?14, ?15, ?16, ?17)",
            params![
                policy_sha256,
                policy.policy_id,
                policy.trust_generation,
                policy.predecessor_policy_sha256,
                policy.trust_generation - 1,
                thresholds.0,
                thresholds.1,
                thresholds.2,
                thresholds.3,
                i64::try_from(policy.keys.len())
                    .map_err(|_| BrowserReleaseRegistryError::InvalidAuthority)?,
                canonical_policy_base64url,
                signature_set_sha256,
                policy.issued_at_ms,
                policy.valid_from_ms,
                policy.expires_at_ms,
                recorded_by,
                recorded_at_ms,
            ],
        )
        .map_err(browser_release_registry_storage)?;
    for key in &policy.keys {
        transaction
            .execute(
                "INSERT INTO jobs_browser_release_trust_keys ( \
                   policy_sha256, trust_generation, key_id, role, public_key_base64url, \
                   state, valid_from_ms, valid_until_ms, minimum_trust_generation, \
                   maximum_trust_generation \
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    policy_sha256,
                    policy.trust_generation,
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
            .map_err(browser_release_registry_storage)?;
    }
    Ok(())
}

fn insert_postgres_browser_release_policy(
    transaction: &mut postgres::Transaction<'_>,
    policy: &BrowserReleaseTrustPolicyAuthority,
    policy_sha256: &str,
    canonical_policy_base64url: &str,
    signature_set_sha256: &str,
    recorded_by: &str,
    recorded_at_ms: i64,
) -> Result<(), BrowserReleaseRegistryError> {
    let thresholds = browser_release_policy_thresholds(policy)?;
    let key_count = i64::try_from(policy.keys.len())
        .map_err(|_| BrowserReleaseRegistryError::InvalidAuthority)?;
    let predecessor_generation = policy.trust_generation - 1;
    transaction
        .execute(
            "INSERT INTO jobs_browser_release_trust_policies ( \
               policy_sha256, policy_id, trust_generation, predecessor_policy_sha256, \
               predecessor_trust_generation, root_threshold, release_threshold, \
               promotion_threshold, incident_threshold, key_count, \
               canonical_policy_base64url, authorization_signature_set_sha256, \
               issued_at_ms, valid_from_ms, expires_at_ms, recorded_by, recorded_at_ms \
             ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, \
                       $13, $14, $15, $16, $17)",
            &[
                &policy_sha256,
                &policy.policy_id,
                &policy.trust_generation,
                &policy.predecessor_policy_sha256,
                &predecessor_generation,
                &thresholds.0,
                &thresholds.1,
                &thresholds.2,
                &thresholds.3,
                &key_count,
                &canonical_policy_base64url,
                &signature_set_sha256,
                &policy.issued_at_ms,
                &policy.valid_from_ms,
                &policy.expires_at_ms,
                &recorded_by,
                &recorded_at_ms,
            ],
        )
        .map_err(browser_release_registry_storage)?;
    for key in &policy.keys {
        transaction
            .execute(
                "INSERT INTO jobs_browser_release_trust_keys ( \
                   policy_sha256, trust_generation, key_id, role, public_key_base64url, \
                   state, valid_from_ms, valid_until_ms, minimum_trust_generation, \
                   maximum_trust_generation \
                 ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
                &[
                    &policy_sha256,
                    &policy.trust_generation,
                    &key.key_id,
                    &key.role,
                    &key.public_key,
                    &key.state,
                    &key.valid_from_ms,
                    &key.valid_until_ms,
                    &key.minimum_trust_generation,
                    &key.maximum_trust_generation,
                ],
            )
            .map_err(browser_release_registry_storage)?;
    }
    Ok(())
}

fn browser_release_policy_thresholds(
    policy: &BrowserReleaseTrustPolicyAuthority,
) -> Result<(i64, i64, i64, i64), BrowserReleaseRegistryError> {
    let threshold = |role: &str| {
        policy
            .roles
            .iter()
            .find(|candidate| candidate.role == role)
            .map(|candidate| candidate.threshold)
            .ok_or(BrowserReleaseRegistryError::InvalidAuthority)
    };
    Ok((
        threshold("root")?,
        threshold("release")?,
        threshold("promotion")?,
        threshold("incident")?,
    ))
}

pub fn import_browser_release_manifest(
    pool: &DbPool,
    request: &BrowserReleaseManifestImportRequest,
    recorded_by: &str,
) -> Result<BrowserReleaseImportResult, BrowserReleaseRegistryError> {
    validate_browser_release_recorded_by(recorded_by)?;
    let verification_time_ms = now_ms();
    let replay_candidate = parse_browser_release_manifest_replay_candidate(request)?;
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut connection = pool.get().map_err(browser_release_registry_storage)?;
            let transaction = connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(browser_release_registry_storage)?;
            if let Some(existing) =
                sqlite_browser_release_manifest_identity(&transaction, &replay_candidate.manifest)?
            {
                require_exact_browser_release_manifest_replay(
                    &existing,
                    &replay_candidate,
                    request,
                )?;
                require_exact_sqlite_browser_release_signature_set(
                    &transaction,
                    &replay_candidate.signature_set,
                    &replay_candidate.signature_set_sha256,
                    &request.signature_set_base64url,
                )?;
                require_exact_sqlite_browser_release_artifacts(&transaction, &replay_candidate)?;
                let replay_policy_sha256 = sqlite_browser_release_replay_policy_sha256(
                    &transaction,
                    replay_candidate.signature_set.trust_generation,
                )?;
                transaction
                    .commit()
                    .map_err(browser_release_registry_storage)?;
                return Ok(browser_release_manifest_import_result(
                    &replay_candidate,
                    &replay_policy_sha256,
                    true,
                ));
            }
            let policy = sqlite_latest_browser_release_policy(&transaction)?
                .ok_or(BrowserReleaseRegistryError::NotFound)?;
            let verified =
                verify_browser_release_manifest_import(request, &policy, verification_time_ms)?;
            ensure_sqlite_browser_artifact_identities_available(&transaction, &verified.manifest)?;
            ensure_sqlite_browser_release_signature_keys_not_revoked(
                &transaction,
                &verified.signature_set,
            )?;
            ensure_sqlite_browser_release_descriptor_keys_not_revoked(&transaction, &verified)?;
            let recorded_at_ms = verification_time_ms;
            insert_sqlite_browser_release_signature_set(
                &transaction,
                &verified.signature_set,
                &verified.signature_set_sha256,
                &request.signature_set_base64url,
                recorded_by,
                recorded_at_ms,
            )?;
            insert_sqlite_browser_release_manifest(
                &transaction,
                &verified,
                request,
                recorded_by,
                recorded_at_ms,
            )?;
            transaction
                .commit()
                .map_err(browser_release_registry_storage)?;
            Ok(browser_release_manifest_import_result(
                &verified,
                &policy.policy_sha256,
                false,
            ))
        }
        DbPool::Postgres(_) => {
            let mut connection = pool.get_pg().map_err(browser_release_registry_storage)?;
            let mut transaction = connection
                .transaction()
                .map_err(browser_release_registry_storage)?;
            transaction
                .query_one(
                    "SELECT pg_advisory_xact_lock( \
                     hashtextextended('jobs-browser-release-registry', 0))",
                    &[],
                )
                .map_err(browser_release_registry_storage)?;
            if let Some(existing) = postgres_browser_release_manifest_identity(
                &mut transaction,
                &replay_candidate.manifest,
            )? {
                require_exact_browser_release_manifest_replay(
                    &existing,
                    &replay_candidate,
                    request,
                )?;
                require_exact_postgres_browser_release_signature_set(
                    &mut transaction,
                    &replay_candidate.signature_set,
                    &replay_candidate.signature_set_sha256,
                    &request.signature_set_base64url,
                )?;
                require_exact_postgres_browser_release_artifacts(
                    &mut transaction,
                    &replay_candidate,
                )?;
                let replay_policy_sha256 = postgres_browser_release_replay_policy_sha256(
                    &mut transaction,
                    replay_candidate.signature_set.trust_generation,
                )?;
                transaction
                    .commit()
                    .map_err(browser_release_registry_storage)?;
                return Ok(browser_release_manifest_import_result(
                    &replay_candidate,
                    &replay_policy_sha256,
                    true,
                ));
            }
            let policy = postgres_latest_browser_release_policy(&mut transaction)?
                .ok_or(BrowserReleaseRegistryError::NotFound)?;
            let verified =
                verify_browser_release_manifest_import(request, &policy, verification_time_ms)?;
            ensure_postgres_browser_artifact_identities_available(
                &mut transaction,
                &verified.manifest,
            )?;
            ensure_postgres_browser_release_signature_keys_not_revoked(
                &mut transaction,
                &verified.signature_set,
            )?;
            ensure_postgres_browser_release_descriptor_keys_not_revoked(
                &mut transaction,
                &verified,
            )?;
            let recorded_at_ms = verification_time_ms;
            insert_postgres_browser_release_signature_set(
                &mut transaction,
                &verified.signature_set,
                &verified.signature_set_sha256,
                &request.signature_set_base64url,
                recorded_by,
                recorded_at_ms,
            )?;
            insert_postgres_browser_release_manifest(
                &mut transaction,
                &verified,
                request,
                recorded_by,
                recorded_at_ms,
            )?;
            transaction
                .commit()
                .map_err(browser_release_registry_storage)?;
            Ok(browser_release_manifest_import_result(
                &verified,
                &policy.policy_sha256,
                false,
            ))
        }
    })
}

pub fn import_browser_release_activation(
    pool: &DbPool,
    envelope: &BrowserReleaseAuthorityEnvelope,
    recorded_by: &str,
) -> Result<BrowserReleaseImportResult, BrowserReleaseRegistryError> {
    validate_browser_release_recorded_by(recorded_by)?;
    let verification_time_ms = now_ms();
    let (activation_bytes, signature_set_bytes) =
        decode_browser_release_authority_envelope(envelope)?;
    let parsed_activation = parse_canonical_browser_release_activation(&activation_bytes)?;
    let parsed_signature_set = parse_canonical_browser_release_signature_set(&signature_set_bytes)?;
    if parsed_activation.issued_at_ms > verification_time_ms
        || parsed_signature_set.signed_at_ms > verification_time_ms
    {
        return Err(BrowserReleaseRegistryError::InvalidAuthority);
    }
    let activation_sha256 = browser_release_authority_sha256(&activation_bytes);
    let signature_set_sha256 = browser_release_authority_sha256(&signature_set_bytes);
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut connection = pool.get().map_err(browser_release_registry_storage)?;
            let transaction = connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(browser_release_registry_storage)?;
            if let Some(existing) =
                sqlite_browser_release_activation_identity(&transaction, &parsed_activation)?
            {
                require_exact_browser_release_activation_replay(
                    &existing,
                    &parsed_activation,
                    &activation_sha256,
                    &envelope.canonical_base64url,
                    &signature_set_sha256,
                )?;
                require_exact_sqlite_browser_release_signature_set(
                    &transaction,
                    &parsed_signature_set,
                    &signature_set_sha256,
                    &envelope.signature_set_base64url,
                )?;
                let replay_policy_sha256 = sqlite_browser_release_replay_policy_sha256(
                    &transaction,
                    parsed_activation.trust_generation,
                )?;
                transaction
                    .commit()
                    .map_err(browser_release_registry_storage)?;
                return Ok(browser_release_activation_import_result(
                    &parsed_activation,
                    &activation_sha256,
                    &signature_set_sha256,
                    &replay_policy_sha256,
                    true,
                ));
            }
            let policy = sqlite_latest_browser_release_policy(&transaction)?
                .ok_or(BrowserReleaseRegistryError::NotFound)?;
            let activation = verify_browser_release_activation_authority(
                &activation_bytes,
                &signature_set_bytes,
                &policy.policy,
                verification_time_ms,
            )?;
            let manifest = sqlite_browser_release_manifest_by_sha256(
                &transaction,
                &activation.manifest_sha256,
            )?
            .ok_or(BrowserReleaseRegistryError::NotFound)?;
            require_browser_release_activation_manifest_binding(&activation, &manifest)?;
            require_stored_browser_release_manifest_artifact_origin(&manifest, &policy)?;
            ensure_sqlite_browser_release_manifest_not_revoked(&transaction, &manifest)?;
            ensure_sqlite_browser_release_signature_keys_not_revoked(
                &transaction,
                &parsed_signature_set,
            )?;
            let recorded_at_ms = verification_time_ms;
            insert_sqlite_browser_release_signature_set(
                &transaction,
                &parsed_signature_set,
                &signature_set_sha256,
                &envelope.signature_set_base64url,
                recorded_by,
                recorded_at_ms,
            )?;
            insert_sqlite_browser_release_activation(
                &transaction,
                &activation,
                &activation_sha256,
                &signature_set_sha256,
                &envelope.canonical_base64url,
                recorded_by,
                recorded_at_ms,
            )?;
            transaction
                .commit()
                .map_err(browser_release_registry_storage)?;
            Ok(browser_release_activation_import_result(
                &activation,
                &activation_sha256,
                &signature_set_sha256,
                &policy.policy_sha256,
                false,
            ))
        }
        DbPool::Postgres(_) => {
            let mut connection = pool.get_pg().map_err(browser_release_registry_storage)?;
            let mut transaction = connection
                .transaction()
                .map_err(browser_release_registry_storage)?;
            transaction
                .query_one(
                    "SELECT pg_advisory_xact_lock( \
                     hashtextextended('jobs-browser-release-registry', 0))",
                    &[],
                )
                .map_err(browser_release_registry_storage)?;
            if let Some(existing) =
                postgres_browser_release_activation_identity(&mut transaction, &parsed_activation)?
            {
                require_exact_browser_release_activation_replay(
                    &existing,
                    &parsed_activation,
                    &activation_sha256,
                    &envelope.canonical_base64url,
                    &signature_set_sha256,
                )?;
                require_exact_postgres_browser_release_signature_set(
                    &mut transaction,
                    &parsed_signature_set,
                    &signature_set_sha256,
                    &envelope.signature_set_base64url,
                )?;
                let replay_policy_sha256 = postgres_browser_release_replay_policy_sha256(
                    &mut transaction,
                    parsed_activation.trust_generation,
                )?;
                transaction
                    .commit()
                    .map_err(browser_release_registry_storage)?;
                return Ok(browser_release_activation_import_result(
                    &parsed_activation,
                    &activation_sha256,
                    &signature_set_sha256,
                    &replay_policy_sha256,
                    true,
                ));
            }
            let policy = postgres_latest_browser_release_policy(&mut transaction)?
                .ok_or(BrowserReleaseRegistryError::NotFound)?;
            let activation = verify_browser_release_activation_authority(
                &activation_bytes,
                &signature_set_bytes,
                &policy.policy,
                verification_time_ms,
            )?;
            let manifest = postgres_browser_release_manifest_by_sha256(
                &mut transaction,
                &activation.manifest_sha256,
            )?
            .ok_or(BrowserReleaseRegistryError::NotFound)?;
            require_browser_release_activation_manifest_binding(&activation, &manifest)?;
            require_stored_browser_release_manifest_artifact_origin(&manifest, &policy)?;
            ensure_postgres_browser_release_manifest_not_revoked(&mut transaction, &manifest)?;
            ensure_postgres_browser_release_signature_keys_not_revoked(
                &mut transaction,
                &parsed_signature_set,
            )?;
            let recorded_at_ms = verification_time_ms;
            insert_postgres_browser_release_signature_set(
                &mut transaction,
                &parsed_signature_set,
                &signature_set_sha256,
                &envelope.signature_set_base64url,
                recorded_by,
                recorded_at_ms,
            )?;
            insert_postgres_browser_release_activation(
                &mut transaction,
                &activation,
                &activation_sha256,
                &signature_set_sha256,
                &envelope.canonical_base64url,
                recorded_by,
                recorded_at_ms,
            )?;
            transaction
                .commit()
                .map_err(browser_release_registry_storage)?;
            Ok(browser_release_activation_import_result(
                &activation,
                &activation_sha256,
                &signature_set_sha256,
                &policy.policy_sha256,
                false,
            ))
        }
    })
}

fn browser_release_manifest_import_result(
    verified: &VerifiedBrowserReleaseManifestImport,
    policy_sha256: &str,
    replayed: bool,
) -> BrowserReleaseImportResult {
    BrowserReleaseImportResult {
        authority_kind: "manifest".to_string(),
        authority_id: verified.manifest.manifest_id.clone(),
        authority_sha256: verified.manifest_sha256.clone(),
        signature_set_sha256: verified.signature_set_sha256.clone(),
        trust_policy_sha256: policy_sha256.to_string(),
        manifest_authorization_signature_set_sha256: Some(verified.signature_set_sha256.clone()),
        activation_authorization_signature_set_sha256: None,
        replayed,
    }
}

fn require_exact_browser_release_manifest_replay(
    existing: &StoredBrowserReleaseManifest,
    verified: &VerifiedBrowserReleaseManifestImport,
    request: &BrowserReleaseManifestImportRequest,
) -> Result<(), BrowserReleaseRegistryError> {
    if existing.manifest_sha256 != verified.manifest_sha256
        || existing.manifest_id != verified.manifest.manifest_id
        || existing.canonical_manifest_base64url != request.canonical_base64url
        || existing.authorization_signature_set_sha256 != verified.signature_set_sha256
        || existing.release_id != verified.manifest.release_id
        || existing.release_sequence != verified.manifest.release_sequence
        || existing.build_id != verified.manifest.build_id
        || existing.app_version != verified.manifest.app_version
        || existing.protocol_version != verified.manifest.protocol_version
    {
        return Err(BrowserReleaseRegistryError::IdentityConflict);
    }
    Ok(())
}

fn expected_browser_release_artifacts(
    verified: &VerifiedBrowserReleaseManifestImport,
) -> Result<Vec<StoredBrowserReleaseArtifact>, BrowserReleaseRegistryError> {
    let mut artifacts = verified
        .manifest
        .artifacts
        .iter()
        .map(|artifact| {
            let descriptor = verified
                .descriptors
                .get(&format!("{}:{}", artifact.platform, artifact.architecture))
                .ok_or(BrowserReleaseRegistryError::InvalidAuthority)?;
            Ok(StoredBrowserReleaseArtifact {
                artifact_id: artifact.artifact_id.clone(),
                platform: artifact.platform.clone(),
                architecture: artifact.architecture.clone(),
                package_kind: artifact.package_kind.clone(),
                build_descriptor_sha256: descriptor.descriptor_sha256.clone(),
                build_descriptor_base64url: descriptor.descriptor_base64url.clone(),
                build_descriptor_signature_base64url: descriptor.signature_base64url.clone(),
                build_descriptor_signing_key_id: descriptor.signing_key_id.clone(),
                artifact_url: artifact.url.clone(),
                artifact_filename: browser_release_artifact_filename(&artifact.url)?,
                artifact_size_bytes: artifact.size_bytes,
                artifact_sha256: artifact.sha256.clone(),
                app_content_sha256: artifact.app_content_sha256.clone(),
                automation_bundle_sha256: artifact.automation_bundle_sha256.clone(),
                chromium_executable_sha256: artifact.chromium_executable_sha256.clone(),
                verification_evidence_sha256: artifact.verification_evidence_sha256.clone(),
                native_signature_kind: artifact.native_signature_kind.clone(),
                native_signer_identity: artifact.native_signer_identity.clone(),
            })
        })
        .collect::<Result<Vec<_>, BrowserReleaseRegistryError>>()?;
    artifacts.sort_by(|left, right| left.artifact_id.cmp(&right.artifact_id));
    Ok(artifacts)
}

fn sqlite_stored_browser_release_artifact(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<StoredBrowserReleaseArtifact> {
    Ok(StoredBrowserReleaseArtifact {
        artifact_id: row.get(0)?,
        platform: row.get(1)?,
        architecture: row.get(2)?,
        package_kind: row.get(3)?,
        build_descriptor_sha256: row.get(4)?,
        build_descriptor_base64url: row.get(5)?,
        build_descriptor_signature_base64url: row.get(6)?,
        build_descriptor_signing_key_id: row.get(7)?,
        artifact_url: row.get(8)?,
        artifact_filename: row.get(9)?,
        artifact_size_bytes: row.get(10)?,
        artifact_sha256: row.get(11)?,
        app_content_sha256: row.get(12)?,
        automation_bundle_sha256: row.get(13)?,
        chromium_executable_sha256: row.get(14)?,
        verification_evidence_sha256: row.get(15)?,
        native_signature_kind: row.get(16)?,
        native_signer_identity: row.get(17)?,
    })
}

fn postgres_stored_browser_release_artifact(row: postgres::Row) -> StoredBrowserReleaseArtifact {
    StoredBrowserReleaseArtifact {
        artifact_id: row.get(0),
        platform: row.get(1),
        architecture: row.get(2),
        package_kind: row.get(3),
        build_descriptor_sha256: row.get(4),
        build_descriptor_base64url: row.get(5),
        build_descriptor_signature_base64url: row.get(6),
        build_descriptor_signing_key_id: row.get(7),
        artifact_url: row.get(8),
        artifact_filename: row.get(9),
        artifact_size_bytes: row.get(10),
        artifact_sha256: row.get(11),
        app_content_sha256: row.get(12),
        automation_bundle_sha256: row.get(13),
        chromium_executable_sha256: row.get(14),
        verification_evidence_sha256: row.get(15),
        native_signature_kind: row.get(16),
        native_signer_identity: row.get(17),
    }
}

fn require_exact_sqlite_browser_release_artifacts(
    transaction: &rusqlite::Transaction<'_>,
    verified: &VerifiedBrowserReleaseManifestImport,
) -> Result<(), BrowserReleaseRegistryError> {
    let mut statement = transaction
        .prepare(
            r#"SELECT artifact.artifact_id, artifact.platform,
                      artifact.architecture, artifact.package_kind,
                      artifact.build_descriptor_sha256,
                      artifact.build_descriptor_base64url,
                      artifact.build_descriptor_signature_base64url,
                      artifact.build_descriptor_signing_key_id,
                      artifact.artifact_url, artifact.artifact_filename,
                      artifact.artifact_size_bytes, artifact.artifact_sha256,
                      artifact.app_content_sha256,
                      runtime.automation_bundle_sha256,
                      runtime.chromium_executable_sha256,
                      artifact.verification_evidence_sha256,
                      artifact.native_signature_kind, artifact.native_signer_identity
                 FROM jobs_browser_release_artifacts artifact
                 JOIN jobs_browser_release_artifact_runtime_components runtime
                   ON runtime.manifest_sha256 = artifact.manifest_sha256
                  AND runtime.artifact_id = artifact.artifact_id
                  AND runtime.build_descriptor_sha256 =
                      artifact.build_descriptor_sha256
                  AND runtime.artifact_sha256 = artifact.artifact_sha256
                  AND runtime.platform = artifact.platform
                  AND runtime.architecture = artifact.architecture
                  AND runtime.package_kind = artifact.package_kind
                WHERE artifact.manifest_sha256 = ?1
                ORDER BY artifact.artifact_id"#,
        )
        .map_err(browser_release_registry_storage)?;
    let stored = statement
        .query_map(
            params![verified.manifest_sha256],
            sqlite_stored_browser_release_artifact,
        )
        .map_err(browser_release_registry_storage)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(browser_release_registry_storage)?;
    let expected = expected_browser_release_artifacts(verified)?;
    if stored != expected {
        return Err(BrowserReleaseRegistryError::IdentityConflict);
    }
    Ok(())
}

fn require_exact_postgres_browser_release_artifacts(
    transaction: &mut postgres::Transaction<'_>,
    verified: &VerifiedBrowserReleaseManifestImport,
) -> Result<(), BrowserReleaseRegistryError> {
    let stored = transaction
        .query(
            r#"SELECT artifact.artifact_id, artifact.platform,
                      artifact.architecture, artifact.package_kind,
                      artifact.build_descriptor_sha256,
                      artifact.build_descriptor_base64url,
                      artifact.build_descriptor_signature_base64url,
                      artifact.build_descriptor_signing_key_id,
                      artifact.artifact_url, artifact.artifact_filename,
                      artifact.artifact_size_bytes, artifact.artifact_sha256,
                      artifact.app_content_sha256,
                      runtime.automation_bundle_sha256,
                      runtime.chromium_executable_sha256,
                      artifact.verification_evidence_sha256,
                      artifact.native_signature_kind, artifact.native_signer_identity
                 FROM jobs_browser_release_artifacts artifact
                 JOIN jobs_browser_release_artifact_runtime_components runtime
                   ON runtime.manifest_sha256 = artifact.manifest_sha256
                  AND runtime.artifact_id = artifact.artifact_id
                  AND runtime.build_descriptor_sha256 =
                      artifact.build_descriptor_sha256
                  AND runtime.artifact_sha256 = artifact.artifact_sha256
                  AND runtime.platform = artifact.platform
                  AND runtime.architecture = artifact.architecture
                  AND runtime.package_kind = artifact.package_kind
                WHERE artifact.manifest_sha256 = $1
                ORDER BY artifact.artifact_id"#,
            &[&verified.manifest_sha256],
        )
        .map_err(browser_release_registry_storage)?
        .into_iter()
        .map(postgres_stored_browser_release_artifact)
        .collect::<Vec<_>>();
    let expected = expected_browser_release_artifacts(verified)?;
    if stored != expected {
        return Err(BrowserReleaseRegistryError::IdentityConflict);
    }
    Ok(())
}

fn sqlite_browser_release_manifest_identity(
    transaction: &rusqlite::Transaction<'_>,
    manifest: &BrowserReleaseManifestAuthority,
) -> Result<Option<StoredBrowserReleaseManifest>, BrowserReleaseRegistryError> {
    transaction
        .query_row(
            "SELECT manifest_sha256, manifest_id, manifest_generation, \
                    canonical_manifest_base64url, authorization_signature_set_sha256, \
                    release_id, release_sequence, build_id, app_version, protocol_version \
             FROM jobs_browser_release_manifests \
             WHERE manifest_id = ?1 OR manifest_generation = ?2 OR release_id = ?3 \
                OR release_sequence = ?4 OR build_id = ?5 \
             LIMIT 1",
            params![
                manifest.manifest_id,
                manifest.manifest_generation,
                manifest.release_id,
                manifest.release_sequence,
                manifest.build_id,
            ],
            sqlite_stored_browser_release_manifest,
        )
        .optional()
        .map_err(browser_release_registry_storage)
}

fn postgres_browser_release_manifest_identity(
    transaction: &mut postgres::Transaction<'_>,
    manifest: &BrowserReleaseManifestAuthority,
) -> Result<Option<StoredBrowserReleaseManifest>, BrowserReleaseRegistryError> {
    Ok(transaction
        .query_opt(
            "SELECT manifest_sha256, manifest_id, manifest_generation, \
                    canonical_manifest_base64url, authorization_signature_set_sha256, \
                    release_id, release_sequence, build_id, app_version, protocol_version \
             FROM jobs_browser_release_manifests \
             WHERE manifest_id = $1 OR manifest_generation = $2 OR release_id = $3 \
                OR release_sequence = $4 OR build_id = $5 \
             LIMIT 1",
            &[
                &manifest.manifest_id,
                &manifest.manifest_generation,
                &manifest.release_id,
                &manifest.release_sequence,
                &manifest.build_id,
            ],
        )
        .map_err(browser_release_registry_storage)?
        .map(|row| StoredBrowserReleaseManifest {
            manifest_sha256: row.get(0),
            manifest_id: row.get(1),
            canonical_manifest_base64url: row.get(3),
            authorization_signature_set_sha256: row.get(4),
            release_id: row.get(5),
            release_sequence: row.get(6),
            build_id: row.get(7),
            app_version: row.get(8),
            protocol_version: row.get(9),
        }))
}

fn sqlite_stored_browser_release_manifest(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<StoredBrowserReleaseManifest> {
    Ok(StoredBrowserReleaseManifest {
        manifest_sha256: row.get(0)?,
        manifest_id: row.get(1)?,
        canonical_manifest_base64url: row.get(3)?,
        authorization_signature_set_sha256: row.get(4)?,
        release_id: row.get(5)?,
        release_sequence: row.get(6)?,
        build_id: row.get(7)?,
        app_version: row.get(8)?,
        protocol_version: row.get(9)?,
    })
}

fn ensure_sqlite_browser_artifact_identities_available(
    transaction: &rusqlite::Transaction<'_>,
    manifest: &BrowserReleaseManifestAuthority,
) -> Result<(), BrowserReleaseRegistryError> {
    for artifact in &manifest.artifacts {
        let exists = transaction
            .query_row(
                "SELECT 1 FROM jobs_browser_release_artifacts WHERE artifact_id = ?1",
                params![artifact.artifact_id],
                |_| Ok(()),
            )
            .optional()
            .map_err(browser_release_registry_storage)?
            .is_some();
        if exists {
            return Err(BrowserReleaseRegistryError::IdentityConflict);
        }
    }
    Ok(())
}

fn ensure_postgres_browser_artifact_identities_available(
    transaction: &mut postgres::Transaction<'_>,
    manifest: &BrowserReleaseManifestAuthority,
) -> Result<(), BrowserReleaseRegistryError> {
    for artifact in &manifest.artifacts {
        if transaction
            .query_opt(
                "SELECT 1 FROM jobs_browser_release_artifacts WHERE artifact_id = $1",
                &[&artifact.artifact_id],
            )
            .map_err(browser_release_registry_storage)?
            .is_some()
        {
            return Err(BrowserReleaseRegistryError::IdentityConflict);
        }
    }
    Ok(())
}

fn insert_sqlite_browser_release_manifest(
    transaction: &rusqlite::Transaction<'_>,
    verified: &VerifiedBrowserReleaseManifestImport,
    request: &BrowserReleaseManifestImportRequest,
    recorded_by: &str,
    recorded_at_ms: i64,
) -> Result<(), BrowserReleaseRegistryError> {
    let manifest = &verified.manifest;
    transaction
        .execute(
            "INSERT INTO jobs_browser_release_manifests ( \
               manifest_sha256, manifest_id, manifest_generation, release_id, \
               release_sequence, build_id, app_version, protocol_version, source_commit, \
               electron_version, playwright_version, chromium_revision, release_notes_url, \
               artifact_count, canonical_manifest_base64url, \
               authorization_signature_set_sha256, published_at_ms, recorded_by, recorded_at_ms \
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, \
                       ?13, ?14, ?15, ?16, ?17, ?18, ?19)",
            params![
                verified.manifest_sha256,
                manifest.manifest_id,
                manifest.manifest_generation,
                manifest.release_id,
                manifest.release_sequence,
                manifest.build_id,
                manifest.app_version,
                manifest.protocol_version,
                manifest.source_commit,
                manifest.electron_version,
                manifest.playwright_version,
                manifest.chromium_revision,
                manifest.release_notes_url,
                i64::try_from(manifest.artifacts.len())
                    .map_err(|_| BrowserReleaseRegistryError::InvalidAuthority)?,
                request.canonical_base64url,
                verified.signature_set_sha256,
                manifest.published_at_ms,
                recorded_by,
                recorded_at_ms,
            ],
        )
        .map_err(browser_release_registry_storage)?;
    for artifact in &manifest.artifacts {
        let descriptor = verified
            .descriptors
            .get(&format!("{}:{}", artifact.platform, artifact.architecture))
            .ok_or(BrowserReleaseRegistryError::InvalidAuthority)?;
        let filename = browser_release_artifact_filename(&artifact.url)?;
        transaction
            .execute(
                "INSERT INTO jobs_browser_release_artifacts ( \
                   artifact_id, manifest_sha256, platform, architecture, package_kind, \
                   build_descriptor_sha256, build_descriptor_base64url, \
                   build_descriptor_signature_base64url, build_descriptor_signing_key_id, \
                   artifact_url, artifact_filename, artifact_size_bytes, artifact_sha256, \
                   app_content_sha256, verification_evidence_sha256, native_signature_kind, \
                   native_signer_identity, recorded_at_ms \
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, \
                           ?13, ?14, ?15, ?16, ?17, ?18)",
                params![
                    artifact.artifact_id,
                    verified.manifest_sha256,
                    artifact.platform,
                    artifact.architecture,
                    artifact.package_kind,
                    descriptor.descriptor_sha256,
                    descriptor.descriptor_base64url,
                    descriptor.signature_base64url,
                    descriptor.signing_key_id,
                    artifact.url,
                    filename,
                    artifact.size_bytes,
                    artifact.sha256,
                    artifact.app_content_sha256,
                    artifact.verification_evidence_sha256,
                    artifact.native_signature_kind,
                    artifact.native_signer_identity,
                    recorded_at_ms,
                ],
            )
            .map_err(browser_release_registry_storage)?;
        transaction
            .execute(
                "INSERT INTO jobs_browser_release_artifact_runtime_components ( \
                   manifest_sha256, artifact_id, build_descriptor_sha256, \
                   artifact_sha256, platform, architecture, package_kind, \
                   automation_bundle_sha256, chromium_executable_sha256, recorded_at_ms \
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    verified.manifest_sha256,
                    artifact.artifact_id,
                    descriptor.descriptor_sha256,
                    artifact.sha256,
                    artifact.platform,
                    artifact.architecture,
                    artifact.package_kind,
                    artifact.automation_bundle_sha256,
                    artifact.chromium_executable_sha256,
                    recorded_at_ms,
                ],
            )
            .map_err(browser_release_registry_storage)?;
    }
    Ok(())
}

fn insert_postgres_browser_release_manifest(
    transaction: &mut postgres::Transaction<'_>,
    verified: &VerifiedBrowserReleaseManifestImport,
    request: &BrowserReleaseManifestImportRequest,
    recorded_by: &str,
    recorded_at_ms: i64,
) -> Result<(), BrowserReleaseRegistryError> {
    let manifest = &verified.manifest;
    let artifact_count = i64::try_from(manifest.artifacts.len())
        .map_err(|_| BrowserReleaseRegistryError::InvalidAuthority)?;
    transaction
        .execute(
            "INSERT INTO jobs_browser_release_manifests ( \
               manifest_sha256, manifest_id, manifest_generation, release_id, \
               release_sequence, build_id, app_version, protocol_version, source_commit, \
               electron_version, playwright_version, chromium_revision, release_notes_url, \
               artifact_count, canonical_manifest_base64url, \
               authorization_signature_set_sha256, published_at_ms, recorded_by, recorded_at_ms \
             ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, \
                       $13, $14, $15, $16, $17, $18, $19)",
            &[
                &verified.manifest_sha256,
                &manifest.manifest_id,
                &manifest.manifest_generation,
                &manifest.release_id,
                &manifest.release_sequence,
                &manifest.build_id,
                &manifest.app_version,
                &manifest.protocol_version,
                &manifest.source_commit,
                &manifest.electron_version,
                &manifest.playwright_version,
                &manifest.chromium_revision,
                &manifest.release_notes_url,
                &artifact_count,
                &request.canonical_base64url,
                &verified.signature_set_sha256,
                &manifest.published_at_ms,
                &recorded_by,
                &recorded_at_ms,
            ],
        )
        .map_err(browser_release_registry_storage)?;
    for artifact in &manifest.artifacts {
        let descriptor = verified
            .descriptors
            .get(&format!("{}:{}", artifact.platform, artifact.architecture))
            .ok_or(BrowserReleaseRegistryError::InvalidAuthority)?;
        let filename = browser_release_artifact_filename(&artifact.url)?;
        transaction
            .execute(
                "INSERT INTO jobs_browser_release_artifacts ( \
                   artifact_id, manifest_sha256, platform, architecture, package_kind, \
                   build_descriptor_sha256, build_descriptor_base64url, \
                   build_descriptor_signature_base64url, build_descriptor_signing_key_id, \
                   artifact_url, artifact_filename, artifact_size_bytes, artifact_sha256, \
                   app_content_sha256, verification_evidence_sha256, native_signature_kind, \
                   native_signer_identity, recorded_at_ms \
                 ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, \
                           $13, $14, $15, $16, $17, $18)",
                &[
                    &artifact.artifact_id,
                    &verified.manifest_sha256,
                    &artifact.platform,
                    &artifact.architecture,
                    &artifact.package_kind,
                    &descriptor.descriptor_sha256,
                    &descriptor.descriptor_base64url,
                    &descriptor.signature_base64url,
                    &descriptor.signing_key_id,
                    &artifact.url,
                    &filename,
                    &artifact.size_bytes,
                    &artifact.sha256,
                    &artifact.app_content_sha256,
                    &artifact.verification_evidence_sha256,
                    &artifact.native_signature_kind,
                    &artifact.native_signer_identity,
                    &recorded_at_ms,
                ],
            )
            .map_err(browser_release_registry_storage)?;
        transaction
            .execute(
                "INSERT INTO jobs_browser_release_artifact_runtime_components ( \
                   manifest_sha256, artifact_id, build_descriptor_sha256, \
                   artifact_sha256, platform, architecture, package_kind, \
                   automation_bundle_sha256, chromium_executable_sha256, recorded_at_ms \
                 ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
                &[
                    &verified.manifest_sha256,
                    &artifact.artifact_id,
                    &descriptor.descriptor_sha256,
                    &artifact.sha256,
                    &artifact.platform,
                    &artifact.architecture,
                    &artifact.package_kind,
                    &artifact.automation_bundle_sha256,
                    &artifact.chromium_executable_sha256,
                    &recorded_at_ms,
                ],
            )
            .map_err(browser_release_registry_storage)?;
    }
    Ok(())
}

fn browser_release_artifact_filename(url: &str) -> Result<String, BrowserReleaseRegistryError> {
    let filename = url.rsplit('/').next().unwrap_or_default();
    if filename.is_empty() || filename.len() > 255 {
        return Err(BrowserReleaseRegistryError::InvalidAuthority);
    }
    Ok(filename.to_string())
}

#[derive(Debug, Clone)]
struct StoredBrowserReleaseActivation {
    activation_sha256: String,
    activation_id: String,
    activation_generation: i64,
    trust_generation: i64,
    channel: String,
    channel_sequence: i64,
    manifest_sha256: String,
    manifest_signature_set_sha256: String,
    authorization_signature_set_sha256: String,
    accepted_server_release_ids_json: String,
    canary_evidence_sha256: String,
    canonical_activation_base64url: String,
    issued_at_ms: i64,
    expires_at_ms: i64,
}

fn browser_release_activation_import_result(
    activation: &BrowserReleaseActivationAuthority,
    activation_sha256: &str,
    signature_set_sha256: &str,
    policy_sha256: &str,
    replayed: bool,
) -> BrowserReleaseImportResult {
    BrowserReleaseImportResult {
        authority_kind: "activation".to_string(),
        authority_id: activation.activation_id.clone(),
        authority_sha256: activation_sha256.to_string(),
        signature_set_sha256: signature_set_sha256.to_string(),
        trust_policy_sha256: policy_sha256.to_string(),
        manifest_authorization_signature_set_sha256: Some(activation.signature_set_sha256.clone()),
        activation_authorization_signature_set_sha256: Some(signature_set_sha256.to_string()),
        replayed,
    }
}

fn require_browser_release_activation_manifest_binding(
    activation: &BrowserReleaseActivationAuthority,
    manifest: &StoredBrowserReleaseManifest,
) -> Result<(), BrowserReleaseRegistryError> {
    if activation.manifest_sha256 != manifest.manifest_sha256
        || activation.signature_set_sha256 != manifest.authorization_signature_set_sha256
    {
        return Err(BrowserReleaseRegistryError::InvalidAuthority);
    }
    Ok(())
}

fn require_stored_browser_release_manifest_artifact_origin(
    manifest: &StoredBrowserReleaseManifest,
    policy: &StoredBrowserReleaseTrustPolicy,
) -> Result<(), BrowserReleaseRegistryError> {
    let canonical = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(&manifest.canonical_manifest_base64url)
        .map_err(browser_release_registry_storage)?;
    let authority = parse_canonical_browser_release_manifest(&canonical)?;
    if browser_release_authority_sha256(&canonical) != manifest.manifest_sha256
        || authority.manifest_id != manifest.manifest_id
        || authority.release_id != manifest.release_id
        || authority.release_sequence != manifest.release_sequence
        || authority.build_id != manifest.build_id
        || authority.app_version != manifest.app_version
        || authority.protocol_version != manifest.protocol_version
    {
        return Err(browser_release_registry_storage(anyhow::anyhow!(
            "stored Browser manifest does not match its canonical authority"
        )));
    }
    require_browser_release_manifest_artifact_origin(&authority, &policy.policy)?;
    Ok(())
}

fn require_exact_browser_release_activation_replay(
    existing: &StoredBrowserReleaseActivation,
    activation: &BrowserReleaseActivationAuthority,
    activation_sha256: &str,
    canonical_activation_base64url: &str,
    signature_set_sha256: &str,
) -> Result<(), BrowserReleaseRegistryError> {
    let accepted_server_release_ids_json =
        serde_json::to_string(&activation.accepted_server_release_ids)
            .map_err(browser_release_registry_storage)?;
    if existing.activation_sha256 != activation_sha256
        || existing.activation_id != activation.activation_id
        || existing.activation_generation != activation.activation_generation
        || existing.trust_generation != activation.trust_generation
        || existing.channel != activation.channel
        || existing.channel_sequence != activation.channel_sequence
        || existing.manifest_sha256 != activation.manifest_sha256
        || existing.manifest_signature_set_sha256 != activation.signature_set_sha256
        || existing.canonical_activation_base64url != canonical_activation_base64url
        || existing.authorization_signature_set_sha256 != signature_set_sha256
        || existing.accepted_server_release_ids_json != accepted_server_release_ids_json
        || existing.canary_evidence_sha256 != activation.canary_evidence_sha256
        || existing.issued_at_ms != activation.issued_at_ms
        || existing.expires_at_ms != activation.expires_at_ms
    {
        return Err(BrowserReleaseRegistryError::IdentityConflict);
    }
    Ok(())
}

fn sqlite_browser_release_manifest_by_sha256(
    transaction: &rusqlite::Transaction<'_>,
    manifest_sha256: &str,
) -> Result<Option<StoredBrowserReleaseManifest>, BrowserReleaseRegistryError> {
    transaction
        .query_row(
            "SELECT manifest_sha256, manifest_id, manifest_generation, \
                    canonical_manifest_base64url, authorization_signature_set_sha256, \
                    release_id, release_sequence, build_id, app_version, protocol_version \
             FROM jobs_browser_release_manifests WHERE manifest_sha256 = ?1",
            params![manifest_sha256],
            sqlite_stored_browser_release_manifest,
        )
        .optional()
        .map_err(browser_release_registry_storage)
}

fn postgres_browser_release_manifest_by_sha256(
    transaction: &mut postgres::Transaction<'_>,
    manifest_sha256: &str,
) -> Result<Option<StoredBrowserReleaseManifest>, BrowserReleaseRegistryError> {
    transaction
        .query_opt(
            "SELECT manifest_sha256, manifest_id, manifest_generation, \
                    canonical_manifest_base64url, authorization_signature_set_sha256, \
                    release_id, release_sequence, build_id, app_version, protocol_version \
             FROM jobs_browser_release_manifests WHERE manifest_sha256 = $1",
            &[&manifest_sha256],
        )
        .map_err(browser_release_registry_storage)
        .map(|result| {
            result.map(|row| StoredBrowserReleaseManifest {
                manifest_sha256: row.get(0),
                manifest_id: row.get(1),
                canonical_manifest_base64url: row.get(3),
                authorization_signature_set_sha256: row.get(4),
                release_id: row.get(5),
                release_sequence: row.get(6),
                build_id: row.get(7),
                app_version: row.get(8),
                protocol_version: row.get(9),
            })
        })
}

fn sqlite_browser_release_activation_identity(
    transaction: &rusqlite::Transaction<'_>,
    activation: &BrowserReleaseActivationAuthority,
) -> Result<Option<StoredBrowserReleaseActivation>, BrowserReleaseRegistryError> {
    transaction
        .query_row(
            "SELECT activation_sha256, activation_id, activation_generation, trust_generation, \
                    channel, channel_sequence, manifest_sha256, manifest_signature_set_sha256, \
                    authorization_signature_set_sha256, accepted_server_release_ids_json, \
                    canary_evidence_sha256, canonical_activation_base64url, issued_at_ms, \
                    expires_at_ms \
             FROM jobs_browser_release_activations \
             WHERE activation_id = ?1 OR activation_generation = ?2 \
                OR (channel = ?3 AND trust_generation = ?4 AND channel_sequence = ?5) \
             LIMIT 1",
            params![
                activation.activation_id,
                activation.activation_generation,
                activation.channel,
                activation.trust_generation,
                activation.channel_sequence,
            ],
            sqlite_stored_browser_release_activation,
        )
        .optional()
        .map_err(browser_release_registry_storage)
}

fn postgres_browser_release_activation_identity(
    transaction: &mut postgres::Transaction<'_>,
    activation: &BrowserReleaseActivationAuthority,
) -> Result<Option<StoredBrowserReleaseActivation>, BrowserReleaseRegistryError> {
    transaction
        .query_opt(
            "SELECT activation_sha256, activation_id, activation_generation, trust_generation, \
                    channel, channel_sequence, manifest_sha256, manifest_signature_set_sha256, \
                    authorization_signature_set_sha256, accepted_server_release_ids_json, \
                    canary_evidence_sha256, canonical_activation_base64url, issued_at_ms, \
                    expires_at_ms \
             FROM jobs_browser_release_activations \
             WHERE activation_id = $1 OR activation_generation = $2 \
                OR (channel = $3 AND trust_generation = $4 AND channel_sequence = $5) \
             LIMIT 1",
            &[
                &activation.activation_id,
                &activation.activation_generation,
                &activation.channel,
                &activation.trust_generation,
                &activation.channel_sequence,
            ],
        )
        .map_err(browser_release_registry_storage)?
        .map(|row| postgres_stored_browser_release_activation(&row))
        .transpose()
}

fn sqlite_stored_browser_release_activation(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<StoredBrowserReleaseActivation> {
    Ok(StoredBrowserReleaseActivation {
        activation_sha256: row.get(0)?,
        activation_id: row.get(1)?,
        activation_generation: row.get(2)?,
        trust_generation: row.get(3)?,
        channel: row.get(4)?,
        channel_sequence: row.get(5)?,
        manifest_sha256: row.get(6)?,
        manifest_signature_set_sha256: row.get(7)?,
        authorization_signature_set_sha256: row.get(8)?,
        accepted_server_release_ids_json: row.get(9)?,
        canary_evidence_sha256: row.get(10)?,
        canonical_activation_base64url: row.get(11)?,
        issued_at_ms: row.get(12)?,
        expires_at_ms: row.get(13)?,
    })
}

fn postgres_stored_browser_release_activation(
    row: &postgres::Row,
) -> Result<StoredBrowserReleaseActivation, BrowserReleaseRegistryError> {
    Ok(StoredBrowserReleaseActivation {
        activation_sha256: row.get(0),
        activation_id: row.get(1),
        activation_generation: row.get(2),
        trust_generation: row.get(3),
        channel: row.get(4),
        channel_sequence: row.get(5),
        manifest_sha256: row.get(6),
        manifest_signature_set_sha256: row.get(7),
        authorization_signature_set_sha256: row.get(8),
        accepted_server_release_ids_json: row.get(9),
        canary_evidence_sha256: row.get(10),
        canonical_activation_base64url: row.get(11),
        issued_at_ms: row.get(12),
        expires_at_ms: row.get(13),
    })
}

fn insert_sqlite_browser_release_activation(
    transaction: &rusqlite::Transaction<'_>,
    activation: &BrowserReleaseActivationAuthority,
    activation_sha256: &str,
    authorization_signature_set_sha256: &str,
    canonical_activation_base64url: &str,
    recorded_by: &str,
    recorded_at_ms: i64,
) -> Result<(), BrowserReleaseRegistryError> {
    let accepted_server_release_ids_json =
        serde_json::to_string(&activation.accepted_server_release_ids)
            .map_err(browser_release_registry_storage)?;
    transaction
        .execute(
            "INSERT INTO jobs_browser_release_activations ( \
               activation_sha256, activation_id, activation_generation, trust_generation, \
               channel, channel_sequence, manifest_sha256, manifest_signature_set_sha256, \
               authorization_signature_set_sha256, accepted_server_release_ids_json, \
               canary_evidence_sha256, canonical_activation_base64url, issued_at_ms, \
               expires_at_ms, recorded_by, recorded_at_ms \
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, \
                       ?13, ?14, ?15, ?16)",
            params![
                activation_sha256,
                activation.activation_id,
                activation.activation_generation,
                activation.trust_generation,
                activation.channel,
                activation.channel_sequence,
                activation.manifest_sha256,
                activation.signature_set_sha256,
                authorization_signature_set_sha256,
                accepted_server_release_ids_json,
                activation.canary_evidence_sha256,
                canonical_activation_base64url,
                activation.issued_at_ms,
                activation.expires_at_ms,
                recorded_by,
                recorded_at_ms,
            ],
        )
        .map_err(browser_release_registry_storage)?;
    Ok(())
}

fn insert_postgres_browser_release_activation(
    transaction: &mut postgres::Transaction<'_>,
    activation: &BrowserReleaseActivationAuthority,
    activation_sha256: &str,
    authorization_signature_set_sha256: &str,
    canonical_activation_base64url: &str,
    recorded_by: &str,
    recorded_at_ms: i64,
) -> Result<(), BrowserReleaseRegistryError> {
    let accepted_server_release_ids_json =
        serde_json::to_string(&activation.accepted_server_release_ids)
            .map_err(browser_release_registry_storage)?;
    transaction
        .execute(
            "INSERT INTO jobs_browser_release_activations ( \
               activation_sha256, activation_id, activation_generation, trust_generation, \
               channel, channel_sequence, manifest_sha256, manifest_signature_set_sha256, \
               authorization_signature_set_sha256, accepted_server_release_ids_json, \
               canary_evidence_sha256, canonical_activation_base64url, issued_at_ms, \
               expires_at_ms, recorded_by, recorded_at_ms \
             ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, \
                       $13, $14, $15, $16)",
            &[
                &activation_sha256,
                &activation.activation_id,
                &activation.activation_generation,
                &activation.trust_generation,
                &activation.channel,
                &activation.channel_sequence,
                &activation.manifest_sha256,
                &activation.signature_set_sha256,
                &authorization_signature_set_sha256,
                &accepted_server_release_ids_json,
                &activation.canary_evidence_sha256,
                &canonical_activation_base64url,
                &activation.issued_at_ms,
                &activation.expires_at_ms,
                &recorded_by,
                &recorded_at_ms,
            ],
        )
        .map_err(browser_release_registry_storage)?;
    Ok(())
}

fn ensure_sqlite_browser_release_manifest_not_revoked(
    transaction: &rusqlite::Transaction<'_>,
    manifest: &StoredBrowserReleaseManifest,
) -> Result<(), BrowserReleaseRegistryError> {
    let release_sha256 = browser_release_authority_sha256(manifest.release_id.as_bytes());
    let revoked = transaction
        .query_row(
            "SELECT 1 FROM jobs_browser_release_revocations r \
             WHERE (r.subject_kind = 'manifest' AND r.subject_id = ?1 \
                    AND r.subject_sha256 = ?2) \
                OR (r.subject_kind = 'release' AND r.subject_id = ?3 \
                    AND r.subject_sha256 = ?4) \
                OR (r.subject_kind = 'artifact' AND EXISTS ( \
                      SELECT 1 FROM jobs_browser_release_artifacts a \
                      WHERE a.manifest_sha256 = ?2 AND a.artifact_id = r.subject_id \
                        AND a.artifact_sha256 = r.subject_sha256)) \
                OR (r.subject_kind = 'build-descriptor' AND EXISTS ( \
                      SELECT 1 FROM jobs_browser_release_artifacts a \
                      WHERE a.manifest_sha256 = ?2 \
                        AND a.build_descriptor_sha256 = r.subject_id \
                        AND a.build_descriptor_sha256 = r.subject_sha256)) \
                OR (r.subject_kind = 'signing-key' AND ( \
                      r.subject_id IN ( \
                        SELECT key_id FROM jobs_browser_release_signatures \
                        WHERE signature_set_sha256 = ?5) \
                      OR r.subject_id IN ( \
                        SELECT build_descriptor_signing_key_id \
                        FROM jobs_browser_release_artifacts \
                        WHERE manifest_sha256 = ?2))) \
             LIMIT 1",
            params![
                manifest.manifest_id,
                manifest.manifest_sha256,
                manifest.release_id,
                release_sha256,
                manifest.authorization_signature_set_sha256,
            ],
            |_| Ok(()),
        )
        .optional()
        .map_err(browser_release_registry_storage)?
        .is_some();
    if revoked {
        return Err(BrowserReleaseRegistryError::Revoked);
    }
    Ok(())
}

fn ensure_postgres_browser_release_manifest_not_revoked(
    transaction: &mut postgres::Transaction<'_>,
    manifest: &StoredBrowserReleaseManifest,
) -> Result<(), BrowserReleaseRegistryError> {
    let release_sha256 = browser_release_authority_sha256(manifest.release_id.as_bytes());
    let revoked = transaction
        .query_opt(
            "SELECT 1 FROM jobs_browser_release_revocations r \
             WHERE (r.subject_kind = 'manifest' AND r.subject_id = $1 \
                    AND r.subject_sha256 = $2) \
                OR (r.subject_kind = 'release' AND r.subject_id = $3 \
                    AND r.subject_sha256 = $4) \
                OR (r.subject_kind = 'artifact' AND EXISTS ( \
                      SELECT 1 FROM jobs_browser_release_artifacts a \
                      WHERE a.manifest_sha256 = $2 AND a.artifact_id = r.subject_id \
                        AND a.artifact_sha256 = r.subject_sha256)) \
                OR (r.subject_kind = 'build-descriptor' AND EXISTS ( \
                      SELECT 1 FROM jobs_browser_release_artifacts a \
                      WHERE a.manifest_sha256 = $2 \
                        AND a.build_descriptor_sha256 = r.subject_id \
                        AND a.build_descriptor_sha256 = r.subject_sha256)) \
                OR (r.subject_kind = 'signing-key' AND ( \
                      r.subject_id IN ( \
                        SELECT key_id FROM jobs_browser_release_signatures \
                        WHERE signature_set_sha256 = $5) \
                      OR r.subject_id IN ( \
                        SELECT build_descriptor_signing_key_id \
                        FROM jobs_browser_release_artifacts \
                        WHERE manifest_sha256 = $2))) \
             LIMIT 1",
            &[
                &manifest.manifest_id,
                &manifest.manifest_sha256,
                &manifest.release_id,
                &release_sha256,
                &manifest.authorization_signature_set_sha256,
            ],
        )
        .map_err(browser_release_registry_storage)?
        .is_some();
    if revoked {
        return Err(BrowserReleaseRegistryError::Revoked);
    }
    Ok(())
}

pub fn apply_browser_release_activation(
    pool: &DbPool,
    request: &ApplyBrowserReleaseActivationRequest,
    recorded_by: &str,
) -> Result<BrowserReleaseChannelStatus, BrowserReleaseRegistryError> {
    validate_browser_release_activation_apply_request(request)?;
    validate_browser_release_recorded_by(recorded_by)?;
    let recorded_at_ms = now_ms();
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut connection = pool.get().map_err(browser_release_registry_storage)?;
            let transaction = connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(browser_release_registry_storage)?;
            let activation = sqlite_browser_release_activation_by_sha256(
                &transaction,
                &request.activation_sha256,
            )?
            .ok_or(BrowserReleaseRegistryError::NotFound)?;
            let head = sqlite_browser_release_channel_head(&transaction, &activation.channel)?;
            if browser_release_activation_apply_is_exact_replay_sqlite(
                &transaction,
                request,
                head.as_ref(),
            )? {
                let status = sqlite_browser_release_channel_status_tx(
                    &transaction,
                    &activation.channel,
                    recorded_at_ms,
                )?;
                transaction
                    .commit()
                    .map_err(browser_release_registry_storage)?;
                return Ok(status);
            }
            let policy = sqlite_latest_browser_release_policy(&transaction)?
                .ok_or(BrowserReleaseRegistryError::NotFound)?;
            require_browser_release_activation_current_policy(
                &activation,
                &policy,
                recorded_at_ms,
            )?;
            let manifest = sqlite_browser_release_manifest_by_sha256(
                &transaction,
                &activation.manifest_sha256,
            )?
            .ok_or(BrowserReleaseRegistryError::NotFound)?;
            require_stored_browser_release_activation_manifest_binding(&activation, &manifest)?;
            require_stored_browser_release_manifest_artifact_origin(&manifest, &policy)?;
            ensure_sqlite_browser_release_target_not_revoked(
                &transaction,
                &activation,
                &manifest,
                &policy,
            )?;
            require_browser_release_apply_compare_and_swap(request, head.as_ref())?;
            require_browser_release_activation_progression_sqlite(
                &transaction,
                &activation,
                &manifest,
                head.as_ref(),
            )?;
            let transition = BrowserReleaseChannelTransition::new(
                head.as_ref(),
                &activation,
                "activation",
                &activation.activation_sha256,
                None,
                recorded_by,
                recorded_at_ms,
            )?;
            insert_sqlite_browser_release_channel_transition(&transaction, &transition)?;
            apply_sqlite_browser_release_channel_head(&transaction, head.as_ref(), &transition)?;
            let status = sqlite_browser_release_channel_status_tx(
                &transaction,
                &activation.channel,
                recorded_at_ms,
            )?;
            transaction
                .commit()
                .map_err(browser_release_registry_storage)?;
            Ok(status)
        }
        DbPool::Postgres(_) => {
            let mut connection = pool.get_pg().map_err(browser_release_registry_storage)?;
            let mut transaction = connection
                .transaction()
                .map_err(browser_release_registry_storage)?;
            let activation = postgres_browser_release_activation_by_sha256(
                &mut transaction,
                &request.activation_sha256,
            )?
            .ok_or(BrowserReleaseRegistryError::NotFound)?;
            lock_postgres_browser_release_mutation(&mut transaction, &activation.channel)?;
            let head = postgres_browser_release_channel_head_for_update(
                &mut transaction,
                &activation.channel,
            )?;
            if browser_release_activation_apply_is_exact_replay_postgres(
                &mut transaction,
                request,
                head.as_ref(),
            )? {
                let status = postgres_browser_release_channel_status_tx(
                    &mut transaction,
                    &activation.channel,
                    recorded_at_ms,
                )?;
                transaction
                    .commit()
                    .map_err(browser_release_registry_storage)?;
                return Ok(status);
            }
            let policy = postgres_latest_browser_release_policy(&mut transaction)?
                .ok_or(BrowserReleaseRegistryError::NotFound)?;
            require_browser_release_activation_current_policy(
                &activation,
                &policy,
                recorded_at_ms,
            )?;
            let manifest = postgres_browser_release_manifest_by_sha256(
                &mut transaction,
                &activation.manifest_sha256,
            )?
            .ok_or(BrowserReleaseRegistryError::NotFound)?;
            require_stored_browser_release_activation_manifest_binding(&activation, &manifest)?;
            require_stored_browser_release_manifest_artifact_origin(&manifest, &policy)?;
            ensure_postgres_browser_release_target_not_revoked(
                &mut transaction,
                &activation,
                &manifest,
                &policy,
            )?;
            require_browser_release_apply_compare_and_swap(request, head.as_ref())?;
            require_browser_release_activation_progression_postgres(
                &mut transaction,
                &activation,
                &manifest,
                head.as_ref(),
            )?;
            let transition = BrowserReleaseChannelTransition::new(
                head.as_ref(),
                &activation,
                "activation",
                &activation.activation_sha256,
                None,
                recorded_by,
                recorded_at_ms,
            )?;
            insert_postgres_browser_release_channel_transition(&mut transaction, &transition)?;
            apply_postgres_browser_release_channel_head(
                &mut transaction,
                head.as_ref(),
                &transition,
            )?;
            let status = postgres_browser_release_channel_status_tx(
                &mut transaction,
                &activation.channel,
                recorded_at_ms,
            )?;
            transaction
                .commit()
                .map_err(browser_release_registry_storage)?;
            Ok(status)
        }
    })
}

fn validate_browser_release_activation_apply_request(
    request: &ApplyBrowserReleaseActivationRequest,
) -> Result<(), BrowserReleaseRegistryError> {
    if !browser_release_registry_sha256(&request.activation_sha256)
        || !browser_release_registry_non_negative_integer(request.expected_head_revision)
        || request
            .expected_transition_sha256
            .as_deref()
            .is_some_and(|value| !browser_release_registry_sha256(value))
        || (request.expected_head_revision == 0) != request.expected_transition_sha256.is_none()
    {
        return Err(BrowserReleaseRegistryError::InvalidRequest);
    }
    Ok(())
}

fn require_browser_release_activation_current_policy(
    activation: &StoredBrowserReleaseActivation,
    policy: &StoredBrowserReleaseTrustPolicy,
    verification_time_ms: i64,
) -> Result<(), BrowserReleaseRegistryError> {
    if activation.trust_generation != policy.trust_generation {
        return Err(BrowserReleaseRegistryError::SequenceRegression);
    }
    if policy.policy.valid_from_ms > verification_time_ms
        || policy.policy.expires_at_ms <= verification_time_ms
        || activation.expires_at_ms <= verification_time_ms
    {
        return Err(BrowserReleaseRegistryError::InvalidAuthority);
    }
    Ok(())
}

fn require_stored_browser_release_activation_manifest_binding(
    activation: &StoredBrowserReleaseActivation,
    manifest: &StoredBrowserReleaseManifest,
) -> Result<(), BrowserReleaseRegistryError> {
    if activation.manifest_sha256 != manifest.manifest_sha256
        || activation.manifest_signature_set_sha256 != manifest.authorization_signature_set_sha256
    {
        return Err(browser_release_registry_storage(anyhow::anyhow!(
            "stored Browser activation does not match its manifest"
        )));
    }
    Ok(())
}

fn require_browser_release_apply_compare_and_swap(
    request: &ApplyBrowserReleaseActivationRequest,
    head: Option<&StoredBrowserReleaseChannelHead>,
) -> Result<(), BrowserReleaseRegistryError> {
    let matches = match head {
        Some(head) => {
            request.expected_head_revision == head.head_revision
                && request.expected_transition_sha256.as_deref()
                    == Some(head.transition_sha256.as_str())
        }
        None => request.expected_head_revision == 0 && request.expected_transition_sha256.is_none(),
    };
    if !matches {
        return Err(BrowserReleaseRegistryError::CompareAndSwapConflict);
    }
    Ok(())
}

fn sqlite_browser_release_activation_by_sha256(
    transaction: &rusqlite::Transaction<'_>,
    activation_sha256: &str,
) -> Result<Option<StoredBrowserReleaseActivation>, BrowserReleaseRegistryError> {
    transaction
        .query_row(
            r#"SELECT activation_sha256, activation_id, activation_generation,
                      trust_generation, channel, channel_sequence, manifest_sha256,
                      manifest_signature_set_sha256,
                      authorization_signature_set_sha256,
                      accepted_server_release_ids_json, canary_evidence_sha256,
                      canonical_activation_base64url, issued_at_ms, expires_at_ms
                 FROM jobs_browser_release_activations
                WHERE activation_sha256 = ?1"#,
            params![activation_sha256],
            sqlite_stored_browser_release_activation,
        )
        .optional()
        .map_err(browser_release_registry_storage)
}

fn postgres_browser_release_activation_by_sha256(
    transaction: &mut postgres::Transaction<'_>,
    activation_sha256: &str,
) -> Result<Option<StoredBrowserReleaseActivation>, BrowserReleaseRegistryError> {
    transaction
        .query_opt(
            r#"SELECT activation_sha256, activation_id, activation_generation,
                      trust_generation, channel, channel_sequence, manifest_sha256,
                      manifest_signature_set_sha256,
                      authorization_signature_set_sha256,
                      accepted_server_release_ids_json, canary_evidence_sha256,
                      canonical_activation_base64url, issued_at_ms, expires_at_ms
                 FROM jobs_browser_release_activations
                WHERE activation_sha256 = $1"#,
            &[&activation_sha256],
        )
        .map_err(browser_release_registry_storage)?
        .map(|row| postgres_stored_browser_release_activation(&row))
        .transpose()
}

fn sqlite_browser_release_channel_head(
    transaction: &rusqlite::Transaction<'_>,
    channel: &str,
) -> Result<Option<StoredBrowserReleaseChannelHead>, BrowserReleaseRegistryError> {
    transaction
        .query_row(
            r#"SELECT channel, head_revision, current_transition_sha256,
                      current_activation_sha256, current_manifest_sha256,
                      current_trust_generation, current_channel_sequence
                 FROM jobs_browser_release_channel_heads
                WHERE channel = ?1"#,
            params![channel],
            |row| {
                Ok(StoredBrowserReleaseChannelHead {
                    channel: row.get(0)?,
                    head_revision: row.get(1)?,
                    transition_sha256: row.get(2)?,
                    activation_sha256: row.get(3)?,
                    manifest_sha256: row.get(4)?,
                    trust_generation: row.get(5)?,
                    channel_sequence: row.get(6)?,
                })
            },
        )
        .optional()
        .map_err(browser_release_registry_storage)
}

fn lock_postgres_browser_release_channel(
    transaction: &mut postgres::Transaction<'_>,
    channel: &str,
) -> Result<(), BrowserReleaseRegistryError> {
    let lock_key = format!("jobs-browser-release-channel:{channel}");
    transaction
        .query_one(
            "SELECT pg_advisory_xact_lock(hashtextextended($1, 0))",
            &[&lock_key],
        )
        .map_err(browser_release_registry_storage)?;
    Ok(())
}

fn lock_postgres_browser_release_registry(
    transaction: &mut postgres::Transaction<'_>,
) -> Result<(), BrowserReleaseRegistryError> {
    transaction
        .query_one(
            "SELECT pg_advisory_xact_lock( \
             hashtextextended('jobs-browser-release-registry', 0))",
            &[],
        )
        .map_err(browser_release_registry_storage)?;
    Ok(())
}

fn lock_postgres_browser_release_mutation(
    transaction: &mut postgres::Transaction<'_>,
    channel: &str,
) -> Result<(), BrowserReleaseRegistryError> {
    lock_postgres_browser_release_registry(transaction)?;
    lock_postgres_browser_release_channel(transaction, channel)
}

fn postgres_browser_release_channel_head_for_update(
    transaction: &mut postgres::Transaction<'_>,
    channel: &str,
) -> Result<Option<StoredBrowserReleaseChannelHead>, BrowserReleaseRegistryError> {
    transaction
        .query_opt(
            r#"SELECT channel, head_revision, current_transition_sha256,
                      current_activation_sha256, current_manifest_sha256,
                      current_trust_generation, current_channel_sequence
                 FROM jobs_browser_release_channel_heads
                WHERE channel = $1
                FOR UPDATE"#,
            &[&channel],
        )
        .map_err(browser_release_registry_storage)
        .map(|row| {
            row.map(|row| StoredBrowserReleaseChannelHead {
                channel: row.get(0),
                head_revision: row.get(1),
                transition_sha256: row.get(2),
                activation_sha256: row.get(3),
                manifest_sha256: row.get(4),
                trust_generation: row.get(5),
                channel_sequence: row.get(6),
            })
        })
}

fn browser_release_activation_apply_is_exact_replay_sqlite(
    transaction: &rusqlite::Transaction<'_>,
    request: &ApplyBrowserReleaseActivationRequest,
    head: Option<&StoredBrowserReleaseChannelHead>,
) -> Result<bool, BrowserReleaseRegistryError> {
    let Some(head) = head else {
        return Ok(false);
    };
    if head.activation_sha256 != request.activation_sha256 {
        return Ok(false);
    }
    if request.expected_head_revision == head.head_revision
        && request.expected_transition_sha256.as_deref() == Some(head.transition_sha256.as_str())
    {
        return Ok(true);
    }
    let transition: Option<(i64, Option<String>, String)> = transaction
        .query_row(
            r#"SELECT previous_head_revision, previous_transition_sha256,
                      next_activation_sha256
                 FROM jobs_browser_release_channel_transitions
                WHERE transition_sha256 = ?1 AND channel = ?2
                  AND head_revision = ?3"#,
            params![head.transition_sha256, head.channel, head.head_revision],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()
        .map_err(browser_release_registry_storage)?;
    Ok(transition.is_some_and(|(revision, digest, activation)| {
        revision == request.expected_head_revision
            && digest.as_deref() == request.expected_transition_sha256.as_deref()
            && activation == request.activation_sha256
    }))
}

fn browser_release_activation_apply_is_exact_replay_postgres(
    transaction: &mut postgres::Transaction<'_>,
    request: &ApplyBrowserReleaseActivationRequest,
    head: Option<&StoredBrowserReleaseChannelHead>,
) -> Result<bool, BrowserReleaseRegistryError> {
    let Some(head) = head else {
        return Ok(false);
    };
    if head.activation_sha256 != request.activation_sha256 {
        return Ok(false);
    }
    if request.expected_head_revision == head.head_revision
        && request.expected_transition_sha256.as_deref() == Some(head.transition_sha256.as_str())
    {
        return Ok(true);
    }
    let transition = transaction
        .query_opt(
            r#"SELECT previous_head_revision, previous_transition_sha256,
                      next_activation_sha256
                 FROM jobs_browser_release_channel_transitions
                WHERE transition_sha256 = $1 AND channel = $2
                  AND head_revision = $3"#,
            &[&head.transition_sha256, &head.channel, &head.head_revision],
        )
        .map_err(browser_release_registry_storage)?;
    Ok(transition.is_some_and(|row| {
        row.get::<_, i64>(0) == request.expected_head_revision
            && row.get::<_, Option<String>>(1).as_deref()
                == request.expected_transition_sha256.as_deref()
            && row.get::<_, String>(2) == request.activation_sha256
    }))
}

fn require_browser_release_activation_progression_sqlite(
    transaction: &rusqlite::Transaction<'_>,
    activation: &StoredBrowserReleaseActivation,
    manifest: &StoredBrowserReleaseManifest,
    head: Option<&StoredBrowserReleaseChannelHead>,
) -> Result<(), BrowserReleaseRegistryError> {
    let Some(head) = head else {
        return Ok(());
    };
    if activation.trust_generation < head.trust_generation {
        return Err(BrowserReleaseRegistryError::SequenceRegression);
    }
    if activation.trust_generation == head.trust_generation
        && activation.channel_sequence <= head.channel_sequence
    {
        return Err(BrowserReleaseRegistryError::SequenceRegression);
    }
    let previous_manifest =
        sqlite_browser_release_manifest_by_sha256(transaction, &head.manifest_sha256)?.ok_or_else(
            || {
                browser_release_registry_storage(anyhow::anyhow!(
                    "Browser channel head references a missing manifest"
                ))
            },
        )?;
    if manifest.release_sequence < previous_manifest.release_sequence {
        return Err(BrowserReleaseRegistryError::DowngradeRequiresRollback);
    }
    Ok(())
}

fn require_browser_release_activation_progression_postgres(
    transaction: &mut postgres::Transaction<'_>,
    activation: &StoredBrowserReleaseActivation,
    manifest: &StoredBrowserReleaseManifest,
    head: Option<&StoredBrowserReleaseChannelHead>,
) -> Result<(), BrowserReleaseRegistryError> {
    let Some(head) = head else {
        return Ok(());
    };
    if activation.trust_generation < head.trust_generation {
        return Err(BrowserReleaseRegistryError::SequenceRegression);
    }
    if activation.trust_generation == head.trust_generation
        && activation.channel_sequence <= head.channel_sequence
    {
        return Err(BrowserReleaseRegistryError::SequenceRegression);
    }
    let previous_manifest =
        postgres_browser_release_manifest_by_sha256(transaction, &head.manifest_sha256)?
            .ok_or_else(|| {
                browser_release_registry_storage(anyhow::anyhow!(
                    "Browser channel head references a missing manifest"
                ))
            })?;
    if manifest.release_sequence < previous_manifest.release_sequence {
        return Err(BrowserReleaseRegistryError::DowngradeRequiresRollback);
    }
    Ok(())
}

fn insert_sqlite_browser_release_channel_transition(
    transaction: &rusqlite::Transaction<'_>,
    transition: &BrowserReleaseChannelTransition,
) -> Result<(), BrowserReleaseRegistryError> {
    transaction
        .execute(
            r#"INSERT INTO jobs_browser_release_channel_transitions (
                 transition_sha256, channel, head_revision, previous_head_revision,
                 previous_transition_sha256, previous_activation_sha256,
                 previous_manifest_sha256, previous_trust_generation,
                 previous_channel_sequence, next_activation_sha256,
                 next_manifest_sha256, next_trust_generation, next_channel_sequence,
                 transition_kind, authority_sha256, rollback_authority_sha256,
                 recorded_by, recorded_at_ms
               ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11,
                         ?12, ?13, ?14, ?15, ?16, ?17, ?18)"#,
            params![
                transition.transition_sha256,
                transition.channel,
                transition.head_revision,
                transition.previous_head_revision,
                transition.previous_transition_sha256,
                transition.previous_activation_sha256,
                transition.previous_manifest_sha256,
                transition.previous_trust_generation,
                transition.previous_channel_sequence,
                transition.next_activation_sha256,
                transition.next_manifest_sha256,
                transition.next_trust_generation,
                transition.next_channel_sequence,
                transition.transition_kind,
                transition.authority_sha256,
                transition.rollback_authority_sha256,
                transition.recorded_by,
                transition.recorded_at_ms,
            ],
        )
        .map_err(browser_release_registry_storage)?;
    Ok(())
}

fn insert_postgres_browser_release_channel_transition(
    transaction: &mut postgres::Transaction<'_>,
    transition: &BrowserReleaseChannelTransition,
) -> Result<(), BrowserReleaseRegistryError> {
    transaction
        .execute(
            r#"INSERT INTO jobs_browser_release_channel_transitions (
                 transition_sha256, channel, head_revision, previous_head_revision,
                 previous_transition_sha256, previous_activation_sha256,
                 previous_manifest_sha256, previous_trust_generation,
                 previous_channel_sequence, next_activation_sha256,
                 next_manifest_sha256, next_trust_generation, next_channel_sequence,
                 transition_kind, authority_sha256, rollback_authority_sha256,
                 recorded_by, recorded_at_ms
               ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11,
                         $12, $13, $14, $15, $16, $17, $18)"#,
            &[
                &transition.transition_sha256,
                &transition.channel,
                &transition.head_revision,
                &transition.previous_head_revision,
                &transition.previous_transition_sha256,
                &transition.previous_activation_sha256,
                &transition.previous_manifest_sha256,
                &transition.previous_trust_generation,
                &transition.previous_channel_sequence,
                &transition.next_activation_sha256,
                &transition.next_manifest_sha256,
                &transition.next_trust_generation,
                &transition.next_channel_sequence,
                &transition.transition_kind,
                &transition.authority_sha256,
                &transition.rollback_authority_sha256,
                &transition.recorded_by,
                &transition.recorded_at_ms,
            ],
        )
        .map_err(browser_release_registry_storage)?;
    Ok(())
}

fn apply_sqlite_browser_release_channel_head(
    transaction: &rusqlite::Transaction<'_>,
    previous: Option<&StoredBrowserReleaseChannelHead>,
    transition: &BrowserReleaseChannelTransition,
) -> Result<(), BrowserReleaseRegistryError> {
    let affected = match previous {
        Some(previous) => transaction.execute(
            r#"UPDATE jobs_browser_release_channel_heads
                  SET head_revision = ?1, current_transition_sha256 = ?2,
                      current_activation_sha256 = ?3, current_manifest_sha256 = ?4,
                      current_trust_generation = ?5, current_channel_sequence = ?6,
                      updated_at_ms = ?7
                WHERE channel = ?8 AND head_revision = ?9
                  AND current_transition_sha256 = ?10"#,
            params![
                transition.head_revision,
                transition.transition_sha256,
                transition.next_activation_sha256,
                transition.next_manifest_sha256,
                transition.next_trust_generation,
                transition.next_channel_sequence,
                transition.recorded_at_ms,
                transition.channel,
                previous.head_revision,
                previous.transition_sha256,
            ],
        ),
        None => transaction.execute(
            r#"INSERT INTO jobs_browser_release_channel_heads (
                 channel, head_revision, current_transition_sha256,
                 current_activation_sha256, current_manifest_sha256,
                 current_trust_generation, current_channel_sequence, updated_at_ms
               ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)"#,
            params![
                transition.channel,
                transition.head_revision,
                transition.transition_sha256,
                transition.next_activation_sha256,
                transition.next_manifest_sha256,
                transition.next_trust_generation,
                transition.next_channel_sequence,
                transition.recorded_at_ms,
            ],
        ),
    }
    .map_err(browser_release_registry_storage)?;
    if affected != 1 {
        return Err(BrowserReleaseRegistryError::CompareAndSwapConflict);
    }
    Ok(())
}

fn apply_postgres_browser_release_channel_head(
    transaction: &mut postgres::Transaction<'_>,
    previous: Option<&StoredBrowserReleaseChannelHead>,
    transition: &BrowserReleaseChannelTransition,
) -> Result<(), BrowserReleaseRegistryError> {
    let affected = match previous {
        Some(previous) => transaction.execute(
            r#"UPDATE jobs_browser_release_channel_heads
                  SET head_revision = $1, current_transition_sha256 = $2,
                      current_activation_sha256 = $3, current_manifest_sha256 = $4,
                      current_trust_generation = $5, current_channel_sequence = $6,
                      updated_at_ms = $7
                WHERE channel = $8 AND head_revision = $9
                  AND current_transition_sha256 = $10"#,
            &[
                &transition.head_revision,
                &transition.transition_sha256,
                &transition.next_activation_sha256,
                &transition.next_manifest_sha256,
                &transition.next_trust_generation,
                &transition.next_channel_sequence,
                &transition.recorded_at_ms,
                &transition.channel,
                &previous.head_revision,
                &previous.transition_sha256,
            ],
        ),
        None => transaction.execute(
            r#"INSERT INTO jobs_browser_release_channel_heads (
                 channel, head_revision, current_transition_sha256,
                 current_activation_sha256, current_manifest_sha256,
                 current_trust_generation, current_channel_sequence, updated_at_ms
               ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)"#,
            &[
                &transition.channel,
                &transition.head_revision,
                &transition.transition_sha256,
                &transition.next_activation_sha256,
                &transition.next_manifest_sha256,
                &transition.next_trust_generation,
                &transition.next_channel_sequence,
                &transition.recorded_at_ms,
            ],
        ),
    }
    .map_err(browser_release_registry_storage)?;
    if affected != 1 {
        return Err(BrowserReleaseRegistryError::CompareAndSwapConflict);
    }
    Ok(())
}

fn ensure_sqlite_browser_release_target_not_revoked(
    transaction: &rusqlite::Transaction<'_>,
    activation: &StoredBrowserReleaseActivation,
    manifest: &StoredBrowserReleaseManifest,
    policy: &StoredBrowserReleaseTrustPolicy,
) -> Result<(), BrowserReleaseRegistryError> {
    ensure_sqlite_browser_release_manifest_not_revoked(transaction, manifest)?;
    let revoked = transaction
        .query_row(
            r#"SELECT 1
                 FROM jobs_browser_release_revocations r
                WHERE r.subject_kind = 'signing-key'
                  AND r.subject_id IN (
                    SELECT key_id FROM jobs_browser_release_signatures
                     WHERE signature_set_sha256 IN (?1, ?2)
                  )
                LIMIT 1"#,
            params![
                activation.authorization_signature_set_sha256,
                policy.authorization_signature_set_sha256,
            ],
            |_| Ok(()),
        )
        .optional()
        .map_err(browser_release_registry_storage)?
        .is_some();
    if revoked {
        return Err(BrowserReleaseRegistryError::Revoked);
    }
    let policy_revoked = transaction
        .query_row(
            r#"SELECT 1
                 FROM jobs_browser_release_trust_keys k
                WHERE k.policy_sha256 = ?1 AND k.state = 'revoked'
                  AND (
                    k.key_id IN (
                      SELECT key_id FROM jobs_browser_release_signatures
                       WHERE signature_set_sha256 IN (?2, ?3, ?4)
                    )
                    OR k.key_id IN (
                      SELECT build_descriptor_signing_key_id
                        FROM jobs_browser_release_artifacts
                       WHERE manifest_sha256 = ?5
                    )
                  )
                LIMIT 1"#,
            params![
                policy.policy_sha256,
                manifest.authorization_signature_set_sha256,
                activation.authorization_signature_set_sha256,
                policy.authorization_signature_set_sha256,
                manifest.manifest_sha256,
            ],
            |_| Ok(()),
        )
        .optional()
        .map_err(browser_release_registry_storage)?
        .is_some();
    if policy_revoked {
        return Err(BrowserReleaseRegistryError::Revoked);
    }
    Ok(())
}

fn ensure_postgres_browser_release_target_not_revoked(
    transaction: &mut postgres::Transaction<'_>,
    activation: &StoredBrowserReleaseActivation,
    manifest: &StoredBrowserReleaseManifest,
    policy: &StoredBrowserReleaseTrustPolicy,
) -> Result<(), BrowserReleaseRegistryError> {
    ensure_postgres_browser_release_manifest_not_revoked(transaction, manifest)?;
    let revoked = transaction
        .query_opt(
            r#"SELECT 1
                 FROM jobs_browser_release_revocations r
                WHERE r.subject_kind = 'signing-key'
                  AND r.subject_id IN (
                    SELECT key_id FROM jobs_browser_release_signatures
                     WHERE signature_set_sha256 IN ($1, $2)
                  )
                LIMIT 1"#,
            &[
                &activation.authorization_signature_set_sha256,
                &policy.authorization_signature_set_sha256,
            ],
        )
        .map_err(browser_release_registry_storage)?
        .is_some();
    if revoked {
        return Err(BrowserReleaseRegistryError::Revoked);
    }
    let policy_revoked = transaction
        .query_opt(
            r#"SELECT 1
                 FROM jobs_browser_release_trust_keys k
                WHERE k.policy_sha256 = $1 AND k.state = 'revoked'
                  AND (
                    k.key_id IN (
                      SELECT key_id FROM jobs_browser_release_signatures
                       WHERE signature_set_sha256 IN ($2, $3, $4)
                    )
                    OR k.key_id IN (
                      SELECT build_descriptor_signing_key_id
                        FROM jobs_browser_release_artifacts
                       WHERE manifest_sha256 = $5
                    )
                  )
                LIMIT 1"#,
            &[
                &policy.policy_sha256,
                &manifest.authorization_signature_set_sha256,
                &activation.authorization_signature_set_sha256,
                &policy.authorization_signature_set_sha256,
                &manifest.manifest_sha256,
            ],
        )
        .map_err(browser_release_registry_storage)?
        .is_some();
    if policy_revoked {
        return Err(BrowserReleaseRegistryError::Revoked);
    }
    Ok(())
}

fn sqlite_browser_release_policy_by_generation(
    transaction: &rusqlite::Transaction<'_>,
    trust_generation: i64,
) -> Result<Option<StoredBrowserReleaseTrustPolicy>, BrowserReleaseRegistryError> {
    transaction
        .query_row(
            r#"SELECT policy_sha256, policy_id, trust_generation,
                      canonical_policy_base64url,
                      authorization_signature_set_sha256
                 FROM jobs_browser_release_trust_policies
                WHERE trust_generation = ?1"#,
            params![trust_generation],
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
        .map_err(browser_release_registry_storage)?
        .map(|(sha256, id, generation, canonical, signature_set)| {
            stored_browser_release_policy(sha256, id, generation, canonical, signature_set)
        })
        .transpose()
}

fn postgres_browser_release_policy_by_generation(
    transaction: &mut postgres::Transaction<'_>,
    trust_generation: i64,
) -> Result<Option<StoredBrowserReleaseTrustPolicy>, BrowserReleaseRegistryError> {
    transaction
        .query_opt(
            r#"SELECT policy_sha256, policy_id, trust_generation,
                      canonical_policy_base64url,
                      authorization_signature_set_sha256
                 FROM jobs_browser_release_trust_policies
                WHERE trust_generation = $1"#,
            &[&trust_generation],
        )
        .map_err(browser_release_registry_storage)?
        .map(|row| {
            stored_browser_release_policy(
                row.get(0),
                row.get(1),
                row.get(2),
                row.get(3),
                row.get(4),
            )
        })
        .transpose()
}

fn sqlite_browser_release_replay_policy_sha256(
    transaction: &rusqlite::Transaction<'_>,
    trust_generation: i64,
) -> Result<String, BrowserReleaseRegistryError> {
    sqlite_browser_release_policy_by_generation(transaction, trust_generation)?
        .map(|policy| policy.policy_sha256)
        .ok_or_else(|| {
            browser_release_registry_storage(anyhow::anyhow!(
                "stored Browser authority references a missing trust policy"
            ))
        })
}

fn postgres_browser_release_replay_policy_sha256(
    transaction: &mut postgres::Transaction<'_>,
    trust_generation: i64,
) -> Result<String, BrowserReleaseRegistryError> {
    postgres_browser_release_policy_by_generation(transaction, trust_generation)?
        .map(|policy| policy.policy_sha256)
        .ok_or_else(|| {
            browser_release_registry_storage(anyhow::anyhow!(
                "stored Browser authority references a missing trust policy"
            ))
        })
}

fn browser_release_status_without_head(
    channel: &str,
    latest_policy: Option<&StoredBrowserReleaseTrustPolicy>,
) -> BrowserReleaseChannelStatus {
    BrowserReleaseChannelStatus {
        channel: channel.to_string(),
        available: false,
        unavailability_reason: Some(if latest_policy.is_some() {
            "no-active-channel-head".to_string()
        } else {
            "no-trust-policy".to_string()
        }),
        trust_policy_sha256: latest_policy.map(|policy| policy.policy_sha256.clone()),
        trust_generation: latest_policy.map(|policy| policy.trust_generation),
        head_revision: 0,
        transition_sha256: None,
        activation_sha256: None,
        activation_authorization_signature_set_sha256: None,
        manifest_sha256: None,
        manifest_authorization_signature_set_sha256: None,
        channel_sequence: None,
        release_sequence: None,
        release_id: None,
        build_id: None,
        app_version: None,
        protocol_version: None,
        activation_expires_at_ms: None,
    }
}

fn browser_release_status_with_head(
    head: &StoredBrowserReleaseChannelHead,
    activation: &StoredBrowserReleaseActivation,
    manifest: &StoredBrowserReleaseManifest,
    policy: &StoredBrowserReleaseTrustPolicy,
    available: bool,
    unavailability_reason: Option<&str>,
) -> BrowserReleaseChannelStatus {
    BrowserReleaseChannelStatus {
        channel: head.channel.clone(),
        available,
        unavailability_reason: unavailability_reason.map(str::to_string),
        trust_policy_sha256: Some(policy.policy_sha256.clone()),
        trust_generation: Some(head.trust_generation),
        head_revision: head.head_revision,
        transition_sha256: Some(head.transition_sha256.clone()),
        activation_sha256: Some(head.activation_sha256.clone()),
        activation_authorization_signature_set_sha256: Some(
            activation.authorization_signature_set_sha256.clone(),
        ),
        manifest_sha256: Some(head.manifest_sha256.clone()),
        manifest_authorization_signature_set_sha256: Some(
            manifest.authorization_signature_set_sha256.clone(),
        ),
        channel_sequence: Some(head.channel_sequence),
        release_sequence: Some(manifest.release_sequence),
        release_id: Some(manifest.release_id.clone()),
        build_id: Some(manifest.build_id.clone()),
        app_version: Some(manifest.app_version.clone()),
        protocol_version: Some(manifest.protocol_version),
        activation_expires_at_ms: Some(activation.expires_at_ms),
    }
}

fn validate_stored_browser_release_head_binding(
    head: &StoredBrowserReleaseChannelHead,
    activation: &StoredBrowserReleaseActivation,
    manifest: &StoredBrowserReleaseManifest,
) -> Result<(), BrowserReleaseRegistryError> {
    if activation.activation_sha256 != head.activation_sha256
        || activation.channel != head.channel
        || activation.manifest_sha256 != head.manifest_sha256
        || activation.trust_generation != head.trust_generation
        || activation.channel_sequence != head.channel_sequence
        || manifest.manifest_sha256 != head.manifest_sha256
        || activation.manifest_signature_set_sha256 != manifest.authorization_signature_set_sha256
    {
        return Err(browser_release_registry_storage(anyhow::anyhow!(
            "stored Browser channel head has inconsistent authority bindings"
        )));
    }
    Ok(())
}

fn sqlite_browser_release_artifact_set_complete(
    transaction: &rusqlite::Transaction<'_>,
    manifest_sha256: &str,
) -> Result<bool, BrowserReleaseRegistryError> {
    let mut statement = transaction
        .prepare(
            r#"SELECT artifact.platform, artifact.architecture,
                      artifact.package_kind, artifact.build_descriptor_sha256,
                      artifact.app_content_sha256,
                      runtime.automation_bundle_sha256,
                      runtime.chromium_executable_sha256, artifact.artifact_url
                 FROM jobs_browser_release_artifacts artifact
                 JOIN jobs_browser_release_artifact_runtime_components runtime
                   ON runtime.manifest_sha256 = artifact.manifest_sha256
                  AND runtime.artifact_id = artifact.artifact_id
                  AND runtime.build_descriptor_sha256 =
                      artifact.build_descriptor_sha256
                  AND runtime.artifact_sha256 = artifact.artifact_sha256
                  AND runtime.platform = artifact.platform
                  AND runtime.architecture = artifact.architecture
                  AND runtime.package_kind = artifact.package_kind
                WHERE artifact.manifest_sha256 = ?1
                ORDER BY artifact.platform, artifact.architecture,
                         artifact.package_kind"#,
        )
        .map_err(browser_release_registry_storage)?;
    let artifacts = statement
        .query_map(params![manifest_sha256], |row| {
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
        })
        .map_err(browser_release_registry_storage)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(browser_release_registry_storage)?;
    Ok(browser_artifact_contract_complete(&artifacts))
}

fn postgres_browser_release_artifact_set_complete(
    transaction: &mut postgres::Transaction<'_>,
    manifest_sha256: &str,
) -> Result<bool, BrowserReleaseRegistryError> {
    let artifacts = transaction
        .query(
            r#"SELECT artifact.platform, artifact.architecture,
                      artifact.package_kind, artifact.build_descriptor_sha256,
                      artifact.app_content_sha256,
                      runtime.automation_bundle_sha256,
                      runtime.chromium_executable_sha256, artifact.artifact_url
                 FROM jobs_browser_release_artifacts artifact
                 JOIN jobs_browser_release_artifact_runtime_components runtime
                   ON runtime.manifest_sha256 = artifact.manifest_sha256
                  AND runtime.artifact_id = artifact.artifact_id
                  AND runtime.build_descriptor_sha256 =
                      artifact.build_descriptor_sha256
                  AND runtime.artifact_sha256 = artifact.artifact_sha256
                  AND runtime.platform = artifact.platform
                  AND runtime.architecture = artifact.architecture
                  AND runtime.package_kind = artifact.package_kind
                WHERE artifact.manifest_sha256 = $1
                ORDER BY artifact.platform, artifact.architecture,
                         artifact.package_kind"#,
            &[&manifest_sha256],
        )
        .map_err(browser_release_registry_storage)?
        .into_iter()
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
        })
        .collect::<Vec<_>>();
    Ok(browser_artifact_contract_complete(&artifacts))
}

fn sqlite_browser_release_channel_status_tx(
    transaction: &rusqlite::Transaction<'_>,
    channel: &str,
    verification_time_ms: i64,
) -> Result<BrowserReleaseChannelStatus, BrowserReleaseRegistryError> {
    let latest_policy = sqlite_latest_browser_release_policy(transaction)?;
    let Some(head) = sqlite_browser_release_channel_head(transaction, channel)? else {
        return Ok(browser_release_status_without_head(
            channel,
            latest_policy.as_ref(),
        ));
    };
    let policy = sqlite_browser_release_policy_by_generation(transaction, head.trust_generation)?
        .ok_or_else(|| {
        browser_release_registry_storage(anyhow::anyhow!(
            "Browser channel head references a missing trust policy"
        ))
    })?;
    let activation =
        sqlite_browser_release_activation_by_sha256(transaction, &head.activation_sha256)?
            .ok_or_else(|| {
                browser_release_registry_storage(anyhow::anyhow!(
                    "Browser channel head references a missing activation"
                ))
            })?;
    let manifest = sqlite_browser_release_manifest_by_sha256(transaction, &head.manifest_sha256)?
        .ok_or_else(|| {
        browser_release_registry_storage(anyhow::anyhow!(
            "Browser channel head references a missing manifest"
        ))
    })?;
    validate_stored_browser_release_head_binding(&head, &activation, &manifest)?;
    let reason = if latest_policy
        .as_ref()
        .is_none_or(|latest| latest.trust_generation != head.trust_generation)
    {
        Some("stale-trust-generation")
    } else if policy.policy.valid_from_ms > verification_time_ms
        || policy.policy.expires_at_ms <= verification_time_ms
    {
        Some("trust-policy-expired")
    } else if activation.expires_at_ms <= verification_time_ms {
        Some("activation-expired")
    } else if !sqlite_browser_release_artifact_set_complete(transaction, &manifest.manifest_sha256)?
    {
        Some("incomplete-artifact-set")
    } else {
        match ensure_sqlite_browser_release_target_not_revoked(
            transaction,
            &activation,
            &manifest,
            &policy,
        ) {
            Ok(()) => None,
            Err(BrowserReleaseRegistryError::Revoked) => Some("release-revoked"),
            Err(error) => return Err(error),
        }
    };
    Ok(browser_release_status_with_head(
        &head,
        &activation,
        &manifest,
        &policy,
        reason.is_none(),
        reason,
    ))
}

fn postgres_browser_release_channel_status_tx(
    transaction: &mut postgres::Transaction<'_>,
    channel: &str,
    verification_time_ms: i64,
) -> Result<BrowserReleaseChannelStatus, BrowserReleaseRegistryError> {
    let latest_policy = postgres_latest_browser_release_policy(transaction)?;
    let Some(head) = postgres_browser_release_channel_head_for_update(transaction, channel)? else {
        return Ok(browser_release_status_without_head(
            channel,
            latest_policy.as_ref(),
        ));
    };
    let policy = postgres_browser_release_policy_by_generation(transaction, head.trust_generation)?
        .ok_or_else(|| {
            browser_release_registry_storage(anyhow::anyhow!(
                "Browser channel head references a missing trust policy"
            ))
        })?;
    let activation =
        postgres_browser_release_activation_by_sha256(transaction, &head.activation_sha256)?
            .ok_or_else(|| {
                browser_release_registry_storage(anyhow::anyhow!(
                    "Browser channel head references a missing activation"
                ))
            })?;
    let manifest = postgres_browser_release_manifest_by_sha256(transaction, &head.manifest_sha256)?
        .ok_or_else(|| {
            browser_release_registry_storage(anyhow::anyhow!(
                "Browser channel head references a missing manifest"
            ))
        })?;
    validate_stored_browser_release_head_binding(&head, &activation, &manifest)?;
    let reason = if latest_policy
        .as_ref()
        .is_none_or(|latest| latest.trust_generation != head.trust_generation)
    {
        Some("stale-trust-generation")
    } else if policy.policy.valid_from_ms > verification_time_ms
        || policy.policy.expires_at_ms <= verification_time_ms
    {
        Some("trust-policy-expired")
    } else if activation.expires_at_ms <= verification_time_ms {
        Some("activation-expired")
    } else if !postgres_browser_release_artifact_set_complete(
        transaction,
        &manifest.manifest_sha256,
    )? {
        Some("incomplete-artifact-set")
    } else {
        match ensure_postgres_browser_release_target_not_revoked(
            transaction,
            &activation,
            &manifest,
            &policy,
        ) {
            Ok(()) => None,
            Err(BrowserReleaseRegistryError::Revoked) => Some("release-revoked"),
            Err(error) => return Err(error),
        }
    };
    Ok(browser_release_status_with_head(
        &head,
        &activation,
        &manifest,
        &policy,
        reason.is_none(),
        reason,
    ))
}

pub fn apply_browser_release_rollback(
    pool: &DbPool,
    envelope: &BrowserReleaseAuthorityEnvelope,
    recorded_by: &str,
) -> Result<BrowserReleaseChannelStatus, BrowserReleaseRegistryError> {
    validate_browser_release_recorded_by(recorded_by)?;
    let verification_time_ms = now_ms();
    let (rollback_bytes, signature_set_bytes) =
        decode_browser_release_authority_envelope(envelope)?;
    let parsed_rollback = parse_canonical_browser_release_rollback(&rollback_bytes)?;
    let parsed_signature_set = parse_canonical_browser_release_signature_set(&signature_set_bytes)?;
    if !browser_release_registry_channel(&parsed_rollback.channel)
        || parsed_rollback.issued_at_ms > verification_time_ms
        || parsed_signature_set.signed_at_ms > verification_time_ms
    {
        return Err(BrowserReleaseRegistryError::InvalidAuthority);
    }
    let rollback_sha256 = browser_release_authority_sha256(&rollback_bytes);
    let signature_set_sha256 = browser_release_authority_sha256(&signature_set_bytes);
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut connection = pool.get().map_err(browser_release_registry_storage)?;
            let transaction = connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(browser_release_registry_storage)?;
            if let Some(existing) =
                sqlite_browser_release_rollback_identity(&transaction, &parsed_rollback)?
            {
                require_exact_browser_release_rollback_replay(
                    &existing,
                    &parsed_rollback,
                    &rollback_sha256,
                    &envelope.canonical_base64url,
                    &signature_set_sha256,
                )?;
                require_exact_sqlite_browser_release_signature_set(
                    &transaction,
                    &parsed_signature_set,
                    &signature_set_sha256,
                    &envelope.signature_set_base64url,
                )?;
                require_sqlite_browser_release_rollback_transition(&transaction, &rollback_sha256)?;
                let status = sqlite_browser_release_channel_status_tx(
                    &transaction,
                    &parsed_rollback.channel,
                    verification_time_ms,
                )?;
                transaction
                    .commit()
                    .map_err(browser_release_registry_storage)?;
                return Ok(status);
            }
            let policy = sqlite_latest_browser_release_policy(&transaction)?
                .ok_or(BrowserReleaseRegistryError::NotFound)?;
            let rollback = verify_browser_release_rollback_authority(
                &rollback_bytes,
                &signature_set_bytes,
                &policy.policy,
                verification_time_ms,
            )?;
            let head = sqlite_browser_release_channel_head(&transaction, &rollback.channel)?
                .ok_or(BrowserReleaseRegistryError::NotFound)?;
            ensure_sqlite_browser_release_signature_keys_not_revoked(
                &transaction,
                &parsed_signature_set,
            )?;
            require_next_sqlite_browser_release_rollback_generation(&transaction, &rollback)?;
            let (target_activation, target_manifest) =
                validate_sqlite_browser_release_rollback_target(
                    &transaction,
                    &rollback,
                    &head,
                    &policy,
                    verification_time_ms,
                )?;
            insert_sqlite_browser_release_signature_set(
                &transaction,
                &parsed_signature_set,
                &signature_set_sha256,
                &envelope.signature_set_base64url,
                recorded_by,
                verification_time_ms,
            )?;
            insert_sqlite_browser_release_rollback(
                &transaction,
                &rollback,
                &rollback_sha256,
                &signature_set_sha256,
                &envelope.canonical_base64url,
                recorded_by,
                verification_time_ms,
            )?;
            let transition = BrowserReleaseChannelTransition::new(
                Some(&head),
                &target_activation,
                "rollback",
                &rollback_sha256,
                Some(&rollback_sha256),
                recorded_by,
                verification_time_ms,
            )?;
            insert_sqlite_browser_release_channel_transition(&transaction, &transition)?;
            apply_sqlite_browser_release_channel_head(&transaction, Some(&head), &transition)?;
            let status = browser_release_status_with_head(
                &StoredBrowserReleaseChannelHead {
                    channel: transition.channel.clone(),
                    head_revision: transition.head_revision,
                    transition_sha256: transition.transition_sha256.clone(),
                    activation_sha256: transition.next_activation_sha256.clone(),
                    manifest_sha256: transition.next_manifest_sha256.clone(),
                    trust_generation: transition.next_trust_generation,
                    channel_sequence: transition.next_channel_sequence,
                },
                &target_activation,
                &target_manifest,
                &policy,
                true,
                None,
            );
            transaction
                .commit()
                .map_err(browser_release_registry_storage)?;
            Ok(status)
        }
        DbPool::Postgres(_) => {
            let mut connection = pool.get_pg().map_err(browser_release_registry_storage)?;
            let mut transaction = connection
                .transaction()
                .map_err(browser_release_registry_storage)?;
            lock_postgres_browser_release_mutation(&mut transaction, &parsed_rollback.channel)?;
            if let Some(existing) =
                postgres_browser_release_rollback_identity(&mut transaction, &parsed_rollback)?
            {
                require_exact_browser_release_rollback_replay(
                    &existing,
                    &parsed_rollback,
                    &rollback_sha256,
                    &envelope.canonical_base64url,
                    &signature_set_sha256,
                )?;
                require_exact_postgres_browser_release_signature_set(
                    &mut transaction,
                    &parsed_signature_set,
                    &signature_set_sha256,
                    &envelope.signature_set_base64url,
                )?;
                require_postgres_browser_release_rollback_transition(
                    &mut transaction,
                    &rollback_sha256,
                )?;
                let status = postgres_browser_release_channel_status_tx(
                    &mut transaction,
                    &parsed_rollback.channel,
                    verification_time_ms,
                )?;
                transaction
                    .commit()
                    .map_err(browser_release_registry_storage)?;
                return Ok(status);
            }
            let head = postgres_browser_release_channel_head_for_update(
                &mut transaction,
                &parsed_rollback.channel,
            )?
            .ok_or(BrowserReleaseRegistryError::NotFound)?;
            let policy = postgres_latest_browser_release_policy(&mut transaction)?
                .ok_or(BrowserReleaseRegistryError::NotFound)?;
            let rollback = verify_browser_release_rollback_authority(
                &rollback_bytes,
                &signature_set_bytes,
                &policy.policy,
                verification_time_ms,
            )?;
            ensure_postgres_browser_release_signature_keys_not_revoked(
                &mut transaction,
                &parsed_signature_set,
            )?;
            require_next_postgres_browser_release_rollback_generation(&mut transaction, &rollback)?;
            let (target_activation, target_manifest) =
                validate_postgres_browser_release_rollback_target(
                    &mut transaction,
                    &rollback,
                    &head,
                    &policy,
                    verification_time_ms,
                )?;
            insert_postgres_browser_release_signature_set(
                &mut transaction,
                &parsed_signature_set,
                &signature_set_sha256,
                &envelope.signature_set_base64url,
                recorded_by,
                verification_time_ms,
            )?;
            insert_postgres_browser_release_rollback(
                &mut transaction,
                &rollback,
                &rollback_sha256,
                &signature_set_sha256,
                &envelope.canonical_base64url,
                recorded_by,
                verification_time_ms,
            )?;
            let transition = BrowserReleaseChannelTransition::new(
                Some(&head),
                &target_activation,
                "rollback",
                &rollback_sha256,
                Some(&rollback_sha256),
                recorded_by,
                verification_time_ms,
            )?;
            insert_postgres_browser_release_channel_transition(&mut transaction, &transition)?;
            apply_postgres_browser_release_channel_head(
                &mut transaction,
                Some(&head),
                &transition,
            )?;
            let status = browser_release_status_with_head(
                &StoredBrowserReleaseChannelHead {
                    channel: transition.channel.clone(),
                    head_revision: transition.head_revision,
                    transition_sha256: transition.transition_sha256.clone(),
                    activation_sha256: transition.next_activation_sha256.clone(),
                    manifest_sha256: transition.next_manifest_sha256.clone(),
                    trust_generation: transition.next_trust_generation,
                    channel_sequence: transition.next_channel_sequence,
                },
                &target_activation,
                &target_manifest,
                &policy,
                true,
                None,
            );
            transaction
                .commit()
                .map_err(browser_release_registry_storage)?;
            Ok(status)
        }
    })
}

fn sqlite_stored_browser_release_rollback(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<StoredBrowserReleaseRollback> {
    Ok(StoredBrowserReleaseRollback {
        rollback_sha256: row.get(0)?,
        rollback_id: row.get(1)?,
        rollback_generation: row.get(2)?,
        trust_generation: row.get(3)?,
        channel: row.get(4)?,
        from_activation_sha256: row.get(5)?,
        from_manifest_sha256: row.get(6)?,
        to_activation_sha256: row.get(7)?,
        to_manifest_sha256: row.get(8)?,
        canonical_rollback_base64url: row.get(9)?,
        authorization_signature_set_sha256: row.get(10)?,
    })
}

fn postgres_stored_browser_release_rollback(row: &postgres::Row) -> StoredBrowserReleaseRollback {
    StoredBrowserReleaseRollback {
        rollback_sha256: row.get(0),
        rollback_id: row.get(1),
        rollback_generation: row.get(2),
        trust_generation: row.get(3),
        channel: row.get(4),
        from_activation_sha256: row.get(5),
        from_manifest_sha256: row.get(6),
        to_activation_sha256: row.get(7),
        to_manifest_sha256: row.get(8),
        canonical_rollback_base64url: row.get(9),
        authorization_signature_set_sha256: row.get(10),
    }
}

fn sqlite_browser_release_rollback_identity(
    transaction: &rusqlite::Transaction<'_>,
    rollback: &BrowserReleaseRollbackAuthority,
) -> Result<Option<StoredBrowserReleaseRollback>, BrowserReleaseRegistryError> {
    transaction
        .query_row(
            r#"SELECT rollback_sha256, rollback_id, rollback_generation,
                      trust_generation, channel, from_activation_sha256,
                      from_manifest_sha256, to_activation_sha256,
                      to_manifest_sha256, canonical_rollback_base64url,
                      authorization_signature_set_sha256
                 FROM jobs_browser_release_rollbacks
                WHERE rollback_id = ?1
                   OR (trust_generation = ?2 AND rollback_generation = ?3)
                LIMIT 1"#,
            params![
                rollback.rollback_id,
                rollback.trust_generation,
                rollback.rollback_generation,
            ],
            sqlite_stored_browser_release_rollback,
        )
        .optional()
        .map_err(browser_release_registry_storage)
}

fn postgres_browser_release_rollback_identity(
    transaction: &mut postgres::Transaction<'_>,
    rollback: &BrowserReleaseRollbackAuthority,
) -> Result<Option<StoredBrowserReleaseRollback>, BrowserReleaseRegistryError> {
    transaction
        .query_opt(
            r#"SELECT rollback_sha256, rollback_id, rollback_generation,
                      trust_generation, channel, from_activation_sha256,
                      from_manifest_sha256, to_activation_sha256,
                      to_manifest_sha256, canonical_rollback_base64url,
                      authorization_signature_set_sha256
                 FROM jobs_browser_release_rollbacks
                WHERE rollback_id = $1
                   OR (trust_generation = $2 AND rollback_generation = $3)
                LIMIT 1"#,
            &[
                &rollback.rollback_id,
                &rollback.trust_generation,
                &rollback.rollback_generation,
            ],
        )
        .map_err(browser_release_registry_storage)
        .map(|row| row.map(|row| postgres_stored_browser_release_rollback(&row)))
}

fn require_exact_browser_release_rollback_replay(
    existing: &StoredBrowserReleaseRollback,
    rollback: &BrowserReleaseRollbackAuthority,
    rollback_sha256: &str,
    canonical_rollback_base64url: &str,
    signature_set_sha256: &str,
) -> Result<(), BrowserReleaseRegistryError> {
    if existing.rollback_sha256 != rollback_sha256
        || existing.rollback_id != rollback.rollback_id
        || existing.rollback_generation != rollback.rollback_generation
        || existing.trust_generation != rollback.trust_generation
        || existing.channel != rollback.channel
        || existing.from_activation_sha256 != rollback.from_activation_sha256
        || existing.from_manifest_sha256 != rollback.from_manifest_sha256
        || existing.to_activation_sha256 != rollback.to_activation_sha256
        || existing.to_manifest_sha256 != rollback.to_manifest_sha256
        || existing.canonical_rollback_base64url != canonical_rollback_base64url
        || existing.authorization_signature_set_sha256 != signature_set_sha256
    {
        return Err(BrowserReleaseRegistryError::IdentityConflict);
    }
    Ok(())
}

fn require_next_sqlite_browser_release_rollback_generation(
    transaction: &rusqlite::Transaction<'_>,
    rollback: &BrowserReleaseRollbackAuthority,
) -> Result<(), BrowserReleaseRegistryError> {
    let previous: i64 = transaction
        .query_row(
            r#"SELECT COALESCE(MAX(rollback_generation), 0)
                 FROM jobs_browser_release_rollbacks
                WHERE trust_generation = ?1"#,
            params![rollback.trust_generation],
            |row| row.get(0),
        )
        .map_err(browser_release_registry_storage)?;
    if rollback.rollback_generation != previous + 1 {
        return Err(BrowserReleaseRegistryError::SequenceRegression);
    }
    Ok(())
}

fn require_next_postgres_browser_release_rollback_generation(
    transaction: &mut postgres::Transaction<'_>,
    rollback: &BrowserReleaseRollbackAuthority,
) -> Result<(), BrowserReleaseRegistryError> {
    let previous: i64 = transaction
        .query_one(
            r#"SELECT COALESCE(MAX(rollback_generation), 0)
                 FROM jobs_browser_release_rollbacks
                WHERE trust_generation = $1"#,
            &[&rollback.trust_generation],
        )
        .map_err(browser_release_registry_storage)?
        .get(0);
    if rollback.rollback_generation != previous + 1 {
        return Err(BrowserReleaseRegistryError::SequenceRegression);
    }
    Ok(())
}

fn validate_sqlite_browser_release_rollback_target(
    transaction: &rusqlite::Transaction<'_>,
    rollback: &BrowserReleaseRollbackAuthority,
    head: &StoredBrowserReleaseChannelHead,
    policy: &StoredBrowserReleaseTrustPolicy,
    verification_time_ms: i64,
) -> Result<
    (StoredBrowserReleaseActivation, StoredBrowserReleaseManifest),
    BrowserReleaseRegistryError,
> {
    require_browser_release_rollback_current_head(rollback, head, policy)?;
    let target =
        sqlite_browser_release_activation_by_sha256(transaction, &rollback.to_activation_sha256)?
            .ok_or(BrowserReleaseRegistryError::NotFound)?;
    let target_manifest =
        sqlite_browser_release_manifest_by_sha256(transaction, &rollback.to_manifest_sha256)?
            .ok_or(BrowserReleaseRegistryError::NotFound)?;
    validate_browser_release_rollback_target_authority(
        rollback,
        head,
        &target,
        &target_manifest,
        verification_time_ms,
    )?;
    let current_manifest =
        sqlite_browser_release_manifest_by_sha256(transaction, &rollback.from_manifest_sha256)?
            .ok_or(BrowserReleaseRegistryError::NotFound)?;
    if target_manifest.release_sequence >= current_manifest.release_sequence {
        return Err(BrowserReleaseRegistryError::InvalidAuthority);
    }
    ensure_sqlite_browser_release_target_not_revoked(
        transaction,
        &target,
        &target_manifest,
        policy,
    )?;
    Ok((target, target_manifest))
}

fn validate_postgres_browser_release_rollback_target(
    transaction: &mut postgres::Transaction<'_>,
    rollback: &BrowserReleaseRollbackAuthority,
    head: &StoredBrowserReleaseChannelHead,
    policy: &StoredBrowserReleaseTrustPolicy,
    verification_time_ms: i64,
) -> Result<
    (StoredBrowserReleaseActivation, StoredBrowserReleaseManifest),
    BrowserReleaseRegistryError,
> {
    require_browser_release_rollback_current_head(rollback, head, policy)?;
    let target =
        postgres_browser_release_activation_by_sha256(transaction, &rollback.to_activation_sha256)?
            .ok_or(BrowserReleaseRegistryError::NotFound)?;
    let target_manifest =
        postgres_browser_release_manifest_by_sha256(transaction, &rollback.to_manifest_sha256)?
            .ok_or(BrowserReleaseRegistryError::NotFound)?;
    validate_browser_release_rollback_target_authority(
        rollback,
        head,
        &target,
        &target_manifest,
        verification_time_ms,
    )?;
    let current_manifest =
        postgres_browser_release_manifest_by_sha256(transaction, &rollback.from_manifest_sha256)?
            .ok_or(BrowserReleaseRegistryError::NotFound)?;
    if target_manifest.release_sequence >= current_manifest.release_sequence {
        return Err(BrowserReleaseRegistryError::InvalidAuthority);
    }
    ensure_postgres_browser_release_target_not_revoked(
        transaction,
        &target,
        &target_manifest,
        policy,
    )?;
    Ok((target, target_manifest))
}

fn require_browser_release_rollback_current_head(
    rollback: &BrowserReleaseRollbackAuthority,
    head: &StoredBrowserReleaseChannelHead,
    policy: &StoredBrowserReleaseTrustPolicy,
) -> Result<(), BrowserReleaseRegistryError> {
    if rollback.channel != head.channel
        || rollback.from_activation_sha256 != head.activation_sha256
        || rollback.from_manifest_sha256 != head.manifest_sha256
        || rollback.trust_generation != policy.trust_generation
        || rollback.trust_generation < head.trust_generation
    {
        return Err(BrowserReleaseRegistryError::CompareAndSwapConflict);
    }
    Ok(())
}

fn validate_browser_release_rollback_target_authority(
    rollback: &BrowserReleaseRollbackAuthority,
    head: &StoredBrowserReleaseChannelHead,
    target: &StoredBrowserReleaseActivation,
    target_manifest: &StoredBrowserReleaseManifest,
    verification_time_ms: i64,
) -> Result<(), BrowserReleaseRegistryError> {
    if target.activation_sha256 != rollback.to_activation_sha256
        || target.manifest_sha256 != rollback.to_manifest_sha256
        || target.channel != rollback.channel
        || target.trust_generation != rollback.trust_generation
        || (target.trust_generation == head.trust_generation
            && target.channel_sequence <= head.channel_sequence)
        || target.expires_at_ms <= verification_time_ms
        || target.manifest_signature_set_sha256
            != target_manifest.authorization_signature_set_sha256
    {
        return Err(BrowserReleaseRegistryError::InvalidAuthority);
    }
    Ok(())
}

fn insert_sqlite_browser_release_rollback(
    transaction: &rusqlite::Transaction<'_>,
    rollback: &BrowserReleaseRollbackAuthority,
    rollback_sha256: &str,
    signature_set_sha256: &str,
    canonical_rollback_base64url: &str,
    recorded_by: &str,
    recorded_at_ms: i64,
) -> Result<(), BrowserReleaseRegistryError> {
    transaction
        .execute(
            r#"INSERT INTO jobs_browser_release_rollbacks (
                 rollback_sha256, rollback_id, rollback_generation, trust_generation,
                 channel, from_activation_sha256, from_manifest_sha256,
                 to_activation_sha256, to_manifest_sha256, canary_evidence_sha256,
                 reason_ref, canonical_rollback_base64url,
                 authorization_signature_set_sha256, issued_at_ms, recorded_by,
                 recorded_at_ms
               ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11,
                         ?12, ?13, ?14, ?15, ?16)"#,
            params![
                rollback_sha256,
                rollback.rollback_id,
                rollback.rollback_generation,
                rollback.trust_generation,
                rollback.channel,
                rollback.from_activation_sha256,
                rollback.from_manifest_sha256,
                rollback.to_activation_sha256,
                rollback.to_manifest_sha256,
                rollback.canary_evidence_sha256,
                rollback.reason_ref,
                canonical_rollback_base64url,
                signature_set_sha256,
                rollback.issued_at_ms,
                recorded_by,
                recorded_at_ms,
            ],
        )
        .map_err(browser_release_registry_storage)?;
    Ok(())
}

fn insert_postgres_browser_release_rollback(
    transaction: &mut postgres::Transaction<'_>,
    rollback: &BrowserReleaseRollbackAuthority,
    rollback_sha256: &str,
    signature_set_sha256: &str,
    canonical_rollback_base64url: &str,
    recorded_by: &str,
    recorded_at_ms: i64,
) -> Result<(), BrowserReleaseRegistryError> {
    transaction
        .execute(
            r#"INSERT INTO jobs_browser_release_rollbacks (
                 rollback_sha256, rollback_id, rollback_generation, trust_generation,
                 channel, from_activation_sha256, from_manifest_sha256,
                 to_activation_sha256, to_manifest_sha256, canary_evidence_sha256,
                 reason_ref, canonical_rollback_base64url,
                 authorization_signature_set_sha256, issued_at_ms, recorded_by,
                 recorded_at_ms
               ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11,
                         $12, $13, $14, $15, $16)"#,
            &[
                &rollback_sha256,
                &rollback.rollback_id,
                &rollback.rollback_generation,
                &rollback.trust_generation,
                &rollback.channel,
                &rollback.from_activation_sha256,
                &rollback.from_manifest_sha256,
                &rollback.to_activation_sha256,
                &rollback.to_manifest_sha256,
                &rollback.canary_evidence_sha256,
                &rollback.reason_ref,
                &canonical_rollback_base64url,
                &signature_set_sha256,
                &rollback.issued_at_ms,
                &recorded_by,
                &recorded_at_ms,
            ],
        )
        .map_err(browser_release_registry_storage)?;
    Ok(())
}

fn require_sqlite_browser_release_rollback_transition(
    transaction: &rusqlite::Transaction<'_>,
    rollback_sha256: &str,
) -> Result<(), BrowserReleaseRegistryError> {
    let exists = transaction
        .query_row(
            r#"SELECT 1 FROM jobs_browser_release_channel_transitions
                WHERE transition_kind = 'rollback' AND authority_sha256 = ?1
                  AND rollback_authority_sha256 = ?1
                LIMIT 1"#,
            params![rollback_sha256],
            |_| Ok(()),
        )
        .optional()
        .map_err(browser_release_registry_storage)?
        .is_some();
    if !exists {
        return Err(browser_release_registry_storage(anyhow::anyhow!(
            "stored Browser rollback has no channel transition"
        )));
    }
    Ok(())
}

fn require_postgres_browser_release_rollback_transition(
    transaction: &mut postgres::Transaction<'_>,
    rollback_sha256: &str,
) -> Result<(), BrowserReleaseRegistryError> {
    let exists = transaction
        .query_opt(
            r#"SELECT 1 FROM jobs_browser_release_channel_transitions
                WHERE transition_kind = 'rollback' AND authority_sha256 = $1
                  AND rollback_authority_sha256 = $1
                LIMIT 1"#,
            &[&rollback_sha256],
        )
        .map_err(browser_release_registry_storage)?
        .is_some();
    if !exists {
        return Err(browser_release_registry_storage(anyhow::anyhow!(
            "stored Browser rollback has no channel transition"
        )));
    }
    Ok(())
}

pub fn append_browser_release_revocation(
    pool: &DbPool,
    envelope: &BrowserReleaseAuthorityEnvelope,
    recorded_by: &str,
) -> Result<BrowserReleaseImportResult, BrowserReleaseRegistryError> {
    validate_browser_release_recorded_by(recorded_by)?;
    let verification_time_ms = now_ms();
    let (revocation_bytes, signature_set_bytes) =
        decode_browser_release_authority_envelope(envelope)?;
    let parsed_revocation = parse_canonical_browser_release_revocation(&revocation_bytes)?;
    let parsed_signature_set = parse_canonical_browser_release_signature_set(&signature_set_bytes)?;
    if parsed_revocation.issued_at_ms > verification_time_ms
        || parsed_signature_set.signed_at_ms > verification_time_ms
    {
        return Err(BrowserReleaseRegistryError::InvalidAuthority);
    }
    let revocation_sha256 = browser_release_authority_sha256(&revocation_bytes);
    let signature_set_sha256 = browser_release_authority_sha256(&signature_set_bytes);
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut connection = pool.get().map_err(browser_release_registry_storage)?;
            let transaction = connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(browser_release_registry_storage)?;
            if let Some(existing) =
                sqlite_browser_release_revocation_identity(&transaction, &parsed_revocation)?
            {
                require_exact_browser_release_revocation_replay(
                    &existing,
                    &parsed_revocation,
                    &revocation_sha256,
                    &envelope.canonical_base64url,
                    &signature_set_sha256,
                )?;
                require_exact_sqlite_browser_release_signature_set(
                    &transaction,
                    &parsed_signature_set,
                    &signature_set_sha256,
                    &envelope.signature_set_base64url,
                )?;
                let replay_policy_sha256 = sqlite_browser_release_replay_policy_sha256(
                    &transaction,
                    parsed_revocation.trust_generation,
                )?;
                transaction
                    .commit()
                    .map_err(browser_release_registry_storage)?;
                return Ok(browser_release_revocation_import_result(
                    &parsed_revocation,
                    &revocation_sha256,
                    &signature_set_sha256,
                    &replay_policy_sha256,
                    true,
                ));
            }
            let policy = sqlite_latest_browser_release_policy(&transaction)?
                .ok_or(BrowserReleaseRegistryError::NotFound)?;
            let revocation = verify_browser_release_revocation_authority(
                &revocation_bytes,
                &signature_set_bytes,
                &policy.policy,
                verification_time_ms,
            )?;
            ensure_sqlite_browser_release_signature_keys_not_revoked(
                &transaction,
                &parsed_signature_set,
            )?;
            require_next_sqlite_browser_release_revocation_generation(&transaction, &revocation)?;
            validate_sqlite_browser_release_revocation_subject(&transaction, &revocation)?;
            insert_sqlite_browser_release_signature_set(
                &transaction,
                &parsed_signature_set,
                &signature_set_sha256,
                &envelope.signature_set_base64url,
                recorded_by,
                verification_time_ms,
            )?;
            insert_sqlite_browser_release_revocation(
                &transaction,
                &revocation,
                &revocation_sha256,
                &signature_set_sha256,
                &envelope.canonical_base64url,
                recorded_by,
                verification_time_ms,
            )?;
            transaction
                .commit()
                .map_err(browser_release_registry_storage)?;
            Ok(browser_release_revocation_import_result(
                &revocation,
                &revocation_sha256,
                &signature_set_sha256,
                &policy.policy_sha256,
                false,
            ))
        }
        DbPool::Postgres(_) => {
            let mut connection = pool.get_pg().map_err(browser_release_registry_storage)?;
            let mut transaction = connection
                .transaction()
                .map_err(browser_release_registry_storage)?;
            transaction
                .query_one(
                    "SELECT pg_advisory_xact_lock(hashtextextended( \
                     'jobs-browser-release-registry', 0))",
                    &[],
                )
                .map_err(browser_release_registry_storage)?;
            if let Some(existing) =
                postgres_browser_release_revocation_identity(&mut transaction, &parsed_revocation)?
            {
                require_exact_browser_release_revocation_replay(
                    &existing,
                    &parsed_revocation,
                    &revocation_sha256,
                    &envelope.canonical_base64url,
                    &signature_set_sha256,
                )?;
                require_exact_postgres_browser_release_signature_set(
                    &mut transaction,
                    &parsed_signature_set,
                    &signature_set_sha256,
                    &envelope.signature_set_base64url,
                )?;
                let replay_policy_sha256 = postgres_browser_release_replay_policy_sha256(
                    &mut transaction,
                    parsed_revocation.trust_generation,
                )?;
                transaction
                    .commit()
                    .map_err(browser_release_registry_storage)?;
                return Ok(browser_release_revocation_import_result(
                    &parsed_revocation,
                    &revocation_sha256,
                    &signature_set_sha256,
                    &replay_policy_sha256,
                    true,
                ));
            }
            let policy = postgres_latest_browser_release_policy(&mut transaction)?
                .ok_or(BrowserReleaseRegistryError::NotFound)?;
            let revocation = verify_browser_release_revocation_authority(
                &revocation_bytes,
                &signature_set_bytes,
                &policy.policy,
                verification_time_ms,
            )?;
            ensure_postgres_browser_release_signature_keys_not_revoked(
                &mut transaction,
                &parsed_signature_set,
            )?;
            require_next_postgres_browser_release_revocation_generation(
                &mut transaction,
                &revocation,
            )?;
            validate_postgres_browser_release_revocation_subject(&mut transaction, &revocation)?;
            insert_postgres_browser_release_signature_set(
                &mut transaction,
                &parsed_signature_set,
                &signature_set_sha256,
                &envelope.signature_set_base64url,
                recorded_by,
                verification_time_ms,
            )?;
            insert_postgres_browser_release_revocation(
                &mut transaction,
                &revocation,
                &revocation_sha256,
                &signature_set_sha256,
                &envelope.canonical_base64url,
                recorded_by,
                verification_time_ms,
            )?;
            transaction
                .commit()
                .map_err(browser_release_registry_storage)?;
            Ok(browser_release_revocation_import_result(
                &revocation,
                &revocation_sha256,
                &signature_set_sha256,
                &policy.policy_sha256,
                false,
            ))
        }
    })
}

fn browser_release_revocation_import_result(
    revocation: &BrowserReleaseRevocationAuthority,
    revocation_sha256: &str,
    signature_set_sha256: &str,
    policy_sha256: &str,
    replayed: bool,
) -> BrowserReleaseImportResult {
    BrowserReleaseImportResult {
        authority_kind: "revocation".to_string(),
        authority_id: revocation.revocation_id.clone(),
        authority_sha256: revocation_sha256.to_string(),
        signature_set_sha256: signature_set_sha256.to_string(),
        trust_policy_sha256: policy_sha256.to_string(),
        manifest_authorization_signature_set_sha256: None,
        activation_authorization_signature_set_sha256: None,
        replayed,
    }
}

fn sqlite_stored_browser_release_revocation(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<StoredBrowserReleaseRevocation> {
    Ok(StoredBrowserReleaseRevocation {
        revocation_sha256: row.get(0)?,
        revocation_id: row.get(1)?,
        revocation_generation: row.get(2)?,
        trust_generation: row.get(3)?,
        subject_kind: row.get(4)?,
        subject_id: row.get(5)?,
        subject_sha256: row.get(6)?,
        canonical_revocation_base64url: row.get(7)?,
        authorization_signature_set_sha256: row.get(8)?,
    })
}

fn postgres_stored_browser_release_revocation(
    row: &postgres::Row,
) -> StoredBrowserReleaseRevocation {
    StoredBrowserReleaseRevocation {
        revocation_sha256: row.get(0),
        revocation_id: row.get(1),
        revocation_generation: row.get(2),
        trust_generation: row.get(3),
        subject_kind: row.get(4),
        subject_id: row.get(5),
        subject_sha256: row.get(6),
        canonical_revocation_base64url: row.get(7),
        authorization_signature_set_sha256: row.get(8),
    }
}

fn sqlite_browser_release_revocation_identity(
    transaction: &rusqlite::Transaction<'_>,
    revocation: &BrowserReleaseRevocationAuthority,
) -> Result<Option<StoredBrowserReleaseRevocation>, BrowserReleaseRegistryError> {
    transaction
        .query_row(
            r#"SELECT revocation_sha256, revocation_id, revocation_generation,
                      trust_generation, subject_kind, subject_id, subject_sha256,
                      canonical_revocation_base64url,
                      authorization_signature_set_sha256
                 FROM jobs_browser_release_revocations
                WHERE revocation_id = ?1
                   OR (trust_generation = ?2 AND revocation_generation = ?3)
                   OR (subject_kind = ?4 AND subject_id = ?5 AND subject_sha256 = ?6)
                LIMIT 1"#,
            params![
                revocation.revocation_id,
                revocation.trust_generation,
                revocation.revocation_generation,
                revocation.subject_kind,
                revocation.subject_id,
                revocation.subject_sha256,
            ],
            sqlite_stored_browser_release_revocation,
        )
        .optional()
        .map_err(browser_release_registry_storage)
}

fn postgres_browser_release_revocation_identity(
    transaction: &mut postgres::Transaction<'_>,
    revocation: &BrowserReleaseRevocationAuthority,
) -> Result<Option<StoredBrowserReleaseRevocation>, BrowserReleaseRegistryError> {
    transaction
        .query_opt(
            r#"SELECT revocation_sha256, revocation_id, revocation_generation,
                      trust_generation, subject_kind, subject_id, subject_sha256,
                      canonical_revocation_base64url,
                      authorization_signature_set_sha256
                 FROM jobs_browser_release_revocations
                WHERE revocation_id = $1
                   OR (trust_generation = $2 AND revocation_generation = $3)
                   OR (subject_kind = $4 AND subject_id = $5 AND subject_sha256 = $6)
                LIMIT 1"#,
            &[
                &revocation.revocation_id,
                &revocation.trust_generation,
                &revocation.revocation_generation,
                &revocation.subject_kind,
                &revocation.subject_id,
                &revocation.subject_sha256,
            ],
        )
        .map_err(browser_release_registry_storage)
        .map(|row| row.map(|row| postgres_stored_browser_release_revocation(&row)))
}

fn require_exact_browser_release_revocation_replay(
    existing: &StoredBrowserReleaseRevocation,
    revocation: &BrowserReleaseRevocationAuthority,
    revocation_sha256: &str,
    canonical_revocation_base64url: &str,
    signature_set_sha256: &str,
) -> Result<(), BrowserReleaseRegistryError> {
    if existing.revocation_sha256 != revocation_sha256
        || existing.revocation_id != revocation.revocation_id
        || existing.revocation_generation != revocation.revocation_generation
        || existing.trust_generation != revocation.trust_generation
        || existing.subject_kind != revocation.subject_kind
        || existing.subject_id != revocation.subject_id
        || existing.subject_sha256 != revocation.subject_sha256
        || existing.canonical_revocation_base64url != canonical_revocation_base64url
        || existing.authorization_signature_set_sha256 != signature_set_sha256
    {
        return Err(BrowserReleaseRegistryError::IdentityConflict);
    }
    Ok(())
}

fn ensure_sqlite_browser_release_signature_keys_not_revoked(
    transaction: &rusqlite::Transaction<'_>,
    signature_set: &BrowserReleaseSignatureSetAuthority,
) -> Result<(), BrowserReleaseRegistryError> {
    // Incident authority is deliberately unable to change root authority. Root
    // state is advanced only by a predecessor-root-authorized policy rotation.
    if signature_set.role == "root" {
        return Ok(());
    }
    for signature in &signature_set.signatures {
        let revoked = transaction
            .query_row(
                r#"SELECT 1 FROM jobs_browser_release_revocations
                    WHERE subject_kind = 'signing-key' AND subject_id = ?1
                    LIMIT 1"#,
                params![signature.key_id],
                |_| Ok(()),
            )
            .optional()
            .map_err(browser_release_registry_storage)?
            .is_some();
        if revoked {
            return Err(BrowserReleaseRegistryError::Revoked);
        }
    }
    Ok(())
}

fn ensure_postgres_browser_release_signature_keys_not_revoked(
    transaction: &mut postgres::Transaction<'_>,
    signature_set: &BrowserReleaseSignatureSetAuthority,
) -> Result<(), BrowserReleaseRegistryError> {
    if signature_set.role == "root" {
        return Ok(());
    }
    for signature in &signature_set.signatures {
        if transaction
            .query_opt(
                r#"SELECT 1 FROM jobs_browser_release_revocations
                    WHERE subject_kind = 'signing-key' AND subject_id = $1
                    LIMIT 1"#,
                &[&signature.key_id],
            )
            .map_err(browser_release_registry_storage)?
            .is_some()
        {
            return Err(BrowserReleaseRegistryError::Revoked);
        }
    }
    Ok(())
}

fn ensure_sqlite_browser_release_descriptor_keys_not_revoked(
    transaction: &rusqlite::Transaction<'_>,
    verified: &VerifiedBrowserReleaseManifestImport,
) -> Result<(), BrowserReleaseRegistryError> {
    for descriptor in verified.descriptors.values() {
        let revoked = transaction
            .query_row(
                r#"SELECT 1 FROM jobs_browser_release_revocations
                    WHERE subject_kind = 'signing-key' AND subject_id = ?1
                    LIMIT 1"#,
                params![descriptor.signing_key_id],
                |_| Ok(()),
            )
            .optional()
            .map_err(browser_release_registry_storage)?
            .is_some();
        if revoked {
            return Err(BrowserReleaseRegistryError::Revoked);
        }
    }
    Ok(())
}

fn ensure_postgres_browser_release_descriptor_keys_not_revoked(
    transaction: &mut postgres::Transaction<'_>,
    verified: &VerifiedBrowserReleaseManifestImport,
) -> Result<(), BrowserReleaseRegistryError> {
    for descriptor in verified.descriptors.values() {
        if transaction
            .query_opt(
                r#"SELECT 1 FROM jobs_browser_release_revocations
                    WHERE subject_kind = 'signing-key' AND subject_id = $1
                    LIMIT 1"#,
                &[&descriptor.signing_key_id],
            )
            .map_err(browser_release_registry_storage)?
            .is_some()
        {
            return Err(BrowserReleaseRegistryError::Revoked);
        }
    }
    Ok(())
}

fn require_next_sqlite_browser_release_revocation_generation(
    transaction: &rusqlite::Transaction<'_>,
    revocation: &BrowserReleaseRevocationAuthority,
) -> Result<(), BrowserReleaseRegistryError> {
    let previous: i64 = transaction
        .query_row(
            r#"SELECT COALESCE(MAX(revocation_generation), 0)
                 FROM jobs_browser_release_revocations
                WHERE trust_generation = ?1"#,
            params![revocation.trust_generation],
            |row| row.get(0),
        )
        .map_err(browser_release_registry_storage)?;
    if revocation.revocation_generation != previous + 1 {
        return Err(BrowserReleaseRegistryError::SequenceRegression);
    }
    Ok(())
}

fn require_next_postgres_browser_release_revocation_generation(
    transaction: &mut postgres::Transaction<'_>,
    revocation: &BrowserReleaseRevocationAuthority,
) -> Result<(), BrowserReleaseRegistryError> {
    let previous: i64 = transaction
        .query_one(
            r#"SELECT COALESCE(MAX(revocation_generation), 0)
                 FROM jobs_browser_release_revocations
                WHERE trust_generation = $1"#,
            &[&revocation.trust_generation],
        )
        .map_err(browser_release_registry_storage)?
        .get(0);
    if revocation.revocation_generation != previous + 1 {
        return Err(BrowserReleaseRegistryError::SequenceRegression);
    }
    Ok(())
}

fn validate_sqlite_browser_release_revocation_subject(
    transaction: &rusqlite::Transaction<'_>,
    revocation: &BrowserReleaseRevocationAuthority,
) -> Result<(), BrowserReleaseRegistryError> {
    let matches = match revocation.subject_kind.as_str() {
        "manifest" => transaction
            .query_row(
                r#"SELECT 1 FROM jobs_browser_release_manifests
                    WHERE manifest_id = ?1 AND manifest_sha256 = ?2"#,
                params![revocation.subject_id, revocation.subject_sha256],
                |_| Ok(()),
            )
            .optional()
            .map_err(browser_release_registry_storage)?
            .is_some(),
        "artifact" => transaction
            .query_row(
                r#"SELECT 1 FROM jobs_browser_release_artifacts
                    WHERE artifact_id = ?1 AND artifact_sha256 = ?2"#,
                params![revocation.subject_id, revocation.subject_sha256],
                |_| Ok(()),
            )
            .optional()
            .map_err(browser_release_registry_storage)?
            .is_some(),
        "build-descriptor" => {
            revocation.subject_id == revocation.subject_sha256
                && transaction
                    .query_row(
                        r#"SELECT 1 FROM jobs_browser_release_artifacts
                            WHERE build_descriptor_sha256 = ?1 LIMIT 1"#,
                        params![revocation.subject_sha256],
                        |_| Ok(()),
                    )
                    .optional()
                    .map_err(browser_release_registry_storage)?
                    .is_some()
        }
        "release" => {
            browser_release_authority_sha256(revocation.subject_id.as_bytes())
                == revocation.subject_sha256
                && transaction
                    .query_row(
                        "SELECT 1 FROM jobs_browser_release_manifests WHERE release_id = ?1",
                        params![revocation.subject_id],
                        |_| Ok(()),
                    )
                    .optional()
                    .map_err(browser_release_registry_storage)?
                    .is_some()
        }
        "signing-key" => sqlite_browser_release_signing_key_subject_matches(
            transaction,
            &revocation.subject_id,
            &revocation.subject_sha256,
        )?,
        _ => false,
    };
    if !matches {
        return Err(BrowserReleaseRegistryError::InvalidAuthority);
    }
    Ok(())
}

fn validate_postgres_browser_release_revocation_subject(
    transaction: &mut postgres::Transaction<'_>,
    revocation: &BrowserReleaseRevocationAuthority,
) -> Result<(), BrowserReleaseRegistryError> {
    let matches = match revocation.subject_kind.as_str() {
        "manifest" => transaction
            .query_opt(
                r#"SELECT 1 FROM jobs_browser_release_manifests
                    WHERE manifest_id = $1 AND manifest_sha256 = $2"#,
                &[&revocation.subject_id, &revocation.subject_sha256],
            )
            .map_err(browser_release_registry_storage)?
            .is_some(),
        "artifact" => transaction
            .query_opt(
                r#"SELECT 1 FROM jobs_browser_release_artifacts
                    WHERE artifact_id = $1 AND artifact_sha256 = $2"#,
                &[&revocation.subject_id, &revocation.subject_sha256],
            )
            .map_err(browser_release_registry_storage)?
            .is_some(),
        "build-descriptor" => {
            revocation.subject_id == revocation.subject_sha256
                && transaction
                    .query_opt(
                        r#"SELECT 1 FROM jobs_browser_release_artifacts
                            WHERE build_descriptor_sha256 = $1 LIMIT 1"#,
                        &[&revocation.subject_sha256],
                    )
                    .map_err(browser_release_registry_storage)?
                    .is_some()
        }
        "release" => {
            browser_release_authority_sha256(revocation.subject_id.as_bytes())
                == revocation.subject_sha256
                && transaction
                    .query_opt(
                        "SELECT 1 FROM jobs_browser_release_manifests WHERE release_id = $1",
                        &[&revocation.subject_id],
                    )
                    .map_err(browser_release_registry_storage)?
                    .is_some()
        }
        "signing-key" => postgres_browser_release_signing_key_subject_matches(
            transaction,
            &revocation.subject_id,
            &revocation.subject_sha256,
        )?,
        _ => false,
    };
    if !matches {
        return Err(BrowserReleaseRegistryError::InvalidAuthority);
    }
    Ok(())
}

fn sqlite_browser_release_signing_key_subject_matches(
    transaction: &rusqlite::Transaction<'_>,
    key_id: &str,
    subject_sha256: &str,
) -> Result<bool, BrowserReleaseRegistryError> {
    let mut statement = transaction
        .prepare(
            r#"SELECT DISTINCT key_id, role, public_key_base64url
                 FROM jobs_browser_release_trust_keys
                WHERE key_id = ?1 OR role = 'root'"#,
        )
        .map_err(browser_release_registry_storage)?;
    let keys = statement
        .query_map(params![key_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map_err(browser_release_registry_storage)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(browser_release_registry_storage)?;
    browser_release_delegated_signing_key_subject_matches(&keys, key_id, subject_sha256)
}

fn postgres_browser_release_signing_key_subject_matches(
    transaction: &mut postgres::Transaction<'_>,
    key_id: &str,
    subject_sha256: &str,
) -> Result<bool, BrowserReleaseRegistryError> {
    let keys = transaction
        .query(
            r#"SELECT DISTINCT key_id, role, public_key_base64url
                 FROM jobs_browser_release_trust_keys
                WHERE key_id = $1 OR role = 'root'"#,
            &[&key_id],
        )
        .map_err(browser_release_registry_storage)?
        .into_iter()
        .map(|row| {
            (
                row.get::<_, String>(0),
                row.get::<_, String>(1),
                row.get::<_, String>(2),
            )
        })
        .collect::<Vec<_>>();
    browser_release_delegated_signing_key_subject_matches(&keys, key_id, subject_sha256)
}

fn browser_release_delegated_signing_key_subject_matches(
    keys: &[(String, String, String)],
    key_id: &str,
    subject_sha256: &str,
) -> Result<bool, BrowserReleaseRegistryError> {
    for (stored_key_id, role, public_key) in keys {
        if role == "root"
            && (stored_key_id == key_id
                || browser_release_signing_key_digest(public_key)? == subject_sha256)
        {
            return Ok(false);
        }
    }
    let subject_keys = keys
        .iter()
        .filter(|(stored_key_id, _, _)| stored_key_id == key_id)
        .map(|(_, _, public_key)| public_key.clone())
        .collect::<Vec<_>>();
    browser_release_signing_key_digest_matches(&subject_keys, subject_sha256)
}

fn browser_release_signing_key_digest_matches(
    public_keys: &[String],
    subject_sha256: &str,
) -> Result<bool, BrowserReleaseRegistryError> {
    if public_keys.len() != 1 {
        return Ok(false);
    }
    Ok(browser_release_signing_key_digest(&public_keys[0])? == subject_sha256)
}

fn browser_release_signing_key_digest(
    public_key: &str,
) -> Result<String, BrowserReleaseRegistryError> {
    let raw_key = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(public_key)
        .map_err(|_| BrowserReleaseRegistryError::InvalidAuthority)?;
    if raw_key.len() != 32
        || base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&raw_key) != public_key
    {
        return Err(browser_release_registry_storage(anyhow::anyhow!(
            "stored Browser signing key is not canonical Ed25519 material"
        )));
    }
    Ok(browser_release_authority_sha256(&raw_key))
}

fn insert_sqlite_browser_release_revocation(
    transaction: &rusqlite::Transaction<'_>,
    revocation: &BrowserReleaseRevocationAuthority,
    revocation_sha256: &str,
    signature_set_sha256: &str,
    canonical_revocation_base64url: &str,
    recorded_by: &str,
    recorded_at_ms: i64,
) -> Result<(), BrowserReleaseRegistryError> {
    transaction
        .execute(
            r#"INSERT INTO jobs_browser_release_revocations (
                 revocation_sha256, revocation_id, revocation_generation,
                 trust_generation, subject_kind, subject_id, subject_sha256,
                 reason_ref, canonical_revocation_base64url,
                 authorization_signature_set_sha256, issued_at_ms, recorded_by,
                 recorded_at_ms
               ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11,
                         ?12, ?13)"#,
            params![
                revocation_sha256,
                revocation.revocation_id,
                revocation.revocation_generation,
                revocation.trust_generation,
                revocation.subject_kind,
                revocation.subject_id,
                revocation.subject_sha256,
                revocation.reason_ref,
                canonical_revocation_base64url,
                signature_set_sha256,
                revocation.issued_at_ms,
                recorded_by,
                recorded_at_ms,
            ],
        )
        .map_err(browser_release_registry_storage)?;
    Ok(())
}

fn insert_postgres_browser_release_revocation(
    transaction: &mut postgres::Transaction<'_>,
    revocation: &BrowserReleaseRevocationAuthority,
    revocation_sha256: &str,
    signature_set_sha256: &str,
    canonical_revocation_base64url: &str,
    recorded_by: &str,
    recorded_at_ms: i64,
) -> Result<(), BrowserReleaseRegistryError> {
    transaction
        .execute(
            r#"INSERT INTO jobs_browser_release_revocations (
                 revocation_sha256, revocation_id, revocation_generation,
                 trust_generation, subject_kind, subject_id, subject_sha256,
                 reason_ref, canonical_revocation_base64url,
                 authorization_signature_set_sha256, issued_at_ms, recorded_by,
                 recorded_at_ms
               ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11,
                         $12, $13)"#,
            &[
                &revocation_sha256,
                &revocation.revocation_id,
                &revocation.revocation_generation,
                &revocation.trust_generation,
                &revocation.subject_kind,
                &revocation.subject_id,
                &revocation.subject_sha256,
                &revocation.reason_ref,
                &canonical_revocation_base64url,
                &signature_set_sha256,
                &revocation.issued_at_ms,
                &recorded_by,
                &recorded_at_ms,
            ],
        )
        .map_err(browser_release_registry_storage)?;
    Ok(())
}

pub fn assign_browser_release_account_channel(
    pool: &DbPool,
    account_id: &str,
    request: &AssignBrowserReleaseChannelRequest,
    assigned_by: &str,
) -> Result<BrowserReleaseAccountChannelAssignment, BrowserReleaseRegistryError> {
    validate_browser_release_assignment_request(account_id, request, assigned_by)?;
    let assignment_sha256 = browser_release_assignment_sha256(account_id, request, assigned_by)?;
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut connection = pool.get().map_err(browser_release_registry_storage)?;
            let transaction = connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(browser_release_registry_storage)?;
            require_sqlite_browser_release_account(&transaction, account_id)?;
            if let Some(existing) = sqlite_browser_release_assignment_identity(
                &transaction,
                account_id,
                request.assignment_generation,
                &assignment_sha256,
            )? {
                require_exact_browser_release_assignment_replay(
                    &existing,
                    account_id,
                    request,
                    assigned_by,
                    &assignment_sha256,
                )?;
                transaction
                    .commit()
                    .map_err(browser_release_registry_storage)?;
                return Ok(browser_release_assignment_result(&existing, true));
            }
            let predecessor = sqlite_latest_browser_release_assignment(&transaction, account_id)?;
            require_browser_release_assignment_progression(request, predecessor.as_ref())?;
            insert_sqlite_browser_release_assignment(
                &transaction,
                account_id,
                request,
                assigned_by,
                &assignment_sha256,
            )?;
            transaction
                .commit()
                .map_err(browser_release_registry_storage)?;
            Ok(BrowserReleaseAccountChannelAssignment {
                assignment_sha256,
                account_id: account_id.to_string(),
                assignment_generation: request.assignment_generation,
                predecessor_assignment_sha256: request.predecessor_assignment_sha256.clone(),
                channel: request.channel.clone(),
                reason_ref: request.reason_ref.clone(),
                assigned_by: assigned_by.to_string(),
                assigned_at_ms: request.assigned_at_ms,
                replayed: false,
            })
        }
        DbPool::Postgres(_) => {
            let mut connection = pool.get_pg().map_err(browser_release_registry_storage)?;
            let mut transaction = connection
                .transaction()
                .map_err(browser_release_registry_storage)?;
            let lock_key = format!("jobs-browser-release-account:{account_id}");
            transaction
                .query_one(
                    "SELECT pg_advisory_xact_lock(hashtextextended($1, 0))",
                    &[&lock_key],
                )
                .map_err(browser_release_registry_storage)?;
            require_postgres_browser_release_account_for_update(&mut transaction, account_id)?;
            if let Some(existing) = postgres_browser_release_assignment_identity(
                &mut transaction,
                account_id,
                request.assignment_generation,
                &assignment_sha256,
            )? {
                require_exact_browser_release_assignment_replay(
                    &existing,
                    account_id,
                    request,
                    assigned_by,
                    &assignment_sha256,
                )?;
                transaction
                    .commit()
                    .map_err(browser_release_registry_storage)?;
                return Ok(browser_release_assignment_result(&existing, true));
            }
            let predecessor =
                postgres_latest_browser_release_assignment(&mut transaction, account_id)?;
            require_browser_release_assignment_progression(request, predecessor.as_ref())?;
            insert_postgres_browser_release_assignment(
                &mut transaction,
                account_id,
                request,
                assigned_by,
                &assignment_sha256,
            )?;
            transaction
                .commit()
                .map_err(browser_release_registry_storage)?;
            Ok(BrowserReleaseAccountChannelAssignment {
                assignment_sha256,
                account_id: account_id.to_string(),
                assignment_generation: request.assignment_generation,
                predecessor_assignment_sha256: request.predecessor_assignment_sha256.clone(),
                channel: request.channel.clone(),
                reason_ref: request.reason_ref.clone(),
                assigned_by: assigned_by.to_string(),
                assigned_at_ms: request.assigned_at_ms,
                replayed: false,
            })
        }
    })
}

fn validate_browser_release_assignment_request(
    account_id: &str,
    request: &AssignBrowserReleaseChannelRequest,
    assigned_by: &str,
) -> Result<(), BrowserReleaseRegistryError> {
    if !browser_release_registry_text(account_id, 128)
        || !browser_release_registry_text(assigned_by, 128)
        || !browser_release_registry_text(&request.reason_ref, 512)
        || !browser_release_registry_channel(&request.channel)
        || !browser_release_registry_positive_integer(request.assignment_generation)
        || !browser_release_registry_non_negative_integer(request.assigned_at_ms)
        || request.assigned_at_ms > now_ms()
        || request
            .predecessor_assignment_sha256
            .as_deref()
            .is_some_and(|digest| !browser_release_registry_sha256(digest))
        || (request.assignment_generation == 1) != request.predecessor_assignment_sha256.is_none()
    {
        return Err(BrowserReleaseRegistryError::InvalidRequest);
    }
    Ok(())
}

fn browser_release_assignment_sha256(
    account_id: &str,
    request: &AssignBrowserReleaseChannelRequest,
    assigned_by: &str,
) -> Result<String, BrowserReleaseRegistryError> {
    browser_release_registry_digest(&BrowserReleaseAssignmentDigest {
        version: 1,
        audience: BROWSER_RELEASE_ASSIGNMENT_AUDIENCE,
        account_id,
        assignment_generation: request.assignment_generation,
        predecessor_assignment_sha256: request.predecessor_assignment_sha256.as_deref(),
        channel: &request.channel,
        reason_ref: &request.reason_ref,
        assigned_by,
        assigned_at_ms: request.assigned_at_ms,
    })
}

fn sqlite_stored_browser_release_assignment(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<StoredBrowserReleaseAssignment> {
    Ok(StoredBrowserReleaseAssignment {
        assignment_sha256: row.get(0)?,
        account_id: row.get(1)?,
        assignment_generation: row.get(2)?,
        predecessor_assignment_sha256: row.get(3)?,
        channel: row.get(4)?,
        reason_ref: row.get(5)?,
        assigned_by: row.get(6)?,
        assigned_at_ms: row.get(7)?,
    })
}

fn postgres_stored_browser_release_assignment(
    row: &postgres::Row,
) -> StoredBrowserReleaseAssignment {
    StoredBrowserReleaseAssignment {
        assignment_sha256: row.get(0),
        account_id: row.get(1),
        assignment_generation: row.get(2),
        predecessor_assignment_sha256: row.get(3),
        channel: row.get(4),
        reason_ref: row.get(5),
        assigned_by: row.get(6),
        assigned_at_ms: row.get(7),
    }
}

fn sqlite_browser_release_assignment_identity(
    transaction: &rusqlite::Transaction<'_>,
    account_id: &str,
    assignment_generation: i64,
    assignment_sha256: &str,
) -> Result<Option<StoredBrowserReleaseAssignment>, BrowserReleaseRegistryError> {
    transaction
        .query_row(
            r#"SELECT assignment_sha256, account_id, assignment_generation,
                      predecessor_assignment_sha256, channel, reason_ref,
                      assigned_by, assigned_at_ms
                 FROM jobs_browser_account_channel_assignments
                WHERE assignment_sha256 = ?1
                   OR (account_id = ?2 AND assignment_generation = ?3)
                LIMIT 1"#,
            params![assignment_sha256, account_id, assignment_generation],
            sqlite_stored_browser_release_assignment,
        )
        .optional()
        .map_err(browser_release_registry_storage)
}

fn postgres_browser_release_assignment_identity(
    transaction: &mut postgres::Transaction<'_>,
    account_id: &str,
    assignment_generation: i64,
    assignment_sha256: &str,
) -> Result<Option<StoredBrowserReleaseAssignment>, BrowserReleaseRegistryError> {
    transaction
        .query_opt(
            r#"SELECT assignment_sha256, account_id, assignment_generation,
                      predecessor_assignment_sha256, channel, reason_ref,
                      assigned_by, assigned_at_ms
                 FROM jobs_browser_account_channel_assignments
                WHERE assignment_sha256 = $1
                   OR (account_id = $2 AND assignment_generation = $3)
                LIMIT 1"#,
            &[&assignment_sha256, &account_id, &assignment_generation],
        )
        .map_err(browser_release_registry_storage)
        .map(|row| row.map(|row| postgres_stored_browser_release_assignment(&row)))
}

fn sqlite_latest_browser_release_assignment(
    transaction: &rusqlite::Transaction<'_>,
    account_id: &str,
) -> Result<Option<StoredBrowserReleaseAssignment>, BrowserReleaseRegistryError> {
    transaction
        .query_row(
            r#"SELECT assignment_sha256, account_id, assignment_generation,
                      predecessor_assignment_sha256, channel, reason_ref,
                      assigned_by, assigned_at_ms
                 FROM jobs_browser_account_channel_assignments
                WHERE account_id = ?1
                ORDER BY assignment_generation DESC
                LIMIT 1"#,
            params![account_id],
            sqlite_stored_browser_release_assignment,
        )
        .optional()
        .map_err(browser_release_registry_storage)
}

fn postgres_latest_browser_release_assignment(
    transaction: &mut postgres::Transaction<'_>,
    account_id: &str,
) -> Result<Option<StoredBrowserReleaseAssignment>, BrowserReleaseRegistryError> {
    transaction
        .query_opt(
            r#"SELECT assignment_sha256, account_id, assignment_generation,
                      predecessor_assignment_sha256, channel, reason_ref,
                      assigned_by, assigned_at_ms
                 FROM jobs_browser_account_channel_assignments
                WHERE account_id = $1
                ORDER BY assignment_generation DESC
                LIMIT 1"#,
            &[&account_id],
        )
        .map_err(browser_release_registry_storage)
        .map(|row| row.map(|row| postgres_stored_browser_release_assignment(&row)))
}

fn require_exact_browser_release_assignment_replay(
    existing: &StoredBrowserReleaseAssignment,
    account_id: &str,
    request: &AssignBrowserReleaseChannelRequest,
    assigned_by: &str,
    assignment_sha256: &str,
) -> Result<(), BrowserReleaseRegistryError> {
    if existing.assignment_sha256 != assignment_sha256
        || existing.account_id != account_id
        || existing.assignment_generation != request.assignment_generation
        || existing.predecessor_assignment_sha256 != request.predecessor_assignment_sha256
        || existing.channel != request.channel
        || existing.reason_ref != request.reason_ref
        || existing.assigned_by != assigned_by
        || existing.assigned_at_ms != request.assigned_at_ms
    {
        return Err(BrowserReleaseRegistryError::IdentityConflict);
    }
    Ok(())
}

fn require_browser_release_assignment_progression(
    request: &AssignBrowserReleaseChannelRequest,
    predecessor: Option<&StoredBrowserReleaseAssignment>,
) -> Result<(), BrowserReleaseRegistryError> {
    let matches = match predecessor {
        Some(predecessor) => {
            request.assignment_generation == predecessor.assignment_generation + 1
                && request.predecessor_assignment_sha256.as_deref()
                    == Some(predecessor.assignment_sha256.as_str())
        }
        None => {
            request.assignment_generation == 1 && request.predecessor_assignment_sha256.is_none()
        }
    };
    if !matches {
        return Err(BrowserReleaseRegistryError::CompareAndSwapConflict);
    }
    Ok(())
}

fn browser_release_assignment_result(
    assignment: &StoredBrowserReleaseAssignment,
    replayed: bool,
) -> BrowserReleaseAccountChannelAssignment {
    BrowserReleaseAccountChannelAssignment {
        assignment_sha256: assignment.assignment_sha256.clone(),
        account_id: assignment.account_id.clone(),
        assignment_generation: assignment.assignment_generation,
        predecessor_assignment_sha256: assignment.predecessor_assignment_sha256.clone(),
        channel: assignment.channel.clone(),
        reason_ref: assignment.reason_ref.clone(),
        assigned_by: assignment.assigned_by.clone(),
        assigned_at_ms: assignment.assigned_at_ms,
        replayed,
    }
}

fn require_sqlite_browser_release_account(
    transaction: &rusqlite::Transaction<'_>,
    account_id: &str,
) -> Result<(), BrowserReleaseRegistryError> {
    let exists = transaction
        .query_row(
            "SELECT 1 FROM accounts WHERE id = ?1",
            params![account_id],
            |_| Ok(()),
        )
        .optional()
        .map_err(browser_release_registry_storage)?
        .is_some();
    if !exists {
        return Err(BrowserReleaseRegistryError::NotFound);
    }
    Ok(())
}

fn require_postgres_browser_release_account_for_update(
    transaction: &mut postgres::Transaction<'_>,
    account_id: &str,
) -> Result<(), BrowserReleaseRegistryError> {
    let exists = transaction
        .query_opt(
            "SELECT 1 FROM accounts WHERE id = $1 FOR UPDATE",
            &[&account_id],
        )
        .map_err(browser_release_registry_storage)?
        .is_some();
    if !exists {
        return Err(BrowserReleaseRegistryError::NotFound);
    }
    Ok(())
}

fn insert_sqlite_browser_release_assignment(
    transaction: &rusqlite::Transaction<'_>,
    account_id: &str,
    request: &AssignBrowserReleaseChannelRequest,
    assigned_by: &str,
    assignment_sha256: &str,
) -> Result<(), BrowserReleaseRegistryError> {
    transaction
        .execute(
            r#"INSERT INTO jobs_browser_account_channel_assignments (
                 assignment_sha256, account_id, assignment_generation,
                 predecessor_assignment_sha256, predecessor_generation,
                 channel, reason_ref, assigned_by, assigned_at_ms
               ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)"#,
            params![
                assignment_sha256,
                account_id,
                request.assignment_generation,
                request.predecessor_assignment_sha256,
                request.assignment_generation - 1,
                request.channel,
                request.reason_ref,
                assigned_by,
                request.assigned_at_ms,
            ],
        )
        .map_err(browser_release_registry_storage)?;
    Ok(())
}

fn insert_postgres_browser_release_assignment(
    transaction: &mut postgres::Transaction<'_>,
    account_id: &str,
    request: &AssignBrowserReleaseChannelRequest,
    assigned_by: &str,
    assignment_sha256: &str,
) -> Result<(), BrowserReleaseRegistryError> {
    let predecessor_generation = request.assignment_generation - 1;
    transaction
        .execute(
            r#"INSERT INTO jobs_browser_account_channel_assignments (
                 assignment_sha256, account_id, assignment_generation,
                 predecessor_assignment_sha256, predecessor_generation,
                 channel, reason_ref, assigned_by, assigned_at_ms
               ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)"#,
            &[
                &assignment_sha256,
                &account_id,
                &request.assignment_generation,
                &request.predecessor_assignment_sha256,
                &predecessor_generation,
                &request.channel,
                &request.reason_ref,
                &assigned_by,
                &request.assigned_at_ms,
            ],
        )
        .map_err(browser_release_registry_storage)?;
    Ok(())
}

pub fn browser_release_channel_status(
    pool: &DbPool,
    channel: &str,
) -> Result<BrowserReleaseChannelStatus, BrowserReleaseRegistryError> {
    if !browser_release_registry_channel(channel) {
        return Err(BrowserReleaseRegistryError::InvalidRequest);
    }
    let verification_time_ms = now_ms();
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut connection = pool.get().map_err(browser_release_registry_storage)?;
            let transaction = connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(browser_release_registry_storage)?;
            let status = sqlite_browser_release_channel_status_tx(
                &transaction,
                channel,
                verification_time_ms,
            )?;
            transaction
                .commit()
                .map_err(browser_release_registry_storage)?;
            Ok(status)
        }
        DbPool::Postgres(_) => {
            let mut connection = pool.get_pg().map_err(browser_release_registry_storage)?;
            let mut transaction = connection
                .transaction()
                .map_err(browser_release_registry_storage)?;
            lock_postgres_browser_release_mutation(&mut transaction, channel)?;
            let status = postgres_browser_release_channel_status_tx(
                &mut transaction,
                channel,
                verification_time_ms,
            )?;
            transaction
                .commit()
                .map_err(browser_release_registry_storage)?;
            Ok(status)
        }
    })
}

#[cfg(test)]
pub(super) mod browser_release_registry_tests {
    use super::*;
    use crate::db;
    use ed25519_dalek::{Signer, SigningKey};
    use std::{ffi::OsString, sync::Mutex};

    static ROOT_ANCHOR_ENV_LOCK: Mutex<()> = Mutex::new(());

    #[derive(Debug, Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct SignedAuthorityFixture {
        canonical: String,
        signature_set: String,
    }

    #[derive(Debug, Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct RegistryFixture {
        trust_policy: SignedAuthorityFixture,
        manifest: SignedAuthorityFixture,
        activation: SignedAuthorityFixture,
        revocation: SignedAuthorityFixture,
    }

    struct RootAnchorEnvironmentGuard(Option<OsString>);

    impl Drop for RootAnchorEnvironmentGuard {
        fn drop(&mut self) {
            match self.0.take() {
                Some(value) => std::env::set_var(BROWSER_RELEASE_ROOT_TRUST_ANCHOR_ENV, value),
                None => std::env::remove_var(BROWSER_RELEASE_ROOT_TRUST_ANCHOR_ENV),
            }
        }
    }

    fn registry_fixture() -> RegistryFixture {
        serde_json::from_str(include_str!(
            "../../../../jobs/browser/fixtures/release-authority-v1.json"
        ))
        .expect("parse shared Browser registry fixture")
    }

    fn decode_fixture(value: &str) -> Vec<u8> {
        base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(value)
            .expect("decode Browser registry fixture")
    }

    fn sqlite_registry_pool() -> DbPool {
        let path = std::env::temp_dir().join(format!(
            "bluey-browser-registry-test-{}-{}.sqlite3",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let pool = db::open_pool(&path).expect("open Browser registry test pool");
        db::run_migrations(&pool).expect("migrate Browser registry test pool");
        let connection = pool.get().expect("get Browser registry test connection");
        connection
            .execute(
                "INSERT INTO accounts (id, email, password_hash, trial_seconds_remaining) \
                 VALUES ('acct-browser-registry', 'registry@example.com', 'hash', 0)",
                [],
            )
            .expect("seed Browser registry account");
        drop(connection);
        pool
    }

    fn install_fixture_root_anchor(policy: &BrowserReleaseTrustPolicyAuthority) {
        let threshold = policy
            .roles
            .iter()
            .find(|role| role.role == "root")
            .expect("fixture root role")
            .threshold;
        let keys = policy
            .keys
            .iter()
            .filter(|key| key.role == "root")
            .map(|key| (key.key_id.clone(), key.public_key.clone()))
            .collect::<BTreeMap<_, _>>();
        std::env::set_var(
            BROWSER_RELEASE_ROOT_TRUST_ANCHOR_ENV,
            serde_json::json!({ "threshold": threshold, "keys": keys }).to_string(),
        );
    }

    fn deterministic_build_proof(platform: &str, architecture: &str) -> BrowserBuildProof {
        let descriptor = format!(
            "version=1\n\
             audience=bluey-jobs-browser-build-v1\n\
             release_id=browser-release-603-1\n\
             build_id=browser-603.1\n\
             app_version=0.1.0\n\
             app_id=sh.bluey.jobs.browser\n\
             protocol_version=1\n\
             source_commit=1111111111111111111111111111111111111111\n\
             platform={platform}\n\
             architecture={architecture}\n\
             electron_version=43.1.0\n\
             playwright_version=1.61.1\n\
             chromium_revision=1228\n\
             issued_at_ms=1785970000000\n\
             signing_key_id=release-key-1\n"
        );
        let signing_key = deterministic_signing_key(6);
        BrowserBuildProof {
            descriptor: base64::engine::general_purpose::URL_SAFE_NO_PAD
                .encode(descriptor.as_bytes()),
            signature: base64::engine::general_purpose::URL_SAFE_NO_PAD
                .encode(signing_key.sign(descriptor.as_bytes()).to_bytes()),
        }
    }

    fn deterministic_signing_key(index: usize) -> SigningKey {
        let seed = std::array::from_fn(|offset| ((index * 37 + offset) % 256) as u8);
        SigningKey::from_bytes(&seed)
    }

    #[allow(clippy::too_many_arguments)]
    fn signed_authority_envelope<T: Serialize>(
        authority: &T,
        signature_set_id: &str,
        trust_generation: i64,
        role: &str,
        target_audience: &str,
        signed_at_ms: i64,
        signers: &[(&str, usize)],
    ) -> BrowserReleaseAuthorityEnvelope {
        let authority_bytes =
            canonical_browser_release_json(authority).expect("canonical signed authority");
        let mut signature_set = BrowserReleaseSignatureSetAuthority {
            version: 1,
            audience: BROWSER_RELEASE_SIGNATURE_SET_AUDIENCE.to_string(),
            signature_set_id: signature_set_id.to_string(),
            trust_generation,
            role: role.to_string(),
            target_audience: target_audience.to_string(),
            target_sha256: browser_release_authority_sha256(&authority_bytes),
            signed_at_ms,
            signatures: Vec::new(),
        };
        let signature_payload = canonical_browser_release_signature_payload(&signature_set)
            .expect("canonical authority signature payload");
        for (key_id, index) in signers {
            signature_set
                .signatures
                .push(BrowserReleaseDetachedSignatureAuthority {
                    key_id: (*key_id).to_string(),
                    signature: base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(
                        deterministic_signing_key(*index)
                            .sign(&signature_payload)
                            .to_bytes(),
                    ),
                });
        }
        let signature_set_bytes = canonical_browser_release_json(&signature_set)
            .expect("canonical authority signature set");
        BrowserReleaseAuthorityEnvelope {
            canonical_base64url: base64::engine::general_purpose::URL_SAFE_NO_PAD
                .encode(authority_bytes),
            signature_set_base64url: base64::engine::general_purpose::URL_SAFE_NO_PAD
                .encode(signature_set_bytes),
        }
    }

    fn signing_key_revocation_envelope(
        revocation_id: &str,
        revocation_generation: i64,
        subject_id: &str,
        subject_key_index: usize,
    ) -> BrowserReleaseAuthorityEnvelope {
        let subject_key = deterministic_signing_key(subject_key_index)
            .verifying_key()
            .to_bytes();
        let revocation = BrowserReleaseRevocationAuthority {
            version: 1,
            audience: BROWSER_RELEASE_REVOCATION_AUDIENCE.to_string(),
            revocation_id: revocation_id.to_string(),
            revocation_generation,
            trust_generation: 1,
            subject_kind: "signing-key".to_string(),
            subject_id: subject_id.to_string(),
            subject_sha256: browser_release_authority_sha256(&subject_key),
            reason_ref: "incident-BR-signer".to_string(),
            issued_at_ms: 1_785_970_300_000 + revocation_generation,
        };
        signed_authority_envelope(
            &revocation,
            &format!("{revocation_id}-signatures"),
            1,
            "incident",
            BROWSER_RELEASE_REVOCATION_AUDIENCE,
            revocation.issued_at_ms,
            &[("incident-key-1", 1)],
        )
    }

    fn signer_revocation_envelope() -> BrowserReleaseAuthorityEnvelope {
        signing_key_revocation_envelope("browser-revocation-signing-key-1", 2, "promotion-key-1", 2)
    }

    fn rollback_replay_envelope(
        from_activation_sha256: &str,
        from_manifest_sha256: &str,
    ) -> BrowserReleaseAuthorityEnvelope {
        let rollback = BrowserReleaseRollbackAuthority {
            version: 1,
            audience: BROWSER_RELEASE_ROLLBACK_AUDIENCE.to_string(),
            rollback_id: "browser-rollback-policy-rotation".to_string(),
            rollback_generation: 1,
            trust_generation: 1,
            channel: "beta".to_string(),
            from_activation_sha256: from_activation_sha256.to_string(),
            from_manifest_sha256: from_manifest_sha256.to_string(),
            to_manifest_sha256: "0".repeat(64),
            to_activation_sha256: "1".repeat(64),
            canary_evidence_sha256: "2".repeat(64),
            reason_ref: "incident-BR-policy-rotation".to_string(),
            issued_at_ms: 1_785_970_200_001,
        };
        let rollback_bytes =
            canonical_browser_release_json(&rollback).expect("canonical replay rollback");
        let mut signature_set = BrowserReleaseSignatureSetAuthority {
            version: 1,
            audience: BROWSER_RELEASE_SIGNATURE_SET_AUDIENCE.to_string(),
            signature_set_id: "browser-rollback-policy-rotation-signatures".to_string(),
            trust_generation: 1,
            role: "promotion".to_string(),
            target_audience: BROWSER_RELEASE_ROLLBACK_AUDIENCE.to_string(),
            target_sha256: browser_release_authority_sha256(&rollback_bytes),
            signed_at_ms: rollback.issued_at_ms,
            signatures: Vec::new(),
        };
        let signature_payload = canonical_browser_release_signature_payload(&signature_set)
            .expect("canonical rollback signature payload");
        for (key_id, index) in [("promotion-key-1", 2_usize), ("promotion-key-2", 3_usize)] {
            signature_set
                .signatures
                .push(BrowserReleaseDetachedSignatureAuthority {
                    key_id: key_id.to_string(),
                    signature: base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(
                        deterministic_signing_key(index)
                            .sign(&signature_payload)
                            .to_bytes(),
                    ),
                });
        }
        let signature_set_bytes = canonical_browser_release_json(&signature_set)
            .expect("canonical rollback signature set");
        BrowserReleaseAuthorityEnvelope {
            canonical_base64url: base64::engine::general_purpose::URL_SAFE_NO_PAD
                .encode(rollback_bytes),
            signature_set_base64url: base64::engine::general_purpose::URL_SAFE_NO_PAD
                .encode(signature_set_bytes),
        }
    }

    fn seed_exact_rollback_replay(
        pool: &DbPool,
        envelope: &BrowserReleaseAuthorityEnvelope,
        manifest_signature_set_sha256: &str,
        activation_signature_set_sha256: &str,
    ) -> String {
        let (rollback_bytes, signature_set_bytes) =
            decode_browser_release_authority_envelope(envelope)
                .expect("decode replay rollback envelope");
        let rollback = parse_canonical_browser_release_rollback(&rollback_bytes)
            .expect("parse replay rollback");
        let signature_set = parse_canonical_browser_release_signature_set(&signature_set_bytes)
            .expect("parse replay rollback signature set");
        let rollback_sha256 = browser_release_authority_sha256(&rollback_bytes);
        let signature_set_sha256 = browser_release_authority_sha256(&signature_set_bytes);
        let mut connection = pool.get().expect("get rollback replay connection");
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .expect("begin rollback replay transaction");
        let policy = sqlite_browser_release_policy_by_generation(&transaction, 1)
            .expect("load rollback replay policy")
            .expect("rollback replay policy");
        let recorded_at_ms = now_ms();
        transaction
            .execute(
                r#"INSERT INTO jobs_browser_release_manifests (
                     manifest_sha256, manifest_id, manifest_generation, release_id,
                     release_sequence, build_id, app_version, protocol_version,
                     source_commit, electron_version, playwright_version,
                     chromium_revision, release_notes_url, artifact_count,
                     canonical_manifest_base64url,
                     authorization_signature_set_sha256, published_at_ms,
                     recorded_by, recorded_at_ms
                   ) VALUES (?1, 'browser-manifest-rollback-target', 2,
                             'browser-release-rollback-target', 1,
                             'browser-rollback-target', '0.0.1', 1, ?2,
                             '43.1.0', '1.61.1', '1228', ?3, 5, 'e30', ?4,
                             ?5, 'admin-registry-a', ?6)"#,
                params![
                    rollback.to_manifest_sha256,
                    "0".repeat(40),
                    "https://bluey.sh/jobs/browser/releases/browser-release-rollback-target/RELEASE.md",
                    manifest_signature_set_sha256,
                    policy.policy.issued_at_ms + 1,
                    recorded_at_ms,
                ],
            )
            .expect("insert rollback target manifest");
        transaction
            .execute(
                r#"INSERT INTO jobs_browser_release_activations (
                     activation_sha256, activation_id, activation_generation,
                     trust_generation, channel, channel_sequence, manifest_sha256,
                     manifest_signature_set_sha256,
                     authorization_signature_set_sha256,
                     accepted_server_release_ids_json, canary_evidence_sha256,
                     canonical_activation_base64url, issued_at_ms, expires_at_ms,
                     recorded_by, recorded_at_ms
                   ) VALUES (?1, 'browser-activation-rollback-target', 2, 1,
                             'beta', 2, ?2, ?3, ?4, '[]', ?5, 'e30', ?6, ?7,
                             'admin-registry-a', ?8)"#,
                params![
                    rollback.to_activation_sha256,
                    rollback.to_manifest_sha256,
                    manifest_signature_set_sha256,
                    activation_signature_set_sha256,
                    "3".repeat(64),
                    policy.policy.issued_at_ms + 2,
                    policy.policy.expires_at_ms - 1,
                    recorded_at_ms,
                ],
            )
            .expect("insert rollback target activation");
        insert_sqlite_browser_release_signature_set(
            &transaction,
            &signature_set,
            &signature_set_sha256,
            &envelope.signature_set_base64url,
            "admin-registry-a",
            recorded_at_ms,
        )
        .expect("insert replay rollback signature set");
        insert_sqlite_browser_release_rollback(
            &transaction,
            &rollback,
            &rollback_sha256,
            &signature_set_sha256,
            &envelope.canonical_base64url,
            "admin-registry-a",
            recorded_at_ms,
        )
        .expect("insert exact replay rollback");
        let head = sqlite_browser_release_channel_head(&transaction, &rollback.channel)
            .expect("load rollback replay channel head")
            .expect("rollback replay channel head");
        let target = sqlite_browser_release_activation_by_sha256(
            &transaction,
            &rollback.to_activation_sha256,
        )
        .expect("load rollback replay target")
        .expect("rollback replay target");
        let transition = BrowserReleaseChannelTransition::new(
            Some(&head),
            &target,
            "rollback",
            &rollback_sha256,
            Some(&rollback_sha256),
            "admin-registry-a",
            recorded_at_ms,
        )
        .expect("build exact replay rollback transition");
        insert_sqlite_browser_release_channel_transition(&transaction, &transition)
            .expect("insert exact replay rollback transition");
        transaction.commit().expect("commit exact replay rollback");
        rollback_sha256
    }

    fn install_synthetic_rotated_policy(pool: &DbPool) -> String {
        install_synthetic_rotated_policy_with_historical_state_and_origin(
            pool,
            "revoked",
            "https://bluey.sh",
        )
    }

    fn successor_policy_envelope(
        predecessor: &BrowserReleaseTrustPolicyAuthority,
        predecessor_sha256: &str,
        artifact_origin: &str,
    ) -> BrowserReleaseAuthorityEnvelope {
        let mut successor = predecessor.clone();
        successor.policy_id = "browser-trust-policy-root-recovery-2".to_string();
        successor.trust_generation = 2;
        successor.predecessor_policy_sha256 = Some(predecessor_sha256.to_string());
        successor.artifact_origin = artifact_origin.to_string();
        successor.issued_at_ms = predecessor.issued_at_ms + 500_000;
        successor.valid_from_ms = successor.issued_at_ms - 1_000;
        signed_authority_envelope(
            &successor,
            "browser-trust-policy-root-recovery-signatures-2",
            2,
            "root",
            BROWSER_RELEASE_TRUST_POLICY_AUDIENCE,
            successor.issued_at_ms,
            &[("root-key-1", 10), ("root-key-2", 11)],
        )
    }

    fn successor_activation_envelope(fixture: &RegistryFixture) -> BrowserReleaseAuthorityEnvelope {
        let mut activation = parse_canonical_browser_release_activation(&decode_fixture(
            &fixture.activation.canonical,
        ))
        .expect("parse predecessor activation");
        activation.activation_id = "browser-activation-beta-origin-2".to_string();
        activation.activation_generation = 2;
        activation.trust_generation = 2;
        activation.channel_sequence = 2;
        activation.issued_at_ms += 600_000;
        signed_authority_envelope(
            &activation,
            "browser-activation-beta-origin-signatures-2",
            2,
            "promotion",
            BROWSER_RELEASE_ACTIVATION_AUDIENCE,
            activation.issued_at_ms,
            &[("promotion-key-3", 4), ("promotion-key-4", 5)],
        )
    }

    fn install_synthetic_rotated_policy_with_historical_state(
        pool: &DbPool,
        historical_state: &str,
    ) -> String {
        install_synthetic_rotated_policy_with_historical_state_and_origin(
            pool,
            historical_state,
            "https://bluey.sh",
        )
    }

    fn install_synthetic_rotated_policy_with_historical_state_and_origin(
        pool: &DbPool,
        historical_state: &str,
        artifact_origin: &str,
    ) -> String {
        assert!(matches!(historical_state, "retired" | "revoked"));
        let mut connection = pool.get().expect("get rotated-policy connection");
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .expect("begin rotated-policy transaction");
        let current = sqlite_latest_browser_release_policy(&transaction)
            .expect("load predecessor policy")
            .expect("predecessor policy");
        let mut successor = current.policy.clone();
        successor.policy_id = "browser-trust-policy-2".to_string();
        successor.trust_generation = 2;
        successor.predecessor_policy_sha256 = Some(current.policy_sha256.clone());
        successor.artifact_origin = artifact_origin.to_string();
        successor.issued_at_ms = current.policy.issued_at_ms + 500_000;
        successor.valid_from_ms = successor.issued_at_ms - 1_000;
        for key in &mut successor.keys {
            key.maximum_trust_generation = 2;
            if key.role != "root" {
                key.state = historical_state.to_string();
            }
        }
        let valid_until_ms = successor.expires_at_ms - 1;
        for (key_id, role, index) in [
            ("incident-key-2", "incident", 14_usize),
            ("promotion-key-3", "promotion", 4_usize),
            ("promotion-key-4", "promotion", 5_usize),
            ("release-key-3", "release", 8_usize),
            ("release-key-4", "release", 9_usize),
        ] {
            successor.keys.push(BrowserReleaseTrustKeyAuthority {
                key_id: key_id.to_string(),
                role: role.to_string(),
                public_key: base64::engine::general_purpose::URL_SAFE_NO_PAD
                    .encode(deterministic_signing_key(index).verifying_key().to_bytes()),
                state: "active".to_string(),
                valid_from_ms: successor.valid_from_ms,
                valid_until_ms,
                minimum_trust_generation: 2,
                maximum_trust_generation: 2,
            });
        }
        successor
            .keys
            .sort_by(|left, right| left.key_id.cmp(&right.key_id));
        let canonical =
            canonical_browser_release_json(&successor).expect("canonical rotated policy");
        parse_canonical_browser_release_trust_policy(&canonical)
            .expect("rotated policy remains strictly valid");
        let policy_sha256 = browser_release_authority_sha256(&canonical);
        let canonical_base64url =
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&canonical);
        insert_sqlite_browser_release_policy(
            &transaction,
            &successor,
            &policy_sha256,
            &canonical_base64url,
            &current.authorization_signature_set_sha256,
            "admin-registry-rotation",
            now_ms(),
        )
        .expect("insert synthetic rotated policy");
        transaction
            .commit()
            .expect("commit synthetic rotated policy");
        policy_sha256
    }

    fn fixture_manifest_request(fixture: &RegistryFixture) -> BrowserReleaseManifestImportRequest {
        BrowserReleaseManifestImportRequest {
            canonical_base64url: fixture.manifest.canonical.clone(),
            signature_set_base64url: fixture.manifest.signature_set.clone(),
            build_proofs: vec![
                deterministic_build_proof("darwin", "arm64"),
                deterministic_build_proof("darwin", "x64"),
                deterministic_build_proof("windows", "x64"),
            ],
        }
    }

    #[derive(Debug, Clone)]
    pub(super) struct InstalledSignedBrowserReleaseFixture {
        pub(super) descriptor: VerifiedBrowserBuildDescriptor,
        pub(super) binding: BrowserReleaseClaimBinding,
        pub(super) runtime_target: AtsCertificationRuntimeTarget,
    }

    fn signed_browser_release_local_runtime_target(
        descriptor: &VerifiedBrowserBuildDescriptor,
        binding: &BrowserReleaseClaimBinding,
    ) -> AtsCertificationRuntimeTarget {
        assert_eq!(binding.release_id, descriptor.release_id);
        assert_eq!(binding.build_id, descriptor.build_id);
        assert_eq!(binding.app_version, descriptor.app_version);
        assert_eq!(binding.protocol_version, descriptor.protocol_version);
        assert_eq!(
            binding.build_descriptor_sha256,
            descriptor.descriptor_sha256
        );

        let platform = match binding.platform.as_str() {
            "darwin" => "macos",
            "windows" => "windows",
            platform => panic!("unsupported signed Browser fixture platform {platform}"),
        };
        let architecture = match binding.architecture.as_str() {
            "arm64" => "arm64",
            "x64" => "x86_64",
            architecture => {
                panic!("unsupported signed Browser fixture architecture {architecture}")
            }
        };
        let attestation = AtsCertificationRuntimeAttestation::Local {
            platform: platform.to_string(),
            architecture: architecture.to_string(),
            browser_release_manifest_sha256: binding.manifest_sha256.clone(),
            browser_artifact_sha256: binding.artifact_sha256.clone(),
            browser_build_descriptor_sha256: binding.build_descriptor_sha256.clone(),
            automation_bundle_sha256: binding.automation_bundle_sha256.clone(),
            playwright_version: descriptor.playwright_version.clone(),
            chromium_revision: descriptor.chromium_revision.clone(),
            chromium_executable_sha256: binding.chromium_executable_sha256.clone(),
        };
        let runtime_sha256 = ats_certification_sha256(
            &ats_certification_canonical_json(&attestation)
                .expect("canonical signed Browser runtime attestation"),
        );
        let runtime_target = AtsCertificationRuntimeTarget {
            runtime_kind: "local".to_string(),
            runtime_id: format!("local:{}:{}:{}", binding.release_id, platform, architecture),
            runtime_sha256,
            platform: platform.to_string(),
            architecture: architecture.to_string(),
            automation_bundle_sha256: binding.automation_bundle_sha256.clone(),
            browser_release_manifest_sha256: Some(binding.manifest_sha256.clone()),
            browser_artifact_sha256: Some(binding.artifact_sha256.clone()),
            browser_build_descriptor_sha256: Some(binding.build_descriptor_sha256.clone()),
            runner_build_id: None,
            runner_image_sha256: None,
            playwright_version: descriptor.playwright_version.clone(),
            chromium_revision: descriptor.chromium_revision.clone(),
            chromium_executable_sha256: binding.chromium_executable_sha256.clone(),
        };
        validate_ats_runtime_target(&runtime_target)
            .expect("validate signed Browser local ATS runtime target");
        runtime_target
    }

    pub(super) fn install_signed_browser_release_fixture(
        pool: &DbPool,
        account_id: &str,
    ) -> InstalledSignedBrowserReleaseFixture {
        let fixture = registry_fixture();
        let policy_bytes = decode_fixture(&fixture.trust_policy.canonical);
        let policy = parse_canonical_browser_release_trust_policy(&policy_bytes)
            .expect("parse signed Browser fixture policy");
        let policy_sha256 = browser_release_authority_sha256(&policy_bytes);
        let trust_envelope = BrowserReleaseAuthorityEnvelope {
            canonical_base64url: fixture.trust_policy.canonical.clone(),
            signature_set_base64url: fixture.trust_policy.signature_set.clone(),
        };
        {
            let _environment_lock = ROOT_ANCHOR_ENV_LOCK.lock().expect("lock root anchor env");
            let previous = std::env::var_os(BROWSER_RELEASE_ROOT_TRUST_ANCHOR_ENV);
            let _environment_guard = RootAnchorEnvironmentGuard(previous);
            install_fixture_root_anchor(&policy);
            let trust = import_browser_release_trust_policy(pool, &trust_envelope, "test-suite")
                .expect("import signed Browser fixture trust policy");
            assert_eq!(trust.authority_sha256, policy_sha256);
        }

        let manifest_request = fixture_manifest_request(&fixture);
        let manifest = import_browser_release_manifest(pool, &manifest_request, "test-suite")
            .expect("import signed Browser fixture manifest");
        let descriptor = manifest_request
            .build_proofs
            .iter()
            .map(|proof| {
                verify_browser_build_proof_against_release_policy(proof, &policy)
                    .expect("verify signed Browser fixture build proof")
            })
            .find(|descriptor| {
                descriptor.platform == "darwin" && descriptor.architecture == "arm64"
            })
            .expect("signed Browser fixture has a Darwin arm64 descriptor");

        let mut activation = parse_canonical_browser_release_activation(&decode_fixture(
            &fixture.activation.canonical,
        ))
        .expect("parse signed Browser fixture activation");
        assert_eq!(activation.manifest_sha256, manifest.authority_sha256);
        assert_eq!(
            activation.signature_set_sha256,
            manifest.signature_set_sha256
        );
        activation.activation_id = "browser-activation-beta-test-suite-1".to_string();
        activation.accepted_server_release_ids = vec![
            "alternate-test-server".to_string(),
            "test-server".to_string(),
        ];
        let activation_envelope = signed_authority_envelope(
            &activation,
            "browser-activation-beta-test-suite-promotion-set",
            activation.trust_generation,
            "promotion",
            BROWSER_RELEASE_ACTIVATION_AUDIENCE,
            activation.issued_at_ms,
            &[("promotion-key-1", 2), ("promotion-key-2", 3)],
        );
        let activation_import =
            import_browser_release_activation(pool, &activation_envelope, "test-suite")
                .expect("import signed Browser fixture activation");
        let active = apply_browser_release_activation(
            pool,
            &ApplyBrowserReleaseActivationRequest {
                activation_sha256: activation_import.authority_sha256,
                expected_head_revision: 0,
                expected_transition_sha256: None,
            },
            "test-suite",
        )
        .expect("apply signed Browser fixture activation");
        assert!(active.available);

        let assignment = assign_browser_release_account_channel(
            pool,
            account_id,
            &AssignBrowserReleaseChannelRequest {
                assignment_generation: 1,
                predecessor_assignment_sha256: None,
                channel: activation.channel,
                reason_ref: "signed-test-fixture".to_string(),
                assigned_at_ms: activation.issued_at_ms,
            },
            "test-suite",
        )
        .expect("assign signed Browser fixture channel");
        let binding =
            browser_release_for_claim(pool, account_id, &descriptor, "test-server", now_ms())
                .expect("resolve signed Browser fixture release")
                .expect("signed Browser fixture release is available");
        assert_eq!(binding.assignment_sha256, assignment.assignment_sha256);
        assert_eq!(
            binding.channel_transition_sha256,
            active
                .transition_sha256
                .expect("signed Browser fixture has an active transition")
        );
        let runtime_target = signed_browser_release_local_runtime_target(&descriptor, &binding);
        InstalledSignedBrowserReleaseFixture {
            descriptor,
            binding,
            runtime_target,
        }
    }

    fn manifest_request_for_authority(
        manifest: &BrowserReleaseManifestAuthority,
        signature_set_id: &str,
    ) -> BrowserReleaseManifestImportRequest {
        let envelope = signed_authority_envelope(
            manifest,
            signature_set_id,
            1,
            "release",
            BROWSER_RELEASE_MANIFEST_AUDIENCE,
            manifest.published_at_ms,
            &[("release-key-1", 6), ("release-key-2", 7)],
        );
        BrowserReleaseManifestImportRequest {
            canonical_base64url: envelope.canonical_base64url,
            signature_set_base64url: envelope.signature_set_base64url,
            build_proofs: vec![
                deterministic_build_proof("darwin", "arm64"),
                deterministic_build_proof("darwin", "x64"),
                deterministic_build_proof("windows", "x64"),
            ],
        }
    }

    fn assert_successor_policy_revoked_key_fence(
        pool: &DbPool,
        activation_sha256: &str,
        manifest_sha256: &str,
    ) {
        let mut connection = pool.get().expect("get successor-policy connection");
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .expect("begin successor-policy transaction");
        let current = sqlite_latest_browser_release_policy(&transaction)
            .expect("load current policy")
            .expect("current policy");
        let activation =
            sqlite_browser_release_activation_by_sha256(&transaction, activation_sha256)
                .expect("load current activation")
                .expect("current activation");
        let manifest = sqlite_browser_release_manifest_by_sha256(&transaction, manifest_sha256)
            .expect("load current manifest")
            .expect("current manifest");
        let release_key = current
            .policy
            .keys
            .iter()
            .find(|key| key.key_id == "release-key-1")
            .expect("fixture release key");
        let revoked_policy_sha256 = "2".repeat(64);
        transaction
            .execute(
                r#"INSERT INTO jobs_browser_release_trust_policies (
                     policy_sha256, policy_id, trust_generation,
                     predecessor_policy_sha256, predecessor_trust_generation,
                     root_threshold, release_threshold, promotion_threshold,
                     incident_threshold, key_count, canonical_policy_base64url,
                     authorization_signature_set_sha256, issued_at_ms, valid_from_ms,
                     expires_at_ms, recorded_by, recorded_at_ms
                   ) VALUES (?1, 'successor-policy-revoked-key', 2, ?2, 1,
                             1, 1, 1, 1, 4, 'e30', ?3, ?4, ?5, ?6,
                             'admin-registry-a', ?7)"#,
                params![
                    revoked_policy_sha256,
                    current.policy_sha256,
                    current.authorization_signature_set_sha256,
                    current.policy.issued_at_ms + 1,
                    current.policy.valid_from_ms,
                    current.policy.expires_at_ms,
                    now_ms(),
                ],
            )
            .expect("insert successor policy with revoked historical signer");
        transaction
            .execute(
                r#"INSERT INTO jobs_browser_release_trust_keys (
                     policy_sha256, trust_generation, key_id, role,
                     public_key_base64url, state, valid_from_ms, valid_until_ms,
                     minimum_trust_generation, maximum_trust_generation
                   ) VALUES (?1, 2, ?2, 'release', ?3, 'revoked', ?4, ?5, 1, 2)"#,
                params![
                    revoked_policy_sha256,
                    release_key.key_id,
                    release_key.public_key,
                    release_key.valid_from_ms,
                    release_key.valid_until_ms,
                ],
            )
            .expect("insert revoked historical signer state");
        let mut revoked_policy = current.clone();
        revoked_policy.policy_sha256 = revoked_policy_sha256.clone();
        revoked_policy.trust_generation = 2;
        assert!(matches!(
            ensure_sqlite_browser_release_target_not_revoked(
                &transaction,
                &activation,
                &manifest,
                &revoked_policy,
            ),
            Err(BrowserReleaseRegistryError::Revoked)
        ));

        let retired_policy_sha256 = "3".repeat(64);
        transaction
            .execute(
                r#"INSERT INTO jobs_browser_release_trust_policies (
                     policy_sha256, policy_id, trust_generation,
                     predecessor_policy_sha256, predecessor_trust_generation,
                     root_threshold, release_threshold, promotion_threshold,
                     incident_threshold, key_count, canonical_policy_base64url,
                     authorization_signature_set_sha256, issued_at_ms, valid_from_ms,
                     expires_at_ms, recorded_by, recorded_at_ms
                   ) VALUES (?1, 'successor-policy-retired-key', 3, ?2, 2,
                             1, 1, 1, 1, 4, 'e30', ?3, ?4, ?5, ?6,
                             'admin-registry-a', ?7)"#,
                params![
                    retired_policy_sha256,
                    revoked_policy_sha256,
                    current.authorization_signature_set_sha256,
                    current.policy.issued_at_ms + 2,
                    current.policy.valid_from_ms,
                    current.policy.expires_at_ms,
                    now_ms(),
                ],
            )
            .expect("insert successor policy with retired historical signer");
        transaction
            .execute(
                r#"INSERT INTO jobs_browser_release_trust_keys (
                     policy_sha256, trust_generation, key_id, role,
                     public_key_base64url, state, valid_from_ms, valid_until_ms,
                     minimum_trust_generation, maximum_trust_generation
                   ) VALUES (?1, 3, ?2, 'release', ?3, 'retired', ?4, ?5, 1, 3)"#,
                params![
                    retired_policy_sha256,
                    release_key.key_id,
                    release_key.public_key,
                    release_key.valid_from_ms,
                    release_key.valid_until_ms,
                ],
            )
            .expect("insert retired historical signer state");
        let mut retired_policy = current;
        retired_policy.policy_sha256 = retired_policy_sha256;
        retired_policy.trust_generation = 3;
        ensure_sqlite_browser_release_target_not_revoked(
            &transaction,
            &activation,
            &manifest,
            &retired_policy,
        )
        .expect("retired historical signer remains valid");
        transaction
            .rollback()
            .expect("rollback synthetic successor policies");
    }

    fn assert_revocation_identity_includes_subject_id(
        pool: &DbPool,
        signature_set_sha256: &str,
        issued_at_ms: i64,
    ) {
        let mut connection = pool.get().expect("get revocation-identity connection");
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .expect("begin revocation-identity transaction");
        let shared_subject_sha256 = "a".repeat(64);
        for (digest, id, generation) in [
            ("4".repeat(64), "artifact-subject-a", 1_i64),
            ("5".repeat(64), "artifact-subject-b", 2_i64),
        ] {
            transaction
                .execute(
                    r#"INSERT INTO jobs_browser_release_revocations (
                         revocation_sha256, revocation_id, revocation_generation,
                         trust_generation, subject_kind, subject_id, subject_sha256,
                         reason_ref, canonical_revocation_base64url,
                         authorization_signature_set_sha256, issued_at_ms,
                         recorded_by, recorded_at_ms
                       ) VALUES (?1, ?2, ?3, 1, 'artifact', ?4, ?5,
                                 'identity-test', 'e30', ?6, ?7,
                                 'admin-registry-a', ?8)"#,
                    params![
                        digest,
                        format!("revocation-{id}"),
                        generation,
                        id,
                        shared_subject_sha256,
                        signature_set_sha256,
                        issued_at_ms,
                        now_ms(),
                    ],
                )
                .expect("insert exact revocation subject identity");
        }
        let unknown_subject = BrowserReleaseRevocationAuthority {
            version: 1,
            audience: "bluey-jobs-browser-release-revocation-v1".to_string(),
            revocation_id: "revocation-artifact-subject-c".to_string(),
            revocation_generation: 3,
            trust_generation: 1,
            subject_kind: "artifact".to_string(),
            subject_id: "artifact-subject-c".to_string(),
            subject_sha256: shared_subject_sha256.clone(),
            reason_ref: "identity-test".to_string(),
            issued_at_ms,
        };
        assert!(
            sqlite_browser_release_revocation_identity(&transaction, &unknown_subject)
                .expect("query unknown exact revocation subject")
                .is_none()
        );
        let mut known_subject = unknown_subject;
        known_subject.subject_id = "artifact-subject-a".to_string();
        assert!(
            sqlite_browser_release_revocation_identity(&transaction, &known_subject)
                .expect("query known exact revocation subject")
                .is_some()
        );
        transaction
            .rollback()
            .expect("rollback synthetic revocation identities");
    }

    #[test]
    fn sqlite_registry_imports_applies_and_revokes_exact_fixture() {
        let _environment_lock = ROOT_ANCHOR_ENV_LOCK.lock().expect("lock root anchor env");
        let previous = std::env::var_os(BROWSER_RELEASE_ROOT_TRUST_ANCHOR_ENV);
        let _environment_guard = RootAnchorEnvironmentGuard(previous);
        let fixture = registry_fixture();
        let policy_bytes = decode_fixture(&fixture.trust_policy.canonical);
        let policy = parse_canonical_browser_release_trust_policy(&policy_bytes)
            .expect("parse fixture policy");
        install_fixture_root_anchor(&policy);
        let pool = sqlite_registry_pool();
        let trust_envelope = BrowserReleaseAuthorityEnvelope {
            canonical_base64url: fixture.trust_policy.canonical.clone(),
            signature_set_base64url: fixture.trust_policy.signature_set.clone(),
        };

        let imported =
            import_browser_release_trust_policy(&pool, &trust_envelope, "admin-registry-a")
                .expect("import fixture trust policy");
        assert!(!imported.replayed);
        let replayed =
            import_browser_release_trust_policy(&pool, &trust_envelope, "admin-registry-b")
                .expect("replay fixture trust policy across administrators");
        assert!(replayed.replayed);
        let connection = pool.get().expect("get registry actor connection");
        let recorded_by: String = connection
            .query_row(
                "SELECT recorded_by FROM jobs_browser_release_trust_policies \
                 WHERE policy_sha256 = ?1",
                params![imported.authority_sha256],
                |row| row.get(0),
            )
            .expect("load immutable registry actor");
        assert_eq!(recorded_by, "admin-registry-a");
        drop(connection);
        assert_revocation_identity_includes_subject_id(
            &pool,
            &imported.signature_set_sha256,
            policy.issued_at_ms,
        );

        let assignment_request = AssignBrowserReleaseChannelRequest {
            assignment_generation: 1,
            predecessor_assignment_sha256: None,
            channel: "beta".to_string(),
            reason_ref: "registry-lifecycle".to_string(),
            assigned_at_ms: policy.issued_at_ms,
        };
        let assignment = assign_browser_release_account_channel(
            &pool,
            "acct-browser-registry",
            &assignment_request,
            "admin-registry-a",
        )
        .expect("assign Browser release channel");
        assert!(!assignment.replayed);
        assert!(
            assign_browser_release_account_channel(
                &pool,
                "acct-browser-registry",
                &assignment_request,
                "admin-registry-a",
            )
            .expect("replay Browser release channel assignment")
            .replayed
        );

        let manifest_request = fixture_manifest_request(&fixture);
        let manifest =
            import_browser_release_manifest(&pool, &manifest_request, "admin-registry-a")
                .expect("import fixture manifest with all target proofs");
        assert!(
            import_browser_release_manifest(&pool, &manifest_request, "admin-registry-b")
                .expect("replay exact fixture manifest across administrators")
                .replayed
        );
        let activation = import_browser_release_activation(
            &pool,
            &BrowserReleaseAuthorityEnvelope {
                canonical_base64url: fixture.activation.canonical.clone(),
                signature_set_base64url: fixture.activation.signature_set.clone(),
            },
            "admin-registry-a",
        )
        .expect("import fixture activation");
        assert_eq!(
            activation.manifest_authorization_signature_set_sha256,
            manifest.manifest_authorization_signature_set_sha256
        );
        let active = apply_browser_release_activation(
            &pool,
            &ApplyBrowserReleaseActivationRequest {
                activation_sha256: activation.authority_sha256,
                expected_head_revision: 0,
                expected_transition_sha256: None,
            },
            "admin-registry-a",
        )
        .expect("apply fixture activation");
        assert!(active.available);
        assert_eq!(active.head_revision, 1);
        assert!(active.transition_sha256.is_some());
        assert_successor_policy_revoked_key_fence(
            &pool,
            active
                .activation_sha256
                .as_deref()
                .expect("active activation digest"),
            active
                .manifest_sha256
                .as_deref()
                .expect("active manifest digest"),
        );

        append_browser_release_revocation(
            &pool,
            &BrowserReleaseAuthorityEnvelope {
                canonical_base64url: fixture.revocation.canonical,
                signature_set_base64url: fixture.revocation.signature_set,
            },
            "admin-registry-a",
        )
        .expect("append fixture release revocation");
        assert!(
            import_browser_release_manifest(&pool, &manifest_request, "admin-registry-b")
                .expect("replay stored manifest after release revocation")
                .replayed
        );
        assert!(
            import_browser_release_activation(
                &pool,
                &BrowserReleaseAuthorityEnvelope {
                    canonical_base64url: fixture.activation.canonical,
                    signature_set_base64url: fixture.activation.signature_set,
                },
                "admin-registry-b",
            )
            .expect("replay stored activation after release revocation")
            .replayed
        );
        let revoked = browser_release_channel_status(&pool, "beta")
            .expect("load revoked fixture channel status");
        assert!(!revoked.available);
        assert_eq!(
            revoked.unavailability_reason.as_deref(),
            Some("release-revoked")
        );
        let replayed_apply = apply_browser_release_activation(
            &pool,
            &ApplyBrowserReleaseActivationRequest {
                activation_sha256: revoked
                    .activation_sha256
                    .clone()
                    .expect("revoked active activation digest"),
                expected_head_revision: 0,
                expected_transition_sha256: None,
            },
            "admin-registry-b",
        )
        .expect("replay exact applied activation after revocation");
        assert!(!replayed_apply.available);
        assert_eq!(
            replayed_apply.unavailability_reason.as_deref(),
            Some("release-revoked")
        );
    }

    #[test]
    fn sqlite_manifest_replay_rejects_changed_or_missing_child_rows() {
        let _environment_lock = ROOT_ANCHOR_ENV_LOCK.lock().expect("lock root anchor env");
        let previous = std::env::var_os(BROWSER_RELEASE_ROOT_TRUST_ANCHOR_ENV);
        let _environment_guard = RootAnchorEnvironmentGuard(previous);
        let fixture = registry_fixture();
        let policy_bytes = decode_fixture(&fixture.trust_policy.canonical);
        let policy = parse_canonical_browser_release_trust_policy(&policy_bytes)
            .expect("parse fixture policy");
        install_fixture_root_anchor(&policy);
        let pool = sqlite_registry_pool();
        import_browser_release_trust_policy(
            &pool,
            &BrowserReleaseAuthorityEnvelope {
                canonical_base64url: fixture.trust_policy.canonical.clone(),
                signature_set_base64url: fixture.trust_policy.signature_set.clone(),
            },
            "admin-registry-a",
        )
        .expect("import fixture trust policy");
        let manifest_request = fixture_manifest_request(&fixture);
        let manifest =
            import_browser_release_manifest(&pool, &manifest_request, "admin-registry-a")
                .expect("import fixture manifest");
        assert!(
            import_browser_release_manifest(&pool, &manifest_request, "admin-registry-b")
                .expect("replay exact fixture manifest")
                .replayed
        );

        let connection = pool.get().expect("get manifest replay connection");
        connection
            .execute_batch(
                r#"DROP TRIGGER trg_jobs_browser_release_signatures_no_update;
                   DROP TRIGGER trg_jobs_browser_release_signatures_no_delete;
                   DROP TRIGGER trg_jobs_browser_release_artifacts_no_update;
                   DROP TRIGGER trg_jobs_browser_release_artifacts_no_delete;
                   DROP TRIGGER trg_jobs_browser_release_runtime_components_no_update;
                   DROP TRIGGER trg_jobs_browser_release_runtime_components_no_delete;"#,
            )
            .expect("disable immutable-row triggers to simulate storage corruption");
        let (signature_key_id, signature_base64url): (String, String) = connection
            .query_row(
                r#"SELECT key_id, signature_base64url
                     FROM jobs_browser_release_signatures
                    WHERE signature_set_sha256 = ?1
                    ORDER BY key_id LIMIT 1"#,
                params![manifest.signature_set_sha256],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("load stored manifest signature");
        connection
            .execute(
                "DELETE FROM jobs_browser_release_signatures \
                 WHERE signature_set_sha256 = ?1 AND key_id = ?2",
                params![manifest.signature_set_sha256, signature_key_id],
            )
            .expect("remove stored manifest signature");
        let missing_signature =
            import_browser_release_manifest(&pool, &manifest_request, "admin-registry-b")
                .expect_err("reject manifest replay with a missing detached signature");
        assert!(matches!(
            missing_signature,
            BrowserReleaseRegistryError::IdentityConflict
        ));
        connection
            .execute(
                r#"INSERT INTO jobs_browser_release_signatures (
                     signature_set_sha256, key_id, signature_base64url
                   ) VALUES (?1, ?2, ?3)"#,
                params![
                    manifest.signature_set_sha256,
                    signature_key_id,
                    signature_base64url,
                ],
            )
            .expect("restore stored manifest signature");

        let (artifact_id, artifact_filename): (String, String) = connection
            .query_row(
                r#"SELECT artifact_id, artifact_filename
                     FROM jobs_browser_release_artifacts
                    WHERE manifest_sha256 = ?1
                    ORDER BY artifact_id LIMIT 1"#,
                params![manifest.authority_sha256],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("load stored manifest artifact");
        connection
            .execute(
                "UPDATE jobs_browser_release_artifacts SET artifact_filename = 'changed.exe' \
                 WHERE artifact_id = ?1",
                params![artifact_id],
            )
            .expect("change stored manifest artifact");
        let changed_artifact =
            import_browser_release_manifest(&pool, &manifest_request, "admin-registry-b")
                .expect_err("reject manifest replay with a changed artifact");
        assert!(matches!(
            changed_artifact,
            BrowserReleaseRegistryError::IdentityConflict
        ));
        connection
            .execute(
                "UPDATE jobs_browser_release_artifacts SET artifact_filename = ?1 \
                 WHERE artifact_id = ?2",
                params![artifact_filename, artifact_id],
            )
            .expect("restore stored manifest artifact");
        let (automation_bundle_sha256, chromium_executable_sha256): (String, String) = connection
            .query_row(
                "SELECT automation_bundle_sha256, chromium_executable_sha256 \
                       FROM jobs_browser_release_artifact_runtime_components \
                      WHERE manifest_sha256 = ?1 AND artifact_id = ?2",
                params![manifest.authority_sha256, artifact_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("load stored Browser runtime components");
        connection
            .execute(
                "UPDATE jobs_browser_release_artifact_runtime_components \
                    SET automation_bundle_sha256 = ?1 \
                  WHERE manifest_sha256 = ?2 AND artifact_id = ?3",
                params!["0".repeat(64), manifest.authority_sha256, artifact_id],
            )
            .expect("corrupt stored automation bundle projection");
        let changed_runtime =
            import_browser_release_manifest(&pool, &manifest_request, "admin-registry-b")
                .expect_err("reject manifest replay with changed runtime components");
        assert!(matches!(
            changed_runtime,
            BrowserReleaseRegistryError::IdentityConflict
        ));
        connection
            .execute(
                "UPDATE jobs_browser_release_artifact_runtime_components \
                    SET automation_bundle_sha256 = ?1, chromium_executable_sha256 = ?2 \
                  WHERE manifest_sha256 = ?3 AND artifact_id = ?4",
                params![
                    automation_bundle_sha256,
                    chromium_executable_sha256,
                    manifest.authority_sha256,
                    artifact_id,
                ],
            )
            .expect("restore stored Browser runtime components");
        connection
            .execute(
                "UPDATE jobs_browser_release_artifact_runtime_components \
                    SET chromium_executable_sha256 = ?1 \
                  WHERE manifest_sha256 = ?2 AND artifact_id = ?3",
                params!["0".repeat(64), manifest.authority_sha256, artifact_id],
            )
            .expect("corrupt stored Chromium executable projection");
        let changed_chromium =
            import_browser_release_manifest(&pool, &manifest_request, "admin-registry-b")
                .expect_err("reject manifest replay with changed Chromium components");
        assert!(matches!(
            changed_chromium,
            BrowserReleaseRegistryError::IdentityConflict
        ));
        connection
            .execute(
                "UPDATE jobs_browser_release_artifact_runtime_components \
                    SET chromium_executable_sha256 = ?1 \
                  WHERE manifest_sha256 = ?2 AND artifact_id = ?3",
                params![
                    chromium_executable_sha256,
                    manifest.authority_sha256,
                    artifact_id,
                ],
            )
            .expect("restore stored Chromium executable projection");
        connection
            .execute(
                "DELETE FROM jobs_browser_release_artifact_runtime_components \
                  WHERE manifest_sha256 = ?1 AND artifact_id = ?2",
                params![manifest.authority_sha256, artifact_id],
            )
            .expect("remove stored Browser runtime components");
        let missing_runtime =
            import_browser_release_manifest(&pool, &manifest_request, "admin-registry-b")
                .expect_err("reject manifest replay with missing runtime components");
        assert!(matches!(
            missing_runtime,
            BrowserReleaseRegistryError::IdentityConflict
        ));
        connection
            .execute(
                "DELETE FROM jobs_browser_release_artifacts WHERE artifact_id = ?1",
                params![artifact_id],
            )
            .expect("remove stored manifest artifact");
        drop(connection);
        let missing_artifact =
            import_browser_release_manifest(&pool, &manifest_request, "admin-registry-b")
                .expect_err("reject manifest replay with a missing artifact");
        assert!(matches!(
            missing_artifact,
            BrowserReleaseRegistryError::IdentityConflict
        ));
    }

    #[test]
    fn sqlite_exact_replay_survives_policy_rotation_and_revoked_historical_signers() {
        let _environment_lock = ROOT_ANCHOR_ENV_LOCK.lock().expect("lock root anchor env");
        let previous = std::env::var_os(BROWSER_RELEASE_ROOT_TRUST_ANCHOR_ENV);
        let _environment_guard = RootAnchorEnvironmentGuard(previous);
        let fixture = registry_fixture();
        let policy_bytes = decode_fixture(&fixture.trust_policy.canonical);
        let policy = parse_canonical_browser_release_trust_policy(&policy_bytes)
            .expect("parse fixture policy");
        install_fixture_root_anchor(&policy);
        let pool = sqlite_registry_pool();
        let trust = import_browser_release_trust_policy(
            &pool,
            &BrowserReleaseAuthorityEnvelope {
                canonical_base64url: fixture.trust_policy.canonical.clone(),
                signature_set_base64url: fixture.trust_policy.signature_set.clone(),
            },
            "admin-registry-a",
        )
        .expect("import fixture trust policy");
        let manifest_request = fixture_manifest_request(&fixture);
        let manifest =
            import_browser_release_manifest(&pool, &manifest_request, "admin-registry-a")
                .expect("import fixture manifest");
        let activation_envelope = BrowserReleaseAuthorityEnvelope {
            canonical_base64url: fixture.activation.canonical.clone(),
            signature_set_base64url: fixture.activation.signature_set.clone(),
        };
        let activation =
            import_browser_release_activation(&pool, &activation_envelope, "admin-registry-a")
                .expect("import fixture activation");
        let release_revocation_envelope = BrowserReleaseAuthorityEnvelope {
            canonical_base64url: fixture.revocation.canonical.clone(),
            signature_set_base64url: fixture.revocation.signature_set.clone(),
        };
        let release_revocation = append_browser_release_revocation(
            &pool,
            &release_revocation_envelope,
            "admin-registry-a",
        )
        .expect("append fixture release revocation");
        let signer_revocation_envelope = signer_revocation_envelope();
        let signer_revocation = append_browser_release_revocation(
            &pool,
            &signer_revocation_envelope,
            "admin-registry-a",
        )
        .expect("append historical promotion-signer revocation");

        let rotated_policy_sha256 =
            install_synthetic_rotated_policy_with_historical_state_and_origin(
                &pool,
                "revoked",
                "https://artifacts.example",
            );
        assert_ne!(rotated_policy_sha256, trust.authority_sha256);
        for replay in [
            import_browser_release_manifest(&pool, &manifest_request, "admin-registry-b")
                .expect("replay manifest after trust rotation"),
            import_browser_release_activation(&pool, &activation_envelope, "admin-registry-b")
                .expect("replay activation after trust rotation"),
            append_browser_release_revocation(
                &pool,
                &release_revocation_envelope,
                "admin-registry-b",
            )
            .expect("replay release revocation after trust rotation"),
            append_browser_release_revocation(
                &pool,
                &signer_revocation_envelope,
                "admin-registry-b",
            )
            .expect("replay signer revocation after trust rotation"),
        ] {
            assert!(replay.replayed);
            assert_eq!(replay.trust_policy_sha256, trust.authority_sha256);
        }

        let connection = pool.get().expect("get rotated replay actor connection");
        for (table, digest) in [
            (
                "jobs_browser_release_manifests",
                manifest.authority_sha256.as_str(),
            ),
            (
                "jobs_browser_release_activations",
                activation.authority_sha256.as_str(),
            ),
            (
                "jobs_browser_release_revocations",
                release_revocation.authority_sha256.as_str(),
            ),
            (
                "jobs_browser_release_revocations",
                signer_revocation.authority_sha256.as_str(),
            ),
        ] {
            let digest_column = match table {
                "jobs_browser_release_manifests" => "manifest_sha256",
                "jobs_browser_release_activations" => "activation_sha256",
                _ => "revocation_sha256",
            };
            let recorded_by: String = connection
                .query_row(
                    &format!("SELECT recorded_by FROM {table} WHERE {digest_column} = ?1"),
                    params![digest],
                    |row| row.get(0),
                )
                .expect("load first-writer actor after replay");
            assert_eq!(recorded_by, "admin-registry-a");
        }
        drop(connection);

        let mut conflicting_manifest =
            parse_canonical_browser_release_manifest(&decode_fixture(&fixture.manifest.canonical))
                .expect("parse conflicting manifest base");
        conflicting_manifest.app_version = "0.1.1".to_string();
        let conflicting_manifest_bytes = canonical_browser_release_json(&conflicting_manifest)
            .expect("canonical conflicting manifest");
        let mut conflicting_request = fixture_manifest_request(&fixture);
        conflicting_request.canonical_base64url =
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(conflicting_manifest_bytes);
        let conflict =
            import_browser_release_manifest(&pool, &conflicting_request, "admin-registry-b")
                .expect_err("reject conflicting manifest identity after rotation");
        assert!(matches!(
            conflict,
            BrowserReleaseRegistryError::IdentityConflict
        ));

        let mut new_activation = parse_canonical_browser_release_activation(&decode_fixture(
            &fixture.activation.canonical,
        ))
        .expect("parse new activation base");
        new_activation.activation_id = "browser-activation-after-rotation".to_string();
        new_activation.activation_generation = 2;
        new_activation.channel_sequence = 2;
        let new_activation_bytes = canonical_browser_release_json(&new_activation)
            .expect("canonical new activation after rotation");
        let new_write = import_browser_release_activation(
            &pool,
            &BrowserReleaseAuthorityEnvelope {
                canonical_base64url: base64::engine::general_purpose::URL_SAFE_NO_PAD
                    .encode(new_activation_bytes),
                signature_set_base64url: fixture.activation.signature_set,
            },
            "admin-registry-b",
        )
        .expect_err("new writes still require current-policy authority");
        assert!(matches!(
            new_write,
            BrowserReleaseRegistryError::InvalidAuthority
        ));
    }

    #[test]
    fn incident_revocation_cannot_target_root_authority_and_root_rotation_recovers() {
        let _environment_lock = ROOT_ANCHOR_ENV_LOCK.lock().expect("lock root anchor env");
        let previous = std::env::var_os(BROWSER_RELEASE_ROOT_TRUST_ANCHOR_ENV);
        let _environment_guard = RootAnchorEnvironmentGuard(previous);
        let fixture = registry_fixture();
        let policy_bytes = decode_fixture(&fixture.trust_policy.canonical);
        let policy = parse_canonical_browser_release_trust_policy(&policy_bytes)
            .expect("parse fixture policy");
        install_fixture_root_anchor(&policy);
        let pool = sqlite_registry_pool();
        let predecessor = import_browser_release_trust_policy(
            &pool,
            &BrowserReleaseAuthorityEnvelope {
                canonical_base64url: fixture.trust_policy.canonical,
                signature_set_base64url: fixture.trust_policy.signature_set,
            },
            "admin-registry-a",
        )
        .expect("import predecessor trust policy");

        let delegated = signing_key_revocation_envelope(
            "browser-revocation-delegated-signing-key-1",
            1,
            "promotion-key-1",
            2,
        );
        append_browser_release_revocation(&pool, &delegated, "admin-registry-a")
            .expect("incident authority may revoke a delegated signing key");

        let root_target = signing_key_revocation_envelope(
            "browser-revocation-root-signing-key-2",
            2,
            "root-key-1",
            10,
        );
        assert!(matches!(
            append_browser_release_revocation(&pool, &root_target, "admin-registry-a"),
            Err(BrowserReleaseRegistryError::InvalidAuthority)
        ));
        let revocation_count: i64 = pool
            .get()
            .expect("get root revocation audit connection")
            .query_row(
                "SELECT COUNT(*) FROM jobs_browser_release_revocations",
                [],
                |row| row.get(0),
            )
            .expect("count persisted revocations");
        assert_eq!(revocation_count, 1);

        let successor =
            successor_policy_envelope(&policy, &predecessor.authority_sha256, "https://bluey.sh");
        let recovered =
            import_browser_release_trust_policy(&pool, &successor, "admin-registry-rotation")
                .expect("predecessor roots remain able to authorize the successor policy");
        assert_eq!(recovered.authority_kind, "trust-policy");
        assert!(!recovered.replayed);
    }

    #[test]
    fn signing_key_revocation_subject_guard_protects_root_ids_and_material() {
        let root_public_key = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(deterministic_signing_key(10).verifying_key().to_bytes());
        let delegated_public_key = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(deterministic_signing_key(2).verifying_key().to_bytes());
        let keys = vec![
            (
                "root-key-1".to_string(),
                "root".to_string(),
                root_public_key.clone(),
            ),
            (
                "root-material-alias".to_string(),
                "promotion".to_string(),
                root_public_key.clone(),
            ),
            (
                "promotion-key-1".to_string(),
                "promotion".to_string(),
                delegated_public_key.clone(),
            ),
        ];
        let root_digest = browser_release_signing_key_digest(&root_public_key).unwrap();
        let delegated_digest = browser_release_signing_key_digest(&delegated_public_key).unwrap();
        assert!(!browser_release_delegated_signing_key_subject_matches(
            &keys,
            "root-key-1",
            &root_digest,
        )
        .unwrap());
        assert!(!browser_release_delegated_signing_key_subject_matches(
            &keys,
            "root-material-alias",
            &root_digest,
        )
        .unwrap());
        assert!(browser_release_delegated_signing_key_subject_matches(
            &keys,
            "promotion-key-1",
            &delegated_digest,
        )
        .unwrap());

        let source = include_str!("browser_release_registry.rs");
        for implementation in [
            "fn sqlite_browser_release_signing_key_subject_matches",
            "fn postgres_browser_release_signing_key_subject_matches",
        ] {
            let operation = source
                .split(implementation)
                .nth(1)
                .expect("signing-key subject implementation")
                .split("fn ")
                .next()
                .expect("bounded signing-key subject implementation");
            assert!(operation.contains("role = 'root'"));
            assert!(operation.contains("browser_release_delegated_signing_key_subject_matches"));
        }
    }

    #[test]
    fn current_artifact_origin_fences_fresh_activation_import_and_apply_but_not_replay() {
        let _environment_lock = ROOT_ANCHOR_ENV_LOCK.lock().expect("lock root anchor env");
        let previous = std::env::var_os(BROWSER_RELEASE_ROOT_TRUST_ANCHOR_ENV);
        let _environment_guard = RootAnchorEnvironmentGuard(previous);
        let fixture = registry_fixture();
        let policy_bytes = decode_fixture(&fixture.trust_policy.canonical);
        let policy = parse_canonical_browser_release_trust_policy(&policy_bytes)
            .expect("parse fixture policy");
        install_fixture_root_anchor(&policy);
        let pool = sqlite_registry_pool();
        import_browser_release_trust_policy(
            &pool,
            &BrowserReleaseAuthorityEnvelope {
                canonical_base64url: fixture.trust_policy.canonical.clone(),
                signature_set_base64url: fixture.trust_policy.signature_set.clone(),
            },
            "admin-registry-a",
        )
        .expect("import fixture trust policy");
        import_browser_release_manifest(
            &pool,
            &fixture_manifest_request(&fixture),
            "admin-registry-a",
        )
        .expect("import old-origin manifest");
        let predecessor_envelope = BrowserReleaseAuthorityEnvelope {
            canonical_base64url: fixture.activation.canonical.clone(),
            signature_set_base64url: fixture.activation.signature_set.clone(),
        };
        let predecessor =
            import_browser_release_activation(&pool, &predecessor_envelope, "admin-registry-a")
                .expect("import predecessor activation");
        let apply_request = ApplyBrowserReleaseActivationRequest {
            activation_sha256: predecessor.authority_sha256.clone(),
            expected_head_revision: 0,
            expected_transition_sha256: None,
        };
        let active = apply_browser_release_activation(&pool, &apply_request, "admin-registry-a")
            .expect("apply predecessor activation");
        assert!(active.available);

        install_synthetic_rotated_policy_with_historical_state_and_origin(
            &pool,
            "retired",
            "https://artifacts.example",
        );
        assert!(
            import_browser_release_activation(&pool, &predecessor_envelope, "admin-registry-b")
                .expect("exact activation import replay survives origin rotation")
                .replayed
        );
        let replayed = apply_browser_release_activation(&pool, &apply_request, "admin-registry-b")
            .expect("exact activation apply replay survives origin rotation");
        assert_eq!(replayed.head_revision, 1);

        let successor_envelope = successor_activation_envelope(&fixture);
        assert!(matches!(
            import_browser_release_activation(&pool, &successor_envelope, "admin-registry-b"),
            Err(BrowserReleaseRegistryError::InvalidAuthority)
        ));

        let (activation_bytes, signature_set_bytes) =
            decode_browser_release_authority_envelope(&successor_envelope)
                .expect("decode successor activation envelope");
        let activation = parse_canonical_browser_release_activation(&activation_bytes)
            .expect("parse successor activation");
        let signature_set = parse_canonical_browser_release_signature_set(&signature_set_bytes)
            .expect("parse successor activation signatures");
        let activation_sha256 = browser_release_authority_sha256(&activation_bytes);
        let signature_set_sha256 = browser_release_authority_sha256(&signature_set_bytes);
        let mut connection = pool.get().expect("get origin apply test connection");
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .expect("begin origin apply test transaction");
        insert_sqlite_browser_release_signature_set(
            &transaction,
            &signature_set,
            &signature_set_sha256,
            &successor_envelope.signature_set_base64url,
            "legacy-importer",
            now_ms(),
        )
        .expect("seed historically imported successor signatures");
        insert_sqlite_browser_release_activation(
            &transaction,
            &activation,
            &activation_sha256,
            &signature_set_sha256,
            &successor_envelope.canonical_base64url,
            "legacy-importer",
            now_ms(),
        )
        .expect("seed historically imported successor activation");
        transaction.commit().expect("commit historical activation");

        let fresh_apply = ApplyBrowserReleaseActivationRequest {
            activation_sha256,
            expected_head_revision: 1,
            expected_transition_sha256: active.transition_sha256,
        };
        assert!(matches!(
            apply_browser_release_activation(&pool, &fresh_apply, "admin-registry-b"),
            Err(BrowserReleaseRegistryError::InvalidAuthority)
        ));
        assert_eq!(
            browser_release_channel_status(&pool, "beta")
                .expect("load unchanged channel head")
                .head_revision,
            1
        );
    }

    #[test]
    fn sqlite_new_manifest_import_rejects_backdated_retired_signers() {
        let _environment_lock = ROOT_ANCHOR_ENV_LOCK.lock().expect("lock root anchor env");
        let previous = std::env::var_os(BROWSER_RELEASE_ROOT_TRUST_ANCHOR_ENV);
        let _environment_guard = RootAnchorEnvironmentGuard(previous);
        let fixture = registry_fixture();
        let policy_bytes = decode_fixture(&fixture.trust_policy.canonical);
        let policy = parse_canonical_browser_release_trust_policy(&policy_bytes)
            .expect("parse fixture policy");
        install_fixture_root_anchor(&policy);
        let pool = sqlite_registry_pool();
        import_browser_release_trust_policy(
            &pool,
            &BrowserReleaseAuthorityEnvelope {
                canonical_base64url: fixture.trust_policy.canonical.clone(),
                signature_set_base64url: fixture.trust_policy.signature_set.clone(),
            },
            "admin-registry-a",
        )
        .expect("import predecessor trust policy");
        install_synthetic_rotated_policy_with_historical_state(&pool, "retired");

        let error = import_browser_release_manifest(
            &pool,
            &fixture_manifest_request(&fixture),
            "admin-registry-b",
        )
        .expect_err("retired historical keys cannot authorize a new manifest import");
        assert!(matches!(
            error,
            BrowserReleaseRegistryError::InvalidAuthority
        ));
    }

    #[test]
    fn sqlite_exact_rollback_replay_survives_policy_rotation() {
        let _environment_lock = ROOT_ANCHOR_ENV_LOCK.lock().expect("lock root anchor env");
        let previous = std::env::var_os(BROWSER_RELEASE_ROOT_TRUST_ANCHOR_ENV);
        let _environment_guard = RootAnchorEnvironmentGuard(previous);
        let fixture = registry_fixture();
        let policy_bytes = decode_fixture(&fixture.trust_policy.canonical);
        let policy = parse_canonical_browser_release_trust_policy(&policy_bytes)
            .expect("parse fixture policy");
        install_fixture_root_anchor(&policy);
        let pool = sqlite_registry_pool();
        import_browser_release_trust_policy(
            &pool,
            &BrowserReleaseAuthorityEnvelope {
                canonical_base64url: fixture.trust_policy.canonical.clone(),
                signature_set_base64url: fixture.trust_policy.signature_set.clone(),
            },
            "admin-registry-a",
        )
        .expect("import fixture trust policy");
        let manifest = import_browser_release_manifest(
            &pool,
            &fixture_manifest_request(&fixture),
            "admin-registry-a",
        )
        .expect("import rollback fixture manifest");
        let activation = import_browser_release_activation(
            &pool,
            &BrowserReleaseAuthorityEnvelope {
                canonical_base64url: fixture.activation.canonical,
                signature_set_base64url: fixture.activation.signature_set,
            },
            "admin-registry-a",
        )
        .expect("import rollback fixture activation");
        let active = apply_browser_release_activation(
            &pool,
            &ApplyBrowserReleaseActivationRequest {
                activation_sha256: activation.authority_sha256.clone(),
                expected_head_revision: 0,
                expected_transition_sha256: None,
            },
            "admin-registry-a",
        )
        .expect("apply rollback fixture activation");
        let rollback_envelope = rollback_replay_envelope(
            active
                .activation_sha256
                .as_deref()
                .expect("rollback source activation"),
            active
                .manifest_sha256
                .as_deref()
                .expect("rollback source manifest"),
        );
        let rollback_sha256 = seed_exact_rollback_replay(
            &pool,
            &rollback_envelope,
            &manifest.signature_set_sha256,
            &activation.signature_set_sha256,
        );
        install_synthetic_rotated_policy(&pool);

        let replayed =
            apply_browser_release_rollback(&pool, &rollback_envelope, "admin-registry-b")
                .expect("replay rollback after trust rotation");
        assert_eq!(replayed.head_revision, 1);
        assert!(!replayed.available);
        assert_eq!(
            replayed.unavailability_reason.as_deref(),
            Some("stale-trust-generation")
        );
        let connection = pool.get().expect("get replayed rollback actor connection");
        let recorded_by: String = connection
            .query_row(
                "SELECT recorded_by FROM jobs_browser_release_rollbacks \
                 WHERE rollback_sha256 = ?1",
                params![rollback_sha256],
                |row| row.get(0),
            )
            .expect("load replayed rollback actor");
        assert_eq!(recorded_by, "admin-registry-a");
    }

    #[test]
    fn registry_rejects_future_authority_before_storage() {
        let _environment_lock = ROOT_ANCHOR_ENV_LOCK.lock().expect("lock root anchor env");
        let previous = std::env::var_os(BROWSER_RELEASE_ROOT_TRUST_ANCHOR_ENV);
        let _environment_guard = RootAnchorEnvironmentGuard(previous);
        let fixture = registry_fixture();
        let policy_bytes = decode_fixture(&fixture.trust_policy.canonical);
        let policy = parse_canonical_browser_release_trust_policy(&policy_bytes)
            .expect("parse fixture policy");
        install_fixture_root_anchor(&policy);
        let mut signature_set = parse_canonical_browser_release_signature_set(&decode_fixture(
            &fixture.trust_policy.signature_set,
        ))
        .expect("parse fixture signature set");
        signature_set.signed_at_ms = now_ms() + 60_000;
        let mut signature_set_bytes =
            serde_json::to_vec(&signature_set).expect("serialize future signature set");
        signature_set_bytes.push(b'\n');
        let pool = sqlite_registry_pool();
        let error = import_browser_release_trust_policy(
            &pool,
            &BrowserReleaseAuthorityEnvelope {
                canonical_base64url: fixture.trust_policy.canonical,
                signature_set_base64url: base64::engine::general_purpose::URL_SAFE_NO_PAD
                    .encode(signature_set_bytes),
            },
            "admin-registry-a",
        )
        .expect_err("reject future Browser signature set");
        assert!(matches!(
            error,
            BrowserReleaseRegistryError::InvalidAuthority
        ));
    }

    #[test]
    fn sqlite_manifest_import_rejects_incoherent_artifact_package_contracts() {
        let _environment_lock = ROOT_ANCHOR_ENV_LOCK.lock().expect("lock root anchor env");
        let previous = std::env::var_os(BROWSER_RELEASE_ROOT_TRUST_ANCHOR_ENV);
        let _environment_guard = RootAnchorEnvironmentGuard(previous);
        let fixture = registry_fixture();
        let policy_bytes = decode_fixture(&fixture.trust_policy.canonical);
        let policy = parse_canonical_browser_release_trust_policy(&policy_bytes)
            .expect("parse fixture policy");
        install_fixture_root_anchor(&policy);
        let pool = sqlite_registry_pool();
        import_browser_release_trust_policy(
            &pool,
            &BrowserReleaseAuthorityEnvelope {
                canonical_base64url: fixture.trust_policy.canonical,
                signature_set_base64url: fixture.trust_policy.signature_set,
            },
            "admin-registry-a",
        )
        .expect("import fixture trust policy");
        let manifest =
            parse_canonical_browser_release_manifest(&decode_fixture(&fixture.manifest.canonical))
                .expect("parse fixture manifest");

        let mut wrong_extension = manifest.clone();
        wrong_extension.artifacts[0].url = wrong_extension.artifacts[0]
            .url
            .replace("darwin-arm64-dmg.dmg", "darwin-arm64-dmg.zip");
        let mut split_macos_app = manifest.clone();
        split_macos_app.artifacts[1].app_content_sha256 = "f".repeat(64);
        let mut split_automation_bundle = manifest.clone();
        split_automation_bundle.artifacts[1].automation_bundle_sha256 = "e".repeat(64);
        let mut split_chromium_executable = manifest.clone();
        split_chromium_executable.artifacts[1].chromium_executable_sha256 = "d".repeat(64);
        let mut duplicate_url = manifest;
        duplicate_url.artifacts[2].url = duplicate_url.artifacts[0].url.clone();

        for (invalid, signature_set_id) in [
            (wrong_extension, "invalid-manifest-extension-signatures"),
            (split_macos_app, "invalid-manifest-app-content-signatures"),
            (
                split_automation_bundle,
                "invalid-manifest-automation-bundle-signatures",
            ),
            (
                split_chromium_executable,
                "invalid-manifest-chromium-executable-signatures",
            ),
            (duplicate_url, "invalid-manifest-duplicate-url-signatures"),
        ] {
            assert!(matches!(
                import_browser_release_manifest(
                    &pool,
                    &manifest_request_for_authority(&invalid, signature_set_id),
                    "admin-registry-a",
                ),
                Err(BrowserReleaseRegistryError::InvalidAuthority)
            ));
        }
        let stored_manifest_count: i64 = pool
            .get()
            .expect("get invalid manifest audit connection")
            .query_row(
                "SELECT COUNT(*) FROM jobs_browser_release_manifests",
                [],
                |row| row.get(0),
            )
            .expect("count stored manifests");
        assert_eq!(stored_manifest_count, 0);
    }

    #[test]
    fn postgres_mutation_lock_helper_orders_global_before_channel() {
        let source = include_str!("browser_release_registry.rs");
        let helper = source
            .split("fn lock_postgres_browser_release_mutation")
            .nth(1)
            .expect("mutation lock helper")
            .split("fn postgres_browser_release_channel_head_for_update")
            .next()
            .expect("bounded mutation lock helper");
        let global = helper
            .find("lock_postgres_browser_release_registry(transaction)")
            .expect("global registry lock");
        let channel = helper
            .find("lock_postgres_browser_release_channel(transaction, channel)")
            .expect("channel lock");
        assert!(global < channel);
        for (start, end) in [
            (
                "pub fn apply_browser_release_activation",
                "fn validate_browser_release_activation_apply_request",
            ),
            (
                "pub fn apply_browser_release_rollback",
                "fn sqlite_stored_browser_release_rollback",
            ),
            ("pub fn browser_release_channel_status", "#[cfg(test)]"),
        ] {
            let operation = source
                .split(start)
                .nth(1)
                .expect("Browser release operation")
                .split(end)
                .next()
                .expect("bounded Browser release operation");
            assert!(operation.contains("lock_postgres_browser_release_mutation"));
        }
    }

    #[test]
    fn exact_replay_identity_checks_precede_latest_policy_verification() {
        let source = include_str!("browser_release_registry.rs");
        for (start, end, sqlite_identity, postgres_identity) in [
            (
                "pub fn import_browser_release_manifest",
                "pub fn import_browser_release_activation",
                "sqlite_browser_release_manifest_identity",
                "postgres_browser_release_manifest_identity",
            ),
            (
                "pub fn import_browser_release_activation",
                "fn browser_release_manifest_import_result",
                "sqlite_browser_release_activation_identity",
                "postgres_browser_release_activation_identity",
            ),
            (
                "pub fn apply_browser_release_rollback",
                "fn sqlite_stored_browser_release_rollback",
                "sqlite_browser_release_rollback_identity",
                "postgres_browser_release_rollback_identity",
            ),
            (
                "pub fn append_browser_release_revocation",
                "fn browser_release_revocation_import_result",
                "sqlite_browser_release_revocation_identity",
                "postgres_browser_release_revocation_identity",
            ),
        ] {
            let operation = source
                .split(start)
                .nth(1)
                .expect("Browser release replay operation")
                .split(end)
                .next()
                .expect("bounded Browser release replay operation");
            let sqlite_replay = operation
                .find(sqlite_identity)
                .expect("SQLite replay identity check");
            let sqlite_latest = operation
                .find("sqlite_latest_browser_release_policy")
                .expect("SQLite latest-policy check");
            let postgres_replay = operation
                .find(postgres_identity)
                .expect("PostgreSQL replay identity check");
            let postgres_latest = operation
                .find("postgres_latest_browser_release_policy")
                .expect("PostgreSQL latest-policy check");
            assert!(sqlite_replay < sqlite_latest);
            assert!(postgres_replay < postgres_latest);
        }
    }
}
