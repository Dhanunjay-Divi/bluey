const OPERATIONAL_HOLD_SCHEMA_VERSION: i64 = 1;
const OPERATIONAL_HOLD_MAX_REVISION: i64 = 9_007_199_254_740_991;
const POSTGRES_OPERATIONAL_HOLD_EXCLUSIVE_LOCK_SQL: &str =
    "SELECT pg_advisory_xact_lock(hashtextextended('bluey-jobs-operational-holds-v1', 0))";
const POSTGRES_OPERATIONAL_HOLD_SHARED_LOCK_SQL: &str =
    "SELECT pg_advisory_xact_lock_shared(hashtextextended('bluey-jobs-operational-holds-v1', 0))";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum OperationalCapability {
    All,
    Discovery,
    OriginalSourceVerification,
    Generation,
    ApplicationQueue,
    RunnerClaim,
    FinalSubmit,
    MailboxSync,
    CommunicationDispatch,
}

impl OperationalCapability {
    pub const CONCRETE: [Self; 8] = [
        Self::Discovery,
        Self::OriginalSourceVerification,
        Self::Generation,
        Self::ApplicationQueue,
        Self::RunnerClaim,
        Self::FinalSubmit,
        Self::MailboxSync,
        Self::CommunicationDispatch,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Discovery => "discovery",
            Self::OriginalSourceVerification => "original_source_verification",
            Self::Generation => "generation",
            Self::ApplicationQueue => "application_queue",
            Self::RunnerClaim => "runner_claim",
            Self::FinalSubmit => "final_submit",
            Self::MailboxSync => "mailbox_sync",
            Self::CommunicationDispatch => "communication_dispatch",
        }
    }

    fn from_storage(value: &str) -> std::result::Result<Self, OperationalHoldError> {
        match value {
            "all" => Ok(Self::All),
            "discovery" => Ok(Self::Discovery),
            "original_source_verification" => Ok(Self::OriginalSourceVerification),
            "generation" => Ok(Self::Generation),
            "application_queue" => Ok(Self::ApplicationQueue),
            "runner_claim" => Ok(Self::RunnerClaim),
            "final_submit" => Ok(Self::FinalSubmit),
            "mailbox_sync" => Ok(Self::MailboxSync),
            "communication_dispatch" => Ok(Self::CommunicationDispatch),
            _ => Err(OperationalHoldError::Storage(anyhow::anyhow!(
                "invalid stored operational capability"
            ))),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum OperationalHoldScopeKind {
    Global,
    DiscoverySource,
    AtsProvider,
    AtsAdapter,
    EmployerDomain,
    Account,
    CareerTrack,
    Region,
    RunnerKind,
    MailboxProvider,
    ModelProvider,
    Model,
}

impl OperationalHoldScopeKind {
    pub const ALL: [Self; 12] = [
        Self::Global,
        Self::DiscoverySource,
        Self::AtsProvider,
        Self::AtsAdapter,
        Self::EmployerDomain,
        Self::Account,
        Self::CareerTrack,
        Self::Region,
        Self::RunnerKind,
        Self::MailboxProvider,
        Self::ModelProvider,
        Self::Model,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Global => "global",
            Self::DiscoverySource => "discovery_source",
            Self::AtsProvider => "ats_provider",
            Self::AtsAdapter => "ats_adapter",
            Self::EmployerDomain => "employer_domain",
            Self::Account => "account",
            Self::CareerTrack => "career_track",
            Self::Region => "region",
            Self::RunnerKind => "runner_kind",
            Self::MailboxProvider => "mailbox_provider",
            Self::ModelProvider => "model_provider",
            Self::Model => "model",
        }
    }

    fn from_storage(value: &str) -> std::result::Result<Self, OperationalHoldError> {
        match value {
            "global" => Ok(Self::Global),
            "discovery_source" => Ok(Self::DiscoverySource),
            "ats_provider" => Ok(Self::AtsProvider),
            "ats_adapter" => Ok(Self::AtsAdapter),
            "employer_domain" => Ok(Self::EmployerDomain),
            "account" => Ok(Self::Account),
            "career_track" => Ok(Self::CareerTrack),
            "region" => Ok(Self::Region),
            "runner_kind" => Ok(Self::RunnerKind),
            "mailbox_provider" => Ok(Self::MailboxProvider),
            "model_provider" => Ok(Self::ModelProvider),
            "model" => Ok(Self::Model),
            _ => Err(OperationalHoldError::Storage(anyhow::anyhow!(
                "invalid stored operational hold scope"
            ))),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OperationalHoldReasonCode {
    Incident,
    SecurityReview,
    PrivacyReview,
    ComplianceReview,
    QualityRegression,
    ProviderOutage,
    CapacityGuard,
    Maintenance,
    AccountRequest,
    CertificationGuard,
    RolloutGuard,
    ManualRelease,
}

impl OperationalHoldReasonCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Incident => "incident",
            Self::SecurityReview => "security_review",
            Self::PrivacyReview => "privacy_review",
            Self::ComplianceReview => "compliance_review",
            Self::QualityRegression => "quality_regression",
            Self::ProviderOutage => "provider_outage",
            Self::CapacityGuard => "capacity_guard",
            Self::Maintenance => "maintenance",
            Self::AccountRequest => "account_request",
            Self::CertificationGuard => "certification_guard",
            Self::RolloutGuard => "rollout_guard",
            Self::ManualRelease => "manual_release",
        }
    }

    fn from_storage(value: &str) -> std::result::Result<Self, OperationalHoldError> {
        match value {
            "incident" => Ok(Self::Incident),
            "security_review" => Ok(Self::SecurityReview),
            "privacy_review" => Ok(Self::PrivacyReview),
            "compliance_review" => Ok(Self::ComplianceReview),
            "quality_regression" => Ok(Self::QualityRegression),
            "provider_outage" => Ok(Self::ProviderOutage),
            "capacity_guard" => Ok(Self::CapacityGuard),
            "maintenance" => Ok(Self::Maintenance),
            "account_request" => Ok(Self::AccountRequest),
            "certification_guard" => Ok(Self::CertificationGuard),
            "rollout_guard" => Ok(Self::RolloutGuard),
            "manual_release" => Ok(Self::ManualRelease),
            _ => Err(OperationalHoldError::Storage(anyhow::anyhow!(
                "invalid stored operational hold reason"
            ))),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OperationalHoldTransition {
    Held,
    Released,
}

impl OperationalHoldTransition {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Held => "held",
            Self::Released => "released",
        }
    }

    fn from_storage(value: &str) -> std::result::Result<Self, OperationalHoldError> {
        match value {
            "held" => Ok(Self::Held),
            "released" => Ok(Self::Released),
            _ => Err(OperationalHoldError::Storage(anyhow::anyhow!(
                "invalid stored operational hold transition"
            ))),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AppendOperationalHoldEventRequest {
    pub event_id: String,
    pub capability: OperationalCapability,
    pub scope_kind: OperationalHoldScopeKind,
    pub scope_id: String,
    pub transition: OperationalHoldTransition,
    pub reason_code: OperationalHoldReasonCode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason_ref: Option<String>,
    pub expected_head_revision: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_current_event_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AppendOperationalHoldEventByRefRequest {
    pub event_id: String,
    pub capability: OperationalCapability,
    pub scope_kind: OperationalHoldScopeKind,
    pub scope_ref: String,
    pub transition: OperationalHoldTransition,
    pub reason_code: OperationalHoldReasonCode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason_ref: Option<String>,
    pub expected_head_revision: i64,
    pub expected_current_event_ref: String,
}

#[derive(Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct OperationalHoldState {
    pub capability: OperationalCapability,
    pub scope_kind: OperationalHoldScopeKind,
    #[serde(skip_serializing)]
    pub scope_id: String,
    pub head_revision: i64,
    #[serde(skip_serializing)]
    pub current_event_id: String,
    pub event_sha256: String,
    pub state: OperationalHoldTransition,
    pub reason_code: OperationalHoldReasonCode,
    #[serde(skip_serializing)]
    pub reason_ref: Option<String>,
    #[serde(skip_serializing)]
    pub recorded_by: String,
    pub recorded_at_ms: i64,
}

impl std::fmt::Debug for OperationalHoldState {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OperationalHoldState")
            .field("capability", &self.capability)
            .field("scope_kind", &self.scope_kind)
            .field("scope_id", &"[redacted]")
            .field("head_revision", &self.head_revision)
            .field("current_event_id", &"[redacted]")
            .field("event_sha256", &self.event_sha256)
            .field("state", &self.state)
            .field("reason_code", &self.reason_code)
            .field(
                "reason_ref",
                &self.reason_ref.as_ref().map(|_| "[redacted]"),
            )
            .field("recorded_by", &"[redacted]")
            .field("recorded_at_ms", &self.recorded_at_ms)
            .finish()
    }
}

impl OperationalHoldState {
    pub fn redacted(
        &self,
    ) -> std::result::Result<OperationalHoldPublicState, OperationalHoldError> {
        Ok(OperationalHoldPublicState {
            capability: self.capability,
            scope_kind: self.scope_kind,
            scope_ref: operational_hold_scope_ref(self.scope_kind, &self.scope_id)?,
            head_revision: self.head_revision,
            current_event_ref: operational_hold_event_ref(&self.current_event_id)?,
            event_sha256: self.event_sha256.clone(),
            state: self.state,
            reason_code: self.reason_code,
            recorded_at_ms: self.recorded_at_ms,
        })
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct OperationalHoldPublicState {
    pub capability: OperationalCapability,
    pub scope_kind: OperationalHoldScopeKind,
    pub scope_ref: String,
    pub head_revision: i64,
    pub current_event_ref: String,
    pub event_sha256: String,
    pub state: OperationalHoldTransition,
    pub reason_code: OperationalHoldReasonCode,
    pub recorded_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct OperationalHoldAppendResult {
    #[serde(flatten)]
    pub state: OperationalHoldPublicState,
    pub replayed: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct OperationalHoldListPage {
    pub states: Vec<OperationalHoldState>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct OperationalHoldListCursor {
    purpose: String,
    active_only: bool,
    capability: OperationalCapability,
    scope_kind: OperationalHoldScopeKind,
    scope_id: String,
}

#[derive(Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct OperationalHoldBlock {
    pub capability: OperationalCapability,
    pub scope_kind: OperationalHoldScopeKind,
    #[serde(skip_serializing)]
    pub scope_id: String,
    pub reason_code: OperationalHoldReasonCode,
    #[serde(skip_serializing)]
    pub reason_ref: Option<String>,
    pub head_revision: i64,
}

impl std::fmt::Debug for OperationalHoldBlock {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OperationalHoldBlock")
            .field("capability", &self.capability)
            .field("scope_kind", &self.scope_kind)
            .field("scope_id", &"[redacted]")
            .field("reason_code", &self.reason_code)
            .field(
                "reason_ref",
                &self.reason_ref.as_ref().map(|_| "[redacted]"),
            )
            .field("head_revision", &self.head_revision)
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OperationalCapabilityEvaluation {
    Allowed,
    Held(OperationalHoldBlock),
}

#[derive(Debug, Error)]
pub enum OperationalHoldError {
    #[error("invalid operational hold request")]
    InvalidRequest,
    #[error("operational hold target not found")]
    NotFound,
    #[error("operational hold compare-and-swap conflict")]
    Conflict,
    #[error("operational hold event identity conflict")]
    IdentityConflict,
    #[error("Bluey Jobs operation is held")]
    Held(OperationalHoldBlock),
    #[error("operational hold storage unavailable")]
    Storage(#[source] anyhow::Error),
}

impl From<rusqlite::Error> for OperationalHoldError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Storage(error.into())
    }
}

impl From<postgres::Error> for OperationalHoldError {
    fn from(error: postgres::Error) -> Self {
        Self::Storage(error.into())
    }
}

#[derive(Clone, Default, PartialEq, Eq)]
pub struct OperationalHoldContext {
    scopes: BTreeMap<OperationalHoldScopeKind, BTreeSet<String>>,
}

impl std::fmt::Debug for OperationalHoldContext {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let scope_counts = self
            .scopes
            .iter()
            .map(|(kind, values)| (kind.as_str(), values.len()))
            .collect::<BTreeMap<_, _>>();
        formatter
            .debug_struct("OperationalHoldContext")
            .field("scope_counts", &scope_counts)
            .finish()
    }
}

impl OperationalHoldContext {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_scope(
        mut self,
        scope_kind: OperationalHoldScopeKind,
        scope_id: &str,
    ) -> std::result::Result<Self, OperationalHoldError> {
        self.insert_scope(scope_kind, scope_id)?;
        Ok(self)
    }

    pub fn insert_scope(
        &mut self,
        scope_kind: OperationalHoldScopeKind,
        scope_id: &str,
    ) -> std::result::Result<(), OperationalHoldError> {
        if scope_kind == OperationalHoldScopeKind::Global {
            return Err(OperationalHoldError::InvalidRequest);
        }
        let normalized = normalize_operational_scope_id(scope_kind, scope_id)?;
        validate_preemptive_operational_scope(scope_kind, &normalized)?;
        self.scopes
            .entry(scope_kind)
            .or_default()
            .insert(normalized);
        Ok(())
    }

    fn matches(&self, scope_kind: OperationalHoldScopeKind, scope_id: &str) -> bool {
        if scope_kind == OperationalHoldScopeKind::Global {
            return scope_id == "*";
        }
        self.scopes
            .get(&scope_kind)
            .is_some_and(|values| values.contains(scope_id))
    }

    fn exact_scope_pairs(&self) -> Vec<(String, String)> {
        let mut pairs = vec![(
            OperationalHoldScopeKind::Global.as_str().to_string(),
            "*".to_string(),
        )];
        for (scope_kind, scope_ids) in &self.scopes {
            for scope_id in scope_ids {
                pairs.push((scope_kind.as_str().to_string(), scope_id.clone()));
            }
        }
        pairs
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CanonicalOperationalHoldEvent {
    schema_version: i64,
    event_id: String,
    capability: OperationalCapability,
    scope_kind: OperationalHoldScopeKind,
    scope_id: String,
    revision_no: i64,
    previous_revision_no: Option<i64>,
    predecessor_event_id: Option<String>,
    transition: OperationalHoldTransition,
    reason_code: OperationalHoldReasonCode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    reason_ref: Option<String>,
    recorded_by: String,
    recorded_at_ms: i64,
}

fn valid_operational_identifier(value: &str, max_len: usize) -> bool {
    !value.is_empty()
        && value.len() <= max_len
        && value.trim() == value
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
}

fn unicode_lowercase_nfc(value: &str) -> String {
    use unicode_normalization::UnicodeNormalization;

    value
        .nfc()
        .collect::<String>()
        .to_lowercase()
        .nfc()
        .collect()
}

fn normalize_operational_scope_id(
    scope_kind: OperationalHoldScopeKind,
    value: &str,
) -> std::result::Result<String, OperationalHoldError> {
    let value = value.trim();
    if value.is_empty() || value.len() > 256 || value.chars().any(char::is_control) {
        return Err(OperationalHoldError::InvalidRequest);
    }
    if scope_kind == OperationalHoldScopeKind::Global {
        return (value == "*")
            .then(|| value.to_string())
            .ok_or(OperationalHoldError::InvalidRequest);
    }
    if value == "*" {
        return Err(OperationalHoldError::InvalidRequest);
    }
    let normalized = match scope_kind {
        OperationalHoldScopeKind::Account
        | OperationalHoldScopeKind::CareerTrack
        | OperationalHoldScopeKind::DiscoverySource => value.to_string(),
        OperationalHoldScopeKind::EmployerDomain => {
            let domain = value.trim_end_matches('.');
            if domain.chars().any(char::is_whitespace) {
                return Err(OperationalHoldError::InvalidRequest);
            }
            unicode_lowercase_nfc(domain)
        }
        OperationalHoldScopeKind::Region => unicode_lowercase_nfc(value),
        _ => value.to_ascii_lowercase(),
    };
    if normalized.is_empty()
        || normalized.len() > 256
        || (!matches!(
            scope_kind,
            OperationalHoldScopeKind::EmployerDomain | OperationalHoldScopeKind::Region
        ) && !normalized.is_ascii())
    {
        return Err(OperationalHoldError::InvalidRequest);
    }
    Ok(normalized)
}

fn valid_operational_scope_token(value: &str) -> bool {
    value
        .bytes()
        .next()
        .is_some_and(|byte| byte.is_ascii_alphanumeric())
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase()
                || byte.is_ascii_digit()
                || matches!(byte, b'-' | b'_' | b'.' | b':' | b'/' | b'+')
        })
}

fn valid_operational_employer_domain(value: &str) -> bool {
    if value.len() > 253 || value.chars().any(char::is_whitespace) {
        return false;
    }
    let Ok(url) = reqwest::Url::parse(&format!("https://{value}/")) else {
        return false;
    };
    let Some(host) = url.host_str() else {
        return false;
    };
    url.username().is_empty()
        && url.password().is_none()
        && url.port().is_none()
        && url.path() == "/"
        && url.query().is_none()
        && url.fragment().is_none()
        && host.contains('.')
        && host.parse::<std::net::IpAddr>().is_err()
}

fn validate_preemptive_operational_scope(
    scope_kind: OperationalHoldScopeKind,
    scope_id: &str,
) -> std::result::Result<(), OperationalHoldError> {
    let valid = match scope_kind {
        OperationalHoldScopeKind::Global => scope_id == "*",
        OperationalHoldScopeKind::AtsProvider => matches!(
            scope_id,
            "ashby"
                | "curated_feed"
                | "greenhouse"
                | "jobhive"
                | "lever"
                | "smartrecruiters"
                | "workday"
        ),
        OperationalHoldScopeKind::RunnerKind => matches!(scope_id, "cloud" | "local"),
        OperationalHoldScopeKind::MailboxProvider => matches!(scope_id, "gmail" | "outlook"),
        OperationalHoldScopeKind::AtsAdapter
        | OperationalHoldScopeKind::ModelProvider
        | OperationalHoldScopeKind::Model => valid_operational_scope_token(scope_id),
        OperationalHoldScopeKind::EmployerDomain => valid_operational_employer_domain(scope_id),
        OperationalHoldScopeKind::Region => scope_id.len() <= 128 && !scope_id.contains('@'),
        OperationalHoldScopeKind::Account
        | OperationalHoldScopeKind::CareerTrack
        | OperationalHoldScopeKind::DiscoverySource => true,
    };
    valid
        .then_some(())
        .ok_or(OperationalHoldError::InvalidRequest)
}

fn validate_new_operational_hold_scope_sqlite(
    tx: &rusqlite::Transaction<'_>,
    scope_kind: OperationalHoldScopeKind,
    scope_id: &str,
) -> std::result::Result<(), OperationalHoldError> {
    validate_preemptive_operational_scope(scope_kind, scope_id)?;
    let exists = match scope_kind {
        OperationalHoldScopeKind::Account => tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM accounts WHERE id = ?1)",
            params![scope_id],
            |row| row.get::<_, bool>(0),
        )?,
        OperationalHoldScopeKind::CareerTrack => tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM jobs_tracks WHERE id = ?1)",
            params![scope_id],
            |row| row.get::<_, bool>(0),
        )?,
        OperationalHoldScopeKind::DiscoverySource => tx.query_row(
            "SELECT EXISTS(
                SELECT 1 FROM jobs_discovery_sources WHERE id = ?1
                UNION ALL
                SELECT 1 FROM jobs_global_discovery_sources WHERE id = ?1
             )",
            params![scope_id],
            |row| row.get::<_, bool>(0),
        )?,
        _ => true,
    };
    exists
        .then_some(())
        .ok_or(OperationalHoldError::InvalidRequest)
}

fn validate_new_operational_hold_scope_postgres(
    tx: &mut postgres::Transaction<'_>,
    scope_kind: OperationalHoldScopeKind,
    scope_id: &str,
) -> std::result::Result<(), OperationalHoldError> {
    validate_preemptive_operational_scope(scope_kind, scope_id)?;
    let exists = match scope_kind {
        OperationalHoldScopeKind::Account => tx
            .query_opt(
                "SELECT 1 FROM accounts WHERE id = $1 FOR KEY SHARE",
                &[&scope_id],
            )?
            .is_some(),
        OperationalHoldScopeKind::CareerTrack => tx
            .query_opt(
                "SELECT 1 FROM jobs_tracks WHERE id = $1 FOR KEY SHARE",
                &[&scope_id],
            )?
            .is_some(),
        OperationalHoldScopeKind::DiscoverySource => {
            tx.query_opt(
                "SELECT 1 FROM jobs_discovery_sources WHERE id = $1 FOR KEY SHARE",
                &[&scope_id],
            )?
            .is_some()
                || tx
                    .query_opt(
                        "SELECT 1 FROM jobs_global_discovery_sources
                          WHERE id = $1 FOR KEY SHARE",
                        &[&scope_id],
                    )?
                    .is_some()
        }
        _ => true,
    };
    exists
        .then_some(())
        .ok_or(OperationalHoldError::InvalidRequest)
}

fn normalize_reason_ref(
    value: Option<&str>,
) -> std::result::Result<Option<String>, OperationalHoldError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let value = value.trim();
    if value.is_empty()
        || value.len() > 120
        || !value
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_alphanumeric())
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(OperationalHoldError::InvalidRequest);
    }
    Ok(Some(value.to_string()))
}

