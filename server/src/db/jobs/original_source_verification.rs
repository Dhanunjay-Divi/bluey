const ORIGINAL_SOURCE_LEASE_TTL_MS: i64 = 60_000;
const ORIGINAL_SOURCE_HARD_DEADLINE_MS: i64 = 10 * 60_000;
const ORIGINAL_SOURCE_RECEIPT_TTL_MS: i64 = 24 * 60 * 60_000;
const ORIGINAL_SOURCE_ASSIGNMENT_TTL_MS: i64 = 30 * 24 * 60 * 60_000;
const ORIGINAL_SOURCE_ATTEMPT_BUDGET: i64 = 5;
const ORIGINAL_SOURCE_CIRCUIT_FAILURE_THRESHOLD: i64 = 3;
const ORIGINAL_SOURCE_CIRCUIT_COOLDOWN_MS: i64 = 15 * 60_000;
const ORIGINAL_SOURCE_LEASE_SCAN_LIMIT: i64 = 32;
const ORIGINAL_SOURCE_HOLD_RECHECK_LIMIT: i64 = 8;
const ORIGINAL_SOURCE_HOLD_RECHECK_MS: i64 = 60_000;

#[derive(Debug, Clone)]
struct OriginalSourceLeaseCandidate {
    assignment_id: String,
    account_id: String,
    job_id: String,
    subject_sha256: String,
    canonical_subject_json: String,
    state: String,
    attempt_count: i64,
    active_attempt_id: Option<String>,
    circuit_state: String,
    assignment_expires_at_ms: i64,
    next_attempt_at_ms: i64,
    created_at_ms: i64,
}

#[derive(Debug, Clone)]
struct OriginalSourceLeaseScanCursor {
    next_attempt_at_ms: i64,
    created_at_ms: i64,
    assignment_id: String,
}

impl OriginalSourceLeaseCandidate {
    fn scan_cursor(&self) -> OriginalSourceLeaseScanCursor {
        OriginalSourceLeaseScanCursor {
            next_attempt_at_ms: self.next_attempt_at_ms,
            created_at_ms: self.created_at_ms,
            assignment_id: self.assignment_id.clone(),
        }
    }
}

fn original_source_advance_lease_scan_cursor(
    current: Option<&OriginalSourceLeaseScanCursor>,
    candidate: &OriginalSourceLeaseCandidate,
) -> OriginalSourceVerificationResult<OriginalSourceLeaseScanCursor> {
    let next = candidate.scan_cursor();
    let advances = current.is_none_or(|current| {
        next.next_attempt_at_ms > current.next_attempt_at_ms
            || (next.next_attempt_at_ms == current.next_attempt_at_ms
                && next.created_at_ms > current.created_at_ms)
            || (next.next_attempt_at_ms == current.next_attempt_at_ms
                && next.created_at_ms == current.created_at_ms
                && next.assignment_id > current.assignment_id)
    });
    if !advances {
        return Err(OriginalSourceVerificationError::Storage(
            "original-source lease scan cursor did not advance".to_string(),
        ));
    }
    Ok(next)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OriginalSourceLeaseCandidateDecision {
    Lease,
    DeferOperationalHold,
    Supersede(&'static str),
}

fn original_source_scan_lease_candidates<
    Context,
    Lease,
    Load,
    Prepare,
    Decide,
    Defer,
    Supersede,
    Claim,
>(
    context: &mut Context,
    mut load: Load,
    mut prepare: Prepare,
    mut decide: Decide,
    mut defer: Defer,
    mut supersede: Supersede,
    mut claim: Claim,
) -> OriginalSourceVerificationResult<Option<Lease>>
where
    Load: FnMut(
        &mut Context,
        Option<&OriginalSourceLeaseScanCursor>,
    ) -> OriginalSourceVerificationResult<Vec<OriginalSourceLeaseCandidate>>,
    Prepare: FnMut(
        &mut Context,
        &OriginalSourceLeaseCandidate,
    ) -> OriginalSourceVerificationResult<Option<OriginalSourceLeaseCandidate>>,
    Decide: FnMut(
        &mut Context,
        &OriginalSourceLeaseCandidate,
    ) -> OriginalSourceVerificationResult<OriginalSourceLeaseCandidateDecision>,
    Defer:
        FnMut(&mut Context, &OriginalSourceLeaseCandidate) -> OriginalSourceVerificationResult<()>,
    Supersede: FnMut(
        &mut Context,
        &OriginalSourceLeaseCandidate,
        &str,
    ) -> OriginalSourceVerificationResult<()>,
    Claim: FnMut(
        &mut Context,
        &OriginalSourceLeaseCandidate,
    ) -> OriginalSourceVerificationResult<Lease>,
{
    let mut cursor = None;
    let candidates = load(context, cursor.as_ref())?;
    for enumerated in candidates {
        cursor = Some(original_source_advance_lease_scan_cursor(
            cursor.as_ref(),
            &enumerated,
        )?);
        let Some(candidate) = prepare(context, &enumerated)? else {
            continue;
        };
        match decide(context, &candidate)? {
            OriginalSourceLeaseCandidateDecision::DeferOperationalHold => {
                defer(context, &candidate)?;
            }
            OriginalSourceLeaseCandidateDecision::Supersede(reason) => {
                supersede(context, &candidate, reason)?;
            }
            OriginalSourceLeaseCandidateDecision::Lease => {
                return claim(context, &candidate).map(Some);
            }
        }
    }
    Ok(None)
}

#[derive(Debug, Error)]
pub enum OriginalSourceVerificationError {
    #[error("managed original-source verifier authority is unavailable")]
    ManagedRuntimeAuthorityUnavailable,
    #[error("invalid original-source verification input: {0}")]
    InvalidInput(String),
    #[error("original-source verification assignment was not found")]
    AssignmentNotFound,
    #[error("original-source verification lease was lost")]
    LeaseLost,
    #[error("original-source verification lease expired")]
    LeaseExpired,
    #[error("original-source verification request conflicts with a prior replay")]
    ConflictingReplay,
    #[error("original-source verification head advanced concurrently")]
    ConcurrentHeadAdvance,
    #[error("original-source verification storage error: {0}")]
    Storage(String),
}

pub type OriginalSourceVerificationResult<T> =
    std::result::Result<T, OriginalSourceVerificationError>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct OriginalSourceVerifierBinding {
    pub worker_id: String,
    pub runtime_instance_id: String,
    pub runtime_instance_epoch: i64,
    pub runtime_authority_sha256: String,
    pub runtime_session_token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct OriginalSourceVerificationAssignment {
    pub assignment_id: String,
    pub assignment_generation: i64,
    pub assignment_sha256: String,
    pub managed_authority_sha256: String,
    pub account_id: String,
    pub job_id: String,
    pub subject_sha256: String,
    pub canonical_subject_json: String,
    pub state: String,
    pub attempt_count: i64,
    pub consecutive_failures: i64,
    pub circuit_state: String,
    pub next_attempt_at_ms: i64,
    pub not_before_at_ms: i64,
    pub expires_at_ms: i64,
    pub attempt_budget: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct OriginalSourceVerificationLease {
    pub assignment_id: String,
    pub subject_sha256: String,
    pub canonical_subject_json: String,
    pub attempt_id: String,
    pub fence: i64,
    pub lease_token: String,
    pub lease_expires_at_ms: i64,
    pub hard_deadline_at_ms: i64,
    pub heartbeat_sequence: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct OriginalSourceVerificationHeartbeatRequest {
    pub binding: OriginalSourceVerifierBinding,
    pub assignment_id: String,
    pub attempt_id: String,
    pub fence: i64,
    pub lease_token: String,
    pub heartbeat_sequence: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct OriginalSourceVerificationHeartbeat {
    pub lease_expires_at_ms: i64,
    pub hard_deadline_at_ms: i64,
    pub heartbeat_sequence: i64,
    pub replayed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct OriginalSourceVerificationObservation {
    pub assurance: String,
    pub result: String,
    pub evidence_sha256: String,
    pub error_code: Option<String>,
    pub requested_url: Option<String>,
    pub canonical_observed_url: Option<String>,
    pub canonical_application_url: Option<String>,
    pub application_domain: Option<String>,
    pub retrieval_status: String,
    pub http_status: Option<i64>,
    pub http_semantics_digest: String,
    pub redirect_chain_digest: String,
    pub headers_digest: String,
    pub content_digest: String,
    pub parser_version: String,
    pub parser_digest: String,
    pub worker_runtime_identity_sha256: String,
    #[serde(default)]
    pub provider_record_id: Option<String>,
    pub company: Option<String>,
    pub title: Option<String>,
    #[serde(default)]
    pub location: Option<String>,
    #[serde(default)]
    pub workplace: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub compensation: Option<String>,
    #[serde(default)]
    pub employment_type: Option<String>,
    #[serde(default)]
    pub posted_at_ms: Option<i64>,
    #[serde(default)]
    pub mismatched_fields: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct OriginalSourceVerificationCompletionRequest {
    pub binding: OriginalSourceVerifierBinding,
    pub assignment_id: String,
    pub attempt_id: String,
    pub fence: i64,
    pub lease_token: String,
    pub request_id: String,
    pub observation: OriginalSourceVerificationObservation,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct OriginalSourceVerificationFailureRequest {
    pub binding: OriginalSourceVerifierBinding,
    pub assignment_id: String,
    pub attempt_id: String,
    pub fence: i64,
    pub lease_token: String,
    pub request_id: String,
    pub error_code: String,
    pub observation: OriginalSourceVerificationObservation,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct OriginalSourceVerificationHead {
    #[serde(skip_serializing)]
    pub account_id: String,
    #[serde(skip_serializing)]
    pub job_id: String,
    pub head_revision: i64,
    pub material_generation: i64,
    pub assignment_id: String,
    pub receipt_id: String,
    pub receipt_sha256: String,
    pub subject_sha256: String,
    pub material_sha256: String,
    pub assurance: String,
    pub result: String,
    pub checked_at_ms: i64,
    pub expires_at_ms: i64,
    pub canonical_application_url: Option<String>,
    pub application_domain: Option<String>,
    #[serde(skip_serializing)]
    pub managed_authority_sha256: String,
    #[serde(skip_serializing)]
    pub canonical_managed_authority_json: String,
    #[serde(skip_serializing)]
    pub assignment_state: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct OriginalSourceVerificationTerminal {
    pub assignment_id: String,
    pub state: String,
    pub replayed: bool,
    pub receipt_sha256: Option<String>,
    pub head: Option<OriginalSourceVerificationHead>,
}

#[derive(Serialize)]
struct OriginalSourceSubject<'a> {
    schema_version: i64,
    canonical_job_id: &'a str,
    employer_id: Option<&'a str>,
    original_url: &'a str,
    provider_family: &'a str,
    provider_record_id: &'a str,
    provider_target: OriginalSourceProviderTarget<'a>,
    expected: OriginalSourceExpected<'a>,
}

#[derive(Serialize)]
struct OriginalSourceProviderTarget<'a> {
    host: &'a str,
    tenant: &'a str,
    job: &'a str,
    variant: &'a str,
}

struct OriginalSourceOwnedProviderTarget {
    host: String,
    tenant: String,
    job: String,
    variant: &'static str,
    provider_record_id: String,
}

#[derive(Serialize)]
struct OriginalSourceExpected<'a> {
    company: &'a str,
    title: &'a str,
    location: &'a str,
    workplace: &'a str,
    description: &'a str,
    compensation: &'a str,
    employment_type: &'a str,
    posted_at_ms: Option<i64>,
    availability_status: &'a str,
}

#[derive(Serialize)]
struct OriginalSourceAssignmentAuthority<'a> {
    assignment_generation: i64,
    assignment_id: &'a str,
    attempt_budget: i64,
    audience: &'static str,
    expires_at_ms: i64,
    managed_authority_sha256: &'a str,
    not_before_at_ms: i64,
    predecessor_assignment_sha256: Option<&'a str>,
    subject_sha256: &'a str,
    version: i64,
}

#[allow(clippy::too_many_arguments)]
fn original_source_assignment_authority(
    assignment_id: &str,
    assignment_generation: i64,
    subject_sha256: &str,
    not_before_at_ms: i64,
    expires_at_ms: i64,
    attempt_budget: i64,
    managed_authority_sha256: &str,
    predecessor_assignment_sha256: Option<&str>,
) -> OriginalSourceVerificationResult<(String, String)> {
    let canonical = serde_json::to_string(&OriginalSourceAssignmentAuthority {
        assignment_generation,
        assignment_id,
        attempt_budget,
        audience: "bluey.jobs.original_source_assignment.v1",
        expires_at_ms,
        managed_authority_sha256,
        not_before_at_ms,
        predecessor_assignment_sha256,
        subject_sha256,
        version: 1,
    })
    .map_err(original_source_storage)?;
    let sha256 = original_source_sha256(canonical.as_bytes());
    Ok((canonical, sha256))
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
struct OriginalSourceManagedAuthorityBinding {
    account_id: String,
    environment: String,
    region: String,
    channel: String,
    head_revision: i64,
    transition_sha256: String,
    activation_sha256: String,
    manifest_sha256: String,
    cohort_sha256: String,
    trust_generation: i64,
    channel_sequence: i64,
    release_id: String,
    release_sequence: i64,
    task_queue_sha256: String,
    failure_converter_sha256: String,
    activation_expires_at_ms: i64,
    source_protocol_schema_sha256: String,
    runtime_identity_sha256: String,
    dependency_evidence_sha256: String,
    heartbeat_ttl_ms: i64,
}

#[derive(Debug, Clone)]
struct OriginalSourceCurrentAuthority {
    binding: OriginalSourceManagedAuthorityBinding,
    canonical_json: String,
    sha256: String,
}

fn original_source_managed_authority_binding(
    authority: &ManagedCloudOriginalSourceVerificationAuthority,
) -> OriginalSourceVerificationResult<(OriginalSourceManagedAuthorityBinding, String, String)> {
    let binding = OriginalSourceManagedAuthorityBinding {
        account_id: authority.account_id.clone(),
        environment: authority.scope.environment.clone(),
        region: authority.scope.region.clone(),
        channel: authority.scope.channel.clone(),
        head_revision: authority.head_revision,
        transition_sha256: authority.transition_sha256.clone(),
        activation_sha256: authority.activation_sha256.clone(),
        manifest_sha256: authority.manifest_sha256.clone(),
        cohort_sha256: authority.cohort_sha256.clone(),
        trust_generation: authority.trust_generation,
        channel_sequence: authority.channel_sequence,
        release_id: authority.release_id.clone(),
        release_sequence: authority.release_sequence,
        task_queue_sha256: authority.task_queue_sha256.clone(),
        failure_converter_sha256: authority.failure_converter_sha256.clone(),
        activation_expires_at_ms: authority.activation_expires_at_ms,
        source_protocol_schema_sha256: authority.source_protocol_schema_sha256.clone(),
        runtime_identity_sha256: authority.runtime_identity_sha256.clone(),
        dependency_evidence_sha256: authority.dependency_evidence_sha256.clone(),
        heartbeat_ttl_ms: authority.heartbeat_ttl_ms,
    };
    let canonical = String::from_utf8(
        authority
            .canonical_json()
            .map_err(original_source_storage)?,
    )
    .map_err(original_source_storage)?;
    let sha256 = authority
        .authority_sha256()
        .map_err(original_source_storage)?;
    Ok((binding, canonical, sha256))
}

fn original_source_storage(error: impl std::fmt::Display) -> OriginalSourceVerificationError {
    OriginalSourceVerificationError::Storage(error.to_string())
}

fn original_source_assignment_authority_error(
    error: ManagedCloudRegistryError,
) -> OriginalSourceVerificationError {
    match error {
        ManagedCloudRegistryError::Unavailable | ManagedCloudRegistryError::Revoked => {
            OriginalSourceVerificationError::ManagedRuntimeAuthorityUnavailable
        }
        error => original_source_storage(error),
    }
}

fn original_source_sha256(bytes: impl AsRef<[u8]>) -> String {
    hex::encode(Sha256::digest(bytes.as_ref()))
}

fn original_source_valid_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .as_bytes()
            .iter()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(byte))
}

fn original_source_valid_id(value: &str) -> bool {
    (1..=128).contains(&value.len())
        && value
            .as_bytes()
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

fn original_source_provider(source: &str) -> Option<&'static str> {
    match source.trim() {
        "greenhouse_import" | "greenhouse" => Some("greenhouse"),
        "lever_import" | "lever" => Some("lever"),
        "ashby_import" | "ashby" => Some("ashby"),
        "smartrecruiters_import" | "smartrecruiters" => Some("smartrecruiters"),
        "workday_import" | "workday" => Some("workday"),
        _ => None,
    }
}

fn original_source_canonical_subject(
    posting: &JobPosting,
) -> OriginalSourceVerificationResult<(String, String)> {
    let fields = [
        posting.canonical_key.trim(),
        posting.source.trim(),
        posting.company.trim(),
        posting.title.trim(),
        posting.canonical_url.trim(),
    ];
    if fields.iter().any(|value| value.is_empty()) {
        return Err(OriginalSourceVerificationError::InvalidInput(
            "job lacks an original-source identity field".to_string(),
        ));
    }
    let provider = original_source_provider(&posting.source).ok_or_else(|| {
        OriginalSourceVerificationError::InvalidInput(
            "job source is not a supported verified import".to_string(),
        )
    })?;
    let (discovery_url, source_key) =
        canonical_public_discovery_url(provider, posting.canonical_url.trim()).map_err(|_| {
            OriginalSourceVerificationError::InvalidInput(
                "job URL is not a supported original-source provider target".to_string(),
            )
        })?;
    if discovery_url != posting.canonical_url.trim() {
        return Err(OriginalSourceVerificationError::InvalidInput(
            "job URL is not in server-canonical form".to_string(),
        ));
    }
    let external_id = posting.external_id.trim();
    if external_id.is_empty()
        || external_id.len() > 512
        || external_id.chars().any(char::is_control)
        || external_id.contains(char::is_whitespace)
    {
        return Err(OriginalSourceVerificationError::InvalidInput(
            "job provider record identity is invalid".to_string(),
        ));
    }
    let parsed_url = reqwest::Url::parse(&discovery_url).map_err(|_| {
        OriginalSourceVerificationError::InvalidInput(
            "canonical original-source URL is invalid".to_string(),
        )
    })?;
    let host = parsed_url
        .host_str()
        .unwrap_or_default()
        .to_ascii_lowercase();
    let provider_target = if matches!(provider, "greenhouse" | "lever") {
        let target = crate::jobs_ats_target::parse_provider_application_target(
            &discovery_url,
            crate::jobs_ats_target::ProviderApplicationTargetPurpose::Submit,
        )
        .ok_or_else(|| {
            OriginalSourceVerificationError::InvalidInput(
                "provider application target is ambiguous".to_string(),
            )
        })?;
        if target.job != external_id {
            return Err(OriginalSourceVerificationError::InvalidInput(
                "provider record identity contradicts the original URL".to_string(),
            ));
        }
        OriginalSourceOwnedProviderTarget {
            host: target.host,
            tenant: target.tenant,
            job: target.job,
            variant: target.variant,
            provider_record_id: target.provider_job_key,
        }
    } else {
        let target_tenant = if provider == "workday" {
            source_key.split('~').next().unwrap_or_default().to_string()
        } else {
            source_key.clone()
        };
        if target_tenant.is_empty() {
            return Err(OriginalSourceVerificationError::InvalidInput(
                "provider tenant identity is invalid".to_string(),
            ));
        }
        let url_record_id = parsed_url
            .path_segments()
            .and_then(|mut segments| segments.rfind(|segment| !segment.is_empty()))
            .ok_or_else(|| {
                OriginalSourceVerificationError::InvalidInput(
                    "provider URL has no record identity".to_string(),
                )
            })?;
        let record_matches = match provider {
            "workday" => {
                url_record_id == external_id
                    || url_record_id
                        .strip_suffix(external_id)
                        .is_some_and(|prefix| prefix.ends_with('_'))
            }
            "smartrecruiters" => {
                url_record_id == external_id
                    || url_record_id
                        .strip_prefix(external_id)
                        .is_some_and(|suffix| suffix.starts_with('-') && suffix.len() > 1)
            }
            _ => url_record_id == external_id,
        };
        if !record_matches {
            return Err(OriginalSourceVerificationError::InvalidInput(
                "provider record identity contradicts the original URL".to_string(),
            ));
        }
        OriginalSourceOwnedProviderTarget {
            host: host.clone(),
            tenant: target_tenant,
            job: external_id.to_string(),
            variant: match provider {
                "ashby" => "ashby_posting",
                "smartrecruiters" => "smartrecruiters_posting",
                "workday" => "workday_posting",
                _ => unreachable!("provider is closed above"),
            },
            provider_record_id: format!("{provider}:{host}:{source_key}:{external_id}"),
        }
    };
    let original_url = if provider == "smartrecruiters" {
        let mut stable = parsed_url.clone();
        stable.set_query(None);
        {
            let mut path = stable.path_segments_mut().map_err(|_| {
                OriginalSourceVerificationError::InvalidInput(
                    "SmartRecruiters identity URL cannot be canonicalized".to_string(),
                )
            })?;
            path.clear();
            path.extend([source_key.as_str(), external_id]);
        }
        stable.to_string().trim_end_matches('/').to_string()
    } else {
        discovery_url
    };
    let employer_id = format!(
        "original-source-employer-{}",
        &original_source_sha256(format!("{provider}:{source_key}").as_bytes())[..32]
    );
    let subject = OriginalSourceSubject {
        schema_version: 1,
        canonical_job_id: posting.canonical_key.trim(),
        employer_id: Some(&employer_id),
        original_url: &original_url,
        provider_family: provider,
        provider_record_id: &provider_target.provider_record_id,
        provider_target: OriginalSourceProviderTarget {
            host: &provider_target.host,
            tenant: &provider_target.tenant,
            job: &provider_target.job,
            variant: provider_target.variant,
        },
        expected: OriginalSourceExpected {
            company: posting.company.trim(),
            title: posting.title.trim(),
            location: posting.location.trim(),
            workplace: posting.workplace.trim(),
            description: posting.description.trim(),
            compensation: posting.compensation.trim(),
            employment_type: posting.employment_type.trim(),
            posted_at_ms: posting.posted_at_ms,
            availability_status: posting.availability_status.trim(),
        },
    };
    let canonical = serde_json::to_string(&subject).map_err(original_source_storage)?;
    if canonical.len() > 131_072 {
        return Err(OriginalSourceVerificationError::InvalidInput(
            "canonical subject is too large".to_string(),
        ));
    }
    let sha256 = original_source_sha256(canonical.as_bytes());
    Ok((canonical, sha256))
}

pub fn original_source_subject_sha256(
    posting: &JobPosting,
) -> OriginalSourceVerificationResult<String> {
    original_source_canonical_subject(posting).map(|(_, sha256)| sha256)
}

fn original_source_provider_coordinates(
    posting: &JobPosting,
) -> OriginalSourceVerificationResult<(&'static str, String)> {
    let provider = original_source_provider(&posting.source).ok_or_else(|| {
        OriginalSourceVerificationError::InvalidInput(
            "job source is not a supported verified import".to_string(),
        )
    })?;
    let (canonical_url, source_key) =
        canonical_public_discovery_url(provider, posting.canonical_url.trim()).map_err(|_| {
            OriginalSourceVerificationError::InvalidInput(
                "job URL is not a supported original-source provider target".to_string(),
            )
        })?;
    if canonical_url != posting.canonical_url.trim() {
        return Err(OriginalSourceVerificationError::InvalidInput(
            "job URL is not in server-canonical form".to_string(),
        ));
    }
    Ok((provider, source_key))
}

fn require_original_source_membership_sqlite(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    posting: &JobPosting,
) -> OriginalSourceVerificationResult<()> {
    let (provider, source_key) = original_source_provider_coordinates(posting)?;
    let trusted = tx
        .query_row(
            "SELECT 1 FROM jobs_discovery_memberships membership
               JOIN jobs_discovery_sources source
                 ON source.id=membership.source_id
                AND source.account_id=membership.account_id
              WHERE membership.account_id=?1 AND membership.job_id=?2
                AND membership.external_id=?3 AND membership.canonical_key=?4
                AND source.status='active'
                AND source.health IN ('waiting','healthy')
                AND source.provider=?5
                AND source.source_key=?6",
            params![
                account_id,
                posting.id,
                posting.external_id,
                posting.canonical_key,
                provider,
                source_key
            ],
            |_| Ok(()),
        )
        .optional()
        .map_err(original_source_storage)?
        .is_some();
    if !trusted {
        return Err(OriginalSourceVerificationError::InvalidInput(
            "job has no trusted discovery membership".to_string(),
        ));
    }
    Ok(())
}

fn require_original_source_membership_postgres(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    posting: &JobPosting,
) -> OriginalSourceVerificationResult<()> {
    let (provider, source_key) = original_source_provider_coordinates(posting)?;
    let trusted = tx
        .query_opt(
            "SELECT 1 FROM jobs_discovery_memberships membership
               JOIN jobs_discovery_sources source
                 ON source.id=membership.source_id
                AND source.account_id=membership.account_id
              WHERE membership.account_id=$1 AND membership.job_id=$2
                AND membership.external_id=$3 AND membership.canonical_key=$4
                AND source.status='active'
                AND source.health IN ('waiting','healthy')
                AND source.provider=$5
                AND source.source_key=$6 FOR SHARE OF membership,source",
            &[
                &account_id,
                &posting.id,
                &posting.external_id,
                &posting.canonical_key,
                &provider,
                &source_key,
            ],
        )
        .map_err(original_source_storage)?
        .is_some();
    if !trusted {
        return Err(OriginalSourceVerificationError::InvalidInput(
            "job has no trusted discovery membership".to_string(),
        ));
    }
    Ok(())
}

fn recheck_original_source_membership_sqlite(
    tx: &rusqlite::Transaction<'_>,
    lease: &OriginalSourceLeaseContext,
) -> OriginalSourceVerificationResult<JobPosting> {
    recheck_original_source_subject_sqlite(
        tx,
        &lease.account_id,
        &lease.job_id,
        &lease.subject_sha256,
    )
}

fn recheck_original_source_subject_sqlite(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    job_id: &str,
    subject_sha256: &str,
) -> OriginalSourceVerificationResult<JobPosting> {
    let posting_json: String = tx
        .query_row(
            "SELECT posting_json FROM jobs_postings WHERE account_id=?1 AND id=?2",
            params![account_id, job_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(original_source_storage)?
        .ok_or(OriginalSourceVerificationError::AssignmentNotFound)?;
    let posting: JobPosting =
        serde_json::from_str(&posting_json).map_err(original_source_storage)?;
    require_original_source_membership_sqlite(tx, account_id, &posting)?;
    if original_source_subject_sha256(&posting)? != subject_sha256 {
        return Err(OriginalSourceVerificationError::LeaseLost);
    }
    Ok(posting)
}

fn recheck_original_source_membership_postgres(
    tx: &mut postgres::Transaction<'_>,
    lease: &OriginalSourceLeaseContext,
) -> OriginalSourceVerificationResult<JobPosting> {
    recheck_original_source_subject_postgres(
        tx,
        &lease.account_id,
        &lease.job_id,
        &lease.subject_sha256,
    )
}

fn recheck_original_source_subject_postgres(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    job_id: &str,
    subject_sha256: &str,
) -> OriginalSourceVerificationResult<JobPosting> {
    let row = tx
        .query_opt(
            "SELECT posting_json FROM jobs_postings
              WHERE account_id=$1 AND id=$2 FOR SHARE",
            &[&account_id, &job_id],
        )
        .map_err(original_source_storage)?
        .ok_or(OriginalSourceVerificationError::AssignmentNotFound)?;
    let posting_json: String = row.get(0);
    let posting: JobPosting =
        serde_json::from_str(&posting_json).map_err(original_source_storage)?;
    require_original_source_membership_postgres(tx, account_id, &posting)?;
    if original_source_subject_sha256(&posting)? != subject_sha256 {
        return Err(OriginalSourceVerificationError::LeaseLost);
    }
    Ok(posting)
}

fn original_source_db_now_sqlite(
    tx: &rusqlite::Transaction<'_>,
) -> OriginalSourceVerificationResult<i64> {
    tx.query_row(
        "SELECT CAST((julianday('now') - 2440587.5) * 86400000 AS INTEGER)",
        [],
        |row| row.get(0),
    )
    .map_err(original_source_storage)
}

fn original_source_db_now_postgres(
    tx: &mut postgres::Transaction<'_>,
) -> OriginalSourceVerificationResult<i64> {
    tx.query_one(
        "SELECT floor(extract(epoch FROM clock_timestamp()) * 1000)::bigint",
        &[],
    )
    .map(|row| row.get(0))
    .map_err(original_source_storage)
}

fn original_source_require_assignment_authority_sqlite(
    tx: &rusqlite::Transaction<'_>,
    assignment_id: &str,
    account_id: &str,
) -> OriginalSourceVerificationResult<OriginalSourceCurrentAuthority> {
    let authority = sqlite_original_source_verification_authority_for_account_tx(tx, account_id)
        .map_err(original_source_assignment_authority_error)?
        .ok_or(OriginalSourceVerificationError::ManagedRuntimeAuthorityUnavailable)?;
    let (binding, canonical, sha256) = original_source_managed_authority_binding(&authority)?;
    let matches = tx
        .query_row(
            "SELECT 1 FROM jobs_original_source_verification_assignments
              WHERE assignment_id=?1 AND account_id=?2
                AND managed_authority_sha256=?3
                AND canonical_managed_authority_json=?4
                AND managed_environment=?5 AND managed_region=?6
                AND managed_channel=?7 AND managed_head_revision=?8
                AND managed_transition_sha256=?9
                AND managed_activation_sha256=?10 AND managed_manifest_sha256=?11
                AND managed_cohort_sha256=?12 AND managed_trust_generation=?13
                AND managed_channel_sequence=?14 AND managed_release_id=?15
                AND managed_release_sequence=?16 AND managed_task_queue_sha256=?17
                AND managed_failure_converter_sha256=?18
                AND managed_activation_expires_at_ms=?19
                AND managed_source_protocol_schema_sha256=?20
                AND managed_runtime_identity_sha256=?21
                AND managed_dependency_evidence_sha256=?22
                AND managed_heartbeat_ttl_ms=?23",
            params![
                assignment_id,
                account_id,
                sha256,
                canonical,
                binding.environment,
                binding.region,
                binding.channel,
                binding.head_revision,
                binding.transition_sha256,
                binding.activation_sha256,
                binding.manifest_sha256,
                binding.cohort_sha256,
                binding.trust_generation,
                binding.channel_sequence,
                binding.release_id,
                binding.release_sequence,
                binding.task_queue_sha256,
                binding.failure_converter_sha256,
                binding.activation_expires_at_ms,
                binding.source_protocol_schema_sha256,
                binding.runtime_identity_sha256,
                binding.dependency_evidence_sha256,
                binding.heartbeat_ttl_ms,
            ],
            |_| Ok(()),
        )
        .optional()
        .map_err(original_source_storage)?
        .is_some();
    if !matches {
        return Err(OriginalSourceVerificationError::ManagedRuntimeAuthorityUnavailable);
    }
    Ok(OriginalSourceCurrentAuthority {
        binding,
        canonical_json: canonical,
        sha256,
    })
}

fn original_source_require_assignment_authority_postgres(
    tx: &mut postgres::Transaction<'_>,
    assignment_id: &str,
    account_id: &str,
) -> OriginalSourceVerificationResult<OriginalSourceCurrentAuthority> {
    let authority = postgres_original_source_verification_authority_for_account_tx(tx, account_id)
        .map_err(original_source_assignment_authority_error)?
        .ok_or(OriginalSourceVerificationError::ManagedRuntimeAuthorityUnavailable)?;
    let (binding, canonical, sha256) = original_source_managed_authority_binding(&authority)?;
    let matches = tx
        .query_opt(
            "SELECT 1 FROM jobs_original_source_verification_assignments
              WHERE assignment_id=$1 AND account_id=$2
                AND managed_authority_sha256=$3
                AND canonical_managed_authority_json=$4
                AND managed_environment=$5 AND managed_region=$6
                AND managed_channel=$7 AND managed_head_revision=$8
                AND managed_transition_sha256=$9
                AND managed_activation_sha256=$10 AND managed_manifest_sha256=$11
                AND managed_cohort_sha256=$12 AND managed_trust_generation=$13
                AND managed_channel_sequence=$14 AND managed_release_id=$15
                AND managed_release_sequence=$16 AND managed_task_queue_sha256=$17
                AND managed_failure_converter_sha256=$18
                AND managed_activation_expires_at_ms=$19
                AND managed_source_protocol_schema_sha256=$20
                AND managed_runtime_identity_sha256=$21
                AND managed_dependency_evidence_sha256=$22
                AND managed_heartbeat_ttl_ms=$23 FOR SHARE",
            &[
                &assignment_id,
                &account_id,
                &sha256,
                &canonical,
                &binding.environment,
                &binding.region,
                &binding.channel,
                &binding.head_revision,
                &binding.transition_sha256,
                &binding.activation_sha256,
                &binding.manifest_sha256,
                &binding.cohort_sha256,
                &binding.trust_generation,
                &binding.channel_sequence,
                &binding.release_id,
                &binding.release_sequence,
                &binding.task_queue_sha256,
                &binding.failure_converter_sha256,
                &binding.activation_expires_at_ms,
                &binding.source_protocol_schema_sha256,
                &binding.runtime_identity_sha256,
                &binding.dependency_evidence_sha256,
                &binding.heartbeat_ttl_ms,
            ],
        )
        .map_err(original_source_storage)?
        .is_some();
    if !matches {
        return Err(OriginalSourceVerificationError::ManagedRuntimeAuthorityUnavailable);
    }
    Ok(OriginalSourceCurrentAuthority {
        binding,
        canonical_json: canonical,
        sha256,
    })
}

fn original_source_require_runtime_binding_sqlite(
    tx: &rusqlite::Transaction<'_>,
    binding: &OriginalSourceVerifierBinding,
    authority: &OriginalSourceManagedAuthorityBinding,
) -> OriginalSourceVerificationResult<String> {
    tx.query_row(
        "SELECT grant_id
           FROM jobs_managed_cloud_original_source_verifier_runtime_instances
          WHERE runtime_instance_id=?1 AND instance_epoch=?2 AND worker_id=?3
            AND runtime_identity_sha256=?4 AND environment=?5 AND region=?6
            AND channel=?7 AND activation_sha256=?8 AND manifest_sha256=?9
            AND head_revision=?10 AND transition_sha256=?11
            AND task_queue_sha256=?12 AND failure_converter_sha256=?13
            AND dependency_evidence_sha256=?14",
        params![
            binding.runtime_instance_id,
            binding.runtime_instance_epoch,
            binding.worker_id,
            authority.runtime_identity_sha256,
            authority.environment,
            authority.region,
            authority.channel,
            authority.activation_sha256,
            authority.manifest_sha256,
            authority.head_revision,
            authority.transition_sha256,
            authority.task_queue_sha256,
            authority.failure_converter_sha256,
            authority.dependency_evidence_sha256,
        ],
        |row| row.get(0),
    )
    .optional()
    .map_err(original_source_storage)?
    .ok_or(OriginalSourceVerificationError::ManagedRuntimeAuthorityUnavailable)
}

fn original_source_require_runtime_binding_postgres(
    tx: &mut postgres::Transaction<'_>,
    binding: &OriginalSourceVerifierBinding,
    authority: &OriginalSourceManagedAuthorityBinding,
) -> OriginalSourceVerificationResult<String> {
    tx.query_opt(
        "SELECT grant_id
           FROM jobs_managed_cloud_original_source_verifier_runtime_instances
          WHERE runtime_instance_id=$1 AND instance_epoch=$2 AND worker_id=$3
            AND runtime_identity_sha256=$4 AND environment=$5 AND region=$6
            AND channel=$7 AND activation_sha256=$8 AND manifest_sha256=$9
            AND head_revision=$10 AND transition_sha256=$11
            AND task_queue_sha256=$12 AND failure_converter_sha256=$13
            AND dependency_evidence_sha256=$14 FOR SHARE",
        &[
            &binding.runtime_instance_id,
            &binding.runtime_instance_epoch,
            &binding.worker_id,
            &authority.runtime_identity_sha256,
            &authority.environment,
            &authority.region,
            &authority.channel,
            &authority.activation_sha256,
            &authority.manifest_sha256,
            &authority.head_revision,
            &authority.transition_sha256,
            &authority.task_queue_sha256,
            &authority.failure_converter_sha256,
            &authority.dependency_evidence_sha256,
        ],
    )
    .map_err(original_source_storage)?
    .map(|row| row.get(0))
    .ok_or(OriginalSourceVerificationError::ManagedRuntimeAuthorityUnavailable)
}

fn original_source_assignment_from_sqlite(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<OriginalSourceVerificationAssignment> {
    Ok(OriginalSourceVerificationAssignment {
        assignment_id: row.get(0)?,
        assignment_generation: row.get(1)?,
        assignment_sha256: row.get(2)?,
        managed_authority_sha256: row.get(3)?,
        account_id: row.get(4)?,
        job_id: row.get(5)?,
        subject_sha256: row.get(6)?,
        canonical_subject_json: row.get(7)?,
        state: row.get(8)?,
        attempt_count: row.get(9)?,
        consecutive_failures: row.get(10)?,
        circuit_state: row.get(11)?,
        next_attempt_at_ms: row.get(12)?,
        not_before_at_ms: row.get(13)?,
        expires_at_ms: row.get(14)?,
        attempt_budget: row.get(15)?,
    })
}

fn original_source_assignment_from_postgres(
    row: &postgres::Row,
) -> OriginalSourceVerificationAssignment {
    OriginalSourceVerificationAssignment {
        assignment_id: row.get(0),
        assignment_generation: row.get(1),
        assignment_sha256: row.get(2),
        managed_authority_sha256: row.get(3),
        account_id: row.get(4),
        job_id: row.get(5),
        subject_sha256: row.get(6),
        canonical_subject_json: row.get(7),
        state: row.get(8),
        attempt_count: row.get(9),
        consecutive_failures: row.get(10),
        circuit_state: row.get(11),
        next_attempt_at_ms: row.get(12),
        not_before_at_ms: row.get(13),
        expires_at_ms: row.get(14),
        attempt_budget: row.get(15),
    }
}

const ORIGINAL_SOURCE_ASSIGNMENT_COLUMNS: &str =
    "assignment_id, assignment_generation, assignment_sha256, managed_authority_sha256,
     account_id, job_id,
     subject_sha256, canonical_subject_json, state, attempt_count,
     consecutive_failures, circuit_state, next_attempt_at_ms, not_before_at_ms,
     expires_at_ms, attempt_budget";

pub(crate) fn ensure_original_source_verification_assignment_sqlite_with_authority_tx(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    posting: &JobPosting,
    authority: &ManagedCloudOriginalSourceVerificationAuthority,
) -> OriginalSourceVerificationResult<OriginalSourceVerificationAssignment> {
    if authority.account_id != account_id {
        return Err(OriginalSourceVerificationError::ManagedRuntimeAuthorityUnavailable);
    }
    let (managed_authority, canonical_managed_authority_json, managed_authority_sha256) =
        original_source_managed_authority_binding(authority)?;
    ensure_original_source_verification_assignment_sqlite_bound_tx(
        tx,
        account_id,
        posting,
        &managed_authority,
        &canonical_managed_authority_json,
        &managed_authority_sha256,
    )
}

fn ensure_original_source_verification_assignment_sqlite_bound_tx(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    posting: &JobPosting,
    managed_authority: &OriginalSourceManagedAuthorityBinding,
    canonical_managed_authority_json: &str,
    managed_authority_sha256: &str,
) -> OriginalSourceVerificationResult<OriginalSourceVerificationAssignment> {
    if !original_source_valid_id(account_id) || !original_source_valid_id(&posting.id) {
        return Err(OriginalSourceVerificationError::InvalidInput(
            "account or posting identity is invalid".to_string(),
        ));
    }
    require_original_source_membership_sqlite(tx, account_id, posting)?;
    let (canonical_subject_json, subject_sha256) = original_source_canonical_subject(posting)?;
    let now = original_source_db_now_sqlite(tx)?;
    let current = tx
        .query_row(
            &format!(
                "SELECT {ORIGINAL_SOURCE_ASSIGNMENT_COLUMNS}
                   FROM jobs_original_source_verification_assignments
                  WHERE account_id = ?1 AND job_id = ?2
                    AND state NOT IN ('superseded', 'cancelled')"
            ),
            params![account_id, posting.id],
            original_source_assignment_from_sqlite,
        )
        .optional()
        .map_err(original_source_storage)?;
    if let Some(existing) = current.as_ref() {
        if existing.subject_sha256 == subject_sha256
            && existing.managed_authority_sha256 == managed_authority_sha256
            && now >= existing.not_before_at_ms
            && now < existing.expires_at_ms
            && existing.attempt_count < existing.attempt_budget
        {
            return Ok(existing.clone());
        }
    }
    if let Some(old) = current.as_ref() {
        let reason = if old.subject_sha256 != subject_sha256 {
            "subject_changed"
        } else if old.managed_authority_sha256 != managed_authority_sha256 {
            "managed_authority_changed"
        } else if now >= old.expires_at_ms {
            "assignment_expired"
        } else {
            "attempt_budget_exhausted"
        };
        original_source_append_event_sqlite(
            tx,
            &old.assignment_id,
            None,
            "superseded",
            None,
            None,
            None,
            None,
            None,
            Some(reason),
            now,
        )?;
        tx.execute(
            "UPDATE jobs_original_source_verification_assignments SET
                state='superseded', active_attempt_id=NULL, lease_owner=NULL,
                lease_token_sha256=NULL, lease_expires_at_ms=NULL,
                hard_deadline_at_ms=NULL, heartbeat_sequence=0, updated_at_ms=?2
              WHERE assignment_id=?1",
            params![old.assignment_id, now],
        )
        .map_err(original_source_storage)?;
    }
    let predecessor = tx
        .query_row(
            "SELECT assignment_generation, assignment_sha256
               FROM jobs_original_source_verification_assignments
              WHERE account_id=?1 AND job_id=?2
              ORDER BY assignment_generation DESC LIMIT 1",
            params![account_id, posting.id],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()
        .map_err(original_source_storage)?;
    if predecessor
        .as_ref()
        .is_some_and(|value| value.0 >= 9_007_199_254_740_991)
    {
        return Err(OriginalSourceVerificationError::Storage(
            "original-source assignment generation is exhausted".to_string(),
        ));
    }
    let assignment_generation = predecessor.as_ref().map_or(1, |value| value.0 + 1);
    let assignment_id = uuid::Uuid::new_v4().to_string();
    let not_before_at_ms = now;
    let expires_at_ms = now
        .saturating_add(ORIGINAL_SOURCE_ASSIGNMENT_TTL_MS)
        .min(managed_authority.activation_expires_at_ms);
    if expires_at_ms <= now {
        return Err(OriginalSourceVerificationError::ManagedRuntimeAuthorityUnavailable);
    }
    let predecessor_assignment_sha256 = predecessor.as_ref().map(|value| value.1.as_str());
    let (canonical_assignment_json, assignment_sha256) = original_source_assignment_authority(
        &assignment_id,
        assignment_generation,
        &subject_sha256,
        not_before_at_ms,
        expires_at_ms,
        ORIGINAL_SOURCE_ATTEMPT_BUDGET,
        managed_authority_sha256,
        predecessor_assignment_sha256,
    )?;
    tx.execute(
        "INSERT INTO jobs_original_source_verification_assignments (
            assignment_id,assignment_generation,assignment_sha256,
            predecessor_assignment_sha256,canonical_assignment_json,
            managed_authority_sha256,canonical_managed_authority_json,
            managed_environment,managed_region,managed_channel,
            managed_head_revision,managed_transition_sha256,
            managed_activation_sha256,managed_manifest_sha256,managed_cohort_sha256,
            managed_trust_generation,managed_channel_sequence,managed_release_id,
            managed_release_sequence,managed_task_queue_sha256,
            managed_failure_converter_sha256,managed_activation_expires_at_ms,
            managed_source_protocol_schema_sha256,managed_runtime_identity_sha256,
            managed_dependency_evidence_sha256,managed_heartbeat_ttl_ms,
            account_id,job_id,subject_sha256,canonical_subject_json,state,
            next_attempt_at_ms,not_before_at_ms,expires_at_ms,attempt_budget,
            created_at_ms,updated_at_ms
         ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,
                   ?15,?16,?17,?18,?19,?20,?21,?22,?23,?24,?25,?26,
                   ?27,?28,?29,?30,'pending',?31,?31,?32,?33,?31,?31)",
        params![
            assignment_id,
            assignment_generation,
            assignment_sha256,
            predecessor_assignment_sha256,
            canonical_assignment_json,
            managed_authority_sha256,
            canonical_managed_authority_json,
            managed_authority.environment,
            managed_authority.region,
            managed_authority.channel,
            managed_authority.head_revision,
            managed_authority.transition_sha256,
            managed_authority.activation_sha256,
            managed_authority.manifest_sha256,
            managed_authority.cohort_sha256,
            managed_authority.trust_generation,
            managed_authority.channel_sequence,
            managed_authority.release_id,
            managed_authority.release_sequence,
            managed_authority.task_queue_sha256,
            managed_authority.failure_converter_sha256,
            managed_authority.activation_expires_at_ms,
            managed_authority.source_protocol_schema_sha256,
            managed_authority.runtime_identity_sha256,
            managed_authority.dependency_evidence_sha256,
            managed_authority.heartbeat_ttl_ms,
            account_id,
            posting.id,
            subject_sha256,
            canonical_subject_json,
            now,
            expires_at_ms,
            ORIGINAL_SOURCE_ATTEMPT_BUDGET
        ],
    )
    .map_err(original_source_storage)?;
    original_source_append_event_sqlite(
        tx,
        &assignment_id,
        None,
        "scheduled",
        None,
        None,
        None,
        None,
        None,
        None,
        now,
    )?;
    tx.query_row(
        &format!(
            "SELECT {ORIGINAL_SOURCE_ASSIGNMENT_COLUMNS}
               FROM jobs_original_source_verification_assignments
              WHERE assignment_id=?1"
        ),
        params![assignment_id],
        original_source_assignment_from_sqlite,
    )
    .map_err(original_source_storage)
}

pub(crate) fn ensure_original_source_verification_assignment_postgres_with_authority_tx(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    posting: &JobPosting,
    authority: &ManagedCloudOriginalSourceVerificationAuthority,
) -> OriginalSourceVerificationResult<OriginalSourceVerificationAssignment> {
    if authority.account_id != account_id {
        return Err(OriginalSourceVerificationError::ManagedRuntimeAuthorityUnavailable);
    }
    let (managed_authority, canonical_managed_authority_json, managed_authority_sha256) =
        original_source_managed_authority_binding(authority)?;
    ensure_original_source_verification_assignment_postgres_bound_tx(
        tx,
        account_id,
        posting,
        &managed_authority,
        &canonical_managed_authority_json,
        &managed_authority_sha256,
    )
}

fn ensure_original_source_verification_assignment_postgres_bound_tx(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    posting: &JobPosting,
    managed_authority: &OriginalSourceManagedAuthorityBinding,
    canonical_managed_authority_json: &str,
    managed_authority_sha256: &str,
) -> OriginalSourceVerificationResult<OriginalSourceVerificationAssignment> {
    if !original_source_valid_id(account_id) || !original_source_valid_id(&posting.id) {
        return Err(OriginalSourceVerificationError::InvalidInput(
            "account or posting identity is invalid".to_string(),
        ));
    }
    lock_discovery_account_postgres(tx, account_id).map_err(original_source_storage)?;
    require_original_source_membership_postgres(tx, account_id, posting)?;
    let (canonical_subject_json, subject_sha256) = original_source_canonical_subject(posting)?;
    let now = original_source_db_now_postgres(tx)?;
    let current = tx
        .query_opt(
            &format!(
                "SELECT {ORIGINAL_SOURCE_ASSIGNMENT_COLUMNS}
                   FROM jobs_original_source_verification_assignments
                  WHERE account_id=$1 AND job_id=$2
                    AND state NOT IN ('superseded','cancelled') FOR UPDATE"
            ),
            &[&account_id, &posting.id],
        )
        .map_err(original_source_storage)?
        .map(|row| original_source_assignment_from_postgres(&row));
    if let Some(existing) = current.as_ref() {
        if existing.subject_sha256 == subject_sha256
            && existing.managed_authority_sha256 == managed_authority_sha256
            && now >= existing.not_before_at_ms
            && now < existing.expires_at_ms
            && existing.attempt_count < existing.attempt_budget
        {
            return Ok(existing.clone());
        }
    }
    if let Some(old) = current.as_ref() {
        let reason = if old.subject_sha256 != subject_sha256 {
            "subject_changed"
        } else if old.managed_authority_sha256 != managed_authority_sha256 {
            "managed_authority_changed"
        } else if now >= old.expires_at_ms {
            "assignment_expired"
        } else {
            "attempt_budget_exhausted"
        };
        original_source_append_event_postgres(
            tx,
            &old.assignment_id,
            None,
            "superseded",
            None,
            None,
            None,
            None,
            None,
            Some(reason),
            now,
        )?;
        tx.execute(
            "UPDATE jobs_original_source_verification_assignments SET
                state='superseded',active_attempt_id=NULL,lease_owner=NULL,
                lease_token_sha256=NULL,lease_expires_at_ms=NULL,
                hard_deadline_at_ms=NULL,heartbeat_sequence=0,updated_at_ms=$2
              WHERE assignment_id=$1",
            &[&old.assignment_id, &now],
        )
        .map_err(original_source_storage)?;
    }
    let predecessor = tx
        .query_opt(
            "SELECT assignment_generation,assignment_sha256
               FROM jobs_original_source_verification_assignments
              WHERE account_id=$1 AND job_id=$2
              ORDER BY assignment_generation DESC LIMIT 1 FOR UPDATE",
            &[&account_id, &posting.id],
        )
        .map_err(original_source_storage)?
        .map(|row| (row.get::<_, i64>(0), row.get::<_, String>(1)));
    if predecessor
        .as_ref()
        .is_some_and(|value| value.0 >= 9_007_199_254_740_991)
    {
        return Err(OriginalSourceVerificationError::Storage(
            "original-source assignment generation is exhausted".to_string(),
        ));
    }
    let assignment_generation = predecessor.as_ref().map_or(1, |value| value.0 + 1);
    let assignment_id = uuid::Uuid::new_v4().to_string();
    let not_before_at_ms = now;
    let expires_at_ms = now
        .saturating_add(ORIGINAL_SOURCE_ASSIGNMENT_TTL_MS)
        .min(managed_authority.activation_expires_at_ms);
    if expires_at_ms <= now {
        return Err(OriginalSourceVerificationError::ManagedRuntimeAuthorityUnavailable);
    }
    let predecessor_assignment_sha256 = predecessor.as_ref().map(|value| value.1.as_str());
    let (canonical_assignment_json, assignment_sha256) = original_source_assignment_authority(
        &assignment_id,
        assignment_generation,
        &subject_sha256,
        not_before_at_ms,
        expires_at_ms,
        ORIGINAL_SOURCE_ATTEMPT_BUDGET,
        managed_authority_sha256,
        predecessor_assignment_sha256,
    )?;
    tx.execute(
        "INSERT INTO jobs_original_source_verification_assignments (
            assignment_id,assignment_generation,assignment_sha256,
            predecessor_assignment_sha256,canonical_assignment_json,
            managed_authority_sha256,canonical_managed_authority_json,
            managed_environment,managed_region,managed_channel,
            managed_head_revision,managed_transition_sha256,
            managed_activation_sha256,managed_manifest_sha256,managed_cohort_sha256,
            managed_trust_generation,managed_channel_sequence,managed_release_id,
            managed_release_sequence,managed_task_queue_sha256,
            managed_failure_converter_sha256,managed_activation_expires_at_ms,
            managed_source_protocol_schema_sha256,managed_runtime_identity_sha256,
            managed_dependency_evidence_sha256,managed_heartbeat_ttl_ms,
            account_id,job_id,subject_sha256,canonical_subject_json,state,
            next_attempt_at_ms,not_before_at_ms,expires_at_ms,attempt_budget,
            created_at_ms,updated_at_ms
         ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,
                   $15,$16,$17,$18,$19,$20,$21,$22,$23,$24,$25,$26,
                   $27,$28,$29,$30,'pending',$31,$31,$32,$33,$31,$31)",
        &[
            &assignment_id,
            &assignment_generation,
            &assignment_sha256,
            &predecessor_assignment_sha256,
            &canonical_assignment_json,
            &managed_authority_sha256,
            &canonical_managed_authority_json,
            &managed_authority.environment,
            &managed_authority.region,
            &managed_authority.channel,
            &managed_authority.head_revision,
            &managed_authority.transition_sha256,
            &managed_authority.activation_sha256,
            &managed_authority.manifest_sha256,
            &managed_authority.cohort_sha256,
            &managed_authority.trust_generation,
            &managed_authority.channel_sequence,
            &managed_authority.release_id,
            &managed_authority.release_sequence,
            &managed_authority.task_queue_sha256,
            &managed_authority.failure_converter_sha256,
            &managed_authority.activation_expires_at_ms,
            &managed_authority.source_protocol_schema_sha256,
            &managed_authority.runtime_identity_sha256,
            &managed_authority.dependency_evidence_sha256,
            &managed_authority.heartbeat_ttl_ms,
            &account_id,
            &posting.id,
            &subject_sha256,
            &canonical_subject_json,
            &now,
            &expires_at_ms,
            &ORIGINAL_SOURCE_ATTEMPT_BUDGET,
        ],
    )
    .map_err(original_source_storage)?;
    original_source_append_event_postgres(
        tx,
        &assignment_id,
        None,
        "scheduled",
        None,
        None,
        None,
        None,
        None,
        None,
        now,
    )?;
    tx.query_one(
        &format!(
            "SELECT {ORIGINAL_SOURCE_ASSIGNMENT_COLUMNS}
               FROM jobs_original_source_verification_assignments
              WHERE assignment_id=$1"
        ),
        &[&assignment_id],
    )
    .map(|row| original_source_assignment_from_postgres(&row))
    .map_err(original_source_storage)
}

#[allow(clippy::too_many_arguments)]
fn original_source_event_payload(
    assignment_id: &str,
    attempt_id: Option<&str>,
    event_sequence: i64,
    predecessor_event_sha256: Option<&str>,
    event_kind: &str,
    heartbeat_sequence: Option<i64>,
    lease_expires_at_ms: Option<i64>,
    completion_request_id: Option<&str>,
    completion_request_sha256: Option<&str>,
    receipt_sha256: Option<&str>,
    reason_code: Option<&str>,
    occurred_at_ms: i64,
) -> OriginalSourceVerificationResult<(String, String, String)> {
    let event_id = uuid::Uuid::new_v4().to_string();
    let canonical = serde_json::to_string(&json!({
        "schemaVersion": 1,
        "eventId": event_id,
        "assignmentId": assignment_id,
        "attemptId": attempt_id,
        "eventSequence": event_sequence,
        "predecessorEventSha256": predecessor_event_sha256,
        "eventKind": event_kind,
        "heartbeatSequence": heartbeat_sequence,
        "leaseExpiresAtMs": lease_expires_at_ms,
        "completionRequestId": completion_request_id,
        "completionRequestSha256": completion_request_sha256,
        "receiptSha256": receipt_sha256,
        "reasonCode": reason_code,
        "occurredAtMs": occurred_at_ms,
    }))
    .map_err(original_source_storage)?;
    let sha256 = original_source_sha256(canonical.as_bytes());
    Ok((event_id, canonical, sha256))
}

#[allow(clippy::too_many_arguments)]
fn original_source_append_event_sqlite(
    tx: &rusqlite::Transaction<'_>,
    assignment_id: &str,
    attempt_id: Option<&str>,
    event_kind: &str,
    heartbeat_sequence: Option<i64>,
    lease_expires_at_ms: Option<i64>,
    completion_request_id: Option<&str>,
    completion_request_sha256: Option<&str>,
    receipt_sha256: Option<&str>,
    reason_code: Option<&str>,
    occurred_at_ms: i64,
) -> OriginalSourceVerificationResult<String> {
    let (sequence, predecessor) = tx
        .query_row(
            "SELECT current_event_sequence + 1, current_event_sha256
               FROM jobs_original_source_verification_assignments
              WHERE assignment_id = ?1",
            params![assignment_id],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, Option<String>>(1)?)),
        )
        .optional()
        .map_err(original_source_storage)?
        .ok_or(OriginalSourceVerificationError::AssignmentNotFound)?;
    let (event_id, canonical, event_sha256) = original_source_event_payload(
        assignment_id,
        attempt_id,
        sequence,
        predecessor.as_deref(),
        event_kind,
        heartbeat_sequence,
        lease_expires_at_ms,
        completion_request_id,
        completion_request_sha256,
        receipt_sha256,
        reason_code,
        occurred_at_ms,
    )?;
    tx.execute(
        "INSERT INTO jobs_original_source_verification_events (
            event_id, assignment_id, attempt_id, event_sequence,
            predecessor_event_sha256, event_kind, heartbeat_sequence,
            lease_expires_at_ms, completion_request_id,
            completion_request_sha256, receipt_sha256, reason_code,
            canonical_event_json, event_sha256, occurred_at_ms
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10,
                   ?11, ?12, ?13, ?14, ?15)",
        params![
            event_id,
            assignment_id,
            attempt_id,
            sequence,
            predecessor,
            event_kind,
            heartbeat_sequence,
            lease_expires_at_ms,
            completion_request_id,
            completion_request_sha256,
            receipt_sha256,
            reason_code,
            canonical,
            event_sha256,
            occurred_at_ms
        ],
    )
    .map_err(original_source_storage)?;
    let changed = tx
        .execute(
            "UPDATE jobs_original_source_verification_assignments
                SET current_event_sequence = ?2, current_event_sha256 = ?3,
                    updated_at_ms = MAX(updated_at_ms, ?4)
              WHERE assignment_id = ?1 AND current_event_sequence = ?2 - 1",
            params![assignment_id, sequence, event_sha256, occurred_at_ms],
        )
        .map_err(original_source_storage)?;
    if changed != 1 {
        return Err(OriginalSourceVerificationError::ConcurrentHeadAdvance);
    }
    Ok(event_sha256)
}

#[allow(clippy::too_many_arguments)]
fn original_source_append_event_postgres(
    tx: &mut postgres::Transaction<'_>,
    assignment_id: &str,
    attempt_id: Option<&str>,
    event_kind: &str,
    heartbeat_sequence: Option<i64>,
    lease_expires_at_ms: Option<i64>,
    completion_request_id: Option<&str>,
    completion_request_sha256: Option<&str>,
    receipt_sha256: Option<&str>,
    reason_code: Option<&str>,
    occurred_at_ms: i64,
) -> OriginalSourceVerificationResult<String> {
    let row = tx
        .query_opt(
            "SELECT current_event_sequence + 1, current_event_sha256
               FROM jobs_original_source_verification_assignments
              WHERE assignment_id = $1 FOR UPDATE",
            &[&assignment_id],
        )
        .map_err(original_source_storage)?
        .ok_or(OriginalSourceVerificationError::AssignmentNotFound)?;
    let sequence: i64 = row.get(0);
    let predecessor: Option<String> = row.get(1);
    let (event_id, canonical, event_sha256) = original_source_event_payload(
        assignment_id,
        attempt_id,
        sequence,
        predecessor.as_deref(),
        event_kind,
        heartbeat_sequence,
        lease_expires_at_ms,
        completion_request_id,
        completion_request_sha256,
        receipt_sha256,
        reason_code,
        occurred_at_ms,
    )?;
    tx.execute(
        "INSERT INTO jobs_original_source_verification_events (
            event_id, assignment_id, attempt_id, event_sequence,
            predecessor_event_sha256, event_kind, heartbeat_sequence,
            lease_expires_at_ms, completion_request_id,
            completion_request_sha256, receipt_sha256, reason_code,
            canonical_event_json, event_sha256, occurred_at_ms
         ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10,
                   $11, $12, $13, $14, $15)",
        &[
            &event_id,
            &assignment_id,
            &attempt_id,
            &sequence,
            &predecessor,
            &event_kind,
            &heartbeat_sequence,
            &lease_expires_at_ms,
            &completion_request_id,
            &completion_request_sha256,
            &receipt_sha256,
            &reason_code,
            &canonical,
            &event_sha256,
            &occurred_at_ms,
        ],
    )
    .map_err(original_source_storage)?;
    let changed = tx
        .execute(
            "UPDATE jobs_original_source_verification_assignments
                SET current_event_sequence = $2, current_event_sha256 = $3,
                    updated_at_ms = GREATEST(updated_at_ms, $4)
              WHERE assignment_id = $1 AND current_event_sequence = $2 - 1",
            &[&assignment_id, &sequence, &event_sha256, &occurred_at_ms],
        )
        .map_err(original_source_storage)?;
    if changed != 1 {
        return Err(OriginalSourceVerificationError::ConcurrentHeadAdvance);
    }
    Ok(event_sha256)
}

fn original_source_validate_binding(
    binding: &OriginalSourceVerifierBinding,
) -> OriginalSourceVerificationResult<()> {
    if !original_source_valid_id(&binding.worker_id)
        || !original_source_valid_id(&binding.runtime_instance_id)
        || !(1..=9_007_199_254_740_991).contains(&binding.runtime_instance_epoch)
        || !original_source_valid_sha256(&binding.runtime_authority_sha256)
        || binding.runtime_session_token.len() < 32
        || binding.runtime_session_token.len() > 4096
    {
        return Err(OriginalSourceVerificationError::InvalidInput(
            "managed verifier binding is invalid".to_string(),
        ));
    }
    Ok(())
}

fn original_source_new_lease_token() -> String {
    let mut bytes = [0_u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

fn original_source_require_operational_hold_clear_sqlite(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    job_id: &str,
) -> OriginalSourceVerificationResult<()> {
    let context = operational_hold_context_for_job_sqlite_tx(tx, account_id, job_id, None, None)
        .map_err(original_source_storage)?;
    require_operational_capability_sqlite_tx(
        tx,
        OperationalCapability::OriginalSourceVerification,
        &context,
    )
    .map_err(|error| match error {
        OperationalHoldError::Held(_) => {
            OriginalSourceVerificationError::ManagedRuntimeAuthorityUnavailable
        }
        other => original_source_storage(other),
    })
}

fn original_source_require_operational_hold_clear_postgres(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    job_id: &str,
) -> OriginalSourceVerificationResult<()> {
    let context = operational_hold_context_for_job_postgres_tx(tx, account_id, job_id, None, None)
        .map_err(original_source_storage)?;
    require_operational_capability_postgres_tx(
        tx,
        OperationalCapability::OriginalSourceVerification,
        &context,
    )
    .map_err(|error| match error {
        OperationalHoldError::Held(_) => {
            OriginalSourceVerificationError::ManagedRuntimeAuthorityUnavailable
        }
        other => original_source_storage(other),
    })
}

fn original_source_lease_candidates_sqlite(
    tx: &rusqlite::Transaction<'_>,
    binding: &OriginalSourceVerifierBinding,
    now: i64,
    cursor: Option<&OriginalSourceLeaseScanCursor>,
) -> OriginalSourceVerificationResult<Vec<OriginalSourceLeaseCandidate>> {
    let cursor_next_attempt_at_ms = cursor.map(|value| value.next_attempt_at_ms);
    let cursor_created_at_ms = cursor.map(|value| value.created_at_ms);
    let cursor_assignment_id = cursor.map(|value| value.assignment_id.as_str());
    let mut statement = tx
        .prepare(
            "SELECT assignment.assignment_id, assignment.account_id,
                    assignment.job_id, assignment.subject_sha256,
                    assignment.canonical_subject_json, assignment.state,
                    assignment.attempt_count, assignment.active_attempt_id,
                    assignment.circuit_state, assignment.expires_at_ms,
                    assignment.next_attempt_at_ms, assignment.created_at_ms
               FROM jobs_original_source_verification_assignments assignment
               JOIN jobs_managed_cloud_original_source_verifier_runtime_instances runtime
                 ON runtime.runtime_instance_id=?2 AND runtime.instance_epoch=?3
                AND runtime.worker_id=?4
                AND runtime.runtime_identity_sha256=assignment.managed_runtime_identity_sha256
                AND runtime.environment=assignment.managed_environment
                AND runtime.region=assignment.managed_region
                AND runtime.channel=assignment.managed_channel
                AND runtime.activation_sha256=assignment.managed_activation_sha256
                AND runtime.manifest_sha256=assignment.managed_manifest_sha256
                AND runtime.head_revision=assignment.managed_head_revision
                AND runtime.transition_sha256=assignment.managed_transition_sha256
                AND runtime.task_queue_sha256=assignment.managed_task_queue_sha256
                AND runtime.failure_converter_sha256=assignment.managed_failure_converter_sha256
                AND runtime.dependency_evidence_sha256=
                    assignment.managed_dependency_evidence_sha256
              WHERE assignment.not_before_at_ms <= ?1
                AND assignment.expires_at_ms > ?1
                AND assignment.attempt_count < assignment.attempt_budget
                AND COALESCE(assignment.last_error_code,'') <> 'operational_hold'
                AND (assignment.managed_channel='general' OR EXISTS(
                  SELECT 1 FROM jobs_managed_cloud_cohort_members member
                   WHERE member.cohort_sha256=assignment.managed_cohort_sha256
                     AND member.account_id=assignment.account_id))
                AND ((assignment.state IN ('pending', 'retry_wait', 'idle')
                  AND assignment.next_attempt_at_ms <= ?1
                  AND (assignment.circuit_state <> 'open'
                    OR assignment.circuit_open_until_ms <= ?1))
                OR (assignment.state = 'leased'
                  AND (assignment.lease_expires_at_ms <= ?1
                    OR assignment.hard_deadline_at_ms <= ?1)))
                AND (?5 IS NULL OR assignment.next_attempt_at_ms > ?5
                  OR (assignment.next_attempt_at_ms = ?5
                    AND assignment.created_at_ms > ?6)
                  OR (assignment.next_attempt_at_ms = ?5
                    AND assignment.created_at_ms = ?6
                    AND assignment.assignment_id > ?7))
              ORDER BY assignment.next_attempt_at_ms,
                       assignment.created_at_ms, assignment.assignment_id
              LIMIT ?8",
        )
        .map_err(original_source_storage)?;
    let candidates = statement
        .query_map(
            params![
                now,
                binding.runtime_instance_id,
                binding.runtime_instance_epoch,
                binding.worker_id,
                cursor_next_attempt_at_ms,
                cursor_created_at_ms,
                cursor_assignment_id,
                ORIGINAL_SOURCE_LEASE_SCAN_LIMIT,
            ],
            |row| {
                Ok(OriginalSourceLeaseCandidate {
                    assignment_id: row.get(0)?,
                    account_id: row.get(1)?,
                    job_id: row.get(2)?,
                    subject_sha256: row.get(3)?,
                    canonical_subject_json: row.get(4)?,
                    state: row.get(5)?,
                    attempt_count: row.get(6)?,
                    active_attempt_id: row.get(7)?,
                    circuit_state: row.get(8)?,
                    assignment_expires_at_ms: row.get(9)?,
                    next_attempt_at_ms: row.get(10)?,
                    created_at_ms: row.get(11)?,
                })
            },
        )
        .map_err(original_source_storage)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(original_source_storage)?;
    Ok(candidates)
}

fn original_source_lease_candidates_postgres(
    tx: &mut postgres::Transaction<'_>,
    binding: &OriginalSourceVerifierBinding,
    now: i64,
    cursor: Option<&OriginalSourceLeaseScanCursor>,
) -> OriginalSourceVerificationResult<Vec<OriginalSourceLeaseCandidate>> {
    let cursor_next_attempt_at_ms = cursor.map(|value| value.next_attempt_at_ms);
    let cursor_created_at_ms = cursor.map(|value| value.created_at_ms);
    let cursor_assignment_id = cursor.map(|value| value.assignment_id.as_str());
    tx.query(
        "SELECT assignment.assignment_id, assignment.account_id,
                assignment.job_id, assignment.subject_sha256,
                assignment.canonical_subject_json, assignment.state,
                assignment.attempt_count, assignment.active_attempt_id,
                assignment.circuit_state, assignment.expires_at_ms,
                assignment.next_attempt_at_ms, assignment.created_at_ms
           FROM jobs_original_source_verification_assignments assignment
           JOIN jobs_managed_cloud_original_source_verifier_runtime_instances runtime
             ON runtime.runtime_instance_id=$2 AND runtime.instance_epoch=$3
            AND runtime.worker_id=$4
            AND runtime.runtime_identity_sha256=assignment.managed_runtime_identity_sha256
            AND runtime.environment=assignment.managed_environment
            AND runtime.region=assignment.managed_region
            AND runtime.channel=assignment.managed_channel
            AND runtime.activation_sha256=assignment.managed_activation_sha256
            AND runtime.manifest_sha256=assignment.managed_manifest_sha256
            AND runtime.head_revision=assignment.managed_head_revision
            AND runtime.transition_sha256=assignment.managed_transition_sha256
            AND runtime.task_queue_sha256=assignment.managed_task_queue_sha256
            AND runtime.failure_converter_sha256=assignment.managed_failure_converter_sha256
            AND runtime.dependency_evidence_sha256=
                assignment.managed_dependency_evidence_sha256
          WHERE assignment.not_before_at_ms <= $1
            AND assignment.expires_at_ms > $1
            AND assignment.attempt_count < assignment.attempt_budget
            AND assignment.last_error_code IS DISTINCT FROM 'operational_hold'
            AND (assignment.managed_channel='general' OR EXISTS(
              SELECT 1 FROM jobs_managed_cloud_cohort_members member
               WHERE member.cohort_sha256=assignment.managed_cohort_sha256
                 AND member.account_id=assignment.account_id))
            AND ((assignment.state IN ('pending', 'retry_wait', 'idle')
              AND assignment.next_attempt_at_ms <= $1
              AND (assignment.circuit_state <> 'open'
                OR assignment.circuit_open_until_ms <= $1))
            OR (assignment.state = 'leased'
              AND (assignment.lease_expires_at_ms <= $1
                OR assignment.hard_deadline_at_ms <= $1)))
            AND ($5::BIGINT IS NULL OR assignment.next_attempt_at_ms > $5
              OR (assignment.next_attempt_at_ms = $5
                AND assignment.created_at_ms > $6)
              OR (assignment.next_attempt_at_ms = $5
                AND assignment.created_at_ms = $6
                AND assignment.assignment_id > $7))
          ORDER BY assignment.next_attempt_at_ms,
                   assignment.created_at_ms, assignment.assignment_id
          LIMIT $8",
        &[
            &now,
            &binding.runtime_instance_id,
            &binding.runtime_instance_epoch,
            &binding.worker_id,
            &cursor_next_attempt_at_ms,
            &cursor_created_at_ms,
            &cursor_assignment_id,
            &ORIGINAL_SOURCE_LEASE_SCAN_LIMIT,
        ],
    )
    .map_err(original_source_storage)
    .map(|rows| {
        rows.into_iter()
            .map(|row| OriginalSourceLeaseCandidate {
                assignment_id: row.get(0),
                account_id: row.get(1),
                job_id: row.get(2),
                subject_sha256: row.get(3),
                canonical_subject_json: row.get(4),
                state: row.get(5),
                attempt_count: row.get(6),
                active_attempt_id: row.get(7),
                circuit_state: row.get(8),
                assignment_expires_at_ms: row.get(9),
                next_attempt_at_ms: row.get(10),
                created_at_ms: row.get(11),
            })
            .collect()
    })
}

fn original_source_lock_lease_candidate_postgres(
    tx: &mut postgres::Transaction<'_>,
    binding: &OriginalSourceVerifierBinding,
    enumerated: &OriginalSourceLeaseCandidate,
    now: i64,
) -> OriginalSourceVerificationResult<Option<OriginalSourceLeaseCandidate>> {
    // The caller already holds H and exclusive M. D must precede the assignment row lock.
    lock_discovery_account_shared_postgres(tx, &enumerated.account_id)
        .map_err(original_source_storage)?;
    tx.query_opt(
        "SELECT assignment.assignment_id, assignment.account_id,
                assignment.job_id, assignment.subject_sha256,
                assignment.canonical_subject_json, assignment.state,
                assignment.attempt_count, assignment.active_attempt_id,
                assignment.circuit_state, assignment.expires_at_ms,
                assignment.next_attempt_at_ms, assignment.created_at_ms
           FROM jobs_original_source_verification_assignments assignment
           JOIN jobs_managed_cloud_original_source_verifier_runtime_instances runtime
             ON runtime.runtime_instance_id=$3 AND runtime.instance_epoch=$4
            AND runtime.worker_id=$5
            AND runtime.runtime_identity_sha256=assignment.managed_runtime_identity_sha256
            AND runtime.environment=assignment.managed_environment
            AND runtime.region=assignment.managed_region
            AND runtime.channel=assignment.managed_channel
            AND runtime.activation_sha256=assignment.managed_activation_sha256
            AND runtime.manifest_sha256=assignment.managed_manifest_sha256
            AND runtime.head_revision=assignment.managed_head_revision
            AND runtime.transition_sha256=assignment.managed_transition_sha256
            AND runtime.task_queue_sha256=assignment.managed_task_queue_sha256
            AND runtime.failure_converter_sha256=assignment.managed_failure_converter_sha256
            AND runtime.dependency_evidence_sha256=
                assignment.managed_dependency_evidence_sha256
          WHERE assignment.assignment_id=$1
            AND assignment.account_id=$6 AND assignment.job_id=$7
            AND assignment.not_before_at_ms <= $2
            AND assignment.expires_at_ms > $2
            AND assignment.attempt_count < assignment.attempt_budget
            AND assignment.last_error_code IS DISTINCT FROM 'operational_hold'
            AND (assignment.managed_channel='general' OR EXISTS(
              SELECT 1 FROM jobs_managed_cloud_cohort_members member
               WHERE member.cohort_sha256=assignment.managed_cohort_sha256
                 AND member.account_id=assignment.account_id))
            AND ((assignment.state IN ('pending', 'retry_wait', 'idle')
              AND assignment.next_attempt_at_ms <= $2
              AND (assignment.circuit_state <> 'open'
                OR assignment.circuit_open_until_ms <= $2))
            OR (assignment.state = 'leased'
              AND (assignment.lease_expires_at_ms <= $2
                OR assignment.hard_deadline_at_ms <= $2)))
          FOR UPDATE OF assignment",
        &[
            &enumerated.assignment_id,
            &now,
            &binding.runtime_instance_id,
            &binding.runtime_instance_epoch,
            &binding.worker_id,
            &enumerated.account_id,
            &enumerated.job_id,
        ],
    )
    .map_err(original_source_storage)
    .map(|row| {
        row.map(|row| OriginalSourceLeaseCandidate {
            assignment_id: row.get(0),
            account_id: row.get(1),
            job_id: row.get(2),
            subject_sha256: row.get(3),
            canonical_subject_json: row.get(4),
            state: row.get(5),
            attempt_count: row.get(6),
            active_attempt_id: row.get(7),
            circuit_state: row.get(8),
            assignment_expires_at_ms: row.get(9),
            next_attempt_at_ms: row.get(10),
            created_at_ms: row.get(11),
        })
    })
}

fn original_source_subject_recheck_decision(
    result: OriginalSourceVerificationResult<JobPosting>,
) -> OriginalSourceVerificationResult<OriginalSourceLeaseCandidateDecision> {
    match result {
        Ok(_) => Ok(OriginalSourceLeaseCandidateDecision::Lease),
        Err(OriginalSourceVerificationError::InvalidInput(_)) => Ok(
            OriginalSourceLeaseCandidateDecision::Supersede("source_untrusted"),
        ),
        Err(OriginalSourceVerificationError::AssignmentNotFound) => Ok(
            OriginalSourceLeaseCandidateDecision::Supersede("subject_revoked"),
        ),
        Err(OriginalSourceVerificationError::LeaseLost) => Ok(
            OriginalSourceLeaseCandidateDecision::Supersede("subject_changed"),
        ),
        Err(error) => Err(error),
    }
}

fn original_source_lease_candidate_decision_sqlite(
    tx: &rusqlite::Transaction<'_>,
    binding: &OriginalSourceVerifierBinding,
    candidate: &OriginalSourceLeaseCandidate,
) -> OriginalSourceVerificationResult<OriginalSourceLeaseCandidateDecision> {
    let current_authority = match original_source_require_assignment_authority_sqlite(
        tx,
        &candidate.assignment_id,
        &candidate.account_id,
    ) {
        Ok(authority) => authority,
        Err(OriginalSourceVerificationError::ManagedRuntimeAuthorityUnavailable) => {
            return Ok(OriginalSourceLeaseCandidateDecision::Supersede(
                "managed_authority_revoked",
            ));
        }
        Err(error) => return Err(error),
    };
    match original_source_require_runtime_binding_sqlite(tx, binding, &current_authority.binding) {
        Ok(_) => {}
        Err(OriginalSourceVerificationError::ManagedRuntimeAuthorityUnavailable) => {
            return Ok(OriginalSourceLeaseCandidateDecision::Supersede(
                "managed_runtime_revoked",
            ));
        }
        Err(error) => return Err(error),
    }
    let subject_decision =
        original_source_subject_recheck_decision(recheck_original_source_subject_sqlite(
            tx,
            &candidate.account_id,
            &candidate.job_id,
            &candidate.subject_sha256,
        ))?;
    if subject_decision != OriginalSourceLeaseCandidateDecision::Lease {
        return Ok(subject_decision);
    }
    match original_source_require_operational_hold_clear_sqlite(
        tx,
        &candidate.account_id,
        &candidate.job_id,
    ) {
        Ok(()) => Ok(OriginalSourceLeaseCandidateDecision::Lease),
        Err(OriginalSourceVerificationError::ManagedRuntimeAuthorityUnavailable) => {
            Ok(OriginalSourceLeaseCandidateDecision::DeferOperationalHold)
        }
        Err(error) => Err(error),
    }
}

fn original_source_lease_candidate_decision_postgres(
    tx: &mut postgres::Transaction<'_>,
    binding: &OriginalSourceVerifierBinding,
    candidate: &OriginalSourceLeaseCandidate,
) -> OriginalSourceVerificationResult<OriginalSourceLeaseCandidateDecision> {
    let current_authority = match original_source_require_assignment_authority_postgres(
        tx,
        &candidate.assignment_id,
        &candidate.account_id,
    ) {
        Ok(authority) => authority,
        Err(OriginalSourceVerificationError::ManagedRuntimeAuthorityUnavailable) => {
            return Ok(OriginalSourceLeaseCandidateDecision::Supersede(
                "managed_authority_revoked",
            ));
        }
        Err(error) => return Err(error),
    };
    match original_source_require_runtime_binding_postgres(tx, binding, &current_authority.binding)
    {
        Ok(_) => {}
        Err(OriginalSourceVerificationError::ManagedRuntimeAuthorityUnavailable) => {
            return Ok(OriginalSourceLeaseCandidateDecision::Supersede(
                "managed_runtime_revoked",
            ));
        }
        Err(error) => return Err(error),
    }
    let subject_decision =
        original_source_subject_recheck_decision(recheck_original_source_subject_postgres(
            tx,
            &candidate.account_id,
            &candidate.job_id,
            &candidate.subject_sha256,
        ))?;
    if subject_decision != OriginalSourceLeaseCandidateDecision::Lease {
        return Ok(subject_decision);
    }
    match original_source_require_operational_hold_clear_postgres(
        tx,
        &candidate.account_id,
        &candidate.job_id,
    ) {
        Ok(()) => Ok(OriginalSourceLeaseCandidateDecision::Lease),
        Err(OriginalSourceVerificationError::ManagedRuntimeAuthorityUnavailable) => {
            Ok(OriginalSourceLeaseCandidateDecision::DeferOperationalHold)
        }
        Err(error) => Err(error),
    }
}

fn original_source_supersede_lease_candidate_sqlite(
    tx: &rusqlite::Transaction<'_>,
    candidate: &OriginalSourceLeaseCandidate,
    reason: &str,
    now: i64,
) -> OriginalSourceVerificationResult<()> {
    original_source_append_event_sqlite(
        tx,
        &candidate.assignment_id,
        candidate.active_attempt_id.as_deref(),
        "superseded",
        None,
        None,
        None,
        None,
        None,
        Some(reason),
        now,
    )?;
    let changed = tx
        .execute(
            "UPDATE jobs_original_source_verification_assignments SET
                state='superseded', active_attempt_id=NULL, lease_owner=NULL,
                lease_token_sha256=NULL, lease_expires_at_ms=NULL,
                hard_deadline_at_ms=NULL, heartbeat_sequence=0,
                last_error_code=?2, updated_at_ms=?3
              WHERE assignment_id=?1 AND state NOT IN ('superseded','cancelled')",
            params![candidate.assignment_id, reason, now],
        )
        .map_err(original_source_storage)?;
    if changed != 1 {
        return Err(OriginalSourceVerificationError::LeaseLost);
    }
    Ok(())
}

fn original_source_supersede_lease_candidate_postgres(
    tx: &mut postgres::Transaction<'_>,
    candidate: &OriginalSourceLeaseCandidate,
    reason: &str,
    now: i64,
) -> OriginalSourceVerificationResult<()> {
    original_source_append_event_postgres(
        tx,
        &candidate.assignment_id,
        candidate.active_attempt_id.as_deref(),
        "superseded",
        None,
        None,
        None,
        None,
        None,
        Some(reason),
        now,
    )?;
    let changed = tx
        .execute(
            "UPDATE jobs_original_source_verification_assignments SET
                state='superseded', active_attempt_id=NULL, lease_owner=NULL,
                lease_token_sha256=NULL, lease_expires_at_ms=NULL,
                hard_deadline_at_ms=NULL, heartbeat_sequence=0,
                last_error_code=$2, updated_at_ms=$3
              WHERE assignment_id=$1 AND state NOT IN ('superseded','cancelled')",
            &[&candidate.assignment_id, &reason, &now],
        )
        .map_err(original_source_storage)?;
    if changed != 1 {
        return Err(OriginalSourceVerificationError::LeaseLost);
    }
    Ok(())
}

fn original_source_defer_operational_hold_sqlite(
    tx: &rusqlite::Transaction<'_>,
    candidate: &OriginalSourceLeaseCandidate,
    now: i64,
) -> OriginalSourceVerificationResult<()> {
    if candidate.state == "leased" {
        original_source_append_event_sqlite(
            tx,
            &candidate.assignment_id,
            candidate.active_attempt_id.as_deref(),
            "lease_expired",
            None,
            None,
            None,
            None,
            None,
            Some("lease_expired"),
            now,
        )?;
    }
    original_source_append_event_sqlite(
        tx,
        &candidate.assignment_id,
        candidate.active_attempt_id.as_deref(),
        "released",
        None,
        None,
        None,
        None,
        None,
        Some("operational_hold"),
        now,
    )?;
    let next_attempt_at_ms = now.saturating_add(ORIGINAL_SOURCE_HOLD_RECHECK_MS);
    let changed = tx
        .execute(
            "UPDATE jobs_original_source_verification_assignments SET
                state='retry_wait', active_attempt_id=NULL, lease_owner=NULL,
                lease_token_sha256=NULL, lease_expires_at_ms=NULL,
                hard_deadline_at_ms=NULL, heartbeat_sequence=0,
                next_attempt_at_ms=?2, last_error_code='operational_hold',
                updated_at_ms=?3
              WHERE assignment_id=?1 AND state NOT IN ('superseded','cancelled')",
            params![&candidate.assignment_id, next_attempt_at_ms, now],
        )
        .map_err(original_source_storage)?;
    if changed != 1 {
        return Err(OriginalSourceVerificationError::LeaseLost);
    }
    Ok(())
}

fn original_source_defer_operational_hold_postgres(
    tx: &mut postgres::Transaction<'_>,
    candidate: &OriginalSourceLeaseCandidate,
    now: i64,
) -> OriginalSourceVerificationResult<()> {
    if candidate.state == "leased" {
        original_source_append_event_postgres(
            tx,
            &candidate.assignment_id,
            candidate.active_attempt_id.as_deref(),
            "lease_expired",
            None,
            None,
            None,
            None,
            None,
            Some("lease_expired"),
            now,
        )?;
    }
    original_source_append_event_postgres(
        tx,
        &candidate.assignment_id,
        candidate.active_attempt_id.as_deref(),
        "released",
        None,
        None,
        None,
        None,
        None,
        Some("operational_hold"),
        now,
    )?;
    let next_attempt_at_ms = now.saturating_add(ORIGINAL_SOURCE_HOLD_RECHECK_MS);
    let changed = tx
        .execute(
            "UPDATE jobs_original_source_verification_assignments SET
                state='retry_wait', active_attempt_id=NULL, lease_owner=NULL,
                lease_token_sha256=NULL, lease_expires_at_ms=NULL,
                hard_deadline_at_ms=NULL, heartbeat_sequence=0,
                next_attempt_at_ms=$2, last_error_code='operational_hold',
                updated_at_ms=$3
              WHERE assignment_id=$1 AND state NOT IN ('superseded','cancelled')",
            &[&candidate.assignment_id, &next_attempt_at_ms, &now],
        )
        .map_err(original_source_storage)?;
    if changed != 1 {
        return Err(OriginalSourceVerificationError::LeaseLost);
    }
    Ok(())
}

fn original_source_deferred_hold_candidates_sqlite(
    tx: &rusqlite::Transaction<'_>,
    binding: &OriginalSourceVerifierBinding,
    now: i64,
) -> OriginalSourceVerificationResult<Vec<OriginalSourceLeaseCandidate>> {
    let mut statement = tx
        .prepare(
            "SELECT assignment.assignment_id,assignment.account_id,assignment.job_id,
                    assignment.subject_sha256,assignment.canonical_subject_json,
                    assignment.state,assignment.attempt_count,assignment.active_attempt_id,
                    assignment.circuit_state,assignment.expires_at_ms,
                    assignment.next_attempt_at_ms,assignment.created_at_ms
               FROM jobs_original_source_verification_assignments assignment
               JOIN jobs_managed_cloud_original_source_verifier_runtime_instances runtime
                 ON runtime.runtime_instance_id=?2 AND runtime.instance_epoch=?3
                AND runtime.worker_id=?4
                AND runtime.runtime_identity_sha256=assignment.managed_runtime_identity_sha256
                AND runtime.environment=assignment.managed_environment
                AND runtime.region=assignment.managed_region
                AND runtime.channel=assignment.managed_channel
                AND runtime.activation_sha256=assignment.managed_activation_sha256
                AND runtime.manifest_sha256=assignment.managed_manifest_sha256
                AND runtime.head_revision=assignment.managed_head_revision
                AND runtime.transition_sha256=assignment.managed_transition_sha256
                AND runtime.task_queue_sha256=assignment.managed_task_queue_sha256
                AND runtime.failure_converter_sha256=assignment.managed_failure_converter_sha256
                AND runtime.dependency_evidence_sha256=
                    assignment.managed_dependency_evidence_sha256
              WHERE assignment.state='retry_wait'
                AND assignment.last_error_code='operational_hold'
                AND assignment.next_attempt_at_ms <= ?1
                AND assignment.expires_at_ms > ?1
                AND assignment.attempt_count < assignment.attempt_budget
              ORDER BY assignment.next_attempt_at_ms,assignment.updated_at_ms,
                       assignment.assignment_id
              LIMIT ?5",
        )
        .map_err(original_source_storage)?;
    let candidates = statement
        .query_map(
            params![
                now,
                binding.runtime_instance_id,
                binding.runtime_instance_epoch,
                binding.worker_id,
                ORIGINAL_SOURCE_HOLD_RECHECK_LIMIT,
            ],
            |row| {
                Ok(OriginalSourceLeaseCandidate {
                    assignment_id: row.get(0)?,
                    account_id: row.get(1)?,
                    job_id: row.get(2)?,
                    subject_sha256: row.get(3)?,
                    canonical_subject_json: row.get(4)?,
                    state: row.get(5)?,
                    attempt_count: row.get(6)?,
                    active_attempt_id: row.get(7)?,
                    circuit_state: row.get(8)?,
                    assignment_expires_at_ms: row.get(9)?,
                    next_attempt_at_ms: row.get(10)?,
                    created_at_ms: row.get(11)?,
                })
            },
        )
        .map_err(original_source_storage)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(original_source_storage)?;
    Ok(candidates)
}

fn original_source_deferred_hold_candidates_postgres(
    tx: &mut postgres::Transaction<'_>,
    binding: &OriginalSourceVerifierBinding,
    now: i64,
) -> OriginalSourceVerificationResult<Vec<OriginalSourceLeaseCandidate>> {
    tx.query(
        "SELECT assignment.assignment_id,assignment.account_id,assignment.job_id,
                assignment.subject_sha256,assignment.canonical_subject_json,
                assignment.state,assignment.attempt_count,assignment.active_attempt_id,
                assignment.circuit_state,assignment.expires_at_ms,
                assignment.next_attempt_at_ms,assignment.created_at_ms
           FROM jobs_original_source_verification_assignments assignment
           JOIN jobs_managed_cloud_original_source_verifier_runtime_instances runtime
             ON runtime.runtime_instance_id=$2 AND runtime.instance_epoch=$3
            AND runtime.worker_id=$4
            AND runtime.runtime_identity_sha256=assignment.managed_runtime_identity_sha256
            AND runtime.environment=assignment.managed_environment
            AND runtime.region=assignment.managed_region
            AND runtime.channel=assignment.managed_channel
            AND runtime.activation_sha256=assignment.managed_activation_sha256
            AND runtime.manifest_sha256=assignment.managed_manifest_sha256
            AND runtime.head_revision=assignment.managed_head_revision
            AND runtime.transition_sha256=assignment.managed_transition_sha256
            AND runtime.task_queue_sha256=assignment.managed_task_queue_sha256
            AND runtime.failure_converter_sha256=assignment.managed_failure_converter_sha256
            AND runtime.dependency_evidence_sha256=
                assignment.managed_dependency_evidence_sha256
          WHERE assignment.state='retry_wait'
            AND assignment.last_error_code='operational_hold'
            AND assignment.next_attempt_at_ms <= $1
            AND assignment.expires_at_ms > $1
            AND assignment.attempt_count < assignment.attempt_budget
          ORDER BY assignment.next_attempt_at_ms,assignment.updated_at_ms,
                   assignment.assignment_id
          LIMIT $5",
        &[
            &now,
            &binding.runtime_instance_id,
            &binding.runtime_instance_epoch,
            &binding.worker_id,
            &ORIGINAL_SOURCE_HOLD_RECHECK_LIMIT,
        ],
    )
    .map_err(original_source_storage)
    .map(|rows| {
        rows.into_iter()
            .map(|row| OriginalSourceLeaseCandidate {
                assignment_id: row.get(0),
                account_id: row.get(1),
                job_id: row.get(2),
                subject_sha256: row.get(3),
                canonical_subject_json: row.get(4),
                state: row.get(5),
                attempt_count: row.get(6),
                active_attempt_id: row.get(7),
                circuit_state: row.get(8),
                assignment_expires_at_ms: row.get(9),
                next_attempt_at_ms: row.get(10),
                created_at_ms: row.get(11),
            })
            .collect()
    })
}

fn original_source_lock_deferred_hold_candidate_postgres(
    tx: &mut postgres::Transaction<'_>,
    enumerated: &OriginalSourceLeaseCandidate,
    now: i64,
) -> OriginalSourceVerificationResult<Option<OriginalSourceLeaseCandidate>> {
    lock_discovery_account_shared_postgres(tx, &enumerated.account_id)
        .map_err(original_source_storage)?;
    tx.query_opt(
        "SELECT assignment_id,account_id,job_id,subject_sha256,
                canonical_subject_json,state,attempt_count,active_attempt_id,
                circuit_state,expires_at_ms,next_attempt_at_ms,created_at_ms
           FROM jobs_original_source_verification_assignments
          WHERE assignment_id=$1 AND account_id=$2 AND job_id=$3
            AND state='retry_wait' AND last_error_code='operational_hold'
            AND next_attempt_at_ms <= $4 AND expires_at_ms > $4
            AND attempt_count < attempt_budget
          FOR UPDATE",
        &[
            &enumerated.assignment_id,
            &enumerated.account_id,
            &enumerated.job_id,
            &now,
        ],
    )
    .map_err(original_source_storage)
    .map(|row| {
        row.map(|row| OriginalSourceLeaseCandidate {
            assignment_id: row.get(0),
            account_id: row.get(1),
            job_id: row.get(2),
            subject_sha256: row.get(3),
            canonical_subject_json: row.get(4),
            state: row.get(5),
            attempt_count: row.get(6),
            active_attempt_id: row.get(7),
            circuit_state: row.get(8),
            assignment_expires_at_ms: row.get(9),
            next_attempt_at_ms: row.get(10),
            created_at_ms: row.get(11),
        })
    })
}

fn original_source_rotate_deferred_hold_sqlite(
    tx: &rusqlite::Transaction<'_>,
    candidate: &OriginalSourceLeaseCandidate,
    now: i64,
    released: bool,
) -> OriginalSourceVerificationResult<()> {
    if released {
        original_source_append_event_sqlite(
            tx,
            &candidate.assignment_id,
            None,
            "released",
            None,
            None,
            None,
            None,
            None,
            Some("operational_hold_released"),
            now,
        )?;
    }
    let state = if released { "pending" } else { "retry_wait" };
    let next_attempt_at_ms = if released {
        now
    } else {
        now.saturating_add(ORIGINAL_SOURCE_HOLD_RECHECK_MS)
    };
    let last_error_code = (!released).then_some("operational_hold");
    let changed = tx
        .execute(
            "UPDATE jobs_original_source_verification_assignments SET
                state=?2,next_attempt_at_ms=?3,last_error_code=?4,updated_at_ms=?5
              WHERE assignment_id=?1 AND state='retry_wait'
                AND last_error_code='operational_hold'",
            params![
                &candidate.assignment_id,
                state,
                next_attempt_at_ms,
                last_error_code,
                now,
            ],
        )
        .map_err(original_source_storage)?;
    if changed != 1 {
        return Err(OriginalSourceVerificationError::LeaseLost);
    }
    Ok(())
}

fn original_source_rotate_deferred_hold_postgres(
    tx: &mut postgres::Transaction<'_>,
    candidate: &OriginalSourceLeaseCandidate,
    now: i64,
    released: bool,
) -> OriginalSourceVerificationResult<()> {
    if released {
        original_source_append_event_postgres(
            tx,
            &candidate.assignment_id,
            None,
            "released",
            None,
            None,
            None,
            None,
            None,
            Some("operational_hold_released"),
            now,
        )?;
    }
    let state = if released { "pending" } else { "retry_wait" };
    let next_attempt_at_ms = if released {
        now
    } else {
        now.saturating_add(ORIGINAL_SOURCE_HOLD_RECHECK_MS)
    };
    let last_error_code = (!released).then_some("operational_hold");
    let changed = tx
        .execute(
            "UPDATE jobs_original_source_verification_assignments SET
                state=$2,next_attempt_at_ms=$3,last_error_code=$4,updated_at_ms=$5
              WHERE assignment_id=$1 AND state='retry_wait'
                AND last_error_code='operational_hold'",
            &[
                &candidate.assignment_id,
                &state,
                &next_attempt_at_ms,
                &last_error_code,
                &now,
            ],
        )
        .map_err(original_source_storage)?;
    if changed != 1 {
        return Err(OriginalSourceVerificationError::LeaseLost);
    }
    Ok(())
}

fn original_source_reconcile_deferred_holds_sqlite(
    tx: &rusqlite::Transaction<'_>,
    binding: &OriginalSourceVerifierBinding,
    now: i64,
) -> OriginalSourceVerificationResult<()> {
    for candidate in original_source_deferred_hold_candidates_sqlite(tx, binding, now)? {
        match original_source_lease_candidate_decision_sqlite(tx, binding, &candidate)? {
            OriginalSourceLeaseCandidateDecision::DeferOperationalHold => {
                original_source_rotate_deferred_hold_sqlite(tx, &candidate, now, false)?;
            }
            OriginalSourceLeaseCandidateDecision::Supersede(reason) => {
                original_source_supersede_lease_candidate_sqlite(tx, &candidate, reason, now)?;
            }
            OriginalSourceLeaseCandidateDecision::Lease => {
                original_source_rotate_deferred_hold_sqlite(tx, &candidate, now, true)?;
            }
        }
    }
    Ok(())
}

fn original_source_reconcile_deferred_holds_postgres(
    tx: &mut postgres::Transaction<'_>,
    binding: &OriginalSourceVerifierBinding,
    now: i64,
) -> OriginalSourceVerificationResult<()> {
    let candidates = original_source_deferred_hold_candidates_postgres(tx, binding, now)?;
    for enumerated in candidates {
        let Some(candidate) =
            original_source_lock_deferred_hold_candidate_postgres(tx, &enumerated, now)?
        else {
            continue;
        };
        match original_source_lease_candidate_decision_postgres(tx, binding, &candidate)? {
            OriginalSourceLeaseCandidateDecision::DeferOperationalHold => {
                original_source_rotate_deferred_hold_postgres(tx, &candidate, now, false)?;
            }
            OriginalSourceLeaseCandidateDecision::Supersede(reason) => {
                original_source_supersede_lease_candidate_postgres(tx, &candidate, reason, now)?;
            }
            OriginalSourceLeaseCandidateDecision::Lease => {
                original_source_rotate_deferred_hold_postgres(tx, &candidate, now, true)?;
            }
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn original_source_claim_lease_candidate_sqlite(
    tx: &rusqlite::Transaction<'_>,
    binding: &OriginalSourceVerifierBinding,
    candidate: &OriginalSourceLeaseCandidate,
    lease_token: &str,
    lease_token_sha256: &str,
    runtime_session_token_sha256: &str,
    now: i64,
) -> OriginalSourceVerificationResult<OriginalSourceVerificationLease> {
    if candidate.state == "leased" {
        original_source_append_event_sqlite(
            tx,
            &candidate.assignment_id,
            candidate.active_attempt_id.as_deref(),
            "lease_expired",
            None,
            None,
            None,
            None,
            None,
            Some("lease_expired"),
            now,
        )?;
        let changed = tx
            .execute(
                "UPDATE jobs_original_source_verification_assignments SET
                    state='retry_wait', active_attempt_id=NULL,
                    lease_owner=NULL, lease_token_sha256=NULL,
                    lease_expires_at_ms=NULL, hard_deadline_at_ms=NULL,
                    heartbeat_sequence=0, next_attempt_at_ms=?2,
                    updated_at_ms=?2
                  WHERE assignment_id=?1 AND state='leased'",
                params![&candidate.assignment_id, now],
            )
            .map_err(original_source_storage)?;
        if changed != 1 {
            return Err(OriginalSourceVerificationError::LeaseLost);
        }
    }
    if candidate.circuit_state == "open" {
        original_source_append_event_sqlite(
            tx,
            &candidate.assignment_id,
            None,
            "circuit_half_opened",
            None,
            None,
            None,
            None,
            None,
            None,
            now,
        )?;
    }
    let attempt_id = uuid::Uuid::new_v4().to_string();
    let fence = candidate.attempt_count.saturating_add(1);
    let hard_deadline_at_ms = now
        .saturating_add(ORIGINAL_SOURCE_HARD_DEADLINE_MS)
        .min(candidate.assignment_expires_at_ms);
    let lease_expires_at_ms = now
        .saturating_add(ORIGINAL_SOURCE_LEASE_TTL_MS)
        .min(hard_deadline_at_ms);
    tx.execute(
        "INSERT INTO jobs_original_source_verification_attempts (
            attempt_id, assignment_id, account_id, job_id, subject_sha256,
            attempt_no, worker_id, runtime_instance_id,
            runtime_instance_epoch, runtime_authority_sha256,
            runtime_session_token_sha256, lease_token_sha256,
            claimed_at_ms, initial_lease_expires_at_ms, hard_deadline_at_ms
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10,
                   ?11, ?12, ?13, ?14, ?15)",
        params![
            &attempt_id,
            &candidate.assignment_id,
            &candidate.account_id,
            &candidate.job_id,
            &candidate.subject_sha256,
            fence,
            &binding.worker_id,
            &binding.runtime_instance_id,
            binding.runtime_instance_epoch,
            &binding.runtime_authority_sha256,
            runtime_session_token_sha256,
            lease_token_sha256,
            now,
            lease_expires_at_ms,
            hard_deadline_at_ms,
        ],
    )
    .map_err(original_source_storage)?;
    let changed = tx
        .execute(
            "UPDATE jobs_original_source_verification_assignments SET
                state='leased', attempt_count=?2, active_attempt_id=?3,
                lease_owner=?4, lease_token_sha256=?5,
                lease_expires_at_ms=?6, hard_deadline_at_ms=?7,
                heartbeat_sequence=0,
                circuit_state=CASE WHEN circuit_state='open'
                  THEN 'half_open' ELSE circuit_state END,
                circuit_open_until_ms=NULL, updated_at_ms=?8
              WHERE assignment_id=?1 AND attempt_count=?2 - 1
                AND state IN ('pending','retry_wait','idle')
                AND next_attempt_at_ms <= ?8
                AND not_before_at_ms <= ?8 AND expires_at_ms > ?8
                AND (circuit_state <> 'open' OR circuit_open_until_ms <= ?8)
                AND ?2 <= attempt_budget",
            params![
                &candidate.assignment_id,
                fence,
                &attempt_id,
                &binding.worker_id,
                lease_token_sha256,
                lease_expires_at_ms,
                hard_deadline_at_ms,
                now,
            ],
        )
        .map_err(original_source_storage)?;
    if changed != 1 {
        return Err(OriginalSourceVerificationError::LeaseLost);
    }
    original_source_append_event_sqlite(
        tx,
        &candidate.assignment_id,
        Some(&attempt_id),
        "claimed",
        None,
        Some(lease_expires_at_ms),
        None,
        None,
        None,
        None,
        now,
    )?;
    Ok(OriginalSourceVerificationLease {
        assignment_id: candidate.assignment_id.clone(),
        subject_sha256: candidate.subject_sha256.clone(),
        canonical_subject_json: candidate.canonical_subject_json.clone(),
        attempt_id,
        fence,
        lease_token: lease_token.to_string(),
        lease_expires_at_ms,
        hard_deadline_at_ms,
        heartbeat_sequence: 0,
    })
}

#[allow(clippy::too_many_arguments)]
fn original_source_claim_lease_candidate_postgres(
    tx: &mut postgres::Transaction<'_>,
    binding: &OriginalSourceVerifierBinding,
    candidate: &OriginalSourceLeaseCandidate,
    lease_token: &str,
    lease_token_sha256: &str,
    runtime_session_token_sha256: &str,
    now: i64,
) -> OriginalSourceVerificationResult<OriginalSourceVerificationLease> {
    if candidate.state == "leased" {
        original_source_append_event_postgres(
            tx,
            &candidate.assignment_id,
            candidate.active_attempt_id.as_deref(),
            "lease_expired",
            None,
            None,
            None,
            None,
            None,
            Some("lease_expired"),
            now,
        )?;
        let changed = tx
            .execute(
                "UPDATE jobs_original_source_verification_assignments SET
                    state='retry_wait', active_attempt_id=NULL,
                    lease_owner=NULL, lease_token_sha256=NULL,
                    lease_expires_at_ms=NULL, hard_deadline_at_ms=NULL,
                    heartbeat_sequence=0, next_attempt_at_ms=$2,
                    updated_at_ms=$2
                  WHERE assignment_id=$1 AND state='leased'",
                &[&candidate.assignment_id, &now],
            )
            .map_err(original_source_storage)?;
        if changed != 1 {
            return Err(OriginalSourceVerificationError::LeaseLost);
        }
    }
    if candidate.circuit_state == "open" {
        original_source_append_event_postgres(
            tx,
            &candidate.assignment_id,
            None,
            "circuit_half_opened",
            None,
            None,
            None,
            None,
            None,
            None,
            now,
        )?;
    }
    let attempt_id = uuid::Uuid::new_v4().to_string();
    let fence = candidate.attempt_count.saturating_add(1);
    let hard_deadline_at_ms = now
        .saturating_add(ORIGINAL_SOURCE_HARD_DEADLINE_MS)
        .min(candidate.assignment_expires_at_ms);
    let lease_expires_at_ms = now
        .saturating_add(ORIGINAL_SOURCE_LEASE_TTL_MS)
        .min(hard_deadline_at_ms);
    tx.execute(
        "INSERT INTO jobs_original_source_verification_attempts (
            attempt_id, assignment_id, account_id, job_id, subject_sha256,
            attempt_no, worker_id, runtime_instance_id,
            runtime_instance_epoch, runtime_authority_sha256,
            runtime_session_token_sha256, lease_token_sha256,
            claimed_at_ms, initial_lease_expires_at_ms, hard_deadline_at_ms
         ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10,
                   $11, $12, $13, $14, $15)",
        &[
            &attempt_id,
            &candidate.assignment_id,
            &candidate.account_id,
            &candidate.job_id,
            &candidate.subject_sha256,
            &fence,
            &binding.worker_id,
            &binding.runtime_instance_id,
            &binding.runtime_instance_epoch,
            &binding.runtime_authority_sha256,
            &runtime_session_token_sha256,
            &lease_token_sha256,
            &now,
            &lease_expires_at_ms,
            &hard_deadline_at_ms,
        ],
    )
    .map_err(original_source_storage)?;
    let changed = tx
        .execute(
            "UPDATE jobs_original_source_verification_assignments SET
                state='leased', attempt_count=$2, active_attempt_id=$3,
                lease_owner=$4, lease_token_sha256=$5,
                lease_expires_at_ms=$6, hard_deadline_at_ms=$7,
                heartbeat_sequence=0,
                circuit_state=CASE WHEN circuit_state='open'
                  THEN 'half_open' ELSE circuit_state END,
                circuit_open_until_ms=NULL, updated_at_ms=$8
              WHERE assignment_id=$1 AND attempt_count=$2 - 1
                AND state IN ('pending','retry_wait','idle')
                AND next_attempt_at_ms <= $8
                AND not_before_at_ms <= $8 AND expires_at_ms > $8
                AND (circuit_state <> 'open' OR circuit_open_until_ms <= $8)
                AND $2 <= attempt_budget",
            &[
                &candidate.assignment_id,
                &fence,
                &attempt_id,
                &binding.worker_id,
                &lease_token_sha256,
                &lease_expires_at_ms,
                &hard_deadline_at_ms,
                &now,
            ],
        )
        .map_err(original_source_storage)?;
    if changed != 1 {
        return Err(OriginalSourceVerificationError::LeaseLost);
    }
    original_source_append_event_postgres(
        tx,
        &candidate.assignment_id,
        Some(&attempt_id),
        "claimed",
        None,
        Some(lease_expires_at_ms),
        None,
        None,
        None,
        None,
        now,
    )?;
    Ok(OriginalSourceVerificationLease {
        assignment_id: candidate.assignment_id.clone(),
        subject_sha256: candidate.subject_sha256.clone(),
        canonical_subject_json: candidate.canonical_subject_json.clone(),
        attempt_id,
        fence,
        lease_token: lease_token.to_string(),
        lease_expires_at_ms,
        hard_deadline_at_ms,
        heartbeat_sequence: 0,
    })
}

pub fn lease_original_source_verification(
    pool: &DbPool,
    binding: &OriginalSourceVerifierBinding,
) -> OriginalSourceVerificationResult<Option<OriginalSourceVerificationLease>> {
    original_source_validate_binding(binding)?;
    let lease_token = original_source_new_lease_token();
    let lease_token_sha256 = original_source_sha256(lease_token.as_bytes());
    let runtime_session_token_sha256 =
        original_source_sha256(binding.runtime_session_token.as_bytes());
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get().map_err(original_source_storage)?;
            let mut tx = conn
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(original_source_storage)?;
            require_original_source_verifier_runtime_active_sqlite_tx(
                &tx,
                &binding.worker_id,
                &binding.runtime_instance_id,
                &binding.runtime_session_token,
                binding.runtime_instance_epoch,
                &binding.runtime_authority_sha256,
            )
            .map_err(|_| OriginalSourceVerificationError::ManagedRuntimeAuthorityUnavailable)?;
            let now = original_source_db_now_sqlite(&tx)?;
            original_source_reconcile_deferred_holds_sqlite(&tx, binding, now)?;
            let lease = original_source_scan_lease_candidates(
                &mut tx,
                |tx, cursor| original_source_lease_candidates_sqlite(tx, binding, now, cursor),
                |_, enumerated| Ok(Some(enumerated.clone())),
                |tx, candidate| {
                    original_source_lease_candidate_decision_sqlite(tx, binding, candidate)
                },
                |tx, candidate| original_source_defer_operational_hold_sqlite(tx, candidate, now),
                |tx, candidate, reason| {
                    original_source_supersede_lease_candidate_sqlite(tx, candidate, reason, now)
                },
                |tx, candidate| {
                    original_source_claim_lease_candidate_sqlite(
                        tx,
                        binding,
                        candidate,
                        &lease_token,
                        &lease_token_sha256,
                        &runtime_session_token_sha256,
                        now,
                    )
                },
            )?;
            tx.commit().map_err(original_source_storage)?;
            Ok(lease)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg().map_err(original_source_storage)?;
            let mut tx = conn.transaction().map_err(original_source_storage)?;
            // Canonical cross-authority order: H -> exclusive M -> per-account D.
            lock_operational_hold_shared_postgres_tx(&mut tx).map_err(original_source_storage)?;
            require_original_source_verifier_runtime_active_postgres_tx(
                &mut tx,
                &binding.worker_id,
                &binding.runtime_instance_id,
                &binding.runtime_session_token,
                binding.runtime_instance_epoch,
                &binding.runtime_authority_sha256,
            )
            .map_err(|_| OriginalSourceVerificationError::ManagedRuntimeAuthorityUnavailable)?;
            let now = original_source_db_now_postgres(&mut tx)?;
            original_source_reconcile_deferred_holds_postgres(&mut tx, binding, now)?;
            let lease = original_source_scan_lease_candidates(
                &mut tx,
                |tx, cursor| original_source_lease_candidates_postgres(tx, binding, now, cursor),
                |tx, enumerated| {
                    original_source_lock_lease_candidate_postgres(tx, binding, enumerated, now)
                },
                |tx, candidate| {
                    original_source_lease_candidate_decision_postgres(tx, binding, candidate)
                },
                |tx, candidate| original_source_defer_operational_hold_postgres(tx, candidate, now),
                |tx, candidate, reason| {
                    original_source_supersede_lease_candidate_postgres(tx, candidate, reason, now)
                },
                |tx, candidate| {
                    original_source_claim_lease_candidate_postgres(
                        tx,
                        binding,
                        candidate,
                        &lease_token,
                        &lease_token_sha256,
                        &runtime_session_token_sha256,
                        now,
                    )
                },
            )?;
            tx.commit().map_err(original_source_storage)?;
            Ok(lease)
        }
    })
}

fn original_source_binding_hashes(
    binding: &OriginalSourceVerifierBinding,
    lease_token: &str,
) -> OriginalSourceVerificationResult<(String, String)> {
    original_source_validate_binding(binding)?;
    if lease_token.len() < 32 || lease_token.len() > 4096 {
        return Err(OriginalSourceVerificationError::InvalidInput(
            "lease token is invalid".to_string(),
        ));
    }
    Ok((
        original_source_sha256(binding.runtime_session_token.as_bytes()),
        original_source_sha256(lease_token.as_bytes()),
    ))
}

pub fn heartbeat_original_source_verification(
    pool: &DbPool,
    request: &OriginalSourceVerificationHeartbeatRequest,
) -> OriginalSourceVerificationResult<OriginalSourceVerificationHeartbeat> {
    if request.fence < 1 || request.heartbeat_sequence < 1 {
        return Err(OriginalSourceVerificationError::InvalidInput(
            "heartbeat fence or sequence is invalid".to_string(),
        ));
    }
    let (session_sha256, lease_sha256) =
        original_source_binding_hashes(&request.binding, &request.lease_token)?;
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get().map_err(original_source_storage)?;
            let tx = conn
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(original_source_storage)?;
            require_original_source_verifier_runtime_active_sqlite_tx(
                &tx,
                &request.binding.worker_id,
                &request.binding.runtime_instance_id,
                &request.binding.runtime_session_token,
                request.binding.runtime_instance_epoch,
                &request.binding.runtime_authority_sha256,
            )
            .map_err(|_| OriginalSourceVerificationError::ManagedRuntimeAuthorityUnavailable)?;
            let now = original_source_db_now_sqlite(&tx)?;
            let row = tx
                .query_row(
                    "SELECT a.state, a.attempt_count, a.active_attempt_id,
                            a.lease_token_sha256, a.lease_expires_at_ms,
                            a.hard_deadline_at_ms, a.heartbeat_sequence,
                            t.worker_id, t.runtime_instance_id,
                            t.runtime_instance_epoch, t.runtime_authority_sha256,
                            t.runtime_session_token_sha256, a.account_id,
                            a.job_id, a.subject_sha256
                       FROM jobs_original_source_verification_assignments a
                       JOIN jobs_original_source_verification_attempts t
                         ON t.attempt_id = ?2 AND t.assignment_id = a.assignment_id
                      WHERE a.assignment_id = ?1",
                    params![request.assignment_id, request.attempt_id],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, i64>(1)?,
                            row.get::<_, Option<String>>(2)?,
                            row.get::<_, Option<String>>(3)?,
                            row.get::<_, Option<i64>>(4)?,
                            row.get::<_, Option<i64>>(5)?,
                            row.get::<_, i64>(6)?,
                            row.get::<_, String>(7)?,
                            row.get::<_, String>(8)?,
                            row.get::<_, i64>(9)?,
                            row.get::<_, String>(10)?,
                            row.get::<_, String>(11)?,
                            row.get::<_, String>(12)?,
                            row.get::<_, String>(13)?,
                            row.get::<_, String>(14)?,
                        ))
                    },
                )
                .optional()
                .map_err(original_source_storage)?
                .ok_or(OriginalSourceVerificationError::LeaseLost)?;
            let (
                state,
                fence,
                active_attempt_id,
                stored_lease_sha256,
                lease_expires_at_ms,
                hard_deadline_at_ms,
                current_sequence,
                worker_id,
                runtime_instance_id,
                runtime_instance_epoch,
                runtime_authority_sha256,
                stored_session_sha256,
                account_id,
                job_id,
                subject_sha256,
            ) = row;
            if state != "leased"
                || fence != request.fence
                || active_attempt_id.as_deref() != Some(request.attempt_id.as_str())
                || stored_lease_sha256.as_deref() != Some(lease_sha256.as_str())
                || worker_id != request.binding.worker_id
                || runtime_instance_id != request.binding.runtime_instance_id
                || runtime_instance_epoch != request.binding.runtime_instance_epoch
                || runtime_authority_sha256 != request.binding.runtime_authority_sha256
                || stored_session_sha256 != session_sha256
            {
                return Err(OriginalSourceVerificationError::LeaseLost);
            }
            let current_authority = original_source_require_assignment_authority_sqlite(
                &tx,
                &request.assignment_id,
                &account_id,
            )?;
            original_source_require_runtime_binding_sqlite(
                &tx,
                &request.binding,
                &current_authority.binding,
            )?;
            recheck_original_source_subject_sqlite(&tx, &account_id, &job_id, &subject_sha256)?;
            original_source_require_operational_hold_clear_sqlite(&tx, &account_id, &job_id)?;
            let old_expiry =
                lease_expires_at_ms.ok_or(OriginalSourceVerificationError::LeaseLost)?;
            let hard_deadline =
                hard_deadline_at_ms.ok_or(OriginalSourceVerificationError::LeaseLost)?;
            if now >= old_expiry || now >= hard_deadline {
                return Err(OriginalSourceVerificationError::LeaseExpired);
            }
            if request.heartbeat_sequence == current_sequence {
                tx.commit().map_err(original_source_storage)?;
                return Ok(OriginalSourceVerificationHeartbeat {
                    lease_expires_at_ms: old_expiry,
                    hard_deadline_at_ms: hard_deadline,
                    heartbeat_sequence: current_sequence,
                    replayed: true,
                });
            }
            if request.heartbeat_sequence != current_sequence.saturating_add(1) {
                return Err(OriginalSourceVerificationError::LeaseLost);
            }
            let new_expiry = now
                .saturating_add(ORIGINAL_SOURCE_LEASE_TTL_MS)
                .min(hard_deadline);
            tx.execute(
                "UPDATE jobs_original_source_verification_assignments
                    SET heartbeat_sequence = ?2, lease_expires_at_ms = ?3,
                        updated_at_ms = ?4
                  WHERE assignment_id = ?1 AND heartbeat_sequence = ?2 - 1",
                params![
                    request.assignment_id,
                    request.heartbeat_sequence,
                    new_expiry,
                    now
                ],
            )
            .map_err(original_source_storage)?;
            original_source_append_event_sqlite(
                &tx,
                &request.assignment_id,
                Some(&request.attempt_id),
                "heartbeat",
                Some(request.heartbeat_sequence),
                Some(new_expiry),
                None,
                None,
                None,
                None,
                now,
            )?;
            tx.commit().map_err(original_source_storage)?;
            Ok(OriginalSourceVerificationHeartbeat {
                lease_expires_at_ms: new_expiry,
                hard_deadline_at_ms: hard_deadline,
                heartbeat_sequence: request.heartbeat_sequence,
                replayed: false,
            })
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg().map_err(original_source_storage)?;
            let mut tx = conn.transaction().map_err(original_source_storage)?;
            lock_operational_hold_shared_postgres_tx(&mut tx).map_err(original_source_storage)?;
            require_original_source_verifier_runtime_active_postgres_tx(
                &mut tx,
                &request.binding.worker_id,
                &request.binding.runtime_instance_id,
                &request.binding.runtime_session_token,
                request.binding.runtime_instance_epoch,
                &request.binding.runtime_authority_sha256,
            )
            .map_err(|_| OriginalSourceVerificationError::ManagedRuntimeAuthorityUnavailable)?;
            let now = original_source_db_now_postgres(&mut tx)?;
            let (expected_account_id, expected_job_id) = original_source_attempt_identity_postgres(
                &mut tx,
                &request.assignment_id,
                &request.attempt_id,
            )?;
            lock_discovery_account_shared_postgres(&mut tx, &expected_account_id)
                .map_err(original_source_storage)?;
            let row = tx
                .query_opt(
                    "SELECT a.state, a.attempt_count, a.active_attempt_id,
                            a.lease_token_sha256, a.lease_expires_at_ms,
                            a.hard_deadline_at_ms, a.heartbeat_sequence,
                            t.worker_id, t.runtime_instance_id,
                            t.runtime_instance_epoch, t.runtime_authority_sha256,
                            t.runtime_session_token_sha256, a.account_id,
                            a.job_id, a.subject_sha256
                       FROM jobs_original_source_verification_assignments a
                       JOIN jobs_original_source_verification_attempts t
                         ON t.attempt_id = $2 AND t.assignment_id = a.assignment_id
                      WHERE a.assignment_id = $1
                        AND a.account_id = $3 AND a.job_id = $4
                      FOR UPDATE OF a",
                    &[
                        &request.assignment_id,
                        &request.attempt_id,
                        &expected_account_id,
                        &expected_job_id,
                    ],
                )
                .map_err(original_source_storage)?
                .ok_or(OriginalSourceVerificationError::LeaseLost)?;
            let state: String = row.get(0);
            let fence: i64 = row.get(1);
            let active_attempt_id: Option<String> = row.get(2);
            let stored_lease_sha256: Option<String> = row.get(3);
            let old_expiry: Option<i64> = row.get(4);
            let hard_deadline: Option<i64> = row.get(5);
            let current_sequence: i64 = row.get(6);
            let binding_matches = row.get::<_, String>(7) == request.binding.worker_id
                && row.get::<_, String>(8) == request.binding.runtime_instance_id
                && row.get::<_, i64>(9) == request.binding.runtime_instance_epoch
                && row.get::<_, String>(10) == request.binding.runtime_authority_sha256
                && row.get::<_, String>(11) == session_sha256;
            if state != "leased"
                || fence != request.fence
                || active_attempt_id.as_deref() != Some(request.attempt_id.as_str())
                || stored_lease_sha256.as_deref() != Some(lease_sha256.as_str())
                || !binding_matches
            {
                return Err(OriginalSourceVerificationError::LeaseLost);
            }
            let account_id: String = row.get(12);
            let job_id: String = row.get(13);
            if account_id != expected_account_id || job_id != expected_job_id {
                return Err(OriginalSourceVerificationError::LeaseLost);
            }
            let subject_sha256: String = row.get(14);
            let current_authority = original_source_require_assignment_authority_postgres(
                &mut tx,
                &request.assignment_id,
                &account_id,
            )?;
            original_source_require_runtime_binding_postgres(
                &mut tx,
                &request.binding,
                &current_authority.binding,
            )?;
            recheck_original_source_subject_postgres(
                &mut tx,
                &account_id,
                &job_id,
                &subject_sha256,
            )?;
            original_source_require_operational_hold_clear_postgres(&mut tx, &account_id, &job_id)?;
            let old_expiry = old_expiry.ok_or(OriginalSourceVerificationError::LeaseLost)?;
            let hard_deadline = hard_deadline.ok_or(OriginalSourceVerificationError::LeaseLost)?;
            if now >= old_expiry || now >= hard_deadline {
                return Err(OriginalSourceVerificationError::LeaseExpired);
            }
            if request.heartbeat_sequence == current_sequence {
                tx.commit().map_err(original_source_storage)?;
                return Ok(OriginalSourceVerificationHeartbeat {
                    lease_expires_at_ms: old_expiry,
                    hard_deadline_at_ms: hard_deadline,
                    heartbeat_sequence: current_sequence,
                    replayed: true,
                });
            }
            if request.heartbeat_sequence != current_sequence.saturating_add(1) {
                return Err(OriginalSourceVerificationError::LeaseLost);
            }
            let new_expiry = now
                .saturating_add(ORIGINAL_SOURCE_LEASE_TTL_MS)
                .min(hard_deadline);
            tx.execute(
                "UPDATE jobs_original_source_verification_assignments
                    SET heartbeat_sequence = $2, lease_expires_at_ms = $3,
                        updated_at_ms = $4
                  WHERE assignment_id = $1 AND heartbeat_sequence = $2 - 1",
                &[
                    &request.assignment_id,
                    &request.heartbeat_sequence,
                    &new_expiry,
                    &now,
                ],
            )
            .map_err(original_source_storage)?;
            original_source_append_event_postgres(
                &mut tx,
                &request.assignment_id,
                Some(&request.attempt_id),
                "heartbeat",
                Some(request.heartbeat_sequence),
                Some(new_expiry),
                None,
                None,
                None,
                None,
                now,
            )?;
            tx.commit().map_err(original_source_storage)?;
            Ok(OriginalSourceVerificationHeartbeat {
                lease_expires_at_ms: new_expiry,
                hard_deadline_at_ms: hard_deadline,
                heartbeat_sequence: request.heartbeat_sequence,
                replayed: false,
            })
        }
    })
}

const ORIGINAL_SOURCE_HEAD_SELECT: &str =
    "SELECT h.account_id, h.job_id, h.head_revision, h.material_generation,
            h.assignment_id, h.receipt_id, h.receipt_sha256, h.subject_sha256,
            h.material_sha256, h.assurance, h.result, h.checked_at_ms,
            h.expires_at_ms, r.canonical_application_url, r.application_domain,
            r.managed_authority_sha256, r.canonical_managed_authority_json,
            a.state
       FROM jobs_original_source_verification_heads h
       JOIN jobs_original_source_verification_transitions t
         ON t.account_id = h.account_id AND t.job_id = h.job_id
        AND t.head_revision = h.head_revision
        AND t.transition_id = h.transition_id
        AND t.transition_sha256 = h.transition_sha256
       JOIN jobs_original_source_verification_receipts r
         ON r.assignment_id = h.assignment_id AND r.receipt_id = h.receipt_id
        AND r.receipt_sha256 = h.receipt_sha256
        AND r.subject_sha256 = h.subject_sha256
        AND r.material_sha256 = h.material_sha256
       JOIN jobs_original_source_verification_assignments a
         ON a.assignment_id = h.assignment_id AND a.account_id = h.account_id
        AND a.job_id = h.job_id AND a.subject_sha256 = h.subject_sha256";

fn original_source_head_from_sqlite(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<OriginalSourceVerificationHead> {
    Ok(OriginalSourceVerificationHead {
        account_id: row.get(0)?,
        job_id: row.get(1)?,
        head_revision: row.get(2)?,
        material_generation: row.get(3)?,
        assignment_id: row.get(4)?,
        receipt_id: row.get(5)?,
        receipt_sha256: row.get(6)?,
        subject_sha256: row.get(7)?,
        material_sha256: row.get(8)?,
        assurance: row.get(9)?,
        result: row.get(10)?,
        checked_at_ms: row.get(11)?,
        expires_at_ms: row.get(12)?,
        canonical_application_url: row.get(13)?,
        application_domain: row.get(14)?,
        managed_authority_sha256: row.get(15)?,
        canonical_managed_authority_json: row.get(16)?,
        assignment_state: row.get(17)?,
    })
}

fn original_source_head_from_postgres(row: &postgres::Row) -> OriginalSourceVerificationHead {
    OriginalSourceVerificationHead {
        account_id: row.get(0),
        job_id: row.get(1),
        head_revision: row.get(2),
        material_generation: row.get(3),
        assignment_id: row.get(4),
        receipt_id: row.get(5),
        receipt_sha256: row.get(6),
        subject_sha256: row.get(7),
        material_sha256: row.get(8),
        assurance: row.get(9),
        result: row.get(10),
        checked_at_ms: row.get(11),
        expires_at_ms: row.get(12),
        canonical_application_url: row.get(13),
        application_domain: row.get(14),
        managed_authority_sha256: row.get(15),
        canonical_managed_authority_json: row.get(16),
        assignment_state: row.get(17),
    }
}

pub(crate) fn resolve_original_source_verification_head_sqlite_tx(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    job_id: &str,
) -> OriginalSourceVerificationResult<Option<OriginalSourceVerificationHead>> {
    tx.query_row(
        &format!("{ORIGINAL_SOURCE_HEAD_SELECT} WHERE h.account_id = ?1 AND h.job_id = ?2"),
        params![account_id, job_id],
        original_source_head_from_sqlite,
    )
    .optional()
    .map_err(original_source_storage)
}

pub(crate) fn resolve_original_source_verification_head_postgres_tx(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    job_id: &str,
) -> OriginalSourceVerificationResult<Option<OriginalSourceVerificationHead>> {
    tx.query_opt(
        &format!(
            "{ORIGINAL_SOURCE_HEAD_SELECT} WHERE h.account_id = $1 AND h.job_id = $2 FOR SHARE OF h"
        ),
        &[&account_id, &job_id],
    )
    .map(|row| row.map(|row| original_source_head_from_postgres(&row)))
    .map_err(original_source_storage)
}

pub fn resolve_original_source_verification_head(
    pool: &DbPool,
    account_id: &str,
    job_id: &str,
) -> OriginalSourceVerificationResult<Option<OriginalSourceVerificationHead>> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get().map_err(original_source_storage)?;
            let tx = conn
                .transaction_with_behavior(TransactionBehavior::Deferred)
                .map_err(original_source_storage)?;
            let result =
                resolve_original_source_verification_head_sqlite_tx(&tx, account_id, job_id)?;
            tx.commit().map_err(original_source_storage)?;
            Ok(result)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg().map_err(original_source_storage)?;
            let mut tx = conn.transaction().map_err(original_source_storage)?;
            let result =
                resolve_original_source_verification_head_postgres_tx(&mut tx, account_id, job_id)?;
            tx.commit().map_err(original_source_storage)?;
            Ok(result)
        }
    })
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct OriginalSourceVerificationExpectedHead {
    pub subject_sha256: String,
    pub material_generation: i64,
    pub material_sha256: String,
    pub receipt_sha256: String,
    pub expires_at_ms: i64,
    pub managed_authority_sha256: String,
}

pub fn compare_original_source_verification_head(
    head: &OriginalSourceVerificationHead,
    expected: &OriginalSourceVerificationExpectedHead,
    db_now_ms: i64,
) -> OriginalSourceVerificationResult<()> {
    if !original_source_valid_sha256(&expected.subject_sha256)
        || !original_source_valid_sha256(&expected.material_sha256)
        || !original_source_valid_sha256(&expected.receipt_sha256)
        || !original_source_valid_sha256(&expected.managed_authority_sha256)
        || expected.material_generation < 1
        || expected.expires_at_ms < 0
    {
        return Err(OriginalSourceVerificationError::InvalidInput(
            "expected original-source head binding is invalid".to_string(),
        ));
    }
    if head.subject_sha256 != expected.subject_sha256
        || head.material_generation != expected.material_generation
        || head.material_sha256 != expected.material_sha256
        || head.receipt_sha256 != expected.receipt_sha256
        || head.expires_at_ms != expected.expires_at_ms
        || head.managed_authority_sha256 != expected.managed_authority_sha256
        || head.assignment_state != "idle"
        || head.expires_at_ms <= db_now_ms
        || head.assurance != "original_verified"
        || head.result != "verified_open"
    {
        return Err(OriginalSourceVerificationError::ConcurrentHeadAdvance);
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct OriginalSourceVerificationProjection {
    pub feature_active: bool,
    pub evidence: Option<JobDiscoveryEvidence>,
    pub expected_head: Option<OriginalSourceVerificationExpectedHead>,
    pub db_time_ms: i64,
}

fn original_source_positive_projection(
    posting: &JobPosting,
    head: &OriginalSourceVerificationHead,
    current_authority_sha256: &str,
    current_authority_json: &str,
    db_time_ms: i64,
) -> OriginalSourceVerificationResult<
    Option<(JobDiscoveryEvidence, OriginalSourceVerificationExpectedHead)>,
> {
    let subject_sha256 = original_source_subject_sha256(posting)?;
    if head.subject_sha256 != subject_sha256 {
        return Ok(None);
    }
    if head.checked_at_ms > head.expires_at_ms {
        return Err(OriginalSourceVerificationError::Storage(
            "current original-source head is inconsistent with the job subject".to_string(),
        ));
    }
    if head.assurance != "original_verified"
        || head.result != "verified_open"
        || head.expires_at_ms <= db_time_ms
        || head.managed_authority_sha256 != current_authority_sha256
        || head.canonical_managed_authority_json != current_authority_json
        || head.canonical_application_url.is_none()
        || head.application_domain.is_none()
        || head.assignment_state != "idle"
    {
        return Ok(None);
    }
    let provider = original_source_provider(&posting.source).ok_or_else(|| {
        OriginalSourceVerificationError::InvalidInput(
            "job source is not a supported verified import".to_string(),
        )
    })?;
    let (_, source_key) = canonical_public_discovery_url(provider, &posting.canonical_url)
        .map_err(|_| {
            OriginalSourceVerificationError::InvalidInput(
                "job URL is not a supported original source".to_string(),
            )
        })?;
    let employer_id = format!(
        "original-source-employer-{}",
        &original_source_sha256(format!("{provider}:{source_key}").as_bytes())[..32]
    );
    let application_domain = head.application_domain.clone();
    let evidence = JobDiscoveryEvidence {
        provenance: "original_source".to_string(),
        canonical_status: "canonical".to_string(),
        canonical_job_id: Some(posting.canonical_key.clone()),
        employer_verification_status: "ats_tenant_verified".to_string(),
        employer_id: Some(employer_id),
        canonical_employer_domain: application_domain.clone(),
        application_domain,
        scam_risk_status: "source_screened".to_string(),
        scam_signals: Vec::new(),
        original_source_status: "verified_open".to_string(),
        original_source_checked_at_ms: Some(head.checked_at_ms),
        original_source_snapshot_expires_at_ms: Some(head.expires_at_ms),
        original_source_evidence_hash: Some(head.receipt_sha256.clone()),
        original_source_mismatched_fields: Vec::new(),
        requires_original_revalidation: false,
    };
    let expected_head = OriginalSourceVerificationExpectedHead {
        subject_sha256: head.subject_sha256.clone(),
        material_generation: head.material_generation,
        material_sha256: head.material_sha256.clone(),
        receipt_sha256: head.receipt_sha256.clone(),
        expires_at_ms: head.expires_at_ms,
        managed_authority_sha256: head.managed_authority_sha256.clone(),
    };
    Ok(Some((evidence, expected_head)))
}

pub(crate) fn resolve_original_source_verification_projection_sqlite_tx(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    posting: &JobPosting,
) -> OriginalSourceVerificationResult<OriginalSourceVerificationProjection> {
    let authority = sqlite_original_source_verification_authority_for_account_tx(tx, account_id)
        .map_err(original_source_storage)?;
    let feature_active = authority.is_some();
    let db_time_ms = original_source_db_now_sqlite(tx)?;
    let Some(authority) = authority else {
        return Ok(OriginalSourceVerificationProjection {
            feature_active,
            evidence: None,
            expected_head: None,
            db_time_ms,
        });
    };
    require_original_source_membership_sqlite(tx, account_id, posting)?;
    let (_, canonical_authority_json, authority_sha256) =
        original_source_managed_authority_binding(&authority)?;
    let projected =
        resolve_original_source_verification_head_sqlite_tx(tx, account_id, &posting.id)?
            .map(|head| {
                original_source_positive_projection(
                    posting,
                    &head,
                    &authority_sha256,
                    &canonical_authority_json,
                    db_time_ms,
                )
            })
            .transpose()?
            .flatten();
    Ok(OriginalSourceVerificationProjection {
        feature_active,
        evidence: projected.as_ref().map(|value| value.0.clone()),
        expected_head: projected.map(|value| value.1),
        db_time_ms,
    })
}

pub(crate) fn resolve_original_source_verification_projection_postgres_tx(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    posting: &JobPosting,
) -> OriginalSourceVerificationResult<OriginalSourceVerificationProjection> {
    let authority = postgres_original_source_verification_authority_for_account_tx(tx, account_id)
        .map_err(original_source_storage)?;
    let feature_active = authority.is_some();
    let db_time_ms = original_source_db_now_postgres(tx)?;
    let Some(authority) = authority else {
        return Ok(OriginalSourceVerificationProjection {
            feature_active,
            evidence: None,
            expected_head: None,
            db_time_ms,
        });
    };
    require_original_source_membership_postgres(tx, account_id, posting)?;
    let (_, canonical_authority_json, authority_sha256) =
        original_source_managed_authority_binding(&authority)?;
    let projected =
        resolve_original_source_verification_head_postgres_tx(tx, account_id, &posting.id)?
            .map(|head| {
                original_source_positive_projection(
                    posting,
                    &head,
                    &authority_sha256,
                    &canonical_authority_json,
                    db_time_ms,
                )
            })
            .transpose()?
            .flatten();
    Ok(OriginalSourceVerificationProjection {
        feature_active,
        evidence: projected.as_ref().map(|value| value.0.clone()),
        expected_head: projected.map(|value| value.1),
        db_time_ms,
    })
}

pub fn resolve_original_source_verification_projection(
    pool: &DbPool,
    account_id: &str,
    posting: &JobPosting,
) -> OriginalSourceVerificationResult<OriginalSourceVerificationProjection> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get().map_err(original_source_storage)?;
            let tx = conn
                .transaction_with_behavior(TransactionBehavior::Deferred)
                .map_err(original_source_storage)?;
            let result = resolve_original_source_verification_projection_sqlite_tx(
                &tx, account_id, posting,
            )?;
            tx.commit().map_err(original_source_storage)?;
            Ok(result)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg().map_err(original_source_storage)?;
            let mut tx = conn.transaction().map_err(original_source_storage)?;
            let result = resolve_original_source_verification_projection_postgres_tx(
                &mut tx, account_id, posting,
            )?;
            tx.commit().map_err(original_source_storage)?;
            Ok(result)
        }
    })
}

pub fn resolve_original_source_verification_evidence(
    pool: &DbPool,
    account_id: &str,
    posting: &JobPosting,
    at_ms: i64,
) -> OriginalSourceVerificationResult<Option<JobDiscoveryEvidence>> {
    if posting.id.trim().is_empty() || at_ms < 0 {
        return Err(OriginalSourceVerificationError::InvalidInput(
            "evidence lookup identity or time is invalid".to_string(),
        ));
    }
    let projection = resolve_original_source_verification_projection(pool, account_id, posting)?;
    if projection
        .expected_head
        .as_ref()
        .is_some_and(|head| head.expires_at_ms <= at_ms)
    {
        return Ok(None);
    }
    Ok(projection.evidence)
}

#[derive(Debug, Clone, Serialize)]
struct NormalizedOriginalSourceObservation {
    assurance: String,
    result: String,
    error_code: Option<String>,
    evidence_sha256: String,
    requested_url: Option<String>,
    canonical_observed_url: Option<String>,
    canonical_application_url: Option<String>,
    application_domain: Option<String>,
    retrieval_status: String,
    http_status: Option<i64>,
    http_semantics_digest: String,
    redirect_chain_digest: String,
    headers_digest: String,
    content_digest: String,
    parser_version: String,
    parser_digest: String,
    worker_runtime_identity_sha256: String,
    provider_record_id: Option<String>,
    company: Option<String>,
    title: Option<String>,
    location: Option<String>,
    workplace: Option<String>,
    description: Option<String>,
    compensation: Option<String>,
    employment_type: Option<String>,
    posted_at_ms: Option<i64>,
    mismatched_fields: Vec<String>,
}

fn original_source_normalize_optional(value: &Option<String>) -> Option<String> {
    value
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn original_source_valid_application_domain(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 253
        && value == value.trim()
        && value == value.to_ascii_lowercase()
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'-')
        })
        && !value.starts_with('.')
        && !value.ends_with('.')
        && !value.contains("..")
}

fn original_source_normalize_observation(
    observation: &OriginalSourceVerificationObservation,
) -> OriginalSourceVerificationResult<NormalizedOriginalSourceObservation> {
    if observation.assurance != "original_verified"
        || !matches!(
            observation.result.as_str(),
            "open" | "closed" | "mismatch" | "quarantined" | "indeterminate"
        )
        || !original_source_valid_sha256(&observation.evidence_sha256)
        || !matches!(
            observation.retrieval_status.as_str(),
            "observed" | "absent" | "not_found" | "gone" | "unreachable" | "preflight_rejected"
        )
        || observation
            .http_status
            .is_some_and(|value| !(100..=599).contains(&value))
        || [
            &observation.http_semantics_digest,
            &observation.redirect_chain_digest,
            &observation.headers_digest,
            &observation.content_digest,
            &observation.parser_digest,
            &observation.worker_runtime_identity_sha256,
        ]
        .iter()
        .any(|digest| !original_source_valid_sha256(digest))
        || observation.parser_version.trim().is_empty()
        || observation.parser_version.len() > 128
        || observation.error_code.as_deref().is_some_and(|value| {
            value.trim().is_empty()
                || value.len() > 64
                || !value
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte == b'_')
        })
        || observation
            .posted_at_ms
            .is_some_and(|value| !(0..=9_007_199_254_740_991).contains(&value))
        || observation
            .canonical_application_url
            .as_deref()
            .is_some_and(|value| value != value.trim() || !(8..=4096).contains(&value.len()))
        || observation
            .application_domain
            .as_deref()
            .is_some_and(|value| !original_source_valid_application_domain(value))
        || observation.canonical_application_url.is_some()
            != observation.application_domain.is_some()
    {
        return Err(OriginalSourceVerificationError::InvalidInput(
            "verification observation authority is invalid".to_string(),
        ));
    }
    let mut mismatched_fields = observation
        .mismatched_fields
        .iter()
        .map(|value| value.trim().to_ascii_lowercase())
        .collect::<Vec<_>>();
    if mismatched_fields.iter().any(|value| {
        value.is_empty()
            || value.len() > 64
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte == b'_')
    }) {
        return Err(OriginalSourceVerificationError::InvalidInput(
            "mismatched fields are invalid".to_string(),
        ));
    }
    mismatched_fields.sort();
    mismatched_fields.dedup();
    if mismatched_fields.len() > 32
        || (observation.result == "mismatch" && mismatched_fields.is_empty())
        || (observation.result != "mismatch" && !mismatched_fields.is_empty())
    {
        return Err(OriginalSourceVerificationError::InvalidInput(
            "mismatched fields do not match the result".to_string(),
        ));
    }
    let normalized = NormalizedOriginalSourceObservation {
        assurance: observation.assurance.clone(),
        result: observation.result.clone(),
        error_code: observation
            .error_code
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_ascii_lowercase),
        evidence_sha256: observation.evidence_sha256.clone(),
        requested_url: original_source_normalize_optional(&observation.requested_url),
        canonical_observed_url: original_source_normalize_optional(
            &observation.canonical_observed_url,
        ),
        canonical_application_url: original_source_normalize_optional(
            &observation.canonical_application_url,
        ),
        application_domain: original_source_normalize_optional(&observation.application_domain),
        retrieval_status: observation.retrieval_status.clone(),
        http_status: observation.http_status,
        http_semantics_digest: observation.http_semantics_digest.clone(),
        redirect_chain_digest: observation.redirect_chain_digest.clone(),
        headers_digest: observation.headers_digest.clone(),
        content_digest: observation.content_digest.clone(),
        parser_version: observation.parser_version.trim().to_string(),
        parser_digest: observation.parser_digest.clone(),
        worker_runtime_identity_sha256: observation.worker_runtime_identity_sha256.clone(),
        provider_record_id: original_source_normalize_optional(&observation.provider_record_id),
        company: original_source_normalize_optional(&observation.company),
        title: original_source_normalize_optional(&observation.title),
        location: original_source_normalize_optional(&observation.location),
        workplace: original_source_normalize_optional(&observation.workplace),
        description: original_source_normalize_optional(&observation.description),
        compensation: original_source_normalize_optional(&observation.compensation),
        employment_type: original_source_normalize_optional(&observation.employment_type),
        posted_at_ms: observation.posted_at_ms,
        mismatched_fields,
    };
    Ok(normalized)
}

#[derive(Deserialize)]
struct OriginalSourceComparisonSubject {
    original_url: String,
    provider_family: String,
    provider_record_id: String,
    provider_target: OriginalSourceComparisonProviderTarget,
    expected: OriginalSourceComparisonExpected,
}

#[derive(Deserialize)]
struct OriginalSourceComparisonProviderTarget {
    host: String,
    tenant: String,
    job: String,
    variant: String,
}

#[derive(Deserialize)]
struct OriginalSourceComparisonExpected {
    company: String,
    title: String,
    location: String,
    workplace: String,
    description: String,
    compensation: String,
    employment_type: String,
    posted_at_ms: Option<i64>,
}

#[derive(Serialize)]
struct CanonicalOriginalSourceObservationBody<'a> {
    application_domain: &'a Option<String>,
    assurance: &'a str,
    canonical_application_url: &'a Option<String>,
    canonical_observed_url: &'a Option<String>,
    company: &'a Option<String>,
    compensation: &'a Option<String>,
    content_digest: &'a str,
    description: &'a Option<String>,
    employment_type: &'a Option<String>,
    error_code: &'a Option<String>,
    headers_digest: &'a str,
    http_semantics_digest: &'a str,
    http_status: Option<i64>,
    location: &'a Option<String>,
    mismatched_fields: &'a [String],
    parser_digest: &'a str,
    parser_version: &'a str,
    posted_at_ms: Option<i64>,
    provider_record_id: &'a Option<String>,
    redirect_chain_digest: &'a str,
    requested_url: &'a Option<String>,
    result: &'a str,
    retrieval_status: &'a str,
    title: &'a Option<String>,
    workplace: &'a Option<String>,
}

fn original_source_observation_evidence_sha256(
    observation: &NormalizedOriginalSourceObservation,
) -> OriginalSourceVerificationResult<String> {
    let canonical = original_source_canonical_observation_json(observation)?;
    let mut bytes = b"bluey-jobs-original-source-observation-v1\0".to_vec();
    bytes.extend_from_slice(canonical.as_bytes());
    Ok(original_source_sha256(bytes))
}

fn original_source_canonical_observation_json(
    observation: &NormalizedOriginalSourceObservation,
) -> OriginalSourceVerificationResult<String> {
    let body = CanonicalOriginalSourceObservationBody {
        application_domain: &observation.application_domain,
        assurance: &observation.assurance,
        canonical_application_url: &observation.canonical_application_url,
        canonical_observed_url: &observation.canonical_observed_url,
        company: &observation.company,
        compensation: &observation.compensation,
        content_digest: &observation.content_digest,
        description: &observation.description,
        employment_type: &observation.employment_type,
        error_code: &observation.error_code,
        headers_digest: &observation.headers_digest,
        http_semantics_digest: &observation.http_semantics_digest,
        http_status: observation.http_status,
        location: &observation.location,
        mismatched_fields: &observation.mismatched_fields,
        parser_digest: &observation.parser_digest,
        parser_version: &observation.parser_version,
        posted_at_ms: observation.posted_at_ms,
        provider_record_id: &observation.provider_record_id,
        redirect_chain_digest: &observation.redirect_chain_digest,
        requested_url: &observation.requested_url,
        result: &observation.result,
        retrieval_status: &observation.retrieval_status,
        title: &observation.title,
        workplace: &observation.workplace,
    };
    let mut canonical = serde_json::to_string(&body).map_err(original_source_storage)?;
    canonical.push('\n');
    Ok(canonical)
}

fn original_source_comparable(value: Option<&str>) -> String {
    value
        .unwrap_or_default()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .nfkc()
        .collect::<String>()
        .to_lowercase()
}

fn original_source_parser_metadata(provider_family: &str) -> (String, String) {
    let parser_version = format!("{provider_family}.original_source.v1");
    let parser_digest = original_source_sha256(
        format!("bluey-jobs-original-source-parser-v1\0{provider_family}\0{parser_version}\n")
            .as_bytes(),
    );
    (parser_version, parser_digest)
}

fn original_source_empty_retrieval_digest(kind: &str) -> String {
    original_source_sha256(format!("bluey-jobs-original-source-empty-{kind}-v1\0").as_bytes())
}

fn original_source_http_semantics_digest(requested_url: &str, status: i64) -> String {
    let mut canonical = serde_json::to_string(&json!({
        "method": "GET",
        "requested_url": requested_url,
        "status": status,
    }))
    .expect("closed HTTP semantics are serializable");
    canonical.push('\n');
    original_source_sha256(
        format!("bluey-jobs-original-source-http-semantics-v1\0{canonical}").as_bytes(),
    )
}

fn original_source_empty_redirect_chain_digest() -> String {
    original_source_sha256(b"bluey-jobs-original-source-redirect-chain-v1\0[]\n")
}

fn original_source_expected_requested_url(
    subject: &OriginalSourceComparisonSubject,
) -> OriginalSourceVerificationResult<String> {
    let target = &subject.provider_target;
    let variant_matches = match subject.provider_family.as_str() {
        "greenhouse" => target.variant == "greenhouse_public",
        "lever" => matches!(
            target.variant.as_str(),
            "lever_posting" | "lever_application"
        ),
        "ashby" => target.variant == "ashby_posting",
        "smartrecruiters" => target.variant == "smartrecruiters_posting",
        "workday" => target.variant == "workday_posting",
        _ => false,
    };
    if !variant_matches {
        return Err(OriginalSourceVerificationError::Storage(
            "canonical subject provider variant is inconsistent".to_string(),
        ));
    }
    let mut url = match subject.provider_family.as_str() {
        "greenhouse" => reqwest::Url::parse("https://boards-api.greenhouse.io/")
            .map_err(original_source_storage)?,
        "lever" => reqwest::Url::parse(if target.host == "jobs.eu.lever.co" {
            "https://api.eu.lever.co/"
        } else {
            "https://api.lever.co/"
        })
        .map_err(original_source_storage)?,
        "ashby" => {
            reqwest::Url::parse("https://api.ashbyhq.com/").map_err(original_source_storage)?
        }
        "smartrecruiters" => reqwest::Url::parse("https://api.smartrecruiters.com/")
            .map_err(original_source_storage)?,
        "workday" => reqwest::Url::parse(&format!("https://{}/", target.host))
            .map_err(original_source_storage)?,
        _ => {
            return Err(OriginalSourceVerificationError::Storage(
                "canonical subject has an unsupported provider family".to_string(),
            ));
        }
    };
    {
        let mut path = url.path_segments_mut().map_err(|_| {
            OriginalSourceVerificationError::Storage(
                "canonical subject provider request URL cannot be constructed".to_string(),
            )
        })?;
        path.clear();
        match subject.provider_family.as_str() {
            "greenhouse" => {
                path.extend(["v1", "boards", &target.tenant, "jobs", &target.job]);
            }
            "lever" => {
                path.extend(["v0", "postings", &target.tenant, &target.job]);
            }
            "ashby" => {
                path.extend(["posting-api", "job-board", &target.tenant]);
            }
            "smartrecruiters" => {
                path.extend(["v1", "companies", &target.tenant, "postings", &target.job]);
            }
            "workday" => {
                let original =
                    reqwest::Url::parse(&subject.original_url).map_err(original_source_storage)?;
                let segments = original
                    .path_segments()
                    .map(|segments| {
                        segments
                            .filter(|value| !value.is_empty())
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                let job_index = segments
                    .iter()
                    .position(|value| *value == "job")
                    .ok_or_else(|| {
                        OriginalSourceVerificationError::Storage(
                            "canonical Workday subject has no job path".to_string(),
                        )
                    })?;
                let url_record = segments.last().copied().unwrap_or_default();
                let record_matches = url_record == target.job
                    || url_record
                        .strip_suffix(&target.job)
                        .is_some_and(|prefix| prefix.ends_with('_'));
                if job_index < 2 || !record_matches {
                    return Err(OriginalSourceVerificationError::Storage(
                        "canonical Workday subject target is inconsistent".to_string(),
                    ));
                }
                path.extend(["wday", "cxs", &target.tenant, segments[1]]);
                path.extend(segments[job_index..].iter().copied());
            }
            _ => unreachable!("provider family was closed above"),
        }
    }
    match subject.provider_family.as_str() {
        "greenhouse" => {
            url.query_pairs_mut().append_pair("content", "true");
        }
        "lever" => {
            url.query_pairs_mut().append_pair("mode", "json");
        }
        _ => {}
    }
    Ok(url.to_string().trim_end_matches('/').to_string())
}

fn original_source_application_destination_is_valid(
    subject: &OriginalSourceComparisonSubject,
    observation: &NormalizedOriginalSourceObservation,
) -> bool {
    let (Some(application_url), Some(application_domain), Some(observed_url)) = (
        observation.canonical_application_url.as_deref(),
        observation.application_domain.as_deref(),
        observation.canonical_observed_url.as_deref(),
    ) else {
        return false;
    };
    let Ok(url) = reqwest::Url::parse(application_url) else {
        return false;
    };
    let Some(host) = url.host_str() else {
        return false;
    };
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
        || url.fragment().is_some()
        || url.query().is_some()
        || host != application_domain
        || host != host.to_ascii_lowercase()
        || url.as_str() != application_url
    {
        return false;
    }
    let segments = url
        .path_segments()
        .map(|segments| {
            segments
                .filter(|segment| !segment.is_empty())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let target = &subject.provider_target;
    match subject.provider_family.as_str() {
        "greenhouse" => {
            application_url == observed_url
                && host == target.host
                && segments.as_slice() == [target.tenant.as_str(), "jobs", target.job.as_str()]
        }
        "lever" => {
            host == target.host
                && segments.as_slice() == [target.tenant.as_str(), target.job.as_str(), "apply"]
        }
        "ashby" => {
            host == "jobs.ashbyhq.com"
                && segments.len() == 3
                && segments[0] == target.tenant
                && segments[1] == target.job
                && matches!(segments[2], "application" | "apply")
        }
        "smartrecruiters" => {
            host == "www.smartrecruiters.com"
                && segments.len() == 2
                && segments[0] == target.tenant
                && (segments[1] == target.job
                    || segments[1]
                        .strip_prefix(target.job.as_str())
                        .is_some_and(|suffix| suffix.starts_with('-') && suffix.len() > 1))
        }
        "workday" => {
            let record = segments.last().copied().unwrap_or_default();
            application_url == observed_url
                && host == target.host
                && segments.contains(&"job")
                && (record == target.job
                    || record
                        .strip_suffix(target.job.as_str())
                        .is_some_and(|prefix| prefix.ends_with('_')))
        }
        _ => false,
    }
}

fn original_source_observation_material_is_absent(
    observation: &NormalizedOriginalSourceObservation,
) -> bool {
    observation.canonical_observed_url.is_none()
        && observation.canonical_application_url.is_none()
        && observation.application_domain.is_none()
        && observation.provider_record_id.is_none()
        && observation.company.is_none()
        && observation.title.is_none()
        && observation.location.is_none()
        && observation.workplace.is_none()
        && observation.description.is_none()
        && observation.compensation.is_none()
        && observation.employment_type.is_none()
        && observation.posted_at_ms.is_none()
}

#[derive(Debug)]
struct OriginalSourceLeaseContext {
    account_id: String,
    job_id: String,
    assignment_generation: i64,
    assignment_sha256: String,
    subject_sha256: String,
    canonical_subject_json: String,
    attempt_no: i64,
}

fn original_source_assignment_identity_postgres(
    tx: &mut postgres::Transaction<'_>,
    assignment_id: &str,
) -> OriginalSourceVerificationResult<(String, String)> {
    tx.query_opt(
        "SELECT account_id,job_id
           FROM jobs_original_source_verification_assignments
          WHERE assignment_id=$1",
        &[&assignment_id],
    )
    .map_err(original_source_storage)?
    .map(|row| (row.get(0), row.get(1)))
    .ok_or(OriginalSourceVerificationError::LeaseLost)
}

fn original_source_attempt_identity_postgres(
    tx: &mut postgres::Transaction<'_>,
    assignment_id: &str,
    attempt_id: &str,
) -> OriginalSourceVerificationResult<(String, String)> {
    tx.query_opt(
        "SELECT assignment.account_id,assignment.job_id
           FROM jobs_original_source_verification_assignments assignment
           JOIN jobs_original_source_verification_attempts attempt
             ON attempt.assignment_id=assignment.assignment_id
            AND attempt.attempt_id=$2
          WHERE assignment.assignment_id=$1",
        &[&assignment_id, &attempt_id],
    )
    .map_err(original_source_storage)?
    .map(|row| (row.get(0), row.get(1)))
    .ok_or(OriginalSourceVerificationError::LeaseLost)
}

#[allow(clippy::too_many_arguments)]
fn original_source_validate_lease_values(
    state: &str,
    attempt_no: i64,
    active_attempt_id: Option<&str>,
    stored_lease_sha256: Option<&str>,
    lease_expires_at_ms: Option<i64>,
    hard_deadline_at_ms: Option<i64>,
    worker_id: &str,
    runtime_instance_id: &str,
    runtime_instance_epoch: i64,
    runtime_authority_sha256: &str,
    runtime_session_token_sha256: &str,
    request_attempt_id: &str,
    request_fence: i64,
    request_binding: &OriginalSourceVerifierBinding,
    request_lease_sha256: &str,
    request_session_sha256: &str,
    assignment_expires_at_ms: i64,
    now: i64,
) -> OriginalSourceVerificationResult<()> {
    if state != "leased"
        || attempt_no != request_fence
        || active_attempt_id != Some(request_attempt_id)
        || stored_lease_sha256 != Some(request_lease_sha256)
        || worker_id != request_binding.worker_id
        || runtime_instance_id != request_binding.runtime_instance_id
        || runtime_instance_epoch != request_binding.runtime_instance_epoch
        || runtime_authority_sha256 != request_binding.runtime_authority_sha256
        || runtime_session_token_sha256 != request_session_sha256
    {
        return Err(OriginalSourceVerificationError::LeaseLost);
    }
    let lease_expires_at_ms =
        lease_expires_at_ms.ok_or(OriginalSourceVerificationError::LeaseLost)?;
    let hard_deadline_at_ms =
        hard_deadline_at_ms.ok_or(OriginalSourceVerificationError::LeaseLost)?;
    if now >= lease_expires_at_ms || now >= hard_deadline_at_ms || now >= assignment_expires_at_ms {
        return Err(OriginalSourceVerificationError::LeaseExpired);
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn original_source_load_lease_sqlite(
    tx: &rusqlite::Transaction<'_>,
    assignment_id: &str,
    attempt_id: &str,
    fence: i64,
    binding: &OriginalSourceVerifierBinding,
    lease_sha256: &str,
    session_sha256: &str,
    now: i64,
) -> OriginalSourceVerificationResult<OriginalSourceLeaseContext> {
    let row = tx
        .query_row(
            "SELECT a.account_id, a.job_id, a.subject_sha256,
                    a.canonical_subject_json, a.state, a.attempt_count,
                    a.active_attempt_id, a.lease_token_sha256,
                    a.lease_expires_at_ms, a.hard_deadline_at_ms,
                    t.worker_id, t.runtime_instance_id, t.runtime_instance_epoch,
                    t.runtime_authority_sha256, t.runtime_session_token_sha256,
                    a.assignment_generation, a.assignment_sha256, a.expires_at_ms
               FROM jobs_original_source_verification_assignments a
               JOIN jobs_original_source_verification_attempts t
                 ON t.assignment_id = a.assignment_id AND t.attempt_id = ?2
              WHERE a.assignment_id = ?1",
            params![assignment_id, attempt_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, i64>(5)?,
                    row.get::<_, Option<String>>(6)?,
                    row.get::<_, Option<String>>(7)?,
                    row.get::<_, Option<i64>>(8)?,
                    row.get::<_, Option<i64>>(9)?,
                    row.get::<_, String>(10)?,
                    row.get::<_, String>(11)?,
                    row.get::<_, i64>(12)?,
                    row.get::<_, String>(13)?,
                    row.get::<_, String>(14)?,
                    row.get::<_, i64>(15)?,
                    row.get::<_, String>(16)?,
                    row.get::<_, i64>(17)?,
                ))
            },
        )
        .optional()
        .map_err(original_source_storage)?
        .ok_or(OriginalSourceVerificationError::LeaseLost)?;
    original_source_validate_lease_values(
        &row.4,
        row.5,
        row.6.as_deref(),
        row.7.as_deref(),
        row.8,
        row.9,
        &row.10,
        &row.11,
        row.12,
        &row.13,
        &row.14,
        attempt_id,
        fence,
        binding,
        lease_sha256,
        session_sha256,
        row.17,
        now,
    )?;
    Ok(OriginalSourceLeaseContext {
        account_id: row.0,
        job_id: row.1,
        assignment_generation: row.15,
        assignment_sha256: row.16,
        subject_sha256: row.2,
        canonical_subject_json: row.3,
        attempt_no: row.5,
    })
}

#[allow(clippy::too_many_arguments)]
fn original_source_load_lease_postgres(
    tx: &mut postgres::Transaction<'_>,
    assignment_id: &str,
    attempt_id: &str,
    expected_account_id: &str,
    expected_job_id: &str,
    fence: i64,
    binding: &OriginalSourceVerifierBinding,
    lease_sha256: &str,
    session_sha256: &str,
    now: i64,
) -> OriginalSourceVerificationResult<OriginalSourceLeaseContext> {
    let row = tx
        .query_opt(
            "SELECT a.account_id, a.job_id, a.subject_sha256,
                    a.canonical_subject_json, a.state, a.attempt_count,
                    a.active_attempt_id, a.lease_token_sha256,
                    a.lease_expires_at_ms, a.hard_deadline_at_ms,
                    t.worker_id, t.runtime_instance_id, t.runtime_instance_epoch,
                    t.runtime_authority_sha256, t.runtime_session_token_sha256,
                    a.assignment_generation, a.assignment_sha256, a.expires_at_ms
               FROM jobs_original_source_verification_assignments a
               JOIN jobs_original_source_verification_attempts t
                 ON t.assignment_id = a.assignment_id AND t.attempt_id = $2
              WHERE a.assignment_id = $1
                AND a.account_id = $3 AND a.job_id = $4
              FOR UPDATE OF a",
            &[
                &assignment_id,
                &attempt_id,
                &expected_account_id,
                &expected_job_id,
            ],
        )
        .map_err(original_source_storage)?
        .ok_or(OriginalSourceVerificationError::LeaseLost)?;
    original_source_validate_lease_values(
        &row.get::<_, String>(4),
        row.get(5),
        row.get::<_, Option<String>>(6).as_deref(),
        row.get::<_, Option<String>>(7).as_deref(),
        row.get(8),
        row.get(9),
        &row.get::<_, String>(10),
        &row.get::<_, String>(11),
        row.get(12),
        &row.get::<_, String>(13),
        &row.get::<_, String>(14),
        attempt_id,
        fence,
        binding,
        lease_sha256,
        session_sha256,
        row.get(17),
        now,
    )?;
    let account_id: String = row.get(0);
    let job_id: String = row.get(1);
    if account_id != expected_account_id || job_id != expected_job_id {
        return Err(OriginalSourceVerificationError::LeaseLost);
    }
    Ok(OriginalSourceLeaseContext {
        account_id,
        job_id,
        assignment_generation: row.get(15),
        assignment_sha256: row.get(16),
        subject_sha256: row.get(2),
        canonical_subject_json: row.get(3),
        attempt_no: row.get(5),
    })
}

fn original_source_request_sha256<T: Serialize>(
    kind: &str,
    assignment_id: &str,
    attempt_id: &str,
    fence: i64,
    request_id: &str,
    binding: &OriginalSourceVerifierBinding,
    payload: &T,
) -> OriginalSourceVerificationResult<String> {
    if !original_source_valid_id(request_id)
        || !original_source_valid_id(assignment_id)
        || !original_source_valid_id(attempt_id)
        || fence < 1
    {
        return Err(OriginalSourceVerificationError::InvalidInput(
            "terminal request identity is invalid".to_string(),
        ));
    }
    let canonical = serde_json::to_vec(&json!({
        "schema_version": 1,
        "kind": kind,
        "assignment_id": assignment_id,
        "attempt_id": attempt_id,
        "fence": fence,
        "request_id": request_id,
        "worker_id": binding.worker_id,
        "runtime_instance_id": binding.runtime_instance_id,
        "runtime_instance_epoch": binding.runtime_instance_epoch,
        "runtime_authority_sha256": binding.runtime_authority_sha256,
        "payload": payload,
    }))
    .map_err(original_source_storage)?;
    Ok(original_source_sha256(canonical))
}

fn original_source_terminal_state_from_result(result: &str) -> &'static str {
    match result {
        "source_untrusted"
        | "redirected_to_unknown"
        | "identity_mismatch"
        | "materially_changed" => "quarantined",
        "unreachable"
        | "rate_limited"
        | "auth_required"
        | "captcha_required"
        | "parse_ambiguous"
        | "provider_unavailable"
        | "expired"
        | "unknown" => "retry_wait",
        _ => "idle",
    }
}

fn original_source_quarantine_conflicting_replay_sqlite(
    tx: &rusqlite::Transaction<'_>,
    assignment_id: &str,
) -> OriginalSourceVerificationResult<()> {
    let state = tx
        .query_row(
            "SELECT state FROM jobs_original_source_verification_assignments
              WHERE assignment_id=?1",
            params![assignment_id],
            |row| row.get::<_, String>(0),
        )
        .map_err(original_source_storage)?;
    if state == "quarantined" {
        return Ok(());
    }
    let now = original_source_db_now_sqlite(tx)?;
    original_source_append_event_sqlite(
        tx,
        assignment_id,
        None,
        "quarantined",
        None,
        None,
        None,
        None,
        None,
        Some("invalid_assignment"),
        now,
    )?;
    tx.execute(
        "UPDATE jobs_original_source_verification_assignments SET
            state='quarantined', active_attempt_id=NULL, lease_owner=NULL,
            lease_token_sha256=NULL, lease_expires_at_ms=NULL,
            hard_deadline_at_ms=NULL, heartbeat_sequence=0,
            circuit_state='closed', circuit_open_until_ms=NULL,
            next_attempt_at_ms=?2, last_error_code='invalid_assignment',
            updated_at_ms=?2 WHERE assignment_id=?1",
        params![assignment_id, now],
    )
    .map_err(original_source_storage)?;
    Ok(())
}

fn original_source_quarantine_conflicting_replay_postgres(
    tx: &mut postgres::Transaction<'_>,
    assignment_id: &str,
    expected_account_id: &str,
    expected_job_id: &str,
) -> OriginalSourceVerificationResult<()> {
    let state: String = tx
        .query_opt(
            "SELECT state FROM jobs_original_source_verification_assignments
              WHERE assignment_id=$1 AND account_id=$2 AND job_id=$3
              FOR UPDATE",
            &[&assignment_id, &expected_account_id, &expected_job_id],
        )
        .map_err(original_source_storage)?
        .ok_or(OriginalSourceVerificationError::LeaseLost)?
        .get(0);
    if state == "quarantined" {
        return Ok(());
    }
    let now = original_source_db_now_postgres(tx)?;
    original_source_append_event_postgres(
        tx,
        assignment_id,
        None,
        "quarantined",
        None,
        None,
        None,
        None,
        None,
        Some("invalid_assignment"),
        now,
    )?;
    tx.execute(
        "UPDATE jobs_original_source_verification_assignments SET
            state='quarantined', active_attempt_id=NULL, lease_owner=NULL,
            lease_token_sha256=NULL, lease_expires_at_ms=NULL,
            hard_deadline_at_ms=NULL, heartbeat_sequence=0,
            circuit_state='closed', circuit_open_until_ms=NULL,
            next_attempt_at_ms=$2, last_error_code='invalid_assignment',
            updated_at_ms=$2 WHERE assignment_id=$1",
        &[&assignment_id, &now],
    )
    .map_err(original_source_storage)?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn original_source_terminal_replay_sqlite(
    tx: &rusqlite::Transaction<'_>,
    assignment_id: &str,
    attempt_id: &str,
    request_id: &str,
    request_sha256: &str,
    binding: &OriginalSourceVerifierBinding,
    lease_sha256: &str,
    session_sha256: &str,
) -> OriginalSourceVerificationResult<
    Option<OriginalSourceVerificationResult<OriginalSourceVerificationTerminal>>,
> {
    let row = tx
        .query_row(
            "SELECT e.completion_request_sha256, e.receipt_sha256,
                    e.attempt_id, t.worker_id, t.runtime_instance_id,
                    t.runtime_instance_epoch, t.runtime_authority_sha256,
                    t.runtime_session_token_sha256, t.lease_token_sha256,
                    a.account_id, a.job_id, a.state,
                    transition.head_revision, transition.material_generation,
                    transition.assignment_id, transition.receipt_id,
                    transition.receipt_sha256, transition.subject_sha256,
                    transition.material_sha256, transition.assurance,
                    transition.result, transition.checked_at_ms,
                    transition.expires_at_ms, receipt.canonical_application_url,
                    receipt.application_domain, receipt.managed_authority_sha256,
                    receipt.canonical_managed_authority_json
               FROM jobs_original_source_verification_events e
               JOIN jobs_original_source_verification_assignments a
                 ON a.assignment_id = e.assignment_id
               JOIN jobs_original_source_verification_attempts t
                 ON t.attempt_id = e.attempt_id AND t.assignment_id = e.assignment_id
               LEFT JOIN jobs_original_source_verification_transitions transition
                 ON transition.assignment_id=e.assignment_id
                AND transition.receipt_sha256=e.receipt_sha256
               LEFT JOIN jobs_original_source_verification_receipts receipt
                 ON receipt.assignment_id=transition.assignment_id
                AND receipt.receipt_id=transition.receipt_id
                AND receipt.receipt_sha256=transition.receipt_sha256
              WHERE e.assignment_id = ?1 AND e.completion_request_id = ?2
              LIMIT 1",
            params![assignment_id, request_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, i64>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, String>(7)?,
                    row.get::<_, String>(8)?,
                    row.get::<_, String>(9)?,
                    row.get::<_, String>(10)?,
                    row.get::<_, String>(11)?,
                    row.get::<_, Option<i64>>(12)?,
                    row.get::<_, Option<i64>>(13)?,
                    row.get::<_, Option<String>>(14)?,
                    row.get::<_, Option<String>>(15)?,
                    row.get::<_, Option<String>>(16)?,
                    row.get::<_, Option<String>>(17)?,
                    row.get::<_, Option<String>>(18)?,
                    row.get::<_, Option<String>>(19)?,
                    row.get::<_, Option<String>>(20)?,
                    row.get::<_, Option<i64>>(21)?,
                    row.get::<_, Option<i64>>(22)?,
                    row.get::<_, Option<String>>(23)?,
                    row.get::<_, Option<String>>(24)?,
                    row.get::<_, Option<String>>(25)?,
                    row.get::<_, Option<String>>(26)?,
                ))
            },
        )
        .optional()
        .map_err(original_source_storage)?;
    let Some(row) = row else { return Ok(None) };
    if row.3 != binding.worker_id
        || row.4 != binding.runtime_instance_id
        || row.5 != binding.runtime_instance_epoch
        || row.6 != binding.runtime_authority_sha256
        || row.7 != session_sha256
        || row.8 != lease_sha256
    {
        return Err(OriginalSourceVerificationError::LeaseLost);
    }
    if row.2 != attempt_id || row.0 != request_sha256 {
        original_source_quarantine_conflicting_replay_sqlite(tx, assignment_id)?;
        return Ok(Some(Err(
            OriginalSourceVerificationError::ConflictingReplay,
        )));
    }
    let head = match (
        row.12, row.13, row.14, row.15, row.16, row.17, row.18, row.19, row.20, row.21, row.22,
        row.25, row.26,
    ) {
        (
            Some(head_revision),
            Some(material_generation),
            Some(head_assignment_id),
            Some(receipt_id),
            Some(receipt_sha256),
            Some(subject_sha256),
            Some(material_sha256),
            Some(assurance),
            Some(result),
            Some(checked_at_ms),
            Some(expires_at_ms),
            Some(managed_authority_sha256),
            Some(canonical_managed_authority_json),
        ) => Some(OriginalSourceVerificationHead {
            account_id: row.9.clone(),
            job_id: row.10.clone(),
            head_revision,
            material_generation,
            assignment_id: head_assignment_id,
            receipt_id,
            receipt_sha256,
            subject_sha256,
            material_sha256,
            assurance,
            assignment_state: original_source_terminal_state_from_result(&result).to_string(),
            result,
            checked_at_ms,
            expires_at_ms,
            canonical_application_url: row.23,
            application_domain: row.24,
            managed_authority_sha256,
            canonical_managed_authority_json,
        }),
        _ if row.1.is_none() => None,
        _ => {
            return Err(OriginalSourceVerificationError::Storage(
                "terminal replay receipt has no exact transition".to_string(),
            ));
        }
    };
    let state = head
        .as_ref()
        .map_or(row.11, |head| head.assignment_state.clone());
    Ok(Some(Ok(OriginalSourceVerificationTerminal {
        assignment_id: assignment_id.to_string(),
        state,
        replayed: true,
        receipt_sha256: row.1,
        head,
    })))
}

#[allow(clippy::too_many_arguments)]
fn original_source_terminal_replay_postgres(
    tx: &mut postgres::Transaction<'_>,
    assignment_id: &str,
    attempt_id: &str,
    request_id: &str,
    request_sha256: &str,
    binding: &OriginalSourceVerifierBinding,
    lease_sha256: &str,
    session_sha256: &str,
) -> OriginalSourceVerificationResult<
    Option<OriginalSourceVerificationResult<OriginalSourceVerificationTerminal>>,
> {
    let row = tx
        .query_opt(
            "SELECT e.completion_request_sha256, e.receipt_sha256,
                    e.attempt_id, t.worker_id, t.runtime_instance_id,
                    t.runtime_instance_epoch, t.runtime_authority_sha256,
                    t.runtime_session_token_sha256, t.lease_token_sha256,
                    a.account_id, a.job_id, a.state,
                    transition.head_revision, transition.material_generation,
                    transition.assignment_id, transition.receipt_id,
                    transition.receipt_sha256, transition.subject_sha256,
                    transition.material_sha256, transition.assurance,
                    transition.result, transition.checked_at_ms,
                    transition.expires_at_ms, receipt.canonical_application_url,
                    receipt.application_domain, receipt.managed_authority_sha256,
                    receipt.canonical_managed_authority_json
               FROM jobs_original_source_verification_events e
               JOIN jobs_original_source_verification_assignments a
                 ON a.assignment_id = e.assignment_id
               JOIN jobs_original_source_verification_attempts t
                 ON t.attempt_id = e.attempt_id AND t.assignment_id = e.assignment_id
               LEFT JOIN jobs_original_source_verification_transitions transition
                 ON transition.assignment_id=e.assignment_id
                AND transition.receipt_sha256=e.receipt_sha256
               LEFT JOIN jobs_original_source_verification_receipts receipt
                 ON receipt.assignment_id=transition.assignment_id
                AND receipt.receipt_id=transition.receipt_id
                AND receipt.receipt_sha256=transition.receipt_sha256
              WHERE e.assignment_id = $1 AND e.completion_request_id = $2
              LIMIT 1",
            &[&assignment_id, &request_id],
        )
        .map_err(original_source_storage)?;
    let Some(row) = row else { return Ok(None) };
    if row.get::<_, String>(3) != binding.worker_id
        || row.get::<_, String>(4) != binding.runtime_instance_id
        || row.get::<_, i64>(5) != binding.runtime_instance_epoch
        || row.get::<_, String>(6) != binding.runtime_authority_sha256
        || row.get::<_, String>(7) != session_sha256
        || row.get::<_, String>(8) != lease_sha256
    {
        return Err(OriginalSourceVerificationError::LeaseLost);
    }
    if row.get::<_, String>(2) != attempt_id || row.get::<_, String>(0) != request_sha256 {
        return Ok(Some(Err(
            OriginalSourceVerificationError::ConflictingReplay,
        )));
    }
    let account_id: String = row.get(9);
    let job_id: String = row.get(10);
    let receipt_sha256: Option<String> = row.get(1);
    let head_revision: Option<i64> = row.get(12);
    let head = if let Some(head_revision) = head_revision {
        let result: String = row.get(20);
        Some(OriginalSourceVerificationHead {
            account_id,
            job_id,
            head_revision,
            material_generation: row.get(13),
            assignment_id: row.get(14),
            receipt_id: row.get(15),
            receipt_sha256: row.get(16),
            subject_sha256: row.get(17),
            material_sha256: row.get(18),
            assurance: row.get(19),
            assignment_state: original_source_terminal_state_from_result(&result).to_string(),
            result,
            checked_at_ms: row.get(21),
            expires_at_ms: row.get(22),
            canonical_application_url: row.get(23),
            application_domain: row.get(24),
            managed_authority_sha256: row.get(25),
            canonical_managed_authority_json: row.get(26),
        })
    } else if receipt_sha256.is_none() {
        None
    } else {
        return Err(OriginalSourceVerificationError::Storage(
            "terminal replay receipt has no exact transition".to_string(),
        ));
    };
    let state = head.as_ref().map_or_else(
        || row.get::<_, String>(11),
        |head| head.assignment_state.clone(),
    );
    Ok(Some(Ok(OriginalSourceVerificationTerminal {
        assignment_id: assignment_id.to_string(),
        state,
        replayed: true,
        receipt_sha256,
        head,
    })))
}

#[derive(Debug)]
struct OriginalSourcePreviousHead {
    revision: i64,
    transition_sha256: Option<String>,
    material_generation: i64,
    material_sha256: Option<String>,
    result: Option<String>,
}

fn original_source_previous_head_sqlite(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    job_id: &str,
) -> OriginalSourceVerificationResult<OriginalSourcePreviousHead> {
    tx.query_row(
        "SELECT head_revision, transition_sha256, material_generation,
                material_sha256, result
           FROM jobs_original_source_verification_heads
          WHERE account_id=?1 AND job_id=?2",
        params![account_id, job_id],
        |row| {
            Ok(OriginalSourcePreviousHead {
                revision: row.get(0)?,
                transition_sha256: row.get(1)?,
                material_generation: row.get(2)?,
                material_sha256: row.get(3)?,
                result: row.get(4)?,
            })
        },
    )
    .optional()
    .map_err(original_source_storage)
    .map(|value| {
        value.unwrap_or(OriginalSourcePreviousHead {
            revision: 0,
            transition_sha256: None,
            material_generation: 0,
            material_sha256: None,
            result: None,
        })
    })
}

fn original_source_previous_head_postgres(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    job_id: &str,
) -> OriginalSourceVerificationResult<OriginalSourcePreviousHead> {
    tx.query_opt(
        "SELECT head_revision, transition_sha256, material_generation,
                material_sha256, result
           FROM jobs_original_source_verification_heads
          WHERE account_id=$1 AND job_id=$2 FOR UPDATE",
        &[&account_id, &job_id],
    )
    .map_err(original_source_storage)
    .map(|row| {
        row.map(|row| OriginalSourcePreviousHead {
            revision: row.get(0),
            transition_sha256: row.get(1),
            material_generation: row.get(2),
            material_sha256: row.get(3),
            result: row.get(4),
        })
        .unwrap_or(OriginalSourcePreviousHead {
            revision: 0,
            transition_sha256: None,
            material_generation: 0,
            material_sha256: None,
            result: None,
        })
    })
}

fn original_source_classification(
    previous: &OriginalSourcePreviousHead,
    result: &str,
    material_sha256: &str,
) -> (&'static str, i64) {
    if previous.revision == 0 {
        return ("initial", 1);
    }
    if previous.material_sha256.as_deref() == Some(material_sha256) {
        return ("unchanged", previous.material_generation);
    }
    let classification = match result {
        "closed" => "closed",
        "redirected_to_unknown" | "identity_mismatch" | "materially_changed" => "mismatch",
        "source_untrusted" => "quarantined",
        "verified_open" if previous.result.as_deref() == Some("closed") => "reopened",
        "unreachable"
        | "rate_limited"
        | "auth_required"
        | "captcha_required"
        | "parse_ambiguous"
        | "provider_unavailable"
        | "unknown"
        | "expired" => "indeterminate",
        _ => "material_change",
    };
    (
        classification,
        previous.material_generation.saturating_add(1),
    )
}

fn original_source_completion_receipt_status(
    observation: &NormalizedOriginalSourceObservation,
) -> &'static str {
    match observation.result.as_str() {
        "open" => "verified_open",
        "closed" => "closed",
        "mismatch" if observation.retrieval_status == "preflight_rejected" => "identity_mismatch",
        "mismatch"
            if observation
                .mismatched_fields
                .binary_search_by_key(&"original_url", String::as_str)
                .is_ok() =>
        {
            "redirected_to_unknown"
        }
        "mismatch"
            if observation
                .mismatched_fields
                .binary_search_by_key(&"provider_record_id", String::as_str)
                .is_ok() =>
        {
            "identity_mismatch"
        }
        "mismatch" => "materially_changed",
        "quarantined" => "source_untrusted",
        _ => "unknown",
    }
}

fn original_source_failure_receipt_status(error_code: &str) -> &'static str {
    match error_code {
        "auth_required" => "auth_required",
        "captcha_required" => "captcha_required",
        "parse_ambiguous" => "parse_ambiguous",
        "provider_unavailable" => "provider_unavailable",
        "rate_limited" => "rate_limited",
        "unreachable" => "unreachable",
        "source_untrusted" | "invalid_assignment" => "source_untrusted",
        _ => "unknown",
    }
}

pub fn complete_original_source_verification(
    pool: &DbPool,
    request: &OriginalSourceVerificationCompletionRequest,
) -> OriginalSourceVerificationResult<OriginalSourceVerificationTerminal> {
    let observation = original_source_normalize_observation(&request.observation)?;
    if observation.error_code.is_some()
        || matches!(observation.result.as_str(), "indeterminate" | "quarantined")
    {
        return Err(OriginalSourceVerificationError::InvalidInput(
            "completion observation carries a failure status".to_string(),
        ));
    }
    let request_sha256 = original_source_request_sha256(
        "complete",
        &request.assignment_id,
        &request.attempt_id,
        request.fence,
        &request.request_id,
        &request.binding,
        &observation,
    )?;
    let (session_sha256, lease_sha256) =
        original_source_binding_hashes(&request.binding, &request.lease_token)?;
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => original_source_complete_sqlite(
            pool,
            request,
            &observation,
            &request_sha256,
            &session_sha256,
            &lease_sha256,
            None,
        ),
        DbPool::Postgres(_) => original_source_complete_postgres(
            pool,
            request,
            &observation,
            &request_sha256,
            &session_sha256,
            &lease_sha256,
            None,
        ),
    })
}

#[allow(clippy::too_many_arguments)]
fn original_source_insert_observation_sqlite(
    tx: &rusqlite::Transaction<'_>,
    request: &OriginalSourceVerificationCompletionRequest,
    observation: &NormalizedOriginalSourceObservation,
    lease: &OriginalSourceLeaseContext,
    request_sha256: &str,
    session_sha256: &str,
    lease_sha256: &str,
    now: i64,
) -> OriginalSourceVerificationResult<(String, String)> {
    let observation_id = uuid::Uuid::new_v4().to_string();
    let observation_sha256 = observation.evidence_sha256.clone();
    let canonical_observation_json = original_source_canonical_observation_json(observation)?;
    tx.execute(
        "INSERT INTO jobs_original_source_verification_observations (
            observation_id, observation_sha256, assignment_id, attempt_id,
            account_id, job_id, subject_sha256, attempt_no, fence,
            completion_request_id, completion_request_sha256, worker_id,
            runtime_instance_id, runtime_instance_epoch, runtime_authority_sha256,
            runtime_session_token_sha256, lease_token_sha256, assurance, result,
            error_code, requested_url, canonical_observed_url,
            canonical_application_url, application_domain, provider_record_id,
            retrieval_status, http_status, http_semantics_digest,
            redirect_chain_digest, headers_digest, content_digest, parser_version,
            parser_digest, worker_runtime_identity_sha256,
            canonical_observation_json, observed_at_ms
         ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,
                   ?15,?16,?17,?18,?19,?20,?21,?22,?23,?24,?25,?26,?27,?28,
                   ?29,?30,?31,?32,?33,?34,?35,?36)",
        params![
            observation_id,
            observation_sha256,
            request.assignment_id,
            request.attempt_id,
            lease.account_id,
            lease.job_id,
            lease.subject_sha256,
            lease.attempt_no,
            request.fence,
            request.request_id,
            request_sha256,
            request.binding.worker_id,
            request.binding.runtime_instance_id,
            request.binding.runtime_instance_epoch,
            request.binding.runtime_authority_sha256,
            session_sha256,
            lease_sha256,
            observation.assurance,
            observation.result,
            observation.error_code,
            observation.requested_url,
            observation.canonical_observed_url,
            observation.canonical_application_url,
            observation.application_domain,
            observation.provider_record_id,
            observation.retrieval_status,
            observation.http_status,
            observation.http_semantics_digest,
            observation.redirect_chain_digest,
            observation.headers_digest,
            observation.content_digest,
            observation.parser_version,
            observation.parser_digest,
            observation.worker_runtime_identity_sha256,
            canonical_observation_json,
            now
        ],
    )
    .map_err(original_source_storage)?;
    Ok((observation_id, observation_sha256))
}

#[allow(clippy::too_many_arguments)]
fn original_source_insert_observation_postgres(
    tx: &mut postgres::Transaction<'_>,
    request: &OriginalSourceVerificationCompletionRequest,
    observation: &NormalizedOriginalSourceObservation,
    lease: &OriginalSourceLeaseContext,
    request_sha256: &str,
    session_sha256: &str,
    lease_sha256: &str,
    now: i64,
) -> OriginalSourceVerificationResult<(String, String)> {
    let observation_id = uuid::Uuid::new_v4().to_string();
    let observation_sha256 = observation.evidence_sha256.clone();
    let canonical_observation_json = original_source_canonical_observation_json(observation)?;
    tx.execute(
        "INSERT INTO jobs_original_source_verification_observations (
            observation_id, observation_sha256, assignment_id, attempt_id,
            account_id, job_id, subject_sha256, attempt_no, fence,
            completion_request_id, completion_request_sha256, worker_id,
            runtime_instance_id, runtime_instance_epoch, runtime_authority_sha256,
            runtime_session_token_sha256, lease_token_sha256, assurance, result,
            error_code, requested_url, canonical_observed_url,
            canonical_application_url, application_domain, provider_record_id,
            retrieval_status, http_status, http_semantics_digest,
            redirect_chain_digest, headers_digest, content_digest, parser_version,
            parser_digest, worker_runtime_identity_sha256,
            canonical_observation_json, observed_at_ms
         ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,
                   $15,$16,$17,$18,$19,$20,$21,$22,$23,$24,$25,$26,$27,$28,
                   $29,$30,$31,$32,$33,$34,$35,$36)",
        &[
            &observation_id,
            &observation_sha256,
            &request.assignment_id,
            &request.attempt_id,
            &lease.account_id,
            &lease.job_id,
            &lease.subject_sha256,
            &lease.attempt_no,
            &request.fence,
            &request.request_id,
            &request_sha256,
            &request.binding.worker_id,
            &request.binding.runtime_instance_id,
            &request.binding.runtime_instance_epoch,
            &request.binding.runtime_authority_sha256,
            &session_sha256,
            &lease_sha256,
            &observation.assurance,
            &observation.result,
            &observation.error_code,
            &observation.requested_url,
            &observation.canonical_observed_url,
            &observation.canonical_application_url,
            &observation.application_domain,
            &observation.provider_record_id,
            &observation.retrieval_status,
            &observation.http_status,
            &observation.http_semantics_digest,
            &observation.redirect_chain_digest,
            &observation.headers_digest,
            &observation.content_digest,
            &observation.parser_version,
            &observation.parser_digest,
            &observation.worker_runtime_identity_sha256,
            &canonical_observation_json,
            &now,
        ],
    )
    .map_err(original_source_storage)?;
    Ok((observation_id, observation_sha256))
}

fn original_source_complete_sqlite(
    pool: &DbPool,
    request: &OriginalSourceVerificationCompletionRequest,
    observation: &NormalizedOriginalSourceObservation,
    request_sha256: &str,
    session_sha256: &str,
    lease_sha256: &str,
    failure: Option<(&str, &str)>,
) -> OriginalSourceVerificationResult<OriginalSourceVerificationTerminal> {
    let mut conn = pool.get().map_err(original_source_storage)?;
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(original_source_storage)?;
    if let Some(replay) = original_source_terminal_replay_sqlite(
        &tx,
        &request.assignment_id,
        &request.attempt_id,
        &request.request_id,
        request_sha256,
        &request.binding,
        lease_sha256,
        session_sha256,
    )? {
        tx.commit().map_err(original_source_storage)?;
        return replay;
    }
    require_original_source_verifier_runtime_active_sqlite_tx(
        &tx,
        &request.binding.worker_id,
        &request.binding.runtime_instance_id,
        &request.binding.runtime_session_token,
        request.binding.runtime_instance_epoch,
        &request.binding.runtime_authority_sha256,
    )
    .map_err(|_| OriginalSourceVerificationError::ManagedRuntimeAuthorityUnavailable)?;
    let now = original_source_db_now_sqlite(&tx)?;
    let lease = original_source_load_lease_sqlite(
        &tx,
        &request.assignment_id,
        &request.attempt_id,
        request.fence,
        &request.binding,
        lease_sha256,
        session_sha256,
        now,
    )?;
    recheck_original_source_membership_sqlite(&tx, &lease)?;
    let current_authority = original_source_require_assignment_authority_sqlite(
        &tx,
        &request.assignment_id,
        &lease.account_id,
    )?;
    let runtime_grant_id = original_source_require_runtime_binding_sqlite(
        &tx,
        &request.binding,
        &current_authority.binding,
    )?;
    original_source_require_operational_hold_clear_sqlite(&tx, &lease.account_id, &lease.job_id)?;
    if observation.worker_runtime_identity_sha256
        != current_authority.binding.runtime_identity_sha256
    {
        return Err(OriginalSourceVerificationError::ManagedRuntimeAuthorityUnavailable);
    }
    original_source_validate_observation_subject(&lease.canonical_subject_json, observation)?;
    let receipt_status = failure
        .map(|(error_code, _)| original_source_failure_receipt_status(error_code))
        .unwrap_or_else(|| original_source_completion_receipt_status(observation));
    let (observation_id, observation_sha256) = original_source_insert_observation_sqlite(
        &tx,
        request,
        observation,
        &lease,
        request_sha256,
        session_sha256,
        lease_sha256,
        now,
    )?;
    let material_json = serde_json::to_string(observation).map_err(original_source_storage)?;
    let material_sha256 = original_source_sha256(material_json.as_bytes());
    let previous = original_source_previous_head_sqlite(&tx, &lease.account_id, &lease.job_id)?;
    let receipt_id = uuid::Uuid::new_v4().to_string();
    let expires_at_ms = if failure.is_some() {
        now.saturating_add(1)
    } else {
        now.saturating_add(ORIGINAL_SOURCE_RECEIPT_TTL_MS)
    };
    let managed_authority_json: Value =
        serde_json::from_str(&current_authority.canonical_json).map_err(original_source_storage)?;
    let subject_json: Value =
        serde_json::from_str(&lease.canonical_subject_json).map_err(original_source_storage)?;
    let canonical_receipt_json = serde_json::to_string(&json!({
        "version": 1,
        "audience": "bluey.jobs.original_source_verification_receipt.v1",
        "receipt_id": receipt_id,
        "assignment_id": request.assignment_id,
        "assignment_generation": lease.assignment_generation,
        "assignment_sha256": lease.assignment_sha256,
        "attempt_id": request.attempt_id,
        "attempt_no": lease.attempt_no,
        "fence": request.fence,
        "completion_request_id": request.request_id,
        "completion_request_sha256": request_sha256,
        "subject_sha256": lease.subject_sha256,
        "provider_family": subject_json["provider_family"],
        "provider_record_id": subject_json["provider_record_id"],
        "provider_target": subject_json["provider_target"],
        "original_url": subject_json["original_url"],
        "managed_authority_sha256": &current_authority.sha256,
        "managed_authority": managed_authority_json,
        "runtime": {
            "grant_id": &runtime_grant_id,
            "runtime_instance_id": request.binding.runtime_instance_id,
            "runtime_instance_epoch": request.binding.runtime_instance_epoch,
            "runtime_authority_sha256": request.binding.runtime_authority_sha256,
            "worker_runtime_identity_sha256": observation.worker_runtime_identity_sha256,
        },
        "observation_id": observation_id,
        "observation_sha256": observation_sha256,
        "observation_result": observation.result,
        "canonical_application_url": observation.canonical_application_url,
        "application_domain": observation.application_domain,
        "material_sha256": material_sha256,
        "status": receipt_status,
        "predecessor": {
            "head_revision": previous.revision,
            "transition_sha256": previous.transition_sha256,
            "material_generation": previous.material_generation,
            "material_sha256": previous.material_sha256,
        },
        "source_risk_status": "provider_observed",
        "employer_verification_status": "unverified",
        "scam_risk_status": "review_required",
        "execution_capability": "review_only",
        "checked_at_ms": now,
        "expires_at_ms": expires_at_ms,
    }))
    .map_err(original_source_storage)?;
    let receipt_sha256 = original_source_sha256(canonical_receipt_json.as_bytes());
    tx.execute(
        "INSERT INTO jobs_original_source_verification_receipts (
            receipt_id, receipt_sha256, assignment_id, attempt_id, account_id,
            job_id, subject_sha256, attempt_no, assignment_generation,
            assignment_sha256, fence, managed_authority_sha256,
            canonical_managed_authority_json, managed_environment, managed_region,
            managed_channel, managed_head_revision, managed_transition_sha256,
            managed_activation_sha256, managed_manifest_sha256,
            managed_source_protocol_schema_sha256, managed_runtime_identity_sha256,
            runtime_grant_id, runtime_instance_id, runtime_instance_epoch,
            runtime_authority_sha256, worker_runtime_identity_sha256,
            completion_request_id, completion_request_sha256, observation_id,
            observation_sha256, observation_result, canonical_application_url,
            application_domain, assurance, result,
            evidence_sha256, material_sha256, source_risk_status,
            employer_verification_status, scam_risk_status, execution_capability,
            canonical_receipt_json, checked_at_ms, expires_at_ms
         ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,
                   ?15,?16,?17,?18,?19,?20,?21,?22,?23,?24,?25,?26,?27,
                   ?28,?29,?30,?31,?32,?33,?34,?35,?36,?37,?38,?39,?40,
                   ?41,?42,?43,?44,?45)",
        params![
            receipt_id,
            receipt_sha256,
            request.assignment_id,
            request.attempt_id,
            lease.account_id,
            lease.job_id,
            lease.subject_sha256,
            lease.attempt_no,
            lease.assignment_generation,
            lease.assignment_sha256,
            request.fence,
            current_authority.sha256,
            current_authority.canonical_json,
            current_authority.binding.environment,
            current_authority.binding.region,
            current_authority.binding.channel,
            current_authority.binding.head_revision,
            current_authority.binding.transition_sha256,
            current_authority.binding.activation_sha256,
            current_authority.binding.manifest_sha256,
            current_authority.binding.source_protocol_schema_sha256,
            current_authority.binding.runtime_identity_sha256,
            runtime_grant_id,
            request.binding.runtime_instance_id,
            request.binding.runtime_instance_epoch,
            request.binding.runtime_authority_sha256,
            observation.worker_runtime_identity_sha256,
            request.request_id,
            request_sha256,
            observation_id,
            observation_sha256,
            observation.result,
            observation.canonical_application_url,
            observation.application_domain,
            observation.assurance,
            receipt_status,
            observation.evidence_sha256,
            material_sha256,
            "provider_observed",
            "unverified",
            "review_required",
            "review_only",
            canonical_receipt_json,
            now,
            expires_at_ms
        ],
    )
    .map_err(original_source_storage)?;
    let (classification, material_generation) =
        original_source_classification(&previous, receipt_status, &material_sha256);
    let head_revision = previous.revision.saturating_add(1);
    let transition_id = uuid::Uuid::new_v4().to_string();
    let canonical_transition = serde_json::to_string(&json!({
        "schema_version": 1,
        "transition_id": transition_id,
        "account_id": lease.account_id,
        "job_id": lease.job_id,
        "head_revision": head_revision,
        "previous_head_revision": previous.revision,
        "predecessor_transition_sha256": previous.transition_sha256,
        "material_generation": material_generation,
        "previous_material_generation": previous.material_generation,
        "classification": classification,
        "assignment_id": request.assignment_id,
        "receipt_id": receipt_id,
        "receipt_sha256": receipt_sha256,
        "subject_sha256": lease.subject_sha256,
        "material_sha256": material_sha256,
        "assurance": observation.assurance,
        "result": receipt_status,
        "checked_at_ms": now,
        "expires_at_ms": expires_at_ms,
    }))
    .map_err(original_source_storage)?;
    let transition_sha256 = original_source_sha256(canonical_transition.as_bytes());
    tx.execute(
        "INSERT INTO jobs_original_source_verification_transitions (
            transition_id, transition_sha256, account_id, job_id, head_revision,
            previous_head_revision, predecessor_transition_sha256,
            material_generation, previous_material_generation, classification,
            assignment_id, receipt_id, receipt_sha256, subject_sha256,
            material_sha256, assurance, result, checked_at_ms, expires_at_ms,
            created_at_ms
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10,
                   ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20)",
        params![
            transition_id,
            transition_sha256,
            lease.account_id,
            lease.job_id,
            head_revision,
            previous.revision,
            previous.transition_sha256,
            material_generation,
            previous.material_generation,
            classification,
            request.assignment_id,
            receipt_id,
            receipt_sha256,
            lease.subject_sha256,
            material_sha256,
            observation.assurance,
            receipt_status,
            now,
            expires_at_ms,
            now
        ],
    )
    .map_err(original_source_storage)?;
    original_source_advance_head_sqlite(
        &tx,
        &lease,
        &previous,
        head_revision,
        material_generation,
        &transition_id,
        &transition_sha256,
        &receipt_id,
        &receipt_sha256,
        &material_sha256,
        observation,
        receipt_status,
        now,
        expires_at_ms,
    )?;
    let prior_failures = if failure.is_some() {
        tx.query_row(
            "SELECT consecutive_failures FROM jobs_original_source_verification_assignments
              WHERE assignment_id=?1",
            params![request.assignment_id],
            |row| row.get::<_, i64>(0),
        )
        .map_err(original_source_storage)?
    } else {
        0
    };
    let failures = if failure.is_some() {
        prior_failures.saturating_add(1)
    } else {
        0
    };
    let (state, circuit_state, circuit_open_until_ms, next_attempt_at_ms, event_kind, reason_code) =
        if let Some((error_code, failure_class)) = failure {
            if failure_class == "unsafe" {
                (
                    "quarantined",
                    "closed",
                    None,
                    now,
                    "quarantined",
                    Some(error_code),
                )
            } else if failures >= ORIGINAL_SOURCE_CIRCUIT_FAILURE_THRESHOLD {
                let until = now.saturating_add(ORIGINAL_SOURCE_CIRCUIT_COOLDOWN_MS);
                (
                    "retry_wait",
                    "open",
                    Some(until),
                    until,
                    "circuit_opened",
                    Some(error_code),
                )
            } else {
                (
                    "retry_wait",
                    "closed",
                    None,
                    original_source_retry_at(&request.assignment_id, failures, now),
                    "failed",
                    Some(error_code),
                )
            }
        } else {
            let state = if matches!(
                receipt_status,
                "redirected_to_unknown"
                    | "identity_mismatch"
                    | "materially_changed"
                    | "source_untrusted"
            ) {
                "quarantined"
            } else {
                "idle"
            };
            (state, "closed", None, expires_at_ms, "verified", None)
        };
    original_source_append_event_sqlite(
        &tx,
        &request.assignment_id,
        Some(&request.attempt_id),
        event_kind,
        None,
        None,
        Some(&request.request_id),
        Some(request_sha256),
        Some(&receipt_sha256),
        reason_code,
        now,
    )?;
    tx.execute(
        "UPDATE jobs_original_source_verification_assignments SET
            state = ?2, active_attempt_id = NULL, lease_owner = NULL,
            lease_token_sha256 = NULL, lease_expires_at_ms = NULL,
            hard_deadline_at_ms = NULL, heartbeat_sequence = 0,
            consecutive_failures = ?3, circuit_state = ?4,
            circuit_open_until_ms = ?5, next_attempt_at_ms = ?6,
            last_error_code = ?7, updated_at_ms = ?8
          WHERE assignment_id = ?1",
        params![
            request.assignment_id,
            state,
            failures,
            circuit_state,
            circuit_open_until_ms,
            next_attempt_at_ms,
            reason_code,
            now
        ],
    )
    .map_err(original_source_storage)?;
    let head =
        resolve_original_source_verification_head_sqlite_tx(&tx, &lease.account_id, &lease.job_id)?;
    tx.commit().map_err(original_source_storage)?;
    Ok(OriginalSourceVerificationTerminal {
        assignment_id: request.assignment_id.clone(),
        state: state.to_string(),
        replayed: false,
        receipt_sha256: Some(receipt_sha256),
        head,
    })
}

fn original_source_validate_observation_subject(
    canonical_subject_json: &str,
    observation: &NormalizedOriginalSourceObservation,
) -> OriginalSourceVerificationResult<()> {
    let subject: OriginalSourceComparisonSubject =
        serde_json::from_str(canonical_subject_json).map_err(original_source_storage)?;
    let expected_failure_result = observation.error_code.as_deref().map(|error_code| {
        if matches!(error_code, "invalid_assignment" | "source_untrusted") {
            "quarantined"
        } else {
            "indeterminate"
        }
    });
    if expected_failure_result.is_some_and(|result| observation.result != result)
        || (expected_failure_result.is_none()
            && matches!(observation.result.as_str(), "quarantined" | "indeterminate"))
    {
        return Err(OriginalSourceVerificationError::InvalidInput(
            "observation error code and result are inconsistent".to_string(),
        ));
    }
    let parser_matches = [subject.provider_family.as_str(), "assignment"]
        .into_iter()
        .filter(|family| {
            *family != "assignment"
                || observation.error_code.as_deref() == Some("invalid_assignment")
        })
        .any(|family| {
            let (version, digest) = original_source_parser_metadata(family);
            observation.parser_version == version && observation.parser_digest == digest
        });
    if !parser_matches {
        return Err(OriginalSourceVerificationError::InvalidInput(
            "observation parser authority differs from the server protocol".to_string(),
        ));
    }
    let expected_requested_url = original_source_expected_requested_url(&subject)?;
    let empty_digests = [
        (
            observation.http_semantics_digest.as_str(),
            original_source_empty_retrieval_digest("http-semantics"),
        ),
        (
            observation.redirect_chain_digest.as_str(),
            original_source_empty_retrieval_digest("redirect-chain"),
        ),
        (
            observation.headers_digest.as_str(),
            original_source_empty_retrieval_digest("headers"),
        ),
        (
            observation.content_digest.as_str(),
            original_source_empty_retrieval_digest("content"),
        ),
    ];
    let has_exact_empty_digests = empty_digests
        .iter()
        .all(|(actual, expected)| *actual == expected);
    let prefetch_target_rejection = observation.result == "mismatch"
        && observation.mismatched_fields.len() == 1
        && observation.mismatched_fields[0] == "original_url"
        && observation.retrieval_status == "preflight_rejected"
        && observation.requested_url.is_none()
        && observation.http_status.is_none()
        && has_exact_empty_digests
        && original_source_observation_material_is_absent(observation);
    let preflight_unsafe_failure = observation.result == "quarantined"
        && matches!(
            observation.error_code.as_deref(),
            Some("invalid_assignment" | "source_untrusted")
        )
        && observation.mismatched_fields.is_empty()
        && observation.retrieval_status == "preflight_rejected"
        && observation.requested_url.is_none()
        && observation.http_status.is_none()
        && has_exact_empty_digests
        && original_source_observation_material_is_absent(observation);
    let preflight_terminal = prefetch_target_rejection || preflight_unsafe_failure;
    if !preflight_terminal {
        if observation.requested_url.as_deref() != Some(expected_requested_url.as_str()) {
            return Err(OriginalSourceVerificationError::InvalidInput(
                "observation request URL differs from the closed provider protocol".to_string(),
            ));
        }
        let status_valid = match observation.retrieval_status.as_str() {
            "observed" => observation.http_status.is_some(),
            "absent" => observation
                .http_status
                .is_some_and(|status| (200..300).contains(&status)),
            "not_found" => observation.http_status == Some(404),
            "gone" => observation.http_status == Some(410),
            "unreachable" => observation.http_status.is_none() && has_exact_empty_digests,
            "preflight_rejected" => false,
            _ => false,
        };
        if !status_valid {
            return Err(OriginalSourceVerificationError::InvalidInput(
                "observation retrieval and HTTP status are inconsistent".to_string(),
            ));
        }
        if observation.retrieval_status != "unreachable" {
            let status = observation.http_status.ok_or_else(|| {
                OriginalSourceVerificationError::InvalidInput(
                    "network observation has no HTTP status".to_string(),
                )
            })?;
            if observation.http_semantics_digest
                != original_source_http_semantics_digest(&expected_requested_url, status)
                || observation.redirect_chain_digest
                    != original_source_empty_redirect_chain_digest()
            {
                return Err(OriginalSourceVerificationError::InvalidInput(
                    "observation HTTP semantics differ from the closed GET/no-redirect protocol"
                        .to_string(),
                ));
            }
        }
        if observation.error_code.is_none()
            && matches!(observation.result.as_str(), "open" | "mismatch")
            && (observation.retrieval_status != "observed"
                || !observation
                    .http_status
                    .is_some_and(|status| (200..300).contains(&status)))
        {
            return Err(OriginalSourceVerificationError::InvalidInput(
                "positive or material source observations require an observed 2xx response"
                    .to_string(),
            ));
        }
    }
    let observed_status_in = |range: std::ops::RangeInclusive<i64>| {
        observation.retrieval_status == "observed"
            && observation
                .http_status
                .is_some_and(|status| range.contains(&status))
    };
    let protocol_outcome_valid = match (
        observation.error_code.as_deref(),
        observation.result.as_str(),
    ) {
        (None, "open" | "mismatch") => prefetch_target_rejection || observed_status_in(200..=299),
        (None, "closed") => {
            matches!(
                (
                    observation.retrieval_status.as_str(),
                    observation.http_status
                ),
                ("not_found", Some(404)) | ("gone", Some(410))
            ) || (observation.retrieval_status == "absent"
                && observation
                    .http_status
                    .is_some_and(|status| (200..=299).contains(&status)))
        }
        (Some("invalid_assignment"), "quarantined") => preflight_unsafe_failure,
        (Some("source_untrusted"), "quarantined") => {
            preflight_unsafe_failure
                || (observation.retrieval_status == "observed"
                    && observation.http_status.is_some_and(|status| {
                        !(200..=299).contains(&status)
                            && status < 500
                            && !matches!(status, 401 | 403 | 404 | 410 | 429)
                    }))
        }
        (Some("unreachable"), "indeterminate") => {
            observation.retrieval_status == "unreachable"
                && observation.http_status.is_none()
                && has_exact_empty_digests
        }
        (Some("auth_required"), "indeterminate") => {
            observation.retrieval_status == "observed"
                && matches!(observation.http_status, Some(401 | 403))
        }
        (Some("rate_limited"), "indeterminate") => {
            observation.retrieval_status == "observed" && observation.http_status == Some(429)
        }
        (Some("provider_unavailable"), "indeterminate") => observed_status_in(500..=599),
        (Some("captcha_required"), "indeterminate") => observed_status_in(200..=299),
        // The bounded response reader runs before status classification, so an
        // oversized or otherwise unreadable response can be parse-ambiguous for
        // any real HTTP response. It can never be positive or hard-closed.
        (Some("parse_ambiguous"), "indeterminate") => {
            observation.retrieval_status == "observed" && observation.http_status.is_some()
        }
        _ => false,
    };
    if !protocol_outcome_valid {
        return Err(OriginalSourceVerificationError::InvalidInput(
            "observation result, retrieval status, HTTP status, and error code differ from the closed provider protocol"
                .to_string(),
        ));
    }
    if !preflight_terminal
        && matches!(
            observation.result.as_str(),
            "closed" | "quarantined" | "indeterminate"
        )
    {
        if !observation.mismatched_fields.is_empty()
            || !original_source_observation_material_is_absent(observation)
        {
            return Err(OriginalSourceVerificationError::InvalidInput(
                "negative observation carries unbound source material".to_string(),
            ));
        }
    } else if !preflight_terminal {
        if !original_source_application_destination_is_valid(&subject, observation) {
            return Err(OriginalSourceVerificationError::InvalidInput(
                "observation application destination differs from the frozen provider target"
                    .to_string(),
            ));
        }
        let mut derived_mismatches = Vec::new();
        if observation.canonical_observed_url.as_deref() != Some(subject.original_url.as_str()) {
            derived_mismatches.push("original_url".to_string());
        }
        if observation.provider_record_id.as_deref() != Some(subject.provider_record_id.as_str()) {
            derived_mismatches.push("provider_record_id".to_string());
        }
        let required_comparisons = [
            (
                "company",
                subject.expected.company.as_str(),
                observation.company.as_deref(),
            ),
            (
                "title",
                subject.expected.title.as_str(),
                observation.title.as_deref(),
            ),
            (
                "location",
                subject.expected.location.as_str(),
                observation.location.as_deref(),
            ),
            (
                "workplace",
                subject.expected.workplace.as_str(),
                observation.workplace.as_deref(),
            ),
        ];
        derived_mismatches.extend(required_comparisons.into_iter().filter_map(
            |(field, expected, observed)| {
                (original_source_comparable(Some(expected)) != original_source_comparable(observed))
                    .then_some(field.to_string())
            },
        ));
        let expected_description =
            original_source_comparable(Some(subject.expected.description.as_str()));
        let observed_description = original_source_comparable(observation.description.as_deref());
        let description_matches = match subject.provider_family.as_str() {
            "workday" => {
                !expected_description.is_empty()
                    && observed_description.starts_with(&expected_description)
            }
            "smartrecruiters" if expected_description.is_empty() => true,
            _ => expected_description == observed_description,
        };
        if !description_matches {
            derived_mismatches.push("description".to_string());
        }
        for (field, expected, observed) in [
            (
                "compensation",
                subject.expected.compensation.as_str(),
                observation.compensation.as_deref(),
            ),
            (
                "employment_type",
                subject.expected.employment_type.as_str(),
                observation.employment_type.as_deref(),
            ),
        ] {
            let expected = original_source_comparable(Some(expected));
            if !expected.is_empty() && expected != original_source_comparable(observed) {
                derived_mismatches.push(field.to_string());
            }
        }
        if subject
            .expected
            .posted_at_ms
            .is_some_and(|expected| Some(expected) != observation.posted_at_ms)
        {
            derived_mismatches.push("posted_at_ms".to_string());
        }
        derived_mismatches.sort();
        let derived_result = if derived_mismatches.is_empty() {
            "open"
        } else {
            "mismatch"
        };
        if observation.result != derived_result
            || observation.mismatched_fields != derived_mismatches
        {
            return Err(OriginalSourceVerificationError::InvalidInput(
                "observation result or mismatch set differs from server comparison".to_string(),
            ));
        }
    }
    let expected_evidence_sha256 = original_source_observation_evidence_sha256(observation)?;
    if !bool::from(
        observation
            .evidence_sha256
            .as_bytes()
            .ct_eq(expected_evidence_sha256.as_bytes()),
    ) {
        return Err(OriginalSourceVerificationError::InvalidInput(
            "observation evidence hash differs from server canonical bytes".to_string(),
        ));
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn original_source_advance_head_sqlite(
    tx: &rusqlite::Transaction<'_>,
    lease: &OriginalSourceLeaseContext,
    previous: &OriginalSourcePreviousHead,
    head_revision: i64,
    material_generation: i64,
    transition_id: &str,
    transition_sha256: &str,
    receipt_id: &str,
    receipt_sha256: &str,
    material_sha256: &str,
    observation: &NormalizedOriginalSourceObservation,
    receipt_status: &str,
    now: i64,
    expires_at_ms: i64,
) -> OriginalSourceVerificationResult<()> {
    let changed = if previous.revision == 0 {
        tx.execute(
            "INSERT INTO jobs_original_source_verification_heads (
                account_id, job_id, head_revision, transition_id,
                transition_sha256, material_generation, assignment_id,
                receipt_id, receipt_sha256, subject_sha256, material_sha256,
                assurance, result, checked_at_ms, expires_at_ms, updated_at_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10,
                       ?11, ?12, ?13, ?14, ?15, ?14)",
            params![
                lease.account_id,
                lease.job_id,
                head_revision,
                transition_id,
                transition_sha256,
                material_generation,
                // Assignment id is present in the subject transition; resolve
                // it through the freshly inserted receipt to avoid a caller field.
                tx.query_row(
                    "SELECT assignment_id FROM jobs_original_source_verification_receipts
                      WHERE receipt_id = ?1",
                    params![receipt_id],
                    |row| row.get::<_, String>(0)
                )
                .map_err(original_source_storage)?,
                receipt_id,
                receipt_sha256,
                lease.subject_sha256,
                material_sha256,
                observation.assurance,
                receipt_status,
                now,
                expires_at_ms
            ],
        )
        .map_err(original_source_storage)?
    } else {
        tx.execute(
            "UPDATE jobs_original_source_verification_heads SET
                head_revision = ?4, transition_id = ?5,
                transition_sha256 = ?6, material_generation = ?7,
                assignment_id = (SELECT assignment_id
                  FROM jobs_original_source_verification_receipts
                  WHERE receipt_id = ?8),
                receipt_id = ?8, receipt_sha256 = ?9,
                subject_sha256 = ?10, material_sha256 = ?11,
                assurance = ?12, result = ?13, checked_at_ms = ?14,
                expires_at_ms = ?15, updated_at_ms = ?14
              WHERE account_id = ?1 AND job_id = ?2 AND head_revision = ?3
                AND transition_sha256 = ?16",
            params![
                lease.account_id,
                lease.job_id,
                previous.revision,
                head_revision,
                transition_id,
                transition_sha256,
                material_generation,
                receipt_id,
                receipt_sha256,
                lease.subject_sha256,
                material_sha256,
                observation.assurance,
                receipt_status,
                now,
                expires_at_ms,
                previous.transition_sha256
            ],
        )
        .map_err(original_source_storage)?
    };
    if changed != 1 {
        return Err(OriginalSourceVerificationError::ConcurrentHeadAdvance);
    }
    Ok(())
}

fn original_source_complete_postgres(
    pool: &DbPool,
    request: &OriginalSourceVerificationCompletionRequest,
    observation: &NormalizedOriginalSourceObservation,
    request_sha256: &str,
    session_sha256: &str,
    lease_sha256: &str,
    failure: Option<(&str, &str)>,
) -> OriginalSourceVerificationResult<OriginalSourceVerificationTerminal> {
    let mut conn = pool.get_pg().map_err(original_source_storage)?;
    let mut tx = conn.transaction().map_err(original_source_storage)?;
    if let Some(replay) = original_source_terminal_replay_postgres(
        &mut tx,
        &request.assignment_id,
        &request.attempt_id,
        &request.request_id,
        request_sha256,
        &request.binding,
        lease_sha256,
        session_sha256,
    )? {
        if matches!(
            &replay,
            Err(OriginalSourceVerificationError::ConflictingReplay)
        ) {
            // Exact response-loss replay remains authority-independent above. A changed-byte
            // replay mutates assignment state, so it must enter the canonical H -> M -> D ->
            // assignment order before quarantining the authenticated conflict.
            lock_operational_hold_shared_postgres_tx(&mut tx).map_err(original_source_storage)?;
            lock_managed_cloud_release_registry_shared_postgres_tx(&mut tx)
                .map_err(original_source_storage)?;
            let (account_id, job_id) =
                original_source_assignment_identity_postgres(&mut tx, &request.assignment_id)?;
            lock_discovery_account_postgres(&mut tx, &account_id)
                .map_err(original_source_storage)?;
            original_source_quarantine_conflicting_replay_postgres(
                &mut tx,
                &request.assignment_id,
                &account_id,
                &job_id,
            )?;
            tx.commit().map_err(original_source_storage)?;
            return Err(OriginalSourceVerificationError::ConflictingReplay);
        }
        tx.commit().map_err(original_source_storage)?;
        return replay;
    }
    // Preserve the global PostgreSQL authority lock order for a new publication:
    // operational holds (H) precede managed runtime/release authority (M), which
    // precedes discovery/source membership (D). Exact terminal replay above is
    // intentionally response-loss recoverable without re-evaluating a later hold.
    lock_operational_hold_shared_postgres_tx(&mut tx).map_err(original_source_storage)?;
    require_original_source_verifier_runtime_active_postgres_tx(
        &mut tx,
        &request.binding.worker_id,
        &request.binding.runtime_instance_id,
        &request.binding.runtime_session_token,
        request.binding.runtime_instance_epoch,
        &request.binding.runtime_authority_sha256,
    )
    .map_err(|_| OriginalSourceVerificationError::ManagedRuntimeAuthorityUnavailable)?;
    let now = original_source_db_now_postgres(&mut tx)?;
    let (expected_account_id, expected_job_id) = original_source_attempt_identity_postgres(
        &mut tx,
        &request.assignment_id,
        &request.attempt_id,
    )?;
    lock_discovery_account_postgres(&mut tx, &expected_account_id)
        .map_err(original_source_storage)?;
    let lease = match original_source_load_lease_postgres(
        &mut tx,
        &request.assignment_id,
        &request.attempt_id,
        &expected_account_id,
        &expected_job_id,
        request.fence,
        &request.binding,
        lease_sha256,
        session_sha256,
        now,
    ) {
        Ok(lease) => lease,
        Err(OriginalSourceVerificationError::LeaseLost) => {
            // A concurrent identical terminal call can publish while this
            // transaction waits for the assignment row. PostgreSQL READ
            // COMMITTED gives this statement a fresh snapshot, so recover the
            // exact committed terminal now. Changed bytes still take the
            // authenticated quarantine path in the replay helper.
            if let Some(replay) = original_source_terminal_replay_postgres(
                &mut tx,
                &request.assignment_id,
                &request.attempt_id,
                &request.request_id,
                request_sha256,
                &request.binding,
                lease_sha256,
                session_sha256,
            )? {
                if matches!(
                    &replay,
                    Err(OriginalSourceVerificationError::ConflictingReplay)
                ) {
                    original_source_quarantine_conflicting_replay_postgres(
                        &mut tx,
                        &request.assignment_id,
                        &expected_account_id,
                        &expected_job_id,
                    )?;
                    tx.commit().map_err(original_source_storage)?;
                    return Err(OriginalSourceVerificationError::ConflictingReplay);
                }
                tx.commit().map_err(original_source_storage)?;
                return replay;
            }
            return Err(OriginalSourceVerificationError::LeaseLost);
        }
        Err(error) => return Err(error),
    };
    recheck_original_source_membership_postgres(&mut tx, &lease)?;
    let current_authority = original_source_require_assignment_authority_postgres(
        &mut tx,
        &request.assignment_id,
        &lease.account_id,
    )?;
    let runtime_grant_id = original_source_require_runtime_binding_postgres(
        &mut tx,
        &request.binding,
        &current_authority.binding,
    )?;
    original_source_require_operational_hold_clear_postgres(
        &mut tx,
        &lease.account_id,
        &lease.job_id,
    )?;
    if observation.worker_runtime_identity_sha256
        != current_authority.binding.runtime_identity_sha256
    {
        return Err(OriginalSourceVerificationError::ManagedRuntimeAuthorityUnavailable);
    }
    original_source_validate_observation_subject(&lease.canonical_subject_json, observation)?;
    let receipt_status = failure
        .map(|(error_code, _)| original_source_failure_receipt_status(error_code))
        .unwrap_or_else(|| original_source_completion_receipt_status(observation));
    let (observation_id, observation_sha256) = original_source_insert_observation_postgres(
        &mut tx,
        request,
        observation,
        &lease,
        request_sha256,
        session_sha256,
        lease_sha256,
        now,
    )?;
    let material_json = serde_json::to_string(observation).map_err(original_source_storage)?;
    let material_sha256 = original_source_sha256(material_json.as_bytes());
    let previous =
        original_source_previous_head_postgres(&mut tx, &lease.account_id, &lease.job_id)?;
    let receipt_id = uuid::Uuid::new_v4().to_string();
    let expires_at_ms = if failure.is_some() {
        now.saturating_add(1)
    } else {
        now.saturating_add(ORIGINAL_SOURCE_RECEIPT_TTL_MS)
    };
    let managed_authority_json: Value =
        serde_json::from_str(&current_authority.canonical_json).map_err(original_source_storage)?;
    let subject_json: Value =
        serde_json::from_str(&lease.canonical_subject_json).map_err(original_source_storage)?;
    let canonical_receipt_json = serde_json::to_string(&json!({
        "version": 1,
        "audience": "bluey.jobs.original_source_verification_receipt.v1",
        "receipt_id": receipt_id,
        "assignment_id": request.assignment_id,
        "assignment_generation": lease.assignment_generation,
        "assignment_sha256": lease.assignment_sha256,
        "attempt_id": request.attempt_id,
        "attempt_no": lease.attempt_no,
        "fence": request.fence,
        "completion_request_id": request.request_id,
        "completion_request_sha256": request_sha256,
        "subject_sha256": lease.subject_sha256,
        "provider_family": subject_json["provider_family"],
        "provider_record_id": subject_json["provider_record_id"],
        "provider_target": subject_json["provider_target"],
        "original_url": subject_json["original_url"],
        "managed_authority_sha256": &current_authority.sha256,
        "managed_authority": managed_authority_json,
        "runtime": {
            "grant_id": &runtime_grant_id,
            "runtime_instance_id": request.binding.runtime_instance_id,
            "runtime_instance_epoch": request.binding.runtime_instance_epoch,
            "runtime_authority_sha256": request.binding.runtime_authority_sha256,
            "worker_runtime_identity_sha256": observation.worker_runtime_identity_sha256,
        },
        "observation_id": observation_id,
        "observation_sha256": observation_sha256,
        "observation_result": observation.result,
        "canonical_application_url": observation.canonical_application_url,
        "application_domain": observation.application_domain,
        "material_sha256": material_sha256,
        "status": receipt_status,
        "predecessor": {
            "head_revision": previous.revision,
            "transition_sha256": previous.transition_sha256,
            "material_generation": previous.material_generation,
            "material_sha256": previous.material_sha256,
        },
        "source_risk_status": "provider_observed",
        "employer_verification_status": "unverified",
        "scam_risk_status": "review_required",
        "execution_capability": "review_only",
        "checked_at_ms": now,
        "expires_at_ms": expires_at_ms,
    }))
    .map_err(original_source_storage)?;
    let receipt_sha256 = original_source_sha256(canonical_receipt_json.as_bytes());
    tx.execute(
        "INSERT INTO jobs_original_source_verification_receipts (
            receipt_id, receipt_sha256, assignment_id, attempt_id, account_id,
            job_id, subject_sha256, attempt_no, assignment_generation,
            assignment_sha256, fence, managed_authority_sha256,
            canonical_managed_authority_json, managed_environment, managed_region,
            managed_channel, managed_head_revision, managed_transition_sha256,
            managed_activation_sha256, managed_manifest_sha256,
            managed_source_protocol_schema_sha256, managed_runtime_identity_sha256,
            runtime_grant_id, runtime_instance_id, runtime_instance_epoch,
            runtime_authority_sha256, worker_runtime_identity_sha256,
            completion_request_id, completion_request_sha256, observation_id,
            observation_sha256, observation_result, canonical_application_url,
            application_domain, assurance, result,
            evidence_sha256, material_sha256, source_risk_status,
            employer_verification_status, scam_risk_status, execution_capability,
            canonical_receipt_json, checked_at_ms, expires_at_ms
         ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,
                   $15,$16,$17,$18,$19,$20,$21,$22,$23,$24,$25,$26,$27,
                   $28,$29,$30,$31,$32,$33,$34,$35,$36,$37,$38,$39,$40,
                   $41,$42,$43,$44,$45)",
        &[
            &receipt_id,
            &receipt_sha256,
            &request.assignment_id,
            &request.attempt_id,
            &lease.account_id,
            &lease.job_id,
            &lease.subject_sha256,
            &lease.attempt_no,
            &lease.assignment_generation,
            &lease.assignment_sha256,
            &request.fence,
            &current_authority.sha256,
            &current_authority.canonical_json,
            &current_authority.binding.environment,
            &current_authority.binding.region,
            &current_authority.binding.channel,
            &current_authority.binding.head_revision,
            &current_authority.binding.transition_sha256,
            &current_authority.binding.activation_sha256,
            &current_authority.binding.manifest_sha256,
            &current_authority.binding.source_protocol_schema_sha256,
            &current_authority.binding.runtime_identity_sha256,
            &runtime_grant_id,
            &request.binding.runtime_instance_id,
            &request.binding.runtime_instance_epoch,
            &request.binding.runtime_authority_sha256,
            &observation.worker_runtime_identity_sha256,
            &request.request_id,
            &request_sha256,
            &observation_id,
            &observation_sha256,
            &observation.result,
            &observation.canonical_application_url,
            &observation.application_domain,
            &observation.assurance,
            &receipt_status,
            &observation.evidence_sha256,
            &material_sha256,
            &"provider_observed",
            &"unverified",
            &"review_required",
            &"review_only",
            &canonical_receipt_json,
            &now,
            &expires_at_ms,
        ],
    )
    .map_err(original_source_storage)?;
    let (classification, material_generation) =
        original_source_classification(&previous, receipt_status, &material_sha256);
    let head_revision = previous.revision.saturating_add(1);
    let transition_id = uuid::Uuid::new_v4().to_string();
    let canonical_transition = serde_json::to_string(&json!({
        "schema_version": 1,
        "transition_id": transition_id,
        "account_id": lease.account_id,
        "job_id": lease.job_id,
        "head_revision": head_revision,
        "previous_head_revision": previous.revision,
        "predecessor_transition_sha256": previous.transition_sha256,
        "material_generation": material_generation,
        "previous_material_generation": previous.material_generation,
        "classification": classification,
        "assignment_id": request.assignment_id,
        "receipt_id": receipt_id,
        "receipt_sha256": receipt_sha256,
        "subject_sha256": lease.subject_sha256,
        "material_sha256": material_sha256,
        "assurance": observation.assurance,
        "result": receipt_status,
        "checked_at_ms": now,
        "expires_at_ms": expires_at_ms,
    }))
    .map_err(original_source_storage)?;
    let transition_sha256 = original_source_sha256(canonical_transition.as_bytes());
    tx.execute(
        "INSERT INTO jobs_original_source_verification_transitions (
            transition_id, transition_sha256, account_id, job_id, head_revision,
            previous_head_revision, predecessor_transition_sha256,
            material_generation, previous_material_generation, classification,
            assignment_id, receipt_id, receipt_sha256, subject_sha256,
            material_sha256, assurance, result, checked_at_ms, expires_at_ms,
            created_at_ms
         ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10,
                   $11, $12, $13, $14, $15, $16, $17, $18, $19, $20)",
        &[
            &transition_id,
            &transition_sha256,
            &lease.account_id,
            &lease.job_id,
            &head_revision,
            &previous.revision,
            &previous.transition_sha256,
            &material_generation,
            &previous.material_generation,
            &classification,
            &request.assignment_id,
            &receipt_id,
            &receipt_sha256,
            &lease.subject_sha256,
            &material_sha256,
            &observation.assurance,
            &receipt_status,
            &now,
            &expires_at_ms,
            &now,
        ],
    )
    .map_err(original_source_storage)?;
    original_source_advance_head_postgres(
        &mut tx,
        &lease,
        &previous,
        head_revision,
        material_generation,
        &transition_id,
        &transition_sha256,
        &request.assignment_id,
        &receipt_id,
        &receipt_sha256,
        &material_sha256,
        observation,
        receipt_status,
        now,
        expires_at_ms,
    )?;
    let prior_failures = if failure.is_some() {
        tx.query_one(
            "SELECT consecutive_failures FROM jobs_original_source_verification_assignments
              WHERE assignment_id=$1 FOR UPDATE",
            &[&request.assignment_id],
        )
        .map_err(original_source_storage)?
        .get::<_, i64>(0)
    } else {
        0
    };
    let failures = if failure.is_some() {
        prior_failures.saturating_add(1)
    } else {
        0
    };
    let (state, circuit_state, circuit_open_until_ms, next_attempt_at_ms, event_kind, reason_code) =
        if let Some((error_code, failure_class)) = failure {
            if failure_class == "unsafe" {
                (
                    "quarantined",
                    "closed",
                    None,
                    now,
                    "quarantined",
                    Some(error_code),
                )
            } else if failures >= ORIGINAL_SOURCE_CIRCUIT_FAILURE_THRESHOLD {
                let until = now.saturating_add(ORIGINAL_SOURCE_CIRCUIT_COOLDOWN_MS);
                (
                    "retry_wait",
                    "open",
                    Some(until),
                    until,
                    "circuit_opened",
                    Some(error_code),
                )
            } else {
                (
                    "retry_wait",
                    "closed",
                    None,
                    original_source_retry_at(&request.assignment_id, failures, now),
                    "failed",
                    Some(error_code),
                )
            }
        } else {
            let state = if matches!(
                receipt_status,
                "redirected_to_unknown"
                    | "identity_mismatch"
                    | "materially_changed"
                    | "source_untrusted"
            ) {
                "quarantined"
            } else {
                "idle"
            };
            (state, "closed", None, expires_at_ms, "verified", None)
        };
    original_source_append_event_postgres(
        &mut tx,
        &request.assignment_id,
        Some(&request.attempt_id),
        event_kind,
        None,
        None,
        Some(&request.request_id),
        Some(request_sha256),
        Some(&receipt_sha256),
        reason_code,
        now,
    )?;
    tx.execute(
        "UPDATE jobs_original_source_verification_assignments SET
            state = $2, active_attempt_id = NULL, lease_owner = NULL,
            lease_token_sha256 = NULL, lease_expires_at_ms = NULL,
            hard_deadline_at_ms = NULL, heartbeat_sequence = 0,
            consecutive_failures = $3, circuit_state = $4,
            circuit_open_until_ms = $5, next_attempt_at_ms = $6,
            last_error_code = $7, updated_at_ms = $8
          WHERE assignment_id = $1",
        &[
            &request.assignment_id,
            &state,
            &failures,
            &circuit_state,
            &circuit_open_until_ms,
            &next_attempt_at_ms,
            &reason_code,
            &now,
        ],
    )
    .map_err(original_source_storage)?;
    let head = resolve_original_source_verification_head_postgres_tx(
        &mut tx,
        &lease.account_id,
        &lease.job_id,
    )?;
    tx.commit().map_err(original_source_storage)?;
    Ok(OriginalSourceVerificationTerminal {
        assignment_id: request.assignment_id.clone(),
        state: state.to_string(),
        replayed: false,
        receipt_sha256: Some(receipt_sha256),
        head,
    })
}

#[allow(clippy::too_many_arguments)]
fn original_source_advance_head_postgres(
    tx: &mut postgres::Transaction<'_>,
    lease: &OriginalSourceLeaseContext,
    previous: &OriginalSourcePreviousHead,
    head_revision: i64,
    material_generation: i64,
    transition_id: &str,
    transition_sha256: &str,
    assignment_id: &str,
    receipt_id: &str,
    receipt_sha256: &str,
    material_sha256: &str,
    observation: &NormalizedOriginalSourceObservation,
    receipt_status: &str,
    now: i64,
    expires_at_ms: i64,
) -> OriginalSourceVerificationResult<()> {
    let changed = if previous.revision == 0 {
        tx.execute(
            "INSERT INTO jobs_original_source_verification_heads (
                account_id, job_id, head_revision, transition_id,
                transition_sha256, material_generation, assignment_id,
                receipt_id, receipt_sha256, subject_sha256, material_sha256,
                assurance, result, checked_at_ms, expires_at_ms, updated_at_ms
             ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10,
                       $11, $12, $13, $14, $15, $14)",
            &[
                &lease.account_id,
                &lease.job_id,
                &head_revision,
                &transition_id,
                &transition_sha256,
                &material_generation,
                &assignment_id,
                &receipt_id,
                &receipt_sha256,
                &lease.subject_sha256,
                &material_sha256,
                &observation.assurance,
                &receipt_status,
                &now,
                &expires_at_ms,
            ],
        )
        .map_err(original_source_storage)?
    } else {
        tx.execute(
            "UPDATE jobs_original_source_verification_heads SET
                head_revision = $4, transition_id = $5,
                transition_sha256 = $6, material_generation = $7,
                assignment_id = $8, receipt_id = $9, receipt_sha256 = $10,
                subject_sha256 = $11, material_sha256 = $12,
                assurance = $13, result = $14, checked_at_ms = $15,
                expires_at_ms = $16, updated_at_ms = $15
              WHERE account_id = $1 AND job_id = $2 AND head_revision = $3
                AND transition_sha256 = $17",
            &[
                &lease.account_id,
                &lease.job_id,
                &previous.revision,
                &head_revision,
                &transition_id,
                &transition_sha256,
                &material_generation,
                &assignment_id,
                &receipt_id,
                &receipt_sha256,
                &lease.subject_sha256,
                &material_sha256,
                &observation.assurance,
                &receipt_status,
                &now,
                &expires_at_ms,
                &previous.transition_sha256,
            ],
        )
        .map_err(original_source_storage)?
    };
    if changed != 1 {
        return Err(OriginalSourceVerificationError::ConcurrentHeadAdvance);
    }
    Ok(())
}

fn original_source_failure_class(error_code: &str) -> Option<&'static str> {
    if matches!(
        error_code,
        "auth_required"
            | "captcha_required"
            | "parse_ambiguous"
            | "provider_unavailable"
            | "rate_limited"
            | "unreachable"
    ) {
        Some("transient")
    } else if matches!(error_code, "source_untrusted" | "invalid_assignment") {
        Some("unsafe")
    } else {
        None
    }
}

fn original_source_retry_at(assignment_id: &str, failures: i64, now: i64) -> i64 {
    let exponent = failures.saturating_sub(1).clamp(0, 5) as u32;
    let delay = 30_000_i64
        .saturating_mul(1_i64 << exponent)
        .min(15 * 60_000);
    let digest = Sha256::digest(format!("{assignment_id}:{failures}").as_bytes());
    let jitter = u64::from_be_bytes(digest[..8].try_into().expect("SHA-256 prefix")) % 10_000;
    now.saturating_add(delay).saturating_add(jitter as i64)
}

pub fn fail_original_source_verification(
    pool: &DbPool,
    request: &OriginalSourceVerificationFailureRequest,
) -> OriginalSourceVerificationResult<OriginalSourceVerificationTerminal> {
    let error_code = request.error_code.trim().to_ascii_lowercase();
    if error_code != request.error_code {
        return Err(OriginalSourceVerificationError::InvalidInput(
            "original-source failure code is not canonical".to_string(),
        ));
    }
    let failure_class = original_source_failure_class(&error_code).ok_or_else(|| {
        OriginalSourceVerificationError::InvalidInput(
            "original-source failure code is not classified".to_string(),
        )
    })?;
    let observation = original_source_normalize_observation(&request.observation)?;
    if observation.error_code.as_deref() != Some(error_code.as_str())
        || (failure_class == "unsafe" && observation.result != "quarantined")
        || (failure_class == "transient" && observation.result != "indeterminate")
    {
        return Err(OriginalSourceVerificationError::InvalidInput(
            "failure observation does not match its classified error code".to_string(),
        ));
    }
    let request_sha256 = original_source_request_sha256(
        "fail",
        &request.assignment_id,
        &request.attempt_id,
        request.fence,
        &request.request_id,
        &request.binding,
        &observation,
    )?;
    let (session_sha256, lease_sha256) =
        original_source_binding_hashes(&request.binding, &request.lease_token)?;
    let completion = OriginalSourceVerificationCompletionRequest {
        binding: request.binding.clone(),
        assignment_id: request.assignment_id.clone(),
        attempt_id: request.attempt_id.clone(),
        fence: request.fence,
        lease_token: request.lease_token.clone(),
        request_id: request.request_id.clone(),
        observation: request.observation.clone(),
    };
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => original_source_complete_sqlite(
            pool,
            &completion,
            &observation,
            &request_sha256,
            &session_sha256,
            &lease_sha256,
            Some((&error_code, failure_class)),
        ),
        DbPool::Postgres(_) => original_source_complete_postgres(
            pool,
            &completion,
            &observation,
            &request_sha256,
            &session_sha256,
            &lease_sha256,
            Some((&error_code, failure_class)),
        ),
    })
}

#[cfg(test)]
mod original_source_verification_tests {
    use super::*;
    use crate::db;

    fn fixture_posting(source: &str, url: &str, external_id: &str) -> JobPosting {
        JobPosting {
            id: uuid::Uuid::new_v4().to_string(),
            canonical_key: format!("canonical-{external_id}"),
            source: source.to_string(),
            external_id: external_id.to_string(),
            company: "Acme".to_string(),
            title: "Software Engineer".to_string(),
            location: "New York, NY".to_string(),
            workplace: "hybrid".to_string(),
            canonical_url: url.to_string(),
            description: "Build reliable systems.".to_string(),
            compensation: "$100k-$140k".to_string(),
            employment_type: "full_time".to_string(),
            track_id: String::new(),
            match_score: 99,
            matched_reasons: vec!["mutable".to_string()],
            missing_requirements: Vec::new(),
            posted_at_ms: Some(1_700_000_000_000),
            last_verified_at_ms: None,
            availability_status: "active".to_string(),
            status: "matched".to_string(),
            created_at_ms: 1,
            updated_at_ms: 1,
            discovery_evidence: JobDiscoveryEvidence::default(),
            eligibility: None,
        }
    }

    fn fixture_pool_and_posting() -> (DbPool, JobPosting) {
        let path = std::env::temp_dir().join(format!(
            "bluey-original-source-{}-{}.sqlite3",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let pool = db::open_pool(&path).expect("open SQLite fixture");
        db::run_migrations(&pool).expect("run SQLite migrations");
        let posting = fixture_posting(
            "greenhouse_import",
            "https://boards.greenhouse.io/acme/jobs/123",
            "123",
        );
        let conn = pool.get().expect("get SQLite fixture");
        conn.execute(
            "INSERT INTO accounts (id,email,password_hash,trial_seconds_remaining)
             VALUES ('acct-original-source','source@example.com','hash',0)",
            [],
        )
        .expect("insert account");
        conn.execute(
            "INSERT INTO jobs_postings (
                id,account_id,canonical_key,posting_json,source,canonical_url,
                company,title,location,match_score,status,created_at_ms,updated_at_ms
             ) VALUES (?1,'acct-original-source',?2,?3,?4,?5,?6,?7,?8,0,'matched',1,1)",
            params![
                posting.id,
                posting.canonical_key,
                serde_json::to_string(&posting).unwrap(),
                posting.source,
                posting.canonical_url,
                posting.company,
                posting.title,
                posting.location
            ],
        )
        .expect("insert posting");
        conn.execute(
            "INSERT INTO jobs_discovery_sources (
                id,account_id,provider,source_key,source_json,status,health,
                next_run_at_ms,created_at_ms,updated_at_ms)
             VALUES ('source-original','acct-original-source','greenhouse','acme',
                '{}','active','healthy',0,1,1)",
            [],
        )
        .expect("insert discovery source");
        conn.execute(
            "INSERT INTO jobs_discovery_memberships (
                source_id,account_id,external_id,canonical_key,job_id,content_hash,
                first_seen_at_ms,last_seen_at_ms,last_seen_run_id,availability_status)
             VALUES ('source-original','acct-original-source',?1,?2,?3,?4,1,1,'run-1','pending')",
            params![
                posting.external_id,
                posting.canonical_key,
                posting.id,
                "a".repeat(64)
            ],
        )
        .expect("insert discovery membership");
        drop(conn);
        (pool, posting)
    }

    fn fixture_managed_authority() -> (OriginalSourceManagedAuthorityBinding, String, String) {
        let authority = OriginalSourceManagedAuthorityBinding {
            account_id: "acct-original-source".to_string(),
            environment: "production".to_string(),
            region: "us-east-1".to_string(),
            channel: "general".to_string(),
            head_revision: 1,
            transition_sha256: "1".repeat(64),
            activation_sha256: "2".repeat(64),
            manifest_sha256: "3".repeat(64),
            cohort_sha256: "4".repeat(64),
            trust_generation: 1,
            channel_sequence: 1,
            release_id: "release-original-source-v2".to_string(),
            release_sequence: 1,
            task_queue_sha256: "5".repeat(64),
            failure_converter_sha256: "6".repeat(64),
            activation_expires_at_ms: 9_007_199_254_740_991,
            source_protocol_schema_sha256: "7".repeat(64),
            runtime_identity_sha256: "8".repeat(64),
            dependency_evidence_sha256: "9".repeat(64),
            heartbeat_ttl_ms: 60_000,
        };
        let canonical = serde_json::to_string(&authority).unwrap();
        let sha256 = original_source_sha256(canonical.as_bytes());
        (authority, canonical, sha256)
    }

    fn insert_fixture_posting_source(
        pool: &DbPool,
        posting: &JobPosting,
        source_id: &str,
        provider: &str,
        source_key: &str,
    ) {
        insert_fixture_posting_source_for_account(
            pool,
            "acct-original-source",
            posting,
            source_id,
            provider,
            source_key,
        );
    }

    fn insert_fixture_posting_source_for_account(
        pool: &DbPool,
        account_id: &str,
        posting: &JobPosting,
        source_id: &str,
        provider: &str,
        source_key: &str,
    ) {
        let conn = pool.get().expect("get additional posting fixture");
        conn.execute(
            "INSERT INTO jobs_postings (
                id,account_id,canonical_key,posting_json,source,canonical_url,
                company,title,location,match_score,status,created_at_ms,updated_at_ms
             ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,0,'matched',1,1)",
            params![
                posting.id,
                account_id,
                posting.canonical_key,
                serde_json::to_string(posting).unwrap(),
                posting.source,
                posting.canonical_url,
                posting.company,
                posting.title,
                posting.location,
            ],
        )
        .expect("insert additional posting");
        conn.execute(
            "INSERT INTO jobs_discovery_sources (
                id,account_id,provider,source_key,source_json,status,health,
                next_run_at_ms,created_at_ms,updated_at_ms)
             VALUES (?1,?2,?3,?4,'{}','active','healthy',0,1,1)",
            params![source_id, account_id, provider, source_key],
        )
        .expect("insert additional discovery source");
        conn.execute(
            "INSERT INTO jobs_discovery_memberships (
                source_id,account_id,external_id,canonical_key,job_id,content_hash,
                first_seen_at_ms,last_seen_at_ms,last_seen_run_id,availability_status)
             VALUES (?1,?2,?3,?4,?5,?6,1,1,'run-2','pending')",
            params![
                source_id,
                account_id,
                posting.external_id,
                posting.canonical_key,
                posting.id,
                "b".repeat(64),
            ],
        )
        .expect("insert additional discovery membership");
    }

    fn fixture_candidate_sqlite(
        tx: &rusqlite::Transaction<'_>,
        assignment_id: &str,
    ) -> OriginalSourceLeaseCandidate {
        tx.query_row(
            "SELECT assignment_id,account_id,job_id,subject_sha256,
                    canonical_subject_json,state,attempt_count,active_attempt_id,
                    circuit_state,expires_at_ms,next_attempt_at_ms,created_at_ms
               FROM jobs_original_source_verification_assignments
              WHERE assignment_id=?1",
            params![assignment_id],
            |row| {
                Ok(OriginalSourceLeaseCandidate {
                    assignment_id: row.get(0)?,
                    account_id: row.get(1)?,
                    job_id: row.get(2)?,
                    subject_sha256: row.get(3)?,
                    canonical_subject_json: row.get(4)?,
                    state: row.get(5)?,
                    attempt_count: row.get(6)?,
                    active_attempt_id: row.get(7)?,
                    circuit_state: row.get(8)?,
                    assignment_expires_at_ms: row.get(9)?,
                    next_attempt_at_ms: row.get(10)?,
                    created_at_ms: row.get(11)?,
                })
            },
        )
        .expect("load lease candidate fixture")
    }

    fn fixture_two_assignments() -> (
        DbPool,
        JobPosting,
        JobPosting,
        OriginalSourceVerificationAssignment,
        OriginalSourceVerificationAssignment,
    ) {
        let (pool, first_posting) = fixture_pool_and_posting();
        let second_posting =
            fixture_posting("lever_import", "https://jobs.lever.co/bravo/456", "456");
        insert_fixture_posting_source(
            &pool,
            &second_posting,
            "source-original-second",
            "lever",
            "bravo",
        );
        let (authority, canonical_authority, authority_sha256) = fixture_managed_authority();
        let mut conn = pool.get().unwrap();
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        let first_assignment = ensure_original_source_verification_assignment_sqlite_bound_tx(
            &tx,
            "acct-original-source",
            &first_posting,
            &authority,
            &canonical_authority,
            &authority_sha256,
        )
        .unwrap();
        let second_assignment = ensure_original_source_verification_assignment_sqlite_bound_tx(
            &tx,
            "acct-original-source",
            &second_posting,
            &authority,
            &canonical_authority,
            &authority_sha256,
        )
        .unwrap();
        tx.commit().unwrap();
        (
            pool,
            first_posting,
            second_posting,
            first_assignment,
            second_assignment,
        )
    }

    struct PublicOriginalSourceLifecycleFixture {
        database_path: std::path::PathBuf,
        pool: DbPool,
        account_id: String,
        source_id: String,
        posting: JobPosting,
        assignment: OriginalSourceVerificationAssignment,
        binding: OriginalSourceVerifierBinding,
        runtime_identity_sha256: String,
        grant_id: String,
    }

    fn public_original_source_lifecycle_fixture() -> PublicOriginalSourceLifecycleFixture {
        let managed = super::managed_cloud_release_authority_tests::
            original_source_verifier_runtime_test_fixture();
        public_original_source_lifecycle_fixture_from_managed(managed)
    }

    fn public_original_source_lifecycle_long_horizon_fixture(
    ) -> PublicOriginalSourceLifecycleFixture {
        let managed = super::managed_cloud_release_authority_tests::
            original_source_verifier_runtime_long_horizon_test_fixture();
        public_original_source_lifecycle_fixture_from_managed(managed)
    }

    fn public_original_source_lifecycle_fixture_from_managed(
        managed: super::managed_cloud_release_authority_tests::
            OriginalSourceVerifierRuntimeTestFixture,
    ) -> PublicOriginalSourceLifecycleFixture {
        let database_path = managed.database_path.clone();
        let pool = managed.pool.clone();
        let account_id = managed.account_id.clone();
        let grant_id = managed.grant_id.clone();
        let binding = OriginalSourceVerifierBinding {
            worker_id: managed.worker_id.clone(),
            runtime_instance_id: managed.runtime_instance_id.clone(),
            runtime_instance_epoch: managed.runtime_instance_epoch,
            runtime_authority_sha256: managed.runtime_authority_sha256.clone(),
            runtime_session_token: managed.runtime_session_token.clone(),
        };
        drop(managed);
        {
            let conn = pool.get().expect("get public lifecycle account fixture");
            conn.execute(
                "INSERT INTO accounts(id,email,password_hash,trial_seconds_remaining)
                 VALUES(?1,?2,'hash',0)",
                params![account_id, "public-osv-lifecycle@example.test"],
            )
            .expect("insert public lifecycle account");
        }
        let posting = fixture_posting(
            "greenhouse_import",
            "https://boards.greenhouse.io/publiclifecycle/jobs/public-lifecycle-job",
            "public-lifecycle-job",
        );
        let source_id = "source-public-osv-lifecycle".to_string();
        insert_fixture_posting_source_for_account(
            &pool,
            &account_id,
            &posting,
            &source_id,
            "greenhouse",
            "publiclifecycle",
        );
        let (assignment, runtime_identity_sha256) = {
            let mut conn = pool.get().expect("get public lifecycle assignment fixture");
            let tx = conn
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .expect("begin public lifecycle assignment fixture");
            let authority =
                sqlite_original_source_verification_authority_for_account_tx(&tx, &account_id)
                    .expect("resolve public lifecycle authority")
                    .expect("public lifecycle authority is active");
            let runtime_identity_sha256 = authority.runtime_identity_sha256.clone();
            let assignment =
                ensure_original_source_verification_assignment_sqlite_with_authority_tx(
                    &tx,
                    &account_id,
                    &posting,
                    &authority,
                )
                .expect("schedule public lifecycle assignment");
            tx.commit().expect("commit public lifecycle assignment");
            (assignment, runtime_identity_sha256)
        };
        PublicOriginalSourceLifecycleFixture {
            database_path,
            pool,
            account_id,
            source_id,
            posting,
            assignment,
            binding,
            runtime_identity_sha256,
            grant_id,
        }
    }

    fn cleanup_public_original_source_lifecycle_fixture(
        fixture: PublicOriginalSourceLifecycleFixture,
    ) {
        let database_path = fixture.database_path.clone();
        drop(fixture);
        let _ = std::fs::remove_file(&database_path);
        let _ = std::fs::remove_file(database_path.with_extension("sqlite3-wal"));
        let _ = std::fs::remove_file(database_path.with_extension("sqlite3-shm"));
    }

    fn refresh_public_original_source_runtime_heartbeat(
        pool: &DbPool,
        binding: &OriginalSourceVerifierBinding,
    ) {
        let current = {
            let connection = pool.get().expect("load public verifier heartbeat");
            connection
                .query_row(
                    &format!(
                        "SELECT {MANAGED_CLOUD_RUNTIME_HEARTBEAT_COLUMNS}
                           FROM jobs_managed_cloud_original_source_verifier_runtime_heartbeats
                          WHERE runtime_instance_id=?1"
                    ),
                    params![binding.runtime_instance_id],
                    managed_cloud_runtime_heartbeat_from_sqlite,
                )
                .expect("public verifier heartbeat exists")
        };
        assert_eq!(current.worker_id, binding.worker_id);
        assert_eq!(current.instance_epoch, binding.runtime_instance_epoch);
        let heartbeat_sequence = current
            .heartbeat_sequence
            .checked_add(1)
            .expect("public verifier heartbeat sequence advances");
        let refreshed = record_managed_cloud_runtime_heartbeat(
            pool,
            &ManagedCloudRuntimeHeartbeatInput {
                runtime_instance_id: current.runtime_instance_id,
                worker_id: current.worker_id,
                session_token: binding.runtime_session_token.clone(),
                heartbeat_sequence,
                observed_head_revision: current.observed_head_revision,
                observed_transition_sha256: current.observed_transition_sha256,
                activation_sha256: current.activation_sha256,
                manifest_sha256: current.manifest_sha256,
                component_id: current.component_id,
                role: current.role,
                artifact_sha256: current.artifact_sha256,
                migration_set_sha256: current.migration_set_sha256,
                config_schema_sha256: current.config_schema_sha256,
                protocol_set_sha256: current.protocol_set_sha256,
                task_queue_sha256: current.task_queue_sha256,
                failure_converter_sha256: current.failure_converter_sha256,
                dependency_evidence_sha256: current.dependency_evidence_sha256,
                health_state: current.health_state,
                reason_code: current.reason_code,
            },
        )
        .expect("refresh exact public verifier runtime heartbeat");
        assert_eq!(refreshed.heartbeat_sequence, heartbeat_sequence);
        assert!(!refreshed.replayed);
    }

    fn expire_public_original_source_runtime_heartbeat(
        fixture: &PublicOriginalSourceLifecycleFixture,
    ) {
        let connection = fixture
            .pool
            .get()
            .expect("expire public verifier heartbeat");
        let now_ms: i64 = connection
            .query_row(
                "SELECT CAST(unixepoch('subsec') * 1000 AS INTEGER)",
                [],
                |row| row.get(0),
            )
            .expect("public verifier database time");
        let heartbeat_ttl_ms: i64 = connection
            .query_row(
                "SELECT requirement.heartbeat_ttl_ms
                   FROM jobs_managed_cloud_original_source_verifier_runtime_heartbeats heartbeat
                   JOIN jobs_managed_cloud_activation_requirements requirement
                     ON requirement.activation_sha256=heartbeat.activation_sha256
                    AND requirement.role=heartbeat.role
                  WHERE heartbeat.runtime_instance_id=?1",
                params![fixture.binding.runtime_instance_id],
                |row| row.get(0),
            )
            .expect("public verifier heartbeat TTL");
        connection
            .execute_batch(
                "DROP TRIGGER
                    trg_jobs_managed_cloud_original_source_verifier_runtime_heartbeats_fenced_update;",
            )
            .expect("drop fixture-only heartbeat update fence");
        connection
            .execute(
                "UPDATE jobs_managed_cloud_original_source_verifier_runtime_heartbeats
                    SET heartbeat_at_ms=?1
                  WHERE runtime_instance_id=?2",
                params![
                    now_ms
                        .saturating_sub(heartbeat_ttl_ms)
                        .saturating_sub(60_000),
                    fixture.binding.runtime_instance_id,
                ],
            )
            .expect("expire fixture-only public verifier heartbeat");
    }

    fn public_open_observation(
        fixture: &PublicOriginalSourceLifecycleFixture,
    ) -> OriginalSourceVerificationObservation {
        let subject: OriginalSourceComparisonSubject =
            serde_json::from_str(&fixture.assignment.canonical_subject_json)
                .expect("parse public lifecycle subject");
        let requested_url =
            original_source_expected_requested_url(&subject).expect("public lifecycle request URL");
        let (parser_version, parser_digest) = original_source_parser_metadata("greenhouse");
        let normalized = seal_observation(NormalizedOriginalSourceObservation {
            assurance: "original_verified".to_string(),
            result: "open".to_string(),
            error_code: None,
            evidence_sha256: "0".repeat(64),
            requested_url: Some(requested_url.clone()),
            canonical_observed_url: Some(fixture.posting.canonical_url.clone()),
            canonical_application_url: Some(fixture.posting.canonical_url.clone()),
            application_domain: Some("boards.greenhouse.io".to_string()),
            retrieval_status: "observed".to_string(),
            http_status: Some(200),
            http_semantics_digest: original_source_http_semantics_digest(&requested_url, 200),
            redirect_chain_digest: original_source_empty_redirect_chain_digest(),
            headers_digest: "1".repeat(64),
            content_digest: "2".repeat(64),
            parser_version,
            parser_digest,
            worker_runtime_identity_sha256: fixture.runtime_identity_sha256.clone(),
            provider_record_id: Some(subject.provider_record_id),
            company: Some(fixture.posting.company.clone()),
            title: Some(fixture.posting.title.clone()),
            location: Some(fixture.posting.location.clone()),
            workplace: Some(fixture.posting.workplace.clone()),
            description: Some(fixture.posting.description.clone()),
            compensation: Some(fixture.posting.compensation.clone()),
            employment_type: Some(fixture.posting.employment_type.clone()),
            posted_at_ms: fixture.posting.posted_at_ms,
            mismatched_fields: Vec::new(),
        });
        serde_json::from_value(serde_json::to_value(normalized).unwrap())
            .expect("convert public lifecycle observation")
    }

    fn public_unreachable_observation(
        fixture: &PublicOriginalSourceLifecycleFixture,
    ) -> OriginalSourceVerificationObservation {
        let subject: OriginalSourceComparisonSubject =
            serde_json::from_str(&fixture.assignment.canonical_subject_json)
                .expect("parse public failure subject");
        let requested_url =
            original_source_expected_requested_url(&subject).expect("public failure request URL");
        let (parser_version, parser_digest) = original_source_parser_metadata("greenhouse");
        let normalized = seal_observation(NormalizedOriginalSourceObservation {
            assurance: "original_verified".to_string(),
            result: "indeterminate".to_string(),
            error_code: Some("unreachable".to_string()),
            evidence_sha256: "0".repeat(64),
            requested_url: Some(requested_url),
            canonical_observed_url: None,
            canonical_application_url: None,
            application_domain: None,
            retrieval_status: "unreachable".to_string(),
            http_status: None,
            http_semantics_digest: original_source_empty_retrieval_digest("http-semantics"),
            redirect_chain_digest: original_source_empty_retrieval_digest("redirect-chain"),
            headers_digest: original_source_empty_retrieval_digest("headers"),
            content_digest: original_source_empty_retrieval_digest("content"),
            parser_version,
            parser_digest,
            worker_runtime_identity_sha256: fixture.runtime_identity_sha256.clone(),
            provider_record_id: None,
            company: None,
            title: None,
            location: None,
            workplace: None,
            description: None,
            compensation: None,
            employment_type: None,
            posted_at_ms: None,
            mismatched_fields: Vec::new(),
        });
        serde_json::from_value(serde_json::to_value(normalized).unwrap())
            .expect("convert public failure observation")
    }

    fn public_completion_request(
        fixture: &PublicOriginalSourceLifecycleFixture,
        lease: &OriginalSourceVerificationLease,
        request_id: &str,
    ) -> OriginalSourceVerificationCompletionRequest {
        OriginalSourceVerificationCompletionRequest {
            binding: fixture.binding.clone(),
            assignment_id: lease.assignment_id.clone(),
            attempt_id: lease.attempt_id.clone(),
            fence: lease.fence,
            lease_token: lease.lease_token.clone(),
            request_id: request_id.to_string(),
            observation: public_open_observation(fixture),
        }
    }

    #[test]
    fn original_source_subject_supports_closed_five_provider_coordinates() {
        let fixtures = [
            (
                "greenhouse_import",
                "https://boards.greenhouse.io/acme/jobs/123",
                "123",
            ),
            ("lever", "https://jobs.lever.co/acme/abc", "abc"),
            (
                "lever_import",
                "https://jobs.lever.co/acme/abc/apply",
                "abc",
            ),
            ("ashby_import", "https://jobs.ashbyhq.com/acme/abc", "abc"),
            (
                "smartrecruiters",
                "https://jobs.smartrecruiters.com/acme/abc",
                "abc",
            ),
            (
                "workday_import",
                "https://acme.wd5.myworkdayjobs.com/en-US/careers/job/engineer/Software-Engineer_R-123",
                "R-123",
            ),
        ];
        for (source, url, external_id) in fixtures {
            let posting = fixture_posting(source, url, external_id);
            let (canonical, digest) = original_source_canonical_subject(&posting)
                .unwrap_or_else(|error| panic!("{source} subject failed: {error}"));
            assert!(original_source_valid_sha256(&digest));
            let value: Value = serde_json::from_str(&canonical).unwrap();
            assert_eq!(value["schema_version"], 1);
            assert_eq!(value["provider_target"]["job"], external_id);
            assert_eq!(value["expected"]["title"], posting.title);
            if source == "workday_import" {
                assert_eq!(value["provider_target"]["tenant"], "acme");
            }
        }
    }

    #[test]
    fn lever_application_subject_validates_closed_request_and_destination() {
        let posting = fixture_posting(
            "lever_import",
            "https://jobs.lever.co/acme/abc/apply",
            "abc",
        );
        let (subject_json, _) = original_source_canonical_subject(&posting).unwrap();
        let subject: OriginalSourceComparisonSubject = serde_json::from_str(&subject_json).unwrap();
        assert_eq!(subject.provider_target.variant, "lever_application");
        let requested_url = original_source_expected_requested_url(&subject).unwrap();
        assert_eq!(
            requested_url,
            "https://api.lever.co/v0/postings/acme/abc?mode=json"
        );
        let (parser_version, parser_digest) = original_source_parser_metadata("lever");
        let observation = seal_observation(NormalizedOriginalSourceObservation {
            assurance: "original_verified".to_string(),
            result: "open".to_string(),
            error_code: None,
            evidence_sha256: "0".repeat(64),
            requested_url: Some(requested_url.clone()),
            canonical_observed_url: Some(posting.canonical_url.clone()),
            canonical_application_url: Some(posting.canonical_url.clone()),
            application_domain: Some("jobs.lever.co".to_string()),
            retrieval_status: "observed".to_string(),
            http_status: Some(200),
            http_semantics_digest: original_source_http_semantics_digest(&requested_url, 200),
            redirect_chain_digest: original_source_empty_redirect_chain_digest(),
            headers_digest: "a".repeat(64),
            content_digest: "b".repeat(64),
            parser_version,
            parser_digest,
            worker_runtime_identity_sha256: "c".repeat(64),
            provider_record_id: Some(subject.provider_record_id),
            company: Some(posting.company.clone()),
            title: Some(posting.title.clone()),
            location: Some(posting.location.clone()),
            workplace: Some(posting.workplace.clone()),
            description: Some(posting.description.clone()),
            compensation: Some(posting.compensation.clone()),
            employment_type: Some(posting.employment_type.clone()),
            posted_at_ms: posting.posted_at_ms,
            mismatched_fields: Vec::new(),
        });
        original_source_validate_observation_subject(&subject_json, &observation).unwrap();
    }

    #[test]
    fn smartrecruiters_slug_subject_uses_stable_identity_and_validates_destination() {
        let posting = fixture_posting(
            "smartrecruiters_import",
            "https://jobs.smartrecruiters.com/acme/abc-platform-engineer",
            "abc",
        );
        let (subject_json, _) = original_source_canonical_subject(&posting).unwrap();
        let subject: OriginalSourceComparisonSubject = serde_json::from_str(&subject_json).unwrap();
        assert_eq!(
            subject.original_url,
            "https://jobs.smartrecruiters.com/acme/abc"
        );
        assert_eq!(subject.provider_target.job, "abc");
        let requested_url = original_source_expected_requested_url(&subject).unwrap();
        assert_eq!(
            requested_url,
            "https://api.smartrecruiters.com/v1/companies/acme/postings/abc"
        );
        let (parser_version, parser_digest) = original_source_parser_metadata("smartrecruiters");
        let observation = seal_observation(NormalizedOriginalSourceObservation {
            assurance: "original_verified".to_string(),
            result: "open".to_string(),
            error_code: None,
            evidence_sha256: "0".repeat(64),
            requested_url: Some(requested_url.clone()),
            canonical_observed_url: Some(subject.original_url.clone()),
            canonical_application_url: Some(
                "https://www.smartrecruiters.com/acme/abc-platform-engineer".to_string(),
            ),
            application_domain: Some("www.smartrecruiters.com".to_string()),
            retrieval_status: "observed".to_string(),
            http_status: Some(200),
            http_semantics_digest: original_source_http_semantics_digest(&requested_url, 200),
            redirect_chain_digest: original_source_empty_redirect_chain_digest(),
            headers_digest: "d".repeat(64),
            content_digest: "e".repeat(64),
            parser_version,
            parser_digest,
            worker_runtime_identity_sha256: "f".repeat(64),
            provider_record_id: Some(subject.provider_record_id),
            company: Some(posting.company.clone()),
            title: Some(posting.title.clone()),
            location: Some(posting.location.clone()),
            workplace: Some(posting.workplace.clone()),
            description: Some(posting.description.clone()),
            compensation: Some(posting.compensation.clone()),
            employment_type: Some(posting.employment_type.clone()),
            posted_at_ms: posting.posted_at_ms,
            mismatched_fields: Vec::new(),
        });
        original_source_validate_observation_subject(&subject_json, &observation).unwrap();

        let (pool, mut assigned_posting) = fixture_pool_and_posting();
        assigned_posting.canonical_key = posting.canonical_key.clone();
        assigned_posting.source = posting.source.clone();
        assigned_posting.external_id = posting.external_id.clone();
        assigned_posting.canonical_url = posting.canonical_url.clone();
        let mut conn = pool.get().unwrap();
        conn.execute(
            "UPDATE jobs_discovery_sources
                SET provider='smartrecruiters', source_key='acme'
              WHERE id='source-original'",
            [],
        )
        .unwrap();
        conn.execute(
            "UPDATE jobs_discovery_memberships
                SET external_id=?1, canonical_key=?2
              WHERE source_id='source-original'",
            params![assigned_posting.external_id, assigned_posting.canonical_key],
        )
        .unwrap();
        conn.execute(
            "UPDATE jobs_postings SET canonical_key=?2, posting_json=?3,
                    source=?4, canonical_url=?5
              WHERE account_id='acct-original-source' AND id=?1",
            params![
                assigned_posting.id,
                assigned_posting.canonical_key,
                serde_json::to_string(&assigned_posting).unwrap(),
                assigned_posting.source,
                assigned_posting.canonical_url,
            ],
        )
        .unwrap();
        let (managed_authority, canonical_authority, authority_sha256) =
            fixture_managed_authority();
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        let assignment = ensure_original_source_verification_assignment_sqlite_bound_tx(
            &tx,
            "acct-original-source",
            &assigned_posting,
            &managed_authority,
            &canonical_authority,
            &authority_sha256,
        )
        .unwrap();
        let assigned_subject: OriginalSourceComparisonSubject =
            serde_json::from_str(&assignment.canonical_subject_json).unwrap();
        assert_eq!(
            assigned_subject.original_url,
            "https://jobs.smartrecruiters.com/acme/abc"
        );
        tx.commit().unwrap();

        let prefix_confusion = fixture_posting(
            "smartrecruiters_import",
            "https://jobs.smartrecruiters.com/acme/abcd-platform-engineer",
            "abc",
        );
        assert!(original_source_canonical_subject(&prefix_confusion).is_err());
    }

    #[test]
    fn subject_ignores_forged_legacy_employer_evidence_and_rejects_hostile_targets() {
        let mut posting = fixture_posting(
            "greenhouse_import",
            "https://boards.greenhouse.io/acme/jobs/123",
            "123",
        );
        let first = original_source_subject_sha256(&posting).unwrap();
        posting.discovery_evidence.employer_id = Some("forged-employer".to_string());
        posting.discovery_evidence.canonical_job_id = Some("forged-job".to_string());
        assert_eq!(first, original_source_subject_sha256(&posting).unwrap());

        posting.canonical_url = "https://evil.example/acme/jobs/123".to_string();
        assert!(original_source_subject_sha256(&posting).is_err());
        posting.canonical_url = "https://boards.greenhouse.io/acme/not-jobs/123".to_string();
        assert!(original_source_subject_sha256(&posting).is_err());
    }

    fn seal_observation(
        mut observation: NormalizedOriginalSourceObservation,
    ) -> NormalizedOriginalSourceObservation {
        observation.evidence_sha256 =
            original_source_observation_evidence_sha256(&observation).unwrap();
        observation
    }

    #[test]
    fn observation_evidence_hash_matches_worker_canonical_vector() {
        let observation = NormalizedOriginalSourceObservation {
            assurance: "original_verified".to_string(),
            result: "mismatch".to_string(),
            error_code: None,
            evidence_sha256: "0".repeat(64),
            requested_url: Some(
                "https://boards-api.greenhouse.io/v1/boards/acme/jobs/job-1?content=true"
                    .to_string(),
            ),
            canonical_observed_url: Some(
                "https://boards.greenhouse.io/acme/jobs/job-1".to_string(),
            ),
            canonical_application_url: Some(
                "https://boards.greenhouse.io/acme/jobs/job-1".to_string(),
            ),
            application_domain: Some("boards.greenhouse.io".to_string()),
            retrieval_status: "observed".to_string(),
            http_status: Some(200),
            http_semantics_digest:
                "6a4e9b73cf75a852965c3d42d4888388512e480196c475bff7fdb27ac59896c0".to_string(),
            redirect_chain_digest:
                "6548f60af9466cb014ce78a18d7859fabb641d079d5a9d844ee1254d8fa18784".to_string(),
            headers_digest: "e53ea0275070466e688898219e04cf4649b9512bd825069700dad32dd149df32"
                .to_string(),
            content_digest: "73d358e93a1cc5a83709202bb00d447b3c8d48622ed0a06a8d386a2d95a1ae9c"
                .to_string(),
            parser_version: "greenhouse.original_source.v1".to_string(),
            parser_digest: "e3b96ea3251d6e3bcd21418cd94b541f05c321f63a5eba7495de8e3c667457cd"
                .to_string(),
            worker_runtime_identity_sha256: "a".repeat(64),
            provider_record_id: Some("greenhouse:acme:job-1".to_string()),
            company: Some("Acme".to_string()),
            title: Some("Platform Engineer".to_string()),
            location: Some("Austin, TX".to_string()),
            workplace: Some("hybrid".to_string()),
            description: Some("Build reliable systems.".to_string()),
            compensation: None,
            employment_type: Some("full_time".to_string()),
            posted_at_ms: Some(1_787_659_200_000),
            mismatched_fields: vec!["compensation".to_string()],
        };
        assert_eq!(
            original_source_observation_evidence_sha256(&observation).unwrap(),
            "b17c32858763afbb9da6ebc48bf37841a34956bf9f9bf6c15ef146e7dabff69f"
        );
    }

    #[test]
    fn observation_result_mismatches_and_assurance_are_server_validated() {
        let posting = fixture_posting(
            "greenhouse_import",
            "https://boards.greenhouse.io/acme/jobs/123",
            "123",
        );
        let (subject_json, _) = original_source_canonical_subject(&posting).unwrap();
        let subject: Value = serde_json::from_str(&subject_json).unwrap();
        let provider_record_id = subject["provider_record_id"].as_str().unwrap().to_string();
        let comparison_subject: OriginalSourceComparisonSubject =
            serde_json::from_str(&subject_json).unwrap();
        let requested_url = original_source_expected_requested_url(&comparison_subject).unwrap();
        let (parser_version, parser_digest) = original_source_parser_metadata("greenhouse");
        let exact = seal_observation(NormalizedOriginalSourceObservation {
            assurance: "original_verified".to_string(),
            result: "open".to_string(),
            error_code: None,
            evidence_sha256: "0".repeat(64),
            requested_url: Some(requested_url.clone()),
            canonical_observed_url: Some(posting.canonical_url.clone()),
            canonical_application_url: Some(posting.canonical_url.clone()),
            application_domain: Some("boards.greenhouse.io".to_string()),
            retrieval_status: "observed".to_string(),
            http_status: Some(200),
            http_semantics_digest: original_source_http_semantics_digest(&requested_url, 200),
            redirect_chain_digest: original_source_empty_redirect_chain_digest(),
            headers_digest: "3".repeat(64),
            content_digest: "4".repeat(64),
            parser_version,
            parser_digest,
            worker_runtime_identity_sha256: "5".repeat(64),
            provider_record_id: Some(provider_record_id),
            company: Some("ＡＣＭＥ".to_string()),
            title: Some("software   engineer".to_string()),
            location: Some(posting.location.clone()),
            workplace: Some(posting.workplace.clone()),
            description: Some(posting.description.clone()),
            compensation: Some(posting.compensation.clone()),
            employment_type: Some(posting.employment_type.clone()),
            posted_at_ms: posting.posted_at_ms,
            mismatched_fields: Vec::new(),
        });
        original_source_validate_observation_subject(&subject_json, &exact).unwrap();

        let mut identity_case_drift = exact.clone();
        identity_case_drift.canonical_observed_url = Some(posting.canonical_url.to_uppercase());
        identity_case_drift.evidence_sha256 =
            original_source_observation_evidence_sha256(&identity_case_drift).unwrap();
        assert!(
            original_source_validate_observation_subject(&subject_json, &identity_case_drift)
                .is_err()
        );
        identity_case_drift.result = "mismatch".to_string();
        identity_case_drift.mismatched_fields = vec!["original_url".to_string()];
        identity_case_drift.evidence_sha256 =
            original_source_observation_evidence_sha256(&identity_case_drift).unwrap();
        assert!(
            original_source_validate_observation_subject(&subject_json, &identity_case_drift)
                .is_err()
        );

        let mut drift = exact.clone();
        drift.title = Some("Principal Engineer".to_string());
        drift.evidence_sha256 = original_source_observation_evidence_sha256(&drift).unwrap();
        assert!(original_source_validate_observation_subject(&subject_json, &drift).is_err());
        drift.result = "mismatch".to_string();
        drift.mismatched_fields = vec!["title".to_string()];
        drift.evidence_sha256 = original_source_observation_evidence_sha256(&drift).unwrap();
        original_source_validate_observation_subject(&subject_json, &drift).unwrap();
        drift.mismatched_fields = vec!["company".to_string()];
        drift.evidence_sha256 = original_source_observation_evidence_sha256(&drift).unwrap();
        assert!(original_source_validate_observation_subject(&subject_json, &drift).is_err());

        let closed = seal_observation(NormalizedOriginalSourceObservation {
            assurance: "original_verified".to_string(),
            result: "closed".to_string(),
            error_code: None,
            evidence_sha256: "0".repeat(64),
            requested_url: exact.requested_url.clone(),
            canonical_observed_url: None,
            canonical_application_url: None,
            application_domain: None,
            retrieval_status: "not_found".to_string(),
            http_status: Some(404),
            http_semantics_digest: original_source_http_semantics_digest(&requested_url, 404),
            redirect_chain_digest: original_source_empty_redirect_chain_digest(),
            headers_digest: "8".repeat(64),
            content_digest: "9".repeat(64),
            parser_version: exact.parser_version.clone(),
            parser_digest: exact.parser_digest.clone(),
            worker_runtime_identity_sha256: exact.worker_runtime_identity_sha256.clone(),
            provider_record_id: None,
            company: None,
            title: None,
            location: None,
            workplace: None,
            description: None,
            compensation: None,
            employment_type: None,
            posted_at_ms: None,
            mismatched_fields: Vec::new(),
        });
        original_source_validate_observation_subject(&subject_json, &closed).unwrap();

        let absent = seal_observation(NormalizedOriginalSourceObservation {
            retrieval_status: "absent".to_string(),
            http_status: Some(200),
            http_semantics_digest: original_source_http_semantics_digest(&requested_url, 200),
            ..closed.clone()
        });
        original_source_validate_observation_subject(&subject_json, &absent).unwrap();
        for status in [200, 500] {
            let impossible_closed = seal_observation(NormalizedOriginalSourceObservation {
                retrieval_status: "observed".to_string(),
                http_status: Some(status),
                http_semantics_digest: original_source_http_semantics_digest(
                    &requested_url,
                    status,
                ),
                ..closed.clone()
            });
            assert!(original_source_validate_observation_subject(
                &subject_json,
                &impossible_closed
            )
            .is_err());
        }

        let network_failure = |error_code: &str, status: i64| {
            seal_observation(NormalizedOriginalSourceObservation {
                result: if error_code == "source_untrusted" {
                    "quarantined".to_string()
                } else {
                    "indeterminate".to_string()
                },
                error_code: Some(error_code.to_string()),
                retrieval_status: "observed".to_string(),
                http_status: Some(status),
                http_semantics_digest: original_source_http_semantics_digest(
                    &requested_url,
                    status,
                ),
                ..closed.clone()
            })
        };
        for (error_code, status) in [
            ("auth_required", 401),
            ("auth_required", 403),
            ("rate_limited", 429),
            ("provider_unavailable", 503),
            ("captcha_required", 200),
            ("parse_ambiguous", 200),
            ("source_untrusted", 400),
        ] {
            original_source_validate_observation_subject(
                &subject_json,
                &network_failure(error_code, status),
            )
            .unwrap();
        }
        for (error_code, status) in [
            ("auth_required", 429),
            ("rate_limited", 503),
            ("provider_unavailable", 429),
            ("captcha_required", 401),
            ("source_untrusted", 401),
        ] {
            assert!(original_source_validate_observation_subject(
                &subject_json,
                &network_failure(error_code, status),
            )
            .is_err());
        }

        let prefetch_rejection = seal_observation(NormalizedOriginalSourceObservation {
            result: "mismatch".to_string(),
            mismatched_fields: vec!["original_url".to_string()],
            requested_url: None,
            retrieval_status: "preflight_rejected".to_string(),
            http_status: None,
            http_semantics_digest: original_source_empty_retrieval_digest("http-semantics"),
            redirect_chain_digest: original_source_empty_retrieval_digest("redirect-chain"),
            headers_digest: original_source_empty_retrieval_digest("headers"),
            content_digest: original_source_empty_retrieval_digest("content"),
            ..closed.clone()
        });
        original_source_validate_observation_subject(&subject_json, &prefetch_rejection).unwrap();

        let (assignment_parser_version, assignment_parser_digest) =
            original_source_parser_metadata("assignment");
        let invalid_assignment = seal_observation(NormalizedOriginalSourceObservation {
            result: "quarantined".to_string(),
            error_code: Some("invalid_assignment".to_string()),
            requested_url: None,
            retrieval_status: "preflight_rejected".to_string(),
            http_status: None,
            http_semantics_digest: original_source_empty_retrieval_digest("http-semantics"),
            redirect_chain_digest: original_source_empty_retrieval_digest("redirect-chain"),
            headers_digest: original_source_empty_retrieval_digest("headers"),
            content_digest: original_source_empty_retrieval_digest("content"),
            parser_version: assignment_parser_version,
            parser_digest: assignment_parser_digest,
            mismatched_fields: Vec::new(),
            ..closed.clone()
        });
        original_source_validate_observation_subject(&subject_json, &invalid_assignment).unwrap();

        let source_untrusted = seal_observation(NormalizedOriginalSourceObservation {
            result: "quarantined".to_string(),
            error_code: Some("source_untrusted".to_string()),
            requested_url: None,
            retrieval_status: "preflight_rejected".to_string(),
            http_status: None,
            http_semantics_digest: original_source_empty_retrieval_digest("http-semantics"),
            redirect_chain_digest: original_source_empty_retrieval_digest("redirect-chain"),
            headers_digest: original_source_empty_retrieval_digest("headers"),
            content_digest: original_source_empty_retrieval_digest("content"),
            mismatched_fields: Vec::new(),
            ..closed.clone()
        });
        original_source_validate_observation_subject(&subject_json, &source_untrusted).unwrap();

        let unreachable = seal_observation(NormalizedOriginalSourceObservation {
            result: "indeterminate".to_string(),
            error_code: Some("unreachable".to_string()),
            requested_url: Some(requested_url),
            retrieval_status: "unreachable".to_string(),
            http_status: None,
            http_semantics_digest: original_source_empty_retrieval_digest("http-semantics"),
            redirect_chain_digest: original_source_empty_retrieval_digest("redirect-chain"),
            headers_digest: original_source_empty_retrieval_digest("headers"),
            content_digest: original_source_empty_retrieval_digest("content"),
            mismatched_fields: Vec::new(),
            ..closed
        });
        original_source_validate_observation_subject(&subject_json, &unreachable).unwrap();
    }

    #[test]
    fn assignment_is_membership_bound_supersedes_and_account_delete_cascades() {
        let (pool, mut posting) = fixture_pool_and_posting();
        let (managed_authority, canonical_managed_authority_json, managed_authority_sha256) =
            fixture_managed_authority();
        let mut conn = pool.get().unwrap();
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        let first = ensure_original_source_verification_assignment_sqlite_bound_tx(
            &tx,
            "acct-original-source",
            &posting,
            &managed_authority,
            &canonical_managed_authority_json,
            &managed_authority_sha256,
        )
        .unwrap();
        tx.commit().unwrap();
        posting.title = "Staff Software Engineer".to_string();
        posting.availability_status = "expired".to_string();
        let mut conn = pool.get().unwrap();
        conn.execute(
            "UPDATE jobs_discovery_memberships SET availability_status='expired'",
            [],
        )
        .unwrap();
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        let second = ensure_original_source_verification_assignment_sqlite_bound_tx(
            &tx,
            "acct-original-source",
            &posting,
            &managed_authority,
            &canonical_managed_authority_json,
            &managed_authority_sha256,
        )
        .unwrap();
        assert_ne!(first.subject_sha256, second.subject_sha256);
        assert_eq!(first.assignment_generation, 1);
        assert_eq!(second.assignment_generation, 2);
        assert!(second.expires_at_ms > second.not_before_at_ms);
        assert_eq!(second.attempt_budget, ORIGINAL_SOURCE_ATTEMPT_BUDGET);
        assert_eq!(
            tx.query_row(
                "SELECT state FROM jobs_original_source_verification_assignments
                  WHERE assignment_id=?1",
                params![first.assignment_id],
                |row| row.get::<_, String>(0)
            )
            .unwrap(),
            "superseded"
        );
        tx.commit().unwrap();

        let mut conn = pool.get().unwrap();
        conn.execute(
            "UPDATE jobs_original_source_verification_assignments
                SET attempt_count=attempt_budget WHERE assignment_id=?1",
            params![second.assignment_id],
        )
        .unwrap();
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        let third = ensure_original_source_verification_assignment_sqlite_bound_tx(
            &tx,
            "acct-original-source",
            &posting,
            &managed_authority,
            &canonical_managed_authority_json,
            &managed_authority_sha256,
        )
        .unwrap();
        assert_eq!(third.assignment_generation, 3);
        assert_ne!(third.assignment_sha256, second.assignment_sha256);
        assert_eq!(
            tx.query_row(
                "SELECT predecessor_assignment_sha256
                   FROM jobs_original_source_verification_assignments
                  WHERE assignment_id=?1",
                params![third.assignment_id],
                |row| row.get::<_, String>(0),
            )
            .unwrap(),
            second.assignment_sha256
        );
        tx.commit().unwrap();

        let conn = pool.get().unwrap();
        conn.execute("DELETE FROM accounts WHERE id='acct-original-source'", [])
            .expect("account deletion must cascade immutable verification rows");
        assert_eq!(
            conn.query_row(
                "SELECT COUNT(*) FROM jobs_original_source_verification_assignments",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
            0
        );
    }

    #[test]
    fn assignment_rejects_missing_or_contradictory_discovery_membership() {
        let (pool, posting) = fixture_pool_and_posting();
        let (managed_authority, canonical_managed_authority_json, managed_authority_sha256) =
            fixture_managed_authority();
        let mut conn = pool.get().unwrap();
        conn.execute("DELETE FROM jobs_discovery_memberships", [])
            .unwrap();
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        assert!(matches!(
            ensure_original_source_verification_assignment_sqlite_bound_tx(
                &tx,
                "acct-original-source",
                &posting,
                &managed_authority,
                &canonical_managed_authority_json,
                &managed_authority_sha256,
            ),
            Err(OriginalSourceVerificationError::InvalidInput(_))
        ));
    }

    #[test]
    fn assignment_authority_expiry_and_revocation_are_typed_and_fail_closed() {
        for managed_error in [
            ManagedCloudRegistryError::Unavailable,
            ManagedCloudRegistryError::Revoked,
        ] {
            assert!(matches!(
                original_source_assignment_authority_error(managed_error),
                OriginalSourceVerificationError::ManagedRuntimeAuthorityUnavailable
            ));
        }
        for managed_error in [
            ManagedCloudRegistryError::InvalidAuthority,
            ManagedCloudRegistryError::IdentityConflict,
            ManagedCloudRegistryError::GrantExpired,
        ] {
            assert!(matches!(
                original_source_assignment_authority_error(managed_error),
                OriginalSourceVerificationError::Storage(_)
            ));
        }

        for denial in ["heartbeat_expired", "grant_revoked"] {
            let fixture = public_original_source_lifecycle_long_horizon_fixture();
            match denial {
                "heartbeat_expired" => {
                    expire_public_original_source_runtime_heartbeat(&fixture);
                }
                "grant_revoked" => {
                    revoke_managed_cloud_runtime_grant(
                        &fixture.pool,
                        &RevokeManagedCloudRuntimeGrant {
                            grant_id: fixture.grant_id.clone(),
                            reason_ref: "public-osv-assignment-authority-revoked-0001".to_string(),
                            revoked_by: "public-osv-lifecycle-test".to_string(),
                        },
                    )
                    .expect("revoke assignment-authority runtime grant");
                }
                _ => unreachable!("assignment-authority denial is closed"),
            }

            let mut connection = fixture
                .pool
                .get()
                .expect("inspect unavailable assignment authority");
            let tx = connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .expect("begin unavailable assignment-authority transaction");
            let authority_result = original_source_require_assignment_authority_sqlite(
                &tx,
                &fixture.assignment.assignment_id,
                &fixture.account_id,
            );
            assert!(
                matches!(
                    &authority_result,
                    Err(OriginalSourceVerificationError::ManagedRuntimeAuthorityUnavailable)
                ),
                "unexpected {denial} assignment-authority result: {authority_result:?}"
            );
            let candidate = fixture_candidate_sqlite(&tx, &fixture.assignment.assignment_id);
            assert_eq!(
                original_source_lease_candidate_decision_sqlite(&tx, &fixture.binding, &candidate,)
                    .expect("classify unavailable assignment authority"),
                OriginalSourceLeaseCandidateDecision::Supersede("managed_authority_revoked")
            );
            let now_ms = original_source_db_now_sqlite(&tx)
                .expect("unavailable assignment-authority database time");
            original_source_supersede_lease_candidate_sqlite(
                &tx,
                &candidate,
                "managed_authority_revoked",
                now_ms,
            )
            .expect("supersede unavailable assignment authority");
            tx.commit()
                .expect("commit unavailable assignment-authority denial");
            drop(connection);

            assert!(matches!(
                lease_original_source_verification(&fixture.pool, &fixture.binding),
                Err(OriginalSourceVerificationError::ManagedRuntimeAuthorityUnavailable)
            ));
            let connection = fixture
                .pool
                .get()
                .expect("assert unavailable assignment authority is fail closed");
            assert_eq!(
                connection
                    .query_row(
                        "SELECT state || ':' || last_error_code
                           FROM jobs_original_source_verification_assignments
                          WHERE assignment_id=?1",
                        params![fixture.assignment.assignment_id],
                        |row| row.get::<_, String>(0),
                    )
                    .unwrap(),
                "superseded:managed_authority_revoked"
            );
            assert_eq!(
                (
                    connection
                        .query_row(
                            "SELECT COUNT(*)
                               FROM jobs_original_source_verification_attempts
                              WHERE assignment_id=?1",
                            params![fixture.assignment.assignment_id],
                            |row| row.get::<_, i64>(0),
                        )
                        .unwrap(),
                    connection
                        .query_row(
                            "SELECT COUNT(*)
                               FROM jobs_original_source_verification_receipts
                              WHERE assignment_id=?1",
                            params![fixture.assignment.assignment_id],
                            |row| row.get::<_, i64>(0),
                        )
                        .unwrap(),
                ),
                (0, 0),
                "unavailable assignment authority must not create an attempt or receipt"
            );
            drop(connection);
            cleanup_public_original_source_lifecycle_fixture(fixture);
        }
    }

    #[test]
    fn lease_candidate_supersedes_paused_or_degraded_first_and_keeps_valid_second() {
        for health in ["paused", "degraded"] {
            let (pool, _, _, first_assignment, second_assignment) = fixture_two_assignments();
            let mut conn = pool.get().unwrap();
            conn.execute(
                "UPDATE jobs_discovery_sources SET health=?1 WHERE id='source-original'",
                params![health],
            )
            .unwrap();
            let tx = conn
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .unwrap();
            let first = fixture_candidate_sqlite(&tx, &first_assignment.assignment_id);
            assert_eq!(
                original_source_subject_recheck_decision(recheck_original_source_subject_sqlite(
                    &tx,
                    &first.account_id,
                    &first.job_id,
                    &first.subject_sha256,
                ))
                .unwrap(),
                OriginalSourceLeaseCandidateDecision::Supersede("source_untrusted")
            );
            original_source_supersede_lease_candidate_sqlite(
                &tx,
                &first,
                "source_untrusted",
                first.created_at_ms.saturating_add(1),
            )
            .unwrap();
            let second = fixture_candidate_sqlite(&tx, &second_assignment.assignment_id);
            assert_eq!(
                original_source_subject_recheck_decision(recheck_original_source_subject_sqlite(
                    &tx,
                    &second.account_id,
                    &second.job_id,
                    &second.subject_sha256,
                ))
                .unwrap(),
                OriginalSourceLeaseCandidateDecision::Lease
            );
            let event: (String, Option<String>) = tx
                .query_row(
                    "SELECT event_kind,reason_code
                       FROM jobs_original_source_verification_events
                      WHERE assignment_id=?1 ORDER BY event_sequence DESC LIMIT 1",
                    params![first.assignment_id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .unwrap();
            assert_eq!(
                event,
                (
                    "superseded".to_string(),
                    Some("source_untrusted".to_string())
                )
            );
            tx.commit().unwrap();
        }
    }

    #[test]
    fn lease_candidate_supersedes_revoked_subject_and_keeps_valid_second() {
        let (pool, mut first_posting, _, first_assignment, second_assignment) =
            fixture_two_assignments();
        first_posting.title = "Revoked subject mutation".to_string();
        let mut conn = pool.get().unwrap();
        conn.execute(
            "UPDATE jobs_postings SET posting_json=?2,title=?3
              WHERE account_id='acct-original-source' AND id=?1",
            params![
                first_posting.id,
                serde_json::to_string(&first_posting).unwrap(),
                first_posting.title,
            ],
        )
        .unwrap();
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        let first = fixture_candidate_sqlite(&tx, &first_assignment.assignment_id);
        assert_eq!(
            original_source_subject_recheck_decision(recheck_original_source_subject_sqlite(
                &tx,
                &first.account_id,
                &first.job_id,
                &first.subject_sha256,
            ))
            .unwrap(),
            OriginalSourceLeaseCandidateDecision::Supersede("subject_changed")
        );
        original_source_supersede_lease_candidate_sqlite(
            &tx,
            &first,
            "subject_changed",
            first.created_at_ms.saturating_add(1),
        )
        .unwrap();
        let second = fixture_candidate_sqlite(&tx, &second_assignment.assignment_id);
        assert_eq!(
            original_source_subject_recheck_decision(recheck_original_source_subject_sqlite(
                &tx,
                &second.account_id,
                &second.job_id,
                &second.subject_sha256,
            ))
            .unwrap(),
            OriginalSourceLeaseCandidateDecision::Lease
        );
        assert_eq!(
            tx.query_row(
                "SELECT reason_code FROM jobs_original_source_verification_events
                  WHERE assignment_id=?1 ORDER BY event_sequence DESC LIMIT 1",
                params![first.assignment_id],
                |row| row.get::<_, Option<String>>(0),
            )
            .unwrap()
            .as_deref(),
            Some("subject_changed")
        );
        tx.commit().unwrap();
    }

    #[test]
    fn lease_candidate_defers_expired_scoped_hold_without_repeat_events_and_recovers() {
        let (pool, _, _, first_assignment, second_assignment) = fixture_two_assignments();
        let held = AppendOperationalHoldEventRequest {
            event_id: "osv-source-hold-event-0001".to_string(),
            capability: OperationalCapability::OriginalSourceVerification,
            scope_kind: OperationalHoldScopeKind::DiscoverySource,
            scope_id: "source-original".to_string(),
            transition: OperationalHoldTransition::Held,
            reason_code: OperationalHoldReasonCode::ProviderOutage,
            reason_ref: Some("INC-OSV-LEASE-0001".to_string()),
            expected_head_revision: 0,
            expected_current_event_id: None,
        };
        append_operational_hold_event(&pool, &held, "osv-lease-test").unwrap();
        let mut conn = pool.get().unwrap();
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        let now = original_source_db_now_sqlite(&tx).unwrap();
        let expired_attempt_id = "attempt-held-expired-0001";
        tx.execute(
            "INSERT INTO jobs_original_source_verification_attempts(
                attempt_id,assignment_id,account_id,job_id,subject_sha256,
                attempt_no,worker_id,runtime_instance_id,runtime_instance_epoch,
                runtime_authority_sha256,runtime_session_token_sha256,
                lease_token_sha256,claimed_at_ms,initial_lease_expires_at_ms,
                hard_deadline_at_ms)
             VALUES(?1,?2,'acct-original-source',?3,?4,1,'worker-held-expired',
                    'runtime-held-expired',1,?5,?6,?7,?8,?9,?10)",
            params![
                expired_attempt_id,
                first_assignment.assignment_id,
                first_assignment.job_id,
                first_assignment.subject_sha256,
                "d".repeat(64),
                "e".repeat(64),
                "f".repeat(64),
                now.saturating_sub(120_000),
                now.saturating_sub(60_000),
                now.saturating_sub(30_000),
            ],
        )
        .unwrap();
        tx.execute(
            "UPDATE jobs_original_source_verification_assignments SET
                state='leased',attempt_count=1,active_attempt_id=?2,
                lease_owner='worker-held-expired',lease_token_sha256=?3,
                lease_expires_at_ms=?4,hard_deadline_at_ms=?5,updated_at_ms=?6
              WHERE assignment_id=?1",
            params![
                first_assignment.assignment_id,
                expired_attempt_id,
                "f".repeat(64),
                now.saturating_sub(60_000),
                now.saturating_sub(30_000),
                now,
            ],
        )
        .unwrap();
        let first = fixture_candidate_sqlite(&tx, &first_assignment.assignment_id);
        assert!(matches!(
            original_source_require_operational_hold_clear_sqlite(
                &tx,
                &first.account_id,
                &first.job_id,
            ),
            Err(OriginalSourceVerificationError::ManagedRuntimeAuthorityUnavailable)
        ));
        original_source_defer_operational_hold_sqlite(&tx, &first, now).unwrap();
        let second = fixture_candidate_sqlite(&tx, &second_assignment.assignment_id);
        original_source_require_operational_hold_clear_sqlite(
            &tx,
            &second.account_id,
            &second.job_id,
        )
        .unwrap();
        let deferred: (String, i64, Option<String>, i64) = tx
            .query_row(
                "SELECT state,attempt_count,last_error_code,next_attempt_at_ms
                   FROM jobs_original_source_verification_assignments
                  WHERE assignment_id=?1",
                params![first.assignment_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap();
        assert_eq!(deferred.0, "retry_wait");
        assert_eq!(deferred.1, 1);
        assert_eq!(deferred.2.as_deref(), Some("operational_hold"));
        assert_eq!(
            deferred.3,
            now.saturating_add(ORIGINAL_SOURCE_HOLD_RECHECK_MS)
        );
        let events = tx
            .prepare(
                "SELECT event_kind,attempt_id,reason_code
                   FROM jobs_original_source_verification_events
                  WHERE assignment_id=?1
                    AND event_kind IN ('lease_expired','released')
                  ORDER BY event_sequence",
            )
            .unwrap()
            .query_map(params![first.assignment_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                ))
            })
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        assert_eq!(
            events,
            vec![
                (
                    "lease_expired".to_string(),
                    Some(expired_attempt_id.to_string()),
                    Some("lease_expired".to_string()),
                ),
                (
                    "released".to_string(),
                    Some(expired_attempt_id.to_string()),
                    Some("operational_hold".to_string()),
                ),
            ]
        );
        let event_count = tx
            .query_row(
                "SELECT COUNT(*) FROM jobs_original_source_verification_events
                  WHERE assignment_id=?1",
                params![first.assignment_id],
                |row| row.get::<_, i64>(0),
            )
            .unwrap();
        let deferred_candidate = fixture_candidate_sqlite(&tx, &first.assignment_id);
        let rotated_at = now.saturating_add(ORIGINAL_SOURCE_HOLD_RECHECK_MS);
        original_source_rotate_deferred_hold_sqlite(&tx, &deferred_candidate, rotated_at, false)
            .unwrap();
        assert_eq!(
            tx.query_row(
                "SELECT COUNT(*) FROM jobs_original_source_verification_events
                  WHERE assignment_id=?1",
                params![first.assignment_id],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
            event_count,
            "a still-active hold must not emit another released event"
        );
        assert_eq!(
            tx.query_row(
                "SELECT next_attempt_at_ms
                   FROM jobs_original_source_verification_assignments
                  WHERE assignment_id=?1",
                params![first.assignment_id],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
            rotated_at.saturating_add(ORIGINAL_SOURCE_HOLD_RECHECK_MS)
        );
        tx.commit().unwrap();

        append_operational_hold_event(
            &pool,
            &AppendOperationalHoldEventRequest {
                event_id: "osv-source-hold-release-0001".to_string(),
                transition: OperationalHoldTransition::Released,
                reason_code: OperationalHoldReasonCode::ManualRelease,
                expected_head_revision: 1,
                expected_current_event_id: Some(held.event_id.clone()),
                ..held
            },
            "osv-lease-test",
        )
        .unwrap();
        let mut conn = pool.get().unwrap();
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        let first = fixture_candidate_sqlite(&tx, &first_assignment.assignment_id);
        original_source_require_operational_hold_clear_sqlite(
            &tx,
            &first.account_id,
            &first.job_id,
        )
        .unwrap();
        original_source_rotate_deferred_hold_sqlite(
            &tx,
            &first,
            rotated_at.saturating_add(ORIGINAL_SOURCE_HOLD_RECHECK_MS),
            true,
        )
        .unwrap();
        assert_eq!(
            tx.query_row(
                "SELECT state || ':' || COALESCE(last_error_code,'clear')
                   FROM jobs_original_source_verification_assignments
                  WHERE assignment_id=?1",
                params![first.assignment_id],
                |row| row.get::<_, String>(0),
            )
            .unwrap(),
            "pending:clear"
        );
        assert_eq!(
            tx.query_row(
                "SELECT event_kind || ':' || reason_code
                   FROM jobs_original_source_verification_events
                  WHERE assignment_id=?1 ORDER BY event_sequence DESC LIMIT 1",
                params![first.assignment_id],
                |row| row.get::<_, String>(0),
            )
            .unwrap(),
            "released:operational_hold_released"
        );
        tx.commit().unwrap();
    }

    #[test]
    fn lease_scan_reaches_valid_tail_beyond_held_page() {
        let candidates = (0..=ORIGINAL_SOURCE_LEASE_SCAN_LIMIT + 1)
            .map(|index| OriginalSourceLeaseCandidate {
                assignment_id: format!("assignment-{index:04}"),
                account_id: "account-cursor-test".to_string(),
                job_id: format!("job-{index:04}"),
                subject_sha256: "a".repeat(64),
                canonical_subject_json: "{}".to_string(),
                state: "pending".to_string(),
                attempt_count: 0,
                active_attempt_id: None,
                circuit_state: "closed".to_string(),
                assignment_expires_at_ms: 100,
                next_attempt_at_ms: 1,
                created_at_ms: index,
            })
            .collect::<Vec<_>>();
        struct ScanFixture {
            candidates: Vec<OriginalSourceLeaseCandidate>,
            deferred: std::collections::HashSet<String>,
            processed: usize,
        }
        let mut fixture = ScanFixture {
            candidates,
            deferred: std::collections::HashSet::new(),
            processed: 0,
        };
        let first = original_source_scan_lease_candidates(
            &mut fixture,
            |fixture, _| {
                Ok(fixture
                    .candidates
                    .iter()
                    .filter(|candidate| !fixture.deferred.contains(&candidate.assignment_id))
                    .take(ORIGINAL_SOURCE_LEASE_SCAN_LIMIT as usize)
                    .cloned()
                    .collect())
            },
            |_, candidate| Ok(Some(candidate.clone())),
            |fixture, candidate| {
                fixture.processed += 1;
                if candidate.created_at_ms <= ORIGINAL_SOURCE_LEASE_SCAN_LIMIT {
                    Ok(OriginalSourceLeaseCandidateDecision::DeferOperationalHold)
                } else {
                    Ok(OriginalSourceLeaseCandidateDecision::Lease)
                }
            },
            |fixture, candidate| {
                fixture.deferred.insert(candidate.assignment_id.clone());
                Ok(())
            },
            |_, _, _| unreachable!("held candidates must not be superseded"),
            |_, candidate| Ok(candidate.assignment_id.clone()),
        )
        .unwrap();
        assert!(first.is_none());
        assert_eq!(fixture.processed, ORIGINAL_SOURCE_LEASE_SCAN_LIMIT as usize);
        assert_eq!(
            fixture.deferred.len(),
            ORIGINAL_SOURCE_LEASE_SCAN_LIMIT as usize
        );

        fixture.processed = 0;
        let second = original_source_scan_lease_candidates(
            &mut fixture,
            |fixture, _| {
                Ok(fixture
                    .candidates
                    .iter()
                    .filter(|candidate| !fixture.deferred.contains(&candidate.assignment_id))
                    .take(ORIGINAL_SOURCE_LEASE_SCAN_LIMIT as usize)
                    .cloned()
                    .collect())
            },
            |_, candidate| Ok(Some(candidate.clone())),
            |fixture, candidate| {
                fixture.processed += 1;
                if candidate.created_at_ms <= ORIGINAL_SOURCE_LEASE_SCAN_LIMIT {
                    Ok(OriginalSourceLeaseCandidateDecision::DeferOperationalHold)
                } else {
                    Ok(OriginalSourceLeaseCandidateDecision::Lease)
                }
            },
            |fixture, candidate| {
                fixture.deferred.insert(candidate.assignment_id.clone());
                Ok(())
            },
            |_, _, _| unreachable!("held candidates must not be superseded"),
            |_, candidate| Ok(candidate.assignment_id.clone()),
        )
        .unwrap();
        assert_eq!(second.as_deref(), Some("assignment-0033"));
        assert_eq!(fixture.processed, 2);
        assert_eq!(
            fixture.deferred.len(),
            ORIGINAL_SOURCE_LEASE_SCAN_LIMIT as usize + 1
        );
    }

    #[test]
    fn lease_scan_reclaims_expired_active_attempt_before_valid_tail() {
        let (pool, _, _, first_assignment, second_assignment) = fixture_two_assignments();
        let binding = OriginalSourceVerifierBinding {
            worker_id: "worker-original-source-test".to_string(),
            runtime_instance_id: "runtime-original-source-test".to_string(),
            runtime_instance_epoch: 1,
            runtime_authority_sha256: "d".repeat(64),
            runtime_session_token: "session-token-original-source-test-0001".to_string(),
        };
        let mut conn = pool.get().unwrap();
        let mut tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        let now = original_source_db_now_sqlite(&tx).unwrap();
        let old_attempt_id = "attempt-original-source-expired-0001";
        let old_lease_sha256 = "e".repeat(64);
        tx.execute(
            "INSERT INTO jobs_original_source_verification_attempts(
                attempt_id,assignment_id,account_id,job_id,subject_sha256,
                attempt_no,worker_id,runtime_instance_id,runtime_instance_epoch,
                runtime_authority_sha256,runtime_session_token_sha256,
                lease_token_sha256,claimed_at_ms,initial_lease_expires_at_ms,
                hard_deadline_at_ms)
             VALUES(?1,?2,'acct-original-source',?3,?4,1,?5,?6,1,?7,?8,?9,
                    ?10,?11,?12)",
            params![
                old_attempt_id,
                first_assignment.assignment_id,
                first_assignment.job_id,
                first_assignment.subject_sha256,
                binding.worker_id,
                binding.runtime_instance_id,
                binding.runtime_authority_sha256,
                original_source_sha256(binding.runtime_session_token.as_bytes()),
                old_lease_sha256,
                now.saturating_sub(120_000),
                now.saturating_sub(60_000),
                now.saturating_sub(30_000),
            ],
        )
        .unwrap();
        tx.execute(
            "UPDATE jobs_original_source_verification_assignments SET
                state='leased',attempt_count=1,active_attempt_id=?2,
                lease_owner=?3,lease_token_sha256=?4,
                lease_expires_at_ms=?5,hard_deadline_at_ms=?6,
                heartbeat_sequence=0,updated_at_ms=?7
              WHERE assignment_id=?1",
            params![
                first_assignment.assignment_id,
                old_attempt_id,
                binding.worker_id,
                old_lease_sha256,
                now.saturating_sub(60_000),
                now.saturating_sub(30_000),
                now,
            ],
        )
        .unwrap();
        let expired = fixture_candidate_sqlite(&tx, &first_assignment.assignment_id);
        let tail = fixture_candidate_sqlite(&tx, &second_assignment.assignment_id);
        let lease_token = original_source_new_lease_token();
        let lease_token_sha256 = original_source_sha256(lease_token.as_bytes());
        let session_sha256 = original_source_sha256(binding.runtime_session_token.as_bytes());
        let mut page = Some(vec![expired, tail]);
        let lease = original_source_scan_lease_candidates(
            &mut tx,
            |_, _| Ok(page.take().unwrap_or_default()),
            |_, candidate| Ok(Some(candidate.clone())),
            |_, _| Ok(OriginalSourceLeaseCandidateDecision::Lease),
            |_, _| unreachable!("unheld expired lease must not defer"),
            |_, _, _| unreachable!("valid expired lease must not supersede"),
            |tx, candidate| {
                original_source_claim_lease_candidate_sqlite(
                    tx,
                    &binding,
                    candidate,
                    &lease_token,
                    &lease_token_sha256,
                    &session_sha256,
                    now,
                )
            },
        )
        .unwrap()
        .expect("reclaimed expired lease");
        assert_eq!(lease.assignment_id, first_assignment.assignment_id);
        assert_eq!(lease.fence, 2);
        assert_ne!(lease.attempt_id, old_attempt_id);
        let events = tx
            .prepare(
                "SELECT event_kind,attempt_id
                   FROM jobs_original_source_verification_events
                  WHERE assignment_id=?1 AND event_kind IN ('lease_expired','claimed')
                  ORDER BY event_sequence DESC LIMIT 2",
            )
            .unwrap()
            .query_map(params![lease.assignment_id], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
            })
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        assert_eq!(events[0], ("claimed".to_string(), Some(lease.attempt_id)));
        assert_eq!(
            events[1],
            (
                "lease_expired".to_string(),
                Some(old_attempt_id.to_string())
            )
        );
        assert_eq!(
            tx.query_row(
                "SELECT state FROM jobs_original_source_verification_assignments
                  WHERE assignment_id=?1",
                params![second_assignment.assignment_id],
                |row| row.get::<_, String>(0),
            )
            .unwrap(),
            "pending"
        );
        tx.commit().unwrap();
    }

    #[test]
    fn public_sqlite_lease_reclaims_expired_attempt_with_strict_runtime_authority() {
        let fixture = super::managed_cloud_release_authority_tests::
            original_source_verifier_runtime_test_fixture();
        let database_path = fixture.database_path.clone();
        let pool = fixture.pool.clone();
        let account_id = fixture.account_id.clone();
        let binding = OriginalSourceVerifierBinding {
            worker_id: fixture.worker_id.clone(),
            runtime_instance_id: fixture.runtime_instance_id.clone(),
            runtime_instance_epoch: fixture.runtime_instance_epoch,
            runtime_authority_sha256: fixture.runtime_authority_sha256.clone(),
            runtime_session_token: fixture.runtime_session_token.clone(),
        };
        {
            let conn = pool.get().expect("get strict-runtime fixture account");
            conn.execute(
                "INSERT INTO accounts(id,email,password_hash,trial_seconds_remaining)
                 VALUES(?1,?2,'hash',0)",
                params![account_id, "strict-runtime-osv@example.test"],
            )
            .expect("insert strict-runtime fixture account");
        }
        let first_posting = fixture_posting(
            "greenhouse_import",
            "https://boards.greenhouse.io/acme/jobs/strict-runtime-first",
            "strict-runtime-first",
        );
        let second_posting = fixture_posting(
            "lever_import",
            "https://jobs.lever.co/bravo/strict-runtime-second",
            "strict-runtime-second",
        );
        insert_fixture_posting_source_for_account(
            &pool,
            &account_id,
            &first_posting,
            "source-strict-runtime-first",
            "greenhouse",
            "acme",
        );
        insert_fixture_posting_source_for_account(
            &pool,
            &account_id,
            &second_posting,
            "source-strict-runtime-second",
            "lever",
            "bravo",
        );

        let old_attempt_id = "attempt-strict-runtime-expired-0001";
        let old_lease_sha256 = "e".repeat(64);
        let (first_assignment, second_assignment) = {
            let mut conn = pool
                .get()
                .expect("get strict-runtime assignment connection");
            let tx = conn
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .expect("begin strict-runtime assignment transaction");
            let authority =
                sqlite_original_source_verification_authority_for_account_tx(&tx, &account_id)
                    .expect("resolve strict original-source authority")
                    .expect("strict original-source authority is active");
            let first_assignment =
                ensure_original_source_verification_assignment_sqlite_with_authority_tx(
                    &tx,
                    &account_id,
                    &first_posting,
                    &authority,
                )
                .expect("schedule strict-runtime first assignment");
            let second_assignment =
                ensure_original_source_verification_assignment_sqlite_with_authority_tx(
                    &tx,
                    &account_id,
                    &second_posting,
                    &authority,
                )
                .expect("schedule strict-runtime second assignment");
            let now = original_source_db_now_sqlite(&tx).expect("strict-runtime database time");
            let oldest_assignment_id = tx
                .query_row(
                    "SELECT assignment_id
                       FROM jobs_original_source_verification_assignments
                      WHERE assignment_id IN (?1,?2)
                      ORDER BY next_attempt_at_ms,created_at_ms,assignment_id
                      LIMIT 1",
                    params![
                        first_assignment.assignment_id,
                        second_assignment.assignment_id
                    ],
                    |row| row.get::<_, String>(0),
                )
                .expect("resolve strict-runtime oldest assignment");
            let (expired_assignment, tail_assignment) =
                if oldest_assignment_id == first_assignment.assignment_id {
                    (first_assignment, second_assignment)
                } else {
                    (second_assignment, first_assignment)
                };
            tx.execute(
                "INSERT INTO jobs_original_source_verification_attempts(
                    attempt_id,assignment_id,account_id,job_id,subject_sha256,
                    attempt_no,worker_id,runtime_instance_id,runtime_instance_epoch,
                    runtime_authority_sha256,runtime_session_token_sha256,
                    lease_token_sha256,claimed_at_ms,initial_lease_expires_at_ms,
                    hard_deadline_at_ms)
                 VALUES(?1,?2,?3,?4,?5,1,?6,?7,?8,?9,?10,?11,?12,?13,?14)",
                params![
                    old_attempt_id,
                    expired_assignment.assignment_id,
                    account_id,
                    expired_assignment.job_id,
                    expired_assignment.subject_sha256,
                    binding.worker_id,
                    binding.runtime_instance_id,
                    binding.runtime_instance_epoch,
                    binding.runtime_authority_sha256,
                    original_source_sha256(binding.runtime_session_token.as_bytes()),
                    old_lease_sha256,
                    now.saturating_sub(120_000),
                    now.saturating_sub(60_000),
                    now.saturating_sub(30_000),
                ],
            )
            .expect("persist strict-runtime expired attempt");
            tx.execute(
                "UPDATE jobs_original_source_verification_assignments SET
                    state='leased',attempt_count=1,active_attempt_id=?2,
                    lease_owner=?3,lease_token_sha256=?4,
                    lease_expires_at_ms=?5,hard_deadline_at_ms=?6,
                    heartbeat_sequence=0,updated_at_ms=?7
                  WHERE assignment_id=?1",
                params![
                    expired_assignment.assignment_id,
                    old_attempt_id,
                    binding.worker_id,
                    old_lease_sha256,
                    now.saturating_sub(60_000),
                    now.saturating_sub(30_000),
                    now,
                ],
            )
            .expect("expire strict-runtime first assignment lease");
            tx.commit().expect("commit strict-runtime assignments");
            (expired_assignment, tail_assignment)
        };

        let lease = lease_original_source_verification(&pool, &binding)
            .expect("lease through public strict-runtime entrypoint")
            .expect("expired first assignment is reclaimed");
        assert_eq!(lease.assignment_id, first_assignment.assignment_id);
        assert_eq!(lease.fence, 2);
        assert_ne!(lease.attempt_id, old_attempt_id);
        {
            let conn = pool.get().expect("assert strict-runtime public lease");
            let events = conn
                .prepare(
                    "SELECT event_kind,attempt_id
                       FROM jobs_original_source_verification_events
                      WHERE assignment_id=?1
                        AND event_kind IN ('lease_expired','claimed')
                      ORDER BY event_sequence DESC LIMIT 2",
                )
                .unwrap()
                .query_map(params![lease.assignment_id], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
                })
                .unwrap()
                .collect::<rusqlite::Result<Vec<_>>>()
                .unwrap();
            assert_eq!(
                events,
                vec![
                    ("claimed".to_string(), Some(lease.attempt_id.clone())),
                    (
                        "lease_expired".to_string(),
                        Some(old_attempt_id.to_string()),
                    ),
                ]
            );
            assert_eq!(
                conn.query_row(
                    "SELECT state FROM jobs_original_source_verification_assignments
                      WHERE assignment_id=?1",
                    params![second_assignment.assignment_id],
                    |row| row.get::<_, String>(0),
                )
                .unwrap(),
                "pending"
            );
        }
        drop(pool);
        drop(fixture);
        let _ = std::fs::remove_file(&database_path);
        let _ = std::fs::remove_file(database_path.with_extension("sqlite3-wal"));
        let _ = std::fs::remove_file(database_path.with_extension("sqlite3-shm"));
    }

    #[test]
    fn public_sqlite_lease_defers_held_prefix_with_bounded_fair_recovery() {
        let fixture = super::managed_cloud_release_authority_tests::
            original_source_verifier_runtime_long_horizon_test_fixture();
        let database_path = fixture.database_path.clone();
        let pool = fixture.pool.clone();
        let account_id = fixture.account_id.clone();
        let binding = OriginalSourceVerifierBinding {
            worker_id: fixture.worker_id.clone(),
            runtime_instance_id: fixture.runtime_instance_id.clone(),
            runtime_instance_epoch: fixture.runtime_instance_epoch,
            runtime_authority_sha256: fixture.runtime_authority_sha256.clone(),
            runtime_session_token: fixture.runtime_session_token.clone(),
        };
        {
            let conn = pool.get().expect("get held-prefix fixture account");
            conn.execute(
                "INSERT INTO accounts(id,email,password_hash,trial_seconds_remaining)
                 VALUES(?1,?2,'hash',0)",
                params![account_id, "held-prefix-osv@example.test"],
            )
            .expect("insert held-prefix fixture account");
        }

        let mut held_postings = Vec::new();
        for index in 0..=ORIGINAL_SOURCE_LEASE_SCAN_LIMIT {
            let external_id = format!("held-prefix-{index:04}");
            let source_key = format!("heldco{index:04}");
            let source_id = format!("source-held-prefix-{index:04}");
            let posting = fixture_posting(
                "greenhouse_import",
                &format!("https://boards.greenhouse.io/{source_key}/jobs/{external_id}"),
                &external_id,
            );
            insert_fixture_posting_source_for_account(
                &pool,
                &account_id,
                &posting,
                &source_id,
                "greenhouse",
                &source_key,
            );
            held_postings.push((posting, source_id));
        }
        let valid_posting = fixture_posting(
            "lever_import",
            "https://jobs.lever.co/validtail/public-held-tail",
            "public-held-tail",
        );
        insert_fixture_posting_source_for_account(
            &pool,
            &account_id,
            &valid_posting,
            "source-public-held-valid-tail",
            "lever",
            "validtail",
        );

        let old_attempt_id = "attempt-public-held-expired-0001";
        let old_lease_sha256 = "e".repeat(64);
        let (held_assignments, valid_assignment) = {
            let mut conn = pool.get().expect("get held-prefix assignment connection");
            let tx = conn
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .expect("begin held-prefix assignment transaction");
            let authority =
                sqlite_original_source_verification_authority_for_account_tx(&tx, &account_id)
                    .expect("resolve held-prefix original-source authority")
                    .expect("held-prefix original-source authority is active");
            let mut held_assignments = Vec::new();
            for (posting, _) in &held_postings {
                let assignment =
                    ensure_original_source_verification_assignment_sqlite_with_authority_tx(
                        &tx,
                        &account_id,
                        posting,
                        &authority,
                    )
                    .expect("schedule held-prefix assignment");
                held_assignments.push(assignment);
            }
            let valid_assignment =
                ensure_original_source_verification_assignment_sqlite_with_authority_tx(
                    &tx,
                    &account_id,
                    &valid_posting,
                    &authority,
                )
                .expect("schedule held-prefix valid tail");
            let now = original_source_db_now_sqlite(&tx).expect("held-prefix database time");
            for (index, assignment) in held_assignments.iter().enumerate() {
                tx.execute(
                    "UPDATE jobs_original_source_verification_assignments
                        SET next_attempt_at_ms=?2
                      WHERE assignment_id=?1",
                    params![
                        assignment.assignment_id,
                        now.saturating_add(i64::try_from(index).unwrap())
                    ],
                )
                .expect("order held-prefix assignment");
            }
            tx.execute(
                "UPDATE jobs_original_source_verification_assignments
                    SET next_attempt_at_ms=?2
                  WHERE assignment_id=?1",
                params![
                    valid_assignment.assignment_id,
                    now.saturating_add(ORIGINAL_SOURCE_LEASE_SCAN_LIMIT + 1)
                ],
            )
            .expect("order held-prefix valid tail");
            let expired = &held_assignments[0];
            tx.execute(
                "INSERT INTO jobs_original_source_verification_attempts(
                    attempt_id,assignment_id,account_id,job_id,subject_sha256,
                    attempt_no,worker_id,runtime_instance_id,runtime_instance_epoch,
                    runtime_authority_sha256,runtime_session_token_sha256,
                    lease_token_sha256,claimed_at_ms,initial_lease_expires_at_ms,
                    hard_deadline_at_ms)
                 VALUES(?1,?2,?3,?4,?5,1,?6,?7,?8,?9,?10,?11,?12,?13,?14)",
                params![
                    old_attempt_id,
                    expired.assignment_id,
                    account_id,
                    expired.job_id,
                    expired.subject_sha256,
                    binding.worker_id,
                    binding.runtime_instance_id,
                    binding.runtime_instance_epoch,
                    binding.runtime_authority_sha256,
                    original_source_sha256(binding.runtime_session_token.as_bytes()),
                    old_lease_sha256,
                    now.saturating_sub(120_000),
                    now.saturating_sub(60_000),
                    now.saturating_sub(30_000),
                ],
            )
            .expect("persist held-prefix expired attempt");
            tx.execute(
                "UPDATE jobs_original_source_verification_assignments SET
                    state='leased',attempt_count=1,active_attempt_id=?2,
                    lease_owner=?3,lease_token_sha256=?4,
                    lease_expires_at_ms=?5,hard_deadline_at_ms=?6,
                    heartbeat_sequence=0,updated_at_ms=?7
                  WHERE assignment_id=?1",
                params![
                    expired.assignment_id,
                    old_attempt_id,
                    binding.worker_id,
                    old_lease_sha256,
                    now.saturating_sub(60_000),
                    now.saturating_sub(30_000),
                    now,
                ],
            )
            .expect("expire first held-prefix assignment lease");
            tx.commit().expect("commit held-prefix assignments");
            (held_assignments, valid_assignment)
        };

        let mut recovery_hold = None;
        for (index, (_, source_id)) in held_postings.iter().enumerate() {
            let held = AppendOperationalHoldEventRequest {
                event_id: format!("osv-held-prefix-event-{index:04}"),
                capability: OperationalCapability::OriginalSourceVerification,
                scope_kind: OperationalHoldScopeKind::DiscoverySource,
                scope_id: source_id.clone(),
                transition: OperationalHoldTransition::Held,
                reason_code: OperationalHoldReasonCode::ProviderOutage,
                reason_ref: Some(format!("INC-OSV-HELD-PREFIX-{index:04}")),
                expected_head_revision: 0,
                expected_current_event_id: None,
            };
            append_operational_hold_event(&pool, &held, "osv-public-held-prefix")
                .expect("hold held-prefix discovery source");
            if index == 1 {
                recovery_hold = Some(held);
            }
        }
        let recovery_hold = recovery_hold.expect("held-prefix recovery source");
        std::thread::sleep(std::time::Duration::from_millis(
            u64::try_from(ORIGINAL_SOURCE_LEASE_SCAN_LIMIT + 2).unwrap(),
        ));

        refresh_public_original_source_runtime_heartbeat(&pool, &binding);

        assert!(
            lease_original_source_verification(&pool, &binding)
                .expect("bounded first held-prefix lease scan")
                .is_none(),
            "the first call must stop after the 32-candidate normal budget"
        );
        {
            let conn = pool.get().expect("assert first held-prefix lease scan");
            assert_eq!(
                conn.query_row(
                    "SELECT COUNT(*)
                       FROM jobs_original_source_verification_assignments
                      WHERE account_id=?1 AND state='retry_wait'
                        AND last_error_code='operational_hold'",
                    params![account_id],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
                ORIGINAL_SOURCE_LEASE_SCAN_LIMIT
            );
            assert_eq!(
                conn.query_row(
                    "SELECT COUNT(*)
                       FROM jobs_original_source_verification_events event
                      WHERE event.assignment_id IN (
                        SELECT assignment_id
                          FROM jobs_original_source_verification_assignments
                         WHERE account_id=?1)
                        AND event.event_kind='released'
                        AND event.reason_code='operational_hold'",
                    params![account_id],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
                ORIGINAL_SOURCE_LEASE_SCAN_LIMIT
            );
            let expired_events = conn
                .prepare(
                    "SELECT event_kind,attempt_id,reason_code
                       FROM jobs_original_source_verification_events
                      WHERE assignment_id=?1
                        AND event_kind IN ('lease_expired','released')
                      ORDER BY event_sequence",
                )
                .unwrap()
                .query_map(params![held_assignments[0].assignment_id], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, Option<String>>(2)?,
                    ))
                })
                .unwrap()
                .collect::<rusqlite::Result<Vec<_>>>()
                .unwrap();
            assert_eq!(
                expired_events,
                vec![
                    (
                        "lease_expired".to_string(),
                        Some(old_attempt_id.to_string()),
                        Some("lease_expired".to_string()),
                    ),
                    (
                        "released".to_string(),
                        Some(old_attempt_id.to_string()),
                        Some("operational_hold".to_string()),
                    ),
                ]
            );
            assert_eq!(
                conn.query_row(
                    "SELECT attempt_count
                       FROM jobs_original_source_verification_assignments
                      WHERE assignment_id=?1",
                    params![held_assignments[0].assignment_id],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
                1,
                "hold deferral must not consume another attempt"
            );
        }

        let valid_lease = lease_original_source_verification(&pool, &binding)
            .expect("second held-prefix lease scan")
            .expect("second call reaches the valid tail");
        assert_eq!(valid_lease.assignment_id, valid_assignment.assignment_id);
        assert_eq!(valid_lease.fence, 1);
        let recovery_assignment = &held_assignments[1];
        let event_count_before_rotation = {
            let conn = pool.get().expect("prepare held-prefix rotation");
            conn.execute(
                "UPDATE jobs_original_source_verification_assignments
                    SET next_attempt_at_ms=not_before_at_ms
                  WHERE assignment_id=?1",
                params![recovery_assignment.assignment_id],
            )
            .expect("make held-prefix deferral due");
            assert_eq!(
                conn.query_row(
                    "SELECT COUNT(*)
                       FROM jobs_original_source_verification_assignments
                      WHERE account_id=?1 AND state='retry_wait'
                        AND last_error_code='operational_hold'",
                    params![account_id],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
                ORIGINAL_SOURCE_LEASE_SCAN_LIMIT + 1
            );
            conn.query_row(
                "SELECT COUNT(*) FROM jobs_original_source_verification_events
                  WHERE assignment_id=?1",
                params![recovery_assignment.assignment_id],
                |row| row.get::<_, i64>(0),
            )
            .unwrap()
        };
        assert!(lease_original_source_verification(&pool, &binding)
            .expect("recheck still-held deferred assignment")
            .is_none());
        {
            let conn = pool.get().expect("assert still-held rotation");
            assert_eq!(
                conn.query_row(
                    "SELECT COUNT(*) FROM jobs_original_source_verification_events
                      WHERE assignment_id=?1",
                    params![recovery_assignment.assignment_id],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
                event_count_before_rotation,
                "still-held reconciliation must not emit a false release event"
            );
            assert!(
                conn.query_row(
                    "SELECT next_attempt_at_ms
                       FROM jobs_original_source_verification_assignments
                      WHERE assignment_id=?1",
                    params![recovery_assignment.assignment_id],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap()
                    > 0
            );
        }

        append_operational_hold_event(
            &pool,
            &AppendOperationalHoldEventRequest {
                event_id: "osv-held-prefix-release-0001".to_string(),
                transition: OperationalHoldTransition::Released,
                reason_code: OperationalHoldReasonCode::ManualRelease,
                expected_head_revision: 1,
                expected_current_event_id: Some(recovery_hold.event_id.clone()),
                ..recovery_hold
            },
            "osv-public-held-prefix",
        )
        .expect("release held-prefix recovery source");
        {
            let conn = pool.get().expect("make released deferral due");
            conn.execute(
                "UPDATE jobs_original_source_verification_assignments
                    SET next_attempt_at_ms=not_before_at_ms
                  WHERE assignment_id=?1",
                params![recovery_assignment.assignment_id],
            )
            .expect("make released held-prefix assignment due");
        }
        let recovered_lease = lease_original_source_verification(&pool, &binding)
            .expect("lease after held-prefix source release")
            .expect("released held-prefix assignment recovers");
        assert_eq!(
            recovered_lease.assignment_id,
            recovery_assignment.assignment_id
        );
        assert_eq!(recovered_lease.fence, 1);
        {
            let conn = pool.get().expect("assert held-prefix recovery events");
            let recovery_events = conn
                .prepare(
                    "SELECT event_kind,reason_code
                       FROM jobs_original_source_verification_events
                      WHERE assignment_id=?1
                        AND (reason_code='operational_hold_released'
                          OR event_kind='claimed')
                      ORDER BY event_sequence",
                )
                .unwrap()
                .query_map(params![recovery_assignment.assignment_id], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
                })
                .unwrap()
                .collect::<rusqlite::Result<Vec<_>>>()
                .unwrap();
            assert_eq!(
                recovery_events,
                vec![
                    (
                        "released".to_string(),
                        Some("operational_hold_released".to_string()),
                    ),
                    ("claimed".to_string(), None),
                ]
            );
        }

        drop(pool);
        drop(fixture);
        let _ = std::fs::remove_file(&database_path);
        let _ = std::fs::remove_file(database_path.with_extension("sqlite3-wal"));
        let _ = std::fs::remove_file(database_path.with_extension("sqlite3-shm"));
    }

    #[test]
    fn public_sqlite_lifecycle_replays_exact_terminal_and_quarantines_changed_bytes() {
        let fixture = public_original_source_lifecycle_fixture();
        let lease = lease_original_source_verification(&fixture.pool, &fixture.binding)
            .expect("lease public lifecycle assignment")
            .expect("public lifecycle assignment is due");
        assert_eq!(lease.assignment_id, fixture.assignment.assignment_id);
        assert_eq!(lease.fence, 1);

        let heartbeat_request = OriginalSourceVerificationHeartbeatRequest {
            binding: fixture.binding.clone(),
            assignment_id: lease.assignment_id.clone(),
            attempt_id: lease.attempt_id.clone(),
            fence: lease.fence,
            lease_token: lease.lease_token.clone(),
            heartbeat_sequence: 1,
        };
        let heartbeat = heartbeat_original_source_verification(&fixture.pool, &heartbeat_request)
            .expect("heartbeat public lifecycle lease");
        assert_eq!(heartbeat.heartbeat_sequence, 1);
        assert!(!heartbeat.replayed);
        let heartbeat_replay =
            heartbeat_original_source_verification(&fixture.pool, &heartbeat_request)
                .expect("replay public lifecycle heartbeat");
        assert!(heartbeat_replay.replayed);
        assert_eq!(
            heartbeat_replay.lease_expires_at_ms,
            heartbeat.lease_expires_at_ms
        );

        let completion =
            public_completion_request(&fixture, &lease, "public-osv-lifecycle-completion-0001");
        let terminal = complete_original_source_verification(&fixture.pool, &completion)
            .expect("complete public lifecycle lease");
        assert_eq!(terminal.state, "idle");
        assert!(!terminal.replayed);
        let original_head = terminal
            .head
            .clone()
            .expect("positive public lifecycle head");
        assert_eq!(original_head.result, "verified_open");
        let projection = resolve_original_source_verification_projection(
            &fixture.pool,
            &fixture.account_id,
            &fixture.posting,
        )
        .expect("resolve positive public lifecycle projection");
        assert!(projection.evidence.is_some());
        let expected_head = projection
            .expected_head
            .expect("positive public lifecycle expected head");
        let counts_before_replay = {
            let conn = fixture
                .pool
                .get()
                .expect("count public lifecycle publication");
            (
                conn.query_row(
                    "SELECT COUNT(*) FROM jobs_original_source_verification_observations
                      WHERE assignment_id=?1",
                    params![lease.assignment_id],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
                conn.query_row(
                    "SELECT COUNT(*) FROM jobs_original_source_verification_receipts
                      WHERE assignment_id=?1",
                    params![lease.assignment_id],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
                conn.query_row(
                    "SELECT COUNT(*) FROM jobs_original_source_verification_transitions
                      WHERE assignment_id=?1",
                    params![lease.assignment_id],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
                conn.query_row(
                    "SELECT COUNT(*) FROM jobs_original_source_verification_events
                      WHERE assignment_id=?1 AND event_kind='heartbeat'",
                    params![lease.assignment_id],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
            )
        };
        assert_eq!(counts_before_replay, (1, 1, 1, 1));

        let replay = complete_original_source_verification(&fixture.pool, &completion)
            .expect("replay byte-identical public lifecycle terminal");
        assert!(replay.replayed);
        assert_eq!(replay.receipt_sha256, terminal.receipt_sha256);
        assert_eq!(replay.head, terminal.head);
        let counts_after_replay = {
            let conn = fixture.pool.get().expect("count exact terminal replay");
            (
                conn.query_row(
                    "SELECT COUNT(*) FROM jobs_original_source_verification_observations
                      WHERE assignment_id=?1",
                    params![lease.assignment_id],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
                conn.query_row(
                    "SELECT COUNT(*) FROM jobs_original_source_verification_receipts
                      WHERE assignment_id=?1",
                    params![lease.assignment_id],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
                conn.query_row(
                    "SELECT COUNT(*) FROM jobs_original_source_verification_transitions
                      WHERE assignment_id=?1",
                    params![lease.assignment_id],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
                conn.query_row(
                    "SELECT COUNT(*) FROM jobs_original_source_verification_events
                      WHERE assignment_id=?1 AND event_kind='heartbeat'",
                    params![lease.assignment_id],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
            )
        };
        assert_eq!(counts_after_replay, counts_before_replay);

        let mut changed = completion.clone();
        changed.observation.headers_digest = "3".repeat(64);
        let changed_normalized = original_source_normalize_observation(&changed.observation)
            .expect("normalize changed-byte public lifecycle replay");
        changed.observation.evidence_sha256 =
            original_source_observation_evidence_sha256(&changed_normalized)
                .expect("seal changed-byte public lifecycle replay");
        assert!(matches!(
            complete_original_source_verification(&fixture.pool, &changed),
            Err(OriginalSourceVerificationError::ConflictingReplay)
        ));
        let quarantined_head = resolve_original_source_verification_head(
            &fixture.pool,
            &fixture.account_id,
            &fixture.posting.id,
        )
        .expect("resolve quarantined public lifecycle head")
        .expect("published public lifecycle head remains auditable");
        assert_eq!(quarantined_head.assignment_state, "quarantined");
        assert!(matches!(
            compare_original_source_verification_head(
                &quarantined_head,
                &expected_head,
                projection.db_time_ms,
            ),
            Err(OriginalSourceVerificationError::ConcurrentHeadAdvance)
        ));
        let quarantined_projection = resolve_original_source_verification_projection(
            &fixture.pool,
            &fixture.account_id,
            &fixture.posting,
        )
        .expect("resolve quarantined public lifecycle projection");
        assert!(quarantined_projection.evidence.is_none());
        assert!(quarantined_projection.expected_head.is_none());
        assert!(resolve_original_source_verification_evidence(
            &fixture.pool,
            &fixture.account_id,
            &fixture.posting,
            quarantined_projection.db_time_ms,
        )
        .expect("resolve quarantined public lifecycle evidence")
        .is_none());

        revoke_managed_cloud_runtime_grant(
            &fixture.pool,
            &RevokeManagedCloudRuntimeGrant {
                grant_id: fixture.grant_id.clone(),
                reason_ref: "public-osv-response-loss-revocation-0001".to_string(),
                revoked_by: "public-osv-lifecycle-test".to_string(),
            },
        )
        .expect("revoke runtime after committed public lifecycle terminal");
        let replay_after_revocation =
            complete_original_source_verification(&fixture.pool, &completion)
                .expect("exact terminal replay survives later runtime revocation");
        assert!(replay_after_revocation.replayed);
        assert_eq!(
            replay_after_revocation.receipt_sha256,
            terminal.receipt_sha256
        );
        assert_eq!(replay_after_revocation.head, terminal.head);
        let mut new_request = completion.clone();
        new_request.request_id = "public-osv-lifecycle-completion-new-0002".to_string();
        assert!(matches!(
            complete_original_source_verification(&fixture.pool, &new_request),
            Err(OriginalSourceVerificationError::ManagedRuntimeAuthorityUnavailable)
        ));

        let conn = fixture
            .pool
            .get()
            .expect("assert conflicting replay quarantine");
        assert_eq!(
            (
                conn.query_row(
                    "SELECT COUNT(*) FROM jobs_original_source_verification_observations
                      WHERE assignment_id=?1",
                    params![lease.assignment_id],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
                conn.query_row(
                    "SELECT COUNT(*) FROM jobs_original_source_verification_receipts
                      WHERE assignment_id=?1",
                    params![lease.assignment_id],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
                conn.query_row(
                    "SELECT COUNT(*) FROM jobs_original_source_verification_transitions
                      WHERE assignment_id=?1",
                    params![lease.assignment_id],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
            ),
            (
                counts_before_replay.0,
                counts_before_replay.1,
                counts_before_replay.2,
            ),
            "authority-independent replay and denied new publication must not remint terminal rows"
        );
        assert_eq!(
            conn.query_row(
                "SELECT state || ':' || last_error_code
                   FROM jobs_original_source_verification_assignments
                  WHERE assignment_id=?1",
                params![lease.assignment_id],
                |row| row.get::<_, String>(0),
            )
            .unwrap(),
            "quarantined:invalid_assignment"
        );
        assert_eq!(
            conn.query_row(
                "SELECT COUNT(*) FROM jobs_original_source_verification_receipts
                  WHERE assignment_id=?1",
                params![lease.assignment_id],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
            1,
            "a conflicting replay must not mint another receipt"
        );
        drop(conn);
        cleanup_public_original_source_lifecycle_fixture(fixture);
    }

    #[test]
    fn public_sqlite_failure_publishes_replayable_nonpositive_terminal() {
        let fixture = public_original_source_lifecycle_fixture();
        let lease = lease_original_source_verification(&fixture.pool, &fixture.binding)
            .expect("lease public failure assignment")
            .expect("public failure assignment is due");
        let failure = OriginalSourceVerificationFailureRequest {
            binding: fixture.binding.clone(),
            assignment_id: lease.assignment_id.clone(),
            attempt_id: lease.attempt_id.clone(),
            fence: lease.fence,
            lease_token: lease.lease_token.clone(),
            request_id: "public-osv-failure-terminal-0001".to_string(),
            error_code: "unreachable".to_string(),
            observation: public_unreachable_observation(&fixture),
        };
        let terminal = fail_original_source_verification(&fixture.pool, &failure)
            .expect("publish public failure terminal");
        assert_eq!(terminal.state, "retry_wait");
        assert!(!terminal.replayed);
        let head = terminal
            .head
            .clone()
            .expect("auditable nonpositive failure head");
        assert_eq!(head.result, "unreachable");
        let projection = resolve_original_source_verification_projection(
            &fixture.pool,
            &fixture.account_id,
            &fixture.posting,
        )
        .expect("resolve nonpositive failure projection");
        assert!(projection.feature_active);
        assert!(projection.evidence.is_none());
        assert!(projection.expected_head.is_none());
        let counts = {
            let conn = fixture
                .pool
                .get()
                .expect("assert public failure publication");
            assert_eq!(
                conn.query_row(
                    "SELECT state || ':' || last_error_code || ':' || consecutive_failures
                       FROM jobs_original_source_verification_assignments
                      WHERE assignment_id=?1",
                    params![lease.assignment_id],
                    |row| row.get::<_, String>(0),
                )
                .unwrap(),
                "retry_wait:unreachable:1"
            );
            (
                conn.query_row(
                    "SELECT COUNT(*) FROM jobs_original_source_verification_observations
                      WHERE assignment_id=?1",
                    params![lease.assignment_id],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
                conn.query_row(
                    "SELECT COUNT(*) FROM jobs_original_source_verification_receipts
                      WHERE assignment_id=?1",
                    params![lease.assignment_id],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
                conn.query_row(
                    "SELECT COUNT(*) FROM jobs_original_source_verification_transitions
                      WHERE assignment_id=?1",
                    params![lease.assignment_id],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
            )
        };
        assert_eq!(counts, (1, 1, 1));
        let replay = fail_original_source_verification(&fixture.pool, &failure)
            .expect("replay exact public failure terminal");
        assert!(replay.replayed);
        assert_eq!(replay.receipt_sha256, terminal.receipt_sha256);
        assert_eq!(replay.head, terminal.head);
        let conn = fixture
            .pool
            .get()
            .expect("assert public failure replay counts");
        assert_eq!(
            (
                conn.query_row(
                    "SELECT COUNT(*) FROM jobs_original_source_verification_observations
                      WHERE assignment_id=?1",
                    params![lease.assignment_id],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
                conn.query_row(
                    "SELECT COUNT(*) FROM jobs_original_source_verification_receipts
                      WHERE assignment_id=?1",
                    params![lease.assignment_id],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
                conn.query_row(
                    "SELECT COUNT(*) FROM jobs_original_source_verification_transitions
                      WHERE assignment_id=?1",
                    params![lease.assignment_id],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
            ),
            counts,
            "exact failure replay must not remint terminal rows"
        );
        drop(conn);
        cleanup_public_original_source_lifecycle_fixture(fixture);
    }

    #[test]
    fn public_sqlite_reclaimed_lease_fences_stale_attempt_lifecycle_calls() {
        let fixture = public_original_source_lifecycle_fixture();
        let stale = lease_original_source_verification(&fixture.pool, &fixture.binding)
            .expect("lease stale-fence fixture")
            .expect("stale-fence assignment is due");
        {
            let conn = fixture.pool.get().expect("expire stale-fence lease");
            let now: i64 = conn
                .query_row(
                    "SELECT CAST(unixepoch('subsec') * 1000 AS INTEGER)",
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            conn.execute(
                "UPDATE jobs_original_source_verification_assignments SET
                    lease_expires_at_ms=?2,updated_at_ms=?3
                  WHERE assignment_id=?1",
                params![stale.assignment_id, now.saturating_sub(1), now],
            )
            .expect("expire stale-fence assignment");
        }
        let current = lease_original_source_verification(&fixture.pool, &fixture.binding)
            .expect("reclaim stale-fence assignment")
            .expect("expired stale-fence assignment is reclaimable");
        assert_eq!(current.assignment_id, stale.assignment_id);
        assert_eq!(current.fence, stale.fence + 1);
        assert_ne!(current.attempt_id, stale.attempt_id);

        let stale_heartbeat = OriginalSourceVerificationHeartbeatRequest {
            binding: fixture.binding.clone(),
            assignment_id: stale.assignment_id.clone(),
            attempt_id: stale.attempt_id.clone(),
            fence: stale.fence,
            lease_token: stale.lease_token.clone(),
            heartbeat_sequence: 1,
        };
        assert!(matches!(
            heartbeat_original_source_verification(&fixture.pool, &stale_heartbeat),
            Err(OriginalSourceVerificationError::LeaseLost)
        ));
        let stale_completion =
            public_completion_request(&fixture, &stale, "public-osv-stale-fence-completion-0001");
        assert!(matches!(
            complete_original_source_verification(&fixture.pool, &stale_completion),
            Err(OriginalSourceVerificationError::LeaseLost)
        ));
        let stale_failure = OriginalSourceVerificationFailureRequest {
            binding: fixture.binding.clone(),
            assignment_id: stale.assignment_id.clone(),
            attempt_id: stale.attempt_id.clone(),
            fence: stale.fence,
            lease_token: stale.lease_token.clone(),
            request_id: "public-osv-stale-fence-failure-0001".to_string(),
            error_code: "unreachable".to_string(),
            observation: public_unreachable_observation(&fixture),
        };
        assert!(matches!(
            fail_original_source_verification(&fixture.pool, &stale_failure),
            Err(OriginalSourceVerificationError::LeaseLost)
        ));
        let current_heartbeat = heartbeat_original_source_verification(
            &fixture.pool,
            &OriginalSourceVerificationHeartbeatRequest {
                binding: fixture.binding.clone(),
                assignment_id: current.assignment_id.clone(),
                attempt_id: current.attempt_id.clone(),
                fence: current.fence,
                lease_token: current.lease_token.clone(),
                heartbeat_sequence: 1,
            },
        )
        .expect("current fenced lease remains usable");
        assert!(!current_heartbeat.replayed);
        let conn = fixture.pool.get().expect("assert current fenced lease");
        assert_eq!(
            conn.query_row(
                "SELECT attempt_count || ':' || active_attempt_id
                   FROM jobs_original_source_verification_assignments
                  WHERE assignment_id=?1",
                params![current.assignment_id],
                |row| row.get::<_, String>(0),
            )
            .unwrap(),
            format!("2:{}", current.attempt_id)
        );
        drop(conn);
        cleanup_public_original_source_lifecycle_fixture(fixture);
    }

    #[test]
    fn public_sqlite_publication_rechecks_hold_source_and_runtime_authority() {
        for denial in ["hold", "source", "runtime"] {
            let fixture = public_original_source_lifecycle_fixture();
            let lease = lease_original_source_verification(&fixture.pool, &fixture.binding)
                .expect("lease publication-authority fixture")
                .expect("publication-authority assignment is due");
            let completion = public_completion_request(
                &fixture,
                &lease,
                &format!("public-osv-publication-denial-{denial}-0001"),
            );
            match denial {
                "hold" => {
                    append_operational_hold_event(
                        &fixture.pool,
                        &AppendOperationalHoldEventRequest {
                            event_id: "public-osv-publication-hold-0001".to_string(),
                            capability: OperationalCapability::OriginalSourceVerification,
                            scope_kind: OperationalHoldScopeKind::DiscoverySource,
                            scope_id: fixture.source_id.clone(),
                            transition: OperationalHoldTransition::Held,
                            reason_code: OperationalHoldReasonCode::ProviderOutage,
                            reason_ref: Some("INC-PUBLIC-OSV-PUBLISH-0001".to_string()),
                            expected_head_revision: 0,
                            expected_current_event_id: None,
                        },
                        "public-osv-lifecycle-test",
                    )
                    .expect("hold publication discovery source");
                }
                "source" => {
                    let conn = fixture.pool.get().expect("pause publication source");
                    conn.execute(
                        "UPDATE jobs_discovery_sources SET health='paused'
                          WHERE id=?1 AND account_id=?2",
                        params![fixture.source_id, fixture.account_id],
                    )
                    .expect("pause publication discovery source");
                }
                "runtime" => {
                    revoke_managed_cloud_runtime_grant(
                        &fixture.pool,
                        &RevokeManagedCloudRuntimeGrant {
                            grant_id: fixture.grant_id.clone(),
                            reason_ref: "public-osv-runtime-revocation-0001".to_string(),
                            revoked_by: "public-osv-lifecycle-test".to_string(),
                        },
                    )
                    .expect("revoke publication runtime grant");
                }
                _ => unreachable!("publication denial is closed"),
            }
            let result = complete_original_source_verification(&fixture.pool, &completion);
            if denial == "source" {
                assert!(matches!(
                    result,
                    Err(OriginalSourceVerificationError::InvalidInput(_))
                ));
            } else {
                assert!(matches!(
                    result,
                    Err(OriginalSourceVerificationError::ManagedRuntimeAuthorityUnavailable)
                ));
            }
            let conn = fixture.pool.get().expect("assert denied publication");
            assert_eq!(
                conn.query_row(
                    "SELECT COUNT(*) FROM jobs_original_source_verification_receipts
                      WHERE assignment_id=?1",
                    params![lease.assignment_id],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
                0
            );
            assert_eq!(
                conn.query_row(
                    "SELECT COUNT(*) FROM jobs_original_source_verification_heads
                      WHERE account_id=?1 AND job_id=?2",
                    params![fixture.account_id, fixture.posting.id],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
                0
            );
            assert_eq!(
                conn.query_row(
                    "SELECT state || ':' || active_attempt_id
                       FROM jobs_original_source_verification_assignments
                      WHERE assignment_id=?1",
                    params![lease.assignment_id],
                    |row| row.get::<_, String>(0),
                )
                .unwrap(),
                format!("leased:{}", lease.attempt_id)
            );
            drop(conn);
            cleanup_public_original_source_lifecycle_fixture(fixture);
        }
    }

    #[test]
    fn postgres_lifecycle_effects_prelock_h_m_d_before_assignment() {
        let source = include_str!("original_source_verification.rs");
        let sqlite_assignment_authority = source
            .split("fn original_source_require_assignment_authority_sqlite")
            .nth(1)
            .and_then(|value| {
                value
                    .split("fn original_source_require_assignment_authority_postgres")
                    .next()
            })
            .unwrap();
        let postgres_assignment_authority = source
            .split("fn original_source_require_assignment_authority_postgres")
            .nth(1)
            .and_then(|value| {
                value
                    .split("fn original_source_require_runtime_binding_sqlite")
                    .next()
            })
            .unwrap();
        assert!(sqlite_assignment_authority
            .contains(".map_err(original_source_assignment_authority_error)"));
        assert!(postgres_assignment_authority
            .contains(".map_err(original_source_assignment_authority_error)"));
        let heartbeat = source
            .split("pub fn heartbeat_original_source_verification")
            .nth(1)
            .and_then(|value| value.split("const ORIGINAL_SOURCE_HEAD_SELECT").next())
            .unwrap();
        let heartbeat_postgres = heartbeat.split("DbPool::Postgres(_)").nth(1).unwrap();
        let heartbeat_h = heartbeat_postgres
            .find("lock_operational_hold_shared_postgres_tx")
            .unwrap();
        let heartbeat_m = heartbeat_postgres
            .find("require_original_source_verifier_runtime_active_postgres_tx")
            .unwrap();
        let heartbeat_identity = heartbeat_postgres
            .find("original_source_attempt_identity_postgres")
            .unwrap();
        let heartbeat_d = heartbeat_postgres
            .find("lock_discovery_account_shared_postgres")
            .unwrap();
        let heartbeat_assignment = heartbeat_postgres.find("FOR UPDATE OF a").unwrap();
        assert!(
            heartbeat_h < heartbeat_m
                && heartbeat_m < heartbeat_identity
                && heartbeat_identity < heartbeat_d
                && heartbeat_d < heartbeat_assignment
        );

        let terminal = source
            .split("fn original_source_complete_postgres")
            .nth(1)
            .and_then(|value| {
                value
                    .split("fn original_source_advance_head_postgres")
                    .next()
            })
            .unwrap();
        let replay_helper = source
            .split("fn original_source_terminal_replay_postgres")
            .nth(1)
            .and_then(|value| value.split("struct OriginalSourcePreviousHead").next())
            .unwrap();
        assert!(!replay_helper.contains("FOR UPDATE"));
        assert!(!replay_helper.contains("original_source_quarantine_conflicting_replay"));
        for mutation in ["UPDATE ", "INSERT ", "DELETE "] {
            assert!(!replay_helper.contains(mutation));
        }
        let (conflict, publication) = terminal
            .split_once("// Preserve the global PostgreSQL authority lock order")
            .unwrap();
        let replay = conflict
            .find("original_source_terminal_replay_postgres")
            .unwrap();
        let conflict_h = conflict
            .find("lock_operational_hold_shared_postgres_tx")
            .unwrap();
        let conflict_m = conflict
            .find("lock_managed_cloud_release_registry_shared_postgres_tx")
            .unwrap();
        let conflict_identity = conflict
            .find("original_source_assignment_identity_postgres")
            .unwrap();
        let conflict_d = conflict.find("lock_discovery_account_postgres").unwrap();
        let conflict_assignment = conflict
            .find("original_source_quarantine_conflicting_replay_postgres")
            .unwrap();
        assert!(
            replay < conflict_h
                && conflict_h < conflict_m
                && conflict_m < conflict_identity
                && conflict_identity < conflict_d
                && conflict_d < conflict_assignment
        );
        let publication_h = publication
            .find("lock_operational_hold_shared_postgres_tx")
            .unwrap();
        let publication_m = publication
            .find("require_original_source_verifier_runtime_active_postgres_tx")
            .unwrap();
        let publication_identity = publication
            .find("original_source_attempt_identity_postgres")
            .unwrap();
        let publication_d = publication.find("lock_discovery_account_postgres").unwrap();
        let publication_assignment = publication
            .find("original_source_load_lease_postgres")
            .unwrap();
        assert!(
            publication_h < publication_m
                && publication_m < publication_identity
                && publication_identity < publication_d
                && publication_d < publication_assignment
        );
        let identity = source
            .split("fn original_source_attempt_identity_postgres")
            .nth(1)
            .and_then(|value| {
                value
                    .split("fn original_source_validate_lease_values")
                    .next()
            })
            .unwrap();
        assert!(!identity.contains("FOR UPDATE"));
        let locked_lease = source
            .split("fn original_source_load_lease_postgres")
            .nth(1)
            .and_then(|value| {
                value
                    .split("fn original_source_validate_observation_subject")
                    .next()
            })
            .unwrap();
        assert!(locked_lease.contains("a.account_id = $3 AND a.job_id = $4"));
        assert!(locked_lease.contains("FOR UPDATE OF a"));
        let quarantine = source
            .split("fn original_source_quarantine_conflicting_replay_postgres")
            .nth(1)
            .and_then(|value| {
                value
                    .split("fn original_source_terminal_replay_sqlite")
                    .next()
            })
            .unwrap();
        assert!(quarantine.contains("account_id=$2 AND job_id=$3"));
        assert!(quarantine.contains("FOR UPDATE"));
    }

    #[test]
    fn lease_scan_is_one_page_bounded_durably_deferred_and_h_m_d_ordered() {
        let source = include_str!("original_source_verification.rs");
        let sqlite = source
            .split("fn original_source_lease_candidates_sqlite")
            .nth(1)
            .and_then(|value| {
                value
                    .split("fn original_source_lease_candidates_postgres")
                    .next()
            })
            .unwrap();
        let postgres = source
            .split("fn original_source_lease_candidates_postgres")
            .nth(1)
            .and_then(|value| {
                value
                    .split("fn original_source_lock_lease_candidate_postgres")
                    .next()
            })
            .unwrap();
        for dialect in [sqlite, postgres] {
            assert!(dialect.contains("assignment.next_attempt_at_ms >"));
            assert!(dialect.contains("assignment.created_at_ms >"));
            assert!(dialect.contains("assignment.assignment_id >"));
            assert!(dialect.contains("ORIGINAL_SOURCE_LEASE_SCAN_LIMIT"));
        }
        assert!(!postgres.contains("FOR UPDATE"));
        let lock = source
            .split("fn original_source_lock_lease_candidate_postgres")
            .nth(1)
            .and_then(|value| {
                value
                    .split("fn original_source_subject_recheck_decision")
                    .next()
            })
            .unwrap();
        let discovery = lock.find("lock_discovery_account_shared_postgres").unwrap();
        let assignment = lock.find("FOR UPDATE OF assignment").unwrap();
        assert!(discovery < assignment);
        let lease = source
            .split("pub fn lease_original_source_verification")
            .nth(1)
            .and_then(|value| value.split("fn original_source_binding_hashes").next())
            .unwrap();
        let hold = lease
            .find("lock_operational_hold_shared_postgres_tx")
            .unwrap();
        let managed = lease
            .find("require_original_source_verifier_runtime_active_postgres_tx")
            .unwrap();
        let decision = source
            .split("fn original_source_lease_candidate_decision_postgres")
            .nth(1)
            .and_then(|value| {
                value
                    .split("fn original_source_supersede_lease_candidate_sqlite")
                    .next()
            })
            .unwrap();
        assert!(hold < managed);
        assert!(decision.contains("DeferOperationalHold"));
        assert!(decision.contains("managed_authority_revoked"));
        assert_eq!(
            lease
                .matches("original_source_scan_lease_candidates(")
                .count(),
            2
        );
        let scan = source
            .split("fn original_source_scan_lease_candidates")
            .nth(1)
            .and_then(|value| {
                value
                    .split("pub enum OriginalSourceVerificationError")
                    .next()
            })
            .unwrap();
        assert!(!scan.contains("loop {"));
        assert!(scan.contains("DeferOperationalHold"));
        assert!(scan.contains("Ok(None)"));
        let deferred = source
            .split("fn original_source_deferred_hold_candidates_postgres")
            .nth(1)
            .and_then(|value| {
                value
                    .split("fn original_source_lock_deferred_hold_candidate_postgres")
                    .next()
            })
            .unwrap();
        assert!(deferred.contains("ORIGINAL_SOURCE_HOLD_RECHECK_LIMIT"));
        assert!(!deferred.contains("ORIGINAL_SOURCE_LEASE_SCAN_LIMIT"));
    }

    #[test]
    fn postgres_subject_scan_skips_invalid_first_when_configured() {
        let Ok(database_url) = std::env::var("BLUEY_TEST_POSTGRES_URL") else {
            eprintln!(
                "SKIP postgres_subject_scan_skips_invalid_first_when_configured: \
                 BLUEY_TEST_POSTGRES_URL is unavailable"
            );
            return;
        };
        let pool = db::open_postgres_pool(&database_url).expect("open PostgreSQL OSV test pool");
        db::run_migrations(&pool).expect("apply PostgreSQL OSV migrations");
        let suffix = uuid::Uuid::new_v4().simple().to_string();
        let account_id = format!("acct-osv-scan-{suffix}");
        let first_source_id = format!("source-osv-first-{suffix}");
        let second_source_id = format!("source-osv-second-{suffix}");
        let mut first = fixture_posting(
            "greenhouse_import",
            "https://boards.greenhouse.io/acme/jobs/123",
            "123",
        );
        first.id = format!("job-osv-first-{suffix}");
        first.canonical_key = format!("canonical-osv-first-{suffix}");
        let mut second = fixture_posting("lever_import", "https://jobs.lever.co/bravo/456", "456");
        second.id = format!("job-osv-second-{suffix}");
        second.canonical_key = format!("canonical-osv-second-{suffix}");
        let first_subject = original_source_subject_sha256(&first).unwrap();
        let second_subject = original_source_subject_sha256(&second).unwrap();

        let mut conn = pool.get_pg().expect("get PostgreSQL OSV test connection");
        let mut tx = conn
            .transaction()
            .expect("begin PostgreSQL OSV test transaction");
        tx.execute(
            "INSERT INTO accounts(id,email,password_hash,trial_seconds_remaining)
             VALUES($1,$2,'hash',0)",
            &[&account_id, &format!("{suffix}@osv.example.test")],
        )
        .expect("insert PostgreSQL OSV account");
        for posting in [&first, &second] {
            tx.execute(
                "INSERT INTO jobs_postings(
                    id,account_id,canonical_key,posting_json,source,canonical_url,
                    company,title,location,match_score,status,created_at_ms,updated_at_ms)
                 VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,0,'matched',1,1)",
                &[
                    &posting.id,
                    &account_id,
                    &posting.canonical_key,
                    &serde_json::to_string(posting).unwrap(),
                    &posting.source,
                    &posting.canonical_url,
                    &posting.company,
                    &posting.title,
                    &posting.location,
                ],
            )
            .expect("insert PostgreSQL OSV posting");
        }
        for (source_id, provider, source_key) in [
            (first_source_id.as_str(), "greenhouse", "acme"),
            (second_source_id.as_str(), "lever", "bravo"),
        ] {
            tx.execute(
                "INSERT INTO jobs_discovery_sources(
                    id,account_id,provider,source_key,source_json,status,health,
                    next_run_at_ms,created_at_ms,updated_at_ms)
                 VALUES($1,$2,$3,$4,'{}','active','healthy',0,1,1)",
                &[&source_id, &account_id, &provider, &source_key],
            )
            .expect("insert PostgreSQL OSV source");
        }
        for (source_id, posting) in [
            (first_source_id.as_str(), &first),
            (second_source_id.as_str(), &second),
        ] {
            tx.execute(
                "INSERT INTO jobs_discovery_memberships(
                    source_id,account_id,external_id,canonical_key,job_id,content_hash,
                    first_seen_at_ms,last_seen_at_ms,last_seen_run_id,availability_status)
                 VALUES($1,$2,$3,$4,$5,$6,1,1,'run-osv','pending')",
                &[
                    &source_id,
                    &account_id,
                    &posting.external_id,
                    &posting.canonical_key,
                    &posting.id,
                    &"a".repeat(64),
                ],
            )
            .expect("insert PostgreSQL OSV membership");
        }
        for health in ["paused", "degraded"] {
            tx.execute(
                "UPDATE jobs_discovery_sources SET health=$2 WHERE id=$1",
                &[&first_source_id, &health],
            )
            .unwrap();
            assert!(matches!(
                recheck_original_source_subject_postgres(
                    &mut tx,
                    &account_id,
                    &first.id,
                    &first_subject,
                ),
                Err(OriginalSourceVerificationError::InvalidInput(_))
            ));
            recheck_original_source_subject_postgres(
                &mut tx,
                &account_id,
                &second.id,
                &second_subject,
            )
            .expect("valid second PostgreSQL subject");
        }
        tx.execute(
            "UPDATE jobs_discovery_sources SET health='healthy' WHERE id=$1",
            &[&first_source_id],
        )
        .unwrap();
        first.title = "Revoked subject mutation".to_string();
        tx.execute(
            "UPDATE jobs_postings SET posting_json=$2,title=$3
              WHERE account_id=$1 AND id=$4",
            &[
                &account_id,
                &serde_json::to_string(&first).unwrap(),
                &first.title,
                &first.id,
            ],
        )
        .unwrap();
        assert!(matches!(
            recheck_original_source_subject_postgres(
                &mut tx,
                &account_id,
                &first.id,
                &first_subject,
            ),
            Err(OriginalSourceVerificationError::LeaseLost)
        ));
        recheck_original_source_subject_postgres(&mut tx, &account_id, &second.id, &second_subject)
            .expect("valid second PostgreSQL subject after first drift");
        tx.rollback().expect("roll back PostgreSQL OSV fixture");
    }

    #[test]
    fn assignment_rejects_paused_or_degraded_source_authority() {
        for health in ["paused", "degraded"] {
            let (pool, posting) = fixture_pool_and_posting();
            let (managed_authority, canonical_managed_authority_json, managed_authority_sha256) =
                fixture_managed_authority();
            let mut conn = pool.get().unwrap();
            conn.execute(
                "UPDATE jobs_discovery_sources SET health=?1",
                params![health],
            )
            .unwrap();
            let tx = conn
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .unwrap();
            assert!(matches!(
                ensure_original_source_verification_assignment_sqlite_bound_tx(
                    &tx,
                    "acct-original-source",
                    &posting,
                    &managed_authority,
                    &canonical_managed_authority_json,
                    &managed_authority_sha256,
                ),
                Err(OriginalSourceVerificationError::InvalidInput(_))
            ));
        }
    }

    #[test]
    fn quarantined_assignment_invalidates_an_exact_positive_head() {
        let digest = "a".repeat(64);
        let mut head = OriginalSourceVerificationHead {
            account_id: "acct-original-source".to_string(),
            job_id: "job-original-source".to_string(),
            head_revision: 1,
            material_generation: 1,
            assignment_id: "assignment-original-source".to_string(),
            receipt_id: "receipt-original-source".to_string(),
            receipt_sha256: digest.clone(),
            subject_sha256: digest.clone(),
            material_sha256: digest.clone(),
            assurance: "original_verified".to_string(),
            result: "verified_open".to_string(),
            checked_at_ms: 1,
            expires_at_ms: 10,
            canonical_application_url: Some(
                "https://boards.greenhouse.io/acme/jobs/123".to_string(),
            ),
            application_domain: Some("boards.greenhouse.io".to_string()),
            managed_authority_sha256: digest.clone(),
            canonical_managed_authority_json: "{}".to_string(),
            assignment_state: "idle".to_string(),
        };
        let expected = OriginalSourceVerificationExpectedHead {
            subject_sha256: digest.clone(),
            material_generation: 1,
            material_sha256: digest.clone(),
            receipt_sha256: digest.clone(),
            expires_at_ms: 10,
            managed_authority_sha256: digest,
        };
        compare_original_source_verification_head(&head, &expected, 2).unwrap();
        head.assignment_state = "quarantined".to_string();
        assert!(matches!(
            compare_original_source_verification_head(&head, &expected, 2),
            Err(OriginalSourceVerificationError::ConcurrentHeadAdvance)
        ));
    }
}