fn validate_append_operational_hold_request(
    request: &AppendOperationalHoldEventRequest,
    recorded_by: &str,
) -> std::result::Result<(String, Option<String>, String), OperationalHoldError> {
    if !valid_operational_identifier(&request.event_id, 128)
        || !valid_operational_identifier(recorded_by, 128)
        || request.expected_head_revision < 0
        || request.expected_head_revision >= OPERATIONAL_HOLD_MAX_REVISION
        || request
            .expected_current_event_id
            .as_deref()
            .is_some_and(|value| !valid_operational_identifier(value, 128))
        || (request.expected_head_revision == 0 && request.expected_current_event_id.is_some())
        || (request.expected_head_revision > 0 && request.expected_current_event_id.is_none())
    {
        return Err(OperationalHoldError::InvalidRequest);
    }
    let scope_id = normalize_operational_scope_id(request.scope_kind, &request.scope_id)?;
    let reason_ref = normalize_reason_ref(request.reason_ref.as_deref())?;
    Ok((scope_id, reason_ref, recorded_by.to_string()))
}

fn operational_hold_event_identity(
    event: &CanonicalOperationalHoldEvent,
) -> std::result::Result<(String, String), OperationalHoldError> {
    let bytes =
        serde_json::to_vec(event).map_err(|error| OperationalHoldError::Storage(error.into()))?;
    Ok((
        hex::encode(Sha256::digest(&bytes)),
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes),
    ))
}

fn decode_operational_hold_event(
    encoded: &str,
) -> std::result::Result<CanonicalOperationalHoldEvent, OperationalHoldError> {
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(encoded)
        .map_err(|error| OperationalHoldError::Storage(error.into()))?;
    serde_json::from_slice(&bytes).map_err(|error| OperationalHoldError::Storage(error.into()))
}

fn operational_hold_scope_ref(
    kind: OperationalHoldScopeKind,
    scope_id: &str,
) -> std::result::Result<String, OperationalHoldError> {
    private_lookup_hash(
        &format!("jobs-operational-hold-scope-v1:{}", kind.as_str()),
        scope_id,
    )
    .map(|digest| format!("scope-{digest}"))
    .map_err(OperationalHoldError::Storage)
}

fn operational_hold_event_ref(event_id: &str) -> std::result::Result<String, OperationalHoldError> {
    private_lookup_hash("jobs-operational-hold-event-v1", event_id)
        .map(|digest| format!("event-{digest}"))
        .map_err(OperationalHoldError::Storage)
}

fn valid_operational_hold_ref(value: &str, prefix: &str) -> bool {
    value.len() == prefix.len() + 64
        && value.starts_with(prefix)
        && value[prefix.len()..]
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn operational_hold_refs_match(expected: &str, presented: &str) -> bool {
    expected.len() == presented.len() && bool::from(expected.as_bytes().ct_eq(presented.as_bytes()))
}

#[derive(Debug)]
struct OperationalHoldCanonicalHeadParts {
    head_capability: String,
    head_scope_kind: String,
    head_scope_id: String,
    head_scope_ref: String,
    head_revision: i64,
    head_event_id: String,
    head_event_ref: String,
    head_state: String,
    head_updated_by: String,
    head_updated_at_ms: i64,
    event_id: Option<String>,
    event_ref: Option<String>,
    event_sha256: Option<String>,
    canonical_event_base64url: Option<String>,
    event_capability: Option<String>,
    event_scope_kind: Option<String>,
    event_scope_id: Option<String>,
    event_revision: Option<i64>,
    event_previous_revision: Option<i64>,
    event_predecessor_id: Option<String>,
    event_transition: Option<String>,
    event_reason_code: Option<String>,
    event_reason_ref: Option<String>,
    event_recorded_by: Option<String>,
    event_recorded_at_ms: Option<i64>,
}

fn operational_hold_canonical_head_from_sqlite_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<OperationalHoldCanonicalHeadParts> {
    Ok(OperationalHoldCanonicalHeadParts {
        head_capability: row.get(0)?,
        head_scope_kind: row.get(1)?,
        head_scope_id: row.get(2)?,
        head_scope_ref: row.get(3)?,
        head_revision: row.get(4)?,
        head_event_id: row.get(5)?,
        head_event_ref: row.get(6)?,
        head_state: row.get(7)?,
        head_updated_by: row.get(8)?,
        head_updated_at_ms: row.get(9)?,
        event_id: row.get(10)?,
        event_ref: row.get(11)?,
        event_sha256: row.get(12)?,
        canonical_event_base64url: row.get(13)?,
        event_capability: row.get(14)?,
        event_scope_kind: row.get(15)?,
        event_scope_id: row.get(16)?,
        event_revision: row.get(17)?,
        event_previous_revision: row.get(18)?,
        event_predecessor_id: row.get(19)?,
        event_transition: row.get(20)?,
        event_reason_code: row.get(21)?,
        event_reason_ref: row.get(22)?,
        event_recorded_by: row.get(23)?,
        event_recorded_at_ms: row.get(24)?,
    })
}

fn operational_hold_canonical_head_from_postgres_row(
    row: postgres::Row,
) -> OperationalHoldCanonicalHeadParts {
    OperationalHoldCanonicalHeadParts {
        head_capability: row.get(0),
        head_scope_kind: row.get(1),
        head_scope_id: row.get(2),
        head_scope_ref: row.get(3),
        head_revision: row.get(4),
        head_event_id: row.get(5),
        head_event_ref: row.get(6),
        head_state: row.get(7),
        head_updated_by: row.get(8),
        head_updated_at_ms: row.get(9),
        event_id: row.get(10),
        event_ref: row.get(11),
        event_sha256: row.get(12),
        canonical_event_base64url: row.get(13),
        event_capability: row.get(14),
        event_scope_kind: row.get(15),
        event_scope_id: row.get(16),
        event_revision: row.get(17),
        event_previous_revision: row.get(18),
        event_predecessor_id: row.get(19),
        event_transition: row.get(20),
        event_reason_code: row.get(21),
        event_reason_ref: row.get(22),
        event_recorded_by: row.get(23),
        event_recorded_at_ms: row.get(24),
    }
}

fn stored_operational_hold_corruption(message: &'static str) -> OperationalHoldError {
    OperationalHoldError::Storage(anyhow::anyhow!(message))
}

fn validate_canonical_operational_hold_event(
    event: &CanonicalOperationalHoldEvent,
) -> std::result::Result<(), OperationalHoldError> {
    let normalized_scope = normalize_operational_scope_id(event.scope_kind, &event.scope_id)
        .map_err(|_| stored_operational_hold_corruption("invalid stored operational hold scope"))?;
    let normalized_reason = normalize_reason_ref(event.reason_ref.as_deref()).map_err(|_| {
        stored_operational_hold_corruption("invalid stored operational hold reason reference")
    })?;
    validate_preemptive_operational_scope(event.scope_kind, &event.scope_id).map_err(|_| {
        stored_operational_hold_corruption("invalid stored operational hold scope authority")
    })?;
    let ancestry_valid = (event.revision_no == 1
        && event.previous_revision_no.is_none()
        && event.predecessor_event_id.is_none()
        && event.transition == OperationalHoldTransition::Held)
        || (event.revision_no > 1
            && event.previous_revision_no == event.revision_no.checked_sub(1)
            && event.predecessor_event_id.is_some());
    if event.schema_version != OPERATIONAL_HOLD_SCHEMA_VERSION
        || !valid_operational_identifier(&event.event_id, 128)
        || !valid_operational_identifier(&event.recorded_by, 128)
        || event.revision_no < 1
        || event.revision_no > OPERATIONAL_HOLD_MAX_REVISION
        || event.recorded_at_ms < 0
        || event.recorded_at_ms > OPERATIONAL_HOLD_MAX_REVISION
        || normalized_scope != event.scope_id
        || normalized_reason != event.reason_ref
        || !ancestry_valid
    {
        return Err(stored_operational_hold_corruption(
            "invalid stored operational hold canonical event",
        ));
    }
    Ok(())
}

fn validated_operational_hold_head_event(
    parts: OperationalHoldCanonicalHeadParts,
) -> std::result::Result<CanonicalOperationalHoldEvent, OperationalHoldError> {
    let head_capability = OperationalCapability::from_storage(&parts.head_capability)?;
    let head_scope_kind = OperationalHoldScopeKind::from_storage(&parts.head_scope_kind)?;
    let head_state = OperationalHoldTransition::from_storage(&parts.head_state)?;
    let event_reason_code = parts
        .event_reason_code
        .as_deref()
        .map(OperationalHoldReasonCode::from_storage)
        .transpose()?;
    let encoded = parts.canonical_event_base64url.as_deref().ok_or_else(|| {
        stored_operational_hold_corruption("Jobs operational hold head has no exact event")
    })?;
    let event = decode_operational_hold_event(encoded)?;
    validate_canonical_operational_hold_event(&event)?;
    let (actual_sha256, actual_encoded) = operational_hold_event_identity(&event)?;
    let actual_scope_ref = operational_hold_scope_ref(head_scope_kind, &parts.head_scope_id)?;
    let actual_event_ref = operational_hold_event_ref(&event.event_id)?;

    if head_capability != event.capability
        || head_scope_kind != event.scope_kind
        || parts.head_scope_id != event.scope_id
        || parts.head_revision != event.revision_no
        || parts.head_event_id != event.event_id
        || head_state != event.transition
        || parts.head_updated_by != event.recorded_by
        || parts.head_updated_at_ms != event.recorded_at_ms
        || !operational_hold_refs_match(&parts.head_scope_ref, &actual_scope_ref)
        || !operational_hold_refs_match(&parts.head_event_ref, &actual_event_ref)
        || !parts
            .event_ref
            .as_deref()
            .is_some_and(|stored| operational_hold_refs_match(stored, &actual_event_ref))
        || parts.event_sha256.as_deref() != Some(actual_sha256.as_str())
        || actual_encoded != encoded
        || parts.event_id.as_deref() != Some(event.event_id.as_str())
        || parts.event_capability.as_deref() != Some(event.capability.as_str())
        || parts.event_scope_kind.as_deref() != Some(event.scope_kind.as_str())
        || parts.event_scope_id.as_deref() != Some(event.scope_id.as_str())
        || parts.event_revision != Some(event.revision_no)
        || parts.event_previous_revision != event.previous_revision_no
        || parts.event_predecessor_id.as_deref() != event.predecessor_event_id.as_deref()
        || parts.event_transition.as_deref() != Some(event.transition.as_str())
        || event_reason_code != Some(event.reason_code)
        || parts.event_reason_ref.as_deref() != event.reason_ref.as_deref()
        || parts.event_recorded_by.as_deref() != Some(event.recorded_by.as_str())
        || parts.event_recorded_at_ms != Some(event.recorded_at_ms)
    {
        return Err(stored_operational_hold_corruption(
            "Jobs operational hold head failed canonical verification",
        ));
    }
    Ok(event)
}

pub fn append_operational_hold_event(
    pool: &DbPool,
    request: &AppendOperationalHoldEventRequest,
    recorded_by: &str,
) -> std::result::Result<OperationalHoldAppendResult, OperationalHoldError> {
    let (scope_id, reason_ref, recorded_by) =
        validate_append_operational_hold_request(request, recorded_by)?;
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => append_operational_hold_event_sqlite(
            pool,
            request,
            &scope_id,
            reason_ref,
            &recorded_by,
            project_operational_hold_public_state,
        ),
        DbPool::Postgres(_) => append_operational_hold_event_postgres(
            pool,
            request,
            &scope_id,
            reason_ref,
            &recorded_by,
        ),
    })
}

pub fn append_operational_hold_event_by_ref(
    pool: &DbPool,
    request: &AppendOperationalHoldEventByRefRequest,
    recorded_by: &str,
) -> std::result::Result<OperationalHoldAppendResult, OperationalHoldError> {
    if !valid_operational_identifier(&request.event_id, 128)
        || !valid_operational_identifier(recorded_by, 128)
        || request.expected_head_revision <= 0
        || request.expected_head_revision >= OPERATIONAL_HOLD_MAX_REVISION
        || !valid_operational_hold_ref(&request.scope_ref, "scope-")
        || !valid_operational_hold_ref(&request.expected_current_event_ref, "event-")
    {
        return Err(OperationalHoldError::InvalidRequest);
    }
    let (scope_id, current_event_id) = resolve_operational_hold_refs(pool, request)?;
    append_operational_hold_event(
        pool,
        &AppendOperationalHoldEventRequest {
            event_id: request.event_id.clone(),
            capability: request.capability,
            scope_kind: request.scope_kind,
            scope_id,
            transition: request.transition,
            reason_code: request.reason_code,
            reason_ref: request.reason_ref.clone(),
            expected_head_revision: request.expected_head_revision,
            expected_current_event_id: Some(current_event_id),
        },
        recorded_by,
    )
}

fn resolve_operational_hold_refs(
    pool: &DbPool,
    request: &AppendOperationalHoldEventByRefRequest,
) -> std::result::Result<(String, String), OperationalHoldError> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => resolve_operational_hold_refs_sqlite(pool, request),
        DbPool::Postgres(_) => resolve_operational_hold_refs_postgres(pool, request),
    })
}

fn resolve_existing_operational_hold_event(
    stored: Option<(String, String, String)>,
    request: &AppendOperationalHoldEventByRefRequest,
) -> std::result::Result<Option<(String, String)>, OperationalHoldError> {
    let Some((stored_ref, stored_sha256, encoded)) = stored else {
        return Ok(None);
    };
    let event = decode_operational_hold_event(&encoded)?;
    validate_canonical_operational_hold_event(&event)?;
    let (actual_sha256, actual_encoded) = operational_hold_event_identity(&event)?;
    let actual_ref = operational_hold_event_ref(&event.event_id)?;
    let predecessor_event_id = event.predecessor_event_id.clone();
    let scope_ref = operational_hold_scope_ref(event.scope_kind, &event.scope_id)?;
    let predecessor_event_ref = predecessor_event_id
        .as_deref()
        .map(operational_hold_event_ref)
        .transpose()?;
    if !operational_hold_refs_match(&stored_ref, &actual_ref)
        || stored_sha256 != actual_sha256
        || encoded != actual_encoded
        || event.event_id != request.event_id
        || event.capability != request.capability
        || event.scope_kind != request.scope_kind
        || event.revision_no != request.expected_head_revision.saturating_add(1)
        || event.previous_revision_no != Some(request.expected_head_revision)
        || !operational_hold_refs_match(&scope_ref, &request.scope_ref)
        || !predecessor_event_ref.as_deref().is_some_and(|event_ref| {
            operational_hold_refs_match(event_ref, &request.expected_current_event_ref)
        })
    {
        return Err(OperationalHoldError::IdentityConflict);
    }
    let predecessor_event_id =
        predecessor_event_id.ok_or(OperationalHoldError::IdentityConflict)?;
    Ok(Some((event.scope_id, predecessor_event_id)))
}

fn match_operational_hold_head_refs<I>(
    rows: I,
    request: &AppendOperationalHoldEventByRefRequest,
) -> std::result::Result<(String, String), OperationalHoldError>
where
    I: IntoIterator<Item = (String, String, String, String)>,
{
    let mut matched = None;
    for (scope_id, stored_scope_ref, current_event_id, stored_current_event_ref) in rows {
        let actual_scope_ref = operational_hold_scope_ref(request.scope_kind, &scope_id)?;
        let actual_current_event_ref = operational_hold_event_ref(&current_event_id)?;
        if !operational_hold_refs_match(&stored_scope_ref, &actual_scope_ref)
            || !operational_hold_refs_match(&stored_current_event_ref, &actual_current_event_ref)
        {
            return Err(OperationalHoldError::Storage(anyhow::anyhow!(
                "invalid stored Jobs operational hold references"
            )));
        }
        if operational_hold_refs_match(&stored_scope_ref, &request.scope_ref)
            && operational_hold_refs_match(
                &stored_current_event_ref,
                &request.expected_current_event_ref,
            )
        {
            if matched.is_some() {
                return Err(OperationalHoldError::Storage(anyhow::anyhow!(
                    "ambiguous Jobs operational hold references"
                )));
            }
            matched = Some((scope_id, current_event_id));
        }
    }
    matched.ok_or(OperationalHoldError::Conflict)
}

fn resolve_operational_hold_refs_sqlite(
    pool: &DbPool,
    request: &AppendOperationalHoldEventByRefRequest,
) -> std::result::Result<(String, String), OperationalHoldError> {
    let conn = pool.get().map_err(OperationalHoldError::Storage)?;
    let existing = conn
        .query_row(
            "SELECT event_ref, event_sha256, canonical_event_base64url
               FROM jobs_operational_hold_events WHERE event_id = ?1",
            params![request.event_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            },
        )
        .optional()
        .map_err(OperationalHoldError::from)?;
    if let Some(target) = resolve_existing_operational_hold_event(existing, request)? {
        return Ok(target);
    }
    let mut statement = conn
        .prepare(
            "SELECT head.capability, head.scope_kind, head.scope_id, head.scope_ref,
                    head.head_revision, head.current_event_id, head.current_event_ref,
                    head.state, head.updated_by, head.updated_at_ms,
                    event.event_id, event.event_ref, event.event_sha256,
                    event.canonical_event_base64url, event.capability, event.scope_kind,
                    event.scope_id, event.revision_no, event.previous_revision_no,
                    event.predecessor_event_id, event.transition, event.reason_code,
                    event.reason_ref, event.recorded_by, event.recorded_at_ms
               FROM jobs_operational_hold_heads head
               LEFT JOIN jobs_operational_hold_events event
                 ON event.event_id = head.current_event_id
              WHERE head.capability = ?1 AND head.scope_kind = ?2
                AND head.scope_ref = ?3 AND head.head_revision = ?4",
        )
        .map_err(OperationalHoldError::from)?;
    let rows = statement
        .query_map(
            params![
                request.capability.as_str(),
                request.scope_kind.as_str(),
                request.scope_ref,
                request.expected_head_revision,
            ],
            operational_hold_canonical_head_from_sqlite_row,
        )
        .map_err(OperationalHoldError::from)?
        .map(|row| {
            let event =
                validated_operational_hold_head_event(row.map_err(OperationalHoldError::from)?)?;
            Ok((
                event.scope_id.clone(),
                operational_hold_scope_ref(event.scope_kind, &event.scope_id)?,
                event.event_id.clone(),
                operational_hold_event_ref(&event.event_id)?,
            ))
        })
        .collect::<std::result::Result<Vec<_>, OperationalHoldError>>()?;
    match_operational_hold_head_refs(rows, request)
}

fn resolve_operational_hold_refs_postgres(
    pool: &DbPool,
    request: &AppendOperationalHoldEventByRefRequest,
) -> std::result::Result<(String, String), OperationalHoldError> {
    let mut conn = pool.get_pg().map_err(OperationalHoldError::Storage)?;
    let existing = conn
        .query_opt(
            "SELECT event_ref, event_sha256, canonical_event_base64url
               FROM jobs_operational_hold_events WHERE event_id = $1",
            &[&request.event_id],
        )
        .map_err(OperationalHoldError::from)?
        .map(|row| {
            (
                row.get::<_, String>(0),
                row.get::<_, String>(1),
                row.get::<_, String>(2),
            )
        });
    if let Some(target) = resolve_existing_operational_hold_event(existing, request)? {
        return Ok(target);
    }
    let rows = conn
        .query(
            "SELECT head.capability, head.scope_kind, head.scope_id, head.scope_ref,
                    head.head_revision, head.current_event_id, head.current_event_ref,
                    head.state, head.updated_by, head.updated_at_ms,
                    event.event_id, event.event_ref, event.event_sha256,
                    event.canonical_event_base64url, event.capability, event.scope_kind,
                    event.scope_id, event.revision_no, event.previous_revision_no,
                    event.predecessor_event_id, event.transition, event.reason_code,
                    event.reason_ref, event.recorded_by, event.recorded_at_ms
               FROM jobs_operational_hold_heads head
               LEFT JOIN jobs_operational_hold_events event
                 ON event.event_id = head.current_event_id
              WHERE head.capability = $1 AND head.scope_kind = $2
                AND head.scope_ref = $3 AND head.head_revision = $4",
            &[
                &request.capability.as_str(),
                &request.scope_kind.as_str(),
                &request.scope_ref,
                &request.expected_head_revision,
            ],
        )
        .map_err(OperationalHoldError::from)?
        .into_iter()
        .map(|row| {
            let event = validated_operational_hold_head_event(
                operational_hold_canonical_head_from_postgres_row(row),
            )?;
            Ok((
                event.scope_id.clone(),
                operational_hold_scope_ref(event.scope_kind, &event.scope_id)?,
                event.event_id.clone(),
                operational_hold_event_ref(&event.event_id)?,
            ))
        });
    match_operational_hold_head_refs(
        rows.collect::<std::result::Result<Vec<_>, OperationalHoldError>>()?,
        request,
    )
}

fn append_operational_hold_event_sqlite(
    pool: &DbPool,
    request: &AppendOperationalHoldEventRequest,
    scope_id: &str,
    reason_ref: Option<String>,
    recorded_by: &str,
    project_public_state: fn(
        &CanonicalOperationalHoldEvent,
        String,
    ) -> std::result::Result<
        OperationalHoldPublicState,
        OperationalHoldError,
    >,
) -> std::result::Result<OperationalHoldAppendResult, OperationalHoldError> {
    let mut conn = pool.get().map_err(OperationalHoldError::Storage)?;
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(OperationalHoldError::from)?;
    if let Some(result) = operational_hold_replay_sqlite(&tx, request, scope_id, recorded_by)? {
        tx.commit().map_err(OperationalHoldError::from)?;
        return Ok(result);
    }
    let head = tx
        .query_row(
            "SELECT head.capability, head.scope_kind, head.scope_id, head.scope_ref,
                    head.head_revision, head.current_event_id, head.current_event_ref,
                    head.state, head.updated_by, head.updated_at_ms,
                    event.event_id, event.event_ref, event.event_sha256,
                    event.canonical_event_base64url, event.capability, event.scope_kind,
                    event.scope_id, event.revision_no, event.previous_revision_no,
                    event.predecessor_event_id, event.transition, event.reason_code,
                    event.reason_ref, event.recorded_by, event.recorded_at_ms
               FROM jobs_operational_hold_heads AS head
               LEFT JOIN jobs_operational_hold_events AS event
                 ON event.event_id = head.current_event_id
              WHERE head.capability = ?1 AND head.scope_kind = ?2 AND head.scope_id = ?3",
            params![
                request.capability.as_str(),
                request.scope_kind.as_str(),
                scope_id
            ],
            operational_hold_canonical_head_from_sqlite_row,
        )
        .optional()
        .map_err(OperationalHoldError::from)?
        .map(validated_operational_hold_head_event)
        .transpose()?
        .map(|event| {
            (
                event.revision_no,
                event.event_id,
                event.recorded_at_ms,
                event.transition.as_str().to_string(),
            )
        });
    validate_expected_operational_head(request, head.as_ref())?;
    if head.is_none() {
        validate_new_operational_hold_scope_sqlite(&tx, request.scope_kind, scope_id)?;
    }
    let event = build_operational_hold_event(request, scope_id, reason_ref, recorded_by, head)?;
    let (event_sha256, canonical_event_base64url) = operational_hold_event_identity(&event)?;
    let scope_ref = operational_hold_scope_ref(event.scope_kind, &event.scope_id)?;
    let event_ref = operational_hold_event_ref(&event.event_id)?;
    tx.execute(
        "INSERT INTO jobs_operational_hold_events (
            event_id, event_ref, event_sha256, canonical_event_base64url, capability,
            scope_kind, scope_id, revision_no, previous_revision_no,
            predecessor_event_id, transition, reason_code, reason_ref,
            recorded_by, recorded_at_ms
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
        params![
            event.event_id,
            event_ref,
            event_sha256,
            canonical_event_base64url,
            event.capability.as_str(),
            event.scope_kind.as_str(),
            event.scope_id,
            event.revision_no,
            event.previous_revision_no,
            event.predecessor_event_id,
            event.transition.as_str(),
            event.reason_code.as_str(),
            event.reason_ref,
            event.recorded_by,
            event.recorded_at_ms,
        ],
    )
    .map_err(OperationalHoldError::from)?;
    if event.revision_no == 1 {
        tx.execute(
            "INSERT INTO jobs_operational_hold_heads (
                capability, scope_kind, scope_id, scope_ref, head_revision, current_event_id,
                current_event_ref, state, updated_by, updated_at_ms
             ) VALUES (?1, ?2, ?3, ?4, 1, ?5, ?6, ?7, ?8, ?9)",
            params![
                event.capability.as_str(),
                event.scope_kind.as_str(),
                event.scope_id,
                scope_ref,
                event.event_id,
                event_ref,
                event.transition.as_str(),
                event.recorded_by,
                event.recorded_at_ms,
            ],
        )
        .map_err(OperationalHoldError::from)?;
    } else if tx
        .execute(
            "UPDATE jobs_operational_hold_heads
                SET head_revision = ?4, current_event_id = ?5, current_event_ref = ?6,
                    state = ?7, updated_by = ?8, updated_at_ms = ?9
              WHERE capability = ?1 AND scope_kind = ?2 AND scope_id = ?3
                AND head_revision = ?10 AND current_event_id = ?11",
            params![
                event.capability.as_str(),
                event.scope_kind.as_str(),
                event.scope_id,
                event.revision_no,
                event.event_id,
                event_ref,
                event.transition.as_str(),
                event.recorded_by,
                event.recorded_at_ms,
                event.previous_revision_no,
                event.predecessor_event_id,
            ],
        )
        .map_err(OperationalHoldError::from)?
        != 1
    {
        return Err(OperationalHoldError::Conflict);
    }
    let result = OperationalHoldAppendResult {
        state: project_public_state(&event, event_sha256)?,
        replayed: false,
    };
    tx.commit().map_err(OperationalHoldError::from)?;
    Ok(result)
}

fn append_operational_hold_event_postgres(
    pool: &DbPool,
    request: &AppendOperationalHoldEventRequest,
    scope_id: &str,
    reason_ref: Option<String>,
    recorded_by: &str,
) -> std::result::Result<OperationalHoldAppendResult, OperationalHoldError> {
    let mut conn = pool.get_pg().map_err(OperationalHoldError::Storage)?;
    let mut tx = conn.transaction().map_err(OperationalHoldError::from)?;
    tx.query_one(POSTGRES_OPERATIONAL_HOLD_EXCLUSIVE_LOCK_SQL, &[])
        .map_err(OperationalHoldError::from)?;
    if let Some(result) = operational_hold_replay_postgres(&mut tx, request, scope_id, recorded_by)?
    {
        tx.commit().map_err(OperationalHoldError::from)?;
        return Ok(result);
    }
    let head = tx
        .query_opt(
            "SELECT head.capability, head.scope_kind, head.scope_id, head.scope_ref,
                    head.head_revision, head.current_event_id, head.current_event_ref,
                    head.state, head.updated_by, head.updated_at_ms,
                    event.event_id, event.event_ref, event.event_sha256,
                    event.canonical_event_base64url, event.capability, event.scope_kind,
                    event.scope_id, event.revision_no, event.previous_revision_no,
                    event.predecessor_event_id, event.transition, event.reason_code,
                    event.reason_ref, event.recorded_by, event.recorded_at_ms
               FROM jobs_operational_hold_heads AS head
               LEFT JOIN jobs_operational_hold_events AS event
                 ON event.event_id = head.current_event_id
              WHERE head.capability = $1 AND head.scope_kind = $2 AND head.scope_id = $3
              FOR UPDATE OF head",
            &[
                &request.capability.as_str(),
                &request.scope_kind.as_str(),
                &scope_id,
            ],
        )
        .map_err(OperationalHoldError::from)?
        .map(operational_hold_canonical_head_from_postgres_row)
        .map(validated_operational_hold_head_event)
        .transpose()?
        .map(|event| {
            (
                event.revision_no,
                event.event_id,
                event.recorded_at_ms,
                event.transition.as_str().to_string(),
            )
        });
    validate_expected_operational_head(request, head.as_ref())?;
    if head.is_none() {
        validate_new_operational_hold_scope_postgres(&mut tx, request.scope_kind, scope_id)?;
    }
    let event = build_operational_hold_event(request, scope_id, reason_ref, recorded_by, head)?;
    let (event_sha256, canonical_event_base64url) = operational_hold_event_identity(&event)?;
    let scope_ref = operational_hold_scope_ref(event.scope_kind, &event.scope_id)?;
    let event_ref = operational_hold_event_ref(&event.event_id)?;
    tx.execute(
        "INSERT INTO jobs_operational_hold_events (
            event_id, event_ref, event_sha256, canonical_event_base64url, capability,
            scope_kind, scope_id, revision_no, previous_revision_no,
            predecessor_event_id, transition, reason_code, reason_ref,
            recorded_by, recorded_at_ms
         ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)",
        &[
            &event.event_id,
            &event_ref,
            &event_sha256,
            &canonical_event_base64url,
            &event.capability.as_str(),
            &event.scope_kind.as_str(),
            &event.scope_id,
            &event.revision_no,
            &event.previous_revision_no,
            &event.predecessor_event_id,
            &event.transition.as_str(),
            &event.reason_code.as_str(),
            &event.reason_ref,
            &event.recorded_by,
            &event.recorded_at_ms,
        ],
    )
    .map_err(OperationalHoldError::from)?;
    if event.revision_no == 1 {
        tx.execute(
            "INSERT INTO jobs_operational_hold_heads (
                capability, scope_kind, scope_id, scope_ref, head_revision, current_event_id,
                current_event_ref, state, updated_by, updated_at_ms
             ) VALUES ($1, $2, $3, $4, 1, $5, $6, $7, $8, $9)",
            &[
                &event.capability.as_str(),
                &event.scope_kind.as_str(),
                &event.scope_id,
                &scope_ref,
                &event.event_id,
                &event_ref,
                &event.transition.as_str(),
                &event.recorded_by,
                &event.recorded_at_ms,
            ],
        )
        .map_err(OperationalHoldError::from)?;
    } else if tx
        .execute(
            "UPDATE jobs_operational_hold_heads
                SET head_revision = $4, current_event_id = $5, current_event_ref = $6,
                    state = $7, updated_by = $8, updated_at_ms = $9
              WHERE capability = $1 AND scope_kind = $2 AND scope_id = $3
                AND head_revision = $10 AND current_event_id = $11",
            &[
                &event.capability.as_str(),
                &event.scope_kind.as_str(),
                &event.scope_id,
                &event.revision_no,
                &event.event_id,
                &event_ref,
                &event.transition.as_str(),
                &event.recorded_by,
                &event.recorded_at_ms,
                &event.previous_revision_no,
                &event.predecessor_event_id,
            ],
        )
        .map_err(OperationalHoldError::from)?
        != 1
    {
        return Err(OperationalHoldError::Conflict);
    }
    let result = OperationalHoldAppendResult {
        state: project_operational_hold_public_state(&event, event_sha256)?,
        replayed: false,
    };
    tx.commit().map_err(OperationalHoldError::from)?;
    Ok(result)
}

fn validate_expected_operational_head(
    request: &AppendOperationalHoldEventRequest,
    head: Option<&(i64, String, i64, String)>,
) -> std::result::Result<(), OperationalHoldError> {
    match head {
        None if request.expected_head_revision == 0
            && request.expected_current_event_id.is_none()
            && request.transition == OperationalHoldTransition::Held =>
        {
            Ok(())
        }
        Some((revision, event_id, _, state))
            if *revision == request.expected_head_revision
                && request.expected_current_event_id.as_deref() == Some(event_id.as_str()) =>
        {
            let state = OperationalHoldTransition::from_storage(state)?;
            if state == OperationalHoldTransition::Released
                && request.transition == OperationalHoldTransition::Released
            {
                Err(OperationalHoldError::Conflict)
            } else {
                Ok(())
            }
        }
        _ => Err(OperationalHoldError::Conflict),
    }
}

fn build_operational_hold_event(
    request: &AppendOperationalHoldEventRequest,
    scope_id: &str,
    reason_ref: Option<String>,
    recorded_by: &str,
    head: Option<(i64, String, i64, String)>,
) -> std::result::Result<CanonicalOperationalHoldEvent, OperationalHoldError> {
    let (revision_no, previous_revision_no, predecessor_event_id, predecessor_recorded_at_ms) =
        match head {
            Some((revision, event_id, recorded_at_ms, _)) => (
                revision
                    .checked_add(1)
                    .filter(|value| *value <= OPERATIONAL_HOLD_MAX_REVISION)
                    .ok_or(OperationalHoldError::Conflict)?,
                Some(revision),
                Some(event_id),
                recorded_at_ms,
            ),
            None => (1, None, None, 0),
        };
    Ok(CanonicalOperationalHoldEvent {
        schema_version: OPERATIONAL_HOLD_SCHEMA_VERSION,
        event_id: request.event_id.clone(),
        capability: request.capability,
        scope_kind: request.scope_kind,
        scope_id: scope_id.to_string(),
        revision_no,
        previous_revision_no,
        predecessor_event_id,
        transition: request.transition,
        reason_code: request.reason_code,
        reason_ref,
        recorded_by: recorded_by.to_string(),
        recorded_at_ms: now_ms().max(predecessor_recorded_at_ms),
    })
}

fn operational_hold_replay_matches(
    event: &CanonicalOperationalHoldEvent,
    request: &AppendOperationalHoldEventRequest,
    scope_id: &str,
    recorded_by: &str,
) -> bool {
    let expected_revision = request.expected_head_revision.checked_add(1);
    let expected_previous_revision =
        (request.expected_head_revision > 0).then_some(request.expected_head_revision);
    event.schema_version == OPERATIONAL_HOLD_SCHEMA_VERSION
        && event.event_id == request.event_id
        && event.capability == request.capability
        && event.scope_kind == request.scope_kind
        && event.scope_id == scope_id
        && event.transition == request.transition
        && event.reason_code == request.reason_code
        && event.reason_ref.as_deref() == request.reason_ref.as_deref().map(str::trim)
        && event.recorded_by == recorded_by
        && Some(event.revision_no) == expected_revision
        && event.previous_revision_no == expected_previous_revision
        && event.predecessor_event_id == request.expected_current_event_id
}

fn operational_hold_replay_sqlite(
    tx: &rusqlite::Transaction<'_>,
    request: &AppendOperationalHoldEventRequest,
    scope_id: &str,
    recorded_by: &str,
) -> std::result::Result<Option<OperationalHoldAppendResult>, OperationalHoldError> {
    let stored = tx
        .query_row(
            "SELECT event_ref, event_sha256, canonical_event_base64url
               FROM jobs_operational_hold_events WHERE event_id = ?1",
            params![request.event_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            },
        )
        .optional()
        .map_err(OperationalHoldError::from)?;
    let Some((stored_event_ref, stored_sha256, encoded)) = stored else {
        return Ok(None);
    };
    let event = decode_operational_hold_event(&encoded)?;
    validate_canonical_operational_hold_event(&event)?;
    let (actual_sha256, actual_encoded) = operational_hold_event_identity(&event)?;
    let actual_event_ref = operational_hold_event_ref(&event.event_id)?;
    if !operational_hold_refs_match(&stored_event_ref, &actual_event_ref)
        || stored_sha256 != actual_sha256
        || encoded != actual_encoded
        || !operational_hold_replay_matches(&event, request, scope_id, recorded_by)
    {
        return Err(OperationalHoldError::IdentityConflict);
    }
    Ok(Some(OperationalHoldAppendResult {
        state: operational_hold_state(&event, stored_sha256).redacted()?,
        replayed: true,
    }))
}

fn operational_hold_replay_postgres(
    tx: &mut postgres::Transaction<'_>,
    request: &AppendOperationalHoldEventRequest,
    scope_id: &str,
    recorded_by: &str,
) -> std::result::Result<Option<OperationalHoldAppendResult>, OperationalHoldError> {
    let stored = tx
        .query_opt(
            "SELECT event_ref, event_sha256, canonical_event_base64url
               FROM jobs_operational_hold_events WHERE event_id = $1 FOR SHARE",
            &[&request.event_id],
        )
        .map_err(OperationalHoldError::from)?
        .map(|row| {
            (
                row.get::<_, String>(0),
                row.get::<_, String>(1),
                row.get::<_, String>(2),
            )
        });
    let Some((stored_event_ref, stored_sha256, encoded)) = stored else {
        return Ok(None);
    };
    let event = decode_operational_hold_event(&encoded)?;
    validate_canonical_operational_hold_event(&event)?;
    let (actual_sha256, actual_encoded) = operational_hold_event_identity(&event)?;
    let actual_event_ref = operational_hold_event_ref(&event.event_id)?;
    if !operational_hold_refs_match(&stored_event_ref, &actual_event_ref)
        || stored_sha256 != actual_sha256
        || encoded != actual_encoded
        || !operational_hold_replay_matches(&event, request, scope_id, recorded_by)
    {
        return Err(OperationalHoldError::IdentityConflict);
    }
    Ok(Some(OperationalHoldAppendResult {
        state: operational_hold_state(&event, stored_sha256).redacted()?,
        replayed: true,
    }))
}

fn operational_hold_state(
    event: &CanonicalOperationalHoldEvent,
    event_sha256: String,
) -> OperationalHoldState {
    OperationalHoldState {
        capability: event.capability,
        scope_kind: event.scope_kind,
        scope_id: event.scope_id.clone(),
        head_revision: event.revision_no,
        current_event_id: event.event_id.clone(),
        event_sha256,
        state: event.transition,
        reason_code: event.reason_code,
        reason_ref: event.reason_ref.clone(),
        recorded_by: event.recorded_by.clone(),
        recorded_at_ms: event.recorded_at_ms,
    }
}

fn project_operational_hold_public_state(
    event: &CanonicalOperationalHoldEvent,
    event_sha256: String,
) -> std::result::Result<OperationalHoldPublicState, OperationalHoldError> {
    operational_hold_state(event, event_sha256).redacted()
}

fn decode_operational_hold_list_cursor(
    value: Option<&str>,
    active_only: bool,
) -> std::result::Result<Option<OperationalHoldListCursor>, OperationalHoldError> {
    let Some(value) = value else {
        return Ok(None);
    };
    if value.len() > 2_048 || !value.starts_with(ENCRYPTED_PAYLOAD_PREFIX) {
        return Err(OperationalHoldError::InvalidRequest);
    }
    validate_data_encryption_config().map_err(OperationalHoldError::Storage)?;
    let plain = decrypt_payload(value).map_err(|_| OperationalHoldError::InvalidRequest)?;
    let cursor: OperationalHoldListCursor =
        serde_json::from_str(&plain).map_err(|_| OperationalHoldError::InvalidRequest)?;
    let normalized = normalize_operational_scope_id(cursor.scope_kind, &cursor.scope_id)?;
    if cursor.purpose != "jobs_operational_hold_list_v1"
        || cursor.active_only != active_only
        || normalized != cursor.scope_id
    {
        return Err(OperationalHoldError::InvalidRequest);
    }
    Ok(Some(cursor))
}

fn encode_operational_hold_list_cursor(
    active_only: bool,
    state: &OperationalHoldState,
) -> std::result::Result<String, OperationalHoldError> {
    to_json(
        &OperationalHoldListCursor {
            purpose: "jobs_operational_hold_list_v1".to_string(),
            active_only,
            capability: state.capability,
            scope_kind: state.scope_kind,
            scope_id: state.scope_id.clone(),
        },
        "Jobs operational hold list cursor",
    )
    .map_err(OperationalHoldError::Storage)
}

pub fn list_operational_hold_states(
    pool: &DbPool,
    active_only: bool,
    limit: usize,
    cursor: Option<&str>,
) -> std::result::Result<OperationalHoldListPage, OperationalHoldError> {
    if !(1..=500).contains(&limit) {
        return Err(OperationalHoldError::InvalidRequest);
    }
    let cursor = decode_operational_hold_list_cursor(cursor, active_only)?;
    let fetch_limit = limit
        .checked_add(1)
        .and_then(|value| i64::try_from(value).ok())
        .ok_or(OperationalHoldError::InvalidRequest)?;
    let cursor_capability = cursor.as_ref().map(|cursor| cursor.capability.as_str());
    let cursor_scope_kind = cursor.as_ref().map(|cursor| cursor.scope_kind.as_str());
    let cursor_scope_id = cursor.as_ref().map(|cursor| cursor.scope_id.as_str());
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let conn = pool.get().map_err(OperationalHoldError::Storage)?;
            validated_operational_hold_active_counts_sqlite(&conn)?;
            let mut statement = conn
                .prepare(
                    "SELECT head.capability, head.scope_kind, head.scope_id, head.scope_ref,
                            head.head_revision, head.current_event_id, head.current_event_ref,
                            head.state, head.updated_by, head.updated_at_ms,
                            event.event_id, event.event_ref, event.event_sha256,
                            event.canonical_event_base64url, event.capability, event.scope_kind,
                            event.scope_id, event.revision_no, event.previous_revision_no,
                            event.predecessor_event_id, event.transition, event.reason_code,
                            event.reason_ref, event.recorded_by, event.recorded_at_ms
                       FROM jobs_operational_hold_heads head
                       LEFT JOIN jobs_operational_hold_events event
                         ON event.event_id = head.current_event_id
                      WHERE (?1 = 0 OR head.state = 'held')
                        AND (?2 IS NULL OR head.capability > ?2
                          OR (head.capability = ?2 AND head.scope_kind > ?3)
                          OR (head.capability = ?2 AND head.scope_kind = ?3
                            AND head.scope_id > ?4))
                      ORDER BY head.capability, head.scope_kind, head.scope_id
                      LIMIT ?5",
                )
                .map_err(OperationalHoldError::from)?;
            let rows = statement
                .query_map(
                    params![
                        i64::from(active_only),
                        cursor_capability,
                        cursor_scope_kind,
                        cursor_scope_id,
                        fetch_limit,
                    ],
                    operational_hold_canonical_head_from_sqlite_row,
                )
                .map_err(OperationalHoldError::from)?;
            rows.map(|row| {
                let event = validated_operational_hold_head_event(
                    row.map_err(OperationalHoldError::from)?,
                )?;
                let (event_sha256, _) = operational_hold_event_identity(&event)?;
                Ok(operational_hold_state(&event, event_sha256))
            })
            .collect::<std::result::Result<Vec<_>, _>>()
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg().map_err(OperationalHoldError::Storage)?;
            validated_operational_hold_active_counts_postgres(&mut conn)?;
            conn.query(
                "SELECT head.capability, head.scope_kind, head.scope_id, head.scope_ref,
                        head.head_revision, head.current_event_id, head.current_event_ref,
                        head.state, head.updated_by, head.updated_at_ms,
                        event.event_id, event.event_ref, event.event_sha256,
                        event.canonical_event_base64url, event.capability, event.scope_kind,
                        event.scope_id, event.revision_no, event.previous_revision_no,
                        event.predecessor_event_id, event.transition, event.reason_code,
                        event.reason_ref, event.recorded_by, event.recorded_at_ms
                   FROM jobs_operational_hold_heads head
                   LEFT JOIN jobs_operational_hold_events event
                     ON event.event_id = head.current_event_id
                  WHERE (NOT $1 OR head.state = 'held')
                    AND ($2::TEXT IS NULL OR head.capability > $2
                      OR (head.capability = $2 AND head.scope_kind > $3)
                      OR (head.capability = $2 AND head.scope_kind = $3
                        AND head.scope_id > $4))
                  ORDER BY head.capability, head.scope_kind, head.scope_id
                  LIMIT $5",
                &[
                    &active_only,
                    &cursor_capability,
                    &cursor_scope_kind,
                    &cursor_scope_id,
                    &fetch_limit,
                ],
            )
            .map_err(OperationalHoldError::from)?
            .into_iter()
            .map(|row| {
                let event = validated_operational_hold_head_event(
                    operational_hold_canonical_head_from_postgres_row(row),
                )?;
                let (event_sha256, _) = operational_hold_event_identity(&event)?;
                Ok(operational_hold_state(&event, event_sha256))
            })
            .collect::<std::result::Result<Vec<_>, _>>()
        }
    })
    .and_then(|mut states| {
        let has_more = states.len() > limit;
        states.truncate(limit);
        let next_cursor = if has_more {
            states
                .last()
                .map(|state| encode_operational_hold_list_cursor(active_only, state))
                .transpose()?
        } else {
            None
        };
        Ok(OperationalHoldListPage {
            states,
            next_cursor,
        })
    })
}

fn operational_hold_active_counts<I>(
    events: I,
) -> std::result::Result<Vec<(String, String, i64)>, OperationalHoldError>
where
    I: IntoIterator<
        Item = std::result::Result<CanonicalOperationalHoldEvent, OperationalHoldError>,
    >,
{
    let mut counts = BTreeMap::<(String, String), i64>::new();
    for event in events {
        let event = event?;
        if event.transition != OperationalHoldTransition::Held {
            continue;
        }
        let count = counts
            .entry((
                event.capability.as_str().to_string(),
                event.scope_kind.as_str().to_string(),
            ))
            .or_default();
        *count = count.checked_add(1).ok_or_else(|| {
            stored_operational_hold_corruption("Jobs operational hold count overflow")
        })?;
    }
    Ok(counts
        .into_iter()
        .map(|((capability, scope_kind), count)| (capability, scope_kind, count))
        .collect())
}

pub(crate) fn validated_operational_hold_active_counts_sqlite(
    conn: &rusqlite::Connection,
) -> std::result::Result<Vec<(String, String, i64)>, OperationalHoldError> {
    let mut statement = conn
        .prepare(
            "SELECT head.capability, head.scope_kind, head.scope_id, head.scope_ref,
                    head.head_revision, head.current_event_id, head.current_event_ref,
                    head.state, head.updated_by, head.updated_at_ms,
                    event.event_id, event.event_ref, event.event_sha256,
                    event.canonical_event_base64url, event.capability, event.scope_kind,
                    event.scope_id, event.revision_no, event.previous_revision_no,
                    event.predecessor_event_id, event.transition, event.reason_code,
                    event.reason_ref, event.recorded_by, event.recorded_at_ms
               FROM jobs_operational_hold_heads head
               LEFT JOIN jobs_operational_hold_events event
                 ON event.event_id = head.current_event_id
              ORDER BY head.capability, head.scope_kind, head.scope_id",
        )
        .map_err(OperationalHoldError::from)?;
    let rows = statement
        .query_map([], operational_hold_canonical_head_from_sqlite_row)
        .map_err(OperationalHoldError::from)?;
    operational_hold_active_counts(
        rows.map(|row| {
            validated_operational_hold_head_event(row.map_err(OperationalHoldError::from)?)
        }),
    )
}

pub(crate) fn validated_operational_hold_active_counts_postgres(
    conn: &mut postgres::Client,
) -> std::result::Result<Vec<(String, String, i64)>, OperationalHoldError> {
    let rows = conn
        .query(
            "SELECT head.capability, head.scope_kind, head.scope_id, head.scope_ref,
                    head.head_revision, head.current_event_id, head.current_event_ref,
                    head.state, head.updated_by, head.updated_at_ms,
                    event.event_id, event.event_ref, event.event_sha256,
                    event.canonical_event_base64url, event.capability, event.scope_kind,
                    event.scope_id, event.revision_no, event.previous_revision_no,
                    event.predecessor_event_id, event.transition, event.reason_code,
                    event.reason_ref, event.recorded_by, event.recorded_at_ms
               FROM jobs_operational_hold_heads head
               LEFT JOIN jobs_operational_hold_events event
                 ON event.event_id = head.current_event_id
              ORDER BY head.capability, head.scope_kind, head.scope_id",
            &[],
        )
        .map_err(OperationalHoldError::from)?;
    operational_hold_active_counts(rows.into_iter().map(|row| {
        validated_operational_hold_head_event(operational_hold_canonical_head_from_postgres_row(
            row,
        ))
    }))
}

pub fn evaluate_operational_capability(
    pool: &DbPool,
    capability: OperationalCapability,
    context: &OperationalHoldContext,
) -> std::result::Result<OperationalCapabilityEvaluation, OperationalHoldError> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get().map_err(OperationalHoldError::Storage)?;
            let tx = conn
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(OperationalHoldError::from)?;
            let result = evaluate_operational_capability_sqlite_tx(&tx, capability, context)?;
            tx.commit().map_err(OperationalHoldError::from)?;
            Ok(result)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg().map_err(OperationalHoldError::Storage)?;
            let mut tx = conn.transaction().map_err(OperationalHoldError::from)?;
            let result = evaluate_operational_capability_postgres_tx(&mut tx, capability, context)?;
            tx.commit().map_err(OperationalHoldError::from)?;
            Ok(result)
        }
    })
}

pub fn require_operational_capability(
    pool: &DbPool,
    capability: OperationalCapability,
    context: &OperationalHoldContext,
) -> std::result::Result<(), OperationalHoldError> {
    match evaluate_operational_capability(pool, capability, context)? {
        OperationalCapabilityEvaluation::Allowed => Ok(()),
        OperationalCapabilityEvaluation::Held(block) => Err(OperationalHoldError::Held(block)),
    }
}

pub(crate) fn operational_hold_allows(
    result: std::result::Result<(), OperationalHoldError>,
) -> anyhow::Result<bool> {
    match result {
        Ok(()) => Ok(true),
        Err(OperationalHoldError::Held(_)) => Ok(false),
        Err(error) => Err(anyhow::Error::new(error)),
    }
}

fn operational_hold_block(event: CanonicalOperationalHoldEvent) -> OperationalHoldBlock {
    OperationalHoldBlock {
        capability: event.capability,
        scope_kind: event.scope_kind,
        scope_id: event.scope_id,
        reason_code: event.reason_code,
        reason_ref: event.reason_ref,
        head_revision: event.revision_no,
    }
}

pub(crate) fn evaluate_operational_capability_sqlite_tx(
    tx: &rusqlite::Transaction<'_>,
    capability: OperationalCapability,
    context: &OperationalHoldContext,
) -> std::result::Result<OperationalCapabilityEvaluation, OperationalHoldError> {
    let scope_pairs = context.exact_scope_pairs();
    let scope_predicate = (0..scope_pairs.len())
        .map(|_| "(head.scope_kind = ? AND head.scope_id = ?)")
        .collect::<Vec<_>>()
        .join(" OR ");
    let sql = format!(
        "SELECT head.capability, head.scope_kind, head.scope_id, head.scope_ref,
                head.head_revision, head.current_event_id, head.current_event_ref,
                head.state, head.updated_by, head.updated_at_ms,
                event.event_id, event.event_ref, event.event_sha256,
                event.canonical_event_base64url, event.capability, event.scope_kind,
                event.scope_id, event.revision_no, event.previous_revision_no,
                event.predecessor_event_id, event.transition, event.reason_code,
                event.reason_ref, event.recorded_by, event.recorded_at_ms
           FROM jobs_operational_hold_heads head
           LEFT JOIN jobs_operational_hold_events event
             ON event.event_id = head.current_event_id
          WHERE head.capability IN ('all', ?) AND ({scope_predicate})
          ORDER BY CASE WHEN head.capability = 'all' THEN 0 ELSE 1 END,
                   head.scope_kind, head.scope_id"
    );
    let mut parameters = Vec::with_capacity(1 + scope_pairs.len() * 2);
    parameters.push(capability.as_str().to_string());
    for (scope_kind, scope_id) in &scope_pairs {
        parameters.push(scope_kind.clone());
        parameters.push(scope_id.clone());
    }
    let mut statement = tx.prepare(&sql).map_err(OperationalHoldError::from)?;
    let rows = statement
        .query_map(
            rusqlite::params_from_iter(parameters.iter()),
            operational_hold_canonical_head_from_sqlite_row,
        )
        .map_err(OperationalHoldError::from)?;
    for row in rows {
        let event =
            validated_operational_hold_head_event(row.map_err(OperationalHoldError::from)?)?;
        if event.transition == OperationalHoldTransition::Held
            && context.matches(event.scope_kind, &event.scope_id)
        {
            return Ok(OperationalCapabilityEvaluation::Held(
                operational_hold_block(event),
            ));
        }
    }
    Ok(OperationalCapabilityEvaluation::Allowed)
}

pub(crate) fn evaluate_operational_capability_postgres_tx(
    tx: &mut postgres::Transaction<'_>,
    capability: OperationalCapability,
    context: &OperationalHoldContext,
) -> std::result::Result<OperationalCapabilityEvaluation, OperationalHoldError> {
    lock_operational_hold_shared_postgres_tx(tx)?;
    let scope_pairs = context.exact_scope_pairs();
    let scope_kinds = scope_pairs
        .iter()
        .map(|(scope_kind, _)| scope_kind.clone())
        .collect::<Vec<_>>();
    let scope_ids = scope_pairs
        .into_iter()
        .map(|(_, scope_id)| scope_id)
        .collect::<Vec<_>>();
    let rows = tx
        .query(
            "SELECT head.capability, head.scope_kind, head.scope_id, head.scope_ref,
                    head.head_revision, head.current_event_id, head.current_event_ref,
                    head.state, head.updated_by, head.updated_at_ms,
                    event.event_id, event.event_ref, event.event_sha256,
                    event.canonical_event_base64url, event.capability, event.scope_kind,
                    event.scope_id, event.revision_no, event.previous_revision_no,
                    event.predecessor_event_id, event.transition, event.reason_code,
                    event.reason_ref, event.recorded_by, event.recorded_at_ms
               FROM jobs_operational_hold_heads head
               LEFT JOIN jobs_operational_hold_events event
                 ON event.event_id = head.current_event_id
              WHERE head.capability IN ('all', $1)
                AND EXISTS (
                  SELECT 1
                    FROM unnest($2::TEXT[], $3::TEXT[]) target(scope_kind, scope_id)
                   WHERE target.scope_kind = head.scope_kind
                     AND target.scope_id = head.scope_id
                )
              ORDER BY CASE WHEN head.capability = 'all' THEN 0 ELSE 1 END,
                       head.scope_kind, head.scope_id",
            &[&capability.as_str(), &scope_kinds, &scope_ids],
        )
        .map_err(OperationalHoldError::from)?;
    for row in rows {
        let event = validated_operational_hold_head_event(
            operational_hold_canonical_head_from_postgres_row(row),
        )?;
        if event.transition == OperationalHoldTransition::Held
            && context.matches(event.scope_kind, &event.scope_id)
        {
            return Ok(OperationalCapabilityEvaluation::Held(
                operational_hold_block(event),
            ));
        }
    }
    Ok(OperationalCapabilityEvaluation::Allowed)
}

pub(crate) fn lock_operational_hold_shared_postgres_tx(
    tx: &mut postgres::Transaction<'_>,
) -> std::result::Result<(), OperationalHoldError> {
    tx.query_one(POSTGRES_OPERATIONAL_HOLD_SHARED_LOCK_SQL, &[])
        .map_err(OperationalHoldError::from)?;
    Ok(())
}

pub(crate) fn require_operational_capability_sqlite_tx(
    tx: &rusqlite::Transaction<'_>,
    capability: OperationalCapability,
    context: &OperationalHoldContext,
) -> std::result::Result<(), OperationalHoldError> {
    match evaluate_operational_capability_sqlite_tx(tx, capability, context)? {
        OperationalCapabilityEvaluation::Allowed => Ok(()),
        OperationalCapabilityEvaluation::Held(block) => Err(OperationalHoldError::Held(block)),
    }
}

pub(crate) fn require_operational_capability_postgres_tx(
    tx: &mut postgres::Transaction<'_>,
    capability: OperationalCapability,
    context: &OperationalHoldContext,
) -> std::result::Result<(), OperationalHoldError> {
    match evaluate_operational_capability_postgres_tx(tx, capability, context)? {
        OperationalCapabilityEvaluation::Allowed => Ok(()),
        OperationalCapabilityEvaluation::Held(block) => Err(OperationalHoldError::Held(block)),
    }
}

fn operational_verified_employer_domains(
    posting: &JobPosting,
) -> std::result::Result<Vec<String>, OperationalHoldError> {
    let Some(raw_domain) = posting
        .discovery_evidence
        .canonical_employer_domain
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(Vec::new());
    };
    let candidate = raw_domain.trim_end_matches('.');
    let url = reqwest::Url::parse(&format!("https://{candidate}/")).map_err(|_| {
        OperationalHoldError::Storage(anyhow::anyhow!("invalid Jobs canonical employer domain"))
    })?;
    if !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
        || url.path() != "/"
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(OperationalHoldError::Storage(anyhow::anyhow!(
            "invalid Jobs canonical employer domain"
        )));
    }
    let ascii_domain = url
        .host_str()
        .map(str::to_ascii_lowercase)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            OperationalHoldError::Storage(anyhow::anyhow!("invalid Jobs canonical employer domain"))
        })?;
    let unicode_domain = unicode_lowercase_nfc(candidate);
    let mut domains = vec![unicode_domain.clone()];
    if ascii_domain != unicode_domain {
        domains.push(ascii_domain);
    }
    Ok(domains)
}

fn operational_known_ats_provider(value: &str) -> Option<&'static str> {
    match value.trim().to_ascii_lowercase().as_str() {
        "ashby" => Some("ashby"),
        "greenhouse" => Some("greenhouse"),
        "lever" => Some("lever"),
        "smartrecruiters" => Some("smartrecruiters"),
        "workday" => Some("workday"),
        _ => None,
    }
}

fn operational_posting_ats_provider(posting: &JobPosting) -> Option<&'static str> {
    crate::jobs_ats_target::parse_provider_application_target(
        &posting.canonical_url,
        crate::jobs_ats_target::ProviderApplicationTargetPurpose::Submit,
    )
    .map(|target| target.provider)
    .or_else(|| operational_known_ats_provider(&posting.source))
}

fn operational_region(value: &str) -> Option<&str> {
    let value = value.trim();
    (!value.is_empty()
        && !matches!(
            value.to_ascii_lowercase().as_str(),
            "unknown" | "n/a" | "na" | "not specified" | "unspecified"
        ))
    .then_some(value)
}

struct OperationalApplicationContextValues<'a> {
    account_id: &'a str,
    posting: &'a JobPosting,
    application_json: Option<&'a str>,
    runner_kind: Option<&'a str>,
    model_provider: Option<&'a str>,
    model: Option<&'a str>,
}

fn add_application_context_values(
    context: &mut OperationalHoldContext,
    values: OperationalApplicationContextValues<'_>,
) -> std::result::Result<(), OperationalHoldError> {
    context.insert_scope(OperationalHoldScopeKind::Account, values.account_id)?;
    if !values.posting.track_id.trim().is_empty() {
        context.insert_scope(
            OperationalHoldScopeKind::CareerTrack,
            &values.posting.track_id,
        )?;
    }
    for domain in operational_verified_employer_domains(values.posting)? {
        context.insert_scope(OperationalHoldScopeKind::EmployerDomain, &domain)?;
    }
    if let Some(region) = operational_region(&values.posting.location) {
        context.insert_scope(OperationalHoldScopeKind::Region, region)?;
    }
    if let crate::jobs_taxonomy::GeographyClassification::Known { normalized, .. } =
        crate::jobs_taxonomy::normalize_geography(&values.posting.location)
    {
        for canonical_region in normalized.canonical_ids() {
            context.insert_scope(OperationalHoldScopeKind::Region, &canonical_region)?;
        }
    }
    if let crate::jobs_taxonomy::WorkplaceClassification::Known { kind, .. } =
        crate::jobs_taxonomy::classify_posting_workplace(
            &values.posting.workplace,
            &values.posting.location,
        )
    {
        let workplace_scope = match kind {
            crate::jobs_taxonomy::WorkplaceKind::Remote => "workplace:remote",
            crate::jobs_taxonomy::WorkplaceKind::Hybrid => "workplace:hybrid",
            crate::jobs_taxonomy::WorkplaceKind::Onsite => "workplace:onsite",
        };
        context.insert_scope(OperationalHoldScopeKind::Region, workplace_scope)?;
    }
    let derived_ats_provider = operational_posting_ats_provider(values.posting);
    if let Some(application_json) = values.application_json {
        let application: JobApplication = parse_json(
            application_json.to_string(),
            "Jobs operational hold application context",
        )
        .map_err(OperationalHoldError::Storage)?;
        if let Some(certification) = application
            .receipt
            .pointer("/approved_execution/admission/ats_certification")
        {
            let certification = certification.as_object().ok_or_else(|| {
                OperationalHoldError::Storage(anyhow::anyhow!(
                    "invalid frozen Jobs ATS certification context"
                ))
            })?;
            let provider = certification
                .get("provider")
                .and_then(Value::as_str)
                .and_then(operational_known_ats_provider)
                .ok_or_else(|| {
                    OperationalHoldError::Storage(anyhow::anyhow!(
                        "invalid frozen Jobs ATS provider context"
                    ))
                })?;
            let adapter = certification
                .get("adapter_version")
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| {
                    OperationalHoldError::Storage(anyhow::anyhow!(
                        "invalid frozen Jobs ATS adapter context"
                    ))
                })?;
            if derived_ats_provider.is_some_and(|derived| derived != provider) {
                return Err(OperationalHoldError::Storage(anyhow::anyhow!(
                    "frozen Jobs ATS provider context mismatch"
                )));
            }
            context.insert_scope(OperationalHoldScopeKind::AtsProvider, provider)?;
            context.insert_scope(OperationalHoldScopeKind::AtsAdapter, adapter)?;
        } else if let Some(provider) = derived_ats_provider {
            context.insert_scope(OperationalHoldScopeKind::AtsProvider, provider)?;
        }
    } else if let Some(provider) = derived_ats_provider {
        context.insert_scope(OperationalHoldScopeKind::AtsProvider, provider)?;
    }
    if let Some(runner_kind) = values.runner_kind {
        context.insert_scope(OperationalHoldScopeKind::RunnerKind, runner_kind)?;
    }
    if let Some(model_provider) = values.model_provider {
        context.insert_scope(OperationalHoldScopeKind::ModelProvider, model_provider)?;
    }
    if let Some(model) = values.model {
        context.insert_scope(OperationalHoldScopeKind::Model, model)?;
    }
    Ok(())
}

fn operational_context_posting(
    posting_json: String,
    canonical_url: Option<String>,
    company: String,
    location: Option<String>,
    source: String,
) -> std::result::Result<JobPosting, OperationalHoldError> {
    let posting: JobPosting = parse_json(posting_json, "Jobs operational hold posting context")
        .map_err(OperationalHoldError::Storage)?;
    if posting.canonical_url != canonical_url.unwrap_or_default()
        || posting.company != company
        || posting.location != location.unwrap_or_default()
        || posting.source != source
    {
        return Err(OperationalHoldError::Storage(anyhow::anyhow!(
            "Jobs operational hold posting projection mismatch"
        )));
    }
    Ok(posting)
}

fn require_curated_discovery_membership(
    posting: &JobPosting,
    has_managed_membership: bool,
) -> std::result::Result<(), OperationalHoldError> {
    if is_curated_job_source(&posting.source) && !has_managed_membership {
        return Err(OperationalHoldError::Storage(anyhow::anyhow!(
            "curated Jobs posting has no managed discovery authority"
        )));
    }
    Ok(())
}

pub(crate) fn operational_hold_context_for_application_sqlite_tx(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    application_id: &str,
    runner_kind: Option<&str>,
    model_provider: Option<&str>,
    model: Option<&str>,
) -> std::result::Result<OperationalHoldContext, OperationalHoldError> {
    let row = tx
        .query_row(
            "SELECT posting.posting_json, posting.canonical_url, posting.company,
                    posting.location, posting.source, application.application_json, posting.id
               FROM jobs_applications application
               JOIN jobs_postings posting
                 ON posting.id = application.job_id
                AND posting.account_id = application.account_id
              WHERE application.account_id = ?1 AND application.id = ?2",
            params![account_id, application_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                ))
            },
        )
        .optional()
        .map_err(OperationalHoldError::from)?
        .ok_or(OperationalHoldError::NotFound)?;
    let posting = operational_context_posting(row.0, row.1, row.2, row.3, row.4)?;
    let mut context = OperationalHoldContext::new();
    add_application_context_values(
        &mut context,
        OperationalApplicationContextValues {
            account_id,
            posting: &posting,
            application_json: Some(&row.5),
            runner_kind,
            model_provider,
            model,
        },
    )?;
    let mut statement = tx
        .prepare(
            "SELECT membership.source_id, source.provider, source.source_key, source.track_id
               FROM jobs_discovery_memberships membership
               JOIN jobs_discovery_sources source
                 ON source.id = membership.source_id
                AND source.account_id = membership.account_id
              WHERE membership.account_id = ?1 AND membership.job_id = ?2",
        )
        .map_err(OperationalHoldError::from)?;
    let rows = statement
        .query_map(params![account_id, row.6], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })
        .map_err(OperationalHoldError::from)?;
    let mut has_managed_curated_membership = false;
    for source in rows {
        let (source_id, provider, source_key, track_id) =
            source.map_err(OperationalHoldError::from)?;
        has_managed_curated_membership |= provider == CURATED_DISCOVERY_PROVIDER
            && source_key == CURATED_DISCOVERY_SOURCE_KEY
            && track_id.is_empty();
        context.insert_scope(OperationalHoldScopeKind::DiscoverySource, &source_id)?;
        if let Some(provider) = operational_known_ats_provider(&provider) {
            context.insert_scope(OperationalHoldScopeKind::AtsProvider, provider)?;
        }
    }
    require_curated_discovery_membership(&posting, has_managed_curated_membership)?;
    Ok(context)
}

pub(crate) fn operational_hold_context_for_application_postgres_tx(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    application_id: &str,
    runner_kind: Option<&str>,
    model_provider: Option<&str>,
    model: Option<&str>,
) -> std::result::Result<OperationalHoldContext, OperationalHoldError> {
    lock_discovery_account_shared_postgres(tx, account_id)
        .map_err(OperationalHoldError::Storage)?;
    let initial_job_id = tx
        .query_opt(
            "SELECT job_id FROM jobs_applications
              WHERE account_id = $1 AND id = $2",
            &[&account_id, &application_id],
        )
        .map_err(OperationalHoldError::from)?
        .map(|row| row.get::<_, String>(0))
        .ok_or(OperationalHoldError::NotFound)?;
    // Every production posting/source/membership writer takes the exclusive form of the account
    // fence above. SHARE row locks are defense in depth and preserve the established
    // posting-before-application order used by packet finalization.
    let posting_row = tx
        .query_opt(
            "SELECT posting_json, canonical_url, company, location, source
               FROM jobs_postings
              WHERE account_id = $1 AND id = $2
              FOR SHARE",
            &[&account_id, &initial_job_id],
        )
        .map_err(OperationalHoldError::from)?
        .ok_or(OperationalHoldError::NotFound)?;
    let application_row = tx
        .query_opt(
            "SELECT application_json, job_id FROM jobs_applications
              WHERE account_id = $1 AND id = $2
              FOR SHARE",
            &[&account_id, &application_id],
        )
        .map_err(OperationalHoldError::from)?
        .ok_or(OperationalHoldError::NotFound)?;
    let locked_job_id = application_row.get::<_, String>(1);
    if locked_job_id != initial_job_id {
        return Err(OperationalHoldError::Storage(anyhow::anyhow!(
            "Jobs operational hold application binding changed while being fenced"
        )));
    }
    let posting = operational_context_posting(
        posting_row.get::<_, String>(0),
        posting_row.get::<_, Option<String>>(1),
        posting_row.get::<_, String>(2),
        posting_row.get::<_, Option<String>>(3),
        posting_row.get::<_, String>(4),
    )?;
    let application_json = application_row.get::<_, String>(0);
    let mut context = OperationalHoldContext::new();
    add_application_context_values(
        &mut context,
        OperationalApplicationContextValues {
            account_id,
            posting: &posting,
            application_json: Some(&application_json),
            runner_kind,
            model_provider,
            model,
        },
    )?;
    let mut has_managed_curated_membership = false;
    for source_row in tx
        .query(
            "SELECT membership.source_id, source.provider, source.source_key, source.track_id
               FROM jobs_discovery_memberships membership
              JOIN jobs_discovery_sources source
                 ON source.id = membership.source_id
                AND source.account_id = membership.account_id
              WHERE membership.account_id = $1 AND membership.job_id = $2
              ORDER BY membership.source_id, membership.external_id
              FOR SHARE OF membership, source",
            &[&account_id, &initial_job_id],
        )
        .map_err(OperationalHoldError::from)?
    {
        has_managed_curated_membership |= source_row.get::<_, String>(1)
            == CURATED_DISCOVERY_PROVIDER
            && source_row.get::<_, String>(2) == CURATED_DISCOVERY_SOURCE_KEY
            && source_row.get::<_, String>(3).is_empty();
        context.insert_scope(
            OperationalHoldScopeKind::DiscoverySource,
            &source_row.get::<_, String>(0),
        )?;
        if let Some(provider) = operational_known_ats_provider(&source_row.get::<_, String>(1)) {
            context.insert_scope(OperationalHoldScopeKind::AtsProvider, provider)?;
        }
    }
    require_curated_discovery_membership(&posting, has_managed_curated_membership)?;
    Ok(context)
}

pub(crate) fn operational_hold_context_for_job_sqlite_tx(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    job_id: &str,
    model_provider: Option<&str>,
    model: Option<&str>,
) -> std::result::Result<OperationalHoldContext, OperationalHoldError> {
    let row = tx
        .query_row(
            "SELECT posting_json, canonical_url, company, location, source FROM jobs_postings
              WHERE account_id = ?1 AND id = ?2",
            params![account_id, job_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, String>(4)?,
                ))
            },
        )
        .optional()
        .map_err(OperationalHoldError::from)?
        .ok_or(OperationalHoldError::NotFound)?;
    let posting = operational_context_posting(row.0, row.1, row.2, row.3, row.4)?;
    let mut context = OperationalHoldContext::new();
    add_application_context_values(
        &mut context,
        OperationalApplicationContextValues {
            account_id,
            posting: &posting,
            application_json: None,
            runner_kind: None,
            model_provider,
            model,
        },
    )?;
    let mut has_managed_curated_membership = false;
    for source_row in tx
        .prepare(
            "SELECT membership.source_id, source.provider, source.source_key, source.track_id
               FROM jobs_discovery_memberships membership
               JOIN jobs_discovery_sources source
                 ON source.id = membership.source_id
                AND source.account_id = membership.account_id
              WHERE membership.account_id = ?1 AND membership.job_id = ?2",
        )
        .map_err(OperationalHoldError::from)?
        .query_map(params![account_id, job_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })
        .map_err(OperationalHoldError::from)?
    {
        let (source_id, provider, source_key, track_id) =
            source_row.map_err(OperationalHoldError::from)?;
        has_managed_curated_membership |= provider == CURATED_DISCOVERY_PROVIDER
            && source_key == CURATED_DISCOVERY_SOURCE_KEY
            && track_id.is_empty();
        context.insert_scope(OperationalHoldScopeKind::DiscoverySource, &source_id)?;
        if let Some(provider) = operational_known_ats_provider(&provider) {
            context.insert_scope(OperationalHoldScopeKind::AtsProvider, provider)?;
        }
    }
    require_curated_discovery_membership(&posting, has_managed_curated_membership)?;
    Ok(context)
}

pub(crate) fn operational_hold_context_for_job_postgres_tx(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    job_id: &str,
    model_provider: Option<&str>,
    model: Option<&str>,
) -> std::result::Result<OperationalHoldContext, OperationalHoldError> {
    lock_discovery_account_shared_postgres(tx, account_id)
        .map_err(OperationalHoldError::Storage)?;
    let row = tx
        .query_opt(
            "SELECT posting_json, canonical_url, company, location, source
               FROM jobs_postings AS posting
              WHERE account_id = $1 AND id = $2
              FOR SHARE OF posting",
            &[&account_id, &job_id],
        )
        .map_err(OperationalHoldError::from)?
        .ok_or(OperationalHoldError::NotFound)?;
    let posting = operational_context_posting(
        row.get::<_, String>(0),
        row.get::<_, Option<String>>(1),
        row.get::<_, String>(2),
        row.get::<_, Option<String>>(3),
        row.get::<_, String>(4),
    )?;
    let mut context = OperationalHoldContext::new();
    add_application_context_values(
        &mut context,
        OperationalApplicationContextValues {
            account_id,
            posting: &posting,
            application_json: None,
            runner_kind: None,
            model_provider,
            model,
        },
    )?;
    let mut has_managed_curated_membership = false;
    for source_row in tx
        .query(
            "SELECT membership.source_id, source.provider, source.source_key, source.track_id
               FROM jobs_discovery_memberships membership
              JOIN jobs_discovery_sources source
                 ON source.id = membership.source_id
                AND source.account_id = membership.account_id
              WHERE membership.account_id = $1 AND membership.job_id = $2
              ORDER BY membership.source_id, membership.external_id
              FOR SHARE OF membership, source",
            &[&account_id, &job_id],
        )
        .map_err(OperationalHoldError::from)?
    {
        has_managed_curated_membership |= source_row.get::<_, String>(1)
            == CURATED_DISCOVERY_PROVIDER
            && source_row.get::<_, String>(2) == CURATED_DISCOVERY_SOURCE_KEY
            && source_row.get::<_, String>(3).is_empty();
        context.insert_scope(
            OperationalHoldScopeKind::DiscoverySource,
            &source_row.get::<_, String>(0),
        )?;
        if let Some(provider) = operational_known_ats_provider(&source_row.get::<_, String>(1)) {
            context.insert_scope(OperationalHoldScopeKind::AtsProvider, provider)?;
        }
    }
    require_curated_discovery_membership(&posting, has_managed_curated_membership)?;
    Ok(context)
}

pub(crate) fn operational_hold_context_for_mailbox_sqlite_tx(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    connection_id: &str,
) -> std::result::Result<OperationalHoldContext, OperationalHoldError> {
    let provider = tx
        .query_row(
            "SELECT provider FROM jobs_mailbox_connections
              WHERE account_id = ?1 AND id = ?2",
            params![account_id, connection_id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(OperationalHoldError::from)?
        .ok_or(OperationalHoldError::NotFound)?;
    OperationalHoldContext::new()
        .with_scope(OperationalHoldScopeKind::Account, account_id)?
        .with_scope(OperationalHoldScopeKind::MailboxProvider, &provider)
}

pub(crate) fn operational_hold_context_for_mailbox_postgres_tx(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    connection_id: &str,
) -> std::result::Result<OperationalHoldContext, OperationalHoldError> {
    let provider = tx
        .query_opt(
            "SELECT provider FROM jobs_mailbox_connections
              WHERE account_id = $1 AND id = $2",
            &[&account_id, &connection_id],
        )
        .map_err(OperationalHoldError::from)?
        .map(|row| row.get::<_, String>(0))
        .ok_or(OperationalHoldError::NotFound)?;
    OperationalHoldContext::new()
        .with_scope(OperationalHoldScopeKind::Account, account_id)?
        .with_scope(OperationalHoldScopeKind::MailboxProvider, &provider)
}

#[cfg(test)]
mod operational_hold_tests {
    use super::*;

    fn hold_pool(migrate: bool) -> DbPool {
        let path = std::env::temp_dir().join(format!(
            "bluey-operational-holds-{}-{}.sqlite3",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let pool = crate::db::open_pool(&path).unwrap();
        if migrate {
            crate::db::run_migrations(&pool).unwrap();
        }
        pool
    }

    fn insert_test_account(pool: &DbPool, account_id: &str) {
        let email_digest = hex::encode(Sha256::digest(account_id.as_bytes()));
        pool.get()
            .unwrap()
            .execute(
                "INSERT OR IGNORE INTO accounts (
                    id, email, password_hash, trial_seconds_remaining
                 ) VALUES (?1, ?2, 'hash', 0)",
                params![account_id, format!("{}@example.test", &email_digest[..24])],
            )
            .unwrap();
    }

    fn insert_test_scope_target(
        pool: &DbPool,
        scope_kind: OperationalHoldScopeKind,
        scope_id: &str,
    ) {
        match scope_kind {
            OperationalHoldScopeKind::Account => insert_test_account(pool, scope_id),
            OperationalHoldScopeKind::CareerTrack => {
                insert_test_account(pool, "acct-scope-owner");
                pool.get()
                    .unwrap()
                    .execute(
                        "INSERT OR IGNORE INTO jobs_tracks (
                            id, account_id, track_json, active, created_at_ms, updated_at_ms
                         ) VALUES (?1, 'acct-scope-owner', '{}', 1, 1, 1)",
                        params![scope_id],
                    )
                    .unwrap();
            }
            OperationalHoldScopeKind::DiscoverySource => {
                pool.get()
                    .unwrap()
                    .execute(
                        "INSERT OR IGNORE INTO jobs_global_discovery_sources (
                            id, provider, source_key, source_json, status, health,
                            run_interval_ms, next_run_at_ms, created_at_ms, updated_at_ms
                         ) VALUES (?1, 'jobhive', ?1, '{}', 'active', 'waiting', 1, 1, 1, 1)",
                        params![scope_id],
                    )
                    .unwrap();
            }
            _ => {}
        }
    }

    fn request(
        event_id: &str,
        capability: OperationalCapability,
        scope_kind: OperationalHoldScopeKind,
        scope_id: &str,
        transition: OperationalHoldTransition,
        revision: i64,
        predecessor: Option<&str>,
    ) -> AppendOperationalHoldEventRequest {
        AppendOperationalHoldEventRequest {
            event_id: event_id.to_string(),
            capability,
            scope_kind,
            scope_id: scope_id.to_string(),
            transition,
            reason_code: if transition == OperationalHoldTransition::Released {
                OperationalHoldReasonCode::ManualRelease
            } else {
                OperationalHoldReasonCode::Incident
            },
            reason_ref: Some("INC-606".to_string()),
            expected_head_revision: revision,
            expected_current_event_id: predecessor.map(str::to_string),
        }
    }

    fn operational_posting(company: &str, source: &str, canonical_url: &str) -> JobPosting {
        JobPosting {
            id: "operational-job".to_string(),
            canonical_key: "operational-job-key".to_string(),
            source: source.to_string(),
            external_id: canonical_url.to_string(),
            company: company.to_string(),
            title: "Engineer".to_string(),
            location: "München".to_string(),
            workplace: "hybrid".to_string(),
            canonical_url: canonical_url.to_string(),
            description: String::new(),
            compensation: String::new(),
            employment_type: "full_time".to_string(),
            track_id: "track-operational".to_string(),
            match_score: 100,
            matched_reasons: Vec::new(),
            missing_requirements: Vec::new(),
            posted_at_ms: Some(1),
            last_verified_at_ms: Some(1),
            availability_status: "active".to_string(),
            status: "saved".to_string(),
            created_at_ms: 1,
            updated_at_ms: 1,
            discovery_evidence: JobDiscoveryEvidence {
                canonical_employer_domain: Some("careers.acme.example".to_string()),
                ..JobDiscoveryEvidence::default()
            },
            eligibility: None,
        }
    }

    #[test]
    fn mutation_boundaries_reject_unhashable_identifiers_scopes_and_reason_refs() {
        let base = request(
            "boundary-event",
            OperationalCapability::Discovery,
            OperationalHoldScopeKind::DiscoverySource,
            "source-boundary",
            OperationalHoldTransition::Held,
            0,
            None,
        );
        assert!(validate_append_operational_hold_request(&base, "admin-boundary").is_ok());

        let mut invalid_event = base.clone();
        invalid_event.event_id = "e".repeat(129);
        assert!(matches!(
            validate_append_operational_hold_request(&invalid_event, "admin-boundary"),
            Err(OperationalHoldError::InvalidRequest)
        ));

        let mut invalid_scope = base.clone();
        invalid_scope.scope_id = "s".repeat(257);
        assert!(matches!(
            validate_append_operational_hold_request(&invalid_scope, "admin-boundary"),
            Err(OperationalHoldError::InvalidRequest)
        ));
        invalid_scope.scope_id = "source\nprivate".to_string();
        assert!(matches!(
            validate_append_operational_hold_request(&invalid_scope, "admin-boundary"),
            Err(OperationalHoldError::InvalidRequest)
        ));

        let mut invalid_reason = base.clone();
        invalid_reason.reason_ref = Some("R".repeat(121));
        assert!(matches!(
            validate_append_operational_hold_request(&invalid_reason, "admin-boundary"),
            Err(OperationalHoldError::InvalidRequest)
        ));
        invalid_reason.reason_ref = Some("INC/private".to_string());
        assert!(matches!(
            validate_append_operational_hold_request(&invalid_reason, "admin-boundary"),
            Err(OperationalHoldError::InvalidRequest)
        ));
        invalid_reason.reason_ref = Some("-INC-606".to_string());
        assert!(matches!(
            validate_append_operational_hold_request(&invalid_reason, "admin-boundary"),
            Err(OperationalHoldError::InvalidRequest)
        ));

        let mut invalid_cas = base;
        invalid_cas.expected_current_event_id = Some("unexpected-predecessor".to_string());
        assert!(matches!(
            validate_append_operational_hold_request(&invalid_cas, "admin-boundary"),
            Err(OperationalHoldError::InvalidRequest)
        ));
        let actor_boundary = request(
            "actor-boundary",
            OperationalCapability::Discovery,
            OperationalHoldScopeKind::DiscoverySource,
            "source-boundary",
            OperationalHoldTransition::Held,
            0,
            None,
        );
        assert!(matches!(
            validate_append_operational_hold_request(&actor_boundary, &"operator".repeat(19)),
            Err(OperationalHoldError::InvalidRequest)
        ));
    }

    #[test]
    fn operational_context_rejects_unknown_or_malformed_authority_dimensions() {
        for (scope_kind, scope_id) in [
            (OperationalHoldScopeKind::AtsProvider, "unknown-ats"),
            (OperationalHoldScopeKind::RunnerKind, "browser"),
            (OperationalHoldScopeKind::MailboxProvider, "imap"),
            (OperationalHoldScopeKind::AtsAdapter, "adapter with spaces"),
            (
                OperationalHoldScopeKind::ModelProvider,
                "provider with spaces",
            ),
            (OperationalHoldScopeKind::Model, "model with spaces"),
            (OperationalHoldScopeKind::EmployerDomain, "not-a-domain"),
            (OperationalHoldScopeKind::Region, "person@example.test"),
        ] {
            assert!(matches!(
                OperationalHoldContext::new().with_scope(scope_kind, scope_id),
                Err(OperationalHoldError::InvalidRequest)
            ));
        }

        assert!(matches!(
            OperationalHoldContext::new()
                .with_scope(OperationalHoldScopeKind::Region, &"r".repeat(129)),
            Err(OperationalHoldError::InvalidRequest)
        ));
        assert!(OperationalHoldContext::new()
            .with_scope(OperationalHoldScopeKind::AtsProvider, "greenhouse")
            .is_ok());
    }

    #[test]
    fn operational_hold_chain_is_replay_safe_and_requires_exact_cas() {
        let pool = hold_pool(true);
        insert_test_account(&pool, "acct-1");
        let opened = request(
            "event-1",
            OperationalCapability::RunnerClaim,
            OperationalHoldScopeKind::Account,
            "acct-1",
            OperationalHoldTransition::Held,
            0,
            None,
        );
        let first = append_operational_hold_event(&pool, &opened, "admin-1").unwrap();
        assert_eq!(first.state.head_revision, 1);
        assert!(!first.replayed);
        let replay = append_operational_hold_event(&pool, &opened, "admin-1").unwrap();
        assert_eq!(replay.state, first.state);
        assert!(replay.replayed);

        let mut changed_cas = opened.clone();
        changed_cas.expected_head_revision = 1;
        changed_cas.expected_current_event_id = Some("different-predecessor".to_string());
        assert!(matches!(
            append_operational_hold_event(&pool, &changed_cas, "admin-1"),
            Err(OperationalHoldError::IdentityConflict)
        ));

        let changed_actor = append_operational_hold_event(&pool, &opened, "admin-2");
        assert!(matches!(
            changed_actor,
            Err(OperationalHoldError::IdentityConflict)
        ));
        let stale = request(
            "event-stale",
            OperationalCapability::RunnerClaim,
            OperationalHoldScopeKind::Account,
            "acct-1",
            OperationalHoldTransition::Released,
            0,
            None,
        );
        assert!(matches!(
            append_operational_hold_event(&pool, &stale, "admin-1"),
            Err(OperationalHoldError::Conflict)
        ));

        let escalated = request(
            "event-2",
            OperationalCapability::RunnerClaim,
            OperationalHoldScopeKind::Account,
            "acct-1",
            OperationalHoldTransition::Held,
            1,
            Some("event-1"),
        );
        assert_eq!(
            append_operational_hold_event(&pool, &escalated, "admin-1")
                .unwrap()
                .state
                .head_revision,
            2
        );
        let released = request(
            "event-3",
            OperationalCapability::RunnerClaim,
            OperationalHoldScopeKind::Account,
            "acct-1",
            OperationalHoldTransition::Released,
            2,
            Some("event-2"),
        );
        assert_eq!(
            append_operational_hold_event(&pool, &released, "admin-1")
                .unwrap()
                .state
                .state,
            OperationalHoldTransition::Released
        );
        let redundant_release = request(
            "event-4",
            OperationalCapability::RunnerClaim,
            OperationalHoldScopeKind::Account,
            "acct-1",
            OperationalHoldTransition::Released,
            3,
            Some("event-3"),
        );
        assert!(matches!(
            append_operational_hold_event(&pool, &redundant_release, "admin-1"),
            Err(OperationalHoldError::Conflict)
        ));

        let reopened = request(
            "event-5",
            OperationalCapability::RunnerClaim,
            OperationalHoldScopeKind::Account,
            "acct-1",
            OperationalHoldTransition::Held,
            3,
            Some("event-3"),
        );
        let reopened = append_operational_hold_event(&pool, &reopened, "admin-1").unwrap();
        assert_eq!(reopened.state.head_revision, 4);
        assert_eq!(reopened.state.state, OperationalHoldTransition::Held);

        let first_release = request(
            "fresh-scope-release",
            OperationalCapability::RunnerClaim,
            OperationalHoldScopeKind::Account,
            "acct-fresh-release",
            OperationalHoldTransition::Released,
            0,
            None,
        );
        assert!(matches!(
            append_operational_hold_event(&pool, &first_release, "admin-1"),
            Err(OperationalHoldError::Conflict)
        ));
    }

    #[test]
    fn mismatched_persisted_event_reference_blocks_replay_and_release() {
        let pool = hold_pool(true);
        insert_test_account(&pool, "acct-ref-corruption");
        let held = request(
            "event-ref-corruption",
            OperationalCapability::RunnerClaim,
            OperationalHoldScopeKind::Account,
            "acct-ref-corruption",
            OperationalHoldTransition::Held,
            0,
            None,
        );
        append_operational_hold_event(&pool, &held, "admin-1").unwrap();

        let conn = pool.get().unwrap();
        conn.execute_batch(
            "PRAGMA foreign_keys = OFF;
             DROP TRIGGER trg_jobs_operational_hold_events_no_update;",
        )
        .unwrap();
        conn.execute(
            "UPDATE jobs_operational_hold_events SET event_ref = ?1 WHERE event_id = ?2",
            params![format!("event-{}", "b".repeat(64)), held.event_id],
        )
        .unwrap();
        drop(conn);

        assert!(matches!(
            append_operational_hold_event(&pool, &held, "admin-1"),
            Err(OperationalHoldError::IdentityConflict)
        ));

        let release = request(
            "event-ref-corruption-release",
            OperationalCapability::RunnerClaim,
            OperationalHoldScopeKind::Account,
            "acct-ref-corruption",
            OperationalHoldTransition::Released,
            1,
            Some("event-ref-corruption"),
        );
        assert!(matches!(
            append_operational_hold_event(&pool, &release, "admin-1"),
            Err(OperationalHoldError::Storage(_))
        ));
        let conn = pool.get().unwrap();
        let head: (i64, String, String) = conn
            .query_row(
                "SELECT head_revision, current_event_id, state
                   FROM jobs_operational_hold_heads
                  WHERE capability = 'runner_claim' AND scope_kind = 'account'
                    AND scope_id = 'acct-ref-corruption'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(
            head,
            (1, "event-ref-corruption".to_string(), "held".to_string())
        );
        let release_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM jobs_operational_hold_events
                  WHERE event_id = 'event-ref-corruption-release'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(release_count, 0);
    }

    fn assert_canonical_corruption_blocks_every_release_path(
        pool: &DbPool,
        held: &AppendOperationalHoldEventRequest,
        public_state: &OperationalHoldPublicState,
        raw_release_event_id: &str,
        by_ref_release_event_id: &str,
    ) {
        let raw_release = request(
            raw_release_event_id,
            held.capability,
            held.scope_kind,
            &held.scope_id,
            OperationalHoldTransition::Released,
            1,
            Some(&held.event_id),
        );
        assert!(matches!(
            append_operational_hold_event(pool, &raw_release, "admin-1"),
            Err(OperationalHoldError::Storage(_))
        ));

        let by_ref_release = AppendOperationalHoldEventByRefRequest {
            event_id: by_ref_release_event_id.to_string(),
            capability: held.capability,
            scope_kind: held.scope_kind,
            scope_ref: public_state.scope_ref.clone(),
            transition: OperationalHoldTransition::Released,
            reason_code: OperationalHoldReasonCode::ManualRelease,
            reason_ref: Some("INC-606".to_string()),
            expected_head_revision: 1,
            expected_current_event_ref: public_state.current_event_ref.clone(),
        };
        assert!(matches!(
            append_operational_hold_event_by_ref(pool, &by_ref_release, "admin-1"),
            Err(OperationalHoldError::Storage(_))
        ));
        assert!(matches!(
            list_operational_hold_states(pool, false, 10, None),
            Err(OperationalHoldError::Storage(_))
        ));

        let conn = pool.get().unwrap();
        let head: (i64, String, String) = conn
            .query_row(
                "SELECT head_revision, current_event_id, state
                   FROM jobs_operational_hold_heads
                  WHERE capability = ?1 AND scope_kind = ?2 AND scope_id = ?3",
                params![
                    held.capability.as_str(),
                    held.scope_kind.as_str(),
                    held.scope_id
                ],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(head, (1, held.event_id.clone(), "held".to_string()));
        let release_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM jobs_operational_hold_events
                  WHERE event_id IN (?1, ?2)",
                params![raw_release_event_id, by_ref_release_event_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(release_count, 0);
    }

    #[test]
    fn corrupted_event_sha256_blocks_raw_and_reference_release_and_listing() {
        let pool = hold_pool(true);
        insert_test_account(&pool, "acct-sha-corruption");
        let held = request(
            "event-sha-corruption",
            OperationalCapability::RunnerClaim,
            OperationalHoldScopeKind::Account,
            "acct-sha-corruption",
            OperationalHoldTransition::Held,
            0,
            None,
        );
        let opened = append_operational_hold_event(&pool, &held, "admin-1").unwrap();

        let conn = pool.get().unwrap();
        conn.execute_batch("DROP TRIGGER trg_jobs_operational_hold_events_no_update;")
            .unwrap();
        conn.execute(
            "UPDATE jobs_operational_hold_events SET event_sha256 = ?1 WHERE event_id = ?2",
            params!["f".repeat(64), held.event_id],
        )
        .unwrap();
        drop(conn);

        assert_canonical_corruption_blocks_every_release_path(
            &pool,
            &held,
            &opened.state,
            "event-sha-corruption-raw-release",
            "event-sha-corruption-ref-release",
        );
    }

    #[test]
    fn canonical_bytes_conflicting_with_relational_projection_block_every_release_path() {
        let pool = hold_pool(true);
        insert_test_account(&pool, "acct-projection-corruption");
        let held = request(
            "event-projection-corruption",
            OperationalCapability::RunnerClaim,
            OperationalHoldScopeKind::Account,
            "acct-projection-corruption",
            OperationalHoldTransition::Held,
            0,
            None,
        );
        let opened = append_operational_hold_event(&pool, &held, "admin-1").unwrap();

        let conn = pool.get().unwrap();
        let encoded: String = conn
            .query_row(
                "SELECT canonical_event_base64url FROM jobs_operational_hold_events
                  WHERE event_id = ?1",
                params![held.event_id],
                |row| row.get(0),
            )
            .unwrap();
        let mut canonical = decode_operational_hold_event(&encoded).unwrap();
        canonical.reason_code = OperationalHoldReasonCode::Maintenance;
        validate_canonical_operational_hold_event(&canonical).unwrap();
        let (event_sha256, canonical_event_base64url) =
            operational_hold_event_identity(&canonical).unwrap();
        assert_eq!(
            decode_operational_hold_event(&canonical_event_base64url).unwrap(),
            canonical
        );
        conn.execute_batch("DROP TRIGGER trg_jobs_operational_hold_events_no_update;")
            .unwrap();
        conn.execute(
            "UPDATE jobs_operational_hold_events
                SET event_sha256 = ?1, canonical_event_base64url = ?2
              WHERE event_id = ?3",
            params![event_sha256, canonical_event_base64url, held.event_id],
        )
        .unwrap();
        drop(conn);

        assert_canonical_corruption_blocks_every_release_path(
            &pool,
            &held,
            &opened.state,
            "event-projection-corruption-raw-release",
            "event-projection-corruption-ref-release",
        );
    }

    #[test]
    fn malformed_canonical_event_blocks_exact_replay_and_every_release_path() {
        let pool = hold_pool(true);
        insert_test_account(&pool, "acct-canonical-corruption");
        let held = request(
            "event-canonical-corruption",
            OperationalCapability::RunnerClaim,
            OperationalHoldScopeKind::Account,
            "acct-canonical-corruption",
            OperationalHoldTransition::Held,
            0,
            None,
        );
        let opened = append_operational_hold_event(&pool, &held, "admin-1").unwrap();

        let conn = pool.get().unwrap();
        let encoded: String = conn
            .query_row(
                "SELECT canonical_event_base64url FROM jobs_operational_hold_events
                  WHERE event_id = ?1",
                params![held.event_id],
                |row| row.get(0),
            )
            .unwrap();
        let mut canonical = decode_operational_hold_event(&encoded).unwrap();
        canonical.recorded_at_ms = OPERATIONAL_HOLD_MAX_REVISION + 1;
        let (event_sha256, canonical_event_base64url) =
            operational_hold_event_identity(&canonical).unwrap();
        conn.execute_batch("DROP TRIGGER trg_jobs_operational_hold_events_no_update;")
            .unwrap();
        conn.execute(
            "UPDATE jobs_operational_hold_events
                SET event_sha256 = ?1, canonical_event_base64url = ?2
              WHERE event_id = ?3",
            params![event_sha256, canonical_event_base64url, held.event_id],
        )
        .unwrap();
        drop(conn);

        assert!(matches!(
            append_operational_hold_event(&pool, &held, "admin-1"),
            Err(OperationalHoldError::Storage(_))
        ));
        assert_canonical_corruption_blocks_every_release_path(
            &pool,
            &held,
            &opened.state,
            "event-canonical-corruption-raw-release",
            "event-canonical-corruption-ref-release",
        );
    }

    #[test]
    fn released_relational_head_with_held_canonical_event_fails_every_read_path() {
        let pool = hold_pool(true);
        insert_test_account(&pool, "acct-head-state-corruption");
        let held = request(
            "event-head-state-corruption",
            OperationalCapability::Generation,
            OperationalHoldScopeKind::Account,
            "acct-head-state-corruption",
            OperationalHoldTransition::Held,
            0,
            None,
        );
        let opened = append_operational_hold_event(&pool, &held, "admin-1").unwrap();

        let conn = pool.get().unwrap();
        conn.execute_batch("DROP TRIGGER trg_jobs_operational_hold_heads_monotonic;")
            .unwrap();
        conn.execute(
            "UPDATE jobs_operational_hold_heads SET state = 'released'
              WHERE capability = 'generation' AND scope_kind = 'account'
                AND scope_id = 'acct-head-state-corruption'",
            [],
        )
        .unwrap();
        drop(conn);

        let context = OperationalHoldContext::new()
            .with_scope(
                OperationalHoldScopeKind::Account,
                "acct-head-state-corruption",
            )
            .unwrap();
        assert!(matches!(
            require_operational_capability(&pool, OperationalCapability::Generation, &context,),
            Err(OperationalHoldError::Storage(_))
        ));
        assert!(crate::db::metrics::jobs_readiness_snapshot(&pool).is_err());
        assert!(matches!(
            list_operational_hold_states(&pool, true, 10, None),
            Err(OperationalHoldError::Storage(_))
        ));

        let raw_release = request(
            "event-head-state-corruption-raw-release",
            OperationalCapability::Generation,
            OperationalHoldScopeKind::Account,
            "acct-head-state-corruption",
            OperationalHoldTransition::Released,
            1,
            Some("event-head-state-corruption"),
        );
        assert!(matches!(
            append_operational_hold_event(&pool, &raw_release, "admin-1"),
            Err(OperationalHoldError::Storage(_))
        ));
        let by_ref_release = AppendOperationalHoldEventByRefRequest {
            event_id: "event-head-state-corruption-ref-release".to_string(),
            capability: OperationalCapability::Generation,
            scope_kind: OperationalHoldScopeKind::Account,
            scope_ref: opened.state.scope_ref,
            transition: OperationalHoldTransition::Released,
            reason_code: OperationalHoldReasonCode::ManualRelease,
            reason_ref: Some("INC-606".to_string()),
            expected_head_revision: 1,
            expected_current_event_ref: opened.state.current_event_ref,
        };
        assert!(matches!(
            append_operational_hold_event_by_ref(&pool, &by_ref_release, "admin-1"),
            Err(OperationalHoldError::Storage(_))
        ));

        let conn = pool.get().unwrap();
        let head: (i64, String, String) = conn
            .query_row(
                "SELECT head_revision, current_event_id, state
                   FROM jobs_operational_hold_heads
                  WHERE capability = 'generation' AND scope_kind = 'account'
                    AND scope_id = 'acct-head-state-corruption'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(
            head,
            (
                1,
                "event-head-state-corruption".to_string(),
                "released".to_string(),
            )
        );
        let release_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM jobs_operational_hold_events
                  WHERE event_id IN (
                    'event-head-state-corruption-raw-release',
                    'event-head-state-corruption-ref-release'
                  )",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(release_count, 0);
    }

    #[test]
    fn public_reference_failure_rolls_back_the_hold_transition() {
        let pool = hold_pool(true);
        insert_test_account(&pool, "acct-atomic");
        let held = request(
            "atomic-held",
            OperationalCapability::Generation,
            OperationalHoldScopeKind::Account,
            "acct-atomic",
            OperationalHoldTransition::Held,
            0,
            None,
        );
        append_operational_hold_event(&pool, &held, "admin-1").unwrap();

        let released = request(
            "atomic-released",
            OperationalCapability::Generation,
            OperationalHoldScopeKind::Account,
            "acct-atomic",
            OperationalHoldTransition::Released,
            1,
            Some("atomic-held"),
        );
        fn fail_public_projection(
            _: &CanonicalOperationalHoldEvent,
            _: String,
        ) -> std::result::Result<OperationalHoldPublicState, OperationalHoldError> {
            Err(OperationalHoldError::Storage(anyhow::anyhow!(
                "injected public projection failure"
            )))
        }
        assert!(matches!(
            append_operational_hold_event_sqlite(
                &pool,
                &released,
                "acct-atomic",
                released.reason_ref.clone(),
                "admin-1",
                fail_public_projection,
            ),
            Err(OperationalHoldError::Storage(_))
        ));

        let conn = pool.get().unwrap();
        let head = conn
            .query_row(
                "SELECT head_revision, current_event_id, state
                   FROM jobs_operational_hold_heads
                  WHERE capability = 'generation' AND scope_kind = 'account'
                    AND scope_id = 'acct-atomic'",
                [],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(head, (1, "atomic-held".to_string(), "held".to_string()));
        let event_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM jobs_operational_hold_events
                  WHERE capability = 'generation' AND scope_kind = 'account'
                    AND scope_id = 'acct-atomic'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(event_count, 1);
    }

    #[test]
    fn overlapping_and_all_capability_holds_fail_closed_until_each_is_released() {
        let pool = hold_pool(true);
        insert_test_account(&pool, "acct-1");
        let global = request(
            "global-1",
            OperationalCapability::All,
            OperationalHoldScopeKind::Global,
            "*",
            OperationalHoldTransition::Held,
            0,
            None,
        );
        append_operational_hold_event(&pool, &global, "admin-1").unwrap();
        let account = request(
            "account-1",
            OperationalCapability::Generation,
            OperationalHoldScopeKind::Account,
            "acct-1",
            OperationalHoldTransition::Held,
            0,
            None,
        );
        append_operational_hold_event(&pool, &account, "admin-1").unwrap();
        let context = OperationalHoldContext::new()
            .with_scope(OperationalHoldScopeKind::Account, "acct-1")
            .unwrap();
        assert!(matches!(
            require_operational_capability(&pool, OperationalCapability::Generation, &context),
            Err(OperationalHoldError::Held(_))
        ));
        let release_global = request(
            "global-2",
            OperationalCapability::All,
            OperationalHoldScopeKind::Global,
            "*",
            OperationalHoldTransition::Released,
            1,
            Some("global-1"),
        );
        append_operational_hold_event(&pool, &release_global, "admin-1").unwrap();
        assert!(matches!(
            require_operational_capability(&pool, OperationalCapability::Generation, &context),
            Err(OperationalHoldError::Held(_))
        ));
        let release_account = request(
            "account-2",
            OperationalCapability::Generation,
            OperationalHoldScopeKind::Account,
            "acct-1",
            OperationalHoldTransition::Released,
            1,
            Some("account-1"),
        );
        append_operational_hold_event(&pool, &release_account, "admin-1").unwrap();
        assert!(
            require_operational_capability(&pool, OperationalCapability::Generation, &context)
                .is_ok()
        );
    }

    #[test]
    fn public_state_redacts_scope_reason_and_actor() {
        let pool = hold_pool(true);
        let nonexistent = request(
            "private-nonexistent",
            OperationalCapability::MailboxSync,
            OperationalHoldScopeKind::Account,
            "acct-private@example.test",
            OperationalHoldTransition::Held,
            0,
            None,
        );
        assert!(matches!(
            append_operational_hold_event(&pool, &nonexistent, "admin-private"),
            Err(OperationalHoldError::InvalidRequest)
        ));

        insert_test_account(&pool, "acct-private");
        let held = request(
            "private-1",
            OperationalCapability::MailboxSync,
            OperationalHoldScopeKind::Account,
            "acct-private",
            OperationalHoldTransition::Held,
            0,
            None,
        );
        let result = append_operational_hold_event(&pool, &held, "admin-private").unwrap();
        let encoded = serde_json::to_string(&result).unwrap();
        assert!(!encoded.contains("acct-private"));
        assert!(!encoded.contains("INC-606"));
        assert!(!encoded.contains("admin-private"));
        assert!(encoded.contains("scope-"));
        assert!(encoded.contains("event-"));
        assert_eq!(result.state.scope_ref.len(), "scope-".len() + 64);
        assert_eq!(result.state.current_event_ref.len(), "event-".len() + 64);
    }

    #[test]
    fn debug_projections_redact_private_operational_hold_identity() {
        let private_scope = "private-debug-account";
        let private_reason = "private-debug-reason";
        let block = OperationalHoldBlock {
            capability: OperationalCapability::Generation,
            scope_kind: OperationalHoldScopeKind::Account,
            scope_id: private_scope.to_string(),
            reason_code: OperationalHoldReasonCode::Incident,
            reason_ref: Some(private_reason.to_string()),
            head_revision: 1,
        };
        let state = OperationalHoldState {
            capability: block.capability,
            scope_kind: block.scope_kind,
            scope_id: private_scope.to_string(),
            head_revision: 1,
            current_event_id: "private-debug-event".to_string(),
            event_sha256: "a".repeat(64),
            state: OperationalHoldTransition::Held,
            reason_code: block.reason_code,
            reason_ref: Some(private_reason.to_string()),
            recorded_by: "private-debug-actor".to_string(),
            recorded_at_ms: 1,
        };
        let context = OperationalHoldContext::new()
            .with_scope(OperationalHoldScopeKind::Account, private_scope)
            .unwrap();
        let rendered = [
            format!("{block:?}"),
            format!("{:?}", OperationalCapabilityEvaluation::Held(block.clone())),
            format!("{:?}", OperationalHoldError::Held(block)),
            format!("{state:?}"),
            format!("{context:?}"),
        ]
        .join("\n");
        for forbidden in [
            private_scope,
            private_reason,
            "private-debug-event",
            "private-debug-actor",
        ] {
            assert!(!rendered.contains(forbidden));
        }
    }

    #[test]
    fn application_context_uses_employer_identity_and_exact_ats_authority() {
        let mut posting = operational_posting(
            "Acme Incorporated",
            "curated_feed",
            "https://boards.greenhouse.io/acme/jobs/123",
        );
        posting.location = "New York, NY".to_string();
        let mut context = OperationalHoldContext::new();
        add_application_context_values(
            &mut context,
            OperationalApplicationContextValues {
                account_id: "acct-operational",
                posting: &posting,
                application_json: None,
                runner_kind: Some("cloud"),
                model_provider: None,
                model: None,
            },
        )
        .unwrap();
        assert!(!context.matches(
            OperationalHoldScopeKind::EmployerDomain,
            "acme incorporated"
        ));
        assert!(context.matches(
            OperationalHoldScopeKind::EmployerDomain,
            "careers.acme.example"
        ));
        assert!(!context.matches(
            OperationalHoldScopeKind::EmployerDomain,
            "boards.greenhouse.io"
        ));
        assert!(context.matches(OperationalHoldScopeKind::AtsProvider, "greenhouse"));
        assert!(!context.matches(OperationalHoldScopeKind::AtsProvider, "curated_feed"));
        assert!(context.matches(OperationalHoldScopeKind::Region, "new york, ny"));
        assert!(context.matches(OperationalHoldScopeKind::Region, "country:us"));
        assert!(context.matches(OperationalHoldScopeKind::Region, "subdivision:us-ny"));
        assert!(context.matches(
            OperationalHoldScopeKind::Region,
            "metro:us-ny-new-york-metro"
        ));
        assert!(context.matches(OperationalHoldScopeKind::Region, "city:us-ny-new-york"));

        let application = JobApplication {
            id: "application-operational".to_string(),
            job_id: posting.id.clone(),
            resume_version_id: None,
            state: "approved".to_string(),
            submission_mode: "auto_submit".to_string(),
            match_score: 100,
            answers: Vec::new(),
            cover_letter: String::new(),
            receipt: serde_json::json!({
                "approved_execution": {
                    "admission": {
                        "ats_certification": {
                            "provider": "greenhouse",
                            "adapter_version": "2026.07.1-beta.1"
                        }
                    }
                }
            }),
            run_id: None,
            created_at_ms: 1,
            updated_at_ms: 1,
            submitted_at_ms: None,
        };
        let application_json = to_json(&application, "operational application test").unwrap();
        let mut certified_context = OperationalHoldContext::new();
        add_application_context_values(
            &mut certified_context,
            OperationalApplicationContextValues {
                account_id: "acct-operational",
                posting: &posting,
                application_json: Some(&application_json),
                runner_kind: Some("cloud"),
                model_provider: None,
                model: None,
            },
        )
        .unwrap();
        assert!(certified_context.matches(OperationalHoldScopeKind::AtsProvider, "greenhouse"));
        assert!(certified_context.matches(OperationalHoldScopeKind::AtsAdapter, "2026.07.1-beta.1"));
    }

    #[test]
    fn application_context_uses_separate_typed_workplace_evidence() {
        let mut posting = operational_posting(
            "Acme Incorporated",
            "curated_feed",
            "https://boards.greenhouse.io/acme/jobs/remote-123",
        );
        posting.location = "United States".to_string();
        posting.workplace = "remote".to_string();
        let mut remote_context = OperationalHoldContext::new();
        add_application_context_values(
            &mut remote_context,
            OperationalApplicationContextValues {
                account_id: "acct-operational",
                posting: &posting,
                application_json: None,
                runner_kind: None,
                model_provider: None,
                model: None,
            },
        )
        .unwrap();
        assert!(remote_context.matches(OperationalHoldScopeKind::Region, "country:us"));
        assert!(remote_context.matches(OperationalHoldScopeKind::Region, "workplace:remote"));

        for unsupported in ["not remote", "remote or hybrid"] {
            posting.workplace = unsupported.to_string();
            let mut context = OperationalHoldContext::new();
            add_application_context_values(
                &mut context,
                OperationalApplicationContextValues {
                    account_id: "acct-operational",
                    posting: &posting,
                    application_json: None,
                    runner_kind: None,
                    model_provider: None,
                    model: None,
                },
            )
            .unwrap();
            assert!(context.matches(OperationalHoldScopeKind::Region, "country:us"));
            assert!(!context.matches(OperationalHoldScopeKind::Region, "workplace:remote"));
            assert!(!context.matches(OperationalHoldScopeKind::Region, "workplace:hybrid"));
        }
    }

    #[test]
    fn unicode_employer_domain_hold_matches_verified_idn_context() {
        let pool = hold_pool(true);
        let mut posting = operational_posting(
            "International Employer",
            "curated_feed",
            "https://jobs.lever.co/international/job-123",
        );
        posting.discovery_evidence.canonical_employer_domain =
            Some("MU\u{308}NICH.example.".to_string());
        let mut context = OperationalHoldContext::new();
        add_application_context_values(
            &mut context,
            OperationalApplicationContextValues {
                account_id: "acct-idn-employer",
                posting: &posting,
                application_json: None,
                runner_kind: None,
                model_provider: None,
                model: None,
            },
        )
        .unwrap();
        assert!(context.matches(OperationalHoldScopeKind::EmployerDomain, "münich.example"));
        assert!(context.matches(
            OperationalHoldScopeKind::EmployerDomain,
            "xn--mnich-kva.example"
        ));

        let held = request(
            "unicode-employer-domain-held",
            OperationalCapability::Generation,
            OperationalHoldScopeKind::EmployerDomain,
            "münich.example",
            OperationalHoldTransition::Held,
            0,
            None,
        );
        append_operational_hold_event(&pool, &held, "admin-1").unwrap();
        assert!(matches!(
            require_operational_capability(&pool, OperationalCapability::Generation, &context),
            Err(OperationalHoldError::Held(_))
        ));
    }

    #[test]
    fn malformed_or_mismatched_frozen_ats_context_fails_closed() {
        let posting = operational_posting(
            "Acme Incorporated",
            "curated_feed",
            "https://jobs.lever.co/acme/job-123",
        );
        for certification in [
            serde_json::json!({ "provider": "lever" }),
            serde_json::json!({
                "provider": "greenhouse",
                "adapter_version": "2026.07.1-beta.1"
            }),
        ] {
            let application = JobApplication {
                id: "application-invalid-operational".to_string(),
                job_id: posting.id.clone(),
                resume_version_id: None,
                state: "approved".to_string(),
                submission_mode: "auto_submit".to_string(),
                match_score: 100,
                answers: Vec::new(),
                cover_letter: String::new(),
                receipt: serde_json::json!({
                    "approved_execution": {
                        "admission": { "ats_certification": certification }
                    }
                }),
                run_id: None,
                created_at_ms: 1,
                updated_at_ms: 1,
                submitted_at_ms: None,
            };
            let application_json =
                to_json(&application, "invalid operational application").unwrap();
            let mut context = OperationalHoldContext::new();
            assert!(matches!(
                add_application_context_values(
                    &mut context,
                    OperationalApplicationContextValues {
                        account_id: "acct-operational",
                        posting: &posting,
                        application_json: Some(&application_json),
                        runner_kind: None,
                        model_provider: None,
                        model: None,
                    },
                ),
                Err(OperationalHoldError::Storage(_))
            ));
        }
    }

    #[test]
    fn opaque_refs_recover_release_authority_and_preserve_exact_replay() {
        let pool = hold_pool(true);
        insert_test_account(&pool, "acct-low-entropy");
        let held = request(
            "opaque-held",
            OperationalCapability::Generation,
            OperationalHoldScopeKind::Account,
            "acct-low-entropy",
            OperationalHoldTransition::Held,
            0,
            None,
        );
        let opened = append_operational_hold_event(&pool, &held, "admin-1").unwrap();
        let release = AppendOperationalHoldEventByRefRequest {
            event_id: "opaque-released".to_string(),
            capability: OperationalCapability::Generation,
            scope_kind: OperationalHoldScopeKind::Account,
            scope_ref: opened.state.scope_ref.clone(),
            transition: OperationalHoldTransition::Released,
            reason_code: OperationalHoldReasonCode::ManualRelease,
            reason_ref: Some("INC-606".to_string()),
            expected_head_revision: opened.state.head_revision,
            expected_current_event_ref: opened.state.current_event_ref.clone(),
        };
        let released = append_operational_hold_event_by_ref(&pool, &release, "admin-1").unwrap();
        assert_eq!(released.state.state, OperationalHoldTransition::Released);
        assert!(!released.replayed);
        assert!(
            append_operational_hold_event_by_ref(&pool, &release, "admin-1")
                .unwrap()
                .replayed
        );

        let mut changed_predecessor = release;
        changed_predecessor.expected_current_event_ref = format!("event-{}", "0".repeat(64));
        assert!(matches!(
            append_operational_hold_event_by_ref(&pool, &changed_predecessor, "admin-1"),
            Err(OperationalHoldError::IdentityConflict)
        ));
    }

    #[test]
    fn operational_hold_listing_pages_every_head() {
        let pool = hold_pool(true);
        for index in 0..3 {
            insert_test_account(&pool, &format!("page-account-{index}"));
            append_operational_hold_event(
                &pool,
                &request(
                    &format!("page-event-{index}"),
                    OperationalCapability::Generation,
                    OperationalHoldScopeKind::Account,
                    &format!("page-account-{index}"),
                    OperationalHoldTransition::Held,
                    0,
                    None,
                ),
                "admin-1",
            )
            .unwrap();
        }
        let all = list_operational_hold_states(&pool, true, 10, None).unwrap();
        let first_page = list_operational_hold_states(&pool, true, 2, None).unwrap();
        let first_cursor = first_page.next_cursor.clone().unwrap();
        assert!(!first_cursor.contains("page-account"));
        let forged_plaintext = serde_json::json!({
            "purpose": "jobs_operational_hold_list_v1",
            "activeOnly": true,
            "capability": "generation",
            "scopeKind": "account",
            "scopeId": "page-account-1"
        })
        .to_string();
        assert!(matches!(
            list_operational_hold_states(&pool, true, 2, Some(&forged_plaintext)),
            Err(OperationalHoldError::InvalidRequest)
        ));
        assert!(matches!(
            list_operational_hold_states(&pool, false, 2, Some(&first_cursor)),
            Err(OperationalHoldError::InvalidRequest)
        ));
        let mut tampered_cursor = first_cursor.clone().into_bytes();
        let tamper_index = ENCRYPTED_PAYLOAD_PREFIX.len() + 4;
        tampered_cursor[tamper_index] = if tampered_cursor[tamper_index] == b'A' {
            b'B'
        } else {
            b'A'
        };
        let tampered_cursor = String::from_utf8(tampered_cursor).unwrap();
        assert!(matches!(
            list_operational_hold_states(&pool, true, 2, Some(&tampered_cursor)),
            Err(OperationalHoldError::InvalidRequest)
        ));
        let released = request(
            "page-event-0-release",
            OperationalCapability::Generation,
            OperationalHoldScopeKind::Account,
            "page-account-0",
            OperationalHoldTransition::Released,
            1,
            Some("page-event-0"),
        );
        append_operational_hold_event(&pool, &released, "admin-1").unwrap();
        let second_page =
            list_operational_hold_states(&pool, true, 2, Some(&first_cursor)).unwrap();
        assert_eq!(all.states.len(), 3);
        assert_eq!(first_page.states.len(), 2);
        assert_eq!(second_page.states.len(), 1);
        assert!(second_page.next_cursor.is_none());
        assert_eq!(
            first_page
                .states
                .into_iter()
                .chain(second_page.states)
                .collect::<Vec<_>>(),
            all.states
        );
    }

    #[test]
    fn missing_operational_hold_schema_is_a_fail_closed_storage_error() {
        let pool = hold_pool(false);
        let context = OperationalHoldContext::new();
        assert!(matches!(
            require_operational_capability(&pool, OperationalCapability::Discovery, &context),
            Err(OperationalHoldError::Storage(_))
        ));
    }

    #[test]
    fn held_head_without_exact_canonical_event_fails_closed() {
        let mut conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE jobs_operational_hold_heads (
                capability TEXT NOT NULL,
                scope_kind TEXT NOT NULL,
                scope_id TEXT NOT NULL,
                head_revision INTEGER NOT NULL,
                current_event_id TEXT NOT NULL,
                state TEXT NOT NULL,
                current_event_ref TEXT NOT NULL
             );
             CREATE TABLE jobs_operational_hold_events (
                event_id TEXT NOT NULL,
                capability TEXT NOT NULL,
                scope_kind TEXT NOT NULL,
                scope_id TEXT NOT NULL,
                revision_no INTEGER NOT NULL,
                transition TEXT NOT NULL,
                event_ref TEXT NOT NULL,
                event_sha256 TEXT NOT NULL,
                canonical_event_base64url TEXT NOT NULL,
                reason_code TEXT NOT NULL,
                reason_ref TEXT
             );
             INSERT INTO jobs_operational_hold_heads VALUES (
                'generation', 'account', 'acct-corrupt', 1, 'missing-event', 'held',
                'event-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa'
             );",
        )
        .unwrap();
        let tx = conn.transaction().unwrap();
        let context = OperationalHoldContext::new()
            .with_scope(OperationalHoldScopeKind::Account, "acct-corrupt")
            .unwrap();
        assert!(matches!(
            evaluate_operational_capability_sqlite_tx(
                &tx,
                OperationalCapability::Generation,
                &context,
            ),
            Err(OperationalHoldError::Storage(_))
        ));
    }

    #[test]
    fn every_capability_and_scope_kind_is_enforced_until_explicit_release() {
        let pool = hold_pool(true);
        for (scope_index, scope_kind) in OperationalHoldScopeKind::ALL.into_iter().enumerate() {
            let scope_id = match scope_kind {
                OperationalHoldScopeKind::Global => "*".to_string(),
                OperationalHoldScopeKind::DiscoverySource => "source-scope".to_string(),
                OperationalHoldScopeKind::AtsProvider => "greenhouse".to_string(),
                OperationalHoldScopeKind::AtsAdapter => "adapter-606".to_string(),
                OperationalHoldScopeKind::EmployerDomain => "careers.example.test".to_string(),
                OperationalHoldScopeKind::Account => "acct-scope".to_string(),
                OperationalHoldScopeKind::CareerTrack => "track-scope".to_string(),
                OperationalHoldScopeKind::Region => "New York, NY".to_string(),
                OperationalHoldScopeKind::RunnerKind => "local".to_string(),
                OperationalHoldScopeKind::MailboxProvider => "gmail".to_string(),
                OperationalHoldScopeKind::ModelProvider => "openai".to_string(),
                OperationalHoldScopeKind::Model => "gpt-5.6".to_string(),
            };
            insert_test_scope_target(&pool, scope_kind, &scope_id);
            let context = if scope_kind == OperationalHoldScopeKind::Global {
                OperationalHoldContext::new()
            } else {
                OperationalHoldContext::new()
                    .with_scope(scope_kind, &scope_id)
                    .unwrap()
            };

            let all_hold_id = format!("all-scope-{scope_index}-held");
            let all_release_id = format!("all-scope-{scope_index}-released");
            append_operational_hold_event(
                &pool,
                &request(
                    &all_hold_id,
                    OperationalCapability::All,
                    scope_kind,
                    &scope_id,
                    OperationalHoldTransition::Held,
                    0,
                    None,
                ),
                "admin-1",
            )
            .unwrap();
            for capability in OperationalCapability::CONCRETE {
                assert!(matches!(
                    require_operational_capability(&pool, capability, &context),
                    Err(OperationalHoldError::Held(_))
                ));
            }
            append_operational_hold_event(
                &pool,
                &request(
                    &all_release_id,
                    OperationalCapability::All,
                    scope_kind,
                    &scope_id,
                    OperationalHoldTransition::Released,
                    1,
                    Some(&all_hold_id),
                ),
                "admin-1",
            )
            .unwrap();

            for (capability_index, capability) in
                OperationalCapability::CONCRETE.into_iter().enumerate()
            {
                let hold_id = format!("cap-{capability_index}-scope-{scope_index}-held");
                let release_id = format!("cap-{capability_index}-scope-{scope_index}-released");
                append_operational_hold_event(
                    &pool,
                    &request(
                        &hold_id,
                        capability,
                        scope_kind,
                        &scope_id,
                        OperationalHoldTransition::Held,
                        0,
                        None,
                    ),
                    "admin-1",
                )
                .unwrap();
                assert!(matches!(
                    require_operational_capability(&pool, capability, &context),
                    Err(OperationalHoldError::Held(_))
                ));
                append_operational_hold_event(
                    &pool,
                    &request(
                        &release_id,
                        capability,
                        scope_kind,
                        &scope_id,
                        OperationalHoldTransition::Released,
                        1,
                        Some(&hold_id),
                    ),
                    "admin-1",
                )
                .unwrap();
                assert!(
                    require_operational_capability(&pool, capability, &context).is_ok(),
                    "released {capability:?}/{scope_kind:?} remained held"
                );
            }
        }
    }

    #[test]
    fn unicode_region_scope_normalizes_and_matches_context() {
        let pool = hold_pool(true);
        let held = request(
            "unicode-region-held",
            OperationalCapability::Discovery,
            OperationalHoldScopeKind::Region,
            "MU\u{308}NCHEN",
            OperationalHoldTransition::Held,
            0,
            None,
        );
        append_operational_hold_event(&pool, &held, "admin-1").unwrap();
        let context = OperationalHoldContext::new()
            .with_scope(OperationalHoldScopeKind::Region, "münchen")
            .unwrap()
            .with_scope(OperationalHoldScopeKind::Region, "São Paulo")
            .unwrap()
            .with_scope(OperationalHoldScopeKind::Region, "東京")
            .unwrap();
        assert!(matches!(
            require_operational_capability(&pool, OperationalCapability::Discovery, &context),
            Err(OperationalHoldError::Held(_))
        ));

        let released = request(
            "unicode-region-released",
            OperationalCapability::Discovery,
            OperationalHoldScopeKind::Region,
            "münchen",
            OperationalHoldTransition::Released,
            1,
            Some("unicode-region-held"),
        );
        append_operational_hold_event(&pool, &released, "admin-1").unwrap();
        assert!(
            require_operational_capability(&pool, OperationalCapability::Discovery, &context)
                .is_ok()
        );
    }
}
